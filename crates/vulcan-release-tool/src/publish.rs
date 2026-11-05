//! Writing and signing release manifests and channel pointers.
//! 写入并签署版本清单与通道指针。

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use sha2::{Digest as _, Sha256};
use vulcan_install_core::Sha256Digest;
use vulcan_install_core::layout::{
    installer_key, manifest_key, notes_key, pointer_key, signature_key,
};
use vulcan_install_core::limits::MAX_NOTES_BYTES;
use vulcan_install_core::manifest::{
    ComponentRef, Download, DownloadKind, NotesRef, ReleaseManifest,
};
use vulcan_install_core::names::{
    Channel, ComponentName, PRODUCT, Platform, ReleaseVersion, SCHEMA_VERSION,
};
use vulcan_install_core::pack::write_object;
use vulcan_install_core::pointer::{ChannelPointer, DocumentRef};
use vulcan_install_core::signing::{TrustedKey, sign_detached, trusted_key_of};

/// Everything a release manifest is made from.
/// 构成版本清单的全部内容。
pub struct ManifestInput {
    /// The version.
    /// 版本。
    pub version: ReleaseVersion,
    /// The platform.
    /// 平台。
    pub platform: Platform,
    /// The install epoch.
    /// 安装纪元。
    pub install_epoch: u32,
    /// Release time as `YYYY-MM-DDTHH:MM:SSZ`.
    /// 发布时间，格式为 `YYYY-MM-DDTHH:MM:SSZ`。
    pub released_at: String,
    /// Components as `pack` reported them.
    /// `pack` 报告的组件。
    pub components: BTreeMap<ComponentName, ComponentRef>,
    /// The release notes document, if any.
    /// 更新说明文档（若有）。
    pub notes: Option<PathBuf>,
    /// Separately downloadable files.
    /// 可单独下载的文件。
    pub downloads: Vec<(DownloadKind, PathBuf)>,
    /// Oldest setup program that can install this version.
    /// 能安装此版本的最旧安装器版本。
    pub min_setup_version: Option<ReleaseVersion>,
    /// The release published before this one, from the release plan.
    /// 在此之前发布的版本，取自发布计划。
    pub previous_version: Option<ReleaseVersion>,
}

/// Copies the notes and downloads below `objects`, then writes and signs the manifest.
/// 将更新说明与下载文件复制到 `objects` 之下，再写入并签署版本清单。
pub fn write_manifest(
    objects: &Path,
    input: ManifestInput,
    secret: &[u8; 32],
    keys: &[TrustedKey],
) -> anyhow::Result<ReleaseManifest> {
    let key = trusted_key_of(secret, keys)?;
    let notes = match &input.notes {
        Some(path) => {
            let bytes =
                std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            if bytes.len() as u64 > MAX_NOTES_BYTES {
                bail!("the release notes exceed {MAX_NOTES_BYTES} bytes");
            }
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .context("the release notes are not JSON")?;
            write_object(objects, &notes_key(&input.version), &bytes)?;
            Some(NotesRef {
                sha256: Sha256Digest::of(&bytes),
            })
        }
        None => None,
    };
    let mut downloads = Vec::with_capacity(input.downloads.len());
    for (kind, path) in &input.downloads {
        let Some(file) = path.file_name().and_then(|name| name.to_str()) else {
            bail!("{} has no usable file name", path.display());
        };
        let target = objects.join(installer_key(&input.version, file));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(path, &target).with_context(|| format!("copying {}", path.display()))?;
        let (size, sha256) = hash_file(&target)?;
        downloads.push(Download {
            kind: *kind,
            file: file.to_owned(),
            sha256,
            size,
        });
    }
    let manifest = ReleaseManifest {
        schema: SCHEMA_VERSION,
        product: PRODUCT.to_owned(),
        version: input.version,
        platform: input.platform,
        install_epoch: input.install_epoch,
        released_at: input.released_at,
        notes,
        components: input.components,
        downloads,
        min_setup_version: input.min_setup_version,
        previous_version: input.previous_version,
        key_id: key.key_id.to_owned(),
    };
    let document = manifest.to_document()?;
    let document_key = manifest_key(&manifest.version, manifest.platform);
    write_object(objects, &document_key, &document)?;
    write_object(
        objects,
        &signature_key(&document_key),
        &sign_detached(&document, secret),
    )?;
    Ok(manifest)
}

/// Signs a pointer to the manifest already below `objects`, after verifying that manifest.
/// 校验 `objects` 之下已有的版本清单后，签署指向它的通道指针。
pub fn write_pointer(
    objects: &Path,
    channel: Channel,
    platform: Platform,
    version: &ReleaseVersion,
    secret: &[u8; 32],
    keys: &[TrustedKey],
) -> anyhow::Result<ChannelPointer> {
    let key = trusted_key_of(secret, keys)?;
    let document_key = manifest_key(version, platform);
    let document = read_object(objects, &document_key)?;
    let signature = read_object(objects, &signature_key(&document_key))?;
    let pointer = ChannelPointer {
        schema: SCHEMA_VERSION,
        product: PRODUCT.to_owned(),
        channel,
        platform,
        version: version.clone(),
        manifest: DocumentRef::of(&document),
        key_id: key.key_id.to_owned(),
    };
    ReleaseManifest::verify(&document, &signature, keys, &pointer)
        .context("the manifest the pointer would name does not verify")?;
    let pointer_document = pointer.to_document()?;
    let pointer_key = pointer_key(channel, platform);
    write_object(objects, &pointer_key, &pointer_document)?;
    write_object(
        objects,
        &signature_key(&pointer_key),
        &sign_detached(&pointer_document, secret),
    )?;
    Ok(pointer)
}

/// Reads an object below `objects`.
/// 读取 `objects` 之下的对象。
pub fn read_object(objects: &Path, key: &str) -> anyhow::Result<Vec<u8>> {
    let path = objects.join(key);
    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))
}

/// Size and SHA-256 of a file.
/// 文件的大小与 SHA-256。
pub fn hash_file(path: &Path) -> anyhow::Result<(u64, Sha256Digest)> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("reading {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 << 10];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok((size, Sha256Digest::finish(hasher)));
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
}
