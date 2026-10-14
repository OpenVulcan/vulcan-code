//! Canonical binary command codec and versioned command checkpoints.
//! 规范二进制命令编解码器与带版本命令检查点。

use crate::{CoreError, ErrorCode, ErrorStage, IdempotencyKey, ScoreContractRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Identifies one stable checkpoint format release.
/// 标识一个稳定的检查点格式版本。
pub const CHECKPOINT_FORMAT_VERSION: u32 = 36;

/// Names the checkpoint magic prefix.
/// 命名检查点魔数前缀。
pub const CHECKPOINT_MAGIC: [u8; 4] = *b"VMCK";

/// Describes one checkpointed command lifecycle position.
/// 描述一个已检查点化的命令生命周期位置。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointState {
    /// Indicates the command has not emitted its first external effect.
    /// 表示命令尚未发出首个外部 Effect。
    Ready,
    /// Indicates the command is waiting for the named effect identity.
    /// 表示命令正在等待命名 Effect 身份。
    WaitingForEffect {
        /// Identifies the pending effect.
        /// 标识待处理 Effect。
        effect_id: crate::EffectId,
        /// Classifies the pending effect flow.
        /// 对待处理 Effect 流程分类。
        effect_kind: String,
    },
    /// Indicates a mutation is pending reconciliation by its durable key.
    /// 表示变更正通过其持久键等待对账。
    AwaitingReconciliation {
        /// Identifies the durable mutation key.
        /// 标识持久变更键。
        idempotency_key: IdempotencyKey,
    },
    /// Indicates the command reached its final terminal state.
    /// 表示命令已到达最终终态。
    Terminal,
}

/// Carries the bound contract identity set of one checkpoint.
/// 携带一个检查点绑定的契约身份集合。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointContractSet {
    /// Carries structured-output contract digests.
    /// 携带结构化输出契约摘要。
    pub structured_output_contracts: Vec<String>,
    /// Carries accepted score contract identities.
    /// 携带接受的评分契约身份。
    pub score_contracts: Vec<ScoreContractRef>,
}

/// Carries the complete versioned command checkpoint payload.
/// 携带完整带版本的命令检查点载荷。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandCheckpoint {
    /// Reports the checkpoint format version.
    /// 报告检查点格式版本。
    pub format_version: u32,
    /// Identifies the owning command handle.
    /// 标识所属命令句柄。
    pub handle: crate::CommandHandle,
    /// Carries the canonical encoded command input.
    /// 携带规范编码的命令输入。
    pub command_input: Vec<u8>,
    /// Carries the persistent mutation identity when the command mutates.
    /// 在命令变更时携带持久变更身份。
    pub mutation_identity: Option<crate::MutationIdentity>,
    /// Carries the current lifecycle position.
    /// 携带当前生命周期位置。
    pub state: CheckpointState,
    /// Carries the bound contract identity set.
    /// 携带绑定的契约身份集合。
    pub contracts: CheckpointContractSet,
    /// Carries a summary of already received results.
    /// 携带已接收结果摘要。
    pub received_summary: Vec<String>,
    /// Carries the full mid-flow continuation needed to resume a waiting command in a new
    /// process: the serialized private flow state, the exact pending effect request, the
    /// effect sequence cursor, the pending mutation key, and the degraded flag.
    /// 携带新进程恢复等待命令所需的完整中间流程续体：序列化私有流程状态、
    /// 精确待处理 Effect 请求、Effect 序号游标、待处理变更键与降级标志。
    pub continuation: Option<CheckpointContinuation>,
    /// Carries the canonical payload digest for integrity checks.
    /// 携带用于完整性校验的规范载荷摘要。
    pub payload_digest: String,
}

/// Carries the full mid-flow continuation payload of one waiting command.
/// 携带一个等待命令的完整中间流程续体载荷。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointContinuation {
    /// Carries the serde-serialized private flow state.
    /// 携带 serde 序列化的私有流程状态。
    pub flow_state: Vec<u8>,
    /// Carries the serde-serialized exact pending effect request.
    /// 携带 serde 序列化的精确待处理 Effect 请求。
    pub pending_effect: Option<Vec<u8>>,
    /// Carries the next monotonic effect sequence cursor.
    /// 携带下一个单调 Effect 序号游标。
    pub next_effect_sequence: u32,
    /// Carries the durable mutation key awaiting reconciliation.
    /// 携带等待对账的持久变更键。
    pub pending_mutation_idempotency_key: Option<crate::IdempotencyKey>,
    /// Records whether any optional host stage degraded.
    /// 记录是否有可选宿主阶段降级。
    pub degraded: bool,
}

/// Serializes one checkpoint into its deterministic binary container.
/// 将一个检查点序列化为其确定性二进制容器。
///
/// # Errors
/// Returns a stable format error when the checkpoint payload is invalid.
/// 当检查点载荷无效时返回稳定格式错误。
pub fn encode_checkpoint(checkpoint: &CommandCheckpoint) -> Result<Vec<u8>, CoreError> {
    let command_len = u64::try_from(checkpoint.command_input.len())
        .map_err(|_| checkpoint_error("checkpoint command input exceeds u64"))?;
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&CHECKPOINT_MAGIC);
    buffer.extend_from_slice(&checkpoint.format_version.to_le_bytes());
    write_checkpoint_state(&mut buffer, &checkpoint.state);
    write_len_string(&mut buffer, checkpoint.handle.command_id.as_str());
    write_len_string(&mut buffer, checkpoint.handle.config_snapshot_id.as_str());
    write_len_string(&mut buffer, &checkpoint.payload_digest);
    write_string_list(
        &mut buffer,
        &checkpoint.contracts.structured_output_contracts,
    );
    buffer.extend_from_slice(
        &u64::try_from(checkpoint.contracts.score_contracts.len())
            .map_err(|_| checkpoint_error("checkpoint score contract count exceeds u64"))?
            .to_le_bytes(),
    );
    for contract in &checkpoint.contracts.score_contracts {
        write_len_string(&mut buffer, &contract.id);
        buffer.extend_from_slice(&contract.version.to_le_bytes());
    }
    write_string_list(&mut buffer, &checkpoint.received_summary);
    let identity = checkpoint.mutation_identity.as_ref();
    buffer.push(u8::from(identity.is_some()));
    if let Some(identity) = identity {
        write_len_string(&mut buffer, identity.operation_id.as_str());
        write_len_string(&mut buffer, identity.idempotency_key.as_str());
        write_len_string(&mut buffer, &identity.request_fingerprint);
        buffer.extend_from_slice(&identity.source_kind.as_u32().to_le_bytes());
        write_len_string(&mut buffer, &identity.request_source_id);
        buffer.extend_from_slice(&identity.observed_at_ns.to_le_bytes());
    }
    buffer.extend_from_slice(&command_len.to_le_bytes());
    buffer.extend_from_slice(&checkpoint.command_input);
    // Append the optional mid-flow continuation after the command input.
    // 在命令输入之后追加可选的中间流程续体。
    let continuation = checkpoint.continuation.as_ref();
    buffer.push(u8::from(continuation.is_some()));
    if let Some(continuation) = continuation {
        write_len_bytes(&mut buffer, &continuation.flow_state);
        match &continuation.pending_effect {
            Some(pending_effect) => {
                buffer.push(1);
                write_len_bytes(&mut buffer, pending_effect);
            }
            None => buffer.push(0),
        }
        buffer.extend_from_slice(&continuation.next_effect_sequence.to_le_bytes());
        let pending_key = continuation.pending_mutation_idempotency_key.as_ref();
        buffer.push(u8::from(pending_key.is_some()));
        if let Some(pending_key) = pending_key {
            write_len_string(&mut buffer, pending_key.as_str());
        }
        buffer.push(u8::from(continuation.degraded));
    }
    Ok(buffer)
}

/// Computes the integrity digest over every canonical checkpoint field except the digest itself.
/// 计算除摘要自身外每个规范检查点字段的完整性摘要。
///
/// # Errors
/// Returns a stable format error when the canonical checkpoint cannot be encoded.
/// 当规范检查点无法编码时返回稳定格式错误。
pub fn checkpoint_payload_digest(checkpoint: &CommandCheckpoint) -> Result<String, CoreError> {
    let mut unsigned = checkpoint.clone();
    unsigned.payload_digest.clear();
    let encoded = encode_checkpoint(&unsigned)?;
    let digest = Sha256::digest(encoded);
    // Preserve lowercase hexadecimal bytes independently of the digest output container type.
    // 保持小写十六进制字节编码，不依赖摘要输出容器实现格式化特性。
    Ok(hex::encode(digest))
}

/// Deserializes one checkpoint from its deterministic binary container.
/// 从确定性二进制容器反序列化一个检查点。
///
/// # Errors
/// Returns a stable format error when the container is truncated or carries an unknown version.
/// 当容器被截断或携带未知版本时返回稳定格式错误。
#[allow(clippy::too_many_lines)]
pub fn decode_checkpoint(bytes: &[u8]) -> Result<CommandCheckpoint, CoreError> {
    if bytes.len() < 4 || bytes[..4] != CHECKPOINT_MAGIC {
        return Err(checkpoint_error("checkpoint magic is invalid"));
    }
    let mut cursor = 4usize;
    let read_u32 = |cursor: &mut usize| -> Result<u32, CoreError> {
        let end = cursor
            .checked_add(4)
            .ok_or_else(|| checkpoint_error("truncated"))?;
        if end > bytes.len() {
            return Err(checkpoint_error("checkpoint truncated before u32"));
        }
        let value = u32::from_le_bytes(
            bytes[*cursor..end]
                .try_into()
                .map_err(|_| checkpoint_error("width"))?,
        );
        *cursor = end;
        Ok(value)
    };
    let read_u64 = |cursor: &mut usize| -> Result<u64, CoreError> {
        let end = cursor
            .checked_add(8)
            .ok_or_else(|| checkpoint_error("truncated"))?;
        if end > bytes.len() {
            return Err(checkpoint_error("checkpoint truncated before u64"));
        }
        let value = u64::from_le_bytes(
            bytes[*cursor..end]
                .try_into()
                .map_err(|_| checkpoint_error("width"))?,
        );
        *cursor = end;
        Ok(value)
    };
    let read_string = |cursor: &mut usize| -> Result<String, CoreError> {
        let length = usize::try_from(read_u64(cursor)?)
            .map_err(|_| checkpoint_error("string length exceeds platform size"))?;
        let end = cursor
            .checked_add(length)
            .ok_or_else(|| checkpoint_error("truncated"))?;
        if end > bytes.len() {
            return Err(checkpoint_error("checkpoint truncated before string"));
        }
        let value = std::str::from_utf8(&bytes[*cursor..end])
            .map_err(|_| checkpoint_error("checkpoint string is not UTF-8"))?
            .to_owned();
        *cursor = end;
        Ok(value)
    };
    let read_flag = |cursor: &mut usize, label: &str| -> Result<bool, CoreError> {
        let value = bytes
            .get(*cursor)
            .copied()
            .ok_or_else(|| checkpoint_error(format!("checkpoint truncated before {label}")))?;
        *cursor += 1;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(checkpoint_error(format!(
                "checkpoint {label} flag is not boolean: {value}"
            ))),
        }
    };
    let read_string_list = |cursor: &mut usize, label: &str| -> Result<Vec<String>, CoreError> {
        let count = usize::try_from(read_u64(cursor)?).map_err(|_| {
            checkpoint_error(format!("checkpoint {label} count exceeds platform size"))
        })?;
        let mut values = Vec::with_capacity(count.min(1_024));
        for _ in 0..count {
            values.push(read_string(cursor)?);
        }
        Ok(values)
    };
    let format_version = read_u32(&mut cursor)?;
    if format_version != CHECKPOINT_FORMAT_VERSION {
        return Err(CoreError::new(
            ErrorCode::CheckpointIncompatible,
            ErrorStage::Validation,
            format!("checkpoint format {format_version} is not supported"),
        ));
    }
    let state_tag = bytes
        .get(cursor)
        .copied()
        .ok_or_else(|| checkpoint_error("checkpoint truncated before state"))?;
    cursor += 1;
    let state = match state_tag {
        0 => CheckpointState::Ready,
        1 => CheckpointState::WaitingForEffect {
            effect_id: crate::EffectId::new(read_string(&mut cursor)?)
                .map_err(|error| checkpoint_error(error.to_string()))?,
            effect_kind: read_string(&mut cursor)?,
        },
        2 => CheckpointState::AwaitingReconciliation {
            idempotency_key: IdempotencyKey::new(read_string(&mut cursor)?)
                .map_err(|error| checkpoint_error(error.to_string()))?,
        },
        3 => CheckpointState::Terminal,
        other => {
            return Err(checkpoint_error(format!(
                "unknown checkpoint state tag: {other}"
            )));
        }
    };
    let command_id = crate::CommandId::new(read_string(&mut cursor)?)
        .map_err(|error| checkpoint_error(error.to_string()))?;
    let config_snapshot_id = crate::ConfigSnapshotId::new(read_string(&mut cursor)?)
        .map_err(|error| checkpoint_error(error.to_string()))?;
    let payload_digest = read_string(&mut cursor)?;
    let structured_output_contracts = read_string_list(&mut cursor, "structured contract")?;
    let score_contract_count = usize::try_from(read_u64(&mut cursor)?)
        .map_err(|_| checkpoint_error("checkpoint score contract count exceeds platform size"))?;
    let mut score_contracts = Vec::with_capacity(score_contract_count.min(1_024));
    for _ in 0..score_contract_count {
        let id = read_string(&mut cursor)?;
        let version = read_u32(&mut cursor)?;
        if id.is_empty() || version == 0 {
            return Err(checkpoint_error(
                "checkpoint score contract identity is invalid",
            ));
        }
        score_contracts.push(ScoreContractRef { id, version });
    }
    let received_summary = read_string_list(&mut cursor, "received summary")?;
    let mutation_identity = if read_flag(&mut cursor, "mutation identity presence")? {
        let operation_id = crate::OperationId::new(read_string(&mut cursor)?)
            .map_err(|error| checkpoint_error(error.to_string()))?;
        let idempotency_key = IdempotencyKey::new(read_string(&mut cursor)?)
            .map_err(|error| checkpoint_error(error.to_string()))?;
        let request_fingerprint = read_string(&mut cursor)?;
        let source_kind = crate::MutationSourceKind::try_from_u32(read_u32(&mut cursor)?)
            .ok_or_else(|| checkpoint_error("checkpoint mutation source is unknown"))?;
        let request_source_id = read_string(&mut cursor)?;
        let observed_at_ns = i64::from_le_bytes(
            bytes
                .get(
                    cursor
                        ..cursor
                            .checked_add(8)
                            .ok_or_else(|| checkpoint_error("truncated"))?,
                )
                .ok_or_else(|| checkpoint_error("checkpoint truncated before time"))?
                .try_into()
                .map_err(|_| checkpoint_error("width"))?,
        );
        cursor += 8;
        Some(crate::MutationIdentity {
            operation_id,
            idempotency_key,
            request_fingerprint,
            source_kind,
            request_source_id,
            observed_at_ns,
        })
    } else {
        None
    };
    let command_len = usize::try_from(read_u64(&mut cursor)?)
        .map_err(|_| checkpoint_error("command length exceeds platform size"))?;
    let end = cursor
        .checked_add(command_len)
        .ok_or_else(|| checkpoint_error("truncated"))?;
    if end > bytes.len() {
        return Err(checkpoint_error(
            "checkpoint truncated before command input",
        ));
    }
    let command_input = bytes[cursor..end].to_vec();
    cursor = end;
    // Parse the optional mid-flow continuation appended after the command input.
    // 解析命令输入之后追加的可选中间流程续体。
    let continuation = if read_flag(&mut cursor, "continuation presence")? {
        let read_len_bytes = |cursor: &mut usize| -> Result<Vec<u8>, CoreError> {
            let length = usize::try_from(read_u64(cursor)?)
                .map_err(|_| checkpoint_error("continuation length exceeds platform size"))?;
            let end = cursor
                .checked_add(length)
                .ok_or_else(|| checkpoint_error("truncated"))?;
            if end > bytes.len() {
                return Err(checkpoint_error(
                    "checkpoint truncated before continuation bytes",
                ));
            }
            let value = bytes[*cursor..end].to_vec();
            *cursor = end;
            Ok(value)
        };
        let flow_state = read_len_bytes(&mut cursor)?;
        let pending_effect = if read_flag(&mut cursor, "pending effect presence")? {
            Some(read_len_bytes(&mut cursor)?)
        } else {
            None
        };
        let next_effect_sequence = read_u32(&mut cursor)?;
        let pending_mutation_idempotency_key =
            if read_flag(&mut cursor, "pending mutation key presence")? {
                Some(
                    IdempotencyKey::new(read_string(&mut cursor)?)
                        .map_err(|error| checkpoint_error(error.to_string()))?,
                )
            } else {
                None
            };
        let degraded = read_flag(&mut cursor, "degraded")?;
        Some(CheckpointContinuation {
            flow_state,
            pending_effect,
            next_effect_sequence,
            pending_mutation_idempotency_key,
            degraded,
        })
    } else {
        None
    };
    if cursor != bytes.len() {
        return Err(checkpoint_error(
            "checkpoint container contains trailing bytes",
        ));
    }
    Ok(CommandCheckpoint {
        format_version,
        handle: crate::CommandHandle {
            command_id,
            config_snapshot_id,
        },
        command_input,
        mutation_identity,
        state,
        contracts: CheckpointContractSet {
            structured_output_contracts,
            score_contracts,
        },
        received_summary,
        continuation,
        payload_digest,
    })
}

/// Writes one checkpoint lifecycle position.
/// 写入一个检查点生命周期位置。
fn write_checkpoint_state(buffer: &mut Vec<u8>, state: &CheckpointState) {
    match state {
        CheckpointState::Ready => buffer.push(0),
        CheckpointState::WaitingForEffect {
            effect_id,
            effect_kind,
        } => {
            buffer.push(1);
            write_len_string(buffer, effect_id.as_str());
            write_len_string(buffer, effect_kind);
        }
        CheckpointState::AwaitingReconciliation { idempotency_key } => {
            buffer.push(2);
            write_len_string(buffer, idempotency_key.as_str());
        }
        CheckpointState::Terminal => buffer.push(3),
    }
}

/// Writes one length-prefixed UTF-8 string.
/// 写入一个带长度前缀的 UTF-8 字符串。
fn write_len_string(buffer: &mut Vec<u8>, value: &str) {
    buffer.extend_from_slice(&(value.len() as u64).to_le_bytes());
    buffer.extend_from_slice(value.as_bytes());
}

/// Writes one count-prefixed list of length-prefixed UTF-8 strings.
/// 写入一个带数量前缀且元素均带长度前缀的 UTF-8 字符串列表。
fn write_string_list(buffer: &mut Vec<u8>, values: &[String]) {
    buffer.extend_from_slice(&(values.len() as u64).to_le_bytes());
    for value in values {
        write_len_string(buffer, value);
    }
}

/// Writes one length-prefixed raw byte slice.
/// 写入一个带长度前缀的原始字节切片。
fn write_len_bytes(buffer: &mut Vec<u8>, value: &[u8]) {
    buffer.extend_from_slice(&(value.len() as u64).to_le_bytes());
    buffer.extend_from_slice(value);
}

/// Creates one checkpoint format error.
/// 创建一个检查点格式错误。
fn checkpoint_error(diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidBinaryFormat,
        ErrorStage::Validation,
        diagnostic,
    )
}
