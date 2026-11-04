//! Finding, describing and fetching a newer release, shared by the in-app updater and the setup
//! program (plan section 8).
//! 查找、描述并获取更新的版本，由应用内更新与安装器共用（方案第八节）。
//!
//! Order of trust: the channel pointer is verified first, versions are compared, and only then are
//! the manifest and the notes it names downloaded and checked.
//! 信任顺序：先校验通道指针，再比较版本，之后才下载并校验其指向的版本清单与更新说明。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{FormatError, FormatResult};
use crate::fetch::Downloader;
use crate::index::ComponentIndex;
use crate::layout::{
    index_key, manifest_key, notes_key, pointer_key, setup_pointer_key, signature_key,
};
use crate::limits::{
    MAX_INDEX_BYTES, MAX_MANIFEST_BYTES, MAX_NOTES_BYTES, MAX_POINTER_BYTES, SIGNATURE_BYTES,
};
use crate::manifest::ReleaseManifest;
use crate::names::{Channel, ComponentName, Platform, ReleaseVersion};
use crate::pointer::ChannelPointer;
use crate::setup::SetupPointer;
use crate::signing::TrustedKey;

/// Format version of the notes document; `scripts/release-plan.mjs` (`NOTES_SCHEMA`) writes it.
/// 更新说明文档的格式版本；由 `scripts/release-plan.mjs`（`NOTES_SCHEMA`）写出。
pub const NOTES_SCHEMA: u32 = 1;

/// The release notes of one version in every registered language.
/// 某版本在每种注册语言中的更新说明。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseNotes {
    /// Always [`NOTES_SCHEMA`].
    /// 恒为 [`NOTES_SCHEMA`]。
    pub schema: u32,
    /// The version described.
    /// 所描述的版本。
    pub version: ReleaseVersion,
    /// Release date as `YYYY-MM-DD`.
    /// 发布日期，格式为 `YYYY-MM-DD`。
    pub date: Option<String>,
    /// Markdown notes by language code.
    /// 按语言代码列出的 Markdown 说明。
    pub notes: BTreeMap<String, String>,
}

/// A verified release newer than the installed one.
/// 比已安装版本更新、已校验的版本。
#[derive(Clone, Debug)]
pub struct Offer {
    /// The channel whose pointer named it.
    /// 指向它的指针所属的通道。
    pub channel: Channel,
    /// Its manifest.
    /// 其版本清单。
    pub manifest: ReleaseManifest,
    /// Its notes, when it has any.
    /// 其更新说明（若有）。
    pub notes: Option<ReleaseNotes>,
}

/// Reads the pointers of `channels` and returns the newest release above `installed`, if any.
/// 读取 `channels` 的指针，返回高于 `installed` 的最新版本（若有）。
///
/// A newer version on any channel wins, so leaving a channel never moves back to an older version.
/// 任一通道上更新的版本胜出，因此退出某通道绝不会回到更旧的版本。
pub fn find_update(
    downloader: &mut Downloader,
    keys: &[TrustedKey],
    platform: Platform,
    channels: &[Channel],
    installed: &ReleaseVersion,
) -> FormatResult<Option<Offer>> {
    let mut newest: Option<ChannelPointer> = None;
    for channel in channels {
        let pointer = read_pointer(downloader, keys, *channel, platform)?;
        if newest
            .as_ref()
            .is_none_or(|known| pointer.version > known.version)
        {
            newest = Some(pointer);
        }
    }
    let Some(pointer) = newest.filter(|pointer| &pointer.version > installed) else {
        return Ok(None);
    };
    let key = manifest_key(&pointer.version, platform);
    let document = downloader.fetch(&key, MAX_MANIFEST_BYTES)?;
    let signature = downloader.fetch(&signature_key(&key), SIGNATURE_BYTES as u64)?;
    let manifest = ReleaseManifest::verify(&document, &signature, keys, &pointer)?;
    let notes = read_notes(downloader, &manifest)?;
    Ok(Some(Offer {
        channel: pointer.channel,
        manifest,
        notes,
    }))
}

/// Downloads and verifies the signed manifest of a known version, as a repair needs it.
/// 下载并校验某个已知版本的已签名版本清单，供修复使用。
pub fn read_manifest(
    downloader: &mut Downloader,
    keys: &[TrustedKey],
    version: &ReleaseVersion,
    platform: Platform,
) -> FormatResult<ReleaseManifest> {
    let key = manifest_key(version, platform);
    let document = downloader.fetch(&key, MAX_MANIFEST_BYTES)?;
    let signature = downloader.fetch(&signature_key(&key), SIGNATURE_BYTES as u64)?;
    ReleaseManifest::verify_release(&document, &signature, keys, version, platform)
}

/// Downloads and verifies the indexes of `components` named by `manifest`.
/// 下载并校验 `manifest` 所列 `components` 的索引。
pub fn read_indexes<'c>(
    downloader: &mut Downloader,
    manifest: &ReleaseManifest,
    components: impl IntoIterator<Item = &'c ComponentName>,
) -> FormatResult<Vec<ComponentIndex>> {
    let mut indexes = Vec::new();
    for component in components {
        let Some(reference) = manifest.components.get(component) else {
            return Err(FormatError::invalid(
                "update",
                format!(
                    "{} {} has no `{component}` component",
                    manifest.version, manifest.platform
                ),
            ));
        };
        let object = downloader.fetch(&index_key(reference.index), MAX_INDEX_BYTES)?;
        indexes.push(ComponentIndex::from_object(
            &object,
            reference.index,
            component,
        )?);
    }
    Ok(indexes)
}

/// Downloads and verifies one channel pointer, fetching it once more when the pair was caught
/// mid-replacement (`pointer.rs`).
/// 下载并校验一个通道指针；若取到的是替换中途的一对文件，再获取一次（见 `pointer.rs`）。
fn read_pointer(
    downloader: &mut Downloader,
    keys: &[TrustedKey],
    channel: Channel,
    platform: Platform,
) -> FormatResult<ChannelPointer> {
    let key = pointer_key(channel, platform);
    let mut attempt = || -> FormatResult<ChannelPointer> {
        let document = downloader.fetch(&key, MAX_POINTER_BYTES)?;
        let signature = downloader.fetch(&signature_key(&key), SIGNATURE_BYTES as u64)?;
        ChannelPointer::verify(&document, &signature, keys, channel, platform)
    };
    match attempt() {
        Err(FormatError::SignatureRejected | FormatError::SignatureLength { .. }) => attempt(),
        other => other,
    }
}

/// Reads the setup pointers of `channels` and returns the newest one above `running`, if any; as
/// with [`find_update`], a newer version on any channel wins.
/// 读取 `channels` 的安装器指针，返回高于 `running` 的最新指针（若有）；与 [`find_update`] 相同，任一
/// 通道上更新的版本胜出。
pub fn find_setup_update(
    downloader: &mut Downloader,
    keys: &[TrustedKey],
    channels: &[Channel],
    running: &ReleaseVersion,
) -> FormatResult<Option<SetupPointer>> {
    let mut newest: Option<SetupPointer> = None;
    for channel in channels {
        let pointer = read_setup_pointer(downloader, keys, *channel)?;
        if newest
            .as_ref()
            .is_none_or(|known| pointer.version > known.version)
        {
            newest = Some(pointer);
        }
    }
    Ok(newest.filter(|pointer| &pointer.version > running))
}

/// Downloads and verifies the setup pointer of `channel`, fetching it once more when the pair was
/// caught mid-replacement (`setup.rs`).
/// 下载并校验 `channel` 的安装器指针；若取到的是替换中途的一对文件，再获取一次（见 `setup.rs`）。
pub fn read_setup_pointer(
    downloader: &mut Downloader,
    keys: &[TrustedKey],
    channel: Channel,
) -> FormatResult<SetupPointer> {
    let key = setup_pointer_key(channel);
    let mut attempt = || -> FormatResult<SetupPointer> {
        let document = downloader.fetch(&key, MAX_POINTER_BYTES)?;
        let signature = downloader.fetch(&signature_key(&key), SIGNATURE_BYTES as u64)?;
        SetupPointer::verify(&document, &signature, keys, channel)
    };
    match attempt() {
        Err(FormatError::SignatureRejected | FormatError::SignatureLength { .. }) => attempt(),
        other => other,
    }
}

/// Downloads and verifies the notes a manifest names.
/// 下载并校验版本清单所指的更新说明。
fn read_notes(
    downloader: &mut Downloader,
    manifest: &ReleaseManifest,
) -> FormatResult<Option<ReleaseNotes>> {
    let Some(reference) = manifest.notes else {
        return Ok(None);
    };
    let bytes = downloader.fetch(&notes_key(&manifest.version), MAX_NOTES_BYTES)?;
    reference
        .sha256
        .expect_eq("release notes", crate::digest::Sha256Digest::of(&bytes))?;
    let notes: ReleaseNotes =
        serde_json::from_slice(&bytes).map_err(|source| FormatError::Json {
            what: "release notes",
            source,
        })?;
    if notes.schema != NOTES_SCHEMA || notes.version != manifest.version {
        return Err(FormatError::invalid(
            "release notes",
            format!(
                "schema {} for {} does not describe {}",
                notes.schema, notes.version, manifest.version
            ),
        ));
    }
    Ok(Some(notes))
}
