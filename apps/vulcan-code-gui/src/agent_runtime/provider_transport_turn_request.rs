//! Host-controlled single-turn request assembly helpers reused by provider-specific streaming adapters.
//! 供各 Provider 专用流式适配器复用的宿主控制单回合请求装配辅助逻辑。

use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_request_turn_snapshot::build_openai_compatible_turn_request_snapshot;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use reqwest::header::HeaderMap;
use serde_json::Value;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationMessage as CoreConversationMessage,
    ConversationSnapshot as CoreConversationSnapshot, CoreError as VulcanCoreError,
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
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    let Some((current_prompt_message, current_history_messages)) = snapshot.messages.split_last()
    else {
        return Err(VulcanCoreError::InvalidState {
            message: format!(
                "{request_label} 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。"
            ),
        });
    };
    let current_history = current_history_messages.to_vec();
    let current_prompt = current_prompt_message.clone();
    let request_snapshot = build_openai_compatible_turn_request_snapshot(
        provider_id,
        model_id,
        config.system_prompt.as_str(),
        current_history_messages,
        current_prompt_message,
        config.temperature,
        config.max_turns,
        provider_tool_definitions,
        request_headers,
        additional_params,
    );
    let _ = live_sender.send(Ok(ProviderStreamDelta::RequestSnapshot(
        request_snapshot.clone(),
    )));

    Ok(OpenAiCompatibleCoreTurnRequest {
        snapshot: request_snapshot,
        current_history,
        current_prompt,
    })
}
