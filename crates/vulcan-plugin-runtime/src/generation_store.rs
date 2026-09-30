//! Immutable package generations with explicit activation revisions and actual filesystem leases.
//! 具有显式激活修订及实际文件系统租约的不可变插件包代次。

mod configuration;
mod credential_staging;
pub use configuration::{MAX_PLUGIN_CONFIGURATION_BYTES, VulcanPluginConfigurationSelection};
mod credentials;
pub use credential_staging::{
    PinnedVulcanPluginCredentials, VulcanPluginCredentialBackend, VulcanPluginCredentialKey,
    VulcanPluginCredentialPhase, VulcanPluginCredentialRecord, VulcanPluginCredentialStaging,
    VulcanPluginCredentialValues,
};
mod discovery;
pub use credentials::{VulcanPluginCredentialBinding, VulcanPluginCredentialRevision};
mod files;
mod operations;
mod transactions;
pub(crate) use transactions::EffectAttempt;
pub(crate) use transactions::{TargetHookInitializationAttempt, TargetHookInitializationOwner};
pub(crate) use transactions::{TargetMcpInitializationAttempt, TargetMcpInitializationOwner};
pub use transactions::{
    VulcanPluginActivationCandidate, VulcanPluginActivationEffect, VulcanPluginActivationEffects,
    VulcanPluginActivationPreparation, VulcanPluginActivationPreparationIntent,
    VulcanPluginActivationPreparationRecord, VulcanPluginActivationRequestRecord,
    VulcanPluginActivationTransaction, VulcanPluginActivationTransactionPhase,
    VulcanPluginActivationTransactionRecord, VulcanPluginEffectObservation,
    VulcanPluginInstallationPhase, VulcanPluginInstallationReceipt,
    VulcanPluginPredecessorDrainIdentity, VulcanPluginTargetHookInitializationPhase,
    VulcanPluginTargetHookInitializationReceipt, VulcanPluginTargetMcpInitializationPhase,
    VulcanPluginTargetMcpInitializationReceipt, VulcanPluginUpdatePolicy,
};
#[cfg(test)]
mod tests;

use VulcanPluginGenerationPhase as GenerationState;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Private generation format and legacy selection format; their existing bytes remain readable.
/// 私有代次格式及旧选择格式；其既有字节保持可读。
const RECORD_VERSION: u32 = 1;
/// Selection format that atomically binds complete approved permissions to its chosen package.
/// 将完整已批准权限与所选插件包原子绑定的选择格式。
const AUTHORIZED_ACTIVATION_VERSION: u32 = 2;
/// Selection format binding the package, approval and secure-store credential revision together.
/// 共同绑定插件包、批准及安全存储凭据修订的选择格式。
const CREDENTIAL_ACTIVATION_VERSION: u32 = 3;
/// Selection format binds a complete explicit configuration to the same package, permission and credential choice.
/// 选择格式将完整显式配置绑定至相同的插件包、权限及凭据选择。
const CONFIGURED_ACTIVATION_VERSION: u32 = 4;
/// Configuration-bearing records retain the existing metadata allowance plus the sole configuration value budget.
/// 携带配置的记录保留既有元数据额度，并加上唯一配置值预算。
const MAX_ACTIVATION_BYTES: u64 = MAX_RECORD_BYTES + MAX_PLUGIN_CONFIGURATION_BYTES;
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
    /// Versions two onward own exact sorted permissions; version one alone reads legacy authorization files.
    /// 第二版及后续版本持有精确有序权限；仅第一版读取旧授权文件。
    #[serde(
        default,
        deserialize_with = "deserialize_present_permissions",
        skip_serializing_if = "Option::is_none"
    )]
    approved_permissions: Option<Vec<String>>,
    /// Versions three onward name an exact credential revision; earlier versions require this field to be absent.
    /// 第三版及后续版本指定精确凭据修订；较早版本要求此字段缺失。
    #[serde(
        default,
        deserialize_with = "deserialize_present_credential_revision",
        skip_serializing_if = "Option::is_none"
    )]
    credential_revision: Option<VulcanPluginCredentialRevision>,
    /// Version four owns one explicit configuration selection; absence belongs only to older formats.
    /// 第四版持有单个显式配置选择；仅旧格式允许缺失。
    #[serde(
        default,
        deserialize_with = "configuration::deserialize_present_selection",
        skip_serializing_if = "Option::is_none"
    )]
    configuration: Option<VulcanPluginConfigurationSelection>,
}

impl VulcanPluginActivation {
    /// Return approval frozen in this selection; absence explicitly denotes the legacy file authority.
    /// 返回此选择中固定的批准权限；缺失明确表示旧文件权威。
    /// An empty slice approves a zero-permission package, or belongs to an authorized deactivation tombstone.
    /// 空切片批准零权限插件包，或属于已授权停用记录。
    pub fn approved_permissions(&self) -> Option<&[String]> {
        self.approved_permissions.as_deref()
    }

    /// Return the declared credential revision; the store validates its presence against the record version before use.
    /// 返回声明的凭据修订；存储在使用前按记录版本校验其存在性。
    pub fn credential_revision(&self) -> Option<&VulcanPluginCredentialRevision> {
        self.credential_revision.as_ref()
    }

    /// Borrow the configuration frozen in this activation; absence explicitly denotes the legacy file authority.
    /// 借用此激活中固定的配置；缺失明确表示旧文件权威。
    pub fn configuration(&self) -> Option<&VulcanPluginConfigurationSelection> {
        self.configuration.as_ref()
    }
}

/// Decode a present credential revision without accepting JSON null as an older format.
/// 解码存在的凭据修订，不将 JSON 空值接受为旧格式。
/// `deserializer` supplies one canonical string; returns an explicitly present validated identity.
/// `deserializer` 提供单个规范字符串；返回明确存在的已校验身份。
fn deserialize_present_credential_revision<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<VulcanPluginCredentialRevision>, D::Error> {
    VulcanPluginCredentialRevision::deserialize(deserializer).map(Some)
}

/// Decode an explicitly present permission list; JSON null is never equivalent to a legacy missing field.
/// 解码明确存在的权限列表；JSON 空值绝不等价于旧格式缺失字段。
/// The deserializer supplies the field value; return only a complete non-null vector.
/// 反序列化器提供字段值；仅返回完整非空值向量。
fn deserialize_present_permissions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Vec<String>>, D::Error> {
    Vec::<String>::deserialize(deserializer).map(Some)
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

/// Metadata derived from one verified package read, shared by preparation and subsequent pins.
/// 从单次已验证插件包读取派生的元数据，由准备及后续固定共用。
struct VerifiedPackageMetadata {
    /// Complete canonical permission keys from that exact verified descriptor.
    /// 来自该精确已验证描述符的完整规范权限键。
    required_permissions: Vec<String>,
    /// Complete canonical credential inventory from the same descriptor, without any values.
    /// 来自相同描述符的完整规范凭据清单，不含任何值。
    credential_keys: Vec<VulcanPluginCredentialKey>,
    /// Configuration declaration belongs to the same verified package inspection as permissions and credential keys.
    /// 配置声明与权限及凭据键属于同一次已验证插件包检查。
    configuration: Option<crate::VulcanPluginConfigManifest>,
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
    /// Complete canonical capability keys derived while verifying this exact physical package.
    /// 校验此精确物理插件包时派生的完整规范能力键。
    required_permissions: Vec<String>,
    /// Complete declared secret slots derived during the same authoritative package verification.
    /// 在同一次权威插件包校验期间派生的完整声明秘密槽位。
    credential_keys: Vec<VulcanPluginCredentialKey>,
    /// Exact configuration schema declaration captured from this immutable package.
    /// 从此不可变插件包捕获的精确配置模式声明。
    configuration: Option<crate::VulcanPluginConfigManifest>,
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
    /// Actual credential ownership for an active revision-bound choice; legacy and tombstones have none.
    /// 绑定修订的有效选择的实际凭据所有权；旧格式和停用记录没有此所有权。
    pub credentials: Option<PinnedVulcanPluginCredentials>,
}
