//! Workspace persistence write-path helpers extracted from the main workspace store module.
//! 从主工作台存储模块中抽取的写入持久化路径辅助实现。

mod edit_fork;
mod index;
mod locators;
mod projects;
mod runtime_tail;
mod session_lifecycle;
mod snapshot;

#[cfg(test)]
mod tests;

use super::segmented_session_store::{
    SEGMENTED_VMM_CHECKPOINT_FILE, SegmentPolicy, SegmentedSessionCreateRequest,
    SegmentedSessionForkRequest, SegmentedSessionForkSource, SegmentedSessionReplayResult,
    SegmentedSessionStateDocument, append_segmented_session_message_update,
    archive_segmented_session_by_created_date, copy_segmented_directory_tree,
    create_segmented_session_in_location, durable_vmm_state_conflicts_with_checkpoint,
    find_segmented_session_directory, find_segmented_session_directory_by_id,
    find_segmented_session_directory_for_task_store_created_at,
    fork_segmented_session_directory_from_source, read_optional_json, read_session_metadata,
    rebuild_runtime_log_from_conversation_state, record_vmm_store_lifecycle_event_best_effort,
    rename_segmented_session_by_created_date, repair_segmented_session_display_if_needed,
    replay_segmented_session_context, replay_segmented_session_context_without_repair,
    restore_segmented_session_by_created_date, rewrite_json_value_identity_tokens,
    rewrite_string_identity_tokens, state_document_from_runtime_state,
    sync_runtime_log_from_compacted_conversation_state, sync_runtime_log_from_conversation_state,
    to_store_error, truncate_segmented_session_before_message, update_segmented_message_checkpoint,
    write_json_file_atomic, write_metadata, write_segmented_session_fork_source, write_state,
};
use super::session_addon_store::{read_session_addon_state, write_session_addon_state};
use super::*;
use crate::conversation_protocol::{
    SessionConversationPayload, SessionConversationState, conversation_state_from_session_messages,
    conversation_state_to_session_messages,
};
use vulcan_context::merge_session_messages_with_conversation_state_records;
use vulcan_context::{SessionConversationMessage, SessionMessageKind};
use vulcan_core::types::ConversationAttachment;

use index::*;
use locators::*;
pub(in crate::workspace_store) use runtime_tail::{
    persist_segmented_runtime_checkpoint_transactionally,
    recover_segmented_runtime_checkpoint_transaction,
};
use runtime_tail::{
    persist_segmented_session_runtime_state_checkpoint, persist_segmented_session_update,
};
#[cfg(test)]
use snapshot::effective_session_messages_for_persistence;

pub use edit_fork::*;
pub use projects::*;
pub use session_lifecycle::{
    archive_project_sessions, archive_single_session, persist_single_session_addon_state_update,
    remove_project_after_archiving_sessions, rename_single_session, restore_project,
};
pub use snapshot::{
    persist_single_session_runtime_state_update, persist_single_session_transcript_messages,
    persist_single_session_update,
};
