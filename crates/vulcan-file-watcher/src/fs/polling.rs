use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::event::{FileWatchEvent, FileWatchEventKind};
use crate::traits::{FileWatchTarget, FileWatcher};

/// Metadata snapshot used by the polling backend.
/// 轮询后端使用的元数据快照。
#[derive(Debug, Clone, PartialEq, Eq)]
struct MetadataSnapshot {
    /// Whether the path is a directory.
    /// 路径是否为目录。
    is_dir: bool,
    /// File length in bytes.
    /// 文件字节长度。
    len: u64,
    /// Modified timestamp in nanoseconds since the Unix epoch when available.
    /// 可用时的 Unix 纪元后纳秒级修改时间戳。
    modified_nanos: u128,
}

/// Filesystem polling watcher.
/// 文件系统轮询监听器。
#[derive(Debug, Default)]
pub struct PollingFileWatcher {
    /// Registered watch targets.
    /// 已注册监听目标。
    targets: Vec<FileWatchTarget>,
    /// Last known path snapshots.
    /// 最近已知路径快照。
    snapshots: BTreeMap<PathBuf, MetadataSnapshot>,
    /// Pending events collected by the latest poll.
    /// 最近轮询收集的待处理事件。
    events: Vec<FileWatchEvent>,
}

impl PollingFileWatcher {
    /// Create one empty polling watcher.
    /// 创建一个空的轮询监听器。
    pub fn new() -> Self {
        Self::default()
    }

    /// Poll every registered target and enqueue change events.
    /// 轮询全部已注册目标并排入变更事件。
    pub fn poll(&mut self) -> Result<(), String> {
        let next = collect_targets_snapshot(&self.targets)?;
        let previous_paths = self.snapshots.keys().cloned().collect::<BTreeSet<_>>();
        let next_paths = next.keys().cloned().collect::<BTreeSet<_>>();

        for path in next_paths.difference(&previous_paths) {
            self.events.push(FileWatchEvent {
                kind: FileWatchEventKind::Created,
                path: path.clone(),
                previous_path: None,
            });
        }
        for path in previous_paths.difference(&next_paths) {
            self.events.push(FileWatchEvent {
                kind: FileWatchEventKind::Removed,
                path: path.clone(),
                previous_path: None,
            });
        }
        for path in next_paths.intersection(&previous_paths) {
            let Some(previous) = self.snapshots.get(path) else {
                continue;
            };
            let Some(current) = next.get(path) else {
                continue;
            };
            if previous != current {
                self.events.push(FileWatchEvent {
                    kind: FileWatchEventKind::Modified,
                    path: path.clone(),
                    previous_path: None,
                });
            }
        }

        self.snapshots = next;
        Ok(())
    }

    /// Return the number of registered targets.
    /// 返回已注册目标数量。
    pub fn target_count(&self) -> usize {
        self.targets.len()
    }
}

impl FileWatcher for PollingFileWatcher {
    /// Register one target and seed its initial snapshot.
    /// 注册一个目标并初始化其初始快照。
    fn watch(&mut self, target: FileWatchTarget) -> Result<(), String> {
        self.targets.push(target);
        self.snapshots = collect_targets_snapshot(&self.targets)?;
        Ok(())
    }

    /// Drain all buffered file events.
    /// 取出全部缓冲文件事件。
    fn drain_events(&mut self) -> Vec<FileWatchEvent> {
        std::mem::take(&mut self.events)
    }
}

/// Collect snapshots for all configured targets.
/// 收集全部配置目标的快照。
fn collect_targets_snapshot(
    targets: &[FileWatchTarget],
) -> Result<BTreeMap<PathBuf, MetadataSnapshot>, String> {
    let mut snapshots = BTreeMap::new();
    for target in targets {
        collect_target_snapshot(target, &mut snapshots)?;
    }
    Ok(snapshots)
}

/// Collect snapshots for one target.
/// 收集单个目标的快照。
fn collect_target_snapshot(
    target: &FileWatchTarget,
    snapshots: &mut BTreeMap<PathBuf, MetadataSnapshot>,
) -> Result<(), String> {
    let path = target.path.clone();
    if !path.exists() {
        return Ok(());
    }
    collect_path_snapshot(&path, target.recursive, snapshots)
}

/// Collect snapshots for one path.
/// 收集单个路径的快照。
fn collect_path_snapshot(
    path: &Path,
    recursive: bool,
    snapshots: &mut BTreeMap<PathBuf, MetadataSnapshot>,
) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    snapshots.insert(path.to_path_buf(), metadata_snapshot(&metadata));
    if metadata.is_dir() {
        let read_dir =
            fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))?;
        for entry in read_dir {
            let entry = entry.map_err(|error| format!("{}: {error}", path.display()))?;
            let child_path = entry.path();
            let child_metadata = entry
                .metadata()
                .map_err(|error| format!("{}: {error}", child_path.display()))?;
            snapshots.insert(child_path.clone(), metadata_snapshot(&child_metadata));
            if recursive && child_metadata.is_dir() {
                collect_path_snapshot(&child_path, recursive, snapshots)?;
            }
        }
    }
    Ok(())
}

/// Convert filesystem metadata into a stable comparable snapshot.
/// 将文件系统元数据转换为稳定可比较快照。
fn metadata_snapshot(metadata: &fs::Metadata) -> MetadataSnapshot {
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    MetadataSnapshot {
        is_dir: metadata.is_dir(),
        len: metadata.len(),
        modified_nanos,
    }
}
