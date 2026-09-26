//! Snapshot/probe/commit delivery and backend reconciliation with no filesystem work under the state lock.
//! 快照、探测、提交式交付及后端核对；状态锁下不进行文件系统操作。

use super::routing::WatchRoute;
use super::{
    LiveFileWatcher, LiveFileWatcherInner, LiveWatchPath, LiveWatchSender, PathWatchCounts,
    SubscriberWatchState, WatchState, actual_watch_path, changed_path_for_event,
};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};

pub(super) type Delivery = (LiveWatchSender, Vec<PathBuf>, Vec<PathBuf>);

/// Reduces pure logical reference counts and remembers only paths whose effective mode changed.
/// 减少纯逻辑引用计数，仅记录有效模式发生变化的路径。
pub(super) fn remove_counts(
    counts: &mut HashMap<PathBuf, PathWatchCounts>,
    actual: &LiveWatchPath,
    amount: usize,
    affected: &mut BTreeSet<PathBuf>,
) {
    let Some(entry) = counts.get_mut(&actual.path) else {
        return;
    };
    let previous = entry.effective_mode();
    entry.decrement(actual.recursive, amount);
    if previous != entry.effective_mode() {
        affected.insert(actual.path.clone());
    }
    if entry.is_empty() {
        counts.remove(&actual.path);
    }
}

/// Serializes native backend changes and reads the latest logical goal, never a queued stale goal.
/// 串行化原生后端变更并读取最新逻辑目标，绝不使用排队中的过期目标。
pub(super) fn reconcile_backend(
    state: &RwLock<WatchState>,
    inner: Option<&Arc<Mutex<LiveFileWatcherInner>>>,
    paths: &BTreeSet<PathBuf>,
) {
    if paths.is_empty() {
        return;
    }
    let Some(inner) = inner else {
        return;
    };
    // Every caller has already dropped state ownership. The only nested order is backend -> state.
    // 所有调用方均已释放状态归属；唯一嵌套顺序为后端锁 -> 状态锁。
    let mut guard = Some(
        inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    for path in paths {
        let desired = {
            let state = state
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state
                .path_ref_counts
                .get(path)
                .and_then(|counts| counts.effective_mode())
        };
        LiveFileWatcher::reconfigure_watch_inner(Some(inner), path, desired, &mut guard);
    }
}

struct ProbeInput {
    route: WatchRoute,
    before: SubscriberWatchState,
    events: BTreeSet<PathBuf>,
}

struct ProbeResult {
    route: WatchRoute,
    before: SubscriberWatchState,
    after: SubscriberWatchState,
    changes: BTreeSet<PathBuf>,
}

fn capture(state: &RwLock<WatchState>, paths: &[PathBuf]) -> Vec<ProbeInput> {
    let state = state
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state
        .routing
        .affected(paths)
        .into_iter()
        .filter_map(|(route, events)| {
            let before = state
                .subscribers
                .get(&route.subscriber)?
                .watched_paths
                .get(&route.key)?
                .clone();
            Some(ProbeInput {
                route,
                before,
                events,
            })
        })
        .collect()
}

/// Performs at most one canonicalization per distinct requested watch and one existence check per namespace.
/// 每个不同请求监听最多规范化一次，每个命名空间最多检查一次存在性。
fn probe(inputs: Vec<ProbeInput>) -> Vec<ProbeResult> {
    let mut actual_cache = HashMap::new();
    let mut existence = HashMap::new();
    inputs
        .into_iter()
        .map(|input| {
            let key = &input.route.key;
            let matched_exists = *existence
                .entry(key.matched.path.clone())
                .or_insert_with(|| key.matched.path.exists());
            let requested_exists = *existence
                .entry(key.requested.path.clone())
                .or_insert_with(|| key.requested.path.exists());
            let (actual, _, fallback) = actual_cache
                .entry(key.requested.clone())
                .or_insert_with(|| actual_watch_path(&key.requested));
            let mut after = input.before.clone();
            let mut changes = BTreeSet::new();
            for event in &input.events {
                if let Some(path) =
                    changed_path_for_event(key, &mut after, event, matched_exists, requested_exists)
                {
                    changes.insert(path);
                }
            }
            after.actual = actual.clone();
            after.fallback |= *fallback;
            ProbeResult {
                route: input.route,
                before: input.before,
                after,
                changes,
            }
        })
        .collect()
}

fn commit(
    state: &RwLock<WatchState>,
    probes: Vec<ProbeResult>,
) -> (Vec<Delivery>, BTreeSet<PathBuf>) {
    let mut state = state
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut notifications = HashMap::<u64, BTreeSet<PathBuf>>::new();
    let mut affected = BTreeSet::new();
    for probe in probes {
        let Some(subscriber) = state.subscribers.get_mut(&probe.route.subscriber) else {
            continue;
        };
        let Some(current) = subscriber.watched_paths.get_mut(&probe.route.key) else {
            continue;
        };
        if !Arc::ptr_eq(&current.identity, &probe.before.identity) {
            continue;
        }
        if current.actual != probe.before.actual
            || current.last_exists != probe.before.last_exists
            || current.fallback != probe.before.fallback
        {
            // Another delivered observation won; never overwrite it, but conservatively invalidate this root.
            // 另一条已交付观察优先生效；绝不覆盖它，而是保守地使本根失效。
            notifications
                .entry(probe.route.subscriber)
                .or_default()
                .insert(probe.route.key.requested.path);
            continue;
        }
        let old_actual = current.actual.clone();
        let count = current.count;
        current.actual = probe.after.actual.clone();
        current.last_exists = probe.after.last_exists;
        current.fallback = probe.after.fallback;
        if old_actual != probe.after.actual {
            remove_counts(
                &mut state.path_ref_counts,
                &old_actual,
                count,
                &mut affected,
            );
            let actual = &probe.after.actual;
            let counts = state
                .path_ref_counts
                .entry(actual.path.clone())
                .or_default();
            let previous = counts.effective_mode();
            counts.increment(actual.recursive, count);
            if previous != counts.effective_mode() {
                affected.insert(actual.path.clone());
            }
        }
        if !probe.changes.is_empty() {
            notifications
                .entry(probe.route.subscriber)
                .or_default()
                .extend(probe.changes);
        }
    }
    let deliveries = notifications
        .into_iter()
        .filter_map(|(id, paths)| {
            let subscriber = state.subscribers.get(&id)?;
            let roots = subscriber
                .watched_paths
                .keys()
                .map(|key| key.requested.path.clone())
                .collect();
            Some((subscriber.tx.clone(), paths.into_iter().collect(), roots))
        })
        .collect();
    (deliveries, affected)
}

/// Owns one finite delivery batch through its native backend reconciliation.
/// 持有单个有限交付批次，直至其原生后端核对完成。
pub(super) fn prepare_delivery(
    state: &RwLock<WatchState>,
    inner: Option<&Arc<Mutex<LiveFileWatcherInner>>>,
    paths: &[PathBuf],
) -> Vec<Delivery> {
    let inputs = capture(state, paths);
    let observed = probe(inputs);
    let (deliveries, affected) = commit(state, observed);
    reconcile_backend(state, inner, &affected);
    deliveries
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Removed and recreated registrations reject observations from the former identity.
    /// 已移除再创建的注册拒绝先前身份的观察。
    #[test]
    fn old_probe_cannot_mutate_a_recreated_registration() {
        let watcher = Arc::new(LiveFileWatcher::noop());
        let (subscriber, _receiver) = watcher.add_subscriber();
        let root = std::fs::canonicalize(std::env::temp_dir()).expect("temporary root");
        let first = subscriber.register_path(root.clone(), true);
        let inputs = capture(&watcher.state, &[root.clone()]);
        assert_eq!(inputs.len(), 1);
        // Filesystem work occurs after capture's read guard has been released.
        // 捕获阶段的读守卫释放后，才进行文件系统操作。
        assert!(watcher.state.try_write().is_ok());
        let observed = probe(inputs);
        drop(first);
        let _second = subscriber.register_path(root.clone(), true);
        let (deliveries, affected) = commit(&watcher.state, observed);
        assert!(deliveries.is_empty() && affected.is_empty());
        assert_eq!(watcher.watch_counts_for_test(&root), Some((0, 1)));
    }

    /// Shared watches are probed once per namespace and keep refcounts when one owner leaves.
    /// 共享监听按命名空间探测，单个所有者退出时保留其他引用。
    #[tokio::test]
    async fn indexed_delivery_preserves_shared_root_lifetimes() {
        let watcher = Arc::new(LiveFileWatcher::noop());
        let root = std::fs::canonicalize(std::env::temp_dir()).expect("temporary root");
        let (first, mut first_rx) = watcher.add_subscriber();
        let (second, mut second_rx) = watcher.add_subscriber();
        let registration = first.register_path(root.clone(), true);
        let _other = second.register_path(root.clone(), true);
        let child = root.join("indexed-observation.txt");
        watcher.send_paths_for_test(vec![child.clone()]).await;
        assert_eq!(
            first_rx.recv().await.expect("first delivery").paths,
            vec![child.clone()]
        );
        assert_eq!(
            second_rx.recv().await.expect("second delivery").paths,
            vec![child.clone()]
        );
        drop(registration);
        assert_eq!(watcher.watch_counts_for_test(&root), Some((0, 1)));
        watcher.send_paths_for_test(vec![child.clone()]).await;
        assert_eq!(
            second_rx.recv().await.expect("remaining delivery").paths,
            vec![child]
        );
    }
}
