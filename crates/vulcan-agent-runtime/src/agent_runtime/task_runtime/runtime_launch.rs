use super::*;

/// Native subagent orchestration tools reserved from child-agent execution by default.
/// 普通子智能体默认禁止执行的原生子智能体调度工具集合。
const SUBAGENT_ORCHESTRATION_TOOL_IDS: [&str; 7] = [
    SPAWN_AGENT_TOOL,
    AGENT_ORCHESTRATION_INFO_TOOL,
    SEND_MESSAGE_TOOL,
    WAIT_AGENT_TOOL,
    INTERRUPT_AGENT_TOOL,
    CLOSE_AGENT_TOOL,
    LIST_AGENTS_TOOL,
];

/// Runtime tools that remain registered for child agents but are rejected when invoked.
/// 子智能体中保持注册但执行时拒绝调用的运行时工具集合。
const SUBAGENT_EXECUTION_REJECTED_TOOL_IDS: [&str; 1] = [TODOLIST_TOOL_ID];

/// Message returned when a child agent invokes a session-level or parent-reserved tool.
/// 子智能体调用会话级或父级保留工具时返回的提示信息。
pub(super) const SUBAGENT_REJECTED_TOOL_REASON: &str = "This child agent cannot execute parent-reserved session tools such as todolist or child-agent orchestration tools. Use the other available tools to complete the delegated task and return the result to the parent agent.";

/// Builds one task-session execution request and provider configuration from a shared parent runtime context.
/// 从共享父级运行时上下文构建一份子代理会话执行请求与供应商配置。
pub(super) fn build_task_runtime_preparation_request(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> (ProviderConfigurationFile, ExecutionPreparationRequest) {
    let mut config = context.config.clone();
    apply_task_provider_model_override(
        &mut config,
        &resolved_config.effective_config.provider_model,
    );
    let mut request = context.request_template.clone();
    request.session_id = invocation.task_id;
    request.controller_runtime = None;
    request.execution_scope = ExecutionScope::TaskSession {
        root_session_id: SessionNodeId::from(invocation.root_session_id),
        task_session_id: SessionNodeId::from(invocation.task_id),
    };
    request.runtime_policy = resolved_config.runtime_policy;
    request.execution_agent_id = context.request_template.execution_agent_id.clone();
    request.agent_system_prompt_override = invocation
        .controller_config
        .as_ref()
        .and_then(|config| normalize_optional_task_label(Some(&config.system_prompt)));
    request.task_session_store_root = context.request_template.task_session_store_root.clone();
    request.session_title = invocation.display_title.clone();
    request.prompt = invocation.prompt.clone();
    request.conversation_state = core_snapshot_to_session_conversation_state(&invocation.snapshot);
    request.tool_exposure_override = resolved_config.effective_config.tool_exposure.clone();
    let mirror_subagent =
        resolved_config.effective_config.subagent_mode.as_deref() == Some(SUBAGENT_MODE_MIRROR);
    if mirror_subagent || !context.allow_subagent_tools {
        reject_registered_child_tools(&mut request, &SUBAGENT_ORCHESTRATION_TOOL_IDS);
    }
    reject_registered_child_tools(&mut request, &SUBAGENT_EXECUTION_REJECTED_TOOL_IDS);
    if let Some(execution_mode) = resolved_config
        .effective_config
        .provider_model
        .execution_mode
        .as_ref()
        .map(String::as_str)
        .and_then(|value| normalize_optional_task_label(Some(value)))
    {
        request.execution_mode = execution_mode;
    }
    if let Some(temperature) = resolved_config
        .effective_config
        .extra
        .get("temperature")
        .and_then(Value::as_f64)
    {
        request.temperature = Some(temperature.clamp(0.0, 2.0) as f32);
    }
    apply_task_prompt_skill_snapshot(
        &mut request,
        &resolved_config.effective_config.prompt_skills,
    );
    (config, request)
}

/// Adds child-forbidden tool ids to the request rejection list without hiding their schemas.
/// 将子智能体禁止执行的工具加入请求拒绝列表，同时不隐藏它们的 Schema。
///
/// # Parameters
/// 参数
/// `request` is the mutable child execution preparation request.
/// `request` 是可变的子智能体执行准备请求。
/// `tool_ids` are model-visible tool ids that should fail when invoked by the child.
/// `tool_ids` 是对子智能体可见但调用时应失败的工具标识。
fn reject_registered_child_tools(request: &mut ExecutionPreparationRequest, tool_ids: &[&str]) {
    request.tool_request_rejection_reason = Some(SUBAGENT_REJECTED_TOOL_REASON.to_string());
    request.tool_request_rejection_all_tools = false;
    for tool_id in tool_ids {
        if !request
            .tool_request_rejection_tool_ids
            .iter()
            .any(|existing| existing == tool_id)
        {
            request
                .tool_request_rejection_tool_ids
                .push((*tool_id).to_string());
        }
    }
}

/// Applies one TaskSession provider/model snapshot to a mutable provider configuration clone.
/// 将一份 TaskSession 供应商与模型快照应用到可变供应商配置副本上。
pub(super) fn apply_task_provider_model_override(
    config: &mut ProviderConfigurationFile,
    provider_model: &ProviderModelSnapshot,
) {
    if let Some((provider_id, model_id)) =
        provider_model_selector_from_snapshot(provider_model, config)
    {
        config.model = Some(format!("{provider_id}/{model_id}"));
    }
}

/// Applies TaskSession prompt-skill overrides to the child execution request.
/// 将 TaskSession 提示词技能覆盖应用到子代理执行请求。
pub(super) fn apply_task_prompt_skill_snapshot(
    request: &mut ExecutionPreparationRequest,
    prompt_skills: &PromptSkillSnapshot,
) {
    for skill in &prompt_skills.enabled {
        request
            .session_prompt_skill_overrides
            .insert(skill.clone(), PersistedPromptSkillState::Enabled);
    }
    for skill in &prompt_skills.disabled {
        request
            .session_prompt_skill_overrides
            .insert(skill.clone(), PersistedPromptSkillState::Disabled);
    }
}

/// Builds a core controller configuration snapshot from a fully prepared child launch.
/// 从一份完整准备后的子代理启动载荷构建核心控制器配置快照。
pub(super) fn core_controller_config_from_child_launch(
    launch: &crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch,
) -> CoreControllerConfig {
    let max_turns = launch
        .preparation
        .prompt_purpose
        .max_turns_override()
        .or_else(|| {
            launch
                .active_agent
                .as_ref()
                .and_then(|agent| agent.max_turns)
                .map(usize::from)
        })
        .unwrap_or(usize::MAX - 1);
    CoreControllerConfig {
        provider_id: launch.preparation.model_blueprint.provider_id.clone(),
        model_id: launch.preparation.model_blueprint.model_id.clone(),
        system_prompt: launch.system_preamble.clone(),
        prompt_layers: CoreProviderPromptLayers::from_system_prompt(launch.system_preamble.clone()),
        temperature: launch.preparation.temperature.map(f64::from),
        max_turns,
        additional_params: None,
        context_injection: launch.preparation.context_injection.clone(),
    }
}

/// Builds one child runtime-stream launch through the same preparation and bootstrap bridge used by main sessions.
/// 通过主会话使用的同一准备与启动桥构建一份子代理运行时流启动载荷。
pub(crate) async fn prepare_task_runtime_stream_launch(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch, String> {
    let (config, request) =
        build_task_runtime_preparation_request(context, invocation, resolved_config);
    let prepared = prepare_execution_with_tool_snapshot(
        &context.foundation,
        context.luaskills_runtime.clone(),
        &config,
        &request,
    )?;
    let bootstrap = bootstrap_runtime_stream(
        &context.foundation,
        context.luaskills_runtime.clone(),
        &request,
        prepared.preparation,
        prepared.tool_snapshot,
    )
    .await?;
    let child_context = TaskExecutionRuntimeContext::new(
        config,
        Arc::clone(&context.foundation),
        context.luaskills_runtime.clone(),
        request.clone(),
    )
    .with_parent_snapshot(bootstrap.core_snapshot.clone())
    .with_subagent_tools_allowed(context.allow_subagent_tools);
    Ok(build_runtime_stream_launch_payload(
        &request,
        bootstrap,
        Some(child_context),
    ))
}

/// Builds one child runtime-stream launch and performs a persisted preflight compaction when the first preparation already exceeds the budget.
/// 构建子代理运行时流启动载荷，并在首次准备已超过预算时执行一次持久化前置压缩。
pub(crate) async fn prepare_task_runtime_stream_launch_with_preflight_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: &TaskExecutionRuntimeContext,
    invocation: &mut TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch, String> {
    let launch = prepare_task_runtime_stream_launch(context, invocation, resolved_config).await?;
    if !resolved_config
        .effective_config
        .context_compaction
        .auto_compaction_enabled
        || !task_launch_should_compact(&launch)
    {
        return Ok(launch);
    }

    if let Some(compacted_snapshot) = run_task_session_context_compaction(
        task_runtime,
        context.clone(),
        invocation.clone(),
        resolved_config.clone(),
    )
    .await?
    {
        invocation.snapshot = compacted_snapshot;
        return prepare_task_runtime_stream_launch(context, invocation, resolved_config).await;
    }

    Ok(launch)
}

/// Returns whether a prepared child launch should compact its context before provider execution.
/// 返回一份已准备的子代理启动载荷是否应在 Provider 执行前压缩上下文。
pub(super) fn task_launch_should_compact(
    launch: &crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch,
) -> bool {
    launch
        .preparation
        .context_budget
        .as_ref()
        .is_some_and(|context_budget| context_budget.should_compact())
}

/// Converts a terminal runtime report back into the TaskSession outcome shape expected by persistence and parent cards.
/// 将终态运行时报告转换回持久化与父级卡片期望的 TaskSession 结果形态。
pub(super) fn task_execution_outcome_from_report(
    invocation: &TaskExecutionRequest,
    controller_config: CoreControllerConfig,
    resolved_config: TaskSessionResolvedConfig,
    report: ExecutionRunReport,
) -> Result<TaskExecutionOutcome, String> {
    if let Some(error) = report.error {
        return Err(error);
    }
    let usage_segments = subagent_usage_segments_from_report(invocation, &report);
    let snapshot = report
        .conversation_state
        .as_ref()
        .map(build_core_conversation_snapshot)
        .unwrap_or_else(|| invocation.snapshot.clone());
    let final_output_text = report
        .assistant_output
        .filter(|output| !output.trim().is_empty())
        .or_else(|| extract_final_assistant_text(&snapshot))
        .unwrap_or_else(|| {
            String::from(
                "The task completed, but it did not produce reusable final assistant text.",
            )
        });
    Ok(TaskExecutionOutcome {
        task_id: invocation.task_id,
        controller_config,
        resolved_config,
        final_assistant_message_id: extract_final_assistant_message_id(&snapshot),
        snapshot,
        provider_turns_executed: report.provider_request_count,
        tool_calls_processed: report
            .controller_events
            .iter()
            .filter(|event| matches!(event.kind, vulcan_core::types::CoreEventKind::ToolFinished))
            .count(),
        final_output_text,
        execution_status: TaskExecutionStatus::Completed,
        usage_segments,
    })
}

/// Builds the parent-turn usage segment represented by one terminal child-agent report.
/// 根据一份终态子代理报告构建其所属父会话回合的用量分段。
fn subagent_usage_segments_from_report(
    invocation: &TaskExecutionRequest,
    report: &ExecutionRunReport,
) -> Vec<SessionTurnUsageSegment> {
    let mut segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        report.provider_request_count,
        report.last_successful_provider_usage.as_ref(),
    );
    segment.trigger = Some(invocation.public_task_id.clone());
    if let Some(preparation) = report.preparation.as_ref() {
        segment.provider_id = Some(preparation.model_blueprint.provider_id.clone());
        segment.model_id = Some(preparation.model_blueprint.model_id.clone());
    }
    if segment.provider_call_count == 0 && segment.total_tokens == 0 {
        return Vec::new();
    }
    vec![segment]
}
