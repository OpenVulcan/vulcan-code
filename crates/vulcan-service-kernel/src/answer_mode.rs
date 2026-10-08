//! Answer mode: durable session admission without a model turn, and the global default for new sessions.
//! 回答模式：不启动模型回合的可靠会话准入，以及新会话使用的全局默认值。

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};
use crate::{
    SharedServiceBridgeState, current_v1_unix_millis, emit_workbench_event,
    mutate_provider_configuration, parse_session_id, refresh_settings_projection_locked,
};
use vulcan_agent_runtime::provider_config::{
    PersistedAnswerModeSelection, ProviderConfigurationFile,
};
use vulcan_service_api::{
    PromptAnswerModeSaveRequest, ServiceEventEmitter, SessionAnswerModeUpdateRequest,
    SessionAnswerModeUpdateResponse,
};
use vulcan_session_core::v1::config::AnswerMode;

/// Returns the persisted global answer mode copied into new sessions.
/// 返回复制到新会话的已持久化全局回答模式。
pub(crate) fn global_answer_mode(configuration: &ProviderConfigurationFile) -> AnswerMode {
    match configuration.prompt.answer_mode {
        PersistedAnswerModeSelection::Standard => AnswerMode::Standard,
        PersistedAnswerModeSelection::Interactive => AnswerMode::Interactive,
    }
}

/// Saves the global answer mode without changing existing sessions.
/// 保存全局回答模式，不改变已有会话。
pub(crate) async fn service_save_prompt_answer_mode(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PromptAnswerModeSaveRequest,
) -> Result<AnswerMode, String> {
    let selection = match request.mode {
        AnswerMode::Standard => PersistedAnswerModeSelection::Standard,
        AnswerMode::Interactive => PersistedAnswerModeSelection::Interactive,
    };
    let provider_configuration = mutate_provider_configuration(
        &state,
        "保存回答模式设置失败",
        |configuration| {
            configuration.prompt.answer_mode = selection;
            Ok(())
        },
        |guard, configuration| {
            refresh_settings_projection_locked(guard, configuration);
            configuration.clone()
        },
    )
    .await?;
    let mode = global_answer_mode(&provider_configuration);
    if let Ok(mut guard) = state.lock() {
        guard.workbench.default_answer_mode = mode;
        emit_workbench_event(&app, &guard.workbench);
    }
    Ok(mode)
}

/// Save `request` through the unique session writer and return the exact durable state or conflict.
/// 通过唯一会话写入器保存 `request`，返回精确可靠状态或冲突。
/// `state` supplies indexed session ownership; no provider invocation or user event is created.
/// `state` 提供索引会话归属；不调用供应商，也不创建用户事件。
pub(crate) async fn service_set_session_answer_mode(
    state: SharedServiceBridgeState,
    request: SessionAnswerModeUpdateRequest,
) -> Result<SessionAnswerModeUpdateResponse, String> {
    let session_id = parse_session_id(&request.session_id)
        .ok_or_else(|| String::from("Invalid answer mode session identity."))?;
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
                "Answer mode settings require an active regular session.",
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
                    document.metadata.config.answer_mode.revision != request.expected_revision;
                let changed = !conflict
                    && document.set_answer_mode(request.mode, request.expected_revision, now_ms)?;
                let value = (
                    document.segment_generation(),
                    document.metadata.config.answer_mode.clone(),
                    conflict,
                );
                Ok(if changed {
                    SessionMutationChange::Changed(value)
                } else {
                    SessionMutationChange::Unchanged(value)
                })
            },
        )
        .map_err(|error| format!("Failed to save answer mode: {error}"))?;
    Ok(SessionAnswerModeUpdateResponse {
        session_id: session_id.to_string(),
        window_generation: receipt.value.0,
        revision: receipt.revision,
        answer_mode: receipt.value.1,
        conflict: receipt.value.2,
    })
}
