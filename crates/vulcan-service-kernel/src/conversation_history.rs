//! Conversation history loading and lazy hydration routes.
//! 会话历史加载与懒水合路由。

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::{Arc, Mutex, OnceLock, Weak},
};

use super::session_hydration::{
    PreservedWorkspaceRunState, capture_preserved_workspace_run_state_for_session_hydration,
};
use serde_json::{Value, json};
use vulcan_context::SessionConversationRecord;
use vulcan_service_api::{
    ConversationHistoryPageResponse, ConversationHydrationChangedDelta,
    ConversationHydrationProgress, ConversationHydrationStatus, ConversationMessage,
    ConversationMessagesLoadResponse, HydratedConversationWindowSummary, ServiceEvent,
    ServiceEventEmitter, WorkbenchDelta, WorkbenchDeltaEnvelope, WorkbenchSnapshot,
};
use vulcan_workbench_model::{SessionId, SessionMessageEntry, SessionMessageRecordKind};
use vulcan_workspace_store::workspace_store::{SessionHistoryPage, WorkspaceSnapshot};

use super::{
    AgentRuntimeBridge, ContextBudgetSnapshot, ControllerRuntimeHandle, ProviderConfigurationFile,
    SELECTED_SESSION_TURN_LOAD_LIMIT, SharedServiceBridgeState, TauriLuaSkillsCatalog,
    TauriMcpRuntimeCatalog, WorkbenchRuntimeProjection, active_kernel_session_ids,
    active_surface_messages_from_workspace_and_state,
    apply_hydration_to_selected_workspace_snapshot,
    apply_webui_unclosed_subagent_surface_preload_once, cached_luaskills_catalog_from_state,
    cached_mcp_catalog_from_state, conversation_messages_from_session, current_unix_millis_value,
    emit_service_event_with_trace, emit_workbench_event, load_selected_session_display_hydration,
    load_webui_unclosed_subagent_surface_preload_once, native_tool_summaries_from_agent_runtime,
    pending_question_snapshots_from_agent_runtime, preserve_active_session_run_state,
    require_segmented_session_directory_for_retained_session,
    restore_webui_subagent_registry_for_session_hydration, selected_session_id_after_refresh,
    session_summary_project_and_created_at, subagent_navigation_snapshot_from_cached_state,
    sync_selected_session_goal_summary_from_store, webui_trace_if_slow, workbench_from_runtime,
    workbench_surface_matches_conversation, workspace_session_summary_for_session,
    workspace_snapshot_contains_regular_session, workspace_surface_allows_user_prompt_editing,
};

mod tool_detail;

pub(super) use tool_detail::*;

/// Process-local exact-key registry for in-progress display hydration locks.
/// 用于显示态水合在途锁的进程内精确键注册表。
static CONVERSATION_HYDRATION_SINGLEFLIGHT_LOCKS: OnceLock<
    Mutex<BTreeMap<SessionId, Weak<tokio::sync::Mutex<()>>>>,
> = OnceLock::new();

/// Owns one acquired display-hydration singleflight lock and performs exact-key cleanup on drop.
/// 持有一个已经取得的显示态水合 singleflight 锁，并在释放时执行精确键清理。
struct ConversationHydrationSingleflightGuard {
    /// Exact session identity owning this singleflight lock.
    /// 持有当前 singleflight 锁的精确会话身份。
    session_id: SessionId,
    /// Shared lock identity retained for pointer-safe registry cleanup.
    /// 为指针安全的注册表清理保留的共享锁身份。
    lock: Arc<tokio::sync::Mutex<()>>,
    /// Owned asynchronous guard released before the exact registry entry is inspected.
    /// 在检查精确注册表项之前释放的异步自有守卫。
    guard: Option<tokio::sync::OwnedMutexGuard<()>>,
}

impl ConversationHydrationSingleflightGuard {
    /// Acquires the exact per-session hydration lock without enumerating other sessions.
    /// 在不枚举其他会话的前提下取得精确的会话水合锁。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the one display hydration request group to serialize.
    /// `session_id` 标识需要串行化的唯一显示态水合请求组。
    ///
    /// # Returns
    /// 返回值
    /// An owned guard shared by all concurrent requests for the same session.
    /// 返回由同一会话全部并发请求共享的自有守卫。
    async fn acquire(session_id: SessionId) -> Result<Self, String> {
        // registry is queried by the exact session key; no cleanup scan is permitted here.
        // registry 仅按精确会话键查询；此处不允许清理扫描。
        let registry =
            CONVERSATION_HYDRATION_SINGLEFLIGHT_LOCKS.get_or_init(|| Mutex::new(BTreeMap::new()));
        // lock is reused when the exact entry still has an active owner or waiter.
        // lock 在精确表项仍有活动所有者或等待者时复用。
        let lock = {
            let mut entries = registry
                .lock()
                .map_err(|error| format!("conversation hydration lock registry failed: {error}"))?;
            match entries.get(&session_id).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    // new_lock replaces only the requested stale or missing entry.
                    // new_lock 仅替换当前请求对应的过期或缺失表项。
                    let new_lock = Arc::new(tokio::sync::Mutex::new(()));
                    entries.insert(session_id, Arc::downgrade(&new_lock));
                    new_lock
                }
            }
        };
        // guard serializes the actual cold display load for this exact session.
        // guard 串行化当前精确会话的真实冷显示加载。
        let guard = lock.clone().lock_owned().await;
        Ok(Self {
            session_id,
            lock,
            guard: Some(guard),
        })
    }
}

impl Drop for ConversationHydrationSingleflightGuard {
    /// Releases the owned lock and removes only an unused matching weak entry.
    /// 释放自有锁，并且只移除未被使用且身份匹配的弱引用表项。
    fn drop(&mut self) {
        // guard must be released before strong-count inspection so queued owners remain visible.
        // 必须先释放 guard 再检查强引用计数，使排队所有者保持可见。
        self.guard.take();
        let Some(registry) = CONVERSATION_HYDRATION_SINGLEFLIGHT_LOCKS.get() else {
            return;
        };
        let Ok(mut entries) = registry.lock() else {
            return;
        };
        // removable requires this wrapper to be the sole remaining strong owner of the exact lock.
        // removable 要求当前包装器是该精确锁唯一剩余的强所有者。
        let removable = Arc::strong_count(&self.lock) == 1
            && entries
                .get(&self.session_id)
                .is_some_and(|entry| Weak::ptr_eq(entry, &Arc::downgrade(&self.lock)));
        if removable {
            entries.remove(&self.session_id);
        }
    }
}

/// Lightweight storage context captured before one display hydration pass.
/// 单次显示水合前捕获的轻量存储上下文。
struct HydratedSessionReadContext {
    /// Controller runtime used for store reads and persisted goal lookups.
    /// 用于读取存储与持久化目标的控制器运行时。
    controller_runtime: ControllerRuntimeHandle,
    /// Minimal run-state overlay captured before any index refresh.
    /// 在任何索引刷新前捕获的最小运行态覆盖数据。
    preserved_workspace_run_state: Option<PreservedWorkspaceRunState>,
}

/// Runtime projection context captured after the selected transcript window is loaded.
/// 当前会话转录窗口载入后捕获的运行时投影上下文。
struct HydratedSessionProjectionContext {
    /// Cached LuaSkills catalog projected into the rebuilt workbench.
    /// 投影进重建工作台的 LuaSkills 缓存目录。
    luaskills_catalog: TauriLuaSkillsCatalog,
    /// Cached MCP catalog projected into the rebuilt workbench.
    /// 投影进重建工作台的 MCP 缓存目录。
    mcp_catalog: TauriMcpRuntimeCatalog,
    /// Recent service logs included in the workbench snapshot.
    /// 包含在工作台快照中的最近服务日志。
    logs: Vec<String>,
    /// Provider configuration used to render model and route choices.
    /// 用于渲染模型与路由选项的 Provider 配置。
    provider_configuration: ProviderConfigurationFile,
    /// Latest context-budget snapshots keyed by session id.
    /// 按会话标识索引的最近上下文预算快照。
    live_context_budgets: BTreeMap<SessionId, ContextBudgetSnapshot>,
    /// Optional agent runtime bridge used for live status projections.
    /// 用于实时状态投影的可选 Agent 运行时桥。
    agent_runtime_for_status: Option<AgentRuntimeBridge>,
    /// Detached subagent task ids that still have live listeners.
    /// 仍然存在实时监听器的分离子代理任务标识集合。
    live_listener_task_ids: BTreeSet<SessionId>,
}

/// Hydrated workbench plus transcript pagination cursors for one selected session.
/// 单个选中会话的已水合工作台与转录分页游标。
pub(super) struct HydratedSessionWorkbench {
    /// Workbench snapshot rebuilt with the loaded conversation window.
    /// 使用已加载会话窗口重建出的工作台快照。
    pub(super) workbench: WorkbenchSnapshot,
    /// Earliest loaded transcript ordinal used to page older turns.
    /// 用于继续分页加载更早轮次的已加载最早转录序号。
    earliest_ordinal: Option<i64>,
    /// Whether older transcript turns exist before the current window.
    /// 当前窗口之前是否存在更早的转录轮次。
    has_more_history: bool,
}

/// Retained pagination state for one hydrated WebUI conversation.
/// 单个已水合 WebUI 会话的保留分页状态。
#[derive(Debug, Clone, Copy)]
pub(super) struct HydratedConversationWindowState {
    /// Whether the exact transcript window and pagination facts were loaded from storage.
    /// 是否已经从存储加载精确转录窗口与分页事实。
    pub(super) transcript_window_loaded: bool,
    /// Absolute persisted message index owned by the first raw row in the retained window.
    /// 保留窗口首条原始行对应的绝对持久化消息下标。
    pub(super) message_start_index: Option<usize>,
    /// Earliest loaded transcript ordinal used to page older turns.
    /// 用于继续分页加载更早轮次的已加载最早转录序号。
    pub(super) earliest_ordinal: Option<i64>,
    /// Whether older transcript turns exist before the current window.
    /// 当前窗口之前是否存在更早的转录轮次。
    pub(super) has_more_history: bool,
}

/// Converts retained window states into the WebUI API summary shape.
/// 将保留窗口状态转换为 WebUI API 摘要结构。
pub(super) fn hydrated_conversation_window_summaries(
    windows: &BTreeMap<SessionId, HydratedConversationWindowState>,
    preloaded_subagent_roots: &BTreeSet<SessionId>,
) -> Vec<HydratedConversationWindowSummary> {
    windows
        .iter()
        .map(|(session_id, window)| HydratedConversationWindowSummary {
            session_id: session_id.to_string(),
            subagent_metadata_loaded: preloaded_subagent_roots.contains(session_id),
            earliest_ordinal: window.earliest_ordinal,
            has_more_history: window.has_more_history,
        })
        .collect()
}

/// Synchronizes retained lazy window entries with workspace-hydrated session ids.
/// 将保留懒加载窗口记录与工作区已水合会话标识同步。
pub(super) fn retain_hydrated_conversation_windows_for_workspace(
    windows: &mut BTreeMap<SessionId, HydratedConversationWindowState>,
    workspace: &WorkspaceSnapshot,
) {
    windows.retain(|session_id, _| workspace.hydrated_session_ids.contains(session_id));
    for session_id in &workspace.hydrated_session_ids {
        windows
            .entry(*session_id)
            .or_insert(HydratedConversationWindowState {
                transcript_window_loaded: false,
                message_start_index: None,
                earliest_ordinal: None,
                has_more_history: false,
            });
    }
}

/// Stores the pagination state for one hydrated WebUI conversation window.
/// 保存单个已水合 WebUI 会话窗口的分页状态。
pub(super) fn remember_hydrated_conversation_window(
    windows: &mut BTreeMap<SessionId, HydratedConversationWindowState>,
    session_id: SessionId,
    message_start_index: Option<usize>,
    earliest_ordinal: Option<i64>,
    has_more_history: bool,
) {
    windows.insert(
        session_id,
        HydratedConversationWindowState {
            transcript_window_loaded: true,
            message_start_index,
            earliest_ordinal,
            has_more_history,
        },
    );
}

/// Loads the recent transcript window for one conversation.
/// 加载单个会话的最近转录窗口。
pub(super) async fn service_session_messages(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    session_id: String,
) -> Result<ConversationMessagesLoadResponse, String> {
    let parsed_session_id = session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{session_id}': {error}"))?;
    if let Some(response) =
        retained_hydrated_session_messages_response(&app, &state, parsed_session_id, &session_id)?
    {
        return Ok(response);
    }
    // hydration_guard merges concurrent cold loads for only this exact session.
    // hydration_guard 仅合并当前精确会话的并发冷加载。
    let _hydration_guard =
        ConversationHydrationSingleflightGuard::acquire(parsed_session_id).await?;
    if let Some(response) =
        retained_hydrated_session_messages_response(&app, &state, parsed_session_id, &session_id)?
    {
        return Ok(response);
    }
    emit_conversation_hydration_progress(
        &app,
        &state,
        conversation_hydration_progress(
            &session_id,
            ConversationHydrationStatus::Loading,
            "locating",
            5,
            "正在定位会话。",
        ),
    )?;
    let hydrated = load_hydrated_session_workbench(&app, &state, parsed_session_id, &session_id)
        .await
        .inspect_err(|error| {
            let _ = emit_conversation_hydration_progress(
                &app,
                &state,
                conversation_hydration_progress(
                    &session_id,
                    ConversationHydrationStatus::Error,
                    "failed",
                    100,
                    error,
                ),
            );
        })?;
    let progress = conversation_hydration_progress(
        &session_id,
        ConversationHydrationStatus::Ready,
        "done",
        100,
        "会话载入完成。",
    );
    let mut completed_workbench = hydrated.workbench;
    completed_workbench.conversation_hydration = Some(progress.clone());
    let messages = completed_workbench.messages.clone();
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        completed_workbench.revision = guard.workbench_generation;
        guard.workbench = completed_workbench.clone();
    }
    emit_workbench_event(&app, &completed_workbench);
    Ok(ConversationMessagesLoadResponse {
        workbench: Box::new(completed_workbench),
        messages,
        earliest_ordinal: hydrated.earliest_ordinal,
        has_more_history: hydrated.has_more_history,
        progress,
    })
}

/// Returns a retained hydrated conversation window without re-reading persisted session files.
/// 返回已保留的水合会话窗口，避免重新读取持久化会话文件。
fn retained_hydrated_session_messages_response(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    parsed_session_id: SessionId,
    session_id: &str,
) -> Result<Option<ConversationMessagesLoadResponse>, String> {
    let (session_messages, allow_user_prompt_editing, retained_window) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
            return Ok(None);
        };
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, parsed_session_id) {
            return Ok(None);
        }
        if !workspace_snapshot
            .hydrated_session_ids
            .contains(&parsed_session_id)
        {
            return Ok(None);
        }
        if !guard
            .webui_preloaded_unclosed_subagent_roots
            .contains(&parsed_session_id)
        {
            return Ok(None);
        }
        let Some(session_messages) = workspace_snapshot
            .session_messages
            .get(&parsed_session_id)
            .cloned()
        else {
            return Ok(None);
        };
        let allow_user_prompt_editing = workspace_surface_allows_user_prompt_editing(
            workspace_snapshot,
            parsed_session_id,
            None,
        );
        let Some(retained_window) = guard
            .hydrated_conversation_windows
            .get(&parsed_session_id)
            .copied()
            .filter(|window| window.transcript_window_loaded)
        else {
            return Ok(None);
        };
        (session_messages, allow_user_prompt_editing, retained_window)
    };
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let window = *guard
        .hydrated_conversation_windows
        .entry(parsed_session_id)
        .or_insert(retained_window);
    guard.active_subagent_panel = None;
    let messages = conversation_messages_from_session(
        session_id,
        &session_messages,
        allow_user_prompt_editing,
    );
    let subagent_navigation = if let Some(mut workspace_snapshot) = guard.workspace_snapshot.take()
    {
        workspace_snapshot.selected_session = parsed_session_id;
        let subagent_navigation =
            subagent_navigation_snapshot_from_cached_state(&mut guard, &workspace_snapshot)?;
        guard.workspace_snapshot = Some(workspace_snapshot);
        subagent_navigation
    } else {
        None
    };
    let progress = conversation_hydration_progress(
        session_id,
        ConversationHydrationStatus::Ready,
        "done",
        100,
        "会话载入完成。",
    );
    guard.workbench.selected_conversation_id = Some(session_id.to_string());
    guard.workbench.messages = messages.clone();
    guard.workbench.hydrated_conversation_windows = hydrated_conversation_window_summaries(
        &guard.hydrated_conversation_windows,
        &guard.webui_preloaded_unclosed_subagent_roots,
    );
    guard.workbench.conversation_hydration = Some(progress.clone());
    guard.workbench.subagent_navigation = subagent_navigation;
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    let workbench = guard.workbench.clone();
    drop(guard);
    emit_workbench_event(app, &workbench);
    Ok(Some(ConversationMessagesLoadResponse {
        workbench: Box::new(workbench),
        messages,
        earliest_ordinal: window.earliest_ordinal,
        has_more_history: window.has_more_history,
        progress,
    }))
}

/// Loads one older transcript page and merges it into the retained conversation window.
/// 加载一页更早转录历史，并将其合并进保留的会话窗口。
///
/// # Parameters
/// 参数
/// `app` emits the resulting workbench event after the page is retained.
/// `app` 在分页进入保留窗口后发出工作台事件。
/// `state` owns the controller runtime, workspace index, and retained message windows.
/// `state` 持有控制器运行时、工作区索引与保留消息窗口。
/// `session_id` identifies the exact conversation to page.
/// `session_id` 标识需要分页的精确会话。
/// `before_ordinal` is the exclusive transcript boundary for the older page.
/// `before_ordinal` 是更早分页使用的排他转录边界。
///
/// # Returns
/// 返回值
/// The conversation-scoped page response, or an explicit location/read failure.
/// 返回会话范围分页响应，或明确的定位与读取失败。
pub(super) async fn service_session_history_page(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    session_id: String,
    before_ordinal: i64,
) -> Result<ConversationHistoryPageResponse, String> {
    // started_at measures the complete page request without logging transcript content.
    // started_at 在不记录转录正文的前提下测量完整分页请求。
    let started_at = std::time::Instant::now();
    // parsed_session_id is the validated storage identity used by every downstream lookup.
    // parsed_session_id 是所有后续查询使用的已验证存储标识。
    let parsed_session_id = session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{session_id}': {error}"))?;
    // controller_runtime and session_location are captured together from one retained workspace boundary.
    // controller_runtime 与 session_location 从同一个保留工作区边界一起捕获。
    let (controller_runtime, session_location) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let session_location = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace_snapshot| {
                workspace_session_summary_for_session(workspace_snapshot, parsed_session_id)
            })
            .and_then(session_summary_project_and_created_at)
            .map(|(project_id, created_at)| (project_id.to_string(), created_at.to_string()));
        (controller_runtime, session_location)
    };
    // project_id and created_at form the deterministic session directory locator without any scan fallback.
    // project_id 与 created_at 组成不含任何扫描回退的确定性会话目录定位器。
    let (project_id, created_at) = session_location.ok_or_else(|| {
        eprintln!(
            "[webui-service][conversation-history-page] status=location_failed session={} before_ordinal={} elapsed_ms={}",
            session_id,
            before_ordinal,
            started_at.elapsed().as_millis(),
        );
        format!(
            "conversation history page cannot locate session `{session_id}` from the retained workspace index"
        )
    })?;
    // page is the exact older segmented-transcript window returned by storage.
    // page 是存储返回的精确更早分段转录窗口。
    let page =
        vulcan_workspace_store::workspace_store::load_session_history_page_for_project_created_at(
            controller_runtime,
            &project_id,
            &created_at,
            parsed_session_id,
            before_ordinal,
            SELECTED_SESSION_TURN_LOAD_LIMIT,
        )
        .await
        .map_err(|error| {
            eprintln!(
                "[webui-service][conversation-history-page] status=read_failed session={} before_ordinal={} elapsed_ms={} error={}",
                session_id,
                before_ordinal,
                started_at.elapsed().as_millis(),
                error,
            );
            error
        })?;
    // page_entry_count records visible/storage boundary entries before projection.
    // page_entry_count 记录投影前的可见及存储边界条目数量。
    let page_entry_count = page.messages.len();
    // page_tool_detail_count records detached tool-detail entries retained with this page.
    // page_tool_detail_count 记录随该分页保留的独立工具详情条目数量。
    let page_tool_detail_count = page.tool_detail_messages.len();
    // The merged tuple preserves exact counts across the short service-state lock boundary.
    // 合并元组跨越短服务状态锁边界保留精确计数。
    let (workbench, response_workbench, page_messages, retained_before, retained_after, added) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() else {
            return Err(String::from("service workspace snapshot is not ready"));
        };
        let (response_messages, page_messages, retained_before, retained_after, added) = {
            let allow_user_prompt_editing = workspace_surface_allows_user_prompt_editing(
                workspace_snapshot,
                parsed_session_id,
                None,
            );
            let retained_messages = workspace_snapshot
                .session_messages
                .entry(parsed_session_id)
                .or_default();
            // retained_before is the in-memory entry count before this page.
            // retained_before 是应用本分页前的内存条目数量。
            let retained_before = retained_messages.len();
            // added is the exact number of stable entries newly prepended.
            // added 是实际新前置的稳定条目数量。
            let added = prepend_unique_older_session_history_page(retained_messages, &page);
            // retained_after is the in-memory entry count after deterministic deduplication.
            // retained_after 是确定性去重后的内存条目数量。
            let retained_after = retained_messages.len();
            (
                conversation_messages_from_session(
                    &session_id,
                    retained_messages,
                    allow_user_prompt_editing,
                ),
                conversation_messages_from_session(&session_id, &page.messages, false),
                retained_before,
                retained_after,
                added,
            )
        };
        remember_hydrated_conversation_window(
            &mut guard.hydrated_conversation_windows,
            parsed_session_id,
            page.message_start_index,
            page.earliest_ordinal,
            page.has_more_history,
        );
        guard.workbench.hydrated_conversation_windows = hydrated_conversation_window_summaries(
            &guard.hydrated_conversation_windows,
            &guard.webui_preloaded_unclosed_subagent_roots,
        );
        if workbench_surface_matches_conversation(&guard.workbench, session_id.as_str()) {
            guard.workbench.messages = response_messages.clone();
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        let workbench = guard.workbench.clone();
        let response_workbench =
            focused_history_page_response_workbench(&workbench, response_messages);
        (
            workbench,
            response_workbench,
            page_messages,
            retained_before,
            retained_after,
            added,
        )
    };
    emit_workbench_event(&app, &workbench);
    eprintln!(
        "[webui-service][conversation-history-page] status=completed session={} before_ordinal={} page_entries={} page_tool_details={} projected_page_messages={} retained_before={} retained_after={} added={} earliest_ordinal={:?} has_more_history={} elapsed_ms={}",
        session_id,
        before_ordinal,
        page_entry_count,
        page_tool_detail_count,
        page_messages.len(),
        retained_before,
        retained_after,
        added,
        page.earliest_ordinal,
        page.has_more_history,
        started_at.elapsed().as_millis(),
    );
    Ok(ConversationHistoryPageResponse {
        session_id,
        workbench: Box::new(response_workbench),
        messages: page_messages,
        earliest_ordinal: page.earliest_ordinal,
        has_more_history: page.has_more_history,
    })
}

/// Prepends an older history page into the retained session message window without duplicating rows.
/// 将更早历史页前置合并到保留的会话消息窗口中，并避免重复行。
/// Prepends older page messages into the retained hydrated window without duplicating stable rows.
/// 将较早分页消息前置合并进已保留的水合窗口，并避免重复稳定行。
///
/// # Parameters
/// 参数
/// `retained_messages` is the current in-memory conversation window.
/// `retained_messages` 是当前内存会话窗口。
/// `page` contains the exact older visible and tool-detail entries.
/// `page` 包含精确的更早可见条目与工具详情条目。
///
/// # Returns
/// 返回值
/// The number of previously unseen stable entries prepended by this call.
/// 返回本次调用前置的此前未出现稳定条目数量。
fn prepend_unique_older_session_history_page(
    retained_messages: &mut Vec<SessionMessageEntry>,
    page: &SessionHistoryPage,
) -> usize {
    // retained_before is the baseline used to report exact additions.
    // retained_before 是用于报告精确新增数的基线。
    let retained_before = retained_messages.len();
    // seen_keys gives retained rows precedence over duplicate rows in older storage pages.
    // seen_keys 使保留行优先于更早存储分页中的重复行。
    let mut seen_keys = retained_messages
        .iter()
        .map(SessionMessageEntry::stable_merge_key)
        .collect::<HashSet<_>>();
    // merged retains the page order for unseen entries before appending the current window.
    // merged 为未见条目保留分页顺序，然后追加当前窗口。
    let mut merged = page
        .messages
        .iter()
        .chain(page.tool_detail_messages.iter())
        .filter(|message| seen_keys.insert(message.stable_merge_key()))
        .cloned()
        .collect::<Vec<_>>();
    merged.append(retained_messages);
    *retained_messages = merged;
    retained_messages.len().saturating_sub(retained_before)
}

/// Captures storage inputs needed before reading one selected-session transcript window.
/// 捕获读取单个选中会话转录窗口前需要的存储输入。
///
/// # Parameters
/// 参数
/// `state` owns the service runtime handles and in-memory run-state overlay.
/// `state` 持有服务运行时句柄与内存运行态覆盖数据。
/// `session_id` identifies the session being hydrated and is used only for diagnostics.
/// `session_id` 标识正在水合的会话，仅用于诊断日志。
///
/// # Returns
/// 返回值
/// A lightweight context that avoids cloning final workbench projection data.
/// 返回避免克隆最终工作台投影数据的轻量上下文。
fn capture_hydrated_session_read_context(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<HydratedSessionReadContext, String> {
    let capture_started_at = current_unix_millis_value();
    let context = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state =
            capture_preserved_workspace_run_state_for_session_hydration(
                guard.workspace_snapshot.as_ref(),
                &active_kernel_sessions,
                session_id,
            );
        HydratedSessionReadContext {
            controller_runtime,
            preserved_workspace_run_state,
        }
    };
    webui_trace_if_slow(
        "session_hydration_read_context_capture_slow",
        capture_started_at,
        format!("session={session_id}"),
    );
    Ok(context)
}

/// Captures runtime inputs needed only when rebuilding the WebUI workbench projection.
/// 捕获仅在重建 WebUI 工作台投影时才需要的运行时输入。
///
/// # Parameters
/// 参数
/// `state` owns cached tool catalogs, runtime summaries, and UI logs.
/// `state` 持有缓存工具目录、运行时摘要与界面日志。
/// `session_id` identifies the session being hydrated and is used only for diagnostics.
/// `session_id` 标识正在水合的会话，仅用于诊断日志。
///
/// # Returns
/// 返回值
/// A projection context intentionally captured after transcript IO is complete.
/// 返回一个刻意在转录 IO 完成之后捕获的投影上下文。
fn capture_hydrated_session_projection_context(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<HydratedSessionProjectionContext, String> {
    let capture_started_at = current_unix_millis_value();
    let context = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        HydratedSessionProjectionContext {
            luaskills_catalog: cached_luaskills_catalog_from_state(&guard),
            mcp_catalog: cached_mcp_catalog_from_state(&guard),
            logs: guard.startup.recent_logs.clone(),
            provider_configuration,
            live_context_budgets: guard.live_context_budgets.clone(),
            agent_runtime_for_status: guard.agent_runtime.clone(),
            live_listener_task_ids: guard
                .detached_subagent_task_listeners
                .keys()
                .copied()
                .collect(),
        }
    };
    webui_trace_if_slow(
        "session_hydration_projection_context_capture_slow",
        capture_started_at,
        format!("session={session_id}"),
    );
    Ok(context)
}

/// Creates a lightweight workspace index snapshot without retained transcript payloads.
/// 创建不携带保留转录载荷的轻量工作区索引快照。
///
/// # Parameters
/// 参数
/// `retained_snapshot` is the in-memory workspace snapshot that owns the current sidebar index.
/// `retained_snapshot` 是持有当前侧栏索引的内存工作区快照。
///
/// # Returns
/// 返回值
/// A workspace snapshot that keeps project/session identity but drops transcript-sized maps.
/// 返回保留项目与会话身份、但移除转录级大 map 的工作区快照。
pub(crate) fn lightweight_workspace_index_snapshot_for_hydration(
    retained_snapshot: &WorkspaceSnapshot,
) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        task_projects: retained_snapshot.task_projects.clone(),
        archived_task_projects: retained_snapshot.archived_task_projects.clone(),
        sessions: retained_snapshot.sessions.clone(),
        archived_sessions: retained_snapshot.archived_sessions.clone(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: retained_snapshot.session_run_phases.clone(),
        expanded_task_projects: retained_snapshot.expanded_task_projects.clone(),
        selected_session: retained_snapshot.selected_session,
        hydrated_session_ids: BTreeSet::new(),
    }
}

/// Restores retained hydrated payloads that are still valid after a workspace index refresh.
/// 在工作区索引刷新后恢复仍然有效的已水合载荷。
///
/// # Parameters
/// 参数
/// `refreshed_snapshot` is the workspace snapshot that will be written back to service state.
/// `refreshed_snapshot` 是即将写回服务状态的工作区快照。
/// `retained_snapshot` is the current in-memory workspace snapshot before this hydration pass.
/// `retained_snapshot` 是本次水合前当前保留在内存中的工作区快照。
/// `hydrating_session_id` identifies the session whose freshly loaded payload must win.
/// `hydrating_session_id` 标识本次新水合载荷必须优先的会话。
pub(crate) fn preserve_retained_hydrated_payloads_after_index_refresh(
    refreshed_snapshot: &mut WorkspaceSnapshot,
    retained_snapshot: &WorkspaceSnapshot,
    hydrating_session_id: SessionId,
) {
    for session_id in retained_snapshot.hydrated_session_ids.iter().copied() {
        if session_id == hydrating_session_id {
            continue;
        }
        if !workspace_snapshot_contains_regular_session(refreshed_snapshot, session_id) {
            continue;
        }
        preserve_retained_hydrated_session_payloads(
            refreshed_snapshot,
            retained_snapshot,
            session_id,
        );
    }
}

/// Restores retained payloads for one already hydrated session after an index refresh.
/// 在索引刷新后为单个已水合会话恢复保留载荷。
///
/// # Parameters
/// 参数
/// `refreshed_snapshot` is the workspace snapshot receiving retained payloads.
/// `refreshed_snapshot` 是接收保留载荷的工作区快照。
/// `retained_snapshot` is the previous in-memory workspace snapshot.
/// `retained_snapshot` 是之前的内存工作区快照。
/// `session_id` identifies the retained hydrated session to restore.
/// `session_id` 标识需要恢复的已水合保留会话。
fn preserve_retained_hydrated_session_payloads(
    refreshed_snapshot: &mut WorkspaceSnapshot,
    retained_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) {
    preserve_retained_hydrated_payload(
        &mut refreshed_snapshot.session_messages,
        &retained_snapshot.session_messages,
        session_id,
    );
    preserve_retained_hydrated_payload(
        &mut refreshed_snapshot.session_runtime_state,
        &retained_snapshot.session_runtime_state,
        session_id,
    );
    preserve_retained_hydrated_payload(
        &mut refreshed_snapshot.session_compactions,
        &retained_snapshot.session_compactions,
        session_id,
    );
    if refreshed_snapshot
        .session_messages
        .contains_key(&session_id)
    {
        refreshed_snapshot.hydrated_session_ids.insert(session_id);
    }
}

/// Copies one retained session payload into a refreshed payload map only when it is missing.
/// 仅在刷新后的载荷表缺失时复制单个保留会话载荷。
///
/// # Parameters
/// 参数
/// `refreshed_payloads` is the refreshed payload map that should keep existing entries.
/// `refreshed_payloads` 是应保留已有条目的刷新后载荷表。
/// `retained_payloads` is the previous payload map used as the fallback source.
/// `retained_payloads` 是作为回退来源的旧载荷表。
/// `session_id` identifies the session payload to restore.
/// `session_id` 标识需要恢复的会话载荷。
fn preserve_retained_hydrated_payload<T: Clone>(
    refreshed_payloads: &mut BTreeMap<SessionId, T>,
    retained_payloads: &BTreeMap<SessionId, T>,
    session_id: SessionId,
) {
    if let Some(payload) = retained_payloads.get(&session_id) {
        refreshed_payloads
            .entry(session_id)
            .or_insert_with(|| payload.clone());
    }
}

/// Loads one conversation using the original selected-session workbench projection path.
/// 使用原始选中会话工作台投影路径加载单个会话。
pub(super) async fn load_hydrated_session_workbench(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    parsed_session_id: SessionId,
    session_id: &str,
) -> Result<HydratedSessionWorkbench, String> {
    let read_context = capture_hydrated_session_read_context(state, parsed_session_id)?;
    emit_conversation_hydration_progress(
        app,
        state,
        conversation_hydration_progress(
            session_id,
            ConversationHydrationStatus::Loading,
            "readingIndex",
            20,
            "正在读取消息索引。",
        ),
    )?;
    let retained_snapshot =
        retained_workspace_snapshot_for_session_hydration_from_state(state, parsed_session_id)?;
    let mut refreshed_snapshot = if let Some(snapshot) = retained_snapshot {
        snapshot
    } else {
        let index_load_started_at = current_unix_millis_value();
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            read_context.controller_runtime.clone(),
        )
        .await
        .inspect(|_| {
            webui_trace_if_slow(
                "session_hydration_workspace_index_load_slow",
                index_load_started_at,
                format!("session={parsed_session_id}"),
            );
        })?
    };
    preserve_active_session_run_state(
        read_context.preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );
    refreshed_snapshot.selected_session = parsed_session_id;
    emit_conversation_hydration_progress(
        app,
        state,
        conversation_hydration_progress(
            session_id,
            ConversationHydrationStatus::Loading,
            "readingMessages",
            55,
            "正在读取消息内容。",
        ),
    )?;
    let display_hydration_started_at = current_unix_millis_value();
    let hydration = load_selected_session_display_hydration(
        read_context.controller_runtime.clone(),
        &refreshed_snapshot,
        parsed_session_id,
    )
    .await
    .inspect(|_| {
        webui_trace_if_slow(
            "session_hydration_display_window_load_slow",
            display_hydration_started_at,
            format!("session={parsed_session_id}"),
        );
    })?;
    let earliest_ordinal = hydration.earliest_ordinal;
    let message_start_index = hydration.message_start_index;
    let has_more_history = hydration.has_more_history;
    emit_conversation_hydration_progress(
        app,
        state,
        conversation_hydration_progress(
            session_id,
            ConversationHydrationStatus::Loading,
            "projecting",
            85,
            "正在整理显示数据。",
        ),
    )?;
    let projection_context = capture_hydrated_session_projection_context(state, parsed_session_id)?;
    apply_hydration_to_selected_workspace_snapshot(
        &read_context.controller_runtime,
        &mut refreshed_snapshot,
        parsed_session_id,
        hydration,
        projection_context
            .agent_runtime_for_status
            .as_ref()
            .map(|runtime| &runtime.task_runtime),
        &projection_context.live_listener_task_ids,
    )?;
    sync_selected_session_goal_summary_from_store(
        state,
        read_context.controller_runtime.clone(),
        parsed_session_id,
    )
    .await?;
    emit_conversation_hydration_progress(
        app,
        state,
        conversation_hydration_progress(
            session_id,
            ConversationHydrationStatus::Loading,
            "loadingSubagents",
            88,
            "正在恢复未关闭智能体会话。",
        ),
    )?;
    let subagent_preload_started_at = current_unix_millis_value();
    restore_webui_subagent_registry_for_session_hydration(
        state,
        &refreshed_snapshot,
        parsed_session_id,
    )?;
    let subagent_surface_preload = load_webui_unclosed_subagent_surface_preload_once(
        state,
        &refreshed_snapshot,
        parsed_session_id,
    )?;
    webui_trace_if_slow(
        "session_hydration_subagent_surface_preload_slow",
        subagent_preload_started_at,
        format!("session={parsed_session_id}"),
    );
    emit_conversation_hydration_progress(
        app,
        state,
        conversation_hydration_progress(
            session_id,
            ConversationHydrationStatus::Loading,
            "projecting",
            95,
            "正在整理显示数据。",
        ),
    )?;
    let (
        surface_messages,
        subagent_navigation,
        session_goal_summaries,
        hydrated_windows,
        preloaded_subagent_roots,
    ) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state =
            capture_preserved_workspace_run_state_for_session_hydration(
                guard.workspace_snapshot.as_ref(),
                &active_kernel_sessions,
                parsed_session_id,
            );
        preserve_active_session_run_state(
            preserved_workspace_run_state.as_ref(),
            &mut refreshed_snapshot,
        );
        if let Some(retained_snapshot) = guard.workspace_snapshot.as_ref() {
            preserve_retained_hydrated_payloads_after_index_refresh(
                &mut refreshed_snapshot,
                retained_snapshot,
                parsed_session_id,
            );
        }
        refreshed_snapshot.selected_session =
            selected_session_id_after_refresh(&refreshed_snapshot, Some(parsed_session_id));
        apply_webui_unclosed_subagent_surface_preload_once(
            &mut guard,
            &mut refreshed_snapshot,
            parsed_session_id,
            subagent_surface_preload,
        );
        let mut hydrated_windows = guard.hydrated_conversation_windows.clone();
        remember_hydrated_conversation_window(
            &mut hydrated_windows,
            parsed_session_id,
            message_start_index,
            earliest_ordinal,
            has_more_history,
        );
        retain_hydrated_conversation_windows_for_workspace(
            &mut hydrated_windows,
            &refreshed_snapshot,
        );
        let surface_messages =
            active_surface_messages_from_workspace_and_state(&mut guard, &refreshed_snapshot)?;
        let subagent_navigation =
            subagent_navigation_snapshot_from_cached_state(&mut guard, &refreshed_snapshot)?;
        let session_goal_summaries = guard.session_goal_summaries.clone();
        let preloaded_subagent_roots = guard.webui_preloaded_unclosed_subagent_roots.clone();
        (
            surface_messages,
            subagent_navigation,
            session_goal_summaries,
            hydrated_windows,
            preloaded_subagent_roots,
        )
    };
    let hydrated_window_summaries =
        hydrated_conversation_window_summaries(&hydrated_windows, &preloaded_subagent_roots);
    let vulcan_plugin_catalog = projection_context
        .agent_runtime_for_status
        .as_ref()
        .and_then(|agent_runtime| agent_runtime.extension_generation.plugin_catalog());
    let mcp_server_states = projection_context
        .agent_runtime_for_status
        .as_ref()
        .map(|agent_runtime| {
            agent_runtime
                .extension_generation
                .mcp_runtime()
                .server_states()
        })
        .unwrap_or_default();
    // subagentCatalog is captured from retained runtime state; history hydration cannot reload global catalog roots.
    // subagentCatalog 从保留运行态捕获；历史水合不得重新加载全局目录根。
    let subagent_catalog = projection_context
        .agent_runtime_for_status
        .as_ref()
        .map(|runtime| std::sync::Arc::clone(&runtime.subagent_catalog))
        .unwrap_or_else(|| std::sync::Arc::new(vulcan_task_session::SubagentCatalog::default()));
    // vmm_controller_registry opens the hydrated session before producing read-only UI snapshots.
    // vmm_controller_registry 在生成只读界面快照前打开已水合会话。
    let (vmm_controller_registry, session_directory) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let session_directory = require_segmented_session_directory_for_retained_session(
            &guard,
            &read_context.controller_runtime.data_root,
            parsed_session_id,
        )?;
        (guard.vmm_controller_registry.clone(), session_directory)
    };
    let _ = vmm_controller_registry.get_or_open(
        &session_directory,
        parsed_session_id,
        &current_unix_millis_value().to_string(),
    )?;
    // vmm_session_states contain no mutable controller ownership.
    // vmm_session_states 不包含可变控制器所有权。
    let vmm_session_states = vmm_controller_registry.snapshots()?;
    let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
        controller: &read_context.controller_runtime,
        command_session_runtime: projection_context
            .agent_runtime_for_status
            .as_ref()
            .map(|agent_runtime| agent_runtime.command_session_runtime.as_ref()),
        luaskills_catalog: &projection_context.luaskills_catalog,
        vulcan_plugin_catalog: vulcan_plugin_catalog.as_deref(),
        subagent_catalog: subagent_catalog.as_ref(),
        mcp_server_states: &mcp_server_states,
        mcp_catalog: &projection_context.mcp_catalog,
        provider_configuration: &projection_context.provider_configuration,
        public_ai_runtime: projection_context
            .agent_runtime_for_status
            .as_ref()
            .map(|runtime| runtime.public_ai_runtime.as_ref()),
        workspace: &refreshed_snapshot,
        vmm_session_states: &vmm_session_states,
        managed_vmm_runtime: None,
        surface_messages: surface_messages.as_slice(),
        subagent_navigation,
        pending_questions: pending_question_snapshots_from_agent_runtime(
            projection_context.agent_runtime_for_status.as_ref(),
        )?,
        logs: &projection_context.logs,
        live_context_budgets: &projection_context.live_context_budgets,
        session_goal_summaries: &session_goal_summaries,
        hydrated_conversation_windows: &hydrated_window_summaries,
        native_tools: native_tool_summaries_from_agent_runtime(
            projection_context.agent_runtime_for_status.as_ref(),
        ),
    });
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // final_active_kernel_sessions closes the hydration projection race before committing the rebuilt workbench.
        // final_active_kernel_sessions 在提交重建工作台前关闭水合投影竞态窗口。
        let final_active_kernel_sessions = active_kernel_session_ids(&guard);
        // final_preserved_run_state carries every live session fact that changed while VMM snapshots were read.
        // final_preserved_run_state 携带读取 VMM 快照期间发生变化的全部实时会话事实。
        let final_preserved_run_state = capture_preserved_workspace_run_state_for_session_hydration(
            guard.workspace_snapshot.as_ref(),
            &final_active_kernel_sessions,
            parsed_session_id,
        );
        preserve_active_session_run_state(
            final_preserved_run_state.as_ref(),
            &mut refreshed_snapshot,
        );
        crate::reconcile_workbench_conversation_lifecycle_projection(
            &mut workbench,
            &guard,
            &refreshed_snapshot,
        )?;
        crate::preserve_project_settings_revisions(&mut workbench, &guard.workbench);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        workbench.revision = guard.workbench_generation;
        guard.hydrated_conversation_windows = hydrated_windows;
        guard.workbench = workbench.clone();
        guard.workspace_snapshot = Some(refreshed_snapshot);
    }
    Ok(HydratedSessionWorkbench {
        workbench,
        earliest_ordinal,
        has_more_history,
    })
}

/// Builds the RPC response workbench focused on the requested history-page session.
/// 构建聚焦本次历史分页请求会话的 RPC 响应工作台。
///
/// # Parameters
/// 参数
/// `workbench` is the event snapshot that still represents the service-selected conversation.
/// `workbench` 是仍代表服务端当前选中会话的事件快照。
/// `response_messages` is the fully merged requested-session message window.
/// `response_messages` 是目标请求会话完整合并后的消息窗口。
///
/// # Returns
/// 返回值
/// A cloned workbench whose messages are replaced for the focused RPC response only.
/// 返回仅为本次聚焦 RPC 响应替换消息窗口后的工作台克隆。
pub(crate) fn focused_history_page_response_workbench(
    workbench: &WorkbenchSnapshot,
    response_messages: Vec<ConversationMessage>,
) -> WorkbenchSnapshot {
    let mut response_workbench = workbench.clone();
    response_workbench.messages = response_messages;
    response_workbench
}

/// Builds a lightweight retained workspace snapshot when it still contains the target regular session.
/// 当保留的工作区快照仍包含目标常规会话时构建轻量保留工作区快照。
///
/// # Parameters
/// 参数
/// `retained_workspace_snapshot` is the optional in-memory workspace snapshot retained by the service.
/// `retained_workspace_snapshot` 是服务端保留的可选内存工作区快照。
/// `session_id` identifies the conversation being hydrated.
/// `session_id` 标识正在水合的会话。
///
/// # Returns
/// 返回值
/// `Some(snapshot)` with transcript payloads stripped when the retained snapshot can replace an index reload.
/// 当保留快照可替代索引重载时返回已剥离转录载荷的 `Some(snapshot)`。
pub(crate) fn retained_workspace_snapshot_for_session_hydration(
    retained_workspace_snapshot: Option<&WorkspaceSnapshot>,
    session_id: SessionId,
) -> Option<WorkspaceSnapshot> {
    retained_workspace_snapshot.and_then(|snapshot| {
        workspace_snapshot_contains_regular_session(snapshot, session_id)
            .then(|| lightweight_workspace_index_snapshot_for_hydration(snapshot))
    })
}

/// Clones the retained workspace snapshot for a session after the UI has entered the index-read phase.
/// 在 UI 进入读取索引阶段后，为目标会话克隆保留的工作区快照。
///
/// # Parameters
/// 参数
/// `state` owns the retained in-memory workspace snapshot.
/// `state` 持有保留在内存中的工作区快照。
/// `session_id` identifies the conversation being hydrated.
/// `session_id` 标识正在水合的会话。
///
/// # Returns
/// 返回值
/// `Some(snapshot)` with transcript payloads stripped when the retained snapshot is valid.
/// 当保留快照有效时返回已剥离转录载荷的 `Some(snapshot)`。
fn retained_workspace_snapshot_for_session_hydration_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Option<WorkspaceSnapshot>, String> {
    let clone_started_at = current_unix_millis_value();
    let retained_snapshot = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        retained_workspace_snapshot_for_session_hydration(
            guard.workspace_snapshot.as_ref(),
            session_id,
        )
    };
    webui_trace_if_slow(
        "session_hydration_retained_workspace_clone_slow",
        clone_started_at,
        format!(
            "session={session_id} reused={}",
            retained_snapshot.is_some()
        ),
    );
    Ok(retained_snapshot)
}

/// Builds one lazy conversation hydration progress payload.
/// 构建单个懒加载会话正文水合进度载荷。
fn conversation_hydration_progress(
    session_id: &str,
    status: ConversationHydrationStatus,
    phase: &str,
    percent: u8,
    message: &str,
) -> ConversationHydrationProgress {
    ConversationHydrationProgress {
        session_id: session_id.to_string(),
        status,
        phase: phase.to_string(),
        message: message.to_string(),
        percent: percent.min(100),
    }
}

/// Stores and emits one lazy conversation hydration progress update.
/// 存储并发送一条懒加载会话正文水合进度更新。
fn emit_conversation_hydration_progress(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    progress: ConversationHydrationProgress,
) -> Result<(), String> {
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench.conversation_hydration = Some(progress.clone());
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        WorkbenchDeltaEnvelope {
            generation: guard.workbench_generation,
            delta: WorkbenchDelta::ConversationHydrationChanged(
                ConversationHydrationChangedDelta { progress },
            ),
        }
    };
    emit_service_event_with_trace(
        app,
        "conversation_hydration_progress_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies concurrent cold loads share one exact session lock and release its registry entry.
    /// 验证并发冷加载共享同一个精确会话锁，并在结束后释放其注册表项。
    #[tokio::test]
    async fn conversation_hydration_singleflight_serializes_exact_session() {
        // session_id isolates this test from every other hydration owner.
        // session_id 将本测试与其他水合所有者隔离。
        let session_id = SessionId::new_v4();
        // first_guard owns the exact lock while the second request is queued.
        // first_guard 在第二个请求排队期间持有精确锁。
        let first_guard = ConversationHydrationSingleflightGuard::acquire(session_id)
            .await
            .expect("first hydration guard");
        // waiter represents a concurrent duplicate service request for the same session.
        // waiter 表示同一会话的并发重复服务请求。
        let mut waiter = tokio::spawn(async move {
            ConversationHydrationSingleflightGuard::acquire(session_id).await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut waiter)
                .await
                .is_err(),
            "the duplicate cold load must wait for the exact session owner"
        );
        drop(first_guard);
        // second_guard proves the queued request acquired the same serialized boundary.
        // second_guard 证明排队请求取得了同一个串行化边界。
        let second_guard = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("queued hydration should resume")
            .expect("queued hydration task")
            .expect("second hydration guard");
        drop(second_guard);
        // registry must release exactly this entry without scanning unrelated sessions.
        // registry 必须精确释放当前表项，且不扫描无关会话。
        let registry = CONVERSATION_HYDRATION_SINGLEFLIGHT_LOCKS
            .get()
            .expect("singleflight registry");
        assert!(
            !registry
                .lock()
                .expect("registry lock")
                .contains_key(&session_id)
        );
    }

    /// Builds one visible history message with stable runtime metadata.
    /// 构建一条带稳定运行时元数据的可见历史消息。
    fn history_test_message(id: &str, body: &str, created_at: &str) -> SessionMessageEntry {
        SessionMessageEntry::new(
            vulcan_workbench_model::SessionMessageKind::Assistant,
            "输出",
            body,
        )
        .with_runtime_metadata(id, created_at)
    }

    /// Verifies older history pages are prepended once while retained duplicates keep precedence.
    /// 验证较早历史分页只前置一次，且已保留窗口中的重复项保持优先。
    #[test]
    fn prepends_unique_older_history_page_into_retained_window() {
        let session_id = SessionId::new_v4();
        let retained_duplicate = history_test_message("m2", "retained", "20");
        let retained_newer = history_test_message("m3", "newer", "30");
        let older_unique = history_test_message("m1", "older", "10");
        let older_duplicate = history_test_message("m2", "older duplicate", "20");
        let mut older_tool_detail = history_test_message("detail-m1", "hidden detail", "11");
        older_tool_detail.record_kind = SessionMessageRecordKind::ToolDetailOutput;
        older_tool_detail.visible = false;
        let page = SessionHistoryPage {
            session_id,
            messages: vec![older_unique.clone(), older_duplicate],
            tool_detail_messages: vec![older_tool_detail.clone()],
            message_start_index: Some(0),
            earliest_ordinal: Some(1),
            has_more_history: false,
        };
        let mut retained_messages = vec![retained_duplicate.clone(), retained_newer.clone()];

        // added captures the exact unique visible and hidden entries introduced by the first page.
        // added 捕获首次分页引入的精确唯一可见及隐藏条目数量。
        let added = prepend_unique_older_session_history_page(&mut retained_messages, &page);

        assert_eq!(added, 2);
        assert_eq!(
            retained_messages,
            vec![
                older_unique,
                older_tool_detail,
                retained_duplicate,
                retained_newer
            ]
        );

        // duplicate_added proves replaying the exact same page is a zero-addition operation.
        // duplicate_added 证明重放完全相同分页是零新增操作。
        let duplicate_added =
            prepend_unique_older_session_history_page(&mut retained_messages, &page);

        assert_eq!(duplicate_added, 0);
        assert_eq!(retained_messages.len(), 4);
    }

    /// Verifies workspace refresh distinguishes loaded pagination facts from marker-only sessions.
    /// 验证工作区刷新会区分已加载分页事实与仅有标记的会话。
    #[test]
    fn hydrated_window_retention_marks_missing_pagination_facts_unloaded() {
        let retained_session_id = SessionId::new_v4();
        let marker_only_session_id = SessionId::new_v4();
        let mut workspace = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: retained_session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        workspace
            .hydrated_session_ids
            .extend([retained_session_id, marker_only_session_id]);
        let retained_window = HydratedConversationWindowState {
            transcript_window_loaded: true,
            message_start_index: None,
            earliest_ordinal: Some(354),
            has_more_history: true,
        };
        let mut windows = BTreeMap::from([(retained_session_id, retained_window)]);

        retain_hydrated_conversation_windows_for_workspace(&mut windows, &workspace);

        let restored_window = windows
            .get(&retained_session_id)
            .expect("retained pagination facts must survive refresh");
        assert_eq!(restored_window.message_start_index, None);
        assert_eq!(restored_window.earliest_ordinal, Some(354));
        assert!(restored_window.has_more_history);
        let marker_only_window = windows
            .get(&marker_only_session_id)
            .expect("marker-only session must remain visible to summary projection");
        assert!(!marker_only_window.transcript_window_loaded);
        assert_eq!(marker_only_window.earliest_ordinal, None);
        assert!(!marker_only_window.has_more_history);
    }
}
