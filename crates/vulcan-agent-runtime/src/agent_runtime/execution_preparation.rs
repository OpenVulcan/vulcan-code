//! Execution-preparation builder for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备构建模块。

use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionPromptPurpose,
};
use crate::agent_runtime::provider::resolve_active_provider_blueprint_with_ai_tools;
use crate::agent_runtime::provider_stream_projection::truncate_preview;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::conversation_protocol::collect_host_visible_attachments;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use crate::tooling::native::SUBMIT_PLAN_TOOL_ID;
use crate::vulcan_tools_bridge::{build_vulcan_tools_manager, resolve_ai_tool_fallback_aliases};
use crate::workspace_path::normalize_workspace_path_text;
use std::path::PathBuf;
use vulcan_luaskills_prompt::{LuaSkillPromptEntry, render_luaskills_prompt_segment};
use vulcan_prompt_skills::{
    PromptSkillsProjectConfig, PromptSkillsSessionConfig, render_prompt_skills_instructions,
    resolve_prompt_skills,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

#[path = "execution_preparation/tool_exposure.rs"]
mod tool_exposure;

use tool_exposure::{filter_resolved_tool_set_by_tool_exposure, resolve_execution_tool_exposure};

/// Prepared execution plus execution-only tool registration objects resolved from the same snapshot.
/// 从同一份快照解析得到的执行准备结果与仅执行期使用的工具注册对象。
pub(crate) struct PreparedExecution {
    /// Validated execution preparation that is safe to clone into UI events and reports.
    /// 可安全克隆进 UI 事件与报告中的已校验执行准备结果。
    pub preparation: ExecutionPreparation,
    /// Execution-only tool snapshot consumed by provider bootstrap and dispatch.
    /// 供 Provider bootstrap 与工具调度消费的仅执行期工具快照。
    pub tool_snapshot: PreparedToolSnapshot,
}

/// Execution-only tool snapshot captured during preparation without leaking into UI-facing preparation data.
/// 准备阶段捕获的仅执行期工具快照，不泄漏进面向 UI 的准备数据。
pub(crate) struct PreparedToolSnapshot {
    /// Host-owned tool manager captured with the resolved tool registration snapshot.
    /// 随已解析工具注册快照一起捕获的宿主自有工具管理器。
    pub tools_manager: VulcanToolsManager,
    /// Provider-registered tool definitions resolved once during execution preparation.
    /// 在执行准备阶段一次性解析得到的 Provider 注册工具定义集合。
    pub resolved_tool_definitions: Vec<ResolvedToolDefinition>,
}

/// Builds one validated execution preparation and the matching execution-only tool snapshot.
/// 构建一份已校验执行准备结果以及与之匹配的仅执行期工具快照。
pub(crate) fn prepare_execution_with_tool_snapshot(
    foundation: &RuntimeFoundation,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    config: &ProviderConfigurationFile,
    request: &ExecutionPreparationRequest,
) -> Result<PreparedExecution, String> {
    let active_agent = match request.execution_agent_id.as_deref() {
        Some(agent_id) => foundation
            .agent_definition(agent_id)
            .ok_or_else(|| format!("执行代理 `{agent_id}` 未能在运行时基础架构中解析到。"))?,
        None => foundation
            .active_agent_definition()
            .ok_or_else(|| String::from("当前激活代理未能在运行时基础架构中解析到。"))?,
    };

    if !foundation.has_built_agent(&active_agent.id) {
        return Err(format!(
            "代理 `{}` 当前尚未成功构建，无法启动执行。",
            active_agent.name
        ));
    }

    let tool_exposure = resolve_execution_tool_exposure(foundation, &active_agent.id, request);
    let model_blueprint = resolve_active_provider_blueprint_with_ai_tools(
        config,
        request.reasoning_effort,
        &tool_exposure.ai_tool_configuration,
    )?;
    let mut warnings = Vec::new();
    let workspace_path = normalize_workspace_path_text(&request.workspace_path);
    let conversation_attachments = collect_host_visible_attachments(&request.conversation_state);
    let prompt_skills_global_config =
        crate::skills::prompt::global_config_from_persisted(&config.prompt_skills);
    let prompt_skill_user_root = Some(vulcan_prompt_skills::configured_user_skill_root(
        &prompt_skills_global_config,
    ));
    let prompt_skill_catalog =
        vulcan_prompt_skills::load_prompt_skills(&prompt_skills_global_config);
    for error in &prompt_skill_catalog.errors {
        warnings.push(format!(
            "提示词技能 `{}` 载入失败：{}",
            error.path.display(),
            error.message
        ));
    }
    let prompt_skills = resolve_prompt_skills(
        &prompt_skill_catalog,
        &prompt_skills_global_config,
        &prompt_skills_project_config_from_request(request),
        &prompt_skills_session_config_from_request(request),
    );
    let implicit_luaskills = tool_exposure.luaskills_dispatch_tools.clone();
    let implicit_luaskill_prompt_entries = tool_exposure.luaskill_prompt_entries.clone();
    let prompt_skill_developer_segment = build_skill_developer_segment(
        request.prompt_purpose,
        &prompt_skills,
        &implicit_luaskill_prompt_entries,
    );
    let enabled_prompt_skill_names = vulcan_prompt_skills::enabled_skill_names(&prompt_skills);
    let ai_tool_fallback_aliases = resolve_ai_tool_fallback_aliases(&model_blueprint);
    let project_vmm = if request.runtime_policy.vmm_enabled {
        request.project_vmm.clone()
    } else {
        Default::default()
    };
    let session_vmm = if request.runtime_policy.vmm_enabled {
        request.session_vmm.clone()
    } else {
        Default::default()
    };
    let system_prompt_hook_blocks = if request.runtime_policy.hooks_enabled {
        request.system_prompt_hook_blocks.clone()
    } else {
        Vec::new()
    };
    let tools_manager = build_vulcan_tools_manager(
        foundation.task_runtime(),
        foundation.question_runtime(),
        foundation.native_tool_catalog().clone(),
        luaskills_runtime,
        (!workspace_path.is_empty()).then(|| PathBuf::from(workspace_path.clone())),
        request.controller_runtime.clone(),
        conversation_attachments.clone(),
        tool_exposure.native_tool_ids.clone(),
        tool_exposure.luaskills_tools.clone(),
        implicit_luaskills,
        tool_exposure.mcp_tools.clone(),
        tool_exposure.ai_tool_configuration.clone(),
        ai_tool_fallback_aliases.clone(),
        enabled_prompt_skill_names,
        prompt_skill_user_root.clone(),
        implicit_luaskill_prompt_entries.clone(),
        project_vmm.clone(),
        session_vmm.clone(),
    );
    let mut resolved_tools = tools_manager.resolve_for_execution_mode(&request.execution_mode);
    if !request.execution_mode.eq_ignore_ascii_case("plan") {
        remove_resolved_tool(&mut resolved_tools, SUBMIT_PLAN_TOOL_ID);
    }
    filter_resolved_tool_set_by_tool_exposure(&mut resolved_tools, &tool_exposure);
    let prompt_skill_tool_available = request.prompt_purpose.supports_user_turn_tooling()
        && render_prompt_skills_instructions(&prompt_skills).is_some();
    if !prompt_skill_tool_available {
        remove_resolved_tool(&mut resolved_tools, "skill");
    }
    if request.prompt_purpose.disables_provider_tools() {
        if !resolved_tools.descriptors.is_empty() {
            warnings.push(format!(
                "内部 `{}` 执行不允许工具调用，本轮已隐藏 {} 个工具。",
                request.prompt_purpose.diagnostics_label(),
                resolved_tools.descriptors.len()
            ));
        }
        resolved_tools.descriptors.clear();
        resolved_tools.definitions.clear();
    }

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

    let resolved_tool_definitions = resolved_tools.definitions;
    let tool_descriptors = resolved_tools.descriptors;
    let preparation = ExecutionPreparation {
        session_id: request.session_id,
        session_title: request.session_title.clone(),
        agent_id: active_agent.id.clone(),
        agent_name: active_agent.name.clone(),
        agent_kind: active_agent.kind.label().to_string(),
        agent_system_prompt_override: request.agent_system_prompt_override.clone(),
        model_blueprint,
        prompt_preview: truncate_preview(request.prompt.trim(), 180),
        workspace_path,
        task_session_store_root: request.task_session_store_root.clone(),
        conversation_attachments,
        context_history_count: request.conversation_state.messages.len(),
        execution_mode: request.execution_mode.clone(),
        temperature,
        prompt_configuration: config.prompt.clone(),
        prompt_purpose: request.prompt_purpose,
        prompt_skills,
        prompt_skill_user_root,
        implicit_luaskill_prompt_entries,
        prompt_skill_developer_segment,
        skill_names: foundation.skill_names_for_agent(&active_agent.id),
        tool_descriptors,
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: mcp_overview.active_summary_rows().to_vec(),
        warnings,
        provider_request_preview_markdown: String::new(),
        provider_request_snapshot_json: None,
        provider_wire_request_json: None,
        context_budget: None,
        context_assembly: None,
        context_baseline: None,
        context_request_mode: None,
        context_injection: request.context_injection.clone(),
        system_prompt_hook_blocks,
        project_vmm,
        session_vmm,
        boundary_events: request.boundary_events.clone(),
        runtime_system_updates: request.runtime_system_updates.clone(),
        session_goal_context: request.session_goal_context.clone(),
        session_plan_context: request.session_plan_context.clone(),
        embedded_turn_usage_segments: request.embedded_turn_usage_segments.clone(),
    };
    let tool_snapshot = PreparedToolSnapshot {
        tools_manager,
        resolved_tool_definitions,
    };

    Ok(PreparedExecution {
        preparation,
        tool_snapshot,
    })
}

/// Builds the AI-visible skill developer segment only for user-turn-like requests.
/// 仅为类似用户回合的请求构建 AI 可见的技能开发者片段。
fn build_skill_developer_segment(
    purpose: ExecutionPromptPurpose,
    prompt_skills: &[vulcan_prompt_skills::ResolvedPromptSkill],
    implicit_luaskills: &[LuaSkillPromptEntry],
) -> Option<String> {
    if !purpose.supports_user_turn_tooling() {
        return None;
    }

    let mut segments = Vec::new();
    if let Some(segment) = render_prompt_skills_instructions(prompt_skills) {
        segments.push(segment);
    }
    if let Some(segment) = render_luaskills_prompt_segment(implicit_luaskills) {
        segments.push(segment);
    }
    (!segments.is_empty()).then(|| segments.join("\n\n"))
}

/// Builds project-level prompt-skill resolver input from one preparation request.
/// 从单条执行准备请求构建项目级提示词技能解析输入。
fn prompt_skills_project_config_from_request(
    request: &ExecutionPreparationRequest,
) -> PromptSkillsProjectConfig {
    PromptSkillsProjectConfig {
        skill_overrides: request
            .project_prompt_skills
            .skill_overrides
            .iter()
            .map(|(name, state)| {
                (
                    name.clone(),
                    crate::skills::prompt::state_from_persisted(*state),
                )
            })
            .collect(),
    }
}

/// Builds session-level prompt-skill resolver input from one preparation request.
/// 从单条执行准备请求构建会话级提示词技能解析输入。
fn prompt_skills_session_config_from_request(
    request: &ExecutionPreparationRequest,
) -> PromptSkillsSessionConfig {
    PromptSkillsSessionConfig {
        skill_overrides: request
            .session_prompt_skill_overrides
            .iter()
            .map(|(name, state)| {
                (
                    name.clone(),
                    crate::skills::prompt::state_from_persisted(*state),
                )
            })
            .collect(),
    }
}

/// Removes one resolved tool by public descriptor id from both descriptors and definitions.
/// 按公开描述符标识从描述与定义两侧移除一个已解析工具。
fn remove_resolved_tool(resolved_tools: &mut vulcan_tools_manager::ResolvedToolSet, tool_id: &str) {
    resolved_tools
        .descriptors
        .retain(|descriptor| descriptor.id != tool_id);
    resolved_tools
        .definitions
        .retain(|definition| definition.descriptor.id != tool_id);
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

#[cfg(test)]
#[path = "execution_preparation/tests.rs"]
mod tests;
