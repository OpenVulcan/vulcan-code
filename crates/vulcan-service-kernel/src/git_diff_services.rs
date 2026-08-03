//! git diff services helpers.
//! git diff services 辅助逻辑。

use super::*;
use crate::git::discover_webui_git_repository_catalog;
use crate::git_watcher::reconcile_webui_git_repository_watches;
use vulcan_workspace_store::workspace_store::{
    load_git_repository_monitoring_preferences, persist_git_repository_monitoring_preference,
};

/// Process-wide weak registry for serializing writes against one canonical Git repository.
/// 进程级弱引用注册表，用于串行化同一个规范 Git 仓库上的写操作。
static WEBUI_GIT_REPOSITORY_OPERATION_LOCKS: std::sync::LazyLock<
    std::sync::Mutex<
        std::collections::HashMap<std::path::PathBuf, std::sync::Weak<tokio::sync::Mutex<()>>>,
    >,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Acquires the exclusive write lane for one canonical Git repository.
/// 获取单个规范 Git 仓库的独占写操作通道。
///
/// # Parameters
/// 参数
/// `repository_root` is the backend-resolved repository root used as the lock identity.
/// `repository_root` 是后端解析出的仓库根目录，并作为锁身份。
///
/// # Returns
/// 返回值
/// Returns an owned asynchronous guard that remains held until the caller finishes its Git mutation.
/// 返回一个自有异步守卫，调用方完成 Git 变更前持续持有该守卫。
pub(crate) async fn begin_webui_git_repository_operation(
    repository_root: &std::path::Path,
) -> Result<tokio::sync::OwnedMutexGuard<()>, String> {
    let repository_root = std::fs::canonicalize(repository_root).map_err(|error| {
        format!(
            "无法规范化 Git 仓库写操作锁路径 `{}`：{error}",
            repository_root.display()
        )
    })?;
    let lock = {
        let mut registry = WEBUI_GIT_REPOSITORY_OPERATION_LOCKS
            .lock()
            .map_err(|error| format!("无法锁定 Git 仓库写操作注册表：{error}"))?;
        registry.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = registry
            .get(&repository_root)
            .and_then(std::sync::Weak::upgrade)
        {
            lock
        } else {
            let lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
            registry.insert(repository_root, std::sync::Arc::downgrade(&lock));
            lock
        }
    };
    Ok(lock.lock_owned().await)
}

/// Discovers Git repositories for one backend-owned WebUI project.
/// 为后端持有的单个 WebUI 项目发现 Git 仓库。
pub(crate) async fn service_workspace_git_catalog(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
) -> Result<WebuiGitRepositoryCatalog, String> {
    let preference_project_id = webui_diff_project_id_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    let mut catalog = spawn_service_blocking_task(move || {
        discover_webui_git_repository_catalog(std::path::Path::new(&resolved_workspace_path))
    })
    .await
    .map_err(|error| format!("workspace Git catalog task failed: {error}"))??;
    if let Some(project_id) = preference_project_id.as_deref() {
        let controller_runtime = crate::controller_runtime_from_state(&state)?;
        let preferences =
            load_git_repository_monitoring_preferences(controller_runtime, project_id).await?;
        apply_git_repository_monitoring_preferences(&mut catalog, &preferences);
    }
    Ok(catalog)
}

/// Discovers repositories and reconciles the enabled set with the notify-backed watcher.
/// 发现仓库并将已启用集合与基于 notify 的监听器进行重调和。
pub(crate) async fn service_workspace_git_catalog_with_watcher(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
) -> Result<WebuiGitRepositoryCatalog, String> {
    let preference_project_id = webui_diff_project_id_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?;
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    let catalog = service_workspace_git_catalog(
        state,
        Some(resolved_workspace_path.clone()),
        project_id,
        conversation_id,
    )
    .await?;
    reconcile_webui_git_repository_watches(
        app,
        preference_project_id.as_deref(),
        &resolved_workspace_path,
        &catalog,
    )?;
    Ok(catalog)
}

/// Persists one repository monitoring switch and returns the refreshed catalog.
/// 持久化单个仓库监听开关并返回刷新后的仓库目录。
pub(crate) async fn service_set_workspace_git_monitoring(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    request: WebuiGitRepositoryMonitoringRequest,
) -> Result<WebuiGitRepositoryCatalog, String> {
    let project_id = webui_diff_project_id_from_state(
        &state,
        project_id.as_deref(),
        conversation_id.as_deref(),
    )?
    .ok_or_else(|| String::from("缺少 Git 审查监听设置所属项目。"))?;
    let resolved_workspace_path = webui_diff_workspace_path_from_state(
        &state,
        Some(project_id.as_str()),
        conversation_id.as_deref(),
        workspace_path.as_deref(),
    )?;
    let catalog = spawn_service_blocking_task({
        let resolved_workspace_path = resolved_workspace_path.clone();
        move || {
            discover_webui_git_repository_catalog(std::path::Path::new(&resolved_workspace_path))
        }
    })
    .await
    .map_err(|error| format!("workspace Git catalog task failed: {error}"))??;
    if !catalog
        .repositories
        .iter()
        .any(|repository| repository.repository_key == request.repository_key)
    {
        return Err(format!(
            "未发现项目内 Git 仓库键 `{}`。",
            request.repository_key
        ));
    }
    let repository_root = crate::git::resolve_webui_repository_root_from_workspace_text_with_key(
        &resolved_workspace_path,
        Some(request.repository_key.as_str()),
    )?;
    // Preference writes share the repository mutation lane so a toggle cannot race an index or ref write.
    // 偏好写入与仓库变更共用同一条互斥通道，避免开关切换与索引或引用写入竞态。
    let operation_guard = begin_webui_git_repository_operation(&repository_root).await?;
    let controller_runtime = crate::controller_runtime_from_state(&state)?;
    persist_git_repository_monitoring_preference(
        controller_runtime,
        project_id.as_str(),
        &request.repository_key,
        request.enabled,
    )
    .await?;
    drop(operation_guard);
    service_workspace_git_catalog_with_watcher(
        app,
        state,
        Some(resolved_workspace_path),
        Some(project_id),
        conversation_id,
    )
    .await
}

/// Rejects Git mutations that target a repository whose review monitoring is disabled.
/// 拒绝指向已关闭审查监听仓库的 Git 写操作。
pub(crate) async fn ensure_webui_git_repository_monitoring_enabled(
    state: SharedServiceBridgeState,
    workspace_path: Option<String>,
    project_id: Option<String>,
    conversation_id: Option<String>,
    repository_key: Option<&str>,
) -> Result<(), String> {
    let catalog =
        service_workspace_git_catalog(state, workspace_path, project_id, conversation_id).await?;
    if catalog.repositories.is_empty() {
        return Ok(());
    }
    let target_key = repository_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| catalog.repositories[0].repository_key.as_str());
    let repository = catalog
        .repositories
        .iter()
        .find(|repository| repository.repository_key == target_key)
        .ok_or_else(|| format!("未发现项目内 Git 仓库键 `{target_key}`。"))?;
    if !repository.monitoring_enabled {
        return Err(format!(
            "Git 仓库 `{target_key}` 已关闭审查监听，无法执行写操作。"
        ));
    }
    Ok(())
}

/// Applies persisted monitoring flags to a discovered repository catalog.
/// 将已持久化监听标记应用到已发现的仓库目录。
fn apply_git_repository_monitoring_preferences(
    catalog: &mut WebuiGitRepositoryCatalog,
    preferences: &std::collections::BTreeMap<String, bool>,
) {
    for repository in &mut catalog.repositories {
        if let Some(enabled) = preferences.get(&repository.repository_key) {
            repository.monitoring_enabled = *enabled;
        }
    }
}

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
    let repository_key = request.repository_key.clone();
    let repository_root = spawn_service_blocking_task({
        let resolved_workspace_path = resolved_workspace_path.clone();
        move || {
            resolve_webui_repository_root_from_workspace_text_with_key(
                &resolved_workspace_path,
                repository_key.as_deref(),
            )
        }
    })
    .await
    .map_err(|error| format!("workspace Git repository resolution task failed: {error}"))??;
    // operation_guard keeps index and worktree mutations for one repository serial.
    // operation_guard 让同一仓库的索引与工作树变更保持串行。
    let _operation_guard = begin_webui_git_repository_operation(&repository_root).await?;
    ensure_webui_git_repository_monitoring_enabled(
        state,
        Some(resolved_workspace_path.clone()),
        project_id,
        conversation_id,
        request.repository_key.as_deref(),
    )
    .await?;
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

/// Resolves a project identity for Git preference reads without trusting a fallback path.
/// 解析 Git 偏好读取使用的项目身份，不信任回退路径推断身份。
pub(crate) fn webui_diff_project_id_from_state(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
) -> Result<Option<String>, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(workspace_snapshot) = guard.workspace_snapshot.as_ref() else {
        return Ok(None);
    };
    let candidate = project_id
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
    Ok(candidate.filter(|candidate| {
        workspace_task_project_for_project_id(workspace_snapshot, candidate).is_some()
    }))
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

    /// Verifies conversation-only catalog requests recover the owning project identity.
    /// 验证仅携带会话标识的目录请求可以恢复所属项目身份。
    #[test]
    fn webui_diff_project_id_resolves_from_conversation() {
        let state = service_state_with_workspace_snapshot(git_diff_workspace_snapshot());
        let session_id = {
            let guard = state.lock().expect("test service state lock must be valid");
            guard
                .workspace_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.sessions.first())
                .map(|session| session.id.to_string())
                .expect("test session should exist")
        };

        assert_eq!(
            webui_diff_project_id_from_state(&state, None, Some(&session_id))
                .expect("conversation project should resolve")
                .as_deref(),
            Some("project-a")
        );
    }

    /// Verifies writes targeting one canonical repository wait for the active operation guard.
    /// 验证指向同一规范仓库的写操作会等待当前活动操作守卫。
    #[tokio::test]
    async fn webui_git_repository_operation_lock_serializes_same_repository() {
        let temporary = tempfile::tempdir().expect("temporary directory should be created");
        let first_guard = begin_webui_git_repository_operation(temporary.path())
            .await
            .expect("first repository operation guard should be acquired");
        let repository_root = temporary.path().to_path_buf();
        let waiter =
            tokio::spawn(
                async move { begin_webui_git_repository_operation(&repository_root).await },
            );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), waiter)
                .await
                .is_err(),
            "a second operation should wait while the first guard is held"
        );
        drop(first_guard);
    }
}
