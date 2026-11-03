//! Writing the install record into a packed tree that an installer then copies as is.
//! 向已打包的发行树写入安装记录，之后由安装包原样复制。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, bail};
use vulcan_install_core::index::{ComponentIndex, IndexEntry};
use vulcan_install_core::layout::index_key;
use vulcan_install_core::manifest::ComponentRef;
use vulcan_install_core::names::{
    ComponentName, PRODUCT, Platform, ReleaseVersion, SCHEMA_VERSION,
};
use vulcan_install_core::record::{InstallRecord, RecordEntry};

use crate::publish::{hash_file, read_object};

/// Writes `.vulcan-install.json` into `tree` from the component indexes `pack` wrote below
/// `objects`, after checking that every indexed file in `tree` still holds the packed bytes; returns
/// the number of entries.
///
/// Installers copy the tree with times of their own, so every file is recorded with time zero: the
/// updater then hashes each file once before reusing it.
///
/// 依据 `pack` 写在 `objects` 之下的组件索引，向 `tree` 写入 `.vulcan-install.json`；写入前检查发行树中
/// 每个已索引文件仍是打包时的字节；返回条目数。
///
/// 安装包复制文件时使用自己的时间，因此每个文件的时间都记为零：更新器复用文件前会先计算一次哈希。
pub fn write_record(
    objects: &Path,
    components: &BTreeMap<ComponentName, ComponentRef>,
    version: &ReleaseVersion,
    platform: Platform,
    install_epoch: u32,
    tree: &Path,
) -> anyhow::Result<usize> {
    let mut entries = Vec::new();
    for (component, reference) in components {
        let object = read_object(objects, &index_key(reference.index))?;
        let index = ComponentIndex::from_object(&object, reference.index, component)
            .with_context(|| format!("reading the `{component}` index"))?;
        for entry in index.entries {
            entries.push(match entry {
                IndexEntry::File {
                    path, size, sha256, ..
                } => {
                    let on_disk = hash_file(&tree.join(path.as_str()))?;
                    if on_disk != (size, sha256) {
                        bail!("`{path}` changed after it was packed");
                    }
                    RecordEntry::File {
                        path,
                        size,
                        sha256,
                        modified_ns: 0,
                    }
                }
                IndexEntry::Symlink { path, target } => RecordEntry::Symlink { path, target },
                IndexEntry::Directory { path } => RecordEntry::Directory { path },
            });
        }
    }
    entries.sort_by(|left, right| left.path().cmp(right.path()));
    let count = entries.len();
    InstallRecord {
        schema: SCHEMA_VERSION,
        product: PRODUCT.to_owned(),
        version: version.clone(),
        platform,
        install_epoch,
        components: components
            .iter()
            .map(|(name, reference)| (name.clone(), reference.index))
            .collect(),
        entries,
    }
    .write(tree)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::pack::{ComponentRules, pack_tree};

    /// Packs a small tree with two components and returns its component references.
    /// 打包一个含两个组件的小发行树，并返回其组件引用。
    fn packed(temp: &Path) -> BTreeMap<ComponentName, ComponentRef> {
        let tree = temp.join("tree");
        std::fs::create_dir_all(tree.join("bin")).expect("bin");
        std::fs::create_dir_all(tree.join("webui/empty")).expect("webui");
        std::fs::write(tree.join("bin/service"), [7; 3_000]).expect("service");
        std::fs::write(tree.join("webui/index.html"), b"<html></html>").expect("index");
        let rules = temp.join("rules.json");
        std::fs::write(
            &rules,
            r#"{"schema":1,"components":{"core":["bin"],"webui":["webui"]}}"#,
        )
        .expect("rules");
        pack_tree(
            &tree,
            &ComponentRules::read(&rules).expect("rules"),
            HashMap::new(),
            &temp.join("objects"),
        )
        .expect("pack")
        .components
    }

    #[test]
    fn the_record_lists_every_indexed_path_with_time_zero() {
        let temp = tempfile::tempdir().expect("temp dir");
        let components = packed(temp.path());
        let tree = temp.path().join("tree");
        let version: ReleaseVersion = "1.2.3".parse().expect("version");
        let count = write_record(
            &temp.path().join("objects"),
            &components,
            &version,
            Platform::LinuxX64,
            1,
            &tree,
        )
        .expect("record");

        let record = InstallRecord::read(&tree).expect("read");
        assert_eq!(count, record.entries.len());
        assert_eq!(record.version, version);
        assert_eq!(record.components.len(), components.len());
        let paths: Vec<_> = record
            .entries
            .iter()
            .map(|entry| entry.path().as_str())
            .collect();
        assert_eq!(paths, ["bin/service", "webui/empty", "webui/index.html"]);
        assert!(record.entries.iter().all(|entry| match entry {
            RecordEntry::File { modified_ns, .. } => *modified_ns == 0,
            _ => true,
        }));
    }

    #[test]
    fn a_file_changed_after_packing_is_rejected() {
        let temp = tempfile::tempdir().expect("temp dir");
        let components = packed(temp.path());
        let tree = temp.path().join("tree");
        std::fs::write(tree.join("bin/service"), [8; 3_000]).expect("change");
        let result = write_record(
            &temp.path().join("objects"),
            &components,
            &"1.2.3".parse().expect("version"),
            Platform::LinuxX64,
            1,
            &tree,
        );
        assert!(result.is_err());
        assert!(!tree.join(".vulcan-install.json").exists());
    }
}
