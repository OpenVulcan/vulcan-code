//! Direct OpenAI-compatible HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 OpenAI 兼容 HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_attachment::{
    build_attachment_transport_error_text, build_missing_attachment_replay_text,
    read_provider_attachment,
};
use crate::agent_runtime::provider_cache_policy::{
    ProviderCacheHintSupport, default_provider_cache_candidates, lower_provider_cache_policy,
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
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_http_support::{
    build_json_http_client, build_rest_endpoint, ensure_bearer_authorization_header,
    ensure_json_content_type, ensure_string_header, format_standard_error_response_with_headers,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::provider_transport_wire_capture::{
    emit_provider_wire_request, provider_request_dry_run_active, provider_request_dry_run_response,
};
use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
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
use uuid::Uuid;
use vulcan_context::RuntimeProviderNativeStateEnvelope;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    ProviderTurnDelta as CoreProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};
use vulcan_tools_manager::{
    VulcanToolSchemaProjectionContext, VulcanToolSchemaProjectionProfile,
    project_tool_parameters_schema, resolve_tool_schema_projection_profile,
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

mod chunk_normalization;
mod error_context;
mod payload;
mod provider_state;
mod streaming;
mod tool_calls;

#[cfg(test)]
mod tests;

use chunk_normalization::*;
use error_context::*;
use payload::*;
use provider_state::*;
pub(crate) use streaming::{
    execute_openai_compatible_http_streaming_turn,
    execute_openai_compatible_http_streaming_turn_with_options,
};
use tool_calls::*;
