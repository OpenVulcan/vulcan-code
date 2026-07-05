use super::*;

/// Runs one child task through the shared runtime preparation and provider-dispatch bridge.
/// 通过共享运行时准备与 Provider 分发桥执行一条子代理任务。
pub(crate) async fn run_task_with_shared_runtime_context(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    base_controller_config: CoreControllerConfig,
    parent_live_sender: ProviderLiveDeltaSender,
    request: TaskExecutionRequest,
) -> Result<TaskExecutionOutcome, String> {
    let mut request = request;
    let task_id = request.task_id;
    let trace_started_at = current_unix_millis_u64();
    task_runtime_trace(
        "shared_task_start",
        format!(
            "parent={} task={} public_task_id={} depth={}",
            request.current_session_id, request.task_id, request.public_task_id, request.depth
        ),
    );
    if provider_model_has_override(&request.invocation_config.provider_model) {
        validate_explicit_spawn_model_selector(
            &context.config,
            &request.invocation_config.provider_model,
        )?;
    }
    let mut resolved_config = task_runtime.resolve_task_session_config_for_execution(
        &base_controller_config,
        &request.invocation_config,
    )?;
    if !provider_model_has_override(&request.invocation_config.provider_model) {
        clear_unavailable_profile_model_override(&context.config, &mut resolved_config);
    }
    task_runtime_trace(
        "shared_task_config_resolved",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    let running_controller_config = build_task_controller_config(
        &base_controller_config,
        request.controller_config.clone(),
        &request.description,
        &request.subagent,
        &request.invocation_config,
        &resolved_config,
    );
    request.controller_config = Some(running_controller_config.clone());
    let started_payload = build_task_host_payload(
        &request,
        "running",
        0,
        0,
        None,
        "",
        &[],
        current_unix_millis_text(),
    );
    task_runtime.open_detached_task_event_stream_for_run(task_id)?;
    let (delegate_started_event, projection_barrier) =
        attach_runtime_event_projection_barrier(build_task_delegate_runtime_event(
            RuntimeEventKind::DelegateStarted,
            &request,
            "Running and waiting for the subagent to return a result.",
            started_payload,
        ));
    if parent_live_sender
        .send(Ok(ProviderStreamDelta::RuntimeEvent(
            delegate_started_event,
        )))
        .is_err()
    {
        cancel_runtime_event_projection_barrier(projection_barrier);
        task_runtime.close_detached_task_event_stream(task_id);
        let error = task_start_parent_stream_closed_error(&request.public_task_id);
        task_runtime_trace(
            "shared_task_delegate_started_parent_stream_closed",
            format!("task={} public_task_id={}", task_id, request.public_task_id),
        );
        return Err(error);
    }
    // Wait until the parent workbench projection sees the subagent-start event before the child run advances.
    // 等待父级工作台投影看到子代理启动事件后，再推进子任务执行。
    let has_projection_barrier = projection_barrier.is_some();
    wait_runtime_event_projection_barrier(projection_barrier).await;
    if !has_projection_barrier {
        tokio::task::yield_now().await;
    }
    task_runtime_trace(
        "shared_task_delegate_started",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );

    let background_task_runtime = Arc::clone(&task_runtime);
    let background_request = request.clone();
    let background_resolved_config = resolved_config.clone();
    let background_running_controller_config = running_controller_config.clone();
    let compaction_runtime_context = context.clone();
    let running_outcome = build_running_task_execution_outcome(
        task_id,
        running_controller_config.clone(),
        resolved_config.clone(),
        request.snapshot.clone(),
    );
    task_runtime.store_running_task_outcome_in_memory(
        request.current_session_id,
        &request,
        &running_outcome,
    )?;
    task_runtime_trace(
        "shared_task_running_stored_in_memory",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    let child_run_task = tokio::spawn(async move {
        let mut background_request = background_request;
        let launch = prepare_task_runtime_stream_launch_with_preflight_compaction(
            Arc::clone(&background_task_runtime),
            &context,
            &mut background_request,
            &background_resolved_config,
        )
        .await?;
        let task_controller_config = core_controller_config_from_child_launch(&launch);
        let mut terminal_report = None;
        let config = launch
            .task_execution_context
            .as_ref()
            .map(|context| context.config.clone())
            .unwrap_or_else(|| context.config.clone());
        let prompt_intervention = Some(build_child_agent_prompt_intervention(
            Arc::clone(&background_task_runtime),
            background_request.task_id,
        ));
        let mut child_event_stream = Box::pin(drive_runtime_stream_provider_loop(
            config,
            launch,
            prompt_intervention,
        ));
        while let Some(event) = child_event_stream.next().await {
            if let ExecutionStreamEvent::Finished(report) = &event {
                terminal_report = Some(report.clone());
            }
            background_task_runtime.publish_detached_task_event(
                background_request.task_id,
                DetachedTaskRuntimeEvent::ChildExecution(event),
            );
        }
        let report = terminal_report.ok_or_else(|| {
            format!(
                "Task `{}` finished without a terminal execution report.",
                background_request.task_id
            )
        })?;
        let should_compact_context = report
            .preparation
            .as_ref()
            .and_then(|preparation| preparation.context_budget.as_ref())
            .is_some_and(|context_budget| context_budget.should_compact());
        let outcome = task_execution_outcome_from_report(
            &background_request,
            task_controller_config,
            background_resolved_config,
            report,
        )?;
        Ok::<TaskExecutionCompletion, String>(TaskExecutionCompletion {
            outcome,
            should_compact_context,
        })
    });
    let child_run_abort_handle = child_run_task.abort_handle();
    let child_run_abort_guard = AbortChildRunOnDrop {
        handle: child_run_abort_handle.clone(),
    };
    let active_task_run_guard =
        task_runtime.install_active_task_run(task_id, child_run_abort_handle)?;
    task_runtime_trace(
        "shared_task_child_spawned",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    tokio::spawn(async move {
        let mut active_task_run_guard = Some(active_task_run_guard);
        let _child_run_abort_guard = child_run_abort_guard;
        let child_run_result = child_run_task.await;
        let resumed_queued_messages = match child_run_result {
            Ok(Ok(completion)) => {
                let outcome = completion.outcome;
                let store_result =
                    task_runtime.store_task_outcome(request.current_session_id, &request, &outcome);
                drop(active_task_run_guard.take());
                match store_result {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_dropped(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_finished_to_parent(
                            &task_runtime,
                            &request,
                            &outcome,
                            "The subagent task has completed.",
                        );
                        if completion.should_compact_context {
                            spawn_task_session_context_compaction(
                                Arc::clone(&task_runtime),
                                compaction_runtime_context,
                                request.clone(),
                                outcome.resolved_config.clone(),
                            );
                        }
                        resumed_queued_messages
                    }
                    Err(error) => {
                        let detail = format!(
                            "The subagent task completed, but its terminal state could not be recorded: {error}"
                        );
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
            Ok(Err(error)) if task_execution_error_is_stopped(error.as_str()) => {
                let stopped_snapshot = stopped_task_snapshot_from_store_or_request(
                    &task_runtime,
                    request.root_session_id,
                    task_id,
                    &request,
                )
                .unwrap_or_else(|| request.snapshot.clone());
                let outcome = build_stopped_task_execution_outcome(
                    task_id,
                    background_running_controller_config,
                    resolved_config,
                    stopped_snapshot,
                );
                let store_result =
                    task_runtime.store_task_outcome(request.current_session_id, &request, &outcome);
                drop(active_task_run_guard.take());
                match store_result {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_dropped(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_finished_to_parent(
                            &task_runtime,
                            &request,
                            &outcome,
                            outcome.final_output_text.as_str(),
                        );
                        resumed_queued_messages
                    }
                    Err(error) => {
                        let detail = format!(
                            "The stopped subagent task could not record its terminal state: {error}"
                        );
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
            Ok(Err(error)) => {
                let mark_result =
                    task_runtime.mark_task_failure(request.current_session_id, &request, &error);
                drop(active_task_run_guard.take());
                match mark_result {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_dropped(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_failure_to_parent(&task_runtime, &request, &error);
                        resumed_queued_messages
                    }
                    Err(record_error) => {
                        let detail = format!(
                            "The subagent task failed with `{error}`, and recording that failure also failed: {record_error}"
                        );
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
            Err(join_error) if join_error.is_cancelled() => {
                let stopped_snapshot = stopped_task_snapshot_from_store_or_request(
                    &task_runtime,
                    request.root_session_id,
                    task_id,
                    &request,
                )
                .unwrap_or_else(|| request.snapshot.clone());
                let outcome = build_stopped_task_execution_outcome(
                    task_id,
                    background_running_controller_config,
                    resolved_config,
                    stopped_snapshot,
                );
                let store_result =
                    task_runtime.store_task_outcome(request.current_session_id, &request, &outcome);
                drop(active_task_run_guard.take());
                match store_result {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_dropped(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_finished_to_parent(
                            &task_runtime,
                            &request,
                            &outcome,
                            outcome.final_output_text.as_str(),
                        );
                        resumed_queued_messages
                    }
                    Err(error) => {
                        let detail = format!(
                            "The cancelled subagent task could not record its stopped state: {error}"
                        );
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
            Err(join_error) => {
                let error = format!(
                    "Task `{}` failed: {}",
                    task_id,
                    format_task_join_error(&join_error)
                );
                let mark_result =
                    task_runtime.mark_task_failure(request.current_session_id, &request, &error);
                drop(active_task_run_guard.take());
                match mark_result {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_dropped(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_failure_to_parent(&task_runtime, &request, &error);
                        resumed_queued_messages
                    }
                    Err(record_error) => {
                        let detail = format!(
                            "The subagent task failed with `{error}`, and recording that failure also failed: {record_error}"
                        );
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
        };
        drop(active_task_run_guard.take());
        if !resumed_queued_messages && !task_runtime.task_has_active_descendant_runs(task_id) {
            task_runtime.close_detached_task_event_stream(task_id);
        }
        task_runtime_trace(
            "shared_task_background_completed",
            format!(
                "task={} elapsed_ms={}",
                task_id,
                current_unix_millis_u64().saturating_sub(trace_started_at)
            ),
        );
    });
    task_runtime_trace(
        "shared_task_spawn_return",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    Ok(running_outcome)
}

/// Resumes queued child-agent messages after the current active-run guard has been dropped.
/// 在当前活动运行守卫释放后续跑排队的子代理消息。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the active-run registry and inter-agent inbox.
/// `task_runtime` 持有活动运行注册表与代理间 inbox。
/// `current_session_id` is the direct parent that owns the child agent.
/// `current_session_id` 是拥有该子代理的直接父级。
/// `task_id` identifies the child agent whose turn just settled.
/// `task_id` 标识刚刚收敛的子代理。
///
/// # Returns
/// 返回值
/// `true` when a queued message started another child-agent turn.
/// 当排队消息已启动另一轮子代理 turn 时返回 `true`。
async fn resume_queued_messages_after_active_run_dropped(
    task_runtime: &Arc<TaskRuntime>,
    current_session_id: SessionId,
    task_id: SessionId,
) -> bool {
    match task_runtime
        .resume_agent_from_inbox_after_task_settled(current_session_id, task_id)
        .await
    {
        Ok(resumed) => resumed,
        Err(error) => {
            eprintln!(
                "[subagent-agent-control] failed to resume queued messages after task settled: {error}"
            );
            false
        }
    }
}

/// Loads the latest persisted TaskSession context as a core snapshot when it is available.
/// 在可用时将最新持久化 TaskSession 上下文加载为核心快照。
pub(super) fn current_task_snapshot_from_store(
    task_runtime: &TaskRuntime,
    root_session_id: SessionId,
    task_id: SessionId,
) -> Result<Option<CoreConversationSnapshot>, String> {
    let Some(store) = task_runtime
        .task_session_store_for_root(root_session_id)
        .map_err(|error| {
            format!("TaskSession store root lookup failed for `{root_session_id}`: {error}")
        })?
    else {
        return Ok(None);
    };
    let Some(context) = store
        .load_context(SessionNodeId::from(task_id))
        .map_err(|error| {
            format!("TaskSession context load failed for task `{task_id}`: {error}")
        })?
    else {
        return Ok(None);
    };
    let conversation_state: SessionConversationState = serde_json::from_value(
        context.conversation_state,
    )
    .map_err(|error| {
        format!(
            "TaskSession context for task `{task_id}` contains invalid conversation state: {error}"
        )
    })?;
    Ok(Some(build_core_conversation_snapshot(&conversation_state)))
}

/// Loads the latest stopped-task snapshot and logs corrupted persisted context before falling back.
/// 加载最新停止任务快照，并在持久化上下文损坏时记录诊断后再回退。
fn stopped_task_snapshot_from_store_or_request(
    task_runtime: &TaskRuntime,
    root_session_id: SessionId,
    task_id: SessionId,
    request: &TaskExecutionRequest,
) -> Option<CoreConversationSnapshot> {
    match current_task_snapshot_from_store(task_runtime, root_session_id, task_id) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!(
                "[task-session-store] using request snapshot for stopped task `{}` because persisted context could not be loaded: {error}",
                request.public_task_id
            );
            Some(request.snapshot.clone())
        }
    }
}

/// Spawns one non-blocking TaskSession context compaction job after a child task has completed.
/// 在子任务完成后派生一个非阻塞 TaskSession 上下文压缩任务。
pub(super) fn spawn_task_session_context_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: TaskExecutionRequest,
    resolved_config: TaskSessionResolvedConfig,
) {
    if !resolved_config
        .effective_config
        .context_compaction
        .auto_compaction_enabled
    {
        return;
    }

    tokio::spawn(async move {
        if let Err(error) =
            run_task_session_context_compaction(task_runtime, context, request, resolved_config)
                .await
        {
            eprintln!("[task-session-store] context compaction failed: {error}");
        }
    });
}

/// Runs one TaskSession context compaction pass using the shared prompt-compaction runtime and segmented store.
/// 使用共享提示词压缩运行时与分段存储执行一次 TaskSession 上下文压缩。
pub(crate) async fn run_task_session_context_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: TaskExecutionRequest,
    resolved_config: TaskSessionResolvedConfig,
) -> Result<Option<CoreConversationSnapshot>, String> {
    let Some(store) = task_runtime.task_session_store_for_root(request.root_session_id)? else {
        return Ok(None);
    };
    let task_node_id = SessionNodeId::from(request.task_id);
    let Some(context_document) = store
        .load_context(task_node_id)
        .map_err(|error| error.message)?
    else {
        return Ok(None);
    };
    let conversation_state: SessionConversationState =
        serde_json::from_value(context_document.conversation_state.clone())
            .map_err(|error| error.to_string())?;
    let messages = conversation_state_to_compaction_source_messages(&conversation_state);
    if messages.is_empty() {
        return Ok(None);
    }
    let Some(plan) = plan_context_compaction(CompactionPlanInput {
        session_label: &request.public_task_id,
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: context_document.compaction_anchor.as_deref(),
        source_until_message_id: None,
        policy: task_context_compaction_policy_from_snapshot(
            &resolved_config.effective_config.context_compaction,
        ),
    }) else {
        return Ok(None);
    };
    let event_sequence = store
        .load_events(task_node_id)
        .map(|events| events.len() as u64)
        .unwrap_or(0);
    let (provider_configuration, _) =
        build_task_runtime_preparation_request(&context, &request, &resolved_config);
    let model_label = provider_configuration.model.clone();
    let stream = stream_prompt_compaction_foundation_job(PromptCompactionFoundationJobRequest {
        session_id: request.task_id,
        session_title: request.display_title.clone(),
        workspace_path: context.request_template.workspace_path.clone(),
        plan,
        full_context_state: conversation_state.clone(),
        input_budget: None,
        observed_input_tokens: None,
        estimated_input_tokens: None,
        provider_configuration,
        model_label,
        reasoning_effort: context.request_template.reasoning_effort,
        ai_tool_configuration: context.request_template.ai_tool_configuration.clone(),
        foundation: Arc::clone(&context.foundation),
        luaskills_runtime: context.luaskills_runtime.clone(),
    });
    futures_util::pin_mut!(stream);
    while let Some(event) = stream.next().await {
        let PromptCompactionStreamEvent::Finished(result) = event else {
            continue;
        };
        let execution = result?;
        let application = apply_task_session_compaction_result(
            context_document,
            &execution.compaction_result,
            event_sequence,
        )
        .map_err(|error| error.message)?;
        let compacted_conversation_state: SessionConversationState =
            serde_json::from_value(application.context.conversation_state.clone())
                .map_err(|error| error.to_string())?;
        let compacted_snapshot = build_core_conversation_snapshot(&compacted_conversation_state);
        let checkpoint_id = application.checkpoint.checkpoint_id.clone();
        let timestamp_ms = application.checkpoint.timestamp_ms;
        let summary = application.checkpoint.summary.clone();
        store
            .save_compacted_context(application.context)
            .map_err(|error| error.message)?;
        store
            .save_compaction_checkpoint(application.checkpoint)
            .map_err(|error| error.message)?;
        store
            .append_event(SessionEventRecord::new(
                format!("task-context-compaction-{checkpoint_id}"),
                task_node_id,
                event_sequence,
                SessionEventKind::RuntimeMarker,
                timestamp_ms,
                json!({
                    "context_compaction_status": "completed",
                    "checkpoint_id": checkpoint_id,
                    "summary": summary,
                    "fallback_used": execution.fallback_used,
                    "provider_request_count": execution.provider_request_count,
                }),
            ))
            .map_err(|error| error.message)?;
        return Ok(Some(compacted_snapshot));
    }
    Ok(None)
}

/// Builds the shared compaction planning policy from a resolved TaskSession policy snapshot.
/// 根据已解析的 TaskSession 策略快照构建共享压缩规划策略。
pub(super) fn task_context_compaction_policy_from_snapshot(
    snapshot: &ContextCompactionPolicySnapshot,
) -> ContextCompactionPolicy {
    ContextCompactionPolicy {
        auto: snapshot.auto_compaction_enabled,
        ..ContextCompactionPolicy::default()
    }
}
