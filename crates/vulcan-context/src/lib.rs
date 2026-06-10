//! Shared context-management primitives reused by the host runtime and workspace UI.
//! 供宿主运行时与工作区界面复用的共享上下文管理基础能力。

pub mod assembly;
pub mod budget;
pub mod protocol;
pub mod request_mode;
pub mod runtime;
pub mod transcript;

pub use assembly::{
    ContextAssembly, ContextBaseline, ContextBaselineComparison, ContextLayer, ContextLayerKind,
    build_context_fingerprint, build_provider_request_context_assembly,
};
pub use budget::{
    ContextBudgetAiToolCapability, ContextBudgetCapabilities, ContextProviderApiFamily,
    ContextReasoningEffort, ContextTokenEstimator, apply_context_budget_anchor_baseline,
    build_context_budget_fingerprint, build_request_context_budget_snapshot,
    build_request_context_budget_snapshot_with_estimator,
    build_request_context_budget_snapshot_with_policy_and_estimator,
    calibrated_input_token_estimate, estimate_mixed_language_text_tokens, is_cjk_character,
    observed_input_tokens_from_usage,
};
pub use protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState, SessionConversationToolCall, SessionProviderContinuationState,
    append_assistant_text_message, append_record, append_record_body_delta,
    append_user_text_message, collect_host_visible_attachments,
    conversation_state_from_session_messages, conversation_state_to_context_records,
    conversation_state_to_session_messages,
};
pub use request_mode::{
    ContextFullRequestReason, ContextIncrementalDisabledReason, ContextRequestMode,
    ContextRequestModeInput, resolve_context_request_mode,
};
pub use runtime::{
    RUNTIME_LOG_SCHEMA_VERSION, RuntimeAssistantMessageEvent, RuntimeContextCompactionAppliedEvent,
    RuntimeInjectionAppliedEvent, RuntimeInjectionCancelledEvent, RuntimeInjectionMergedEvent,
    RuntimeLogEvent, RuntimeLogEventLine, RuntimeProviderLoweringRepairEvent,
    RuntimeProviderNativeStateEnvelope, RuntimeProviderRequestCheckpointEvent,
    RuntimeProviderResponseCheckpointEvent, RuntimeProviderTurnCheckpointEvent, RuntimeRepairEvent,
    RuntimeSystemUpdateEvent, RuntimeSystemUpdateLoweringHint, RuntimeSystemUpdateTrust,
    RuntimeToolBatchCompletedEvent, RuntimeToolBatchStartedEvent, RuntimeToolCallEvent,
    RuntimeToolResultEvent, RuntimeToolResultStatus, RuntimeUserMessageEvent,
    conversation_state_from_runtime_log_lines, runtime_log_lines_from_conversation_state,
};
pub use transcript::{
    SessionCompactionEntry, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
};

use vulcan_core::types::ProviderUsageSnapshot;

/// Ratio above which the host should proactively compact history before the next run.
/// 超过该占用比例后，宿主应在下一次执行前主动压缩历史。
pub const CONTEXT_BUDGET_COMPACTION_RATIO: f32 = 0.85;

/// Ratio above which the host should warn that the session is close to the hard model window.
/// 超过该占用比例后，宿主应警告当前会话已经接近模型硬上限。
pub const CONTEXT_BUDGET_WARNING_RATIO: f32 = 0.70;

/// Ratio above which the host should treat the session as effectively overflowed.
/// 超过该占用比例后，宿主应将当前会话视为实际上已经溢出。
pub const CONTEXT_BUDGET_OVERFLOW_RATIO: f32 = 0.97;

/// OpenCode-style policy that controls automatic compaction and retained history shape.
/// 控制自动压缩与历史保留形态的 OpenCode 风格策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextCompactionPolicy {
    /// Whether the host may automatically compact history after a budget check.
    /// 宿主是否允许在预算检查后自动压缩历史。
    pub auto: bool,
    /// Whether old tool outputs should be pruned into compact placeholders during compaction.
    /// 压缩期间是否应将旧工具输出剪枝为紧凑占位内容。
    pub prune: bool,
    /// Number of recent user turns retained verbatim after compaction.
    /// 压缩后按原样保留的最近用户轮次数。
    pub tail_turns: usize,
    /// Maximum approximate token budget retained for the recent tail.
    /// 为最近尾部保留的近似 Token 上限。
    pub preserve_recent_tokens: usize,
    /// Input-token reserve kept free for compaction summaries and the next user turn.
    /// 为压缩摘要与下一轮用户输入预留的输入 Token 缓冲。
    pub reserved: usize,
}

impl Default for ContextCompactionPolicy {
    /// Builds the default compaction policy used until project-level settings expose overrides.
    /// 构建默认压缩策略，直到项目级设置暴露覆盖项。
    fn default() -> Self {
        Self {
            auto: true,
            prune: true,
            tail_turns: 1,
            preserve_recent_tokens: 2_048,
            reserved: 2_048,
        }
    }
}

/// Resolved context-budget severity used by UI and compaction decisions.
/// 供界面与压缩决策使用的已解析上下文预算严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextBudgetSeverity {
    /// The current request sits comfortably inside the model budget.
    /// 当前请求仍然舒适地落在模型预算以内。
    Healthy,
    /// The current request is approaching the configured model budget.
    /// 当前请求正在逼近配置模型预算。
    Warning,
    /// The current request should trigger history compaction before the next execution.
    /// 当前请求应在下一次执行前触发历史压缩。
    CompactSoon,
    /// The current request is effectively overflowed for the configured model budget.
    /// 对当前配置模型预算而言，该请求已基本溢出。
    Overflow,
}

impl Default for ContextBudgetSeverity {
    /// Builds the default severity bucket used before any explicit budget calculation has been performed.
    /// 构建在尚未执行显式预算计算前使用的默认严重级别分桶。
    fn default() -> Self {
        Self::Healthy
    }
}

/// Structured context-budget snapshot reused by execution previews, compaction checks, and runtime summaries.
/// 供执行预览、压缩检查与运行时摘要复用的结构化上下文预算快照。
#[derive(Debug, Clone)]
pub struct ContextBudgetSnapshot {
    /// Stable budget fingerprint used to detect whether one previous compaction anchor still matches the active runtime shape.
    /// 用于判断旧压缩锚点是否仍匹配当前运行时形态的稳定预算指纹。
    pub fingerprint: String,
    /// Provider/model context-window limit declared by the active capability snapshot.
    /// 当前能力快照声明的 Provider/Model 上下文窗口上限。
    pub context_limit: Option<usize>,
    /// Effective input budget limit used by host estimation.
    /// 宿主估算使用的生效输入预算上限。
    pub effective_input_limit: Option<usize>,
    /// Effective output reserve held back for the provider response.
    /// 为 Provider 响应预留的生效输出预算。
    pub reserved_output_tokens: usize,
    /// Additional reserve held for explicit reasoning or thinking payloads.
    /// 为显式推理或思考载荷保留的附加预算。
    pub reserved_reasoning_tokens: usize,
    /// Additional input reserve held for automatic compaction and the next user turn.
    /// 为自动压缩与下一轮用户输入保留的附加输入预算。
    pub reserved_compaction_tokens: usize,
    /// Whether this snapshot allows automatic compaction when the severity crosses the threshold.
    /// 当严重级别越过阈值时，该快照是否允许自动压缩。
    pub compaction_auto_enabled: bool,
    /// Whether the current runtime shape still matches the last accepted budget baseline.
    /// 当前运行时形态是否仍匹配最近一次已接受的预算基线。
    pub baseline_matches_active_runtime: bool,
    /// Whether the anchor budget fingerprint still matches the active runtime budget fingerprint.
    /// 锚点预算指纹是否仍匹配当前运行时预算指纹。
    pub baseline_budget_fingerprint_matches: bool,
    /// Whether the anchor layered-context fingerprint still matches the active runtime layered-context fingerprint.
    /// 锚点分层上下文指纹是否仍匹配当前运行时分层上下文指纹。
    pub baseline_assembly_fingerprint_matches: bool,
    /// Optional human-readable mismatch reason recorded when the active runtime no longer matches the anchor baseline.
    /// 当当前运行时不再匹配锚点基线时记录的可选人类可读失配原因。
    pub baseline_mismatch_reason: Option<String>,
    /// Estimated system-prompt token cost.
    /// 系统提示词的估算 Token 成本。
    pub estimated_system_tokens: usize,
    /// Estimated canonical-history token cost.
    /// 规范历史的估算 Token 成本。
    pub estimated_history_tokens: usize,
    /// Estimated current-prompt token cost.
    /// 当前提示词的估算 Token 成本。
    pub estimated_prompt_tokens: usize,
    /// Estimated tool-schema token cost.
    /// 工具 schema 的估算 Token 成本。
    pub estimated_tool_tokens: usize,
    /// Estimated total input token cost before response generation starts.
    /// 响应生成开始前的估算总输入 Token 成本。
    pub estimated_input_tokens: usize,
    /// Estimated total end-to-end token footprint including reasoning and output reserves.
    /// 包含推理与输出预留在内的估算端到端 Token 占用总量。
    pub estimated_total_tokens: usize,
    /// Final usage ratio derived from the effective input limit.
    /// 根据生效输入上限推导出的最终占用比例。
    pub usage_ratio: f32,
    /// Optional latest observed provider usage snapshot reused to calibrate host estimates conservatively.
    /// 用于保守校准宿主估算的可选最近一次 Provider 实测用量快照。
    pub observed_usage: Option<ProviderUsageSnapshot>,
    /// Final severity bucket used by diagnostics and compaction thresholds.
    /// 供诊断与压缩阈值使用的最终严重级别分桶。
    pub severity: ContextBudgetSeverity,
}

impl Default for ContextBudgetSnapshot {
    /// Builds the empty budget snapshot used before any explicit context-budget estimation has been performed.
    /// 构建在尚未执行显式上下文预算估算前使用的空预算快照。
    fn default() -> Self {
        Self {
            fingerprint: String::new(),
            context_limit: None,
            effective_input_limit: None,
            reserved_output_tokens: 0,
            reserved_reasoning_tokens: 0,
            reserved_compaction_tokens: 0,
            compaction_auto_enabled: false,
            baseline_matches_active_runtime: true,
            baseline_budget_fingerprint_matches: true,
            baseline_assembly_fingerprint_matches: true,
            baseline_mismatch_reason: None,
            estimated_system_tokens: 0,
            estimated_history_tokens: 0,
            estimated_prompt_tokens: 0,
            estimated_tool_tokens: 0,
            estimated_input_tokens: 0,
            estimated_total_tokens: 0,
            usage_ratio: 0.0,
            observed_usage: None,
            severity: ContextBudgetSeverity::Healthy,
        }
    }
}

impl ContextBudgetSnapshot {
    /// Returns the usage percentage used by the workbench progress bar.
    /// 返回供工作台进度条使用的占用百分比。
    pub fn usage_percent(&self) -> f32 {
        (self.usage_ratio * 100.0).clamp(0.0, 100.0)
    }

    /// Returns whether the host should compact visible history before or immediately after the next execution.
    /// 返回宿主是否应在下一次执行前或执行后立即压缩可见历史。
    pub fn should_compact(&self) -> bool {
        if !self.compaction_auto_enabled {
            return false;
        }
        if !self.baseline_matches_active_runtime {
            return false;
        }
        matches!(
            self.severity,
            ContextBudgetSeverity::CompactSoon | ContextBudgetSeverity::Overflow
        )
    }

    /// Builds one compact usage label for the workbench context bar.
    /// 为工作台上下文条构建一条紧凑占用标签。
    pub fn usage_label(&self) -> String {
        if let Some(limit) = self.effective_input_limit {
            format!("{} / {}", self.estimated_input_tokens, limit)
        } else {
            format!("{} / 未声明上限", self.estimated_input_tokens)
        }
    }

    /// Builds one compact diagnostics note describing the current budget state.
    /// 构建一条描述当前预算状态的紧凑诊断说明。
    pub fn summary_note(&self) -> String {
        let severity_label = match self.severity {
            ContextBudgetSeverity::Healthy => "正常",
            ContextBudgetSeverity::Warning => "预警",
            ContextBudgetSeverity::CompactSoon => "建议压缩",
            ContextBudgetSeverity::Overflow => "预算溢出",
        };
        let baseline_label = if self.baseline_matches_active_runtime {
            "基线匹配"
        } else {
            "基线失配"
        };
        let baseline_reason_suffix = self
            .baseline_mismatch_reason
            .as_deref()
            .map(|reason| format!(" · {reason}"))
            .unwrap_or_default();
        let context_limit_label = self
            .context_limit
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("未声明"));
        let input_source_label = if self.observed_usage.is_some() {
            "输入(实测)"
        } else {
            "输入(估算)"
        };
        let breakdown_label = if self.observed_usage.is_some() {
            "本地估算拆分"
        } else {
            "估算拆分"
        };
        format!(
            "{severity_label} · {baseline_label}{} · 上下文窗 {} · {} {} · {} 系统 {} / 历史 {} / 当前 {} / 工具 {} · 端到端占用 {} · 输出预留 {} · 推理预留 {} · 压缩预留 {}{}",
            baseline_reason_suffix,
            context_limit_label,
            input_source_label,
            self.usage_label(),
            breakdown_label,
            self.estimated_system_tokens,
            self.estimated_history_tokens,
            self.estimated_prompt_tokens,
            self.estimated_tool_tokens,
            self.estimated_total_tokens,
            self.reserved_output_tokens,
            self.reserved_reasoning_tokens,
            self.reserved_compaction_tokens,
            self.observed_usage
                .as_ref()
                .map(|usage| format!(" · {}", provider_usage_label(usage)))
                .unwrap_or_default()
        )
    }
}

/// Builds one compact provider-usage label suitable for diagnostics and context-budget summaries.
/// 构建一条适合诊断与上下文预算摘要展示的紧凑 Provider 用量标签。
fn provider_usage_label(usage: &ProviderUsageSnapshot) -> String {
    let input = usage
        .input_tokens
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("?"));
    let output = usage
        .output_tokens
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("?"));
    let reasoning = usage
        .reasoning_tokens
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("?"));
    let total = usage
        .total_tokens
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("?"));
    format!("最近实测 输入 {input} / 输出 {output} / 推理 {reasoning} / 总计 {total}")
}
