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
                    clear_webui_subagent_parent_turn_binding(&mut guard, task_id);
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
                refresh_webui_subagent_navigation_round_visibility(state, parent_session_id)?;
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

/// Enqueues one completed subagent result into the parent kernel mailbox when no wait_agent owns it.
/// 当没有 wait_agent 接管时，将单个已完成子代理结果加入父级内核 mailbox。
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
    if guard
        .parent_sessions_waiting_for_subagents
        .contains(&parent_session_id)
    {
        webui_trace(
            "session_mailbox_skip_wait_agent",
            format!(
                "parent={parent_session_id} task={} public_task_id={}",
                panel_state.task_id, panel_state.public_task_id
            ),
        );
        return Ok(false);
    }
    if let Some(completed_selectors) = guard
        .parent_session_completed_wait_agent_selectors
        .get(&parent_session_id)
    {
        let candidate_selectors = delegate_finished_mailbox_selector_values(
            parent_session_id,
            panel_state.task_id,
            &panel_state.public_task_id,
            metadata,
        );
        if completed_selectors.is_empty()
            || candidate_selectors
                .iter()
                .any(|selector| completed_selectors.contains(selector))
        {
            webui_trace(
                "session_mailbox_skip_completed_wait_agent",
                format!(
                    "parent={parent_session_id} task={} public_task_id={} wait_selectors={} candidate_selectors={}",
                    panel_state.task_id,
                    panel_state.public_task_id,
                    completed_selectors.len(),
                    candidate_selectors.len()
                ),
            );
            return Ok(false);
        }
    }
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

/// Builds one provider input from the parent session kernel mailbox.
/// 从父会话内核 mailbox 构建一条 provider 输入。
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
/// A structured mailbox prompt when matching mailbox input exists.
/// 当存在匹配 mailbox 输入时返回结构化 mailbox prompt。
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
    let events = matching_inputs
        .into_iter()
        .map(|input| {
            input
                .payload
                .get("mailbox_event")
                .cloned()
                .unwrap_or(input.payload)
        })
        .collect::<Vec<_>>();
    if events.len() == 1 {
        let event_text =
            serde_json::to_string_pretty(&events[0]).unwrap_or_else(|_| events[0].to_string());
        return Ok(Some(format!(
            "<subagent_mailbox_event>\n{event_text}\n</subagent_mailbox_event>"
        )));
    }
    let event_text =
        serde_json::to_string_pretty(&events).unwrap_or_else(|_| json!(events).to_string());
    Ok(Some(format!(
        "<subagent_mailbox_events>\n{event_text}\n</subagent_mailbox_events>"
    )))
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
