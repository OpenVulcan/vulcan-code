//! Runtime stream launch payload and launch-builder helpers for the outer execution shell.
//! 面向外层执行壳的运行时流启动载荷与启动构建辅助模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::agent_runtime::runtime_stream_bootstrap::bootstrap_runtime_stream;
use crate::agent_runtime::runtime_stream_launch_payload::{
    RuntimeStreamLaunch, build_runtime_stream_launch_payload,
};
use crate::agent_runtime::runtime_stream_preparation::{
    RuntimePreparedLaunchInput, prepare_runtime_launch_input,
};
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeAccess;
use crate::provider_config::ProviderConfigurationFile;

/// Builds one fully prepared runtime-stream launch payload or one terminal preparation-failure report.
/// 构建一份完全就绪的运行时流启动载荷，或者生成一份终态准备失败报告。
pub(crate) async fn prepare_runtime_stream_launch(
    config: &ProviderConfigurationFile,
    agent_profile_catalog: &AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime_access: McpRuntimeAccess,
    request: ExecutionPreparationRequest,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    let prepared_launch_input = prepare_runtime_launch_input(
        config,
        agent_profile_catalog,
        luaskills_runtime,
        mcp_runtime_access,
        request,
    )?;
    build_runtime_stream_launch_from_prepared_input(prepared_launch_input).await
}

/// Assembles the final runtime-stream launch payload from one already prepared launch input.
/// 根据一份已经准备完成的启动输入组装最终运行时流启动载荷。
async fn build_runtime_stream_launch_from_prepared_input(
    prepared_launch_input: RuntimePreparedLaunchInput,
) -> Result<RuntimeStreamLaunch, ExecutionRunReport> {
    let RuntimePreparedLaunchInput {
        foundation,
        luaskills_runtime,
        request,
        preparation,
    } = prepared_launch_input;
    let bootstrap =
        bootstrap_runtime_stream(&foundation, luaskills_runtime, &request, preparation).await;

    Ok(build_runtime_stream_launch_payload(&request, bootstrap))
}
