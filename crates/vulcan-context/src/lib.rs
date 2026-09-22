//! Shared context-management primitives reused by the host runtime and workspace UI.
//! 供宿主运行时与工作区界面复用的共享上下文管理基础能力。

pub mod assembly;
pub mod budget;
pub mod protocol;
pub mod request_mode;
pub mod runtime_host;
pub mod system_update;
pub mod transcript;

pub use assembly::{
    ContextAssembly, ContextBaseline, ContextBaselineComparison, ContextLayer, ContextLayerKind,
    build_context_fingerprint, build_provider_request_context_assembly,
};
pub use budget::{
    ContextBudgetAiToolCapability, ContextBudgetCapabilities, ContextReasoningEffort,
    RequestContextBudgetSnapshotInput, RequestContextBudgetSnapshotOptions,
    apply_context_budget_anchor_baseline, build_context_budget_fingerprint,
    build_request_context_budget_snapshot, build_request_context_budget_snapshot_with_options,
    calibrated_input_token_estimate, estimate_mixed_language_text_tokens, is_cjk_character,
    observed_input_tokens_from_usage,
};
pub use protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState, SessionConversationToolCall, SessionProviderContinuationState,
    append_assistant_text_message, append_record, append_record_body_delta,
    append_session_conversation_message, append_user_text_message,
    collect_host_visible_attachments, conversation_state_from_session_messages,
    conversation_state_to_compaction_source_messages,
};
pub use request_mode::{
    ContextFullRequestReason, ContextIncrementalDisabledReason, ContextRequestMode,
    ContextRequestModeInput, resolve_context_request_mode,
};
pub use system_update::{
    RuntimeProviderLoweringRepairEvent, RuntimeSystemUpdateEvent, RuntimeSystemUpdateLoweringHint,
    RuntimeSystemUpdateTrust,
};
pub use transcript::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};

use serde::{Deserialize, Serialize};
pub use vulcan_ai_contract::ContextCompactionWindowNodeId;
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

/// Selects the usage scope used to compare the active context with the model window.
/// 选择用于将活动上下文与模型窗口比较的用量范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextWindowUsageScope {
    /// Counts the provider-reported total usage for the active context.
    /// 使用 Provider 报告的总用量作为活动上下文。
    Total,
    /// Counts only the body after the stable prompt prefix.
    /// 仅统计稳定提示词前缀之后的正文部分。
    BodyAfterPrefix,
}

impl Default for ContextWindowUsageScope {
    /// Returns the official Codex-compatible total usage scope.
    /// 返回官方 Codex 兼容的总用量范围。
    fn default() -> Self {
        Self::Total
    }
}

/// Stores sparse context-window policy changes for one configuration layer.
/// 保存某一配置层的稀疏上下文窗口策略变更。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextWindowPolicyOverrides {
    /// Effective hard-window percentage applied to the raw model context window.
    /// 应用于模型原始上下文窗口的有效硬窗口百分比。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_context_window_percent: Option<u8>,
    /// Automatic-compaction percentage applied to the raw model context window.
    /// 应用于模型原始上下文窗口的自动压缩百分比。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact_percent: Option<u8>,
    /// Explicit automatic-compaction token limit, when the provider exposes one.
    /// Provider 明确公开自动压缩 Token 上限时使用的值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact_token_limit: Option<u64>,
    /// Usage scope used by the automatic-compaction calculation.
    /// 自动压缩计算使用的用量范围。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact_scope: Option<ContextWindowUsageScope>,
    /// Optional fallback buffer reserved only when a provider fallback prompt exists.
    /// 仅在 Provider 存在回退提示词时预留的可选缓冲区。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_buffer_tokens: Option<u64>,
}

impl ContextWindowPolicyOverrides {
    /// Reports whether this sparse layer carries no explicit value.
    /// 报告当前稀疏层是否没有任何显式值。
    pub fn is_empty(&self) -> bool {
        self.effective_context_window_percent.is_none()
            && self.auto_compact_percent.is_none()
            && self.auto_compact_token_limit.is_none()
            && self.auto_compact_scope.is_none()
            && self.fallback_buffer_tokens.is_none()
    }

    /// Validates one sparse policy layer before it is persisted.
    /// 在持久化前校验一层稀疏策略。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` for a safe layer, or a controlled validation message.
    /// 安全时返回 `Ok(())`，否则返回受控校验消息。
    pub fn validate(&self) -> Result<(), String> {
        for (label, value) in [
            (
                "effective_context_window_percent",
                self.effective_context_window_percent,
            ),
            ("auto_compact_percent", self.auto_compact_percent),
        ] {
            if value.is_some_and(|percent| !(1..=100).contains(&percent)) {
                return Err(format!("{label} must be between 1 and 100."));
            }
        }
        if self
            .auto_compact_token_limit
            .is_some_and(|tokens| tokens == 0)
        {
            return Err(String::from(
                "auto_compact_token_limit must be greater than zero.",
            ));
        }
        if self
            .fallback_buffer_tokens
            .is_some_and(|tokens| usize::try_from(tokens).is_err())
        {
            return Err(String::from(
                "fallback_buffer_tokens exceeds the host usize range.",
            ));
        }
        if let (Some(effective), Some(auto_compact)) = (
            self.effective_context_window_percent,
            self.auto_compact_percent,
        ) && auto_compact > effective
        {
            return Err(String::from(
                "auto_compact_percent cannot exceed effective_context_window_percent.",
            ));
        }
        Ok(())
    }

    /// Merges a stronger sparse layer over this layer without inventing absent values.
    /// 将更强的稀疏层覆盖到当前层，且不为缺失字段臆造值。
    pub fn merged_with(self, stronger: Self) -> Self {
        Self {
            effective_context_window_percent: stronger
                .effective_context_window_percent
                .or(self.effective_context_window_percent),
            auto_compact_percent: stronger.auto_compact_percent.or(self.auto_compact_percent),
            auto_compact_token_limit: stronger
                .auto_compact_token_limit
                .or(self.auto_compact_token_limit),
            auto_compact_scope: stronger.auto_compact_scope.or(self.auto_compact_scope),
            fallback_buffer_tokens: stronger
                .fallback_buffer_tokens
                .or(self.fallback_buffer_tokens),
        }
    }

    /// Resolves one sparse layer against the official Codex-compatible defaults.
    /// 将一层稀疏策略解析为官方 Codex 兼容的完整默认值。
    pub fn resolve(self) -> ContextWindowPolicy {
        ContextWindowPolicy {
            effective_context_window_percent: self.effective_context_window_percent.unwrap_or(95),
            auto_compact_percent: self.auto_compact_percent.unwrap_or(90),
            auto_compact_token_limit: self.auto_compact_token_limit,
            auto_compact_scope: self.auto_compact_scope.unwrap_or_default(),
            fallback_buffer_tokens: self
                .fallback_buffer_tokens
                .map(|value| usize::try_from(value).unwrap_or(usize::MAX))
                .unwrap_or(0),
        }
    }
}

/// Fully resolved Codex-compatible context-window policy frozen for one turn.
/// 为单个回合冻结的完整 Codex 兼容上下文窗口策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextWindowPolicy {
    /// Effective hard-window percentage applied to the raw model window.
    /// 应用于模型原始窗口的有效硬窗口百分比。
    pub effective_context_window_percent: u8,
    /// Automatic-compaction percentage applied to the raw model context window.
    /// 应用于模型原始上下文窗口的自动压缩百分比。
    pub auto_compact_percent: u8,
    /// Explicit automatic-compaction token limit, when configured.
    /// 已配置的显式自动压缩 Token 上限。
    pub auto_compact_token_limit: Option<u64>,
    /// Usage scope used for automatic compaction.
    /// 自动压缩使用的用量范围。
    pub auto_compact_scope: ContextWindowUsageScope,
    /// Fallback buffer used only when a fallback prompt exists.
    /// 仅在存在回退提示词时使用的回退缓冲区。
    pub fallback_buffer_tokens: usize,
}

impl Default for ContextWindowPolicy {
    /// Returns the official Codex defaults: 95% hard window, 90% compaction, total scope, no buffer.
    /// 返回官方 Codex 默认值：95% 硬窗口、90% 压缩线、总用量范围、零缓冲。
    fn default() -> Self {
        ContextWindowPolicyOverrides::default().resolve()
    }
}

impl ContextWindowPolicy {
    /// Validates one fully resolved context-window policy, including cross-field ordering.
    /// 校验一份完整解析后的上下文窗口策略，包括字段之间的顺序约束。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` for a policy that can be used by the budget calculator, or an actionable error.
    /// 可供预算计算器使用时返回 `Ok(())`，否则返回可操作的错误信息。
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=100).contains(&self.effective_context_window_percent) {
            return Err(String::from(
                "effective_context_window_percent must be between 1 and 100.",
            ));
        }
        if !(1..=100).contains(&self.auto_compact_percent) {
            return Err(String::from(
                "auto_compact_percent must be between 1 and 100.",
            ));
        }
        if self.auto_compact_percent > self.effective_context_window_percent {
            return Err(String::from(
                "auto_compact_percent cannot exceed effective_context_window_percent.",
            ));
        }
        if self.auto_compact_token_limit == Some(0) {
            return Err(String::from(
                "auto_compact_token_limit must be greater than zero.",
            ));
        }
        Ok(())
    }
}

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
    /// Maximum recent assistant/provider execution segments retained after compaction.
    /// 压缩后保留的最近 Assistant/Provider 执行段数量上限。
    pub tail_turns: usize,
    /// Maximum approximate token budget retained for the recent tail.
    /// 为最近尾部保留的近似 Token 上限。
    pub preserve_recent_tokens: usize,
    /// Raw model-declared context window used to resolve the retained-tail policy.
    /// 用于解析尾部保留策略的模型原始声明上下文窗口。
    pub tail_context_limit: Option<usize>,
    /// Provenance describing how the retained-tail policy was resolved.
    /// 描述尾部保留策略解析方式的来源。
    pub tail_policy_source: ContextCompactionTailPolicySource,
    /// Optional fixed node selected by the owning session for this frozen execution.
    /// 所属会话为本次冻结执行选择的可选固定节点。
    pub compression_window_node_id: Option<ContextCompactionWindowNodeId>,
    /// Resolved node token limit used only by compaction calculations for this execution.
    /// 本次执行中仅供压缩计算使用的已解析节点 Token 上限。
    pub compression_window_limit: Option<usize>,
    /// Legacy input reserve retained for decoding old settings; it no longer shrinks the model window.
    /// 保留用于读取旧设置的输入缓冲字段；它不再缩小模型窗口。
    pub reserved: usize,
    /// Codex-compatible context-window policy frozen for this execution turn.
    /// 为本次执行回合冻结的 Codex 兼容上下文窗口策略。
    pub window: ContextWindowPolicy,
    /// Whether the window policy was explicitly frozen by the resolved request rather than supplied as behavior-only input.
    /// 标记窗口策略是否已经由已解析请求显式冻结，而不是仅作为行为开关输入。
    pub window_policy_is_explicit: bool,
}

impl Default for ContextCompactionPolicy {
    /// Builds the minimum retained-tail fallback used only when the model context window is missing.
    /// 构建仅在模型上下文窗口缺失时使用的最小尾部保底策略。
    fn default() -> Self {
        Self {
            auto: true,
            prune: true,
            tail_turns: CONTEXT_COMPACTION_MIN_TAIL_SEGMENTS,
            preserve_recent_tokens: CONTEXT_COMPACTION_MIN_TAIL_TOKENS,
            tail_context_limit: None,
            tail_policy_source: ContextCompactionTailPolicySource::MissingContextLimitMinimum,
            compression_window_node_id: None,
            compression_window_limit: None,
            reserved: 0,
            window: ContextWindowPolicy::default(),
            window_policy_is_explicit: false,
        }
    }
}

/// Raw context-window interval that adds one retained segment and 2.5K retained tokens.
/// 每增加一个尾部执行段和 2.5K 尾部 Token 所对应的原始上下文窗口区间。
pub const CONTEXT_COMPACTION_TAIL_WINDOW_STEP: usize = 50_000;

/// Retained-token increment applied for every complete context-window interval.
/// 每个完整上下文窗口区间增加的尾部 Token 数。
pub const CONTEXT_COMPACTION_TAIL_TOKEN_STEP: usize = 2_500;

/// Minimum retained-tail token budget used for small or undeclared windows.
/// 小窗口或未声明窗口使用的最小尾部 Token 预算。
pub const CONTEXT_COMPACTION_MIN_TAIL_TOKENS: usize = 5_000;

/// Maximum retained-tail token budget used for one million tokens and larger windows.
/// 一百万 Token 及以上窗口使用的最大尾部 Token 预算。
pub const CONTEXT_COMPACTION_MAX_TAIL_TOKENS: usize = 50_000;

/// Minimum retained assistant/provider execution-segment limit.
/// 最小 Assistant/Provider 执行段保留上限。
pub const CONTEXT_COMPACTION_MIN_TAIL_SEGMENTS: usize = 5;

/// Maximum retained assistant/provider execution-segment limit.
/// 最大 Assistant/Provider 执行段保留上限。
pub const CONTEXT_COMPACTION_MAX_TAIL_SEGMENTS: usize = 20;

/// Provenance of one resolved adaptive retained-tail policy.
/// 一份已解析自适应尾部保留策略的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextCompactionTailPolicySource {
    /// The policy was calculated from the raw model-declared context limit.
    /// 策略由模型原始声明的上下文上限计算得到。
    DeclaredContextLimit,
    /// The model omitted its context limit, so the minimum fallback was used.
    /// 模型未声明上下文上限，因此使用最小保底值。
    MissingContextLimitMinimum,
}

impl ContextCompactionTailPolicySource {
    /// Returns the stable persisted diagnostic label for this policy source.
    /// 返回该策略来源用于持久化诊断的稳定标签。
    ///
    /// # Returns
    /// # 返回值
    /// A stable snake-case label shared by fingerprints, WebUI metadata, and task markers.
    /// 返回由指纹、WebUI 元数据和任务标记共享的稳定蛇形命名标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::DeclaredContextLimit => "declared_context_limit",
            Self::MissingContextLimitMinimum => "missing_context_limit_minimum",
        }
    }
}

/// Complete adaptive retained-tail policy resolved from one model context window.
/// 由单个模型上下文窗口解析出的完整自适应尾部保留策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextCompactionTailPolicyResolution {
    /// Raw model-declared context limit used by the formula.
    /// 公式使用的模型原始声明上下文上限。
    pub context_limit: Option<usize>,
    /// Nominal retained-tail token budget.
    /// 名义尾部保留 Token 预算。
    pub preserve_recent_tokens: usize,
    /// Maximum number of retained assistant/provider execution segments.
    /// 最大 Assistant/Provider 执行段保留数量。
    pub tail_segments: usize,
    /// Provenance of the resolved values.
    /// 已解析值的来源。
    pub source: ContextCompactionTailPolicySource,
}

/// Resolves the unique adaptive retained-tail policy from a raw model context limit.
/// 根据模型原始上下文上限解析唯一的自适应尾部保留策略。
///
/// # Parameters
/// # 参数
/// `context_limit` is the model-declared total context window without effective-window reductions.
/// `context_limit` 是未扣除有效窗口比例的模型声明总上下文窗口。
///
/// # Returns
/// # 返回值
/// A deterministic policy resolution using the minimum fallback when the declaration is absent.
/// 返回确定性策略解析结果；声明缺失时使用最小保底值。
pub fn resolve_context_compaction_tail_policy(
    context_limit: Option<usize>,
) -> ContextCompactionTailPolicyResolution {
    let Some(context_limit) = context_limit else {
        return ContextCompactionTailPolicyResolution {
            context_limit: None,
            preserve_recent_tokens: CONTEXT_COMPACTION_MIN_TAIL_TOKENS,
            tail_segments: CONTEXT_COMPACTION_MIN_TAIL_SEGMENTS,
            source: ContextCompactionTailPolicySource::MissingContextLimitMinimum,
        };
    };
    let window_bucket = context_limit / CONTEXT_COMPACTION_TAIL_WINDOW_STEP;
    ContextCompactionTailPolicyResolution {
        context_limit: Some(context_limit),
        preserve_recent_tokens: window_bucket
            .saturating_mul(CONTEXT_COMPACTION_TAIL_TOKEN_STEP)
            .clamp(
                CONTEXT_COMPACTION_MIN_TAIL_TOKENS,
                CONTEXT_COMPACTION_MAX_TAIL_TOKENS,
            ),
        tail_segments: window_bucket.clamp(
            CONTEXT_COMPACTION_MIN_TAIL_SEGMENTS,
            CONTEXT_COMPACTION_MAX_TAIL_SEGMENTS,
        ),
        source: ContextCompactionTailPolicySource::DeclaredContextLimit,
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

impl ContextBudgetSeverity {
    /// Returns the stable machine-facing key for this context-budget severity.
    /// 返回该上下文预算严重级别的稳定机器可读键。
    ///
    /// # Parameters
    /// 参数
    /// `self` is the resolved context-budget severity bucket.
    /// `self` 是已解析的上下文预算严重级别分桶。
    ///
    /// # Returns
    /// 返回值
    /// A stable kebab-case key suitable for serialized workbench state and CSS selectors.
    /// 返回适合序列化工作台状态与 CSS 选择器使用的稳定 kebab-case 键。
    pub const fn stable_key(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Warning => "warning",
            Self::CompactSoon => "compact-soon",
            Self::Overflow => "overflow",
        }
    }

    /// Returns the user-facing Chinese label for this context-budget severity.
    /// 返回该上下文预算严重级别面向用户的中文标签。
    ///
    /// # Parameters
    /// 参数
    /// `self` is the resolved context-budget severity bucket.
    /// `self` 是已解析的上下文预算严重级别分桶。
    ///
    /// # Returns
    /// 返回值
    /// A concise Chinese label used by diagnostics, previews, and workbench tooltips.
    /// 返回供诊断、预览与工作台提示浮层使用的简短中文标签。
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Healthy => "正常",
            Self::Warning => "预警",
            Self::CompactSoon => "建议压缩",
            Self::Overflow => "预算溢出",
        }
    }
}

/// Provenance and quality of the input-token value used for context admission.
/// 上下文准入所使用输入 Token 数值的来源与质量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ContextBudgetAccountingSource {
    /// The exact assembled request matches one Provider-reported input baseline.
    /// 精确装配请求与某个 Provider 报告的输入基线完全匹配。
    ProviderReported,
    /// A proven request prefix extends a Provider baseline with a character-level delta.
    /// 已证明的请求前缀在 Provider 基线上叠加字符级增量。
    ProviderAnchorPlusCharacterDelta,
    /// No valid Provider anchor exists and the complete request uses character estimation.
    /// 不存在有效 Provider 锚点，完整请求使用字符级估算。
    #[default]
    CharacterEstimate,
}

impl ContextBudgetAccountingSource {
    /// Returns the stable serialized key used by session accounting and diagnostics.
    /// 返回会话记账与诊断使用的稳定序列化键。
    pub const fn stable_key(self) -> &'static str {
        match self {
            Self::ProviderReported => "provider_reported",
            Self::ProviderAnchorPlusCharacterDelta => "provider_anchor_plus_character_delta",
            Self::CharacterEstimate => "character_estimate",
        }
    }

    /// Returns the concise Chinese label shown in Usage Tips.
    /// 返回用量提示中展示的简短中文标签。
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::ProviderReported => "Provider 实测",
            Self::ProviderAnchorPlusCharacterDelta => "Provider 锚点 + 字符增量",
            Self::CharacterEstimate => "字符粗估",
        }
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
    /// Session planning window after the configured availability percentage is applied.
    /// 应用配置的可用百分比后的会话规划窗口。
    pub effective_context_window: Option<usize>,
    /// Provider admission window derived from the real model limit, independent of the session node.
    /// 从真实模型上限推导的 Provider 准入窗口，不受会话节点影响。
    pub provider_request_hard_limit: Option<usize>,
    /// Automatic-compaction threshold resolved for the active turn.
    /// 为当前回合解析出的自动压缩阈值。
    pub auto_compact_limit: Option<usize>,
    /// Usage scope used to calculate the active context.
    /// 计算活动上下文使用的用量范围。
    pub auto_compact_scope: ContextWindowUsageScope,
    /// Absolute input-token baseline for the current BodyAfterPrefix compaction window.
    /// 当前 BodyAfterPrefix 压缩窗口使用的绝对输入 Token 基线。
    pub auto_compact_prefill_input_tokens: Option<usize>,
    /// Fallback buffer included only when the route supplied a fallback prompt.
    /// 仅当路由提供回退提示词时计入的回退缓冲区。
    pub fallback_buffer_tokens: usize,
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
    /// Active context tokens used for Codex window admission.
    /// 用于 Codex 窗口准入判断的活动上下文 Token 数。
    pub active_context_tokens: usize,
    /// Scope-specific context tokens used only for the automatic-compaction line.
    /// 仅用于自动压缩线的范围专属活动上下文 Token 数。
    pub auto_compact_context_tokens: usize,
    /// Estimated total end-to-end token footprint including reasoning and output reserves.
    /// 包含推理与输出预留在内的估算端到端 Token 占用总量。
    pub estimated_total_tokens: usize,
    /// Final usage ratio derived from the effective input limit.
    /// 根据生效输入上限推导出的最终占用比例。
    pub usage_ratio: f32,
    /// Optional latest observed provider usage snapshot reused to calibrate host estimates conservatively.
    /// 用于保守校准宿主估算的可选最近一次 Provider 实测用量快照。
    pub observed_usage: Option<ProviderUsageSnapshot>,
    /// Explicit source and quality of the input-token accounting value.
    /// 输入 Token 记账值的明确来源与质量。
    pub accounting_source: ContextBudgetAccountingSource,
    /// Stable warning code when the accounting value is not exact Provider evidence.
    /// 记账值不是精确 Provider 证据时使用的稳定警告码。
    pub accounting_warning_code: Option<String>,
    /// Physical Provider call used as the accounting anchor, when present.
    /// 存在时作为记账锚点的物理 Provider 调用。
    pub provider_anchor_call_id: Option<String>,
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
            effective_context_window: None,
            provider_request_hard_limit: None,
            auto_compact_limit: None,
            auto_compact_scope: ContextWindowUsageScope::Total,
            auto_compact_prefill_input_tokens: None,
            fallback_buffer_tokens: 0,
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
            active_context_tokens: 0,
            auto_compact_context_tokens: 0,
            estimated_total_tokens: 0,
            usage_ratio: 0.0,
            observed_usage: None,
            accounting_source: ContextBudgetAccountingSource::CharacterEstimate,
            accounting_warning_code: Some(String::from("provider_usage_baseline_unavailable")),
            provider_anchor_call_id: None,
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
        if let Some(limit) = self.effective_context_window {
            format!("{} / {}", self.active_context_tokens, limit)
        } else {
            format!("{} / 未声明上限", self.active_context_tokens)
        }
    }

    /// Builds one compact diagnostics note describing the current budget state.
    /// 构建一条描述当前预算状态的紧凑诊断说明。
    pub fn summary_note(&self) -> String {
        let severity_label = self.severity.display_label();
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
        let input_source_label = self.accounting_source.display_label();
        let breakdown_label = if self.observed_usage.is_some() {
            "本地估算拆分"
        } else {
            "估算拆分"
        };
        format!(
            "{severity_label} · {baseline_label}{} · 原始窗口 {} · 有效窗口 {} · 自动压缩线 {} · 输入来源 {} · {} · {} 系统 {} / 历史 {} / 当前 {} / 工具 {} · 活动上下文 {} · 自动压缩计量 {} · 端到端占用 {} · 输出预留 {} · 推理预留 {} · 旧压缩预留 {}{}{}",
            baseline_reason_suffix,
            context_limit_label,
            self.effective_context_window
                .map(|value| value.to_string())
                .unwrap_or_else(|| String::from("未声明")),
            self.auto_compact_limit
                .map(|value| value.to_string())
                .unwrap_or_else(|| String::from("未声明")),
            input_source_label,
            self.usage_label(),
            breakdown_label,
            self.estimated_system_tokens,
            self.estimated_history_tokens,
            self.estimated_prompt_tokens,
            self.estimated_tool_tokens,
            self.active_context_tokens,
            self.auto_compact_context_tokens,
            self.estimated_total_tokens,
            self.reserved_output_tokens,
            self.reserved_reasoning_tokens,
            self.reserved_compaction_tokens,
            self.observed_usage
                .as_ref()
                .map(|usage| format!(" · {}", provider_usage_label(usage)))
                .unwrap_or_default(),
            self.accounting_warning_code
                .as_ref()
                .map(|warning| format!(" · 警告 {warning}"))
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

#[cfg(test)]
mod tests {
    //! Shared context primitive tests.
    //! 共享上下文基础类型测试。

    use super::*;

    /// Verifies context-budget severity keys and labels stay centralized.
    /// 验证上下文预算严重级别键与标签保持集中定义。
    #[test]
    fn context_budget_severity_keys_and_labels_are_stable() {
        let cases = [
            (ContextBudgetSeverity::Healthy, "healthy", "正常"),
            (ContextBudgetSeverity::Warning, "warning", "预警"),
            (
                ContextBudgetSeverity::CompactSoon,
                "compact-soon",
                "建议压缩",
            ),
            (ContextBudgetSeverity::Overflow, "overflow", "预算溢出"),
        ];

        for (severity, key, label) in cases {
            assert_eq!(severity.stable_key(), key);
            assert_eq!(severity.display_label(), label);
        }
    }

    /// Verifies context-budget usage percent owns progress-bar clamping.
    /// 验证上下文预算占用百分比自身负责进度条上下界裁剪。
    #[test]
    fn context_budget_usage_percent_is_clamped() {
        let low = ContextBudgetSnapshot {
            usage_ratio: -0.25,
            ..ContextBudgetSnapshot::default()
        };
        let high = ContextBudgetSnapshot {
            usage_ratio: 1.25,
            ..ContextBudgetSnapshot::default()
        };

        assert_eq!(low.usage_percent(), 0.0);
        assert_eq!(high.usage_percent(), 100.0);
    }

    /// Verifies the compacting threshold cannot sit above the hard available window.
    /// 验证自动压缩阈值不能高于有效硬窗口。
    #[test]
    fn context_window_policy_rejects_compaction_above_hard_window() {
        let policy = ContextWindowPolicyOverrides {
            effective_context_window_percent: Some(95),
            auto_compact_percent: Some(96),
            ..ContextWindowPolicyOverrides::default()
        };

        let error = policy
            .validate()
            .expect_err("sparse policy must reject an impossible same-layer ordering");
        assert!(error.contains("cannot exceed"));

        let resolved = ContextWindowPolicy {
            effective_context_window_percent: 95,
            auto_compact_percent: 96,
            ..ContextWindowPolicy::default()
        };
        let error = resolved
            .validate()
            .expect_err("resolved policy must reject an impossible inherited ordering");
        assert!(error.contains("cannot exceed"));
    }

    /// Verifies every adaptive retained-tail formula boundary and cap.
    /// 验证自适应尾部保留公式的全部边界与上限。
    #[test]
    fn adaptive_tail_policy_resolves_declared_context_window_boundaries() {
        let cases = [
            (0usize, 5_000usize, 5usize),
            (32_000usize, 5_000usize, 5usize),
            (49_999, 5_000, 5),
            (50_000, 5_000, 5),
            (99_999, 5_000, 5),
            (100_000, 5_000, 5),
            (199_999, 7_500, 5),
            (200_000, 10_000, 5),
            (249_999, 10_000, 5),
            (250_000, 12_500, 5),
            (299_999, 12_500, 5),
            (300_000, 15_000, 6),
            (499_999, 22_500, 9),
            (500_000, 25_000, 10),
            (999_999, 47_500, 19),
            (1_000_000, 50_000, 20),
            (1_050_000, 50_000, 20),
            (2_000_000, 50_000, 20),
            (usize::MAX, 50_000, 20),
        ];

        for (context_limit, expected_tokens, expected_segments) in cases {
            let resolution = resolve_context_compaction_tail_policy(Some(context_limit));
            assert_eq!(resolution.context_limit, Some(context_limit));
            assert_eq!(resolution.preserve_recent_tokens, expected_tokens);
            assert_eq!(resolution.tail_segments, expected_segments);
            assert_eq!(
                resolution.source,
                ContextCompactionTailPolicySource::DeclaredContextLimit
            );
        }
    }

    /// Verifies a missing context declaration uses the explicit 5K and five-segment fallback.
    /// 验证上下文声明缺失时使用明确的 5K 与五段保底。
    #[test]
    fn adaptive_tail_policy_uses_minimum_for_missing_context_window() {
        let resolution = resolve_context_compaction_tail_policy(None);
        assert_eq!(resolution.context_limit, None);
        assert_eq!(resolution.preserve_recent_tokens, 5_000);
        assert_eq!(resolution.tail_segments, 5);
        assert_eq!(
            resolution.source,
            ContextCompactionTailPolicySource::MissingContextLimitMinimum
        );
        assert_eq!(resolution.source.label(), "missing_context_limit_minimum");
        assert_eq!(
            ContextCompactionTailPolicySource::DeclaredContextLimit.label(),
            "declared_context_limit"
        );
        assert_eq!(ContextCompactionPolicy::default().tail_turns, 5);
        assert_eq!(
            ContextCompactionPolicy::default().preserve_recent_tokens,
            5_000
        );
    }
}
