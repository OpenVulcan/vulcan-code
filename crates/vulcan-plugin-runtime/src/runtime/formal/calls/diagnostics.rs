//! Real invocation ownership remains separate from fixed-tool sessions.
//! 真实调用所有权与固定工具会话保持区分。

use super::*;
use crate::runtime::formal::diagnostics::{
    VulcanPluginResourceDiagnostic, VulcanPluginResourceErrorDiagnostic, VulcanPluginResourceKind,
    diagnostic_lock, operation,
};

/// Clone owner calls matching plugin_id/generation and return only safe original metadata.
/// 克隆 owner 中匹配 plugin_id/generation 的调用，仅返回安全原元数据。
/// Per-call state protects operation acknowledgement; no preparation input or business result is projected.
/// 独立调用状态保护操作确认；不投影准备输入或业务结果。
pub(in crate::runtime::formal) fn sample(
    owner: &ManagedFormalRuntime,
    plugin_id: &str,
    generation: Option<u64>,
) -> Result<Vec<VulcanPluginResourceDiagnostic>, VulcanPluginRuntimeError> {
    // Release registry ownership before any original call or native observation.
    // 在任何原调用或原生观测前释放注册表所有权。
    let calls: Vec<_> = diagnostic_lock(&owner.calls)?
        .values()
        .filter(|call| {
            call.plugin.id == plugin_id
                && generation.is_none_or(|value| call.plugin.generation == value)
        })
        .cloned()
        .collect();
    calls
        .iter()
        .map(|call| {
            // Retained evidence cannot be acknowledged while its synchronous snapshot is read.
            // 同步读取快照期间保留证据不能被确认。
            let state = diagnostic_lock(&call.state)?;
            // Internal owner variants explicitly have no conversation identity.
            // 内部所有者变体明确没有会话身份。
            let mut row = VulcanPluginResourceDiagnostic::new(
                VulcanPluginResourceKind::FormalCall,
                call.id.clone(),
                call.plugin.generation,
                call.ownership.controller_id().map(str::to_owned),
            );
            row.original_pool_id = state.pool.clone();
            row.preparing = state.preparing;
            row.closing = call.cancelled.load(Ordering::Acquire);
            row.retired = state.retired;
            // Formal drain waits for retirement and the original supervisor's completion.
            // 正式排空等待退役及原监督器完成。
            row.is_drain_blocking = !state.retired || state.supervised || state.preparing;
            row.cleanup_recovery = !state.supervised && !state.retired;
            if let Some(handle) = &state.operation {
                // Actual core operation status is copied without its value or host effects.
                // 复制实际核心操作状态，不复制其值或宿主副作用。
                let snapshot = handle.snapshot().map_err(core_error)?;
                if !snapshot.phase.is_terminal() {
                    row.active_operation_id = Some(snapshot.operation_id.clone());
                }
                if snapshot.finalization.is_some() {
                    row.finalization_operation_id = Some(snapshot.operation_id.clone());
                }
                row.operations.push(operation(&snapshot));
            }
            if let Some(initialized) = &state.initialized {
                row.core_session_id = Some(initialized.session_id().to_owned());
                if !initialized.session_forgotten() {
                    row.session(
                        owner
                            .host
                            .runtime()
                            .session(initialized.session_id())
                            .map_err(core_error)?,
                    );
                }
                if let Some(handle) = initialized.initialization_operation() {
                    row.operations
                        .push(operation(&handle.snapshot().map_err(core_error)?));
                }
            }
            if state.cleanup_error.is_some() {
                row.errors
                    .push(VulcanPluginResourceErrorDiagnostic::new("cleanup", None));
            }
            if matches!(state.result, Some(Err(_))) {
                row.errors
                    .push(VulcanPluginResourceErrorDiagnostic::new("result", None));
            }
            Ok(row)
        })
        .collect()
}
