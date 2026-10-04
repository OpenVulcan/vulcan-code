//! Persistent Hook and UI ownership sampled independently of initialization and operation gates.
//! 独立于初始化及操作门采样的持久钩子及界面所有权。

use super::*;
use crate::runtime::formal::diagnostics::{
    VulcanPluginResourceDiagnostic, VulcanPluginResourceErrorDiagnostic, VulcanPluginResourceKind,
    diagnostic_lock, operation,
};

/// Clone owner persistent entries matching plugin_id/generation, returning safe lifecycle metadata.
/// 克隆 owner 中匹配 plugin_id/generation 的持久条目，返回安全生命周期元数据。
/// Registry locks are released before state reads; neither initialization nor operation gates are acquired.
/// 状态读取前释放注册表锁；不获取初始化门或操作门。
pub(in crate::runtime::formal) fn sample(
    owner: &ManagedFormalRuntime,
    plugin_id: &str,
    generation: Option<u64>,
) -> Result<Vec<VulcanPluginResourceDiagnostic>, VulcanPluginRuntimeError> {
    // Persistent registries are bounded by the existing plugin/purpose admission paths.
    // 持久注册表由既有插件及用途准入路径约束。
    let mut rows = Vec::new();
    for kind in [
        VulcanPluginPersistentRuntimeKind::Hook,
        VulcanPluginPersistentRuntimeKind::UiAction,
    ] {
        // Retain the exact registered Arc after releasing this kind's registry.
        // 释放此类别注册表后保留精确已注册 Arc。
        let instances: Vec<_> = diagnostic_lock(owner.instances(kind))?
            .iter()
            .filter(|(id, instance)| {
                id.as_str() == plugin_id
                    && generation.is_none_or(|value| instance.plugin.generation == value)
            })
            .map(|(id, instance)| (id.clone(), Arc::clone(instance)))
            .collect();
        for (id, instance) in instances {
            // State serialization prevents native session/operation forgetting during observation.
            // 状态串行化阻止观测期间原生会话及操作被遗忘。
            let state = diagnostic_lock(&instance.state)?;
            // Registry kind identifies the real persistent ownership category.
            // 注册表类别标识真实持久所有权类别。
            let resource_kind = match kind {
                VulcanPluginPersistentRuntimeKind::Hook => VulcanPluginResourceKind::HookRuntime,
                VulcanPluginPersistentRuntimeKind::UiAction => {
                    VulcanPluginResourceKind::UiActionRuntime
                }
            };
            // Persistent ownership has no controller-session field in its actual type.
            // 持久所有权的实际类型没有控制器会话字段。
            let mut row = VulcanPluginResourceDiagnostic::new(
                resource_kind,
                id,
                instance.plugin.generation,
                None,
            );
            row.original_pool_id = state.pool.clone();
            row.core_session_id = state.session.clone();
            row.preparing = state.preparing;
            row.closing = instance.closing.load(Ordering::Acquire);
            row.retired = state.retired;
            row.is_drain_blocking = !state.retired || state.preparing;
            row.cleanup_recovery = !state.monitored && !state.retired;
            if !state.session_forgotten
                && let Some(id) = &state.session
            {
                row.session(owner.host.runtime().session(id).map_err(core_error)?);
            }
            if let Some(handle) = &state.initialization {
                if !state.initialization_forgotten {
                    row.operations
                        .push(operation(&handle.snapshot().map_err(core_error)?));
                }
            }
            for retained in state.operations.values() {
                row.operations
                    .push(operation(&retained.handle.snapshot().map_err(core_error)?));
            }
            if let Some(finalization) = &state.finalization {
                row.finalization_operation_id = Some(finalization.operation_id.clone());
                row.operations.push(operation(finalization));
            }
            if state.cleanup_error.is_some() {
                row.errors
                    .push(VulcanPluginResourceErrorDiagnostic::new("cleanup", None));
            }
            if matches!(state.prepared, Some(Err(_))) {
                row.errors.push(VulcanPluginResourceErrorDiagnostic::new(
                    "preparation",
                    None,
                ));
            }
            if state.target_receipt_error.is_some() {
                row.errors.push(VulcanPluginResourceErrorDiagnostic::new(
                    "target_receipt",
                    None,
                ));
            }
            rows.push(row);
        }
    }
    Ok(rows)
}
