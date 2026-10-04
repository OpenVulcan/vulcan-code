//! Real manager transitions must distinguish core finalization from ordinary shutdown exports.
//! 真实管理器转换必须区分核心关闭阶段与普通关闭导出调用。

use super::*;
use crate::{
    VulcanPluginHostServices, VulcanPluginRuntimeState, VulcanPluginUiEventPublisher,
    VulcanPluginUiInvalidation,
};
use luaskills::runtime::embedded::OperationSnapshot;
use std::sync::Mutex;

/// Record actual native UI effects independently of the Lua return frame.
/// 独立于 Lua 返回帧记录真实原生界面副作用。
#[derive(Default)]
struct Events {
    /// Only admitted native publications enter this collection.
    /// 仅已接纳的原生发布进入此集合。
    values: Mutex<Vec<VulcanPluginUiInvalidation>>,
}

impl VulcanPluginUiEventPublisher for Events {
    /// Record `event`; return a diagnostic if the observation lock is unavailable.
    /// 记录 `event`；观测锁不可用时返回诊断。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|error| error.to_string())?
            .push(event);
        Ok(())
    }
}

/// Own one real package, formal manager, pinned instance and effect recorder for a transition test.
/// 为转换测试拥有真实包、正式管理器、固定实例及副作用记录器。
struct Fixture {
    /// Manager owns the formal parent and authenticates lifecycle publication.
    /// 管理器拥有正式父级并认证生命周期发布。
    manager: VulcanPluginRuntimeManager,
    /// This observer retains the same parent, never creates another runtime.
    /// 此观测者保留相同父级，绝不创建另一运行时。
    owner: Arc<ManagedFormalRuntime>,
    /// Native effects distinguish denied calls from successful cleanup.
    /// 原生副作用用于区分被拒调用与成功清理。
    events: Arc<Events>,
    /// Exact session and pool identities come from core admission.
    /// 精确会话及池身份来自核心入场。
    session: String,
    /// Pool identity scopes operation evidence discovery.
    /// 池身份限定操作证据发现范围。
    pool: String,
    /// All fixture waits derive from the configured shutdown budget.
    /// 全部夹具等待派生自已配置关闭预算。
    timeout: Duration,
    /// Package files must outlive manager and runtime observations.
    /// 包文件必须比管理器及运行时观测存活更久。
    _root: tempfile::TempDir,
}

impl Fixture {
    /// Initialize a declared UI-capable bundle before any transition; return its exact retained owner.
    /// 在任何转换前初始化声明界面能力的 bundle；返回其精确保留所有者。
    fn new() -> Self {
        // The known fixture creates its manifest and exports before catalog discovery.
        // 已知夹具在目录发现前创建清单及导出。
        let root = tempfile::tempdir().expect("create drain fixture");
        let bundled = root.path().join("bundled");
        write_hook_package(&bundled);
        let package = bundled.join("hook-plugin");
        let mut manifest =
            std::fs::read_to_string(package.join("plugin.yaml")).expect("read manifest");
        manifest.push_str("permissions: [ui_host_context]\n");
        std::fs::write(package.join("plugin.yaml"), manifest).expect("declare UI permission");
        std::fs::write(package.join("runtime/plugin.bundle.lua"), r#"
-- The module and counter belong to this one physical instance.
-- 模块及计数器属于此单个物理实例。
local M, count = {}, 0
-- Accept no business parameters; return a version-one frame with the physical counter.
-- 不接收业务参数；返回包含物理计数器的版本一帧。
function M.dispatch()
    count = count + 1
    return vulcan.json.encode({ok=true,result={count=count}})
end
-- Arguments cannot select the core execution stage; emit one real native effect on admission.
-- 参数不能选择核心执行阶段；入场后发出一次真实原生副作用。
-- No input is needed; return the same-instance counter in the closing frame.
-- 不需要输入；在关闭帧中返回同实例计数器。
function M.shutdown()
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='formal-drain',revision=count,is_finalization=true})
    return vulcan.json.encode({ok=true,result={count=count}})
end
-- Keep every manifest-declared fixture export explicit.
-- 保留清单声明的每个明确夹具导出。
M.on_precheck, M.ui_action, M.on_disable, M.shutdown_hooks = M.dispatch, M.dispatch, M.dispatch, M.shutdown
return M
"#).expect("write real bundle");
        let manager = test_manager(root.path(), bundled);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("publish enabled fixture");
        let events = Arc::new(Events::default());
        manager
            .install_host_services(VulcanPluginHostServices {
                storage: None,
                vldb: None,
                ui_events: Some(events.clone()),
            })
            .expect("install native recorder");
        let settings = VulcanPluginRuntimeSettings::default();
        let timeout = Duration::from_millis(settings.shutdown_timeout_ms);
        manager
            .initialize_formal_runtime(settings)
            .expect("attach actual parent");
        let owner = manager
            .formal_runtime_owner()
            .expect("lookup parent")
            .expect("attached parent");
        let parent = &owner.settings.parent;
        owner
            .host
            .runtime()
            .register_plugin(
                "hook-plugin".to_string(),
                EmbeddedPluginConfig {
                    max_registered_pools: parent.max_registered_pools,
                    max_sessions: parent.max_sessions,
                    max_resident_vms: parent.max_resident_vms,
                    max_running_calls: parent.max_running_calls,
                    max_queued_calls: parent.max_queued_calls,
                    max_queued_bytes: parent.max_queued_bytes,
                    max_operations: parent.max_operations,
                },
            )
            .expect("register plugin aggregate");
        let pool = owner
            .host
            .register_bundle_pool(VulcanPluginBundlePoolRequest {
                plugin_id: "hook-plugin".to_string(),
                plugin_generation: manager.snapshot().plugins["hook-plugin"].generation,
                workspace_root: root.path().to_path_buf(),
                security_partition: "drain-fixture".to_string(),
                scope: VulcanPluginBridgeLeaseScope::Enabled,
                policy: PluginPoolConfig {
                    kind: PoolKind::Shared,
                    min_resident_vms: 0,
                    max_resident_vms: 1,
                    max_running_calls: 1,
                    max_queued_calls: parent.max_queued_calls,
                    reuse: InstanceReuse::Session,
                    serial: true,
                    backend: ExecutionBackend::InProcess,
                    idle_ttl_ms: None,
                    max_uses: None,
                },
                finalizer: Some(ModuleFinalizer {
                    export: "shutdown".to_string(),
                    arguments: json!({}),
                    timeout_ms: owner.settings.shutdown_timeout_ms,
                }),
            })
            .expect("register real retained bundle");
        let opening = owner
            .host
            .runtime()
            .open_session(&pool, timeout)
            .expect("open original instance");
        assert_eq!(
            opening
                .operation
                .wait(timeout)
                .expect("initialize instance")
                .phase,
            OperationPhase::Succeeded
        );
        let fixture = Self {
            manager,
            owner,
            events,
            session: opening.session_id,
            pool,
            timeout,
            _root: root,
        };
        assert_eq!(fixture.call("dispatch").phase, OperationPhase::Succeeded);
        fixture
    }

    /// Invoke ordinary `export` with a forged stage argument; return the actual core snapshot.
    /// 使用伪造阶段参数调用普通 `export`；返回实际核心快照。
    fn call(&self, export: &str) -> OperationSnapshot {
        self.owner
            .host
            .runtime()
            .submit_session(
                &self.session,
                export.to_string(),
                json!({"is_finalization":true}),
                LuaInvocationContext::default(),
                self.timeout,
            )
            .expect("submit ordinary export")
            .wait(self.timeout)
            .expect("wait ordinary export")
    }

    /// Request actual shutdown, then return its unique core finalization snapshot after physical retirement.
    /// 请求实际关闭，然后在物理退役后返回唯一核心关闭快照。
    fn close(&self) -> OperationSnapshot {
        self.manager
            .request_formal_runtime_close()
            .expect("request parent close");
        wait_closed(&self.manager, self.timeout);
        let page = self
            .owner
            .host
            .runtime()
            .list_operations(
                Some(&self.pool),
                None,
                self.owner.settings.parent.max_operations,
            )
            .expect("discover retained operations");
        assert!(!page.has_more, "complete bounded fixture evidence must fit");
        let mut closing = page
            .operation_ids
            .iter()
            .map(|id| {
                self.owner
                    .host
                    .runtime()
                    .operation(id)
                    .expect("operation identity")
                    .snapshot()
                    .expect("operation evidence")
            })
            .filter(|operation| operation.finalization.is_some());
        let result = closing.next().expect("one finalization record");
        assert!(closing.next().is_none(), "closing must not replay");
        result
    }
}

/// A shutdown-named business call and forged flags cannot use the grant, while core cleanup can.
/// 名为关闭的业务调用及伪造标记不能使用授权，核心清理则可以。
#[test]
fn formal_draining_only_authorizes_core_finalizers() {
    let fixture = Fixture::new();
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("enter exact drain");
    let forged = fixture.call("shutdown");
    assert_eq!(forged.phase, OperationPhase::Failed, "{forged:?}");
    let closing = fixture.close();
    assert!(
        closing
            .finalization
            .expect("closing evidence")
            .outcome
            .expect("closing outcome")
            .result()
            .is_ok()
    );
    let events = fixture.events.values.lock().expect("native effects");
    assert_eq!(
        events.as_slice(),
        &[VulcanPluginUiInvalidation {
            plugin_id: "hook-plugin".to_string(),
            topic: "formal-drain".to_string(),
            revision: 1,
        }]
    );
}

/// Rollback, later drains, changed bytes and live revocation must invalidate cleanup access.
/// 回滚、后续排空、内容变化及实时撤权必须使清理访问失效。
#[test]
fn formal_draining_grants_do_not_survive_transition_or_content_changes() {
    for mutation in ["rollback", "later-drain", "content", "revoked"] {
        let fixture = Fixture::new();
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("enter original drain");
        match mutation {
            "rollback" => fixture
                .manager
                .rollback_draining_to_enabling("hook-plugin")
                .expect("publish rollback"),
            "later-drain" => {
                fixture
                    .manager
                    .rollback_draining_to_enabling("hook-plugin")
                    .expect("publish rollback");
                fixture
                    .manager
                    .finish_lifecycle_transition(
                        "hook-plugin",
                        &BTreeSet::new(),
                        &BTreeSet::new(),
                        vec![],
                    )
                    .expect("finish rollback");
                fixture
                    .manager
                    .begin_draining("hook-plugin")
                    .expect("enter later drain");
            }
            "content" => {
                let package = fixture.manager.snapshot().plugins["hook-plugin"]
                    .package_root
                    .clone();
                let manifest = std::fs::read_to_string(package.join("plugin.yaml"))
                    .expect("read actual manifest");
                std::fs::write(
                    package.join("plugin.yaml"),
                    manifest.replace("version: 1.0.0", "version: 1.0.1"),
                )
                .expect("change actual package bytes");
                fixture
                    .manager
                    .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
                    .expect("publish changed content");
            }
            "revoked" => {
                fixture
                    .owner
                    .host
                    .runtime()
                    .revoke_pool_permission(&fixture.pool, "vulcan.plugin.ui.invalidate")
                    .expect("revoke original permission during authorized drain");
            }
            _ => unreachable!("fixed mutation matrix"),
        }
        let closing = fixture.close();
        assert!(
            closing
                .finalization
                .expect("closing evidence")
                .outcome
                .expect("closing outcome")
                .result()
                .is_err(),
            "{mutation}"
        );
        assert!(
            fixture
                .events
                .values
                .lock()
                .expect("native effects")
                .is_empty(),
            "{mutation}"
        );
    }
}

/// Exhausted transition identities must fail without publishing state or granting cleanup access.
/// 耗尽的转换身份必须失败，不发布状态或授予清理访问。
#[test]
fn formal_draining_rejects_exhausted_generation_before_publication() {
    let fixture = Fixture::new();
    let generation = fixture.manager.snapshot().plugins["hook-plugin"].generation;
    fixture
        .manager
        .state
        .plugin_generations
        .lock()
        .expect("generation registry")
        .insert("hook-plugin".to_string(), u64::MAX);
    assert!(
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect_err("reject reused generation")
            .message
            .contains("generation is exhausted")
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].generation,
        generation
    );
    assert_eq!(
        fixture.manager.snapshot().plugins["hook-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    assert!(
        fixture
            .close()
            .finalization
            .expect("normal shutdown")
            .outcome
            .expect("normal outcome")
            .result()
            .is_ok()
    );
}

/// A closing callback delayed at service lookup must recheck transition and permissions before effects.
/// 在服务查询处延迟的关闭回调必须在副作用前重新检查转换及权限。
#[test]
fn formal_draining_rechecks_authority_after_service_lock_wait() {
    for revoked in [false, true] {
        // Only service resolution is blocked; lifecycle publication and core observation remain independent.
        // 仅阻塞服务解析；生命周期发布及核心观测保持独立。
        let fixture = Fixture::new();
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("enter exact drain");
        let service_gate = fixture
            .manager
            .state
            .host_services
            .write()
            .expect("pause native service lookup");
        fixture
            .owner
            .host
            .runtime()
            .close_session(&fixture.session)
            .expect("schedule automatic finalizer");
        let deadline = Instant::now() + fixture.timeout;
        loop {
            let page = fixture
                .owner
                .host
                .runtime()
                .list_operations(
                    Some(&fixture.pool),
                    None,
                    fixture.owner.settings.parent.max_operations,
                )
                .expect("observe closing ledger");
            assert!(!page.has_more, "bounded fixture ledger must fit");
            let running = page.operation_ids.iter().any(|id| {
                fixture
                    .owner
                    .host
                    .runtime()
                    .operation(id)
                    .expect("closing identity")
                    .snapshot()
                    .expect("live closing snapshot")
                    .host_effects
                    .iter()
                    .any(|effect| {
                        effect.phase == luaskills::runtime::embedded::HostEffectPhase::Running
                    })
            });
            if running {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "closing callback never entered native dispatch"
            );
            std::thread::yield_now();
        }
        if revoked {
            fixture
                .owner
                .host
                .runtime()
                .revoke_pool_permission(&fixture.pool, "vulcan.plugin.ui.invalidate")
                .expect("revoke while callback waits");
        } else {
            fixture
                .manager
                .rollback_draining_to_enabling("hook-plugin")
                .expect("rollback while callback waits");
        }
        drop(service_gate);
        let closing = fixture.close();
        assert!(
            closing
                .finalization
                .expect("closing evidence")
                .outcome
                .expect("closing outcome")
                .result()
                .is_err()
        );
        assert!(
            closing
                .host_effects
                .iter()
                .all(|effect| effect.effects
                    == luaskills::runtime::embedded::EffectState::NotStarted)
        );
        assert!(
            fixture
                .events
                .values
                .lock()
                .expect("no native entry")
                .is_empty()
        );
    }
}
