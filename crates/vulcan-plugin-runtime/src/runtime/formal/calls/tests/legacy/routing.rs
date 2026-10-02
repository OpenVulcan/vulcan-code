//! Real flat uninstall coordinates native configuration and preserves original callback sources.
//! 真实平铺卸载协调原生配置，并保留原回调来源。

use super::*;
use crate::runtime::routing::ActivationRoutingOriginal;

/// Install an authorized real flat package whose Lua callback writes mutable configuration.
/// 安装已授权真实平铺包，其 Lua 回调写入可变配置。
/// No parameters; returns the existing actual manager/parent fixture after production installation.
/// 无参数；生产安装后返回既有实际管理器／父级夹具。
fn configured_fixture() -> Fixture {
    // Actual bundled bytes and formal runtime are created by the existing production fixture.
    // 实际内建字节及正式运行时由既有生产夹具创建。
    let fixture = Fixture::new();
    // Move only this test's real source outside catalog discovery before authorization.
    // 授权之前仅将此测试真实来源移出目录发现。
    let source = fixture._root.path().join("source/hook-plugin");
    std::fs::create_dir_all(source.parent().expect("actual source parent")).expect("source parent");
    std::fs::rename(fixture._root.path().join("bundled/hook-plugin"), &source)
        .expect("real source move");
    // Complete source declaration is modified before inspection and installation.
    // 完整来源声明在检查及安装之前修改。
    let manifest_path = source.join("plugin.yaml");
    // Schema and permission use the real manifest's existing declaration spelling.
    // 模式及权限使用真实清单既有声明拼写。
    let manifest = std::fs::read_to_string(&manifest_path).expect("actual manifest").replace(
        "permissions: [process_binary, ui_host_context]",
        "permissions: [process_binary, ui_host_context, config]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json",
    );
    std::fs::write(manifest_path, manifest).expect("real config declaration");
    std::fs::write(source.join("config.schema.json"), r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#).expect("real schema");
    std::fs::write(source.join("config.defaults.json"), r#"{"label":"before"}"#)
        .expect("real defaults");
    // Real native configuration effects occur before the synchronized UI callback worker.
    // 真实原生配置效果发生在同步界面回调工作者之前。
    let bundle_path = source.join("runtime/plugin.bundle.lua");
    // Exact existing callback receives the actual config capability, not a simulated writer.
    // 精确既有回调接收实际配置能力，而非模拟写入器。
    let bundle = std::fs::read_to_string(&bundle_path).expect("actual Lua bundle").replace(
        "function M.on_lifecycle(args)",
        "function M.on_lifecycle(args)\n    -- Commit the actual flat lifecycle configuration under its original guard.\n    -- 在原守卫内提交实际平铺生命周期配置。\n    local updated = vulcan.host.call('vulcan.plugin.config.update', {value={label='callback'}})\n    assert(updated.label == 'callback')",
    );
    std::fs::write(bundle_path, bundle).expect("real config callback");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove bundled discovery");
    // Production inspection supplies exact bytes and every required permission.
    // 生产检查提供精确字节及每个必需权限。
    let preview = fixture
        .manager
        .inspect_local_user_plugin(&source)
        .expect("real inspection");
    fixture
        .manager
        .install_local_user_plugin(
            &source,
            &preview.content_fingerprint,
            preview.permissions,
            false,
        )
        .expect("authorized production installation");
    fixture
        .manager
        .refresh_catalog(
            &BTreeSet::from(["hook-plugin".to_owned()]),
            &BTreeSet::new(),
        )
        .expect("actual disabled source");
    fixture
        .manager
        .install_host_services(VulcanPluginHostServices {
            storage: None,
            vldb: None,
            ui_events: Some(Arc::new(LegacyEvents(Arc::clone(&fixture.events)))),
        })
        .expect("actual synchronized native service");
    fixture
}

/// Ordinary refresh cannot destroy a real flat callback source; legal native config keeps the original manifest Arc.
/// 普通刷新不能损坏真实平铺回调来源；合法原生配置保留原清单 Arc。
/// No parameters; checks actual namespace counts, callback effects, pre-intent retry and final same-original projection.
/// 无参数；检查实际命名空间计数、回调效果、意图前重试及最终同原投影。
#[test]
fn legacy_uninstallation_routing_original_keeps_callback_config_and_guarded_projection() {
    // Actual source contains the real config writer and synchronized native callback.
    // 实际来源包含真实配置写入器及同步原生回调。
    let fixture = configured_fixture();
    // Actual retained uninstall owns the only namespace and local preimages before callback effects.
    // 回调效果之前，实际保留卸载拥有唯一命名空间及本地前像。
    let mut original = fixture
        .manager
        .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "hook-plugin")
        .expect("real uninstall original");
    // Coordination remains weak with respect to the actual namespace File.
    // 对实际命名空间文件，协调保持为弱引用。
    let routing = fixture
        .manager
        .reserve_legacy_uninstallation_routing(&original)
        .expect("same original routing");
    assert_eq!(Arc::strong_count(original.mutation_guard()), 1);
    // Freeze actual published routing before any native callback starts.
    // 任何原生回调启动之前固定实际已发布路由。
    let catalog = fixture.manager.snapshot();
    assert!(
        fixture
            .manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .is_err()
    );
    assert!(
        fixture
            .manager
            .update_plugin_config("hook-plugin", json!({"label":"ordinary"}))
            .is_err()
    );
    assert_eq!(
        fixture
            .manager
            .plugin_config("hook-plugin")
            .expect("unchanged real config"),
        json!({"label":"before"})
    );
    assert!(Arc::ptr_eq(&catalog, &fixture.manager.snapshot()));
    // One actual callback borrows the retained original namespace.
    // 单个实际回调借用保留原命名空间。
    let mut callback = fixture
        .manager
        .prepare_legacy_lifecycle_call(
            "hook-plugin",
            VulcanPluginLegacyLifecycleEvent::Uninstall,
            Arc::clone(original.mutation_guard()),
        )
        .expect("same original callback");
    // Existing executor observes real formal/native work.
    // 既有执行器观测真实正式／原生工作。
    let runtime = executor();
    runtime.block_on(async {
        callback.start().expect("actual original start");
        callback.start().expect("repeat start does not replay");
        fixture.entered().await;
    });
    assert_eq!(
        fixture
            .manager
            .plugin_config("hook-plugin")
            .expect("actual native write"),
        json!({"label":"callback"})
    );
    // Native config invalidates routing revision while preserving the actual original manifest identity.
    // 原生配置使路由修订失效，同时保留实际原清单身份。
    let configured = fixture.manager.snapshot();
    assert!(Arc::ptr_eq(
        catalog.plugins["hook-plugin"]
            .manifest
            .as_ref()
            .expect("original manifest"),
        configured.plugins["hook-plugin"]
            .manifest
            .as_ref()
            .expect("current manifest")
    ));
    assert!(
        fixture
            .manager
            .refresh_catalog_with_legacy_mutation(
                &BTreeSet::from(["hook-plugin".to_owned()]),
                &BTreeSet::new(),
                "hook-plugin",
                original.mutation_guard()
            )
            .is_err()
    );
    assert!(Arc::ptr_eq(&configured, &fixture.manager.snapshot()));
    assert!(
        fixture
            .manager
            .continue_legacy_user_plugin_uninstallation(&mut original)
            .is_err()
    );
    // Retained actual record provides physical-retirement evidence independently of business observation.
    // 保留实际记录独立于业务观测提供物理退役证据。
    let admitted = runtime.block_on(fixture.admitted());
    fixture.events.release();
    runtime
        .block_on(callback.observe())
        .expect("actual native callback result");
    runtime.block_on(retired(&admitted));
    callback
        .confirm_retired()
        .expect("physical acknowledgement")
        .expect("callback business success");
    drop(admitted);
    assert_eq!(Arc::strong_count(original.mutation_guard()), 1);
    assert_eq!(fixture.events.values.lock().expect("real effects").len(), 1);
    fixture
        .manager
        .continue_legacy_user_plugin_uninstallation(&mut original)
        .expect("continue initial hide after actual pre-intent borrower failure");
    fixture
        .manager
        .continue_legacy_user_plugin_uninstallation(&mut original)
        .expect("same intent read-only continuation");
    // Same actual guard projects removal only after callback retirement and actual package hide.
    // 仅在回调退役及实际插件包隐藏之后，相同实际守卫投影移除。
    let removed = fixture
        .manager
        .refresh_catalog_with_legacy_mutation(
            &BTreeSet::from(["hook-plugin".to_owned()]),
            &BTreeSet::new(),
            "hook-plugin",
            original.mutation_guard(),
        )
        .expect("same original hidden projection");
    assert!(!removed.plugins.contains_key("hook-plugin"));
    drop(routing);
    drop(original);
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("ordinary refresh after actual release");
}

/// Real foreign namespace and manager cannot impersonate the retained flat uninstall.
/// 真实外来命名空间及管理器不能冒充保留平铺卸载。
/// No parameters; full absent selections still require same actual physical allocation and manager.
/// 无参数；完整缺失选择仍要求相同实际物理分配及管理器。
#[test]
fn legacy_uninstallation_routing_original_rejects_foreign_namespace_and_manager() {
    // Existing real fixtures create separate production managers and namespace Files.
    // 既有真实夹具创建独立生产管理器及命名空间文件。
    let fixture = user_fixture();
    // Actual uninstall request is retained before routing reservation.
    // 路由预留之前保留实际卸载请求。
    let original = fixture
        .manager
        .reserve_legacy_user_plugin_uninstallation(Uuid::new_v4(), "hook-plugin")
        .expect("real original");
    // Weak coordination neither acquires nor prolongs the namespace File.
    // 弱协调既不获取也不延长命名空间文件。
    let routing = fixture
        .manager
        .reserve_legacy_uninstallation_routing(&original)
        .expect("original routing");
    // Real foreign guard has the same plugin spelling and complete absent selected record.
    // 真实外来守卫具有相同插件拼写及完整缺失已选记录。
    let foreign = user_fixture();
    // Independent real namespace acquisition is the contender's physical identity producer.
    // 独立真实命名空间获取为争用者物理身份生产者。
    let foreign_guard = guard(&foreign);
    assert_eq!(
        foreign_guard.routing_original(),
        original.mutation_guard().routing_original()
    );
    {
        // Coordination validation uses the original manager's existing gate.
        // 协调校验使用原管理器既有门。
        let registration = fixture
            .manager
            .state
            .session_registration_gate
            .lock()
            .expect("actual gate");
        assert!(
            fixture
                .manager
                .require_routing_publication_locked(
                    &registration,
                    Some(ActivationRoutingOriginal::LegacyMutation(&foreign_guard))
                )
                .is_err()
        );
    }
    assert!(
        foreign
            .manager
            .reserve_legacy_uninstallation_routing(&original)
            .is_err()
    );
    assert!(
        fixture
            .manager
            .prepare_legacy_lifecycle_call(
                "hook-plugin",
                VulcanPluginLegacyLifecycleEvent::Uninstall,
                foreign_guard
            )
            .is_err()
    );
    assert_eq!(Arc::strong_count(original.mutation_guard()), 1);
    drop(routing);
}
