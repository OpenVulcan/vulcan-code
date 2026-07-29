//! Structured provider request snapshot builders reused by runtime bootstrap and regression tests.
//! 供运行时 bootstrap 与回归测试复用的结构化 Provider 请求快照构建模块。

use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionProviderVisibleAgentMessage,
};
use crate::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot;
use crate::agent_runtime::provider_request_bridge_messages::core_messages_to_provider_visible_history_for_model_with_system_updates;
use crate::agent_runtime::provider_request_bridge_tools::resolved_tool_definition_to_core_tool_schema;
use crate::agent_runtime::provider_request_controller_config::build_core_controller_config_with_layers;
use crate::agent_runtime::provider_request_snapshot_preview::CoreRequestPreviewProviderAdapter;
use crate::agent_runtime::provider_system_update_lowering::ProviderSystemUpdateLoweringPolicy;
use crate::agent_runtime::provider_visible_agent_messages::append_provider_visible_agent_messages_to_core_snapshot;
use crate::conversation_protocol::SessionConversationState;
use vulcan_core::types::{
    ConversationPayload as CoreConversationPayload,
    ProviderPromptLayers as CoreProviderPromptLayers,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};
use vulcan_tools_manager::ResolvedToolDefinition;

/// Builds one `vulcan-core` request snapshot from the current execution preparation and canonical session state.
/// 根据当前执行准备信息与规范会话状态构建一份 `vulcan-core` 请求快照。
pub(crate) async fn build_core_provider_request_snapshot(
    preparation: &ExecutionPreparation,
    conversation_state: &SessionConversationState,
    system_preamble: &str,
    prompt_layers: CoreProviderPromptLayers,
    prompt: &str,
    provider_visible_agent_messages: &[ExecutionProviderVisibleAgentMessage],
    resolved_tools: &[ResolvedToolDefinition],
    max_turns_override: Option<usize>,
) -> Result<CoreProviderRequestSnapshot, String> {
    let tool_schemas = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_core_tool_schema)
        .collect::<Vec<_>>();
    let mut core_snapshot = build_core_conversation_snapshot(conversation_state)?;
    core_snapshot.messages =
        core_messages_to_provider_visible_history_for_model_with_system_updates(
            &core_snapshot.messages,
            Some(preparation.model_blueprint.protocol_profile_id.as_str()),
            Some(&preparation.model_blueprint.public_capabilities),
            &preparation.runtime_system_updates,
            ProviderSystemUpdateLoweringPolicy::default(),
        )?;
    append_provider_visible_agent_messages_to_core_snapshot(
        &mut core_snapshot,
        provider_visible_agent_messages,
    )?;
    let mut controller = vulcan_core::controller::VulcanCoreController::new(
        build_core_controller_config_with_layers(
            preparation,
            system_preamble,
            prompt_layers,
            max_turns_override,
            None,
        ),
        core_snapshot,
    )
    .map_err(|error| format!("Invalid core conversation snapshot: {error}"))?;
    let preview_prompt = provider_request_snapshot_prompt(prompt, provider_visible_agent_messages);
    let adapter = CoreRequestPreviewProviderAdapter::new(
        preparation.model_blueprint.provider_id.clone(),
        preparation.model_blueprint.protocol_profile_id.clone(),
        preparation.model_blueprint.model_id.clone(),
        preview_prompt,
        None,
        tool_schemas,
    );

    controller
        .build_request(&adapter)
        .await
        .map_err(|error| error.to_string())
}

/// Builds the prompt field retained in request snapshots for user and seeded-agent turns.
/// 为用户回合与已播种智能体回合构建请求快照中保留的 prompt 字段。
fn provider_request_snapshot_prompt(
    prompt: &str,
    provider_visible_agent_messages: &[ExecutionProviderVisibleAgentMessage],
) -> String {
    let agent_message_prompt = provider_visible_agent_messages
        .iter()
        .map(|message| {
            CoreConversationPayload::render_agent_message_text(
                &message.message_type,
                &message.task_name,
                &message.sender,
                &message.payload,
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if !agent_message_prompt.trim().is_empty() {
        return agent_message_prompt;
    }
    prompt.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one seeded child-agent message for request snapshot prompt tests.
    /// 为请求快照提示词测试构建一条已播种子智能体消息。
    fn seeded_agent_message(payload: &str) -> ExecutionProviderVisibleAgentMessage {
        ExecutionProviderVisibleAgentMessage {
            delivery_id: String::from("delivery-1"),
            message_type: String::from("FINAL_ANSWER"),
            task_name: String::from("child_agent"),
            sender: String::from("child_agent"),
            recipient: String::from("parent:session-1"),
            payload: payload.to_string(),
            trigger_turn: true,
            timestamp_ms: 1,
            metadata: None,
        }
    }

    /// Prefers seeded child-agent messages over hidden lifecycle prompts.
    /// 优先使用已播种子智能体消息而不是隐藏生命周期提示。
    #[test]
    fn request_snapshot_prompt_prefers_seeded_agent_message() {
        let prompt = provider_request_snapshot_prompt(
            "Mailbox-triggered agent message turn.",
            &[seeded_agent_message("子代理完成。")],
        );

        assert!(prompt.contains("<agent_message>"));
        assert!(prompt.contains("子代理完成。"));
        assert!(!prompt.contains("Mailbox-triggered"));
    }

    /// Keeps normal user prompts when no seeded child-agent messages exist.
    /// 在不存在已播种子智能体消息时保留普通用户提示词。
    #[test]
    fn request_snapshot_prompt_keeps_user_prompt_without_seeded_messages() {
        let prompt = provider_request_snapshot_prompt("  请继续  ", &[]);

        assert_eq!(prompt, "请继续");
    }
}
