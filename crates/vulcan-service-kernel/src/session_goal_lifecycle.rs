//! Session goal lifecycle helpers for accounting, prompt context, and request validation.
//! 会话目标生命周期辅助逻辑，覆盖记账、提示上下文与请求校验。

use std::collections::BTreeMap;

use vulcan_agent_runtime::{
    ExecutionPromptPurpose, ExecutionRunReport, ExecutionSessionGoalContext,
    agent_runtime::provider_request_telemetry::ProviderRequestCompletedUsage,
};
use vulcan_service_api::{SessionGoalStatus, SessionGoalSummary};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::data_plane::HostRuntimeHandle;

use super::{
    SharedServiceBridgeState, parse_session_id, selected_session_id_from_state,
    workspace_snapshot_contains_regular_session,
};

mod accounting;
mod prompt_context;
mod state_cache;
mod validation;

pub(super) use accounting::*;
pub(super) use prompt_context::*;
pub(super) use state_cache::*;
pub(super) use validation::*;
