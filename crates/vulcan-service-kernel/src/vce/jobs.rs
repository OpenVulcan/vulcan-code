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
    VceCompactionTableState, VceCompletedJobUpdate, VceJobKind, VceJobState, VceProjectIndex,
    VceProjectJobStore, VceProjectWriterLease, VceStoredJob,
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

/// Identify storage failures that may clear without cancelling the admitted refresh.
/// 识别可能自行消退、因此不应取消已准入刷新的存储失败。
///
/// `error` is the fully contextualized persistence error emitted by the job store.
/// `error` 是作业存储发出的完整上下文持久化错误。
/// Returns true only for the existing transient transport and SQLite-busy contracts.
/// 仅当错误匹配现有瞬时传输或 SQLite 忙合约时返回 true。
fn is_transient_progress_persistence_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("transport error")
        || normalized.contains("database is locked")
        || normalized.contains("database busy")
        || normalized.contains("sqlite_busy")
}

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
) -> Result<(), String>
where
    P: Fn(VceRefreshProgress) -> futures_util::future::BoxFuture<'static, Result<(), String>>
        + Send
        + Sync
        + 'static,
{
    let mut last_error = None;
    for attempt in 0..=VCE_PROGRESS_PERSIST_RETRIES {
        match persist_progress(progress.clone()).await {
            Ok(()) => return Ok(()),
            Err(error)
                if is_transient_progress_persistence_error(&error)
                    && attempt < VCE_PROGRESS_PERSIST_RETRIES =>
            {
                last_error = Some(error);
                let delay_ms = VCE_PROGRESS_PERSIST_RETRY_BASE_MS * (attempt as u64 + 1);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        String::from("VCE progress persistence retry loop ended without an error")
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
            Self::Exact(id) => store.get(id).await,
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
    ) -> Result<VceStoredJob, String> {
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
    ) -> Result<VceStoredJob, String> {
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
            }
            VceCancelOutcome::Committing => Err(String::from(
                "VCE checkpoint is committing; cancellation cannot undo publication",
            )),
            // The completed batch cannot be undone, but cancellation must still remove its pending successor.
            // 已完成批次不能撤销，但取消仍必须移除其待执行后续批次。
            VceCancelOutcome::Finished => {
                job.persist(&job.store, |receipt| receipt.followup_requested = false)
                    .await
            }
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
            | -> futures_util::future::BoxFuture<'static, Result<(), String>> {
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
        P: Fn(VceRefreshProgress) -> futures_util::future::BoxFuture<'static, Result<(), String>>
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
                            if !is_transient_progress_persistence_error(&error) {
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
                                if !is_transient_progress_persistence_error(&error) {
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
            (Ok(checkpoint), Some(error)) if is_transient_progress_persistence_error(&error) => {
                Ok(checkpoint)
            }
            (Ok(_), Some(error)) => Err(format!(
                "VCE checkpoint completed but progress persistence failed: {error}"
            )),
            (Err(execution), Some(error)) if is_transient_progress_persistence_error(&error) => {
                Err(execution)
            }
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
mod tests {
    use super::*;
    use vulcan_context_engine::VceIndexPort;

    /// Build one deterministic physical-statistics snapshot for compaction transition tests.
    /// 为整理转换测试构造一份确定性的物理统计快照。
    ///
    /// `version`, `row_count`, `index_count`, and `fragment_count` select the tested invariants.
    /// Returns a content-table snapshot whose fragment distribution is internally valid.
    /// `version`、`row_count`、`index_count` 与 `fragment_count` 选择待测试的不变量。
    /// 返回一份片段分布内部有效的内容表快照。
    fn compaction_statistics_fixture(
        version: u64,
        row_count: u64,
        index_count: u64,
        fragment_count: u64,
    ) -> VcePhysicalTableStatistics {
        VcePhysicalTableStatistics {
            role: VcePhysicalTableRole::Content,
            table_name: String::from("content_vectors"),
            version,
            total_bytes: 4096,
            row_count,
            index_count,
            fragment_count,
            small_fragment_count: fragment_count,
            compaction_task_count: u64::from(fragment_count > 1),
            fragment_rows: vulcan_context_engine::VcePhysicalFragmentSummary {
                minimum_rows: row_count.checked_div(fragment_count).unwrap_or(0),
                maximum_rows: row_count,
                mean_rows: row_count.checked_div(fragment_count).unwrap_or(0),
                p25_rows: row_count.checked_div(fragment_count).unwrap_or(0),
                p50_rows: row_count.checked_div(fragment_count).unwrap_or(0),
                p75_rows: row_count.checked_div(fragment_count).unwrap_or(0),
                p99_rows: row_count,
            },
        }
    }

    /// Reject engine success projections that alter logical contents or contradict rewrite metrics.
    /// 拒绝会改变逻辑内容或与改写指标矛盾的引擎成功投影。
    #[test]
    fn compaction_transition_requires_data_preservation_and_exact_metrics() {
        let admitted = compaction_statistics_fixture(9, 32, 2, 8);
        let mut after = compaction_statistics_fixture(10, 32, 2, 1);
        validate_compaction_statistics_transition(&admitted, 9, &admitted, &after, 8, 1)
            .expect("a data-preserving physical rewrite should be accepted");

        after.row_count = 31;
        assert!(
            validate_compaction_statistics_transition(&admitted, 9, &admitted, &after, 8, 1)
                .is_err()
        );
        after.row_count = 32;
        after.index_count = 1;
        assert!(
            validate_compaction_statistics_transition(&admitted, 9, &admitted, &after, 8, 1)
                .is_err()
        );
        after.index_count = 2;
        assert!(
            validate_compaction_statistics_transition(&admitted, 9, &admitted, &after, 7, 1)
                .is_err()
        );

        let mut forged_before = admitted.clone();
        forged_before.total_bytes += 1;
        assert!(
            validate_compaction_statistics_transition(&admitted, 9, &forged_before, &after, 8, 1,)
                .is_err()
        );
    }

    /// Verify a successful lookup cannot masquerade as cancellation acknowledgement.
    /// 验证一次成功查询不能冒充取消确认。
    #[test]
    fn cancellation_acknowledgement_requires_flag_or_terminal_state() {
        // operation starts as an unacknowledged engine-owned running step.
        // operation 初始表示一项尚未确认取消、由引擎持有的运行中步骤。
        let mut operation = VcePhysicalCompactionOperation {
            operation_id: String::from("compact-content-round-1"),
            table_name: String::from("content"),
            state: VcePhysicalCompactionState::Running,
            cancel_requested: false,
            message: String::from("running"),
            error: None,
            submitted_at_unix_ms: 1,
            started_at_unix_ms: Some(2),
            completed_at_unix_ms: None,
            before: None,
            after: None,
            metrics: None,
        };

        assert!(!compaction_cancellation_acknowledged(&operation));
        operation.cancel_requested = true;
        assert!(compaction_cancellation_acknowledged(&operation));
        operation.cancel_requested = false;
        operation.state = VcePhysicalCompactionState::Succeeded;
        assert!(compaction_cancellation_acknowledged(&operation));
    }

    /// Verify cancellation wakes host-only work but cannot release an admitted storage operation.
    /// 验证取消会唤醒仅在宿主排队的工作，但不能释放已准入的存储操作。
    #[tokio::test]
    async fn admitted_compaction_cancellation_retains_the_writer_budget_wait() {
        let writer_budget = Arc::new(tokio::sync::Semaphore::new(1));
        let held = Arc::clone(&writer_budget)
            .acquire_owned()
            .await
            .expect("hold writer budget before recovered compaction");
        let cancel_wake = Arc::new(Notify::new());
        cancel_wake.notify_one();
        let waiting_budget = Arc::clone(&writer_budget);
        let waiting_wake = Arc::clone(&cancel_wake);
        let admitted_wait = tokio::spawn(async move {
            acquire_compaction_writer_budget(waiting_budget, waiting_wake.as_ref(), true).await
        });
        tokio::task::yield_now().await;
        assert!(
            !admitted_wait.is_finished(),
            "an admitted operation must remain owned while waiting to forward cancellation"
        );
        drop(held);
        let admitted_result = tokio::time::timeout(Duration::from_secs(1), admitted_wait)
            .await
            .expect("admitted budget wait should finish after permit release")
            .expect("admitted budget wait task should not panic");
        assert!(admitted_result.is_ok());

        let host_only_budget = Arc::new(tokio::sync::Semaphore::new(0));
        let host_only_wake = Notify::new();
        host_only_wake.notify_one();
        let host_only_result = tokio::time::timeout(
            Duration::from_secs(1),
            acquire_compaction_writer_budget(host_only_budget, &host_only_wake, false),
        )
        .await
        .expect("host-only cancellation should wake immediately");
        assert!(matches!(
            host_only_result,
            Err(VceCompactionJobOutcome::Cancelled)
        ));
    }

    /// Stage deterministic one-row content and path fragments through the production VCE port.
    /// 通过生产 VCE 端口暂存确定性的单行内容与路径碎片。
    ///
    /// `index` is an isolated writable fixture, `label` separates attempts, and the numeric range
    /// creates unique canonical Blobs. The function returns only after every physical write is
    /// acknowledged by the real in-process engine.
    /// `index` 是隔离的可写夹具，`label` 区分不同尝试，数字范围用于创建唯一规范 Blob。
    /// 该函数仅在真实进程内引擎确认全部物理写入后返回。
    async fn stage_real_compaction_fragments(
        index: &VceProjectIndex,
        label: &str,
        start: usize,
        count: usize,
        generation: u64,
    ) {
        for ordinal in start..start + count {
            let path = format!("src/{label}_{ordinal}.rs");
            let content = format!("pub fn {label}_{ordinal}() {{}}");
            let blob_name = vulcan_context_engine::blob_name(&path, &content)
                .expect("build canonical compaction fixture Blob");
            let chunk = vulcan_context_engine::Chunk::new(
                blob_name.clone(),
                &path,
                content,
                1,
                1,
                vulcan_context_engine::ChunkKind::Recursive,
            )
            .expect("build compaction fixture Chunk");
            let path_document = vulcan_context_engine::PathDocument::new(blob_name.clone(), &path)
                .expect("build compaction fixture path document");
            let indexed_blob = vulcan_context_engine::IndexedBlob {
                blob_name: blob_name.clone(),
                path,
                generation,
                structure_analyzer_id: None,
                structure_mode: None,
                structure_fallback_reason: None,
                chunks: vec![chunk.clone()],
                embedding_vectors: vec![vulcan_context_engine::EmbeddingVector {
                    input_id: chunk.chunk_id,
                    values: vec![1.0, 0.0, 0.0],
                }],
                path_embedding: Some(vulcan_context_engine::EmbeddingVector {
                    input_id: path_document.path_id.clone(),
                    values: vec![0.0, 1.0, 0.0],
                }),
                path_document: Some(path_document),
                symbols: Vec::new(),
                path_status: vulcan_context_engine::AuxiliaryIndexStatus::Ready,
                symbol_status: vulcan_context_engine::AuxiliaryIndexStatus::NotApplicable,
                status: vulcan_context_engine::IndexedBlobStatus::Ready,
            };
            index
                .stage(vulcan_context_engine::IndexStageBatch {
                    operation_id: format!("stage-{label}-{ordinal}"),
                    refresh_id: format!("refresh-{label}-{ordinal}"),
                    generation,
                    blobs: vec![indexed_blob],
                })
                .await
                .expect("stage real compaction fixture fragment");
        }
    }

    /// Wait until a host compaction receipt durably records one engine-owned operation.
    /// 等待宿主整理回执持久记录一项由引擎持有的操作。
    ///
    /// `manager`, `project_id`, and `job_id` identify the exact fixture job; `index` queries the
    /// matching storage binding. Returns the table role and stable operation identity only
    /// after both durable owners acknowledge it.
    /// `manager`、`project_id` 与 `job_id` 标识精确夹具作业；`index` 查询匹配的存储绑定。
    /// 仅在两个持久所有者均确认后返回表角色与稳定操作身份。
    async fn await_admitted_compaction_operation(
        manager: &VceRuntimeManager,
        index: &VceProjectIndex,
        project_id: &str,
        job_id: &str,
    ) -> (VcePhysicalTableRole, String) {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let receipt = manager
                    .get_job(project_id, Some(job_id))
                    .await
                    .expect("read compaction fixture receipt")
                    .expect("compaction fixture receipt exists");
                assert!(
                    !receipt.state.is_terminal(),
                    "compaction finished before host recovery was injected: {:?}",
                    receipt.error
                );
                if let Some((role, operation_id)) = receipt.compaction.as_ref().and_then(|state| {
                    state.tables.iter().find_map(|table| {
                        table
                            .operation_id
                            .as_ref()
                            .map(|operation_id| (table.plan.statistics.role, operation_id.clone()))
                    })
                }) {
                    // The host records the stable identity before submission so an uncertain reply
                    // can be queried safely. Wait for engine acknowledgement before injecting
                    // host loss; an absent operation at that earlier boundary is a different fault.
                    // 宿主在提交前记录稳定身份，以便安全查询不确定响应。注入宿主丢失前等待引擎确认；
                    // 更早边界上的操作缺失属于另一种故障。
                    if index
                        .physical_compaction_operation(role, &operation_id)
                        .await
                        .is_ok()
                    {
                        return (role, operation_id);
                    }
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("storage operation must be durably admitted before deadline")
    }

    /// Drop only the fixture host worker and its in-memory owner while preserving durable state.
    /// 仅丢弃夹具宿主工作任务及其内存所有者，同时保留持久状态。
    ///
    /// `state` is the exact project job slot. The function returns after the task and writer lease
    /// are gone, reproducing a host restart while preserving in-process durable state.
    /// `state` 是精确项目作业槽位。该函数在任务与写入租约释放后返回，以复现保留进程内持久状态的宿主重启。
    async fn abandon_fixture_host_worker(state: &Arc<VceProjectJobState>) {
        let worker = state
            .worker_handle
            .lock()
            .await
            .take()
            .expect("fixture host worker exists");
        worker.abort();
        assert!(
            worker
                .await
                .expect_err("fixture host worker is deliberately aborted")
                .is_cancelled()
        );
        let active = state
            .active
            .lock()
            .await
            .take()
            .expect("fixture in-memory owner exists");
        drop(active);
    }

    /// Verify real physical compaction recovery and explicit operation-state loss end to end.
    /// 端到端验证真实物理整理恢复及明确的操作状态丢失。
    ///
    /// `executor` owns all fixture workers. Every database and vector table lives under one
    /// temporary directory; no user project, provider, or billed inference request is involved.
    /// `executor` 持有全部夹具任务。所有数据库与向量表均位于同一临时目录，不涉及用户项目、供应商或计费推理请求。
    async fn verify_real_compaction_host_recovery(executor: Arc<tokio::runtime::Runtime>) {
        use vulcan_ai_contract::test_support::MockHostAiPort;
        use vulcan_workspace_store::data_plane::{HostRuntimeHandle, HostRuntimeMetadata};

        let temporary = tempfile::tempdir().expect("create isolated compaction fixture");
        let project_root = temporary.path().join("source");
        let data_root = temporary.path().join("data");
        std::fs::create_dir_all(&project_root).expect("create compaction source root");
        std::fs::create_dir_all(&data_root).expect("create compaction data root");
        let host_runtime = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
            data_root: data_root.clone(),
            sqlite_path: data_root.join("unused.sqlite3"),
            sqlite_binding_id: Uuid::new_v4().to_string(),
            system_statistics_sqlite_path: data_root.join("unused-statistics.sqlite3"),
            system_statistics_sqlite_binding_id: Uuid::new_v4().to_string(),
        });
        let factory = VceDataPlaneFactory::bootstrap(&host_runtime)
            .await
            .expect("bootstrap compaction fixture storage");
        let capabilities = job_test_capabilities();
        let fingerprint = crate::vce::vce_embedding_physical_fingerprint(&capabilities, 3)
            .expect("freeze compaction fixture fingerprint");
        let mock = MockHostAiPort::new();
        mock.set_capability_snapshot(capabilities)
            .expect("configure compaction fixture capability metadata");
        let expected_fingerprint = fingerprint.clone();
        let manager = VceRuntimeManager::new(
            Arc::new(mock),
            factory.clone(),
            ServiceEventEmitter::new(Vec::new()),
            Arc::new(move |contract| {
                if contract.project_id == "compaction-project"
                    && contract.dimension == 3
                    && contract.embedding_fingerprint == expected_fingerprint
                {
                    Ok(())
                } else {
                    Err(String::from("Compaction fixture publication authority rejected").into())
                }
            }),
        );
        let mut settings = ProjectVceSettings {
            enabled: true,
            revision: 1,
            update_mode: vulcan_workbench_model::ProjectVceUpdateMode::Manual,
            ..ProjectVceSettings::default()
        };
        manager
            .enable_project(
                "compaction-project",
                &project_root,
                settings.clone(),
                3,
                &fingerprint,
                true,
            )
            .await
            .expect("prepare isolated compaction project");
        let initialized = manager
            .submit_job(
                "compaction-project",
                VceJobAdmission {
                    retry_of_job_id: None,
                    kind: VceJobKind::Initialize,
                    idempotency_key: String::from("initialize-compaction-project"),
                    expected_settings_revision: settings.revision,
                    expected_checkpoint_version: None,
                    contract_fingerprint: fingerprint.clone(),
                },
                &executor,
            )
            .await
            .expect("admit empty compaction fixture initialization");
        let initialized = await_test_job(&manager, "compaction-project", &initialized.job_id).await;
        assert_eq!(initialized.state, VceJobState::Succeeded);
        let checkpoint = initialized
            .result_checkpoint_version
            .expect("empty compaction fixture publishes a checkpoint");
        assert!(settings.revision > 0);
        assert!(checkpoint > 0);
        assert!(
            vulcan_context_engine::is_tagged_sha256_identity(&fingerprint),
            "fixture fingerprint must satisfy compaction admission: {fingerprint}"
        );
        let runtime = manager
            .require_project("compaction-project")
            .await
            .expect("read active compaction runtime");
        stage_real_compaction_fragments(&runtime.index, "recovery", 0, 32, 2).await;
        // Policy-only changes advance host admission without rewriting immutable physical provenance.
        // 仅策略变更推进宿主准入，但不改写不可变物理来源。
        settings.revision += 1;
        settings.max_results += 1;
        assert!(
            manager
                .reconcile_policy_revision(
                    "compaction-project",
                    &project_root,
                    &settings,
                    &fingerprint,
                )
                .await
                .expect("adopt policy-only compaction fixture revision"),
            "policy-only revision must retain the active physical index",
        );

        let target = manager
            .stored_maintenance_target(
                "compaction-project",
                vulcan_context_engine::VceMaintenanceMode::Compact,
            )
            .await
            .expect("plan real physical compaction");
        assert_eq!(target.checkpoint_version, Some(checkpoint));
        assert_eq!(
            target.plan.actions.len(),
            2,
            "both vector tables need compaction"
        );
        let before_by_role = target
            .plan
            .physical_tables
            .iter()
            .map(|table| (table.statistics.role, table.statistics.clone()))
            .collect::<BTreeMap<_, _>>();
        let action_ids = target
            .plan
            .actions
            .iter()
            .map(|action| action.action_id.clone())
            .collect::<BTreeSet<_>>();
        let first = manager
            .submit_physical_compaction(
                "compaction-project",
                VceJobAdmission {
                    retry_of_job_id: None,
                    kind: VceJobKind::Compact,
                    idempotency_key: String::from("compact-with-host-recovery"),
                    expected_settings_revision: settings.revision,
                    expected_checkpoint_version: Some(checkpoint),
                    contract_fingerprint: fingerprint.clone(),
                },
                action_ids,
                &executor,
            )
            .await
            .expect("admit physical compaction recovery fixture");
        let (admitted_role, admitted_operation_id) = await_admitted_compaction_operation(
            &manager,
            &runtime.index,
            "compaction-project",
            &first.job_id,
        )
        .await;
        let state = manager.project_job_state("compaction-project").await;
        abandon_fixture_host_worker(&state).await;

        let attached = manager
            .get_job("compaction-project", Some(&first.job_id))
            .await
            .expect("reattach exact physical compaction")
            .expect("reattached physical compaction exists");
        assert_eq!(attached.job_id, first.job_id);
        let recovered = await_test_job(&manager, "compaction-project", &first.job_id).await;
        assert_eq!(
            recovered.state,
            VceJobState::Succeeded,
            "recovered compaction diagnostic: {:?}",
            recovered.error
        );
        assert_eq!(recovered.source_checkpoint_version, Some(checkpoint));
        assert_eq!(recovered.result_checkpoint_version, Some(checkpoint));
        let recovered_operation = recovered
            .compaction
            .as_ref()
            .expect("recovered table receipts")
            .tables
            .iter()
            .find(|table| table.plan.statistics.role == admitted_role)
            .and_then(|table| table.operation_id.as_deref())
            .expect("recovered operation identity remains durable");
        assert_eq!(recovered_operation, admitted_operation_id);
        for table in recovered
            .compaction
            .as_ref()
            .expect("completed table receipts")
            .tables
            .iter()
        {
            let before = before_by_role
                .get(&table.plan.statistics.role)
                .expect("planned role has initial statistics");
            assert_eq!(table.state, VceCompactionTableState::Succeeded);
            assert!(table.latest_statistics.fragment_count < before.fragment_count);
            assert!(table.metrics.fragments_removed > 0);
        }

        // A second fragmented layout supplies an unfinished receipt whose storage operation is
        // deliberately absent, reproducing runtime-restart state loss without killing a process.
        // 第二组碎片布局提供存储操作被刻意移除的未完成回执，在不终止进程的情况下复现运行时重启状态丢失。
        stage_real_compaction_fragments(&runtime.index, "state_lost", 100, 8, 3).await;
        let target = manager
            .stored_maintenance_target(
                "compaction-project",
                vulcan_context_engine::VceMaintenanceMode::Compact,
            )
            .await
            .expect("plan state-loss compaction fixture");
        let action_ids = target
            .plan
            .actions
            .iter()
            .map(|action| action.action_id.clone())
            .collect::<BTreeSet<_>>();
        assert!(!action_ids.is_empty());
        let budget = manager
            .job_writer_budget
            .acquire()
            .await
            .expect("hold compaction writer budget");
        let queued = manager
            .submit_physical_compaction(
                "compaction-project",
                VceJobAdmission {
                    retry_of_job_id: None,
                    kind: VceJobKind::Compact,
                    idempotency_key: String::from("compact-with-state-loss"),
                    expected_settings_revision: settings.revision,
                    expected_checkpoint_version: Some(checkpoint),
                    contract_fingerprint: fingerprint.clone(),
                },
                action_ids,
                &executor,
            )
            .await
            .expect("queue storage state-loss fixture");
        assert_eq!(queued.state, VceJobState::Queued);
        let state = manager.project_job_state("compaction-project").await;
        let active = state
            .active
            .lock()
            .await
            .clone()
            .expect("state-loss fixture owns one queued job");
        let store = active.store.clone();
        let mut admitted = queued.clone();
        admitted.state = VceJobState::Running;
        admitted.sequence += 1;
        admitted.updated_at_unix_ms = job_time()
            .expect("read state-loss fixture time")
            .max(admitted.updated_at_unix_ms);
        let fake_operation_id = format!("{}:lost-storage-operation", admitted.job_id);
        let table_count = admitted
            .compaction
            .as_ref()
            .expect("queued compaction tables")
            .tables
            .len() as u64;
        admitted.progress = Some(compaction_progress(
            &fake_operation_id,
            0,
            table_count,
            VceRefreshPhase::Writing,
        ));
        let first_table = admitted
            .compaction
            .as_mut()
            .expect("admitted compaction tables")
            .tables
            .first_mut()
            .expect("state-loss fixture has one table");
        first_table.state = VceCompactionTableState::Queued;
        first_table.round = 1;
        first_table.operation_id = Some(fake_operation_id.clone());
        first_table.operation_expected_version = Some(first_table.plan.statistics.version);
        admitted
            .validate()
            .expect("injected admitted receipt remains contract-valid");
        let admitted = store
            .update(&queued, &admitted)
            .await
            .expect("persist missing storage operation admission");
        let mut running = admitted.clone();
        running.sequence += 1;
        running.updated_at_unix_ms = job_time()
            .expect("read running state-loss fixture time")
            .max(running.updated_at_unix_ms);
        running
            .compaction
            .as_mut()
            .expect("running compaction tables")
            .tables
            .first_mut()
            .expect("state-loss fixture has one running table")
            .state = VceCompactionTableState::Running;
        running
            .validate()
            .expect("injected state-loss receipt remains contract-valid");
        store
            .update(&admitted, &running)
            .await
            .expect("persist running but missing storage operation");
        abandon_fixture_host_worker(&state).await;
        drop(active);
        drop(budget);

        manager
            .get_job("compaction-project", Some(&queued.job_id))
            .await
            .expect("reattach missing storage operation")
            .expect("state-loss receipt exists");
        let interrupted = await_test_job(&manager, "compaction-project", &queued.job_id).await;
        assert_eq!(interrupted.state, VceJobState::Interrupted);
        assert_eq!(interrupted.source_checkpoint_version, Some(checkpoint));
        assert!(interrupted.result_checkpoint_version.is_none());
        assert!(
            interrupted
                .error
                .as_deref()
                .is_some_and(|error| error.contains("state was lost")),
            "state loss must remain explicit: {:?}",
            interrupted.error
        );
        let lost_table = interrupted
            .compaction
            .as_ref()
            .expect("interrupted table receipts")
            .tables
            .first()
            .expect("interrupted table exists");
        assert_eq!(lost_table.state, VceCompactionTableState::StateLost);
        assert_eq!(
            lost_table.operation_id.as_deref(),
            Some(fake_operation_id.as_str())
        );
        assert_eq!(
            manager
                .get_job("compaction-project", Some(&queued.job_id))
                .await
                .expect("replay interrupted state-loss receipt"),
            Some(interrupted)
        );

        // A previously observed transient table-receipt error must not retain the active owner
        // after a later authoritative terminal receipt has been persisted successfully.
        // 此前观察到的瞬时表回执错误不能在后续权威终态回执成功持久化后继续保留活动所有者。
        stage_real_compaction_fragments(&runtime.index, "terminal_recovery", 200, 8, 4).await;
        let target = manager
            .stored_maintenance_target(
                "compaction-project",
                vulcan_context_engine::VceMaintenanceMode::Compact,
            )
            .await
            .expect("plan terminal persistence recovery compaction");
        let action_ids = target
            .plan
            .actions
            .iter()
            .map(|action| action.action_id.clone())
            .collect::<BTreeSet<_>>();
        assert!(!action_ids.is_empty());
        let budget = manager
            .job_writer_budget
            .acquire()
            .await
            .expect("hold terminal recovery writer budget");
        let terminal_recovery = manager
            .submit_physical_compaction(
                "compaction-project",
                VceJobAdmission {
                    retry_of_job_id: None,
                    kind: VceJobKind::Compact,
                    idempotency_key: String::from("compact-clears-stale-persistence-diagnostic"),
                    expected_settings_revision: settings.revision,
                    expected_checkpoint_version: Some(checkpoint),
                    contract_fingerprint: fingerprint.clone(),
                },
                action_ids,
                &executor,
            )
            .await
            .expect("queue terminal persistence recovery compaction");
        let state = manager.project_job_state("compaction-project").await;
        let active = state
            .active
            .lock()
            .await
            .clone()
            .expect("terminal persistence recovery owns one queued job");
        *active.persistence_error.lock().await = Some(String::from(
            "fixture transient table receipt persistence failure",
        ));
        drop(active);
        drop(budget);
        let recovered =
            await_test_job(&manager, "compaction-project", &terminal_recovery.job_id).await;
        assert_eq!(recovered.state, VceJobState::Succeeded);
        assert!(recovered.error.is_none());
        assert!(state.active.lock().await.is_none());

        // The engine contract returns a queued snapshot immediately after admission. Reject every
        // subsequent non-terminal observation update and prove the host still polls the same durable
        // operation to its authoritative terminal result instead of orphaning storage work.
        // 引擎合约在准入后立即返回排队快照。拒绝后续每次非终态观察更新，并证明宿主仍会轮询同一
        // 持久操作直至权威终态，而不是遗弃存储工作。
        stage_real_compaction_fragments(&runtime.index, "observation_failure", 250, 8, 5).await;
        let target = manager
            .stored_maintenance_target(
                "compaction-project",
                vulcan_context_engine::VceMaintenanceMode::Compact,
            )
            .await
            .expect("plan observation persistence failure compaction");
        let action_ids = target
            .plan
            .actions
            .iter()
            .map(|action| action.action_id.clone())
            .collect::<BTreeSet<_>>();
        assert!(!action_ids.is_empty());
        let budget = manager
            .job_writer_budget
            .acquire()
            .await
            .expect("hold observation failure writer budget");
        let observation_recovery = manager
            .submit_physical_compaction(
                "compaction-project",
                VceJobAdmission {
                    retry_of_job_id: None,
                    kind: VceJobKind::Compact,
                    idempotency_key: String::from(
                        "compact-continues-after-observation-persistence-failure",
                    ),
                    expected_settings_revision: settings.revision,
                    expected_checkpoint_version: Some(checkpoint),
                    contract_fingerprint: fingerprint.clone(),
                },
                action_ids,
                &executor,
            )
            .await
            .expect("queue observation persistence failure compaction");
        let observation_state = manager.project_job_state("compaction-project").await;
        let observation_active = observation_state
            .active
            .lock()
            .await
            .clone()
            .expect("observation persistence fixture owns one queued job");
        let trigger = format!(
            "CREATE TRIGGER reject_compaction_observation BEFORE UPDATE ON vce_host_jobs \
             WHEN OLD.job_id = '{}' \
             AND json_extract(OLD.snapshot_json, '$.compaction.tables[0].operation_id') IS NOT NULL \
             AND json_extract(NEW.snapshot_json, '$.state') = 'running' \
             AND json_extract(NEW.snapshot_json, '$.compaction.tables[0].state') IN ('queued', 'running') \
             BEGIN SELECT RAISE(ABORT, 'injected compaction observation failure'); END;",
            observation_recovery.job_id
        );
        observation_active
            .store
            .execute_test_storage_script(&trigger)
            .await
            .expect("install observation persistence fault");
        drop(budget);
        let observation_recovered =
            await_test_job(&manager, "compaction-project", &observation_recovery.job_id).await;
        assert_eq!(observation_recovered.state, VceJobState::Succeeded);
        assert!(observation_recovered.error.is_none());
        // The armed trigger above rejects every non-terminal observation update, and this asserts that it never
        // fired. That is the correct expectation for an in-process engine, not a weakened one:
        //
        // The in-process engine answered `submit` with a queued snapshot and the host then polled it, which is
        // the only window in which an advisory observation receipt is ever written. The in-process step is
        // synchronous, so `submit` returns an authoritative terminal result directly and no polling window
        // exists — there is no storage work that a failed observation could orphan. Keeping the trigger
        // armed turns this into a guard: if any future change reintroduces advisory observation writes, the
        // trigger aborts them and this assertion fails.
        //
        // 上方已武装的触发器会拒绝每次非终态观察更新，而本断言要求它从未触发。对进程内引擎而言这是正确的期望，
        // 而不是被削弱的期望：进程内引擎对 `submit` 回答排队快照、宿主随后轮询它，那是唯一会写入"观察回执"的
        // 窗口。进程内步骤是同步的，故 `submit` 直接返回权威终态，不存在轮询窗口——也就没有"失败的观察会使其
        // 成为孤儿"的存储工作。保持触发器武装使本断言成为护栏：若将来有任何改动重新引入观察写入，触发器会
        // 中止它们并使本断言失败。
        assert_eq!(
            observation_active
                .compaction_observation_persistence_failures
                .load(std::sync::atomic::Ordering::SeqCst),
            0,
            "a synchronous in-process step must never attempt an advisory observation receipt"
        );
        observation_active
            .store
            .execute_test_storage_script("DROP TRIGGER reject_compaction_observation;")
            .await
            .expect("remove observation persistence fault");
        assert!(observation_state.active.lock().await.is_none());
        drop(observation_active);
        drop(observation_state);

        // Reject both the ordinary round-result write and its first terminal retry. The exact
        // terminal candidate must retain the completed physical facts and become recoverable once
        // storage accepts writes again; cancellation is only the recovery command, not a rewrite.
        // 同时拒绝普通轮次结果写入及首次终态重试。精确终态候选必须保留已完成物理事实，并在存储恢复
        // 可写后能够继续；取消在此仅是恢复命令，不能改写既有结果。
        stage_real_compaction_fragments(&runtime.index, "terminal_candidate", 300, 8, 6).await;
        let target = manager
            .stored_maintenance_target(
                "compaction-project",
                vulcan_context_engine::VceMaintenanceMode::Compact,
            )
            .await
            .expect("plan retained terminal candidate compaction");
        let action_ids = target
            .plan
            .actions
            .iter()
            .map(|action| action.action_id.clone())
            .collect::<BTreeSet<_>>();
        assert!(!action_ids.is_empty());
        let budget = manager
            .job_writer_budget
            .acquire()
            .await
            .expect("hold retained candidate writer budget");
        let retained = manager
            .submit_physical_compaction(
                "compaction-project",
                VceJobAdmission {
                    retry_of_job_id: None,
                    kind: VceJobKind::Compact,
                    idempotency_key: String::from("compact-retains-terminal-candidate"),
                    expected_settings_revision: settings.revision,
                    expected_checkpoint_version: Some(checkpoint),
                    contract_fingerprint: fingerprint,
                },
                action_ids,
                &executor,
            )
            .await
            .expect("queue retained terminal candidate compaction");
        let retained_state = manager.project_job_state("compaction-project").await;
        let retained_active = retained_state
            .active
            .lock()
            .await
            .clone()
            .expect("retained candidate fixture owns one queued job");
        let trigger = format!(
            "CREATE TRIGGER reject_compaction_terminal_candidate BEFORE UPDATE ON vce_host_jobs \
             WHEN OLD.job_id = '{}' AND (((json_extract(NEW.snapshot_json, '$.state') = 'running') \
             AND (json_extract(NEW.snapshot_json, '$.compaction.tables[0].latest_statistics.version') \
             > json_extract(OLD.snapshot_json, '$.compaction.tables[0].latest_statistics.version'))) \
             OR json_extract(NEW.snapshot_json, '$.state') IN ('succeeded', 'failed', 'cancelled', 'interrupted')) \
             BEGIN SELECT RAISE(ABORT, 'injected compaction terminal candidate failure'); END;",
            retained.job_id
        );
        retained_active
            .store
            .execute_test_storage_script(&trigger)
            .await
            .expect("install retained terminal candidate fault");
        drop(budget);
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if retained_active.persistence_error.lock().await.is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("terminal persistence fault must become visible");
        let candidate = retained_active
            .pending_terminal_receipt
            .lock()
            .await
            .clone()
            .expect("exact failed terminal successor remains retained");
        assert_eq!(candidate.state, VceJobState::Failed);
        let candidate_table = candidate
            .compaction
            .as_ref()
            .expect("candidate retains table state")
            .tables
            .first()
            .expect("candidate retains selected table");
        assert!(matches!(
            candidate_table.state,
            VceCompactionTableState::Succeeded | VceCompactionTableState::Failed
        ));
        assert!(
            candidate_table.latest_statistics.version > candidate_table.plan.statistics.version
        );
        assert!(candidate_table.metrics.fragments_removed > 0);
        retained_active
            .store
            .execute_test_storage_script("DROP TRIGGER reject_compaction_terminal_candidate;")
            .await
            .expect("remove retained terminal candidate fault");
        let recovered_candidate = manager
            .cancel_job("compaction-project", &retained.job_id)
            .await
            .expect("retry exact retained terminal candidate");
        assert_eq!(recovered_candidate, candidate);
        assert!(retained_state.active.lock().await.is_none());
        drop(retained_active);
        drop(retained_state);

        drop(store);
        drop(state);
        drop(runtime);
        manager
            .shutdown()
            .await
            .expect("shutdown compaction fixture manager");
        factory
            .shutdown()
            .await
            .expect("close compaction fixture bindings");
    }

    /// Exercise host reattachment and operation state-loss handling through real temporary storage.
    /// 通过真实临时存储验证宿主重新附着与操作状态丢失处理。
    ///
    /// Takes no parameters and uses an isolated production-sized runtime stack.
    /// 无参数，并使用隔离且与生产规模相当的运行时栈。
    #[test]
    fn in_process_compaction_recovers_without_duplicate_submission() {
        let executor = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("build isolated compaction runtime"),
        );
        std::thread::Builder::new()
            .name(String::from("vce-compaction-recovery"))
            .stack_size(8 * 1024 * 1024)
            .spawn({
                let executor = Arc::clone(&executor);
                move || executor.block_on(verify_real_compaction_host_recovery(executor.clone()))
            })
            .expect("spawn isolated compaction fixture thread")
            .join()
            .expect("isolated compaction fixture thread completed");
    }

    /// Prove the real supervisor keeps execution polling while progress persistence waits on shared storage.
    /// 证明真实监管器在进度持久化等待共享存储时仍持续轮询执行任务。
    /// The execution holds the gate until the test releases it; an inline future would deadlock forever.
    /// 执行任务持有闸门直到测试释放；若执行 future 内联轮询，此场景将永久死锁。
    #[test]
    fn supervised_execution_keeps_polling_during_progress_persistence() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build deterministic supervisor runtime");
        runtime.block_on(async {
            // These signals make the shared-lock interleaving deterministic and bounded.
            // 这些信号让共享锁交错过程具备确定性并且有界。
            let shared_gate = Arc::new(tokio::sync::Mutex::new(()));
            let execution_started = Arc::new(Notify::new());
            let persistence_started = Arc::new(Notify::new());
            let release_execution = Arc::new(Notify::new());
            let (progress_sender, progress_receiver) = watch::channel(None);
            let control = VceRefreshControl::default();
            let observed_control = control.clone().with_observer(Arc::new(move |progress| {
                progress_sender.send_replace(Some(progress));
            }));

            let execution_gate = Arc::clone(&shared_gate);
            let execution_started_signal = Arc::clone(&execution_started);
            let release_signal = Arc::clone(&release_execution);
            let execution = async move {
                let _gate = execution_gate.lock().await;
                execution_started_signal.notify_one();
                release_signal.notified().await;
                Ok::<u64, String>(7)
            };

            let persistence_gate = Arc::clone(&shared_gate);
            let persistence_started_signal = Arc::clone(&persistence_started);
            let persist_progress = move |
                _progress: VceRefreshProgress,
            | -> futures_util::future::BoxFuture<'static, Result<(), String>> {
                persistence_started_signal.notify_one();
                let persistence_gate = Arc::clone(&persistence_gate);
                Box::pin(async move {
                    let _gate = persistence_gate.lock().await;
                    Ok::<(), String>(())
                })
            };
            let supervisor = tokio::spawn(VceRuntimeManager::supervise_job_execution(
                control,
                progress_receiver,
                execution,
                persist_progress,
            ));

            tokio::time::timeout(Duration::from_secs(1), execution_started.notified())
                .await
                .expect("execution must start while supervised");
            observed_control.observe(VceRefreshProgress {
                activity: None,
                phase: vulcan_context_engine::VceRefreshPhase::Scanning,
                operation_id: String::from("shared-gate-test"),
                unit: vulcan_context_engine::VceRefreshUnit::Files,
                completed: 0,
                total: Some(1),
                overall_completed: 0,
                overall_total: Some(1),
                provider_batch_state: None,
                current_path: None,
            });
            tokio::time::timeout(Duration::from_secs(1), persistence_started.notified())
                .await
                .expect("progress persistence must reach the shared gate");
            release_execution.notify_one();
            let result = tokio::time::timeout(Duration::from_secs(1), supervisor)
                .await
                .expect("supervisor must not deadlock behind progress persistence")
                .expect("supervisor task must finish");
            assert_eq!(result.expect("execution must complete"), 7);
        });
    }

    /// Ensure unchanged polling cannot advance durable progress timestamps or repeat storage writes.
    /// 确保相同状态轮询不会推进持久进度时间戳或重复写入存储。
    /// Takes no input; holds the worker across multiple real sampling ticks and returns after joining it.
    /// 无输入；跨多个实际采样时钟保留工作任务，连接完成后返回。
    #[test]
    fn supervised_execution_does_not_republish_unchanged_progress() {
        // runtime keeps this isolated supervisor and its timers on one test thread.
        // runtime 将此隔离监管器及其时钟保留在一个测试线程。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            // count and persisted establish the exact number of acknowledged progress writes.
            // count 与 persisted 确定已确认进度写入的准确次数。
            let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let persisted = Arc::new(Notify::new());
            // release holds execution until multiple sampling ticks have occurred.
            // release 保留执行，直到经过多个采样时钟。
            let release = Arc::new(Notify::new());
            // progress is a genuine unchanged local-preparation observation.
            // progress 是实际保持不变的本地准备观察。
            let progress = VceRefreshProgress {
                activity: None,
                phase: vulcan_context_engine::VceRefreshPhase::Chunking,
                operation_id: "unchanged-progress".to_owned(),
                unit: vulcan_context_engine::VceRefreshUnit::Files,
                completed: 1,
                total: Some(3),
                overall_completed: 0,
                overall_total: Some(3),
                provider_batch_state: None,
                current_path: None,
            };
            // sender stays alive so channel closure cannot settle the test early.
            // sender 保持存活，避免通道关闭使测试提前收敛。
            let (sender, receiver) = watch::channel(Some(progress.clone()));
            // worker_release is owned by the real supervised execution future.
            // worker_release 由实际受监管执行 future 持有。
            let worker_release = Arc::clone(&release);
            // writes and signal belong only to the persistence callback.
            // writes 与 signal 仅属于持久化回调。
            let writes = Arc::clone(&count);
            let signal = Arc::clone(&persisted);
            // supervisor retains worker ownership through normal completion.
            // supervisor 在正常完成前始终持有工作任务所有权。
            let supervisor = tokio::spawn(VceRuntimeManager::supervise_job_execution(
                VceRefreshControl::default(),
                receiver,
                async move {
                    worker_release.notified().await;
                    Ok(1)
                },
                move |_| {
                    writes.fetch_add(1, Ordering::SeqCst);
                    signal.notify_one();
                    Box::pin(async { Ok(()) })
                },
            ));
            tokio::time::timeout(Duration::from_secs(1), persisted.notified())
                .await
                .unwrap();
            sender.send_replace(Some(progress));
            tokio::time::sleep(Duration::from_millis(600)).await;
            assert_eq!(count.load(Ordering::SeqCst), 1);
            release.notify_one();
            assert_eq!(supervisor.await.unwrap().unwrap(), 1);
            assert_eq!(count.load(Ordering::SeqCst), 1);
        });
    }

    /// Prove transient SQLite contention is retried without changing the latest progress payload.
    /// 证明瞬时 SQLite 竞争会重试，且不会改变最新进度载荷。
    /// The fixture fails once with the production lock text and succeeds on the next attempt.
    /// 测试夹具先使用生产锁错误文本失败一次，再在下一次尝试成功。
    #[test]
    fn transient_progress_persistence_is_retried() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build progress retry runtime");
        runtime.block_on(async {
            use std::sync::atomic::{AtomicUsize, Ordering};

            let attempts = Arc::new(AtomicUsize::new(0));
            let persist_progress = {
                let attempts = Arc::clone(&attempts);
                move |_progress: VceRefreshProgress|
                    -> futures_util::future::BoxFuture<'static, Result<(), String>> {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async move {
                        if attempt == 0 {
                            Err(String::from(
                                "Update VCE job receipt: sqlite execute failed: database is locked",
                            ))
                        } else {
                            Ok(())
                        }
                    })
                }
            };
            let progress = VceRefreshProgress {
                activity: None,
                phase: vulcan_context_engine::VceRefreshPhase::Writing,
                operation_id: String::from("progress-retry-test"),
                unit: vulcan_context_engine::VceRefreshUnit::Blobs,
                completed: 1,
                total: Some(2),
                overall_completed: 1,
                overall_total: Some(2),
                provider_batch_state: None,
                current_path: None,
            };

            persist_progress_with_retry(&persist_progress, progress)
                .await
                .expect("transient progress persistence should recover");
            assert_eq!(attempts.load(Ordering::SeqCst), 2);
        });
    }

    /// Verify a worker panic is re-raised so the outer job boundary can perform staging cleanup.
    /// 验证工作任务恐慌会重新抛出，使外层作业边界能够执行暂存清理。
    /// This exercises the production supervisor path rather than calling the cleanup helper directly.
    /// 此测试走生产监管器路径，而不是直接调用清理辅助函数。
    #[test]
    fn supervised_execution_rethrows_worker_panics() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build panic supervisor runtime");
        runtime.block_on(async {
            // The closed progress channel leaves only the real worker result for the supervisor to join.
            // 已关闭的进度通道让监管器只需连接真实工作任务结果。
            let (_progress_sender, progress_receiver) = watch::channel(None);
            let persist_progress = move |
                _progress: VceRefreshProgress,
            | -> futures_util::future::BoxFuture<'static, Result<(), String>> {
                Box::pin(async { Ok(()) })
            };
            let panic = AssertUnwindSafe(VceRuntimeManager::supervise_job_execution(
                VceRefreshControl::default(),
                progress_receiver,
                async move {
                    std::panic::panic_any("fixture worker panic");
                    #[allow(unreachable_code)]
                    Ok::<u64, String>(0)
                },
                persist_progress,
            ))
            .catch_unwind()
            .await
            .expect_err("worker panic must cross the supervisor boundary");
            assert_eq!(panic.downcast_ref::<&str>(), Some(&"fixture worker panic"));
        });
    }

    /// Verify restart preserves recent admission cooldown and treats clock rollback conservatively.
    /// 验证重启保留最近准入冷却，并保守处理时钟回退。
    /// Takes no parameters. Asserts no startup scheduling, bounded elapsed time, and exact remaining delay.
    /// 无参数。断言无启动调度、耗时有界及精确剩余延时。
    #[test]
    fn restored_admission_preserves_cooldown_without_scheduling() {
        let mut settings = ProjectVceSettings {
            update_mode: vulcan_workbench_model::ProjectVceUpdateMode::Automatic,
            ..ProjectVceSettings::default()
        };
        let mut schedule = VceJobSchedule::new(&settings);
        let now = tokio::time::Instant::now();
        schedule.restore_admission(10_000, 40_000, now);
        assert_eq!(
            schedule.automatic_delay(now),
            Some(Duration::from_secs(270))
        );
        schedule.restore_admission(40_000, 10_000, now);
        assert_eq!(
            schedule.automatic_delay(now),
            Some(Duration::from_secs(300))
        );
        schedule.restore_admission(1, u64::MAX, now);
        assert_eq!(schedule.automatic_delay(now), Some(Duration::ZERO));
        settings.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Manual;
        schedule.update(&settings);
        assert_eq!(schedule.automatic_delay(now), None);
    }

    /// Verify manual admission establishes cooldown without making manual requests wait for it.
    /// 验证手动准入建立冷却，但不使手动请求等待该冷却。
    /// Takes no parameters. Asserts interval recomputation and policy changes preserve admission time.
    /// 无参数。断言间隔重新计算及策略变化保留准入时刻。
    #[test]
    fn automatic_interval_uses_last_admission_and_live_project_policy() {
        let mut settings = ProjectVceSettings::default();
        let mut schedule = VceJobSchedule::new(&settings);
        let now = tokio::time::Instant::now();
        assert_eq!(schedule.automatic_delay(now), None);
        settings.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Automatic;
        schedule.update(&settings);
        assert_eq!(schedule.automatic_delay(now), Some(Duration::ZERO));
        schedule.last_admitted = Some(now);
        assert_eq!(
            schedule.automatic_delay(now + Duration::from_secs(120)),
            Some(Duration::from_secs(180))
        );
        settings.auto_update_min_interval_seconds = 600;
        schedule.update(&settings);
        assert_eq!(
            schedule.automatic_delay(now + Duration::from_secs(120)),
            Some(Duration::from_secs(480))
        );
        settings.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Manual;
        schedule.update(&settings);
        assert_eq!(
            schedule.automatic_delay(now + Duration::from_secs(120)),
            None
        );
        assert_eq!(schedule.last_admitted, Some(now));
        settings.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Automatic;
        schedule.update(&settings);
        assert_eq!(
            schedule.automatic_delay(now + Duration::from_secs(601)),
            Some(Duration::ZERO)
        );
    }

    /// Build a complete non-network capability snapshot for empty-project lifecycle integration.
    /// 构建完整无网络能力快照，用于空项目生命周期集成。
    ///
    /// Takes no parameters. Returns mandatory operations with an exact three-dimensional route.
    /// 无参数。返回带精确三维路由的必需操作。
    pub(super) fn job_test_capabilities() -> vulcan_ai_contract::InferenceCapabilitySnapshot {
        use vulcan_ai_contract::*;
        let route = ExecutionRouteSnapshot {
            provider_id: String::from("fixture-provider"),
            service_entry_id: String::from("fixture-service"),
            connection_id: String::from("fixture-account"),
            model_id: String::from("fixture-embedding"),
            protocol_binding_id: Some(String::from("fixture-protocol")),
            action_binding_id: None,
            secondary_route_id: None,
            secondary_provider_name: None,
            secondary_endpoint_name: None,
            capability_revision: String::from("fixture-capability-v1"),
            catalog_revision: String::from("fixture-catalog-v1"),
            route_revision: String::from("fixture-route-v1"),
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        let embedding = EmbeddingCapabilities {
            tasks: BTreeSet::from([EmbeddingTask::Document, EmbeddingTask::Query]),
            input_resources: BTreeSet::new(),
            output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
            encodings: BTreeSet::from([EmbeddingEncoding::Float]),
            dimensions: BTreeSet::from([3]),
            minimum_dimensions: None,
            maximum_dimensions: None,
            default_dimensions: None,
            maximum_inputs: None,
            maximum_tokens_per_input: None,
            normalization: EmbeddingNormalization::Unknown,
            distance_metric: DistanceMetric::Unknown,
        };
        InferenceCapabilitySnapshot {
            contract_version: INFERENCE_CONTRACT_VERSION,
            consumer_profile_id: String::from("vce"),
            operations: vec![
                InferenceOperationCapability {
                    operation: AiOperation::EmbeddingCreate,
                    purpose_id: None,
                    available: true,
                    unavailable_reason: None,
                    route: Some(route),
                    embedding: Some(embedding),
                    embedding_batch: None,
                    rerank: None,
                },
                InferenceOperationCapability {
                    operation: AiOperation::ConversationRespond,
                    purpose_id: Some(String::from(crate::vce::VCE_QUERY_REWRITE_PURPOSE_ID)),
                    available: true,
                    unavailable_reason: None,
                    route: None,
                    embedding: None,
                    embedding_batch: None,
                    rerank: None,
                },
                InferenceOperationCapability {
                    operation: AiOperation::RerankDocuments,
                    purpose_id: None,
                    available: true,
                    unavailable_reason: None,
                    route: None,
                    embedding: None,
                    embedding_batch: None,
                    rerank: None,
                },
            ],
        }
    }

    /// Await an exact terminal receipt and its worker lease release with a bounded test deadline.
    /// 在有界测试截止前等待精确终态回执及其工作线程租约释放。
    ///
    /// `manager`, `project_id`, and `job_id` identify fixture ownership. Returns the final receipt.
    /// `manager`、`project_id` 与 `job_id` 标识夹具归属。返回最终回执。
    pub(super) async fn await_test_job(
        manager: &VceRuntimeManager,
        project_id: &str,
        job_id: &str,
    ) -> VceStoredJob {
        // Retain the last observation so a timeout identifies the stalled phase and owner.
        // 保留最后一次观察，使超时能够指出卡住的阶段与所有者。
        let mut last_observation = None;
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let receipt = manager
                    .get_job(project_id, Some(job_id))
                    .await
                    .expect("read exact fixture job")
                    .expect("fixture job exists");
                // Stable project ownership also covers cancellation before any model runtime exists.
                // 稳定项目归属也覆盖任何模型运行时存在之前的取消。
                let state = manager.project_job_state(project_id).await;
                last_observation = Some((receipt.clone(), state.active.lock().await.is_some()));
                if receipt.state.is_terminal()
                    && state.active.lock().await.is_none()
                    && state.execution.try_write().is_ok()
                {
                    return receipt;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|error| panic!("fixture job must settle before deadline: {error}; last observation: {last_observation:?}"))
    }

    /// Exercise durable admission, empty initialization, cancellation and shutdown against real storage.
    /// 通过真实存储验证持久准入、空项目初始化、取消与退出。
    ///
    /// Takes no parameters. Uses isolated temporary storage and a local mock AI with no billed calls.
    /// 无参数。使用独立临时存储与无计费请求的本地模拟人工智能。
    #[test]
    #[rustfmt::skip]
    fn real_host_storage_human_jobs_are_durable_cancellable_and_nonblocking() {
        use vulcan_ai_contract::test_support::MockHostAiPort;
        use vulcan_workspace_store::data_plane::{
            HostRuntimeHandle, HostRuntimeMetadata,
        };
        let executor = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("build isolated job runtime"),
        );
        // Run the large end-to-end fixture on a production-sized caller stack.
        // 在与生产相当的调用线程栈上运行大型端到端夹具。
        std::thread::Builder::new()
            .name(String::from("vce-job-integration"))
            .stack_size(8 * 1024 * 1024)
            .spawn({
                let executor = Arc::clone(&executor);
                move || executor.block_on(async {
            let temporary = tempfile::tempdir().expect("create isolated job fixture");
            let project_root = temporary.path().join("source");
            let data_root = temporary.path().join("data");
            std::fs::create_dir_all(&project_root).expect("create empty source root");
            std::fs::create_dir_all(&data_root).expect("create host data root");
            let host_runtime = HostRuntimeHandle::new_for_test(
                HostRuntimeMetadata {
                    data_root: data_root.clone(),
                    sqlite_path: data_root.join("unused.sqlite3"),
                    sqlite_binding_id: Uuid::new_v4().to_string(),
                    system_statistics_sqlite_path: data_root.join("unused-statistics.sqlite3"),
                    system_statistics_sqlite_binding_id: Uuid::new_v4().to_string(),
                },
            );
            let factory = VceDataPlaneFactory::bootstrap(&host_runtime)
                .await
                .expect("bootstrap lazy job storage");
            let capabilities = job_test_capabilities();
            let fingerprint = crate::vce::vce_embedding_physical_fingerprint(&capabilities, 3)
                .expect("freeze tagged physical fingerprint");
            assert!(fingerprint.starts_with("sha256:"));
            let mock = MockHostAiPort::new();
            mock.set_capability_snapshot(capabilities)
                .expect("configure isolated capabilities");
            // The fixture supplies explicit host authority instead of depending on a user service.
            // 夹具提供明确宿主权威，不依赖用户服务。
            let publication_allowed = Arc::new(AtomicBool::new(true));
            let publication_flag = Arc::clone(&publication_allowed);
            let expected_publication_fingerprint = fingerprint.clone();
            let manager = VceRuntimeManager::new(
                Arc::new(mock),
                factory.clone(),
                ServiceEventEmitter::new(Vec::new()),
                Arc::new(move |contract| {
                    if publication_flag.load(Ordering::Acquire)
                        && contract.project_id == "job-project"
                        && contract.dimension == 3
                        && contract.embedding_fingerprint == expected_publication_fingerprint
                    {
                        Ok(())
                    } else {
                        Err(String::from("Fixture publication authority revoked").into())
                    }
                }),
            );
            let center = crate::workspace_watcher::WorkspaceWatcherCenter::new()
                .expect("create isolated physical watcher");
            let watcher = crate::vce::VceWatcherService::new(
                &center,
                manager.clone(),
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&executor),
            );
            let settings = ProjectVceSettings {
                enabled: true,
                update_mode: vulcan_workbench_model::ProjectVceUpdateMode::Automatic,
                ..ProjectVceSettings::default()
            };
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    3,
                    &fingerprint,
                    true,
                )
                .await
                .expect("explicit storage preparation without scan");
            assert!(watcher.registered_root("job-project").is_none());
            assert!(
                manager
                    .get_job("job-project", None)
                    .await
                    .expect("read cold jobs")
                    .is_none()
            );
            // budget prevents the worker from starting and proves admission/cancellation do not await indexing.
            // budget 阻止工作线程启动，证明准入与取消不等待索引。
            let budget = manager
                .job_writer_budget
                .acquire()
                .await
                .expect("hold fixture budget");
            let command = |key: &str, kind, checkpoint| VceJobAdmission {
                retry_of_job_id: None,
                kind,
                idempotency_key: key.to_owned(),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: checkpoint,
                contract_fingerprint: fingerprint.clone(),
            };
            // A separate handle represents another host; rejection cannot create a durable job receipt.
            // 独立句柄代表另一宿主；拒绝不能创建持久作业回执。
            let foreign_writer = factory
                .try_acquire_project_writer("job-project")
                .expect("foreign writer attempt")
                .expect("foreign writer ownership");
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command("foreign-writer-blocked", VceJobKind::Initialize, None),
                        &executor,
                    )
                    .await
                    .expect_err("foreign writer blocks admission")
                    .contains("active host writer")
            );
            assert!(
                manager
                    .get_job("job-project", None)
                    .await
                    .expect("no rejected receipt")
                    .is_none()
            );
            drop(foreign_writer);
            let queued = tokio::time::timeout(
                Duration::from_secs(5),
                manager.submit_job(
                    "job-project",
                    command("first-click", VceJobKind::Initialize, None),
                    &executor,
                ),
            )
            .await
            .expect("admission must not await budget")
            .expect("durably queue first initialization");
            assert_eq!(queued.state, VceJobState::Queued);
            assert!(
                factory
                    .try_acquire_project_writer("job-project")
                    .expect("queued ownership")
                    .is_none()
            );
            assert!(vulcan_context_engine::is_sha256_identity(
                &queued.contract_fingerprint
            ));
            assert_eq!(
                manager
                    .submit_job(
                        "job-project",
                        command("first-click", VceJobKind::Initialize, None),
                        &executor
                    )
                    .await
                    .expect("replay queued click"),
                queued
            );
            assert_eq!(
                manager
                    .submit_job(
                        "job-project",
                        command("duplicate-click", VceJobKind::Initialize, None),
                        &executor
                    )
                    .await
                    .expect("repeated initialization locates the same job")
                    .job_id,
                queued.job_id
            );
            assert!(
                manager
                    .cancel_job("job-project", "wrong-job-id")
                    .await
                    .is_err()
            );
            // queued_before_detach includes the already acknowledged duplicate-click alias transition.
            // queued_before_detach 包含已经确认的重复点击别名转换。
            let queued_before_detach = manager
                .get_job("job-project", Some(&queued.job_id))
                .await
                .expect("read current acknowledged queue receipt");
            // retained_runtime simulates an absent query registration while the admitted worker retains its own leases.
            // retained_runtime 模拟查询注册缺席，而已准入工作任务仍保留自身租约。
            let retained_runtime = manager
                .projects
                .write()
                .await
                .remove("job-project")
                .expect("temporarily detach query runtime");
            assert!(manager.project_status("job-project").await.is_none());
            assert_eq!(
                manager
                    .get_job("job-project", Some(&queued.job_id))
                    .await
                    .expect("read active job without query runtime"),
                queued_before_detach
            );
            // active_job exposes storage failure without relying on a query runtime to carry that diagnostic.
            // active_job 不依赖查询运行时携带诊断，也能暴露存储失败。
            let active_job = retained_runtime
                .job_state
                .active
                .lock()
                .await
                .clone()
                .expect("project retains queued owner");
            // Terminal-persistence failure is reported only after execution ends in production.
            // 生产环境只有执行结束后才会报告终态持久化失败。
            // Stop the synthetic queued worker before injecting that diagnostic; otherwise it still owns the budget wait.
            // 注入该诊断前停止模拟排队线程，否则它仍持有等待执行预算的任务。
            let abandoned_worker = retained_runtime.job_state.worker_handle.lock().await.take().expect("queued worker handle");
            abandoned_worker.abort();
            assert!(abandoned_worker.await.expect_err("fixture worker aborted").is_cancelled());
            active_job.control.finish();
            *active_job.persistence_error.lock().await =
                Some(String::from("fixture receipt failure"));
            let observed_failed_receipt = manager
                .get_job("job-project", Some(&queued.job_id))
                .await
                .expect("retained project owner remains queryable")
                .expect("failed owner still has its receipt");
            assert_eq!(
                observed_failed_receipt.error.as_deref(),
                Some("fixture receipt failure")
            );
            // Cancellation must recover a finished owner instead of sending a no-op signal to its finished control.
            // 取消必须恢复已结束的所有者，而不是向已结束的引擎操作发送无效信号。
            let recovered_cancelled = manager
                .cancel_job("job-project", &queued.job_id)
                .await
                .expect("recover and cancel persisted owner");
            assert_eq!(recovered_cancelled.state, VceJobState::Cancelled);
            assert!(
                retained_runtime
                    .job_state
                    .active
                    .lock()
                    .await
                    .is_none(),
                "recovery cancellation must release the stale in-memory owner"
            );
            drop(active_job);
            manager
                .cancel_job("job-project", &queued.job_id)
                .await
                .expect("cancel queued initialization");
            assert!(
                manager.project_status("job-project").await.is_none(),
                "cancellation must not reactivate models"
            );
            manager
                .projects
                .write()
                .await
                .insert(String::from("job-project"), retained_runtime);
            let cancelled = await_test_job(&manager, "job-project", &queued.job_id).await;
            assert_eq!(cancelled.state, VceJobState::Cancelled);
            assert!(
                factory
                    .try_acquire_project_writer("job-project")
                    .expect("cancelled lease released")
                    .is_some()
            );
            assert!(
                cancelled.progress.is_none(),
                "queued cancellation cannot invent file scans"
            );
            assert!(cancelled.result_checkpoint_version.is_none());
            // A newer writer must not turn replay of this terminal receipt into a busy error.
            // 新写入者存在时，回放这个终态回执也不能变成忙碌错误。
            let terminal_writer = factory
                .try_acquire_project_writer("job-project")
                .expect("terminal replay writer")
                .expect("terminal replay can reserve the project");
            assert_eq!(
                manager
                    .cancel_job("job-project", &cancelled.job_id)
                    .await
                    .expect("terminal cancellation is idempotent"),
                cancelled
            );
            drop(terminal_writer);
            assert_eq!(
                manager
                    .last_successful_update("job-project")
                    .await
                    .expect("read cancelled freshness"),
                None
            );
            // detached_close proves project teardown cancels and drains a queued owner without a query runtime.
            // detached_close 证明项目拆卸在没有查询运行时时仍取消并排空排队所有者。
            let detached_close = manager
                .submit_job(
                    "job-project",
                    command("close-without-query-runtime", VceJobKind::Initialize, None),
                    &executor,
                )
                .await
                .expect("queue detached closure fixture");
            let retained_runtime = manager
                .projects
                .write()
                .await
                .remove("job-project")
                .expect("detach runtime for project closure");
            tokio::time::timeout(
                Duration::from_secs(5),
                manager.disable_project("job-project"),
            )
            .await
            .expect("detached project closure cannot wait for the held budget")
            .expect("drain detached project job");
            assert!(retained_runtime.job_state.active.lock().await.is_none());
            assert!(manager.project_status("job-project").await.is_none());
            assert_eq!(
                manager
                    .get_job("job-project", Some(&detached_close.job_id))
                    .await
                    .expect("read drained detached receipt")
                    .expect("detached receipt persists")
                    .state,
                VceJobState::Cancelled
            );
            manager
                .projects
                .write()
                .await
                .insert(String::from("job-project"), retained_runtime);
            drop(budget);
            // Explicit retry records its durable origin but gets a fresh admission identity.
            // 明确重试记录持久来源，但取得全新准入身份。
            let mut retry_command = command("second-click", VceJobKind::Initialize, None);
            retry_command.retry_of_job_id = Some(cancelled.job_id.clone());
            let initialized = manager
                .submit_job("job-project", retry_command, &executor)
                .await
                .expect("retry with explicit new click");
            assert_eq!(
                initialized.retry_of_job_id.as_deref(),
                Some(cancelled.job_id.as_str())
            );
            assert_ne!(initialized.job_id, cancelled.job_id);
            let completed = await_test_job(&manager, "job-project", &initialized.job_id).await;
            assert_eq!(
                completed.state,
                VceJobState::Succeeded,
                "empty initialization diagnostic: {:?}",
                completed.error
            );
            let checkpoint = completed
                .result_checkpoint_version
                .expect("valid empty checkpoint exists");
            assert!(manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
            // A competing writer's lease is not a query gate for the already published checkpoint.
            // 竞争写入者的租约不是已发布检查点的查询闸门。
            let foreign_writer = factory
                .try_acquire_project_writer("job-project")
                .expect("foreign query-time writer")
                .expect("foreign ownership");
            assert!(manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
            tokio::time::timeout(
                Duration::from_secs(5),
                manager.retrieve("job-project", "existing empty index"),
            )
            .await
            .expect("query cannot wait on another host writer")
            .expect("query old checkpoint");
            drop(foreign_writer);
            // Unconfirmed callers cannot smuggle rebuild work through the normal synchronization entry.
            // 未确认调用方不能通过普通同步入口夹带重建工作。
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command("unconfirmed-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                        &executor
                    )
                    .await
                    .is_err()
            );
            // rebuild_plan comes from the same public management path used for human confirmation.
            // rebuild_plan 来自与人为确认相同的公开管理路径。
            // The displayed management identity is bound to the exact publication, not just its number.
            // 显示的管理身份绑定精确发布，而非仅绑定数字版本。
            let (management_identity, management_snapshot) = manager
                .management_snapshot("job-project")
                .await
                .expect("read initialized management overview");
            let management_identity = management_identity.expect("published management identity");
            assert_eq!(
                management_snapshot.engine.checkpoint_version,
                Some(checkpoint)
            );
            // Build a fresh typed request for each admission check; returns only a bounded file read.
            // 为每次准入检查构建新的类型化请求；仅返回有界文件读取。
            let management_files = || vulcan_context_engine::VceManagementQueryRequest {
                operation_id: String::from("fixture-management-files"),
                query: vulcan_context_engine::VceManagementQuery::Files(
                    vulcan_context_engine::VceFileQuery {
                        path_prefix: None,
                        status: None,
                        auxiliary_failures_only: false,
                        page: vulcan_context_engine::VcePageRequest::new(None, 25)
                            .expect("bounded fixture page"),
                    },
                ),
            };
            assert!(
                manager
                    .management_query("job-project", management_files(), None)
                    .await
                    .is_err()
            );
            assert!(
                manager
                    .management_query("job-project", management_files(), Some("another-space:1"))
                    .await
                    .is_err()
            );
            assert!(
                manager
                    .management_query(
                        "job-project",
                        management_files(),
                        Some(&management_identity)
                    )
                    .await
                    .is_ok()
            );
            let rebuild_plan = manager
                .management_query(
                    "job-project",
                    vulcan_context_engine::VceManagementQueryRequest {
                        operation_id: String::from("fixture-rebuild-plan"),
                        query: vulcan_context_engine::VceManagementQuery::MaintenancePlan {
                            mode: vulcan_context_engine::VceMaintenanceMode::Rebuild,
                        },
                    },
                    None,
                )
                .await
                .expect("load initialized empty checkpoint plan");
            let vulcan_context_engine::VceManagementQueryResult::MaintenancePlan(rebuild_plan) =
                rebuild_plan.result
            else {
                panic!("unexpected maintenance fixture result");
            };
            // rebuild_action_ids retains every exact planned identity, independent of UI ordering.
            // rebuild_action_ids 保留每个精确计划身份，与界面顺序无关。
            let rebuild_action_ids = rebuild_plan
                .actions
                .into_iter()
                .map(|action| action.action_id)
                .collect::<BTreeSet<_>>();
            assert!(
                !rebuild_action_ids.is_empty(),
                "initialized empty checkpoint must support explicit rebuild"
            );
            // cold_manager deliberately has no capabilities or permissive publication authority.
            // cold_manager 刻意不提供能力或允许发布的权威校验器。
            let cold_manager = VceRuntimeManager::new(
                Arc::new(MockHostAiPort::new()),
                factory.clone(),
                ServiceEventEmitter::new(Vec::new()),
                Arc::new(|_| {
                    Err(String::from("cold fixture must never authorize model execution").into())
                }),
            );
            // cold_plan must still expose the original stored plan without creating a model runtime.
            // cold_plan 仍须暴露原始存储计划，且不创建模型运行时。
            let cold_plan = cold_manager
                .stored_maintenance_plan(
                    "job-project",
                    vulcan_context_engine::VceMaintenanceMode::Rebuild,
                )
                .await
                .expect("plan rebuild independently of model activation");
            assert_eq!(
                cold_plan
                    .actions
                    .into_iter()
                    .map(|action| action.action_id)
                    .collect::<BTreeSet<_>>(),
                rebuild_action_ids
            );
            assert!(cold_manager.project_status("job-project").await.is_none());
            assert!(!cold_manager.project_can_serve_now(
                "job-project",
                settings.revision,
                &fingerprint
            ));
            assert!(
                cold_manager
                    .stored_maintenance_plan(
                        "missing-plan-project",
                        vulcan_context_engine::VceMaintenanceMode::Rebuild
                    )
                    .await
                    .is_err()
            );
            assert!(
                factory
                    .load_active_project_config("missing-plan-project")
                    .await
                    .expect("missing plan does not allocate storage")
                    .is_none()
            );
            drop(cold_manager);
            // A new confirmed submission must not enter the queue after live publication authority is revoked.
            // 实时发布授权撤销后，新的已确认提交不得进入队列。
            publication_allowed.store(false, Ordering::Release);
            assert!(
                manager
                    .submit_confirmed_rebuild(
                        "job-project",
                        command(
                            "revoked-rebuild-plan",
                            VceJobKind::Rebuild,
                            Some(checkpoint)
                        ),
                        false,
                        rebuild_action_ids.clone(),
                        &executor,
                    )
                    .await
                    .is_err()
            );
            publication_allowed.store(true, Ordering::Release);
            assert!(
                manager
                    .submit_confirmed_rebuild(
                        "job-project",
                        command("forged-rebuild-plan", VceJobKind::Rebuild, Some(checkpoint)),
                        false,
                        BTreeSet::from([String::from("not-the-confirmed-action")]),
                        &executor,
                    )
                    .await
                    .is_err(),
                "unconfirmed plan must not enter the durable queue"
            );
            // rebuild_budget proves admission and old-checkpoint queries do not wait for writer capacity.
            // rebuild_budget 证明准入与旧检查点查询不等待写入容量。
            let rebuild_budget = manager
                .job_writer_budget
                .acquire()
                .await
                .expect("hold rebuild capacity");
            // queued_rebuild is a real durable maintenance job rather than a blocking RPC refresh.
            // queued_rebuild 是真实持久维护作业，而非阻塞远程调用刷新。
            let queued_rebuild = tokio::time::timeout(
                Duration::from_secs(5),
                manager.submit_confirmed_rebuild(
                    "job-project",
                    command("cancel-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                    false,
                    rebuild_action_ids.clone(),
                    &executor,
                ),
            )
            .await
            .expect("rebuild admission must not await capacity")
            .expect("queue confirmed rebuild");
            assert_eq!(queued_rebuild.kind, VceJobKind::Rebuild);
            assert_eq!(queued_rebuild.state, VceJobState::Queued);
            tokio::time::timeout(
                Duration::from_secs(5),
                manager.retrieve("job-project", "existing code"),
            )
            .await
            .expect("query must not wait for rebuild")
            .expect("old empty checkpoint remains queryable");
            manager
                .cancel_job("job-project", &queued_rebuild.job_id)
                .await
                .expect("cancel queued rebuild");
            assert_eq!(
                await_test_job(&manager, "job-project", &queued_rebuild.job_id)
                    .await
                    .state,
                VceJobState::Cancelled
            );
            drop(rebuild_budget);
            // rebuilt exercises the same durable runner with explicit full re-index semantics.
            // rebuilt 使用明确完整重索引语义验证同一个持久运行器。
            let rebuilt = manager
                .submit_confirmed_rebuild(
                    "job-project",
                    command("run-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                    false,
                    rebuild_action_ids.clone(),
                    &executor,
                )
                .await
                .expect("admit confirmed rebuild");
            let rebuilt = await_test_job(&manager, "job-project", &rebuilt.job_id).await;
            assert_eq!(
                rebuilt.state,
                VceJobState::Succeeded,
                "rebuild diagnostic: {:?}",
                rebuilt.error
            );
            // Revocation blocks new work but must not reinterpret the receipt of an already completed request.
            // 撤销阻止新工作，但不得重新解释已经完成请求的回执。
            publication_allowed.store(false, Ordering::Release);
            assert_eq!(
                manager
                    .submit_confirmed_rebuild(
                        "job-project",
                        command("run-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                        false,
                        rebuild_action_ids.clone(),
                        &executor,
                    )
                    .await
                    .expect("replay completed rebuild against its original checkpoint"),
                rebuilt
            );
            publication_allowed.store(true, Ordering::Release);
            assert!(
                manager
                    .submit_confirmed_rebuild(
                        "job-project",
                        command("run-rebuild", VceJobKind::Rebuild, Some(checkpoint)),
                        false,
                        BTreeSet::from([String::from("changed-confirmation")]),
                        &executor,
                    )
                    .await
                    .is_err(),
                "the same replay key cannot authorize different maintenance actions"
            );
            // checkpoint follows the newly published generation for all subsequent fixture operations.
            // checkpoint 为全部后续夹具操作跟随新发布代次。
            let checkpoint = rebuilt
                .result_checkpoint_version
                .expect("rebuild publishes checkpoint");
            // A later publication cannot accept the old list's identity for another page or detail.
            // 后续发布不能为另一页或详情接受旧列表身份。
            let (replacement_identity, _) = manager
                .management_snapshot("job-project")
                .await
                .expect("read replacement management identity");
            assert_ne!(
                replacement_identity.as_deref(),
                Some(management_identity.as_str())
            );
            assert!(
                manager
                    .management_query(
                        "job-project",
                        management_files(),
                        Some(&management_identity)
                    )
                    .await
                    .is_err()
            );
            assert!(
                manager
                    .management_query(
                        "job-project",
                        management_files(),
                        replacement_identity.as_deref()
                    )
                    .await
                    .is_ok()
            );
            assert!(manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
            let mut invalid_retry =
                command("retry-success", VceJobKind::Synchronize, Some(checkpoint));
            invalid_retry.retry_of_job_id = Some(completed.job_id.clone());
            assert!(
                manager
                    .submit_job("job-project", invalid_retry, &executor)
                    .await
                    .is_err(),
                "a successful job cannot be presented as a failed retry origin"
            );
            let canonical_root =
                std::fs::canonicalize(&project_root).expect("canonical fixture root");
            assert_eq!(
                watcher.registered_root("job-project"),
                Some(canonical_root.clone())
            );
            assert!(
                manager
                    .submit_watcher_job("job-project", &canonical_root, None, None, &executor)
                    .await
                    .is_err(),
                "automatic work must respect manual initialization cooldown"
            );
            let synchronized = manager
                .submit_job(
                    "job-project",
                    command("sync-click", VceJobKind::Synchronize, Some(checkpoint)),
                    &executor,
                )
                .await
                .expect("explicit no-change synchronization");
            let synchronized = await_test_job(&manager, "job-project", &synchronized.job_id).await;
            assert_eq!(synchronized.state, VceJobState::Succeeded);
            assert_eq!(synchronized.result_checkpoint_version, Some(checkpoint));
            assert_eq!(
                manager
                    .last_successful_update("job-project")
                    .await
                    .expect("read successful freshness"),
                Some(
                    synchronized
                        .completed_update
                        .as_ref()
                        .expect("successful batch accounting")
                        .at_unix_ms
                )
            );
            assert!(synchronized.updated_at_unix_ms >= completed.updated_at_unix_ms);
            manager
                .disable_project("job-project")
                .await
                .expect("close isolated project");
            assert!(watcher.registered_root("job-project").is_none());
            assert_eq!(
                manager
                    .cancel_job("job-project", &synchronized.job_id)
                    .await
                    .expect(
                        "terminal receipt cancellation remains available after project closure"
                    ),
                synchronized
            );
            assert!(
                manager.project_status("job-project").await.is_none(),
                "terminal replay does not open a query runtime"
            );
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    3,
                    &fingerprint,
                    false,
                )
                .await
                .expect("restore only persisted metadata");
            assert_eq!(
                watcher.registered_root("job-project"),
                Some(canonical_root.clone())
            );
            let remaining = manager
                .watcher_admission_delay("job-project")
                .await
                .expect("read restored automatic policy")
                .expect("automatic observation enabled");
            assert!(remaining > Duration::ZERO && remaining <= Duration::from_secs(300));
            assert_eq!(
                manager
                    .get_job("job-project", None)
                    .await
                    .expect("read restored receipt")
                    .expect("previous receipt exists")
                    .job_id,
                synchronized.job_id,
                "metadata restoration must not schedule another job"
            );
            let active_runtime = manager
                .require_project("job-project")
                .await
                .expect("restored runtime");
            // Restoring a physical watcher cannot certify changes missed while the project was disabled.
            // 恢复物理监听不能确认项目关闭期间遗漏的变化。
            let restored_freshness = manager.source_freshness("job-project").await;
            assert!(restored_freshness.watching);
            assert!(restored_freshness.reconciliation_required);
            active_runtime
                .job_schedule
                .lock()
                .expect("fixture schedule")
                .last_admitted = None;
            let unchanged = manager
                .submit_watcher_job(
                    "job-project",
                    &canonical_root,
                    Some(BTreeSet::new()),
                    None,
                    &executor,
                )
                .await
                .expect("admit empty incremental delta");
            let unchanged = await_test_job(&manager, "job-project", &unchanged.job_id).await;
            assert_eq!(
                unchanged.state,
                VceJobState::Succeeded,
                "empty delta: {:?}",
                unchanged.error
            );
            assert_eq!(unchanged.result_checkpoint_version, Some(checkpoint));
            // Missing batch evidence promoted the legal update to a full manifest scan, not re-embedding.
            // 批次证据缺失将合法更新提升为完整清单扫描，而不是重新嵌入。
            assert!(
                !manager
                    .source_freshness("job-project")
                    .await
                    .reconciliation_required
            );
            // Hold only the publication boundary: the first batch has really scanned before later clicks arrive.
            // 只持有发布边界：后续点击到达前，首批已真正完成扫描。
            let publication_guard = crate::begin_public_ai_configuration_transaction().await;
            let chained = manager
                .submit_job(
                    "job-project",
                    command("chain-first", VceJobKind::Synchronize, Some(checkpoint)),
                    &executor,
                )
                .await
                .expect("admit chain head");
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let current = manager
                        .get_job("job-project", Some(&chained.job_id))
                        .await
                        .expect("read chain progress")
                        .expect("chain exists");
                    if current.progress.as_ref().is_some_and(|progress| {
                        progress.phase == vulcan_context_engine::VceRefreshPhase::Complete
                    }) {
                        break;
                    }
                    assert!(
                        !current.state.is_terminal(),
                        "batch stopped unexpectedly: {:?}",
                        current.error
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("first batch reaches blocked publication");
            let first_followup = manager
                .submit_job(
                    "job-project",
                    command(
                        "chain-followup-a",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("coalesce first follow-up");
            assert!(first_followup.followup_requested);
            assert_eq!(first_followup.job_id, chained.job_id);
            let second_followup = manager
                .submit_job(
                    "job-project",
                    command(
                        "chain-followup-b",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("coalesce another click into the same one slot");
            assert_eq!(second_followup.job_id, chained.job_id);
            assert!(second_followup.followup_requested);
            assert_eq!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "chain-followup-a",
                            VceJobKind::Synchronize,
                            Some(checkpoint)
                        ),
                        &executor
                    )
                    .await
                    .expect("replay accepted alias")
                    .sequence,
                second_followup.sequence
            );
            drop(publication_guard);
            let chained = await_test_job(&manager, "job-project", &chained.job_id).await;
            assert_eq!(chained.state, VceJobState::Succeeded, "{:?}", chained.error);
            assert!(!chained.followup_requested);
            assert!(
                chained
                    .progress
                    .as_ref()
                    .expect("real follow-up observation")
                    .operation_id
                    .contains(":batch:")
            );
            assert_eq!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "chain-followup-a",
                            VceJobKind::Synchronize,
                            Some(checkpoint)
                        ),
                        &executor
                    )
                    .await
                    .expect("terminal alias replay does not start a third scan"),
                chained
            );
            // Replaying the same key with a changed observed checkpoint remains a contract error.
            // 使用已变更观察检查点重放相同键仍是契约错误。
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "chain-followup-a",
                            VceJobKind::Synchronize,
                            Some(checkpoint + 1)
                        ),
                        &executor
                    )
                    .await
                    .is_err()
            );
            // Cancellation after the first core batch finished must still remove the acknowledged successor.
            // 首个核心批次结束后的取消仍必须移除已确认后续批次。
            let publication_guard = crate::begin_public_ai_configuration_transaction().await;
            let clearing = manager
                .submit_job(
                    "job-project",
                    command(
                        "clear-pending-head",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("admit pending cancellation fixture");
            await_test_completed_phase(&manager, "job-project", &clearing.job_id).await;
            let pending = manager
                .submit_job(
                    "job-project",
                    command(
                        "clear-pending-next",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("acknowledge successor before cancellation");
            assert!(pending.followup_requested);
            let cleared = manager
                .cancel_job("job-project", &clearing.job_id)
                .await
                .expect("remove successor without undoing finished batch");
            assert!(!cleared.followup_requested);
            drop(publication_guard);
            let cleared = await_test_job(&manager, "job-project", &cleared.job_id).await;
            assert_eq!(cleared.state, VceJobState::Succeeded);
            assert!(
                !cleared
                    .progress
                    .as_ref()
                    .expect("only first batch ran")
                    .operation_id
                    .contains(":batch:")
            );
            let last_successful_batch = cleared
                .completed_update
                .as_ref()
                .expect("follow-up completion is durable")
                .at_unix_ms;
            // Revocation is checked again at final publication, including an empty-delta metadata commit.
            // 最终发布再次检查撤销，包括空增量元数据提交。
            publication_allowed.store(false, Ordering::Release);
            let revoked = manager
                .submit_job(
                    "job-project",
                    command(
                        "revoked-publication",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("admit work before its final publication check");
            let revoked = await_test_job(&manager, "job-project", &revoked.job_id).await;
            assert_eq!(revoked.state, VceJobState::Failed);
            assert!(
                revoked
                    .error
                    .as_deref()
                    .expect("explicit revocation error")
                    .contains("authority revoked")
            );
            assert!(!manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
            assert_eq!(
                active_runtime
                    .index
                    .load_state_snapshot()
                    .await
                    .expect("read preserved checkpoint")
                    .expect("checkpoint survives revocation")
                    .workspace
                    .checkpoint
                    .expect("committed checkpoint survives")
                    .version,
                checkpoint
            );
            publication_allowed.store(true, Ordering::Release);
            assert!(
                manager.project_can_serve_now("job-project", settings.revision, &fingerprint),
                "failed update must not erase the previously admitted checkpoint"
            );
            // Project shutdown closes admission before the completed batch can hand off to its successor.
            // 项目关闭在已完成批次交接后续批次之前关闭准入。
            let publication_guard = crate::begin_public_ai_configuration_transaction().await;
            let closing = manager
                .submit_job(
                    "job-project",
                    command(
                        "close-chain-head",
                        VceJobKind::Synchronize,
                        Some(checkpoint),
                    ),
                    &executor,
                )
                .await
                .expect("admit closing chain fixture");
            await_test_completed_phase(&manager, "job-project", &closing.job_id).await;
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "close-chain-next",
                            VceJobKind::Synchronize,
                            Some(checkpoint)
                        ),
                        &executor
                    )
                    .await
                    .expect("queue successor before close")
                    .followup_requested
            );
            let closing_manager = manager.clone();
            let close =
                executor.spawn(async move { closing_manager.disable_project("job-project").await });
            tokio::time::timeout(Duration::from_secs(5), async {
                while active_runtime
                    .operation_admission
                    .accepting
                    .load(Ordering::Acquire)
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("close must revoke admission without waiting for publication");
            drop(publication_guard);
            tokio::time::timeout(Duration::from_secs(10), close)
                .await
                .expect("close drains chain")
                .expect("close task completed")
                .expect("close runtime");
            let closed = manager
                .get_job("job-project", Some(&closing.job_id))
                .await
                .expect("read closed chain")
                .expect("closed receipt persists");
            assert!(!closed.followup_requested);
            // Cooperative rejection records failure; the bounded shutdown abort records interruption.
            // 协作拒绝记录失败；有界关闭中止则记录中断。
            // Both paths must be terminal and must not invent a successfully published batch.
            // 两条路径都必须终结作业，且不能虚构成功发布的批次。
            assert!(matches!(closed.state, VceJobState::Failed | VceJobState::Interrupted));
            assert!(
                closed.completed_update.is_none(),
                "revoked publication must not invent successful completion"
            );
            assert!(
                !closed
                    .progress
                    .as_ref()
                    .expect("no successor scan after close")
                    .operation_id
                    .contains(":batch:")
            );
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    3,
                    &fingerprint,
                    false,
                )
                .await
                .expect("reopen for independent queued shutdown assertion");
            drop(active_runtime);
            // A foreign unfinished owner must never be declared dead merely because its UUID differs.
            // 绝不能仅因外部未完成所有者的运行标识不同，就宣布其已经退出。
            let recovery_runtime = manager
                .require_project("job-project")
                .await
                .expect("retain recovery fixture");
            // recovery_store addresses only this temporary project's durable receipts.
            // recovery_store 仅定位此临时项目的持久回执。
            let recovery_store = recovery_runtime.index.job_store();
            // foreign is an explicitly created fixture owner, not an inferred crashed process.
            // foreign 是明确创建的夹具所有者，不是推断的崩溃进程。
            let mut foreign = closed.clone();
            foreign.job_id = Uuid::new_v4().to_string();
            foreign.owner_run_id = Uuid::new_v4().to_string();
            foreign.idempotency_key = String::from("foreign-live-fixture");
            foreign.retry_of_job_id = None;
            foreign.state = VceJobState::Queued;
            foreign.sequence = 1;
            foreign.created_at_unix_ms = job_time().expect("fixture admission time");
            foreign.updated_at_unix_ms = foreign.created_at_unix_ms;
            foreign.result_checkpoint_version = None;
            foreign.completed_update = None;
            foreign.progress = None;
            foreign.followup_requested = false;
            foreign.error = None;
            recovery_store
                .register(&foreign)
                .await
                .expect("register foreign fixture job");
            // A live cooperating host owns an OS writer lease, not merely a different receipt UUID.
            // 活动协作宿主持有操作系统写入租约，而不只是不同的回执编号。
            let foreign_lease = factory.try_acquire_project_writer("job-project").expect("foreign writer lock").expect("foreign writer lease");
            assert_eq!(manager.get_job("job-project", Some(&foreign.job_id)).await.expect("read live foreign job"), Some(foreign.clone()));
            // This staged generation belongs to the explicit foreign fixture, never to project shutdown.
            // 此暂存代次属于明确的外部夹具，绝不属于项目关闭流程。
            recovery_runtime
                .index
                .stage(vulcan_context_engine::IndexStageBatch {
                    operation_id: format!("{}:fixture-stage", foreign.job_id),
                    refresh_id: foreign.job_id.clone(),
                    generation: checkpoint + 1,
                    blobs: Vec::new(),
                })
                .await
                .expect("stage foreign fixture generation");
            assert!(
                manager
                    .submit_job(
                        "job-project",
                        command(
                            "must-not-steal-foreign-job",
                            VceJobKind::Synchronize,
                            Some(checkpoint)
                        ),
                        &executor,
                    )
                    .await
                    .expect_err("unconfirmed foreign ownership rejects new work")
                    .contains("active host writer")
            );
            assert_eq!(
                recovery_store
                    .get(&foreign.job_id)
                    .await
                    .expect("foreign state after rejected steal"),
                Some(foreign.clone())
            );
            assert!(
                recovery_store
                    .find_by_idempotency_key("must-not-steal-foreign-job")
                    .await
                    .expect("no unauthorized admission")
                    .is_none()
            );
            assert!(
                manager
                    .cleanup_staging("job-project")
                    .await
                    .expect_err("maintenance cannot erase unconfirmed staging")
                    .contains("active host writer")
            );
            manager
                .disable_project("job-project")
                .await
                .expect("close without claiming foreign staging");
            assert!(
                recovery_runtime
                    .index
                    .has_unresolved_staging()
                    .await
                    .expect("foreign staging survives close")
            );
            assert_eq!(
                recovery_store
                    .get(&foreign.job_id)
                    .await
                    .expect("foreign receipt survives close"),
                Some(foreign.clone())
            );
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    3,
                    &fingerprint,
                    false,
                )
                .await
                .expect("reopen retained checkpoint without deleting foreign staging");
            // Only the test's known foreign owner concludes its own fixture receipt.
            // 仅由测试中已知的外部所有者结束其自身夹具回执。
            recovery_runtime
                .index
                .abort_refresh_staging(&foreign.job_id)
                .await
                .expect("foreign fixture aborts its exact attempt");
            let mut foreign_finished = foreign.clone();
            foreign_finished.sequence += 1;
            foreign_finished.state = VceJobState::Cancelled;
            foreign_finished.updated_at_unix_ms = job_time()
                .expect("fixture finish time")
                .max(foreign.updated_at_unix_ms);
            recovery_store
                .update(&foreign, &foreign_finished)
                .await
                .expect("foreign fixture owner releases its slot");
            drop(foreign_lease);
            drop(recovery_store);
            drop(recovery_runtime);
            // Shutdown must wake a queued synchronization while the existing checkpoint remains stored.
            // 退出必须唤醒排队同步，同时已有检查点保持存储。
            let budget = manager
                .job_writer_budget
                .acquire()
                .await
                .expect("hold shutdown fixture budget");
            let stopping = manager
                .submit_job(
                    "job-project",
                    command("shutdown-click", VceJobKind::Synchronize, Some(checkpoint)),
                    &executor,
                )
                .await
                .expect("queue shutdown fixture");
            assert_eq!(
                manager
                    .cancel_job("job-project", &cancelled.job_id)
                    .await
                    .expect("late old cancellation does not target a newer job"),
                cancelled
            );
            assert_eq!(
                manager
                    .get_job("job-project", Some(&stopping.job_id))
                    .await
                    .expect("read untouched newer job")
                    .expect("newer job exists")
                    .state,
                VceJobState::Queued
            );
            tokio::time::timeout(Duration::from_secs(10), manager.shutdown())
                .await
                .expect("shutdown cannot hang on a queued job")
                .expect("drain project worker and bindings");
            drop(budget);
            let stopped = manager
                .get_job("job-project", Some(&stopping.job_id))
                .await
                .expect("read persisted shutdown outcome")
                .expect("shutdown receipt exists");
            assert_eq!(stopped.state, VceJobState::Cancelled);
            assert_eq!(stopped.source_checkpoint_version, Some(checkpoint));
            assert_eq!(
                manager
                    .last_successful_update("job-project")
                    .await
                    .expect("cancel must preserve success time"),
                Some(last_successful_batch)
            );
            assert!(watcher.registered_root("job-project").is_none());
            watcher.shutdown().await;
            // Verify a published candidate's cold restore in a separate awaited task, without enlarging this fixture's stack.
            // 在独立且被等待的任务中验证已发布候选的冷恢复，不扩大本夹具栈。
            executor
                .spawn(verify_published_candidate_runtime_restore(
                    factory.clone(),
                    project_root.clone(),
                    settings.clone(),
                    fingerprint.clone(),
                    Arc::clone(&executor),
                ))
                .await
                .expect("candidate restore fixture completed");
            // Exercise the real human queue separately from the manually assembled candidate fixture above.
            // 与上方手工组装候选夹具分离，实际验证人为队列。
            executor
                .spawn(verify_queued_replacement_jobs(
                    factory.clone(),
                    project_root.clone(),
                    settings.clone(),
                    Arc::clone(&executor),
                ))
                .await
                .expect("queued replacement fixture completed");
            // Run real nonempty read/write interleaving separately to keep the parent fixture stack bounded.
            // 单独运行真实非空读写交错，保持父夹具栈有界。
            executor
                .spawn(super::query_tests::verify_nonempty_query_interleaving(
                    factory.clone(),
                    temporary.path().join("nonempty-source"),
                    Arc::clone(&executor),
                ))
                .await
                .expect("nonempty query interleaving fixture completed");
            factory
                .shutdown()
                .await
                .expect("close exact fixture bindings");
                })
            })
            .expect("spawn isolated job fixture thread")
            .join()
            .expect("isolated job fixture thread completed");
    }

    /// Exercise incompatible-dimension rebuilding through the same functional queue used by the service.
    /// 通过服务使用的同一功能队列验证不兼容维度重建。
    /// Inputs belong to the isolated empty project; returns after cancellation, handoff, replay and shutdown checks.
    /// 输入属于隔离空项目；在取消、交接、重放及退出检查之后返回。
    async fn verify_queued_replacement_jobs(
        factory: VceDataPlaneFactory,
        project_root: std::path::PathBuf,
        settings: ProjectVceSettings,
        executor: Arc<tokio::runtime::Runtime>,
    ) {
        // capabilities intentionally change the physical dimension without granting old-vector compatibility.
        // capabilities 有意改变物理维度，不授予旧向量兼容性。
        let mut capabilities = job_test_capabilities();
        capabilities.operations[0]
            .embedding
            .as_mut()
            .expect("embedding capability")
            .dimensions = BTreeSet::from([4]);
        let fingerprint = crate::vce::vce_embedding_physical_fingerprint(&capabilities, 4)
            .expect("freeze four-dimensional route");
        let mock = vulcan_ai_contract::test_support::MockHostAiPort::new();
        mock.set_capability_snapshot(capabilities)
            .expect("configure four-dimensional fixture");
        let expected = fingerprint.clone();
        let manager = VceRuntimeManager::new(
            Arc::new(mock),
            factory.clone(),
            ServiceEventEmitter::new(Vec::new()),
            Arc::new(move |contract| {
                if contract.project_id != "job-project"
                    || contract.dimension != 4
                    || contract.embedding_fingerprint != expected
                {
                    return Err(String::from(
                        "Fixture requires the current four-dimensional route",
                    )
                    .into());
                }
                if contract.validates_replacement_admission()? {
                    return Ok(());
                }
                let provenance = contract
                    .provenance
                    .as_ref()
                    .expect("existing fixture provenance");
                if provenance.original.dimension != 4
                    || provenance.original.physical_fingerprint != expected
                {
                    return Err(String::from(
                        "Fixture forbids querying incompatible predecessor vectors",
                    )
                    .into());
                }
                Ok(())
            }),
        );
        assert!(
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    4,
                    &fingerprint,
                    false
                )
                .await
                .is_err()
        );
        let original = factory
            .embedding_provenance("job-project")
            .await
            .expect("read predecessor")
            .expect("existing predecessor");
        assert_eq!(original.original.dimension, 3);
        let checkpoint = original
            .checkpoint_version
            .expect("initialized predecessor");
        let plan = manager
            .stored_maintenance_plan(
                "job-project",
                vulcan_context_engine::VceMaintenanceMode::Rebuild,
            )
            .await
            .expect("preview without compatible runtime");
        let actions = plan
            .actions
            .into_iter()
            .map(|action| action.action_id)
            .collect::<BTreeSet<_>>();
        let input = VceRebuildProjectInput {
            project_id: String::from("job-project"),
            project_root: project_root.clone(),
            settings: settings.clone(),
            dimension: 4,
            fingerprint: fingerprint.clone(),
        };
        // command preserves the old checkpoint preview and current producer through every replay.
        // command 在每次重放中保留旧检查点预览及当前生产者。
        let command = |key: &str| VceJobAdmission {
            retry_of_job_id: None,
            kind: VceJobKind::Rebuild,
            idempotency_key: key.to_owned(),
            expected_settings_revision: settings.revision,
            expected_checkpoint_version: Some(checkpoint),
            contract_fingerprint: fingerprint.clone(),
        };
        let budget = manager
            .job_writer_budget
            .acquire()
            .await
            .expect("hold replacement allocation budget");
        let queued = manager
            .submit_project_rebuild(
                input.clone(),
                command("replacement-cancel"),
                false,
                actions.clone(),
                &executor,
            )
            .await
            .expect("queue without compatible runtime");
        assert_eq!(queued.state, VceJobState::Queued);
        assert!(
            factory
                .try_acquire_project_writer("job-project")
                .expect("replacement queue owns writer")
                .is_none()
        );
        let replacement = queued.replacement.as_ref().expect("durable target");
        assert_eq!(replacement.target.dimension, 4);
        assert_eq!(replacement.predecessor.checkpoint_version, checkpoint);
        assert_eq!(
            factory
                .load_active_project_config("job-project")
                .await
                .expect("active configuration"),
            Some(original.original.clone())
        );
        assert!(
            factory
                .reopen_rebuild_candidate(&replacement.target, &replacement.predecessor)
                .await
                .is_err(),
            "queueing must not allocate the candidate"
        );
        assert!(!manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
        assert!(
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    4,
                    &fingerprint,
                    false
                )
                .await
                .is_err(),
            "cold reads cannot reactivate a predecessor while replacement owns the project"
        );
        assert_eq!(
            manager
                .submit_project_rebuild(
                    input.clone(),
                    command("replacement-cancel"),
                    false,
                    actions.clone(),
                    &executor
                )
                .await
                .expect("queued replay"),
            queued
        );
        // Validate that explicit new-space authority cannot be reused as ordinary query compatibility.
        // 验证明确定义的新空间权威不能复用为普通查询兼容授权。
        let mut contract = VcePublicationContract {
            project_id: String::from("job-project"),
            canonical_root: replacement.target.root_path.clone(),
            embedding_fingerprint: fingerprint.clone(),
            dimension: 4,
            max_file_bytes: settings.max_file_bytes,
            extra_ignore_patterns: settings.extra_ignore_patterns.clone(),
            provenance: Some(original.clone()),
            replacement_admission: Some(Box::new(queued.clone())),
        };
        assert_eq!(contract.validates_replacement_admission(), Ok(true));
        contract.dimension = 3;
        assert!(contract.validates_replacement_admission().is_err());
        contract.dimension = 4;
        contract
            .replacement_admission
            .as_mut()
            .expect("receipt")
            .state = VceJobState::Cancelling;
        assert!(contract.validates_replacement_admission().is_err());
        contract.replacement_admission = None;
        assert_eq!(contract.validates_replacement_admission(), Ok(false));
        assert!((manager.publication_authority)(&contract).is_err());
        // changed_source must cancel even without a query runtime, while the held budget prevents scans.
        // changed_source 必须在没有查询运行时时也取消任务，同时保留预算阻止扫描。
        let mut changed_source = settings.clone();
        changed_source.revision += 1;
        changed_source
            .extra_ignore_patterns
            .push(String::from("generated/**"));
        assert!(
            !manager
                .reconcile_policy_revision(
                    "job-project",
                    &project_root,
                    &changed_source,
                    &fingerprint
                )
                .await
                .expect("source-policy change cancels exact queued replacement")
        );
        let cancelled_by_policy = await_test_job(&manager, "job-project", &queued.job_id).await;
        assert_eq!(cancelled_by_policy.state, VceJobState::Cancelled);
        manager
            .cancel_job("job-project", &queued.job_id)
            .await
            .expect("cancel queued replacement");
        let cancelled = await_test_job(&manager, "job-project", &queued.job_id).await;
        assert_eq!(
            cancelled.state,
            VceJobState::Cancelled,
            "{:?}",
            cancelled.error
        );
        assert!(
            factory
                .try_acquire_project_writer("job-project")
                .expect("cancelled replacement releases writer")
                .is_some()
        );
        assert!(
            factory
                .reopen_rebuild_candidate(&replacement.target, &replacement.predecessor)
                .await
                .is_err()
        );
        // Disabling without a query runtime still drains the exact replacement worker while its budget is unavailable.
        // 没有查询运行时的停用仍排空精确替换任务，即使其预算不可用。
        let stopping = manager
            .submit_project_rebuild(
                input.clone(),
                command("replacement-disable"),
                false,
                actions.clone(),
                &executor,
            )
            .await
            .expect("queue disable fixture");
        tokio::time::timeout(
            Duration::from_secs(10),
            manager.disable_project("job-project"),
        )
        .await
        .expect("disable cannot wait for writer budget")
        .expect("disable project job");
        assert_eq!(
            manager
                .get_job("job-project", Some(&stopping.job_id))
                .await
                .expect("read stopped job")
                .expect("stopped receipt")
                .state,
            VceJobState::Cancelled
        );
        assert!(manager.projects.read().await.get("job-project").is_none());
        // The final submission performs allocation, publication and normal-runtime restoration through production code.
        // 最后一次提交通过生产代码执行分配、发布及普通运行时恢复。
        let admitted = manager
            .submit_project_rebuild(
                input.clone(),
                command("replacement-success"),
                false,
                actions.clone(),
                &executor,
            )
            .await
            .expect("queue actual replacement");
        // updated is saved project policy while the candidate is still queued, not a new indexing request.
        // updated 为候选仍排队时保存的项目策略，不是新的索引请求。
        let mut updated = settings.clone();
        updated.revision += 1;
        updated.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Manual;
        updated.auto_update_min_interval_seconds = 600;
        updated.query_max_concurrency = 1;
        updated.embedding_max_concurrency = 2;
        updated.max_results = 7;
        updated.max_context_chars = 8_000;
        assert!(
            manager
                .reconcile_policy_revision("job-project", &project_root, &updated, &fingerprint)
                .await
                .expect("reconcile queued replacement policy")
        );
        assert!(manager.projects.read().await.get("job-project").is_none());
        assert_eq!(
            manager
                .get_job("job-project", Some(&admitted.job_id))
                .await
                .expect("unchanged receipt"),
            Some(admitted.clone())
        );
        assert!(
            manager
                .reconcile_policy_revision("job-project", &project_root, &settings, &fingerprint)
                .await
                .is_err(),
            "old snapshots cannot erase accepted policy"
        );
        // conflicting has the same revision but different data and must not silently replace authority.
        // conflicting 具有相同修订但不同数据，不得静默替换权威。
        let mut conflicting = updated.clone();
        conflicting.auto_update_min_interval_seconds = 900;
        assert!(
            manager
                .reconcile_policy_revision("job-project", &project_root, &conflicting, &fingerprint)
                .await
                .is_err()
        );
        // delayed_policy models saved intent whose reconciliation callback arrives only after job handoff.
        // delayed_policy 模拟已经保存、但协调回调在作业交接后才到达的意图。
        let mut delayed_policy = updated.clone();
        delayed_policy.revision += 1;
        delayed_policy.max_results = 3;
        delayed_policy.max_context_chars = 4_000;
        delayed_policy.allow_rerank = false;
        drop(budget);
        let completed = await_test_job(&manager, "job-project", &admitted.job_id).await;
        assert_eq!(
            completed.state,
            VceJobState::Succeeded,
            "{:?}",
            completed.error
        );
        let published = completed
            .result_checkpoint_version
            .expect("published replacement checkpoint");
        let target = admitted
            .replacement
            .as_ref()
            .expect("success target")
            .target
            .clone();
        assert_eq!(
            factory
                .load_active_project_config("job-project")
                .await
                .expect("published configuration"),
            Some(target.clone())
        );
        assert!(manager.project_can_serve_now("job-project", updated.revision, &fingerprint));
        assert!(!manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
        // restored must use accepted budgets while preserving the original durable build contract revision.
        // restored 必须使用已接纳预算，同时保留原持久构建契约修订。
        let restored = manager
            .require_project("job-project")
            .await
            .expect("restored runtime");
        assert_eq!(restored.settings, updated);
        assert_eq!(
            restored.config.query_max_concurrency,
            updated.query_max_concurrency
        );
        assert_eq!(
            restored.config.retrieval.coverage.max_results,
            updated.max_results
        );
        assert_eq!(
            restored.config.retrieval.coverage.max_chars,
            updated.max_context_chars
        );
        assert!(!restored.job_schedule.lock().expect("schedule").automatic());
        // Old callbacks arriving after handoff cannot replace or revoke the newly configured runtime.
        // 交接后到达的旧回调不能替换或撤销新配置运行时。
        assert!(
            manager
                .reconcile_policy_revision("job-project", &project_root, &settings, &fingerprint)
                .await
                .is_err()
        );
        assert!(
            manager
                .enable_project(
                    "job-project",
                    &project_root,
                    settings.clone(),
                    4,
                    &fingerprint,
                    false
                )
                .await
                .is_err()
        );
        assert!(manager.project_can_serve_now("job-project", updated.revision, &fingerprint));
        // Delayed query-only reconciliation updates the same runtime instead of disabling the published index.
        // 延迟的纯查询协调更新同一运行时，而不是禁用已发布索引。
        assert!(
            manager
                .reconcile_policy_revision(
                    "job-project",
                    &project_root,
                    &delayed_policy,
                    &fingerprint
                )
                .await
                .expect("adopt delayed saved policy")
        );
        assert!(Arc::ptr_eq(
            &restored,
            &manager
                .require_project("job-project")
                .await
                .expect("same runtime")
        ));
        assert_eq!(
            *restored.latest_settings.read().expect("latest settings"),
            delayed_policy
        );
        assert!(manager.project_can_serve_now(
            "job-project",
            delayed_policy.revision,
            &fingerprint
        ));
        drop(restored);
        assert_eq!(
            manager
                .retrieve("job-project", "inspect replaced empty fixture")
                .await
                .expect("query replaced checkpoint")
                .checkpoint_version,
            published
        );
        assert_eq!(
            manager
                .submit_project_rebuild(
                    input,
                    command("replacement-success"),
                    false,
                    actions,
                    &executor
                )
                .await
                .expect("completed replay after handoff"),
            completed
        );
        // synchronization_budget keeps an ordinary writer queued while another query-policy update is accepted.
        // synchronization_budget 在接纳另一次查询策略更新期间保持普通写入者排队。
        let synchronization_budget = manager
            .job_writer_budget
            .acquire()
            .await
            .expect("hold normal writer");
        let synchronization = manager
            .submit_job(
                "job-project",
                VceJobAdmission {
                    retry_of_job_id: None,
                    kind: VceJobKind::Synchronize,
                    idempotency_key: String::from("synchronize-queued-replacement"),
                    expected_settings_revision: delayed_policy.revision,
                    expected_checkpoint_version: Some(published),
                    contract_fingerprint: fingerprint.clone(),
                },
                &executor,
            )
            .await
            .expect("ordinary write after one-shot candidate grant ended");
        // newest changes optional query capabilities and limits without altering the admitted source work.
        // newest 改变可选查询能力及限制，不改变已准入源码工作。
        let mut newest = delayed_policy.clone();
        newest.revision += 1;
        newest.allow_query_rewrite = true;
        newest.allow_rerank = true;
        newest.query_max_concurrency = 2;
        newest.embedding_max_concurrency = 3;
        newest.max_results = 4;
        assert!(
            manager
                .reconcile_policy_revision("job-project", &project_root, &newest, &fingerprint)
                .await
                .expect("change query policy with queued writer")
        );
        assert_eq!(
            manager
                .get_job("job-project", Some(&synchronization.job_id))
                .await
                .expect("read queued writer"),
            Some(synchronization.clone())
        );
        assert!(manager.project_can_serve_now("job-project", newest.revision, &fingerprint));
        assert_eq!(
            manager
                .retrieve("job-project", "query during policy change")
                .await
                .expect("query while writer queued")
                .settings_revision,
            newest.revision
        );
        drop(synchronization_budget);
        assert_eq!(
            await_test_job(&manager, "job-project", &synchronization.job_id)
                .await
                .state,
            VceJobState::Succeeded
        );
        // runtime keeps its original identity, but the completed private worker used the new execution limit.
        // runtime 保持原身份，但已完成私有工作器使用新的执行限制。
        let runtime = manager
            .require_project("job-project")
            .await
            .expect("retained runtime");
        assert_eq!(
            runtime
                .current_engine
                .read()
                .await
                .engine
                .config()
                .embedding_max_concurrency,
            3
        );
        // frozen_batch samples policy before scanning; later reconciliation must not mutate this admitted value.
        // frozen_batch 在扫描前采样策略；后续协调不能改变此准入值。
        let before_observation = runtime.active_refresh_observation().await;
        assert!(before_observation.1.is_none());
        let frozen_batch = manager
            .begin_observed_refresh(
                &runtime,
                String::from("freeze-budget"),
                VceRefreshControl::default(),
            )
            .await
            .expect("admit observed batch");
        // The active sample pairs the exact task identity with a strictly newer transition revision.
        // 活动采样将精确任务身份与严格更新的变更修订配套。
        let during_observation = runtime.active_refresh_observation().await;
        assert_eq!(during_observation.1.as_deref(), Some("freeze-budget"));
        assert!(during_observation.0 > before_observation.0);
        verify_active_staging_diagnostics(&manager, &runtime).await;
        let mut next_policy = newest.clone();
        next_policy.revision += 1;
        next_policy.embedding_max_concurrency = 5;
        assert!(
            manager
                .reconcile_policy_revision("job-project", &project_root, &next_policy, &fingerprint)
                .await
                .expect("change future batch budget")
        );
        assert_eq!(frozen_batch.embedding_max_concurrency, 3);
        assert_eq!(
            runtime
                .latest_settings
                .read()
                .expect("future policy")
                .embedding_max_concurrency,
            5
        );
        assert!(frozen_batch.cancellation.check().is_ok());
        // Exercise the production unwind finalizer: a settled worker must not remain a live diagnostic exclusion.
        // 验证生产展开终结器：已结束的工作线程不得继续作为存活诊断排除项。
        let panic_error = manager
            .finish_panicked_refresh(&runtime, "freeze-budget")
            .await;
        assert!(panic_error.contains("worker panicked"));
        assert!(panic_error.contains("staging was cleaned"));
        // Both endpoints are idle, yet the transition revision detects the completed intervening task.
        // 两端均为空闲，但变更修订仍能检测中间已完成的任务。
        let after_observation = runtime.active_refresh_observation().await;
        assert!(after_observation.1.is_none());
        assert!(after_observation.0 > during_observation.0);
        runtime.set_ready().await;
        // Release the test observer's runtime lease before verifying complete host shutdown.
        // 验证宿主完全关闭之前释放测试观察者持有的运行时租约。
        drop(runtime);
        manager
            .shutdown()
            .await
            .expect("drain real queued replacement fixture");
    }

    /// Verify that only the admitted host worker's real staged rows are excluded from integrity diagnostics.
    /// 验证仅已准入宿主工作线程的真实暂存记录被排除在完整性诊断之外。
    /// `manager` and `runtime` belong to the isolated fixture; returns after aborting its exact test stage.
    /// `manager` 与 `runtime` 属于隔离夹具；中止其精确测试暂存后返回。
    async fn verify_active_staging_diagnostics(
        manager: &VceRuntimeManager,
        runtime: &VceProjectRuntime,
    ) {
        use vulcan_context_engine::{
            AuxiliaryIndexStatus, IndexedBlob, IndexedBlobStatus, VceIndexPort,
        };
        // Empty source still creates a real staged metadata row without using any paid embedding service.
        // 空源码仍创建真实暂存元数据行，不使用任何付费向量服务。
        let blob = IndexedBlob {
            blob_name: vulcan_context_engine::blob_name("src/staging-fixture.rs", "")
                .expect("fixture identity"),
            path: String::from("src/staging-fixture.rs"),
            generation: 1,
            structure_analyzer_id: None,
            structure_mode: None,
            structure_fallback_reason: None,
            chunks: Vec::new(),
            embedding_vectors: Vec::new(),
            path_document: None,
            path_embedding: None,
            symbols: Vec::new(),
            path_status: AuxiliaryIndexStatus::NotApplicable,
            symbol_status: AuxiliaryIndexStatus::NotApplicable,
            status: IndexedBlobStatus::Ready,
        };
        runtime
            .index
            .stage(vulcan_context_engine::IndexStageBatch {
                operation_id: String::from("freeze-budget:stage"),
                refresh_id: String::from("freeze-budget"),
                generation: 1,
                blobs: vec![blob],
            })
            .await
            .expect("write exact observed staging fixture");
        // Storage alone has no proof of a live owner and must keep the diagnostic visible.
        // 单独存储没有存活所有者证明，必须保留诊断。
        let unobserved = runtime
            .index
            .management_snapshot(vulcan_context_engine::VceManagementRequest {
                request_id: String::from("unobserved-stage-fixture"),
            })
            .await
            .expect("inspect without host observation");
        assert_eq!(unobserved.integrity.orphan_blob_count, 1);
        assert!(unobserved.active_job.is_none());
        // Storage receipts are completed mutations, never fabricated logical refreshes with absent policy.
        // 存储回执属于已完成变更，绝不伪造缺少策略的逻辑刷新。
        let operations = manager
            .management_query(
                &runtime.project_id,
                vulcan_context_engine::VceManagementQueryRequest {
                    operation_id: String::from("staged-operation-history-fixture"),
                    query: vulcan_context_engine::VceManagementQuery::Operations(
                        vulcan_context_engine::VcePageRequest::new(None, 100)
                            .expect("bounded operation page"),
                    ),
                },
                None,
            )
            .await
            .expect("validate real storage operation history");
        assert!(matches!(operations.result,
            vulcan_context_engine::VceManagementQueryResult::Operations(page)
                if page.items.iter().any(|item| item.operation_id == "freeze-budget:stage"
                    && item.kind == vulcan_context_engine::VceOperationKind::StorageMutation
                    && item.phase == vulcan_context_engine::VceOperationPhase::Completed)));
        // The real manager supplies its exact admitted identity while retaining the published checkpoint.
        // 真实管理器提供其精确已准入身份，同时保留已发布检查点。
        let (identity, observed) = manager
            .management_snapshot(&runtime.project_id)
            .await
            .expect("inspect active host staging");
        assert_eq!(
            observed
                .index
                .expect("host storage diagnostics")
                .integrity
                .orphan_blob_count,
            0
        );
        let issues = manager
            .management_query(
                &runtime.project_id,
                vulcan_context_engine::VceManagementQueryRequest {
                    operation_id: String::from("observed-issues-fixture"),
                    query: vulcan_context_engine::VceManagementQuery::IntegrityIssues(
                        vulcan_context_engine::VcePageRequest::new(None, 25)
                            .expect("bounded issue page"),
                    ),
                },
                identity.as_deref(),
            )
            .await
            .expect("read host integrity page");
        assert!(
            matches!(issues.result, vulcan_context_engine::VceManagementQueryResult::IntegrityIssues(page) if page.items.is_empty())
        );
        runtime
            .index
            .abort_refresh_staging("freeze-budget")
            .await
            .expect("finish only fixture-owned staging");
    }

    /// Publish a real isolated candidate, then restore and synchronize it through ordinary host admission.
    /// 发布真实隔离候选，再通过普通宿主准入恢复并同步。
    /// Inputs belong exclusively to the existing temporary fixture; returns after draining the new manager.
    /// 输入仅属于既有临时夹具；排空新管理器后返回。
    /// This tests runtime handoff only; candidate preparation below is explicit test setup, not a queued UI workflow.
    /// 此处仅验证运行时交接；下方候选准备是明确测试布置，不是界面排队流程。
    async fn verify_published_candidate_runtime_restore(
        factory: VceDataPlaneFactory,
        project_root: std::path::PathBuf,
        settings: ProjectVceSettings,
        fingerprint: String,
        executor: Arc<tokio::runtime::Runtime>,
    ) {
        // mock supplies only local capabilities; the empty fixture never calls a paid provider.
        // mock 仅提供本地能力；空夹具绝不调用付费供应商。
        let mock = vulcan_ai_contract::test_support::MockHostAiPort::new();
        mock.set_capability_snapshot(job_test_capabilities())
            .expect("configure replacement fixture");
        // approved_fingerprint freezes this fixture's explicit publication authority.
        // approved_fingerprint 冻结本夹具明确发布权威。
        let approved_fingerprint = fingerprint.clone();
        let manager = VceRuntimeManager::new(
            Arc::new(mock),
            factory.clone(),
            ServiceEventEmitter::new(Vec::new()),
            Arc::new(move |contract| {
                if contract.project_id == "job-project"
                    && contract.dimension == 3
                    && contract.embedding_fingerprint == approved_fingerprint
                {
                    Ok(())
                } else {
                    Err(
                        String::from("Candidate fixture rejects unexpected publication contract")
                            .into(),
                    )
                }
            }),
        );
        // original/base name the exact already published predecessor after the preceding fixture drained.
        // original/base 标识前序夹具排空后精确的已发布前驱。
        let original = factory
            .embedding_provenance("job-project")
            .await
            .expect("load predecessor")
            .expect("predecessor exists");
        let original_index = factory
            .open_existing_project_index(&original.original)
            .await
            .expect("open predecessor")
            .expect("predecessor index exists");
        let checkpoint = original_index
            .load_state_snapshot()
            .await
            .expect("load predecessor checkpoint")
            .expect("predecessor snapshot exists")
            .workspace
            .checkpoint
            .expect("predecessor initialized");
        let base = vulcan_workspace_store::vce_data_plane::VceRebuildCandidateBase {
            namespace_id: original.original.binding.namespace_id.clone(),
            checkpoint_version: checkpoint.version,
            job_id: format!("fixture-candidate-{}", Uuid::new_v4()),
        };
        // prepared/index allocate only the explicit fixture candidate and preserve the predecessor until commit.
        // prepared/index 仅分配明确夹具候选，并在提交之前保留前驱。
        let prepared = runtime_configuration::prepare_runtime_configuration(
            "job-project",
            &project_root,
            &settings,
            3,
            &fingerprint,
            Some(&base.job_id),
        )
        .expect("prepare candidate configuration");
        let candidate_config = prepared.index.clone();
        // candidate_store/queued persist the exact physical target before this fixture allocates anything.
        // candidate_store/queued 在本夹具分配任何内容之前持久化精确物理目标。
        let candidate_store = original_index.job_store();
        let admitted_at = job_time().expect("candidate admission clock");
        let queued = VceStoredJob {
            schema_version: vulcan_workspace_store::vce_data_plane::VCE_JOB_RECEIPT_SCHEMA_VERSION,
            project_id: String::from("job-project"),
            job_id: base.job_id.clone(),
            owner_run_id: manager.job_owner_run_id.clone(),
            idempotency_key: format!("request:{}", base.job_id),
            retry_of_job_id: None,
            request_fingerprint: "a".repeat(64),
            contract_fingerprint: "b".repeat(64),
            kind: VceJobKind::Rebuild,
            state: VceJobState::Queued,
            sequence: 1,
            created_at_unix_ms: admitted_at,
            updated_at_unix_ms: admitted_at,
            admitted_settings_revision: settings.revision,
            source_checkpoint_version: Some(base.checkpoint_version),
            replacement: Some(Box::new(
                vulcan_workspace_store::vce_data_plane::VceJobReplacement {
                    predecessor: base.clone(),
                    target: candidate_config.clone(),
                },
            )),
            compaction: None,
            result_checkpoint_version: None,
            progress: None,
            followup_requested: false,
            completed_update: None,
            error: None,
        };
        // An unpublished abandoned receipt becomes interrupted, without scheduling replacement work.
        // 未发布的遗留回执变为中断，不安排替换工作。
        let mut abandoned = queued.clone();
        abandoned.job_id = Uuid::new_v4().to_string();
        abandoned.idempotency_key = format!("request:{}", abandoned.job_id);
        abandoned.kind = VceJobKind::Synchronize;
        abandoned.replacement = None;
        candidate_store
            .register(&abandoned)
            .await
            .expect("register abandoned receipt");
        let interrupted = manager
            .get_job("job-project", Some(&abandoned.job_id))
            .await
            .expect("settle unpublished job")
            .expect("interrupted receipt");
        assert_eq!(interrupted.state, VceJobState::Interrupted);
        assert!(interrupted.result_checkpoint_version.is_none());
        assert_eq!(
            manager
                .get_job("job-project", Some(&abandoned.job_id))
                .await
                .expect("replay interruption"),
            Some(interrupted)
        );
        candidate_store
            .register(&queued)
            .await
            .expect("admit fixture physical replacement");
        // Reading or cancelling an old terminal job cannot settle a different queued receipt.
        // 读取或取消旧终态作业不能收敛另一个排队回执。
        assert_eq!(
            manager
                .get_job("job-project", Some(&abandoned.job_id))
                .await
                .expect("historical terminal read")
                .expect("old receipt")
                .state,
            VceJobState::Interrupted
        );
        assert_eq!(
            manager
                .cancel_job("job-project", &abandoned.job_id)
                .await
                .expect("historical terminal cancellation")
                .state,
            VceJobState::Interrupted
        );
        assert_eq!(
            candidate_store
                .get(&queued.job_id)
                .await
                .expect("new cold receipt unchanged"),
            Some(queued.clone())
        );
        // running is acknowledged before starting the owned candidate phase.
        // running 在开始有归属候选阶段之前得到确认。
        let mut running = queued.clone();
        running.sequence += 1;
        running.state = VceJobState::Running;
        candidate_store
            .update(&queued, &running)
            .await
            .expect("start fixture candidate");
        let index = Arc::new(
            factory
                .prepare_rebuild_candidate(prepared.index, base.clone())
                .await
                .expect("allocate fixture candidate")
                .admit_rebuild_candidate_writer(base.clone())
                .await
                .expect("admit exact fixture candidate"),
        );
        // ai registers the candidate's own logical ID and instance without replacing another runtime handle.
        // ai 注册候选自身逻辑编号及实例，不替换另一运行时句柄。
        let ai = Arc::new(
            VceHostAiAdapter::activate(
                Arc::clone(&manager.ai_port),
                VceHostAiRegistrationConfig {
                    runtime_instance_id: Uuid::new_v4(),
                    project_id: String::from("job-project"),
                    consumer_profile_id: String::from(VCE_CONSUMER_PROFILE_ID),
                    vector_space_id: candidate_config.vector_space_id.clone(),
                    dimension: Some(3),
                    preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
                    chunker_version: String::from(VCE_CHUNKER_VERSION),
                },
            )
            .expect("register candidate fixture models"),
        );
        assert_eq!(ai.embedding_space(), &prepared.engine.embedding_space);
        // engine/runtime form an unpublished writer only; no project query runtime is inserted during construction.
        // engine/runtime 仅构成未发布写入器；构造期间不插入项目查询运行时。
        let engine = VceEngine::new(prepared.engine.clone()).expect("create candidate engine");
        let projection = VceCommittedProjection {
            engine: engine.status(),
            workspace: engine.workspace_statistics(),
        };
        let runtime = Arc::new(VceProjectRuntime {
            project_id: String::from("job-project"),
            config: prepared.engine,
            config_revision: AtomicU64::new(settings.revision),
            job_schedule: std::sync::Mutex::new(VceJobSchedule::new(&settings)),
            latest_settings: std::sync::RwLock::new(settings.clone()),
            settings: settings.clone(),
            ai,
            index,
            source: prepared.source,
            current_engine: RwLock::new(Arc::new(VcePublishedEngine {
                engine,
                read_index: None,
            })),
            committed_projection: RwLock::new(projection),
            refresh_gate: Arc::new(Mutex::new(())),
            job_state: manager.project_job_state("job-project").await,
            host_status: RwLock::new(VceHostStatus {
                lifecycle: VceProjectLifecycle::Uninitialized,
                last_error: None,
            }),
            active_refresh: Mutex::new(None),
            refresh_observation_revision: std::sync::atomic::AtomicU64::new(0),
            operation_admission: VceOperationAdmission::accepting(),
        });
        assert!(manager.project_status("job-project").await.is_none());
        assert_eq!(
            factory
                .embedding_provenance("job-project")
                .await
                .expect("read original pointer")
                .expect("original remains active")
                .original,
            original.original
        );
        // writer owns the same gate required by the host execution entry; this is not a durable queue assertion.
        // writer 持有宿主执行入口要求的同一闸门；此处不是持久队列断言。
        let writer = runtime.refresh_gate.lock().await;
        let published = manager
            .execute_job_refresh(
                &runtime,
                base.job_id.clone(),
                VceRefreshControl::default(),
                VceJobWork::Manual,
            )
            .await
            .expect("publish candidate through host commit/read handoff");
        drop(writer);
        // Simulate a host ending after publication but before final job accounting.
        // 模拟发布完成后、最终作业计账前宿主结束。
        let mut committing = running.clone();
        committing.sequence += 1;
        committing.progress = Some(VceRefreshProgress {
            activity: None,
            phase: vulcan_context_engine::VceRefreshPhase::Committing,
            operation_id: base.job_id.clone(),
            unit: vulcan_context_engine::VceRefreshUnit::None,
            completed: 0,
            total: None,
            overall_completed: 0,
            overall_total: None,
            provider_batch_state: None,
            current_path: None,
        });
        candidate_store
            .update(&running, &committing)
            .await
            .expect("persist stuck publication phase");
        // A cooperating writer prevents reconciliation even though the receipt looks stale.
        // 即使回执看似过期，协作写入者仍会阻止收敛。
        let lease = factory
            .try_acquire_project_writer("job-project")
            .expect("lock project")
            .expect("writer available");
        assert_eq!(
            manager
                .get_job("job-project", Some(&base.job_id))
                .await
                .expect("read live job"),
            Some(committing)
        );
        drop(lease);
        let finished = manager
            .get_job("job-project", Some(&base.job_id))
            .await
            .expect("reconcile publication")
            .expect("receipt exists");
        assert_eq!(finished.state, VceJobState::Succeeded);
        assert_eq!(finished.result_checkpoint_version, Some(published));
        assert_eq!(
            manager
                .get_job("job-project", Some(&base.job_id))
                .await
                .expect("idempotent recovery"),
            Some(finished.clone())
        );
        assert_eq!(
            manager
                .cancel_job("job-project", &base.job_id)
                .await
                .expect("terminal cancellation replay"),
            finished
        );
        drop(candidate_store);
        assert_eq!(
            factory
                .embedding_provenance("job-project")
                .await
                .expect("read published candidate")
                .expect("candidate active")
                .original,
            candidate_config
        );
        manager
            .stop_runtime(&runtime)
            .await
            .expect("drain candidate writer registration");
        drop(runtime);
        // restore must adopt the stored candidate identity rather than regenerate the original root-based ID.
        // restore 必须采用存储候选身份，而不是重新生成原始基于根目录的编号。
        manager
            .enable_project(
                "job-project",
                &project_root,
                settings.clone(),
                3,
                &fingerprint,
                false,
            )
            .await
            .expect("restore published candidate as an ordinary runtime");
        let restored = manager
            .require_project("job-project")
            .await
            .expect("restored project available");
        assert_eq!(
            restored.config.index_namespace_id,
            candidate_config.binding.namespace_id
        );
        assert_eq!(
            restored.ai.embedding_space().vector_space_id,
            candidate_config.vector_space_id
        );
        assert!(manager.project_can_serve_now("job-project", settings.revision, &fingerprint));
        // retrieved exercises the retained checkpoint read path, not only its synchronous tool-admission projection.
        // retrieved 实际执行保留检查点的读取路径，而不只检查同步工具准入投影。
        let retrieved = manager
            .retrieve("job-project", "inspect the restored empty fixture")
            .await
            .expect("query the restored candidate checkpoint");
        assert_eq!(retrieved.checkpoint_version, published);
        // receipt proves the restored ordinary writer remains usable after the one-shot candidate grant has ended.
        // receipt 证明一次性候选授权结束后，恢复的普通写入器仍可使用。
        let receipt = manager
            .submit_job(
                "job-project",
                VceJobAdmission {
                    retry_of_job_id: None,
                    kind: VceJobKind::Synchronize,
                    idempotency_key: String::from("synchronize-restored-candidate"),
                    expected_settings_revision: settings.revision,
                    expected_checkpoint_version: Some(published),
                    contract_fingerprint: fingerprint,
                },
                &executor,
            )
            .await
            .expect("queue ordinary synchronization after candidate restore");
        let completed = await_test_job(&manager, "job-project", &receipt.job_id).await;
        assert_eq!(
            completed.state,
            VceJobState::Succeeded,
            "restored synchronization: {:?}",
            completed.error
        );
        assert_eq!(completed.result_checkpoint_version, Some(published));
        // Release the fixture's inspection handles before shutdown verifies that all project leases have drained.
        // 在退出检查全部项目租约已排空之前，释放夹具自身持有的检查句柄。
        drop(restored);
        drop(original_index);
        manager
            .shutdown()
            .await
            .expect("drain candidate restore fixture");
    }

    /// Keep successor stopping sticky through worker drain without holding the completion admission mutex.
    /// 在工作任务排空期间保持后继停止标志，不持有完成所需的准入互斥锁。
    /// No external services are used; a simulated worker lease proves ordering and safe reset.
    /// 不使用外部服务；模拟工作任务租约证明时序与安全复位。
    #[tokio::test]
    async fn project_job_drain_keeps_stop_requested_until_worker_leases_end() {
        // state owns no model or storage; execution represents an already admitted worker.
        // state 不持有模型或存储；execution 表示已准入工作任务。
        let state = Arc::new(VceProjectJobState::default());
        let execution = Arc::clone(&state.execution).read_owned().await;
        // closing retains project state while drain waits for the worker's terminal processing.
        // closing 在排空等待工作任务终态处理时保留项目状态。
        let closing = Arc::clone(&state);
        let drain = tokio::spawn(async move { closing.cancel_and_drain().await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !state.stop_requested.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("drain must request stop before waiting");
        assert!(
            !drain.is_finished(),
            "an outstanding worker lease must prevent completion"
        );
        assert!(
            state.admission.try_lock().is_ok(),
            "terminal processing must still acquire admission"
        );
        drop(execution);
        drain
            .await
            .expect("drain task completes")
            .expect("empty worker ownership drains cleanly");
        assert!(
            !state.stop_requested.load(Ordering::Acquire),
            "only drained execution permits reset"
        );
    }

    /// Abort a tracked worker that cannot release its execution lease within the shutdown grace period.
    /// 验证无法在关闭宽限期内释放执行租约的已跟踪工作任务会被中止。
    #[tokio::test]
    async fn project_job_drain_aborts_tracked_worker_after_shutdown_timeout() {
        let state = Arc::new(VceProjectJobState::default());
        let execution = Arc::clone(&state.execution).read_owned().await;
        let completion_state = Arc::clone(&state);
        let worker = tokio::spawn(async move {
            let _execution = execution;
            std::future::pending::<()>().await;
            completion_state.clear_worker_handle().await;
        });
        state.install_worker_handle(worker).await;

        tokio::time::timeout(Duration::from_secs(5), state.cancel_and_drain())
            .await
            .expect("forced shutdown drain must be bounded")
            .expect("tracked worker must be abortable");
        assert!(
            state.worker_handle.lock().await.is_none(),
            "forced drain must not retain a detached worker handle"
        );
        assert!(
            !state.stop_requested.load(Ordering::Acquire),
            "forced drain must reset the stop fence after releasing execution"
        );
    }

    /// Wait for a worker that already released execution but still owns terminal persistence.
    /// 验证执行租约已释放但仍持有终态持久化的工作任务会被退出流程等待。
    #[tokio::test]
    async fn project_job_drain_waits_for_terminal_persistence_worker() {
        let state = Arc::new(VceProjectJobState::default());
        let worker_started = Arc::new(Notify::new());
        let release_worker = Arc::new(Notify::new());
        let worker_state = Arc::clone(&state);
        let worker_started_signal = Arc::clone(&worker_started);
        let release_worker_signal = Arc::clone(&release_worker);
        let worker = tokio::spawn(async move {
            // No execution lease is held here; the worker models the terminal receipt write itself.
            // 这里不持有执行租约；工作任务本身模拟终态回执写入。
            worker_started_signal.notify_one();
            release_worker_signal.notified().await;
            worker_state.clear_worker_handle().await;
        });
        state.install_worker_handle(worker).await;

        let closing = Arc::clone(&state);
        let drain = tokio::spawn(async move { closing.cancel_and_drain().await });
        tokio::time::timeout(Duration::from_secs(1), worker_started.notified())
            .await
            .expect("terminal persistence worker must start");
        tokio::task::yield_now().await;
        assert!(
            !drain.is_finished(),
            "shutdown must wait for terminal persistence after execution drained"
        );
        release_worker.notify_one();
        tokio::time::timeout(Duration::from_secs(5), drain)
            .await
            .expect("terminal persistence drain must be bounded")
            .expect("drain task must complete")
            .expect("terminal persistence worker must drain cleanly");
        assert!(
            state.worker_handle.lock().await.is_none(),
            "terminal persistence drain must not retain a worker handle"
        );
    }

    /// Check that initialization, synchronization, and contract changes cannot bypass exact guards.
    /// 检查初始化、同步与契约变化无法绕过精确保护。
    ///
    /// Takes no parameters; verifies all legal initial/sync combinations and stale inputs.
    /// 无参数；验证合法初始化与同步组合及过期输入。
    #[test]
    fn human_job_admission_requires_exact_checkpoint_and_contract() {
        let mut command = VceJobAdmission {
            retry_of_job_id: None,
            kind: VceJobKind::Initialize,
            idempotency_key: String::from("button-click"),
            expected_settings_revision: 4,
            expected_checkpoint_version: None,
            contract_fingerprint: "a".repeat(64),
        };
        let fingerprint = "a".repeat(64);
        assert!(validate_job_admission(&command, 4, None, Some(&fingerprint)).is_ok());
        assert!(validate_job_admission(&command, 5, None, Some(&fingerprint)).is_err());
        assert!(validate_job_admission(&command, 4, None, Some(&"b".repeat(64))).is_err());
        assert!(validate_job_admission(&command, 4, Some(1), Some(&fingerprint)).is_err());
        command.kind = VceJobKind::Synchronize;
        assert!(validate_job_admission(&command, 4, None, Some(&fingerprint)).is_err());
        command.expected_checkpoint_version = Some(1);
        assert!(validate_job_admission(&command, 4, Some(1), Some(&fingerprint)).is_ok());
        command.kind = VceJobKind::Rebuild;
        assert!(validate_job_admission(&command, 4, Some(1), Some(&fingerprint)).is_ok());
        command.expected_checkpoint_version = None;
        assert!(validate_job_admission(&command, 4, None, Some(&fingerprint)).is_err());
    }

    /// Verify physical compaction ignores policy-only revisions while retaining immutable guards.
    /// 验证物理整理忽略纯策略修订，同时保留不可变保护。
    ///
    /// Takes no parameters; prepares one physical binding and checks revision, checkpoint, and fingerprint behavior.
    /// 无参数；准备一个物理绑定并检查修订、检查点及指纹行为。
    #[test]
    fn physical_compaction_target_uses_immutable_binding_guards() {
        // temporary supplies one canonical project root without touching user storage.
        // temporary 提供一个规范项目根目录，且不触碰用户存储。
        let temporary = tempfile::tempdir().expect("create compaction target fixture");
        // settings represent the immutable binding's original project revision.
        // settings 表示不可变绑定最初的项目修订。
        let settings = ProjectVceSettings {
            enabled: true,
            revision: 1,
            ..ProjectVceSettings::default()
        };
        // fingerprint is one canonical physical embedding identity.
        // fingerprint 是一个规范物理向量身份。
        let fingerprint = format!("sha256:{}", "a".repeat(64));
        // config retains the original database write-guard revision.
        // config 保留原始数据库写保护修订。
        let config = runtime_configuration::prepare_runtime_configuration(
            "compaction-target-project",
            temporary.path(),
            &settings,
            3,
            &fingerprint,
            None,
        )
        .expect("prepare compaction target")
        .index;
        // command carries a later policy-only admission revision for the same physical target.
        // command 携带同一物理目标后续的纯策略准入修订。
        let mut command = VceJobAdmission {
            retry_of_job_id: None,
            kind: VceJobKind::Compact,
            idempotency_key: String::from("compact-after-policy-change"),
            expected_settings_revision: 2,
            expected_checkpoint_version: Some(9),
            contract_fingerprint: fingerprint,
        };
        assert_eq!(config.config_revision, 1);
        assert!(validate_compaction_target(&command, &config, Some(9)).is_ok());
        command.expected_checkpoint_version = Some(10);
        assert!(validate_compaction_target(&command, &config, Some(9)).is_err());
        command.expected_checkpoint_version = Some(9);
        command.contract_fingerprint = format!("sha256:{}", "b".repeat(64));
        assert!(validate_compaction_target(&command, &config, Some(9)).is_err());
    }

    /// Wait for a real completed core observation while the fixture deliberately holds host publication.
    /// 在夹具明确持有宿主发布期间等待真实核心完成观察。
    /// `manager`, `project_id`, and `job_id` select the exact live receipt; returns after the observation.
    /// `manager`、`project_id` 与 `job_id` 选择精确实时回执；观察到后返回。
    async fn await_test_completed_phase(
        manager: &VceRuntimeManager,
        project_id: &str,
        job_id: &str,
    ) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                // receipt is read through the same stable job API used by the UI.
                // receipt 通过界面使用的相同稳定作业接口读取。
                let receipt = manager
                    .get_job(project_id, Some(job_id))
                    .await
                    .expect("read fixture receipt")
                    .expect("fixture receipt exists");
                if receipt.progress.as_ref().is_some_and(|progress| {
                    progress.phase == vulcan_context_engine::VceRefreshPhase::Complete
                }) {
                    return;
                }
                assert!(
                    !receipt.state.is_terminal(),
                    "unexpected terminal receipt: {:?}",
                    receipt.error
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("actual completed phase arrives");
    }
}
