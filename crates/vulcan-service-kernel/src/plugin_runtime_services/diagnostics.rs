//! Read-only diagnostics retain the original native parent and never borrow a running mutation.
//! 只读诊断保留原生父级，绝不借用正在运行的变更。

use super::*;
use crate::skills_services::LocalPluginUninstallDrainDiagnostics;
use vulcan_plugin_runtime::{
    VulcanPluginPredecessorDrainDiagnostics, VulcanPluginResourceDiagnostics,
};

/// Validate runtime_id against manager's existing parent; return an explicit stale or unattached error.
/// 按 manager 的既有父级校验 runtime_id；返回明确的过期或未附加错误。
/// This read does not attach, replace, cancel or advance the original runtime.
/// 此读取不附加、替换、取消或推进原运行时。
pub(crate) fn require_runtime_identity(
    manager: &VulcanPluginRuntimeManager,
    runtime_id: &str,
) -> Result<(), VulcanPluginRuntimeError> {
    // The manager never replaces its attached parent; validating this immutable ID needs no mutation lock.
    // 管理器绝不替换已附加父级；校验此不可变身份无需变更锁。
    let current = manager.formal_runtime_status()?.ok_or_else(|| {
        VulcanPluginRuntimeError::new("Formal plugin runtime is not attached for diagnostics.")
    })?;
    if current.runtime_id != runtime_id {
        return Err(VulcanPluginRuntimeError::new(
            "Plugin diagnostic runtime identity is stale.",
        ));
    }
    Ok(())
}

/// Read plugin_id's retained resources from state's exact runtime_id; return metadata or a native error.
/// 从 state 的精确 runtime_id 读取 plugin_id 的保留资源；返回元数据或原生错误。
/// All retained generations remain explicit; this general read does not associate them with a transaction.
/// 所有保留代次保持明确；此通用读取不将它们关联到事务。
pub(crate) async fn resources(
    state: SharedServiceBridgeState,
    runtime_id: String,
    plugin_id: String,
) -> Result<Option<VulcanPluginResourceDiagnostics>, String> {
    run_plugin_runtime_control(state, move |manager| {
        require_runtime_identity(manager, &runtime_id)?;
        manager.formal_plugin_resource_diagnostics(&plugin_id, None)
    })
    .await
}

/// Read request_id's real predecessor drain for plugin_id and runtime_id; return None when no live grant exists.
/// 为 plugin_id 及 runtime_id 读取 request_id 的真实前驱排空；没有实时授权时返回空值。
/// None is unavailable live evidence, never certification that historical work or physical cleanup succeeded.
/// 空值表示实时证据不可用，绝不证明历史工作或物理清理成功。
pub(crate) async fn update_drain(
    state: SharedServiceBridgeState,
    runtime_id: String,
    plugin_id: String,
    request_id: uuid::Uuid,
) -> Result<Option<VulcanPluginPredecessorDrainDiagnostics>, String> {
    run_plugin_runtime_control(state, move |manager| {
        require_runtime_identity(manager, &runtime_id)?;
        manager.activation_predecessor_drain_diagnostics(&plugin_id, request_id)
    })
    .await
}

/// Read plugin_id/request_id's actual retained removal in state; validate runtime_id without waiting for execution.
/// 在 state 中读取 plugin_id/request_id 的实际保留移除；校验 runtime_id 而不等待执行。
/// Return the original owner's metadata; a missing original is an explicit error and is never recreated.
/// 返回原所有者元数据；原实例缺失时明确报错，绝不重建。
pub(crate) async fn uninstall_drain(
    state: SharedServiceBridgeState,
    runtime_id: String,
    plugin_id: String,
    request_id: uuid::Uuid,
) -> Result<LocalPluginUninstallDrainDiagnostics, String> {
    // Clone the existing registry under service exclusion, then release it before any native reads.
    // 在服务互斥内克隆既有注册表，然后在任何原生读取之前释放它。
    let registry = std::sync::Arc::clone(
        &state
            .lock()
            .map_err(|error| error.to_string())?
            .pending_local_plugin_uninstalls,
    );
    tokio::task::spawn_blocking(move || {
        registry.drain_diagnostics(&runtime_id, &plugin_id, request_id)
    })
    .await
    .map_err(|error| format!("Plugin uninstall diagnostic task failed: {error}"))?
}
