//! Workspace file browsing and preview services for the WebUI.
//! WebUI 使用的工作区文件浏览与预览服务。

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use encoding_rs::GB18030;
use ignore::WalkBuilder;
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
        "目录路径",
    )?;
    if !directory.is_dir() {
        return Err(format!(
            "文件树路径不是目录：{}",
            normalized_workspace_path_text_from_path(&directory)
        ));
    }

    let mut walker = WalkBuilder::new(&directory);
    walker
        .max_depth(Some(1))
        .follow_links(false)
        .hidden(false)
        .ignore(filters.use_ignore_files)
        .git_ignore(filters.use_gitignore)
        .git_global(filters.use_gitignore)
        .git_exclude(filters.use_gitignore)
        .parents(filters.use_ignore_files || filters.use_gitignore);

    let mut entries = Vec::new();
    let mut truncated = false;
    let mut notice = None;
    for walked_entry in walker.build() {
        let walked_entry = match walked_entry {
            Ok(entry) => entry,
            Err(error) => {
                notice.get_or_insert_with(|| format!("部分文件无法读取：{error}"));
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
                "当前目录超过 {WORKSPACE_FILE_DIRECTORY_ENTRY_LIMIT} 个可显示条目，结果已截断。"
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
    let file = workspace_file_canonical_child_path(&workspace_root, Some(&file_path), "文件路径")?;
    let metadata = fs::metadata(&file).map_err(|error| {
        format!(
            "读取文件元数据失败 `{}`：{error}",
            normalized_workspace_path_text_from_path(&file)
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "文件树路径不是文件：{}",
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
        WorkspaceFileEntryKind::Image if filters.allow_images => Ok(workspace_file_media_response(
            &workspace_root,
            &file,
            &metadata,
            WorkspaceFileEntryKind::Image,
        )),
        WorkspaceFileEntryKind::Video if filters.allow_videos => Ok(workspace_file_media_response(
            &workspace_root,
            &file,
            &metadata,
            WorkspaceFileEntryKind::Video,
        )),
        WorkspaceFileEntryKind::Image | WorkspaceFileEntryKind::Video => {
            Err(String::from("当前过滤规则未允许预览该媒体文件。"))
        }
        WorkspaceFileEntryKind::Directory | WorkspaceFileEntryKind::Other => {
            Err(String::from("当前文件类型不支持预览。"))
        }
    }
}

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
fn workspace_file_canonical_root(workspace_path: &str) -> Result<PathBuf, String> {
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
fn workspace_file_canonical_child_path(
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
fn workspace_path_is_inside_root(path: &Path, workspace_root: &Path) -> bool {
    let path_text = normalized_workspace_path_text_from_path(path).to_ascii_lowercase();
    let root_text = normalized_workspace_path_text_from_path(workspace_root).to_ascii_lowercase();
    path_text == root_text
        || path_text
            .strip_prefix(&root_text)
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
fn workspace_path_is_inside_root(path: &Path, workspace_root: &Path) -> bool {
    path == workspace_root || path.starts_with(workspace_root)
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
fn workspace_file_entry_summary(
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
fn workspace_directory_has_previewable_descendant(
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
fn workspace_file_entry_sort(
    left: &WorkspaceFileEntrySummary,
    right: &WorkspaceFileEntrySummary,
) -> std::cmp::Ordering {
    right
        .is_directory
        .cmp(&left.is_directory)
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        .then_with(|| left.name.cmp(&right.name))
}

/// Classifies one workspace file path for preview eligibility.
/// 对单个工作区文件路径进行预览资格分类。
///
/// # Parameters
/// 参数
/// `path` is the file path to classify.
/// `path` 是需要分类的文件路径。
///
/// # Returns
/// 返回值
/// The file entry kind used by the backend and WebUI.
/// 返回后端与 WebUI 使用的文件条目类别。
fn workspace_file_kind_from_path(path: &Path) -> WorkspaceFileEntryKind {
    let Some(extension) = workspace_file_extension(path) else {
        return if workspace_file_name_without_extension_is_code(path) {
            WorkspaceFileEntryKind::Code
        } else {
            WorkspaceFileEntryKind::Other
        };
    };
    if workspace_file_extension_is_image(&extension) {
        return WorkspaceFileEntryKind::Image;
    }
    if workspace_file_extension_is_video(&extension) {
        return WorkspaceFileEntryKind::Video;
    }
    if workspace_file_extension_is_code(&extension) {
        return WorkspaceFileEntryKind::Code;
    }
    WorkspaceFileEntryKind::Other
}

/// Reads one code file with a bounded UTF-8 preview limit.
/// 使用有界 UTF-8 预览限制读取单个代码文件。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
/// `file` is the canonical file path to read.
/// `file` 是需要读取的规范化文件路径。
/// `metadata` is the already loaded file metadata.
/// `metadata` 是已加载的文件元数据。
/// `kind` is the known code file kind.
/// `kind` 是已知代码文件类别。
///
/// # Returns
/// 返回值
/// A bounded code preview response.
/// 返回有界代码预览响应。
fn workspace_file_read_code_response(
    workspace_root: &Path,
    file: &Path,
    metadata: &fs::Metadata,
    kind: WorkspaceFileEntryKind,
) -> Result<WorkspaceFileReadResponse, String> {
    let mut handle = fs::File::open(file).map_err(|error| {
        format!(
            "打开文件失败 `{}`：{error}",
            normalized_workspace_path_text_from_path(file)
        )
    })?;
    let mut bytes =
        Vec::with_capacity(WORKSPACE_FILE_READ_LIMIT_BYTES.min(metadata.len() as usize));
    Read::by_ref(&mut handle)
        .take((WORKSPACE_FILE_READ_LIMIT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            format!(
                "读取文件失败 `{}`：{error}",
                normalized_workspace_path_text_from_path(file)
            )
        })?;
    let truncated = bytes.len() > WORKSPACE_FILE_READ_LIMIT_BYTES;
    if truncated {
        bytes.truncate(WORKSPACE_FILE_READ_LIMIT_BYTES);
    }
    if bytes.contains(&0) {
        return Err(String::from("该文件包含二进制内容，无法作为代码预览。"));
    }
    let decoded = workspace_file_decode_text_preview(&bytes);
    let line_count = decoded.content.lines().count();
    Ok(WorkspaceFileReadResponse {
        workspace_path: normalized_workspace_path_text_from_path(workspace_root),
        path: normalized_workspace_path_text_from_path(file),
        relative_path: workspace_file_relative_path_text(workspace_root, file),
        kind,
        mime_type: workspace_file_mime_type(file, &WorkspaceFileEntryKind::Code),
        size_bytes: metadata.len(),
        content: Some(decoded.content),
        encoding: Some(decoded.encoding),
        line_count: Some(line_count),
        truncated,
    })
}

/// Stores decoded text and the encoding label used for a workspace file preview.
/// 保存工作区文件预览使用的解码文本与编码标签。
struct WorkspaceFileDecodedText {
    /// Decoded text content ready for WebUI rendering.
    /// 可供 WebUI 渲染的解码文本内容。
    content: String,
    /// Stable encoding label reported to the WebUI.
    /// 返回给 WebUI 的稳定编码标签。
    encoding: String,
}

/// Decodes workspace text preview bytes with UTF-8 first and GB18030 fallback.
/// 优先使用 UTF-8 解码工作区文本预览字节，并回退到 GB18030。
fn workspace_file_decode_text_preview(bytes: &[u8]) -> WorkspaceFileDecodedText {
    match std::str::from_utf8(bytes) {
        Ok(content) => {
            return WorkspaceFileDecodedText {
                content: content.to_string(),
                encoding: String::from("utf-8"),
            };
        }
        Err(error) if error.error_len().is_none() && error.valid_up_to() > 0 => {
            return WorkspaceFileDecodedText {
                content: String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned(),
                encoding: String::from("utf-8"),
            };
        }
        Err(_) => {}
    }
    let (content, _, _) = GB18030.decode(bytes);
    WorkspaceFileDecodedText {
        content: content.into_owned(),
        encoding: String::from("gb18030"),
    }
}

/// Builds one media metadata response without reading the file bytes.
/// 构建单个不读取文件字节的媒体元数据响应。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
/// `file` is the canonical media file path.
/// `file` 是规范化媒体文件路径。
/// `metadata` is the already loaded file metadata.
/// `metadata` 是已加载的文件元数据。
/// `kind` is the media kind.
/// `kind` 是媒体类别。
///
/// # Returns
/// 返回值
/// A media metadata response for frontend URL-based rendering.
/// 返回供前端基于 URL 渲染的媒体元数据响应。
fn workspace_file_media_response(
    workspace_root: &Path,
    file: &Path,
    metadata: &fs::Metadata,
    kind: WorkspaceFileEntryKind,
) -> WorkspaceFileReadResponse {
    WorkspaceFileReadResponse {
        workspace_path: normalized_workspace_path_text_from_path(workspace_root),
        path: normalized_workspace_path_text_from_path(file),
        relative_path: workspace_file_relative_path_text(workspace_root, file),
        mime_type: workspace_file_mime_type(file, &kind),
        size_bytes: metadata.len(),
        content: None,
        encoding: None,
        line_count: None,
        truncated: false,
        kind,
    }
}

/// Returns a workspace-relative path using forward slashes.
/// 返回使用正斜杠的工作区相对路径。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
/// `path` is the canonical child path.
/// `path` 是规范化子路径。
///
/// # Returns
/// 返回值
/// A slash-separated relative path, or an empty string for the root itself.
/// 返回斜杠分隔的相对路径，根目录自身返回空字符串。
fn workspace_file_relative_path_text(workspace_root: &Path, path: &Path) -> String {
    path.strip_prefix(workspace_root)
        .ok()
        .map(|relative| {
            relative
                .components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(value) => {
                        Some(value.to_string_lossy().to_string())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

/// Returns a parent directory only when it remains inside the workspace root.
/// 仅当父目录仍位于工作区根内时返回该父目录。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化工作区根。
/// `path` is the canonical current directory path.
/// `path` 是规范化当前目录路径。
///
/// # Returns
/// 返回值
/// The parent path when it is inside the workspace boundary.
/// 当父路径位于工作区边界内时返回父路径。
fn workspace_file_parent_inside_root(workspace_root: &Path, path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    if parent == workspace_root || parent.starts_with(workspace_root) {
        Some(parent.to_path_buf())
    } else {
        None
    }
}

/// Returns the lowercase extension for one path.
/// 返回单个路径的小写扩展名。
///
/// # Parameters
/// 参数
/// `path` is the path whose extension should be read.
/// `path` 是需要读取扩展名的路径。
///
/// # Returns
/// 返回值
/// The lowercase extension without the dot.
/// 返回不含点号的小写扩展名。
fn workspace_file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

/// Returns whether an extension is treated as a code/text preview file.
/// 返回某个扩展名是否按代码或文本预览文件处理。
///
/// # Parameters
/// 参数
/// `extension` is the lowercase extension without the dot.
/// `extension` 是不含点号的小写扩展名。
///
/// # Returns
/// 返回值
/// `true` when the extension belongs to the default code allowlist.
/// 当扩展名属于默认代码白名单时返回 `true`。
fn workspace_file_extension_is_code(extension: &str) -> bool {
    matches!(
        extension,
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "json"
            | "toml"
            | "yaml"
            | "yml"
            | "md"
            | "hbs"
            | "ps1"
            | "sh"
            | "bash"
            | "zsh"
            | "py"
            | "lua"
            | "css"
            | "html"
            | "htm"
            | "sql"
            | "txt"
            | "lock"
            | "ini"
            | "cfg"
            | "conf"
            | "xml"
            | "svg"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "go"
            | "java"
            | "kt"
            | "swift"
            | "rb"
            | "php"
    )
}

/// Returns whether an extension is treated as an image file.
/// 返回某个扩展名是否按图片文件处理。
///
/// # Parameters
/// 参数
/// `extension` is the lowercase extension without the dot.
/// `extension` 是不含点号的小写扩展名。
///
/// # Returns
/// 返回值
/// `true` when the extension is displayable through the image preview path.
/// 当扩展名可通过图片预览路径显示时返回 `true`。
fn workspace_file_extension_is_image(extension: &str) -> bool {
    matches!(
        extension,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg"
    )
}

/// Returns whether an extension is treated as a video file.
/// 返回某个扩展名是否按视频文件处理。
///
/// # Parameters
/// 参数
/// `extension` is the lowercase extension without the dot.
/// `extension` 是不含点号的小写扩展名。
///
/// # Returns
/// 返回值
/// `true` when the extension is displayable through the video preview path.
/// 当扩展名可通过视频预览路径显示时返回 `true`。
fn workspace_file_extension_is_video(extension: &str) -> bool {
    matches!(extension, "mp4" | "webm" | "mov" | "mkv")
}

/// Returns whether a no-extension file name should still be treated as code.
/// 返回无扩展文件名是否仍应按代码处理。
///
/// # Parameters
/// 参数
/// `path` is the no-extension path to inspect.
/// `path` 是需要检查的无扩展路径。
///
/// # Returns
/// 返回值
/// `true` when the file name is a common text/code file name.
/// 当文件名是常见文本或代码文件名时返回 `true`。
fn workspace_file_name_without_extension_is_code(path: &Path) -> bool {
    let Some(name) = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    else {
        return false;
    };
    matches!(
        name.as_str(),
        "license"
            | "readme"
            | "makefile"
            | "dockerfile"
            | "gitignore"
            | "gitattributes"
            | "ignore"
            | "agents.md"
    )
}

/// Returns a best-effort MIME type for a workspace file.
/// 返回工作区文件的尽力推断 MIME 类型。
///
/// # Parameters
/// 参数
/// `path` is the file path whose MIME type should be inferred.
/// `path` 是需要推断 MIME 类型的文件路径。
/// `kind` is the already classified file kind.
/// `kind` 是已完成分类的文件类别。
///
/// # Returns
/// 返回值
/// A MIME type string when the kind has a known mapping.
/// 当类别存在已知映射时返回 MIME 类型字符串。
fn workspace_file_mime_type(path: &Path, kind: &WorkspaceFileEntryKind) -> Option<String> {
    let extension = workspace_file_extension(path)?;
    let mime = match kind {
        WorkspaceFileEntryKind::Code => match extension.as_str() {
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "json" => "application/json",
            "md" => "text/markdown",
            "svg" => "image/svg+xml",
            _ => "text/plain",
        },
        WorkspaceFileEntryKind::Image => match extension.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "svg" => "image/svg+xml",
            _ => "image/png",
        },
        WorkspaceFileEntryKind::Video => match extension.as_str() {
            "webm" => "video/webm",
            "mov" => "video/quicktime",
            "mkv" => "video/x-matroska",
            _ => "video/mp4",
        },
        WorkspaceFileEntryKind::Directory | WorkspaceFileEntryKind::Other => return None,
    };
    Some(String::from(mime))
}

/// Returns whether one file or directory name uses the Unix dot-hidden convention.
/// 返回单个文件或目录名称是否使用 Unix 点隐藏约定。
///
/// # Parameters
/// 参数
/// `name` is the display file name.
/// `name` 是显示文件名。
///
/// # Returns
/// 返回值
/// `true` when the name starts with a dot and is not `.` or `..`.
/// 当名称以点开头且不是 `.` 或 `..` 时返回 `true`。
fn workspace_file_name_is_dot_hidden(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

/// Returns whether one path is platform-hidden.
/// 返回单个路径是否为平台隐藏项。
///
/// # Parameters
/// 参数
/// `_path` is the canonical path being inspected.
/// `_path` 是正在检查的规范化路径。
/// `metadata` is the filesystem metadata for the path.
/// `metadata` 是该路径的文件系统元数据。
///
/// # Returns
/// 返回值
/// `true` when the platform marks the file hidden.
/// 当平台将文件标记为隐藏时返回 `true`。
#[cfg(windows)]
fn workspace_file_is_platform_hidden(_path: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

/// Returns whether one path is platform-hidden.
/// 返回单个路径是否为平台隐藏项。
///
/// # Parameters
/// 参数
/// `_path` is the canonical path being inspected.
/// `_path` 是正在检查的规范化路径。
/// `_metadata` is the filesystem metadata for the path.
/// `_metadata` 是该路径的文件系统元数据。
///
/// # Returns
/// 返回值
/// `true` when the platform marks the file hidden.
/// 当平台将文件标记为隐藏时返回 `true`。
#[cfg(not(windows))]
fn workspace_file_is_platform_hidden(_path: &Path, _metadata: &fs::Metadata) -> bool {
    false
}

/// Converts file modified metadata into a Unix millisecond string.
/// 将文件修改时间元数据转换为 Unix 毫秒字符串。
///
/// # Parameters
/// 参数
/// `metadata` is the filesystem metadata to inspect.
/// `metadata` 是需要检查的文件系统元数据。
///
/// # Returns
/// 返回值
/// The millisecond timestamp when the platform provides one.
/// 当平台提供时间时返回毫秒时间戳。
fn workspace_file_modified_at_ms(metadata: &fs::Metadata) -> Option<String> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Verifies workspace file path resolution rejects paths outside the root.
    /// 验证工作区文件路径解析会拒绝根目录外路径。
    #[test]
    fn workspace_file_child_path_rejects_outside_root() {
        let root = test_workspace_file_temp_dir("root");
        let outside = test_workspace_file_temp_dir("outside");
        let error = workspace_file_canonical_child_path(&root, outside.to_str(), "文件路径")
            .expect_err("outside path should be rejected");

        assert!(error.contains("越出工作区"));
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    /// Verifies dot-name filtering can hide and reveal dot-prefixed code files.
    /// 验证点名称过滤可以隐藏和显示点开头代码文件。
    #[test]
    fn workspace_file_entry_summary_filters_dot_names() {
        let root = test_workspace_file_temp_dir("dot-filter");
        let hidden_file = root.join(".hidden.rs");
        std::fs::write(&hidden_file, "fn main() {}\n").expect("write hidden file");

        assert!(
            workspace_file_entry_summary(
                &root,
                &hidden_file,
                &WorkspaceFileFilterOptions::default()
            )
            .is_none()
        );
        let mut filters = WorkspaceFileFilterOptions::default();
        filters.ignore_dot_names = false;
        let summary = workspace_file_entry_summary(&root, &hidden_file, &filters)
            .expect("dot file should be visible after disabling dot filter");

        assert_eq!(summary.kind, WorkspaceFileEntryKind::Code);
        assert!(summary.hidden);
        let _ = std::fs::remove_dir_all(root);
    }

    /// Verifies workspace file kind classification marks raw files as other files.
    /// 验证工作区文件类别识别会将 raw 文件标记为其他文件。
    #[test]
    fn workspace_file_kind_classifies_code_media_and_other_files() {
        assert_eq!(
            workspace_file_kind_from_path(Path::new("src/lib.rs")),
            WorkspaceFileEntryKind::Code
        );
        assert_eq!(
            workspace_file_kind_from_path(Path::new("preview.png")),
            WorkspaceFileEntryKind::Image
        );
        assert_eq!(
            workspace_file_kind_from_path(Path::new("clip.mp4")),
            WorkspaceFileEntryKind::Video
        );
        assert_eq!(
            workspace_file_kind_from_path(Path::new("capture.raw")),
            WorkspaceFileEntryKind::Other
        );
    }

    /// Verifies non-preview files are visible unless the dedicated filter is enabled.
    /// 验证非预览文件默认可见，仅在专用过滤器启用后隐藏。
    #[test]
    fn workspace_file_entry_summary_filters_non_preview_files_only_when_requested() {
        let root = test_workspace_file_temp_dir("non-preview-filter");
        let data_directory = root.join("DATA").join("yycfg");
        std::fs::create_dir_all(&data_directory).expect("create data directory");
        let spreadsheet = data_directory.join("中文配置.xls");
        std::fs::write(&spreadsheet, "spreadsheet placeholder").expect("write spreadsheet");

        let default_file_summary = workspace_file_entry_summary(
            &root,
            &spreadsheet,
            &WorkspaceFileFilterOptions::default(),
        )
        .expect("non-preview file should be visible by default");
        let default_directory_summary = workspace_file_entry_summary(
            &root,
            &root.join("DATA"),
            &WorkspaceFileFilterOptions::default(),
        )
        .expect("directory containing non-preview files should be visible by default");
        let mut filters = WorkspaceFileFilterOptions::default();
        filters.filter_non_preview_files = true;

        assert_eq!(default_file_summary.kind, WorkspaceFileEntryKind::Other);
        assert_eq!(
            default_directory_summary.kind,
            WorkspaceFileEntryKind::Directory
        );
        assert!(workspace_file_entry_summary(&root, &spreadsheet, &filters).is_none());
        assert!(workspace_file_entry_summary(&root, &root.join("DATA"), &filters).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Verifies large code previews are truncated by the backend read limit.
    /// 验证大代码预览会被后端读取上限截断。
    #[test]
    fn workspace_file_read_code_response_truncates_large_files() {
        let root = test_workspace_file_temp_dir("read-limit");
        let file = root.join("large.rs");
        let content = "a".repeat(WORKSPACE_FILE_READ_LIMIT_BYTES + 128);
        std::fs::write(&file, content).expect("write large file");
        let metadata = std::fs::metadata(&file).expect("large file metadata");

        let response = workspace_file_read_code_response(
            &root,
            &file,
            &metadata,
            WorkspaceFileEntryKind::Code,
        )
        .expect("large code preview should read");

        assert!(response.truncated);
        assert_eq!(
            response.content.as_deref().map(str::len),
            Some(WORKSPACE_FILE_READ_LIMIT_BYTES)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Verifies non-UTF-8 text files can be previewed through the GB18030 fallback.
    /// 验证非 UTF-8 文本文件可以通过 GB18030 回退编码预览。
    #[test]
    fn workspace_file_read_code_response_decodes_gb18030_text() {
        let root = test_workspace_file_temp_dir("gb18030-read");
        let file = root.join("Nations.ini");
        let source = "[国家]\r\n名称=盟重\r\n";
        let (encoded, _, had_errors) = GB18030.encode(source);
        assert!(!had_errors);
        std::fs::write(&file, encoded.as_ref()).expect("write gb18030 ini");
        let metadata = std::fs::metadata(&file).expect("gb18030 file metadata");

        let response = workspace_file_read_code_response(
            &root,
            &file,
            &metadata,
            WorkspaceFileEntryKind::Code,
        )
        .expect("gb18030 code preview should decode");

        assert_eq!(response.encoding.as_deref(), Some("gb18030"));
        assert_eq!(response.content.as_deref(), Some(source));
        let _ = std::fs::remove_dir_all(root);
    }

    /// Verifies truncated UTF-8 previews keep valid text instead of falling back to GB18030.
    /// 验证被截断的 UTF-8 预览会保留有效文本，而不是回退到 GB18030。
    #[test]
    fn workspace_file_decode_text_preview_keeps_truncated_utf8_prefix() {
        let mut bytes = "配置😀".as_bytes().to_vec();
        bytes.pop();

        let decoded = workspace_file_decode_text_preview(&bytes);

        assert_eq!(decoded.encoding, "utf-8");
        assert_eq!(decoded.content, "配置");
    }

    /// Creates one unique canonical temporary directory for workspace file tests.
    /// 为工作区文件测试创建单个唯一规范化临时目录。
    fn test_workspace_file_temp_dir(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "vulcan-workspace-files-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&directory).expect("create workspace file temp dir");
        std::fs::canonicalize(directory).expect("canonical workspace file temp dir")
    }
}
