//! WebUI prompt lifecycle pending prepare helpers.
//! WebUI 提示词生命周期 pending prepare 辅助逻辑。

use super::*;

/// Prepares and dispatches the next queued WebUI pending prompt when the session is idle.
/// 在会话空闲时准备并分发下一个排队的 WebUI 待发送提示词。
///
/// # Parameters
/// 参数
/// `app` emits updated workbench snapshots.
/// `app` 发出更新后的工作台快照。
/// `state` owns retained workspace and runtime state.
/// `state` 持有保留的工作区与运行态。
/// `controller_runtime` provides persistence for context and runtime snapshots.
/// `controller_runtime` 提供上下文与运行态快照持久化能力。
/// `agent_runtime` is the default runtime used unless the pending entry overrides it.
/// `agent_runtime` 是待发送条目未覆盖时使用的默认运行时。
/// `session_id` selects the session queue to dispatch.
/// `session_id` 选择要分发的会话队列。
/// `allow_from_stopped_phase` controls guided dispatch from a stopped phase.
/// `allow_from_stopped_phase` 控制是否允许从停止阶段进行引导式分发。
///
/// # Returns
/// 返回值
/// Whether a queued prompt turn was started.
/// 是否已启动一个排队提示词回合。
pub(crate) async fn maybe_prepare_next_webui_pending_prompt(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    allow_from_stopped_phase: bool,
) -> Result<bool, String> {
    let (
        mut workspace_snapshot,
        has_active_stream_task,
        has_active_kernel_turn,
        has_active_session_run,
        has_pending_plan_artifact,
    ) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard
            .workspace_snapshot
            .clone()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        (
            workspace_snapshot,
            guard.active_stream_tasks.contains_key(&session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
            webui_session_has_active_run(&guard, session_id),
            guard
                .workspace_snapshot
                .as_ref()
                .is_some_and(|snapshot| session_has_pending_plan_artifact(snapshot, session_id)),
        )
    };
    let priority_override = false;
    let queue_status = pending_prompt_queue_status(&workspace_snapshot, session_id);
    webui_trace(
        "pending_prompt_prepare_check",
        format!(
            "session={session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_running_subagents=false priority_override={priority_override} internal_queue_len={internal_queue_len} visible_queue_len={visible_queue_len} run_phase={run_phase:?} allow_from_stopped_phase={allow_from_stopped_phase} has_pending_plan_artifact={has_pending_plan_artifact}",
            internal_queue_len = queue_status.internal_queue_len,
            visible_queue_len = queue_status.visible_queue_len,
            run_phase = queue_status.run_phase
        ),
    );
    if has_pending_plan_artifact {
        webui_trace(
            "pending_prompt_prepare_waiting_for_plan_decision",
            format!("session={session_id}"),
        );
        return Ok(false);
    }
    let mut retried_after_stale_mailbox_cleanup = false;
    let next_source = loop {
        match pending_prompt_dispatch_readiness(
            app,
            state,
            &workspace_snapshot,
            agent_runtime,
            session_id,
            has_active_session_run,
            allow_from_stopped_phase,
            priority_override,
        )? {
            PendingPromptDispatchReadiness::Empty => {
                webui_trace(
                    "pending_prompt_prepare_none",
                    format!("session={session_id}"),
                );
                return Ok(false);
            }
            PendingPromptDispatchReadiness::StaleSessionMailboxCleared { removed } => {
                webui_trace(
                    "pending_prompt_stale_session_mailbox_cleared",
                    format!("session={session_id} removed={removed}"),
                );
                if retried_after_stale_mailbox_cleanup || removed == 0 {
                    return Ok(false);
                }
                retried_after_stale_mailbox_cleanup = true;
                workspace_snapshot = state
                    .lock()
                    .map_err(|error| format!("failed to lock service state: {error}"))?
                    .workspace_snapshot
                    .clone()
                    .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            }
            PendingPromptDispatchReadiness::Blocked { source, reason } => {
                webui_trace(
                    "pending_prompt_prepare_blocked",
                    format!("session={session_id} next_source={source:?} reason={reason:?}"),
                );
                return Ok(false);
            }
            PendingPromptDispatchReadiness::Ready { source } => break source,
        }
    };
    let Some(session_title) = workspace_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .map(|session| session.title.clone())
    else {
        webui_trace(
            "pending_prompt_prepare_missing_session_title",
            format!("session={session_id} next_source={next_source:?}"),
        );
        return Ok(false);
    };
    let Some(entry) =
        take_next_webui_pending_prompt(&mut workspace_snapshot, session_id, priority_override)
    else {
        webui_trace(
            "pending_prompt_prepare_take_none",
            format!(
                "session={session_id} next_source={next_source:?} priority_override={priority_override}"
            ),
        );
        return Ok(false);
    };
    webui_trace(
        "pending_prompt_prepare_start",
        format!(
            "session={session_id} entry_id={} source={:?} task_id={} prompt_chars={} attachments={}",
            entry.id,
            entry.source,
            entry.task_id.as_deref().unwrap_or("<none>"),
            entry.prompt.chars().count(),
            entry.attachments.len()
        ),
    );
    if entry.source.starts_user_parent_turn() {
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
    let workspace_path = workspace_path_for_session(&workspace_snapshot, session_id);
    let runtime_options = entry.runtime_options.clone();
    let entry_agent_runtime = agent_runtime_for_pending_prompt(agent_runtime, &entry);
    let panel_entry_id = entry.panel_entry_id.clone();
    let disabled_ai_capability_kinds = runtime_options
        .disabled_ai_capability_kinds
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if entry.source.runs_user_prompt_precheck() {
        snapshot_webui_context_before_user_prompt(
            controller_runtime,
            &workspace_snapshot,
            session_id,
            entry.source,
        )?;
    }
    let prompt_text = entry.prompt.clone();
    let precheck_start = {
        let mut hook_runtime = entry_agent_runtime
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
                prompt_entry_id: entry.id.clone(),
                prompt: &prompt_text,
                hidden_model_guidance: entry.hidden_model_guidance.as_deref(),
                attachments: entry.attachments,
                append_canonical_user_message: entry.source.persists_canonical_user_message(),
                append_visible_user_prompt_record: entry
                    .source
                    .appends_visible_user_prompt_record(),
                source: entry.source,
                execution_mode: String::from("build"),
                reasoning_effort: Some(runtime_options.reasoning_effort),
                disabled_ai_capability_kinds,
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
            precheck_start.precheck_request.turn_id(),
            ActiveTurnKind::UserPrompt,
        )?;
    }
    apply_webui_prompt_begin_outer_steps(
        controller_runtime,
        &mut workspace_snapshot,
        session_id,
        &session_title,
    );
    entry_agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &entry_agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    let precheck_plan = if entry.source.runs_user_prompt_precheck() {
        webui_shared_prompt_precheck_plan(
            &entry_agent_runtime,
            &workspace_snapshot,
            session_id,
            &workspace_path,
        )?
    } else {
        SharedPromptPrecheckPlan {
            has_precheck_hooks: false,
            should_skip_precheck: true,
            progress_labels: Vec::new(),
        }
    };
    if let Some(panel_entry_id) = panel_entry_id.as_deref() {
        remove_dispatched_pending_prompt_panel_entry(
            &mut workspace_snapshot,
            session_id,
            panel_entry_id,
        );
    }
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        &entry_agent_runtime,
        workspace_snapshot,
    )?;
    if entry.source.runs_user_prompt_precheck() {
        let state_for_persist = state.clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        spawn_service_task(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("[webui-service] queued begin persist snapshot failed: {error}");
                    return;
                }
            };
            spawn_workspace_session_runtime_state_persist_task(
                state_for_persist,
                controller_runtime_for_persist,
                snapshot,
                session_id,
                Some(String::from("queued_begin")),
            );
        });
    }
    start_webui_precheck_or_continue(
        app,
        state,
        controller_runtime,
        &entry_agent_runtime,
        precheck_start,
        precheck_plan,
    )?;
    webui_trace(
        "pending_prompt_prepare_started",
        format!("session={session_id}"),
    );
    Ok(true)
}
