//! Crash-resistant same-directory replacement for plugin state documents.
//! 面向插件状态文档的抗崩溃同目录替换实现。

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-local suffix source paired with create-new filesystem semantics.
/// 与文件系统独占创建语义配合的进程内后缀源。
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Writes and synchronizes one complete sibling file before atomically publishing it.
/// 在原子发布前写入并同步一个完整的同级文件。
///
/// # Parameters
/// 参数
/// `target` is the final state file and `contents` is its complete next image.
/// `target` 是最终状态文件，`contents` 是其完整下一版本内容。
///
/// # Returns
/// 返回值
/// Returns success after publication or a diagnostic that leaves the old target untouched.
/// 发布成功后返回成功；失败诊断保证旧目标不受影响。
pub(crate) fn write_file_atomically(target: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            format!(
                "Plugin state target `{}` has no parent directory.",
                target.display()
            )
        })?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Failed to create plugin state directory `{}`: {error}",
            parent.display()
        )
    })?;
    let existing_permissions = match fs::symlink_metadata(target) {
        Ok(metadata) if crate::metadata_is_link_or_reparse_point(&metadata) => {
            return Err(format!(
                "Refusing to replace symbolic-link or reparse-point plugin state `{}`.",
                target.display()
            ));
        }
        Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
        Ok(_) => {
            return Err(format!(
                "Plugin state target `{}` is not a regular file.",
                target.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "Failed to inspect plugin state `{}`: {error}",
                target.display()
            ));
        }
    };
    let (staged_path, mut staged_file) = create_sibling_temporary_file(target, parent)?;
    let staging_result = (|| -> std::io::Result<()> {
        staged_file.write_all(contents)?;
        if let Some(permissions) = existing_permissions {
            staged_file.set_permissions(permissions)?;
        }
        staged_file.sync_all()
    })();
    if let Err(error) = staging_result {
        drop(staged_file);
        let _ = fs::remove_file(&staged_path);
        return Err(format!(
            "Failed to stage plugin state `{}`: {error}",
            staged_path.display()
        ));
    }
    drop(staged_file);
    if let Err(error) = fs::rename(&staged_path, target) {
        let _ = fs::remove_file(&staged_path);
        return Err(format!(
            "Failed to publish plugin state `{}`: {error}",
            target.display()
        ));
    }
    #[cfg(unix)]
    synchronize_parent_directory(parent)?;
    Ok(())
}

/// Creates one unique writable sibling without opening an existing candidate.
/// 创建一个唯一可写同级文件，且不会打开已存在候选文件。
///
/// # Parameters
/// 参数
/// `target` supplies the base name and `parent` is the shared filesystem boundary.
/// `target` 提供基础名称，`parent` 是共享文件系统边界。
///
/// # Returns
/// 返回值
/// Returns the staged path and exclusively created file handle.
/// 返回暂存路径与独占创建的文件句柄。
fn create_sibling_temporary_file(target: &Path, parent: &Path) -> Result<(PathBuf, File), String> {
    let target_name = target.file_name().ok_or_else(|| {
        format!(
            "Plugin state target `{}` has no file name.",
            target.display()
        )
    })?;
    for _ in 0..128 {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut name = OsString::from(".");
        name.push(target_name);
        name.push(format!(".{}.{}.tmp", std::process::id(), sequence));
        let path = parent.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to create staged plugin state `{}`: {error}",
                    path.display()
                ));
            }
        }
    }
    Err(format!(
        "Failed to allocate a unique staged file for plugin state `{}`.",
        target.display()
    ))
}

/// Synchronizes a Unix parent directory after publishing its child entry.
/// 在发布子目录项后同步 Unix 父目录。
#[cfg(unix)]
fn synchronize_parent_directory(parent: &Path) -> Result<(), String> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            format!(
                "Failed to synchronize plugin state directory `{}`: {error}",
                parent.display()
            )
        })
}
