//! Durable project-job receipts, separate from core checkpoint operations and UI event delivery.
//! 持久项目作业回执，独立于核心检查点操作与界面事件投递。

use super::{
    CountRow, SqliteValue, VceDataPlaneHandle, VceProjectIndexConfig, VceRebuildCandidateBase,
    sqlite_string_literal, sqlite_text,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_context_engine::{VcePhysicalTablePlan, VcePhysicalTableStatistics, VceRefreshProgress};

/// Current durable receipt format written by this runtime.
/// 当前运行时写入的持久回执格式。
///
/// This is the only version this build accepts, in the same sense as the pinned Lance file version: a receipt
/// carrying any other value is rejected rather than interpreted. There is deliberately no "oldest readable
/// version" constant beside it — such a constant is the dual-read path, expressed as a range.
/// 这是本构建唯一接受的版本，与固定的 Lance 文件版本同理：携带任何其他值的回执会被拒绝而不是被解释。此处刻意不
/// 在其旁保留"最旧可读版本"常量——那种常量就是双读路径，只是写成了区间形式。
pub const VCE_JOB_RECEIPT_SCHEMA_VERSION: u32 = 3;

/// Bound storage confirmation retries so a transient SQLite busy state cannot strand a live owner.
/// 限制存储确认重试次数，避免短暂的 SQLite 忙状态把活动作业所有权永久卡住。
const VCE_RECEIPT_CONFIRM_RETRIES: usize = 7;

/// Base delay for bounded receipt reconciliation after a SQLite busy response.
/// SQLite 返回忙状态后进行有界回执收敛时使用的基础延迟。
const VCE_RECEIPT_CONFIRM_RETRY_BASE_MS: u64 = 250;

/// Identify SQLite busy failures that are safe to reconcile after an ambiguous receipt operation.
/// 识别可以在回执操作结果不明确时收敛处理的 SQLite 忙错误。
fn is_retryable_receipt_storage_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("database is locked")
        || normalized.contains("database busy")
        || normalized.contains("sqlite_busy")
}

/// Calculate exponential backoff without allowing one ambiguous write to spin.
/// 计算指数退避，避免一次结果不明确的写入进入忙循环。
fn receipt_retry_delay(attempt: usize) -> std::time::Duration {
    let shift = attempt.min(5) as u32;
    let multiplier = 1_u64 << shift;
    std::time::Duration::from_millis(VCE_RECEIPT_CONFIRM_RETRY_BASE_MS.saturating_mul(multiplier))
}

/// Immutable physical replacement target captured before candidate allocation starts.
/// 候选分配开始之前捕获的不可变物理替换目标。
/// This is recovery identity, not permission to resume work or a provider configuration.
/// 此类型为恢复身份，不是恢复执行权限或供应商配置。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VceJobReplacement {
    /// Exact published predecessor and durable job which owns this attempt.
    /// 精确已发布前驱及持有此次尝试的持久作业。
    pub predecessor: VceRebuildCandidateBase,
    /// Exact separate namespace, root and explicit embedding identity to allocate.
    /// 将要分配的精确独立命名空间、根目录及明确向量身份。
    pub target: VceProjectIndexConfig,
}

/// Explicitly admitted host operation; none of these are AI tools.
/// 明确准入的宿主操作；均不是 AI 工具。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VceJobKind {
    /// Human-requested first initialization.
    /// 人工请求的首次初始化。
    Initialize,
    /// Synchronization against an existing compatible checkpoint.
    /// 针对现有兼容检查点的同步。
    Synchronize,
    /// Human-confirmed reconstruction, preserving the prior committed space until publication.
    /// 人工确认的重建，在发布前保留先前已提交空间。
    Rebuild,
    /// Human-confirmed native physical compaction that preserves the logical checkpoint.
    /// 人工确认且保留逻辑检查点的原生物理整理。
    Compact,
}

/// Durable job lifecycle independent from whether an older checkpoint remains queryable.
/// 持久作业生命周期，独立于旧检查点是否仍可查询。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VceJobState {
    /// Registered durably and waiting for execution capacity.
    /// 已持久登记，等待执行容量。
    Queued,
    /// An admitted worker is executing real stages.
    /// 已准入工作线程正在执行真实阶段。
    Running,
    /// Cooperative cancellation has been accepted and cleanup is pending.
    /// 已受理协作取消，等待清理。
    Cancelling,
    /// All required persistence and publication completed successfully.
    /// 全部必需持久化与发布成功完成。
    Succeeded,
    /// Execution or persistence failed with explicit diagnostics.
    /// 执行或持久化失败，附带明确诊断。
    Failed,
    /// Cancellation and unpublished-stage cleanup completed.
    /// 取消与未发布阶段清理完成。
    Cancelled,
    /// The owning host run ended before a terminal receipt was committed.
    /// 所属宿主运行在提交终态回执前结束。
    Interrupted,
}

impl VceJobState {
    /// Determine whether a durable receipt must never return to an active state.
    /// 判断持久回执是否绝不能再返回活动状态。
    ///
    /// Takes no parameters. Returns true for success, failure, cancellation, or interruption.
    /// 无参数。成功、失败、取消或中断时返回真。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    /// Validate one explicit state-machine edge without resurrecting terminal jobs.
    /// 校验一条明确状态机边，不使终态作业复活。
    ///
    /// `next` is the proposed successor. Returns true only for permitted execution transitions.
    /// `next` 是拟议后继。仅允许的执行转换返回真。
    fn permits(self, next: Self) -> bool {
        match self {
            Self::Queued => matches!(
                next,
                Self::Queued
                    | Self::Running
                    | Self::Cancelling
                    | Self::Cancelled
                    | Self::Failed
                    | Self::Interrupted
            ),
            Self::Running => matches!(
                next,
                Self::Queued
                    | Self::Running
                    | Self::Cancelling
                    | Self::Succeeded
                    | Self::Failed
                    | Self::Interrupted
            ),
            Self::Cancelling => matches!(
                next,
                Self::Cancelling | Self::Cancelled | Self::Failed | Self::Interrupted
            ),
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted => false,
        }
    }
}

/// Last fully published batch within a job that may contain one coalesced successor at a time.
/// 每次至多包含一个合并后续批次的作业内，最近完全发布的批次。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VceCompletedJobUpdate {
    /// Exact successfully published checkpoint, including an unchanged empty checkpoint.
    /// 精确成功发布的检查点，包括未改变的空检查点。
    pub checkpoint_version: u64,
    /// Successful batch completion time, never overwritten by a later failed batch.
    /// 成功批次结束时间，绝不被后续失败批次覆盖。
    pub at_unix_ms: u64,
}

/// Durable lifecycle of one physical table within a compaction job.
/// 整理作业中一张物理表的持久生命周期。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VceCompactionTableState {
    /// The table is admitted but no storage operation has been submitted.
    /// 表已准入，但尚未提交存储操作。
    Pending,
    /// The in-process engine accepted the current bounded operation.
    /// 进程内引擎已受理当前有界操作。
    Queued,
    /// The in-process engine is executing the current atomic operation.
    /// 进程内引擎正在执行当前原子操作。
    Running,
    /// The table no longer requires compaction under the frozen policy.
    /// 表在冻结策略下已不再需要整理。
    Succeeded,
    /// The table operation failed with an explicit diagnostic.
    /// 表操作失败并带有明确诊断。
    Failed,
    /// Cancellation completed at a safe storage-operation boundary.
    /// 取消已在存储操作的安全边界完成。
    Cancelled,
    /// Runtime restart or history expiry made the submitted operation unknowable.
    /// 运行时重启或历史过期导致已提交操作状态不可知。
    StateLost,
    /// The owning host ended before a later bounded round could be submitted.
    /// 所属宿主在后续有界轮次提交前结束。
    Interrupted,
}

impl VceCompactionTableState {
    /// Return whether no more storage work may be attached to this table receipt.
    /// 返回此表回执是否已不能再附加存储工作。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::StateLost | Self::Interrupted
        )
    }

    /// Return whether one non-identical durable table transition is permitted.
    /// 返回一条非恒等持久表状态转换是否允许。
    ///
    /// `next` is the proposed successor. Returns false for regression or terminal rewriting.
    /// `next` 是拟议后继。状态倒退或改写终态时返回假。
    fn permits(self, next: Self) -> bool {
        match self {
            Self::Pending => matches!(
                next,
                Self::Queued | Self::Failed | Self::Cancelled | Self::Interrupted
            ),
            Self::Queued => matches!(
                next,
                Self::Running
                    | Self::Pending
                    | Self::Succeeded
                    | Self::Failed
                    | Self::Cancelled
                    | Self::StateLost
                    | Self::Interrupted
            ),
            Self::Running => matches!(
                next,
                Self::Pending
                    | Self::Succeeded
                    | Self::Failed
                    | Self::Cancelled
                    | Self::StateLost
                    | Self::Interrupted
            ),
            Self::Succeeded
            | Self::Failed
            | Self::Cancelled
            | Self::StateLost
            | Self::Interrupted => false,
        }
    }
}

/// Cumulative physical file-compaction metrics for one table.
/// 一张表的累计物理文件整理指标。
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VceCompactionMetrics {
    /// Total source fragments replaced by acknowledged rounds.
    /// 已确认轮次累计替换的源片段数量。
    pub fragments_removed: u64,
    /// Total replacement fragments added by acknowledged rounds.
    /// 已确认轮次累计新增的替代片段数量。
    pub fragments_added: u64,
    /// Total physical files replaced by acknowledged rounds.
    /// 已确认轮次累计替换的物理文件数量。
    pub files_removed: u64,
    /// Total physical files added by acknowledged rounds.
    /// 已确认轮次累计新增的物理文件数量。
    pub files_added: u64,
}

/// Durable receipt for one table and its current bounded storage operation.
/// 一张表及其当前有界存储操作的持久回执。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VceCompactionTableReceipt {
    /// Exact confirmed action identity.
    /// 精确的已确认操作身份。
    pub action_id: String,
    /// Frozen table statistics and bounded options from the confirmed plan.
    /// 从已确认计划冻结的表统计与有界参数。
    pub plan: VcePhysicalTablePlan,
    /// Current table lifecycle state.
    /// 当前表生命周期状态。
    pub state: VceCompactionTableState,
    /// One-based submitted operation round; zero means pending.
    /// 从一开始的已提交操作轮次；零表示待处理。
    pub round: u32,
    /// Stable identifier of the current or latest storage operation.
    /// 当前或最近一次存储操作的稳定标识。
    pub operation_id: Option<String>,
    /// Exact physical version supplied to the current operation.
    /// 提交给当前操作的精确物理版本。
    pub operation_expected_version: Option<u64>,
    /// Latest authoritative statistics acknowledged from the in-process engine.
    /// 最近一次从进程内引擎确认的权威统计信息。
    pub latest_statistics: VcePhysicalTableStatistics,
    /// Cumulative metrics from every acknowledged successful round.
    /// 所有已确认成功轮次的累计指标。
    pub metrics: VceCompactionMetrics,
    /// Bounded table-local failure or state-loss diagnostic.
    /// 有界的表级失败或状态丢失诊断。
    pub error: Option<String>,
}

/// Small durable compaction payload attached to one host job receipt.
/// 附加到一个宿主作业回执的小型持久整理载荷。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VceCompactionJobReceipt {
    /// Selected physical tables in deterministic role order.
    /// 按确定性角色顺序排列的已选物理表。
    pub tables: Vec<VceCompactionTableReceipt>,
}

impl VceCompactionTableReceipt {
    /// Validate one table receipt without consulting mutable runtime state.
    /// 在不查询可变运行时状态的情况下校验一张表的回执。
    fn validate(&self) -> Result<(), String> {
        let plan_statistics = &self.plan.statistics;
        let latest = &self.latest_statistics;
        let options = &self.plan.options;
        if !vulcan_context_engine::is_sha256_identity(&self.action_id)
            || !self.plan.compaction_required
            || plan_statistics.table_name.trim().is_empty()
            || plan_statistics.table_name.len() > 512
            || plan_statistics.version == 0
            || plan_statistics.small_fragment_count > plan_statistics.fragment_count
            || latest.role != plan_statistics.role
            || latest.table_name != plan_statistics.table_name
            || latest.version == 0
            || latest.version < plan_statistics.version
            || latest.row_count != plan_statistics.row_count
            || latest.index_count != plan_statistics.index_count
            || latest.small_fragment_count > latest.fragment_count
            || [
                options.target_rows_per_fragment,
                options.max_rows_per_group,
                options.max_bytes_per_file,
                options.num_threads,
                options.batch_size,
                options.max_source_fragments,
            ]
            .contains(&0)
            || options.num_threads > 8
        {
            return Err(String::from(
                "VCE compaction table receipt has invalid identity, statistics, or options",
            ));
        }
        if self
            .operation_id
            .as_ref()
            .is_some_and(|operation_id| operation_id.trim().is_empty() || operation_id.len() > 2048)
            || self.operation_expected_version == Some(0)
        {
            return Err(String::from(
                "VCE compaction table operation identity is invalid",
            ));
        }
        let operation_ready = self.round > 0
            && self.operation_id.is_some()
            && self.operation_expected_version.is_some();
        let terminal_without_operation = self.state.is_terminal()
            && self.state != VceCompactionTableState::Succeeded
            && self.round == 0
            && self.operation_id.is_none()
            && self.operation_expected_version.is_none();
        let terminal_after_completed_round = matches!(
            self.state,
            VceCompactionTableState::Failed
                | VceCompactionTableState::Cancelled
                | VceCompactionTableState::Interrupted
        ) && self.round > 0
            && self.operation_id.is_none()
            && self.operation_expected_version.is_none()
            && self.latest_statistics.version > self.plan.statistics.version
            && self.metrics.fragments_removed > 0;
        if (self.state == VceCompactionTableState::Pending
            && (self.operation_id.is_some()
                || self.operation_expected_version.is_some()
                || (self.round == 0 && latest != plan_statistics)))
            || (matches!(
                self.state,
                VceCompactionTableState::Queued | VceCompactionTableState::Running
            ) && !operation_ready)
            || (self.state.is_terminal()
                && !operation_ready
                && !terminal_without_operation
                && !terminal_after_completed_round)
        {
            return Err(String::from(
                "VCE compaction table state does not match its operation round",
            ));
        }
        if self
            .operation_expected_version
            .is_some_and(|version| latest.version < version)
        {
            return Err(String::from(
                "VCE compaction table statistics precede its submitted version",
            ));
        }
        let requires_error = matches!(
            self.state,
            VceCompactionTableState::Failed
                | VceCompactionTableState::StateLost
                | VceCompactionTableState::Interrupted
        );
        if requires_error
            != self
                .error
                .as_ref()
                .is_some_and(|error| !error.trim().is_empty())
            || self.error.as_ref().is_some_and(|error| error.len() > 8192)
        {
            return Err(String::from(
                "VCE compaction table diagnostic does not match its state",
            ));
        }
        if self.state == VceCompactionTableState::Succeeded
            && latest.fragment_count > 1
            && latest.small_fragment_count > 0
        {
            return Err(String::from(
                "VCE compaction table succeeded while still requiring compaction",
            ));
        }
        Ok(())
    }
}

impl VceCompactionJobReceipt {
    /// Validate deterministic bounded table receipts for one compact job.
    /// 校验一个整理作业中确定且有界的表回执。
    fn validate(&self) -> Result<(), String> {
        if self.tables.is_empty() || self.tables.len() > 2 {
            return Err(String::from(
                "VCE compaction job must contain one or two physical tables",
            ));
        }
        let mut roles = std::collections::BTreeSet::new();
        let mut actions = std::collections::BTreeSet::new();
        let mut previous_role = None;
        for table in &self.tables {
            table.validate()?;
            let role = table.plan.statistics.role;
            if previous_role.is_some_and(|previous| previous >= role)
                || !roles.insert(role)
                || !actions.insert(table.action_id.as_str())
            {
                return Err(String::from(
                    "VCE compaction job table order or identity is invalid",
                ));
            }
            previous_role = Some(role);
        }
        Ok(())
    }

    /// Validate a monotonic durable successor without consulting runtime memory.
    /// 在不查询运行时内存的情况下校验单调持久后继。
    ///
    /// `next` is the proposed next receipt. Returns unit only when immutable plans and every
    /// table lifecycle advance without rewriting acknowledged physical facts.
    /// `next` 是拟议下一份回执。仅当不可变计划及所有表生命周期均未改写已确认物理事实时返回空值。
    fn validate_transition(&self, next: &Self) -> Result<(), String> {
        if self.tables.len() != next.tables.len() {
            return Err(String::from(
                "VCE compaction update changed its frozen table set",
            ));
        }
        for (previous, successor) in self.tables.iter().zip(&next.tables) {
            if previous.action_id != successor.action_id || previous.plan != successor.plan {
                return Err(String::from(
                    "VCE compaction update changed its frozen table plan",
                ));
            }
            validate_compaction_table_transition(previous, successor)?;
        }
        Ok(())
    }
}

/// Validate one table's durable state, operation round, statistics, and cumulative metrics.
/// 校验一张表的持久状态、操作轮次、统计信息及累计指标。
///
/// `previous` is already acknowledged and `next` is proposed. Returns unit only for one
/// forward transition that preserves every immutable or cumulative fact.
/// `previous` 已获确认，`next` 是拟议后继。仅对保留全部不可变或累计事实的前向转换返回空值。
fn validate_compaction_table_transition(
    previous: &VceCompactionTableReceipt,
    next: &VceCompactionTableReceipt,
) -> Result<(), String> {
    if previous == next {
        return Ok(());
    }
    let round_advanced = next.round > previous.round;
    let expected_next_round = previous.round.checked_add(1);
    if !previous.state.permits(next.state)
        || next.round < previous.round
        || (round_advanced && expected_next_round != Some(next.round))
        || next.latest_statistics.version < previous.latest_statistics.version
        || next.metrics.fragments_removed < previous.metrics.fragments_removed
        || next.metrics.fragments_added < previous.metrics.fragments_added
        || next.metrics.files_removed < previous.metrics.files_removed
        || next.metrics.files_added < previous.metrics.files_added
    {
        return Err(String::from(
            "VCE compaction table update regressed its durable lifecycle or physical facts",
        ));
    }

    // A new round is admitted only from a pending boundary frozen at the latest acknowledged version.
    // 新轮次只能从待处理边界准入，并冻结最近一次已确认版本。
    if round_advanced {
        if previous.state != VceCompactionTableState::Pending
            || next.state != VceCompactionTableState::Queued
            || next.operation_id.is_none()
            || next.operation_expected_version != Some(previous.latest_statistics.version)
            || next.latest_statistics != previous.latest_statistics
            || next.metrics != previous.metrics
        {
            return Err(String::from(
                "VCE compaction table update has an invalid operation admission",
            ));
        }
        return Ok(());
    }

    // Completing a bounded round may clear its operation only when another pending round is needed.
    // 完成一个有界轮次后，仅在还需下一待处理轮次时才可清除当前操作。
    let completed_round = matches!(
        previous.state,
        VceCompactionTableState::Queued | VceCompactionTableState::Running
    ) && next.state == VceCompactionTableState::Pending;
    if completed_round {
        if next.operation_id.is_some()
            || next.operation_expected_version.is_some()
            || (next.latest_statistics == previous.latest_statistics
                && next.metrics == previous.metrics)
        {
            return Err(String::from(
                "VCE compaction table update did not acknowledge its completed round",
            ));
        }
    } else if next.operation_id != previous.operation_id
        || next.operation_expected_version != previous.operation_expected_version
    {
        return Err(String::from(
            "VCE compaction table update rewrote its admitted operation identity",
        ));
    }

    // Runtime observations cannot rewrite statistics unless the active round has settled.
    // 运行时观察不能改写统计信息；只有活动轮次结算时才可更新。
    let physical_facts_changed =
        next.latest_statistics != previous.latest_statistics || next.metrics != previous.metrics;
    let may_acknowledge_physical_result = matches!(
        previous.state,
        VceCompactionTableState::Queued | VceCompactionTableState::Running
    ) && matches!(
        next.state,
        VceCompactionTableState::Pending
            | VceCompactionTableState::Succeeded
            | VceCompactionTableState::Failed
            | VceCompactionTableState::Cancelled
    );
    if physical_facts_changed && !may_acknowledge_physical_result {
        return Err(String::from(
            "VCE compaction table update rewrote unacknowledged physical facts",
        ));
    }
    Ok(())
}

/// Versioned durable receipt owned by one project and one host execution run.
/// 由单个项目及单次宿主执行持有的版本化持久回执。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VceStoredJob {
    /// Receipt format version, independent from the code index schema.
    /// 回执格式版本，独立于代码索引结构。
    pub schema_version: u32,
    /// Exact stable workbench project identity.
    /// 精确稳定的工作台项目身份。
    pub project_id: String,
    /// Stable job identifier shown to the user and used for cancellation.
    /// 向用户显示并用于取消的稳定作业标识。
    pub job_id: String,
    /// Exact host run that admitted this job; restarts cannot silently resume it.
    /// 准入此作业的精确宿主运行；重启不能静默恢复执行。
    pub owner_run_id: String,
    /// Caller-generated key used to replay admission without duplicate work.
    /// 调用方生成的键，用于重放准入而不重复执行。
    pub idempotency_key: String,
    /// Exact prior failed/cancelled/interrupted job explicitly retried by the user.
    /// 用户明确重试的精确先前失败、取消或中断作业。
    #[serde(default)]
    pub retry_of_job_id: Option<String>,
    /// Hash of the exact submitted command, not a mutable settings object.
    /// 精确提交命令的哈希，而非可变设置对象。
    pub request_fingerprint: String,
    /// Exact indexing-relevant contract admitted for publication.
    /// 发布时准入的精确索引相关契约。
    pub contract_fingerprint: String,
    /// Explicit host operation kind.
    /// 明确宿主操作种类。
    pub kind: VceJobKind,
    /// Durable execution state, never reused as query eligibility.
    /// 持久执行状态，绝不复用为查询资格。
    pub state: VceJobState,
    /// Strictly increasing receipt revision, starting at one.
    /// 严格递增且从一开始的回执修订。
    pub sequence: u64,
    /// Admission wall-clock timestamp used only for display and recovery.
    /// 仅用于显示与恢复的准入墙上时钟时间戳。
    pub created_at_unix_ms: u64,
    /// Latest receipt timestamp; monotonic task scheduling is separate.
    /// 最近回执时间戳；单调任务调度与其分离。
    pub updated_at_unix_ms: u64,
    /// Project settings revision observed during admission, not the physical index identity.
    /// 准入时观察的项目设置修订，而非物理索引身份。
    pub admitted_settings_revision: u64,
    /// Previously committed checkpoint, including a valid empty checkpoint.
    /// 先前已提交检查点，包括合法空检查点。
    pub source_checkpoint_version: Option<u64>,
    /// Explicit physical replacement, absent for ordinary initialization and same-binding updates.
    /// 明确物理替换；普通初始化及同绑定更新时缺失。
    /// Box the uncommon target so ordinary receipts and their asynchronous owners remain small.
    /// 单独分配较少使用的目标，保持普通回执及其异步所有者足够小。
    pub replacement: Option<Box<VceJobReplacement>>,
    /// Table-scoped physical compaction state, present only for compact jobs.
    /// 仅整理作业持有的表作用域物理整理状态。
    #[serde(default)]
    pub compaction: Option<VceCompactionJobReceipt>,
    /// Resulting checkpoint after successful initialization, synchronization or rebuilding.
    /// 成功初始化、同步或重建之后的结果检查点。
    pub result_checkpoint_version: Option<u64>,
    /// Real source/core/storage progress, absent before the first actual phase.
    /// 真实源码、核心或存储进度；首个实际阶段前为空。
    pub progress: Option<VceRefreshProgress>,
    /// At most one coalesced subsequent manual synchronization request.
    /// 至多一个合并的后续手动同步请求。
    pub followup_requested: bool,
    /// Most recent successful batch; absent only before the first success.
    /// 最近成功批次；仅首次成功之前缺失。
    pub completed_update: Option<VceCompletedJobUpdate>,
    /// Bounded terminal or cleanup diagnostic, excluding secrets and source bodies.
    /// 有界终态或清理诊断，不包含密钥和源码正文。
    pub error: Option<String>,
}

impl VceStoredJob {
    /// Validate exact identities and bounded serialization before any durable mutation.
    /// 在任何持久修改前校验精确身份与有界序列化。
    ///
    /// Takes no parameters. Returns explicit malformed-contract diagnostics.
    /// 无参数。返回明确的契约格式错误诊断。
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != VCE_JOB_RECEIPT_SCHEMA_VERSION
            || self.sequence == 0
            || self.admitted_settings_revision == 0
            || self.created_at_unix_ms == 0
        {
            return Err(String::from(
                "VCE job receipt has an invalid version, revision, or admission timestamp",
            ));
        }
        for identity in [
            &self.project_id,
            &self.job_id,
            &self.owner_run_id,
            &self.idempotency_key,
        ] {
            if identity.trim().is_empty() || identity.len() > 512 {
                return Err(String::from(
                    "VCE job identity is empty or exceeds its bound",
                ));
            }
        }
        if !vulcan_context_engine::is_sha256_identity(&self.request_fingerprint) {
            return Err(String::from(
                "VCE job request fingerprint must be an exact SHA-256 identity",
            ));
        }
        let valid_contract_fingerprint = if self.kind == VceJobKind::Compact {
            vulcan_context_engine::is_tagged_sha256_identity(&self.contract_fingerprint)
        } else {
            vulcan_context_engine::is_sha256_identity(&self.contract_fingerprint)
        };
        if !valid_contract_fingerprint {
            return Err(String::from(
                "VCE job contract fingerprint does not match its operation kind",
            ));
        }
        if self
            .retry_of_job_id
            .as_ref()
            .is_some_and(|id| id.trim().is_empty() || id.len() > 512 || id == &self.job_id)
        {
            return Err(String::from(
                "VCE retry origin must identify a different bounded job",
            ));
        }
        if self.sequence > i64::MAX as u64
            || self.created_at_unix_ms > i64::MAX as u64
            || self.updated_at_unix_ms > i64::MAX as u64
            || self.updated_at_unix_ms < self.created_at_unix_ms
            || self
                .source_checkpoint_version
                .is_some_and(|version| version == 0 || version > i64::MAX as u64)
            || self
                .result_checkpoint_version
                .is_some_and(|version| version == 0 || version > i64::MAX as u64)
        {
            return Err(String::from(
                "VCE job receipt has an invalid timestamp or exceeds the storage integer range",
            ));
        }
        if self.error.as_ref().is_some_and(|error| error.len() > 8192) {
            return Err(String::from("VCE job diagnostic exceeds its bound"));
        }
        if self.followup_requested && self.kind != VceJobKind::Synchronize {
            return Err(String::from(
                "Only VCE synchronization jobs can coalesce a follow-up",
            ));
        }
        match (self.kind, &self.compaction) {
            (VceJobKind::Compact, Some(compaction)) => {
                if self.schema_version != VCE_JOB_RECEIPT_SCHEMA_VERSION
                    || self.source_checkpoint_version.is_none()
                    || self.replacement.is_some()
                    || self.followup_requested
                {
                    return Err(String::from(
                        "VCE compaction job has an incompatible receipt or checkpoint contract",
                    ));
                }
                compaction.validate()?;
            }
            (VceJobKind::Compact, None) => {
                return Err(String::from("VCE compaction job omitted its table receipt"));
            }
            (_, Some(_)) => {
                return Err(String::from(
                    "Non-compaction VCE job included physical table state",
                ));
            }
            (_, None) => {}
        }
        if let Some(replacement) = &self.replacement {
            replacement.target.validate()?;
            // space is the single immutable binding identity; duplicated host metadata must agree exactly.
            // space 为唯一不可变绑定身份；重复宿主元数据必须精确一致。
            let space = &replacement.target.binding.embedding_space;
            vulcan_context_engine::VceIndexBinding::new(
                replacement.target.binding.namespace_id.clone(),
                replacement.target.binding.checkpoint_chain_id.clone(),
                replacement.target.binding.schema_version,
                space.clone(),
            )
            .map_err(|error| format!("Validate VCE replacement binding: {error}"))?;
            if self.kind != VceJobKind::Rebuild
                || self.retry_of_job_id.is_some()
                || replacement.predecessor.job_id != self.job_id
                || replacement.predecessor.namespace_id.trim().is_empty()
                || replacement.predecessor.checkpoint_version == 0
                || replacement.predecessor.checkpoint_version > i64::MAX as u64
                || self.source_checkpoint_version
                    != Some(replacement.predecessor.checkpoint_version)
                || replacement.target.project_id != self.project_id
                || replacement.target.config_revision != self.admitted_settings_revision
                || replacement.target.binding.namespace_id == replacement.predecessor.namespace_id
                || space.vector_space_id != replacement.target.vector_space_id
                || space.physical_fingerprint.as_deref()
                    != Some(replacement.target.physical_fingerprint.as_str())
                || space.dimension != replacement.target.dimension as usize
                || space.preprocessing_version != replacement.target.preprocessing_version
                || space.chunker_version != replacement.target.chunker_version
            {
                return Err(String::from(
                    "VCE replacement receipt has inconsistent project, predecessor, or target identity",
                ));
            }
            // Bound recovery metadata independently of source size; receipts never carry source or model secrets.
            // 独立于源码大小限制恢复元数据；回执绝不携带源码或模型密钥。
            if serde_json::to_vec(replacement)
                .map_err(|error| format!("Encode VCE replacement identity: {error}"))?
                .len()
                > 65_536
            {
                return Err(String::from(
                    "VCE replacement identity exceeds its metadata bound",
                ));
            }
        }
        if self.completed_update.as_ref().is_some_and(|update| {
            update.checkpoint_version == 0
                || update.at_unix_ms < self.created_at_unix_ms
                || update.at_unix_ms > self.updated_at_unix_ms
                || (self.state == VceJobState::Succeeded
                    && self.result_checkpoint_version != Some(update.checkpoint_version))
        }) {
            return Err(String::from(
                "VCE completed batch has invalid checkpoint or time",
            ));
        }
        if (self.state == VceJobState::Succeeded
            && (self.result_checkpoint_version.is_none()
                || (self.kind != VceJobKind::Compact && self.completed_update.is_none())))
            || (self.state == VceJobState::Failed
                && self
                    .error
                    .as_ref()
                    .is_none_or(|error| error.trim().is_empty()))
            || (self.state != VceJobState::Succeeded && self.result_checkpoint_version.is_some())
        {
            return Err(String::from(
                "VCE job terminal receipt does not match its result state",
            ));
        }
        if self.kind == VceJobKind::Compact
            && (self.completed_update.is_some()
                || (self.state == VceJobState::Succeeded
                    && (self.result_checkpoint_version != self.source_checkpoint_version
                        || self.compaction.as_ref().is_none_or(|compaction| {
                            compaction
                                .tables
                                .iter()
                                .any(|table| table.state != VceCompactionTableState::Succeeded)
                        }))))
        {
            return Err(String::from(
                "VCE compaction success must preserve its checkpoint and complete every table",
            ));
        }
        if let Some(progress) = &self.progress
            && (progress
                .total
                .is_some_and(|total| progress.completed > total)
                || progress
                    .overall_total
                    .is_some_and(|total| progress.overall_completed > total)
                || progress.operation_id.len() > 2048
                || progress
                    .current_path
                    .as_ref()
                    .is_some_and(|path| path.len() > 4096))
        {
            return Err(String::from(
                "VCE job progress contains invalid counters or oversized metadata",
            ));
        }
        Ok(())
    }
}

/// Validate a job lifecycle edge with the compaction-only atomic cancellation exception.
/// 校验作业生命周期边，并处理仅适用于整理作业的原子取消例外。
///
/// `previous` is the acknowledged receipt and `next` is the proposed successor. Returns true for
/// ordinary lifecycle edges or for a direct running-to-cancelled compaction settlement whose every
/// table is already terminal in the same compare-and-swap.
/// `previous` 是已确认回执，`next` 是拟议后继。普通生命周期边返回 true；若整理作业在同一次
/// 比较交换中把全部表确定为终态，也允许从运行中直接进入已取消。
fn job_state_transition_permitted(previous: &VceStoredJob, next: &VceStoredJob) -> bool {
    previous.state.permits(next.state)
        || (previous.kind == VceJobKind::Compact
            && previous.state == VceJobState::Running
            && next.state == VceJobState::Cancelled
            && next.compaction.as_ref().is_some_and(|compaction| {
                compaction
                    .tables
                    .iter()
                    .all(|table| table.state.is_terminal())
            }))
}

/// One exact physical project's durable job repository supplied by the host factory.
/// 由宿主工厂提供的精确物理项目持久作业仓库。
#[derive(Clone)]
pub struct VceProjectJobStore {
    /// Host-owned storage bindings with a counted lifetime lease.
    /// 带计数生命周期租约的宿主持有存储绑定。
    data_plane: VceDataPlaneHandle,
    /// Stable project owner; callers cannot select an arbitrary namespace or path.
    /// 稳定项目所有者；调用方不能选择任意命名空间或路径。
    project_id: String,
}

/// One storage row containing a versioned receipt, never a guessed union of old fields.
/// 包含版本化回执的存储行，绝不是猜测的旧字段并集。
#[derive(Deserialize)]
struct JobRow {
    /// Indexed project ownership must agree with serialized domain metadata.
    /// 索引项目归属必须与序列化领域元数据一致。
    project_id: String,
    /// Exact stored primary-key identity.
    /// 精确存储主键身份。
    job_id: String,
    /// Indexed host execution owner.
    /// 已建立索引的宿主执行所有者。
    owner_run_id: String,
    /// Indexed replay identity.
    /// 已建立索引的重放身份。
    idempotency_key: String,
    /// Indexed command fingerprint.
    /// 已建立索引的命令指纹。
    request_fingerprint: String,
    /// Compare-and-swap sequence stored outside the receipt JSON.
    /// 存储在回执 JSON 外的比较交换序号。
    sequence: u64,
    /// Indexed original admission timestamp.
    /// 已建立索引的原始准入时间戳。
    created_at_unix_ms: u64,
    /// Indexed latest receipt timestamp.
    /// 已建立索引的最近回执时间戳。
    updated_at_unix_ms: u64,
    /// Exact serialized receipt checked after decoding.
    /// 解码后进行检查的精确序列化回执。
    snapshot_json: String,
}

impl VceProjectJobStore {
    /// Bind this repository to exact host-owned project storage.
    /// 将此仓库绑定到精确宿主持有项目存储。
    ///
    /// `data_plane` and `project_id` come from the factory. Returns a lightweight leased repository.
    /// `data_plane` 与 `project_id` 来自工厂。返回轻量租约仓库。
    pub(crate) fn new(data_plane: VceDataPlaneHandle, project_id: String) -> Self {
        Self {
            data_plane,
            project_id,
        }
    }

    /// Execute one bounded project-storage script solely for deterministic downstream fault tests.
    /// 仅为下游确定性故障测试执行一条有界项目存储脚本。
    ///
    /// `sql` is test-owned schema instrumentation and never accepts user input. Returns the real
    /// storage result through the same process-wide SQLite mutation gate as production writes.
    /// `sql` 是测试持有的结构插桩，绝不接收用户输入。通过与生产写入相同的进程级 SQLite
    /// 变更闸门返回真实存储结果。
    #[cfg(any(test, feature = "test-support"))]
    pub async fn execute_test_storage_script(&self, sql: &str) -> Result<(), String> {
        if sql.trim().is_empty() || sql.len() > 65_536 {
            return Err(String::from(
                "VCE test storage script is empty or exceeds its bound",
            ));
        }
        self.data_plane
            .execute(sql, Vec::new(), "Execute VCE test storage script")
            .await
    }

    /// Register a queued receipt before returning admission to the user; replay an identical key.
    /// 在向用户返回准入前登记排队回执；相同键重放相同结果。
    ///
    /// `job` is a fresh queued receipt. Returns the persisted receipt or a conflicting-key error.
    /// `job` 是全新排队回执。返回持久回执或冲突键错误。
    pub async fn register(&self, job: &VceStoredJob) -> Result<VceStoredJob, String> {
        self.validate_owner(job)?;
        if job.state != VceJobState::Queued || job.sequence != 1 {
            return Err(String::from(
                "VCE job admission requires a fresh queued receipt",
            ));
        }
        if let Some(existing) = self
            .find_submission(&job.idempotency_key, &job.request_fingerprint)
            .await?
        {
            return Ok(existing);
        }
        self.data_plane.execute("CREATE TABLE IF NOT EXISTS vce_host_jobs(project_id TEXT NOT NULL, job_id TEXT NOT NULL, owner_run_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, request_fingerprint TEXT NOT NULL, sequence INTEGER NOT NULL CHECK(sequence > 0), snapshot_json TEXT NOT NULL, created_at_unix_ms INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL, PRIMARY KEY(project_id, job_id), UNIQUE(project_id, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_vce_host_jobs_recent ON vce_host_jobs(project_id, updated_at_unix_ms DESC, job_id);", Vec::new(), "Create VCE job receipt table").await?;
        // serialized is bounded and never contains model credentials or code bodies.
        // serialized 有界且绝不包含模型凭据或代码正文。
        let serialized = serde_json::to_string(job)
            .map_err(|error| format!("Encode VCE job receipt: {error}"))?;
        self.data_plane.execute("INSERT INTO vce_host_jobs(project_id, job_id, owner_run_id, idempotency_key, request_fingerprint, sequence, snapshot_json, created_at_unix_ms, updated_at_unix_ms) VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?) ON CONFLICT(project_id, idempotency_key) DO NOTHING;", vec![sqlite_text(&job.project_id), sqlite_text(&job.job_id), sqlite_text(&job.owner_run_id), sqlite_text(&job.idempotency_key), sqlite_text(&job.request_fingerprint), sqlite_text(serialized), SqliteValue::Integer(job.created_at_unix_ms as i64), SqliteValue::Integer(job.updated_at_unix_ms as i64)], "Register VCE job receipt").await?;
        // persisted is authoritative for idempotency, including a replay whose original job finished.
        // persisted 是幂等性的权威值，包括原始作业已经完成的重放。
        let persisted = self
            .find_by_idempotency_key(&job.idempotency_key)
            .await?
            .ok_or_else(|| String::from("VCE admitted job receipt is missing"))?;
        if persisted.request_fingerprint != job.request_fingerprint {
            return Err(String::from(
                "VCE idempotency key was reused for a different command",
            ));
        }
        Ok(persisted)
    }

    /// Read one stable job identity without creating any table or project storage.
    /// 读取一个稳定作业身份，不创建任何表或项目存储。
    ///
    /// `job_id` is the exact displayed identity. Returns a validated receipt or absence.
    /// `job_id` 是精确显示身份。返回已校验回执或空值。
    pub async fn get(&self, job_id: &str) -> Result<Option<VceStoredJob>, String> {
        if !self.table_exists().await? {
            return Ok(None);
        }
        self.read_one(
            "SELECT project_id, job_id, owner_run_id, idempotency_key, request_fingerprint, sequence, created_at_unix_ms, updated_at_unix_ms, snapshot_json FROM vce_host_jobs WHERE project_id = ? AND job_id = ?;",
            job_id,
        )
        .await
    }

    /// Re-read one receipt after a transport failure without repeating an ambiguous write.
    /// 传输失败后重新读取一次回执，不重复执行结果未知的写入。
    ///
    /// `job_id` selects one project-owned receipt; returns the latest validated value or absence.
    /// `job_id` 选择项目所有的一个回执；返回最新已校验值或明确不存在。
    pub async fn get_after_storage_retry(
        &self,
        job_id: &str,
    ) -> Result<Option<VceStoredJob>, String> {
        for attempt in 0..=VCE_RECEIPT_CONFIRM_RETRIES {
            match self.get(job_id).await {
                Ok(receipt) => return Ok(receipt),
                Err(error)
                    if is_retryable_receipt_storage_error(&error)
                        && attempt < VCE_RECEIPT_CONFIRM_RETRIES =>
                {
                    tokio::time::sleep(receipt_retry_delay(attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("bounded VCE receipt read retry exhausted without a result")
    }

    /// Convert the sole unfinished receipt into an explicit interruption after its host has stopped.
    /// 宿主停止后，将唯一未完成回执转换为明确的中断终态。
    ///
    /// `diagnostic` explains why the previous owner ended; the caller must already hold the
    /// project writer lease, so a live cooperating worker cannot be silently adopted.
    /// `diagnostic` 说明前一个所有者为何结束；调用方必须已经持有项目写入租约，
    /// 因此活动的协作工作线程不会被静默接管。
    /// Returns the acknowledged interrupted receipt, or `None` when no unfinished receipt exists.
    /// 返回已确认的中断回执；不存在未完成回执时返回 `None`。
    pub async fn interrupt_unfinished(
        &self,
        diagnostic: impl Into<String>,
    ) -> Result<Option<VceStoredJob>, String> {
        let Some(previous) = self.unfinished().await? else {
            return Ok(None);
        };
        let now = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| format!("Read VCE interruption timestamp: {error}"))?
                .as_millis(),
        )
        .map_err(|_| String::from("VCE interruption timestamp exceeds storage range"))?;
        let mut next = previous.clone();
        next.state = VceJobState::Interrupted;
        next.followup_requested = false;
        next.error = Some(diagnostic.into().chars().take(8_192).collect());
        next.sequence = previous
            .sequence
            .checked_add(1)
            .ok_or_else(|| String::from("VCE job interruption sequence overflow"))?;
        next.updated_at_unix_ms = now.max(previous.updated_at_unix_ms);
        self.update(&previous, &next).await.map(Some)
    }

    /// Settle a receipt whose executor has ended, using exact committed refresh evidence.
    /// 使用精确已提交刷新证据收敛执行者已经结束的回执。
    /// `expected_job_id` fences recovery to the receipt originally selected by the caller.
    /// `expected_job_id` 将恢复限制在调用方最初选定的回执。
    /// The caller must hold the exclusive project writer lease and exclude local admission.
    /// 调用方必须持有项目独占写入租约并排斥本地准入。
    /// Returns the terminal receipt without scheduling source work or deleting stored content.
    /// 返回终态回执，不安排源码处理、不删除存储内容。
    pub(crate) async fn reconcile_ended_job(
        &self,
        expected_job_id: &str,
    ) -> Result<Option<VceStoredJob>, String> {
        let Some(mut previous) = self.unfinished().await? else {
            return Ok(None);
        };
        // The selected job may have ended while a newer one was admitted; never settle that successor.
        // 选定作业可能已结束并准入了更新作业；绝不收敛该后继。
        if previous.job_id != expected_job_id {
            return Ok(None);
        }
        // Only the final, non-coalesced publication phase identifies the whole job's last refresh.
        // 只有不存在待合并后续任务的最终发布阶段才能标识整个作业的最后一次刷新。
        let refresh = previous.progress.as_ref().filter(|progress| {
            previous.state == VceJobState::Running
                && !previous.followup_requested
                && matches!(
                    progress.phase,
                    vulcan_context_engine::VceRefreshPhase::Committing
                        | vulcan_context_engine::VceRefreshPhase::Complete
                )
                && (progress.operation_id == previous.job_id
                    || progress
                        .operation_id
                        .strip_prefix(&format!("{}:batch:", previous.job_id))
                        .is_some_and(|suffix| uuid::Uuid::parse_str(suffix).is_ok()))
        });
        if let Some(refresh) = refresh {
            // Join all three atomic publication facts; a completed storage-operation counter is insufficient.
            // 联接三个原子发布事实；存储操作完成计数不足以证明发布。
            let publications: Vec<VceCompletedJobUpdate> = self.data_plane.query_rows(
                "SELECT c.checkpoint_version, c.committed_at_unix_ms AS at_unix_ms FROM vce_read_checkpoints c JOIN vce_project_bindings b ON b.namespace_id = c.namespace_id JOIN vce_generations g ON g.namespace_id = c.namespace_id AND g.refresh_id = c.refresh_id AND g.generation = c.generation AND g.state = 'committed' JOIN vce_commit_receipts r ON r.namespace_id = g.namespace_id AND r.operation_id = g.operation_id WHERE b.project_id = ? AND c.refresh_id = ? AND g.operation_id = ? AND (? IS NULL OR c.namespace_id = ?) LIMIT 2;",
                vec![sqlite_text(&self.project_id), sqlite_text(&refresh.operation_id), sqlite_text(format!("{}:commit", refresh.operation_id)), previous.replacement.as_ref().map_or(SqliteValue::Null, |replacement| sqlite_text(&replacement.target.binding.namespace_id)), previous.replacement.as_ref().map_or(SqliteValue::Null, |replacement| sqlite_text(&replacement.target.binding.namespace_id))],
            ).await?;
            if publications.len() > 1 {
                return Err(String::from(
                    "VCE ended job has ambiguous committed refresh evidence",
                ));
            }
            if let Some(publication) = publications.into_iter().next() {
                let mut accounted = previous.clone();
                // Preserve acknowledged completion time when replaying an already accounted publication.
                // 重放已经计账的发布时保留已确认完成时间。
                if accounted.completed_update.as_ref().is_none_or(|update| {
                    update.checkpoint_version != publication.checkpoint_version
                }) {
                    accounted.completed_update = Some(publication.clone());
                    accounted.sequence = accounted
                        .sequence
                        .checked_add(1)
                        .ok_or("VCE job sequence overflow")?;
                    accounted.updated_at_unix_ms =
                        accounted.updated_at_unix_ms.max(publication.at_unix_ms);
                    previous = self.update(&previous, &accounted).await?;
                }
                let mut finished = previous.clone();
                finished.state = VceJobState::Succeeded;
                finished.result_checkpoint_version = Some(publication.checkpoint_version);
                finished.error = None;
                finished.sequence = finished
                    .sequence
                    .checked_add(1)
                    .ok_or("VCE job sequence overflow")?;
                return self.update(&previous, &finished).await.map(Some);
            }
        }
        self.interrupt_unfinished("VCE executor ended before a complete job publication could be confirmed; stored index data has been preserved").await
    }

    /// Find a previously admitted request without rerunning its expensive work.
    /// 查找先前准入请求，不重新执行其高成本工作。
    ///
    /// `key` is the caller's exact idempotency key. Returns the original validated receipt or absence.
    /// `key` 是调用方精确幂等键。返回原始已校验回执或空值。
    pub async fn find_by_idempotency_key(&self, key: &str) -> Result<Option<VceStoredJob>, String> {
        if !self.table_exists().await? {
            return Ok(None);
        }
        self.read_one(
            "SELECT project_id, job_id, owner_run_id, idempotency_key, request_fingerprint, sequence, created_at_unix_ms, updated_at_unix_ms, snapshot_json FROM vce_host_jobs WHERE project_id = ? AND idempotency_key = ?;",
            key,
        )
        .await
    }

    /// Resolve a primary or coalesced submission without creating storage or executing work.
    /// 解析主要或合并提交，不创建存储或执行工作。
    /// `key` and `fingerprint` identify the exact command. Returns its original current receipt.
    /// `key` 与 `fingerprint` 标识精确命令。返回其原始作业的当前回执。
    pub async fn find_submission(
        &self,
        key: &str,
        fingerprint: &str,
    ) -> Result<Option<VceStoredJob>, String> {
        if let Some(job) = self.find_by_idempotency_key(key).await? {
            if job.request_fingerprint != fingerprint {
                return Err(String::from(
                    "VCE idempotency key was reused for a different command",
                ));
            }
            return Ok(Some(job));
        }
        // Absence of the alias table means this database has never had a job submitted, so there is no alias
        // to find. This is a read-only lookup for that ordinary state — it is not a migration path, and it
        // deliberately does not create the table as a side effect of a read.
        // 别名表不存在意味着该数据库从未提交过作业，因此没有别名可找。这是一次针对该正常状态的只读查找——它不是
        // 迁移路径，也刻意不因一次读取而顺带创建该表。
        let tables: Vec<CountRow> = self.data_plane.query_rows("SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'vce_host_job_requests';", Vec::new()).await?;
        if !tables.first().is_some_and(|row| row.count == 1) {
            return Ok(None);
        }
        // Alias lookup is bounded by the exact project and unique caller key.
        // 别名读取由精确项目与调用方唯一键限定。
        let rows: Vec<CoalescedRequestRow> = self.data_plane.query_rows("SELECT job_id, request_fingerprint FROM vce_host_job_requests WHERE project_id = ? AND idempotency_key = ?;", vec![sqlite_text(&self.project_id), sqlite_text(key)]).await?;
        if rows.len() > 1 {
            return Err(String::from("VCE submission has duplicate ownership"));
        }
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        if row.request_fingerprint != fingerprint {
            return Err(String::from(
                "VCE idempotency key was reused for a different command",
            ));
        }
        // A persisted alias must name a real validated receipt, never a guessed active owner.
        // 持久别名必须指向真实已校验回执，绝不指向猜测的活动所有者。
        let job = self
            .get(&row.job_id)
            .await?
            .ok_or_else(|| String::from("VCE coalesced submission lost its job receipt"))?;
        Ok(Some(job))
    }

    /// Atomically acknowledge one distinct button request and at most one later synchronization.
    /// 原子确认一个不同按钮请求与至多一次后续同步。
    /// `previous` is locked by the host, `key`/`fingerprint` identify this request, and `timestamp` is admission time.
    /// `previous` 由宿主锁定，`key`／`fingerprint` 标识此请求，`timestamp` 为准入时间。
    /// `followup` requests a later batch only for running synchronization. Returns the durable successor.
    /// `followup` 仅对正在运行的同步请求后续批次。返回持久后继。
    pub async fn coalesce(
        &self,
        previous: &VceStoredJob,
        key: &str,
        fingerprint: &str,
        timestamp: u64,
        followup: bool,
    ) -> Result<VceStoredJob, String> {
        self.validate_owner(previous)?;
        if key.trim().is_empty()
            || key.len() > 512
            || !vulcan_context_engine::is_sha256_identity(fingerprint)
            || !matches!(previous.state, VceJobState::Queued | VceJobState::Running)
            || (followup
                && (previous.kind != VceJobKind::Synchronize
                    || previous.state != VceJobState::Running))
        {
            return Err(String::from(
                "VCE coalesced submission has invalid identity or state",
            ));
        }
        // Each accepted key has an immutable replay mapping; already accepted requests do not add a batch.
        // 每个已受理键具有不可变重放映射；已受理请求不增加批次。
        if let Some(existing) = self.find_submission(key, fingerprint).await? {
            return Ok(existing);
        }
        // The successor changes only the bounded pending flag and monotonic receipt envelope.
        // 后继仅改变有界待处理标记与单调回执封装。
        let mut next = previous.clone();
        next.sequence = previous
            .sequence
            .checked_add(1)
            .ok_or_else(|| String::from("VCE receipt sequence overflow"))?;
        next.updated_at_unix_ms = timestamp.max(previous.updated_at_unix_ms);
        next.followup_requested |= followup;
        self.validate_owner(&next)?;
        // Exact project scope, escaped as a value rather than an SQL identifier.
        // 精确项目范围，转义为值而非 SQL 标识符。
        let project = sqlite_string_literal(&self.project_id);
        // Stable job identity shared by all accepted aliases.
        // 全部已受理别名共享的稳定作业身份。
        let job = sqlite_string_literal(&previous.job_id);
        // Exact admitting host run required by the compare-and-swap.
        // 比较交换要求的精确准入宿主运行。
        let owner = sqlite_string_literal(&previous.owner_run_id);
        // Caller identity may contain quotes and must remain an escaped value.
        // 调用方身份可能包含引号，必须保持为已转义值。
        let key = sqlite_string_literal(key);
        // Immutable submitted-command hash, separate from the head job's command.
        // 不可变提交命令哈希，独立于首作业命令。
        let fingerprint = sqlite_string_literal(fingerprint);
        // Full prior snapshot prevents a same-sequence envelope from masking another persisted value.
        // 完整先前快照防止相同序号封装掩盖另一持久值。
        let old_json = sqlite_string_literal(
            &serde_json::to_string(previous).map_err(|error| error.to_string())?,
        );
        // The acknowledged snapshot and alias commit in the same database transaction.
        // 已确认快照与别名在相同数据库事务中提交。
        let new_json = sqlite_string_literal(
            &serde_json::to_string(&next).map_err(|error| error.to_string())?,
        );
        // NOT NULL plus OR ROLLBACK makes a stale receipt reject the entire request, not create a false replay alias.
        // 非空约束与回滚冲突策略使过期回执拒绝整个请求，而非生成虚假的重放别名。
        let sql = format!("BEGIN IMMEDIATE;
CREATE TABLE IF NOT EXISTS vce_host_job_requests(project_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, job_id TEXT NOT NULL, request_fingerprint TEXT NOT NULL, PRIMARY KEY(project_id, idempotency_key), FOREIGN KEY(project_id, job_id) REFERENCES vce_host_jobs(project_id, job_id));
INSERT OR ROLLBACK INTO vce_host_job_requests(project_id, idempotency_key, job_id, request_fingerprint)
VALUES ('{project}', '{key}', CASE WHEN EXISTS(SELECT 1 FROM vce_host_jobs WHERE project_id = '{project}' AND job_id = '{job}' AND owner_run_id = '{owner}' AND sequence = {old_sequence} AND snapshot_json = '{old_json}') AND NOT EXISTS(SELECT 1 FROM vce_host_jobs WHERE project_id = '{project}' AND idempotency_key = '{key}') THEN '{job}' ELSE NULL END, '{fingerprint}');
UPDATE vce_host_jobs SET sequence = {new_sequence}, snapshot_json = '{new_json}', updated_at_unix_ms = {updated} WHERE project_id = '{project}' AND job_id = '{job}' AND sequence = {old_sequence};
COMMIT;", old_sequence = previous.sequence, new_sequence = next.sequence, updated = next.updated_at_unix_ms);
        self.data_plane
            .execute(&sql, Vec::new(), "Coalesce VCE manual submission")
            .await?;
        // Confirm the committed envelope before the host publishes any acknowledgement.
        // 在宿主发布任何确认之前核对已提交封装。
        let stored = self
            .get(&next.job_id)
            .await?
            .ok_or_else(|| String::from("VCE coalesced receipt is missing"))?;
        if stored != next {
            return Err(String::from(
                "VCE coalesced receipt changed during publication",
            ));
        }
        Ok(stored)
    }

    /// Read the latest durable receipt without activating source work or creating schema.
    /// 读取最新持久回执，不激活源码处理或创建结构。
    ///
    /// Takes no parameters. Returns the newest admitted job, including a terminal result.
    /// 无参数。返回最近准入作业，包括终态结果。
    pub async fn latest(&self) -> Result<Option<VceStoredJob>, String> {
        if !self.table_exists().await? {
            return Ok(None);
        }
        self.read_receipt("SELECT project_id, job_id, owner_run_id, idempotency_key, request_fingerprint, sequence, created_at_unix_ms, updated_at_unix_ms, snapshot_json FROM vce_host_jobs WHERE project_id = ? ORDER BY created_at_unix_ms DESC, job_id DESC LIMIT 1;", vec![sqlite_text(&self.project_id)]).await
    }

    /// Read the latest successful content batch, even if a later batch in that job failed.
    /// 读取最近成功内容批次，即使该作业的后续批次失败也保留。
    /// Takes no parameters. Returns the persisted receipt or absence without creating tables.
    /// 无参数。返回持久回执或空值，不创建表。
    pub async fn latest_successful_update(&self) -> Result<Option<VceStoredJob>, String> {
        if !self.table_exists().await? {
            return Ok(None);
        }
        // The only freshness authority is a fully published batch, including one retained after failure.
        // 唯一新鲜度权威是完全发布的批次，包括失败后保留的成功批次。
        self.read_receipt("SELECT project_id, job_id, owner_run_id, idempotency_key, request_fingerprint, sequence, created_at_unix_ms, updated_at_unix_ms, snapshot_json FROM vce_host_jobs WHERE project_id = ? AND json_extract(snapshot_json, '$.completed_update') IS NOT NULL ORDER BY json_extract(snapshot_json, '$.completed_update.at_unix_ms') DESC, job_id DESC LIMIT 1;", vec![sqlite_text(&self.project_id)]).await
    }

    /// Find the sole unfinished owner before an explicit new job can be admitted.
    /// 在准入明确新作业前查找唯一未结束所有者。
    ///
    /// Takes no parameters. Returns the unfinished receipt; duplicate writers are an explicit error.
    /// 无参数。返回未结束回执；重复写入者属于明确错误。
    pub async fn unfinished(&self) -> Result<Option<VceStoredJob>, String> {
        if !self.table_exists().await? {
            return Ok(None);
        }
        self.read_receipt("SELECT project_id, job_id, owner_run_id, idempotency_key, request_fingerprint, sequence, created_at_unix_ms, updated_at_unix_ms, snapshot_json FROM vce_host_jobs WHERE project_id = ? AND json_extract(snapshot_json, '$.state') IN ('queued', 'running', 'cancelling') LIMIT 2;", vec![sqlite_text(&self.project_id)]).await
    }

    /// Persist a monotonic update only if the exact prior receipt still owns the job.
    /// 仅在精确先前回执仍持有作业时持久化单调更新。
    ///
    /// `previous` is the last acknowledged receipt; `next` advances its sequence by one.
    /// Returns the durable successor, rejecting stale ownership, terminal rewrites, and identity drift.
    /// `previous` 是最近确认回执；`next` 将其序号增加一。
    /// 返回持久后继，拒绝过期归属、终态改写与身份漂移。
    pub async fn update(
        &self,
        previous: &VceStoredJob,
        next: &VceStoredJob,
    ) -> Result<VceStoredJob, String> {
        self.validate_owner(previous)?;
        self.validate_owner(next)?;
        if let (Some(previous_compaction), Some(next_compaction)) =
            (&previous.compaction, &next.compaction)
        {
            previous_compaction.validate_transition(next_compaction)?;
        }
        if !job_state_transition_permitted(previous, next)
            || next.updated_at_unix_ms < previous.updated_at_unix_ms
            || (previous.completed_update != next.completed_update
                && next.state != VceJobState::Running)
            || (previous.state == VceJobState::Running
                && next.state == VceJobState::Queued
                && (!previous.followup_requested
                    || next.followup_requested
                    || next.progress.is_some()
                    || next.completed_update.is_none()
                    || next.completed_update != previous.completed_update))
            || (previous.completed_update != next.completed_update
                && previous.completed_update.is_some()
                && next.completed_update.as_ref().is_none_or(|update| {
                    previous.completed_update.as_ref().is_some_and(|old| {
                        update.at_unix_ms < old.at_unix_ms
                            || update.checkpoint_version < old.checkpoint_version
                    })
                }))
            || next.sequence
                != previous
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| String::from("VCE job sequence overflow"))?
            || previous.job_id != next.job_id
            || previous.owner_run_id != next.owner_run_id
            || previous.idempotency_key != next.idempotency_key
            || previous.retry_of_job_id != next.retry_of_job_id
            || previous.request_fingerprint != next.request_fingerprint
            || previous.contract_fingerprint != next.contract_fingerprint
            || previous.kind != next.kind
            || previous.created_at_unix_ms != next.created_at_unix_ms
            || previous.admitted_settings_revision != next.admitted_settings_revision
            || previous.source_checkpoint_version != next.source_checkpoint_version
            || previous.replacement != next.replacement
        {
            return Err(String::from(
                "VCE job update violates immutable ownership or terminal state",
            ));
        }
        // current guards against stale same-sequence receipts before the database compare-and-swap.
        // current 在数据库比较交换前防止相同序号的过期回执。
        let current = self
            .get_after_storage_retry(&previous.job_id)
            .await?
            .ok_or_else(|| String::from("VCE job to update does not exist"))?;
        if current != *previous {
            return Err(String::from("VCE job receipt changed before update"));
        }
        let serialized = serde_json::to_string(next)
            .map_err(|error| format!("Encode VCE job update: {error}"))?;
        let update_params = vec![
            SqliteValue::Integer(next.sequence as i64),
            sqlite_text(serialized),
            SqliteValue::Integer(next.updated_at_unix_ms as i64),
            sqlite_text(&self.project_id),
            sqlite_text(&previous.job_id),
            SqliteValue::Integer(previous.sequence as i64),
            sqlite_text(&previous.owner_run_id),
        ];
        for attempt in 0..=VCE_RECEIPT_CONFIRM_RETRIES {
            match self
                .data_plane
                .execute(
                    "UPDATE vce_host_jobs SET sequence = ?, snapshot_json = ?, updated_at_unix_ms = ? WHERE project_id = ? AND job_id = ? AND sequence = ? AND owner_run_id = ?;",
                    update_params.clone(),
                    "Update VCE job receipt",
                )
                .await
            {
                Ok(()) => break,
                Err(error) if is_retryable_receipt_storage_error(&error) => {
                    // The write may already have committed; reconcile before attempting it again.
                    // 写入可能已经提交，重试写入前先读取并收敛到已提交结果。
                    match self.get_after_storage_retry(&next.job_id).await {
                        Ok(Some(receipt)) if receipt == *next => return Ok(receipt),
                        Ok(Some(receipt))
                            if receipt == *previous && attempt < VCE_RECEIPT_CONFIRM_RETRIES =>
                        {
                            tokio::time::sleep(receipt_retry_delay(attempt)).await;
                        }
                        Ok(Some(receipt)) if receipt == *previous => return Err(error),
                        Ok(Some(_)) => {
                            return Err(String::from("VCE job receipt changed during update"));
                        }
                        Ok(None) => return Err(String::from("VCE job to update does not exist")),
                        Err(read_error)
                            if is_retryable_receipt_storage_error(&read_error)
                                && attempt < VCE_RECEIPT_CONFIRM_RETRIES =>
                        {
                            tokio::time::sleep(receipt_retry_delay(attempt)).await;
                        }
                        Err(read_error) => return Err(read_error),
                    }
                }
                Err(error) => return Err(error),
            }
        }
        let persisted = self
            .get_after_storage_retry(&next.job_id)
            .await?
            .ok_or_else(|| String::from("VCE updated job receipt is missing"))?;
        if persisted != *next {
            return Err(String::from("VCE job receipt changed during update"));
        }
        Ok(persisted)
    }

    /// Check a receipt belongs to this physical project and satisfies the current format.
    /// 检查回执属于此物理项目并满足当前格式。
    ///
    /// `job` is a decoded or proposed receipt. Returns an explicit ownership/format error.
    /// `job` 是已解码或拟议回执。返回明确归属或格式错误。
    fn validate_owner(&self, job: &VceStoredJob) -> Result<(), String> {
        job.validate()?;
        if job.project_id != self.project_id {
            return Err(String::from("VCE job belongs to another project"));
        }
        self.data_plane.validate_project_owner(&job.project_id)
    }

    /// Determine table presence using read-only schema metadata.
    /// 使用只读结构元数据判断表是否存在。
    ///
    /// Takes no parameters. Returns false for an index that has never admitted a host job.
    /// 无参数。从未准入宿主作业的索引返回假。
    async fn table_exists(&self) -> Result<bool, String> {
        let rows: Vec<CountRow> = self.data_plane.query_rows("SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'vce_host_jobs';", Vec::new()).await?;
        Ok(rows.first().is_some_and(|row| row.count == 1))
    }

    /// Decode one exact project-scoped lookup and validate its immutable domain envelope.
    /// 解码一次精确项目作用域查找，并校验不可变领域封装。
    ///
    /// `sql` is one repository-owned query and `key` its exact lookup identity. Returns zero or one receipt.
    /// `sql` 是仓库自有查询，`key` 是其精确查找身份。返回零或一个回执。
    async fn read_one(&self, sql: &str, key: &str) -> Result<Option<VceStoredJob>, String> {
        self.read_receipt(sql, vec![sqlite_text(&self.project_id), sqlite_text(key)])
            .await
    }

    /// Decode a bounded repository-owned receipt query and verify every indexed mirror.
    /// 解码有界仓库自有回执查询，并验证所有索引镜像。
    ///
    /// `sql` and `params` are exact repository-owned statements. Returns zero or one valid receipt.
    /// `sql` 与 `params` 是精确仓库自有语句。返回零或一个有效回执。
    async fn read_receipt(
        &self,
        sql: &str,
        params: Vec<SqliteValue>,
    ) -> Result<Option<VceStoredJob>, String> {
        let rows: Vec<JobRow> = self.data_plane.query_rows(sql, params).await?;
        if rows.len() > 1 {
            return Err(String::from("VCE job query returned duplicate ownership"));
        }
        rows.first()
            .map(|row| {
                let job: VceStoredJob = serde_json::from_str(&row.snapshot_json)
                    .map_err(|error| format!("Decode VCE job receipt: {error}"))?;
                self.validate_owner(&job)?;
                if row.project_id != job.project_id
                    || row.job_id != job.job_id
                    || row.owner_run_id != job.owner_run_id
                    || row.idempotency_key != job.idempotency_key
                    || row.request_fingerprint != job.request_fingerprint
                    || row.sequence != job.sequence
                    || row.created_at_unix_ms != job.created_at_unix_ms
                    || row.updated_at_unix_ms != job.updated_at_unix_ms
                {
                    return Err(String::from(
                        "VCE job receipt disagrees with its indexed ownership envelope",
                    ));
                }
                Ok(job)
            })
            .transpose()
    }
}

/// Exact durable replay alias; each row belongs to the project selected by the repository query.
/// 精确持久重放别名；每行均属于仓库查询选定的项目。
#[derive(Deserialize)]
struct CoalescedRequestRow {
    /// Original job identity, not a newly scheduled writer.
    /// 原始作业身份，不是新调度写入者。
    job_id: String,
    /// Immutable identity of the coalesced button command.
    /// 合并按钮命令的不可变身份。
    request_fingerprint: String,
}

/// Build an isolated queued replacement receipt for storage contract tests without allocating its target.
/// 为存储契约测试构造隔离的排队替换回执，不分配其目标。
/// `target` and `predecessor` are exact fixture identities. Returns metadata only, not registration authority.
/// `target` 与 `predecessor` 为精确夹具身份。仅返回元数据，不构成注册授权。
#[cfg(test)]
pub(super) fn replacement_test_receipt(
    target: VceProjectIndexConfig,
    predecessor: VceRebuildCandidateBase,
) -> VceStoredJob {
    VceStoredJob {
        schema_version: VCE_JOB_RECEIPT_SCHEMA_VERSION,
        project_id: target.project_id.clone(),
        job_id: predecessor.job_id.clone(),
        owner_run_id: String::from("replacement-fixture-owner"),
        idempotency_key: format!("request:{}", predecessor.job_id),
        retry_of_job_id: None,
        request_fingerprint: "a".repeat(64),
        contract_fingerprint: "b".repeat(64),
        kind: VceJobKind::Rebuild,
        state: VceJobState::Queued,
        sequence: 1,
        created_at_unix_ms: 2000,
        updated_at_unix_ms: 2000,
        admitted_settings_revision: target.config_revision,
        source_checkpoint_version: Some(predecessor.checkpoint_version),
        result_checkpoint_version: None,
        replacement: Some(Box::new(VceJobReplacement {
            predecessor,
            target,
        })),
        compaction: None,
        progress: None,
        followup_requested: false,
        completed_update: None,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    /// Build one valid pending physical compaction receipt for lifecycle validation tests.
    /// 为生命周期校验测试构造一份有效的待处理物理整理回执。
    fn compaction_test_receipt() -> VceStoredJob {
        let statistics = VcePhysicalTableStatistics {
            role: vulcan_context_engine::VcePhysicalTableRole::Content,
            table_name: String::from("content_vectors"),
            version: 9,
            total_bytes: 4096,
            row_count: 32,
            index_count: 0,
            fragment_count: 8,
            small_fragment_count: 8,
            fragment_rows: vulcan_context_engine::VcePhysicalFragmentSummary {
                minimum_rows: 4,
                maximum_rows: 4,
                mean_rows: 4,
                p25_rows: 4,
                p50_rows: 4,
                p75_rows: 4,
                p99_rows: 4,
            },
        };
        VceStoredJob {
            schema_version: VCE_JOB_RECEIPT_SCHEMA_VERSION,
            project_id: String::from("compaction-project"),
            job_id: String::from("compaction-job"),
            owner_run_id: String::from("compaction-owner"),
            idempotency_key: String::from("compaction-request"),
            retry_of_job_id: None,
            request_fingerprint: "a".repeat(64),
            contract_fingerprint: format!("sha256:{}", "b".repeat(64)),
            kind: VceJobKind::Compact,
            state: VceJobState::Queued,
            sequence: 1,
            created_at_unix_ms: 1_000,
            updated_at_unix_ms: 1_000,
            admitted_settings_revision: 1,
            source_checkpoint_version: Some(7),
            replacement: None,
            compaction: Some(VceCompactionJobReceipt {
                tables: vec![VceCompactionTableReceipt {
                    action_id: "c".repeat(64),
                    plan: VcePhysicalTablePlan {
                        statistics: statistics.clone(),
                        compaction_required: true,
                        options: vulcan_context_engine::VceCompactionOptions {
                            target_rows_per_fragment: 100_000,
                            max_rows_per_group: 1_024,
                            max_bytes_per_file: 256 * 1024 * 1024,
                            num_threads: 2,
                            batch_size: 8_192,
                            max_source_fragments: 512,
                        },
                    },
                    state: VceCompactionTableState::Pending,
                    round: 0,
                    operation_id: None,
                    operation_expected_version: None,
                    latest_statistics: statistics,
                    metrics: VceCompactionMetrics::default(),
                    error: None,
                }],
            }),
            result_checkpoint_version: None,
            progress: None,
            followup_requested: false,
            completed_update: None,
            error: None,
        }
    }

    // SQLite lock conflicts require receipt reconciliation, not blind duplicate writes.
    // SQLite 锁冲突必须走回执收敛，而不是盲目重复写入。
    #[test]
    fn receipt_reconciliation_recognizes_sqlite_lock_conflicts() {
        assert!(is_retryable_receipt_storage_error(
            "Update VCE job receipt: sqlite execute failed: database is locked"
        ));
        assert!(!is_retryable_receipt_storage_error(
            "storage engine returned an invalid receipt"
        ));
        assert!(!is_retryable_receipt_storage_error(
            "VCE job receipt has invalid ownership"
        ));
    }

    /// Reject a durable receipt whose whole-job progress exceeds its declared denominator.
    /// 拒绝整体作业进度超过声明分母的持久回执。
    #[test]
    fn receipt_rejects_overflowing_overall_progress() {
        let mut receipt = compaction_test_receipt();
        receipt.progress = Some(VceRefreshProgress {
            phase: vulcan_context_engine::VceRefreshPhase::Writing,
            operation_id: String::from("compaction-job:compact:content:1"),
            unit: vulcan_context_engine::VceRefreshUnit::Tables,
            completed: 0,
            total: Some(1),
            overall_completed: 2,
            overall_total: Some(1),
            current_path: None,
        });

        assert!(receipt.validate().is_err());
    }

    /// Reject non-monotonic receipt time and zero or out-of-range checkpoint identities.
    /// 拒绝非单调回执时间以及零值或越界的检查点身份。
    #[test]
    fn receipt_rejects_invalid_durable_time_and_checkpoint_identity() {
        let mut backwards_time = compaction_test_receipt();
        backwards_time.updated_at_unix_ms = backwards_time.created_at_unix_ms - 1;
        assert!(backwards_time.validate().is_err());

        let mut zero_checkpoint = compaction_test_receipt();
        zero_checkpoint.source_checkpoint_version = Some(0);
        assert!(zero_checkpoint.validate().is_err());

        let mut oversized_checkpoint = compaction_test_receipt();
        oversized_checkpoint.source_checkpoint_version = Some(i64::MAX as u64 + 1);
        assert!(oversized_checkpoint.validate().is_err());
    }

    /// Round-trip exact replacement identity and reject inconsistent or unsupported receipt facts.
    /// 往返保存精确替换身份，并拒绝不一致或不支持的回执事实。
    /// Uses only path text and in-memory serialization; returns no value and performs no database or file writes.
    /// 仅使用路径文本及内存序列化；无返回值，不进行数据库或文件写入。
    #[test]
    fn replacement_receipt_requires_one_exact_bounded_target() {
        // root is existing absolute path text only; the fixture never opens an index there.
        // root 仅为已存在的绝对路径文本；夹具绝不在那里打开索引。
        let root = std::env::current_dir()
            .expect("read absolute fixture root")
            .display()
            .to_string();
        // space/config describe an explicit target with mutually consistent duplicated physical facts.
        // space/config 描述明确目标，重复物理事实互相一致。
        let space = vulcan_context_engine::EmbeddingSpace {
            vector_space_id: String::from("target-space"),
            physical_fingerprint: Some(String::from("fixture-producer")),
            dimension: 3,
            preprocessing_version: String::from("preprocess-v1"),
            chunker_version: String::from("chunker-v1"),
        };
        let config = VceProjectIndexConfig {
            project_id: String::from("receipt-project"),
            root_hash: hex::encode(Sha256::digest(root.as_bytes())),
            root_path: root,
            binding: vulcan_context_engine::VceIndexBinding::new(
                "target",
                "target-chain",
                1,
                space.clone(),
            )
            .expect("valid binding"),
            vector_space_id: space.vector_space_id,
            physical_fingerprint: space.physical_fingerprint.expect("explicit producer"),
            preprocessing_version: space.preprocessing_version,
            chunker_version: space.chunker_version,
            dimension: 3,
            config_revision: 1,
        };
        // receipt binds a distinct initialized predecessor to this exact new target and owning job.
        // receipt 将独立且已初始化的前驱绑定到此精确新目标及所属作业。
        let receipt = replacement_test_receipt(
            config,
            VceRebuildCandidateBase {
                namespace_id: String::from("original"),
                checkpoint_version: 4,
                job_id: String::from("replacement-job"),
            },
        );
        receipt.validate().expect("valid replacement receipt");
        let encoded = serde_json::to_vec(&receipt).expect("encode replacement receipt");
        assert_eq!(
            serde_json::from_slice::<VceStoredJob>(&encoded).expect("decode exact current format"),
            receipt
        );
        for case in [
            "old_format",
            "wrong_job",
            "wrong_project",
            "wrong_checkpoint",
            "wrong_kind",
            "retry",
            "same_namespace",
            "dimension",
            "producer",
            "revision",
            "root_hash",
            "preprocessing",
            "oversized",
        ] {
            // invalid changes one fact while preserving the original fixture for the remaining cases.
            // invalid 仅修改一个事实，为其余情况保留原始夹具。
            let mut invalid = receipt.clone();
            let replacement = invalid.replacement.as_mut().expect("replacement exists");
            match case {
                "old_format" => invalid.schema_version = 1,
                "wrong_job" => replacement.predecessor.job_id.push_str("-other"),
                "wrong_project" => replacement.target.project_id.push_str("-other"),
                "wrong_checkpoint" => replacement.predecessor.checkpoint_version += 1,
                "wrong_kind" => invalid.kind = VceJobKind::Synchronize,
                "retry" => invalid.retry_of_job_id = Some(String::from("unconfirmed-old-job")),
                "same_namespace" => {
                    replacement.target.binding.namespace_id =
                        replacement.predecessor.namespace_id.clone()
                }
                "dimension" => replacement.target.dimension = 4,
                "producer" => {
                    replacement
                        .target
                        .binding
                        .embedding_space
                        .physical_fingerprint = None
                }
                "revision" => replacement.target.config_revision += 1,
                "root_hash" => replacement.target.root_hash = "0".repeat(64),
                "preprocessing" => replacement
                    .target
                    .preprocessing_version
                    .push_str("-different"),
                "oversized" => replacement.target.binding.checkpoint_chain_id = "x".repeat(65_536),
                _ => unreachable!("fixture cases are exhaustive"),
            }
            assert!(
                invalid.validate().is_err(),
                "invalid replacement case must reject: {case}"
            );
        }
        // Unknown target fields are not treated as compatibility aliases for another receipt version.
        // 未知目标字段不作为另一回执版本的兼容别名处理。
        let mut unknown = serde_json::to_value(&receipt).expect("encode malformed fixture");
        unknown["replacement"]["target"]["unrecognized_target"] = serde_json::json!(true);
        assert!(serde_json::from_value::<VceStoredJob>(unknown).is_err());
    }

    /// Verify a pre-plan receipt format is refused rather than read as though it were current.
    /// 验证前代回执格式被拒绝，而不是被当作当前格式读取。
    ///
    /// A receipt that decodes is not a receipt this build may act on: the version marker is what separates
    /// "readable JSON" from "a contract this runtime understands". Accepting version 2 would mean acting on a
    /// compaction-free contract that predates the plan, which is the dual-read path the plan forbids.
    /// 能解码的回执并不等于本构建可以据以行动的回执：版本标记正是"可读的 JSON"与"本运行时可理解的契约"之间的界线。
    /// 接受版本 2 意味着依据一份早于本方案、且不含整理信息的契约行动，而这正是方案禁止的双读路径。
    #[test]
    fn a_pre_plan_job_receipt_version_is_refused() {
        let root = std::env::current_dir()
            .expect("read absolute fixture root")
            .display()
            .to_string();
        let space = vulcan_context_engine::EmbeddingSpace {
            vector_space_id: String::from("legacy-space"),
            physical_fingerprint: Some(String::from("legacy-producer")),
            dimension: 3,
            preprocessing_version: String::from("preprocess-v1"),
            chunker_version: String::from("chunker-v1"),
        };
        let mut receipt = replacement_test_receipt(
            VceProjectIndexConfig {
                project_id: String::from("legacy-project"),
                root_hash: hex::encode(Sha256::digest(root.as_bytes())),
                root_path: root,
                binding: vulcan_context_engine::VceIndexBinding::new(
                    "legacy-target",
                    "legacy-chain",
                    1,
                    space.clone(),
                )
                .expect("legacy binding should be valid"),
                vector_space_id: space.vector_space_id,
                physical_fingerprint: space
                    .physical_fingerprint
                    .expect("legacy producer should be explicit"),
                preprocessing_version: space.preprocessing_version,
                chunker_version: space.chunker_version,
                dimension: 3,
                config_revision: 1,
            },
            VceRebuildCandidateBase {
                namespace_id: String::from("legacy-source"),
                checkpoint_version: 4,
                job_id: String::from("legacy-job"),
            },
        );
        // The control: the fixture as written by this runtime is valid, so the rejection below is about the
        // version marker and not about a malformed fixture.
        // 对照：本运行时写出的夹具本身有效，因此下方的拒绝只关乎版本标记，而与夹具格式错误无关。
        receipt
            .validate()
            .expect("the current-version fixture must be valid");

        // The exact pre-plan shape: version 2 with the compaction field absent.
        // 前代的确切形态：版本 2 且缺少整理字段。
        receipt.schema_version = 2;
        let mut encoded = serde_json::to_value(&receipt).expect("legacy receipt should encode");
        encoded
            .as_object_mut()
            .expect("legacy receipt should be an object")
            .remove("compaction");

        let decoded = serde_json::from_value::<VceStoredJob>(encoded)
            .expect("the pre-plan receipt is still structurally decodable");
        assert_eq!(decoded.schema_version, 2);
        assert!(decoded.compaction.is_none());
        // Decodable is not the same as acceptable: the marker must stop it here.
        // 可解码不等于可接受：版本标记必须在此拦住它。
        let error = decoded
            .validate()
            .expect_err("a pre-plan receipt version must be refused, not read");
        assert!(
            error.contains("invalid version"),
            "unexpected rejection reason: {error}"
        );
    }

    /// Verify table and host terminal states preserve the checkpoint and reject false success.
    /// 验证表级与宿主终态会保留检查点，并拒绝虚假成功。
    #[test]
    fn compaction_receipt_state_machine_rejects_false_success() {
        let mut receipt = compaction_test_receipt();
        receipt
            .validate()
            .expect("pending compaction receipt should be valid");

        let table = receipt
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("compaction table should exist");
        table.state = VceCompactionTableState::Queued;
        table.round = 1;
        table.operation_id = Some(String::from("compaction-job:compact:content:1"));
        table.operation_expected_version = Some(9);
        receipt.state = VceJobState::Running;
        receipt.sequence += 1;
        receipt.updated_at_unix_ms += 1;
        receipt
            .validate()
            .expect("admitted operation should be valid");

        let table = receipt
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("compaction table should exist");
        table.state = VceCompactionTableState::Succeeded;
        table.latest_statistics.version = 10;
        table.latest_statistics.fragment_count = 1;
        table.latest_statistics.small_fragment_count = 1;
        table.latest_statistics.fragment_rows.minimum_rows = 32;
        table.latest_statistics.fragment_rows.maximum_rows = 32;
        table.latest_statistics.fragment_rows.mean_rows = 32;
        table.latest_statistics.fragment_rows.p25_rows = 32;
        table.latest_statistics.fragment_rows.p50_rows = 32;
        table.latest_statistics.fragment_rows.p75_rows = 32;
        table.latest_statistics.fragment_rows.p99_rows = 32;
        table.metrics.fragments_removed = 8;
        table.metrics.fragments_added = 1;
        receipt.state = VceJobState::Succeeded;
        receipt.result_checkpoint_version = receipt.source_checkpoint_version;
        receipt.sequence += 1;
        receipt.updated_at_unix_ms += 1;
        receipt
            .validate()
            .expect("successful compaction should preserve the source checkpoint");

        let mut wrong_checkpoint = receipt.clone();
        wrong_checkpoint.result_checkpoint_version = Some(8);
        assert!(wrong_checkpoint.validate().is_err());

        let mut unfinished_table = receipt.clone();
        let table = unfinished_table
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("compaction table should exist");
        table.latest_statistics.fragment_count = 2;
        table.latest_statistics.small_fragment_count = 2;
        assert!(unfinished_table.validate().is_err());

        let mut changed_rows = receipt.clone();
        changed_rows
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("compaction table should exist")
            .latest_statistics
            .row_count -= 1;
        assert!(changed_rows.validate().is_err());

        let mut changed_indexes = receipt.clone();
        changed_indexes
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("compaction table should exist")
            .latest_statistics
            .index_count += 1;
        assert!(changed_indexes.validate().is_err());

        let mut lost_state = compaction_test_receipt();
        let table = lost_state
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("compaction table should exist");
        table.state = VceCompactionTableState::StateLost;
        table.round = 1;
        table.operation_id = Some(String::from("lost-operation"));
        table.operation_expected_version = Some(9);
        table.error = Some(String::from("storage operation history was lost"));
        lost_state.state = VceJobState::Interrupted;
        lost_state.error = Some(String::from("storage operation history was lost"));
        lost_state.sequence += 1;
        lost_state.updated_at_unix_ms += 1;
        lost_state
            .validate()
            .expect("state loss should remain an explicit interrupted terminal receipt");
    }

    /// Verify durable compaction updates cannot rewrite plans, operation identities, or acknowledged facts.
    /// 验证持久整理更新不能改写计划、操作身份或已确认事实。
    #[test]
    fn compaction_receipt_transition_is_monotonic() {
        // Pending plan and its valid first storage admission.
        // 待处理计划及其合法的首次存储准入。
        let pending = compaction_test_receipt()
            .compaction
            .expect("pending compaction payload");
        let mut admitted = pending.clone();
        let table = admitted.tables.first_mut().expect("admitted table");
        table.state = VceCompactionTableState::Queued;
        table.round = 1;
        table.operation_id = Some(String::from("compaction-job:compact:content:1"));
        table.operation_expected_version = Some(9);
        admitted.validate().expect("admitted receipt");
        pending
            .validate_transition(&admitted)
            .expect("first round admission should advance");

        let mut skipped_round = admitted.clone();
        let table = skipped_round.tables.first_mut().expect("skipped table");
        table.round = 3;
        table.operation_id = Some(String::from("compaction-job:compact:content:3"));
        assert!(pending.validate_transition(&skipped_round).is_err());

        let mut changed_plan = admitted.clone();
        changed_plan.tables[0].plan.options.batch_size += 1;
        changed_plan
            .validate()
            .expect("changed plan is standalone valid");
        assert!(pending.validate_transition(&changed_plan).is_err());

        let mut running = admitted.clone();
        running.tables[0].state = VceCompactionTableState::Running;
        admitted
            .validate_transition(&running)
            .expect("queued operation may begin running");

        let mut rewritten_operation = running.clone();
        rewritten_operation.tables[0].state = VceCompactionTableState::Failed;
        rewritten_operation.tables[0].operation_id = Some(String::from("replacement-operation"));
        rewritten_operation.tables[0].error = Some(String::from("storage failure"));
        rewritten_operation
            .validate()
            .expect("rewritten operation is standalone valid");
        assert!(running.validate_transition(&rewritten_operation).is_err());

        let mut completed_round = running.clone();
        let table = completed_round.tables.first_mut().expect("completed table");
        table.state = VceCompactionTableState::Pending;
        table.operation_id = None;
        table.operation_expected_version = None;
        table.latest_statistics.version = 10;
        table.latest_statistics.fragment_count = 4;
        table.latest_statistics.small_fragment_count = 4;
        table.metrics.fragments_removed = 8;
        table.metrics.fragments_added = 4;
        running
            .validate_transition(&completed_round)
            .expect("acknowledged round may request another bounded round");

        let mut regressed_facts = completed_round.clone();
        let table = regressed_facts.tables.first_mut().expect("regressed table");
        table.state = VceCompactionTableState::Queued;
        table.round = 2;
        table.operation_id = Some(String::from("compaction-job:compact:content:2"));
        table.operation_expected_version = Some(10);
        regressed_facts
            .validate()
            .expect("second admission should be standalone valid");
        completed_round
            .validate_transition(&regressed_facts)
            .expect("second round admission should advance");
        regressed_facts.tables[0].metrics.fragments_removed = 7;
        assert!(
            completed_round
                .validate_transition(&regressed_facts)
                .is_err()
        );
    }

    /// Verify atomic cancellation and post-round settlement have explicit durable transitions.
    /// 验证原子取消及轮次完成后的收敛具有明确持久转换。
    #[test]
    fn compaction_terminal_transition_covers_running_and_completed_round_boundaries() {
        let pending = compaction_test_receipt();
        let mut admitted = pending.clone();
        admitted.state = VceJobState::Running;
        admitted.sequence += 1;
        admitted.updated_at_unix_ms += 1;
        let admitted_table = admitted
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("admitted table exists");
        admitted_table.state = VceCompactionTableState::Queued;
        admitted_table.round = 1;
        admitted_table.operation_id = Some(String::from("compaction-job:compact:content:1"));
        admitted_table.operation_expected_version = Some(9);
        admitted.validate().expect("admitted receipt is valid");
        pending
            .compaction
            .as_ref()
            .expect("pending payload")
            .validate_transition(admitted.compaction.as_ref().expect("admitted payload"))
            .expect("pending operation may advance to admitted");

        let mut running = admitted.clone();
        running.sequence += 1;
        running.updated_at_unix_ms += 1;
        running
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("running table exists")
            .state = VceCompactionTableState::Running;
        running.validate().expect("running receipt is valid");
        admitted
            .compaction
            .as_ref()
            .expect("admitted payload")
            .validate_transition(running.compaction.as_ref().expect("running payload"))
            .expect("admitted operation may advance to running");

        let mut cancelled = running.clone();
        cancelled.state = VceJobState::Cancelled;
        cancelled.sequence += 1;
        cancelled.updated_at_unix_ms += 1;
        cancelled.error = Some(String::from("VCE refresh cancelled"));
        let cancelled_table = cancelled
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("cancelled table exists");
        cancelled_table.state = VceCompactionTableState::Cancelled;
        cancelled.validate().expect("cancelled receipt is valid");
        assert!(job_state_transition_permitted(&running, &cancelled));
        running
            .compaction
            .as_ref()
            .expect("running payload")
            .validate_transition(cancelled.compaction.as_ref().expect("cancelled payload"))
            .expect("running compaction may settle cancellation atomically");

        let mut non_compaction = running.clone();
        non_compaction.kind = VceJobKind::Synchronize;
        assert!(!job_state_transition_permitted(&non_compaction, &cancelled));

        let mut completed_boundary = running.clone();
        let completed_table = completed_boundary
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("completed boundary table exists");
        completed_table.state = VceCompactionTableState::Pending;
        completed_table.operation_id = None;
        completed_table.operation_expected_version = None;
        completed_table.latest_statistics.version = 10;
        completed_table.latest_statistics.fragment_count = 4;
        completed_table.latest_statistics.small_fragment_count = 4;
        completed_table.metrics.fragments_removed = 8;
        completed_table.metrics.fragments_added = 4;
        completed_boundary
            .validate()
            .expect("completed round boundary is valid");
        running
            .compaction
            .as_ref()
            .expect("running payload")
            .validate_transition(
                completed_boundary
                    .compaction
                    .as_ref()
                    .expect("completed boundary payload"),
            )
            .expect("running round may persist its continuation boundary");

        let mut failed_after_round = completed_boundary.clone();
        failed_after_round.state = VceJobState::Failed;
        failed_after_round.sequence += 1;
        failed_after_round.updated_at_unix_ms += 1;
        failed_after_round.error = Some(String::from("continuation persistence failed"));
        let failed_table = failed_after_round
            .compaction
            .as_mut()
            .and_then(|compaction| compaction.tables.first_mut())
            .expect("failed table exists");
        failed_table.state = VceCompactionTableState::Failed;
        failed_table.error = Some(String::from("continuation persistence failed"));
        failed_after_round
            .validate()
            .expect("post-round failure remains readable without an active operation");
        completed_boundary
            .compaction
            .as_ref()
            .expect("completed boundary payload")
            .validate_transition(
                failed_after_round
                    .compaction
                    .as_ref()
                    .expect("failed payload"),
            )
            .expect("completed round boundary may settle as failed");
    }
}
