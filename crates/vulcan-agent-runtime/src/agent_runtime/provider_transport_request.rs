//! Owned provider transport request models shared by the host-controlled transport router and executor.
//! 供宿主控制传输路由层与执行器共享的自有 Provider 传输请求模型。

use crate::agent_runtime::provider_transport_request_budget::ProviderRequestBudgetGuard;
use crate::agent_runtime::provider_transport_retry::ProviderRetryPolicy;
use crate::agent_runtime::runtime_stream_intervention::RuntimePromptIntervention;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::model::SessionId;
use crate::provider_config::{ModelInterleavedCapability, ProviderApiFamily, ProviderRuntimeKind};
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::sync::Arc;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage,
    ConversationSnapshot as CoreConversationSnapshot,
    ProviderContextInjection as CoreProviderContextInjection,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Owned provider transport execution request used to route one prepared runtime turn into the correct provider-specific adapter.
/// 用于把一份已准备好的运行时回合路由到正确 Provider 专用适配器的自有 Provider 传输执行请求。
pub(crate) struct ProviderTransportExecutionRequest {
    /// Canonical provider API family used to group transport routing and shared protocol behavior.
    /// 用于归并传输路由与共享协议行为的规范 Provider 接口族。
    pub(crate) api_family: ProviderApiFamily,
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
    /// Execution mode label carried to invocation-time tool guards.
    /// 传递给调用期工具守卫的执行模式标签。
    pub(crate) execution_mode: String,
    /// Exact system preamble that should be visible to the outgoing provider request.
    /// 需要暴露给出站 Provider 请求的精确系统前导提示词。
    pub(crate) system_preamble: String,
    /// Exact outgoing prompt text for the current controller turn.
    /// 当前控制器回合的精确出站提示词文本。
    pub(crate) prompt: String,
    /// Provider-visible request history retained for debugging parity and adapter signatures.
    /// 为保持调试一致性与适配器签名而保留的 Provider 可见请求历史。
    pub(crate) request_history: Vec<CoreConversationMessage>,
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
    /// Shared resumable task runtime reused by the provider execution path.
    /// 被 Provider 执行路径复用的共享可恢复任务运行时。
    pub(crate) task_runtime: Arc<TaskRuntime>,
    /// Resolved tool definitions exposed to the provider for the current turn.
    /// 当前回合向 Provider 暴露的已解析工具定义集合。
    pub(crate) resolved_tools: Vec<ResolvedToolDefinition>,
    /// Optional provider-specific additional parameters already merged by upper layers.
    /// 已由上层合并完成的可选 Provider 专用附加参数。
    pub(crate) additional_params: Option<Value>,
    /// Optional provider-facing context injection overlay for the current turn.
    /// 当前回合可选的 Provider 可见上下文注入覆盖层。
    pub(crate) context_injection: Option<CoreProviderContextInjection>,
    /// Optional model-scoped interleaved reasoning capability reused by OpenAI-compatible transports.
    /// 供 OpenAI 兼容传输链路复用的可选模型级交错推理能力描述。
    pub(crate) interleaved_reasoning: Option<ModelInterleavedCapability>,
    /// Optional request-budget guard evaluated before the exact request is dispatched.
    /// 在精确请求分发前执行的可选请求预算守卫。
    pub(crate) request_budget_guard: Option<ProviderRequestBudgetGuard>,
    /// Runtime retry policy applied around host-controlled provider attempts.
    /// 包裹宿主控制 Provider 尝试的运行时重试策略。
    pub(crate) retry_policy: ProviderRetryPolicy,
    /// Optional host intervention invoked at replay-safe tool boundaries.
    /// 在可回放安全工具边界调用的可选宿主干预钩子。
    pub(crate) prompt_intervention: Option<RuntimePromptIntervention>,
}

/// Builds one owned provider transport execution request from already resolved runtime inputs.
/// 根据已经解析完成的运行时输入构建一份自有的 Provider 传输执行请求。
pub(crate) fn build_provider_transport_execution_request(
    runtime_kind: ProviderRuntimeKind,
    provider_id: String,
    runtime_label: String,
    api_key: String,
    base_url: Option<&str>,
    model_id: &str,
    execution_mode: &str,
    system_preamble: &str,
    prompt: &str,
    request_history: Vec<CoreConversationMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: HeaderMap,
    task_runtime: Arc<TaskRuntime>,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
    context_injection: Option<CoreProviderContextInjection>,
    interleaved_reasoning: Option<ModelInterleavedCapability>,
    request_budget_guard: Option<ProviderRequestBudgetGuard>,
    retry_policy: ProviderRetryPolicy,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> ProviderTransportExecutionRequest {
    ProviderTransportExecutionRequest {
        api_family: runtime_kind.api_family(),
        runtime_kind,
        provider_id,
        runtime_label,
        api_key,
        base_url: base_url.map(ToString::to_string),
        model_id: model_id.to_string(),
        execution_mode: execution_mode.to_string(),
        system_preamble: system_preamble.to_string(),
        prompt: prompt.to_string(),
        request_history,
        core_snapshot,
        temperature,
        max_turns,
        headers,
        task_runtime,
        tools_manager,
        session_id,
        resolved_tools,
        additional_params,
        context_injection,
        interleaved_reasoning,
        request_budget_guard,
        retry_policy,
        prompt_intervention,
    }
}
