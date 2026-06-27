//! Context compaction orchestration and WebUI projection helpers.
//! 上下文压缩编排与 WebUI 投影辅助逻辑。

#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use futures_util::StreamExt;
use serde_json::{Value, json};
use vulcan_agent_runtime::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionPromptPurpose,
    ExecutionStreamEvent,
    agent_runtime::{
        context_injection_runtime::core_provider_context_injection_from_state_for_turn,
        prompt_compaction::{
            PromptCompactionExecution, PromptCompactionJobRequest, PromptCompactionProgressEvent,
            PromptCompactionStreamEvent, stream_prompt_compaction_job,
        },
    },
    conversation_protocol::{
        SessionConversationState, compact_conversation_records_after_summary,
        compact_conversation_with_summary_until, conversation_state_to_compaction_source_messages,
    },
    provider_config::{
        PersistedAiToolConfiguration, ProviderConfigurationFile, ReasoningEffortLevel,
    },
    session_event_runtime::SessionStreamingRecordIds,
};
use vulcan_context::{
    ContextBudgetSnapshot, SessionConversationMessage, SessionConversationPayload,
    append_record_body_delta,
};
use vulcan_context_compaction::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport, CompactionPlan,
    CompactionPlanInput, plan_context_compaction,
};
use vulcan_context_injection::InjectionPruneReport;
use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::TurnTerminalStatus;
#[cfg(test)]
use vulcan_workbench_model::SessionSummary;
use vulcan_workbench_model::{
    SessionId, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState, SessionTurnStatsKind, SessionTurnStatsStatus,
    SessionTurnUsageSegment, SessionTurnUsageSegmentKind, build_session_record_message_id,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, acknowledge_runtime_event_projection_barrier,
    active_reasoning_selection_effort, append_runtime_event_projection, append_stream_delta_record,
    append_webui_session_message_entry_to_state, append_webui_session_turn_stats_to_state,
    apply_observed_usage_to_context_budget, apply_session_phase, build_vmm_hook_config_snapshot,
    collapse_streaming_thinking, current_unix_millis_text,
    current_workspace_snapshot_from_shared_state, finish_webui_session_kernel_turn,
    historical_context_budget_snapshot_from_runtime_state,
    merge_runtime_state_records_into_display_messages, next_webui_session_persist_version,
    normalized_workspace_path_text_from_path, owning_project_id_for_session,
    persistent_injection_fingerprint, persistent_injection_source_node_ids,
    record_webui_runtime_injection_merged, session_ai_tool_configuration_or_default,
    spawn_service_task, spawn_workspace_session_persist_task,
    sync_hydrated_session_dialogue_ready_marker, webui_session_persist_version_is_current,
    webui_trace, webui_workspace_persist_lock,
    with_workspace_mutation_preserving_subagent_navigation, workspace_path_for_session,
    workspace_root, workspace_snapshot_contains_regular_session,
};

/// Metadata trigger label for user-requested manual context compaction.
/// 用户手动触发上下文压缩的元数据触发标签。
pub(super) const MANUAL_CONTEXT_COMPACTION_TRIGGER: &str = "manual_slash";
/// Metadata trigger label for host-initiated budget auto-compaction.
/// 宿主因预算触发自动上下文压缩的元数据触发标签。
const AUTO_CONTEXT_COMPACTION_TRIGGER: &str = "auto_budget";
/// Maximum consecutive automatic compaction failures before the session circuit breaker opens.
/// 会话自动压缩熔断前允许的最大连续失败次数。
const MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES: usize = 3;
/// Fixed input-token reserve kept free when deciding whether compaction input must be clone-pruned.
/// 判断压缩输入是否必须进行克隆剪枝时保留的固定输入 Token 安全阈值。
const CONTEXT_COMPACTION_INPUT_SAFETY_RESERVE_TOKENS: usize = 30_000;
/// Minimum allowed reserve for compaction-input pruning decisions.
/// 压缩输入剪枝决策允许使用的最低安全阈值。
const MIN_CONTEXT_COMPACTION_INPUT_SAFETY_RESERVE_TOKENS: usize = 20_000;

/// Builds one usage segment from a completed context-compaction execution.
/// 从已完成的上下文压缩执行结果构建一段用量统计。
///
/// # Parameters
/// # 参数
/// `trigger` is the stable compaction trigger label stored in metadata.
/// `trigger` 是写入元数据的稳定压缩触发标签。
/// `execution` is the completed compaction execution carrying provider usage.
/// `execution` 是携带 Provider 用量的已完成压缩执行结果。
///
/// # Returns
/// # 返回值
/// Returns one context-compaction usage segment ready to merge into turn statistics.
/// 返回一段可合并进轮次统计的上下文压缩用量。
fn context_compaction_usage_segment_from_execution(
    trigger: &str,
    execution: &PromptCompactionExecution,
) -> SessionTurnUsageSegment {
    let mut segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::ContextCompaction,
        execution.provider_request_count,
        execution.last_successful_provider_usage.as_ref(),
    );
    segment.trigger = Some(trigger.to_string());
    segment.execution_profile = Some(execution.execution_profile.label().to_string());
    segment.provider_id = execution.provider_id.clone();
    segment.model_id = execution.model_id.clone();
    segment
}

fn spawn_context_compaction_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    section_id: String,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    let persist_version = match next_webui_session_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[webui-service] context-compaction persist version failed: {error}");
            return;
        }
    };
    spawn_service_task(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_persist_version_is_current(&state, session_id, persist_version) {
            return;
        }
        let display_messages = context_compaction_complete_display_messages(
            &workspace_snapshot,
            session_id,
            &section_id,
        );
        if let Err(error) =
            vulcan_workspace_store::workspace_store::persist_single_session_transcript_messages(
                controller_runtime.clone(),
                session_id,
                &display_messages,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("context_compaction");
            eprintln!(
                "[webui-service] background context-compaction transcript persist failed ({reason_label}): {error}"
            );
        }
        let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .or_else(|| {
                workspace_snapshot
                    .archived_session_runtime_state
                    .get(&session_id)
                    .cloned()
            })
        else {
            return;
        };
        if let Err(error) =
            vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
                controller_runtime,
                session_id,
                &runtime_state,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("context_compaction");
            eprintln!(
                "[webui-service] background context-compaction runtime-state persist failed ({reason_label}): {error}"
            );
        }
    });
}

/// Returns the complete visible display window that must survive runtime compaction.
/// 返回必须在运行态压缩后仍可保留的完整可见显示窗口。
fn context_compaction_complete_display_messages(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    section_id: &str,
) -> Vec<SessionMessageEntry> {
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .or_else(|| {
            workspace_snapshot
                .archived_session_messages
                .get(&session_id)
        })
        .cloned()
        .filter(|messages| !messages.is_empty())
        .unwrap_or_else(|| {
            context_compaction_section_display_messages(workspace_snapshot, session_id, section_id)
        })
}

/// Returns the visible display rows that belong to one context-compaction section.
/// 返回归属于单个上下文压缩分段的可见显示行。
fn context_compaction_section_display_messages(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    section_id: &str,
) -> Vec<SessionMessageEntry> {
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .into_iter()
        .flat_map(|messages| messages.iter())
        .filter(|message| context_compaction_message_belongs_to_section(message, section_id))
        .cloned()
        .collect()
}

/// Returns whether one display row belongs to the requested context-compaction section.
/// 返回单条显示行是否归属于请求的上下文压缩分段。
fn context_compaction_message_belongs_to_section(
    message: &SessionMessageEntry,
    section_id: &str,
) -> bool {
    message.id == section_id
        || message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_section_id"))
            .and_then(Value::as_str)
            == Some(section_id)
}

pub(super) async fn maybe_auto_compact_webui_session_before_provider_request(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    request: &ExecutionPreparationRequest,
    preparation: &ExecutionPreparation,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    if request.prompt_purpose != ExecutionPromptPurpose::UserTurn {
        return Ok(None);
    }
    let Some(context_budget) = preparation.context_budget.as_ref() else {
        return Ok(None);
    };
    if !context_budget.should_compact() {
        return Ok(None);
    }
    let session_id = request.session_id;
    let budget_note = context_budget.summary_note();
    webui_trace(
        "stream_auto_compaction_needed",
        format!(
            "session={session_id} severity={:?} usage={}",
            context_budget.severity,
            context_budget.usage_label()
        ),
    );

    let full_context_state = request.conversation_state.clone();
    let context_start_message_id = current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten()
        .and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.context_start_message_id.clone())
        });
    let mut previous_phase = SessionRunPhase::Preparing;
    let mut status_message_id = String::new();
    with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
        previous_phase = workspace
            .session_run_phases
            .get(&session_id)
            .copied()
            .unwrap_or(SessionRunPhase::Preparing);
        status_message_id = append_webui_context_compaction_status_message_to_workspace(
            workspace,
            session_id,
            &budget_note,
            AUTO_CONTEXT_COMPACTION_TRIGGER,
        );
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Preparing);
        if let Some(session) = workspace
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Running;
            session.last_updated_label = String::from("刚刚");
            session.updated_at = current_unix_millis_text();
        }
    })?;

    if webui_auto_context_compaction_failure_count(state, session_id)
        >= MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES
    {
        let error = format!(
            "自动上下文压缩已连续失败 {MAX_CONSECUTIVE_AUTO_COMPACTION_FAILURES} 次，本次请求已停止，避免覆盖或反复改写当前会话状态。"
        );
        persist_failed_webui_context_compaction(
            app,
            state,
            controller_runtime,
            session_id,
            previous_phase,
            &status_message_id,
            &budget_note,
            AUTO_CONTEXT_COMPACTION_TRIGGER,
            &error,
        )?;
        return Err(error);
    }

    let plan = match webui_context_compaction_plan_from_conversation_state(
        session_id,
        &full_context_state,
        context_start_message_id.as_deref(),
        "自动上下文压缩未找到可压缩的历史上下文，已停止本次请求以避免预算溢出。",
    ) {
        Ok(plan) => plan,
        Err(error) => {
            persist_failed_webui_context_compaction(
                app,
                state,
                controller_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &error,
            )?;
            return Err(error);
        }
    };
    let job_request = match build_webui_context_compaction_job_request(
        state,
        agent_runtime,
        session_id,
        plan,
        full_context_state,
    ) {
        Ok(request) => request,
        Err(error) => {
            persist_failed_webui_context_compaction(
                app,
                state,
                controller_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &error,
            )?;
            return Err(error);
        }
    };
    let terminal_result = stream_webui_context_compaction_execution(
        app,
        state,
        job_request,
        session_id,
        &status_message_id,
        &budget_note,
        AUTO_CONTEXT_COMPACTION_TRIGGER,
    )
    .await;
    match terminal_result {
        Ok(execution) => {
            let compaction_usage_segment = context_compaction_usage_segment_from_execution(
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &execution,
            );
            let snapshot = finish_webui_context_compaction_execution(
                app,
                state,
                agent_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                execution,
            )?;
            spawn_context_compaction_persist_task(
                state.clone(),
                controller_runtime.clone(),
                snapshot,
                session_id,
                status_message_id.clone(),
                Some(String::from("auto_context_compaction")),
            );
            let mut refreshed =
                refreshed_execution_request_after_context_compaction(state, request)?;
            refreshed
                .embedded_turn_usage_segments
                .push(compaction_usage_segment);
            Ok(Some(refreshed))
        }
        Err(error) => {
            persist_failed_webui_context_compaction(
                app,
                state,
                controller_runtime,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                AUTO_CONTEXT_COMPACTION_TRIGGER,
                &error,
            )?;
            Err(error)
        }
    }
}

/// Returns the current consecutive automatic compaction failure count for one session.
/// 返回单个会话当前连续自动压缩失败次数。
fn webui_auto_context_compaction_failure_count(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> usize {
    current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten()
        .and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&session_id)
                .map(|runtime_state| runtime_state.auto_compaction_consecutive_failures)
        })
        .unwrap_or_default()
}

/// Builds a compaction plan from a specific pre-status conversation snapshot.
/// 从指定的状态节点插入前会话快照构建压缩计划。
pub(super) fn webui_context_compaction_plan_from_conversation_state(
    session_id: SessionId,
    conversation_state: &SessionConversationState,
    context_start_message_id: Option<&str>,
    empty_error: &str,
) -> Result<CompactionPlan, String> {
    let full_session_messages =
        conversation_state_to_compaction_source_messages(conversation_state);
    if full_session_messages.is_empty() {
        return Err(empty_error.to_string());
    }
    plan_context_compaction(CompactionPlanInput {
        session_label: &session_id.to_string(),
        messages: &full_session_messages,
        all_messages: &full_session_messages,
        context_start_message_id,
        source_until_message_id: None,
        policy: Default::default(),
    })
    .ok_or_else(|| empty_error.to_string())
}

/// Rebuilds an execution request after a successful compaction commit.
/// 在压缩提交成功后重建执行请求。
fn refreshed_execution_request_after_context_compaction(
    state: &SharedServiceBridgeState,
    request: &ExecutionPreparationRequest,
) -> Result<ExecutionPreparationRequest, String> {
    let workspace_snapshot =
        current_workspace_snapshot_from_shared_state(state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&request.session_id)
        .ok_or_else(|| String::from("自动上下文压缩完成后未找到会话运行时状态。"))?;
    let mut refreshed = request.clone();
    refreshed.conversation_state = runtime_state.conversation_state.clone();
    let turn_marker = refreshed
        .boundary_events
        .first()
        .and_then(|event| event.turn_id.clone());
    refreshed.context_injection = core_provider_context_injection_from_state_for_turn(
        &runtime_state.context_injection_state,
        turn_marker.as_deref(),
    );
    Ok(refreshed)
}

/// Persists and projects one failed automatic or manual compaction attempt.
/// 持久化并投影一次失败的自动或手动压缩尝试。
fn persist_failed_webui_context_compaction(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) -> Result<(), String> {
    let snapshot = fail_webui_context_compaction_execution(
        app,
        state,
        session_id,
        previous_phase,
        section_id,
        budget_note,
        trigger,
        error,
    )?;
    spawn_workspace_session_persist_task(
        state.clone(),
        controller_runtime.clone(),
        snapshot,
        session_id,
        Some(String::from("context_compaction_failed")),
    );
    Ok(())
}

fn build_webui_context_compaction_job_request(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    plan: CompactionPlan,
    full_context_state: SessionConversationState,
) -> Result<PromptCompactionJobRequest, String> {
    let mcp_runtime_access = agent_runtime
        .mcp_runtime
        .clone()
        .ok_or_else(|| String::from("上下文压缩运行时未就绪，无法发起模型总结请求。"))?;
    let workspace_snapshot = current_workspace_snapshot_from_shared_state(state)
        .ok()
        .flatten();
    let workspace_path = workspace_snapshot
        .as_ref()
        .map(|workspace| workspace_path_for_session(workspace, session_id))
        .unwrap_or_else(|| normalized_workspace_path_text_from_path(&workspace_root()));
    let ai_tool_configuration = workspace_snapshot
        .as_ref()
        .map(|workspace| {
            session_ai_tool_configuration_or_default(
                workspace,
                session_id,
                &agent_runtime.ai_tool_configuration,
            )
            .clone()
        })
        .unwrap_or_else(|| agent_runtime.ai_tool_configuration.clone());
    let budget_snapshot = context_compaction_budget_snapshot_from_state(
        state,
        session_id,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
    );
    Ok(PromptCompactionJobRequest {
        session_id,
        session_title: current_session_title_from_state(state, session_id),
        workspace_path,
        plan,
        full_context_state,
        input_budget: context_compaction_input_budget_from_snapshot(budget_snapshot.as_ref()),
        observed_input_tokens: context_compaction_observed_input_tokens_from_snapshot(
            budget_snapshot.as_ref(),
        ),
        estimated_input_tokens: context_compaction_estimated_input_tokens_from_snapshot(
            budget_snapshot.as_ref(),
        ),
        provider_configuration: agent_runtime.provider_configuration.clone(),
        model_label: agent_runtime.provider_configuration.model.clone(),
        reasoning_effort: active_reasoning_selection_effort(&agent_runtime.provider_configuration)
            .unwrap_or(ReasoningEffortLevel::Medium),
        ai_tool_configuration,
        agent_profile_catalog: agent_runtime.agent_profile_catalog.clone(),
        luaskills_runtime: agent_runtime.luaskills_runtime.clone(),
        mcp_runtime_access,
        task_runtime: Arc::clone(&agent_runtime.task_runtime),
        question_runtime: Arc::clone(&agent_runtime.question_runtime),
        command_session_runtime: Arc::clone(&agent_runtime.command_session_runtime),
        token_budget_runtime: Arc::clone(&agent_runtime.token_budget_runtime),
    })
}

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
async fn stream_webui_context_compaction_execution(
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
pub(super) fn spawn_webui_context_compaction_task(
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
    let handle = spawn_service_task(async move {
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
        if let Ok(mut guard) = task_state.lock() {
            guard.active_stream_tasks.remove(&session_id);
            finish_webui_session_kernel_turn(&mut guard, session_id, terminal_status);
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
    });
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(previous_task) = guard.active_stream_tasks.insert(session_id, handle) {
        previous_task.abort();
    }
    Ok(())
}

/// Applies one prompt-compaction stream event to the retained WebUI transcript.
/// 将一条提示词压缩流事件应用到保留的 WebUI 转录状态。
fn apply_webui_context_compaction_stream_event(
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
fn current_session_title_from_state(
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

/// Returns the current compact budget note for one compaction status row.
/// 返回单条压缩状态行使用的当前预算摘要。
pub(super) fn context_compaction_budget_note_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> String {
    context_compaction_budget_snapshot_from_state(
        state,
        session_id,
        provider_configuration,
        ai_tool_configuration,
    )
    .map(|budget| budget.summary_note())
    .unwrap_or_else(|| String::from("当前未提供预算快照"))
}

/// Returns the best available budget snapshot for compaction decisions.
/// 返回用于压缩决策的最佳可用预算快照。
///
/// # Parameters
/// # 参数
/// `state` is the shared service bridge state containing live and persisted runtime snapshots.
/// `state` 是包含实时与持久运行态快照的共享服务桥接状态。
/// `session_id` identifies the session whose context budget should be read.
/// `session_id` 标识需要读取上下文预算的会话。
/// `provider_configuration` resolves model catalog limits when only historical usage exists.
/// `provider_configuration` 在只有历史用量时解析模型目录限制。
/// `ai_tool_configuration` resolves effective model capability overrides.
/// `ai_tool_configuration` 解析生效模型能力覆盖。
///
/// # Returns
/// # 返回值
/// Returns the live snapshot enriched with the latest historical provider usage when available.
/// 返回实时快照，并在可用时合并最近一次历史 Provider 用量。
fn context_compaction_budget_snapshot_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ContextBudgetSnapshot> {
    let (live_budget, runtime_state) = state
        .lock()
        .ok()
        .map(|guard| {
            (
                guard.live_context_budgets.get(&session_id).cloned(),
                guard
                    .workspace_snapshot
                    .as_ref()
                    .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                    .cloned(),
            )
        })
        .unwrap_or_default();
    let historical_budget = runtime_state.as_ref().and_then(|runtime_state| {
        historical_context_budget_snapshot_from_runtime_state(
            runtime_state,
            provider_configuration,
            ai_tool_configuration,
        )
    });
    live_budget
        .map(|mut live_budget| {
            if live_budget.observed_usage.is_none() {
                if let Some(usage) = historical_budget
                    .as_ref()
                    .and_then(|budget| budget.observed_usage.as_ref())
                {
                    apply_observed_usage_to_context_budget(&mut live_budget, usage);
                }
            }
            live_budget
        })
        .or(historical_budget)
}

/// Builds the target compaction input budget from one context budget snapshot.
/// 根据上下文预算快照构建目标压缩输入预算。
fn context_compaction_input_budget_from_snapshot(
    budget: Option<&ContextBudgetSnapshot>,
) -> Option<CompactionInputBudget> {
    let safety_reserve_tokens = CONTEXT_COMPACTION_INPUT_SAFETY_RESERVE_TOKENS
        .max(MIN_CONTEXT_COMPACTION_INPUT_SAFETY_RESERVE_TOKENS);
    budget
        .and_then(|budget| budget.context_limit.or(budget.effective_input_limit))
        .map(|context_limit| CompactionInputBudget {
            target_input_budget_tokens: context_limit
                .saturating_sub(safety_reserve_tokens)
                .max(1usize),
            safety_reserve_tokens,
        })
}

/// Returns the latest provider-observed input tokens for compaction downgrade decisions.
/// 返回用于压缩降级判断的最近一次 Provider 实测输入 Token。
fn context_compaction_observed_input_tokens_from_snapshot(
    budget: Option<&ContextBudgetSnapshot>,
) -> Option<usize> {
    budget.and_then(|budget| {
        budget.observed_usage.as_ref().and_then(|usage| {
            vulcan_context::observed_input_tokens_from_usage(Some(usage)).or(usage.total_tokens)
        })
    })
}

/// Returns the latest budget-estimated input tokens when provider feedback is unavailable.
/// 当 Provider 反馈不可用时返回最近一次预算估算输入 Token。
fn context_compaction_estimated_input_tokens_from_snapshot(
    budget: Option<&ContextBudgetSnapshot>,
) -> Option<usize> {
    budget.map(|budget| budget.estimated_input_tokens)
}

/// Immutable baseline captured before a compaction draft is built.
/// 构建压缩候选状态前捕获的不可变基线。
#[derive(Debug, Clone, PartialEq, Eq)]
struct WebuiContextCompactionBaseline {
    /// Number of canonical provider messages in the active state.
    /// 活跃状态中的规范 Provider 消息数量。
    message_count: usize,
    /// Number of canonical transcript records in the active state.
    /// 活跃状态中的规范转录记录数量。
    record_count: usize,
    /// Latest canonical provider message id in the active state.
    /// 活跃状态中最新的规范 Provider 消息标识。
    last_message_id: Option<String>,
    /// Latest canonical transcript record id in the active state.
    /// 活跃状态中最新的规范转录记录标识。
    last_record_id: Option<String>,
    /// Context-start anchor active when the draft was built.
    /// 构建候选状态时生效的上下文起始锚点。
    context_start_message_id: Option<String>,
}

impl WebuiContextCompactionBaseline {
    /// Captures the current active runtime state shape used to validate a later draft commit.
    /// 捕获当前活跃运行时状态形态，用于校验后续候选状态提交。
    ///
    /// # Parameters
    /// # 参数
    /// `runtime_state` is the active session runtime state before draft construction begins.
    /// `runtime_state` 是候选状态构建开始前的活跃会话运行时状态。
    ///
    /// # Returns
    /// # 返回值
    /// A compact baseline containing state shape and anchor identifiers.
    /// 返回包含状态形态与锚点标识的紧凑基线。
    fn capture(runtime_state: &SessionRuntimeStateEntry) -> Self {
        Self {
            message_count: runtime_state.conversation_state.messages.len(),
            record_count: runtime_state.conversation_state.records.len(),
            last_message_id: runtime_state
                .conversation_state
                .messages
                .last()
                .map(|message| message.id.clone()),
            last_record_id: runtime_state
                .conversation_state
                .records
                .last()
                .map(|record| record.id.clone()),
            context_start_message_id: runtime_state.context_start_message_id.clone(),
        }
    }

    /// Returns whether the active runtime state still matches this baseline.
    /// 返回活跃运行时状态是否仍匹配该基线。
    ///
    /// # Parameters
    /// # 参数
    /// `runtime_state` is the active runtime state checked immediately before draft commit.
    /// `runtime_state` 是候选状态提交前立即检查的活跃运行时状态。
    ///
    /// # Returns
    /// # 返回值
    /// `true` when the active state has not changed in ways that would make the draft stale.
    /// 当活跃状态未发生会使候选状态过期的变化时返回 `true`。
    fn matches(&self, runtime_state: &SessionRuntimeStateEntry) -> bool {
        self == &Self::capture(runtime_state)
    }
}

/// Candidate compaction state prepared before it is atomically committed.
/// 原子提交前准备好的候选压缩状态。
struct WebuiContextCompactionDraft {
    /// Baseline that must still match before the draft may replace active state.
    /// 候选状态替换活跃状态前必须仍然匹配的基线。
    baseline: WebuiContextCompactionBaseline,
    /// Fully rewritten runtime state candidate.
    /// 已完整改写的运行时状态候选。
    runtime_state: SessionRuntimeStateEntry,
    /// Checkpoint that should be appended only after the runtime state is accepted.
    /// 仅在运行时状态被接受后追加的检查点。
    checkpoint: vulcan_context::SessionCompactionEntry,
    /// Visible terminal record id that closes the committed compaction section.
    /// 闭合已提交压缩分段的可见终态记录标识。
    terminal_record_id: String,
    /// Optional runtime audit event produced when persistent injections are folded.
    /// 持久注入被折叠时产出的可选运行时审计事件。
    injection_merge_event: Option<vulcan_context::RuntimeInjectionMergedEvent>,
    /// Timestamp used for the optional injection merge audit event.
    /// 可选注入合并审计事件使用的时间戳。
    injection_merge_timestamp: String,
    /// Report describing expired transient injection pruning.
    /// 描述过期临时注入裁剪情况的报告。
    ttl_prune_report: InjectionPruneReport,
}

/// Appends the visible start boundary for one WebUI context-compaction section.
/// 为 WebUI 上下文压缩分段追加可见开始边界。
pub(super) fn append_webui_context_compaction_status_message_to_workspace(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
    budget_note: &str,
    trigger: &str,
) -> String {
    let now = current_unix_millis_text();
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    let entry = SessionMessageEntry {
        id: String::new(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("context_compaction.running"),
        body: String::new(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_state": "running",
            "context_compaction_trigger": trigger,
            "context_compaction_budget_note": budget_note,
        })),
        created_at: String::new(),
    };
    let message_id = append_webui_session_message_entry_to_state(runtime_state, session_id, entry);
    set_webui_context_compaction_section_metadata_in_state(
        runtime_state,
        &message_id,
        &message_id,
        "start",
        "running",
        Some(budget_note),
        trigger,
    );
    let runtime_state_snapshot = runtime_state.clone();
    merge_runtime_state_records_into_display_messages(
        workspace,
        session_id,
        &runtime_state_snapshot,
    );
    sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    message_id
}

/// Appends model-produced summary output into the visible compaction body row.
/// 将模型生成的总结输出追加到可见压缩正文行。
fn append_webui_context_compaction_output_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    output_message_id: &mut Option<String>,
    delta: &str,
) -> Result<(), String> {
    if delta.is_empty() {
        return Ok(());
    }
    with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        let message_id = output_message_id.get_or_insert_with(|| {
            let entry = SessionMessageEntry {
                id: String::new(),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionStatus,
                visible: true,
                title: String::from("context_compaction.summary"),
                body: String::new(),
                metadata: Some(json!({
                    "context_compaction_status": true,
                    "context_compaction_section_id": section_id,
                    "context_compaction_role": "body",
                    "context_compaction_state": "running",
                    "context_compaction_trigger": trigger,
                    "context_compaction_budget_note": budget_note,
                })),
                created_at: String::new(),
            };
            append_webui_session_message_entry_to_state(runtime_state, session_id, entry)
        });
        append_record_body_delta(&mut runtime_state.conversation_state, message_id, delta);
        runtime_state.updated_at = now;
        let runtime_state_snapshot = runtime_state.clone();
        merge_runtime_state_records_into_display_messages(
            workspace,
            session_id,
            &runtime_state_snapshot,
        );
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Requesting);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    })
    .map(|_| ())
}

/// Appends one visible progress line to the running context-compaction status section.
/// 向正在运行的上下文压缩状态分段追加一条可见进度行。
fn append_webui_context_compaction_progress(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    progress: &PromptCompactionProgressEvent,
) -> Result<(), String> {
    let progress_stage = progress.stage.trim();
    if progress_stage.is_empty() {
        return Ok(());
    }
    let line = progress_stage.to_string();
    with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
        let now = current_unix_millis_text();
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: now.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        let existing_progress =
            runtime_state
                .conversation_state
                .records
                .iter_mut()
                .find(|record| {
                    record
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("context_compaction_section_id"))
                        .and_then(Value::as_str)
                        == Some(section_id)
                        && record
                            .metadata
                            .as_ref()
                            .and_then(|metadata| metadata.get("context_compaction_role"))
                            .and_then(Value::as_str)
                            == Some("progress")
                });
        match existing_progress {
            Some(record) => {
                if !record.body.lines().any(|existing| existing == line) {
                    if !record.body.trim().is_empty() {
                        record.body.push('\n');
                    }
                    record.body.push_str(&line);
                }
                if let Some(metadata) = record.metadata.as_mut().and_then(Value::as_object_mut) {
                    metadata.insert(
                        String::from("context_compaction_progress_stage"),
                        json!(progress.stage),
                    );
                    metadata.insert(String::from("context_compaction_state"), json!("running"));
                }
            }
            None => {
                let entry = SessionMessageEntry {
                    id: String::new(),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::CompactionStatus,
                    visible: true,
                    title: String::from("context_compaction.progress"),
                    body: line,
                    metadata: Some(json!({
                        "context_compaction_status": true,
                        "context_compaction_section_id": section_id,
                        "context_compaction_role": "progress",
                        "context_compaction_state": "running",
                        "context_compaction_trigger": trigger,
                        "context_compaction_budget_note": budget_note,
                        "context_compaction_progress_stage": progress.stage,
                    })),
                    created_at: String::new(),
                };
                let message_id =
                    append_webui_session_message_entry_to_state(runtime_state, session_id, entry);
                set_webui_context_compaction_section_metadata_in_state(
                    runtime_state,
                    &message_id,
                    section_id,
                    "progress",
                    "running",
                    Some(budget_note),
                    trigger,
                );
            }
        }
        runtime_state.updated_at = now;
        let runtime_state_snapshot = runtime_state.clone();
        merge_runtime_state_records_into_display_messages(
            workspace,
            session_id,
            &runtime_state_snapshot,
        );
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Requesting);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
    })
    .map(|_| ())
}

/// Maximum user-message characters embedded into the automatic compaction continuation note.
/// 自动压缩续接提示中嵌入的用户消息最大字符数。
const AUTO_COMPACTION_CONTINUATION_LAST_USER_MAX_CHARS: usize = 4_000;

/// Stable prefix for model-visible automatic compaction continuation notes.
/// 模型可见自动压缩续接提示的稳定前缀。
const AUTO_COMPACTION_CONTINUATION_PREFIX: &str =
    "Automatic context compaction was applied while the current task was in progress.";

/// Appends one model-visible continuation note after automatic context compaction.
/// 在自动上下文压缩后追加一条模型可见的续接提示。
fn append_auto_context_compaction_continuation_message(
    draft_runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    created_at: &str,
    active_runtime_state: &SessionRuntimeStateEntry,
) {
    let Some(last_user_message) =
        last_runtime_user_message_text(&active_runtime_state.conversation_state)
    else {
        return;
    };
    let last_user_message = truncate_auto_compaction_last_user_message(&last_user_message);
    let message_id = build_session_record_message_id(
        session_id,
        created_at,
        draft_runtime_state.conversation_state.messages.len(),
    );
    let text = format!(
        "{AUTO_COMPACTION_CONTINUATION_PREFIX}\n\nThis is a synthetic continuation prompt for the resumed provider request, not a new user request. Continue the interrupted work from the retained context immediately. Do not greet, recap the compaction, or ask the user to restate the task. The last real user message before compaction was:\n<last_user_message>\n{last_user_message}\n</last_user_message>"
    );
    draft_runtime_state
        .conversation_state
        .messages
        .push(SessionConversationMessage {
            id: message_id,
            payload: SessionConversationPayload::UserText {
                text,
                attachments: Vec::new(),
            },
        });
}

/// Returns the latest real user-text payload from one runtime conversation state.
/// 返回运行时会话状态中的最后一条真实用户文本载荷。
fn last_runtime_user_message_text(state: &SessionConversationState) -> Option<String> {
    state.messages.iter().rev().find_map(|message| {
        let SessionConversationPayload::UserText { text, .. } = &message.payload else {
            return None;
        };
        let text = text.trim();
        (!text.is_empty() && !text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX))
            .then(|| text.to_string())
    })
}

/// Truncates the embedded last-user message while preserving its leading content.
/// 截断嵌入的最后用户消息，同时保留其开头内容。
fn truncate_auto_compaction_last_user_message(message: &str) -> String {
    let char_count = message.chars().count();
    if char_count <= AUTO_COMPACTION_CONTINUATION_LAST_USER_MAX_CHARS {
        return message.to_string();
    }
    let preserved = message
        .chars()
        .take(AUTO_COMPACTION_CONTINUATION_LAST_USER_MAX_CHARS)
        .collect::<String>();
    format!(
        "{preserved}\n[Last user message truncated after {AUTO_COMPACTION_CONTINUATION_LAST_USER_MAX_CHARS} chars for automatic context-compaction continuation.]"
    )
}

/// Builds one validated WebUI compaction draft without mutating the active runtime state.
/// 在不修改活跃运行时状态的情况下构建一份已校验的 WebUI 压缩候选状态。
///
/// # Parameters
/// # 参数
/// `active_runtime_state` is the current session runtime state used as the immutable draft source.
/// `active_runtime_state` 是作为不可变候选来源使用的当前会话运行时状态。
/// `session_id` identifies the session that will receive the draft if commit validation succeeds.
/// `session_id` 标识在提交校验通过后接收候选状态的会话。
/// `section_id` identifies the visible compaction status section being completed.
/// `section_id` 标识正在完成的可见压缩状态分段。
/// `budget_note` is the human-readable context budget note attached to status records.
/// `budget_note` 是附加到状态记录的人类可读上下文预算说明。
/// `provider_request_count` is the number of provider requests used to produce the summary.
/// `provider_request_count` 是生成摘要时使用的 Provider 请求数量。
/// `summary_body` is the durable summary body already extracted from the model response.
/// `summary_body` 是已从模型响应中提取出的持久摘要正文。
/// `execution` is the terminal prompt-compaction execution result carrying checkpoint metadata.
/// `execution` 是携带检查点元数据的终态提示词压缩执行结果。
///
/// # Returns
/// # 返回值
/// A complete draft that can replace active state atomically, or an error explaining why no commit should occur.
/// 返回可原子替换活跃状态的完整候选状态；或返回说明不应提交的错误。
fn build_webui_context_compaction_draft(
    active_runtime_state: &SessionRuntimeStateEntry,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    provider_request_count: usize,
    summary_body: &str,
    execution: &PromptCompactionExecution,
) -> Result<WebuiContextCompactionDraft, String> {
    if summary_body.trim().is_empty() {
        return Err(String::from("压缩摘要为空，已拒绝提交候选状态。"));
    }
    let baseline = WebuiContextCompactionBaseline::capture(active_runtime_state);
    let mut draft_runtime_state = active_runtime_state.clone();
    let now = current_unix_millis_text();
    ensure_webui_context_compaction_summary_body_for_state(
        &mut draft_runtime_state,
        session_id,
        section_id,
        "completed",
        budget_note,
        trigger,
        summary_body,
    );
    let summary_entry = SessionMessageEntry {
        id: String::new(),
        kind: execution.compaction_result.summary_record.kind,
        record_kind: execution.compaction_result.summary_record.record_kind,
        visible: false,
        title: execution.compaction_result.summary_record.title.clone(),
        body: summary_body.to_string(),
        metadata: None,
        created_at: String::new(),
    };
    let summary_message_id = append_webui_session_message_entry_to_state(
        &mut draft_runtime_state,
        session_id,
        summary_entry,
    );
    let mut checkpoint = execution
        .compaction_result
        .materialize_checkpoint(summary_message_id.clone());
    checkpoint.context_budget_fingerprint =
        draft_runtime_state.live_context_budget_fingerprint.clone();
    checkpoint.context_assembly_fingerprint = draft_runtime_state
        .live_context_assembly_fingerprint
        .clone();
    draft_runtime_state.last_compaction_id = Some(checkpoint.id.clone());
    draft_runtime_state.context_start_message_id = checkpoint.context_start_message_id.clone();
    draft_runtime_state.anchor_context_budget_fingerprint =
        checkpoint.context_budget_fingerprint.clone();
    draft_runtime_state.anchor_context_assembly_fingerprint =
        checkpoint.context_assembly_fingerprint.clone();

    let previous_persistent_fingerprint =
        persistent_injection_fingerprint(&draft_runtime_state.context_injection_state);
    let (mut pruned_injection_state, ttl_prune_report) = draft_runtime_state
        .context_injection_state
        .clone_pruned_for_compaction();
    pruned_injection_state.apply_compaction();
    let merged_baseline_fingerprint = persistent_injection_fingerprint(&pruned_injection_state);
    let source_node_ids = persistent_injection_source_node_ids(&pruned_injection_state);
    draft_runtime_state.context_injection_state = pruned_injection_state;
    let injection_merge_event = if previous_persistent_fingerprint.is_some()
        || merged_baseline_fingerprint.is_some()
        || !source_node_ids.is_empty()
    {
        Some(vulcan_context::RuntimeInjectionMergedEvent {
            turn_id: None,
            merge_id: format!("injection-merge-{session_id}-{}", checkpoint.id),
            compaction_id: Some(checkpoint.id.clone()),
            previous_persistent_fingerprint,
            merged_baseline_fingerprint,
            source_node_ids,
        })
    } else {
        None
    };

    compact_conversation_with_summary_until(
        &mut draft_runtime_state.conversation_state,
        summary_message_id.clone(),
        execution.compaction_result.summary_record.kind,
        summary_body.to_string(),
        checkpoint.created_at.clone(),
        execution
            .compaction_result
            .source_until_message_id
            .as_deref(),
        execution.compaction_result.policy,
    );
    if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
        append_auto_context_compaction_continuation_message(
            &mut draft_runtime_state,
            session_id,
            &checkpoint.created_at,
            active_runtime_state,
        );
    }
    compact_conversation_records_after_summary(
        &mut draft_runtime_state.conversation_state,
        &summary_message_id,
        execution.compaction_result.policy,
    );
    set_webui_context_compaction_section_metadata_in_state(
        &mut draft_runtime_state,
        section_id,
        section_id,
        "start",
        "completed",
        Some(budget_note),
        trigger,
    );
    update_webui_context_compaction_body_state(&mut draft_runtime_state, section_id, "completed");
    let terminal_record_id = append_webui_context_compaction_end_message_to_state(
        &mut draft_runtime_state,
        session_id,
        section_id,
        budget_note,
        trigger,
        provider_request_count,
        execution.input_prune_report.as_ref(),
    );
    if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
        draft_runtime_state.auto_compaction_consecutive_failures = 0;
    }
    draft_runtime_state.updated_at = now.clone();
    Ok(WebuiContextCompactionDraft {
        baseline,
        runtime_state: draft_runtime_state,
        checkpoint,
        terminal_record_id,
        injection_merge_event,
        injection_merge_timestamp: now,
        ttl_prune_report,
    })
}

/// Applies the terminal compaction result, writes its checkpoint, and closes the visible status section.
/// 应用终态压缩结果、写入检查点，并关闭可见状态分段。
fn finish_webui_context_compaction_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    execution: PromptCompactionExecution,
) -> Result<WorkspaceSnapshot, String> {
    if execution.fallback_used {
        let retry_hint = if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
            "自动压缩未完成，已停止本次请求以避免预算溢出。"
        } else {
            "请重新触发 /compact。"
        };
        return fail_webui_context_compaction_execution(
            app,
            state,
            session_id,
            previous_phase,
            section_id,
            budget_note,
            trigger,
            &format!(
                "API 未返回可用压缩结果，本次上下文压缩已失败。Provider 请求：{}。{retry_hint}",
                execution.provider_request_count,
            ),
        );
    }
    let summary_body = execution.compaction_result.summary_record.body.clone();
    let provider_request_count = execution.provider_request_count;
    let runtime_backup_controller = state
        .lock()
        .ok()
        .and_then(|guard| guard.controller_runtime.clone());
    if let Some(controller_runtime) = runtime_backup_controller.as_ref() {
        match vulcan_workspace_store::workspace_store::backup_session_runtime_log_before_compaction(
            &controller_runtime.data_root,
            session_id,
            &execution.compaction_result.compaction_id,
            &execution.compaction_result.created_at,
        ) {
            Ok(copied_files) if !copied_files.is_empty() => {
                webui_trace(
                    "context_compaction_runtime_backup",
                    format!("session={session_id} files={}", copied_files.len()),
                );
            }
            Ok(_) => {}
            Err(error) => {
                webui_trace(
                    "context_compaction_runtime_backup_failed",
                    format!("session={session_id} error={error}"),
                );
            }
        }
    }
    let mut injection_merge_event = None;
    let mut injection_merge_timestamp = String::new();
    let mut ttl_prune_report = InjectionPruneReport::default();
    let mut draft_error: Option<String> = None;
    let workspace =
        with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
            let now = current_unix_millis_text();
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            let draft = match build_webui_context_compaction_draft(
                runtime_state,
                session_id,
                section_id,
                budget_note,
                trigger,
                provider_request_count,
                &summary_body,
                &execution,
            ) {
                Ok(draft) => draft,
                Err(error) => {
                    draft_error = Some(error);
                    return;
                }
            };
            if !draft.baseline.matches(runtime_state) {
                draft_error = Some(String::from(
                    "上下文压缩候选状态基线失配，已保留当前会话状态并取消提交。",
                ));
                return;
            }
            injection_merge_event = draft.injection_merge_event.clone();
            injection_merge_timestamp = draft.injection_merge_timestamp.clone();
            ttl_prune_report = draft.ttl_prune_report;
            let terminal_record_id = draft.terminal_record_id.clone();
            let checkpoint = draft.checkpoint;
            *runtime_state = draft.runtime_state;
            if trigger != AUTO_CONTEXT_COMPACTION_TRIGGER {
                let usage_segment =
                    context_compaction_usage_segment_from_execution(trigger, &execution);
                let _ = append_webui_session_turn_stats_to_state(
                    runtime_state,
                    session_id,
                    &terminal_record_id,
                    SessionTurnStatsStatus::Completed,
                    None,
                    SessionTurnStatsKind::ContextCompaction,
                    vec![usage_segment],
                );
            }
            workspace
                .session_compactions
                .entry(session_id)
                .or_default()
                .push(checkpoint);
            let runtime_state_snapshot = runtime_state.clone();
            merge_runtime_state_records_into_display_messages(
                workspace,
                session_id,
                &runtime_state_snapshot,
            );
            workspace
                .session_run_phases
                .insert(session_id, previous_phase);
            if let Some(session) = workspace
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            {
                session.status = SessionState::Completed;
                session.last_updated_label = String::from("刚刚");
                session.updated_at = current_unix_millis_text();
            }
            sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            agent_runtime
                .vmm_hook_runtime
                .replace_snapshot(build_vmm_hook_config_snapshot(
                    &agent_runtime.provider_configuration,
                    workspace,
                ));
        })?;
    if let Some(error) = draft_error {
        return fail_webui_context_compaction_execution(
            app,
            state,
            session_id,
            previous_phase,
            section_id,
            budget_note,
            trigger,
            &error,
        );
    }
    if ttl_prune_report.removed_item_count > 0 || ttl_prune_report.removed_group_count > 0 {
        webui_trace(
            "context_compaction_pruned_expired_injections",
            format!(
                "session={session_id} items={} groups={}",
                ttl_prune_report.removed_item_count, ttl_prune_report.removed_group_count
            ),
        );
    }
    if let (Some(controller_runtime), Some(event)) =
        (runtime_backup_controller.as_ref(), injection_merge_event)
        && let Err(error) = record_webui_runtime_injection_merged(
            controller_runtime,
            session_id,
            event,
            &injection_merge_timestamp,
        )
    {
        webui_trace(
            "runtime_injection_merged_audit_failed",
            format!("session={session_id} error={error}"),
        );
    }
    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
        hook_runtime.record_compacted(
            session_id,
            owning_project_id_for_session(&workspace, session_id),
            trigger,
            false,
            provider_request_count,
            &summary_body,
        );
    }
    Ok(workspace)
}

/// Records a failed manual context-compaction attempt without moving the conversation anchor.
/// 记录失败的手动上下文压缩尝试，并且不移动会话上下文锚点。
fn fail_webui_context_compaction_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) -> Result<WorkspaceSnapshot, String> {
    with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
        apply_webui_context_compaction_failure_to_workspace(
            workspace,
            session_id,
            previous_phase,
            section_id,
            budget_note,
            trigger,
            error,
        );
    })
}

/// Applies failed context-compaction state inside one retained workspace snapshot.
/// 在单个保留的工作区快照内应用上下文压缩失败状态。
fn apply_webui_context_compaction_failure_to_workspace(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) {
    let now = current_unix_millis_text();
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    set_webui_context_compaction_section_metadata_in_state(
        runtime_state,
        section_id,
        section_id,
        "start",
        "failed",
        Some(budget_note),
        trigger,
    );
    update_webui_context_compaction_body_state(runtime_state, section_id, "failed");
    append_webui_context_compaction_failed_message_to_state(
        runtime_state,
        session_id,
        section_id,
        budget_note,
        trigger,
        error,
    );
    if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
        runtime_state.auto_compaction_consecutive_failures = runtime_state
            .auto_compaction_consecutive_failures
            .saturating_add(1);
    }
    runtime_state.updated_at = now;
    let runtime_state_snapshot = runtime_state.clone();
    merge_runtime_state_records_into_display_messages(
        workspace,
        session_id,
        &runtime_state_snapshot,
    );
    workspace
        .session_run_phases
        .insert(session_id, previous_phase);
    if let Some(session) = workspace
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Completed;
        session.last_updated_label = String::from("刚刚");
        session.updated_at = current_unix_millis_text();
    }
    sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
}

/// Ensures the API-produced compaction summary body is visible when streaming output was empty.
/// 当 API 没有流式输出正文时，确保 API 生成的压缩总结正文可见。
fn ensure_webui_context_compaction_summary_body_for_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    section_id: &str,
    state_label: &str,
    budget_note: &str,
    trigger: &str,
    summary_body: &str,
) {
    let has_body = runtime_state
        .conversation_state
        .records
        .iter()
        .any(|record| {
            record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("context_compaction_section_id"))
                .and_then(Value::as_str)
                == Some(section_id)
                && record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("context_compaction_role"))
                    .and_then(Value::as_str)
                    == Some("body")
                && !record.body.trim().is_empty()
        });
    if has_body {
        return;
    }
    let entry = SessionMessageEntry {
        id: String::new(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("context_compaction.summary"),
        body: summary_body.to_string(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_section_id": section_id,
            "context_compaction_role": "body",
            "context_compaction_state": state_label,
            "context_compaction_trigger": trigger,
            "context_compaction_budget_note": budget_note,
        })),
        created_at: String::new(),
    };
    append_webui_session_message_entry_to_state(runtime_state, session_id, entry);
}

/// Appends the terminal visible boundary for a failed compaction section.
/// 追加压缩失败分段的终止可见边界。
fn append_webui_context_compaction_failed_message_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) {
    let entry = SessionMessageEntry {
        id: String::new(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("context_compaction.failed"),
        body: String::new(),
        metadata: Some(json!({
            "context_compaction_status": true,
            "context_compaction_section_id": section_id,
            "context_compaction_role": "end",
            "context_compaction_state": "failed",
            "context_compaction_trigger": trigger,
            "context_compaction_budget_note": budget_note,
            "context_compaction_error": error,
        })),
        created_at: String::new(),
    };
    append_webui_session_message_entry_to_state(runtime_state, session_id, entry);
}

/// Appends the terminal visible boundary for one compaction section.
/// 追加单个压缩分段的终止可见边界。
///
/// # Parameters
/// # 参数
/// `runtime_state` is the draft runtime state being prepared for commit.
/// `runtime_state` 是正在准备提交的候选运行时状态。
/// `session_id` identifies the session receiving the visible boundary.
/// `session_id` 标识接收可见边界的会话。
/// `section_id` is the visible compaction section identifier.
/// `section_id` 是可见压缩分段标识。
/// `budget_note` is the budget text captured when compaction started.
/// `budget_note` 是压缩启动时捕获的预算文本。
/// `trigger` records whether compaction was manual or automatic.
/// `trigger` 记录压缩是手动触发还是自动触发。
/// `provider_request_count` is the total provider request count across successful and failed attempts.
/// `provider_request_count` 是成功与失败尝试累计的 Provider 请求次数。
/// `input_prune_report` is the optional clone-only pruning report for the successful attempt.
/// `input_prune_report` 是成功尝试对应的可选仅克隆剪枝报告。
///
/// # Returns
/// # 返回值
/// Returns the newly appended terminal record id.
/// 返回新追加的终态记录标识。
fn append_webui_context_compaction_end_message_to_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    session_id: SessionId,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    provider_request_count: usize,
    input_prune_report: Option<&CompactionInputPruneReport>,
) -> String {
    let mut metadata = json!({
        "context_compaction_status": true,
        "context_compaction_section_id": section_id,
        "context_compaction_role": "end",
        "context_compaction_state": "completed",
        "context_compaction_trigger": trigger,
        "context_compaction_budget_note": budget_note,
        "context_compaction_provider_request_count": provider_request_count,
    });
    if let (Some(report), Some(object)) = (input_prune_report, metadata.as_object_mut()) {
        object.insert(
            String::from("context_compaction_input_prune_report"),
            json!({
                "mode": context_compaction_input_prune_mode_label(report.mode),
                "target_input_budget_tokens": report.target_input_budget_tokens,
                "safety_reserve_tokens": report.safety_reserve_tokens,
                "original_token_estimate": report.original_token_estimate,
                "pruned_token_estimate": report.pruned_token_estimate,
                "pruned_tool_output_count": report.pruned_tool_output_count,
                "preserved_tool_output_count": report.preserved_tool_output_count,
                "dropped_source_message_count": report.dropped_source_message_count,
                "truncated_source_message_count": report.truncated_source_message_count,
                "still_exceeds_budget": report.still_exceeds_budget,
            }),
        );
    }
    let entry = SessionMessageEntry {
        id: String::new(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::CompactionStatus,
        visible: true,
        title: String::from("context_compaction.completed"),
        body: context_compaction_end_message_body(input_prune_report),
        metadata: Some(metadata),
        created_at: String::new(),
    };
    append_webui_session_message_entry_to_state(runtime_state, session_id, entry)
}

/// Builds the visible terminal note for one completed context-compaction section.
/// 构建单个已完成上下文压缩分段的可见终端说明。
///
/// # Parameters
/// # 参数
/// `input_prune_report` is the optional clone-only pruning report for the successful compaction input.
/// `input_prune_report` 是成功压缩输入对应的可选仅克隆剪枝报告。
///
/// # Returns
/// # 返回值
/// Returns an empty body when no pruning happened, or a concise visible note when pruning happened.
/// 未发生剪枝时返回空正文，发生剪枝时返回简洁的可见说明。
fn context_compaction_end_message_body(
    _input_prune_report: Option<&CompactionInputPruneReport>,
) -> String {
    String::new()
}

/// Returns the stable metadata label for one context-compaction input pruning mode.
/// 返回上下文压缩输入剪枝模式的稳定元数据标签。
///
/// # Parameters
/// # 参数
/// `mode` is the clone-only pruning mode reported by the compaction runtime.
/// `mode` 是压缩运行时报告的仅克隆剪枝模式。
///
/// # Returns
/// # 返回值
/// Returns a stable snake_case label suitable for metadata and user-facing diagnostics.
/// 返回适用于元数据与用户侧诊断的稳定 snake_case 标签。
fn context_compaction_input_prune_mode_label(mode: CompactionInputPruneMode) -> &'static str {
    match mode {
        CompactionInputPruneMode::None => "none",
        CompactionInputPruneMode::RequiredForWindowDowngrade => "required_for_window_downgrade",
        CompactionInputPruneMode::RetryAfterFailure => "retry_after_failure",
    }
}

/// Updates shared metadata on one compaction section record.
/// 更新单条压缩分段记录上的共享元数据。
fn set_webui_context_compaction_section_metadata_in_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    message_id: &str,
    section_id: &str,
    role: &str,
    state_label: &str,
    budget_note: Option<&str>,
    trigger: &str,
) -> bool {
    let Some(record) = runtime_state
        .conversation_state
        .records
        .iter_mut()
        .find(|record| record.id == message_id)
    else {
        return false;
    };
    let mut metadata = record.metadata.take().unwrap_or_else(|| json!({}));
    if let Some(object) = metadata.as_object_mut() {
        object.insert(String::from("context_compaction_status"), json!(true));
        object.insert(
            String::from("context_compaction_section_id"),
            json!(section_id),
        );
        object.insert(String::from("context_compaction_role"), json!(role));
        object.insert(String::from("context_compaction_state"), json!(state_label));
        object.insert(String::from("context_compaction_trigger"), json!(trigger));
        if let Some(budget_note) = budget_note {
            object.insert(
                String::from("context_compaction_budget_note"),
                json!(budget_note),
            );
        }
    }
    record.metadata = Some(metadata);
    true
}

/// Updates body-row state metadata for all records in one compaction section.
/// 更新单个压缩分段内所有正文行的状态元数据。
fn update_webui_context_compaction_body_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    section_id: &str,
    state_label: &str,
) {
    for record in &mut runtime_state.conversation_state.records {
        let is_body = record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_section_id"))
            .and_then(Value::as_str)
            == Some(section_id)
            && record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("context_compaction_role"))
                .and_then(Value::as_str)
                == Some("body");
        if !is_body {
            continue;
        }
        if let Some(metadata) = record.metadata.as_mut().and_then(Value::as_object_mut) {
            metadata.insert(String::from("context_compaction_state"), json!(state_label));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_agent_runtime::agent_runtime::prompt_compaction::PromptCompactionExecutionProfile;
    use vulcan_agent_runtime::conversation_protocol::conversation_state_from_session_messages;
    use vulcan_context_injection::{InjectionInsert, InjectionKind};

    /// Returns whether text contains CJK characters.
    /// 返回文本是否包含 CJK 字符。
    fn contains_cjk_text(text: &str) -> bool {
        text.chars().any(|character| {
            let codepoint = character as u32;
            (0x4E00..=0x9FFF).contains(&codepoint)
                || (0x3400..=0x4DBF).contains(&codepoint)
                || (0xF900..=0xFAFF).contains(&codepoint)
        })
    }

    /// Removes the variable last-user payload from an automatic compaction continuation prompt.
    /// 从自动压缩续接提示中移除可变的最后用户消息载荷。
    fn auto_compaction_continuation_shell(prompt: &str) -> String {
        let Some((before_user, after_start)) = prompt.split_once("<last_user_message>") else {
            return prompt.to_string();
        };
        let Some((_, after_user)) = after_start.split_once("</last_user_message>") else {
            return prompt.to_string();
        };
        format!("{before_user}<last_user_message></last_user_message>{after_user}")
    }

    /// Verifies compaction draft creation prunes expired transient injections without mutating active state.
    /// 验证压缩候选状态会裁剪过期临时注入且不会修改活跃状态。
    #[test]
    fn webui_context_compaction_draft_prunes_expired_transient_injections_transactionally() {
        let session_id = SessionId::new_v4();
        let messages = vec![
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "请实现上下文压缩")
                .with_runtime_metadata("user-1", "1780000000000"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "已开始实现")
                .with_runtime_metadata("assistant-1", "1780000000001"),
        ];
        let mut active_runtime_state = SessionRuntimeStateEntry {
            conversation_state: conversation_state_from_session_messages(&messages),
            live_context_budget_fingerprint: Some(String::from("budget-live")),
            live_context_assembly_fingerprint: Some(String::from("assembly-live")),
            ..SessionRuntimeStateEntry::default()
        };
        active_runtime_state.context_injection_state.apply_insert(
            InjectionInsert::single(
                InjectionKind::Transient,
                "vmm_inject",
                "Transient memory",
                "expired transient memory",
                Some(1),
            ),
            "turn-1",
        );
        let expired_item = active_runtime_state
            .context_injection_state
            .groups
            .get_mut("vmm_inject")
            .and_then(|group| group.items.values_mut().next())
            .expect("expired transient injection item should exist");
        expired_item.remaining_turns = Some(0);
        let plan = CompactionPlan {
            source_messages: vec![messages[0].clone()],
            source_until_message_id: Some(String::from("user-1")),
            source_until_turn_index: Some(0),
            retained_tail_start_turn_index: Some(1),
            compaction_id: String::from("test-compaction"),
            created_at: String::from("1780000000100"),
            policy: vulcan_context::ContextCompactionPolicy::default(),
        };
        let execution = PromptCompactionExecution {
            compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
                &plan,
                "## Goal\n- 保留上下文压缩升级目标。",
            ),
            fallback_used: false,
            provider_request_count: 1,
            last_successful_provider_usage: None,
            provider_id: None,
            model_id: None,
            execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
            input_prune_report: None,
        };

        let draft = build_webui_context_compaction_draft(
            &active_runtime_state,
            session_id,
            "section-test",
            "预算快照",
            MANUAL_CONTEXT_COMPACTION_TRIGGER,
            1,
            "## Goal\n- 保留上下文压缩升级目标。",
            &execution,
        )
        .expect("draft should be built from a valid summary");

        assert_eq!(draft.ttl_prune_report.removed_item_count, 1);
        assert!(
            active_runtime_state
                .context_injection_state
                .groups
                .contains_key("vmm_inject"),
            "active runtime state must remain untouched"
        );
        assert!(
            draft
                .runtime_state
                .context_injection_state
                .groups
                .get("vmm_inject")
                .is_none(),
            "draft runtime state should remove expired transient injections"
        );
        assert!(active_runtime_state.context_start_message_id.is_none());
        assert!(draft.runtime_state.context_start_message_id.is_some());
        assert_eq!(
            draft.checkpoint.context_budget_fingerprint.as_deref(),
            Some("budget-live")
        );
    }

    /// Verifies completed compaction markers are not anchored back into older visible turns.
    /// 验证已完成压缩标记不会被锚回更早的可见轮次。
    #[test]
    fn webui_context_compaction_draft_keeps_status_markers_unanchored() {
        let session_id = SessionId::new_v4();
        let section_id = String::from("section-test");
        let section = SessionMessageEntry {
            id: section_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::CompactionStatus,
            visible: true,
            title: String::from("上下文正在压缩"),
            body: String::new(),
            metadata: Some(json!({
                "context_compaction_status": true,
                "context_compaction_section_id": section_id.clone(),
                "context_compaction_role": "start",
                "context_compaction_state": "running",
            })),
            created_at: String::from("1780000000002"),
        };
        let messages = vec![
            SessionMessageEntry::new(SessionMessageKind::User, "用户", "压缩前问题")
                .with_runtime_metadata("user-before", "1780000000000"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "压缩前回答")
                .with_runtime_metadata("assistant-before", "1780000000001"),
            section,
            SessionMessageEntry::new(SessionMessageKind::User, "用户", "压缩后继续提问")
                .with_runtime_metadata("user-after", "1780000000003"),
        ];
        let active_runtime_state = SessionRuntimeStateEntry {
            conversation_state: conversation_state_from_session_messages(&messages),
            live_context_budget_fingerprint: Some(String::from("budget-live")),
            live_context_assembly_fingerprint: Some(String::from("assembly-live")),
            ..SessionRuntimeStateEntry::default()
        };
        let plan = CompactionPlan {
            source_messages: vec![messages[0].clone()],
            source_until_message_id: Some(String::from("user-before")),
            source_until_turn_index: Some(0),
            retained_tail_start_turn_index: Some(1),
            compaction_id: String::from("test-compaction-unanchored"),
            created_at: String::from("1780000000100"),
            policy: vulcan_context::ContextCompactionPolicy::default(),
        };
        let execution = PromptCompactionExecution {
            compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
                &plan,
                "## Goal\n- 保留上下文压缩升级目标。",
            ),
            fallback_used: false,
            provider_request_count: 1,
            last_successful_provider_usage: None,
            provider_id: None,
            model_id: None,
            execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
            input_prune_report: None,
        };

        let draft = build_webui_context_compaction_draft(
            &active_runtime_state,
            session_id,
            &section_id,
            "预算快照",
            MANUAL_CONTEXT_COMPACTION_TRIGGER,
            1,
            "## Goal\n- 保留上下文压缩升级目标。",
            &execution,
        )
        .expect("draft should be built from a valid summary");

        let section_records = draft
            .runtime_state
            .conversation_state
            .records
            .iter()
            .filter(|record| {
                record.id == section_id
                    || record
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("context_compaction_section_id"))
                        .and_then(Value::as_str)
                        == Some(section_id.as_str())
            })
            .collect::<Vec<_>>();
        assert!(
            section_records.len() >= 2,
            "completed compaction should keep visible section rows"
        );
        assert!(section_records.iter().all(|record| {
            record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("context_compaction_insert_before_message_id"))
                .is_none()
        }));
    }

    /// Verifies only automatic compaction appends one runtime-only continuation note.
    /// 验证只有自动压缩会追加一条仅存在于运行态的续接提示。
    #[test]
    fn webui_context_compaction_draft_appends_auto_continuation_only_for_auto_trigger() {
        let session_id = SessionId::new_v4();
        let messages = vec![
            SessionMessageEntry::new(SessionMessageKind::User, "User", "Old question")
                .with_runtime_metadata("user-old", "1780000000000"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "Assistant", "Old answer")
                .with_runtime_metadata("assistant-old", "1780000000001"),
            SessionMessageEntry::new(
                SessionMessageKind::User,
                "User",
                "Please continue the real task",
            )
            .with_runtime_metadata("user-latest", "1780000000002"),
        ];
        let active_runtime_state = SessionRuntimeStateEntry {
            conversation_state: conversation_state_from_session_messages(&messages),
            live_context_budget_fingerprint: Some(String::from("budget-live")),
            live_context_assembly_fingerprint: Some(String::from("assembly-live")),
            ..SessionRuntimeStateEntry::default()
        };
        let plan = CompactionPlan {
            source_messages: vec![messages[0].clone()],
            source_until_message_id: Some(String::from("user-old")),
            source_until_turn_index: Some(0),
            retained_tail_start_turn_index: Some(1),
            compaction_id: String::from("test-auto-continuation"),
            created_at: String::from("1780000000100"),
            policy: vulcan_context::ContextCompactionPolicy::default(),
        };
        let manual_execution = PromptCompactionExecution {
            compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
                &plan,
                "## Goal\n- Keep automatic compaction continuation semantics.",
            ),
            fallback_used: false,
            provider_request_count: 1,
            last_successful_provider_usage: None,
            provider_id: None,
            model_id: None,
            execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
            input_prune_report: None,
        };
        let manual_draft = build_webui_context_compaction_draft(
            &active_runtime_state,
            session_id,
            "section-manual",
            "Budget snapshot",
            MANUAL_CONTEXT_COMPACTION_TRIGGER,
            1,
            "## Goal\n- Keep automatic compaction continuation semantics.",
            &manual_execution,
        )
        .expect("manual draft should be built from a valid summary");
        assert!(
            !manual_draft
                .runtime_state
                .conversation_state
                .messages
                .iter()
                .any(|message| matches!(
                    &message.payload,
                    SessionConversationPayload::UserText { text, .. }
                        if text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX)
                )),
            "manual compaction must not append the automatic continuation note"
        );

        let auto_execution = PromptCompactionExecution {
            compaction_result: vulcan_context_compaction::CompactionResult::from_model_summary(
                &plan,
                "## Goal\n- Keep automatic compaction continuation semantics.",
            ),
            fallback_used: false,
            provider_request_count: 1,
            last_successful_provider_usage: None,
            provider_id: None,
            model_id: None,
            execution_profile: PromptCompactionExecutionProfile::CachePreservingFullFidelity,
            input_prune_report: None,
        };
        let auto_draft = build_webui_context_compaction_draft(
            &active_runtime_state,
            session_id,
            "section-auto",
            "Budget snapshot",
            AUTO_CONTEXT_COMPACTION_TRIGGER,
            1,
            "## Goal\n- Keep automatic compaction continuation semantics.",
            &auto_execution,
        )
        .expect("automatic draft should be built from a valid summary");
        let continuation_messages = auto_draft
            .runtime_state
            .conversation_state
            .messages
            .iter()
            .filter_map(|message| {
                let SessionConversationPayload::UserText { text, .. } = &message.payload else {
                    return None;
                };
                text.starts_with(AUTO_COMPACTION_CONTINUATION_PREFIX)
                    .then_some(text.as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(continuation_messages.len(), 1);
        assert!(continuation_messages[0].contains("synthetic continuation prompt"));
        assert!(continuation_messages[0].contains("resumed provider request"));
        assert!(continuation_messages[0].contains("Do not greet"));
        assert!(continuation_messages[0].contains("Please continue the real task"));
        assert!(
            !contains_cjk_text(continuation_messages[0]),
            "model-visible automatic compaction continuation message must remain fixed English in this host-generated test path"
        );
        assert!(
            !contains_cjk_text(&auto_compaction_continuation_shell(
                continuation_messages[0]
            )),
            "model-visible automatic compaction continuation shell must remain fixed English"
        );
        assert!(
            auto_draft
                .runtime_state
                .conversation_state
                .records
                .iter()
                .all(|record| !record.body.contains(AUTO_COMPACTION_CONTINUATION_PREFIX)),
            "automatic continuation is model-visible runtime state, not a visible transcript record"
        );
    }

    /// Verifies repeated automatic compaction ignores previous continuation notes when finding the last user prompt.
    /// 验证重复自动压缩查找最后用户提示时会忽略旧续接提示。
    #[test]
    fn last_runtime_user_message_text_skips_previous_auto_continuation() {
        let state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("real-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("真正的最后用户请求"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("old-auto-continuation"),
                    payload: SessionConversationPayload::UserText {
                        text: format!("{AUTO_COMPACTION_CONTINUATION_PREFIX} stale note"),
                        attachments: Vec::new(),
                    },
                },
            ],
            ..SessionConversationState::default()
        };

        assert_eq!(
            last_runtime_user_message_text(&state).as_deref(),
            Some("真正的最后用户请求")
        );
    }

    /// Verifies context compaction persistence keeps the complete visible transcript window.
    /// 验证上下文压缩持久化会保留完整可见转录窗口。
    #[test]
    fn context_compaction_complete_display_messages_preserve_visible_history() {
        let session_id = SessionId::new_v4();
        let section_id = String::from("compaction-section-1");
        let mut user_entry =
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "循环调用工具");
        user_entry.id = String::from("user-1");
        let mut tool_entry =
            SessionMessageEntry::new(SessionMessageKind::Assistant, "", "已执行 sleep 5");
        tool_entry.id = String::from("tool-1");
        tool_entry.record_kind = SessionMessageRecordKind::ToolCallOutput;
        let mut interrupt_entry =
            SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "不用继续了");
        interrupt_entry.id = String::from("interrupt-1");
        interrupt_entry.metadata = Some(json!({
            "source": "user_interrupt",
            "starts_user_turn": false,
            "turn_process": true,
            "editable": false,
        }));
        let mut compaction_entry =
            SessionMessageEntry::new(SessionMessageKind::Assistant, "上下文已压缩", "");
        compaction_entry.id = section_id.clone();
        compaction_entry.record_kind = SessionMessageRecordKind::CompactionStatus;
        let workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::from([(
                session_id,
                vec![
                    user_entry.clone(),
                    tool_entry.clone(),
                    interrupt_entry.clone(),
                    compaction_entry.clone(),
                ],
            )]),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::from([session_id]),
        };

        let messages = context_compaction_complete_display_messages(
            &workspace_snapshot,
            session_id,
            &section_id,
        );

        assert_eq!(messages.len(), 4);
        assert!(messages.iter().any(|message| message.id == tool_entry.id));
        assert!(
            messages
                .iter()
                .any(|message| message.id == interrupt_entry.id)
        );
        assert!(
            messages
                .iter()
                .any(|message| message.id == compaction_entry.id)
        );
    }

    /// Verifies compaction terminal records preserve clone-prune report metadata.
    /// 验证压缩终态记录会保留克隆剪枝报告元数据。
    #[test]
    fn append_webui_context_compaction_end_message_preserves_input_prune_report() {
        let session_id = SessionId::new_v4();
        let mut runtime_state = SessionRuntimeStateEntry::default();
        let prune_report = CompactionInputPruneReport {
            mode: CompactionInputPruneMode::RequiredForWindowDowngrade,
            target_input_budget_tokens: 1024,
            safety_reserve_tokens: 128,
            original_token_estimate: 4096,
            pruned_token_estimate: 900,
            pruned_tool_output_count: 2,
            preserved_tool_output_count: 3,
            dropped_source_message_count: 4,
            truncated_source_message_count: 5,
            still_exceeds_budget: false,
        };

        let record_id = append_webui_context_compaction_end_message_to_state(
            &mut runtime_state,
            session_id,
            "section-a",
            "预算说明",
            AUTO_CONTEXT_COMPACTION_TRIGGER,
            7,
            Some(&prune_report),
        );

        let record = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.id == record_id)
            .expect("terminal record should be appended");
        let metadata = record
            .metadata
            .as_ref()
            .expect("terminal record should carry metadata");
        assert_eq!(
            metadata
                .get("context_compaction_input_prune_report")
                .and_then(|value| value.get("mode"))
                .and_then(Value::as_str),
            Some("required_for_window_downgrade")
        );
        assert_eq!(
            metadata
                .get("context_compaction_input_prune_report")
                .and_then(|value| value.get("pruned_tool_output_count"))
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            metadata
                .get("context_compaction_provider_request_count")
                .and_then(Value::as_u64),
            Some(7)
        );
        assert_eq!(
            metadata
                .get("context_compaction_trigger")
                .and_then(Value::as_str),
            Some(AUTO_CONTEXT_COMPACTION_TRIGGER)
        );
    }

    /// Verifies failed auto-compaction records failure metadata and circuit-breaker progress.
    /// 验证自动压缩失败会记录失败元数据并推进熔断计数。
    #[test]
    fn fail_webui_context_compaction_execution_records_auto_failure_metadata() {
        let session_id = SessionId::new_v4();
        let mut runtime_state = SessionRuntimeStateEntry {
            auto_compaction_consecutive_failures: 2,
            ..SessionRuntimeStateEntry::default()
        };
        let mut workspace_snapshot = WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "压缩测试会话",
                "",
                SessionState::Running,
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
            session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Preparing)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        };
        let section_id = append_webui_context_compaction_status_message_to_workspace(
            &mut workspace_snapshot,
            session_id,
            "预算说明",
            AUTO_CONTEXT_COMPACTION_TRIGGER,
        );
        runtime_state.conversation_state = workspace_snapshot
            .session_runtime_state
            .remove(&session_id)
            .expect("status append should create runtime state")
            .conversation_state;
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
        apply_webui_context_compaction_failure_to_workspace(
            &mut workspace_snapshot,
            session_id,
            SessionRunPhase::Ready,
            &section_id,
            "预算说明",
            AUTO_CONTEXT_COMPACTION_TRIGGER,
            "测试失败",
        );

        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("runtime state should remain present");
        assert_eq!(runtime_state.auto_compaction_consecutive_failures, 3);
        assert_eq!(
            workspace_snapshot.session_run_phases.get(&session_id),
            Some(&SessionRunPhase::Ready)
        );
        assert!(
            workspace_snapshot.sessions.iter().any(
                |session| session.id == session_id && session.status == SessionState::Completed
            )
        );
        let failed_record = runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| {
                record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("context_compaction_role"))
                    .and_then(Value::as_str)
                    == Some("end")
            })
            .expect("failed terminal record should be appended");
        let metadata = failed_record
            .metadata
            .as_ref()
            .expect("failed terminal record should carry metadata");
        assert_eq!(
            metadata
                .get("context_compaction_state")
                .and_then(Value::as_str),
            Some("failed")
        );
        assert_eq!(
            metadata
                .get("context_compaction_trigger")
                .and_then(Value::as_str),
            Some(AUTO_CONTEXT_COMPACTION_TRIGGER)
        );
        assert_eq!(
            metadata
                .get("context_compaction_error")
                .and_then(Value::as_str),
            Some("测试失败")
        );
    }
}
