//! Runtime stream preparation helpers for foundation construction, preparation validation, and failure-report conversion.
//! 面向基础运行时构建、准备阶段校验与失败报告转换的运行时流准备辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::ExtensionGeneration;
use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport,
};
use crate::agent_runtime::execution_preparation::{
    PreparedToolSnapshot, prepare_execution_with_provider_blueprint,
};
use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_bootstrap_core::{
    after_precheck_trace, after_precheck_unix_millis,
};
use crate::agent_runtime::runtime_stream_preparation_failure::build_preparation_failure_report;
use crate::agent_runtime::runtime_stream_preparation_resources::{
    RuntimePreparationResources, build_runtime_preparation_resources,
};
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::CommandSessionRuntime;
use std::sync::Arc;

/// Prepared runtime-launch input captured after foundation construction and execution-preparation validation have both succeeded.
/// 在基础运行时构建与执行准备校验都成功之后捕获到的运行时启动准备输入。
pub(crate) struct RuntimePreparedLaunchInput {
    /// Runtime foundation built from the provider config and active agent profile catalog.
    /// 根据 Provider 配置与当前代理画像目录构建出的运行时基础架构。
    pub foundation: RuntimeFoundation,
    /// Optional LuaSkills runtime handle that remains available for later bootstrap steps.
    /// 供后续 bootstrap 步骤继续使用的可选 LuaSkills 运行时句柄。
    pub luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Original execution request owned by the current runtime launch.
    /// 归属于当前运行时启动流程的原始执行请求。
    pub request: ExecutionPreparationRequest,
    /// Validated execution preparation built from the runtime foundation and request.
    /// 根据运行时基础架构与执行请求构建出的通过校验的执行准备对象。
    pub preparation: ExecutionPreparation,
    /// Execution-only tool snapshot resolved together with the preparation data.
    /// 与准备数据一起解析得到的仅执行期工具快照。
    pub tool_snapshot: PreparedToolSnapshot,
}

impl RuntimePreparedLaunchInput {
    /// Builds one prepared launch input from an already assembled preparation-resource bundle and one validated execution preparation.
    /// 根据一份已组装的准备资源打包对象与一份通过校验的执行准备构建一份启动准备输入。
    fn from_resources(
        resources: RuntimePreparationResources,
        preparation: ExecutionPreparation,
        tool_snapshot: PreparedToolSnapshot,
    ) -> Self {
        Self {
            foundation: resources.foundation,
            luaskills_runtime: resources.luaskills_runtime,
            request: resources.request,
            preparation,
            tool_snapshot,
        }
    }
}

/// Builds one prepared runtime-launch input or converts one preparation failure into the terminal execution report expected by the outer shell.
/// 构建一份运行时启动准备输入，或在准备失败时转换成外层执行壳期望的终态执行报告。
pub(crate) fn prepare_runtime_launch_input(
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
) -> Result<RuntimePreparedLaunchInput, ExecutionRunReport> {
    // preparationStartedAt measures the complete synchronous launch-preparation boundary.
    // preparationStartedAt 测量完整同步启动准备边界。
    let preparation_started_at = after_precheck_unix_millis();
    after_precheck_trace(
        "after_precheck_24_runtime_preparation_enter",
        format!("session={}", request.session_id),
    );
    // resourcesStartedAt isolates runtime-foundation construction from later execution preparation.
    // resourcesStartedAt 将运行时基础设施构建与后续执行准备隔离计时。
    let resources_started_at = after_precheck_unix_millis();
    let resources = build_runtime_preparation_resources(
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
    );
    after_precheck_trace(
        "after_precheck_24a_preparation_resources_ready",
        format!(
            "session={} elapsed_ms={}",
            resources.request.session_id,
            after_precheck_unix_millis().saturating_sub(resources_started_at)
        ),
    );
    // executionPreparationStartedAt isolates tool and provider preparation after foundation construction.
    // executionPreparationStartedAt 将基础设施构建后的工具及供应商准备隔离计时。
    let execution_preparation_started_at = after_precheck_unix_millis();
    let prepared = match prepare_execution_with_provider_blueprint(
        &resources.foundation,
        resources.luaskills_runtime.clone(),
        config,
        &resources.request,
        manual_summary_blueprint,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Err(build_preparation_failure_report(
                resources.request.session_id,
                error,
            ));
        }
    };
    after_precheck_trace(
        "after_precheck_25_execution_preparation_ready",
        format!(
            "session={} tools={} elapsed_ms={} elapsed_ms_since_preparation={}",
            resources.request.session_id,
            prepared.tool_snapshot.resolved_tool_definitions.len(),
            after_precheck_unix_millis().saturating_sub(execution_preparation_started_at),
            after_precheck_unix_millis().saturating_sub(preparation_started_at)
        ),
    );

    Ok(RuntimePreparedLaunchInput::from_resources(
        resources,
        prepared.preparation,
        prepared.tool_snapshot,
    ))
}
