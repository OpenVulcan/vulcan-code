//! Real Lua finalization must retain its native owner through timeout and actual retry joins.
//! 真实 Lua 关闭必须在超时及实际重试等待退出期间保留其原生所有者。

use super::*;
use std::collections::BTreeSet;
use vulcan_plugin_runtime::{
    VulcanPluginHostServices, VulcanPluginUiEventPublisher, VulcanPluginUiInvalidation,
};

/// Hold one real native finalizer callback until the test acknowledges its unfinished physical work.
/// 保持单个真实原生关闭回调，直至测试确认其尚未完成的物理工作。
struct HeldFinalizer {
    /// Entry is emitted from the actual native callback, never from a synthetic polling result.
    /// 进入信号由实际原生回调发出，绝不由模拟轮询结果发出。
    entered: mpsc::Sender<()>,
    /// One callback consumes the exact release acknowledgement.
    /// 单个回调消费精确释放确认。
    release: Mutex<mpsc::Receiver<()>>,
    /// Native waits are bounded by the established test observation setting.
    /// 原生等待受既有测试观测设置约束。
    wait: Duration,
}

impl VulcanPluginUiEventPublisher for HeldFinalizer {
    /// Hold the actual declared finalization `event`; return only after release or explicit observation timeout.
    /// 保持实际声明关闭 `event`；仅在释放或明确观测超时后返回。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        if event.topic != "original-retirement" {
            return Err(String::from("Unexpected original finalizer topic."));
        }
        self.entered.send(()).map_err(|error| error.to_string())?;
        self.release
            .lock()
            .map_err(|error| error.to_string())?
            .recv_timeout(self.wait)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

/// Write a known persistent Hook and native finalizer declaration into `fixture`; returns no value.
/// 将已知持久 Hook 及原生关闭声明写入 `fixture`；不返回值。
fn write_persistent_package(fixture: &Fixture) {
    // This package uses the established bundled manifest contract and a real fixed bridge.
    // 此插件包使用既有内置清单契约及真实固定桥接。
    let package = fixture.root.path().join("bundled/retirement-plugin");
    std::fs::create_dir_all(package.join("runtime")).expect("real Hook package");
    std::fs::write(package.join("dependencies.yaml"), "{}\n").expect("package dependencies");
    std::fs::write(
        package.join("plugin.yaml"),
        r#"schema_version: 1
id: retirement-plugin
version: 1.0.0
default_enabled: true
display:
  name: Retirement boundary
  capability: Real finalizer ownership
permissions: [ui_host_context]
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  hook_shutdown_function: shutdown
  max_active_sessions: 1
  hooks:
    - id: precheck
      point: conversation.turn.precheck
      mode: transform
      function: dispatch
      failure_policy: stop
"#,
    )
    .expect("real Hook manifest");
    std::fs::write(
        package.join("runtime/plugin.bundle.lua"),
        r#"
-- One physical module retains its exact Hook exports.
-- 单个物理模块保留其精确 Hook 导出。
local M = {}
-- Consume no arguments; return the declared successful empty business frame.
-- 不消费参数；返回声明的成功空业务帧。
function M.dispatch() return '{"ok":true,"result":{}}' end
-- Consume no arguments; hold the real native callback before returning cleanup success.
-- 不消费参数；在返回清理成功前保持真实原生回调。
function M.shutdown()
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='original-retirement',revision=1})
    return '{"ok":true,"result":{}}'
end
return M
"#,
    )
    .expect("real module source");
}

/// A real held finalizer forces the configured timeout, preserves native ownership, and later permits same-parent true closure.
/// 真实保持的关闭器强制触发配置超时、保留原生所有权，并允许同父级随后真实关闭。
/// No inputs or result; actual callback entry, resident accounting and poll_closed verify the boundary.
/// 无输入及返回值；实际回调进入、常驻记账及 poll_closed 验证边界。
#[test]
fn native_timeout_retains_original_owner_until_real_finalization_and_retry_join() {
    // All fixture files and manager services remain alive through actual retirement.
    // 全部夹具文件及管理器服务在实际退役期间保持存活。
    let fixture = Fixture::new();
    write_persistent_package(&fixture);
    // The broad test observation wait derives from the production default setting, independently of the deliberately short closing budget.
    // 较宽测试观测等待派生自生产默认设置，独立于有意缩短的关闭预算。
    let wait = Duration::from_millis(VulcanPluginRuntimeSettings::default().shutdown_timeout_ms);
    // Actual callback entry identifies physical finalization still owning its VM.
    // 实际回调进入标识仍拥有 VM 的物理关闭。
    let (entered_tx, entered_rx) = mpsc::channel();
    // The test controls acknowledgement while the native owner remains charged.
    // 原生所有者仍计费期间，测试控制确认。
    let (release_tx, release_rx) = mpsc::channel();
    fixture
        .manager
        .install_host_services(VulcanPluginHostServices {
            storage: None,
            vldb: None,
            ui_events: Some(Arc::new(HeldFinalizer {
                entered: entered_tx,
                release: Mutex::new(release_rx),
                wait,
            })),
        })
        .expect("real native finalizer service");
    // A test-specific setting makes timeout deterministic while the finalizer waits for explicit acknowledgement.
    // 测试专用设置使超时确定，同时关闭器等待明确确认。
    let settings = VulcanPluginRuntimeSettings {
        shutdown_timeout_ms: 25,
        ..VulcanPluginRuntimeSettings::default()
    };
    fixture.initialize(settings).expect("real native parent");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("real enabled catalog");
    // Tokio is used only for the existing public preparation path; actual closure observes Core workers synchronously.
    // Tokio 仅用于既有公开准备路径；实际关闭同步观测 Core 工作者。
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("prewarm observer");
    assert!(
        runtime
            .block_on(fixture.manager.prewarm_plugin_runtime("retirement-plugin"))
            .expect("real persistent Hook")
    );
    // Freeze the Core-issued ID before permanent closure.
    // 永久关闭前冻结 Core 签发 ID。
    let original = fixture.runtime_id();
    fixture.block_recovery();
    // This actual call requests cancellation and samples physical closure under the short configured budget.
    // 此实际调用请求取消，并在较短配置预算内采样物理关闭。
    let failed = close_original_plugin_runtime(&fixture.state);
    entered_rx
        .recv_timeout(wait)
        .expect("actual finalization callback entered");
    assert!(
        failed
            .expect_err("unfinished real finalizer must not report success")
            .contains("did not completely close within 25 ms")
    );
    assert!(
        !fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("unfinished physical ownership")
    );
    assert!(
        fixture
            .manager
            .formal_runtime_status()
            .expect("native status")
            .expect("original status")
            .resources
            .resident
            > 0
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
    assert!(crate::service_resume_after_failed_app_shutdown(&fixture.state).is_err());
    release_tx.send(()).expect("release real finalizer");
    // Wait for the actual original parent joins using the broad test budget, without changing its configured closing setting.
    // 使用较宽测试预算等待实际原父级线程退出，不变更其配置关闭设置。
    let deadline = std::time::Instant::now() + wait;
    loop {
        match close_original_plugin_runtime(&fixture.state) {
            Ok(()) => break,
            Err(error) if error.contains("did not completely close within 25 ms") => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "released original parent failed to join: {error}"
                );
            }
            Err(error) => panic!("unexpected original closing observation: {error}"),
        }
    }
    fixture
        .state
        .lock()
        .expect("retry gate")
        .service_lifecycle_state
        .begin_shutdown_drain();
    close_original_plugin_runtime(&fixture.state)
        .expect("same original owner retry after physical release");
    assert_eq!(fixture.runtime_id(), original);
    assert!(
        fixture
            .state
            .lock()
            .expect("permanent gate")
            .service_lifecycle_state
            .shutdown_recovery_blocked
    );
}
