//! Runtime stream preparation helpers for foundation construction, preparation validation, and failure-report conversion.
//! 面向基础运行时构建、准备阶段校验与失败报告转换的运行时流准备辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionRunReport,
};
use crate::agent_runtime::execution_preparation::prepare_execution;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;

/// Prepared runtime-launch input captured after foundation construction and execution-preparation validation have both succeeded.
/// 在基础运行时构建与执行准备校验都成功之后捕获到的运行时启动准备输入。
pub(crate) struct RuntimePreparedLaunchInput {
    /// Runtime foundation built from the provider config and active agent profile catalog.
    /// 根据 Provider 配置与当前代理画像目录构建出的运行时基础架构。
    pub foundation: RuntimeFoundation,
    /// Optional LuaSkills runtime handle that remains available for later bootstrap steps.
    /// 供后续 bootstrap 步骤继续使用的可选 LuaSkills 运行时句柄。
    pub luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Optional MCP runtime handle that remains available for later bootstrap steps.
    /// 供后续 bootstrap 步骤继续使用的可选 MCP 运行时句柄。
    pub mcp_runtime: Option<McpRuntimeHandle>,
    /// Original execution request owned by the current runtime launch.
    /// 归属于当前运行时启动流程的原始执行请求。
    pub request: ExecutionPreparationRequest,
    /// Validated execution preparation built from the runtime foundation and request.
    /// 根据运行时基础架构与执行请求构建出的通过校验的执行准备对象。
    pub preparation: ExecutionPreparation,
}

/// Builds one prepared runtime-launch input or converts one preparation failure into the terminal execution report expected by the outer shell.
/// 构建一份运行时启动准备输入，或在准备失败时转换成外层执行壳期望的终态执行报告。
pub(crate) fn prepare_runtime_launch_input(
    config: &ProviderConfigurationFile,
    agent_profile_catalog: &AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    request: ExecutionPreparationRequest,
) -> Result<RuntimePreparedLaunchInput, ExecutionRunReport> {
    let foundation = RuntimeFoundation::build(
        config,
        agent_profile_catalog,
        luaskills_runtime.clone(),
        mcp_runtime.clone(),
    );
    let preparation = match prepare_execution(
        &foundation,
        luaskills_runtime.clone(),
        mcp_runtime.clone(),
        config,
        &request,
    ) {
        Ok(preparation) => preparation,
        Err(error) => {
            return Err(ExecutionRunReport::failure(
                request.session_id,
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                error,
            ));
        }
    };

    Ok(RuntimePreparedLaunchInput {
        foundation,
        luaskills_runtime,
        mcp_runtime,
        request,
        preparation,
    })
}
