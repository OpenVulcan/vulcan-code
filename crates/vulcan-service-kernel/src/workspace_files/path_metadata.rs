//! Workspace file path metadata helpers.
//! 工作区文件 path metadata 辅助逻辑。

use super::*;

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
pub(crate) fn workspace_file_relative_path_text(workspace_root: &Path, path: &Path) -> String {
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
pub(crate) fn workspace_file_parent_inside_root(
    workspace_root: &Path,
    path: &Path,
) -> Option<PathBuf> {
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
pub(crate) fn workspace_file_extension(path: &Path) -> Option<String> {
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
pub(crate) fn workspace_file_extension_is_code(extension: &str) -> bool {
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
pub(crate) fn workspace_file_extension_is_image(extension: &str) -> bool {
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
pub(crate) fn workspace_file_extension_is_video(extension: &str) -> bool {
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
pub(crate) fn workspace_file_mime_type(
    path: &Path,
    kind: &WorkspaceFileEntryKind,
) -> Option<String> {
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
