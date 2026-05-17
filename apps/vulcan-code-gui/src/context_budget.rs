//! Context budget estimation helpers shared by execution preparation, diagnostics, and compaction triggers.
//! 供执行准备、诊断与压缩触发共享的上下文预算估算辅助模块。

use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
use crate::provider_capabilities::ResolvedModelCapabilities;
use crate::provider_config::{ProviderApiFamily, ReasoningEffortLevel};
use crate::provider_usage::{calibrated_input_token_estimate, provider_usage_label};
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema, ProviderUsageSnapshot,
};

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
            tail_turns: 4,
            preserve_recent_tokens: 8_192,
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
#[derive(Debug, Clone, Default)]
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
        let context_limit_label = self
            .context_limit
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("未声明"));
        format!(
            "{severity_label} · 上下文窗 {} · 输入 {} · 系统 {} / 历史 {} / 当前 {} / 工具 {} · 总占用 {} · 输出预留 {} · 推理预留 {} · 压缩预留 {}{}",
            context_limit_label,
            self.usage_label(),
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

/// Builds one exact request-facing budget snapshot from the final provider request snapshot.
/// 根据最终 Provider 请求快照构建一份精确面向请求的预算快照。
pub fn build_request_context_budget_snapshot(
    provider_id: &str,
    model_id: &str,
    api_family: ProviderApiFamily,
    capabilities: &ResolvedModelCapabilities,
    reasoning: &ProviderRuntimeReasoningResolution,
    request_snapshot: &CoreProviderRequestSnapshot,
    observed_usage: Option<&ProviderUsageSnapshot>,
) -> ContextBudgetSnapshot {
    let reserved_reasoning_tokens = reasoning_token_reserve(reasoning.requested_effort);
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
            reasoning.requested_effort,
            request_snapshot.tool_schemas.len(),
            None,
        ),
        context_limit: capabilities.context_limit,
        effective_input_limit,
        reserved_output_tokens,
        reserved_reasoning_tokens,
        reserved_compaction_tokens,
        compaction_auto_enabled: false,
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
    api_family: ProviderApiFamily,
    capabilities: &ResolvedModelCapabilities,
    reasoning_effort: ReasoningEffortLevel,
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
    api_family: ProviderApiFamily,
    capabilities: &ResolvedModelCapabilities,
    reasoning_effort: ReasoningEffortLevel,
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
                "{}:{:?}:{}",
                tool.kind.key(),
                tool.policy,
                tool.native_tool_type.as_deref().unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}|{}|{}|ctx={:?}|in={:?}|out={:?}|tool={}|temp={}|attach={}|reason={}|it={}|ot={}|tools={}|ai={}|compact={}",
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

/// Estimates the token footprint of one canonical conversation message.
/// 估算单条规范会话消息的 Token 占用。
fn estimate_core_message_tokens(message: &CoreConversationMessage) -> usize {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            estimate_text_tokens(text)
                + attachments
                    .iter()
                    .map(estimate_conversation_attachment_tokens)
                    .sum::<usize>()
                + 8usize
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
                            + estimate_json_tokens(&tool_call.arguments)
                            + 24usize
                    })
                    .sum::<usize>()
                + 12usize
        }
        CoreConversationPayload::ToolResult { text, .. } => estimate_text_tokens(text) + 12usize,
    }
}

/// Estimates the token footprint of one structured user attachment using a conservative host-side reserve.
/// 使用保守宿主侧预留估算单条结构化用户附件的 Token 占用。
fn estimate_conversation_attachment_tokens(
    attachment: &vulcan_core::types::ConversationAttachment,
) -> usize {
    if matches!(
        crate::attachment_asset_index::resolve_attachment_replay_status(attachment),
        crate::attachment_asset_index::AttachmentReplayStatus::Purged
            | crate::attachment_asset_index::AttachmentReplayStatus::Missing
    ) {
        return 0usize;
    }

    let (kind_reserve, byte_divisor) = match attachment.kind {
        vulcan_core::types::ConversationAttachmentKind::Audio => (1_536usize, 768u64),
        vulcan_core::types::ConversationAttachmentKind::Image => (768usize, 1_024u64),
        vulcan_core::types::ConversationAttachmentKind::Video => (4_096usize, 512u64),
        vulcan_core::types::ConversationAttachmentKind::Pdf => (2_048usize, 896u64),
    };
    let byte_reserve = attachment
        .byte_len
        .map(|byte_len| ((byte_len / byte_divisor) as usize).min(kind_reserve.saturating_mul(4)))
        .unwrap_or_default();

    kind_reserve
        + byte_reserve
        + estimate_text_tokens(&attachment.file_name)
        + estimate_text_tokens(&attachment.mime_type)
        + 24usize
}

/// Estimates the token footprint of one provider-visible tool schema.
/// 估算单条 Provider 可见工具 schema 的 Token 占用。
fn estimate_tool_schema_tokens(definition: &CoreProviderRequestToolSchema) -> usize {
    estimate_text_tokens(&definition.id)
        + estimate_text_tokens(&definition.name)
        + estimate_text_tokens(&definition.description)
        + estimate_json_tokens(&definition.parameters_schema)
        + 32usize
}

/// Estimates the token footprint of one text payload using a lightweight character heuristic.
/// 使用轻量字符启发式估算单段文本载荷的 Token 占用。
fn estimate_text_tokens(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    let characters = trimmed.chars().count();
    ((characters + 3usize) / 4usize).max(1usize)
}

/// Estimates the token footprint of one JSON payload by serializing it into a compact string first.
/// 先将 JSON 载荷序列化为紧凑字符串，再估算它的 Token 占用。
fn estimate_json_tokens(value: &serde_json::Value) -> usize {
    serde_json::to_string(value)
        .map(|serialized| estimate_text_tokens(&serialized))
        .unwrap_or(0usize)
}

/// Resolves the output reserve that should be held back for provider-visible model output.
/// 解析应为 Provider 可见模型输出预留的输出预算。
fn resolve_output_token_reserve(capabilities: &ResolvedModelCapabilities) -> usize {
    capabilities
        .effective_output_limit()
        .unwrap_or(4096usize)
        .max(512usize)
}

/// Resolves the reasoning reserve that should be held back for models running explicit thinking modes.
/// 解析应为显式思考模式模型预留的推理预算。
fn reasoning_token_reserve(effort: ReasoningEffortLevel) -> usize {
    match effort {
        ReasoningEffortLevel::Off => 0usize,
        ReasoningEffortLevel::Low => 1024usize,
        ReasoningEffortLevel::Medium => 2048usize,
        ReasoningEffortLevel::High => 4096usize,
        ReasoningEffortLevel::XHigh => 8192usize,
    }
}

#[cfg(test)]
mod tests {
    //! Context-budget regression tests covering exact request estimation and compaction thresholds.
    //! 覆盖精确请求估算与压缩阈值的上下文预算回归测试。

    use super::{
        CONTEXT_BUDGET_COMPACTION_RATIO, ContextBudgetSeverity, ContextCompactionPolicy,
        build_context_budget_fingerprint, build_projected_session_context_budget_snapshot,
        build_request_context_budget_snapshot,
    };
    use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
    use crate::provider_capabilities::ResolvedModelCapabilities;
    use crate::provider_config::{ProviderApiFamily, ReasoningEffortLevel};
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use vulcan_core::types::{
        ConversationAttachment, ConversationAttachmentKind, ConversationMessage,
        ConversationPayload, ProviderContinuation, ProviderRequestSnapshot,
        ProviderRequestToolSchema, ProviderUsageSnapshot,
    };

    /// Owns one temporary attachment file so replay-aware budget tests exercise available attachments instead of missing placeholders.
    /// 持有一个临时附件文件，使感知回放状态的预算测试覆盖可用附件，而不是缺失占位路径。
    struct TemporaryAttachmentFile {
        /// Absolute path to the temporary attachment file retained until drop.
        /// 临时附件文件在释放前保留的绝对路径。
        path: PathBuf,
    }

    impl TemporaryAttachmentFile {
        /// Creates one temporary attachment file with the requested byte length.
        /// 按请求的字节长度创建一个临时附件文件。
        fn new(file_name: &str, byte_len: usize) -> Self {
            let unique_id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("vulcan-context-budget-{unique_id}-{file_name}"));
            std::fs::write(&path, vec![0_u8; byte_len])
                .expect("temporary attachment file should write");
            Self { path }
        }

        /// Builds one canonical attachment record pointing at the live temporary file.
        /// 构建一条指向当前临时文件的规范附件记录。
        fn to_attachment(&self, file_name: &str, byte_len: u64) -> ConversationAttachment {
            ConversationAttachment {
                kind: ConversationAttachmentKind::Pdf,
                file_name: String::from(file_name),
                local_path: self.path.to_string_lossy().into_owned(),
                source_path: None,
                asset_id: Some(format!("asset-{file_name}")),
                byte_len: Some(byte_len),
                mime_type: String::from("application/pdf"),
            }
        }
    }

    impl Drop for TemporaryAttachmentFile {
        /// Removes the temporary attachment file when the test fixture leaves scope.
        /// 当测试夹具离开作用域时移除临时附件文件。
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    /// Builds one minimal capability snapshot for budget tests.
    /// 为预算测试构建一份最小能力快照。
    fn test_capabilities() -> ResolvedModelCapabilities {
        ResolvedModelCapabilities {
            supports_tool_call: true,
            supports_temperature: true,
            context_limit: Some(8_000),
            input_limit: Some(6_000),
            output_limit: Some(1_024),
            ..ResolvedModelCapabilities::default()
        }
    }

    /// Uses the exact request snapshot to produce a stable fingerprint and non-zero budget estimate.
    /// 使用精确请求快照产出稳定指纹与非零预算估算结果。
    #[test]
    fn request_context_budget_snapshot_uses_exact_request_state() {
        let capabilities = test_capabilities();
        let request_snapshot = ProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::from("You are a precise coding assistant."),
            prompt: String::from("Summarize the latest changes."),
            history: vec![ConversationMessage {
                id: String::from("m1"),
                payload: ConversationPayload::UserText {
                    text: String::from("Inspect src/main.rs"),
                    attachments: Vec::new(),
                },
            }],
            temperature: None,
            max_turns: 8,
            tool_schemas: vec![ProviderRequestToolSchema {
                id: String::from("native_workspace_describe"),
                name: String::from("native_workspace_describe"),
                description: String::from("Describe the current workspace."),
                source_label: String::from("native"),
                parameters_schema: json!({"type":"object","properties":{}}),
            }],
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        };

        let snapshot = build_request_context_budget_snapshot(
            "openai",
            "gpt-test",
            ProviderApiFamily::OpenAiResponses,
            &capabilities,
            &ProviderRuntimeReasoningResolution {
                requested_effort: ReasoningEffortLevel::Low,
                ..ProviderRuntimeReasoningResolution::default()
            },
            &request_snapshot,
            None,
        );

        assert!(snapshot.estimated_input_tokens > 0);
        assert!(snapshot.fingerprint.contains("openai|gpt-test"));
    }

    /// Uses the latest observed usage snapshot as a conservative floor for subsequent request estimates.
    /// 使用最近一次实测用量快照作为后续请求估算的保守下限。
    #[test]
    fn request_context_budget_snapshot_uses_observed_usage_as_floor() {
        let capabilities = test_capabilities();
        let request_snapshot = ProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::from("system"),
            prompt: String::from("prompt"),
            history: Vec::new(),
            temperature: None,
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        };
        let observed_usage = ProviderUsageSnapshot {
            input_tokens: Some(2048),
            output_tokens: Some(256),
            reasoning_tokens: Some(128),
            total_tokens: Some(2432),
        };

        let snapshot = build_request_context_budget_snapshot(
            "openai",
            "gpt-test",
            ProviderApiFamily::OpenAiResponses,
            &capabilities,
            &ProviderRuntimeReasoningResolution::default(),
            &request_snapshot,
            Some(&observed_usage),
        );

        assert_eq!(snapshot.observed_usage, Some(observed_usage.clone()));
        assert_eq!(snapshot.estimated_input_tokens, 2048);
        assert!(snapshot.summary_note().contains("最近实测 输入 2048"));
    }

    /// Promotes large session histories into the compaction bucket when the estimated ratio crosses the configured threshold.
    /// 当估算占用比例超过配置阈值时，将大型会话历史提升到需要压缩的分桶。
    #[test]
    fn session_context_budget_snapshot_reaches_compaction_threshold() {
        let capabilities = ResolvedModelCapabilities {
            context_limit: Some(4_000),
            input_limit: Some(1_000),
            output_limit: Some(512),
            ..ResolvedModelCapabilities::default()
        };
        let messages = vec![ConversationMessage {
            id: String::from("message-1"),
            payload: ConversationPayload::Assistant {
                text: "A".repeat(4_500),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                tool_calls: Vec::new(),
                usage: None,
            },
        }];

        let snapshot = build_projected_session_context_budget_snapshot(
            "deepseek",
            "deepseek-v4-pro",
            ProviderApiFamily::OpenAiChatCompatible,
            &capabilities,
            ReasoningEffortLevel::Off,
            ContextCompactionPolicy::default(),
            &messages,
            0usize,
            None,
        );

        assert!(snapshot.usage_ratio >= CONTEXT_BUDGET_COMPACTION_RATIO);
        assert!(matches!(
            snapshot.severity,
            ContextBudgetSeverity::CompactSoon | ContextBudgetSeverity::Overflow
        ));
    }

    /// Verifies compaction policy auto and reserved-budget fields affect session-level compaction decisions.
    /// 验证压缩策略的自动开关与预留预算会影响会话级压缩决策。
    #[test]
    fn session_context_budget_snapshot_respects_compaction_policy() {
        let capabilities = ResolvedModelCapabilities {
            context_limit: Some(4_000),
            input_limit: Some(1_000),
            output_limit: Some(512),
            ..ResolvedModelCapabilities::default()
        };
        let messages = vec![ConversationMessage {
            id: String::from("message-1"),
            payload: ConversationPayload::Assistant {
                text: "A".repeat(4_500),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                tool_calls: Vec::new(),
                usage: None,
            },
        }];
        let policy = ContextCompactionPolicy {
            auto: false,
            reserved: 200,
            ..ContextCompactionPolicy::default()
        };

        let snapshot = build_projected_session_context_budget_snapshot(
            "deepseek",
            "deepseek-v4-pro",
            ProviderApiFamily::OpenAiChatCompatible,
            &capabilities,
            ReasoningEffortLevel::Off,
            policy,
            &messages,
            0usize,
            None,
        );

        assert_eq!(snapshot.effective_input_limit, Some(800));
        assert_eq!(snapshot.reserved_compaction_tokens, 200);
        assert!(
            snapshot.estimated_total_tokens
                >= snapshot.estimated_input_tokens + snapshot.reserved_compaction_tokens
        );
        assert!(!snapshot.should_compact());
        assert!(matches!(
            snapshot.severity,
            ContextBudgetSeverity::CompactSoon | ContextBudgetSeverity::Overflow
        ));
    }

    /// Keeps the fingerprint stable for identical budget inputs so model-switch reconciliation can compare snapshots reliably.
    /// 在预算输入相同时保持指纹稳定，以便模型切换对齐逻辑可靠比较快照。
    #[test]
    fn context_budget_fingerprint_is_stable_for_identical_inputs() {
        let capabilities = test_capabilities();

        let left = build_context_budget_fingerprint(
            "openai",
            "gpt-test",
            ProviderApiFamily::OpenAiResponses,
            &capabilities,
            ReasoningEffortLevel::Medium,
            3usize,
            None,
        );
        let right = build_context_budget_fingerprint(
            "openai",
            "gpt-test",
            ProviderApiFamily::OpenAiResponses,
            &capabilities,
            ReasoningEffortLevel::Medium,
            3usize,
            None,
        );

        assert_eq!(left, right);
    }

    /// Scales attachment token estimation with stored byte length so larger multimodal payloads become visibly more expensive than tiny placeholders.
    /// 依据已存储字节长度放大附件 Token 估算，使更大的多模态载荷显著高于微小占位附件。
    #[test]
    fn request_context_budget_snapshot_accounts_for_attachment_byte_length() {
        let capabilities = test_capabilities();
        let small_file = TemporaryAttachmentFile::new("small.pdf", 2_048);
        let large_file = TemporaryAttachmentFile::new("large.pdf", 2_048_000);
        let small_attachment = small_file.to_attachment("small.pdf", 2_048);
        let large_attachment = large_file.to_attachment("large.pdf", 2_048_000);
        let small_snapshot = ProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::new(),
            prompt: String::from("请阅读附件"),
            history: vec![ConversationMessage {
                id: String::from("m1"),
                payload: ConversationPayload::UserText {
                    text: String::from("请阅读附件"),
                    attachments: vec![small_attachment],
                },
            }],
            temperature: None,
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        };
        let large_snapshot = ProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::new(),
            prompt: String::from("请阅读附件"),
            history: vec![ConversationMessage {
                id: String::from("m1"),
                payload: ConversationPayload::UserText {
                    text: String::from("请阅读附件"),
                    attachments: vec![large_attachment],
                },
            }],
            temperature: None,
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        };

        let small_budget = build_request_context_budget_snapshot(
            "openai",
            "gpt-test",
            ProviderApiFamily::OpenAiResponses,
            &capabilities,
            &ProviderRuntimeReasoningResolution::default(),
            &small_snapshot,
            None,
        );
        let large_budget = build_request_context_budget_snapshot(
            "openai",
            "gpt-test",
            ProviderApiFamily::OpenAiResponses,
            &capabilities,
            &ProviderRuntimeReasoningResolution::default(),
            &large_snapshot,
            None,
        );

        assert!(large_budget.estimated_history_tokens > small_budget.estimated_history_tokens);
    }
}
