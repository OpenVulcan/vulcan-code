//! Detached subagent stream event handling.
//! 分离子代理流事件处理。

use super::*;

/// Starts one detached child-task listener using the shared task runtime stream.
/// 使用共享任务运行时流启动单个分离子任务监听器。
pub(crate) fn maybe_start_webui_detached_subagent_task_listener(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    if event.kind != RuntimeEventKind::DelegateStarted {
        return Ok(());
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(());
    };
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(());
    };
    let task_id = panel_state.task_id;
    let execution_id = subagent_feedback_execution_id_from_tool_metadata(metadata)?;
    if !agent_runtime
        .task_runtime
        .is_task_execution_current(task_id, &execution_id)?
    {
        webui_trace(
            "detached_listener_delegate_started_stale_execution",
            format!("parent={parent_session_id} task={task_id} execution_id={execution_id}"),
        );
        return Ok(());
    }
    webui_trace(
        "detached_listener_delegate_started",
        format!(
            "parent={parent_session_id} task={task_id} execution_id={execution_id} public_task_id={} label={}",
            panel_state.public_task_id, panel_state.subagent_label
        ),
    );
    let should_start = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let should_start =
            guard.detached_subagent_task_listeners.get(&task_id) != Some(&execution_id);
        if should_start {
            guard
                .detached_subagent_task_listeners
                .insert(task_id, execution_id.clone());
            bind_webui_subagent_parent_turn(&mut guard, parent_session_id, task_id, &execution_id);
        }
        should_start
    };
    if !should_start {
        webui_trace(
            "detached_listener_already_running",
            format!("parent={parent_session_id} task={task_id} execution_id={execution_id}"),
        );
        return Ok(());
    }
    // detached_stream is created before the background task so subscription failure can roll back the just-committed service registration synchronously.
    // detached_stream 在后台任务创建前完成订阅，使订阅失败可以同步回滚刚提交的服务侧注册。
    let detached_stream = match agent_runtime
        .task_runtime
        .subscribe_detached_task_events(task_id, &execution_id)
    {
        Ok(stream) => stream,
        Err(subscription_error) => {
            // rollback_result clears only this exact execution and therefore cannot delete a replacement listener.
            // rollback_result 仅清理当前精确执行，因此不会误删替代监听器。
            let rollback_result = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))
                .map(|mut guard| {
                    clear_webui_detached_listener_execution_if_current(
                        &mut guard,
                        task_id,
                        &execution_id,
                        true,
                    )
                });
            return match rollback_result {
                // cleared reports whether the failed subscription still owned and removed its service registration.
                // cleared 表示失败订阅是否仍拥有并移除了自身的服务侧注册。
                Ok(cleared) => {
                    webui_trace(
                        "detached_listener_subscription_rollback",
                        format!(
                            "parent={parent_session_id} task={task_id} execution_id={execution_id} cleared={cleared}"
                        ),
                    );
                    Err(subscription_error)
                }
                Err(rollback_error) => Err(format!(
                    "{subscription_error} The detached listener registration rollback also failed: {rollback_error}"
                )),
            };
        }
    };
    webui_trace(
        "detached_listener_spawn",
        format!("parent={parent_session_id} task={task_id} execution_id={execution_id}"),
    );
    spawn_service_task(async move {
        let mut record_ids = SessionStreamingRecordIds::default();
        // providerTelemetryGuard survives for the complete detached child-event subscription.
        // providerTelemetryGuard 在完整分离子事件订阅期间保持存活。
        let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new_with_persistence(
            state.clone(),
            Some(controller_runtime.clone()),
            Some(app.clone()),
        );
        let mut stream = Box::pin(detached_stream);
        while let Some(event) = stream.next().await {
            webui_trace(
                "detached_listener_event",
                format!(
                    "task={task_id} execution_id={execution_id} kind={} {}",
                    detached_task_event_label(&event),
                    detached_task_event_trace_detail(&event)
                ),
            );
            if let Err(error) = handle_webui_detached_subagent_task_event(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                task_id,
                &execution_id,
                &mut record_ids,
                &mut provider_telemetry_guard,
                event,
            )
            .await
            {
                eprintln!("[webui-service] detached subagent event failed: {error}");
            }
        }
        if let Some(event) = provider_telemetry_guard.take_cancelled_event()
            && let Err(error) = process_provider_request_telemetry_event(
                &state,
                &controller_runtime,
                Some(&app),
                &mut provider_telemetry_guard,
                event,
                "detached_provider_usage_cancelled",
            )
            .await
        {
            eprintln!(
                "[webui-service] detached provider cancellation usage update failed: {error}"
            );
        }
        // Stream exhaustion without a Finished event must not leave child transcript rows active forever.
        // 缺少 Finished 事件的流耗尽不得让子任务转录行永久保持活跃。
        if let Err(error) = seal_stream_content_records(&app, &state, task_id, &mut record_ids) {
            eprintln!("[webui-service] seal exhausted detached subagent records failed: {error}");
        }
    });
    Ok(())
}

/// Applies one detached subagent runtime event to the shared WebUI transcript projection.
/// 将单条分离子代理运行时事件应用到共享 WebUI 转录投影。
pub(crate) async fn handle_webui_detached_subagent_task_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    task_id: SessionId,
    execution_id: &str,
    record_ids: &mut SessionStreamingRecordIds,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    event: DetachedTaskRuntimeEvent,
) -> Result<(), String> {
    let listener_is_current = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .detached_subagent_task_listeners
        .get(&task_id)
        .is_some_and(|current_execution_id| current_execution_id == execution_id);
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(event) => {
            if !listener_is_current {
                webui_trace(
                    "detached_child_event_stale_execution",
                    format!("task={task_id} execution_id={execution_id}"),
                );
                return Ok(());
            }
            handle_webui_detached_child_execution_event(
                app,
                state,
                controller_runtime,
                agent_runtime,
                execution_id,
                record_ids,
                provider_telemetry_guard,
                event,
            )
            .await
        }
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id,
            event,
        } => {
            let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
            webui_trace(
                "detached_parent_event_received",
                format!(
                    "listener_task={task_id} execution_id={execution_id} parent={parent_session_id} kind={:?} is_finished={is_finished} listener_is_current={listener_is_current}",
                    event.kind
                ),
            );
            let parent_state_check_started_at = current_unix_millis_value();
            let (parent_is_regular_session, parent_is_stopped) = {
                let guard = state
                    .lock()
                    .map_err(|error| format!("failed to lock service state: {error}"))?;
                match guard.workspace_snapshot.as_ref() {
                    Some(workspace_snapshot) => (
                        workspace_snapshot_contains_regular_session(
                            workspace_snapshot,
                            parent_session_id,
                        ),
                        workspace_snapshot
                            .session_run_phases
                            .get(&parent_session_id)
                            == Some(&SessionRunPhase::Stopped),
                    ),
                    None => (false, false),
                }
            };
            webui_trace_if_slow(
                "detached_parent_event_state_check_slow",
                parent_state_check_started_at,
                format!(
                    "listener_task={task_id} parent={parent_session_id} kind={:?}",
                    event.kind
                ),
            );
            let consumed = if listener_is_current {
                match ingest_webui_subagent_runtime_event_for_listener_execution(
                    app,
                    state,
                    agent_runtime,
                    parent_session_id,
                    &event,
                    Some(execution_id),
                ) {
                    Ok(consumed) => consumed,
                    Err(error) => {
                        eprintln!("[webui-service] ingest subagent parent event failed: {error}");
                        false
                    }
                }
            } else {
                true
            };
            webui_trace(
                "detached_parent_event_ingested",
                format!(
                    "listener_task={task_id} parent={parent_session_id} kind={:?} consumed={consumed}",
                    event.kind
                ),
            );
            if !consumed {
                append_runtime_event_projection_and_release_barrier(
                    app,
                    state,
                    parent_session_id,
                    &event,
                )?;
            } else {
                release_runtime_event_projection_barrier_after_attempt(&event)?;
            }
            if listener_is_current && is_finished && !parent_is_regular_session {
                // A nested parent is a TaskSession, so its new inclusive child contribution must reach the task checkpoint before the parent resumes.
                // 嵌套父级属于 TaskSession，因此新的包容性子贡献必须在父级恢复前写入任务检查点。
                let parent_provider_call_ledger = state
                    .lock()
                    .map_err(|error| format!("failed to lock service state: {error}"))?
                    .workspace_snapshot
                    .as_ref()
                    .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id))
                    .map(|runtime_state| runtime_state.provider_call_ledger.clone())
                    .ok_or_else(|| {
                        format!(
                            "Nested parent TaskSession `{parent_session_id}` has no runtime ledger after child completion."
                        )
                    })?;
                agent_runtime
                    .task_runtime
                    .persist_task_provider_call_ledger(
                        parent_session_id,
                        &parent_provider_call_ledger,
                    )?;
            }
            let mut mailbox_delivery_error: Option<String> = None;
            if is_finished {
                if let Err(error) = enqueue_webui_session_mailbox_input_if_needed(
                    state,
                    parent_session_id,
                    &event,
                    parent_is_regular_session,
                    parent_is_stopped,
                ) {
                    eprintln!("[webui-service] enqueue session mailbox input failed: {error}");
                    webui_trace(
                        "detached_parent_event_mailbox_delivery_rejected",
                        format!("parent={parent_session_id} task={task_id} error={error}"),
                    );
                    mailbox_delivery_error = Some(error);
                }
                let cleaned_current_listener = {
                    let mut guard = state
                        .lock()
                        .map_err(|error| format!("failed to lock service state: {error}"))?;
                    let cleaned = clear_webui_detached_listener_execution_if_current(
                        &mut guard,
                        task_id,
                        execution_id,
                        mailbox_delivery_error.is_none(),
                    );
                    if cleaned && mailbox_delivery_error.is_some() {
                        webui_trace(
                            "subagent_parent_turn_binding_preserved_after_mailbox_error",
                            format!("parent={parent_session_id} task={task_id}"),
                        );
                    }
                    if !cleaned && mailbox_delivery_error.is_none() {
                        clear_webui_subagent_parent_turn_binding(&mut guard, task_id, execution_id);
                        clear_webui_subagent_completion_gate_ignored(
                            &mut guard,
                            task_id,
                            execution_id,
                        );
                    }
                    cleaned
                };
                if cleaned_current_listener {
                    refresh_webui_subagent_navigation_round_visibility(state, parent_session_id)?;
                    emit_webui_subagent_navigation_changed_delta_for_parent(
                        app,
                        state,
                        parent_session_id,
                    )?;
                }
            }
            if webui_detached_parent_event_should_continue_pending_prompt(
                parent_is_regular_session,
                parent_is_stopped,
            ) {
                if let Some(error) = mailbox_delivery_error.as_ref() {
                    webui_trace(
                        "detached_parent_event_pending_skip_mailbox_error",
                        format!("parent={parent_session_id} task={task_id} error={error}"),
                    );
                } else {
                    let started_mailbox_turn = maybe_start_webui_mailbox_turn_if_ready(
                        app,
                        state,
                        controller_runtime,
                        agent_runtime,
                        parent_session_id,
                    )
                    .await?;
                    let started = if started_mailbox_turn {
                        true
                    } else {
                        maybe_prepare_next_webui_pending_prompt(
                            app,
                            state,
                            controller_runtime,
                            agent_runtime,
                            parent_session_id,
                            false,
                        )
                        .await?
                    };
                    webui_trace(
                        "detached_parent_event_pending_result",
                        format!(
                            "parent={parent_session_id} task={task_id} started_next_prompt={started}"
                        ),
                    );
                }
            } else if parent_is_regular_session && parent_is_stopped {
                webui_trace(
                    "detached_parent_event_pending_skip_stopped",
                    format!("parent={parent_session_id} task={task_id}"),
                );
            }
            if webui_detached_parent_event_should_persist_terminal_projection(
                is_finished,
                parent_is_regular_session,
                parent_is_stopped,
            ) {
                webui_trace(
                    "detached_parent_event_persist_spawn",
                    format!("parent={parent_session_id} task={task_id}"),
                );
                spawn_workspace_session_persist_task(
                    state.clone(),
                    controller_runtime.clone(),
                    parent_session_id,
                    Some(String::from("detached_subagent_parent_event")),
                );
            }
            if let Some(error) = mailbox_delivery_error {
                return Err(error);
            }
            Ok(())
        }
    }
}

/// Clears detached-listener state only when the caller still owns the exact execution generation.
/// 仅当调用方仍拥有精确执行代际时清理分离监听器状态。
///
/// # Parameters
/// 参数
/// `state` owns listener identities and parent-turn coordination under one lock.
/// `state` 在同一锁下持有监听器身份与父回合协调状态。
/// `task_id` identifies the stable child task.
/// `task_id` 标识稳定子任务。
/// `execution_id` identifies the listener execution attempting cleanup.
/// `execution_id` 标识尝试清理的监听器执行。
/// `clear_parent_binding` controls whether successful cleanup may release the parent binding.
/// `clear_parent_binding` 控制成功清理时是否可以释放父级绑定。
///
/// # Returns
/// 返回值
/// Whether the exact execution still owned and cleared the listener row.
/// 精确执行是否仍拥有并清理了监听器行。
pub(crate) fn clear_webui_detached_listener_execution_if_current(
    state: &mut ServiceBridgeState,
    task_id: SessionId,
    execution_id: &str,
    clear_parent_binding: bool,
) -> bool {
    let owns_listener = state
        .detached_subagent_task_listeners
        .get(&task_id)
        .is_some_and(|current_execution_id| current_execution_id == execution_id);
    if !owns_listener {
        return false;
    }
    state.detached_subagent_task_listeners.remove(&task_id);
    clear_webui_subagent_completion_gate_ignored(state, task_id, execution_id);
    if clear_parent_binding {
        clear_webui_subagent_parent_turn_binding(state, task_id, execution_id);
    }
    true
}

/// Enqueues one completed subagent result into the parent kernel mailbox for provider delivery.
/// 将单个已完成子代理结果加入父级内核 mailbox，供后续 Provider 输入投递。
pub(crate) fn enqueue_webui_session_mailbox_input_if_needed(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
    parent_is_regular_session: bool,
    parent_is_stopped: bool,
) -> Result<bool, String> {
    if event.kind != RuntimeEventKind::DelegateFinished
        || !parent_is_regular_session
        || parent_is_stopped
    {
        return Ok(false);
    }
    let Some(metadata) = event.metadata.as_ref() else {
        return Ok(false);
    };
    let Some(panel_state) = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
    else {
        return Ok(false);
    };
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let mailbox_outcome = enqueue_webui_subagent_mailbox_input(
        &mut guard,
        parent_session_id,
        panel_state.task_id,
        metadata,
    );
    match mailbox_outcome {
        SubagentMailboxEnqueueOutcome::Queued => {}
        SubagentMailboxEnqueueOutcome::AlreadyPresentOrConsumed { input_id } => {
            webui_trace(
                "session_mailbox_input_idempotent_skip",
                format!(
                    "parent={parent_session_id} task={} input_id={input_id}",
                    panel_state.task_id
                ),
            );
            return Ok(false);
        }
        SubagentMailboxEnqueueOutcome::Rejected(rejection) => {
            return Err(format!(
                "subagent mailbox input rejected: parent={parent_session_id} task={} reason={}",
                panel_state.task_id,
                rejection.diagnostic()
            ));
        }
    }
    webui_trace(
        "session_mailbox_input_enqueued",
        format!(
            "parent={parent_session_id} task={} public_task_id={} result_chars={}",
            panel_state.task_id,
            panel_state.public_task_id,
            panel_state.result_text.chars().count()
        ),
    );
    Ok(true)
}

/// Collects typed provider-visible agent messages from the parent session kernel mailbox.
/// 从父会话内核 mailbox 收集 Provider 可见类型化智能体消息。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns runtime kernels.
/// `state` 是持有运行时内核的共享服务状态。
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `task_id` optionally selects one child task session.
/// `task_id` 可选指定一个子任务会话。
///
/// # Returns
/// 返回值
/// Typed inter-agent messages matching the requested mailbox scope.
/// 返回匹配指定 mailbox 范围的类型化代理间消息。
pub(crate) fn webui_session_mailbox_agent_messages_from_kernel(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
    task_id: Option<&str>,
) -> Result<Vec<ExecutionProviderVisibleAgentMessage>, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(kernel) = guard.session_runtime_kernels.get(&parent_session_id) else {
        return Ok(Vec::new());
    };
    let mut messages = Vec::new();
    for input in kernel.input_queue().get_unconsumed_mailbox_input() {
        if input.kind != TurnInputKind::SubagentMessage
            || task_id.is_some_and(|task_id| {
                input.payload.get("task_id").and_then(Value::as_str) != Some(task_id)
            })
        {
            continue;
        }
        messages.push(webui_provider_visible_agent_message_from_turn_input(
            parent_session_id,
            &input,
        )?);
    }
    Ok(messages)
}

/// Builds Codex-compatible provider input text from the parent session kernel mailbox.
/// 从父会话内核 mailbox 构建 Codex 兼容的 provider 输入文本。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns runtime kernels.
/// `state` 是持有运行时内核的共享服务状态。
/// `parent_session_id` identifies the parent WebUI session.
/// `parent_session_id` 标识父级 WebUI 会话。
/// `task_id` optionally selects one child task session.
/// `task_id` 可选指定一个子任务会话。
///
/// # Returns
/// 返回值
/// Inter-agent message text when matching mailbox input exists.
/// 当存在匹配 mailbox 输入时返回代理间消息文本。
#[cfg(test)]
pub(crate) fn webui_session_mailbox_input_from_kernel(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
    task_id: Option<&str>,
) -> Result<Option<String>, String> {
    let messages =
        webui_session_mailbox_agent_messages_from_kernel(state, parent_session_id, task_id)?;
    if messages.is_empty() {
        Ok(None)
    } else {
        Ok(Some(
            messages
                .iter()
                .map(|message| {
                    vulcan_core::types::ConversationPayload::render_agent_message_text(
                        &message.message_type,
                        &message.task_name,
                        &message.sender,
                        &message.payload,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        ))
    }
}

/// Returns whether a detached subagent parent event may dispatch the next queued prompt.
/// 返回分离子代理父级事件是否可以调度下一条排队提示词。
pub(crate) fn webui_detached_parent_event_should_continue_pending_prompt(
    parent_is_regular_session: bool,
    parent_is_stopped: bool,
) -> bool {
    parent_is_regular_session && !parent_is_stopped
}

/// Returns whether one detached terminal event may refresh and persist the parent projection.
/// 返回一条分离终态事件是否可以刷新并持久化父级投影。
pub(crate) fn webui_detached_parent_event_should_persist_terminal_projection(
    is_finished: bool,
    parent_is_regular_session: bool,
    parent_is_stopped: bool,
) -> bool {
    is_finished
        && webui_detached_parent_event_should_continue_pending_prompt(
            parent_is_regular_session,
            parent_is_stopped,
        )
}

/// Applies one child execution stream event using the same projection functions as root WebUI turns.
/// 使用与根 WebUI 回合相同的投影函数应用单条子执行流事件。
pub(crate) async fn handle_webui_detached_child_execution_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    execution_id: &str,
    record_ids: &mut SessionStreamingRecordIds,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    event: ExecutionStreamEvent,
) -> Result<(), String> {
    match event {
        ExecutionStreamEvent::Prepared(_) => Ok(()),
        ExecutionStreamEvent::ProviderRequestCheckpoint { .. } => Ok(()),
        ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
            process_provider_request_telemetry_event(
                state,
                controller_runtime,
                Some(app),
                provider_telemetry_guard,
                event,
                "detached_provider_usage_telemetry",
            )
            .await
        }
        ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
            process_provider_request_durability_boundary(
                state,
                controller_runtime,
                Some(app),
                provider_telemetry_guard,
                event,
                completion,
                "detached_provider_usage_durability_boundary",
            )
            .await
        }
        ExecutionStreamEvent::ProviderLoweringRepair { session_id, event } => {
            let session_directory = require_segmented_session_directory_from_state(
                state,
                &controller_runtime.data_root,
                session_id,
            )?;
            record_webui_provider_lowering_repair(&session_directory, session_id, event)
        }
        ExecutionStreamEvent::RequestReady { session_id }
        | ExecutionStreamEvent::WireRequestReady { session_id } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::RequestStarted { session_id } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::ThinkingDelta { session_id, delta } => append_stream_content_delta(
            app,
            state,
            session_id,
            record_ids,
            StreamContentKind::Thinking,
            &delta,
        ),
        ExecutionStreamEvent::TransientReasoningStatusChanged { session_id, status } => {
            emit_transient_reasoning_status(app, state, session_id, status)
        }
        ExecutionStreamEvent::ThinkingCompleted { session_id } => {
            complete_streaming_thinking_phase(app, state, session_id, record_ids)
        }
        ExecutionStreamEvent::OutputDelta { session_id, delta } => append_stream_content_delta(
            app,
            state,
            session_id,
            record_ids,
            StreamContentKind::Output,
            &delta,
        ),
        ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
            if let Err(error) =
                dispatch_runtime_event_hook(agent_runtime, session_id, &event, execution_id)
            {
                eprintln!("[webui-service] detached runtime event Hook dispatch failed: {error}");
            }
            if is_provider_tool_boundary_runtime_event(&event) {
                seal_stream_content_records(app, state, session_id, record_ids)?;
            }
            append_runtime_event_projection_and_release_barrier(app, state, session_id, &event)?;
            Ok(())
        }
        ExecutionStreamEvent::ConversationCheckpoint {
            session_id,
            conversation_state,
            ..
        } => apply_conversation_checkpoint(
            app,
            state,
            controller_runtime,
            session_id,
            conversation_state,
        ),
        ExecutionStreamEvent::ContextCompactionRequired {
            session_id,
            conversation_state,
            ..
        } => apply_conversation_checkpoint(
            app,
            state,
            controller_runtime,
            session_id,
            conversation_state,
        ),
        ExecutionStreamEvent::ProviderRouteSwitchRequired {
            session_id,
            conversation_state,
            ..
        } => apply_conversation_checkpoint(
            app,
            state,
            controller_runtime,
            session_id,
            conversation_state,
        ),
        ExecutionStreamEvent::Finished(report) => {
            let _ = agent_runtime;
            let session_id = report.session_id;
            seal_stream_content_records(app, state, session_id, record_ids)?;
            let before_visible_messages =
                visible_workbench_messages_for_conversation(state, session_id)?;
            apply_task_session_execution_report_to_memory_in_place(state, &report)?;
            emit_detached_child_terminal_messages_patch(
                app,
                state,
                session_id,
                before_visible_messages.as_slice(),
            )
        }
    }
}

/// Returns the currently retained WebUI messages for one conversation surface.
/// 返回当前保留的单个会话界面消息集合。
fn visible_workbench_messages_for_conversation(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Vec<ConversationMessage>, String> {
    let conversation_id = session_id.to_string();
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard
        .workbench
        .messages
        .iter()
        .filter(|message| message.conversation_id == conversation_id)
        .cloned()
        .collect())
}

/// Emits the refreshed child transcript after a detached child reaches a terminal state.
/// 在分离子任务进入终态后发送刷新的子转录补丁。
fn emit_detached_child_terminal_messages_patch(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    before_visible_messages: &[ConversationMessage],
) -> Result<(), String> {
    let conversation_id = session_id.to_string();
    let message_window_event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
            return Ok(());
        }
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let messages = surface_messages_for_session_without_subagent_navigation_refresh(
            &guard, workspace, session_id,
        );
        let projected_messages =
            conversation_messages_from_session(&conversation_id, messages.as_slice(), false);
        let Some(message_patch) = build_conversation_messages_patched_delta(
            &conversation_id,
            before_visible_messages,
            projected_messages.as_slice(),
        ) else {
            return Ok(());
        };
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        replace_conversation_messages_in_workbench_window(
            &mut guard.workbench.messages,
            &conversation_id,
            projected_messages,
        );
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
        }
    };
    emit_service_event_with_trace(
        app,
        "detached_child_terminal_messages_patch_emit",
        ServiceEvent::WorkbenchDelta(Box::new(message_window_event)),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Minimal shell host used only by detached-subagent projection tests.
    /// 仅供分离智能体投影测试使用的最小宿主壳。
    struct DetachedSubagentProjectionTestShellHost;

    impl ServiceShellHost for DetachedSubagentProjectionTestShellHost {
        /// Accepts path-open requests without side effects.
        /// 接受路径打开请求但不产生副作用。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Ok(())
        }

        /// Accepts URL-open requests without side effects.
        /// 接受 URL 打开请求但不产生副作用。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Ok(())
        }
    }

    /// Captures service events emitted during one projection test.
    /// 捕获单个投影测试期间发出的服务事件。
    struct DetachedSubagentProjectionEventSink {
        /// Shared buffer containing emitted events in order.
        /// 按顺序保存已发事件的共享缓冲区。
        events: Arc<Mutex<Vec<ServiceEvent>>>,
    }

    impl vulcan_service_api::ServiceEventSink for DetachedSubagentProjectionEventSink {
        /// Pushes one event into the shared capture buffer.
        /// 将单个事件写入共享捕获缓冲区。
        fn emit_service_event(&self, event: ServiceEvent) {
            self.events.lock().expect("event sink lock").push(event);
        }
    }

    /// Verifies terminal patches reuse cached child detail when workspace display rows are absent.
    /// 验证终态补丁在工作区显示行缺失时复用缓存的子会话详情。
    #[test]
    fn detached_terminal_patch_uses_cached_surface_detail_when_workspace_rows_are_empty() {
        let parent_session_id = SessionId::new_v4();
        let task_session_id = SessionId::new_v4();
        let conversation_id = task_session_id.to_string();
        let cached_body = "cached child transcript";
        let mut conversation_state = SessionConversationState::default();
        conversation_state.records.push(SessionConversationRecord {
            id: String::from("cached-record"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("智能体回复"),
            body: String::from(cached_body),
            metadata: None,
            created_at: String::from("100"),
        });
        let cached_messages = conversation_state_to_session_messages(&conversation_state);
        let mut workbench = fallback_workbench_snapshot(&[]);
        workbench.selected_conversation_id = Some(parent_session_id.to_string());
        workbench.subagent_navigation = Some(SubagentNavigationSnapshot {
            parent_conversation_id: parent_session_id.to_string(),
            active_conversation_id: conversation_id.clone(),
            active_task_id: Some(conversation_id.clone()),
            tasks: Vec::new(),
        });
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                parent_session_id,
                Some(String::from("project-a")),
                "父会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: parent_session_id,
            hydrated_session_ids: BTreeSet::from([parent_session_id]),
        };
        let mut service_state = ServiceBridgeState {
            shell_host: Arc::new(DetachedSubagentProjectionTestShellHost),
            startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
            workbench,
            workbench_generation: 0,
            workspace_snapshot: Some(workspace_snapshot),
            hydrated_conversation_windows: BTreeMap::new(),
            agent_runtime: None,
            codebase_memory_controller: Default::default(),
            managed_memory_mesh_runtime: None,
            service_runtime: None,
            webui_service_discovery: None,
            controller_runtime: None,
            knowledge_runtime: None,
            luaskills_runtime: None,
            cached_luaskills_catalog: None,
            mcp_runtime: None,
            cached_mcp_catalog: None,
            provider_manifest_sync: None,
            live_context_budgets: BTreeMap::new(),
            session_goal_summaries: BTreeMap::new(),
            session_goal_dispatch_versions: BTreeMap::new(),
            budget_limit_reported_goal_ids: BTreeSet::new(),
            webui_preloaded_unclosed_subagent_roots: BTreeSet::new(),
            subagent_surface_detail_cache: BTreeMap::new(),
            session_subagent_navigation_rounds: BTreeSet::new(),
            active_subagent_panel: Some(SelectedSubagentTaskPanel {
                parent_session_id,
                task_id: task_session_id,
            }),
            detached_subagent_task_listeners: BTreeMap::new(),
            subagent_parent_turn_bindings: BTreeMap::new(),
            subagent_completion_gate_ignored_tasks: BTreeSet::new(),
            parent_sessions_waiting_for_subagents: BTreeSet::new(),
            active_stream_tasks: BTreeMap::new(),
            session_terminal_persistence_owners: BTreeMap::new(),
            session_runtime_kernels: BTreeMap::new(),
            session_turn_admission_reservations: BTreeMap::new(),
            project_admission_reservations: BTreeMap::new(),
            session_persist_versions: BTreeMap::new(),
            vmm_controller_registry: Default::default(),
            session_transcript_persist_versions: BTreeMap::new(),
            session_addon_persist_versions: BTreeMap::new(),
        };
        service_state.subagent_surface_detail_cache.insert(
            task_session_id,
            WebuiSubagentSurfaceDetailCacheEntry {
                task_state: SubagentTaskPanelState {
                    task_id: task_session_id,
                    public_task_id: String::from("cached-task"),
                    parent_session_id,
                    subagent_label: String::from("cached-agent"),
                    description: None,
                    run_state: SubagentTaskRunState::Completed,
                    result_text: String::new(),
                    conversation_state: Some(conversation_state),
                },
                messages: cached_messages,
                last_access_ms: 0,
            },
        );
        let state = Arc::new(Mutex::new(service_state));
        let captured_events = Arc::new(Mutex::new(Vec::new()));
        let app = ServiceEventEmitter::new(vec![Arc::new(DetachedSubagentProjectionEventSink {
            events: captured_events.clone(),
        })]);

        emit_detached_child_terminal_messages_patch(&app, &state, task_session_id, &[])
            .expect("terminal patch should emit from cached detail");

        let events = captured_events.lock().expect("captured events lock");
        let patched_messages = events
            .iter()
            .find_map(|event| {
                if let ServiceEvent::WorkbenchDelta(envelope) = event
                    && let WorkbenchDelta::ConversationMessagesPatched(delta) = &envelope.delta
                {
                    return Some(delta.upsert_messages.clone());
                }
                None
            })
            .expect("cached child transcript should produce a message patch");
        assert_eq!(patched_messages.len(), 1);
        assert_eq!(patched_messages[0].body, cached_body);
    }
}
