//! Workspace file path metadata helpers.
//! 工作区文件 path metadata 辅助逻辑。

use super::*;

/// Metadata shared by extension-based workspace file classification and MIME projection.
/// 工作区文件扩展名分类与 MIME 投影共享的元数据。
#[derive(Debug, Clone)]
pub(crate) struct WorkspaceFileExtensionMetadata {
    /// Entry kind assigned to files with this extension.
    /// 拥有该扩展名的文件应分配的条目类别。
    pub(crate) kind: WorkspaceFileEntryKind,
    /// MIME type emitted for files with this extension.
    /// 拥有该扩展名的文件应输出的 MIME 类型。
    pub(crate) mime_type: &'static str,
}

/// Path-derived workspace file metadata used by listing and preview decisions.
/// 列表与预览决策使用的路径派生工作区文件元数据。
#[derive(Debug, Clone)]
pub(crate) struct WorkspaceFilePathMetadata {
    /// Entry kind assigned to the path.
    /// 路径对应的条目类别。
    pub(crate) kind: WorkspaceFileEntryKind,
    /// Lowercase file extension without the dot.
    /// 不含点号的小写文件扩展名。
    pub(crate) extension: Option<String>,
    /// Static MIME type associated with the classified file.
    /// 与已分类文件关联的静态 MIME 类型。
    pub(crate) mime_type: Option<&'static str>,
}

/// Returns one workspace-relative path using WebUI-stable slash separators.
/// 返回使用 WebUI 稳定斜杠分隔符的工作区相对路径。
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
pub(crate) fn workspace_file_relative_path_text(workspace_root: &Path, path: &Path) -> String {
    let Ok(relative) = path.strip_prefix(workspace_root) else {
        return String::new();
    };
    let mut relative_path = String::new();
    // Build the slash-separated projection without materializing every segment.
    // 直接构建斜杠分隔投影，避免为每个路径段分配中间集合。
    for component in relative.components() {
        let std::path::Component::Normal(value) = component else {
            continue;
        };
        if !relative_path.is_empty() {
            relative_path.push('/');
        }
        relative_path.push_str(value.to_string_lossy().as_ref());
    }
    relative_path
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
pub(crate) fn workspace_file_parent_inside_root(
    workspace_root: &Path,
    path: &Path,
) -> Option<PathBuf> {
    let parent = path.parent()?;
    if workspace_path_is_inside_root(parent, workspace_root) {
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
pub(crate) fn workspace_file_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

/// Returns kind, extension, and MIME facts derived from one workspace file path.
/// 返回从单个工作区文件路径派生出的类别、扩展名和 MIME 事实。
///
/// # Parameters
/// 参数
/// `path` is the file path to classify.
/// `path` 是需要分类的文件路径。
///
/// # Returns
/// 返回值
/// A single metadata bundle used to avoid re-reading extension rules per field.
/// 返回单个元数据集合，避免为每个字段重复读取扩展名规则。
pub(crate) fn workspace_file_path_metadata(path: &Path) -> WorkspaceFilePathMetadata {
    let Some(extension) = workspace_file_extension(path) else {
        let kind = if workspace_file_name_without_extension_is_code(path) {
            WorkspaceFileEntryKind::Code
        } else {
            WorkspaceFileEntryKind::Other
        };
        let mime_type = (kind == WorkspaceFileEntryKind::Code).then_some("text/plain");
        return WorkspaceFilePathMetadata {
            kind,
            extension: None,
            mime_type,
        };
    };
    let Some(metadata) = workspace_file_extension_metadata(&extension) else {
        return WorkspaceFilePathMetadata {
            kind: WorkspaceFileEntryKind::Other,
            extension: Some(extension),
            mime_type: None,
        };
    };
    WorkspaceFilePathMetadata {
        kind: metadata.kind,
        extension: Some(extension),
        mime_type: Some(metadata.mime_type),
    }
}

/// Returns the preview metadata assigned to a lowercase extension.
/// 返回小写扩展名对应的预览元数据。
///
/// # Parameters
/// 参数
/// `extension` is the lowercase extension without the dot.
/// `extension` 是不含点号的小写扩展名。
///
/// # Returns
/// 返回值
/// Shared metadata when the extension can be previewed by the workspace file panel.
/// 当扩展名可由工作区文件面板预览时返回共享元数据。
pub(crate) fn workspace_file_extension_metadata(
    extension: &str,
) -> Option<WorkspaceFileExtensionMetadata> {
    let metadata = match extension {
        "png" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Image,
            mime_type: "image/png",
        },
        "jpg" | "jpeg" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Image,
            mime_type: "image/jpeg",
        },
        "webp" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Image,
            mime_type: "image/webp",
        },
        "gif" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Image,
            mime_type: "image/gif",
        },
        "bmp" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Image,
            mime_type: "image/bmp",
        },
        "svg" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Image,
            mime_type: "image/svg+xml",
        },
        "mp4" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Video,
            mime_type: "video/mp4",
        },
        "webm" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Video,
            mime_type: "video/webm",
        },
        "mov" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Video,
            mime_type: "video/quicktime",
        },
        "mkv" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Video,
            mime_type: "video/x-matroska",
        },
        "html" | "htm" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Code,
            mime_type: "text/html",
        },
        "css" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Code,
            mime_type: "text/css",
        },
        "json" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Code,
            mime_type: "application/json",
        },
        "md" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Code,
            mime_type: "text/markdown",
        },
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "toml" | "yaml" | "yml" | "hbs"
        | "ps1" | "sh" | "bash" | "zsh" | "py" | "lua" | "sql" | "txt" | "lock" | "ini" | "cfg"
        | "conf" | "xml" | "c" | "h" | "cpp" | "hpp" | "go" | "java" | "kt" | "swift" | "rb"
        | "php" => WorkspaceFileExtensionMetadata {
            kind: WorkspaceFileEntryKind::Code,
            mime_type: "text/plain",
        },
        _ => return None,
    };
    Some(metadata)
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
pub(crate) fn workspace_file_name_without_extension_is_code(path: &Path) -> bool {
    let Some(name) = path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    else {
        return false;
    };
    matches!(
        name.as_str(),
        "license" | "readme" | "makefile" | "dockerfile" | "gitignore" | "gitattributes" | "ignore"
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
pub(crate) fn workspace_file_mime_type(
    path: &Path,
    kind: &WorkspaceFileEntryKind,
) -> Option<String> {
    let metadata = workspace_file_path_metadata(path);
    metadata
        .kind
        .eq(kind)
        .then_some(metadata.mime_type)
        .flatten()
        .map(String::from)
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
pub(crate) fn workspace_file_name_is_dot_hidden(name: &str) -> bool {
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
pub(crate) fn workspace_file_is_platform_hidden(_path: &Path, metadata: &fs::Metadata) -> bool {
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
pub(crate) fn workspace_file_is_platform_hidden(_path: &Path, _metadata: &fs::Metadata) -> bool {
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
pub(crate) fn workspace_file_modified_at_ms(metadata: &fs::Metadata) -> Option<String> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().to_string())
}
