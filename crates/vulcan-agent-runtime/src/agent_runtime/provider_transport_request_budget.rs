//! Provider request budget guard used immediately after request assembly and before network dispatch.
//! 在请求装配完成后、网络发送前使用的 Provider 请求预算守卫。

use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::context_budget::build_request_context_compaction_budget_snapshot_with_estimator;
use crate::provider_capabilities::ResolvedModelCapabilities;
use crate::provider_config::ProviderApiFamily;
use std::sync::Arc;
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
    api_family: ProviderApiFamily,
    /// Resolved provider/model capabilities used to derive budget limits.
    /// 用于推导预算上限的已解析 Provider/Model 能力。
    capabilities: ResolvedModelCapabilities,
    /// Resolved reasoning controls used to reserve thinking tokens.
    /// 用于预留思考 Token 的已解析推理控制。
    reasoning: ProviderRuntimeReasoningResolution,
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
        api_family: ProviderApiFamily,
        capabilities: ResolvedModelCapabilities,
        reasoning: ProviderRuntimeReasoningResolution,
        token_budget_runtime: Arc<TokenBudgetRuntime>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            api_family,
            capabilities,
            reasoning,
            token_budget_runtime,
        }
    }

    /// Validates one final request snapshot and rejects dispatch when automatic compaction should run first.
    /// 校验一份最终请求快照，并在应先执行自动压缩时拒绝发送。
    fn validate_snapshot(
        &self,
        snapshot: &CoreProviderRequestSnapshot,
    ) -> Result<(), VulcanCoreError> {
        let budget = build_request_context_compaction_budget_snapshot_with_estimator(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            self.api_family,
            &self.capabilities,
            &self.reasoning,
            snapshot,
            None,
            self.token_budget_runtime.estimator(),
        );
        if budget.should_compact() {
            return Err(VulcanCoreError::InvalidState {
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
    A: VulcanStreamingProviderAdapter<
            Request = OpenAiCompatibleCoreTurnRequest,
            Response = CoreProviderTurnResponse,
        > + Clone
        + Send
        + Sync,
{
    type Request = OpenAiCompatibleCoreTurnRequest;
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
            guard.validate_snapshot(&request.snapshot)?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<A> VulcanStreamingProviderAdapter for BudgetCheckedStreamingAdapter<A>
where
    A: VulcanStreamingProviderAdapter<
            Request = OpenAiCompatibleCoreTurnRequest,
            Response = CoreProviderTurnResponse,
        > + Clone
        + Send
        + Sync,
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
