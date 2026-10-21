//! Durable provider-neutral assistant replay state.
//! 可持久化的供应商无关助手重放状态。

use crate::{AiMessagePhase, ContentItem};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Closed reasoning-replay contract implemented end to end for one protocol route.
/// 为一条协议路由端到端实现的封闭推理回传契约。
///
/// # Remarks
/// 说明
/// Every value names one exact wire shape whose request projection, replay acceptance and
/// regression tests exist together, so a manifest can never declare a shape this engine only
/// pretends to implement.
/// 每个取值都对应一种精确线路形态，其请求投影、重放接受条件与回归测试同时存在，
/// 因此 Manifest 绝不会声明本引擎只是假装实现的形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningReplayContract {
    /// DeepSeek Responses replays ordered provider items as plaintext `reasoning.content`.
    /// DeepSeek Responses 以明文 `reasoning.content` 回放有序供应商项。
    DeepseekResponsesPlaintextV1,
    /// DeepSeek Chat merges one turn's reasoning, body and tool calls into one assistant message.
    /// DeepSeek Chat 把一个回合的推理、正文与工具调用合并进同一条助手消息。
    DeepseekChatReasoningContentV1,
    /// DeepSeek Anthropic projects original `anthropic#` thinking blocks without a bridge signature.
    /// DeepSeek Anthropic 投影原始 `anthropic#` thinking 块，不使用桥接签名。
    DeepseekAnthropicThinkingV1,
}

impl ReasoningReplayContract {
    /// Returns the stable engine primitive identifier declared by `engine_contract`.
    /// 返回 `engine_contract` 声明的稳定引擎原语标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact serialized name shared by the manifest field and the engine contract.
    /// 返回 Manifest 字段与引擎契约共用的精确序列化名称。
    pub fn primitive_id(self) -> &'static str {
        match self {
            Self::DeepseekResponsesPlaintextV1 => "deepseek_responses_plaintext_v1",
            Self::DeepseekChatReasoningContentV1 => "deepseek_chat_reasoning_content_v1",
            Self::DeepseekAnthropicThinkingV1 => "deepseek_anthropic_thinking_v1",
        }
    }
}

/// Attributes one original request message to the internal input items it produced.
/// 把一条原始请求消息归属到它所产生的内部输入项。
#[derive(Debug, Clone, PartialEq)]
pub struct ReplaySidecarMessage {
    /// Absolute index of this message inside `AiExecutionRequest::messages`.
    /// 本条消息在 `AiExecutionRequest::messages` 中的绝对索引。
    pub message_index: usize,
    /// Exact semantic author of this message.
    /// 本条消息的精确语义作者。
    pub role: crate::MessageRole,
    /// Ordered canonical content of this exact message, captured before any target encoder drops it.
    /// 本条消息在任何目标编码器丢弃之前的精确有序规范内容。
    pub content: Vec<ContentItem>,
    /// Half-open range of internal input items this message produced, possibly empty.
    /// 本条消息产生的内部输入项的半开区间，可以为空。
    pub input_range: std::ops::Range<usize>,
    /// Number of leading input items supplied by this message's durable provider context.
    /// 此消息的可持久化供应商上下文提供的前导输入项数量。
    pub provider_context_item_count: usize,
    /// Provider-visible tool call identities declared by this exact message, in message order.
    /// 本条消息按消息顺序声明的供应商可见工具调用身份。
    pub tool_call_ids: Vec<String>,
}

/// Non-serializable request replay sidecar built before any target encoder loses information.
/// 在任何目标编码器丢失信息之前构建的不可序列化请求重放侧车。
///
/// # Remarks
/// 说明
/// The sidecar deliberately implements neither `Serialize` nor `Deserialize`, and no transport,
/// log or durable session may carry it: it exists only between the internal encoder and the exact
/// target converter selected for one request.
/// 本侧车有意不实现 `Serialize` 或 `Deserialize`，且任何传输、日志或持久化会话都不得携带它：
/// 它只存在于内部编码器与本次请求选定的精确目标转换器之间。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplaySidecar {
    /// Every message this envelope actually encoded, in original message order.
    /// 本次封装实际编码的每条消息，按原始消息顺序排列。
    messages: Vec<ReplaySidecarMessage>,
}

impl ReplaySidecar {
    /// Creates one sidecar from messages already ordered by original message index.
    /// 从已按原始消息索引排序的消息创建一份侧车。
    pub fn new(messages: Vec<ReplaySidecarMessage>) -> Self {
        Self { messages }
    }

    /// Returns every message this sidecar retained.
    /// 返回本侧车保留的每条消息。
    pub fn messages(&self) -> &[ReplaySidecarMessage] {
        &self.messages
    }

    /// Iterates only the assistant messages, which own reasoning, body and tool calls.
    /// 仅迭代持有推理、正文与工具调用的助手消息。
    pub fn assistant_messages(&self) -> impl Iterator<Item = &ReplaySidecarMessage> {
        self.messages
            .iter()
            .filter(|message| message.role == crate::MessageRole::Assistant)
    }

    /// Reports whether this sidecar retained no message.
    /// 报告本侧车是否未保留任何消息。
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

/// Wraps one opaque provider continuation with an explicit closed provenance prefix.
/// 使用显式封闭来源前缀封装一项不透明供应商延续载荷。
///
/// # Parameters
/// 参数
/// `provider` is the closed protocol family and `payload` is its opaque continuation value.
/// `provider` 是封闭协议族，`payload` 是其不透明延续值。
///
/// # Returns
/// 返回值
/// A replay-safe value, or `None` when either component violates the delimiter contract.
/// 返回可安全重放的值；任一部分违反分隔符契约时返回 `None`。
pub fn wrap_provider_continuation(provider: &str, payload: &str) -> Option<String> {
    let provider = provider.trim();
    let payload = payload.trim();
    if provider.is_empty() || payload.is_empty() || provider.contains('#') || payload.contains('#')
    {
        return None;
    }
    Some(format!("{provider}#{payload}"))
}

/// Unwraps one opaque continuation only for its exact owning provider family.
/// 仅针对其精确归属供应商协议族解封一项不透明延续载荷。
///
/// # Parameters
/// 参数
/// `provider` is the expected protocol family and `signature` is the normalized stored value.
/// `provider` 是预期协议族，`signature` 是归一化存储值。
///
/// # Returns
/// 返回值
/// The original opaque payload when provenance matches exactly.
/// 来源精确匹配时返回原始不透明载荷。
pub fn provider_continuation_payload<'a>(provider: &str, signature: &'a str) -> Option<&'a str> {
    let (source, payload) = signature.split_once('#')?;
    let payload = payload.trim();
    (source == provider && !payload.is_empty() && !payload.contains('#')).then_some(payload)
}

/// Current durable replay envelope format version.
/// 当前可持久化重放封套格式版本。
pub const PROVIDER_REPLAY_ENVELOPE_VERSION: u16 = 4;

/// Preserves one provider message phase without exposing private provider context.
/// 在不暴露供应商私有上下文的情况下保存一项供应商消息阶段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMessagePhaseSnapshot {
    /// Exact public response item identifier owning the phase.
    /// 持有该阶段的精确公共响应项标识。
    pub item_id: String,
    /// Closed assistant message phase.
    /// 封闭的助手消息阶段。
    pub phase: AiMessagePhase,
}

/// Canonicalizes official Codex Responses input aliases before durable replay validation.
/// 在可持久化重放校验前规范化官方 Codex Responses 输入别名。
///
/// # Parameters
/// 参数
/// `items` is the ordered output returned by the standalone Responses compact endpoint.
/// `items` 是独立 Responses 压缩端点返回的有序输出。
pub fn canonicalize_codex_responses_replay_items(items: &mut [Value]) {
    for item in items {
        if item.get("type").and_then(Value::as_str) != Some("compaction_summary") {
            continue;
        }
        // object is the exact provider item whose official legacy alias must serialize canonically on replay.
        // object 是必须在重放时按规范值序列化的官方旧别名供应商项。
        if let Some(object) = item.as_object_mut() {
            object.insert(
                String::from("type"),
                Value::String(String::from("compaction")),
            );
        }
    }
}

/// Returns whether one official Codex Responses item can be preserved and replayed as input.
/// 返回一个官方 Codex Responses 项是否可以保存并作为输入重放。
///
/// # Parameters
/// 参数
/// `value` is the provider-produced JSON item from a compact response.
/// `value` 是 compact 响应中的供应商 JSON 项。
///
/// # Returns
/// 返回值
/// `true` only for the current official Codex `ResponseItem` variants that are valid history
/// items; request-only controls and unknown variants are rejected before persistence.
/// 仅当它属于官方 Codex `ResponseItem` 中可作为历史项的变体时返回 `true`；请求控制项与未知
/// 变体会在持久化前被拒绝。
pub fn is_replayable_codex_responses_item(value: &Value) -> bool {
    match value.get("type").and_then(Value::as_str) {
        // A compaction item without its required opaque payload cannot continue an official Codex session.
        // 缺少必需不透明载荷的压缩项无法续写官方 Codex 会话。
        Some("compaction") => value
            .get("encrypted_content")
            .and_then(Value::as_str)
            .is_some_and(|encrypted_content| !encrypted_content.is_empty()),
        // Reasoning is replayable only when it contains full content or an opaque continuation;
        // a summary-only item is transient presentation state.
        // 推理项仅在包含完整正文或不透明延续时可重放；只有摘要的项目属于瞬态展示状态。
        Some("reasoning") => {
            value
                .get("content")
                .and_then(Value::as_array)
                .is_some_and(|content| !content.is_empty())
                || value
                    .get("encrypted_content")
                    .and_then(Value::as_str)
                    .is_some_and(|encrypted_content| !encrypted_content.is_empty())
        }
        Some(
            "additional_tools"
            | "message"
            | "agent_message"
            | "local_shell_call"
            | "function_call"
            | "tool_search_call"
            | "function_call_output"
            | "custom_tool_call"
            | "custom_tool_call_output"
            | "tool_search_output"
            | "web_search_call"
            | "image_generation_call"
            | "context_compaction",
        ) => true,
        _ => false,
    }
}

/// Preserves ordered assistant content together with its exact protocol scope.
/// 在精确协议作用域内保存有序助手内容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderReplayEnvelope {
    /// Closed envelope format version.
    /// 封闭封套格式版本。
    pub version: u16,
    /// Provider definition identifier that produced the content.
    /// 产生内容的供应商定义标识。
    pub provider_id: String,
    /// Credential connection identifier that produced the content.
    /// 产生内容的凭据连接标识。
    pub connection_id: String,
    /// Non-secret resolved request URL that produced the content.
    /// 产生内容的不含秘密的已解析请求 URL。
    pub route_fingerprint: String,
    /// Protocol profile identifier that produced provider-specific signatures.
    /// 产生供应商专属签名的协议 Profile 标识。
    pub protocol_profile_id: String,
    /// Upstream model identifier that produced the content.
    /// 产生内容的上游模型标识。
    pub model_id: String,
    /// Ordered provider-neutral assistant content.
    /// 有序的供应商无关助手内容。
    pub content: Vec<ContentItem>,
    /// Ordered exact Responses output items for the bound physical route.
    /// 绑定物理路由使用的有序 Responses 原始输出项。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_context_items: Vec<Value>,
    /// Typed message phases retained separately from opaque provider items.
    /// 与不透明供应商项目分开保存的类型化消息阶段。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub message_phases: Vec<ProviderMessagePhaseSnapshot>,
}

impl ProviderReplayEnvelope {
    /// Creates one versioned replay envelope for an exact execution route.
    /// 为一条精确执行路由创建一个带版本的重放封套。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `connection_id`, `route_fingerprint`, `protocol_profile_id`, and `model_id`
    /// define the immutable route scope; `content` contains provider-neutral assistant output and
    /// `provider_context_items` contains exact Responses output items in original output order.
    /// `provider_id`、`connection_id`、`route_fingerprint`、`protocol_profile_id` 与 `model_id`
    /// 定义不可变路由作用域；`content` 按原始流顺序保存供应商无关助手输出，
    /// `provider_context_items` 按原始输出顺序保存 Responses 原始输出项。
    ///
    /// # Returns
    /// 返回值
    /// A replay envelope using the current closed format version.
    /// 返回使用当前封闭格式版本的重放封套。
    pub fn new(
        provider_id: impl Into<String>,
        connection_id: impl Into<String>,
        route_fingerprint: impl Into<String>,
        protocol_profile_id: impl Into<String>,
        model_id: impl Into<String>,
        content: Vec<ContentItem>,
        provider_context_items: Vec<Value>,
    ) -> Self {
        Self {
            version: PROVIDER_REPLAY_ENVELOPE_VERSION,
            provider_id: provider_id.into(),
            connection_id: connection_id.into(),
            route_fingerprint: route_fingerprint.into(),
            protocol_profile_id: protocol_profile_id.into(),
            model_id: model_id.into(),
            content,
            provider_context_items,
            message_phases: Vec::new(),
        }
    }

    /// Attaches validated message phases observed for this exact provider response.
    /// 附加从当前精确供应商响应观察到的已校验消息阶段。
    ///
    /// # Parameters
    /// 参数
    /// `message_phases` preserves response item identity and closed phase values in stream order.
    /// `message_phases` 按流顺序保存响应项身份与封闭阶段值。
    ///
    /// # Returns
    /// 返回值
    /// The replay envelope with typed message-phase metadata attached.
    /// 返回附带类型化消息阶段元数据的重放封套。
    pub fn with_message_phases(
        mut self,
        message_phases: Vec<ProviderMessagePhaseSnapshot>,
    ) -> Self {
        self.message_phases = message_phases;
        self
    }

    /// Verifies that replay state belongs to the exact active execution route.
    /// 验证重放状态属于当前精确执行路由。
    ///
    /// # Parameters
    /// 参数
    /// The five identifiers are the active provider, credential connection, resolved request URL,
    /// protocol profile, and model.
    /// 五个标识分别是当前供应商、凭据连接、已解析请求 URL、协议 Profile 与模型。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the current version and an exact route match.
    /// 仅当前版本且路由完全匹配时返回 `true`。
    pub fn matches_route(
        &self,
        provider_id: &str,
        connection_id: &str,
        route_fingerprint: &str,
        protocol_profile_id: &str,
        model_id: &str,
    ) -> bool {
        self.version == PROVIDER_REPLAY_ENVELOPE_VERSION
            && self.provider_id == provider_id
            && self.connection_id == connection_id
            && self.route_fingerprint == route_fingerprint
            && self.protocol_profile_id == protocol_profile_id
            && self.model_id == model_id
    }
}

/// Five immutable identifiers of one exact physical Provider execution route.
/// 一次精确物理 Provider 执行路由的五个不可变标识。
///
/// # Remarks
/// 说明
/// The route fingerprint is the resolved request URL captured by the producing adapter
/// (`route.request_url("/responses")`); projecting with any other endpoint convention can never
/// match a durable envelope.
/// 路由指纹是产生端适配器捕获的已解析请求 URL（`route.request_url("/responses")`）；
/// 使用任何其他端点约定投影都不可能匹配已持久化封套。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderReplayRoute<'a> {
    /// Provider definition identifier of the active route.
    /// 活动路由的供应商定义标识。
    pub provider_id: &'a str,
    /// Credential connection identifier of the active route.
    /// 活动路由的凭据连接标识。
    pub connection_id: &'a str,
    /// Non-secret resolved request URL of the active route.
    /// 活动路由不含秘密的已解析请求 URL。
    pub route_fingerprint: &'a str,
    /// Active protocol profile identifier.
    /// 活动的协议 Profile 标识。
    pub protocol_profile_id: &'a str,
    /// Active upstream model identifier.
    /// 活动的上游模型标识。
    pub model_id: &'a str,
}

/// Why one route-matched envelope cannot supply ordered provider-native replay items.
/// 一份路由匹配的封套为何无法提供有序供应商原生重放项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderReplayUnusable {
    /// One persisted provider item is outside the closed replayable vocabulary.
    /// 一个已持久化供应商项落在封闭可重放词表之外。
    ProviderItemRejected {
        /// Zero-based position of the rejected item.
        /// 被拒项从零开始的位置。
        index: usize,
    },
    /// Two persisted provider items claim one tool-call identity.
    /// 两个已持久化供应商项声明了同一个工具调用身份。
    DuplicateToolCallIdentity {
        /// Repeated tool-call identity.
        /// 重复的工具调用身份。
        call_id: String,
    },
    /// Ordered provider items do not cover the canonical assistant output they would replace.
    /// 有序供应商项没有覆盖它们将要替代的规范助手输出。
    CoverageMismatch {
        /// Human-readable description of the first uncovered or reordered segment.
        /// 首个未覆盖或次序不符片段的可读描述。
        detail: String,
    },
}

/// Four-way projection of one durable replay envelope for one frozen route.
/// 一份可持久化重放封套在一条冻结路由上的四态投影结果。
///
/// # Remarks
/// 说明
/// The four variants are intentionally distinct so callers can never turn a route mismatch or an
/// unusable envelope into an implicit local rebuild.
/// 四个变体刻意彼此区分，使调用方无法把路由不匹配或不可用封套变成隐式的本地重建。
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderReplayProjection {
    /// Route matched and ordered provider-native items must be replayed verbatim.
    /// 路由匹配，必须原样重放有序供应商原生项。
    ProviderItems(Vec<Value>),
    /// Route matched and only ordered canonical content is available.
    /// 路由匹配，但只有有序规范内容可用。
    CanonicalContent(Vec<ContentItem>),
    /// The envelope belongs to a different route or a closed older format version.
    /// 封套属于不同路由或已关闭的旧格式版本。
    RouteMismatch,
    /// Route matched but the persisted content cannot be replayed and must not be guessed at.
    /// 路由匹配，但已持久化内容不可重放，且不得对其进行猜测。
    Unusable(ProviderReplayUnusable),
}

/// Ordered assistant-authored kind of one Responses item or canonical content item.
/// 一个 Responses 项或规范内容项的有序助手归属种类。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReplayAssistantSegment {
    /// Model reasoning segment.
    /// 模型推理片段。
    Reasoning,
    /// Assistant-visible text segment.
    /// 助手可见正文片段。
    Text,
    /// One assistant tool call carrying its exact provider-visible identity.
    /// 一个携带精确供应商可见身份的助手工具调用。
    ToolCall(String),
}

/// Reads the concatenated visible text of one assistant `message` item.
/// 读取一个助手 `message` 项的拼接可见正文。
fn responses_message_text(item: &Value) -> String {
    match item.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Reads the provider-visible tool-call identity of one persisted Responses item.
/// 读取一个已持久化 Responses 项的供应商可见工具调用身份。
///
/// # Remarks
/// 说明
/// Function-style items carry `call_id`; hosted provider tools carry `id`. Both are accepted so a
/// hosted call can never be mistaken for an absent call.
/// 函数式项携带 `call_id`；供应商托管工具携带 `id`。两者都被接受，使托管调用绝不会被误判为缺失调用。
/// One provider tool call borrowed from its persisted Responses item.
/// 从一个已持久化 Responses 项借用的供应商工具调用。
///
/// # Remarks
/// 说明
/// Function-style items carry `call_id`, `name` and `arguments`; hosted provider tools carry only
/// `id`, so a field the provider never reported is left absent and is not compared.
/// 函数式项携带 `call_id`、`name` 与 `arguments`；供应商托管工具只携带 `id`，
/// 因此供应商从未报告的字段保持缺失且不参与比较。
struct ProviderToolCall<'a> {
    /// Provider-visible call identifier shared with the canonical tool call.
    /// 与规范工具调用共享的供应商可见调用标识。
    call_id: &'a str,
    /// Reported function name, when the item carries one.
    /// 该项携带的函数名（若存在）。
    name: Option<String>,
    /// Raw JSON argument text exactly as the provider reported it.
    /// 供应商报告的原始 JSON 参数文本。
    arguments: Option<String>,
}

fn provider_item_tool_identity(item: &Value) -> Option<&str> {
    match item.get("type").and_then(Value::as_str)? {
        "function_call"
        | "custom_tool_call"
        | "local_shell_call"
        | "tool_search_call"
        | "web_search_call"
        | "image_generation_call"
        | "mcp_call" => item
            .get("call_id")
            .and_then(Value::as_str)
            .or_else(|| item.get("id").and_then(Value::as_str)),
        _ => None,
    }
}

/// Maps one persisted Responses item onto the assistant segment it replays, when it replays one.
/// 把一个已持久化 Responses 项映射到它所重放的助手片段（若确实重放片段）。
fn provider_item_assistant_segment(item: &Value) -> Option<ReplayAssistantSegment> {
    match item.get("type").and_then(Value::as_str)? {
        // Only full reasoning content becomes canonical reasoning; an encrypted-only or
        // summary-only item is replayed as an opaque continuation that canonical content never declares.
        // 只有完整推理正文会成为规范推理；仅加密或仅摘要的项作为不透明延续重放，规范内容从不声明它。
        "reasoning" => item
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|content| !content.is_empty())
            .then_some(ReplayAssistantSegment::Reasoning),
        "message" => {
            (!responses_message_text(item).is_empty()).then_some(ReplayAssistantSegment::Text)
        }
        _ => provider_item_tool_identity(item)
            .map(|call_id| ReplayAssistantSegment::ToolCall(call_id.to_string())),
    }
}

/// Maps one canonical content item onto the assistant segment it contributes.
/// 把一个规范内容项映射到它所贡献的助手片段。
fn canonical_assistant_segment(item: &ContentItem) -> Option<ReplayAssistantSegment> {
    match item {
        ContentItem::Reasoning { .. } => Some(ReplayAssistantSegment::Reasoning),
        ContentItem::Text { text } | ContentItem::Refusal { text } => {
            (!text.is_empty()).then_some(ReplayAssistantSegment::Text)
        }
        ContentItem::StructuredJson { .. } => Some(ReplayAssistantSegment::Text),
        ContentItem::ToolCall { call } => Some(ReplayAssistantSegment::ToolCall(call.id.clone())),
        ContentItem::Resource { .. }
        | ContentItem::ToolResult { .. }
        | ContentItem::Citation { .. } => None,
    }
}

/// Proves that ordered provider items still cover the canonical assistant output in order.
/// 证明有序供应商项仍按次序覆盖规范助手输出。
///
/// # Parameters
/// 参数
/// `items` is the persisted provider output order and `content` is the canonical assistant content
/// that the provider items would replace.
/// `items` 是已持久化的供应商输出顺序，`content` 是供应商项将要替代的规范助手内容。
///
/// # Returns
/// 返回值
/// Success only when every replayed reasoning/text segment matches in order and every canonical
/// tool-call identity is present among the provider items.
/// 仅当每个被重放的推理/正文片段按序匹配、且每个规范工具调用身份都存在于供应商项中时返回成功。
fn verify_provider_item_coverage(items: &[Value], content: &[ContentItem]) -> Result<(), String> {
    let ordered: Vec<ReplayAssistantSegment> = items
        .iter()
        .filter_map(provider_item_assistant_segment)
        .filter(|segment| !matches!(segment, ReplayAssistantSegment::ToolCall(_)))
        .collect();
    let canonical_ordered: Vec<ReplayAssistantSegment> = content
        .iter()
        .filter_map(canonical_assistant_segment)
        .filter(|segment| !matches!(segment, ReplayAssistantSegment::ToolCall(_)))
        .collect();
    if ordered != canonical_ordered {
        return Err(format!(
            "provider items replay {ordered:?} but canonical content declares {canonical_ordered:?}"
        ));
    }
    // Tool calls are compared as an ordered sequence with their names and arguments, not as a set:
    // a reordered or renamed batch would change the conversation the model sees even though every
    // identifier is still present.
    // 工具调用按带名称与参数的有序序列比较，而不是集合：
    // 即使每个标识都还在，重排或改名后的批次仍会改变模型看到的对话。
    let provider_calls = items
        .iter()
        .filter_map(provider_item_tool_call)
        .collect::<Vec<_>>();
    let mut search_from = 0usize;
    for canonical_item in content {
        let ContentItem::ToolCall { call } = canonical_item else {
            continue;
        };
        let Some(offset) = provider_calls[search_from..]
            .iter()
            .position(|provider_call| provider_call.call_id == call.id)
        else {
            return Err(format!(
                "canonical tool call `{}` has no provider item left on this route in order",
                call.id
            ));
        };
        let provider_call = &provider_calls[search_from + offset];
        // Only a field the provider actually reported is compared; a hosted call reports no name
        // and no arguments, so its identity plus order remain the whole contract.
        // 只比较供应商确实报告的字段；托管调用不报告名称与参数，
        // 因此其标识与顺序就是全部契约。
        if let Some(provider_name) = provider_call.name.as_deref()
            && provider_name != call.name
        {
            return Err(format!(
                "provider tool call `{}` is named `{provider_name}` but canonical content declares `{}`",
                call.id, call.name
            ));
        }
        if let Some(provider_arguments) = provider_call.arguments.as_deref()
            && let Ok(provider_arguments) = serde_json::from_str::<Value>(provider_arguments)
            && provider_arguments != call.arguments
        {
            return Err(format!(
                "provider tool call `{}` carries different arguments than canonical content",
                call.id
            ));
        }
        search_from += offset + 1;
    }
    Ok(())
}

/// Reads one provider tool call with the identity, name and raw arguments it carries.
/// 读取一个供应商工具调用及其携带的标识、名称与原始参数。
fn provider_item_tool_call(item: &Value) -> Option<ProviderToolCall<'_>> {
    Some(ProviderToolCall {
        call_id: provider_item_tool_identity(item)?,
        name: item.get("name").and_then(Value::as_str).map(str::to_string),
        arguments: item
            .get("arguments")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// Projects one durable replay envelope onto one frozen route without mutating either input.
/// 在一次不修改任一输入的前提下，把一份可持久化重放封套投影到一条冻结路由上。
///
/// # Parameters
/// 参数
/// `envelope` is the persisted strong-typed replay state and `route` is the five-identifier
/// identity of the route that is about to dispatch.
/// `envelope` 是已持久化的强类型重放状态，`route` 是即将分派的路由的五标识身份。
///
/// # Returns
/// 返回值
/// Provider items, canonical content, a route mismatch, or a controlled unusable reason. Callers
/// may project portable text for a route mismatch, but must not transfer native signatures or
/// treat an unusable same-route envelope as a model switch.
/// 返回供应商项、规范内容、路由不匹配或受控的不可用原因。调用方可为路由不匹配投影可移植文本，
/// 但不得传递原生签名，也不得把不可用的同路由封套当作模型切换。
pub fn project_provider_replay(
    envelope: &ProviderReplayEnvelope,
    route: &ProviderReplayRoute<'_>,
) -> ProviderReplayProjection {
    if !envelope.matches_route(
        route.provider_id,
        route.connection_id,
        route.route_fingerprint,
        route.protocol_profile_id,
        route.model_id,
    ) {
        return ProviderReplayProjection::RouteMismatch;
    }
    if envelope.provider_context_items.is_empty() {
        return ProviderReplayProjection::CanonicalContent(envelope.content.clone());
    }
    let mut identities: Vec<&str> = Vec::new();
    for (index, item) in envelope.provider_context_items.iter().enumerate() {
        if !is_replayable_codex_responses_item(item) {
            return ProviderReplayProjection::Unusable(
                ProviderReplayUnusable::ProviderItemRejected { index },
            );
        }
        if let Some(call_id) = provider_item_tool_identity(item) {
            if identities.contains(&call_id) {
                return ProviderReplayProjection::Unusable(
                    ProviderReplayUnusable::DuplicateToolCallIdentity {
                        call_id: call_id.to_string(),
                    },
                );
            }
            identities.push(call_id);
        }
    }
    match verify_provider_item_coverage(&envelope.provider_context_items, &envelope.content) {
        Ok(()) => ProviderReplayProjection::ProviderItems(envelope.provider_context_items.clone()),
        Err(detail) => {
            ProviderReplayProjection::Unusable(ProviderReplayUnusable::CoverageMismatch { detail })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProviderReplayEnvelope, ProviderReplayProjection, ProviderReplayRoute,
        ProviderReplayUnusable, canonicalize_codex_responses_replay_items,
        is_replayable_codex_responses_item, project_provider_replay,
    };
    use crate::{ContentItem, ToolCall};
    use serde_json::json;

    /// Verifies official history item variants are accepted while request-only controls are rejected.
    /// 验证官方历史项变体会被接受，而仅请求控制项会被拒绝。
    #[test]
    fn codex_responses_replay_item_allowlist_is_explicit() {
        assert!(is_replayable_codex_responses_item(&json!({
            "type": "compaction",
            "encrypted_content": "opaque"
        })));
        assert!(!is_replayable_codex_responses_item(&json!({
            "type": "compaction",
            "encrypted_content": ""
        })));
        assert!(!is_replayable_codex_responses_item(&json!({
            "type": "compaction"
        })));
        assert!(is_replayable_codex_responses_item(&json!({
            "type": "message",
            "role": "assistant",
            "content": []
        })));
        assert!(!is_replayable_codex_responses_item(&json!({
            "type": "compaction_trigger"
        })));
        assert!(!is_replayable_codex_responses_item(&json!({
            "type": "provider_private_item"
        })));
    }

    /// Verifies the official `compaction_summary` input alias is stored as canonical `compaction`.
    /// 验证官方 `compaction_summary` 输入别名按规范 `compaction` 值存储。
    #[test]
    fn codex_responses_compaction_alias_is_canonicalized() {
        // items contains the exact alias accepted by the official Codex ResponseItem deserializer.
        // items 包含官方 Codex ResponseItem 反序列化器接受的精确别名。
        let mut items = vec![json!({
            "type": "compaction_summary",
            "encrypted_content": "opaque"
        })];

        canonicalize_codex_responses_replay_items(&mut items);

        assert_eq!(items[0]["type"], "compaction");
        assert!(is_replayable_codex_responses_item(&items[0]));
    }

    /// Verifies ordinary replay signatures are scoped to the exact credential and resolved route.
    /// 验证普通重放签名会限定到精确凭据与已解析路由。
    #[test]
    fn provider_replay_envelope_requires_exact_connection_route() {
        // envelope owns provider-generated content that must not cross credential boundaries.
        // envelope 持有不得跨越凭据边界的供应商生成内容。
        let envelope = ProviderReplayEnvelope::new(
            "custom:test",
            "connection-a",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5.6-luna",
            vec![ContentItem::Reasoning {
                text: String::from("reasoning"),
                signature: Some(String::from("opaque-signature")),
            }],
            Vec::new(),
        );

        assert!(envelope.matches_route(
            "custom:test",
            "connection-a",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5.6-luna",
        ));
        assert!(!envelope.matches_route(
            "custom:test",
            "connection-b",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5.6-luna",
        ));
        assert!(!envelope.matches_route(
            "custom:test",
            "connection-a",
            "https://proxy.example/changed/responses",
            "openai-codex-responses.v1",
            "gpt-5.6-luna",
        ));
    }

    /// Builds the frozen DeepSeek Responses route identity used by projection tests.
    /// 构建投影测试使用的冻结 DeepSeek Responses 路由身份。
    fn deepseek_responses_route() -> ProviderReplayRoute<'static> {
        ProviderReplayRoute {
            provider_id: "deepseek",
            connection_id: "connection-a",
            route_fingerprint: "https://api.deepseek.com/responses",
            protocol_profile_id: "openai-responses.v1",
            model_id: "deepseek-flash",
        }
    }

    /// Builds one canonical tool call carrying an exact provider-visible identity.
    /// 构建一个携带精确供应商可见身份的规范工具调用。
    fn projection_tool_call(call_id: &str) -> ContentItem {
        ContentItem::ToolCall {
            call: ToolCall {
                id: String::from(call_id),
                name: String::from("read_file"),
                arguments: json!({}),
                signature: None,
            },
        }
    }

    /// Builds the ordered raw items and canonical content of one reasoning-then-tool turn.
    /// 构建一轮“先推理再工具调用”的有序原始项与规范内容。
    fn reasoning_then_tool_fixture() -> (Vec<ContentItem>, Vec<serde_json::Value>) {
        (
            vec![
                ContentItem::Reasoning {
                    text: String::from("think"),
                    signature: None,
                },
                projection_tool_call("call-1"),
            ],
            vec![
                json!({
                    "type": "reasoning",
                    "content": [{"type": "reasoning_text", "text": "think"}]
                }),
                json!({
                    "type": "function_call",
                    "call_id": "call-1",
                    "name": "read_file",
                    "arguments": "{}"
                }),
            ],
        )
    }

    /// Builds one route-bound envelope around the supplied content and provider items.
    /// 围绕给定内容与供应商项构建一份路由绑定封套。
    fn projection_envelope(
        content: Vec<ContentItem>,
        provider_context_items: Vec<serde_json::Value>,
    ) -> ProviderReplayEnvelope {
        ProviderReplayEnvelope::new(
            "deepseek",
            "connection-a",
            "https://api.deepseek.com/responses",
            "openai-responses.v1",
            "deepseek-flash",
            content,
            provider_context_items,
        )
    }

    /// Verifies a fully matching route replays ordered provider items instead of canonical content.
    /// 验证完全匹配的路由重放有序供应商项，而不是规范内容。
    #[test]
    fn provider_replay_projection_replays_ordered_provider_items() {
        // items preserves the exact provider output order that must reach the wire unchanged.
        // items 保留必须原样到达线路的精确供应商输出顺序。
        let (content, items) = reasoning_then_tool_fixture();
        let envelope = projection_envelope(content, items.clone());

        assert_eq!(
            project_provider_replay(&envelope, &deepseek_responses_route()),
            ProviderReplayProjection::ProviderItems(items)
        );
    }

    /// Verifies each single route identifier mismatch is reported as a route mismatch.
    /// 验证每一项路由标识单独不匹配时都被报告为路由不匹配。
    #[test]
    fn provider_replay_projection_rejects_every_single_route_mismatch() {
        let (content, items) = reasoning_then_tool_fixture();
        let envelope = projection_envelope(content, items);

        // mismatches isolates one identifier at a time so no field can silently stop being checked.
        // mismatches 每次只隔离一个标识，使任何字段都不会静默地不再被校验。
        let mismatches = [
            ProviderReplayRoute {
                provider_id: "other",
                ..deepseek_responses_route()
            },
            ProviderReplayRoute {
                connection_id: "connection-b",
                ..deepseek_responses_route()
            },
            ProviderReplayRoute {
                route_fingerprint: "https://api.deepseek.com/chat/completions",
                ..deepseek_responses_route()
            },
            ProviderReplayRoute {
                protocol_profile_id: "openai-chat-completions.v1",
                ..deepseek_responses_route()
            },
            ProviderReplayRoute {
                model_id: "deepseek-other",
                ..deepseek_responses_route()
            },
        ];

        for route in mismatches {
            assert_eq!(
                project_provider_replay(&envelope, &route),
                ProviderReplayProjection::RouteMismatch,
                "route {route:?} must not replay another route's envelope"
            );
        }

        // closed holds the same identity with a retired format version.
        // closed 持有相同身份但使用已停用的格式版本。
        let mut closed = projection_envelope(Vec::new(), Vec::new());
        closed.version = 1;
        assert_eq!(
            project_provider_replay(&closed, &deepseek_responses_route()),
            ProviderReplayProjection::RouteMismatch
        );
    }

    /// Verifies a matching route without provider items falls back to ordered canonical content.
    /// 验证匹配路由在缺少供应商项时回退到有序规范内容。
    #[test]
    fn provider_replay_projection_uses_canonical_content_without_provider_items() {
        let (content, _) = reasoning_then_tool_fixture();
        let envelope = projection_envelope(content.clone(), Vec::new());

        assert_eq!(
            project_provider_replay(&envelope, &deepseek_responses_route()),
            ProviderReplayProjection::CanonicalContent(content)
        );
    }

    /// Verifies a provider item outside the closed vocabulary is rejected before dispatch.
    /// 验证封闭词表之外的供应商项会在分派前被拒绝。
    #[test]
    fn provider_replay_projection_rejects_non_replayable_provider_item() {
        let (content, mut items) = reasoning_then_tool_fixture();
        items.push(json!({"type": "provider_private_item"}));
        let envelope = projection_envelope(content, items);

        assert_eq!(
            project_provider_replay(&envelope, &deepseek_responses_route()),
            ProviderReplayProjection::Unusable(ProviderReplayUnusable::ProviderItemRejected {
                index: 2
            })
        );
    }

    /// Verifies two provider items claiming one tool identity are rejected before dispatch.
    /// 验证两个供应商项声明同一工具身份时会在分派前被拒绝。
    #[test]
    fn provider_replay_projection_rejects_duplicate_tool_identity() {
        let (content, mut items) = reasoning_then_tool_fixture();
        items.push(json!({
            "type": "function_call",
            "call_id": "call-1",
            "name": "read_file",
            "arguments": "{}"
        }));
        let envelope = projection_envelope(content, items);

        assert_eq!(
            project_provider_replay(&envelope, &deepseek_responses_route()),
            ProviderReplayProjection::Unusable(ProviderReplayUnusable::DuplicateToolCallIdentity {
                call_id: String::from("call-1")
            })
        );
    }

    /// Verifies provider items that stop covering the canonical output are rejected before dispatch.
    /// 验证不再覆盖规范输出的供应商项会在分派前被拒绝。
    #[test]
    fn provider_replay_projection_rejects_incomplete_coverage() {
        // truncated drops the assistant message while canonical content still declares its text.
        // truncated 丢弃助手消息，而规范内容仍声明其正文。
        let (mut content, mut items) = reasoning_then_tool_fixture();
        content.insert(
            0,
            ContentItem::Text {
                text: String::from("answer"),
            },
        );
        items.retain(|item| item["type"] != "message");
        let envelope = projection_envelope(content, items);

        assert!(matches!(
            project_provider_replay(&envelope, &deepseek_responses_route()),
            ProviderReplayProjection::Unusable(ProviderReplayUnusable::CoverageMismatch { .. })
        ));
    }

    /// Verifies encrypted-only and summary-only reasoning replays without declaring canonical reasoning.
    /// 验证仅加密与仅摘要的推理项在规范内容不声明推理时仍可重放。
    #[test]
    fn provider_replay_projection_replays_opaque_reasoning_without_canonical_reasoning() {
        // The decoder records no canonical reasoning for these items, only the tool call.
        // 解码器不为这些项记录规范推理，只记录工具调用。
        for reasoning in [
            json!({"type": "reasoning", "content": [], "summary": [], "encrypted_content": "opaque"}),
            json!({
                "type": "reasoning",
                "summary": [{"type": "summary_text", "text": "plan"}],
                "encrypted_content": "opaque"
            }),
        ] {
            let items = vec![
                reasoning,
                json!({
                    "type": "function_call",
                    "call_id": "call-1",
                    "name": "read_file",
                    "arguments": "{}"
                }),
            ];
            let envelope = projection_envelope(vec![projection_tool_call("call-1")], items.clone());

            assert_eq!(
                project_provider_replay(&envelope, &deepseek_responses_route()),
                ProviderReplayProjection::ProviderItems(items)
            );
        }
    }

    /// Verifies a canonical tool call without any provider item identity is rejected before dispatch.
    /// 验证规范工具调用缺少任何供应商项身份时会在分派前被拒绝。
    #[test]
    fn provider_replay_projection_rejects_unpaired_canonical_tool_call() {
        let (mut content, items) = reasoning_then_tool_fixture();
        content.push(projection_tool_call("call-9"));
        let envelope = projection_envelope(content, items);

        assert!(matches!(
            project_provider_replay(&envelope, &deepseek_responses_route()),
            ProviderReplayProjection::Unusable(ProviderReplayUnusable::CoverageMismatch { .. })
        ));
    }

    /// Verifies a hosted provider tool identified by `id` still pairs with its canonical call.
    /// 验证以 `id` 标识的供应商托管工具仍能与其规范调用配对。
    #[test]
    fn provider_replay_projection_pairs_hosted_tool_identity() {
        // hosted carries `id` rather than `call_id`, which must not be mistaken for an absent call.
        // hosted 携带 `id` 而不是 `call_id`，不得被误判为缺失调用。
        let items = vec![
            json!({
                "type": "reasoning",
                "content": [{"type": "reasoning_text", "text": "think"}]
            }),
            json!({"type": "web_search_call", "id": "ws-1", "status": "completed"}),
        ];
        let content = vec![
            ContentItem::Reasoning {
                text: String::from("think"),
                signature: None,
            },
            projection_tool_call("ws-1"),
        ];
        let envelope = projection_envelope(content, items.clone());

        assert_eq!(
            project_provider_replay(&envelope, &deepseek_responses_route()),
            ProviderReplayProjection::ProviderItems(items)
        );
    }

    /// Verifies a tool batch must keep its provider order, names and arguments, not only its ids.
    /// 验证工具批次必须保持供应商的顺序、名称与参数，而不只是标识。
    ///
    /// # Remarks
    /// 说明
    /// Every identifier can still be present while the batch the model saw has changed, so a
    /// set-style check cannot prove the replayed history is the recorded one.
    /// 即使每个标识都还在，模型看到的批次也可能已经改变，
    /// 因此集合式校验无法证明被回放的历史就是被记录的那一段。
    #[test]
    fn provider_replay_projection_requires_ordered_named_tool_calls() {
        // canonical is one turn that called two tools in a fixed order.
        // canonical 是一个按固定顺序调用两个工具的回合。
        let canonical = || {
            vec![
                ContentItem::Reasoning {
                    text: String::from("think"),
                    signature: None,
                },
                projection_tool_call("call-1"),
                projection_tool_call("call-2"),
            ]
        };
        // providerCall builds one persisted function call with an explicit name and raw arguments.
        // providerCall 构建一个带明确名称与原始参数的已持久化函数调用。
        let provider_call = |call_id: &str, name: &str, arguments: &str| {
            json!({
                "type": "function_call",
                "call_id": call_id,
                "name": name,
                "arguments": arguments
            })
        };
        let items = |first: serde_json::Value, second: serde_json::Value| {
            vec![
                json!({
                    "type": "reasoning",
                    "content": [{"type": "reasoning_text", "text": "think"}]
                }),
                first,
                second,
            ]
        };
        let projection = |first: serde_json::Value, second: serde_json::Value| {
            project_provider_replay(
                &projection_envelope(canonical(), items(first, second)),
                &deepseek_responses_route(),
            )
        };

        // The exact batch replays unchanged.
        // 完全一致的批次原样回放。
        assert!(matches!(
            projection(
                provider_call("call-1", "read_file", "{}"),
                provider_call("call-2", "read_file", "{}")
            ),
            ProviderReplayProjection::ProviderItems(_)
        ));
        // Swapping the two calls changes the conversation even though both identities survive.
        // 交换两个调用会改变对话，即使两个标识都还在。
        assert!(matches!(
            projection(
                provider_call("call-2", "read_file", "{}"),
                provider_call("call-1", "read_file", "{}")
            ),
            ProviderReplayProjection::Unusable(ProviderReplayUnusable::CoverageMismatch { .. })
        ));
        // A renamed call is a different call.
        // 改名的调用就是另一个调用。
        assert!(matches!(
            projection(
                provider_call("call-1", "write_file", "{}"),
                provider_call("call-2", "read_file", "{}")
            ),
            ProviderReplayProjection::Unusable(ProviderReplayUnusable::CoverageMismatch { .. })
        ));
        // Arguments the ledger diverged from are refused instead of being silently overwritten.
        // 与账本发生分歧的参数会被拒绝，而不是被静默覆盖。
        assert!(matches!(
            projection(
                provider_call("call-1", "read_file", "{\"path\":\"a\"}"),
                provider_call("call-2", "read_file", "{}")
            ),
            ProviderReplayProjection::Unusable(ProviderReplayUnusable::CoverageMismatch { .. })
        ));
    }
}
