//! Exact WebUI answer mode command builders using authoritative shared request types.
//! 使用权威共享请求类型的一对一 WebUI 回答模式命令构建器。

use serde_json::{Value, json};
use vulcan_service_api::SessionAnswerModeUpdateRequest;

/// Build the WebUI session answer mode request; return its method and exact parameter envelope.
/// 构建 WebUI 会话回答模式请求；返回方法及精确参数信封。
pub fn set_session_answer_mode(request: &SessionAnswerModeUpdateRequest) -> (&'static str, Value) {
    (
        "service_set_session_answer_mode",
        json!({ "request": request }),
    )
}
