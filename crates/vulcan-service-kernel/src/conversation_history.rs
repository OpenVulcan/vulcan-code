//! Conversation history loading and lazy hydration routes.
//! 会话历史加载与懒水合路由。

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use vulcan_context::SessionConversationRecord;
use vulcan_service_api::{
    ConversationHistoryPageResponse, ConversationHydrationProgress, ConversationHydrationStatus,
    ConversationMessagesLoadResponse, HydratedConversationWindowSummary, ServiceEventEmitter,
    WorkbenchSnapshot,
};
use vulcan_workbench_model::{SessionId, SessionMessageEntry, SessionMessageRecordKind};
use vulcan_workspace_store::workspace_store::{SessionHistoryPage, WorkspaceSnapshot};

use super::{
    SELECTED_SESSION_TURN_LOAD_LIMIT, SharedServiceBridgeState, active_kernel_session_ids,
    active_surface_messages_from_workspace_and_state, append_hydrated_workspace_messages,
    apply_hydration_to_selected_workspace_snapshot, cached_luaskills_catalog_from_state,
    cached_mcp_catalog_from_state, capture_preserved_workspace_run_state,
    conversation_messages_from_session, emit_workbench_event,
    load_selected_session_display_hydration, native_tool_summaries_from_agent_runtime,
    pending_question_snapshots_from_agent_runtime, preserve_active_session_run_state,
    subagent_navigation_snapshot_from_state, sync_selected_session_goal_summary_from_store,
    workbench_from_runtime, workspace_surface_allows_user_prompt_editing,
};

mod tool_detail;

pub(super) use tool_detail::*;

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
) -> Vec<HydratedConversationWindowSummary> {
    windows
        .iter()
        .map(|(session_id, window)| HydratedConversationWindowSummary {
            session_id: session_id.to_string(),
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
fn remember_hydrated_conversation_window(
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
        .map_err(|error| {
            let _ = emit_conversation_hydration_progress(
                &app,
                &state,
                conversation_hydration_progress(
                    &session_id,
                    ConversationHydrationStatus::Error,
                    "failed",
                    100,
                    &error,
                ),
            );
            error
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
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let (window, session_messages, allow_user_prompt_editing) = {
        let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
            return Ok(None);
        };
        if !workspace_snapshot
            .hydrated_session_ids
            .contains(&parsed_session_id)
        {
            return Ok(None);
        }
        let Some(window) = guard
            .hydrated_conversation_windows
            .get(&parsed_session_id)
            .copied()
        else {
            return Ok(None);
        };
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
        (window, session_messages, allow_user_prompt_editing)
    };
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() {
        workspace_snapshot.selected_session = parsed_session_id;
    }
    guard.active_subagent_panel = None;
    let mut messages = conversation_messages_from_session(
        session_id,
        &session_messages,
        allow_user_prompt_editing,
    );
    if let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() {
        append_hydrated_workspace_messages(&mut messages, workspace_snapshot);
    }
    let progress = conversation_hydration_progress(
        session_id,
        ConversationHydrationStatus::Ready,
        "done",
        100,
        "会话载入完成。",
    );
    guard.workbench.selected_conversation_id = Some(session_id.to_string());
    guard.workbench.messages = messages.clone();
    guard.workbench.hydrated_conversation_windows =
        hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows);
    guard.workbench.conversation_hydration = Some(progress.clone());
    guard.workbench.subagent_navigation = None;
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
    let controller_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?
    };
    let page = vulcan_workspace_store::workspace_store::load_session_history_page(
        controller_runtime,
        parsed_session_id,
        before_ordinal,
        SELECTED_SESSION_TURN_LOAD_LIMIT,
    )
    .await?;
    let page_messages = conversation_messages_from_session(&session_id, &page.messages, false);
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(workspace_snapshot) = guard.workspace_snapshot.as_mut() else {
            return Err(String::from("service workspace snapshot is not ready"));
        };
        let merged_messages = merge_older_session_history_page(
            workspace_snapshot
                .session_messages
                .entry(parsed_session_id)
                .or_default(),
            &page,
        );
        let allow_user_prompt_editing = workspace_surface_allows_user_prompt_editing(
            workspace_snapshot,
            parsed_session_id,
            None,
        );
        remember_hydrated_conversation_window(
            &mut guard.hydrated_conversation_windows,
            parsed_session_id,
            page.earliest_ordinal,
            page.has_more_history,
        );
        guard.workbench.hydrated_conversation_windows =
            hydrated_conversation_window_summaries(&guard.hydrated_conversation_windows);
        if guard.workbench.selected_conversation_id.as_deref() == Some(session_id.as_str()) {
            guard.workbench.messages = conversation_messages_from_session(
                &session_id,
                &merged_messages,
                allow_user_prompt_editing,
            );
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        guard.workbench.clone()
    };
    emit_workbench_event(&app, &workbench);
    Ok(ConversationHistoryPageResponse {
        session_id,
        workbench: Box::new(workbench),
        messages: page_messages,
        earliest_ordinal: page.earliest_ordinal,
        has_more_history: page.has_more_history,
    })
}

/// Prepends an older history page into the retained session message window without duplicating rows.
/// 将更早历史页前置合并到保留的会话消息窗口中，并避免重复行。
fn merge_older_session_history_page(
    retained_messages: &mut Vec<SessionMessageEntry>,
    page: &SessionHistoryPage,
) -> Vec<SessionMessageEntry> {
    let mut seen_keys = retained_messages
        .iter()
        .map(session_message_entry_stable_merge_key)
        .collect::<BTreeSet<_>>();
    let mut merged = page
        .messages
        .iter()
        .filter(|message| seen_keys.insert(session_message_entry_stable_merge_key(message)))
        .cloned()
        .collect::<Vec<_>>();
    merged.append(retained_messages);
    *retained_messages = merged.clone();
    retained_messages.clone()
}

/// Returns the stable merge key used by transcript history pagination.
/// 返回转录历史分页使用的稳定合并键。
fn session_message_entry_stable_merge_key(message: &SessionMessageEntry) -> String {
    if !message.id.is_empty() {
        return message.id.clone();
    }
    format!(
        "{}:{}:{}",
        message.created_at,
        message.record_kind.as_persisted_key(),
        message.body
    )
}

/// Loads one conversation using the original selected-session workbench projection path.
/// 使用原始选中会话工作台投影路径加载单个会话。
pub(super) async fn load_hydrated_session_workbench(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    parsed_session_id: SessionId,
    session_id: &str,
) -> Result<HydratedSessionWorkbench, String> {
    let (
        controller_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
        preserved_workspace_run_state,
        provider_configuration,
        ai_tool_configuration,
        live_context_budgets,
        agent_runtime_for_status,
        live_listener_task_ids,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            None,
        );
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
        (
            controller_runtime,
            cached_luaskills_catalog_from_state(&guard),
            guard.token_budget_runtime.clone().ok_or_else(|| {
                String::from("service is still starting; token budget runtime is not ready")
            })?,
            cached_mcp_catalog_from_state(&guard),
            guard.startup.recent_logs.clone(),
            preserved_workspace_run_state,
            provider_configuration,
            ai_tool_configuration,
            guard.live_context_budgets.clone(),
            guard.agent_runtime.clone(),
            guard.detached_subagent_task_listeners.clone(),
        )
    };
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
    let mut refreshed_snapshot =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller_runtime.clone(),
        )
        .await?;
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
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
    let hydration = load_selected_session_display_hydration(
        controller_runtime.clone(),
        &refreshed_snapshot,
        parsed_session_id,
    )
    .await?;
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
    apply_hydration_to_selected_workspace_snapshot(
        &controller_runtime,
        &mut refreshed_snapshot,
        parsed_session_id,
        hydration,
        agent_runtime_for_status
            .as_ref()
            .map(|runtime| &runtime.task_runtime),
        &live_listener_task_ids,
    )?;
    sync_selected_session_goal_summary_from_store(
        state,
        controller_runtime.clone(),
        parsed_session_id,
    )
    .await?;
    let (surface_messages, subagent_navigation, session_goal_summaries, hydrated_windows) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            None,
        );
        preserve_active_session_run_state(
            preserved_workspace_run_state.as_ref(),
            &mut refreshed_snapshot,
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
        (
            active_surface_messages_from_workspace_and_state(&mut guard, &refreshed_snapshot),
            subagent_navigation_snapshot_from_state(&mut guard, &refreshed_snapshot),
            guard.session_goal_summaries.clone(),
            hydrated_windows,
        )
    };
    let hydrated_window_summaries = hydrated_conversation_window_summaries(&hydrated_windows);
    let mut workbench = workbench_from_runtime(
        &controller_runtime,
        agent_runtime_for_status
            .as_ref()
            .map(|agent_runtime| agent_runtime.command_session_runtime.as_ref()),
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &provider_configuration,
        &ai_tool_configuration,
        &refreshed_snapshot,
        surface_messages.as_slice(),
        subagent_navigation,
        pending_question_snapshots_from_agent_runtime(agent_runtime_for_status.as_ref()),
        &logs,
        &live_context_budgets,
        &session_goal_summaries,
        &hydrated_window_summaries,
        native_tool_summaries_from_agent_runtime(agent_runtime_for_status.as_ref()),
    );
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
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
    let workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench.conversation_hydration = Some(progress);
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        guard.workbench.clone()
    };
    emit_workbench_event(app, &workbench);
    Ok(())
}
