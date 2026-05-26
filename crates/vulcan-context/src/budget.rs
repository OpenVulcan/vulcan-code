//! Context budget estimation helpers shared by execution preparation, diagnostics, and compaction triggers.
//! 供执行准备、诊断与压缩触发共享的上下文预算估算辅助模块。

use crate::{
    CONTEXT_BUDGET_COMPACTION_RATIO, CONTEXT_BUDGET_OVERFLOW_RATIO, CONTEXT_BUDGET_WARNING_RATIO,
    ContextBudgetSeverity, ContextBudgetSnapshot, ContextCompactionPolicy,
};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot, ProviderRequestToolSchema as CoreProviderRequestToolSchema,
    ProviderUsageSnapshot,
};

/// Canonical provider API family used by context budget computation.
/// 上下文预算计算使用的规范 Provider 接口族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextProviderApiFamily {
    /// OpenAI Responses API family.
    /// OpenAI Responses 接口族。
    OpenAiResponses,
    /// OpenAI Chat Completions compatible API family.
    /// OpenAI Chat Completions 兼容接口族。
    OpenAiChatCompatible,
    /// Anthropic Messages API family.
    /// Anthropic Messages 接口族。
    AnthropicMessages,
    /// Google Gemini GenerateContent API family.
    /// Google Gemini GenerateContent 接口族。
    GoogleGeminiGenerateContent,
    /// Cohere Chat API family.
    /// Cohere Chat 接口族。
    CohereChat,
    /// Reserved AWS Bedrock Converse API family.
    /// 预留的 AWS Bedrock Converse 接口族。
    BedrockConverse,
}

impl ContextProviderApiFamily {
    /// Returns one stable API-family key used by fingerprints and diagnostics.
    /// 返回供指纹与诊断使用的稳定接口族键值。
    pub const fn key(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai-responses",
            Self::OpenAiChatCompatible => "openai-chat-compatible",
            Self::AnthropicMessages => "anthropic-messages",
            Self::GoogleGeminiGenerateContent => "google-gemini-generate-content",
            Self::CohereChat => "cohere-chat",
            Self::BedrockConverse => "bedrock-converse",
        }
    }
}

/// Coarse reasoning-effort bucket used by context-budget reserves.
/// 上下文预算预留使用的粗粒度推理强度分桶。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextReasoningEffort {
    /// No explicit reasoning reserve.
    /// 不保留显式推理预算。
    None,
    /// Minimal reasoning reserve.
    /// 最小推理预留。
    Minimal,
    /// Low reasoning reserve.
    /// 低推理预留。
    Low,
    /// Medium reasoning reserve.
    /// 中推理预留。
    Medium,
    /// High reasoning reserve.
    /// 高推理预留。
    High,
    /// Extra-high reasoning reserve.
    /// 超高推理预留。
    XHigh,
}

/// One semantic AI-tool capability reduced to the shape needed by budget fingerprints.
/// 被收敛为预算指纹所需形态的单条语义 AI 工具能力。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBudgetAiToolCapability {
    /// Stable semantic tool key.
    /// 稳定语义工具键值。
    pub kind_key: String,
    /// Stable policy label.
    /// 稳定策略标签。
    pub policy_key: String,
    /// Optional native tool type.
    /// 可选原生工具类型。
    pub native_tool_type: Option<String>,
}

/// Generic capability snapshot consumed by context-budget computation.
/// 供上下文预算计算消费的通用能力快照。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextBudgetCapabilities {
    /// Optional provider/model context-window limit.
    /// Provider/Model 可选上下文窗口上限。
    pub context_limit: Option<usize>,
    /// Optional provider/model input-token limit.
    /// Provider/Model 可选输入 Token 上限。
    pub input_limit: Option<usize>,
    /// Optional provider/model output-token limit.
    /// Provider/Model 可选输出 Token 上限。
    pub output_limit: Option<usize>,
    /// Whether the selected model declares tool-calling support.
    /// 当前选中模型是否声明支持工具调用能力。
    pub supports_tool_call: bool,
    /// Whether the selected model accepts explicit temperature overrides.
    /// 当前选中模型是否接受显式温度覆盖。
    pub supports_temperature: bool,
    /// Whether the selected model declares attachment support.
    /// 当前选中模型是否声明支持附件能力。
    pub supports_attachment: bool,
    /// Normalized input modalities declared by the selected model.
    /// 当前选中模型声明的规范化输入模态集合。
    pub input_modalities: Vec<String>,
    /// Normalized output modalities declared by the selected model.
    /// 当前选中模型声明的规范化输出模态集合。
    pub output_modalities: Vec<String>,
    /// Resolved AI-specialized tool capabilities available to the selected provider/model pair.
    /// 当前供应商/模型组合可用的已解析 AI 专用工具能力集合。
    pub ai_tools: Vec<ContextBudgetAiToolCapability>,
}

impl ContextBudgetCapabilities {
    /// Returns the effective input budget limit used by context-budget estimation and compaction checks.
    /// 返回供上下文预算估算与压缩检查使用的生效输入预算上限。
    pub fn effective_input_limit(&self) -> Option<usize> {
        self.input_limit.or(self.context_limit)
    }

    /// Returns the effective output reserve limit used by context-budget estimation.
    /// 返回供上下文预算估算使用的生效输出预留上限。
    pub fn effective_output_limit(&self) -> Option<usize> {
        self.output_limit
    }
}

/// Returns the observed provider input-token count when the provider reported enough usage data.
/// 当 Provider 返回了足够的用量数据时，返回实测输入 Token 数。
pub fn observed_input_tokens_from_usage(usage: Option<&ProviderUsageSnapshot>) -> Option<usize> {
    let snapshot = usage?;
    if let Some(input_tokens) = snapshot.input_tokens {
        return Some(input_tokens);
    }
    snapshot.total_tokens.map(|total_tokens| {
        let known_non_input_tokens = snapshot
            .output_tokens
            .unwrap_or(0usize)
            .saturating_add(snapshot.reasoning_tokens.unwrap_or(0usize));
        total_tokens
            .saturating_sub(known_non_input_tokens)
            .max(1usize)
    })
}

/// Returns one calibrated input-token estimate, preferring provider-observed usage over host heuristics.
/// 返回校准后的输入 Token 估算值，优先采用 Provider 实测用量而不是宿主启发式估算。
pub fn calibrated_input_token_estimate(
    estimated_input_tokens: usize,
    usage: Option<&ProviderUsageSnapshot>,
) -> usize {
    observed_input_tokens_from_usage(usage).unwrap_or(estimated_input_tokens)
}

/// Builds one exact request-facing budget snapshot from the final provider request snapshot.
/// 根据最终 Provider 请求快照构建一份精确面向请求的预算快照。
pub fn build_request_context_budget_snapshot(
    provider_id: &str,
    model_id: &str,
    api_family: ContextProviderApiFamily,
    capabilities: &ContextBudgetCapabilities,
    reasoning: ContextReasoningEffort,
    request_snapshot: &CoreProviderRequestSnapshot,
    observed_usage: Option<&ProviderUsageSnapshot>,
) -> ContextBudgetSnapshot {
    let reserved_reasoning_tokens = reasoning_token_reserve(reasoning);
    let reserved_output_tokens = resolve_output_token_reserve(capabilities);
    let reserved_compaction_tokens = 0usize;
    let estimated_system_tokens = estimate_text_tokens(&request_snapshot.system_prompt);
    let estimated_history_tokens = request_snapshot
        .history
        .iter()
        .map(estimate_core_message_tokens)
        .sum();
    let estimated_prompt_tokens = estimate_text_tokens(&request_snapshot.prompt);
    let estimated_tool_tokens = request_snapshot
        .tool_schemas
        .iter()
        .map(estimate_tool_schema_tokens)
        .sum();
    let estimated_input_tokens = estimated_system_tokens
        + estimated_history_tokens
        + estimated_prompt_tokens
        + estimated_tool_tokens;
    let calibrated_input_tokens =
        calibrated_input_token_estimate(estimated_input_tokens, observed_usage);
    let estimated_total_tokens = calibrated_input_tokens
        + reserved_reasoning_tokens
        + reserved_output_tokens
        + reserved_compaction_tokens;
    let effective_input_limit = capabilities.effective_input_limit();
    let usage_ratio = effective_input_limit
        .map(|limit| calibrated_input_tokens as f32 / limit.max(1) as f32)
        .unwrap_or(0.0);

    ContextBudgetSnapshot {
        fingerprint: build_context_budget_fingerprint(
            provider_id,
            model_id,
            api_family,
            capabilities,
            reasoning,
            request_snapshot.tool_schemas.len(),
            None,
        ),
        context_limit: capabilities.context_limit,
        effective_input_limit,
        reserved_output_tokens,
        reserved_reasoning_tokens,
        reserved_compaction_tokens,
        compaction_auto_enabled: false,
        baseline_matches_active_runtime: true,
        baseline_budget_fingerprint_matches: true,
        baseline_assembly_fingerprint_matches: true,
        baseline_mismatch_reason: None,
        estimated_system_tokens,
        estimated_history_tokens,
        estimated_prompt_tokens,
        estimated_tool_tokens,
        estimated_input_tokens: calibrated_input_tokens,
        estimated_total_tokens,
        usage_ratio,
        observed_usage: observed_usage.cloned(),
        severity: resolve_budget_severity(usage_ratio),
    }
}

/// Builds one compaction-facing budget snapshot from already projected provider-visible core messages.
/// 根据已经投影完成的 Provider 可见 core 消息构建一份面向压缩判断的预算快照。
pub fn build_projected_session_context_budget_snapshot(
    provider_id: &str,
    model_id: &str,
    api_family: ContextProviderApiFamily,
    capabilities: &ContextBudgetCapabilities,
    reasoning_effort: ContextReasoningEffort,
    compaction_policy: ContextCompactionPolicy,
    projected_messages: &[CoreConversationMessage],
    visible_tool_count: usize,
    observed_usage: Option<&ProviderUsageSnapshot>,
) -> ContextBudgetSnapshot {
    let reserved_reasoning_tokens = reasoning_token_reserve(reasoning_effort);
    let reserved_output_tokens = resolve_output_token_reserve(capabilities);
    let reserved_compaction_tokens = compaction_policy.reserved;
    let estimated_history_tokens = projected_messages
        .iter()
        .map(estimate_core_message_tokens)
        .sum();
    let estimated_tool_tokens = visible_tool_count * 96usize;
    let estimated_system_tokens = 384usize;
    let estimated_prompt_tokens = 0usize;
    let estimated_input_tokens =
        estimated_system_tokens + estimated_history_tokens + estimated_tool_tokens;
    let calibrated_input_tokens =
        calibrated_input_token_estimate(estimated_input_tokens, observed_usage);
    let estimated_total_tokens = calibrated_input_tokens
        + reserved_reasoning_tokens
        + reserved_output_tokens
        + reserved_compaction_tokens;
    let effective_input_limit = capabilities
        .effective_input_limit()
        .map(|limit| limit.saturating_sub(reserved_compaction_tokens).max(1usize));
    let usage_ratio = effective_input_limit
        .map(|limit| calibrated_input_tokens as f32 / limit.max(1) as f32)
        .unwrap_or(0.0);

    ContextBudgetSnapshot {
        fingerprint: build_context_budget_fingerprint(
            provider_id,
            model_id,
            api_family,
            capabilities,
            reasoning_effort,
            visible_tool_count,
            Some(compaction_policy),
        ),
        context_limit: capabilities.context_limit,
        effective_input_limit,
        reserved_output_tokens,
        reserved_reasoning_tokens,
        reserved_compaction_tokens,
        compaction_auto_enabled: compaction_policy.auto,
        baseline_matches_active_runtime: true,
        baseline_budget_fingerprint_matches: true,
        baseline_assembly_fingerprint_matches: true,
        baseline_mismatch_reason: None,
        estimated_system_tokens,
        estimated_history_tokens,
        estimated_prompt_tokens,
        estimated_tool_tokens,
        estimated_input_tokens: calibrated_input_tokens,
        estimated_total_tokens,
        usage_ratio,
        observed_usage: observed_usage.cloned(),
        severity: resolve_budget_severity(usage_ratio),
    }
}

/// Builds one stable budget fingerprint used by model-switch reconciliation and compaction-anchor invalidation.
/// 构建供模型切换对齐与压缩锚点失效判断使用的稳定预算指纹。
pub fn build_context_budget_fingerprint(
    provider_id: &str,
    model_id: &str,
    api_family: ContextProviderApiFamily,
    capabilities: &ContextBudgetCapabilities,
    reasoning_effort: ContextReasoningEffort,
    visible_tool_count: usize,
    compaction_policy: Option<ContextCompactionPolicy>,
) -> String {
    let compaction_policy_label = compaction_policy
        .map(|policy| {
            format!(
                "auto={}|prune={}|tail={}|preserve={}|reserved={}",
                policy.auto,
                policy.prune,
                policy.tail_turns,
                policy.preserve_recent_tokens,
                policy.reserved
            )
        })
        .unwrap_or_else(|| String::from("none"));
    let ai_tools_label = capabilities
        .ai_tools
        .iter()
        .map(|tool| {
            format!(
                "{}:{}:{}",
                tool.kind_key,
                tool.policy_key,
                tool.native_tool_type.as_deref().unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}|{}|{}|ctx={:?}|in={:?}|out={:?}|tool={}|temp={}|attach={}|reason={:?}|it={}|ot={}|tools={}|ai={}|compact={}",
        provider_id,
        model_id,
        api_family.key(),
        capabilities.context_limit,
        capabilities.input_limit,
        capabilities.output_limit,
        capabilities.supports_tool_call,
        capabilities.supports_temperature,
        capabilities.supports_attachment,
        reasoning_effort,
        capabilities.input_modalities.join(","),
        capabilities.output_modalities.join(","),
        visible_tool_count,
        ai_tools_label,
        compaction_policy_label,
    )
}

/// Returns the coarse budget severity bucket that corresponds to one usage ratio.
/// 返回某个占用比例对应的粗粒度预算严重级别分桶。
fn resolve_budget_severity(usage_ratio: f32) -> ContextBudgetSeverity {
    if usage_ratio >= CONTEXT_BUDGET_OVERFLOW_RATIO {
        ContextBudgetSeverity::Overflow
    } else if usage_ratio >= CONTEXT_BUDGET_COMPACTION_RATIO {
        ContextBudgetSeverity::CompactSoon
    } else if usage_ratio >= CONTEXT_BUDGET_WARNING_RATIO {
        ContextBudgetSeverity::Warning
    } else {
        ContextBudgetSeverity::Healthy
    }
}

/// Returns the reasoning-token reserve implied by one coarse reasoning effort bucket.
/// 返回某个粗粒度推理强度分桶所隐含的推理 Token 预留值。
fn reasoning_token_reserve(reasoning_effort: ContextReasoningEffort) -> usize {
    match reasoning_effort {
        ContextReasoningEffort::None => 0usize,
        ContextReasoningEffort::Minimal => 1024usize,
        ContextReasoningEffort::Low => 2048usize,
        ContextReasoningEffort::Medium => 4096usize,
        ContextReasoningEffort::High => 8192usize,
        ContextReasoningEffort::XHigh => 16384usize,
    }
}

/// Returns the provider-output reserve used by host-side context budgeting.
/// 返回宿主侧上下文预算使用的 Provider 输出预留值。
fn resolve_output_token_reserve(capabilities: &ContextBudgetCapabilities) -> usize {
    capabilities.effective_output_limit().unwrap_or(384000usize)
}

/// Estimates one provider-visible message without consulting provider-specific tokenizers.
/// 在不依赖 Provider 专用 tokenizer 的情况下估算一条 Provider 可见消息。
fn estimate_core_message_tokens(message: &CoreConversationMessage) -> usize {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            estimate_text_tokens(text)
                + attachments
                    .iter()
                    .map(|attachment| {
                        256usize
                            + estimate_text_tokens(&attachment.file_name)
                            + estimate_text_tokens(&attachment.mime_type)
                    })
                    .sum::<usize>()
                + 12usize
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            tool_calls,
            ..
        } => {
            estimate_text_tokens(text)
                + estimate_text_tokens(reasoning)
                + tool_calls
                    .iter()
                    .map(|tool_call| {
                        estimate_text_tokens(&tool_call.tool_name)
                            + serde_json::to_string(&tool_call.arguments)
                                .map(|arguments| estimate_text_tokens(&arguments))
                                .unwrap_or_default()
                            + 24usize
                    })
                    .sum::<usize>()
                + 16usize
        }
        CoreConversationPayload::ToolResult { text, .. } => estimate_text_tokens(text) + 12usize,
    }
}

/// Estimates one provider-visible tool schema.
/// 估算一条 Provider 可见工具 schema。
fn estimate_tool_schema_tokens(definition: &CoreProviderRequestToolSchema) -> usize {
    estimate_text_tokens(&definition.id)
        + estimate_text_tokens(&definition.name)
        + estimate_text_tokens(&definition.description)
        + serde_json::to_string(&definition.parameters_schema)
            .map(|parameters| estimate_text_tokens(&parameters))
            .unwrap_or_default()
        + 32usize
}

/// Estimates text tokens with the local lightweight heuristic.
/// 使用本地轻量启发式估算文本 Token。
fn estimate_text_tokens(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    ((trimmed.chars().count() + 3usize) / 4usize).max(1usize)
}
