//! Failed drain observation must not restore an enabled projection over the durable disabled target.
//! 排空观察失败不能在持久停用目标之上恢复启用投影。

use super::*;
use crate::skills_services::local_plugin_update::tests::{Fixture, PLUGIN, write_package};
use crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH;
use vulcan_plugin_runtime::VulcanPluginRuntimeState;

/// Hold one real reused Action while counting actual Disable callback effects.
/// 持有单个真实复用 Action，同时统计实际停用回调效果。
#[derive(Default)]
struct PendingCapacityEvents {
    /// Native Action entry proves the original member is physically executing.
    /// 原生 Action 入场证明原成员正在物理执行。
    entered: std::sync::atomic::AtomicBool,
    /// Count only actual declared Disable callback effects.
    /// 仅统计实际声明的停用回调效果。
    callbacks: std::sync::atomic::AtomicUsize,
    /// The sole release condition belongs to this native publisher.
    /// 唯一释放条件属于此原生发布器。
    released: std::sync::Mutex<bool>,
    /// Wake the same native worker without invoking another business action.
    /// 唤醒相同原生工作者，不调用另一个业务 Action。
    changed: std::sync::Condvar,
}

impl PendingCapacityEvents {
    /// Release the original worker; no parameters or return value.
    /// 释放原工作者；无参数或返回值。
    fn release(&self) {
        *self
            .released
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = true;
        self.changed.notify_all();
    }
}

/// Release the actual publisher even when the old-source negative assertion fails.
/// 即使旧源码负对照断言失败也释放实际发布器。
struct PendingCapacityRelease(Arc<PendingCapacityEvents>);

impl Drop for PendingCapacityRelease {
    /// Release this guard's original publisher at scope exit; return no value.
    /// 作用域退出时释放此守卫原发布器；不返回值。
    fn drop(&mut self) {
        self.0.release();
    }
}

impl vulcan_plugin_runtime::VulcanPluginUiEventPublisher for PendingCapacityEvents {
    /// Block only event's real Action; record Disable effects and return actual delivery status.
    /// 仅阻塞 event 的真实 Action；记录停用效果并返回实际投递状态。
    fn publish(
        &self,
        event: vulcan_plugin_runtime::VulcanPluginUiInvalidation,
    ) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        match event.topic.as_str() {
            "business" => {
                self.entered.store(true, Ordering::Release);
                // Hold the actual host callback until its test owner releases the original condition.
                // 持有实际宿主回调，直到测试所有者释放原条件。
                let mut released = self.released.lock().map_err(|error| error.to_string())?;
                while !*released {
                    released = self
                        .changed
                        .wait(released)
                        .map_err(|error| error.to_string())?;
                }
            }
            "disable" => {
                self.callbacks.fetch_add(1, Ordering::AcqRel);
            }
            _ => return Err(String::from("Unexpected capacity retry native event.")),
        }
        Ok(())
    }
}

/// A real service Disable retry must not turn capacity Pending into a sticky callback Failure.
/// 真实服务停用重试不能将容量 Pending 转为持续回调 Failure。
/// No arguments or return value; the callback remains unexecuted until actual original member retirement.
/// 无参数或返回值；实际原成员退役之前回调保持未执行。
#[tokio::test]
async fn immutable_service_disable_capacity_pending_reconciles_after_original_retirement() {
    use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    use std::sync::atomic::Ordering;
    // Reuse the original real service bridge and its isolated managed predecessor.
    // 复用原真实服务桥及其隔离受管前驱。
    let fixture = Fixture::new(false);
    // Resolve the one root from the manager's authoritative user namespace.
    // 从管理器权威用户命名空间解析唯一根。
    let root = fixture
        .runtime
        .user_plugin_root()
        .parent()
        .expect("fixture root");
    // Attach the same bounded compatibility engine as existing actual native service fixtures.
    // 附加与既有实际原生服务夹具相同的有界兼容引擎。
    let engine = LuaEngine::new(LuaEngineOptions::new(
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 60,
        },
        LuaRuntimeHostOptions::with_runtime_root(vulcan_runtime_assets::native_host_path(
            &root.canonicalize().expect("canonical root"),
        )),
    ))
    .expect("actual engine");
    fixture
        .runtime
        .attach_runtime_engine(Arc::new(engine))
        .expect("same manager engine");
    std::fs::create_dir_all(root.join("bridge/runtime")).expect("bridge root");
    std::fs::write(root.join("bridge/dependencies.yaml"), "{}\n").expect("bridge dependencies");
    std::fs::write(
        root.join("bridge/runtime/bridge.lua"),
        include_str!("../../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
    )
    .expect("same bridge");
    std::fs::create_dir_all(root.join("distributions")).expect("distribution root");
    // A native successor of the installed predecessor is selected before its first native execution.
    // 已安装前驱的原生后继在首次原生执行之前被选定。
    let package = write_package(&root.join("native-source"), "1.0.0", false, None);
    std::fs::create_dir_all(package.join("runtime")).expect("actual runtime");
    std::fs::write(package.join("dependencies.yaml"), "{}\n").expect("dependencies");
    // Preserve the exact initial manifest and append strict native declarations.
    // 保留精确初始清单，追加严格原生声明。
    let mut manifest = std::fs::read_to_string(package.join("plugin.yaml")).expect("manifest");
    manifest.push_str("permissions: [ui_host_context]\nexecution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\nlua_runtime:\n  bundle: runtime/plugin.bundle.lua\n  invoke_function: dispatch\n  shutdown_function: shutdown\n  max_active_sessions: 1\n  lifecycle:\n    disable: on_disable\n  actions:\n    - id: hold\n      effect: read\n      function: hold\n      timeout_ms: 1000\n      callers: [plugin_ui]\n");
    std::fs::write(package.join("plugin.yaml"), manifest).expect("actual declaration");
    std::fs::write(
        package.join("runtime/plugin.bundle.lua"),
        r#"
-- Module exports belong to this actual retained instance.
-- 模块导出属于此实际保留实例。
local M = {}
-- No arguments are consumed; return the normal version-one success frame.
-- 不消费参数；返回正常版本一成功帧。
function M.dispatch() return '{"ok":true,"result":{}}' end
-- Shutdown completes normally, without a blocked finalizer or intentional failure.
-- 关闭正常完成，没有阻塞关闭器或刻意失败。
M.shutdown = M.dispatch
-- args is the original Action envelope; block only the real business host callback.
-- args 是原 Action 信封；仅阻塞真实业务宿主回调。
function M.hold(args)
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='business', revision=1})
    return M.dispatch()
end
-- args identifies the declared lifecycle event; count it only if Lua actually runs.
-- args 标识声明生命周期事件；仅在 Lua 实际运行时计数。
function M.on_disable(args)
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic=args.event, revision=1})
    error('actual cleanup disable failure after native effect')
end
return M
"#,
    )
    .expect("actual module");
    // Selection approves the full new declaration and binds its actual fingerprint.
    // 选择批准完整新声明并绑定其实际指纹。
    vulcan_plugin_runtime::install_user_package_for_test(
        fixture.runtime.user_plugin_root(),
        &package,
    );
    fixture
        .runtime
        .refresh_catalog(&BTreeSet::from([PLUGIN.to_owned()]), &BTreeSet::new())
        .expect("publish authorized predecessor");
    fixture
        .runtime
        .prepare_plugin_environment(PLUGIN)
        .expect("prepare actual declared environment");
    // The publisher owns physical execution and all observed callback counts.
    // 发布器拥有物理执行及全部观测回调计数。
    let events = Arc::new(PendingCapacityEvents::default());
    // Release precedes every possible assertion after native admission.
    // 释放守卫先于原生入场后的每个可能断言。
    let _release = PendingCapacityRelease(events.clone());
    fixture
        .runtime
        .install_host_services(vulcan_plugin_runtime::VulcanPluginHostServices {
            storage: None,
            vldb: None,
            ui_events: Some(events.clone()),
        })
        .expect("actual publisher");
    fixture
        .runtime
        .initialize_formal_runtime(vulcan_plugin_runtime::VulcanPluginRuntimeSettings {
            shutdown_timeout_ms: 20,
            ..Default::default()
        })
        .expect("actual parent");
    // Persist policy only inside this test-owned destination.
    // 仅在此测试自有目标中持久化策略。
    let destination = tempfile::tempdir().expect("private policy");
    // Retain finite diagnostics, the recorded Failure and its final projection until real parent cleanup.
    // 将有限诊断、已登记 Failure 及其最终投影保留至真实父级清理之后。
    let (diagnostics, failure, actual_failure_diagnostics) = PROVIDER_CONFIGURATION_PATH
        .scope(destination.path().join("policy.json"), async {
            set_vulcan_plugin_enabled(Arc::clone(&fixture.state), PLUGIN.to_owned(), true)
                .await
                .expect("enable actual package");
            // Freeze the exact enabled routing generation before the real Action starts.
            // 在真实 Action 启动前固定精确启用路由代次。
            let catalog = fixture.runtime.snapshot();
            // This caller owns its original UI invocation independently of the Disable retry.
            // 此调用方独立于停用重试持有原 UI 调用。
            let identity = vulcan_plugin_runtime::VulcanPluginActionIdentity {
                client_id: uuid::Uuid::new_v4().to_string(),
                invocation_id: uuid::Uuid::new_v4().to_string(),
            };
            // The same actual temporary workspace supplies action authority.
            // 相同实际临时工作区提供 Action 权威。
            let workspace = root.join("request");
            std::fs::create_dir_all(&workspace).expect("workspace");
            // Routing generation comes from the same public manager snapshot.
            // 路由代次来自相同公开管理器快照。
            let generation = format!(
                "{}:{}",
                catalog.service_epoch, catalog.plugins[PLUGIN].generation
            );
            // Clone only the original manager for the actual asynchronous observer.
            // 仅为实际异步观察者克隆原管理器。
            let manager = fixture.runtime.clone();
            // The original reused UI member remains physically retained inside the native callback.
            // 原复用 UI 成员在原生回调中保持物理保留。
            let action = tokio::spawn(async move {
                manager
                    .invoke_ui_action(
                        vulcan_plugin_runtime::VulcanPluginActionRequest {
                            plugin_id: PLUGIN.to_owned(),
                            action_id: "hold".into(),
                            arguments: json!({}),
                            caller: vulcan_plugin_runtime::VulcanPluginActionCaller::PluginUi,
                            workspace_root: workspace,
                            reuse_enabled_runtime: true,
                        },
                        &generation,
                        identity,
                    )
                    .await
            });
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while !events.entered.load(Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("actual Action entry");
            fixture
                .runtime
                .begin_draining(PLUGIN)
                .expect("original disable intent");
            fixture
                .runtime
                .finish_lifecycle_transition(
                    PLUGIN,
                    &BTreeSet::from([PLUGIN.to_owned()]),
                    &BTreeSet::new(),
                    Vec::new(),
                )
                .expect("retain capacity retirement intent");
            // Real service retry observes Pending and cannot execute its callback through the retained capacity.
            // 真实服务重试观测 Pending，不能经保留容量执行回调。
            let retry = transition(&fixture, false, true);
            prepare_disable_transition(&fixture.state, &fixture.runtime, &retry)
                .await
                .expect("prepare original service cleanup retry");
            commit_disable_transition(&fixture.state, &fixture.runtime, &retry)
                .await
                .expect("publish finite disabled warning");
            assert_eq!(events.callbacks.load(Ordering::Acquire), 0);
            assert!(
                fixture
                    .runtime
                    .has_cleanup_diagnostics(PLUGIN)
                    .expect("actual retained warning")
            );
            assert!(
                fixture
                    .runtime
                    .observe_catalog(&fixture.runtime.snapshot())
                    .expect("actual preflight observation")
                    .plugins[PLUGIN]
                    .diagnostics
                    .iter()
                    .any(|message| message.contains("retained member pools")),
                "the real retry must reach the capacity owner preflight"
            );
            events.release();
            let _ = action.await.expect("original observer ended");
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    if fixture
                        .runtime
                        .drain_plugin_sessions(PLUGIN)
                        .await
                        .expect("normal physical drain")
                        .failures
                        .is_empty()
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("actual member retired");
            fixture
                .runtime
                .finish_lifecycle_transition(
                    PLUGIN,
                    &BTreeSet::from([PLUGIN.to_owned()]),
                    &BTreeSet::new(),
                    Vec::new(),
                )
                .expect("normal final publication");
            assert_eq!(events.callbacks.load(Ordering::Acquire), 0);
            // Freeze the Pending-only observation before any actual Disable callback can execute.
            // 在任何实际停用回调执行之前固定仅 Pending 的观测。
            let finite_diagnostics = fixture
                .runtime
                .observe_catalog(&fixture.runtime.snapshot())
                .expect("actual final observation")
                .plugins[PLUGIN]
                .diagnostics
                .clone();
            // Disabled is not a legal Draining source; use the original complete public enabling sequence first.
            // Disabled 不是合法 Draining 来源；先使用原完整公开启用序列。
            fixture
                .runtime
                .begin_enabling(PLUGIN)
                .expect("normal public reactivation");
            fixture
                .runtime
                .invoke_lifecycle(
                    PLUGIN,
                    vulcan_plugin_runtime::VulcanPluginLifecycleEvent::Enable,
                )
                .await
                .expect("no declared Enable callback");
            fixture
                .runtime
                .probe_plugin(PLUGIN)
                .await
                .expect("actual activation probe");
            fixture
                .runtime
                .finish_lifecycle_transition(
                    PLUGIN,
                    &BTreeSet::new(),
                    &BTreeSet::from([PLUGIN.to_owned()]),
                    Vec::new(),
                )
                .expect("publish normal enabled source");
            fixture
                .runtime
                .begin_draining(PLUGIN)
                .expect("fresh permitted Disable attempt");
            // This is the first actual declared Disable execution, after the original member physically retired.
            // 这是首次实际声明停用执行，发生于原成员物理退役之后。
            let recorded_failure = fixture
                .runtime
                .invoke_lifecycle_for_cleanup(
                    PLUGIN,
                    vulcan_plugin_runtime::VulcanPluginLifecycleEvent::Disable,
                    "插件停用生命周期回调失败",
                )
                .await
                .expect("actual callback failure recorded")
                .expect("real callback failed");
            assert!(
                recorded_failure.contains("actual cleanup disable failure after native effect"),
                "{recorded_failure}"
            );
            assert_eq!(events.callbacks.load(Ordering::Acquire), 1);
            fixture
                .runtime
                .drain_plugin_sessions(PLUGIN)
                .await
                .expect("normal drain after real failure");
            fixture
                .runtime
                .finish_lifecycle_transition(
                    PLUGIN,
                    &BTreeSet::from([PLUGIN.to_owned()]),
                    &BTreeSet::new(),
                    Vec::new(),
                )
                .expect("normal finish retains real Failure");
            // Observe the exact recorded message from the sole live diagnostic authority.
            // 从唯一实时诊断权威观测精确已登记消息。
            let failure_diagnostics = fixture
                .runtime
                .observe_catalog(&fixture.runtime.snapshot())
                .expect("actual sticky callback failure")
                .plugins[PLUGIN]
                .diagnostics
                .clone();
            (finite_diagnostics, recorded_failure, failure_diagnostics)
        })
        .await;
    fixture
        .runtime
        .request_formal_runtime_close()
        .expect("close actual parent");
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !fixture
            .runtime
            .poll_formal_runtime_closed()
            .expect("parent closed")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual parent retired");
    assert!(
        diagnostics.is_empty(),
        "finite callback preflight must not become Failure: {diagnostics:?}"
    );
    assert!(
        actual_failure_diagnostics.contains(&failure),
        "ordinary cleanup must preserve actual callback Failure: {actual_failure_diagnostics:?}"
    );
    assert_eq!(events.callbacks.load(Ordering::Acquire), 1);
}

/// Freezes exact policy and descriptors before exercising production rollback paths.
/// 在执行生产回滚路径之前固定精确策略及描述符。
/// The fixture supplies the current package; target and previous select the policy direction.
/// fixture 提供当前插件包；target 及 previous 选择策略方向。
/// Returns the same immutable transition facts consumed by service lifecycle operations.
/// 返回服务生命周期操作消费的同一不可变转换事实。
fn transition(fixture: &Fixture, target: bool, previous: bool) -> VulcanPluginTransition {
    // Keep the current catalog alive while reading its unique package entry.
    // 读取唯一插件包条目时保持当前目录存活。
    let catalog = fixture.runtime.snapshot();
    // Resolve by stable package identity, never a position in the catalog.
    // 按稳定插件包身份解析，绝不使用目录位置。
    let plugin = catalog
        .plugins
        .get(PLUGIN)
        .expect("fixture package identity may have changed");
    // Construct exact explicit sets; this fixture's manifest default is disabled.
    // 构造精确显式集合；此夹具清单默认停用。
    let policy = |enabled: bool| VulcanPluginPolicySnapshot {
        disabled_plugin_ids: if enabled {
            BTreeSet::new()
        } else {
            BTreeSet::from([PLUGIN.to_owned()])
        },
        enabled_plugin_ids: if enabled {
            BTreeSet::from([PLUGIN.to_owned()])
        } else {
            BTreeSet::new()
        },
    };
    VulcanPluginTransition {
        plugin_id: PLUGIN.to_owned(),
        target: VulcanPluginTargetState::from_enabled(target),
        default_enabled: false,
        previous_enabled: previous,
        plugin_generation: plugin.generation,
        target_mcp_servers: plugin.mcp_servers.clone(),
        previous_mcp_servers: plugin.mcp_servers.clone(),
        previous_policy: policy(previous),
        target_policy: policy(target),
    }
}

/// Recovers a finalized disable by publishing the recovered catalog and reopening admission.
/// 通过发布恢复目录及重新开放入场，恢复已完成停用。
/// This content-only service fixture checks real coordination; native resources are covered in runtime tests.
/// 此纯内容服务夹具检查真实协调；原生资源由运行时测试覆盖。
#[tokio::test]
async fn immutable_service_update_disable_rollback_republishes_recovered_generation() {
    // All runtime paths and policy writes remain inside isolated temporary directories.
    // 全部运行时路径及策略写入保持在独立临时目录内。
    let fixture = Fixture::new(false);
    // The existing task-local destination keeps writes away from user settings.
    // 既有任务局部目标使写入远离用户设置。
    let policy_root = tempfile::tempdir().expect("policy workspace");
    PROVIDER_CONFIGURATION_PATH
        .scope(policy_root.path().join("policy.json"), async {
            set_vulcan_plugin_enabled(Arc::clone(&fixture.state), PLUGIN.to_owned(), true)
                .await
                .expect("initial enable");
            // Freeze rollback authority before the real disable transaction.
            // 在真实停用事务之前固定回滚权威。
            let transition = transition(&fixture, false, true);
            set_vulcan_plugin_enabled(Arc::clone(&fixture.state), PLUGIN.to_owned(), false)
                .await
                .expect("committed disable");
            // Capture the actual disabled pointer so publication can be distinguished from a manager-only refresh.
            // 捕获实际停用指针，区分发布与仅刷新管理器。
            let disabled = fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("bridge")
                .extension_generation
                .clone();
            rollback_disable(
                &fixture.state,
                &fixture.runtime,
                &transition,
                CleanupEvidence::default(),
                true,
            )
            .await
            .expect("reverse enable");
            // Inspect the same authoritative pointer consumed by new executions.
            // 检查新执行消费的同一权威指针。
            let recovered = fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("bridge")
                .extension_generation
                .clone();
            assert!(recovered.revision() > disabled.revision());
            assert!(
                recovered
                    .plugin_catalog()
                    .expect("recovered catalog")
                    .plugins[PLUGIN]
                    .desired_enabled
            );
            assert!(
                !disabled
                    .plugin_catalog()
                    .expect("captured disabled catalog")
                    .plugins[PLUGIN]
                    .desired_enabled
            );
            // Reopening occurs only after the recovered generation is published.
            // 仅在恢复代次发布之后重新开放入场。
            recovered
                .mcp_runtime()
                .open_owned_stdio_partition(
                    PLUGIN,
                    transition.plugin_generation,
                    SessionId::new_v4(),
                    Vec::new(),
                )
                .await
                .expect("recovered owner admission")
                .shutdown()
                .expect("close test owner");
            set_vulcan_plugin_enabled(Arc::clone(&fixture.state), PLUGIN.to_owned(), false)
                .await
                .expect("final cleanup");
        })
        .await;
}

/// A failed finalized enable publishes the disabled extension, not only a disabled manager snapshot.
/// 已完成启用的回滚必须发布停用扩展，而不能仅更新管理器快照。
#[tokio::test]
async fn immutable_service_update_enable_rollback_republishes_disabled_generation() {
    // The content-only fixture exercises production lifecycle without a fabricated Lua engine.
    // 纯内容夹具执行生产生命周期，不伪造 Lua 引擎。
    let fixture = Fixture::new(false);
    // Every explicit policy mutation uses this test-owned destination.
    // 每次显式策略变更使用此测试自有目标。
    let policy_root = tempfile::tempdir().expect("policy workspace");
    PROVIDER_CONFIGURATION_PATH
        .scope(policy_root.path().join("policy.json"), async {
            // Preserve the disabled predecessor before activating the plugin.
            // 激活插件前保留停用前驱。
            let transition = transition(&fixture, true, false);
            set_vulcan_plugin_enabled(Arc::clone(&fixture.state), PLUGIN.to_owned(), true)
                .await
                .expect("finalized enable");
            // Captured readers must keep their original snapshot while the current publication changes.
            // 当前发布改变时，已捕获读者必须保留原快照。
            let enabled = fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("bridge")
                .extension_generation
                .clone();
            rollback_finalized_enable_to_policy(
                &fixture.state,
                &fixture.runtime,
                &transition,
                false,
                &transition.previous_policy,
            )
            .await
            .expect("compensated enable");
            // Read the actual extension pointer after compensation.
            // 补偿后读取实际扩展指针。
            let disabled = fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_ref()
                .expect("bridge")
                .extension_generation
                .clone();
            assert!(disabled.revision() > enabled.revision());
            assert!(
                !disabled.plugin_catalog().expect("disabled catalog").plugins[PLUGIN]
                    .desired_enabled
            );
            assert!(
                enabled
                    .plugin_catalog()
                    .expect("captured enabled catalog")
                    .plugins[PLUGIN]
                    .desired_enabled
            );
            disabled
                .mcp_runtime()
                .wait_plugin_mcp_drain(
                    PLUGIN,
                    AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
                )
                .await
                .expect("original resources remain drained");
            assert!(
                disabled
                    .mcp_runtime()
                    .open_owned_stdio_partition(
                        PLUGIN,
                        transition.plugin_generation,
                        SessionId::new_v4(),
                        Vec::new(),
                    )
                    .await
                    .is_err()
            );
        })
        .await;
}

/// Injects an admission-observation failure and verifies the actual rollback publication stays disabled.
/// 注入入场观察失败，验证实际回滚发布保持停用。
/// No processes or writes are needed: the production wait rejects the deliberately open gate.
/// 无需进程或写入：生产等待拒绝故意开放的闸门。
#[tokio::test]
async fn immutable_service_update_owner_drain_failure_cannot_restore_enabled_projection() {
    // Reuse the real immutable-update fixture instead of introducing another service-state model.
    // 复用真实不可变更新夹具，不引入另一份服务状态模型。
    let fixture = Fixture::new(false);
    // These immutable facts describe an already persisted disable with a previously enabled predecessor.
    // 这些不可变事实描述已经持久化的停用，以及此前启用的前驱。
    let transition = VulcanPluginTransition {
        plugin_id: PLUGIN.to_owned(),
        target: VulcanPluginTargetState::Disabled,
        default_enabled: false,
        previous_enabled: true,
        plugin_generation: fixture.runtime.snapshot().plugins[PLUGIN].generation,
        target_mcp_servers: Vec::new(),
        previous_mcp_servers: Vec::new(),
        previous_policy: VulcanPluginPolicySnapshot {
            disabled_plugin_ids: BTreeSet::new(),
            enabled_plugin_ids: BTreeSet::from([PLUGIN.to_owned()]),
        },
        target_policy: VulcanPluginPolicySnapshot {
            disabled_plugin_ids: BTreeSet::from([PLUGIN.to_owned()]),
            enabled_plugin_ids: BTreeSet::new(),
        },
    };
    // The failure arises from the real owner admission contract, before any reverse enable callback.
    // 失败来自实际所有者入场契约，发生在任何反向启用回调之前。
    let error = rollback_disable(
        &fixture.state,
        &fixture.runtime,
        &transition,
        CleanupEvidence::additional(vec!["original cleanup failed".into()]),
        false,
    )
    .await
    .expect_err("open gate must reject drain observation");
    assert!(error.contains("not closed"), "{error}");
    // Retain the immutable routing catalog independently from cleanup observations.
    // 保留不可变路由目录，与清理观察结果分开核对。
    let catalog = fixture.runtime.snapshot();
    // Select by stable identity rather than catalog ordering.
    // 按稳定身份选择，不依赖目录顺序。
    let plugin = catalog
        .plugins
        .get(PLUGIN)
        .expect("original fixture plugin");
    assert!(!plugin.desired_enabled);
    assert_eq!(plugin.runtime_state, VulcanPluginRuntimeState::Disabled);
    assert!(
        fixture
            .runtime
            .has_cleanup_diagnostics(PLUGIN)
            .expect("actual cleanup diagnostics")
    );
    // Project the actual diagnostics for UI observation without replacing the routing catalog.
    // 从实际诊断生成界面观察投影，不替换路由目录。
    let observed = fixture
        .runtime
        .observe_catalog(&catalog)
        .expect("actual cleanup observation");
    // Select the observed plugin by the same stable identity.
    // 按同一稳定身份选择观察投影中的插件。
    let observed_plugin = observed
        .plugins
        .get(PLUGIN)
        .expect("observed fixture plugin");
    assert!(!observed_plugin.desired_enabled);
    assert_eq!(
        observed_plugin.runtime_state,
        VulcanPluginRuntimeState::DisabledWithCleanupWarning
    );
}
