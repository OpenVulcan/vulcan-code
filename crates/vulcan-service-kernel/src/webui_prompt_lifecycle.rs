//! WebUI prompt lifecycle orchestration around precheck and queued dispatch.
//! WebUI 提示词生命周期编排，覆盖 precheck 与排队分发。

use std::collections::{BTreeMap, BTreeSet};

use vulcan_agent_runtime::{
    ExecutionPreparationRequest,
    agent_runtime::{
        conversation_hooks::ConversationPrecheckOutcome,
        events::{RuntimeEvent, RuntimeEventKind},
    },
    session_event_runtime::append_runtime_event_projection_to_state,
    workbench_ai_capability_router::effective_ai_tool_configuration_for_prompt_overrides,
    workbench_prompt_turn::{
        SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, SharedPromptTurnContinueInput,
        SharedPromptTurnStart, begin_shared_prompt_turn,
        continue_shared_prompt_turn_after_precheck, shared_prompt_precheck_plan,
    },
};
use vulcan_service_api::{ServiceEventEmitter, WorkbenchSnapshot};
use vulcan_session_core::ActiveTurnKind;
use vulcan_workbench_model::{
    PendingPromptDispatchBlockReason, PendingSessionPromptSource, SessionId, SessionRunPhase,
    SessionRuntimeStateEntry, SessionState, next_pending_prompt_dispatch_source,
    pending_prompt_dispatch_block_reason_for_source,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use super::pending_prompts::{
    agent_runtime_for_pending_prompt, clear_stale_webui_session_mailbox_pending_prompts,
    insert_webui_pending_prompt_into_internal_queue, pending_prompt_queue_status,
    remove_dispatched_pending_prompt_panel_entry, take_next_webui_pending_prompt,
    webui_pending_prompt_dispatch_context,
};
use super::session_hydration::{
    capture_preserved_workspace_run_state, load_workspace_for_submission,
    preserve_active_session_run_state, retained_submission_snapshot_has_session_context,
};
use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, active_kernel_session_ids,
    active_reasoning_selection_effort, apply_webui_prompt_begin_outer_steps,
    build_vmm_hook_config_snapshot, current_unix_millis_text, current_unix_millis_value,
    current_workbench_snapshot_from_state, current_workspace_snapshot_from_shared_state,
    current_workspace_snapshot_from_state, ensure_webui_session_runtime_state,
    load_project_configuration, merge_runtime_state_records_into_display_messages,
    owning_project_id_for_session, persist_webui_context_current_document,
    push_webui_service_diagnostic, runtime_parts_from_state, selected_session_id_from_state,
    session_ai_tool_configuration_or_default, session_goal_context_for_prompt_from_state,
    session_has_pending_plan_artifact, session_plan_context_for_prompt_from_state,
    snapshot_webui_context_before_user_prompt, spawn_agent_stream_task, spawn_service_task,
    spawn_workspace_session_persist_task, spawn_workspace_session_runtime_state_persist_task,
    start_webui_session_kernel_turn, stop_webui_session_kernel_generation,
    store_and_emit_workbench_snapshot, store_current_workbench_snapshot,
    sync_hydrated_session_dialogue_ready_marker, webui_session_has_active_run,
    webui_session_kernel_has_active_turn, webui_session_mailbox_input_from_kernel, webui_trace,
    webui_trace_if_slow, workspace_path_for_session,
};

/// Inserts one visible pending prompt and starts dispatch immediately when the session can run.
/// 插入单条可见待发送提示词，并在会话可运行时立即启动分发。
///
/// # Parameters
/// 参数
/// `app` emits the queue and dispatch workbench snapshots.
/// `app` 发出队列与分发工作台快照。
/// `state` owns retained service state and runtime handles.
/// `state` 持有保留的服务状态与运行时句柄。
/// `entry_id` selects the visible queue row to insert.
/// `entry_id` 选择要插入的可见队列行。
///
/// # Returns
/// 返回值
/// The latest workbench snapshot after queue insertion and optional dispatch.
/// 返回队列插入及可选分发后的最新工作台快照。
pub(super) async fn service_insert_pending_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    entry_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
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
    let mut workbench = store_and_emit_workbench_snapshot(
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
    if started_mailbox_turn || started_next_prompt {
        workbench = current_workbench_snapshot_from_state(&state)?;
    }
    Ok(workbench)
}

/// Starts one mailbox-triggered parent model turn directly from kernel mailbox input.
/// 直接基于内核 mailbox 输入启动一个 mailbox 触发的父模型回合。
pub(super) async fn maybe_start_webui_mailbox_turn_if_ready(
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
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        agent_runtime,
        workspace_snapshot,
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
enum PendingPromptDispatchReadiness {
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
fn pending_prompt_dispatch_readiness(
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
fn mark_webui_session_waiting_for_guided_prompt_dispatch(
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

/// Builds the shared precheck plan for a WebUI session using the service VMM layering rules.
/// 使用服务运行时的 VMM 分层规则为一个 WebUI 会话构建共享 precheck 计划。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns provider configuration and hook runtime state.
/// `agent_runtime` 持有 Provider 配置与 hook 运行态。
/// `workspace_snapshot` contains session-scoped VMM overrides.
/// `workspace_snapshot` 包含会话级 VMM 覆盖配置。
/// `session_id` selects the session being prepared.
/// `session_id` 选择正在准备的会话。
/// `workspace_path` identifies the project configuration file to load.
/// `workspace_path` 标识需要加载的项目配置文件。
///
/// # Returns
/// 返回值
/// A shared precheck plan matching the WebUI service shell.
/// 返回与 WebUI 服务外壳一致的共享 precheck 计划。
pub(super) fn webui_shared_prompt_precheck_plan(
    agent_runtime: &AgentRuntimeBridge,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    workspace_path: &str,
) -> Result<SharedPromptPrecheckPlan, String> {
    let project_configuration_started_at = current_unix_millis_value();
    let project_configuration = load_project_configuration(workspace_path);
    webui_trace(
        "precheck_plan_project_config_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(project_configuration_started_at)
        ),
    );
    let session_layer = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .map(|runtime_state| &runtime_state.vmm);
    let effective_vmm_config = vulcan_vmm::VmmEffectiveConfig::resolve(
        &agent_runtime.provider_configuration.vmm,
        Some(&project_configuration.vmm),
        session_layer,
    );
    let hook_snapshot_started_at = current_unix_millis_value();
    let precheck_hook_snapshots = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?
        .precheck_hook_snapshots();
    webui_trace(
        "precheck_plan_hook_snapshots_ready",
        format!(
            "session={session_id} hooks={} elapsed_ms={}",
            precheck_hook_snapshots.len(),
            current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
        ),
    );
    Ok(shared_prompt_precheck_plan(
        &effective_vmm_config,
        &precheck_hook_snapshots,
    ))
}

/// Continues a WebUI prompt turn after the shared precheck outcome is known.
/// 在共享 precheck 结果确定后继续 WebUI 提示词回合。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and diagnostics.
/// `app` 发出工作台快照与诊断信息。
/// `state` owns the retained workspace snapshot.
/// `state` 持有保留的工作区快照。
/// `controller_runtime` provides segmented session persistence.
/// `controller_runtime` 提供分段会话持久化能力。
/// `agent_runtime` owns provider, hook, and AI tool configuration.
/// `agent_runtime` 持有 Provider、hook 与 AI 工具配置。
/// `start` is the prompt turn start produced by the shared reducer.
/// `start` 是共享 reducer 产出的提示词回合起点。
/// `precheck_outcome` is either a real hook result or a skipped outcome.
/// `precheck_outcome` 是真实 hook 结果或跳过后的结果。
///
/// # Returns
/// 返回值
/// A provider execution request when the turn should continue, or `None` when stopped.
/// 当回合应继续时返回 Provider 执行请求；停止时返回 `None`。
fn continue_webui_prompt_turn_after_precheck_from_state(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    precheck_outcome: ConversationPrecheckOutcome,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    let session_id = start.session_id;
    let continue_started_at = current_unix_millis_value();
    webui_trace(
        "precheck_continue_entered",
        format!("session={session_id} source={:?}", start.source),
    );
    let runs_user_prompt_precheck = start.source.runs_user_prompt_precheck();
    let snapshot_lock_started_at = current_unix_millis_value();
    let mut workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    webui_trace(
        "precheck_continue_snapshot_loaded",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(snapshot_lock_started_at)
        ),
    );
    if workspace_snapshot.session_run_phases.get(&session_id) == Some(&SessionRunPhase::Stopped) {
        if let Ok(mut guard) = state.lock() {
            stop_webui_session_kernel_generation(&mut guard, session_id);
        }
        spawn_workspace_session_persist_task(
            state.clone(),
            controller_runtime.clone(),
            workspace_snapshot,
            session_id,
            Some(String::from("precheck_stopped")),
        );
        return Ok(None);
    }
    ensure_webui_session_runtime_state(
        &mut workspace_snapshot,
        session_id,
        Some(start.precheck_request.turn_id()),
    );
    let session_ai_tool_configuration = session_ai_tool_configuration_or_default(
        &workspace_snapshot,
        session_id,
        &agent_runtime.ai_tool_configuration,
    );
    let ai_tool_configuration = effective_ai_tool_configuration_for_prompt_overrides(
        session_ai_tool_configuration,
        &start.disabled_ai_capability_kinds,
        &start.disabled_ai_capability_target_selectors,
        &start.ai_capability_target_order_overrides,
    );
    let project_configuration = load_project_configuration(&start.workspace_path);
    let runtime_reasoning_effort = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_reasoning_effort);
    let session_title_for_context = start.session_title.clone();
    let prompt_turn_id = start.precheck_request.turn_id().to_string();
    let shared_continue_started_at = current_unix_millis_value();
    let continuation = continue_shared_prompt_turn_after_precheck(
        &mut workspace_snapshot,
        SharedPromptTurnContinueInput {
            start,
            precheck_outcome,
            ai_tool_configuration,
            project_configuration,
            runtime_reasoning_effort,
            provider_reasoning_effort: active_reasoning_selection_effort(
                &agent_runtime.provider_configuration,
            ),
            patch_visible_user_prompt_record: runs_user_prompt_precheck,
        },
    )?;
    webui_trace(
        "precheck_continue_request_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(shared_continue_started_at)
        ),
    );
    let vmm_snapshot_started_at = current_unix_millis_value();
    agent_runtime
        .vmm_hook_runtime
        .replace_snapshot(build_vmm_hook_config_snapshot(
            &agent_runtime.provider_configuration,
            &workspace_snapshot,
        ));
    webui_trace(
        "precheck_continue_hook_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(vmm_snapshot_started_at)
        ),
    );
    if let Some(detail) = continuation.precheck_failure_detail.as_deref() {
        eprintln!("[webui-service] precheck hook failed but request will continue: {detail}");
        if let Err(error) = push_webui_service_diagnostic(
            state,
            format!("warning · Precheck hook 执行失败：{detail}"),
        ) {
            eprintln!("[webui-service] record precheck warning failed: {error}");
        }
    }
    let mut request = continuation.request;
    request.controller_runtime = Some(controller_runtime.clone());
    request.session_goal_context = session_goal_context_for_prompt_from_state(state, session_id)?;
    request.session_plan_context =
        session_plan_context_for_prompt_from_state(app, state, controller_runtime, session_id)?;
    request.task_session_store_root = owning_project_id_for_session(
        &workspace_snapshot,
        session_id,
    )
    .and_then(|project_id| {
        vulcan_workspace_store::workspace_store::find_segmented_session_directory_for_task_store(
            &controller_runtime.data_root,
            &project_id,
            &session_id.to_string(),
        )
        .ok()
        .flatten()
    });
    request.runtime_system_updates =
        match vulcan_workspace_store::workspace_store::load_session_runtime_system_updates(
            &controller_runtime.data_root,
            session_id,
        ) {
            Ok(updates) => updates,
            Err(error) => {
                webui_trace(
                    "runtime_system_updates_load_failed",
                    format!("session={session_id} error={error}"),
                );
                Vec::new()
            }
        };
    if let Some(injection) = request.context_injection.as_ref()
        && let Err(error) =
            vulcan_workspace_store::workspace_store::record_session_runtime_injection_applied(
                &controller_runtime.data_root,
                session_id,
                Some(prompt_turn_id),
                Some(injection.fingerprint.clone()),
                &injection.persistent_blocks,
                injection
                    .has_transient()
                    .then_some(injection.transient_prefix.as_str()),
                None,
                &current_unix_millis_text(),
            )
    {
        webui_trace(
            "runtime_injection_audit_failed",
            format!("session={session_id} error={error}"),
        );
    }
    if runs_user_prompt_precheck {
        let context_persist_started_at = current_unix_millis_value();
        persist_webui_context_current_document(
            controller_runtime,
            &workspace_snapshot,
            session_id,
            &session_title_for_context,
        );
        webui_trace(
            "precheck_continue_context_document_persisted",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(context_persist_started_at)
            ),
        );
    }
    let now = current_unix_millis_text();
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.updated_at = now.clone();
    }
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Running;
        session.focus = String::from("正在请求");
        session.last_updated_label = String::from("刚刚");
    }
    let store_started_at = current_unix_millis_value();
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        agent_runtime,
        workspace_snapshot,
    )?;
    webui_trace(
        "precheck_continue_workbench_stored",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(store_started_at)
        ),
    );
    if runs_user_prompt_precheck {
        let state_for_persist = state.clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        spawn_service_task(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("[webui-service] precheck continue persist snapshot failed: {error}");
                    return;
                }
            };
            spawn_workspace_session_persist_task(
                state_for_persist,
                controller_runtime_for_persist,
                snapshot,
                session_id,
                Some(String::from("precheck_continue")),
            );
        });
    }
    webui_trace(
        "precheck_continue_finished",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(continue_started_at)
        ),
    );
    Ok(Some(request))
}

/// Spawns the shared precheck continuation chain for one WebUI prompt turn.
/// 为一个 WebUI 提示词回合启动共享 precheck continuation 链路。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and is forwarded to the stream task.
/// `app` 发出工作台快照，并继续传递给流任务。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` owns provider and hook runtimes.
/// `agent_runtime` 持有 Provider 与 hook 运行时。
/// `start` is the first-stage prompt turn.
/// `start` 是第一阶段提示词回合。
fn spawn_webui_precheck_continue_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    start: SharedPromptTurnStart,
) {
    let session_id = start.session_id;
    webui_trace("precheck_task_spawn", format!("session={session_id}"));
    spawn_service_task(async move {
        let precheck_started_at = current_unix_millis_value();
        webui_trace("precheck_task_run_start", format!("session={session_id}"));
        let precheck_outcome = start.precheck_request.clone().run().await;
        webui_trace(
            "precheck_task_run_finished",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(precheck_started_at)
            ),
        );
        let continue_started_at = current_unix_millis_value();
        match continue_webui_prompt_turn_after_precheck_from_state(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            start,
            precheck_outcome,
        ) {
            Ok(Some(request)) => {
                webui_trace(
                    "precheck_task_continue_finished",
                    format!(
                        "session={session_id} will_spawn_stream=true elapsed_ms={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
                spawn_agent_stream_task(app, state, controller_runtime, agent_runtime, request)
            }
            Ok(None) => {
                webui_trace(
                    "precheck_task_continue_finished",
                    format!(
                        "session={session_id} will_spawn_stream=false elapsed_ms={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
            }
            Err(error) => {
                eprintln!("[webui-service] continue WebUI prompt after precheck failed: {error}");
                if let Err(cleanup_error) = mark_webui_precheck_continuation_failed(
                    &app,
                    &state,
                    &controller_runtime,
                    &agent_runtime,
                    session_id,
                    &error,
                ) {
                    eprintln!(
                        "[webui-service] cleanup failed WebUI precheck continuation failed: {cleanup_error}"
                    );
                }
            }
        }
    });
}

/// Starts precheck dispatch or immediately continues with a skipped precheck using shared semantics.
/// 使用共享语义启动 precheck 调度，或在需要跳过时立即继续。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and runtime stream events.
/// `app` 发出工作台快照与运行时流事件。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` owns provider and hook runtimes.
/// `agent_runtime` 持有 Provider 与 hook 运行时。
/// `start` is the first-stage prompt turn.
/// `start` 是第一阶段提示词回合。
/// `precheck_plan` decides whether handlers should run.
/// `precheck_plan` 决定是否需要运行处理器。
///
/// # Returns
/// 返回值
/// Whether any continuation or precheck task was started.
/// 是否已经启动 continuation 或 precheck 任务。
pub(super) fn start_webui_precheck_or_continue(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    precheck_plan: SharedPromptPrecheckPlan,
) -> Result<bool, String> {
    let session_id = start.session_id;
    webui_trace(
        "precheck_dispatch_entered",
        format!(
            "session={} has_hooks={} skip={} labels={}",
            session_id,
            precheck_plan.has_precheck_hooks,
            precheck_plan.should_skip_precheck,
            precheck_plan.progress_labels.len()
        ),
    );
    if precheck_plan.should_skip_precheck || !precheck_plan.has_precheck_hooks {
        let skipped_continue_started_at = current_unix_millis_value();
        let precheck_outcome = start.precheck_request.clone().skip();
        if let Some(request) = continue_webui_prompt_turn_after_precheck_from_state(
            app,
            state,
            controller_runtime,
            agent_runtime,
            start,
            precheck_outcome,
        )? {
            spawn_agent_stream_task(
                app.clone(),
                state.clone(),
                controller_runtime.clone(),
                agent_runtime.clone(),
                request,
            );
            webui_trace(
                "precheck_dispatch_skipped_spawned_stream",
                format!(
                    "session={} elapsed_ms={}",
                    session_id,
                    current_unix_millis_value().saturating_sub(skipped_continue_started_at)
                ),
            );
            return Ok(true);
        }
        webui_trace(
            "precheck_dispatch_skipped_no_stream",
            format!(
                "session={} elapsed_ms={}",
                session_id,
                current_unix_millis_value().saturating_sub(skipped_continue_started_at)
            ),
        );
        return Ok(false);
    }
    spawn_webui_precheck_continue_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        agent_runtime.clone(),
        start,
    );
    webui_trace(
        "precheck_dispatch_spawned_task",
        format!("session={session_id}"),
    );
    Ok(true)
}

/// Marks a WebUI prompt turn as stopped when precheck continuation cannot be applied.
/// 当 WebUI 提示词回合无法应用 precheck continuation 时，将其标记为已停止。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained workspace snapshot.
/// `state` 持有保留的工作区快照。
/// `controller_runtime` persists the cleaned session state.
/// `controller_runtime` 持久化清理后的会话状态。
/// `agent_runtime` provides projection configuration for the emitted workbench.
/// `agent_runtime` 提供发出工作台快照所需的投影配置。
/// `session_id` selects the failed prompt turn.
/// `session_id` 选择失败的提示词回合。
/// `error` describes the continuation failure.
/// `error` 描述 continuation 失败原因。
///
/// # Returns
/// 返回值
/// `Ok(())` after the failed state is projected and persisted.
/// 失败状态完成投影与持久化后返回 `Ok(())`。
fn mark_webui_precheck_continuation_failed(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    error: &str,
) -> Result<(), String> {
    if let Ok(mut guard) = state.lock() {
        stop_webui_session_kernel_generation(&mut guard, session_id);
    }
    let mut workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    let now = current_unix_millis_text();
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.active_precheck_turn_id = None;
        append_runtime_event_projection_to_state(
            runtime_state,
            session_id,
            &now,
            &RuntimeEvent::new(
                RuntimeEventKind::Failed,
                "Precheck 结果已忽略",
                error.to_string(),
            ),
        );
        runtime_state.updated_at = now.clone();
    }
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("Precheck 结果无法应用，已停止本轮请求。");
        session.last_updated_label = String::from("刚刚");
    }
    store_current_workbench_snapshot(
        app,
        state,
        controller_runtime,
        agent_runtime,
        workspace_snapshot,
    )?;
    let state_for_persist = state.clone();
    let controller_runtime_for_persist = controller_runtime.clone();
    spawn_service_task(async move {
        let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return,
            Err(error) => {
                eprintln!("[webui-service] precheck failed persist snapshot failed: {error}");
                return;
            }
        };
        spawn_workspace_session_persist_task(
            state_for_persist,
            controller_runtime_for_persist,
            snapshot,
            session_id,
            Some(String::from("precheck_continue_failed")),
        );
    });
    Ok(())
}

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
pub(super) async fn maybe_prepare_next_webui_pending_prompt(
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
