//! Provider transport facade that owns provider-family dispatch while delegating the concrete work to provider-specific adapters.
//! 持有 Provider 家族分发逻辑并把具体工作委派给各 Provider 专用适配器的传输门面模块。

use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport_anthropic_openrouter::{
    execute_anthropic_stream, execute_openrouter_stream,
};
use crate::agent_runtime::provider_transport_openai::{
    execute_openai_compatible_stream, execute_openai_responses_stream,
};
use crate::model::SessionId;
use crate::provider_config::ProviderRuntimeKind;
use async_stream::stream;
use reqwest::header::HeaderMap;
use rig::message::Message as RigMessage;
use serde_json::Value;
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Owned provider transport execution request used by the facade to route one prepared runtime turn into the correct provider-specific adapter.
/// 供门面层把一份已准备好的运行时回合路由到正确 Provider 专用适配器的自有 Provider 传输执行请求。
pub(crate) struct ProviderTransportExecutionRequest {
    /// Runtime family used to select the concrete transport adapter implementation.
    /// 用于选择具体传输适配实现的运行时家族类型。
    pub(crate) runtime_kind: ProviderRuntimeKind,
    /// Stable provider identifier attached to request snapshots and continuation records.
    /// 附着到请求快照与 continuation 记录上的稳定 Provider 标识。
    pub(crate) provider_id: String,
    /// Human-readable runtime label surfaced when a provider family is still unsupported.
    /// 当某个 Provider 家族尚未接通时向外暴露的人类可读运行时标签。
    pub(crate) runtime_label: String,
    /// Resolved API key passed down to the low-level provider adapter.
    /// 下传给底层 Provider 适配器的已解析 API Key。
    pub(crate) api_key: String,
    /// Optional base URL override used by providers that support custom endpoints.
    /// 供支持自定义端点的 Provider 使用的可选基础地址覆盖值。
    pub(crate) base_url: Option<String>,
    /// Stable model identifier used by the target provider transport.
    /// 目标 Provider 传输层使用的稳定模型标识。
    pub(crate) model_id: String,
    /// Exact system preamble that should be visible to the outgoing provider request.
    /// 需要暴露给出站 Provider 请求的精确系统前导提示词。
    pub(crate) system_preamble: String,
    /// Exact outgoing prompt text for the current controller turn.
    /// 当前控制器回合的精确出站提示词文本。
    pub(crate) prompt: String,
    /// Provider-visible request history retained for debugging parity and adapter signatures.
    /// 为保持调试一致性与适配器签名而保留的 Provider 可见请求历史。
    pub(crate) request_history: Vec<RigMessage>,
    /// Canonical conversation snapshot consumed by the host-owned controller loop.
    /// 供宿主自有控制器循环消费的规范会话快照。
    pub(crate) core_snapshot: CoreConversationSnapshot,
    /// Optional temperature override already normalized by the executor.
    /// 已由执行器归一化后的可选 temperature 覆盖值。
    pub(crate) temperature: Option<f64>,
    /// Effective turn budget used by the host-owned controller loop.
    /// 宿主自有控制器循环使用的生效轮次预算。
    pub(crate) max_turns: usize,
    /// Fully resolved outbound request headers.
    /// 完整解析后的出站请求头集合。
    pub(crate) headers: HeaderMap,
    /// Shared tools manager used to construct the host-managed runtime bridge.
    /// 用于构建宿主管理运行时桥接层的共享工具管理器。
    pub(crate) tools_manager: VulcanToolsManager,
    /// Stable session identifier propagated into tool runtime events.
    /// 传播到工具运行时事件中的稳定会话标识。
    pub(crate) session_id: SessionId,
    /// Resolved tool definitions exposed to the provider for the current turn.
    /// 当前回合向 Provider 暴露的已解析工具定义集合。
    pub(crate) resolved_tools: Vec<ResolvedToolDefinition>,
    /// Optional provider-specific additional parameters already merged by upper layers.
    /// 已由上层合并完成的可选 Provider 专用附加参数。
    pub(crate) additional_params: Option<Value>,
}

/// Dispatches one prepared provider execution request into the correct provider-family transport adapter.
/// 将一份已准备好的 Provider 执行请求分发到正确的 Provider 家族传输适配器。
pub(crate) fn execute_provider_transport_stream(
    request: ProviderTransportExecutionRequest,
) -> ProviderDeltaStream {
    match request.runtime_kind {
        ProviderRuntimeKind::OpenAi => execute_openai_responses_stream(
            &request.provider_id,
            &request.api_key,
            request.base_url.as_deref(),
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::OpenAiCompatible
        | ProviderRuntimeKind::XAi
        | ProviderRuntimeKind::Mistral
        | ProviderRuntimeKind::Groq
        | ProviderRuntimeKind::TogetherAi
        | ProviderRuntimeKind::Perplexity => execute_openai_compatible_stream(
            &request.provider_id,
            &request.api_key,
            request.base_url.as_deref(),
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::OpenRouter => execute_openrouter_stream(
            &request.provider_id,
            &request.api_key,
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::Anthropic => execute_anthropic_stream(
            &request.provider_id,
            &request.api_key,
            request.base_url.as_deref(),
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::AzureOpenAi
        | ProviderRuntimeKind::GoogleGemini
        | ProviderRuntimeKind::Cohere => unsupported_provider_stream(request.runtime_label),
    }
}

/// Returns one one-shot error stream for provider families whose host-controlled transport path is not implemented yet.
/// 为尚未实现宿主控制传输链路的 Provider 家族返回一次性错误流。
fn unsupported_provider_stream(runtime_label: String) -> ProviderDeltaStream {
    Box::pin(stream! {
        yield Err(format!(
            "当前阶段尚未接通 `{}` 的真实流式执行链路，请优先选择 OpenAI 兼容接口、百炼 Coding Plan、OpenRouter 或 Anthropic。",
            runtime_label
        ));
    })
}
