// Mirror of `service/workbench-snapshot-service.ts`.
// `service/workbench-snapshot-service.ts` 的镜像。

use serde_json::Value;

use super::{ServiceCall, Slot};

/// Builds the request for `workbench.snapshot`.
/// 构建 `workbench.snapshot` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_workbench_snapshot() -> ServiceCall {
    ServiceCall::load(
        "load_workbench_snapshot",
        "workbench.snapshot",
        Value::Null,
        Slot::Workbench,
    )
}

/// Builds `loadWorkspaceFileDirectory` with the service's safe default filters, as the WebUI
/// composer's `@` path mention sends it. `directory_path` is `None` for the project root.
/// 以服务的安全默认过滤器构建 `loadWorkspaceFileDirectory`，与 WebUI 输入区 `@` 路径提及发送的一致。
/// 项目根目录时 `directory_path` 为 `None`。
pub fn load_workspace_file_directory(
    workspace_path: &str,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
    directory_path: Option<&str>,
) -> ServiceCall {
    ServiceCall::load(
        "load_workspace_file_directory",
        "workspace.files.list",
        serde_json::json!({
            "workspacePath": workspace_path,
            "projectId": project_id,
            "conversationId": conversation_id,
            "directoryPath": directory_path,
            "filters": vulcan_service_api::WorkspaceFileFilterOptions::default(),
        }),
        Slot::PathMentionDirectory,
    )
}
