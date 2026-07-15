//! Workspace input and output RPC routing.
//! 工作区输入输出 RPC 路由。

use super::*;

/// Exact public RPC wire names owned by this domain router.
/// 本领域路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "startup.snapshot",
    "service_startup_snapshot",
    "workbench.snapshot",
    "service_workbench_snapshot",
    "workspace.diff.snapshot",
    "service_workspace_diff_snapshot",
    "workspace.diff.fileDetail",
    "service_workspace_diff_file_detail",
    "workspace.diff.search",
    "service_workspace_diff_search",
    "workspace.git.fileAction",
    "service_workspace_git_file_action",
    "workspace.files.list",
    "service_workspace_files_list",
    "workspace.files.read",
    "service_workspace_files_read",
];

/// Dispatches one RPC method owned by this domain.
/// 分发本领域持有的单个 RPC 方法。
///
/// # Parameters
/// 参数
/// `state` owns service data, `method` is the exact wire name, and `params` is the normalized object.
/// `state` 持有服务数据，`method` 是精确线协议名称，`params` 是归一化对象。
///
/// # Returns
/// 返回值
/// Returns `Handled` with the service result, `Unknown` for another domain, or a parameter parsing error.
/// 返回携带服务结果的 `Handled`、表示其他领域的 `Unknown`，或参数解析错误。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, String> {
    // result preserves service failures inside Handled so the top-level return trace still runs.
    // result 把服务失败保留在 Handled 内，使顶层返回追踪仍会执行。
    let result = match method {
        "startup.snapshot" | "service_startup_snapshot" => {
            rpc_json_result(service_startup_snapshot(state))
        }
        "workbench.snapshot" | "service_workbench_snapshot" => {
            rpc_json_result(service_workbench_snapshot(state))
        }
        "workspace.diff.snapshot" | "service_workspace_diff_snapshot" => rpc_json_result(
            service_workspace_diff_snapshot(
                state,
                rpc_optional_param(params, "workspacePath")?,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_optional_param(params, "request")?,
            )
            .await,
        ),
        "workspace.diff.fileDetail" | "service_workspace_diff_file_detail" => rpc_json_result(
            service_workspace_diff_file_detail(
                state,
                rpc_optional_param(params, "workspacePath")?,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_optional_param(params, "request")?,
                rpc_required_param(params, "file")?,
            )
            .await,
        ),
        "workspace.diff.search" | "service_workspace_diff_search" => rpc_json_result(
            service_workspace_diff_search(
                state,
                rpc_optional_param(params, "workspacePath")?,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_optional_param(params, "request")?,
                rpc_required_param(params, "search")?,
            )
            .await,
        ),
        "workspace.git.fileAction" | "service_workspace_git_file_action" => rpc_json_result(
            service_workspace_git_file_action(
                state,
                rpc_optional_param(params, "workspacePath")?,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "request")?,
            )
            .await,
        ),
        "workspace.files.list" | "service_workspace_files_list" => rpc_json_result(
            service_workspace_files_list(
                rpc_required_param(params, "workspacePath")?,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_optional_param(params, "directoryPath")?,
                rpc_optional_param(params, "filters")?,
            )
            .await,
        ),
        "workspace.files.read" | "service_workspace_files_read" => rpc_json_result(
            service_workspace_files_read(
                rpc_required_param(params, "workspacePath")?,
                rpc_optional_param(params, "projectId")?,
                rpc_optional_param(params, "conversationId")?,
                rpc_required_param(params, "filePath")?,
                rpc_optional_param(params, "filters")?,
            )
            .await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
