//! Segmented session locator resolution helpers.
//! 分段会话定位解析辅助逻辑。

use super::*;

/// Resolves one segmented session locator by its deterministic project, created-at, and session id.
/// 通过确定性的项目、创建时间与会话标识解析单个分段会话定位信息。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known segmented project tag for this session.
/// `project_id` 是该会话已知的分段项目标签。
/// `created_at` is the raw Unix-milliseconds creation timestamp from the session summary.
/// `created_at` 是会话摘要中的原始 Unix 毫秒创建时间戳。
/// `session_id` selects the session directory.
/// `session_id` 选择会话目录。
///
/// # Returns
/// 返回值
/// The locator when the deterministic active or archived directory exists.
/// 当确定性的活动或归档目录存在时返回定位信息。
pub(super) async fn query_session_locator_for_project_created_at(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    created_at: &str,
    session_id: SessionId,
) -> Result<Option<StoredSessionLocatorRow>, String> {
    let Some(session_dir) = find_segmented_session_directory_for_task_store_created_at(
        &controller.data_root,
        project_id,
        created_at,
        &session_id.to_string(),
    ) else {
        return Ok(None);
    };
    Ok(Some(StoredSessionLocatorRow {
        session_dir,
        latest_turn_index: -1,
    }))
}

/// Resolves the segmented session directory for an active or archived session locator.
/// 根据活动或归档会话定位信息解析分段会话目录。
pub(super) fn find_segmented_session_dir_for_locator(
    controller: &ControllerRuntimeHandle,
    locator: &StoredSessionLocatorRow,
    session_id: SessionId,
) -> Result<Option<std::path::PathBuf>, String> {
    let _ = (controller, session_id);
    Ok(Some(locator.session_dir.clone()))
}

/// Returns no controller runtime document because segmented state files own hydration state.
/// 不返回控制器运行态文档，因为分段 state 文件拥有水合状态。
pub(super) async fn query_session_runtime_state_row(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<StoredRuntimeStateRow>, String> {
    let _ = (controller, session_id);
    Ok(None)
}
