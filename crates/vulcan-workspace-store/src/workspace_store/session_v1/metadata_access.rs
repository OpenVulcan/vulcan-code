//! Metadata reads with automatic upgrades, and the shared metadata publication lock.
//! 带自动升级的元数据读取，以及共享元数据发布锁。
//!
//! Versions from the upgrade baseline up to the current schema are upgraded on first read: the
//! original bytes are kept beside the file, the converted document is validated and published
//! atomically. Older and future versions are rejected untouched (AGENTS.md section 1 rule 12).
//! 自升级基线到当前模式之间的版本在首次读取时自动升级：原字节保留在文件旁，转换后的文档经校验后
//! 原子发布。更早与未来的版本原样拒绝（AGENTS.md 第一节第 12 条）。

use std::fs::{self, File, OpenOptions};
use std::path::Path;

use serde::Deserialize;
use vulcan_session_core::v1::config::{
    METADATA_CONTRACT, METADATA_SCHEMA_VERSION, METADATA_UPGRADE_BASELINE_VERSION, MetadataDocument,
};

use super::atomic_io::write_file_atomic;
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

/// Reads metadata, upgrading a supported older schema on first read.
/// 读取元数据；受支持的旧模式在首次读取时升级。
/// `path` identifies one session; returns the typed current document.
/// `path` 标识一个会话；返回当前强类型文档。
pub(super) fn read_metadata(path: &Path) -> Result<MetadataDocument, SessionV1Error> {
    let bytes = read_bytes(path)?;
    if schema_version(path, &bytes)? == METADATA_SCHEMA_VERSION {
        return decode(path, &bytes);
    }
    upgrade_metadata(path)
}

/// Validates the metadata family and requires the exact current schema version.
/// 校验元数据家族并要求精确的当前模式版本。
/// `bytes` are the untouched file contents. Callers that never upgrade (explicit rebuilds) use it;
/// upgradable versions are reported as unsupported here until an ordinary read upgrades them.
/// `bytes` 是未改动的文件内容。不做升级的调用方（显式重建）使用它；可升级版本在普通读取完成升级前
/// 在此报告为不支持。
pub(super) fn require_current_schema(path: &Path, bytes: &[u8]) -> Result<(), SessionV1Error> {
    let version = schema_version(path, bytes)?;
    if version != METADATA_SCHEMA_VERSION {
        return Err(SessionV1Error::unsupported(
            path,
            format!(
                "metadata schema {version} must be read once to upgrade it to {METADATA_SCHEMA_VERSION} before this operation"
            ),
        ));
    }
    Ok(())
}

/// Returns the schema version of a supported metadata family revision.
/// 返回受支持元数据家族修订的模式版本。
/// Versions below the upgrade baseline and future versions return `unsupported` without writes.
/// 低于升级基线的版本与未来版本在不写入的情况下返回 `unsupported`。
fn schema_version(path: &Path, bytes: &[u8]) -> Result<u32, SessionV1Error> {
    let header: MetadataHeader = decode(path, bytes)?;
    if header.contract != METADATA_CONTRACT {
        return Err(SessionV1Error::unsupported(
            path,
            format!("unsupported metadata contract {:?}", header.contract),
        ));
    }
    if !(METADATA_UPGRADE_BASELINE_VERSION..=METADATA_SCHEMA_VERSION)
        .contains(&header.schema_version)
    {
        return Err(SessionV1Error::unsupported(
            path,
            format!(
                "metadata schema {} is not supported (this build reads {METADATA_UPGRADE_BASELINE_VERSION} through {METADATA_SCHEMA_VERSION}); sessions older than the upgrade baseline are not migrated",
                header.schema_version,
            ),
        ));
    }
    Ok(header.schema_version)
}

/// Upgrades one older metadata file in place under the publication lock.
/// 在发布锁内原地升级一个旧元数据文件。
///
/// The original bytes are kept as `metadata.v{N}.backup.json` before the converted document is
/// published atomically. A concurrent reader that won the lock leaves a current file behind,
/// which is returned without a second upgrade.
/// 转换后的文档原子发布之前，原字节保存为 `metadata.v{N}.backup.json`。抢先取得锁的并发读取方
/// 会留下当前版本文件，此时直接返回而不再次升级。
fn upgrade_metadata(path: &Path) -> Result<MetadataDocument, SessionV1Error> {
    let _guard = publication_lock(path)?;
    let original = read_bytes(path)?;
    let from_version = schema_version(path, &original)?;
    if from_version == METADATA_SCHEMA_VERSION {
        return decode(path, &original);
    }
    let mut raw: serde_json::Value = decode(path, &original)?;
    for version in from_version..METADATA_SCHEMA_VERSION {
        upgrade_step(path, version, &mut raw)?;
    }
    let document: MetadataDocument = serde_json::from_value(raw)
        .map_err(|error| corrupt(path, format!("validate upgraded metadata: {error}")))?;
    let encoded = serde_json::to_vec(&document).map_err(|error| {
        SessionV1Error::atomic_write(path, "serialize", format!("serialize metadata: {error}"))
    })?;
    keep_original_bytes(path, from_version, &original)?;
    write_file_atomic(path, &encoded, "metadata")?;
    Ok(document)
}

/// Converts one raw document from `version` to `version + 1`.
/// 将一份原始文档从 `version` 转换到 `version + 1`。
fn upgrade_step(
    path: &Path,
    version: u32,
    raw: &mut serde_json::Value,
) -> Result<(), SessionV1Error> {
    match version {
        // 10 adds the optional `config.selection.request_options`; absence means defaults.
        // 10 新增可选的 `config.selection.request_options`；缺省即默认值。
        9 => {}
        // 11 permits a null maintenance child identity for explicit root compaction; existing identities remain exact.
        // 11 允许显式主会话压缩使用空子执行身份；已有身份原样保留。
        10 => {}
        other => {
            return Err(SessionV1Error::unsupported(
                path,
                format!("no upgrade step from metadata schema {other}"),
            ));
        }
    }
    let object = raw
        .as_object_mut()
        .ok_or_else(|| corrupt(path, "metadata root must be an object"))?;
    object.insert(String::from("schema_version"), (version + 1).into());
    Ok(())
}

/// Stores the untouched original bytes beside the metadata file before it is replaced.
/// 在元数据文件被替换前，将未改动的原字节保存在其旁边。
///
/// An existing backup with identical bytes is kept (an earlier attempt failed after writing it);
/// different bytes are a conflict and stop the upgrade so no original is ever overwritten.
/// 已存在且字节相同的备份予以保留（先前尝试在写入备份后失败）；字节不同属于冲突并中止升级，
/// 从而绝不覆盖任何原始文件。
fn keep_original_bytes(path: &Path, version: u32, original: &[u8]) -> Result<(), SessionV1Error> {
    let backup = path.with_file_name(format!("metadata.v{version}.backup.json"));
    match fs::read(&backup) {
        Ok(existing) if existing == original => return Ok(()),
        Ok(_) => {
            return Err(SessionV1Error::io(
                &backup,
                "a different metadata backup already exists; refusing to overwrite it",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(SessionV1Error::io(
                &backup,
                format!("inspect metadata backup: {error}"),
            ));
        }
    }
    write_file_atomic(&backup, original, "session upgrade backup")
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
