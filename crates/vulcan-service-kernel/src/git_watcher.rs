//! Notify-backed Git repository review watcher.
//! 基于 notify 的 Git 仓库审查文件监听器。

use super::*;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use vulcan_file_watcher::live::{
    LiveFileWatcherSubscriber, LiveWatchRegistration, ThrottledLiveWatchReceiver,
};

/// Process-lifetime ownership for the shared Git watcher backend.
/// 进程级共享 Git 监听后端的生命周期所有权。
static WEBUI_GIT_WATCHER: OnceLock<Arc<WebuiGitWatcherManager>> = OnceLock::new();

/// Identifies one repository target exposed by a watched path.
/// 标识一个监听路径对应的仓库目标。
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct GitWatchTarget {
    /// Optional persisted project identity owning this repository.
    /// 可选的持久化项目标识。
    project_id: Option<String>,
    /// Canonical workspace path used by the WebUI request.
    /// WebUI 请求使用的规范化工作区路径。
    workspace_path: PathBuf,
    /// Stable project-relative repository key.
    /// 稳定的项目相对仓库键。
    repository_key: String,
}

/// Owns registrations and the event task for all active WebUI Git repositories.
/// 持有所有活跃 WebUI Git 仓库的监听注册与事件任务。
struct WebuiGitWatcherManager {
    /// One logical subscriber owning the manager registrations.
    /// 持有管理器全部注册项的逻辑订阅者。
    _subscriber: LiveFileWatcherSubscriber,
    /// RAII registrations keyed by canonical repository root.
    /// 以规范化仓库根目录为键的 RAII 注册项。
    registrations: std::sync::Mutex<HashMap<PathBuf, LiveWatchRegistration>>,
    /// Current watched targets read by the background event task.
    /// 后台事件任务读取的当前监听目标。
    targets: Arc<RwLock<BTreeMap<PathBuf, BTreeSet<GitWatchTarget>>>>,
    /// Background event-consumer task retained for the process lifetime.
    /// 进程生命周期内保留的后台事件消费任务。
    _task: ServiceTaskHandle<()>,
}

/// Reconciles one project's enabled repository set with the shared notify watcher.
/// 将一个项目已启用仓库集合与共享 notify 监听器进行重调和。
///
/// # Parameters
/// 参数
/// `event_emitter` forwards change notifications to connected WebUI clients.
/// `event_emitter` 将变更通知转发给已连接的 WebUI 客户端。
/// `project_id` identifies the persisted project when one is available.
/// `project_id` 标识可用的持久化项目。
/// `workspace_path` is the exact workspace path used for repository resolution.
/// `workspace_path` 是用于仓库解析的精确工作区路径。
/// `catalog` contains the repositories and their persisted monitoring flags.
/// `catalog` 包含仓库及其持久化监听开关。
///
/// # Returns
/// 返回值
/// Returns an error when the shared filesystem watcher cannot be initialized or a repository root cannot be resolved.
/// 当共享文件监听器无法初始化或仓库根目录无法解析时返回错误。
pub(crate) fn reconcile_webui_git_repository_watches(
    event_emitter: ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
    workspace_path: &str,
    catalog: &WebuiGitRepositoryCatalog,
) -> Result<(), String> {
    let workspace_path = std::fs::canonicalize(workspace_path).map_err(|error| {
        format!("Cannot normalize the Git watcher workspace `{workspace_path}`: {error}")
    })?;
    let watcher_center = state
        .lock()
        .map_err(|_| {
            String::from("The service state was locked while reading the shared workspace watcher.")
        })?
        .workspace_watcher
        .clone()
        .ok_or_else(|| String::from("The shared workspace watcher is not initialized yet."))?;
    let manager = shared_webui_git_watcher(event_emitter, &watcher_center)?;
    let owner_project_id = project_id.map(str::to_owned);
    let mut desired_targets: BTreeMap<PathBuf, BTreeSet<GitWatchTarget>> = BTreeMap::new();
    for repository in catalog
        .repositories
        .iter()
        .filter(|repository| repository.monitoring_enabled)
    {
        let repository_root =
            crate::git::resolve_webui_repository_root_from_workspace_text_with_key(
                workspace_path.to_string_lossy().as_ref(),
                Some(repository.repository_key.as_str()),
            )?;
        desired_targets
            .entry(repository_root)
            .or_default()
            .insert(GitWatchTarget {
                project_id: owner_project_id.clone(),
                workspace_path: workspace_path.clone(),
                repository_key: repository.repository_key.clone(),
            });
    }
    manager.reconcile_project(owner_project_id, workspace_path, desired_targets);
    Ok(())
}

/// Creates or returns the process-wide notify watcher and its event task.
/// 创建或返回进程级 notify 监听器及其事件任务。
fn shared_webui_git_watcher(
    event_emitter: ServiceEventEmitter,
    watcher_center: &crate::workspace_watcher::WorkspaceWatcherCenter,
) -> Result<Arc<WebuiGitWatcherManager>, String> {
    if let Some(manager) = WEBUI_GIT_WATCHER.get() {
        return Ok(Arc::clone(manager));
    }

    let (subscriber, receiver) = watcher_center.subscribe();
    let targets = Arc::new(RwLock::new(BTreeMap::new()));
    let event_targets = Arc::clone(&targets);
    let task = spawn_service_task(async move {
        let mut receiver = ThrottledLiveWatchReceiver::new(receiver, Duration::from_millis(250));
        let mut ignore_matchers = HashMap::new();
        while let Some(event) = receiver.recv().await {
            let grouped =
                group_webui_git_watch_events(&event_targets, &mut ignore_matchers, &event.paths);
            for ((project_id, workspace_path), repository_keys) in grouped {
                event_emitter.emit(ServiceEvent::WorkspaceGitChanged(Box::new(
                    workspace_git_changed_event(project_id, &workspace_path, repository_keys),
                )));
            }
        }
    });
    let manager = Arc::new(WebuiGitWatcherManager {
        _subscriber: subscriber,
        registrations: std::sync::Mutex::new(HashMap::new()),
        targets,
        _task: task,
    });
    match WEBUI_GIT_WATCHER.set(Arc::clone(&manager)) {
        Ok(()) => Ok(manager),
        Err(_) => {
            Ok(Arc::clone(WEBUI_GIT_WATCHER.get().expect(
                "Git watcher manager is initialized after a concurrent set",
            )))
        }
    }
}

impl WebuiGitWatcherManager {
    /// Replaces one project's watched repository targets without affecting other projects.
    /// 替换一个项目的监听仓库目标，同时不影响其他项目。
    fn reconcile_project(
        &self,
        project_id: Option<String>,
        workspace_path: PathBuf,
        desired_targets: BTreeMap<PathBuf, BTreeSet<GitWatchTarget>>,
    ) {
        let mut targets = self
            .targets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for project_targets in targets.values_mut() {
            project_targets.retain(|target| {
                !(target.project_id == project_id && target.workspace_path == workspace_path)
            });
        }
        targets.retain(|_, project_targets| !project_targets.is_empty());
        for (repository_root, repository_targets) in desired_targets {
            targets
                .entry(repository_root)
                .or_default()
                .extend(repository_targets);
        }

        let mut registrations = self
            .registrations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registrations.retain(|repository_root, _| targets.contains_key(repository_root));
        for repository_root in targets.keys() {
            if registrations.contains_key(repository_root) {
                continue;
            }
            let registration = self
                ._subscriber
                .register_path(repository_root.clone(), true);
            registrations.insert(repository_root.clone(), registration);
        }
    }
}

/// Builds the change notification for one watched workspace.
/// 为一个被监听的工作区构建变更通知。
///
/// # Parameters
/// 参数
/// `workspace_path` is the canonical path the watcher keys targets by; on Windows it carries the `\\?\`
/// prefix, which clients never use, so the event names the workspace in host path form instead.
/// `workspace_path` 是监听器用作目标键的规范路径；在 Windows 上它带有客户端从不使用的 `\\?\` 前缀，
/// 因此事件改用宿主路径形式指明工作区。
///
/// # Returns
/// 返回值
/// The event that WebUI and TUI clients match against the workspace path they opened.
/// 返回 WebUI 与 TUI 客户端用其打开的工作区路径进行匹配的事件。
fn workspace_git_changed_event(
    project_id: Option<String>,
    workspace_path: &Path,
    repository_keys: BTreeSet<String>,
) -> WorkspaceGitChangedEvent {
    WorkspaceGitChangedEvent {
        project_id,
        workspace_path: vulcan_runtime_assets::native_host_path_text_from_path(workspace_path),
        repository_keys: repository_keys.into_iter().collect(),
    }
}

/// Git-internal directories whose churn never changes the reviewable repository state by itself.
/// 其变化本身不会改变可审查仓库状态的 Git 内部目录。
///
/// Object and reflog writes always accompany an `index`, `HEAD` or `refs` update that still notifies.
/// 对象与 reflog 写入总是伴随仍会通知的 `index`、`HEAD` 或 `refs` 更新。
const GIT_WATCH_SILENT_DIRECTORIES: [&str; 2] = ["objects", "logs"];

/// Builds the ignore matcher from one repository's root `.gitignore` and `.git/info/exclude`.
/// 根据一个仓库根目录的 `.gitignore` 与 `.git/info/exclude` 构建忽略匹配器。
///
/// Unreadable files and invalid lines only narrow filtering, so affected paths keep notifying.
/// 无法读取的文件与无效规则只会缩小过滤范围，受影响路径仍会照常通知。
fn build_repository_ignore_matcher(repository_root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(repository_root);
    for ignore_file in repository_ignore_files(repository_root) {
        if ignore_file.is_file() {
            let _ = builder.add(ignore_file);
        }
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// Lists the ignore files whose change invalidates one repository's cached matcher.
/// 列出其变化会使一个仓库缓存匹配器失效的忽略文件。
fn repository_ignore_files(repository_root: &Path) -> [PathBuf; 2] {
    [
        repository_root.join(".gitignore"),
        repository_root.join(".git").join("info").join("exclude"),
    ]
}

/// Reports whether one changed path can alter the Git review state of one repository.
/// 判断一个变更路径是否可能改变某个仓库的 Git 审查状态。
fn change_affects_repository(
    ignore_matchers: &mut HashMap<PathBuf, Gitignore>,
    repository_root: &Path,
    changed_path: &Path,
) -> bool {
    let Ok(relative_path) = changed_path.strip_prefix(repository_root) else {
        return false;
    };
    let mut components = relative_path.components();
    if components.next() == Some(std::path::Component::Normal(".git".as_ref())) {
        return !components.next().is_some_and(|directory| {
            GIT_WATCH_SILENT_DIRECTORIES
                .iter()
                .any(|silent| directory == std::path::Component::Normal(silent.as_ref()))
        });
    }
    if relative_path.as_os_str().is_empty() {
        return true;
    }
    ignore_matchers
        .entry(repository_root.to_path_buf())
        .or_insert_with(|| build_repository_ignore_matcher(repository_root))
        .matched_path_or_any_parents(relative_path, changed_path.is_dir())
        .is_none()
}

/// Groups changed paths by project and repository while retaining deterministic ordering.
/// 按项目与仓库归并变更路径，并保持确定性排序。
///
/// Paths ignored by a repository's root ignore rules, and Git object or reflog churn, are dropped for
/// that repository only; `ignore_matchers` caches one matcher per watched repository root.
/// 被某仓库根忽略规则忽略的路径以及 Git 对象、reflog 变化仅对该仓库丢弃；`ignore_matchers`
/// 为每个监听仓库根缓存一个匹配器。
fn group_webui_git_watch_events(
    targets: &RwLock<BTreeMap<PathBuf, BTreeSet<GitWatchTarget>>>,
    ignore_matchers: &mut HashMap<PathBuf, Gitignore>,
    changed_paths: &[PathBuf],
) -> BTreeMap<(Option<String>, PathBuf), BTreeSet<String>> {
    let targets = targets
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    ignore_matchers.retain(|repository_root, _| {
        targets.contains_key(repository_root)
            && !changed_paths
                .iter()
                .any(|changed_path| repository_ignore_files(repository_root).contains(changed_path))
    });
    let mut grouped = BTreeMap::new();
    for changed_path in changed_paths {
        // Only ancestors can own a changed path, so look them up instead of scanning every root.
        // 只有祖先目录可能拥有变更路径，因此逐级查表而不是扫描全部仓库根。
        for ancestor in changed_path.ancestors() {
            let Some((repository_root, repository_targets)) = targets.get_key_value(ancestor)
            else {
                continue;
            };
            if !change_affects_repository(ignore_matchers, repository_root, changed_path) {
                continue;
            }
            for target in repository_targets {
                grouped
                    .entry((target.project_id.clone(), target.workspace_path.clone()))
                    .or_insert_with(BTreeSet::new)
                    .insert(target.repository_key.clone());
            }
        }
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the event names a canonical Windows workspace without the verbatim prefix clients never use.
    /// 验证事件指明规范化的 Windows 工作区时不带客户端从不使用的 verbatim 前缀。
    #[test]
    fn change_event_names_the_workspace_in_host_path_form() {
        let event = workspace_git_changed_event(
            Some(String::from("project")),
            Path::new(r"\\?\D:\openvulcan\vulcan-code"),
            BTreeSet::from([String::from(".")]),
        );
        assert_eq!(event.workspace_path, r"D:\openvulcan\vulcan-code");
        assert_eq!(event.repository_keys, vec![String::from(".")]);
    }

    /// Verifies nested repository paths notify both matching independent targets.
    /// 验证嵌套仓库路径会通知所有匹配的独立目标。
    #[test]
    fn groups_nested_repository_change_for_matching_targets() {
        let targets = RwLock::new(BTreeMap::from([
            (
                PathBuf::from("/workspace"),
                BTreeSet::from([GitWatchTarget {
                    project_id: Some(String::from("project")),
                    workspace_path: PathBuf::from("/workspace"),
                    repository_key: String::from("."),
                }]),
            ),
            (
                PathBuf::from("/workspace/service"),
                BTreeSet::from([GitWatchTarget {
                    project_id: Some(String::from("project")),
                    workspace_path: PathBuf::from("/workspace"),
                    repository_key: String::from("service"),
                }]),
            ),
        ]));
        let grouped = group_webui_git_watch_events(
            &targets,
            &mut HashMap::new(),
            &[PathBuf::from("/workspace/service/src/lib.rs")],
        );
        assert_eq!(
            grouped
                .get(&(Some(String::from("project")), PathBuf::from("/workspace")))
                .expect("project event should exist"),
            &BTreeSet::from([String::from("."), String::from("service")]),
        );
    }

    /// Verifies unrelated filesystem paths do not produce Git review events.
    /// 验证无关文件系统路径不会产生 Git 审查事件。
    #[test]
    fn ignores_unrelated_change_paths() {
        let targets = RwLock::new(BTreeMap::from([(
            PathBuf::from("/workspace"),
            BTreeSet::from([GitWatchTarget {
                project_id: None,
                workspace_path: PathBuf::from("/workspace"),
                repository_key: String::from("."),
            }]),
        )]));
        assert!(
            group_webui_git_watch_events(
                &targets,
                &mut HashMap::new(),
                &[PathBuf::from("/other/file.txt")]
            )
            .is_empty()
        );
    }

    /// Verifies ignored worktree paths and Git object churn stay silent, while tracked files, refs and
    /// a rewritten `.gitignore` still notify and refresh the cached matcher.
    /// 验证被忽略的工作树路径与 Git 对象变化保持静默，而受跟踪文件、refs 与改写后的 `.gitignore`
    /// 仍会通知并刷新缓存匹配器。
    #[test]
    fn filters_ignored_paths_and_git_object_churn() {
        let root = tempfile::tempdir().expect("create git watch test root");
        let root = std::fs::canonicalize(root.path()).expect("canonicalize git watch test root");
        std::fs::create_dir_all(root.join("target/debug")).expect("create ignored directory");
        std::fs::write(root.join(".gitignore"), "target/\n").expect("write gitignore");
        let targets = RwLock::new(BTreeMap::from([(
            root.clone(),
            BTreeSet::from([GitWatchTarget {
                project_id: None,
                workspace_path: root.clone(),
                repository_key: String::from("."),
            }]),
        )]));
        let mut matchers = HashMap::new();
        let mut notifies = |path: PathBuf| {
            !group_webui_git_watch_events(&targets, &mut matchers, &[path]).is_empty()
        };

        assert!(!notifies(root.join("target/debug/app.o")));
        assert!(!notifies(root.join(".git/objects/ab/cdef")));
        assert!(!notifies(root.join(".git/logs/HEAD")));
        assert!(notifies(root.join(".git/refs/heads/main")));
        assert!(notifies(root.join(".git/index")));
        assert!(notifies(root.join("src/lib.rs")));

        std::fs::write(root.join(".gitignore"), "").expect("rewrite gitignore");
        assert!(notifies(root.join(".gitignore")));
        assert!(notifies(root.join("target/debug/app.o")));
    }
}
