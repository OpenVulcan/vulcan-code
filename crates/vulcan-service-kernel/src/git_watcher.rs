//! Notify-backed Git repository review watcher.
//! 基于 notify 的 Git 仓库审查文件监听器。

use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use vulcan_file_watcher::live::{
    LiveFileWatcher, LiveFileWatcherSubscriber, LiveWatchRegistration, ThrottledLiveWatchReceiver,
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
    /// Shared notify-backed watcher used by every project subscription.
    /// 所有项目订阅共用的 notify 文件监听器。
    _watcher: Arc<LiveFileWatcher>,
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
    project_id: Option<&str>,
    workspace_path: &str,
    catalog: &WebuiGitRepositoryCatalog,
) -> Result<(), String> {
    let workspace_path = std::fs::canonicalize(workspace_path)
        .map_err(|error| format!("无法规范化 Git 监听工作区 `{workspace_path}`：{error}"))?;
    let manager = shared_webui_git_watcher(event_emitter)?;
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
) -> Result<Arc<WebuiGitWatcherManager>, String> {
    if let Some(manager) = WEBUI_GIT_WATCHER.get() {
        return Ok(Arc::clone(manager));
    }

    let watcher = Arc::new(
        LiveFileWatcher::new()
            .map_err(|error| format!("无法初始化 WebUI Git 文件监听器：{error}"))?,
    );
    let (subscriber, receiver) = watcher.add_subscriber();
    let targets = Arc::new(RwLock::new(BTreeMap::new()));
    let event_targets = Arc::clone(&targets);
    let task = spawn_service_task(async move {
        let mut receiver = ThrottledLiveWatchReceiver::new(receiver, Duration::from_millis(250));
        while let Some(event) = receiver.recv().await {
            let grouped = group_webui_git_watch_events(&event_targets, &event.paths);
            for ((project_id, workspace_path), repository_keys) in grouped {
                event_emitter.emit(ServiceEvent::WorkspaceGitChanged(Box::new(
                    WorkspaceGitChangedEvent {
                        project_id,
                        workspace_path: workspace_path.to_string_lossy().into_owned(),
                        repository_keys: repository_keys.into_iter().collect(),
                    },
                )));
            }
        }
    });
    let manager = Arc::new(WebuiGitWatcherManager {
        _watcher: watcher,
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

/// Groups changed paths by project and repository while retaining deterministic ordering.
/// 按项目与仓库归并变更路径，并保持确定性排序。
fn group_webui_git_watch_events(
    targets: &RwLock<BTreeMap<PathBuf, BTreeSet<GitWatchTarget>>>,
    changed_paths: &[PathBuf],
) -> BTreeMap<(Option<String>, PathBuf), BTreeSet<String>> {
    let targets = targets
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut grouped = BTreeMap::new();
    for changed_path in changed_paths {
        for (repository_root, repository_targets) in targets.iter() {
            if changed_path != repository_root && !changed_path.starts_with(repository_root) {
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
            group_webui_git_watch_events(&targets, &[PathBuf::from("/other/file.txt")]).is_empty()
        );
    }
}
