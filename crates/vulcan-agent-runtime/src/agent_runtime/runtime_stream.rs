//! Runtime streaming entrypoint for provider execution and final report assembly.
//! Provider 执行与最终报告组装的运行时流式入口模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::ExtensionGeneration;
use crate::agent_runtime::events::ExecutionStreamEvent;
use crate::agent_runtime::execution_models::ExecutionPreparationRequest;
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime_stream_driver::drive_runtime_stream_launch;
use crate::agent_runtime::runtime_stream_intervention::RuntimePromptIntervention;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::{McpRuntimeAccess, configured_mcp_runtime_access};
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::CommandSessionRuntime;
use async_stream::stream;
use futures_util::{Stream, StreamExt};
use std::sync::Arc;

/// Streams one real provider run and yields preparation, thinking, output, and final-report events incrementally.
/// 以增量方式流式执行一次真实 Provider 调用，并依次产出准备、思考、输出与最终报告事件。
pub fn stream_runtime_run(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeAccess>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    request: ExecutionPreparationRequest,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream_runtime_run_with_intervention(
        config,
        agent_profile_catalog,
        luaskills_runtime,
        mcp_runtime,
        task_runtime,
        question_runtime,
        command_session_runtime,
        token_budget_runtime,
        request,
        None,
    )
}

/// Streams one provider run while allowing a host to inject guidance at safe tool boundaries.
/// 流式执行一次 Provider 调用，并允许宿主在安全工具边界注入引导。
pub fn stream_runtime_run_with_intervention(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeAccess>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    request: ExecutionPreparationRequest,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    let mcp_runtime = mcp_runtime.unwrap_or_else(|| configured_mcp_runtime_access(&config));
    let extension_generation = Arc::new(ExtensionGeneration::new(0, None, mcp_runtime));
    stream_runtime_run_with_intervention_and_plugins(
        config,
        agent_profile_catalog,
        luaskills_runtime,
        None,
        Some(extension_generation),
        task_runtime,
        question_runtime,
        command_session_runtime,
        token_budget_runtime,
        request,
        prompt_intervention,
    )
}

/// Streams one provider run with independently owned LuaSkills and Vulcan Plugin runtimes.
/// 使用相互独立持有的 LuaSkills 与 Vulcan 插件运行时流式执行一次 Provider 调用。
pub fn stream_runtime_run_with_intervention_and_plugins(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    vulcan_plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    extension_generation: Option<Arc<ExtensionGeneration>>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    request: ExecutionPreparationRequest,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    let extension_generation = extension_generation.unwrap_or_else(|| {
        Arc::new(ExtensionGeneration::new(
            0,
            vulcan_plugin_runtime
                .as_ref()
                .map(vulcan_plugin_runtime::VulcanPluginRuntimeManager::snapshot),
            configured_mcp_runtime_access(&config),
        ))
    });
    stream_runtime_run_with_access(
        config,
        agent_profile_catalog,
        luaskills_runtime,
        vulcan_plugin_runtime,
        extension_generation,
        task_runtime,
        question_runtime,
        command_session_runtime,
        token_budget_runtime,
        request,
        prompt_intervention,
    )
}

/// Streams one provider run from a launch payload that has already completed preparation.
/// 从一份已经完成准备的启动载荷继续流式执行一次 Provider 调用。
///
/// # Parameters
/// # 参数
/// `config` is the provider configuration used for the prepared launch.
/// `config` 是该已准备启动载荷使用的 Provider 配置。
/// `launch` is the prepared runtime-stream payload.
/// `launch` 是已准备完成的运行时流启动载荷。
/// `prompt_intervention` optionally injects host guidance at safe tool boundaries.
/// `prompt_intervention` 可选地在安全工具边界注入宿主引导。
///
/// # Returns
/// # 返回值
/// A stream of execution events for the prepared provider run.
/// 返回该已准备 Provider 执行的执行事件流。
/// Streams one real provider run from an already prepared MCP runtime access surface.
/// 根据一份已经准备好的 MCP 运行时访问面流式执行一次真实 Provider 调用。
fn stream_runtime_run_with_access(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    vulcan_plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    extension_generation: Arc<ExtensionGeneration>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    request: ExecutionPreparationRequest,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream! {
        let launch = match crate::agent_runtime::runtime_stream_launch::prepare_runtime_stream_launch_with_plugins(
            &config,
            &agent_profile_catalog,
            luaskills_runtime.clone(),
            vulcan_plugin_runtime.clone(),
            extension_generation,
            task_runtime,
            question_runtime,
            command_session_runtime,
            token_budget_runtime,
            request,
        ).await {
            Ok(launch) => launch,
            Err(report) => {
                yield ExecutionStreamEvent::Finished(report);
                return;
            }
        };
        let runtime_stream = drive_runtime_stream_launch(config, launch, prompt_intervention);
        futures_util::pin_mut!(runtime_stream);
        while let Some(event) = runtime_stream.next().await {
            yield event;
        }
    }
}
