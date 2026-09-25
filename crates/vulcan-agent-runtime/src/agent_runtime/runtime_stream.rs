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
    request: ExecutionPreparationRequest,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    let mcp_runtime = mcp_runtime.unwrap_or_else(|| configured_mcp_runtime_access(&config));
    let extension_generation = Arc::new(ExtensionGeneration::new(0, None, mcp_runtime, None));
    stream_runtime_run_with_intervention_and_plugins(
        config,
        agent_profile_catalog,
        luaskills_runtime,
        None,
        Some(extension_generation),
        task_runtime,
        question_runtime,
        command_session_runtime,
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
    request: ExecutionPreparationRequest,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream_runtime_run_with_intervention_plugins_and_public_ai(
        config,
        agent_profile_catalog,
        luaskills_runtime,
        vulcan_plugin_runtime,
        extension_generation,
        task_runtime,
        question_runtime,
        command_session_runtime,
        None,
        request,
        prompt_intervention,
    )
}

/// Streams one provider run with plugins and one service-owned public AI runtime.
/// 使用插件与一个服务持有的公共 AI 运行时流式执行一次 Provider 调用。
#[allow(clippy::too_many_arguments)]
pub fn stream_runtime_run_with_intervention_plugins_and_public_ai(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    vulcan_plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    extension_generation: Option<Arc<ExtensionGeneration>>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
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
            None,
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
        public_ai_runtime,
        request,
        prompt_intervention,
    )
}

/// Lazily prepares and streams one provider run using the resolved runtime access handles.
/// 使用已解析的运行时访问句柄惰性准备并流式执行一次供应商调用。
///
/// # Parameters
/// # 参数
/// The configuration, catalog, runtime handles, and request retain their frozen execution ownership.
/// 配置、目录、运行时句柄与请求保留各自冻结的执行归属。
/// `prompt_intervention` optionally injects host guidance at safe tool boundaries.
/// `prompt_intervention` 可选地在安全工具边界注入宿主引导。
///
/// # Returns
/// # 返回值
/// An owned event stream whose drop cancels its preparation and provider driver together.
/// 返回一个自持有事件流，丢弃时同时取消其准备与供应商驱动。
fn stream_runtime_run_with_access(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    vulcan_plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    extension_generation: Arc<ExtensionGeneration>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
    request: ExecutionPreparationRequest,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    // Box at the producer boundary so host wrappers never embed the concrete stream state.
    // 在生产者边界装箱，避免宿主包装层内嵌具体流状态。
    Box::pin(stream! {
        // Await preparation in this task; dropping the stream must also cancel preparation.
        // 在本任务中等待准备；丢弃流时必须同时取消准备。
        let launch_result = Box::pin(
            crate::agent_runtime::runtime_stream_launch::prepare_runtime_stream_launch_with_plugins_and_public_ai(
                &config,
                &agent_profile_catalog,
                luaskills_runtime,
                vulcan_plugin_runtime,
                extension_generation,
                task_runtime,
                question_runtime,
                command_session_runtime,
                public_ai_runtime,
                request,
            ),
        ).await;
        // Preserve exactly one terminal failure event when preparation rejects the request.
        // 准备拒绝请求时仍然只产生一个终态失败事件。
        let launch = match launch_result {
            Ok(launch) => launch,
            Err(report) => {
                yield ExecutionStreamEvent::Finished(report);
                return;
            }
        };
        // Keep the provider driver on the heap without creating another scheduler task.
        // 将供应商驱动置于堆上，不创建额外调度任务。
        let mut runtime_stream = drive_runtime_stream_launch(config, launch, prompt_intervention);
        while let Some(event) = runtime_stream.next().await {
            yield event;
        }
    })
}
