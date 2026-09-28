//! Recover original reusable ownership after executor loss and failed diagnostic publication.
//! 在执行器丢失及诊断发布失败后恢复原始可复用归属。

use super::*;

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
