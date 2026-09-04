//! VCE subscriber over the service-owned shared workspace watcher.
//! 服务持有的共享工作区监听器之上的 VCE 订阅器。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use futures_util::future::BoxFuture;
use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::sync::{Mutex as AsyncMutex, watch};
use vulcan_context_engine::RefreshReason;
use vulcan_file_watcher::live::{
    LiveFileWatcherSubscriber, LiveWatchRegistration, ThrottledLiveWatchReceiver,
};

use crate::ServiceEventEmitter;
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
    /// Physical event consumer task retained for convergent shutdown.
    /// 为收敛式关闭保留的物理事件消费任务。
    event_task: Mutex<Option<crate::ServiceTaskHandle<()>>>,
    /// Concurrent cross-project refresh scheduler retained for convergent shutdown.
    /// 为收敛式关闭保留的跨项目并发刷新调度任务。
    refresh_task: Mutex<Option<crate::ServiceTaskHandle<()>>>,
    /// Cooperative shutdown signal shared by the event consumer and refresh scheduler.
    /// 由事件消费者与刷新调度器共享的协作关闭信号。
    shutdown_sender: watch::Sender<bool>,
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
    pub(crate) fn new(
        center: &WorkspaceWatcherCenter,
        manager: VceRuntimeManager,
        event_emitter: ServiceEventEmitter,
    ) -> Self {
        let (subscriber, receiver) = center.subscribe();
        let targets = Arc::new(RwLock::new(BTreeMap::new()));
        let pending: Arc<AsyncMutex<BTreeMap<String, VcePendingWatchBatch>>> =
            Arc::new(AsyncMutex::new(BTreeMap::new()));
        let (shutdown_sender, mut event_shutdown) = watch::channel(false);

        let event_targets = Arc::clone(&targets);
        let event_pending = Arc::clone(&pending);
        let event_task = crate::spawn_service_task(async move {
            let mut receiver =
                ThrottledLiveWatchReceiver::new(receiver, Duration::from_millis(250));
            loop {
                let event = tokio::select! {
                    shutdown = event_shutdown.changed() => {
                        if shutdown.is_err() || *event_shutdown.borrow() {
                            break;
                        }
                        continue;
                    }
                    event = receiver.recv() => {
                        let Some(event) = event else {
                            break;
                        };
                        event
                    }
                };
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
        // refreshTargets prevents completed workers from restoring work after project removal.
        // refreshTargets 防止已完成工作器在项目移除后重新写回任务。
        let refresh_targets = Arc::clone(&targets);
        let mut refresh_shutdown = shutdown_sender.subscribe();
        let refresh_task = crate::spawn_service_task(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut active_projects = BTreeSet::<String>::new();
            let mut workers = FuturesUnordered::<
                BoxFuture<'static, (String, VcePendingWatchBatch, Result<(), String>)>,
            >::new();
            loop {
                tokio::select! {
                    shutdown = refresh_shutdown.changed() => {
                        if shutdown.is_ok() && !*refresh_shutdown.borrow() {
                            continue;
                        }
                        // Stop admitting new batches, cancel every active project refresh, and
                        // drain workers so their staging cleanup and runtime gates can converge.
                        // 停止接纳新批次，取消所有活动项目刷新，并排空工作器，使暂存清理与运行时闸门收敛。
                        for project_id in &active_projects {
                            let _ = manager.cancel_refresh(project_id, None).await;
                        }
                        while workers.next().await.is_some() {}
                        break;
                    }
                    _ = interval.tick() => {
                        let ready_batches = {
                            let now = tokio::time::Instant::now();
                            let mut queued = refresh_pending.lock().await;
                            let project_ids = queued
                                .iter()
                                .filter(|(project_id, batch)| {
                                    !active_projects.contains(*project_id)
                                        && batch
                                            .retry_not_before
                                            .is_none_or(|deadline| deadline <= now)
                                })
                                .map(|(project_id, _)| project_id.clone())
                                .collect::<Vec<_>>();
                            project_ids
                                .into_iter()
                                .filter_map(|project_id| {
                                    queued.remove(&project_id).map(|batch| (project_id, batch))
                                })
                                .collect::<Vec<_>>()
                        };
                        for (project_id, batch) in ready_batches {
                            active_projects.insert(project_id.clone());
                            let worker_manager = manager.clone();
                            let worker_emitter = event_emitter.clone();
                            workers.push(Box::pin(async move {
                                let result = if batch.force_full {
                                    worker_manager
                                        .refresh_full(&project_id, RefreshReason::FileWatcher)
                                        .await
                                        .map(|_| ())
                                } else {
                                    worker_manager
                                        .refresh_paths(
                                            &project_id,
                                            batch.paths.clone(),
                                            RefreshReason::FileWatcher,
                                        )
                                        .await
                                        .map(|_| ())
                                };
                                super::services::emit_vce_runtime_changed(
                                    &worker_emitter,
                                    &worker_manager,
                                    &project_id,
                                )
                                .await;
                                (project_id, batch, result)
                            }));
                        }
                    }
                    completed = workers.next(), if !workers.is_empty() => {
                        let Some((project_id, batch, result)) = completed else {
                            continue;
                        };
                        active_projects.remove(&project_id);
                        if let Err(error) = result {
                            eprintln!(
                                "[webui-service] VCE watcher refresh failed for `{project_id}`: {error}"
                            );
                            requeue_vce_watch_batch(
                                &refresh_targets,
                                &refresh_pending,
                                project_id,
                                batch,
                                error == super::VCE_REFRESH_CANCELLED_ERROR,
                            )
                            .await;
                        }
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
                event_task: Mutex::new(Some(event_task)),
                refresh_task: Mutex::new(Some(refresh_task)),
                shutdown_sender,
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
        // registration starts the physical recursive observation before publication.
        // registration 在发布映射前启动物理递归观察。
        let registration = self.inner.subscriber.register_path(canonical.clone(), true);
        // registrations retains the RAII handle through the same publication critical section.
        // registrations 在同一发布临界区内保留 RAII 句柄。
        let mut registrations = self
            .inner
            .registrations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // targets blocks the event consumer until both ownership maps are coherent.
        // targets 阻塞事件消费者，直到两个所有权映射保持一致。
        let mut targets = self
            .inner
            .targets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Publish the target while retaining the physical registration under the same critical
        // section. The event consumer cannot observe a registered project without its root map.
        // 在同一临界区内发布目标并保留物理注册，事件消费者无法观察到缺少根目录映射的已注册项目。
        targets.insert(project_id.to_owned(), canonical);
        registrations.insert(
            project_id.to_owned(),
            VceWatchRegistration {
                _registration: registration,
            },
        );
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
        let _ = self.inner.shutdown_sender.send(true);
        let event_task = self
            .inner
            .event_task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let refresh_task = self
            .inner
            .refresh_task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(mut task) = event_task {
            let timed_out = tokio::time::timeout(Duration::from_secs(5), &mut task)
                .await
                .is_err();
            if timed_out {
                task.abort();
                let _ = task.await;
            }
        }
        if let Some(mut task) = refresh_task {
            let timed_out = tokio::time::timeout(Duration::from_secs(15), &mut task)
                .await
                .is_err();
            if timed_out {
                // The manager shutdown immediately preceding this call already joined every
                // project gate and converted abandoned generations into idempotent cleanup work.
                // 紧邻本调用之前的管理器关闭已汇合全部项目闸门，并把遗留代次转换为幂等清理任务。
                task.abort();
                let _ = task.await;
            }
        }
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

/// Requeue one failed watcher batch only while the project remains registered.
/// 仅在项目仍处于监听注册状态时重新排队失败批次。
///
/// # Parameters
/// 参数
/// `targets` is the authoritative set of watched project roots.
/// `targets` 是已监听项目根目录的权威集合。
/// `pending` retains coalesced work across transient failures.
/// `pending` 跨瞬时失败保留合并任务。
/// `project_id` identifies the failed project worker.
/// `project_id` 标识失败的项目工作器。
/// `batch` is the exact work removed before the failed attempt.
/// `batch` 是失败尝试前移出的精确工作批次。
/// `cancelled` distinguishes cooperative cancellation from an operational failure.
/// `cancelled` 区分协作式取消与操作失败。
async fn requeue_vce_watch_batch(
    targets: &RwLock<BTreeMap<String, PathBuf>>,
    pending: &AsyncMutex<BTreeMap<String, VcePendingWatchBatch>>,
    project_id: String,
    batch: VcePendingWatchBatch,
    cancelled: bool,
) {
    if !vce_watch_target_exists(targets, &project_id) {
        return;
    }
    // queued serializes retry restoration against watcher event coalescing.
    // queued 将重试恢复与监听事件合并串行化。
    let mut queued = pending.lock().await;
    // Recheck after acquiring the pending lock so an overlapping unwatch either clears this entry
    // afterwards or wins the target removal before insertion.
    // 获取待处理锁后再次检查，使并发取消监听要么随后清除此项，要么先移除目标并阻止插入。
    if !vce_watch_target_exists(targets, &project_id) {
        return;
    }
    // retry merges the failed batch with any newer events already waiting for the project.
    // retry 将失败批次与项目已经等待的较新事件合并。
    let retry = queued.entry(project_id).or_default();
    retry.paths.extend(batch.paths);
    retry.force_full |= batch.force_full;
    if cancelled {
        retry.retry_attempt = retry.retry_attempt.max(batch.retry_attempt);
        retry.retry_not_before = Some(tokio::time::Instant::now() + Duration::from_secs(1));
    } else {
        retry.retry_attempt = retry
            .retry_attempt
            .max(batch.retry_attempt.saturating_add(1));
        retry.retry_not_before =
            Some(tokio::time::Instant::now() + vce_watch_retry_delay(retry.retry_attempt));
    }
}

/// Return whether one project still owns a watcher target.
/// 返回指定项目是否仍持有监听目标。
///
/// # Parameters
/// 参数
/// `targets` is the authoritative project-root map.
/// `targets` 是权威项目根目录映射。
/// `project_id` is the stable workbench project identity.
/// `project_id` 是稳定工作台项目身份。
///
/// # Returns
/// 返回值
/// `true` only while watcher-driven convergence remains enabled for the project.
/// 仅在项目仍启用监听驱动收敛时返回 `true`。
fn vce_watch_target_exists(targets: &RwLock<BTreeMap<String, PathBuf>>, project_id: &str) -> bool {
    targets
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(project_id)
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
        // targets models one project whose watcher-driven convergence remains enabled.
        // targets 模拟一个仍启用监听驱动收敛的项目。
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

    /// A watcher storm is deduplicated per project and ignores paths outside registered roots.
    /// 监听事件风暴会按项目去重，并忽略已注册根目录之外的路径。
    #[test]
    fn watcher_storm_is_project_scoped_and_deduplicated() {
        let targets = RwLock::new(BTreeMap::from([
            (
                String::from("project-a"),
                PathBuf::from("/workspace/project-a"),
            ),
            (
                String::from("project-b"),
                PathBuf::from("/workspace/project-b"),
            ),
        ]));
        let changed_paths = (0..1_000)
            .flat_map(|index| {
                [
                    PathBuf::from(format!("/workspace/project-a/src/file-{}.rs", index % 10)),
                    PathBuf::from(format!("/workspace/project-b/src/file-{}.rs", index % 5)),
                    PathBuf::from("/workspace/outside/ignored.rs"),
                ]
            })
            .collect::<Vec<_>>();

        let grouped = group_vce_watch_events(&targets, &changed_paths);

        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped["project-a"].paths.len(), 10);
        assert_eq!(grouped["project-b"].paths.len(), 5);
        assert!(!grouped["project-a"].force_full);
        assert!(!grouped["project-b"].force_full);
    }

    /// Cancelled batches retry while watched, whereas removed projects cannot regain stale work.
    /// 已取消批次会在仍受监听时重试，而已移除项目不能重新产生陈旧任务。
    #[tokio::test]
    async fn cancelled_batches_requeue_only_for_registered_projects() {
        let targets = RwLock::new(BTreeMap::from([(
            String::from("project-a"),
            PathBuf::from("/workspace/project-a"),
        )]));
        // pending records work restored after a cooperative cancellation.
        // pending 记录协作式取消后恢复的任务。
        let pending = AsyncMutex::new(BTreeMap::new());
        // batch is the exact dirty set removed before the cancelled worker started.
        // batch 是已取消工作器启动前移出的精确脏集合。
        let batch = VcePendingWatchBatch {
            paths: BTreeSet::from([String::from("src/lib.rs")]),
            force_full: false,
            retry_attempt: 0,
            retry_not_before: None,
        };

        requeue_vce_watch_batch(&targets, &pending, String::from("project-a"), batch, true).await;
        // queued is the restored convergence work inspected by this regression test.
        // queued 是本回归测试检查的已恢复收敛任务。
        let queued = pending
            .lock()
            .await
            .remove("project-a")
            .expect("retry batch");
        assert_eq!(queued.paths, BTreeSet::from([String::from("src/lib.rs")]));
        assert_eq!(queued.retry_attempt, 0);
        assert!(queued.retry_not_before.is_some());

        targets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove("project-a");
        requeue_vce_watch_batch(
            &targets,
            &pending,
            String::from("project-a"),
            VcePendingWatchBatch {
                paths: BTreeSet::from([String::from("src/main.rs")]),
                force_full: false,
                retry_attempt: 1,
                retry_not_before: None,
            },
            false,
        )
        .await;
        assert!(pending.lock().await.is_empty());
    }
}
