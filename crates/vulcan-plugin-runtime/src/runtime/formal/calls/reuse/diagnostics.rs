//! Safe domain metadata is cloned under its original registry without consulting business values.
//! 安全域元数据在原注册表下克隆，不读取业务值。

use super::*;
use crate::runtime::formal::diagnostics::{
    VulcanPluginResourceDiagnostic, VulcanPluginResourceErrorDiagnostic, VulcanPluginResourceKind,
    diagnostic_lock,
};

/// Read owner domains matching plugin_id/generation and return only existing metadata ownership.
/// 读取 owner 中匹配 plugin_id/generation 的域，仅返回既有元数据所有权。
/// No native lookup occurs under the registry; physical retirement excludes terminal failure-only history.
/// 注册表内不进行原生查询；物理退役排除仅有终态失败的历史。
pub(in crate::runtime::formal) fn sample(
    owner: &ManagedFormalRuntime,
    plugin_id: &str,
    generation: Option<u64>,
) -> Result<Vec<VulcanPluginResourceDiagnostic>, VulcanPluginRuntimeError> {
    // The existing registry owns all facts needed here; no mutable domain or native record is copied.
    // 既有注册表拥有此处所需全部事实；不复制可变域或原生记录。
    let registry = diagnostic_lock(&owner.reusable)?;
    Ok(registry
        .domains
        .iter()
        .filter(|(_, domain)| {
            domain.plugin_id == plugin_id
                && generation.is_none_or(|value| domain.key.generation == value)
        })
        .map(|(id, domain)| {
            // Whitelist excludes workspace, configuration hashes, capabilities and package handles.
            // 白名单排除工作区、配置哈希、能力及包句柄。
            let mut row = VulcanPluginResourceDiagnostic::new(
                VulcanPluginResourceKind::ReusableDomain,
                id.clone(),
                domain.key.generation,
                Some(domain.key.controller.clone()),
            );
            row.original_pool_id = domain.pool.clone();
            row.preparing = domain.preparing;
            row.closing = domain.closing;
            row.retired = domain.pool_retired;
            // Match maintenance's actual physical ownership; failures alone retain history, never a live VM.
            // 匹配维护的实际物理所有权；仅有失败保留历史，绝不表示实时 VM。
            row.is_drain_blocking = domain.preparing
                || domain.borrowers > 0
                || (domain.pool.is_some() && !domain.pool_retired)
                || domain.pending_finalizations;
            row.cleanup_recovery =
                row.is_drain_blocking && !owner.reuse_observer.load(Ordering::Acquire);
            row.borrowers = Some(domain.borrowers);
            row.pending_finalizations = domain.pending_finalizations;
            row.finalization_operation_ids = domain.failures.keys().cloned().collect();
            if !domain.failures.is_empty() {
                row.errors.push(VulcanPluginResourceErrorDiagnostic::new(
                    "finalization",
                    None,
                ));
            }
            if registry.observer_error.is_some() {
                row.errors
                    .push(VulcanPluginResourceErrorDiagnostic::new("observer", None));
            }
            row
        })
        .collect())
}
