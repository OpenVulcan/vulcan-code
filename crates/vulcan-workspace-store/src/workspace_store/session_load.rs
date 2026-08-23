//! Session-loading helpers extracted from the main workspace-store module.
//! 从主工作区存储模块中抽取的会话加载辅助逻辑。

mod hydration;
mod locator;
mod pages;
mod runtime_state;
mod tool_detail_hydration;
mod types;
mod windows;

pub use hydration::{
    load_session_display_hydration_snapshot_for_project_created_at,
    load_session_execution_hydration_snapshot_for_project_created_at,
    load_session_hydration_snapshot_for_project_created_at,
};
pub use pages::{
    load_session_history_page_for_project_created_at,
    load_session_transcript_window_for_project_created_at,
};

use super::persistence::reject_historical_segmented_runtime_checkpoint;
#[cfg(test)]
use super::segmented_session_store::read_segmented_session_event_window;
use super::segmented_session_store::{
    ensure_runtime_log_from_segmented_context,
    find_segmented_session_directory_for_task_store_created_at,
    read_segmented_session_combined_turn_window, read_state, segmented_lines_to_session_messages,
};
use super::session_addon_store::read_session_addon_state;
use super::*;
use locator::{
    find_segmented_session_dir_for_locator, query_session_locator_for_project_created_at,
};
#[cfg(test)]
use runtime_state::apply_segmented_current_context;
use runtime_state::{
    load_segmented_runtime_state, resolve_initial_hydration_turn_count,
    restore_hydration_window_from_runtime_state_if_empty,
};
use types::{SegmentedWindowMessages, StoredSessionLocatorRow};
use windows::load_segmented_window_messages;

/// Minimum elapsed time that makes one session-loading stage worth reporting.
/// 会话加载阶段需要输出诊断日志的最小耗时。
const SESSION_LOAD_SLOW_TRACE_THRESHOLD_MS: u128 = 20;

/// Emits one unconditional slow-stage diagnostic for exact session loading.
/// 为精确会话加载输出一条不依赖环境开关的慢阶段诊断日志。
///
/// # Parameters
/// 参数
/// `stage` identifies the exact loading node being measured.
/// `stage` 标识正在测量的精确加载节点。
/// `session_id` identifies the owning session without exposing its filesystem path.
/// `session_id` 标识所属会话，同时避免暴露其文件系统路径。
/// `started_at` is the monotonic start time of the measured node.
/// `started_at` 是被测节点的单调起始时间。
/// `detail` carries compact result metadata such as limits and row counts.
/// `detail` 携带限制值与行数等紧凑结果元数据。
///
/// # Returns
/// 返回值
/// This function returns nothing and writes only stages exceeding the diagnostic threshold.
/// 此函数不返回数据，仅输出超过诊断阈值的阶段。
fn trace_session_load_if_slow(
    stage: &str,
    session_id: SessionId,
    started_at: std::time::Instant,
    detail: impl AsRef<str>,
) {
    // elapsed_ms is monotonic so clock adjustments cannot corrupt the measurement.
    // elapsed_ms 使用单调时钟，系统时钟调整不会破坏测量结果。
    let elapsed_ms = started_at.elapsed().as_millis();
    if elapsed_ms <= SESSION_LOAD_SLOW_TRACE_THRESHOLD_MS {
        return;
    }
    eprintln!(
        "[workspace-store][session-load-timing] session={session_id} stage={stage} elapsed_ms={elapsed_ms} {}",
        detail.as_ref()
    );
}
#[cfg(test)]
use windows::{
    merge_raw_timeline_window_into_boundary_window, read_segmented_session_boundary_turn_window,
    session_boundary_lines_to_window_messages,
};

#[cfg(test)]
mod tests;
