//! Agent runtime stream task entrypoints.
//! Agent 运行时流任务入口。

use super::*;

mod prepared;

use prepared::{PreparedStreamEventAction, handle_prepared_stream_event};

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
    webui_after_precheck_trace(
        "after_precheck_19_stream_task_scheduled",
        format!("session={session_id}"),
    );
    webui_trace("stream_task_spawn", format!("session={session_id}"));
    let expected_kernel_generation = state
        .lock()
        .ok()
        .and_then(|guard| webui_session_kernel_generation(&guard, session_id));
    let task_state = state.clone();
    let handle = spawn_service_task(async move {
        let stream_task_started_at = current_unix_millis_value();
        webui_after_precheck_trace(
            "after_precheck_20_stream_task_entered",
            format!("session={session_id}"),
        );
        webui_trace("stream_task_entered", format!("session={session_id}"));
        let mut record_ids = SessionStreamingRecordIds::default();
        let mut reached_finished_event = false;
        let intervention_started_at = current_unix_millis_value();
        let prompt_ai_tool_configuration = request.ai_tool_configuration.clone();
        let prompt_intervention = build_webui_prompt_intervention(
            app.clone(),
            task_state.clone(),
            agent_runtime.provider_configuration.clone(),
            prompt_ai_tool_configuration,
        );
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
        let mut stream = Box::pin(stream_runtime_run_with_intervention(
            agent_runtime.provider_configuration.clone(),
            agent_runtime.agent_profile_catalog.clone(),
            agent_runtime.luaskills_runtime.clone(),
            agent_runtime.mcp_runtime.clone(),
            Arc::clone(&agent_runtime.task_runtime),
            Arc::clone(&agent_runtime.question_runtime),
            Arc::clone(&agent_runtime.command_session_runtime),
            Arc::clone(&agent_runtime.token_budget_runtime),
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
        // These request-local timers isolate provider latency from UI persistence latency.
        // 这些请求局部计时器用于区分 provider 延迟和界面持久化延迟。
        let mut current_provider_request_started_at: Option<i128> = None;
        let mut first_event_after_request_logged = false;
        let mut first_visible_delta_logged = false;
        while let Some(event) = stream.next().await {
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
                break;
            }
            match event {
                ExecutionStreamEvent::Prepared(preparation) => {
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
                    snapshot,
                } => {
                    if let Err(error) = record_webui_provider_request_checkpoint(
                        &controller_runtime,
                        session_id,
                        &snapshot,
                    ) {
                        eprintln!("[webui-service] provider request checkpoint failed: {error}");
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
                    webui_after_precheck_trace(
                        "after_precheck_30_request_ready",
                        format!(
                            "session={session_id} elapsed_ms_since_task={}",
                            current_unix_millis_value().saturating_sub(stream_task_started_at)
                        ),
                    );
                    webui_trace(
                        "stream_provider_request_ready",
                        format!(
                            "session={session_id} elapsed_ms_since_task={}",
                            current_unix_millis_value().saturating_sub(stream_task_started_at)
                        ),
                    );
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[webui-service] apply request-ready phase failed: {error}");
                    }
                }
                ExecutionStreamEvent::WireRequestReady { session_id } => {
                    webui_after_precheck_trace(
                        "after_precheck_31_wire_request_ready",
                        format!(
                            "session={session_id} elapsed_ms_since_request={} elapsed_ms_since_task={}",
                            current_provider_request_started_at
                                .map(|started_at| current_unix_millis_value()
                                    .saturating_sub(started_at)
                                    .to_string())
                                .unwrap_or_else(|| String::from("<unknown>")),
                            current_unix_millis_value().saturating_sub(stream_task_started_at)
                        ),
                    );
                    webui_trace(
                        "stream_wire_request_ready",
                        format!(
                            "session={session_id} elapsed_ms_since_request={} elapsed_ms_since_task={}",
                            current_provider_request_started_at
                                .map(|started_at| current_unix_millis_value()
                                    .saturating_sub(started_at)
                                    .to_string())
                                .unwrap_or_else(|| String::from("<unknown>")),
                            current_unix_millis_value().saturating_sub(stream_task_started_at)
                        ),
                    );
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[webui-service] apply wire-request-ready phase failed: {error}");
                    }
                }
                ExecutionStreamEvent::RequestStarted { session_id } => {
                    current_provider_request_started_at = Some(current_unix_millis_value());
                    first_event_after_request_logged = false;
                    first_visible_delta_logged = false;
                    webui_after_precheck_trace(
                        "after_precheck_32_request_started",
                        format!(
                            "session={session_id} elapsed_ms_since_task={}",
                            current_unix_millis_value().saturating_sub(stream_task_started_at)
                        ),
                    );
                    webui_trace("stream_request_started", format!("session={session_id}"));
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[webui-service] apply request-started failed: {error}");
                    }
                }
                ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                    if !first_event_after_request_logged {
                        first_event_after_request_logged = true;
                        webui_after_precheck_trace(
                            "after_precheck_33_first_event_after_request",
                            format!(
                                "session={session_id} event=thinking_delta elapsed_ms_since_request={} elapsed_ms_since_task={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                current_unix_millis_value().saturating_sub(stream_task_started_at)
                            ),
                        );
                        webui_trace(
                            "stream_first_event_after_request",
                            format!(
                                "session={session_id} event=thinking_delta elapsed_ms_since_request={} elapsed_ms_since_task={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                current_unix_millis_value().saturating_sub(stream_task_started_at)
                            ),
                        );
                    }
                    if !first_visible_delta_logged {
                        first_visible_delta_logged = true;
                        webui_after_precheck_trace(
                            "after_precheck_34_first_visible_delta",
                            format!(
                                "session={session_id} kind=thinking elapsed_ms_since_request={} chars={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                delta.chars().count()
                            ),
                        );
                        webui_trace(
                            "stream_first_visible_delta",
                            format!(
                                "session={session_id} kind=thinking elapsed_ms_since_request={} chars={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                delta.chars().count()
                            ),
                        );
                    }
                    if let Err(error) = append_stream_delta_record(
                        &app,
                        &task_state,
                        session_id,
                        &mut record_ids.thinking_record_id,
                        SessionMessageRecordKind::ThinkingOutput,
                        "思考",
                        &delta,
                        true,
                    ) {
                        eprintln!("[webui-service] append thinking delta failed: {error}");
                    }
                }
                ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                    if let Err(error) =
                        collapse_streaming_thinking(&app, &task_state, session_id, &record_ids)
                    {
                        eprintln!("[webui-service] complete thinking failed: {error}");
                    }
                    if let Err(error) = apply_session_phase(
                        &app,
                        &task_state,
                        session_id,
                        SessionRunPhase::Requesting,
                    ) {
                        eprintln!("[webui-service] restore request phase failed: {error}");
                    }
                }
                ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                    if !first_event_after_request_logged {
                        first_event_after_request_logged = true;
                        webui_after_precheck_trace(
                            "after_precheck_33_first_event_after_request",
                            format!(
                                "session={session_id} event=output_delta elapsed_ms_since_request={} elapsed_ms_since_task={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                current_unix_millis_value().saturating_sub(stream_task_started_at)
                            ),
                        );
                        webui_trace(
                            "stream_first_event_after_request",
                            format!(
                                "session={session_id} event=output_delta elapsed_ms_since_request={} elapsed_ms_since_task={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                current_unix_millis_value().saturating_sub(stream_task_started_at)
                            ),
                        );
                    }
                    if !first_visible_delta_logged {
                        first_visible_delta_logged = true;
                        webui_after_precheck_trace(
                            "after_precheck_34_first_visible_delta",
                            format!(
                                "session={session_id} kind=output elapsed_ms_since_request={} chars={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                delta.chars().count()
                            ),
                        );
                        webui_trace(
                            "stream_first_visible_delta",
                            format!(
                                "session={session_id} kind=output elapsed_ms_since_request={} chars={}",
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                delta.chars().count()
                            ),
                        );
                    }
                    if let Err(error) = append_stream_delta_record(
                        &app,
                        &task_state,
                        session_id,
                        &mut record_ids.output_record_id,
                        SessionMessageRecordKind::AssistantOutput,
                        "输出",
                        &delta,
                        false,
                    ) {
                        eprintln!("[webui-service] append output delta failed: {error}");
                    }
                }
                ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                    if current_provider_request_started_at.is_some()
                        && !first_event_after_request_logged
                    {
                        first_event_after_request_logged = true;
                        webui_after_precheck_trace(
                            "after_precheck_33_first_event_after_request",
                            format!(
                                "session={session_id} event=runtime.{:?} elapsed_ms_since_request={} elapsed_ms_since_task={}",
                                event.kind,
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                current_unix_millis_value().saturating_sub(stream_task_started_at)
                            ),
                        );
                        webui_trace(
                            "stream_first_event_after_request",
                            format!(
                                "session={session_id} event=runtime.{:?} elapsed_ms_since_request={} elapsed_ms_since_task={}",
                                event.kind,
                                current_provider_request_started_at
                                    .map(|started_at| current_unix_millis_value()
                                        .saturating_sub(started_at)
                                        .to_string())
                                    .unwrap_or_else(|| String::from("<unknown>")),
                                current_unix_millis_value().saturating_sub(stream_task_started_at)
                            ),
                        );
                    }
                    if matches!(
                        event.kind,
                        RuntimeEventKind::ToolStarted
                            | RuntimeEventKind::ToolFinished
                            | RuntimeEventKind::DelegateStarted
                            | RuntimeEventKind::DelegateFinished
                    ) {
                        webui_trace(
                            "stream_runtime_event",
                            format!(
                                "session={session_id} kind={:?} {}",
                                event.kind,
                                runtime_event_trace_detail(&event)
                            ),
                        );
                    }
                    if is_provider_tool_boundary_runtime_event(&event) {
                        record_ids.thinking_record_id = None;
                        record_ids.output_record_id = None;
                    }
                    update_webui_wait_agent_runtime_state(&task_state, session_id, &event);
                    if matches!(event.kind, RuntimeEventKind::ToolStarted)
                        && let Err(error) = apply_session_phase(
                            &app,
                            &task_state,
                            session_id,
                            SessionRunPhase::ToolRunning,
                        )
                    {
                        eprintln!("[webui-service] apply tool phase failed: {error}");
                    }
                    if matches!(event.kind, RuntimeEventKind::ToolFinished)
                        && let Err(error) = apply_session_phase(
                            &app,
                            &task_state,
                            session_id,
                            SessionRunPhase::Requesting,
                        )
                    {
                        eprintln!(
                            "[webui-service] restore request phase after tool failed: {error}"
                        );
                    }
                    let event_is_main_session = task_state.lock().ok().is_some_and(|guard| {
                        guard.workspace_snapshot.as_ref().is_some_and(|workspace| {
                            workspace
                                .sessions
                                .iter()
                                .any(|candidate| candidate.id == session_id)
                        })
                    });
                    if event_is_main_session
                        && let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock()
                    {
                        hook_runtime.record_tool_runtime_event(session_id, &event);
                    }
                    if let Err(error) = maybe_start_webui_detached_subagent_task_listener(
                        app.clone(),
                        task_state.clone(),
                        controller_runtime.clone(),
                        agent_runtime.clone(),
                        session_id,
                        &event,
                    ) {
                        eprintln!(
                            "[webui-service] start detached subagent listener failed: {error}"
                        );
                    }
                    let subagent_event_consumed = match ingest_webui_subagent_runtime_event(
                        &app,
                        &task_state,
                        &agent_runtime,
                        session_id,
                        &event,
                    ) {
                        Ok(consumed) => consumed,
                        Err(error) => {
                            eprintln!("[webui-service] ingest subagent event failed: {error}");
                            false
                        }
                    };
                    if !subagent_event_consumed
                        && let Err(error) =
                            append_runtime_event_projection(&app, &task_state, session_id, &event)
                    {
                        eprintln!("[webui-service] append runtime event failed: {error}");
                    }
                    acknowledge_runtime_event_projection_barrier(&event);
                    let pending_after_runtime_event_started_at = current_unix_millis_value();
                    match maybe_prepare_next_webui_pending_prompt(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        session_id,
                        false,
                    )
                    .await
                    {
                        Ok(started) => {
                            webui_trace_if_slow(
                                "pending_prompt_after_runtime_event_slow",
                                pending_after_runtime_event_started_at,
                                format!(
                                    "session={session_id} kind={:?} started={started}",
                                    event.kind
                                ),
                            );
                            if started {
                                webui_trace(
                                    "pending_prompt_started_after_runtime_event",
                                    format!("session={session_id} kind={:?}", event.kind),
                                );
                            }
                        }
                        Err(error) => {
                            eprintln!(
                                "[webui-service] prepare pending prompt after runtime event failed: {error}"
                            );
                        }
                    }
                }
                ExecutionStreamEvent::ConversationCheckpoint {
                    session_id,
                    conversation_state,
                    last_successful_provider_usage,
                    provider_request_count,
                } => {
                    if let Some(usage) = last_successful_provider_usage.as_ref()
                        && let Err(error) =
                            cache_live_context_budget_observed_usage(&task_state, session_id, usage)
                    {
                        eprintln!(
                            "[webui-service] live context budget usage update failed: {error}"
                        );
                    }
                    if let Err(error) = cache_session_goal_live_usage_from_checkpoint(
                        &task_state,
                        session_id,
                        &conversation_state,
                        provider_request_count,
                        true,
                    ) {
                        eprintln!("[webui-service] session goal live usage update failed: {error}");
                    }
                    if let Err(error) = apply_conversation_checkpoint(
                        &app,
                        &task_state,
                        &controller_runtime,
                        session_id,
                        conversation_state,
                        last_successful_provider_usage,
                        provider_request_count,
                    ) {
                        eprintln!("[webui-service] apply checkpoint failed: {error}");
                    }
                }
                ExecutionStreamEvent::Finished(mut report) => {
                    reached_finished_event = true;
                    let finished_session_id = report.session_id;
                    webui_trace(
                        "stream_finished",
                        format!(
                            "session={finished_session_id} provider_requests={} error={} output_chars={}",
                            report.provider_request_count,
                            report.error.is_some(),
                            report
                                .assistant_output
                                .as_deref()
                                .map(str::len)
                                .unwrap_or(0)
                        ),
                    );
                    if let Err(error) =
                        record_webui_provider_response_checkpoint(&controller_runtime, &report)
                    {
                        eprintln!(
                            "[webui-service] provider response checkpoint write failed: {error}"
                        );
                    }
                    match take_pending_webui_subagent_usage_segments(
                        &task_state,
                        finished_session_id,
                    ) {
                        Ok(mut usage_segments) => {
                            report
                                .embedded_turn_usage_segments
                                .append(&mut usage_segments);
                        }
                        Err(error) => {
                            eprintln!(
                                "[webui-service] pending subagent usage merge failed: {error}"
                            );
                        }
                    }
                    let goal_elapsed_seconds = current_unix_millis_value()
                        .saturating_sub(stream_task_started_at)
                        .checked_div(1_000)
                        .and_then(|value| i64::try_from(value).ok())
                        .unwrap_or_default();
                    if let Err(error) = account_session_goal_from_report(
                        &task_state,
                        &controller_runtime,
                        &report,
                        goal_elapsed_seconds,
                    )
                    .await
                    {
                        eprintln!("[webui-service] session goal accounting failed: {error}");
                    }
                    if let Ok(mut guard) = task_state.lock() {
                        guard.active_stream_tasks.remove(&finished_session_id);
                        finish_webui_session_kernel_turn(
                            &mut guard,
                            finished_session_id,
                            if report.error.is_some() {
                                TurnTerminalStatus::Failed
                            } else {
                                TurnTerminalStatus::Completed
                            },
                        );
                    }
                    let terminal_lifecycle = report
                        .turn_completed_boundary_event(
                            u64::try_from(current_unix_millis_value()).unwrap_or_default(),
                        )
                        .map(|terminal_boundary_event| {
                            let final_boundary_events = report
                                .final_assistant_boundary_event(
                                    u64::try_from(current_unix_millis_value()).unwrap_or_default(),
                                )
                                .into_iter()
                                .collect::<Vec<_>>();
                            let session_title = report
                                .preparation
                                .as_ref()
                                .map(|preparation| preparation.session_title.clone())
                                .unwrap_or_else(|| String::from("Untitled"));
                            (
                                final_boundary_events,
                                terminal_boundary_event,
                                session_title,
                            )
                        });
                    // Measures terminal report projection, including turn stats and final session state.
                    // 衡量终态报告投影耗时，包括统计行与最终会话状态。
                    let final_report_apply_started_at = current_unix_millis_value();
                    let final_report_apply = match apply_execution_report_to_memory(
                        &app,
                        &task_state,
                        &agent_runtime,
                        report,
                    ) {
                        Ok(apply) => Some(apply),
                        Err(error) => {
                            eprintln!("[webui-service] apply final report failed: {error}");
                            None
                        }
                    };
                    webui_trace_if_slow(
                        "stream_final_report_apply_slow",
                        final_report_apply_started_at,
                        format!(
                            "session={finished_session_id} applied={}",
                            final_report_apply.is_some()
                        ),
                    );
                    if let (
                        Some(apply),
                        Some((final_boundary_events, terminal_boundary_event, session_title)),
                    ) = (final_report_apply.as_ref(), terminal_lifecycle)
                        && let Some(project_id) = owning_project_id_for_session(
                            &apply.workspace_snapshot,
                            finished_session_id,
                        )
                    {
                        let controller_runtime_for_lifecycle = controller_runtime.clone();
                        spawn_service_task(async move {
                            let lifecycle_started_at = current_unix_millis_value();
                            if let Err(error) = finish_session_external_turn_lifecycle(
                                &controller_runtime_for_lifecycle,
                                &project_id,
                                finished_session_id,
                                &session_title,
                                final_boundary_events,
                                terminal_boundary_event,
                            ) {
                                eprintln!(
                                    "[webui-service] append session terminal boundary failed: {error}"
                                );
                            }
                            webui_trace_if_slow(
                                "stream_terminal_lifecycle_finish_slow",
                                lifecycle_started_at,
                                format!("session={finished_session_id}"),
                            );
                        });
                    }
                    let started_next_prompt = maybe_start_webui_turn_chain_after_finished(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        finished_session_id,
                    )
                    .await;
                    if let Some(completion_request) = final_report_apply
                        .as_ref()
                        .and_then(|apply| apply.completion_request.clone())
                    {
                        spawn_conversation_completion_hook_task(
                            app.clone(),
                            task_state.clone(),
                            controller_runtime.clone(),
                            agent_runtime.clone(),
                            completion_request,
                        );
                    }
                    if !started_next_prompt && let Some(apply) = final_report_apply {
                        // Measures final session persistence only when no chained turn starts immediately.
                        // 仅在没有立即链式启动下一回合时衡量最终会话持久化耗时。
                        let final_persist_started_at = current_unix_millis_value();
                        if let Err(error) = persist_workspace_session_update(
                            Some(&task_state),
                            controller_runtime.clone(),
                            &apply.workspace_snapshot,
                            finished_session_id,
                        )
                        .await
                        {
                            eprintln!("[webui-service] persist final report failed: {error}");
                        }
                        webui_trace_if_slow(
                            "stream_final_persist_slow",
                            final_persist_started_at,
                            format!("session={finished_session_id}"),
                        );
                    }
                    break;
                }
            }
        }
        if !reached_finished_event && let Ok(mut guard) = task_state.lock() {
            webui_trace(
                "stream_task_ended_without_finished",
                format!("session={session_id}"),
            );
            guard.active_stream_tasks.remove(&session_id);
            finish_webui_session_kernel_turn(&mut guard, session_id, TurnTerminalStatus::Stopped);
        }
    });
    if let Ok(mut guard) = state.lock()
        && let Some(previous_task) = guard.active_stream_tasks.insert(session_id, handle)
    {
        webui_trace(
            "stream_task_abort_previous",
            format!("session={session_id}"),
        );
        previous_task.abort();
    }
}
