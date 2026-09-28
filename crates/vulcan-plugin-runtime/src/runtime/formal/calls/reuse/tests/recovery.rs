//! Recover original reusable ownership after executor loss and failed diagnostic publication.
//! 在执行器丢失及诊断发布失败后恢复原始可复用归属。

use super::*;

/// Current manifest resource floors reject policy changes before either native or persistent state is touched.
/// 当前清单资源下限在触碰原生或持久状态前拒绝策略变化。
#[tokio::test]
async fn hot_reuse_capacity_policy_validates_current_manifest_before_commit() {
    // A real version-two package supplies both current admission and reusable-tool requirements.
    // 真实第二版包同时提供当前入场及可复用工具要求。
    let fixture = Fixture::new(10_000, 100);
    fixture
        .invoke("owner", fixture.root.path().to_path_buf(), "ok")
        .await
        .expect("register actual capacity");
    let status = fixture
        .manager
        .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
        .expect("current policy")
        .expect("registered");
    // Publish an exact manifest floor satisfied by the currently active queue policy.
    // 发布由当前活动队列策略满足的精确清单下限。
    let path = fixture.root.path().join("bundled/idle-plugin/plugin.yaml");
    let mut manifest: Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("original manifest"))
            .expect("manifest");
    manifest["runtime_requirements"]["capacities"] = json!([{
        "purpose":"tool_call","reserved_resident_vms":0,"resident_vms":1,"running_calls":1,
        "queued_calls":status.snapshot.config.max_queued_calls,"queued_bytes":1
    }]);
    std::fs::write(
        path,
        serde_yaml::to_string(&manifest).expect("manifest bytes"),
    )
    .expect("publish manifest");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("new requirements");
    // The otherwise valid core change now conflicts with the current package's declared minimum.
    // 原本合法的核心变化现在与当前包声明下限冲突。
    let mut config = status.snapshot.config;
    config.max_queued_calls -= 1;
    let request = crate::VulcanPluginCapacityPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: status.runtime_id,
        capacity_id: status.snapshot.capacity_id,
        expected_revision: status.policy_revision.clone(),
        plugin_id: "idle-plugin".into(),
        purpose: VulcanPluginCapacityPurpose::ToolCall,
        config: Some(config),
    };
    assert!(
        fixture
            .manager
            .begin_capacity_policy_update(request.clone())
            .expect_err("manifest floor")
            .to_string()
            .contains("queued_calls")
    );
    assert!(
        fixture
            .manager
            .capacity_policy_update(&request.update_id)
            .expect("no transaction")
            .is_none()
    );
    assert!(
        !fixture
            .manager
            .plugin_state_root()
            .join("plugin-runtime-settings.json")
            .exists()
    );
    assert_eq!(
        fixture
            .manager
            .formal_capacity_status("idle-plugin", request.purpose)
            .expect("unchanged policy")
            .expect("registered")
            .policy_revision,
        status.policy_revision
    );
    assert_eq!(
        fixture
            .invoke("owner", fixture.root.path().to_path_buf(), "ok")
            .await
            .expect("same accepted policy")["count"],
        json!(1)
    );
    fixture.close().await;
}

/// Execution shrink waits for two real callbacks and then resumes the original predecessor without silent retries.
/// 执行缩容等待两个真实回调，然后恢复原前驱，不进行静默重试。
#[tokio::test]
async fn hot_reuse_capacity_policy_busy_shrink_waits_for_original_operations() {
    // Separate controllers keep distinct real VMs while sharing the same native purpose budget.
    // 独立控制器保持不同真实 VM，同时共享同一原生用途预算。
    let recovery = RecoveryFixture::new(false);
    let manager = recovery.fixture.manager.clone();
    let workspace = recovery.fixture.root.path().to_path_buf();
    let mut running = Vec::new();
    for controller in ["first", "second"] {
        // Each original task retains its immutable controller and native callback ownership.
        // 每个原任务保留不可变控制器及原生回调归属。
        let manager = manager.clone();
        let workspace = workspace.clone();
        running.push(tokio::spawn(async move {
            manager
                .invoke(VulcanPluginInvocationRequest {
                    tool_id: "idle_ping".into(),
                    arguments: json!({"mode":"block"}),
                    owner_session_id: controller.into(),
                    workspace_root: workspace,
                })
                .await
        }));
    }
    recovery.events.wait_entered().await;
    // Observe the real dispatched count instead of assuming the second spawn already entered Lua.
    // 观测真实分发数量，不假定第二个派生任务已进入 Lua。
    let before = tokio::time::timeout(WAIT, async {
        loop {
            let status = manager
                .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
                .expect("capacity")
                .expect("registered");
            if status.snapshot.active_operations == 2 {
                break status;
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    })
    .await
    .expect("both real operations");
    // One execution remains sufficient for each declared member while the aggregate currently has two in flight.
    // 每个声明成员只需一个执行，而聚合当前有两个进行中操作。
    let mut config = before.snapshot.config.clone();
    config.resources.max_running_calls = 1;
    let change = crate::VulcanPluginCapacityPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: before.runtime_id,
        capacity_id: before.snapshot.capacity_id,
        expected_revision: before.policy_revision.clone(),
        plugin_id: "idle-plugin".into(),
        purpose: VulcanPluginCapacityPurpose::ToolCall,
        config: Some(config),
    };
    let waiting = manager
        .begin_capacity_policy_update(change.clone())
        .expect("busy receipt");
    assert_eq!(
        waiting.phase,
        crate::VulcanPluginCapacityPolicyPhase::Prepared
    );
    assert!(waiting.error.is_some());
    assert_eq!(
        manager
            .formal_capacity_status("idle-plugin", change.purpose)
            .expect("same policy")
            .expect("registered")
            .policy_revision,
        before.policy_revision
    );
    let projection = manager
        .formal_runtime_status()
        .expect("management status")
        .expect("parent");
    assert_eq!(
        projection
            .policy_update
            .expect("discover retained receipt")
            .request
            .update_id,
        change.update_id
    );
    recovery.events.release();
    for task in running {
        assert_eq!(
            task.await
                .expect("original task")
                .expect("original business")["count"],
            json!(1)
        );
    }
    assert_eq!(
        manager
            .resume_capacity_policy_update(&change.update_id)
            .expect("explicit drain continuation")
            .phase,
        crate::VulcanPluginCapacityPolicyPhase::Committed
    );
    assert_eq!(
        manager
            .formal_capacity_status("idle-plugin", change.purpose)
            .expect("new policy")
            .expect("registered")
            .snapshot
            .config
            .resources
            .max_running_calls,
        1
    );
    manager
        .forget_capacity_policy_update(&change.update_id)
        .expect("acknowledge");
    recovery.fixture.close().await;
}

/// A live business borrower finishes on its original VM while a committed policy selects a fresh execution binding.
/// 活动业务借用者在原 VM 完成，同时已提交策略选择新的执行绑定。
#[tokio::test]
async fn hot_reuse_capacity_policy_revision_drains_original_business_without_replay() {
    // Existing instrumentation records actual native business and finalization effects.
    // 既有插桩记录实际原生业务及关闭副作用。
    let recovery = RecoveryFixture::new(false);
    let manager = recovery.fixture.manager.clone();
    let workspace = recovery.fixture.root.path().to_path_buf();
    // Keep one original business call blocked in its native callback across the policy commit.
    // 在策略提交期间，使一个原业务调用阻塞于原生回调。
    let running = tokio::spawn({
        // The task owns its manager and original input independently from the administrative observer.
        // 任务独立于管理观测者拥有其管理器及原输入。
        let manager = manager.clone();
        let workspace = workspace.clone();
        async move {
            manager
                .invoke(VulcanPluginInvocationRequest {
                    tool_id: "idle_ping".into(),
                    arguments: json!({"mode":"block"}),
                    owner_session_id: "owner".into(),
                    workspace_root: workspace,
                })
                .await
        }
    });
    recovery.events.wait_entered().await;
    // Capture the actual capacity predecessor and original domain before changing policy.
    // 改变策略前捕获实际容量前驱及原域。
    let before = manager
        .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
        .expect("capacity")
        .expect("registered");
    let original = manager
        .reusable_tool_domains("owner")
        .expect("original domain")
        .into_iter()
        .find(|domain| domain.tool_id == "idle_ping")
        .expect("original tool");
    let mut config = before.snapshot.config.clone();
    config.resources.kind = luaskills::runtime::embedded::PoolKind::Dedicated;
    config.resources.min_resident_vms = 1;
    // A non-execution shrink can commit while the exact original business operation remains active.
    // 非执行额度缩减可在精确原业务操作仍活动时提交。
    let request = crate::VulcanPluginCapacityPolicyUpdate {
        update_id: uuid::Uuid::new_v4().to_string(),
        runtime_id: before.runtime_id,
        capacity_id: before.snapshot.capacity_id,
        expected_revision: before.policy_revision,
        plugin_id: "idle-plugin".into(),
        purpose: VulcanPluginCapacityPurpose::ToolCall,
        config: Some(config),
    };
    let committed = manager
        .begin_capacity_policy_update(request.clone())
        .expect("policy commit");
    assert_eq!(
        committed.phase,
        crate::VulcanPluginCapacityPolicyPhase::Committed
    );
    manager
        .maintain_reusable_domains(&recovery.fixture.owner())
        .expect("retire old binding");
    assert!(
        !running.is_finished(),
        "policy update must not cancel the original callback"
    );
    // The original borrower and pool remain physically owned until its actual native return.
    // 直到实际原生返回，原借用者及池仍保留物理归属。
    let retained = manager
        .reusable_tool_domains("owner")
        .expect("retained original")
        .into_iter()
        .find(|domain| domain.domain_id == original.domain_id)
        .expect("same domain");
    assert_eq!(retained.pool_id, original.pool_id);
    assert_eq!(retained.borrowers, 1);
    recovery.events.release();
    assert_eq!(
        running
            .await
            .expect("original task")
            .expect("original business")["count"],
        json!(1)
    );
    assert_eq!(
        recovery
            .fixture
            .invoke("owner", workspace, "ok")
            .await
            .expect("new binding")["count"],
        json!(1)
    );
    // Two original operations committed once each; the second counter proves fresh private module state.
    // 两个原操作各提交一次；第二个计数证明新的私有模块状态。
    assert_eq!(
        recovery
            .events
            .values
            .lock()
            .expect("actual effects")
            .iter()
            .filter(|event| event.topic == "business")
            .count(),
        2
    );
    manager
        .forget_capacity_policy_update(&request.update_id)
        .expect("acknowledge policy");
    recovery.fixture.close().await;
}

/// Actual native effects and a finite-test-controlled callback barrier.
/// 真实原生副作用及有限测试控制的回调屏障。
#[derive(Default)]
struct RecoveryEvents {
    /// Actual callback entry is observed before stopping its executor.
    /// 停止执行器前观测真实回调入场。
    entered: AtomicBool,
    /// Releasing this condition permits the original native worker to return.
    /// 释放此条件允许原原生工作者返回。
    released: Mutex<bool>,
    /// Wake the original worker without replacing its callback or operation.
    /// 唤醒原工作者，不替换回调或操作。
    changed: Condvar,
    /// Committed events prove that recovery did not replay initialization or business.
    /// 已提交事件证明恢复没有重放初始化或业务。
    values: Mutex<Vec<VulcanPluginUiInvalidation>>,
}
impl RecoveryEvents {
    /// Release the callback during ordinary completion or assertion unwind; returns no synthetic result.
    /// 正常完成或断言展开期间释放回调；不返回伪造结果。
    fn release(&self) {
        *self
            .released
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = true;
        self.changed.notify_all();
    }

    /// Await the original native entry within WAIT; return only after real worker observation.
    /// 在 WAIT 内等待原原生入场；仅在真实工作者可观测后返回。
    async fn wait_entered(&self) {
        // One deadline bounds the test without fabricating callback completion.
        // 单个截止时间约束测试，不伪造回调完成。
        let deadline = Instant::now() + WAIT;
        while !self.entered.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "native recovery callback never entered"
            );
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }
}
impl VulcanPluginUiEventPublisher for RecoveryEvents {
    /// Commit event only after any requested native wait; return actual lock errors.
    /// 仅在请求的原生等待结束后提交 event；返回真实锁错误。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        if event.topic == "block" {
            self.entered.store(true, Ordering::Release);
            // The actual callback retains this barrier until test-controlled release.
            // 实际回调保留此屏障，直到测试控制释放。
            let mut released = self.released.lock().map_err(|error| error.to_string())?;
            while !*released {
                released = self
                    .changed
                    .wait(released)
                    .map_err(|error| error.to_string())?;
            }
        }
        self.values
            .lock()
            .map_err(|error| error.to_string())?
            .push(event);
        Ok(())
    }
}

/// Keep original package and callback ownership across multiple Tokio executors.
/// 跨多个 Tokio 执行器保留原包及回调归属。
struct RecoveryFixture {
    /// Existing real reusable package under test.
    /// 被测既有真实可复用包。
    fixture: Fixture,
    /// Native effects outlive every dropped observation task.
    /// 原生副作用比全部丢弃的观测任务存活更久。
    events: Arc<RecoveryEvents>,
}
impl Drop for RecoveryFixture {
    /// Release blocked native work before temporary fixture files leave scope.
    /// 在临时夹具文件离开作用域前释放阻塞的原生工作。
    fn drop(&mut self) {
        self.events.release();
    }
}
impl RecoveryFixture {
    /// Build a real callback barrier in initialization when initialization is true, otherwise in business.
    /// initialization 为真时在初始化中构建真实回调屏障，否则在业务中构建。
    /// Returns an enabled package with a short drain observation budget and unchanged business deadline.
    /// 返回具有短排空观测预算且业务截止时间不变的已启用包。
    fn new(initialization: bool) -> Self {
        // Bounded drain reports must not depend on the native callback returning.
        // 有界排空报告不能依赖原生回调返回。
        let settings = VulcanPluginRuntimeSettings {
            shutdown_timeout_ms: 50,
            ..VulcanPluginRuntimeSettings::default()
        };
        // Retain the established real fixture and its public invocation path.
        // 保留既有真实夹具及公开调用路径。
        let fixture = Fixture::with_settings(10_000, 100, settings);
        // Change only the original package's declared native permission.
        // 仅改变原包声明的原生权限。
        let path = fixture.root.path().join("bundled/idle-plugin/plugin.yaml");
        let mut manifest: Value =
            serde_yaml::from_str(&std::fs::read_to_string(&path).expect("manifest"))
                .expect("manifest mapping");
        manifest["permissions"] = json!(["config", "ui_host_context"]);
        std::fs::write(&path, serde_yaml::to_string(&manifest).expect("encode")).expect("write");
        // Instrument exact known functions before catalog publication, never substitute a mock VM.
        // 在目录发布前对精确已知函数插桩，绝不替换为模拟 VM。
        let source_path = fixture
            .root
            .path()
            .join("bundled/idle-plugin/runtime/plugin.bundle.lua");
        let mut source = std::fs::read_to_string(&source_path).expect("real bundle");
        assert!(
            source.contains("    count = count + 1"),
            "fixture dispatcher changed"
        );
        source = source.replace("    count = count + 1", "    count = count + 1\n    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='business',revision=count})\n    if active.mode == 'block' then vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='block',revision=count}) end");
        source = source.replace("function M.shutdown(envelope)", "function M.shutdown(envelope)\n    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='closed',revision=count})");
        if initialization {
            source = format!(
                "vulcan.host.call('vulcan.plugin.ui.invalidate', {{topic='block',revision=0}})\n{source}"
            );
        }
        std::fs::write(&source_path, source).expect("instrument bundle");
        fixture
            .manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("publish instrumentation");
        // The host service itself performs the controlled native wait.
        // 宿主服务自身执行受控原生等待。
        let events = Arc::new(RecoveryEvents::default());
        fixture
            .manager
            .install_host_services(VulcanPluginHostServices {
                storage: None,
                vldb: None,
                ui_events: Some(events.clone()),
            })
            .expect("native service");
        Self { fixture, events }
    }
}

/// Replacing an executor cannot release or replay the original VM, operation, domain or request borrow.
/// 替换执行器不能释放或重放原 VM、操作、域或请求借用。
#[test]
fn hot_reuse_executor_loss_recovers_original_initialization_and_business() {
    for initialization in [true, false] {
        // Two real execution phases must preserve the same ownership checkpoints.
        // 两个真实执行阶段必须保留同一归属检查点。
        let recovery = RecoveryFixture::new(initialization);
        // Stop exactly this runtime after observing the original native worker.
        // 观测原原生工作者后，停止精确的此运行时。
        let previous = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("first executor");
        let (invocation_id, operation_id, domain_id, pool_id) = previous.block_on(async {
            // The task is intentionally abandoned by executor shutdown, not replaced or replayed.
            // 任务有意因执行器关闭而放弃，不替换或重放。
            let manager = recovery.fixture.manager.clone();
            let request = VulcanPluginInvocationRequest {
                tool_id: "idle_ping".to_owned(),
                arguments: json!({"mode": if initialization { "ok" } else { "block" }}),
                owner_session_id: "owner".to_owned(),
                workspace_root: recovery.fixture.root.path().to_path_buf(),
            };
            tokio::spawn(async move { manager.invoke(request).await });
            recovery.events.wait_entered().await;
            // Capture actual published identities while the original callback is still blocked.
            // 原回调仍阻塞时捕获真实已发布身份。
            let call = recovery
                .fixture
                .manager
                .formal_calls("owner")
                .expect("original call")
                .into_iter()
                .find(|call| !call.retired)
                .expect("in-flight call");
            let domain = recovery
                .fixture
                .manager
                .reusable_tool_domains("owner")
                .expect("original domain")
                .into_iter()
                .find(|domain| domain.borrowers == 1)
                .expect("original borrow");
            (
                call.invocation_id,
                call.operation.expect("core operation").operation_id,
                domain.domain_id,
                domain.pool_id,
            )
        });
        previous.shutdown_timeout(WAIT);
        assert!(
            !recovery
                .fixture
                .owner()
                .reuse_observer
                .load(Ordering::Acquire),
            "stopped observer must release only its observer flag"
        );
        // The replacement executor resumes cleanup for those exact identities.
        // 替代执行器为这些精确身份恢复清理。
        let replacement = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("replacement executor");
        replacement.block_on(async {
            let before = recovery
                .fixture
                .manager
                .formal_call_status(&invocation_id, "owner")
                .expect("interrupted status");
            assert!(before.requires_cleanup_recovery && !before.retired);
            let pending = recovery
                .fixture
                .manager
                .drain_owner_sessions("owner")
                .await
                .expect("finite drain");
            assert!(!pending.failures.is_empty());
            let retained = recovery
                .fixture
                .manager
                .reusable_tool_domains("owner")
                .expect("retained domain")
                .into_iter()
                .find(|domain| domain.domain_id == domain_id)
                .expect("same domain");
            assert_eq!(retained.pool_id, pool_id);
            assert_eq!(retained.borrowers, 1);
            assert_eq!(
                recovery
                    .fixture
                    .owner()
                    .host
                    .runtime()
                    .resources()
                    .expect("actual resources")
                    .resident,
                1
            );
            recovery.events.release();
            let deadline = Instant::now() + WAIT;
            loop {
                let report = recovery
                    .fixture
                    .manager
                    .drain_owner_sessions("owner")
                    .await
                    .expect("resume exact cleanup");
                if report.failures.is_empty() {
                    break;
                }
                assert!(Instant::now() < deadline, "{report:?}");
            }
            let terminal = recovery
                .fixture
                .manager
                .formal_call_status(&invocation_id, "owner")
                .expect("same terminal call");
            assert!(terminal.retired);
            assert_eq!(
                terminal
                    .operation
                    .expect("original core evidence")
                    .operation_id,
                operation_id
            );
            assert!(
                recovery
                    .fixture
                    .manager
                    .reusable_tool_domains("owner")
                    .expect("retired domains")
                    .is_empty()
            );
            recovery
                .fixture
                .manager
                .forget_formal_call(&invocation_id, "owner")
                .expect("acknowledge original result");
            recovery.fixture.close().await;
        });
        // Native commits prove initialization/business was never replayed on another VM.
        // 原生提交证明初始化／业务从未在其他 VM 上重放。
        let values = recovery.events.values.lock().expect("real effects");
        assert_eq!(
            values.iter().filter(|event| event.topic == "block").count(),
            1
        );
        assert_eq!(
            values
                .iter()
                .filter(|event| event.topic == "business")
                .count(),
            usize::from(!initialization)
        );
        if !initialization {
            assert_eq!(
                values
                    .iter()
                    .filter(|event| event.topic == "closed")
                    .count(),
                1
            );
        }
    }
}

/// A failed diagnostic publication must retain the same failure until publication succeeds on a later pass.
/// 诊断发布失败必须保留同一失败，直到后续轮次发布成功。
#[tokio::test]
async fn hot_reuse_failed_publication_fences_admission_and_retries_original_evidence() {
    // Create one real pending idle shutdown while the runtime remains healthy.
    // 运行时健康期间创建一个真实待空闲关闭。
    let fixture = Fixture::new(100, 100);
    fixture
        .invoke("owner", fixture.root.path().to_path_buf(), "shutdown_error")
        .await
        .expect("business success");
    // Deliberately poison the actual host diagnostic lock; no native identity or result is fabricated.
    // 刻意使真实宿主诊断锁中毒；不伪造原生身份或结果。
    let state = fixture.manager.state.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = state
                .cleanup_diagnostics
                .lock()
                .expect("diagnostic fault injection");
            panic!("controlled diagnostic publication failure");
        })
        .join()
        .is_err()
    );
    // Observe the real maintenance error and its retained original shutdown operation.
    // 观测真实维护错误及其保留的原关闭操作。
    let deadline = Instant::now() + WAIT;
    let failed = loop {
        let domains = fixture
            .manager
            .reusable_tool_domains("owner")
            .expect("failure evidence remains queryable");
        if let Some(domain) = domains.into_iter().find(|domain| {
            domain.observer_error.is_some() && !domain.finalization_failures.is_empty()
        }) {
            break domain;
        }
        assert!(
            Instant::now() < deadline,
            "maintenance failure was not retained"
        );
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    };
    assert!(
        fixture.owner().reuse_observer.load(Ordering::Acquire),
        "maintenance must keep supervising original ownership"
    );
    fixture
        .invoke("other", fixture.root.path().to_path_buf(), "ok")
        .await
        .expect_err("unhealthy maintenance must fence new domains");
    assert!(
        fixture
            .manager
            .reusable_tool_domains("other")
            .expect("no substitute domain")
            .is_empty()
    );
    // Repair only the injected mutex poison; the normal supervisor must publish and recover the original evidence.
    // 仅修复注入的锁中毒；正常监督者必须发布并恢复原证据。
    fixture.manager.state.cleanup_diagnostics.clear_poison();
    loop {
        let domain = fixture
            .manager
            .reusable_tool_domains("owner")
            .expect("recovery status")
            .into_iter()
            .find(|domain| domain.domain_id == failed.domain_id)
            .expect("same failure domain");
        if domain.observer_error.is_none() && domain.pool_retired {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "maintenance did not recover after lock repair"
        );
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    }
    assert!(
        fixture.manager.snapshot().plugins["idle-plugin"]
            .diagnostics
            .iter()
            .any(|message| message.contains("shutdown-marker"))
    );
    assert_eq!(
        fixture
            .manager
            .reusable_tool_domains("owner")
            .expect("same core failure")
            .into_iter()
            .find(|domain| domain.domain_id == failed.domain_id)
            .expect("original domain")
            .finalization_failures
            .iter()
            .map(|failure| &failure.operation_id)
            .collect::<Vec<_>>(),
        failed
            .finalization_failures
            .iter()
            .map(|failure| &failure.operation_id)
            .collect::<Vec<_>>()
    );
    // Explicit cleanup recovers any rejected caller's accounting without creating or replaying business.
    // 显式清理恢复被拒调用方的记账，不创建或重放业务。
    fixture
        .manager
        .drain_owner_sessions("other")
        .await
        .expect("recover rejected caller");
    for call in fixture
        .manager
        .formal_calls("other")
        .expect("retained rejected callers")
    {
        fixture
            .manager
            .forget_formal_call(&call.invocation_id, "other")
            .expect("acknowledge rejection");
    }
    fixture
        .manager
        .forget_reusable_tool_domain(&failed.domain_id, "owner")
        .expect("acknowledge original closing failure");
    fixture.close().await;
}
