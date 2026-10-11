//! WebUI project directory browsing and project creation services.
//! WebUI 项目目录浏览与项目创建服务。

use std::{
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use vulcan_service_api::{
    ProjectDirectoryEntrySummary, ProjectDirectoryListing, ProjectDirectoryRootSummary,
    WorkbenchSnapshot,
};
use vulcan_workbench_model::TaskProjectSummary;
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use crate::project_conversation_services::begin_project_lifecycle_transition;
use crate::{
    SharedServiceBridgeState, controller_runtime_from_state, current_unix_millis_value,
    normalized_workspace_path_text_from_path, refresh_workbench_snapshot,
    service_add_project_initial_directory, webui_workspace_persist_lock,
    workspace_task_project_for_project_id_including_archived,
};

/// Returns the current Unix millisecond timestamp within the signed V1 lifecycle contract.
/// 返回符合有符号 V1 生命周期契约的当前 Unix 毫秒时间戳。
///
/// # Returns / 返回值
///
/// Returns the exact signed timestamp or an explicit range failure.
/// 返回精确的有符号时间戳，或显式的范围错误。
fn current_project_restore_unix_millis() -> Result<i64, String> {
    i64::try_from(current_unix_millis_value())
        .map_err(|_| String::from("Project restore timestamp exceeds the signed V1 range."))
}

/// Browses one host directory for the WebUI project folder picker.
/// 为 WebUI 项目文件夹选择器浏览一个宿主目录。
///
/// # Parameters
/// 参数
/// `state` provides the default initial project directory when `path` is omitted.
/// `state` 在省略 `path` 时提供默认初始项目目录。
/// `path` is the optional directory path requested by the WebUI.
/// `path` 是 WebUI 请求浏览的可选目录路径。
///
/// # Returns
/// 返回值
/// A directory listing containing child directories and the optional parent path.
/// 返回包含子目录与可选父目录路径的目录列表。
pub(super) fn service_browse_project_directories(
    state: SharedServiceBridgeState,
    path: Option<String>,
) -> Result<ProjectDirectoryListing, String> {
    let requested_path = match path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(service_add_project_initial_directory(&state)?),
    };
    let directory_path = normalize_task_project_path(&requested_path);
    if !directory_path.exists() {
        return Err(format!(
            "Directory does not exist: {}",
            normalized_workspace_path_text_from_path(&directory_path)
        ));
    }
    if !directory_path.is_dir() {
        return Err(format!(
            "The path is not a directory: {}",
            normalized_workspace_path_text_from_path(&directory_path)
        ));
    }

    let mut entries = fs::read_dir(&directory_path)
        .map_err(|error| {
            format!(
                "Failed to read directory `{}`: {error}",
                normalized_workspace_path_text_from_path(&directory_path)
            )
        })?
        .filter_map(|entry| project_directory_entry_from_read_dir_entry(entry.ok()?))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.hidden
            .cmp(&right.hidden)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(ProjectDirectoryListing {
        roots: service_project_directory_roots(&directory_path),
        path: normalized_workspace_path_text_from_path(&directory_path),
        parent_path: directory_path
            .parent()
            .map(normalized_workspace_path_text_from_path),
        can_select: true,
        entries,
        notice: None,
    })
}

/// Returns filesystem roots that the WebUI project picker can display.
/// 返回 WebUI 项目选择器可展示的文件系统根。
///
/// # Parameters
/// 参数
/// `current_path` is the currently browsed directory used as a fallback root source.
/// `current_path` 是当前浏览目录，用作兜底根目录来源。
///
/// # Returns
/// 返回值
/// A sorted list of service-host filesystem roots.
/// 已排序的服务宿主文件系统根列表。
fn service_project_directory_roots(current_path: &Path) -> Vec<ProjectDirectoryRootSummary> {
    let mut roots = platform_project_directory_roots();
    if let Some(current_root) = project_directory_root_from_path(current_path) {
        let current_root_key = current_root.path.to_ascii_lowercase();
        if !roots
            .iter()
            .any(|root| root.path.to_ascii_lowercase() == current_root_key)
        {
            roots.push(current_root);
        }
    }
    roots.sort_by(|left, right| {
        left.label
            .to_ascii_lowercase()
            .cmp(&right.label.to_ascii_lowercase())
            .then_with(|| left.label.cmp(&right.label))
    });
    roots
}

/// Returns platform filesystem roots visible to the service host.
/// 返回服务宿主当前平台可见的文件系统根。
///
/// # Returns
/// 返回值
/// A platform-specific list of root directories.
/// 平台相关的根目录列表。
#[cfg(windows)]
fn platform_project_directory_roots() -> Vec<ProjectDirectoryRootSummary> {
    (b'A'..=b'Z')
        .filter_map(|letter| {
            let drive = (letter as char).to_ascii_uppercase();
            let label = format!("{drive}:");
            let path = format!("{label}\\");
            if Path::new(&path).is_dir() {
                Some(ProjectDirectoryRootSummary {
                    label,
                    path: normalized_workspace_path_text_from_path(Path::new(&path)),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Returns platform filesystem roots visible to the service host.
/// 返回服务宿主当前平台可见的文件系统根。
///
/// # Returns
/// 返回值
/// A platform-specific list of root directories.
/// 平台相关的根目录列表。
#[cfg(not(windows))]
fn platform_project_directory_roots() -> Vec<ProjectDirectoryRootSummary> {
    vec![ProjectDirectoryRootSummary {
        label: String::from("/"),
        path: String::from("/"),
    }]
}

/// Resolves the filesystem root that owns one path.
/// 解析拥有指定路径的文件系统根。
///
/// # Parameters
/// 参数
/// `path` is the directory path whose root should be resolved.
/// `path` 是需要解析根目录的目录路径。
///
/// # Returns
/// 返回值
/// The owning root when it can be represented by the picker.
/// 当选择器可表示该根目录时返回所属根。
#[cfg(windows)]
fn project_directory_root_from_path(path: &Path) -> Option<ProjectDirectoryRootSummary> {
    for component in path.components() {
        let std::path::Component::Prefix(prefix_component) = component else {
            continue;
        };
        let drive = match prefix_component.kind() {
            std::path::Prefix::Disk(letter) | std::path::Prefix::VerbatimDisk(letter) => {
                (letter as char).to_ascii_uppercase()
            }
            _ => return None,
        };
        let label = format!("{drive}:");
        let root_path = format!("{label}\\");
        return Some(ProjectDirectoryRootSummary {
            label,
            path: normalized_workspace_path_text_from_path(Path::new(&root_path)),
        });
    }
    None
}

/// Resolves the filesystem root that owns one path.
/// 解析拥有指定路径的文件系统根。
///
/// # Parameters
/// 参数
/// `path` is the directory path whose root should be resolved.
/// `path` 是需要解析根目录的目录路径。
///
/// # Returns
/// 返回值
/// The owning root when it can be represented by the picker.
/// 当选择器可表示该根目录时返回所属根。
#[cfg(not(windows))]
fn project_directory_root_from_path(path: &Path) -> Option<ProjectDirectoryRootSummary> {
    if path.is_absolute() {
        Some(ProjectDirectoryRootSummary {
            label: String::from("/"),
            path: String::from("/"),
        })
    } else {
        None
    }
}

/// Converts one filesystem directory entry into a WebUI directory row.
/// 将单个文件系统目录项转换为 WebUI 目录行。
///
/// # Parameters
/// 参数
/// `entry` is the filesystem entry returned by `read_dir`.
/// `entry` 是 `read_dir` 返回的文件系统目录项。
///
/// # Returns
/// 返回值
/// A directory row when the entry is a child directory.
/// 当目录项为子目录时返回目录行。
fn project_directory_entry_from_read_dir_entry(
    entry: fs::DirEntry,
) -> Option<ProjectDirectoryEntrySummary> {
    let metadata = entry.metadata().ok()?;
    if !metadata.is_dir() {
        return None;
    }
    let path = normalize_task_project_path(&entry.path());
    let name = entry.file_name().to_string_lossy().to_string();
    Some(ProjectDirectoryEntrySummary {
        hidden: name.starts_with('.'),
        name,
        path: normalized_workspace_path_text_from_path(&path),
    })
}

/// Adds or restores one project from a concrete local directory path.
/// 根据一个具体本地目录路径新增或恢复项目。
///
/// # Parameters
/// 参数
/// `state` is the shared service state used to read runtime and workspace snapshots.
/// `state` 是用于读取运行时与工作区快照的共享服务状态。
/// `selected_path` is the local directory chosen by the shell or another trusted caller.
/// `selected_path` 是桌面壳或其他可信调用方选择的本地目录。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot when the selected path changed project state.
/// 当所选路径改变项目状态时返回刷新后的工作台快照。
pub async fn service_add_project_from_path(
    state: SharedServiceBridgeState,
    selected_path: PathBuf,
) -> Result<Option<WorkbenchSnapshot>, String> {
    let normalized_path = normalize_task_project_path(&selected_path);
    if !normalized_path.exists() {
        return Err(format!(
            "Project directory does not exist: {}",
            normalized_workspace_path_text_from_path(&normalized_path)
        ));
    }
    if !normalized_path.is_dir() {
        return Err(format!(
            "The project path is not a directory: {}",
            normalized_workspace_path_text_from_path(&normalized_path)
        ));
    }
    let normalized_path_text = normalized_workspace_path_text_from_path(&normalized_path);
    let controller_runtime = controller_runtime_from_state(&state)?;
    // persist_lock serializes project restoration and creation decisions.
    // persist_lock 串行化项目恢复与创建决策。
    let persist_lock = webui_workspace_persist_lock();
    // persist_guard protects the durable index decision and its resulting project-row mutation.
    // persist_guard 保护持久索引决策及其结果项目行变更。
    let persist_guard = persist_lock.lock().await;
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    // workspace_snapshot is reloaded from the durable index so a cancelled prior refresh cannot
    // make a persisted project look absent and cause a duplicate path or project identity.
    // workspace_snapshot 从持久索引重载，避免先前刷新被取消后把已落盘项目误判为不存在，
    // 从而产生重复路径或项目身份。
    let workspace_snapshot =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller_runtime.clone(),
        )
        .await?;

    if workspace_snapshot
        .task_projects
        .iter()
        .any(|project| task_project_matches_path(project, &normalized_path_text))
    {
        // refresh_result is computed while project-row decisions remain serialized.
        // refresh_result 在项目行决策保持串行时计算。
        let refresh_result = refresh_workbench_snapshot(state.clone()).await;
        drop(persist_guard);
        return refresh_result.map(Some);
    }

    if let Some(project) = workspace_snapshot
        .archived_task_projects
        .iter()
        .find(|project| task_project_matches_path(project, &normalized_path_text))
        .cloned()
    {
        // lifecycle_guard blocks new project/session admissions until every V1 projection commits.
        // lifecycle_guard 在每个 V1 投影提交前阻止新的项目/会话准入。
        let _lifecycle_guard = begin_project_lifecycle_transition(
            &state,
            &project.id,
            project.session_ids.iter().copied(),
            "restore",
        )?;
        let (vmm_controller_registry, session_registry) = {
            let guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            (
                guard.vmm_controller_registry.clone(),
                guard.session_runtime_registry.clone(),
            )
        };
        // vmm_lifecycle_fence blocks archived-history readers while authoritative rows transition.
        // vmm_lifecycle_fence 在权威行转换期间阻止归档历史读取器。
        let vmm_lifecycle_fence = vmm_controller_registry
            .begin_lifecycle_transition(project.session_ids.iter().copied())?;
        crate::session_v1_lifecycle::restore_project_sessions_lifecycle_v1(
            &controller_runtime,
            session_registry.as_ref(),
            &project.id,
            current_project_restore_unix_millis()?,
        )
        .await
        .map_err(|error| error.to_string())?;
        vulcan_workspace_store::workspace_store::restore_archived_project_row_v1(
            &controller_runtime,
            &project.id,
        )
        .await?;
        // workbench must commit restored membership before replacement controllers may open active paths.
        // workbench 必须先提交恢复后的成员关系，替代控制器才可打开活动路径。
        let workbench = refresh_workbench_snapshot(state.clone()).await?;
        vmm_lifecycle_fence.release()?;
        drop(persist_guard);
        return Ok(Some(workbench));
    }

    let title = normalized_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalized_path_text.clone());
    let order = workspace_snapshot
        .task_projects
        .iter()
        .filter(|project| !project.pinned)
        .map(|project| project.order)
        .max()
        .map(|order| order + 1)
        .unwrap_or(0);
    let project_id = build_task_project_id(&workspace_snapshot, &title, &normalized_path_text);
    let project = TaskProjectSummary::new(
        project_id,
        title,
        normalized_path_text,
        false,
        order,
        None,
        None,
        Vec::new(),
    );
    vulcan_workspace_store::workspace_store::persist_new_project(controller_runtime, &project)
        .await?;
    // The project row and its workbench membership must publish before releasing the workspace
    // writer lock. VMM binding may await a remote service and must not block unrelated projects.
    // 项目行及工作台成员关系必须在释放工作区写锁前发布；VMM 绑定可能等待远端服务，
    // 不应阻塞无关项目。
    // Keep only the publication outcome; the full snapshot can be large and is rebuilt after binding.
    // 仅保留发布结果；完整快照可能很大，并且绑定完成后会重新构建。
    let initial_refresh_result = refresh_workbench_snapshot(state.clone()).await.map(|_| ());
    if initial_refresh_result.is_err() {
        // A failed full projection must still retain the already-durable project membership.
        // Otherwise a concurrent settings writer could act on a stale in-memory project list.
        // 全量投影失败时仍需保留已落盘的项目成员关系，否则并发设置写入者可能依据过时的
        // 内存项目列表操作。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let retained = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !retained
            .task_projects
            .iter()
            .any(|candidate| candidate.id == project.id)
        {
            retained.task_projects.push(project.clone());
        }
    }
    drop(persist_guard);
    #[cfg(test)]
    tests::pause_before_project_binding_for_test().await;
    // Memory binding is intentionally non-fatal: the durable VulcanCode project remains created and
    // the response carries an explicit retry notice when VMM binding fails.
    // 记忆绑定有意采用非致命语义：VMM 绑定失败时，持久化的 VulcanCode 项目仍创建成功，响应携带明确重试通知。
    let project_binding_notice = crate::vmm::bind_new_vulcan_project_to_vmm(&state, &project).await;
    // Reacquire the writer lock before the final refresh so an intervening project publication
    // cannot be rolled back or omitted from the response after a slow VMM binding.
    // 最终刷新前重新获取写锁，避免缓慢的 VMM 绑定期间插入的项目发布被回滚或在响应中遗漏。
    let persist_guard = persist_lock.lock().await;
    let refresh_result = refresh_workbench_snapshot(state.clone())
        .await
        .map(|mut workbench| {
            workbench.vmm_memory.project_binding_notice = project_binding_notice;
            workbench
        });
    drop(persist_guard);
    match (initial_refresh_result, refresh_result) {
        (_, Ok(workbench)) => Ok(Some(workbench)),
        (Ok(_), Err(error)) => Err(error),
        (Err(initial_error), Err(final_error)) => Err(format!(
            "The project was created, but the first workbench publication failed: {initial_error}; retrying after binding still failed: {final_error}"
        )),
    }
}

/// Normalizes one project path by resolving it when the host filesystem allows it.
/// 在宿主文件系统允许时解析并规范化单个项目路径。
fn normalize_task_project_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Checks whether one project points to the normalized selected path.
/// 检查单个项目是否指向已规范化的选中路径。
fn task_project_matches_path(project: &TaskProjectSummary, normalized_path_text: &str) -> bool {
    normalized_workspace_path_text_from_path(&normalize_task_project_path(Path::new(&project.path)))
        == normalized_path_text
}

/// Builds a stable project id from the title and normalized path while avoiding collisions.
/// 根据标题与规范化路径构建稳定项目标识，并避免冲突。
fn build_task_project_id(
    workspace_snapshot: &WorkspaceSnapshot,
    title: &str,
    normalized_path_text: &str,
) -> String {
    let base_slug = slugify_identifier(title);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized_path_text.hash(&mut hasher);
    let path_suffix = format!("{:x}", hasher.finish());
    let path_suffix = &path_suffix[..path_suffix.len().min(8)];
    let candidate = if base_slug.is_empty() {
        format!("project-{path_suffix}")
    } else {
        format!("{base_slug}-{path_suffix}")
    };

    if !workspace_task_project_id_exists(workspace_snapshot, &candidate) {
        return candidate;
    }

    let mut counter = 2usize;
    loop {
        let next_candidate = format!("{candidate}-{counter}");
        if !workspace_task_project_id_exists(workspace_snapshot, &next_candidate) {
            return next_candidate;
        }
        counter += 1;
    }
}

/// Checks whether a project id already exists in visible or archived project lists.
/// 检查项目标识是否已经存在于可见或归档项目列表中。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the project lists used by project creation.
/// `workspace_snapshot` 持有项目创建时使用的项目列表。
/// `project_id` is the candidate project identifier.
/// `project_id` 是候选项目标识。
///
/// # Returns
/// 返回值
/// `true` when either visible or archived projects already use the id.
/// 当可见或归档项目已经使用该标识时返回 `true`。
fn workspace_task_project_id_exists(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: &str,
) -> bool {
    workspace_task_project_for_project_id_including_archived(workspace_snapshot, project_id)
        .is_some()
}

/// Converts one label into a compact ASCII identifier component.
/// 将单个标签转换为紧凑的 ASCII 标识片段。
fn slugify_identifier(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::{Arc, Mutex};

    use tokio::sync::Notify;

    use super::*;
    use crate::tests::test_support::{
        TestHostRuntimeRoot, test_agent_runtime_bridge, test_service_state_with_parent_session,
    };
    use vulcan_workbench_model::SessionId;

    tokio::task_local! {
        /// Holds a deterministic pause at the real project-binding boundary for one test task.
        /// 为单个测试任务在真实项目绑定边界保留确定性暂停点。
        static PROJECT_BINDING_PAUSE: Arc<ProjectBindingPause>;
    }

    /// Coordinates one delayed binding without changing any production VMM configuration.
    /// 在不修改生产 VMM 配置的情况下协调一次延迟绑定。
    #[derive(Default)]
    struct ProjectBindingPause {
        /// Signals that the first add request reached the binding boundary.
        /// 通知首个新增请求到达绑定边界。
        entered: Notify,
        /// Releases the first add request after the concurrent request has completed.
        /// 在并发请求完成后释放首个新增请求。
        release: Notify,
    }

    /// Pauses only the task that explicitly installed the binding test scope.
    /// 仅暂停明确安装绑定测试作用域的任务。
    ///
    /// # Returns
    /// 返回值
    /// Returns after the test releases the binding boundary, or immediately outside the scope.
    /// 测试释放绑定边界后返回；不在该作用域时立即返回。
    pub(super) async fn pause_before_project_binding_for_test() {
        if let Ok(pause) = PROJECT_BINDING_PAUSE.try_with(Arc::clone) {
            pause.entered.notify_one();
            pause.release.notified().await;
        }
    }

    /// Builds an empty workspace snapshot for project id generation tests.
    /// 构建用于项目标识生成测试的空工作区快照。
    ///
    /// # Returns
    /// 返回值
    /// A workspace snapshot with no visible or archived projects.
    /// 返回不包含可见或归档项目的工作区快照。
    fn empty_workspace_snapshot() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: SessionId::nil(),
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Builds one task project summary with only the id relevant to collision tests.
    /// 构建仅项目标识与冲突测试相关的任务项目摘要。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the summary id to store in the workspace snapshot.
    /// `project_id` 是写入工作区快照的摘要标识。
    ///
    /// # Returns
    /// 返回值
    /// A task project summary using the supplied id.
    /// 返回使用给定标识的任务项目摘要。
    fn task_project_with_id(project_id: &str) -> TaskProjectSummary {
        TaskProjectSummary::new(
            project_id,
            "Project",
            format!(r"D:\projects\{project_id}"),
            false,
            0,
            None,
            None,
            Vec::new(),
        )
    }

    /// Verifies generated project ids avoid visible and archived project collisions.
    /// 验证生成的项目标识会避开可见和归档项目冲突。
    #[test]
    fn build_task_project_id_avoids_visible_and_archived_collisions() {
        let title = "My Project";
        let normalized_path_text = r"D:\projects\my-project";
        let mut workspace_snapshot = empty_workspace_snapshot();
        let base_id = build_task_project_id(&workspace_snapshot, title, normalized_path_text);
        workspace_snapshot
            .task_projects
            .push(task_project_with_id(&base_id));
        workspace_snapshot
            .archived_task_projects
            .push(task_project_with_id(&format!("{base_id}-2")));

        let project_id = build_task_project_id(&workspace_snapshot, title, normalized_path_text);

        assert_eq!(format!("{base_id}-3"), project_id);
    }

    /// Verifies project id lookup treats archived project ids as occupied.
    /// 验证项目标识查找会把归档项目标识视为已占用。
    #[test]
    fn workspace_task_project_id_exists_checks_archived_projects() {
        let mut workspace_snapshot = empty_workspace_snapshot();
        workspace_snapshot
            .archived_task_projects
            .push(task_project_with_id("archived-project"));

        assert!(workspace_task_project_id_exists(
            &workspace_snapshot,
            "archived-project"
        ));
        assert!(!workspace_task_project_id_exists(
            &workspace_snapshot,
            "missing-project"
        ));
    }

    /// Verifies a slow VMM binding cannot block a second real project create or hide its result.
    /// 验证缓慢的 VMM 绑定不会阻塞第二次真实项目创建，也不会隐藏其结果。
    #[tokio::test]
    async fn project_binding_wait_releases_workspace_writer_and_preserves_both_projects() {
        // root owns the isolated V1 and APP storage used by the production project path.
        // root 持有生产项目入口使用的隔离 V1 与 APP 存储。
        let session_id = SessionId::new_v4();
        let root = TestHostRuntimeRoot::new(session_id);
        let controller = root.attached_host_runtime_handle().await;
        vulcan_workspace_store::data_plane::prepare_application_schema_for_test(&controller)
            .await
            .expect("isolated application schema");
        // existing_project makes the indexed session's owner resolvable during both refreshes.
        // existing_project 使索引会话的所属项目在两次刷新期间都可解析。
        let existing_path = root.data_root().join("existing-project");
        std::fs::create_dir_all(&existing_path).expect("existing project directory");
        let existing_project = TaskProjectSummary::new(
            "v1-test-project",
            "Existing project",
            existing_path.display().to_string(),
            false,
            0,
            None,
            None,
            Vec::new(),
        );
        vulcan_workspace_store::workspace_store::persist_new_project(
            controller.clone(),
            &existing_project,
        )
        .await
        .expect("persist indexed session owner");
        // workspace comes from the same durable index used by the service, not an invented list.
        // workspace 来自服务使用的同一持久索引，而不是编造的列表。
        let workspace = vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller.clone(),
        )
        .await
        .expect("load initial project index");
        // initial_workspace later simulates a retained projection from before both durable adds.
        // initial_workspace 稍后模拟两次持久新增之前保留的过时投影。
        let initial_workspace = workspace.clone();
        let mut service = test_service_state_with_parent_session(session_id);
        // A deliberately unavailable native manager exercises the real non-fatal binding notice.
        // 故意缺席的原生管理器用于验收真实的非致命绑定通知。
        let mut agent = test_agent_runtime_bridge();
        agent.provider_configuration.vmm.enabled = Some(true);
        agent.provider_configuration.vmm.backend_mode = Some(vulcan_vmm::VmmBackendMode::Native);
        service.agent_runtime = Some(agent);
        service.controller_runtime = Some(controller.clone());
        service.workspace_snapshot = Some(workspace);
        let state = Arc::new(Mutex::new(service));
        // first_path and second_path are distinct real directories beneath the private root.
        // first_path 与 second_path 是私有根目录下两个不同的真实目录。
        let first_path = root.data_root().join("first-project");
        let second_path = root.data_root().join("second-project");
        std::fs::create_dir_all(&first_path).expect("first project directory");
        std::fs::create_dir_all(&second_path).expect("second project directory");
        // Only the first request pauses; a JoinSet aborts it if an assertion fails.
        // 仅首个请求暂停；断言失败时 JoinSet 会中止该请求。
        let pause = Arc::new(ProjectBindingPause::default());
        let mut pending = tokio::task::JoinSet::new();
        pending.spawn(PROJECT_BINDING_PAUSE.scope(
            pause.clone(),
            service_add_project_from_path(state.clone(), first_path.clone()),
        ));
        tokio::time::timeout(std::time::Duration::from_secs(30), pause.entered.notified())
            .await
            .expect("first project reaches VMM binding boundary");
        // The first project is already visible before its VMM binding may finish.
        // 首个项目在其 VMM 绑定完成前已经可见。
        let first_published = state
            .lock()
            .expect("retained workspace state")
            .workspace_snapshot
            .as_ref()
            .expect("published workspace")
            .task_projects
            .iter()
            .any(|project| task_project_matches_path(project, &first_path.display().to_string()));
        assert!(first_published);
        // The second real service call must finish while the first binder remains paused.
        // 首个绑定仍暂停时，第二次真实服务调用必须完成。
        let second_response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            service_add_project_from_path(state.clone(), second_path.clone()),
        )
        .await
        .expect("second create must not wait for first VMM binding")
        .expect("second create")
        .expect("second workbench");
        assert_eq!(second_response.projects.len(), 3);
        assert_eq!(
            second_response
                .vmm_memory
                .project_binding_notice
                .as_ref()
                .map(|notice| notice.status.as_str()),
            Some("failed")
        );
        pause.release.notify_one();
        let first_response = pending
            .join_next()
            .await
            .expect("first project task")
            .expect("first project task must not panic")
            .expect("first create")
            .expect("first workbench");
        assert_eq!(first_response.projects.len(), 3);
        assert_eq!(
            first_response
                .vmm_memory
                .project_binding_notice
                .as_ref()
                .map(|notice| notice.status.as_str()),
            Some("failed")
        );
        // A stale retained projection must not make an already persisted path create a new row.
        // 过时的保留投影不能让已持久化路径再次创建新项目行。
        state
            .lock()
            .expect("retained workspace state")
            .workspace_snapshot = Some(initial_workspace);
        let retry_response = service_add_project_from_path(state.clone(), first_path.clone())
            .await
            .expect("retry known durable project")
            .expect("retry workbench");
        assert_eq!(retry_response.projects.len(), 3);
        // Read the durable index again to prove both projects survived publication interleaving.
        // 再次读取持久索引，证明两个项目在发布交错后均已保留。
        let durable =
            vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(controller)
                .await
                .expect("load final project index");
        assert_eq!(durable.task_projects.len(), 3);
        assert!(
            durable
                .task_projects
                .iter()
                .any(|project| task_project_matches_path(
                    project,
                    &first_path.display().to_string()
                ))
        );
        assert!(
            durable
                .task_projects
                .iter()
                .any(|project| task_project_matches_path(
                    project,
                    &second_path.display().to_string()
                ))
        );
    }
}
