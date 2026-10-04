//! Authenticated host management uses the existing plugin owner without blocking async workers.
//! 已认证宿主管理使用既有插件所有者，不阻塞异步工作线程。

use crate::{SharedServiceBridgeState, service_vulcan_plugin_runtime};
use vulcan_plugin_runtime::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};

pub(crate) mod automatic_prewarm;
pub(crate) mod diagnostics;
pub(crate) mod prewarm;

/// Run operation against state's exact manager on the blocking executor; return its original result.
/// 在阻塞执行器上针对 state 的精确管理器运行 operation；返回其原始结果。
/// HTTP observer cancellation does not undo an accepted native or persistent mutation; receipts remain queryable.
/// HTTP 观测取消不撤销已接纳的原生或持久变更；回执仍可查询。
pub(crate) async fn run_plugin_runtime_control<T, F>(
    state: SharedServiceBridgeState,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&VulcanPluginRuntimeManager) -> Result<T, VulcanPluginRuntimeError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        // Clone only the manager handle; its state lock is released before disk I/O or native control.
        // 仅克隆管理器句柄；在磁盘操作或原生控制前释放服务状态锁。
        let runtime = service_vulcan_plugin_runtime(&state)?;
        operation(&runtime).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Plugin runtime control task failed: {error}"))?
}
