//! Shared streaming adapter support helpers reused by provider-specific transport adapters.
//! 供各 Provider 专用传输适配器复用的共享流式适配器支持辅助模块。

use crate::agent_runtime::provider_protocol_bridge::{
    build_openai_compatible_turn_response, core_provider_turn_delta_to_runtime_delta,
    inject_previous_response_id_into_additional_params,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::{
    OpenAiCompatibleCoreTurnRequest, build_host_controlled_turn_request,
};
use futures_util::{Stream, StreamExt};
use reqwest::header::HeaderMap;
use rig::completion::{
    CompletionModel, CompletionRequestBuilder, ToolDefinition as RigToolDefinition,
};
use rig::streaming::StreamingCompletionResponse;
use std::fmt::Display;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
};
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderTurnDelta as CoreProviderTurnDelta,
    ProviderTurnResponse as CoreProviderTurnResponse,
};
use vulcan_tools_manager::ResolvedToolDefinition;

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

/// Forwards one streaming provider content stream into both the `vulcan-core` delta sink and the GUI live-delta sender.
/// 将单个流式 Provider 内容流同时转发到 `vulcan-core` 增量汇聚器与 GUI 实时增量发送器。
pub(crate) async fn forward_streaming_provider_deltas<S, St, C, E, F>(
    provider_id: &str,
    provider_label: &str,
    live_sender: &ProviderLiveDeltaSender,
    stream: &mut St,
    sink: &mut S,
    mut convert_content: F,
) -> Result<(), VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
    St: Stream<Item = Result<C, E>> + Unpin,
    E: Display,
    F: FnMut(C) -> Vec<CoreProviderTurnDelta>,
{
    let provider_label_prefix = format_provider_label_prefix(provider_label);
    while let Some(item) = stream.next().await {
        let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label_prefix}流式执行失败：{error}"),
        })?;
        for delta in convert_content(content) {
            sink.push_delta(delta.clone());
            let _ = live_sender.send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
        }
    }

    Ok(())
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

/// Builds one shared streaming completion request builder so provider adapters only supply protocol-specific clients and labels.
/// 构建一份共享流式 Completion 请求构建器，使各 Provider 适配器只需提供协议专用客户端与标签。
pub(crate) fn build_streaming_completion_request_builder<M>(
    model: &M,
    request: OpenAiCompatibleCoreTurnRequest,
    rig_tool_definitions: &[RigToolDefinition],
) -> CompletionRequestBuilder<M>
where
    M: CompletionModel,
{
    let mut request_builder = model
        .completion_request(request.current_prompt)
        .preamble(request.snapshot.system_prompt.clone())
        .messages(request.current_history)
        .temperature_opt(request.snapshot.temperature)
        .additional_params_opt(request.snapshot.additional_params.clone());

    if !rig_tool_definitions.is_empty() {
        request_builder = request_builder.tools(rig_tool_definitions.to_vec());
    }

    request_builder
}

/// Starts one shared streaming completion request and normalizes provider-specific stream bootstrap failures.
/// 启动一份共享流式 Completion 请求，并规范化 Provider 专用的流式启动失败。
pub(crate) async fn start_streaming_completion_request<M>(
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

/// Executes one shared OpenAI-like streaming completion turn so adapters only keep client creation and provider-specific conversion details.
/// 执行一次共享的 OpenAI 风格流式 Completion 回合，使适配器只保留客户端构建与 Provider 专用转换细节。
pub(crate) async fn execute_openai_like_streaming_turn<M, S, C, E, F>(
    provider_id: &str,
    provider_label: &str,
    model_id: &str,
    model: &M,
    request: OpenAiCompatibleCoreTurnRequest,
    rig_tool_definitions: &[RigToolDefinition],
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
        build_streaming_completion_request_builder(model, request, rig_tool_definitions);
    let mut stream =
        start_streaming_completion_request(provider_id, provider_label, request_builder).await?;

    forward_streaming_provider_deltas(
        provider_id,
        provider_label,
        live_sender,
        &mut stream,
        sink,
        convert_content,
    )
    .await?;

    Ok(build_openai_compatible_turn_response(
        provider_id,
        model_id,
        stream.message_id.clone(),
        &stream.choice,
    ))
}

/// Builds one standard host-controlled single-turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
/// 根据规范控制器快照构建一份标准宿主控制单回合请求，并立即转发结构化快照。
pub(crate) fn build_standard_streaming_request(
    provider_id: &str,
    model_id: &str,
    request_label: &str,
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    resolved_tools: &[ResolvedToolDefinition],
    request_headers: &HeaderMap,
    live_sender: &ProviderLiveDeltaSender,
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    build_host_controlled_turn_request(
        provider_id,
        model_id,
        request_label,
        snapshot,
        config,
        resolved_tools,
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
    resolved_tools: &[ResolvedToolDefinition],
    request_headers: &HeaderMap,
    live_sender: &ProviderLiveDeltaSender,
) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
    let effective_additional_params = inject_previous_response_id_into_additional_params(
        config.additional_params.clone(),
        &snapshot.continuation,
    );
    build_host_controlled_turn_request(
        provider_id,
        model_id,
        "OpenAI Responses",
        snapshot,
        config,
        resolved_tools,
        request_headers,
        live_sender,
        effective_additional_params,
    )
}
