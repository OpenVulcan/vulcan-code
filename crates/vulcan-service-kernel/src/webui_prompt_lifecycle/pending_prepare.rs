//! WebUI prompt lifecycle pending prepare helpers.
//! WebUI 提示词生命周期 pending prepare 辅助逻辑。

use super::*;
use crate::{
    commit_prepared_submission_media_ledger,
    conversation_submission::{
        deferred_consumed_execution_retry_cleanup, effective_pending_prompt_reasoning_effort,
        promote_submission_attachment_draft_scope, retain_accepted_preparation_retry_checkpoint,
        spawn_accepted_retry_runtime_state_persist_task,
    },
    ensure_webui_goal_session_kernel_turn_start_allowed,
    pending_prompts::pending_prompt_attachment_staging_scope,
    prepare_submission_media,
    project_conversation_services::try_begin_pending_prompt_dispatch_exclusion,
    start_reserved_webui_session_kernel_user_turn, start_webui_goal_session_kernel_turn,
};

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
    let (next_source, goal_dispatch_version) = loop {
        match pending_prompt_dispatch_readiness(
            app,
            state,
            controller_runtime,
            &workspace_snapshot,
            session_id,
            has_active_session_run,
            allow_from_stopped_phase,
            priority_override,
        )
        .await?
        {
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
                if removed == 0 {
                    return Ok(false);
                }
                workspace_snapshot = state
                    .lock()
                    .map_err(|error| format!("failed to lock service state: {error}"))?
                    .workspace_snapshot
                    .clone()
                    .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            }
            PendingPromptDispatchReadiness::StaleGoalGuidanceCleared { removed } => {
                webui_trace(
                    "pending_prompt_stale_goal_guidance_cleared",
                    format!("session={session_id} removed={removed}"),
                );
                if removed == 0 {
                    return Ok(false);
                }
                workspace_snapshot = state
                    .lock()
                    .map_err(|error| format!("failed to lock service state: {error}"))?
                    .workspace_snapshot
                    .clone()
                    .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
            }
            PendingPromptDispatchReadiness::GoalAuthorityChanged => {
                webui_trace(
                    "pending_prompt_goal_authority_changed",
                    format!("session={session_id}"),
                );
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
            PendingPromptDispatchReadiness::Ready {
                source,
                goal_dispatch_version,
            } => break (source, goal_dispatch_version),
        }
    };
    // User-authored queued work needs one process-local admission owner across media preparation and Hook setup.
    // 用户编写的排队工作在媒体准备与 Hook 设置期间必须只有一个进程内准入所有者。
    let pending_prompt_dispatch_guard = if next_source.persists_canonical_user_message() {
        let Some(guard) = try_begin_pending_prompt_dispatch_exclusion(state, session_id)? else {
            webui_trace(
                "pending_prompt_prepare_admission_busy",
                format!("session={session_id} next_source={next_source:?}"),
            );
            return Ok(false);
        };
        Some(guard)
    } else {
        None
    };
    let pending_prompt_dispatch_authorization = pending_prompt_dispatch_guard
        .as_ref()
        .map(|guard| guard.existing_submission_authorization(session_id))
        .transpose()?;
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
    let Some(mut entry) =
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
    // A queued interrupt reaches this path only after no active run remains, so it now owns a new user turn.
    // 排队插队消息只有在活动运行已结束后才会进入此路径，因此此时应拥有新的用户回合。
    entry.source = entry.source.for_idle_dispatch();
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
    // Queued media is prepared at the actual dispatch boundary so native provider references cannot expire while waiting.
    // 排队媒体在实际分发边界准备，避免原生供应商引用在等待期间过期。
    // pendingRuntimeState supplies the exact persisted lease identity captured for this queue owner.
    // pendingRuntimeState 提供为该队列所有者捕获的精确持久化租约身份。
    let pending_runtime_state = workspace_snapshot.session_runtime_state.get(&session_id);
    let prepared_submission_projection = match prepare_submission_media(
        app,
        state,
        &entry_agent_runtime,
        entry.id.as_str(),
        Some(session_id),
        pending_runtime_state,
        None,
        entry.attachments.as_slice(),
    )
    .await
    {
        Ok(prepared_submission_projection) => prepared_submission_projection,
        Err(error) => {
            // Restore before returning so cancellation or upload failure never discards the unsent queued prompt.
            // 返回前先恢复队列，确保取消或上传失败不会丢失尚未发送的排队提示词。
            restore_webui_pending_prompt(&mut workspace_snapshot, session_id, entry)?;
            store_workspace_snapshot_without_workbench_rebuild(state, workspace_snapshot)?;
            emit_pending_prompts_changed_delta(app, state, session_id)?;
            // Restoring an unsent queue entry changes runtime metadata only; transcript ownership stays with the ordered writer.
            // 恢复尚未发送的队列项只修改运行态元数据；转录所有权仍属于顺序写入器。
            spawn_workspace_session_runtime_state_persist_task(
                state.clone(),
                controller_runtime.clone(),
                session_id,
                Some(String::from("queued_media_prepare_failed")),
            );
            if error.starts_with("submission_cancelled:") {
                webui_trace(
                    "pending_prompt_media_prepare_cancelled",
                    format!("session={session_id}"),
                );
                return Ok(false);
            }
            return Err(error);
        }
    };
    let provider_attachments = prepared_submission_projection.provider_attachments;
    let prepared_submission_media = prepared_submission_projection.prepared_media;
    let panel_entry_id = entry.panel_entry_id.clone();
    // reasoning_effort prevents a neutral internal queue placeholder from changing the active reasoning selection.
    // reasoning_effort 防止中性的内部队列占位值改变当前推理强度选择。
    let reasoning_effort = effective_pending_prompt_reasoning_effort(
        entry.source,
        runtime_options.reasoning_effort,
        active_reasoning_selection_effort(&entry_agent_runtime.provider_configuration),
    );
    if entry.source.runs_user_prompt_precheck() {
        snapshot_webui_context_before_user_prompt(
            controller_runtime,
            &workspace_snapshot,
            session_id,
            entry.source,
        )?;
    }
    let prompt_text = entry.prompt.clone();
    // canonicalAttachments strip every request-local delivery mode before the queued user message
    // enters durable history; providerAttachments retain only this dispatch's authoritative copy.
    // canonicalAttachments 在排队用户消息进入持久历史前移除全部单次请求传输模式；
    // providerAttachments 仅保留本次分发的权威副本。
    let canonical_attachments = entry
        .attachments
        .iter()
        .cloned()
        .map(|mut attachment| {
            attachment.delivery_mode = None;
            attachment
        })
        .collect();
    let prompt_turn_begin_input = SharedPromptTurnBeginInput {
        session_id,
        session_title: &session_title,
        workspace_path: &workspace_path,
        prompt_entry_id: entry.id.clone(),
        prompt: &prompt_text,
        hidden_model_guidance: entry.hidden_model_guidance.as_deref(),
        provider_visible_agent_messages: Vec::new(),
        attachments: canonical_attachments,
        provider_attachments: Some(provider_attachments),
        append_canonical_user_message: entry.source.persists_canonical_user_message(),
        append_visible_user_prompt_record: entry.source.appends_visible_user_prompt_record(),
        source: entry.source,
        execution_mode: String::from("build"),
        reasoning_effort,
    };
    // A new user-authored queued turn supersedes any frozen retry identity owned by the completed run.
    // 新的用户排队回合会替代已完成运行所拥有的冻结重试身份。
    let mut deferred_consumed_retry_cleanup = None;
    if entry.source.persists_canonical_user_message() {
        match deferred_consumed_execution_retry_cleanup(
            controller_runtime,
            &workspace_snapshot,
            session_id,
        ) {
            Ok(cleanup) => deferred_consumed_retry_cleanup = cleanup,
            Err(error) => eprintln!(
                "[webui-service] queued prompt consumed execution retry cleanup could not be scheduled: {error}"
            ),
        }
    }
    let precheck_start = if let Some(expected_goal_dispatch_version) = goal_dispatch_version {
        // Service state always precedes the shared Hook runtime in the cross-runtime lock order.
        // 跨运行时锁顺序中，服务状态始终先于共享 Hook 运行时。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        ensure_webui_goal_session_kernel_turn_start_allowed(
            &guard,
            session_id,
            expected_goal_dispatch_version,
        )?;
        let mut hook_runtime = entry_agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        let precheck_start = begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            prompt_turn_begin_input,
        )?;
        start_webui_goal_session_kernel_turn(
            &mut guard,
            session_id,
            expected_goal_dispatch_version,
            &precheck_start.preparation_turn_id,
            ActiveTurnKind::UserPrompt,
        )?;
        precheck_start
    } else {
        // Service state always precedes the shared Hook runtime in the cross-runtime lock order.
        // 跨运行时锁顺序中，服务状态始终先于共享 Hook 运行时。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let mut hook_runtime = entry_agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        let precheck_start = begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            prompt_turn_begin_input,
        )?;
        if let Some(authorization) = pending_prompt_dispatch_authorization {
            start_reserved_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
                authorization.reservation_id,
            )?;
        } else {
            start_webui_session_kernel_turn(
                &mut guard,
                session_id,
                &precheck_start.preparation_turn_id,
                ActiveTurnKind::UserPrompt,
            )?;
        }
        precheck_start
    };
    if entry.source.persists_canonical_user_message()
        && let Err(error) = retain_accepted_preparation_retry_checkpoint(
            &mut workspace_snapshot,
            session_id,
            entry.id.as_str(),
            &precheck_start,
            deferred_consumed_retry_cleanup
                .as_ref()
                .map(|cleanup| cleanup.generation()),
        )
    {
        // Retry retention is auxiliary; the accepted user prompt remains authoritative even if this projection fails.
        // 重试保留属于辅助能力；即使投影失败，已接纳的用户提示词仍保持权威。
        eprintln!(
            "[webui-service] queued prompt accepted preparation retry checkpoint skipped: {error}"
        );
        if let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
        {
            let _ = runtime_state.execution_retry_snapshot.take();
            runtime_state.transient_execution_retry_snapshot = None;
            runtime_state.updated_at = current_unix_millis_text();
        }
    }
    commit_prepared_submission_media_ledger(
        &mut workspace_snapshot,
        session_id,
        precheck_start.canonical_user_message_id.as_deref(),
        prepared_submission_media,
    )?;
    apply_webui_prompt_begin_outer_steps(
        controller_runtime,
        &mut workspace_snapshot,
        session_id,
        &session_title,
    );
    refresh_exact_vmm_hook_session_projection(
        &entry_agent_runtime.vmm_hook_runtime,
        &workspace_snapshot,
        session_id,
    )?;
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
    // Only the canonical user row crosses the durable transcript boundary; the earlier admission row is UI-only.
    // 只有规范用户记录跨越可靠转录边界；此前的接纳行仅用于界面投影。
    let visible_user_prompt_record_for_persistence = if entry
        .source
        .persists_canonical_user_message()
    {
        let visible_user_prompt_record_id = precheck_start
            .visible_user_prompt_record_id
            .as_deref()
            .ok_or_else(|| {
                String::from("Queued user prompt started without a visible transcript record id.")
            })?;
        Some(
            workspace_snapshot
                .session_messages
                .get(&session_id)
                .and_then(|messages| {
                    messages
                        .iter()
                        .find(|message| message.id == visible_user_prompt_record_id)
                })
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Queued user prompt visible record `{visible_user_prompt_record_id}` is missing from the retained transcript."
                    )
                })?,
        )
    } else {
        None
    };
    commit_workspace_snapshot_and_emit_session_phase_delta(
        app,
        state,
        workspace_snapshot,
        session_id,
        SessionRunPhase::Preparing,
    )?;
    if let Some(panel_entry_id) = panel_entry_id.as_deref() {
        let promotion_result =
            pending_prompt_attachment_staging_scope(panel_entry_id).and_then(|queue_scope| {
                promote_submission_attachment_draft_scope(
                    &workspace_path,
                    session_id,
                    precheck_start.canonical_user_message_id.as_deref(),
                    entry.attachments.as_slice(),
                    Some(queue_scope.as_str()),
                )
            });
        if let Err(error) = promotion_result {
            // The canonical queued message is already committed; retaining the queue lease is the safe repairable outcome.
            // 规范排队消息已经提交；保留队列租约是安全且可修复的结果。
            webui_trace(
                "pending_prompt_attachment_lease_promotion_failed",
                format!("session={session_id} panel_entry={panel_entry_id} error={error}"),
            );
        }
    }
    if panel_entry_id.is_some() {
        emit_pending_prompts_changed_delta(app, state, session_id)?;
    }
    if let Some(visible_user_prompt_record) = visible_user_prompt_record_for_persistence {
        // Admission is synchronous inside the writer helper, so later runtime events cannot reserve this row's position first.
        // 写入器辅助函数会同步完成准入，因此后续运行时事件不能抢先预留该记录的位置。
        // transcriptAdmissionResult preserves synchronous locator/cursor failure before any durability wait begins.
        // transcriptAdmissionResult 在开始可靠性等待前保留同步定位器或游标失败。
        let transcript_admission_result = spawn_exact_session_transcript_entries_persist_task(
            state.clone(),
            controller_runtime.clone(),
            session_id,
            vec![visible_user_prompt_record],
            Some(String::from("queued_canonical_user_message")),
        );
        // The queued Provider turn cannot start until its canonical user boundary is physically durable.
        // 排队 Provider 回合只能在其规范用户边界完成物理持久化后启动。
        // transcriptDurabilityResult unifies synchronous admission and asynchronous physical-write failure.
        // transcriptDurabilityResult 统一同步准入失败与异步物理写入失败。
        let transcript_durability_result = match transcript_admission_result {
            Ok(()) => await_session_transcript_writer_durable(session_id).await,
            Err(error) => Err(error),
        };
        if let Err(error) = transcript_durability_result {
            let cleanup_error = mark_webui_prompt_preparation_failed(
                app,
                state,
                controller_runtime,
                &entry_agent_runtime,
                session_id,
                &precheck_start.preparation_turn_id,
                &error,
            )
            .await
            .err();
            return Err(cleanup_error.map_or_else(
                || format!("Queued user prompt transcript durability failed: {error}"),
                |cleanup_error| {
                    format!(
                        "Queued user prompt transcript durability failed: {error}; cleanup also failed: {cleanup_error}"
                    )
                },
            ));
        }
    }
    if entry.source.persists_canonical_user_message() {
        spawn_accepted_retry_runtime_state_persist_task(
            state.clone(),
            controller_runtime.clone(),
            session_id,
            deferred_consumed_retry_cleanup,
            "queued_begin",
        );
    } else if entry.source.runs_user_prompt_precheck() {
        spawn_workspace_session_runtime_state_persist_task(
            state.clone(),
            controller_runtime.clone(),
            session_id,
            Some(String::from("queued_begin")),
        );
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
