//! Runtime facade for shared context-budget estimation helpers.
//! 面向运行时的共享上下文预算估算辅助门面。

use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
use crate::provider_capabilities::ResolvedModelCapabilities;
use crate::provider_config::{ProviderApiFamily, ReasoningEffortLevel};
use vulcan_context::budget::{
    ContextBudgetAiToolCapability, ContextBudgetCapabilities, ContextProviderApiFamily,
    ContextReasoningEffort, ContextTokenEstimator, RequestContextBudgetSnapshotInput,
    RequestContextBudgetSnapshotOptions, build_request_context_budget_snapshot_with_options,
};
pub use vulcan_context::{
    CONTEXT_BUDGET_COMPACTION_RATIO, CONTEXT_BUDGET_OVERFLOW_RATIO, CONTEXT_BUDGET_WARNING_RATIO,
    ContextBudgetSeverity, ContextBudgetSnapshot, ContextCompactionPolicy,
    apply_context_budget_anchor_baseline,
};
use vulcan_core::types::{
    ProviderRequestSnapshot as CoreProviderRequestSnapshot, ProviderUsageSnapshot,
};

/// Runtime input payload for compaction-aware request budget snapshots.
/// 面向运行时压缩预算快照的输入载荷。
pub struct RequestContextCompactionBudgetSnapshotInput<'a> {
    /// Stable provider identifier used by diagnostics and fingerprints.
    /// 诊断与指纹使用的稳定 Provider 标识。
    pub provider_id: &'a str,
    /// Stable model identifier used by token estimation.
    /// Token 估算使用的稳定模型标识。
    pub model_id: &'a str,
    /// Runtime provider API family.
    /// 运行时 Provider 接口族。
    pub api_family: ProviderApiFamily,
    /// Resolved provider/model capabilities.
    /// 已解析的 Provider/Model 能力。
    pub capabilities: &'a ResolvedModelCapabilities,
    /// Resolved reasoning controls.
    /// 已解析的推理控制。
    pub reasoning: &'a ProviderRuntimeReasoningResolution,
    /// Final provider request snapshot.
    /// 最终 Provider 请求快照。
    pub request_snapshot: &'a CoreProviderRequestSnapshot,
    /// Optional compaction policy resolved for this request.
    /// 为本次请求解析出的可选压缩策略。
    pub compaction_policy: Option<ContextCompactionPolicy>,
    /// Optional request-level input-token budget limit.
    /// 可选的请求级输入 Token 预算上限。
    pub context_budget_override: Option<usize>,
    /// Optional provider-observed usage used to calibrate estimates.
    /// 用于校准估算值的可选 Provider 实测用量。
    pub observed_usage: Option<&'a ProviderUsageSnapshot>,
    /// Optional host token estimator.
    /// 可选宿主 Token 估算器。
    pub estimator: Option<&'a dyn ContextTokenEstimator>,
}

/// Builds one request-facing compaction budget snapshot with an initialized token estimator.
/// 使用已初始化的 Token 估算器构建一份面向真实请求的压缩预算快照。
pub fn build_request_context_compaction_budget_snapshot(
    input: RequestContextCompactionBudgetSnapshotInput<'_>,
) -> ContextBudgetSnapshot {
    let compaction_policy = input.compaction_policy.unwrap_or_default();
    let capabilities = apply_context_budget_override(
        map_capabilities(input.capabilities),
        input.context_budget_override,
    );
    build_request_context_budget_snapshot_with_options(
        RequestContextBudgetSnapshotInput {
            provider_id: input.provider_id,
            model_id: input.model_id,
            api_family: map_provider_api_family(input.api_family),
            capabilities: &capabilities,
            reasoning: map_reasoning_effort(input.reasoning.requested_effort),
            request_snapshot: input.request_snapshot,
            observed_usage: input.observed_usage,
        },
        RequestContextBudgetSnapshotOptions {
            compaction_policy: Some(compaction_policy),
            estimator: input.estimator,
        },
    )
}

/// Applies a request-level context budget without exceeding the model-declared input window.
/// 应用请求级上下文预算，同时不超过模型声明的输入窗口。
fn apply_context_budget_override(
    mut capabilities: ContextBudgetCapabilities,
    context_budget_override: Option<usize>,
) -> ContextBudgetCapabilities {
    let Some(request_limit) = context_budget_override.filter(|limit| *limit > 0) else {
        return capabilities;
    };
    let effective_limit = capabilities
        .effective_input_limit()
        .map(|model_limit| request_limit.min(model_limit))
        .unwrap_or(request_limit);
    capabilities.input_limit = Some(effective_limit);
    capabilities
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
    vulcan_context::budget::build_context_budget_fingerprint(
        provider_id,
        model_id,
        map_provider_api_family(api_family),
        &map_capabilities(capabilities),
        map_reasoning_effort(reasoning_effort),
        visible_tool_count,
        compaction_policy,
    )
}

/// Maps one runtime provider API family into the shared budget-computation family enum.
/// 将一条运行时 Provider 接口族映射为共享预算计算接口族枚举。
fn map_provider_api_family(api_family: ProviderApiFamily) -> ContextProviderApiFamily {
    match api_family {
        ProviderApiFamily::OpenAiResponses => ContextProviderApiFamily::OpenAiResponses,
        ProviderApiFamily::OpenAiChatCompatible => ContextProviderApiFamily::OpenAiChatCompatible,
        ProviderApiFamily::AnthropicMessages => ContextProviderApiFamily::AnthropicMessages,
        ProviderApiFamily::GoogleGeminiGenerateContent => {
            ContextProviderApiFamily::GoogleGeminiGenerateContent
        }
        ProviderApiFamily::CohereChat => ContextProviderApiFamily::CohereChat,
        ProviderApiFamily::BedrockConverse => ContextProviderApiFamily::BedrockConverse,
    }
}

/// Maps one runtime reasoning preset into the shared budget-computation reasoning bucket.
/// 将一条运行时推理档位映射为共享预算计算推理分桶。
fn map_reasoning_effort(reasoning_effort: ReasoningEffortLevel) -> ContextReasoningEffort {
    match reasoning_effort {
        ReasoningEffortLevel::Off => ContextReasoningEffort::None,
        ReasoningEffortLevel::Low => ContextReasoningEffort::Low,
        ReasoningEffortLevel::Medium => ContextReasoningEffort::Medium,
        ReasoningEffortLevel::High => ContextReasoningEffort::High,
        ReasoningEffortLevel::XHigh => ContextReasoningEffort::XHigh,
    }
}

/// Maps one runtime capability snapshot into the shared budget-computation capability shape.
/// 将一份运行时能力快照映射为共享预算计算能力形态。
fn map_capabilities(capabilities: &ResolvedModelCapabilities) -> ContextBudgetCapabilities {
    ContextBudgetCapabilities {
        context_limit: capabilities.context_limit,
        input_limit: capabilities.input_limit,
        output_limit: capabilities.output_limit,
        supports_tool_call: capabilities.supports_tool_call,
        supports_temperature: capabilities.supports_temperature,
        supports_attachment: capabilities.supports_attachment,
        input_modalities: capabilities.input_modalities.clone(),
        output_modalities: capabilities.output_modalities.clone(),
        ai_tools: capabilities
            .ai_tools
            .iter()
            .map(|tool| ContextBudgetAiToolCapability {
                kind_key: tool.kind.key().to_string(),
                policy_key: format!("{:?}", tool.policy),
                native_tool_type: tool.native_tool_type.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    //! Runtime context-budget facade tests.
    //! 运行时上下文预算门面测试。

    use super::*;
    use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
    use vulcan_core::types::{
        ProviderContinuation, ProviderPromptLayers,
        ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    };

    /// Builds one empty provider request snapshot for budget-policy facade tests.
    /// 为预算策略门面测试构建一份空 Provider 请求快照。
    fn test_request_snapshot() -> CoreProviderRequestSnapshot {
        CoreProviderRequestSnapshot {
            provider_id: String::from("test-provider"),
            model_id: String::from("test-model"),
            system_prompt: String::new(),
            prompt_layers: ProviderPromptLayers::default(),
            prompt: String::new(),
            history: Vec::new(),
            temperature: None,
            max_turns: 1,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        }
    }

    /// Verifies request budget snapshots preserve the resolved compaction policy instead of forcing defaults.
    /// 验证请求预算快照会保留已解析压缩策略，而不是强制使用默认值。
    #[test]
    fn request_budget_snapshot_uses_resolved_compaction_policy() {
        let capabilities = ResolvedModelCapabilities {
            input_limit: Some(20_000),
            ..ResolvedModelCapabilities::default()
        };
        let reasoning = ProviderRuntimeReasoningResolution::default();
        let request_snapshot = test_request_snapshot();
        let policy = ContextCompactionPolicy {
            auto: false,
            reserved: 1_234,
            ..ContextCompactionPolicy::default()
        };

        let budget = build_request_context_compaction_budget_snapshot(
            RequestContextCompactionBudgetSnapshotInput {
                provider_id: "test-provider",
                model_id: "test-model",
                api_family: ProviderApiFamily::OpenAiChatCompatible,
                capabilities: &capabilities,
                reasoning: &reasoning,
                request_snapshot: &request_snapshot,
                compaction_policy: Some(policy),
                context_budget_override: None,
                observed_usage: None,
                estimator: None,
            },
        );

        assert!(!budget.compaction_auto_enabled);
        assert_eq!(budget.reserved_compaction_tokens, 1_234);
        assert_eq!(budget.effective_input_limit, Some(18_766));

        let enabled_budget = build_request_context_compaction_budget_snapshot(
            RequestContextCompactionBudgetSnapshotInput {
                provider_id: "test-provider",
                model_id: "test-model",
                api_family: ProviderApiFamily::OpenAiChatCompatible,
                capabilities: &capabilities,
                reasoning: &reasoning,
                request_snapshot: &request_snapshot,
                compaction_policy: Some(ContextCompactionPolicy::default()),
                context_budget_override: None,
                observed_usage: None,
                estimator: None,
            },
        );

        assert!(enabled_budget.compaction_auto_enabled);
    }

    /// Verifies request-level context budgets cap the effective input limit below the model window.
    /// 验证请求级上下文预算会把生效输入上限压到模型窗口以内。
    #[test]
    fn request_budget_snapshot_uses_request_context_budget_limit() {
        let capabilities = ResolvedModelCapabilities {
            input_limit: Some(20_000),
            ..ResolvedModelCapabilities::default()
        };
        let reasoning = ProviderRuntimeReasoningResolution::default();
        let request_snapshot = test_request_snapshot();

        let budget = build_request_context_compaction_budget_snapshot(
            RequestContextCompactionBudgetSnapshotInput {
                provider_id: "test-provider",
                model_id: "test-model",
                api_family: ProviderApiFamily::OpenAiChatCompatible,
                capabilities: &capabilities,
                reasoning: &reasoning,
                request_snapshot: &request_snapshot,
                compaction_policy: Some(ContextCompactionPolicy::default()),
                context_budget_override: Some(12_000),
                observed_usage: None,
                estimator: None,
            },
        );

        assert_eq!(budget.effective_input_limit, Some(3_808));

        let model_capped_budget = build_request_context_compaction_budget_snapshot(
            RequestContextCompactionBudgetSnapshotInput {
                provider_id: "test-provider",
                model_id: "test-model",
                api_family: ProviderApiFamily::OpenAiChatCompatible,
                capabilities: &capabilities,
                reasoning: &reasoning,
                request_snapshot: &request_snapshot,
                compaction_policy: Some(ContextCompactionPolicy::default()),
                context_budget_override: Some(30_000),
                observed_usage: None,
                estimator: None,
            },
        );

        assert_eq!(model_capped_budget.effective_input_limit, Some(11_808));
    }
}
