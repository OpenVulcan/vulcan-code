//! V1 conversation-event RPC routing.
//! V1 会话事件 RPC 路由。

use super::*;
use vulcan_service_api::{
    ConversationEditGenerationV1Request, ConversationEventHydrationV1Request,
    ConversationEventPageV1Request, ConversationEventReconnectV1Request,
    ConversationForkGenerationV1Request,
};

/// Exact public RPC wire names owned by the V1 event-window router.
/// V1 事件窗口路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_session_event_hydration_v1",
    "service_session_event_page_v1",
    "service_session_event_reconnect_v1",
    "service_edit_conversation_generation_v1",
    "service_fork_conversation_generation_v1",
];

/// Dispatches one V1 conversation-event RPC with strict whole-request parsing.
/// 使用严格整请求解析分发一个 V1 会话事件 RPC。
///
/// # Parameters
/// 参数
/// `state` owns the service runtime, `method` is the exact wire name, and
/// `params` is the complete normalized request object.
/// `state` 持有服务运行时，`method` 是精确线协议名称，`params` 是完整归一化请求对象。
///
/// # Returns
/// 返回值
/// Returns `Handled` for the three owned methods and `Unknown` for every other
/// domain.
/// 对三个自有方法返回 `Handled`，对其他领域返回 `Unknown`。
pub(super) async fn dispatch(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, String> {
    let result = match method {
        "service_session_event_hydration_v1" => rpc_json_result(
            service_session_event_hydration_v1(
                state,
                parse_request::<ConversationEventHydrationV1Request>(params, method)?,
            )
            .await,
        ),
        "service_session_event_page_v1" => rpc_json_result(
            service_session_event_page_v1(
                state,
                parse_request::<ConversationEventPageV1Request>(params, method)?,
            )
            .await,
        ),
        "service_session_event_reconnect_v1" => rpc_json_result(
            service_session_event_reconnect_v1(
                state,
                parse_request::<ConversationEventReconnectV1Request>(params, method)?,
            )
            .await,
        ),
        "service_edit_conversation_generation_v1" => rpc_json_result(
            service_edit_conversation_generation_v1(
                app.clone(),
                state,
                parse_request::<ConversationEditGenerationV1Request>(params, method)?,
            )
            .await,
        ),
        "service_fork_conversation_generation_v1" => rpc_json_result(
            service_fork_conversation_generation_v1(
                app,
                state,
                parse_request::<ConversationForkGenerationV1Request>(params, method)?,
            )
            .await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}

/// Deserializes one complete strict V1 request object.
/// 反序列化一个完整严格 V1 请求对象。
fn parse_request<T: serde::de::DeserializeOwned>(
    params: &Value,
    method: &str,
) -> Result<T, String> {
    serde_json::from_value(params.clone())
        .map_err(|error| format!("invalid params for {method}: {error}"))
}
