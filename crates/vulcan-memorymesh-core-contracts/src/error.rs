//! Stable error catalog and diagnostic contracts.
//! 稳定错误目录与诊断契约。

use crate::{CommandId, ConfigSnapshotId, IdempotencyKey};
use serde::{Deserialize, Serialize};

/// Defines stable machine-readable error codes.
/// 定义稳定的机器可读错误码。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ErrorCode {
    /// Indicates invalid caller input.
    /// 表示调用方输入无效。
    InvalidArgument = 1000,
    /// Indicates an identifier contract violation.
    /// 表示标识符契约违规。
    InvalidIdentifier = 1001,
    /// Indicates an invalid configuration resource.
    /// 表示配置资源无效。
    InvalidConfiguration = 1100,
    /// Indicates a missing configuration resource.
    /// 表示缺少配置资源。
    MissingResource = 1101,
    /// Indicates a resource digest mismatch.
    /// 表示资源摘要不匹配。
    ResourceDigestMismatch = 1102,
    /// Indicates an unknown configuration snapshot.
    /// 表示配置快照未知。
    SnapshotNotFound = 1103,
    /// Indicates an unknown command.
    /// 表示命令未知。
    CommandNotFound = 1200,
    /// Indicates an effect result that does not match the pending effect.
    /// 表示 Effect 结果与待处理 Effect 不匹配。
    EffectMismatch = 1201,
    /// Indicates a Host effect failure for which this path cannot complete through degradation.
    /// 表示宿主 Effect 失败且当前路径不能通过降级完成。
    EffectDegraded = 1202,
    /// Indicates an operation is invalid for an already completed or failed command.
    /// 表示对已完成或已失败命令执行了不允许的操作。
    CommandCompleted = 1203,
    /// Indicates a command was cancelled.
    /// 表示命令已取消。
    CommandCancelled = 1204,
    /// Indicates a storage version conflict.
    /// 表示存储版本冲突。
    StorageConflict = 1300,
    /// Indicates an uncertain storage commit result.
    /// 表示存储提交结果不确定。
    StorageResultUncertain = 1301,
    /// Indicates a prohibited scope access.
    /// 表示禁止的作用域访问。
    ScopeViolation = 1302,
    /// Indicates one idempotency key reused with a different request fingerprint.
    /// 表示一个幂等键被不同请求指纹复用。
    IdempotencyConflict = 1303,
    /// Indicates a malformed persistent mutation identity.
    /// 表示格式错误的持久变更身份。
    MutationIdentityInvalid = 1304,
    /// Indicates one analysis attempt generation bound to a different identity.
    /// 表示一个分析代次绑定了不同身份。
    AnalysisAttemptConflict = 1305,
    /// Indicates malformed binary data.
    /// 表示二进制数据格式错误。
    InvalidBinaryFormat = 1400,
    /// Indicates an unsupported binary schema version.
    /// 表示不支持的二进制 Schema 版本。
    UnsupportedSchemaVersion = 1401,
    /// Indicates an incompatible embedding vector space.
    /// 表示嵌入向量空间不兼容。
    IncompatibleVectorSpace = 1402,
    /// Indicates an incompatible checkpoint that must not be restored.
    /// 表示不可恢复的不兼容检查点。
    CheckpointIncompatible = 1403,
    /// Indicates the exact checkpoint-bound snapshot is unavailable.
    /// 表示检查点绑定的精确快照不可用。
    CheckpointSnapshotUnavailable = 1404,
    /// Indicates a declared score contract was violated.
    /// 表示声明的评分契约被违反。
    ScoreContractViolation = 1405,
    /// Indicates a structured-output contract violation.
    /// 表示结构化输出契约违规。
    StructuredOutputContractViolation = 1406,
    /// Indicates an ABI contract violation.
    /// 表示 ABI 契约违规。
    AbiViolation = 1500,
    /// Indicates an internal panic isolated at the ABI boundary.
    /// 表示在 ABI 边界隔离的内部 panic。
    PanicIsolated = 1501,
    /// Indicates an unexpected internal invariant failure.
    /// 表示意外的内部不变量失败。
    Internal = 1999,
}

/// Identifies the processing stage where an error occurred.
/// 标识错误发生的处理阶段。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ErrorStage {
    /// Indicates input validation.
    /// 表示输入校验阶段。
    Validation = 1,
    /// Indicates configuration compilation.
    /// 表示配置编译阶段。
    Configuration = 2,
    /// Indicates command execution.
    /// 表示命令执行阶段。
    Command = 3,
    /// Indicates host effect processing.
    /// 表示宿主 Effect 处理阶段。
    Effect = 4,
    /// Indicates storage processing.
    /// 表示存储处理阶段。
    Storage = 5,
    /// Indicates import or export processing.
    /// 表示导入或导出阶段。
    ImportExport = 6,
    /// Indicates the C ABI boundary.
    /// 表示 C ABI 边界。
    Abi = 7,
}

/// Carries stable error semantics and developer diagnostics.
/// 携带稳定错误语义与开发者诊断信息。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code:?} at {stage:?}: {diagnostic}")]
pub struct CoreError {
    /// Provides the stable machine-readable code.
    /// 提供稳定的机器可读错误码。
    pub code: ErrorCode,
    /// Provides the stage where the failure occurred.
    /// 提供失败发生的阶段。
    pub stage: ErrorStage,
    /// States whether retrying is safe.
    /// 表示重试是否安全。
    pub retryable: bool,
    /// States whether persistent side effects may already exist.
    /// 表示是否可能已经产生持久化副作用。
    pub side_effect_possible: bool,
    /// Identifies the frozen configuration snapshot when available.
    /// 在可用时标识冻结的配置快照。
    pub config_snapshot_id: Option<ConfigSnapshotId>,
    /// Identifies the command when available.
    /// 在可用时标识命令。
    pub command_id: Option<CommandId>,
    /// Identifies the idempotent operation when available.
    /// 在可用时标识幂等操作。
    pub idempotency_key: Option<IdempotencyKey>,
    /// Provides a developer-facing diagnostic without secrets.
    /// 提供不包含秘密的开发者诊断文本。
    pub diagnostic: String,
}

impl CoreError {
    /// Creates an error with conservative side-effect defaults.
    /// 使用保守的副作用默认值创建错误。
    #[must_use]
    pub fn new(code: ErrorCode, stage: ErrorStage, diagnostic: impl Into<String>) -> Self {
        Self {
            code,
            stage,
            retryable: false,
            side_effect_possible: false,
            config_snapshot_id: None,
            command_id: None,
            idempotency_key: None,
            diagnostic: diagnostic.into(),
        }
    }

    /// Marks whether the same operation may be retried safely.
    /// 标记同一操作是否可以安全重试。
    #[must_use]
    pub const fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// Attaches the immutable configuration snapshot identity.
    /// 附加不可变配置快照标识。
    #[must_use]
    pub fn with_snapshot(mut self, snapshot_id: ConfigSnapshotId) -> Self {
        self.config_snapshot_id = Some(snapshot_id);
        self
    }

    /// Attaches the command identity.
    /// 附加命令标识。
    #[must_use]
    pub fn with_command(mut self, command_id: CommandId) -> Self {
        self.command_id = Some(command_id);
        self
    }

    /// Marks that a durable side effect may already have occurred.
    /// 标记可能已经发生持久化副作用。
    #[must_use]
    pub fn with_uncertain_side_effect(mut self, idempotency_key: IdempotencyKey) -> Self {
        self.side_effect_possible = true;
        self.idempotency_key = Some(idempotency_key);
        self
    }
}
