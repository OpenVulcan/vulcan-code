//! Offline proof of a fully prepared update. This marker never authorizes a switch.
//! 已完整准备的更新的离线凭证。此标记不授权版本切换。

use serde::{Deserialize, Serialize};
use vulcan_install_core::assemble::hash_file;
use vulcan_install_core::install_root::UPDATES_DIR;
use vulcan_install_core::manifest::ReleaseManifest;
use vulcan_install_core::names::{Channel, ReleaseVersion};
use vulcan_install_core::record::{InstallRecord, RecordEntry, modified_ns};
use vulcan_install_core::signing::TrustedKey;
use vulcan_install_core::{FormatError, Sha256Digest};

use super::{
    Apply, Installed, Method, Ready, full_installer, invalid, method_for, signer, update_package,
};

/// Marker read by Core only; the launcher continues to use the current-version pointer.
/// 仅由 Core 读取的标记；启动器继续使用当前版本指针。
pub(super) const PREPARED_UPDATE_FILE: &str = "prepared-update.json";

/// The initial marker schema.
/// 标记文件的初始格式。
const SCHEMA: u32 = 1;

/// A signed target prepared against one running version.
/// 针对一个运行版本完整准备的签名目标。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PreparedUpdate {
    /// The format of this marker.
    /// 此标记的格式。
    schema: u32,
    /// The running version that supplied the selected components.
    /// 提供所选组件列表的运行版本。
    running: ReleaseVersion,
    /// The fully prepared version.
    /// 完整准备好的版本。
    version: ReleaseVersion,
    /// The channel that offered this release.
    /// 提供该发布的通道。
    channel: Channel,
    /// Unmodified signed bytes as verified at check time.
    /// 检查时已校验的原始签名字节。
    manifest: Vec<u8>,
    /// The detached signature of the manifest.
    /// 清单的分离签名。
    signature: Vec<u8>,
}

impl PreparedUpdate {
    /// Captures the newly prepared `version` and proof for recovery after exit.
    /// 记录新准备的 `version` 及凭证，以便退出后恢复。
    pub(super) fn new(
        installed: &Installed,
        version: ReleaseVersion,
        channel: Channel,
        (manifest, signature): (Vec<u8>, Vec<u8>),
    ) -> Self {
        Self {
            schema: SCHEMA,
            running: installed.version.clone(),
            version,
            channel,
            manifest,
            signature,
        }
    }

    /// Revalidates the manifest and available artifacts and reconstructs the apply action.
    /// 复核清单及可用产物，并还原安装操作。
    pub(super) fn restore(
        self,
        installed: &Installed,
        keys: &[TrustedKey],
    ) -> Result<Ready, FormatError> {
        if self.schema != SCHEMA
            || self.running != installed.version
            || self.version <= self.running
        {
            return Err(invalid(
                "the prepared update does not belong to this running version",
            ));
        }
        let manifest = ReleaseManifest::verify_release(
            &self.manifest,
            &self.signature,
            keys,
            &self.version,
            installed.platform,
        )?;
        let apply = match method_for(installed, &manifest) {
            Method::Switch => {
                validate_assembly(installed, &manifest)?;
                Apply::Switch
            }
            Method::Setup => {
                validate_assembly(installed, &manifest)?;
                Apply::Setup(self.channel)
            }
            Method::InnoUpdate => {
                validate_assembly(installed, &manifest)?;
                let package = update_package(installed, &manifest).ok_or_else(|| {
                    invalid("the prepared release has no compatible update package")
                })?;
                installer(installed, &package.file, package.size, package.sha256)?
            }
            Method::Installer => {
                let package = full_installer(installed, &manifest)
                    .ok_or_else(|| invalid("the prepared release has no compatible installer"))?;
                installer(installed, &package.file, package.size, package.sha256)?
            }
            Method::Manual => return Err(invalid("the prepared release cannot be installed here")),
        };
        Ok(Ready {
            version: self.version,
            apply,
        })
    }
}

/// Verifies the complete version record against the signed selected component digests.
/// 按已签名的所选组件摘要核对完整版本记录。
pub(super) fn validate_assembly(
    installed: &Installed,
    manifest: &ReleaseManifest,
) -> Result<(), FormatError> {
    let directory = installed.root.version_dir(&manifest.version);
    let record = InstallRecord::read(&directory)?;
    if record.version != manifest.version
        || record.platform != manifest.platform
        || record.install_epoch != manifest.install_epoch
        || record.components.len() != installed.components.len()
        || installed.components.iter().any(|component| {
            manifest
                .components
                .get(component)
                .is_none_or(|reference| record.components.get(component) != Some(&reference.index))
        })
    {
        return Err(invalid(
            "the assembled version does not match the signed release manifest",
        ));
    }
    for entry in &record.entries {
        let path = directory.join(entry.path().as_str());
        let metadata = path.symlink_metadata().map_err(|source| FormatError::Io {
            what: "reading a prepared file",
            source,
        })?;
        match entry {
            RecordEntry::File {
                size,
                sha256,
                modified_ns: recorded,
                ..
            } if metadata.is_file() && metadata.len() == *size => {
                if *recorded == 0 || modified_ns(&metadata) != *recorded {
                    sha256.expect_eq("prepared file", hash_file(&path)?)?;
                }
            }
            RecordEntry::Directory { .. } if metadata.is_dir() => {}
            RecordEntry::Symlink { target, .. }
                if std::fs::read_link(&path)
                    .is_ok_and(|path| path == std::path::Path::new(target)) => {}
            _ => return Err(invalid("a prepared path differs from its install record")),
        }
    }
    Ok(())
}

/// Resolves an installer from signed metadata and revalidates its contents and publisher.
/// 根据签名元数据定位安装包，并复核其内容及发布者。
fn installer(
    installed: &Installed,
    file: &str,
    size: u64,
    sha256: Sha256Digest,
) -> Result<Apply, FormatError> {
    let path = installed
        .root
        .path()
        .join(UPDATES_DIR)
        .join("installers")
        .join(file);
    let metadata = path.symlink_metadata().map_err(|source| FormatError::Io {
        what: "reading the prepared installer",
        source,
    })?;
    if !metadata.is_file() || metadata.len() != size {
        return Err(invalid(
            "the prepared installer does not match the signed size",
        ));
    }
    sha256.expect_eq("prepared installer", hash_file(&path)?)?;
    signer::verify_installer_signer(&path).map_err(|reason| FormatError::Download {
        key: file.to_owned(),
        reason,
    })?;
    Ok(Apply::Installer(path))
}
