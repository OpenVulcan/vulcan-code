//! Session hydration and refreshed workspace state preservation helpers.
//! 会话水合与刷新后工作区状态保留辅助逻辑。

use std::collections::BTreeSet;
use vulcan_workbench_model::{
    SessionId, SessionRunPhase, SessionRuntimeStateEntry, SessionState, SessionSummary,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use super::{
    SharedServiceBridgeState, current_unix_millis_value, is_active_session_run_phase,
    selected_workspace_session_id, session_summary_project_id, webui_trace_if_slow,
    workspace_session_summary_for_session, workspace_snapshot_contains_active_project,
    workspace_task_project_for_project_id, workspace_task_project_for_project_id_mut,
};

mod context_marker;
mod loading;
mod run_state_preservation;

pub(super) use context_marker::*;
pub(super) use loading::*;
pub(super) use run_state_preservation::*;
