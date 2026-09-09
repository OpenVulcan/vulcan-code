//! Durable project-job receipts, separate from core checkpoint operations and UI event delivery.
//! 持久项目作业回执，独立于核心检查点操作与界面事件投递。

use super::{
    ControllerSqliteValue, CountRow, VceDataPlaneHandle, VceProjectIndexConfig,
    VceRebuildCandidateBase, sqlite_string_literal, sqlite_text,
};
use serde::{Deserialize, Serialize};
use vulcan_context_engine::VceRefreshProgress;

/// Sole supported receipt format; no legacy receipt migration is performed.
/// 唯一支持的回执格式；不执行历史回执迁移。
pub const VCE_JOB_RECEIPT_SCHEMA_VERSION: u32 = 2;

/// Bound storage confirmation retries so a transient controller transport fault cannot strand a live owner.
/// 限制存储确认重试次数，避免短暂的控制器传输故障把活动作业所有权永久卡住。
const VCE_RECEIPT_CONFIRM_RETRIES: usize = 3;

/// Identify the exact controller failure that is safe to re-read after an ambiguous SQLite operation.
/// 只识别已观测到的控制器传输错误，允许在结果不确定时重新读取而不是盲目重复写入。
fn is_retryable_receipt_transport_error(error: &str) -> bool {
    error.contains("transport error")
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
        for fingerprint in [&self.request_fingerprint, &self.contract_fingerprint] {
            if !vulcan_context_engine::is_sha256_identity(fingerprint) {
                return Err(String::from(
                    "VCE job fingerprint must be an exact SHA-256 identity",
                ));
            }
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
        {
            return Err(String::from(
                "VCE job receipt exceeds the storage integer range",
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
            && (self.result_checkpoint_version.is_none() || self.completed_update.is_none()))
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
        if let Some(progress) = &self.progress
            && (progress
                .total
                .is_some_and(|total| progress.completed > total)
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

/// One exact physical project's durable job repository supplied by the host factory.
/// 由宿主工厂提供的精确物理项目持久作业仓库。
#[derive(Clone)]
pub struct VceProjectJobStore {
    /// Host-owned controller bindings with a counted lifetime lease.
    /// 带计数生命周期租约的宿主持有控制器绑定。
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
    pub(super) fn new(data_plane: VceDataPlaneHandle, project_id: String) -> Self {
        Self {
            data_plane,
            project_id,
        }
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
        self.data_plane.execute("INSERT INTO vce_host_jobs(project_id, job_id, owner_run_id, idempotency_key, request_fingerprint, sequence, snapshot_json, created_at_unix_ms, updated_at_unix_ms) VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?) ON CONFLICT(project_id, idempotency_key) DO NOTHING;", vec![sqlite_text(&job.project_id), sqlite_text(&job.job_id), sqlite_text(&job.owner_run_id), sqlite_text(&job.idempotency_key), sqlite_text(&job.request_fingerprint), sqlite_text(serialized), ControllerSqliteValue::Int64(job.created_at_unix_ms as i64), ControllerSqliteValue::Int64(job.updated_at_unix_ms as i64)], "Register VCE job receipt").await?;
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
    pub async fn get_after_transport_retry(
        &self,
        job_id: &str,
    ) -> Result<Option<VceStoredJob>, String> {
        for attempt in 0..=VCE_RECEIPT_CONFIRM_RETRIES {
            match self.get(job_id).await {
                Ok(receipt) => return Ok(receipt),
                Err(error)
                    if is_retryable_receipt_transport_error(&error)
                        && attempt < VCE_RECEIPT_CONFIRM_RETRIES =>
                {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        200 * (attempt as u64 + 1),
                    ))
                    .await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("bounded VCE receipt read retry exhausted without a result")
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
        // Presence is read-only; old job repositories need no migration merely to display receipts.
        // 表存在性检查只读；旧作业仓库无需为显示回执而迁移。
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
        if !previous.state.permits(next.state)
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
            .get_after_transport_retry(&previous.job_id)
            .await?
            .ok_or_else(|| String::from("VCE job to update does not exist"))?;
        if current != *previous {
            return Err(String::from("VCE job receipt changed before update"));
        }
        let serialized = serde_json::to_string(next)
            .map_err(|error| format!("Encode VCE job update: {error}"))?;
        let update_params = vec![
            ControllerSqliteValue::Int64(next.sequence as i64),
            sqlite_text(serialized),
            ControllerSqliteValue::Int64(next.updated_at_unix_ms as i64),
            sqlite_text(&self.project_id),
            sqlite_text(&previous.job_id),
            ControllerSqliteValue::Int64(previous.sequence as i64),
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
                Err(error) if is_retryable_receipt_transport_error(&error) => {
                    // The write may already have committed; reconcile before attempting it again.
                    // 写入可能已经提交，重试写入前先读取并收敛到已提交结果。
                    match self.get_after_transport_retry(&next.job_id).await? {
                        Some(receipt) if receipt == *next => return Ok(receipt),
                        Some(receipt)
                            if receipt == *previous && attempt < VCE_RECEIPT_CONFIRM_RETRIES =>
                        {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                200 * (attempt as u64 + 1),
                            ))
                            .await;
                        }
                        Some(receipt) if receipt == *previous => return Err(error),
                        Some(_) => {
                            return Err(String::from("VCE job receipt changed during update"));
                        }
                        None => return Err(String::from("VCE job to update does not exist")),
                    }
                }
                Err(error) => return Err(error),
            }
        }
        let persisted = self
            .get_after_transport_retry(&next.job_id)
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
        params: Vec<ControllerSqliteValue>,
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
}
