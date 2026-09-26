//! Bounded subscriber-local invalidations with conservative root fallback.
//! 订阅者本地的有界失效集合，超限后保守回退到注册根。

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Maximum retained exact paths before a subscriber receives a root invalidation instead.
/// 超过该精确路径数量后，向订阅者返回根失效通知。
const MAX_PENDING_PATHS: usize = 4_096;
/// Maximum combined path bytes before degrading to the bounded registration set.
/// 超过路径总字节限制后，退化为由注册集合界定的根路径。
const MAX_PENDING_PATH_BYTES: usize = 512 * 1_024;

#[derive(Default)]
pub(super) struct PendingPaths {
    paths: BTreeSet<PathBuf>,
    bytes: usize,
    roots_only: bool,
}

impl PendingPaths {
    /// Returns true only when this batch first loses exact-path precision.
    /// 仅当本批次首次失去精确路径粒度时返回真。
    pub(super) fn add(&mut self, paths: &[PathBuf], roots: &[PathBuf]) -> bool {
        if self.roots_only {
            self.replace_roots(roots);
            return false;
        }
        for path in paths {
            if self.paths.contains(path) {
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
            self.paths.insert(path.clone());
        }
        false
    }

    fn replace_roots(&mut self, roots: &[PathBuf]) {
        // Retained memory depends on live subscriptions, never the number of subsequent edits.
        // 保留内存取决于活跃订阅，不随之后的编辑次数增长。
        self.paths = roots.iter().cloned().collect();
        self.bytes = 0;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub(super) fn take(&mut self) -> Vec<PathBuf> {
        self.bytes = 0;
        self.roots_only = false;
        std::mem::take(&mut self.paths).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_consumer_retains_roots_instead_of_unbounded_unique_paths() {
        let roots = vec![PathBuf::from("project-a"), PathBuf::from("project-b")];
        let mut pending = PendingPaths::default();
        let changes = (0..MAX_PENDING_PATHS + 1)
            .map(|id| roots[0].join(id.to_string()))
            .collect::<Vec<_>>();
        assert!(pending.add(&changes, &roots));
        for batch in 0..10 {
            assert!(!pending.add(&[roots[1].join(format!("later-{batch}"))], &roots));
            assert_eq!(pending.paths.len(), roots.len());
        }
        assert_eq!(pending.take(), roots);
        assert!(!pending.add(&[PathBuf::from("project-a/new")], &roots));
        assert_eq!(pending.take(), vec![PathBuf::from("project-a/new")]);
    }

    #[test]
    fn path_bytes_and_registration_churn_are_bounded() {
        let roots = vec![PathBuf::from("root")];
        let mut pending = PendingPaths::default();
        assert!(pending.add(
            &[PathBuf::from("x".repeat(MAX_PENDING_PATH_BYTES + 1))],
            &roots
        ));
        for id in 0..100 {
            let current = vec![PathBuf::from(format!("root-{id}"))];
            assert!(!pending.add(&[current[0].join("file")], &current));
            assert_eq!(pending.paths.len(), 1);
        }
        assert_eq!(pending.take(), vec![PathBuf::from("root-99")]);
    }
}
