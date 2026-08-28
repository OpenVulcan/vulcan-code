//! Detached subagent stream event handling.
//! 分离子代理流事件处理。

use super::*;
use std::sync::Arc;
use vulcan_agent_runtime::agent_runtime::provider_v1_bridge::ProviderV1LifecycleFacts;
use vulcan_session_core::v1::document::{SubagentReturnBuildRequest, SubagentReturnReceipt};
use vulcan_session_core::v1::event::{AssistantSemanticStatus, EventBody};
use vulcan_session_core::v1::ids::SubagentTaskId;

use crate::session_runtime_registry_v1::SessionHandle;
use crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1;

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
    // executionFacts are retained by TaskRuntime before DelegateStarted and are the only child
    // lifecycle input accepted here.
    // executionFacts 由 TaskRuntime 在 DelegateStarted 前保留，是此处唯一接纳的子会话生命周期输入。
    let execution_facts = agent_runtime
        .task_runtime
        .active_task_v1_execution_facts(task_id, &execution_id)?;
    if execution_facts.parent_session_id != parent_session_id
        || execution_facts.task_id != task_id
        || execution_facts.execution_id != execution_id
    {
        return Err(format!(
            "Detached task V1 execution facts do not match the listener identity: parent={parent_session_id} task={task_id} execution_id={execution_id}."
        ));
    }
    // sessionRegistry is cloned outside the lifecycle call so no service-state lock spans disk I/O.
    // sessionRegistry 在生命周期调用前完成克隆，确保服务状态锁不会跨越磁盘 I/O。
    let session_registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .session_runtime_registry
        .clone();
    // childHandle owns the deterministic root/TASK child tree before event subscription begins.
    // childHandle 在事件订阅开始前持有确定性的 root/TASK 子目录树。
    let child_handle = create_or_load_embedded_task_session_v1(&session_registry, execution_facts)
        .map_err(|error| error.to_string())?;
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
    let project_resource_catalog_registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| Arc::clone(&guard.project_resource_catalog_registry))?;
    spawn_service_task(async move {
        // providerV1Sink retains the exact child handle for the complete detached execution.
        // providerV1Sink 在完整分离执行期间保留精确子句柄。
        let mut provider_v1_sink =
            ProviderV1StreamSink::new(Arc::clone(&child_handle), project_resource_catalog_registry);
        // providerTelemetryGuard survives for the complete detached child-event subscription.
        // providerTelemetryGuard 在完整分离子事件订阅期间保持存活。
        let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new_with_persistence(
            state.clone(),
            Some(controller_runtime.clone()),
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
                child_handle.as_ref(),
                &mut provider_telemetry_guard,
                &mut provider_v1_sink,
                event,
            )
            .await
            {
                eprintln!("[webui-service] detached subagent event failed: {error}");
            }
        }
        if let Some(event) = provider_telemetry_guard.take_cancelled_event() {
            provider_telemetry_guard.forward(event);
        }
    });
    Ok(())
}

/// Atomically returns one durable child final assistant and its complete statistics to the parent.
/// 将一份可靠子会话最终 assistant 正文及其完整统计原子回流给父会话。
fn append_exact_v1_subagent_return(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
    child_handle: &SessionHandle,
    provider_v1_sink: &ProviderV1StreamSink,
) -> Result<SubagentReturnReceipt, String> {
    if child_handle.session_id() != task_id {
        return Err(format!(
            "Detached child handle identity mismatch: expected={task_id} actual={}",
            child_handle.session_id()
        ));
    }
    let completed_assistant = provider_v1_sink
        .completed_assistant_text()
        .cloned()
        .ok_or_else(|| {
            String::from(
                "Detached child completion has no exact durable V1 final assistant receipt.",
            )
        })?;
    let input = child_handle
        .read(|document| {
            let source_event = document
                .events()
                .iter()
                .find(|event| event.seq == completed_assistant.seq)
                .ok_or_else(|| {
                    String::from("Detached child final assistant seq is missing from V1.")
                })?;
            let final_turn_id = source_event.turn_id.ok_or_else(|| {
                String::from("Detached child final assistant has no V1 Turn identity.")
            })?;
            let EventBody::AssistantText(source_text) = &source_event.event else {
                return Err(String::from(
                    "Detached child final assistant receipt points to another V1 event kind.",
                ));
            };
            if source_text.status != AssistantSemanticStatus::Completed
                || source_text.api_call_id != completed_assistant.api_call_id
                || source_text.text != completed_assistant.text
            {
                return Err(String::from(
                    "Detached child final assistant differs from its durable stream receipt.",
                ));
            }
            document
                .build_subagent_return_input(SubagentReturnBuildRequest {
                    final_turn_id,
                    child_task_id: SubagentTaskId::try_new(task_id.to_string())
                        .map_err(|error| error.to_string())?,
                    source_assistant_text_seq: completed_assistant.seq,
                })
                .map_err(|error| error.to_string())
        })
        .map_err(|error| format!("读取子会话 V1 回流事实失败：{error}"))??;
    let parent_handle = {
        let registry = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .session_runtime_registry
            .clone();
        registry
            .get(parent_session_id)
            .map_err(|error| format!("父会话 V1 owner 不可用：{error}"))?
    };
    let committed = parent_handle
        .append_subagent_return_with_stats(input, current_v1_unix_millis()?)
        .map_err(|error| format!("子代理 V1 回流提交失败：{error}"))?;
    webui_trace(
        "detached_subagent_v1_return_committed",
        format!(
            "parent={parent_session_id} task={task_id} return_seq={} stats_seq={} revision={} changed={}",
            committed.value.return_seq.value(),
            committed.value.stats_seq.value(),
            committed.revision,
            committed.changed
        ),
    );
    Ok(committed.value)
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
    child_handle: &SessionHandle,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    provider_v1_sink: &mut ProviderV1StreamSink,
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
                agent_runtime,
                execution_id,
                provider_telemetry_guard,
                provider_v1_sink,
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
            let v1_return_receipt = if listener_is_current && is_finished {
                Some(append_exact_v1_subagent_return(
                    state,
                    parent_session_id,
                    task_id,
                    child_handle,
                    provider_v1_sink,
                )?)
            } else {
                None
            };
            let consumed = if listener_is_current {
                match ingest_webui_subagent_runtime_event_for_listener_execution(
                    app,
                    state,
                    agent_runtime,
                    parent_session_id,
                    &event,
                    Some(execution_id),
                    false,
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
            if !consumed && !is_finished {
                append_runtime_event_projection(app, state, parent_session_id, &event)?;
                release_runtime_event_projection_barrier_after_attempt(&event)?;
            } else {
                release_runtime_event_projection_barrier_after_attempt(&event)?;
            }
            if is_finished {
                let cleaned_current_listener = {
                    let mut guard = state
                        .lock()
                        .map_err(|error| format!("failed to lock service state: {error}"))?;
                    let cleaned = clear_webui_detached_listener_execution_if_current(
                        &mut guard,
                        task_id,
                        execution_id,
                    );
                    if !cleaned && v1_return_receipt.is_some() {
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
                if let Some(receipt) = v1_return_receipt
                    && parent_is_regular_session
                    && !parent_is_stopped
                {
                    let started = start_exact_v1_subagent_return_turn(
                        app,
                        state,
                        controller_runtime,
                        agent_runtime,
                        parent_session_id,
                        receipt,
                    )
                    .await?;
                    webui_trace(
                        "detached_parent_event_v1_return_dispatch",
                        format!("parent={parent_session_id} task={task_id} started={started}"),
                    );
                    if !started {
                        return Err(format!(
                            "The durable subagent return for parent `{parent_session_id}` was not physically dispatched."
                        ));
                    }
                }
            }
            if is_finished && parent_is_regular_session && parent_is_stopped {
                webui_trace(
                    "detached_parent_event_v1_return_preserved_while_stopped",
                    format!("parent={parent_session_id} task={task_id}"),
                );
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
/// `state` owns listener identities under one lock.
/// `state` 在同一锁下持有监听器身份。
/// `task_id` identifies the stable child task.
/// `task_id` 标识稳定子任务。
/// `execution_id` identifies the listener execution attempting cleanup.
/// `execution_id` 标识尝试清理的监听器执行。
/// # Returns
/// 返回值
/// Whether the exact execution still owned and cleared the listener row.
/// 精确执行是否仍拥有并清理了监听器行。
pub(crate) fn clear_webui_detached_listener_execution_if_current(
    state: &mut ServiceBridgeState,
    task_id: SessionId,
    execution_id: &str,
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
    true
}

/// Applies one child execution stream event using the same projection functions as root WebUI turns.
/// 使用与根 WebUI 回合相同的投影函数应用单条子执行流事件。
pub(crate) async fn handle_webui_detached_child_execution_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    execution_id: &str,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    provider_v1_sink: &mut ProviderV1StreamSink,
    event: ExecutionStreamEvent,
) -> Result<(), String> {
    match event {
        ExecutionStreamEvent::Prepared(_) => Ok(()),
        ExecutionStreamEvent::ProviderRequestCheckpoint { .. } => Ok(()),
        ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
            provider_telemetry_guard.forward(event);
            Ok(())
        }
        ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
            // eventKind and typed lifecycle are retained before telemetry consumes the envelope.
            // eventKind 与强类型生命周期在遥测消费信封前保留。
            let event_kind = event.kind;
            let v1_lifecycle = event.v1_lifecycle.clone();
            let v1_boundary_result = match (event_kind, v1_lifecycle) {
                (
                    ProviderRequestTelemetryEventKind::Started,
                    Some(ProviderV1LifecycleFacts::RequestPrepared(_)),
                ) => Err(ProviderV1SinkError::UnexpectedRequestStartDurabilityBoundary),
                (
                    ProviderRequestTelemetryEventKind::Completed,
                    Some(ProviderV1LifecycleFacts::ApiTerminal(facts)),
                ) => provider_v1_sink.accept_terminal(facts).await.map(|_| ()),
                (
                    ProviderRequestTelemetryEventKind::Dispatched,
                    Some(ProviderV1LifecycleFacts::TransportObserved(facts)),
                ) => provider_v1_sink.mark_transport_observed(facts).await,
                (
                    ProviderRequestTelemetryEventKind::Started
                    | ProviderRequestTelemetryEventKind::Completed,
                    Some(ProviderV1LifecycleFacts::ExcludedNonSession),
                ) => Ok(()),
                (ProviderRequestTelemetryEventKind::Started, _) => {
                    Err(ProviderV1SinkError::MissingTypedRequestFacts)
                }
                (ProviderRequestTelemetryEventKind::Completed, _) => {
                    Err(ProviderV1SinkError::MissingTypedTerminalFacts)
                }
                _ => Ok(()),
            };
            if let Err(error) = v1_boundary_result {
                let error = error.to_string();
                let _ = completion.complete(Err(error.clone()));
                return Err(error);
            }
            provider_telemetry_guard.observe_durable_boundary(&event);
            provider_telemetry_guard.forward(event);
            let result = Ok(());
            let _ = completion.complete(result.clone());
            result
        }
        ExecutionStreamEvent::ProviderRequestPreparationBoundary {
            event,
            candidate,
            completion,
        } => {
            let result = match event.v1_lifecycle.as_ref() {
                Some(ProviderV1LifecycleFacts::ExcludedNonSession) => {
                    Ok(candidate.into_excluded_non_session())
                }
                Some(ProviderV1LifecycleFacts::RequestPrepared(facts)) => provider_v1_sink
                    .prepare_request(facts.clone(), candidate)
                    .await
                    .map_err(|error| error.to_string()),
                _ => Err(String::from(
                    "Detached Provider preparation is missing typed request facts.",
                )),
            };
            let response = result.as_ref().map(|_| ()).map_err(Clone::clone);
            let _ = completion.complete(result);
            response
        }
        ExecutionStreamEvent::ProviderLoweringRepair { .. } => {
            // Detached sessions persist exact request facts through their V1 Provider permit.
            // 子会话通过自身的 V1 Provider 许可持久化精确请求事实。
            Ok(())
        }
        ExecutionStreamEvent::RequestReady { session_id }
        | ExecutionStreamEvent::WireRequestReady { session_id } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::RequestStarted { session_id } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::ThinkingDelta { delta, .. } => provider_v1_sink
            .append_reasoning_delta(&delta, current_v1_unix_millis()?)
            .map_err(|error| error.to_string()),
        ExecutionStreamEvent::TransientReasoningStatusChanged { session_id, status } => {
            emit_transient_reasoning_status(app, state, session_id, status)
        }
        ExecutionStreamEvent::ThinkingCompleted { .. } => provider_v1_sink
            .complete_reasoning(current_v1_unix_millis()?)
            .map_err(|error| error.to_string()),
        ExecutionStreamEvent::OutputDelta { delta, .. } => provider_v1_sink
            .append_assistant_delta(&delta, current_v1_unix_millis()?)
            .map_err(|error| error.to_string()),
        ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
            if let Err(error) =
                dispatch_runtime_event_hook(agent_runtime, session_id, &event, execution_id)
            {
                eprintln!("[webui-service] detached runtime event Hook dispatch failed: {error}");
            }
            if let Some(boundary) = event.tool_execution_boundary.as_ref() {
                let outcome = provider_v1_sink
                    .apply_tool_boundary(boundary, current_v1_unix_millis()?)
                    .await
                    .map_err(|error| error.to_string())?;
                match outcome {
                    ProviderV1ToolBoundaryOutcome::Applied => {
                        vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::acknowledge_runtime_event_projection_barrier_from_event(&event)?;
                    }
                    ProviderV1ToolBoundaryOutcome::BatchPrepared(identities) => {
                        vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::acknowledge_tool_batch_projection_barrier_from_event(&event, identities)?;
                    }
                }
                return Ok(());
            }
            append_runtime_event_projection_and_release_barrier(app, state, session_id, &event)?;
            Ok(())
        }
        ExecutionStreamEvent::ConversationCheckpoint { .. }
        | ExecutionStreamEvent::ContextCompactionRequired { .. }
        | ExecutionStreamEvent::ProviderRouteSwitchRequired { .. }
        | ExecutionStreamEvent::ContextCompactionPolicyRefreshRequired { .. } => {
            // Child-session durability is already committed by the V1 stream sink at semantic boundaries.
            // 子会话持久化已由 V1 流接收器在语义边界提交，此处不再生成旧 checkpoint 副本。
            Ok(())
        }
        ExecutionStreamEvent::Finished(report) => {
            let _ = agent_runtime;
            apply_task_session_execution_report_to_memory_in_place(state, &report)?;
            Ok(())
        }
    }
}
