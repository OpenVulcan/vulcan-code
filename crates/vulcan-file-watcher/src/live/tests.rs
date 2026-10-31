use super::*;
use notify::event::AccessKind;
use notify::event::AccessMode;
use notify::event::CreateKind;
use notify::event::ModifyKind;
use tokio::time::timeout;

/// Short interval used by throttling tests.
/// 节流测试使用的短间隔。
const TEST_THROTTLE_INTERVAL: Duration = Duration::from_millis(50);

/// Build one test path.
/// 构建一个测试路径。
fn path(name: &str) -> PathBuf {
    PathBuf::from(name)
}

/// Build one notify event.
/// 构建一个 notify 事件。
fn notify_event(kind: EventKind, paths: Vec<PathBuf>) -> Event {
    let mut event = Event::new(kind);
    for path in paths {
        event = event.add_path(path);
    }
    event
}

/// Build one unique temporary directory.
/// 构建一个唯一临时目录。
fn unique_temp_dir(prefix: &str) -> PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{suffix}"))
}

/// Verify throttling coalesces pending paths between emissions.
/// 验证节流会合并两次发出之间的待处理路径。
#[tokio::test]
async fn throttled_receiver_coalesces_within_interval() {
    let (tx, rx) = watch_channel(Arc::new(LiveWatchHealthState::default()));
    let mut throttled = ThrottledLiveWatchReceiver::new(rx, TEST_THROTTLE_INTERVAL);

    tx.add_changed_paths(&[path("a")], &[path("root")], false)
        .await;
    let first = timeout(Duration::from_secs(1), throttled.recv())
        .await
        .expect("first emit timeout");
    assert_eq!(
        first,
        Some(LiveFileWatchEvent {
            paths: vec![path("a")],
            modification_only_paths: BTreeSet::new(),
        })
    );

    tx.add_changed_paths(&[path("b"), path("c")], &[path("root")], false)
        .await;
    let blocked = timeout(TEST_THROTTLE_INTERVAL / 2, throttled.recv()).await;
    assert!(blocked.is_err());

    let second = timeout(TEST_THROTTLE_INTERVAL * 2, throttled.recv())
        .await
        .expect("second emit timeout");
    assert_eq!(
        second,
        Some(LiveFileWatchEvent {
            paths: vec![path("b"), path("c")],
            modification_only_paths: BTreeSet::new(),
        })
    );
}

/// Verify non-mutating notify events are filtered.
/// 验证非变更类 notify 事件会被过滤。
#[test]
fn is_mutating_event_filters_non_mutating_event_kinds() {
    assert!(is_mutating_event(&notify_event(
        EventKind::Create(CreateKind::Any),
        vec![path("/tmp/created")]
    )));
    assert!(is_mutating_event(&notify_event(
        EventKind::Modify(ModifyKind::Any),
        vec![path("/tmp/modified")]
    )));
    assert!(!is_mutating_event(&notify_event(
        EventKind::Access(AccessKind::Open(AccessMode::Any)),
        vec![path("/tmp/accessed")]
    )));
}

/// Raw delivery distinguishes directory modifications from renames, creation, removal and rescans.
/// 原始事件投递区分目录修改与重命名、新建、删除以及重扫。
#[tokio::test]
async fn raw_delivery_retains_modification_evidence_without_weakening_structural_events() {
    // Isolate real directory metadata checks while injecting deterministic raw events.
    // 隔离真实目录属性检查，同时注入确定性的原始事件。
    let root = unique_temp_dir("vulcan-live-watch-modification");
    std::fs::create_dir_all(root.join(".git")).expect("create administrative directory");
    // Match the canonical path namespace used by the native backend.
    // 匹配原生后端使用的规范路径命名空间。
    let root = std::fs::canonicalize(root).expect("canonicalize fixture root");
    // Keep OS notifications out of the fixture; the production raw loop is still exercised.
    // 将 OS 通知排除在夹具外，但仍覆盖生产原始循环。
    let watcher = Arc::new(LiveFileWatcher::noop());
    // This subscriber represents the repository's recursive Git watch.
    // 此订阅者代表仓库的递归 Git 监听。
    let (subscriber, mut receiver) = watcher.add_subscriber();
    // Retain registration ownership through every injected event.
    // 在全部注入事件期间保持注册归属。
    let registration = subscriber.register_path(root.clone(), true);
    // A peer consumer must receive the same paths, including events Git later treats as noise.
    // 同级消费者必须收到相同路径，包括 Git 后续视为噪声的事件。
    let (peer, mut peer_receiver) = watcher.add_subscriber();
    // Independent registration ownership must not inherit any Git-specific suppression.
    // 独立注册归属不得继承任何 Git 专用过滤。
    let peer_registration = peer.register_path(root.clone(), true);
    // Inject the same event classifications produced by the native notify backend.
    // 注入与原生 notify 后端相同的事件分类。
    let (sender, raw) = raw_events::channel(Arc::clone(&watcher.health));
    watcher.spawn_event_loop(raw);
    for (kind, modification_only) in [
        (EventKind::Modify(ModifyKind::Any), cfg!(windows)),
        (
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            false,
        ),
        (
            EventKind::Modify(ModifyKind::Metadata(notify::event::MetadataKind::Any)),
            true,
        ),
        (
            EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Any)),
            false,
        ),
        (EventKind::Modify(ModifyKind::Other), false),
        (EventKind::Create(CreateKind::Folder), false),
        (EventKind::Remove(notify::event::RemoveKind::Folder), false),
    ] {
        sender
            .send(Ok(notify_event(kind, vec![root.join(".git")])))
            .expect("inject event");
        // Each delivery must retain the path regardless of whether Git later filters its metadata.
        // 无论 Git 后续是否过滤属性，每次投递都必须保留路径。
        let event = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("receive raw event")
            .expect("open receiver");
        assert_eq!(event.paths, vec![root.join(".git")]);
        assert_eq!(
            timeout(Duration::from_secs(2), peer_receiver.recv())
                .await
                .expect("receive peer event")
                .expect("open peer receiver"),
            event,
        );
        assert_eq!(
            event.modification_only_paths.contains(&root.join(".git")),
            modification_only
        );
    }
    sender
        .send(Ok(notify_event(
            EventKind::Modify(ModifyKind::Any),
            vec![root.join(".git")],
        )
        .set_flag(notify::event::Flag::Rescan)))
        .expect("inject rescan hint");
    // Rescan hints cannot be mistaken for ordinary directory modification noise.
    // 重扫提示不能被误认为普通目录修改噪声。
    let rescan = timeout(Duration::from_secs(2), receiver.recv())
        .await
        .expect("receive rescan")
        .expect("open receiver");
    assert!(rescan.modification_only_paths.is_empty());
    assert_eq!(
        timeout(Duration::from_secs(2), peer_receiver.recv())
            .await
            .expect("receive peer rescan")
            .expect("open peer receiver"),
        rescan,
    );
    drop(registration);
    sender
        .send(Ok(notify_event(
            EventKind::Create(CreateKind::File),
            vec![root.join(".git/index.lock")],
        )))
        .expect("inject lock event after peer release");
    // Releasing the Git-side watch must not suppress lock paths for the remaining consumer.
    // 释放 Git 侧监听后，不得为剩余消费者抑制锁文件路径。
    let peer_lock = timeout(Duration::from_secs(2), peer_receiver.recv())
        .await
        .expect("receive peer lock")
        .expect("open peer receiver");
    assert_eq!(peer_lock.paths, vec![root.join(".git/index.lock")]);
    drop(sender);
    drop(peer_registration);
    std::fs::remove_dir_all(root).expect("remove isolated fixture");
}

/// Verify a live watcher is refused outside a runtime, where its event loop could never deliver a change.
/// 验证在运行时之外拒绝创建实时监听器，因为其事件循环永远无法送达变更。
#[test]
fn live_watcher_requires_a_tokio_runtime() {
    assert!(LiveFileWatcher::new().is_err());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("fixture runtime");
    let _entered = runtime.enter();
    let watcher = LiveFileWatcher::new().expect("watcher inside a runtime");
    assert!(watcher.health.running.load(Ordering::Acquire));
}

/// Verify duplicate registrations are ref-counted by path and scope.
/// 验证重复注册会按路径和范围进行引用计数。
#[test]
fn register_dedupes_by_path_and_scope() {
    let temp_dir = unique_temp_dir("vulcan-live-watch-dedupe");
    let skills = temp_dir.join("skills");
    let other_skills = temp_dir.join("other-skills");
    std::fs::create_dir_all(&skills).expect("create skills dir");
    std::fs::create_dir_all(&other_skills).expect("create other skills dir");

    let watcher = Arc::new(LiveFileWatcher::noop());
    let (subscriber, _rx) = watcher.add_subscriber();
    let _first = subscriber.register_path(skills.clone(), false);
    let _second = subscriber.register_path(skills.clone(), false);
    let _third = subscriber.register_path(skills.clone(), true);
    let _fourth = subscriber.register_path(other_skills.clone(), true);

    assert_eq!(watcher.watch_counts_for_test(&skills), Some((2, 1)));
    assert_eq!(watcher.watch_counts_for_test(&other_skills), Some((0, 1)));

    let _ = std::fs::remove_dir_all(temp_dir);
}

/// Verify dropping a registration unregisters its watched paths.
/// 验证释放注册守卫会注销其监听路径。
#[test]
fn watch_registration_drop_unregisters_paths() {
    let temp_dir = unique_temp_dir("vulcan-live-watch-drop");
    let root = temp_dir.join("watched");
    std::fs::create_dir_all(&root).expect("create root");

    let watcher = Arc::new(LiveFileWatcher::noop());
    let (subscriber, _rx) = watcher.add_subscriber();
    let registration = subscriber.register_path(root.clone(), true);
    drop(registration);

    assert_eq!(watcher.watch_counts_for_test(&root), None);

    let _ = std::fs::remove_dir_all(temp_dir);
}

/// Verify missing paths are watched through the nearest existing parent.
/// 验证缺失路径会通过最近存在的父级进行监听。
#[test]
fn missing_path_registers_nearest_existing_parent() {
    let temp_dir = unique_temp_dir("vulcan-live-watch-missing");
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let missing_file = temp_dir.join("FETCH_HEAD");

    let watcher = Arc::new(LiveFileWatcher::noop());
    let (subscriber, _rx) = watcher.add_subscriber();
    let registration = subscriber.register_path(missing_file.clone(), false);

    assert_eq!(watcher.watch_counts_for_test(&temp_dir), Some((1, 0)));
    assert_eq!(watcher.watch_counts_for_test(&missing_file), None);

    drop(registration);
    assert_eq!(watcher.watch_counts_for_test(&temp_dir), None);

    let _ = std::fs::remove_dir_all(temp_dir);
}

/// Verify subscribers receive requested paths for missing-file creation.
/// 验证订阅者在缺失文件创建时收到请求路径。
#[tokio::test]
async fn missing_file_creation_notifies_requested_path() {
    let temp_dir = unique_temp_dir("vulcan-live-watch-create");
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let missing_file = temp_dir.join("AGENTS.md");

    let watcher = Arc::new(LiveFileWatcher::noop());
    let (subscriber, mut rx) = watcher.add_subscriber();
    let _registration = subscriber.register_path(missing_file.clone(), false);

    std::fs::write(&missing_file, "rules").expect("write missing file");
    watcher.send_paths_for_test(vec![temp_dir.clone()]).await;

    let event = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("event timeout")
        .expect("event should exist");
    assert_eq!(event.paths, vec![missing_file]);

    let _ = std::fs::remove_dir_all(temp_dir);
}

/// Verify receivers close when subscribers are dropped.
/// 验证订阅者释放后接收器会关闭。
#[tokio::test]
async fn receiver_closes_when_subscriber_drops() {
    let watcher = Arc::new(LiveFileWatcher::noop());
    let (subscriber, mut rx) = watcher.add_subscriber();

    drop(subscriber);

    let closed = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("closed recv timeout");
    assert_eq!(closed, None);
}

/// Queue overflow wakes every current subscriber with its exact roots for full reconciliation.
/// 队列溢出通过精确根路径唤醒所有当前订阅者，要求完整核对。
#[tokio::test]
async fn raw_overflow_delivers_current_registered_roots() {
    let watcher = Arc::new(LiveFileWatcher::noop());
    let (subscriber, mut receiver) = watcher.add_subscriber();
    let root = std::env::temp_dir()
        .canonicalize()
        .expect("existing temporary root");
    let registration = subscriber.register_path(root.clone(), true);
    let (sender, raw) = raw_events::channel(Arc::clone(&watcher.health));
    for _ in 0..300 {
        sender
            .send(Ok(Event::new(EventKind::Other)))
            .expect("bounded raw admission");
    }
    watcher.spawn_event_loop(raw);
    let event = timeout(Duration::from_secs(2), receiver.recv())
        .await
        .expect("recovery wakes consumer")
        .expect("root event");
    assert_eq!(event.paths, vec![root]);
    assert!(
        registration
            .observation_health()
            .expect("gap evidence")
            .loss_revision
            > 0
    );
}
