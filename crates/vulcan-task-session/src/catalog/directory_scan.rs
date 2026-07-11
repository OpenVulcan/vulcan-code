//! Observable and symbolic-link-safe catalog directory traversal.
//! 可观察且防符号链接的目录遍历。

use super::SubagentCatalogLoadError;
use std::fs;
use std::path::Path;

/// Determines whether an optional catalog root is an existing directory candidate.
/// 判断一个可选目录根是否为已存在的目录候选。
///
/// # Parameters
/// 参数
/// `path` is the optional root to classify before `read_dir`.
/// `path` 是在 `read_dir` 前分类的可选根路径。
/// `directory_label` identifies the root in a stable user-facing diagnostic.
/// `directory_label` 在稳定的用户诊断中标识该根目录。
/// `errors` receives type or metadata inspection failures.
/// `errors` 接收类型错误或元数据检查失败。
///
/// # Returns
/// 返回值
/// `true` only for an existing directory; absent optional roots return `false` without an error.
/// 仅对已存在目录返回 `true`；缺失的可选根返回 `false` 且不产生错误。
pub(super) fn scannable_catalog_directory(
    path: &Path,
    directory_label: &str,
    errors: &mut Vec<SubagentCatalogLoadError>,
) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => true,
        Ok(_) => {
            errors.push(SubagentCatalogLoadError {
                path: path.to_path_buf(),
                message: format!("{directory_label} exists but is not a directory."),
            });
            false
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            errors.push(SubagentCatalogLoadError {
                path: path.to_path_buf(),
                message: format!("Failed to inspect {directory_label}: {error}"),
            });
            false
        }
    }
}

/// Resolves and classifies one directory entry without following symbolic links.
/// 在不跟随符号链接的情况下解析并分类一个目录项。
///
/// # Parameters
/// 参数
/// `entry_result` is the next result yielded by `ReadDir`.
/// `entry_result` 是 `ReadDir` 产生的下一个结果。
/// `directory` is the directory whose iterator produced the result.
/// `directory` 是产生该结果的迭代器所属目录。
/// `scan_context` identifies the active catalog scan in diagnostics.
/// `scan_context` 在诊断中标识当前目录扫描。
/// `errors` receives entry, file-type, or symbolic-link policy failures.
/// `errors` 接收目录项、文件类型或符号链接策略失败。
///
/// # Returns
/// 返回值
/// The readable entry and its non-following file type, or `None` after recording a failure.
/// 返回可读目录项及其不跟随链接的文件类型，或在记录失败后返回 `None`。
pub(super) fn catalog_directory_entry(
    entry_result: std::io::Result<fs::DirEntry>,
    directory: &Path,
    scan_context: &str,
    errors: &mut Vec<SubagentCatalogLoadError>,
) -> Option<(fs::DirEntry, fs::FileType)> {
    // entry remains unavailable when the directory iterator itself fails.
    // 当目录迭代器自身失败时，entry 保持不可用。
    let entry = match entry_result {
        Ok(entry) => entry,
        Err(error) => {
            errors.push(SubagentCatalogLoadError {
                path: directory.to_path_buf(),
                message: format!("Failed to read {scan_context} directory entry: {error}"),
            });
            return None;
        }
    };
    // path identifies the exact entry for type and policy diagnostics.
    // path 标识用于类型与策略诊断的精确目录项。
    let path = entry.path();
    // file_type is obtained from DirEntry so symbolic links are not followed.
    // file_type 直接从 DirEntry 获取，因此不会跟随符号链接。
    let file_type = match entry.file_type() {
        Ok(file_type) => file_type,
        Err(error) => {
            errors.push(SubagentCatalogLoadError {
                path,
                message: format!("Failed to inspect {scan_context} directory entry type: {error}"),
            });
            return None;
        }
    };
    if reject_symbolic_link_entry(&path, file_type.is_symlink(), scan_context, errors) {
        return None;
    }
    Some((entry, file_type))
}

/// Records and rejects one symbolic-link or Windows name-surrogate reparse entry.
/// 记录并拒绝一个符号链接或 Windows 名称代理重解析目录项。
///
/// # Parameters
/// 参数
/// `path` identifies the exact link entry.
/// `path` 标识精确的链接目录项。
/// `is_symbolic_link` is the non-following `FileType` classification.
/// `is_symbolic_link` 是不跟随链接的 `FileType` 分类结果。
/// `scan_context` identifies the active catalog scan in diagnostics.
/// `scan_context` 在诊断中标识当前目录扫描。
/// `errors` receives the strict link-policy failure.
/// `errors` 接收严格链接策略失败。
///
/// # Returns
/// 返回值
/// `true` when the entry was rejected, otherwise `false`.
/// 目录项被拒绝时返回 `true`，否则返回 `false`。
fn reject_symbolic_link_entry(
    path: &Path,
    is_symbolic_link: bool,
    scan_context: &str,
    errors: &mut Vec<SubagentCatalogLoadError>,
) -> bool {
    if !is_symbolic_link {
        return false;
    }
    errors.push(SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Symbolic-link {scan_context} entry is not supported."),
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Verifies a failed `ReadDir` item retains its owning scan context and directory path.
    /// 验证失败的 `ReadDir` 项会保留所属扫描上下文和目录路径。
    #[test]
    fn directory_entry_errors_are_reported_with_scan_context() {
        // directory identifies the iterator owner included in the catalog load error.
        // directory 标识会写入目录加载错误的迭代器所属目录。
        let directory = PathBuf::from("user-root/packages");
        // errors receives the synthetic item failure through the production helper.
        // errors 通过生产辅助函数接收合成的目录项失败。
        let mut errors = Vec::new();
        // entry is absent because the synthetic permission failure cannot yield a directory entry.
        // entry 为空，因为合成的权限失败无法产出目录项。
        let entry = catalog_directory_entry(
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fixture directory entry denial",
            )),
            &directory,
            "subagent package",
            &mut errors,
        );

        assert!(entry.is_none());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, directory);
        assert_eq!(
            errors[0].message,
            "Failed to read subagent package directory entry: fixture directory entry denial"
        );
    }

    /// Verifies the strict link policy records the exact entry and scan context.
    /// 验证严格链接策略会记录精确目录项和扫描上下文。
    #[test]
    fn symbolic_link_entries_are_rejected_with_exact_path() {
        // link_path identifies the synthetic link entry used by the pure policy boundary.
        // link_path 标识纯策略边界使用的合成链接目录项。
        let link_path = PathBuf::from("user-root/definitions/loop");
        // errors receives the strict policy diagnostic.
        // errors 接收严格策略诊断。
        let mut errors = Vec::new();

        let rejected =
            reject_symbolic_link_entry(&link_path, true, "subagent catalog", &mut errors);

        assert!(rejected);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, link_path);
        assert_eq!(
            errors[0].message,
            "Symbolic-link subagent catalog entry is not supported."
        );
    }
}
