//! Provider-neutral AI execution requests.
//! 供应商无关的 AI 执行请求。

use crate::{
    AiMessage, AiOperation, ModelParameterValue, NativeToolKind, PromptCachePolicy,
    ReasoningEffort, ResourceReference, ToolDefinition,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Carries one provider-neutral AI execution request.
/// 承载一个供应商无关的 AI 执行请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiExecutionRequest {
    /// Requested operation.
    /// 请求的操作。
    pub operation: AiOperation,
    /// Exact model identifier selected from the active connection catalog.
    /// 从当前连接目录选择的精确模型标识。
    pub model_id: String,
    /// Stable provider-neutral cache scope for related physical requests.
    /// 相关物理请求使用的稳定供应商无关缓存作用域。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_scope_id: Option<String>,
    /// Ordered conversation messages.
    /// 有序对话消息。
    #[serde(default)]
    pub messages: Vec<AiMessage>,
    /// Model-visible tool definitions.
    /// 对模型可见的工具定义。
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    /// Provider-native tools selected from exact model capabilities.
    /// 根据精确模型能力选择的供应商原生工具。
    #[serde(default)]
    pub native_tools: Vec<NativeToolKind>,
    /// Prompt-cache intent.
    /// 提示词缓存意图。
    #[serde(default)]
    pub prompt_cache: PromptCachePolicy,
    /// Optional explicit reasoning effort selected from the active model catalog.
    /// 从当前模型目录选择的可选显式推理强度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Adapter-registered model-specific parameter values.
    /// 由适配器注册的模型独立参数值。
    #[serde(default)]
    pub model_parameters: BTreeMap<String, ModelParameterValue>,
    /// Local or Vulcan-side input resources.
    /// 本地或 Vulcan 侧输入资源。
    #[serde(default)]
    pub resources: Vec<ResourceReference>,
    /// Optional user-narrowed output token limit.
    /// 可选的用户收窄输出 Token 限制。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_token_limit: Option<u64>,
}
