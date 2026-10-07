//! Workspace file path security helpers.
//! 工作区文件 path security 辅助逻辑。

use super::*;

/// Resolves and validates one workspace root path.
/// 解析并校验单个工作区根路径。
///
/// # Parameters
/// 参数
/// `workspace_path` is the raw workspace path text received from WebUI.
/// `workspace_path` 是从 WebUI 收到的原始工作区路径文本。
///
/// # Returns
/// 返回值
/// A canonical directory path that can be used as a security boundary.
/// 返回可作为安全边界使用的规范化目录路径。
pub(crate) fn workspace_file_canonical_root(workspace_path: &str) -> Result<PathBuf, String> {
    let trimmed_path = workspace_path.trim();
    if trimmed_path.is_empty() {
        return Err(String::from("工作区路径不能为空。"));
    }
    let root = fs::canonicalize(Path::new(trimmed_path))
        .map_err(|error| format!("解析工作区路径失败 `{trimmed_path}`：{error}"))?;
    if !root.is_dir() {
        return Err(format!(
            "工作区路径不是目录：{}",
            normalized_workspace_path_text_from_path(&root)
        ));
    }
    Ok(root)
}

/// Resolves one user-supplied child path under a canonical workspace root.
/// 在规范化工作区根下解析单个用户提供的子路径。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace security boundary.
/// `workspace_root` 是规范化后的工作区安全边界。
/// `path` is the optional absolute or workspace-relative child path.
/// `path` 是可选的绝对或工作区相对子路径。
/// `label` is a human-readable field label for diagnostics.
/// `label` 是用于诊断的人类可读字段标签。
///
/// # Returns
/// 返回值
/// A canonical child path that is guaranteed to remain inside the root.
/// 返回保证仍位于根目录内的规范化子路径。
pub(crate) fn workspace_file_canonical_child_path(
    workspace_root: &Path,
    path: Option<&str>,
    label: &str,
) -> Result<PathBuf, String> {
    let requested = path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.to_path_buf());
    let joined = if requested.is_absolute() {
        requested
    } else {
        workspace_root.join(requested)
    };
    let canonical = fs::canonicalize(&joined).map_err(|error| {
        format!(
            "解析{label}失败 `{}`：{error}",
            normalized_workspace_path_text_from_path(&joined)
        )
    })?;
    if !workspace_path_is_inside_root(&canonical, workspace_root) {
        return Err(format!(
            "{label}越出工作区：{}",
            normalized_workspace_path_text_from_path(&canonical)
        ));
    }
    Ok(canonical)
}

/// Returns whether one canonical path is inside or equal to a canonical workspace root.
/// 返回单个规范化路径是否位于规范化工作区根内或等于该根。
///
/// # Parameters
/// 参数
/// `path` is the canonical target path.
/// `path` 是规范化目标路径。
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
///
/// # Returns
/// 返回值
/// `true` when the path remains inside the workspace boundary.
/// 当路径仍位于工作区边界内时返回 `true`。
#[cfg(windows)]
pub(crate) fn workspace_path_is_inside_root(path: &Path, workspace_root: &Path) -> bool {
    let path_text = normalized_workspace_path_text_from_path(path).to_ascii_lowercase();
    let root_text = normalized_workspace_path_text_from_path(workspace_root).to_ascii_lowercase();
    // A drive root keeps its trailing separator (`d:\`), so compare against the separator-free stem.
    // 盘符根保留结尾分隔符（`d:\`），因此与去掉分隔符的主干比较。
    let root_stem = root_text.trim_end_matches(['\\', '/']);
    path_text == root_text
        || path_text
            .strip_prefix(root_stem)
            .is_some_and(|suffix| suffix.starts_with('\\') || suffix.starts_with('/'))
}

/// Returns whether one canonical path is inside or equal to a canonical workspace root.
/// 返回单个规范化路径是否位于规范化工作区根内或等于该根。
///
/// # Parameters
/// 参数
/// `path` is the canonical target path.
/// `path` 是规范化目标路径。
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
///
/// # Returns
/// 返回值
/// `true` when the path remains inside the workspace boundary.
/// 当路径仍位于工作区边界内时返回 `true`。
#[cfg(not(windows))]
pub(crate) fn workspace_path_is_inside_root(path: &Path, workspace_root: &Path) -> bool {
    path == workspace_root || path.starts_with(workspace_root)
}
