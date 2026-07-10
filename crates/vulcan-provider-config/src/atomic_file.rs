//! Crash-resistant same-directory file replacement for durable configuration documents.
//! 为持久配置文档提供抗崩溃的同目录文件替换能力。

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Allocates process-local suffixes while `create_new` provides cross-process collision safety.
/// 分配进程内后缀，同时由 `create_new` 提供跨进程碰撞安全性。
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Replaces one file through a fully written and synchronized sibling temporary file.
/// 通过完整写入并同步的同级临时文件替换一个文件。
///
/// # Parameters
/// 参数
/// `target` is the final file path whose parent directory defines the atomic rename boundary.
/// `target` 是最终文件路径，其父目录定义原子重命名边界。
/// `contents` contains the complete next file image.
/// `contents` 包含下一版本文件的完整内容。
///
/// # Returns
/// 返回值
/// `Ok(())` after the staged bytes are synchronized and atomically renamed onto the target.
/// 暂存字节完成同步并原子重命名到目标后返回 `Ok(())`。
pub(crate) fn write_file_atomically(target: &Path, contents: &[u8]) -> Result<(), String> {
    // parent is intentionally shared by the temporary and target files so rename stays on one filesystem.
    // parent 刻意由临时文件与目标文件共享，使重命名保持在同一文件系统内。
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create parent directory `{}`: {error}",
            parent.display()
        )
    })?;

    // existing_permissions preserves the access policy of a regular target across temp-file replacement.
    // existing_permissions 在临时文件替换期间保留普通目标文件的访问策略。
    let existing_permissions = match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "refusing to atomically replace symbolic-link target `{}`",
                target.display()
            ));
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "failed to inspect existing target `{}`: {error}",
                target.display()
            ));
        }
    };

    // staged_path and staged_file are exclusively created, preventing concurrent writers from sharing scratch data.
    // staged_path 与 staged_file 以独占方式创建，防止并发写入者共享暂存数据。
    let (staged_path, mut staged_file) = create_sibling_temporary_file(target, parent)?;
    // staging_result writes complete bytes, preserves an existing access policy, and reaches durable storage before publication.
    // staging_result 在发布前写入完整字节、保留现有访问策略并到达持久存储。
    let staging_result = (|| -> std::io::Result<()> {
        staged_file.write_all(contents)?;
        if let Some(permissions) = existing_permissions {
            staged_file.set_permissions(permissions)?;
        }
        staged_file.sync_all()
    })();
    if let Err(error) = staging_result {
        drop(staged_file);
        remove_staged_file(&staged_path);
        return Err(format!(
            "failed to stage and synchronize `{}`: {error}",
            staged_path.display()
        ));
    }
    drop(staged_file);

    // rename publishes the complete file image in one step; a failure leaves the previous target untouched.
    // rename 一次性发布完整文件内容；失败时旧目标保持不变。
    if let Err(error) = fs::rename(&staged_path, target) {
        remove_staged_file(&staged_path);
        return Err(format!(
            "failed to replace `{}` from staged file `{}`: {error}",
            target.display(),
            staged_path.display()
        ));
    }

    // POSIX durability requires synchronizing the directory entry after rename; Rust has no portable Windows directory-sync API.
    // POSIX 持久性要求重命名后同步目录项；Rust 尚无可移植的 Windows 目录同步 API。
    #[cfg(unix)]
    synchronize_parent_directory(parent)?;

    Ok(())
}

/// Creates one unique sibling temporary file without opening any existing candidate.
/// 创建一个唯一的同级临时文件，且不会打开任何已存在的候选文件。
///
/// # Parameters
/// 参数
/// `target` supplies the human-recognizable base name for temporary artifacts.
/// `target` 为临时产物提供可识别的基础名称。
/// `parent` is the already-created directory shared with the final target.
/// `parent` 是已经创建并与最终目标共享的目录。
///
/// # Returns
/// 返回值
/// The unique temporary path and its exclusively created writable file handle.
/// 返回唯一临时路径及其独占创建的可写文件句柄。
fn create_sibling_temporary_file(target: &Path, parent: &Path) -> Result<(PathBuf, File), String> {
    // target_name preserves non-Unicode file names instead of converting lock or persistence identity through lossy text.
    // target_name 保留非 Unicode 文件名，避免通过有损文本转换持久化身份。
    let target_name = target
        .file_name()
        .ok_or_else(|| format!("target path `{}` has no file name", target.display()))?;

    // Every collision consumes a new suffix; `create_new` is the authoritative cross-process uniqueness check.
    // 每次碰撞都会消耗新后缀；`create_new` 是跨进程唯一性的权威检查。
    loop {
        // sequence distinguishes concurrent attempts within this process without requiring a new dependency.
        // sequence 无需引入新依赖即可区分当前进程内的并发尝试。
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        // staged_name combines the exact target name, process id, and process-local sequence.
        // staged_name 组合精确目标名称、进程标识与进程内序号。
        let mut staged_name = OsString::from(".");
        staged_name.push(target_name);
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
                    "failed to create staged file `{}`: {error}",
                    staged_path.display()
                ));
            }
        }
    }
}

/// Removes one staged file after a failed write or rename attempt.
/// 在写入或重命名失败后删除一个暂存文件。
///
/// # Parameters
/// 参数
/// `staged_path` is the exclusively owned temporary path to clean up.
/// `staged_path` 是需要清理的独占临时路径。
///
/// # Returns
/// 返回值
/// This best-effort cleanup intentionally has no return value because the primary persistence error remains authoritative.
/// 此尽力清理刻意不返回值，因为首要持久化错误保持权威性。
fn remove_staged_file(staged_path: &Path) {
    let _ = fs::remove_file(staged_path);
}

/// Synchronizes the parent directory entry after a successful POSIX rename.
/// 在 POSIX 重命名成功后同步父目录项。
///
/// # Parameters
/// 参数
/// `parent` is the directory containing both the staged and final files.
/// `parent` 是同时包含暂存文件与最终文件的目录。
///
/// # Returns
/// 返回值
/// `Ok(())` after directory metadata reaches the storage device.
/// 目录元数据到达存储设备后返回 `Ok(())`。
#[cfg(unix)]
fn synchronize_parent_directory(parent: &Path) -> Result<(), String> {
    // directory is opened only after rename so a sync error cannot expose a partial target file.
    // directory 仅在重命名后打开，因此同步错误不会暴露部分目标文件。
    let directory = File::open(parent).map_err(|error| {
        format!(
            "failed to open parent directory `{}` for synchronization: {error}",
            parent.display()
        )
    })?;
    directory.sync_all().map_err(|error| {
        format!(
            "failed to synchronize parent directory `{}`: {error}",
            parent.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies atomic replacement publishes complete bytes and leaves no sibling staging artifact.
    /// 验证原子替换会发布完整字节且不遗留同级暂存产物。
    #[test]
    fn atomic_write_replaces_existing_file_without_staging_artifacts() {
        // root isolates this file-system assertion from other concurrently running tests.
        // root 将该文件系统断言与其他并发测试隔离。
        let root = create_unique_test_directory("replace");
        // target starts with an older complete file image that must be replaced in one operation.
        // target 初始包含一份旧的完整文件内容，必须通过单次操作完成替换。
        let target = root.join("config.json");
        fs::write(&target, b"old configuration").expect("old target should be written");

        write_file_atomically(&target, b"new complete configuration")
            .expect("atomic replacement should succeed");

        // persisted captures the final bytes before cleanup.
        // persisted 在清理前捕获最终字节。
        let persisted = fs::read(&target).expect("replaced target should remain readable");
        // remaining_entries proves the helper removed or renamed every staging artifact.
        // remaining_entries 证明辅助函数已经删除或重命名全部暂存产物。
        let remaining_entries = fs::read_dir(&root)
            .expect("test directory should be readable")
            .count();
        fs::remove_dir_all(&root).expect("atomic replacement test directory should be removed");

        assert_eq!(persisted, b"new complete configuration");
        assert_eq!(remaining_entries, 1);
    }

    /// Verifies a failed rename preserves the target and cleans the exclusively owned staged file.
    /// 验证重命名失败时会保留目标并清理独占暂存文件。
    #[test]
    fn atomic_write_cleans_staged_file_after_rename_failure() {
        // root isolates the deliberately invalid file-to-directory replacement attempt.
        // root 隔离刻意构造的文件到目录替换失败场景。
        let root = create_unique_test_directory("rename-failure");
        // target_directory cannot be replaced by a regular staged file and therefore forces rename failure.
        // target_directory 无法被普通暂存文件替换，因此会强制触发重命名失败。
        let target_directory = root.join("config.json");
        fs::create_dir(&target_directory).expect("target directory should be created");
        fs::write(target_directory.join("retained.txt"), b"retained")
            .expect("target directory content should be written");

        // result captures the expected publication failure after successful staging.
        // result 捕获成功暂存后的预期发布失败。
        let result = write_file_atomically(&target_directory, b"replacement");
        // remaining_entries excludes descendants and therefore detects leaked sibling staging files.
        // remaining_entries 不包含后代项，因此可检测泄漏的同级暂存文件。
        let remaining_entries = fs::read_dir(&root)
            .expect("test directory should be readable")
            .count();
        // retained_contents proves failure did not modify the existing target tree.
        // retained_contents 证明失败没有修改现有目标目录树。
        let retained_contents = fs::read(target_directory.join("retained.txt"))
            .expect("existing target contents should remain readable");
        fs::remove_dir_all(&root).expect("rename failure test directory should be removed");

        assert!(result.is_err());
        assert_eq!(remaining_entries, 1);
        assert_eq!(retained_contents, b"retained");
    }

    /// Verifies replacing an existing Unix configuration file retains its restrictive permission mode.
    /// 验证替换现有 Unix 配置文件时会保留其限制性权限模式。
    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;

        // root isolates the permission assertion from the caller's actual umask and configuration files.
        // root 将权限断言与调用方真实 umask 和配置文件隔离。
        let root = create_unique_test_directory("permissions");
        // target represents a credential-bearing configuration restricted to its owner.
        // target 表示仅允许所有者访问的含凭据配置。
        let target = root.join("config.json");
        fs::write(&target, b"old configuration").expect("old target should be written");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600))
            .expect("old target permissions should be restricted");

        write_file_atomically(&target, b"new configuration")
            .expect("atomic replacement should succeed");

        // persisted_mode captures only Unix permission bits before test cleanup.
        // persisted_mode 在测试清理前仅捕获 Unix 权限位。
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
        // create_dir is the authoritative collision check across concurrent tests and processes.
        // create_dir 是跨并发测试与进程的权威碰撞检查。
        loop {
            // sequence shares the production monotonic allocator while remaining isolated by the test label.
            // sequence 复用生产单调分配器，同时由测试标签保持可识别性。
            let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            // candidate remains under the operating-system temporary root and includes the current process id.
            // candidate 位于操作系统临时根目录下，并包含当前进程标识。
            let candidate = std::env::temp_dir().join(format!(
                "vulcan-provider-config-atomic-{label}-{}-{sequence}",
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
