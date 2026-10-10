//! Exact WebUI permission mode command builder using the authoritative shared request type.
//! 使用权威共享请求类型的一对一 WebUI 权限模式命令构建器。

use serde_json::{Value, json};
use vulcan_service_api::SessionPermissionModeUpdateRequest;

/// Build the WebUI session permission mode request; return its method and exact parameter envelope.
/// 构建 WebUI 会话权限模式请求；返回方法及精确参数信封。
pub fn set_session_permission_mode(
    request: &SessionPermissionModeUpdateRequest,
) -> (&'static str, Value) {
    (
        "service_set_session_permission_mode",
        json!({ "request": request }),
    )
}
