//! Provider runtime dispatch helpers that resolve runtime inputs and launch the host-controlled transport stream.
//! 负责解析运行时输入并启动宿主控制传输流的 Provider 运行时分发辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::provider_runtime_inputs::one_shot_provider_error_stream;
use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport::build_provider_transport_execution_request;
use crate::agent_runtime::provider_transport_request::ProviderTransportTurnStartMode;
use crate::agent_runtime::provider_transport_request_budget::ProviderRequestBudgetGuard;
use crate::agent_runtime::provider_transport_retry::ProviderRetryPolicy;
use crate::agent_runtime::runtime_stream_intervention::RuntimePromptIntervention;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::provider_config::ProviderConfigurationFile;
use async_stream::stream;
use futures_util::StreamExt;
use std::sync::Arc;
use vulcan_ai_contract::{ModelParameterValue, ReasoningEffort};
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;
use vulcan_provider_adapters::{ResolvedPublicAiExecution, SAMPLING_TEMPERATURE_PARAMETER_ID};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Safe runtime default turn budget used when no explicit max-turn override is configured.
/// 未配置显式最大轮次覆盖时使用的安全运行时默认轮次预算。
pub(crate) const DEFAULT_RUNTIME_MAX_TURNS_LIMIT: usize = usize::MAX - 1;

/// Keeps the host-controlled provider dispatch loop unlimited for every agent configuration.
/// 为所有智能体配置保持宿主控制 Provider 分发循环无限。
pub(crate) fn resolve_runtime_default_max_turns_limit(
    _active_agent: Option<&RuntimeAgentDefinition>,
) -> usize {
    DEFAULT_RUNTIME_MAX_TURNS_LIMIT
}

/// Applies one runtime temperature override through the selected model's public parameter contract.
/// 通过选中模型的公共参数契约应用一次运行时温度覆盖。
///
/// # Parameters
/// # 参数
/// `execution` is the immutable public execution selection to update.
/// `execution` 是待更新的不可变公共执行选择。
/// `requested_temperature` is the agent or workbench override before provider normalization.
/// `requested_temperature` 是供应商归一化前来自智能体或工作台的覆盖值。
///
/// # Returns
/// # 返回值
/// Returns success after validation and insertion, or an explicit unsupported/value error.
/// 校验并插入后返回成功，否则返回显式的不支持或数值错误。
fn apply_runtime_temperature_override(
    execution: &mut ResolvedPublicAiExecution,
    requested_temperature: Option<f32>,
) -> Result<(), String> {
    let Some(requested_temperature) = requested_temperature else {
        return Ok(());
    };
    let definition = execution
        .model
        .capabilities
        .model_parameters
        .iter()
        .find(|definition| definition.id == SAMPLING_TEMPERATURE_PARAMETER_ID)
        .ok_or_else(|| {
            format!(
                "Model `{}` does not expose the registered sampling temperature parameter.",
                execution.model.id
            )
        })?;
    let protocol_id = execution.route.protocol_route.protocol_profile_id.as_str();
    if !definition.protocol_ids.is_empty()
        && !definition
            .protocol_ids
            .iter()
            .any(|candidate| candidate == protocol_id)
    {
        return Err(format!(
            "Model parameter `{SAMPLING_TEMPERATURE_PARAMETER_ID}` is not registered for protocol `{protocol_id}`."
        ));
    }
    let value = ModelParameterValue::Decimal(requested_temperature.to_string());
    definition.validate_value(&value)?;
    execution
        .overrides
        .model_parameters
        .insert(String::from(SAMPLING_TEMPERATURE_PARAMETER_ID), value);
    Ok(())
}

/// Resolves and validates the workbench reasoning preference against provider metadata.
/// 根据供应商元数据解析并校验工作台推理偏好。
///
/// # Parameters
/// 参数
/// `execution` is the immutable selected model and `requested` is the request-scoped workbench
/// preference.
/// `execution` 是不可变的已选模型，`requested` 是请求作用域工作台偏好。
///
/// # Returns
/// 返回值
/// No explicit override for `Off`, or one supported provider-neutral effort level.
/// `Off` 返回无显式覆盖，否则返回一个受支持的供应商无关推理档位。
fn resolve_runtime_reasoning_effort(
    execution: &ResolvedPublicAiExecution,
    requested: crate::provider_config::ReasoningEffortLevel,
) -> Result<Option<ReasoningEffort>, String> {
    let effort = match requested {
        crate::provider_config::ReasoningEffortLevel::Off => return Ok(None),
        crate::provider_config::ReasoningEffortLevel::None => ReasoningEffort::None,
        crate::provider_config::ReasoningEffortLevel::Adaptive => ReasoningEffort::Adaptive,
        crate::provider_config::ReasoningEffortLevel::Minimal => ReasoningEffort::Minimal,
        crate::provider_config::ReasoningEffortLevel::Low => ReasoningEffort::Low,
        crate::provider_config::ReasoningEffortLevel::Medium => ReasoningEffort::Medium,
        crate::provider_config::ReasoningEffortLevel::High => ReasoningEffort::High,
        crate::provider_config::ReasoningEffortLevel::XHigh => ReasoningEffort::XHigh,
        crate::provider_config::ReasoningEffortLevel::Max => ReasoningEffort::Max,
        crate::provider_config::ReasoningEffortLevel::Ultra => ReasoningEffort::Ultra,
    };
    if execution
        .model
        .capabilities
        .reasoning_efforts
        .contains(&effort)
    {
        Ok(Some(effort))
    } else {
        // A saved workbench preference can outlive the selected model. The catalog remains
        // authoritative, so an unsupported preference means provider-default reasoning.
        // 已保存的工作台偏好可能比所选模型存活更久。目录保持权威，因此不受支持的偏好表示使用供应商默认推理。
        Ok(None)
    }
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
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
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
        public_ai_runtime,
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
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
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
        public_ai_runtime,
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
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
    capture_only: bool,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> ProviderDeltaStream {
    let Some(public_ai_runtime) = public_ai_runtime else {
        return one_shot_provider_error_stream(
            "The service has not installed the public AI runtime.",
        );
    };
    let configuration = match public_ai_runtime.configuration_snapshot() {
        Ok(configuration) => configuration,
        Err(error) => return one_shot_provider_error_stream(error),
    };
    // Public execution owns the secret; the legacy transport request field is intentionally empty.
    // 公共执行拥有秘钥；旧传输请求字段刻意保持为空。
    // requested_temperature preserves the existing precedence where an active agent override wins over the workbench value.
    // requested_temperature 保留活动智能体覆盖优先于工作台值的既有优先级。
    let requested_temperature = active_agent
        .and_then(|agent| agent.temperature)
        .or(preparation.temperature);
    let temperature = requested_temperature.map(f64::from);
    // maxTurns remains unlimited so only the model's no-tool response ends the provider loop.
    // maxTurns 保持无限，因此只有模型返回无工具调用的响应才会结束 Provider 循环。
    let max_turns = resolve_runtime_default_max_turns_limit(active_agent);
    let model_id = preparation.model_blueprint.model_id.as_str();
    let request_budget_guard = (!capture_only).then(|| {
        ProviderRequestBudgetGuard::new(
            preparation.model_blueprint.provider_id.clone(),
            preparation.model_blueprint.model_id.clone(),
            preparation.model_blueprint.protocol_profile_id.clone(),
            preparation.model_blueprint.public_capabilities.clone(),
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

    let transport_request = build_provider_transport_execution_request(
        preparation.model_blueprint.provider_id.clone(),
        model_id,
        preparation.workspace_path.as_str(),
        &preparation.execution_mode,
        system_preamble,
        prompt,
        turn_start_mode,
        core_snapshot,
        temperature,
        max_turns,
        task_runtime,
        tools_manager,
        preparation.session_id,
        resolved_tools,
        preparation.context_injection.clone(),
        request_budget_guard,
        retry_policy,
        prompt_intervention,
        preparation.tool_request_rejection_reason.clone(),
        preparation.tool_request_rejection_all_tools,
        preparation.tool_request_rejection_tool_ids.clone(),
    );
    let provider_id = preparation.model_blueprint.provider_id.clone();
    let model_id = preparation.model_blueprint.model_id.clone();
    let reasoning_effort = preparation.model_blueprint.reasoning.requested_effort;
    Box::pin(stream! {
        let mut public_execution = match public_ai_runtime
            .resolve_model_execution(&configuration, provider_id.as_str(), model_id.as_str())
            .await
        {
            Ok(execution) => execution,
            Err(error) => {
                yield Err(error);
                return;
            }
        };
        if let Err(error) =
            apply_runtime_temperature_override(&mut public_execution, requested_temperature)
        {
            yield Err(error);
            return;
        }
        let reasoning_effort =
            match resolve_runtime_reasoning_effort(&public_execution, reasoning_effort) {
                Ok(effort) => effort,
                Err(error) => {
                    yield Err(error);
                    return;
                }
            };
        let mut provider_stream = if capture_only {
            crate::agent_runtime::provider_transport_vulcan::capture_vulcan_public_stream(
                transport_request,
                public_execution,
                reasoning_effort,
            )
        } else {
            crate::agent_runtime::provider_transport_vulcan::execute_vulcan_public_stream(
                transport_request,
                public_execution,
                reasoning_effort,
            )
        };
        while let Some(delta) = provider_stream.next().await {
            yield delta;
        }
    })
}
