//! WebUI prompt lifecycle orchestration around precheck and queued dispatch.
//! WebUI 提示词生命周期编排，覆盖 precheck 与排队分发。

use std::collections::{BTreeMap, BTreeSet};

use vulcan_agent_runtime::{
    ExecutionPreparationRequest,
    agent_runtime::{
        conversation_hooks::ConversationPrecheckOutcome,
        events::{RuntimeEvent, RuntimeEventKind},
    },
    session_event_runtime::append_runtime_event_projection_to_state,
    workbench_ai_capability_router::effective_ai_tool_configuration_for_prompt_overrides,
    workbench_prompt_turn::{
        SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, SharedPromptTurnContinueInput,
        SharedPromptTurnStart, begin_shared_prompt_turn,
        continue_shared_prompt_turn_after_precheck, shared_prompt_precheck_plan,
    },
};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::ActiveTurnKind;
use vulcan_workbench_model::{
    PendingPromptDispatchBlockReason, PendingSessionPromptSource, SessionId, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState, next_pending_prompt_dispatch_source,
    pending_prompt_dispatch_block_reason_for_source,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use super::pending_prompts::{
    agent_runtime_for_pending_prompt, clear_stale_webui_session_mailbox_pending_prompts,
    emit_pending_prompts_changed_delta, insert_webui_pending_prompt_into_internal_queue,
    pending_prompt_queue_status, remove_dispatched_pending_prompt_panel_entry,
    take_next_webui_pending_prompt, webui_pending_prompt_dispatch_context,
};
use super::session_hydration::{
    capture_preserved_workspace_run_state, load_workspace_for_submission,
    preserve_active_session_run_state, retained_submission_snapshot_has_session_context,
};
use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, active_kernel_session_ids,
    active_reasoning_selection_effort, apply_webui_prompt_begin_outer_steps,
    build_vmm_hook_config_snapshot, cached_project_vmm_layer,
    commit_workspace_snapshot_and_emit_session_phase_delta, current_unix_millis_text,
    current_unix_millis_value, current_workspace_snapshot_from_shared_state,
    current_workspace_snapshot_from_state, emit_materialized_session_message_delta,
    ensure_webui_session_runtime_state, load_project_configuration,
    merge_runtime_state_records_into_display_messages, persist_webui_context_current_document,
    push_webui_service_diagnostic, runtime_parts_from_state,
    segmented_session_directory_for_workspace_session, selected_session_id_from_state,
    session_ai_tool_configuration_or_default, session_goal_context_for_prompt_from_state,
    session_has_pending_plan_artifact, session_plan_context_for_prompt_from_state,
    snapshot_webui_context_before_user_prompt, spawn_agent_stream_task, spawn_service_task,
    spawn_workspace_session_persist_task, spawn_workspace_session_runtime_state_persist_task,
    start_webui_session_kernel_turn, stop_webui_session_kernel_generation,
    store_workspace_snapshot_without_workbench_rebuild,
    sync_hydrated_session_dialogue_ready_marker,
    sync_webui_session_runtime_kernel_to_workspace_state, webui_after_precheck_trace,
    webui_session_has_active_run, webui_session_kernel_has_active_turn,
    webui_session_mailbox_agent_messages_from_kernel, webui_trace, webui_trace_if_slow,
    workspace_path_for_session,
};

/// Inserts one visible pending prompt and starts dispatch immediately when the session can run.
mod pending_dispatch;
mod pending_prepare;
mod precheck_continue;

pub(super) use pending_dispatch::*;
pub(super) use pending_prepare::*;
pub(super) use precheck_continue::*;
