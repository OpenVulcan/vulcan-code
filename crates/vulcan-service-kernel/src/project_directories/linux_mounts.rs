//! Linux project picker volumes come from the service process's mount namespace.
//! Linux 项目选择器的卷来自服务进程的挂载命名空间。

use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

/// Returns user-facing volume paths recorded by Linux, without touching the mounted filesystems.
/// 返回 Linux 记录的面向用户的卷路径，无需访问对应文件系统。
pub(super) fn mounted_volume_paths() -> io::Result<Vec<PathBuf>> {
    fs::read("/proc/self/mountinfo").map(|contents| parse_volume_paths(&contents))
}

/// Selects browsable mount targets from the mountinfo byte stream.
/// 从 mountinfo 字节流中选取用于目录浏览的挂载目标。
fn parse_volume_paths(contents: &[u8]) -> Vec<PathBuf> {
    contents
        .split(|byte| *byte == b'\n')
        .filter_map(|line| {
            let mut fields = line.split(|byte| *byte == b' ');
            let target = fields.nth(4)?;
            // The separator follows zero or more optional fields; source and type always follow it.
            // 分隔符位于零个或多个可选字段之后，来源与文件系统类型始终在分隔符后。
            let mut details = fields.skip_while(|field| *field != b"-").skip(1);
            let filesystem = details.next()?;
            let source = details.next()?;
            let path = PathBuf::from(OsString::from_vec(decode_mountinfo_path(target)?));
            (path.is_absolute() && visible_volume(&path, filesystem, source)).then_some(path)
        })
        .collect()
}

/// Decodes the kernel's escapes for spaces, tabs, newlines and literal backslashes, preserving Unix bytes.
/// 解码内核对空格、制表符、换行与反斜杠的转义，并保留 Unix 路径字节。
fn decode_mountinfo_path(mut input: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len());
    while let Some((&first, remaining)) = input.split_first() {
        if first != b'\\' {
            output.push(first);
            input = remaining;
            continue;
        }
        let (escape, rest) = remaining.split_at_checked(3)?;
        output.push(match escape {
            b"040" => b' ',
            b"011" => b'\t',
            b"012" => b'\n',
            b"134" => b'\\',
            _ => return None,
        });
        input = rest;
    }
    Some(output)
}

/// Applies the sidebar display policy after the kernel has identified actual mounts.
/// 由内核确认实际挂载后，应用侧栏展示规则。
fn visible_volume(path: &Path, filesystem: &[u8], source: &[u8]) -> bool {
    let media_path = ["/media", "/run/media", "/mnt"]
        .iter()
        .any(|base| path.starts_with(base));
    let system_path = ["/dev", "/proc", "/sys", "/run", "/boot"]
        .iter()
        .any(|base| path.starts_with(base));
    if (system_path && !path.starts_with("/run/media"))
        || matches!(
            filesystem,
            b"autofs" | b"devtmpfs" | b"overlay" | b"proc" | b"sysfs" | b"tmpfs"
        )
    {
        return false;
    }
    // Common removable-volume paths include filesystems without a /dev source (e.g. WSL drives).
    // 常见可移动卷位置也包含没有 /dev 来源的文件系统，例如 WSL 盘符。
    media_path
        || source.starts_with(b"/dev/")
        || matches!(
            filesystem,
            b"cifs" | b"smb3" | b"nfs" | b"nfs4" | b"fuse.sshfs"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps real removable, nested and same-device mount targets, and ignores service pseudo-filesystems.
    /// 保留可移动卷、嵌套挂载与同设备挂载，并排除系统伪文件系统。
    #[test]
    fn mountinfo_uses_actual_targets_and_decodes_kernel_escapes() {
        let mounts = br#"1 0 8:1 / / rw - ext4 /dev/sda1 rw
2 1 8:2 / /run/media/user/My\040USB rw shared:2 - exfat /dev/sdb1 rw
3 2 8:2 /work /run/media/user/My\040USB/team rw - exfat /dev/sdb1 rw
4 1 8:1 /cache /srv/Work\134Logs rw - ext4 /dev/sda1 rw
5 1 0:51 / /srv/network rw - nfs4 server:/share rw
6 1 0:42 / /mnt/wslg rw - tmpfs tmpfs rw
7 1 0:33 / /proc rw - proc proc rw
8 1 8:3 / /boot/efi rw - vfat /dev/sda2 rw
9 1 8:2 / /mnt/Other\011Drive rw - exfat /dev/sdb1 rw
10 1 8:2 / /mnt/Broken\099 rw - exfat /dev/sdb1 rw"#;
        assert_eq!(
            parse_volume_paths(mounts),
            [
                "/",
                "/run/media/user/My USB",
                "/run/media/user/My USB/team",
                "/srv/Work\\Logs",
                "/srv/network",
                "/mnt/Other\tDrive",
            ]
            .map(PathBuf::from)
        );
    }
}
