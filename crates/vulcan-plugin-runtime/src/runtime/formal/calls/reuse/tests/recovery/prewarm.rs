//! Prewarming retains original initialization across caller loss, catalog updates and owner drainage.
//! 预热在调用方丢失、目录更新及所有者排空期间保留原初始化。

use super::super::targets::wait_target;
use super::*;

/// A prewarm target starts without a caller executor and survives that caller's executor; repeated
/// startup only observes the original receipt and never replays initialization.
/// 预热目标无需调用方执行器即可启动，并在该调用方执行器消失后继续存活；重复启动只观测原回执，
/// 绝不重放初始化。
#[test]
fn hot_reuse_target_survives_caller_executor_and_preserves_receipt() {
    // The native callback remains independently alive until the fixture explicitly releases it.
    // 原生回调独立存活，直到夹具显式释放。
    let recovery = RecoveryFixture::new(true);
    // Keep the same manager throughout receipt observation and recovery.
    // 在回执观测及恢复期间保持同一管理器。
    let manager = &recovery.fixture.manager;
    // Retain the inert original receipt before starting any native initialization.
    // 在启动任何原生初始化前保留原未启动回执。
    let prepared = manager
        .prepare_reusable_tool_prewarm(recovery.fixture.target(1))
        .expect("register original prewarm target");
    // Supervision belongs to the manager, so synchronous code may start the target directly.
    // 监督属于管理器，因此同步代码可直接启动目标。
    manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("start original target without a caller executor");
    // Stop only the observing executor after observing the real native callback.
    // 仅在观测真实原生回调后停止观测执行器。
    let previous = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create test executor");
    // Capture the actual core operation before dropping the observer.
    // 在丢弃观测者前捕获真实核心操作。
    let operation = previous.block_on(async {
        recovery.events.wait_entered().await;
        manager
            .formal_call_status(&prepared.invocation_id, "owner")
            .expect("query original target")
            .operation
            .expect("original native operation")
    });
    previous.shutdown_timeout(WAIT);
    // Repeated startup only observes the original receipt, which needs no recovery.
    // 重复启动只观测原回执，且无需恢复。
    let observed = manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("start or observe original target");
    assert!(!observed.requires_cleanup_recovery);
    assert_eq!(
        observed
            .operation
            .expect("original native operation")
            .operation_id,
        operation.operation_id
    );
    // A new caller executor may cancel and observe cleanup but must not restart convergence.
    // 新的调用方执行器可以取消并观测清理，但不得重启目标收敛。
    let replacement = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create test executor");
    replacement.block_on(async {
        assert!(
            manager
                .recover_reusable_tool_prewarm_cleanup(&prepared.invocation_id, "another-owner")
                .is_err(),
            "another controller cannot resume original cleanup"
        );
        manager
            .cancel_formal_call(&prepared.invocation_id, "owner")
            .expect("cancel original target");
        // Cleanup observation changes nothing while the native callback still owns the VM.
        // 原生回调仍拥有 VM 时，清理观测不改变任何内容。
        let pending = manager
            .recover_reusable_tool_prewarm_cleanup(&prepared.invocation_id, "owner")
            .expect("recover original cleanup");
        assert!(!pending.retired);
        recovery.events.release();
        // Bound observation with the shared fixture timeout without completing native work artificially.
        // 用共享夹具超时约束观测，不人为完成原生工作。
        let deadline = Instant::now() + WAIT;
        while !manager
            .recover_reusable_tool_prewarm_cleanup(&prepared.invocation_id, "owner")
            .expect("observe or resume original cleanup")
            .retired
        {
            assert!(
                Instant::now() < deadline,
                "original cleanup did not complete"
            );
            tokio::task::yield_now().await;
        }
        // Retirement preserves the original operation and explicit cancelled-target failure.
        // 退役保留原操作及明确目标取消失败。
        let terminal = manager
            .formal_call_status(&prepared.invocation_id, "owner")
            .expect("query original target");
        assert!(terminal.retired);
        assert!(
            terminal.error.is_some(),
            "cancelled convergence cannot become successful"
        );
        assert!(
            terminal
                .prewarm
                .expect("retained target progress")
                .satisfied
                .is_none()
        );
        assert_eq!(
            terminal
                .operation
                .expect("original native operation")
                .operation_id,
            operation.operation_id
        );
        manager
            .forget_formal_call(&prepared.invocation_id, "owner")
            .expect("acknowledge retained target");
        recovery.fixture.close().await;
    });
    // Actual native effects independently establish that initialization was never replayed.
    // 真实原生副作用独立证明初始化从未重放。
    let values = recovery
        .events
        .values
        .lock()
        .expect("lock native effect evidence");
    assert_eq!(
        values.iter().filter(|event| event.topic == "block").count(),
        1
    );
    assert!(!values.iter().any(|event| event.topic == "business"));
}

/// Forward the first real initializer and hold the second at the existing native barrier.
/// 转发第一个真实初始化器，并在既有原生屏障处持有第二个。
struct SecondInitializer {
    /// Shared barrier also releases native work during fixture unwinding.
    /// 共享屏障也在夹具展开时释放原生工作。
    events: Arc<RecoveryEvents>,
    /// Real initialization callback count, independent of target status projections.
    /// 真实初始化回调次数，独立于目标状态投影。
    count: AtomicUsize,
}

impl VulcanPluginUiEventPublisher for SecondInitializer {
    /// Forward event through the actual host service; only the second initializer blocks.
    /// 通过真实宿主服务转发 event；仅第二个初始化器阻塞。
    /// Return the native publication outcome without fabricating operation completion.
    /// 返回原生发布结果，不伪造操作完成。
    fn publish(&self, mut event: VulcanPluginUiInvalidation) -> Result<(), String> {
        if event.topic == "block" && self.count.fetch_add(1, Ordering::AcqRel) == 0 {
            event.topic = "first-initializer".into();
        }
        self.events.publish(event)
    }
}

/// Partial completion retains every real operation while same-domain targets, cancellation and updates race.
/// 同域目标、取消及更新交错时，部分完成保留每个真实操作。
#[tokio::test]
async fn hot_reuse_target_partial_completion_conflict_and_update_keep_original_work() {
    for mode in ["source", "cancel", "policy"] {
        // Each explicit authority transition must preserve the same in-flight initialization.
        // 每种明确权威转换都必须保留同一在途初始化。
        let cancel = mode == "cancel";
        // The fixture's drop guard releases the native callback even if an assertion fails.
        // 即使断言失败，夹具丢弃守卫仍释放原生回调。
        let recovery = RecoveryFixture::new(true);
        recovery.fixture.target_capacity(2);
        // The real callback wrapper allows one success before holding the second initialization.
        // 真实回调包装器允许一次成功，再持有第二次初始化。
        let initializer = Arc::new(SecondInitializer {
            events: Arc::clone(&recovery.events),
            count: AtomicUsize::new(0),
        });
        recovery
            .fixture
            .manager
            .install_host_services(VulcanPluginHostServices {
                storage: None,
                vldb: None,
                ui_events: Some(initializer.clone()),
            })
            .expect("two-stage native callback");
        // Keep the same manager throughout receipt observation and recovery.
        // 在回执观测及恢复期间保持同一管理器。
        let manager = &recovery.fixture.manager;
        // Retain the inert original receipt before starting any native initialization.
        // 在启动任何原生初始化前保留原未启动回执。
        let prepared = manager
            .prepare_reusable_tool_prewarm(recovery.fixture.target(2))
            .expect("register original prewarm target");
        manager
            .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
            .expect("start or observe original target");
        recovery.events.wait_entered().await;
        // Keep original native evidence for later identity and retention assertions.
        // 保留原生证据，以供后续身份及保留断言。
        let original = manager
            .formal_call_status(&prepared.invocation_id, "owner")
            .expect("query original target");
        assert_eq!(
            original
                .prewarm
                .as_ref()
                .expect("retained target progress")
                .completed
                .len(),
            1
        );
        assert_eq!(initializer.count.load(Ordering::Acquire), 2);
        // Capture the actual core operation before interrupting its observer.
        // 在中断观测者前捕获真实核心操作。
        let operation = original
            .operation
            .as_ref()
            .expect("original native operation");
        assert!(!operation.phase.is_terminal());
        manager
            .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
            .expect("start or observe original target");
        // A separate same-domain target must fail without stealing the first target's ownership.
        // 独立同域目标必须失败，且不能夺取首个目标归属。
        let conflicting = manager
            .prepare_reusable_tool_prewarm(recovery.fixture.target(2))
            .expect("register original prewarm target");
        manager
            .start_reusable_tool_prewarm(&conflicting.invocation_id, "owner")
            .expect("start or observe original target");
        // A separate same-domain target must fail without stealing the first target's ownership.
        // 独立同域目标必须失败，且不能夺取首个目标归属。
        let conflicting = wait_target(&recovery.fixture, &conflicting.invocation_id).await;
        assert!(
            conflicting
                .error
                .as_ref()
                .expect("expected target failure")
                .contains("Another prewarm target")
        );
        assert!(conflicting.operation.is_none());
        if cancel {
            manager
                .cancel_formal_call(&prepared.invocation_id, "owner")
                .expect("cancel original target");
            // Bounded drain reports incomplete work while the real callback is still held.
            // 真实回调仍被持有时，有界排空报告未完成工作。
            let report = manager
                .drain_formal_calls(Some("idle-plugin"), Some("owner"))
                .await
                .expect("drain original target work");
            assert!(
                !report.failures.is_empty(),
                "real callback must remain owned"
            );
        } else if mode == "policy" {
            // Commit a real purpose-policy revision while the original second initializer is still active.
            // 原第二个初始化器仍活动时提交真实用途策略修订。
            let before = manager
                .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
                .expect("original purpose capacity")
                .expect("registered capacity");
            // Preserve execution limits while changing the declared capacity kind and guarantee.
            // 改变声明容量类别及保证，同时保留执行上限。
            let mut config = before.snapshot.config.clone();
            config.resources.kind = luaskills::runtime::embedded::PoolKind::Dedicated;
            config.resources.min_resident_vms = 1;
            // The transaction remains tied to its exact predecessor, never a guessed live revision.
            // 事务保持绑定精确前驱，绝不猜测活动修订。
            let update = crate::VulcanPluginCapacityPolicyUpdate {
                update_id: uuid::Uuid::new_v4().to_string(),
                runtime_id: before.runtime_id,
                capacity_id: before.snapshot.capacity_id,
                expected_revision: before.policy_revision,
                plugin_id: "idle-plugin".into(),
                purpose: VulcanPluginCapacityPurpose::ToolCall,
                config: Some(config),
            };
            assert_eq!(
                manager
                    .begin_capacity_policy_update(update.clone())
                    .expect("commit policy")
                    .phase,
                crate::VulcanPluginCapacityPolicyPhase::Committed
            );
            manager
                .forget_capacity_policy_update(&update.update_id)
                .expect("acknowledge policy");
        } else {
            // Publishing new source cannot retarget the already active initializer or its completed predecessor.
            // 发布新源码不能改投已活动初始化器或其已完成前驱。
            let path = recovery
                .fixture
                .root
                .path()
                .join("bundled/idle-plugin/runtime/plugin.bundle.lua");
            // Preserve real bundle behavior while making its source fingerprint change.
            // 保留真实 bundle 行为，同时改变其源码指纹。
            let source = std::fs::read_to_string(&path).expect("read real plugin source");
            std::fs::write(path, format!("-- updated during prewarming\n{source}"))
                .expect("write updated real plugin source");
            manager
                .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
                .expect("publish updated plugin generation");
        }
        // Inspect the unchanged original identities while cancellation or publication is in flight.
        // 取消或发布进行中时检查未改变的原身份。
        let retained = manager
            .formal_call_status(&prepared.invocation_id, "owner")
            .expect("query original target");
        assert!(!retained.retired);
        assert_eq!(
            retained
                .operation
                .expect("original native operation")
                .operation_id,
            operation.operation_id
        );
        assert_eq!(retained.plugin_generation, original.plugin_generation);
        assert_eq!(initializer.count.load(Ordering::Acquire), 2);
        recovery.events.release();
        // Observe actual retirement while retaining all admitted operation evidence.
        // 观测真实退役，同时保留全部已接纳操作证据。
        let failed = wait_target(&recovery.fixture, &prepared.invocation_id).await;
        assert!(failed.error.is_some(), "{failed:?}");
        assert!(
            failed
                .prewarm
                .as_ref()
                .expect("retained target progress")
                .satisfied
                .is_none()
        );
        assert_eq!(
            failed
                .prewarm
                .as_ref()
                .expect("retained target progress")
                .completed
                .len(),
            1
        );
        assert_eq!(
            failed
                .operation
                .as_ref()
                .expect("original native operation")
                .operation_id,
            operation.operation_id
        );
        assert_eq!(initializer.count.load(Ordering::Acquire), 2);
        assert!(
            !recovery
                .events
                .values
                .lock()
                .expect("lock native effect evidence")
                .iter()
                .any(|event| event.topic == "business")
        );
        manager
            .forget_formal_call(&prepared.invocation_id, "owner")
            .expect("acknowledge retained target");
        manager
            .forget_formal_call(&conflicting.invocation_id, "owner")
            .expect("acknowledge retained target");
        manager
            .request_formal_runtime_close()
            .expect("request original runtime close");
        if mode == "source" {
            // Direct source replacement withdraws old callback authority; preserve and acknowledge the actual closing failures.
            // 直接替换源码撤回旧回调权威；保留并确认真实关闭失败。
            let deadline = Instant::now() + WAIT;
            // Keep physically retired domain evidence until its actual failures are acknowledged.
            // 在真实失败确认前保留已物理退役的域证据。
            let domain = loop {
                // Read live owned domains instead of treating absent business calls as complete cleanup.
                // 读取存活有归属域，不将缺少业务调用当作清理完成。
                let domains = manager
                    .reusable_tool_domains("owner")
                    .expect("inspect original reusable domains");
                if let Some(domain) = domains
                    .into_iter()
                    .find(|domain| domain.pool_retired && !domain.pending_finalizations)
                {
                    break domain;
                }
                assert!(
                    Instant::now() < deadline,
                    "original domain did not physically retire"
                );
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
            };
            assert_eq!(domain.plugin_generation, prepared.plugin_generation);
            assert_eq!(domain.finalization_failures.len(), 2);
            assert!(domain.finalization_failures.iter().all(|operation| {
                operation.error.as_ref().is_some_and(|error| {
                    error
                        .message
                        .contains("Plugin content changed before host admission.")
                })
            }));
            assert_eq!(
                recovery
                    .fixture
                    .owner()
                    .host
                    .runtime()
                    .resources()
                    .expect("query actual native resources")
                    .resident,
                0
            );
            assert!(
                !manager
                    .poll_formal_runtime_closed()
                    .expect("query actual runtime closure"),
                "unacknowledged failures must remain visible"
            );
            manager
                .forget_reusable_tool_domain(&domain.domain_id, "owner")
                .expect("acknowledge exact closing failures");
        }
        // Bound observation with the shared fixture timeout without completing native work artificially.
        // 用共享夹具超时约束观测，不人为完成原生工作。
        let deadline = Instant::now() + WAIT;
        while !manager
            .poll_formal_runtime_closed()
            .expect("query actual runtime closure")
        {
            assert!(
                Instant::now() < deadline,
                "cancel={cancel}; domains={:?}; calls={:?}; resources={:?}",
                manager.reusable_tool_domains("owner"),
                manager.formal_calls("owner"),
                recovery.fixture.owner().host.runtime().resources()
            );
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }
}

/// Cancellation and a newer package generation cannot release or replay a still-running prewarm callback.
/// 取消及更新包代次不能释放或重放仍在运行的预热回调。
#[tokio::test]
async fn hot_reuse_prewarm_cancel_and_update_keep_actual_initializer_owned() {
    // The existing fixture blocks a real initialization callback and releases it even during test unwinding.
    // 既有夹具阻塞真实初始化回调，并在测试展开时仍释放它。
    let recovery = RecoveryFixture::new(true);
    // Retain the published generation before queueing this exact prewarm request.
    // 排队此精确预热请求前保留已发布代次。
    let generation =
        recovery.fixture.manager.snapshot().enabled_tools["idle_ping"].plugin_generation;
    // The original manager and authorized workspace move into the interruptible observation task.
    // 原管理器及已授权工作区移入可中断观测任务。
    let manager = recovery.fixture.manager.clone();
    let workspace = recovery.fixture.root.path().to_path_buf();
    // Only this observer is aborted; the formal supervisor and real native worker retain ownership.
    // 仅中止此观测者；正式监督者及真实原生工作者保留归属。
    let task = tokio::spawn(async move {
        manager
            .prewarm_reusable_tool_instance(
                "idle-plugin",
                "idle_ping",
                generation,
                "owner".to_owned(),
                workspace,
            )
            .await
    });
    recovery.events.wait_entered().await;
    // Capture actual core identity before changing the catalog or requesting cancellation.
    // 更改目录或请求取消前捕获真实核心身份。
    let original = recovery
        .fixture
        .manager
        .formal_calls("owner")
        .expect("prewarm calls")
        .into_iter()
        .find(|call| !call.retired)
        .expect("live prewarm");
    let operation = original.operation.as_ref().expect("native operation");
    assert!(matches!(
        &operation.context,
        OperationContext::Module(context) if context.prewarm
    ));
    // Publish a new source generation while the original callback is still running.
    // 原回调仍在运行时发布新的源码代次。
    let source_path = recovery
        .fixture
        .root
        .path()
        .join("bundled/idle-plugin/runtime/plugin.bundle.lua");
    let mut source = std::fs::read_to_string(&source_path).expect("old source");
    source.push_str("\n-- New package generation.\n-- 新包代次。\n");
    std::fs::write(&source_path, source).expect("new source");
    recovery
        .fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("new generation");
    assert_ne!(
        recovery.fixture.manager.snapshot().enabled_tools["idle_ping"].plugin_generation,
        generation
    );
    task.abort();
    assert!(task.await.expect_err("aborted observer").is_cancelled());
    // Finite drain reports pending ownership rather than pretending that cancellation returned the callback.
    // 有限排空报告待完成归属，不假装取消已使回调返回。
    let pending = recovery
        .fixture
        .manager
        .drain_owner_sessions("owner")
        .await
        .expect("finite drain");
    assert!(!pending.failures.is_empty());
    assert_eq!(
        recovery
            .fixture
            .owner()
            .host
            .runtime()
            .resources()
            .expect("physical occupancy")
            .resident,
        1
    );
    let retained = recovery
        .fixture
        .manager
        .formal_call_status(&original.invocation_id, "owner")
        .expect("retained call");
    assert_eq!(retained.plugin_generation, generation);
    assert!(!retained.retired);
    assert_eq!(
        retained.operation.expect("same operation").operation_id,
        operation.operation_id
    );
    recovery.events.release();
    // All subsequent passes resume the same cleanup and never submit another initialization.
    // 全部后续轮次恢复同一清理，绝不提交另一初始化。
    let deadline = Instant::now() + WAIT;
    loop {
        let report = recovery
            .fixture
            .manager
            .drain_owner_sessions("owner")
            .await
            .expect("resume cleanup");
        if report.failures.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "{report:?}");
    }
    let terminal = recovery
        .fixture
        .manager
        .formal_call_status(&original.invocation_id, "owner")
        .expect("terminal evidence");
    assert!(terminal.retired);
    assert_eq!(
        terminal
            .operation
            .expect("original terminal operation")
            .operation_id,
        operation.operation_id
    );
    recovery
        .fixture
        .manager
        .forget_formal_call(&original.invocation_id, "owner")
        .expect("acknowledge original operation");
    recovery.fixture.close().await;
    // Committed callbacks prove initialization was executed once and business was never invoked.
    // 已提交回调证明初始化只执行一次且业务从未调用。
    let values = recovery.events.values.lock().expect("actual effects");
    assert_eq!(
        values.iter().filter(|event| event.topic == "block").count(),
        1
    );
    assert!(!values.iter().any(|event| event.topic == "business"));
}
