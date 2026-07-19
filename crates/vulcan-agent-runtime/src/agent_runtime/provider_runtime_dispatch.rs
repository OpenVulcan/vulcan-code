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
use crate::agent_runtime::provider_transport_request::ProviderTransportTurnStartMode;
use crate::agent_runtime::provider_transport_request_budget::ProviderRequestBudgetGuard;
use crate::agent_runtime::provider_transport_retry::ProviderRetryPolicy;
use crate::agent_runtime::provider_transport_wire_capture::{
    PROVIDER_REQUEST_CAPTURE_ONLY_API_KEY, provider_request_dry_run_enabled,
};
use crate::agent_runtime::runtime_stream_intervention::RuntimePromptIntervention;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::provider_capabilities::resolve_provider_api_key;
use crate::provider_config::ProviderConfigurationFile;
use std::sync::Arc;
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Safe runtime default turn budget used when no explicit max-turn override is configured.
/// 未配置显式最大轮次覆盖时使用的安全运行时默认轮次预算。
pub(crate) const DEFAULT_RUNTIME_MAX_TURNS_LIMIT: usize = usize::MAX - 1;

/// Alibaba Token Plan provider identifier that owns the Qwen 3.8 temperature floor contract.
/// 持有千问 3.8 温度下限契约的阿里云 Token Plan 供应商标识。
const ALIBABA_TOKEN_PLAN_CN_PROVIDER_ID: &str = "alibaba-token-plan-cn";

/// Qwen 3.8 model identifier whose always-on reasoning mode requires temperature 0.6 or higher.
/// 始终开启思考模式且要求温度不低于 0.6 的千问 3.8 模型标识。
const QWEN_3_8_MAX_PREVIEW_MODEL_ID: &str = "qwen3.8-max-preview";

/// Minimum and default temperature required by Qwen 3.8 Max Preview reasoning mode.
/// 千问 3.8 Max Preview 思考模式要求的最低及默认温度。
const QWEN_3_8_REASONING_TEMPERATURE: f64 = 0.6;

/// Keeps the host-controlled provider dispatch loop unlimited for every agent configuration.
/// 为所有智能体配置保持宿主控制 Provider 分发循环无限。
pub(crate) fn resolve_runtime_default_max_turns_limit(
    _active_agent: Option<&RuntimeAgentDefinition>,
) -> usize {
    DEFAULT_RUNTIME_MAX_TURNS_LIMIT
}

/// Normalizes one temperature request according to shared bounds and exact provider/model compatibility contracts.
/// 根据共享边界与精确供应商/模型兼容契约归一化一次温度请求。
///
/// # Parameters
/// # 参数
/// `provider_id` identifies the selected provider definition.
/// `provider_id` 标识当前选中的供应商定义。
/// `model_id` identifies the selected model definition.
/// `model_id` 标识当前选中的模型定义。
/// `requested_temperature` is the agent or workbench override before provider normalization.
/// `requested_temperature` 是供应商归一化前来自智能体或工作台的覆盖值。
/// `supports_temperature` indicates whether the model accepts a temperature field.
/// `supports_temperature` 表示模型是否接受温度字段。
///
/// # Returns
/// # 返回值
/// Returns the effective temperature sent to the provider, or `None` when the model does not support temperature.
/// 返回发送给供应商的生效温度；模型不支持温度时返回 `None`。
fn normalize_provider_temperature(
    provider_id: &str,
    model_id: &str,
    requested_temperature: Option<f32>,
    supports_temperature: bool,
) -> Option<f64> {
    if !supports_temperature {
        return None;
    }

    // bounded_temperature keeps the existing provider-wide safety range before applying a stricter model floor.
    // bounded_temperature 在应用更严格的模型下限前保留既有的供应商通用安全范围。
    let bounded_temperature = requested_temperature.map(|value| f64::from(value).clamp(0.0, 2.0));
    if provider_id == ALIBABA_TOKEN_PLAN_CN_PROVIDER_ID && model_id == QWEN_3_8_MAX_PREVIEW_MODEL_ID
    {
        return Some(
            bounded_temperature
                .unwrap_or(QWEN_3_8_REASONING_TEMPERATURE)
                .max(QWEN_3_8_REASONING_TEMPERATURE),
        );
    }

    bounded_temperature
}

/// Executes one provider prompt against the currently selected runtime family and returns a delta stream.
/// 针对当前选中的运行时族执行一次 Provider 提示词调用，并返回增量流。
pub(crate) fn execute_provider_prompt_stream(
    config: &ProviderConfigurationFile,
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
    system_preamble: &str,
    prompt: &str,
    turn_start_mode: ProviderTransportTurnStartMode,
    core_snapshot: CoreConversationSnapshot,
    task_runtime: Arc<TaskRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    tools_manager: VulcanToolsManager,
    resolved_tools: Vec<ResolvedToolDefinition>,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> ProviderDeltaStream {
    execute_provider_prompt_stream_with_capture_mode(
        config,
        preparation,
        active_agent,
        system_preamble,
        prompt,
        turn_start_mode,
        core_snapshot,
        task_runtime,
        token_budget_runtime,
        tools_manager,
        resolved_tools,
        false,
        prompt_intervention,
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
    turn_start_mode: ProviderTransportTurnStartMode,
    core_snapshot: CoreConversationSnapshot,
    task_runtime: Arc<TaskRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    tools_manager: VulcanToolsManager,
    resolved_tools: Vec<ResolvedToolDefinition>,
) -> ProviderDeltaStream {
    execute_provider_prompt_stream_with_capture_mode(
        config,
        preparation,
        active_agent,
        system_preamble,
        prompt,
        turn_start_mode,
        core_snapshot,
        task_runtime,
        token_budget_runtime,
        tools_manager,
        resolved_tools,
        true,
        None,
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
    turn_start_mode: ProviderTransportTurnStartMode,
    core_snapshot: CoreConversationSnapshot,
    task_runtime: Arc<TaskRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    tools_manager: VulcanToolsManager,
    resolved_tools: Vec<ResolvedToolDefinition>,
    capture_only: bool,
    prompt_intervention: Option<RuntimePromptIntervention>,
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
    // requested_temperature preserves the existing precedence where an active agent override wins over the workbench value.
    // requested_temperature 保留活动智能体覆盖优先于工作台值的既有优先级。
    let requested_temperature = active_agent
        .and_then(|agent| agent.temperature)
        .or(preparation.temperature);
    // temperature applies exact provider/model normalization after the final model identity is known.
    // temperature 在最终模型身份确定后应用精确的供应商/模型归一化。
    let temperature = normalize_provider_temperature(
        preparation.model_blueprint.provider_id.as_str(),
        preparation.model_blueprint.model_id.as_str(),
        requested_temperature,
        preparation.model_blueprint.supports_temperature(),
    );
    // maxTurns remains unlimited so only the model's no-tool response ends the provider loop.
    // maxTurns 保持无限，因此只有模型返回无工具调用的响应才会结束 Provider 循环。
    let max_turns = resolve_runtime_default_max_turns_limit(active_agent);
    let model_id = preparation.model_blueprint.model_id.as_str();
    let base_url = preparation.model_blueprint.base_url.as_deref();
    let request_headers = match build_provider_request_headers(&preparation.model_blueprint) {
        Ok(headers) => headers,
        Err(error) => return one_shot_provider_error_stream(error),
    };
    let additional_params = resolve_provider_additional_params(&preparation.model_blueprint);
    let request_budget_guard = (!capture_only).then(|| {
        ProviderRequestBudgetGuard::new(
            preparation.model_blueprint.provider_id.clone(),
            preparation.model_blueprint.model_id.clone(),
            preparation.model_blueprint.api_family(),
            preparation.model_blueprint.capabilities.clone(),
            preparation.model_blueprint.reasoning.clone(),
            preparation.context_compaction_policy,
            preparation.context_budget_override,
            token_budget_runtime,
        )
    });
    let retry_policy = if capture_only {
        ProviderRetryPolicy::disabled()
    } else {
        ProviderRetryPolicy::from_config(&config.retry)
    };
    if let Some(task_session_store_root) = preparation.task_session_store_root.clone()
        && let Err(error) = task_runtime
            .configure_task_session_store_root(preparation.session_id, task_session_store_root)
    {
        eprintln!(
            "[task-session-store] configure store root failed for session {}: {error}",
            preparation.session_id
        );
    }

    execute_provider_transport_stream(build_provider_transport_execution_request(
        preparation.model_blueprint.runtime_kind,
        preparation.model_blueprint.provider_id.clone(),
        preparation.model_blueprint.runtime_label.clone(),
        api_key,
        base_url,
        model_id,
        &preparation.execution_mode,
        system_preamble,
        prompt,
        turn_start_mode,
        core_snapshot,
        temperature,
        max_turns,
        request_headers,
        task_runtime,
        tools_manager,
        preparation.session_id,
        resolved_tools,
        additional_params,
        preparation.context_injection.clone(),
        preparation.model_blueprint.interleaved_reasoning.clone(),
        request_budget_guard,
        retry_policy,
        prompt_intervention,
        preparation.tool_request_rejection_reason.clone(),
        preparation.tool_request_rejection_all_tools,
        preparation.tool_request_rejection_tool_ids.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies Qwen 3.8 Token Plan requests default to 0.6, raise lower values, preserve higher values, and keep unrelated providers unchanged.
    /// 验证 Token Plan 千问 3.8 请求默认使用 0.6、抬升更低值、保留更高值，并保持无关供应商行为不变。
    #[test]
    fn token_plan_qwen_temperature_contract_is_applied_exactly() {
        assert_eq!(
            normalize_provider_temperature(
                ALIBABA_TOKEN_PLAN_CN_PROVIDER_ID,
                QWEN_3_8_MAX_PREVIEW_MODEL_ID,
                None,
                true,
            ),
            Some(0.6),
        );
        assert_eq!(
            normalize_provider_temperature(
                ALIBABA_TOKEN_PLAN_CN_PROVIDER_ID,
                QWEN_3_8_MAX_PREVIEW_MODEL_ID,
                Some(0.2),
                true,
            ),
            Some(0.6),
        );
        // higher_temperature remains represented through the caller's f32 contract, so compare its semantic value with a tight tolerance.
        // higher_temperature 仍通过调用方的 f32 契约表示，因此使用严格容差比较其语义值。
        let higher_temperature = normalize_provider_temperature(
            ALIBABA_TOKEN_PLAN_CN_PROVIDER_ID,
            QWEN_3_8_MAX_PREVIEW_MODEL_ID,
            Some(0.9),
            true,
        )
        .expect("supported Qwen 3.8 temperature must be present");
        assert!((higher_temperature - 0.9).abs() < 1e-6);
        assert_eq!(
            normalize_provider_temperature("alibaba-coding-plan-cn", "qwen3.7-max", None, true),
            None,
        );
        assert_eq!(
            normalize_provider_temperature(
                ALIBABA_TOKEN_PLAN_CN_PROVIDER_ID,
                QWEN_3_8_MAX_PREVIEW_MODEL_ID,
                Some(0.9),
                false,
            ),
            None,
        );
    }
}
