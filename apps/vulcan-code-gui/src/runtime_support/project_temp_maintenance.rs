//! Project-scoped temporary-directory helpers used by the desktop host.
//! 桌面宿主使用的项目级临时目录辅助模块。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Shared retention window applied to project-scoped temp files before cleanup removes them.
/// 项目级临时文件在清理前允许保留的共享时间窗口。
const PROJECT_TEMP_FILE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Shared hidden Vulcan metadata directory name stored below one project root.
/// 存放在项目根目录下的共享隐藏 Vulcan 元数据目录名称。
const PROJECT_VULCAN_DIRECTORY_NAME: &str = ".vulcan";

/// Shared temp directory name stored below one project-scoped Vulcan metadata directory.
/// 存放在项目级 Vulcan 元数据目录下的共享临时目录名称。
const PROJECT_TEMP_DIRECTORY_NAME: &str = "temp";

/// Resolve the project-scoped temp directory used by host-managed oversized tool output files.
/// 解析宿主管理超长工具输出文件使用的项目级临时目录。
pub fn resolve_project_temp_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(PROJECT_VULCAN_DIRECTORY_NAME)
        .join(PROJECT_TEMP_DIRECTORY_NAME)
}

/// Ensure the project-scoped temp directory exists and return the resolved path.
/// 确保项目级临时目录存在，并返回解析后的目录路径。
pub fn ensure_project_temp_dir(project_root: &Path) -> Result<PathBuf, String> {
    let temp_dir = resolve_project_temp_dir(project_root);
    fs::create_dir_all(&temp_dir).map_err(|error| {
        format!(
            "创建项目临时目录失败（{}）：{}。",
            temp_dir.display(),
            error
        )
    })?;
    Ok(temp_dir)
}

/// Perform one cleanup pass for a single project temp directory and silently skip missing projects.
/// 对单个项目临时目录执行一次清理，并在项目不存在时静默跳过。
pub fn maintain_project_temp_dir(project_root: &Path) -> Result<(), String> {
    if !project_root.exists() {
        return Ok(());
    }

    let temp_dir = resolve_project_temp_dir(project_root);
    if !temp_dir.exists() {
        return Ok(());
    }
    cleanup_directory_recursive(&temp_dir, SystemTime::now(), PROJECT_TEMP_FILE_RETENTION)
}

/// Perform one cleanup pass for multiple project roots, deduplicating identical directories first.
/// 对多个项目根目录执行一次清理，并在开始前去重相同目录。
pub fn maintain_project_temp_dirs(project_roots: &[PathBuf]) -> Result<(), String> {
    let mut normalized_roots = project_roots
        .iter()
        .filter(|path| !path.as_os_str().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    normalized_roots.sort();
    normalized_roots.dedup();

    for project_root in normalized_roots {
        maintain_project_temp_dir(&project_root)?;
    }

    Ok(())
}

/// Recursively delete expired files and prune empty directories inside one project temp subtree.
/// 递归删除单个项目临时子树中的过期文件，并裁剪空目录。
fn cleanup_directory_recursive(
    directory_path: &Path,
    now: SystemTime,
    retention: Duration,
) -> Result<(), String> {
    if !directory_path.exists() {
        return Ok(());
    }

    let read_dir = fs::read_dir(directory_path).map_err(|error| {
        format!(
            "读取项目临时目录失败（{}）：{}。",
            directory_path.display(),
            error
        )
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|error| {
            format!(
                "遍历项目临时目录失败（{}）：{}。",
                directory_path.display(),
                error
            )
        })?;
        let entry_path = entry.path();
        let metadata = entry.metadata().map_err(|error| {
            format!(
                "读取临时路径元数据失败（{}）：{}。",
                entry_path.display(),
                error
            )
        })?;

        if metadata.is_dir() {
            cleanup_directory_recursive(&entry_path, now, retention)?;
            let is_empty = fs::read_dir(&entry_path)
                .map_err(|error| {
                    format!(
                        "读取子临时目录失败（{}）：{}。",
                        entry_path.display(),
                        error
                    )
                })?
                .next()
                .is_none();
            if is_empty {
                let _ = fs::remove_dir(&entry_path);
            }
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        let modified_at = metadata.modified().unwrap_or(now);
        let age = now.duration_since(modified_at).unwrap_or_default();
        if age > retention {
            let _ = fs::remove_file(&entry_path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Project temp directories should always live below the hidden `.vulcan/temp` subtree.
    /// 项目临时目录应始终位于隐藏的 `.vulcan/temp` 子树下。
    #[test]
    fn resolve_project_temp_dir_uses_hidden_vulcan_subtree() {
        let root = PathBuf::from(r"D:\projects\demo");
        assert_eq!(
            resolve_project_temp_dir(&root),
            PathBuf::from(r"D:\projects\demo\.vulcan\temp")
        );
    }
}
