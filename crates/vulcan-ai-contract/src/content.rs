//! Provider-neutral conversational content.
//! 供应商无关的对话内容。

use crate::{ResourceReference, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Identifies the semantic author of one message.
/// 标识一条消息的语义作者。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// System-authored instructions.
    /// 系统编写的指令。
    System,
    /// User-authored input.
    /// 用户编写的输入。
    User,
    /// Assistant-authored output.
    /// 助手编写的输出。
    Assistant,
    /// Tool-authored result.
    /// 工具编写的结果。
    Tool,
}

/// Carries one provider-neutral message.
/// 承载一条供应商无关消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiMessage {
    /// Semantic author of the message.
    /// 消息的语义作者。
    pub role: MessageRole,
    /// Ordered content items in the message.
    /// 消息中的有序内容项。
    pub content: Vec<ContentItem>,
    /// Provider-native context items that must be replayed only on the exact owning route.
    /// 仅应在精确归属路由上重放的供应商原生上下文项。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_context_items: Vec<Value>,
}

/// Represents one provider-neutral message content item.
/// 表示一个供应商无关的消息内容项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentItem {
    /// Plain text content.
    /// 纯文本内容。
    Text {
        /// Text payload.
        /// 文本载荷。
        text: String,
    },
    /// Structured JSON content.
    /// 结构化 JSON 内容。
    StructuredJson {
        /// Closed or provider-returned structured value.
        /// 封闭或供应商返回的结构化值。
        value: Value,
    },
    /// Model reasoning content.
    /// 模型推理内容。
    Reasoning {
        /// Visible or internally retained reasoning text.
        /// 可见或内部保留的推理文本。
        text: String,
        /// Optional provider signature required for continuation.
        /// 延续请求所需的可选供应商签名。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// A resource reference such as an image or audio file.
    /// 图片或音频文件等资源引用。
    Resource {
        /// Referenced resource.
        /// 被引用的资源。
        resource: ResourceReference,
    },
    /// A model-emitted tool call.
    /// 模型产生的工具调用。
    ToolCall {
        /// Tool call payload.
        /// 工具调用载荷。
        call: ToolCall,
    },
    /// A client-emitted tool result.
    /// 客户端产生的工具结果。
    ToolResult {
        /// Tool result payload.
        /// 工具结果载荷。
        result: ToolResult,
    },
    /// A refusal emitted by the model.
    /// 模型产生的拒绝内容。
    Refusal {
        /// Refusal explanation.
        /// 拒绝说明。
        text: String,
    },
    /// One provider-returned citation.
    /// 一个供应商返回的引用。
    Citation {
        /// User-visible citation title.
        /// 用户可见引用标题。
        title: String,
        /// User-safe citation URL.
        /// 用户安全引用 URL。
        url: String,
        /// Optional quoted or summarized citation text.
        /// 可选引用或摘要文本。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
}
