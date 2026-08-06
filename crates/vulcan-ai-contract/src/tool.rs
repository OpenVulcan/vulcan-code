//! Provider-neutral tool call contracts.
//! 供应商无关的工具调用契约。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Identifies one provider-native tool requested through provider-neutral semantics.
/// 标识一项通过供应商无关语义请求的供应商原生工具。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeToolKind {
    /// Provider-native web search.
    /// 供应商原生网络搜索。
    WebSearch,
    /// Provider-native managed code execution.
    /// 供应商原生托管代码执行。
    CodeInterpreter,
    /// Provider-native image generation.
    /// 供应商原生图片生成。
    ImageGeneration,
}

/// Describes one tool visible to a model.
/// 描述一个对模型可见的工具。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Stable tool name.
    /// 稳定工具名称。
    pub name: String,
    /// English model-facing description.
    /// 面向模型的英文说明。
    pub description: String,
    /// JSON Schema for tool arguments.
    /// 工具参数的 JSON Schema。
    pub input_schema: Value,
    /// Whether the provider must enforce exact schema-conforming arguments.
    /// 供应商是否必须强制参数严格符合模式。
    #[serde(default)]
    pub strict: bool,
}

/// Carries one complete model-emitted tool call.
/// 承载一个完整的模型工具调用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Stable call identifier.
    /// 稳定调用标识。
    pub id: String,
    /// Tool name.
    /// 工具名称。
    pub name: String,
    /// Complete decoded argument object.
    /// 完整解码后的参数对象。
    pub arguments: Value,
    /// Optional provider-scoped continuation signature attached to this exact call.
    /// 绑定到当前精确调用的可选供应商作用域延续签名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Carries one tool result returned to the model.
/// 承载一个返回给模型的工具结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// Tool call identifier being answered.
    /// 被响应的工具调用标识。
    pub call_id: String,
    /// Whether the result represents an error.
    /// 结果是否表示错误。
    pub is_error: bool,
    /// Structured result content.
    /// 结构化结果内容。
    pub content: Value,
}

/// Tracks an incomplete streamed tool call.
/// 跟踪一个尚未完成的流式工具调用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallProgress {
    /// Tool call identifier when already known.
    /// 已知时的工具调用标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    /// Tool name when already known.
    /// 已知时的工具名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Number of UTF-8 bytes received for the argument stream.
    /// 参数流已接收的 UTF-8 字节数。
    pub bytes_received: u64,
    /// Number of decoded Unicode scalar values received.
    /// 已接收的 Unicode 标量字符数。
    pub characters_received: u64,
    /// Whether the argument JSON is complete.
    /// 参数 JSON 是否完整。
    pub complete: bool,
}
