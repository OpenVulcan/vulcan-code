//! Process-local health state for external VMM endpoints.
//! 外部托管 VMM 端点的进程内健康状态。

use std::sync::{Mutex, OnceLock};

use vulcan_service_api::VmmRuntimeSummary;

/// Latest explicit health probe retained for one external VMM endpoint.
/// 为单个外部 VMM 端点保留的最近一次显式健康探测。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ExternalVmmHealthProbe {
    /// Exact gRPC target that produced this probe result.
    /// 产生当前探测结果的精确 gRPC 目标地址。
    grpc_target: String,
    /// Controlled diagnostic captured when the explicit probe failed.
    /// 显式探测失败时捕获的受控诊断。
    error: Option<String>,
}

/// Returns the process-local external VMM health probe store.
/// 返回进程内外部 VMM 健康探测存储。
fn external_vmm_health_probe_store() -> &'static Mutex<Option<ExternalVmmHealthProbe>> {
    static STORE: OnceLock<Mutex<Option<ExternalVmmHealthProbe>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(None))
}

/// Normalizes one gRPC target for exact process-local probe matching.
/// 规范化单个 gRPC 目标地址，用于精确匹配进程内探测结果。
///
/// # Parameters
/// 参数
/// `grpc_target` is the endpoint supplied by the active VMM configuration.
/// `grpc_target` 是当前 VMM 配置提供的端点地址。
///
/// # Returns
/// 返回值
/// The trimmed endpoint used as the probe identity.
/// 返回作为探测身份使用的去首尾空白端点。
fn normalize_external_vmm_grpc_target(grpc_target: &str) -> String {
    grpc_target.trim().to_string()
}

/// Returns whether the exact external endpoint has a current successful health probe.
/// 返回准确外部端点当前是否拥有成功的健康探测事实。
///
/// # Parameters
/// 参数
/// `grpc_target` is the endpoint from the current authoritative configuration.
/// `grpc_target` 是当前权威配置中的端点。
/// `scope_configured` reports whether the current effective service scope is complete.
/// `scope_configured` 表示当前最终服务作用域是否已完整配置。
///
/// # Returns
/// 返回值
/// `true` only when configuration is complete and the matching endpoint is currently healthy.
/// 仅当配置完整且匹配端点当前健康时返回 `true`。
pub(super) fn external_vmm_service_ready(grpc_target: &str, scope_configured: bool) -> bool {
    if !scope_configured {
        return false;
    }
    let target = normalize_external_vmm_grpc_target(grpc_target);
    external_vmm_health_probe_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .is_some_and(|probe| probe.grpc_target == target && probe.error.is_none())
}

/// Records a successful explicit health probe for one external VMM endpoint.
/// 记录单个外部 VMM 端点的显式健康探测成功结果。
///
/// # Parameters
/// 参数
/// `grpc_target` is the exact endpoint that answered the probe.
/// `grpc_target` 是响应探测的精确端点地址。
///
/// # Returns
/// 返回值
/// This function returns no value after replacing the process-local probe.
/// 替换进程内探测结果后不返回值。
pub(super) fn record_external_vmm_health_success(grpc_target: &str) {
    let mut store = external_vmm_health_probe_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *store = Some(ExternalVmmHealthProbe {
        grpc_target: normalize_external_vmm_grpc_target(grpc_target),
        error: None,
    });
}

/// Records a failed explicit health probe for one external VMM endpoint.
/// 记录单个外部 VMM 端点的显式健康探测失败结果。
///
/// # Parameters
/// 参数
/// `grpc_target` is the exact endpoint that failed the probe.
/// `grpc_target` 是探测失败的精确端点地址。
/// `error` is the controlled failure diagnostic returned by the transport.
/// `error` 是传输层返回的受控失败诊断。
///
/// # Returns
/// 返回值
/// This function returns no value after replacing the process-local probe.
/// 替换进程内探测结果后不返回值。
pub(super) fn record_external_vmm_health_failure(grpc_target: &str, error: &str) {
    let mut store = external_vmm_health_probe_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *store = Some(ExternalVmmHealthProbe {
        grpc_target: normalize_external_vmm_grpc_target(grpc_target),
        error: Some(error.trim().to_string()),
    });
}

/// Invalidates the process-local external VMM health result after configuration changes.
/// 在配置变化后使进程内外部 VMM 健康结果失效。
///
/// # Returns
/// 返回值
/// This function returns no value after clearing the retained probe.
/// 清除保留的探测结果后不返回值。
pub(super) fn clear_external_vmm_health_probe() {
    let mut store = external_vmm_health_probe_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *store = None;
}

/// Applies the matching external health probe to one VMM runtime summary.
/// 将匹配的外部健康探测结果应用到单个 VMM 运行摘要。
///
/// # Parameters
/// 参数
/// `grpc_target` is the currently effective external VMM endpoint.
/// `grpc_target` 是当前生效的外部 VMM 端点。
/// `scope_configured` reports whether the effective identity and project scope are complete.
/// `scope_configured` 表示最终身份与项目作用域配置是否完整。
/// `precheck_enabled` reports whether recall injection is enabled for the effective scope.
/// `precheck_enabled` 表示最终作用域是否启用了召回注入。
/// `postaction_enabled` reports whether memory writeback is enabled for the effective scope.
/// `postaction_enabled` 表示最终作用域是否启用了记忆回写。
/// `runtime` is the runtime summary being projected to the workbench.
/// `runtime` 是正在投影到工作台的运行摘要。
///
/// # Returns
/// 返回值
/// This function returns no value after applying a matching in-memory result.
/// 应用匹配的内存结果后不返回值。
pub(super) fn apply_external_vmm_health_probe(
    grpc_target: &str,
    scope_configured: bool,
    precheck_enabled: bool,
    postaction_enabled: bool,
    runtime: &mut VmmRuntimeSummary,
) {
    let target = normalize_external_vmm_grpc_target(grpc_target);
    let store = external_vmm_health_probe_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(probe) = store.as_ref().filter(|probe| probe.grpc_target == target) else {
        return;
    };
    runtime.process_ready = probe.error.is_none();
    runtime.scope_ready = runtime.process_ready && scope_configured;
    runtime.precheck_ready = runtime.scope_ready && precheck_enabled;
    runtime.postaction_ready = runtime.scope_ready && postaction_enabled;
    runtime.profile_instruction_ready = runtime.scope_ready;
    runtime.rerank_ready = runtime.scope_ready;
    runtime.backend_error = probe.error.clone();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies endpoint matching and controlled failure diagnostics for external probes.
    /// 验证外部探测的端点匹配与受控失败诊断。
    #[test]
    fn external_health_probe_tracks_exact_endpoint_and_failure() {
        clear_external_vmm_health_probe();
        record_external_vmm_health_success(" 127.0.0.1:17625 ");

        assert!(external_vmm_service_ready("127.0.0.1:17625", true));
        assert!(!external_vmm_service_ready("127.0.0.1:17625", false));
        assert!(!external_vmm_service_ready("127.0.0.1:17635", true));

        let mut matching_runtime = VmmRuntimeSummary::default();
        apply_external_vmm_health_probe("127.0.0.1:17625", true, true, true, &mut matching_runtime);
        assert!(matching_runtime.process_ready);
        assert!(matching_runtime.scope_ready);
        assert!(matching_runtime.precheck_ready);
        assert!(matching_runtime.postaction_ready);
        assert!(matching_runtime.profile_instruction_ready);
        assert!(matching_runtime.rerank_ready);

        let mut incomplete_scope_runtime = VmmRuntimeSummary::default();
        apply_external_vmm_health_probe(
            "127.0.0.1:17625",
            false,
            true,
            true,
            &mut incomplete_scope_runtime,
        );
        assert!(incomplete_scope_runtime.process_ready);
        assert!(!incomplete_scope_runtime.scope_ready);
        assert!(!incomplete_scope_runtime.precheck_ready);

        let mut different_runtime = VmmRuntimeSummary::default();
        apply_external_vmm_health_probe(
            "127.0.0.1:17635",
            true,
            true,
            true,
            &mut different_runtime,
        );
        assert!(!different_runtime.process_ready);
        assert!(!different_runtime.scope_ready);

        record_external_vmm_health_failure("127.0.0.1:17625", " connection refused ");
        assert!(!external_vmm_service_ready("127.0.0.1:17625", true));

        let mut runtime = VmmRuntimeSummary::default();
        apply_external_vmm_health_probe("127.0.0.1:17625", true, true, true, &mut runtime);
        assert!(!runtime.process_ready);
        assert!(!runtime.scope_ready);
        assert_eq!(runtime.backend_error.as_deref(), Some("connection refused"));
        clear_external_vmm_health_probe();
    }
}
