//! Native code-semantic VCE RPC routing.
//! 原生代码语义 VCE RPC 路由。

use super::*;

/// Exact public RPC wire names owned only by the VCE router.
/// 仅由 VCE 路由持有的精确公共 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_get_vce_configuration_status",
    "service_get_vce_project_overview",
    "service_get_vce_project_activity",
    "service_update_vce_project_settings",
    "service_submit_vce_job",
    "service_get_vce_job",
    "service_cancel_vce_job",
    "service_list_vce_files",
    "service_get_vce_blob_detail",
    "service_list_vce_operations",
    "service_list_vce_integrity_issues",
    "service_plan_vce_maintenance",
    "service_execute_vce_maintenance",
];

/// Dispatch one VCE RPC without exposing storage or lifecycle internals to the browser.
/// 分发一项 VCE RPC，且不向浏览器暴露存储或生命周期内部实现。
///
/// # Parameters
/// 参数
/// `app` emits focused lifecycle events.
/// `app` 发送聚焦生命周期事件。
/// `state` owns current VCE runtime and persisted project state.
/// `state` 持有当前 VCE 运行时与持久化项目状态。
/// `method` is the exact wire method name.
/// `method` 是精确线协议方法名称。
/// `params` is normalized JSON containing the typed request object.
/// `params` 是包含类型化请求对象的规范 JSON。
///
/// # Returns
/// 返回值
/// A handled result for owned methods or `Unknown` for another domain.
/// 对所持有方法返回已处理结果，对其他领域返回 `Unknown`。
pub(super) async fn dispatch(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    let result = match method {
        "service_get_vce_configuration_status" => {
            rpc_json_result(service_get_vce_configuration_status(state))
        }
        "service_get_vce_project_overview" => rpc_json_result(
            service_get_vce_project_overview(state, rpc_required_param(params, "request")?).await,
        ),
        "service_get_vce_project_activity" => rpc_json_result(
            service_get_vce_project_activity(state, rpc_required_param(params, "request")?).await,
        ),
        "service_update_vce_project_settings" => rpc_json_result(
            service_update_vce_project_settings(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        "service_submit_vce_job" => rpc_json_result(
            service_submit_vce_job(state, rpc_required_param(params, "request")?).await,
        ),
        "service_get_vce_job" => rpc_json_result(
            service_get_vce_job(state, rpc_required_param(params, "request")?).await,
        ),
        "service_cancel_vce_job" => rpc_json_result(
            service_cancel_vce_job(state, rpc_required_param(params, "request")?).await,
        ),
        "service_list_vce_files" => rpc_json_result(
            service_list_vce_files(state, rpc_required_param(params, "request")?).await,
        ),
        "service_get_vce_blob_detail" => rpc_json_result(
            service_get_vce_blob_detail(state, rpc_required_param(params, "request")?).await,
        ),
        "service_list_vce_operations" => rpc_json_result(
            service_list_vce_operations(state, rpc_required_param(params, "request")?).await,
        ),
        "service_list_vce_integrity_issues" => rpc_json_result(
            service_list_vce_integrity_issues(state, rpc_required_param(params, "request")?).await,
        ),
        "service_plan_vce_maintenance" => rpc_json_result(
            service_plan_vce_maintenance(state, rpc_required_param(params, "request")?).await,
        ),
        "service_execute_vce_maintenance" => rpc_json_result(
            service_execute_vce_maintenance(app, state, rpc_required_param(params, "request")?)
                .await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
