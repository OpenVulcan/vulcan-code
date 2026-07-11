//! Crash-resistant same-directory replacement for task-session files.
//! 为任务会话文件提供抗崩溃的同目录替换能力。

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Allocates process-local suffixes while `create_new` provides cross-process collision safety.
/// 分配进程内后缀，同时由 `create_new` 提供跨进程碰撞安全性。
static FILE_STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Replaces one regular file through a fully written and synchronized sibling staging file.
/// 通过完整写入并同步的同级暂存文件替换一个普通文件。
///
/// # Parameters
/// 参数
/// `target` is the final file path whose parent directory defines the atomic rename boundary.
/// `target` 是最终文件路径，其父目录定义原子重命名边界。
/// `contents` contains the complete next file image.
/// `contents` 包含下一版本文件的完整内容。
/// `artifact_kind` identifies the persisted document in diagnostics.
/// `artifact_kind` 在诊断中标识持久化文档。
///
/// # Returns
/// 返回值
/// `Ok(())` after synchronized bytes are atomically published, otherwise a contextual error.
/// 同步字节完成原子发布后返回 `Ok(())`，否则返回包含上下文的错误。
pub(crate) fn write_file_atomically(
    target: &Path,
    contents: &[u8],
    artifact_kind: &str,
) -> Result<(), String> {
    // parent is shared by target and staging files so publication never crosses filesystems.
    // parent 由目标文件与暂存文件共享，因此发布不会跨越文件系统。
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Failed to create parent directory `{}` for {artifact_kind}: {error}",
            parent.display()
        )
    })?;

    // existing_permissions preserves the access policy of an existing regular target.
    // existing_permissions 保留现有普通目标文件的访问策略。
    let existing_permissions = match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "Refusing to replace symbolic-link {artifact_kind}: {}",
                target.display()
            ));
        }
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(format!(
                "Refusing to replace non-regular-file {artifact_kind}: {}",
                target.display()
            ));
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "Failed to inspect existing {artifact_kind} `{}`: {error}",
                target.display()
            ));
        }
    };

    // staged_path and staged_file are exclusively owned by this save attempt.
    // staged_path 与 staged_file 由当前保存尝试独占。
    let (staged_path, mut staged_file) = create_unique_staging_file(target, parent, artifact_kind)?;
    // staging_result makes complete bytes and inherited permissions durable before publication.
    // staging_result 在发布前使完整字节与继承权限持久化。
    let staging_result = (|| -> std::io::Result<()> {
        staged_file.write_all(contents)?;
        if let Some(permissions) = existing_permissions {
            staged_file.set_permissions(permissions)?;
        }
        staged_file.sync_all()
    })();
    if let Err(error) = staging_result {
        drop(staged_file);
        remove_staging_file(&staged_path);
        return Err(format!(
            "Failed to stage {artifact_kind} `{}` for `{}`: {error}",
            staged_path.display(),
            target.display()
        ));
    }
    drop(staged_file);

    publish_staged_file(&staged_path, target, artifact_kind)?;

    // POSIX durability requires synchronizing the parent directory entry after rename.
    // POSIX 持久性要求重命名后同步父目录项。
    #[cfg(unix)]
    synchronize_parent_directory(target)?;

    Ok(())
}

/// Creates one unique sibling staging file without opening an existing candidate.
/// 创建一个唯一的同级暂存文件，且不会打开任何已存在的候选文件。
///
/// # Parameters
/// 参数
/// `target` supplies the exact target file name used in the staging name.
/// `target` 提供暂存名称使用的精确目标文件名。
/// `parent` is the already-created directory shared with the final target.
/// `parent` 是已经创建并与最终目标共享的目录。
/// `artifact_kind` identifies the persisted document in diagnostics.
/// `artifact_kind` 在诊断中标识持久化文档。
///
/// # Returns
/// 返回值
/// The unique staging path and its exclusively created writable handle.
/// 返回唯一暂存路径及其独占创建的可写句柄。
fn create_unique_staging_file(
    target: &Path,
    parent: &Path,
    artifact_kind: &str,
) -> Result<(PathBuf, File), String> {
    // file_name preserves non-Unicode filesystem identity in staging paths and diagnostics.
    // file_name 在暂存路径与诊断中保留非 Unicode 文件系统身份。
    let file_name = target.file_name().ok_or_else(|| {
        format!(
            "Atomic {artifact_kind} path has no file name: {}",
            target.display()
        )
    })?;

    // `create_new` is the authoritative collision check across threads and processes.
    // `create_new` 是跨线程与进程的权威碰撞检查。
    loop {
        // sequence distinguishes process-local attempts without introducing another dependency.
        // sequence 无需引入另一依赖即可区分进程内尝试。
        let sequence = FILE_STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        // staged_name combines the exact target name, process id, and process-local sequence.
        // staged_name 组合精确目标名称、进程标识与进程内序号。
        let mut staged_name = OsString::from(".");
        staged_name.push(file_name);
        staged_name.push(format!(".vulcan-stage-{}-{sequence}", std::process::id()));
        // staged_path remains a sibling of target, which is required for atomic rename guarantees.
        // staged_path 保持为目标同级文件，这是原子重命名保证的必要条件。
        let staged_path = parent.join(staged_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged_path)
        {
            Ok(file) => return Ok((staged_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to create staged {artifact_kind} `{}`: {error}",
                    staged_path.display()
                ));
            }
        }
    }
}

/// Removes one exclusively owned staging file after a failed save attempt.
/// 在保存失败后删除一个独占暂存文件。
///
/// # Parameters
/// 参数
/// `staged_path` is the temporary path owned by the failed attempt.
/// `staged_path` 是失败尝试独占的临时路径。
///
/// # Returns
/// 返回值
/// Best-effort cleanup has no return value because the primary persistence error is authoritative.
/// 尽力清理不返回值，因为首要持久化错误保持权威性。
fn remove_staging_file(staged_path: &Path) {
    let _ = fs::remove_file(staged_path);
}

/// Publishes one complete staging file and removes it if the atomic rename fails.
/// 发布一个完整暂存文件，并在原子重命名失败时将其删除。
///
/// # Parameters
/// 参数
/// `staged_path` is the exclusively owned, synchronized sibling file to publish.
/// `staged_path` 是等待发布的独占、已同步同级文件。
/// `target` is the authoritative final file path.
/// `target` 是权威最终文件路径。
/// `artifact_kind` identifies the persisted document in diagnostics.
/// `artifact_kind` 在诊断中标识持久化文档。
///
/// # Returns
/// 返回值
/// `Ok(())` after rename, otherwise an error after best-effort staging cleanup.
/// 重命名成功后返回 `Ok(())`，否则在尽力清理暂存文件后返回错误。
fn publish_staged_file(
    staged_path: &Path,
    target: &Path,
    artifact_kind: &str,
) -> Result<(), String> {
    // rename publishes one complete file image; failure leaves the previous target untouched.
    // rename 发布一份完整文件内容；失败时旧目标保持不变。
    if let Err(error) = fs::rename(staged_path, target) {
        remove_staging_file(staged_path);
        return Err(format!(
            "Failed to replace {artifact_kind} `{}` from `{}`: {error}",
            target.display(),
            staged_path.display()
        ));
    }
    Ok(())
}

/// Synchronizes the parent directory containing a newly renamed entry on POSIX.
/// 在 POSIX 上同步包含新重命名目录项的父目录。
///
/// # Parameters
/// 参数
/// `path` is the final file or directory whose parent entry was published.
/// `path` 是父目录项已发布的最终文件或目录。
///
/// # Returns
/// 返回值
/// `Ok(())` after parent directory metadata reaches durable storage.
/// 父目录元数据到达持久存储后返回 `Ok(())`。
#[cfg(unix)]
pub(crate) fn synchronize_parent_directory(path: &Path) -> Result<(), String> {
    // parent resolves to the directory containing the newly published entry.
    // parent 解析为包含新发布目录项的目录。
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // directory remains open only for the post-rename synchronization operation.
    // directory 仅在重命名后的同步操作期间保持打开。
    let directory = File::open(parent).map_err(|error| {
        format!(
            "Failed to open parent directory `{}` for synchronization: {error}",
            parent.display()
        )
    })?;
    directory.sync_all().map_err(|error| {
        format!(
            "Failed to synchronize parent directory `{}`: {error}",
            parent.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies atomic replacement publishes complete bytes and leaves no staging artifact.
    /// 验证原子替换会发布完整字节且不遗留暂存产物。
    #[test]
    fn atomic_write_replaces_existing_file_without_staging_artifacts() {
        // root isolates the file-system assertion from concurrently running tests.
        // root 将文件系统断言与并发运行的测试隔离。
        let root = create_unique_test_directory("replace");
        // target starts with an older complete image that must be replaced in one operation.
        // target 初始包含一份旧的完整内容，必须通过单次操作完成替换。
        let target = root.join("catalog.yaml");
        fs::write(&target, b"old catalog").expect("old target should be written");

        write_file_atomically(&target, b"new complete catalog", "test catalog")
            .expect("atomic replacement should succeed");

        // persisted captures final bytes before cleanup.
        // persisted 在清理前捕获最终字节。
        let persisted = fs::read(&target).expect("replaced target should remain readable");
        // remaining_entries proves no sibling staging file survived publication.
        // remaining_entries 证明发布后没有同级暂存文件残留。
        let remaining_entries = fs::read_dir(&root)
            .expect("test directory should be readable")
            .count();
        fs::remove_dir_all(&root).expect("replacement test directory should be removed");

        assert_eq!(persisted, b"new complete catalog");
        assert_eq!(remaining_entries, 1);
    }

    /// Verifies atomic writes create missing parent directories before staging.
    /// 验证原子写入会在暂存前创建缺失的父目录。
    #[test]
    fn atomic_write_creates_missing_parent_directories() {
        // root isolates the nested parent creation scenario.
        // root 隔离嵌套父目录创建场景。
        let root = create_unique_test_directory("parents");
        // target deliberately references two missing parent levels.
        // target 刻意引用两级缺失父目录。
        let target = root.join("nested").join("catalog").join("agent.yaml");

        write_file_atomically(&target, b"schemaVersion: 1\n", "test sidecar")
            .expect("atomic write should create parents");

        // persisted proves publication reached the final nested path.
        // persisted 证明发布已到达最终嵌套路径。
        let persisted = fs::read(&target).expect("nested target should be readable");
        fs::remove_dir_all(&root).expect("parent creation test directory should be removed");

        assert_eq!(persisted, b"schemaVersion: 1\n");
    }

    /// Verifies non-regular targets are rejected without changing their contents or staging files.
    /// 验证非普通目标会被拒绝，且不会改变其内容或产生暂存文件。
    #[test]
    fn atomic_write_rejects_non_regular_target_without_staging_artifacts() {
        // root isolates the deliberately invalid file-to-directory replacement attempt.
        // root 隔离刻意构造的文件到目录替换场景。
        let root = create_unique_test_directory("non-regular-target");
        // target_directory must remain a directory with its retained child after rejection.
        // target_directory 在拒绝后必须仍是目录并保留其子文件。
        let target_directory = root.join("catalog.yaml");
        fs::create_dir(&target_directory).expect("target directory should be created");
        fs::write(target_directory.join("retained.txt"), b"retained")
            .expect("target directory content should be written");

        // result captures the expected pre-publication rejection.
        // result 捕获发布前的预期拒绝。
        let result = write_file_atomically(&target_directory, b"replacement", "test catalog");
        // remaining_entries detects any leaked sibling staging artifact.
        // remaining_entries 检测任何泄漏的同级暂存产物。
        let remaining_entries = fs::read_dir(&root)
            .expect("test directory should be readable")
            .count();
        // retained_contents proves rejection did not modify the existing target tree.
        // retained_contents 证明拒绝没有修改现有目标目录树。
        let retained_contents = fs::read(target_directory.join("retained.txt"))
            .expect("existing target contents should remain readable");
        fs::remove_dir_all(&root).expect("non-regular target test directory should be removed");

        assert!(result.is_err());
        assert_eq!(remaining_entries, 1);
        assert_eq!(retained_contents, b"retained");
    }

    /// Verifies publication failure preserves the target and removes the owned staging file.
    /// 验证发布失败会保留目标并删除独占暂存文件。
    #[test]
    fn failed_atomic_publication_preserves_target_and_removes_staging_file() {
        // root isolates a deterministic file-to-directory rename failure.
        // root 隔离确定性的文件到目录重命名失败场景。
        let root = create_unique_test_directory("publication-failure");
        // target_directory represents an authoritative target tree that must remain unchanged.
        // target_directory 表示必须保持不变的权威目标目录树。
        let target_directory = root.join("catalog.yaml");
        fs::create_dir(&target_directory).expect("target directory should be created");
        fs::write(target_directory.join("retained.txt"), b"retained")
            .expect("target directory content should be written");
        // staged_path represents a complete exclusively owned file ready for publication.
        // staged_path 表示等待发布的完整独占文件。
        let staged_path = root.join(".catalog.yaml.manual-stage");
        fs::write(&staged_path, b"replacement").expect("staging file should be written");

        // result captures the forced rename failure through the production publication function.
        // result 通过生产发布函数捕获强制重命名失败。
        let result = publish_staged_file(&staged_path, &target_directory, "test catalog");
        // retained_contents proves the authoritative target was not mutated.
        // retained_contents 证明权威目标未被修改。
        let retained_contents = fs::read(target_directory.join("retained.txt"))
            .expect("existing target contents should remain readable");
        // staging_exists proves cleanup removes the exclusively owned failed publication file.
        // staging_exists 证明清理会删除发布失败的独占文件。
        let staging_exists = staged_path.exists();
        fs::remove_dir_all(&root).expect("publication failure test directory should be removed");

        assert!(result.is_err());
        assert_eq!(retained_contents, b"retained");
        assert!(!staging_exists);
    }

    /// Verifies replacing an existing Unix file retains its restrictive permission mode.
    /// 验证替换现有 Unix 文件时会保留其限制性权限模式。
    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;

        // root isolates the permission assertion from the caller's actual umask.
        // root 将权限断言与调用方真实 umask 隔离。
        let root = create_unique_test_directory("permissions");
        // target represents a private configuration restricted to its owner.
        // target 表示仅允许所有者访问的私有配置。
        let target = root.join("catalog.yaml");
        fs::write(&target, b"old catalog").expect("old target should be written");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600))
            .expect("old target permissions should be restricted");

        write_file_atomically(&target, b"new catalog", "test catalog")
            .expect("atomic replacement should succeed");

        // persisted_mode captures only Unix permission bits before cleanup.
        // persisted_mode 在清理前仅捕获 Unix 权限位。
        let persisted_mode = fs::metadata(&target)
            .expect("replaced target metadata should remain readable")
            .permissions()
            .mode()
            & 0o777;
        fs::remove_dir_all(&root).expect("permission test directory should be removed");

        assert_eq!(persisted_mode, 0o600);
    }

    /// Creates one process-unique temporary directory for atomic file tests.
    /// 为原子文件测试创建一个进程唯一的临时目录。
    ///
    /// # Parameters
    /// 参数
    /// `label` identifies the test scenario in temporary-file diagnostics.
    /// `label` 在临时文件诊断中标识测试场景。
    ///
    /// # Returns
    /// 返回值
    /// A newly created directory path owned by the current test.
    /// 返回当前测试独占的新建目录路径。
    fn create_unique_test_directory(label: &str) -> PathBuf {
        // `create_dir` is the authoritative collision check across concurrent test processes.
        // `create_dir` 是跨并发测试进程的权威碰撞检查。
        loop {
            // sequence shares the production allocator while the label keeps paths diagnosable.
            // sequence 复用生产分配器，同时由标签保持路径可诊断性。
            let sequence = FILE_STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            // candidate stays under the operating-system temporary root and includes the process id.
            // candidate 位于操作系统临时根目录下，并包含进程标识。
            let candidate = std::env::temp_dir().join(format!(
                "vulcan-task-session-atomic-{label}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => return candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!(
                    "atomic file test directory `{}` should be created: {error}",
                    candidate.display()
                ),
            }
        }
    }
}
