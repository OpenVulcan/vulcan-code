//! Runtime request-capture entrypoint for workbench API-request diagnostics.
//! 面向工作台 API 请求诊断的运行时请求捕获入口。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::ExtensionGeneration;
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::agent_runtime::provider_runtime_dispatch::execute_provider_prompt_capture_stream;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_request::ProviderTransportTurnStartMode;
use crate::agent_runtime::question_runtime::QuestionRuntime;
use crate::agent_runtime::runtime_stream_launch::prepare_runtime_stream_launch_with_plugins_and_public_ai;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::{McpRuntimeAccess, configured_mcp_runtime_access};
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::CommandSessionRuntime;
use futures_util::StreamExt;
use serde_json::Value;
use std::sync::Arc;

/// Captures the final provider wire request for one simulated runtime turn without sending it.
/// 捕获一次模拟运行时回合的最终 Provider wire 请求，但不实际发送。
///
/// # Parameters
/// 参数
/// `config` is the provider configuration used by the normal runtime launch path.
/// `config` 是普通运行时启动链路使用的 Provider 配置。
/// `agent_profile_catalog` is the current resolved agent profile catalog.
/// `agent_profile_catalog` 是当前已解析的代理画像目录。
/// `luaskills_runtime` is the optional shared LuaSkills runtime handle.
/// `luaskills_runtime` 是可选的共享 LuaSkills 运行时句柄。
/// `mcp_runtime` is the optional preconfigured MCP runtime access surface.
/// `mcp_runtime` 是可选的已配置 MCP 运行时访问面。
/// `task_runtime` is the shared resumable task runtime used by provider execution.
/// `task_runtime` 是 Provider 执行复用的共享可恢复任务运行时。
/// `question_runtime` is the shared question runtime used by host tools.
/// `question_runtime` 是宿主工具使用的共享提问运行时。
/// `command_session_runtime` is the shared command-session runtime used by shell tools.
/// `command_session_runtime` 是 shell 工具使用的共享命令会话运行时。
/// `public_ai_runtime` owns the process-local catalogs, routes, and credential boundary.
/// `public_ai_runtime` 持有进程内目录、路由与凭据边界。
/// `request` is the simulated execution request to assemble.
/// `request` 是需要装配的模拟执行请求。
///
/// # Returns
/// 返回值
/// The captured provider-specific HTTP request JSON.
/// 捕获到的 Provider 专用 HTTP 请求 JSON。
pub async fn capture_runtime_provider_wire_request(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeAccess>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    public_ai_runtime: Arc<vulcan_provider_adapters::PublicAiRuntime>,
    request: ExecutionPreparationRequest,
) -> Result<Value, String> {
    let mcp_runtime_access = mcp_runtime.unwrap_or_else(|| configured_mcp_runtime_access(&config));
    let launch = prepare_runtime_stream_launch_with_plugins_and_public_ai(
        &config,
        &agent_profile_catalog,
        luaskills_runtime,
        None,
        Arc::new(ExtensionGeneration::new(0, None, mcp_runtime_access, None)),
        task_runtime,
        question_runtime,
        command_session_runtime,
        Some(public_ai_runtime),
        request,
    )
    .await
    .map_err(execution_report_error_message)?;

    let mut provider_stream = execute_provider_prompt_capture_stream(
        &config,
        &launch.preparation,
        launch.active_agent.as_ref(),
        &launch.system_preamble,
        &launch.effective_prompt,
        match launch.turn_start_mode {
            crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamTurnStartMode::UserPrompt => {
                ProviderTransportTurnStartMode::UserPrompt
            }
            crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamTurnStartMode::RestoredCheckpoint => {
                ProviderTransportTurnStartMode::RestoredCheckpoint
            }
        },
        launch.core_snapshot,
        launch.task_runtime,
        launch.tools_manager,
        launch.resolved_tool_definitions,
        launch.public_ai_runtime,
    );

    while let Some(item) = provider_stream.next().await {
        match item {
            Ok(ProviderStreamDelta::WireRequest(request_json)) => return Ok(request_json),
            Ok(_) => {}
            Err(error) => return Err(error),
        }
    }

    Err(String::from(
        "模拟请求已完成，但未捕获到最终 Provider wire 请求。",
    ))
}

/// Converts a failed preparation report into a compact diagnostic message.
/// 将失败的准备报告转换为紧凑诊断信息。
///
/// # Parameters
/// 参数
/// `report` is the terminal execution report produced during preparation.
/// `report` 是准备阶段产生的终态执行报告。
///
/// # Returns
/// 返回值
/// A human-readable error message for the request preview tab.
/// 面向请求预览页签的人类可读错误信息。
fn execution_report_error_message(report: ExecutionRunReport) -> String {
    report
        .error
        .unwrap_or_else(|| String::from("模拟请求准备失败，但运行时未返回具体错误。"))
}
