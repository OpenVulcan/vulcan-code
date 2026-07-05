//! Session-loading helpers extracted from the main workspace-store module.
//! 从主工作区存储模块中抽取的会话加载辅助逻辑。

mod hydration;
mod locator;
mod pages;
mod runtime_state;
mod types;
mod windows;

pub use hydration::{
    load_session_display_hydration_snapshot_for_project_created_at,
    load_session_hydration_snapshot_for_project_created_at,
};
pub use pages::{
    load_session_history_page_for_project_created_at,
    load_session_transcript_window_for_project_created_at,
};

use super::segmented_session_store::{
    SegmentedSessionReplayResult, ensure_runtime_log_from_segmented_context,
    find_segmented_session_directory_for_task_store_created_at,
    read_segmented_session_event_window, read_segmented_session_turn_window, read_state,
    repair_segmented_session_display_if_needed, replay_segmented_session_context,
    segmented_lines_to_session_messages,
};
use super::session_addon_store::read_session_addon_state;
use super::*;
use hydration::repair_duplicate_conversation_record_ids;
use locator::{
    find_segmented_session_dir_for_locator, query_session_locator_for_project_created_at,
};
#[cfg(test)]
use runtime_state::{apply_segmented_current_context, runtime_state_entry_from_segmented_replay};
use runtime_state::{
    hydrate_session_runtime_state, load_segmented_runtime_state,
    resolve_initial_hydration_turn_count, restore_hydration_window_from_runtime_state_if_empty,
};
use types::{
    SegmentedWindowMessages, SegmentedWindowSourcePreference, StoredSessionLocatorRow,
    default_session_hydration_window_source_preference,
};
#[cfg(test)]
use windows::{
    read_segmented_session_boundary_turn_window, session_boundary_lines_to_window_messages,
};
use windows::{
    apply_boundary_conversation_projection_to_runtime_state, load_segmented_window_messages,
};

#[cfg(test)]
mod tests;
