//! Request-head admission before any RPC JSON buffering or parsing.
//! 在任何 RPC JSON 缓冲及解析之前执行请求头准入。

use super::{
    AxumResponse, AxumStatusCode, WebuiServiceHttpState, webui_service_headers_origin_is_allowed,
    webui_service_request_token_is_valid, webui_service_rpc_error_response,
};
use axum::{extract::FromRequestParts, http::request::Parts};

/// Proves Origin and token admission without consuming or materializing the request body.
/// 无需消费或物化请求体即可证明 Origin 与令牌准入。
pub(super) struct RpcAdmission;

impl FromRequestParts<WebuiServiceHttpState> for RpcAdmission {
    type Rejection = AxumResponse;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &WebuiServiceHttpState,
    ) -> Result<Self, Self::Rejection> {
        if !webui_service_headers_origin_is_allowed(&parts.headers) {
            return Err(webui_service_rpc_error_response(
                AxumStatusCode::FORBIDDEN,
                "origin_not_allowed",
                "Browser service origin is not allowed.",
            ));
        }
        if !webui_service_request_token_is_valid(
            &parts.headers,
            parts.uri.query(),
            &state.service_token,
        ) {
            return Err(webui_service_rpc_error_response(
                AxumStatusCode::UNAUTHORIZED,
                "unauthorized",
                "Browser service token is missing or invalid.",
            ));
        }
        Ok(Self)
    }
}
