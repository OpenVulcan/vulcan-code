//! Provider-neutral AI execution requests.
//! 供应商无关的 AI 执行请求。

use crate::{
    AiContinuation, AiMessage, AiOperation, ModelParameterValue, NativeToolKind, PromptCachePolicy,
    ReasoningEffort, ResourceReference, ToolDefinition,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Selects how model-visible tools may be invoked for one request.
/// 选择一次请求中模型可见工具的调用方式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ToolChoicePolicy {
    /// Lets the provider decide whether to call a tool.
    /// 允许供应商决定是否调用工具。
    #[default]
    Auto,
    /// Requires at least one tool call.
    /// 要求至少调用一个工具。
    Required,
    /// Prevents tool calls for this request.
    /// 禁止本次请求调用工具。
    Disabled,
    /// Requires one exact registered tool.
    /// 要求调用一个精确注册工具。
    Specific {
        /// Exact public tool name.
        /// 精确公共工具名称。
        tool_name: String,
    },
}

/// Controls parallel model tool calls.
/// 控制模型并行工具调用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParallelToolCallPolicy {
    /// Uses the exact protocol profile default.
    /// 使用精确协议 Profile 默认值。
    #[default]
    ProviderDefault,
    /// Explicitly permits parallel calls.
    /// 显式允许并行调用。
    Allowed,
    /// Explicitly requires serial calls.
    /// 显式要求串行调用。
    Disabled,
}

/// Selects a provider-neutral structured output contract.
/// 选择供应商无关的结构化输出契约。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum StructuredOutputPolicy {
    /// Requests ordinary provider text output.
    /// 请求普通供应商文本输出。
    #[default]
    None,
    /// Requests one JSON object without a declared schema.
    /// 请求一个没有声明 Schema 的 JSON 对象。
    JsonObject,
    /// Requests output matching one exact JSON Schema.
    /// 请求符合一个精确 JSON Schema 的输出。
    JsonSchema {
        /// Stable schema name.
        /// 稳定 Schema 名称。
        name: String,
        /// Optional model-visible schema description.
        /// 可选的模型可见 Schema 说明。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Exact JSON Schema.
        /// 精确 JSON Schema。
        schema: Value,
        /// Whether the provider must enforce strict schema adherence.
        /// 是否要求供应商严格执行 Schema。
        strict: bool,
    },
}

/// Selects the public reasoning output representation.
/// 选择公共推理输出表示方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningOutputPolicy {
    /// Uses the exact protocol profile default.
    /// 使用精确协议 Profile 默认值。
    #[default]
    ProviderDefault,
    /// Retains provider reasoning state without requesting visible reasoning text.
    /// 保留供应商推理状态但不请求可见推理文本。
    Hidden,
    /// Requests an automatic user-visible reasoning summary.
    /// 请求自动生成的用户可见推理摘要。
    SummaryAuto,
    /// Requests the most detailed public reasoning form registered by the profile.
    /// 请求 Profile 已注册的最详细公共推理形式。
    Detailed,
}

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
    /// Proven provider continuation used for incremental protocol requests.
    /// 用于增量协议请求的已证明安全供应商延续信息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<AiContinuation>,
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
    /// Tool invocation policy.
    /// 工具调用策略。
    #[serde(default)]
    pub tool_choice: ToolChoicePolicy,
    /// Parallel tool invocation policy.
    /// 并行工具调用策略。
    #[serde(default)]
    pub parallel_tool_calls: ParallelToolCallPolicy,
    /// Structured output contract.
    /// 结构化输出契约。
    #[serde(default)]
    pub structured_output: StructuredOutputPolicy,
    /// Ordered stop sequences.
    /// 有序停止序列。
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    /// Prompt-cache intent.
    /// 提示词缓存意图。
    #[serde(default)]
    pub prompt_cache: PromptCachePolicy,
    /// Optional explicit reasoning effort selected from the active model catalog.
    /// 从当前模型目录选择的可选显式推理强度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Public reasoning output policy.
    /// 公共推理输出策略。
    #[serde(default)]
    pub reasoning_output: ReasoningOutputPolicy,
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
