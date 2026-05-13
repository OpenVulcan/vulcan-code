//! Rig-specific completion-request execution helpers that keep third-party request builders and streaming response types at the transport edge only.
//! 仅在传输边界保留第三方请求构建器与流式响应类型的 rig 专用 Completion 执行辅助模块。

use crate::agent_runtime::provider_protocol_rig_content::build_openai_compatible_turn_response_from_rig;
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_adapter_support::forward_streaming_provider_deltas;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_rig_messages::{
    core_message_to_rig_message, core_messages_to_rig_history,
};
use crate::agent_runtime::provider_transport_rig_tool_definition::provider_tool_definition_to_rig_tool_definition;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use futures_util::Stream;
use rig::completion::{CompletionModel, CompletionRequestBuilder};
use rig::streaming::StreamingCompletionResponse;
use std::fmt::Display;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderTurnDelta as CoreProviderTurnDelta,
    ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Builds one shared rig streaming completion request builder so provider adapters only supply transport-specific clients and labels.
/// 构建一份共享的 rig 流式 Completion 请求构建器，使各 Provider 适配器只需提供传输专用客户端与标签。
pub(crate) fn build_rig_streaming_completion_request_builder<M>(
    model: &M,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> CompletionRequestBuilder<M>
where
    M: CompletionModel,
{
    let mut request_builder = model
        .completion_request(core_message_to_rig_message(&request.current_prompt))
        .preamble(request.snapshot.system_prompt.clone())
        .messages(core_messages_to_rig_history(&request.current_history))
        .temperature_opt(request.snapshot.temperature)
        .additional_params_opt(request.snapshot.additional_params.clone());

    if !provider_tool_definitions.is_empty() {
        let rig_tool_definitions = provider_tool_definitions
            .iter()
            .map(provider_tool_definition_to_rig_tool_definition)
            .collect::<Vec<_>>();
        request_builder = request_builder.tools(rig_tool_definitions);
    }

    request_builder
}

/// Starts one shared rig streaming completion request and normalizes provider-specific stream bootstrap failures.
/// 启动一份共享的 rig 流式 Completion 请求，并规范化 Provider 专用的流式启动失败。
pub(crate) async fn start_rig_streaming_completion_request<M>(
    provider_id: &str,
    provider_label: &str,
    request_builder: CompletionRequestBuilder<M>,
) -> Result<StreamingCompletionResponse<M::StreamingResponse>, VulcanCoreError>
where
    M: CompletionModel,
{
    let provider_label_prefix = format_provider_label_prefix(provider_label);
    request_builder
        .stream()
        .await
        .map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label_prefix}流式请求启动失败：{error}"),
        })
}

/// Executes one shared rig-backed OpenAI-like streaming completion turn so adapters only keep client creation and provider-specific content conversion details.
/// 执行一次共享的 rig 驱动 OpenAI 风格流式 Completion 回合，使适配器只保留客户端构建与 Provider 专用内容转换细节。
pub(crate) async fn execute_rig_openai_like_streaming_turn<M, S, C, E, F>(
    provider_id: &str,
    provider_label: &str,
    model_id: &str,
    model: &M,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    convert_content: F,
) -> Result<CoreProviderTurnResponse, VulcanCoreError>
where
    M: CompletionModel,
    S: VulcanProviderDeltaSink,
    E: Display,
    F: FnMut(C) -> Vec<CoreProviderTurnDelta>,
    StreamingCompletionResponse<M::StreamingResponse>: Stream<Item = Result<C, E>> + Unpin,
{
    let request_builder =
        build_rig_streaming_completion_request_builder(model, request, provider_tool_definitions);
    let mut stream =
        start_rig_streaming_completion_request(provider_id, provider_label, request_builder)
            .await?;

    forward_streaming_provider_deltas(
        provider_id,
        provider_label,
        live_sender,
        &mut stream,
        sink,
        convert_content,
    )
    .await?;

    Ok(build_openai_compatible_turn_response_from_rig(
        provider_id,
        model_id,
        stream.message_id.clone(),
        &stream.choice,
    ))
}

/// Formats one provider label prefix so mixed English/Chinese runtime messages keep natural spacing automatically inside rig-specific transport helpers.
/// 在 rig 专用传输辅助模块内部格式化单个 Provider 标签前缀，使中英混排运行时文案能够自动保持自然空格。
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
