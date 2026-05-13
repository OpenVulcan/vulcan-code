//! Host-owned Cohere native protocol models used by the direct Cohere transport.
//! 供直连 Cohere 传输复用的宿主自有 Cohere 原生协议模型。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One native Cohere chat message kept inside the host-owned transport layer.
/// 由宿主自有传输层保留的一条 Cohere 原生对话消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub(crate) enum CohereChatMessage {
    /// One user message containing one ordered content list.
    /// 一条包含有序内容列表的用户消息。
    User {
        /// Ordered user content list.
        /// 有序用户内容列表。
        content: Vec<CohereUserContent>,
    },
    /// One assistant message containing visible text, optional thinking, and optional tool calls.
    /// 一条包含可见文本、可选思考和可选工具调用的助手消息。
    Assistant {
        /// Ordered assistant content list.
        /// 有序助手内容列表。
        #[serde(default)]
        content: Vec<CohereAssistantContent>,
        /// Provider-native tool call list.
        /// Provider 原生工具调用列表。
        #[serde(default)]
        tool_calls: Vec<CohereToolCall>,
        /// Optional tool-plan text retained by Cohere.
        /// Cohere 保留的可选工具计划文本。
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_plan: Option<String>,
        /// Optional citation list kept for wire compatibility.
        /// 为线协议兼容保留的可选引用列表。
        #[serde(default)]
        citations: Vec<Value>,
    },
    /// One tool-result message bound to one prior tool call identifier.
    /// 绑定到某条先前工具调用标识的一条工具结果消息。
    Tool {
        /// Tool result content list.
        /// 工具结果内容列表。
        content: Vec<CohereToolResultContent>,
        /// Stable upstream tool call identifier.
        /// 稳定的上游工具调用标识。
        tool_call_id: String,
    },
    /// One system instruction message.
    /// 一条系统指令消息。
    System {
        /// Plain system text.
        /// 纯系统文本。
        content: String,
    },
}

/// One native Cohere user content item.
/// 一条 Cohere 原生用户内容项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum CohereUserContent {
    /// One plain-text user content item.
    /// 一条纯文本用户内容项。
    Text {
        /// Visible text body.
        /// 可见文本正文。
        text: String,
    },
}

/// One native Cohere assistant content item.
/// 一条 Cohere 原生助手内容项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum CohereAssistantContent {
    /// One visible assistant text item.
    /// 一条可见助手文本项。
    Text {
        /// Visible text body.
        /// 可见文本正文。
        text: String,
    },
    /// One assistant thinking item.
    /// 一条助手思考内容项。
    Thinking {
        /// Provider-visible thinking text.
        /// Provider 可见思考文本。
        thinking: String,
    },
}

/// One native Cohere tool-result content item.
/// 一条 Cohere 原生工具结果内容项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum CohereToolResultContent {
    /// One plain-text tool result item.
    /// 一条纯文本工具结果项。
    Text {
        /// Visible text body.
        /// 可见文本正文。
        text: String,
    },
}

/// One native Cohere tool call emitted by the provider.
/// 由 Provider 发出的一条 Cohere 原生工具调用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereToolCall {
    /// Optional provider-native tool call identifier.
    /// 可选的 Provider 原生工具调用标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
    /// Optional tool call kind.
    /// 可选的工具调用类型。
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<CohereToolCallKind>,
    /// Optional function payload.
    /// 可选的函数载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) function: Option<CohereToolCallFunction>,
}

/// One native Cohere tool call kind.
/// 一种 Cohere 原生工具调用类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CohereToolCallKind {
    /// Standard function tool call.
    /// 标准函数工具调用。
    Function,
}

/// One native Cohere tool-call function payload.
/// 一条 Cohere 原生工具调用函数载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereToolCallFunction {
    /// Stable function name.
    /// 稳定函数名称。
    pub(crate) name: String,
    /// JSON-stringified argument object expected by Cohere.
    /// Cohere 期望的 JSON 字符串化参数对象。
    pub(crate) arguments: String,
}

/// One native Cohere tool declaration exposed in the request payload.
/// 在请求载荷中暴露的一条 Cohere 原生工具声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereToolDefinition {
    /// Native tool declaration kind.
    /// 原生工具声明类型。
    #[serde(rename = "type")]
    pub(crate) kind: CohereToolCallKind,
    /// Function definition payload.
    /// 函数定义载荷。
    pub(crate) function: CohereFunctionDefinition,
}

/// One native Cohere function definition mirrored from one host tool definition.
/// 从一条宿主工具定义镜像而来的一条 Cohere 原生函数定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereFunctionDefinition {
    /// Stable function name.
    /// 稳定函数名称。
    pub(crate) name: String,
    /// Human-readable function description.
    /// 人类可读函数描述。
    pub(crate) description: String,
    /// JSON schema parameters visible to the provider.
    /// 暴露给 Provider 的 JSON Schema 参数。
    pub(crate) parameters: Value,
}

/// One minimal Cohere streaming event used by the host-owned direct transport.
/// 宿主自有直连传输使用的一条最小 Cohere 流式事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(crate) enum CohereStreamingEvent {
    /// One message-start event.
    /// 一条消息开始事件。
    MessageStart {
        /// Optional response identifier.
        /// 可选的响应标识。
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Nested delta payload.
        /// 嵌套增量载荷。
        #[serde(skip_serializing_if = "Option::is_none")]
        delta: Option<CohereStreamingDeltaEnvelope>,
    },
    /// One content-start event.
    /// 一条内容开始事件。
    ContentStart {
        /// Stable content block index.
        /// 稳定内容块索引。
        index: usize,
        /// Nested delta payload.
        /// 嵌套增量载荷。
        #[serde(skip_serializing_if = "Option::is_none")]
        delta: Option<CohereStreamingDeltaEnvelope>,
    },
    /// One content-delta event.
    /// 一条内容增量事件。
    ContentDelta {
        /// Stable content block index.
        /// 稳定内容块索引。
        index: usize,
        /// Nested delta payload.
        /// 嵌套增量载荷。
        #[serde(skip_serializing_if = "Option::is_none")]
        delta: Option<CohereStreamingDeltaEnvelope>,
    },
    /// One content-end event.
    /// 一条内容结束事件。
    ContentEnd {
        /// Stable content block index.
        /// 稳定内容块索引。
        index: usize,
    },
    /// One tool-plan delta event.
    /// 一条工具计划增量事件。
    ToolPlanDelta {
        /// Nested delta payload.
        /// 嵌套增量载荷。
        #[serde(skip_serializing_if = "Option::is_none")]
        delta: Option<CohereStreamingDeltaEnvelope>,
    },
    /// One tool-call-start event.
    /// 一条工具调用开始事件。
    ToolCallStart {
        /// Stable tool call index.
        /// 稳定工具调用索引。
        index: usize,
        /// Nested delta payload.
        /// 嵌套增量载荷。
        #[serde(skip_serializing_if = "Option::is_none")]
        delta: Option<CohereStreamingDeltaEnvelope>,
    },
    /// One tool-call-delta event.
    /// 一条工具调用增量事件。
    ToolCallDelta {
        /// Stable tool call index.
        /// 稳定工具调用索引。
        index: usize,
        /// Nested delta payload.
        /// 嵌套增量载荷。
        #[serde(skip_serializing_if = "Option::is_none")]
        delta: Option<CohereStreamingDeltaEnvelope>,
    },
    /// One tool-call-end event.
    /// 一条工具调用结束事件。
    ToolCallEnd {
        /// Stable tool call index.
        /// 稳定工具调用索引。
        index: usize,
    },
    /// One citation-start event ignored by the current runtime.
    /// 被当前运行时忽略的一条引用开始事件。
    CitationStart {
        /// Stable citation index.
        /// 稳定引用索引。
        index: usize,
    },
    /// One citation-end event ignored by the current runtime.
    /// 被当前运行时忽略的一条引用结束事件。
    CitationEnd {
        /// Stable citation index.
        /// 稳定引用索引。
        index: usize,
    },
    /// One message-end event carrying finish reason and usage.
    /// 携带结束原因与用量的一条消息结束事件。
    MessageEnd {
        /// Optional response identifier.
        /// 可选的响应标识。
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Nested delta payload.
        /// 嵌套增量载荷。
        #[serde(skip_serializing_if = "Option::is_none")]
        delta: Option<CohereMessageEndDeltaEnvelope>,
    },
    /// One debug event ignored by the current runtime.
    /// 被当前运行时忽略的一条调试事件。
    Debug,
    /// Any unsupported event type.
    /// 任意当前不支持的事件类型。
    #[serde(other)]
    Unknown,
}

/// One nested Cohere streaming delta envelope.
/// 一条嵌套的 Cohere 流式增量包装对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereStreamingDeltaEnvelope {
    /// Optional nested message delta.
    /// 可选的嵌套消息增量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<CohereStreamingMessageDelta>,
}

/// One nested Cohere streaming message delta.
/// 一条嵌套的 Cohere 流式消息增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereStreamingMessageDelta {
    /// Optional content descriptor used by content-start/content-delta events.
    /// 被内容开始/内容增量事件使用的可选内容描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<CohereStreamingContentDelta>,
    /// Optional tool-plan text delta.
    /// 可选的工具计划文本增量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_plan: Option<String>,
    /// Optional tool-call payload.
    /// 可选的工具调用载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_calls: Option<CohereStreamingToolCallDelta>,
}

/// One nested Cohere streaming content descriptor.
/// 一条嵌套的 Cohere 流式内容描述。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereStreamingContentDelta {
    /// Optional content kind such as `text` or `thinking`.
    /// 可选内容类型，例如 `text` 或 `thinking`。
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<String>,
    /// Optional visible text fragment.
    /// 可选的可见文本片段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    /// Optional visible thinking fragment.
    /// 可选的可见思考片段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thinking: Option<String>,
}

/// One nested Cohere streaming tool-call delta payload.
/// 一条嵌套的 Cohere 流式工具调用增量载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereStreamingToolCallDelta {
    /// Optional provider-native tool call identifier.
    /// 可选的 Provider 原生工具调用标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
    /// Optional nested function delta.
    /// 可选的嵌套函数增量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) function: Option<CohereStreamingToolCallFunctionDelta>,
}

/// One nested Cohere streaming tool-call function delta payload.
/// 一条嵌套的 Cohere 流式工具调用函数增量载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereStreamingToolCallFunctionDelta {
    /// Optional function name fragment.
    /// 可选的函数名称片段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
    /// Optional JSON-stringified arguments fragment.
    /// 可选的 JSON 字符串化参数片段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) arguments: Option<String>,
}

/// One nested Cohere message-end delta envelope.
/// 一条嵌套的 Cohere 消息结束增量包装对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereMessageEndDeltaEnvelope {
    /// Optional finish reason.
    /// 可选结束原因。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) finish_reason: Option<String>,
    /// Optional usage payload.
    /// 可选用量载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) usage: Option<CohereUsage>,
}

/// One minimal Cohere usage payload.
/// 一条最小的 Cohere 用量载荷。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereUsage {
    /// Optional billed units payload.
    /// 可选的计费用量载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) billed_units: Option<CohereBilledUnits>,
    /// Optional token usage payload.
    /// 可选的 Token 用量载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tokens: Option<CohereTokenUsage>,
}

/// One minimal Cohere billed-units payload.
/// 一条最小的 Cohere 计费用量载荷。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereBilledUnits {
    /// Optional billed input tokens.
    /// 可选的计费输入 Token。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) input_tokens: Option<f64>,
    /// Optional billed output tokens.
    /// 可选的计费输出 Token。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) output_tokens: Option<f64>,
}

/// One minimal Cohere token-usage payload.
/// 一条最小的 Cohere Token 用量载荷。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct CohereTokenUsage {
    /// Optional input token count.
    /// 可选的输入 Token 数量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) input_tokens: Option<f64>,
    /// Optional output token count.
    /// 可选的输出 Token 数量。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) output_tokens: Option<f64>,
}
