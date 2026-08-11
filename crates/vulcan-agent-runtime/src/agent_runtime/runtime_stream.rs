//! Runtime streaming entrypoint for provider execution and final report assembly.
//! Provider 执行与最终报告组装的运行时流式入口模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::ExtensionGeneration;
use crate::agent_runtime::events::ExecutionStreamEvent;
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime_stream_bootstrap_core::{
    after_precheck_trace, after_precheck_trace_enabled,
};
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
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
    request: ExecutionPreparationRequest,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream! {
        // launchSessionId keeps non-sensitive correlation available after the request moves into the lazy future.
        // launchSessionId 在请求移入惰性 Future 后继续提供非敏感关联标识。
        let launch_session_id = request.session_id;
        // launchResult uses an isolated scheduler task only while tracing so the experiment can distinguish nested poll-stack pressure without changing normal execution semantics.
        // launchResult 仅在追踪时使用独立调度任务，使实验能够区分嵌套轮询栈压力且不改变正常执行语义。
        let launch_result = if after_precheck_trace_enabled() {
            after_precheck_trace(
                "after_precheck_23b0_isolated_launch_future_construct_enter",
                format!("session={launch_session_id}"),
            );
            // isolatedConfig owns the Provider configuration across the diagnostic task boundary.
            // isolatedConfig 跨诊断任务边界持有 Provider 配置。
            let isolated_config = config.clone();
            // isolatedAgentProfileCatalog owns the agent catalog across the diagnostic task boundary.
            // isolatedAgentProfileCatalog 跨诊断任务边界持有智能体目录。
            let isolated_agent_profile_catalog = agent_profile_catalog.clone();
            // isolatedLaunchFuture resets the Tokio poll call stack while retaining the exact launch inputs.
            // isolatedLaunchFuture 在保留精确启动输入的同时重置 Tokio 轮询调用栈。
            let isolated_launch_future = async move {
                crate::agent_runtime::runtime_stream_launch::prepare_runtime_stream_launch_with_plugins_and_public_ai(
                    &isolated_config,
                    &isolated_agent_profile_catalog,
                    luaskills_runtime.clone(),
                    vulcan_plugin_runtime.clone(),
                    extension_generation,
                    task_runtime,
                    question_runtime,
                    command_session_runtime,
                    public_ai_runtime,
                    request,
                )
                .await
            };
            after_precheck_trace(
                "after_precheck_23b1_isolated_launch_future_constructed",
                format!(
                    "session={} future_size_bytes={}",
                    launch_session_id,
                    std::mem::size_of_val(&isolated_launch_future)
                ),
            );
            // boxedIsolatedLaunchFuture moves the oversized concrete Future to the heap before it crosses the Tokio spawn boundary.
            // boxedIsolatedLaunchFuture 在超大具体 Future 跨越 Tokio 启动边界前将其移入堆内存。
            let boxed_isolated_launch_future = Box::pin(isolated_launch_future);
            after_precheck_trace(
                "after_precheck_23b2_isolated_launch_future_boxed",
                format!(
                    "session={} boxed_future_size_bytes={}",
                    launch_session_id,
                    std::mem::size_of_val(&boxed_isolated_launch_future)
                ),
            );
            // isolatedLaunchTask lets Tokio poll the boxed launch Future from a fresh scheduler boundary.
            // isolatedLaunchTask 使 Tokio 从新的调度边界轮询装箱后的启动 Future。
            let isolated_launch_task = tokio::spawn(boxed_isolated_launch_future);
            after_precheck_trace(
                "after_precheck_23b3_isolated_launch_task_spawned",
                format!("session={launch_session_id}"),
            );
            match isolated_launch_task.await {
                Ok(result) => {
                    after_precheck_trace(
                        "after_precheck_23b4_isolated_launch_task_joined",
                        format!("session={launch_session_id}"),
                    );
                    result
                }
                Err(error) => Err(ExecutionRunReport::failure(
                    launch_session_id,
                    None,
                    0,
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    None,
                    format!("The isolated runtime launch task failed: {error}"),
                )),
            }
        } else {
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
            )
            .await
        };
        let launch = match launch_result {
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

#[cfg(test)]
mod stack_size_diagnostics {
    //! Runtime launch type-size diagnostics for stack-boundary regression analysis.
    //! 用于栈边界回归分析的运行时启动类型大小诊断。

    use crate::agent_runtime::execution_models::ExecutionRunReport;
    use crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch;

    /// Prints exact launch output sizes so diagnostics can distinguish Future storage from output storage.
    /// 打印精确启动输出大小，使诊断能够区分 Future 存储与输出存储。
    #[test]
    fn reports_runtime_launch_output_type_sizes() {
        eprintln!(
            "runtime_stream_launch_bytes={} execution_run_report_bytes={} launch_result_bytes={}",
            std::mem::size_of::<RuntimeStreamLaunch>(),
            std::mem::size_of::<ExecutionRunReport>(),
            std::mem::size_of::<Result<RuntimeStreamLaunch, ExecutionRunReport>>()
        );
    }
}
