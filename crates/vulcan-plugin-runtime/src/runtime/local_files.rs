//! Trusted plugin package file access and local state document I/O.
//! 可信插件包文件访问与本地状态文档读写。

use super::VulcanPluginRuntimeError;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Canonicalizes one package-relative regular file without allowing root escape.
/// 规范化一个插件包相对普通文件，且不允许逃逸包根目录。
/// Resolves relative within root; label identifies errors, and success returns the contained canonical file.
/// 在root内解析relative；label标识错误，成功返回包内规范文件。
pub(super) fn canonical_plugin_file(
    root: &Path,
    relative: &str,
    label: &str,
) -> Result<PathBuf, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve Vulcan plugin package root: {error}"))?;
    let path = canonical_root
        .join(relative)
        .canonicalize()
        .map_err(|error| format!("Failed to resolve Vulcan plugin {label}: {error}"))?;
    if !path.starts_with(&canonical_root) || !path.is_file() {
        return Err(format!(
            "Vulcan plugin {label} escapes its package root or is not a regular file."
        ));
    }
    Ok(path)
}

/// Atomically writes one strongly typed plugin state record.
/// 原子写入一条强类型插件状态记录。
///
/// # Parameters
/// 参数
/// `path` is the final state path, `record` is serialized as pretty JSON, and `label` identifies failures.
/// `path` 是最终状态路径，`record` 序列化为格式化 JSON，`label` 标识失败类型。
/// Returns unit after atomic publication or an encoding/write diagnostic.
/// 原子发布后返回空值，否则返回编码/写入诊断。
pub(super) fn write_plugin_json_record(
    path: &Path,
    record: &impl Serialize,
    label: &str,
) -> Result<(), VulcanPluginRuntimeError> {
    let encoded = serde_json::to_vec_pretty(record).map_err(|error| {
        VulcanPluginRuntimeError::new(format!("Failed to encode Vulcan plugin {label}: {error}"))
    })?;
    crate::atomic_file::write_file_atomically(path, &encoded).map_err(VulcanPluginRuntimeError::new)
}

/// Recursively copies a plugin package while rejecting symlinks and special files.
/// 递归复制插件包，同时拒绝符号链接与特殊文件。
///
/// # Parameters
/// 参数
/// `source` is a validated package directory and `destination` is a fresh staging directory.
/// `source` 是已校验插件包目录，`destination` 是全新的暂存目录。
/// Returns unit after copying or an explicit file-type/I/O error; transaction cleanup remains with the caller.
/// 复制后返回空值，否则显式报告文件类型/读写错误；事务清理由调用者负责。
pub(super) fn copy_plugin_directory_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| {
        format!(
            "Failed to create plugin staging directory `{}`: {error}",
            destination.display()
        )
    })?;
    for entry in fs::read_dir(source).map_err(|error| {
        format!(
            "Failed to read plugin package `{}`: {error}",
            source.display()
        )
    })? {
        let entry =
            entry.map_err(|error| format!("Failed to read plugin package entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path).map_err(|error| {
            format!(
                "Failed to inspect plugin package entry `{}`: {error}",
                source_path.display()
            )
        })?;
        if crate::metadata_is_link_or_reparse_point(&metadata) {
            return Err(format!(
                "Plugin package entry `{}` must not be a symbolic link or reparse point.",
                source_path.display()
            ));
        }
        if metadata.is_dir() {
            copy_plugin_directory_tree(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "Failed to copy plugin package file `{}`: {error}",
                    source_path.display()
                )
            })?;
        } else {
            return Err(format!(
                "Plugin package entry `{}` has an unsupported file type.",
                source_path.display()
            ));
        }
    }
    Ok(())
}

/// Reads one regular plugin JSON document after rejecting links and reparse points.
/// 在拒绝链接与重解析点后读取一个普通插件 JSON 文档。
///
/// # Parameters
/// 参数
/// `path` is the exact document path and `label` identifies it in diagnostics.
/// `path` 是精确文档路径，`label` 用于诊断标识。
///
/// # Returns
/// 返回值
/// Returns the decoded JSON value.
/// 返回解码后的 JSON 值。
pub(super) fn read_plugin_json_state(
    path: &Path,
    label: &str,
) -> Result<Value, VulcanPluginRuntimeError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to inspect Vulcan plugin {label} `{}`: {error}",
            path.display()
        ))
    })?;
    if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_file() {
        return Err(VulcanPluginRuntimeError::new(format!(
            "Vulcan plugin {label} `{}` must be a regular non-symbolic-link or reparse-point file.",
            path.display()
        )));
    }
    let bytes = fs::read(path).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to read Vulcan plugin {label} `{}`: {error}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to decode Vulcan plugin {label} `{}`: {error}",
            path.display()
        ))
    })
}
