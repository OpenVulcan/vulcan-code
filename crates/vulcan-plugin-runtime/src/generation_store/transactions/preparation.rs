//! Typed early intent precedes physical preparation without inventing a configured activation.
//! 类型化早期意图先于物理准备，不虚构已配置激活。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};

mod operations;

/// Immutable identities approved before any package copy or secure-store staging begins.
/// 在任何插件包复制或安全存储暂存开始前批准的不可变身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginActivationPreparationIntent {
    /// Exact physical package allocation, distinct from a claim that its bytes already exist.
    /// 精确物理插件包分配，不表示其字节已存在。
    generation: VulcanPluginGenerationIdentity,
    /// Planned secure-store revision is never substituted after uncertain writes.
    /// 安全存储计划修订在写入不确定后绝不被替换。
    credential_revision: VulcanPluginCredentialRevision,
    /// Exact sorted permissions approved for the source fingerprint.
    /// 为来源指纹批准的精确有序权限。
    approved_permissions: Vec<String>,
    /// Exact successor revision is reserved against the complete original predecessor.
    /// 相对完整原前驱预留的精确后继修订。
    activation_revision: u64,
    /// Version three owns the original user policy; earlier formats must omit this field.
    /// 第三版持有原用户策略；较早格式必须省略此字段。
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    update_policy: Option<VulcanPluginUpdatePolicy>,
}

impl VulcanPluginActivationPreparationIntent {
    /// Borrow the original policy without consulting the candidate manifest or current user settings.
    /// 借用原策略，不查询候选清单或当前用户设置。
    pub fn update_policy(&self) -> Option<&VulcanPluginUpdatePolicy> {
        self.update_policy.as_ref()
    }

    /// Validate field presence against declared `schema_version`; return an error instead of silently losing policy.
    /// 按声明 `schema_version` 校验字段存在性；返回错误而非静默丢失策略。
    pub(super) fn validate_schema(&self, schema_version: u32) -> VulcanPluginStoreResult<()> {
        match (schema_version, self.update_policy.is_some()) {
            (PREPARATION_RECORD_VERSION, false) | (UPDATE_RECORD_VERSION, true) => Ok(()),
            _ => Err(Error::invalid(
                "Preparation policy does not match its declared schema.",
            )),
        }
    }

    /// Return the original planned package identity without claiming a live lease.
    /// 返回原计划插件包身份，不声称存在活动租约。
    pub fn generation(&self) -> &VulcanPluginGenerationIdentity {
        &self.generation
    }
    /// Return the original secure-store revision without exposing secret values.
    /// 返回原安全存储修订，不暴露秘密值。
    pub fn credential_revision(&self) -> &VulcanPluginCredentialRevision {
        &self.credential_revision
    }
    /// Return the exact planned activation successor.
    /// 返回精确计划激活后继。
    pub fn activation_revision(&self) -> u64 {
        self.activation_revision
    }
    /// Compare `target` with all identities fixed before preparation; configuration must be validated from its pinned copy.
    /// 比较 `target` 与准备前固定的全部身份；配置必须按其固定副本校验。
    pub(super) fn matches_target(&self, target: &VulcanPluginActivation) -> bool {
        target.generation.as_ref() == Some(&self.generation)
            && target.credential_revision() == Some(&self.credential_revision)
            && target.approved_permissions() == Some(self.approved_permissions.as_slice())
            && target.revision == self.activation_revision
            && target.plugin_id == self.generation.plugin_id
    }
}

/// Original request before a complete configured target exists; terminal abandonment retains this evidence.
/// 完整已配置目标存在前的原请求；终态放弃保留此证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginActivationPreparationRecord {
    /// Explicit preparation journal schema determines whether original update policy is required.
    /// 明确准备日志模式决定是否必须包含原更新策略。
    schema_version: u32,
    /// Canonical original request also names the actual exclusive lock.
    /// 规范原请求同时命名实际排他锁。
    #[serde(deserialize_with = "records::deserialize_request_id")]
    request_id: Uuid,
    /// Complete original predecessor; null explicitly represents no managed selection.
    /// 完整原前驱；空值明确表示无受管选择。
    #[serde(deserialize_with = "records::deserialize_predecessor")]
    predecessor: Option<VulcanPluginActivation>,
    /// Frozen resource identities are not an executable target.
    /// 固定资源身份不是可执行目标。
    preparation: VulcanPluginActivationPreparationIntent,
    /// Only preparing and aborted are valid without a configured target.
    /// 无已配置目标时仅准备中及已中止有效。
    phase: VulcanPluginActivationTransactionPhase,
}

/// Typed original request projection keeps preparation separate from an actual configured activation.
/// 类型化原请求投影将准备与实际已配置激活区分。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VulcanPluginActivationRequestRecord {
    /// No complete activation target exists yet.
    /// 尚不存在完整激活目标。
    Preparation(VulcanPluginActivationPreparationRecord),
    /// The immutable configured target is available under the same original identity.
    /// 相同原身份下已具有不可变已配置目标。
    Activation(VulcanPluginActivationTransactionRecord),
}

impl VulcanPluginActivationRequestRecord {
    /// Borrow the original policy in either preparation or configured stages; older records return none.
    /// 在准备或已配置阶段借用原策略；旧记录返回空值。
    pub fn update_policy(&self) -> Option<&VulcanPluginUpdatePolicy> {
        self.preparation().and_then(|intent| intent.update_policy())
    }

    /// Return the canonical original request UUID for either declared stage.
    /// 返回任一声明阶段的规范原请求 UUID。
    pub fn request_id(&self) -> Uuid {
        match self {
            Self::Preparation(record) => record.request_id,
            Self::Activation(record) => record.request_id,
        }
    }
    /// Return the declared durable phase without inferring execution or readiness.
    /// 返回声明持久阶段，不推断执行或就绪。
    pub fn phase(&self) -> VulcanPluginActivationTransactionPhase {
        match self {
            Self::Preparation(record) => record.phase,
            Self::Activation(record) => record.phase,
        }
    }
    /// Borrow the complete predecessor, returning none only for an originally unmanaged namespace.
    /// 借用完整前驱，仅原无受管选择的命名空间返回空值。
    pub fn predecessor(&self) -> Option<&VulcanPluginActivation> {
        match self {
            Self::Preparation(record) => record.predecessor.as_ref(),
            Self::Activation(record) => record.predecessor(),
        }
    }
    /// Borrow an actual configured target only after its explicit preparation transition.
    /// 仅在明确准备转换后借用实际已配置目标。
    pub fn target(&self) -> Option<&VulcanPluginActivation> {
        match self {
            Self::Preparation(_) => None,
            Self::Activation(record) => Some(record.target()),
        }
    }
    /// Borrow the planned or completed immutable candidate; only unvalidated externally decoded records may lack it.
    /// 借用计划中或已完成的不可变候选；仅外部解码且未经校验的记录可能缺失。
    pub fn candidate(&self) -> Option<&VulcanPluginGenerationIdentity> {
        match self {
            Self::Preparation(record) => Some(&record.preparation.generation),
            Self::Activation(record) => record.target.generation.as_ref(),
        }
    }
    /// Borrow the planned or selected credential revision without claiming that staging is complete.
    /// 借用计划中或已选凭据修订，不声称暂存已经完成。
    pub fn credential_revision(&self) -> Option<&VulcanPluginCredentialRevision> {
        match self {
            Self::Preparation(record) => Some(&record.preparation.credential_revision),
            Self::Activation(record) => record.target.credential_revision(),
        }
    }
    /// Return the reserved successor revision independently of target readiness.
    /// 独立于目标就绪状态返回预留后继修订。
    pub fn activation_revision(&self) -> u64 {
        match self {
            Self::Preparation(record) => record.preparation.activation_revision,
            Self::Activation(record) => record.target.revision,
        }
    }
    /// Borrow the optional early intent; legacy records explicitly have none.
    /// 借用可选早期意图；旧格式记录明确没有该意图。
    fn preparation(&self) -> Option<&VulcanPluginActivationPreparationIntent> {
        match self {
            Self::Preparation(record) => Some(&record.preparation),
            Self::Activation(record) => record.preparation.as_ref(),
        }
    }
    /// Check that `current` preserves this original identity and every target already observed.
    /// 检查 `current` 保留本原身份及全部已观察目标。
    pub fn preserves_original(&self, current: &Self) -> bool {
        self.request_id() == current.request_id()
            && self.predecessor() == current.predecessor()
            && self.preparation() == current.preparation()
            && match self.target() {
                Some(target) => current.target() == Some(target),
                None => true,
            }
    }
}

/// Exclusive early owner survives physical preparation and transfers the same lock to the ready transaction.
/// 早期排他所有者跨物理准备存续，并将同一锁转交给就绪事务。
#[derive(Debug)]
pub struct VulcanPluginActivationPreparation {
    /// Canonical original store binding.
    /// 规范原存储绑定。
    store_root: PathBuf,
    /// Last confirmed early record, never reconstructed from directory order.
    /// 最后确认的早期记录，绝不按目录顺序重建。
    record: VulcanPluginActivationPreparationRecord,
    /// Actual exclusive original-request ownership transfers without an unlock gap.
    /// 实际原请求排他所有权在无解锁间隙下转交。
    lease: Arc<File>,
}

impl VulcanPluginActivationPreparation {
    /// Borrow the frozen intent for exact resource preparation; this grants no plugin execution.
    /// 借用固定意图用于精确资源准备；不授予插件执行权。
    pub fn intent(&self) -> &VulcanPluginActivationPreparationIntent {
        &self.record.preparation
    }
    /// Return the original request identity to preserve after any uncertain result.
    /// 返回原请求身份，以便在任何不确定结果后保留。
    pub fn request_id(&self) -> Uuid {
        self.record.request_id
    }
}

/// One strict wire decoder uses declared schema and field presence, never trial parsing alternate formats.
/// 单个严格线格式解码器使用声明模式及字段存在性，绝不试读备用格式。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRequest {
    /// Explicit record version selects the allowed field set.
    /// 明确记录版本选择允许字段集合。
    schema_version: u32,
    /// Canonical identity must match the literal filename.
    /// 规范身份必须匹配字面文件名。
    #[serde(deserialize_with = "records::deserialize_request_id")]
    request_id: Uuid,
    /// Required predecessor field distinguishes absence from explicit unmanaged state.
    /// 必需前驱字段区分缺失与明确无受管状态。
    #[serde(deserialize_with = "records::deserialize_predecessor")]
    predecessor: Option<VulcanPluginActivation>,
    /// A configured target is present only after preparation completes.
    /// 已配置目标仅在准备完成后存在。
    #[serde(default, deserialize_with = "deserialize_present")]
    target: Option<VulcanPluginActivation>,
    /// Version two onward carries a frozen early intent.
    /// 第二版及后续版本携带固定早期意图。
    #[serde(default, deserialize_with = "deserialize_present")]
    preparation: Option<VulcanPluginActivationPreparationIntent>,
    /// Explicit checkpoint determines whether target absence is valid.
    /// 明确检查点决定目标缺失是否有效。
    phase: VulcanPluginActivationTransactionPhase,
}

/// Decode a present `T` from `deserializer`; explicit null is invalid while omitted optional fields use defaults.
/// 从 `deserializer` 解码存在的 `T`；显式空值无效，省略的可选字段使用默认值。
pub(super) fn deserialize_present<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

impl VulcanPluginGenerationStore {
    /// Query either declared stage by exact `plugin_id` and `request_id`; return no ownership or execution grant.
    /// 按精确 `plugin_id` 及 `request_id` 查询任一声明阶段；不授予所有权或执行权。
    pub fn activation_request(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginActivationRequestRecord>> {
        // Inspection shares the existing namespace fence with every original selection writer.
        // 检查与全部原选择写入者共用既有命名空间屏障。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        self.read_request_locked(plugin_id, request_id)
    }
    /// Read exact `plugin_id`/`request_id` under its caller-held namespace fence; return only real absence.
    /// 在调用方命名空间屏障下读取精确 `plugin_id`／`request_id`；仅返回实际缺失。
    pub(super) fn read_request_locked(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginActivationRequestRecord>> {
        if request_id.is_nil() {
            return Err(Error::invalid("Activation request identity cannot be nil."));
        }
        // Discovery never creates missing journal directories.
        // 发现绝不创建缺失日志目录。
        let Some(directory) = self.transaction_directory(plugin_id, false)? else {
            return Ok(None);
        };
        // Existing journal budgets cover both stage shapes without a second independently defined limit.
        // 既有日志预算覆盖两种阶段形状，不另行定义独立限制。
        let Some(bytes) = files::read_bytes(
            &directory.join(format!("{request_id}.json")),
            MAX_TRANSACTION_BYTES,
        )?
        else {
            return Ok(None);
        };
        // Decoder details are private because configured targets contain user documents.
        // 解码器细节保持私密，因为已配置目标包含用户文档。
        let wire: StoredRequest = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid activation request record."))?;
        match (wire.schema_version, wire.target, wire.preparation) {
            (PREPARATION_RECORD_VERSION | UPDATE_RECORD_VERSION, None, Some(preparation)) => {
                // Absence of a target is allowed only for the explicitly typed preparation phase.
                // 仅明确类型化准备阶段允许目标缺失。
                let record = VulcanPluginActivationPreparationRecord {
                    schema_version: wire.schema_version,
                    request_id: wire.request_id,
                    predecessor: wire.predecessor,
                    preparation,
                    phase: wire.phase,
                };
                Self::validate_preparation_record(&record, plugin_id, request_id)?;
                Ok(Some(VulcanPluginActivationRequestRecord::Preparation(
                    record,
                )))
            }
            (
                TRANSACTION_RECORD_VERSION | PREPARATION_RECORD_VERSION | UPDATE_RECORD_VERSION,
                Some(target),
                preparation,
            ) => {
                // Complete records retain the old typed target API, including legacy version-one bytes.
                // 完整记录保留旧类型化目标接口，包括旧第一版字节。
                let record = VulcanPluginActivationTransactionRecord {
                    schema_version: wire.schema_version,
                    request_id: wire.request_id,
                    predecessor: wire.predecessor,
                    target,
                    preparation,
                    phase: wire.phase,
                };
                Self::validate_transaction_record(&record, plugin_id, request_id)?;
                Ok(Some(VulcanPluginActivationRequestRecord::Activation(
                    record,
                )))
            }
            _ => Err(Error::invalid(
                "Activation request version and stage fields are inconsistent.",
            )),
        }
    }

    /// Validate early `record` against exact namespace and request identity before persistence or recovery.
    /// 持久保存或恢复前，按精确命名空间及请求身份校验早期 `record`。
    fn validate_preparation_record(
        record: &VulcanPluginActivationPreparationRecord,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<()> {
        if !matches!(
            record.schema_version,
            PREPARATION_RECORD_VERSION | UPDATE_RECORD_VERSION
        ) || record.request_id != request_id
            || !matches!(
                record.phase,
                VulcanPluginActivationTransactionPhase::Preparing
                    | VulcanPluginActivationTransactionPhase::Aborted
            )
        {
            return Err(Error::invalid(
                "Invalid early activation request identity or phase.",
            ));
        }
        record.preparation.validate_schema(record.schema_version)?;
        Self::validate_preparation_intent(
            &record.preparation,
            record.predecessor.as_ref(),
            plugin_id,
        )
    }

    /// Validate frozen `intent` and complete `predecessor` for `plugin_id`; return no resource allocation.
    /// 为 `plugin_id` 校验固定 `intent` 及完整 `predecessor`；不分配资源。
    pub(super) fn validate_preparation_intent(
        intent: &VulcanPluginActivationPreparationIntent,
        predecessor: Option<&VulcanPluginActivation>,
        plugin_id: &str,
    ) -> VulcanPluginStoreResult<()> {
        files::validate_identity(&intent.generation)?;
        if intent.generation.plugin_id != plugin_id
            || predecessor
                .map_or(0, |record| record.revision)
                .checked_add(1)
                != Some(intent.activation_revision)
            || intent
                .approved_permissions
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err(Error::invalid(
                "Early activation identities or permissions are inconsistent.",
            ));
        }
        if let Some(predecessor) = predecessor {
            // Retain the existing nested activation format and budget checks.
            // 保留既有嵌套激活格式及预算检查。
            let size = serde_json::to_vec(predecessor)
                .map_err(|_| Error::invalid("Cannot encode predecessor."))?
                .len() as u64;
            Self::validate_activation_record(predecessor, plugin_id, size)?;
        }
        Ok(())
    }
}
