//! Environment variable RPC routing.
//! 环境变量 RPC 路由。

use super::*;
use crate::environment_variables;

/// Exact public RPC wire names owned by the environment-variable domain.
/// 环境变量领域持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_environment_variables",
    "service_save_environment_variable",
    "service_delete_environment_variable",
    "service_check_environment_variable_name",
    "service_session_environment",
    "service_delete_session_environment_variable",
    "service_submit_environment_values",
];

/// Dispatches one exact environment-variable RPC.
/// 分发一个精确环境变量 RPC。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    let result = match method {
        "service_environment_variables" => {
            rpc_json_result(environment_variables::service_environment_variables(state).await)
        }
        "service_save_environment_variable" => rpc_json_result(
            environment_variables::service_save_environment_variable(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "service_delete_environment_variable" => rpc_json_result(
            environment_variables::service_delete_environment_variable(
                state,
                rpc_required_param(params, "id")?,
            )
            .await,
        ),
        "service_check_environment_variable_name" => rpc_json_result(
            environment_variables::service_check_environment_variable_name(rpc_required_param(
                params, "name",
            )?),
        ),
        "service_session_environment" => rpc_json_result(
            environment_variables::service_session_environment(
                state,
                rpc_required_param(params, "sessionId")?,
            )
            .await,
        ),
        "service_delete_session_environment_variable" => rpc_json_result(
            environment_variables::service_delete_session_environment_variable(
                state,
                rpc_required_param(params, "sessionId")?,
                rpc_required_param(params, "name")?,
            )
            .await,
        ),
        "service_submit_environment_values" => rpc_json_result(
            environment_variables::service_submit_environment_values(
                state,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
