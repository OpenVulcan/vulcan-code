//! Context request send-mode diagnostics shared by runtime preparation and UI previews.
//! 供运行时准备与界面预览共享的上下文请求发送模式诊断能力。

use crate::{ContextAssembly, ContextBaseline, ContextBudgetSnapshot};

/// Immutable facts used to decide the context send mode for one provider request.
/// 用于判断单次 Provider 请求上下文发送模式的不可变事实集合。
#[derive(Debug, Clone, Copy)]
pub struct ContextRequestModeInput<'a> {
    /// Whether this request is an internal host task instead of a normal user turn.
    /// 该请求是否为宿主内部任务而不是普通用户回合。
    pub internal_request: bool,
    /// Current provider identifier.
    /// 当前 Provider 标识。
    pub provider_id: &'a str,
    /// Current protocol profile identifier.
    /// 当前协议配置标识。
    pub protocol_profile_id: &'a str,
    /// Current model identifier.
    /// 当前模型标识。
    pub model_id: &'a str,
    /// Number of provider-visible history messages attached to the request.
    /// 附加到该请求的 Provider 可见历史消息数量。
    pub history_message_count: usize,
    /// Current layered context assembly.
    /// 当前分层上下文装配结果。
    pub context_assembly: Option<&'a ContextAssembly>,
    /// Current stable context baseline derived from the layered assembly.
    /// 从分层装配结果派生出的当前稳定上下文基线。
    pub context_baseline: Option<&'a ContextBaseline>,
    /// Current context-budget snapshot after anchor validation.
    /// 经过锚点校验后的当前上下文预算快照。
    pub context_budget: Option<&'a ContextBudgetSnapshot>,
    /// Provider identifier recorded in the continuation cursor.
    /// continuation 游标中记录的 Provider 标识。
    pub continuation_provider_id: Option<&'a str>,
    /// Protocol profile identifier recorded in the continuation cursor.
    /// continuation 游标中记录的协议配置标识。
    pub continuation_protocol_profile_id: Option<&'a str>,
    /// Model identifier recorded in the continuation cursor.
    /// continuation 游标中记录的模型标识。
    pub continuation_model_id: Option<&'a str>,
    /// Latest provider-visible assistant message identifier recorded in the continuation cursor.
    /// continuation 游标中记录的最近一次 Provider 可见助手消息标识。
    pub continuation_message_id: Option<&'a str>,
}

/// Final context send mode selected for one provider request.
/// 单次 Provider 请求最终选择的上下文发送模式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextRequestMode {
    /// The request must keep using the full context payload for normal safe reasons.
    /// 该请求必须因正常安全原因继续使用完整上下文载荷。
    Full {
        /// Structured reason explaining why full context is required.
        /// 解释为何需要完整上下文的结构化原因。
        reason: ContextFullRequestReason,
    },
    /// The request is a candidate for future incremental sending, but still sends full context today.
    /// 该请求是未来增量发送候选，但当前仍然发送完整上下文。
    IncrementalCandidate {
        /// Stable baseline fingerprint that can anchor a future incremental request.
        /// 可用于锚定未来增量请求的稳定基线指纹。
        baseline_fingerprint: String,
        /// Provider-visible continuation message identifier available for request chaining.
        /// 可用于请求串联的 Provider 可见 continuation 消息标识。
        continuation_message_id: String,
        /// Number of provider-visible history messages in the current full fallback payload.
        /// 当前完整回退载荷中的 Provider 可见历史消息数量。
        history_message_count: usize,
    },
    /// Incremental sending is currently unsafe and must stay disabled.
    /// 当前增量发送不安全，必须保持禁用。
    IncrementalDisabled {
        /// Structured reason explaining why incremental sending is disabled.
        /// 解释为何禁用增量发送的结构化原因。
        reason: ContextIncrementalDisabledReason,
    },
}

/// Normal full-request reason that does not indicate a broken baseline.
/// 不代表基线损坏的正常完整请求原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextFullRequestReason {
    /// The request has not produced a layered context assembly yet.
    /// 该请求尚未生成分层上下文装配结果。
    AssemblyNotReady,
    /// The request has no stable baseline yet.
    /// 该请求尚未生成稳定基线。
    BaselineNotReady,
    /// The provider continuation cursor is missing.
    /// Provider continuation 游标缺失。
    ProviderContinuationMissing,
    /// The request has no provider-visible history and behaves like an initial turn.
    /// 该请求没有 Provider 可见历史，表现为初始回合。
    InitialTurn,
}

/// Unsafe incremental-disable reason that should remain visible to diagnostics.
/// 应保持诊断可见的不安全增量禁用原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextIncrementalDisabledReason {
    /// Internal host requests should not participate in user-session incremental chaining.
    /// 宿主内部请求不应参与用户会话的增量串联。
    InternalRequest,
    /// The validated compaction anchor no longer matches the active runtime shape.
    /// 已校验的压缩锚点不再匹配当前运行时形态。
    BaselineMismatch {
        /// Human-readable mismatch detail captured by the budget snapshot.
        /// 预算快照捕获的人类可读失配详情。
        detail: Option<String>,
    },
    /// The continuation cursor belongs to another provider, protocol profile, or model.
    /// continuation 游标属于另一个 Provider、协议配置或模型。
    ProviderContinuationMismatch {
        /// Provider identifier stored on the continuation cursor.
        /// continuation 游标中保存的 Provider 标识。
        continuation_provider_id: Option<String>,
        /// Protocol profile identifier stored on the continuation cursor.
        /// continuation 游标中保存的协议配置标识。
        continuation_protocol_profile_id: Option<String>,
        /// Model identifier stored on the continuation cursor.
        /// continuation 游标中保存的模型标识。
        continuation_model_id: Option<String>,
    },
}

impl ContextRequestMode {
    /// Returns the compact Chinese label used by UI summaries.
    /// 返回供界面摘要使用的紧凑中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Full { .. } => "全量上下文",
            Self::IncrementalCandidate { .. } => "可增量候选",
            Self::IncrementalDisabled { .. } => "增量禁用",
        }
    }

    /// Returns one human-readable diagnostics sentence for previews and tooltips.
    /// 返回供预览和提示层使用的一条人类可读诊断语句。
    pub fn summary_note(&self) -> String {
        match self {
            Self::Full { reason } => format!(
                "{}：{}。实际发送仍为完整上下文。",
                self.label(),
                reason.label()
            ),
            Self::IncrementalCandidate {
                baseline_fingerprint,
                continuation_message_id,
                history_message_count,
            } => format!(
                "{}：基线 `{}` 与 continuation `{}` 可用于后续增量；当前仍安全全量发送 {} 条历史。",
                self.label(),
                baseline_fingerprint,
                continuation_message_id,
                history_message_count
            ),
            Self::IncrementalDisabled { reason } => format!(
                "{}：{}。实际发送回退为完整上下文。",
                self.label(),
                reason.label()
            ),
        }
    }

    /// Returns a short diagnostics sentence for compact UI surfaces.
    /// 返回供紧凑界面区域使用的短诊断语句。
    pub fn compact_note(&self) -> String {
        match self {
            Self::Full { reason } => format!("{} · {}", self.label(), reason.label()),
            Self::IncrementalCandidate {
                history_message_count,
                ..
            } => format!(
                "{} · 当前仍全量发送 {} 条历史",
                self.label(),
                history_message_count
            ),
            Self::IncrementalDisabled { reason } => {
                format!("{} · {}", self.label(), reason.label())
            }
        }
    }

    /// Returns whether this request is a candidate for future incremental provider payloads.
    /// 返回该请求是否可作为未来 Provider 增量载荷候选。
    pub const fn is_incremental_candidate(&self) -> bool {
        matches!(self, Self::IncrementalCandidate { .. })
    }
}

impl ContextFullRequestReason {
    /// Returns a compact Chinese diagnostics label for this full-request reason.
    /// 返回该完整请求原因的紧凑中文诊断标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::AssemblyNotReady => "上下文层尚未就绪",
            Self::BaselineNotReady => "上下文基线尚未就绪",
            Self::ProviderContinuationMissing => "Provider continuation 尚未建立",
            Self::InitialTurn => "初始回合无需增量",
        }
    }
}

impl ContextIncrementalDisabledReason {
    /// Returns a compact Chinese diagnostics label for this disable reason.
    /// 返回该禁用原因的紧凑中文诊断标签。
    pub fn label(&self) -> String {
        match self {
            Self::InternalRequest => String::from("内部请求不参与增量上下文"),
            Self::BaselineMismatch { detail } => detail
                .as_deref()
                .map(|value| format!("基线失配：{value}"))
                .unwrap_or_else(|| String::from("基线失配")),
            Self::ProviderContinuationMismatch {
                continuation_provider_id,
                continuation_protocol_profile_id,
                continuation_model_id,
            } => format!(
                "continuation 属于 {}/{}/{}",
                continuation_provider_id.as_deref().unwrap_or("?"),
                continuation_protocol_profile_id.as_deref().unwrap_or("?"),
                continuation_model_id.as_deref().unwrap_or("?")
            ),
        }
    }
}

/// Resolves the context send mode for one provider request from stable runtime facts.
/// 根据稳定运行时事实解析单次 Provider 请求的上下文发送模式。
pub fn resolve_context_request_mode(input: ContextRequestModeInput<'_>) -> ContextRequestMode {
    if input.internal_request {
        return ContextRequestMode::IncrementalDisabled {
            reason: ContextIncrementalDisabledReason::InternalRequest,
        };
    }

    let Some(context_assembly) = input.context_assembly else {
        return ContextRequestMode::Full {
            reason: ContextFullRequestReason::AssemblyNotReady,
        };
    };
    if context_assembly.is_empty() {
        return ContextRequestMode::Full {
            reason: ContextFullRequestReason::AssemblyNotReady,
        };
    }

    let Some(context_baseline) = input.context_baseline else {
        return ContextRequestMode::Full {
            reason: ContextFullRequestReason::BaselineNotReady,
        };
    };

    if let Some(context_budget) = input.context_budget
        && !context_budget.baseline_matches_active_runtime
    {
        return ContextRequestMode::IncrementalDisabled {
            reason: ContextIncrementalDisabledReason::BaselineMismatch {
                detail: context_budget.baseline_mismatch_reason.clone(),
            },
        };
    }

    if input.history_message_count == 0 && input.continuation_message_id.is_none() {
        return ContextRequestMode::Full {
            reason: ContextFullRequestReason::InitialTurn,
        };
    }

    let Some(continuation_message_id) = input.continuation_message_id else {
        return ContextRequestMode::Full {
            reason: ContextFullRequestReason::ProviderContinuationMissing,
        };
    };

    let continuation_matches_provider = input.continuation_provider_id == Some(input.provider_id)
        && input.continuation_protocol_profile_id == Some(input.protocol_profile_id)
        && input.continuation_model_id == Some(input.model_id);
    if !continuation_matches_provider {
        return ContextRequestMode::IncrementalDisabled {
            reason: ContextIncrementalDisabledReason::ProviderContinuationMismatch {
                continuation_provider_id: input.continuation_provider_id.map(str::to_string),
                continuation_protocol_profile_id: input
                    .continuation_protocol_profile_id
                    .map(str::to_string),
                continuation_model_id: input.continuation_model_id.map(str::to_string),
            },
        };
    }

    ContextRequestMode::IncrementalCandidate {
        baseline_fingerprint: context_baseline.fingerprint.clone(),
        continuation_message_id: continuation_message_id.to_string(),
        history_message_count: input.history_message_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ContextBudgetSeverity, ContextLayer, ContextLayerKind, build_context_budget_fingerprint,
    };

    /// Builds one minimal context assembly for request-mode tests.
    /// 为请求模式测试构建一个最小上下文装配。
    fn test_assembly() -> ContextAssembly {
        let mut assembly = ContextAssembly::new();
        assembly.push(ContextLayer::new(
            ContextLayerKind::System,
            "System",
            "You are VulcanCode.",
        ));
        assembly
    }

    /// Builds one default request-mode input with a matching continuation cursor.
    /// 构建一个带匹配 continuation 游标的默认请求模式输入。
    fn test_input<'a>(
        assembly: &'a ContextAssembly,
        baseline: &'a ContextBaseline,
        budget: Option<&'a ContextBudgetSnapshot>,
    ) -> ContextRequestModeInput<'a> {
        ContextRequestModeInput {
            internal_request: false,
            provider_id: "openai",
            protocol_profile_id: "openai-responses",
            model_id: "gpt-5",
            history_message_count: 2,
            context_assembly: Some(assembly),
            context_baseline: Some(baseline),
            context_budget: budget,
            continuation_provider_id: Some("openai"),
            continuation_protocol_profile_id: Some("openai-responses"),
            continuation_model_id: Some("gpt-5"),
            continuation_message_id: Some("msg-1"),
        }
    }

    /// Verifies stable baseline and matching continuation produce an incremental candidate.
    /// 验证稳定基线和匹配 continuation 会产生增量候选。
    #[test]
    fn request_mode_uses_incremental_candidate_when_baseline_and_continuation_match() {
        let assembly = test_assembly();
        let baseline = ContextBaseline::from_assembly(&assembly);

        let mode = resolve_context_request_mode(test_input(&assembly, &baseline, None));

        assert!(mode.is_incremental_candidate());
        assert!(mode.summary_note().contains("可增量候选"));
    }

    /// Verifies missing continuation keeps the request in safe full mode.
    /// 验证缺失 continuation 时请求保持安全全量模式。
    #[test]
    fn request_mode_uses_full_when_provider_continuation_is_missing() {
        let assembly = test_assembly();
        let baseline = ContextBaseline::from_assembly(&assembly);
        let mut input = test_input(&assembly, &baseline, None);
        input.continuation_message_id = None;

        let mode = resolve_context_request_mode(input);

        assert_eq!(
            mode,
            ContextRequestMode::Full {
                reason: ContextFullRequestReason::ProviderContinuationMissing,
            }
        );
    }

    /// Verifies a protocol-profile switch disables incremental continuation even when provider and model match.
    /// 验证即使供应商与模型相同，协议配置切换也会禁用增量 continuation。
    #[test]
    fn request_mode_disables_incremental_when_protocol_profile_changes() {
        let assembly = test_assembly();
        let baseline = ContextBaseline::from_assembly(&assembly);
        let mut input = test_input(&assembly, &baseline, None);
        input.protocol_profile_id = "anthropic-messages";

        let mode = resolve_context_request_mode(input);

        assert_eq!(
            mode,
            ContextRequestMode::IncrementalDisabled {
                reason: ContextIncrementalDisabledReason::ProviderContinuationMismatch {
                    continuation_provider_id: Some(String::from("openai")),
                    continuation_protocol_profile_id: Some(String::from("openai-responses")),
                    continuation_model_id: Some(String::from("gpt-5")),
                },
            }
        );
    }

    /// Verifies anchor mismatch disables incremental sending instead of presenting it as a normal full turn.
    /// 验证锚点失配会禁用增量发送，而不是把它展示为普通全量回合。
    #[test]
    fn request_mode_disables_incremental_when_budget_baseline_mismatches() {
        let assembly = test_assembly();
        let baseline = ContextBaseline::from_assembly(&assembly);
        let budget = ContextBudgetSnapshot {
            fingerprint: build_context_budget_fingerprint(
                "openai",
                "gpt-5",
                "openai.responses.v1",
                &crate::ContextBudgetCapabilities::default(),
                crate::ContextReasoningEffort::None,
                0,
                None,
            ),
            baseline_matches_active_runtime: false,
            baseline_mismatch_reason: Some(String::from("预算指纹变化")),
            severity: ContextBudgetSeverity::Healthy,
            ..ContextBudgetSnapshot::default()
        };

        let mode = resolve_context_request_mode(test_input(&assembly, &baseline, Some(&budget)));

        assert_eq!(
            mode,
            ContextRequestMode::IncrementalDisabled {
                reason: ContextIncrementalDisabledReason::BaselineMismatch {
                    detail: Some(String::from("预算指纹变化")),
                },
            }
        );
    }
}
