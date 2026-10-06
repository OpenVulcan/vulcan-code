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
