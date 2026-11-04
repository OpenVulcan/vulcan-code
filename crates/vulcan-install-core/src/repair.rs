//! Repairing an installed version in place: every file is hashed, and only missing or changed files
//! are fetched and swapped in.
//! 就地修复已安装版本：计算每个文件的哈希，只获取缺失或被改动的文件并替换进去。
//!
//! The version directory cannot be replaced while it runs, so repaired files are extracted into
//! `updates/<version>.repair/` first and then renamed over their targets one by one. A running
//! program keeps the file it opened: on Windows the old file is first renamed into the repair area,
//! which Windows allows even for a running executable.
//! 运行中的版本目录无法被整体替换，因此修复的文件先解压到 `updates/<version>.repair/`，再逐个改名覆盖
//! 目标。运行中的程序保留它已打开的文件：在 Windows 上旧文件先被改名到修复区，Windows 即使对运行中的
//! 可执行文件也允许这样做。

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::assemble::{
    FrameTarget, PackSource, RangePlan, apply_mode, create_parent, fetch_range, hash_file,
    link_or_copy, mode_of, native, place_symlink, plan_ranges,
};
use crate::digest::Sha256Digest;
use crate::error::{FormatError, FormatResult};
use crate::index::{ComponentIndex, IndexEntry};
use crate::install_root::{InstallLock, InstallRoot, UPDATES_DIR};
use crate::limits::DISK_HEADROOM_PERCENT;
use crate::manifest::ReleaseManifest;
use crate::path::RelativePath;
use crate::record::{InstallRecord, RecordEntry, modified_ns};

/// Suffix of a version's repair area below [`UPDATES_DIR`].
/// [`UPDATES_DIR`] 下版本修复区的后缀。
pub const REPAIR_SUFFIX: &str = ".repair";

/// A checked installed version and the files it still needs.
/// 已检查的已安装版本及其仍需的文件。
pub struct PreparedRepair<'r> {
    /// Proof that the caller holds the install lock for the whole repair.
    /// 调用方在整个修复期间持有安装锁的凭证。
    _lock: &'r InstallLock,
    /// The version directory being repaired.
    /// 被修复的版本目录。
    version_dir: PathBuf,
    /// Area holding fetched files and replaced originals.
    /// 存放获取的文件与被替换原文件的区域。
    area: PathBuf,
    /// The record rewritten once every file is in place.
    /// 全部文件就位后重写的记录。
    record: InstallRecord,
    /// Files to swap in, with the content digest staged for each.
    /// 待替换进去的文件及各自暂存内容的摘要。
    damaged: Vec<(RelativePath, Sha256Digest, u32)>,
    /// Range requests still to make.
    /// 仍需发出的 Range 请求。
    ranges: Vec<RangePlan>,
}

/// Hashes every file of the installed `manifest.version` against the component indexes it was
/// installed from, restores directories and links at once, and plans the fetches for the rest.
/// 按安装所用的组件索引计算已安装 `manifest.version` 每个文件的哈希，立即恢复目录与链接，并为其余文件
/// 规划获取。
///
/// The caller verified `manifest` and read `indexes` against its digests; they must be exactly the
/// components the install record names, so a repair never changes what is installed.
/// 调用方已校验 `manifest` 并按其摘要读取 `indexes`；它们必须正是安装记录所列的组件，使修复从不改变
/// 已安装的内容。
pub fn prepare_repair<'r>(
    root: &InstallRoot,
    lock: &'r InstallLock,
    manifest: &ReleaseManifest,
    indexes: Vec<ComponentIndex>,
) -> FormatResult<PreparedRepair<'r>> {
    let fail = |source| FormatError::io("checking the installed files", source);
    let version_dir = root.version_dir(&manifest.version);
    let record = InstallRecord::read(&version_dir)?;
    if record.version != manifest.version || record.platform != manifest.platform {
        return Err(FormatError::invalid(
            "install record",
            format!(
                "describes {} {} instead of {} {}",
                record.version, record.platform, manifest.version, manifest.platform
            ),
        ));
    }
    let mut selected = BTreeMap::new();
    for index in &indexes {
        let Some(reference) = manifest.components.get(&index.component) else {
            return Err(FormatError::invalid(
                "repair",
                format!(
                    "component `{}` is not part of {}",
                    index.component, manifest.version
                ),
            ));
        };
        selected.insert(index.component.clone(), reference.index);
    }
    if selected != record.components {
        return Err(FormatError::invalid(
            "repair",
            "the selected components differ from the installed ones",
        ));
    }

    let area = root
        .path()
        .join(UPDATES_DIR)
        .join(format!("{}{REPAIR_SUFFIX}", manifest.version));
    std::fs::create_dir_all(&area).map_err(fail)?;
    let mut entries = Vec::new();
    let mut damaged = Vec::new();
    let mut pending: BTreeMap<(Sha256Digest, u64), FrameTarget> = BTreeMap::new();
    let mut fetched: HashSet<Sha256Digest> = HashSet::new();
    for index in indexes {
        for entry in index.entries {
            let target = native(&version_dir, entry.path());
            match &entry {
                IndexEntry::Directory { path } => {
                    std::fs::create_dir_all(&target).map_err(fail)?;
                    entries.push(RecordEntry::Directory { path: path.clone() });
                }
                IndexEntry::Symlink { path, target: link } => {
                    create_parent(&target)?;
                    place_symlink(link, &target)?;
                    entries.push(RecordEntry::Symlink {
                        path: path.clone(),
                        target: link.clone(),
                    });
                }
                IndexEntry::File {
                    path,
                    mode,
                    size,
                    sha256,
                    pack,
                    offset,
                    length,
                } => {
                    if let Some(modified) = intact(&target, *size, *sha256, *mode)? {
                        entries.push(RecordEntry::File {
                            path: path.clone(),
                            size: *size,
                            sha256: *sha256,
                            modified_ns: modified,
                        });
                        continue;
                    }
                    damaged.push((path.clone(), *sha256, *mode));
                    let staged = area.join(sha256.to_string());
                    if fetched.insert(*sha256) && !staged_matches(&staged, *size, *sha256)? {
                        pending.insert(
                            (*pack, *offset),
                            FrameTarget {
                                offset: *offset,
                                length: *length,
                                size: *size,
                                sha256: *sha256,
                                paths: vec![(staged, 0o644)],
                            },
                        );
                    }
                    // Its record entry is written once the file is in place.
                    // 其记录条目在文件就位后写入。
                    entries.push(RecordEntry::File {
                        path: path.clone(),
                        size: *size,
                        sha256: *sha256,
                        modified_ns: 0,
                    });
                }
            }
        }
    }
    entries.sort_by(|left, right| left.path().cmp(right.path()));

    let to_write: u64 = pending.values().map(|frame| frame.size).sum();
    // Each fetched file is written once in the area and once more next to its target.
    // 每个获取的文件在修复区写入一次，在目标旁再写入一次。
    let doubled = to_write.saturating_mul(2);
    let needed = doubled.saturating_add(doubled / 100 * DISK_HEADROOM_PERCENT);
    let available = fs4::available_space(&area)
        .map_err(|source| FormatError::io("reading free disk space", source))?;
    if needed > available {
        return Err(FormatError::InsufficientSpace { needed, available });
    }
    Ok(PreparedRepair {
        _lock: lock,
        version_dir,
        area,
        record: InstallRecord { entries, ..record },
        damaged,
        ranges: plan_ranges(pending),
    })
}

impl PreparedRepair<'_> {
    /// Number of files that are missing or changed.
    /// 缺失或被改动的文件数。
    pub fn damaged_count(&self) -> usize {
        self.damaged.len()
    }

    /// Bytes the remaining range requests download.
    /// 剩余 Range 请求将下载的字节数。
    pub fn download_bytes(&self) -> u64 {
        self.ranges.iter().map(|range| range.length).sum()
    }

    /// Fetches the missing contents, swaps every damaged file in and rewrites the install record.
    /// 获取缺失的内容，替换每个损坏的文件，并重写安装记录。
    ///
    /// On failure the fetched contents stay in the repair area, so a later run resumes.
    /// 失败时已获取的内容保留在修复区，以便之后的运行续修。
    pub fn complete(mut self, source: &mut dyn PackSource) -> FormatResult<usize> {
        for range in &self.ranges {
            fetch_range(source, range)?;
        }
        let fail = |source| FormatError::io("replacing a damaged file", source);
        for (number, (path, sha256, mode)) in self.damaged.iter().enumerate() {
            let target = native(&self.version_dir, path);
            create_parent(&target)?;
            let incoming = target.with_file_name(format!(
                "{}.vulcan-repair",
                target
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("file")
            ));
            let _ = std::fs::remove_file(&incoming);
            link_or_copy(&self.area.join(sha256.to_string()), 0o644, &incoming, *mode)?;
            apply_mode(&incoming, *mode)?;
            if cfg!(windows) && std::fs::symlink_metadata(&target).is_ok() {
                let displaced = self.area.join(format!("replaced-{number}"));
                let _ = std::fs::remove_file(&displaced);
                std::fs::rename(&target, &displaced).map_err(fail)?;
            } else if std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.is_dir()) {
                std::fs::remove_dir_all(&target).map_err(fail)?;
            }
            std::fs::rename(&incoming, &target).map_err(fail)?;
        }
        let version_dir = &self.version_dir;
        for entry in &mut self.record.entries {
            if let RecordEntry::File {
                path,
                modified_ns: modified,
                ..
            } = entry
            {
                let metadata = std::fs::symlink_metadata(native(version_dir, path))
                    .map_err(|source| FormatError::io("reading a repaired file", source))?;
                *modified = modified_ns(&metadata);
            }
        }
        self.record.write(&self.version_dir)?;
        // Displaced files may still be open by a running program; a later cleanup removes them.
        // 被替换的文件可能仍被运行中的程序打开；之后的清理会删除它们。
        let _ = std::fs::remove_dir_all(&self.area);
        Ok(self.damaged.len())
    }
}

/// The modification time of an installed file that holds the expected content and mode, or `None`.
/// 内容与权限均符合期望的已安装文件的修改时间，否则为 `None`。
fn intact(path: &Path, size: u64, sha256: Sha256Digest, mode: u32) -> FormatResult<Option<u64>> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if !metadata.is_file() || metadata.len() != size || hash_file(path)? != sha256 {
        return Ok(None);
    }
    if cfg!(unix) && mode_of(&metadata) != mode {
        apply_mode(path, mode)?;
    }
    Ok(Some(modified_ns(&metadata)))
}

/// Whether a content fetched by an interrupted repair is complete; a wrong one is removed.
/// 中断的修复已获取的内容是否完整；不完整的会被删除。
fn staged_matches(path: &Path, size: u64, sha256: Sha256Digest) -> FormatResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() == size => {
            if hash_file(path)? == sha256 {
                return Ok(true);
            }
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(FormatError::io("reading a fetched file", source)),
    }
    std::fs::remove_file(path)
        .map_err(|source| FormatError::io("removing a stale fetched file", source))?;
    Ok(false)
}
