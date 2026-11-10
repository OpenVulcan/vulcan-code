//! Runtime stream launch payload and launch-builder helpers for the outer execution shell.
//! 面向外层执行壳的运行时流启动载荷与启动构建辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::ExtensionGeneration;
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime_stream_bootstrap::bootstrap_runtime_stream;
use crate::agent_runtime::runtime_stream_bootstrap_core::{
    after_precheck_trace, after_precheck_unix_millis,
};
use crate::agent_runtime::runtime_stream_launch_payload::{
    RuntimeStreamLaunch, build_runtime_stream_launch_payload,
};
use crate::agent_runtime::runtime_stream_preparation::{
    RuntimePreparedLaunchInput, prepare_runtime_launch_input,
};
use crate::agent_runtime::task_runtime::{TaskExecutionRuntimeContext, TaskRuntime};
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::CommandSessionRuntime;
use std::sync::Arc;

/// Builds one runtime-stream launch with plugins and one service-owned public AI runtime.
/// 使用插件与一个服务持有的公共 AI 运行时构建运行时流启动。
#[allow(clippy::too_many_arguments)]
pub async fn prepare_runtime_stream_launch_with_plugins_and_public_ai(
    config: &ProviderConfigurationFile,
    agent_profile_catalog: &AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    vulcan_plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    extension_generation: Arc<ExtensionGeneration>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
    request: ExecutionPreparationRequest,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    Box::pin(prepare_runtime_stream_launch_with_provider_blueprint(
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
        None,
    ))
    .await
}

/// Builds a launch from the runtime resources and an execution request, optionally using the
/// request-scoped manual summary blueprint. Returns the launch or a terminal preparation report.
/// 使用运行时资源与执行请求构建启动，可选用请求级手动摘要蓝图。
/// 返回启动载荷或终态准备报告。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare_runtime_stream_launch_with_provider_blueprint(
    config: &ProviderConfigurationFile,
    agent_profile_catalog: &AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    vulcan_plugin_runtime: Option<vulcan_plugin_runtime::VulcanPluginRuntimeManager>,
    extension_generation: Arc<ExtensionGeneration>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    public_ai_runtime: Option<Arc<vulcan_provider_adapters::PublicAiRuntime>>,
    request: ExecutionPreparationRequest,
    manual_summary_blueprint: Option<&ProviderRuntimeBlueprint>,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    // launchStartedAt measures the complete lazy launch preparation reached by the first stream poll.
    // launchStartedAt 测量首次流轮询进入的完整惰性启动准备边界。
    let launch_started_at = after_precheck_unix_millis();
    after_precheck_trace(
        "after_precheck_23a_runtime_launch_enter",
        format!("session={}", request.session_id),
    );
    let prepared_launch_input = prepare_runtime_launch_input(
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
        manual_summary_blueprint,
    )?;
    after_precheck_trace(
        "after_precheck_26_prepared_launch_input_ready",
        format!(
            "session={} elapsed_ms={}",
            prepared_launch_input.request.session_id,
            after_precheck_unix_millis().saturating_sub(launch_started_at)
        ),
    );
    // Keep bootstrap state out of the preparation future's inline storage.
    // 避免将引导状态内嵌在准备 Future 的存储中。
    Box::pin(build_runtime_stream_launch_from_prepared_input(
        config.clone(),
        prepared_launch_input,
    ))
    .await
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
    // Heap ownership reduces nested poll frames while retaining cancellation by the parent future.
    // 堆持有缩小嵌套轮询栈帧，并保留父 Future 的取消控制。
    let bootstrap = match Box::pin(bootstrap_runtime_stream(
        &foundation,
        luaskills_runtime,
        &request,
        preparation,
        tool_snapshot,
    ))
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

    build_runtime_stream_launch_payload(&request, bootstrap, task_execution_context).map_err(
        |error| {
            ExecutionRunReport::failure(
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
                format!("Provider request continuation validation failed: {error}"),
            )
        },
    )
}
