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
#[allow(unused_imports)]
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
    build_request_context_budget_snapshot_with_options(
        RequestContextBudgetSnapshotInput {
            provider_id: input.provider_id,
            model_id: input.model_id,
            api_family: map_provider_api_family(input.api_family),
            capabilities: &map_capabilities(input.capabilities),
            reasoning: map_reasoning_effort(input.reasoning.requested_effort),
            request_snapshot: input.request_snapshot,
            observed_usage: input.observed_usage,
        },
        RequestContextBudgetSnapshotOptions {
            compaction_policy: Some(ContextCompactionPolicy::default()),
            estimator: input.estimator,
        },
    )
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
