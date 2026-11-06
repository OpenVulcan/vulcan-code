//! Session activity RPC routing: shell jobs and subagent activity.
//! 会话活动 RPC 路由：shell 任务与子代理活动。

use super::*;
use vulcan_service_api::{
    ClearSessionActivityRequest, SessionActivityRequest, ShellCommandBackgroundRequest,
    ShellJobListRequest, ShellJobOutputRequest, ShellJobTargetRequest,
};

/// Exact public RPC wire names owned by the session activity router.
/// 会话活动路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_list_shell_jobs",
    "service_read_shell_job_output",
    "service_stop_shell_job",
    "service_background_shell_command",
    "service_session_activity",
    "service_clear_session_activity",
    "service_running_shell_job_count",
];

/// Dispatches one session activity RPC with strict whole-request parsing.
/// 使用严格整请求解析分发一个会话活动 RPC。
///
/// # Returns
/// 返回值
/// Returns `Handled` for the owned methods and `Unknown` for every other domain.
/// 对自有方法返回 `Handled`，对其他领域返回 `Unknown`。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    let result = match method {
        "service_list_shell_jobs" => rpc_json_result(
            service_list_shell_jobs(state, parse_request::<ShellJobListRequest>(params, method)?)
                .await,
        ),
        "service_read_shell_job_output" => rpc_json_result(
            service_read_shell_job_output(
                state,
                parse_request::<ShellJobOutputRequest>(params, method)?,
            )
            .await,
        ),
        "service_stop_shell_job" => rpc_json_result(
            service_stop_shell_job(
                state,
                parse_request::<ShellJobTargetRequest>(params, method)?,
            )
            .await,
        ),
        "service_background_shell_command" => rpc_json_result(
            service_background_shell_command(parse_request::<ShellCommandBackgroundRequest>(
                params, method,
            )?)
            .await,
        ),
        "service_session_activity" => rpc_json_result(
            service_session_activity(
                state,
                parse_request::<SessionActivityRequest>(params, method)?,
            )
            .await,
        ),
        "service_clear_session_activity" => rpc_json_result(
            service_clear_session_activity(
                state,
                parse_request::<ClearSessionActivityRequest>(params, method)?,
            )
            .await,
        ),
        "service_running_shell_job_count" => rpc_json_result(Ok::<usize, String>(
            vulcan_host_tools::shell_job::ShellJobRuntime::global().running_background_count_all(),
        )),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}

/// Deserializes one complete strict request object.
/// 反序列化一个完整严格请求对象。
fn parse_request<T: serde::de::DeserializeOwned>(
    params: &Value,
    method: &str,
) -> Result<T, WebuiServiceRpcError> {
    serde_json::from_value(params.clone()).map_err(|error| {
        WebuiServiceRpcError::InvalidParams(format!("invalid params for {method}: {error}"))
    })
}
