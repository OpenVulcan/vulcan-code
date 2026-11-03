//! The install record `.vulcan-install.json` written into every assembled version directory.
//! 写入每个已组装版本目录的安装记录 `.vulcan-install.json`。
//!
//! The setup program, the Inno installer and the updater all write it, so the next update compares
//! against what is on disk instead of downloading anything about the installed version. The size and
//! modification time of each file let the updater trust a file without hashing it again; a file whose
//! size or time changed is hashed before it is reused.
//! 安装器、Inno 安装包与更新器都会写入它，因此下一次更新只与磁盘上的内容比对，无需下载已安装版本的任何
//! 信息。每个文件的大小与修改时间让更新器无需重新计算哈希就能信任该文件；大小或时间变化的文件在复用前
//! 先重新计算哈希。

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::digest::Sha256Digest;
use crate::error::{FormatError, FormatResult};
use crate::names::{ComponentName, PRODUCT, Platform, ReleaseVersion, SCHEMA_VERSION};
use crate::path::RelativePath;

/// File name of the record at the root of a version directory; no index may use this path.
/// 记录位于版本目录根部的文件名；任何索引都不得使用此路径。
pub const RECORD_FILE_NAME: &str = ".vulcan-install.json";

/// One installed path.
/// 一个已安装路径。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RecordEntry {
    /// A regular file.
    /// 普通文件。
    #[serde(rename_all = "camelCase")]
    File {
        /// Path from the version root.
        /// 相对版本根的路径。
        path: RelativePath,
        /// Size of the file.
        /// 文件大小。
        size: u64,
        /// SHA-256 of the file.
        /// 文件的 SHA-256。
        sha256: Sha256Digest,
        /// Modification time when the record was written, in nanoseconds since the Unix epoch.
        /// 写入记录时的修改时间，自 Unix 纪元起的纳秒数。
        modified_ns: u64,
    },
    /// A symbolic link.
    /// 符号链接。
    Symlink {
        /// Path from the version root.
        /// 相对版本根的路径。
        path: RelativePath,
        /// Link target.
        /// 链接目标。
        target: String,
    },
    /// A directory that exists even when empty.
    /// 即使为空也存在的目录。
    Directory {
        /// Path from the version root.
        /// 相对版本根的路径。
        path: RelativePath,
    },
}

/// What one version directory holds.
/// 一个版本目录包含的内容。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    /// Format version; only [`SCHEMA_VERSION`] is accepted.
    /// 格式版本；只接受 [`SCHEMA_VERSION`]。
    pub schema: u32,
    /// Always [`PRODUCT`].
    /// 恒为 [`PRODUCT`]。
    pub product: String,
    /// The installed version.
    /// 已安装的版本。
    pub version: ReleaseVersion,
    /// The platform.
    /// 平台。
    pub platform: Platform,
    /// Install epoch of the installed version.
    /// 已安装版本的安装纪元。
    pub install_epoch: u32,
    /// Installed components and the digest of the index each was assembled from.
    /// 已安装的组件及其组装所用索引的摘要。
    pub components: BTreeMap<ComponentName, Sha256Digest>,
    /// Every installed path, in byte order.
    /// 全部已安装路径，按字节序排列。
    pub entries: Vec<RecordEntry>,
}

impl RecordEntry {
    /// The entry's path.
    /// 条目的路径。
    pub fn path(&self) -> &RelativePath {
        match self {
            Self::File { path, .. } | Self::Symlink { path, .. } | Self::Directory { path } => path,
        }
    }
}

impl InstallRecord {
    /// Reads the record of a version directory.
    /// 读取某版本目录的记录。
    pub fn read(version_dir: &Path) -> FormatResult<Self> {
        let bytes = std::fs::read(version_dir.join(RECORD_FILE_NAME))
            .map_err(|source| FormatError::io("reading the install record", source))?;
        let record: Self = serde_json::from_slice(&bytes).map_err(|source| FormatError::Json {
            what: "install record",
            source,
        })?;
        if record.schema != SCHEMA_VERSION || record.product != PRODUCT {
            return Err(FormatError::invalid(
                "install record",
                format!(
                    "schema {} of product `{}` is not supported",
                    record.schema, record.product
                ),
            ));
        }
        Ok(record)
    }

    /// Writes the record into a version directory, replacing any earlier one atomically.
    /// 将记录写入版本目录，原子替换任何旧记录。
    pub fn write(&self, version_dir: &Path) -> FormatResult<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|source| FormatError::Json {
            what: "install record",
            source,
        })?;
        let fail = |source| FormatError::io("writing the install record", source);
        let partial = version_dir.join(format!("{RECORD_FILE_NAME}.partial"));
        std::fs::write(&partial, bytes).map_err(fail)?;
        std::fs::rename(&partial, version_dir.join(RECORD_FILE_NAME)).map_err(fail)
    }

    /// Recorded files by content digest; the first path wins when several share one content.
    /// 按内容摘要列出的已记录文件；多个路径共享同一内容时取第一个。
    pub fn files_by_digest(&self) -> HashMap<Sha256Digest, RecordedFile<'_>> {
        let mut files = HashMap::new();
        for entry in &self.entries {
            if let RecordEntry::File {
                path,
                size,
                sha256,
                modified_ns,
            } = entry
            {
                files.entry(*sha256).or_insert(RecordedFile {
                    path,
                    size: *size,
                    modified_ns: *modified_ns,
                });
            }
        }
        files
    }
}

/// A file as the record saw it.
/// 记录所见的文件。
#[derive(Clone, Copy, Debug)]
pub struct RecordedFile<'r> {
    /// Path from the version root.
    /// 相对版本根的路径。
    pub path: &'r RelativePath,
    /// Recorded size.
    /// 记录的大小。
    pub size: u64,
    /// Recorded modification time.
    /// 记录的修改时间。
    pub modified_ns: u64,
}

/// Modification time of file metadata in nanoseconds since the Unix epoch; 0 when unavailable.
/// 文件元数据的修改时间，自 Unix 纪元起的纳秒数；无法取得时为 0。
pub fn modified_ns(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
        })
}
