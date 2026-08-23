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
    SegmentPolicy, SegmentedSessionCreateRequest, SegmentedSessionForkRequest,
    SegmentedSessionForkSource, SegmentedSessionReplayResult,
    append_segmented_session_message_tail_update, append_segmented_session_message_update,
    archive_segmented_session_by_created_date, create_segmented_session_in_location,
    finalize_staged_archived_session_deletion,
    find_segmented_session_directory_for_task_store_created_at,
    fork_segmented_session_directory_from_source, history_rewrite_mutable_relative_paths,
    read_session_metadata, rebuild_runtime_log_from_conversation_state,
    rename_segmented_session_by_created_date, repair_segmented_session_display_if_needed,
    replay_segmented_session_context_without_repair, restore_segmented_session_by_created_date,
    rewrite_json_value_identity_tokens, rewrite_string_identity_tokens,
    rollback_staged_archived_session_deletion, segmented_session_directory,
    segmented_session_directory_exists, stage_archived_segmented_session_deletion,
    state_document_from_runtime_state, sync_runtime_log_from_compacted_conversation_state,
    sync_runtime_log_from_conversation_state, to_store_error,
    truncate_segmented_session_before_message, update_segmented_message_checkpoint, write_metadata,
    write_segmented_session_fork_source, write_state,
};
use super::session_addon_store::{read_session_addon_state, write_session_addon_state};
use super::*;
use crate::conversation_protocol::{
    SessionConversationPayload, SessionConversationState, conversation_state_from_session_messages,
    conversation_state_to_session_messages,
};
use vulcan_context::merge_session_messages_with_conversation_state_records;
use vulcan_context::{SessionConversationMessage, SessionMessageKind};

use index::*;
use locators::*;
use runtime_tail::{
    persist_segmented_session_runtime_state_checkpoint, persist_segmented_session_update,
};
pub(in crate::workspace_store) use runtime_tail::{
    reject_historical_segmented_runtime_checkpoint, sync_segmented_runtime_log_for_state,
};
#[cfg(test)]
use snapshot::effective_session_messages_for_persistence;

pub use edit_fork::*;
pub use projects::*;
pub use session_lifecycle::{
    archive_project_sessions, archive_single_session, cleanup_missing_session_index,
    delete_archived_session, persist_single_session_addon_state_update,
    persist_single_session_addon_state_update_in_directory,
    remove_project_after_archiving_sessions, rename_single_session, restore_archived_session,
    restore_project,
};
pub use snapshot::{
    load_single_session_transcript_message_count_in_directory,
    persist_single_session_runtime_state_update,
    persist_single_session_transcript_messages_in_directory,
    persist_single_session_transcript_tail_in_directory, persist_single_session_update,
};
