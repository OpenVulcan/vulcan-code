//! Strict current-schema runtime-log line decoding.
//! 严格的当前结构运行时日志行解码。

use super::{RUNTIME_LOG_SCHEMA_VERSION, RuntimeLogEventLine};
use serde_json::Value;

/// Strict failure returned while decoding one persisted runtime-log line.
/// 解码单条持久化运行时日志行时返回的严格错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeLogEventLineDecodeError {
    /// Source text is not syntactically valid JSON and may belong to a torn suffix.
    /// 源文本不是有效 JSON 语法，可能属于写入中断形成的尾部。
    InvalidJson {
        /// JSON parser diagnostic retained for the storage error boundary.
        /// 为存储错误边界保留的 JSON 解析器诊断。
        detail: String,
    },
    /// JSON does not expose a valid unsigned 32-bit schema version.
    /// JSON 未提供有效的无符号 32 位结构版本。
    InvalidSchemaEnvelope {
        /// Exact envelope contract violation.
        /// 精确的信封契约违规说明。
        detail: String,
    },
    /// Persisted data does not use the one supported current schema.
    /// 持久化数据未使用唯一受支持的当前结构。
    UnsupportedSchemaVersion {
        /// Unsupported version declared by the persisted source.
        /// 持久化源声明的不受支持版本。
        schema_version: u32,
    },
    /// A current-version envelope violates the current payload contract.
    /// 当前版本信封违反了当前载荷契约。
    InvalidCurrentPayload {
        /// Exact current-schema payload violation.
        /// 精确的当前结构载荷违规说明。
        detail: String,
    },
}

impl RuntimeLogEventLineDecodeError {
    /// Reports whether this failure is eligible for corrupt-JSON suffix repair.
    /// 返回该错误是否可进入损坏 JSON 尾部修复流程。
    pub fn is_invalid_json(&self) -> bool {
        matches!(self, Self::InvalidJson { .. })
    }
}

impl std::fmt::Display for RuntimeLogEventLineDecodeError {
    /// Formats one stable English diagnostic for storage and UI propagation.
    /// 为存储层与界面传播格式化稳定英文诊断。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson { detail } => {
                write!(formatter, "Runtime log line is not valid JSON: {detail}")
            }
            Self::InvalidSchemaEnvelope { detail } => {
                write!(formatter, "Invalid runtime log schema envelope: {detail}")
            }
            Self::UnsupportedSchemaVersion { schema_version } => write!(
                formatter,
                "Unsupported runtime log schema version {schema_version}; current version is {RUNTIME_LOG_SCHEMA_VERSION}."
            ),
            Self::InvalidCurrentPayload { detail } => {
                write!(formatter, "Invalid runtime log v1 payload: {detail}")
            }
        }
    }
}

impl std::error::Error for RuntimeLogEventLineDecodeError {}

/// Decodes one persisted JSONL line using only the current runtime schema.
/// 仅使用当前运行时结构解码一条持久化 JSONL 日志行。
///
/// # Parameters
/// 参数
/// `source` is one complete non-empty JSONL row.
/// `source` 是一条完整且非空的 JSONL 记录。
///
/// # Returns
/// 返回值
/// A current-schema line, or a strict syntax/version/payload error without migration.
/// 返回当前结构日志行；失败时返回严格的语法、版本或载荷错误且不执行迁移。
pub fn decode_runtime_log_event_line_json(
    source: &str,
) -> Result<RuntimeLogEventLine, RuntimeLogEventLineDecodeError> {
    // value preserves the envelope until the schema selector has been validated.
    // value 在结构选择器完成验证前保留信封。
    let value = serde_json::from_str::<Value>(source).map_err(|error| {
        RuntimeLogEventLineDecodeError::InvalidJson {
            detail: error.to_string(),
        }
    })?;
    // schema_version is the sole accepted format selector; field probing is forbidden.
    // schema_version 是唯一接受的格式选择器，禁止探测字段来猜测格式。
    let schema_version = runtime_log_schema_version(&value)?;
    if schema_version != RUNTIME_LOG_SCHEMA_VERSION {
        return Err(RuntimeLogEventLineDecodeError::UnsupportedSchemaVersion { schema_version });
    }
    serde_json::from_value::<RuntimeLogEventLine>(value).map_err(|error| {
        RuntimeLogEventLineDecodeError::InvalidCurrentPayload {
            detail: error.to_string(),
        }
    })
}

/// Reads and validates the schema selector from one runtime-log JSON envelope.
/// 从运行时日志 JSON 信封中读取并验证结构选择器。
fn runtime_log_schema_version(value: &Value) -> Result<u32, RuntimeLogEventLineDecodeError> {
    // raw_schema_version must be an integer because schema selection cannot be inferred.
    // raw_schema_version 必须是整数，因为结构选择不能依靠推断。
    let raw_schema_version = value
        .as_object()
        .and_then(|object| object.get("schema_version"))
        .and_then(Value::as_u64)
        .ok_or_else(|| RuntimeLogEventLineDecodeError::InvalidSchemaEnvelope {
            detail: String::from("`schema_version` must be an unsigned integer."),
        })?;
    u32::try_from(raw_schema_version).map_err(|_| {
        RuntimeLogEventLineDecodeError::InvalidSchemaEnvelope {
            detail: format!("`schema_version` {raw_schema_version} exceeds u32 capacity."),
        }
    })
}
