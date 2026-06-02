//! Thin GUI adapter for shared context-budget estimation helpers.
//! 面向图形界面的共享上下文预算估算辅助薄适配层。

use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
use crate::provider_capabilities::ResolvedModelCapabilities;
use crate::provider_config::{ProviderApiFamily, ReasoningEffortLevel};
use vulcan_context::budget::{
    ContextBudgetAiToolCapability, ContextBudgetCapabilities, ContextProviderApiFamily,
    ContextReasoningEffort, ContextTokenEstimator,
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

/// Builds one request-facing compaction budget snapshot with an initialized token estimator.
/// 使用已初始化的 Token 估算器构建一份面向真实请求的压缩预算快照。
pub fn build_request_context_compaction_budget_snapshot_with_estimator(
    provider_id: &str,
    model_id: &str,
    api_family: ProviderApiFamily,
    capabilities: &ResolvedModelCapabilities,
    reasoning: &ProviderRuntimeReasoningResolution,
    request_snapshot: &CoreProviderRequestSnapshot,
    observed_usage: Option<&ProviderUsageSnapshot>,
    estimator: Option<&dyn ContextTokenEstimator>,
) -> ContextBudgetSnapshot {
    vulcan_context::budget::build_request_context_budget_snapshot_with_policy_and_estimator(
        provider_id,
        model_id,
        map_provider_api_family(api_family),
        &map_capabilities(capabilities),
        map_reasoning_effort(reasoning.requested_effort),
        request_snapshot,
        observed_usage,
        Some(ContextCompactionPolicy::default()),
        estimator,
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

/// Maps one GUI provider API family into the shared budget-computation family enum.
/// 将一条图形界面 Provider 接口族映射为共享预算计算接口族枚举。
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

/// Maps one GUI reasoning preset into the shared budget-computation reasoning bucket.
/// 将一条图形界面推理档位映射为共享预算计算推理分桶。
fn map_reasoning_effort(reasoning_effort: ReasoningEffortLevel) -> ContextReasoningEffort {
    match reasoning_effort {
        ReasoningEffortLevel::Off => ContextReasoningEffort::None,
        ReasoningEffortLevel::Low => ContextReasoningEffort::Low,
        ReasoningEffortLevel::Medium => ContextReasoningEffort::Medium,
        ReasoningEffortLevel::High => ContextReasoningEffort::High,
        ReasoningEffortLevel::XHigh => ContextReasoningEffort::XHigh,
    }
}

/// Maps one GUI capability snapshot into the shared budget-computation capability shape.
/// 将一份图形界面能力快照映射为共享预算计算能力形态。
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
