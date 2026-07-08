//! Shared workbench-facing domain models reused across WebUI, persistence, and runtime layers.
//! 在 WebUI、持久化与运行时层之间复用的共享工作台领域模型。

mod activity;
mod project;
mod runtime_state;
mod session;
mod session_addon;
mod subagent;
mod turn_stats;

pub use activity::{ActivityEntry, ActivityLevel};
pub use project::TaskProjectSummary;
pub use runtime_state::{
    PENDING_PROMPT_QUEUE_LIMIT, PendingPromptBusyLaneContext, PendingPromptDispatchBlockReason,
    PendingPromptDispatchContext, PendingPromptInternalQueueEnqueueError,
    PendingPromptInternalQueueEnqueueOutcome, PendingPromptInternalQueueEnqueuePlan,
    PendingPromptModelReference, PendingPromptPanelDropTarget, PendingPromptPanelEntry,
    PendingPromptPanelEntryState, PendingPromptQueueRouteError, PendingPromptQueueRouteOutcome,
    PendingPromptRuntimeOptions, PendingPromptWebSearchOverride, PendingSessionPromptEntry,
    PendingSessionPromptSource, SessionComposerDraftState, SessionPlanArtifactState,
    SessionPlanModeState, SessionRuntimeStateEntry, SessionTodolistItem, SessionTodolistItemStatus,
    SessionTodolistState, SessionTranscriptUiState, VmmProfileSyncState, VmmWritebackOutboxEntry,
    apply_pending_prompt_internal_queue_enqueue_plan, apply_pending_prompt_panel_drop_target,
    clear_internal_pending_prompt_queue_and_restore_panel,
    enqueue_pending_prompt_internal_queue_entry, next_pending_prompt_dispatch_source,
    pending_prompt_dispatch_block_reason, pending_prompt_dispatch_block_reason_for_source,
    pending_prompt_dispatch_index, pending_prompt_lane_is_busy,
    pending_session_prompt_entry_from_panel_entry, plan_pending_prompt_internal_queue_enqueue,
    remove_internal_pending_prompt_entries_by_source, remove_pending_prompt_panel_entry,
    route_pending_prompt_panel_entry_into_internal_queue, take_guided_pending_prompt_entries,
    take_next_internal_pending_prompt_entry, take_next_pending_prompt_entry,
    take_runtime_intervention_guided_pending_prompt_entries,
};
pub use session::{
    SessionId, SessionRunPhase, SessionState, SessionSummary, SessionTitleSource,
    build_session_record_message_id,
};
pub use session_addon::{
    SESSION_ADDON_SCHEMA_VERSION, SessionAddonNamespace, SessionAddonState, SessionContextAnchor,
    SessionRuntimeControls, SessionWorkMode, VersionedSessionAddonValue,
};
pub use subagent::{
    SUBAGENT_TASK_STOPPED_SUMMARY_TEXT, SelectedSubagentTaskPanel, SubagentCloseAgentProjection,
    SubagentFeedbackDisplayRecords, SubagentSendMessageProjection, SubagentTaskPanelState,
    SubagentTaskRunState, TaskSessionMirrorDisplayMetadataUpdate,
    attach_tool_display_state_to_metadata, build_stopped_task_session_metadata,
    collect_running_subagent_task_states_for_tree, find_task_session_tool_message_id_for_payload,
    merge_subagent_task_panel_cache, merge_task_mirror_tool_display_metadata,
    merge_tool_display_metadata, normalize_subagent_task_panel_state,
    parse_subagent_task_panel_metadata, parse_subagent_task_panel_payload,
    parse_subagent_task_prompt_metadata, session_message_is_subagent_send_message_display_note,
    subagent_close_agent_projection_from_host_result,
    subagent_close_agent_projection_from_tool_metadata,
    subagent_feedback_display_records_from_tool_metadata,
    subagent_feedback_execution_id_from_tool_metadata, subagent_feedback_record_matches,
    subagent_navigation_round_should_remain_visible, subagent_navigation_surface_is_visible,
    subagent_navigation_task_label, subagent_parent_ref_from_task_session_store,
    subagent_run_state_from_execution_status, subagent_run_state_from_task_session_status,
    subagent_send_message_projection_from_tool_metadata, subagent_task_panel_is_openable,
    subagent_task_panel_state_detail_from_segmented_task_session_store,
    subagent_task_panel_state_from_task_session,
    subagent_task_panel_state_has_recoverable_conversation,
    subagent_task_panel_states_from_segmented_task_session_store,
    task_session_arguments_from_tool_metadata, task_session_declared_task_id_from_tool_metadata,
    task_session_description_from_tool_metadata, task_session_display_metadata_update,
    task_session_display_title_from_payload, task_session_display_title_from_tool_metadata,
    task_session_execution_status_from_tool_metadata, task_session_internal_id_from_payload,
    task_session_message_match_score, task_session_mirror_display_metadata_update,
    task_session_payload_from_tool_metadata, task_session_prompt_text_from_tool_metadata,
    task_session_public_id_from_payload, task_session_subagent_from_tool_metadata,
    task_session_tool_display_label,
};
pub use turn_stats::{
    SessionTurnStats, SessionTurnStatsKind, SessionTurnStatsStatus, SessionTurnUsageGroup,
    SessionTurnUsageGroupKind, SessionTurnUsageSegment, SessionTurnUsageSegmentKind,
};
pub use vulcan_context::{
    SessionCompactionEntry, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
};
