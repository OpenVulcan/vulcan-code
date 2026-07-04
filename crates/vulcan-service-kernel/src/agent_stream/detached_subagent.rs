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
    webui_trace(
        "detached_listener_delegate_started",
        format!(
            "parent={parent_session_id} task={task_id} public_task_id={} label={}",
            panel_state.public_task_id, panel_state.subagent_label
        ),
    );
    let should_start = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let should_start = guard.detached_subagent_task_listeners.insert(task_id);
        if should_start {
            bind_webui_subagent_parent_turn(&mut guard, parent_session_id, task_id);
        }
        should_start
    };
    if !should_start {
        webui_trace(
            "detached_listener_already_running",
            format!("parent={parent_session_id} task={task_id}"),
        );
        return Ok(());
    }
    webui_trace(
        "detached_listener_spawn",
        format!("parent={parent_session_id} task={task_id}"),
    );
    spawn_service_task(async move {
        let mut record_ids = SessionStreamingRecordIds::default();
        let mut stream = Box::pin(
            agent_runtime
                .task_runtime
                .subscribe_detached_task_events(task_id),
        );
        while let Some(event) = stream.next().await {
            webui_trace(
                "detached_listener_event",
                format!(
                    "task={task_id} kind={} {}",
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
                &mut record_ids,
                event,
            )
            .await
            {
                eprintln!("[webui-service] detached subagent event failed: {error}");
            }
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
    record_ids: &mut SessionStreamingRecordIds,
    event: DetachedTaskRuntimeEvent,
) -> Result<(), String> {
    match event {
        DetachedTaskRuntimeEvent::ChildExecution(event) => {
            handle_webui_detached_child_execution_event(
                app,
                state,
                controller_runtime,
                agent_runtime,
                record_ids,
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
                    "listener_task={task_id} parent={parent_session_id} kind={:?} is_finished={is_finished}",
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
            let consumed = match ingest_webui_subagent_runtime_event(
                app,
                state,
                agent_runtime,
                parent_session_id,
                &event,
            ) {
                Ok(consumed) => consumed,
                Err(error) => {
                    eprintln!("[webui-service] ingest subagent parent event failed: {error}");
                    false
                }
            };
            webui_trace(
                "detached_parent_event_ingested",
                format!(
                    "listener_task={task_id} parent={parent_session_id} kind={:?} consumed={consumed}",
                    event.kind
                ),
            );
            if !consumed {
                append_runtime_event_projection(app, state, parent_session_id, &event)?;
            }
            acknowledge_runtime_event_projection_barrier(&event);
            if is_finished {
                {
                    let mut guard = state
                        .lock()
                        .map_err(|error| format!("failed to lock service state: {error}"))?;
                    guard.detached_subagent_task_listeners.remove(&task_id);
                }
                if let Err(error) = enqueue_webui_session_mailbox_input_if_needed(
                    state,
                    parent_session_id,
                    &event,
                    parent_is_regular_session,
                    parent_is_stopped,
                ) {
                    eprintln!("[webui-service] enqueue session mailbox input failed: {error}");
                }
                {
                    let mut guard = state
                        .lock()
                        .map_err(|error| format!("failed to lock service state: {error}"))?;
                    clear_webui_subagent_completion_gate_ignored(&mut guard, task_id);
                    clear_webui_subagent_parent_turn_binding(&mut guard, task_id);
                }
                refresh_webui_subagent_navigation_round_visibility(state, parent_session_id)?;
                emit_webui_subagent_navigation_changed_delta(app, state)?;
                if parent_is_regular_session
                    && !parent_is_stopped
                    && let Some(completion_request) =
                        maybe_prepare_webui_parent_completion_after_subagent_finished(
                            state,
                            agent_runtime,
                            parent_session_id,
                        )?
                {
                    spawn_conversation_completion_hook_task(
                        app.clone(),
                        state.clone(),
                        controller_runtime.clone(),
                        agent_runtime.clone(),
                        completion_request,
                    );
                }
            }
            if webui_detached_parent_event_should_continue_pending_prompt(
                parent_is_regular_session,
                parent_is_stopped,
            ) {
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
                let state_for_persist = state.clone();
                let controller_runtime_for_persist = controller_runtime.clone();
                spawn_service_task(async move {
                    let snapshot = match current_workspace_snapshot_from_shared_state(
                        &state_for_persist,
                    ) {
                        Ok(Some(snapshot)) => snapshot,
                        Ok(None) => return,
                        Err(error) => {
                            eprintln!(
                                "[webui-service] detached subagent persist snapshot failed: {error}"
                            );
                            return;
                        }
                    };
                    spawn_workspace_session_persist_task(
                        state_for_persist,
                        controller_runtime_for_persist,
                        snapshot,
                        parent_session_id,
                        Some(String::from("detached_subagent_parent_event")),
                    );
                });
            }
            Ok(())
        }
    }
}

/// Prepares parent logical turn completion once its final child subagent reaches a terminal state.
/// 当最后一个子代理进入终态后准备父级逻辑回合闭合请求。
pub(crate) fn maybe_prepare_webui_parent_completion_after_subagent_finished(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    parent_session_id: SessionId,
) -> Result<
    Option<vulcan_agent_runtime::agent_runtime::conversation_hooks::ConversationCompletionRequest>,
    String,
> {
    let gate = webui_turn_completion_gate_from_state(state, agent_runtime, parent_session_id)?;
    webui_trace(
        "subagent_parent_completion_gate",
        format!(
            "parent={parent_session_id} pending_subagents={} has_active_hook_turn={}",
            gate.pending_subagent_count, gate.has_active_hook_turn
        ),
    );
    if gate.has_pending_work() || !gate.has_active_hook_turn {
        return Ok(None);
    }

    let completion_request = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let now = current_unix_millis_text();
        let project_id = workspace
            .sessions
            .iter()
            .find(|session| session.id == parent_session_id)
            .and_then(|session| session.project_id.clone());
        let runtime_state = workspace
            .session_runtime_state
            .entry(parent_session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        let assistant_content =
            latest_assistant_content_for_parent_completion(&runtime_state.conversation_state);
        let provider_request_count = runtime_state
            .last_safe_provider_request_count
            .unwrap_or_default();
        let conversation_state = runtime_state.conversation_state.clone();
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("failed to lock conversation hook runtime: {error}"))?;
        let (hook_turn_id, request) = hook_runtime.prepare_turn_completion_dispatch_with_status(
            parent_session_id,
            project_id,
            assistant_content.as_str(),
            provider_request_count,
            &conversation_state,
            ConversationTurnCompletionStatus::Completed,
            0,
        );
        runtime_state
            .context_injection_state
            .consume_turn(&hook_turn_id, &now);
        runtime_state.vmm_profile_sync.record_completed_turn();
        agent_runtime
            .vmm_hook_runtime
            .store_profile_sync_state(parent_session_id, runtime_state.vmm_profile_sync.clone());
        runtime_state.updated_at = now;
        request
    };
    Ok(Some(completion_request))
}

/// Returns the latest assistant text to close a parent completion hook payload.
/// 返回用于闭合父级完成 hook 载荷的最近助手文本。
fn latest_assistant_content_for_parent_completion(
    conversation_state: &vulcan_agent_runtime::conversation_protocol::SessionConversationState,
) -> String {
    for message in conversation_state.messages.iter().rev() {
        if let vulcan_agent_runtime::conversation_protocol::SessionConversationPayload::Assistant {
            text,
            ..
        } = &message.payload
        {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    String::from("The model completed the run but did not return visible text output.")
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
    let mailbox_accepted = enqueue_webui_subagent_mailbox_input(
        &mut guard,
        parent_session_id,
        panel_state.task_id,
        metadata,
    );
    if !mailbox_accepted {
        return Ok(false);
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

/// Builds Codex-compatible provider input from the parent session kernel mailbox.
/// 从父会话内核 mailbox 构建 Codex 兼容的 provider 输入。
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
pub(crate) fn webui_session_mailbox_input_from_kernel(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
    task_id: Option<&str>,
) -> Result<Option<String>, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(kernel) = guard.session_runtime_kernels.get(&parent_session_id) else {
        return Ok(None);
    };
    let matching_inputs = kernel
        .input_queue()
        .get_unconsumed_mailbox_input()
        .into_iter()
        .filter(|input| {
            input.kind == TurnInputKind::SubagentMessage
                && task_id.is_none_or(|task_id| {
                    input.payload.get("task_id").and_then(Value::as_str) == Some(task_id)
                })
        })
        .collect::<Vec<_>>();
    if matching_inputs.is_empty() {
        return Ok(None);
    };
    let messages = matching_inputs
        .into_iter()
        .map(|input| webui_agent_message_text_from_turn_input(parent_session_id, &input))
        .collect::<Vec<_>>();
    Ok(Some(messages.join("\n\n")))
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
    record_ids: &mut SessionStreamingRecordIds,
    event: ExecutionStreamEvent,
) -> Result<(), String> {
    match event {
        ExecutionStreamEvent::Prepared(_) => Ok(()),
        ExecutionStreamEvent::ProviderRequestCheckpoint { .. } => Ok(()),
        ExecutionStreamEvent::ProviderLoweringRepair { session_id, event } => {
            record_webui_provider_lowering_repair(controller_runtime, session_id, event)
        }
        ExecutionStreamEvent::RequestReady { session_id }
        | ExecutionStreamEvent::WireRequestReady { session_id } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::RequestStarted { session_id } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::ThinkingDelta { session_id, delta } => append_stream_delta_record(
            app,
            state,
            session_id,
            &mut record_ids.thinking_record_id,
            SessionMessageRecordKind::ThinkingOutput,
            "思考",
            &delta,
            true,
        ),
        ExecutionStreamEvent::ThinkingCompleted { session_id } => {
            collapse_streaming_thinking(app, state, session_id, record_ids)?;
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::OutputDelta { session_id, delta } => append_stream_delta_record(
            app,
            state,
            session_id,
            &mut record_ids.output_record_id,
            SessionMessageRecordKind::AssistantOutput,
            "输出",
            &delta,
            false,
        ),
        ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
            if is_provider_tool_boundary_runtime_event(&event) {
                record_ids.thinking_record_id = None;
                record_ids.output_record_id = None;
            }
            append_runtime_event_projection(app, state, session_id, &event)?;
            acknowledge_runtime_event_projection_barrier(&event);
            Ok(())
        }
        ExecutionStreamEvent::ConversationCheckpoint {
            session_id,
            conversation_state,
            last_successful_provider_usage,
            provider_request_count,
        } => apply_conversation_checkpoint(
            app,
            state,
            controller_runtime,
            session_id,
            conversation_state,
            last_successful_provider_usage,
            provider_request_count,
        ),
        ExecutionStreamEvent::Finished(report) => {
            let _ = (app, agent_runtime);
            apply_task_session_execution_report_to_memory_in_place(state, &report).map(|_| ())
        }
    }
}
