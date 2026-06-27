//! WebUI conversation prompt submission orchestration.
//! WebUI 会话提示词提交编排逻辑。

use std::collections::BTreeMap;

use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptTurnBeginInput, begin_shared_prompt_turn,
};
use vulcan_service_api::{ComposerAttachmentSummary, ServiceEventEmitter, WorkbenchSnapshot};
use vulcan_workbench_model::{
    PendingSessionPromptSource, SessionRunPhase, SessionRuntimeStateEntry, SessionState,
};

use super::{
    SharedServiceBridgeState, UserPromptEditSubmitRequest, active_kernel_session_ids,
    apply_webui_prompt_begin_outer_steps, build_vmm_hook_config_snapshot,
    capture_preserved_workspace_run_state, composer_attachment_summary_to_core,
    conversation_messages_from_session, current_unix_millis_text, current_unix_millis_value,
    current_workbench_snapshot_from_state, current_workspace_snapshot_from_shared_state,
    effective_workbench_disabled_ai_capability_kinds, enqueue_webui_pending_prompt_from_submission,
    ensure_session_ai_tool_configuration, ensure_session_can_enter_plan_mode,
    ensure_webui_session_runtime_state, hydrate_retained_submission_session_if_needed,
    load_workspace_for_submission, mark_webui_prompt_snapshot_preparing,
    materialize_prompt_session, parse_ai_capability_target_order_overrides,
    parse_ai_capability_target_selectors, parse_reasoning_effort, parse_session_id,
    preserve_active_session_run_state, retained_submission_snapshot_has_session_context,
    retained_submission_snapshot_is_usable, rollback_persisted_session_before_user_prompt,
    runtime_parts_from_state, session_ai_tool_configuration_or_default,
    session_has_blocking_system_prompt_work, snapshot_webui_context_before_user_prompt,
    spawn_service_task, spawn_workspace_session_persist_task, start_webui_precheck_or_continue,
    start_webui_session_kernel_user_turn, store_and_emit_workbench_snapshot,
    store_and_emit_workbench_snapshot_preserving_subagent_navigation,
    sync_hydrated_session_dialogue_ready_marker, webui_session_allows_user_prompt_editing,
    webui_session_has_active_run, webui_session_kernel_has_active_turn,
    webui_shared_prompt_precheck_plan, webui_trace, webui_trace_if_slow,
    workbench_with_live_pending_question, workspace_path_for_session,
    workspace_user_prompt_message_allows_destructive_edit,
};

/// Applies a user prompt edit and resubmits the edited prompt into the conversation.
/// 应用用户提示词编辑，并将编辑后的提示词重新提交到会话。
///
/// # Parameters
/// 参数
/// `app` is the service event emitter used by downstream submission updates.
/// `app` 是下游提交流程用于发送更新的服务事件发射器。
/// `state` owns the shared service bridge state and retained workbench snapshot.
/// `state` 持有共享服务桥状态与保留工作台快照。
/// `request` contains the edit target, replacement prompt, and attachment summaries.
/// `request` 包含编辑目标、替换提示词与附件摘要。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot after the edited prompt is resubmitted.
/// 返回编辑后的提示词重新提交后的刷新工作台快照。
pub(super) async fn service_submit_user_prompt_edit(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: UserPromptEditSubmitRequest,
) -> Result<WorkbenchSnapshot, String> {
    if request.stop_running {
        return Err(String::from("ERR_USER_PROMPT_EDIT_SESSION_BUSY"));
    }
    let session_id = parse_session_id(&request.conversation_id)
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_INVALID_CONVERSATION"))?;
    let _validated_attachments = request
        .attachments
        .clone()
        .into_iter()
        .map(composer_attachment_summary_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    let controller_runtime = {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
        let controller_runtime = guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("ERR_CONTROLLER_RUNTIME_NOT_READY"))?;
        if !webui_session_allows_user_prompt_editing(&mut guard, session_id) {
            return Err(String::from("ERR_USER_PROMPT_EDIT_SESSION_BUSY"));
        }
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        if !workspace_user_prompt_message_allows_destructive_edit(
            workspace,
            session_id,
            &request.message_id,
        ) {
            return Err(String::from("ERR_USER_PROMPT_EDIT_NOT_EDITABLE"));
        }
        controller_runtime
    };
    let rollback = rollback_persisted_session_before_user_prompt(
        &controller_runtime,
        session_id,
        &request.message_id,
    )
    .await?;
    {
        let mut guard = state
            .lock()
            .map_err(|_| String::from("ERR_SERVICE_STATE_LOCK_FAILED"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("ERR_WORKSPACE_SNAPSHOT_NOT_READY"))?;
        workspace
            .session_messages
            .insert(session_id, rollback.retained_messages.clone());
        workspace
            .session_runtime_state
            .insert(session_id, rollback.runtime_state);
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Ready);
        if let Some(session) = workspace
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.status = SessionState::Completed;
            session.focus = String::new();
            session.last_updated_label = String::new();
            session.updated_at = current_unix_millis_text();
        }
        guard.workbench.messages = conversation_messages_from_session(
            &session_id.to_string(),
            rollback.retained_messages.as_slice(),
            false,
        );
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench = workbench_with_live_pending_question(&guard);
    }
    service_submit_conversation_prompt(
        app,
        state,
        None,
        Some(request.conversation_id),
        request.prompt,
        None,
        Some(String::from("build")),
        None,
        Some(request.attachments),
        None,
        None,
        None,
    )
    .await
}

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
/// Returns the latest workbench snapshot after queuing or starting the submitted prompt.
/// 返回排队或启动提交提示词后的最新工作台快照。
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
) -> Result<WorkbenchSnapshot, String> {
    let prompt = prompt.trim().to_string();
    webui_trace(
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
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    webui_trace(
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
    webui_trace(
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
        webui_trace(
            "submit_prompt_subagent_running_check_skipped",
            format!("session={session_id} reason=independent_subagent_lane"),
        );
        webui_trace(
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
            webui_trace(
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
            let workbench = store_and_emit_workbench_snapshot(
                &app,
                &state,
                &controller_runtime,
                &luaskills_catalog,
                &token_budget_runtime,
                &mcp_catalog,
                &agent_runtime.provider_configuration,
                &agent_runtime.ai_tool_configuration,
                workspace_snapshot,
                &logs,
            )?;
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
            return Ok(workbench);
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
    webui_trace(
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
    webui_trace(
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
    webui_trace(
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
    webui_trace(
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
    webui_trace(
        "submit_prompt_turn_begin_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(turn_begin_started_at)
        ),
    );
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
        webui_trace(
            "submit_prompt_kernel_turn_started",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(kernel_turn_started_at)
            ),
        );
    }
    mark_webui_prompt_snapshot_preparing(&mut workspace_snapshot, session_id);
    let begin_emit_started_at = current_unix_millis_value();
    let _ = store_and_emit_workbench_snapshot_preserving_subagent_navigation(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot.clone(),
        &logs,
    )?;
    webui_trace(
        "submit_prompt_begin_workbench_emitted",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(begin_emit_started_at)
        ),
    );
    let outer_steps_started_at = current_unix_millis_value();
    apply_webui_prompt_begin_outer_steps(
        &controller_runtime,
        &mut workspace_snapshot,
        session_id,
        &session_title,
    );
    webui_trace(
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
    webui_trace(
        "submit_prompt_hook_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
        ),
    );
    let precheck_plan_started_at = current_unix_millis_value();
    let precheck_plan = webui_shared_prompt_precheck_plan(
        &agent_runtime,
        &workspace_snapshot,
        session_id,
        &workspace_path,
    )?;
    webui_trace(
        "submit_prompt_precheck_plan_ready",
        format!(
            "session={session_id} has_hooks={} skip={} labels={} elapsed_ms={}",
            precheck_plan.has_precheck_hooks,
            precheck_plan.should_skip_precheck,
            precheck_plan.progress_labels.len(),
            current_unix_millis_value().saturating_sub(precheck_plan_started_at)
        ),
    );
    let store_emit_started_at = current_unix_millis_value();
    let mut workbench = store_and_emit_workbench_snapshot_preserving_subagent_navigation(
        &app,
        &state,
        &controller_runtime,
        &luaskills_catalog,
        &token_budget_runtime,
        &mcp_catalog,
        &agent_runtime.provider_configuration,
        &agent_runtime.ai_tool_configuration,
        workspace_snapshot,
        &logs,
    )?;
    webui_trace(
        "submit_prompt_workbench_emitted",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(store_emit_started_at)
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
    if start_webui_precheck_or_continue(
        &app,
        &service_state,
        &controller_runtime,
        &agent_runtime,
        precheck_start,
        precheck_plan,
    )? {
        workbench = current_workbench_snapshot_from_state(&state)?;
    }
    webui_trace(
        "submit_prompt_precheck_dispatch_returned",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(precheck_dispatch_started_at)
        ),
    );
    Ok(workbench)
}
