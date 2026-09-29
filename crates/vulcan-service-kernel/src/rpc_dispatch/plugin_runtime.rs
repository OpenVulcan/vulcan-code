//! Formal plugin administration belongs to the authenticated host RPC boundary.
//! 正式插件管理属于已认证宿主 RPC 边界。

use super::*;
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
    "service_plugin_capacity_catalog",
    "service_plugin_capacity_registration_status",
    "service_register_plugin_capacity",
    "service_control_plugin_capacity",
    "service_begin_plugin_capacity_policy_update",
    "service_plugin_capacity_policy_update_status",
    "service_resume_plugin_capacity_policy_update",
    "service_cancel_plugin_capacity_policy_update",
    "service_forget_plugin_capacity_policy_update",
    "service_plugin_prewarm_targets",
    "service_prepare_plugin_prewarm",
    "service_plugin_prewarm_status",
    "service_start_plugin_prewarm",
    "service_cancel_plugin_prewarm",
    "service_recover_plugin_prewarm_cleanup",
    "service_forget_plugin_prewarm",
    "service_plugin_prewarm_policy_status",
    "service_begin_plugin_prewarm_policy_update",
    "service_resume_plugin_prewarm_policy_update",
    "service_cancel_plugin_prewarm_policy_update",
    "service_forget_plugin_prewarm_policy_update",
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

/// Durable UUID selects the same transaction even after an explicit process restart recovery.
/// 持久 UUID 即使在明确进程重启恢复后也选择同一事务。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateParams {
    /// Exact original transaction identity is never replaced by a latest-record lookup.
    /// 精确原始事务身份绝不由最新记录查找替代。
    update_id: String,
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
        "service_plugin_capacity_policy_update_status" => {
            // Lookup never acquires the settings writer or advances the record.
            // 查找绝不取得设置写入租约或推进记录。
            let request: UpdateParams = parse_params(params)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.capacity_policy_update(&request.update_id)
                })
                .await,
            )
        }
        "service_resume_plugin_capacity_policy_update" => {
            // Recovery reuses the retained original checkpoint and never refreshes its predecessor.
            // 恢复复用保留原始检查点，绝不刷新其前驱。
            let request: UpdateParams = parse_params(params)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.resume_capacity_policy_update(&request.update_id)
                })
                .await,
            )
        }
        "service_cancel_plugin_capacity_policy_update" => {
            // Cancellation is an explicit manager operation, not inferred from a disconnected HTTP observer.
            // 取消为明确管理器操作，不从 HTTP 观测者断线推断。
            let request: UpdateParams = parse_params(params)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.cancel_capacity_policy_update(&request.update_id)
                })
                .await,
            )
        }
        "service_forget_plugin_capacity_policy_update" => {
            // Only a terminal original receipt may be durably acknowledged.
            // 仅终态原回执可以被持久确认。
            let request: UpdateParams = parse_params(params)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.forget_capacity_policy_update(&request.update_id)
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
