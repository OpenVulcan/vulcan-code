//! Component indexes `indexes/<sha256>.json.zst`: every path of one component and where its bytes are.
//! 组件索引 `indexes/<sha256>.json.zst`：某组件的每个路径及其字节所在位置。
//!
//! The digest in the object name and in the manifest covers the compressed bytes, like a pack's.
//! 对象名与版本清单中的摘要覆盖压缩后的字节，与文件包相同。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::digest::Sha256Digest;
use crate::error::{FormatError, FormatResult};
use crate::limits::{COMPRESSION_LEVEL, MAX_EXPANDED_INDEX_BYTES, MAX_INDEX_BYTES, MAX_PACK_BYTES};
use crate::names::{ComponentName, SCHEMA_VERSION};
use crate::pack::{FileLocation, compress_frame, decompress_bounded};
use crate::path::RelativePath;
use crate::record::RECORD_FILE_NAME;

/// Largest permission value an entry may carry; only the Unix permission bits are recorded.
/// 条目可携带的最大权限值；只记录 Unix 权限位。
const MAX_MODE: u32 = 0o777;

/// One path of a component.
/// 组件中的一个路径。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum IndexEntry {
    /// A regular file stored as one zstd frame inside a pack.
    /// 以一个 zstd 帧存放在文件包内的普通文件。
    File {
        /// Path from the version root.
        /// 相对版本根的路径。
        path: RelativePath,
        /// Unix permission bits; Linux and macOS apply the executable bits.
        /// Unix 权限位；Linux 与 macOS 应用其中的可执行位。
        mode: u32,
        /// Size of the file.
        /// 文件大小。
        size: u64,
        /// SHA-256 of the file.
        /// 文件的 SHA-256。
        sha256: Sha256Digest,
        /// Digest of the pack holding the frame.
        /// 存放该帧的文件包摘要。
        pack: Sha256Digest,
        /// Offset of the frame in the pack.
        /// 帧在文件包中的偏移。
        offset: u64,
        /// Length of the frame.
        /// 帧的长度。
        length: u64,
    },
    /// A symbolic link that stays inside the version root.
    /// 不越出版本根的符号链接。
    Symlink {
        /// Path from the version root.
        /// 相对版本根的路径。
        path: RelativePath,
        /// Link target, relative to the link's directory.
        /// 链接目标，相对于链接所在目录。
        target: String,
    },
    /// A directory that must exist even when empty.
    /// 即使为空也必须存在的目录。
    Directory {
        /// Path from the version root.
        /// 相对版本根的路径。
        path: RelativePath,
    },
}

impl IndexEntry {
    /// Describes a file whose frame sits at `location`.
    /// 描述其帧位于 `location` 的文件。
    pub fn file(
        path: RelativePath,
        mode: u32,
        size: u64,
        sha256: Sha256Digest,
        location: FileLocation,
    ) -> Self {
        Self::File {
            path,
            mode,
            size,
            sha256,
            pack: location.pack,
            offset: location.offset,
            length: location.length,
        }
    }

    /// The entry's path.
    /// 条目的路径。
    pub fn path(&self) -> &RelativePath {
        match self {
            Self::File { path, .. } | Self::Symlink { path, .. } | Self::Directory { path } => path,
        }
    }

    /// Checks the rules of one entry.
    /// 校验单个条目的规则。
    fn validate(&self) -> FormatResult<()> {
        match self {
            Self::File {
                path,
                mode,
                offset,
                length,
                ..
            } => {
                if *mode > MAX_MODE {
                    return Err(FormatError::invalid(
                        "component index",
                        format!("`{path}` mode {mode:o} has bits beyond {MAX_MODE:o}"),
                    ));
                }
                let end = offset.checked_add(*length);
                if *length == 0 || end.is_none_or(|end| end > MAX_PACK_BYTES) {
                    return Err(FormatError::invalid(
                        "component index",
                        format!("`{path}` frame {offset}+{length} is outside its pack"),
                    ));
                }
                Ok(())
            }
            Self::Symlink { path, target } => path.check_symlink_target(target),
            Self::Directory { .. } => Ok(()),
        }
    }
}

/// Checks entries already in path order as one tree: each entry valid, paths unique and distinct on
/// case-insensitive file systems, none reserved, and none below a file or symbolic link.
/// 将已按路径排序的条目作为一棵树校验：每个条目有效，路径唯一且在大小写不敏感的文件系统上互不相同，
/// 不使用保留路径，且没有条目位于文件或符号链接之下。
pub(crate) fn check_entry_set(entries: &[&IndexEntry]) -> FormatResult<()> {
    let invalid = |reason: String| Err(FormatError::invalid("component index", reason));
    let mut folded = HashSet::with_capacity(entries.len());
    let mut leaves = HashSet::new();
    for (position, entry) in entries.iter().enumerate() {
        entry.validate()?;
        let path = entry.path();
        if path.as_str() == RECORD_FILE_NAME {
            return invalid(format!("`{path}` is reserved for the install record"));
        }
        if position > 0 && entries[position - 1].path() >= path {
            return invalid(format!("`{path}` is out of order or repeated"));
        }
        if !folded.insert(path.case_folded()) {
            return invalid(format!(
                "`{path}` collides with another path on case-insensitive file systems"
            ));
        }
        if !matches!(entry, IndexEntry::Directory { .. }) {
            leaves.insert(path.as_str());
        }
    }
    for entry in entries {
        if let Some(parent) = entry
            .path()
            .ancestors()
            .find(|ancestor| leaves.contains(ancestor))
        {
            return invalid(format!(
                "`{}` lies below the file or link `{parent}`",
                entry.path()
            ));
        }
    }
    Ok(())
}

/// Every path of one component.
/// 某组件的全部路径。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentIndex {
    /// Format version; only [`SCHEMA_VERSION`] is accepted.
    /// 格式版本；只接受 [`SCHEMA_VERSION`]。
    pub schema: u32,
    /// The component.
    /// 组件。
    pub component: ComponentName,
    /// Entries in byte order of their paths.
    /// 按路径字节序排列的条目。
    pub entries: Vec<IndexEntry>,
}

impl ComponentIndex {
    /// Builds an index from entries in any order.
    /// 用任意顺序的条目构建索引。
    pub fn new(component: ComponentName, mut entries: Vec<IndexEntry>) -> FormatResult<Self> {
        entries.sort_by(|left, right| left.path().cmp(right.path()));
        let index = Self {
            schema: SCHEMA_VERSION,
            component,
            entries,
        };
        index.validate()?;
        Ok(index)
    }

    /// Checks the index rules: sorted unique paths that stay distinct on case-insensitive file
    /// systems, and no entry below a file or symbolic link.
    /// 校验索引规则：路径有序且唯一，在大小写不敏感的文件系统上仍互不相同，且没有条目位于文件或符号链接
    /// 之下。
    pub fn validate(&self) -> FormatResult<()> {
        let invalid = |reason: String| Err(FormatError::invalid("component index", reason));
        if self.schema != SCHEMA_VERSION {
            return invalid(format!("unsupported schema {}", self.schema));
        }
        check_entry_set(&self.entries.iter().collect::<Vec<_>>())
    }

    /// Total size of the component's files.
    /// 组件文件的总大小。
    pub fn installed_size(&self) -> u64 {
        self.entries
            .iter()
            .map(|entry| match entry {
                IndexEntry::File { size, .. } => *size,
                IndexEntry::Symlink { .. } | IndexEntry::Directory { .. } => 0,
            })
            .sum()
    }

    /// Validates, serializes and compresses the index; returns the object digest and bytes.
    /// 校验、序列化并压缩索引；返回对象摘要与字节。
    pub fn to_object(&self) -> FormatResult<(Sha256Digest, Vec<u8>)> {
        self.validate()?;
        let json = serde_json::to_vec(self).map_err(|source| FormatError::Json {
            what: "component index",
            source,
        })?;
        if json.len() as u64 > MAX_EXPANDED_INDEX_BYTES {
            return Err(FormatError::TooLarge {
                what: "expanded component index",
                limit: MAX_EXPANDED_INDEX_BYTES,
            });
        }
        let compressed = compress_frame(&json, COMPRESSION_LEVEL)?;
        if compressed.len() as u64 > MAX_INDEX_BYTES {
            return Err(FormatError::TooLarge {
                what: "component index",
                limit: MAX_INDEX_BYTES,
            });
        }
        Ok((Sha256Digest::of(&compressed), compressed))
    }

    /// Verifies and reads a downloaded index object of the expected component.
    /// 校验并读取下载的、属于期望组件的索引对象。
    pub fn from_object(
        object: &[u8],
        digest: Sha256Digest,
        component: &ComponentName,
    ) -> FormatResult<Self> {
        let index = Self::read_object(object, digest)?;
        if &index.component != component {
            return Err(FormatError::invalid(
                "component index",
                format!("describes `{}` instead of `{component}`", index.component),
            ));
        }
        Ok(index)
    }

    /// Verifies and reads an index object of any component, as the release tooling does with every
    /// published index.
    /// 校验并读取任意组件的索引对象，发布工具以此读取每个已发布索引。
    pub fn read_object(object: &[u8], digest: Sha256Digest) -> FormatResult<Self> {
        if object.len() as u64 > MAX_INDEX_BYTES {
            return Err(FormatError::TooLarge {
                what: "component index",
                limit: MAX_INDEX_BYTES,
            });
        }
        digest.expect_eq("component index", Sha256Digest::of(object))?;
        let json = decompress_bounded("component index", object, MAX_EXPANDED_INDEX_BYTES)?;
        let index: Self = serde_json::from_slice(&json).map_err(|source| FormatError::Json {
            what: "component index",
            source,
        })?;
        index.validate()?;
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> RelativePath {
        RelativePath::new(text).expect("path")
    }

    fn file(text: &str) -> IndexEntry {
        IndexEntry::file(
            path(text),
            0o644,
            3,
            Sha256Digest::of(text.as_bytes()),
            FileLocation {
                pack: Sha256Digest::of(b"pack"),
                offset: 0,
                length: 12,
            },
        )
    }

    fn component() -> ComponentName {
        ComponentName::new("webui").expect("component")
    }

    #[test]
    fn an_index_object_round_trips_and_checks_digest_and_component() {
        let index = ComponentIndex::new(
            component(),
            vec![
                file("webui/index.html"),
                IndexEntry::Directory {
                    path: path("webui/empty"),
                },
                IndexEntry::Symlink {
                    path: path("webui/link"),
                    target: "index.html".to_owned(),
                },
                file("webui/assets/a.js"),
            ],
        )
        .expect("index");
        assert_eq!(index.installed_size(), 6);
        let (digest, object) = index.to_object().expect("object");
        assert_eq!(
            ComponentIndex::from_object(&object, digest, &component()).expect("read"),
            index
        );
        assert!(
            ComponentIndex::from_object(&object, Sha256Digest::of(b"x"), &component()).is_err()
        );
        let other = ComponentName::new("core").expect("component");
        assert!(ComponentIndex::from_object(&object, digest, &other).is_err());
    }

    #[test]
    fn repeated_and_case_colliding_paths_are_rejected() {
        assert!(ComponentIndex::new(component(), vec![file("a"), file("a")]).is_err());
        assert!(ComponentIndex::new(component(), vec![file("Readme"), file("README")]).is_err());
    }

    #[test]
    fn entries_below_a_file_or_link_are_rejected() {
        assert!(ComponentIndex::new(component(), vec![file("a"), file("a/b")]).is_err());
        assert!(
            ComponentIndex::new(component(), vec![file("a"), file("a.txt"), file("a/b")]).is_err()
        );
        let below_link = vec![
            IndexEntry::Symlink {
                path: path("lib"),
                target: "lib64".to_owned(),
            },
            file("lib/x.so"),
        ];
        assert!(ComponentIndex::new(component(), below_link).is_err());
        let below_directory = vec![IndexEntry::Directory { path: path("a") }, file("a/b")];
        assert!(ComponentIndex::new(component(), below_directory).is_ok());
    }

    #[test]
    fn unsorted_documents_are_rejected() {
        let index = ComponentIndex {
            schema: SCHEMA_VERSION,
            component: component(),
            entries: vec![file("b"), file("a")],
        };
        assert!(index.validate().is_err());
    }

    #[test]
    fn frames_must_fit_inside_a_pack() {
        let mut entry = file("a");
        if let IndexEntry::File { offset, length, .. } = &mut entry {
            *offset = MAX_PACK_BYTES;
            *length = 1;
        }
        assert!(ComponentIndex::new(component(), vec![entry]).is_err());
        let mut empty = file("a");
        if let IndexEntry::File { length, .. } = &mut empty {
            *length = 0;
        }
        assert!(ComponentIndex::new(component(), vec![empty]).is_err());
    }

    #[test]
    fn entry_json_carries_the_type_tag_used_by_the_web_sync() {
        let json = serde_json::to_value(file("a")).expect("json");
        assert_eq!(json["type"], "file");
        let json = serde_json::to_value(IndexEntry::Directory { path: path("d") }).expect("json");
        assert_eq!(json["type"], "directory");
    }
}
