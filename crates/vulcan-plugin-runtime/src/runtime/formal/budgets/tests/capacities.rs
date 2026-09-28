//! Real production admission selects exact purpose capacity and preserves explicit control ownership.
//! 真实生产入场选择精确用途容量，并保留显式控制归属。

use super::*;
use luaskills::runtime::embedded::{EmbeddedCapacityConfig, PoolKind};

mod lifecycle;

/// Derive one dedicated purpose config from settings and plugin_id; minimum selects its explicit guarantee.
/// 从 settings 及 plugin_id 派生单个专用用途配置；minimum 选择显式保证。
/// Return a one-VM policy using the original aggregate byte budget and queue semantics.
/// 返回单 VM 策略，使用原聚合字节预算及队列语义。
fn dedicated(
    settings: &VulcanPluginRuntimeSettings,
    plugin_id: &str,
    minimum: usize,
) -> EmbeddedCapacityConfig {
    // Reuse the authoritative resolver instead of copying a second set of parent defaults.
    // 复用权威解析器，不复制第二份父级默认值。
    let mut config = settings
        .capacity_budget(plugin_id, VulcanPluginCapacityPurpose::Hook)
        .0;
    config.resources.kind = PoolKind::Dedicated;
    config.resources.min_resident_vms = minimum;
    config.resources.max_resident_vms = 1;
    config.resources.max_running_calls = 1;
    config.max_queued_calls = 1;
    config
}

/// Explicit guarantees exist before the first call and cannot be borrowed by another plugin's fixed session.
/// 显式保证在首次调用前存在，不能被另一插件的固定会话借用。
#[tokio::test(flavor = "current_thread")]
async fn formal_purpose_capacity_reserves_before_shared_business_and_protects_owner() {
    // One reserved slot and one shared slot exercise the actual parent boundary.
    // 一个预留槽及一个公共槽用于验证实际父级边界。
    let mut settings = VulcanPluginRuntimeSettings::default();
    settings.parent.max_resident_vms = 2;
    settings.parent.max_running_calls = 2;
    // The Hook reservation must exist before the unrelated session can claim any VM.
    // Hook 预留必须在无关会话声明任何 VM 前存在。
    let config = dedicated(&settings, "hook-plugin", 1);
    settings.capacity_budgets.insert(
        "hook-plugin".into(),
        BTreeMap::from([(VulcanPluginCapacityPurpose::Hook, config.clone())]),
    );
    // Construct the real published runtime, which installs configured native guarantees first.
    // 构造真实已发布运行时；它先安装配置原生保证。
    let fixture = Fixture::new(settings);
    // Permit two logical sessions so native capacity, rather than the fixture's legacy one-session limit, decides admission.
    // 允许两个逻辑会话，由原生容量而非夹具原有单会话限制决定入场。
    let manifest_path = fixture.manager.snapshot().plugins["idle-plugin"]
        .package_root
        .join("plugin.yaml");
    // Edit this test-owned manifest through its declared YAML structure before admitting any session.
    // 在任何会话入场前，通过声明的 YAML 结构修改本测试拥有的清单。
    let mut manifest: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&manifest_path).expect("fixture manifest"))
            .expect("parse fixture manifest");
    manifest["lua_runtime"]["max_active_sessions"] = serde_yaml::Value::from(2);
    std::fs::write(
        &manifest_path,
        serde_yaml::to_string(&manifest).expect("serialize session limit"),
    )
    .expect("write session limit");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish two-session fixture");
    // Configuration reserves capacity without fabricating prewarmed Lua state.
    // 配置预留容量，不虚构已经预热的 Lua 状态。
    let initial = fixture
        .manager
        .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect("capacity status")
        .expect("startup registration");
    assert_eq!(initial.snapshot.config, config);
    assert_eq!(initial.snapshot.committed_resident_vms, 1);
    assert_eq!(initial.snapshot.resources.resident, 0);
    assert_eq!(
        initial.budget_source,
        VulcanPluginCapacityBudgetSource::HostOverride
    );
    // Consume exactly the unreserved slot through actual production session initialization.
    // 通过实际生产会话初始化恰好消费无预留槽。
    let opened = fixture.tool("idle_open", json!({})).await;
    assert_eq!(fixture.status("idle-plugin").snapshot.resources.resident, 1);
    // A second session cannot steal the still-idle guarantee from another plugin.
    // 第二个会话不能窃取另一插件仍空闲的保证。
    let refused = fixture
        .manager
        .invoke(VulcanPluginInvocationRequest {
            tool_id: "idle_open".into(),
            arguments: json!({}),
            owner_session_id: "aggregate-owner".into(),
            workspace_root: fixture.root.path().to_owned(),
        })
        .await
        .expect_err("reserved parent slot");
    assert!(
        refused
            .to_string()
            .contains("parent resident capacity is occupied or reserved"),
        "{refused}"
    );
    assert_eq!(fixture.status("idle-plugin").snapshot.resources.resident, 1);
    assert!(
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("consume original guarantee")
    );
    // Physical creation consumes the same original reservation, not a second capacity identity.
    // 物理创建消费同一原始预留，而非第二个容量身份。
    let actual = fixture
        .manager
        .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect("actual capacity")
        .expect("registered");
    assert_eq!(actual.snapshot.capacity_id, initial.snapshot.capacity_id);
    assert_eq!(actual.snapshot.resources.resident, 1);
    assert_eq!(actual.snapshot.retained_pools, 1);
    fixture
        .tool("idle_close", json!({"session_id":opened["session_id"]}))
        .await;
    fixture.close().await;
}

/// Every production category honors its chosen capacity without changing single-call or fixed-session semantics.
/// 每种生产类别均遵守所选容量，不改变单次调用或固定会话语义。
#[tokio::test(flavor = "current_thread")]
async fn formal_purpose_capacity_routes_all_categories_through_frozen_owners() {
    // Explicit zero-minimum dedicated policies prove routing independently of reservation size.
    // 显式零最小值专用策略独立于预留大小证明路由。
    let mut settings = VulcanPluginRuntimeSettings::default();
    // These pairs cover all host-selected purposes using their real fixture declarations.
    // 这些配对通过真实夹具声明覆盖全部宿主选择用途。
    let purposes = [
        ("idle-plugin", VulcanPluginCapacityPurpose::ToolCall),
        ("idle-plugin", VulcanPluginCapacityPurpose::ToolSession),
        ("hook-plugin", VulcanPluginCapacityPurpose::Hook),
        ("hook-plugin", VulcanPluginCapacityPurpose::UiAction),
        ("hook-plugin", VulcanPluginCapacityPurpose::Lifecycle),
        ("hook-plugin", VulcanPluginCapacityPurpose::ActivationProbe),
    ];
    for (plugin, purpose) in purposes {
        // Complete per-purpose overrides retain the upstream configuration shape.
        // 每用途完整覆盖保留上游配置形状。
        let config = dedicated(&settings, plugin, 0);
        settings
            .capacity_budgets
            .entry(plugin.into())
            .or_default()
            .insert(purpose, config);
    }
    // Public status must refer to the same identities before and after actual module execution.
    // 公开状态必须在实际模块执行前后引用相同身份。
    let fixture = Fixture::new(settings.clone());
    // Save the actual startup identities, never derive native IDs from purpose names.
    // 保存实际启动身份，绝不从用途名称派生原生标识。
    let original: Vec<_> = purposes
        .iter()
        .map(|(plugin, purpose)| {
            fixture
                .manager
                .formal_capacity_status(plugin, *purpose)
                .expect("startup status")
                .expect("configured capacity")
                .snapshot
                .capacity_id
        })
        .collect();
    fixture.tool("idle_ping", json!({})).await;
    // Pinned tool sessions and ordinary tools must select distinct capacity identities.
    // 固定工具会话及普通工具必须选择不同容量身份。
    let opened = fixture.tool("idle_open", json!({})).await;
    assert_eq!(
        fixture
            .manager
            .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolSession)
            .expect("session capacity")
            .expect("registered")
            .snapshot
            .resources
            .resident,
        1
    );
    fixture
        .tool("idle_close", json!({"session_id":opened["session_id"]}))
        .await;
    assert!(
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("Hook initialization")
    );
    assert_eq!(
        fixture
            .manager
            .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
            .expect("Hook capacity")
            .expect("registered")
            .snapshot
            .resources
            .resident,
        1
    );
    for reuse in [false, true] {
        // Both UI routes share capacity while starting with their own Lua state.
        // 两条 UI 路由共享容量，同时以各自 Lua 状态开始。
        let result = fixture
            .manager
            .invoke_action(fixture.action(reuse))
            .await
            .expect("Action");
        assert_eq!(result["count"], 100);
    }
    assert_eq!(
        fixture
            .manager
            .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::UiAction)
            .expect("UI capacity")
            .expect("registered")
            .snapshot
            .resources
            .resident,
        1
    );
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("draining");
    // The existing drain owns actual member cleanup; capacity metadata remains independently queryable.
    // 既有排空拥有实际成员清理；容量元数据仍可独立查询。
    let report = fixture
        .manager
        .drain_plugin_sessions("hook-plugin")
        .await
        .expect("drain members");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    // This exact declared error proves the lifecycle body ran through its own capacity.
    // 此精确声明错误证明生命周期主体经自身容量执行。
    let failure = fixture
        .manager
        .invoke_lifecycle("hook-plugin", VulcanPluginLifecycleEvent::Disable)
        .await
        .expect_err("fixture marker");
    assert!(
        failure.to_string().contains("enable-rollback-marker"),
        "{failure}"
    );
    fixture
        .manager
        .rollback_draining_to_enabling("hook-plugin")
        .expect("probe scope");
    fixture
        .manager
        .probe_plugin("hook-plugin")
        .await
        .expect("real activation probe");
    for ((plugin, purpose), identity) in purposes.iter().zip(original) {
        // Each complete live native policy equals the one frozen at startup.
        // 每份完整实时原生策略均等于启动时冻结的策略。
        let actual = fixture
            .manager
            .formal_capacity_status(plugin, *purpose)
            .expect("live status")
            .expect("retained");
        assert_eq!(actual.snapshot.capacity_id, identity);
        assert_eq!(
            actual.snapshot.config,
            settings.capacity_budget(plugin, *purpose).0
        );
        assert_eq!(actual.preparing_members, 0);
        assert_eq!(actual.snapshot.config.resources.kind, PoolKind::Dedicated);
    }
    assert!(
        fixture
            .manager
            .request_formal_capacity_close("hook-plugin", VulcanPluginCapacityPurpose::Lifecycle)
            .expect("close lifecycle purpose")
    );
    // Closing this purpose fences its real callback without borrowing another configured group.
    // 关闭此用途阻止其真实回调，不借用另一配置分组。
    let blocked_lifecycle = fixture
        .manager
        .invoke_lifecycle("hook-plugin", VulcanPluginLifecycleEvent::EnableRollback)
        .await
        .expect_err("closed lifecycle group");
    assert!(
        blocked_lifecycle.to_string().contains("permanently closed"),
        "{blocked_lifecycle}"
    );
    assert!(
        fixture
            .manager
            .request_formal_capacity_close(
                "hook-plugin",
                VulcanPluginCapacityPurpose::ActivationProbe
            )
            .expect("close probe purpose")
    );
    // Probe initialization is also subject to the exact purpose's permanent admission fence.
    // 探测初始化同样受精确用途永久入场屏障约束。
    let blocked_probe = fixture
        .manager
        .probe_plugin("hook-plugin")
        .await
        .expect_err("closed probe group");
    assert!(
        blocked_probe.to_string().contains("permanently closed"),
        "{blocked_probe}"
    );
    fixture.close().await;
}

/// A real host preparation claim survives close and cannot be forgotten or silently rebound.
/// 真实宿主准备声明在关闭后存活，不能被遗忘或静默重新绑定。
#[tokio::test(flavor = "current_thread")]
async fn formal_purpose_capacity_preparation_blocks_forgetting_until_explicit_release() {
    // The unconfigured shared policy is allocated only by exact production preparation admission.
    // 未配置公共策略仅由精确生产准备入场分配。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    // Retain the same managed owner used by all production preparation tasks.
    // 保留全部生产准备任务使用的同一受管所有者。
    let owner = fixture
        .manager
        .formal_runtime_owner()
        .expect("owner")
        .expect("attached");
    // Publish a real preparation claim while obeying the production publication-lock order.
    // 遵守生产发布锁顺序，发布真实准备声明。
    let claim = {
        // Release only the short catalog gate; the returned guard owns continued preparation.
        // 仅释放短目录门；返回守卫拥有持续准备。
        let publication = fixture
            .manager
            .state
            .session_registration_gate
            .lock()
            .expect("publication");
        owner
            .admit_capacity_member(
                "idle-plugin",
                VulcanPluginCapacityPurpose::ToolCall,
                &publication,
            )
            .expect("admit preparing member")
    };
    // Save exact original identity before any explicit close request.
    // 在任何显式关闭请求前保存精确原身份。
    let identity = claim.capacity_id.clone();
    assert!(
        fixture
            .manager
            .request_formal_capacity_close("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
            .expect("close")
    );
    assert!(
        fixture
            .manager
            .forget_formal_capacity("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
            .expect_err("preparation still owns capacity")
            .to_string()
            .contains("unfinished member preparation")
    );
    // Snapshot must report real retained preparation even though the native owner has no member yet.
    // 即使原生所有者尚无成员，快照也必须报告真实保留准备。
    let closed = fixture
        .manager
        .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
        .expect("closed snapshot")
        .expect("retained");
    assert_eq!(closed.snapshot.capacity_id, identity);
    assert!(closed.snapshot.closing);
    assert_eq!(closed.preparing_members, 1);
    drop(claim);
    // Explicit closure keeps future production work fenced until actual metadata release is requested.
    // 显式关闭持续阻止后续生产工作，直到请求实际元数据释放。
    let failure = fixture
        .manager
        .invoke(VulcanPluginInvocationRequest {
            tool_id: "idle_ping".into(),
            arguments: json!({}),
            owner_session_id: "aggregate-owner".into(),
            workspace_root: fixture.root.path().to_owned(),
        })
        .await
        .expect_err("closed original identity");
    assert!(
        failure.to_string().contains("permanently closed"),
        "{failure}"
    );
    assert!(
        fixture
            .manager
            .forget_formal_capacity("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
            .expect("explicit removal")
    );
    assert!(
        fixture
            .manager
            .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
            .expect("forgotten snapshot")
            .is_none()
    );
    fixture.tool("idle_ping", json!({})).await;
    // Only explicit successful forgetting permits a newly signed native identity on later admission.
    // 仅显式成功遗忘允许后续入场取得新签发的原生身份。
    let replacement = fixture
        .manager
        .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
        .expect("new snapshot")
        .expect("new registration");
    assert_ne!(replacement.snapshot.capacity_id, identity);
    assert_eq!(replacement.preparing_members, 0);
    assert_eq!(
        replacement.budget_source,
        VulcanPluginCapacityBudgetSource::PluginBudget
    );
    fixture.close().await;
}

/// A real persistent member keeps its capacity and guarantee until its physical cleanup acknowledges pool removal.
/// 真实持久成员在物理清理确认池移除前，持续保留容量及保证。
#[tokio::test(flavor = "current_thread")]
async fn formal_purpose_capacity_requires_actual_member_retirement_before_forgetting() {
    // Reserve one dedicated Hook slot before any module is initialized.
    // 在任何模块初始化前预留一个专用 Hook 槽。
    let mut settings = VulcanPluginRuntimeSettings::default();
    // Derive the complete upstream policy from this exact parent and plugin aggregate.
    // 从此精确父级及插件聚合派生完整上游策略。
    let config = dedicated(&settings, "hook-plugin", 1);
    settings.capacity_budgets.insert(
        "hook-plugin".into(),
        BTreeMap::from([(VulcanPluginCapacityPurpose::Hook, config)]),
    );
    // Execute actual module initialization, including the real core session and member pool.
    // 执行实际模块初始化，包括真实核心会话及成员池。
    let fixture = Fixture::new(settings);
    assert!(
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("prewarm")
    );
    // Native identity is observed, never reconstructed from a plugin or purpose name.
    // 观测原生身份，绝不从插件或用途名称重新构造。
    let original = fixture
        .manager
        .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect("status")
        .expect("retained capacity");
    assert_eq!(original.snapshot.retained_pools, 1);
    assert_eq!(original.snapshot.resources.resident, 1);
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("authorize real finalization");
    assert!(
        fixture
            .manager
            .request_formal_capacity_close("hook-plugin", VulcanPluginCapacityPurpose::Hook)
            .expect("request capacity closure")
    );
    // No await occurs before this removal attempt, so the host monitor has not acknowledged member metadata release.
    // 此移除尝试前没有异步等待，因此宿主监控器尚未确认成员元数据释放。
    let refused = fixture
        .manager
        .forget_formal_capacity("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect_err("member still retained");
    assert!(
        refused
            .to_string()
            .contains("capacity ownership has not drained"),
        "{refused}"
    );
    // A rejected removal must leave the same owner and non-borrowable reservation visible.
    // 被拒绝的移除必须保持同一所有者及不可借用预留可见。
    let closing = fixture
        .manager
        .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
        .expect("closing status")
        .expect("original retained");
    assert_eq!(closing.snapshot.capacity_id, original.snapshot.capacity_id);
    assert_eq!(closing.snapshot.committed_resident_vms, 1);
    assert_eq!(closing.snapshot.retained_pools, 1);
    // The existing production drain owns session finalization and explicit pool forgetting.
    // 既有生产排空拥有会话关闭及显式池遗忘。
    let report = fixture
        .manager
        .drain_plugin_sessions("hook-plugin")
        .await
        .expect("physical drain");
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(
        fixture
            .manager
            .forget_formal_capacity("hook-plugin", VulcanPluginCapacityPurpose::Hook)
            .expect("release drained native guarantee")
    );
    assert!(
        fixture
            .manager
            .formal_capacity_status("hook-plugin", VulcanPluginCapacityPurpose::Hook)
            .expect("removed status")
            .is_none()
    );
    fixture.close().await;
}

/// Catalog generation changes never register another reservation for the same retained purpose.
/// 目录代次变化绝不为同一保留用途注册另一份预留。
#[tokio::test(flavor = "current_thread")]
async fn formal_purpose_capacity_identity_survives_actual_catalog_generation_change() {
    // Use the production stateless tool so each invocation has a new immutable module member.
    // 使用生产无状态工具，使每次调用都有新的不可变模块成员。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.tool("idle_ping", json!({})).await;
    // Capture only the real authority returned by live status.
    // 仅捕获实时状态返回的真实权威。
    let original = fixture
        .manager
        .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
        .expect("status")
        .expect("registered");
    // Resolve the bundle path through the actual retained catalog and manifest declaration.
    // 通过实际保留目录及清单声明解析 bundle 路径。
    let plugin = fixture.manager.snapshot().plugins["idle-plugin"].clone();
    // The bundle declaration is authoritative; no candidate filenames are probed.
    // bundle 声明为权威；不探测候选文件名。
    let lua =
        crate::runtime::lease::required_lua_runtime(plugin.manifest.as_ref().expect("manifest"))
            .expect("Lua declaration");
    // This fixture owns its physical package and may publish a deliberate source change.
    // 此夹具拥有物理包，可发布主动源码变化。
    let bundle = plugin.package_root.join(&lua.bundle);
    // A harmless source comment changes the real content generation without changing the tool protocol.
    // 无害源码注释改变真实内容代次，不改变工具协议。
    let source = std::fs::read_to_string(&bundle).expect("actual source");
    std::fs::write(
        &bundle,
        format!("{source}\n-- Next catalog generation.\n-- 下一目录代次。\n"),
    )
    .expect("update fixture source");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish changed generation");
    assert_ne!(
        fixture.manager.snapshot().plugins["idle-plugin"].generation,
        plugin.generation
    );
    fixture.tool("idle_ping", json!({})).await;
    // New module preparation reuses the retained semantic owner's exact capacity identity.
    // 新模块准备复用保留语义所有者的精确容量身份。
    let current = fixture
        .manager
        .formal_capacity_status("idle-plugin", VulcanPluginCapacityPurpose::ToolCall)
        .expect("current status")
        .expect("registered");
    assert_eq!(current.snapshot.capacity_id, original.snapshot.capacity_id);
    assert_eq!(current.preparing_members, 0);
    fixture.close().await;
}
