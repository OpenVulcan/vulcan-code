//! Bounded subscriber-local invalidations with conservative root fallback.
//! 订阅者本地的有界失效集合，超限后保守回退到注册根。

use super::LiveFileWatchEvent;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Maximum retained exact paths before a subscriber receives a root invalidation instead.
/// 超过该精确路径数量后，向订阅者返回根失效通知。
const MAX_PENDING_PATHS: usize = 4_096;
/// Maximum combined path bytes before degrading to the bounded registration set.
/// 超过路径总字节限制后，退化为由注册集合界定的根路径。
const MAX_PENDING_PATH_BYTES: usize = 512 * 1_024;

/// Bounded changes retaining whether every observation proves a non-structural modification.
/// 有界变更集合，同时保留各路径的每次观察是否都能证明非结构性修改。
#[derive(Default)]
pub(super) struct PendingPaths {
    /// Each path is stored once; false dominates when any structural or root event arrives.
    /// 每条路径只存一次；任一结构性事件或根事件到达时，false 优先。
    paths: BTreeMap<PathBuf, bool>,
    bytes: usize,
    roots_only: bool,
}

impl PendingPaths {
    /// Returns true only when this batch first loses exact-path precision.
    /// 仅当本批次首次失去精确路径粒度时返回真。
    /// `paths` adds observations, `roots` defines recovery coverage, and `modification_only` is positive evidence.
    /// `paths` 增加观察，`roots` 定义恢复覆盖范围，`modification_only` 提供肯定证据。
    pub(super) fn add(
        &mut self,
        paths: &[PathBuf],
        roots: &[PathBuf],
        modification_only: bool,
    ) -> bool {
        if self.roots_only {
            self.replace_roots(roots);
            return false;
        }
        for path in paths {
            // Registered roots can represent recovery or routing reconciliation, never metadata noise.
            // 注册根可能表示恢复或路由核对，绝不能作为属性噪声丢弃。
            let path_modification_only = modification_only && !roots.contains(path);
            if let Some(previous_modification_only) = self.paths.get_mut(path) {
                *previous_modification_only &= path_modification_only;
                continue;
            }
            let bytes = self.bytes.checked_add(path.as_os_str().len());
            if self.paths.len() >= MAX_PENDING_PATHS
                || bytes.is_none_or(|bytes| bytes > MAX_PENDING_PATH_BYTES)
            {
                self.replace_roots(roots);
                self.roots_only = true;
                return true;
            }
            self.bytes = bytes.expect("checked path-byte addition");
            self.paths.insert(path.clone(), path_modification_only);
        }
        false
    }

    fn replace_roots(&mut self, roots: &[PathBuf]) {
        // Retained memory depends on live subscriptions, never the number of subsequent edits.
        // 保留内存取决于活跃订阅，不随之后的编辑次数增长。
        self.paths = roots.iter().cloned().map(|root| (root, false)).collect();
        self.bytes = 0;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Drains one sorted batch, preserving modification evidence only for paths without stronger events.
    /// 排序取出一批事件，仅为没有更强事件的路径保留修改证据。
    pub(super) fn take(&mut self) -> LiveFileWatchEvent {
        self.bytes = 0;
        self.roots_only = false;
        // Split the bounded map only at delivery; pending path storage is not duplicated.
        // 仅在投递时拆分有界映射；不重复保存待处理路径。
        let changes = std::mem::take(&mut self.paths);
        LiveFileWatchEvent {
            modification_only_paths: changes
                .iter()
                .filter(|(_, modification_only)| **modification_only)
                .map(|(path, _)| path.clone())
                .collect(),
            paths: changes.into_keys().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Structural observations dominate modifications in either arrival order, including later repeats.
    /// 结构性观察在任意到达顺序下优先于修改，包括之后重复到达的修改。
    #[test]
    fn coalescing_preserves_structural_changes_and_root_invalidations() {
        // Keep the same path across observations to exercise deduplication rather than separate entries.
        // 多次观察保持同一路径，以覆盖去重行为而非多个独立条目。
        let root = PathBuf::from("project");
        // A directory can be renamed and then receive metadata updates before the next delivery.
        // 目录可能在下一次投递之前先被重命名，再接收属性更新。
        let directory = root.join(".git/refs/heads");
        for observations in [[true, false, true], [false, true, true]] {
            // Each order starts with an independent pending batch.
            // 每种顺序使用独立的待处理批次。
            let mut pending = PendingPaths::default();
            for modification_only in observations {
                assert!(!pending.add(
                    std::slice::from_ref(&directory),
                    std::slice::from_ref(&root),
                    modification_only,
                ));
            }
            // The delivered directory must not qualify for metadata filtering.
            // 投递出的目录绝不能满足属性过滤条件。
            let event = pending.take();
            assert_eq!(event.paths, vec![directory.clone()]);
            assert!(event.modification_only_paths.is_empty());

            assert!(!pending.add(
                &[root.clone(), directory.clone()],
                std::slice::from_ref(&root),
                true,
            ));
            // Consuming a batch resets its evidence, while explicit roots remain conservative.
            // 消费批次会重置其证据，但显式根通知仍保持保守语义。
            let next = pending.take();
            assert_eq!(next.modification_only_paths, [directory.clone()].into());
            assert!(next.paths.contains(&root));
        }
    }

    /// Overflow drops modification-only evidence so slow receivers cannot suppress a required rescan.
    /// 溢出会丢弃仅修改证据，使慢接收器无法抑制必需的重扫。
    #[test]
    fn overflow_preserves_full_root_invalidation_after_modifications() {
        // Use enough distinct child modifications to cross the existing bounded path capacity.
        // 使用足够多的不同子路径修改，超过既有的有界路径容量。
        let root = PathBuf::from("project");
        // Pending changes use the same production capacity as ordinary file events.
        // 待处理变更与普通文件事件共用生产容量限制。
        let changes = (0..=MAX_PENDING_PATHS)
            .map(|id| root.join(id.to_string()))
            .collect::<Vec<_>>();
        // The queue initially retains exact paths and then degrades to its registered root.
        // 队列最初保留精确路径，随后退化到注册根。
        let mut pending = PendingPaths::default();
        assert!(pending.add(&changes, std::slice::from_ref(&root), true));
        assert!(!pending.add(&[root.join(".git")], std::slice::from_ref(&root), true));
        // A root rescan always wins over directory-noise evidence.
        // 根重扫始终优先于目录噪声证据。
        let event = pending.take();
        assert_eq!(event.paths, vec![root]);
        assert!(event.modification_only_paths.is_empty());
    }

    #[test]
    fn slow_consumer_retains_roots_instead_of_unbounded_unique_paths() {
        let roots = vec![PathBuf::from("project-a"), PathBuf::from("project-b")];
        let mut pending = PendingPaths::default();
        let changes = (0..MAX_PENDING_PATHS + 1)
            .map(|id| roots[0].join(id.to_string()))
            .collect::<Vec<_>>();
        assert!(pending.add(&changes, &roots, false));
        for batch in 0..10 {
            assert!(!pending.add(&[roots[1].join(format!("later-{batch}"))], &roots, false));
            assert_eq!(pending.paths.len(), roots.len());
        }
        assert_eq!(pending.take().paths, roots);
        assert!(!pending.add(&[PathBuf::from("project-a/new")], &roots, false));
        assert_eq!(pending.take().paths, vec![PathBuf::from("project-a/new")]);
    }

    #[test]
    fn path_bytes_and_registration_churn_are_bounded() {
        let roots = vec![PathBuf::from("root")];
        let mut pending = PendingPaths::default();
        assert!(pending.add(
            &[PathBuf::from("x".repeat(MAX_PENDING_PATH_BYTES + 1))],
            &roots,
            false,
        ));
        for id in 0..100 {
            let current = vec![PathBuf::from(format!("root-{id}"))];
            assert!(!pending.add(&[current[0].join("file")], &current, false));
            assert_eq!(pending.paths.len(), 1);
        }
        assert_eq!(pending.take().paths, vec![PathBuf::from("root-99")]);
    }
}
