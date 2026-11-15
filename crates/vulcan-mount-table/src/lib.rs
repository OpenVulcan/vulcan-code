//! Safe access to mounted volume identifiers.
//! 已挂载卷标识的安全访问接口。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::volume_identity;
#[cfg(target_os = "macos")]
pub use macos::{Mount, mounted_filesystems, volume_identity};
#[cfg(windows)]
pub use windows::volume_identity;

/// Returns no volume identifier on platforms without a supported mount API.
/// 对于无受支持挂载 API 的平台返回空卷标识。
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn volume_identity(_path: &std::path::Path) -> std::io::Result<Option<String>> {
    Ok(None)
}
