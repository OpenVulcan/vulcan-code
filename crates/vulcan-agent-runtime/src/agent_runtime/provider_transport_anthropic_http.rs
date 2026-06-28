//! Direct Anthropic HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 Anthropic HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_attachment::{
    build_attachment_transport_error_text, build_missing_attachment_replay_text,
    read_provider_attachment,
};
use crate::agent_runtime::provider_cache_policy::{
    ANTHROPIC_MAX_CACHE_BREAKPOINTS, ProviderCacheHintSupport, ProviderCacheHintTarget,
    default_provider_cache_candidates, lower_provider_cache_policy,
};
use crate::agent_runtime::provider_prompt_layers::provider_fallback_system_prompt;
use crate::agent_runtime::provider_protocol_anthropic_client::normalize_anthropic_base_url;
use crate::agent_runtime::provider_protocol_anthropic_decoders::sse::iter_sse_messages;
use crate::agent_runtime::provider_protocol_bridge::{
    build_openai_compatible_turn_response, core_provider_turn_delta_to_runtime_delta,
};
use crate::agent_runtime::provider_protocol_content::{
    ProviderAssistantContent, ProviderReasoningPart,
};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_http_support::{
    build_json_http_client, ensure_json_content_type, ensure_static_header, ensure_string_header,
    format_standard_error_response_with_headers,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::provider_transport_wire_capture::{
    emit_provider_wire_request, provider_request_dry_run_active, provider_request_dry_run_response,
};
use crate::provider_config::ProviderApiFamily;
use crate::provider_input_normalizer::normalize_provider_user_input;
use crate::tooling::build_direct_public_tool_name;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value, json};
use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use vulcan_context::{RuntimeProviderLoweringRepairEvent, RuntimeProviderNativeStateEnvelope};
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall, CoreError as VulcanCoreError,
    ProviderTurnDelta as CoreProviderTurnDelta, ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
};

/// Latest Anthropic API version used by the host-owned direct transport.
/// 宿主自有直连传输使用的最新 Anthropic API 版本。
const ANTHROPIC_API_VERSION_LATEST: &str = "2023-06-01";

/// Diagnostic marker used when unsafe Anthropic tool results are downgraded to text.
/// 当不安全的 Anthropic 工具结果被降级为文本时使用的诊断标记。
const ANTHROPIC_DOWNGRADED_TOOL_RESULT_MARKER: &str =
    "Tool result could not be safely replayed as Anthropic tool_result";

/// Provider-message-state key used to persist replayable Anthropic-native assistant content.
/// 用于持久化可回放 Anthropic 原生助手内容的 provider_message_state 键名。
const ANTHROPIC_PROVIDER_STATE_KEY: &str = "anthropic_native";

/// Minimal Anthropic streaming event used by the host-owned direct transport.
/// 宿主自有直连传输使用的最小 Anthropic 流式事件。
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamingEvent {
    /// Message-start event carrying response identity and initial usage.
    /// 携带响应标识与初始用量的消息开始事件。
    MessageStart { message: AnthropicMessageStart },
    /// Content-block-start event carrying one initial content block.
    /// 携带初始内容块的内容块开始事件。
    ContentBlockStart {
        #[serde(rename = "index")]
        _index: usize,
        content_block: AnthropicContentBlock,
    },
    /// Content-block-delta event carrying one streamed content fragment.
    /// 携带单条流式内容碎片的内容块增量事件。
    ContentBlockDelta {
        #[serde(rename = "index")]
        _index: usize,
        delta: AnthropicContentDelta,
    },
    /// Content-block-stop event closing the current content block.
    /// 用于关闭当前内容块的内容块结束事件。
    ContentBlockStop {
        #[serde(rename = "index")]
        _index: usize,
    },
    /// Message-delta event carrying incremental usage and stop reason.
    /// 携带增量用量与停止原因的消息增量事件。
    MessageDelta {
        delta: AnthropicMessageDelta,
        #[serde(rename = "usage")]
        _usage: AnthropicPartialUsage,
    },
    /// Message-stop event marking the natural end of the stream.
    /// 标记流自然结束的消息停止事件。
    MessageStop,
    /// Ping event emitted by the provider to keep the stream alive.
    /// Provider 用于保活流的 Ping 事件。
    Ping,
    /// Unknown or unsupported event variant.
    /// 未知或当前不支持的事件变体。
    #[serde(other)]
    Unknown,
}

/// Minimal Anthropic message-start payload.
/// 最小 Anthropic 消息开始载荷。
#[derive(Debug, Deserialize)]
struct AnthropicMessageStart {
    /// Upstream response identifier.
    /// 上游响应标识。
    id: String,
    /// Upstream model identifier.
    /// 上游模型标识。
    model: String,
    /// Initial usage snapshot.
    /// 初始用量快照。
    #[serde(rename = "usage")]
    _usage: AnthropicUsage,
}

/// Minimal Anthropic content block used by the host-owned direct transport.
/// 宿主自有直连传输使用的最小 Anthropic 内容块。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    /// Plain text content block.
    /// 纯文本内容块。
    Text { text: String },
    /// Tool-use content block.
    /// 工具使用内容块。
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// Thinking content block.
    /// 思考内容块。
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    /// Redacted thinking content block.
    /// 脱敏思考内容块。
    RedactedThinking {
        #[serde(rename = "data")]
        _data: String,
    },
    /// Provider-native server-side tool use such as Anthropic web search.
    /// Provider 原生服务端工具使用，例如 Anthropic 网页搜索。
    ServerToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    /// Provider-native web-search result block returned by Anthropic.
    /// Anthropic 返回的 Provider 原生网页搜索结果块。
    WebSearchToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: Value,
    },
}

/// Minimal Anthropic content delta used by the host-owned direct transport.
/// 宿主自有直连传输使用的最小 Anthropic 内容增量。
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentDelta {
    /// Streamed text delta.
    /// 流式文本增量。
    TextDelta { text: String },
    /// Streamed partial JSON for tool input.
    /// 工具输入的流式部分 JSON。
    InputJsonDelta { partial_json: String },
    /// Streamed visible thinking delta.
    /// 流式可见思考增量。
    ThinkingDelta { thinking: String },
    /// Streamed thinking signature delta.
    /// 流式思考签名增量。
    SignatureDelta { signature: String },
}

/// Minimal Anthropic message-delta payload.
/// 最小 Anthropic 消息增量载荷。
#[derive(Debug, Deserialize)]
struct AnthropicMessageDelta {
    /// Optional provider stop reason.
    /// 可选的 Provider 停止原因。
    stop_reason: Option<String>,
}

/// Minimal Anthropic full usage payload.
/// 最小 Anthropic 完整用量载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct AnthropicUsage {
    /// Input token count.
    /// 输入 token 数量。
    #[serde(rename = "input_tokens")]
    _input_tokens: u64,
    /// Output token count.
    /// 输出 token 数量。
    #[serde(rename = "output_tokens")]
    _output_tokens: u64,
    /// Optional cache-creation input token count.
    /// 可选的缓存写入输入 token 数量。
    #[serde(rename = "cache_creation_input_tokens", default)]
    _cache_creation_input_tokens: Option<u64>,
    /// Optional cache-read input token count.
    /// 可选的缓存命中输入 token 数量。
    #[serde(rename = "cache_read_input_tokens", default)]
    _cache_read_input_tokens: Option<u64>,
}

/// Minimal Anthropic partial usage payload.
/// 最小 Anthropic 增量用量载荷。
#[derive(Debug, Clone, Default, Deserialize)]
struct AnthropicPartialUsage {
    /// Output token count accumulated so far.
    /// 当前累计的输出 token 数量。
    #[serde(rename = "output_tokens")]
    _output_tokens: usize,
}

/// Converts one full Anthropic usage payload into the shared normalized usage snapshot.
/// 将一份完整 Anthropic 用量载荷转换为共享规范化用量快照。
fn anthropic_usage_to_snapshot(usage: &AnthropicUsage) -> Option<CoreProviderUsageSnapshot> {
    let snapshot = CoreProviderUsageSnapshot {
        input_tokens: Some(usage._input_tokens as usize),
        output_tokens: Some(usage._output_tokens as usize),
        reasoning_tokens: None,
        total_tokens: Some((usage._input_tokens.saturating_add(usage._output_tokens)) as usize),
        cached_input_tokens: usage._cache_read_input_tokens.map(|value| value as usize),
        cache_creation_input_tokens: usage
            ._cache_creation_input_tokens
            .map(|value| value as usize),
    };
    snapshot.has_any_tokens().then_some(snapshot)
}

/// Merges one Anthropic partial usage payload into the current normalized usage snapshot.
/// 将一份 Anthropic 增量用量载荷合并进当前规范化用量快照。
fn merge_anthropic_partial_usage(
    current: Option<CoreProviderUsageSnapshot>,
    partial: &AnthropicPartialUsage,
) -> Option<CoreProviderUsageSnapshot> {
    let mut snapshot = current.unwrap_or_default();
    if partial._output_tokens > 0 {
        snapshot.output_tokens = Some(partial._output_tokens);
    }
    if let (Some(input_tokens), Some(output_tokens)) =
        (snapshot.input_tokens, snapshot.output_tokens)
    {
        snapshot.total_tokens = Some(input_tokens.saturating_add(output_tokens));
    }
    snapshot.has_any_tokens().then_some(snapshot)
}

/// Pending tool-use block assembled across multiple Anthropic stream events.
/// 跨多条 Anthropic 流事件累计组装的待定工具使用块。
#[derive(Debug, Default)]
struct PendingAnthropicToolUse {
    /// Stable tool-use identifier emitted by the provider.
    /// Provider 发出的稳定工具使用标识。
    id: String,
    /// Stable tool name emitted by the provider.
    /// Provider 发出的稳定工具名称。
    name: String,
    /// Accumulated partial JSON payload for the tool input.
    /// 为工具输入累计的部分 JSON 载荷。
    input_json: String,
}

/// Pending provider-native server-side tool-use block assembled across stream events.
/// 跨流事件累计组装的待定 Provider 原生服务端工具使用块。
#[derive(Debug, Default)]
struct PendingAnthropicServerToolUse {
    /// Stable server-tool-use identifier emitted by the provider.
    /// Provider 发出的稳定服务端工具使用标识。
    id: String,
    /// Stable server-tool name emitted by the provider.
    /// Provider 发出的稳定服务端工具名称。
    name: String,
    /// Accumulated partial JSON payload for server-tool input.
    /// 为服务端工具输入累计的部分 JSON 载荷。
    input_json: String,
}

/// Pending thinking block assembled across multiple Anthropic stream events.
/// 跨多条 Anthropic 流事件累计组装的待定思考块。
#[derive(Debug, Default)]
struct PendingAnthropicThinking {
    /// Accumulated visible thinking text.
    /// 累计的可见思考文本。
    thinking: String,
    /// Accumulated thinking signature text.
    /// 累计的思考签名文本。
    signature: String,
}

mod payload;
mod provider_state;
mod streaming;
mod tools;
mod topology;

#[cfg(test)]
mod tests;

use payload::*;
use provider_state::*;
pub(crate) use streaming::execute_anthropic_http_streaming_turn;
use tools::*;
use topology::*;
