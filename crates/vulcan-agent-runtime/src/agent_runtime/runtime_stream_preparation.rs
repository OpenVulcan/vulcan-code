//! Runtime stream preparation helpers for foundation construction, preparation validation, and failure-report conversion.
//! 面向基础运行时构建、准备阶段校验与失败报告转换的运行时流准备辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport,
};
use crate::agent_runtime::execution_preparation::{
    PreparedToolSnapshot, prepare_execution_with_tool_snapshot,
};
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_preparation_failure::build_preparation_failure_report;
use crate::agent_runtime::runtime_stream_preparation_resources::{
    RuntimePreparationResources, build_runtime_preparation_resources,
};
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeAccess;
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
    system_plugin_runtime: Option<vulcan_plugin_runtime::SystemPluginRuntimeManager>,
    mcp_runtime_access: McpRuntimeAccess,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    token_budget_runtime: Arc<TokenBudgetRuntime>,
    request: ExecutionPreparationRequest,
) -> Result<RuntimePreparedLaunchInput, ExecutionRunReport> {
    let resources = build_runtime_preparation_resources(
        config,
        agent_profile_catalog,
        luaskills_runtime,
        system_plugin_runtime,
        mcp_runtime_access,
        task_runtime,
        question_runtime,
        command_session_runtime,
        token_budget_runtime,
        request,
    );
    let prepared = match prepare_execution_with_tool_snapshot(
        &resources.foundation,
        resources.luaskills_runtime.clone(),
        config,
        &resources.request,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Err(build_preparation_failure_report(
                resources.request.session_id,
                error,
            ));
        }
    };

    Ok(RuntimePreparedLaunchInput::from_resources(
        resources,
        prepared.preparation,
        prepared.tool_snapshot,
    ))
}
