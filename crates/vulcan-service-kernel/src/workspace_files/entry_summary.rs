//! Workspace file entry summary helpers.
//! 工作区文件 entry summary 辅助逻辑。

use super::*;

/// Converts one filesystem entry into a workspace file tree row.
/// 将单个文件系统条目转换为工作区文件树行。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
/// `path` is the entry path returned by the ignore walker.
/// `path` 是 ignore walker 返回的条目路径。
/// `filters` are the active file tree filters.
/// `filters` 是当前生效的文件树过滤器。
///
/// # Returns
/// 返回值
/// A visible summary row, or `None` when the entry should be filtered out.
/// 返回可见摘要行，若条目应被过滤则返回 `None`。
pub(crate) fn workspace_file_entry_summary(
    workspace_root: &Path,
    path: &Path,
    filters: &WorkspaceFileFilterOptions,
) -> Option<WorkspaceFileEntrySummary> {
    let canonical = fs::canonicalize(path).ok()?;
    if !workspace_path_is_inside_root(&canonical, workspace_root) {
        return None;
    }
    let metadata = fs::metadata(&canonical).ok()?;
    let name = canonical.file_name()?.to_string_lossy().to_string();
    let is_dot_hidden = workspace_file_name_is_dot_hidden(&name);
    let is_platform_hidden = workspace_file_is_platform_hidden(&canonical, &metadata);
    if (filters.ignore_dot_names && is_dot_hidden) || (filters.ignore_hidden && is_platform_hidden)
    {
        return None;
    }

    let is_directory = metadata.is_dir();
    let kind = if is_directory {
        WorkspaceFileEntryKind::Directory
    } else {
        workspace_file_kind_from_path(&canonical)
    };
    let visible_kind = match kind {
        WorkspaceFileEntryKind::Directory | WorkspaceFileEntryKind::Code => true,
        WorkspaceFileEntryKind::Image | WorkspaceFileEntryKind::Video => true,
        WorkspaceFileEntryKind::Other => !filters.filter_non_preview_files,
    };
    if !visible_kind {
        return None;
    }
    if is_directory
        && filters.filter_non_preview_files
        && !workspace_directory_has_previewable_descendant(workspace_root, &canonical, filters)
    {
        return None;
    }

    Some(WorkspaceFileEntrySummary {
        name,
        path: normalized_workspace_path_text_from_path(&canonical),
        relative_path: workspace_file_relative_path_text(workspace_root, &canonical),
        extension: workspace_file_extension(&canonical),
        mime_type: workspace_file_mime_type(&canonical, &kind),
        size_bytes: metadata.is_file().then_some(metadata.len()),
        modified_at: workspace_file_modified_at_ms(&metadata),
        hidden: is_dot_hidden || is_platform_hidden,
        has_children: is_directory,
        is_directory,
        kind,
    })
}

/// Returns whether a directory contains a visible code, image, or video descendant.
/// 返回目录是否包含可见的代码、图片或视频后代。
pub(crate) fn workspace_directory_has_previewable_descendant(
    workspace_root: &Path,
    directory: &Path,
    filters: &WorkspaceFileFilterOptions,
) -> bool {
    let mut walker = WalkBuilder::new(directory);
    walker
        .follow_links(false)
        .hidden(false)
        .ignore(filters.use_ignore_files)
        .git_ignore(filters.use_gitignore)
        .git_global(filters.use_gitignore)
        .git_exclude(filters.use_gitignore)
        .parents(filters.use_ignore_files || filters.use_gitignore);

    for walked_entry in walker.build() {
        let Ok(walked_entry) = walked_entry else {
            continue;
        };
        if walked_entry.depth() == 0 {
            continue;
        }
        let path = walked_entry.path();
        let Ok(canonical) = fs::canonicalize(path) else {
            continue;
        };
        if !workspace_path_is_inside_root(&canonical, workspace_root) {
            continue;
        }
        let Ok(metadata) = fs::metadata(&canonical) else {
            continue;
        };
        let Some(name) = canonical
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
        else {
            continue;
        };
        let is_dot_hidden = workspace_file_name_is_dot_hidden(&name);
        let is_platform_hidden = workspace_file_is_platform_hidden(&canonical, &metadata);
        if (filters.ignore_dot_names && is_dot_hidden)
            || (filters.ignore_hidden && is_platform_hidden)
        {
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        if matches!(
            workspace_file_kind_from_path(&canonical),
            WorkspaceFileEntryKind::Code
                | WorkspaceFileEntryKind::Image
                | WorkspaceFileEntryKind::Video
        ) {
            return true;
        }
    }
    false
}

/// Sorts workspace file rows with directories first and names case-insensitively.
/// 以目录优先和大小写不敏感名称顺序排序工作区文件行。
///
/// # Parameters
/// 参数
/// `left` is the left row.
/// `left` 是左侧行。
/// `right` is the right row.
/// `right` 是右侧行。
///
/// # Returns
/// 返回值
/// Ordering used by stable WebUI tree display.
/// 返回稳定 WebUI 树展示使用的排序结果。
pub(crate) fn workspace_file_entry_sort(
    left: &WorkspaceFileEntrySummary,
    right: &WorkspaceFileEntrySummary,
) -> std::cmp::Ordering {
    right
        .is_directory
        .cmp(&left.is_directory)
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        .then_with(|| left.name.cmp(&right.name))
}
