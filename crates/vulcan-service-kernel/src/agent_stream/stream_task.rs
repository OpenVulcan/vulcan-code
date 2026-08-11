//! Agent runtime stream task entrypoints.
//! Agent 运行时流任务入口。

use super::*;

mod content_delta;
mod conversation_checkpoint;
mod finished;
mod launch_failure;
mod prepared;
mod request_phase;
mod request_trace;
mod runtime_event;
mod stream_exit;

use conversation_checkpoint::handle_conversation_checkpoint_stream_event;
use finished::{FinishedStreamEventOutcome, handle_finished_stream_event};
pub(crate) use launch_failure::StreamTaskLaunchFailureCommit;
#[cfg(test)]
pub(crate) use launch_failure::StreamTaskLaunchFailureOutcome;
pub(crate) use launch_failure::{
    commit_stream_task_registration_failure, handle_pre_registration_stream_task_launch_failure,
    handle_stream_task_registration_failure,
};
#[cfg(test)]
pub(crate) use prepared::PreparedStreamFailureCommit;
pub(crate) use prepared::commit_prepared_stream_failure;
use prepared::{PreparedStreamEventAction, handle_prepared_stream_event};
use request_phase::{RequestPhaseStreamEvent, handle_request_phase_stream_event};
use request_trace::StreamRequestTraceState;
use runtime_event::{handle_runtime_stream_event, runtime_event_hook_policy_for_execution_scope};
#[cfg(test)]
pub(crate) use stream_exit::StreamTaskExitOutcome;
pub(crate) use stream_exit::{StreamTaskExitReason, handle_stream_task_exit};

/// Spawns one background task that forwards agent runtime stream events into the WebUI state.
/// 启动一个后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
pub(crate) fn spawn_agent_stream_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
) {
    spawn_agent_stream_task_with_auto_compaction_state(
        app,
        state,
        controller_runtime,
        agent_runtime,
        request,
        false,
    );
}

/// Spawns one background task that forwards agent runtime stream events with per-request auto-compaction state.
/// 启动一个携带单请求自动压缩状态的后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
///
/// # Parameters
/// # 参数
/// `app` emits workbench snapshots and runtime stream projections.
/// `app` 发出工作台快照与运行时流投影。
/// `state` owns retained service state and active stream handles.
/// `state` 持有保留的服务状态与活跃流句柄。
/// `controller_runtime` persists provider checkpoints and terminal session state.
/// `controller_runtime` 持久化 Provider 检查点与终态会话状态。
/// `agent_runtime` provides provider, tool, hook, and compaction runtimes.
/// `agent_runtime` 提供 Provider、工具、hook 与压缩运行时。
/// `request` is the prepared execution request for the current user turn.
/// `request` 是当前用户回合的待执行请求。
/// `auto_compaction_already_attempted` prevents repeated automatic compaction loops for one user request.
/// `auto_compaction_already_attempted` 防止单个用户请求反复进入自动压缩循环。
pub(crate) fn spawn_agent_stream_task_with_auto_compaction_state(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
    auto_compaction_already_attempted: bool,
) {
    let session_id = request.session_id;
    // expected_kernel_generation binds any pre-registration contract failure to the exact active turn.
    // expected_kernel_generation 将任意预注册契约失败绑定到精确活动回合。
    let expected_kernel_generation = state
        .lock()
        .ok()
        .and_then(|guard| webui_session_kernel_generation(&guard, session_id));
    // runtime_event_hook_policy captures the request-authoritative Hook scope before the request moves into the task.
    // runtime_event_hook_policy 在请求移入任务前捕获由请求权威确定的 Hook 范围。
    let runtime_event_hook_policy = runtime_event_hook_policy_for_execution_scope(
        &request.execution_scope,
        request.runtime_policy.hooks_enabled,
        request.conversation_hook_turn_id.as_deref(),
    );
    let runtime_event_hook_policy = match runtime_event_hook_policy {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("[webui-service] runtime event Hook correlation rejected: {error}");
            // cleanup_outcome records how the pre-registration contract failure terminalized the session.
            // cleanup_outcome 记录预注册契约失败如何终态化该会话。
            let cleanup_outcome = handle_pre_registration_stream_task_launch_failure(
                &app,
                &state,
                session_id,
                expected_kernel_generation,
            );
            match cleanup_outcome {
                Ok(outcome) => webui_trace(
                    "stream_hook_correlation_rejected",
                    format!("session={session_id} cleanup={outcome:?} error={error}"),
                ),
                Err(cleanup_error) => eprintln!(
                    "[webui-service] runtime event Hook correlation cleanup failed: {cleanup_error}; original error: {error}"
                ),
            }
            return;
        }
    };
    let session_goal_id = request
        .session_goal_context
        .as_ref()
        .map(|context| context.goal_id.clone());
    let active_stream_task_id = Uuid::new_v4();
    webui_after_precheck_trace(
        "after_precheck_19_stream_task_scheduled",
        format!("session={session_id}"),
    );
    webui_trace("stream_task_spawn", format!("session={session_id}"));
    let task_state = state.clone();
    let registration_failure_app = app.clone();
    let registration_failure_state = state.clone();
    let task_future = async move {
        // request is enriched exactly once with the authorized session selection before tool resolution.
        // request 在工具解析前仅使用已授权会话选择增强一次。
        let mut request = request;
        if request.knowledge_access.is_none() {
            request.knowledge_access =
                match resolve_turn_knowledge_access(&task_state, session_id).await {
                    Ok(access) => access,
                    Err(error) => {
                        eprintln!("[webui-service] knowledge access resolution failed: {error}");
                        None
                    }
                };
        }
        if let Some(access) = request.knowledge_access.as_mut()
            && access.knowledge_search_handler.is_none()
        {
            access.knowledge_search_handler =
                Some(build_knowledge_search_handler(task_state.clone()));
        }
        let stream_task_started_at = current_unix_millis_value();
        webui_after_precheck_trace(
            "after_precheck_20_stream_task_entered",
            format!("session={session_id}"),
        );
        webui_trace("stream_task_entered", format!("session={session_id}"));
        let mut record_ids = SessionStreamingRecordIds::default();
        // providerTelemetryGuard closes an active physical request even when this task future is aborted.
        // providerTelemetryGuard 即使当前任务 Future 被中止，也会关闭活动真实请求。
        let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new_with_persistence(
            task_state.clone(),
            Some(controller_runtime.clone()),
            Some(app.clone()),
        );
        // executionProviderGoalId preserves goal ownership across tool-result requests after a goal reaches a stopped state.
        // executionProviderGoalId 在目标进入停止状态后，跨工具结果请求保留目标归属。
        let mut execution_provider_goal_id = request
            .session_goal_context
            .as_ref()
            .map(|goal_context| goal_context.goal_id.clone());
        // activeProviderGoalId binds exactly one physical request to its resolved goal owner.
        // activeProviderGoalId 将恰好一次物理请求绑定到其解析后的目标所有者。
        let mut active_provider_goal_id: Option<String> = None;
        // active_preparation preserves the exact provider/model budget facts for a mid-turn compaction restart.
        // active_preparation 保留中途回合压缩重启所需的精确 Provider/模型预算事实。
        let mut active_preparation: Option<ExecutionPreparation> = None;
        let intervention_started_at = current_unix_millis_value();
        let prompt_intervention = build_webui_prompt_intervention(app.clone(), task_state.clone());
        webui_after_precheck_trace(
            "after_precheck_21_intervention_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(intervention_started_at)
            ),
        );
        let provider_stream_create_started_at = current_unix_millis_value();
        webui_trace(
            "stream_provider_create_start",
            format!("session={session_id}"),
        );
        let mut stream = Box::pin(stream_runtime_run_with_intervention_plugins_and_public_ai(
            agent_runtime.provider_configuration.clone(),
            agent_runtime.agent_profile_catalog.clone(),
            agent_runtime.luaskills_runtime.clone(),
            Some(agent_runtime.vulcan_plugin_runtime.clone()),
            Some(Arc::clone(&agent_runtime.extension_generation)),
            Arc::clone(&agent_runtime.task_runtime),
            Arc::clone(&agent_runtime.question_runtime),
            Arc::clone(&agent_runtime.command_session_runtime),
            Some(Arc::clone(&agent_runtime.public_ai_runtime)),
            request.clone(),
            Some(prompt_intervention),
        ));

        webui_trace(
            "stream_provider_create_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(provider_stream_create_started_at)
            ),
        );
        webui_after_precheck_trace(
            "after_precheck_22_stream_created",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(provider_stream_create_started_at)
            ),
        );
        webui_trace(
            "stream_task_poll_start",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(stream_task_started_at)
            ),
        );
        webui_after_precheck_trace(
            "after_precheck_23_poll_start",
            format!(
                "session={session_id} elapsed_ms_since_task={}",
                current_unix_millis_value().saturating_sub(stream_task_started_at)
            ),
        );
        // This request-local state machine keeps latency markers consistent across every stream event kind.
        // 该请求局部状态机用于确保所有流事件类型的延迟标记保持一致。
        let mut request_trace_state = StreamRequestTraceState::new(stream_task_started_at);
        // stream_exit_reason makes every polling-loop terminal path explicit without a shadow completion boolean.
        // stream_exit_reason 显式表达每条轮询循环终止路径，不再使用影子完成布尔值。
        let stream_exit_reason = loop {
            // event is the next ordered runtime item owned by this exact stream generation.
            // event 是当前精确流代际拥有的下一条有序运行时事件。
            let Some(event) = stream.next().await else {
                break StreamTaskExitReason::StreamExhausted;
            };
            if !webui_stream_event_matches_kernel_generation(
                &task_state,
                &event,
                expected_kernel_generation,
            ) {
                webui_trace(
                    "stream_event_skipped_stale_generation",
                    format!(
                        "session={} expected_generation={}",
                        execution_stream_event_session_id(&event),
                        expected_kernel_generation
                            .map(|generation| generation.to_string())
                            .unwrap_or_else(|| String::from("<none>"))
                    ),
                );
                break StreamTaskExitReason::StaleKernelGeneration;
            }
            match event {
                ExecutionStreamEvent::Prepared(preparation) => {
                    active_preparation = Some(preparation.clone());
                    match handle_prepared_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        &request,
                        preparation,
                        session_id,
                        stream_task_started_at,
                        auto_compaction_already_attempted,
                        active_stream_task_id,
                    )
                    .await
                    {
                        PreparedStreamEventAction::Continue => {}
                        PreparedStreamEventAction::Restarted
                        | PreparedStreamEventAction::Failed => {
                            return;
                        }
                    }
                }
                ExecutionStreamEvent::ProviderRequestCheckpoint {
                    session_id,
                    provider_family,
                    snapshot,
                } => {
                    if let Err(error) = record_webui_provider_request_checkpoint(
                        &controller_runtime,
                        session_id,
                        &provider_family,
                        &snapshot,
                    ) {
                        eprintln!("[webui-service] provider request checkpoint failed: {error}");
                    }
                }
                ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
                    if let Err(error) = handle_stream_provider_request_telemetry(
                        &task_state,
                        &controller_runtime,
                        &app,
                        &mut provider_telemetry_guard,
                        event,
                        &mut active_provider_goal_id,
                        &mut execution_provider_goal_id,
                        active_stream_task_id,
                        None,
                    )
                    .await
                    {
                        eprintln!(
                            "[webui-service] provider usage telemetry state update failed: {error}"
                        );
                    }
                }
                ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
                    if let Err(error) = handle_stream_provider_request_telemetry(
                        &task_state,
                        &controller_runtime,
                        &app,
                        &mut provider_telemetry_guard,
                        event,
                        &mut active_provider_goal_id,
                        &mut execution_provider_goal_id,
                        active_stream_task_id,
                        Some(completion),
                    )
                    .await
                    {
                        eprintln!(
                            "[webui-service] durable provider usage boundary failed: {error}"
                        );
                    }
                }
                ExecutionStreamEvent::ProviderLoweringRepair { session_id, event } => {
                    if let Err(error) = record_webui_provider_lowering_repair(
                        &controller_runtime,
                        session_id,
                        event,
                    ) {
                        eprintln!("[webui-service] provider lowering repair failed: {error}");
                    }
                }
                ExecutionStreamEvent::RequestReady { session_id } => {
                    handle_request_phase_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        session_id,
                        active_stream_task_id,
                        stream_task_started_at,
                        RequestPhaseStreamEvent::RequestReady,
                    );
                }
                ExecutionStreamEvent::WireRequestReady { session_id } => {
                    handle_request_phase_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        session_id,
                        active_stream_task_id,
                        stream_task_started_at,
                        RequestPhaseStreamEvent::WireRequestReady,
                    );
                }
                ExecutionStreamEvent::RequestStarted { session_id } => {
                    handle_request_phase_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        session_id,
                        active_stream_task_id,
                        stream_task_started_at,
                        RequestPhaseStreamEvent::RequestStarted,
                    );
                }
                ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                    handle_content_delta_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        &mut record_ids,
                        session_id,
                        StreamContentKind::Thinking,
                        delta,
                    );
                }
                ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                    if let Err(error) = complete_streaming_thinking_phase(
                        &app,
                        &task_state,
                        session_id,
                        &mut record_ids,
                    ) {
                        eprintln!("[webui-service] complete thinking phase failed: {error}");
                    }
                }
                ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                    handle_content_delta_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        &mut record_ids,
                        session_id,
                        StreamContentKind::Output,
                        delta,
                    );
                }
                ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                    handle_runtime_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        &mut request_trace_state,
                        &mut record_ids,
                        &runtime_event_hook_policy,
                        session_id,
                        active_stream_task_id,
                        event,
                    )
                    .await;
                }
                ExecutionStreamEvent::ConversationCheckpoint {
                    session_id,
                    conversation_state,
                    last_successful_provider_usage,
                    ..
                } => handle_conversation_checkpoint_stream_event(
                    &app,
                    &task_state,
                    &controller_runtime,
                    session_id,
                    conversation_state,
                    last_successful_provider_usage,
                ),
                ExecutionStreamEvent::ContextCompactionRequired {
                    session_id,
                    conversation_state,
                    reason,
                } => {
                    if auto_compaction_already_attempted {
                        eprintln!(
                            "[webui-service] repeated mid-turn context compaction rejected: session={session_id} reason={reason}"
                        );
                        let _ = commit_prepared_stream_failure(
                            &app,
                            &task_state,
                            session_id,
                            active_stream_task_id,
                            "mid-turn context compaction retry limit reached",
                        );
                        return;
                    }
                    let Some(preparation) = active_preparation.as_ref() else {
                        let _ = commit_prepared_stream_failure(
                            &app,
                            &task_state,
                            session_id,
                            active_stream_task_id,
                            "mid-turn context compaction preparation missing",
                        );
                        return;
                    };
                    let mut compaction_request = request.clone();
                    compaction_request.conversation_state = conversation_state;
                    let compaction_result =
                        force_auto_compact_webui_session_after_provider_boundary(
                            &app,
                            &task_state,
                            &controller_runtime,
                            &agent_runtime,
                            active_stream_task_id,
                            &compaction_request,
                            preparation,
                        )
                        .await;
                    match compaction_result {
                        Ok(Some(updated_request)) => {
                            webui_trace(
                                "stream_mid_turn_compaction_restarting",
                                format!("session={session_id} reason={reason}"),
                            );
                            let owns_active_stream_entry = task_state
                                .lock()
                                .ok()
                                .and_then(|mut guard| {
                                    remove_active_stream_task_if_id(
                                        &mut guard,
                                        session_id,
                                        active_stream_task_id,
                                    )
                                })
                                .is_some();
                            if !owns_active_stream_entry {
                                return;
                            }
                            spawn_agent_stream_task_with_auto_compaction_state(
                                app.clone(),
                                task_state.clone(),
                                controller_runtime.clone(),
                                agent_runtime.clone(),
                                updated_request,
                                true,
                            );
                            return;
                        }
                        Ok(None) => {
                            let error = String::from(
                                "mid-turn context compaction was required but produced no refreshed request",
                            );
                            eprintln!(
                                "[webui-service] {error}: session={session_id} reason={reason}"
                            );
                            let _ = commit_prepared_stream_failure(
                                &app,
                                &task_state,
                                session_id,
                                active_stream_task_id,
                                "mid-turn context compaction produced no request",
                            );
                            return;
                        }
                        Err(error) => {
                            eprintln!(
                                "[webui-service] mid-turn context compaction failed: session={session_id} reason={reason} error={error}"
                            );
                            let _ = commit_prepared_stream_failure(
                                &app,
                                &task_state,
                                session_id,
                                active_stream_task_id,
                                "mid-turn context compaction failure",
                            );
                            return;
                        }
                    }
                }
                ExecutionStreamEvent::Finished(report) => {
                    // finished_session_id keeps the exact owner available before the report moves into terminal handling.
                    // finished_session_id 在报告移入终态处理前保留精确所属会话。
                    let finished_session_id = report.session_id;
                    if let Err(error) = seal_stream_content_records(
                        &app,
                        &task_state,
                        finished_session_id,
                        &mut record_ids,
                    ) {
                        eprintln!("[webui-service] seal finished content records failed: {error}");
                    }
                    match handle_finished_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        active_stream_task_id,
                        stream_task_started_at,
                        execution_provider_goal_id.as_deref(),
                        report,
                    )
                    .await
                    {
                        FinishedStreamEventOutcome::Handled
                        | FinishedStreamEventOutcome::StaleOwner
                        | FinishedStreamEventOutcome::OwnershipUnavailable => {
                            break StreamTaskExitReason::FinishedEventHandled;
                        }
                    }
                }
            }
        };
        if let Some(event) = provider_telemetry_guard.take_cancelled_event()
            && let Err(error) = process_provider_request_telemetry_event(
                &task_state,
                &controller_runtime,
                Some(&app),
                &mut provider_telemetry_guard,
                event,
                "provider_usage_cancelled",
            )
            .await
        {
            eprintln!("[webui-service] provider cancellation usage state update failed: {error}");
        }
        // Remaining identities belong to non-Finished exits such as exhaustion or stale ownership and must not remain active.
        // 剩余身份属于流耗尽或旧所有权等非 Finished 退出路径，不能继续保持活跃状态。
        if let Err(error) =
            seal_stream_content_records(&app, &task_state, session_id, &mut record_ids)
        {
            eprintln!("[webui-service] seal exiting content records failed: {error}");
        }
        // stream_exit_outcome records the exact ownership decision made after polling stops.
        // stream_exit_outcome 记录轮询停止后作出的精确所有权决策。
        let stream_exit_outcome = handle_stream_task_exit(
            &task_state,
            session_id,
            active_stream_task_id,
            stream_exit_reason,
        );
        webui_trace(
            "stream_task_exit_handled",
            format!(
                "session={session_id} reason={stream_exit_reason:?} outcome={stream_exit_outcome:?}"
            ),
        );
    };
    if let Err(error) = spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        active_stream_task_id,
        session_goal_id,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        task_future,
    ) {
        eprintln!("[webui-service] register stream task failed: {error}");
        match handle_stream_task_registration_failure(
            &registration_failure_app,
            &registration_failure_state,
            session_id,
            expected_kernel_generation,
            &error,
        ) {
            Ok(outcome) => webui_trace(
                "stream_task_registration_failure_handled",
                format!("session={session_id} outcome={outcome:?}"),
            ),
            Err(cleanup_error) => eprintln!(
                "[webui-service] register stream task failure projection failed: {cleanup_error}"
            ),
        }
    }
}

/// Applies one physical Provider telemetry event and releases its optional durable barrier.
/// 应用一条物理 Provider 遥测事件并释放其可选可靠屏障。
///
/// # Parameters
/// 参数
/// The arguments retain exact session Goal ownership, telemetry order, and durable completion state.
/// 参数保留精确的会话目标归属、遥测顺序与可靠完成状态。
///
/// # Returns
/// 返回值
/// Success only when authoritative session accounting and persistence succeed.
/// 仅在权威会话记账与持久化成功时返回成功。
#[allow(clippy::too_many_arguments)]
async fn handle_stream_provider_request_telemetry(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    app: &ServiceEventEmitter,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    event: ProviderRequestTelemetryEvent,
    active_provider_goal_id: &mut Option<String>,
    execution_provider_goal_id: &mut Option<String>,
    active_stream_task_id: Uuid,
    completion: Option<ProviderRequestDurabilityCompletion>,
) -> Result<(), String> {
    match event.kind {
        ProviderRequestTelemetryEventKind::Started => {
            *active_provider_goal_id = match session_goal_id_for_provider_request(
                state,
                event.session_id,
                execution_provider_goal_id.as_deref(),
            ) {
                Ok(goal_id) => {
                    if let Some(goal_id) = goal_id.as_ref() {
                        *execution_provider_goal_id = Some(goal_id.clone());
                        // The retained task binding lets an immediate user stop pause a Goal created during this execution.
                        // 保留任务绑定使用户立即停止时也能暂停当前执行期间创建的目标。
                        match state.lock() {
                            Ok(mut guard) => {
                                let _ = bind_active_stream_task_goal_if_id(
                                    &mut guard,
                                    event.session_id,
                                    active_stream_task_id,
                                    goal_id.clone(),
                                );
                            }
                            Err(error) => {
                                eprintln!(
                                    "[webui-service] provider request goal task binding failed: {error}"
                                );
                            }
                        }
                    }
                    goal_id
                }
                Err(error) => {
                    eprintln!("[webui-service] provider request goal binding failed: {error}");
                    None
                }
            };
        }
        ProviderRequestTelemetryEventKind::Completed => {
            // completedGoalId is consumed exactly once with the physical request terminal event.
            // completedGoalId 随物理请求终态事件恰好消费一次。
            let completed_goal_id = active_provider_goal_id.take();
            if let (Some(goal_id), Some(usage)) =
                (completed_goal_id.as_deref(), event.completed_usage.as_ref())
                && let Err(error) = account_session_goal_provider_usage(
                    state,
                    controller_runtime,
                    event.session_id,
                    goal_id,
                    usage,
                )
                .await
            {
                eprintln!("[webui-service] provider goal usage accounting failed: {error}");
            }
        }
        ProviderRequestTelemetryEventKind::Dispatched
        | ProviderRequestTelemetryEventKind::FirstToken
        | ProviderRequestTelemetryEventKind::Retrying => {}
    }
    let result = process_provider_request_telemetry_event(
        state,
        controller_runtime,
        Some(app),
        provider_telemetry_guard,
        event,
        "provider_usage_telemetry",
    )
    .await;
    if let Some(completion) = completion {
        let _ = completion.complete(result.clone());
    }
    result
}

/// Resolves the non-widening knowledge access snapshot for one provider turn.
/// 为一个 Provider 回合解析不可扩大的知识库访问快照。
///
/// # Parameters
/// 参数
/// `state` owns the current workspace, runtime state, and retained knowledge runtime.
/// `state` 持有当前工作区、运行态与保留的知识库运行时。
/// `session_id` identifies the conversation whose selection must be applied.
/// `session_id` 标识必须应用其选择的会话。
///
/// # Returns
/// 返回值
/// A project-bound access snapshot, possibly with an empty library list, or `None` without a project.
/// 绑定项目且知识库列表可能为空的访问快照；无所属项目时返回 `None`。
async fn resolve_turn_knowledge_access(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Option<ExecutionKnowledgeAccessContext>, String> {
    let (runtime, project_id, selection) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(runtime) = guard.knowledge_runtime.clone() else {
            return Ok(None);
        };
        let Some(workspace) = guard.workspace_snapshot.as_ref() else {
            return Ok(None);
        };
        let Some(project_id) = owning_project_id_for_session(workspace, session_id) else {
            return Ok(None);
        };
        let selection = workspace
            .session_runtime_state
            .get(&session_id)
            .map(|runtime_state| runtime_state.knowledge_library_selection.clone())
            .unwrap_or_default();
        (runtime, project_id, selection)
    };
    let mut library_ids = match selection.mode {
        vulcan_workbench_model::SessionKnowledgeLibrarySelectionMode::Inherit => {
            runtime.authorized_library_ids(&project_id, true).await?
        }
        vulcan_workbench_model::SessionKnowledgeLibrarySelectionMode::Explicit => {
            let authorized: BTreeSet<String> = runtime
                .authorized_library_ids(&project_id, false)
                .await?
                .into_iter()
                .collect();
            selection
                .library_ids
                .into_iter()
                .filter(|library_id| authorized.contains(library_id))
                .collect()
        }
    };
    library_ids.sort();
    library_ids.dedup();
    Ok(Some(ExecutionKnowledgeAccessContext {
        project_id,
        library_ids,
        retrieval_mode: selection.retrieval_mode,
        knowledge_search_handler: Some(build_knowledge_search_handler(state.clone())),
    }))
}
use content_delta::handle_content_delta_stream_event;
