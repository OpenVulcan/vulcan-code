//! VCE subscriber over the service-owned shared workspace watcher.
//! 服务持有的共享工作区监听器之上的 VCE 订阅器。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tokio::sync::Mutex as AsyncMutex;
use vulcan_context_engine::RefreshReason;
use vulcan_file_watcher::live::{
    LiveFileWatcherSubscriber, LiveWatchRegistration, ThrottledLiveWatchReceiver,
};

use crate::workspace_watcher::WorkspaceWatcherCenter;

use super::VceRuntimeManager;

/// One coalesced dirty set waiting for a project refresh worker.
/// 等待项目刷新工作器处理的一份合并脏集合。
#[derive(Default)]
struct VcePendingWatchBatch {
    /// Canonical workspace-relative changed paths.
    /// 规范工作区相对变更路径。
    paths: BTreeSet<String>,
    /// Whether the project root itself changed and requires a full scan.
    /// 项目根本身是否变化并要求完整扫描。
    force_full: bool,
    /// Consecutive refresh failures retained for bounded exponential backoff.
    /// 为有界指数退避保留的连续刷新失败次数。
    retry_attempt: u32,
    /// Earliest instant at which the failed batch may be attempted again.
    /// 失败批次允许再次尝试的最早时刻。
    retry_not_before: Option<tokio::time::Instant>,
}

/// One active project root registered by the VCE watcher subscriber.
/// VCE 监听订阅器注册的一个活动项目根。
struct VceWatchRegistration {
    /// RAII registration retaining the physical recursive watch.
    /// 保留物理递归监听的 RAII 注册。
    _registration: LiveWatchRegistration,
}

/// Cloneable VCE watcher service with independent subscriber and retry queue.
/// 具有独立订阅者与重试队列的可克隆 VCE 监听服务。
#[derive(Clone)]
pub(crate) struct VceWatcherService {
    /// Shared implementation retained by service state clones.
    /// 由服务状态克隆共同持有的实现。
    inner: Arc<VceWatcherInner>,
}

/// Long-lived watcher tasks and project registrations.
/// 长生命周期监听任务与项目注册。
struct VceWatcherInner {
    /// Logical VCE subscriber over the shared physical watcher.
    /// 共享物理监听器之上的逻辑 VCE 订阅者。
    subscriber: LiveFileWatcherSubscriber,
    /// Active project registrations keyed by stable workbench project ID.
    /// 按稳定工作台项目 ID 索引的活动项目注册。
    registrations: Mutex<BTreeMap<String, VceWatchRegistration>>,
    /// Canonical roots read by the physical event consumer.
    /// 物理事件消费者读取的规范根目录。
    targets: Arc<RwLock<BTreeMap<String, PathBuf>>>,
    /// Coalesced dirty sets retained across refresh failures.
    /// 跨刷新失败保留的合并脏集合。
    pending: Arc<AsyncMutex<BTreeMap<String, VcePendingWatchBatch>>>,
    /// Physical event consumer task.
    /// 物理事件消费任务。
    _event_task: crate::ServiceTaskHandle<()>,
    /// Serialized retrying refresh worker task.
    /// 串行重试刷新工作任务。
    _refresh_task: crate::ServiceTaskHandle<()>,
}

impl VceWatcherService {
    /// Create the independent VCE subscriber and its coalescing worker.
    /// 创建独立 VCE 订阅器及其合并工作器。
    ///
    /// # Parameters
    /// 参数
    /// `center` is the service-owned physical workspace watcher.
    /// `center` 是服务持有的物理工作区监听器。
    /// `manager` receives functional refresh calls.
    /// `manager` 接收功能刷新调用。
    ///
    /// # Returns
    /// 返回值
    /// A running watcher service.
    /// 返回运行中的监听服务。
    pub(crate) fn new(center: &WorkspaceWatcherCenter, manager: VceRuntimeManager) -> Self {
        let (subscriber, receiver) = center.subscribe();
        let targets = Arc::new(RwLock::new(BTreeMap::new()));
        let pending: Arc<AsyncMutex<BTreeMap<String, VcePendingWatchBatch>>> =
            Arc::new(AsyncMutex::new(BTreeMap::new()));

        let event_targets = Arc::clone(&targets);
        let event_pending = Arc::clone(&pending);
        let event_task = crate::spawn_service_task(async move {
            let mut receiver =
                ThrottledLiveWatchReceiver::new(receiver, Duration::from_millis(250));
            while let Some(event) = receiver.recv().await {
                let grouped = group_vce_watch_events(&event_targets, &event.paths);
                if grouped.is_empty() {
                    continue;
                }
                let mut queued = event_pending.lock().await;
                for (project_id, batch) in grouped {
                    let target = queued.entry(project_id).or_default();
                    target.paths.extend(batch.paths);
                    target.force_full |= batch.force_full;
                    target.retry_attempt = 0;
                    target.retry_not_before = None;
                }
            }
        });

        let refresh_pending = Arc::clone(&pending);
        let refresh_task = crate::spawn_service_task(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                let batches = {
                    let mut queued = refresh_pending.lock().await;
                    std::mem::take(&mut *queued)
                };
                for (project_id, batch) in batches {
                    if batch
                        .retry_not_before
                        .is_some_and(|deadline| deadline > tokio::time::Instant::now())
                    {
                        refresh_pending.lock().await.insert(project_id, batch);
                        continue;
                    }
                    let result = if batch.force_full {
                        manager
                            .refresh_full(&project_id, RefreshReason::FileWatcher)
                            .await
                            .map(Some)
                    } else {
                        manager
                            .refresh_paths(
                                &project_id,
                                batch.paths.clone(),
                                RefreshReason::FileWatcher,
                            )
                            .await
                    };
                    if let Err(error) = result {
                        eprintln!(
                            "[webui-service] VCE watcher refresh failed for `{project_id}`: {error}"
                        );
                        let mut queued = refresh_pending.lock().await;
                        let retry = queued.entry(project_id).or_default();
                        retry.paths.extend(batch.paths);
                        retry.force_full |= batch.force_full;
                        retry.retry_attempt = batch.retry_attempt.saturating_add(1);
                        retry.retry_not_before = Some(
                            tokio::time::Instant::now()
                                + vce_watch_retry_delay(retry.retry_attempt),
                        );
                    }
                }
            }
        });

        Self {
            inner: Arc::new(VceWatcherInner {
                subscriber,
                registrations: Mutex::new(BTreeMap::new()),
                targets,
                pending,
                _event_task: event_task,
                _refresh_task: refresh_task,
            }),
        }
    }

    /// Register or replace one project's recursive VCE watch.
    /// 注册或替换一个项目的递归 VCE 监听。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the stable workbench project identity.
    /// `project_id` 是稳定工作台项目身份。
    /// `project_root` is the canonicalizable project directory.
    /// `project_root` 是可规范化的项目目录。
    ///
    /// # Returns
    /// 返回值
    /// Success after the target and RAII registration are atomically replaced.
    /// 原子替换目标与 RAII 注册后返回成功。
    pub(crate) fn watch_project(
        &self,
        project_id: &str,
        project_root: &Path,
    ) -> Result<(), String> {
        if project_id.trim().is_empty() {
            return Err(String::from("VCE watcher project_id must not be empty"));
        }
        let canonical = std::fs::canonicalize(project_root).map_err(|error| {
            format!(
                "无法规范化 VCE 监听项目目录 `{}`：{error}",
                project_root.display()
            )
        })?;
        if !canonical.is_dir() {
            return Err(format!("VCE 监听项目路径不是目录：{}", canonical.display()));
        }
        let registration = self.inner.subscriber.register_path(canonical.clone(), true);
        self.inner
            .registrations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                project_id.to_owned(),
                VceWatchRegistration {
                    _registration: registration,
                },
            );
        self.inner
            .targets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(project_id.to_owned(), canonical);
        Ok(())
    }

    /// Remove one project's watch and pending dirty set.
    /// 移除一个项目的监听与待处理脏集合。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the stable workbench project identity.
    /// `project_id` 是稳定工作台项目身份。
    pub(crate) async fn unwatch_project(&self, project_id: &str) {
        self.inner
            .registrations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(project_id);
        self.inner
            .targets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(project_id);
        self.inner.pending.lock().await.remove(project_id);
    }

    /// Stop background tasks and release every physical project registration.
    /// 停止后台任务并释放全部物理项目注册。
    pub(crate) async fn shutdown(&self) {
        self.inner._event_task.abort();
        self.inner._refresh_task.abort();
        self.inner
            .registrations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.inner
            .targets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.inner.pending.lock().await.clear();
    }
}

/// Return a bounded exponential delay for one watcher refresh retry.
/// 返回单次监听刷新重试的有界指数延迟。
///
/// # Parameters
/// 参数
/// `attempt` is the one-based consecutive failure count.
/// `attempt` 是从一开始计数的连续失败次数。
///
/// # Returns
/// 返回值
/// A delay from one second through sixty seconds.
/// 返回一秒到六十秒之间的延迟。
fn vce_watch_retry_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(6);
    Duration::from_secs((1_u64 << exponent).min(60))
}

/// Group physical paths into canonical project-relative VCE dirty batches.
/// 将物理路径归并为规范项目相对 VCE 脏批次。
///
/// # Parameters
/// 参数
/// `targets` contains active canonical project roots.
/// `targets` 包含活动规范项目根。
/// `changed_paths` contains paths emitted by the shared physical watcher.
/// `changed_paths` 包含共享物理监听器发出的路径。
///
/// # Returns
/// 返回值
/// Deterministically keyed project batches.
/// 返回以项目确定性索引的批次。
fn group_vce_watch_events(
    targets: &RwLock<BTreeMap<String, PathBuf>>,
    changed_paths: &[PathBuf],
) -> BTreeMap<String, VcePendingWatchBatch> {
    let targets = targets
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut grouped = BTreeMap::<String, VcePendingWatchBatch>::new();
    for changed_path in changed_paths {
        for (project_id, project_root) in targets.iter() {
            if changed_path == project_root {
                grouped.entry(project_id.clone()).or_default().force_full = true;
                continue;
            }
            let Ok(relative) = changed_path.strip_prefix(project_root) else {
                continue;
            };
            let relative = relative
                .components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(value) => Some(value.to_string_lossy()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            if relative.is_empty() {
                grouped.entry(project_id.clone()).or_default().force_full = true;
            } else {
                grouped
                    .entry(project_id.clone())
                    .or_default()
                    .paths
                    .insert(relative);
            }
        }
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies refresh retries grow exponentially and remain bounded.
    /// 验证刷新重试按指数增长并保持有界。
    #[test]
    fn watcher_retry_delay_is_exponential_and_bounded() {
        assert_eq!(vce_watch_retry_delay(1), Duration::from_secs(1));
        assert_eq!(vce_watch_retry_delay(2), Duration::from_secs(2));
        assert_eq!(vce_watch_retry_delay(6), Duration::from_secs(32));
        assert_eq!(vce_watch_retry_delay(7), Duration::from_secs(60));
        assert_eq!(vce_watch_retry_delay(u32::MAX), Duration::from_secs(60));
    }

    /// Nested physical paths become project-relative dirty identities.
    /// 嵌套物理路径会转换为项目相对脏身份。
    #[test]
    fn groups_nested_path_for_exact_project() {
        let targets = RwLock::new(BTreeMap::from([(
            String::from("project-a"),
            PathBuf::from("/workspace/project-a"),
        )]));
        let grouped = group_vce_watch_events(
            &targets,
            &[PathBuf::from("/workspace/project-a/src/lib.rs")],
        );

        assert_eq!(
            grouped.get("project-a").expect("project batch").paths,
            BTreeSet::from([String::from("src/lib.rs")])
        );
    }

    /// A project-root event escalates to an authoritative full scan.
    /// 项目根事件会升级为权威完整扫描。
    #[test]
    fn project_root_event_forces_full_refresh() {
        let targets = RwLock::new(BTreeMap::from([(
            String::from("project-a"),
            PathBuf::from("/workspace/project-a"),
        )]));
        let grouped = group_vce_watch_events(&targets, &[PathBuf::from("/workspace/project-a")]);

        assert!(grouped["project-a"].force_full);
    }
}
