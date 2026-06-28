//! git diff services helpers.
//! git diff services 辅助逻辑。

use super::*;

/// Loads a Git diff snapshot for one WebUI project workspace.
/// 为单个 WebUI 项目工作区加载 Git diff 快照。
pub(crate) async fn service_workspace_diff_snapshot(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    request: Option<WebuiWorkspaceDiffRequest>,
) -> Result<WebuiWorkspaceDiffSnapshot, String> {
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    spawn_service_blocking_task(move || {
        load_webui_workspace_diff_snapshot(&resolved_workspace_path, request)
    })
    .await
    .map_err(|error| format!("workspace diff task failed: {error}"))?
}

/// Loads a Git diff detail for one WebUI project file.
/// 为单个 WebUI 项目文件加载 Git diff 详情。
pub(crate) async fn service_workspace_diff_file_detail(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    request: Option<WebuiWorkspaceDiffRequest>,
    file: WebuiWorkspaceDiffFileDetailRequest,
) -> Result<WebuiWorkspaceDiffFileDetail, String> {
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    spawn_service_blocking_task(move || {
        load_webui_workspace_diff_file_detail(&resolved_workspace_path, request, file)
    })
    .await
    .map_err(|error| format!("workspace diff file detail task failed: {error}"))?
}

/// Searches the complete backend Git diff and returns compact file-level hits.
/// 搜索后端完整 Git diff 并返回紧凑的文件级命中。
pub(crate) async fn service_workspace_diff_search(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    request: Option<WebuiWorkspaceDiffRequest>,
    search: WebuiWorkspaceDiffSearchRequest,
) -> Result<WebuiWorkspaceDiffSearchResponse, String> {
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    spawn_service_blocking_task(move || {
        load_webui_workspace_diff_search(&resolved_workspace_path, request, search)
    })
    .await
    .map_err(|error| format!("workspace diff search task failed: {error}"))?
}

/// Executes one file-level Git action for a WebUI workspace.
/// 为 WebUI 工作区执行单个文件级 Git 操作。
pub(crate) async fn service_workspace_git_file_action(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    request: WebuiWorkspaceGitFileActionRequest,
) -> Result<WebuiWorkspaceGitFileActionResult, String> {
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    spawn_service_blocking_task(move || {
        run_webui_workspace_git_file_action(&resolved_workspace_path, request)
    })
    .await
    .map_err(|error| format!("workspace Git file action task failed: {error}"))?
}

/// Resolves the workspace path for a WebUI diff request from backend-owned state first.
/// 优先从后端持有的状态中解析 WebUI diff 请求对应的工作区路径。
pub(crate) fn webui_diff_workspace_path_from_state(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
    fallback_path: Option<&str>,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
        return Ok(fallback_path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root())));
    };
    let resolved_project_id = project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            conversation_id
                .and_then(parse_session_id)
                .and_then(|session_id| {
                    owning_project_id_for_session(workspace_snapshot, session_id)
                })
        });
    if let Some(resolved_project_id) = resolved_project_id
        && let Some(project) = workspace_snapshot
            .task_projects
            .iter()
            .find(|project| project.id == resolved_project_id)
    {
        return Ok(project.path.clone());
    }
    Ok(fallback_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root())))
}
