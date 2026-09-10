//! Provider-neutral tool call contracts.
//! 供应商无关的工具调用契约。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable name of the neutral Apply Patch tool.
/// 中立 Apply Patch 工具的稳定名称。
pub const APPLY_PATCH_TOOL_NAME: &str = "apply_patch";

/// Complete neutral name of the Apply Patch input language.
/// Apply Patch 输入语言的完整中立名称。
pub const APPLY_PATCH_LANGUAGE_NAME: &str = "Apply Patch text format";

/// Canonical Lark grammar for the Apply Patch custom-text projection.
/// Apply Patch 自定义文本投影使用的规范 Lark grammar。
pub use vulcan_apply_patch::APPLY_PATCH_LARK_GRAMMAR;

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

/// Selects how one model-visible tool receives its wire input.
/// 选择一个模型可见工具如何接收线路输入。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolInputFormat {
    /// Uses a JSON value constrained by the tool's input schema.
    /// 使用受工具输入 Schema 约束的 JSON 值。
    #[default]
    JsonSchema,
    /// Uses exact freeform text constrained by a provider-visible format.
    /// 使用受供应商可见格式约束的精确自由文本。
    Freeform {
        /// Provider-visible text format declaration.
        /// 面向供应商的文本格式声明。
        format: FreeformToolFormat,
        /// Host-side representation used for the completed public tool call.
        /// 已完成公共工具调用使用的宿主侧表示。
        argument_encoding: FreeformToolArgumentEncoding,
    },
}

/// Declares one closed provider-visible freeform tool format.
/// 声明一种封闭的供应商可见自由文本工具格式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FreeformToolFormat {
    /// Constrains text with an exact grammar definition.
    /// 使用精确 grammar 定义约束文本。
    Grammar {
        /// Grammar language understood by the selected protocol.
        /// 所选协议能够理解的 grammar 语言。
        syntax: FreeformToolSyntax,
        /// Complete provider-visible grammar definition.
        /// 完整的供应商可见 grammar 定义。
        definition: String,
    },
}

/// Identifies the closed grammar syntax understood by registered Responses profiles.
/// 标识已注册 Responses Profile 能够理解的封闭 grammar 语法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreeformToolSyntax {
    /// Lark grammar syntax.
    /// Lark grammar 语法。
    Lark,
}

/// Selects the host representation of completed freeform tool text.
/// 选择已完成自由文本工具正文的宿主表示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreeformToolArgumentEncoding {
    /// Preserves the exact text as a JSON string value.
    /// 将精确正文保留为 JSON 字符串值。
    RawText,
    /// Wraps Apply Patch text in the existing `{"patch": ...}` execution contract.
    /// 按既有 `{"patch": ...}` 执行契约包装 Apply Patch 正文。
    ApplyPatchObject,
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
    /// Exact wire input format and completed-call representation.
    /// 精确的线路输入格式与完成调用表示。
    #[serde(default)]
    pub input_format: ToolInputFormat,
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
    /// Complete decoded argument value.
    /// 完整解码后的参数值。
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
