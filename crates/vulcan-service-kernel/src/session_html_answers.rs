//! Durable HTML answer preference admission without starting a model turn.
//! 不启动模型回合的可靠 HTML 回答偏好准入。

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};
use crate::{SharedServiceBridgeState, current_v1_unix_millis, parse_session_id};
use vulcan_service_api::{SessionHtmlAnswerModeUpdateRequest, SessionHtmlAnswerModeUpdateResponse};

/// Save `request` through the unique session writer and return the exact durable state or conflict.
/// 通过唯一会话写入器保存 `request`，返回精确可靠状态或冲突。
/// `state` supplies indexed session ownership; no provider invocation or user event is created.
/// `state` 提供索引会话归属；不调用供应商，也不创建用户事件。
pub(crate) async fn service_set_session_html_answer_mode(
    state: SharedServiceBridgeState,
    request: SessionHtmlAnswerModeUpdateRequest,
) -> Result<SessionHtmlAnswerModeUpdateResponse, String> {
    let session_id = parse_session_id(&request.session_id)
        .ok_or_else(|| String::from("Invalid HTML answer session identity."))?;
    let registry = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("Workspace is not ready."))?;
        if !crate::workspace_snapshot_contains_regular_session(workspace, session_id) {
            return Err(String::from(
                "HTML answer settings require an active regular session.",
            ));
        }
        std::sync::Arc::clone(&guard.session_runtime_registry)
    };
    let _lifecycle = registry
        .acquire_lifecycle_transition(session_id)
        .await
        .map_err(|error| error.to_string())?;
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state(&state, session_id).await?;
    let now_ms = current_v1_unix_millis()?;
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                let conflict =
                    document.metadata.config.html_answers.revision != request.expected_revision;
                let changed = !conflict
                    && document.set_html_answer_mode(
                        request.enabled,
                        request.expected_revision,
                        now_ms,
                    )?;
                let value = (
                    document.segment_generation(),
                    document.metadata.config.html_answers.clone(),
                    conflict,
                );
                Ok(if changed {
                    SessionMutationChange::Changed(value)
                } else {
                    SessionMutationChange::Unchanged(value)
                })
            },
        )
        .map_err(|error| format!("Failed to save HTML answer preference: {error}"))?;
    Ok(SessionHtmlAnswerModeUpdateResponse {
        session_id: session_id.to_string(),
        window_generation: receipt.value.0,
        revision: receipt.revision,
        html_answers: receipt.value.1,
        conflict: receipt.value.2,
    })
}
