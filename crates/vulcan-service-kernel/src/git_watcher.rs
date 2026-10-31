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
            let grouped = group_webui_git_watch_events(
                &event_targets,
                &mut ignore_matchers,
                &event.paths,
                &event.modification_only_paths,
            );
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

/// Reserved metadata lock files directly under a Git administrative root.
/// 直接位于 Git 管理根下的保留元数据锁文件。
/// Module and worktree directory names are not part of this set, even when they end in `.lock`.
/// 模块与工作树目录名不属于此集合，即使其名称以 `.lock` 结尾。
const GIT_WATCH_METADATA_LOCK_FILES: [&str; 5] = [
    "index.lock",
    "HEAD.lock",
    "config.lock",
    "packed-refs.lock",
    "shallow.lock",
];

/// Reports whether an administrative-relative path occupies a known temporary lock-file slot.
/// 判断相对于管理根的路径是否位于已知的临时锁文件位置。
/// `git_relative_path` excludes `.git`; unknown locations return false and keep notifying.
/// `git_relative_path` 不含 `.git`；未知位置返回 false，保持通知。
fn is_git_temporary_lock_path(git_relative_path: &Path) -> bool {
    // Git reference names cannot contain components ending in `.lock`.
    // Git 引用名称不能包含以 `.lock` 结尾的路径分量。
    let mut components = git_relative_path.components();
    // The first component distinguishes root metadata files from the reference namespace.
    // 首个路径分量用于区分根元数据文件与引用命名空间。
    let Some(first) = components.next() else {
        return false;
    };
    if components.next().is_none() {
        return GIT_WATCH_METADATA_LOCK_FILES
            .iter()
            .any(|name| first == std::path::Component::Normal(name.as_ref()));
    }
    first == std::path::Component::Normal("refs".as_ref())
        && git_relative_path
            .extension()
            .is_some_and(|extension| extension == "lock")
}

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
/// `modification_only` excludes structural events and root invalidations from directory filtering.
/// `modification_only` 用于确保目录过滤不会丢弃结构事件和根失效通知。
fn change_affects_repository(
    ignore_matchers: &mut HashMap<PathBuf, Gitignore>,
    repository_root: &Path,
    changed_path: &Path,
    modification_only: bool,
) -> bool {
    // Resolve the event within this repository; unrelated paths never invalidate its review state.
    // 将事件定位到当前仓库内部；无关路径不会使其审查状态失效。
    let Ok(relative_path) = changed_path.strip_prefix(repository_root) else {
        return false;
    };
    if relative_path.as_os_str().is_empty() {
        return true;
    }
    // Only this repository's own administrative tree bypasses its worktree ignore rules.
    // 只有当前仓库自身的管理目录可以绕过其工作树忽略规则。
    let own_git_tree =
        relative_path.components().next() == Some(std::path::Component::Normal(".git".as_ref()));
    if !own_git_tree
        && ignore_matchers
            .entry(repository_root.to_path_buf())
            .or_insert_with(|| build_repository_ignore_matcher(repository_root))
            .matched_path_or_any_parents(relative_path, changed_path.is_dir())
            .is_ignore()
    {
        return false;
    }
    // Nested Git metadata remains independently visible to its own registered repository.
    // 嵌套 Git 元数据仍对其自身独立注册的仓库可见。
    let mut components = relative_path.components();
    while let Some(component) = components.next() {
        if component != std::path::Component::Normal(".git".as_ref()) {
            continue;
        }
        // Keep the exact path relative to the administrative root, not only its filename suffix.
        // 保留相对于管理根的精确路径，而不是只检查文件名后缀。
        let git_relative_path = components.as_path();
        // Preserve the existing cheap object/reflog filter before filesystem lookups.
        // 在文件系统查询前保留既有的低成本对象与 reflog 过滤。
        if components.next().is_some_and(|directory| {
            GIT_WATCH_SILENT_DIRECTORIES
                .iter()
                .any(|silent| directory == std::path::Component::Normal(silent.as_ref()))
        }) {
            return false;
        }
        // Unknown .lock paths may be real module directories even after a rename or removal.
        // 未知 .lock 路径可能是真实模块目录，重命名或删除后也必须保留该可能性。
        if is_git_temporary_lock_path(git_relative_path) && !changed_path.is_dir() {
            return false;
        }
        // Only explicit non-rename modifications of existing directories are metadata noise.
        // 仅明确的既有目录非重命名修改属于属性噪声。
        return !(modification_only && changed_path.is_dir());
    }
    true
}

/// Groups changed paths by project and repository while retaining deterministic ordering.
/// 按项目与仓库归并变更路径，并保持确定性排序。
///
/// Paths ignored by a repository's root ignore rules, and Git object or reflog churn, are dropped for
/// that repository only; `ignore_matchers` caches one matcher per watched repository root.
/// 被某仓库根忽略规则忽略的路径以及 Git 对象、reflog 变化仅对该仓库丢弃；`ignore_matchers`
/// 为每个监听仓库根缓存一个匹配器。
/// `modification_only_paths` identifies paths whose batch contains no structural change or rescan.
/// `modification_only_paths` 标识本批次中没有结构变更或重扫的路径。
fn group_webui_git_watch_events(
    targets: &RwLock<BTreeMap<PathBuf, BTreeSet<GitWatchTarget>>>,
    ignore_matchers: &mut HashMap<PathBuf, Gitignore>,
    changed_paths: &[PathBuf],
    modification_only_paths: &BTreeSet<PathBuf>,
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
            if !change_affects_repository(
                ignore_matchers,
                repository_root,
                changed_path,
                modification_only_paths.contains(changed_path),
            ) {
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

    /// Real administrative directories ending in .lock still invalidate Git before and after removal.
    /// 以 .lock 结尾的真实管理目录在删除前后仍会使 Git 状态失效。
    #[test]
    fn lock_suffixed_directories_preserve_creation_rename_and_removal() {
        // Keep repository and detached directory destinations inside one isolated fixture.
        // 将仓库与移出目录的目标保留在同一隔离夹具内。
        let temporary = tempfile::tempdir().expect("create lock directory fixture");
        // The repository root is distinct from the directory used as a rename destination.
        // 仓库根与重命名目标目录分离。
        let root = temporary.path().join("repository");
        // Reuse the real filter cache across directory lifecycles.
        // 在目录生命周期中复用真实过滤器缓存。
        let mut matchers = HashMap::new();
        for relative in [
            ".git/modules/module.lock",
            ".git/modules/index.lock",
            ".git/worktrees/topic.lock",
            ".git/custom.lock",
        ] {
            // Include a reserved lock basename under modules to reject suffix-only classification.
            // 在 modules 下包含保留锁文件的同名目录，以排除仅依据后缀的分类。
            let directory = root.join(relative);
            std::fs::create_dir_all(&directory).expect("create real administrative directory");
            std::fs::write(directory.join("HEAD"), "ref: refs/heads/main\n")
                .expect("write administrative content");
            assert!(
                change_affects_repository(&mut matchers, &root, &directory, false),
                "directory creation must notify: {relative}"
            );
            assert!(
                !change_affects_repository(&mut matchers, &root, &directory, true),
                "directory metadata alone stays silent: {relative}"
            );
            // Renaming away leaves no source metadata for the filter to inspect.
            // 移出目录后，过滤器无法再检查源路径的属性。
            let detached = temporary.path().join("detached");
            std::fs::rename(&directory, &detached).expect("move directory out of repository");
            assert!(
                change_affects_repository(&mut matchers, &root, &directory, false),
                "missing rename source must notify: {relative}"
            );
            std::fs::rename(&detached, &directory).expect("restore renamed directory");
            assert!(
                change_affects_repository(&mut matchers, &root, &directory, false),
                "rename destination must notify: {relative}"
            );
            std::fs::remove_dir_all(&directory).expect("remove isolated administrative directory");
            assert!(
                change_affects_repository(&mut matchers, &root, &directory, false),
                "removed directory must notify: {relative}"
            );
        }
    }

    /// Parent ignore rules suppress nested metadata without silencing the independently watched child.
    /// 父仓库忽略规则抑制嵌套元数据，但不使独立监听的子仓库静默。
    #[test]
    fn ignored_nested_repository_refreshes_only_its_registered_owner() {
        // Build one ignored child with both metadata and worktree changes.
        // 构造同时存在元数据与工作树变更的被忽略子仓库。
        let temporary = tempfile::tempdir().expect("create ignored repository fixture");
        // Use the native canonical namespace for grouping ownership.
        // 使用原生规范路径命名空间进行归属分组。
        let root = std::fs::canonicalize(temporary.path()).expect("canonicalize ignored fixture");
        std::fs::create_dir_all(root.join("ignored/child/.git/refs/heads"))
            .expect("create child repository");
        std::fs::write(root.join(".gitignore"), "ignored/\n").expect("ignore child from parent");
        // Initially only the parent repository owns a watch target.
        // 最初只有父仓库拥有监听目标。
        let targets = RwLock::new(BTreeMap::from([(
            root.clone(),
            BTreeSet::from([GitWatchTarget {
                project_id: None,
                workspace_path: root.clone(),
                repository_key: String::from("."),
            }]),
        )]));
        // Keep cached ignore rules while adding the independently owned child.
        // 添加独立归属的子仓库时，保留已缓存的忽略规则。
        let mut matchers = HashMap::new();
        // Every path is a real child change, but is outside the parent's reviewable worktree.
        // 每条路径都是真实子仓库变更，但均不属于父仓库可审查的工作树。
        let paths = [
            ".git/index",
            ".git/HEAD",
            ".git/refs/heads/main",
            ".git",
            "source.rs",
        ]
        .map(|relative| root.join("ignored/child").join(relative));
        for path in &paths {
            assert!(
                group_webui_git_watch_events(
                    &targets,
                    &mut matchers,
                    std::slice::from_ref(path),
                    &BTreeSet::new()
                )
                .is_empty(),
                "ignored child must not refresh parent: {}",
                path.display()
            );
        }
        targets.write().expect("update fixture targets").insert(
            root.join("ignored/child"),
            BTreeSet::from([GitWatchTarget {
                project_id: None,
                workspace_path: root.clone(),
                repository_key: String::from("ignored/child"),
            }]),
        );
        for path in &paths {
            // Only the independently enabled child gets a notification.
            // 只有独立启用的子仓库收到通知。
            let grouped = group_webui_git_watch_events(
                &targets,
                &mut matchers,
                std::slice::from_ref(path),
                &BTreeSet::new(),
            );
            assert_eq!(
                grouped.get(&(None, root.clone())),
                Some(&BTreeSet::from([String::from("ignored/child")])),
                "child ownership must survive parent ignore: {}",
                path.display()
            );
        }
        std::fs::write(root.join(".gitignore"), "").expect("remove parent ignore rule");
        let _ = group_webui_git_watch_events(
            &targets,
            &mut matchers,
            &[root.join(".gitignore")],
            &BTreeSet::new(),
        );
        // The next real change uses rebuilt ignore rules and reaches both owners.
        // 下一次真实变更使用重建后的忽略规则，通知两个所有者。
        let grouped = group_webui_git_watch_events(
            &targets,
            &mut matchers,
            &[root.join("ignored/child/.git/index")],
            &BTreeSet::new(),
        );
        assert_eq!(
            grouped.get(&(None, root)),
            Some(&BTreeSet::from([
                String::from("."),
                String::from("ignored/child")
            ]))
        );
    }

    /// Explicit ignore exceptions and this repository's own Git metadata remain refreshable.
    /// 显式忽略例外与当前仓库自身 Git 元数据仍可触发刷新。
    #[test]
    fn ignore_exceptions_and_own_git_metadata_preserve_real_changes() {
        // Use selective ignore rules so whitelist matches are distinguished from ignored paths.
        // 使用选择性忽略规则，区分重新包含匹配与被忽略路径。
        let temporary = tempfile::tempdir().expect("create ignore exception fixture");
        // The root owns both ordinary files and an explicitly re-included nested repository.
        // 根同时拥有普通文件与被明确重新包含的嵌套仓库。
        let root = temporary.path();
        std::fs::create_dir_all(root.join("ignored/kept/.git")).expect("create included child");
        std::fs::write(
            root.join(".gitignore"),
            "*.rs\n!keep.rs\nignored/*\n!ignored/kept/\n/.git/\n",
        )
        .expect("write ignore exceptions");
        // All checks exercise the production cached matcher.
        // 所有检查均覆盖生产缓存匹配器。
        let mut matchers = HashMap::new();
        assert!(!change_affects_repository(
            &mut matchers,
            root,
            &root.join("ignored.rs"),
            false
        ));
        for relative in [
            "keep.rs",
            "ignored/kept/.git/index",
            ".git/index",
            ".git/HEAD",
        ] {
            assert!(
                change_affects_repository(&mut matchers, root, &root.join(relative), false),
                "included path must refresh: {relative}"
            );
        }
    }

    /// Native Windows locks stay silent while index installation and .lock directory renames notify.
    /// 原生 Windows 锁保持静默，但 index 安装与 .lock 目录重命名仍会通知。
    #[cfg(windows)]
    #[tokio::test]
    async fn native_git_lock_filter_preserves_index_and_directory_renames() {
        // Keep native filesystem writes isolated from the user's working repository.
        // 将原生文件系统写入与用户工作仓库隔离。
        let temporary = tempfile::tempdir().expect("create native git fixture");
        // Moving a directory outside the watched root reports only its vanished source path.
        // 将目录移出监听根只会报告已经消失的源路径。
        let detached = tempfile::tempdir().expect("create detached directory destination");
        // Native notify paths and registered paths use the same canonical namespace.
        // 原生 notify 路径与注册路径使用相同的规范命名空间。
        let root = std::fs::canonicalize(temporary.path()).expect("canonicalize native fixture");
        std::fs::create_dir(root.join(".git")).expect("create administrative directory");
        std::fs::create_dir_all(root.join(".git/modules/module.lock"))
            .expect("create lock-suffixed module directory");
        std::fs::write(
            root.join(".git/modules/module.lock/HEAD"),
            "ref: refs/heads/main\n",
        )
        .expect("write module administrative content");
        // Exercise the real OS callback, bounded queue and delivery pipeline without the global service.
        // 覆盖真实 OS 回调、有界队列和投递管线，不使用全局服务。
        let watcher =
            Arc::new(vulcan_file_watcher::live::LiveFileWatcher::new().expect("native watcher"));
        // Retain the local receiver until both the lock and installed index have been observed.
        // 保留本地接收器，直到观察到锁文件和已安装的 index。
        let (subscriber, mut receiver) = watcher.add_subscriber();
        // Recursive coverage must include .git and its children.
        // 递归覆盖必须包含 .git 及其子路径。
        let _registration = subscriber.register_path(root.clone(), true);
        // Match the production grouping target for one repository.
        // 匹配生产分组中单个仓库的目标。
        let targets = RwLock::new(BTreeMap::from([(
            root.clone(),
            BTreeSet::from([GitWatchTarget {
                project_id: None,
                workspace_path: root.clone(),
                repository_key: String::from("."),
            }]),
        )]));
        // Reuse the matcher across the transaction's prepare and commit phases.
        // 在事务的准备与提交阶段之间复用匹配器。
        let mut matchers = HashMap::new();
        std::fs::write(root.join(".git/index.lock"), "index fixture").expect("prepare index lock");
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                // Lock-only batches, including .git directory modifications, must not reach the UI.
                // 仅含锁文件的批次，包括 .git 目录修改，不得到达界面。
                let event = receiver.recv().await.expect("native watch remains open");
                assert!(
                    group_webui_git_watch_events(
                        &targets,
                        &mut matchers,
                        &event.paths,
                        &event.modification_only_paths
                    )
                    .is_empty()
                );
                if event.paths.contains(&root.join(".git/index.lock")) {
                    break;
                }
            }
            std::fs::rename(root.join(".git/index.lock"), root.join(".git/index"))
                .expect("install real index");
            loop {
                // Renaming the prepared lock into the real index must survive the same filter.
                // 将准备好的锁重命名为真实 index 后，必须通过同一过滤器。
                let event = receiver.recv().await.expect("native watch remains open");
                // Late lock and parent metadata events may precede the final rename notification.
                // 延迟的锁与父目录属性事件可能先于最终重命名通知到达。
                let grouped = group_webui_git_watch_events(
                    &targets,
                    &mut matchers,
                    &event.paths,
                    &event.modification_only_paths,
                );
                if event.paths.contains(&root.join(".git/index")) {
                    assert_eq!(
                        grouped.get(&(None, root.clone())),
                        Some(&BTreeSet::from([String::from(".")]))
                    );
                    break;
                }
                assert!(grouped.is_empty());
            }
            std::fs::rename(
                root.join(".git/modules/module.lock"),
                detached.path().join("module.lock"),
            )
            .expect("move real module directory out of repository");
            loop {
                // The old directory no longer exists, so suffix or current-type guesses would lose this event.
                // 旧目录已经不存在，依据后缀或当前类型猜测会丢失此事件。
                let event = receiver
                    .recv()
                    .await
                    .expect("native directory watch remains open");
                if !event.paths.contains(&root.join(".git/modules/module.lock")) {
                    continue;
                }
                // A real directory rename must reach the same frontend notification group.
                // 真实目录重命名必须到达同一前端通知分组。
                let grouped = group_webui_git_watch_events(
                    &targets,
                    &mut matchers,
                    &event.paths,
                    &event.modification_only_paths,
                );
                assert_eq!(
                    grouped.get(&(None, root.clone())),
                    Some(&BTreeSet::from([String::from(".")]))
                );
                break;
            }
        })
        .await
        .expect("native index and directory delivery");
    }

    /// Lock churn and directory-only modifications stay silent, but final Git state changes still notify.
    /// 锁文件变化与纯目录修改保持静默，但 Git 最终状态变化仍会通知。
    #[test]
    fn filters_git_lock_and_directory_noise_without_losing_real_changes() {
        // Build real administrative directories so path classification matches the live filesystem.
        // 创建真实管理目录，使路径分类与实时文件系统一致。
        let temporary = tempfile::tempdir().expect("create git noise fixture");
        // Canonical paths match the watch registration namespace on Windows as well.
        // 规范路径也与 Windows 上的监听注册命名空间一致。
        let root = std::fs::canonicalize(temporary.path()).expect("canonicalize fixture root");
        std::fs::create_dir_all(root.join(".git/refs/heads")).expect("create refs directory");
        // Register one repository and inspect the same grouped notifications emitted to the frontend.
        // 注册一个仓库，检查与发送到前端相同的分组通知。
        let targets = RwLock::new(BTreeMap::from([(
            root.clone(),
            BTreeSet::from([GitWatchTarget {
                project_id: None,
                workspace_path: root.clone(),
                repository_key: String::from("."),
            }]),
        )]));
        // Reuse the production ignore cache across noisy and meaningful batches.
        // 在噪声批次与有效批次之间复用生产忽略缓存。
        let mut matchers = HashMap::new();
        // Parent-directory notifications accompany lock creation/removal on Windows.
        // Windows 上锁文件的新建和删除会伴随父目录通知。
        let directories = [".git", ".git/refs", ".git/refs/heads"]
            .map(|path| root.join(path))
            .into_iter()
            .collect::<BTreeSet<_>>();
        for lock in [
            ".git/index.lock",
            ".git/HEAD.lock",
            ".git/config.lock",
            ".git/packed-refs.lock",
            ".git/refs/heads/main.lock",
        ] {
            // Exercise both isolated lock events and their directory-noise companions.
            // 同时覆盖独立锁事件与伴随目录噪声的事件。
            let mut paths = vec![root.join(lock)];
            assert!(
                group_webui_git_watch_events(&targets, &mut matchers, &paths, &BTreeSet::new())
                    .is_empty(),
                "lock path must be silent: {lock}"
            );
            paths.extend(directories.iter().cloned());
            assert!(
                group_webui_git_watch_events(&targets, &mut matchers, &paths, &directories)
                    .is_empty(),
                "lock and directory churn must be silent: {lock}"
            );
        }
        for changed in [
            ".git/index",
            ".git/HEAD",
            ".git/refs/heads/main",
            ".git/packed-refs",
            ".git/config",
            ".git/info/exclude",
            "src/lib.rs",
            "Cargo.lock",
        ] {
            // Real files ensure filtering checks file type, not only the absence of a directory.
            // 使用真实文件，确保过滤检查的是文件类型，而非仅仅不存在目录。
            let changed_path = root.join(changed);
            std::fs::create_dir_all(changed_path.parent().expect("fixture parent"))
                .expect("create changed file parent");
            std::fs::write(&changed_path, "fixture\n").expect("write changed file");
            // A committed lock rename and a working-tree lockfile must both survive a noisy batch.
            // 锁事务提交后的重命名与工作树锁定文件都必须在噪声批次中保留。
            let mut paths = vec![root.join(".git/index.lock"), changed_path.clone()];
            paths.extend(directories.iter().cloned());
            // Structural events and ordinary file modifications both require a refresh.
            // 结构性事件与普通文件内容修改都需要刷新。
            for modification_only in [false, true] {
                // Only existing directories qualify for suppression; modified files remain meaningful.
                // 只有既有目录满足过滤条件；修改后的文件仍属于有效变化。
                let mut modifications = directories.clone();
                if modification_only {
                    modifications.insert(changed_path.clone());
                }
                // One repository refresh is emitted regardless of the number of coalesced paths.
                // 无论合并了多少路径，均只发出一次仓库刷新。
                let grouped =
                    group_webui_git_watch_events(&targets, &mut matchers, &paths, &modifications);
                assert_eq!(
                    grouped.get(&(None, root.clone())),
                    Some(&BTreeSet::from([String::from(".")])),
                    "real change must notify: {changed}"
                );
            }
        }
    }

    /// Structural directories, root rescans and linked-worktree pointer files remain meaningful.
    /// 目录结构变更、根重扫与关联工作树指针文件仍属于有效事件。
    #[test]
    fn preserves_git_directory_structure_roots_and_worktree_pointer_files() {
        // This fixture needs actual directories to distinguish metadata from structural notifications.
        // 此夹具需要真实目录，以区分属性通知与结构通知。
        let temporary = tempfile::tempdir().expect("create structural fixture");
        // Keep all fixture operations within the isolated temporary repository.
        // 所有夹具操作均限定在隔离临时仓库中。
        let root = temporary.path();
        std::fs::create_dir_all(root.join(".git/refs/heads/topic"))
            .expect("create branch directory");
        std::fs::create_dir(root.join("src")).expect("create worktree directory");
        // Reuse the cache to ensure stronger events are not suppressed after earlier noise.
        // 复用缓存，确保更强事件不会在先前噪声后被过滤。
        let mut matchers = HashMap::new();
        for directory in [".git", ".git/refs/heads/topic"] {
            assert!(!change_affects_repository(
                &mut matchers,
                root,
                &root.join(directory),
                true
            ));
            assert!(change_affects_repository(
                &mut matchers,
                root,
                &root.join(directory),
                false
            ));
        }
        assert!(change_affects_repository(&mut matchers, root, root, true));
        assert!(change_affects_repository(&mut matchers, root, root, false));
        assert!(change_affects_repository(
            &mut matchers,
            root,
            &root.join("src"),
            true
        ));
        assert!(change_affects_repository(
            &mut matchers,
            root,
            &root.join(".git/refs/heads/deleted"),
            false
        ));
        std::fs::rename(root.join(".git"), root.join("git-admin"))
            .expect("move administrative directory");
        assert!(change_affects_repository(
            &mut matchers,
            root,
            &root.join(".git"),
            false
        ));
        std::fs::write(root.join(".git"), "gitdir: git-admin\n").expect("write worktree pointer");
        assert!(change_affects_repository(
            &mut matchers,
            root,
            &root.join(".git"),
            true
        ));
        assert!(change_affects_repository(
            &mut matchers,
            root,
            &root.join(".git"),
            false
        ));
    }

    /// Nested Git noise cannot leak through a parent repository, while real child changes notify both.
    /// 嵌套 Git 噪声不能经父仓库泄漏，但真实子仓库变更仍通知两者。
    #[test]
    fn filters_nested_git_noise_for_every_matching_repository() {
        // Watch both a parent workspace and its independently registered child repository.
        // 同时监听父工作区及其独立注册的子仓库。
        let temporary = tempfile::tempdir().expect("create nested fixture");
        // Canonical paths prevent platform path aliases from affecting the test.
        // 规范路径避免平台路径别名影响测试。
        let root = std::fs::canonicalize(temporary.path()).expect("canonicalize nested fixture");
        std::fs::create_dir_all(root.join("child/.git/refs/heads")).expect("create child refs");
        // Each repository contributes its stable key to the shared workspace event.
        // 每个仓库向共享工作区事件提供自己的稳定键。
        let targets = RwLock::new(
            [(".", root.clone()), ("child", root.join("child"))]
                .into_iter()
                .map(|(key, path)| {
                    (
                        path,
                        BTreeSet::from([GitWatchTarget {
                            project_id: None,
                            workspace_path: root.clone(),
                            repository_key: String::from(key),
                        }]),
                    )
                })
                .collect(),
        );
        // The same cache covers parent and child routing.
        // 同一缓存覆盖父仓库与子仓库路由。
        let mut matchers = HashMap::new();
        // These directory paths are modifications, not structural events.
        // 这些目录路径属于修改，而非结构性事件。
        let modifications =
            BTreeSet::from([root.join("child/.git"), root.join("child/.git/refs/heads")]);
        // Lock creation alone, together with its parent metadata events, must produce no refresh.
        // 单纯锁创建及其父目录属性事件不得产生刷新。
        let mut paths = vec![root.join("child/.git/refs/heads/main.lock")];
        paths.extend(modifications.iter().cloned());
        assert!(
            group_webui_git_watch_events(&targets, &mut matchers, &paths, &modifications)
                .is_empty()
        );
        paths.push(root.join("child/.git/refs/heads/main"));
        // The final reference update still invalidates every matching repository.
        // 最终引用更新仍使所有匹配仓库失效。
        let grouped = group_webui_git_watch_events(&targets, &mut matchers, &paths, &modifications);
        assert_eq!(
            grouped.get(&(None, root)),
            Some(&BTreeSet::from([String::from("."), String::from("child")]))
        );
    }

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
            &BTreeSet::new(),
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
                &[PathBuf::from("/other/file.txt")],
                &BTreeSet::new(),
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
            !group_webui_git_watch_events(&targets, &mut matchers, &[path], &BTreeSet::new())
                .is_empty()
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
