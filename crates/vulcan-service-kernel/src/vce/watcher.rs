//! VCE subscriber over the service-owned shared workspace watcher.
//! 服务持有的共享工作区监听器之上的 VCE 订阅器。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant, SystemTime};

use futures_util::future::BoxFuture;
use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::sync::{Mutex as AsyncMutex, watch};
use vulcan_file_watcher::live::{
    LiveFileWatcherSubscriber, LiveWatchRegistration, ThrottledLiveWatchReceiver,
};
use vulcan_workspace_store::vce_data_plane::{VceJobState, VceStoredJob};

use crate::ServiceEventEmitter;
use crate::workspace_watcher::WorkspaceWatcherCenter;

use super::VceRuntimeManager;

#[path = "watcher_clock.rs"]
mod clock;
#[path = "watcher_freshness.rs"]
mod freshness;
use clock::{VCE_WATCH_OBSERVATION_INTERVAL, VceWatchDeliveryClock};
use freshness::VceWatchSourceObservation;
pub(super) use freshness::{VceSourceFreshness, VceSourceObservationTicket};

/// One coalesced dirty set waiting for a project refresh worker.
/// 等待项目刷新工作器处理的一份合并脏集合。
#[derive(Default)]
struct VcePendingWatchBatch {
    /// Exact delivered source boundary covered by these paths; absent without live observation evidence.
    /// 这些路径覆盖的精确投递源码边界；缺少存活观察证据时为空。
    observation: Option<VceSourceObservationTicket>,
    /// Exact canonical root that produced this dirty set; never replay onto a replacement root.
    /// 产生此脏集合的精确规范根；绝不重放到替换后的根。
    project_root: PathBuf,
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
    registration: LiveWatchRegistration,
}

/// Cloneable VCE watcher service with independent subscriber and retry queue.
/// 具有独立订阅者与重试队列的可克隆 VCE 监听服务。
#[derive(Clone)]
pub(crate) struct VceWatcherService {
    /// Shared implementation retained by service state clones.
    /// 由服务状态克隆共同持有的实现。
    inner: Arc<VceWatcherInner>,
}

/// Non-owning link used by checkpoint publication to attach an already-created host watcher.
/// 检查点发布用于挂接既有宿主监听器的非拥有链接。
#[derive(Clone)]
pub(super) struct VceWeakWatcherService(Weak<VceWatcherInner>);

impl VceWeakWatcherService {
    /// Upgrade the host link without restarting a stopped or released watcher service.
    /// 升级宿主链接，不重新启动已停止或释放的监听服务。
    /// Takes no parameters; returns the existing service only while its host retains ownership.
    /// 无参数；仅宿主仍持有所有权时返回既有服务。
    pub(super) fn upgrade(&self) -> Option<VceWatcherService> {
        self.0.upgrade().map(|inner| VceWatcherService { inner })
    }
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
    /// Source evidence is retained independently from pending queue removal and manual-mode unwatch.
    /// 源码证据独立于待处理队列移除及手动模式取消监听而保留。
    observations: Arc<Mutex<BTreeMap<String, VceWatchSourceObservation>>>,
    /// Actual event-task polling evidence; status readers may expire it but never renew it.
    /// 实际事件任务轮询证据；状态读取者可以使其过期，但绝不能续期。
    delivery_clock: Arc<Mutex<VceWatchDeliveryClock>>,
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
        // executor is the retained host runtime, never a per-request temporary executor.
        // executor 为保留宿主运行时，绝非单请求临时执行器。
        executor: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        let (subscriber, receiver) = center.subscribe();
        // registration_manager binds the completed host service after worker closures take their clones.
        // registration_manager 在工作闭包取得克隆后绑定完整宿主服务。
        let registration_manager = manager.clone();
        let targets = Arc::new(RwLock::new(BTreeMap::new()));
        let pending: Arc<AsyncMutex<BTreeMap<String, VcePendingWatchBatch>>> =
            Arc::new(AsyncMutex::new(BTreeMap::new()));
        // Track exact delivered boundaries without consulting user files or persistent indexes.
        // 跟踪精确投递边界，不查询用户文件或持久索引。
        let observations = Arc::new(Mutex::new(BTreeMap::new()));
        // One service-local clock observes delivery independently from project update timers.
        // 一个服务内时钟独立于项目更新计时器观察投递。
        let delivery_clock = Arc::new(Mutex::new(VceWatchDeliveryClock::new(
            Instant::now(),
            SystemTime::now(),
        )));
        let (shutdown_sender, mut event_shutdown) = watch::channel(false);

        let event_targets = Arc::clone(&targets);
        let event_pending = Arc::clone(&pending);
        // The event queue and diagnostic state acknowledge each exact event under the same root guard.
        // 事件队列和诊断状态在同一根守卫下确认每个精确事件。
        let event_observations = Arc::clone(&observations);
        // Only this event-task clone can renew successful consumer polling evidence.
        // 仅此事件任务克隆可以续期成功消费者轮询证据。
        let event_clock = Arc::clone(&delivery_clock);
        let event_task = crate::spawn_service_task(async move {
            let mut receiver =
                ThrottledLiveWatchReceiver::new(receiver, Duration::from_millis(250));
            // A bounded idle poll observes scheduling gaps without emitting file changes or scheduling indexing.
            // 有界空闲轮询观察调度缺口，不发出文件变化，也不调度索引。
            let mut observation_interval = tokio::time::interval(VCE_WATCH_OBSERVATION_INTERVAL);
            observation_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                {
                    // Lock order matches status sampling: delivery clock before source observations.
                    // 锁顺序与状态采样一致：先投递时钟，后源码观察。
                    let mut clock = event_clock
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let mut observations = event_observations
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    freshness::sample_source_delivery(&mut clock, &mut observations, true);
                }
                let event = tokio::select! {
                    shutdown = event_shutdown.changed() => {
                        if shutdown.is_err() || *event_shutdown.borrow() {
                            break;
                        }
                        continue;
                    }
                    _ = observation_interval.tick() => continue,
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
                enqueue_vce_watch_events(
                    &event_targets,
                    &event_pending,
                    &event_observations,
                    grouped,
                )
                .await;
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
                        // Each worker observes shutdown and cancels its exact job, never an unrelated manual job.
                        // 每个工作线程观察退出并取消精确作业，绝不取消无关手动作业。
                        while workers.next().await.is_some() {}
                        break;
                    }
                    _ = interval.tick() => {
                        // eligible is derived from live project policy and the last manual/automatic admission.
                        // eligible 从实时项目策略及最近手动或自动准入推导。
                        let candidates = refresh_pending.lock().await.keys().cloned().collect::<Vec<_>>();
                        let mut eligible = BTreeSet::new();
                        for project_id in candidates {
                            if !active_projects.contains(&project_id)
                                && matches!(manager.watcher_admission_delay(&project_id).await, Ok(Some(delay)) if delay.is_zero()) {
                                eligible.insert(project_id);
                            }
                        }
                        let ready_batches = {
                            let now = tokio::time::Instant::now();
                            let mut queued = refresh_pending.lock().await;
                            let project_ids = queued
                                .iter()
                                .filter(|(project_id, batch)| {
                                    !active_projects.contains(*project_id)
                                        && eligible.contains(*project_id)
                                        && vce_watch_target_matches(&refresh_targets, project_id, &batch.project_root)
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
                            let worker_executor = Arc::clone(&executor);
                            let worker_shutdown = refresh_shutdown.clone();
                            workers.push(Box::pin(async move {
                                let result = run_vce_watch_job(&worker_manager, &worker_executor, &project_id, &batch, worker_shutdown).await;
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
                            tracing::warn!(
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

        let service = Self {
            inner: Arc::new(VceWatcherInner {
                subscriber,
                registrations: Mutex::new(BTreeMap::new()),
                observations,
                delivery_clock,
                targets,
                pending,
                event_task: Mutex::new(Some(event_task)),
                refresh_task: Mutex::new(Some(refresh_task)),
                shutdown_sender,
            }),
        };
        registration_manager.bind_watcher(VceWeakWatcherService(Arc::downgrade(&service.inner)));
        service
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
                "Cannot normalize the VCE watched project directory `{}`: {error}",
                project_root.display()
            )
        })?;
        if !canonical.is_dir() {
            return Err(format!(
                "The VCE watched project path is not a directory: {}",
                canonical.display()
            ));
        }
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
        if *self.inner.shutdown_sender.borrow() {
            return Err(String::from("VCE watcher is stopping"));
        }
        if targets.get(project_id) == Some(&canonical) && registrations.contains_key(project_id) {
            return Ok(());
        }
        // Preserve an identical live registration, avoiding watcher gaps on every status refresh.
        // 保留相同活动注册，避免每次状态刷新制造监听空窗。
        let registration = self.inner.subscriber.register_path(canonical.clone(), true);
        // Re-registration always creates a continuity gap, even for the same project and root.
        // 重新注册始终产生连续性缺口，即使项目和根相同。
        let mut observations = self
            .inner
            .observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let observation = observations
            .entry(project_id.to_owned())
            .or_insert_with(|| VceWatchSourceObservation::new(canonical.clone()));
        if observation.project_root != canonical {
            *observation = VceWatchSourceObservation::new(canonical.clone());
        }
        observation.register(registration.observation_health());
        // Publish the target while retaining the physical registration under the same critical
        // section. The event consumer cannot observe a registered project without its root map.
        // 在同一临界区内发布目标并保留物理注册，事件消费者无法观察到缺少根目录映射的已注册项目。
        targets.insert(project_id.to_owned(), canonical);
        registrations.insert(project_id.to_owned(), VceWatchRegistration { registration });
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
        // Acquire the asynchronous queue before synchronous ownership locks; remove work in the same scope.
        // 在同步归属锁前取得异步队列锁；在同一作用域移除工作。
        let mut pending = self.inner.pending.lock().await;
        // Hold ownership until the diagnostic is retired, so a concurrent registration cannot be invalidated.
        // 保持归属直到诊断退役，避免使并发新注册失效。
        let mut registrations = self
            .inner
            .registrations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registrations.remove(project_id);
        self.inner
            .targets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(project_id);
        if let Some(observation) = self
            .inner
            .observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_mut(project_id)
        {
            observation.unwatch();
        }
        pending.remove(project_id);
    }

    /// Inspect a registered canonical root for isolated lifecycle integration tests.
    /// 为独立生命周期集成测试检查已注册的规范根。
    /// `project_id` selects the host registration. Returns its current root without changing observation.
    /// `project_id` 选择宿主注册。返回当前根，不改变观察。
    #[cfg(test)]
    pub(super) fn registered_root(&self, project_id: &str) -> Option<PathBuf> {
        self.inner
            .targets
            .read()
            .expect("fixture target lock")
            .get(project_id)
            .cloned()
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

/// Merge grouped physical events into the project-owned queue.
/// 将分组物理事件合并到项目所属队列。
/// `targets` owns live roots; `pending` owns queued work; `grouped` contains observed events.
/// `targets` 持有活动根；`pending` 持有排队工作；`grouped` 包含已观察事件。
/// Returns after merging events without starting index work.
/// 合并事件后返回，不启动索引工作。
async fn enqueue_vce_watch_events(
    targets: &RwLock<BTreeMap<String, PathBuf>>,
    pending: &AsyncMutex<BTreeMap<String, VcePendingWatchBatch>>,
    observations: &Mutex<BTreeMap<String, VceWatchSourceObservation>>,
    grouped: BTreeMap<String, VcePendingWatchBatch>,
) {
    // queued serializes physical events with retry restoration and project removal.
    // queued 将物理事件与重试恢复及项目移除串行化。
    let mut queued = pending.lock().await;
    // live_roots freezes registration ownership through insertion. Grouping happened before an await,
    // so a root replacement or unwatch may have invalidated the observed batch in the meantime.
    // live_roots 在插入期间固定注册归属。分组发生在等待前，
    // 因而期间替换根或取消监听可能已经使观察批次失效。
    let live_roots = targets
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (project_id, mut batch) in grouped {
        if live_roots.get(&project_id) != Some(&batch.project_root) {
            continue;
        }
        // Freeze the source boundary now, not later when a worker may observe unrelated newer changes.
        // 现在固定源码边界，而非稍后工作线程可能观察到无关较新变化时。
        if let Some(observation) = observations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_mut(&project_id)
            && observation.project_root == batch.project_root
        {
            batch.observation = observation.note_changes();
        }
        // target retains the batch associated with this stable project identity.
        // target 保留与此稳定项目身份关联的批次。
        let target = queued.entry(project_id).or_default();
        if target.project_root != batch.project_root {
            *target = VcePendingWatchBatch {
                project_root: batch.project_root.clone(),
                ..VcePendingWatchBatch::default()
            };
        }
        target.paths.extend(batch.paths);
        target.observation = batch.observation;
        target.force_full |= batch.force_full;
        bound_vce_watch_batch(target);
        // New source edits do not demonstrate recovery from a provider/storage failure.
        // Keep the existing retry gate; a successfully completed batch naturally consumes it.
        // 新源码编辑不代表供应商或存储故障已经恢复。
        // 保留既有重试闸门；成功完成的批次自然将其消费。
    }
}

/// Collapse oversized storms to one explicit full-reconciliation marker.
/// 将超大风暴收敛为一个明确完整核对标记。
/// `batch` owns exact relative paths. Returns nothing; memory stays bounded to 4096 paths.
/// `batch` 持有精确相对路径。无返回值；内存限制为四千零九十六个路径。
fn bound_vce_watch_batch(batch: &mut VcePendingWatchBatch) {
    if batch.force_full || batch.paths.len() > 4096 {
        batch.force_full = true;
        batch.paths.clear();
    }
}

/// Submit and follow one exact watcher job through the common durable coordinator.
/// 通过公共持久协调器提交并跟随一个精确监听作业。
///
/// `manager`, `executor`, and `project_id` select host ownership; `batch` freezes source scope.
/// `shutdown` cancels only this admitted job. Returns after its durable terminal outcome.
/// `manager`、`executor` 与 `project_id` 选择宿主归属；`batch` 冻结源码范围。
/// `shutdown` 仅取消此已准入作业。在其持久终态确定后返回。
async fn run_vce_watch_job(
    manager: &VceRuntimeManager,
    executor: &tokio::runtime::Runtime,
    project_id: &str,
    batch: &VcePendingWatchBatch,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    if *shutdown.borrow() {
        return Err(String::from(super::VCE_REFRESH_CANCELLED_ERROR));
    }
    let paths = if batch.force_full {
        None
    } else {
        Some(batch.paths.clone())
    };
    let mut receipt = manager
        .submit_watcher_job(
            project_id,
            &batch.project_root,
            paths,
            batch.observation.clone(),
            executor,
        )
        .await?;
    let mut cancel_requested = false;
    let mut interval = tokio::time::interval(Duration::from_millis(250));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if receipt.state.is_terminal() {
            return vce_watch_job_result(&receipt);
        }
        if *shutdown.borrow() && !cancel_requested {
            // A commit may reject cancellation; continue waiting for its authoritative receipt.
            // 提交可能拒绝取消；继续等待其权威回执。
            let _ = manager.cancel_job(project_id, &receipt.job_id).await;
            cancel_requested = true;
        }
        tokio::select! {
            changed = shutdown.changed(), if !cancel_requested => {
                if changed.is_err() {
                    let _ = manager.cancel_job(project_id, &receipt.job_id).await;
                    cancel_requested = true;
                }
            }
            _ = interval.tick() => {
                receipt = manager.get_job(project_id, Some(&receipt.job_id)).await?
                    .ok_or_else(|| String::from("VCE watcher job lost its durable receipt"))?;
            }
        }
    }
}

/// Convert a durable watcher terminal receipt to the coalescer's exact retry outcome.
/// 将持久监听终态回执转换为合并器的精确重试结果。
/// `receipt` is a validated terminal job. Returns success, cancellation, or an explicit failure.
/// `receipt` 为已校验终态作业。返回成功、取消或明确失败。
fn vce_watch_job_result(receipt: &VceStoredJob) -> Result<(), String> {
    match receipt.state {
        VceJobState::Succeeded => Ok(()),
        VceJobState::Cancelled => Err(String::from(super::VCE_REFRESH_CANCELLED_ERROR)),
        VceJobState::Failed | VceJobState::Interrupted => {
            Err(receipt.error.clone().unwrap_or_else(|| {
                String::from("VCE watcher job ended without a successful checkpoint")
            }))
        }
        _ => Err(String::from(
            "VCE watcher requested a terminal result for an active job",
        )),
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
    if !vce_watch_target_matches(targets, &project_id, &batch.project_root) {
        return;
    }
    // queued serializes retry restoration against watcher event coalescing.
    // queued 将重试恢复与监听事件合并串行化。
    let mut queued = pending.lock().await;
    // Recheck after acquiring the pending lock so an overlapping unwatch either clears this entry
    // afterwards or wins the target removal before insertion.
    // 获取待处理锁后再次检查，使并发取消监听要么随后清除此项，要么先移除目标并阻止插入。
    // Keep exact root authority through insertion, not only through a preliminary lookup.
    // 将精确根权威保持到插入完成，而不只保持到前置查询。
    let live_roots = targets
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if live_roots.get(&project_id) != Some(&batch.project_root) {
        return;
    }
    // retry merges the failed batch with any newer events already waiting for the project.
    // retry 将失败批次与项目已经等待的较新事件合并。
    let retry = queued
        .entry(project_id)
        .or_insert_with(|| VcePendingWatchBatch {
            observation: batch.observation.clone(),
            ..VcePendingWatchBatch::default()
        });
    retry.observation = match (&retry.observation, &batch.observation) {
        (Some(current), Some(failed)) => current.merge(failed),
        _ => None,
    };
    retry.project_root = batch.project_root;
    retry.paths.extend(batch.paths);
    retry.force_full |= batch.force_full;
    bound_vce_watch_batch(retry);
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
fn vce_watch_target_matches(
    targets: &RwLock<BTreeMap<String, PathBuf>>,
    project_id: &str,
    project_root: &Path,
) -> bool {
    targets
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(project_id)
        .is_some_and(|root| root == project_root)
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
            if !changed_path.starts_with(project_root) {
                continue;
            }
            let batch = grouped
                .entry(project_id.clone())
                .or_insert_with(|| VcePendingWatchBatch {
                    project_root: project_root.clone(),
                    ..VcePendingWatchBatch::default()
                });
            if changed_path == project_root {
                batch.force_full = true;
                bound_vce_watch_batch(batch);
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
                batch.force_full = true;
            } else {
                batch.paths.insert(relative);
            }
            bound_vce_watch_batch(batch);
        }
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserve failed-work backoff while merging later edits for the same root.
    /// 合并同根后续编辑时保留失败工作的退避。
    /// Takes no parameters; returns after asserting both dirty paths and the original retry deadline.
    /// 无参数；断言两份脏路径及原始重试期限后返回。
    #[tokio::test]
    async fn new_events_preserve_existing_retry_deadline() {
        // targets fixes the registered source identity for both event batches.
        // targets 固定两个事件批次的已注册源码身份。
        let targets = RwLock::new(BTreeMap::from([(
            String::from("project-a"),
            PathBuf::from("/workspace/project-a"),
        )]));
        // deadline models the existing retry gate independently of automatic admission cooldown.
        // deadline 模拟既有重试闸门，独立于自动准入冷却。
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        // pending retains failed work which must not be reset by source activity.
        // pending 保留不得被源码活动重置的失败工作。
        let pending = AsyncMutex::new(BTreeMap::from([(
            String::from("project-a"),
            VcePendingWatchBatch {
                project_root: PathBuf::from("/workspace/project-a"),
                paths: BTreeSet::from([String::from("old.rs")]),
                retry_attempt: 7,
                retry_not_before: Some(deadline),
                ..VcePendingWatchBatch::default()
            },
        )]));
        enqueue_vce_watch_events(
            &targets,
            &pending,
            &Mutex::new(BTreeMap::new()),
            group_vce_watch_events(&targets, &[PathBuf::from("/workspace/project-a/new.rs")]),
        )
        .await;
        // queued exposes the resulting retry state without submitting a writer.
        // queued 暴露合并后的重试状态，不提交写入者。
        let queued = pending.lock().await;
        assert_eq!(
            queued["project-a"].paths,
            BTreeSet::from([String::from("old.rs"), String::from("new.rs")])
        );
        assert_eq!(queued["project-a"].retry_attempt, 7);
        assert_eq!(queued["project-a"].retry_not_before, Some(deadline));
    }

    /// Reject already-grouped events after their root is replaced or removed.
    /// 根被替换或移除后拒绝此前已分组的事件。
    /// Takes no parameters; returns after proving stale delivery cannot overwrite current work.
    /// 无参数；证明陈旧投递不能覆盖当前工作后返回。
    #[tokio::test]
    async fn delayed_events_cannot_replace_current_root_queue() {
        // targets initially owns the root that generated delayed events.
        // targets 最初持有产生延迟事件的根。
        let targets = RwLock::new(BTreeMap::from([(
            String::from("project-a"),
            PathBuf::from("/workspace/previous"),
        )]));
        // delayed has already passed path grouping before the project root changes.
        // delayed 在项目根变化前已经通过路径分组。
        let delayed =
            group_vce_watch_events(&targets, &[PathBuf::from("/workspace/previous/old.rs")]);
        targets.write().expect("target lock").insert(
            String::from("project-a"),
            PathBuf::from("/workspace/current"),
        );
        // pending receives new-root events before delivery of the old batch.
        // pending 在旧批次投递前接收新根事件。
        let pending = AsyncMutex::new(BTreeMap::new());
        enqueue_vce_watch_events(
            &targets,
            &pending,
            &Mutex::new(BTreeMap::new()),
            group_vce_watch_events(&targets, &[PathBuf::from("/workspace/current/new.rs")]),
        )
        .await;
        enqueue_vce_watch_events(&targets, &pending, &Mutex::new(BTreeMap::new()), delayed).await;
        {
            // queued must still refer only to the replacement source tree.
            // queued 必须仍仅指向替换后的源码树。
            let queued = pending.lock().await;
            assert_eq!(
                queued["project-a"].project_root,
                PathBuf::from("/workspace/current")
            );
            assert_eq!(
                queued["project-a"].paths,
                BTreeSet::from([String::from("new.rs")])
            );
        }
        // removed models events grouped immediately before unwatch clears ownership.
        // removed 模拟取消监听清除所有权前刚分组的事件。
        let removed =
            group_vce_watch_events(&targets, &[PathBuf::from("/workspace/current/later.rs")]);
        targets.write().expect("target lock").clear();
        pending.lock().await.clear();
        enqueue_vce_watch_events(&targets, &pending, &Mutex::new(BTreeMap::new()), removed).await;
        assert!(pending.lock().await.is_empty());
    }

    /// Ensure event storms are bounded without losing the need for complete reconciliation.
    /// 确保事件风暴有界且不丢失完整核对需求。
    /// Takes no parameters. Returns nothing after asserting overflow retains only a full marker.
    /// 无参数。断言溢出仅保留全量标记后返回空值。
    #[test]
    fn watcher_overflow_keeps_one_full_reconciliation_marker() {
        let targets = RwLock::new(BTreeMap::from([(
            String::from("project-a"),
            PathBuf::from("/workspace/project-a"),
        )]));
        let paths = (0..6000)
            .map(|number| PathBuf::from(format!("/workspace/project-a/file-{number}.rs")))
            .collect::<Vec<_>>();
        let grouped = group_vce_watch_events(&targets, &paths);
        assert!(grouped["project-a"].force_full);
        assert!(grouped["project-a"].paths.is_empty());
        assert_eq!(
            grouped["project-a"].project_root,
            PathBuf::from("/workspace/project-a")
        );
    }

    /// Ensure a replaced project root cannot inherit dirty paths from a former registration.
    /// 确保替换后的项目根不能继承先前注册的脏路径。
    /// Takes no parameters. Returns after rejecting an old-root requeue with the same project ID.
    /// 无参数。同项目编号的旧根重新排队被拒绝后返回。
    #[tokio::test]
    async fn replaced_project_root_rejects_previous_dirty_batch() {
        let targets = RwLock::new(BTreeMap::from([(
            String::from("project-a"),
            PathBuf::from("/workspace/replaced"),
        )]));
        let pending = AsyncMutex::new(BTreeMap::new());
        let batch = VcePendingWatchBatch {
            project_root: PathBuf::from("/workspace/previous"),
            paths: BTreeSet::from([String::from("src/lib.rs")]),
            ..VcePendingWatchBatch::default()
        };
        requeue_vce_watch_batch(&targets, &pending, String::from("project-a"), batch, false).await;
        assert!(pending.lock().await.is_empty());
    }

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
            project_root: PathBuf::from("/workspace/project-a"),
            paths: BTreeSet::from([String::from("src/lib.rs")]),
            force_full: false,
            retry_attempt: 0,
            retry_not_before: None,
            observation: None,
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
                project_root: PathBuf::from("/workspace/project-a"),
                paths: BTreeSet::from([String::from("src/main.rs")]),
                force_full: false,
                retry_attempt: 1,
                retry_not_before: None,
                observation: None,
            },
            false,
        )
        .await;
        assert!(pending.lock().await.is_empty());
    }
}
