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

/// Most skipped releases whose notes an update reads; an older installation sees the newest ones and
/// learns that earlier notes are missing.
/// 一次更新最多读取多少个被跳过版本的说明；更旧的安装看到最新的若干个，并得知更早的说明未显示。
pub const MAX_SKIPPED_RELEASES: usize = 32;

/// The notes of the releases between the installed version and an offered one.
/// 已安装版本与所提供版本之间各发布的更新说明。
#[derive(Debug)]
pub struct SkippedReleaseNotes {
    /// Notes of the skipped releases that have any, newest first.
    /// 有说明的被跳过版本的说明，从新到旧排列。
    pub releases: Vec<ReleaseNotes>,
    /// Whether the walk reached the installed version, so no skipped release is missing.
    /// 回溯是否到达已安装版本，即没有遗漏任何被跳过的版本。
    pub complete: bool,
    /// The failure that ended the walk early, if one did.
    /// 使回溯提前结束的错误（若有）。
    pub error: Option<FormatError>,
}

/// Walks back from `newest` through each manifest's `previousVersion` and reads the notes of every
/// release above `installed`, verifying each manifest by its signature and each notes document by
/// the digest its manifest names.
/// 从 `newest` 起沿每份清单的 `previousVersion` 回溯，读取高于 `installed` 的每个发布的说明；每份清单
/// 按签名校验，每份说明按其清单所列摘要校验。
///
/// The notes only describe the update, so a missing link, an unreadable release or the
/// [`MAX_SKIPPED_RELEASES`] bound ends the walk with `complete: false` instead of failing the offer.
/// 这些说明只用于介绍更新，因此缺少链接、无法读取的发布或达到 [`MAX_SKIPPED_RELEASES`] 上限时，回溯以
/// `complete: false` 结束，而不使整个提议失败。
pub fn read_skipped_release_notes(
    downloader: &mut Downloader,
    keys: &[TrustedKey],
    newest: &ReleaseManifest,
    installed: &ReleaseVersion,
) -> SkippedReleaseNotes {
    let mut walk = SkippedReleaseNotes {
        releases: Vec::new(),
        complete: false,
        error: None,
    };
    let mut previous = newest.previous_version.clone();
    let mut visited = 0;
    while let Some(version) = previous {
        if &version <= installed {
            walk.complete = true;
            break;
        }
        if visited == MAX_SKIPPED_RELEASES {
            break;
        }
        visited += 1;
        let read = read_manifest(downloader, keys, &version, newest.platform)
            .and_then(|manifest| read_notes(downloader, &manifest).map(|notes| (manifest, notes)));
        match read {
            Ok((manifest, notes)) => {
                walk.releases.extend(notes);
                previous = manifest.previous_version;
            }
            Err(error) => {
                walk.error = Some(error);
                break;
            }
        }
    }
    walk
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::net::TcpListener;
    use std::sync::Arc;

    use super::*;
    use crate::digest::Sha256Digest;
    use crate::layout::{manifest_key, notes_key, signature_key};
    use crate::manifest::{ComponentRef, NotesRef};
    use crate::names::{PRODUCT, SCHEMA_VERSION};
    use crate::signing::sign_detached;
    use crate::signing::tests::{TEST_SECRET, test_keys};

    const PLATFORM: Platform = Platform::LinuxX64;

    /// Serves `objects` by key over plain HTTP and answers 404 for anything else.
    /// 通过明文 HTTP 按键提供 `objects`，其他路径一律应答 404。
    fn serve(objects: HashMap<String, Vec<u8>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let origin = format!("http://{}", listener.local_addr().expect("address"));
        let objects = Arc::new(objects);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut request = String::new();
                let _ = reader.read_line(&mut request);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let key = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .trim_start_matches('/');
                let (status, body) = match objects.get(key) {
                    Some(body) => ("200 OK", body.clone()),
                    None => ("404 Not Found", Vec::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        origin
    }

    /// Publishes one signed release with notes into `objects` and returns its manifest.
    /// 将一个带说明的已签名发布写入 `objects` 并返回其清单。
    fn publish(
        objects: &mut HashMap<String, Vec<u8>>,
        version: &str,
        previous: Option<&str>,
    ) -> ReleaseManifest {
        let version: ReleaseVersion = version.parse().expect("version");
        let notes = serde_json::to_vec(&ReleaseNotes {
            schema: NOTES_SCHEMA,
            version: version.clone(),
            date: Some("2026-11-05".to_owned()),
            notes: BTreeMap::from([("en-US".to_owned(), format!("Notes of {version}."))]),
        })
        .expect("notes");
        let manifest = ReleaseManifest {
            schema: SCHEMA_VERSION,
            product: PRODUCT.to_owned(),
            version: version.clone(),
            platform: PLATFORM,
            install_epoch: 1,
            released_at: "2026-11-05T08:00:00Z".to_owned(),
            notes: Some(NotesRef {
                sha256: Sha256Digest::of(&notes),
            }),
            components: BTreeMap::from([(
                ComponentName::new("core").expect("name"),
                ComponentRef {
                    index: Sha256Digest::of(b"index"),
                    size: 1,
                    fingerprint: None,
                },
            )]),
            downloads: Vec::new(),
            min_setup_version: None,
            previous_version: previous.map(|previous| previous.parse().expect("previous")),
            inno_update: None,
            key_id: "test-key".to_owned(),
        };
        let document = manifest.to_document().expect("document");
        let key = manifest_key(&version, PLATFORM);
        objects.insert(
            signature_key(&key),
            sign_detached(&document, &TEST_SECRET).to_vec(),
        );
        objects.insert(key, document);
        objects.insert(notes_key(&version), notes);
        manifest
    }

    /// Versions of the notes a walk returned, newest first.
    /// 回溯返回的说明所属版本，从新到旧排列。
    fn versions(walk: &SkippedReleaseNotes) -> Vec<String> {
        walk.releases
            .iter()
            .map(|notes| notes.version.to_string())
            .collect()
    }

    #[test]
    fn the_walk_reads_every_skipped_release_down_to_the_installed_one() {
        let mut objects = HashMap::new();
        publish(&mut objects, "0.1.4", None);
        publish(&mut objects, "0.1.5", Some("0.1.4"));
        publish(&mut objects, "0.1.6", Some("0.1.5"));
        let newest = publish(&mut objects, "0.1.7", Some("0.1.6"));
        let mut downloader = Downloader::new(vec![serve(objects)], "test").expect("downloader");
        let keys = test_keys();

        let walk = read_skipped_release_notes(
            &mut downloader,
            &keys,
            &newest,
            &"0.1.4".parse().expect("version"),
        );
        assert_eq!(versions(&walk), ["0.1.6", "0.1.5"]);
        assert!(walk.complete && walk.error.is_none());

        let walk = read_skipped_release_notes(
            &mut downloader,
            &keys,
            &newest,
            &"0.1.6".parse().expect("version"),
        );
        assert!(walk.releases.is_empty() && walk.complete);

        // 0.1.4 names no previous release, so whatever lies below it is unknown.
        // 0.1.4 未写明上一发布，因此其下是否还有版本无从得知。
        let walk = read_skipped_release_notes(
            &mut downloader,
            &keys,
            &newest,
            &"0.1.2".parse().expect("version"),
        );
        assert_eq!(versions(&walk), ["0.1.6", "0.1.5", "0.1.4"]);
        assert!(!walk.complete && walk.error.is_none());
    }

    #[test]
    fn an_unreadable_release_ends_the_walk_without_failing_it() {
        let mut objects = HashMap::new();
        publish(&mut objects, "0.1.5", Some("0.1.4"));
        let newest = publish(&mut objects, "0.1.7", Some("0.1.6"));
        objects.retain(|key, _| !key.contains("0.1.6"));
        let mut downloader = Downloader::new(vec![serve(objects)], "test").expect("downloader");

        let walk = read_skipped_release_notes(
            &mut downloader,
            &test_keys(),
            &newest,
            &"0.1.4".parse().expect("version"),
        );
        assert!(walk.releases.is_empty());
        assert!(!walk.complete && walk.error.is_some());
    }

    #[test]
    fn a_previous_version_must_be_older_than_its_release() {
        let mut objects = HashMap::new();
        let mut manifest = publish(&mut objects, "0.1.5", None);
        manifest.previous_version = Some("0.1.5".parse().expect("version"));
        assert!(manifest.validate().is_err());
    }
}
