//! Segmented session locator resolution helpers.
//! 分段会话定位解析辅助逻辑。

use super::*;
/// Finds the lightweight session locator needed to open one segmented session directory.
/// 查找打开单个分段会话目录所需的轻量会话定位信息。
pub(super) async fn query_session_locator(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<StoredSessionLocatorRow>, String> {
    Ok(
        find_segmented_session_directory_by_id(&controller.data_root, &session_id.to_string())?
            .map(|session_dir| StoredSessionLocatorRow {
                session_dir,
                latest_turn_index: -1,
            }),
    )
}

/// Resolves one segmented session locator by checking only the known owning project.
/// 仅检查已知所属项目来解析单个分段会话定位信息。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace-store runtime bridge.
/// `controller` 持有工作区存储运行时桥。
/// `project_id` is the known segmented project tag for this session.
/// `project_id` 是该会话已知的分段项目标签。
/// `session_id` selects the session directory suffix.
/// `session_id` 选择会话目录后缀。
///
/// # Returns
/// 返回值
/// The locator when a matching active or archived segmented session directory exists.
/// 当活动或归档分段会话目录存在匹配项时返回定位信息。
pub(super) async fn query_session_locator_for_project(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    session_id: SessionId,
) -> Result<Option<StoredSessionLocatorRow>, String> {
    let session_id_text = session_id.to_string();
    let active_dir = find_segmented_session_directory(
        &controller.data_root,
        false,
        project_id,
        &session_id_text,
    )?;
    let session_dir = match active_dir {
        Some(session_dir) => Some(session_dir),
        None => find_segmented_session_directory(
            &controller.data_root,
            true,
            project_id,
            &session_id_text,
        )?,
    };
    Ok(session_dir.map(|session_dir| StoredSessionLocatorRow {
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
