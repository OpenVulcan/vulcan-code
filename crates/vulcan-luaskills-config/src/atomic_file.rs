//! Crash-resistant same-directory replacement for LuaSkills settings documents.
//! 为 LuaSkills 设置文档提供抗崩溃的同目录替换能力。

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-local suffix sequence paired with exclusive temporary-file creation.
/// 与临时文件独占创建配合使用的进程内后缀序列。
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Replace one file through a synchronized sibling staging file.
/// 通过已同步的同级暂存文件替换单个文件。
///
/// Returns success after the complete document is published atomically.
/// 完整文档完成原子发布后返回成功。
pub(crate) fn write_file_atomically(target: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create settings directory `{}`: {error}",
            parent.display()
        )
    })?;
    let existing_permissions = match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "refusing to replace symbolic-link settings target `{}`",
                target.display()
            ));
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "failed to inspect settings target `{}`: {error}",
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
        remove_staged_file(&staged_path);
        return Err(format!(
            "failed to stage LuaSkills settings `{}`: {error}",
            staged_path.display()
        ));
    }
    drop(staged_file);
    if let Err(error) = fs::rename(&staged_path, target) {
        remove_staged_file(&staged_path);
        return Err(format!(
            "failed to publish LuaSkills settings `{}`: {error}",
            target.display()
        ));
    }
    #[cfg(unix)]
    synchronize_parent_directory(parent)?;
    Ok(())
}

/// Create one unique sibling staging file without opening existing candidates.
/// 创建一个唯一同级暂存文件且不打开已有候选文件。
///
/// Returns the staging path and its exclusively created writable file.
/// 返回暂存路径及其独占创建的可写文件。
fn create_sibling_temporary_file(target: &Path, parent: &Path) -> Result<(PathBuf, File), String> {
    let target_name = target
        .file_name()
        .ok_or_else(|| format!("settings target `{}` has no file name", target.display()))?;
    loop {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut staged_name = OsString::from(".");
        staged_name.push(target_name);
        staged_name.push(format!(".vulcan-stage-{}-{sequence}", std::process::id()));
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
                    "failed to create settings staging file `{}`: {error}",
                    staged_path.display()
                ));
            }
        }
    }
}

/// Remove one staging file after a failed write or rename attempt.
/// 在写入或重命名失败后删除单个暂存文件。
fn remove_staged_file(staged_path: &Path) {
    let _ = fs::remove_file(staged_path);
}

/// Synchronize the parent directory entry after a successful POSIX rename.
/// 在 POSIX 重命名成功后同步父目录项。
#[cfg(unix)]
fn synchronize_parent_directory(parent: &Path) -> Result<(), String> {
    let directory = File::open(parent).map_err(|error| {
        format!(
            "failed to open settings directory `{}`: {error}",
            parent.display()
        )
    })?;
    directory.sync_all().map_err(|error| {
        format!(
            "failed to synchronize settings directory `{}`: {error}",
            parent.display()
        )
    })
}
