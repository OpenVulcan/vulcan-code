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

#[cfg(test)]
mod admission_tests;
mod listener_owner;

pub(crate) mod non_success;
pub(crate) mod retry;
pub(crate) mod startup_abort;
pub(crate) mod startup_rejection;
mod terminal;

/// Starts one detached child-task listener using the shared task runtime stream.
/// 使用共享任务运行时流启动单个分离子任务监听器。
pub(crate) fn maybe_start_webui_detached_subagent_task_listener(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    parent_session_id: SessionId,
    event: &RuntimeEvent,
) -> Result<(), String> {
    if event.kind != RuntimeEventKind::DelegateStarted {
        return Ok(());
    }
    let metadata = event
        .metadata
        .as_ref()
        .ok_or_else(|| String::from("DelegateStarted is missing authoritative task metadata."))?;
    let panel_state = parse_subagent_task_panel_metadata(parent_session_id, metadata, None)
        .ok_or_else(|| String::from("DelegateStarted has invalid authoritative task metadata."))?;
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
        return Err(format!(
            "DelegateStarted execution is no longer current: task={task_id} execution={execution_id}."
        ));
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
    start_admitted_detached_subagent_listener(
        app,
        state,
        controller_runtime,
        agent_runtime,
        parent_session_id,
        child_handle,
        execution_id,
    )
}

/// Attaches the existing event sink and reliable finalizer to an already admitted child document.
/// 为已经准入的子文档附加既有事件接收器及可靠终态处理器。
/// Callers must retain canonical lifecycle admission; this entry never creates another root input.
/// 调用者必须保留规范生命周期准入；此入口绝不创建另一条根输入。
/// Returns after subscription is installed, or after exact failed-start cleanup.
/// 安装订阅后返回，或在精确失败启动清理后返回错误。
pub(crate) fn start_admitted_detached_subagent_listener(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    parent_session_id: SessionId,
    child_handle: Arc<SessionHandle>,
    execution_id: String,
) -> Result<(), String> {
    // Runtime facts and the existing document must refer to the same concrete execution.
    // 运行时事实与已有文档必须指向同一个具体执行。
    let task_id = child_handle.session_id();
    let facts = agent_runtime
        .task_runtime
        .active_task_v1_execution_facts(task_id, &execution_id)?;
    if facts.parent_session_id != parent_session_id || facts.task_id != task_id {
        return Err(String::from(
            "Detached listener does not match its runtime execution owner.",
        ));
    }
    // Freeze exact cleanup ownership before any fallible listener admission.
    // 在任何可能失败的监听器准入之前冻结精确清理归属。
    let startup_owner = startup_rejection::capture_owner(&child_handle, &execution_id)?;
    // Retain canonical cleanup owners when successful admission moves originals into the listener.
    // 成功准入将原对象移入监听器时，保留规范清理所有者。
    let rejected_child = Arc::clone(&child_handle);
    // The rejected generation must not be replaced by a later listener's identifier.
    // 被拒代次不得替换为后续监听器的标识。
    let rejected_execution = execution_id.clone();
    // Only this runtime owns the unpublished return gate.
    // 只有该运行时拥有未发布回流门。
    let rejected_runtime = agent_runtime.task_runtime.clone();
    // Registration cleanup must compare the same service instance and exact execution.
    // 注册清理必须比较同一服务实例和精确执行。
    let rejected_state = state.clone();
    // Admission must finish before the caller acknowledges the Provider start barrier.
    // 准入必须在调用方确认 Provider 启动屏障之前完成。
    let admission = (|| -> Result<(), String> {
        crate::session_plan_runtime::emit_lifetime_usage_changed(&app, &state, &child_handle);
        agent_runtime
            .task_runtime
            .expect_durable_task_return(task_id, &execution_id)?;

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
        // Subscribe before spawning; every rejection uses the common exact-ownership cleanup below.
        // 先订阅再派生；所有拒绝统一使用下方精确归属清理。
        let detached_stream = agent_runtime
            .task_runtime
            .subscribe_active_detached_task_events(task_id, &execution_id)?;
        webui_trace(
            "detached_listener_spawn",
            format!("parent={parent_session_id} task={task_id} execution_id={execution_id}"),
        );
        let project_resource_catalog_registry = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))
            .map(|guard| Arc::clone(&guard.project_resource_catalog_registry))?;
        // Capture before spawning so cancellation before the first poll also releases ownership.
        // 派生前捕获，确保首次轮询前取消也会释放所有权。
        let listener_owner = listener_owner::ListenerOwner::new(
            state.clone(),
            agent_runtime.clone(),
            task_id,
            execution_id.clone(),
        );
        spawn_service_task(async move {
            // Keep ownership through retained terminal retries, including EOF after valid evidence.
            // 在保留终态重试期间保持所有权，包括有效凭据之后的流结束。
            let _listener_owner = listener_owner;
            // providerV1Sink retains the exact child handle for the complete detached execution.
            // providerV1Sink 在完整分离执行期间保留精确子句柄。
            let mut provider_v1_sink = ProviderV1StreamSink::new(
                Arc::clone(&child_handle),
                project_resource_catalog_registry,
            );
            // providerTelemetryGuard survives for the complete detached child-event subscription.
            // providerTelemetryGuard 在完整分离子事件订阅期间保持存活。
            let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new_with_persistence(
                state.clone(),
                Some(controller_runtime.clone()),
            );
            retry::drive(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                parent_session_id,
                task_id,
                &execution_id,
                &child_handle,
                startup_owner,
                detached_stream,
                &mut provider_telemetry_guard,
                &mut provider_v1_sink,
            )
            .await;
            if let Some(event) = provider_telemetry_guard.take_cancelled_event() {
                provider_telemetry_guard.forward(event);
                crate::session_plan_runtime::emit_lifetime_usage_changed(
                    &app,
                    &state,
                    &child_handle,
                );
            }
        });
        Ok(())
    })();
    if let Err(error) = admission {
        // Attempt every independent cleanup and preserve each failure beside the original cause.
        // 尝试每个独立清理，并在原原因旁保留各项失败。
        let mut cleanup_errors = Vec::new();
        if let Err(cleanup) =
            startup_rejection::close_rejected_child(&rejected_child, startup_owner, &error)
        {
            cleanup_errors.push(format!("Child document cleanup failed: {cleanup}"));
        }
        if let Err(cleanup) =
            rejected_runtime.cancel_rejected_task_return(task_id, &rejected_execution)
        {
            cleanup_errors.push(format!("Child return gate cleanup failed: {cleanup}"));
        }
        match rejected_state.lock() {
            Ok(mut guard) => {
                clear_webui_detached_listener_execution_if_current(
                    &mut guard,
                    task_id,
                    &rejected_execution,
                );
            }
            Err(cleanup) => {
                cleanup_errors.push(format!("Child listener cleanup failed: {cleanup}"))
            }
        }
        return if cleanup_errors.is_empty() {
            Err(error)
        } else {
            Err(format!("{error} {}", cleanup_errors.join(" ")))
        };
    }
    Ok(())
}

/// Atomically returns durable child final content and its source identity to the parent.
/// 将可靠子会话最终正文与来源身份原子回流给父会话。
fn append_exact_v1_subagent_return(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
    task_id: SessionId,
    execution_id: &str,
    child_handle: &SessionHandle,
    completed_assistant: Option<&ProviderV1CompletedAssistantText>,
) -> Result<SubagentReturnReceipt, String> {
    if child_handle.session_id() != task_id {
        return Err(format!(
            "Detached child handle identity mismatch: expected={task_id} actual={}",
            child_handle.session_id()
        ));
    }
    let completed_assistant = completed_assistant.cloned().ok_or_else(|| {
        String::from("Detached child completion has no exact durable V1 final assistant receipt.")
    })?;
    let parent_handle = {
        let registry = state
            .lock()
            .map_err(|error| error.to_string())?
            .session_runtime_registry
            .clone();
        registry
            .get(parent_session_id)
            .map_err(|error| error.to_string())?
    };
    // Source sequence coverage is meaningful only inside the exact child generation.
    // 来源序号覆盖只在精确子代次内部有意义。
    let child_generation = child_handle
        .read(|document| document.segment_generation())
        .map_err(|error| error.to_string())?;
    let after_seq = parent_handle
        .read_full_durable(|document| {
            document.subagent_return_frontier_before(
                &task_id.to_string(),
                child_generation,
                completed_assistant.seq,
            )
        })
        .map_err(|error| error.to_string())?;
    let input = child_handle
        .read_full_durable(|document| {
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
                .build_subagent_return_input_since(
                    SubagentReturnBuildRequest {
                        final_turn_id,
                        child_task_id: SubagentTaskId::try_new(task_id.to_string())
                            .map_err(|error| error.to_string())?,
                        source_assistant_text_seq: completed_assistant.seq,
                    },
                    after_seq,
                )
                .map_err(|error| error.to_string())
        })
        .map_err(|error| format!("读取子会话 V1 回流事实失败：{error}"))??;
    let committed = parent_handle
        .append_execution_subagent_return(
            vulcan_session_core::v1::collaboration::CollaborationExecutionId::try_new(execution_id)
                .map_err(|error| error.to_string())?,
            input,
            current_v1_unix_millis()?,
        )
        .map_err(|error| format!("子代理 V1 回流提交失败：{error}"))?;
    webui_trace(
        "detached_subagent_v1_return_committed",
        format!(
            "parent={parent_session_id} task={task_id} return_seq={} revision={} changed={}",
            committed.value.return_seq.value(),
            committed.revision,
            committed.changed
        ),
    );
    Ok(committed.value)
}

/// Applies one detached subagent runtime event to the shared WebUI transcript projection.
/// 将单条分离子代理运行时事件应用到共享 WebUI 转录投影。
// Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary.
// 事件边界显式保留流所有权、代际守卫和独立借用的遥测状态。
#[expect(
    clippy::too_many_arguments,
    reason = "Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary."
)]
pub(crate) async fn handle_webui_detached_subagent_task_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
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
        DetachedTaskRuntimeEvent::StartupAborted { .. } => Err(String::from(
            "Startup abort must be handled by its captured admission owner.",
        )),
        DetachedTaskRuntimeEvent::ChildExecution(event) => {
            if !listener_is_current {
                webui_trace(
                    "detached_child_event_stale_execution",
                    format!("task={task_id} execution_id={execution_id}"),
                );
                // A superseded parent cannot admit descendants; reject their waiter immediately.
                // 已被替代的父执行不能准入后代；立即拒绝其等待器。
                if let ExecutionStreamEvent::RuntimeEvent { event, .. } = &event
                    && event.kind == RuntimeEventKind::DelegateStarted
                {
                    let error = "Nested DelegateStarted belongs to a superseded parent execution.";
                    vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::reject_runtime_event_projection_barrier_from_event_with_reason(event, error)?;
                    return Err(String::from(error));
                }
                return Ok(());
            }
            // Nested starts travel through the owning child's stream, not the root stream.
            // 嵌套启动经由所属子任务的事件流传递，不经过根事件流。
            // Establish the descendant sink before acknowledging its execution barrier.
            // 在确认执行屏障前建立后代任务的持久接收器。
            if let ExecutionStreamEvent::RuntimeEvent { session_id, event } = &event
                && event.kind == RuntimeEventKind::DelegateStarted
            {
                // The listener identity is authoritative for the spawning parent.
                // 监听器身份是启动父任务的权威来源。
                let admission = if *session_id != task_id {
                    Err(String::from(
                        "Nested DelegateStarted does not belong to the owning task session.",
                    ))
                } else {
                    maybe_start_webui_detached_subagent_task_listener(
                        app.clone(),
                        state.clone(),
                        controller_runtime.clone(),
                        agent_runtime.clone(),
                        task_id,
                        event,
                    )
                };
                if let Err(error) = admission {
                    vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::reject_runtime_event_projection_barrier_from_event_with_reason(event, &error)
                        .map_err(|rejection_error| format!("{error} Rejecting the nested start barrier also failed: {rejection_error}"))?;
                    return Err(error);
                }
            }
            handle_webui_detached_child_execution_event(
                app,
                state,
                agent_runtime,
                task_id,
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
            terminal::handle_parent_event(
                app,
                state,
                controller_runtime,
                agent_runtime,
                task_id,
                execution_id,
                child_handle,
                parent_session_id,
                event,
                provider_v1_sink.completed_assistant_text(),
            )
            .await
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
// Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary.
// 事件边界显式保留流所有权、代际守卫和独立借用的遥测状态。
#[expect(
    clippy::too_many_arguments,
    reason = "Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary."
)]
pub(crate) async fn handle_webui_detached_child_execution_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    task_id: SessionId,
    execution_id: &str,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    provider_v1_sink: &mut ProviderV1StreamSink,
    event: ExecutionStreamEvent,
) -> Result<(), String> {
    match event {
        ExecutionStreamEvent::Prepared(_) => Ok(()),
        ExecutionStreamEvent::ProviderRequestCheckpoint { .. } => Ok(()),
        ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
            if event.kind == ProviderRequestTelemetryEventKind::FirstToken {
                // Preserve replay-safety evidence without treating best-effort Started as admission.
                // 保留重放安全证据，不把尽力投递的 Started 视为准入。
                provider_telemetry_guard.observe_durable_boundary(&event);
            }
            provider_telemetry_guard.forward(event);
            Ok(())
        }
        ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
            // eventKind and typed lifecycle are retained before telemetry consumes the envelope.
            // eventKind 与强类型生命周期在遥测消费信封前保留。
            let event_kind = event.kind;
            let v1_lifecycle = event.v1_lifecycle.clone();
            // Consume the physical terminal before projection so failures do not create a second cancellation.
            // 在投影前消费物理终态，避免失败再产生一次取消。
            if event_kind == ProviderRequestTelemetryEventKind::Completed {
                provider_telemetry_guard.observe_durable_boundary(&event);
            }
            let v1_boundary_result = match (event_kind, v1_lifecycle) {
                (
                    ProviderRequestTelemetryEventKind::Started,
                    Some(ProviderV1LifecycleFacts::RequestPrepared(_)),
                ) => Err(ProviderV1SinkError::UnexpectedRequestStartDurabilityBoundary),
                (
                    ProviderRequestTelemetryEventKind::Completed,
                    Some(ProviderV1LifecycleFacts::ApiTerminal(_)),
                ) => provider_v1_sink
                    .accept_telemetry_terminal(&event, app, state)
                    .await
                    .map(|_| ()),
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
                    candidate.into_excluded_non_session()
                }
                Some(ProviderV1LifecycleFacts::RequestPrepared(facts)) => {
                    match provider_v1_sink.consume_subagent_steering(&agent_runtime.task_runtime) {
                        Ok(consumed) => {
                            if consumed > 0 {
                                // Refresh the owning root even when another conversation is selected.
                                // 即使选中了其他会话，也刷新队列所属的根会话。
                                let refresh = agent_runtime
                                    .task_runtime
                                    .active_task_v1_execution_facts(task_id, execution_id)
                                    .and_then(|facts| {
                                        emit_webui_subagent_navigation_changed_delta_for_parent(
                                            app,
                                            state,
                                            facts.root_session_id,
                                        )
                                    });
                                if let Err(error) = refresh {
                                    tracing::warn!(
                                        "[subagent] consumed message queue projection failed: {error}"
                                    );
                                }
                            }
                            provider_v1_sink
                                .prepare_request(state, facts.clone(), candidate)
                                .await
                                .map_err(|error| error.to_string())
                        }
                        Err(error) => Err(error),
                    }
                }
                _ => Err(String::from(
                    "Detached Provider preparation is missing typed request facts.",
                )),
            };
            // The committed API remains owned even if the waiting producer has already been dropped.
            // 即使等待中的生产者已经被丢弃，已提交 API 仍有明确归属。
            provider_telemetry_guard.observe_preparation_result(&event, &result);
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
                tracing::warn!(
                    "[webui-service] detached runtime event Hook dispatch failed: {error}"
                );
            }
            if event.tool_execution_boundary.is_some() {
                // tool_boundary_result retains the exact sink or timestamp failure before this
                // detached listener can leave the mandatory runtime projection barrier pending.
                // tool_boundary_result 在分离监听器可能遗留强制运行时投影屏障前，保留精确的
                // sink 或时间戳失败。
                let tool_boundary_result = match current_v1_unix_millis() {
                    Ok(now_ms) => provider_v1_sink
                        .apply_tool_event(app, state, &event, now_ms)
                        .await
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error),
                };
                // outcome is released only after the same detached V1 owner accepted the boundary.
                // outcome 仅在同一分离 V1 owner 接纳边界后释放。
                let outcome = match tool_boundary_result {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        // rejection_result ensures the waiting executor receives the originating
                        // failure immediately instead of losing it behind a later timeout.
                        // rejection_result 确保等待中的执行器立即收到原始失败，而不是让其在
                        // 后续超时中丢失。
                        let rejection_result = vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::reject_runtime_event_projection_barrier_from_event_with_reason(&event, &error);
                        if let Err(rejection_error) = rejection_result {
                            return Err(format!(
                                "{error} Rejecting the detached tool projection barrier also failed: {rejection_error}"
                            ));
                        }
                        return Err(error);
                    }
                };
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
