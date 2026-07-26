use super::*;
use crate::git::{
    WebuiWorkspaceDiffMode, run_webui_git_command, webui_diff_hunk_scoped_unified_diff,
    webui_unified_diff_file_summaries, webui_unified_diff_line_count, webui_unified_diff_stats,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use vulcan_agent_runtime::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload,
    conversation_state_to_compaction_source_messages,
};
use vulcan_agent_runtime::provider_capabilities::ResolvedAiToolCapability;
use vulcan_provider_config::config::{
    AiToolPolicy, PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
};
use vulcan_workbench_model::SessionTurnUsageSegmentKind;

pub(crate) mod test_support;

use test_support::*;

mod active_session_state_tests;
mod ai_capabilities_tests;
mod conversation_selection_tests;
mod conversation_submission_activation_tests;
mod conversation_submission_preflight_tests;
mod conversation_submission_queue_tests;
mod conversation_tool_detail_tests;
mod detached_subagent_tests;
mod goal_todolist_tests;
mod mailbox_tests;
mod mcp_settings_tests;
mod pending_question_projection_tests;
mod prompt_runtime_state_tests;
mod provider_settings_tests;
mod rpc_diff_tests;
mod session_kernel_tests;
mod settings_projection_tests;
mod stop_task_mirror_tests;
mod subagent_fixed_model_tests;
mod subagent_navigation_tests;
mod thinking_completion_tests;
mod workbench_model_routes_tests;
