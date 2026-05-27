//! Direct OpenAI-compatible HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 OpenAI 兼容 HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_attachment::{
    build_attachment_transport_error_text, build_missing_attachment_replay_text,
    read_provider_attachment,
};
use crate::agent_runtime::provider_prompt_layers::provider_fallback_system_prompt;
use crate::agent_runtime::provider_protocol_anthropic_decoders::sse::iter_sse_messages;
use crate::agent_runtime::provider_protocol_bridge::{
    build_openai_compatible_turn_response, core_provider_turn_delta_to_runtime_delta,
};
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_protocol_openai_compatible::{
    CompatibleAccumulatedToolCall, CompatibleChoiceData, CompatibleFinishReason,
    CompatibleToolCallChunk, append_tool_call_arguments, finalize_completed_streaming_tool_call,
    finalize_pending_tool_call, normalize_first_choice_chunk,
    should_evict_distinct_named_tool_call, tool_call_chunks,
};
use crate::agent_runtime::provider_protocol_tool_definition::{
    ProviderToolDefinition, normalize_provider_tool_parameters_schema,
};
use crate::agent_runtime::provider_transport_http_support::{
    build_json_http_client, build_rest_endpoint, ensure_bearer_authorization_header,
    ensure_json_content_type, ensure_string_header, format_standard_error_response,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::provider_transport_wire_capture::{
    emit_provider_wire_request, provider_request_dry_run_response,
};
use crate::provider_config::{
    ModelInterleavedCapability, OpenAiCompatibleStreamingUsageConfig,
    OpenAiCompatibleStreamingUsageSource, ProviderApiFamily, supported_provider_by_id,
};
use crate::provider_input_normalizer::normalize_provider_user_input;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde_json::{Map as JsonMap, Value, json};
use std::collections::{BTreeSet, HashMap};
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    ProviderTurnDelta as CoreProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Protocol tag stored inside provider_message_state for OpenAI-compatible native assistant payload replays.
/// 存入 provider_message_state 以支持 OpenAI 兼容原生助手载荷回放的协议标记。
const OPENAI_COMPATIBLE_PROVIDER_MESSAGE_STATE_PROTOCOL: &str = "openai-compatible";

/// Minimal OpenAI-compatible streaming function payload decoded from one delta chunk.
/// 从单条增量块解码出的最小 OpenAI 兼容流式函数载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingFunction {
    /// Optional streamed function name fragment.
    /// 可选的流式函数名称碎片。
    name: Option<String>,
    /// Optional streamed function-arguments fragment.
    /// 可选的流式函数参数碎片。
    arguments: Option<String>,
}

/// Minimal OpenAI-compatible streaming tool-call payload decoded from one delta chunk.
/// 从单条增量块解码出的最小 OpenAI 兼容流式工具调用载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingToolCall {
    /// Stable positional index used to merge multiple chunks for the same logical tool call.
    /// 用于合并同一逻辑工具调用多段碎片的稳定位置索引。
    index: usize,
    /// Optional upstream tool-call identifier.
    /// 可选的上游工具调用标识。
    id: Option<String>,
    /// Streamed function payload.
    /// 流式函数载荷。
    function: OpenAiCompatibleStreamingFunction,
}

impl From<&OpenAiCompatibleStreamingToolCall> for CompatibleToolCallChunk {
    /// Converts one streamed OpenAI-compatible tool-call fragment into the shared compatibility chunk shape.
    /// 将单条流式 OpenAI 兼容工具调用碎片转换为共享兼容层工具调用碎片形状。
    fn from(value: &OpenAiCompatibleStreamingToolCall) -> Self {
        Self {
            index: value.index,
            id: value.id.clone(),
            name: value.function.name.clone(),
            arguments: value.function.arguments.clone(),
        }
    }
}

/// Minimal OpenAI-compatible delta payload decoded from one streamed choice.
/// 从单条流式 choice 解码出的最小 OpenAI 兼容增量载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingDelta {
    /// Optional assistant text delta.
    /// 可选的助手文本增量。
    #[serde(default)]
    content: Option<String>,
    /// Optional plaintext reasoning delta exposed by providers that follow the OpenRouter/OpenAI `reasoning` alias.
    /// 采用 OpenRouter/OpenAI `reasoning` 别名的 Provider 暴露的可选纯文本思考增量。
    #[serde(default)]
    reasoning: Option<String>,
    /// Optional hidden reasoning delta carried by some OpenAI-compatible providers.
    /// 某些 OpenAI 兼容 Provider 携带的可选隐藏思考增量。
    #[serde(default)]
    reasoning_content: Option<String>,
    /// Optional structured reasoning payload exposed by providers that stream `reasoning_details`.
    /// 流式返回 `reasoning_details` 的 Provider 暴露的可选结构化思考载荷。
    #[serde(default)]
    reasoning_details: Option<Value>,
    /// Streamed tool-call fragments.
    /// 流式工具调用碎片集合。
    #[serde(default)]
    tool_calls: Vec<OpenAiCompatibleStreamingToolCall>,
    /// Additional provider-visible assistant fields that should survive interleaved replay verbatim.
    /// 应在交错回放中按原样保留的额外 Provider 可见助手字段。
    #[serde(default, flatten)]
    additional_fields: JsonMap<String, Value>,
}

/// Minimal OpenAI-compatible finish reason used by streamed choices.
/// 流式 choice 使用的最小 OpenAI 兼容结束原因。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum OpenAiCompatibleFinishReason {
    /// Tool calls have completed and should be flushed immediately.
    /// 工具调用已经结束并应立即冲刷。
    ToolCalls,
    /// Assistant output stopped normally.
    /// 助手输出正常结束。
    Stop,
    /// Provider filtered the content.
    /// Provider 过滤了内容。
    ContentFilter,
    /// Provider reached the length limit.
    /// Provider 达到了长度限制。
    Length,
    /// Any provider-specific finish reason not covered above.
    /// 上述枚举之外的任意 Provider 专用结束原因。
    #[serde(untagged)]
    Other(String),
}

/// Minimal OpenAI-compatible streamed choice payload.
/// 最小 OpenAI 兼容流式 choice 载荷。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingChoice {
    /// Streamed delta payload.
    /// 流式增量载荷。
    delta: OpenAiCompatibleStreamingDelta,
    /// Optional finish reason emitted by the provider.
    /// Provider 发出的可选结束原因。
    finish_reason: Option<OpenAiCompatibleFinishReason>,
    /// Optional provider usage payload nested inside one completed streamed choice.
    /// 可选的嵌套在单个已完成流式 choice 内部的 Provider 用量载荷。
    #[serde(default)]
    usage: Option<OpenAiCompatibleUsage>,
}

/// Minimal OpenAI-compatible streaming chunk decoded from one SSE `data:` payload.
/// 从单条 SSE `data:` 载荷解码出的最小 OpenAI 兼容流式块。
#[derive(Debug, Clone, Deserialize)]
struct OpenAiCompatibleStreamingChunk {
    /// Optional upstream response identifier.
    /// 可选的上游响应标识。
    id: Option<String>,
    /// Streamed choice list.
    /// 流式 choice 列表。
    #[serde(default)]
    choices: Vec<OpenAiCompatibleStreamingChoice>,
    /// Optional provider usage payload.
    /// 可选的 Provider 用量载荷。
    #[serde(default)]
    usage: Option<OpenAiCompatibleUsage>,
}

/// Extracts one visible reasoning delta from the current OpenAI-compatible stream payload while preserving provider field-precedence semantics.
/// 在保留 Provider 字段优先级语义的前提下，从当前 OpenAI 兼容流载荷中提取一段可见思考增量。
fn extract_openai_compatible_reasoning_delta(
    delta: &OpenAiCompatibleStreamingDelta,
) -> Option<String> {
    if delta.reasoning_content.is_some() {
        return delta.reasoning_content.clone();
    }

    if delta.reasoning.is_some() {
        return delta.reasoning.clone();
    }

    delta
        .reasoning_details
        .as_ref()
        .map(flatten_openai_compatible_reasoning_details)
}

/// Flattens one OpenAI-compatible `reasoning_details` payload into transcript-visible plaintext while skipping opaque items.
/// 将一份 OpenAI 兼容 `reasoning_details` 载荷压平成转录可见纯文本，并跳过不透明条目。
fn flatten_openai_compatible_reasoning_details(details: &Value) -> String {
    let mut parts = Vec::new();
    collect_openai_compatible_reasoning_detail_text(details, &mut parts);
    parts.join("\n")
}

/// Collects visible text recursively from one OpenAI-compatible reasoning-details fragment using the common `text` and `summary` shapes.
/// 使用常见的 `text` 与 `summary` 结构，从单个 OpenAI 兼容 reasoning-details 片段中递归收集可见文本。
fn collect_openai_compatible_reasoning_detail_text(value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_openai_compatible_reasoning_detail_text(item, parts);
            }
        }
        Value::Object(object) => {
            if let Some(text) = object.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
            if let Some(summary) = object.get("summary") {
                collect_openai_compatible_reasoning_detail_text(summary, parts);
            }
        }
        _ => {}
    }
}

/// Restores explicit provider-native reasoning replay fields that were deserialized into dedicated struct members instead of the flattened catch-all map.
/// 恢复那些被反序列化到专用结构成员、因此未进入扁平化兜底映射的 Provider 原生思考回放字段。
fn merge_openai_compatible_reasoning_replay_fields(
    fields: &mut JsonMap<String, Value>,
    delta: &OpenAiCompatibleStreamingDelta,
) {
    if let Some(reasoning) = delta.reasoning.clone() {
        fields
            .entry(String::from("reasoning"))
            .or_insert(Value::String(reasoning));
    }
    if let Some(reasoning_details) = delta.reasoning_details.clone() {
        fields
            .entry(String::from("reasoning_details"))
            .or_insert(reasoning_details);
    }
}

/// Default ordered OpenAI-compatible terminal usage sources used when one provider definition does not declare an explicit override.
/// 当供应商定义未声明显式覆盖时使用的默认 OpenAI 兼容终态 usage 来源顺序。
const DEFAULT_OPENAI_COMPATIBLE_STREAMING_USAGE_SOURCES: [OpenAiCompatibleStreamingUsageSource; 3] = [
    OpenAiCompatibleStreamingUsageSource::TopLevelEmptyChoicesChunk,
    OpenAiCompatibleStreamingUsageSource::TopLevelTerminalChunk,
    OpenAiCompatibleStreamingUsageSource::TerminalChoice,
];

/// Minimal OpenAI-compatible usage payload decoded from one streamed terminal chunk.
/// 从一条流式终态块中解码出的最小 OpenAI 兼容用量载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct OpenAiCompatibleUsage {
    /// Optional prompt token count.
    /// 可选的提示词 Token 数量。
    #[serde(default)]
    prompt_tokens: Option<usize>,
    /// Optional completion token count.
    /// 可选的补全文本 Token 数量。
    #[serde(default)]
    completion_tokens: Option<usize>,
    /// Optional total token count.
    /// 可选的总 Token 数量。
    #[serde(default)]
    total_tokens: Option<usize>,
    /// Optional completion-token detail payload.
    /// 可选的补全 Token 细分载荷。
    #[serde(default)]
    completion_tokens_details: Option<OpenAiCompatibleCompletionTokenDetails>,
    /// Optional prompt-token detail payload.
    /// 可选的提示词 Token 细分载荷。
    #[serde(default)]
    prompt_tokens_details: Option<OpenAiCompatiblePromptTokenDetails>,
}

/// Minimal OpenAI-compatible completion-token detail payload.
/// 最小 OpenAI 兼容补全 Token 细分载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct OpenAiCompatibleCompletionTokenDetails {
    /// Optional reasoning token count.
    /// 可选的推理 Token 数量。
    #[serde(default)]
    reasoning_tokens: Option<usize>,
}

/// Minimal OpenAI-compatible prompt-token detail payload.
/// 最小 OpenAI 兼容提示词 Token 细分载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct OpenAiCompatiblePromptTokenDetails {
    /// Optional cached-token count reported for prompt reuse.
    /// 为提示词复用报告的可选缓存命中 Token 数量。
    #[serde(default)]
    cached_tokens: Option<usize>,
}

/// Authorization strategy used by OpenAI-compatible wire protocols whose payload semantics are shared but whose auth headers may differ.
/// 供 OpenAI 兼容线协议复用、但可能具有不同鉴权头形式的鉴权策略。
#[derive(Debug, Clone, Copy)]
pub(crate) enum OpenAiCompatibleAuthorizationStrategy {
    /// Standard bearer-token authorization.
    /// 标准 Bearer Token 鉴权。
    Bearer,
    /// Static string header that carries the raw API key value.
    /// 直接承载原始 API Key 值的静态字符串请求头。
    StaticApiKeyHeader {
        /// Target header name.
        /// 目标请求头名称。
        header_name: &'static str,
        /// Human-readable header label used in diagnostics.
        /// 用于诊断文案的人类可读请求头标签。
        header_label: &'static str,
    },
}

/// Launch options used by one OpenAI-compatible wire-protocol turn whose transport details differ from the default OpenAI REST endpoint.
/// 供单次 OpenAI 兼容线协议回合使用、但其传输细节不同于默认 OpenAI REST 端点的启动配置。
#[derive(Debug, Clone)]
pub(crate) struct OpenAiCompatibleHttpStreamingOptions {
    /// Human-readable provider label used in diagnostics.
    /// 用于诊断文案的人类可读 Provider 标签。
    pub(crate) provider_label: String,
    /// Final REST endpoint used for the current request.
    /// 当前请求使用的最终 REST 端点。
    pub(crate) endpoint: String,
    /// Whether the request payload should keep the top-level `model` field.
    /// 请求载荷是否应保留顶层 `model` 字段。
    pub(crate) include_model_field: bool,
    /// Authorization strategy used by the concrete provider variant.
    /// 具体 Provider 变体使用的鉴权策略。
    pub(crate) authorization_strategy: OpenAiCompatibleAuthorizationStrategy,
}

/// Executes one direct OpenAI-compatible streaming turn without going through any third-party completion-model abstraction.
/// 在不经过任何第三方 completion model 抽象的前提下执行一次直连 OpenAI 兼容流式回合。
pub(crate) async fn execute_openai_compatible_http_streaming_turn<S>(
    provider_id: &str,
    provider_label: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    request_headers: HeaderMap,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
) -> Result<CoreProviderTurnResponse, VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
{
    execute_openai_compatible_http_streaming_turn_with_options(
        provider_id,
        api_key,
        model_id,
        request_headers,
        request,
        provider_tool_definitions,
        live_sender,
        sink,
        OpenAiCompatibleHttpStreamingOptions {
            provider_label: provider_label.to_string(),
            endpoint: build_openai_compatible_chat_completions_endpoint(base_url),
            include_model_field: true,
            authorization_strategy: OpenAiCompatibleAuthorizationStrategy::Bearer,
        },
    )
    .await
}

/// Executes one direct OpenAI-compatible wire-protocol turn using caller-supplied transport options such as auth strategy and final endpoint.
/// 使用调用方提供的鉴权策略与最终端点等传输配置执行一次直连 OpenAI 兼容线协议回合。
pub(crate) async fn execute_openai_compatible_http_streaming_turn_with_options<S>(
    provider_id: &str,
    api_key: &str,
    model_id: &str,
    request_headers: HeaderMap,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    options: OpenAiCompatibleHttpStreamingOptions,
) -> Result<CoreProviderTurnResponse, VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
{
    let OpenAiCompatibleHttpStreamingOptions {
        provider_label,
        endpoint,
        include_model_field,
        authorization_strategy,
    } = options;
    let mut wire_headers = request_headers.clone();
    match authorization_strategy {
        OpenAiCompatibleAuthorizationStrategy::Bearer => ensure_bearer_authorization_header(
            provider_id,
            provider_label.as_str(),
            api_key,
            &mut wire_headers,
        )?,
        OpenAiCompatibleAuthorizationStrategy::StaticApiKeyHeader {
            header_name,
            header_label,
        } => ensure_string_header(
            provider_id,
            provider_label.as_str(),
            &mut wire_headers,
            header_name,
            api_key,
            header_label,
        )?,
    }
    ensure_json_content_type(&mut wire_headers);
    let client = build_openai_compatible_http_client(
        provider_id,
        provider_label.as_str(),
        api_key,
        request_headers,
        authorization_strategy,
    )?;
    let payload = build_openai_compatible_request_payload(
        model_id,
        request,
        provider_tool_definitions,
        include_model_field,
    )?;
    let wire_request = emit_provider_wire_request(
        live_sender,
        provider_id,
        model_id,
        "POST",
        endpoint.as_str(),
        &wire_headers,
        &payload,
    );
    if let Some(response) = provider_request_dry_run_response(&wire_request) {
        return Ok(response);
    }
    let mut response = client
        .post(endpoint)
        .json(&payload)
        .send()
        .await
        .map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} 请求发送失败：{error}"),
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = augment_openai_compatible_error_with_tool_context(
            format_standard_error_response(status, &body),
            &body,
            provider_tool_definitions,
        );
        return Err(VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} request failed: {message}"),
        });
    }

    let byte_stream = Box::pin(async_stream::stream! {
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => yield Ok::<Vec<u8>, std::io::Error>(chunk.to_vec()),
                Ok(None) => break,
                Err(error) => {
                    yield Err(std::io::Error::other(error.to_string()));
                    break;
                }
            }
        }
    });

    let events = iter_sse_messages(byte_stream);
    futures_util::pin_mut!(events);

    let mut response_id = None;
    let mut assistant_text = String::new();
    let mut assistant_reasoning = String::new();
    let mut pending_tool_calls: HashMap<usize, CompatibleAccumulatedToolCall> = HashMap::new();
    let mut finalized_tool_calls = Vec::new();
    let mut final_usage = None::<CoreProviderUsageSnapshot>;
    let mut provider_visible_assistant_fields = JsonMap::new();
    let mut saw_reasoning_field = false;
    let mut raw_reasoning_text = String::new();
    let usage_policy = resolve_openai_compatible_streaming_usage_policy(provider_id);

    while let Some(event_result) = events.next().await {
        let event = event_result.map_err(|error| VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} SSE 解码失败：{error}"),
        })?;

        if event.data.trim().is_empty() || event.data.trim() == "[DONE]" {
            continue;
        }

        let chunk = normalize_openai_compatible_chunk_with_policy(
            provider_id,
            provider_label.as_str(),
            &event.data,
            &usage_policy,
        )?;
        response_id = chunk.response_id.or(response_id);
        if let Some(usage) = chunk
            .usage
            .filter(CoreProviderUsageSnapshot::has_any_tokens)
        {
            final_usage = Some(usage);
        }
        let Some(choice) = chunk.choice else {
            continue;
        };

        for (field_name, field_value) in choice.provider_visible_assistant_fields {
            merge_openai_compatible_assistant_field(
                &mut provider_visible_assistant_fields,
                field_name,
                field_value,
            );
        }

        process_compatible_tool_call_chunks(
            &mut pending_tool_calls,
            &mut finalized_tool_calls,
            choice.tool_calls,
        );

        if let Some(reasoning) = choice.reasoning {
            saw_reasoning_field = true;
            raw_reasoning_text.push_str(&reasoning);
            if !reasoning.is_empty() {
                assistant_reasoning.push_str(&reasoning);
                forward_openai_compatible_delta(
                    live_sender,
                    sink,
                    CoreProviderTurnDelta::Thinking { delta: reasoning },
                );
            }
        }

        if let Some(text) = choice.text
            && !text.is_empty()
        {
            assistant_text.push_str(&text);
            forward_openai_compatible_delta(
                live_sender,
                sink,
                CoreProviderTurnDelta::Output { delta: text },
            );
        }

        if choice.finish_reason == CompatibleFinishReason::ToolCalls {
            flush_pending_compatible_tool_calls(&mut pending_tool_calls, &mut finalized_tool_calls);
        }
    }

    flush_pending_compatible_tool_calls(&mut pending_tool_calls, &mut finalized_tool_calls);

    let provider_message_state = build_openai_compatible_provider_message_state(
        &assistant_text,
        &finalized_tool_calls,
        saw_reasoning_field.then_some(raw_reasoning_text),
        provider_visible_assistant_fields,
    );
    let mut assistant_content = Vec::new();
    if !assistant_text.is_empty() {
        assistant_content.push(ProviderAssistantContent::Text(assistant_text));
    }
    if !assistant_reasoning.is_empty() {
        assistant_content.push(ProviderAssistantContent::Reasoning(vec![
            ProviderReasoningPart::Text(assistant_reasoning),
        ]));
    }
    assistant_content.extend(
        finalized_tool_calls
            .into_iter()
            .map(ProviderAssistantContent::ToolCall),
    );

    let mut response = build_openai_compatible_turn_response(
        provider_id,
        model_id,
        response_id,
        &assistant_content,
        final_usage,
    );
    response.provider_message_state = provider_message_state;
    Ok(response)
}

/// Builds one direct OpenAI-compatible HTTP client with merged authorization and caller-supplied request headers.
/// 构建一个直连 OpenAI 兼容 HTTP 客户端，并合并鉴权头与调用方提供的请求头。
fn build_openai_compatible_http_client(
    provider_id: &str,
    provider_label: &str,
    api_key: &str,
    mut request_headers: HeaderMap,
    authorization_strategy: OpenAiCompatibleAuthorizationStrategy,
) -> Result<reqwest::Client, VulcanCoreError> {
    match authorization_strategy {
        OpenAiCompatibleAuthorizationStrategy::Bearer => ensure_bearer_authorization_header(
            provider_id,
            provider_label,
            api_key,
            &mut request_headers,
        )?,
        OpenAiCompatibleAuthorizationStrategy::StaticApiKeyHeader {
            header_name,
            header_label,
        } => ensure_string_header(
            provider_id,
            provider_label,
            &mut request_headers,
            header_name,
            api_key,
            header_label,
        )?,
    }
    ensure_json_content_type(&mut request_headers);
    build_json_http_client(provider_id, provider_label, request_headers)
}

/// Builds the final OpenAI-compatible `/chat/completions` endpoint from one optional base URL override.
/// 根据可选基础地址覆盖值构建最终 OpenAI 兼容 `/chat/completions` 端点。
fn build_openai_compatible_chat_completions_endpoint(base_url: Option<&str>) -> String {
    if let Some(base_url) = base_url
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().trim_end_matches('/'))
    {
        if base_url.ends_with("/chat/completions") {
            return base_url.to_string();
        }
    }

    build_rest_endpoint(base_url, "https://api.openai.com/v1", "chat/completions")
}

/// Builds the final OpenAI-compatible JSON payload from one host-controlled turn request and tool schema set.
/// 根据一份宿主控制回合请求与工具 schema 集合构建最终 OpenAI 兼容 JSON 载荷。
fn build_openai_compatible_request_payload(
    model_id: &str,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
    include_model_field: bool,
) -> Result<Value, VulcanCoreError> {
    let OpenAiCompatibleCoreTurnRequest {
        snapshot,
        current_history,
        current_prompt,
        interleaved_reasoning,
    } = request;
    let usage_policy =
        resolve_openai_compatible_streaming_usage_policy(snapshot.provider_id.as_str());

    let system_prompt = provider_fallback_system_prompt(&snapshot);
    let mut payload = match snapshot.additional_params {
        Some(Value::Object(object)) => object,
        Some(other) => {
            let mut object = JsonMap::new();
            object.insert(String::from("host_additional_params"), other);
            object
        }
        None => JsonMap::new(),
    };

    let mut messages = Vec::new();
    if !system_prompt.trim().is_empty() {
        messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));
    }
    messages.extend(
        current_history
            .iter()
            .map(|message| {
                core_message_to_openai_compatible_message(message, interleaved_reasoning.as_ref())
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    messages.push(core_message_to_openai_compatible_message(
        &current_prompt,
        interleaved_reasoning.as_ref(),
    )?);

    if include_model_field {
        payload.insert(String::from("model"), Value::String(model_id.to_string()));
    }
    payload.insert(String::from("messages"), Value::Array(messages));
    payload.insert(String::from("stream"), Value::Bool(true));
    ensure_openai_compatible_stream_usage_requested(&mut payload, &usage_policy);

    if let Some(temperature) = snapshot.temperature {
        payload.insert(String::from("temperature"), json!(temperature));
    }

    if !provider_tool_definitions.is_empty() {
        let mut tools = payload
            .remove("tools")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default();
        tools.extend(
            provider_tool_definitions
                .iter()
                .map(provider_tool_definition_to_openai_compatible_tool),
        );
        payload.insert(String::from("tools"), Value::Array(tools));
    }

    Ok(Value::Object(payload))
}

/// Ensures OpenAI-compatible streaming requests ask the provider to emit the terminal usage chunk when the upstream protocol supports `stream_options.include_usage`.
/// 确保 OpenAI 兼容流式请求在上游协议支持 `stream_options.include_usage` 时明确请求返回终态 usage 块。
fn ensure_openai_compatible_stream_usage_requested(
    payload: &mut JsonMap<String, Value>,
    usage_policy: &OpenAiCompatibleStreamingUsageConfig,
) {
    if !usage_policy.request_include_usage {
        return;
    }
    let mut stream_options = match payload.remove("stream_options") {
        Some(Value::Object(object)) => object,
        Some(_) | None => JsonMap::new(),
    };
    stream_options
        .entry(String::from("include_usage"))
        .or_insert(Value::Bool(true));
    payload.insert(
        String::from("stream_options"),
        Value::Object(stream_options),
    );
}

/// Converts one canonical core message into one OpenAI-compatible request message and replays provider-native interleaved state only when the active model explicitly declares compatible support.
/// 将一条规范 core 消息转换为一条 OpenAI 兼容请求消息，并且仅在当前模型显式声明兼容支持时回放 Provider 原生交错状态。
fn core_message_to_openai_compatible_message(
    message: &CoreConversationMessage,
    interleaved_reasoning: Option<&ModelInterleavedCapability>,
) -> Result<Value, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let normalized = normalize_provider_user_input(
                ProviderApiFamily::OpenAiChatCompatible,
                text,
                attachments,
            );
            if normalized.attachments.is_empty() {
                Ok(json!({
                    "role": "user",
                    "content": normalized.text
                }))
            } else {
                let mut content = Vec::new();
                if !normalized.text.is_empty() {
                    content.push(json!({
                        "type": "text",
                        "text": normalized.text
                    }));
                }
                for attachment in &normalized.attachments {
                    let attachment_payload = match read_provider_attachment(attachment)? {
                        Some(payload) => payload,
                        None => {
                            content.push(json!({
                                "type": "text",
                                "text": build_missing_attachment_replay_text(attachment)
                            }));
                            continue;
                        }
                    };
                    match attachment_payload.kind {
                        vulcan_core::types::ConversationAttachmentKind::Image => {
                            content.push(json!({
                                "type": "image_url",
                                "image_url": {
                                    "url": attachment_payload.data_url
                                }
                            }));
                        }
                        // OpenAI-compatible Chat endpoints are not uniform beyond text/image parts.
                        // OpenAI-Compatible Chat 端点在文本/图片以外的 Part 形态并不统一。
                        vulcan_core::types::ConversationAttachmentKind::Audio
                        | vulcan_core::types::ConversationAttachmentKind::Video
                        | vulcan_core::types::ConversationAttachmentKind::Pdf => {
                            content.push(json!({
                                "type": "text",
                                "text": build_attachment_transport_error_text(
                                    attachment,
                                    "this API path"
                                )
                            }));
                        }
                    }
                }
                Ok(json!({
                    "role": "user",
                    "content": content
                }))
            }
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_state,
            tool_calls,
            ..
        } => {
            let mut object =
                if openai_compatible_supports_native_interleaved_replay(interleaved_reasoning) {
                    provider_message_state
                        .as_ref()
                        .and_then(parse_openai_compatible_provider_message_state)
                        .unwrap_or_default()
                } else {
                    JsonMap::new()
                };
            object.insert(
                String::from("role"),
                Value::String(String::from("assistant")),
            );
            if !text.is_empty() && !object.contains_key("content") {
                object.insert(String::from("content"), Value::String(text.clone()));
            }
            if !tool_calls.is_empty() && !object.contains_key("tool_calls") {
                object.insert(
                    String::from("tool_calls"),
                    Value::Array(
                        tool_calls
                            .iter()
                            .map(core_tool_call_to_openai_compatible_tool_call)
                            .collect(),
                    ),
                );
            }
            if let Some(reasoning_field) =
                openai_compatible_textual_interleaved_reasoning_field(interleaved_reasoning)
                && !object.contains_key(reasoning_field)
            {
                object.insert(
                    reasoning_field.to_string(),
                    Value::String(reasoning.clone()),
                );
            }
            ensure_openai_compatible_assistant_message_has_required_payload(&mut object);
            Ok(Value::Object(object))
        }
        CoreConversationPayload::ToolResult {
            tool_call_id, text, ..
        } => {
            let Some(tool_call_id) = tool_call_id
                .clone()
                .filter(|value| !value.trim().is_empty())
            else {
                return Err(VulcanCoreError::InvalidState {
                    message: String::from(
                        "OpenAI 兼容工具结果消息缺少 tool_call_id，无法构造 provider 请求。",
                    ),
                });
            };
            Ok(json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": text
            }))
        }
    }
}

/// Converts one canonical core tool call into one OpenAI-compatible tool-call object with stringified JSON arguments.
/// 将一条规范 core 工具调用转换为一条 OpenAI 兼容工具调用对象，并把 JSON 参数字符串化。
fn core_tool_call_to_openai_compatible_tool_call(tool_call: &CoreConversationToolCall) -> Value {
    json!({
        "id": tool_call.id,
        "type": "function",
        "function": {
            "name": tool_call.tool_name,
            "arguments": stringify_openai_compatible_tool_arguments(&tool_call.arguments)
        }
    })
}

/// Converts one host-owned provider tool definition into one OpenAI-compatible tool schema object.
/// 将一条宿主自有 Provider 工具定义转换为一条 OpenAI 兼容工具 schema 对象。
fn provider_tool_definition_to_openai_compatible_tool(
    definition: &ProviderToolDefinition,
) -> Value {
    let mut parameters = definition.parameters.clone();
    normalize_provider_tool_parameters_schema(&mut parameters);

    json!({
        "type": "function",
        "function": {
            "name": definition.name,
            "description": definition.description,
            "parameters": parameters
        }
    })
}

/// Appends OpenAI-compatible function-tool index hints to one provider error summary when the upstream payload reports Gemini-style `function_declarations[n]` validation failures.
/// 当上游载荷报告 Gemini 风格的 `function_declarations[n]` 校验失败时，为单条 OpenAI 兼容 Provider 错误摘要追加函数工具索引提示。
fn augment_openai_compatible_error_with_tool_context(
    base_message: String,
    body: &str,
    provider_tool_definitions: &[ProviderToolDefinition],
) -> String {
    let tool_indices = collect_openai_compatible_function_declaration_indices(body);
    if tool_indices.is_empty() {
        return base_message;
    }

    let mut tool_segments = Vec::new();
    for index in tool_indices {
        let Some(definition) = provider_tool_definitions.get(index) else {
            continue;
        };
        tool_segments.push(format!("tool[{index}]={}", definition.name));
    }

    if tool_segments.is_empty() {
        return base_message;
    }

    format!("{base_message} | {}", tool_segments.join(" | "))
}

/// Collects unique upstream function-declaration indices from one provider error body by scanning for `function_declarations[n]` fragments.
/// 通过扫描 `function_declarations[n]` 片段，从单条 Provider 错误正文中收集唯一的上游函数声明索引。
fn collect_openai_compatible_function_declaration_indices(body: &str) -> Vec<usize> {
    let needle = "function_declarations[";
    let mut indices = BTreeSet::new();
    let mut search_offset = 0usize;

    while let Some(relative_start) = body[search_offset..].find(needle) {
        let start = search_offset + relative_start + needle.len();
        let Some(relative_end) = body[start..].find(']') else {
            break;
        };
        let end = start + relative_end;
        if let Ok(index) = body[start..end].trim().parse::<usize>() {
            indices.insert(index);
        }
        search_offset = end + 1;
    }

    indices.into_iter().collect()
}

/// Serializes one tool-arguments JSON value into the OpenAI-compatible string form expected inside assistant tool calls.
/// 将单个工具参数 JSON 值序列化为助手工具调用中期望的 OpenAI 兼容字符串形式。
fn stringify_openai_compatible_tool_arguments(arguments: &Value) -> String {
    match arguments {
        Value::String(value) => value.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// Normalizes one OpenAI-compatible SSE `data:` payload into the shared compatibility chunk shape.
/// 将单条 OpenAI 兼容 SSE `data:` 载荷规范化为共享兼容块形状。
#[cfg_attr(not(test), allow(dead_code))]
fn normalize_openai_compatible_chunk(
    provider_id: &str,
    provider_label: &str,
    data: &str,
) -> Result<
    crate::agent_runtime::provider_protocol_openai_compatible::CompatibleChunk,
    VulcanCoreError,
> {
    let usage_policy = resolve_openai_compatible_streaming_usage_policy(provider_id);
    normalize_openai_compatible_chunk_with_policy(provider_id, provider_label, data, &usage_policy)
}

/// Normalizes one OpenAI-compatible SSE `data:` payload into the shared compatibility chunk shape using one resolved provider usage policy.
/// 使用一份已解析的供应商 usage 策略，将单条 OpenAI 兼容 SSE `data:` 载荷规范化为共享兼容块形状。
fn normalize_openai_compatible_chunk_with_policy(
    provider_id: &str,
    provider_label: &str,
    data: &str,
    usage_policy: &OpenAiCompatibleStreamingUsageConfig,
) -> Result<
    crate::agent_runtime::provider_protocol_openai_compatible::CompatibleChunk,
    VulcanCoreError,
> {
    let chunk = serde_json::from_str::<OpenAiCompatibleStreamingChunk>(data).map_err(|error| {
        VulcanCoreError::ProviderFailure {
            provider_id: provider_id.to_string(),
            message: format!("{provider_label} 流式数据解析失败：{error}"),
        }
    })?;

    Ok(normalize_first_choice_chunk(
        chunk.id.clone(),
        extract_openai_compatible_terminal_usage(&chunk, usage_policy),
        &chunk.choices,
        |choice| {
            let mut provider_visible_assistant_fields = choice.delta.additional_fields.clone();
            merge_openai_compatible_reasoning_replay_fields(
                &mut provider_visible_assistant_fields,
                &choice.delta,
            );
            CompatibleChoiceData {
                finish_reason: if choice.finish_reason
                    == Some(OpenAiCompatibleFinishReason::ToolCalls)
                {
                    CompatibleFinishReason::ToolCalls
                } else {
                    CompatibleFinishReason::Other
                },
                text: choice.delta.content.clone(),
                reasoning: extract_openai_compatible_reasoning_delta(&choice.delta),
                provider_visible_assistant_fields,
                tool_calls: tool_call_chunks(&choice.delta.tool_calls),
            }
        },
    ))
}

/// Resolves one provider-scoped OpenAI-compatible streaming usage policy from the provider catalog and falls back to conservative terminal-only defaults.
/// 从供应商目录解析单个 Provider 作用域的 OpenAI 兼容流式 usage 策略，并在缺失时回退到保守的终态默认值。
fn resolve_openai_compatible_streaming_usage_policy(
    provider_id: &str,
) -> OpenAiCompatibleStreamingUsageConfig {
    supported_provider_by_id(provider_id)
        .and_then(|definition| definition.usage_capture.openai_compatible_streaming.clone())
        .unwrap_or_default()
}

/// Returns the effective ordered terminal usage sources for one OpenAI-compatible provider policy.
/// 返回单个 OpenAI 兼容 Provider 策略生效中的终态 usage 来源顺序。
fn effective_openai_compatible_streaming_usage_sources(
    usage_policy: &OpenAiCompatibleStreamingUsageConfig,
) -> &[OpenAiCompatibleStreamingUsageSource] {
    if usage_policy.response_sources.is_empty() {
        &DEFAULT_OPENAI_COMPATIBLE_STREAMING_USAGE_SOURCES
    } else {
        usage_policy.response_sources.as_slice()
    }
}

/// Extracts one terminal OpenAI-compatible usage snapshot from the current streamed chunk according to the resolved provider policy.
/// 按照已解析的供应商策略，从当前流式块中提取一份终态 OpenAI 兼容用量快照。
fn extract_openai_compatible_terminal_usage(
    chunk: &OpenAiCompatibleStreamingChunk,
    usage_policy: &OpenAiCompatibleStreamingUsageConfig,
) -> Option<CoreProviderUsageSnapshot> {
    for source in effective_openai_compatible_streaming_usage_sources(usage_policy) {
        let usage = match source {
            OpenAiCompatibleStreamingUsageSource::TopLevelEmptyChoicesChunk => {
                if chunk.choices.is_empty() {
                    chunk
                        .usage
                        .clone()
                        .and_then(openai_compatible_usage_to_snapshot)
                } else {
                    None
                }
            }
            OpenAiCompatibleStreamingUsageSource::TopLevelTerminalChunk => {
                if chunk
                    .choices
                    .iter()
                    .any(|choice| choice.finish_reason.is_some())
                {
                    chunk
                        .usage
                        .clone()
                        .and_then(openai_compatible_usage_to_snapshot)
                } else {
                    None
                }
            }
            OpenAiCompatibleStreamingUsageSource::TerminalChoice => chunk
                .choices
                .iter()
                .find(|choice| choice.finish_reason.is_some())
                .and_then(|choice| choice.usage.clone())
                .and_then(openai_compatible_usage_to_snapshot),
        };

        if usage
            .as_ref()
            .is_some_and(CoreProviderUsageSnapshot::has_any_tokens)
        {
            return usage;
        }
    }

    None
}

/// Builds one provider_message_state payload that preserves an OpenAI-compatible assistant message for future high-fidelity history replay.
/// 构建一份 provider_message_state 载荷，以便未来高保真历史回放时保留 OpenAI 兼容助手消息。
fn build_openai_compatible_provider_message_state(
    assistant_text: &str,
    finalized_tool_calls: &[CoreConversationToolCall],
    flattened_reasoning_text: Option<String>,
    mut provider_visible_fields: JsonMap<String, Value>,
) -> Option<Value> {
    if provider_visible_fields.is_empty() && flattened_reasoning_text.is_none() {
        return None;
    }

    provider_visible_fields.insert(
        String::from("role"),
        Value::String(String::from("assistant")),
    );
    if !assistant_text.is_empty() {
        provider_visible_fields.insert(
            String::from("content"),
            Value::String(assistant_text.to_string()),
        );
    }
    if let Some(reasoning_content) = flattened_reasoning_text.filter(|_| {
        !provider_visible_fields.contains_key("reasoning_content")
            && !provider_visible_fields.contains_key("reasoning")
            && !provider_visible_fields.contains_key("reasoning_details")
    }) {
        provider_visible_fields.insert(
            String::from("reasoning_content"),
            Value::String(reasoning_content),
        );
    }
    if !finalized_tool_calls.is_empty() {
        provider_visible_fields.insert(
            String::from("tool_calls"),
            Value::Array(
                finalized_tool_calls
                    .iter()
                    .map(core_tool_call_to_openai_compatible_tool_call)
                    .collect(),
            ),
        );
    }
    ensure_openai_compatible_assistant_message_has_required_payload(&mut provider_visible_fields);

    Some(Value::Object(JsonMap::from_iter([
        (
            String::from("protocol"),
            Value::String(String::from(
                OPENAI_COMPATIBLE_PROVIDER_MESSAGE_STATE_PROTOCOL,
            )),
        ),
        (
            String::from("assistant_message"),
            Value::Object(provider_visible_fields),
        ),
    ])))
}

/// Restores one previously persisted OpenAI-compatible provider_message_state into the raw assistant-message object expected by the wire protocol.
/// 将一份已持久化的 OpenAI 兼容 provider_message_state 恢复为线协议期望的原生助手消息对象。
fn parse_openai_compatible_provider_message_state(state: &Value) -> Option<JsonMap<String, Value>> {
    let object = state.as_object()?;
    if object.get("protocol")?.as_str()? != OPENAI_COMPATIBLE_PROVIDER_MESSAGE_STATE_PROTOCOL {
        return None;
    }
    object.get("assistant_message")?.as_object().cloned()
}

/// Ensures one OpenAI-compatible assistant message object always carries provider-required replay fields by injecting an empty `content` string when neither visible content nor tool calls are present.
/// 确保单条 OpenAI 兼容助手消息对象始终携带 Provider 要求的回放字段；当既没有可见内容也没有工具调用时，会注入空字符串 `content`。
fn ensure_openai_compatible_assistant_message_has_required_payload(
    object: &mut JsonMap<String, Value>,
) {
    let has_content = object.get("content").is_some_and(|content| match content {
        Value::Null => false,
        Value::Array(items) => !items.is_empty(),
        Value::String(_) => true,
        _ => true,
    });
    let has_tool_calls = object
        .get("tool_calls")
        .is_some_and(|tool_calls| match tool_calls {
            Value::Null => false,
            Value::Array(items) => !items.is_empty(),
            _ => true,
        });
    if !has_content && !has_tool_calls {
        object.insert(String::from("content"), Value::String(String::new()));
    }
}

/// Merges one streamed provider-visible assistant field into the accumulated replay state while preserving common append semantics for string, array, and object payloads.
/// 将一条流式 Provider 可见助手字段合并进累计回放状态，同时为字符串、数组与对象载荷保留常见的追加语义。
fn merge_openai_compatible_assistant_field(
    fields: &mut JsonMap<String, Value>,
    field_name: String,
    field_value: Value,
) {
    match fields.get_mut(&field_name) {
        Some(Value::String(existing)) => {
            if let Value::String(incoming) = field_value {
                existing.push_str(&incoming);
            } else {
                fields.insert(field_name, field_value);
            }
        }
        Some(Value::Array(existing)) => {
            if let Value::Array(mut incoming) = field_value {
                existing.append(&mut incoming);
            } else {
                fields.insert(field_name, field_value);
            }
        }
        Some(Value::Object(existing)) => {
            if let Value::Object(incoming) = field_value {
                for (nested_key, nested_value) in incoming {
                    existing.insert(nested_key, nested_value);
                }
            } else {
                fields.insert(field_name, field_value);
            }
        }
        Some(existing) if existing.is_null() => {
            *existing = field_value;
        }
        Some(_) => {
            fields.insert(field_name, field_value);
        }
        None => {
            fields.insert(field_name, field_value);
        }
    }
}

/// Returns whether the active model can safely replay provider-native OpenAI-compatible interleaved assistant fields.
/// 返回当前模型是否可以安全回放 Provider 原生 OpenAI 兼容交错助手字段。
fn openai_compatible_supports_native_interleaved_replay(
    interleaved_reasoning: Option<&ModelInterleavedCapability>,
) -> bool {
    match interleaved_reasoning {
        Some(ModelInterleavedCapability::Enabled(true)) => true,
        Some(ModelInterleavedCapability::Field(field)) => !field.field.trim().is_empty(),
        _ => false,
    }
}

/// Converts one raw OpenAI-compatible usage payload into the shared normalized usage snapshot.
/// 将一份原始 OpenAI 兼容用量载荷转换为共享规范化用量快照。
fn openai_compatible_usage_to_snapshot(
    usage: OpenAiCompatibleUsage,
) -> Option<CoreProviderUsageSnapshot> {
    let snapshot = CoreProviderUsageSnapshot {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        reasoning_tokens: usage
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens),
        total_tokens: usage.total_tokens,
        cached_input_tokens: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens),
        cache_creation_input_tokens: None,
    };
    snapshot.has_any_tokens().then_some(snapshot)
}

/// Processes one batch of OpenAI-compatible tool-call chunks, merging fragments and emitting finalized calls when boundaries are reached.
/// 处理一批 OpenAI 兼容工具调用碎片，在边界到达时合并片段并产出已完成调用。
fn process_compatible_tool_call_chunks(
    pending_tool_calls: &mut HashMap<usize, CompatibleAccumulatedToolCall>,
    finalized_tool_calls: &mut Vec<CoreConversationToolCall>,
    chunks: Vec<CompatibleToolCallChunk>,
) {
    for chunk in chunks {
        if let Some(existing) = pending_tool_calls.get(&chunk.index)
            && should_evict_distinct_named_tool_call(existing, &chunk)
            && let Some(evicted) = pending_tool_calls.remove(&chunk.index)
            && let Some(finalized) = finalize_pending_tool_call(evicted)
        {
            finalized_tool_calls.push(compatible_tool_call_to_core_tool_call(finalized));
        }

        let pending = pending_tool_calls
            .entry(chunk.index)
            .or_insert_with(CompatibleAccumulatedToolCall::empty);

        if let Some(id) = chunk.id.as_ref()
            && !id.is_empty()
        {
            pending.id = id.clone();
        }

        if let Some(name) = chunk.name.as_ref()
            && !name.is_empty()
        {
            pending.name = name.clone();
        }

        if let Some(arguments) = chunk.arguments.as_ref()
            && !arguments.is_empty()
        {
            append_tool_call_arguments(pending, arguments);
        }
    }
}

/// Flushes every pending OpenAI-compatible tool call into finalized canonical tool-call entries.
/// 将所有待定 OpenAI 兼容工具调用冲刷为已完成的规范工具调用项。
fn flush_pending_compatible_tool_calls(
    pending_tool_calls: &mut HashMap<usize, CompatibleAccumulatedToolCall>,
    finalized_tool_calls: &mut Vec<CoreConversationToolCall>,
) {
    let mut indices = pending_tool_calls.keys().copied().collect::<Vec<_>>();
    indices.sort_unstable();

    for index in indices {
        let Some(pending) = pending_tool_calls.remove(&index) else {
            continue;
        };
        let finalized = finalize_pending_tool_call(finalize_completed_streaming_tool_call(pending));
        if let Some(tool_call) = finalized {
            finalized_tool_calls.push(compatible_tool_call_to_core_tool_call(tool_call));
        }
    }
}

/// Converts one accumulated compatible tool call into the canonical `vulcan-core` tool-call shape.
/// 将一条累计完成的兼容工具调用转换为规范 `vulcan-core` 工具调用形状。
fn compatible_tool_call_to_core_tool_call(
    tool_call: CompatibleAccumulatedToolCall,
) -> CoreConversationToolCall {
    let tool_call_id = (!tool_call.id.is_empty()).then_some(tool_call.id.clone());
    CoreConversationToolCall {
        id: tool_call.id,
        tool_call_id,
        tool_name: tool_call.name,
        arguments: tool_call.arguments,
    }
}

/// Forwards one normalized provider delta into both the controller sink and the GUI live sender.
/// 将一条规范 Provider 增量同时转发给控制器汇聚器与 GUI 实时发送器。
fn forward_openai_compatible_delta<S>(
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
    delta: CoreProviderTurnDelta,
) where
    S: VulcanProviderDeltaSink,
{
    sink.push_delta(delta.clone());
    let _ = live_sender.send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
}

#[cfg(test)]
mod tests {
    use super::{
        augment_openai_compatible_error_with_tool_context,
        build_openai_compatible_provider_message_state, build_openai_compatible_request_payload,
        collect_openai_compatible_function_declaration_indices, normalize_openai_compatible_chunk,
        provider_tool_definition_to_openai_compatible_tool,
    };
    use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
    use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
    use crate::provider_config::{ModelInterleavedCapability, ModelInterleavedField};
    use serde_json::{Map as JsonMap, json};
    use std::time::{SystemTime, UNIX_EPOCH};
    use vulcan_core::types::{
        ConversationAttachment, ConversationAttachmentKind,
        ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
        ConversationToolCall as CoreConversationToolCall,
        ProviderContinuation as CoreProviderContinuation,
        ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    };

    /// Builds one temporary attachment used by multimodal transport tests.
    /// 构建一条供多模态传输测试使用的临时附件。
    fn build_test_attachment(
        extension: &str,
        kind: ConversationAttachmentKind,
        mime_type: &str,
        file_name: &str,
    ) -> ConversationAttachment {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let file_path =
            std::env::temp_dir().join(format!("vulcan-openai-compatible-{unique_id}.{extension}"));
        std::fs::write(&file_path, [0_u8, 1, 2, 3]).expect("temporary attachment should write");

        ConversationAttachment {
            kind,
            file_name: String::from(file_name),
            local_path: file_path.to_string_lossy().into_owned(),
            source_path: None,
            asset_id: None,
            byte_len: Some(4),
            preview_local_path: None,
            mime_type: String::from(mime_type),
        }
    }

    /// Verifies the direct OpenAI-compatible request payload replays `reasoning_content` when the selected model explicitly declares a textual interleaved reasoning field.
    /// 验证当所选模型显式声明文本型交错推理字段时，直连 OpenAI 兼容请求载荷会回放 `reasoning_content`。
    #[test]
    fn request_payload_replays_reasoning_content_and_preserves_tool_shapes() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-test"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("tool result body"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({
                    "reasoning": { "effort": "high" }
                })),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("我先读取文件。"),
                    reasoning: String::from("这段思考不应回放。"),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("tool-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("read_file"),
                        arguments: json!({ "path": "src/main.rs" }),
                    }],
                    usage: None,
                },
            }],
            current_prompt: CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("call-1")),
                    text: String::from("tool result body"),
                },
            },
            interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
                field: String::from("reasoning_content"),
            })),
        };

        let payload = build_openai_compatible_request_payload(
            "gpt-test",
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
            true,
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["reasoning_content"], "这段思考不应回放。");
        assert_eq!(messages[1]["tool_calls"][0]["id"], "tool-1");
        assert_eq!(
            messages[1]["tool_calls"][0]["function"]["name"],
            "read_file"
        );
        assert_eq!(
            messages[1]["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"src/main.rs\"}"
        );
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call-1");
        assert_eq!(messages[2]["content"], "tool result body");
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["reasoning"]["effort"], "high");
        assert_eq!(payload["tools"][0]["function"]["name"], "read_file");
    }

    /// Verifies OpenAI-compatible request payloads append host function tools after provider-native tool patches instead of overwriting them.
    /// 验证 OpenAI 兼容请求载荷会在 Provider 原生工具补丁之后追加宿主函数工具，而不是将其覆盖。
    #[test]
    fn request_payload_appends_function_tools_after_existing_native_tools() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("alibaba-coding-plan-cn"),
                model_id: String::from("qwen3.6-plus"),
                system_prompt: String::new(),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::new().clone(),
                ),
                prompt: String::from("请搜索最新资料"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({
                    "enable_search": true,
                    "tools": [
                        { "type": "web_search" }
                    ]
                })),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("请搜索最新资料"),
                    attachments: Vec::new(),
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_compatible_request_payload(
            "qwen3.6-plus",
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
            true,
        )
        .expect("payload should build");

        let tools = payload["tools"]
            .as_array()
            .expect("tools should be an array");
        assert_eq!(tools[0]["type"], "web_search");
        assert_eq!(tools[1]["function"]["name"], "read_file");
        assert_eq!(payload["enable_search"], true);
        assert!(payload.get("search_options").is_none());
        assert_eq!(payload["stream_options"]["include_usage"], true);
    }

    /// Verifies OpenAI-compatible streaming payloads explicitly request the terminal usage chunk while preserving caller-supplied stream-options fields.
    /// 验证 OpenAI 兼容流式载荷会显式请求终态 usage 块，同时保留调用方自带的 stream-options 字段。
    #[test]
    fn request_payload_requests_stream_usage_without_overwriting_other_options() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("deepseek"),
                model_id: String::from("deepseek-v4-pro"),
                system_prompt: String::new(),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::new().clone(),
                ),
                prompt: String::from("hello"),
                history: Vec::new(),
                temperature: None,
                max_turns: 4,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({
                    "stream_options": {
                        "foo": "bar"
                    }
                })),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("hello"),
                    attachments: Vec::new(),
                },
            },
            interleaved_reasoning: None,
        };

        let payload =
            build_openai_compatible_request_payload("deepseek-v4-pro", request, &[], true)
                .expect("payload should build");

        assert_eq!(payload["stream"], true);
        assert_eq!(payload["stream_options"]["include_usage"], true);
        assert_eq!(payload["stream_options"]["foo"], "bar");
    }

    /// Verifies providers whose catalog rules do not request `stream_options.include_usage` keep caller payloads unchanged while still using provider-declared terminal usage sources.
    /// 验证目录规则未请求 `stream_options.include_usage` 的供应商会保持调用方载荷不变，同时仍使用供应商声明的终态 usage 来源。
    #[test]
    fn request_payload_skips_stream_usage_flag_when_provider_rule_disables_it() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openrouter"),
                model_id: String::from("openai/gpt-5"),
                system_prompt: String::new(),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::new().clone(),
                ),
                prompt: String::from("hello"),
                history: Vec::new(),
                temperature: None,
                max_turns: 4,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({
                    "stream_options": {
                        "foo": "bar"
                    }
                })),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("hello"),
                    attachments: Vec::new(),
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_compatible_request_payload("openai/gpt-5", request, &[], true)
            .expect("payload should build");

        assert_eq!(payload["stream"], true);
        assert!(payload["stream_options"].get("include_usage").is_none());
        assert_eq!(payload["stream_options"]["foo"], "bar");
    }

    /// Verifies OpenAI-compatible chunk normalization can extract terminal usage nested inside one completed streamed choice, which matches Kimi-style terminal chunks.
    /// 验证 OpenAI 兼容块规范化可以提取嵌套在已完成流式 choice 内部的终态 usage，这与 Kimi 风格终态块一致。
    #[test]
    fn normalize_chunk_extracts_terminal_choice_usage_from_provider_rule() {
        let chunk = normalize_openai_compatible_chunk(
            "moonshotai",
            "Moonshot AI",
            r#"{
                "id":"resp_1",
                "choices":[
                    {
                        "delta":{"content":"done"},
                        "finish_reason":"stop",
                        "usage":{
                            "prompt_tokens":128,
                            "completion_tokens":32,
                            "total_tokens":160
                        }
                    }
                ]
            }"#,
        )
        .expect("chunk should parse");

        let usage = chunk
            .usage
            .expect("terminal choice usage should be captured");
        assert_eq!(usage.input_tokens, Some(128));
        assert_eq!(usage.output_tokens, Some(32));
        assert_eq!(usage.total_tokens, Some(160));
    }

    /// Verifies OpenRouter-style OpenAI-compatible chunks map the plaintext `reasoning` alias into visible thinking output and raw replay state.
    /// 验证 OpenRouter 风格 OpenAI 兼容块会把纯文本 `reasoning` 别名映射到可见思考输出与原始回放状态。
    #[test]
    fn normalize_chunk_reads_reasoning_alias_and_preserves_replay_field() {
        let chunk = normalize_openai_compatible_chunk(
            "openrouter",
            "OpenRouter",
            r#"{
                "id":"resp_1",
                "choices":[
                    {
                        "delta":{
                            "reasoning":"先检查目录。",
                            "content":"已完成。"
                        },
                        "finish_reason":"stop"
                    }
                ]
            }"#,
        )
        .expect("chunk should parse");

        let choice = chunk.choice.expect("choice should exist");
        assert_eq!(choice.reasoning.as_deref(), Some("先检查目录。"));
        assert_eq!(choice.text.as_deref(), Some("已完成。"));
        assert_eq!(
            choice.provider_visible_assistant_fields["reasoning"],
            json!("先检查目录。")
        );
    }

    /// Verifies OpenRouter-style structured `reasoning_details` payloads flatten into visible thinking text while remaining intact for native replay.
    /// 验证 OpenRouter 风格结构化 `reasoning_details` 载荷会压平成可见思考文本，同时保持原样供原生回放使用。
    #[test]
    fn normalize_chunk_flattens_reasoning_details_and_preserves_structured_payload() {
        let chunk = normalize_openai_compatible_chunk(
            "openrouter",
            "OpenRouter",
            r#"{
                "id":"resp_1",
                "choices":[
                    {
                        "delta":{
                            "reasoning_details":[
                                { "type":"summary", "text":"先检查目录。" },
                                {
                                    "type":"summary",
                                    "summary":[
                                        { "text":"再汇总结果。" }
                                    ]
                                }
                            ]
                        },
                        "finish_reason":"stop"
                    }
                ]
            }"#,
        )
        .expect("chunk should parse");

        let choice = chunk.choice.expect("choice should exist");
        assert_eq!(
            choice.reasoning.as_deref(),
            Some("先检查目录。\n再汇总结果。")
        );
        assert_eq!(
            choice.provider_visible_assistant_fields["reasoning_details"][1]["summary"][0]["text"],
            json!("再汇总结果。")
        );
    }

    /// Verifies provider message replay does not synthesize a duplicate `reasoning_content` field when one native reasoning field is already preserved.
    /// 验证当原生思考字段已经被保留时，Provider 消息回放不会再额外合成重复的 `reasoning_content` 字段。
    #[test]
    fn provider_message_state_avoids_duplicate_textual_reasoning_field() {
        let state = build_openai_compatible_provider_message_state(
            "已完成。",
            &[],
            Some(String::from("先检查目录。\n再汇总结果。")),
            JsonMap::from_iter([(
                String::from("reasoning_details"),
                json!([
                    { "type": "summary", "text": "先检查目录。" },
                    { "type": "summary", "text": "再汇总结果。" }
                ]),
            )]),
        )
        .expect("provider message state should exist");

        let assistant_message = state["assistant_message"]
            .as_object()
            .expect("assistant message should be one object");
        assert!(assistant_message.get("reasoning_content").is_none());
        assert_eq!(assistant_message["content"], json!("已完成。"));
        assert_eq!(
            assistant_message["reasoning_details"][0]["text"],
            json!("先检查目录。")
        );
    }

    /// Verifies OpenAI-compatible transports keep parallel tool results as separate `tool` messages instead of borrowing Anthropic-only coalescing behavior.
    /// 验证 OpenAI 兼容传输会把并行工具结果保持为独立的 `tool` 消息，而不会借用 Anthropic 专用的合并行为。
    #[test]
    fn request_payload_keeps_parallel_tool_results_split_for_openai_compatible() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-test"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("tool result body 4"),
                history: Vec::new(),
                temperature: Some(0.2),
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![
                CoreConversationMessage {
                    id: String::from("assistant-1"),
                    payload: CoreConversationPayload::Assistant {
                        text: String::from("并行等待四个子代理完成。"),
                        reasoning: String::new(),
                        provider_message_id: Some(String::from("resp-1")),
                        provider_message_state: None,
                        tool_calls: vec![
                            CoreConversationToolCall {
                                id: String::from("local-call-1"),
                                tool_call_id: Some(String::from("call-1")),
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-1" }),
                            },
                            CoreConversationToolCall {
                                id: String::from("local-call-2"),
                                tool_call_id: Some(String::from("call-2")),
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-2" }),
                            },
                            CoreConversationToolCall {
                                id: String::from("local-call-3"),
                                tool_call_id: Some(String::from("call-3")),
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-3" }),
                            },
                            CoreConversationToolCall {
                                id: String::from("local-call-4"),
                                tool_call_id: Some(String::from("call-4")),
                                tool_name: String::from("spawn_agent"),
                                arguments: json!({ "task": "delegate-4" }),
                            },
                        ],
                        usage: None,
                    },
                },
                CoreConversationMessage {
                    id: String::from("tool-result-1"),
                    payload: CoreConversationPayload::ToolResult {
                        tool_result_id: String::from("result-1"),
                        tool_call_id: Some(String::from("call-1")),
                        text: String::from("tool result body 1"),
                    },
                },
                CoreConversationMessage {
                    id: String::from("tool-result-2"),
                    payload: CoreConversationPayload::ToolResult {
                        tool_result_id: String::from("result-2"),
                        tool_call_id: Some(String::from("call-2")),
                        text: String::from("tool result body 2"),
                    },
                },
                CoreConversationMessage {
                    id: String::from("tool-result-3"),
                    payload: CoreConversationPayload::ToolResult {
                        tool_result_id: String::from("result-3"),
                        tool_call_id: Some(String::from("call-3")),
                        text: String::from("tool result body 3"),
                    },
                },
            ],
            current_prompt: CoreConversationMessage {
                id: String::from("tool-result-4"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-4"),
                    tool_call_id: Some(String::from("call-4")),
                    text: String::from("tool result body 4"),
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_compatible_request_payload(
            "gpt-test",
            request,
            &[ProviderToolDefinition {
                name: String::from("spawn_agent"),
                description: String::from("Spawn one delegate agent"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "task": { "type": "string" }
                    },
                    "required": ["task"]
                }),
            }],
            true,
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[4]["role"], "tool");
        assert_eq!(messages[5]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call-1");
        assert_eq!(messages[3]["tool_call_id"], "call-2");
        assert_eq!(messages[4]["tool_call_id"], "call-3");
        assert_eq!(messages[5]["tool_call_id"], "call-4");
    }

    /// Verifies OpenAI-compatible request replay preserves an explicitly empty `reasoning_content` field so DeepSeek-style thinking sessions can continue after tool calls.
    /// 验证 OpenAI 兼容请求回放会保留显式为空的 `reasoning_content` 字段，以便 DeepSeek 风格的 thinking 会话在工具调用后继续执行。
    #[test]
    fn request_payload_preserves_empty_reasoning_content_when_interleaved() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("deepseek"),
                model_id: String::from("deepseek-v4-pro"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("tool result body"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("先调用工具。"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("tool-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("read_file"),
                        arguments: json!({ "path": "src/main.rs" }),
                    }],
                    usage: None,
                },
            }],
            current_prompt: CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("call-1")),
                    text: String::from("tool result body"),
                },
            },
            interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
                field: String::from("reasoning_content"),
            })),
        };

        let payload = build_openai_compatible_request_payload(
            "deepseek-v4-pro",
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
            true,
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["reasoning_content"], "");
    }

    /// Verifies OpenAI-compatible request replay injects an empty `content` field for reasoning-only assistant history so strict providers such as DeepSeek do not reject the assistant message as structurally incomplete.
    /// 验证 OpenAI 兼容请求回放会为纯 reasoning 的助手历史注入空 `content` 字段，从而避免 DeepSeek 这类严格 Provider 因助手消息结构不完整而拒绝请求。
    #[test]
    fn request_payload_keeps_reasoning_only_assistant_history_provider_valid() {
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("deepseek"),
                model_id: String::from("deepseek-v4-pro"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("继续"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![CoreConversationMessage {
                id: String::from("assistant-reasoning-only-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::from("继续保持思考上下文。"),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    usage: None,
                },
            }],
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("继续"),
                    attachments: Vec::new(),
                },
            },
            interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
                field: String::from("reasoning_content"),
            })),
        };

        let payload =
            build_openai_compatible_request_payload("deepseek-v4-pro", request, &[], true)
                .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"], "");
        assert_eq!(messages[1]["reasoning_content"], "继续保持思考上下文。");
    }

    /// Verifies replaying one persisted OpenAI-compatible provider message state with reasoning-only assistant fields still injects empty `content` during request rebuild so previously stored sessions remain provider-valid.
    /// 验证回放一条只含 reasoning 助手字段的已持久化 OpenAI 兼容 provider message state 时，请求重建仍会注入空 `content`，从而保证旧会话依然满足 Provider 要求。
    #[test]
    fn request_payload_repairs_reasoning_only_provider_message_state() {
        let assistant_state = json!({
            "protocol": "openai-compatible",
            "assistant_message": {
                "role": "assistant",
                "reasoning_content": "继续保持思考上下文。"
            }
        });
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("deepseek"),
                model_id: String::from("deepseek-v4-pro"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("继续"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: Some(json!({})),
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![CoreConversationMessage {
                id: String::from("assistant-reasoning-only-state-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: Some(assistant_state),
                    tool_calls: Vec::new(),
                    usage: None,
                },
            }],
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("继续"),
                    attachments: Vec::new(),
                },
            },
            interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
                field: String::from("reasoning_content"),
            })),
        };

        let payload =
            build_openai_compatible_request_payload("deepseek-v4-pro", request, &[], true)
                .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"], "");
        assert_eq!(messages[1]["reasoning_content"], "继续保持思考上下文。");
    }

    /// Verifies OpenAI-compatible request replay restores raw provider-visible interleaved fields from provider_message_state when the active model declares a matching field-based capability.
    /// 验证当当前模型声明匹配的字段型交错能力时，OpenAI 兼容请求回放会从 provider_message_state 恢复原生 Provider 可见交错字段。
    #[test]
    fn request_payload_replays_reasoning_details_from_provider_message_state() {
        let assistant_state = json!({
            "protocol": "openai-compatible",
            "assistant_message": {
                "role": "assistant",
                "content": "先总结已有上下文。",
                "reasoning_details": [
                    { "type": "summary", "text": "已读取主要文件结构。" }
                ],
                "tool_calls": [
                    {
                        "id": "tool-1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"src/main.rs\"}"
                        }
                    }
                ]
            }
        });
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai-compatible"),
                model_id: String::from("qwen-test"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("tool result body"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: vec![CoreConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("先总结已有上下文。"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-1")),
                    provider_message_state: Some(assistant_state),
                    tool_calls: vec![CoreConversationToolCall {
                        id: String::from("tool-1"),
                        tool_call_id: Some(String::from("call-1")),
                        tool_name: String::from("read_file"),
                        arguments: json!({ "path": "src/main.rs" }),
                    }],
                    usage: None,
                },
            }],
            current_prompt: CoreConversationMessage {
                id: String::from("tool-result-1"),
                payload: CoreConversationPayload::ToolResult {
                    tool_result_id: String::from("result-1"),
                    tool_call_id: Some(String::from("call-1")),
                    text: String::from("tool result body"),
                },
            },
            interleaved_reasoning: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
                field: String::from("reasoning_details"),
            })),
        };

        let payload = build_openai_compatible_request_payload(
            "qwen-test",
            request,
            &[ProviderToolDefinition {
                name: String::from("read_file"),
                description: String::from("Read one file"),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
            true,
        )
        .expect("payload should build");

        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(
            messages[1]["reasoning_details"][0]["text"],
            "已读取主要文件结构。"
        );
        assert_eq!(messages[1]["tool_calls"][0]["id"], "tool-1");
    }

    /// Verifies OpenAI-compatible request replay restores provider-native interleaved fields for boolean-style capabilities and ignores them when the active model does not declare interleaved support.
    /// 验证 OpenAI 兼容请求回放会为布尔型交错能力恢复 Provider 原生交错字段，并在当前模型未声明交错支持时忽略这些字段。
    #[test]
    fn request_payload_replays_or_ignores_provider_message_state_based_on_interleaved_capability() {
        let assistant_state = json!({
            "protocol": "openai-compatible",
            "assistant_message": {
                "role": "assistant",
                "content": "先做规划。",
                "reasoning_details": [
                    { "type": "summary", "text": "这是隐藏规划。" }
                ]
            }
        });
        let assistant_message = CoreConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("先做规划。"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: Some(assistant_state),
                tool_calls: Vec::new(),
                usage: None,
            },
        };
        let current_prompt = CoreConversationMessage {
            id: String::from("user-1"),
            payload: CoreConversationPayload::UserText {
                text: String::from("继续"),
                attachments: Vec::new(),
            },
        };

        let payload_with_interleaved = build_openai_compatible_request_payload(
            "qwen-test",
            OpenAiCompatibleCoreTurnRequest {
                snapshot: CoreProviderRequestSnapshot {
                    provider_id: String::from("openai-compatible"),
                    model_id: String::from("qwen-test"),
                    system_prompt: String::new(),
                    prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                        String::new().clone(),
                    ),
                    prompt: String::from("继续"),
                    history: Vec::new(),
                    temperature: None,
                    max_turns: 4,
                    tool_schemas: Vec::new(),
                    additional_params: None,
                    request_headers: None,
                    continuation: CoreProviderContinuation::default(),
                },
                current_history: vec![assistant_message.clone()],
                current_prompt: current_prompt.clone(),
                interleaved_reasoning: Some(ModelInterleavedCapability::Enabled(true)),
            },
            &[],
            true,
        )
        .expect("payload should build");
        let payload_without_interleaved = build_openai_compatible_request_payload(
            "qwen-test",
            OpenAiCompatibleCoreTurnRequest {
                snapshot: CoreProviderRequestSnapshot {
                    provider_id: String::from("openai-compatible"),
                    model_id: String::from("qwen-test"),
                    system_prompt: String::new(),
                    prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                        String::new().clone(),
                    ),
                    prompt: String::from("继续"),
                    history: Vec::new(),
                    temperature: None,
                    max_turns: 4,
                    tool_schemas: Vec::new(),
                    additional_params: None,
                    request_headers: None,
                    continuation: CoreProviderContinuation::default(),
                },
                current_history: vec![assistant_message],
                current_prompt,
                interleaved_reasoning: None,
            },
            &[],
            true,
        )
        .expect("payload should build");

        let with_messages = payload_with_interleaved["messages"]
            .as_array()
            .expect("messages should be an array");
        assert_eq!(
            with_messages[0]["reasoning_details"][0]["text"],
            "这是隐藏规划。"
        );

        let without_messages = payload_without_interleaved["messages"]
            .as_array()
            .expect("messages should be an array");
        assert!(without_messages[0].get("reasoning_details").is_none());
        assert_eq!(without_messages[0]["content"], "先做规划。");
    }

    /// Verifies OpenAI-compatible function-tool conversion fills in explicit array and nested object types so Gemini-backed proxy routes can validate host schemas.
    /// 验证 OpenAI 兼容函数工具转换会补齐显式数组与嵌套对象类型，以便 Gemini 背后的代理路由能够校验宿主 schema。
    #[test]
    fn openai_compatible_tool_conversion_normalizes_nested_container_types() {
        let tool = provider_tool_definition_to_openai_compatible_tool(&ProviderToolDefinition {
            name: String::from("read_nodes"),
            description: String::from("Read structured nodes"),
            parameters: json!({
                "type": "object",
                "properties": {
                    "nodes": {
                        "items": {
                            "properties": {
                                "file": { "type": "string" },
                                "selector": { "type": "string" }
                            },
                            "required": ["file", "selector"]
                        }
                    }
                }
            }),
        });

        assert_eq!(
            tool["function"]["parameters"]["properties"]["nodes"]["type"],
            json!("array")
        );
        assert_eq!(
            tool["function"]["parameters"]["properties"]["nodes"]["items"]["type"],
            json!("object")
        );
    }

    /// Verifies OpenAI-compatible error augmentation maps Gemini-style function declaration indices back to the original host tool names.
    /// 验证 OpenAI 兼容错误增强逻辑会把 Gemini 风格的函数声明索引映射回原始宿主工具名称。
    #[test]
    fn openai_compatible_error_augmentation_reports_tool_names_for_function_indices() {
        let tool_definitions = vec![
            ProviderToolDefinition {
                name: String::from("ask"),
                description: String::from("Ask the user"),
                parameters: json!({"type":"object"}),
            },
            ProviderToolDefinition {
                name: String::from("vulcan_codekit_node_source"),
                description: String::from("Read node source"),
                parameters: json!({"type":"object"}),
            },
            ProviderToolDefinition {
                name: String::from("vulcan_codekit_patch"),
                description: String::from("Patch one node"),
                parameters: json!({"type":"object"}),
            },
        ];
        let body = "* GenerateContentRequest.tools[0].function_declarations[1].parameters.properties[nodes].items: missing field.\n\
* GenerateContentRequest.tools[0].function_declarations[2].parameters.properties[patches].items: missing field.";

        let message = augment_openai_compatible_error_with_tool_context(
            String::from("HTTP 400 Bad Request | Provider returned error"),
            body,
            &tool_definitions,
        );

        assert_eq!(
            collect_openai_compatible_function_declaration_indices(body),
            vec![1, 2]
        );
        assert!(message.contains("tool[1]=vulcan_codekit_node_source"));
        assert!(message.contains("tool[2]=vulcan_codekit_patch"));
    }

    /// Verifies OpenAI-compatible user messages switch to content-part arrays and carry staged image attachments as data URLs.
    /// 验证 OpenAI 兼容用户消息会切换到内容 Part 数组，并以 Data URL 携带已暂存图片附件。
    #[test]
    fn request_payload_embeds_user_image_attachments_as_data_urls() {
        let attachment = build_test_attachment(
            "png",
            ConversationAttachmentKind::Image,
            "image/png",
            "diagram.png",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-4.1"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("请分析这张图"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("请分析这张图"),
                    attachments: vec![attachment.clone()],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_compatible_request_payload("gpt-4.1", request, &[], true)
            .expect("payload should build");
        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        let content = messages[1]["content"]
            .as_array()
            .expect("user content should be one part array");

        assert_eq!(messages[1]["role"], "user");
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(
            content[1]["image_url"]["url"],
            json!("data:image/png;base64,AAECAw==")
        );

        let _ = std::fs::remove_file(&attachment.local_path);
    }

    /// Verifies strict OpenAI-compatible Chat payloads downgrade staged MP3 attachments to explicit text notices.
    /// 验证严格 OpenAI 兼容 Chat 载荷会把已暂存 MP3 附件降级为显式文本提示。
    #[test]
    fn request_payload_downgrades_user_audio_attachments_to_text_notice() {
        let attachment = build_test_attachment(
            "mp3",
            ConversationAttachmentKind::Audio,
            "audio/mpeg",
            "sample.mp3",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-4.1"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("请转写这段音频"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("请转写这段音频"),
                    attachments: vec![attachment.clone()],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_compatible_request_payload("gpt-4.1", request, &[], true)
            .expect("payload should build");
        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        let content = messages[1]["content"]
            .as_str()
            .expect("user content should downgrade to text");

        assert!(content.contains("请转写这段音频"));
        assert!(content.contains("Cannot read \"sample.mp3\""));
        assert!(content.contains("this API path"));

        let _ = std::fs::remove_file(&attachment.local_path);
    }

    /// Verifies strict OpenAI-compatible Chat payloads downgrade staged PDF and video attachments to explicit text notices.
    /// 验证严格 OpenAI 兼容 Chat 载荷会把已暂存 PDF 与视频附件降级为显式文本提示。
    #[test]
    fn request_payload_downgrades_user_file_like_attachments_to_text_notice() {
        let pdf_attachment = build_test_attachment(
            "pdf",
            ConversationAttachmentKind::Pdf,
            "application/pdf",
            "guide.pdf",
        );
        let video_attachment = build_test_attachment(
            "mp4",
            ConversationAttachmentKind::Video,
            "video/mp4",
            "clip.mp4",
        );
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-4.1"),
                system_prompt: String::from("system prompt"),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::from("system prompt").clone(),
                ),
                prompt: String::from("请分析这些附件"),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("请分析这些附件"),
                    attachments: vec![pdf_attachment.clone(), video_attachment.clone()],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_compatible_request_payload("gpt-4.1", request, &[], true)
            .expect("payload should build");
        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        let content = messages[1]["content"]
            .as_str()
            .expect("user content should downgrade to text");

        assert!(content.contains("请分析这些附件"));
        assert!(content.contains("Cannot read \"guide.pdf\""));
        assert!(content.contains("Cannot read \"clip.mp4\""));
        assert!(content.contains("this API path"));

        let _ = std::fs::remove_file(&pdf_attachment.local_path);
        let _ = std::fs::remove_file(&video_attachment.local_path);
    }

    /// Verifies a pure attachment-only OpenAI-compatible user message remains provider-visible when its managed replay file is gone.
    /// 验证纯附件 OpenAI 兼容用户消息在托管回放文件缺失时仍会以文本提示形式传给 Provider。
    #[test]
    fn request_payload_replaces_missing_attachment_only_message_with_text_notice() {
        let attachment = build_test_attachment(
            "png",
            ConversationAttachmentKind::Image,
            "image/png",
            "lost-diagram.png",
        );
        std::fs::remove_file(&attachment.local_path).expect("managed attachment should remove");
        let request = OpenAiCompatibleCoreTurnRequest {
            snapshot: CoreProviderRequestSnapshot {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-4.1"),
                system_prompt: String::new(),
                prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                    String::new().clone(),
                ),
                prompt: String::new(),
                history: Vec::new(),
                temperature: None,
                max_turns: 8,
                tool_schemas: Vec::new(),
                additional_params: None,
                request_headers: None,
                continuation: CoreProviderContinuation::default(),
            },
            current_history: Vec::new(),
            current_prompt: CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::new(),
                    attachments: vec![attachment],
                },
            },
            interleaved_reasoning: None,
        };

        let payload = build_openai_compatible_request_payload("gpt-4.1", request, &[], true)
            .expect("payload should build");
        let messages = payload["messages"]
            .as_array()
            .expect("messages should be an array");
        let content_text = messages[0]["content"]
            .as_array()
            .and_then(|content| content.first())
            .and_then(|part| part.get("text"))
            .and_then(|value| value.as_str())
            .or_else(|| messages[0]["content"].as_str())
            .expect("user content should expose one text notice");

        assert_eq!(messages[0]["role"], "user");
        assert_eq!(
            content_text,
            "ERROR: Attachment \"lost-diagram.png\" no longer exists. Please ignore it."
        );
    }
}

/// Returns the supported OpenAI-compatible textual interleaved reasoning field when the selected model can safely replay flattened reasoning text.
/// 返回所选模型可安全回放扁平化思考文本时支持的 OpenAI 兼容文本型交错推理字段。
fn openai_compatible_textual_interleaved_reasoning_field(
    interleaved_reasoning: Option<&ModelInterleavedCapability>,
) -> Option<&str> {
    match interleaved_reasoning {
        Some(ModelInterleavedCapability::Field(field))
            if field.field.trim().eq_ignore_ascii_case("reasoning_content") =>
        {
            Some("reasoning_content")
        }
        _ => None,
    }
}
