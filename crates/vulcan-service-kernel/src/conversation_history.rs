//! Conversation history loading and lazy hydration routes.
//! 会话历史加载与懒水合路由。

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
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
    AgentRuntimeBridge, ContextBudgetSnapshot, ControllerRuntimeHandle,
    PersistedAiToolConfiguration, ProviderConfigurationFile, SELECTED_SESSION_TURN_LOAD_LIMIT,
    SharedServiceBridgeState, TauriLuaSkillsCatalog, TauriMcpRuntimeCatalog,
    TauriTokenBudgetRuntime, WorkbenchRuntimeProjection, active_kernel_session_ids,
    active_surface_messages_from_workspace_and_state,
    apply_hydration_to_selected_workspace_snapshot,
    apply_webui_unclosed_subagent_surface_preload_once, cached_luaskills_catalog_from_state,
    cached_mcp_catalog_from_state, conversation_messages_from_session, current_unix_millis_value,
    emit_service_event_with_trace, emit_workbench_event, load_selected_session_display_hydration,
    load_webui_unclosed_subagent_surface_preload_once, native_tool_summaries_from_agent_runtime,
    pending_question_snapshots_from_agent_runtime, preserve_active_session_run_state,
    restore_webui_subagent_registry_for_session_hydration, selected_session_id_after_refresh,
    session_summary_project_and_created_at, subagent_navigation_snapshot_from_cached_state,
    sync_selected_session_goal_summary_from_store, webui_trace_if_slow, workbench_from_runtime,
    workbench_surface_matches_conversation, workspace_session_summary_for_session,
    workspace_snapshot_contains_regular_session, workspace_surface_allows_user_prompt_editing,
};

mod tool_detail;

pub(super) use tool_detail::*;

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
    /// Token-budget runtime projected into context usage summaries.
    /// 投影到上下文用量摘要中的 Token 预算运行时。
    token_budget_runtime: Arc<TauriTokenBudgetRuntime>,
    /// Cached MCP catalog projected into the rebuilt workbench.
    /// 投影进重建工作台的 MCP 缓存目录。
    mcp_catalog: TauriMcpRuntimeCatalog,
    /// Recent service logs included in the workbench snapshot.
    /// 包含在工作台快照中的最近服务日志。
    logs: Vec<String>,
    /// Provider configuration used to render model and route choices.
    /// 用于渲染模型与路由选项的 Provider 配置。
    provider_configuration: ProviderConfigurationFile,
    /// AI tool configuration used to render tool visibility summaries.
    /// 用于渲染工具可见性摘要的 AI 工具配置。
    ai_tool_configuration: PersistedAiToolConfiguration,
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
    /// Earliest loaded transcript ordinal used to page older turns.
    /// 用于继续分页加载更早轮次的已加载最早转录序号。
    earliest_ordinal: Option<i64>,
    /// Whether older transcript turns exist before the current window.
    /// 当前窗口之前是否存在更早的转录轮次。
    has_more_history: bool,
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
    earliest_ordinal: Option<i64>,
    has_more_history: bool,
) {
    windows.insert(
        session_id,
        HydratedConversationWindowState {
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
        let retained_window = guard
            .hydrated_conversation_windows
            .get(&parsed_session_id)
            .copied()
            .unwrap_or(HydratedConversationWindowState {
                earliest_ordinal: None,
                has_more_history: false,
            });
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
pub(super) async fn service_session_history_page(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    session_id: String,
    before_ordinal: i64,
) -> Result<ConversationHistoryPageResponse, String> {
    let parsed_session_id = session_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid session id '{session_id}': {error}"))?;
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
    let page = if let Some((project_id, created_at)) = session_location {
        vulcan_workspace_store::workspace_store::load_session_history_page_for_project_created_at(
            controller_runtime,
            &project_id,
            &created_at,
            parsed_session_id,
            before_ordinal,
            SELECTED_SESSION_TURN_LOAD_LIMIT,
        )
        .await?
    } else {
        empty_session_history_page(parsed_session_id)
    };
    let (workbench, response_workbench, page_messages) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() else {
            return Err(String::from("service workspace snapshot is not ready"));
        };
        let (response_messages, page_messages) = {
            let allow_user_prompt_editing = workspace_surface_allows_user_prompt_editing(
                workspace_snapshot,
                parsed_session_id,
                None,
            );
            let retained_messages = workspace_snapshot
                .session_messages
                .entry(parsed_session_id)
                .or_default();
            prepend_unique_older_session_history_page(retained_messages, &page);
            (
                conversation_messages_from_session(
                    &session_id,
                    retained_messages,
                    allow_user_prompt_editing,
                ),
                conversation_messages_from_session(&session_id, &page.messages, false),
            )
        };
        remember_hydrated_conversation_window(
            &mut guard.hydrated_conversation_windows,
            parsed_session_id,
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
        (workbench, response_workbench, page_messages)
    };
    emit_workbench_event(&app, &workbench);
    Ok(ConversationHistoryPageResponse {
        session_id,
        workbench: Box::new(response_workbench),
        messages: page_messages,
        earliest_ordinal: page.earliest_ordinal,
        has_more_history: page.has_more_history,
    })
}

/// Builds an empty history page when a session cannot be deterministically located.
/// 当会话无法被确定性定位时构建空历史页。
///
/// # Parameters
/// 参数
/// `session_id` identifies the requested session.
/// `session_id` 标识请求的会话。
///
/// # Returns
/// 返回值
/// An empty page that does not trigger any directory scan fallback.
/// 返回不会触发目录扫描回退的空分页结果。
fn empty_session_history_page(session_id: SessionId) -> SessionHistoryPage {
    SessionHistoryPage {
        session_id,
        messages: Vec::new(),
        tool_detail_messages: Vec::new(),
        earliest_ordinal: None,
        has_more_history: false,
    }
}

/// Prepends an older history page into the retained session message window without duplicating rows.
/// 将更早历史页前置合并到保留的会话消息窗口中，并避免重复行。
/// Prepends older page messages into the retained hydrated window without duplicating stable rows.
/// 将较早分页消息前置合并进已保留的水合窗口，并避免重复稳定行。
fn prepend_unique_older_session_history_page(
    retained_messages: &mut Vec<SessionMessageEntry>,
    page: &SessionHistoryPage,
) {
    let mut seen_keys = retained_messages
        .iter()
        .map(SessionMessageEntry::stable_merge_key)
        .collect::<BTreeSet<_>>();
    let mut merged = page
        .messages
        .iter()
        .chain(page.tool_detail_messages.iter())
        .filter(|message| seen_keys.insert(message.stable_merge_key()))
        .cloned()
        .collect::<Vec<_>>();
    merged.append(retained_messages);
    *retained_messages = merged;
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
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        HydratedSessionProjectionContext {
            luaskills_catalog: cached_luaskills_catalog_from_state(&guard),
            token_budget_runtime: guard.token_budget_runtime.clone().ok_or_else(|| {
                String::from("service is still starting; token budget runtime is not ready")
            })?,
            mcp_catalog: cached_mcp_catalog_from_state(&guard),
            logs: guard.startup.recent_logs.clone(),
            provider_configuration,
            ai_tool_configuration,
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
    let mut workbench = workbench_from_runtime(WorkbenchRuntimeProjection {
        controller: &read_context.controller_runtime,
        command_session_runtime: projection_context
            .agent_runtime_for_status
            .as_ref()
            .map(|agent_runtime| agent_runtime.command_session_runtime.as_ref()),
        luaskills_catalog: &projection_context.luaskills_catalog,
        vulcan_plugin_catalog: vulcan_plugin_catalog.as_deref(),
        mcp_server_states: &mcp_server_states,
        token_budget_runtime: &projection_context.token_budget_runtime,
        mcp_catalog: &projection_context.mcp_catalog,
        provider_configuration: &projection_context.provider_configuration,
        ai_tool_configuration: &projection_context.ai_tool_configuration,
        workspace: &refreshed_snapshot,
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
            earliest_ordinal: Some(1),
            has_more_history: false,
        };
        let mut retained_messages = vec![retained_duplicate.clone(), retained_newer.clone()];

        prepend_unique_older_session_history_page(&mut retained_messages, &page);

        assert_eq!(
            retained_messages,
            vec![
                older_unique,
                older_tool_detail,
                retained_duplicate,
                retained_newer
            ]
        );
    }
}
