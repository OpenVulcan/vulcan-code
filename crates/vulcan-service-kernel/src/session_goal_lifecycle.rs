//! Session goal lifecycle helpers for accounting, prompt context, and request validation.
//! 会话目标生命周期辅助逻辑，覆盖记账、提示上下文与请求校验。

use std::collections::BTreeMap;

use serde_json::Value;
use vulcan_agent_runtime::{
    ExecutionPromptPurpose, ExecutionRunReport,
    agent_runtime::events::{RuntimeEvent, RuntimeEventKind},
    conversation_protocol::SessionConversationState,
    turn_stats::build_session_turn_stats_from_state_with_segments,
};
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_service_api::SessionGoalSummary;
use vulcan_workbench_model::{
    PendingPromptRuntimeOptions, PendingSessionPromptEntry, PendingSessionPromptSource, SessionId,
    SessionRuntimeStateEntry, SessionTurnStats, SessionTurnStatsKind, SessionTurnStatsStatus,
    SessionTurnUsageSegment,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle,
    workspace_store::{
        GoalAccountingMode, GoalAccountingOutcome, GoalUpdate, ThreadGoal, ThreadGoalStatus,
        VulcanGoalStore,
    },
};

use super::pending_prompts::enqueue_internal_pending_prompt_entry;
use super::{
    SharedServiceBridgeState, current_unix_millis_text, parse_session_id,
    selected_session_id_from_state, webui_session_has_active_run,
    webui_session_kernel_has_active_turn, workspace_snapshot_contains_regular_session,
};

mod accounting;
mod budget_guidance;
mod prompt_context;
mod state_cache;
mod summary_parse;
mod validation;

pub(super) use accounting::*;
pub(super) use budget_guidance::*;
pub(super) use prompt_context::*;
pub(super) use state_cache::*;
pub(super) use summary_parse::*;
pub(super) use validation::*;
