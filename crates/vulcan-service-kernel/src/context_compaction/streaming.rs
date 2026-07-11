//! Context compaction streaming helpers.
//! 上下文压缩 streaming 辅助逻辑。

use super::*;
use crate::{Uuid, remove_active_stream_task_if_id, spawn_registered_active_stream_task};

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
    request: PromptCompactionJobRequest,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
) -> Result<PromptCompactionExecution, String> {
    let mut record_ids = SessionStreamingRecordIds::default();
    let mut output_message_id: Option<String> = None;
    let mut terminal_result = None;
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
                    session_id,
                    section_id,
                    budget_note,
                    trigger,
                    &mut record_ids,
                    &mut output_message_id,
                    event,
                ) {
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
    terminal_result
        .unwrap_or_else(|| Err(String::from("模型总结流提前结束，已取消本次提示词压缩。")))
}

/// Spawns the selected-session context-compaction stream and mirrors it into WebUI state.
/// 启动当前会话的上下文压缩流，并将流式事件镜像到 WebUI 状态。
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
) -> Result<(), String> {
    let request = build_webui_context_compaction_job_request(
        &state,
        &agent_runtime,
        session_id,
        plan.clone(),
        full_context_state,
    );
    let task_state = state.clone();
    let task_app = app.clone();
    let active_stream_task_id = Uuid::new_v4();
    let task_future = async move {
        let terminal_result = match request {
            Ok(request) => {
                stream_webui_context_compaction_execution(
                    &task_app,
                    &task_state,
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
        let terminal_status = if terminal_result.is_ok() {
            TurnTerminalStatus::Completed
        } else {
            TurnTerminalStatus::Failed
        };
        let owns_active_stream_entry = match task_state.lock() {
            Ok(mut guard) => {
                let owns_entry =
                    remove_active_stream_task_if_id(&mut guard, session_id, active_stream_task_id)
                        .is_some();
                if owns_entry {
                    finish_webui_session_kernel_turn(&mut guard, session_id, terminal_status);
                }
                owns_entry
            }
            Err(_) => false,
        };
        if !owns_active_stream_entry {
            return;
        }
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
                &error,
            ),
        };
        match finish_result {
            Ok(snapshot) => spawn_context_compaction_persist_task(
                task_state,
                controller_runtime,
                snapshot,
                session_id,
                status_message_id.clone(),
                Some(String::from("context_compaction")),
            ),
            Err(error) => {
                eprintln!("[webui-service] context compaction finish failed: {error}");
                let _ = apply_session_phase(
                    &task_app,
                    &task_state,
                    session_id,
                    SessionRunPhase::Failed,
                );
            }
        }
    };
    spawn_registered_active_stream_task(
        &state,
        session_id,
        active_stream_task_id,
        None,
        task_future,
    )?;
    Ok(())
}

/// Applies one prompt-compaction stream event to the retained WebUI transcript.
/// 将一条提示词压缩流事件应用到保留的 WebUI 转录状态。
pub(crate) fn apply_webui_context_compaction_stream_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    record_ids: &mut SessionStreamingRecordIds,
    output_message_id: &mut Option<String>,
    event: ExecutionStreamEvent,
) -> Result<(), String> {
    match event {
        ExecutionStreamEvent::Prepared(_)
        | ExecutionStreamEvent::ProviderRequestCheckpoint { .. }
        | ExecutionStreamEvent::ProviderLoweringRepair { .. }
        | ExecutionStreamEvent::RequestReady { .. } => Ok(()),
        ExecutionStreamEvent::WireRequestReady { .. } => Ok(()),
        ExecutionStreamEvent::RequestStarted { .. } => {
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
        }
        ExecutionStreamEvent::ThinkingDelta { delta, .. } => append_stream_delta_record(
            app,
            state,
            session_id,
            &mut record_ids.thinking_record_id,
            SessionMessageRecordKind::ThinkingOutput,
            "思考",
            &delta,
            true,
        ),
        ExecutionStreamEvent::ThinkingCompleted { .. } => {
            collapse_streaming_thinking(app, state, session_id, record_ids)?;
            apply_session_phase(app, state, session_id, SessionRunPhase::Requesting)
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
            append_runtime_event_projection(app, state, session_id, &event)?;
            acknowledge_runtime_event_projection_barrier(&event);
            Ok(())
        }
        ExecutionStreamEvent::ConversationCheckpoint { .. } => Ok(()),
        ExecutionStreamEvent::Finished(_) => Ok(()),
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
