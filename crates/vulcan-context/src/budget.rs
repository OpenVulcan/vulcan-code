//! Context budget estimation helpers shared by execution preparation, diagnostics, and compaction triggers.
//! 供执行准备、诊断与压缩触发共享的上下文预算估算辅助模块。

use crate::{
    CONTEXT_BUDGET_COMPACTION_RATIO, CONTEXT_BUDGET_OVERFLOW_RATIO, CONTEXT_BUDGET_WARNING_RATIO,
    ContextBudgetSeverity, ContextBudgetSnapshot, ContextCompactionPolicy,
};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema, ProviderUsageSnapshot,
};

/// Host-provided model-aware text token estimator used by context budget calculation.
/// 宿主提供的模型感知文本 Token 估算器，用于上下文预算计算。
pub trait ContextTokenEstimator: Send + Sync {
    /// Estimates one text payload for the active model and returns `None` when the estimator cannot serve the request.
    /// 为当前模型估算单段文本；当估算器无法处理该请求时返回 `None`。
    fn estimate_text_tokens(&self, model_id: &str, text: &str) -> Option<usize>;
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
        // cache_tokens captures Provider-reported cache reads and cache writes that may sit outside input_tokens.
        // cache_tokens 汇总 Provider 报告且可能未包含在 input_tokens 内的缓存读取与缓存写入。
        let cache_tokens = snapshot
            .cached_input_tokens
            .unwrap_or_default()
            .saturating_add(snapshot.cache_creation_input_tokens.unwrap_or_default());
        // expanded_input_tokens represents the Anthropic-style split-input interpretation.
        // expanded_input_tokens 表示 Anthropic 风格的拆分输入口径。
        let expanded_input_tokens = input_tokens.saturating_add(cache_tokens);
        // provider_total_input derives the full input side without assuming whether cache tokens are a subset.
        // provider_total_input 从总量推导完整输入侧，且不预设缓存 Token 是否属于 input_tokens 子集。
        let provider_total_input = snapshot
            .total_tokens
            .zip(snapshot.output_tokens)
            .map(|(total_tokens, output_tokens)| total_tokens.saturating_sub(output_tokens));
        if cache_tokens > 0 && provider_total_input == Some(expanded_input_tokens) {
            return Some(expanded_input_tokens);
        }
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

/// Input payload required to build one request-facing budget snapshot.
/// 构建单个面向请求的预算快照所需的输入载荷。
#[derive(Clone, Copy)]
pub struct RequestContextBudgetSnapshotInput<'a> {
    /// Stable provider identifier used by diagnostics and fingerprints.
    /// 诊断与指纹使用的稳定 Provider 标识。
    pub provider_id: &'a str,
    /// Stable model identifier used by token estimation.
    /// Token 估算使用的稳定模型标识。
    pub model_id: &'a str,
    /// Canonical provider API family.
    /// 规范 Provider 接口族。
    pub protocol_profile_id: &'a str,
    /// Resolved provider/model capability snapshot.
    /// 已解析的 Provider/Model 能力快照。
    pub capabilities: &'a ContextBudgetCapabilities,
    /// Resolved reasoning-effort reserve bucket.
    /// 已解析的推理强度预留分桶。
    pub reasoning: ContextReasoningEffort,
    /// Final provider request snapshot used for exact estimation.
    /// 用于精确估算的最终 Provider 请求快照。
    pub request_snapshot: &'a CoreProviderRequestSnapshot,
    /// Optional provider-observed usage used to calibrate estimates.
    /// 用于校准估算值的可选 Provider 实测用量。
    pub observed_usage: Option<&'a ProviderUsageSnapshot>,
}

/// Optional controls for request-facing budget snapshot construction.
/// 面向请求的预算快照构造可选控制项。
#[derive(Clone, Copy, Default)]
pub struct RequestContextBudgetSnapshotOptions<'a> {
    /// Optional compaction policy used to reserve summary budget.
    /// 用于预留摘要预算的可选压缩策略。
    pub compaction_policy: Option<ContextCompactionPolicy>,
    /// Optional host token estimator.
    /// 可选宿主 Token 估算器。
    pub estimator: Option<&'a dyn ContextTokenEstimator>,
}

/// Builds one exact request-facing budget snapshot from the final provider request snapshot.
/// 根据最终 Provider 请求快照构建一份精确面向请求的预算快照。
pub fn build_request_context_budget_snapshot(
    input: RequestContextBudgetSnapshotInput<'_>,
) -> ContextBudgetSnapshot {
    build_request_context_budget_snapshot_with_options(input, Default::default())
}

/// Builds one request-facing budget snapshot with an optional compaction policy and host token estimator.
/// 使用可选压缩策略与宿主 Token 估算器构建一份面向真实请求的预算快照。
pub fn build_request_context_budget_snapshot_with_options(
    input: RequestContextBudgetSnapshotInput<'_>,
    options: RequestContextBudgetSnapshotOptions<'_>,
) -> ContextBudgetSnapshot {
    let RequestContextBudgetSnapshotInput {
        provider_id,
        model_id,
        protocol_profile_id,
        capabilities,
        reasoning,
        request_snapshot,
        observed_usage,
    } = input;
    let reserved_reasoning_tokens = reasoning_token_reserve(reasoning);
    let reserved_output_tokens = resolve_output_token_reserve(capabilities);
    let reserved_compaction_tokens = options
        .compaction_policy
        .map(|policy| policy.reserved)
        .unwrap_or(0usize);
    let compaction_auto_enabled = options
        .compaction_policy
        .map(|policy| policy.auto)
        .unwrap_or(false);
    let estimated_system_tokens = estimate_text_tokens_for_model(
        model_id,
        options.estimator,
        &request_snapshot.system_prompt,
    );
    let estimated_history_tokens = request_snapshot
        .history
        .iter()
        .map(|message| estimate_core_message_tokens(model_id, options.estimator, message))
        .sum();
    let estimated_prompt_tokens =
        estimate_text_tokens_for_model(model_id, options.estimator, &request_snapshot.prompt);
    let estimated_tool_tokens = request_snapshot
        .tool_schemas
        .iter()
        .map(|schema| estimate_tool_schema_tokens(model_id, options.estimator, schema))
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
            protocol_profile_id,
            capabilities,
            reasoning,
            request_snapshot.tool_schemas.len(),
            options.compaction_policy,
        ),
        context_limit: capabilities.context_limit,
        effective_input_limit,
        reserved_output_tokens,
        reserved_reasoning_tokens,
        reserved_compaction_tokens,
        compaction_auto_enabled,
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
        severity: context_budget_severity_for_ratio(usage_ratio),
    }
}

/// Applies compaction-anchor baseline validation to one context-budget snapshot.
/// 将压缩锚点基线校验结果应用到单个上下文预算快照。
pub fn apply_context_budget_anchor_baseline(
    mut snapshot: ContextBudgetSnapshot,
    anchor_budget_fingerprint: Option<&str>,
    anchor_assembly_fingerprint: Option<&str>,
    live_assembly_fingerprint: Option<&str>,
) -> ContextBudgetSnapshot {
    let mut mismatch_reasons = Vec::new();

    if let Some(previous_budget_fingerprint) = anchor_budget_fingerprint {
        snapshot.baseline_budget_fingerprint_matches =
            previous_budget_fingerprint == snapshot.fingerprint;
        if !snapshot.baseline_budget_fingerprint_matches {
            mismatch_reasons.push(String::from("预算指纹变化"));
        }
    }

    if let (Some(previous_assembly_fingerprint), Some(current_assembly_fingerprint)) =
        (anchor_assembly_fingerprint, live_assembly_fingerprint)
    {
        snapshot.baseline_assembly_fingerprint_matches =
            previous_assembly_fingerprint == current_assembly_fingerprint;
        if !snapshot.baseline_assembly_fingerprint_matches {
            mismatch_reasons.push(String::from("上下文层指纹变化"));
        }
    }

    if mismatch_reasons.is_empty() {
        snapshot.baseline_matches_active_runtime = true;
        snapshot.baseline_mismatch_reason = None;
    } else {
        snapshot.baseline_matches_active_runtime = false;
        snapshot.baseline_mismatch_reason = Some(mismatch_reasons.join(" · "));
    }

    snapshot
}

/// Builds one stable budget fingerprint used by model-switch reconciliation and compaction-anchor invalidation.
/// 构建供模型切换对齐与压缩锚点失效判断使用的稳定预算指纹。
pub fn build_context_budget_fingerprint(
    provider_id: &str,
    model_id: &str,
    protocol_profile_id: &str,
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
        protocol_profile_id,
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
///
/// # Parameters
/// 参数
/// `usage_ratio` is the estimated or observed input-token ratio against the effective window.
/// `usage_ratio` 是估算或实测输入 token 相对有效窗口的占用比例。
///
/// # Returns
/// 返回值
/// The shared severity bucket used by both live and historical context-budget snapshots.
/// 返回实时与历史上下文预算快照共同使用的共享严重级别分桶。
pub fn context_budget_severity_for_ratio(usage_ratio: f32) -> ContextBudgetSeverity {
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

/// Estimates one provider-visible message with an optional model-aware estimator.
/// 使用可选模型感知估算器估算一条 Provider 可见消息。
fn estimate_core_message_tokens(
    model_id: &str,
    estimator: Option<&dyn ContextTokenEstimator>,
    message: &CoreConversationMessage,
) -> usize {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            estimate_text_tokens_for_model(model_id, estimator, text)
                + attachments
                    .iter()
                    .map(|attachment| {
                        256usize
                            + estimate_text_tokens_for_model(
                                model_id,
                                estimator,
                                &attachment.file_name,
                            )
                            + estimate_text_tokens_for_model(
                                model_id,
                                estimator,
                                &attachment.mime_type,
                            )
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
            estimate_text_tokens_for_model(model_id, estimator, text)
                + estimate_text_tokens_for_model(model_id, estimator, reasoning)
                + tool_calls
                    .iter()
                    .map(|tool_call| {
                        estimate_text_tokens_for_model(model_id, estimator, &tool_call.tool_name)
                            + serde_json::to_string(&tool_call.arguments)
                                .map(|arguments| {
                                    estimate_text_tokens_for_model(model_id, estimator, &arguments)
                                })
                                .unwrap_or_default()
                            + 24usize
                    })
                    .sum::<usize>()
                + 16usize
        }
        CoreConversationPayload::ToolResult { text, .. } => {
            estimate_text_tokens_for_model(model_id, estimator, text) + 12usize
        }
        CoreConversationPayload::AgentMessage { .. } => message
            .payload
            .agent_message_model_text()
            .map(|text| estimate_text_tokens_for_model(model_id, estimator, &text) + 16usize)
            .unwrap_or(16usize),
    }
}

/// Estimates one provider-visible tool schema with an optional model-aware estimator.
/// 使用可选模型感知估算器估算一条 Provider 可见工具 schema。
fn estimate_tool_schema_tokens(
    model_id: &str,
    estimator: Option<&dyn ContextTokenEstimator>,
    definition: &CoreProviderRequestToolSchema,
) -> usize {
    estimate_text_tokens_for_model(model_id, estimator, &definition.id)
        + estimate_text_tokens_for_model(model_id, estimator, &definition.name)
        + estimate_text_tokens_for_model(model_id, estimator, &definition.description)
        + serde_json::to_string(&definition.parameters_schema)
            .map(|parameters| estimate_text_tokens_for_model(model_id, estimator, &parameters))
            .unwrap_or_default()
        + 32usize
}

/// Estimates text tokens with a host estimator and falls back to the local lightweight heuristic.
/// 使用宿主估算器估算文本 Token，并在不可用时回退到本地轻量启发式。
fn estimate_text_tokens_for_model(
    model_id: &str,
    estimator: Option<&dyn ContextTokenEstimator>,
    text: &str,
) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    if let Some(estimated_tokens) =
        estimator.and_then(|estimator| estimator.estimate_text_tokens(model_id, trimmed))
    {
        return estimated_tokens.max(1usize);
    }
    estimate_mixed_language_text_tokens(trimmed).max(1usize)
}

/// Estimate mixed-language text with CJK characters counted close to one token each.
/// 按中日韩字符约一字一 Token、ASCII 约四字符一 Token 的方式估算混合文本。
pub fn estimate_mixed_language_text_tokens(text: &str) -> usize {
    let mut ascii_run = 0usize;
    let mut tokens = 0usize;
    for character in text.chars() {
        if character.is_ascii() {
            ascii_run += 1;
            continue;
        }
        if ascii_run > 0 {
            tokens += ascii_run.div_ceil(4usize);
            ascii_run = 0;
        }
        tokens += if is_cjk_character(character) {
            1usize
        } else {
            2usize
        };
    }
    if ascii_run > 0 {
        tokens += ascii_run.div_ceil(4usize);
    }
    tokens
}

/// Return whether one character belongs to the common CJK text ranges.
/// 返回单个字符是否属于常见中日韩文本范围。
pub fn is_cjk_character(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x3040..=0x30FF
            | 0xAC00..=0xD7AF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0x2CEB0..=0x2EBEF
            | 0x30000..=0x3134F
    )
}

#[cfg(test)]
mod tests {
    //! Context-budget estimation tests.
    //! 上下文预算估算测试。

    use super::*;

    /// Verifies Chinese text is estimated close to one token per visible character.
    /// 验证中文文本按接近每个可见字符一个 Token 估算。
    #[test]
    fn estimate_text_tokens_counts_chinese_characters_one_to_one() {
        assert_eq!(
            estimate_text_tokens_for_model("test-model", None, "你好世界"),
            4
        );
    }

    /// Verifies ASCII text still uses the compact four-character heuristic.
    /// 验证 ASCII 文本仍使用紧凑的四字符启发式估算。
    #[test]
    fn estimate_text_tokens_keeps_ascii_four_to_one() {
        assert_eq!(
            estimate_text_tokens_for_model("test-model", None, "abcdefgh"),
            2
        );
    }

    /// Verifies provider-observed usage overrides local mixed-language estimates.
    /// 验证 Provider 实测用量会覆盖本地混合语言估算。
    #[test]
    fn calibrated_input_token_estimate_prefers_observed_usage() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(128),
            output_tokens: Some(8),
            reasoning_tokens: Some(4),
            total_tokens: Some(140),
            cached_input_tokens: None,
            cache_creation_input_tokens: None,
        };

        assert_eq!(calibrated_input_token_estimate(10, Some(&usage)), 128);
    }

    /// Verifies Anthropic cache reads outside input_tokens contribute to the complete context window.
    /// 验证 Anthropic 在 input_tokens 之外报告的缓存读取会计入完整上下文窗口。
    #[test]
    fn observed_input_tokens_include_anthropic_split_cache_usage() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(399),
            output_tokens: Some(380),
            reasoning_tokens: None,
            total_tokens: Some(112_395),
            cached_input_tokens: Some(111_616),
            cache_creation_input_tokens: Some(0),
        };

        assert_eq!(
            observed_input_tokens_from_usage(Some(&usage)),
            Some(112_015)
        );
        assert_eq!(
            calibrated_input_token_estimate(88_824, Some(&usage)),
            112_015
        );
    }

    /// Verifies OpenAI-style cached input remains a subset and is not counted twice.
    /// 验证 OpenAI 风格的缓存输入保持为输入子集且不会被重复计算。
    #[test]
    fn observed_input_tokens_do_not_duplicate_cached_input_subset() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(112_015),
            output_tokens: Some(380),
            reasoning_tokens: None,
            total_tokens: Some(112_395),
            cached_input_tokens: Some(111_616),
            cache_creation_input_tokens: None,
        };

        assert_eq!(
            observed_input_tokens_from_usage(Some(&usage)),
            Some(112_015)
        );
    }

    /// Verifies ratio boundaries map to the shared context-budget severity buckets.
    /// 验证占用比例边界会映射到共享上下文预算严重级别分桶。
    #[test]
    fn context_budget_severity_for_ratio_respects_thresholds() {
        assert_eq!(
            context_budget_severity_for_ratio(CONTEXT_BUDGET_WARNING_RATIO - 0.01),
            ContextBudgetSeverity::Healthy
        );
        assert_eq!(
            context_budget_severity_for_ratio(CONTEXT_BUDGET_WARNING_RATIO),
            ContextBudgetSeverity::Warning
        );
        assert_eq!(
            context_budget_severity_for_ratio(CONTEXT_BUDGET_COMPACTION_RATIO),
            ContextBudgetSeverity::CompactSoon
        );
        assert_eq!(
            context_budget_severity_for_ratio(CONTEXT_BUDGET_OVERFLOW_RATIO),
            ContextBudgetSeverity::Overflow
        );
    }

    /// Verifies anchor validation reports both budget and assembly fingerprint drift.
    /// 验证锚点校验会同时报告预算与分层上下文指纹漂移。
    #[test]
    fn apply_context_budget_anchor_baseline_reports_fingerprint_drift() {
        let snapshot = ContextBudgetSnapshot {
            fingerprint: String::from("budget-new"),
            ..ContextBudgetSnapshot::default()
        };

        let validated = apply_context_budget_anchor_baseline(
            snapshot,
            Some("budget-old"),
            Some("assembly-old"),
            Some("assembly-new"),
        );

        assert!(!validated.baseline_matches_active_runtime);
        assert!(!validated.baseline_budget_fingerprint_matches);
        assert!(!validated.baseline_assembly_fingerprint_matches);
        assert_eq!(
            validated.baseline_mismatch_reason.as_deref(),
            Some("预算指纹变化 · 上下文层指纹变化")
        );
    }
}
