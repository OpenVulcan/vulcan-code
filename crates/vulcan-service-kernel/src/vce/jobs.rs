//! Durable human-owned indexing jobs, separated from conversation execution and source logic.
//! 持久的人为索引作业，独立于会话执行与源码逻辑。

use super::*;
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Notify, watch};
use vulcan_context_engine::{
    VcePhysicalCompactionOperation, VcePhysicalCompactionState, VcePhysicalTableRole,
    VcePhysicalTableStatistics, VceRefreshFailure, VceRefreshPhase, VceRefreshProgress,
    VceRefreshUnit,
};
use vulcan_workspace_store::vce_data_plane::{
    VceCompactionJobReceipt, VceCompactionMetrics, VceCompactionTableReceipt,
    VceCompactionTableState, VceCompletedJobUpdate, VceJobKind, VceJobState, VceJobStoreError,
    VceProjectIndex, VceProjectJobStore, VceProjectWriterLease, VceStoredJob,
};

/// Maximum number of additional attempts for transient progress-receipt contention.
/// 瞬时进度回执竞争允许的最大额外重试次数。
const VCE_PROGRESS_PERSIST_RETRIES: usize = 3;

/// Base delay for the bounded progress-receipt retry backoff.
/// 有界进度回执重试退避的基础延迟。
const VCE_PROGRESS_PERSIST_RETRY_BASE_MS: u64 = 250;

/// Maximum grace period before a shutdown aborts a worker that is stuck in terminal persistence.
/// 关闭期间等待终态持久化的最长宽限时间，超时后中止卡住的工作任务。
const VCE_JOB_SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Polling interval for one already-submitted storage compaction operation.
/// 已提交存储整理操作的轮询间隔。
const VCE_COMPACTION_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Retry one progress receipt update without blocking the indexing worker forever.
/// 在不永久阻塞索引工作任务的前提下重试一次进度回执更新。
///
/// `persist_progress` performs one durable update and must be reusable for each attempt;
/// `progress` is cloned because every retry submits the same latest observation.
/// `persist_progress` 执行一次持久化更新且必须可重复调用；`progress` 会被克隆，
/// 因为每次重试都提交同一份最新观察值。
/// Returns after the first success, after the bounded retry budget, or immediately for a
/// non-transient error.
/// 首次成功、达到有界重试预算或遇到非瞬时错误时返回。
async fn persist_progress_with_retry<P>(
    persist_progress: &P,
    progress: VceRefreshProgress,
) -> Result<(), VceJobStoreError>
where
    P: Fn(
            VceRefreshProgress,
        ) -> futures_util::future::BoxFuture<'static, Result<(), VceJobStoreError>>
        + Send
        + Sync
        + 'static,
{
    let mut last_error = None;
    for attempt in 0..=VCE_PROGRESS_PERSIST_RETRIES {
        match persist_progress(progress.clone()).await {
            Ok(()) => return Ok(()),
            Err(error) if error.is_busy() && attempt < VCE_PROGRESS_PERSIST_RETRIES => {
                last_error = Some(error);
                let delay_ms = VCE_PROGRESS_PERSIST_RETRY_BASE_MS * (attempt as u64 + 1);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        String::from("VCE progress persistence retry loop ended without an error").into()
    }))
}

#[path = "replacement_jobs.rs"]
mod replacement;
pub(crate) use replacement::VceRebuildProjectInput;

#[cfg(test)]
#[path = "job_query_tests.rs"]
mod query_tests;

/// Exact source work admitted by a human or the existing file watcher.
/// 人为操作或既有文件监听器准入的精确源码工作。
#[derive(Clone, serde::Serialize)]
enum VceJobWork {
    /// User-requested full manifest synchronization.
    /// 用户请求的完整清单同步。
    Manual,
    /// Explicitly confirmed full re-embedding, optionally preceded by authorized cleanup retries.
    /// 明确确认的完整重新向量化，可在此前重试已授权清理。
    Rebuild {
        /// Repair additionally checks unresolved staging before scanning; it never takes over unknown work.
        /// 修复还会在扫描前检查未确认暂存；绝不接管未知工作。
        repair: bool,
        /// Exact user-confirmed plan identities, also part of the durable replay fingerprint.
        /// 用户确认的精确计划身份，也是持久重放指纹的一部分。
        action_ids: BTreeSet<String>,
    },
    /// Exact dirty paths; absence explicitly requests root/overflow full reconciliation.
    /// 精确脏路径；缺失明确要求根目录或溢出的完整核对。
    FileChanges {
        /// Exact event boundary coalesced into paths; absent evidence requires a full manifest scan.
        /// 合并进路径的精确事件边界；缺少证据时要求完整清单扫描。
        observation: Option<super::super::watcher::VceSourceObservationTicket>,
        /// Canonical root frozen by the registration that generated these events.
        /// 生成这些事件的注册所冻结的规范根。
        project_root: std::path::PathBuf,
        /// Coalesced paths or an explicit full-reconciliation marker.
        /// 合并路径或明确完整核对标记。
        paths: Option<BTreeSet<String>>,
    },
}

/// Terminal host interpretation of one model-independent physical compaction job.
/// 一项模型无关物理整理作业的宿主终态解释。
enum VceCompactionJobOutcome {
    /// Every selected table reached the frozen compaction target.
    /// 每张已选表均达到冻结整理目标。
    Succeeded,
    /// Every selected table completed, and the last physical result still needs terminal persistence.
    /// 每张已选表均已完成，且最后一份物理结果仍需随终态一并持久化。
    SucceededWithTableUpdate(Box<VceCompactionTableTerminalUpdate>),
    /// One table failed with a deterministic execution error.
    /// 一张表因确定性执行错误失败。
    Failed(String),
    /// Execution failed after one engine result that must not be lost from the terminal receipt.
    /// 引擎已返回一份不能从终态回执中丢失的结果，随后执行失败。
    FailedWithTableUpdate {
        /// Bounded job-level failure diagnostic.
        /// 有界的作业级失败诊断。
        error: String,
        /// Exact table facts to merge into the same terminal compare-and-swap.
        /// 需要合并进同一次终态比较交换的精确表事实。
        table_update: Box<VceCompactionTableTerminalUpdate>,
    },
    /// User or shutdown cancellation settled at a safe boundary.
    /// 用户或退出取消已在安全边界确定。
    Cancelled,
    /// Cancellation settled after an atomic storage round completed and returned authoritative facts.
    /// 一轮存储原子操作完成并返回权威事实后，取消才进入确定状态。
    CancelledWithTableUpdate(Box<VceCompactionTableTerminalUpdate>),
    /// The host runtime lost authority needed to continue safely.
    /// 宿主运行时丢失了安全继续所需的权威状态。
    Interrupted(String),
    /// Authority was lost after a table-local terminal condition that must remain queryable.
    /// 表级终止条件已经确定后丢失继续执行所需权限，该事实必须保持可查询。
    InterruptedWithTableUpdate {
        /// Bounded job-level interruption diagnostic.
        /// 有界的作业级中断诊断。
        error: String,
        /// Exact table condition to merge into the terminal receipt.
        /// 需要合并进终态回执的精确表状态。
        table_update: Box<VceCompactionTableTerminalUpdate>,
    },
}

/// One authoritative table result retained until the host terminal receipt is durable.
/// 在宿主终态回执持久化之前保留的一份权威表结果。
struct VceCompactionTableTerminalUpdate {
    /// Deterministic slot in the frozen compaction plan.
    /// 冻结整理计划中的确定性槽位。
    table_index: usize,
    /// Immutable action identity proving the slot still represents the same confirmed plan.
    /// 用于证明该槽位仍表示同一确认计划的不可变操作身份。
    action_id: String,
    /// Submitted round that produced this terminal observation.
    /// 产生本次终止观察的已提交轮次。
    round: u32,
    /// Stable storage-operation identity, absent only before engine admission.
    /// 稳定的存储操作身份，仅在引擎准入前缺失。
    operation_id: Option<String>,
    /// Frozen physical version for the operation, absent only before engine admission.
    /// 操作冻结的物理版本，仅在引擎准入前缺失。
    operation_expected_version: Option<u64>,
    /// Latest engine statistics known to be authoritative.
    /// 最近一份已确认权威的引擎统计。
    latest_statistics: VcePhysicalTableStatistics,
    /// Cumulative metrics including the completed storage round when one exists.
    /// 累计指标；存在已完成存储轮次时包含该轮结果。
    metrics: VceCompactionMetrics,
    /// Terminal table state represented by this update.
    /// 本次更新表示的表终态。
    state: VceCompactionTableState,
    /// Bounded table-local diagnostic required by failure and interruption states.
    /// 失败及中断状态所需的有界表级诊断。
    error: Option<String>,
}

/// Acquire the process-wide compaction budget without abandoning an admitted storage operation.
/// 获取进程级整理预算，同时不得遗弃已被存储引擎准入的操作。
///
/// `writer_budget` serializes host writers, `cancel_wake` wakes host-only queued work, and
/// `has_admitted_operation` preserves operation ownership across host recovery. Returns a permit,
/// an immediate safe cancellation before engine admission, or an explicit closed-budget error.
/// `writer_budget` 串行化宿主写入，`cancel_wake` 唤醒仅在宿主排队的工作，
/// `has_admitted_operation` 在宿主恢复期间保留操作所有权。返回预算许可、引擎准入前的即时安全取消，
/// 或明确的预算关闭错误。
async fn acquire_compaction_writer_budget(
    writer_budget: Arc<tokio::sync::Semaphore>,
    cancel_wake: &Notify,
    has_admitted_operation: bool,
) -> Result<tokio::sync::OwnedSemaphorePermit, VceCompactionJobOutcome> {
    if has_admitted_operation {
        return writer_budget.acquire_owned().await.map_err(|_| {
            VceCompactionJobOutcome::Failed(String::from("VCE writer budget was closed"))
        });
    }
    let notified = cancel_wake.notified();
    tokio::pin!(notified);
    notified.as_mut().enable();
    tokio::select! {
        permit = writer_budget.acquire_owned() => permit.map_err(|_| {
            VceCompactionJobOutcome::Failed(String::from("VCE writer budget was closed"))
        }),
        _ = notified => Err(VceCompactionJobOutcome::Cancelled),
    }
}

/// Settled result of one engine-owned bounded compaction operation.
/// 一项引擎持有的有界整理操作的确定结果。
enum VceCompactionStepOutcome {
    /// Engine returned complete before/after statistics and metrics.
    /// 引擎返回了完整的前后统计与指标。
    Succeeded(Box<VcePhysicalCompactionOperation>),
    /// Engine recorded an execution failure.
    /// 引擎记录了执行失败。
    Failed(String),
    /// Engine confirmed cancellation before mutation.
    /// 引擎确认在变更前取消。
    Cancelled,
    /// A previously admitted operation is no longer queryable.
    /// 先前已准入的操作已无法查询。
    StateLost(String),
}

/// Runtime-only project scheduling policy and the monotonic last admission time.
/// 仅运行时项目调度策略与单调最近准入时刻。
pub(super) struct VceJobSchedule {
    /// Exact saved project policy; manual is never inferred from job state.
    /// 精确保存的项目策略；手动模式绝不从作业状态推断。
    automatic: bool,
    /// Saved minimum interval between automatic admissions, also measured after manual jobs.
    /// 保存的自动准入最低间隔；手动作业之后也从此计量。
    minimum_interval: Duration,
    /// Last accepted manual or automatic job in this runtime.
    /// 此运行时最近受理的手动或自动作业。
    last_admitted: Option<tokio::time::Instant>,
}

impl VceJobSchedule {
    /// Construct scheduling from validated project settings without scheduling any work.
    /// 从已校验项目设置构建调度，不安排任何工作。
    /// `settings` is authoritative; returns an initially idle scheduling record.
    /// `settings` 为权威值；返回初始空闲调度记录。
    pub(super) fn new(settings: &ProjectVceSettings) -> Self {
        Self {
            automatic: settings.update_mode
                == vulcan_workbench_model::ProjectVceUpdateMode::Automatic,
            minimum_interval: Duration::from_secs(u64::from(
                settings.auto_update_min_interval_seconds,
            )),
            last_admitted: None,
        }
    }

    /// Update only policy, preserving the last admission and any already running job.
    /// 仅更新策略，保留最近准入与已经运行的作业。
    /// `settings` is the new persisted revision. Returns no value and never touches storage.
    /// `settings` 是新持久修订。无返回值，绝不触碰存储。
    pub(super) fn update(&mut self, settings: &ProjectVceSettings) {
        self.automatic =
            settings.update_mode == vulcan_workbench_model::ProjectVceUpdateMode::Automatic;
        self.minimum_interval =
            Duration::from_secs(u64::from(settings.auto_update_min_interval_seconds));
    }

    /// Restore a persisted admission into the monotonic clock without scheduling source work.
    /// 将持久准入恢复到单调时钟，不安排源码处理。
    /// `admitted_ms` and `wall_ms` are Unix milliseconds; `now` is the current monotonic instant.
    /// Returns no value; clock rollback conservatively retains one complete minimum interval.
    /// `admitted_ms` 与 `wall_ms` 为时间戳毫秒；`now` 为当前单调时刻。
    /// 无返回值；时钟回退时保守保留一个完整最低间隔。
    pub(super) fn restore_admission(
        &mut self,
        admitted_ms: u64,
        wall_ms: u64,
        now: tokio::time::Instant,
    ) {
        // elapsed is bounded by validated project policy, avoiding ancient-clock underflow.
        // elapsed 受已校验项目策略约束，避免历史时间导致时钟下溢。
        let elapsed =
            Duration::from_millis(wall_ms.saturating_sub(admitted_ms)).min(self.minimum_interval);
        self.last_admitted = Some(now - elapsed);
    }

    /// Return the saved automatic policy independently of the current cooldown.
    /// 独立于当前冷却返回已保存的自动策略。
    /// Takes no parameters; returns whether observation is requested by project settings.
    /// 无参数；返回项目设置是否请求观察文件。
    pub(super) fn automatic(&self) -> bool {
        self.automatic
    }

    /// Calculate remaining automatic delay without applying it to human commands.
    /// 计算剩余自动延时，不将其应用于人为命令。
    /// `now` is monotonic. Returns no scheduling permission in manual mode, otherwise remaining delay.
    /// `now` 为单调时刻。手动模式返回无调度许可，否则返回剩余延时。
    fn automatic_delay(&self, now: tokio::time::Instant) -> Option<Duration> {
        self.automatic.then(|| {
            self.last_admitted
                .map(|last| {
                    self.minimum_interval
                        .saturating_sub(now.saturating_duration_since(last))
                })
                .unwrap_or(Duration::ZERO)
        })
    }
}

/// Exact validated human command; it never grants implicit migration or model compatibility.
/// 精确已校验人为命令；绝不授予隐式迁移或模型兼容授权。
pub(crate) struct VceJobAdmission {
    /// Optional exact durable retry origin; a new click always receives a new job identity.
    /// 可选精确持久重试来源；新点击始终取得新作业身份。
    pub(crate) retry_of_job_id: Option<String>,
    /// Human-selected operation; rebuilding also requires the confirmed maintenance entry point.
    /// 人为选择的操作；重建还要求从已确认维护入口进入。
    pub(crate) kind: VceJobKind,
    /// Caller-generated stable replay key, reused only for the same button submission.
    /// 调用方生成的稳定重放键，仅在同一次按钮提交时复用。
    pub(crate) idempotency_key: String,
    /// Project settings revision observed by the human command.
    /// 人为命令观察到的项目设置修订。
    pub(crate) expected_settings_revision: u64,
    /// Exact checkpoint observed before submission, absent only before initialization.
    /// 提交前观察到的精确检查点，仅在初始化前缺失。
    pub(crate) expected_checkpoint_version: Option<u64>,
    /// Current admitted physical embedding contract, not a model display name.
    /// 当前准入物理向量契约，而非模型展示名。
    pub(crate) contract_fingerprint: String,
}

/// Exact bounded receipt selection shared by active and cold project storage reads.
/// 活动与冷态项目存储读取共享的精确有界回执选择。
enum VceJobSelection<'a> {
    /// Most recently admitted receipt, regardless of outcome.
    /// 最近准入回执，不限结果。
    Latest,
    /// One stable job identity, never a scan through history.
    /// 一个稳定作业身份，绝不扫描历史。
    Exact(&'a str),
    /// Most recent successful content update, not a migration or failed attempt.
    /// 最近成功内容更新，不是迁移或失败尝试。
    LastSuccessfulUpdate,
}

impl VceJobSelection<'_> {
    /// Execute the selected bounded read against the already-owned project repository.
    /// 对已经持有的项目仓库执行选定有界读取。
    /// `store` supplies physical ownership. Returns the exact optional durable receipt.
    /// `store` 提供物理归属。返回精确可选持久回执。
    async fn read(&self, store: &VceProjectJobStore) -> Result<Option<VceStoredJob>, String> {
        match self {
            Self::Latest => store.latest().await,
            Self::Exact(id) => store.get(id).await.map_err(Into::into),
            Self::LastSuccessfulUpdate => store.latest_successful_update().await,
        }
    }
}

/// One active job with durable receipt ownership and an independent cancellation channel.
/// 一个具有持久回执归属和独立取消通道的活动作业。
pub(super) struct VceManagedJob {
    /// Immutable durable identity used to match cancellation without reading mutable receipt state.
    /// 用于匹配取消请求的不可变持久身份，不读取可变回执状态。
    job_id: String,
    /// Latest saved non-index policy for replacement handoff; ordinary jobs have no separate handoff.
    /// 替换交接使用的最新已保存非索引策略；普通作业没有独立交接。
    replacement_settings: Option<Mutex<ProjectVceSettings>>,
    /// One shared OS lease spans coalesced batches and remains held after failed final persistence.
    /// 同一个共享操作系统租约覆盖合并批次，并在最终持久化失败后继续持有。
    project_writer: Arc<VceProjectWriterLease>,
    /// Exact durable repository retained by the job, not borrowed from a possibly absent model runtime.
    /// 由作业保留的精确持久仓库，不借用可能缺失的模型运行时。
    store: VceProjectJobStore,
    /// Publish only acknowledged receipt changes through the existing service event boundary.
    /// 通过既有服务事件边界仅发布已确认回执变化。
    event_emitter: ServiceEventEmitter,
    /// Only acknowledged persisted receipts enter this projection.
    /// 仅确认已持久化的回执可以进入此投影。
    receipt: Mutex<VceStoredJob>,
    /// Serialize durable receipt transitions without holding the receipt snapshot lock over I/O.
    /// 串行化持久回执转换，但不在 I/O 期间持有回执快照锁。
    persistence_gate: Mutex<()>,
    /// Shared atomic boundary used by both scanning and core publication.
    /// 扫描与核心发布共同使用的共享原子边界。
    control: VceRefreshControl,
    /// Wake a queued worker immediately when cancellation is requested.
    /// 请求取消时立即唤醒排队工作线程。
    cancel_wake: Notify,
    /// Persistence failure is explicit, never represented by invented successful progress.
    /// 持久化失败明确暴露，绝不表示为虚构的成功进度。
    persistence_error: Mutex<Option<String>>,
    /// Exact terminal successor retained when its compare-and-swap could not be acknowledged.
    /// 终态比较交换未获确认时保留的精确后继回执。
    pending_terminal_receipt: Mutex<Option<VceStoredJob>>,
    /// Test-only count proving advisory compaction observations can fail without losing ownership.
    /// 仅供测试统计，用于证明整理观察值失败时不会丢失操作所有权。
    #[cfg(test)]
    compaction_observation_persistence_failures: std::sync::atomic::AtomicUsize,
}

/// Stable control for one project's job, independent of query engine and model registration lifetimes.
/// 单个项目的稳定作业控制，独立于查询引擎及模型注册生命周期。
#[derive(Default)]
pub(super) struct VceProjectJobState {
    /// Serialize submission, cancellation and worker handoff for this exact project.
    /// 串行化此精确项目的提交、取消及工作任务交接。
    pub(super) admission: Mutex<()>,
    /// Retain the exact admitted owner until terminal persistence succeeds.
    /// 保留精确准入所有者，直到终态持久化成功。
    pub(super) active: Mutex<Option<Arc<VceManagedJob>>>,
    /// Read leases belong to admitted workers through terminal acknowledgement, independent of model runtimes.
    /// 读取租约归已准入工作任务持有直至终态确认，独立于模型运行时。
    execution: Arc<RwLock<()>>,
    /// Stop successor admission even when the current one-shot control has already completed its commit.
    /// 即使当前一次性控制已经完成提交，也停止后继准入。
    stop_requested: AtomicBool,
    /// Retain the spawned worker handle so shutdown can abort a task stuck on a locked receipt.
    /// 保留工作任务句柄，使关闭时可以中止因回执持锁而卡住的任务。
    worker_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl VceProjectJobState {
    /// Fence worker handoff before removing a runtime; the caller separately drains admitted execution.
    /// 移除运行时之前封锁工作任务交接；调用方另行排空已准入执行。
    /// No parameters. Returns after cancelling the exact active owner without waiting for completion.
    /// 无参数。在取消精确活动所有者后返回，不等待完成。
    pub(super) async fn request_stop(&self) {
        // admission makes the stop visible before a replacement worker can install its query runtime.
        // admission 保证替换工作任务安装查询运行时之前可以看见停止请求。
        let _admission = self.admission.lock().await;
        self.stop_requested.store(true, Ordering::Release);
        if let Some(job) = self.active.lock().await.as_ref() {
            job.cancel_for_shutdown();
        }
    }

    /// Install the spawned worker handle while the caller still holds the admission fence.
    /// 在调用方仍持有准入栅栏时安装工作任务句柄。
    async fn install_worker_handle(&self, handle: tokio::task::JoinHandle<()>) {
        *self.worker_handle.lock().await = Some(handle);
    }

    /// Clear the worker handle after normal completion without retaining a detached task handle.
    /// 正常完成后清除工作任务句柄，避免保留已脱离的任务句柄。
    async fn clear_worker_handle(&self) {
        let _admission = self.admission.lock().await;
        let _ = self.worker_handle.lock().await.take();
    }

    /// Take the worker handle under the admission fence so shutdown can await or abort it safely.
    /// 在准入栅栏下取出工作任务句柄，使退出流程可以安全等待或中止它。
    async fn take_worker_handle_for_shutdown(&self) -> Option<tokio::task::JoinHandle<()>> {
        let _admission = self.admission.lock().await;
        self.worker_handle.lock().await.take()
    }

    /// Cancel the admitted owner and drain its worker without holding the transition lock during completion.
    /// 取消已准入所有者并排空其任务，完成期间不持有转换锁。
    /// Caller must exclude new submissions with the project or manager lifecycle gate.
    /// 调用方必须通过项目或管理器生命周期闸门排斥新提交。
    pub(super) async fn cancel_and_drain(&self) -> Result<(), String> {
        self.request_stop().await;
        // A locked receipt must not make application shutdown wait forever; the tracked worker is
        // aborted only after the normal cooperative drain grace period expires.
        // 锁住的回执不能让应用退出永久等待；仅在正常协作排空宽限期结束后中止已跟踪任务。
        let (drained, mut forced_abort) = match tokio::time::timeout(
            VCE_JOB_SHUTDOWN_DRAIN_TIMEOUT,
            self.execution.write(),
        )
        .await
        {
            Ok(drained) => (drained, false),
            Err(_) => {
                let Some(worker) = self.take_worker_handle_for_shutdown().await else {
                    return Err(String::from(
                        "VCE worker did not drain and no tracked worker handle was available",
                    ));
                };
                tracing::warn!(
                    "VCE worker exceeded shutdown drain grace period; aborting it before releasing project leases"
                );
                worker.abort();
                let _ = worker.await;
                let drained = tokio::time::timeout(
                    VCE_JOB_SHUTDOWN_DRAIN_TIMEOUT,
                    self.execution.write(),
                )
                .await
                .map_err(|_| {
                    String::from(
                        "VCE worker abort completed without releasing its project execution lease",
                    )
                })?;
                (drained, true)
            }
        };
        if !forced_abort {
            // Execution may have drained while terminal receipt persistence still owns the job lease.
            // 即使执行租约已排空，终态回执持久化仍可能继续持有作业租约。
            if let Some(mut worker) = self.take_worker_handle_for_shutdown().await {
                match tokio::time::timeout(VCE_JOB_SHUTDOWN_DRAIN_TIMEOUT, &mut worker).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        tracing::warn!(
                            error = %error,
                            "VCE worker ended abnormally while draining terminal persistence"
                        );
                    }
                    Err(_) => {
                        tracing::warn!(
                            "VCE worker exceeded shutdown drain grace period during terminal persistence; aborting it"
                        );
                        worker.abort();
                        let _ = worker.await;
                        forced_abort = true;
                    }
                }
            }
        }
        let active = self.active.lock().await.take();
        if let Some(job) = active {
            let existing_persistence_error = job.persistence_error.lock().await.clone();
            let error = existing_persistence_error.clone().unwrap_or_else(|| {
                String::from(
                    "VCE project job owner survived worker drain; explicit recovery is required",
                )
            });
            if forced_abort || existing_persistence_error.is_some() {
                // The durable receipt remains recoverable by the next admission after a forced abort.
                // 强制中止或已知回执写入失败后由下一次准入恢复未完成的持久回执。
                tracing::warn!(
                    job_id = %job.job_id,
                    forced_abort,
                    "VCE shutdown left an interrupted receipt for next-admission recovery"
                );
            } else {
                // Settle the durable receipt while the exact owner and its storage binding are still available.
                // 在精确所有者及其存储绑定仍可用时确定持久回执。
                let recovery = tokio::time::timeout(
                    VCE_JOB_SHUTDOWN_DRAIN_TIMEOUT,
                    job.persist_interrupted_after_drain(error.clone()),
                )
                .await
                .map_err(|_| {
                    String::from(
                        "VCE shutdown interruption receipt persistence exceeded its grace period",
                    )
                })
                .and_then(|result| result);
                if let Err(recovery_error) = recovery {
                    // The owner is still released below; an unresolved receipt is recoverable on the next
                    // admission because the writer lease proves this host no longer owns live work.
                    // 下面仍会释放所有者；未确定的回执可在下次准入时恢复，因为写入租约证明本宿主已无活动工作。
                    let project_id = job.receipt.lock().await.project_id.clone();
                    tracing::error!(
                        project_id = %project_id,
                        job_id = %job.job_id,
                        error = %recovery_error,
                        "failed to persist interrupted VCE job during shutdown"
                    );
                }
            }
            // The execution write lease proves the worker is gone; drop the owner before returning so
            // shutdown can close the project binding even when the last receipt write was busy.
            // 执行写租约证明工作线程已经结束；返回前释放所有者，使最后一次回执写入遇到忙状态时退出仍可关闭项目绑定。
            drop(job);
            self.stop_requested.store(false, Ordering::Release);
            drop(drained);
            return Ok(());
        }
        // The execution write lease proves no old worker can observe the reset and start another batch.
        // 执行写租约证明没有旧工作任务能观察到复位并开始另一批。
        self.stop_requested.store(false, Ordering::Release);
        drop(drained);
        Ok(())
    }
}

impl VceManagedJob {
    /// Cancel this exact job during teardown and wake a pending budget wait.
    /// 拆卸期间取消此精确作业，并唤醒等待预算的流程。
    ///
    /// Takes no parameters; does not cancel or mutate a conversation.
    /// 无参数；不取消或修改会话。
    pub(super) fn cancel_for_shutdown(&self) {
        self.control.cancel();
        self.cancel_wake.notify_one();
    }

    /// Persist a serialized monotonic receipt transition before exposing it to readers.
    /// 向读取者暴露之前持久化串行单调回执转换。
    ///
    /// `store` owns this project's physical database and `change` edits the proposed successor.
    /// Returns the acknowledged receipt; failures leave the last acknowledged value unchanged.
    /// `store` 持有本项目物理数据库，`change` 编辑拟议后继。
    /// 返回已确认回执；失败时保留最近确认值不变。
    async fn persist(
        &self,
        store: &VceProjectJobStore,
        change: impl FnOnce(&mut VceStoredJob),
    ) -> Result<VceStoredJob, VceJobStoreError> {
        self.persist_checked(store, |receipt| {
            change(receipt);
            Ok(())
        })
        .await
    }

    /// Persist a checked receipt transition and retain an unacknowledged terminal successor exactly.
    /// 持久化一条经过校验的回执转换，并精确保留未获确认的终态后继。
    ///
    /// `store` owns the physical project and `change` may reject an inconsistent runtime projection
    /// before any write. Returns the acknowledged receipt; a failed terminal compare-and-swap leaves
    /// the exact proposed successor available for explicit retry instead of reconstructing its facts.
    /// `store` 持有物理项目，`change` 可在写入前拒绝不一致的运行时投影。返回已确认回执；
    /// 终态比较交换失败时保留精确拟议后继，以供明确重试，而不是重新推测其中事实。
    async fn persist_checked(
        &self,
        store: &VceProjectJobStore,
        change: impl FnOnce(&mut VceStoredJob) -> Result<(), String>,
    ) -> Result<VceStoredJob, VceJobStoreError> {
        // The gate serializes compare-and-swap transitions while the receipt lock remains short-lived.
        // 闸门串行化比较交换转换，同时让回执锁只覆盖短暂的内存快照。
        let _persistence = self.persistence_gate.lock().await;
        let previous = self.receipt.lock().await.clone();
        let mut next = previous.clone();
        change(&mut next)?;
        if next == previous {
            return Ok(next);
        }
        next.sequence = previous
            .sequence
            .checked_add(1)
            .ok_or_else(|| String::from("VCE job receipt sequence overflow"))?;
        next.updated_at_unix_ms = job_time()?.max(previous.updated_at_unix_ms);
        let persisted = match store.update(&previous, &next).await {
            Ok(persisted) => persisted,
            Err(error) => {
                if next.state.is_terminal() {
                    *self.pending_terminal_receipt.lock().await = Some(next);
                }
                return Err(error);
            }
        };
        // No other receipt transition can pass the gate while this compare-and-swap is in flight.
        // 此比较交换进行期间没有其他回执转换可以通过闸门。
        *self.receipt.lock().await = persisted.clone();
        if persisted.state.is_terminal() {
            *self.pending_terminal_receipt.lock().await = None;
        }
        self.event_emitter
            .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                super::super::services::map_vce_job(persisted.clone()),
            )));
        Ok(persisted)
    }

    /// Reconcile one exact active receipt to `Interrupted` after its worker has fully drained.
    /// 在工作任务完全排空后，将一个精确活动回执收敛为 `Interrupted`。
    ///
    /// No provider or source work is resumed here; the method only adopts an already committed
    /// terminal receipt or records a bounded shutdown interruption before the owner is dropped.
    /// 本方法不恢复供应商或源码工作；仅采用已经提交的终态回执，或在释放所有者前记录有界的退出中断。
    async fn persist_interrupted_after_drain(
        &self,
        diagnostic: String,
    ) -> Result<VceStoredJob, String> {
        let _persistence = self.persistence_gate.lock().await;
        let current = self
            .store
            .get_after_storage_retry(&self.job_id)
            .await?
            .ok_or_else(|| String::from("VCE drained job receipt does not exist"))?;
        if current.state.is_terminal() {
            // An ambiguous terminal write already settled the job; never overwrite it as interrupted.
            // 结果不明确的终态写入已经确定作业；绝不把它覆盖为中断。
            *self.receipt.lock().await = current.clone();
            *self.persistence_error.lock().await = None;
            *self.pending_terminal_receipt.lock().await = None;
            return Ok(current);
        }
        let mut next = current.clone();
        next.state = VceJobState::Interrupted;
        next.followup_requested = false;
        next.error = Some(bounded_error(format!(
            "VCE job interrupted during shutdown: {diagnostic}"
        )));
        next.sequence = current
            .sequence
            .checked_add(1)
            .ok_or_else(|| String::from("VCE interruption sequence overflow"))?;
        next.updated_at_unix_ms = job_time()?.max(current.updated_at_unix_ms);
        let persisted = self.store.update(&current, &next).await?;
        *self.receipt.lock().await = persisted.clone();
        *self.persistence_error.lock().await = None;
        *self.pending_terminal_receipt.lock().await = None;
        self.event_emitter
            .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                super::super::services::map_vce_job(persisted.clone()),
            )));
        Ok(persisted)
    }
}

impl VceRuntimeManager {
    /// Counts retained project job controls for the protected status endpoint.
    /// 为受保护状态端点统计保留的项目作业控制数。
    ///
    /// # Returns
    /// 返回值
    /// Best-effort job-control gauge; a contended lock reports zero.
    /// 尽力而为的作业控制度量；锁竞争时报告零。
    pub(crate) fn retained_job_control_gauge(&self) -> u64 {
        self.project_job_states
            .try_lock()
            .map(|states| states.len() as u64)
            .unwrap_or(0)
    }

    /// Resolve the stable project job slot without opening storage, registering models or starting work.
    /// 解析稳定项目作业槽位，不打开存储、注册模型或启动处理。
    /// `project_id` is already validated by the owning lifecycle entry; returns shared metadata-only control.
    /// `project_id` 已由所属生命周期入口校验；返回仅含元数据的共享控制。
    pub(super) async fn project_job_state(&self, project_id: &str) -> Arc<VceProjectJobState> {
        self.project_job_states
            .lock()
            .await
            .entry(project_id.to_owned())
            .or_insert_with(|| Arc::new(VceProjectJobState::default()))
            .clone()
    }

    /// Admit a durable explicit job and return without scanning or waiting for any model call.
    /// 准入持久显式作业并返回，不扫描、不等待任何模型调用。
    ///
    /// `project_id` selects the activated project, `command` freezes user intent, and `executor`
    /// is the retained service runtime. Returns a durable queued receipt or an idempotent replay.
    /// `project_id` 选择已激活项目，`command` 冻结用户意图，`executor` 是保留的服务运行时。
    /// 返回持久排队回执或幂等重放。
    pub(crate) async fn submit_job(
        &self,
        project_id: &str,
        command: VceJobAdmission,
        executor: &tokio::runtime::Runtime,
    ) -> Result<VceStoredJob, String> {
        self.submit_index_job(project_id, command, VceJobWork::Manual, executor)
            .await
    }

    /// Admit a confirmed maintenance rebuild into the existing durable, cancellable writer queue.
    /// 将已确认维护重建准入既有持久、可取消的写入队列。
    /// `project_id` and `command` freeze the validated plan authority; `repair` selects cleanup checks.
    /// `project_id` 与 `command` 固定已验证计划权威；`repair` 选择清理检查。
    /// `action_ids` is the exact confirmed plan, checked under the writer gate and hashed for replay.
    /// `action_ids` 为精确确认计划，在写入锁内检查并纳入重放摘要。
    /// `executor` owns the worker lifetime. Returns an admitted receipt without waiting for indexing.
    /// `executor` 持有工作线程生命周期。返回准入回执，不等待索引完成。
    pub(crate) async fn submit_confirmed_rebuild(
        &self,
        project_id: &str,
        command: VceJobAdmission,
        repair: bool,
        action_ids: BTreeSet<String>,
        executor: &tokio::runtime::Runtime,
    ) -> Result<VceStoredJob, String> {
        self.submit_index_job(
            project_id,
            command,
            VceJobWork::Rebuild { repair, action_ids },
            executor,
        )
        .await
    }

    /// Admit a confirmed model-independent physical compaction job.
    /// 准入一项已确认且模型无关的物理整理作业。
    ///
    /// `project_id`, `command`, and `action_ids` freeze the stored checkpoint and exact current
    /// plan; `executor` owns the tracked background worker. Returns immediately after durable
    /// admission or returns the exact idempotent replay.
    /// `project_id`、`command` 与 `action_ids` 冻结已存检查点及精确当前计划；`executor` 持有
    /// 受跟踪后台任务。持久准入后立即返回，或返回精确幂等重放。
    pub(crate) async fn submit_physical_compaction(
        &self,
        project_id: &str,
        command: VceJobAdmission,
        action_ids: BTreeSet<String>,
        executor: &tokio::runtime::Runtime,
    ) -> Result<VceStoredJob, String> {
        validate_compaction_command(&command, &action_ids)?;
        let _lifecycle = self.lifecycle_gate.read().await;
        let project_gate = self.project_lifecycle_gate(project_id).await;
        let _project_gate = project_gate.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(String::from("VCE job manager is shutting down"));
        }
        let state = self.project_job_state(project_id).await;
        let _admission = state.admission.lock().await;
        let config = self
            .data_plane
            .load_active_project_config(project_id)
            .await?
            .ok_or_else(|| String::from("VCE physical compaction requires an active index"))?;
        let index = self
            .data_plane
            .open_existing_project_index(&config)
            .await?
            .ok_or_else(|| String::from("VCE physical compaction index disappeared"))?;
        index.require_write_access()?;
        let provenance = index.embedding_provenance().await?;
        validate_compaction_target(&command, &config, provenance.checkpoint_version)?;
        let store = index.job_store();
        let request_fingerprint =
            compaction_request_fingerprint(project_id, &command, &action_ids)?;
        if let Some(existing) = store
            .find_submission(&command.idempotency_key, &request_fingerprint)
            .await?
        {
            return Ok(existing);
        }
        if let Some(active) = state.active.lock().await.clone() {
            let receipt = active.receipt.lock().await.clone();
            if receipt.idempotency_key == command.idempotency_key
                && receipt.request_fingerprint == request_fingerprint
            {
                return Ok(receipt);
            }
            return Err(String::from(
                "VCE project already has an admitted index or maintenance job",
            ));
        }
        let project_writer = self
            .data_plane
            .try_acquire_project_writer(project_id)?
            .ok_or_else(|| String::from("VCE project already has an active host writer"))?;
        if let Some(previous) = store.unfinished().await? {
            if previous.kind == VceJobKind::Compact {
                return Err(format!(
                    "VCE physical compaction job `{}` is still unfinished; query that job before submitting another",
                    previous.job_id
                ));
            }
            let reason = if previous.owner_run_id == self.job_owner_run_id {
                "VCE previous job ended before its terminal receipt was acknowledged"
            } else {
                "VCE previous host run ended before its terminal receipt was acknowledged"
            };
            store.interrupt_unfinished(reason).await?;
        }
        let plan = index
            .physical_compaction_plan()
            .await
            .map_err(|error| error.to_string())?;
        let planned_ids = plan
            .actions
            .iter()
            .map(|action| action.action_id.clone())
            .collect::<BTreeSet<_>>();
        if planned_ids.is_empty() || planned_ids != action_ids {
            return Err(String::from(
                "VCE physical compaction plan changed; generate and confirm a new preview",
            ));
        }
        let mut tables = plan
            .actions
            .into_iter()
            .map(|action| {
                let table = action.physical_table.ok_or_else(|| {
                    String::from("VCE physical compaction action omitted its table target")
                })?;
                Ok(VceCompactionTableReceipt {
                    action_id: action.action_id,
                    latest_statistics: table.statistics.clone(),
                    plan: table,
                    state: VceCompactionTableState::Pending,
                    round: 0,
                    operation_id: None,
                    operation_expected_version: None,
                    metrics: VceCompactionMetrics::default(),
                    error: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        tables.sort_by_key(|table| table.plan.statistics.role);
        let previous_time = store
            .latest()
            .await?
            .map(|job| job.created_at_unix_ms)
            .unwrap_or(0);
        let timestamp = job_time()?.max(
            previous_time
                .checked_add(1)
                .ok_or_else(|| String::from("VCE job admission timestamp overflow"))?,
        );
        let receipt = VceStoredJob {
            schema_version: vulcan_workspace_store::vce_data_plane::VCE_JOB_RECEIPT_SCHEMA_VERSION,
            project_id: project_id.to_owned(),
            job_id: Uuid::new_v4().to_string(),
            owner_run_id: self.job_owner_run_id.clone(),
            idempotency_key: command.idempotency_key,
            retry_of_job_id: None,
            request_fingerprint,
            contract_fingerprint: command.contract_fingerprint,
            kind: VceJobKind::Compact,
            state: VceJobState::Queued,
            sequence: 1,
            created_at_unix_ms: timestamp,
            updated_at_unix_ms: timestamp,
            admitted_settings_revision: command.expected_settings_revision,
            source_checkpoint_version: command.expected_checkpoint_version,
            replacement: None,
            compaction: Some(VceCompactionJobReceipt { tables }),
            result_checkpoint_version: None,
            progress: None,
            followup_requested: false,
            completed_update: None,
            error: None,
        };
        let receipt = store.register(&receipt).await?;
        state.stop_requested.store(false, Ordering::Release);
        let project_execution = Arc::clone(&state.execution).read_owned().await;
        let job = Arc::new(VceManagedJob {
            job_id: receipt.job_id.clone(),
            replacement_settings: None,
            project_writer: Arc::new(project_writer),
            store: store.clone(),
            event_emitter: self.event_emitter.clone(),
            receipt: Mutex::new(receipt.clone()),
            persistence_gate: Mutex::new(()),
            control: VceRefreshControl::default(),
            cancel_wake: Notify::new(),
            persistence_error: Mutex::new(None),
            pending_terminal_receipt: Mutex::new(None),
            #[cfg(test)]
            compaction_observation_persistence_failures: std::sync::atomic::AtomicUsize::new(0),
        });
        *state.active.lock().await = Some(Arc::clone(&job));
        self.event_emitter
            .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                super::super::services::map_vce_job(receipt.clone()),
            )));
        let manager = self.clone();
        let worker_state = Arc::clone(&state);
        let worker = executor.spawn(async move {
            let _project_execution = project_execution;
            manager
                .run_physical_compaction_job(index, Arc::clone(&job))
                .await;
            drop(job);
            drop(_project_execution);
            worker_state.clear_worker_handle().await;
        });
        state.install_worker_handle(worker).await;
        Ok(receipt)
    }

    /// Admit initialized watcher work through the same durable writer and progress queue.
    /// 通过相同持久写入与进度队列准入已初始化监听工作。
    ///
    /// `project_id` owns the watch and `paths` is a dirty set or explicit full reconciliation.
    /// `executor` retains the task. Returns a durable receipt and never creates an index.
    /// `project_id` 持有监听，`paths` 为脏集合或明确完整核对。
    /// `executor` 保留任务。返回持久回执，绝不创建索引。
    /// `expected_root` freezes source ownership; `observation` covers the delivered batch, not future events.
    /// `expected_root` 冻结源码归属；`observation` 覆盖已投递批次，而非未来事件。
    pub(crate) async fn submit_watcher_job(
        &self,
        project_id: &str,
        expected_root: &Path,
        paths: Option<BTreeSet<String>>,
        observation: Option<super::super::watcher::VceSourceObservationTicket>,
        executor: &tokio::runtime::Runtime,
    ) -> Result<VceStoredJob, String> {
        let runtime = self.require_project(project_id).await?;
        if runtime.source.project_root() != expected_root {
            return Err(String::from(
                "VCE watcher batch belongs to a previous project root",
            ));
        }
        let checkpoint = runtime
            .committed_projection
            .read()
            .await
            .engine
            .checkpoint_version
            .ok_or_else(|| String::from("VCE watcher cannot initialize a missing checkpoint"))?;
        let fingerprint = runtime
            .ai
            .embedding_space()
            .physical_fingerprint
            .clone()
            .ok_or_else(|| String::from("VCE watcher has no admitted embedding fingerprint"))?;
        let command = VceJobAdmission {
            retry_of_job_id: None,
            kind: VceJobKind::Synchronize,
            idempotency_key: Uuid::new_v4().to_string(),
            expected_settings_revision: runtime.config_revision.load(Ordering::Acquire),
            expected_checkpoint_version: Some(checkpoint),
            contract_fingerprint: fingerprint,
        };
        self.submit_index_job(
            project_id,
            command,
            VceJobWork::FileChanges {
                observation,
                project_root: expected_root.to_path_buf(),
                paths,
            },
            executor,
        )
        .await
    }

    /// Read automatic scheduling readiness without scanning or opening missing project storage.
    /// 读取自动调度就绪状态，不扫描或打开缺失项目存储。
    /// `project_id` is a watched owner. Returns remaining delay only for writable initialized projects.
    /// `project_id` 为监听所有者。仅对可写已初始化项目返回剩余延时。
    pub(crate) async fn watcher_admission_delay(
        &self,
        project_id: &str,
    ) -> Result<Option<Duration>, String> {
        let runtime = self.require_project(project_id).await?;
        if runtime.index.is_read_only()
            || runtime
                .committed_projection
                .read()
                .await
                .engine
                .checkpoint_version
                .is_none()
        {
            return Ok(None);
        }
        let schedule = runtime
            .job_schedule
            .lock()
            .map_err(|_| String::from("VCE job scheduling lock is poisoned"))?;
        Ok(schedule.automatic_delay(tokio::time::Instant::now()))
    }

    /// Share durable admission across exact host source triggers without adding model-facing tools.
    /// 在精确宿主源码触发器间共享持久准入，不增加面向模型的工具。
    ///
    /// `command` freezes guards, `work` owns source scope, and `executor` retains job lifetime.
    /// Returns the persisted receipt before scanning or provider execution.
    /// `command` 冻结保护，`work` 持有源码范围，`executor` 保留作业生命周期。
    /// 扫描或供应商执行前返回持久回执。
    async fn submit_index_job(
        &self,
        project_id: &str,
        command: VceJobAdmission,
        work: VceJobWork,
        executor: &tokio::runtime::Runtime,
    ) -> Result<VceStoredJob, String> {
        if !matches!(
            (&work, command.kind),
            (
                VceJobWork::Manual,
                VceJobKind::Initialize | VceJobKind::Synchronize
            ) | (VceJobWork::FileChanges { .. }, VceJobKind::Synchronize)
                | (VceJobWork::Rebuild { .. }, VceJobKind::Rebuild)
        ) {
            return Err(String::from(
                "VCE rebuilding requires its separate confirmed contract",
            ));
        }
        if command.idempotency_key.trim().is_empty() || command.idempotency_key.len() > 512 {
            return Err(String::from(
                "VCE job idempotency key must contain 1..512 bytes",
            ));
        }
        // lifecycle excludes concurrent project removal until both receipt and worker own leases.
        // lifecycle 在回执与工作线程都取得租约之前排斥并发项目移除。
        let _lifecycle = self.lifecycle_gate.read().await;
        let project_gate = self.project_lifecycle_gate(project_id).await;
        let _project_gate = project_gate.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(String::from("VCE job manager is shutting down"));
        }
        let runtime = self.require_project(project_id).await?;
        let _admission = runtime.job_state.admission.lock().await;
        runtime.index.require_write_access()?;
        if let VceJobWork::FileChanges { project_root, .. } = &work {
            if runtime.source.project_root() != project_root {
                return Err(String::from(
                    "VCE watcher root changed before durable admission",
                ));
            }
            let delay = runtime
                .job_schedule
                .lock()
                .map_err(|_| String::from("VCE job scheduling lock is poisoned"))?
                .automatic_delay(tokio::time::Instant::now());
            if delay != Some(Duration::ZERO) {
                return Err(String::from(
                    "VCE automatic updates are disabled or still within the minimum interval",
                ));
            }
        }
        let store = runtime.index.job_store();
        let request_fingerprint = job_request_fingerprint(
            &runtime.project_id,
            runtime.source.project_root(),
            &command,
            &work,
        )?;
        // Replays are resolved before checkpoint checks: a completed request changed that checkpoint.
        // 重放先于检查点校验解析：已完成请求本身已经改变该检查点。
        if let Some(existing) = store
            .find_submission(&command.idempotency_key, &request_fingerprint)
            .await?
        {
            return Ok(existing);
        }
        // Only human clicks can attach work; watcher failures retain their dirty set for later admission.
        // 仅人为点击能够附加工作；监听失败保留脏集合以待稍后准入。
        let current_job = runtime.job_state.active.lock().await.clone();
        if let Some(current_job) = current_job {
            if !matches!(work, VceJobWork::Manual) || command.retry_of_job_id.is_some() {
                return Err(String::from(
                    "VCE project already has an admitted index job",
                ));
            }
            validate_job_admission(
                &command,
                runtime.config_revision.load(Ordering::Acquire),
                runtime
                    .committed_projection
                    .read()
                    .await
                    .engine
                    .checkpoint_version,
                runtime.ai.embedding_space().physical_fingerprint.as_deref(),
            )?;
            // Serialize the replay mapping with progress and terminal receipt transitions.
            // 将重放映射与进度及终态回执转换串行化。
            let (accepted, followup) = {
                let _persistence = current_job.persistence_gate.lock().await;
                let previous = current_job.receipt.lock().await.clone();
                if previous.kind != command.kind
                    || previous.contract_fingerprint
                        != job_index_contract_fingerprint(&runtime.config, &runtime.settings)?
                {
                    return Err(String::from(
                        "VCE active job has a different admitted operation or index contract",
                    ));
                }
                // Only running synchronization needs another scan; a queued scan already includes later clicks.
                // 仅运行中的同步需要另一轮扫描；排队扫描已包含后续点击时的变化。
                let followup = previous.kind == VceJobKind::Synchronize
                    && previous.state == VceJobState::Running;
                // Acknowledge only after both the replay alias and pending state are durable.
                // 仅在重放别名与待处理状态均持久化后确认。
                let accepted = store
                    .coalesce(
                        &previous,
                        &command.idempotency_key,
                        &request_fingerprint,
                        job_time()?,
                        followup,
                    )
                    .await?;
                *current_job.receipt.lock().await = accepted.clone();
                (accepted, followup)
            };
            if followup {
                runtime
                    .job_schedule
                    .lock()
                    .map_err(|_| String::from("VCE job scheduling lock is poisoned"))?
                    .last_admitted = Some(tokio::time::Instant::now());
            }
            self.event_emitter
                .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                    super::super::services::map_vce_job(accepted.clone()),
                )));
            return Ok(accepted);
        }
        // Exact replays and same-owner coalescing already returned; only a new writer needs exclusion.
        // 精确重放和同所有者合并已经返回；只有新写入者需要互斥。
        let project_writer = Arc::new(
            self.data_plane
                .try_acquire_project_writer(project_id)?
                .ok_or_else(|| String::from("VCE project already has an active host writer"))?,
        );
        if let Some(previous) = store.unfinished().await? {
            // The writer lease was acquired only after the in-memory owner slot was found empty;
            // this proves the old cooperating worker no longer owns the project.
            // 只有在内存所有者槽为空后才取得写入租约；这证明旧协作工作线程已不再持有项目。
            let reason = if previous.owner_run_id == self.job_owner_run_id {
                "VCE previous job ended before its terminal receipt was acknowledged"
            } else {
                "VCE previous host run ended before its terminal receipt was acknowledged"
            };
            store.interrupt_unfinished(reason).await?;
        }
        if let Some(origin_id) = &command.retry_of_job_id {
            let origin = store.get(origin_id).await?
                .ok_or_else(|| String::from("VCE retry origin has no durable host receipt; submit an explicit synchronization instead"))?;
            if !matches!(
                origin.state,
                VceJobState::Failed | VceJobState::Cancelled | VceJobState::Interrupted
            ) || !matches!(
                origin.kind,
                VceJobKind::Initialize | VceJobKind::Synchronize
            ) {
                return Err(String::from(
                    "VCE retry origin is not a retryable initialization or synchronization",
                ));
            }
        }
        let checkpoint = runtime
            .committed_projection
            .read()
            .await
            .engine
            .checkpoint_version;
        validate_job_admission(
            &command,
            runtime.config_revision.load(Ordering::Acquire),
            checkpoint,
            runtime.ai.embedding_space().physical_fingerprint.as_deref(),
        )?;
        if runtime.host_status.read().await.lifecycle == VceProjectLifecycle::RebuildRequired {
            return Err(String::from(
                "VCE index requires an explicitly confirmed rebuild",
            ));
        }
        // writer rejects a second command instead of silently creating an unbounded refresh queue.
        // writer 拒绝第二个命令，而不是静默创建无界刷新队列。
        let writer = Arc::clone(&runtime.refresh_gate)
            .try_lock_owned()
            .map_err(|_| String::from("VCE project already has an active index writer"))?;
        // Maintenance confirmation is checked after writer admission, not in a separate pre-lock RPC read.
        // 维护确认在写入准入后检查，而不是在独立的加锁前远程读取中检查。
        if let VceJobWork::Rebuild { repair, action_ids } = &work {
            validate_rebuild_confirmation(&runtime, *repair, action_ids).await?;
            // New work must still be authorized under the current route; exact durable replays already returned above.
            // 新工作必须仍获当前路由授权；精确持久重放已经在上方返回。
            let provenance = runtime.index.embedding_provenance().await?;
            let contract = super::VcePublicationContract::for_runtime(&runtime, provenance)?;
            let _configuration = crate::begin_public_ai_configuration_transaction().await;
            (self.publication_authority)(&contract)?;
        }
        let operation = Arc::clone(&runtime.operation_admission.gate)
            .read_owned()
            .await;
        // project_execution also lets teardown drain a worker when no query runtime is registered.
        // project_execution 还允许拆卸在没有已注册查询运行时时排空工作任务。
        let project_execution = Arc::clone(&runtime.job_state.execution).read_owned().await;
        if !runtime
            .operation_admission
            .accepting
            .load(Ordering::Acquire)
        {
            return Err(String::from("VCE project stopped before job admission"));
        }
        // Project-local admission order remains strict even if the wall clock moves backwards.
        // 即使墙上时钟回拨，项目内准入顺序仍保持严格递增。
        let previous_time = store
            .latest()
            .await?
            .map(|job| job.created_at_unix_ms)
            .unwrap_or(0);
        let timestamp = job_time()?.max(
            previous_time
                .checked_add(1)
                .ok_or_else(|| String::from("VCE job admission timestamp overflow"))?,
        );
        let receipt = VceStoredJob {
            retry_of_job_id: command.retry_of_job_id.clone(),
            schema_version: vulcan_workspace_store::vce_data_plane::VCE_JOB_RECEIPT_SCHEMA_VERSION,
            replacement: None,
            compaction: None,
            project_id: project_id.to_owned(),
            job_id: Uuid::new_v4().to_string(),
            owner_run_id: self.job_owner_run_id.clone(),
            idempotency_key: command.idempotency_key,
            request_fingerprint,
            contract_fingerprint: job_index_contract_fingerprint(
                &runtime.config,
                &runtime.settings,
            )?,
            kind: command.kind,
            state: VceJobState::Queued,
            sequence: 1,
            created_at_unix_ms: timestamp,
            updated_at_unix_ms: timestamp,
            admitted_settings_revision: command.expected_settings_revision,
            source_checkpoint_version: checkpoint,
            result_checkpoint_version: None,
            progress: None,
            followup_requested: false,
            completed_update: None,
            error: None,
        };
        let receipt = store.register(&receipt).await?;
        runtime
            .job_schedule
            .lock()
            .map_err(|_| String::from("VCE job scheduling lock is poisoned"))?
            .last_admitted = Some(tokio::time::Instant::now());
        let job = Arc::new(VceManagedJob {
            job_id: receipt.job_id.clone(),
            replacement_settings: None,
            project_writer,
            store: store.clone(),
            event_emitter: self.event_emitter.clone(),
            receipt: Mutex::new(receipt.clone()),
            persistence_gate: Mutex::new(()),
            control: VceRefreshControl::default(),
            cancel_wake: Notify::new(),
            persistence_error: Mutex::new(None),
            pending_terminal_receipt: Mutex::new(None),
            #[cfg(test)]
            compaction_observation_persistence_failures: std::sync::atomic::AtomicUsize::new(0),
        });
        *runtime.job_state.active.lock().await = Some(Arc::clone(&job));
        self.event_emitter
            .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                super::super::services::map_vce_job(receipt.clone()),
            )));
        let manager = self.clone();
        let worker_runtime = Arc::clone(&runtime);
        let worker_state = Arc::clone(&runtime.job_state);
        let worker = executor.spawn(async move {
            // Leases live until the worker and terminal receipt settle, including queued cancellation.
            // 租约持续到工作线程与终态回执确定，包括排队取消。
            let _operation = operation;
            let _project_execution = project_execution;
            let _writer = writer;
            manager
                .run_managed_job_chain(&worker_runtime, job, &store, work)
                .await;
            // Release every project/storage lease before removing the only shutdown join handle.
            // 在移除关闭流程唯一可回收的任务句柄之前，先释放全部项目和存储租约。
            drop(_writer);
            drop(_project_execution);
            drop(_operation);
            // Release the worker's storage-bearing clones before removing the only shutdown join handle.
            // 在移除关闭流程唯一可回收的任务句柄之前，先释放工作线程持有的存储租约克隆。
            drop(store);
            drop(worker_runtime);
            worker_state.clear_worker_handle().await;
        });
        // The admission fence remains held here, so a worker that finishes immediately cannot
        // clear a handle that has not been installed yet.
        // 此处仍持有准入栅栏，因此立即完成的工作任务不会清除尚未安装的句柄。
        runtime.job_state.install_worker_handle(worker).await;
        Ok(receipt)
    }

    /// Query a stable job receipt without requiring valid model routes or starting source work.
    /// 查询稳定作业回执，不要求模型路由有效，不启动源码处理。
    ///
    /// `project_id` and optional `job_id` select an exact receipt or the latest admitted job.
    /// Returns durable metadata; missing runtime is handled through existing storage only.
    /// `project_id` 与可选 `job_id` 选择精确回执或最近准入作业。
    /// 返回持久元数据；运行时缺失时仅访问已有存储。
    pub(crate) async fn get_job(
        &self,
        project_id: &str,
        job_id: Option<&str>,
    ) -> Result<Option<VceStoredJob>, String> {
        // Historical terminal queries remain read-only, even when a newer cold job exists.
        // 即使存在更新的冷态作业，历史终态查询也保持只读。
        let receipt = self
            .read_job(
                project_id,
                match job_id {
                    Some(id) => VceJobSelection::Exact(id),
                    None => VceJobSelection::Latest,
                },
            )
            .await?;
        let Some(receipt) = receipt else {
            return Ok(None);
        };
        if receipt.state.is_terminal() {
            return Ok(Some(receipt));
        }
        self.reconcile_cold_job(project_id, &receipt).await?;
        self.read_job(
            project_id,
            match job_id {
                Some(id) => VceJobSelection::Exact(id),
                None => VceJobSelection::Latest,
            },
        )
        .await
    }

    /// Resolve an abandoned receipt without adopting a worker or starting a new job.
    /// 收敛遗留回执，不接管工作线程、不启动新作业。
    /// `project_id` and `job_id` select the observed owner; returns after reconciliation or a live-owner no-op.
    /// `project_id` 与 `job_id` 选择已观察所有者；持久收敛完成或确认活动所有者后返回。
    async fn reconcile_cold_job(
        &self,
        project_id: &str,
        observed: &VceStoredJob,
    ) -> Result<(), String> {
        // Polling must not wait on shutdown/removal gates that may themselves be draining this watcher.
        // 轮询不能等待关闭或移除闸门，因为这些闸门可能正在排空当前观察线程。
        let Ok(_lifecycle) = self.lifecycle_gate.try_read() else {
            return Ok(());
        };
        if self.shutting_down.load(Ordering::Acquire) {
            return Ok(());
        }
        let project_gate = self.project_lifecycle_gate(project_id).await;
        let Ok(_project_gate) = project_gate.try_lock() else {
            return Ok(());
        };
        let state = self.project_job_state(project_id).await;
        let Ok(_admission) = state.admission.try_lock() else {
            return Ok(());
        };
        if state.active.lock().await.is_some() {
            return Ok(());
        }
        if observed.kind == VceJobKind::Compact {
            let Some((writer, index, store, receipt)) = self
                .data_plane
                .prepare_cold_compaction_recovery(project_id, &observed.job_id)
                .await?
            else {
                // Another cooperating process still holds the exact project writer lease.
                // 另一个协作进程仍持有精确项目写入租约。
                return Ok(());
            };
            if receipt.state.is_terminal() {
                self.event_emitter
                    .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                        super::super::services::map_vce_job(receipt),
                    )));
                return Ok(());
            }
            self.attach_cold_compaction_job(&state, writer, index, store, receipt, false)
                .await?;
            return Ok(());
        }
        if let Some(receipt) = self
            .data_plane
            .reconcile_ended_project_job(project_id, &observed.job_id)
            .await?
        {
            self.event_emitter
                .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                    super::super::services::map_vce_job(receipt),
                )));
        }
        Ok(())
    }

    /// Attach one cold durable compaction receipt to this host without inventing another operation.
    /// 将一项冷态持久整理回执关联到当前宿主，且不虚构另一项操作。
    ///
    /// `state` is the exact project slot; `writer`, `index`, and `store` are the storage factory's
    /// jointly validated ownership; `receipt` is the sole unfinished owner. `cancel_requested`
    /// restores or initiates cancellation before monitoring. Returns the acknowledged attached state.
    /// `state` 是精确项目槽位；`writer`、`index` 与 `store` 是存储工厂联合校验的所有权；
    /// `receipt` 是唯一未完成所有者。`cancel_requested` 会在观察前恢复或发起取消。
    /// 返回已确认的关联状态。
    async fn attach_cold_compaction_job(
        &self,
        state: &Arc<VceProjectJobState>,
        writer: VceProjectWriterLease,
        index: VceProjectIndex,
        store: VceProjectJobStore,
        receipt: VceStoredJob,
        cancel_requested: bool,
    ) -> Result<VceStoredJob, String> {
        if receipt.kind != VceJobKind::Compact || receipt.state.is_terminal() {
            return Err(String::from(
                "VCE cold compaction attachment requires one unfinished compact receipt",
            ));
        }
        let executor = tokio::runtime::Handle::try_current().map_err(|error| {
            format!("VCE compaction recovery requires the retained service runtime: {error}")
        })?;
        let should_cancel = cancel_requested || receipt.state == VceJobState::Cancelling;
        let job = Arc::new(VceManagedJob {
            job_id: receipt.job_id.clone(),
            replacement_settings: None,
            project_writer: Arc::new(writer),
            store: store.clone(),
            event_emitter: self.event_emitter.clone(),
            receipt: Mutex::new(receipt),
            persistence_gate: Mutex::new(()),
            control: VceRefreshControl::default(),
            cancel_wake: Notify::new(),
            persistence_error: Mutex::new(None),
            pending_terminal_receipt: Mutex::new(None),
            #[cfg(test)]
            compaction_observation_persistence_failures: std::sync::atomic::AtomicUsize::new(0),
        });
        let attached = if should_cancel {
            let _ = job.control.cancel();
            job.persist(&store, |receipt| {
                if !receipt.state.is_terminal() {
                    receipt.state = VceJobState::Cancelling;
                    receipt.followup_requested = false;
                }
            })
            .await?
        } else {
            job.receipt.lock().await.clone()
        };
        state.stop_requested.store(false, Ordering::Release);
        let project_execution = Arc::clone(&state.execution).read_owned().await;
        *state.active.lock().await = Some(Arc::clone(&job));
        self.event_emitter
            .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                super::super::services::map_vce_job(attached.clone()),
            )));
        let manager = self.clone();
        let worker_state = Arc::clone(state);
        let worker = executor.spawn(async move {
            let _project_execution = project_execution;
            manager
                .run_physical_compaction_job(index, Arc::clone(&job))
                .await;
            drop(job);
            drop(_project_execution);
            worker_state.clear_worker_handle().await;
        });
        state.install_worker_handle(worker).await;
        Ok(attached)
    }

    /// Read the last successful content-update time without mistaking a failed attempt for freshness.
    /// 读取最近成功内容更新时间，不把失败尝试误认为新鲜度。
    /// `project_id` selects exact storage. Returns no time until an actual batch has been published.
    /// `project_id` 选择精确存储。在实际批次发布前返回无时间。
    pub(crate) async fn last_successful_update(
        &self,
        project_id: &str,
    ) -> Result<Option<u64>, String> {
        self.read_job(project_id, VceJobSelection::LastSuccessfulUpdate)
            .await?
            .map(|receipt| {
                receipt
                    .completed_update
                    .map(|update| update.at_unix_ms)
                    .ok_or_else(|| {
                        String::from("VCE successful batch query returned no completed update")
                    })
            })
            .transpose()
    }

    /// Read selected job metadata while retaining the exact active-runtime or cold-storage lease.
    /// 保留精确活动运行时或冷态存储租约期间读取选定作业元数据。
    /// `project_id` and `selection` identify the owner and bounded query. Returns no fabricated receipt.
    /// `project_id` 与 `selection` 标识归属及有界查询。不返回虚构回执。
    async fn read_job(
        &self,
        project_id: &str,
        selection: VceJobSelection<'_>,
    ) -> Result<Option<VceStoredJob>, String> {
        // The project slot remains authoritative even when no query runtime can use the current models.
        // 即使没有查询运行时能够使用当前模型，项目槽位仍为权威来源。
        let job_state = self
            .project_job_states
            .lock()
            .await
            .get(project_id)
            .cloned();
        if let Some(job_state) = job_state {
            let active = job_state.active.lock().await.clone();
            if let Some(job) = active {
                if let Some(error) = job.persistence_error.lock().await.as_ref() {
                    // Return the exact active receipt with an explicit projection diagnostic so the UI
                    // can still expose the recovery cancellation action after terminal persistence fails.
                    // 终态持久化失败后仍返回带明确投影诊断的精确活动回执，使界面能够继续显示恢复取消操作。
                    let include_active = match &selection {
                        VceJobSelection::Latest => true,
                        VceJobSelection::Exact(job_id) => job.job_id == *job_id,
                        VceJobSelection::LastSuccessfulUpdate => false,
                    };
                    if include_active {
                        let mut receipt = job.receipt.lock().await.clone();
                        receipt.error = Some(error.clone());
                        return Ok(Some(receipt));
                    }
                }
                // Active progress lives in the managed receipt; SQLite may be busy publishing a checkpoint.
                // 活动进度位于托管回执中；SQLite 此时可能正忙于发布检查点。
                let include_active = match &selection {
                    VceJobSelection::Latest => true,
                    VceJobSelection::Exact(job_id) => job.job_id == *job_id,
                    VceJobSelection::LastSuccessfulUpdate => false,
                };
                if include_active {
                    return Ok(Some(job.receipt.lock().await.clone()));
                }
                return selection.read(&job.store).await;
            }
        }
        let runtime = self.projects.read().await.get(project_id).cloned();
        if let Some(runtime) = runtime {
            let _operation = runtime.begin_operation().await?;
            let store = runtime.index.job_store();
            return selection.read(&store).await;
        }
        let Some(config) = self
            .data_plane
            .load_active_project_config(project_id)
            .await?
        else {
            let Some(recovery) = self
                .data_plane
                .load_uninitialized_recovery_config(project_id)
                .await?
            else {
                return Ok(None);
            };
            let index = self
                .data_plane
                .open_uninitialized_recovery_project_index(&recovery)
                .await?
                .ok_or_else(|| String::from("VCE recovery job storage disappeared during read"))?;
            let store = index.job_store();
            return selection.read(&store).await;
        };
        let index = self
            .data_plane
            .open_existing_project_index(&config)
            .await?
            .ok_or_else(|| String::from("VCE job storage disappeared during read"))?;
        let store = index.job_store();
        selection.read(&store).await
    }

    /// Cancel the exact human job; queued work wakes immediately and commit remains non-cancellable.
    /// 取消精确人为作业；排队工作立即唤醒，提交仍不可取消。
    ///
    /// `project_id` and `job_id` are stable displayed identities. Returns a durable receipt.
    /// `project_id` 与 `job_id` 为稳定展示身份。返回持久回执。
    pub(crate) async fn cancel_job(
        &self,
        project_id: &str,
        job_id: &str,
    ) -> Result<VceStoredJob, String> {
        validate_project_identity(project_id, 1)?;
        if let Some(receipt) = self
            .read_job(project_id, VceJobSelection::Exact(job_id))
            .await?
            && receipt.state.is_terminal()
        {
            return Ok(receipt);
        }
        // Stable project control, not model activation, owns cancellation and exact receipt replay.
        // 稳定项目控制而非模型激活，持有取消及精确回执重放。
        let job_state = self.project_job_state(project_id).await;
        let _admission = job_state.admission.lock().await;
        let active = job_state.active.lock().await.clone();
        // A late cancellation of a terminal receipt is idempotent and must not touch a newer job.
        // 针对终态回执的迟到取消具有幂等性，不能触碰较新作业。
        let job = match active {
            Some(job) if job.job_id == job_id => job,
            _ => {
                let receipt = self
                    .read_job(project_id, VceJobSelection::Exact(job_id))
                    .await?
                    .ok_or_else(|| String::from("VCE cancellation job does not exist"))?;
                // Terminal replay stays read-only, even when a newer writer currently owns the project.
                // 终态回放保持只读，即使项目当前由更新的写入者持有也不能被阻塞。
                if receipt.state.is_terminal() {
                    return Ok(receipt);
                }
                return self.cancel_cold_job(project_id, &receipt).await;
            }
        };
        // A finished worker with an unacknowledged terminal receipt cannot accept another control signal;
        // settle its durable owner explicitly so the project is no longer blocked by stale in-memory state.
        // 工作线程已结束但终态回执未确认时不能再次发送控制信号；显式确定持久所有者，解除过期内存状态对项目的阻塞。
        if job.persistence_error.lock().await.is_some() {
            return self.recover_persistence_failed_job(&job_state, &job).await;
        }
        match job.control.cancel() {
            VceCancelOutcome::Accepted => {
                job.cancel_wake.notify_one();
                job.persist(&job.store, |receipt| {
                    if !receipt.state.is_terminal() {
                        receipt.state = VceJobState::Cancelling;
                        receipt.followup_requested = false;
                    }
                })
                .await
                .map_err(Into::into)
            }
            VceCancelOutcome::Committing => Err(String::from(
                "VCE checkpoint is committing; cancellation cannot undo publication",
            )),
            // The completed batch cannot be undone, but cancellation must still remove its pending successor.
            // 已完成批次不能撤销，但取消仍必须移除其待执行后续批次。
            VceCancelOutcome::Finished => job
                .persist(&job.store, |receipt| receipt.followup_requested = false)
                .await
                .map_err(Into::into),
        }
    }

    /// Settle a durable cold job, cancelling only an unpublished uninitialized binding.
    /// 收敛持久冷态作业，仅对未发布的未初始化绑定执行取消。
    ///
    /// `project_id` and `job_id` select the exact cold receipt. The storage factory holds the
    /// project writer while fencing the binding and while both receipt transitions are persisted.
    /// `project_id` 与 `job_id` 选择精确的冷态回执。存储工厂在封锁绑定及持久化两次回执转换期间
    /// 持有项目写入租约。
    ///
    /// Returns a terminal receipt; published and replacement attempts preserve all stored content.
    /// 返回终态回执；已发布与替换尝试保留全部存储内容。
    async fn cancel_cold_job(
        &self,
        project_id: &str,
        observed: &VceStoredJob,
    ) -> Result<VceStoredJob, String> {
        if observed.kind == VceJobKind::Compact {
            let Some((writer, index, store, receipt)) = self
                .data_plane
                .prepare_cold_compaction_recovery(project_id, &observed.job_id)
                .await?
            else {
                return Err(String::from(
                    "VCE compaction cancellation is still owned by another active host",
                ));
            };
            if receipt.state.is_terminal() {
                return Ok(receipt);
            }
            let state = self.project_job_state(project_id).await;
            if state.active.lock().await.is_some() {
                return Err(String::from(
                    "VCE project acquired another active job during cold cancellation",
                ));
            }
            return self
                .attach_cold_compaction_job(&state, writer, index, store, receipt, true)
                .await;
        }
        let Some((_project_writer, store, receipt)) = self
            .data_plane
            .prepare_cold_job_cancellation(project_id, &observed.job_id)
            .await?
        else {
            return Err(String::from("VCE cancellation job does not exist"));
        };
        if receipt.state.is_terminal() {
            return Ok(receipt);
        }
        if !matches!(receipt.kind, VceJobKind::Initialize | VceJobKind::Rebuild)
            || receipt.source_checkpoint_version.is_some()
            || receipt.replacement.is_some()
        {
            return Err(String::from(
                "VCE cold cancellation requires an uninitialized initialize or rebuild job",
            ));
        }
        let cancelling = if receipt.state == VceJobState::Cancelling {
            receipt
        } else {
            self.persist_cold_cancel_state(&store, &receipt, VceJobState::Cancelling)
                .await?
        };
        self.persist_cold_cancel_state(&store, &cancelling, VceJobState::Cancelled)
            .await
    }

    /// Recover an in-memory owner whose terminal receipt could not be persisted after its worker ended.
    /// 恢复工作线程已经结束但终态回执持久化失败的内存所有者。
    ///
    /// `state` and `job` are the exact project owner and durable receipt selected by cancellation.
    /// `state` 与 `job` 是取消操作选中的精确项目所有者与持久回执。
    /// The method settles only the durable receipt; it never resumes indexing or fabricates success.
    /// 此方法仅确定持久回执，不恢复索引执行，也不虚构成功结果。
    async fn recover_persistence_failed_job(
        &self,
        state: &Arc<VceProjectJobState>,
        job: &Arc<VceManagedJob>,
    ) -> Result<VceStoredJob, String> {
        // The worker calls finish before recording persistence_error, so this gate cannot race live work.
        // 工作线程在记录 persistence_error 前已经调用 finish，因此此闸门不会与活动执行竞争。
        let _persistence = job.persistence_gate.lock().await;
        let current = job
            .store
            .get_after_storage_retry(&job.job_id)
            .await?
            .ok_or_else(|| String::from("VCE recovery job receipt does not exist"))?;
        if current.state.is_terminal() {
            // An ambiguous write may already have committed; adopt that authoritative terminal receipt.
            // 结果不确定的写入可能已经提交；采用该权威终态回执。
            *job.receipt.lock().await = current.clone();
            *job.persistence_error.lock().await = None;
            *job.pending_terminal_receipt.lock().await = None;
            *state.active.lock().await = None;
            self.event_emitter
                .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                    super::super::services::map_vce_job(current.clone()),
                )));
            return Ok(current);
        }

        // Retry the exact successor built from authoritative engine facts before considering
        // any generic settlement. Reconstructing this receipt would lose a completed physical round.
        // 在考虑任何通用收敛前重试由权威引擎事实构建的精确后继。重新构造该回执会丢失已完成物理轮次。
        let pending_terminal_receipt = job.pending_terminal_receipt.lock().await.clone();
        if let Some(candidate) = pending_terminal_receipt {
            let expected_sequence = current
                .sequence
                .checked_add(1)
                .ok_or_else(|| String::from("VCE terminal recovery sequence overflow"))?;
            if candidate.sequence != expected_sequence {
                return Err(String::from(
                    "VCE retained terminal receipt no longer follows the durable receipt",
                ));
            }
            let persisted = job.store.update(&current, &candidate).await?;
            *job.receipt.lock().await = persisted.clone();
            *job.persistence_error.lock().await = None;
            *job.pending_terminal_receipt.lock().await = None;
            *state.active.lock().await = None;
            self.event_emitter
                .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                    super::super::services::map_vce_job(persisted.clone()),
                )));
            return Ok(persisted);
        }

        // A retained successful batch proves that some publication may already exist, so recovery is a
        // failure with an explicit retry path; cancellation must not claim that publication was undone.
        // 保留成功批次说明某次发布可能已经存在，因此恢复结果为明确失败并提供重试路径；取消不能声称已撤销该发布。
        let diagnostic = job
            .persistence_error
            .lock()
            .await
            .clone()
            .unwrap_or_else(|| String::from("VCE terminal receipt requires explicit recovery"));
        let mut next = current.clone();
        next.followup_requested = false;
        next.error = Some(bounded_error(diagnostic));
        next.state = if current.completed_update.is_some() {
            VceJobState::Failed
        } else {
            VceJobState::Cancelled
        };
        if next.state == VceJobState::Cancelled {
            next.error = Some(String::from(VCE_REFRESH_CANCELLED_ERROR));
        }

        // Running -> Cancelled is intentionally two-step because the storage state machine requires
        // cooperative cancellation acknowledgement before the terminal cleanup receipt.
        // Running -> Cancelled 必须分两步，因为存储状态机要求先确认协作取消再写入终态清理回执。
        let persisted =
            if current.state == VceJobState::Running && next.state == VceJobState::Cancelled {
                let mut cancelling = current.clone();
                cancelling.state = VceJobState::Cancelling;
                cancelling.followup_requested = false;
                cancelling.error = None;
                cancelling.sequence = current
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| String::from("VCE job sequence overflow"))?;
                cancelling.updated_at_unix_ms = job_time()?.max(current.updated_at_unix_ms);
                let cancelling = job.store.update(&current, &cancelling).await?;
                next.sequence = cancelling
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| String::from("VCE job sequence overflow"))?;
                next.updated_at_unix_ms = job_time()?.max(cancelling.updated_at_unix_ms);
                job.store.update(&cancelling, &next).await?
            } else {
                next.sequence = current
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| String::from("VCE job sequence overflow"))?;
                next.updated_at_unix_ms = job_time()?.max(current.updated_at_unix_ms);
                job.store.update(&current, &next).await?
            };
        *job.receipt.lock().await = persisted.clone();
        *job.persistence_error.lock().await = None;
        *job.pending_terminal_receipt.lock().await = None;
        *state.active.lock().await = None;
        self.event_emitter
            .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                super::super::services::map_vce_job(persisted.clone()),
            )));
        Ok(persisted)
    }

    /// Persist one cold cancellation state with the same compare-and-swap contract as active jobs.
    /// 使用与活动作业相同的比较交换契约，持久化一次冷取消状态。
    ///
    /// `store` owns the fenced project binding and `previous` is the last exact receipt snapshot.
    /// `store` 持有已封锁的项目绑定，`previous` 是最近一次精确回执快照。
    /// `next_state` must be a permitted cancellation transition; returns the acknowledged receipt.
    /// `next_state` 必须是允许的取消状态转换；返回已确认的持久化回执。
    async fn persist_cold_cancel_state(
        &self,
        store: &VceProjectJobStore,
        previous: &VceStoredJob,
        next_state: VceJobState,
    ) -> Result<VceStoredJob, String> {
        let mut next = previous.clone();
        next.state = next_state;
        next.followup_requested = false;
        if next_state == VceJobState::Cancelled {
            next.error = Some(String::from(VCE_REFRESH_CANCELLED_ERROR));
        }
        next.sequence = previous
            .sequence
            .checked_add(1)
            .ok_or_else(|| String::from("VCE job receipt sequence overflow"))?;
        next.updated_at_unix_ms = job_time()?.max(previous.updated_at_unix_ms);
        let persisted = store.update(previous, &next).await?;
        self.event_emitter
            .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                super::super::services::map_vce_job(persisted.clone()),
            )));
        Ok(persisted)
    }

    /// Run one tracked physical compaction job and settle its durable terminal receipt.
    /// 运行一项受跟踪物理整理作业，并确定其持久终态回执。
    ///
    /// `index` is the exact admitted storage target and `job` owns every lease. A recovered table
    /// with an operation identity is queried only; a pending table has provably never been submitted
    /// because identity persistence precedes engine admission. Returns after terminal persistence
    /// or an explicitly retained persistence error.
    /// `index` 是精确准入存储目标，`job` 持有全部租约。恢复后带操作身份的表仅执行查询；
    /// 待处理表由于身份持久化先于引擎准入，可证明从未提交。终态持久化完成或明确保留持久化错误后返回。
    async fn run_physical_compaction_job(&self, index: VceProjectIndex, job: Arc<VceManagedJob>) {
        let outcome = AssertUnwindSafe(self.execute_physical_compaction_job(&index, &job))
            .catch_unwind()
            .await
            .unwrap_or_else(|_| {
                VceCompactionJobOutcome::Failed(String::from(
                    "VCE physical compaction worker panicked",
                ))
            });
        self.finish_physical_compaction_job(&job, outcome).await;
        let project_id = job.receipt.lock().await.project_id.clone();
        let state = self.project_job_state(&project_id).await;
        if job.persistence_error.lock().await.is_none() {
            let mut active = state.active.lock().await;
            if active
                .as_ref()
                .is_some_and(|active_job| active_job.job_id == job.job_id)
            {
                *active = None;
            }
        }
        super::super::services::emit_vce_runtime_changed(&self.event_emitter, self, &project_id)
            .await;
    }

    /// Execute selected table compactions under the shared global writer budget.
    /// 在共享全局写入预算下执行已选表整理。
    ///
    /// `index` and `job` are immutable admitted owners. Pending tables may submit their first or
    /// next uniquely identified bounded round; persisted operation identities are never submitted
    /// again. Returns a typed host terminal outcome.
    /// `index` 与 `job` 是不可变准入所有者。待处理表可提交其首轮或下一轮具有唯一身份的有界操作；
    /// 已持久化的操作身份绝不再次提交。返回类型化宿主终态结果。
    async fn execute_physical_compaction_job(
        &self,
        index: &VceProjectIndex,
        job: &Arc<VceManagedJob>,
    ) -> VceCompactionJobOutcome {
        // A recovered cancellation must still reach an already-admitted storage operation;
        // a pending host-only job can finish immediately without creating physical work.
        // 恢复后的取消仍必须送达已经准入的存储操作；仅在宿主中待处理的作业可直接结束，
        // 且不会创建物理工作。
        let has_admitted_operation = compaction_has_admitted_operation(job).await;
        if job.control.check().is_err() && !has_admitted_operation {
            return VceCompactionJobOutcome::Cancelled;
        }
        let _budget = match acquire_compaction_writer_budget(
            Arc::clone(&self.job_writer_budget),
            &job.cancel_wake,
            has_admitted_operation,
        )
        .await
        {
            Ok(permit) => permit,
            Err(outcome) => return outcome,
        };
        if job.control.check().is_err() && !has_admitted_operation {
            return VceCompactionJobOutcome::Cancelled;
        }
        if let Err(error) = job
            .persist(&job.store, |receipt| {
                if receipt.state == VceJobState::Queued {
                    receipt.state = VceJobState::Running;
                }
            })
            .await
        {
            return VceCompactionJobOutcome::Failed(format!(
                "Persist VCE compaction start: {error}"
            ));
        }
        let table_count = job
            .receipt
            .lock()
            .await
            .compaction
            .as_ref()
            .map(|compaction| compaction.tables.len())
            .unwrap_or(0);
        if table_count == 0 {
            return VceCompactionJobOutcome::Failed(String::from(
                "VCE physical compaction receipt has no selected tables",
            ));
        }
        for table_index in 0..table_count {
            loop {
                let Some(mut table) = compaction_table_snapshot(job, table_index).await else {
                    return VceCompactionJobOutcome::Failed(String::from(
                        "VCE physical compaction table receipt disappeared",
                    ));
                };
                match table.state {
                    VceCompactionTableState::Succeeded => break,
                    VceCompactionTableState::Failed => {
                        return VceCompactionJobOutcome::Failed(table.error.unwrap_or_else(|| {
                            String::from("VCE physical compaction table failed")
                        }));
                    }
                    VceCompactionTableState::Cancelled => {
                        return VceCompactionJobOutcome::Cancelled;
                    }
                    VceCompactionTableState::StateLost | VceCompactionTableState::Interrupted => {
                        return VceCompactionJobOutcome::Interrupted(table.error.unwrap_or_else(
                            || String::from("VCE physical compaction operation was interrupted"),
                        ));
                    }
                    VceCompactionTableState::Pending
                    | VceCompactionTableState::Queued
                    | VceCompactionTableState::Running => {}
                }
                // Cancellation before engine admission settles every remaining pending table;
                // queued or running operations continue below so cancellation reaches their owner.
                // 引擎准入前的取消会确定全部剩余待处理表；排队或运行中的操作继续向下执行，
                // 使取消请求能够送达其所有者。
                if job.control.check().is_err() && table.state == VceCompactionTableState::Pending {
                    return VceCompactionJobOutcome::Cancelled;
                }
                let submit_current = table.state == VceCompactionTableState::Pending;
                if submit_current {
                    let Some(round) = table.round.checked_add(1) else {
                        let error = String::from("VCE physical compaction round overflow");
                        return VceCompactionJobOutcome::FailedWithTableUpdate {
                            table_update: compaction_table_terminal_update(
                                table_index,
                                &table,
                                VceCompactionTableState::Failed,
                                Some(error.clone()),
                            ),
                            error,
                        };
                    };
                    let role_text = compaction_table_role_text(table.plan.statistics.role);
                    let operation_id = format!("{}:compact:{role_text}:{round}", job.job_id);
                    let expected_version = table.latest_statistics.version;
                    let operation_id_for_receipt = operation_id.clone();
                    if let Err(error) = job
                        .persist(&job.store, |receipt| {
                            receipt.state = VceJobState::Running;
                            receipt.progress = Some(compaction_progress(
                                &operation_id_for_receipt,
                                table_index as u64,
                                table_count as u64,
                                VceRefreshPhase::Writing,
                            ));
                            if let Some(table) = receipt
                                .compaction
                                .as_mut()
                                .and_then(|compaction| compaction.tables.get_mut(table_index))
                            {
                                table.state = VceCompactionTableState::Queued;
                                table.round = round;
                                table.operation_id = Some(operation_id_for_receipt);
                                table.operation_expected_version = Some(expected_version);
                                table.error = None;
                            }
                        })
                        .await
                    {
                        return VceCompactionJobOutcome::Failed(format!(
                            "Persist VCE compaction operation admission: {error}"
                        ));
                    }
                    table = match compaction_table_snapshot(job, table_index).await {
                        Some(table) => table,
                        None => {
                            return VceCompactionJobOutcome::Failed(String::from(
                                "VCE admitted compaction table receipt disappeared",
                            ));
                        }
                    };
                }
                let Some(operation_id) = table.operation_id.clone() else {
                    return VceCompactionJobOutcome::Interrupted(String::from(
                        "VCE physical compaction operation identity is missing",
                    ));
                };
                let Some(expected_version) = table.operation_expected_version else {
                    return VceCompactionJobOutcome::Interrupted(String::from(
                        "VCE physical compaction expected version is missing",
                    ));
                };
                let initial = if submit_current {
                    match index
                        .submit_physical_compaction(
                            table.plan.statistics.role,
                            &operation_id,
                            expected_version,
                            &table.plan.options,
                        )
                        .await
                    {
                        Ok(operation) => operation,
                        Err(submit_error) => match index
                            .physical_compaction_operation(
                                table.plan.statistics.role,
                                &operation_id,
                            )
                            .await
                        {
                            Ok(operation) => operation,
                            Err(query_error) if is_uncertain_compaction_error(&submit_error) => {
                                let error = format!(
                                    "VCE compaction submission is uncertain and its operation cannot be queried: {submit_error}; {query_error}"
                                );
                                return VceCompactionJobOutcome::InterruptedWithTableUpdate {
                                    table_update: compaction_table_terminal_update(
                                        table_index,
                                        &table,
                                        VceCompactionTableState::StateLost,
                                        Some(error.clone()),
                                    ),
                                    error,
                                };
                            }
                            Err(_) => {
                                return VceCompactionJobOutcome::FailedWithTableUpdate {
                                    table_update: compaction_table_terminal_update(
                                        table_index,
                                        &table,
                                        VceCompactionTableState::Failed,
                                        Some(submit_error.clone()),
                                    ),
                                    error: submit_error,
                                };
                            }
                        },
                    }
                } else {
                    match index
                        .physical_compaction_operation(table.plan.statistics.role, &operation_id)
                        .await
                    {
                        Ok(operation) => operation,
                        Err(error) => {
                            let diagnostic = format!(
                                "VCE admitted compaction operation state was lost: {error}"
                            );
                            return VceCompactionJobOutcome::InterruptedWithTableUpdate {
                                table_update: compaction_table_terminal_update(
                                    table_index,
                                    &table,
                                    VceCompactionTableState::StateLost,
                                    Some(diagnostic.clone()),
                                ),
                                error: diagnostic,
                            };
                        }
                    }
                };
                let step = self
                    .wait_physical_compaction_operation(
                        index,
                        job,
                        table_index,
                        table_count,
                        table.plan.statistics.role,
                        initial,
                    )
                    .await;
                match step {
                    VceCompactionStepOutcome::Failed(error) => {
                        return VceCompactionJobOutcome::FailedWithTableUpdate {
                            table_update: compaction_table_terminal_update(
                                table_index,
                                &table,
                                VceCompactionTableState::Failed,
                                Some(error.clone()),
                            ),
                            error,
                        };
                    }
                    VceCompactionStepOutcome::Cancelled => {
                        return VceCompactionJobOutcome::CancelledWithTableUpdate(
                            compaction_table_terminal_update(
                                table_index,
                                &table,
                                VceCompactionTableState::Cancelled,
                                None,
                            ),
                        );
                    }
                    VceCompactionStepOutcome::StateLost(error) => {
                        return VceCompactionJobOutcome::InterruptedWithTableUpdate {
                            table_update: compaction_table_terminal_update(
                                table_index,
                                &table,
                                VceCompactionTableState::StateLost,
                                Some(error.clone()),
                            ),
                            error,
                        };
                    }
                    VceCompactionStepOutcome::Succeeded(operation) => {
                        let before = match operation.before {
                            Some(statistics) => match index.physical_operation_statistics(
                                table.plan.statistics.role,
                                statistics,
                            ) {
                                Ok(statistics) => statistics,
                                Err(error) => {
                                    return VceCompactionJobOutcome::FailedWithTableUpdate {
                                        table_update: compaction_table_terminal_update(
                                            table_index,
                                            &table,
                                            VceCompactionTableState::Failed,
                                            Some(error.clone()),
                                        ),
                                        error,
                                    };
                                }
                            },
                            None => {
                                let error = String::from(
                                    "VCE storage engine omitted pre-compaction statistics",
                                );
                                return VceCompactionJobOutcome::FailedWithTableUpdate {
                                    table_update: compaction_table_terminal_update(
                                        table_index,
                                        &table,
                                        VceCompactionTableState::Failed,
                                        Some(error.clone()),
                                    ),
                                    error,
                                };
                            }
                        };
                        let after = match operation.after {
                            Some(statistics) => match index.physical_operation_statistics(
                                table.plan.statistics.role,
                                statistics,
                            ) {
                                Ok(statistics) => statistics,
                                Err(error) => {
                                    return VceCompactionJobOutcome::FailedWithTableUpdate {
                                        table_update: compaction_table_terminal_update(
                                            table_index,
                                            &table,
                                            VceCompactionTableState::Failed,
                                            Some(error.clone()),
                                        ),
                                        error,
                                    };
                                }
                            },
                            None => {
                                let error = String::from(
                                    "VCE storage engine omitted post-compaction statistics",
                                );
                                return VceCompactionJobOutcome::FailedWithTableUpdate {
                                    table_update: compaction_table_terminal_update(
                                        table_index,
                                        &table,
                                        VceCompactionTableState::Failed,
                                        Some(error.clone()),
                                    ),
                                    error,
                                };
                            }
                        };
                        let Some(metrics) = operation.metrics else {
                            let error =
                                String::from("VCE storage engine omitted compaction metrics");
                            return VceCompactionJobOutcome::FailedWithTableUpdate {
                                table_update: compaction_table_terminal_update(
                                    table_index,
                                    &table,
                                    VceCompactionTableState::Failed,
                                    Some(error.clone()),
                                ),
                                error,
                            };
                        };
                        if let Err(error) = validate_compaction_statistics_transition(
                            &table.latest_statistics,
                            expected_version,
                            &before,
                            &after,
                            metrics.fragments_removed,
                            metrics.fragments_added,
                        ) {
                            return VceCompactionJobOutcome::FailedWithTableUpdate {
                                table_update: compaction_table_terminal_update(
                                    table_index,
                                    &table,
                                    VceCompactionTableState::Failed,
                                    Some(error.clone()),
                                ),
                                error,
                            };
                        }
                        let accumulated = match accumulate_compaction_metrics(
                            &table.metrics,
                            metrics.fragments_removed,
                            metrics.fragments_added,
                            metrics.files_removed,
                            metrics.files_added,
                        ) {
                            Ok(metrics) => metrics,
                            Err(error) => {
                                return VceCompactionJobOutcome::FailedWithTableUpdate {
                                    table_update: compaction_table_terminal_update(
                                        table_index,
                                        &table,
                                        VceCompactionTableState::Failed,
                                        Some(error.clone()),
                                    ),
                                    error,
                                };
                            }
                        };
                        let requires_more = after.requires_compaction();
                        let cancelled = job.control.check().is_err();
                        if requires_more
                            && (metrics.fragments_removed == 0
                                || after.fragment_count >= before.fragment_count)
                        {
                            let error = String::from(
                                "VCE native compaction made no measurable fragment progress",
                            );
                            return VceCompactionJobOutcome::FailedWithTableUpdate {
                                table_update: completed_compaction_table_terminal_update(
                                    table_index,
                                    &table,
                                    after,
                                    accumulated,
                                    VceCompactionTableState::Failed,
                                    Some(error.clone()),
                                ),
                                error,
                            };
                        }
                        if cancelled {
                            let next_state = if requires_more {
                                VceCompactionTableState::Cancelled
                            } else {
                                VceCompactionTableState::Succeeded
                            };
                            let update = completed_compaction_table_terminal_update(
                                table_index,
                                &table,
                                after,
                                accumulated,
                                next_state,
                                None,
                            );
                            if !requires_more && table_index + 1 == table_count {
                                return VceCompactionJobOutcome::SucceededWithTableUpdate(update);
                            }
                            return VceCompactionJobOutcome::CancelledWithTableUpdate(update);
                        }
                        let operation_id_for_progress = operation_id.clone();
                        if let Err(error) = job
                            .persist(&job.store, |receipt| {
                                let completed = table_index as u64 + u64::from(!requires_more);
                                receipt.progress = Some(compaction_progress(
                                    &operation_id_for_progress,
                                    completed,
                                    table_count as u64,
                                    VceRefreshPhase::Writing,
                                ));
                                if let Some(table) = receipt
                                    .compaction
                                    .as_mut()
                                    .and_then(|compaction| compaction.tables.get_mut(table_index))
                                {
                                    table.latest_statistics = after.clone();
                                    table.metrics = accumulated.clone();
                                    table.state = if requires_more {
                                        VceCompactionTableState::Pending
                                    } else {
                                        VceCompactionTableState::Succeeded
                                    };
                                    if requires_more {
                                        table.operation_id = None;
                                        table.operation_expected_version = None;
                                    }
                                    table.error = None;
                                }
                            })
                            .await
                        {
                            let diagnostic =
                                format!("Persist VCE compaction round result: {error}");
                            let table_state = if requires_more {
                                VceCompactionTableState::Failed
                            } else {
                                VceCompactionTableState::Succeeded
                            };
                            let table_error = requires_more.then(|| diagnostic.clone());
                            let update = completed_compaction_table_terminal_update(
                                table_index,
                                &table,
                                after,
                                accumulated,
                                table_state,
                                table_error,
                            );
                            if !requires_more && table_index + 1 == table_count {
                                return VceCompactionJobOutcome::SucceededWithTableUpdate(update);
                            }
                            return VceCompactionJobOutcome::FailedWithTableUpdate {
                                error: diagnostic,
                                table_update: update,
                            };
                        }
                        if !requires_more {
                            break;
                        }
                    }
                }
            }
        }
        match job.control.finish_without_commit() {
            Ok(()) => VceCompactionJobOutcome::Succeeded,
            Err(vulcan_context_engine::VceError::RefreshCancelled) => {
                VceCompactionJobOutcome::Cancelled
            }
            Err(error) => VceCompactionJobOutcome::Failed(error.to_string()),
        }
    }

    /// Poll one storage operation while forwarding cancellation at its real safe boundary.
    /// 轮询一项存储操作，同时在其真实安全边界转发取消请求。
    async fn wait_physical_compaction_operation(
        &self,
        index: &VceProjectIndex,
        job: &Arc<VceManagedJob>,
        table_index: usize,
        table_count: usize,
        role: VcePhysicalTableRole,
        mut operation: VcePhysicalCompactionOperation,
    ) -> VceCompactionStepOutcome {
        let mut cancellation_forwarded = false;
        // Observation persistence is advisory after the operation identity has been durably admitted.
        // 操作身份持久准入后，运行中观察值的持久化仅用于展示，不再决定操作所有权。
        let mut observation_persistence_failure_logged = false;
        loop {
            if job.control.check().is_err() && !cancellation_forwarded {
                match index
                    .cancel_physical_compaction(role, &operation.operation_id)
                    .await
                {
                    Ok(cancelled) => operation = cancelled,
                    Err(cancel_error) => {
                        match index
                            .physical_compaction_operation(role, &operation.operation_id)
                            .await
                        {
                            Ok(current) => operation = current,
                            Err(query_error) => {
                                return VceCompactionStepOutcome::StateLost(format!(
                                    "VCE compaction cancellation result is unknown: {cancel_error}; {query_error}"
                                ));
                            }
                        }
                    }
                }
                // A successful query does not acknowledge a failed cancellation RPC. Keep retrying
                // until the engine reports the durable flag or the operation has already settled.
                // 查询成功并不能确认失败的取消请求已送达。仅当引擎报告持久取消标记，或操作已经
                // 进入终态时，才停止后续取消重试。
                cancellation_forwarded = compaction_cancellation_acknowledged(&operation);
            }
            let table_state = match operation.state {
                VcePhysicalCompactionState::Queued => VceCompactionTableState::Queued,
                VcePhysicalCompactionState::Running => VceCompactionTableState::Running,
                VcePhysicalCompactionState::Succeeded => {
                    return VceCompactionStepOutcome::Succeeded(Box::new(operation));
                }
                VcePhysicalCompactionState::Failed => {
                    return VceCompactionStepOutcome::Failed(
                        operation.error.unwrap_or(operation.message),
                    );
                }
                VcePhysicalCompactionState::Cancelled => {
                    return VceCompactionStepOutcome::Cancelled;
                }
                VcePhysicalCompactionState::Unspecified => {
                    return VceCompactionStepOutcome::StateLost(String::from(
                        "VCE storage engine returned an unspecified compaction state",
                    ));
                }
            };
            let operation_id = operation.operation_id.clone();
            if let Err(error) = job
                .persist(&job.store, |receipt| {
                    receipt.progress = Some(compaction_progress(
                        &operation_id,
                        table_index as u64,
                        table_count as u64,
                        VceRefreshPhase::Writing,
                    ));
                    if let Some(table) = receipt
                        .compaction
                        .as_mut()
                        .and_then(|compaction| compaction.tables.get_mut(table_index))
                    {
                        table.state = table_state;
                    }
                })
                .await
            {
                // The durable receipt already contains the stable storage-operation ID. Abandoning
                // polling here would release host ownership while the engine may still be mutating
                // the table. Continue querying the same operation and let its authoritative terminal
                // result share the final compare-and-swap; a terminal persistence failure keeps the
                // project lease and exact recovery candidate instead of creating an orphan operation.
                // 持久回执已经保存稳定存储操作 ID。此处放弃轮询会在引擎仍可能修改表时释放宿主
                // 所有权。继续查询同一操作，并让权威终态结果进入最终比较交换；若终态持久化失败，
                // 则继续保留项目租约和精确恢复候选，而不是产生孤儿操作。
                if !observation_persistence_failure_logged {
                    #[cfg(test)]
                    job.compaction_observation_persistence_failures
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    tracing::warn!(
                        job_id = %job.job_id,
                        operation_id = %operation.operation_id,
                        error = %error,
                        "VCE compaction observation persistence failed; continuing authoritative operation polling"
                    );
                    observation_persistence_failure_logged = true;
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(VCE_COMPACTION_POLL_INTERVAL) => {}
                _ = job.cancel_wake.notified() => {}
            }
            operation = match index
                .physical_compaction_operation(role, &operation.operation_id)
                .await
            {
                Ok(operation) => operation,
                Err(error) => {
                    return VceCompactionStepOutcome::StateLost(format!(
                        "VCE admitted compaction operation state was lost: {error}"
                    ));
                }
            };
        }
    }

    /// Persist one physical compaction terminal state without changing the logical checkpoint.
    /// 在不改变逻辑检查点的情况下持久化物理整理终态。
    async fn finish_physical_compaction_job(
        &self,
        job: &VceManagedJob,
        outcome: VceCompactionJobOutcome,
    ) {
        // Table facts and the host terminal state share one compare-and-swap. This removes the
        // failure window where a completed native round could be omitted by a later terminal write.
        // 表事实与宿主终态共享同一次比较交换，从而消除已完成原生轮次被后续终态写入遗漏的故障窗口。
        let persisted = job
            .persist_checked(&job.store, |receipt| {
                apply_physical_compaction_terminal_outcome(receipt, outcome)
            })
            .await;
        job.control.finish();
        match persisted {
            Ok(_) => {
                // One acknowledged terminal receipt contains every table fact used by the decision.
                // 一份已确认终态回执包含该决定使用的全部表事实。
                *job.persistence_error.lock().await = None;
            }
            Err(error) => {
                *job.persistence_error.lock().await = Some(format!(
                    "Persist VCE physical compaction terminal receipt failed: {error}"
                ));
            }
        }
    }

    /// Run coalesced batches serially under one durable job, renewing one-shot controls between batches.
    /// 在一个持久作业内串行运行合并批次，在批次之间更新一次性控制信号。
    /// `runtime` owns the writer lease, `job` owns the receipt, and `work` is the initial source scope.
    /// `runtime` 持有写入租约，`job` 持有回执，`work` 为初始源码范围。
    /// Returns after a terminal acknowledgement or an explicitly retained persistence failure.
    /// 在终态确认或明确保留的持久化失败后返回。
    async fn run_managed_job_chain(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        mut job: Arc<VceManagedJob>,
        store: &VceProjectJobStore,
        mut work: VceJobWork,
    ) {
        // Every batch has a distinct core operation, while UI cancellation continues to use the durable job ID.
        // 每批具有不同核心操作，而界面取消继续使用持久作业编号。
        let mut operation_id = job.receipt.lock().await.job_id.clone();
        loop {
            // Catch a batch panic while its precise cancellation and storage owners are still retained.
            // 在精确取消与存储所有者仍被保留期间捕获批次恐慌。
            let attempted = AssertUnwindSafe(self.run_managed_job(
                runtime,
                &job,
                store,
                work,
                operation_id.clone(),
                None,
            ))
            .catch_unwind()
            .await;
            // Keep execution failure distinct from failure to acknowledge an already committed batch.
            // 区分执行失败与已提交批次的确认失败。
            let mut outcome = match attempted {
                Ok(outcome) => outcome,
                Err(_) => {
                    job.control.cancel();
                    Err(self.finish_panicked_refresh(runtime, &operation_id).await)
                }
            };
            // Completion accounting is a durable write, so it must not hold the admission mutex while
            // shutdown is trying to fence the same owner and wait for its execution lease.
            // 完成计账属于持久化写入，因此不能在退出封锁同一所有者并等待执行租约时持有准入互斥锁。
            if let Ok(checkpoint) = outcome {
                match job_time() {
                    Ok(now) => {
                        if let Err(error) = job
                            .persist(store, |receipt| {
                                receipt.completed_update = Some(VceCompletedJobUpdate {
                                    checkpoint_version: checkpoint,
                                    at_unix_ms: now.max(receipt.updated_at_unix_ms),
                                });
                            })
                            .await
                        {
                            outcome = Err(format!(
                                "VCE batch committed but completion accounting failed: {error}"
                            ));
                        }
                    }
                    Err(error) => outcome = Err(error),
                }
            }
            // Admission excludes clicks, cancellation and teardown handoff while choosing the one successor.
            // 决定唯一后续批次时，准入锁排斥点击、取消与拆卸交接。
            {
                let _admission = runtime.job_state.admission.lock().await;
                // Inspect the acknowledged pending slot after excluding competing clicks and cancellation.
                // 排除竞争点击与取消后检查已确认的待处理槽位。
                let receipt = job.receipt.lock().await.clone();
                // No successor survives failure, closure, cancellation or shutdown.
                // 失败、关闭、取消或退出后均不继续后续批次。
                let continue_requested = outcome.is_ok()
                    && receipt.followup_requested
                    && receipt.state == VceJobState::Running
                    && !runtime.job_state.stop_requested.load(Ordering::Acquire)
                    && runtime
                        .operation_admission
                        .accepting
                        .load(Ordering::Acquire)
                    && !self.shutting_down.load(Ordering::Acquire);
                if continue_requested {
                    // Recheck live shared authority before another scan or paid request, not merely at publication.
                    // 在另一次扫描或付费请求前重新校验实时共享权威，而不只在发布时检查。
                    let publication = publication_index::VcePublicationIndex::new(
                        Arc::clone(runtime),
                        Arc::clone(&self.publication_authority),
                        &self.job_owner_run_id,
                    );
                    // Reuse the host authority contract instead of creating a second provider registry.
                    // 复用宿主权威契约，而非创建第二个供应商注册表。
                    let authorization = {
                        // Match the publisher's existing shared transaction contract; release it before the next batch.
                        // 遵守发布器既有共享事务契约；在下一批次之前释放。
                        let _transaction = crate::begin_public_ai_configuration_transaction().await;
                        publication.validate().await
                    };
                    match authorization {
                        Ok(()) => {
                            match job
                                .persist(store, |receipt| {
                                    receipt.followup_requested = false;
                                    receipt.state = VceJobState::Queued;
                                    receipt.progress = None;
                                })
                                .await
                            {
                                Ok(next) => {
                                    operation_id =
                                        format!("{}:batch:{}", next.job_id, Uuid::new_v4());
                                    job = Arc::new(VceManagedJob {
                                        job_id: next.job_id.clone(),
                                        replacement_settings: None,
                                        project_writer: Arc::clone(&job.project_writer),
                                        store: store.clone(),
                                        event_emitter: self.event_emitter.clone(),
                                        receipt: Mutex::new(next),
                                        persistence_gate: Mutex::new(()),
                                        control: VceRefreshControl::default(),
                                        cancel_wake: Notify::new(),
                                        persistence_error: Mutex::new(None),
                                        pending_terminal_receipt: Mutex::new(None),
                                        #[cfg(test)]
                                        compaction_observation_persistence_failures:
                                            std::sync::atomic::AtomicUsize::new(0),
                                    });
                                    *runtime.job_state.active.lock().await = Some(Arc::clone(&job));
                                    work = VceJobWork::Manual;
                                    continue;
                                }
                                Err(error) => {
                                    outcome = Err(format!(
                                        "VCE follow-up admission persistence failed: {error}"
                                    ))
                                }
                            }
                        }
                        Err(error) => outcome = Err(error),
                    }
                }
            }
            // Terminal receipt persistence must not wait behind admission while shutdown is draining.
            // 终态回执持久化不能在退出排空期间继续占用准入锁。
            self.finish_managed_job(Some(runtime), &job, store, outcome)
                .await;
            // Failed persistence keeps ownership visible and blocks new writers until explicit recovery.
            // 持久化失败保留所有权可见，并阻止新写入者直到明确恢复。
            if job.persistence_error.lock().await.is_none() {
                *runtime.job_state.active.lock().await = None;
            }
            return;
        }
    }

    /// Clean up the exact unwound batch and retire its live observation even when cleanup fails.
    /// 清理精确展开退出的批次；即使清理失败，也结束其存活观察。
    /// `runtime` owns the worker; `operation_id` is retained by its panic boundary, never inferred from storage.
    /// `runtime` 持有工作线程；`operation_id` 由其恐慌边界保留，绝不从存储推断。
    /// Returns the retained failure without changing the last published checkpoint or another batch's observation.
    /// 返回保留的失败，不改变最近发布的检查点或其他批次的观察。
    async fn finish_panicked_refresh(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        operation_id: &str,
    ) -> String {
        // Only the awaited worker's exact identity grants cleanup authority after unwinding.
        // 展开退出后，只有已等待工作线程的精确身份授予清理权限。
        let cleanup = runtime.index.abort_refresh_staging(operation_id).await;
        // Release the observation lock before ending it; end_active_refresh checks identity again.
        // 结束观察前释放观察锁；end_active_refresh 会再次校验身份。
        let active = runtime.active_refresh.lock().await.clone();
        if let Some(active) = active.filter(|active| active.request_id == operation_id) {
            self.end_active_refresh(runtime, &active).await;
        }
        // A panic remains a failure, not user cancellation; cleanup failure stays visible in integrity reads.
        // 恐慌仍属于失败而非用户取消；清理失败在完整性读取中保持可见。
        let error = match cleanup {
            Ok(()) => String::from("VCE indexing worker panicked; unpublished staging was cleaned"),
            Err(error) => format!("VCE indexing worker panicked; staging cleanup failed: {error}"),
        };
        self.fail_runtime(runtime, error).await
    }

    /// Run one durable job under the global budget and sample only actual progress observations.
    /// 在全局预算下运行一个持久作业，仅采样实际进度观察。
    ///
    /// `runtime`, `job`, and `store` are exact admitted owners; `work` and `operation_id` identify this batch.
    /// `runtime`、`job` 与 `store` 是精确准入所有者；`work` 与 `operation_id` 标识此批次。
    /// Returns its published checkpoint or execution error after releasing the global batch budget.
    /// 释放全局批次预算后返回已发布检查点或执行错误。
    async fn run_managed_job(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        job: &Arc<VceManagedJob>,
        store: &VceProjectJobStore,
        work: VceJobWork,
        operation_id: String,
        // A replacement acquires this budget before allocating storage or registering models.
        // 替换任务在分配存储或注册模型之前取得此预算。
        admitted_budget: Option<tokio::sync::OwnedSemaphorePermit>,
    ) -> Result<u64, String> {
        async {
            // notified is registered before checking cancellation, closing the queued wake-up race.
            // notified 在检查取消之前注册，消除排队唤醒竞态。
            let notified = job.cancel_wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            job.control.check().map_err(|error| error.to_string())?;
            let _budget = match admitted_budget {
                Some(permit) => permit,
                None => tokio::select! {
                    permit = Arc::clone(&self.job_writer_budget).acquire_owned() => permit.map_err(|_| String::from("VCE writer budget was closed"))?,
                    _ = notified => return Err(String::from(VCE_REFRESH_CANCELLED_ERROR)),
                },
            };
            job.control.check().map_err(|error| error.to_string())?;
            job.persist(store, |receipt| {
                if receipt.state == VceJobState::Queued {
                    receipt.state = VceJobState::Running;
                }
            }).await?;
            job.control.check().map_err(|error| error.to_string())?;
            // Watch retains one latest observation, so huge projects cannot grow a progress queue.
            // 观察通道仅保留最近一份观察，大型项目不会使进度队列增长。
            let (progress_sender, progress_receiver) = watch::channel(None::<VceRefreshProgress>);
            let failure_project_id = runtime.project_id.clone();
            let failure_job_id = job.job_id.clone();
            let failure_observer = Arc::new(move |failure: VceRefreshFailure| {
                // Queue persistence outside the refresh callback so diagnostics cannot stall indexing.
                // 将持久化排到刷新回调之外，避免诊断写入阻塞索引。
                let project_id = failure_project_id.clone();
                let job_id = failure_job_id.clone();
                let trace_project_id = project_id.clone();
                let trace = serde_json::json!({
                    "schema_version": 1,
                    "captured_at_unix_ms": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|duration| duration.as_millis())
                        .unwrap_or_default(),
                    "capture_point": "vce_refresh_failure",
                    "project_id": project_id,
                    "job_id": job_id,
                    "operation_id": failure.operation_id,
                    "request_stage": "refresh",
                    "api_request_reached": failure.api_request_reached,
                    "failure": failure,
                    "api_request": null,
                });
                let data_root = vulcan_workspace_store::data_plane::application_data_root();
                tokio::task::spawn_blocking(move || {
                    let _ = vulcan_workspace_store::workspace_store::persist_vce_diagnostic(
                        &data_root,
                        &trace_project_id,
                        &trace,
                    );
                });
            });
            let progress_observer = Arc::new(move |progress| {
                progress_sender.send_replace(Some(progress));
            });
            let control = job
                .control
                .clone()
                .with_observer(progress_observer)
                .with_failure_observer(failure_observer);
            // The manager and runtime clones give the structured execution task owned lifetimes.
            // 管理器与运行时克隆为结构化执行任务提供独立所有权生命周期。
            let manager = self.clone();
            let refresh_runtime = Arc::clone(runtime);
            let execution = async move {
                manager
                    .execute_job_refresh(&refresh_runtime, operation_id, control, work)
                    .await
            };
            // The callback retains the exact job store for each bounded progress write.
            // 回调为每次有界进度写入保留精确作业存储。
            let progress_job = Arc::clone(job);
            let progress_store = store.clone();
            let persist_progress = move |
                progress: VceRefreshProgress,
            | -> futures_util::future::BoxFuture<'static, Result<(), VceJobStoreError>> {
                let job = Arc::clone(&progress_job);
                let store = progress_store.clone();
                Box::pin(async move {
                    job.persist(&store, |receipt| receipt.progress = Some(progress))
                        .await
                        .map(|_| ())
                })
            };
            Self::supervise_job_execution(
                job.control.clone(),
                progress_receiver,
                execution,
                persist_progress,
            )
            .await
        }.await
    }

    /// Keep execution polling independent from progress persistence while preserving structured cleanup.
    /// 让执行轮询独立于进度持久化，同时保留结构化清理。
    ///
    /// `execution` is the real admitted refresh future; `persist_progress` performs its durable receipt update.
    /// `execution` 是真实准入刷新 future；`persist_progress` 执行其持久回执更新。
    /// `control` owns cancellation; `progress_receiver` contains only the latest observation.
    /// `control` 持有取消权；`progress_receiver` 仅包含最新观察值。
    /// Returns the execution result or an explicit progress-persistence failure.
    /// 返回执行结果或明确的进度持久化失败。
    async fn supervise_job_execution<F, P>(
        control: VceRefreshControl,
        mut progress_receiver: watch::Receiver<Option<VceRefreshProgress>>,
        execution: F,
        persist_progress: P,
    ) -> Result<u64, String>
    where
        F: std::future::Future<Output = Result<u64, String>> + Send + 'static,
        P: Fn(
                VceRefreshProgress,
            )
                -> futures_util::future::BoxFuture<'static, Result<(), VceJobStoreError>>
            + Send
            + Sync
            + 'static,
    {
        // JoinSet owns the execution task and joins it before this supervisor returns.
        // JoinSet 持有执行任务，并在此监管器返回前完成连接。
        let mut execution_tasks = tokio::task::JoinSet::new();
        execution_tasks.spawn(execution);
        let execution = execution_tasks.join_next();
        tokio::pin!(execution);
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut progress_error = None;
        // Phase changes bypass counter throttling; identical-phase counters remain sampled.
        // 阶段变化绕过计数节流；同阶段计数仍采用采样。
        let mut published_phase = None;
        // Retain the last actual observation so polling does not fabricate new progress timestamps.
        // 保留最近实际观察，避免轮询伪造新的进度时间戳。
        let mut published_progress = None;
        // Closed observers must not create a permanently ready selector branch.
        // 已关闭观察器不能产生永久就绪的选择器分支。
        let mut observing = true;
        let result = loop {
            tokio::select! {
                biased;
                // The supervised execution task keeps progressing while persistence awaits storage.
                // 受监管的执行任务在持久化等待存储时仍持续推进。
                joined = &mut execution => break match joined {
                    Some(Ok(result)) => result,
                    Some(Err(error)) if error.is_panic() => {
                        // Re-raise worker panics so the outer boundary performs exact staging cleanup.
                        // 重新抛出工作任务恐慌，使外层边界执行精确暂存清理。
                        std::panic::resume_unwind(error.into_panic())
                    }
                    Some(Err(error)) => Err(format!("VCE indexing worker task was cancelled: {error}")),
                    None => Err(String::from("VCE indexing worker task ended without a result")),
                },
                // A continuously ready observer must not starve the counter sampling deadline.
                // 持续就绪的观察器不能饿死计数采样期限。
                _ = tick.tick(), if progress_error.is_none() => {
                    let progress = progress_receiver.borrow_and_update().clone();
                    if let Some(progress) = progress {
                        if published_progress.as_ref() == Some(&progress) {
                            continue;
                        }
                        let phase = (progress.operation_id.clone(), progress.phase, progress.activity.as_ref().map(|activity| activity.stage));
                        if let Err(error) =
                            persist_progress_with_retry(&persist_progress, progress.clone()).await
                        {
                            if !error.is_busy() {
                                control.cancel();
                            }
                            progress_error = Some(error);
                        } else {
                            published_phase = Some(phase);
                            published_progress = Some(progress);
                        }
                    }
                }
                changed = progress_receiver.changed(), if progress_error.is_none() && observing => {
                    if changed.is_err() {
                        observing = false;
                        continue;
                    }
                    let progress = progress_receiver.borrow_and_update().clone();
                    if let Some(progress) = progress {
                        let phase = (progress.operation_id.clone(), progress.phase, progress.activity.as_ref().map(|activity| activity.stage));
                        if published_phase.as_ref() != Some(&phase) {
                            if let Err(error) =
                                persist_progress_with_retry(&persist_progress, progress.clone()).await
                            {
                                if !error.is_busy() {
                                    control.cancel();
                                }
                                progress_error = Some(error);
                            } else {
                                published_phase = Some(phase);
                                published_progress = Some(progress);
                            }
                        }
                    }
                }
            }
        };
        // Preserve the final actual observation even when execution ends between sampling ticks.
        // 即使执行在采样时钟之间结束，也保留最终实际观察。
        if progress_error.is_none() {
            let final_progress = progress_receiver.borrow().clone();
            if let Some(progress) = final_progress
                && published_progress.as_ref() != Some(&progress)
                && let Err(error) = persist_progress_with_retry(&persist_progress, progress).await
            {
                progress_error = Some(error);
            }
        }
        // Always await execution/cleanup after a persistence failure; never drop an in-flight commit.
        // 持久化失败后始终等待执行与清理；绝不丢弃正在执行的提交。
        match (result, progress_error) {
            (Ok(checkpoint), None) => Ok(checkpoint),
            (Err(error), None) => Err(error),
            (Ok(checkpoint), Some(error)) if error.is_busy() => Ok(checkpoint),
            (Ok(_), Some(error)) => Err(format!(
                "VCE checkpoint completed but progress persistence failed: {error}"
            )),
            (Err(execution), Some(error)) if error.is_busy() => Err(execution),
            (Err(execution), Some(error)) => {
                Err(format!("{execution}; persist VCE job progress: {error}"))
            }
        }
    }

    /// Execute an admitted full synchronization without reacquiring its already-owned writer gate.
    /// 执行已准入完整同步，不重新获取已经持有的写入闸门。
    ///
    /// `runtime` owns source/storage, `job_id` is the exact receipt identity, and `control` observes work.
    /// Returns the published checkpoint only after host state persistence succeeds.
    /// `runtime` 持有源码与存储，`job_id` 为精确回执身份，`control` 观察处理。
    /// 仅在宿主状态持久化成功后返回已发布检查点。
    async fn execute_job_refresh(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        job_id: String,
        control: VceRefreshControl,
        mut work: VceJobWork,
    ) -> Result<u64, String> {
        control.check().map_err(|error| error.to_string())?;
        let active = self
            .begin_observed_refresh(runtime, job_id.clone(), control)
            .await?;
        if matches!(&work, VceJobWork::Rebuild { repair: true, .. }) {
            // The admitted job already owns the writer gate; do not reenter public maintenance locking.
            // 已准入作业已持有写入锁；不能重入公开维护入口的锁。
            let cleanup = async {
                runtime.index.retry_pending_cleanup().await?;
                if runtime.index.has_unresolved_staging().await? {
                    return Err(String::from(
                        "VCE repair requires confirmed ownership of unresolved staging",
                    ));
                }
                Ok::<(), String>(())
            }
            .await;
            if let Err(error) = cleanup {
                return Err(self.fail_active_refresh(runtime, &active, error).await);
            }
        }
        // Sample before scanning; the event batch's older boundary is retained for incremental acknowledgement.
        // 扫描前采样；保留事件批次较早边界，用于增量确认。
        let watcher = self.source_watcher();
        let before_scan = watcher.as_ref().and_then(|watcher| {
            watcher.begin_source_reconciliation(&runtime.project_id, runtime.source.project_root())
        });
        let incremental_observation = if let VceJobWork::FileChanges {
            paths, observation, ..
        } = &mut work
        {
            if !matches!((before_scan.as_ref(), observation.as_ref()), (Some(current), Some(queued)) if !current.reconciliation_required && queued.same_continuity(current))
            {
                // A legal event may reconcile a gap, but a status read or startup may never create this job.
                // 合法事件可以核对缺口，但状态读取或启动绝不能创建本作业。
                *paths = None;
            }
            observation.clone()
        } else {
            None
        };
        let request = match self.scan_job_request(runtime, &active, work).await {
            Ok(request) => request,
            Err(error) => return Err(self.fail_active_refresh(runtime, &active, error).await),
        };
        // Inspect the actual scan result: directory and ignore changes may already have promoted it to Full.
        // 检查实际扫描结果：目录和忽略规则变化可能已经将其提升为 Full。
        let full = request.mode == vulcan_context_engine::RefreshMode::Full;
        self.refresh_runtime_locked(runtime, request, active)
            .await?;
        if let (Some(watcher), Some(ticket)) = (
            watcher,
            if full {
                before_scan
            } else {
                incremental_observation
            },
        ) {
            watcher.finish_source_reconciliation(
                &runtime.project_id,
                runtime.source.project_root(),
                &ticket,
                full,
            );
        }
        runtime
            .committed_projection
            .read()
            .await
            .engine
            .checkpoint_version
            .ok_or_else(|| String::from("VCE refresh completed without an initialized checkpoint"))
    }

    /// Build full or incremental source input with existing ignore/directory semantics.
    /// 使用既有忽略与目录语义构建全量或增量源码输入。
    ///
    /// `runtime` owns source, `active` owns cancellation, and `work` freezes the dirty set.
    /// Returns exact observations, escalating directory and ignore changes to full reconciliation.
    /// `runtime` 持有源码，`active` 持有取消，`work` 冻结脏集合。
    /// 返回精确观察，目录与忽略变化升级为完整核对。
    async fn scan_job_request(
        &self,
        runtime: &Arc<VceProjectRuntime>,
        active: &VceActiveRefresh,
        work: VceJobWork,
    ) -> Result<RefreshRequest, String> {
        // The first human-triggered run is initialization, even though its admission source is manual.
        // 即使准入来源是手动操作，尚无已发布检查点的首次执行仍属于初始化。
        // Preserve this distinction so Batch-capable models use the initialization transport policy.
        // 保留该区分，使支持 Batch 的模型使用初始化传输策略。
        let initializing = runtime
            .committed_projection
            .read()
            .await
            .engine
            .checkpoint_version
            .is_none();
        let reason = match &work {
            VceJobWork::Manual if initializing => RefreshReason::Initial,
            VceJobWork::Manual => RefreshReason::Manual,
            VceJobWork::Rebuild { repair, .. } => {
                if *repair {
                    RefreshReason::Recovery
                } else {
                    RefreshReason::Manual
                }
            }
            VceJobWork::FileChanges { .. } => RefreshReason::FileWatcher,
        };
        // full_rebuild forces unchanged content through indexing rather than ordinary manifest reconciliation.
        // full_rebuild 强制未变内容重新索引，而不是普通清单核对。
        let full_rebuild = matches!(&work, VceJobWork::Rebuild { .. });
        if let VceJobWork::FileChanges {
            paths: Some(paths), ..
        } = work
        {
            let current = runtime.current_engine.read().await.clone();
            let snapshot = current.engine.workspace_snapshot();
            let removed_directory = paths.iter().any(|path| {
                !runtime.source.project_root().join(path).exists()
                    && snapshot
                        .records
                        .iter()
                        .any(|record| record.path.starts_with(&format!("{path}/")))
            });
            let delta = runtime
                .source
                .scan_paths_observed(
                    paths,
                    active.cancellation.clone(),
                    active.request_id.clone(),
                )
                .await?;
            if !delta.requires_full_refresh && !removed_directory {
                return Ok(RefreshRequest::paths(
                    active.request_id.clone(),
                    reason,
                    delta.observations,
                    delta.removed_paths,
                    None,
                ));
            }
        }
        let manifest = runtime
            .source
            .scan_full_observed(active.cancellation.clone(), active.request_id.clone())
            .await?;
        if full_rebuild {
            return Ok(RefreshRequest::full_rebuild(
                active.request_id.clone(),
                reason,
                manifest.observations,
                None,
            ));
        }
        Ok(RefreshRequest::full(
            active.request_id.clone(),
            reason,
            manifest.observations,
            None,
        ))
    }

    /// Record the authoritative outcome without revoking an older readable checkpoint.
    /// 记录权威结果，不撤销较早的可读检查点。
    ///
    /// `outcome` follows completed execution, cleanup, and host persistence. Returns no value.
    /// `outcome` 在执行、清理与宿主持久化完成后产生。无返回值。
    async fn finish_managed_job(
        &self,
        runtime: Option<&Arc<VceProjectRuntime>>,
        job: &VceManagedJob,
        store: &VceProjectJobStore,
        outcome: Result<u64, String>,
    ) {
        let cancelled = outcome
            .as_ref()
            .is_err_and(|error| error == VCE_REFRESH_CANCELLED_ERROR);
        let persisted = async {
            if cancelled {
                job.persist(store, |receipt| {
                    if !receipt.state.is_terminal() {
                        receipt.state = VceJobState::Cancelling;
                    }
                })
                .await?;
            }
            job.persist(store, |receipt| match outcome {
                Ok(checkpoint) => {
                    receipt.followup_requested = false;
                    receipt.state = VceJobState::Succeeded;
                    receipt.result_checkpoint_version = Some(checkpoint);
                    receipt.error = None;
                }
                Err(error) => {
                    receipt.followup_requested = false;
                    receipt.state = if cancelled {
                        VceJobState::Cancelled
                    } else {
                        VceJobState::Failed
                    };
                    receipt.error = Some(bounded_error(error));
                }
            })
            .await
        }
        .await;
        job.control.finish();
        if let Err(error) = persisted {
            let diagnostic = format!("Persist VCE terminal job receipt failed: {error}");
            *job.persistence_error.lock().await = Some(diagnostic.clone());
            if let Some(runtime) = runtime {
                self.fail_runtime(runtime, diagnostic).await;
            }
        }
        // First publication may make a previously configured automatic project eligible to observe.
        // 首次发布可能使此前已配置自动模式的项目取得文件观察资格。
        if let Some(runtime) = runtime {
            self.reconcile_runtime_watcher(runtime).await;
        }
        // project_id remains available even if cancellation happened before candidate allocation.
        // 即使取消发生于候选分配之前，project_id 仍然可用。
        let project_id = job.receipt.lock().await.project_id.clone();
        super::super::services::emit_vce_runtime_changed(&self.event_emitter, self, &project_id)
            .await;
    }
}

/// Validate the confirmed plan against current storage while the caller retains the writer gate.
/// 在调用方保留写入锁期间，以当前存储验证确认计划。
/// `runtime` owns the exact index, `repair` selects the plan, and `action_ids` is user confirmation.
/// `runtime` 持有精确索引，`repair` 选择计划，`action_ids` 为用户确认。
/// Returns success only for a nonempty exact match; never scans sources or mutates storage.
/// 仅非空精确匹配返回成功；绝不扫描源码或修改存储。
async fn validate_rebuild_confirmation(
    runtime: &VceProjectRuntime,
    repair: bool,
    action_ids: &BTreeSet<String>,
) -> Result<(), String> {
    // engine pins the management request to the runtime whose writer gate is held.
    // engine 将管理请求固定到当前持有写入锁的运行时。
    let engine = runtime.current_engine.read().await.clone();
    // response is the core-validated authoritative plan, not a UI-provided description.
    // response 是核心验证的权威计划，而非界面提供的描述。
    let response = engine
        .engine
        .management_query(
            vulcan_context_engine::VceManagementQueryRequest {
                operation_id: format!("vce:{}:confirm:{}", runtime.project_id, Uuid::new_v4()),
                query: vulcan_context_engine::VceManagementQuery::MaintenancePlan {
                    mode: if repair {
                        vulcan_context_engine::VceMaintenanceMode::Repair
                    } else {
                        vulcan_context_engine::VceMaintenanceMode::Rebuild
                    },
                },
            },
            runtime.index.as_ref(),
        )
        .await
        .map_err(|error| error.to_string())?;
    // plan must have the declared result variant; mismatches are explicit contract errors.
    // plan 必须具有声明的结果分支；不匹配为明确契约错误。
    let vulcan_context_engine::VceManagementQueryResult::MaintenancePlan(plan) = response.result
    else {
        return Err(String::from(
            "VCE maintenance confirmation returned an unexpected result",
        ));
    };
    // planned_ids preserves set semantics while binding every selected action to the replay identity.
    // planned_ids 保留集合语义，同时将全部所选动作绑定到重放身份。
    let planned_ids = plan
        .actions
        .into_iter()
        .map(|action| action.action_id)
        .collect::<BTreeSet<_>>();
    if planned_ids.is_empty() || &planned_ids != action_ids {
        return Err(String::from(
            "VCE maintenance actions changed; regenerate and confirm the current plan",
        ));
    }
    Ok(())
}

/// Validate the bounded human command shape used only by physical compaction admission.
/// 校验仅供物理整理准入使用的有界人为命令结构。
///
/// `command` supplies the exact observed project state and `action_ids` supplies the confirmed
/// table actions. Returns an explicit error before any storage lease or engine request.
/// `command` 提供精确观察到的项目状态，`action_ids` 提供已确认表操作。
/// 在取得任何存储租约或发送引擎请求前返回明确错误。
fn validate_compaction_command(
    command: &VceJobAdmission,
    action_ids: &BTreeSet<String>,
) -> Result<(), String> {
    if command.kind != VceJobKind::Compact || command.retry_of_job_id.is_some() {
        return Err(String::from(
            "VCE physical compaction requires a fresh compact command",
        ));
    }
    if command.idempotency_key.trim().is_empty() || command.idempotency_key.len() > 512 {
        return Err(String::from(
            "VCE compaction idempotency key must contain 1..512 bytes",
        ));
    }
    if command.expected_settings_revision == 0
        || command
            .expected_checkpoint_version
            .is_none_or(|version| version == 0)
        || !vulcan_context_engine::is_tagged_sha256_identity(&command.contract_fingerprint)
    {
        return Err(String::from(
            "VCE compaction command has an invalid revision, checkpoint, or contract fingerprint",
        ));
    }
    if action_ids.is_empty()
        || action_ids.len() > 2
        || action_ids
            .iter()
            .any(|action_id| !vulcan_context_engine::is_sha256_identity(action_id))
    {
        return Err(String::from(
            "VCE physical compaction requires one or two exact action identities",
        ));
    }
    Ok(())
}

/// Compare a compaction command with the authoritative immutable physical binding.
/// 将整理命令与权威不可变物理绑定进行比较。
///
/// `command` is caller-observed state, `config` is the active stored binding, and `checkpoint`
/// is the index provenance. The service boundary separately guards the mutable project-policy
/// revision. Returns only when every immutable execution guard matches exactly.
/// `command` 是调用方观察状态，`config` 是活动存储绑定，`checkpoint` 是索引来源信息。
/// 服务边界单独保护可变的项目策略修订。仅在全部不可变执行保护精确匹配时返回成功。
fn validate_compaction_target(
    command: &VceJobAdmission,
    config: &vulcan_workspace_store::vce_data_plane::VceProjectIndexConfig,
    checkpoint: Option<u64>,
) -> Result<(), String> {
    if command.expected_checkpoint_version != checkpoint
        || command.contract_fingerprint != config.physical_fingerprint
    {
        return Err(String::from(
            "VCE compaction checkpoint or physical contract changed; reload before submitting",
        ));
    }
    Ok(())
}

/// Hash one exact compaction submission independently of mutable execution progress.
/// 独立于可变执行进度散列一项精确整理提交。
///
/// `project_id`, `command`, and sorted `action_ids` form the complete replay authority.
/// Returns a SHA-256 identity suitable for durable idempotency matching.
/// `project_id`、`command` 与已排序 `action_ids` 构成完整重放权威。
/// 返回适用于持久幂等匹配的 SHA-256 身份。
fn compaction_request_fingerprint(
    project_id: &str,
    command: &VceJobAdmission,
    action_ids: &BTreeSet<String>,
) -> Result<String, String> {
    let identity = serde_json::to_vec(&(
        project_id,
        command.kind,
        command.expected_settings_revision,
        command.expected_checkpoint_version,
        &command.contract_fingerprint,
        action_ids,
    ))
    .map_err(|error| format!("Encode VCE compaction identity: {error}"))?;
    Ok(hex::encode(Sha256::digest(identity)))
}

/// Determine whether a durable receipt already owns a queryable storage operation.
/// 判断持久回执是否已经持有可查询的存储操作。
///
/// `job` supplies the acknowledged receipt. Returns true only for a queued or running table with
/// both stable operation identity and expected physical version.
/// `job` 提供已确认回执。仅当排队或运行中的表同时具有稳定操作身份与预期物理版本时返回真。
async fn compaction_has_admitted_operation(job: &VceManagedJob) -> bool {
    job.receipt
        .lock()
        .await
        .compaction
        .as_ref()
        .is_some_and(|compaction| {
            compaction.tables.iter().any(|table| {
                matches!(
                    table.state,
                    VceCompactionTableState::Queued | VceCompactionTableState::Running
                ) && table.operation_id.is_some()
                    && table.operation_expected_version.is_some()
            })
        })
}

/// Clone one table receipt without retaining the job receipt lock across engine I/O.
/// 克隆一份表回执，避免在引擎输入输出期间持续持有作业回执锁。
///
/// `job` owns the acknowledged snapshot and `table_index` selects its deterministic table slot.
/// Returns absence only when the durable compaction payload or slot is missing.
/// `job` 持有已确认快照，`table_index` 选择其确定性表槽位。
/// 仅在持久整理载荷或槽位缺失时返回空值。
async fn compaction_table_snapshot(
    job: &VceManagedJob,
    table_index: usize,
) -> Option<VceCompactionTableReceipt> {
    job.receipt
        .lock()
        .await
        .compaction
        .as_ref()
        .and_then(|compaction| compaction.tables.get(table_index))
        .cloned()
}

/// Freeze one table-local terminal condition from the last acknowledged receipt facts.
/// 从最近确认的回执事实冻结一项表级终止条件。
///
/// `table_index` identifies the deterministic slot, `table` supplies immutable operation ownership,
/// and `state` plus `error` describe the terminal condition. Returns an owned update suitable for
/// the same compare-and-swap as the host terminal state.
/// `table_index` 标识确定性槽位，`table` 提供不可变操作归属，`state` 与 `error` 描述终止条件。
/// 返回可与宿主终态使用同一次比较交换的自包含更新。
fn compaction_table_terminal_update(
    table_index: usize,
    table: &VceCompactionTableReceipt,
    state: VceCompactionTableState,
    error: Option<String>,
) -> Box<VceCompactionTableTerminalUpdate> {
    Box::new(VceCompactionTableTerminalUpdate {
        table_index,
        action_id: table.action_id.clone(),
        round: table.round,
        operation_id: table.operation_id.clone(),
        operation_expected_version: table.operation_expected_version,
        latest_statistics: table.latest_statistics.clone(),
        metrics: table.metrics.clone(),
        state,
        error: error.map(bounded_error),
    })
}

/// Freeze one completed storage round together with its authoritative statistics and metrics.
/// 将一轮已完成存储操作与其权威统计及指标一并冻结。
///
/// `table_index` and `table` identify the admitted operation; `latest_statistics` and `metrics`
/// are already validated cumulative facts. `state` and `error` describe the terminal host decision.
/// Returns an update that preserves the completed physical round even if an earlier receipt write failed.
/// `table_index` 与 `table` 标识已准入操作；`latest_statistics` 与 `metrics` 是已校验累计事实。
/// `state` 与 `error` 描述宿主终止决定。即使先前回执写入失败，返回值仍保留已完成物理轮次。
fn completed_compaction_table_terminal_update(
    table_index: usize,
    table: &VceCompactionTableReceipt,
    latest_statistics: VcePhysicalTableStatistics,
    metrics: VceCompactionMetrics,
    state: VceCompactionTableState,
    error: Option<String>,
) -> Box<VceCompactionTableTerminalUpdate> {
    let mut update = compaction_table_terminal_update(table_index, table, state, error);
    update.latest_statistics = latest_statistics;
    update.metrics = metrics;
    update
}

/// Merge one retained table update only into its exact frozen slot and operation identity.
/// 仅将一份保留的表更新合并进其精确冻结槽位及操作身份。
///
/// `receipt` is the proposed durable successor and `update` contains engine-authoritative facts.
/// Returns an explicit error rather than applying a result to a changed plan or operation.
/// `receipt` 是拟议持久后继，`update` 包含引擎权威事实。计划或操作已经变化时返回明确错误，
/// 绝不把结果应用到错误目标。
fn apply_compaction_table_terminal_update(
    receipt: &mut VceStoredJob,
    update: VceCompactionTableTerminalUpdate,
) -> Result<(), String> {
    let table = receipt
        .compaction
        .as_mut()
        .and_then(|compaction| compaction.tables.get_mut(update.table_index))
        .ok_or_else(|| String::from("VCE terminal compaction table slot disappeared"))?;
    if table.action_id != update.action_id
        || table.round != update.round
        || table.operation_id != update.operation_id
        || table.operation_expected_version != update.operation_expected_version
        || !update.state.is_terminal()
    {
        return Err(String::from(
            "VCE terminal compaction table update no longer matches its admitted operation",
        ));
    }
    table.latest_statistics = update.latest_statistics;
    table.metrics = update.metrics;
    table.state = update.state;
    table.error = update.error;
    Ok(())
}

/// Apply a physical compaction outcome as one complete terminal receipt transition.
/// 将物理整理结果作为一次完整终态回执转换应用。
///
/// `receipt` is cloned from the latest acknowledged state and `outcome` may carry one engine
/// result not yet persisted. Returns only after every selected table has a truthful terminal state.
/// `receipt` 从最近确认状态克隆，`outcome` 可携带一份尚未持久化的引擎结果。
/// 仅当每张已选表均具有真实终态后返回。
fn apply_physical_compaction_terminal_outcome(
    receipt: &mut VceStoredJob,
    outcome: VceCompactionJobOutcome,
) -> Result<(), String> {
    match outcome {
        VceCompactionJobOutcome::SucceededWithTableUpdate(update) => {
            apply_compaction_table_terminal_update(receipt, *update)?;
            apply_physical_compaction_terminal_outcome(receipt, VceCompactionJobOutcome::Succeeded)
        }
        VceCompactionJobOutcome::FailedWithTableUpdate {
            error,
            table_update,
        } => {
            apply_compaction_table_terminal_update(receipt, *table_update)?;
            apply_physical_compaction_terminal_outcome(
                receipt,
                VceCompactionJobOutcome::Failed(error),
            )
        }
        VceCompactionJobOutcome::CancelledWithTableUpdate(update) => {
            apply_compaction_table_terminal_update(receipt, *update)?;
            apply_physical_compaction_terminal_outcome(receipt, VceCompactionJobOutcome::Cancelled)
        }
        VceCompactionJobOutcome::InterruptedWithTableUpdate {
            error,
            table_update,
        } => {
            apply_compaction_table_terminal_update(receipt, *table_update)?;
            apply_physical_compaction_terminal_outcome(
                receipt,
                VceCompactionJobOutcome::Interrupted(error),
            )
        }
        VceCompactionJobOutcome::Succeeded => {
            let compaction = receipt.compaction.as_ref().ok_or_else(|| {
                String::from("VCE physical compaction terminal receipt omitted table state")
            })?;
            if compaction
                .tables
                .iter()
                .any(|table| table.state != VceCompactionTableState::Succeeded)
            {
                return Err(String::from(
                    "VCE physical compaction cannot succeed before every table succeeds",
                ));
            }
            let table_count = compaction.tables.len() as u64;
            receipt.state = VceJobState::Succeeded;
            receipt.result_checkpoint_version = receipt.source_checkpoint_version;
            receipt.progress = Some(compaction_progress(
                &receipt.job_id,
                table_count,
                table_count,
                VceRefreshPhase::Complete,
            ));
            receipt.error = None;
            Ok(())
        }
        VceCompactionJobOutcome::Failed(error) => {
            let diagnostic = bounded_error(error);
            let table_diagnostic = bounded_error(format!(
                "VCE compaction table was not completed because the job failed: {diagnostic}"
            ));
            let compaction = receipt.compaction.as_mut().ok_or_else(|| {
                String::from("VCE physical compaction terminal receipt omitted table state")
            })?;
            for table in &mut compaction.tables {
                if !table.state.is_terminal() {
                    table.state = VceCompactionTableState::Interrupted;
                    table.error = Some(table_diagnostic.clone());
                }
            }
            receipt.state = VceJobState::Failed;
            receipt.result_checkpoint_version = None;
            receipt.error = Some(diagnostic);
            Ok(())
        }
        VceCompactionJobOutcome::Cancelled => {
            let compaction = receipt.compaction.as_mut().ok_or_else(|| {
                String::from("VCE physical compaction terminal receipt omitted table state")
            })?;
            for table in &mut compaction.tables {
                if !table.state.is_terminal() {
                    table.state = VceCompactionTableState::Cancelled;
                    table.error = None;
                }
            }
            receipt.state = VceJobState::Cancelled;
            receipt.result_checkpoint_version = None;
            receipt.error = Some(String::from(VCE_REFRESH_CANCELLED_ERROR));
            Ok(())
        }
        VceCompactionJobOutcome::Interrupted(error) => {
            let diagnostic = bounded_error(error);
            let compaction = receipt.compaction.as_mut().ok_or_else(|| {
                String::from("VCE physical compaction terminal receipt omitted table state")
            })?;
            for table in &mut compaction.tables {
                if !table.state.is_terminal() {
                    table.state = VceCompactionTableState::Interrupted;
                    table.error = Some(diagnostic.clone());
                }
            }
            receipt.state = VceJobState::Interrupted;
            receipt.result_checkpoint_version = None;
            receipt.error = Some(diagnostic);
            Ok(())
        }
    }
}

/// Return the stable operation identifier component for one physical table role.
/// 返回一个物理表角色的稳定操作标识组成部分。
///
/// `role` is the typed VCE table owner. Returns storage-safe ASCII text.
/// `role` 是类型化 VCE 表归属。返回存储安全的 ASCII 文本。
fn compaction_table_role_text(role: VcePhysicalTableRole) -> &'static str {
    match role {
        VcePhysicalTableRole::Content => "content",
        VcePhysicalTableRole::Path => "path",
    }
}

/// Build truthful table-level progress for one physical maintenance operation.
/// 为一项物理维护操作构建真实的表级进度。
///
/// `operation_id` identifies the current child operation, while `completed` and `total` count
/// confirmed tables. Returns a progress snapshot without simulated percentages or source paths.
/// `operation_id` 标识当前子操作，`completed` 与 `total` 统计已确认表。
/// 返回不含模拟百分比或源码路径的进度快照。
fn compaction_progress(
    operation_id: &str,
    completed: u64,
    total: u64,
    phase: VceRefreshPhase,
) -> VceRefreshProgress {
    VceRefreshProgress {
        activity: None,
        phase,
        operation_id: operation_id.to_owned(),
        unit: VceRefreshUnit::Tables,
        completed,
        total: Some(total),
        overall_completed: completed,
        overall_total: Some(total),
        provider_batch_state: None,
        current_path: None,
    }
}

/// Identify submit failures whose server-side admission may have succeeded before reply loss.
/// 识别服务端可能已准入、但回复丢失的提交失败。
///
/// `error` is the engine error projected through the data plane. Returns true only for the
/// typed uncertain-admission prefix rather than interpreting domain validation failures.
/// `error` 是经数据面投影的引擎错误。仅对类型化不确定准入前缀返回真，
/// 不会把领域校验失败解释为不确定结果。
fn is_uncertain_compaction_error(error: &str) -> bool {
    error
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("transport error:")
}

/// Report whether the engine has durably acknowledged cancellation or already settled the work.
/// 判断引擎是否已持久确认取消，或相关工作是否已经进入终态。
fn compaction_cancellation_acknowledged(operation: &VcePhysicalCompactionOperation) -> bool {
    operation.cancel_requested || operation.state.is_terminal()
}

/// Add one acknowledged storage round to cumulative metrics with overflow checks.
/// 通过溢出检查将一轮已确认存储结果加入累计指标。
///
/// `current` is the prior durable total and the remaining values are the new round metrics.
/// Returns the exact accumulated metrics or an explicit integer-overflow error.
/// `current` 是先前持久累计值，其余值为新轮次指标。
/// 返回精确累计指标或明确整数溢出错误。
fn accumulate_compaction_metrics(
    current: &VceCompactionMetrics,
    fragments_removed: u64,
    fragments_added: u64,
    files_removed: u64,
    files_added: u64,
) -> Result<VceCompactionMetrics, String> {
    Ok(VceCompactionMetrics {
        fragments_removed: current
            .fragments_removed
            .checked_add(fragments_removed)
            .ok_or_else(|| String::from("VCE removed-fragment metric overflow"))?,
        fragments_added: current
            .fragments_added
            .checked_add(fragments_added)
            .ok_or_else(|| String::from("VCE added-fragment metric overflow"))?,
        files_removed: current
            .files_removed
            .checked_add(files_removed)
            .ok_or_else(|| String::from("VCE removed-file metric overflow"))?,
        files_added: current
            .files_added
            .checked_add(files_added)
            .ok_or_else(|| String::from("VCE added-file metric overflow"))?,
    })
}

/// Validate that one native compaction round preserves logical table contents and exact admission facts.
/// 校验一轮原生整理会保留逻辑表内容及精确准入事实。
///
/// `admitted` is the latest durable snapshot, `expected_version` is the frozen engine guard,
/// `before` and `after` are untrusted operation boundaries, and fragment metrics describe the
/// physical rewrite. Returns success only for one internally consistent, data-preserving transition.
/// `admitted` 是最近的持久快照，`expected_version` 是冻结的引擎保护值，`before` 与 `after`
/// 是不受信任的操作边界，片段指标描述物理改写。仅对内部一致且保留数据的转换返回成功。
fn validate_compaction_statistics_transition(
    admitted: &VcePhysicalTableStatistics,
    expected_version: u64,
    before: &VcePhysicalTableStatistics,
    after: &VcePhysicalTableStatistics,
    fragments_removed: u64,
    fragments_added: u64,
) -> Result<(), String> {
    if before != admitted || before.version != expected_version {
        return Err(String::from(
            "VCE compaction result does not match its admitted table statistics",
        ));
    }
    if after.role != before.role
        || after.table_name != before.table_name
        || after.version < before.version
    {
        return Err(String::from(
            "VCE compaction result changed its table identity or regressed its version",
        ));
    }
    if after.row_count != before.row_count || after.index_count != before.index_count {
        return Err(String::from(
            "VCE compaction changed logical rows or secondary indexes",
        ));
    }
    let expected_fragments = before
        .fragment_count
        .checked_sub(fragments_removed)
        .and_then(|count| count.checked_add(fragments_added))
        .ok_or_else(|| String::from("VCE compaction fragment metrics are inconsistent"))?;
    if after.fragment_count != expected_fragments {
        return Err(String::from(
            "VCE compaction fragment metrics do not match its table statistics",
        ));
    }
    Ok(())
}

/// Freeze command identity from exact host facts; display names and current job state are excluded.
/// 根据精确宿主事实冻结命令身份；不包含展示名与当前作业状态。
///
/// Project identity and root supply host scope; `command` and `work` supply human intent. Returns SHA-256.
/// 项目身份与根目录提供宿主作用域；`command` 与 `work` 提供人为意图。返回摘要。
fn job_request_fingerprint(
    project_id: &str,
    project_root: &Path,
    command: &VceJobAdmission,
    work: &VceJobWork,
) -> Result<String, String> {
    let identity = serde_json::to_vec(&(
        project_id,
        crate::normalized_workspace_path_text_from_path(project_root),
        command.kind,
        command.expected_settings_revision,
        command.expected_checkpoint_version,
        &command.contract_fingerprint,
        &command.retry_of_job_id,
        work,
    ))
    .map_err(|error| format!("Encode VCE job identity: {error}"))?;
    Ok(hex::encode(Sha256::digest(identity)))
}

/// Hash the actual indexing contract separately from the tagged embedding route fingerprint.
/// 将实际索引契约与带类型前缀的向量路由指纹分别散列。
///
/// `config` and `settings` supply exact admitted source filters and physical/preprocessing/chunking identities.
/// Returns the raw SHA-256 receipt identity; project policy and query budgets are deliberately excluded.
/// `config` 与 `settings` 提供精确准入源码过滤及物理、预处理、切块身份。
/// 返回回执的原始摘要身份；明确排除项目调度策略与查询预算。
fn job_index_contract_fingerprint(
    config: &vulcan_context_engine::VceConfig,
    settings: &ProjectVceSettings,
) -> Result<String, String> {
    let encoded = serde_json::to_vec(&(
        &config.index_namespace_id,
        config.index_schema_version,
        &config.checkpoint_chain_id,
        &config.embedding_space,
        settings.max_file_bytes,
        // Built-in source exclusions are part of the durable job's indexing contract.
        // 内置源码排除规则属于持久作业的索引契约。
        crate::vce::source_port::default_ignore_patterns(),
        &settings.extra_ignore_patterns,
        (
            config.indexing.chunker.max_chars,
            config.indexing.chunker.min_chars,
            config.indexing.embedding_max_inputs,
            config.indexing.embedding_max_chars,
            config.indexing.embedding_max_segment_chars,
            config.indexing.embedding_overlap_chars,
        ),
    ))
    .map_err(|error| format!("Encode VCE job indexing contract: {error}"))?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

/// Validate the exact checkpoint and physical contract before spending indexing resources.
/// 消耗索引资源前校验精确检查点与物理契约。
///
/// `command` is caller intent; remaining inputs are authoritative current runtime facts.
/// Returns an explicit mismatch or unsupported-initialization error.
/// `command` 是调用方意图；其余输入为权威当前运行时事实。返回明确不匹配或不支持初始化错误。
fn validate_job_admission(
    command: &VceJobAdmission,
    revision: u64,
    checkpoint: Option<u64>,
    fingerprint: Option<&str>,
) -> Result<(), String> {
    if command.expected_settings_revision != revision
        || command.expected_checkpoint_version != checkpoint
        || fingerprint != Some(command.contract_fingerprint.as_str())
    {
        return Err(String::from(
            "VCE job settings, checkpoint, or embedding contract changed; reload before submitting",
        ));
    }
    match (command.kind, checkpoint) {
        (VceJobKind::Initialize, None)
        | (VceJobKind::Synchronize | VceJobKind::Rebuild, Some(_)) => Ok(()),
        (VceJobKind::Initialize, Some(_)) => Err(String::from(
            "VCE index is already initialized; synchronize it instead",
        )),
        (VceJobKind::Synchronize, None) => Err(String::from("VCE index has not been initialized")),
        _ => Err(String::from(
            "VCE rebuilding requires an initialized checkpoint",
        )),
    }
}

/// Read a bounded Unix timestamp for durable receipt ordering, never for simulated progress.
/// 读取有界时间戳用于持久回执排序，绝不用于模拟进度。
///
/// Takes no parameters. Returns positive milliseconds or an explicit clock-range error.
/// 无参数。返回正毫秒数或明确时钟范围错误。
pub(super) fn job_time() -> Result<u64, String> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("VCE job clock precedes Unix epoch: {error}"))?
        .as_millis();
    if value == 0 || value > i64::MAX as u128 {
        return Err(String::from(
            "VCE job timestamp is outside the database range",
        ));
    }
    Ok(value as u64)
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
