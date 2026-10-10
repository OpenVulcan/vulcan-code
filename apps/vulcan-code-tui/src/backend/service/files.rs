// Local disk listing for the attachment picker.
// 附件选择器的本地磁盘列表。
//
// The service runs on this machine and attachments are staged from bytes the TUI reads itself, so
// the picker lists the real local filesystem instead of asking the service.
// 服务运行在本机，附件由 TUI 自行读取字节后暂存，因此选择器直接列出真实本地文件系统而不询问服务。

use std::fs;
use std::path::PathBuf;

use crate::model::{DirEntry, DirectoryListing};

/// Lists `path` (home directory when `None`); directories first, then files, each by name.
/// 列出 `path`（为 `None` 时列出主目录）；目录在前、文件在后，各自按名称排序。
/// Returns `DirectoryListing`.
/// 返回 `DirectoryListing`。
pub fn list(path: Option<&str>, show_hidden: bool, files: bool) -> DirectoryListing {
    // Exact path.
    // 准确路径。
    let path = path
        .map(PathBuf::from)
        .or_else(home)
        .unwrap_or_else(|| PathBuf::from("/"));
    // Display.
    // 路径显示文本。
    let display = path.to_string_lossy().into_owned();
    // Listing.
    // 目录列表结果。
    let mut listing = DirectoryListing {
        path: display,
        parent: path.parent().map(|p| p.to_string_lossy().into_owned()),
        roots: roots(),
        entries: Vec::new(),
        error: None,
    };
    // Reader.
    // 文件系统目录读取器。
    let reader = match fs::read_dir(&path) {
        Ok(reader) => reader,
        Err(error) => {
            listing.error = Some(format!("{}: {error}", listing.path));
            return listing;
        }
    };
    for entry in reader.flatten() {
        // Name.
        // 当前名称。
        let name = entry.file_name().to_string_lossy().into_owned();
        // Meta.
        // 准确文件系统元数据。
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        // Hidden.
        // 是否为隐藏项。
        let hidden = name.starts_with('.') || is_hidden(&meta);
        if (hidden && !show_hidden) || (!meta.is_dir() && !files) {
            continue;
        }
        listing.entries.push(DirEntry {
            is_repo: meta.is_dir() && entry.path().join(".git").exists(),
            is_dir: meta.is_dir(),
            size: (!meta.is_dir()).then_some(meta.len()),
            hidden,
            name,
        });
    }
    listing.entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    listing
}

/// Resolves the declared Windows or Unix user-home environment value.
/// 解析声明的 Windows 或 Unix 用户主目录环境值。
/// Returns `Option<PathBuf>`.
/// 返回 `Option<PathBuf>`。
fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(windows)]
/// Lists existing platform filesystem roots for the attachment picker.
/// 为附件选择器列出存在的平台文件系统根。
/// Returns `Vec<String>`.
/// 返回 `Vec<String>`。
fn roots() -> Vec<String> {
    (b'A'..=b'Z')
        .map(|letter| format!("{}:\\", letter as char))
        .filter(|root| std::path::Path::new(root).exists())
        .collect()
}

#[cfg(not(windows))]
/// Lists existing platform filesystem roots for the attachment picker.
/// 为附件选择器列出存在的平台文件系统根。
/// Returns `Vec<String>`.
/// 返回 `Vec<String>`。
fn roots() -> Vec<String> {
    vec!["/".to_string()]
}

#[cfg(windows)]
/// Checks platform hidden-file metadata; filename hiding is handled by the caller.
/// 检查平台隐藏文件元数据；文件名隐藏规则由调用方处理。
/// Returns `bool`.
/// 返回 `bool`。
fn is_hidden(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    /// `FILE_ATTRIBUTE_HIDDEN`
    const HIDDEN: u32 = 0x2;
    meta.file_attributes() & HIDDEN != 0
}

#[cfg(not(windows))]
/// Checks platform hidden-file metadata; filename hiding is handled by the caller.
/// 检查平台隐藏文件元数据；文件名隐藏规则由调用方处理。
/// Returns `bool`.
/// 返回 `bool`。
fn is_hidden(_meta: &fs::Metadata) -> bool {
    false
}
