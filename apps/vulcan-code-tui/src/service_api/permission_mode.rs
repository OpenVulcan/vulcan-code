//! Exact WebUI permission mode command builder using the authoritative shared request type.
//! 使用权威共享请求类型的一对一 WebUI 权限模式命令构建器。

use serde_json::{Value, json};
use vulcan_service_api::{
    SessionPermissionModeUpdateRequest, SessionUnattendedUpdateRequest, UnattendedDefaultRequest,
};

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

/// Build the WebUI `setSessionUnattended` request; return its method and exact parameter envelope.
/// 构建 WebUI `setSessionUnattended` 请求；返回方法及精确参数信封。
pub fn set_session_unattended(request: &SessionUnattendedUpdateRequest) -> (&'static str, Value) {
    (
        "service_set_session_unattended",
        json!({ "request": request }),
    )
}

/// Build the WebUI `loadSessionUnattendedDefault` request, the prefill of the enable dialog.
/// 构建 WebUI `loadSessionUnattendedDefault` 请求，即开启对话框的预填值。
pub fn load_session_unattended_default(
    request: &UnattendedDefaultRequest,
) -> (&'static str, Value) {
    (
        "service_session_unattended_default",
        json!({ "request": request }),
    )
}
