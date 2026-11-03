//! Packing a release tree into component indexes and packs.
//! 将发行树打包为组件索引与文件包。

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use serde::Deserialize;
use vulcan_install_core::Sha256Digest;
use vulcan_install_core::index::{ComponentIndex, IndexEntry};
use vulcan_install_core::layout::index_key;
use vulcan_install_core::manifest::ComponentRef;
use vulcan_install_core::names::{ComponentName, SCHEMA_VERSION};
use vulcan_install_core::pack::{FileLocation, PackBuilder, write_object};
use vulcan_install_core::path::RelativePath;

/// Which component each path of the tree belongs to; the longest matching prefix wins.
/// 发行树中每个路径所属的组件；最长匹配前缀优先。
#[derive(Debug)]
pub struct ComponentRules {
    /// Prefixes with their component, longest first.
    /// 前缀及其组件，最长的在前。
    prefixes: Vec<(RelativePath, ComponentName)>,
}

/// The rules file: `{"schema":1,"components":{"<name>":["<path or directory>", ...]}}`.
/// 规则文件：`{"schema":1,"components":{"<组件名>":["<路径或目录>", ...]}}`。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesFile {
    /// Format version.
    /// 格式版本。
    schema: u32,
    /// Paths each component owns, with everything below them.
    /// 各组件拥有的路径及其下的全部内容。
    components: BTreeMap<ComponentName, Vec<String>>,
}

/// What [`pack_tree`] produced.
/// [`pack_tree`] 的产物。
pub struct PackReport {
    /// Index reference of every component that has paths in the tree.
    /// 在发行树中有路径的每个组件的索引引用。
    pub components: BTreeMap<ComponentName, ComponentRef>,
    /// Number of packs written.
    /// 写入的文件包数。
    pub new_packs: usize,
}

/// One path found in the tree.
/// 在发行树中找到的一个路径。
enum TreeItem {
    /// A regular file.
    /// 普通文件。
    File {
        /// Path from the tree root.
        /// 相对树根的路径。
        path: RelativePath,
        /// Unix permission bits.
        /// Unix 权限位。
        mode: u32,
        /// Location on disk.
        /// 磁盘上的位置。
        source: PathBuf,
    },
    /// A symbolic link.
    /// 符号链接。
    Symlink {
        /// Path from the tree root.
        /// 相对树根的路径。
        path: RelativePath,
        /// Link target.
        /// 链接目标。
        target: String,
    },
    /// An empty directory.
    /// 空目录。
    Directory {
        /// Path from the tree root.
        /// 相对树根的路径。
        path: RelativePath,
    },
}

impl TreeItem {
    /// The item's path.
    /// 条目的路径。
    fn path(&self) -> &RelativePath {
        match self {
            Self::File { path, .. } | Self::Symlink { path, .. } | Self::Directory { path } => path,
        }
    }
}

impl ComponentRules {
    /// Reads a rules file.
    /// 读取规则文件。
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let file: RulesFile = serde_json::from_slice(&bytes)
            .with_context(|| format!("{} is not a valid rules file", path.display()))?;
        Self::from_file(file)
    }

    /// Checks a parsed rules file.
    /// 校验已解析的规则文件。
    fn from_file(file: RulesFile) -> anyhow::Result<Self> {
        if file.schema != SCHEMA_VERSION {
            bail!("rules schema {} is not supported", file.schema);
        }
        let mut prefixes = Vec::new();
        for (component, paths) in file.components {
            for path in paths {
                prefixes.push((RelativePath::new(path)?, component.clone()));
            }
        }
        prefixes.sort_by(|left, right| {
            right
                .0
                .as_str()
                .len()
                .cmp(&left.0.as_str().len())
                .then_with(|| left.0.cmp(&right.0))
        });
        if let Some(pair) = prefixes.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            bail!("`{}` is claimed twice", pair[0].0);
        }
        Ok(Self { prefixes })
    }

    /// The component owning `path`.
    /// 拥有 `path` 的组件。
    fn component_of(&self, path: &RelativePath) -> Option<&ComponentName> {
        let path = path.as_str();
        self.prefixes
            .iter()
            .find(|(prefix, _)| {
                let prefix = prefix.as_str();
                path == prefix
                    || path
                        .strip_prefix(prefix)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .map(|(_, component)| component)
    }
}

/// Locations of every file content the index objects in `dir` reference; each object is checked
/// against the digest in its name `<sha256>.json.zst`.
/// `dir` 中索引对象引用的每份文件内容的位置；每个对象都按其文件名 `<sha256>.json.zst` 中的摘要校验。
pub fn published_locations(
    dir: Option<&Path>,
) -> anyhow::Result<HashMap<Sha256Digest, FileLocation>> {
    let mut locations = HashMap::new();
    let Some(dir) = dir else {
        return Ok(locations);
    };
    for child in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let child = child?;
        let name = child.file_name();
        let Some(digest) = name
            .to_str()
            .and_then(|name| name.strip_suffix(".json.zst"))
        else {
            bail!("{} is not an index object", child.path().display());
        };
        let object = std::fs::read(child.path())?;
        let index = ComponentIndex::read_object(&object, digest.parse()?)
            .with_context(|| format!("reading published index {}", child.path().display()))?;
        for entry in index.entries {
            if let IndexEntry::File {
                sha256,
                pack,
                offset,
                length,
                ..
            } = entry
            {
                locations.insert(
                    sha256,
                    FileLocation {
                        pack,
                        offset,
                        length,
                    },
                );
            }
        }
    }
    Ok(locations)
}

/// Packs `tree` below `objects` and writes one index per component.
/// 将 `tree` 打包到 `objects` 之下，并为每个组件写出一个索引。
pub fn pack_tree(
    tree: &Path,
    rules: &ComponentRules,
    published: HashMap<Sha256Digest, FileLocation>,
    objects: &Path,
) -> anyhow::Result<PackReport> {
    let mut items = Vec::new();
    walk(tree, "", &mut items)?;
    let mut owned = Vec::with_capacity(items.len());
    for item in items {
        let Some(component) = rules.component_of(item.path()) else {
            bail!("`{}` belongs to no component", item.path());
        };
        owned.push((component.clone(), item));
    }
    // Packs fill in component, directory, path order (plan section 7).
    // 文件包按组件、目录、路径的顺序填充（方案第七节）。
    owned.sort_by(|(left_component, left), (right_component, right)| {
        let parent = |path: &RelativePath| {
            path.as_str()
                .rsplit_once('/')
                .map_or("", |(parent, _)| parent)
                .to_owned()
        };
        left_component
            .cmp(right_component)
            .then_with(|| parent(left.path()).cmp(&parent(right.path())))
            .then_with(|| left.path().cmp(right.path()))
    });

    let mut builder = PackBuilder::new(objects, published);
    let mut contents = Vec::with_capacity(owned.len());
    for (_, item) in &owned {
        if let TreeItem::File { source, .. } = item {
            let content =
                std::fs::read(source).with_context(|| format!("reading {}", source.display()))?;
            contents.push((content.len() as u64, builder.add(&content)?));
        }
    }
    let outcome = builder.finish()?;

    let mut entries: BTreeMap<ComponentName, Vec<IndexEntry>> = BTreeMap::new();
    let mut contents = contents.into_iter();
    for (component, item) in owned {
        let entry = match item {
            TreeItem::File { path, mode, .. } => {
                let Some((size, sha256)) = contents.next() else {
                    bail!("packed contents ran out before `{path}`");
                };
                IndexEntry::file(path, mode, size, sha256, outcome.locations[&sha256])
            }
            TreeItem::Symlink { path, target } => IndexEntry::Symlink { path, target },
            TreeItem::Directory { path } => IndexEntry::Directory { path },
        };
        entries.entry(component).or_default().push(entry);
    }
    let mut components = BTreeMap::new();
    for (component, entries) in entries {
        let index = ComponentIndex::new(component.clone(), entries)?;
        let (digest, object) = index.to_object()?;
        write_object(objects, &index_key(digest), &object)?;
        components.insert(
            component,
            ComponentRef {
                index: digest,
                size: index.installed_size(),
                fingerprint: None,
            },
        );
    }
    Ok(PackReport {
        components,
        new_packs: outcome.new_packs.len(),
    })
}

/// Collects the files, links and empty directories below `dir`.
/// 收集 `dir` 之下的文件、链接与空目录。
fn walk(dir: &Path, prefix: &str, items: &mut Vec<TreeItem>) -> anyhow::Result<()> {
    let mut children = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        let Some(name) = child.file_name().to_str().map(str::to_owned) else {
            bail!("{} has a name that is not UTF-8", child.path().display());
        };
        let path = RelativePath::new(format!("{prefix}{name}"))?;
        let file_type = child.file_type()?;
        if file_type.is_symlink() {
            if cfg!(windows) {
                bail!("`{path}` is a symbolic link, which Windows trees must not contain");
            }
            let target = std::fs::read_link(child.path())?;
            let Some(target) = target.to_str() else {
                bail!("`{path}` links to a target that is not UTF-8");
            };
            items.push(TreeItem::Symlink {
                path,
                target: target.to_owned(),
            });
        } else if file_type.is_dir() {
            let before = items.len();
            walk(&child.path(), &format!("{path}/"), items)?;
            if items.len() == before {
                items.push(TreeItem::Directory { path });
            }
        } else {
            let metadata = child.metadata()?;
            items.push(TreeItem::File {
                path,
                mode: mode_of(&metadata),
                source: child.path(),
            });
        }
    }
    Ok(())
}

/// Unix permission bits of a tree file; Windows trees carry plain file modes.
/// 发行树文件的 Unix 权限位；Windows 发行树使用普通文件权限。
fn mode_of(metadata: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()) & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0o644
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(json: &str) -> anyhow::Result<ComponentRules> {
        ComponentRules::from_file(serde_json::from_str(json)?)
    }

    #[test]
    fn the_longest_prefix_owns_a_path() {
        let rules = rules(
            r#"{"schema":1,"components":{"resources":["resources"],"desktop-app":["resources/app"]}}"#,
        )
        .expect("rules");
        let owner = |path: &str| {
            rules
                .component_of(&RelativePath::new(path).expect("path"))
                .map(ComponentName::as_str)
        };
        assert_eq!(owner("resources/app/main.js"), Some("desktop-app"));
        assert_eq!(owner("resources/app"), Some("desktop-app"));
        assert_eq!(owner("resources/application.json"), Some("resources"));
        assert_eq!(owner("bin/tool"), None);
    }

    #[test]
    fn rules_reject_claims_on_one_path_and_unknown_fields() {
        assert!(rules(r#"{"schema":1,"components":{"a":["bin"],"b":["bin"]}}"#).is_err());
        assert!(rules(r#"{"schema":2,"components":{}}"#).is_err());
        assert!(rules(r#"{"schema":1,"components":{},"extra":1}"#).is_err());
    }
}
