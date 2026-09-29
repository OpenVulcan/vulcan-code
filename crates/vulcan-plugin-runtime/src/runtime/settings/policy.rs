//! Durable capacity-policy intent shares the single authoritative settings document.
//! 持久容量策略意图与唯一权威设置文档共存。

use super::*;
use luaskills::runtime::embedded::EmbeddedCapacityConfig;
use std::fs::{File, OpenOptions};
mod prewarm;
#[cfg(test)]
mod prewarm_tests;
pub use prewarm::{
    VulcanPluginPrewarmPolicyPhase, VulcanPluginPrewarmPolicyRecord,
    VulcanPluginPrewarmPolicyStatus, VulcanPluginPrewarmPolicyUpdate,
};

/// The transaction container has its own format while its settings retain the strict current schema.
/// 事务容器拥有独立格式，内部设置仍使用严格当前 Schema。
pub(super) const DOCUMENT_VERSION: u32 = 6;
/// Every settings reader and policy writer uses this exact filename.
/// 全部设置读取器及策略写入器使用此精确文件名。
pub(in crate::runtime) const SETTINGS_FILE_NAME: &str = "plugin-runtime-settings.json";

/// Explicit caller-owned identity and predecessor select one existing native capacity.
/// 明确调用方身份及前驱选择一个既有原生容量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginCapacityPolicyUpdate {
    /// Caller-generated UUID remains queryable even if the submitting observer disappears.
    /// 调用方生成的 UUID 在提交观测者消失后仍可查询。
    pub update_id: String,
    /// Exact original runtime; a restarted runtime can never inherit this authorization.
    /// 精确原运行时；重启后的运行时绝不能继承此授权。
    pub runtime_id: String,
    /// Exact native capacity identity, independent of its semantic plugin name.
    /// 精确原生容量身份，独立于语义插件名称。
    pub capacity_id: String,
    /// Opaque native predecessor is echoed without numeric conversion or automatic refresh.
    /// 不透明原生前驱原样回传，不进行数值转换或自动刷新。
    pub expected_revision: String,
    /// Host-validated plugin ownership cannot be supplied by plugin business arguments.
    /// 宿主校验的插件归属不能由插件业务参数提供。
    pub plugin_id: String,
    /// One host routing purpose is revised per transaction.
    /// 每个事务修订一个宿主路由用途。
    pub purpose: VulcanPluginCapacityPurpose,
    /// Some replaces the entire override; None explicitly restores inherited plugin limits.
    /// Some 替换完整覆盖；None 明确恢复继承插件上限。
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub config: Option<EmbeddedCapacityConfig>,
}

impl VulcanPluginCapacityPolicyUpdate {
    /// Validate identities and derive complete candidate settings from current; return errors without mutation.
    /// 校验身份并从 current 派生完整候选设置；失败返回错误，不产生变更。
    pub(in crate::runtime) fn candidate(
        &self,
        current: &VulcanPluginRuntimeSettings,
    ) -> Result<VulcanPluginRuntimeSettings, String> {
        uuid::Uuid::parse_str(&self.update_id)
            .map_err(|_| "Capacity policy update_id must be a UUID.".to_owned())?;
        crate::catalog::validate_identifier(&self.plugin_id, "capacity policy owner")?;
        for identity in [&self.runtime_id, &self.capacity_id, &self.expected_revision] {
            if identity.is_empty() || identity.contains('\0') {
                return Err(
                    "Capacity policy identities must be nonempty and contain no zero byte.".into(),
                );
            }
        }
        // Only the selected purpose changes; parent, plugin aggregate and execution timeouts stay frozen.
        // 仅所选用途变化；父级、插件聚合及执行超时保持冻结。
        let mut candidate = current.clone();
        match &self.config {
            Some(config) => {
                candidate
                    .capacity_budgets
                    .entry(self.plugin_id.clone())
                    .or_default()
                    .insert(self.purpose, config.clone());
            }
            None => {
                if let Some(purposes) = candidate.capacity_budgets.get_mut(&self.plugin_id) {
                    purposes.remove(&self.purpose);
                    if purposes.is_empty() {
                        candidate.capacity_budgets.remove(&self.plugin_id);
                    }
                }
            }
        }
        candidate.validate()?;
        Ok(candidate)
    }
}

/// Acknowledged stages distinguish native application from durable configuration commit.
/// 已确认阶段区分原生应用与持久配置提交。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginCapacityPolicyPhase {
    /// Durable intent exists or its publication is still being recovered; native work is not acknowledged.
    /// 持久意图已存在或其发布仍待恢复；原生工作尚未确认。
    Prepared,
    /// Original native mutation succeeded; only durable confirmation may be resumed.
    /// 原原生变更成功；仅可恢复持久确认。
    Applied,
    /// Both native application and persistent settings publication were acknowledged.
    /// 原生应用及持久设置发布均已确认。
    Committed,
    /// Native application was explicitly rejected before changing the policy.
    /// 原生应用在改变策略前被明确拒绝。
    Rejected,
    /// Original runtime ownership ended before durable completion could be proved.
    /// 证明持久完成前，原运行时归属已经结束。
    Interrupted,
}

impl VulcanPluginCapacityPolicyPhase {
    /// Return whether explicit acknowledgement may release this terminal record.
    /// 返回显式确认是否可以释放此终态记录。
    pub(in crate::runtime) fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::Rejected | Self::Interrupted)
    }
}

/// One retained transaction is both the administrative receipt and restart evidence.
/// 单个保留事务同时作为管理回执与重启证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginCapacityPolicyRecord {
    /// Original immutable request is never rebound after conflict, timeout or restart.
    /// 原不可变请求在冲突、超时或重启后绝不重新绑定。
    pub request: VulcanPluginCapacityPolicyUpdate,
    /// Exact acknowledged checkpoint, not a prediction of future convergence.
    /// 精确已确认检查点，不预测未来收敛。
    pub phase: VulcanPluginCapacityPolicyPhase,
    /// Native acknowledgement is retained before attempting another fallible filesystem step.
    /// 尝试下一个可能失败的文件系统步骤前保留原生确认。
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub applied_revision: Option<String>,
    /// Latest explicit recovery or rejection diagnostic; business work is never replayed to erase it.
    /// 最新明确恢复或拒绝诊断；绝不通过重放业务来清除它。
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub error: Option<String>,
    /// Persistence uncertainty is separate from the original native refusal or recovery diagnostic.
    /// 持久性不确定独立于原原生拒绝或恢复诊断。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistence_error: Option<String>,
}

/// Version-six container commits settings and one mutually exclusive policy receipt in one replacement.
/// 第六版容器通过一次替换提交设置及单个互斥策略回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::runtime) struct PolicyDocument {
    /// Discriminator is checked before consuming either settings or transaction metadata.
    /// 消费设置或事务元数据前检查判别字段。
    pub schema_version: u32,
    /// Only durably committed settings initialize the next runtime.
    /// 仅持久提交的设置用于初始化下一个运行时。
    pub settings: VulcanPluginRuntimeSettings,
    /// At most one retained record bounds administrative history until explicit acknowledgement.
    /// 显式确认前最多保留一个记录，以限制管理历史。
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub update: Option<VulcanPluginCapacityPolicyRecord>,
    /// At most one policy kind is retained; explicit null means no automatic-prewarm policy receipt.
    /// 最多保留一种策略；明确空值表示没有自动预热策略回执。
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub prewarm_update: Option<VulcanPluginPrewarmPolicyRecord>,
}

impl PolicyDocument {
    /// Construct a current container from settings and update without writing user configuration.
    /// 从 settings 和 update 构造当前容器，不写入用户配置。
    pub(in crate::runtime) fn new(
        settings: VulcanPluginRuntimeSettings,
        update: Option<VulcanPluginCapacityPolicyRecord>,
    ) -> Self {
        Self {
            schema_version: DOCUMENT_VERSION,
            settings,
            update,
            prewarm_update: None,
        }
    }
}

/// Parse current container bytes strictly; return committed settings and preserved transaction evidence.
/// 严格解析当前容器字节；返回已提交设置及保留事务证据。
pub(super) fn decode_document(bytes: &str) -> Result<PolicyDocument, String> {
    // Decode original nested maps directly so duplicate fields remain visible to strict Serde.
    // 直接解码原嵌套映射，使重复字段继续对严格 Serde 可见。
    let document: PolicyDocument =
        serde_json::from_str(bytes).map_err(|error| error.to_string())?;
    validate_document(document)
}

/// Validate a current or strictly migrated document; return its unchanged committed evidence.
/// 校验当前或严格迁移的文档；返回其不变的已提交证据。
pub(super) fn validate_document(document: PolicyDocument) -> Result<PolicyDocument, String> {
    if document.schema_version != DOCUMENT_VERSION {
        return Err("Unsupported runtime policy document version.".into());
    }
    document.settings.validate()?;
    if document.update.is_some() && document.prewarm_update.is_some() {
        return Err("Only one policy transaction may be retained in the settings document.".into());
    }
    if let Some(record) = &document.prewarm_update {
        prewarm::validate_record(record, &document.settings)?;
    }
    if let Some(record) = &document.update {
        if record.persistence_error.is_some() {
            return Err(
                "A durable policy record cannot claim an unacknowledged persistence attempt."
                    .into(),
            );
        }
        record.request.candidate(&document.settings)?;
        // Applied is exclusively a live-memory checkpoint: on disk only final commit proves new settings.
        // Applied 仅为活动内存检查点；磁盘上仅最终提交证明新设置。
        if record.phase == VulcanPluginCapacityPolicyPhase::Applied
            || (record.phase == VulcanPluginCapacityPolicyPhase::Committed)
                != record
                    .applied_revision
                    .as_ref()
                    .is_some_and(|revision| !revision.is_empty() && !revision.contains('\0'))
            || (record.phase != VulcanPluginCapacityPolicyPhase::Committed
                && record.applied_revision.is_some())
        {
            return Err("Invalid runtime policy transaction checkpoint.".into());
        }
        if record.phase == VulcanPluginCapacityPolicyPhase::Committed
            && record.request.candidate(&document.settings)? != document.settings
        {
            return Err("Committed runtime policy differs from its retained request.".into());
        }
    }
    Ok(document)
}

/// Runtime-owned settings persistence keeps exact bytes and any ambiguous publication candidate.
/// 运行时拥有的设置持久化保留精确字节及任何发布不确定的候选。
pub(in crate::runtime) struct PolicyStore {
    /// Capacity settings follow native acknowledgement; automatic targets follow durable publication acknowledgement.
    /// 容量设置遵循原生确认；自动目标遵循持久发布确认。
    pub settings: VulcanPluginRuntimeSettings,
    /// One original receipt survives observers and failed persistence attempts.
    /// 单个原回执在观测者消失及持久尝试失败后保留。
    pub update: Option<VulcanPluginCapacityPolicyRecord>,
    /// Live predecessor changes once per acknowledged automatic policy publication and per attachment.
    /// 活动前驱在每次已确认自动策略发布及每次附加时改变一次。
    pub prewarm_revision: String,
    /// Mutually exclusive original automatic-policy receipt, separate from execution targets.
    /// 互斥的原自动策略回执，独立于执行目标。
    pub prewarm_update: Option<VulcanPluginPrewarmPolicyRecord>,
    /// Last disk image accepted under the writer lease, including explicit initial absence.
    /// 写入租约下最后接纳的磁盘映像，包含明确初始缺失。
    persisted: Option<Vec<u8>>,
    /// A failed atomic replace may have published exactly these bytes before directory synchronization failed.
    /// 原子替换失败可能已在目录同步失败前发布这些精确字节。
    attempted: Option<Vec<u8>>,
    /// The OS lease lasts until this native owner ends, preventing another live owner from recovering its intent.
    /// 操作系统租约持续至此原生所有者结束，防止另一活动所有者恢复其意图。
    lease: Option<File>,
    /// Test-only filesystem failure injection selects one actual publication boundary.
    /// 仅测试文件系统故障注入选择一个实际发布边界。
    #[cfg(test)]
    pub fault: Option<(VulcanPluginCapacityPolicyPhase, bool)>,
    /// Test-only automatic policy publication boundary injection.
    /// 仅测试自动策略发布边界注入。
    #[cfg(test)]
    pub prewarm_fault: Option<(VulcanPluginPrewarmPolicyPhase, bool)>,
}

impl PolicyStore {
    /// Load original disk evidence for settings at root; return a memory-only owner without acquiring a writer.
    /// 为 root 下的 settings 加载原磁盘证据；返回仅内存所有者，不取得写入租约。
    pub(in crate::runtime) fn load(
        root: &Path,
        settings: &VulcanPluginRuntimeSettings,
    ) -> Result<Self, String> {
        // Preserve original bytes as the compare-and-replace authority even after an in-memory version migration.
        // 即使进行内存版本迁移，也保留原字节作为比较替换权威。
        let persisted = read_image(&root.join(SETTINGS_FILE_NAME))?;
        let (update, prewarm_update) = if let Some(bytes) = &persisted {
            let document = version::decode_container(
                std::str::from_utf8(bytes).map_err(|error| error.to_string())?,
            )?;
            if document.settings != *settings {
                return Err("Runtime settings changed between loading and attachment.".into());
            }
            (document.update, document.prewarm_update)
        } else {
            (None, None)
        };
        Ok(Self {
            settings: settings.clone(),
            update,
            prewarm_update,
            prewarm_revision: uuid::Uuid::new_v4().to_string(),
            persisted,
            attempted: None,
            lease: None,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            prewarm_fault: None,
        })
    }

    /// Acquire root's permanent writer inode once; return busy without modifying configuration if another owner lives.
    /// 为 root 一次取得永久写入文件对象；另一所有者存活时返回忙碌，不修改配置。
    pub(in crate::runtime) fn acquire(&mut self, root: &Path) -> Result<(), String> {
        if self.lease.is_some() {
            return Ok(());
        }
        std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
        // The lock is a separate stable inode, because replacing the settings file changes its inode.
        // 锁使用独立稳定文件对象，因为替换设置文件会改变其文件对象。
        let path = root.join("plugin-runtime-settings.lock");
        check_regular(&path)?;
        // Opening never truncates another owner's lock or follows an accepted symbolic-link target.
        // 打开绝不截断另一所有者的锁，也不接纳符号链接目标。
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| error.to_string())?;
        check_regular(&path)?;
        file.try_lock()
            .map_err(|error| format!("Runtime policy writer is unavailable: {error}"))?;
        self.lease = Some(file);
        Ok(())
    }

    /// Persist document at root only against accepted original bytes; return publication or durability errors explicitly.
    /// 仅针对接纳的原字节在 root 持久化 document；明确返回发布或持久性错误。
    /// Failed replacement retains both known images; a later retry never overwrites unrelated external edits.
    /// 替换失败保留两个已知映像；后续重试绝不覆盖无关外部编辑。
    pub(in crate::runtime) fn persist(
        &mut self,
        root: &Path,
        document: &PolicyDocument,
    ) -> Result<(), String> {
        self.acquire(root)?;
        // Revalidate the complete wire image before attempting to publish a transaction checkpoint.
        // 尝试发布事务检查点前复核完整线映像。
        let bytes = serde_json::to_vec_pretty(document).map_err(|error| error.to_string())?;
        decode_document(std::str::from_utf8(&bytes).map_err(|error| error.to_string())?)?;
        // Two accepted images are permitted only after our own explicitly retained uncertain publication.
        // 仅在明确保留自身不确定发布之后，才允许两个接纳映像。
        let path = root.join(SETTINGS_FILE_NAME);
        let current = read_image(&path)?;
        if current != self.persisted && !(self.attempted.is_some() && current == self.attempted) {
            return Err("Runtime policy settings were edited by another writer; original bytes were preserved.".into());
        }
        // A prior uncertain write may now be observed; retain that exact image before attempting another replacement.
        // 此前不确定写入可能现已观测到；尝试另一次替换前保留该精确映像。
        self.persisted = current;
        self.attempted = Some(bytes.clone());
        // Injection affects only tests and never changes production filesystem ordering.
        // 注入仅影响测试，绝不改变生产文件系统顺序。
        #[cfg(test)]
        let fault = self.fault.filter(|(phase, _)| {
            document
                .update
                .as_ref()
                .is_some_and(|record| record.phase == *phase)
        });
        #[cfg(test)]
        if let Some((_, after_publication)) = fault {
            self.fault = None;
            if !after_publication {
                return Err("Injected failure before policy file replacement.".into());
            }
        }
        // Automatic policy tests use the same actual replacement boundary as capacity policy tests.
        // 自动策略测试与容量策略测试使用同一实际替换边界。
        #[cfg(test)]
        let prewarm_fault = self.prewarm_fault.filter(|(phase, _)| {
            document
                .prewarm_update
                .as_ref()
                .is_some_and(|record| record.phase == *phase)
        });
        #[cfg(test)]
        if let Some((_, after_publication)) = prewarm_fault {
            self.prewarm_fault = None;
            if !after_publication {
                return Err("Injected failure before prewarm policy replacement.".into());
            }
        }
        crate::atomic_file::write_file_atomically(&path, &bytes)?;
        #[cfg(test)]
        if fault.is_some() {
            return Err("Injected failure after policy file replacement.".into());
        }
        #[cfg(test)]
        if prewarm_fault.is_some() {
            return Err("Injected failure after prewarm policy replacement.".into());
        }
        self.persisted = Some(bytes);
        self.attempted = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

/// Decode an explicitly present nullable value of T from deserializer; missing fields remain errors.
/// 从 deserializer 解码明确存在的可空 T 值；字段缺失仍为错误。
/// Return the declared value or null without guessing reset, acknowledgement or diagnostic removal.
/// 返回声明值或空值，不猜测重置、确认或诊断移除。
pub(super) fn deserialize_required_nullable<
    'de,
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

/// Reject nonregular existing entries at path; absence remains an explicit supported state.
/// 拒绝 path 下的非普通既有条目；缺失仍为明确支持状态。
fn check_regular(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file() && !crate::metadata_is_link_or_reparse_point(&metadata) =>
        {
            Ok(())
        }
        Ok(_) => Err(format!(
            "Runtime policy entry is not a regular file: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// Read exact regular-file bytes from path; return None only for a missing document.
/// 从 path 读取精确普通文件字节；仅文档缺失时返回 None。
fn read_image(path: &Path) -> Result<Option<Vec<u8>>, String> {
    check_regular(path)?;
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
