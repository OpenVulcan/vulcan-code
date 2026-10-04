//! Manifest-driven bundled Vulcan Plugin catalog and managed runtime host.
//! 清单驱动的 Vulcan 插件目录与受管运行时宿主。

mod atomic_file;
mod catalog;
mod generation_store;
mod host_services;
mod manifest;
#[cfg(feature = "manifest-schema")]
pub mod manifest_authoring;
mod runtime;

/// Returns whether metadata identifies a symbolic link or any Windows reparse point.
/// 返回元数据是否标识符号链接或任意 Windows 重解析点。
///
/// # Parameters
/// 参数
/// `metadata` is obtained without following the inspected filesystem entry.
/// `metadata` 通过不跟随被检查文件系统条目的方式取得。
///
/// # Returns
/// 返回值
/// Returns `true` for link-like entries that must never cross a plugin trust boundary.
/// 对不得跨越插件信任边界的链接类条目返回 `true`。
pub(crate) fn metadata_is_link_or_reparse_point(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    false
}

pub use catalog::*;
pub use generation_store::*;
pub use host_services::*;
pub use manifest::*;
pub use runtime::*;
