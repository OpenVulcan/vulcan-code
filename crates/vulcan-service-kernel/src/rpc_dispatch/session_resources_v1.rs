//! V1 session-resource RPC routing.
//! V1 会话资源 RPC 路由。

use super::*;
use vulcan_service_api::{
    SessionDraftReadV1Request, SessionDraftWriteV1Request, SessionResourceListV1Request,
    SessionResourcePathV1Request,
};

use crate::conversation_drafts_v1::{service_session_draft_v1, service_set_session_draft_v1};
use crate::conversation_resources_v1::{
    service_session_resource_list_v1, service_session_resource_path_v1,
};

/// Exact public RPC wire names owned by the V1 session-resource router.
/// V1 会话资源路由拥有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_session_resource_list_v1",
    "service_session_resource_path_v1",
    "service_session_draft_v1",
    "service_set_session_draft_v1",
];

/// Dispatches one V1 session-resource RPC with strict whole-request parsing.
/// 使用严格整请求解析分发一个 V1 会话资源 RPC。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    let result =
        match method {
            "service_session_resource_list_v1" => rpc_json_result(
                service_session_resource_list_v1(
                    state,
                    serde_json::from_value::<SessionResourceListV1Request>(params.clone())
                        .map_err(|error| {
                            WebuiServiceRpcError::InvalidParams(format!(
                                "invalid params for {method}: {error}"
                            ))
                        })?,
                )
                .await,
            ),
            "service_session_resource_path_v1" => rpc_json_result(
                service_session_resource_path_v1(
                    state,
                    serde_json::from_value::<SessionResourcePathV1Request>(params.clone())
                        .map_err(|error| {
                            WebuiServiceRpcError::InvalidParams(format!(
                                "invalid params for {method}: {error}"
                            ))
                        })?,
                )
                .await,
            ),
            "service_session_draft_v1" => rpc_json_result(
                service_session_draft_v1(
                    state,
                    serde_json::from_value::<SessionDraftReadV1Request>(params.clone()).map_err(
                        |error| {
                            WebuiServiceRpcError::InvalidParams(format!(
                                "invalid params for {method}: {error}"
                            ))
                        },
                    )?,
                )
                .await,
            ),
            "service_set_session_draft_v1" => rpc_json_result(
                service_set_session_draft_v1(
                    state,
                    serde_json::from_value::<SessionDraftWriteV1Request>(params.clone()).map_err(
                        |error| {
                            WebuiServiceRpcError::InvalidParams(format!(
                                "invalid params for {method}: {error}"
                            ))
                        },
                    )?,
                )
                .await,
            ),
            _ => return Ok(ServiceRpcDispatch::Unknown),
        };
    Ok(ServiceRpcDispatch::Handled(result))
}
