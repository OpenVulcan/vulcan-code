//! Shared provider transport entry-input preparation helpers reused by provider-specific stream entrypoints.
//! 供各 Provider 专用流式入口复用的共享传输入口输入准备辅助模块。

use crate::agent_runtime::provider_transport_request_budget::ProviderRequestBudgetGuard;
use crate::agent_runtime::provider_transport_retry::ProviderRetryPolicy;
use crate::agent_runtime::provider_transport_tool_context::{
    PreparedHostToolRuntimeContext, prepare_host_tool_runtime_context,
};
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::model::SessionId;
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::sync::Arc;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig,
    ProviderContextInjection as CoreProviderContextInjection,
    ProviderPromptLayers as CoreProviderPromptLayers,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Prepared host-controlled stream inputs shared by provider-specific entrypoints before they build the concrete adapter.
/// 在各 Provider 专用入口构建具体适配器之前复用的宿主控制流输入对象。
pub(crate) struct PreparedProviderTransportEntryInputs {
    /// Stable provider identifier reused by controller config and transport adapters.
    /// 被控制器配置与传输适配器复用的稳定 Provider 标识。
    pub(crate) provider_id: String,
    /// Stable model identifier reused by controller config and transport adapters.
    /// 被控制器配置与传输适配器复用的稳定模型标识。
    pub(crate) model_id: String,
    /// Final outgoing prompt body forwarded into the host-controlled runtime stream.
    /// 被转发到宿主控制运行时流中的最终出站提示词正文。
    pub(crate) prompt: String,
    /// Fully cloned request-header map reused by the concrete transport adapter.
    /// 被具体传输适配器复用的完整克隆请求头映射。
    pub(crate) request_headers: HeaderMap,
    /// Prebuilt controller configuration handed to `vulcan-core`.
    /// 交给 `vulcan-core` 的预构建控制器配置。
    pub(crate) controller_config: CoreControllerConfig,
}

/// Prepared host-controlled runtime inputs shared by provider-specific entrypoints after both controller config assembly and tool-runtime preparation complete.
/// 在控制器配置装配与工具运行时准备都完成后，供各 Provider 专用入口复用的宿主控制运行时输入对象。
pub(crate) struct PreparedProviderTransportRuntimeInputs {
    /// Shared provider transport entry inputs reused by concrete adapter constructors.
    /// 被具体适配器构造过程复用的共享 Provider 传输入口输入对象。
    pub(crate) entry_inputs: PreparedProviderTransportEntryInputs,
    /// Shared host-managed tool runtime context reused by concrete adapter constructors.
    /// 被具体适配器构造过程复用的共享宿主管理工具运行时上下文。
    pub(crate) tool_context: PreparedHostToolRuntimeContext,
    /// Optional request-budget guard evaluated after exact request assembly and before provider dispatch.
    /// 在精确请求装配后、Provider 分发前执行的可选请求预算守卫。
    pub(crate) request_budget_guard: Option<ProviderRequestBudgetGuard>,
    /// Runtime retry policy applied around provider attempts.
    /// 包裹 Provider 尝试的运行时重试策略。
    pub(crate) retry_policy: ProviderRetryPolicy,
    /// Optional reason used to reject host-managed tool requests before execution.
    /// 在执行前拒绝宿主管理工具请求时使用的可选原因。
    pub(crate) tool_request_rejection_reason: Option<String>,
    /// Whether the rejection reason applies to every host-managed tool.
    /// 拒绝原因是否应用到全部宿主管理工具。
    pub(crate) tool_request_rejection_all_tools: bool,
    /// Optional scoped tool ids rejected with the rejection reason.
    /// 使用拒绝原因拒绝的可选限定工具标识。
    pub(crate) tool_request_rejection_tool_ids: Vec<String>,
}

/// Prepares the shared provider transport entry inputs from lightweight borrowed arguments.
/// 根据轻量借用参数准备共享 Provider 传输入口输入对象。
pub(crate) fn prepare_provider_transport_entry_inputs(
    provider_id: &str,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    additional_params: Option<Value>,
    context_injection: Option<CoreProviderContextInjection>,
) -> PreparedProviderTransportEntryInputs {
    let provider_id = provider_id.to_string();
    let model_id = model_id.to_string();

    PreparedProviderTransportEntryInputs {
        provider_id: provider_id.clone(),
        model_id: model_id.clone(),
        prompt: prompt.to_string(),
        request_headers: headers.clone(),
        controller_config: CoreControllerConfig {
            provider_id,
            model_id,
            system_prompt: system_preamble.to_string(),
            prompt_layers: CoreProviderPromptLayers::from_system_prompt(system_preamble),
            temperature,
            max_turns,
            additional_params,
            context_injection,
        },
    }
}

/// Prepares the full shared runtime inputs for one host-controlled provider stream, including controller config plus host tool runtime state.
/// 为一次宿主控制 Provider 流准备完整共享运行时输入，包括控制器配置与宿主工具运行时状态。
pub(crate) fn prepare_provider_transport_runtime_inputs(
    provider_id: &str,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    additional_params: Option<Value>,
    context_injection: Option<CoreProviderContextInjection>,
    tools_manager: VulcanToolsManager,
    resolved_tools: &[ResolvedToolDefinition],
    session_id: SessionId,
    execution_mode: &str,
    task_runtime: Arc<TaskRuntime>,
    request_budget_guard: Option<ProviderRequestBudgetGuard>,
    retry_policy: ProviderRetryPolicy,
    tool_request_rejection_reason: Option<String>,
    tool_request_rejection_all_tools: bool,
    tool_request_rejection_tool_ids: Vec<String>,
) -> PreparedProviderTransportRuntimeInputs {
    PreparedProviderTransportRuntimeInputs {
        entry_inputs: prepare_provider_transport_entry_inputs(
            provider_id,
            model_id,
            system_preamble,
            prompt,
            temperature,
            max_turns,
            headers,
            additional_params,
            context_injection,
        ),
        tool_context: prepare_host_tool_runtime_context(
            tools_manager,
            resolved_tools,
            session_id,
            execution_mode,
            task_runtime,
        ),
        request_budget_guard,
        retry_policy,
        tool_request_rejection_reason,
        tool_request_rejection_all_tools,
        tool_request_rejection_tool_ids,
    }
}

/// Normalizes one optional base URL override by dropping empty strings and returning an owned value when present.
/// 规范化单个可选基础地址覆盖值，在存在时返回拥有所有权的值，并丢弃空字符串。
pub(crate) fn normalize_optional_base_url(base_url: Option<&str>) -> Option<String> {
    base_url
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    //! Provider transport entry-input regression tests.
    //! Provider 传输入口输入回归测试。

    use super::*;

    #[test]
    fn entry_inputs_preserve_context_injection_overlay() {
        let injection = CoreProviderContextInjection {
            persistent_blocks: vec![String::from("<perm>\n固定\n</perm>")],
            transient_prefix: String::from("<temp>\n临时\n</temp>"),
            fingerprint: String::from("fingerprint"),
        };

        let inputs = prepare_provider_transport_entry_inputs(
            "openai",
            "gpt-test",
            "system",
            "prompt",
            Some(0.2),
            4,
            &HeaderMap::new(),
            None,
            Some(injection.clone()),
        );

        let captured = inputs
            .controller_config
            .context_injection
            .expect("context injection should be preserved");
        assert_eq!(captured.persistent_blocks, injection.persistent_blocks);
        assert_eq!(captured.transient_prefix, injection.transient_prefix);
        assert_eq!(captured.fingerprint, injection.fingerprint);
    }
}
