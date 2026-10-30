//! Context budget estimation helpers shared by execution preparation, diagnostics, and compaction triggers.
//! 供执行准备、诊断与压缩触发共享的上下文预算估算辅助模块。

use crate::{
    CONTEXT_BUDGET_COMPACTION_RATIO, CONTEXT_BUDGET_OVERFLOW_RATIO, CONTEXT_BUDGET_WARNING_RATIO,
    ContextBudgetAccountingSource, ContextBudgetSeverity, ContextBudgetSnapshot,
    ContextCompactionPolicy, ContextWindowUsageScope,
};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema, ProviderUsageSnapshot,
};

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
    // Protocol extraction owns full input, including cache reads and writes exactly once.
    // 协议提取负责完整输入，缓存读取与写入均只计入一次。
    // A total can include independent thoughts or server-tool work; it cannot recover missing input here.
    // 总量可能包含独立思考或服务端工具工作，不能在此反推缺失输入。
    usage.and_then(|snapshot| snapshot.input_tokens)
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
    /// Optional authoritative input-token accounting already aligned to this exact request.
    /// 已与当前精确请求对齐的可选权威输入 Token 记账值。
    pub accounted_input_tokens: Option<usize>,
    /// Explicit provenance and quality of `accounted_input_tokens`.
    /// `accounted_input_tokens` 的明确来源与质量。
    pub accounting_source: ContextBudgetAccountingSource,
    /// Stable warning code when the accounting source is not exact Provider evidence.
    /// 记账来源不是精确 Provider 证据时的稳定警告码。
    pub accounting_warning_code: Option<&'a str>,
    /// Physical Provider call used as the accounting anchor, when present.
    /// 存在时作为记账锚点的物理 Provider 调用。
    pub provider_anchor_call_id: Option<&'a str>,
    /// Absolute input-token baseline captured at the start of the current compaction window.
    /// 当前压缩窗口开始时捕获的绝对输入 Token 基线。
    pub prefill_input_tokens: Option<usize>,
}

/// Optional controls for request-facing budget snapshot construction.
/// 面向请求的预算快照构造可选控制项。
#[derive(Clone, Copy, Default)]
pub struct RequestContextBudgetSnapshotOptions {
    /// Optional compaction policy used to reserve summary budget.
    /// 用于预留摘要预算的可选压缩策略。
    pub compaction_policy: Option<ContextCompactionPolicy>,
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
    options: RequestContextBudgetSnapshotOptions,
) -> ContextBudgetSnapshot {
    let RequestContextBudgetSnapshotInput {
        provider_id,
        model_id,
        protocol_profile_id,
        capabilities,
        reasoning,
        request_snapshot,
        observed_usage,
        accounted_input_tokens,
        accounting_source,
        accounting_warning_code,
        provider_anchor_call_id,
        prefill_input_tokens,
    } = input;
    let reserved_reasoning_tokens = reasoning_token_reserve(reasoning);
    let reserved_output_tokens = resolve_output_token_reserve(capabilities);
    // Codex keeps the raw model window separate from the output ceiling; legacy reserved values
    // must not silently shrink the active context window.
    // Codex 将模型原始窗口与输出上限分离；旧版 reserved 不得静默缩小活动上下文窗口。
    let reserved_compaction_tokens = 0usize;
    let window_policy = options
        .compaction_policy
        .map(|policy| policy.window)
        .unwrap_or_default();
    // The session node changes only the compaction calculation base, never the hard model window.
    // 会话节点只改变压缩计算基准，绝不改变模型硬窗口。
    let compression_window_limit = options
        .compaction_policy
        .and_then(|policy| policy.compression_window_limit);
    let compaction_auto_enabled = options
        .compaction_policy
        .map(|policy| policy.auto)
        .unwrap_or(false);
    let estimated_system_tokens =
        estimate_text_tokens_from_characters(&request_snapshot.system_prompt);
    let estimated_history_tokens = request_snapshot
        .history
        .iter()
        .fold(0_usize, |total, message| {
            total.saturating_add(estimate_core_message_tokens(message))
        });
    let estimated_prompt_tokens = if request_snapshot.prompt_in_history {
        0
    } else {
        estimate_text_tokens_from_characters(&request_snapshot.prompt)
    };
    let estimated_tool_tokens = request_snapshot
        .tool_schemas
        .iter()
        .fold(0_usize, |total, definition| {
            total.saturating_add(estimate_tool_schema_tokens(definition))
        });
    let estimated_input_tokens = estimated_system_tokens
        .saturating_add(estimated_history_tokens)
        .saturating_add(estimated_prompt_tokens)
        .saturating_add(estimated_tool_tokens);
    let calibrated_input_tokens = accounted_input_tokens.unwrap_or(estimated_input_tokens);
    // The hard-window admission always uses the complete active context.
    // 硬窗口准入始终使用完整活动上下文。
    let active_context_tokens = context_tokens_for_scope(
        calibrated_input_tokens,
        ContextWindowUsageScope::Total,
        None,
    );
    // The configured scope only changes the automatic-compaction line.
    // 配置的计量范围只改变自动压缩线。
    let auto_compact_context_tokens = context_tokens_for_scope(
        calibrated_input_tokens,
        window_policy.auto_compact_scope,
        prefill_input_tokens,
    );
    let estimated_total_tokens = active_context_tokens
        .saturating_add(reserved_reasoning_tokens)
        .saturating_add(reserved_output_tokens)
        .saturating_add(reserved_compaction_tokens);
    let raw_context_limit = capabilities.effective_input_limit();
    let planning_context_limit = compression_window_limit.or(raw_context_limit);
    let effective_context_window = planning_context_limit.map(|limit| {
        let percentage = usize::from(window_policy.effective_context_window_percent);
        limit.saturating_mul(percentage).saturating_div(100).max(1)
    });
    // Provider admission uses the model's real input limit. Availability percentages and the
    // selected session node belong only to planning and automatic compaction.
    // Provider 准入使用模型真实输入上限；可用百分比与所选会话节点仅属于规划和自动压缩。
    let provider_request_hard_limit = raw_context_limit;
    let percentage_limit =
        compression_window_limit
            .or(raw_context_limit)
            .map(|compression_window| {
                compression_window
                    .saturating_mul(usize::from(window_policy.auto_compact_percent))
                    .saturating_div(100)
                    .max(1)
            });
    let explicit_limit = window_policy
        .auto_compact_token_limit
        .and_then(|limit| usize::try_from(limit).ok());
    let compact_line = match (percentage_limit, explicit_limit) {
        (Some(percentage), Some(explicit)) => Some(percentage.min(explicit)),
        (Some(percentage), None) => Some(percentage),
        (None, Some(explicit)) => Some(explicit),
        (None, None) => None,
    };
    let auto_compact_limit = compact_line.map(|line| {
        let buffered_line = line.saturating_add(window_policy.fallback_buffer_tokens);
        if let Some(hard_window) = effective_context_window {
            buffered_line.min(hard_window)
        } else {
            buffered_line
        }
    });
    let usage_ratio = effective_context_window
        .map(|limit| active_context_tokens as f32 / limit.max(1) as f32)
        .unwrap_or(0.0);
    let severity = context_budget_severity_for_limits_with_compaction_context(
        active_context_tokens,
        effective_context_window,
        auto_compact_limit,
        auto_compact_context_tokens,
    );

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
        effective_input_limit: effective_context_window,
        effective_context_window,
        provider_request_hard_limit,
        auto_compact_limit,
        auto_compact_scope: window_policy.auto_compact_scope,
        auto_compact_prefill_input_tokens: prefill_input_tokens,
        fallback_buffer_tokens: window_policy.fallback_buffer_tokens,
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
        active_context_tokens,
        auto_compact_context_tokens,
        estimated_total_tokens,
        usage_ratio,
        observed_usage: observed_usage.cloned(),
        accounting_source,
        accounting_warning_code: accounting_warning_code.map(str::to_string),
        provider_anchor_call_id: provider_anchor_call_id.map(str::to_string),
        severity,
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
            mismatch_reasons.push(String::from("Budget fingerprint changed"));
        }
    }

    if let (Some(previous_assembly_fingerprint), Some(current_assembly_fingerprint)) =
        (anchor_assembly_fingerprint, live_assembly_fingerprint)
    {
        snapshot.baseline_assembly_fingerprint_matches =
            previous_assembly_fingerprint == current_assembly_fingerprint;
        if !snapshot.baseline_assembly_fingerprint_matches {
            mismatch_reasons.push(String::from("Context layer fingerprint changed"));
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
                "auto={}|prune={}|tail={}|preserve={}|tail_context={:?}|tail_source={}|compression_node={}|compression_limit={:?}|reserved={}|window_effective={}|window_compact={}|window_limit={:?}|window_scope={:?}|window_buffer={}",
                policy.auto,
                policy.prune,
                policy.tail_turns,
                policy.preserve_recent_tokens,
                policy.tail_context_limit,
                policy.tail_policy_source.label(),
                policy
                    .compression_window_node_id
                    .map(vulcan_context_node_label)
                    .unwrap_or("none"),
                policy.compression_window_limit,
                policy.reserved,
                policy.window.effective_context_window_percent,
                policy.window.auto_compact_percent,
                policy.window.auto_compact_token_limit,
                policy.window.auto_compact_scope,
                policy.window.fallback_buffer_tokens,
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
        "{}|{}|{}|ctx={:?}|in={:?}|out={:?}|temp={}|attach={}|reason={:?}|it={}|ot={}|tools={}|ai={}|compact={}",
        provider_id,
        model_id,
        protocol_profile_id,
        capabilities.context_limit,
        capabilities.input_limit,
        capabilities.output_limit,
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

/// Returns the stable label for one frozen context-compaction window node.
/// 返回一个冻结上下文压缩窗口节点的稳定标签。
fn vulcan_context_node_label(node_id: crate::ContextCompactionWindowNodeId) -> &'static str {
    node_id.as_str()
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

/// Resolves the active context token count from the selected Codex usage scope.
/// 根据选定的 Codex 用量范围解析活动上下文 Token 数。
fn context_tokens_for_scope(
    accounted_input_tokens: usize,
    scope: ContextWindowUsageScope,
    prefill_input_tokens: Option<usize>,
) -> usize {
    let active_context_tokens = accounted_input_tokens;
    if matches!(scope, ContextWindowUsageScope::BodyAfterPrefix) {
        // Codex treats an unknown prefill baseline as the current active context, so the first
        // request in a new compaction window contributes zero body tokens until usage is observed.
        // Codex 将未知前缀基线视为当前活动上下文，因此新压缩窗口的首个请求在拿到实测用量前正文为零。
        return active_context_tokens
            .saturating_sub(prefill_input_tokens.unwrap_or(active_context_tokens));
    }
    active_context_tokens
}

/// Resolves severity with a separate automatic-compaction usage scope.
/// 使用独立的自动压缩计量范围解析严重级别。
pub fn context_budget_severity_for_limits_with_compaction_context(
    active_context_tokens: usize,
    effective_context_window: Option<usize>,
    auto_compact_limit: Option<usize>,
    auto_compact_context_tokens: usize,
) -> ContextBudgetSeverity {
    if effective_context_window.is_some_and(|hard_window| active_context_tokens >= hard_window) {
        return ContextBudgetSeverity::Overflow;
    }
    if auto_compact_limit.is_some_and(|limit| auto_compact_context_tokens >= limit) {
        return ContextBudgetSeverity::CompactSoon;
    }
    // The Codex hard and automatic lines are the only active thresholds for a resolved policy.
    // 已解析策略只使用 Codex 硬窗口线与自动压缩线作为活动阈值。
    ContextBudgetSeverity::Healthy
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

/// Estimates one Provider-visible message using the stable character-level fallback.
/// 使用稳定字符级回退估算一条 Provider 可见消息。
fn estimate_core_message_tokens(message: &CoreConversationMessage) -> usize {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            estimate_text_tokens_from_characters(text)
                .saturating_add(attachments.iter().fold(0_usize, |total, attachment| {
                    total.saturating_add(
                        256usize
                            .saturating_add(estimate_text_tokens_from_characters(
                                &attachment.file_name,
                            ))
                            .saturating_add(estimate_text_tokens_from_characters(
                                &attachment.mime_type,
                            )),
                    )
                }))
                .saturating_add(12usize)
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            tool_calls,
            ..
        } => estimate_text_tokens_from_characters(text)
            .saturating_add(estimate_text_tokens_from_characters(reasoning))
            .saturating_add(tool_calls.iter().fold(0_usize, |total, tool_call| {
                total.saturating_add(
                    estimate_text_tokens_from_characters(&tool_call.tool_name)
                        .saturating_add(
                            serde_json::to_string(&tool_call.arguments)
                                .map(|arguments| estimate_text_tokens_from_characters(&arguments))
                                .unwrap_or_default(),
                        )
                        .saturating_add(24usize),
                )
            }))
            .saturating_add(16usize),
        CoreConversationPayload::ToolResult { text, .. } => {
            estimate_text_tokens_from_characters(text).saturating_add(12usize)
        }
        CoreConversationPayload::AgentMessage { .. } => {
            match message.payload.agent_message_model_text(&message.id) {
                Ok(text) => estimate_text_tokens_from_characters(&text).saturating_add(16usize),
                Err(error) => estimate_text_tokens_from_characters(&format!(
                    "Agent message rendering failed: {error}"
                )),
            }
        }
    }
}

/// Estimates one Provider-visible tool schema using the stable character-level fallback.
/// 使用稳定字符级回退估算一条 Provider 可见工具 schema。
fn estimate_tool_schema_tokens(definition: &CoreProviderRequestToolSchema) -> usize {
    estimate_text_tokens_from_characters(&definition.id)
        .saturating_add(estimate_text_tokens_from_characters(&definition.name))
        .saturating_add(estimate_text_tokens_from_characters(
            &definition.description,
        ))
        .saturating_add(
            serde_json::to_string(&definition.parameters_schema)
                .map(|parameters| estimate_text_tokens_from_characters(&parameters))
                .unwrap_or_default(),
        )
        .saturating_add(32usize)
}

/// Estimates text tokens only with the non-panicking character-level fallback.
/// 仅使用不会发生 panic 的字符级回退估算文本 Token。
fn estimate_text_tokens_from_characters(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
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
            ascii_run = ascii_run.saturating_add(1);
            continue;
        }
        if ascii_run > 0 {
            tokens = tokens.saturating_add(ascii_run.div_ceil(4usize));
            ascii_run = 0;
        }
        tokens = tokens.saturating_add(if is_cjk_character(character) {
            1usize
        } else {
            2usize
        });
    }
    if ascii_run > 0 {
        tokens = tokens.saturating_add(ascii_run.div_ceil(4usize));
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
    use vulcan_core::types::{ProviderContinuation, ProviderPromptLayers};

    /// Builds one empty request snapshot for context-window budget tests.
    /// 为上下文窗口预算测试构建一份空请求快照。
    fn test_request_snapshot() -> CoreProviderRequestSnapshot {
        CoreProviderRequestSnapshot {
            provider_id: String::from("provider"),
            model_id: String::from("model"),
            system_prompt: String::new(),
            prompt_layers: ProviderPromptLayers::default(),
            prompt: String::new(),
            prompt_in_history: false,
            history: Vec::new(),
            temperature: None,
            max_turns: 1,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        }
    }

    /// Verifies a session node sets planning lines while retaining the real model request limit.
    /// 验证会话节点决定规划阈值，同时保留模型真实请求上限。
    #[test]
    fn session_node_changes_planning_window_without_changing_provider_limit() {
        let capabilities = ContextBudgetCapabilities {
            context_limit: Some(1_000_000),
            output_limit: Some(32_000),
            ..ContextBudgetCapabilities::default()
        };
        let request_snapshot = test_request_snapshot();
        let policy = ContextCompactionPolicy {
            compression_window_node_id: Some(
                crate::ContextCompactionWindowNodeId::CompressionWindow3,
            ),
            compression_window_limit: Some(272_000),
            ..ContextCompactionPolicy::default()
        };

        let budget = build_request_context_budget_snapshot_with_options(
            RequestContextBudgetSnapshotInput {
                provider_id: "provider",
                model_id: "model",
                protocol_profile_id: "protocol",
                capabilities: &capabilities,
                reasoning: ContextReasoningEffort::None,
                request_snapshot: &request_snapshot,
                observed_usage: None,
                accounted_input_tokens: Some(500_000),
                accounting_source: ContextBudgetAccountingSource::CharacterEstimate,
                accounting_warning_code: Some("test"),
                provider_anchor_call_id: None,
                prefill_input_tokens: None,
            },
            RequestContextBudgetSnapshotOptions {
                compaction_policy: Some(policy),
            },
        );

        assert_eq!(budget.context_limit, Some(1_000_000));
        assert_eq!(budget.effective_context_window, Some(258_400));
        assert_eq!(budget.provider_request_hard_limit, Some(1_000_000));
        assert_eq!(budget.auto_compact_limit, Some(244_800));
        assert!(budget.should_compact());
        assert!(budget.active_context_tokens < budget.provider_request_hard_limit.unwrap());
    }

    /// Verifies a smaller selected model exposes its own hard admission line before compaction.
    /// 验证切换到较小模型时先暴露其自身的硬准入线。
    #[test]
    fn smaller_model_limit_is_exceeded_before_any_session_compaction() {
        let capabilities = ContextBudgetCapabilities {
            context_limit: Some(250_000),
            output_limit: Some(32_000),
            ..ContextBudgetCapabilities::default()
        };
        let request_snapshot = test_request_snapshot();
        let budget = build_request_context_budget_snapshot_with_options(
            RequestContextBudgetSnapshotInput {
                provider_id: "provider",
                model_id: "smaller-model",
                protocol_profile_id: "protocol",
                capabilities: &capabilities,
                reasoning: ContextReasoningEffort::None,
                request_snapshot: &request_snapshot,
                observed_usage: None,
                accounted_input_tokens: Some(500_000),
                accounting_source: ContextBudgetAccountingSource::CharacterEstimate,
                accounting_warning_code: None,
                provider_anchor_call_id: None,
                prefill_input_tokens: None,
            },
            RequestContextBudgetSnapshotOptions::default(),
        );

        assert_eq!(budget.provider_request_hard_limit, Some(250_000));
        assert!(budget.active_context_tokens >= budget.provider_request_hard_limit.unwrap());
    }

    /// Verifies Chinese text is estimated close to one token per visible character.
    /// 验证中文文本按接近每个可见字符一个 Token 估算。
    #[test]
    fn estimate_text_tokens_counts_chinese_characters_one_to_one() {
        assert_eq!(estimate_text_tokens_from_characters("你好世界"), 4);
    }

    /// Verifies ASCII text still uses the compact four-character heuristic.
    /// 验证 ASCII 文本仍使用紧凑的四字符启发式估算。
    #[test]
    fn estimate_text_tokens_keeps_ascii_four_to_one() {
        assert_eq!(estimate_text_tokens_from_characters("abcdefgh"), 2);
    }

    /// Verifies one-megabyte CJK and ASCII inputs remain iterative, deterministic, and overflow-safe.
    /// 验证一 MiB 的中日韩与 ASCII 输入保持迭代、确定且不会发生算术溢出。
    #[test]
    fn character_estimate_handles_one_megabyte_language_extremes() {
        let ascii = "a".repeat(1_048_576);
        let cjk = "你".repeat(1_048_576);

        assert_eq!(estimate_mixed_language_text_tokens(&ascii), 262_144);
        assert_eq!(estimate_mixed_language_text_tokens(&cjk), 1_048_576);
    }

    /// Verifies a ten-megabyte mixed CJK, ASCII, and emoji payload completes without recursion or panic.
    /// 验证十 MiB 的中日韩、ASCII 与表情混合载荷无需递归且不会 panic。
    #[test]
    fn character_estimate_handles_ten_megabyte_mixed_text() {
        // Each repeated unit is exactly eight UTF-8 bytes and contributes four conservative tokens.
        // 每个重复单元恰好占八个 UTF-8 字节，并贡献四个保守 Token。
        let mixed = "a你🙂".repeat(1_310_720);

        assert_eq!(mixed.len(), 10_485_760);
        assert_eq!(estimate_mixed_language_text_tokens(&mixed), 5_242_880);
    }

    /// Verifies provider-observed usage overrides local mixed-language estimates.
    /// 验证 Provider 实测用量会覆盖本地混合语言估算。
    #[test]
    fn calibrated_input_token_estimate_prefers_observed_usage() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(128),
            uncached_input_tokens: Some(128),
            output_tokens: Some(8),
            reasoning_tokens: Some(4),
            total_tokens: Some(140),
            cached_input_tokens: None,
            cache_creation_input_tokens: None,
            ..Default::default()
        };

        assert_eq!(calibrated_input_token_estimate(10, Some(&usage)), 128);
    }

    /// Verifies total-scope usage consumes the already aligned accounting input.
    /// 验证总量范围直接使用已对齐的会计输入值。
    #[test]
    fn active_context_tokens_uses_aligned_total_scope_input() {
        assert_eq!(
            context_tokens_for_scope(175, ContextWindowUsageScope::Total, None),
            175
        );
    }

    /// Verifies body-after-prefix scope subtracts the absolute Codex prefill baseline.
    /// 验证前缀后正文范围会减去 Codex 的绝对前缀基线。
    #[test]
    fn active_context_tokens_body_scope_subtracts_prefill_baseline() {
        assert_eq!(
            context_tokens_for_scope(175, ContextWindowUsageScope::BodyAfterPrefix, Some(100),),
            75
        );
    }

    /// Verifies an unknown prefill baseline starts a new body window at zero.
    /// 验证未知前缀基线会让新正文窗口从零开始。
    #[test]
    fn active_context_tokens_body_scope_defaults_unknown_prefill_to_current_total() {
        assert_eq!(
            context_tokens_for_scope(175, ContextWindowUsageScope::BodyAfterPrefix, None),
            0
        );
    }

    /// Verifies normalized Anthropic input includes cache reads exactly once.
    /// 验证归一化的 Anthropic 输入只包含一次缓存读取。
    #[test]
    fn observed_input_tokens_use_normalized_anthropic_input() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(112_015),
            uncached_input_tokens: Some(399),
            output_tokens: Some(380),
            reasoning_tokens: None,
            total_tokens: Some(112_395),
            cached_input_tokens: Some(111_616),
            cache_creation_input_tokens: Some(0),
            ..Default::default()
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
            uncached_input_tokens: Some(399),
            output_tokens: Some(380),
            reasoning_tokens: None,
            total_tokens: Some(112_395),
            cached_input_tokens: Some(111_616),
            cache_creation_input_tokens: None,
            ..Default::default()
        };

        assert_eq!(
            observed_input_tokens_from_usage(Some(&usage)),
            Some(112_015)
        );
    }

    /// Exercises absent, zero and reported inputs against coincident cache and thinking totals.
    /// 验证输入缺失、零值与已报告值不会被恰好相等的缓存和思考总量改写。
    #[test]
    fn observed_input_preserves_unknown_and_ignores_cache_total_coincidences() {
        for input in [None, Some(0), Some(10_000)] {
            for cache in [None, Some(0), Some(2_500)] {
                // The total includes independent thinking; cache remains an input subset.
                // 总量包含独立思考，而缓存仍然只是输入子集。
                let usage = ProviderUsageSnapshot {
                    input_tokens: input,
                    output_tokens: Some(5_000),
                    reasoning_tokens: Some(2_500),
                    total_tokens: Some(17_500),
                    cached_input_tokens: cache,
                    ..Default::default()
                };
                assert_eq!(observed_input_tokens_from_usage(Some(&usage)), input);
                assert_eq!(
                    calibrated_input_token_estimate(321, Some(&usage)),
                    input.unwrap_or(321)
                );
            }
        }
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

    /// Verifies BodyAfterPrefix affects only the automatic line while the full context still guards H.
    /// 验证 BodyAfterPrefix 只影响自动压缩线，而完整上下文仍负责 H 硬窗口保护。
    #[test]
    fn body_scope_does_not_bypass_hard_context_window() {
        assert_eq!(
            context_budget_severity_for_limits_with_compaction_context(
                940,
                Some(950),
                Some(900),
                800
            ),
            ContextBudgetSeverity::Healthy
        );
        assert_eq!(
            context_budget_severity_for_limits_with_compaction_context(
                960,
                Some(950),
                Some(900),
                800
            ),
            ContextBudgetSeverity::Overflow
        );
        assert_eq!(
            context_budget_severity_for_limits_with_compaction_context(
                700,
                Some(950),
                Some(900),
                900
            ),
            ContextBudgetSeverity::CompactSoon
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
            Some("Budget fingerprint changed · Context layer fingerprint changed")
        );
    }

    /// Verifies retained-tail provenance and raw context windows participate in budget fingerprints.
    /// 验证尾部策略来源与原始上下文窗口会参与预算指纹。
    #[test]
    fn budget_fingerprint_changes_with_adaptive_tail_resolution() {
        let capabilities = ContextBudgetCapabilities::default();
        let missing_policy = ContextCompactionPolicy::default();
        let declared_tail = crate::resolve_context_compaction_tail_policy(Some(300_000));
        let declared_policy = ContextCompactionPolicy {
            tail_turns: declared_tail.tail_segments,
            preserve_recent_tokens: declared_tail.preserve_recent_tokens,
            tail_context_limit: declared_tail.context_limit,
            tail_policy_source: declared_tail.source,
            ..missing_policy
        };

        let missing_fingerprint = build_context_budget_fingerprint(
            "provider",
            "model",
            "protocol",
            &capabilities,
            ContextReasoningEffort::None,
            0,
            Some(missing_policy),
        );
        let declared_fingerprint = build_context_budget_fingerprint(
            "provider",
            "model",
            "protocol",
            &capabilities,
            ContextReasoningEffort::None,
            0,
            Some(declared_policy),
        );

        assert_ne!(missing_fingerprint, declared_fingerprint);
        assert!(declared_fingerprint.contains("tail_context=Some(300000)"));
        assert!(declared_fingerprint.contains("tail_source=declared_context_limit"));
    }
}
