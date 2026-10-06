//! Real service removal exercises actual originals, callback effects, Full preparation and physical readers.
//! 真实服务移除验证实际原实例、回调效果、完整准备及物理读者。

use super::super::local_plugin_update::tests::{Fixture, PLUGIN};
use super::*;
use crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Count only real native events emitted by the declared actual Lua Uninstall callback.
/// 仅统计声明实际 Lua 卸载回调发出的真实原生事件。
#[derive(Default)]
struct CallbackEvents(AtomicUsize);

/// Fail only actual client event delivery after the real projection commit.
/// 仅在真实投影提交之后使实际客户端事件投递失败。
struct FailedDelivery;

impl vulcan_service_api::ServiceEventSink for FailedDelivery {
    /// Actual event reaches this sink; panic models the existing synchronous client callback failure surface.
    /// 实际事件到达此接收器；panic 模拟既有同步客户端回调失败面。
    /// No successful delivery or replacement event is returned.
    /// 不返回成功投递或替代事件。
    fn emit_service_event(&self, _event: vulcan_service_api::ServiceEvent) {
        panic!("intentional actual removal projection client delivery failure");
    }
}

impl vulcan_plugin_runtime::VulcanPluginUiEventPublisher for CallbackEvents {
    /// event must identify Uninstall; return actual native success after retaining its count.
    /// event 必须标识卸载；保留其计数之后返回实际原生成功。
    fn publish(
        &self,
        event: vulcan_plugin_runtime::VulcanPluginUiInvalidation,
    ) -> Result<(), String> {
        if event.topic != "uninstall" {
            return Err(String::from("Unexpected actual removal callback event."));
        }
        self.0.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
}

/// Attach real Lua to this fixture and declare a failed Uninstall before immutable inspection.
/// 为此夹具附加真实 Lua，并在不可变检查之前声明失败卸载。
/// fixture supplies authoritative paths; returns actual native callback evidence.
/// fixture 提供权威路径；返回实际原生回调证据。
fn declare_failed_callback(fixture: &Fixture) -> Arc<CallbackEvents> {
    // Actual event counter belongs to this fixture's native publisher.
    // 实际事件计数器属于此夹具的原生发布器。
    let events = Arc::new(CallbackEvents::default());
    declare_callback_with_publisher(fixture, events.clone());
    events
}

/// Attach real Lua and declare its actual failed callback; fixture owns bytes and events receives native effects.
/// 附加真实 Lua 并声明实际失败回调；fixture 拥有字节，events 接收原生效果。
/// Returns no value; the production inspector later freezes the same declarations.
/// 不返回值；生产检查器随后固定同一声明。
fn declare_callback_with_publisher(
    fixture: &Fixture,
    events: Arc<dyn vulcan_plugin_runtime::VulcanPluginUiEventPublisher>,
) {
    use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    // Derive the existing isolated root from the actual configured user root.
    // 从实际配置用户根派生既有隔离根。
    let root = fixture
        .runtime
        .user_plugin_root()
        .parent()
        .expect("actual fixture root");
    // Match established real native integration prerequisites and bounded engine settings.
    // 匹配既有真实原生集成前提及有界引擎设置。
    let native_root = vulcan_runtime_assets::native_host_path(
        &root.canonicalize().expect("canonical fixture root"),
    );
    // The compatibility pool is independent from the production formal parent below.
    // 兼容池独立于下方生产正式父级。
    let engine = LuaEngine::new(LuaEngineOptions::new(
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 60,
        },
        LuaRuntimeHostOptions::with_runtime_root(native_root),
    ))
    .expect("actual Lua engine");
    fixture
        .runtime
        .attach_runtime_engine(Arc::new(engine))
        .expect("same manager engine");
    std::fs::create_dir_all(root.join("bridge/runtime")).expect("actual bridge root");
    std::fs::write(root.join("bridge/dependencies.yaml"), "{}\n").expect("bridge dependencies");
    std::fs::write(
        root.join("bridge/runtime/bridge.lua"),
        include_str!("../../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
    )
    .expect("authoritative bridge");
    std::fs::create_dir_all(root.join("distributions")).expect("distribution root");
    std::fs::create_dir_all(fixture.source.join("runtime")).expect("real candidate runtime");
    std::fs::write(fixture.source.join("dependencies.yaml"), "{}\n")
        .expect("candidate dependencies");
    // All new declarations are frozen in source before production inspector/materialization.
    // 所有新声明在生产检查／实体化之前于来源中固定。
    let mut manifest =
        std::fs::read_to_string(fixture.source.join("plugin.yaml")).expect("actual manifest");
    manifest.push_str("permissions: [ui_host_context]\nexecution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\nlua_runtime:\n  bundle: runtime/plugin.bundle.lua\n  invoke_function: dispatch\n  shutdown_function: shutdown\n  max_active_sessions: 1\n  lifecycle:\n    uninstall: on_uninstall\n");
    std::fs::write(fixture.source.join("plugin.yaml"), manifest)
        .expect("actual callback declaration");
    std::fs::write(
        fixture.source.join("runtime/plugin.bundle.lua"),
        r#"
-- Actual exports are created once for each physically admitted module instance.
-- 每个物理准入模块实例一次性创建实际导出。
local M = {}
-- dispatch consumes no arguments and returns the declared success envelope.
-- dispatch 不消费参数，返回声明成功信封。
function M.dispatch() return '{"ok":true,"result":{}}' end
-- shutdown returns real empty cleanup success without emitting callback effects.
-- shutdown 返回真实空清理成功，不发出回调效果。
function M.shutdown() return '{"ok":true,"result":{}}' end
-- Uninstall emits the actual native effect before an explicit business failure.
-- 卸载在明确业务失败之前发出实际原生效果。
-- args supplies the declared lifecycle event; failure is intentional and no success is returned.
-- args 提供声明生命周期事件；失败为有意触发，不返回成功。
function M.on_uninstall(args)
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic=args.event, revision=1})
    error('actual failed uninstall after native effect')
end
return M
"#,
    )
    .expect("actual Lua callback bytes");
    // Native evidence belongs to the actual manager and not to a copied call receipt.
    // 原生证据属于实际管理器，而非复制调用回执。
    fixture
        .runtime
        .install_host_services(vulcan_plugin_runtime::VulcanPluginHostServices {
            storage: None,
            vldb: None,
            ui_events: Some(events),
        })
        .expect("actual native callback publisher");
    fixture
        .runtime
        .initialize_formal_runtime(vulcan_plugin_runtime::VulcanPluginRuntimeSettings::default())
        .expect("actual formal parent");
}

/// One real native callback can remain pending until its test owner releases the original publisher.
/// 单个真实原生回调可保持待完成，直到测试所有者释放原发布器。
#[derive(Default)]
struct BlockedCallbackEvents {
    /// Count actual Uninstall effects before blocking, independently from observers.
    /// 在阻塞前统计实际卸载效果，独立于观察者。
    count: AtomicUsize,
    /// Actual publisher entry proves a physically executing native callback.
    /// 实际发布器入场证明物理执行中的原生回调。
    entered: std::sync::atomic::AtomicBool,
    /// Original release flag is shared only with this native publisher.
    /// 原释放标志仅与此原生发布器共享。
    released: std::sync::Mutex<bool>,
    /// Notify the existing host worker without replaying a callback.
    /// 通知既有宿主工作者，不重放回调。
    changed: std::sync::Condvar,
}

impl BlockedCallbackEvents {
    /// Release the actual blocked host worker; no parameters or return value.
    /// 释放实际阻塞宿主工作者；无参数或返回值。
    fn release(&self) {
        *self.released.lock().expect("release original callback") = true;
        self.changed.notify_all();
    }
}

/// Test-scope ownership releases the actual host worker even when an assertion fails.
/// 测试作用域所有权即使断言失败也释放实际宿主工作者。
struct CallbackReleaseGuard(Arc<BlockedCallbackEvents>);

impl Drop for CallbackReleaseGuard {
    /// Release this guard's original publisher on scope exit; return no value.
    /// 作用域退出时释放此守卫原发布器；不返回值。
    fn drop(&mut self) {
        self.0.release();
    }
}

impl vulcan_plugin_runtime::VulcanPluginUiEventPublisher for BlockedCallbackEvents {
    /// Require event's actual Uninstall identity and retain one effect; return success only after original release.
    /// 要求 event 实际卸载身份并保留单个效果；仅在原释放后返回成功。
    fn publish(
        &self,
        event: vulcan_plugin_runtime::VulcanPluginUiInvalidation,
    ) -> Result<(), String> {
        if event.topic != "uninstall" {
            return Err(String::from("Unexpected blocked removal callback event."));
        }
        self.count.fetch_add(1, Ordering::AcqRel);
        self.entered.store(true, Ordering::Release);
        // Released remains protected by the original mutex throughout the real wait.
        // released 在实际等待期间始终由原互斥锁保护。
        let mut released = self.released.lock().map_err(|error| error.to_string())?;
        while !*released {
            released = self
                .changed
                .wait(released)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// A real Pending callback keeps the original UUID through cancellation and explicit known-failure acknowledgement.
/// 真实待完成回调跨取消及显式已知失败确认保留原 UUID。
/// No inputs or return value; actual native effects run once and ordinary drain reconciles only their finite warning.
/// 无输入及返回值；实际原生效果仅执行一次，普通排空仅对账其有限警告。
#[tokio::test]
async fn retained_service_uninstall_pending_callback_resumes_same_uuid_once() {
    // Fixture owns original managed source and the actual service bridge.
    // fixture 拥有原受管来源及实际服务桥。
    let fixture = Fixture::new(false);
    // Events retain the original real callback and its exact effect count.
    // events 保留原真实回调及其精确效果计数。
    let events = Arc::new(BlockedCallbackEvents::default());
    // Release guard precedes every possible assertion after installing the blocking native publisher.
    // release_guard 先于安装阻塞原生发布器后的任何可能断言。
    let _release_guard = CallbackReleaseGuard(events.clone());
    declare_callback_with_publisher(&fixture, events.clone());
    // Policy writes are restricted to this test's actual isolated destination.
    // 策略写入限于此测试实际隔离目标。
    let destination = tempfile::tempdir().expect("private removal policy");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination.path().join("policy.json"), async {
            seed_managed(&fixture).await;
            // State clones the same real service slot; the foreground waiter has no independent supervisor.
            // state 克隆相同真实服务槽；前台等待者没有独立监督者。
            let state = Arc::clone(&fixture.state);
            // Pending retains the ordinary public removal future until only that waiter is cancelled.
            // pending 保留普通公开移除 future，直到仅此等待者被取消。
            let pending = tokio::spawn(async move {
                service_uninstall_user_plugin(
                    ServiceEventEmitter::new(Vec::new()),
                    state,
                    PLUGIN.to_owned(),
                )
                .await
            });
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while !events.entered.load(Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("actual Uninstall publisher entered");
            // Original UUID is read from the actual retained service registry while the callback is still live.
            // 回调仍活动时，从实际保留服务注册表读取原 UUID。
            let row = service_local_plugin_uninstall_requests(Arc::clone(&fixture.state))
                .expect("actual original query")
                .into_iter()
                .find(|row| row.plugin_id == PLUGIN)
                .expect("actual original Uninstall");
            // Actual finite drain observes the same native call and records Pending without a synthetic map write.
            // 实际有限排空观测同一原生调用并登记 Pending，不合成映射写入。
            let report = fixture
                .runtime
                .drain_plugin_sessions(PLUGIN)
                .await
                .expect("bounded live callback drain");
            assert!(!report.failures.is_empty());
            assert!(
                fixture
                    .runtime
                    .has_cleanup_diagnostics(PLUGIN)
                    .expect("actual finite warning")
            );
            pending.abort();
            assert!(
                pending
                    .await
                    .expect_err("only foreground observer cancelled")
                    .is_cancelled()
            );
            events.release();
            // False acknowledgement must first observe the original physically retired business failure.
            // false 确认必须先观测原实际退役业务失败。
            assert!(
                service_resume_local_plugin_uninstall(
                    ServiceEventEmitter::new(Vec::new()),
                    Arc::clone(&fixture.state),
                    PLUGIN.to_owned(),
                    row.request_id.clone(),
                    false
                )
                .await
                .is_err()
            );
            // Retained metadata authorizes only the existing failed callback, never unknown replay.
            // 保留元数据仅授权既有失败回调，绝不授权未知重放。
            let observed = service_local_plugin_uninstall_requests(Arc::clone(&fixture.state))
                .expect("known retired original")
                .into_iter()
                .find(|current| current.request_id == row.request_id)
                .expect("same UUID retained");
            assert!(observed.requires_external_effects_acknowledgement);
            assert_eq!(events.count.load(Ordering::Acquire), 1);
            service_resume_local_plugin_uninstall(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                PLUGIN.to_owned(),
                row.request_id,
                true,
            )
            .await
            .expect("same original acknowledged and collected");
            assert_eq!(events.count.load(Ordering::Acquire), 1);
            assert!(
                !fixture
                    .runtime
                    .has_cleanup_diagnostics(PLUGIN)
                    .expect("only finite warning reconciled")
            );
            assert!(!fixture.runtime.user_plugin_root().join(PLUGIN).exists());
            assert!(
                service_local_plugin_uninstall_requests(Arc::clone(&fixture.state))
                    .expect("original slot released")
                    .is_empty()
            );
        })
        .await;
    fixture
        .runtime
        .request_formal_runtime_close()
        .expect("close actual native parent");
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !fixture
            .runtime
            .poll_formal_runtime_closed()
            .expect("actual parent retirement")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("original native parent closed");
}

/// Actual Lua failure requires acknowledgement only after physical retirement and never replays its native effect.
/// 实际 Lua 失败仅在物理退役之后要求确认，并且绝不重放原生效果。
/// No inputs or return value; true managed selection, callback and same live owner participate.
/// 无输入或返回值；真实受管选择、回调及相同存活所有者参与。
#[tokio::test]
async fn retained_service_uninstall_failed_callback_requires_ack_without_replay() {
    // Real existing fixture supplies isolated source and complete service dependencies.
    // 真实既有夹具提供隔离来源及完整服务依赖。
    let fixture = Fixture::new(false);
    // Declare callback before the production managed source is created.
    // 在生产受管来源创建之前声明回调。
    let events = declare_failed_callback(&fixture);
    // All true update and uninstall policy writes use this private destination.
    // 全部真实更新及卸载策略写入使用此私有目标。
    let destination = tempfile::tempdir().expect("private policy directory");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination.path().join("policy.json"), async {
            seed_managed(&fixture).await;
            // Freeze the actual disabled descriptor generation before the original removal is admitted.
            // 在原移除入场之前冻结实际停用描述符代次。
            let original_generation = fixture
                .runtime
                .snapshot()
                .plugins
                .get(PLUGIN)
                .expect("installed disabled source")
                .generation;
            let error = service_uninstall_user_plugin(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                PLUGIN.to_owned(),
            )
            .await
            .expect_err("actual retired failed callback");
            assert!(error.contains("外部副作用"), "{error}");
            assert_eq!(events.0.load(Ordering::Acquire), 1);
            // Sanitized status exposes acknowledgement without private callback diagnostics.
            // 脱敏状态暴露确认需求，不暴露私有回调诊断。
            let row = service_local_plugin_uninstall_requests(Arc::clone(&fixture.state))
                .expect("live original query")
                .into_iter()
                .find(|row| row.plugin_id == PLUGIN)
                .expect("actual original row");
            assert!(row.requires_external_effects_acknowledgement);
            // A real failed callback remains readable while the original mutable execution borrow is held.
            // 即使原可变执行借用被持有，真实失败回调仍保持可读取。
            {
                // The same strong registry supplies the actual retained owner, not reconstructed metadata.
                // 相同强注册表提供实际保留所有者，而非重建元数据。
                let registry = Arc::clone(
                    &fixture
                        .state
                        .lock()
                        .expect("state")
                        .pending_local_plugin_uninstalls,
                );
                // UUID is parsed only from the original status delivered by production service code.
                // UUID 仅从生产服务代码交付的原状态解析。
                let request = uuid::Uuid::parse_str(&row.request_id).expect("original request UUID");
                // This owner is the same concrete allocation that produced the failed callback evidence.
                // 此所有者为产生失败回调证据的相同具体分配。
                let original = registry.lookup(PLUGIN, request).expect("actual original");
                // Holding this borrow reproduces the live executor's exclusion without restarting its callback.
                // 持有此借用复现实时执行器的互斥，不重新启动其回调。
                let _execution_borrow = original.resources.lock().await;
                // The native parent ID is read from the exact fixture manager, never guessed from the request.
                // 原生父级身份从精确夹具管理器读取，绝不从请求猜测。
                let runtime_id = fixture
                    .runtime
                    .formal_runtime_status()
                    .expect("real parent status")
                    .expect("attached native parent")
                    .runtime_id;
                // This deadline detects accidental joining of the held execution borrow, not Lua performance.
                // 此截止时间检测误联接已持有执行借用，而非 Lua 性能。
                let sample = tokio::time::timeout(
                    std::time::Duration::from_secs(1),
                    crate::plugin_runtime_services::diagnostics::uninstall_drain(
                        Arc::clone(&fixture.state),
                        runtime_id,
                        PLUGIN.to_owned(),
                        request,
                    ),
                )
                .await
                .expect("diagnostic read must not wait for the original execution borrow")
                .expect("actual original diagnostic sample");
                assert_eq!(sample.request_id, row.request_id);
                assert_eq!(sample.plugin_id, PLUGIN);
                assert_eq!(sample.plugin_generation, original_generation.to_string());
                assert!(
                    sample
                        .resources
                        .as_ref()
                        .expect("actual native ownership sample")
                        .resources
                        .iter()
                        .all(|resource| !resource.is_drain_blocking),
                    "physically retired callback failure must not masquerade as a live drain blocker"
                );
                assert!(
                    !serde_json::to_string(&sample)
                        .expect("safe diagnostic JSON")
                        .contains("actual failed uninstall after native effect"),
                    "diagnostic metadata must not expose callback business error text"
                );
            }
            service_resume_local_plugin_uninstall(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                PLUGIN.to_owned(),
                row.request_id.clone(),
                false,
            )
            .await
            .expect_err("explicit acknowledgement still required");
            assert_eq!(events.0.load(Ordering::Acquire), 1);
            // Shutdown physically drains but cannot authorize deleting a known failed-effects original.
            // 关闭进行物理排空，但不能授权删除已知失败副作用原实例。
            let registry = Arc::clone(
                &fixture
                    .state
                    .lock()
                    .expect("state")
                    .pending_local_plugin_uninstalls,
            );
            registry
                .drain_for_shutdown()
                .await
                .expect_err("shutdown must retain unacknowledged original");
            assert!(
                registry
                    .lookup(
                        PLUGIN,
                        uuid::Uuid::parse_str(&row.request_id).expect("request UUID")
                    )
                    .is_ok()
            );
            service_resume_local_plugin_uninstall(
                ServiceEventEmitter::new(vec![Arc::new(FailedDelivery)]),
                Arc::clone(&fixture.state),
                PLUGIN.to_owned(),
                row.request_id.clone(),
                true,
            )
            .await
            .expect_err("actual client delivery panic after original terminal work");
            // The actual completed original owns the exact projection receipt after client delivery failure.
            // 客户端投递失败之后，实际已完成原实例拥有精确投影回执。
            let owner = registry
                .lookup(
                    PLUGIN,
                    uuid::Uuid::parse_str(&row.request_id).expect("request UUID"),
                )
                .expect("same original after client panic");
            // Observe the genuine neighboring revision and Full before retrying public delivery.
            // 重试公开投递之前观测真实相邻修订及完整代次。
            let (revision, committed) = {
                // No observer substitutes for its actual physical original or projection.
                // 任何观测者均不替代实际物理原实例或投影。
                let resources = owner.resources.lock().await;
                assert!(resources.collection_confirmed);
                assert!(resources.effects_acknowledged);
                (
                    resources
                        .projection
                        .as_ref()
                        .expect("actual retained projection")
                        .revision,
                    Arc::clone(
                        resources
                            .committed_full
                            .as_ref()
                            .expect("actual retained Full"),
                    ),
                )
            };
            // Acknowledged terminal work must have persisted valid JSON at this exact private destination.
            // 已确认终态工作必须在此精确私有目标持久化有效 JSON。
            assert!(destination.path().join("policy.json").is_file());
            assert!(
                serde_json::from_slice::<serde_json::Value>(
                    &std::fs::read(destination.path().join("policy.json"))
                        .expect("actual private acknowledged removal policy"),
                )
                .expect("actual private acknowledged removal JSON")
                .is_object()
            );
            assert_eq!(events.0.load(Ordering::Acquire), 1);
            // Successful delivery uses the same response revision and actual Full, not a second projection or callback.
            // 成功投递使用相同响应修订及实际完整代次，而非第二次投影或回调。
            let update = service_resume_local_plugin_uninstall(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                PLUGIN.to_owned(),
                row.request_id,
                false,
            )
            .await
            .expect("same original event delivery retry");
            assert_eq!(update.revision, revision);
            assert_eq!(
                fixture.state.lock().expect("state").workbench_generation,
                revision
            );
            assert!(Arc::ptr_eq(
                &committed,
                &fixture
                    .state
                    .lock()
                    .expect("state")
                    .agent_runtime
                    .as_ref()
                    .expect("bridge")
                    .extension_generation
            ));
            assert_eq!(events.0.load(Ordering::Acquire), 1);
            assert!(
                service_local_plugin_uninstall_requests(Arc::clone(&fixture.state))
                    .expect("empty original query")
                    .is_empty()
            );
        })
        .await;
    fixture
        .runtime
        .request_formal_runtime_close()
        .expect("close actual formal parent");
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if fixture
                    .runtime
                    .poll_formal_runtime_closed()
                    .expect("actual parent close")
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .is_ok()
    );
}

/// Upgrade through the actual public service; fixture owns source bytes and returns no synthetic selection.
/// 经实际公开服务升级；fixture 拥有来源字节，不返回合成选择。
/// Returns unit after the disabled managed selection and genuine Full are confirmed.
/// 停用受管选择及真实完整代次确认后返回空值。
async fn seed_managed(fixture: &Fixture) {
    // Production inspection is the only authority for content fingerprint and complete permissions.
    // 生产检查是内容指纹及完整权限的唯一权威。
    let preview = fixture
        .runtime
        .inspect_local_user_plugin(&fixture.source)
        .expect("actual source preview");
    service_update_local_plugin(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.state),
        fixture.source.to_string_lossy().into_owned(),
        preview.content_fingerprint,
        preview.permissions,
        false,
        Vec::new(),
    )
    .await
    .expect("actual disabled managed update");
    assert!(
        fixture.runtime.snapshot().plugins[PLUGIN]
            .managed_package()
            .is_some()
    );
}

/// True managed removal releases its namespace even while a query/executor Arc remains alive.
/// 真实受管移除即使查询／执行器引用仍存活，也释放命名空间。
/// No inputs or return value; checks catalog absence, registry removal and independent File acquisition.
/// 无输入或返回值；检查目录缺失、注册表移除及独立文件取得。
#[tokio::test]
async fn retained_service_uninstall_terminal_releases_actual_original() {
    // Reuse the real installed disabled managed predecessor and production service bridge.
    // 复用真实已安装停用受管前驱及生产服务桥。
    let fixture = Fixture::new(false);
    // Every provider write belongs to one test-local destination.
    // 每个供应商写入属于单个测试局部目标。
    let destination = tempfile::tempdir().expect("private policy directory");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination.path().join("policy.json"), async {
            // Capture creates and retains the real physical original before effects.
            // 捕获在副作用之前创建并保留真实物理原实例。
            let owner = owner::reserve(&fixture.state, PLUGIN).expect("actual managed original");
            // Ordinary replacement must fail before changing the actual routing Arc.
            // 普通替换必须在改变实际路由引用之前失败。
            let previous = fixture.runtime.snapshot();
            assert!(
                fixture
                    .runtime
                    .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
                    .is_err()
            );
            assert!(Arc::ptr_eq(&previous, &fixture.runtime.snapshot()));
            drop(previous);
            execute(
                &ServiceEventEmitter::new(Vec::new()),
                &fixture.state,
                Arc::clone(&owner),
                false,
            )
            .await
            .expect("actual managed terminal removal");
            assert!(!fixture.runtime.snapshot().plugins.contains_key(PLUGIN));
            assert!(
                service_local_plugin_uninstall_requests(Arc::clone(&fixture.state))
                    .expect("readonly query")
                    .is_empty()
            );
            assert!(owner.resources.lock().await.original.is_none());
            // A new independent physical guard can acquire while the former owner Arc is still alive.
            // 前所有者引用仍存活时，新独立物理守卫可以取得。
            let store =
                VulcanPluginGenerationStore::open_existing(fixture.runtime.user_plugin_root())
                    .expect("observer store")
                    .expect("existing store");
            let tombstone = store.activation(PLUGIN).expect("committed tombstone");
            store
                .acquire_legacy_mutation_guard(PLUGIN, tombstone.as_ref())
                .expect("actual original File released");
        })
        .await;
}

/// Actual managed readers block GC after successful Full removal, and the same request continues after reader release.
/// 实际受管读者在成功完整移除之后阻止回收，相同请求在读者释放后继续。
/// No inputs or return value; original Arc/File and committed Full remain exact across both attempts.
/// 无输入或返回值；两次尝试期间原引用／文件及已提交完整代次保持精确。
#[tokio::test]
async fn retained_service_uninstall_managed_external_pin_reuses_original() {
    // Original flat source is upgraded through production into a complete managed source.
    // 原平铺来源经生产升级为完整受管来源。
    let fixture = Fixture::new(false);
    // Policy persistence is test-local, including the real managed seed update.
    // 策略持久化为测试局部，包含真实受管建立更新。
    let destination = tempfile::tempdir().expect("private policy directory");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination.path().join("policy.json"), async {
            seed_managed(&fixture).await;
            // This actual catalog holds the real package reader, not copied metadata.
            // 此实际目录持有真实插件包读者，而非复制元数据。
            let external = fixture.runtime.snapshot();
            // The physical path is observed from the authoritative selected descriptor.
            // 从权威已选描述符观测物理路径。
            let package_path = external.plugins[PLUGIN].package_root.clone();
            service_uninstall_user_plugin(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                PLUGIN.to_owned(),
            )
            .await
            .expect_err("actual external reader must block collection");
            // Stable identity locates the one live request without relying on row order.
            // 稳定身份定位单个存活请求，不依赖行顺序。
            let row = service_local_plugin_uninstall_requests(Arc::clone(&fixture.state))
                .expect("live query")
                .into_iter()
                .find(|row| row.plugin_id == PLUGIN)
                .expect("same original");
            assert_eq!(row.phase, LocalPluginUninstallPhase::Collection);
            // Collection admission follows the actual policy commit, which must target this private JSON file.
            // 回收准入位于实际策略提交之后，该提交必须写入此私有 JSON 文件。
            assert!(destination.path().join("policy.json").is_file());
            assert!(
                serde_json::from_slice::<serde_json::Value>(
                    &std::fs::read(destination.path().join("policy.json"))
                        .expect("actual private managed removal policy"),
                )
                .expect("actual private managed removal JSON")
                .is_object()
            );
            assert!(package_path.exists());
            // Lookup clones the same strong slot without recreating its physical originals.
            // 查询克隆相同强槽，不重新创建物理原实例。
            let owner = fixture
                .state
                .lock()
                .expect("state")
                .pending_local_plugin_uninstalls
                .lookup(
                    PLUGIN,
                    uuid::Uuid::parse_str(&row.request_id).expect("request UUID"),
                )
                .expect("actual owner");
            // Observe the actual committed pointer before releasing the physical reader.
            // 释放物理读者之前观测实际已提交指针。
            let committed = Arc::clone(
                owner
                    .resources
                    .lock()
                    .await
                    .committed_full
                    .as_ref()
                    .expect("real Full receipt"),
            );
            drop(external);
            service_resume_local_plugin_uninstall(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                PLUGIN.to_owned(),
                row.request_id,
                false,
            )
            .await
            .expect("same original cleanup retry");
            assert!(!package_path.exists());
            assert!(Arc::ptr_eq(
                &committed,
                &fixture
                    .state
                    .lock()
                    .expect("state")
                    .agent_runtime
                    .as_ref()
                    .expect("bridge")
                    .extension_generation
            ));
            assert!(owner.resources.lock().await.original.is_none());
        })
        .await;
}

/// A true poisoned base Hook mutex fails actual Full preparation, preserving the original File for explicit repair/retry.
/// 真实中毒基础 Hook 互斥锁使实际完整准备失败，保留原文件供明确修复／重试。
/// No inputs or return value; failure does not retire the owner or permit ordinary writers.
/// 无输入或返回值；失败不退役所有者，也不允许普通写入器。
#[tokio::test]
async fn retained_service_uninstall_full_preparation_failure_preserves_original() {
    // The real disabled flat fixture needs no synthetic callback or managed source.
    // 真实停用平铺夹具无需合成回调或受管来源。
    let fixture = Fixture::new(false);
    // All provider writes remain isolated during failure and continuation.
    // 失败及继续期间全部供应商写入保持隔离。
    let destination = tempfile::tempdir().expect("private policy directory");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination.path().join("policy.json"), async {
            // Strong reservation captures actual Full before poisoning its separate actual base Hook mutex.
            // 强预留在使独立实际基础 Hook 互斥锁中毒之前捕获实际完整代次。
            let owner = owner::reserve(&fixture.state, PLUGIN).expect("actual original");
            // Exact existing Hook policy Arc is used by the production Full preparation path.
            // 生产完整准备路径使用精确既有 Hook 策略引用。
            let hooks = Arc::clone(
                &fixture
                    .state
                    .lock()
                    .expect("state")
                    .agent_runtime
                    .as_ref()
                    .expect("bridge")
                    .conversation_hook_runtime,
            );
            // Panic while holding the real mutex, without substituting any Full/catalog/source allocation.
            // 持有真实互斥锁时 panic，不替代任何完整代次／目录／来源分配。
            let poisoned = Arc::clone(&hooks);
            assert!(
                std::thread::spawn(move || {
                    let _guard = poisoned.lock().expect("base Hook policy");
                    panic!("intentional actual Full preparation mutex failure");
                })
                .join()
                .is_err()
            );
            let error = execute(
                &ServiceEventEmitter::new(Vec::new()),
                &fixture.state,
                Arc::clone(&owner),
                false,
            )
            .await
            .expect_err("actual Full preparation failure");
            assert!(error.contains("conversation hook runtime"), "{error}");
            assert!(owner.resources.lock().await.committed_full.is_none());
            assert!(
                fixture
                    .runtime
                    .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
                    .is_err()
            );
            // Explicit test repair restores only the existing sequencing mutex, never original physical authority.
            // 明确测试修复仅恢复既有排序互斥锁，绝不恢复原物理权威。
            hooks.clear_poison();
            service_resume_local_plugin_uninstall(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                PLUGIN.to_owned(),
                owner.request_id.to_string(),
                false,
            )
            .await
            .expect("same actual original Full retry");
            assert!(owner.resources.lock().await.original.is_none());
        })
        .await;
}

/// Refused actual policy membership never creates an uncertain-persistence intent.
/// 被拒绝实际策略成员关系绝不创建不确定持久化意图。
/// No inputs or return value; checks the actual retained original and explicit validation failure.
/// 无输入或返回值；检查实际保留原实例及明确校验失败。
#[tokio::test]
async fn retained_service_uninstall_policy_admission_failure_has_no_write_intent() {
    // A real original captures the complete old disabled/enabled pair before any policy effects.
    // 真实原实例在任何策略副作用之前捕获完整旧停用／启用配对。
    let fixture = Fixture::new(false);
    // Isolate the refused persistence destination as well as eventual cleanup.
    // 隔离被拒绝持久化目标及最终清理。
    let destination = tempfile::tempdir().expect("private policy directory");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination.path().join("policy.json"), async {
            // Same actual map slot is used by the lower configuration transaction and subsequent recovery.
            // 相同实际映射槽用于下层配置事务及随后恢复。
            let owner = owner::reserve(&fixture.state, PLUGIN).expect("actual original");
            fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_mut()
                .expect("bridge")
                .provider_configuration
                .luaskills
                .disabled_vulcan_plugins
                .remove(PLUGIN);
            {
                // Direct actual resource borrowing observes the failed admission without any fabricated phase.
                // 直接实际资源借用观察失败准入，不伪造任何阶段。
                let mut resources = owner.resources.lock().await;
                publication::remove_policy(&fixture.state, &owner, &mut resources)
                    .await
                    .expect_err("changed actual membership");
                assert!(!resources.policy_started);
                assert!(!resources.policy_committed);
            }
            fixture
                .state
                .lock()
                .expect("state")
                .agent_runtime
                .as_mut()
                .expect("bridge")
                .provider_configuration
                .luaskills
                .disabled_vulcan_plugins
                .insert(PLUGIN.to_owned());
            execute(
                &ServiceEventEmitter::new(Vec::new()),
                &fixture.state,
                owner,
                false,
            )
            .await
            .expect("actual original after explicit membership repair");
        })
        .await;
}

/// Cancelling while the actual Provider transaction gate is held cannot claim an admitted persistence intent.
/// 实际供应商事务门被持有期间取消，不能宣称已准入持久化意图。
/// No inputs or return value; real gate, original resource borrow and pending future prove the ordering.
/// 无输入或返回值；真实门、原资源借用及待处理 Future 证明顺序。
#[tokio::test]
async fn retained_service_uninstall_policy_gate_cancellation_has_no_write_intent() {
    // Reserve the actual installed flat source and retain its same physical original.
    // 预留实际已安装平铺来源，并保留相同物理原实例。
    let fixture = Fixture::new(false);
    // All eventual policy persistence remains test-local after cancellation.
    // 取消之后所有最终策略持久化保持测试局部。
    let destination = tempfile::tempdir().expect("private policy directory");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination.path().join("policy.json"), async {
            // Actual same-slot resources retain their intent across a cancelled observer future.
            // 实际同槽资源跨取消观测 Future 保留其意图。
            let owner = owner::reserve(&fixture.state, PLUGIN).expect("actual original");
            {
                // Hold the production serialization gate so removal cannot enter service/member validation or persistence.
                // 持有生产串行化门，使移除无法进入服务／成员校验或持久化。
                let transaction =
                    crate::service_state_access::begin_provider_configuration_transaction().await;
                // The original borrowed resource survives dropping the blocked configuration future.
                // 原借用资源在丢弃阻塞配置 Future 后存续。
                let mut resources = owner.resources.lock().await;
                assert!(
                    tokio::time::timeout(
                        std::time::Duration::from_millis(20),
                        publication::remove_policy(&fixture.state, &owner, &mut resources)
                    )
                    .await
                    .is_err()
                );
                assert!(!resources.policy_started);
                assert!(!resources.policy_committed);
                drop(transaction);
            }
            execute(
                &ServiceEventEmitter::new(Vec::new()),
                &fixture.state,
                owner,
                false,
            )
            .await
            .expect("same actual original after gate cancellation");
        })
        .await;
}
