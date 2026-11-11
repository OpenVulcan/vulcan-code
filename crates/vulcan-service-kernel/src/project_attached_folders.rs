//! WebUI management and Git discovery for explicit project folder associations.
//! WebUI 显式项目目录关联管理与 Git 发现。

use std::path::Path;

use vulcan_workspace_store::workspace_store::{
    load_project_attached_folders, set_project_attached_folder,
};

use crate::git::{WebuiGitRepositoryCatalog, discover_webui_git_repository_catalog};
use crate::workspace_files::workspace_file_canonical_root;
use crate::{
    SharedServiceBridgeState, controller_runtime_from_state,
    normalized_workspace_path_text_from_path, spawn_service_blocking_task,
    workspace_task_project_for_project_id,
};

/// Resolves the active `project_id` in `state` to its registered primary directory.
/// 在 `state` 中将活动的 `project_id` 解析为其已注册的主目录。
fn active_project_path(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<String, String> {
    // Use the active project registry so an archived or missing project cannot be edited.
    // 以活动项目登记为准，禁止编辑已归档或缺失的项目。
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?;
    guard
        .workspace_snapshot
        .as_ref()
        .and_then(|snapshot| workspace_task_project_for_project_id(snapshot, project_id))
        .map(|project| project.path.clone())
        .ok_or_else(|| format!("Active project `{project_id}` was not found."))
}

/// Returns persisted folder paths for the active `project_id` registered in `state`.
/// 返回 `state` 中活动项目 `project_id` 持有的已持久化目录路径。
pub(crate) async fn service_list_project_attached_folders(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<Vec<String>, String> {
    active_project_path(&state, &project_id)?;
    load_project_attached_folders(controller_runtime_from_state(&state)?, &project_id).await
}

/// Updates one `path` for the given `project_id` in `state` and returns the persistence result.
/// 更新 `state` 中 `project_id` 的一条 `path` 并返回持久化结果。
/// `attached` selects registration or removal. Removal accepts a saved path even if it no longer exists.
/// `attached` 指定添加或移除。移除支持已不存在的持久化路径。
pub(crate) async fn service_set_project_attached_folder(
    state: SharedServiceBridgeState,
    project_id: String,
    path: String,
    attached: bool,
) -> Result<(), String> {
    // Validate the exact project before using its registered root for duplicate checks.
    // 先校验项目身份，再用已注册的主目录检查重复。
    let primary = active_project_path(&state, &project_id)?;
    // Validate additions on the host so aliases are persisted under one canonical identity.
    // 在宿主校验新增路径，使同一目录的别名共用一个规范化标识。
    let path = if attached {
        spawn_service_blocking_task(move || {
            if !Path::new(&path).is_absolute() {
                return Err(String::from(
                    "An attached folder must have an absolute path.",
                ));
            }
            // Compare the real directories so the primary cannot be attached through an alias.
            // 比较实际目录，防止通过别名添加项目主目录。
            let directory = workspace_file_canonical_root(&path)?;
            if workspace_file_canonical_root(&primary)? == directory {
                return Err(String::from(
                    "The primary project folder is already included.",
                ));
            }
            Ok(normalized_workspace_path_text_from_path(&directory))
        })
        .await
        .map_err(|error| format!("Attached folder validation failed: {error}"))??
    } else {
        path
    };
    set_project_attached_folder(
        controller_runtime_from_state(&state)?,
        &project_id,
        &path,
        attached,
    )
    .await
}

/// Detects Git repositories beneath the registered `path` of `project_id` in `state`.
/// 检测 `state` 中 `project_id` 已登记的 `path` 下的 Git 仓库。
/// Returns the existing Git catalog for that directory and propagates discovery errors.
/// 返回该目录的现有 Git 目录并传递发现错误。
pub(crate) async fn service_detect_project_attached_folder_git(
    state: SharedServiceBridgeState,
    project_id: String,
    path: String,
) -> Result<WebuiGitRepositoryCatalog, String> {
    if !service_list_project_attached_folders(state, project_id)
        .await?
        .contains(&path)
    {
        return Err(String::from("The folder is not attached to this project."));
    }
    spawn_service_blocking_task(move || discover_webui_git_repository_catalog(Path::new(&path)))
        .await
        .map_err(|error| format!("Attached folder Git discovery failed: {error}"))?
}
