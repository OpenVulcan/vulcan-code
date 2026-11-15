//! Resolve the effective mount through the Linux directory descriptor.
//! 使用 Linux 目录描述符解析实际挂载点。

use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
use std::path::Path;

/// Returns an identifier for the mounted filesystem backing path, or an OS error.
/// 返回支撑 path 的已挂载文件系统标识，或操作系统错误。
pub fn volume_identity(path: &Path) -> io::Result<Option<String>> {
    // The kernel associates this descriptor with the actual mount even for nested or stacked mounts.
    // 即使存在嵌套或叠加挂载，内核也把描述符与实际挂载点关联。
    let directory = File::open(path)?;
    let fd_info = fs::read_to_string(format!("/proc/self/fdinfo/{}", directory.as_raw_fd()))?;
    let mount_id = fd_info
        .lines()
        .find_map(|line| line.strip_prefix("mnt_id:"))
        .map(str::trim)
        .ok_or_else(|| io::Error::other("The directory descriptor did not report a mount id"))?;
    let mounts = fs::read("/proc/self/mountinfo")?;
    let MountIdentityRow {
        device,
        target,
        filesystem,
        source,
    } = mount_identity_row(&mounts, mount_id.as_bytes())
        .ok_or_else(|| io::Error::other("The directory's mount is no longer present"))?;
    // The UUID follows local filesystems even when the same disk receives a different device number.
    // 即使同一磁盘被分配不同设备号，UUID 仍随本地文件系统保持稳定。
    if source.starts_with(b"/dev/")
        && let Ok(source_metadata) = fs::metadata(std::ffi::OsStr::from_bytes(source))
        && let Some(uuid) = filesystem_uuid(source_metadata.rdev())
    {
        return Ok(Some(format!("linux:uuid:{uuid}")));
    }
    // Virtual and network filesystems may expose no block UUID. The host's mount source, target,
    // and device number together distinguish the mounted filesystem from its underlying directory.
    // 虚拟及网络文件系统可没有块设备 UUID。宿主的挂载来源、目标和设备号共同区分挂载卷与底层目录。
    Ok(Some(format!(
        "linux:mount:{device:?}:{target:?}:{filesystem:?}:{source:?}"
    )))
}

/// Holds the borrowed fields that identify a single mountinfo row.
/// 保存标识一条 mountinfo 记录所需的借用字段。
struct MountIdentityRow<'a> {
    /// The kernel's major and minor device numbers as printed in mountinfo.
    /// mountinfo 中记录的内核主、次设备号。
    device: &'a [u8],
    /// The mount point in the current mount namespace.
    /// 当前挂载命名空间中的挂载点。
    target: &'a [u8],
    /// The filesystem type recorded after the separator.
    /// 分隔符之后记录的文件系统类型。
    filesystem: &'a [u8],
    /// The source backing this mount.
    /// 支撑此挂载点的来源。
    source: &'a [u8],
}

/// Finds the row with the exact mount_id in contents and returns its identity fields if present.
/// 在 contents 中查找 mount_id 完全匹配的行，字段齐全时返回该行的标识信息。
fn mount_identity_row<'a>(contents: &'a [u8], mount_id: &[u8]) -> Option<MountIdentityRow<'a>> {
    let line = contents
        .split(|byte| *byte == b'\n')
        .find(|line| line.split(|byte| *byte == b' ').next() == Some(mount_id))?;
    let mut fields = line.split(|byte| *byte == b' ');
    let device = fields.nth(2)?;
    let target = fields.nth(1)?;
    let mut details = fields.skip_while(|field| *field != b"-").skip(1);
    Some(MountIdentityRow {
        device,
        target,
        filesystem: details.next()?,
        source: details.next()?,
    })
}

/// Matches the kernel's device number to the filesystem UUID symlink maintained by udev.
/// 将内核设备号匹配到 udev 维护的文件系统 UUID 符号链接。
fn filesystem_uuid(device: u64) -> Option<String> {
    if device == 0 {
        return None;
    }
    fs::read_dir("/dev/disk/by-uuid")
        .ok()?
        .flatten()
        .find_map(|entry| {
            (fs::metadata(entry.path()).ok()?.rdev() == device)
                .then(|| entry.file_name().into_string().ok())
                .flatten()
        })
}
