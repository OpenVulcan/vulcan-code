//! Shared workbench-facing domain models reused across GUI, persistence, and runtime layers.
//! 在 GUI、持久化与运行时层之间复用的共享工作台领域模型。

mod activity;
mod project;
mod runtime_state;
mod session;
mod turn_stats;

pub use activity::{ActivityEntry, ActivityLevel};
pub use project::TaskProjectSummary;
pub use runtime_state::{
    PENDING_PROMPT_QUEUE_LIMIT, PendingPromptBusyLaneContext, PendingPromptDispatchBlockReason,
    PendingPromptDispatchContext, PendingPromptModelReference, PendingPromptPanelDropTarget,
    PendingPromptPanelEntry, PendingPromptPanelEntryState, PendingPromptQueueRouteError,
    PendingPromptQueueRouteOutcome, PendingPromptRuntimeOptions, PendingPromptWebSearchOverride,
    PendingSessionPromptEntry, PendingSessionPromptSource, SessionComposerDraftState,
    SessionRuntimeStateEntry, SessionTranscriptUiState, VmmProfileSyncState,
    VmmWritebackOutboxEntry, apply_pending_prompt_panel_drop_target,
    clear_internal_pending_prompt_queue_and_restore_panel, pending_prompt_dispatch_block_reason,
    pending_prompt_dispatch_index, pending_prompt_lane_is_busy,
    pending_session_prompt_entry_from_panel_entry, remove_pending_prompt_panel_entry,
    route_pending_prompt_panel_entry_into_internal_queue, take_guided_pending_prompt_entries,
    take_next_internal_pending_prompt_entry, take_next_pending_prompt_entry,
};
pub use session::{SessionId, SessionRunPhase, SessionState, SessionSummary, SessionTitleSource};
pub use turn_stats::{SessionTurnStats, SessionTurnStatsStatus};
pub use vulcan_context::{
    SessionCompactionEntry, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
};
