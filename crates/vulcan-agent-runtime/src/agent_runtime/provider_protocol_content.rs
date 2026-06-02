//! Host-owned provider content models that decouple shared bridge logic from any specific upstream SDK or transport crate.
//! 将共享桥接逻辑与特定上游 SDK 或传输 crate 解耦的宿主自有 Provider 内容模型。

use vulcan_core::types::ConversationToolCall as CoreConversationToolCall;

/// One normalized reasoning fragment retained by the shared provider bridge for transcript thinking projection and final-response flattening.
/// 由共享 Provider 桥接层保留的一条规范化思考片段，用于转录思考投影与最终响应压平。
#[derive(Debug, Clone)]
pub(crate) enum ProviderReasoningPart {
    /// One plain-text reasoning fragment.
    /// 一条纯文本思考片段。
    Text(String),
    /// One summarized reasoning fragment.
    /// 一条摘要化思考片段。
    Summary(String),
    /// One opaque reasoning fragment that should stay hidden from transcript flattening.
    /// 一条应当在转录压平阶段保持隐藏的不透明思考片段。
    Opaque,
}

/// One normalized assistant content item reused by the shared provider bridge after low-level transport content has already been decoded.
/// 在低层传输内容完成解码后，由共享 Provider 桥接层复用的一条规范化助手内容项。
#[derive(Debug, Clone)]
pub(crate) enum ProviderAssistantContent {
    /// One visible assistant text fragment.
    /// 一条可见的助手文本片段。
    Text(String),
    /// One structured reasoning block retained for local-only thinking projection.
    /// 一条为本地思考投影保留的结构化思考块。
    Reasoning(Vec<ProviderReasoningPart>),
    /// One structured assistant tool call.
    /// 一条结构化助手工具调用。
    ToolCall(CoreConversationToolCall),
}
