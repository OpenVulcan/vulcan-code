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
        return Ok(webui_diff_fallback_workspace_path(fallback_path));
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
        && let Some(project) =
            workspace_task_project_for_project_id(workspace_snapshot, &resolved_project_id)
    {
        return Ok(project.path.clone());
    }
    Ok(webui_diff_fallback_workspace_path(fallback_path))
}

/// Returns the fallback workspace path used when backend project resolution misses.
/// 返回后端项目解析未命中时使用的回退工作区路径。
///
/// # Parameters
/// 参数
/// `fallback_path` is the optional path supplied by the WebUI request.
/// `fallback_path` 是 WebUI 请求提供的可选路径。
///
/// # Returns
/// 返回值
/// A trimmed fallback path or the normalized service workspace root.
/// 返回修剪后的回退路径，或规范化后的服务工作区根目录。
fn webui_diff_fallback_workspace_path(fallback_path: Option<&str>) -> String {
    fallback_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root()))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use super::*;
    use vulcan_workbench_model::{SessionId, SessionState, SessionSummary, TaskProjectSummary};
    use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

    /// Test shell host used by Git diff path-resolution tests.
    /// Git diff 路径解析测试使用的测试宿主壳。
    struct GitDiffServiceTestShellHost;

    impl ServiceShellHost for GitDiffServiceTestShellHost {
        /// Rejects host file-manager use in Git diff path tests.
        /// 在 Git diff 路径测试中拒绝使用宿主文件管理器。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Err(String::from("Git diff tests do not open host paths"))
        }

        /// Rejects host browser use in Git diff path tests.
        /// 在 Git diff 路径测试中拒绝使用宿主浏览器。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Err(String::from("Git diff tests do not open host URLs"))
        }
    }

    /// Builds service state containing one workspace snapshot.
    /// 构建包含单个工作区快照的服务状态。
    ///
    /// # Parameters
    /// 参数
    /// `workspace_snapshot` is the retained workspace snapshot.
    /// `workspace_snapshot` 是保留的工作区快照。
    ///
    /// # Returns
    /// 返回值
    /// Shared service state containing the supplied workspace snapshot.
    /// 返回包含给定工作区快照的共享服务状态。
    fn service_state_with_workspace_snapshot(
        workspace_snapshot: WorkspaceSnapshot,
    ) -> SharedServiceBridgeState {
        // state stores the minimal bridge state needed by Git diff path resolution.
        // state 保存 Git diff 路径解析所需的最小 bridge 状态。
        let state = create_service_bridge_state(Arc::new(GitDiffServiceTestShellHost));
        // guard installs the retained workspace snapshot for the resolver.
        // guard 为解析器安装保留工作区快照。
        let mut guard = state.lock().expect("test service state lock must be valid");
        guard.workspace_snapshot = Some(workspace_snapshot);
        drop(guard);
        state
    }

    /// Builds one workspace snapshot for Git diff project path tests.
    /// 构建用于 Git diff 项目路径测试的工作区快照。
    ///
    /// # Returns
    /// 返回值
    /// A workspace snapshot with one valid project and one blank-id project.
    /// 返回包含一个有效项目与一个空白标识项目的工作区快照。
    fn git_diff_workspace_snapshot() -> WorkspaceSnapshot {
        // session_id identifies the retained conversation row in the snapshot.
        // session_id 标识快照中的保留会话行。
        let session_id = SessionId::new_v4();
        WorkspaceSnapshot {
            task_projects: vec![
                TaskProjectSummary::new(
                    "project-a",
                    "Project A",
                    "D:/project-a",
                    false,
                    0,
                    None,
                    None,
                    vec![session_id],
                ),
                TaskProjectSummary::new(
                    "   ",
                    "Blank Project",
                    "D:/blank-project",
                    false,
                    0,
                    None,
                    None,
                    Vec::new(),
                ),
            ],
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "Git diff 测试会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies explicit project ids resolve to backend-owned project paths.
    /// 验证明示项目标识会解析为后端持有的项目路径。
    #[test]
    fn webui_diff_workspace_path_resolves_explicit_project_id() {
        // state owns the workspace snapshot used by the path resolver.
        // state 持有路径解析器使用的工作区快照。
        let state = service_state_with_workspace_snapshot(git_diff_workspace_snapshot());

        assert_eq!(
            webui_diff_workspace_path_from_state(
                &state,
                Some(" project-a "),
                None,
                Some("D:/fallback"),
            )
            .expect("explicit project should resolve"),
            "D:/project-a"
        );
    }

    /// Verifies blank project ids do not match blank project nodes.
    /// 验证空白项目标识不会命中空白项目节点。
    #[test]
    fn webui_diff_workspace_path_uses_fallback_for_blank_project_id() {
        // state owns a workspace with a blank-id project that must not be selected.
        // state 持有一个不应被选中的空白标识项目。
        let state = service_state_with_workspace_snapshot(git_diff_workspace_snapshot());

        assert_eq!(
            webui_diff_workspace_path_from_state(&state, Some("   "), None, Some(" D:/fallback "))
                .expect("blank project should resolve through fallback"),
            "D:/fallback"
        );
    }
}
