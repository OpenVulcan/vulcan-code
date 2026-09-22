//! Same-business preparation from an exact durable collaboration acceptance.
//! 从精确可靠协作接纳准备同一业务续跑。

use super::pending_prepare::{ExactPendingPromptDispatch, prepare_webui_pending_prompt};
use super::*;
use crate::conversation_submission::exact_v1_dispatch_runtime_options;
use crate::session_goal_lifecycle::registered_session_v1_handle_from_state;
use crate::session_runtime_registry_v1::SessionHandle;
use vulcan_session_core::v1::collaboration::CollaborationContinuation;
use vulcan_workbench_model::PendingSessionPromptEntry;

/// Revalidates `authority` against the canonical handle and requires a closed physical API.
/// 按规范句柄重验 `authority`，并要求物理 API 已闭合。
/// Returns an explicit error if the original turn, control state or oldest pending acceptance changed.
/// 原回合、控制状态或最早待处理接纳改变时返回明确错误。
pub(super) fn validate_collaboration_dispatch(
    handle: &SessionHandle,
    authority: &CollaborationContinuation,
) -> Result<(), String> {
    handle
        .read(|document| {
            if document.active_api().is_some()
                || document.pending_collaboration_continuation().as_ref() != Some(authority)
            {
                return Err(String::from(
                    "Collaboration continuation no longer owns the original open business turn.",
                ));
            }
            Ok(())
        })
        .map_err(|error| error.to_string())?
}

/// Prepares the exact pending collaboration using the existing submission reservation and business Hook.
/// 使用已有提交预留和业务 Hook 准备精确待处理协作。
/// All runtime arguments retain their ordinary service ownership; `authority` identifies the original acceptance.
/// 所有运行时参数保留通常服务归属；`authority` 标识原始接纳。
/// Returns whether preparation started; no human message or new logical turn is created.
/// 返回是否开始准备；不创建人类消息或新的逻辑回合。
pub(crate) async fn maybe_prepare_webui_collaboration_continuation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: CollaborationContinuation,
) -> Result<bool, String> {
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    validate_collaboration_dispatch(&handle, &authority)?;
    let entry = PendingSessionPromptEntry {
        id: format!(
            "webui-v1-agent-continuation-{}-{}",
            authority.turn_id.value(),
            authority.acceptance_seq.value()
        ),
        prompt: String::from(
            "Continue the current task using its pending agent guidance or result.",
        ),
        hidden_model_guidance: None,
        preview: String::from(
            "Continue the current task using its pending agent guidance or result.",
        ),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: exact_v1_dispatch_runtime_options(handle.as_ref())?,
        source: PendingSessionPromptSource::AgentContinuation,
        task_id: None,
        panel_entry_id: None,
        created_at: current_unix_millis_text(),
    };
    prepare_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        ExactPendingPromptDispatch::Collaboration { authority, entry },
    )
    .await
}
