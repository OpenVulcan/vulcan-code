//! WebUI prompt lifecycle orchestration around precheck and queued dispatch.
//! WebUI 提示词生命周期编排，覆盖 precheck 与排队分发。

use vulcan_agent_runtime::{
    ExecutionPreparationRequest,
    agent_runtime::conversation_hooks::{ConversationPrecheckOutcome, ConversationProfileOutcome},
    workbench_prompt_turn::{
        SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, SharedPromptTurnContinueInput,
        SharedPromptTurnStart, begin_shared_prompt_turn,
        continue_shared_prompt_turn_after_preparation, shared_prompt_precheck_plan,
    },
};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::ActiveTurnKind;
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionId, SessionRunPhase, SessionState,
};
use vulcan_workspace_store::{data_plane::HostRuntimeHandle, workspace_store::WorkspaceSnapshot};

use super::pending_prompts::{
    agent_runtime_for_pending_prompt, emit_pending_prompts_changed_delta,
};
use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, active_reasoning_selection_effort,
    cached_project_vmm_layer, capture_webui_prompt_preparation_generation,
    capture_webui_prompt_preparation_workspace,
    commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current,
    current_unix_millis_text, current_unix_millis_value, ensure_webui_session_runtime_entry,
    load_project_configuration, persist_goal_turn_usage_context, push_webui_service_diagnostic,
    refresh_exact_vmm_hook_session_projection, selected_session_id_from_state,
    session_goal_prompt_parts_from_state, session_has_pending_plan_artifact,
    spawn_agent_stream_task, spawn_service_task, stop_webui_session_kernel_generation,
    webui_after_precheck_trace, webui_session_has_active_run, webui_session_kernel_has_active_turn,
    webui_trace, workspace_path_for_session,
};

mod agent_task_prepare;
mod collaboration_prepare;
mod internal_continuation_prepare;
pub(crate) use collaboration_prepare::maybe_prepare_webui_collaboration_continuation;
/// Inserts one visible pending prompt and starts dispatch immediately when the session can run.
mod pending_dispatch;
mod pending_prepare;
mod precheck_continue;
mod question_prepare;

pub(crate) use agent_task_prepare::*;
pub(super) use pending_dispatch::*;
pub(super) use pending_prepare::*;
pub(super) use precheck_continue::*;
pub(crate) use question_prepare::spawn_webui_question_prepare_task;
