//! Assembling a version directory from component indexes, reusing what is already on disk.
//! 依据组件索引组装版本目录，并复用磁盘上已有的内容。
//!
//! A first install and an incremental update are the same operation: every file of the selected
//! components is placed in `updates/<version>.partial/`, taken in this order from a file already
//! staged by an interrupted run, from another staged path with the same content, from the current
//! version (hard link, or copy when linking fails or the Unix mode differs), and only then from the
//! packs. Frames still needed are fetched with as few range requests as practical. Once every file is
//! in place the install record is written and the directory is renamed to `versions/<version>`;
//! switching the current version is a separate step.
//!
//! 首次安装与增量更新是同一操作：所选组件的每个文件都放入 `updates/<version>.partial/`，依次取自中断
//! 运行已放好的文件、暂存区中内容相同的其他路径、当前版本（硬链接；链接失败或 Unix 权限不同时复制），
//! 最后才取自文件包。仍需的帧以尽量少的 Range 请求获取。全部文件就位后写入安装记录，并将目录改名为
//! `versions/<version>`；切换当前版本是单独的步骤。

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::digest::Sha256Digest;
use crate::error::{FormatError, FormatResult};
use crate::index::{ComponentIndex, IndexEntry, check_entry_set};
use crate::install_root::{InstallLock, InstallRoot, VERSIONS_DIR, create_symlink};
use crate::layout::pack_key;
use crate::limits::{DISK_HEADROOM_PERCENT, RANGE_MERGE_GAP_BYTES};
use crate::manifest::ReleaseManifest;
use crate::names::{ComponentName, PRODUCT, Platform, ReleaseVersion, SCHEMA_VERSION};
use crate::pack::extract_frame;
use crate::path::RelativePath;
use crate::record::{InstallRecord, RECORD_FILE_NAME, RecordEntry, modified_ns};

/// Where pack bytes come from: an HTTP mirror, or a local object directory.
/// 文件包字节的来源：HTTP 镜像或本地对象目录。
pub trait PackSource {
    /// Opens exactly `length` bytes of pack `pack` starting at `offset`.
    /// 打开文件包 `pack` 自 `offset` 起恰好 `length` 字节的内容。
    ///
    /// The bytes need not be trusted: every file is checked against its index digest.
    /// 这些字节无需被信任：每个文件都会按索引摘要校验。
    fn open_range(
        &mut self,
        pack: Sha256Digest,
        offset: u64,
        length: u64,
    ) -> FormatResult<Box<dyn Read + '_>>;
}

/// Packs read from a local copy of the object layout, as the release tooling writes it.
/// 从本地对象布局副本读取的文件包，布局与发布工具写出的一致。
#[derive(Clone, Debug)]
pub struct DirectoryPackSource {
    /// Root of the object layout.
    /// 对象布局的根目录。
    root: PathBuf,
}

impl DirectoryPackSource {
    /// Reads packs below `root`.
    /// 从 `root` 之下读取文件包。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl PackSource for DirectoryPackSource {
    fn open_range(
        &mut self,
        pack: Sha256Digest,
        offset: u64,
        length: u64,
    ) -> FormatResult<Box<dyn Read + '_>> {
        let fail = |source| FormatError::io("reading a local pack", source);
        let mut file = File::open(self.root.join(pack_key(pack))).map_err(fail)?;
        file.seek(SeekFrom::Start(offset)).map_err(fail)?;
        Ok(Box::new(file.take(length)))
    }
}

/// The selected components of one release, checked as a single tree.
/// 某版本的所选组件，已作为一棵树校验。
pub struct Assembly<'r> {
    /// The install root.
    /// 安装根目录。
    root: &'r InstallRoot,
    /// Proof that the caller holds the install lock for the whole assembly.
    /// 调用方在整个组装期间持有安装锁的凭证。
    _lock: &'r InstallLock,
    /// Version being assembled.
    /// 正在组装的版本。
    version: ReleaseVersion,
    /// Its platform.
    /// 其平台。
    platform: Platform,
    /// Its install epoch.
    /// 其安装纪元。
    install_epoch: u32,
    /// Selected components and their index digests.
    /// 所选组件及其索引摘要。
    components: BTreeMap<ComponentName, Sha256Digest>,
    /// Entries of all selected components in path order.
    /// 所选全部组件的条目，按路径排序。
    entries: Vec<IndexEntry>,
}

/// An assembly whose local work is done; only frames from the packs remain.
/// 本地工作已完成的组装；只剩来自文件包的帧。
pub struct PreparedAssembly<'r> {
    /// The assembly.
    /// 组装。
    assembly: Assembly<'r>,
    /// Its staging directory.
    /// 其暂存目录。
    staging: PathBuf,
    /// Range requests still to make.
    /// 仍需发出的 Range 请求。
    ranges: Vec<RangePlan>,
}

/// One range request covering one or more frames of a pack.
/// 覆盖某文件包一个或多个帧的一次 Range 请求。
#[derive(Clone, Debug, PartialEq, Eq)]
struct RangePlan {
    /// The pack.
    /// 文件包。
    pack: Sha256Digest,
    /// First byte requested.
    /// 请求的首字节。
    offset: u64,
    /// Bytes requested.
    /// 请求的字节数。
    length: u64,
    /// Frames inside the range, in offset order.
    /// 范围内的帧，按偏移排序。
    frames: Vec<FrameTarget>,
}

/// One frame to extract and the staged paths that receive its content.
/// 一个待解压的帧及接收其内容的暂存路径。
#[derive(Clone, Debug, PartialEq, Eq)]
struct FrameTarget {
    /// Offset of the frame in its pack.
    /// 帧在文件包中的偏移。
    offset: u64,
    /// Length of the frame.
    /// 帧的长度。
    length: u64,
    /// Size of the file.
    /// 文件大小。
    size: u64,
    /// Digest of the file.
    /// 文件摘要。
    sha256: Sha256Digest,
    /// Staged paths and their modes; the first is extracted, the rest linked or copied from it.
    /// 暂存路径及其权限；第一个由解压得到，其余从它链接或复制。
    paths: Vec<(PathBuf, u32)>,
}

/// What a staging path is expected to be.
/// 暂存路径应有的类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedKind {
    /// A regular file.
    /// 普通文件。
    File,
    /// A symbolic link.
    /// 符号链接。
    Symlink,
    /// A directory.
    /// 目录。
    Directory,
}

impl<'r> Assembly<'r> {
    /// Selects components of a verified manifest whose indexes the caller read with
    /// [`ComponentIndex::from_object`] against the manifest digests.
    /// 选取已校验清单中的组件；调用方已用 [`ComponentIndex::from_object`] 按清单摘要读取其索引。
    pub fn new(
        root: &'r InstallRoot,
        lock: &'r InstallLock,
        manifest: &ReleaseManifest,
        indexes: Vec<ComponentIndex>,
    ) -> FormatResult<Self> {
        let invalid = |reason: String| Err(FormatError::invalid("assembly", reason));
        if indexes.is_empty() {
            return invalid("no component is selected".to_owned());
        }
        let mut components = BTreeMap::new();
        let mut entries = Vec::new();
        for index in indexes {
            let Some(reference) = manifest.components.get(&index.component) else {
                return invalid(format!(
                    "component `{}` is not part of {}",
                    index.component, manifest.version
                ));
            };
            if components
                .insert(index.component.clone(), reference.index)
                .is_some()
            {
                return invalid(format!("component `{}` is selected twice", index.component));
            }
            entries.extend(index.entries);
        }
        entries.sort_by(|left, right| left.path().cmp(right.path()));
        // Components may each declare a directory they share.
        // 各组件可以各自声明它们共享的目录。
        entries.dedup_by(|later, earlier| {
            later == earlier && matches!(later, IndexEntry::Directory { .. })
        });
        check_entry_set(&entries.iter().collect::<Vec<_>>())?;
        if cfg!(windows)
            && let Some(link) = entries
                .iter()
                .find(|entry| matches!(entry, IndexEntry::Symlink { .. }))
        {
            return invalid(format!(
                "`{}` is a symbolic link, which Windows installs never contain",
                link.path()
            ));
        }
        Ok(Self {
            root,
            _lock: lock,
            version: manifest.version.clone(),
            platform: manifest.platform,
            install_epoch: manifest.install_epoch,
            components,
            entries,
        })
    }

    /// Places every directory, link and locally available file in the staging directory, plans the
    /// range requests for the rest, and checks that the disk can hold what remains.
    /// 将全部目录、链接与本地可得的文件放入暂存目录，为其余文件规划 Range 请求，并检查磁盘能否容纳剩余
    /// 内容。
    pub fn prepare(self) -> FormatResult<PreparedAssembly<'r>> {
        let fail = |source| FormatError::io("preparing the staging directory", source);
        if self.root.version_dir(&self.version).exists() {
            return Err(FormatError::invalid(
                "assembly",
                format!("{} is already installed", self.version),
            ));
        }
        let staging = self.root.staging_dir(&self.version);
        std::fs::create_dir_all(&staging).map_err(fail)?;
        remove_unexpected(&staging, "", &self.expected_kinds())?;

        for entry in &self.entries {
            let target = native(&staging, entry.path());
            match entry {
                IndexEntry::Directory { .. } => std::fs::create_dir_all(&target).map_err(fail)?,
                IndexEntry::Symlink { target: link, .. } => {
                    create_parent(&target)?;
                    place_symlink(link, &target)?;
                }
                IndexEntry::File { .. } => create_parent(&target)?,
            }
        }

        let reuse = self.reusable_version()?;
        let reusable = reuse
            .as_ref()
            .map(|(_, record)| record.files_by_digest())
            .unwrap_or_default();
        let mut placed: HashMap<Sha256Digest, (PathBuf, u32)> = HashMap::new();
        let mut pending: BTreeMap<(Sha256Digest, u64), FrameTarget> = BTreeMap::new();
        for entry in &self.entries {
            let IndexEntry::File {
                path,
                mode,
                size,
                sha256,
                pack,
                offset,
                length,
            } = entry
            else {
                continue;
            };
            let target = native(&staging, path);
            if staged_file_matches(&target, *size, *sha256)? {
                apply_mode(&target, *mode)?;
                placed.entry(*sha256).or_insert((target, *mode));
                continue;
            }
            if let Some((source, source_mode)) = placed.get(sha256) {
                link_or_copy(source, *source_mode, &target, *mode)?;
                continue;
            }
            let recorded = reusable
                .get(sha256)
                .filter(|recorded| recorded.size == *size);
            if let (Some((dir, _)), Some(recorded)) = (&reuse, recorded) {
                let source = native(dir, recorded.path);
                if let Some(source_mode) =
                    trusted_mode(&source, recorded.modified_ns, *size, *sha256)?
                {
                    link_or_copy(&source, source_mode, &target, *mode)?;
                    placed.insert(*sha256, (target, *mode));
                    continue;
                }
            }
            pending
                .entry((*pack, *offset))
                .or_insert_with(|| FrameTarget {
                    offset: *offset,
                    length: *length,
                    size: *size,
                    sha256: *sha256,
                    paths: Vec::new(),
                })
                .paths
                .push((target, *mode));
        }

        // Further paths of one content are expected to become hard links and need no space.
        // 同一内容的其余路径预计成为硬链接，不占用空间。
        let to_write: u64 = pending.values().map(|frame| frame.size).sum();
        let needed = to_write.saturating_add(to_write / 100 * DISK_HEADROOM_PERCENT);
        let available = fs4::available_space(&staging)
            .map_err(|source| FormatError::io("reading free disk space", source))?;
        if needed > available {
            return Err(FormatError::InsufficientSpace { needed, available });
        }
        Ok(PreparedAssembly {
            ranges: plan_ranges(pending),
            assembly: self,
            staging,
        })
    }

    /// Every path the staging directory may hold, with its kind; ancestors are directories.
    /// 暂存目录可包含的全部路径及其类型；各级父路径为目录。
    fn expected_kinds(&self) -> HashMap<&str, ExpectedKind> {
        let mut expected = HashMap::new();
        for entry in &self.entries {
            for ancestor in entry.path().ancestors() {
                expected.insert(ancestor, ExpectedKind::Directory);
            }
            let kind = match entry {
                IndexEntry::File { .. } => ExpectedKind::File,
                IndexEntry::Symlink { .. } => ExpectedKind::Symlink,
                IndexEntry::Directory { .. } => ExpectedKind::Directory,
            };
            expected.insert(entry.path().as_str(), kind);
        }
        expected
    }

    /// The current version's directory and record, when another version is current and recorded.
    /// 当前版本的目录与记录（当前为另一版本且有记录时）。
    fn reusable_version(&self) -> FormatResult<Option<(PathBuf, InstallRecord)>> {
        let Some(current) = self.root.current_version()? else {
            return Ok(None);
        };
        if current == self.version {
            return Ok(None);
        }
        let dir = self.root.version_dir(&current);
        if !dir.join(RECORD_FILE_NAME).is_file() {
            return Ok(None);
        }
        let record = InstallRecord::read(&dir)?;
        Ok(Some((dir, record)))
    }
}

impl<'r> PreparedAssembly<'r> {
    /// Bytes the remaining range requests download.
    /// 剩余 Range 请求将下载的字节数。
    pub fn download_bytes(&self) -> u64 {
        self.ranges.iter().map(|range| range.length).sum()
    }

    /// Number of range requests remaining.
    /// 剩余的 Range 请求数。
    pub fn request_count(&self) -> usize {
        self.ranges.len()
    }

    /// Fetches the remaining frames, writes the install record and moves the staging directory to
    /// `versions/<version>`, whose path is returned.
    /// 获取剩余的帧，写入安装记录，并将暂存目录移动到 `versions/<version>`，返回该路径。
    ///
    /// On failure the staging directory keeps every complete file, so a later run resumes.
    /// 失败时暂存目录保留全部完整文件，以便之后的运行续装。
    pub fn complete(self, source: &mut dyn PackSource) -> FormatResult<PathBuf> {
        for range in &self.ranges {
            fetch_range(source, range)?;
        }
        let assembly = &self.assembly;
        let mut entries = Vec::with_capacity(assembly.entries.len());
        for entry in &assembly.entries {
            entries.push(match entry {
                IndexEntry::File {
                    path, size, sha256, ..
                } => {
                    let metadata = std::fs::symlink_metadata(native(&self.staging, path))
                        .map_err(|source| FormatError::io("reading a staged file", source))?;
                    RecordEntry::File {
                        path: path.clone(),
                        size: *size,
                        sha256: *sha256,
                        modified_ns: modified_ns(&metadata),
                    }
                }
                IndexEntry::Symlink { path, target } => RecordEntry::Symlink {
                    path: path.clone(),
                    target: target.clone(),
                },
                IndexEntry::Directory { path } => RecordEntry::Directory { path: path.clone() },
            });
        }
        let record = InstallRecord {
            schema: SCHEMA_VERSION,
            product: PRODUCT.to_owned(),
            version: assembly.version.clone(),
            platform: assembly.platform,
            install_epoch: assembly.install_epoch,
            components: assembly.components.clone(),
            entries,
        };
        record.write(&self.staging)?;
        let fail = |source| FormatError::io("moving the assembled version", source);
        let target = assembly.root.version_dir(&assembly.version);
        std::fs::create_dir_all(assembly.root.path().join(VERSIONS_DIR)).map_err(fail)?;
        std::fs::rename(&self.staging, &target).map_err(fail)?;
        Ok(target)
    }
}

/// Groups needed frames into range requests: a pack whose needed bytes exceed half of the span
/// they cover is fetched as that one span; otherwise frames separated by at most
/// [`RANGE_MERGE_GAP_BYTES`] share a request.
/// 将所需帧分组为 Range 请求：某文件包所需字节超过其覆盖跨度的一半时整段获取；否则间隔不超过
/// [`RANGE_MERGE_GAP_BYTES`] 的帧共用一个请求。
fn plan_ranges(pending: BTreeMap<(Sha256Digest, u64), FrameTarget>) -> Vec<RangePlan> {
    let mut by_pack: BTreeMap<Sha256Digest, Vec<FrameTarget>> = BTreeMap::new();
    for ((pack, _), frame) in pending {
        by_pack.entry(pack).or_default().push(frame);
    }
    let mut ranges = Vec::new();
    for (pack, frames) in by_pack {
        let overlapping = frames
            .windows(2)
            .any(|pair| pair[1].offset < pair[0].offset + pair[0].length);
        let needed: u64 = frames.iter().map(|frame| frame.length).sum();
        let start = frames[0].offset;
        let end = frames
            .iter()
            .map(|frame| frame.offset + frame.length)
            .max()
            .unwrap_or(start);
        let whole_span = !overlapping && needed.saturating_mul(2) > end - start;
        let mut current: Option<RangePlan> = None;
        for frame in frames {
            let frame_end = frame.offset + frame.length;
            if let Some(range) = current.as_mut() {
                let range_end = range.offset + range.length;
                if frame.offset >= range_end
                    && (whole_span || frame.offset - range_end <= RANGE_MERGE_GAP_BYTES)
                {
                    range.length = frame_end - range.offset;
                    range.frames.push(frame);
                    continue;
                }
                ranges.extend(current.take());
            }
            current = Some(RangePlan {
                pack,
                offset: frame.offset,
                length: frame.length,
                frames: vec![frame],
            });
        }
        ranges.extend(current);
    }
    ranges
}

/// Fetches one range and places every frame inside it.
/// 获取一个范围并放置其中的每个帧。
fn fetch_range(source: &mut dyn PackSource, range: &RangePlan) -> FormatResult<()> {
    let mut reader = source.open_range(range.pack, range.offset, range.length)?;
    let mut position = range.offset;
    for frame in &range.frames {
        let gap = frame.offset - position;
        let skipped = std::io::copy(&mut (&mut reader).take(gap), &mut std::io::sink())
            .map_err(|source| FormatError::io("reading a pack range", source))?;
        if skipped != gap {
            return Err(FormatError::SizeMismatch {
                what: "pack range",
                expected: range.length,
                actual: skipped + (position - range.offset),
            });
        }
        let Some(((first, first_mode), rest)) = frame.paths.split_first() else {
            continue;
        };
        let mut frame_reader = (&mut reader).take(frame.length);
        if let Err(error) = extract_to(&mut frame_reader, frame, first) {
            let _ = std::fs::remove_file(first);
            return Err(error);
        }
        std::io::copy(&mut frame_reader, &mut std::io::sink())
            .map_err(|source| FormatError::io("reading a pack range", source))?;
        apply_mode(first, *first_mode)?;
        for (path, mode) in rest {
            link_or_copy(first, *first_mode, path, *mode)?;
        }
        position = frame.offset + frame.length;
    }
    Ok(())
}

/// Extracts one frame into a new file at `target`.
/// 将一个帧解压到 `target` 处的新文件。
fn extract_to(frame: &mut dyn Read, target: &FrameTarget, path: &Path) -> FormatResult<()> {
    let fail = |source| FormatError::io("writing a staged file", source);
    let file = File::create(path).map_err(fail)?;
    let mut writer = BufWriter::new(file);
    extract_frame(frame, target.size, target.sha256, &mut writer)?;
    writer.flush().map_err(fail)
}

/// Removes everything below `dir` that the assembly does not expect, recursively.
/// 递归删除 `dir` 之下组装不期望的一切内容。
fn remove_unexpected(
    dir: &Path,
    prefix: &str,
    expected: &HashMap<&str, ExpectedKind>,
) -> FormatResult<()> {
    let fail = |source| FormatError::io("cleaning the staging directory", source);
    for child in std::fs::read_dir(dir).map_err(fail)? {
        let child = child.map_err(fail)?;
        let file_type = child.file_type().map_err(fail)?;
        let kind = if file_type.is_symlink() {
            ExpectedKind::Symlink
        } else if file_type.is_dir() {
            ExpectedKind::Directory
        } else {
            ExpectedKind::File
        };
        let relative = child
            .file_name()
            .to_str()
            .map(|name| format!("{prefix}{name}"));
        let wanted = relative
            .as_deref()
            .and_then(|relative| expected.get(relative))
            == Some(&kind);
        match (wanted, kind) {
            (true, ExpectedKind::Directory) => {
                let relative = relative.unwrap_or_default();
                remove_unexpected(&child.path(), &format!("{relative}/"), expected)?;
            }
            (true, _) => {}
            (false, ExpectedKind::Directory) => {
                std::fs::remove_dir_all(child.path()).map_err(fail)?;
            }
            (false, _) => std::fs::remove_file(child.path()).map_err(fail)?,
        }
    }
    Ok(())
}

/// The native path of `path` below `base`.
/// `path` 在 `base` 之下的本地路径。
fn native(base: &Path, path: &RelativePath) -> PathBuf {
    let mut native = base.to_path_buf();
    native.extend(path.segments());
    native
}

/// Creates the parent directory of `path`.
/// 创建 `path` 的父目录。
fn create_parent(path: &Path) -> FormatResult<()> {
    match path.parent() {
        Some(parent) => std::fs::create_dir_all(parent)
            .map_err(|source| FormatError::io("creating a staged directory", source)),
        None => Ok(()),
    }
}

/// Makes `path` a symbolic link to `target`, replacing a link that points elsewhere.
/// 使 `path` 成为指向 `target` 的符号链接，替换指向其他位置的链接。
fn place_symlink(target: &str, path: &Path) -> FormatResult<()> {
    let fail = |source| FormatError::io("creating a staged link", source);
    match std::fs::read_link(path) {
        Ok(existing) if existing.as_os_str() == target => return Ok(()),
        Ok(_) => std::fs::remove_file(path).map_err(fail)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => return Err(fail(source)),
    }
    create_symlink(target, path).map_err(fail)
}

/// Whether a file left by an interrupted run has the expected content; a wrong one is removed.
/// 中断运行留下的文件内容是否符合期望；不符合的会被删除。
fn staged_file_matches(path: &Path, size: u64, sha256: Sha256Digest) -> FormatResult<bool> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(FormatError::io("reading a staged file", source)),
    };
    if metadata.is_file() && metadata.len() == size && hash_file(path)? == sha256 {
        return Ok(true);
    }
    std::fs::remove_file(path)
        .map_err(|source| FormatError::io("removing a stale staged file", source))?;
    Ok(false)
}

/// The Unix mode of an installed file that still has the recorded content, or `None`.
/// 仍为已记录内容的已安装文件的 Unix 权限，否则为 `None`。
///
/// A file whose size and modification time match the record is trusted without hashing.
/// 大小与修改时间均与记录一致的文件无需计算哈希即被信任。
fn trusted_mode(
    path: &Path,
    recorded_modified_ns: u64,
    size: u64,
    sha256: Sha256Digest,
) -> FormatResult<Option<u32>> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if !metadata.is_file() || metadata.len() != size {
        return Ok(None);
    }
    if modified_ns(&metadata) != recorded_modified_ns && hash_file(path)? != sha256 {
        return Ok(None);
    }
    Ok(Some(mode_of(&metadata)))
}

/// SHA-256 of a file.
/// 文件的 SHA-256。
fn hash_file(path: &Path) -> FormatResult<Sha256Digest> {
    let fail = |source| FormatError::io("hashing a file", source);
    let mut file = File::open(path).map_err(fail)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 << 10];
    loop {
        let read = file.read(&mut buffer).map_err(fail)?;
        if read == 0 {
            return Ok(Sha256Digest::finish(hasher));
        }
        hasher.update(&buffer[..read]);
    }
}

/// Places `target` with the content of `source`: a hard link when the modes agree, else a copy.
/// 以 `source` 的内容放置 `target`：权限一致时用硬链接，否则复制。
fn link_or_copy(source: &Path, source_mode: u32, target: &Path, mode: u32) -> FormatResult<()> {
    let fail = |source| FormatError::io("placing a reused file", source);
    if (cfg!(windows) || source_mode == mode) && std::fs::hard_link(source, target).is_ok() {
        return Ok(());
    }
    std::fs::copy(source, target).map_err(fail)?;
    apply_mode(target, mode)
}

/// The Unix permission bits of file metadata; 0 elsewhere.
/// 文件元数据的 Unix 权限位；其他平台为 0。
fn mode_of(metadata: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()) & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0
    }
}

/// Applies the index mode on Unix; Windows has no permission bits to apply.
/// 在 Unix 上应用索引中的权限；Windows 没有可应用的权限位。
fn apply_mode(path: &Path, mode: u32) -> FormatResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|source| FormatError::io("setting a file mode", source))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ComponentRef;
    use crate::pack::PackBuilder;

    /// Counts the requests and bytes a source serves, and can fail after a number of requests.
    /// 统计来源提供的请求数与字节数，并可在若干请求后失败。
    struct CountingSource {
        inner: DirectoryPackSource,
        requests: usize,
        bytes: u64,
        fail_after: Option<usize>,
    }

    impl CountingSource {
        fn new(root: &Path) -> Self {
            Self {
                inner: DirectoryPackSource::new(root),
                requests: 0,
                bytes: 0,
                fail_after: None,
            }
        }
    }

    impl PackSource for CountingSource {
        fn open_range(
            &mut self,
            pack: Sha256Digest,
            offset: u64,
            length: u64,
        ) -> FormatResult<Box<dyn Read + '_>> {
            if self.fail_after == Some(self.requests) {
                return Err(FormatError::io(
                    "test source",
                    std::io::Error::other("interrupted"),
                ));
            }
            self.requests += 1;
            self.bytes += length;
            self.inner.open_range(pack, offset, length)
        }
    }

    /// Bytes zstd cannot shrink.
    /// zstd 无法压缩的字节。
    fn noise(seed: u64, length: usize) -> Vec<u8> {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect()
    }

    /// A published release: its object directory, manifest and component indexes.
    /// 一个已发布版本：对象目录、清单与组件索引。
    struct Release {
        manifest: ReleaseManifest,
        indexes: Vec<ComponentIndex>,
    }

    /// Publishes `files` (component, path, content) as `version` into `objects`, reusing contents
    /// already published there.
    /// 将 `files`（组件、路径、内容）作为 `version` 发布到 `objects`，复用其中已发布的内容。
    fn publish(
        objects: &Path,
        published: &mut HashMap<Sha256Digest, crate::pack::FileLocation>,
        version: &str,
        files: &[(&str, &str, &[u8])],
    ) -> Release {
        let mut builder = PackBuilder::new(objects, published.clone())
            .with_target_bytes(4_096)
            .with_level(1);
        for (_, _, content) in files {
            builder.add(content).expect("add");
        }
        let outcome = builder.finish().expect("finish");
        published.extend(outcome.locations.iter().map(|(key, value)| (*key, *value)));
        let mut by_component: BTreeMap<&str, Vec<IndexEntry>> = BTreeMap::new();
        for (component, path, content) in files {
            let digest = Sha256Digest::of(content);
            by_component
                .entry(*component)
                .or_default()
                .push(IndexEntry::file(
                    RelativePath::new(*path).expect("path"),
                    0o755,
                    content.len() as u64,
                    digest,
                    outcome.locations[&digest],
                ));
        }
        let mut components = BTreeMap::new();
        let mut indexes = Vec::new();
        for (component, entries) in by_component {
            let name = ComponentName::new(component).expect("name");
            let index = ComponentIndex::new(name.clone(), entries).expect("index");
            let (digest, _) = index.to_object().expect("object");
            components.insert(
                name,
                ComponentRef {
                    index: digest,
                    size: index.installed_size(),
                    fingerprint: None,
                },
            );
            indexes.push(index);
        }
        Release {
            manifest: ReleaseManifest {
                schema: SCHEMA_VERSION,
                product: PRODUCT.to_owned(),
                version: version.parse().expect("version"),
                platform: Platform::current().unwrap_or(Platform::LinuxX64),
                install_epoch: 1,
                released_at: "2026-11-03T00:00:00Z".to_owned(),
                notes: None,
                components,
                downloads: Vec::new(),
                min_setup_version: None,
                key_id: "test-key".to_owned(),
            },
            indexes,
        }
    }

    fn install(
        root: &InstallRoot,
        release: &Release,
        source: &mut CountingSource,
    ) -> FormatResult<PathBuf> {
        let lock = root.lock()?;
        let prepared =
            Assembly::new(root, &lock, &release.manifest, release.indexes.clone())?.prepare()?;
        let dir = prepared.complete(source)?;
        root.switch_to(&lock, &release.manifest.version)?;
        Ok(dir)
    }

    fn read(dir: &Path, path: &str) -> Vec<u8> {
        std::fs::read(native(dir, &RelativePath::new(path).expect("path"))).expect("read")
    }

    #[test]
    fn an_update_fetches_only_changed_contents() {
        let temp = tempfile::tempdir().expect("temp dir");
        let objects = temp.path().join("objects");
        let root = InstallRoot::new(temp.path().join("install"));
        let mut published = HashMap::new();
        let core = noise(1, 3_000);
        let shared = noise(2, 3_000);
        let old_tool = noise(3, 3_000);
        let new_tool = noise(4, 3_000);
        let first = publish(
            &objects,
            &mut published,
            "0.1.3",
            &[
                ("core", "bin/core", &core),
                ("core", "lib/shared", &shared),
                ("tools", "tools/tool", &old_tool),
                ("tools", "tools/copy-of-shared", &shared),
            ],
        );
        let mut source = CountingSource::new(&objects);
        let dir = install(&root, &first, &mut source).expect("first install");
        assert_eq!(read(&dir, "tools/copy-of-shared"), shared);
        let record = InstallRecord::read(&dir).expect("record");
        assert_eq!(record.components.len(), 2);
        assert_eq!(record.entries.len(), 4);

        let second = publish(
            &objects,
            &mut published,
            "0.1.4",
            &[
                ("core", "bin/core", &core),
                ("core", "lib/shared", &shared),
                ("tools", "tools/tool", &new_tool),
            ],
        );
        let mut source = CountingSource::new(&objects);
        let dir = install(&root, &second, &mut source).expect("update");
        assert_eq!(source.requests, 1);
        assert!(source.bytes < 4_096, "only the changed frame is fetched");
        assert_eq!(read(&dir, "tools/tool"), new_tool);
        assert_eq!(read(&dir, "bin/core"), core);
        assert!(!dir.join("tools").join("copy-of-shared").exists());
        assert_eq!(
            root.current_version().expect("current"),
            Some("0.1.4".parse().expect("version"))
        );
    }

    #[test]
    fn a_changed_installed_file_is_not_reused() {
        let temp = tempfile::tempdir().expect("temp dir");
        let objects = temp.path().join("objects");
        let root = InstallRoot::new(temp.path().join("install"));
        let mut published = HashMap::new();
        let core = noise(1, 3_000);
        let first = publish(
            &objects,
            &mut published,
            "0.1.3",
            &[("core", "bin/core", &core)],
        );
        let dir = install(&root, &first, &mut CountingSource::new(&objects)).expect("install");
        let mut tampered = core.clone();
        tampered[0] ^= 0xFF;
        let installed = dir.join("bin").join("core");
        std::fs::write(&installed, &tampered).expect("tamper");
        // Make sure the modification time differs from the record even on coarse clocks.
        // 确保即使在粗粒度时钟下修改时间也与记录不同。
        let file = File::options().write(true).open(&installed).expect("open");
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))
            .expect("set time");
        drop(file);

        let second = publish(
            &objects,
            &mut published,
            "0.1.4",
            &[("core", "bin/core", &core)],
        );
        let mut source = CountingSource::new(&objects);
        let dir = install(&root, &second, &mut source).expect("update");
        assert_eq!(source.requests, 1);
        assert_eq!(read(&dir, "bin/core"), core);
    }

    #[test]
    fn an_interrupted_assembly_resumes_with_what_it_kept() {
        let temp = tempfile::tempdir().expect("temp dir");
        let objects = temp.path().join("objects");
        let root = InstallRoot::new(temp.path().join("install"));
        let mut published = HashMap::new();
        let contents: Vec<Vec<u8>> = (0..4).map(|seed| noise(seed, 3_000)).collect();
        let files: Vec<(&str, String, &[u8])> = contents
            .iter()
            .enumerate()
            .map(|(position, content)| ("core", format!("file{position}"), content.as_slice()))
            .collect();
        let files: Vec<(&str, &str, &[u8])> = files
            .iter()
            .map(|(component, path, content)| (*component, path.as_str(), *content))
            .collect();
        let release = publish(&objects, &mut published, "0.1.3", &files);
        let version = release.manifest.version.clone();
        std::fs::create_dir_all(root.staging_dir(&version)).expect("staging");
        let stray = root.staging_dir(&version).join("stray");
        std::fs::write(&stray, b"left over").expect("stray");

        let lock = root.lock().expect("lock");
        let prepared = Assembly::new(&root, &lock, &release.manifest, release.indexes.clone())
            .expect("assembly")
            .prepare()
            .expect("prepare");
        assert!(!stray.exists());
        let full = prepared.download_bytes();
        assert!(prepared.request_count() > 1);
        let mut failing = CountingSource::new(&objects);
        failing.fail_after = Some(1);
        assert!(prepared.complete(&mut failing).is_err());
        assert!(!root.version_dir(&version).exists());

        let prepared = Assembly::new(&root, &lock, &release.manifest, release.indexes.clone())
            .expect("assembly")
            .prepare()
            .expect("prepare again");
        assert!(prepared.download_bytes() < full);
        let dir = prepared
            .complete(&mut CountingSource::new(&objects))
            .expect("resume");
        for (position, content) in contents.iter().enumerate() {
            assert_eq!(&read(&dir, &format!("file{position}")), content);
        }
    }

    #[test]
    fn corrupt_packs_leave_no_version_and_no_partial_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        let objects = temp.path().join("objects");
        let root = InstallRoot::new(temp.path().join("install"));
        let mut published = HashMap::new();
        let core = noise(1, 3_000);
        let release = publish(
            &objects,
            &mut published,
            "0.1.3",
            &[("core", "bin/core", &core)],
        );
        let location = published[&Sha256Digest::of(&core)];
        let pack_path = objects.join(pack_key(location.pack));
        let mut pack = std::fs::read(&pack_path).expect("pack");
        let middle = (location.offset + location.length / 2) as usize;
        pack[middle] ^= 0xFF;
        std::fs::write(&pack_path, pack).expect("corrupt");

        let error = install(&root, &release, &mut CountingSource::new(&objects));
        assert!(error.is_err());
        let version = release.manifest.version;
        assert!(!root.version_dir(&version).exists());
        assert!(!root.staging_dir(&version).join("bin").join("core").exists());
    }

    #[test]
    fn components_must_belong_to_the_manifest_and_not_collide() {
        let temp = tempfile::tempdir().expect("temp dir");
        let objects = temp.path().join("objects");
        let root = InstallRoot::new(temp.path().join("install"));
        let lock = root.lock().expect("lock");
        let mut published = HashMap::new();
        let release = publish(
            &objects,
            &mut published,
            "0.1.3",
            &[("core", "bin/tool", b"one"), ("tools", "bin/TOOL", b"two")],
        );
        assert!(Assembly::new(&root, &lock, &release.manifest, release.indexes.clone()).is_err());
        let mut foreign = release.manifest.clone();
        foreign.components.clear();
        assert!(Assembly::new(&root, &lock, &foreign, release.indexes[..1].to_vec()).is_err());
        assert!(Assembly::new(&root, &lock, &release.manifest, Vec::new()).is_err());
    }

    fn frame(offset: u64, length: u64) -> ((Sha256Digest, u64), FrameTarget) {
        (
            (Sha256Digest::of(b"pack"), offset),
            FrameTarget {
                offset,
                length,
                size: length,
                sha256: Sha256Digest::of(&offset.to_le_bytes()),
                paths: Vec::new(),
            },
        )
    }

    fn spans(frames: &[(u64, u64)]) -> Vec<(u64, u64)> {
        plan_ranges(
            frames
                .iter()
                .map(|(offset, length)| frame(*offset, *length))
                .collect(),
        )
        .iter()
        .map(|range| (range.offset, range.length))
        .collect()
    }

    #[test]
    fn ranges_merge_close_frames_and_dense_packs() {
        let gap = RANGE_MERGE_GAP_BYTES;
        // Sparse frames: only those within the gap share a request.
        // 稀疏的帧：只有间隔内的帧共用请求。
        assert_eq!(
            spans(&[(0, 10), (10 + gap, 10), (100 * gap, 10)]),
            vec![(0, 20 + gap), (100 * gap, 10)]
        );
        // Needed bytes above half the span: one request despite a wide gap.
        // 所需字节超过跨度的一半：即使间隔很大也只用一个请求。
        assert_eq!(
            spans(&[(0, 4 * gap), (6 * gap, 4 * gap)]),
            vec![(0, 10 * gap)]
        );
    }
}
