use super::*;

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
    request.execution_agent_id = resolved_config
        .effective_config
        .subagent_profile_id
        .clone()
        .or_else(|| context.request_template.execution_agent_id.clone());
    request.agent_system_prompt_override = invocation
        .controller_config
        .as_ref()
        .and_then(|config| normalize_optional_task_label(Some(&config.system_prompt)));
    request.task_session_store_root = context.request_template.task_session_store_root.clone();
    request.session_title = invocation.display_title.clone();
    request.prompt = invocation.prompt.clone();
    request.conversation_state = core_snapshot_to_session_conversation_state(&invocation.snapshot);
    request.tool_exposure_override = resolved_config.effective_config.tool_exposure.clone();
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

/// Applies one TaskSession provider/model snapshot to a mutable provider configuration clone.
/// 将一份 TaskSession 供应商与模型快照应用到可变供应商配置副本上。
pub(super) fn apply_task_provider_model_override(
    config: &mut ProviderConfigurationFile,
    provider_model: &ProviderModelSnapshot,
) {
    let provider_id = provider_model
        .provider_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            config
                .model
                .as_deref()
                .and_then(|model| model.split_once('/'))
                .map(|(provider, _)| provider.to_string())
        });
    let model_id = provider_model
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if let Some(model_id) = model_id {
        if let Some((provider, model)) = model_id.split_once('/') {
            config.model = Some(format!("{provider}/{model}"));
        } else if let Some(provider_id) = provider_id {
            config.model = Some(format!("{provider_id}/{model_id}"));
        }
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
    );
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
    })
}
