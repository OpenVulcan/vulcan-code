//! Workspace file browsing and preview services for the WebUI.
//! WebUI 使用的工作区文件浏览与预览服务。

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use encoding_rs::GB18030;
use ignore::WalkBuilder;
use vulcan_host_tools::output_decode::decode_complete_user_text;
use vulcan_service_api::{
    WorkspaceFileDirectoryListing, WorkspaceFileEntryKind, WorkspaceFileEntrySummary,
    WorkspaceFileFilterOptions, WorkspaceFileReadResponse,
};

use crate::normalized_workspace_path_text_from_path;

// WORKSPACE_FILE_DIRECTORY_ENTRY_LIMIT caps one lazy directory listing response.
// WORKSPACE_FILE_DIRECTORY_ENTRY_LIMIT 限制单次懒加载目录列表响应大小。
const WORKSPACE_FILE_DIRECTORY_ENTRY_LIMIT: usize = 1_000;

// WORKSPACE_FILE_READ_LIMIT_BYTES caps one code preview response payload.
// WORKSPACE_FILE_READ_LIMIT_BYTES 限制单次代码预览响应载荷大小。
const WORKSPACE_FILE_READ_LIMIT_BYTES: usize = 1_048_576;

mod entry_summary;
mod file_kind;
mod path_metadata;
mod path_security;
mod read_response;

use entry_summary::*;
use file_kind::*;
use path_metadata::*;
pub(crate) use path_security::*;
use read_response::*;

/// Builds a workspace file walker with the shared ignore and symlink policy.
/// 使用共享忽略规则和符号链接策略构建工作区文件遍历器。
///
/// # Parameters
/// 参数
/// `directory` is the directory that the walker should scan.
/// `directory` 是遍历器需要扫描的目录。
/// `filters` are the active workspace file filters.
/// `filters` 是当前生效的工作区文件过滤器。
/// `max_depth` optionally limits recursive traversal depth.
/// `max_depth` 可选限制递归遍历深度。
///
/// # Returns
/// 返回值
/// A configured walker builder shared by listing and descendant-preview scans.
/// 返回由列表和后代预览扫描共享的已配置遍历器构建器。
pub(crate) fn workspace_file_walk_builder(
    directory: &Path,
    filters: &WorkspaceFileFilterOptions,
    max_depth: Option<usize>,
) -> WalkBuilder {
    let mut walker = WalkBuilder::new(directory);
    walker
        .max_depth(max_depth)
        .follow_links(false)
        .hidden(false)
        .ignore(filters.use_ignore_files)
        .git_ignore(filters.use_gitignore)
        .git_global(filters.use_gitignore)
        .git_exclude(filters.use_gitignore)
        .parents(filters.use_ignore_files || filters.use_gitignore);
    walker
}

/// Returns whether the active filters allow previewing one media file kind.
/// 返回当前过滤规则是否允许预览单个媒体文件类别。
///
/// # Parameters
/// 参数
/// `kind` is the already classified workspace file kind.
/// `kind` 是已完成分类的工作区文件类别。
/// `filters` are the active workspace file filters.
/// `filters` 是当前生效的工作区文件过滤器。
///
/// # Returns
/// 返回值
/// `true` when the media kind is enabled by the current filters.
/// 当当前过滤器启用该媒体类别时返回 `true`。
fn workspace_file_media_kind_allowed(
    kind: &WorkspaceFileEntryKind,
    filters: &WorkspaceFileFilterOptions,
) -> bool {
    match kind {
        WorkspaceFileEntryKind::Image => filters.allow_images,
        WorkspaceFileEntryKind::Video => filters.allow_videos,
        WorkspaceFileEntryKind::Code
        | WorkspaceFileEntryKind::Directory
        | WorkspaceFileEntryKind::Other => false,
    }
}

/// Lists one workspace directory for the right-side file context panel.
/// 为右侧文件上下文面板列出单个工作区目录。
///
/// # Parameters
/// 参数
/// `workspace_path` is the project root path supplied by the selected conversation.
/// `workspace_path` 是当前选中会话提供的项目根路径。
/// `_project_id` is currently accepted for protocol symmetry with other workspace RPCs.
/// `_project_id` 当前仅用于与其他工作区 RPC 保持协议对称。
/// `_conversation_id` is currently accepted for protocol symmetry with other workspace RPCs.
/// `_conversation_id` 当前仅用于与其他工作区 RPC 保持协议对称。
/// `directory_path` optionally selects a child directory to list.
/// `directory_path` 可选指定需要列出的子目录。
/// `filters` optionally overrides the default file tree filters.
/// `filters` 可选覆盖默认文件树过滤选项。
///
/// # Returns
/// 返回值
/// A bounded, filtered directory listing for WebUI rendering.
/// 返回供 WebUI 渲染的有界过滤目录列表。
pub(super) async fn service_workspace_files_list(
    workspace_path: String,
    _project_id: Option<String>,
    _conversation_id: Option<String>,
    directory_path: Option<String>,
    filters: Option<WorkspaceFileFilterOptions>,
) -> Result<WorkspaceFileDirectoryListing, String> {
    let filters = filters.unwrap_or_default();
    let workspace_root = workspace_file_canonical_root(&workspace_path)?;
    let directory = workspace_file_canonical_child_path(
        &workspace_root,
        directory_path.as_deref(),
        "directory path",
    )?;
    if !directory.is_dir() {
        return Err(format!(
            "The file tree path is not a directory: {}",
            normalized_workspace_path_text_from_path(&directory)
        ));
    }

    let walker = workspace_file_walk_builder(&directory, &filters, Some(1));

    let mut entries = Vec::new();
    let mut truncated = false;
    let mut notice = None;
    for walked_entry in walker.build() {
        let walked_entry = match walked_entry {
            Ok(entry) => entry,
            Err(error) => {
                notice.get_or_insert_with(|| format!("Some files cannot be read: {error}"));
                continue;
            }
        };
        if walked_entry.depth() == 0 {
            continue;
        }
        let Some(entry) =
            workspace_file_entry_summary(&workspace_root, walked_entry.path(), &filters)
        else {
            continue;
        };
        if entries.len() >= WORKSPACE_FILE_DIRECTORY_ENTRY_LIMIT {
            truncated = true;
            break;
        }
        entries.push(entry);
    }

    entries.sort_by(workspace_file_entry_sort);
    if truncated {
        notice.get_or_insert_with(|| {
            format!(
                "The current directory exceeds {WORKSPACE_FILE_DIRECTORY_ENTRY_LIMIT} displayable entries; the result was truncated."
            )
        });
    }
    Ok(WorkspaceFileDirectoryListing {
        workspace_path: normalized_workspace_path_text_from_path(&workspace_root),
        path: normalized_workspace_path_text_from_path(&directory),
        relative_path: workspace_file_relative_path_text(&workspace_root, &directory),
        parent_path: workspace_file_parent_inside_root(&workspace_root, &directory)
            .map(|path| normalized_workspace_path_text_from_path(&path)),
        entry_count: entries.len(),
        entries,
        filters,
        truncated,
        notice,
    })
}

/// Reads one workspace file preview for the right-side file context panel.
/// 为右侧文件上下文面板读取单个工作区文件预览。
///
/// # Parameters
/// 参数
/// `workspace_path` is the project root path supplied by the selected conversation.
/// `workspace_path` 是当前选中会话提供的项目根路径。
/// `_project_id` is currently accepted for protocol symmetry with other workspace RPCs.
/// `_project_id` 当前仅用于与其他工作区 RPC 保持协议对称。
/// `_conversation_id` is currently accepted for protocol symmetry with other workspace RPCs.
/// `_conversation_id` 当前仅用于与其他工作区 RPC 保持协议对称。
/// `file_path` is the absolute or workspace-relative file path to preview.
/// `file_path` 是需要预览的绝对或工作区相对文件路径。
/// `filters` optionally carries media allow switches from the WebUI.
/// `filters` 可选携带 WebUI 的媒体允许开关。
///
/// # Returns
/// 返回值
/// A code content response or media metadata response.
/// 返回代码内容响应或媒体元数据响应。
pub(super) async fn service_workspace_files_read(
    workspace_path: String,
    _project_id: Option<String>,
    _conversation_id: Option<String>,
    file_path: String,
    filters: Option<WorkspaceFileFilterOptions>,
) -> Result<WorkspaceFileReadResponse, String> {
    let filters = filters.unwrap_or_default();
    let workspace_root = workspace_file_canonical_root(&workspace_path)?;
    let file = workspace_file_canonical_child_path(&workspace_root, Some(&file_path), "file path")?;
    let metadata = fs::metadata(&file).map_err(|error| {
        format!(
            "Failed to read file metadata `{}`: {error}",
            normalized_workspace_path_text_from_path(&file)
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "The file tree path is not a file: {}",
            normalized_workspace_path_text_from_path(&file)
        ));
    }

    let kind = workspace_file_kind_from_path(&file);
    match kind {
        WorkspaceFileEntryKind::Code => workspace_file_read_code_response(
            &workspace_root,
            &file,
            &metadata,
            WorkspaceFileEntryKind::Code,
        ),
        WorkspaceFileEntryKind::Image | WorkspaceFileEntryKind::Video => {
            if workspace_file_media_kind_allowed(&kind, &filters) {
                Ok(workspace_file_media_response(
                    &workspace_root,
                    &file,
                    &metadata,
                    kind,
                ))
            } else {
                Err(String::from(
                    "The current filter rules do not allow previewing this media file.",
                ))
            }
        }
        WorkspaceFileEntryKind::Directory | WorkspaceFileEntryKind::Other => {
            Err(String::from("Previewing this file type is not supported."))
        }
    }
}

#[cfg(test)]
mod tests;
