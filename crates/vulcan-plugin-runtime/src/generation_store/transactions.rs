//! Durable selection intent separates storage publication from the host's route handoff.
//! 持久选择意图区分存储发布与宿主路由交接。

use super::*;
use uuid::Uuid;

mod installation;
mod operations;
mod preparation;
mod records;
pub use installation::{VulcanPluginInstallationPhase, VulcanPluginInstallationReceipt};
pub use preparation::{
    VulcanPluginActivationPreparation, VulcanPluginActivationPreparationIntent,
    VulcanPluginActivationPreparationRecord, VulcanPluginActivationRequestRecord,
};

/// Legacy complete-record and stable request-index format; version two keeps the same index schema.
/// 旧完整记录及稳定请求索引格式；第二版保持相同索引模式。
const TRANSACTION_RECORD_VERSION: u32 = 1;
/// Version two records bind early preparation to the same original request and index.
/// 第二版记录将早期准备绑定至相同原请求及索引。
const PREPARATION_RECORD_VERSION: u32 = 2;

/// Journal envelope contains two bounded selections and one metadata allowance, all derived from existing budgets.
/// 日志封套容纳两个有界选择及一个元数据额度，全部从既有限制派生。
const MAX_TRANSACTION_BYTES: u64 = MAX_ACTIVATION_BYTES * 2 + MAX_RECORD_BYTES;

/// Exact durable checkpoint of one host-controlled activation request; no phase implies a callback was replayed.
/// 宿主控制的单次激活请求的精确持久检查点；任何阶段都不表示回调已被重放。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginActivationTransactionPhase {
    /// The original request reserves exact resources before package copying and credential staging.
    /// 原请求在插件包复制及凭据暂存前预留精确资源。
    Preparing,
    /// Intent exists, but the host has not requested selection publication.
    /// 意图已存在，但宿主尚未请求发布选择。
    Prepared,
    /// Publication was requested; exact activation readback decides whether it happened.
    /// 已请求发布；精确激活回读决定其是否已经发生。
    Committing,
    /// The target selection is durable; route handoff still requires explicit host acknowledgement.
    /// 目标选择已持久化；路由交接仍要求宿主明确确认。
    Published,
    /// The host acknowledged its complete route handoff; this historical receipt is retained.
    /// 宿主已确认完整路由交接；保留此历史回执。
    Completed,
    /// The unchanged predecessor was confirmed before explicitly abandoning this intent.
    /// 明确放弃此意图前已确认前驱未变。
    Aborted,
}

impl VulcanPluginActivationTransactionPhase {
    /// Return whether this phase no longer fences successor writes or historical resource collection.
    /// 返回此阶段是否不再阻挡后继写入或历史资源回收。
    fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Aborted)
    }
}

/// Original request, predecessor and target retained independently from process lifetime and later transactions.
/// 原请求、前驱及目标独立于进程寿命和后续事务保留。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginActivationTransactionRecord {
    /// Exact private journal format; unknown versions cannot be repaired during reads.
    /// 精确私有日志格式；读取时不能修复未知版本。
    schema_version: u32,
    /// Canonical non-nil request identity allocated by the host before any side effect.
    /// 宿主在任何副作用之前分配的规范非空请求身份。
    #[serde(deserialize_with = "records::deserialize_request_id")]
    request_id: Uuid,
    /// Explicit prior selection, including absence only for a never-published namespace.
    /// 显式前驱选择，仅从未发布的命名空间允许缺失。
    #[serde(deserialize_with = "records::deserialize_predecessor")]
    predecessor: Option<VulcanPluginActivation>,
    /// Complete configured target, including the unique successor activation revision.
    /// 完整配置目标，包含唯一后继激活修订。
    target: VulcanPluginActivation,
    /// Version two retains its immutable early intent after the complete target becomes available.
    /// 第二版在完整目标可用之后仍保留不可变早期意图。
    #[serde(
        default,
        deserialize_with = "preparation::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    preparation: Option<VulcanPluginActivationPreparationIntent>,
    /// Durable checkpoint, independent from the host's in-memory readiness objects.
    /// 持久检查点，独立于宿主内存就绪对象。
    phase: VulcanPluginActivationTransactionPhase,
}

impl VulcanPluginActivationTransactionRecord {
    /// Return the original request ID; callers retain it even when a mutation returns an uncertain result.
    /// 返回原请求标识；即使变更返回不确定结果，调用方也保留它。
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    /// Borrow the exact predecessor, returning None only for the original unpublished namespace.
    /// 借用精确前驱，仅原始未发布命名空间返回 None。
    pub fn predecessor(&self) -> Option<&VulcanPluginActivation> {
        self.predecessor.as_ref()
    }

    /// Borrow the original target without interpreting it as the currently published selection.
    /// 借用原目标，不将其解释为当前已发布选择。
    pub fn target(&self) -> &VulcanPluginActivation {
        &self.target
    }

    /// Return the durable checkpoint without probing callbacks or external routes.
    /// 返回持久检查点，不探测回调或外部路由。
    pub fn phase(&self) -> VulcanPluginActivationTransactionPhase {
        self.phase
    }
}

/// Exclusive live request owner with real package and credential leases; dropping it does not abort its journal.
/// 具有实际插件包及凭据租约的请求排他所有者；释放它不会中止日志。
#[derive(Debug)]
pub struct VulcanPluginActivationTransaction {
    /// Exact store prevents passing an owner into an unrelated namespace root.
    /// 精确存储防止向无关命名空间根传入所有者。
    store_root: PathBuf,
    /// Last confirmed record must still match disk before each mutation.
    /// 每次变更前，最后确认的记录仍须与磁盘匹配。
    record: VulcanPluginActivationTransactionRecord,
    /// Actual exclusive request ownership is shared with real readiness workers, without another lock namespace.
    /// 实际排他请求所有权与真实就绪工作者共享，不创建另一锁命名空间。
    _lease: Arc<File>,
    /// Target package ownership is retained while host callbacks inspect the candidate.
    /// 宿主回调检查候选期间保留目标插件包所有权。
    generation: PinnedVulcanPluginGeneration,
    /// Actual secure-store revision retention prevents cleanup during candidate readiness.
    /// 实际安全存储修订保留防止候选就绪期间发生清理。
    credentials: PinnedVulcanPluginCredentials,
}

impl VulcanPluginActivationTransaction {
    /// Borrow the last confirmed journal checkpoint; query disk after any failed mutation.
    /// 借用最后确认的日志检查点；任何变更失败后查询磁盘。
    pub fn record(&self) -> &VulcanPluginActivationTransactionRecord {
        &self.record
    }

    /// Borrow the actually pinned target package for explicit host readiness preparation.
    /// 借用实际固定的目标插件包，用于明确的宿主就绪准备。
    pub fn generation(&self) -> &PinnedVulcanPluginGeneration {
        &self.generation
    }

    /// Borrow the actual target credential reader, preserving ownership when readiness descriptors clone it.
    /// 借用实际目标凭据读者，在就绪描述符克隆它时保留所有权。
    pub fn credentials(&self) -> &PinnedVulcanPluginCredentials {
        &self.credentials
    }
}

/// Inspected unpublished package borrowing the exclusive transaction owner; this is not an execution grant.
/// 借用排他事务所有者的已检查未发布插件包；此对象并非执行授权。
/// The borrow prevents commit or abort through that owner while the candidate view remains in use.
/// 此借用防止候选视图仍在使用时通过该所有者提交或中止事务。
#[derive(Debug)]
pub struct VulcanPluginActivationCandidate<'transaction> {
    /// The original exclusive owner remains live throughout the borrowed candidate view.
    /// 原排他所有者在借用候选视图的整个寿命中保持存活。
    transaction: &'transaction VulcanPluginActivationTransaction,
    /// Exact target authority retains actual package and credential leases independently of public discovery.
    /// 精确目标权威独立于公开发现保留实际插件包及凭据租约。
    package: crate::VulcanPluginDescriptor,
    /// The exact validated user catalog binds candidate admission to its owning host manager.
    /// 精确已校验用户目录将候选入场绑定至所属宿主管理器。
    catalog_root: PathBuf,
    /// Shared ownership of the original exclusive request lock survives every actual readiness worker.
    /// 原排他请求锁的共享所有权在每个实际就绪工作者期间存续。
    readiness: Arc<File>,
}

impl VulcanPluginActivationCandidate<'_> {
    /// Borrow the validated user catalog root for exact manager ownership checks.
    /// 借用已校验用户目录根，用于精确管理器归属检查。
    pub(crate) fn catalog_root(&self) -> &Path {
        &self.catalog_root
    }

    /// Clone original request lock ownership for a tracked call or native pool; identity alone does not retain it.
    /// 为受跟踪调用或原生池克隆原请求锁所有权；仅身份不能保留它。
    pub(crate) fn retain_readiness(&self) -> Arc<File> {
        Arc::clone(&self.readiness)
    }

    /// Borrow the original prepared request, including its complete predecessor and target.
    /// 借用原准备请求，包含其完整前驱及目标。
    pub fn transaction_record(&self) -> &VulcanPluginActivationTransactionRecord {
        self.transaction.record()
    }

    /// Borrow the validated disabled package; cloned descriptors retain files but confer no runtime admission.
    /// 借用已校验且停用的插件包；克隆描述符保留文件，但不授予运行时入场权。
    /// Generation zero is deliberately unassigned; only the runtime publication owner allocates routing identities.
    /// 代次零有意保持未分配；仅运行时发布所有者分配路由身份。
    pub fn package(&self) -> &crate::VulcanPluginDescriptor {
        &self.package
    }
}
