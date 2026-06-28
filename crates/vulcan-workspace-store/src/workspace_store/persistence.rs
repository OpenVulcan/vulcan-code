//! Workspace persistence write-path helpers extracted from the main workspace store module.
//! 从主工作台存储模块中抽取的写入持久化路径辅助实现。

#![allow(dead_code)]

mod deletion;
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
    SegmentedSessionForkSource, append_segmented_session_message_suffix,
    append_segmented_session_message_tail, append_segmented_session_message_update,
    archive_segmented_session_by_created_date, archive_segmented_session_by_id,
    create_segmented_session_in_location, delete_segmented_session_by_created_date,
    delete_segmented_session_by_id, find_segmented_session_directory,
    find_segmented_session_directory_by_id, fork_segmented_session_directory,
    patch_segmented_session_metadata, rebuild_runtime_log_from_conversation_state,
    repair_segmented_session_display_if_needed, replay_segmented_session_context,
    restore_segmented_session_by_created_date, segmented_session_directory,
    state_document_from_runtime_state, sync_runtime_log_from_compacted_conversation_state,
    sync_runtime_log_from_conversation_state, truncate_segmented_session_before_message,
    write_segmented_session_fork_source, write_state,
};
use super::session_addon_store::{read_session_addon_state, write_session_addon_state};
use super::*;
use crate::conversation_protocol::{
    SessionConversationPayload, conversation_state_to_session_messages,
};
use vulcan_context::SessionMessageKind;
use vulcan_context::merge_session_messages_with_conversation_state_records;
use vulcan_core::types::ConversationAttachment;

use index::*;
use locators::*;
use runtime_tail::*;
use session_lifecycle::*;

#[cfg(test)]
use snapshot::{effective_session_messages_for_persistence, snapshot_session_transcript_is_loaded};

pub use deletion::*;
pub use edit_fork::*;
pub use projects::*;
pub use session_lifecycle::{
    archive_project_sessions, archive_project_with_sessions, archive_single_session,
    archive_single_session_for_project, persist_single_session_addon_state_update,
    remove_project_after_archiving_sessions, rename_single_session_metadata,
    restore_project_with_sessions, restore_single_session,
};
pub use snapshot::{
    append_single_session_transcript_suffix, persist_single_session_runtime_state_update,
    persist_single_session_transcript_messages, persist_single_session_update,
    persist_workspace_snapshot,
};
