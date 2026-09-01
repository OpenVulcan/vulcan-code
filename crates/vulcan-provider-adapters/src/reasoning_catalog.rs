//! Evidence-backed reasoning controls for fixed provider catalogs.
//! 固定供应商目录的证据化思考控制。

use vulcan_ai_contract::{CapabilitySupport, ReasoningEffort};

/// Identifies one fixed provider whose model endpoint omits reasoning controls.
/// 标识模型端点未返回思考控制信息的固定供应商。
///
/// Every variant remains reachable through the test-only equivalence oracle that compares
/// migrated manifest rules against this legacy table; only `OpenAi` is still referenced by
/// the production degradation path.
/// 每个变体仍通过比较迁移清单规则与旧版表的仅测试等价 oracle 可达；生产退化路径仅引用 `OpenAi`。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixedReasoningProvider {
    /// Official Anthropic Messages API.
    /// Anthropic 官方 Messages API。
    Anthropic,
    /// Official Google Gemini APIs and Code Assist catalog.
    /// Google Gemini 官方 API 与 Code Assist 目录。
    Google,
    /// Official MiniMax compatible text APIs.
    /// MiniMax 官方兼容文本 API。
    MiniMax,
    /// Official OpenAI API.
    /// OpenAI 官方 API。
    OpenAi,
}

/// Contains the resolved reasoning support and exact selectable effort values.
/// 包含解析后的思考支持级别与精确可选档位。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FixedReasoningCapabilities {
    /// Whether reasoning is explicitly supported.
    /// 是否明确支持思考。
    pub(crate) support: CapabilitySupport,
    /// Exact user-selectable effort values.
    /// 用户可选的精确思考档位。
    pub(crate) efforts: Vec<ReasoningEffort>,
    /// Evidence revision attached to the resolved rule.
    /// 附加到解析规则的证据修订号。
    pub(crate) evidence_revision: &'static str,
}

/// Matches either one exact model identity or a stable model-family prefix.
/// 匹配一个精确模型身份或稳定模型族前缀。
#[derive(Debug, Clone, Copy)]
pub(crate) enum ModelRuleMatcher {
    /// Matches one exact provider model identifier.
    /// 匹配一个精确供应商模型标识。
    Exact(&'static str),
    /// Matches a provider model family including dated snapshots.
    /// 匹配包含日期快照的供应商模型族。
    Prefix(&'static str),
}

impl ModelRuleMatcher {
    /// Reports whether the exact discovered model belongs to this rule.
    /// 报告精确发现模型是否属于此规则。
    ///
    /// # Parameters
    /// 参数
    /// `model_id` is the provider-returned model identity.
    /// `model_id` 是供应商返回的模型身份。
    ///
    /// # Returns
    /// 返回值
    /// `true` when this rule owns the model.
    /// 当此规则归属该模型时返回 `true`。
    fn matches(self, model_id: &str) -> bool {
        match self {
            Self::Exact(expected) => model_id == expected,
            Self::Prefix(prefix) => model_id.starts_with(prefix),
        }
    }
}

/// Declares one model-specific override or model-family default.
/// 声明一个单模型覆盖或模型族默认值。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModelReasoningRule {
    /// Exact or family matcher.
    /// 精确或模型族匹配器。
    pub(crate) matcher: ModelRuleMatcher,
    /// Exact effort values accepted by the matched API model.
    /// 匹配 API 模型接受的精确档位。
    pub(crate) efforts: &'static [ReasoningEffort],
}

/// Resolves reasoning capabilities for one fixed provider and exact discovered model.
/// 解析一个固定供应商及精确发现模型的思考能力。
///
/// # Parameters
/// 参数
/// `provider` selects the reviewed provider rule set and `model_id` is the exact model identity.
/// `provider` 选择已审核供应商规则集，`model_id` 是精确模型身份。
///
/// # Returns
/// 返回值
/// A native contract for verified models or a conservative unknown contract.
/// 已验证模型返回原生合同，其他模型返回保守未知合同。
pub(crate) fn fixed_reasoning_capabilities(
    provider: FixedReasoningProvider,
    model_id: &str,
) -> FixedReasoningCapabilities {
    let (rules, revision) = match provider {
        FixedReasoningProvider::Anthropic => (
            ANTHROPIC_REASONING_RULES,
            "anthropic-effort-docs-2026-07-28",
        ),
        FixedReasoningProvider::Google => {
            (GOOGLE_REASONING_RULES, "google-thinking-docs-2026-07-28")
        }
        FixedReasoningProvider::MiniMax => (
            MINIMAX_REASONING_RULES,
            "minimax-compatible-text-openapi-2026-07-30",
        ),
        FixedReasoningProvider::OpenAi => (OPENAI_REASONING_RULES, "openai-model-docs-2026-07-28"),
    };
    let matched = rules.iter().find(|rule| rule.matcher.matches(model_id));
    match matched {
        Some(rule) => FixedReasoningCapabilities {
            support: CapabilitySupport::Native,
            efforts: rule.efforts.to_vec(),
            evidence_revision: revision,
        },
        None => FixedReasoningCapabilities {
            support: CapabilitySupport::Unknown,
            efforts: Vec::new(),
            evidence_revision: "reasoning-unverified",
        },
    }
}

/// Effort aliases shared by fixed rules.
/// 固定规则共享的思考档位别名。
const NONE_LOW_MEDIUM_HIGH: &[ReasoningEffort] = &[
    ReasoningEffort::None,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];
/// Effort aliases shared by fixed rules.
/// 固定规则共享的思考档位别名。
const MINIMAL_LOW_MEDIUM_HIGH: &[ReasoningEffort] = &[
    ReasoningEffort::Minimal,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];
/// Effort aliases shared by fixed rules.
/// 固定规则共享的思考档位别名。
const LOW_MEDIUM_HIGH: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];
/// Effort aliases shared by fixed rules.
/// 固定规则共享的思考档位别名。
const LOW_MEDIUM_HIGH_MAX: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::Max,
];
/// Effort aliases shared by fixed rules.
/// 固定规则共享的思考档位别名。
const LOW_MEDIUM_HIGH_XHIGH_MAX: &[ReasoningEffort] = &[
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::XHigh,
    ReasoningEffort::Max,
];
/// Effort aliases shared by fixed rules.
/// 固定规则共享的思考档位别名。
const NONE_LOW_MEDIUM_HIGH_XHIGH: &[ReasoningEffort] = &[
    ReasoningEffort::None,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::XHigh,
];
/// Effort aliases shared by fixed rules.
/// 固定规则共享的思考档位别名。
const NONE_LOW_MEDIUM_HIGH_XHIGH_MAX: &[ReasoningEffort] = &[
    ReasoningEffort::None,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::XHigh,
    ReasoningEffort::Max,
];
/// Effort aliases shared by fixed rules.
/// 固定规则共享的思考档位别名。
const MEDIUM_HIGH_XHIGH: &[ReasoningEffort] = &[
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::XHigh,
];
/// Effort aliases shared by fixed rules.
/// 固定规则共享的思考档位别名。
const MINIMAL_HIGH: &[ReasoningEffort] = &[ReasoningEffort::Minimal, ReasoningEffort::High];
/// Exact overrides precede broader Anthropic family defaults.
/// Anthropic 精确覆盖位于更宽泛模型族默认值之前。
pub(crate) const ANTHROPIC_REASONING_RULES: &[ModelReasoningRule] = &[
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("claude-opus-4-8"),
        efforts: LOW_MEDIUM_HIGH_XHIGH_MAX,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("claude-opus-4-7"),
        efforts: LOW_MEDIUM_HIGH_XHIGH_MAX,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("claude-sonnet-5"),
        efforts: LOW_MEDIUM_HIGH_XHIGH_MAX,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("claude-opus-5"),
        efforts: LOW_MEDIUM_HIGH_XHIGH_MAX,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("claude-fable-5"),
        efforts: LOW_MEDIUM_HIGH_XHIGH_MAX,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("claude-mythos-5"),
        efforts: LOW_MEDIUM_HIGH_XHIGH_MAX,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("claude-opus-4-6"),
        efforts: LOW_MEDIUM_HIGH_MAX,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("claude-sonnet-4-6"),
        efforts: LOW_MEDIUM_HIGH_MAX,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("claude-opus-4-5"),
        efforts: LOW_MEDIUM_HIGH,
    },
];

/// Google exact model exceptions precede series defaults.
/// Google 精确模型例外位于系列默认值之前。
pub(crate) const GOOGLE_REASONING_RULES: &[ModelReasoningRule] = &[
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Exact("gemini-3.1-flash-lite-image"),
        efforts: MINIMAL_HIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gemini-3.1-pro"),
        efforts: LOW_MEDIUM_HIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gemini-3-pro"),
        efforts: &[ReasoningEffort::Low, ReasoningEffort::High],
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gemini-3.6-flash"),
        efforts: MINIMAL_LOW_MEDIUM_HIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gemini-3.5-flash"),
        efforts: MINIMAL_LOW_MEDIUM_HIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gemini-3.1-flash-lite"),
        efforts: MINIMAL_LOW_MEDIUM_HIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gemini-3-flash"),
        efforts: MINIMAL_LOW_MEDIUM_HIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gemini-2.5-pro"),
        efforts: LOW_MEDIUM_HIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gemini-2.5-flash"),
        efforts: NONE_LOW_MEDIUM_HIGH,
    },
];

/// MiniMax M3 exposes an on/off adaptive-thinking switch while M2.x exposes one provider-adaptive mode.
/// MiniMax M3 暴露自适应思考开关，而 M2.x 只暴露一个供应商自适应模式。
pub(crate) const MINIMAX_REASONING_RULES: &[ModelReasoningRule] = &[
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("MiniMax-M3"),
        efforts: &[ReasoningEffort::None, ReasoningEffort::Adaptive],
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("MiniMax-M2"),
        efforts: &[ReasoningEffort::Adaptive],
    },
];

/// OpenAI exact Pro-model overrides precede broader GPT family defaults.
/// OpenAI 精确 Pro 模型覆盖位于更宽泛 GPT 模型族默认值之前。
pub(crate) const OPENAI_REASONING_RULES: &[ModelReasoningRule] = &[
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gpt-5.4-pro"),
        efforts: MEDIUM_HIGH_XHIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gpt-5.2-pro"),
        efforts: MEDIUM_HIGH_XHIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gpt-5.6"),
        efforts: NONE_LOW_MEDIUM_HIGH_XHIGH_MAX,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gpt-5.4"),
        efforts: NONE_LOW_MEDIUM_HIGH_XHIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gpt-5.2-codex"),
        efforts: &[
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::XHigh,
        ],
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gpt-5.1"),
        efforts: NONE_LOW_MEDIUM_HIGH,
    },
    ModelReasoningRule {
        matcher: ModelRuleMatcher::Prefix("gpt-5"),
        efforts: MINIMAL_LOW_MEDIUM_HIGH,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies exact overrides win before broader model-family defaults.
    /// 验证精确覆盖优先于更宽泛模型族默认值。
    #[test]
    fn exact_reasoning_override_precedes_family_default() {
        let capabilities = fixed_reasoning_capabilities(
            FixedReasoningProvider::Google,
            "gemini-3.1-flash-lite-image",
        );
        assert_eq!(capabilities.efforts, MINIMAL_HIGH);
    }

    /// Verifies MiniMax reasoning remains enabled without exposing unsupported effort values.
    /// 验证 MiniMax 保持启用思考且不暴露不受支持的强度值。
    #[test]
    fn minimax_m2_reasoning_is_provider_adaptive_only() {
        let capabilities =
            fixed_reasoning_capabilities(FixedReasoningProvider::MiniMax, "MiniMax-M2.7");
        assert_eq!(capabilities.support, CapabilitySupport::Native);
        assert_eq!(capabilities.efforts, [ReasoningEffort::Adaptive]);
    }

    /// Verifies MiniMax M3 exposes only the documented disabled/adaptive semantic switch.
    /// 验证 MiniMax M3 仅暴露文档化的关闭/自适应语义开关。
    #[test]
    fn minimax_m3_reasoning_exposes_documented_toggle() {
        let capabilities =
            fixed_reasoning_capabilities(FixedReasoningProvider::MiniMax, "MiniMax-M3");
        assert_eq!(capabilities.support, CapabilitySupport::Native);
        assert_eq!(
            capabilities.efforts,
            [ReasoningEffort::None, ReasoningEffort::Adaptive]
        );
    }

    /// Verifies unknown discovered models stay conservative instead of inheriting guessed controls.
    /// 验证未知发现模型保持保守且不会继承猜测控制。
    #[test]
    fn unknown_reasoning_model_stays_conservative() {
        let capabilities =
            fixed_reasoning_capabilities(FixedReasoningProvider::OpenAi, "future-model");
        assert_eq!(capabilities.support, CapabilitySupport::Unknown);
        assert!(capabilities.efforts.is_empty());
    }
}
