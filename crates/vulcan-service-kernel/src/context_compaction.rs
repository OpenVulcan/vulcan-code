//! Context compaction orchestration and WebUI projection helpers.
//! 上下文压缩编排与 WebUI 投影辅助逻辑。

use std::sync::Arc;
use uuid::Uuid;

use futures_util::StreamExt;
use serde_json::{Value, json};
use vulcan_agent_runtime::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionStreamEvent,
    agent_runtime::{
        context_injection_runtime::core_provider_context_injection_from_state_for_turn_draining_expiry_events,
        prompt_compaction::{
            PromptCompactionExecution, PromptCompactionJobRequest, PromptCompactionLaunchContext,
            PromptCompactionModelTarget, PromptCompactionProgressEvent,
            PromptCompactionStreamEvent, resolve_prompt_compaction_model_target,
            stream_prompt_compaction_job,
        },
    },
    conversation_protocol::SessionConversationState,
    provider_config::ReasoningEffortLevel,
};
use vulcan_context::{ContextBudgetSnapshot, ContextCompactionPolicy, append_record_body_delta};
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionPlan, CompactionPlanInput, plan_context_compaction,
};
use vulcan_service_api::{ServiceEvent, ServiceEventEmitter, WorkbenchDeltaEnvelope};
use vulcan_session_core::{
    TurnTerminalStatus,
    v1::{
        ids::Seq,
        projection::{ProjectedItem, SourcedProjectedItem},
    },
};
use vulcan_workbench_model::{
    SessionId, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState, SessionTurnUsageSegment, SessionTurnUsageSegmentKind,
};
use vulcan_workspace_store::{data_plane::HostRuntimeHandle, workspace_store::WorkspaceSnapshot};

use super::{
    AgentRuntimeBridge, ServiceBridgeState, SharedServiceBridgeState,
    StreamTaskLaunchFailureCommit, active_reasoning_selection_effort,
    active_stream_task_matches_id, append_runtime_event_projection_and_release_barrier,
    append_webui_session_message_entry_to_state, apply_observed_usage_to_context_budget,
    apply_session_phase, apply_session_phase_to_state, commit_stream_task_registration_failure,
    current_unix_millis_text, current_unix_millis_value, emit_service_event_with_trace,
    finish_webui_session_kernel_turn, historical_context_budget_snapshot_from_runtime_state,
    owning_project_id_for_session, refresh_exact_vmm_hook_session_projection,
    remove_active_stream_task_if_id, session_public_ai_model_capabilities, spawn_service_task,
    sync_hydrated_session_dialogue_ready_marker, webui_session_kernel_generation, webui_trace,
    webui_vmm_runtime_readiness, workspace_path_for_session,
    workspace_snapshot_contains_regular_session,
};

/// Metadata trigger label for user-requested manual context compaction.
/// 用户手动触发上下文压缩的元数据触发标签。
pub(super) const MANUAL_CONTEXT_COMPACTION_TRIGGER: &str = "manual_slash";
/// Metadata trigger label for host-initiated budget auto-compaction.
/// 宿主因预算触发自动上下文压缩的元数据触发标签。
const AUTO_CONTEXT_COMPACTION_TRIGGER: &str = "auto_budget";
/// Maximum consecutive automatic compaction failures within one stream execution.
/// 单个流执行内允许的最大连续自动压缩失败次数。
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
mod manual;
mod messages;
mod persist_display;
mod streaming;
mod vmm_projection;

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
    _app: &ServiceEventEmitter,
    _state: &SharedServiceBridgeState,
    _session_id: SessionId,
) -> Result<(), String> {
    // The V1 compaction mutation publishes its own sparse event patch; Workbench no longer mirrors transcript rows.
    // V1 压缩变更会发布自身的稀疏事件补丁；Workbench 不再镜像转录行。
    Ok(())
}
