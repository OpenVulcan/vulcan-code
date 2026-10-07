//! Formal plugin administration belongs to the authenticated host RPC boundary.
//! 正式插件管理属于已认证宿主 RPC 边界。

use super::*;
use crate::plugin_runtime_services::diagnostics;
use crate::plugin_runtime_services::run_plugin_runtime_control;
use serde::{Deserialize, de::DeserializeOwned};
use vulcan_plugin_runtime::{
    VulcanPluginCapacityControlRequest, VulcanPluginCapacityPolicyUpdate,
    VulcanPluginCapacityRegistrationRequest, VulcanPluginRuntimeManager,
};

mod prewarm;
mod prewarm_policy;

/// Exact wire names have one domain owner; plugin business actions do not dispatch these methods.
/// 精确线协议名称仅有一个领域所有者；插件业务 Action 不分发这些方法。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_plugin_runtime_status",
    "service_plugin_resource_diagnostics",
    "service_plugin_update_drain_diagnostics",
    "service_plugin_uninstall_drain_diagnostics",
    "service_plugin_capacity_catalog",
    "service_plugin_capacity_registration_status",
    "service_register_plugin_capacity",
    "service_control_plugin_capacity",
    "service_begin_plugin_capacity_policy_update",
    "service_plugin_prewarm_targets",
    "service_prepare_plugin_prewarm",
    "service_plugin_prewarm_status",
    "service_start_plugin_prewarm",
    "service_cancel_plugin_prewarm",
    "service_recover_plugin_prewarm_cleanup",
    "service_forget_plugin_prewarm",
    "service_plugin_prewarm_policy_status",
    "service_begin_plugin_prewarm_policy_update",
];

/// Empty reads reject accidental mutation parameters instead of silently ignoring them.
/// 空参数读取拒绝误传变更参数，而非静默忽略。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyParams {}

/// Exactly one JSON object request crosses the management submission boundary.
/// 恰好单个 JSON 对象请求跨越管理提交边界。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestParams {
    /// Full original request retains caller identity and predecessor tokens.
    /// 完整原始请求保留调用方身份及前驱令牌。
    request: Value,
}

/// Runtime-local lookup selects one retained registration receipt.
/// 运行时内查找选择单个保留登记回执。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistrationParams {
    /// Original native parent must still own the receipt.
    /// 原始原生父级必须仍拥有回执。
    runtime_id: String,
    /// Caller-issued UUID is retained only while the original capacity exists.
    /// 调用方签发 UUID 仅在原容量存在时保留。
    registration_id: String,
}

/// An authenticated read selects one exact existing parent and plugin namespace.
/// 已认证读取选择单个精确既有父级及插件命名空间。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResourceDiagnosticParams {
    /// Original parent identity rejects queries accidentally retained across a runtime restart.
    /// 原父级身份拒绝运行时重启后误保留的查询。
    runtime_id: String,
    /// Exact plugin spelling is retained without changing catalog selection.
    /// 保留精确插件拼写，不改变目录选择。
    plugin_id: String,
}

/// Original mutation identity selects live diagnostic ownership without granting recovery or execution.
/// 原变更身份选择实时诊断所有权，不授予恢复或执行权限。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DrainDiagnosticParams {
    /// Existing native parent must match the page's retained identity.
    /// 既有原生父级必须匹配页面保留身份。
    runtime_id: String,
    /// Namespace must match the same original request, never the latest selected plugin.
    /// 命名空间必须匹配相同原请求，绝不使用最新已选插件。
    plugin_id: String,
    /// UUID syntax is validated before any service or registry access.
    /// 在任何服务或注册表访问前校验 UUID 语法。
    request_id: uuid::Uuid,
}

/// Require nonempty runtime_id and plugin_id; return a parameter error without normalizing original identities.
/// 要求 runtime_id 及 plugin_id 非空；返回参数错误，不规范化原身份。
fn require_diagnostic_names(runtime_id: &str, plugin_id: &str) -> Result<(), WebuiServiceRpcError> {
    if runtime_id.is_empty() || plugin_id.is_empty() {
        return Err(WebuiServiceRpcError::InvalidParams(
            "Plugin diagnostics require nonempty original runtimeId and pluginId.".into(),
        ));
    }
    Ok(())
}

/// Decode params using its exact serde contract; return a canonical parameter error before accessing state.
/// 使用精确 Serde 契约解码 params；在访问状态前返回标准参数错误。
fn parse_params<T: DeserializeOwned>(params: &Value) -> Result<T, WebuiServiceRpcError> {
    if !params.is_object() {
        return Err(WebuiServiceRpcError::InvalidParams(
            "Plugin runtime parameters must be a JSON object.".into(),
        ));
    }
    serde_json::from_value(params.clone())
        .map_err(|error| WebuiServiceRpcError::InvalidParams(error.to_string()))
}

/// Dispatch method against state's existing owner with strict params; return handled JSON or another-domain marker.
/// 使用严格 params，针对 state 既有所有者分发 method；返回已处理 JSON 或其他领域标记。
/// Native identities and durable checkpoints are validated atomically by the manager, never in a detached preflight.
/// 原生身份及持久检查点由管理器原子校验，绝不通过分离的预检处理。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    // Preserve service errors as handled results so later routers cannot reinterpret a failed control operation.
    // 将服务错误保留为已处理结果，避免后续路由重新解释失败的控制操作。
    let result = match method {
        "service_plugin_resource_diagnostics" => {
            // Parse the exact read contract before resolving the retained runtime.
            // 在解析保留运行时之前，解码精确读取契约。
            let request: ResourceDiagnosticParams = parse_params(params)?;
            require_diagnostic_names(&request.runtime_id, &request.plugin_id)?;
            rpc_json_result(
                diagnostics::resources(state, request.runtime_id, request.plugin_id).await,
            )
        }
        "service_plugin_update_drain_diagnostics" => {
            // The original request is a read identity, never permission to start another drain.
            // 原请求为读取身份，绝不是启动另一次排空的权限。
            let request: DrainDiagnosticParams = parse_params(params)?;
            require_diagnostic_names(&request.runtime_id, &request.plugin_id)?;
            rpc_json_result(
                diagnostics::update_drain(
                    state,
                    request.runtime_id,
                    request.plugin_id,
                    request.request_id,
                )
                .await,
            )
        }
        "service_plugin_uninstall_drain_diagnostics" => {
            // Uninstall diagnostics read the same actual owner even while its resource borrow is busy.
            // 即使资源借用忙碌，卸载诊断仍读取相同实际所有者。
            let request: DrainDiagnosticParams = parse_params(params)?;
            require_diagnostic_names(&request.runtime_id, &request.plugin_id)?;
            rpc_json_result(
                diagnostics::uninstall_drain(
                    state,
                    request.runtime_id,
                    request.plugin_id,
                    request.request_id,
                )
                .await,
            )
        }
        "service_plugin_runtime_status" => {
            parse_params::<EmptyParams>(params)?;
            rpc_json_result(
                run_plugin_runtime_control(
                    state,
                    VulcanPluginRuntimeManager::formal_runtime_status,
                )
                .await,
            )
        }
        "service_plugin_capacity_catalog" => {
            parse_params::<EmptyParams>(params)?;
            rpc_json_result(
                run_plugin_runtime_control(state, VulcanPluginRuntimeManager::formal_capacities)
                    .await,
            )
        }
        "service_plugin_capacity_registration_status" => {
            // Both receipt identities come from the original caller submission.
            // 两个回执身份均来自原始调用方提交。
            let request: RegistrationParams = parse_params(params)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime
                        .capacity_registration_status(&request.runtime_id, &request.registration_id)
                })
                .await,
            )
        }
        "service_register_plugin_capacity" => {
            // Deserialization rejects omissions and unknown fields before any metadata registration.
            // 反序列化在任何元数据登记前拒绝遗漏及未知字段。
            let params: RequestParams = parse_params(params)?;
            // JSON arrays must not use Serde's positional struct representation to bypass named fields.
            // JSON 数组不得使用 Serde 的位置结构体表示绕过命名字段。
            let request: VulcanPluginCapacityRegistrationRequest = parse_params(&params.request)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.register_formal_capacity(request)
                })
                .await,
            )
        }
        "service_control_plugin_capacity" => {
            // Exact parent, capacity and policy predecessor remain mandatory for destructive lifecycle control.
            // 破坏性生命周期控制必须携带精确父级、容量及策略前驱。
            let params: RequestParams = parse_params(params)?;
            // The complete request must be a named object, not a positional tuple.
            // 完整请求必须为命名对象，而非位置元组。
            let request: VulcanPluginCapacityControlRequest = parse_params(&params.request)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.control_formal_capacity(request)
                })
                .await,
            )
        }
        "service_begin_plugin_capacity_policy_update" => {
            // A nullable full config is required; missing config cannot silently reset policy.
            // 可空完整配置为必需字段；缺失配置不能静默重置策略。
            let params: RequestParams = parse_params(params)?;
            // Core typing owns value validation; the RPC boundary additionally enforces JSON object carriers.
            // 核心类型拥有值校验；RPC 边界另行要求 JSON 对象载体。
            let request: VulcanPluginCapacityPolicyUpdate = parse_params(&params.request)?;
            if request.config.is_some()
                && (!params.request["config"].is_object()
                    || !params.request["config"]["resources"].is_object())
            {
                return Err(WebuiServiceRpcError::InvalidParams(
                    "Capacity config and resources must be JSON objects.".into(),
                ));
            }
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.begin_capacity_policy_update(request)
                })
                .await,
            )
        }
        _ => return prewarm_policy::dispatch(state, method, params).await,
    };
    Ok(ServiceRpcDispatch::Handled(result))
}

#[cfg(test)]
mod tests;
