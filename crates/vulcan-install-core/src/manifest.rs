//! The signed release manifest `releases/<version>/<platform>/manifest.json` (plan section 7).
//! 已签名的版本清单 `releases/<版本>/<平台>/manifest.json`（方案第七节）。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::digest::Sha256Digest;
use crate::error::{FormatError, FormatResult};
use crate::limits::{MAX_INSTALLER_BYTES, MAX_MANIFEST_BYTES};
use crate::names::{ComponentName, PRODUCT, Platform, ReleaseVersion, SCHEMA_VERSION};
use crate::pointer::ChannelPointer;
use crate::signing::{TrustedKey, expect_key_id, parse_signed};

/// The release notes of the version.
/// 该版本的更新说明。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotesRef {
    /// SHA-256 of the notes document.
    /// 更新说明文档的 SHA-256。
    pub sha256: Sha256Digest,
}

/// One installable component of the release.
/// 发布中的一个可安装组件。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentRef {
    /// SHA-256 of the compressed component index.
    /// 压缩后组件索引的 SHA-256。
    pub index: Sha256Digest,
    /// Total size of the component's files once installed.
    /// 组件文件安装后的总大小。
    pub size: u64,
    /// Build fingerprint the release tooling reused or produced; clients ignore it.
    /// 发布工具复用或生成的构建指纹；客户端忽略它。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<Sha256Digest>,
}

/// What a separately downloadable file is. The setup program and the install scripts are released
/// on their own (`crate::setup`), so a product release never lists them.
/// 可单独下载文件的类型。安装器与安装脚本单独发布（`crate::setup`），因此产品版本从不列出它们。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadKind {
    /// The signed Inno installer, on Windows only; the in-app full update runs it.
    /// 已签名的 Inno 安装包，仅 Windows；应用内全量更新运行它。
    Inno,
    /// The disk image for browser downloads, on macOS only.
    /// 供浏览器下载的磁盘映像，仅 macOS。
    Dmg,
}

impl DownloadKind {
    /// The file names this kind may carry for `version` on `platform`; empty when the platform
    /// does not ship this kind, so a manifest can never name another file.
    /// 此类型在 `platform` 上 `version` 版本可用的文件名；平台不提供此类型时为空，因此版本清单无法指定
    /// 其他文件。
    pub fn allowed_files(self, version: &ReleaseVersion, platform: Platform) -> Vec<String> {
        match self {
            Self::Inno if platform.is_windows() => {
                vec![format!("VulcanCode-{version}-{platform}-setup.exe")]
            }
            Self::Dmg if platform.is_macos() => {
                vec![format!("VulcanCode-{version}-{platform}.dmg")]
            }
            Self::Inno | Self::Dmg => Vec::new(),
        }
    }

    /// The largest file of this kind.
    /// 此类型文件的最大字节数。
    pub const fn size_limit(self) -> u64 {
        match self {
            Self::Inno | Self::Dmg => MAX_INSTALLER_BYTES,
        }
    }
}

/// One file stored at `installers/<version>/<file>`; the manifest names no URLs.
/// 存放在 `installers/<版本>/<文件>` 的一个文件；版本清单不写网址。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Download {
    /// What the file is.
    /// 文件类型。
    pub kind: DownloadKind,
    /// File name below `installers/<version>/`.
    /// `installers/<版本>/` 下的文件名。
    pub file: String,
    /// SHA-256 of the file.
    /// 文件的 SHA-256。
    pub sha256: Sha256Digest,
    /// Size of the file.
    /// 文件大小。
    pub size: u64,
}

/// Everything one version publishes for one platform.
/// 一个版本为一个平台发布的全部内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifest {
    /// Format version; only [`SCHEMA_VERSION`] is accepted.
    /// 格式版本；只接受 [`SCHEMA_VERSION`]。
    pub schema: u32,
    /// Always [`PRODUCT`].
    /// 恒为 [`PRODUCT`]。
    pub product: String,
    /// The product version.
    /// 产品版本。
    pub version: ReleaseVersion,
    /// The platform.
    /// 平台。
    pub platform: Platform,
    /// Raised when an incremental update cannot reach this version (plan section 9).
    /// 增量更新无法到达此版本时递增（方案第九节）。
    pub install_epoch: u32,
    /// Release time as `YYYY-MM-DDTHH:MM:SSZ`.
    /// 发布时间，格式为 `YYYY-MM-DDTHH:MM:SSZ`。
    pub released_at: String,
    /// The release notes, when the version has any.
    /// 更新说明（若该版本有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<NotesRef>,
    /// Installable components by name.
    /// 按名称列出的可安装组件。
    pub components: BTreeMap<ComponentName, ComponentRef>,
    /// Separately downloadable files of this platform.
    /// 本平台可单独下载的文件。
    #[serde(default)]
    pub downloads: Vec<Download>,
    /// Oldest setup program that can install this version's layout.
    /// 能安装此版本布局的最旧安装器版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_setup_version: Option<ReleaseVersion>,
    /// The release published before this one, so an updater can walk back and show the notes of
    /// every skipped version. Optional and ignored by older readers; manifests published before it
    /// existed end the walk.
    /// 在此之前发布的版本，使更新器能逐个回溯并显示每个被跳过版本的说明。该字段可选，旧读取方会忽略它；
    /// 在它出现之前发布的清单使回溯到此为止。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_version: Option<ReleaseVersion>,
    /// The key that signed this manifest.
    /// 签署本清单的公钥。
    pub key_id: String,
}

impl ReleaseManifest {
    /// Verifies downloaded manifest bytes against the pointer that named them.
    /// 按指向它的指针校验下载的版本清单字节。
    pub fn verify(
        document: &[u8],
        signature: &[u8],
        keys: &[TrustedKey],
        pointer: &ChannelPointer,
    ) -> FormatResult<Self> {
        pointer
            .manifest
            .expect_matches("release manifest", document)?;
        let (manifest, key): (Self, _) = parse_signed(
            "release manifest",
            MAX_MANIFEST_BYTES,
            document,
            signature,
            keys,
        )?;
        expect_key_id(&manifest.key_id, key)?;
        manifest.validate()?;
        if manifest.version != pointer.version || manifest.platform != pointer.platform {
            return Err(FormatError::invalid(
                "release manifest",
                format!(
                    "describes {} {} but the pointer names {} {}",
                    manifest.version, manifest.platform, pointer.version, pointer.platform
                ),
            ));
        }
        Ok(manifest)
    }

    /// Verifies the manifest of an already known version without a pointer, as a repair of the
    /// installed version reads it; the signature alone establishes trust.
    /// 不经指针校验某个已知版本的版本清单，修复已安装版本时如此读取；仅凭签名建立信任。
    pub fn verify_release(
        document: &[u8],
        signature: &[u8],
        keys: &[TrustedKey],
        version: &ReleaseVersion,
        platform: Platform,
    ) -> FormatResult<Self> {
        let (manifest, key): (Self, _) = parse_signed(
            "release manifest",
            MAX_MANIFEST_BYTES,
            document,
            signature,
            keys,
        )?;
        expect_key_id(&manifest.key_id, key)?;
        manifest.validate()?;
        if &manifest.version != version || manifest.platform != platform {
            return Err(FormatError::invalid(
                "release manifest",
                format!(
                    "describes {} {} instead of {version} {platform}",
                    manifest.version, manifest.platform
                ),
            ));
        }
        Ok(manifest)
    }

    /// Checks the rules that hold for every manifest.
    /// 校验对每份清单都成立的规则。
    pub fn validate(&self) -> FormatResult<()> {
        let invalid = |reason: String| Err(FormatError::invalid("release manifest", reason));
        if self.schema != SCHEMA_VERSION {
            return invalid(format!("unsupported schema {}", self.schema));
        }
        if self.product != PRODUCT {
            return invalid(format!("describes product `{}`", self.product));
        }
        if self.install_epoch == 0 {
            return invalid("install epoch starts at 1".to_owned());
        }
        if !is_utc_timestamp(&self.released_at) {
            return invalid(format!(
                "releasedAt `{}` is not YYYY-MM-DDTHH:MM:SSZ",
                self.released_at
            ));
        }
        if self.components.is_empty() {
            return invalid("lists no components".to_owned());
        }
        if let Some(previous) = &self.previous_version
            && previous >= &self.version
        {
            return invalid(format!(
                "previous version {previous} is not older than {}",
                self.version
            ));
        }
        let mut files = BTreeSet::new();
        for download in &self.downloads {
            let allowed = download.kind.allowed_files(&self.version, self.platform);
            if !allowed.contains(&download.file) {
                return invalid(format!(
                    "{:?} download `{}` is not allowed for {} {}",
                    download.kind, download.file, self.version, self.platform
                ));
            }
            if !files.insert(download.file.as_str()) {
                return invalid(format!("download `{}` is listed twice", download.file));
            }
            if download.size == 0 || download.size > download.kind.size_limit() {
                return invalid(format!(
                    "download `{}` size {} is outside 1..={}",
                    download.file,
                    download.size,
                    download.kind.size_limit()
                ));
            }
        }
        Ok(())
    }

    /// Validates and serializes the manifest as the bytes to sign and publish.
    /// 校验并序列化版本清单，得到待签名与发布的字节。
    pub fn to_document(&self) -> FormatResult<Vec<u8>> {
        self.validate()?;
        let document = serde_json::to_vec(self).map_err(|source| FormatError::Json {
            what: "release manifest",
            source,
        })?;
        if document.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(FormatError::TooLarge {
                what: "release manifest",
                limit: MAX_MANIFEST_BYTES,
            });
        }
        Ok(document)
    }

    /// The download of `kind` named `file`, if listed.
    /// 名为 `file` 的 `kind` 类型下载（若已列出）。
    pub fn download(&self, kind: DownloadKind, file: &str) -> Option<&Download> {
        self.downloads
            .iter()
            .find(|download| download.kind == kind && download.file == file)
    }
}

/// Whether `text` is exactly `YYYY-MM-DDTHH:MM:SSZ` with digits in every numeric place.
/// `text` 是否恰为 `YYYY-MM-DDTHH:MM:SSZ` 且数字位都是数字。
pub(crate) fn is_utc_timestamp(text: &str) -> bool {
    const SHAPE: &[u8] = b"dddd-dd-ddTdd:dd:ddZ";
    text.len() == SHAPE.len()
        && text.bytes().zip(SHAPE).all(|(byte, shape)| match shape {
            b'd' => byte.is_ascii_digit(),
            literal => byte == *literal,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Channel;
    use crate::pointer::DocumentRef;
    use crate::signing::sign_detached;
    use crate::signing::tests::{TEST_SECRET, test_keys};

    fn manifest(platform: Platform) -> ReleaseManifest {
        let version: ReleaseVersion = "0.1.3".parse().expect("version");
        let downloads = [DownloadKind::Inno, DownloadKind::Dmg]
            .into_iter()
            .flat_map(|kind| {
                kind.allowed_files(&version, platform)
                    .into_iter()
                    .map(move |file| Download {
                        kind,
                        file,
                        sha256: Sha256Digest::of(b"installer"),
                        size: 5,
                    })
            })
            .collect();
        ReleaseManifest {
            schema: SCHEMA_VERSION,
            product: PRODUCT.to_owned(),
            version,
            platform,
            install_epoch: 1,
            released_at: "2026-11-20T08:00:00Z".to_owned(),
            notes: None,
            components: BTreeMap::from([(
                ComponentName::new("core").expect("name"),
                ComponentRef {
                    index: Sha256Digest::of(b"index"),
                    size: 1,
                    fingerprint: None,
                },
            )]),
            downloads,
            min_setup_version: None,
            previous_version: None,
            key_id: "test-key".to_owned(),
        }
    }

    fn signed(manifest: &ReleaseManifest) -> (Vec<u8>, [u8; 64], ChannelPointer) {
        let document = manifest.to_document().expect("document");
        let signature = sign_detached(&document, &TEST_SECRET);
        let pointer = ChannelPointer {
            schema: SCHEMA_VERSION,
            product: PRODUCT.to_owned(),
            channel: Channel::Stable,
            platform: manifest.platform,
            version: manifest.version.clone(),
            manifest: DocumentRef::of(&document),
            key_id: "test-key".to_owned(),
        };
        (document, signature, pointer)
    }

    #[test]
    fn a_manifest_verifies_against_the_pointer_that_names_it() {
        let original = manifest(Platform::WindowsX64);
        let (document, signature, pointer) = signed(&original);
        let verified = ReleaseManifest::verify(&document, &signature, &test_keys(), &pointer)
            .expect("verified");
        assert_eq!(verified, original);

        let mut other_version = pointer.clone();
        other_version.version = "0.1.4".parse().expect("version");
        assert!(
            ReleaseManifest::verify(&document, &signature, &test_keys(), &other_version).is_err()
        );

        let mut tampered = document.clone();
        tampered.push(b' ');
        assert!(matches!(
            ReleaseManifest::verify(&tampered, &signature, &test_keys(), &pointer),
            Err(FormatError::SizeMismatch { .. })
        ));
    }

    #[test]
    fn unknown_fields_from_newer_producers_are_ignored() {
        let mut value = serde_json::to_value(manifest(Platform::LinuxX64)).expect("value");
        value["futureField"] = serde_json::json!(true);
        let parsed: ReleaseManifest = serde_json::from_value(value).expect("parsed");
        assert!(parsed.validate().is_ok());
    }

    #[test]
    fn downloads_must_match_their_kind_version_and_platform() {
        let mut dmg_on_windows = manifest(Platform::WindowsX64);
        dmg_on_windows.downloads.push(Download {
            kind: DownloadKind::Dmg,
            file: "VulcanCode-0.1.3-windows-x64.dmg".to_owned(),
            sha256: Sha256Digest::of(b"dmg"),
            size: 1,
        });
        assert!(dmg_on_windows.validate().is_err());

        let mut wrong_version = manifest(Platform::WindowsX64);
        wrong_version.downloads[0].file = "VulcanCode-0.1.2-windows-x64-setup.exe".to_owned();
        assert!(wrong_version.validate().is_err());

        let mut duplicated = manifest(Platform::MacosArm64);
        duplicated.downloads.push(duplicated.downloads[0].clone());
        assert!(duplicated.validate().is_err());

        let mut oversized = manifest(Platform::WindowsX64);
        oversized.downloads[0].size = MAX_INSTALLER_BYTES + 1;
        assert!(oversized.validate().is_err());

        let mut empty = manifest(Platform::MacosX64);
        empty.downloads[0].size = 0;
        assert!(empty.validate().is_err());
    }

    #[test]
    fn release_time_must_be_utc_seconds() {
        let mut local_time = manifest(Platform::MacosArm64);
        local_time.released_at = "2026-11-20T08:00:00+08:00".to_owned();
        assert!(local_time.validate().is_err());
    }
}
