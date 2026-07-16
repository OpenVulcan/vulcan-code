//! Runtime stream launch payload and launch-builder helpers for the outer execution shell.
//! 面向外层执行壳的运行时流启动载荷与启动构建辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::ExtensionGeneration;
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime_stream_bootstrap::bootstrap_runtime_stream;
use crate::agent_runtime::runtime_stream_launch_payload::{
    RuntimeStreamLaunch, build_runtime_stream_launch_payload,
};
use crate::agent_runtime::runtime_stream_preparation::{
    RuntimePreparedLaunchInput, prepare_runtime_launch_input,
};
use crate::agent_runtime::task_runtime::{TaskExecutionRuntimeContext, TaskRuntime};
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeAccess;
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::CommandSessionRuntime;
use std::sync::Arc;

/// Builds one fully prepared runtime-stream launch payload or one terminal preparation-failure report.
/// 构建一份完全就绪的运行时流启动载荷，或者生成一份终态准备失败报告。
pub async fn prepare_runtime_stream_launch(
    config: &ProviderConfigurationFile,
    agent_profile_catalog: &AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime_access: McpRuntimeAccess,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    request: ExecutionPreparationRequest,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    prepare_runtime_stream_launch_with_plugins(
        config,
        agent_profile_catalog,
        luaskills_runtime,
        None,
        Arc::new(ExtensionGeneration::new(0, None, mcp_runtime_access)),
        task_runtime,
        question_runtime,
        command_session_runtime,
        token_budget_runtime,
        request,
    )
    .await
}

/// Builds one runtime-stream launch with an independently owned Vulcan Plugin runtime.
/// 使用独立持有的 Vulcan 插件运行时构建一次运行时流启动载荷。
pub async fn prepare_runtime_stream_launch_with_plugins(
    config: &ProviderConfigurationFile,
    agent_profile_catalog: &AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    vulcan_plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    extension_generation: Arc<ExtensionGeneration>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    request: ExecutionPreparationRequest,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    let prepared_launch_input = prepare_runtime_launch_input(
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
    )?;
    build_runtime_stream_launch_from_prepared_input(config.clone(), prepared_launch_input).await
}

/// Assembles the final runtime-stream launch payload from one already prepared launch input.
/// 根据一份已经准备完成的启动输入组装最终运行时流启动载荷。
async fn build_runtime_stream_launch_from_prepared_input(
    config: ProviderConfigurationFile,
    prepared_launch_input: RuntimePreparedLaunchInput,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    let RuntimePreparedLaunchInput {
        foundation,
        luaskills_runtime,
        request,
        preparation,
        tool_snapshot,
    } = prepared_launch_input;
    let foundation = Arc::new(foundation);
    let task_execution_context_luaskills_runtime = luaskills_runtime.clone();
    let preparation_for_failure = preparation.clone();
    let bootstrap = match bootstrap_runtime_stream(
        &foundation,
        luaskills_runtime,
        &request,
        preparation,
        tool_snapshot,
    )
    .await
    {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            return Err(ExecutionRunReport::failure(
                request.session_id,
                Some(preparation_for_failure),
                0,
                None,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                error,
            ));
        }
    };
    let task_execution_context = Some(
        TaskExecutionRuntimeContext::new(
            config,
            Arc::clone(&foundation),
            task_execution_context_luaskills_runtime,
            request.clone(),
        )
        .with_parent_snapshot(bootstrap.core_snapshot.clone()),
    );

    Ok(build_runtime_stream_launch_payload(
        &request,
        bootstrap,
        task_execution_context,
    ))
}
