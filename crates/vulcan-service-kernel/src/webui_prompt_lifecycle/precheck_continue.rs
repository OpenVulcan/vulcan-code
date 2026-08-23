//! WebUI prompt lifecycle precheck continue helpers.
//! WebUI 提示词生命周期 precheck continue 辅助逻辑。

use uuid::Uuid;
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;
use vulcan_service_api::{ServiceEvent, WorkbenchDelta, WorkbenchDeltaEnvelope};
use vulcan_workbench_model::{SessionMessageEntry, SessionMessageKind};

use super::*;
use crate::state::{
    SessionTerminalPersistenceOwner, SessionTerminalPersistenceOwnerGuard,
    SessionTurnAdmissionReservationKind,
};
use crate::{
    append_webui_session_message_entry_to_state, apply_session_phase_to_state,
    build_conversation_messages_patched_delta, conversation_messages_from_session,
    emit_context_injection_changed_delta, emit_conversation_continuation_summary_changed,
    emit_service_event_with_trace, is_prompt_free_continuation_tail_kind,
    merge_runtime_state_records_into_display_messages, owning_project_id_for_session,
    persist_latest_retained_workspace_session_update, register_session_terminal_persistence_owner,
    replace_conversation_messages_in_workbench_window,
    require_segmented_session_directory_for_retained_session,
    retain_hydrated_conversation_windows_for_workspace,
    sync_hydrated_session_dialogue_ready_marker, workbench_surface_matches_conversation,
};
use crate::{
    invalidate_execution_retry_reference, invalidate_execution_retry_runtime_state,
    mark_execution_retry_snapshot_provider_dispatch_unknown,
    refreeze_claimed_execution_retry_snapshot_with_canonical_state,
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
/// `expected_kernel_generation` binds all detached preparation writes to the kernel generation that accepted `start`.
/// `expected_kernel_generation` 将所有分离准备写入绑定到接纳 `start` 的 kernel 代际。
/// `profile_outcome` is the independently dispatched profile preparation result.
/// `profile_outcome` 是独立派发的画像准备结果。
/// `precheck_outcome` is either a real hook result or a skipped outcome.
/// `precheck_outcome` 是真实 hook 结果或跳过后的结果。
///
/// # Returns
/// 返回值
/// A provider execution request when the turn should continue, or `None` when stopped.
/// 当回合应继续时返回 Provider 执行请求；停止时返回 `None`。
pub(crate) fn continue_webui_prompt_turn_after_preparation_from_state(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    expected_kernel_generation: u64,
    profile_outcome: ConversationProfileOutcome,
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
    // capturedOwner makes the initial workspace read indivisible from exact generation validation.
    // capturedOwner 让初始工作区读取与精确代际校验不可分割。
    let Some((mut workspace_snapshot, current_kernel_generation)) =
        capture_webui_prompt_preparation_workspace(state, session_id, &start.preparation_turn_id)?
    else {
        webui_trace(
            "precheck_continue_owner_closed",
            format!(
                "session={session_id} expected_generation={expected_kernel_generation} turn_id={}",
                start.preparation_turn_id
            ),
        );
        return Ok(None);
    };
    if current_kernel_generation != expected_kernel_generation {
        webui_trace(
            "precheck_continue_generation_replaced",
            format!(
                "session={session_id} expected_generation={expected_kernel_generation} current_generation={current_kernel_generation} turn_id={}",
                start.preparation_turn_id
            ),
        );
        return Ok(None);
    }
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
            session_id,
            Some(String::from("precheck_stopped")),
        );
        return Ok(None);
    }
    ensure_webui_session_runtime_entry(
        &mut workspace_snapshot,
        session_id,
        Some(&start.preparation_turn_id),
    )?;
    // vmm_controller_registry is cloned outside command execution so the shared service lock is never nested.
    // vmm_controller_registry 在命令执行外完成克隆，避免嵌套共享服务锁。
    let vmm_controller_registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .vmm_controller_registry
        .clone();
    // vmm_controller is the sole authoritative state owner for this session.
    // vmm_controller 是该会话唯一的权威状态所有者。
    let session_directory = segmented_session_directory_for_workspace_session(
        &controller_runtime.data_root,
        &workspace_snapshot,
        session_id,
    )
    .ok_or_else(|| format!("Session `{session_id}` has no exact VMM storage locator."))?;
    let vmm_controller = vmm_controller_registry.get_or_open(
        &session_directory,
        session_id,
        &current_unix_millis_text(),
    )?;
    // vmm_state_before_precheck is retained only for post-commit audit projection.
    // vmm_state_before_precheck 仅为提交后的审计投影保留。
    let vmm_state_before_precheck = vmm_controller.snapshot()?;
    webui_after_precheck_trace(
        "after_precheck_03_runtime_state_ready",
        format!(
            "session={session_id} elapsed_ms_since_continue={}",
            current_unix_millis_value().saturating_sub(continue_started_at)
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
    // Effective disable revokes session leases before pending events bind to the current user message.
    // 生效禁用会在待处理事件绑定当前用户消息之前撤销会话租约。
    let effective_vmm_enabled = {
        let session_layer = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .map(|runtime_state| &runtime_state.vmm);
        vulcan_vmm::VmmEffectiveConfig::resolve(
            &agent_runtime.provider_configuration.vmm,
            Some(&project_configuration.vmm),
            session_layer,
        )
        .enabled
    };
    if vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled
        && !effective_vmm_enabled
    {
        webui_trace(
            "vmm_effective_disable_pending",
            format!("session={session_id}"),
        );
    }
    let runtime_reasoning_effort = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_reasoning_effort);
    let session_title_for_context = start.session_title.clone();
    let visible_user_prompt_record_id = start.visible_user_prompt_record_id.clone();
    // canonicalUserMessageId identifies the exact provider-history row that must receive post-precheck Goal text.
    // canonicalUserMessageId 标识必须接收预检后目标文本的精确 Provider 历史行。
    let canonical_user_message_id = start.canonical_user_message_id.clone();
    let prompt_turn_id = start.preparation_turn_id.clone();
    let shared_continue_started_at = current_unix_millis_value();
    // vmm_command_id makes the full Provider-boundary transition durably idempotent.
    // vmm_command_id 使完整 Provider 边界转换具备持久幂等性。
    let vmm_command_id = format!("provider_boundary:{prompt_turn_id}");
    // vmm_command_timestamp is shared by domain changes, receipt, and durable publication.
    // vmm_command_timestamp 由领域变更、回执与持久发布共享。
    let vmm_command_timestamp = current_unix_millis_text();
    // continuation_result crosses the serialized controller closure without exposing mutable VMM state.
    // continuation_result 跨越串行控制器闭包传递，且不暴露可变 VMM 状态。
    let mut continuation_result = None;
    // continuation_input is cloned because an idempotent command receipt skips the mutator and
    // therefore needs a second request-building pass against the already committed VMM state.
    // continuation_input 需要克隆，因为幂等命令回执会跳过变更器，必须基于已提交 VMM 状态
    // 再构建一次请求。
    let continuation_input = SharedPromptTurnContinueInput {
        start,
        profile_outcome,
        precheck_outcome,
        project_configuration,
        runtime_reasoning_effort,
        provider_reasoning_effort: active_reasoning_selection_effort(
            &agent_runtime.provider_configuration,
        ),
        patch_visible_user_prompt_record: runs_user_prompt_precheck,
    };
    let replay_continuation_input = continuation_input.clone();
    // vmm_commit owns both shared prompt VMM mutation and durable publication under one session lock.
    // vmm_commit 在同一会话锁内拥有共享提示词 VMM 变更与持久发布。
    let vmm_commit =
        vmm_controller.execute_command(&vmm_command_id, &vmm_command_timestamp, |candidate| {
            if vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled
                && !effective_vmm_enabled
            {
                candidate.disable(&vmm_command_timestamp)?;
            }
            // continuation is built against the exact isolated controller candidate.
            // continuation 基于精确的隔离控制器候选状态构建。
            let continuation = continue_shared_prompt_turn_after_preparation(
                &mut workspace_snapshot,
                candidate,
                continuation_input,
            )?;
            continuation_result = Some(continuation);
            Ok(())
        })?;
    // A duplicate receipt means the initial pre-provider VMM transition committed before Stop
    // cut off the request builder. Rebuild only the transient provider request from a clone of
    // the authoritative post-commit state; never run or persist the VMM mutator a second time.
    // 重复回执表示首次 Provider 前 VMM 转换已在 Stop 截断请求构建前提交。这里仅基于权威提交后
    // 状态的克隆重建临时 Provider 请求，绝不再次运行或持久化 VMM 变更器。
    let continuation = if vmm_commit.applied {
        continuation_result.ok_or_else(|| {
            format!(
                "Core VMM Provider-boundary command `{vmm_command_id}` committed without a continuation result."
            )
        })?
    } else {
        if continuation_result.is_some() {
            return Err(format!(
                "Core VMM Provider-boundary command `{vmm_command_id}` returned an invalid duplicate continuation result."
            ));
        }
        webui_trace(
            "precheck_continue_vmm_command_replayed",
            format!("session={session_id} command_id={vmm_command_id}"),
        );
        let mut replay_vmm_state = vmm_commit.state.clone();
        continue_shared_prompt_turn_after_preparation(
            &mut workspace_snapshot,
            &mut replay_vmm_state,
            replay_continuation_input,
        )
        .map_err(|error| {
            format!(
                "Core VMM Provider-boundary command `{vmm_command_id}` was already committed, but continuation reconstruction failed: {error}"
            )
        })?
    };
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
    if let Some(detail) = continuation.precheck_failure_detail.as_deref() {
        eprintln!("[webui-service] precheck hook failed but request will continue: {detail}");
        if let Err(error) = push_webui_service_diagnostic(
            state,
            format!("warning · Precheck hook 执行失败：{detail}"),
        ) {
            eprintln!("[webui-service] record precheck warning failed: {error}");
        }
    }
    if let Some(detail) = continuation.profile_failure_detail.as_deref() {
        eprintln!("[webui-service] profile hook failed but request will continue: {detail}");
        if let Err(error) = push_webui_service_diagnostic(
            state,
            format!("warning · 画像准备 Hook 执行失败：{detail}"),
        ) {
            eprintln!("[webui-service] record profile warning failed: {error}");
        }
    }
    if let Some(message_id) = visible_user_prompt_record_id.as_deref()
        && let Some(message) = workspace_snapshot
            .session_messages
            .get(&session_id)
            .and_then(|messages| messages.iter().find(|message| message.id == message_id))
    {
        let message_delta_started_at = current_unix_millis_value();
        emit_materialized_session_message_delta_if_prompt_owner_current(
            app,
            state,
            session_id,
            message,
            SessionRunPhase::Preparing,
            expected_kernel_generation,
            &prompt_turn_id,
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
    // Live VMM readiness is captured at turn admission so model-visible memory tools cannot outlive
    // a stopped or failed transport. Child tasks inherit the same immutable request fact.
    // 在回合准入时捕获 VMM 实时就绪状态，避免模型可见记忆工具在传输停止或失败后继续暴露；子任务继承同一不可变请求事实。
    request.vmm_runtime_ready = state
        .lock()
        .map_err(|error| format!("读取 VMM 实时就绪状态失败：{error}"))?
        .workbench
        .vmm_memory
        .runtime
        .process_ready;
    request.provider_id_override = agent_runtime.provider_id_override.clone();
    request.model_id_override = agent_runtime.model_id_override.clone();
    request.connection_id_override = agent_runtime.connection_id_override.clone();
    if request.provider_id_override.is_none() && request.model_id_override.is_none() {
        // sessionRuntimeState owns both the selected target and its persisted non-secret lease identity.
        // sessionRuntimeState 同时持有所选目标及其持久化非秘密租约身份。
        let session_runtime_state = workspace_snapshot.session_runtime_state.get(&session_id);
        if let Some(runtime_state) = session_runtime_state {
            request.provider_route_lease_identity = runtime_state.provider_route_lease.clone();
            if let Some((provider_id, model_id)) = runtime_state
                .active_provider_id
                .as_deref()
                .zip(runtime_state.active_model_id.as_deref())
            {
                // Exact persisted identifiers are passed through without consulting the current
                // catalog. The provider runtime either reuses the in-memory lease or proves an
                // identical restart restoration; it never degrades to another account or route.
                // 精确持久化标识不查询当前目录而直接传递。供应商运行时要么复用内存租约，
                // 要么证明重启恢复完全一致；绝不降级到其他账号或路由。
                request.provider_id_override = Some(provider_id.to_string());
                request.model_id_override = Some(model_id.to_string());
                request.connection_id_override =
                    runtime_state.active_connection_id.as_ref().cloned();
            }
        }
    } else {
        // An explicit agent-scoped model target owns an independent lease and must not inherit
        // the main session model identity.
        // 显式代理作用域模型目标拥有独立租约，不得继承主会话模型身份。
        request.provider_route_lease_identity = None;
    }
    request.controller_runtime = Some(controller_runtime.clone());
    // managedCodebaseMemory is resolved from the live controller authorization table without any workspace-path fallback.
    // managedCodebaseMemory 从实时控制器授权表解析，绝不使用工作区路径回退。
    request.managed_codebase_memory = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(workspace) = guard.workspace_snapshot.as_ref() {
            // Session mutations refresh authorization only; watcher synchronization remains project-lifecycle owned.
            // 会话变更只刷新授权；Watcher 同步仍由项目生命周期持有。
            guard
                .codebase_memory_controller
                .refresh_workspace(workspace)?;
        }
        guard
            .codebase_memory_controller
            .execution_context(session_id)
    };
    let goal_context_started_at = current_unix_millis_value();
    let (session_goal_context, goal_turn_usage_context) =
        session_goal_prompt_parts_from_state(state, session_id)?;
    request.session_goal_context = session_goal_context;
    if request.provider_visible_agent_messages.is_empty() {
        request.prompt = prepend_goal_context_to_current_prompt(
            request.prompt,
            &request.conversation_state,
            request.session_goal_context.as_ref(),
        );
        request.prompt =
            append_goal_turn_usage_context(request.prompt, goal_turn_usage_context.as_deref());
        if let Some(canonical_user_message_id) = canonical_user_message_id.as_deref() {
            // runtimeState remains the persistence owner while request history receives an identical immutable snapshot.
            // runtimeState 继续作为持久化所有者，请求历史取得完全相同的不可变快照。
            let runtime_state = workspace_snapshot
                .session_runtime_state
                .get_mut(&session_id)
                .ok_or_else(|| {
                    format!("会话 `{session_id}` 缺少运行态，无法绑定目标提示词到规范消息。")
                })?;
            replace_canonical_current_user_prompt(
                &mut runtime_state.conversation_state,
                canonical_user_message_id,
                &request.prompt,
            )?;
            // updatedAt makes the provider-history mutation visible to runtime-state persistence.
            // updatedAt 使 Provider 历史变更对运行态持久化可见。
            runtime_state.updated_at = current_unix_millis_text();
            request.conversation_state = runtime_state.conversation_state.clone();
        }
    } else {
        attach_goal_context_to_provider_visible_agent_messages(
            &mut request.provider_visible_agent_messages,
            &request.conversation_state,
            request.session_goal_context.as_ref(),
            goal_turn_usage_context.as_deref(),
        );
    }
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
    // sessionDirectory is the exact main-session storage locator reused by every hot-path audit write.
    // sessionDirectory 是由全部热路径审计写入复用的精确主会话存储定位。
    let session_directory = request.task_session_store_root.clone().ok_or_else(|| {
        format!("Session `{session_id}` has no exact storage locator before prompt continuation.")
    })?;
    request.session_storage_directory = Some(session_directory.clone());
    webui_after_precheck_trace(
        "after_precheck_11_task_session_root_ready",
        format!(
            "session={session_id} found={} elapsed_ms={}",
            request.task_session_store_root.is_some(),
            current_unix_millis_value().saturating_sub(task_session_root_started_at)
        ),
    );
    // vmm_state_after_precheck is the already persisted controller-confirmed snapshot.
    // vmm_state_after_precheck 是已经持久化并由控制器确认的快照。
    let vmm_state_after_precheck = vmm_commit.state.clone();
    // now timestamps the authoritative pre-provider workspace transition.
    // now 为权威的 Provider 调用前工作区转换提供时间戳。
    let now = current_unix_millis_text();
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.updated_at = now;
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
    // vmmProjection and contextProjection are exact, history-free payloads retained after the detached slice moves into commit.
    // vmmProjection 与 contextProjection 是在分离切片移入提交后继续保留的精确无历史载荷。
    let (vmm_layer, vmm_outbox, last_compaction_id, context_projection) = {
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .ok_or_else(|| {
                format!("Prompt continuation is missing runtime state for session `{session_id}`.")
            })?;
        (
            runtime_state.vmm.clone(),
            runtime_state.vmm_writeback_outbox.clone(),
            runtime_state.last_compaction_id.clone(),
            vulcan_workspace_store::workspace_store::project_single_session_runtime_state_update(
                session_id,
                runtime_state,
            )?,
        )
    };
    // phase_commit_started_at measures the authoritative in-memory commit and its required projections.
    // phase_commit_started_at 测量权威内存提交及其必要投影的耗时。
    let phase_commit_started_at = current_unix_millis_value();
    let phase_committed =
        commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
            app,
            state,
            workspace_snapshot,
            session_id,
            SessionRunPhase::Preparing,
            expected_kernel_generation,
            &prompt_turn_id,
        )?;
    if !phase_committed {
        webui_trace(
            "precheck_continue_phase_owner_closed",
            format!(
                "session={session_id} expected_generation={expected_kernel_generation} turn_id={prompt_turn_id}"
            ),
        );
        return Ok(None);
    }
    // The VMM Hook projection refreshes only after authoritative memory and required disk state agree.
    // VMM Hook 投影仅在权威内存与所需磁盘状态一致后刷新。
    // vmm_snapshot_started_at measures the derived Hook projection refresh.
    // vmm_snapshot_started_at 测量派生 Hook 投影刷新的耗时。
    let vmm_snapshot_started_at = current_unix_millis_value();
    agent_runtime.vmm_hook_runtime.replace_session_projection(
        session_id,
        &vmm_layer,
        &vmm_outbox,
        last_compaction_id.as_deref(),
    )?;
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
    if let Err(error) = emit_context_injection_changed_delta(app, state, session_id) {
        webui_trace(
            "context_injection_delta_emit_failed_after_commit",
            format!("session={session_id} error={error}"),
        );
    }
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
    // vmm_lifecycle_events describe only identities, hashes, counters, and byte totals.
    // vmm_lifecycle_events 仅描述身份、哈希、计数与字节总量。
    let vmm_lifecycle_events =
        super::super::vmm_runtime_audit::build_vmm_lifecycle_events_for_state_diff(
            &vmm_state_before_precheck,
            &vmm_state_after_precheck,
            Some(&prompt_turn_id),
            super::super::vmm_runtime_audit::VmmRemovedLeaseAuditAction::Revoked,
        );
    if !vmm_lifecycle_events.is_empty() {
        let vmm_audit_session_directory = session_directory.clone();
        spawn_service_task(async move {
            let timestamp = current_unix_millis_text();
            if let Err(error) = super::super::vmm_runtime_audit::record_vmm_lifecycle_events(
                &vmm_audit_session_directory,
                session_id,
                &timestamp,
                vmm_lifecycle_events,
            ) {
                webui_trace(
                    "runtime_vmm_lifecycle_audit_failed",
                    format!("session={session_id} error={error}"),
                );
            }
        });
    }
    let runtime_updates_started_at = current_unix_millis_value();
    request.runtime_system_updates =
        match vulcan_workspace_store::workspace_store::load_session_runtime_system_updates(
            &session_directory,
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
        let session_directory = session_directory.clone();
        let prompt_turn_id_for_audit = prompt_turn_id.clone();
        let fingerprint = injection.fingerprint.clone();
        let persistent_blocks = injection.persistent_blocks.clone();
        let transient_prefix = injection
            .has_transient()
            .then(|| injection.transient_prefix.clone());
        // cache_epoch_id and anchor_revision expose exact continuation coverage without memory text.
        // cache_epoch_id 与 anchor_revision 在不记录记忆正文的前提下暴露精确 continuation 覆盖范围。
        let cache_epoch_id = injection.cache_epoch_id.clone();
        let anchor_revision = injection.anchor_revision;
        // mutation_kind is serialized as the stable protocol spelling used by ProviderContextInjection.
        // mutation_kind 按 ProviderContextInjection 使用的稳定协议拼写序列化。
        let mutation_kind = Some(match injection.mutation_kind {
            vulcan_core::types::ProviderContextMutationKind::None => String::from("none"),
            vulcan_core::types::ProviderContextMutationKind::AppendOnly => {
                String::from("append_only")
            }
            vulcan_core::types::ProviderContextMutationKind::HistoricalRewrite => {
                String::from("historical_rewrite")
            }
            vulcan_core::types::ProviderContextMutationKind::EpochReset => {
                String::from("epoch_reset")
            }
        });
        // vmm_anchor_ids retain only stable identities for cache diagnostics.
        // vmm_anchor_ids 仅保留用于缓存诊断的稳定身份。
        let vmm_anchor_ids = injection
            .anchored_prefixes
            .iter()
            .map(|anchor| anchor.anchor_id.clone())
            .collect::<Vec<_>>();
        let active_memory_body_bytes = injection.active_memory_body_bytes;
        let active_memory_estimated_tokens = injection.active_memory_estimated_tokens;
        let reclaimable_anchor_bytes = injection.reclaimable_anchor_bytes;
        let reclaimable_anchor_estimated_tokens = injection.reclaimable_anchor_estimated_tokens;
        spawn_service_task(async move {
            let timestamp = current_unix_millis_text();
            if let Err(error) =
                vulcan_workspace_store::workspace_store::record_session_runtime_injection_applied(
                    vulcan_workspace_store::workspace_store::SessionRuntimeInjectionAppliedRecord {
                        session_directory: &session_directory,
                        session_id,
                        turn_id: Some(prompt_turn_id_for_audit),
                        combined_fingerprint: Some(fingerprint),
                        persistent_blocks: &persistent_blocks,
                        transient_prefix: transient_prefix.as_deref(),
                        merged_baseline_fingerprint: None,
                        cache_epoch_id,
                        anchor_revision,
                        mutation_kind,
                        vmm_anchor_ids,
                        active_memory_body_bytes,
                        active_memory_estimated_tokens,
                        reclaimable_anchor_bytes,
                        reclaimable_anchor_estimated_tokens,
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
        let context_persist_started_at = current_unix_millis_value();
        spawn_projected_webui_context_current_document_persist_task(
            session_directory.clone(),
            session_id,
            context_projection,
            session_title_for_context,
        );
        webui_trace(
            "precheck_continue_context_document_persisted",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(context_persist_started_at)
            ),
        );
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
    if runs_user_prompt_precheck {
        spawn_workspace_session_persist_task(
            state.clone(),
            controller_runtime.clone(),
            session_id,
            Some(String::from("precheck_continue")),
        );
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

/// Spawns independent profile preparation plus the optional user-precheck continuation chain.
/// 为一个 WebUI 提示词回合启动独立画像准备及可选用户 precheck 续接链路。
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
/// `expected_kernel_generation` is the immutable generation captured before asynchronous preparation dispatch.
/// `expected_kernel_generation` 是异步准备派发前捕获的不可变代际。
/// `run_precheck` decides whether the user-recall line dispatches or returns a skipped outcome.
/// `run_precheck` 决定用户召回线路是执行派发还是返回跳过结果。
pub(crate) fn spawn_webui_prompt_prepare_continue_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    expected_kernel_generation: u64,
    run_precheck: bool,
) {
    let session_id = start.session_id;
    // preparation_turn_id remains available for exact VMM failure cleanup after start moves into continuation.
    // preparation_turn_id 在 start 移入续接流程后仍用于精确的 VMM 失败清理。
    let preparation_turn_id = start.preparation_turn_id.clone();
    webui_trace(
        "prompt_prepare_task_spawn",
        format!("session={session_id} run_precheck={run_precheck}"),
    );
    spawn_service_task(async move {
        let preparation_started_at = current_unix_millis_value();
        webui_trace(
            "prompt_prepare_task_run_start",
            format!("session={session_id} run_precheck={run_precheck}"),
        );
        // profile_request always runs, while precheck_request remains governed only by user-recall policy.
        // profile_request 始终运行，而 precheck_request 仅受用户召回策略控制。
        let profile_request = start.profile_request.clone();
        let precheck_request = start.precheck_request.clone();
        // profile_outcome and precheck_outcome retain separate reports even when they execute concurrently.
        // profile_outcome 与 precheck_outcome 即使并发执行也保留相互独立的报告。
        let (profile_outcome, precheck_outcome) = if run_precheck {
            tokio::join!(profile_request.run(), precheck_request.run())
        } else {
            (profile_request.run().await, precheck_request.skip())
        };
        webui_after_precheck_trace(
            "after_precheck_00_outcome_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(preparation_started_at)
            ),
        );
        webui_trace(
            "prompt_prepare_task_run_finished",
            format!(
                "session={session_id} run_precheck={run_precheck} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(preparation_started_at)
            ),
        );
        let continue_started_at = current_unix_millis_value();
        match continue_webui_prompt_turn_after_preparation_from_state(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            start,
            expected_kernel_generation,
            profile_outcome,
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
                    "prompt_prepare_task_continue_finished",
                    format!(
                        "session={session_id} will_spawn_stream=true elapsed_ms={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
                spawn_agent_stream_task(
                    app,
                    state,
                    controller_runtime,
                    agent_runtime,
                    request,
                    expected_kernel_generation,
                    preparation_turn_id,
                )
            }
            Ok(None) => {
                webui_trace(
                    "prompt_prepare_task_continue_finished",
                    format!(
                        "session={session_id} will_spawn_stream=false elapsed_ms={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
            }
            Err(error) => {
                eprintln!(
                    "[webui-service] continue WebUI prompt after preparation failed: {error}"
                );
                let cleanup_succeeded = match mark_webui_prompt_preparation_failed(
                    &app,
                    &state,
                    &controller_runtime,
                    &agent_runtime,
                    session_id,
                    &preparation_turn_id,
                    &error,
                )
                .await
                {
                    Ok(()) => true,
                    Err(cleanup_error) => {
                        eprintln!(
                            "[webui-service] cleanup failed WebUI prompt preparation failed: {cleanup_error}"
                        );
                        false
                    }
                };
                if cleanup_succeeded {
                    // Failure cleanup closes the former turn; explicitly guided queued work must receive a new dispatch chance.
                    // 失败清理会闭合旧回合；已显式引导的排队工作必须获得新的分发机会。
                    let started_mailbox_turn = match maybe_start_webui_mailbox_turn_if_ready(
                        &app,
                        &state,
                        &controller_runtime,
                        &agent_runtime,
                        session_id,
                    )
                    .await
                    {
                        Ok(started) => started,
                        Err(dispatch_error) => {
                            eprintln!(
                                "[webui-service] resume mailbox after preparation failure failed: {dispatch_error}"
                            );
                            false
                        }
                    };
                    if !started_mailbox_turn
                        && let Err(dispatch_error) = maybe_prepare_next_webui_pending_prompt(
                            &app,
                            &state,
                            &controller_runtime,
                            &agent_runtime,
                            session_id,
                            true,
                        )
                        .await
                    {
                        eprintln!(
                            "[webui-service] resume guided prompt after preparation failure failed: {dispatch_error}"
                        );
                    }
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
/// Whether the independent prompt-preparation continuation task was started.
/// 是否已经启动独立提示词准备续接任务。
pub(crate) fn start_webui_precheck_or_continue(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    precheck_plan: SharedPromptPrecheckPlan,
) -> Result<bool, String> {
    let session_id = start.session_id;
    // expectedKernelGeneration is captured before any phase mutation so Stop can close every later detached write.
    // expectedKernelGeneration 在任何阶段变更前捕获，使 Stop 能关闭所有后续分离写入。
    let Some(expected_kernel_generation) =
        capture_webui_prompt_preparation_generation(state, session_id, &start.preparation_turn_id)?
    else {
        webui_trace(
            "precheck_dispatch_owner_closed",
            format!("session={session_id} turn_id={}", start.preparation_turn_id),
        );
        return Ok(false);
    };
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
        // Profile preparation remains asynchronous and mandatory even when user recall is skipped.
        // 即使跳过用户召回，画像准备仍保持异步且必须执行。
        spawn_webui_prompt_prepare_continue_task(
            app.clone(),
            state.clone(),
            controller_runtime.clone(),
            agent_runtime.clone(),
            start,
            expected_kernel_generation,
            false,
        );
        webui_trace(
            "precheck_dispatch_skipped_profile_task_spawned",
            format!("session={session_id}"),
        );
        return Ok(true);
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
    let phase_committed =
        commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
            app,
            state,
            workspace_snapshot,
            session_id,
            SessionRunPhase::MemoryRecall,
            expected_kernel_generation,
            &start.preparation_turn_id,
        )?;
    if !phase_committed {
        return Ok(false);
    }
    spawn_webui_prompt_prepare_continue_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        agent_runtime.clone(),
        start,
        expected_kernel_generation,
        true,
    );
    webui_trace(
        "precheck_dispatch_spawned_task",
        format!("session={session_id}"),
    );
    Ok(true)
}

/// Captures the atomic in-memory boundary committed before prompt-preparation cleanup performs external work.
/// 捕获提示词准备清理执行外部工作前原子提交的内存边界。
#[derive(Debug)]
pub(crate) struct PromptPreparationFailureBoundaryCommit {
    /// Exact owner that keeps Continue closed until the terminal snapshot is durable.
    /// 在终态快照可靠保存前保持 Continue 关闭的精确所有者。
    terminal_persistence_owner: SessionTerminalPersistenceOwner,
    /// Optional ordered phase delta produced by the same terminal-state transaction.
    /// 由同一终态事务产出的可选有序阶段增量。
    phase_event: Option<WorkbenchDeltaEnvelope>,
    /// Canonical state eligible for retry refreeze only when every safety check succeeds.
    /// 仅在所有安全检查成功时可用于重冻重试的规范状态。
    retry_refreeze_conversation_state: Option<SessionConversationState>,
    /// Project identity required to terminate the exact Hook turn.
    /// 终止精确 Hook 回合所需的项目身份。
    owning_project_id: Option<String>,
    /// Whether the detached ledger reached a replay-safe terminal boundary.
    /// 分离账本是否到达可安全重放的终态边界。
    retry_boundary_safe: bool,
    /// Non-fatal diagnostics accumulated while forcing the boundary closed.
    /// 强制关闭边界时累计的非致命诊断。
    diagnostics: Vec<String>,
}

/// Atomically stops one preparation turn, registers its persistence owner, and commits a fail-closed terminal draft.
/// 原子停止一个准备回合、登记其持久化所有者，并提交失败关闭的终态草稿。
///
/// # Parameters
/// 参数
/// `state` owns the exact kernel, owner tables, retained workspace, and visible workbench projection.
/// `state` 持有精确 kernel、所有者表、保留工作区与可见工作台投影。
/// `session_id` and `failed_turn_id` identify the preparation turn being terminalized.
/// `session_id` 与 `failed_turn_id` 标识正在终态化的准备回合。
/// `error` is the controlled failure diagnostic, while `provider_boundary_dispatch_unknown` closes replay after an uncertain Provider boundary.
/// `error` 是受控失败诊断；`provider_boundary_dispatch_unknown` 在 Provider 边界不确定时关闭重放。
///
/// # Returns
/// 返回值
/// The exact persistence owner and detached facts needed by the asynchronous settlement phase.
/// 返回异步结算阶段所需的精确持久化所有者与分离事实。
pub(crate) fn commit_prompt_preparation_failure_boundary(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    failed_turn_id: &str,
    error: &str,
    provider_boundary_dispatch_unknown: bool,
) -> Result<PromptPreparationFailureBoundaryCommit, String> {
    // guard serializes owner validation, kernel stop, retry closure, and retained-state replacement.
    // guard 串行化所有者校验、kernel 停止、重试关闭与保留状态替换。
    let mut guard = state
        .lock()
        .map_err(|lock_error| format!("failed to lock service state: {lock_error}"))?;
    if guard.active_stream_tasks.contains_key(&session_id) {
        return Err(format!(
            "Prompt preparation cleanup rejected an active stream owner for session `{session_id}`."
        ));
    }
    if let Some(reservation) = guard.session_turn_admission_reservations.get(&session_id)
        && reservation.kind != SessionTurnAdmissionReservationKind::PromptSubmission
    {
        return Err(format!(
            "Prompt preparation cleanup conflicts with {} for session `{session_id}`.",
            reservation.kind.diagnostic_label()
        ));
    }
    // active_kernel_turn_id proves this cleanup cannot stop a replacement turn.
    // active_kernel_turn_id 证明当前清理不会停止替代回合。
    let active_kernel_turn_id = guard
        .session_runtime_kernels
        .get(&session_id)
        .and_then(|kernel| kernel.active_turn().map(|turn| turn.turn_id.clone()));
    if active_kernel_turn_id
        .as_deref()
        .is_some_and(|turn_id| turn_id != failed_turn_id)
    {
        return Err(format!(
            "Prompt preparation cleanup turn mismatch for session `{session_id}`: failed=`{failed_turn_id}` active=`{}`.",
            active_kernel_turn_id.as_deref().unwrap_or_default()
        ));
    }
    if let Some(existing_owner) = guard.session_terminal_persistence_owners.get(&session_id) {
        return Err(format!(
            "Prompt preparation cleanup conflicts with terminal owner `{existing_owner:?}` for session `{session_id}`."
        ));
    }
    // terminal_persistence_owner uses a unique transaction identity to avoid generation-zero ABA cleanup.
    // terminal_persistence_owner 使用唯一事务身份，避免 generation-zero ABA 清理。
    let terminal_persistence_owner =
        SessionTerminalPersistenceOwner::PromptPreparation(Uuid::new_v4());
    if guard.workspace_snapshot.is_none() {
        if active_kernel_turn_id.is_some() {
            register_session_terminal_persistence_owner(
                &mut guard,
                session_id,
                terminal_persistence_owner,
            )?;
            stop_webui_session_kernel_generation(&mut guard, session_id);
            return Err(format!(
                "Prompt preparation workspace is unavailable for session `{session_id}`; the kernel was stopped and terminal owner `{terminal_persistence_owner:?}` was retained."
            ));
        }
        return Err(String::from("workspace snapshot is not ready"));
    }
    register_session_terminal_persistence_owner(
        &mut guard,
        session_id,
        terminal_persistence_owner,
    )?;
    stop_webui_session_kernel_generation(&mut guard, session_id);
    // workspace_snapshot starts from the kernel-stopped authoritative state mirrored by the same lock.
    // workspace_snapshot 从同一锁内已镜像 kernel 停止事实的权威状态开始。
    let mut workspace_snapshot = guard
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot disappeared after kernel stop"))?;
    // now is shared by the error record, ledger boundary, and retained session metadata.
    // now 由错误记录、账本边界与保留会话元数据共享。
    let now = current_unix_millis_text();
    // diagnostics records every degraded projection while terminal ownership remains closed.
    // diagnostics 记录终态所有权保持关闭期间的每个降级投影。
    let mut diagnostics = Vec::new();
    // retry_refreeze_conversation_state remains absent unless terminal ledger safety is proven.
    // retry_refreeze_conversation_state 仅在终态账本安全性得到证明后才会存在。
    let mut retry_refreeze_conversation_state = None;
    // retry_boundary_safe is promoted only after the terminal ledger revision commits.
    // retry_boundary_safe 仅在终态账本修订提交后才会提升为真。
    let mut retry_boundary_safe = false;
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.active_precheck_turn_id = None;
        // failure_entry creates immediate, durable feedback for asynchronous post-ACK failures.
        // failure_entry 为 ACK 后异步失败创建即时且可靠的反馈。
        let failure_entry = SessionMessageEntry::new(
            SessionMessageKind::Error,
            "请求准备失败",
            format!("本轮请求未能继续，已安全停止。原因：{error}"),
        );
        if let Err(projection_error) =
            append_webui_session_message_entry_to_state(runtime_state, session_id, failure_entry)
        {
            diagnostics.push(format!(
                "Prompt preparation failure message projection failed: {projection_error}"
            ));
        }
        // canonical_tail_kind is derived only after the failure record attempt completes.
        // canonical_tail_kind 仅在失败记录尝试完成后从规范状态派生。
        let canonical_tail_kind = runtime_state
            .conversation_state
            .records
            .iter()
            .rev()
            .find(|record| {
                record.visible
                    && record.record_kind
                        == vulcan_workbench_model::SessionMessageRecordKind::Transcript
            })
            .map(|record| match record.kind {
                vulcan_workbench_model::SessionMessageKind::Assistant => "assistant",
                vulcan_workbench_model::SessionMessageKind::User => "user",
                vulcan_workbench_model::SessionMessageKind::Error => "error",
            })
            .unwrap_or("empty")
            .to_string();
        // replay_safety_proven requires the exact immutable retry reference to carry safe evidence.
        // replay_safety_proven 要求精确不可变重试引用携带安全证据。
        let replay_safety_proven = runtime_state
            .execution_retry_snapshot
            .as_ref()
            .is_some_and(|reference| reference.replay_safety_proven());
        // provider_call_still_active is an explicit uncertain side-effect boundary and cannot be terminalized as replayable.
        // provider_call_still_active 是明确的不确定副作用边界，不能被终态化为可重放。
        let provider_call_still_active =
            runtime_state.provider_call_ledger.in_flight_call_count() != 0;
        // terminal_at_ms validates the timestamp before the monotonic ledger revision is advanced.
        // terminal_at_ms 在推进单调账本修订前校验时间戳。
        let terminal_at_ms = now.parse::<i64>();
        if provider_call_still_active {
            diagnostics.push(String::from(
                "ERR_CONVERSATION_PREPARATION_FAILED_PROVIDER_CALL_STILL_ACTIVE",
            ));
            let _ = invalidate_execution_retry_runtime_state(
                runtime_state,
                "provider_dispatch_outcome_unknown",
                "Prompt preparation failed while a Provider call was still active.",
                now.clone(),
            );
        } else if let Ok(terminal_at_ms) = terminal_at_ms {
            // continuation_eligible requires the exact prompt-free tail, replay proof, and a known pre-dispatch boundary.
            // continuation_eligible 要求精确无提示词尾部、重放证明与确定的分派前边界。
            let continuation_eligible =
                is_prompt_free_continuation_tail_kind(canonical_tail_kind.as_str())
                    && replay_safety_proven
                    && !provider_boundary_dispatch_unknown;
            if runtime_state
                .provider_call_ledger
                .mark_continuation_terminal(
                    Some(failed_turn_id.to_string()),
                    String::from("preparation_failed"),
                    terminal_at_ms,
                    canonical_tail_kind,
                    continuation_eligible,
                )
            {
                retry_boundary_safe = !provider_boundary_dispatch_unknown;
                retry_refreeze_conversation_state = Some(runtime_state.conversation_state.clone());
            } else {
                diagnostics.push(String::from(
                    "ERR_CONVERSATION_PREPARATION_FAILED_CONTINUATION_REVISION_EXHAUSTED",
                ));
                let _ = invalidate_execution_retry_runtime_state(
                    runtime_state,
                    "retry_snapshot_transition_failed",
                    "Prompt preparation failure could not advance the continuation ledger.",
                    now.clone(),
                );
            }
        } else if let Err(timestamp_error) = terminal_at_ms {
            diagnostics.push(format!(
                "Prompt preparation terminal timestamp is invalid: {timestamp_error}"
            ));
            let _ = invalidate_execution_retry_runtime_state(
                runtime_state,
                "retry_snapshot_transition_failed",
                "Prompt preparation failure produced an invalid terminal timestamp.",
                now.clone(),
            );
        }
        runtime_state.updated_at = now.clone();
    } else {
        diagnostics.push(String::from(
            "Prompt preparation failure runtime state is unavailable.",
        ));
    }
    // runtime_state moves temporarily so failure display projection never clones canonical history.
    // runtime_state 临时移出，使失败显示投影绝不复制规范历史。
    if let Some(runtime_state) = workspace_snapshot.session_runtime_state.remove(&session_id) {
        merge_runtime_state_records_into_display_messages(
            &mut workspace_snapshot,
            session_id,
            &runtime_state,
        );
        workspace_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
        sync_hydrated_session_dialogue_ready_marker(&mut workspace_snapshot, session_id);
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
        session.focus = String::from("提示词准备结果无法应用，已停止本轮请求。");
        session.last_updated_label = String::from("刚刚");
    }
    // owning_project_id is derived before the committed workspace replaces shared state.
    // owning_project_id 在已提交工作区替换共享状态前派生。
    let owning_project_id = owning_project_id_for_session(&workspace_snapshot, session_id);
    retain_hydrated_conversation_windows_for_workspace(
        &mut guard.hydrated_conversation_windows,
        &workspace_snapshot,
    );
    guard.workspace_snapshot = Some(workspace_snapshot);
    // phase_event is best-effort because the owner marker and stopped kernel already fail closed.
    // phase_event 为尽力投影，因为所有者标记与已停止 kernel 已经失败关闭。
    let phase_event =
        match apply_session_phase_to_state(&mut guard, session_id, SessionRunPhase::Stopped) {
            Ok(event) => Some(event),
            Err(phase_error) => {
                diagnostics.push(format!(
                    "Prompt preparation stopped-phase projection failed: {phase_error}"
                ));
                None
            }
        };
    Ok(PromptPreparationFailureBoundaryCommit {
        terminal_persistence_owner,
        phase_event,
        retry_refreeze_conversation_state,
        owning_project_id,
        retry_boundary_safe,
        diagnostics,
    })
}

/// Emits an immediate message-window patch after the failure record enters retained workspace state.
/// 在失败记录进入保留工作区状态后立即发出消息窗口补丁。
///
/// # Parameters
/// 参数
/// `app` emits the ordered patch, `state` owns its authoritative revision, and `session_id` selects the visible conversation.
/// `app` 发出有序补丁，`state` 持有其权威修订，`session_id` 选择可见会话。
///
/// # Returns
/// 返回值
/// Success when no focused surface needs a patch or the exact patch was emitted.
/// 没有聚焦界面需要补丁或精确补丁已发出时返回成功。
fn emit_prompt_preparation_failure_message_window_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    // conversation_id is the stable workbench surface key for the failed session.
    // conversation_id 是失败会话的稳定工作台界面键。
    let conversation_id = session_id.to_string();
    // event is produced only when the currently focused surface has a real message difference.
    // event 仅在当前聚焦界面存在真实消息差异时产出。
    let event = {
        let mut guard = state
            .lock()
            .map_err(|lock_error| format!("failed to lock service state: {lock_error}"))?;
        // before_visible_messages captures the exact client-visible baseline for node-level diffing.
        // before_visible_messages 捕获用于节点级差异计算的精确客户端可见基线。
        let before_visible_messages = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect::<Vec<_>>();
        // messages projects the committed retained display records without reconstructing canonical history.
        // messages 从已提交保留显示记录投影，不重建规范历史。
        let messages = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_messages.get(&session_id))
            .map(|entries| conversation_messages_from_session(&conversation_id, entries, false))
            .unwrap_or_default();
        if workbench_surface_matches_conversation(&guard.workbench, &conversation_id) {
            build_conversation_messages_patched_delta(
                &conversation_id,
                before_visible_messages.as_slice(),
                messages.as_slice(),
            )
            .map(|message_patch| {
                guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
                // generation binds the retained workbench mutation to its emitted delta.
                // generation 将保留工作台变更绑定到其发出增量。
                let generation = guard.workbench_generation;
                guard.workbench.revision = generation;
                replace_conversation_messages_in_workbench_window(
                    &mut guard.workbench.messages,
                    &conversation_id,
                    messages,
                );
                WorkbenchDeltaEnvelope {
                    generation,
                    delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
                }
            })
        } else {
            None
        }
    };
    if let Some(event) = event {
        emit_service_event_with_trace(
            app,
            "prompt_preparation_failure_messages_patch_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    Ok(())
}

/// Marks a WebUI prompt turn as stopped when either preparation outcome cannot be applied.
/// 当 WebUI 提示词回合无法应用任一准备线路结果时，将其标记为已停止。
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
/// `failed_turn_id` identifies the exact logical VMM owner that must be abandoned.
/// `failed_turn_id` 标识必须放弃的精确 VMM 逻辑所有者。
/// `error` describes the continuation failure.
/// `error` 描述 continuation 失败原因。
///
/// # Returns
/// 返回值
/// `Ok(())` after the failed state is projected and persisted.
/// 失败状态完成投影与持久化后返回 `Ok(())`。
pub(crate) async fn mark_webui_prompt_preparation_failed(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    failed_turn_id: &str,
    error: &str,
) -> Result<(), String> {
    // A committed provider-boundary receipt without a rebuilt request leaves dispatch outcome
    // unknown, so the cleanup path must close Continue instead of reopening the same boundary.
    // 已提交 Provider 边界回执但未能重建请求表示分派结果未知，清理路径必须关闭 Continue，
    // 不能重新打开同一边界。
    let provider_boundary_dispatch_unknown =
        error.contains("Provider-boundary command `") && error.contains("was already committed");
    // boundary_commit atomically closes execution ownership before any external cleanup can fail.
    // boundary_commit 在任何外部清理可能失败前原子关闭执行所有权。
    let boundary_commit = commit_prompt_preparation_failure_boundary(
        state,
        session_id,
        failed_turn_id,
        error,
        provider_boundary_dispatch_unknown,
    )?;
    // terminal_persistence_guard keeps every replacement submission and Continue claim blocked until durable settlement.
    // terminal_persistence_guard 在可靠结算前阻止所有替代提交与 Continue 领取。
    let terminal_persistence_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        boundary_commit.terminal_persistence_owner,
    )?;
    if let Some(phase_event) = boundary_commit.phase_event {
        emit_service_event_with_trace(
            app,
            "workbench_phase_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(phase_event)),
        );
    }
    // diagnostics retains degraded external cleanup and projection results without reopening execution.
    // diagnostics 保留降级的外部清理与投影结果，同时不重新开放执行。
    let mut diagnostics = boundary_commit.diagnostics;
    if let Err(message_projection_error) =
        emit_prompt_preparation_failure_message_window_delta(app, state, session_id)
    {
        diagnostics.push(format!(
            "Prompt preparation failure message delta failed: {message_projection_error}"
        ));
    }
    // cleanup_updated_at provides one stable command timestamp for the idempotent VMM abandonment.
    // cleanup_updated_at 为幂等 VMM 放弃命令提供一个稳定时间戳。
    let cleanup_updated_at = current_unix_millis_text();
    // external_cleanup_safe remains true only when both VMM and Hook ownership are conclusively closed.
    // external_cleanup_safe 仅在 VMM 与 Hook 所有权均被确定关闭时保持为真。
    let mut external_cleanup_safe = true;
    if let Err(vmm_error) = abandon_failed_webui_prompt_vmm_turn(
        state,
        controller_runtime,
        session_id,
        failed_turn_id,
        &cleanup_updated_at,
    ) {
        external_cleanup_safe = false;
        diagnostics.push(format!(
            "Prompt preparation VMM cleanup failed: {vmm_error}"
        ));
    }
    match agent_runtime.conversation_hook_runtime.lock() {
        Ok(mut hook_runtime) => {
            hook_runtime.terminate_turn(session_id, boundary_commit.owning_project_id, error, 0);
        }
        Err(hook_error) => {
            external_cleanup_safe = false;
            diagnostics.push(format!(
                "Prompt preparation Hook cleanup failed: {hook_error}"
            ));
        }
    }
    if provider_boundary_dispatch_unknown {
        if let Err(provider_boundary_error) =
            mark_execution_retry_snapshot_provider_dispatch_unknown(
                state,
                controller_runtime,
                session_id,
                error,
            )
        {
            diagnostics.push(format!(
                "Prompt preparation Provider-boundary closure failed: {provider_boundary_error}"
            ));
            if let Err(invalidation_error) = invalidate_execution_retry_reference(
                state,
                session_id,
                "provider_dispatch_outcome_unknown",
                "Provider dispatch outcome remained unknown during prompt-preparation cleanup.",
            ) {
                diagnostics.push(format!(
                    "Prompt preparation Provider-boundary invalidation failed: {invalidation_error}"
                ));
            }
        }
    } else if boundary_commit.retry_boundary_safe && external_cleanup_safe {
        if let Some(conversation_state) = boundary_commit.retry_refreeze_conversation_state.as_ref()
            && let Err(refreeze_failure) =
                refreeze_claimed_execution_retry_snapshot_with_canonical_state(
                    state,
                    controller_runtime,
                    session_id,
                    conversation_state,
                    None,
                    error,
                )
        {
            diagnostics.push(format!(
                "Prompt preparation retry refreeze failed: {refreeze_failure}"
            ));
            // A failed refreeze closes the claim in-process instead of leaving duplicate requests in admission-in-flight.
            // 重冻失败会在进程内关闭领取，避免重复请求停留在准入进行中。
            if let Err(invalidation_error) = invalidate_execution_retry_reference(
                state,
                session_id,
                refreeze_failure.blocked_reason(),
                refreeze_failure.error_summary(),
            ) {
                diagnostics.push(format!(
                    "Prompt preparation retry invalidation failed: {invalidation_error}"
                ));
            }
        }
    } else if let Err(invalidation_error) = invalidate_execution_retry_reference(
        state,
        session_id,
        "retry_snapshot_transition_failed",
        "Prompt-preparation cleanup could not prove a safe replay boundary.",
    ) {
        diagnostics.push(format!(
            "Prompt preparation unsafe-boundary invalidation failed: {invalidation_error}"
        ));
    }
    // exact retained persistence captures only the target session after every boundary invalidation.
    // 精确保留持久化仅在全部边界失效变更完成后捕获目标会话。
    if let Err(persist_error) = persist_latest_retained_workspace_session_update(
        state,
        controller_runtime.clone(),
        session_id,
    )
    .await
    {
        if let Err(projection_error) =
            emit_conversation_continuation_summary_changed(app, state, session_id)
        {
            eprintln!(
                "[webui-service] continuation summary refresh after failure persistence error failed: {projection_error}"
            );
        }
        return Err(format!(
            "Prompt preparation terminal persistence failed: {persist_error}; terminal owner retained"
        ));
    }
    terminal_persistence_guard.release()?;
    if let Err(projection_error) =
        emit_conversation_continuation_summary_changed(app, state, session_id)
    {
        eprintln!(
            "[webui-service] continuation summary refresh after precheck failure failed: {projection_error}"
        );
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics.join("; "))
    }
}

/// Abandons the exact failed prompt turn through the session's sole Core VMM controller.
/// 通过会话唯一的 Core VMM Controller 放弃精确的失败提示词回合。
///
/// # Parameters
/// 参数
/// `state` owns the process-wide session controller registry.
/// `state` 持有进程级会话 Controller Registry。
/// `controller_runtime` fixes the sole durable data root for this process.
/// `controller_runtime` 固定本进程唯一的持久数据根目录。
/// `session_id` selects the exact session controller.
/// `session_id` 选择精确的会话 Controller。
/// `failed_turn_id` is the logical turn whose active ownership must be cleared.
/// `failed_turn_id` 是必须清除活动所有权的逻辑回合。
/// `updated_at` is the shared failure-cleanup timestamp.
/// `updated_at` 是共享的失败清理时间戳。
///
/// # Returns
/// 返回值
/// Success when no VMM turn was registered or the exact active turn is durably abandoned.
/// 未登记 VMM 回合或精确活动回合已持久放弃时返回成功。
fn abandon_failed_webui_prompt_vmm_turn(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    failed_turn_id: &str,
    updated_at: &str,
) -> Result<(), String> {
    // registry is cloned without retaining the shared service lock during controller I/O.
    // registry 在 Controller I/O 期间不保留共享服务锁。
    let (registry, session_directory) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let session_directory = require_segmented_session_directory_for_retained_session(
            &guard,
            &controller_runtime.data_root,
            session_id,
        )?;
        (guard.vmm_controller_registry.clone(), session_directory)
    };
    // controller is the sole authority for active VMM logical-turn ownership.
    // controller 是 VMM 活动逻辑回合所有权的唯一权威。
    let controller = registry.get_or_open(&session_directory, session_id, updated_at)?;
    // active_turn_id distinguishes failures before the Provider-boundary command from committed failures.
    // active_turn_id 区分 Provider 边界命令前的失败与已经提交的失败。
    let active_turn_id = controller.snapshot()?.active_turn_id;
    let Some(active_turn_id) = active_turn_id else {
        return Ok(());
    };
    if active_turn_id != failed_turn_id {
        return Err(format!(
            "Failed prompt turn `{failed_turn_id}` does not own Core VMM active turn `{active_turn_id}` for session `{session_id}`."
        ));
    }
    // command_id makes repeated cleanup harmless after the durable abandonment succeeds.
    // command_id 使持久放弃成功后的重复清理不产生副作用。
    let command_id = format!("preparation_failed:{failed_turn_id}");
    controller.execute_command(&command_id, updated_at, |candidate| {
        candidate.abandon_active_turn(failed_turn_id, updated_at)?;
        Ok(())
    })?;
    Ok(())
}
