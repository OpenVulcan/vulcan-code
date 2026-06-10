//! Host-controlled single-turn request assembly helpers reused by provider-specific streaming adapters.
//! 供各 Provider 专用流式适配器复用的宿主控制单回合请求装配辅助逻辑。

use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_request_turn_snapshot::build_openai_compatible_turn_request_snapshot;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::provider_config::ModelInterleavedCapability;
use reqwest::header::HeaderMap;
use serde_json::Value;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationMessage as CoreConversationMessage,
    ConversationPayload as CoreConversationPayload,
    ConversationSnapshot as CoreConversationSnapshot, CoreError as VulcanCoreError,
    ProviderContextInjection as CoreProviderContextInjection,
    ProviderPromptLayers as CoreProviderPromptLayers,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};

/// Serializable OpenAI-compatible single-turn request owned by `vulcan-core` while preserving provider-visible canonical messages for the final transport adapter.
/// 由 `vulcan-core` 持有的可序列化 OpenAI 兼容单回合请求，同时为最终传输适配器保留 Provider 可见的规范消息。
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct OpenAiCompatibleCoreTurnRequest {
    /// Structured request snapshot persisted for inspection and debugging.
    /// 被持久化用于检查与调试的结构化请求快照。
    #[serde(flatten)]
    pub(crate) snapshot: CoreProviderRequestSnapshot,
    /// Provider-visible history kept in canonical core shape for the final transport adapter.
    /// 为最终传输适配器保留的规范 core 形状 Provider 可见历史。
    #[serde(skip_serializing)]
    pub(crate) current_history: Vec<CoreConversationMessage>,
    /// Last outgoing prompt message kept in canonical core shape for the final transport adapter.
    /// 为最终传输适配器保留的规范 core 形状最后一条出站提示消息。
    #[serde(skip_serializing)]
    pub(crate) current_prompt: CoreConversationMessage,
    /// Optional model-scoped interleaved reasoning capability reused by OpenAI-compatible replay code.
    /// 供 OpenAI 兼容回放逻辑复用的可选模型级交错推理能力描述。
    #[serde(skip_serializing)]
    pub(crate) interleaved_reasoning: Option<ModelInterleavedCapability>,
}

/// Builds one exact host-controlled turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
/// 根据规范控制器快照构建一份精确宿主控制回合请求，并立即转发结构化快照。
pub(crate) fn build_host_controlled_turn_request(
    provider_id: &str,
    model_id: &str,
    request_label: &str,
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    provider_tool_definitions: &[ProviderToolDefinition],
    request_headers: &HeaderMap,
    live_sender: &ProviderLiveDeltaSender,
    additional_params: Option<Value>,
    interleaved_reasoning: Option<ModelInterleavedCapability>,
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    let Some((current_prompt_message, current_history_messages)) = snapshot.messages.split_last()
    else {
        return Err(VulcanCoreError::InvalidState {
            message: format!(
                "{request_label} 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。"
            ),
        });
    };
    let mut current_history = current_history_messages.to_vec();
    let mut current_prompt = current_prompt_message.clone();
    apply_context_injection_overlay(
        &mut current_history,
        &mut current_prompt,
        config.context_injection.as_ref(),
    );
    let provider_system_prompt = provider_system_prompt_with_persistent_injection(config);
    let provider_prompt_layers = provider_prompt_layers_with_persistent_injection(config);
    let request_snapshot = build_openai_compatible_turn_request_snapshot(
        provider_id,
        model_id,
        provider_system_prompt.as_str(),
        &snapshot.continuation,
        &current_history,
        &current_prompt,
        config.temperature,
        config.max_turns,
        provider_tool_definitions,
        request_headers,
        additional_params,
    );
    let request_snapshot = CoreProviderRequestSnapshot {
        prompt_layers: provider_prompt_layers,
        ..request_snapshot
    };
    let _ = live_sender.send(Ok(ProviderStreamDelta::RequestSnapshot(
        request_snapshot.clone(),
    )));

    Ok(OpenAiCompatibleCoreTurnRequest {
        snapshot: request_snapshot,
        current_history,
        current_prompt,
        interleaved_reasoning,
    })
}

/// Builds the provider-facing system prompt with persistent injection rendered into the system layer.
/// 构建 Provider 可见系统提示词，并将长期注入渲染进系统层。
fn provider_system_prompt_with_persistent_injection(config: &CoreControllerConfig) -> String {
    let base = config
        .prompt_layers
        .render_or_legacy(config.system_prompt.as_str());
    append_persistent_injection_to_system_prompt(base, config.context_injection.as_ref())
}

/// Builds request-preview prompt layers with persistent injection carried by runtime context.
/// 构建请求预览提示词层，并通过运行时上下文携带长期注入。
fn provider_prompt_layers_with_persistent_injection(
    config: &CoreControllerConfig,
) -> CoreProviderPromptLayers {
    let mut layers = if config.prompt_layers.is_empty() {
        CoreProviderPromptLayers::from_system_prompt(config.system_prompt.clone())
    } else {
        config.prompt_layers.clone()
    };
    let Some(injection) = config
        .context_injection
        .as_ref()
        .filter(|injection| !injection.persistent_blocks.is_empty())
    else {
        return layers;
    };
    let persistent_text = injection.persistent_blocks.join("\n\n");
    if persistent_text.trim().is_empty() {
        return layers;
    }
    let existing_runtime_context = layers.runtime_context.trim();
    layers.runtime_context = if existing_runtime_context.is_empty() {
        persistent_text
    } else {
        format!("{existing_runtime_context}\n\n{persistent_text}")
    };
    layers
}

/// Appends persistent injection blocks to a rendered system prompt.
/// 将长期注入块追加到已渲染的系统提示词。
fn append_persistent_injection_to_system_prompt(
    system_prompt: String,
    injection: Option<&CoreProviderContextInjection>,
) -> String {
    let Some(injection) = injection.filter(|injection| !injection.persistent_blocks.is_empty())
    else {
        return system_prompt;
    };
    let persistent_text = injection.persistent_blocks.join("\n\n");
    if persistent_text.trim().is_empty() {
        return system_prompt;
    }
    let trimmed_system_prompt = system_prompt.trim();
    if trimmed_system_prompt.is_empty() {
        persistent_text
    } else {
        format!("{trimmed_system_prompt}\n\n{persistent_text}")
    }
}

/// Applies the provider-facing injection overlay to the outgoing request view without mutating canonical history.
/// 将 Provider 可见注入覆盖层应用到出站请求视图，同时不修改规范历史。
fn apply_context_injection_overlay(
    current_history: &mut Vec<CoreConversationMessage>,
    current_prompt: &mut CoreConversationMessage,
    injection: Option<&CoreProviderContextInjection>,
) {
    let Some(injection) = injection.filter(|injection| !injection.is_empty()) else {
        return;
    };
    let mut prefixes = Vec::new();
    if injection.has_transient() {
        prefixes.push(injection.transient_prefix.clone());
    }
    let prefix = prefixes.join("\n\n");
    if !prefix.trim().is_empty()
        && !prefix_target_user_message(current_history, current_prompt, &prefix)
    {
        prefix_user_text_message(current_prompt, prefix.as_str());
    }
}

/// Prefixes the provider-facing user text that owns the current logical turn.
/// 前置拥有当前逻辑回合的 Provider 可见用户文本。
fn prefix_target_user_message(
    current_history: &mut [CoreConversationMessage],
    current_prompt: &mut CoreConversationMessage,
    prefix: &str,
) -> bool {
    if prefix_user_text_message(current_prompt, prefix) {
        return true;
    }
    current_history
        .iter_mut()
        .rev()
        .filter(|message| !message.id.starts_with("context-injection-"))
        .any(|message| prefix_user_text_message(message, prefix))
}

/// Prefixes one outgoing user-text message with injection text.
/// 使用注入文本前置一条出站用户文本消息。
fn prefix_user_text_message(message: &mut CoreConversationMessage, prefix: &str) -> bool {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return false;
    }
    let CoreConversationPayload::UserText { text, .. } = &mut message.payload else {
        return false;
    };
    let trimmed_text = text.trim();
    *text = if trimmed_text.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}\n\n<user_prompt>\n{trimmed_text}\n</user_prompt>")
    };
    true
}

#[cfg(test)]
mod tests {
    //! Context-injection overlay tests for provider turn request assembly.
    //! Provider 回合请求装配的上下文注入覆盖层测试。

    use super::*;

    /// Builds one canonical user-text message for overlay tests.
    /// 为覆盖层测试构建一条规范用户文本消息。
    fn user_message(id: &str, text: &str) -> CoreConversationMessage {
        CoreConversationMessage {
            id: id.to_string(),
            payload: CoreConversationPayload::UserText {
                text: text.to_string(),
                attachments: Vec::new(),
            },
        }
    }

    /// Builds one canonical assistant message for overlay tests.
    /// 为覆盖层测试构建一条规范助手消息。
    fn assistant_message(id: &str) -> CoreConversationMessage {
        CoreConversationMessage {
            id: id.to_string(),
            payload: CoreConversationPayload::Assistant {
                text: String::from("tool call"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        }
    }

    #[test]
    fn transient_prefix_targets_current_user_prompt() {
        let mut history = vec![assistant_message("assistant-1")];
        let mut prompt = user_message("user-2", "真正问题");
        let injection = CoreProviderContextInjection {
            persistent_blocks: Vec::new(),
            transient_prefix: String::from("<manual_inject>\n隐藏内容\n</manual_inject>"),
            fingerprint: String::from("abc"),
        };

        apply_context_injection_overlay(&mut history, &mut prompt, Some(&injection));

        let CoreConversationPayload::UserText { text, .. } = prompt.payload else {
            panic!("prompt should stay user text");
        };
        assert!(text.starts_with("<manual_inject>"));
        assert!(text.contains("<user_prompt>\n真正问题\n</user_prompt>"));
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn tool_loop_overlay_prefixes_original_user_without_breaking_tool_prompt() {
        let mut history = vec![
            user_message("user-1", "原始问题"),
            assistant_message("assistant-1"),
        ];
        let mut prompt = CoreConversationMessage {
            id: String::from("tool-result-1"),
            payload: CoreConversationPayload::ToolResult {
                tool_result_id: String::from("tool-result-1"),
                tool_call_id: Some(String::from("tool-call-1")),
                text: String::from("工具返回"),
            },
        };
        let injection = CoreProviderContextInjection {
            persistent_blocks: vec![String::from("<persistent>\n固定内容\n</persistent>")],
            transient_prefix: String::from("<manual_inject>\n临时内容\n</manual_inject>"),
            fingerprint: String::from("def"),
        };

        apply_context_injection_overlay(&mut history, &mut prompt, Some(&injection));

        let CoreConversationPayload::UserText { text, .. } = &history[0].payload else {
            panic!("first history item should stay user text");
        };
        assert!(!text.contains("<persistent>"));
        assert!(text.contains("<manual_inject>"));
        assert!(matches!(
            prompt.payload,
            CoreConversationPayload::ToolResult { .. }
        ));
    }

    #[test]
    fn persistent_injection_renders_into_system_prompt() {
        let config = CoreControllerConfig {
            system_prompt: String::from("base"),
            context_injection: Some(CoreProviderContextInjection {
                persistent_blocks: vec![String::from("<persistent>\n固定内容\n</persistent>")],
                transient_prefix: String::new(),
                fingerprint: String::from("p"),
            }),
            ..CoreControllerConfig::default()
        };

        let system_prompt = provider_system_prompt_with_persistent_injection(&config);
        let prompt_layers = provider_prompt_layers_with_persistent_injection(&config);

        assert!(system_prompt.starts_with("base"));
        assert!(system_prompt.contains("<persistent>"));
        assert!(prompt_layers.runtime_context.contains("<persistent>"));
    }
}
