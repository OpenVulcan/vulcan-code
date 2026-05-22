//! Execution-preparation builder for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备构建模块。

use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPreparationRequest};
use crate::agent_runtime::provider::resolve_active_provider_blueprint_with_ai_tools;
use crate::agent_runtime::provider_stream_projection::truncate_preview;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::conversation_protocol::collect_host_visible_attachments;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use crate::vulcan_tools_bridge::{build_vulcan_tools_manager, resolve_ai_tool_fallback_aliases};
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

    let model_blueprint = resolve_active_provider_blueprint_with_ai_tools(
        config,
        request.reasoning_effort,
        &request.ai_tool_configuration,
    )?;
    let mut warnings = Vec::new();
    let workspace_path = normalize_workspace_path_text(&request.workspace_path);
    let conversation_attachments = collect_host_visible_attachments(&request.conversation_state);
    let ai_tool_fallback_aliases = resolve_ai_tool_fallback_aliases(&model_blueprint);
    let tools_manager = build_vulcan_tools_manager(
        foundation.task_runtime(),
        foundation.question_runtime(),
        foundation.native_tool_catalog().clone(),
        luaskills_runtime,
        (!workspace_path.is_empty()).then(|| PathBuf::from(workspace_path.clone())),
        conversation_attachments.clone(),
        foundation.resolved_native_tools_for_agent(&active_agent.id),
        foundation.resolved_luaskills_tools_for_agent(&active_agent.id),
        foundation.resolved_mcp_tools_for_agent(&active_agent.id),
        request.ai_tool_configuration.clone(),
        ai_tool_fallback_aliases.clone(),
    );
    let mut resolved_tools = tools_manager.resolve_for_execution_mode(&request.execution_mode);

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

    push_missing_ai_tool_fallback_warnings(
        &mut warnings,
        &ai_tool_fallback_aliases,
        &resolved_tools,
    );

    if !model_blueprint.supports_tool_call() && !resolved_tools.descriptors.is_empty() {
        warnings.push(format!(
            "当前模型 `{}` 未声明工具调用能力，本轮已自动隐藏 {} 个工具。",
            model_blueprint.model_name,
            resolved_tools.descriptors.len()
        ));
        resolved_tools.descriptors.clear();
        resolved_tools.definitions.clear();
    }

    let temperature = if model_blueprint.supports_temperature() {
        request.temperature
    } else {
        if request.temperature.is_some() {
            warnings.push(format!(
                "当前模型 `{}` 未声明显式温度能力，本轮将自动回退到模型默认采样设置。",
                model_blueprint.model_name
            ));
        }
        None
    };

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
        conversation_attachments,
        context_history_count: request.conversation_state.messages.len(),
        execution_mode: request.execution_mode.clone(),
        temperature,
        skill_names: foundation.skill_names_for_agent(&active_agent.id),
        tool_descriptors: resolved_tools.descriptors,
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: mcp_overview.active_summary_rows().to_vec(),
        warnings,
        provider_request_preview_markdown: String::new(),
        provider_request_snapshot_json: None,
        context_budget: None,
    })
}

/// Adds non-fatal diagnostics when a configured semantic AI fallback could not be exposed.
/// 当配置的语义 AI 回退工具无法暴露时追加非致命诊断。
fn push_missing_ai_tool_fallback_warnings(
    warnings: &mut Vec<String>,
    aliases: &[crate::vulcan_tools_bridge::AiToolFallbackAlias],
    resolved_tools: &vulcan_tools_manager::ResolvedToolSet,
) {
    for alias in aliases {
        let semantic_tool_name = alias.kind.key();
        let exposed = resolved_tools.descriptors.iter().any(|descriptor| {
            descriptor.source == vulcan_tools_manager::VulcanToolSourceKind::AiFallback
                && descriptor.id == semantic_tool_name
        });
        if !exposed {
            warnings.push(format!(
                "AI 专用工具 `{}` 已配置回退 `{}`，但本轮未找到可暴露的宿主工具。",
                alias.kind.label(),
                alias.fallback_tool_id
            ));
        }
    }
}
