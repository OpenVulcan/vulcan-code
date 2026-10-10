//! Dedicated sessions (专职助手): memory lock transitions between turns and base prompt drafting.
//! 专职会话（专职助手）：回合之间的记忆锁定切换与基础提示词起草。

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange, SessionRuntimeError,
};
use crate::{SharedServiceBridgeState, current_v1_unix_millis, parse_session_id};
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, ContentItem, MessageRole, ParallelToolCallPolicy,
    ReasoningOutputPolicy, StructuredOutputPolicy, ToolChoicePolicy,
};
use vulcan_service_api::{
    DedicatedMemoryLockRequest, DedicatedMemoryLockResponse, DedicatedPromptDraftRequest,
    DedicatedPromptDraftResponse,
};
use vulcan_session_core::v1::{DEDICATED_BASE_PROMPT_MAX_CHARS, DomainError};

/// Locks or unlocks dedicated-session memory through the unique session writer.
/// 通过唯一会话写入器锁定或解锁专职会话记忆。
///
/// # Parameters
/// 参数
/// `state` supplies indexed session ownership; `request` names the session and target state.
/// `state` 提供索引会话归属；`request` 指定会话与目标状态。
///
/// # Returns
/// 返回值
/// Returns the durable profile and the appended note seq, or a stable error code when the
/// session is not dedicated or a turn is still open.
/// 返回可靠配置与追加的通知 seq；会话不是专职会话或仍有开放回合时返回稳定错误码。
pub(crate) async fn service_set_dedicated_memory_lock(
    state: SharedServiceBridgeState,
    request: DedicatedMemoryLockRequest,
) -> Result<DedicatedMemoryLockResponse, String> {
    let session_id = parse_session_id(&request.session_id)
        .ok_or_else(|| String::from("Invalid dedicated session identity."))?;
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
                "Dedicated memory lock requires an active regular session.",
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
            SessionFlushReason::DisplayBoundary,
            |document| {
                let note_seq = document.set_dedicated_memory_locked(request.locked, now_ms)?;
                let profile = document
                    .dedicated_profile()
                    .cloned()
                    .ok_or(DomainError::NotDedicatedSession)?;
                let value = (document.segment_generation(), note_seq, profile);
                Ok(if note_seq.is_some() {
                    SessionMutationChange::Changed(value)
                } else {
                    SessionMutationChange::Unchanged(value)
                })
            },
        )
        .map_err(|error| dedicated_lock_error_code(&error))?;
    Ok(DedicatedMemoryLockResponse {
        session_id: session_id.to_string(),
        window_generation: receipt.value.0,
        revision: receipt.revision,
        note_seq: receipt.value.1,
        dedicated: receipt.value.2,
    })
}

/// Maps a lock failure onto a stable code the clients can localize, keeping the detail.
/// 将锁定失败映射为客户端可本地化的稳定错误码，并保留细节。
fn dedicated_lock_error_code(error: &SessionRuntimeError) -> String {
    let code = match error {
        SessionRuntimeError::Domain(domain) => match domain.as_ref() {
            DomainError::NotDedicatedSession => "ERR_DEDICATED_SESSION_REQUIRED",
            DomainError::DedicatedMemoryLockWhileTurnOpen { .. } => {
                "ERR_DEDICATED_MEMORY_LOCK_TURN_OPEN"
            }
            _ => "ERR_DEDICATED_MEMORY_LOCK_FAILED",
        },
        _ => "ERR_DEDICATED_MEMORY_LOCK_FAILED",
    };
    format!("{code}: {error}")
}

/// Drafts or revises a dedicated-session base prompt on the exact model the user selected.
/// 在用户所选的精确模型上起草或修订专职会话基础提示词。
///
/// # Parameters
/// 参数
/// `state` owns the AI runtime; `request` freezes the route and the user's material.
/// `state` 持有 AI 运行时；`request` 冻结路由与用户提供的材料。
///
/// # Returns
/// 返回值
/// Returns the trimmed prompt, or a stable error code when the input is empty or too long, or the
/// output exceeds the base prompt limit. The route is never substituted.
/// 返回去除首尾空白的提示词；输入为空或过长、输出超过基础提示词上限时返回稳定错误码。绝不替换路由。
pub(crate) async fn service_draft_dedicated_prompt(
    state: SharedServiceBridgeState,
    request: DedicatedPromptDraftRequest,
) -> Result<DedicatedPromptDraftResponse, String> {
    let user_message = dedicated_prompt_draft_message(&request)?;
    let (runtime, configuration) =
        crate::public_ai_settings::public_ai_runtime_and_configuration(&state)?;
    let selection = runtime.resolve_connection_model_selection(
        &configuration,
        request.connection_id.as_str(),
        request.provider_id.as_str(),
        request.model_id.as_str(),
    )?;
    let service = state
        .lock()
        .map_err(|error| format!("Failed to lock service state: {error}"))?
        .agent_runtime
        .as_ref()
        .map(|runtime| std::sync::Arc::clone(&runtime.configured_inference_service))
        .ok_or_else(|| String::from("Configured inference runtime is still starting."))?;
    let execution_request = AiExecutionRequest {
        operation: AiOperation::ConversationRespond,
        model_id: selection.model.id.clone(),
        cache_scope_id: None,
        continuation: None,
        messages: vec![
            text_message(
                MessageRole::System,
                vulcan_runtime_assets::DEDICATED_PROMPT_GENERATOR_MD,
            ),
            text_message(MessageRole::User, &user_message),
        ],
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: ToolChoicePolicy::Disabled,
        parallel_tool_calls: ParallelToolCallPolicy::Disabled,
        structured_output: StructuredOutputPolicy::None,
        stop_sequences: Vec::new(),
        prompt_cache: selection.overrides.prompt_cache.clone().unwrap_or_default(),
        reasoning_effort: None,
        reasoning_output: ReasoningOutputPolicy::ProviderDefault,
        model_parameters: selection.overrides.model_parameters.clone(),
        resources: Vec::new(),
        output_token_limit: Some(
            crate::configured_inference::DEDICATED_PROMPT_DRAFT_OUTPUT_TOKEN_LIMIT,
        ),
    };
    let prompt = service
        .execute_exact_selection_text(
            crate::configured_inference::ExactSelectionTextPurpose::DedicatedPromptDraft,
            &selection,
            execution_request,
            uuid::Uuid::new_v4().to_string(),
        )
        .await
        .map_err(|error| format!("ERR_DEDICATED_PROMPT_DRAFT_FAILED: {error}"))?;
    if prompt.chars().count() > DEDICATED_BASE_PROMPT_MAX_CHARS {
        return Err(String::from("ERR_DEDICATED_PROMPT_DRAFT_TOO_LONG"));
    }
    Ok(DedicatedPromptDraftResponse {
        prompt,
        provider_id: request.provider_id,
        model_id: request.model_id,
    })
}

/// Wraps the user's material in the data tags the generator prompt names.
/// 用生成器提示词约定的数据标签包裹用户材料。
fn dedicated_prompt_draft_message(request: &DedicatedPromptDraftRequest) -> Result<String, String> {
    let requirement = request.requirement.trim();
    let current_prompt = request
        .current_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let revision_request = request
        .revision_request
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if requirement.is_empty() && current_prompt.is_none() {
        return Err(String::from("ERR_DEDICATED_PROMPT_DRAFT_EMPTY"));
    }
    if [Some(requirement), current_prompt, revision_request]
        .into_iter()
        .flatten()
        .any(|text| text.chars().count() > DEDICATED_BASE_PROMPT_MAX_CHARS)
    {
        return Err(String::from("ERR_DEDICATED_PROMPT_DRAFT_INPUT_TOO_LONG"));
    }
    let mut message = format!("<requirement>\n{requirement}\n</requirement>");
    if let Some(current_prompt) = current_prompt {
        message.push_str(&format!(
            "\n\n<current_prompt>\n{current_prompt}\n</current_prompt>"
        ));
    }
    if let Some(revision_request) = revision_request {
        message.push_str(&format!(
            "\n\n<revision_request>\n{revision_request}\n</revision_request>"
        ));
    }
    Ok(message)
}

/// Builds one text-only message.
/// 构建一条纯文本消息。
fn text_message(role: MessageRole, text: &str) -> AiMessage {
    AiMessage {
        role,
        content: vec![ContentItem::Text {
            text: text.to_string(),
        }],
        provider_context_items: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a draft request with the given material.
    /// 以给定材料构建起草请求。
    fn draft_request(
        requirement: &str,
        current_prompt: Option<&str>,
        revision_request: Option<&str>,
    ) -> DedicatedPromptDraftRequest {
        DedicatedPromptDraftRequest {
            provider_id: String::from("provider"),
            connection_id: String::from("connection"),
            model_id: String::from("model"),
            requirement: requirement.to_string(),
            current_prompt: current_prompt.map(str::to_string),
            revision_request: revision_request.map(str::to_string),
        }
    }

    /// Verifies the draft message carries only present material and rejects empty or oversized input.
    /// 验证起草消息只携带存在的材料，并拒绝空输入或超长输入。
    #[test]
    fn draft_message_tags_present_material() {
        let fresh =
            dedicated_prompt_draft_message(&draft_request("translate", None, None)).unwrap();
        assert!(fresh.contains("<requirement>"));
        assert!(!fresh.contains("<current_prompt>"));
        let revision = dedicated_prompt_draft_message(&draft_request(
            "",
            Some("You translate."),
            Some("also Italian"),
        ))
        .unwrap();
        assert!(revision.contains("<current_prompt>"));
        assert!(revision.contains("<revision_request>"));
        assert_eq!(
            dedicated_prompt_draft_message(&draft_request("  ", Some(" "), None)).unwrap_err(),
            "ERR_DEDICATED_PROMPT_DRAFT_EMPTY"
        );
        let oversized = "x".repeat(DEDICATED_BASE_PROMPT_MAX_CHARS + 1);
        assert_eq!(
            dedicated_prompt_draft_message(&draft_request(&oversized, None, None)).unwrap_err(),
            "ERR_DEDICATED_PROMPT_DRAFT_INPUT_TOO_LONG"
        );
    }
}
