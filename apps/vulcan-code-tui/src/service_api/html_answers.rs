//! Exact WebUI HTML command builders using authoritative shared request types.
//! 使用权威共享请求类型的一对一 WebUI HTML 命令构建器。

use serde_json::{Value, json};
use vulcan_service_api::{
    HtmlAnswerRevokeRequest, HtmlAnswerViewLinkRequest, SessionHtmlAnswerModeUpdateRequest,
};

/// Build the WebUI preference request; return its method and exact parameter envelope.
/// 构建 WebUI 偏好请求；返回方法及精确参数信封。
pub fn set_session_html_answer_mode(
    request: &SessionHtmlAnswerModeUpdateRequest,
) -> (&'static str, Value) {
    (
        "service_set_session_html_answer_mode",
        json!({ "request": request }),
    )
}

/// Build the WebUI single-result grant request without including generated source bytes.
/// 构建 WebUI 单结果授权请求，不包含生成源码字节。
pub fn create_html_answer_view_link(request: &HtmlAnswerViewLinkRequest) -> (&'static str, Value) {
    (
        "service_create_html_answer_view_link",
        json!({ "request": request }),
    )
}

/// Build the WebUI client-owned revocation request; return method and parameters.
/// 构建 WebUI 客户端所有的撤销请求；返回方法和参数。
pub fn revoke_html_answer_view_link(request: &HtmlAnswerRevokeRequest) -> (&'static str, Value) {
    (
        "service_revoke_html_answer_view_link",
        json!({ "request": request }),
    )
}
