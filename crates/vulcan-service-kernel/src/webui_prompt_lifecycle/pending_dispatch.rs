//! WebUI prompt lifecycle pending dispatch helpers.
//! WebUI 提示词生命周期 pending dispatch 辅助逻辑。

use super::*;

/// Inserts one visible pending prompt and starts dispatch when the selected session can run.
/// 插入单条可见待发送提示词，并在当前会话可运行时启动分发。
///
/// # Parameters
/// 参数
/// `app` emits pending-prompt and dispatch deltas.
/// `app` 发出待发送队列与分发增量。
/// `state` owns retained service state and runtime handles.
/// `state` 持有保留的服务状态与运行时句柄。
/// `entry_id` selects the visible queue row to insert.
/// `entry_id` 选择要插入的可见队列行。
///
/// # Returns
/// 返回值
/// `true` when the prompt was accepted for insertion.
/// 当提示词已被接受插入时返回 `true`。
pub(crate) async fn service_insert_pending_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    entry_id: String,
) -> Result<bool, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let workspace_path = workspace_path_for_session(&workspace_snapshot, selected_session_id);
    insert_webui_pending_prompt_into_internal_queue(
        &mut workspace_snapshot,
        &agent_runtime,
        selected_session_id,
        &workspace_path,
        &entry_id,
    )?;
    mark_webui_session_waiting_for_guided_prompt_dispatch(
        &mut workspace_snapshot,
        selected_session_id,
    );
    store_workspace_snapshot_without_workbench_rebuild(&state, workspace_snapshot)?;
    emit_pending_prompts_changed_delta(&app, &state, selected_session_id)?;
    let snapshot_for_persist = current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    spawn_workspace_session_persist_task(
        state.clone(),
        controller_runtime.clone(),
        snapshot_for_persist,
        selected_session_id,
        Some(String::from("guided_insert")),
    );
    // Preserve the global turn priority even for manually guided queue insertion.
    // 即使是手动引导队列插入，也保留全局回合优先级。
    let started_mailbox_turn = maybe_start_webui_mailbox_turn_if_ready(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        selected_session_id,
    )
    .await?;
    let started_next_prompt = if started_mailbox_turn {
        false
    } else {
        maybe_prepare_next_webui_pending_prompt(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            selected_session_id,
            true,
        )
        .await?
    };
    let _dispatch_started = started_mailbox_turn || started_next_prompt;
    Ok(true)
}

/// Starts one mailbox-triggered parent model turn directly from kernel mailbox input.
/// 直接基于内核 mailbox 输入启动一个 mailbox 触发的父模型回合。
pub(crate) async fn maybe_start_webui_mailbox_turn_if_ready(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
) -> Result<bool, String> {
    let mailbox_prepare_started_at = current_unix_millis_value();
    let (
        has_active_run,
        has_active_kernel_turn,
        has_trigger_mailbox,
        has_pending_plan_artifact,
        preserved_workspace_run_state,
        retained_workspace_snapshot,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let active_kernel_sessions = active_kernel_session_ids(&guard);
        let preserved_workspace_run_state = capture_preserved_workspace_run_state(
            guard.workspace_snapshot.as_ref(),
            &active_kernel_sessions,
            Some(session_id),
        );
        let has_trigger_mailbox = guard
            .session_runtime_kernels
            .get(&session_id)
            .is_some_and(|kernel| kernel.input_queue().has_trigger_turn_mailbox_items());
        let has_pending_plan_artifact = guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|snapshot| session_has_pending_plan_artifact(snapshot, session_id));
        // Reuses the retained session context for mailbox-triggered follow-up turns to avoid disk reloads after child completion.
        // 对 mailbox 触发的后续回合复用保留会话上下文，避免子代理完成后重新从磁盘加载。
        let retained_workspace_snapshot = guard.workspace_snapshot.as_ref().and_then(|snapshot| {
            retained_submission_snapshot_has_session_context(snapshot, session_id)
                .then(|| snapshot.clone())
        });
        (
            webui_session_has_active_run(&guard, session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
            has_trigger_mailbox,
            has_pending_plan_artifact,
            preserved_workspace_run_state,
            retained_workspace_snapshot,
        )
    };
    webui_trace_if_slow(
        "mailbox_turn_prepare_slow",
        mailbox_prepare_started_at,
        format!("session={session_id}"),
    );
    webui_trace(
        "mailbox_turn_prepare_check",
        format!(
            "session={session_id} has_active_run={has_active_run} has_active_kernel_turn={has_active_kernel_turn} has_trigger_mailbox={has_trigger_mailbox} has_pending_plan_artifact={has_pending_plan_artifact}"
        ),
    );
    if has_active_run || has_active_kernel_turn || !has_trigger_mailbox || has_pending_plan_artifact
    {
        return Ok(false);
    }
    let Some(prompt_text) = webui_session_mailbox_input_from_kernel(state, session_id, None)?
    else {
        return Ok(false);
    };
    let mailbox_workspace_started_at = current_unix_millis_value();
    let session_id_text = session_id.to_string();
    let (mut workspace_snapshot, workspace_source) =
        if let Some(workspace_snapshot) = retained_workspace_snapshot {
            (workspace_snapshot, "retained")
        } else {
            (
                load_workspace_for_submission(controller_runtime.clone(), Some(&session_id_text))
                    .await?,
                "disk",
            )
        };
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut workspace_snapshot,
    );
    webui_trace_if_slow(
        "mailbox_turn_workspace_ready_slow",
        mailbox_workspace_started_at,
        format!("session={session_id} source={workspace_source}"),
    );
    let Some(session_title) = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .map(|session| session.title.clone())
    else {
        return Ok(false);
    };
    let workspace_path = workspace_path_for_session(&workspace_snapshot, session_id);
    let start = {
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
                prompt_entry_id: format!("webui-mailbox-{}", SessionId::new_v4()),
                prompt: &prompt_text,
                hidden_model_guidance: None,
                attachments: Vec::new(),
                append_canonical_user_message: false,
                append_visible_user_prompt_record: false,
                source: PendingSessionPromptSource::SessionMailbox,
                execution_mode: String::from("build"),
                reasoning_effort: active_reasoning_selection_effort(
                    &agent_runtime.provider_configuration,
                ),
                disabled_ai_capability_kinds: BTreeSet::new(),
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_order_overrides: BTreeMap::new(),
            },
        )
    }?;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        start_webui_session_kernel_turn(
            &mut guard,
            session_id,
            start.precheck_request.turn_id(),
            ActiveTurnKind::MailboxTriggered,
        )?;
        if let Some(kernel) = guard.session_runtime_kernels.get_mut(&session_id) {
            let consumed = kernel.input_queue_mut().drain_current_turn_inputs().len();
            webui_trace(
                "mailbox_turn_inputs_consumed",
                format!("session={session_id} consumed={consumed}"),
            );
        }
    }
    apply_webui_prompt_begin_outer_steps(
        controller_runtime,
        &mut workspace_snapshot,
        session_id,
        &session_title,
    );
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    commit_workspace_snapshot_and_emit_session_phase_delta(
        app,
        state,
        workspace_snapshot,
        session_id,
        SessionRunPhase::Preparing,
    )?;
    let precheck_plan = SharedPromptPrecheckPlan {
        has_precheck_hooks: false,
        should_skip_precheck: true,
        progress_labels: Vec::new(),
    };
    start_webui_precheck_or_continue(
        app,
        state,
        controller_runtime,
        agent_runtime,
        start,
        precheck_plan,
    )?;
    webui_trace(
        "mailbox_turn_started",
        format!(
            "session={session_id} prompt_chars={}",
            prompt_text.chars().count()
        ),
    );
    Ok(true)
}

/// Readiness decision for the next pending prompt dispatch attempt.
/// 下一次待发送提示词分发尝试的就绪判定。
#[derive(Debug, Clone, Copy)]
pub(crate) enum PendingPromptDispatchReadiness {
    /// No dispatchable pending prompt source exists for the selected session.
    /// 当前选中会话不存在可分发的待发送提示词来源。
    Empty,
    /// Stale session mailbox entries were removed instead of starting a new turn.
    /// 已清理过期会话邮箱条目，因此不启动新回合。
    StaleSessionMailboxCleared { removed: usize },
    /// A pending prompt source exists but the shared dispatch gate is blocking it.
    /// 存在待发送提示词来源，但共享分发闸门正在阻塞它。
    Blocked {
        source: PendingSessionPromptSource,
        reason: PendingPromptDispatchBlockReason,
    },
    /// A pending prompt source can be dispatched by the caller.
    /// 调用方可以分发一个待发送提示词来源。
    Ready { source: PendingSessionPromptSource },
}

/// Determines whether the next pending prompt source may be dispatched by the prompt lifecycle.
/// 判断下一个待发送提示词来源是否可由提示词生命周期继续分发。
pub(crate) fn pending_prompt_dispatch_readiness(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    has_active_run: bool,
    allow_from_stopped_phase: bool,
    priority_override: bool,
) -> Result<PendingPromptDispatchReadiness, String> {
    let dispatch_context = webui_pending_prompt_dispatch_context(
        workspace_snapshot,
        &agent_runtime.provider_configuration,
        session_id,
        has_active_run,
    );
    let Some(source) = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| {
            next_pending_prompt_dispatch_source(&runtime_state.ui_state, priority_override)
        })
    else {
        return Ok(PendingPromptDispatchReadiness::Empty);
    };
    if source == PendingSessionPromptSource::SessionMailbox {
        let removed = clear_stale_webui_session_mailbox_pending_prompts(app, state, session_id)?;
        return Ok(PendingPromptDispatchReadiness::StaleSessionMailboxCleared { removed });
    }
    if let Some(reason) = pending_prompt_dispatch_block_reason_for_source(
        dispatch_context,
        allow_from_stopped_phase,
        Some(source),
    ) {
        return Ok(PendingPromptDispatchReadiness::Blocked { source, reason });
    }
    Ok(PendingPromptDispatchReadiness::Ready { source })
}

/// Marks the selected session as waiting for a guided prompt to reach the provider loop.
/// 标记当前会话正在等待引导提示词进入 Provider 循环。
pub(crate) fn mark_webui_session_waiting_for_guided_prompt_dispatch(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) {
    if workspace_snapshot
        .sessions
        .iter()
        .any(|session| session.id == session_id && session.status == SessionState::Running)
    {
        return;
    }
    let now = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.updated_at = now.clone();
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("已有消息接入主会话，正在等待当前系统任务处理完成后继续。");
        session.last_updated_label = String::from("刚刚");
    }
    let runtime_state_snapshot = runtime_state.clone();
    merge_runtime_state_records_into_display_messages(
        workspace_snapshot,
        session_id,
        &runtime_state_snapshot,
    );
    sync_hydrated_session_dialogue_ready_marker(workspace_snapshot, session_id);
}
