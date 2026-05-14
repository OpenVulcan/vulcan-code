//! Execution-preparation builder for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备构建模块。

use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPreparationRequest};
use crate::agent_runtime::provider::resolve_active_provider_blueprint;
use crate::agent_runtime::provider_stream_projection::truncate_preview;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use crate::vulcan_tools_bridge::build_vulcan_tools_manager;
use crate::workspace_path::normalize_workspace_path_text;
use std::path::PathBuf;

/// Builds one validated execution-preparation object from the current runtime foundation and GUI request.
/// 根据当前运行时基础架构与 GUI 请求构建一份通过校验的执行准备对象。
pub(crate) fn prepare_execution(
    foundation: &RuntimeFoundation,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    config: &ProviderConfigurationFile,
    request: &ExecutionPreparationRequest,
) -> Result<ExecutionPreparation, String> {
    let Some(active_agent) = foundation.active_agent_definition() else {
        return Err(String::from("当前激活代理未能在运行时基础架构中解析到。"));
    };

    if !foundation.has_built_agent(&active_agent.id) {
        return Err(format!(
            "代理 `{}` 当前尚未成功构建，无法启动执行。",
            active_agent.name
        ));
    }

    let model_blueprint = resolve_active_provider_blueprint(config, request.reasoning_effort)?;
    let mut warnings = Vec::new();
    let workspace_path = normalize_workspace_path_text(&request.workspace_path);
    let tools_manager = build_vulcan_tools_manager(
        foundation.task_runtime(),
        foundation.native_tool_catalog().clone(),
        luaskills_runtime,
        (!workspace_path.is_empty()).then(|| PathBuf::from(workspace_path.clone())),
        foundation.resolved_native_tools_for_agent(&active_agent.id),
        foundation.resolved_luaskills_tools_for_agent(&active_agent.id),
        foundation.resolved_mcp_tools_for_agent(&active_agent.id),
    );
    let resolved_tools = tools_manager.resolve_for_execution_mode(&request.execution_mode);

    if workspace_path.is_empty() {
        warnings.push(String::from(
            "当前工作区路径为空，后续原生工具可能无法提供完整上下文。",
        ));
    }

    if request.prompt.trim().is_empty() {
        warnings.push(String::from(
            "当前提示词为空，本次准备计划仅验证运行时链路。",
        ));
    }

    if let Some(reasoning_note) = model_blueprint.reasoning.note.as_ref() {
        warnings.push(reasoning_note.clone());
    }

    let mcp_overview = foundation.mcp_overview();

    Ok(ExecutionPreparation {
        session_id: request.session_id,
        session_title: request.session_title.clone(),
        agent_id: active_agent.id.clone(),
        agent_name: active_agent.name.clone(),
        agent_kind: active_agent.kind.label().to_string(),
        model_blueprint,
        prompt_preview: truncate_preview(request.prompt.trim(), 180),
        workspace_path,
        context_history_count: request.conversation_state.messages.len(),
        execution_mode: request.execution_mode.clone(),
        temperature: request.temperature,
        skill_names: foundation.skill_names_for_agent(&active_agent.id),
        tool_descriptors: resolved_tools.descriptors,
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: mcp_overview.active_summary_rows().to_vec(),
        warnings,
        provider_request_preview_markdown: String::new(),
        provider_request_snapshot_json: None,
    })
}
