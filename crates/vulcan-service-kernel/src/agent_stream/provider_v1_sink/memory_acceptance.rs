//! Test-only access to the production V1 physical request builder.
//! 仅供测试访问生产 V1 物理请求构建器。

use super::*;
use vulcan_agent_runtime::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
use vulcan_agent_runtime::agent_runtime::provider_transport_request_budget::ProviderRequestBudgetGuard;
use vulcan_ai_contract::{AiExecutionRequest, AiOperation, ModelCapabilities};
use vulcan_core::types::{ControllerConfig, ProviderContextInjection};
use vulcan_session_core::v1::event::{ApiPurposeKind, ApiRequestSnapshot};

/// Builds a text-only fixture candidate and passes its real V1 snapshot through production reconstruction.
/// 构造纯文本夹具候选，并将其真实 V1 快照送入生产重建。
/// Takes the authoritative snapshot and admitted overlay; returns the actual request, budget and canonical projections.
/// 接收权威快照与准入覆盖层，返回实际请求、预算及规范投影。
pub(crate) fn rebuild_memory_acceptance_request(
    snapshot: SessionProviderProjectionSnapshot,
    injection: ProviderContextInjection,
) -> Result<ProviderRequestPreparationCandidate, String> {
    // The fixed transport fixture carries no credentials and performs no Provider dispatch.
    // 固定传输夹具不携带凭证，也不执行 Provider 分派。
    let facts = ProviderV1RequestFacts {
        provider_call_id: "memory-acceptance".to_owned(),
        purpose: ApiPurposeKind::MainGeneration,
        continuation_action_id: None,
        request: ApiRequestSnapshot::new("provider-1", "model-1"),
        prepared_at_ms: 10,
    };
    // All history starts empty so only the supplied V1 owner can contribute conversation content.
    // 历史初始为空，只有传入的 V1 归属方可以贡献会话内容。
    let candidate = ProviderRequestPreparationCandidate {
        source_message_positions: Default::default(),
        media_context: None,
        exact_request: AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: facts.request.model.clone(),
            cache_scope_id: None,
            continuation: None,
            messages: Vec::new(),
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: Default::default(),
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: None,
        },
        controller_config: ControllerConfig {
            provider_id: facts.request.provider.clone(),
            model_id: facts.request.model.clone(),
            temperature: facts.request.temperature,
            context_injection: Some(injection),
            ..Default::default()
        },
        controller_snapshot: ConversationSnapshot::default(),
        request_snapshot: CoreProviderRequestSnapshot {
            provider_id: facts.request.provider.clone(),
            model_id: facts.request.model.clone(),
            system_prompt: String::new(),
            prompt_layers: ProviderPromptLayers::default(),
            prompt: String::new(),
            prompt_in_history: true,
            history: Vec::new(),
            temperature: facts.request.temperature,
            max_turns: 8,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        },
        context_budget: None,
        // Match the admitted fixture route through the same frozen guard used by physical V1 tests.
        // 使用物理 V1 测试同款冻结守卫匹配已准入夹具路由。
        request_budget_guard: Some(ProviderRequestBudgetGuard::new_v1(
            facts.request.provider.clone(),
            facts.request.model.clone(),
            "test-profile",
            "sha256:test-route",
            ModelCapabilities::default(),
            ProviderRuntimeReasoningResolution::default(),
            None,
            None,
            None,
        )),
        dynamic_prompt_layers: ProviderPromptLayers::default(),
        locked_baseline: LockedBaselineProjection::empty(),
    };
    // Resource-bearing fixtures are rejected by the production builder without fabricated media delivery.
    // 带资源的夹具由生产构建器拒绝，不伪造媒体投递。
    let prepared_media = media::PreparedV1UserMedia {
        prompts: BTreeMap::new(),
        bindings: Vec::new(),
    };
    rebuild_provider_transport_candidate(snapshot, &facts, candidate, prepared_media)
        .map(|prepared| prepared.transport)
        .map_err(|error| error.to_string())
}
