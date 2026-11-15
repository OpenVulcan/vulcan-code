//! Read the volume serial from an open Windows directory handle.
//! 从已打开的 Windows 目录句柄读取卷序列号。

use std::fs::OpenOptions;
use std::io;
use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, GetVolumeInformationByHandleW,
};

/// Returns the filesystem serial for the volume currently backing path, or an OS error.
/// 返回当前支撑 path 的卷的文件系统序列号，或操作系统错误。
#[allow(unsafe_code)]
pub fn volume_identity(path: &Path) -> io::Result<Option<String>> {
    // Open the directory itself, so a drive-letter or junction reassignment is resolved by Windows.
    // 打开目录对象，由 Windows 解析盘符或目录联接的重新分配。
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut serial = 0;
    // SAFETY: The directory handle remains live, serial points to writable storage, and all
    // optional output buffers are null with zero length.
    // 安全性：目录句柄仍存活，serial 指向可写内存，可选输出缓冲区为空且长度为零。
    let success = unsafe {
        GetVolumeInformationByHandleW(
            directory.as_raw_handle(),
            null_mut(),
            0,
            &mut serial,
            null_mut(),
            null_mut(),
            null_mut(),
            0,
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Some(format!("win:{serial:08x}")))
}
