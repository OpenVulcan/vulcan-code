//! Preserve exact rollback records until package and state restoration both succeed.
//! 保留精确回滚记录，直至插件包与状态均恢复成功。

use super::{VulcanPluginRuntimeError, write_plugin_json_record};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Private snapshot filename inside the unique update transaction directory.
/// 唯一更新事务目录内的私有快照文件名。
pub(super) const ROLLBACK_RECORDS_FILE: &str = "rollback-records.json";

/// Exact pre-update state bytes retained independently of the in-memory transaction.
/// 独立于内存事务保留的精确更新前状态字节。
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateRollbackRecords {
    /// Private snapshot format version; this record alone does not authorize recovery.
    /// 私有快照格式版本；仅此记录不构成恢复授权。
    schema_version: u32,
    /// Validated plugin identity whose records were snapshotted.
    /// 被快照记录所属的已校验插件身份。
    plugin_id: String,
    /// Original authorization bytes without normalization or secret expansion.
    /// 未经规范化或秘密展开的原始授权字节。
    authorization: Vec<u8>,
    /// Original installation bytes without normalization.
    /// 未经规范化的原始安装记录字节。
    installation: Vec<u8>,
}

/// Persist both original records before moving the old package or publishing new state.
/// 移动旧插件包或发布新状态前，持久保存两份原始记录。
/// `transaction_root` is the newly created private directory; `plugin_id` is validated.
/// `transaction_root` 为新建私有目录；`plugin_id` 已通过校验。
/// `authorization` and `installation` contain the original bytes; returns a write failure explicitly.
/// `authorization` 与 `installation` 包含原始字节；明确返回写入失败。
pub(super) fn preserve_rollback_records(
    transaction_root: &Path,
    plugin_id: &str,
    authorization: &[u8],
    installation: &[u8],
) -> Result<(), VulcanPluginRuntimeError> {
    // Snapshot bytes remain available even after the returned transaction or error is dropped.
    // 即使返回事务或错误被丢弃，快照字节仍可访问。
    let records = UpdateRollbackRecords {
        schema_version: 1,
        plugin_id: plugin_id.to_owned(),
        authorization: authorization.to_vec(),
        installation: installation.to_vec(),
    };
    write_plugin_json_record(
        &transaction_root.join(ROLLBACK_RECORDS_FILE),
        &records,
        "update rollback snapshot",
    )
}

/// Inspect one package directory without converting an unreadable or redirected path into absence.
/// 检查一个插件目录，不把不可读或被重定向的路径转为不存在。
/// `path` is an exact host-owned package location; returns false only for a missing path.
/// `path` 为精确的宿主管理插件位置；仅路径缺失时返回假。
fn package_directory_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_dir() =>
        {
            Err(format!(
                "Rollback package path `{}` must be a real directory.",
                path.display()
            ))
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "Failed to inspect rollback package path `{}`: {error}",
            path.display()
        )),
    }
}

/// Check original package identity before permitting removal of recovery material.
/// 允许删除恢复材料前检查原始插件包身份。
/// `path` must be a real directory; `expected_fingerprint` is frozen from the installed catalog.
/// `path` 必须为真实目录；`expected_fingerprint` 从安装目录快照中固定。
/// Returns an explicit error when reading or matching the complete content fails.
/// 完整内容读取或匹配失败时返回明确错误。
pub(super) fn verify_original_package(
    path: &Path,
    expected_fingerprint: &str,
) -> Result<(), String> {
    if !package_directory_exists(path)? {
        return Err(format!(
            "Original rollback package `{}` is missing.",
            path.display()
        ));
    }
    // The existing fingerprint implementation requires a canonical package root.
    // 现有指纹实现要求规范化的插件包根目录。
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("Failed to resolve rollback package: {error}"))?;
    if crate::catalog::compute_package_fingerprint(&canonical)? != expected_fingerprint {
        return Err(format!(
            "Rollback package `{}` differs from the original installed contents.",
            path.display()
        ));
    }
    Ok(())
}

/// Restore only the verified original generation, retaining conflicting or unreadable paths.
/// 仅恢复已验证的原始代次，保留存在冲突或不可读的路径。
/// The two paths identify the private transaction and formal destination; the fingerprint pins original contents.
/// 两个路径标识私有事务与正式目标；指纹固定原始内容。
/// Returns success only when the formal destination contains the original package.
/// 仅正式目标包含原始插件包时返回成功。
fn restore_original_package(
    transaction_root: &Path,
    destination: &Path,
    expected_fingerprint: &str,
) -> Result<(), String> {
    // The private location may be absent when preparation failed before moving the original package.
    // 准备过程在移动原始插件包前失败时，私有位置可以不存在。
    let previous_package = transaction_root.join("previous");
    if package_directory_exists(&previous_package)? {
        verify_original_package(&previous_package, expected_fingerprint)?;
        if package_directory_exists(destination)? {
            fs::remove_dir_all(destination).map_err(|error| {
                format!("Failed to remove candidate package during update rollback: {error}")
            })?;
        }
        fs::rename(&previous_package, destination).map_err(|error| {
            format!("Failed to restore previous package during update rollback: {error}")
        })?;
    }
    if !package_directory_exists(destination)? {
        return Err("Original plugin package is missing from both rollback locations.".into());
    }
    verify_original_package(destination, expected_fingerprint)
}

/// Restore a failed preparation while retaining every recovery artifact after any failure.
/// 恢复失败的准备操作；任一步失败后均保留全部剩余恢复材料。
/// `transaction_root` and `destination` are the exact host-owned package locations.
/// `transaction_root` 与 `destination` 为精确的宿主管理插件包位置。
/// The state paths and byte slices are the original authorization and installation snapshots.
/// 状态路径与字节切片为原始授权及安装记录快照。
/// `previous_fingerprint` pins the original package whose successful restoration permits cleanup.
/// `previous_fingerprint` 固定原始插件包，仅其恢复成功才允许清理。
/// Returns all restoration errors; only a completely successful restore permits transaction removal.
/// 返回全部恢复错误；仅完整恢复成功才允许删除事务目录。
pub(super) fn rollback_failed_preparation(
    transaction_root: &Path,
    destination: &Path,
    authorization_path: &Path,
    installation_path: &Path,
    previous_authorization: &[u8],
    previous_installation: &[u8],
    previous_fingerprint: &str,
) -> Vec<String> {
    // Keep independent failures observable instead of allowing cleanup to erase their evidence.
    // 保持各项失败可观察，避免清理擦除其证据。
    let mut failures = Vec::new();
    if let Err(error) =
        restore_original_package(transaction_root, destination, previous_fingerprint)
    {
        failures.push(error);
    }
    // Restore each original record independently so one failure does not hide another.
    // 独立恢复各原始记录，避免一项失败掩盖另一项。
    for (path, bytes, label) in [
        (authorization_path, previous_authorization, "authorization"),
        (installation_path, previous_installation, "installation"),
    ] {
        if let Err(error) = crate::atomic_file::write_file_atomically(path, bytes) {
            failures.push(format!(
                "Failed to restore previous plugin {label} record: {error}"
            ));
        }
    }
    // A failed restore must never delete the only surviving old package or record snapshots.
    // 恢复失败时绝不能删除唯一剩余的旧插件包或记录快照。
    if failures.is_empty()
        && let Err(error) = fs::remove_dir_all(transaction_root)
    {
        failures.push(format!("Failed to remove update transaction root: {error}"));
    }
    failures
}

#[cfg(test)]
mod tests;
