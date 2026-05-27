//! Provider runtime dispatch helpers that resolve runtime inputs and launch the host-controlled transport stream.
//! 负责解析运行时输入并启动宿主控制传输流的 Provider 运行时分发辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_runtime_inputs::{
    build_provider_request_headers, one_shot_provider_error_stream,
    resolve_provider_additional_params, resolve_provider_options,
};
use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport::{
    build_provider_transport_execution_request, execute_provider_transport_stream,
};
use crate::agent_runtime::provider_transport_wire_capture::{
    PROVIDER_REQUEST_CAPTURE_ONLY_API_KEY, provider_request_dry_run_enabled,
};
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::provider_capabilities::resolve_provider_api_key;
use crate::provider_config::ProviderConfigurationFile;
use std::sync::Arc;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage,
    ConversationSnapshot as CoreConversationSnapshot,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Safe runtime default turn budget used when no explicit max-turn override is configured.
/// 未配置显式最大轮次覆盖时使用的安全运行时默认轮次预算。
pub(crate) const DEFAULT_RUNTIME_MAX_TURNS_LIMIT: usize = usize::MAX - 1;

/// Resolves one effective max-turn budget for the host-controlled provider dispatch path.
/// 为宿主控制的 Provider 分发路径解析一份生效中的最大轮次预算。
pub(crate) fn resolve_runtime_default_max_turns_limit(
    active_agent: Option<&RuntimeAgentDefinition>,
) -> usize {
    active_agent
        .and_then(|agent| agent.max_turns)
        .filter(|value| *value > 0)
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_RUNTIME_MAX_TURNS_LIMIT)
}

/// Executes one provider prompt against the currently selected runtime family and returns a delta stream.
/// 针对当前选中的运行时族执行一次 Provider 提示词调用，并返回增量流。
pub(crate) fn execute_provider_prompt_stream(
    config: &ProviderConfigurationFile,
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
    system_preamble: &str,
    prompt: &str,
    request_history: Vec<CoreConversationMessage>,
    core_snapshot: CoreConversationSnapshot,
    task_runtime: Arc<TaskRuntime>,
    tools_manager: VulcanToolsManager,
    resolved_tools: Vec<ResolvedToolDefinition>,
) -> ProviderDeltaStream {
    execute_provider_prompt_stream_with_capture_mode(
        config,
        preparation,
        active_agent,
        system_preamble,
        prompt,
        request_history,
        core_snapshot,
        task_runtime,
        tools_manager,
        resolved_tools,
        false,
    )
}

/// Executes one provider prompt in capture-only mode and returns the final request as stream data without network dispatch.
/// 以仅捕获模式执行一次 Provider 提示词调用，通过流数据返回最终请求且不进行网络发送。
pub(crate) fn execute_provider_prompt_capture_stream(
    config: &ProviderConfigurationFile,
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
    system_preamble: &str,
    prompt: &str,
    request_history: Vec<CoreConversationMessage>,
    core_snapshot: CoreConversationSnapshot,
    task_runtime: Arc<TaskRuntime>,
    tools_manager: VulcanToolsManager,
    resolved_tools: Vec<ResolvedToolDefinition>,
) -> ProviderDeltaStream {
    execute_provider_prompt_stream_with_capture_mode(
        config,
        preparation,
        active_agent,
        system_preamble,
        prompt,
        request_history,
        core_snapshot,
        task_runtime,
        tools_manager,
        resolved_tools,
        true,
    )
}

/// Executes one provider prompt with an explicit request-capture switch shared by real and simulated paths.
/// 使用真实路径与模拟路径共享的显式请求捕获开关执行一次 Provider 提示词调用。
fn execute_provider_prompt_stream_with_capture_mode(
    config: &ProviderConfigurationFile,
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
    system_preamble: &str,
    prompt: &str,
    request_history: Vec<CoreConversationMessage>,
    core_snapshot: CoreConversationSnapshot,
    task_runtime: Arc<TaskRuntime>,
    tools_manager: VulcanToolsManager,
    resolved_tools: Vec<ResolvedToolDefinition>,
    capture_only: bool,
) -> ProviderDeltaStream {
    let options = match resolve_provider_options(config, &preparation.model_blueprint.provider_id) {
        Ok(options) => options,
        Err(error) => return one_shot_provider_error_stream(error),
    };
    let api_key = if capture_only {
        PROVIDER_REQUEST_CAPTURE_ONLY_API_KEY.to_string()
    } else {
        match resolve_provider_api_key(options, &preparation.model_blueprint.capabilities) {
            Ok(Some(api_key)) => api_key,
            Ok(None) if provider_request_dry_run_enabled() => {
                PROVIDER_REQUEST_CAPTURE_ONLY_API_KEY.to_string()
            }
            Ok(None) => {
                return one_shot_provider_error_stream(format!(
                    "供应商 `{}` 当前缺少可用 API Key，无法启动真实执行。",
                    preparation.model_blueprint.provider_name
                ));
            }
            Err(error) => return one_shot_provider_error_stream(error),
        }
    };
    let temperature = active_agent
        .and_then(|agent| agent.temperature)
        .or(preparation.temperature)
        .map(|value| value.clamp(0.0, 2.0) as f64)
        .filter(|_| preparation.model_blueprint.supports_temperature());
    let max_turns = preparation
        .prompt_purpose
        .max_turns_override()
        .unwrap_or_else(|| resolve_runtime_default_max_turns_limit(active_agent));
    let model_id = preparation.model_blueprint.model_id.as_str();
    let base_url = preparation.model_blueprint.base_url.as_deref();
    let request_headers = match build_provider_request_headers(&preparation.model_blueprint) {
        Ok(headers) => headers,
        Err(error) => return one_shot_provider_error_stream(error),
    };
    let additional_params = resolve_provider_additional_params(&preparation.model_blueprint);

    execute_provider_transport_stream(build_provider_transport_execution_request(
        preparation.model_blueprint.runtime_kind,
        preparation.model_blueprint.provider_id.clone(),
        preparation.model_blueprint.runtime_label.clone(),
        api_key,
        base_url,
        model_id,
        system_preamble,
        prompt,
        request_history,
        core_snapshot,
        temperature,
        max_turns,
        request_headers,
        task_runtime,
        tools_manager,
        preparation.session_id,
        resolved_tools,
        additional_params,
        preparation.model_blueprint.interleaved_reasoning.clone(),
    ))
}
