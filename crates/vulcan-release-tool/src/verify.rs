//! Checking a local object tree the way a client does, ending with a real install from it.
//! 按客户端的方式校验本地对象树，最后从中真实安装一次。

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use vulcan_install_core::assemble::{Assembly, DirectoryPackSource};
use vulcan_install_core::index::ComponentIndex;
use vulcan_install_core::install_root::InstallRoot;
use vulcan_install_core::layout::{
    index_key, installer_key, manifest_key, notes_key, pointer_key, signature_key,
};
use vulcan_install_core::limits::MAX_NOTES_BYTES;
use vulcan_install_core::manifest::ReleaseManifest;
use vulcan_install_core::names::{Channel, PRODUCT, Platform, ReleaseVersion, SCHEMA_VERSION};
use vulcan_install_core::pointer::{ChannelPointer, DocumentRef};
use vulcan_install_core::signing::TrustedKey;

use crate::publish::{hash_file, read_object};

/// Verifies the manifest of `version` (through the channel pointer when `channel` is given), every
/// index, the notes and downloads, then installs all components into the empty `install_root`.
/// 校验 `version` 的版本清单（给出 `channel` 时经通道指针校验）、每个索引、更新说明与下载文件，再将全部
/// 组件安装到空的 `install_root` 中。
pub fn verify_release(
    objects: &Path,
    platform: Platform,
    version: &ReleaseVersion,
    channel: Option<Channel>,
    install_root: &Path,
    keys: &[TrustedKey],
) -> anyhow::Result<PathBuf> {
    let document_key = manifest_key(version, platform);
    let document = read_object(objects, &document_key)?;
    let signature = read_object(objects, &signature_key(&document_key))?;
    let pointer = match channel {
        Some(channel) => {
            let key = pointer_key(channel, platform);
            let pointer = ChannelPointer::verify(
                &read_object(objects, &key)?,
                &read_object(objects, &signature_key(&key))?,
                keys,
                channel,
                platform,
            )?;
            if &pointer.version != version {
                bail!(
                    "the {channel} pointer names {} instead of {version}",
                    pointer.version
                );
            }
            pointer
        }
        // Before promotion no pointer exists; the manifest signature is still checked.
        // 发布前尚无指针；版本清单的签名仍会校验。
        None => ChannelPointer {
            schema: SCHEMA_VERSION,
            product: PRODUCT.to_owned(),
            channel: if version.is_prerelease() {
                Channel::Preview
            } else {
                Channel::Stable
            },
            platform,
            version: version.clone(),
            manifest: DocumentRef::of(&document),
            key_id: String::new(),
        },
    };
    let manifest = ReleaseManifest::verify(&document, &signature, keys, &pointer)?;

    if let Some(notes) = &manifest.notes {
        let bytes = read_object(objects, &notes_key(version))?;
        if bytes.len() as u64 > MAX_NOTES_BYTES {
            bail!("the release notes exceed {MAX_NOTES_BYTES} bytes");
        }
        notes.sha256.expect_eq(
            "release notes",
            vulcan_install_core::Sha256Digest::of(&bytes),
        )?;
    }
    for download in &manifest.downloads {
        let (size, sha256) = hash_file(&objects.join(installer_key(version, &download.file)))?;
        if size != download.size || sha256 != download.sha256 {
            bail!("download `{}` differs from the manifest", download.file);
        }
    }
    let indexes = read_indexes(objects, &manifest)?;

    let root = InstallRoot::new(install_root);
    let lock = root.lock()?;
    let installed = Assembly::new(&root, &lock, &manifest, indexes)?
        .prepare()?
        .complete(&mut DirectoryPackSource::new(objects))?;
    root.switch_to(&lock, version)?;
    Ok(installed)
}

/// Reads the signed manifest of a published `version` and the indexes it names, each checked
/// against its digest and size; the manifest is checked by its own signature, without a pointer.
/// 读取已发布 `version` 的签名版本清单及其列出的索引，每个索引按摘要与大小校验；版本清单只按自身签名
/// 校验，不经指针。
pub fn published_release(
    objects: &Path,
    platform: Platform,
    version: &ReleaseVersion,
    keys: &[TrustedKey],
) -> anyhow::Result<(ReleaseManifest, Vec<ComponentIndex>)> {
    let document_key = manifest_key(version, platform);
    let document = read_object(objects, &document_key)?;
    let signature = read_object(objects, &signature_key(&document_key))?;
    let pointer = ChannelPointer {
        schema: SCHEMA_VERSION,
        product: PRODUCT.to_owned(),
        channel: Channel::Preview,
        platform,
        version: version.clone(),
        manifest: DocumentRef::of(&document),
        key_id: String::new(),
    };
    let manifest = ReleaseManifest::verify(&document, &signature, keys, &pointer)?;
    let indexes = read_indexes(objects, &manifest)?;
    Ok((manifest, indexes))
}

/// Reads every index `manifest` names, checked against its digest and size.
/// 读取 `manifest` 列出的每个索引，并按摘要与大小校验。
fn read_indexes(objects: &Path, manifest: &ReleaseManifest) -> anyhow::Result<Vec<ComponentIndex>> {
    let mut indexes = Vec::with_capacity(manifest.components.len());
    for (component, reference) in &manifest.components {
        let object = read_object(objects, &index_key(reference.index))?;
        let index = ComponentIndex::from_object(&object, reference.index, component)
            .with_context(|| format!("reading the `{component}` index"))?;
        if index.installed_size() != reference.size {
            bail!("the `{component}` size differs from its index");
        }
        indexes.push(index);
    }
    Ok(indexes)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;

    use vulcan_install_core::record::InstallRecord;
    use vulcan_install_core::signing::public_key_of;

    use super::*;
    use crate::pack::{ComponentRules, pack_tree, published_locations};
    use crate::publish::{ManifestInput, write_manifest, write_pointer};

    pub(crate) const SECRET: [u8; 32] = [9; 32];

    pub(crate) fn keys() -> Vec<TrustedKey> {
        vec![TrustedKey {
            key_id: "test-key",
            public_key: public_key_of(&SECRET),
        }]
    }

    pub(crate) fn write(root: &Path, path: &str, content: &[u8]) {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().expect("parent")).expect("dirs");
        std::fs::write(target, content).expect("write");
    }

    /// Packs, signs and promotes one version of `tree`, as the release workflow does.
    /// 像发布工作流那样，对 `tree` 的一个版本打包、签名并推进通道。
    pub(crate) fn release(temp: &Path, tree: &Path, version: &str) -> ReleaseVersion {
        let objects = temp.join("objects");
        let platform = Platform::LinuxX64;
        let rules = temp.join("rules.json");
        std::fs::write(
            &rules,
            r#"{"schema":1,"components":{"service":["bin"],"webui":["webui"],"config":["config"]}}"#,
        )
        .expect("rules");
        let published = if objects.join("indexes").is_dir() {
            published_locations(Some(&objects.join("indexes"))).expect("published")
        } else {
            HashMap::new()
        };
        let report = pack_tree(
            tree,
            &ComponentRules::read(&rules).expect("rules"),
            published,
            &objects,
        )
        .expect("pack");
        let notes = temp.join("notes.json");
        std::fs::write(&notes, br#"{"en":"Fixes."}"#).expect("notes");
        let version: ReleaseVersion = version.parse().expect("version");
        write_manifest(
            &objects,
            ManifestInput {
                version: version.clone(),
                platform,
                install_epoch: 1,
                released_at: "2026-11-03T08:00:00Z".to_owned(),
                components: report.components,
                notes: Some(notes),
                downloads: Vec::new(),
                min_setup_version: None,
            },
            &SECRET,
            &keys(),
        )
        .expect("manifest");
        write_pointer(
            &objects,
            Channel::Stable,
            platform,
            &version,
            &SECRET,
            &keys(),
        )
        .expect("pointer");
        version
    }

    #[test]
    fn a_packed_and_signed_release_installs_and_the_next_one_reuses_packs() {
        let temp = tempfile::tempdir().expect("temp dir");
        let tree = temp.path().join("tree");
        write(&tree, "bin/service", &[1; 5_000]);
        write(&tree, "webui/index.html", b"<html></html>");
        write(&tree, "webui/assets/app.js", b"console.log(1)");
        std::fs::create_dir_all(tree.join("config/empty")).expect("empty dir");
        let objects = temp.path().join("objects");

        let first = release(temp.path(), &tree, "0.1.3");
        let installed = verify_release(
            &objects,
            Platform::LinuxX64,
            &first,
            Some(Channel::Stable),
            &temp.path().join("install-a"),
            &keys(),
        )
        .expect("verify");
        assert_eq!(
            std::fs::read(installed.join("webui").join("index.html")).expect("read"),
            b"<html></html>"
        );
        assert!(installed.join("config").join("empty").is_dir());
        assert_eq!(
            InstallRecord::read(&installed)
                .expect("record")
                .components
                .len(),
            3
        );

        let packs = |objects: &Path| {
            std::fs::read_dir(objects.join("packs"))
                .expect("packs")
                .count()
        };
        let before = packs(&objects);
        write(&tree, "webui/index.html", b"<html>2</html>");
        let second = release(temp.path(), &tree, "0.1.4");
        assert_eq!(
            packs(&objects),
            before + 1,
            "only the changed file is packed"
        );
        verify_release(
            &objects,
            Platform::LinuxX64,
            &second,
            None,
            &temp.path().join("install-b"),
            &keys(),
        )
        .expect("verify second");
    }

    #[test]
    fn verification_rejects_a_tampered_index_and_unowned_paths() {
        let temp = tempfile::tempdir().expect("temp dir");
        let tree = temp.path().join("tree");
        write(&tree, "bin/service", b"service");
        let version = release(temp.path(), &tree, "0.1.3");
        let objects = temp.path().join("objects");
        for index in std::fs::read_dir(objects.join("indexes")).expect("indexes") {
            let path = index.expect("entry").path();
            let mut bytes = std::fs::read(&path).expect("read");
            bytes[0] ^= 0xFF;
            std::fs::write(&path, bytes).expect("tamper");
        }
        assert!(
            verify_release(
                &objects,
                Platform::LinuxX64,
                &version,
                None,
                &temp.path().join("install"),
                &keys(),
            )
            .is_err()
        );

        write(&tree, "stray/file", b"unowned");
        let rules = ComponentRules::read(&temp.path().join("rules.json")).expect("rules");
        assert!(pack_tree(&tree, &rules, HashMap::new(), &temp.path().join("other")).is_err());
    }

    #[test]
    fn only_a_trusted_key_signs() {
        let temp = tempfile::tempdir().expect("temp dir");
        let error = write_manifest(
            temp.path(),
            ManifestInput {
                version: "0.1.3".parse().expect("version"),
                platform: Platform::LinuxX64,
                install_epoch: 1,
                released_at: "2026-11-03T08:00:00Z".to_owned(),
                components: Default::default(),
                notes: None,
                downloads: Vec::new(),
                min_setup_version: None,
            },
            &[1; 32],
            &keys(),
        );
        assert!(error.is_err());
        assert!(!temp.path().join("releases").exists());
    }
}
