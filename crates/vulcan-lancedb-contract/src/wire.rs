//! Framed request/response protocol between the LanceDB client and the native library.
//! LanceDB 客户端与原生库之间的分帧请求/响应协议。
//!
//! The native library exports exactly three C functions: one reporting [`PROTOCOL_VERSION`], one
//! taking a request frame and returning a response frame, and one releasing a response frame. A
//! frame is a little-endian `u64` header length, the JSON header, then the raw payload bytes that
//! carry encoded rows. Rust types never cross the boundary, so the library and its host may be built
//! by different compilers; only this protocol must match, and a mismatch is refused at load time.
//!
//! 原生库只导出三个 C 函数：一个报告 [`PROTOCOL_VERSION`]，一个接收请求帧并返回响应帧，一个释放响应帧。
//! 帧由小端 `u64` 头部长度、JSON 头部以及承载已编码行的原始载荷字节组成。Rust 类型从不跨越边界，因此
//! 库与宿主可以由不同编译器构建；只有本协议必须一致，不一致时在加载阶段拒绝。

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::config::{DatabaseRuntimeConfig, LanceDbEngineOptions};
use crate::error::LanceDbEngineError;
use crate::types::{
    LanceDbCompactTableInput, LanceDbCompactTableResult, LanceDbCreateTableInput,
    LanceDbCreateTableResult, LanceDbDeleteInput, LanceDbDeleteResult, LanceDbDistanceMetric,
    LanceDbDropTableInput, LanceDbDropTableResult, LanceDbSearchInput, LanceDbSearchResult,
    LanceDbTableStatisticsInput, LanceDbTableStatisticsResult, LanceDbUpsertInput,
    LanceDbUpsertResult,
};

/// Protocol version both sides must report; increment it with every change to this module's types.
/// 双方必须报告的协议版本；本模块类型的每次变化都必须递增它。
pub const PROTOCOL_VERSION: u32 = 1;

/// Library file stem; the platform prefix and suffix come from [`library_file_name`].
/// 库文件主名；平台前缀与后缀由 [`library_file_name`] 给出。
pub const LIBRARY_STEM: &str = "vulcan_lancedb_native";

/// Exported symbol returning the library's [`PROTOCOL_VERSION`].
/// 返回库 [`PROTOCOL_VERSION`] 的导出符号。
pub const PROTOCOL_VERSION_SYMBOL: &str = "vulcan_lancedb_protocol_version";

/// Exported symbol executing one request frame.
/// 执行一个请求帧的导出符号。
pub const CALL_SYMBOL: &str = "vulcan_lancedb_call";

/// Exported symbol releasing one response frame returned by [`CALL_SYMBOL`].
/// 释放 [`CALL_SYMBOL`] 返回的一个响应帧的导出符号。
pub const FREE_SYMBOL: &str = "vulcan_lancedb_free";

/// Byte width of the frame's header-length prefix.
/// 帧头部长度前缀的字节宽度。
const HEADER_LENGTH_BYTES: usize = std::mem::size_of::<u64>();

/// Returns the platform file name of the native library, such as `vulcan_lancedb_native.dll`.
/// 返回原生库的平台文件名，例如 `vulcan_lancedb_native.dll`。
pub fn library_file_name() -> String {
    format!(
        "{}{LIBRARY_STEM}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )
}

/// One operation sent to the native library.
/// 发送给原生库的一个操作。
///
/// Runtimes and engines are addressed by opaque handles the library issued; a handle stays valid
/// until its matching release request.
/// 运行时与引擎通过库签发的不透明句柄寻址；句柄在对应的释放请求之前保持有效。
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Request {
    /// Creates one runtime that caches one engine and one root lock per physical database.
    /// 创建一个运行时，按物理数据库缓存一个引擎与一把根锁。
    CreateRuntime {
        /// Database configuration.
        /// 数据库配置。
        config: DatabaseRuntimeConfig,
        /// Options applied to every engine the runtime opens.
        /// 施加于运行时打开的每个引擎的选项。
        options: LanceDbEngineOptions,
    },
    /// Releases a runtime together with every root lock it holds.
    /// 释放一个运行时及其持有的全部根锁。
    ReleaseRuntime {
        /// Runtime handle.
        /// 运行时句柄。
        runtime: u64,
    },
    /// Opens, or reuses, the engine of one database; `None` names the default database.
    /// 打开或复用一个数据库的引擎；`None` 表示默认数据库。
    OpenEngine {
        /// Runtime handle.
        /// 运行时句柄。
        runtime: u64,
        /// Database name.
        /// 数据库名。
        database_name: Option<String>,
    },
    /// Releases one engine handle.
    /// 释放一个引擎句柄。
    ReleaseEngine {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
    },
    /// Creates a table.
    /// 建表。
    CreateTable {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
        /// Operation input.
        /// 操作输入。
        input: LanceDbCreateTableInput,
    },
    /// Writes rows; the encoded rows are the frame payload.
    /// 写入行；已编码行为帧载荷。
    VectorUpsert {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
        /// Operation input without its rows.
        /// 不含行数据的操作输入。
        input: LanceDbUpsertInput,
    },
    /// Maintains the scalar index of one merge key column.
    /// 维护一个合并键列的标量索引。
    MaintainMergeKeyIndex {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
        /// Table name.
        /// 表名。
        table_name: String,
        /// Key column.
        /// 键列。
        column: String,
    },
    /// Searches nearest neighbours under an explicit metric.
    /// 按显式距离执行最近邻检索。
    VectorSearch {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
        /// Operation input.
        /// 操作输入。
        input: LanceDbSearchInput,
        /// Distance metric.
        /// 距离度量。
        metric: LanceDbDistanceMetric,
    },
    /// Deletes rows by predicate.
    /// 按谓词删除行。
    Delete {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
        /// Operation input.
        /// 操作输入。
        input: LanceDbDeleteInput,
    },
    /// Drops a table.
    /// 删除表。
    DropTable {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
        /// Operation input.
        /// 操作输入。
        input: LanceDbDropTableInput,
    },
    /// Reads physical table statistics.
    /// 读取表物理统计。
    TableStatistics {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
        /// Operation input.
        /// 操作输入。
        input: LanceDbTableStatisticsInput,
    },
    /// Compacts small physical files.
    /// 合并小型物理文件。
    CompactTable {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
        /// Operation input.
        /// 操作输入。
        input: LanceDbCompactTableInput,
    },
}

/// One outcome returned by the native library.
/// 原生库返回的一个结果。
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Response {
    /// A runtime was created.
    /// 已创建运行时。
    Runtime {
        /// Runtime handle.
        /// 运行时句柄。
        runtime: u64,
    },
    /// An engine handle was issued.
    /// 已签发引擎句柄。
    Engine {
        /// Engine handle.
        /// 引擎句柄。
        engine: u64,
        /// Identity shared by every handle of the same database engine.
        /// 同一数据库引擎的所有句柄共用的身份。
        database: u64,
    },
    /// A handle was released.
    /// 已释放句柄。
    Released,
    /// A table was created.
    /// 已建表。
    TableCreated {
        /// Operation result.
        /// 操作结果。
        result: LanceDbCreateTableResult,
    },
    /// Rows were written.
    /// 已写入行。
    Upserted {
        /// Operation result.
        /// 操作结果。
        result: LanceDbUpsertResult,
    },
    /// A merge key index was maintained.
    /// 已维护合并键索引。
    IndexMaintained,
    /// A search completed; the encoded rows are the frame payload.
    /// 检索完成；已编码行为帧载荷。
    Searched {
        /// Operation result without its rows.
        /// 不含行数据的操作结果。
        result: LanceDbSearchResult,
    },
    /// Rows were deleted.
    /// 已删除行。
    Deleted {
        /// Operation result.
        /// 操作结果。
        result: LanceDbDeleteResult,
    },
    /// A table was dropped.
    /// 已删除表。
    TableDropped {
        /// Operation result.
        /// 操作结果。
        result: LanceDbDropTableResult,
    },
    /// Table statistics were read.
    /// 已读取表统计。
    Statistics {
        /// Operation result.
        /// 操作结果。
        result: LanceDbTableStatisticsResult,
    },
    /// A table was compacted.
    /// 已整理表。
    Compacted {
        /// Operation result.
        /// 操作结果。
        result: LanceDbCompactTableResult,
    },
    /// The operation failed.
    /// 操作失败。
    Failed {
        /// Structured failure.
        /// 结构化失败。
        error: WireError,
    },
}

/// Serializable mirror of [`LanceDbEngineError`]; I/O sources travel as their message.
/// [`LanceDbEngineError`] 的可序列化镜像；I/O 源错误以其消息传输。
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireError {
    /// See [`LanceDbEngineError::InvalidArgument`].
    /// 见 [`LanceDbEngineError::InvalidArgument`]。
    InvalidArgument {
        /// Failure description.
        /// 失败描述。
        detail: String,
    },
    /// See [`LanceDbEngineError::Open`].
    /// 见 [`LanceDbEngineError::Open`]。
    Open {
        /// Database path.
        /// 数据库路径。
        path: PathBuf,
        /// Failure description.
        /// 失败描述。
        detail: String,
    },
    /// See [`LanceDbEngineError::PathNotAuthorized`].
    /// 见 [`LanceDbEngineError::PathNotAuthorized`]。
    PathNotAuthorized {
        /// Failure description.
        /// 失败描述。
        detail: String,
    },
    /// See [`LanceDbEngineError::LockHeld`].
    /// 见 [`LanceDbEngineError::LockHeld`]。
    LockHeld {
        /// Lock file path.
        /// 锁文件路径。
        lock_path: PathBuf,
        /// Recorded owner.
        /// 记录的持有者。
        owner: String,
    },
    /// See [`LanceDbEngineError::LockIo`].
    /// 见 [`LanceDbEngineError::LockIo`]。
    LockIo {
        /// Lock file path.
        /// 锁文件路径。
        lock_path: PathBuf,
        /// I/O failure message.
        /// I/O 失败消息。
        detail: String,
    },
    /// See [`LanceDbEngineError::TableNotFound`].
    /// 见 [`LanceDbEngineError::TableNotFound`]。
    TableNotFound {
        /// Table name.
        /// 表名。
        table_name: String,
    },
    /// See [`LanceDbEngineError::SchemaNotAllowed`].
    /// 见 [`LanceDbEngineError::SchemaNotAllowed`]。
    SchemaNotAllowed {
        /// Failure description.
        /// 失败描述。
        detail: String,
    },
    /// See [`LanceDbEngineError::Compaction`].
    /// 见 [`LanceDbEngineError::Compaction`]。
    Compaction {
        /// Failure description.
        /// 失败描述。
        detail: String,
    },
    /// See [`LanceDbEngineError::Internal`].
    /// 见 [`LanceDbEngineError::Internal`]。
    Internal {
        /// Failure description.
        /// 失败描述。
        detail: String,
    },
}

impl From<LanceDbEngineError> for WireError {
    /// Converts a library failure for transport.
    /// 为传输转换库失败。
    fn from(error: LanceDbEngineError) -> Self {
        match error {
            LanceDbEngineError::InvalidArgument { detail } => Self::InvalidArgument { detail },
            LanceDbEngineError::Open { path, detail } => Self::Open { path, detail },
            LanceDbEngineError::PathNotAuthorized { detail } => Self::PathNotAuthorized { detail },
            LanceDbEngineError::LockHeld { lock_path, owner } => {
                Self::LockHeld { lock_path, owner }
            }
            LanceDbEngineError::LockIo { lock_path, source } => Self::LockIo {
                lock_path,
                detail: source.to_string(),
            },
            LanceDbEngineError::TableNotFound { table_name } => Self::TableNotFound { table_name },
            LanceDbEngineError::SchemaNotAllowed { detail } => Self::SchemaNotAllowed { detail },
            LanceDbEngineError::Compaction { detail } => Self::Compaction { detail },
            LanceDbEngineError::Internal { detail } => Self::Internal { detail },
            // The library itself never reports this; the client raises it before any call.
            // 库自身从不报告此错误；客户端在任何调用之前抛出它。
            LanceDbEngineError::LibraryUnavailable { path, detail } => Self::Internal {
                detail: format!(
                    "unexpected library failure for {}: {detail}",
                    path.display()
                ),
            },
        }
    }
}

impl From<WireError> for LanceDbEngineError {
    /// Restores a transported failure.
    /// 还原传输的失败。
    fn from(error: WireError) -> Self {
        match error {
            WireError::InvalidArgument { detail } => Self::InvalidArgument { detail },
            WireError::Open { path, detail } => Self::Open { path, detail },
            WireError::PathNotAuthorized { detail } => Self::PathNotAuthorized { detail },
            WireError::LockHeld { lock_path, owner } => Self::LockHeld { lock_path, owner },
            WireError::LockIo { lock_path, detail } => Self::LockIo {
                lock_path,
                source: std::io::Error::other(detail),
            },
            WireError::TableNotFound { table_name } => Self::TableNotFound { table_name },
            WireError::SchemaNotAllowed { detail } => Self::SchemaNotAllowed { detail },
            WireError::Compaction { detail } => Self::Compaction { detail },
            WireError::Internal { detail } => Self::Internal { detail },
        }
    }
}

/// Encodes one frame from a JSON header and raw payload bytes.
/// 由 JSON 头部与原始载荷字节编码一个帧。
pub fn encode_frame<T: Serialize>(header: &T, payload: &[u8]) -> Result<Vec<u8>, String> {
    let header = serde_json::to_vec(header)
        .map_err(|error| format!("failed to encode LanceDB frame header: {error}"))?;
    let mut frame = Vec::with_capacity(HEADER_LENGTH_BYTES + header.len() + payload.len());
    frame.extend_from_slice(&(header.len() as u64).to_le_bytes());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Decodes one frame into its JSON header and the borrowed payload bytes.
/// 将一个帧解码为 JSON 头部与借用的载荷字节。
pub fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<(T, &[u8]), String> {
    let (length, rest) = frame
        .split_first_chunk::<HEADER_LENGTH_BYTES>()
        .ok_or_else(|| "LanceDB frame is shorter than its header length".to_string())?;
    let length = usize::try_from(u64::from_le_bytes(*length))
        .ok()
        .filter(|length| *length <= rest.len())
        .ok_or_else(|| "LanceDB frame header length exceeds the frame".to_string())?;
    let (header, payload) = rest.split_at(length);
    let header = serde_json::from_slice(header)
        .map_err(|error| format!("failed to decode LanceDB frame header: {error}"))?;
    Ok((header, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LanceDbInputFormat;

    /// Verifies rows travel as the payload, not inside the JSON header.
    /// 验证行数据作为载荷传输，而不在 JSON 头部中。
    #[test]
    fn upsert_rows_travel_as_payload_outside_the_header() {
        let rows = vec![7_u8; 4096];
        let request = Request::VectorUpsert {
            engine: 3,
            input: LanceDbUpsertInput {
                table_name: "chunks".to_string(),
                input_format: LanceDbInputFormat::ArrowIpc,
                data: rows.clone(),
                key_columns: vec!["id".to_string()],
            },
        };
        let frame = encode_frame(&request, &rows).expect("encode");
        assert!(
            frame.len() < rows.len() * 2,
            "rows must not be expanded into JSON"
        );

        let (decoded, payload) = decode_frame::<Request>(&frame).expect("decode");
        assert_eq!(payload, rows.as_slice());
        match decoded {
            Request::VectorUpsert { engine, input } => {
                assert_eq!(engine, 3);
                assert_eq!(input.table_name, "chunks");
                assert!(input.data.is_empty(), "the header never carries rows");
            }
            other => panic!("unexpected request {other:?}"),
        }
    }

    /// Verifies truncated frames are refused instead of being read past their end.
    /// 验证截断帧被拒绝，而不是越界读取。
    #[test]
    fn truncated_frames_are_refused() {
        let frame = encode_frame(&Response::Released, &[]).expect("encode");
        assert!(decode_frame::<Response>(&frame[..4]).is_err());
        assert!(decode_frame::<Response>(&frame[..frame.len() - 1]).is_err());
    }

    /// Verifies a transported failure keeps its variant so callers can still match on it.
    /// 验证传输后的失败保留其变体，使调用方仍可按变体匹配。
    #[test]
    fn transported_failures_keep_their_variant() {
        let original = LanceDbEngineError::LockHeld {
            lock_path: PathBuf::from("db.vldb.lock"),
            owner: "pid=1".to_string(),
        };
        let frame = encode_frame(
            &Response::Failed {
                error: original.into(),
            },
            &[],
        )
        .expect("encode");
        let (response, _) = decode_frame::<Response>(&frame).expect("decode");
        let Response::Failed { error } = response else {
            panic!("expected a failure");
        };
        assert!(matches!(
            LanceDbEngineError::from(error),
            LanceDbEngineError::LockHeld { owner, .. } if owner == "pid=1"
        ));
    }
}
