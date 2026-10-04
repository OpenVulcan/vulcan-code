//! Original pinned-tool ownership without controller enumeration or lifecycle mutation.
//! 原固定工具所有权，不枚举控制器或修改生命周期。

use super::*;
use crate::runtime::formal::diagnostics::{
    VulcanPluginResourceDiagnostic, VulcanPluginResourceErrorDiagnostic, VulcanPluginResourceKind,
    diagnostic_lock, operation,
};

/// Clone owner registry entries selected by plugin_id/generation, then return original safe state observations.
/// 克隆 owner 注册表中由 plugin_id/generation 选中的条目，然后返回原安全状态观测。
/// Registry locks never span per-session evidence reads; retired history remains explicitly nonblocking.
/// 注册表锁绝不跨独立会话证据读取；退役历史明确保持不阻塞。
pub(in crate::runtime::formal) fn sample(
    owner: &ManagedFormalRuntime,
    plugin_id: &str,
    generation: Option<u64>,
) -> Result<Vec<VulcanPluginResourceDiagnostic>, VulcanPluginRuntimeError> {
    // Registry admission already bounds this exact retained collection.
    // 注册表准入已经约束此精确保留集合。
    let sessions: Vec<_> = diagnostic_lock(&owner.pinned)?
        .values()
        .filter(|session| {
            session.plugin.id == plugin_id
                && generation.is_none_or(|value| session.plugin.generation == value)
        })
        .cloned()
        .collect();
    sessions
        .iter()
        .map(|session| {
            // The same short evidence lock protects native forgetting against this synchronous observation.
            // 相同短证据锁保护原生遗忘与此同步观测之间的关系。
            let state = diagnostic_lock(&session.state)?;
            // Identity comes only from the immutable registered owner.
            // 身份仅来自不可变已注册所有者。
            let mut row = VulcanPluginResourceDiagnostic::new(
                VulcanPluginResourceKind::PinnedSession,
                session.id.clone(),
                session.plugin.generation,
                Some(session.owner.clone()),
            );
            row.original_pool_id = state.pool.clone();
            row.core_session_id = state.core_session.clone();
            row.preparing = state.preparing;
            row.closing = session.closing.load(Ordering::Acquire);
            row.retired = state.retired;
            row.is_drain_blocking = !state.retired || state.preparing;
            row.cleanup_recovery = !state.monitored && !state.retired;
            if !state.session_forgotten
                && let Some(id) = &state.core_session
            {
                row.session(owner.host.runtime().session(id).map_err(core_error)?);
            }
            if let Some(finalization) = &state.finalization {
                row.finalization_operation_id = Some(finalization.operation_id.clone());
                row.operations.push(operation(finalization));
            }
            if let Some(initialization) = &state.initialization {
                row.operations
                    .push(operation(&initialization.snapshot().map_err(core_error)?));
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
            Ok(row)
        })
        .collect()
}
