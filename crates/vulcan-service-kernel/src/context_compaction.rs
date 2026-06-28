//! Context compaction orchestration and WebUI projection helpers.
//! 上下文压缩编排与 WebUI 投影辅助逻辑。

#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use futures_util::StreamExt;
use serde_json::{Value, json};
use vulcan_agent_runtime::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionPromptPurpose,
    ExecutionStreamEvent,
    agent_runtime::{
        context_injection_runtime::core_provider_context_injection_from_state_for_turn,
        prompt_compaction::{
            PromptCompactionExecution, PromptCompactionJobRequest, PromptCompactionProgressEvent,
            PromptCompactionStreamEvent, stream_prompt_compaction_job,
        },
    },
    conversation_protocol::{
        SessionConversationState, compact_conversation_records_after_summary,
        compact_conversation_with_summary_until, conversation_state_to_compaction_source_messages,
    },
    provider_config::{
        PersistedAiToolConfiguration, ProviderConfigurationFile, ReasoningEffortLevel,
    },
    session_event_runtime::SessionStreamingRecordIds,
};
use vulcan_context::{
    ContextBudgetSnapshot, SessionConversationMessage, SessionConversationPayload,
    append_record_body_delta,
};
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport, CompactionPlan,
    CompactionPlanInput, plan_context_compaction,
};
use vulcan_context_injection::InjectionPruneReport;
use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::TurnTerminalStatus;
#[cfg(test)]
use vulcan_workbench_model::SessionSummary;
use vulcan_workbench_model::{
    SessionId, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState, SessionTurnStatsKind, SessionTurnStatsStatus,
    SessionTurnUsageSegment, SessionTurnUsageSegmentKind, build_session_record_message_id,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, acknowledge_runtime_event_projection_barrier,
    active_reasoning_selection_effort, append_runtime_event_projection, append_stream_delta_record,
    append_webui_session_message_entry_to_state, append_webui_session_turn_stats_to_state,
    apply_observed_usage_to_context_budget, apply_session_phase, build_vmm_hook_config_snapshot,
    collapse_streaming_thinking, current_unix_millis_text,
    current_workspace_snapshot_from_shared_state, finish_webui_session_kernel_turn,
    historical_context_budget_snapshot_from_runtime_state,
    merge_runtime_state_records_into_display_messages, next_webui_session_persist_version,
    normalized_workspace_path_text_from_path, owning_project_id_for_session,
    persistent_injection_fingerprint, persistent_injection_source_node_ids,
    record_webui_runtime_injection_merged, session_ai_tool_configuration_or_default,
    spawn_service_task, spawn_workspace_session_persist_task,
    sync_hydrated_session_dialogue_ready_marker, webui_session_persist_version_is_current,
    webui_trace, webui_workspace_persist_lock,
    with_workspace_mutation_preserving_subagent_navigation, workspace_path_for_session,
    workspace_root, workspace_snapshot_contains_regular_session,
};

/// Metadata trigger label for user-requested manual context compaction.
/// 用户手动触发上下文压缩的元数据触发标签。
pub(super) const MANUAL_CONTEXT_COMPACTION_TRIGGER: &str = "manual_slash";
/// Metadata trigger label for host-initiated budget auto-compaction.
/// 宿主因预算触发自动上下文压缩的元数据触发标签。
const AUTO_CONTEXT_COMPACTION_TRIGGER: &str = "auto_budget";
/// Maximum consecutive automatic compaction failures before the session circuit breaker opens.
/// 会话自动压缩熔断前允许的最大连续失败次数。
const MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES: usize = 3;
/// Fixed input-token reserve kept free when deciding whether compaction input must be clone-pruned.
/// 判断压缩输入是否必须进行克隆剪枝时保留的固定输入 Token 安全阈值。
const CONTEXT_COMPACTION_INPUT_SAFETY_RESERVE_TOKENS: usize = 30_000;
/// Minimum allowed reserve for compaction-input pruning decisions.
/// 压缩输入剪枝决策允许使用的最低安全阈值。
const MIN_CONTEXT_COMPACTION_INPUT_SAFETY_RESERVE_TOKENS: usize = 20_000;

mod auto_compaction;
mod budget;
mod completion;
mod draft;
mod messages;
mod persist_display;
mod streaming;

pub(super) use auto_compaction::*;
pub(super) use budget::*;
pub(super) use completion::*;
pub(super) use draft::*;
pub(super) use messages::*;
pub(super) use persist_display::*;
pub(super) use streaming::*;

#[cfg(test)]
mod tests;
