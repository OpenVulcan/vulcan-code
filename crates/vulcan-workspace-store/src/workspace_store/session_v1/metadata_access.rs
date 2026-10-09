//! Exact-version metadata reads and the shared metadata publication lock.
//! 精确版本的元数据读取与共享元数据发布锁。
//!
//! Pre-release builds (V0.1.x) do not migrate older metadata schemas: any other version is
//! rejected explicitly and the file is left untouched (AGENTS.md section 1 rule 12).
//! 发布前版本（V0.1.x）不迁移旧元数据模式：其他版本一律明确拒绝，且不改动文件（AGENTS.md 第一节第 12 条）。

use std::fs::{self, File, OpenOptions};
use std::path::Path;

use serde::Deserialize;
use vulcan_session_core::v1::config::{
    METADATA_CONTRACT, METADATA_SCHEMA_VERSION, MetadataDocument,
};

use super::errors::{ErrorCode, SessionV1Error};

/// Minimal version discriminator, decoded before the current required fields.
/// 在当前必填字段之前解码的最小版本判别结构。
#[derive(Deserialize)]
struct MetadataHeader {
    /// Stable metadata family, independent from its evolvable schema.
    /// 稳定元数据家族标识，独立于可演进模式。
    contract: String,
    /// Exact persisted schema revision; never inferred from missing fields.
    /// 精确持久化模式修订；绝不根据缺失字段猜测版本。
    schema_version: u32,
}

/// Acquires the permanent metadata publication lock for an existing session directory.
/// 为已有会话目录取得永久元数据发布锁。
/// `path` is the exact metadata path; dropping the returned handle releases the OS lock.
/// `path` 是精确元数据路径；释放返回的句柄即释放操作系统锁。
pub(super) fn publication_lock(path: &Path) -> Result<File, SessionV1Error> {
    // A separate stable inode survives atomic replacement of metadata and process crashes.
    // 独立稳定文件节点不受元数据原子替换和进程崩溃影响。
    let lock_path = path.with_extension("write.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0x0000_0003).custom_flags(0x0020_0000);
    }
    let file = options.open(&lock_path).map_err(|error| {
        SessionV1Error::io(
            &lock_path,
            format!("open metadata publication lock: {error}"),
        )
    })?;
    let info = fs::symlink_metadata(&lock_path).map_err(|error| {
        SessionV1Error::io(
            &lock_path,
            format!("inspect metadata publication lock: {error}"),
        )
    })?;
    if !info.is_file() || info.file_type().is_symlink() {
        return Err(SessionV1Error::io(
            &lock_path,
            "metadata lock must be a regular file",
        ));
    }
    file.lock().map_err(|error| {
        SessionV1Error::io(
            &lock_path,
            format!("acquire metadata publication lock: {error}"),
        )
    })?;
    Ok(file)
}

/// Reads metadata of the exact current schema; every other version is rejected without writes.
/// 读取精确当前模式的元数据；其他任何版本都在不写入的情况下被拒绝。
/// `path` identifies one session; returns the typed document.
/// `path` 标识一个会话；返回强类型文档。
pub(super) fn read_metadata(path: &Path) -> Result<MetadataDocument, SessionV1Error> {
    let bytes = read_bytes(path)?;
    require_current_schema(path, &bytes)?;
    decode(path, &bytes)
}

/// Validates the metadata family and requires the exact current schema version.
/// 校验元数据家族并要求精确的当前模式版本。
/// `bytes` are the untouched file contents; older and future versions both return `unsupported`.
/// `bytes` 是未改动的文件内容；旧版本与未来版本都返回 `unsupported`。
pub(super) fn require_current_schema(path: &Path, bytes: &[u8]) -> Result<(), SessionV1Error> {
    let header: MetadataHeader = decode(path, bytes)?;
    if header.contract != METADATA_CONTRACT {
        return Err(SessionV1Error::unsupported(
            path,
            format!("unsupported metadata contract {:?}", header.contract),
        ));
    }
    if header.schema_version != METADATA_SCHEMA_VERSION {
        // Pre-release builds deliberately keep no upgrade path for older sessions.
        // 发布前版本刻意不为旧会话保留升级路径。
        return Err(SessionV1Error::unsupported(
            path,
            format!(
                "metadata schema {} is not supported by this pre-release build (requires exactly {}); older sessions are not migrated",
                header.schema_version, METADATA_SCHEMA_VERSION,
            ),
        ));
    }
    Ok(())
}

/// Reads exact metadata bytes, preserving absence and I/O diagnostics.
/// 读取精确元数据字节，保留缺失和输入输出诊断。
/// Never creates a missing session.
/// 绝不创建缺失会话。
pub(super) fn read_bytes(path: &Path) -> Result<Vec<u8>, SessionV1Error> {
    fs::read(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => corrupt(path, "metadata file does not exist"),
        _ => SessionV1Error::io(path, format!("read metadata: {error}")),
    })
}

/// Deserializes a metadata shape and preserves an explicit corruption error.
/// 反序列化元数据结构并保留明确的损坏错误。
/// `bytes` are untouched input; the caller chooses the required typed shape.
/// `bytes` 为未改写输入；调用方选择所需强类型结构。
fn decode<T: serde::de::DeserializeOwned>(path: &Path, bytes: &[u8]) -> Result<T, SessionV1Error> {
    serde_json::from_slice(bytes).map_err(|error| corrupt(path, format!("parse metadata: {error}")))
}

/// Builds one metadata-specific corruption error for the exact offending path.
/// 为精确出错路径构造元数据专用的损坏错误。
fn corrupt(path: &Path, detail: impl Into<String>) -> SessionV1Error {
    SessionV1Error::corrupt(ErrorCode::MetadataCorrupt, path, detail)
}
