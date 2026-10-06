//! Real old Hook preparation retains physical identities and cannot reopen ordinary business or replay initialization.
//! 真实旧 Hook 准备保留物理身份，不能重开普通业务或重放初始化。

use super::*;

/// Observe actual persistent cleanup while preserving deliberate old finalizer failures as business evidence.
/// 观测实际持久清理，同时将刻意旧关闭器失败保留为业务证据。
/// fixture owns the actual Hook; return its physical retirement report, including the expected business failure.
/// fixture 拥有实际 Hook；返回其物理退役报告，包含预期业务失败。
async fn retire_predecessor_hook(fixture: &Fixture) -> crate::VulcanPluginDrainReport {
    tokio::time::timeout(WAIT, async {
        loop {
            // The existing drain API supervises and forgets the same original pool instead of setting test state.
            // 既有排空 API 监督并遗忘相同原池，而非设置测试状态。
            let report = fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("actual predecessor Hook drain observation");
            if report.closed_hook_runtimes == 1 {
                assert!(
                    report.failures.iter().any(|failure| failure
                        .contains("lifecycle must not invoke tool or Hook shutdown")),
                    "the intentional old finalizer failure must remain visible: {:?}",
                    report.failures
                );
                return report;
            }
            assert!(
                !report.failures.is_empty(),
                "physical retirement has not yet been acknowledged"
            );
            assert!(
                report.failures.iter().all(|failure| failure
                    == "persistent cleanup timed out; original ownership remains retained."),
                "unexpected physical cleanup failure: {:?}",
                report.failures
            );
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual predecessor Hook eventually retires")
}

/// Repeated old Hook prewarm reuses actual core identities while old configuration and Pending remain authoritative.
/// 重复旧 Hook 预热复用实际核心身份，旧配置及 Pending 仍保持权威。
/// No inputs or return value; real Lua initialization asserts old-published and ordinary Hook execution is denied.
/// 无输入与返回值；真实 Lua 初始化断言 old-published，普通 Hook 执行被拒绝。
#[test]
fn predecessor_private_hook_reuses_actual_old_initialization_without_business_admission() {
    // Reuse the immutable managed predecessor whose module asserts its actual selected old configuration.
    // 复用不可变托管前驱，其模块断言实际所选旧配置。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual old independent Pending");
    fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual original abort staging");
    // This manager-issued source owns the old pins before actual Hook initialization starts.
    // 此管理器签发来源在实际 Hook 初始化开始前拥有旧固定句柄。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("actual old physical source");
    // A separate actual File proves the same Pending reservation remains independently locked.
    // 独立实际文件证明相同 Pending 预留权保持独立锁定。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual independent lease path"),
        )
        .expect("separate actual File");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("old configured Lua actually initialized")
        );
        // Read the instance produced by the actual formal registry, never a replacement constructor.
        // 读取实际正式注册表产生的实例，绝不使用替代构造器。
        let original = fixture
            .owner()
            .hooks
            .lock()
            .expect("actual Hook registry")
            .get("hook-plugin")
            .cloned()
            .expect("actual initialized old Hook");
        // Capture actual pool, session, initialization operation and private retirement flags.
        // 捕获实际池、会话、初始化操作及私有退役标志。
        let identities = original.target_test_identity();
        assert!(identities.0.is_some() && identities.1.is_some() && identities.2.is_some());
        assert!(identities.3 && !identities.4);
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("same actual old Hook retry")
        );
        assert_eq!(original.target_test_identity(), identities);
        assert!(Arc::ptr_eq(
            &original,
            fixture
                .owner()
                .hooks
                .lock()
                .expect("same actual registry")
                .get("hook-plugin")
                .expect("same actual old Hook")
        ));
        // The sole actual formal parent exposes readiness for the original session, without widening admission.
        // 唯一实际正式父级暴露原会话就绪状态，不扩宽入场。
        let owner = fixture.owner();
        // Query the exact recorded core session and operation rather than inferring success from source presence.
        // 查询精确已记录核心会话及操作，而非从来源存在推断成功。
        let session = owner
            .host
            .runtime()
            .session(identities.1.as_ref().expect("actual session ID"))
            .expect("actual old session remains registered");
        assert_eq!(
            session.phase,
            luaskills::runtime::embedded::EmbeddedSessionPhase::Ready
        );
        assert_eq!(
            session.pool_id,
            *identities.0.as_ref().expect("actual pool ID")
        );
        assert!(session.active_operation.is_none());
        assert_eq!(
            owner
                .host
                .runtime()
                .operation(identities.2.as_ref().expect("actual init ID"))
                .expect("actual old initialization observation")
                .snapshot()
                .expect("actual original initialization snapshot")
                .phase,
            luaskills::runtime::embedded::OperationPhase::Succeeded
        );
        // Select the declared original Hook by semantic plugin identity even though it is absent from enabled exposure.
        // 按语义插件身份选择声明原 Hook，即使其不在启用暴露面中。
        let hook = source
            .plugin()
            .hooks
            .iter()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("real old declared Hook");
        assert!(
            fixture
                .manager
                .invoke_formal_hook(
                    hook,
                    json!({"payload_json":"{}", "host_timeout_ms":1000}),
                    None,
                    Instant::now() + WAIT
                )
                .await
                .is_err(),
            "private preparation must not admit ordinary Hook business"
        );
        assert_eq!(original.target_test_identity(), identities);
        assert!(fixture.manager.snapshot().enabled_hooks.is_empty());
        assert!(
            store
                .predecessor_handoff_blocks_admission("hook-plugin")
                .expect("actual Pending fence")
        );
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        // Cleanup retains the expected business failure while physically retiring this same real Hook.
        // 清理保留预期业务失败，同时物理退役此相同真实 Hook。
        let report = retire_predecessor_hook(&fixture).await;
        assert_eq!(report.closed_hook_runtimes, 1);
        assert!(!report.failures.is_empty());
        assert!(!original.target_test_identity().3 && original.target_test_identity().4);
        assert!(
            fixture
                .owner()
                .hooks
                .lock()
                .expect("actual retired registry")
                .is_empty()
        );
    });
}

/// A foreign manager and equal-field replacement catalog cannot reuse or mutate the actual original Hook.
/// 外来管理器及字段相同替代目录，不能复用或变更实际原 Hook。
/// No inputs or return value; restoring the exact original Arc permits the same initialized IDs to be observed again.
/// 无输入与返回值；恢复精确原 Arc 后，允许再次观测相同已初始化身份。
#[test]
fn predecessor_private_hook_rejects_foreign_manager_and_catalog_without_replacement() {
    // Only the actual original immutable fixture provides old configuration and original transaction authority.
    // 仅实际原不可变夹具提供旧配置及原事务权威。
    let (fixture, _store, mut transaction, mut restoration) = private_owner_fixture();
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual old Pending");
    // Retain the exact actual closed staging Arc before any foreign allocation is introduced.
    // 引入任何外来分配前保留精确实际关闭暂存 Arc。
    let staged = fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual original stage");
    // This original source is signed only by the real original manager.
    // 此原来源仅由真实原管理器签发。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("original source");
    // A separately constructed actual manager supplies a genuinely foreign allocation identity.
    // 独立构建的实际管理器提供真正外来分配身份。
    let foreign = Fixture::new();
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("actual old Hook init")
        );
        // Retain the real original instance across both rejected observations.
        // 跨两次被拒绝观测保留真实原实例。
        let original = fixture
            .owner()
            .hooks
            .lock()
            .expect("actual original registry")
            .get("hook-plugin")
            .cloned()
            .expect("real initialized instance");
        // Original core IDs distinguish intact ownership from a reconstructed equivalent runtime.
        // 原核心身份区分完好所有权与重建等价运行时。
        let identities = original.target_test_identity();
        assert!(
            foreign
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .is_err()
        );
        assert!(
            foreign
                .owner()
                .hooks
                .lock()
                .expect("foreign actual registry")
                .is_empty()
        );
        assert_eq!(original.target_test_identity(), identities);
        // Replace only the actual manager snapshot using a field-identical clone to exercise allocation authority.
        // 仅以字段完全相同克隆替换实际管理器快照，以验证分配权威。
        let foreign_catalog = Arc::new(staged.as_ref().clone());
        *fixture
            .manager
            .state
            .snapshot
            .write()
            .expect("actual snapshot writer") = Arc::clone(&foreign_catalog);
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .is_err()
        );
        assert!(Arc::ptr_eq(&fixture.manager.snapshot(), &foreign_catalog));
        assert_eq!(original.target_test_identity(), identities);
        assert!(Arc::ptr_eq(
            &original,
            fixture
                .owner()
                .hooks
                .lock()
                .expect("unchanged original registry")
                .get("hook-plugin")
                .expect("same original instance")
        ));
        *fixture
            .manager
            .state
            .snapshot
            .write()
            .expect("restore exact real original stage") = Arc::clone(&staged);
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("same original source after actual Arc restoration")
        );
        assert_eq!(original.target_test_identity(), identities);
        retire_predecessor_hook(&fixture).await;
    });
}

/// Actual pool forgetting consumes the original initialization permanently without releasing the Pending source fence.
/// 实际池遗忘永久消费原初始化，而不释放 Pending 来源屏障。
/// No inputs or return value; real drain reports business failure separately from physical retirement and rejects replay.
/// 无输入与返回值；真实排空区分业务失败与物理退役，并拒绝重放。
#[test]
fn predecessor_private_hook_actual_retirement_rejects_same_source_reinitialization() {
    // Existing production preparation supplies only real original predecessor ownership and completed drain.
    // 既有生产准备仅提供真实原前驱所有权及已完成排空。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("real original Pending");
    fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("real old closed successor");
    // The same source must retain the consumed attempt after its initialized Hook physically retires.
    // 其已初始化 Hook 物理退役后，相同来源必须保留已消费尝试。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("same original source");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("actual original old initialization")
        );
        // Retain the actual original runtime only for observation through its real cleanup.
        // 仅为跨真实清理观测保留实际原运行时。
        let original = fixture
            .owner()
            .hooks
            .lock()
            .expect("actual original Hook registry")
            .get("hook-plugin")
            .cloned()
            .expect("real Hook instance");
        // Freeze the actual IDs before core forget removes their live scheduler records.
        // 核心遗忘移除活动调度器记录之前，固定实际身份。
        let identities = original.target_test_identity();
        assert!(identities.3 && !identities.4);
        // Production drain exposes the deliberately failing old finalizer without disguising physical success.
        // 生产排空暴露刻意失败的旧关闭器，不掩盖物理成功。
        let report = retire_predecessor_hook(&fixture).await;
        assert_eq!(report.closed_hook_runtimes, 1);
        assert!(
            !report.failures.is_empty(),
            "old finalizer business failure is expected"
        );
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("actual finalizer events")
                .iter()
                .filter(|event| event.topic == "wrong-finalizer")
                .count(),
            1
        );
        assert!(
            fixture
                .owner()
                .hooks
                .lock()
                .expect("actual physically retired registry")
                .is_empty()
        );
        // This observation retains historical IDs but reports actual private-origin release and retirement.
        // 此观测保留历史身份，但报告实际私有来源释放及退役。
        let retired = original.target_test_identity();
        assert_eq!(retired.0, identities.0);
        assert_eq!(retired.1, identities.1);
        assert_eq!(retired.2, identities.2);
        assert!(!retired.3 && retired.4);
        // The sole actual formal parent must no longer own any core records for these original IDs.
        // 唯一实际正式父级不得再拥有这些原身份的任何核心记录。
        let owner = fixture.owner();
        assert_eq!(
            owner
                .host
                .runtime()
                .pool_resources(identities.0.as_ref().expect("actual old pool"))
                .expect_err("actual old pool was forgotten")
                .code,
            luaskills::runtime::embedded::EmbeddedErrorCode::NotFound
        );
        assert_eq!(
            owner
                .host
                .runtime()
                .session(identities.1.as_ref().expect("actual old session"))
                .expect_err("actual old session was forgotten")
                .code,
            luaskills::runtime::embedded::EmbeddedErrorCode::NotFound
        );
        assert_eq!(
            owner
                .host
                .runtime()
                .operation(identities.2.as_ref().expect("actual old initialization"))
                .err()
                .expect("actual old operation was forgotten")
                .code,
            luaskills::runtime::embedded::EmbeddedErrorCode::NotFound
        );
        // Physical retirement and cleanup observations must preserve the originally signed routing allocation.
        // 物理退役及清理观测必须保留最初已签发路由分配。
        let current = fixture.manager.snapshot();
        // This is the actual closed allocation signed by Source, never a reconstructed value-equal catalog.
        // 此为 Source 签发的实际关闭分配，绝非重建的值相等目录。
        let staged = source.staged_catalog();
        assert!(Arc::ptr_eq(&current, staged));
        assert_eq!(current.revision, staged.revision);
        assert_eq!(
            current.plugins["hook-plugin"].generation,
            staged.plugins["hook-plugin"].generation
        );
        assert_eq!(
            current.plugins["hook-plugin"].runtime_state,
            staged.plugins["hook-plugin"].runtime_state
        );
        assert_eq!(
            fixture
                .manager
                .active_session_count("hook-plugin")
                .expect("actual retired private Hook count"),
            0
        );
        if fixture
            .manager
            .has_cleanup_diagnostics("hook-plugin")
            .expect("actual post-retirement cleanup observation")
        {
            // Display-only cleanup keeps its real evidence visible without replacing Source's routing Arc.
            // 仅展示清理保持真实凭据可见，不替换 Source 路由 Arc。
            let observed = fixture
                .manager
                .observe_catalog(&current)
                .expect("actual post-retirement diagnostic projection");
            assert!(
                !observed.plugins["hook-plugin"].diagnostics.is_empty(),
                "recorded cleanup evidence must reach the catalog projection"
            );
        }
        // The unchanged original route exposes the consumed attempt directly; no test-side pointer replacement is needed.
        // 未变原路由直接暴露已消费尝试；不需要测试端替换指针。
        let consumed_failure = fixture
            .manager
            .prewarm_activation_predecessor_hook(&source)
            .await
            .expect_err("same actual source cannot replay initialization after retirement");
        assert!(
            consumed_failure.message.contains("already attempted"),
            "{}",
            consumed_failure.message
        );
        assert!(Arc::ptr_eq(&current, &fixture.manager.snapshot()));
        assert!(
            fixture
                .owner()
                .hooks
                .lock()
                .expect("no replacement registry entry")
                .is_empty()
        );
        assert!(
            store
                .predecessor_handoff_blocks_admission("hook-plugin")
                .expect("Pending remains closed")
        );
        assert_eq!(
            store
                .deactivate(
                    "hook-plugin",
                    source
                        .original()
                        .predecessor()
                        .expect("actual old selection")
                        .revision
                )
                .expect_err("ordinary admission remains blocked by real Pending")
                .code,
            crate::VulcanPluginStoreErrorCode::Busy
        );
        assert!(fixture.manager.snapshot().enabled_hooks.is_empty());
    });
}
