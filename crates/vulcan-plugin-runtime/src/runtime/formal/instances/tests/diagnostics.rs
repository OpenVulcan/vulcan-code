//! Persistent instances expose genuine native ownership without claiming a conversation controller.
//! 持久实例暴露真实原生所有权，不声称具有会话控制器。

use super::*;
use crate::runtime::formal::tests::diagnostics::observed;

/// Real Hook state is distinct from pinned-tool sessions, and locked original evidence returns explicit Busy.
/// 真实钩子状态与固定工具会话区分，原证据已锁时返回明确忙碌。
#[test]
fn formal_hook_resource_diagnostics_preserve_persistent_ownership_and_lock_boundary() {
    // Existing fixture uses production Hook preparation and the exact declared export.
    // 既有夹具使用生产钩子准备及精确声明导出。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default(), 1000);
    // Observer executor owns all existing production supervision until this test ends.
    // 观测执行器拥有全部既有生产监督，直至此测试结束。
    let runtime = executor();
    runtime.block_on(async {
        fixture
            .manager
            .invoke_hook(
                &fixture.hook(),
                fixture.invocation(json!({"mode":"ok"}), None),
            )
            .await
            .expect("real Hook call");
        // Parent identity is the one already used by the real Hook invocation.
        // 父级身份就是实际钩子调用已经使用的身份。
        let owner = fixture.owner();
        // Clone the actual original instance under its registry before inspecting the state lock.
        // 检查状态锁前，在其注册表下克隆实际原实例。
        let instance = owner
            .hooks
            .lock()
            .expect("real Hook registry")
            .get("hook-plugin")
            .cloned()
            .expect("real Hook instance");
        // The diagnostic reader must never wait for an unavailable per-instance state guard.
        // 诊断读取器绝不等待不可用的独立实例状态守卫。
        let guard = instance.lock().expect("hold original evidence");
        assert_eq!(
            fixture
                .manager
                .formal_plugin_resource_diagnostics("hook-plugin", None)
                .expect_err("unavailable original evidence")
                .message,
            "Plugin resource diagnostics are busy."
        );
        drop(guard);
        // A normal completed Hook leaves the persistent native instance physically owned.
        // 正常完成的钩子使持久原生实例仍具有物理所有权。
        let diagnostic = observed(&fixture.manager, "hook-plugin", None).await;
        assert_eq!(diagnostic.pinned_session_count, 0);
        // Registry category prevents its underlying core session from impersonating a tool-pinned session.
        // 注册表类别阻止底层核心会话冒充固定工具会话。
        let row = diagnostic
            .resources
            .iter()
            .find(|row| row.resource_kind == VulcanPluginResourceKind::HookRuntime)
            .expect("expected persistent Hook may have changed");
        assert_eq!(row.resource_id, "hook-plugin");
        assert!(row.owner_session_id.is_none());
        assert!(row.core_session_id.is_some());
        assert!(row.original_pool_id.is_some());
        assert!(row.is_drain_blocking);
        assert!(!row.retired);
        fixture
            .manager
            .drain_plugin_sessions("hook-plugin")
            .await
            .expect("retire original Hook");
    });
}
