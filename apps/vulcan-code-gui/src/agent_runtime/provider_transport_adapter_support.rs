//! Shared streaming adapter support helpers reused by provider-specific transport adapters.
//! 供各 Provider 专用传输适配器复用的共享流式适配器支持辅助模块。

use crate::agent_runtime::provider_protocol_bridge::{
    inject_previous_response_id_into_additional_params, scope_continuation_to_active_provider,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::{
    OpenAiCompatibleCoreTurnRequest, build_host_controlled_turn_request,
};
use reqwest::header::HeaderMap;
use std::fmt::Display;
use vulcan_core::types::CoreError as VulcanCoreError;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
};

/// Formats one provider label prefix so mixed English/Chinese runtime messages keep natural spacing automatically.
/// 格式化单个 Provider 标签前缀，使中英混排运行时文案能够自动保持自然空格。
fn format_provider_label_prefix(provider_label: &str) -> String {
    let needs_space = provider_label
        .chars()
        .last()
        .map(|character| character.is_ascii_alphanumeric())
        .unwrap_or(false);
    if needs_space {
        format!("{provider_label} ")
    } else {
        provider_label.to_string()
    }
}

/// Builds the standard invalid-state error returned when one streaming-only adapter is asked to run the non-streaming execution path.
/// 当仅支持流式的适配器被要求执行非流式路径时，构建其返回的标准无效状态错误。
pub(crate) fn unsupported_streaming_execute<T>(provider_label: &str) -> Result<T, VulcanCoreError> {
    let provider_label_prefix = format_provider_label_prefix(provider_label);
    Err(VulcanCoreError::InvalidState {
        message: format!(
            "{provider_label_prefix}流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。"
        ),
    })
}

/// Maps one low-level provider client build result into the normalized `vulcan-core` provider failure shape.
/// 将单个低层 Provider 客户端构建结果映射成规范化的 `vulcan-core` Provider 失败形态。
pub(crate) fn map_provider_client_build_result<T, E>(
    provider_id: &str,
    provider_label: &str,
    result: Result<T, E>,
) -> Result<T, VulcanCoreError>
where
    E: Display,
{
    let provider_label_prefix = format_provider_label_prefix(provider_label);
    result.map_err(|error| VulcanCoreError::ProviderFailure {
        provider_id: provider_id.to_string(),
        message: format!("创建 {provider_label_prefix}客户端失败：{error}"),
    })
}

/// Builds one standard host-controlled single-turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
/// 根据规范控制器快照构建一份标准宿主控制单回合请求，并立即转发结构化快照。
pub(crate) fn build_standard_streaming_request(
    provider_id: &str,
    model_id: &str,
    request_label: &str,
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    provider_tool_definitions: &[ProviderToolDefinition],
    request_headers: &HeaderMap,
    live_sender: &ProviderLiveDeltaSender,
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    build_host_controlled_turn_request(
        provider_id,
        model_id,
        request_label,
        snapshot,
        config,
        provider_tool_definitions,
        request_headers,
        live_sender,
        config.additional_params.clone(),
    )
}

/// Builds one OpenAI Responses host-controlled single-turn request while injecting `previous_response_id` when the current continuation snapshot already carries one.
/// 构建一份 OpenAI Responses 宿主控制单回合请求，并在当前 continuation 快照已经携带上游响应标识时注入 `previous_response_id`。
pub(crate) fn build_openai_responses_streaming_request(
    provider_id: &str,
    model_id: &str,
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    provider_tool_definitions: &[ProviderToolDefinition],
    request_headers: &HeaderMap,
    live_sender: &ProviderLiveDeltaSender,
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    let scoped_continuation =
        scope_continuation_to_active_provider(&snapshot.continuation, provider_id, model_id);
    let effective_additional_params = inject_previous_response_id_into_additional_params(
        config.additional_params.clone(),
        &scoped_continuation,
    );
    build_host_controlled_turn_request(
        provider_id,
        model_id,
        "OpenAI Responses",
        snapshot,
        config,
        provider_tool_definitions,
        request_headers,
        live_sender,
        effective_additional_params,
    )
}
