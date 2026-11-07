//! Feedback and message-center RPC routing.
//! 反馈与消息中心 RPC 路由。

use super::*;
use crate::feedback_services;

/// Exact public RPC wire names owned by the feedback and message-center domain.
/// 反馈与消息中心领域持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_prepare_feedback",
    "service_submit_feedback",
    "service_list_feedback",
    "service_feedback_detail",
    "service_link_feedback_pr",
    "service_delete_feedback",
    "service_list_announcements",
    "service_mark_announcements",
    "service_message_summary",
    "service_list_messages",
    "service_mark_messages_read",
    "service_message_center_settings",
    "service_save_message_center_settings",
];

/// Dispatches one exact feedback or message-center RPC.
/// 分发一个精确反馈或消息中心 RPC。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    let result = match method {
        "service_prepare_feedback" => rpc_json_result(
            feedback_services::service_prepare_feedback(
                state,
                rpc_required_param(params, "draft")?,
            )
            .await,
        ),
        "service_submit_feedback" => rpc_json_result(
            feedback_services::service_submit_feedback(
                state,
                rpc_required_param(params, "draftId")?,
                rpc_required_param(params, "digest")?,
            )
            .await,
        ),
        "service_list_feedback" => rpc_json_result(
            feedback_services::service_list_feedback(state, rpc_optional_param(params, "cursor")?)
                .await,
        ),
        "service_feedback_detail" => rpc_json_result(
            feedback_services::service_feedback_detail(
                state,
                rpc_required_param(params, "feedbackId")?,
            )
            .await,
        ),
        "service_link_feedback_pr" => rpc_json_result(
            feedback_services::service_link_feedback_pr(
                state,
                rpc_required_param(params, "feedbackId")?,
                rpc_required_param(params, "url")?,
            )
            .await,
        ),
        "service_delete_feedback" => rpc_json_result(
            feedback_services::service_delete_feedback(
                state,
                rpc_required_param(params, "feedbackId")?,
            )
            .await,
        ),
        "service_list_announcements" => {
            rpc_json_result(feedback_services::service_list_announcements(state).await)
        }
        "service_mark_announcements" => rpc_json_result(
            feedback_services::service_mark_announcements(
                state,
                rpc_required_param(params, "announcementIds")?,
                rpc_optional_param(params, "dismissed")?.unwrap_or(false),
            )
            .await,
        ),
        "service_message_summary" => {
            rpc_json_result(feedback_services::service_message_summary(state).await)
        }
        "service_list_messages" => rpc_json_result(
            feedback_services::service_list_messages(
                state,
                rpc_optional_param(params, "cursor")?,
                rpc_optional_param(params, "unreadOnly")?.unwrap_or(false),
            )
            .await,
        ),
        "service_mark_messages_read" => rpc_json_result(
            feedback_services::service_mark_messages_read(
                state,
                rpc_optional_param(params, "ids")?,
                rpc_optional_param(params, "allBefore")?,
            )
            .await,
        ),
        "service_message_center_settings" => {
            rpc_json_result(feedback_services::service_message_center_settings(state))
        }
        "service_save_message_center_settings" => rpc_json_result(
            feedback_services::service_save_message_center_settings(
                state,
                rpc_required_param(params, "settings")?,
            )
            .await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
