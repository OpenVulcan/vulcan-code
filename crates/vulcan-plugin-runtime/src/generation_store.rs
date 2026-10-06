//! Immutable package generations with explicit activation revisions and actual filesystem leases.
//! 具有显式激活修订及实际文件系统租约的不可变插件包代次。

mod configuration;
mod credential_staging;
pub use configuration::{MAX_PLUGIN_CONFIGURATION_BYTES, VulcanPluginConfigurationSelection};
mod credentials;
pub(crate) use credential_staging::declared_keys;
pub use credential_staging::{
    PinnedVulcanPluginCredentials, VulcanPluginCredentialBackend, VulcanPluginCredentialKey,
    VulcanPluginCredentialPackage, VulcanPluginCredentialPhase, VulcanPluginCredentialRecord,
    VulcanPluginCredentialStaging, VulcanPluginCredentialStagingReservation,
    VulcanPluginCredentialValues,
};
mod deactivation;
mod discovery;
pub use credentials::{VulcanPluginCredentialBinding, VulcanPluginCredentialRevision};
pub use deactivation::{VulcanPluginDeactivationObservation, VulcanPluginDeactivationReservation};
mod files;
mod namespace_guard;
pub use namespace_guard::VulcanPluginNamespaceGuard;
mod operations;
mod published_dispatch;
mod settings;
pub(crate) use published_dispatch::PublishedDispatchFence;
mod transactions;
pub(crate) use transactions::EffectAttempt;
pub(crate) use transactions::{PredecessorHandoffReservation, PredecessorHandoffTransferAuthority};
pub(crate) use transactions::{TargetHookInitializationAttempt, TargetHookInitializationOwner};
pub(crate) use transactions::{TargetMcpInitializationAttempt, TargetMcpInitializationOwner};
pub use transactions::{
    VulcanPluginActivationCandidate, VulcanPluginActivationEffect, VulcanPluginActivationEffects,
    VulcanPluginActivationPreparation, VulcanPluginActivationPreparationInspection,
    VulcanPluginActivationPreparationIntent, VulcanPluginActivationPreparationRecord,
    VulcanPluginActivationPreparationReservation, VulcanPluginActivationRequestRecord,
    VulcanPluginActivationTransaction, VulcanPluginActivationTransactionPhase,
    VulcanPluginActivationTransactionRecord, VulcanPluginEffectObservation,
    VulcanPluginInstallationPhase, VulcanPluginInstallationReceipt,
    VulcanPluginPredecessorDrainIdentity, VulcanPluginPredecessorHandoffPhase,
    VulcanPluginPredecessorHandoffRecord, VulcanPluginPreparationPackageCleanupObservation,
    VulcanPluginTargetHookInitializationPhase, VulcanPluginTargetHookInitializationReceipt,
    VulcanPluginTargetMcpInitializationPhase, VulcanPluginTargetMcpInitializationReceipt,
    VulcanPluginUpdatePolicy,
};
#[cfg(any(test, feature = "test-support"))]
mod test_support;
#[cfg(any(test, feature = "test-support"))]
pub use test_support::install_user_package_for_test;
#[cfg(test)]
mod tests;

use VulcanPluginGenerationPhase as GenerationState;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Private generation marker format.
/// 私有代次标记格式。
const RECORD_VERSION: u32 = 1;
/// Sole activation format: a package selection with complete approval, beside an independently revisioned settings section.
/// 唯一激活格式：带完整批准的插件包选择，旁附独立修订的设置段。
/// Other versions are rejected as invalid; earlier unreleased formats are intentionally not recognized.
/// 其他版本按无效拒绝；较早未发布格式有意不再识别。
const ACTIVATION_RECORD_VERSION: u32 = 5;
/// Activation records retain the common metadata allowance plus the sole configuration value budget.
/// 激活记录保留通用元数据额度，并加上唯一配置值预算。
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

/// Package half of an activation: which package runs, changed only by install, update and uninstall transactions.
/// 激活的插件包部分：运行哪个插件包，仅由安装、更新及卸载交易改变。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VulcanPluginPackageSelection {
    /// One immutable managed generation.
    /// 单个不可变受管代次。
    Generation {
        /// Exact selected generation identity.
        /// 精确所选代次身份。
        identity: VulcanPluginGenerationIdentity,
    },
    /// The package shipped with the host; only its settings are stored here.
    /// 随宿主分发的插件包；此处只保存其设置。
    /// Keep a struct variant so Serde rejects unknown fields that a unit variant would otherwise ignore.
    /// 保持结构体分支，使 Serde 拒绝单位分支原本会忽略的未知字段。
    Bundled {},
    /// Explicit removal tombstone that prevents revision reuse.
    /// 防止修订复用的显式移除停用记录。
    Uninstalled {},
}

/// Settings half of an activation, edited in place without changing the package selection.
/// 激活的设置部分，可就地编辑而不改变插件包选择。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginSettings {
    /// Monotonic nonzero settings revision; edits and transaction commits advance it.
    /// 单调非零设置修订；编辑及交易提交使其递增。
    pub revision: u64,
    /// Complete configuration selection; a tombstone retains its last value.
    /// 完整配置选择；停用记录保留最后值。
    configuration: VulcanPluginConfigurationSelection,
    /// Exact credential revision, required for managed generations and absent for bundled packages;
    /// a tombstone retains its last revision for cleanup.
    /// 精确凭据修订，受管代次必需，内建插件包缺省；停用记录保留最后修订以供清理。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    credential_revision: Option<VulcanPluginCredentialRevision>,
}

impl VulcanPluginSettings {
    /// Borrow the selected configuration.
    /// 借用所选配置。
    pub fn configuration(&self) -> &VulcanPluginConfigurationSelection {
        &self.configuration
    }

    /// Return the selected credential revision without exposing any secret value.
    /// 返回所选凭据修订，不暴露任何秘密值。
    pub fn credential_revision(&self) -> Option<&VulcanPluginCredentialRevision> {
        self.credential_revision.as_ref()
    }
}

/// One settings edit; absent parts keep their current value.
/// 单次设置编辑；缺省部分保留当前值。
#[derive(Clone, Debug, Default)]
pub struct VulcanPluginSettingsChange {
    /// Replacement configuration, validated against the selected package.
    /// 替换配置，按所选插件包校验。
    pub configuration: Option<VulcanPluginConfigurationSelection>,
    /// Replacement credential revision, already staged for the selected package.
    /// 替换凭据修订，须已为所选插件包暂存。
    pub credential_revision: Option<VulcanPluginCredentialRevision>,
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
    /// Monotonic nonzero package-selection revision; a never-published namespace is observed as revision zero.
    /// 非零单调插件包选择修订；从未发布的命名空间观察为修订零。
    pub revision: u64,
    /// Selected package, or an explicit removal tombstone.
    /// 所选插件包，或显式移除停用记录。
    package: VulcanPluginPackageSelection,
    /// Exact sorted permissions approved for the selected package; a tombstone retains an empty list.
    /// 为所选插件包批准的精确有序权限；停用记录保留空列表。
    approved_permissions: Vec<String>,
    /// Independently revisioned settings of this namespace.
    /// 此命名空间独立修订的设置。
    settings: VulcanPluginSettings,
}

impl VulcanPluginActivation {
    /// Build a managed selection at `revision` whose settings revision is `settings_revision`.
    /// 在 `revision` 构造受管选择，其设置修订为 `settings_revision`。
    pub(in crate::generation_store) fn generation_selection(
        plugin_id: String,
        revision: u64,
        identity: VulcanPluginGenerationIdentity,
        approved_permissions: Vec<String>,
        settings_revision: u64,
        configuration: VulcanPluginConfigurationSelection,
        credential_revision: VulcanPluginCredentialRevision,
    ) -> Self {
        Self {
            schema_version: ACTIVATION_RECORD_VERSION,
            plugin_id,
            revision,
            package: VulcanPluginPackageSelection::Generation { identity },
            approved_permissions,
            settings: VulcanPluginSettings {
                revision: settings_revision,
                configuration,
                credential_revision: Some(credential_revision),
            },
        }
    }

    /// Build the first or next bundled settings record carrying `configuration` and optional staged credentials.
    /// 构造携带 `configuration` 及可选已暂存凭据的首个或下一个内建设置记录。
    /// A bundled package is never selected by a transaction, so its package revision stays one.
    /// 内建插件包从不由交易选择，因此其包修订固定为一。
    pub(in crate::generation_store) fn bundled_settings(
        plugin_id: String,
        settings_revision: u64,
        configuration: VulcanPluginConfigurationSelection,
        credential_revision: Option<VulcanPluginCredentialRevision>,
    ) -> Self {
        Self {
            schema_version: ACTIVATION_RECORD_VERSION,
            plugin_id,
            revision: 1,
            package: VulcanPluginPackageSelection::Bundled {},
            approved_permissions: Vec::new(),
            settings: VulcanPluginSettings {
                revision: settings_revision,
                configuration,
                credential_revision,
            },
        }
    }

    /// Settings revision a commit after `predecessor` takes; the first selection starts at one.
    /// 在 `predecessor` 之后提交所取的设置修订；首个选择从一开始。
    pub(in crate::generation_store) fn next_settings_revision(
        predecessor: Option<&Self>,
    ) -> VulcanPluginStoreResult<u64> {
        predecessor
            .map_or(0, |record| record.settings.revision)
            .checked_add(1)
            .ok_or_else(|| {
                VulcanPluginStoreError::invalid("Plugin settings revision is exhausted.")
            })
    }

    /// Borrow the package selection.
    /// 借用插件包选择。
    pub fn package(&self) -> &VulcanPluginPackageSelection {
        &self.package
    }

    /// Return the selected managed generation; bundled packages and tombstones have none.
    /// 返回所选受管代次；内建插件包及停用记录没有代次。
    pub fn generation(&self) -> Option<&VulcanPluginGenerationIdentity> {
        match &self.package {
            VulcanPluginPackageSelection::Generation { identity } => Some(identity),
            VulcanPluginPackageSelection::Bundled {}
            | VulcanPluginPackageSelection::Uninstalled {} => None,
        }
    }

    /// Return approval frozen in this selection; an empty slice approves a zero-permission package or belongs to a tombstone.
    /// 返回此选择中固定的批准权限；空切片批准零权限插件包，或属于停用记录。
    pub fn approved_permissions(&self) -> &[String] {
        &self.approved_permissions
    }

    /// Borrow the independently revisioned settings section.
    /// 借用独立修订的设置段。
    pub fn settings(&self) -> &VulcanPluginSettings {
        &self.settings
    }

    /// Return the selected credential revision without exposing any secret value; bundled packages have none.
    /// 返回所选凭据修订，不暴露任何秘密值；内建插件包没有凭据修订。
    pub fn credential_revision(&self) -> Option<&VulcanPluginCredentialRevision> {
        self.settings.credential_revision.as_ref()
    }

    /// Return the credential revision a managed generation must carry.
    /// 返回受管代次必须携带的凭据修订。
    pub(crate) fn generation_credential_revision(
        &self,
    ) -> VulcanPluginStoreResult<&VulcanPluginCredentialRevision> {
        self.credential_revision().ok_or_else(|| {
            VulcanPluginStoreError::invalid("Managed plugin selection has no credential revision.")
        })
    }

    /// Borrow the selected configuration.
    /// 借用所选配置。
    pub fn configuration(&self) -> &VulcanPluginConfigurationSelection {
        &self.settings.configuration
    }
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

impl VulcanPluginGenerationPhase {
    /// Sole declared phase list derives encoded checkpoint growth; it grants neither collection nor phase-transition authority.
    /// 唯一声明阶段列表推导编码检查点增长；不授予回收或阶段转换权威。
    const ENCODED_PHASES: [Self; 4] = [
        Self::Preparing,
        Self::Prepared,
        Self::Collecting,
        Self::Collected,
    ];
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
    /// Actual credential ownership for an active revision-bound choice; unbound choices and tombstones have none.
    /// 绑定修订的有效选择的实际凭据所有权；未绑定选择和停用记录没有此所有权。
    pub credentials: Option<PinnedVulcanPluginCredentials>,
}
