//! Provider request budget guard used immediately after request assembly and before network dispatch.
//! 在请求装配完成后、网络发送前使用的 Provider 请求预算守卫。

use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
use crate::agent_runtime::provider_transport_vulcan::VulcanPublicContractRequest;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::context_budget::{
    ContextCompactionPolicy, RequestContextCompactionBudgetSnapshotInput,
    build_request_context_compaction_budget_snapshot,
};
use std::sync::Arc;
use vulcan_ai_contract::ModelCapabilities;
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Immutable request-budget guard shared by the host-controlled streaming adapter wrapper.
/// 由宿主控制流式适配器包装层共享的不可变请求预算守卫。
#[derive(Clone)]
pub(crate) struct ProviderRequestBudgetGuard {
    /// Stable provider identifier used by the request budget fingerprint.
    /// 请求预算指纹使用的稳定 Provider 标识。
    provider_id: String,
    /// Stable model identifier used by the token estimator.
    /// Token 估算器使用的稳定模型标识。
    model_id: String,
    /// Canonical provider API family used by context-budget computation.
    /// 上下文预算计算使用的规范 Provider 接口族。
    protocol_profile_id: String,
    /// Resolved provider/model capabilities used to derive budget limits.
    /// 用于推导预算上限的已解析 Provider/Model 能力。
    capabilities: ModelCapabilities,
    /// Resolved reasoning controls used to reserve thinking tokens.
    /// 用于预留思考 Token 的已解析推理控制。
    reasoning: ProviderRuntimeReasoningResolution,
    /// Optional compaction policy resolved for this request.
    /// 为本次请求解析出的可选压缩策略。
    context_compaction_policy: Option<ContextCompactionPolicy>,
    /// Optional request-level input-token budget limit.
    /// 可选的请求级输入 Token 预算上限。
    context_budget_override: Option<usize>,
    /// Shared token-budget runtime reused by the real request hot path.
    /// 真实请求热路径复用的共享 Token 预算运行时。
    token_budget_runtime: Arc<TokenBudgetRuntime>,
}

impl ProviderRequestBudgetGuard {
    /// Builds one request-budget guard for the active provider/model pair.
    /// 为当前 Provider/Model 组合构建一份请求预算守卫。
    pub(crate) fn new(
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        protocol_profile_id: impl Into<String>,
        capabilities: ModelCapabilities,
        reasoning: ProviderRuntimeReasoningResolution,
        context_compaction_policy: Option<ContextCompactionPolicy>,
        context_budget_override: Option<usize>,
        token_budget_runtime: Arc<TokenBudgetRuntime>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            protocol_profile_id: protocol_profile_id.into(),
            capabilities,
            reasoning,
            context_compaction_policy,
            context_budget_override,
            token_budget_runtime,
        }
    }

    /// Validates one final request snapshot and rejects dispatch when automatic compaction should run first.
    /// 校验一份最终请求快照，并在应先执行自动压缩时拒绝发送。
    fn validate_snapshot(
        &self,
        snapshot: &CoreProviderRequestSnapshot,
    ) -> Result<(), VulcanCoreError> {
        let budget = build_request_context_compaction_budget_snapshot(
            RequestContextCompactionBudgetSnapshotInput {
                provider_id: self.provider_id.as_str(),
                model_id: self.model_id.as_str(),
                protocol_profile_id: self.protocol_profile_id.as_str(),
                capabilities: &self.capabilities,
                reasoning: &self.reasoning,
                request_snapshot: snapshot,
                compaction_policy: self.context_compaction_policy,
                context_budget_override: self.context_budget_override,
                observed_usage: None,
                prefill_input_tokens: None,
                estimator: self.token_budget_runtime.estimator(),
            },
        );
        if budget.should_compact() {
            return Err(VulcanCoreError::ContextCompactionRequired {
                message: format!(
                    "请求预算门阻止发送：{}。请先执行上下文压缩后重新组装请求。",
                    budget.summary_note()
                ),
            });
        }

        Ok(())
    }
}

/// Streaming adapter wrapper that validates the exact built request before provider dispatch.
/// 在 Provider 分发前校验精确已构建请求的流式适配器包装层。
#[derive(Clone)]
pub(crate) struct BudgetCheckedStreamingAdapter<A> {
    /// Inner provider adapter that still owns provider-specific request execution.
    /// 仍然持有 Provider 专属请求执行逻辑的内部适配器。
    inner: A,
    /// Optional request-budget guard used before network dispatch.
    /// 网络分发前使用的可选请求预算守卫。
    request_budget_guard: Option<ProviderRequestBudgetGuard>,
}

/// Exposes the canonical request snapshot consumed by the budget guard.
/// 公开预算门消费的规范请求快照。
pub(crate) trait ProviderBudgetRequest {
    /// Returns the exact canonical snapshot represented by this transport request.
    /// 返回该传输请求所表示的精确规范快照。
    fn budget_snapshot(&self) -> &CoreProviderRequestSnapshot;
}

impl ProviderBudgetRequest for VulcanPublicContractRequest {
    fn budget_snapshot(&self) -> &CoreProviderRequestSnapshot {
        &self.budget
    }
}

impl<A> BudgetCheckedStreamingAdapter<A> {
    /// Wraps one provider adapter with an optional request-budget guard.
    /// 使用可选请求预算守卫包装一个 Provider 适配器。
    pub(crate) fn new(inner: A, request_budget_guard: Option<ProviderRequestBudgetGuard>) -> Self {
        Self {
            inner,
            request_budget_guard,
        }
    }
}

#[async_trait::async_trait]
impl<A> VulcanProviderAdapter for BudgetCheckedStreamingAdapter<A>
where
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse> + Clone + Send + Sync,
    A::Request: ProviderBudgetRequest,
{
    type Request = A::Request;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the wrapped adapter.
    /// 返回被包装适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }

    /// Returns the stable model identifier used by the wrapped adapter.
    /// 返回被包装适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        self.inner.model_id()
    }

    /// Builds one provider request through the wrapped adapter.
    /// 通过被包装适配器构建一份 Provider 请求。
    async fn build_request(
        &self,
        snapshot: &vulcan_core::types::ConversationSnapshot,
        config: &vulcan_core::types::ControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        self.inner.build_request(snapshot, config).await
    }

    /// Forwards one-shot execution to the wrapped adapter.
    /// 将单次执行转发给被包装适配器。
    async fn execute_request(
        &self,
        request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        self.inner.execute_request(request).await
    }

    /// Validates the exact request snapshot before network dispatch.
    /// 在网络分发前校验精确请求快照。
    fn validate_built_request(&self, request: &Self::Request) -> Result<(), VulcanCoreError> {
        self.inner.validate_built_request(request)?;
        if let Some(guard) = self.request_budget_guard.as_ref() {
            guard.validate_snapshot(request.budget_snapshot())?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<A> VulcanStreamingProviderAdapter for BudgetCheckedStreamingAdapter<A>
where
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse> + Clone + Send + Sync,
    A::Request: ProviderBudgetRequest,
{
    /// Forwards streaming execution to the wrapped adapter after validation has already passed.
    /// 在校验通过后将流式执行转发给被包装适配器。
    async fn execute_request_streaming<S>(
        &self,
        request: Self::Request,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        self.inner.execute_request_streaming(request, sink).await
    }
}

#[cfg(test)]
mod tests {
    //! Request-budget guard tests for provider and tool boundaries.
    //! Provider 与工具边界的请求预算守卫测试。

    use super::*;
    use vulcan_core::types::{
        ConversationMessage, ConversationPayload, ProviderContinuation, ProviderPromptLayers,
    };

    /// Builds one minimal request snapshot for request-boundary budget tests.
    /// 为请求边界预算测试构建一份最小请求快照。
    ///
    /// `history` is the exact canonical history assembled for the next physical request.
    /// `history` 是为下一次真实请求装配的精确规范历史。
    /// The returned snapshot contains no provider-specific optional fields.
    /// 返回的快照不包含供应商专属可选字段。
    fn request_snapshot(history: Vec<ConversationMessage>) -> CoreProviderRequestSnapshot {
        CoreProviderRequestSnapshot {
            provider_id: String::from("test-provider"),
            model_id: String::from("test-model"),
            system_prompt: String::new(),
            prompt_layers: ProviderPromptLayers::default(),
            prompt: String::from("continue"),
            history,
            temperature: None,
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        }
    }

    /// Verifies a large tool result is rechecked before the next physical provider request.
    /// 验证大型工具结果会在下一次真实 Provider 请求前重新接受预算检查。
    #[test]
    fn tool_result_growth_requires_compaction_before_next_provider_request() {
        let guard = ProviderRequestBudgetGuard::new(
            "test-provider",
            "test-model",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            ModelCapabilities {
                context_limit: Some(1_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            Some(100),
            Arc::new(TokenBudgetRuntime::pending()),
        );

        guard
            .validate_snapshot(&request_snapshot(Vec::new()))
            .expect("the first compact request should pass the dispatch budget gate");

        let request_after_tool = request_snapshot(vec![ConversationMessage {
            id: String::from("tool-result-1"),
            payload: ConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                is_error: false,
                text: "large tool output ".repeat(200),
            },
        }]);
        let error = guard
            .validate_snapshot(&request_after_tool)
            .expect_err("tool-result growth must trigger compaction before dispatch");

        assert!(matches!(
            error,
            VulcanCoreError::ContextCompactionRequired { .. }
        ));
    }
}
