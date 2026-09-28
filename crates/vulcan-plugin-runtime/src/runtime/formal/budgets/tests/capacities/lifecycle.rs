//! Real lifecycle transitions release exact native capacity owners and restore guarantees before activation.
//! 真实生命周期转换释放精确原生容量所有者，并在启用前恢复保证。

use super::*;
use crate::VulcanPluginRuntimeState;

/// Return settings with a one-slot Hook guarantee; all other limits come from the actual host defaults.
/// 返回具有单槽 Hook 保证的设置；其余上限均来自实际宿主默认值。
fn reserved_hook_settings() -> VulcanPluginRuntimeSettings {
    // Preserve the authoritative parent shape while adding one complete explicit purpose override.
    // 保留权威父级结构，并添加一份完整显式用途覆盖。
    let mut settings = VulcanPluginRuntimeSettings::default();
    // The existing fixture resolver derives the exact native policy from these settings.
    // 既有夹具解析器从这些设置派生精确原生策略。
    let config = dedicated(&settings, "hook-plugin", 1);
    settings.capacity_budgets.insert(
        "hook-plugin".into(),
        BTreeMap::from([(VulcanPluginCapacityPurpose::Hook, config)]),
    );
    settings
}

/// Actual disable callback completion precedes capacity release; reactivation restores the guarantee before Lua runs.
/// 实际停用回调完成先于容量释放；重新启用在 Lua 执行前恢复保证。
#[tokio::test(flavor = "current_thread")]
async fn formal_capacity_lifecycle_disable_and_reenable_preserve_callback_order() {
    // Exercise the same manager, filesystem packages and original parent across the complete cycle.
    // 在完整循环中使用同一管理器、文件系统包及原父级。
    let fixture = Fixture::new(reserved_hook_settings());
    assert!(
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("prewarm")
    );
    // Keep the issued identity to prove release does not silently reopen the old native owner.
    // 保留签发身份，以证明释放后不会静默重开旧原生所有者。
    let original = fixture
        .manager
        .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect("original status")
        .expect("registered");
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("begin disable");
    // Real session cleanup must finish before the host invokes the disable callback.
    // 宿主调用停用回调前，真实会话清理必须完成。
    let report = fixture
        .manager
        .drain_plugin_sessions("hook-plugin")
        .await
        .expect("drain");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    // This declared business error proves the lifecycle export ran, rather than being blocked by early capacity closure.
    // 此声明业务错误证明生命周期导出实际执行，而非被过早容量关闭阻止。
    let callback_error = fixture
        .manager
        .invoke_lifecycle("hook-plugin", VulcanPluginLifecycleEvent::Disable)
        .await
        .expect_err("declared disable marker");
    assert!(
        callback_error
            .to_string()
            .contains("enable-rollback-marker"),
        "{callback_error}"
    );
    // Persisted disabled policy remains explicit, while the callback failure remains a separate diagnostic.
    // 持久停用策略保持明确，回调失败仍为独立诊断。
    let disabled = BTreeSet::from(["hook-plugin".to_owned()]);
    // Final publication is also the production capacity retirement boundary.
    // 最终发布也是生产容量回收边界。
    let stopped = fixture
        .manager
        .finish_lifecycle_transition(
            "hook-plugin",
            &disabled,
            &BTreeSet::new(),
            vec![callback_error.to_string()],
        )
        .expect("finish disable");
    assert_eq!(
        stopped.plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::DisabledWithCleanupWarning
    );
    for purpose in [
        VulcanPluginCapacityPurpose::Hook,
        VulcanPluginCapacityPurpose::Lifecycle,
    ] {
        assert!(
            fixture
                .manager
                .formal_capacity_status("hook-plugin", purpose)
                .expect("released status")
                .is_none()
        );
    }
    assert_eq!(
        fixture
            .status("hook-plugin")
            .snapshot
            .committed_resident_vms,
        0
    );
    fixture
        .manager
        .begin_enabling("hook-plugin")
        .expect("restore reservations before activation");
    // A new empty native capacity already owns its original guarantee while the catalog is still Enabling.
    // 目录仍处于启用中时，新空原生容量已持有原保证。
    let restored = fixture
        .manager
        .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect("restored status")
        .expect("reserved before execution");
    assert_ne!(restored.snapshot.capacity_id, original.snapshot.capacity_id);
    assert_eq!(restored.snapshot.committed_resident_vms, 1);
    assert_eq!(restored.snapshot.resources.resident, 0);
    assert!(!restored.retirement_requested);
    fixture
        .manager
        .probe_plugin("hook-plugin")
        .await
        .expect("activation probe");
    // Successful recovery acknowledges the old callback diagnostic through the real lifecycle projection.
    // 成功恢复通过真实生命周期投影确认旧回调诊断。
    let enabled = fixture
        .manager
        .finish_lifecycle_transition(
            "hook-plugin",
            &BTreeSet::new(),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("publish enabled");
    assert_eq!(
        enabled.plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    assert!(
        enabled.plugins["hook-plugin"].diagnostics.is_empty(),
        "{:?}",
        enabled.plugins["hook-plugin"].diagnostics
    );
    assert_eq!(
        enabled.revision.checked_add(1),
        Some(
            fixture
                .manager
                .state
                .next_revision
                .load(std::sync::atomic::Ordering::Relaxed)
        )
    );
    assert!(
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("reactivated Hook")
    );
    fixture.close().await;
}

/// Partial reactivation cannot retain one new guarantee when a later configured purpose cannot fit.
/// 后续配置用途无法满足时，局部重新启用不能保留此前新建的一份保证。
#[tokio::test(flavor = "current_thread")]
async fn formal_capacity_lifecycle_activation_rolls_back_partial_reservations() {
    // Exactly two explicit guarantees fill the parent before disable releases them.
    // 两份显式保证在停用释放前恰好占满父级。
    let mut settings = reserved_hook_settings();
    settings.parent.max_resident_vms = 2;
    settings.parent.max_running_calls = 2;
    // The second purpose uses a distinct native identity with the same immutable one-slot guarantee.
    // 第二个用途使用独立原生身份，具有相同不可变单槽保证。
    let ui = dedicated(&settings, "hook-plugin", 1);
    settings
        .capacity_budgets
        .get_mut("hook-plugin")
        .expect("configured plugin")
        .insert(VulcanPluginCapacityPurpose::UiAction, ui);
    // All operations below use the production parent and actual plugin session resources.
    // 下列全部操作使用生产父级及实际插件会话资源。
    let fixture = Fixture::new(settings);
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("stop unused configured plugin");
    fixture
        .manager
        .finish_lifecycle_transition(
            "hook-plugin",
            &BTreeSet::from(["hook-plugin".into()]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("release both guarantees");
    // One real shared VM leaves room for the first registration but not the second.
    // 一个真实公共 VM 为第一项注册保留空间，但不足以满足第二项。
    let opened = fixture.tool("idle_open", json!({})).await;
    // Failed activation must not advance the published routing generation.
    // 失败启用不能推进已发布路由代次。
    let generation = fixture.manager.snapshot().plugins["hook-plugin"].generation;
    // Registration fails before any activation callback or Hook instance is admitted.
    // 注册在任何启用回调或 Hook 实例入场前失败。
    let failure = fixture
        .manager
        .begin_enabling("hook-plugin")
        .expect_err("insufficient combined guarantee");
    assert!(failure.to_string().contains("capacity"), "{failure}");
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].generation,
        generation
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    for purpose in [
        VulcanPluginCapacityPurpose::Hook,
        VulcanPluginCapacityPurpose::UiAction,
    ] {
        assert!(
            fixture
                .manager
                .formal_capacity_status("hook-plugin", purpose)
                .expect("rollback status")
                .is_none()
        );
    }
    assert_eq!(
        fixture
            .status("hook-plugin")
            .snapshot
            .committed_resident_vms,
        0
    );
    assert_eq!(fixture.status("idle-plugin").snapshot.resources.resident, 1);
    fixture
        .tool("idle_close", json!({"session_id":opened["session_id"]}))
        .await;
    fixture
        .manager
        .begin_enabling("hook-plugin")
        .expect("explicit retry after real capacity release");
    assert_eq!(
        fixture
            .status("hook-plugin")
            .snapshot
            .committed_resident_vms,
        2
    );
    fixture
        .manager
        .finish_lifecycle_transition(
            "hook-plugin",
            &BTreeSet::from(["hook-plugin".into()]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("compensate unused activation");
    fixture.close().await;
}

/// Incomplete physical cleanup keeps both a diagnostic and an admission fence until explicit lifecycle recovery succeeds.
/// 未完成物理清理同时保留诊断和入场屏障，直到明确生命周期恢复成功。
#[tokio::test(flavor = "current_thread")]
async fn formal_capacity_lifecycle_retained_member_blocks_reactivation_and_diagnostic_clear() {
    // The persistent Hook creates a real native member which must not disappear on metadata publication.
    // 持久 Hook 创建真实原生成员，不能因元数据发布而消失。
    let fixture = Fixture::new(reserved_hook_settings());
    assert!(
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("real member")
    );
    // Retain the exact original owner for comparison across unsuccessful cleanup attempts.
    // 保留精确原所有者，比较不成功清理尝试前后状态。
    let original = fixture
        .manager
        .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect("status")
        .expect("registered");
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("draining authority");
    // Simulate a terminal disable publication whose resource drain has not actually completed.
    // 模拟资源排空尚未实际完成的停用终态发布。
    let disabled = fixture
        .manager
        .finish_lifecycle_transition(
            "hook-plugin",
            &BTreeSet::from(["hook-plugin".into()]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("publish cleanup warning");
    assert_eq!(
        disabled.plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::DisabledWithCleanupWarning
    );
    // Preflight rejects the whole release before closing an otherwise healthy original capacity.
    // 前置检查在关闭原健康容量之前拒绝整组释放。
    let retained = fixture
        .manager
        .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect("retained status")
        .expect("retained guarantee");
    assert_eq!(retained.snapshot.capacity_id, original.snapshot.capacity_id);
    assert_eq!(retained.snapshot.committed_resident_vms, 1);
    assert!(retained.retirement_requested);
    assert!(!retained.snapshot.closing);
    assert!(
        fixture
            .manager
            .begin_enabling("hook-plugin")
            .expect_err("member still retained")
            .to_string()
            .contains("retained member pools")
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].generation,
        disabled.plugins["hook-plugin"].generation
    );
    // Physical drain uses the original registered session rather than reconstructing or replaying it.
    // 物理排空使用原已注册会话，不重建或重放。
    let report = fixture
        .manager
        .drain_plugin_sessions("hook-plugin")
        .await
        .expect("recover real drain");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    fixture
        .manager
        .clear_cleanup_diagnostics_if_fully_drained("hook-plugin")
        .expect("attempt diagnostic acknowledgement");
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::DisabledWithCleanupWarning
    );
    fixture
        .manager
        .begin_enabling("hook-plugin")
        .expect("retire original and restore guarantee");
    // Explicit recovery produces a new owner only after the original member and capacity are truly gone.
    // 明确恢复仅在原成员及容量实际消失后产生新所有者。
    let recovered = fixture
        .manager
        .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect("recovered status")
        .expect("new guarantee");
    assert_ne!(
        recovered.snapshot.capacity_id,
        original.snapshot.capacity_id
    );
    assert!(!recovered.retirement_requested);
    fixture
        .manager
        .finish_lifecycle_transition(
            "hook-plugin",
            &BTreeSet::new(),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("publish recovery");
    assert!(
        fixture.manager.snapshot().plugins["hook-plugin"]
            .diagnostics
            .is_empty()
    );
    fixture.close().await;
}

/// A disable retry finishes old capacity retirement before its own callback, without allowing ordinary work to reopen it.
/// 停用重试在自身回调前完成旧容量回收，不允许普通工作重新打开容量。
#[tokio::test(flavor = "current_thread")]
async fn formal_capacity_lifecycle_disable_retry_runs_callback_after_member_drain() {
    // A real persistent member makes the first terminal publication retain explicit cleanup ownership.
    // 真实持久成员使首次终态发布保留明确清理归属。
    let fixture = Fixture::new(reserved_hook_settings());
    assert!(
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("actual member")
    );
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("first disable");
    fixture
        .manager
        .finish_lifecycle_transition(
            "hook-plugin",
            &BTreeSet::from(["hook-plugin".into()]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("incomplete disable projection");
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("explicit cleanup retry");
    // Lifecycle recovery cannot discard a still-retained member to make its callback fit.
    // 生命周期恢复不能丢弃仍保留的成员来让回调入场。
    let blocked = fixture
        .manager
        .invoke_lifecycle("hook-plugin", VulcanPluginLifecycleEvent::Disable)
        .await
        .expect_err("retry still owns a member");
    assert!(
        blocked.to_string().contains("retained member pools"),
        "{blocked}"
    );
    // The second lifecycle attempt consumes the same real drain path before running its export.
    // 第二次生命周期尝试先消费同一真实排空路径，再执行导出。
    let report = fixture
        .manager
        .drain_plugin_sessions("hook-plugin")
        .await
        .expect("drain old member");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    // The declared error proves recovery reached the exact callback body after retiring the old owner.
    // 声明错误证明恢复在回收旧所有者后到达精确回调主体。
    let callback = fixture
        .manager
        .invoke_lifecycle("hook-plugin", VulcanPluginLifecycleEvent::Disable)
        .await
        .expect_err("real disable callback marker");
    assert!(
        callback.to_string().contains("enable-rollback-marker"),
        "{callback}"
    );
    fixture
        .manager
        .finish_lifecycle_transition(
            "hook-plugin",
            &BTreeSet::from(["hook-plugin".into()]),
            &BTreeSet::new(),
            vec![callback.to_string()],
        )
        .expect("release retry capacity after callback");
    assert!(
        fixture
            .manager
            .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
            .expect("retired Hook owner")
            .is_none()
    );
    assert!(
        fixture
            .manager
            .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Lifecycle)
            .expect("retired callback owner")
            .is_none()
    );
    fixture.close().await;
}

/// Real uninstallation releases the callback purpose before deleting the disabled package and its environment.
/// 真实卸载在删除停用包及环境之前释放回调用途。
#[tokio::test(flavor = "current_thread")]
async fn formal_capacity_lifecycle_uninstall_releases_callback_owner() {
    // Future package policy is reserved by startup using the same configured native guarantee.
    // 启动使用同一配置原生保证，为未来包策略建立预留。
    let mut settings = VulcanPluginRuntimeSettings::default();
    // The lifecycle purpose remains available to the real uninstall callback before removal.
    // 生命周期用途在移除前保持可供真实卸载回调使用。
    let config = dedicated(&settings, "local-plugin", 1);
    settings.capacity_budgets.insert(
        "local-plugin".into(),
        BTreeMap::from([(VulcanPluginCapacityPurpose::Lifecycle, config)]),
    );
    // Reuse the actual production fixture without mocking installation or authorization state.
    // 复用实际生产夹具，不模拟安装或授权状态。
    let fixture = Fixture::new(settings);
    // This source is test-owned and inspected only after the explicit callback declaration is written.
    // 此来源归测试所有，仅在写入明确回调声明后进行检查。
    let source = fixture.root.path().join("source/local-plugin");
    crate::runtime::tests::write_user_plugin_package(&source, "local-plugin");
    // Parse the verified fixture manifest shape before adding its lifecycle export.
    // 添加生命周期导出前，解析已核实夹具清单结构。
    let mut manifest: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(source.join("plugin.yaml")).expect("source manifest"),
    )
    .expect("parse source manifest");
    manifest["lua_runtime"]["lifecycle"] =
        serde_yaml::to_value(BTreeMap::from([("uninstall", "shutdown")]))
            .expect("declared uninstall function");
    std::fs::write(
        source.join("plugin.yaml"),
        serde_yaml::to_string(&manifest).expect("manifest bytes"),
    )
    .expect("write uninstall declaration");
    // Actual inspection freezes the source digest and declared permissions consumed by installation.
    // 实际检查冻结安装消费的来源摘要和声明权限。
    let inspected = fixture
        .manager
        .inspect_local_user_plugin(&source)
        .expect("inspect source");
    fixture
        .manager
        .install_local_user_plugin(
            &source,
            &inspected.content_fingerprint,
            inspected.permissions,
            false,
        )
        .expect("install exact source");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("discover disabled installed package");
    // Real host preparation ownership must prevent deletion even before any native member pool exists.
    // 即使尚无原生成员池，真实宿主准备归属也必须阻止删除。
    let owner = fixture
        .manager
        .formal_runtime_owner()
        .expect("formal owner")
        .expect("attached");
    // The production preparation guard retains the exact capacity while the publication lock is released.
    // 生产准备守卫在发布锁释放期间保留精确容量。
    let preparing = {
        // Follow the same gate order as actual member registration.
        // 遵守实际成员注册的同一门顺序。
        let publication = fixture
            .manager
            .state
            .session_registration_gate
            .lock()
            .expect("publication");
        owner
            .admit_capacity_member(
                "local-plugin",
                VulcanPluginCapacityPurpose::Lifecycle,
                &publication,
            )
            .expect("actual preparation claim")
    };
    // The authoritative installed path must survive the refused destructive transition.
    // 权威已安装路径必须在破坏性转换被拒绝后保留。
    let retained_package = fixture.manager.snapshot().plugins["local-plugin"]
        .package_root
        .clone();
    assert!(
        fixture
            .manager
            .uninstall_user_plugin("local-plugin")
            .expect_err("preparation blocks uninstall")
            .to_string()
            .contains("unfinished member preparation")
    );
    assert!(retained_package.is_dir());
    assert_eq!(
        fixture.manager.snapshot().plugins["local-plugin"].runtime_state,
        VulcanPluginRuntimeState::DisabledWithCleanupWarning
    );
    fixture
        .manager
        .clear_cleanup_diagnostics_if_fully_drained("local-plugin")
        .expect("preserve pending retirement warning");
    assert_eq!(
        fixture.manager.snapshot().plugins["local-plugin"].runtime_state,
        VulcanPluginRuntimeState::DisabledWithCleanupWarning
    );
    drop(preparing);
    fixture
        .manager
        .begin_draining("local-plugin")
        .expect("explicit failed-uninstall cleanup retry");
    fixture
        .manager
        .finish_lifecycle_transition(
            "local-plugin",
            &BTreeSet::new(),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("acknowledge actual preparation completion");
    assert_eq!(
        fixture.manager.snapshot().plugins["local-plugin"].runtime_state,
        VulcanPluginRuntimeState::Disabled
    );
    fixture
        .manager
        .invoke_lifecycle("local-plugin", VulcanPluginLifecycleEvent::Uninstall)
        .await
        .expect("real uninstall callback");
    // The callback has physically retired, while the configured empty purpose still holds its guarantee.
    // 回调已物理退役，但配置的空用途仍持有保证。
    let completed = fixture
        .manager
        .formal_capacity_status("local-plugin", VulcanPluginCapacityPurpose::Lifecycle)
        .expect("callback status")
        .expect("callback capacity");
    assert_eq!(completed.snapshot.retained_pools, 0);
    assert_eq!(completed.snapshot.committed_resident_vms, 1);
    // The published package root is the authority for physical deletion, not a reconstructed installation path.
    // 已发布包根是物理删除的权威，不重新构造安装路径。
    let package = fixture.manager.snapshot().plugins["local-plugin"]
        .package_root
        .clone();
    fixture
        .manager
        .uninstall_user_plugin("local-plugin")
        .expect("uninstall after callback");
    assert!(!package.exists());
    assert!(
        fixture
            .manager
            .formal_capacity_status("local-plugin", VulcanPluginCapacityPurpose::Lifecycle)
            .expect("released uninstall owner")
            .is_none()
    );
    assert_eq!(
        fixture
            .status("local-plugin")
            .snapshot
            .committed_resident_vms,
        0
    );
    fixture.close().await;
}
