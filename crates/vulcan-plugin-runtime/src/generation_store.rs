//! Immutable package generations with explicit activation revisions and actual filesystem leases.
//! 具有显式激活修订及实际文件系统租约的不可变插件包代次。

mod files;
mod operations;
#[cfg(test)]
mod tests;

use VulcanPluginGenerationPhase as GenerationState;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Private on-disk format shared by generation and activation records.
/// 代次记录与激活记录共用的私有磁盘格式。
const RECORD_VERSION: u32 = 1;
/// Maximum encoded metadata size, checked before writes and before allocating reads.
/// 编码元数据大小上限，在写入及读取分配前检查。
const MAX_RECORD_BYTES: u64 = 64 * 1024;
/// Fixed private directory below the host-selected user-plugin root.
/// 宿主选定用户插件根目录下的固定私有目录。
const STORE_DIRECTORY: &str = ".vulcan-generations";

/// Stable failure classes for generation storage without parsing diagnostic text.
/// 无需解析诊断文本的代次存储稳定失败类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginStoreErrorCode {
    /// A real cooperating owner still holds the requested filesystem lock.
    /// 实际协作所有者仍持有所需文件系统锁。
    Busy,
    /// The caller's exact activation revision is no longer current.
    /// 调用方的精确激活修订已不再是当前修订。
    Conflict,
    /// An identity, path, package, or stored record failed validation.
    /// 身份、路径、插件包或存储记录未通过校验。
    Invalid,
    /// A filesystem operation failed; activation writes may require explicit readback.
    /// 文件系统操作失败；激活写入可能需要显式回读。
    Storage,
}

/// Structured storage failure retaining a stable class and contextual diagnostic.
/// 保留稳定类别及上下文诊断的结构化存储失败。
#[derive(Debug, Clone)]
pub struct VulcanPluginStoreError {
    /// Stable failure class used by the host transaction supervisor.
    /// 宿主事务监督器使用的稳定失败类别。
    pub code: VulcanPluginStoreErrorCode,
    /// Diagnostic without plugin secret values or arbitrary serialized payloads.
    /// 不包含插件秘密值或任意序列化载荷的诊断。
    pub message: String,
    /// Exact candidate identity retained after preparation allocation; absence means no such identity is reported.
    /// 准备分配后保留的精确候选身份；缺失表示未报告此类身份。
    pub retained_generation: Option<Box<VulcanPluginGenerationIdentity>>,
}

impl VulcanPluginStoreError {
    /// Construct one classified error from its class and safe diagnostic; returns the retained error.
    /// 根据类别及安全诊断构造错误；返回保留错误。
    fn new(code: VulcanPluginStoreErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retained_generation: None,
        }
    }
    /// Convert a contextual validation failure into the common store error.
    /// 将具有上下文的校验失败转为统一存储错误。
    fn invalid(message: impl Into<String>) -> Self {
        Self::new(VulcanPluginStoreErrorCode::Invalid, message)
    }
    /// Wrap an I/O failure from `action` without claiming that a preceding mutation was rolled back.
    /// 包装 `action` 的 I/O 失败，不声称先前变更已回滚。
    fn io(action: &str, error: std::io::Error) -> Self {
        Self::new(
            VulcanPluginStoreErrorCode::Storage,
            format!("{action}: {error}"),
        )
    }
}

impl std::fmt::Display for VulcanPluginStoreError {
    /// Write the retained diagnostic to `formatter`; returns its formatting result.
    /// 向 `formatter` 写入保留诊断；返回格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for VulcanPluginStoreError {}

/// Result of one explicit store operation with classified failures.
/// 带分类失败的一次显式存储操作结果。
pub type VulcanPluginStoreResult<T> = Result<T, VulcanPluginStoreError>;

/// Frozen expected identity of one package generation, retained throughout preparation and reclamation.
/// 一个插件包代次的固定预期身份，在准备及回收全程保留。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginGenerationIdentity {
    /// Validated plugin identifier selecting the namespace.
    /// 选择命名空间的已校验插件标识。
    pub plugin_id: String,
    /// Canonical UUID identifying one unique physical generation directory.
    /// 标识唯一物理代次目录的规范 UUID。
    pub generation_id: String,
    /// Validated manifest version of this exact package.
    /// 此精确插件包已校验的清单版本。
    pub version: String,
    /// Complete inspected source digest that copied contents must match before readiness.
    /// 完整已检查来源摘要；就绪前复制内容必须与其匹配。
    pub content_fingerprint: String,
}

/// Explicit activation state; a retained tombstone prevents revision reuse after deactivation.
/// 显式激活状态；保留停用记录以防止停用后复用修订号。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginActivation {
    /// Private record format version, rejected when unknown.
    /// 私有记录格式版本，未知版本明确拒绝。
    schema_version: u32,
    /// Plugin namespace selected by the activation file name.
    /// 由激活文件名选择的插件命名空间。
    pub plugin_id: String,
    /// Monotonic nonzero revision; a never-published namespace is observed as revision zero.
    /// 非零单调修订；从未发布的命名空间观察为修订零。
    pub revision: u64,
    /// Selected immutable generation, or an explicit deactivation tombstone.
    /// 选定不可变代次，或显式停用记录。
    #[serde(deserialize_with = "deserialize_required_generation")]
    pub generation: Option<VulcanPluginGenerationIdentity>,
}

/// Decode a required nullable generation field; missing data must not become a deactivation tombstone.
/// 解码必需但可空的代次字段；缺失数据不得成为停用记录。
/// `deserializer` owns the explicitly present JSON value; returns its declared optional identity.
/// `deserializer` 持有明确存在的 JSON 值；返回其声明的可选身份。
fn deserialize_required_generation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<VulcanPluginGenerationIdentity>, D::Error> {
    Option::<VulcanPluginGenerationIdentity>::deserialize(deserializer)
}

/// Persistent preparation, readiness, and collection state for one exact generation identity.
/// 一个精确代次身份的持久准备、就绪及回收状态。
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct GenerationRecord {
    /// Private record version.
    /// 私有记录版本。
    schema_version: u32,
    /// Complete generation identity.
    /// 完整代次身份。
    identity: VulcanPluginGenerationIdentity,
    /// Explicit collection state; interrupted cleanup never becomes a reusable generation.
    /// 显式回收状态；被中断清理的代次永不重新成为可复用代次。
    state: GenerationState,
}

/// Persisted cleanup boundary for one immutable identity.
/// 一个不可变身份的持久清理边界。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginGenerationPhase {
    /// Preparation identity is durable but package contents are not yet complete or verified.
    /// 准备身份已持久保存，但插件内容尚未完整或未验证。
    Preparing,
    /// Complete package verified and available for an explicit pin.
    /// 完整插件包已验证，可显式固定。
    Prepared,
    /// Cleanup intent is durable; only explicit cleanup retry may continue.
    /// 清理意图已持久保存；仅显式清理重试可继续。
    Collecting,
    /// Package bytes are removed; identity and lock tombstones remain retained.
    /// 插件包字节已删除；仍保留身份及锁记录。
    Collected,
}

/// Actual shared filesystem lease held until the last cloned package owner is dropped.
/// 持有至最后一个克隆插件包所有者被丢弃的实际共享文件系统租约。
#[derive(Debug)]
struct GenerationLease {
    /// The real locked file is never unlinked by this store.
    /// 实际加锁文件永不由此存储删除。
    _lock: File,
}

/// Verified package location pinned against cooperating garbage collection across processes.
/// 已验证插件包位置，阻止跨进程协作回收。
#[derive(Debug, Clone)]
pub struct PinnedVulcanPluginGeneration {
    /// Canonical store identity preventing cross-root handle reuse.
    /// 防止跨根目录复用句柄的规范存储身份。
    store_root: PathBuf,
    /// Immutable validated generation identity.
    /// 不可变的已校验代次身份。
    identity: VulcanPluginGenerationIdentity,
    /// Canonical physical package directory, never moved during activation.
    /// 激活过程中从不移动的规范物理插件包目录。
    package_root: PathBuf,
    /// Shared real lock ownership, including all handle clones.
    /// 包含全部句柄克隆的共享实际锁所有权。
    _lease: Arc<GenerationLease>,
}

impl PinnedVulcanPluginGeneration {
    /// Return the frozen identity; borrowing it cannot change the pinned generation.
    /// 返回固定身份；借用身份不能改变所固定代次。
    pub fn identity(&self) -> &VulcanPluginGenerationIdentity {
        &self.identity
    }
    /// Return the pinned physical root; the caller must retain this handle while using its files.
    /// 返回固定物理根目录；使用其文件期间调用方必须保留此句柄。
    pub fn package_root(&self) -> &Path {
        &self.package_root
    }
}

/// Immutable storage primitive; host authorization, readiness, drain, and route publication remain explicit.
/// 不可变存储基础组件；宿主授权、就绪、排空及路由发布仍须显式执行。
#[derive(Debug, Clone)]
pub struct VulcanPluginGenerationStore {
    /// Canonical fixed private root whose children are validated without following links.
    /// 规范固定私有根目录；检查子项时不跟随链接。
    root: PathBuf,
}

/// One namespace observation whose selected package is pinned before its selection fence is released.
/// 一个命名空间观察结果，其选定插件包在选择屏障释放前已固定。
#[derive(Debug)]
pub struct VulcanPluginActiveGeneration {
    /// Exact persistent selection, including explicit deactivation tombstones.
    /// 精确持久选择，包含显式停用记录。
    pub activation: Option<VulcanPluginActivation>,
    /// Real lease for the selected generation, absent only for no selection or a tombstone.
    /// 选定代次的实际租约，仅未选择或停用记录时缺失。
    pub generation: Option<PinnedVulcanPluginGeneration>,
}
