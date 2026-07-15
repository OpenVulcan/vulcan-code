//! Runtime stream preparation resource-bundle helpers for foundation construction and launch-input ownership.
//! 面向基础运行时构建与启动输入所有权封装的运行时流准备资源辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_models::ExecutionPreparationRequest;
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::agent_runtime::token_budget_runtime::TokenBudgetRuntime;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeAccess;
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::CommandSessionRuntime;
use std::sync::Arc;

/// Resource bundle captured before execution preparation begins.
/// 在执行准备开始前捕获的资源打包对象。
pub(crate) struct RuntimePreparationResources {
    /// Runtime foundation built from the provider config and active agent profile catalog.
    /// 根据 Provider 配置与当前代理画像目录构建出的运行时基础架构。
    pub foundation: RuntimeFoundation,
    /// Optional LuaSkills runtime handle that remains available for later bootstrap steps.
    /// 供后续 bootstrap 步骤继续使用的可选 LuaSkills 运行时句柄。
    pub luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Original execution request owned by the current runtime launch.
    /// 归属于当前运行时启动流程的原始执行请求。
    pub request: ExecutionPreparationRequest,
}

/// Builds one resource bundle that owns the runtime foundation, runtime handles, and execution request for one launch attempt.
/// 为一次启动尝试构建一份持有运行时基础架构、运行时句柄与执行请求的资源打包对象。
pub(crate) fn build_runtime_preparation_resources(
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
) -> RuntimePreparationResources {
    let foundation = RuntimeFoundation::build_from_optional_mcp_access_with_runtimes_and_plugins(
        config,
        agent_profile_catalog,
        luaskills_runtime.clone(),
        system_plugin_runtime,
        Some(mcp_runtime_access),
        task_runtime,
        question_runtime,
        command_session_runtime,
        token_budget_runtime,
    );

    RuntimePreparationResources {
        foundation,
        luaskills_runtime,
        request,
    }
}
