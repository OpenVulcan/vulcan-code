//! Actual original native ownership verifies bootstrap, generation and irreversible shutdown boundaries.
//! 实际原生所有权验证引导、代次及不可逆关闭边界。

use super::*;
use crate::tests::test_support::{test_agent_runtime_bridge, test_service_shell_host};
use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use vulcan_plugin_runtime::VulcanPluginRuntimeSettings;

mod retirement;

/// Private roots and one real native engine outlive their original service manager.
/// 私有根及单个真实原生引擎比原服务管理器存活更久。
struct Fixture {
    /// The service retains its manager before any formal parent can start.
    /// 服务在任何正式父级可以启动前保留其管理器。
    state: SharedServiceBridgeState,
    /// The original manager is never replaced by a generation snapshot.
    /// 原管理器绝不被代次快照替换。
    manager: VulcanPluginRuntimeManager,
    /// The real engine supplies the production formal constructor.
    /// 真实引擎提供生产正式构造器。
    engine: Arc<LuaEngine>,
    /// All engine and plugin paths stay inside this temporary owner.
    /// 全部引擎及插件路径保持在此临时所有者内。
    root: tempfile::TempDir,
}

impl Fixture {
    /// Create exact private roots and an unattached original manager; no inputs, returns the real fixture.
    /// 创建精确私有根及尚未附加的原管理器；无输入，返回真实夹具。
    fn new() -> Self {
        // Native paths follow the established integration fixture's Windows normalization.
        // 原生路径遵循既有集成夹具的 Windows 规范化。
        let root = tempfile::tempdir().expect("native shutdown roots");
        // The native engine owns actual VM services, independently of formal admission.
        // 原生引擎拥有真实 VM 服务，独立于正式准入。
        let engine = Arc::new(
            LuaEngine::new(LuaEngineOptions::new(
                LuaVmPoolConfig {
                    min_size: 1,
                    max_size: 1,
                    idle_ttl_secs: 60,
                },
                LuaRuntimeHostOptions::with_runtime_root(vulcan_runtime_assets::native_host_path(
                    &root.path().canonicalize().expect("canonical fixture"),
                )),
            ))
            .expect("real engine"),
        );
        // The fixed bridge is the repository's actual authoritative adapter, not a mock dispatcher.
        // 固定桥接为仓库真实权威适配器，而非模拟分发器。
        let bridge = root.path().join("bridge");
        std::fs::create_dir_all(bridge.join("runtime")).expect("bridge root");
        std::fs::write(bridge.join("dependencies.yaml"), "{}\n").expect("bridge dependencies");
        std::fs::write(
            bridge.join("runtime/bridge.lua"),
            include_str!("../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
        )
        .expect("real bridge");
        std::fs::create_dir_all(root.path().join("bundled")).expect("bundled root");
        std::fs::create_dir_all(root.path().join("user")).expect("user root");
        std::fs::create_dir_all(root.path().join("distributions")).expect("distribution root");
        // The metadata-only manager cannot start a parent until the service gate admits attachment.
        // 纯元数据管理器在服务门准入附加之前不能启动父级。
        let manager = VulcanPluginRuntimeManager::without_runtime(
            root.path().join("bundled"),
            root.path().join("user"),
            bridge,
            root.path().join("distributions"),
            root.path().join("state"),
            root.path().join("runtime"),
        );
        Self {
            state: crate::create_service_bridge_state(test_service_shell_host()),
            manager,
            engine,
            root,
        }
    }

    /// Attach the engine and parent under the production short gate using `settings`; return its exact outcome.
    /// 使用 `settings` 在生产短门下附加引擎及父级；返回精确结果。
    fn initialize(&self, settings: VulcanPluginRuntimeSettings) -> Result<(), String> {
        retain_and_initialize_original_manager(&self.state, &self.manager, || {
            self.manager
                .attach_runtime_engine(Arc::clone(&self.engine))
                .map_err(|error| error.to_string())?;
            self.manager
                .initialize_formal_runtime(settings)
                .map_err(|error| error.to_string())
        })
    }

    /// Freeze closing and its permanent boundary in the same actual service gate; no inputs or result.
    /// 在同一实际服务门冻结关闭及其永久边界；无输入及返回值。
    fn block_recovery(&self) {
        // The original slot survives every drain attempt.
        // 原槽位在每次排空尝试中持续存在。
        let mut service = self.state.lock().expect("service gate");
        service.service_lifecycle_state.begin_shutdown_drain();
        service.service_lifecycle_state.block_shutdown_recovery();
    }

    /// Observe the core-issued parent identity; no inputs, returns the frozen native ID.
    /// 观测核心签发父级身份；无输入，返回冻结原生 ID。
    fn runtime_id(&self) -> String {
        self.manager
            .formal_runtime_status()
            .expect("status")
            .expect("attached parent")
            .runtime_id
    }
}

/// A parent started before ready survives bootstrap failure and is really joined through its original owner.
/// 就绪前启动的父级在引导失败后仍存活，并通过原所有者真实等待退出。
/// No inputs or result; actual Core status and final polling supply the assertions.
/// 无输入及返回值；真实 Core 状态及最终轮询提供断言。
#[test]
fn bootstrap_failure_before_agent_publication_retains_and_closes_real_parent() {
    // Keep the original manager in the service before introducing the exact post-initialization failure.
    // 在引入精确初始化后失败之前，将原管理器保留在服务内。
    let fixture = Fixture::new();
    assert!(
        retain_and_initialize_original_manager(&fixture.state, &fixture.manager, || {
            fixture
                .manager
                .attach_runtime_engine(fixture.engine.clone())
                .map_err(|error| error.to_string())?;
            fixture
                .manager
                .initialize_formal_runtime(VulcanPluginRuntimeSettings::default())
                .map_err(|error| error.to_string())?;
            Err(String::from(
                "actual bootstrap failure after parent creation",
            ))
        })
        .is_err()
    );
    assert!(fixture.state.lock().expect("state").agent_runtime.is_none());
    // This identity remains discoverable independently of ready publication.
    // 此身份独立于就绪发布而继续可发现。
    let original = fixture.runtime_id();
    fixture.block_recovery();
    close_original_plugin_runtime(&fixture.state).expect("real original parent closed");
    assert!(
        fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("real joins")
    );
    assert_eq!(fixture.runtime_id(), original);
    assert!(
        fixture
            .state
            .lock()
            .expect("retained owner")
            .service_lifecycle_state
            .original_plugin_manager
            .is_some()
    );
}

/// A real post-Core registration failure remains an error for business, while the original service shutdown can still join it and retry.
/// 真实 Core 后注册失败对业务持续保持错误，同时原服务关闭仍可等待其退出并重试。
/// No inputs or result; the valid overlong configured owner fails only the actual native registration boundary.
/// 无输入及返回值；有效超长配置所有者仅在实际原生注册边界失败。
#[test]
fn failed_native_registration_remains_owned_by_the_original_service_shutdown() {
    use luaskills::runtime::embedded::{EmbeddedCapacityConfig, PoolKind, VmCapacityConfig};
    use vulcan_plugin_runtime::VulcanPluginCapacityPurpose;
    // This fixture is still before agent-runtime publication, matching the real failing bootstrap boundary.
    // 此夹具仍位于智能体运行时发布之前，匹配真实失败引导边界。
    let fixture = Fixture::new();
    // The fixture setting bounds memory without bypassing any production validation.
    // 夹具设置约束内存，不绕过任何生产校验。
    let mut settings = VulcanPluginRuntimeSettings::default();
    settings.parent.max_value_bytes = 1024;
    // Only the original Core byte limit determines the valid owner's excessive length.
    // 仅原 Core 字节上限决定合法所有者的过长长度。
    let plugin_id = "a".repeat(settings.parent.max_value_bytes + 1);
    settings.capacity_budgets.insert(
        plugin_id,
        [(
            VulcanPluginCapacityPurpose::Hook,
            EmbeddedCapacityConfig {
                resources: VmCapacityConfig {
                    kind: PoolKind::Shared,
                    min_resident_vms: 0,
                    max_resident_vms: settings.parent.max_resident_vms,
                    max_running_calls: settings.parent.max_running_calls,
                },
                max_queued_calls: settings.parent.max_queued_calls,
                max_queued_bytes: settings.parent.max_queued_bytes,
            },
        )]
        .into(),
    );
    settings.validate().expect("valid original settings");
    // Original initialization returns the actual native error, not a synthetic failure before Core construction.
    // 原初始化返回实际原生错误，而非 Core 构造之前的模拟失败。
    let original_error = fixture
        .initialize(settings)
        .expect_err("actual native registration failure");
    assert!(original_error.contains("JSON value exceeds its configured byte limit"));
    assert_eq!(
        fixture
            .manager
            .formal_runtime_status()
            .err()
            .expect("failed original is not absence")
            .message,
        original_error
    );
    assert!(
        !fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("constructed Core remains open")
    );
    assert!(
        fixture
            .state
            .lock()
            .expect("original owner")
            .service_lifecycle_state
            .original_plugin_manager
            .is_some()
    );
    fixture.block_recovery();
    close_original_plugin_runtime(&fixture.state).expect("service closes actual failed original");
    assert!(
        fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("actual failed Core joins")
    );
    fixture
        .state
        .lock()
        .expect("retry")
        .service_lifecycle_state
        .begin_shutdown_drain();
    close_original_plugin_runtime(&fixture.state)
        .expect("service retries the same failed original");
    assert_eq!(
        fixture
            .manager
            .formal_runtime_status()
            .err()
            .expect("original failure persists")
            .message,
        original_error
    );
    assert!(crate::service_resume_after_failed_app_shutdown(&fixture.state).is_err());
}

/// Closing without a parent permanently rejects late attachment instead of returning a false native success.
/// 无父级关闭后永久拒绝迟到附加，避免虚假的原生成功。
/// No inputs or result; the actual manager must still report explicit absence.
/// 无输入及返回值；实际管理器仍须报告明确缺失。
#[test]
fn closing_before_initialization_rejects_the_late_original_parent() {
    // Explicit pre-bootstrap absence is allowed only with the future initialization gate closed.
    // 明确引导前缺失仅在未来初始化门关闭时允许。
    let fixture = Fixture::new();
    fixture.block_recovery();
    close_original_plugin_runtime(&fixture.state).expect("explicit absence");
    assert!(
        fixture
            .initialize(VulcanPluginRuntimeSettings::default())
            .is_err()
    );
    assert!(
        fixture
            .manager
            .formal_runtime_status()
            .expect("absence")
            .is_none()
    );
    assert!(require_bootstrap_admission(&fixture.state.lock().expect("state")).is_err());
}

/// An actual metadata-only original stays explicitly absent, and the real HTTP failure gate rejects native initialization.
/// 实际纯元数据原所有者保持明确缺失，真实 HTTP 失败门拒绝原生初始化。
/// No inputs or result; neither branch allocates a formal parent or substitutes another manager.
/// 无输入及返回值；两分支均不分配正式父级或替换另一管理器。
#[test]
fn metadata_only_original_and_http_failure_keep_native_absence_explicit() {
    // A retained content-only manager is an explicit supported startup outcome.
    // 已保留纯内容管理器是明确支持的启动结果。
    let content = Fixture::new();
    retain_and_initialize_original_manager(&content.state, &content.manager, || Ok(()))
        .expect("retain original content manager");
    content.block_recovery();
    close_original_plugin_runtime(&content.state).expect("content-only absence");
    assert!(
        content
            .manager
            .formal_runtime_status()
            .expect("content status")
            .is_none()
    );
    assert!(
        content
            .state
            .lock()
            .expect("content owner")
            .service_lifecycle_state
            .original_plugin_manager
            .is_some()
    );
    // A failed required HTTP bridge cannot later publish a parent or ready state.
    // 必需 HTTP 桥失败后不能迟到发布父级或就绪状态。
    let failed = Fixture::new();
    failed
        .state
        .lock()
        .expect("HTTP failure fact")
        .service_lifecycle_state
        .http_bridge_failed = true;
    assert!(
        failed
            .initialize(VulcanPluginRuntimeSettings::default())
            .is_err()
    );
    assert!(
        failed
            .manager
            .formal_runtime_status()
            .expect("unattached status")
            .is_none()
    );
    assert!(require_bootstrap_admission(&failed.state.lock().expect("ready gate")).is_err());
}

/// The same gate prevents closing from observing an empty attachment while initialization is still inside its short boundary.
/// 同一门防止初始化仍位于短边界内时，关闭观察到空附加状态。
/// No inputs or result; channels hold the exact constructor boundary and bounded waits detect premature completion.
/// 无输入及返回值；通道保持精确构造边界，有界等待检测过早完成。
#[test]
fn initialization_and_closing_share_the_original_service_gate() {
    // Every channel wait derives from the established native shutdown setting.
    // 每次通道等待均派生自既有原生关闭设置。
    let fixture = Fixture::new();
    // The configured limit bounds injected constructor delay and all thread observations.
    // 已配置限制约束注入构造延迟及全部线程观测。
    let wait = Duration::from_millis(VulcanPluginRuntimeSettings::default().shutdown_timeout_ms);
    // Signal only after the initializer owns the real service gate and the original slot.
    // 仅在初始化器拥有真实服务门及原槽位之后发送信号。
    let (entered_tx, entered_rx) = mpsc::channel();
    // Release the held boundary only after the closing thread has been started.
    // 仅在关闭线程启动之后释放保持的边界。
    let (release_tx, release_rx) = mpsc::channel();
    // Completion cannot occur before the initializer leaves the gate.
    // 初始化器离开门之前不能完成关闭。
    let (closed_tx, closed_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        // Move the non-Sync receiver into its sole worker while sharing only the real fixture.
        // 将非 Sync 接收器移入其唯一工作者，仅共享真实夹具。
        let initializing = &fixture;
        scope.spawn(move || {
            retain_and_initialize_original_manager(
                &initializing.state,
                &initializing.manager,
                || {
                    entered_tx.send(()).expect("entered gate");
                    release_rx
                        .recv_timeout(wait)
                        .expect("release constructor boundary");
                    initializing
                        .manager
                        .attach_runtime_engine(initializing.engine.clone())
                        .map_err(|error| error.to_string())?;
                    initializing
                        .manager
                        .initialize_formal_runtime(VulcanPluginRuntimeSettings::default())
                        .map_err(|error| error.to_string())
                },
            )
            .expect("real admitted constructor")
        });
        entered_rx
            .recv_timeout(wait)
            .expect("constructor owns gate");
        scope.spawn(|| {
            fixture.block_recovery();
            closed_tx
                .send(close_original_plugin_runtime(&fixture.state))
                .expect("closing result");
        });
        assert!(matches!(
            closed_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        release_tx.send(()).expect("release native initialization");
        closed_rx
            .recv_timeout(wait)
            .expect("bounded close result")
            .expect("real parent closed");
    });
    assert!(
        fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("joined parent")
    );
}

/// Retained older Full generations cannot hide or replace the original native parent.
/// 保留较旧 Full 代次不能隐藏或替换原生父级。
/// No inputs or result; the actual old extension remains retained while the sole parent closes.
/// 无输入及返回值；唯一父级关闭时，实际旧扩展仍保留。
#[test]
fn older_extension_generation_still_observes_the_same_closed_parent() {
    // All manager clones share the one core-issued identity.
    // 全部管理器克隆共享同一核心签发身份。
    let fixture = Fixture::new();
    fixture
        .initialize(VulcanPluginRuntimeSettings::default())
        .expect("initialize");
    // Reuse the real service bridge's established complete extension shape.
    // 复用真实服务桥既有完整扩展形状。
    let mut bridge = test_agent_runtime_bridge();
    bridge.vulcan_plugin_runtime = fixture.manager.clone();
    bridge.vulcan_plugin_native_mcp_host = fixture.manager.register_native_mcp_host_adapter();
    // Keep the actual displaced Full allocation alive while publishing its successor.
    // 发布后继时保持实际被替换 Full 分配存活。
    let old = bridge.extension_generation.clone();
    bridge.extension_generation = Arc::new(
        old.republish(old.mcp_runtime(), old.prompt_skill_catalog())
            .expect("successor"),
    );
    fixture.state.lock().expect("state").agent_runtime = Some(bridge);
    // Identity checks are independent of catalog positions and extension revision numbers.
    // 身份检查独立于目录位置及扩展修订数字。
    let original = fixture.runtime_id();
    fixture.block_recovery();
    close_original_plugin_runtime(&fixture.state).expect("closed across generations");
    assert!(
        fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("joined")
    );
    assert_eq!(fixture.runtime_id(), original);
    assert!(Arc::strong_count(&old) >= 1);
}

/// A shutdown retry and drain completion never erase permanent closure, while an earlier legal recovery remains possible.
/// 关闭重试及排空完成绝不抹除永久关闭，同时较早的合法恢复仍可执行。
/// No inputs or result; the actual recovery entry supplies both admission outcomes.
/// 无输入及返回值；真实恢复入口提供两种准入结果。
#[test]
fn irreversible_native_closure_survives_retry_without_breaking_earlier_recovery() {
    // This service has not crossed any irreversible boundary.
    // 此服务尚未越过任何不可逆边界。
    let fixture = Fixture::new();
    fixture
        .state
        .lock()
        .expect("state")
        .service_lifecycle_state
        .begin_shutdown_drain();
    crate::service_resume_after_failed_app_shutdown(&fixture.state).expect("legal early recovery");
    assert!(require_bootstrap_admission(&fixture.state.lock().expect("state")).is_ok());
    fixture
        .initialize(VulcanPluginRuntimeSettings::default())
        .expect("initialize");
    fixture.block_recovery();
    close_original_plugin_runtime(&fixture.state).expect("permanent native close");
    assert!(crate::service_resume_after_failed_app_shutdown(&fixture.state).is_err());
    {
        // Retry begins on the same owner and cannot restore its already closed native runtime.
        // 重试在同一所有者上开始，不能恢复其已经关闭的原生运行时。
        let mut service = fixture.state.lock().expect("retry gate");
        service.service_lifecycle_state.begin_shutdown_drain();
        assert!(service.service_lifecycle_state.shutdown_recovery_blocked);
        service.service_lifecycle_state.end_shutdown_drain();
        assert!(service.service_lifecycle_state.shutdown_recovery_blocked);
    }
    assert!(crate::service_resume_after_failed_app_shutdown(&fixture.state).is_err());
    close_original_plugin_runtime(&fixture.state).expect("same owner retry");
}
