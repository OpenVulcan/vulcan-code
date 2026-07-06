//! Workspace file entry summary helpers.
//! 工作区文件 entry summary 辅助逻辑。

use super::*;

/// Filesystem metadata for one workspace entry that passed root and hidden filters.
/// 已通过根目录与隐藏规则过滤的单个工作区条目文件系统元数据。
struct WorkspaceVisibleFileEntryMetadata {
    /// Canonical path of the entry inside the workspace root.
    /// 位于工作区根内的条目规范化路径。
    canonical: PathBuf,
    /// Filesystem metadata already loaded for this entry.
    /// 已为该条目加载的文件系统元数据。
    metadata: fs::Metadata,
    /// Display name derived from the canonical path.
    /// 从规范化路径派生出的显示名称。
    name: String,
    /// Whether the entry name uses the Unix dot-hidden convention.
    /// 条目名称是否使用 Unix 点隐藏约定。
    is_dot_hidden: bool,
    /// Whether the entry is hidden by platform metadata.
    /// 条目是否被平台元数据标记为隐藏。
    is_platform_hidden: bool,
}

/// Loads entry metadata only when the path is inside the workspace and passes hidden filters.
/// 仅当路径位于工作区内且通过隐藏过滤时加载条目元数据。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
/// `path` is the candidate filesystem entry.
/// `path` 是候选文件系统条目。
/// `filters` are the active workspace file filters.
/// `filters` 是当前生效的工作区文件过滤器。
///
/// # Returns
/// 返回值
/// Visible entry metadata, or `None` when the entry is invalid or filtered.
/// 返回可见条目元数据；若条目无效或被过滤则返回 `None`。
fn workspace_visible_file_entry_metadata(
    workspace_root: &Path,
    path: &Path,
    filters: &WorkspaceFileFilterOptions,
) -> Option<WorkspaceVisibleFileEntryMetadata> {
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
    Some(WorkspaceVisibleFileEntryMetadata {
        canonical,
        metadata,
        name,
        is_dot_hidden,
        is_platform_hidden,
    })
}

/// Returns whether a file kind can be previewed by the workspace file panel.
/// 返回某个文件类别是否可由工作区文件面板预览。
///
/// # Parameters
/// 参数
/// `kind` is the already classified workspace file kind.
/// `kind` 是已完成分类的工作区文件类别。
///
/// # Returns
/// 返回值
/// `true` when the kind has a code or media preview path.
/// 当该类别具备代码或媒体预览路径时返回 `true`。
fn workspace_file_kind_is_previewable(kind: &WorkspaceFileEntryKind) -> bool {
    matches!(
        kind,
        WorkspaceFileEntryKind::Code
            | WorkspaceFileEntryKind::Image
            | WorkspaceFileEntryKind::Video
    )
}

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
    let entry = workspace_visible_file_entry_metadata(workspace_root, path, filters)?;
    let is_directory = entry.metadata.is_dir();
    let (kind, extension, mime_type) = if is_directory {
        (WorkspaceFileEntryKind::Directory, None, None)
    } else {
        let path_metadata = workspace_file_path_metadata(&entry.canonical);
        (
            path_metadata.kind,
            path_metadata.extension,
            path_metadata.mime_type.map(String::from),
        )
    };
    let visible_kind = match &kind {
        WorkspaceFileEntryKind::Directory => true,
        WorkspaceFileEntryKind::Other => !filters.filter_non_preview_files,
        _ => workspace_file_kind_is_previewable(&kind),
    };
    if !visible_kind {
        return None;
    }
    if is_directory
        && filters.filter_non_preview_files
        && !workspace_directory_has_previewable_descendant(
            workspace_root,
            &entry.canonical,
            filters,
        )
    {
        return None;
    }

    Some(WorkspaceFileEntrySummary {
        name: entry.name,
        path: normalized_workspace_path_text_from_path(&entry.canonical),
        relative_path: workspace_file_relative_path_text(workspace_root, &entry.canonical),
        extension,
        mime_type,
        size_bytes: entry.metadata.is_file().then_some(entry.metadata.len()),
        modified_at: workspace_file_modified_at_ms(&entry.metadata),
        hidden: entry.is_dot_hidden || entry.is_platform_hidden,
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
    let walker = workspace_file_walk_builder(directory, filters, None);

    for walked_entry in walker.build() {
        let Ok(walked_entry) = walked_entry else {
            continue;
        };
        if walked_entry.depth() == 0 {
            continue;
        }
        let Some(entry) =
            workspace_visible_file_entry_metadata(workspace_root, walked_entry.path(), filters)
        else {
            continue;
        };
        if !entry.metadata.is_file() {
            continue;
        }
        if workspace_file_kind_is_previewable(&workspace_file_kind_from_path(&entry.canonical)) {
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
