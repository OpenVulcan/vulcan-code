//! Durable automatic-prewarm policy controls never dispatch plugin initialization.
//! 持久自动预热策略控制绝不派发插件初始化。

use super::*;
use vulcan_plugin_runtime::VulcanPluginPrewarmPolicyUpdate;

/// Dispatch method with strict named params against state's existing owner; return the original policy result.
/// 使用严格具名 params 针对 state 的既有所有者分发 method；返回原策略结果。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    // Management receipts remain recoverable after an HTTP observer disconnects.
    // HTTP 观测者断线后，管理回执仍可恢复。
    let result = match method {
        "service_plugin_prewarm_policy_status" => {
            parse_params::<EmptyParams>(params)?;
            rpc_json_result(
                run_plugin_runtime_control(
                    state,
                    VulcanPluginRuntimeManager::prewarm_policy_status,
                )
                .await,
            )
        }
        "service_begin_plugin_prewarm_policy_update" => {
            // Require explicit rules, including an empty collection for disable; missing rules are never a reset.
            // 要求明确规则，包括用于关闭的空集合；缺失规则绝不表示重置。
            let params: RequestParams = parse_params(params)?;
            let request: VulcanPluginPrewarmPolicyUpdate = parse_params(&params.request)?;
            // Serde accepts positional structs; every nested rule must still use its named wire contract.
            // Serde 接受位置结构体；每个嵌套规则仍须使用具名线协议。
            if params.request["rules"]
                .as_array()
                .expect("typed rules collection")
                .iter()
                .any(|rule| !rule.is_object())
            {
                return Err(WebuiServiceRpcError::InvalidParams(
                    "Automatic prewarm rules must be JSON objects.".into(),
                ));
            }
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.begin_prewarm_policy_update(request)
                })
                .await,
            )
        }
        "service_resume_plugin_prewarm_policy_update" => {
            let request: UpdateParams = parse_params(params)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.resume_prewarm_policy_update(&request.update_id)
                })
                .await,
            )
        }
        "service_cancel_plugin_prewarm_policy_update" => {
            let request: UpdateParams = parse_params(params)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.cancel_prewarm_policy_update(&request.update_id)
                })
                .await,
            )
        }
        "service_forget_plugin_prewarm_policy_update" => {
            let request: UpdateParams = parse_params(params)?;
            rpc_json_result(
                run_plugin_runtime_control(state, move |runtime| {
                    runtime.forget_prewarm_policy_update(&request.update_id)
                })
                .await,
            )
        }
        _ => return prewarm::dispatch(state, method, params).await,
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
