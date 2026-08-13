//! Context compaction streaming helpers.
//! 上下文压缩 streaming 辅助逻辑。

use super::*;
use crate::{
    ActiveStreamTaskRegistrationMode, ProviderTelemetryHookGuard, Uuid,
    seal_stream_content_records, spawn_registered_active_stream_task_with_mode,
};

/// Streams one prompt-compaction request and mirrors provider deltas into WebUI state.
/// 流式执行一次提示词压缩请求，并将 Provider 增量镜像到 WebUI 状态。
///
/// # Parameters
/// # 参数
/// `app` emits WebUI workbench updates.
/// `app` 发出 WebUI 工作台更新。
/// `state` owns the retained workspace snapshot.
/// `state` 持有保留的工作区快照。
/// `request` is the ready compaction job request.
/// `request` 是已就绪的压缩任务请求。
/// `session_id` selects the compacted session.
/// `session_id` 选择被压缩的会话。
/// `section_id` identifies the visible compaction status section.
/// `section_id` 标识可见压缩状态分段。
/// `budget_note` describes the triggering context budget.
/// `budget_note` 描述触发压缩的上下文预算。
/// `trigger` labels the manual or automatic compaction source.
/// `trigger` 标记手动或自动压缩来源。
///
/// # Returns
/// # 返回值
/// The terminal compaction execution when a durable summary was produced.
/// 当产出持久摘要时返回终态压缩执行结果。
pub(crate) async fn stream_webui_context_compaction_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    request: PromptCompactionJobRequest,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
) -> Result<PromptCompactionExecution, String> {
    let mut record_ids = SessionStreamingRecordIds::default();
    let mut output_message_id: Option<String> = None;
    let mut terminal_result = None;
    // providerTelemetryGuard closes an active compaction request if the stream future is dropped.
    // providerTelemetryGuard 在压缩流 Future 被丢弃时关闭活动请求。
    let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new_with_persistence(
        state.clone(),
        Some(controller_runtime.clone()),
        Some(app.clone()),
    );
    let mut stream = Box::pin(stream_prompt_compaction_job(request));
    while let Some(event) = stream.next().await {
        match event {
            PromptCompactionStreamEvent::Progress(progress) => {
                if let Err(error) = append_webui_context_compaction_progress(
                    app,
                    state,
                    session_id,
                    section_id,
                    budget_note,
                    trigger,
                    &progress,
                ) {
                    eprintln!(
                        "[webui-service] context compaction progress projection failed: {error}"
                    );
                }
            }
            PromptCompactionStreamEvent::Stream(event) => {
                if let Err(error) = apply_webui_context_compaction_stream_event(
                    app,
                    state,
                    controller_runtime,
                    session_id,
                    section_id,
                    budget_note,
                    trigger,
                    &mut record_ids,
                    &mut output_message_id,
                    &mut provider_telemetry_guard,
                    event,
                )
                .await
                {
                    eprintln!(
                        "[webui-service] context compaction stream projection failed: {error}"
                    );
                }
            }
            PromptCompactionStreamEvent::Finished(result) => {
                terminal_result = Some(result.and_then(|execution| {
                    if execution.fallback_used {
                        return Err(format!(
                            "API 未返回可用压缩结果，本次上下文压缩已失败。Provider 请求：{}。",
                            execution.provider_request_count
                        ));
                    }
                    Ok(execution)
                }));
                break;
            }
        }
    }
    // Every terminal path, including provider failure and premature exhaustion, closes active compaction rows.
    // 每条终止路径（包括 Provider 失败与提前耗尽）都关闭活跃压缩行。
    if let Some(event) = provider_telemetry_guard.take_cancelled_event()
        && let Err(error) = crate::process_provider_request_telemetry_event(
            state,
            controller_runtime,
            Some(app),
            &mut provider_telemetry_guard,
            event,
            "context_compaction_provider_usage_cancelled",
        )
        .await
    {
        eprintln!(
            "[webui-service] context compaction provider cancellation usage update failed: {error}"
        );
    }
    seal_stream_content_records(app, state, session_id, &mut record_ids)?;
    terminal_result
        .unwrap_or_else(|| Err(String::from("模型总结流提前结束，已取消本次提示词压缩。")))
}

/// Spawns the selected-session context-compaction stream and mirrors it into WebUI state.
/// 启动当前会话的上下文压缩流，并将流式事件镜像到 WebUI 状态。
/// `model_target` is the exact route and adaptive tail policy frozen before planning.
/// `model_target` 是规划前冻结的精确路由与自适应尾部策略。
pub(crate) fn spawn_webui_context_compaction_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    status_message_id: String,
    budget_note: String,
    trigger: String,
    plan: CompactionPlan,
    full_context_state: SessionConversationState,
    model_target: PromptCompactionModelTarget,
) -> Result<(), String> {
    let request = build_webui_context_compaction_job_request(
        &state,
        &agent_runtime,
        session_id,
        plan.clone(),
        full_context_state,
        None,
        model_target,
    );
    let task_state = state.clone();
    let task_app = app.clone();
    // registration_failure_controller_runtime persists a formal failure after task-body ownership moves its clone.
    // registration_failure_controller_runtime 在任务体取得其克隆所有权后持久化正式注册失败。
    let registration_failure_controller_runtime = controller_runtime.clone();
    // registration_failure_status_message_id retains the committed section identity for launch compensation.
    // registration_failure_status_message_id 为启动补偿保留已提交分段身份。
    let registration_failure_status_message_id = status_message_id.clone();
    // registration_failure_budget_note retains the exact visible budget diagnosis for failure persistence.
    // registration_failure_budget_note 为失败持久化保留精确可见预算诊断。
    let registration_failure_budget_note = budget_note.clone();
    // registration_failure_trigger retains the authoritative manual or automatic trigger label.
    // registration_failure_trigger 保留权威的手动或自动触发标签。
    let registration_failure_trigger = trigger.clone();
    let active_stream_task_id = Uuid::new_v4();
    // expected_kernel_generation binds registration failure cleanup to this exact compaction turn.
    // expected_kernel_generation 将注册失败清理绑定到当前精确压缩回合。
    let expected_kernel_generation = state
        .lock()
        .ok()
        .and_then(|guard| webui_session_kernel_generation(&guard, session_id));
    let task_future = async move {
        let terminal_result = match request {
            Ok(request) => {
                stream_webui_context_compaction_execution(
                    &task_app,
                    &task_state,
                    &controller_runtime,
                    request,
                    session_id,
                    &status_message_id,
                    &budget_note,
                    &trigger,
                )
                .await
            }
            Err(error) => Err(error),
        };
        // commit_mode keeps ownership validation, domain mutation, task removal, and kernel closure in one transaction.
        // commit_mode 将所有权校验、领域修改、任务移除与 kernel 关闭保留在同一事务中。
        let commit_mode = ContextCompactionCommitMode::FinishActiveStream {
            task_id: active_stream_task_id,
        };
        // finish_result classifies both provider failures and fallback summaries before kernel closure.
        // finish_result 在关闭内核前同时分类 Provider 失败与回退摘要。
        let finish_result = match terminal_result {
            Ok(execution) => finish_webui_context_compaction_execution(
                &task_app,
                &task_state,
                &agent_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                &trigger,
                commit_mode,
                execution,
            ),
            Err(error) => fail_webui_context_compaction_execution(
                &task_app,
                &task_state,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                &trigger,
                commit_mode,
                &error,
            )
            .map(
                |workspace_snapshot| ContextCompactionFinishOutcome::Rejected {
                    workspace_snapshot,
                    error,
                },
            ),
        };
        match finish_result {
            Ok(outcome) => {
                // rejection_error determines the durable reason without discarding its snapshot.
                // rejection_error 在不丢弃快照的前提下决定持久化原因。
                let rejection_error = outcome.rejection_error().map(str::to_owned);
                // snapshot contains either the successful checkpoint or the rejected-attempt record.
                // snapshot 包含成功检查点或被拒绝尝试记录。
                let snapshot = outcome.into_workspace_snapshot();
                if let Some(error) = rejection_error.as_ref() {
                    eprintln!("[webui-service] context compaction rejected: {error}");
                }
                if let Err(error) = persist_context_compaction_snapshot(
                    task_state,
                    controller_runtime,
                    snapshot,
                    session_id,
                    status_message_id.clone(),
                    Some(String::from(if rejection_error.is_some() {
                        "context_compaction_rejected"
                    } else {
                        "context_compaction"
                    })),
                )
                .await
                {
                    eprintln!("[webui-service] context compaction persistence failed: {error}");
                }
            }
            Err(error) => {
                eprintln!("[webui-service] context compaction finish failed: {error}");
            }
        }
    };
    if let Err(error) = spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        active_stream_task_id,
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        task_future,
    ) {
        eprintln!("[webui-service] register context compaction stream task failed: {error}");
        // registration_error_text is committed into the durable failed compaction boundary.
        // registration_error_text 被提交到可持久化的压缩失败边界。
        let registration_error_text = error.to_string();
        // failure_commit combines exact launch ownership, domain failure state, and kernel closure.
        // failure_commit 组合精确启动所有权、领域失败状态与内核关闭。
        let failure_commit = commit_stream_task_registration_failure(
            &state,
            session_id,
            expected_kernel_generation,
            &error,
            |service_state| {
                // workspace owns the Running compaction section installed by the cold commit.
                // workspace 持有冷提交安装的 Running 压缩分段。
                let workspace = service_state
                    .workspace_snapshot
                    .as_mut()
                    .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
                apply_webui_context_compaction_failure_to_workspace(
                    workspace,
                    session_id,
                    previous_phase,
                    &registration_failure_status_message_id,
                    &registration_failure_budget_note,
                    &registration_failure_trigger,
                    &registration_error_text,
                )?;
                // phase_event is generated inside the exact launch-ownership transaction.
                // phase_event 在精确启动所有权事务内部生成。
                apply_session_phase_to_state(service_state, session_id, previous_phase)
            },
            |service_state, phase_event| {
                // workspace_snapshot is projected only after the generic boundary terminalizes the exact kernel generation.
                // workspace_snapshot 仅在通用边界终结精确内核代际后投影。
                let workspace_snapshot = service_state
                    .workspace_snapshot
                    .as_ref()
                    .ok_or_else(|| String::from("workspace snapshot is not ready"))?
                    .clone();
                Ok((workspace_snapshot, phase_event))
            },
        );
        match failure_commit {
            Ok(StreamTaskLaunchFailureCommit::Committed((snapshot, phase_event))) => {
                project_committed_context_compaction_surfaces(
                    &app,
                    &state,
                    session_id,
                    phase_event,
                );
                spawn_context_compaction_persist_task(
                    state.clone(),
                    registration_failure_controller_runtime,
                    snapshot,
                    session_id,
                    registration_failure_status_message_id,
                    Some(String::from("context_compaction_registration_failed")),
                );
                webui_trace(
                    "context_compaction_registration_failure_handled",
                    format!("session={session_id} outcome=domain_failure_committed"),
                );
            }
            Ok(StreamTaskLaunchFailureCommit::Skipped(outcome)) => webui_trace(
                "context_compaction_registration_failure_handled",
                format!("session={session_id} outcome={outcome:?}"),
            ),
            Err(cleanup_error) => eprintln!(
                "[webui-service] context compaction registration failure projection failed: {cleanup_error}"
            ),
        }
        return Err(error.to_string());
    }
    Ok(())
}

/// Applies one prompt-compaction stream event to the retained WebUI transcript.
/// 将一条提示词压缩流事件应用到保留的 WebUI 转录状态。
pub(crate) async fn apply_webui_context_compaction_stream_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    record_ids: &mut SessionStreamingRecordIds,
    output_message_id: &mut Option<String>,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    event: ExecutionStreamEvent,
) -> Result<(), String> {
    match event {
        ExecutionStreamEvent::Prepared(_)
        | ExecutionStreamEvent::ProviderRequestCheckpoint { .. }
        | ExecutionStreamEvent::ProviderLoweringRepair { .. }
        | ExecutionStreamEvent::RequestReady { .. } => Ok(()),
        ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
            crate::process_provider_request_telemetry_event(
                state,
                controller_runtime,
                Some(app),
                provider_telemetry_guard,
                event,
                "context_compaction_provider_usage_telemetry",
            )
            .await
        }
        ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
            crate::process_provider_request_durability_boundary(
                state,
                controller_runtime,
                Some(app),
                provider_telemetry_guard,
                event,
                completion,
                "context_compaction_provider_usage_durability_boundary",
            )
            .await
        }
        ExecutionStreamEvent::WireRequestReady { .. } => Ok(()),
        ExecutionStreamEvent::RequestStarted { .. } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::ThinkingDelta { delta, .. } => append_stream_content_delta(
            app,
            state,
            session_id,
            record_ids,
            StreamContentKind::Thinking,
            &delta,
        ),
        ExecutionStreamEvent::ThinkingCompleted { .. } => {
            complete_streaming_thinking_phase(app, state, session_id, record_ids)
        }
        ExecutionStreamEvent::OutputDelta { delta, .. } => {
            append_webui_context_compaction_output_delta(
                app,
                state,
                session_id,
                section_id,
                budget_note,
                trigger,
                output_message_id,
                &delta,
            )
        }
        ExecutionStreamEvent::RuntimeEvent { event, .. } => {
            append_runtime_event_projection_and_release_barrier(app, state, session_id, &event)?;
            Ok(())
        }
        ExecutionStreamEvent::ConversationCheckpoint { .. } => Ok(()),
        ExecutionStreamEvent::ContextCompactionRequired { .. } => Ok(()),
        ExecutionStreamEvent::Finished(_) => {
            seal_stream_content_records(app, state, session_id, record_ids)
        }
    }
}

/// Returns the retained selected-session title used by compaction diagnostics.
/// 返回压缩诊断使用的保留会话标题。
pub(crate) fn current_session_title_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> String {
    current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten()
        .and_then(|workspace| {
            workspace
                .sessions
                .into_iter()
                .find(|session| session.id == session_id)
                .map(|session| session.title)
        })
        .unwrap_or_else(|| format!("会话 #{session_id}"))
}
