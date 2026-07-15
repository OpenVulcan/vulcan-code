use super::*;

/// Message returned when a child agent invokes a session-level or parent-reserved tool.
/// 子智能体调用会话级或父级保留工具时返回的提示信息。
pub(super) const SUBAGENT_REJECTED_TOOL_REASON: &str = "The current execution is running in child-agent mode. This tool is reserved for the parent session and cannot be used here. Continue with available tools and put anything that needs parent action in the final response.";

/// Builds one task-session execution request and provider configuration from a shared parent runtime context.
/// 从共享父级运行时上下文构建一份子代理会话执行请求与供应商配置。
///
/// # Parameters
/// 参数
/// `context` provides the shared runtime foundation and parent request template.
/// `context` 提供共享运行时基础与父请求模板。
/// `invocation` provides the isolated task identity, prompt, and canonical snapshot.
/// `invocation` 提供隔离任务身份、提示词与规范快照。
/// `resolved_config` provides the immutable effective task configuration.
/// `resolved_config` 提供不可变的任务有效配置。
///
/// # Returns
/// 返回值
/// Returns the task provider configuration and validated request, or the exact snapshot conversion failure.
/// 返回任务 Provider 配置与经过校验的请求，或返回准确的快照转换失败。
pub(super) fn build_task_runtime_preparation_request(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<(ProviderConfigurationFile, ExecutionPreparationRequest), String> {
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
    request.agent_prompt_snapshot =
        task_record_agent_prompt_snapshot(&context.foundation.task_runtime(), invocation.task_id)?
            .or_else(|| context.agent_prompt_snapshot.clone());
    request.subagent_prompt_context =
        build_task_subagent_prompt_context(invocation, resolved_config);
    request.task_session_store_root = context.request_template.task_session_store_root.clone();
    request.session_title = invocation.display_title.clone();
    request.prompt = invocation.prompt.clone();
    request.conversation_state = core_snapshot_to_session_conversation_state(&invocation.snapshot)?;
    request.context_compaction_policy = Some(task_context_compaction_policy_from_snapshot(
        &resolved_config.effective_config.context_compaction,
    ));
    request.context_budget_override = task_context_budget_override_from_snapshot(
        &resolved_config.effective_config.context_compaction,
    );
    request.tool_exposure_override = resolved_config.effective_config.tool_exposure.clone();
    clear_parent_request_runtime_state_for_task_session(&mut request);
    let mirror_subagent =
        resolved_config.effective_config.subagent_mode.as_deref() == Some(SUBAGENT_MODE_MIRROR);
    apply_child_tool_policy(&mut request, mirror_subagent);
    if let Some(execution_mode) = resolved_config
        .effective_config
        .provider_model
        .execution_mode
        .as_deref()
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
    Ok((config, request))
}

/// Clears parent-turn runtime state that must not leak into an isolated TaskSession request.
/// 清理不得泄漏进隔离 TaskSession 请求的父回合运行态。
///
/// # Parameters
/// 参数
/// `request` is the cloned parent request being rewritten into a child-agent request.
/// `request` 是正在改写为子智能体请求的父级请求克隆。
fn clear_parent_request_runtime_state_for_task_session(request: &mut ExecutionPreparationRequest) {
    request.conversation_hook_turn_id = None;
    request.tool_request_rejection_reason = None;
    request.tool_request_rejection_all_tools = false;
    request.tool_request_rejection_tool_ids.clear();
    request.tool_request_hidden_tool_ids.clear();
    request.system_prompt_hook_blocks.clear();
    request.context_injection = None;
    request.boundary_events.clear();
    request.runtime_system_updates.clear();
    request.session_goal_context = None;
    request.session_plan_context = None;
    request.embedded_turn_usage_segments.clear();
}

/// Converts the resolved TaskSession context budget into a request-level input budget override.
/// 将已解析的 TaskSession 上下文预算转换为请求级输入预算覆盖。
///
/// # Parameters
/// 参数
/// `snapshot` is the resolved TaskSession context-compaction policy snapshot.
/// `snapshot` 是已解析的 TaskSession 上下文压缩策略快照。
///
/// # Returns
/// 返回值
/// A non-zero input-token budget when the resolved snapshot defines one.
/// 当解析后的快照定义了非零预算时返回输入 Token 预算。
fn task_context_budget_override_from_snapshot(
    snapshot: &ContextCompactionPolicySnapshot,
) -> Option<usize> {
    snapshot
        .context_budget
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
}

/// Reads the fixed agent prompt snapshot attached to a TaskSession record.
/// 读取附着在 TaskSession 记录上的固定代理提示词快照。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the in-memory task registry.
/// `task_runtime` 持有内存任务注册表。
/// `task_id` is the stable TaskSession identifier being launched.
/// `task_id` 是正在启动的稳定 TaskSession 标识。
///
/// # Returns
/// 返回值
/// The stored fixed prompt snapshot, a legitimate absence, or an explicit registry error.
/// 返回已保存的固定提示词快照、合法缺失状态或明确的注册表错误。
fn task_record_agent_prompt_snapshot(
    task_runtime: &TaskRuntime,
    task_id: SessionId,
) -> Result<Option<ExecutionAgentPromptSnapshot>, String> {
    // sessions is the authoritative task-record registry guard for this lookup.
    // sessions 是本次查询所使用的权威任务记录注册表守卫。
    let sessions = task_runtime.sessions.read().map_err(|_| {
        format!(
            "The task-session registry is locked and cannot read the fixed agent prompt snapshot for task {task_id}."
        )
    })?;
    Ok(sessions
        .get(&task_id)
        .and_then(|record| record.agent_prompt_snapshot.clone()))
}

/// Builds the structured prompt context for one child-agent execution.
/// 为一次子智能体执行构建结构化提示词上下文。
///
/// # Parameters
/// 参数
/// `invocation` is the prepared child-agent execution request.
/// `invocation` 是已准备好的子智能体执行请求。
/// `resolved_config` is the effective child-agent session configuration.
/// `resolved_config` 是生效后的子智能体会话配置。
///
/// # Returns
/// 返回值
/// A prompt context consumed by the shared runtime prompt composer.
/// 供共享运行时提示词组合器消费的提示词上下文。
fn build_task_subagent_prompt_context(
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Option<ExecutionSubagentPromptContext> {
    let mirror_subagent =
        resolved_config.effective_config.subagent_mode.as_deref() == Some(SUBAGENT_MODE_MIRROR);
    let identity = invocation
        .subagent
        .as_deref()
        .or(invocation.description.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("subagent")
        .to_string();
    let delegated_task = invocation
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Continue the delegated task.")
        .to_string();
    Some(ExecutionSubagentPromptContext {
        identity,
        delegated_task,
        delegation: Some(child_agent_delegation_prompt(mirror_subagent)),
        functional_system_prompt: invocation.controller_config.as_ref().and_then(|base| {
            task_functional_system_prompt(base, &invocation.invocation_config, resolved_config)
        }),
    })
}

/// Applies the hard child-agent tool policy to one execution request.
/// 对单个执行请求应用智能体硬禁用工具策略。
///
/// # Parameters
/// 参数
/// `request` is the child execution request being prepared.
/// `request` 是正在准备的智能体执行请求。
/// `preserve_parent_tool_shape` indicates clone or mirror runtime parity.
/// `preserve_parent_tool_shape` 表示克隆或镜像运行时需要保持父级工具形状。
fn apply_child_tool_policy(
    request: &mut ExecutionPreparationRequest,
    preserve_parent_tool_shape: bool,
) {
    let forbidden_tool_ids = vulcan_session_core::child_agent_forbidden_tool_ids();
    if preserve_parent_tool_shape {
        reject_registered_child_tools(request, forbidden_tool_ids);
    } else {
        hide_child_tools(request, forbidden_tool_ids);
    }
}

/// Builds the hidden child-agent delegation guidance used by the shared prompt composer.
/// 构建共享提示词组合器使用的隐式智能体委托说明。
///
/// # Parameters
/// 参数
/// `preserve_parent_tool_shape` indicates whether forbidden tools remain registered for runtime parity.
/// `preserve_parent_tool_shape` 表示禁用工具是否因运行时对齐而保留注册。
///
/// # Returns
/// 返回值
/// A model-facing child-agent instruction string.
/// 面向模型的智能体说明文本。
fn child_agent_delegation_prompt(preserve_parent_tool_shape: bool) -> String {
    let policy_sentence = if preserve_parent_tool_shape {
        "The following parent-session tools may remain registered for context parity but are unavailable in child-agent mode and will return a policy rejection if called:"
    } else {
        "The following parent-session tools are unavailable in child-agent mode and should not be called:"
    };
    format!(
        "Run as a child agent for the delegated task. Return the final result to the parent agent; do not assume you are the root conversation. {policy_sentence} {}.",
        child_forbidden_tool_prompt_list()
    )
}

/// Returns the stable prompt-facing forbidden child-agent tool list.
/// 返回面向提示词的稳定智能体禁用工具列表。
///
/// # Returns
/// 返回值
/// A comma-separated list of child-agent forbidden tool ids.
/// 逗号分隔的智能体禁用工具标识列表。
fn child_forbidden_tool_prompt_list() -> String {
    vulcan_session_core::child_agent_forbidden_tool_ids().join(", ")
}

/// Adds child-forbidden tool ids to the request hidden list without changing global policies.
/// 将智能体禁用工具加入请求隐藏列表，同时不改变全局工具策略。
///
/// # Parameters
/// 参数
/// `request` is the mutable child execution preparation request.
/// `request` 是可变的智能体执行准备请求。
/// `tool_ids` are public tool ids that should not be registered for the child turn.
/// `tool_ids` 是本次智能体回合不应注册的公开工具标识。
fn hide_child_tools(request: &mut ExecutionPreparationRequest, tool_ids: &[&str]) {
    for tool_id in tool_ids {
        if !request
            .tool_request_hidden_tool_ids
            .iter()
            .any(|existing| existing == tool_id)
        {
            request
                .tool_request_hidden_tool_ids
                .push((*tool_id).to_string());
        }
    }
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

/// Builds one child runtime-stream launch through the same preparation and bootstrap bridge used by main sessions.
/// 通过主会话使用的同一准备与启动桥构建一份子代理运行时流启动载荷。
pub(crate) async fn prepare_task_runtime_stream_launch(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch, String> {
    let (config, request) =
        build_task_runtime_preparation_request(context, invocation, resolved_config)?;
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
    .with_parent_snapshot(bootstrap.core_snapshot.clone());
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
    let context_compaction = &resolved_config.effective_config.context_compaction;
    if task_provider_request_limit_is_enabled(context_compaction)
        && task_provider_request_limit_reached(
            context_compaction,
            task_provider_requests_since_latest_compaction(
                &task_runtime,
                invocation.root_session_id,
                invocation.task_id,
            )?,
        )
        && let Some(compacted_snapshot) = run_task_session_context_compaction(
            Arc::clone(&task_runtime),
            context.clone(),
            invocation.clone(),
            resolved_config.clone(),
        )
        .await?
    {
        invocation.snapshot = compacted_snapshot;
    }

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
    let tool_calls_processed = report
        .controller_events
        .iter()
        .filter(|event| matches!(event.kind, vulcan_core::types::CoreEventKind::ToolFinished))
        .count();
    let usage_segments =
        subagent_usage_segments_from_report(invocation, &report, tool_calls_processed);
    let snapshot = match report.conversation_state.as_ref() {
        Some(conversation_state) => build_core_conversation_snapshot(conversation_state)?,
        None => invocation.snapshot.clone(),
    };
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
        tool_calls_processed,
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
    tool_calls_processed: usize,
) -> Vec<SessionTurnUsageSegment> {
    let provider_usage = report
        .cumulative_successful_provider_usage
        .as_ref()
        .or(report.last_successful_provider_usage.as_ref());
    let mut segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        report.provider_request_count,
        provider_usage,
    );
    segment.trigger = Some(invocation.public_task_id.clone());
    segment.tool_call_count = tool_calls_processed;
    if let Some(preparation) = report.preparation.as_ref() {
        segment.provider_id = Some(preparation.model_blueprint.provider_id.clone());
        segment.model_id = Some(preparation.model_blueprint.model_id.clone());
    }
    if segment.provider_call_count == 0 && segment.tool_call_count == 0 && segment.total_tokens == 0
    {
        return Vec::new();
    }
    vec![segment]
}
