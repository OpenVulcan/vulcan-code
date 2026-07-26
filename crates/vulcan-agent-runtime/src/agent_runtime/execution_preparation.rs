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
use crate::vulcan_tools_bridge::{
    build_vulcan_tools_manager, resolve_ai_tool_host_binding_aliases,
};
use crate::workspace_path::normalize_workspace_path_text;
use std::path::PathBuf;
use vulcan_core::types::ConversationPayload as CoreConversationPayload;
use vulcan_luaskills_prompt::{LuaSkillPromptEntry, render_luaskills_prompt_segment};
use vulcan_prompt_skills::{
    PromptSkillsProjectConfig, PromptSkillsSessionConfig, render_prompt_skills_instructions,
    resolve_prompt_skills,
};
use vulcan_session_core::{CapabilityPolicyMode, CapabilityPolicySnapshot};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

#[path = "execution_preparation/codebase_tool_routing_prompt.rs"]
mod codebase_tool_routing_prompt;
#[path = "execution_preparation/tool_exposure.rs"]
mod tool_exposure;

use codebase_tool_routing_prompt::append_codebase_tool_routing_prompt;
use tool_exposure::{
    filter_resolved_tool_set_by_tool_exposure, filter_vulcan_plugin_tool_groups,
    resolve_execution_tool_exposure,
};

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
    let mut prompt_skill_catalog =
        vulcan_prompt_skills::load_prompt_skills(&prompt_skills_global_config);
    if let Some(plugin_catalog) = foundation.vulcan_plugin_catalog() {
        // A published plugin snapshot is part of the execution contract; invariant failures must not degrade silently.
        // 已发布插件快照属于执行契约的一部分；不变量失败不得静默降级。
        prompt_skill_catalog
            .merge_plugin_skills(plugin_catalog.enabled_skills.values().cloned())
            .map_err(|error| format!("合并插件技能目录失败：{error}"))?;
    }
    for error in &prompt_skill_catalog.errors {
        warnings.push(format!(
            "提示词技能 `{}` 载入失败：{}",
            error.path.display(),
            error.message
        ));
    }
    let mut prompt_skills = resolve_prompt_skills(
        &prompt_skill_catalog,
        &prompt_skills_global_config,
        &prompt_skills_project_config_from_request(request),
        &prompt_skills_session_config_from_request(request),
    );
    // Agent-owned visibility policy is independent from plugin lifecycle ownership and user state overrides.
    // 智能体自有可见性策略独立于插件生命周期归属及用户状态覆盖。
    prompt_skills.retain(|resolved| {
        prompt_skill_is_visible_for_policy(
            resolved.skill.name.as_str(),
            &request.prompt_skill_policy_override,
        )
    });
    let implicit_luaskills = tool_exposure.luaskills_dispatch_tools.clone();
    let implicit_luaskill_prompt_entries = tool_exposure.luaskill_prompt_entries.clone();
    let mut prompt_skill_developer_segment = build_skill_developer_segment(
        request.prompt_purpose,
        &prompt_skills,
        &implicit_luaskill_prompt_entries,
    );
    let enabled_prompt_skill_names = vulcan_prompt_skills::enabled_skill_names(&prompt_skills);
    // prepared_prompt_skills freezes parsed instructions for the complete execution turn.
    // prepared_prompt_skills 为完整执行回合冻结已解析技能说明。
    let prepared_prompt_skills = prompt_skills
        .iter()
        .filter(|resolved| resolved.is_enabled())
        .map(|resolved| resolved.skill.clone())
        .collect::<Vec<_>>();
    let ai_tool_host_binding_aliases = resolve_ai_tool_host_binding_aliases(&model_blueprint);
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
        foundation.vulcan_plugin_runtime(),
        filter_vulcan_plugin_tool_groups(
            foundation.resolved_vulcan_plugin_tools_for_agent(&active_agent.id),
            &tool_exposure,
        ),
        luaskills_runtime,
        (!workspace_path.is_empty()).then(|| PathBuf::from(workspace_path.clone())),
        request.controller_runtime.clone(),
        request.knowledge_access.clone(),
        conversation_attachments.clone(),
        tool_exposure.native_tool_ids.clone(),
        tool_exposure.luaskills_tools.clone(),
        implicit_luaskills,
        tool_exposure.mcp_tools.clone(),
        request.managed_codebase_memory.clone(),
        tool_exposure.ai_tool_configuration.clone(),
        ai_tool_host_binding_aliases.clone(),
        enabled_prompt_skill_names,
        prepared_prompt_skills,
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
    remove_request_hidden_tools(&mut resolved_tools, &request.tool_request_hidden_tool_ids);
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
    append_vulcan_plugin_prompt_contributions(
        &mut prompt_skill_developer_segment,
        foundation.vulcan_plugin_catalog().as_deref(),
        &resolved_tools.descriptors,
    );

    if workspace_path.is_empty() {
        warnings.push(String::from(
            "当前工作区路径为空，后续原生工具可能无法提供完整上下文。",
        ));
    }

    if request.prompt.trim().is_empty() && request.provider_visible_agent_messages.is_empty() {
        warnings.push(String::from(
            "当前提示词为空，本次准备计划仅验证运行时链路。",
        ));
    }

    if let Some(reasoning_note) = model_blueprint.reasoning.note.as_ref() {
        warnings.push(reasoning_note.clone());
    }

    push_missing_ai_tool_host_binding_warnings(
        &mut warnings,
        &ai_tool_host_binding_aliases,
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
    append_codebase_tool_routing_prompt(
        &mut prompt_skill_developer_segment,
        &resolved_tools.descriptors,
    );

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
        agent_prompt_snapshot: request.agent_prompt_snapshot.clone(),
        subagent_prompt_context: request.subagent_prompt_context.clone(),
        model_blueprint,
        prompt_preview: execution_prompt_preview(request),
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
        tool_request_rejection_reason: request.tool_request_rejection_reason.clone().or_else(
            || {
                request.prompt_purpose.rejects_tool_requests().then(|| {
                    String::from("Tool calls are not allowed during this internal execution.")
                })
            },
        ),
        tool_request_rejection_all_tools: request.tool_request_rejection_all_tools
            || (request.prompt_purpose.rejects_tool_requests()
                && request.tool_request_rejection_reason.is_none()),
        tool_request_rejection_tool_ids: request.tool_request_rejection_tool_ids.clone(),
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: mcp_overview.active_summary_rows().to_vec(),
        warnings,
        provider_request_preview_markdown: String::new(),
        provider_request_snapshot_json: None,
        provider_wire_request_json: None,
        context_budget: None,
        context_compaction_policy: request.context_compaction_policy,
        context_budget_override: request.context_budget_override,
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

/// Returns whether one resolved prompt skill remains visible under a request-scoped capability policy.
/// 返回单个已解析提示词技能在请求级能力策略下是否继续可见。
///
/// # Parameters
/// 参数
/// `skill_name` is the canonical prompt-skill identifier and `policy` is the normalized request snapshot.
/// `skill_name` 是规范提示词技能标识，`policy` 是已规范化的请求快照。
///
/// # Returns
/// 返回值
/// `true` when the skill may enter the developer prompt and detail-tool snapshot.
/// 当技能可以进入开发者提示词与详情工具快照时返回 `true`。
fn prompt_skill_is_visible_for_policy(skill_name: &str, policy: &CapabilityPolicySnapshot) -> bool {
    let selected = policy
        .selectors
        .iter()
        .any(|selector| selector == skill_name);
    match policy.mode {
        CapabilityPolicyMode::All => true,
        CapabilityPolicyMode::AllowOnly => selected,
        CapabilityPolicyMode::DenyOnly => !selected,
    }
}

/// Appends plugin-level instructions only when every declared tool dependency remains visible.
/// 仅在所有声明的工具依赖仍然可见时追加插件级说明。
fn append_vulcan_plugin_prompt_contributions(
    developer_segment: &mut Option<String>,
    catalog: Option<&vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    visible_tools: &[vulcan_tools_manager::VulcanToolDescriptor],
) {
    let Some(catalog) = catalog else {
        return;
    };
    let visible_ids = visible_tools
        .iter()
        .map(|descriptor| descriptor.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let plugin_instructions = catalog
        .prompt_contributions
        .iter()
        .filter(|contribution| {
            contribution
                .required_tools
                .iter()
                .all(|tool_id| visible_ids.contains(tool_id.as_str()))
        })
        .map(|contribution| contribution.body.trim())
        .filter(|body| !body.is_empty())
        .collect::<Vec<_>>();
    if plugin_instructions.is_empty() {
        return;
    }
    let plugin_segment = plugin_instructions.join("\n\n");
    match developer_segment {
        Some(segment) if !segment.trim().is_empty() => {
            segment.push_str("\n\n");
            segment.push_str(&plugin_segment);
        }
        Some(segment) => *segment = plugin_segment,
        None => *developer_segment = Some(plugin_segment),
    }
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

/// Removes request-hidden tools after normal exposure resolution has completed.
/// 在常规可见性解析完成后移除请求级强制隐藏工具。
fn remove_request_hidden_tools(
    resolved_tools: &mut vulcan_tools_manager::ResolvedToolSet,
    hidden_tool_ids: &[String],
) {
    for tool_id in hidden_tool_ids {
        remove_resolved_tool(resolved_tools, tool_id);
    }
}

/// Adds non-fatal diagnostics when a configured semantic AI fallback could not be exposed.
/// 当配置的语义 AI 回退工具无法暴露时追加非致命诊断。
fn push_missing_ai_tool_host_binding_warnings(
    warnings: &mut Vec<String>,
    aliases: &[crate::vulcan_tools_bridge::AiToolHostBindingAlias],
    resolved_tools: &vulcan_tools_manager::ResolvedToolSet,
) {
    for alias in aliases {
        let semantic_tool_name = alias.kind.key();
        let exposed = resolved_tools.descriptors.iter().any(|descriptor| {
            descriptor.source == vulcan_tools_manager::VulcanToolSourceKind::AiHostBinding
                && descriptor.id == semantic_tool_name
        });
        if !exposed {
            warnings.push(format!(
                "AI 专用工具 `{}` 已配置回退 `{}`，但本轮未找到可暴露的宿主工具。",
                alias.kind.label(),
                alias.host_tool_id
            ));
        }
    }
}

/// Builds the human-readable prompt preview for user and seeded-agent turns.
/// 为用户回合与已播种智能体回合构建人类可读提示预览。
///
/// # Parameters
/// 参数
/// `request` is the execution request being prepared.
/// `request` 是正在准备的执行请求。
///
/// # Returns
/// 返回值
/// A truncated preview of the user prompt or the first provider-visible agent message.
/// 返回用户提示词或第一条 Provider 可见智能体消息的截断预览。
fn execution_prompt_preview(request: &ExecutionPreparationRequest) -> String {
    let trimmed_prompt = request.prompt.trim();
    if !trimmed_prompt.is_empty() {
        return truncate_preview(trimmed_prompt, 180);
    }
    request
        .provider_visible_agent_messages
        .first()
        .map(|message| {
            CoreConversationPayload::render_agent_message_text(
                &message.message_type,
                &message.task_name,
                &message.sender,
                &message.payload,
            )
        })
        .map(|preview| truncate_preview(&preview, 180))
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "execution_preparation/tests.rs"]
mod tests;
