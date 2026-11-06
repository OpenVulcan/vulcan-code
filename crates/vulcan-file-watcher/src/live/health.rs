use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64};

/// Read-only observation evidence for one exact registration, not proof that indexed content is current.
/// 一份精确注册的只读观察证据，不证明索引内容为当前值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveWatchObservationHealth {
    /// Whether the retained event consumer is still running.
    /// 所保留的事件消费者是否仍在运行。
    pub event_loop_running: bool,
    /// Whether all exact logical paths have matching physical backend watches.
    /// 全部精确逻辑路径是否具有匹配的物理后端监听。
    pub physically_registered: bool,
    /// Watcher-local monotonic counter of reported errors, rescan hints and event-loop retirement.
    /// 监听器内已报告错误、重新扫描提示及事件循环退役的单调计数。
    /// Compare only within the same retained registration lifetime; gaps may affect every subscriber.
    /// 仅在同一保留注册生命周期内比较；缺口可能影响每个订阅者。
    pub loss_revision: u64,
}

/// Internal evidence shared by the backend, event task and read-only registration handles.
/// 后端、事件任务和只读注册句柄共享的内部证据。
#[derive(Default)]
pub(super) struct LiveWatchHealthState {
    /// Event-task liveness, cleared even if its future is aborted.
    /// 事件任务存活状态，即使其未来值被中止也会清除。
    pub(super) running: AtomicBool,
    /// Reported observation discontinuities; never reset by a successful metadata read.
    /// 已报告观察中断；绝不因元数据读取成功而重置。
    loss_revision: AtomicU64,
}

impl LiveWatchHealthState {
    /// Retain one reported discontinuity; takes no input and does not schedule or emit file events.
    /// 保留一次已报告中断；无输入，不调度或发送文件事件。
    pub(super) fn record_gap(&self) {
        self.loss_revision.fetch_add(1, Ordering::AcqRel);
    }
}

/// Own event-task liveness across normal completion, panic and runtime cancellation.
/// 在正常完成、恐慌和运行时取消期间持有事件任务存活状态。
pub(super) struct LiveWatchLoopGuard(pub(super) Arc<LiveWatchHealthState>);

impl Drop for LiveWatchLoopGuard {
    /// Retire this exact consumer; no parameters, result or synthetic path notification.
    /// 退役此精确消费者；无参数、返回值或合成路径通知。
    fn drop(&mut self) {
        self.0.running.store(false, Ordering::Release);
        self.0.record_gap();
    }
}

impl LiveWatchRegistration {
    /// Sample this registration without adding watches, reading source files or triggering work.
    /// 采样本注册，不增加监听、不读取源码文件、不触发工作。
    /// Returns None after the owning watcher is gone or subscription state is poisoned; otherwise returns backend evidence.
    /// 所属监听器消失或订阅状态锁损坏时返回 None，否则返回后端证据。
    /// Consumers must separately track unconsumed paths and reconcile content after any observed gap.
    /// 消费者必须独立跟踪未消费路径，并在观察到缺口后核对内容。
    pub fn observation_health(&self) -> Option<LiveWatchObservationHealth> {
        // Retain the exact watcher while sampling; a similarly named registration grants no authority.
        // 采样期间保留精确监听器；相似名称的注册不授予权威。
        let watcher = self.file_watcher.upgrade()?;
        // Reconciliation owns backend before reading logical goals; observers use the same order.
        // 核对阶段先持有后端再读取逻辑目标；观察者采用同一顺序。
        let backend = watcher.inner.as_ref().and_then(|inner| inner.lock().ok());
        let state = watcher.state.read().ok()?;
        let physically_registered = backend.as_ref().is_some_and(|backend| {
            !self.watched_paths.is_empty()
                && state
                    .subscribers
                    .get(&self.subscriber_id)
                    .is_some_and(|subscriber| {
                        self.watched_paths.iter().all(|key| {
                            subscriber.watched_paths.get(key).is_some_and(|path| {
                                // Historical fallback remains sticky for event matching, not current coverage.
                                // 历史回退标记为事件匹配保留，不能代表当前覆盖情况。
                                path.actual.path == key.requested.path
                                    && path.last_exists
                                    && backend.watched_paths.get(&path.actual.path).is_some_and(
                                        |mode| {
                                            !key.requested.recursive
                                                || *mode == RecursiveMode::Recursive
                                        },
                                    )
                            })
                        })
                    })
        });
        Some(LiveWatchObservationHealth {
            event_loop_running: watcher.health.running.load(Ordering::Acquire),
            physically_registered,
            loss_revision: watcher.health.loss_revision.load(Ordering::Acquire),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::Flag;
    use tokio::time::timeout;

    /// Wait for exact reported evidence in this isolated watcher; fail instead of guessing from elapsed time.
    /// 等待本隔离监听器的精确报告证据；超时失败，不从经过时间猜测。
    /// registration is retained by the test and revision/running select the expected sample; returns nothing.
    /// registration 由测试保留，revision/running 选择预期采样；无返回值。
    async fn wait_health(registration: &LiveWatchRegistration, revision: u64, running: bool) {
        timeout(Duration::from_secs(2), async {
            loop {
                // Sampling does not drain subscriber paths or modify the evidence.
                // 采样不排空订阅者路径，也不修改证据。
                let health = registration.observation_health().expect("retained watcher");
                if health.loss_revision == revision && health.event_loop_running == running {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("expected health transition");
    }

    /// Verify inert and expired registrations never claim physical or live backend coverage.
    /// 验证惰性及过期注册绝不宣称物理或活动后端覆盖。
    #[test]
    fn inert_registration_is_not_physical_coverage() {
        // Only this test owns the noop backend and its logical subscription.
        // 只有本测试持有惰性后端及其逻辑订阅。
        let watcher = Arc::new(LiveFileWatcher::noop());
        let (subscriber, _receiver) = watcher.add_subscriber();
        let registration = subscriber.register_path(std::env::temp_dir(), true);
        let first = registration
            .observation_health()
            .expect("logical registration");
        assert!(!first.event_loop_running);
        assert!(!first.physically_registered);
        assert_eq!(first.loss_revision, 0);
        assert_eq!(registration.observation_health(), Some(first));
        drop(subscriber);
        drop(watcher);
        assert!(registration.observation_health().is_none());
        assert!(
            LiveWatchRegistration::default()
                .observation_health()
                .is_none()
        );
    }

    /// Poisoned subscription state cannot be used as evidence of valid observation coverage.
    /// 损坏的订阅状态不能作为有效观察覆盖的证据。
    #[test]
    fn poisoned_subscription_state_returns_no_health_evidence() {
        // Isolate deliberate lock poisoning from every production watcher and shared process resource.
        // 将主动锁损坏与每个生产监听器及共享进程资源隔离。
        let watcher = Arc::new(LiveFileWatcher::noop());
        // Retain one exact logical owner and registration while injecting the failure.
        // 注入失败期间保留一个精确逻辑所有者及注册。
        let (subscriber, _receiver) = watcher.add_subscriber();
        let registration = subscriber.register_path(std::env::temp_dir(), true);
        // The caught unwind leaves only this fixture's subscription lock poisoned.
        // 捕获的展开仅使本夹具订阅锁损坏。
        let failed = std::panic::catch_unwind(|| {
            // Keep the guard held through unwinding to exercise the actual poisoned-lock branch.
            // 展开期间保持守卫，验证真实锁损坏分支。
            let _state = watcher.state.write().expect("fixture subscription lock");
            panic!("fixture subscription state failure");
        });
        assert!(failed.is_err());
        assert!(registration.observation_health().is_none());
    }

    /// Verify raw-loop errors retain evidence without synthesizing events, while a pathless rescan
    /// hint also invalidates the registered roots.
    /// 验证原始循环错误保留证据且不合成事件，无路径重新扫描提示还会让注册根失效。
    #[tokio::test]
    async fn raw_loop_reports_gaps_without_new_path_notifications() {
        // Inject into the production raw-loop boundary while keeping OS notifications out of this fixture.
        // 注入生产原始循环边界，同时排除本夹具中的操作系统通知。
        let watcher = Arc::new(LiveFileWatcher::noop());
        let (subscriber, mut receiver) = watcher.add_subscriber();
        let root = std::fs::canonicalize(std::env::temp_dir()).expect("temporary root");
        let registration = subscriber.register_path(root.clone(), true);
        let (sender, raw) = raw_events::channel(Arc::clone(&watcher.health));
        watcher.spawn_event_loop(raw);
        wait_health(&registration, 0, true).await;
        sender
            .send(Err(notify::Error::generic("fixture backend error")))
            .expect("open raw loop");
        wait_health(&registration, 1, true).await;
        assert!(
            timeout(Duration::from_millis(10), receiver.recv())
                .await
                .is_err()
        );
        sender
            .send(Ok(Event::new(EventKind::Other).set_flag(Flag::Rescan)))
            .expect("pathless rescan");
        wait_health(&registration, 2, true).await;
        assert_eq!(
            timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("rescan delivery")
                .map(|event| event.paths),
            Some(vec![root.clone()])
        );
        sender
            .send(Ok(Event::new(EventKind::Modify(
                notify::event::ModifyKind::Any,
            ))
            .add_path(root.clone())))
            .expect("ordinary event");
        assert_eq!(
            timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("path delivery"),
            Some(LiveFileWatchEvent {
                paths: vec![root],
                modification_only_paths: BTreeSet::new(),
            })
        );
        assert_eq!(
            registration
                .observation_health()
                .expect("current health")
                .loss_revision,
            2
        );
        drop(sender);
        wait_health(&registration, 3, false).await;
    }

    /// Verify a cancelled event task retires its evidence even when its body never gets polled.
    /// 验证被取消事件任务即使未轮询主体也会退役其证据。
    #[test]
    fn runtime_drop_retires_unpolled_event_loop() {
        // A private single-thread runtime is never driven after spawning the raw consumer.
        // 派生原始消费者后绝不驱动私有单线程运行时。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("fixture runtime");
        let watcher = Arc::new(LiveFileWatcher::noop());
        let (subscriber, _receiver) = watcher.add_subscriber();
        let registration = subscriber.register_path(std::env::temp_dir(), true);
        let (_sender, raw) = raw_events::channel(Arc::clone(&watcher.health));
        {
            // Entering allows task admission but does not poll the task.
            // 进入运行时允许任务准入，但不轮询任务。
            let _entered = runtime.enter();
            watcher.spawn_event_loop(raw);
        }
        assert!(
            registration
                .observation_health()
                .expect("admitted task")
                .event_loop_running
        );
        drop(runtime);
        let retired = registration
            .observation_health()
            .expect("retained metadata");
        assert!(!retired.event_loop_running);
        assert_eq!(retired.loss_revision, 1);
    }

    /// Verify a restored directory regains physical coverage without changing historical fallback matching.
    /// 验证恢复目录重新取得物理覆盖，同时不改变历史父目录匹配行为。
    /// Takes no parameters; returns after real backend moves and private empty-directory cleanup.
    /// 无参数；真实后端迁移及私有空目录清理完成后返回。
    #[tokio::test]
    async fn restored_directory_regains_physical_coverage() {
        // parent is a unique test-owned directory; no existing project is registered or removed.
        // parent 是测试专有唯一目录；不注册或移除任何现有项目。
        let parent = std::env::temp_dir().join(format!(
            "watcher-restored-health-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&parent).expect("private parent");
        // root uses the same canonical namespace as backend notifications.
        // root 使用与后端通知相同的规范命名空间。
        let root = parent
            .canonicalize()
            .expect("canonical parent")
            .join("project");
        std::fs::create_dir(&root).expect("private project");
        // watcher and registration exercise actual backend watch/unwatch without waiting for OS event timing.
        // watcher 与 registration 验证实际后端监听及取消监听，不依赖操作系统事件时序。
        let watcher = Arc::new(LiveFileWatcher::new().expect("real watcher"));
        let (subscriber, _receiver) = watcher.add_subscriber();
        let registration = subscriber.register_path(root.clone(), true);
        // Record coverage before removal, during fallback, and after exact-path reattachment.
        // 记录移除前、退回父目录期间和重新挂接精确路径后的覆盖状态。
        let initial = registration.observation_health().expect("initial health");
        std::fs::remove_dir(&root).expect("remove own empty project");
        watcher.send_paths_for_test(vec![root.clone()]).await;
        let missing = registration.observation_health().expect("missing health");
        std::fs::create_dir(&root).expect("restore own empty project");
        watcher.send_paths_for_test(vec![root.clone()]).await;
        let restored = registration.observation_health().expect("restored health");
        // Preserve the historical flag needed by event matching; current coverage must not depend on it.
        // 保留事件匹配所需的历史标记；当前覆盖不能依赖该标记。
        let fallback_retained = watcher
            .state
            .read()
            .expect("subscription state")
            .subscribers[&subscriber.id]
            .watched_paths
            .values()
            .all(|path| path.fallback);
        drop(registration);
        drop(subscriber);
        drop(watcher);
        std::fs::remove_dir(&root).expect("remove restored empty project");
        std::fs::remove_dir(&parent).expect("remove private parent");
        assert!(initial.physically_registered);
        assert!(!missing.physically_registered);
        assert!(fallback_retained);
        assert!(restored.physically_registered);
    }

    /// Verify a real OS registration is distinct from a skipped missing-path backend watch.
    /// 验证真实操作系统注册与跳过的缺失路径后端监听可以区分。
    #[tokio::test]
    async fn successful_backend_registration_is_observable() {
        // Use a unique empty directory, never a user's project or existing index.
        // 使用唯一空目录，绝不使用用户项目或既有索引。
        let root = std::env::temp_dir().join(format!(
            "watcher-health-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("isolated root");
        let watcher = Arc::new(LiveFileWatcher::new().expect("real watcher"));
        let (subscriber, _receiver) = watcher.add_subscriber();
        let registration = subscriber.register_path(root.clone(), true);
        let first = registration
            .observation_health()
            .expect("registered backend");
        assert!(first.event_loop_running);
        assert!(first.physically_registered);
        // This absent sibling is passed through the production backend admission check only.
        // 该不存在的同级路径仅传入生产后端准入检查。
        let missing = root.with_extension("absent");
        watcher.reconfigure_watch(&missing, Some(RecursiveMode::Recursive), &mut None);
        assert!(
            registration
                .observation_health()
                .expect("reported gap")
                .loss_revision
                > first.loss_revision
        );
        drop(registration);
        drop(subscriber);
        drop(watcher);
        std::fs::remove_dir(&root).expect("remove isolated empty root");
    }
}
