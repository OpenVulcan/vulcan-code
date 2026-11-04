//! Taking files a published release already ships, so a release reuses their built and signed bytes
//! (plan section 6).
//! 取出已发布版本中已有的文件，使新版本复用其已构建、已签名的字节（方案第六节）。
//!
//! A reuse record only names the version; the files come from that version's signed manifest and the
//! indexes it lists, so a record or pack changed on the server can never bring other bytes into a release.
//! 复用记录只指明版本；文件取自该版本的签名版本清单及其列出的索引，因此服务器上被改动的记录或文件包
//! 永远无法把其他字节带进新版本。

use std::collections::HashMap;
use std::fs::File;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context as _, bail};
use vulcan_install_core::assemble::{DirectoryPackSource, PackSource as _};
use vulcan_install_core::index::IndexEntry;
use vulcan_install_core::names::{Platform, ReleaseVersion};
use vulcan_install_core::pack::extract_frame;
use vulcan_install_core::signing::TrustedKey;

use crate::verify::published_release;

/// Writes each of `paths` as `version` ships it on `platform` below `out`; `mirror` holds the
/// manifest, its signature, the indexes and the packs in the object layout.
/// 将 `paths` 中的每个路径按 `version` 在 `platform` 上的发行内容写到 `out` 之下；`mirror` 以对象布局存放
/// 版本清单、其签名、索引与文件包。
pub fn restore_files(
    mirror: &Path,
    platform: Platform,
    version: &ReleaseVersion,
    paths: &[&str],
    out: &Path,
    keys: &[TrustedKey],
) -> anyhow::Result<()> {
    let (_, indexes) = published_release(mirror, platform, version, keys)?;
    let entries: HashMap<&str, &IndexEntry> = indexes
        .iter()
        .flat_map(|index| &index.entries)
        .map(|entry| (entry.path().as_str(), entry))
        .collect();
    let mut source = DirectoryPackSource::new(mirror);
    for path in paths {
        let Some(IndexEntry::File {
            mode,
            size,
            sha256,
            pack,
            offset,
            length,
            ..
        }) = entries.get(path).copied()
        else {
            bail!("{version} ships no file `{path}` for {platform}");
        };
        let target = out.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let mut partial = target.clone().into_os_string();
        partial.push(".partial");
        let partial = std::path::PathBuf::from(partial);
        let written = (|| -> anyhow::Result<()> {
            let mut file = File::create(&partial)?;
            extract_frame(
                source.open_range(*pack, *offset, *length)?,
                *size,
                *sha256,
                &mut file,
            )?;
            file.flush()?;
            set_mode(&file, *mode)?;
            Ok(())
        })();
        if let Err(error) = written {
            let _ = std::fs::remove_file(&partial);
            return Err(error.context(format!("restoring `{path}` from {version}")));
        }
        std::fs::rename(&partial, &target)
            .with_context(|| format!("moving {} into place", target.display()))?;
    }
    Ok(())
}

/// Applies the recorded Unix permission bits; Windows has none to apply.
/// 应用记录的 Unix 权限位；Windows 没有可应用的权限位。
fn set_mode(file: &File, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(std::fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (file, mode);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::tests::{keys, release, write};

    #[test]
    fn only_files_the_signed_release_ships_are_restored() {
        let temp = tempfile::tempdir().expect("temp dir");
        let tree = temp.path().join("tree");
        write(&tree, "bin/service", &[7; 3_000]);
        write(&tree, "webui/index.html", b"<html></html>");
        let version = release(temp.path(), &tree, "0.2.0");
        let objects = temp.path().join("objects");
        let out = temp.path().join("out");

        restore_files(
            &objects,
            Platform::LinuxX64,
            &version,
            &["bin/service"],
            &out,
            &keys(),
        )
        .expect("restore");
        assert_eq!(
            std::fs::read(out.join("bin/service")).expect("read"),
            vec![7; 3_000]
        );
        assert!(!out.join("webui").exists(), "only the named paths");

        let missing = restore_files(
            &objects,
            Platform::LinuxX64,
            &version,
            &["bin/other"],
            &out,
            &keys(),
        );
        assert!(missing.is_err(), "a path the release does not ship");
        assert!(
            restore_files(
                &objects,
                Platform::LinuxX64,
                &version,
                &["bin/service"],
                &out,
                &[]
            )
            .is_err(),
            "a manifest no trusted key signed"
        );
    }

    #[test]
    fn a_changed_pack_is_rejected() {
        let temp = tempfile::tempdir().expect("temp dir");
        let tree = temp.path().join("tree");
        write(&tree, "bin/service", &[7; 3_000]);
        let version = release(temp.path(), &tree, "0.2.0");
        let objects = temp.path().join("objects");
        for pack in std::fs::read_dir(objects.join("packs")).expect("packs") {
            let path = pack.expect("pack").path();
            let mut bytes = std::fs::read(&path).expect("read");
            let last = bytes.len() - 1;
            bytes[last] ^= 0xff;
            std::fs::write(&path, bytes).expect("write");
        }
        let out = temp.path().join("out");
        assert!(
            restore_files(
                &objects,
                Platform::LinuxX64,
                &version,
                &["bin/service"],
                &out,
                &keys()
            )
            .is_err()
        );
        assert!(!out.join("bin/service").exists(), "nothing is left behind");
    }
}
