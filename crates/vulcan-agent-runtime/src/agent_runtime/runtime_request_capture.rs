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
use crate::prompt_media_projection::PromptMediaProjectionRuntime;
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::CommandSessionRuntime;
use futures_util::StreamExt;
use serde_json::Value;
use std::sync::Arc;

/// Captures an exact wire request using the ordinary launch inputs and optional prompt-owned media authority.
/// 使用普通启动输入及可选提示词所有媒体权威捕获精确线上请求。
/// `config` and `agent_profile_catalog` freeze configuration and agents; optional LuaSkills/MCP handles supply tools.
/// `config`和`agent_profile_catalog`冻结配置与智能体；可选LuaSkills/MCP句柄提供工具。
/// Task, question, command-session and public-AI runtimes retain the ordinary execution ownership boundaries.
/// 任务、提问、命令会话及公共AI运行时保留普通执行的所有权边界。
/// `request` selects the simulated turn; `media_projection` binds its exact ordered source message identities.
/// `request`选择模拟回合；`media_projection`绑定其精确有序来源消息身份。
/// Returns encoded JSON without dispatch, or an explicit error when exact capture would require an upload or lacks proof.
/// 返回未分派的编码JSON；精确捕获需要上传或缺少证明时返回明确错误。
pub async fn capture_runtime_provider_wire_request_with_media_projection(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeAccess>,
    task_runtime: Arc<TaskRuntime>,
    question_runtime: Arc<QuestionRuntime>,
    command_session_runtime: Arc<CommandSessionRuntime>,
    public_ai_runtime: Arc<vulcan_provider_adapters::PublicAiRuntime>,
    request: ExecutionPreparationRequest,
    media_projection: Option<Arc<dyn PromptMediaProjectionRuntime>>,
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
        launch.dynamic_prompt_layers,
        launch.locked_baseline,
        &launch.effective_prompt,
        match launch.turn_start_mode {
            crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamTurnStartMode::UserPrompt => {
                ProviderTransportTurnStartMode::UserPrompt
            }
            crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamTurnStartMode::RestoredCheckpoint => {
                ProviderTransportTurnStartMode::RestoredCheckpoint
            }
            crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamTurnStartMode::ContinuedProviderTurn => {
                ProviderTransportTurnStartMode::ContinuedProviderTurn
            }
        },
        launch.core_snapshot,
        launch.task_runtime,
        launch.tools_manager,
        launch.resolved_tool_definitions,
        launch.public_ai_runtime,
        media_projection,
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
