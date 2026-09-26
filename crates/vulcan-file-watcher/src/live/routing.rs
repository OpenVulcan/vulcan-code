//! Prefix-indexed routing without per-event scans of unrelated registrations.
//! 使用前缀索引路由，不逐事件扫描无关注册。

use super::{SubscriberId, SubscriberWatchKey};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
};

/// Identifies a single subscriber's watch in either path namespace.
/// 在任一命名空间标识单个订阅者的监听。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct WatchRoute {
    pub subscriber: SubscriberId,
    pub key: SubscriberWatchKey,
}

/// Indexes path components rather than string prefixes, so `a` cannot match `ab`.
/// 索引路径组件而非字符串前缀，避免 `a` 匹配 `ab`。
#[derive(Default)]
pub(super) struct WatchRoutingIndex {
    paths: BTreeMap<PathBuf, HashSet<WatchRoute>>,
}

impl WatchRoutingIndex {
    /// Both original and canonical paths refer to the same logical owner.
    /// 原路径与规范路径都指向同一逻辑所有者。
    pub fn insert(&mut self, subscriber: SubscriberId, key: &SubscriberWatchKey) {
        let route = WatchRoute {
            subscriber,
            key: key.clone(),
        };
        for path in [&key.requested.path, &key.matched.path] {
            self.paths
                .entry(path.clone())
                .or_default()
                .insert(route.clone());
        }
    }

    /// Removes only this logical route, preserving shared roots used by other subscribers.
    /// 仅移除当前逻辑路由，保留其他订阅者使用的共享根。
    pub fn remove(&mut self, subscriber: SubscriberId, key: &SubscriberWatchKey) {
        let route = WatchRoute {
            subscriber,
            key: key.clone(),
        };
        for path in [&key.requested.path, &key.matched.path] {
            if let Some(routes) = self.paths.get_mut(path) {
                routes.remove(&route);
                if routes.is_empty() {
                    self.paths.remove(path);
                }
            }
        }
    }

    /// Returns only ancestor or descendant watches for the supplied changed paths.
    /// 仅返回给定变化路径的祖先或后代监听。
    pub fn affected(&self, events: &[PathBuf]) -> HashMap<WatchRoute, BTreeSet<PathBuf>> {
        let mut result = HashMap::<WatchRoute, BTreeSet<PathBuf>>::new();
        for event in events {
            for ancestor in event.ancestors() {
                if let Some(routes) = self.paths.get(ancestor) {
                    Self::include(&mut result, routes, event);
                }
            }
            // PathBuf ordering follows components; all descendants form one contiguous range.
            // PathBuf 按组件排序；全部后代构成连续区间。
            for (_, routes) in self
                .paths
                .range(event.clone()..)
                .take_while(|(path, _)| path.starts_with(event))
            {
                Self::include(&mut result, routes, event);
            }
        }
        result
    }

    fn include(
        result: &mut HashMap<WatchRoute, BTreeSet<PathBuf>>,
        routes: &HashSet<WatchRoute>,
        event: &Path,
    ) {
        for route in routes {
            result
                .entry(route.clone())
                .or_default()
                .insert(event.to_path_buf());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::LiveWatchPath;
    use super::*;

    fn key(path: PathBuf) -> SubscriberWatchKey {
        let requested = LiveWatchPath {
            path,
            recursive: true,
        };
        SubscriberWatchKey {
            matched: requested.clone(),
            requested,
        }
    }

    /// Thousands of sibling roots must not enter an unrelated event's probe set.
    /// 数千个同级根不得进入无关事件的探测集合。
    #[test]
    fn routing_limits_probes_to_related_components() {
        let root = std::env::temp_dir().join("watch-route-fixture");
        let mut index = WatchRoutingIndex::default();
        for id in 0..10_000 {
            index.insert(id, &key(root.join(format!("r{id:05}"))));
        }
        let event = root.join("r05000/file.txt");
        let affected = index.affected(&[event.clone()]);
        assert_eq!(affected.len(), 1);
        assert_eq!(
            affected.values().next().expect("one owner"),
            &BTreeSet::from([event])
        );
        assert!(
            index
                .affected(&[root.join("r05000suffix/file.txt")])
                .is_empty()
        );
        assert_eq!(index.affected(&[root.clone()]).len(), 10_000);
    }

    /// Alias namespaces, shared roots and removed registrations keep exact ownership.
    /// 别名命名空间、共享根与已移除注册保持精确归属。
    #[test]
    fn aliases_and_shared_roots_are_deduplicated_by_owner() {
        let root = std::env::temp_dir().join("watch-alias-fixture");
        let mut original = key(root.join("logical"));
        original.matched.path = root.join("physical");
        let mut index = WatchRoutingIndex::default();
        index.insert(1, &original);
        index.insert(2, &original);
        assert_eq!(
            index
                .affected(&[root.join("logical/x"), root.join("physical/x")])
                .len(),
            2
        );
        index.remove(1, &original);
        let affected = index.affected(&[root.join("physical")]);
        assert_eq!(affected.len(), 1);
        assert_eq!(
            affected.keys().next().expect("remaining owner").subscriber,
            2
        );
        index.remove(2, &original);
        assert!(index.paths.is_empty());
    }
}
