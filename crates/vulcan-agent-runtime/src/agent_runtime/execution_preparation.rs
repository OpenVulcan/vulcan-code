//! Execution-preparation builder for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备构建模块。

use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionPromptPurpose,
};
use crate::agent_runtime::provider::{
    resolve_git_commit_summary_provider_blueprint, resolve_public_ai_provider_blueprint,
};
use crate::agent_runtime::provider_stream_projection::truncate_preview;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_bootstrap_core::{
    after_precheck_trace, after_precheck_unix_millis,
};
use crate::context_budget::model_declared_context_limit;
use crate::conversation_protocol::collect_host_visible_attachments;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use crate::provider_tool_bridge::build_provider_tool_candidates;
use crate::tooling::native::SUBMIT_PLAN_TOOL_ID;
use crate::vulcan_tools_bridge::build_vulcan_tools_manager;
use crate::workspace_path::normalize_workspace_path_text;
use std::path::PathBuf;
use std::sync::Arc;
use vulcan_core::types::ConversationPayload as CoreConversationPayload;
use vulcan_luaskills_prompt::{
    LUASKILL_HELP_TOOL_NAME, LuaSkillInvocationRoutes, LuaSkillPackageHelp,
    render_luaskill_package_segment,
};
use vulcan_prompt_composer::{
    AdditionalPromptSegment, PromptBaselineKind, PromptBaselineProvenance,
};
use vulcan_prompt_skills::{
    PromptSkillsProjectConfig, PromptSkillsSessionConfig, render_prompt_skills_instructions,
    resolve_prompt_skills,
};
use vulcan_session_core::{CapabilityPolicyMode, CapabilityPolicySnapshot};
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolDescriptor, VulcanToolSourceKind, VulcanToolsManager,
};

pub(crate) mod captured_child_tool_admission;
#[path = "execution_preparation/codebase_tool_routing_prompt.rs"]
mod codebase_tool_routing_prompt;
mod dedicated_session;
#[path = "execution_preparation/tool_exposure.rs"]
mod tool_exposure;

use codebase_tool_routing_prompt::codebase_tool_routing_prompt;
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
    captured_child_tool_admission::validate_request(request)?;
    // preparationStartedAt measures the complete synchronous execution-preparation boundary.
    // preparationStartedAt 测量完整同步执行准备边界。
    let preparation_started_at = after_precheck_unix_millis();
    after_precheck_trace(
        "after_precheck_25a_execution_preparation_enter",
        format!("session={}", request.session_id),
    );
    let active_agent = match request.execution_agent_id.as_deref() {
        Some(agent_id) => foundation.agent_definition(agent_id).ok_or_else(|| {
            format!("Execution agent `{agent_id}` could not be resolved in the runtime.")
        })?,
        None => foundation.active_agent_definition().ok_or_else(|| {
            String::from("The active agent could not be resolved in the runtime.")
        })?,
    };

    if !foundation.has_built_agent(&active_agent.id) {
        return Err(format!(
            "Agent `{}` has not been built successfully, so execution cannot start.",
            active_agent.name
        ));
    }
    after_precheck_trace(
        "after_precheck_25a1_active_agent_ready",
        format!(
            "session={} agent_id={}",
            request.session_id, active_agent.id
        ),
    );

    let tool_exposure = resolve_execution_tool_exposure(foundation, &active_agent.id, request);
    after_precheck_trace(
        "after_precheck_25a2_tool_exposure_ready",
        format!(
            "session={} native={} luaskills={} luaskills_dispatch={} mcp={}",
            request.session_id,
            tool_exposure.native_tool_ids.len(),
            tool_exposure.luaskills_tools.len(),
            tool_exposure.luaskills_dispatch_tools.len(),
            tool_exposure.mcp_tools.len()
        ),
    );
    let public_ai_runtime = foundation
        .public_ai_runtime()
        .ok_or_else(|| String::from("The service has not installed the public AI runtime."))?;
    // sessionId scopes the process-local immutable provider-route lease.
    // sessionId 确定进程内不可变供应商路由租约的作用域。
    let session_id = request.session_id.to_string();
    // Git summaries are excluded from session V1 and may deliberately select another model.
    // Git 总结不属于会话 V1，并且可以明确选择另一个模型。
    // Preserve session ownership for accounting while keeping its route lease untouched.
    // 保留统计中的会话归属，同时不触碰会话路由租约。
    let model_blueprint = if request.prompt_purpose == ExecutionPromptPurpose::GitCommitSummary {
        // summaryTarget must come from the service's validated model selection, never global defaults.
        // summaryTarget 必须来自服务已校验的模型选择，绝不取全局默认值。
        let (provider_id, model_id) = request
            .provider_id_override
            .as_deref()
            .zip(request.model_id_override.as_deref())
            .ok_or_else(|| {
                String::from("Git commit summary requires an explicit provider and model target.")
            })?;
        resolve_git_commit_summary_provider_blueprint(
            public_ai_runtime.as_ref(),
            provider_id,
            model_id,
            request.reasoning_effort,
        )?
    } else {
        resolve_public_ai_provider_blueprint(
            public_ai_runtime.as_ref(),
            session_id.as_str(),
            request.provider_route_lease_identity.as_ref(),
            request.reasoning_effort,
            request.provider_id_override.as_deref(),
            request.model_id_override.as_deref(),
            request.connection_id_override.as_deref(),
        )?
    };
    after_precheck_trace(
        "after_precheck_25a3_provider_blueprint_ready",
        format!(
            "session={} provider={} model={} protocol={}",
            request.session_id,
            model_blueprint.provider_id,
            model_blueprint.model_id,
            model_blueprint.protocol_profile_id
        ),
    );
    // contextWindowPolicySnapshot freezes system/provider/model defaults at the safe turn boundary.
    // contextWindowPolicySnapshot 在安全回合边界冻结系统、供应商与模型默认值。
    let public_ai_configuration = public_ai_runtime.configuration_snapshot()?;
    let resolved_context_compaction_policy = public_ai_configuration
        .resolve_context_compaction_policy(
            model_blueprint.provider_id.as_str(),
            model_blueprint.model_id.as_str(),
            model_declared_context_limit(&model_blueprint.public_capabilities),
            request.context_compaction_window_node_id,
            request.context_compaction_policy,
        )?;
    after_precheck_trace(
        "after_precheck_25a4_provider_configuration_ready",
        format!("session={}", request.session_id),
    );
    let mut warnings = Vec::new();
    if resolved_context_compaction_policy.tail_policy_source
        == vulcan_context::ContextCompactionTailPolicySource::MissingContextLimitMinimum
    {
        warnings.push(format!(
            "Model context_limit is not declared; context compaction retained-tail policy uses the minimum 5000 tokens / 5 assistant-provider segments (provider={}, connection={}, model={}).",
            model_blueprint.provider_id,
            model_blueprint.connection_id,
            model_blueprint.model_id
        ));
    }
    let workspace_path = normalize_workspace_path_text(&request.workspace_path);
    let conversation_attachments = collect_host_visible_attachments(&request.conversation_state);
    let prompt_skills_global_config =
        crate::skills::prompt::global_config_from_persisted(&config.prompt_skills);
    let prompt_skill_user_root = Some(vulcan_prompt_skills::configured_user_skill_root(
        &prompt_skills_global_config,
    ));
    after_precheck_trace(
        "after_precheck_25a5_prompt_skill_catalog_snapshot_enter",
        format!("session={}", request.session_id),
    );
    // promptSkillCatalog is an immutable generation captured before submission; execution
    // preparation must never enumerate skill directories.
    // promptSkillCatalog 是提交前捕获的不可变代次；执行准备绝不能枚举技能目录。
    let mut prompt_skill_catalog = foundation.prompt_skill_catalog().as_ref().clone();
    after_precheck_trace(
        "after_precheck_25a6_prompt_skill_catalog_snapshot_ready",
        format!(
            "session={} errors={}",
            request.session_id,
            prompt_skill_catalog.errors.len()
        ),
    );
    if let Some(plugin_catalog) = foundation.vulcan_plugin_catalog() {
        // A published plugin snapshot is part of the execution contract; invariant failures must not degrade silently.
        // 已发布插件快照属于执行契约的一部分；不变量失败不得静默降级。
        prompt_skill_catalog
            .merge_plugin_skills(plugin_catalog.enabled_skills.values().cloned())
            .map_err(|error| format!("Failed to merge plugin skill directories: {error}"))?;
    }
    for error in &prompt_skill_catalog.errors {
        warnings.push(format!(
            "Failed to load prompt skill `{}`: {}",
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
    after_precheck_trace(
        "after_precheck_25a7_prompt_skills_resolved",
        format!(
            "session={} resolved={}",
            request.session_id,
            prompt_skills.len()
        ),
    );
    // Agent-owned visibility policy is independent from plugin lifecycle ownership and user state overrides.
    // 智能体自有可见性策略独立于插件生命周期归属及用户状态覆盖。
    prompt_skills.retain(|resolved| {
        prompt_skill_is_visible_for_policy(
            resolved.skill.name.as_str(),
            &request.prompt_skill_policy_override,
        )
    });
    if request.dedicated_session.as_ref().is_some_and(|profile| {
        profile.tool_access == vulcan_session_core::v1::DedicatedToolAccess::None
    }) {
        // Without general tools, prompt skills have no invocation route and are not described.
        // 未开放通用工具时，提示词技能没有调用途径，因此不作描述。
        prompt_skills.clear();
    }
    let implicit_luaskills = tool_exposure.luaskills_dispatch_tools.clone();
    let mut additional_developer_segments = Vec::new();
    if let Some(segment) = build_skill_developer_segment(request.prompt_purpose, &prompt_skills) {
        additional_developer_segments.push(AdditionalPromptSegment::locked(
            "Prompt Skill Injection",
            segment,
            "Enabled prompt-skill instructions",
            PromptBaselineProvenance::new(
                PromptBaselineKind::PromptSkillInjection,
                "enabled-prompt-skills",
            ),
        ));
    }
    let enabled_prompt_skill_names = vulcan_prompt_skills::enabled_skill_names(&prompt_skills);
    // prepared_prompt_skills freezes parsed instructions for the complete execution turn.
    // prepared_prompt_skills 为完整执行回合冻结已解析技能说明。
    let prepared_prompt_skills = prompt_skills
        .iter()
        .filter(|resolved| resolved.is_enabled())
        .map(|resolved| resolved.skill.clone())
        .collect::<Vec<_>>();
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
    let workspace_root =
        (!workspace_path.is_empty()).then(|| PathBuf::from(workspace_path.clone()));
    let suppress_external_web_search = model_blueprint
        .public_capabilities
        .provider_native_tools
        .contains(&vulcan_ai_contract::NativeToolKind::WebSearch);
    if suppress_external_web_search {
        warnings.push(String::from(
            "The exact model route uses the provider's native web search, so external web search tools are hidden this turn; the saved external search settings are unchanged.",
        ));
    }
    let provider_tool_candidates = build_provider_tool_candidates(
        Arc::clone(&public_ai_runtime),
        public_ai_configuration,
        workspace_root.clone(),
        &request.provider_tool_selections_override,
        &request.provider_tool_enabled_capabilities_override,
        &request.provider_extension_target_sets_override,
        &request.provider_tool_media_understanding_selections_override,
        suppress_external_web_search,
        model_blueprint.frozen_selection.as_ref(),
        request.media_resource_runtime.clone(),
    )?;
    after_precheck_trace(
        "after_precheck_25a8_provider_tool_candidates_ready",
        format!(
            "session={} candidates={}",
            request.session_id,
            provider_tool_candidates.len()
        ),
    );
    let tools_manager = build_vulcan_tools_manager(
        foundation.task_runtime(),
        foundation.question_runtime(),
        foundation.native_tool_catalog().clone(),
        foundation.vulcan_plugin_runtime(),
        filter_vulcan_plugin_tool_groups(
            foundation.resolved_vulcan_plugin_tools_for_agent(&active_agent.id),
            &tool_exposure,
        ),
        provider_tool_candidates,
        luaskills_runtime,
        workspace_root,
        request.project_id.clone(),
        request.controller_runtime.clone(),
        request.native_goal_runtime.clone(),
        request.knowledge_access.clone(),
        request.vce_retrieval_handler.clone(),
        tool_exposure.native_tool_ids.clone(),
        tool_exposure.luaskills_tools.clone(),
        implicit_luaskills,
        tool_exposure.mcp_tools.clone(),
        enabled_prompt_skill_names,
        prepared_prompt_skills,
        prompt_skill_user_root.clone(),
        tool_exposure.luaskill_help_packages.clone(),
        project_vmm.clone(),
        session_vmm.clone(),
        request.vmm_runtime_ready,
        request.native_browser_runtime.clone(),
        request.native_agent_collaboration_runtime.clone(),
    );
    after_precheck_trace(
        "after_precheck_25a9_tools_manager_ready",
        format!("session={}", request.session_id),
    );
    let mut resolved_tools = tools_manager.resolve_for_execution_mode(&request.execution_mode);
    after_precheck_trace(
        "after_precheck_25a10_tools_resolved",
        format!(
            "session={} descriptors={} definitions={}",
            request.session_id,
            resolved_tools.descriptors.len(),
            resolved_tools.definitions.len()
        ),
    );
    if !request.execution_mode.eq_ignore_ascii_case("plan") {
        remove_resolved_tool(&mut resolved_tools, SUBMIT_PLAN_TOOL_ID);
    }
    filter_resolved_tool_set_by_tool_exposure(&mut resolved_tools, &tool_exposure);
    remove_request_hidden_tools(&mut resolved_tools, &request.tool_request_hidden_tool_ids);
    if request.prompt_purpose == ExecutionPromptPurpose::QuestionExecution {
        remove_question_execution_business_tools(&mut resolved_tools);
    }
    captured_child_tool_admission::filter_tools(
        &mut resolved_tools,
        request.captured_child_tool_admission.as_ref(),
    );
    dedicated_session::filter_tools(&mut resolved_tools, request.dedicated_session.as_ref());
    let prompt_skill_tool_available = request.prompt_purpose.supports_user_turn_tooling()
        && render_prompt_skills_instructions(&prompt_skills).is_some();
    if !prompt_skill_tool_available {
        remove_resolved_tool(&mut resolved_tools, "skill");
    }
    if request.prompt_purpose.disables_provider_tools() {
        if !resolved_tools.descriptors.is_empty() {
            warnings.push(format!(
                "Internal `{}` execution does not allow tool calls; {} tools are hidden this turn.",
                request.prompt_purpose.diagnostics_label(),
                resolved_tools.descriptors.len()
            ));
        }
        resolved_tools.descriptors.clear();
        resolved_tools.definitions.clear();
    }
    append_vulcan_plugin_prompt_contributions(
        &mut additional_developer_segments,
        foundation.vulcan_plugin_catalog().as_deref(),
        &resolved_tools.descriptors,
    );
    if request.prompt_purpose.supports_user_turn_tooling() {
        append_luaskill_package_segments(
            &mut additional_developer_segments,
            &tool_exposure.luaskill_help_packages,
            &resolved_tools.descriptors,
        );
    }

    if workspace_path.is_empty() {
        warnings.push(String::from(
            "The workspace path is empty; native tools may not have full context.",
        ));
    }

    if request.prompt.trim().is_empty() && request.provider_visible_agent_messages.is_empty() {
        warnings.push(String::from(
            "The prompt is empty; this preparation only validates the runtime chain.",
        ));
    }

    if let Some(reasoning_note) = model_blueprint.reasoning.note.as_ref() {
        warnings.push(reasoning_note.clone());
    }

    if let Some(segment) = codebase_tool_routing_prompt(&resolved_tools.descriptors) {
        additional_developer_segments.push(AdditionalPromptSegment::dynamic(
            "Codebase Tool Routing",
            segment,
            "Final provider-visible codebase tool contract",
        ));
    }

    let temperature = if model_blueprint.supports_temperature() {
        request.temperature
    } else {
        if request.temperature.is_some() {
            warnings.push(format!(
                "Model `{}` does not declare temperature support; this turn falls back to the model's default sampling.",
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
        conversation_hook_turn_id: request.conversation_hook_turn_id.clone(),
        continuation_action_id: request.continuation_action_id.clone(),
        session_title: request.session_title.clone(),
        agent_id: active_agent.id.clone(),
        agent_name: active_agent.name.clone(),
        agent_kind: active_agent.kind.label().to_string(),
        agent_prompt_snapshot: request.agent_prompt_snapshot.clone(),
        dedicated_session: request.dedicated_session.clone(),
        subagent_prompt_context: request.subagent_prompt_context.clone(),
        model_blueprint,
        prompt_preview: execution_prompt_preview(request),
        workspace_path,
        session_storage_directory: request.session_storage_directory.clone(),
        conversation_attachments,
        context_history_count: request.conversation_state.messages.len(),
        execution_mode: request.execution_mode.clone(),
        temperature,
        prompt_configuration: config.prompt.clone(),
        output_style_snapshot: request.output_style_snapshot.clone(),
        prompt_purpose: request.prompt_purpose,
        prompt_skills,
        prompt_skill_user_root,
        additional_developer_segments,
        skill_names: foundation.skill_names_for_agent(&active_agent.id),
        tool_descriptors,
        // Internal execution policy is mandatory even when the source request rejected only selected tools.
        // 即使来源请求只拒绝部分工具，内部执行策略也必须强制生效。
        tool_request_rejection_reason: if request.prompt_purpose.rejects_tool_requests() {
            Some(String::from(
                "Tool calls are not allowed during this internal execution.",
            ))
        } else {
            request.tool_request_rejection_reason.clone()
        },
        tool_request_rejection_all_tools: request.tool_request_rejection_all_tools
            || request.prompt_purpose.rejects_tool_requests(),
        tool_request_rejection_tool_ids: request.tool_request_rejection_tool_ids.clone(),
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: mcp_overview.active_summary_rows().to_vec(),
        warnings,
        provider_request_preview_markdown: String::new(),
        provider_request_snapshot_json: None,
        context_budget: None,
        context_compaction_policy: Some(resolved_context_compaction_policy),
        context_budget_override: request.context_budget_override,
        context_assembly: None,
        context_baseline: None,
        context_request_mode: None,
        context_injection: request.context_injection.clone(),
        system_prompt_hook_blocks,
        project_vmm,
        session_vmm,
        vmm_runtime_ready: request.vmm_runtime_ready,
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
    after_precheck_trace(
        "after_precheck_25a11_execution_preparation_complete",
        format!(
            "session={} tools={} elapsed_ms={}",
            request.session_id,
            tool_snapshot.resolved_tool_definitions.len(),
            after_precheck_unix_millis().saturating_sub(preparation_started_at)
        ),
    );

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
    developer_segments: &mut Vec<AdditionalPromptSegment>,
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
        .filter_map(|contribution| {
            let body = contribution.body.trim();
            (!body.is_empty()).then_some((contribution, body))
        })
        .collect::<Vec<_>>();
    for (contribution, body) in plugin_instructions {
        developer_segments.push(AdditionalPromptSegment::locked(
            format!(
                "Plugin Instructions: {}/{}",
                contribution.plugin_id, contribution.id
            ),
            body,
            format!("Plugin {}/{}", contribution.plugin_id, contribution.id),
            PromptBaselineProvenance::new(
                PromptBaselineKind::PluginInstructions,
                format!("{}/{}", contribution.plugin_id, contribution.id),
            ),
        ));
    }
}

/// Builds the AI-visible prompt-skill developer segment only for user-turn-like requests.
/// 仅为类似用户回合的请求构建 AI 可见的提示词技能开发者片段。
fn build_skill_developer_segment(
    purpose: ExecutionPromptPurpose,
    prompt_skills: &[vulcan_prompt_skills::ResolvedPromptSkill],
) -> Option<String> {
    if !purpose.supports_user_turn_tooling() {
        return None;
    }
    render_prompt_skills_instructions(prompt_skills)
}

/// Appends one locked developer segment per LuaSkills package that keeps an invocable entry.
/// 为每个仍含可调用入口的 LuaSkills 包追加一条锁定开发者片段。
///
/// # Parameters
/// 参数
/// - `developer_segments`: Developer segments of the prepared request.
/// - `developer_segments`：已准备请求的开发者片段。
/// - `packages`: Help trees limited to request-visible entries.
/// - `packages`：限定为请求可见入口的帮助树。
/// - `visible_tools`: Tool descriptors remaining after all host and model filters.
/// - `visible_tools`：经过全部宿主与模型过滤后保留的工具描述。
fn append_luaskill_package_segments(
    developer_segments: &mut Vec<AdditionalPromptSegment>,
    packages: &[LuaSkillPackageHelp],
    visible_tools: &[VulcanToolDescriptor],
) {
    if packages.is_empty() {
        return;
    }
    let native_visible = |tool_id: &str| {
        visible_tools.iter().any(|descriptor| {
            descriptor.id == tool_id && descriptor.source == VulcanToolSourceKind::Native
        })
    };
    let routes = LuaSkillInvocationRoutes {
        function_tools: visible_tools
            .iter()
            .filter(|descriptor| descriptor.source == VulcanToolSourceKind::LuaSkills)
            .map(|descriptor| {
                (
                    descriptor.origin().original_name().to_string(),
                    descriptor.id.clone(),
                )
            })
            .collect(),
        dispatcher_visible: native_visible(CALL_LUASKILL_TOOL_NAME),
        help_tool_visible: native_visible(LUASKILL_HELP_TOOL_NAME),
    };
    for package in packages {
        let Some(body) = render_luaskill_package_segment(package, &routes) else {
            continue;
        };
        developer_segments.push(AdditionalPromptSegment::locked(
            format!("LuaSkills Package: {}", package.skill_id),
            body,
            format!("LuaSkills package {}", package.skill_id),
            PromptBaselineProvenance::new(
                PromptBaselineKind::PromptSkillInjection,
                format!("luaskills/{}", package.skill_id),
            ),
        ));
    }
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

/// Removes business lifecycle mutations from both the model catalog and executable definitions.
/// 从模型目录和可执行定义中同时移除业务生命周期修改工具。
/// Existing profile and mode restrictions remain authoritative for every retained tool.
/// 每个保留工具仍遵守既有角色与模式限制。
fn remove_question_execution_business_tools(
    resolved_tools: &mut vulcan_tools_manager::ResolvedToolSet,
) {
    // Exact registered IDs prevent auxiliary Q&A from creating obligations or restarting business modes.
    // 精确注册 ID 防止辅助答疑创建业务义务或重新启动业务模式。
    let forbidden = [
        vulcan_subagent_core::SPAWN_AGENT_TOOL,
        vulcan_subagent_core::SEND_MESSAGE_TOOL,
        vulcan_subagent_core::INTERRUPT_AGENT_TOOL,
        vulcan_subagent_core::CLOSE_AGENT_TOOL,
        crate::tooling::collaboration::SEND_SESSION_MESSAGE_TOOL_ID,
        crate::tooling::native::CREATE_GOAL_TOOL_ID,
        crate::tooling::native::UPDATE_GOAL_TOOL_ID,
        crate::tooling::native::SUBMIT_PLAN_TOOL_ID,
        crate::tooling::native::TODOLIST_TOOL_ID,
    ];
    for tool_id in forbidden {
        remove_resolved_tool(resolved_tools, tool_id);
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
                &message.delivery_id,
                message.kind,
                &message.sender,
                &message.recipient,
                &message.task_id,
                &message.payload,
            )
            .map(|preview| truncate_preview(&preview, 180))
            .unwrap_or_else(|error| format!("Agent message rendering failed: {error}"))
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "execution_preparation/tests.rs"]
pub(in crate::agent_runtime) mod tests;
