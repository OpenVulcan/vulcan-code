//! Safe read-only access to the macOS mount table.
//! 以安全的只读接口访问 macOS 挂载表。

use std::ffi::OsString;
use std::io;
use std::mem::{MaybeUninit, size_of};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

/// One mount reported by the macOS kernel.
/// macOS 内核报告的一条挂载记录。
pub struct Mount {
    /// Absolute target path as reported by the kernel.
    /// 内核报告的绝对挂载路径。
    pub path: PathBuf,
    /// Whether the operating system hides the volume from user-facing browsers.
    /// 操作系统是否对用户界面的浏览器隐藏此卷。
    pub hidden: bool,
}

/// Returns one snapshot of mounted filesystems from the macOS kernel.
/// 返回 macOS 内核当前挂载的文件系统快照。
///
/// Reads cached kernel records so an unrelated disconnected filesystem does not delay browsing.
/// 读取内核缓存记录，避免不相关的断开文件系统拖慢浏览。
#[allow(unsafe_code)]
pub fn mounted_filesystems() -> io::Result<Vec<Mount>> {
    // SAFETY: A null buffer and zero byte length query the required record count only.
    // 安全性：空指针与零字节长度仅查询所需记录数量。
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }

    // A spare record lets a mount added between calls appear in the same snapshot.
    // 预留一个记录，以便两次调用之间新增的挂载也能进入同一快照。
    let capacity = count as usize + 1;
    let bytes = capacity
        .checked_mul(size_of::<libc::statfs>())
        .and_then(|bytes| libc::c_int::try_from(bytes).ok())
        .ok_or_else(|| io::Error::other("macOS mount table is too large"))?;
    let mut records = vec![MaybeUninit::<libc::statfs>::uninit(); capacity];
    // SAFETY: The allocation is aligned and provides exactly the byte count passed to the kernel.
    // 安全性：缓冲区已按记录对齐，其大小与传入内核的字节数一致。
    let written = unsafe { libc::getfsstat(records.as_mut_ptr().cast(), bytes, libc::MNT_NOWAIT) };
    if written < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(records
        .into_iter()
        .take(written as usize)
        .map(|record| {
            // SAFETY: The call initialized the returned number of records in the supplied buffer.
            // 安全性：调用初始化了提供的缓冲区中返回数量对应的记录。
            let record = unsafe { record.assume_init() };
            let path = record
                .f_mntonname
                .into_iter()
                .take_while(|byte| *byte != 0)
                .map(|byte| byte as u8)
                .collect();
            Mount {
                path: OsString::from_vec(path).into(),
                hidden: record.f_flags & libc::MNT_DONTBROWSE as u32 != 0,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The native snapshot exposes the root with its actual absolute path.
    /// 原生快照以实际绝对路径报告根挂载。
    #[test]
    fn native_snapshot_contains_root() {
        let mounts = mounted_filesystems().expect("read the macOS mount table");
        assert!(
            mounts
                .iter()
                .any(|mount| mount.path == std::path::Path::new("/"))
        );
        assert!(mounts.iter().all(|mount| mount.path.is_absolute()));
    }
}

/// Returns the filesystem UUID backing path. Filesystems without a UUID use their device number.
/// 返回支撑 path 的文件系统 UUID。未提供 UUID 的文件系统使用设备号。
#[allow(unsafe_code)]
pub fn volume_identity(path: &std::path::Path) -> io::Result<Option<String>> {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

    // ATTR_VOL_INFO is required for requesting any Darwin volume attribute.
    // 请求任意 Darwin 卷属性时必须设置 ATTR_VOL_INFO。
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: 0,
        volattr: libc::ATTR_VOL_INFO | libc::ATTR_VOL_UUID,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut output = [0_u32; 5];
    // SAFETY: c_path is NUL terminated and output has room for the length word and 16-byte UUID.
    // 安全性：c_path 以 NUL 结尾，output 包含长度字段和 16 字节 UUID 所需的空间。
    let result = unsafe {
        libc::getattrlist(
            c_path.as_ptr(),
            std::ptr::from_mut(&mut attributes).cast(),
            output.as_mut_ptr().cast(),
            std::mem::size_of_val(&output),
            0,
        )
    };
    if result == 0 && output[0] == 20 && output[1..].iter().any(|word| *word != 0) {
        return Ok(Some(format!("mac:uuid:{:08x?}", &output[1..])));
    }
    Ok(Some(format!("mac:dev:{}", std::fs::metadata(path)?.dev())))
}
