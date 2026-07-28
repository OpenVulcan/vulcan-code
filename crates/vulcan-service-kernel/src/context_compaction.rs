//! Context compaction orchestration and WebUI projection helpers.
//! 上下文压缩编排与 WebUI 投影辅助逻辑。

#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use uuid::Uuid;

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
    provider_config::ReasoningEffortLevel,
    session_event_runtime::SessionStreamingRecordIds,
};
#[cfg(test)]
use vulcan_context::SessionConversationMessage;
use vulcan_context::{ContextBudgetSnapshot, SessionConversationPayload, append_record_body_delta};
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport, CompactionPlan,
    CompactionPlanInput, plan_context_compaction,
};
use vulcan_context_injection::InjectionPruneReport;
use vulcan_service_api::{
    ServiceEvent, ServiceEventEmitter, WorkbenchDelta, WorkbenchDeltaEnvelope,
};
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
    AgentRuntimeBridge, ServiceBridgeState, SharedServiceBridgeState, StreamContentKind,
    StreamTaskLaunchFailureCommit, active_public_ai_model_capabilities,
    active_reasoning_selection_effort, active_stream_task_matches_id,
    append_runtime_event_projection_and_release_barrier, append_stream_content_delta,
    append_webui_session_message_entry_to_state, append_webui_session_turn_stats_to_state,
    apply_observed_usage_to_context_budget, apply_session_phase, apply_session_phase_to_state,
    build_conversation_messages_patched_delta, build_vmm_hook_config_snapshot,
    commit_stream_task_registration_failure, complete_streaming_thinking_phase,
    conversation_messages_from_session, current_unix_millis_text,
    current_workspace_snapshot_from_shared_state, emit_service_event_with_trace,
    finish_webui_session_kernel_turn, historical_context_budget_snapshot_from_runtime_state,
    merge_runtime_state_records_into_display_messages, next_webui_session_runtime_persist_version,
    normalized_workspace_path_text_from_path, owning_project_id_for_session,
    persistent_injection_fingerprint, persistent_injection_source_node_ids,
    record_webui_runtime_injection_merged, remove_active_stream_task_if_id,
    replace_conversation_messages_in_workbench_window, spawn_service_task,
    spawn_workspace_session_persist_task, sync_hydrated_session_dialogue_ready_marker,
    webui_session_kernel_generation, webui_session_runtime_persist_version_is_current, webui_trace,
    webui_workspace_persist_lock, workbench_surface_matches_conversation,
    workspace_path_for_session, workspace_root, workspace_snapshot_contains_regular_session,
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
/// Reclaimable VMM anchor tokens that independently justify cache-boundary compaction.
/// 可独立触发缓存边界压缩的 VMM 可回收锚点 Token 数。
const VMM_RECLAIMABLE_ANCHOR_COMPACTION_TOKENS: usize = 32_768;
/// Effective input-window ratio at which reclaimable VMM anchors justify earlier compaction.
/// VMM 可回收锚点足以提前触发压缩的生效输入窗口比例。
const VMM_RECLAIMABLE_ANCHOR_COMPACTION_RATIO: f32 = 0.05;
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

/// Emits a focused message-window delta for one context-compaction session.
/// 为单个上下文压缩会话发出聚焦消息窗口增量。
fn emit_context_compaction_message_window_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    let conversation_id = session_id.to_string();
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let before_visible_messages = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect::<Vec<_>>();
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let messages = workspace
            .session_messages
            .get(&session_id)
            .map(|entries| conversation_messages_from_session(&conversation_id, entries, false))
            .unwrap_or_default();
        if workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
            build_conversation_messages_patched_delta(
                &conversation_id,
                before_visible_messages.as_slice(),
                messages.as_slice(),
            )
            .map(|message_patch| {
                guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
                let generation = guard.workbench_generation;
                guard.workbench.revision = generation;
                replace_conversation_messages_in_workbench_window(
                    &mut guard.workbench.messages,
                    &conversation_id,
                    messages,
                );
                WorkbenchDeltaEnvelope {
                    generation,
                    delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
                }
            })
        } else {
            None
        }
    };
    if let Some(event) = event {
        emit_service_event_with_trace(
            app,
            "context_compaction_messages_patch_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
