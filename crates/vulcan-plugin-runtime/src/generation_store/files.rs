//! Strict filesystem primitives shared by every generation-store operation.
//! 所有代次存储操作共用的严格文件系统基础操作。

use super::{
    MAX_RECORD_BYTES, VulcanPluginGenerationIdentity, VulcanPluginStoreError as Error,
    VulcanPluginStoreErrorCode as Code, VulcanPluginStoreResult as Result,
};
use serde::{Serialize, de::DeserializeOwned};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Read;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Validate plugin and generation path components; returns errors before constructing filesystem paths.
/// 校验插件与代次路径组件；在构造文件系统路径前返回错误。
pub(super) fn validate_identity(identity: &VulcanPluginGenerationIdentity) -> Result<()> {
    crate::catalog::validate_identifier(&identity.plugin_id, "generation plugin id")
        .map_err(Error::invalid)?;
    // Canonical UUID text excludes traversal, aliases, and alternate encodings of the same directory.
    // 规范 UUID 文本排除路径遍历、别名及同一目录的其他编码。
    let uuid = Uuid::parse_str(&identity.generation_id)
        .map_err(|_| Error::invalid("Invalid package generation UUID."))?;
    if uuid.to_string() != identity.generation_id || identity.version.is_empty() {
        return Err(Error::invalid(
            "Package generation identity is not canonical.",
        ));
    }
    validate_content_fingerprint(&identity.content_fingerprint)
}

/// Validate canonical SHA-256 content spelling shared by package and predecessor identities.
/// 校验插件包及前驱身份共用的规范 SHA-256 内容拼写。
/// The fingerprint must match the existing package digest contract; return no filesystem access.
/// fingerprint 必须匹配既有插件包摘要契约；不执行文件系统访问。
pub(super) fn validate_content_fingerprint(fingerprint: &str) -> Result<()> {
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::invalid(
            "Package content fingerprint is not canonical.",
        ));
    }
    Ok(())
}

/// Inspect `path` without following links; returns false only for an actually missing entry.
/// 不跟随链接检查 `path`；仅条目确实缺失时返回假。
/// `directory` chooses the required regular entry type.
/// `directory` 选择要求的普通条目类型。
pub(super) fn exists(path: &Path, directory: bool) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if crate::metadata_is_link_or_reparse_point(&metadata)
                || (directory && !metadata.is_dir())
                || (!directory && !metadata.is_file())
            {
                return Err(Error::invalid(format!(
                    "Generation store entry `{}` has an invalid type or is redirected.",
                    path.display()
                )));
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(Error::io("Inspect generation store entry", error)),
    }
}

/// Create exactly one controlled child directory and validate existing entries; returns its path.
/// 创建一个受控子目录并校验既有条目；返回其路径。
/// `parent` is an already checked directory and `name` is a fixed or validated single component.
/// `parent` 为已检查目录，`name` 为固定或已校验的单一组件。
pub(super) fn ensure_child(parent: &Path, name: &str) -> Result<PathBuf> {
    // Do not traverse an unchecked replacement of a previously canonical parent.
    // 不穿越先前规范父目录未经检查的替换对象。
    if !exists(parent, true)? {
        return Err(Error::invalid("Generation store parent is missing."));
    }
    // The caller owns component validation; creation never recursively follows intermediate links.
    // 调用方负责组件校验；创建过程不递归跟随中间链接。
    let child = parent.join(name);
    match fs::create_dir(&child) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(Error::io("Create generation store directory", error)),
    }
    if !exists(&child, true)? {
        return Err(Error::invalid(
            "Generation store directory disappeared after creation.",
        ));
    }
    Ok(child)
}

/// Open a permanent lock inode and acquire a nonblocking shared or exclusive lease.
/// 打开永久锁文件，并取得非阻塞共享或排他租约。
/// `path` is below a checked lock directory; `shared` pins content instead of selecting a writer.
/// `path` 位于已检查锁目录下；`shared` 表示固定内容而非选定写入者。
/// The returned file owns the actual OS lock; the caller must retain it through the protected operation.
/// 返回文件拥有实际操作系统锁；调用方须在受保护操作期间持有它。
pub(super) fn lock(path: &Path, shared: bool) -> Result<File> {
    exists(path, false)?;
    // Opening without truncation preserves one inode across all current and future owners.
    // 不截断打开，确保当前及未来所有者共用同一文件对象。
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| Error::io("Open generation store lock", error))?;
    exists(path, false)?;
    // Shared and exclusive ownership are both enforced by actual filesystem locks.
    // 共享及排他所有权均由实际文件系统锁约束。
    let result = if shared {
        file.try_lock_shared()
    } else {
        file.try_lock()
    };
    match result {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(Error::new(
            Code::Busy,
            "A generation store owner still holds the requested lease.",
        )),
        Err(TryLockError::Error(error)) => Err(Error::io("Lock generation store entry", error)),
    }
}

/// Read regular-file bytes from `path` within `maximum`; returns absence only for a missing entry.
/// 在 `maximum` 内读取 `path` 的普通文件字节；仅条目缺失时返回不存在。
pub(super) fn read_bytes(path: &Path, maximum: u64) -> Result<Option<Vec<u8>>> {
    if !exists(path, false)? {
        return Ok(None);
    }
    // Limit the actual read as well as checking metadata so concurrent growth cannot exceed the bound.
    // 除元数据检查外，还限制实际读取，避免并发增长突破边界。
    let file = File::open(path).map_err(|error| Error::io("Open generation record", error))?;
    if file
        .metadata()
        .map_err(|error| Error::io("Inspect generation record", error))?
        .len()
        > maximum
    {
        return Err(Error::invalid(
            "Generation metadata exceeds its encoded byte limit.",
        ));
    }
    // This bounded buffer owns the exact record bytes used by the strict decoder.
    // 此有界缓冲持有严格解码器使用的精确记录字节。
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Error::io("Read generation record", error))?;
    if bytes.len() as u64 > maximum {
        return Err(Error::invalid(
            "Generation metadata grew beyond its encoded byte limit.",
        ));
    }
    Ok(Some(bytes))
}

/// Decode strict metadata from `path` using the original metadata budget; return actual absence or the parsed record.
/// 使用原元数据预算解码 `path` 的严格元数据；返回实际缺失或已解析记录。
pub(super) fn read_record<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    read_bytes(path, MAX_RECORD_BYTES)?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|error| Error::invalid(format!("Invalid generation metadata: {error}")))
        })
        .transpose()
}

/// Atomically write one bounded record using the existing host publication primitive.
/// 使用现有宿主发布基础操作原子写入一个有界记录。
/// `path` is exact and `record` is host-owned; errors do not imply that publication was undone.
/// `path` 为精确位置，`record` 由宿主持有；错误不意味着发布已撤销。
pub(super) fn write_record(path: &Path, record: &impl Serialize) -> Result<()> {
    // Encode before performing a mutation so oversize metadata cannot partially replace an existing record.
    // 变更前先编码，避免过大元数据部分替换既有记录。
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|error| Error::invalid(format!("Encode generation metadata: {error}")))?;
    write_encoded_record(path, &bytes, MAX_RECORD_BYTES)
}

/// Verify that the actual pretty-encoded record can publish every declared phase within the sole durable record limit.
/// 验证实际美化编码记录能够在唯一持久记录上限内发布每个声明阶段。
/// record/phase/progress are typed actual schema values; returns unit or an error before any metadata or secure effects.
/// record／phase／progress 为类型化实际结构值；任何元数据或安全效果之前返回空值或错误。
pub(super) fn validate_record_phase_capacity<P: Serialize>(
    record: &impl Serialize,
    phase: &P,
    progress: &[P],
) -> Result<()> {
    // Use the same durable encoding as write_record, rather than the smaller compact retained-inventory representation.
    // 使用与 write_record 相同持久编码，而非较小紧凑保留清单表示。
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|_| Error::invalid("Cannot encode cleanup record capacity."))?;
    // Actual phase encodings derive future growth without introducing another numeric record limit.
    // 实际阶段编码推导未来增长，不引入另一数值记录上限。
    let current_size = serde_json::to_vec_pretty(phase)
        .map_err(|_| Error::invalid("Cannot encode cleanup phase capacity."))?
        .len();
    // The maximum comes from the sole schema phase list, not a guessed byte allowance.
    // 最大值来自唯一结构阶段列表，而非猜测字节额度。
    let largest_size = progress.iter().try_fold(current_size, |size, candidate| {
        serde_json::to_vec_pretty(candidate)
            .map(|encoded| size.max(encoded.len()))
            .map_err(|_| Error::invalid("Cannot encode cleanup progress capacity."))
    })?;
    // Checked exact growth reserves publication capacity before a first journal can leave an uncollectable original.
    // 检查精确增长，在首日志可能留下不可回收原实例之前预留发布容量。
    let maximum_size = bytes
        .len()
        .checked_sub(current_size)
        .and_then(|size| size.checked_add(largest_size))
        .ok_or_else(|| Error::invalid("Cleanup record capacity overflow."))?;
    if maximum_size as u64 > MAX_RECORD_BYTES {
        return Err(Error::invalid(
            "Cleanup phase exceeds the durable record byte limit.",
        ));
    }
    Ok(())
}

/// Publish pre-encoded `bytes` at `path` within `maximum`; errors require exact readback before further mutation.
/// 在 `maximum` 内向 `path` 发布预编码 `bytes`；错误要求进一步变更前精确回读。
pub(super) fn write_encoded_record(path: &Path, bytes: &[u8], maximum: u64) -> Result<()> {
    if bytes.len() as u64 > maximum {
        return Err(Error::invalid(
            "Generation metadata exceeds its encoded byte limit.",
        ));
    }
    crate::atomic_file::write_file_atomically(path, bytes).map_err(|error| {
        Error::new(
            Code::Storage,
            format!(
                "{error}; read the exact record before deciding whether publication completed."
            ),
        )
    })
}

/// Copy regular package contents into a new directory, preserving modes only after syncing each file.
/// 将普通插件内容复制到新目录，在逐文件同步后保留其权限。
/// `source` must be a validated package and `destination` must not already exist.
/// `source` 必须是已校验插件包，`destination` 必须尚不存在。
/// Returns only after all files are synchronized; errors retain the incomplete unique generation.
/// 仅全部文件同步后返回；失败保留不完整唯一代次。
pub(super) fn copy_synced(source: &Path, destination: &Path) -> Result<()> {
    if !exists(source, true)? {
        return Err(Error::invalid("Source package directory is missing."));
    }
    fs::create_dir(destination)
        .map_err(|error| Error::io("Create unique generation package", error))?;
    for entry in
        fs::read_dir(source).map_err(|error| Error::io("Enumerate generation source", error))?
    {
        // Each literal source entry is checked before choosing file or directory handling.
        // 每个字面来源条目均在选择文件或目录处理前检查。
        let entry = entry.map_err(|error| Error::io("Read generation source entry", error))?;
        // Source metadata must not follow a link into another package or host directory.
        // 来源元数据不得跟随链接进入其他插件包或宿主目录。
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| Error::io("Inspect generation source entry", error))?;
        if crate::metadata_is_link_or_reparse_point(&metadata) {
            return Err(Error::invalid(
                "Generation source contains a redirected entry.",
            ));
        }
        // Target names originate from one filesystem entry rather than caller-supplied relative paths.
        // 目标名称来自单个文件系统条目，而非调用方提供的相对路径。
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_synced(&entry.path(), &target)?;
        } else if metadata.is_file() {
            // Opening fresh descriptors prevents a caller from lending mutable buffers or aliased file offsets.
            // 打开新文件句柄，避免调用方借出可变缓冲或别名文件偏移。
            let mut input = File::open(entry.path())
                .map_err(|error| Error::io("Open source package file", error))?;
            // Exclusive creation cannot overwrite any preexisting candidate file.
            // 排他创建不能覆盖任何既有候选文件。
            let mut output = File::create_new(&target)
                .map_err(|error| Error::io("Create generation package file", error))?;
            std::io::copy(&mut input, &mut output)
                .map_err(|error| Error::io("Copy generation package file", error))?;
            output
                .set_permissions(metadata.permissions())
                .map_err(|error| Error::io("Preserve package file permissions", error))?;
            output
                .sync_all()
                .map_err(|error| Error::io("Sync generation package file", error))?;
        } else {
            return Err(Error::invalid(
                "Generation source contains a special filesystem entry.",
            ));
        }
    }
    sync_directory(destination)
}

/// Sync a completed directory where the platform supports the existing host durability convention.
/// 在平台支持现有宿主持久化约定时同步已完成目录。
/// `path` is a checked directory; this does not claim hardware power-loss verification on Windows.
/// `path` 为已检查目录；此操作不声称已经验证 Windows 硬件断电恢复。
pub(super) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| Error::io("Sync generation directory", error))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
