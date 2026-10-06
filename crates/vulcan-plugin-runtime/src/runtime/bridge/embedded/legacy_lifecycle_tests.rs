//! Actual managed-removal lifecycle pools retain the original guard through native calls and retirement.
//! 实际受管移除生命周期池在原生调用及退役全程保留原守卫。

use super::*;
use crate::runtime::tests::{test_manager, write_hook_package};
use crate::{
    VulcanPluginActivation, VulcanPluginDeactivationReservation, VulcanPluginGenerationStore,
    VulcanPluginLegacyMutationGuard,
};
use luaskills::LuaInvocationContext;
use luaskills::runtime::embedded::{
    EmbeddedCall, EmbeddedCapacityConfig, EmbeddedPluginConfig, ExecutionBackend, InstanceReuse,
    OperationPhase, PoolKind, VmCapacityConfig,
};
use std::fs;

/// One finite budget governs this fixture's initialization, native calls and physical cleanup.
/// 此夹具初始化、原生调用及物理清理共用一个有限预算。
const WAIT: Duration = Duration::from_secs(5);

/// Actual manager, package, core pool and original removal reservation for one disabled managed package.
/// 单个已停用受管插件包的实际管理器、插件包、核心池及原移除预留。
struct LegacyFixture {
    /// The actual core is closed before its physical package directory disappears.
    /// 实际核心在物理插件包目录消失前关闭。
    host: VulcanPluginEmbeddedHost,
    /// The original manager owns the sole frozen descriptor and publication gate.
    /// 原管理器拥有唯一冻结描述符及发布门。
    manager: VulcanPluginRuntimeManager,
    /// The existing store observes exclusivity without manufacturing another guard.
    /// 既有存储观测排他性，不制造另一个守卫。
    store: VulcanPluginGenerationStore,
    /// One pool and capacity retain the exact original authority independently of caller handles.
    /// 单个池及容量独立于调用方句柄保留精确原权威。
    pool: String,
    /// Weak observation proves the guard outlives the pool only through the retained reservation.
    /// 弱观测证明守卫仅经由保留预留在池之后存活。
    guard: Weak<VulcanPluginLegacyMutationGuard>,
    /// The removal reservation owns the namespace File until the test releases it.
    /// 移除预留拥有命名空间文件，直至测试释放它。
    original: Option<VulcanPluginDeactivationReservation>,
    /// The unchanged selection proves the namespace is free once the reservation is released.
    /// 预留释放后，未变选择用于证明命名空间空闲。
    selected: VulcanPluginActivation,
    /// Real temporary files outlive all actual core cleanup.
    /// 真实临时文件存活至全部实际核心清理之后。
    _root: tempfile::TempDir,
}

impl LegacyFixture {
    /// Build a real disabled managed bundle and admit its Uninstall callback through the original removal reservation.
    /// 构建真实已停用受管 bundle，并通过原移除预留接纳其卸载回调。
    /// Return the actual pool fixture after caller-owned authority handles have been released.
    /// 调用方所有权句柄释放后返回实际池夹具。
    fn new() -> Self {
        // Each fixture uses independent real roots and the production manager constructor.
        // 每个夹具使用独立真实根及生产管理器构造入口。
        let root = tempfile::tempdir().expect("managed lifecycle native fixture");
        // The bundled root is deliberately empty; this package is selected from the actual user root.
        // 内建根有意保持空；此插件包从实际用户根选择。
        let manager = test_manager(root.path(), root.path().join("bundled"));
        // Source bytes remain outside user discovery until the production installer records exact authorization.
        // 生产安装器记录精确授权之前，来源字节保持在用户发现范围之外。
        let source = root.path().join("source");
        write_hook_package(&source);
        // All modified files exist before catalog publication and original namespace acquisition.
        // 所有修改文件在目录发布及原命名空间获取前存在。
        let package = source.join("hook-plugin");
        // Keep schema/defaults and declared lifecycle exports in the same actual package manifest.
        // 模式、默认值及声明生命周期导出保留在同一实际包清单内。
        let manifest = fs::read_to_string(package.join("plugin.yaml"))
            .expect("actual legacy manifest")
            .replace("default_enabled: true", "default_enabled: false")
            .replace(
                "    disable: on_disable",
                "    install: dispatch\n    uninstall: dispatch\n    disable: on_disable",
            );
        fs::write(
            package.join("plugin.yaml"),
            format!("{manifest}permissions: [config]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json\n"),
        )
        .expect("actual legacy config declaration");
        fs::write(
            package.join("config.schema.json"),
            r#"{"type":"object","required":["owner"],"properties":{"owner":{"type":"string"}},"additionalProperties":false}"#,
        )
        .expect("actual config schema");
        fs::write(
            package.join("config.defaults.json"),
            r#"{"owner":"initial"}"#,
        )
        .expect("actual defaults");
        fs::write(
            package.join("runtime/plugin.bundle.lua"),
            r#"
-- The actual declared exports all reach the same native configuration effect.
-- 实际声明导出均到达同一原生配置副作用。
local M = {}
-- Replace the complete document using the caller's bounded native operation; return its committed value.
-- 使用调用方有界原生操作替换完整文档；返回其已提交值。
function M.dispatch(arguments)
    -- Core admission supplies identity; this business value cannot select another configuration owner.
    -- 核心入场提供身份；此业务值不能选择另一个配置所有者。
    local document = vulcan.host.call('vulcan.plugin.config.update', {value=arguments.value})
    return vulcan.json.encode({ok=true,result=document})
end
-- Required manifest exports share this fixture's exact function without selecting a business finalizer.
-- 必需清单导出共享此夹具精确函数，不选择业务关闭器。
M.shutdown = M.dispatch
M.on_precheck = M.dispatch
M.ui_action = M.dispatch
M.shutdown_hooks = M.dispatch
M.on_disable = M.dispatch
return M
"#,
        )
        .expect("actual native lifecycle bundle");
        // Select the complete final fingerprint and permission set through the production store writers.
        // 通过生产存储写入器选择完整最终指纹及权限集合。
        let selected = crate::install_user_package_for_test(manager.user_plugin_root(), &package);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("publish disabled managed descriptor");
        // Keep the actual descriptor rather than reconstructing fields from a guessed package path.
        // 保留实际描述符，不从猜测包路径重建字段。
        let plugin = manager.snapshot().plugins["hook-plugin"].clone();
        assert_eq!(
            plugin.runtime_state,
            VulcanPluginRuntimeState::Disabled,
            "actual installed descriptor diagnostics: {:?}",
            plugin.diagnostics
        );
        assert_eq!(
            plugin.origin,
            crate::VulcanPluginPackageOrigin::UserInstalled
        );
        // The original removal reservation acquires the physical namespace exactly once.
        // 原移除预留仅获取一次物理命名空间。
        let store = VulcanPluginGenerationStore::open(manager.user_plugin_root())
            .expect("actual managed store");
        let original = store
            .reserve_deactivation(uuid::Uuid::new_v4(), &selected)
            .expect("actual removal reservation");
        // Bind the actual manager and original full selection, using the production authority constructor.
        // 使用生产权威构造入口绑定实际管理器及原完整选择。
        let authority =
            LegacyLifecycleExecution::for_deactivation(&manager.state, plugin.clone(), &original)
                .expect("original lifecycle authority");
        // Weak observation never becomes an independent owner registry.
        // 弱观测绝不成为独立所有者注册表。
        let weak_guard =
            Arc::downgrade(&original.lifecycle_guard().expect("actual namespace guard"));
        authority
            .retain_execution_environment(&manager.state)
            .expect("original prepared environment lease");
        // A same-name foreign manager must never adopt the original physical namespace owner.
        // 同名外来管理器绝不能接管原物理命名空间所有者。
        let foreign_root = tempfile::tempdir().expect("foreign actual manager root");
        // This real independent manager uses its own roots, not a reconstructed state clone.
        // 此真实独立管理器使用自身根目录，不重建状态克隆。
        let foreign = test_manager(foreign_root.path(), foreign_root.path().join("bundled"));
        assert!(
            admitted_plugin(
                &foreign.state,
                &plugin.id,
                &PoolAdmission {
                    scope: authority.scope(),
                    source: ExecutionSource::LegacyLifecycle(Arc::clone(&authority)),
                },
            )
            .is_err(),
            "foreign manager cannot admit an original legacy owner"
        );
        assert!(
            admitted_plugin(
                &manager.state,
                &plugin.id,
                &PoolAdmission {
                    scope: VulcanPluginBridgeLeaseScope::Enabled,
                    source: ExecutionSource::LegacyLifecycle(Arc::clone(&authority)),
                },
            )
            .is_err(),
            "legacy owner cannot authorize an enabled callback"
        );
        // The parent budget is defined once; every child maximum derives from it.
        // 父级预算仅定义一次；每个子级上限从它派生。
        let config = EmbeddedRuntimeConfig {
            max_registered_plugins: 1,
            max_registered_pools: 2,
            max_sessions: 2,
            max_registered_capabilities: CAPABILITIES.len(),
            max_resident_vms: 1,
            max_running_calls: 1,
            max_queued_calls: 2,
            max_queued_bytes: 16384,
            max_operations: 8,
            max_effect_records_per_operation: 8,
            max_effect_bytes_per_operation: 16384,
            max_host_requests: 1,
            max_host_request_bytes: 16384,
            max_value_bytes: 4096,
        };
        // One aggregate owns all actual resources, without a separate legacy engine.
        // 单个聚合拥有全部实际资源，不建立独立旧格式引擎。
        let aggregate = EmbeddedPluginConfig {
            max_registered_pools: config.max_registered_pools,
            max_sessions: config.max_sessions,
            max_resident_vms: config.max_resident_vms,
            max_running_calls: config.max_running_calls,
            max_queued_calls: config.max_queued_calls,
            max_queued_bytes: config.max_queued_bytes,
            max_operations: config.max_operations,
        };
        // The native time limit shares the actual call and cleanup budget.
        // 原生时间上限共用实际调用及清理预算。
        let host = VulcanPluginEmbeddedHost::new(&manager, config, WAIT.as_millis() as u64)
            .expect("actual native host");
        host.runtime()
            .register_plugin(plugin.id.clone(), aggregate.clone())
            .expect("actual plugin aggregate");
        // The capacity is explicitly chosen before registering the legacy pool.
        // 旧格式池注册前显式选择容量。
        let capacity = host
            .runtime()
            .register_capacity(
                &plugin.id,
                EmbeddedCapacityConfig {
                    resources: VmCapacityConfig {
                        kind: PoolKind::Shared,
                        min_resident_vms: 0,
                        max_resident_vms: aggregate.max_resident_vms,
                        max_running_calls: aggregate.max_running_calls,
                    },
                    max_queued_calls: aggregate.max_queued_calls,
                    max_queued_bytes: aggregate.max_queued_bytes,
                },
            )
            .expect("actual lifecycle capacity");
        // The real call-specific entry captures the same original authority Arc into its actual core owner.
        // 真实调用专用入口将同一原权威 Arc 捕获至实际核心所有者。
        let pool = host
            .register_call_bundle_pool_in_capacity(
                &capacity,
                VulcanPluginBundlePoolRequest {
                    plugin_id: plugin.id.clone(),
                    plugin_generation: plugin.generation,
                    workspace_root: root.path().to_path_buf(),
                    security_partition: "original-legacy-lifecycle".to_string(),
                    scope: authority.scope(),
                    policy: PluginPoolConfig {
                        kind: PoolKind::Shared,
                        min_resident_vms: 0,
                        max_resident_vms: aggregate.max_resident_vms,
                        max_running_calls: aggregate.max_running_calls,
                        max_queued_calls: aggregate.max_queued_calls,
                        reuse: InstanceReuse::SingleCall,
                        serial: true,
                        backend: ExecutionBackend::InProcess,
                        idle_ttl_ms: None,
                        max_uses: None,
                    },
                    finalizer: None,
                },
                CallExecutionSource::LegacyLifecycle(Arc::clone(&authority)),
            )
            .expect("actual original lifecycle pool");
        {
            // Inspect only the actual pool-owned domain; source identity must be the original Arc.
            // 仅检查实际池所有域；来源身份必须为原 Arc。
            let domains = host
                .authority
                .domains
                .lock()
                .expect("actual native domains");
            // Stable pool identity avoids relying on evolving registry positions.
            // 稳定池身份避免依赖演进注册表位置。
            let domain = domains
                .values()
                .filter_map(Weak::upgrade)
                .find(|domain| domain.pool_id.get() == Some(&pool))
                .expect("actual pool domain");
            assert!(matches!(
                &*domain.source.lock().expect("actual captured source"),
                ExecutionSource::LegacyLifecycle(retained) if Arc::ptr_eq(retained, &authority)
            ));
            assert!(
                domain._environment.is_some(),
                "actual domain must own the original environment file"
            );
        }
        assert!(
            authority.take_environment().is_err(),
            "environment ownership must transfer exactly once"
        );
        drop(authority);
        Self {
            host,
            manager,
            store,
            pool,
            guard: weak_guard,
            original: Some(original),
            selected,
            _root: root,
        }
    }

    /// Submit document through the real bundle and return terminal core evidence after bounded waiting.
    /// 通过真实 bundle 提交 document，并在有界等待后返回终态核心证据。
    fn call(&self, document: Value) -> luaskills::runtime::embedded::OperationSnapshot {
        // This operation uses ordinary core capability authorization and its original finite budget.
        // 此操作使用普通核心能力授权及其原有限预算。
        let operation = self
            .host
            .runtime()
            .submit(
                EmbeddedCall {
                    pool_id: self.pool.clone(),
                    export: "dispatch".to_string(),
                    arguments: json!({"value": document}),
                    context: LuaInvocationContext::default(),
                },
                WAIT,
            )
            .expect("actual lifecycle Lua submission");
        // Preserve the original terminal receipt rather than synthesizing a configuration acknowledgement.
        // 保留原终态回执，不合成配置确认。
        let result = operation
            .wait(WAIT)
            .expect("actual lifecycle Lua completion");
        self.host
            .runtime()
            .forget_operation(operation.id())
            .expect("forget observed operation");
        result
    }

    /// Close and forget the actual pool, observing physical retirement before checking namespace release.
    /// 关闭并遗忘实际池，在检查命名空间释放前观测物理退役。
    fn close(&self) {
        // The scheduler's exact pool admission must close before forgetting its retained identity.
        // 遗忘保留身份前，必须关闭调度器中精确池的入场。
        self.host
            .runtime()
            .close_pool(&self.pool)
            .expect("close exact original lifecycle pool");
        self.host
            .runtime()
            .request_close()
            .expect("actual close request");
        // The shared finite deadline constrains real physical cleanup observation.
        // 共用有限截止时间约束真实物理清理观测。
        let deadline = Instant::now() + WAIT;
        while !self
            .host
            .runtime()
            .poll_closed()
            .expect("actual close observation")
        {
            assert!(
                Instant::now() < deadline,
                "actual legacy pool cleanup timed out"
            );
            std::thread::yield_now();
        }
        self.host
            .runtime()
            .forget_pool(&self.pool)
            .expect("forget physically retired pool");
    }
}

/// A managed Uninstall callback keeps the original guard yet cannot edit the settings it is retiring.
/// 受管卸载回调保留原守卫，但不能编辑其正在退役的设置。
#[test]
fn managed_uninstall_callback_rejects_configuration_update_and_retains_original_guard() {
    // No caller-owned authority survives this constructor; the actual pool must retain it.
    // 此构造函数后无调用方所有权权威存续；实际池必须保留它。
    let mut fixture = LegacyFixture::new();
    assert!(
        fixture.guard.upgrade().is_some(),
        "actual domain lost original guard"
    );
    assert!(
        fixture
            .store
            .acquire_legacy_mutation_guard("hook-plugin", None)
            .is_err(),
        "actual pool must retain namespace exclusivity"
    );
    let rejected = fixture.call(json!({"owner":"committed"}));
    assert_eq!(
        rejected.phase,
        OperationPhase::Failed,
        "removal callback must not commit configuration: {rejected:?}"
    );
    assert_eq!(
        fixture
            .manager
            .plugin_config("hook-plugin")
            .expect("package default document"),
        json!({"owner":"initial"})
    );
    fixture.close();
    drop(fixture.original.take());
    assert!(
        fixture.guard.upgrade().is_none(),
        "physically retired domain and released reservation still own guard"
    );
    fixture
        .store
        .acquire_legacy_mutation_guard("hook-plugin", Some(&fixture.selected))
        .expect("namespace released only after physical retirement");
}
