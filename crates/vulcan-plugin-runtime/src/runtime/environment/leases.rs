//! Actual filesystem ownership excludes cleanup while an environment is prepared or retained.
//! 实际文件系统所有权在环境准备或保留期间排除清理。

use super::VulcanPluginManagerState;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;

/// Permanent lock directory sits outside all version directories subject to reclamation.
/// 永久锁目录位于所有可能回收的版本目录之外。
const LEASE_DIRECTORY: &str = ".environment-leases";

/// Retains this runtime root's plugin environments without excluding concurrent readers.
/// 保留此运行时根的插件环境，不排除并发读取者。
/// State and plugin identity select one permanent lock; returns its actual shared file owner.
/// 状态与插件身份选择单个永久锁；返回实际共享文件所有者。
pub(in crate::runtime) fn retain_plugin_execution_environment(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
) -> Result<File, String> {
    lock_plugin_execution_environment(state, plugin_id, true)
}

/// Excludes every cooperating preparation or retained owner before recursive environment removal.
/// 递归移除环境前排除全部协作准备者或保留所有者。
/// State and plugin identity match readers; returns an exclusive owner or an explicit busy error.
/// 状态与插件身份和读取者一致；返回排他所有者或明确占用错误。
pub(in crate::runtime) fn lock_plugin_execution_environment_cleanup(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
) -> Result<File, String> {
    lock_plugin_execution_environment(state, plugin_id, false)
}

/// Acquires a permanent shared or exclusive plugin lock without replacing its filesystem identity.
/// 获取永久共享或排他插件锁，不替换其文件系统身份。
/// The selected mode separates use from deletion; returns the actual lock until its file is dropped.
/// 选定模式区分使用与删除；返回实际锁，直至其文件释放。
fn lock_plugin_execution_environment(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
    shared: bool,
) -> Result<File, String> {
    crate::catalog::validate_identifier(plugin_id, "environment lease plugin id")?;
    // The host selects the root; only checked single components are created beneath it.
    // 宿主选择根；仅在其下创建已检查的单个组件。
    fs::create_dir_all(&state.plugin_runtime_root)
        .map_err(|error| format!("Failed to create plugin environment root: {error}"))?;
    require_plain_entry(&state.plugin_runtime_root, true)?;
    // Canonical root fixes aliases before deriving a shared lock identity.
    // 规范根在推导共享锁身份前固定别名。
    let root = state
        .plugin_runtime_root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve plugin environment root: {error}"))?;
    // The permanent directory survives every plugin-version deletion.
    // 永久目录在每次插件版本删除后继续存在。
    let directory = root.join(LEASE_DIRECTORY);
    match fs::create_dir(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(format!(
                "Failed to create environment lease directory: {error}"
            ));
        }
    }
    require_plain_entry(&directory, true)?;
    // The validated plugin identity cannot escape this directory.
    // 已校验插件身份不能逃逸此目录。
    let path = directory.join(format!("{plugin_id}.lock"));
    // Existing entries must be plain files; never delete or truncate a cooperating lock inode.
    // 既有条目必须为普通文件；绝不删除或截断协作锁文件。
    match fs::symlink_metadata(&path) {
        Ok(_) => require_plain_entry(&path, false)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Failed to inspect environment lease: {error}")),
    }
    // The returned handle itself owns the operating-system lock, not an in-memory counter.
    // 返回句柄本身拥有操作系统锁，而非内存计数器。
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| format!("Failed to open environment lease: {error}"))?;
    require_plain_entry(&path, false)?;
    // Both sides are nonblocking so callers retain explicit retry and cancellation ownership.
    // 两侧均非阻塞，使调用方保留明确重试及取消所有权。
    let locked = if shared {
        file.try_lock_shared()
    } else {
        file.try_lock()
    };
    match locked {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(format!(
            "Plugin `{plugin_id}` execution environment is still owned by another preparation, resource or cleanup operation."
        )),
        Err(TryLockError::Error(error)) => {
            Err(format!("Failed to lock plugin environment: {error}"))
        }
    }
}

/// Rejects redirected or mistyped root, directory and lock entries before using them.
/// 使用根、目录及锁条目前拒绝重定向或类型错误。
/// Path is host-derived and directory selects its required type; returns validation only.
/// 路径由宿主推导，directory 选择要求类型；仅返回校验结果。
fn require_plain_entry(path: &Path, directory: bool) -> Result<(), String> {
    // Inspect the link itself rather than following its target.
    // 检查链接本身，而非跟随其目标。
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Failed to inspect environment lease path `{}`: {error}",
            path.display()
        )
    })?;
    if crate::metadata_is_link_or_reparse_point(&metadata)
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(format!(
            "Environment lease path `{}` is not a plain expected entry.",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
