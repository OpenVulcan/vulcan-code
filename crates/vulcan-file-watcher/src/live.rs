//! Live multi-subscriber watcher built on top of `notify`.
//! 基于 `notify` 的实时多订阅文件监听器。

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use notify::Event;
use notify::EventKind;
use notify::RecommendedWatcher;
use notify::RecursiveMode;
use notify::Watcher;
use tokio::runtime::Handle;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio::time::sleep_until;
use tracing::warn;

mod dispatch;
/// Registration-scoped read-only observation evidence.
/// 注册范围的只读观察证据。
mod health;
mod pending_paths;
mod raw_events;
mod routing;
pub use health::LiveWatchObservationHealth;
use health::{LiveWatchHealthState, LiveWatchLoopGuard};

/// Coalesced file change notification for one subscriber.
/// 单个订阅者收到的合并文件变更通知。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveFileWatchEvent {
    /// Changed paths in sorted order with duplicates removed. A root invalidates all its descendants.
    /// 已排序去重的变更路径；根路径表示需要重新核对该根的全部后代。
    pub paths: Vec<PathBuf>,
}

/// Path subscription requested by a live watcher subscriber.
/// 实时监听订阅者请求的路径订阅。
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LiveWatchPath {
    /// Root file or directory path to watch.
    /// 要监听的根文件或目录路径。
    pub path: PathBuf,
    /// Whether descendant paths should also match.
    /// 是否同时匹配后代路径。
    pub recursive: bool,
}

/// Stable subscriber identifier.
/// 稳定订阅者标识。
type SubscriberId = u64;

/// Mutable live watcher shared state.
/// 实时监听器共享可变状态。
#[derive(Default)]
struct WatchState {
    /// Next subscriber id to allocate.
    /// 下一个待分配订阅者标识。
    next_subscriber_id: SubscriberId,
    /// Ref-counted actual OS watch paths.
    /// 按引用计数维护的真实 OS 监听路径。
    path_ref_counts: HashMap<PathBuf, PathWatchCounts>,
    /// Active logical subscribers.
    /// 当前活跃的逻辑订阅者。
    subscribers: HashMap<SubscriberId, SubscriberState>,
    /// Both logical and canonical namespaces point to exact registration identities.
    /// 逻辑与规范命名空间均指向精确注册身份。
    routing: routing::WatchRoutingIndex,
}

/// State for one logical subscriber.
/// 单个逻辑订阅者状态。
struct SubscriberState {
    /// Registered paths owned by this subscriber.
    /// 此订阅者拥有的已注册路径。
    watched_paths: HashMap<SubscriberWatchKey, SubscriberWatchState>,
    /// Sender used to deliver coalesced batches.
    /// 用于投递合并批次的发送器。
    tx: LiveWatchSender,
}

/// Immutable identity for one subscriber watch.
/// 单个订阅者监听项的不可变身份。
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SubscriberWatchKey {
    /// Original path requested by the caller.
    /// 调用方请求的原始路径。
    requested: LiveWatchPath,
    /// Canonical or fallback path namespace used for backend matching.
    /// 用于后端匹配的规范化或回退路径命名空间。
    matched: LiveWatchPath,
}

/// Mutable state for one subscriber watch.
/// 单个订阅者监听项的可变状态。
#[derive(Clone)]
struct SubscriberWatchState {
    /// A removed and recreated watch must never accept the old watch's in-flight probe.
    /// 已删除再创建的监听绝不能接受旧监听的进行中探测。
    identity: Arc<()>,
    /// Existing path currently passed to the OS watcher.
    /// 当前传给 OS 监听器的已存在路径。
    actual: LiveWatchPath,
    /// Number of identical registrations held by this subscriber.
    /// 此订阅者持有的相同注册数量。
    count: usize,
    /// Whether the requested path existed after the previous ancestor event.
    /// 上一次祖先事件后请求路径是否存在。
    last_exists: bool,
    /// Whether the watch began from a missing target fallback.
    /// 该监听是否从缺失目标回退开始。
    fallback: bool,
}

/// Registration-time data before merging into subscriber state.
/// 合并进订阅者状态前的注册期数据。
#[derive(Clone)]
struct SubscriberWatchRegistration {
    /// Stable subscriber-visible key.
    /// 对订阅者可见的稳定键。
    key: SubscriberWatchKey,
    /// Existing path initially watched by the backend.
    /// 后端最初监听的已存在路径。
    actual: LiveWatchPath,
    /// Exact existence observation captured before shared state is locked.
    /// 获取共享状态锁之前捕获的精确存在性观察。
    last_exists: bool,
    /// Whether registration used a missing-path fallback.
    /// 注册是否使用了缺失路径回退。
    fallback: bool,
}

/// Receives coalesced change notifications for one subscriber.
/// 接收单个订阅者的合并变更通知。
pub struct LiveWatchReceiver {
    /// Shared receiver state.
    /// 共享接收器状态。
    inner: Arc<ReceiverInner>,
}

/// Sends changed paths into one receiver.
/// 向单个接收器发送变更路径。
struct LiveWatchSender {
    /// Shared receiver state.
    /// 共享接收器状态。
    inner: Arc<ReceiverInner>,
}

/// Shared receiver state.
/// 共享接收器状态。
struct ReceiverInner {
    /// Buffered changed paths waiting to be emitted.
    /// 等待发出的缓冲变更路径。
    changed_paths: AsyncMutex<pending_paths::PendingPaths>,
    /// Records degradation immediately, before a slow subscriber consumes its root invalidation.
    /// 慢订阅者消费根失效通知之前，立即记录退化证据。
    health: Arc<LiveWatchHealthState>,
    /// Notification used to wake receivers.
    /// 用于唤醒接收器的通知对象。
    notify: Notify,
    /// Number of senders still alive.
    /// 仍然存活的发送器数量。
    sender_count: AtomicUsize,
}

impl LiveWatchReceiver {
    /// Wait for the next coalesced file-watch batch.
    /// 等待下一批合并后的文件监听事件。
    pub async fn recv(&mut self) -> Option<LiveFileWatchEvent> {
        loop {
            let notified = self.inner.notify.notified();
            tokio::pin!(notified);
            // Register before checking buffered paths and sender liveness because the last-sender
            // path uses notify_waiters, which does not retain a permit for future waiters.
            // 在检查缓冲路径与发送者存活状态前完成注册，因为最后一个发送者使用 notify_waiters，
            // 它不会为未来等待者保留许可。
            notified.as_mut().enable();
            {
                let mut changed_paths = self.inner.changed_paths.lock().await;
                if !changed_paths.is_empty() {
                    return Some(LiveFileWatchEvent {
                        paths: changed_paths.take(),
                    });
                }
                if self.inner.sender_count.load(Ordering::Acquire) == 0 {
                    return None;
                }
            }
            notified.as_mut().await;
        }
    }
}

impl LiveWatchSender {
    /// Bound pending paths and degrade to the exact registered roots rather than dropping changes.
    /// 限制待处理路径数量，超限时退化为精确注册根失效，而不是丢弃变化。
    async fn add_changed_paths(&self, paths: &[PathBuf], roots: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        let mut changed_paths = self.inner.changed_paths.lock().await;
        if changed_paths.add(paths, roots) {
            self.inner.health.record_gap();
        }
        self.inner.notify.notify_one();
    }
}

impl Clone for LiveWatchSender {
    /// Clone the sender and increment the shared sender count.
    /// 克隆发送器并增加共享发送器计数。
    fn clone(&self) -> Self {
        self.inner.sender_count.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Drop for LiveWatchSender {
    /// Drop one sender and close receivers when the last sender disappears.
    /// 丢弃一个发送器，并在最后一个发送器消失时关闭接收器。
    fn drop(&mut self) {
        if self.inner.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.inner.notify.notify_waiters();
        }
    }
}

/// Create one sender and receiver pair.
/// 创建一组发送器和接收器。
fn watch_channel(health: Arc<LiveWatchHealthState>) -> (LiveWatchSender, LiveWatchReceiver) {
    let inner = Arc::new(ReceiverInner {
        changed_paths: AsyncMutex::new(pending_paths::PendingPaths::default()),
        health,
        notify: Notify::new(),
        sender_count: AtomicUsize::new(1),
    });
    (
        LiveWatchSender {
            inner: Arc::clone(&inner),
        },
        LiveWatchReceiver { inner },
    )
}

/// Ref counts for one actual watch path.
/// 单个真实监听路径的引用计数。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PathWatchCounts {
    /// Number of non-recursive registrations.
    /// 非递归注册数量。
    non_recursive: usize,
    /// Number of recursive registrations.
    /// 递归注册数量。
    recursive: usize,
}

impl PathWatchCounts {
    /// Increase one count bucket.
    /// 增加一个计数桶。
    fn increment(&mut self, recursive: bool, amount: usize) {
        if recursive {
            self.recursive += amount;
        } else {
            self.non_recursive += amount;
        }
    }

    /// Decrease one count bucket.
    /// 减少一个计数桶。
    fn decrement(&mut self, recursive: bool, amount: usize) {
        if recursive {
            self.recursive = self.recursive.saturating_sub(amount);
        } else {
            self.non_recursive = self.non_recursive.saturating_sub(amount);
        }
    }

    /// Return the effective backend mode required by current counts.
    /// 返回当前计数所需的有效后端模式。
    fn effective_mode(self) -> Option<RecursiveMode> {
        if self.recursive > 0 {
            Some(RecursiveMode::Recursive)
        } else if self.non_recursive > 0 {
            Some(RecursiveMode::NonRecursive)
        } else {
            None
        }
    }

    /// Return whether every count bucket is empty.
    /// 返回所有计数桶是否为空。
    fn is_empty(self) -> bool {
        self.non_recursive == 0 && self.recursive == 0
    }
}

/// Actual notify backend state.
/// 真实 notify 后端状态。
struct LiveFileWatcherInner {
    /// Read-only observation evidence also updated by actual backend registration failures.
    /// 实际后端注册失败也会更新的只读观察证据。
    health: Arc<LiveWatchHealthState>,
    /// Notify watcher instance.
    /// notify 监听器实例。
    watcher: RecommendedWatcher,
    /// Actual watched paths and their current backend mode.
    /// 真实监听路径及其当前后端模式。
    watched_paths: HashMap<PathBuf, RecursiveMode>,
}

/// Coalesces bursts and emits at most once per interval.
/// 合并突发事件，并按最小间隔发出。
pub struct ThrottledLiveWatchReceiver {
    /// Wrapped raw receiver.
    /// 被包装的原始接收器。
    rx: LiveWatchReceiver,
    /// Minimum interval between emitted batches.
    /// 发出批次之间的最小间隔。
    interval: Duration,
    /// Next time at which a batch may be emitted.
    /// 下一次允许发出批次的时间。
    next_allowed: Option<Instant>,
}

impl ThrottledLiveWatchReceiver {
    /// Create one throttling wrapper.
    /// 创建一个节流包装器。
    pub fn new(rx: LiveWatchReceiver, interval: Duration) -> Self {
        Self {
            rx,
            interval,
            next_allowed: None,
        }
    }

    /// Receive the next throttled batch.
    /// 接收下一批节流后的事件。
    pub async fn recv(&mut self) -> Option<LiveFileWatchEvent> {
        if let Some(next_allowed) = self.next_allowed {
            sleep_until(next_allowed).await;
        }

        let event = self.rx.recv().await;
        if event.is_some() {
            self.next_allowed = Some(Instant::now() + self.interval);
        }
        event
    }
}

/// Handle used to register watched paths for one logical consumer.
/// 用于为单个逻辑消费者注册监听路径的句柄。
pub struct LiveFileWatcherSubscriber {
    /// Subscriber id owned by this handle.
    /// 此句柄拥有的订阅者标识。
    id: SubscriberId,
    /// Shared live watcher.
    /// 共享实时监听器。
    file_watcher: Arc<LiveFileWatcher>,
}

impl LiveFileWatcherSubscriber {
    /// Register paths and return an RAII guard that unregisters on drop.
    /// 注册路径并返回一个在释放时自动注销的 RAII 守卫。
    pub fn register_paths(&self, watched_paths: Vec<LiveWatchPath>) -> LiveWatchRegistration {
        let watched_paths = dedupe_watched_paths(watched_paths)
            .into_iter()
            .map(|requested| {
                let (actual, matched, fallback) = actual_watch_path(&requested);
                let last_exists = matched.path.exists();
                let key = SubscriberWatchKey { requested, matched };
                SubscriberWatchRegistration {
                    key,
                    actual,
                    last_exists,
                    fallback,
                }
            })
            .collect::<Vec<_>>();
        self.file_watcher.register_paths(self.id, &watched_paths);

        LiveWatchRegistration {
            file_watcher: Arc::downgrade(&self.file_watcher),
            subscriber_id: self.id,
            watched_paths: watched_paths
                .iter()
                .map(|watch| watch.key.clone())
                .collect(),
        }
    }

    /// Register one path and return an RAII guard.
    /// 注册单个路径并返回 RAII 守卫。
    pub fn register_path(&self, path: PathBuf, recursive: bool) -> LiveWatchRegistration {
        self.register_paths(vec![LiveWatchPath { path, recursive }])
    }
}

impl Drop for LiveFileWatcherSubscriber {
    /// Remove all registrations owned by this subscriber.
    /// 移除此订阅者拥有的全部注册。
    fn drop(&mut self) {
        self.file_watcher.remove_subscriber(self.id);
    }
}

/// RAII guard for active path registrations.
/// 活跃路径注册的 RAII 守卫。
pub struct LiveWatchRegistration {
    /// Weak watcher reference used during drop.
    /// 释放时使用的弱监听器引用。
    file_watcher: std::sync::Weak<LiveFileWatcher>,
    /// Subscriber id owning the registration.
    /// 拥有该注册的订阅者标识。
    subscriber_id: SubscriberId,
    /// Watched path keys owned by this registration.
    /// 此注册拥有的监听路径键。
    watched_paths: Vec<SubscriberWatchKey>,
}

impl Default for LiveWatchRegistration {
    /// Build an empty inert registration guard.
    /// 构建一个空的惰性注册守卫。
    fn default() -> Self {
        Self {
            file_watcher: std::sync::Weak::new(),
            subscriber_id: 0,
            watched_paths: Vec::new(),
        }
    }
}

impl Drop for LiveWatchRegistration {
    /// Unregister the owned paths when the guard is dropped.
    /// 在守卫释放时注销其拥有的路径。
    fn drop(&mut self) {
        if let Some(file_watcher) = self.file_watcher.upgrade() {
            file_watcher.unregister_paths(self.subscriber_id, &self.watched_paths);
        }
    }
}

/// Multi-subscriber live file watcher.
/// 多订阅实时文件监听器。
pub struct LiveFileWatcher {
    /// Health evidence has the same exact lifetime as this watcher, never a global singleton.
    /// 健康证据与本监听器具有相同精确生命周期，绝非全局单例。
    health: Arc<LiveWatchHealthState>,
    /// Optional live notify backend.
    /// 可选实时 notify 后端。
    inner: Option<Arc<Mutex<LiveFileWatcherInner>>>,
    /// Shared subscription state.
    /// 共享订阅状态。
    state: Arc<RwLock<WatchState>>,
}

impl LiveFileWatcher {
    /// Create a live filesystem watcher and spawn its event loop when possible.
    /// 创建实时文件系统监听器，并在可用时启动事件循环。
    pub fn new() -> notify::Result<Self> {
        let health = Arc::new(LiveWatchHealthState::default());
        let (raw_tx, raw_rx) = raw_events::channel(Arc::clone(&health));
        let watcher = notify::recommended_watcher(move |res| {
            let _ = raw_tx.send(res);
        })?;
        let inner = LiveFileWatcherInner {
            health: Arc::clone(&health),
            watcher,
            watched_paths: HashMap::new(),
        };
        let file_watcher = Self {
            health,
            inner: Some(Arc::new(Mutex::new(inner))),
            state: Arc::new(RwLock::new(WatchState::default())),
        };
        file_watcher.spawn_event_loop(raw_rx);
        Ok(file_watcher)
    }

    /// Create an inert watcher that still supports synthetic test notifications.
    /// 创建惰性监听器，但仍支持测试用合成通知。
    pub fn noop() -> Self {
        Self {
            health: Arc::new(LiveWatchHealthState::default()),
            inner: None,
            state: Arc::new(RwLock::new(WatchState::default())),
        }
    }

    /// Add one logical subscriber and return its handle plus receiver.
    /// 添加一个逻辑订阅者，并返回其句柄和接收器。
    pub fn add_subscriber(self: &Arc<Self>) -> (LiveFileWatcherSubscriber, LiveWatchReceiver) {
        let (tx, rx) = watch_channel(Arc::clone(&self.health));
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let subscriber_id = state.next_subscriber_id;
        state.next_subscriber_id += 1;
        state.subscribers.insert(
            subscriber_id,
            SubscriberState {
                watched_paths: HashMap::new(),
                tx,
            },
        );

        let subscriber = LiveFileWatcherSubscriber {
            id: subscriber_id,
            file_watcher: Arc::clone(self),
        };
        (subscriber, rx)
    }

    /// Registers logical ownership under the short state lock, then reconciles backend watches.
    /// 在短状态锁下注册逻辑归属，随后核对后端监听。
    fn register_paths(
        &self,
        subscriber_id: SubscriberId,
        watched_paths: &[SubscriberWatchRegistration],
    ) {
        let mut affected = BTreeSet::new();
        {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !state.subscribers.contains_key(&subscriber_id) {
                return;
            }
            for registration in watched_paths {
                let subscriber = state
                    .subscribers
                    .get_mut(&subscriber_id)
                    .expect("checked subscriber");
                let (actual, inserted) =
                    match subscriber.watched_paths.entry(registration.key.clone()) {
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            entry.get_mut().count += 1;
                            (entry.get().actual.clone(), false)
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(SubscriberWatchState {
                                identity: Arc::new(()),
                                actual: registration.actual.clone(),
                                count: 1,
                                last_exists: registration.last_exists,
                                fallback: registration.fallback,
                            });
                            (registration.actual.clone(), true)
                        }
                    };
                if inserted {
                    state.routing.insert(subscriber_id, &registration.key);
                }
                let counts = state
                    .path_ref_counts
                    .entry(actual.path.clone())
                    .or_default();
                let previous = counts.effective_mode();
                counts.increment(actual.recursive, 1);
                if previous != counts.effective_mode() {
                    affected.insert(actual.path);
                }
            }
        }
        dispatch::reconcile_backend(&self.state, self.inner.as_ref(), &affected);
    }

    /// Removes logical references before performing any potentially blocking backend operations.
    /// 在执行任何可能阻塞的后端操作之前移除逻辑引用。
    fn unregister_paths(&self, subscriber_id: SubscriberId, watched_paths: &[SubscriberWatchKey]) {
        let mut affected = BTreeSet::new();
        {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for key in watched_paths {
                let Some(subscriber) = state.subscribers.get_mut(&subscriber_id) else {
                    return;
                };
                let Some(watch) = subscriber.watched_paths.get_mut(key) else {
                    continue;
                };
                let actual = watch.actual.clone();
                watch.count = watch.count.saturating_sub(1);
                if watch.count == 0 {
                    subscriber.watched_paths.remove(key);
                    state.routing.remove(subscriber_id, key);
                }
                dispatch::remove_counts(&mut state.path_ref_counts, &actual, 1, &mut affected);
            }
        }
        dispatch::reconcile_backend(&self.state, self.inner.as_ref(), &affected);
    }

    /// Drops one subscriber's logical routes and reconciles the exact affected backend paths.
    /// 移除单个订阅者的逻辑路由，并核对精确受影响的后端路径。
    fn remove_subscriber(&self, subscriber_id: SubscriberId) {
        let mut affected = BTreeSet::new();
        {
            let mut state = self
                .state
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(subscriber) = state.subscribers.remove(&subscriber_id) else {
                return;
            };
            for (key, watch) in subscriber.watched_paths {
                state.routing.remove(subscriber_id, &key);
                dispatch::remove_counts(
                    &mut state.path_ref_counts,
                    &watch.actual,
                    watch.count,
                    &mut affected,
                );
            }
        }
        dispatch::reconcile_backend(&self.state, self.inner.as_ref(), &affected);
    }

    /// Reconfigure one actual backend watch path.
    /// 重新配置一个真实后端监听路径。
    #[cfg(test)]
    fn reconfigure_watch<'a>(
        &'a self,
        path: &Path,
        next_mode: Option<RecursiveMode>,
        inner_guard: &mut Option<std::sync::MutexGuard<'a, LiveFileWatcherInner>>,
    ) {
        Self::reconfigure_watch_inner(self.inner.as_ref(), path, next_mode, inner_guard);
    }

    /// Reconfigure one backend path with a shared lock guard.
    /// 使用共享锁守卫重新配置一个后端路径。
    fn reconfigure_watch_inner<'a>(
        inner: Option<&'a Arc<Mutex<LiveFileWatcherInner>>>,
        path: &Path,
        next_mode: Option<RecursiveMode>,
        inner_guard: &mut Option<std::sync::MutexGuard<'a, LiveFileWatcherInner>>,
    ) {
        let Some(inner) = inner else {
            return;
        };
        if inner_guard.is_none() {
            let guard = inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *inner_guard = Some(guard);
        }
        let Some(guard) = inner_guard.as_mut() else {
            return;
        };

        let existing_mode = guard.watched_paths.get(path).copied();
        if existing_mode == next_mode {
            return;
        }

        if existing_mode.is_some() {
            if let Err(err) = guard.watcher.unwatch(path) {
                guard.health.record_gap();
                warn!("failed to unwatch {}: {err}", path.display());
            }
            guard.watched_paths.remove(path);
        }

        let Some(next_mode) = next_mode else {
            return;
        };
        if !path.exists() {
            guard.health.record_gap();
            return;
        }

        if let Err(err) = guard.watcher.watch(path, next_mode) {
            guard.health.record_gap();
            warn!("failed to watch {}: {err}", path.display());
            return;
        }
        guard.watched_paths.insert(path.to_path_buf(), next_mode);
    }

    /// Bridge raw notify events into subscriber notifications.
    /// 将原始 notify 事件桥接为订阅者通知。
    fn spawn_event_loop(&self, mut raw_rx: raw_events::RawWatchReceiver) {
        if let Ok(handle) = Handle::try_current() {
            // Construct ownership before spawning so dropping an unpolled future also retires liveness.
            // 派生任务前构造归属，使丢弃未轮询未来值也会退役存活状态。
            let health_guard = LiveWatchLoopGuard(Arc::clone(&self.health));
            self.health.running.store(true, Ordering::Release);
            let state = Arc::clone(&self.state);
            let inner = self.inner.as_ref().map(Arc::downgrade);
            handle.spawn(async move {
                // Keep this guard until the exact consumer exits or is cancelled.
                // 保留此守卫直到精确消费者退出或被取消。
                let event_loop_health = health_guard;
                loop {
                    match raw_rx.recv().await {
                        Some(raw_events::RawWatchMessage::Event(Ok(event))) => {
                            // A rescan flag may have no paths; retain the gap before ordinary event filtering.
                            // 重新扫描标志可能没有路径；在普通事件过滤之前保留缺口。
                            if event.need_rescan() {
                                event_loop_health.0.record_gap();
                            }
                            if !is_mutating_event(&event) || event.paths.is_empty() {
                                continue;
                            }
                            let inner = inner.as_ref().and_then(std::sync::Weak::upgrade);
                            Self::notify_subscribers(&state, inner.as_ref(), &event.paths).await;
                        }
                        Some(raw_events::RawWatchMessage::Event(Err(err))) => {
                            event_loop_health.0.record_gap();
                            warn!("file watcher error: {err}");
                        }
                        Some(raw_events::RawWatchMessage::Rescan) => {
                            let roots = {
                                let guard = state
                                    .read()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                guard
                                    .subscribers
                                    .values()
                                    .flat_map(|subscriber| {
                                        subscriber
                                            .watched_paths
                                            .keys()
                                            .map(|key| key.requested.path.clone())
                                    })
                                    .collect::<BTreeSet<_>>()
                                    .into_iter()
                                    .collect::<Vec<_>>()
                            };
                            let inner = inner.as_ref().and_then(std::sync::Weak::upgrade);
                            // Root delivery both reconciles fallback watches and forces consumer rescan.
                            // 根通知同时重新核对回退监听，并要求消费者重新扫描。
                            Self::notify_subscribers(&state, inner.as_ref(), &roots).await;
                        }
                        None => break,
                    }
                }
            });
        } else {
            warn!("file watcher loop skipped: no Tokio runtime available");
        }
    }

    /// Routes and probes on the blocking boundary without retaining shared state across filesystem I/O.
    /// 在阻塞边界进行路由与探测，文件系统 I/O 期间不保留共享状态锁。
    async fn notify_subscribers(
        state: &Arc<RwLock<WatchState>>,
        inner: Option<&Arc<Mutex<LiveFileWatcherInner>>>,
        event_paths: &[PathBuf],
    ) {
        let state = Arc::clone(state);
        let inner = inner.cloned();
        let paths = event_paths.to_vec();
        // One raw consumer awaits each batch; this is not per-subscriber task fan-out.
        // 单个原始消费者等待每个批次完成；不按订阅者扩散派生任务。
        let delivered = tokio::task::spawn_blocking(move || {
            dispatch::prepare_delivery(&state, inner.as_ref(), &paths)
        })
        .await;
        match delivered {
            Ok(deliveries) => {
                for (sender, paths, roots) in deliveries {
                    sender.add_changed_paths(&paths, &roots).await;
                }
            }
            Err(error) => {
                // A failed worker cannot silently certify delivery; retiring the consumer records a gap.
                // 工作线程失败不能静默证明已交付；消费者退役会记录缺口。
                panic!("file watcher dispatch worker failed: {error}");
            }
        }
    }

    /// Send synthetic paths through the normal matching path.
    /// 通过正常匹配链路发送合成路径。
    #[cfg(test)]
    async fn send_paths_for_test(&self, paths: Vec<PathBuf>) {
        Self::notify_subscribers(&self.state, self.inner.as_ref(), &paths).await;
    }

    /// Return ref counts for tests.
    /// 返回测试用引用计数。
    #[cfg(test)]
    fn watch_counts_for_test(&self, path: &Path) -> Option<(usize, usize)> {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .path_ref_counts
            .get(path)
            .map(|counts| (counts.non_recursive, counts.recursive))
    }
}

/// Return whether a raw notify event mutates filesystem state.
/// 返回原始 notify 事件是否改变文件系统状态。
fn is_mutating_event(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Sort and deduplicate watch paths.
/// 排序并去重监听路径。
fn dedupe_watched_paths(mut watched_paths: Vec<LiveWatchPath>) -> Vec<LiveWatchPath> {
    watched_paths.sort_unstable_by(|a, b| {
        a.path
            .as_os_str()
            .cmp(b.path.as_os_str())
            .then(a.recursive.cmp(&b.recursive))
    });
    watched_paths.dedup();
    watched_paths
}

/// Return the actual backend path, match path, and fallback flag.
/// 返回真实后端路径、匹配路径和回退标记。
fn actual_watch_path(requested: &LiveWatchPath) -> (LiveWatchPath, LiveWatchPath, bool) {
    if requested.path.exists() {
        let matched_path = requested
            .path
            .canonicalize()
            .unwrap_or_else(|_| requested.path.clone());
        let actual = requested.clone();
        let matched = LiveWatchPath {
            path: matched_path,
            recursive: requested.recursive,
        };
        return (actual, matched, false);
    }

    let requested_parent = requested.path.parent();
    let mut ancestor = requested_parent;
    while let Some(path) = ancestor {
        if path.is_dir() {
            let actual_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let matched_path = requested
                .path
                .strip_prefix(path)
                .map(|suffix| actual_path.join(suffix))
                .unwrap_or_else(|_| requested.path.clone());
            let actual = LiveWatchPath {
                path: path.to_path_buf(),
                recursive: false,
            };
            let matched = LiveWatchPath {
                path: matched_path,
                recursive: requested.recursive,
            };
            return (actual, matched, true);
        }
        ancestor = path.parent();
    }

    (requested.clone(), requested.clone(), false)
}

/// Map one backend event path into subscriber-visible path space.
/// 将单个后端事件路径映射到订阅者可见路径空间。
fn changed_path_for_event(
    subscriber_watch: &SubscriberWatchKey,
    subscriber_watch_state: &mut SubscriberWatchState,
    event_path: &Path,
    matched_exists: bool,
    requested_exists: bool,
) -> Option<PathBuf> {
    if let Some(path) = changed_path_for_matched_path(
        subscriber_watch,
        subscriber_watch_state,
        &subscriber_watch.matched,
        event_path,
        matched_exists,
    ) {
        return Some(path);
    }
    if subscriber_watch.matched.path == subscriber_watch.requested.path {
        return None;
    }
    changed_path_for_matched_path(
        subscriber_watch,
        subscriber_watch_state,
        &subscriber_watch.requested,
        event_path,
        requested_exists,
    )
}

/// Apply matching rules in one path namespace.
/// 在单个路径命名空间内应用匹配规则。
fn changed_path_for_matched_path(
    subscriber_watch: &SubscriberWatchKey,
    subscriber_watch_state: &mut SubscriberWatchState,
    matched: &LiveWatchPath,
    event_path: &Path,
    now_exists: bool,
) -> Option<PathBuf> {
    let requested = &subscriber_watch.requested;
    if event_path == matched.path {
        subscriber_watch_state.last_exists = now_exists;
        return Some(requested.path.clone());
    }
    if matched.path.starts_with(event_path) {
        if subscriber_watch_state.fallback {
            let should_notify = now_exists || subscriber_watch_state.last_exists;
            subscriber_watch_state.last_exists = now_exists;
            return should_notify.then(|| requested.path.clone());
        }
        if subscriber_watch_state.actual.path != matched.path {
            let should_notify = now_exists || subscriber_watch_state.last_exists;
            subscriber_watch_state.last_exists = now_exists;
            return should_notify.then(|| requested.path.clone());
        }
        subscriber_watch_state.last_exists = now_exists;
        return Some(event_path.to_path_buf());
    }
    if !event_path.starts_with(&matched.path) {
        return None;
    }
    if !(matched.recursive || event_path.parent() == Some(matched.path.as_path())) {
        return None;
    }
    subscriber_watch_state.last_exists = now_exists;
    Some(
        event_path
            .strip_prefix(&matched.path)
            .map(|suffix| requested.path.join(suffix))
            .unwrap_or_else(|_| event_path.to_path_buf()),
    )
}

#[cfg(test)]
mod tests;
