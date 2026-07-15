//! Session hydration and refreshed workspace state preservation helpers.
//! 会话水合与刷新后工作区状态保留辅助逻辑。

use std::{collections::BTreeSet, sync::Arc};

use vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime;
use vulcan_workbench_model::{
    SessionId, SessionMessageEntry, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
    SessionSummary,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle,
    workspace_store::{SessionHydrationSnapshot, WorkspaceSnapshot},
};

use super::{
    SELECTED_SESSION_TURN_LOAD_LIMIT, apply_persisted_task_mirror_statuses_to_workspace,
    current_unix_millis_value, filter_legacy_goal_internal_user_messages,
    is_active_session_run_phase, runtime_state_has_queued_pending_prompt_work,
    sanitize_runtime_state_legacy_goal_internal_messages, selected_workspace_session_id,
    session_summary_project_and_created_at, session_summary_project_id,
    webui_subagent_task_has_live_run_for_phase, webui_trace_if_slow,
    workspace_session_summary_for_session, workspace_snapshot_contains_active_project,
    workspace_task_project_for_project_id, workspace_task_project_for_project_id_mut,
};

mod context_marker;
mod loading;
mod run_state_preservation;

pub(super) use context_marker::*;
pub(super) use loading::*;
pub(super) use run_state_preservation::*;

#[cfg(test)]
mod tests;
