//! WebUI conversation prompt submission orchestration.
//! WebUI 会话提示词提交编排逻辑。

use std::collections::BTreeMap;

use uuid::Uuid;
use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptTurnBeginInput, begin_shared_prompt_turn,
};
use vulcan_service_api::{
    ComposerAttachmentSummary, ConversationSummary, ConversationSummaryChangedDelta, ServiceEvent,
    ServiceEventEmitter, SubmitConversationPromptAck, UpdateConversationRunPhaseDelta,
    WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
};

use super::{
    SharedServiceBridgeState, UserPromptEditSubmitRequest, active_kernel_session_ids,
    apply_webui_prompt_begin_outer_steps_deferred_context_persist,
    build_conversation_messages_patched_delta, build_vmm_hook_config_snapshot,
    capture_preserved_workspace_run_state, composer_attachment_summary_to_core,
    conversation_attention_summary_for_session, conversation_fork_source_summary_from_session,
    conversation_messages_from_session, current_unix_millis_text, current_unix_millis_value,
    current_workspace_snapshot_from_shared_state, effective_workbench_disabled_ai_capability_kinds,
    emit_materialized_session_message_delta, emit_pending_prompts_changed_delta,
    emit_service_event_with_trace, enqueue_webui_pending_prompt_from_submission,
    ensure_session_ai_tool_configuration, ensure_session_can_enter_plan_mode,
    ensure_webui_session_runtime_state, hydrate_retained_submission_session_if_needed,
    is_active_session_run_phase, load_workspace_for_submission,
    mark_webui_prompt_snapshot_preparing, materialize_prompt_session,
    parse_ai_capability_target_order_overrides, parse_ai_capability_target_selectors,
    parse_reasoning_effort, parse_session_id, pending_question_snapshots_from_agent_runtime,
    preserve_active_session_run_state, replace_conversation_messages_in_workbench_window,
    retained_submission_snapshot_has_session_context, retained_submission_snapshot_is_usable,
    rollback_persisted_session_before_user_prompt, runtime_parts_from_state,
    session_ai_tool_configuration_or_default, session_has_blocking_system_prompt_work,
    session_plan_mode_summary_from_runtime_state, snapshot_webui_context_before_user_prompt,
    spawn_service_task, spawn_workspace_session_persist_task, start_webui_precheck_or_continue,
    start_webui_session_kernel_user_turn, store_workspace_snapshot_without_workbench_rebuild,
    sync_hydrated_session_dialogue_ready_marker, webui_session_allows_user_prompt_editing,
    webui_session_has_active_run, webui_session_kernel_has_active_turn,
    webui_shared_prompt_precheck_plan, webui_submit_trace, webui_trace_if_slow,
    workbench_with_live_pending_question, workspace_path_for_session,
    workspace_user_prompt_message_allows_destructive_edit,
};
mod edit;

pub(super) use edit::*;

/// Submits a WebUI user prompt into a new or existing conversation.
/// 将 WebUI 用户提示词提交到新会话或已有会话。
///
/// # Parameters
/// 参数
/// `app` emits workbench updates during submission preparation and precheck dispatch.
/// `app` 在提交准备与 precheck 分发期间发送工作台更新。
/// `state` owns service runtime handles and the retained workspace snapshot.
/// `state` 持有服务运行时句柄与保留工作区快照。
/// `project_id` selects the target project when creating a new conversation.
/// `project_id` 在创建新会话时选择目标项目。
/// `conversation_id` selects an existing conversation when present.
/// `conversation_id` 在存在时选择已有会话。
/// `prompt` is the trimmed user prompt body.
/// `prompt` 是裁剪后的用户提示词正文。
/// `hidden_model_guidance` optionally adds hidden runtime guidance.
/// `hidden_model_guidance` 可选追加隐藏运行时引导。
/// `execution_mode` chooses build or plan execution mode.
/// `execution_mode` 选择 build 或 plan 执行模式。
/// `reasoning_effort` optionally overrides model reasoning effort for this turn.
/// `reasoning_effort` 可选覆盖本回合模型推理强度。
/// `attachments` contains composer attachment summaries to stage into the turn.
/// `attachments` 包含要加入本回合的输入区附件摘要。
/// `disabled_ai_capability_kinds` disables selected AI capability kinds for this turn.
/// `disabled_ai_capability_kinds` 禁用本回合选定 AI 能力类型。
/// `disabled_ai_capability_target_selectors` disables selected AI capability targets for this turn.
/// `disabled_ai_capability_target_selectors` 禁用本回合选定 AI 能力目标。
/// `ai_capability_target_order_overrides` overrides selected AI capability target ordering.
/// `ai_capability_target_order_overrides` 覆盖选定 AI 能力目标排序。
///
/// # Returns
/// 返回值
/// Returns a lightweight acknowledgement after queuing or starting the submitted prompt.
/// 返回排队或启动提交提示词后的轻量确认信息。
pub(super) async fn service_submit_conversation_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    project_id: Option<String>,
    conversation_id: Option<String>,
    prompt: String,
    hidden_model_guidance: Option<String>,
    execution_mode: Option<String>,
    reasoning_effort: Option<String>,
    attachments: Option<Vec<ComposerAttachmentSummary>>,
    disabled_ai_capability_kinds: Option<Vec<String>>,
    disabled_ai_capability_target_selectors: Option<Vec<String>>,
    ai_capability_target_order_overrides: Option<BTreeMap<String, Vec<String>>>,
) -> Result<SubmitConversationPromptAck, String> {
    // Measures the whole submit RPC body from service entry to final response.
    // 衡量提交 RPC 主体从服务入口到最终返回的整体耗时。
    let submit_total_started_at = current_unix_millis_value();
    let prompt = prompt.trim().to_string();
    webui_submit_trace(
        "submit_prompt_received",
        format!(
            "conversation_id={} project_id={} prompt_chars={} attachments={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            project_id.as_deref().unwrap_or("<none>"),
            prompt.chars().count(),
            attachments.as_ref().map(Vec::len).unwrap_or(0)
        ),
    );
    let attachments = attachments
        .unwrap_or_default()
        .into_iter()
        .map(composer_attachment_summary_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    if prompt.is_empty() && attachments.is_empty() {
        return Err(String::from("请输入任务指令。"));
    }

    // Measures runtime handle acquisition before workspace hydration begins.
    // 衡量工作区水合开始前获取运行时句柄的耗时。
    let runtime_parts_started_at = current_unix_millis_value();
    let (
        controller_runtime,
        agent_runtime,
        _luaskills_catalog,
        _token_budget_runtime,
        _mcp_catalog,
        _logs,
    ) = runtime_parts_from_state(&state)?;
    webui_submit_trace(
        "submit_prompt_runtime_parts_ready",
        format!(
            "conversation_id={} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(runtime_parts_started_at)
        ),
    );
    let submission_session_id = conversation_id.as_deref().and_then(parse_session_id);
    // Measures retained workspace reuse without cloning the full workbench snapshot on the hot path.
    // 衡量保留工作区复用，并避免在热路径克隆完整工作台快照。
    let retained_snapshot_started_at = current_unix_millis_value();
    let (
        preserved_workspace_run_state,
        retained_dialogue_context_available,
        retained_workspace_snapshot,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let retained_dialogue_context_available = submission_session_id.is_some_and(|session_id| {
            guard
                .workspace_snapshot
                .as_ref()
                .is_some_and(|workspace_snapshot| {
                    retained_submission_snapshot_has_session_context(workspace_snapshot, session_id)
                })
        });
        // Reuses the retained workspace on the submit hot path before falling back to full disk reload.
        // 在发送热路径优先复用保留工作区，再按需回退到完整磁盘重载。
        let retained_workspace_snapshot =
            guard
                .workspace_snapshot
                .as_ref()
                .and_then(|workspace_snapshot| {
                    retained_submission_snapshot_is_usable(
                        workspace_snapshot,
                        project_id.as_deref(),
                        submission_session_id,
                    )
                    .then(|| workspace_snapshot.clone())
                });
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            submission_session_id,
        );
        (
            preserved_workspace_run_state,
            retained_dialogue_context_available,
            retained_workspace_snapshot,
        )
    };
    webui_trace_if_slow(
        "submit_prompt_retained_snapshot_slow",
        retained_snapshot_started_at,
        format!(
            "conversation_id={} retained_dialogue_context_available={retained_dialogue_context_available} preserved_sessions={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            preserved_workspace_run_state
                .as_ref()
                .map(|state| state.session_count())
                .unwrap_or_default()
        ),
    );
    let workspace_load_started_at = current_unix_millis_value();
    let (mut workspace_snapshot, workspace_source) = if let Some(workspace_snapshot) =
        retained_workspace_snapshot
    {
        (workspace_snapshot, "retained")
    } else {
        (
            load_workspace_for_submission(controller_runtime.clone(), conversation_id.as_deref())
                .await?,
            "disk",
        )
    };
    if let Some(session_id) = submission_session_id {
        hydrate_retained_submission_session_if_needed(
            controller_runtime.clone(),
            &mut workspace_snapshot,
            session_id,
        )
        .await?;
    }
    webui_submit_trace(
        "submit_prompt_workspace_ready",
        format!(
            "conversation_id={} retained_dialogue_context_available={retained_dialogue_context_available} source={workspace_source} elapsed_ms={}",
            conversation_id.as_deref().unwrap_or("<draft>"),
            current_unix_millis_value().saturating_sub(workspace_load_started_at)
        ),
    );
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut workspace_snapshot,
    );
    if let Some(session_id) = conversation_id.as_deref().and_then(parse_session_id) {
        let blocking_check_started_at = current_unix_millis_value();
        // Measures the active stream and kernel state lookup portion of the blocking gate.
        // 衡量阻塞闸门中活跃流与内核状态查询部分的耗时。
        let active_state_started_at = current_unix_millis_value();
        let (has_active_stream_task, has_active_kernel_turn, has_active_session_run) = {
            let guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            (
                guard.active_stream_tasks.contains_key(&session_id),
                webui_session_kernel_has_active_turn(&guard, session_id),
                webui_session_has_active_run(&guard, session_id),
            )
        };
        webui_trace_if_slow(
            "submit_prompt_active_state_check_slow",
            active_state_started_at,
            format!("session={session_id}"),
        );
        let has_running_subagents = false;
        webui_submit_trace(
            "submit_prompt_subagent_running_check_skipped",
            format!("session={session_id} reason=independent_subagent_lane"),
        );
        webui_submit_trace(
            "submit_prompt_blocking_check",
            format!(
                "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_active_session_run={has_active_session_run} has_running_subagents={has_running_subagents} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(blocking_check_started_at)
            ),
        );
        let has_precheck_progress = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .and_then(|runtime_state| runtime_state.active_precheck_turn_id.as_deref())
            .is_some_and(|turn_id| !turn_id.trim().is_empty());
        if session_has_blocking_system_prompt_work(
            &workspace_snapshot,
            &agent_runtime.provider_configuration,
            session_id,
            has_active_session_run,
            has_precheck_progress,
        ) {
            webui_submit_trace(
                "submit_prompt_queued",
                format!(
                    "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_running_subagents={has_running_subagents}"
                ),
            );
            let workspace_path = workspace_path_for_session(&workspace_snapshot, session_id);
            enqueue_webui_pending_prompt_from_submission(
                &mut workspace_snapshot,
                &agent_runtime,
                session_id,
                &workspace_path,
                prompt,
                hidden_model_guidance.clone(),
                attachments,
                reasoning_effort.as_deref(),
                disabled_ai_capability_kinds.as_deref(),
            )?;
            let run_phase = workspace_snapshot
                .session_run_phases
                .get(&session_id)
                .copied()
                .unwrap_or(SessionRunPhase::Idle);
            store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot.clone())?;
            let _pending_revision = emit_pending_prompts_changed_delta(&app, &state, session_id)?;
            let revision = emit_queued_prompt_run_phase_delta(&app, &state, session_id, run_phase)?;
            let state_for_persist = state.clone();
            let controller_runtime_for_persist = controller_runtime.clone();
            spawn_service_task(async move {
                let snapshot =
                    match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                        Ok(Some(snapshot)) => snapshot,
                        Ok(None) => return,
                        Err(error) => {
                            eprintln!(
                                "[webui-service] queued submit persist snapshot failed: {error}"
                            );
                            return;
                        }
                    };
                spawn_workspace_session_persist_task(
                    state_for_persist,
                    controller_runtime_for_persist,
                    snapshot,
                    session_id,
                    Some(String::from("queued_submit")),
                );
            });
            webui_submit_trace(
                "submit_prompt_queued_returning",
                format!(
                    "session={session_id} elapsed_ms_since_received={}",
                    current_unix_millis_value().saturating_sub(submit_total_started_at)
                ),
            );
            return Ok(SubmitConversationPromptAck {
                conversation_id: session_id.to_string(),
                accepted_message_id: None,
                turn_id: None,
                run_phase: run_phase.to_string(),
                queued: true,
                revision,
            });
        }
    }
    let materialize_started_at = current_unix_millis_value();
    let (session_id, session_title, workspace_path) = materialize_prompt_session(
        &mut workspace_snapshot,
        project_id.as_deref(),
        conversation_id.as_deref(),
        if prompt.is_empty() {
            "附件会话"
        } else {
            &prompt
        },
    )?;
    webui_submit_trace(
        "submit_prompt_materialize_ready",
        format!(
            "session={session_id} title_chars={} workspace_path_chars={} elapsed_ms={}",
            session_title.chars().count(),
            workspace_path.chars().count(),
            current_unix_millis_value().saturating_sub(materialize_started_at)
        ),
    );
    ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None);
    ensure_session_ai_tool_configuration(
        &mut workspace_snapshot,
        session_id,
        &agent_runtime.ai_tool_configuration,
    );
    emit_submitted_conversation_summary_delta(&app, &state, &workspace_snapshot, session_id)?;
    if conversation_id.is_some() {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.session_subagent_navigation_rounds.remove(&session_id);
        if guard
            .active_subagent_panel
            .is_some_and(|selection| selection.parent_session_id == session_id)
        {
            guard.active_subagent_panel = None;
        }
    }
    webui_submit_trace(
        "submit_prompt_starting_run",
        format!(
            "session={session_id} title_chars={}",
            session_title.chars().count()
        ),
    );
    let submit_run_prepare_started_at = current_unix_millis_value();
    let session_ai_tool_configuration = session_ai_tool_configuration_or_default(
        &workspace_snapshot,
        session_id,
        &agent_runtime.ai_tool_configuration,
    );
    let disabled_ai_capability_kinds = effective_workbench_disabled_ai_capability_kinds(
        session_ai_tool_configuration,
        disabled_ai_capability_kinds.as_deref(),
    )?;
    let disabled_ai_capability_target_selectors =
        parse_ai_capability_target_selectors(disabled_ai_capability_target_selectors);
    let ai_capability_target_order_overrides =
        parse_ai_capability_target_order_overrides(ai_capability_target_order_overrides)?;
    let effective_execution_mode = execution_mode.unwrap_or_else(|| String::from("build"));
    if effective_execution_mode.eq_ignore_ascii_case("plan") {
        ensure_session_can_enter_plan_mode(&controller_runtime, session_id).await?;
        let updated_at = current_unix_millis_text();
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: updated_at.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        runtime_state.active_work_mode = Some(String::from("plan"));
        runtime_state.updated_at = updated_at;
    }
    webui_submit_trace(
        "submit_prompt_runtime_options_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(submit_run_prepare_started_at)
        ),
    );
    let context_snapshot_started_at = current_unix_millis_value();
    snapshot_webui_context_before_user_prompt(
        &controller_runtime,
        &workspace_snapshot,
        session_id,
        PendingSessionPromptSource::UserFollowup,
    )?;
    webui_submit_trace(
        "submit_prompt_context_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(context_snapshot_started_at)
        ),
    );
    let turn_begin_started_at = current_unix_millis_value();
    let precheck_start = {
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            SharedPromptTurnBeginInput {
                session_id,
                session_title: &session_title,
                workspace_path: &workspace_path,
                prompt_entry_id: format!(
                    "webui-prompt-{session_id}-{}",
                    current_unix_millis_text()
                ),
                prompt: &prompt,
                hidden_model_guidance: hidden_model_guidance.as_deref(),
                attachments,
                append_canonical_user_message: true,
                append_visible_user_prompt_record: true,
                source: PendingSessionPromptSource::UserFollowup,
                execution_mode: effective_execution_mode,
                reasoning_effort: reasoning_effort.as_deref().and_then(parse_reasoning_effort),
                disabled_ai_capability_kinds,
                disabled_ai_capability_target_selectors,
                ai_capability_target_order_overrides,
            },
        )
    }?;
    let accepted_message_id = precheck_start.visible_user_prompt_record_id.clone();
    let turn_id = Some(precheck_start.precheck_request.turn_id().to_string());
    webui_submit_trace(
        "submit_prompt_turn_begin_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(turn_begin_started_at)
        ),
    );
    // Resolves the precheck decision before the first visible delta chooses its phase.
    // 在首个可见增量选择阶段前解析 precheck 决策。
    let precheck_plan_started_at = current_unix_millis_value();
    let precheck_plan = webui_shared_prompt_precheck_plan(
        &agent_runtime,
        &workspace_snapshot,
        session_id,
        &workspace_path,
    )?;
    webui_submit_trace(
        "submit_prompt_precheck_plan_ready",
        format!(
            "session={session_id} has_hooks={} skip={} labels={} elapsed_ms={}",
            precheck_plan.has_precheck_hooks,
            precheck_plan.should_skip_precheck,
            precheck_plan.progress_labels.len(),
            current_unix_millis_value().saturating_sub(precheck_plan_started_at)
        ),
    );
    // Keeps skipped precheck turns in the preparatory phase until provider execution starts.
    // 在 precheck 跳过时保持准备阶段，直到模型执行真正开始。
    let submit_visible_run_phase =
        if precheck_plan.should_skip_precheck || !precheck_plan.has_precheck_hooks {
            SessionRunPhase::Preparing
        } else {
            SessionRunPhase::MemoryRecall
        };
    if let Some(message_id) = precheck_start.visible_user_prompt_record_id.as_deref()
        && let Some(message) = workspace_snapshot
            .session_messages
            .get(&session_id)
            .and_then(|messages| messages.iter().find(|message| message.id == message_id))
    {
        let delta_started_at = current_unix_millis_value();
        emit_materialized_session_message_delta(
            &app,
            &state,
            session_id,
            message,
            submit_visible_run_phase,
        )?;
        webui_submit_trace(
            "submit_prompt_user_message_delta_ready",
            format!(
                "session={session_id} message_id={message_id} elapsed_ms={} elapsed_ms_since_received={}",
                current_unix_millis_value().saturating_sub(delta_started_at),
                current_unix_millis_value().saturating_sub(submit_total_started_at)
            ),
        );
        tokio::task::yield_now().await;
        webui_submit_trace(
            "submit_prompt_user_message_delta_yielded",
            format!(
                "session={session_id} message_id={message_id} elapsed_ms_since_received={}",
                current_unix_millis_value().saturating_sub(submit_total_started_at)
            ),
        );
    }
    {
        let kernel_turn_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        start_webui_session_kernel_user_turn(
            &mut guard,
            session_id,
            precheck_start.precheck_request.turn_id(),
        )?;
        webui_submit_trace(
            "submit_prompt_kernel_turn_started",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(kernel_turn_started_at)
            ),
        );
    }
    mark_webui_prompt_snapshot_preparing(&mut workspace_snapshot, session_id);
    let begin_commit_started_at = current_unix_millis_value();
    store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot.clone())?;
    webui_submit_trace(
        "submit_prompt_begin_workspace_committed",
        format!(
            "session={session_id} elapsed_ms={} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(begin_commit_started_at),
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    webui_submit_trace(
        "submit_prompt_visible_user_message_emitted",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    let outer_steps_started_at = current_unix_millis_value();
    apply_webui_prompt_begin_outer_steps_deferred_context_persist(
        &controller_runtime,
        &mut workspace_snapshot,
        session_id,
        &session_title,
    );
    webui_submit_trace(
        "submit_prompt_outer_steps_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(outer_steps_started_at)
        ),
    );
    let hook_snapshot_started_at = current_unix_millis_value();
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    webui_submit_trace(
        "submit_prompt_hook_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
        ),
    );
    let precheck_workspace_commit_started_at = current_unix_millis_value();
    store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot)?;
    webui_submit_trace(
        "submit_prompt_precheck_workspace_committed",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(precheck_workspace_commit_started_at)
        ),
    );
    let state_for_persist = state.clone();
    let controller_runtime_for_persist = controller_runtime.clone();
    spawn_service_task(async move {
        let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return,
            Err(error) => {
                eprintln!("[webui-service] submit begin persist snapshot failed: {error}");
                return;
            }
        };
        spawn_workspace_session_persist_task(
            state_for_persist,
            controller_runtime_for_persist,
            snapshot,
            session_id,
            Some(String::from("submit_begin")),
        );
    });
    let service_state = state.clone();
    let precheck_dispatch_started_at = current_unix_millis_value();
    let precheck_started = start_webui_precheck_or_continue(
        &app,
        &service_state,
        &controller_runtime,
        &agent_runtime,
        precheck_start,
        precheck_plan,
    )?;
    let revision = current_workbench_revision_from_state(&state)?;
    webui_submit_trace(
        "submit_prompt_precheck_dispatch_returned",
        format!(
            "session={session_id} started={precheck_started} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(precheck_dispatch_started_at)
        ),
    );
    webui_submit_trace(
        "submit_prompt_returning",
        format!(
            "session={session_id} elapsed_ms_since_received={}",
            current_unix_millis_value().saturating_sub(submit_total_started_at)
        ),
    );
    Ok(SubmitConversationPromptAck {
        conversation_id: session_id.to_string(),
        accepted_message_id,
        turn_id,
        run_phase: submit_visible_run_phase.to_string(),
        queued: false,
        revision,
    })
}

/// Emits the submitted conversation sidebar summary before transcript deltas target the surface.
/// 在转录增量指向界面前发送本次提交会话的侧栏摘要。
fn emit_submitted_conversation_summary_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    workspace_snapshot: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    session_id: vulcan_workbench_model::SessionId,
) -> Result<u64, String> {
    let session = workspace_snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session_id)
        .ok_or_else(|| format!("submitted session was not materialized: {session_id}"))?;
    let run_phase = workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .copied()
        .unwrap_or(SessionRunPhase::Idle);
    let is_running =
        session.status == SessionState::Running || is_active_session_run_phase(run_phase);
    let unread = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(|runtime_state| runtime_state.ui_state.unread);
    let session_plan_mode = session_plan_mode_summary_from_runtime_state(
        workspace_snapshot.session_runtime_state.get(&session_id),
    );
    let mut conversation = ConversationSummary {
        id: session_id.to_string(),
        title: session.title.clone(),
        status: if is_running {
            SessionState::Running.label()
        } else {
            session.status.label()
        }
        .to_string(),
        run_phase: run_phase.to_string(),
        last_updated_label: session.last_updated_label.clone(),
        last_updated_at: session.updated_at.clone(),
        unread,
        attention: None,
        project_id: session.project_id.clone(),
        fork_source: conversation_fork_source_summary_from_session(session),
    };
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref());
        conversation.attention = conversation_attention_summary_for_session(
            session_id,
            &pending_questions,
            session_plan_mode.as_ref(),
            unread,
            Some(session.updated_at.clone()),
        );
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let revision = guard.workbench_generation;
        guard.workbench.revision = revision;
        guard.workbench.selected_conversation_id = Some(conversation.id.clone());
        if let Some(existing) = guard
            .workbench
            .conversations
            .iter_mut()
            .find(|candidate| candidate.id == conversation.id)
        {
            *existing = conversation.clone();
        } else {
            guard
                .workbench
                .conversations
                .insert(0, conversation.clone());
        }
        WorkbenchDeltaEnvelope {
            generation: revision,
            delta: WorkbenchDelta::ConversationSummaryChanged(ConversationSummaryChangedDelta {
                conversation,
                selected_conversation_id: Some(session_id.to_string()),
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_conversation_summary_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    current_workbench_revision_from_state(state)
}

/// Reads the latest accepted workbench revision from shared service state.
/// 从共享服务状态读取最新已接受的工作台版本号。
fn current_workbench_revision_from_state(state: &SharedServiceBridgeState) -> Result<u64, String> {
    state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))
        .map(|guard| guard.workbench_generation)
}

/// Emits the queued-prompt run phase as an ordered workbench delta.
/// 将排队提示词对应的运行阶段作为有序工作台增量发出。
fn emit_queued_prompt_run_phase_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: Uuid,
    run_phase: SessionRunPhase,
) -> Result<u64, String> {
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let revision = guard.workbench_generation;
        guard.workbench.revision = revision;
        let conversation_id = session_id.to_string();
        let run_phase_text = run_phase.to_string();
        let mut status = None;
        let mut last_updated_label = None;
        if let Some(conversation) = guard
            .workbench
            .conversations
            .iter_mut()
            .find(|candidate| candidate.id == conversation_id)
        {
            conversation.run_phase = run_phase_text.clone();
            status = Some(conversation.status.clone());
            last_updated_label = Some(conversation.last_updated_label.clone());
        }
        WorkbenchDeltaEnvelope {
            generation: revision,
            delta: WorkbenchDelta::UpdateConversationRunPhase(UpdateConversationRunPhaseDelta {
                conversation_id,
                run_phase: run_phase_text,
                status,
                last_updated_label,
            }),
        }
    };
    let generation = event.generation;
    emit_service_event_with_trace(
        app,
        "workbench_queued_prompt_run_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(generation)
}
