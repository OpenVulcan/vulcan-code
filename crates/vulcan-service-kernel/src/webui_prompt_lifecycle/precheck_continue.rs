//! WebUI prompt lifecycle precheck continue helpers.
//! WebUI 提示词生命周期 precheck continue 辅助逻辑。

use vulcan_service_api::ServiceEvent;

use super::*;
use crate::{
    apply_session_phase_to_state, emit_context_injection_changed_delta,
    emit_service_event_with_trace, owning_project_id_for_session,
    retain_hydrated_conversation_windows_for_workspace,
};

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
/// `workspace_path` identifies the cached project VMM layer.
/// `workspace_path` 标识缓存中的项目 VMM 配置层。
///
/// # Returns
/// 返回值
/// A shared precheck plan matching the WebUI service shell.
/// 返回与 WebUI 服务外壳一致的共享 precheck 计划。
pub(crate) fn webui_shared_prompt_precheck_plan(
    agent_runtime: &AgentRuntimeBridge,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    workspace_path: &str,
) -> Result<SharedPromptPrecheckPlan, String> {
    let project_configuration_started_at = current_unix_millis_value();
    let project_layer = cached_project_vmm_layer(workspace_path);
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
        Some(&project_layer),
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
pub(crate) fn continue_webui_prompt_turn_after_precheck_from_state(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    precheck_outcome: ConversationPrecheckOutcome,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    let session_id = start.session_id;
    let continue_started_at = current_unix_millis_value();
    webui_after_precheck_trace(
        "after_precheck_01_continue_enter",
        format!("session={session_id} source={:?}", start.source),
    );
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
    webui_after_precheck_trace(
        "after_precheck_02_snapshot_loaded",
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
    )?;
    webui_after_precheck_trace(
        "after_precheck_03_runtime_state_ready",
        format!(
            "session={session_id} elapsed_ms_since_continue={}",
            current_unix_millis_value().saturating_sub(continue_started_at)
        ),
    );
    let tool_configuration_started_at = current_unix_millis_value();
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
    webui_after_precheck_trace(
        "after_precheck_04_tool_config_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(tool_configuration_started_at)
        ),
    );
    let project_configuration_started_at = current_unix_millis_value();
    let project_configuration = load_project_configuration(&start.workspace_path);
    webui_after_precheck_trace(
        "after_precheck_05_project_config_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(project_configuration_started_at)
        ),
    );
    let runtime_reasoning_effort = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_reasoning_effort);
    let session_title_for_context = start.session_title.clone();
    let visible_user_prompt_record_id = start.visible_user_prompt_record_id.clone();
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
    webui_after_precheck_trace(
        "after_precheck_06_shared_continue_ready",
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
        ))?;
    webui_trace(
        "precheck_continue_hook_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(vmm_snapshot_started_at)
        ),
    );
    webui_after_precheck_trace(
        "after_precheck_07_hook_snapshot_ready",
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
    if let Some(message_id) = visible_user_prompt_record_id.as_deref()
        && let Some(message) = workspace_snapshot
            .session_messages
            .get(&session_id)
            .and_then(|messages| messages.iter().find(|message| message.id == message_id))
    {
        let message_delta_started_at = current_unix_millis_value();
        emit_materialized_session_message_delta(
            app,
            state,
            session_id,
            message,
            SessionRunPhase::Preparing,
        )?;
        webui_after_precheck_trace(
            "after_precheck_08_prompt_delta_emitted",
            format!(
                "session={session_id} message_id={message_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(message_delta_started_at)
            ),
        );
    } else {
        webui_after_precheck_trace(
            "after_precheck_08_prompt_delta_skipped",
            format!("session={session_id}"),
        );
    }
    let mut request = continuation.request;
    request.controller_runtime = Some(controller_runtime.clone());
    let goal_context_started_at = current_unix_millis_value();
    request.session_goal_context = session_goal_context_for_prompt_from_state(state, session_id)?;
    webui_after_precheck_trace(
        "after_precheck_09_goal_context_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(goal_context_started_at)
        ),
    );
    let plan_context_started_at = current_unix_millis_value();
    request.session_plan_context =
        session_plan_context_for_prompt_from_state(app, state, controller_runtime, session_id)?;
    webui_after_precheck_trace(
        "after_precheck_10_plan_context_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(plan_context_started_at)
        ),
    );
    let task_session_root_started_at = current_unix_millis_value();
    request.task_session_store_root = segmented_session_directory_for_workspace_session(
        &controller_runtime.data_root,
        &workspace_snapshot,
        session_id,
    );
    webui_after_precheck_trace(
        "after_precheck_11_task_session_root_ready",
        format!(
            "session={session_id} found={} elapsed_ms={}",
            request.task_session_store_root.is_some(),
            current_unix_millis_value().saturating_sub(task_session_root_started_at)
        ),
    );
    let runtime_updates_started_at = current_unix_millis_value();
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
    webui_after_precheck_trace(
        "after_precheck_12_runtime_updates_ready",
        format!(
            "session={session_id} count={} elapsed_ms={}",
            request.runtime_system_updates.len(),
            current_unix_millis_value().saturating_sub(runtime_updates_started_at)
        ),
    );
    if let Some(injection) = request.context_injection.as_ref() {
        let data_root = controller_runtime.data_root.clone();
        let prompt_turn_id_for_audit = prompt_turn_id.clone();
        let fingerprint = injection.fingerprint.clone();
        let persistent_blocks = injection.persistent_blocks.clone();
        let transient_prefix = injection
            .has_transient()
            .then(|| injection.transient_prefix.clone());
        spawn_service_task(async move {
            let timestamp = current_unix_millis_text();
            if let Err(error) =
                vulcan_workspace_store::workspace_store::record_session_runtime_injection_applied(
                    vulcan_workspace_store::workspace_store::SessionRuntimeInjectionAppliedRecord {
                        data_root: &data_root,
                        session_id,
                        turn_id: Some(prompt_turn_id_for_audit),
                        combined_fingerprint: Some(fingerprint),
                        persistent_blocks: &persistent_blocks,
                        transient_prefix: transient_prefix.as_deref(),
                        merged_baseline_fingerprint: None,
                        timestamp: &timestamp,
                    },
                )
            {
                webui_trace(
                    "runtime_injection_audit_failed",
                    format!("session={session_id} error={error}"),
                );
            }
        });
        webui_after_precheck_trace(
            "after_precheck_13_injection_audit_spawned",
            format!("session={session_id}"),
        );
    } else {
        webui_after_precheck_trace(
            "after_precheck_13_injection_audit_skipped",
            format!("session={session_id}"),
        );
    }
    if runs_user_prompt_precheck {
        let controller_runtime_for_context = controller_runtime.clone();
        let workspace_snapshot_for_context = workspace_snapshot.clone();
        spawn_service_task(async move {
            let context_persist_started_at = current_unix_millis_value();
            persist_webui_context_current_document(
                &controller_runtime_for_context,
                &workspace_snapshot_for_context,
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
        });
        webui_after_precheck_trace(
            "after_precheck_14_context_persist_spawned",
            format!("session={session_id}"),
        );
    } else {
        webui_after_precheck_trace(
            "after_precheck_14_context_persist_skipped",
            format!("session={session_id}"),
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
    let phase_commit_started_at = current_unix_millis_value();
    commit_workspace_snapshot_and_emit_session_phase_delta(
        app,
        state,
        workspace_snapshot,
        session_id,
        SessionRunPhase::Preparing,
    )?;
    emit_context_injection_changed_delta(app, state, session_id)?;
    webui_trace(
        "precheck_continue_phase_committed",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(phase_commit_started_at)
        ),
    );
    webui_after_precheck_trace(
        "after_precheck_15_phase_delta_committed",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(phase_commit_started_at)
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
        webui_after_precheck_trace(
            "after_precheck_16_snapshot_persist_spawned",
            format!("session={session_id}"),
        );
    } else {
        webui_after_precheck_trace(
            "after_precheck_16_snapshot_persist_skipped",
            format!("session={session_id}"),
        );
    }
    webui_trace(
        "precheck_continue_finished",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(continue_started_at)
        ),
    );
    webui_after_precheck_trace(
        "after_precheck_17_continue_finished",
        format!(
            "session={session_id} elapsed_ms_since_continue={}",
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
pub(crate) fn spawn_webui_precheck_continue_task(
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
        webui_after_precheck_trace(
            "after_precheck_00_outcome_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(precheck_started_at)
            ),
        );
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
                webui_after_precheck_trace(
                    "after_precheck_18_stream_spawn_call",
                    format!(
                        "session={session_id} elapsed_ms_since_continue_call={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
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
pub(crate) fn start_webui_precheck_or_continue(
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
        // continuation is resolved explicitly so the synchronous skip path shares terminal cleanup with async precheck.
        // continuation 被显式解析，使同步跳过路径与异步 precheck 共享终态清理。
        let continuation = continue_webui_prompt_turn_after_precheck_from_state(
            app,
            state,
            controller_runtime,
            agent_runtime,
            start,
            precheck_outcome,
        );
        // continuation now carries only a successfully prepared request or an intentional no-stream result.
        // continuation 现在只携带成功准备的请求或有意不启动流的结果。
        let continuation = match continuation {
            Ok(continuation) => continuation,
            Err(error) => {
                mark_webui_precheck_continuation_failed(
                    app,
                    state,
                    controller_runtime,
                    agent_runtime,
                    session_id,
                    &error,
                )
                .map_err(|cleanup_error| {
                    format!(
                        "{error}; failed to clean rejected prompt continuation: {cleanup_error}"
                    )
                })?;
                return Err(error);
            }
        };
        if let Some(request) = continuation {
            webui_after_precheck_trace(
                "after_precheck_18_stream_spawn_call",
                format!(
                    "session={session_id} elapsed_ms_since_skip_continue={}",
                    current_unix_millis_value().saturating_sub(skipped_continue_started_at)
                ),
            );
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
    // Emits the memory-recall phase only after the shared plan confirms dispatch will run.
    // 仅在共享计划确认会调度 precheck 后才发出记忆召回阶段。
    let mut workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::MemoryRecall);
    commit_workspace_snapshot_and_emit_session_phase_delta(
        app,
        state,
        workspace_snapshot,
        session_id,
        SessionRunPhase::MemoryRecall,
    )?;
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
pub(crate) fn mark_webui_precheck_continuation_failed(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    error: &str,
) -> Result<(), String> {
    // workspace_snapshot is a detached failure draft validated before terminal runtime side effects.
    // workspace_snapshot 是在运行时终态副作用前完成校验的分离失败草稿。
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
        )?;
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
    // owning_project_id is derived from the validated failure draft before ownership is terminated.
    // owning_project_id 在终止所有权前从已校验失败草稿派生。
    let owning_project_id = owning_project_id_for_session(&workspace_snapshot, session_id);
    {
        // hook_runtime termination removes the logical turn before any later submission can reuse it.
        // hook_runtime 终止会在后续提交复用前移除逻辑回合。
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
        hook_runtime.terminate_turn(session_id, owning_project_id, error, 0);
    }
    // event is built from the same lock transaction that stops the kernel and commits the failure draft.
    // event 从停止 kernel 并提交失败草稿的同一锁事务构建。
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        retain_hydrated_conversation_windows_for_workspace(
            &mut guard.hydrated_conversation_windows,
            &workspace_snapshot,
        );
        guard.workspace_snapshot = Some(workspace_snapshot);
        stop_webui_session_kernel_generation(&mut guard, session_id);
        apply_session_phase_to_state(&mut guard, session_id, SessionRunPhase::Stopped)?
    };
    emit_service_event_with_trace(
        app,
        "workbench_phase_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
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
