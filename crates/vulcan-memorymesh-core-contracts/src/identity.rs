//! Unified persistent mutation identity, entity slots, and commit receipts.
//! 统一持久变更身份、实体槽位与提交收据。

use crate::{
    ConfigSnapshotId, CoreError, ErrorCode, ErrorStage, IdempotencyKey, MemoryId, OperationId,
    ScoreContractRef, StructuredOutputContractId, TurnId,
};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

/// Identifies one command-local slot that references a not-yet-committed entity.
/// 标识一个引用尚未提交实体的命令局部槽位。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct EntitySlotId(String);

impl EntitySlotId {
    /// Creates one slot identity after rejecting empty, oversized, or NUL values.
    /// 在拒绝空值、超长值或 NUL 后创建槽位标识。
    ///
    /// # Errors
    /// Returns an error when the slot identity is empty, longer than 128 bytes, or contains NUL.
    /// 当槽位标识为空、超过 128 字节或包含 NUL 时返回错误。
    pub fn new(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.is_empty() {
            return Err(identity_error(
                ErrorCode::InvalidIdentifier,
                "entity slot id is empty",
            ));
        }
        if value.len() > 128 {
            return Err(identity_error(
                ErrorCode::InvalidIdentifier,
                "entity slot id exceeds 128 bytes",
            ));
        }
        if value.contains('\0') {
            return Err(identity_error(
                ErrorCode::InvalidIdentifier,
                "entity slot id contains NUL",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the exact slot identity text.
    /// 借用精确的槽位标识文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EntitySlotId {
    /// Formats the slot identity without changing its value.
    /// 在不改变值的前提下格式化槽位标识。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for EntitySlotId {
    /// Deserializes one slot through the same closed validation as `new`.
    /// 通过与 `new` 相同的封闭校验反序列化一个槽位。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Classifies the persistent caller origin of one mutating request.
/// 对一个变更请求的持久调用方来源分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MutationSourceKind {
    /// Indicates a host RPC entry with a caller-provided durable idempotency key.
    /// 表示由调用方提供持久幂等键的宿主 RPC 入口。
    HostRpc = 1,
    /// Indicates a host background worker with a durable derived identity.
    /// 表示由宿主持久派生身份的后台 Worker。
    BackgroundWorker = 2,
    /// Indicates a validated portable import pipeline.
    /// 表示经过校验的可移植导入管道。
    Import = 3,
    /// Indicates a caller that cannot supply a durable idempotency key.
    /// 表示无法提供持久幂等键的调用方。
    InvocationScoped = 4,
}

/// Carries the complete persistent operation identity supplied by the host.
/// 携带由宿主提供的完整持久操作身份。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationIdentity {
    /// Identifies the persistent operation supplied by the host.
    /// 标识由宿主提供的持久操作。
    pub operation_id: OperationId,
    /// Identifies the durable caller retry reconciliation key.
    /// 标识持久调用方重试对账键。
    pub idempotency_key: IdempotencyKey,
    /// Carries the canonical semantic request fingerprint computed over the typed command.
    /// 携带基于强类型命令计算的规范语义请求指纹。
    pub request_fingerprint: String,
    /// Classifies the persistent caller origin.
    /// 对持久调用方来源分类。
    pub source_kind: MutationSourceKind,
    /// Identifies the exact caller source when known.
    /// 在已知时标识精确调用方来源。
    pub request_source_id: String,
    /// Carries the host observation time excluded from the fingerprint.
    /// 携带不参与指纹的宿主观测时间。
    pub observed_at_ns: i64,
}

impl MutationIdentity {
    /// Validates structural identity fields without recomputing the fingerprint.
    /// 在不重算指纹的前提下校验结构性身份字段。
    ///
    /// # Errors
    /// Returns an error when the fingerprint text or source identity is malformed.
    /// 当指纹文本或来源身份格式错误时返回错误。
    pub fn validate(&self) -> Result<(), CoreError> {
        let fingerprint = self.request_fingerprint.trim();
        if fingerprint.len() != 70 || !fingerprint.starts_with("fp256:") {
            return Err(identity_error(
                ErrorCode::InvalidArgument,
                "request fingerprint must be a 64-hex fp256 digest",
            ));
        }
        if fingerprint[6..]
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit())
        {
            return Err(identity_error(
                ErrorCode::InvalidArgument,
                "request fingerprint digest is not hexadecimal",
            ));
        }
        if self.request_source_id.len() > 512 {
            return Err(identity_error(
                ErrorCode::InvalidArgument,
                "request source id exceeds 512 bytes",
            ));
        }
        Ok(())
    }
}

/// Derives one role-isolated mutation identity from a durable host root identity.
/// 从一个耐久宿主根身份派生按角色隔离的变更身份。
///
/// # Parameters
/// 参数
/// `identity` is the exact host root identity and `role` is the stable Core mutation role.
/// `identity` 是准确宿主根身份，`role` 是稳定 Core 变更角色。
///
/// # Returns
/// 返回值
/// A deterministic sub-operation identity that preserves request and source semantics.
/// 返回保留请求与来源语义的确定性子操作身份。
///
/// # Errors
/// Returns an identifier error if a derived operation or idempotency identity is invalid.
/// 派生操作或幂等身份无效时返回标识错误。
pub fn derive_mutation_identity(
    identity: &MutationIdentity,
    role: &str,
) -> Result<MutationIdentity, CoreError> {
    Ok(MutationIdentity {
        operation_id: OperationId::new(mutation_sub_identity(
            "op",
            identity.operation_id.as_str(),
            role,
        ))
        .map_err(|error| identity_error(ErrorCode::Internal, error.to_string()))?,
        idempotency_key: IdempotencyKey::new(mutation_sub_identity(
            "key",
            identity.idempotency_key.as_str(),
            role,
        ))
        .map_err(|error| identity_error(ErrorCode::Internal, error.to_string()))?,
        request_fingerprint: identity.request_fingerprint.clone(),
        source_kind: identity.source_kind,
        request_source_id: identity.request_source_id.clone(),
        observed_at_ns: identity.observed_at_ns,
    })
}

/// Hashes length-prefixed namespace, root, and role into one stable Core sub-identity.
/// 将带长度前缀的命名域、根身份与角色哈希为一个稳定 Core 子身份。
fn mutation_sub_identity(namespace: &str, root: &str, role: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"vmm-sub-operation-v1");
    for field in [namespace, root, role] {
        digest.update((field.len() as u64).to_le_bytes());
        digest.update(field.as_bytes());
    }
    format!("vmm-{namespace}-v1:{}", hex::encode(digest.finalize()))
}

/// References one entity as an exact existing record or a command-local new slot.
/// 以精确既有记录或命令局部新槽位的方式引用一个实体。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityRef {
    /// References one exact existing domain entity with its expected version.
    /// 以期望版本引用一个精确既有的领域实体。
    Existing {
        /// Identifies the existing domain entity.
        /// 标识既有领域实体。
        domain_id: String,
        /// Carries the required optimistic version.
        /// 携带必需的乐观版本。
        expected_version: u64,
    },
    /// References one command-local slot bound to a real identity at commit time.
    /// 引用一个在提交时绑定真实身份的命令局部槽位。
    New(EntitySlotId),
}

/// Classifies the durable kind of one bound entity.
/// 对一个已绑定实体的持久类别分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum EntityKind {
    /// Indicates a canonical turn entity.
    /// 表示规范化对话轮次实体。
    Turn = 1,
    /// Indicates a durable memory entity.
    /// 表示持久记忆实体。
    Memory = 2,
    /// Indicates a profile node entity.
    /// 表示画像节点实体。
    ProfileNode = 3,
    /// Indicates a context evidence entity.
    /// 表示情境证据实体。
    ContextEvidence = 4,
}

/// Reports one stable slot-to-domain-identity binding produced by an atomic commit.
/// 报告一次原子提交产生的稳定槽位到领域身份绑定。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityBinding {
    /// Identifies the command-local slot that was bound.
    /// 标识被绑定的命令局部槽位。
    pub entity_slot_id: EntitySlotId,
    /// Classifies the bound entity kind.
    /// 对绑定实体类别分类。
    pub entity_kind: EntityKind,
    /// Carries the host-assigned durable domain identity.
    /// 携带由宿主分配的持久领域身份。
    pub domain_id: String,
}

/// Reports one final committed version for an exact entity.
/// 报告一个精确实体的最终已提交版本。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionedEntity {
    /// Classifies the entity kind.
    /// 对实体类别分类。
    pub entity_kind: EntityKind,
    /// Identifies the durable domain entity.
    /// 标识持久领域实体。
    pub domain_id: String,
    /// Carries the version after the atomic commit.
    /// 携带原子提交后的版本。
    pub version: u64,
}

/// Builds the canonical durable identity of one composite context-evidence entity.
/// 构建一条复合情境证据实体的规范持久身份。
///
/// Length prefixes use UTF-8 byte counts so arbitrary normalized label text cannot create
/// ambiguous receipt identities, even when it contains separators.
/// 长度前缀使用 UTF-8 字节数，因此任意规范化标签文本即使包含分隔符也不会产生歧义收据身份。
#[must_use]
pub fn context_evidence_domain_id(
    memory_id: &MemoryId,
    context_key: &str,
    context_value: &str,
) -> String {
    let memory_id = memory_id.as_str();
    format!(
        "ce:v1:{}:{memory_id}{}:{context_key}{}:{context_value}",
        memory_id.len(),
        context_key.len(),
        context_value.len()
    )
}

/// Carries the durable all-or-nothing commit receipt of one mutation batch.
/// 携带一个变更批次的持久全有或全无提交收据。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitReceipt {
    /// Identifies the committed persistent operation.
    /// 标识已提交的持久操作。
    pub operation_id: OperationId,
    /// Identifies the durable idempotency key of the committed batch.
    /// 标识已提交批次的持久幂等键。
    pub idempotency_key: IdempotencyKey,
    /// Carries the canonical request fingerprint recorded at commit time.
    /// 携带提交时记录的规范请求指纹。
    pub request_fingerprint: String,
    /// Carries every slot-to-domain binding established by this commit.
    /// 携带本次提交建立的每个槽位到领域绑定。
    pub entity_bindings: Vec<EntityBinding>,
    /// Carries final versions for every entity this commit created or advanced.
    /// 携带本次提交创建或推进的每个实体的最终版本。
    pub committed_versions: Vec<VersionedEntity>,
    /// Summarizes durable side effects for audit without leaking payloads.
    /// 为审计总结持久副作用且不泄露载荷。
    pub side_effect_summary: String,
}

/// Binds one analysis attempt to its exact durable recovery identity.
/// 将一次分析尝试绑定到其精确持久恢复身份。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisAttemptIdentity {
    /// Identifies the pending turn under analysis.
    /// 标识正在分析的待处理轮次。
    pub turn_id: TurnId,
    /// Carries the explicit analysis generation supplied by the host.
    /// 携带由宿主提供的显式分析代次。
    pub analysis_generation: u64,
    /// Carries the persistent mutation identity of this attempt.
    /// 携带本次尝试的持久变更身份。
    pub mutation_identity: MutationIdentity,
    /// Identifies the exact immutable configuration snapshot bound to this attempt.
    /// 标识绑定到本次尝试的精确不可变配置快照。
    pub config_snapshot_id: ConfigSnapshotId,
    /// Carries the closed structured-output contract set used by this attempt.
    /// 携带本次尝试使用的封闭结构化输出契约集合。
    pub structured_output_contracts: Vec<StructuredOutputContractId>,
    /// Carries the declared score-contract identities accepted by this attempt.
    /// 携带本次尝试接受的已声明评分契约身份。
    pub score_contracts: Vec<ScoreContractRef>,
}

/// Creates one validation-stage identity error.
/// 创建一个校验阶段身份错误。
fn identity_error(code: ErrorCode, diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(code, ErrorStage::Validation, diagnostic)
}

#[cfg(test)]
mod tests {
    use super::{EntitySlotId, MutationIdentity, MutationSourceKind, derive_mutation_identity};
    use crate::{IdempotencyKey, OperationId};
    use serde::Deserialize;
    use serde::de::value::{Error as ValueError, StringDeserializer};

    /// Verifies deserialization cannot bypass entity-slot invariants.
    /// 验证反序列化无法绕过实体槽位不变量。
    #[test]
    fn entity_slot_deserialization_reuses_validation() {
        let empty = EntitySlotId::deserialize(StringDeserializer::<ValueError>::new(String::new()));
        assert!(empty.is_err());
        let oversized =
            EntitySlotId::deserialize(StringDeserializer::<ValueError>::new("x".repeat(129)));
        assert!(oversized.is_err());
        let nul = EntitySlotId::deserialize(StringDeserializer::<ValueError>::new(
            "slot\0invalid".to_owned(),
        ));
        assert!(nul.is_err());
    }

    /// Verifies derived identities are deterministic and isolated by mutation role.
    /// 验证派生身份具有确定性并按变更角色隔离。
    #[test]
    fn mutation_identity_derivation_is_deterministic_and_role_isolated() {
        let root = MutationIdentity {
            operation_id: OperationId::new("root-operation").expect("valid operation"),
            idempotency_key: IdempotencyKey::new("root-key").expect("valid key"),
            request_fingerprint: format!("fp256:{}", "0".repeat(64)),
            source_kind: MutationSourceKind::HostRpc,
            request_source_id: String::from("test"),
            observed_at_ns: 42,
        };
        let first = derive_mutation_identity(&root, "intake").expect("derived identity");
        let repeated = derive_mutation_identity(&root, "intake").expect("repeated identity");
        let other = derive_mutation_identity(&root, "analysis").expect("other identity");

        assert_eq!(first, repeated);
        assert_ne!(first.operation_id, other.operation_id);
        assert_ne!(first.idempotency_key, other.idempotency_key);
        assert_eq!(first.request_fingerprint, root.request_fingerprint);
        assert_eq!(first.source_kind, root.source_kind);
        assert_ne!(
            super::mutation_sub_identity("op", "a#b", "c"),
            super::mutation_sub_identity("op", "a", "b#c")
        );
    }
}
