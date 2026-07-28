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
    let vmm_controller = vmm_controller_registry.get_or_open(
        &controller_runtime.data_root,
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
                SharedPromptTurnContinueInput {
                    start,
                    profile_outcome,
                    precheck_outcome,
                    project_configuration,
                    runtime_reasoning_effort,
                    provider_reasoning_effort: active_reasoning_selection_effort(
                        &agent_runtime.provider_configuration,
                    ),
                    patch_visible_user_prompt_record: runs_user_prompt_precheck,
                },
            )?;
            continuation_result = Some(continuation);
            Ok(())
        })?;
    if !vmm_commit.applied {
        return Err(format!(
            "Core VMM Provider-boundary command `{vmm_command_id}` was already committed; duplicate Provider dispatch is rejected."
        ));
    }
    // continuation must exist because a newly applied command always runs its mutator.
    // continuation 必须存在，因为新应用命令始终会运行其变更器。
    let continuation = continuation_result.ok_or_else(|| {
        format!(
            "Core VMM Provider-boundary command `{vmm_command_id}` committed without a continuation result."
        )
    })?;
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
    request.provider_id_override = agent_runtime.provider_id_override.clone();
    request.model_id_override = agent_runtime.model_id_override.clone();
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
    // phase_commit_started_at measures the authoritative in-memory commit and its required projections.
    // phase_commit_started_at 测量权威内存提交及其必要投影的耗时。
    let phase_commit_started_at = current_unix_millis_value();
    commit_workspace_snapshot_and_emit_session_phase_delta(
        app,
        state,
        workspace_snapshot.clone(),
        session_id,
        SessionRunPhase::Preparing,
    )?;
    // The VMM Hook projection refreshes only after authoritative memory and required disk state agree.
    // VMM Hook 投影仅在权威内存与所需磁盘状态一致后刷新。
    // vmm_snapshot_started_at measures the derived Hook projection refresh.
    // vmm_snapshot_started_at 测量派生 Hook 投影刷新的耗时。
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
        let data_root = controller_runtime.data_root.clone();
        spawn_service_task(async move {
            let timestamp = current_unix_millis_text();
            if let Err(error) = super::super::vmm_runtime_audit::record_vmm_lifecycle_events(
                &data_root,
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
                        data_root: &data_root,
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
/// `run_precheck` decides whether the user-recall line dispatches or returns a skipped outcome.
/// `run_precheck` 决定用户召回线路是执行派发还是返回跳过结果。
pub(crate) fn spawn_webui_prompt_prepare_continue_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    start: SharedPromptTurnStart,
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
                spawn_agent_stream_task(app, state, controller_runtime, agent_runtime, request)
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
                if let Err(cleanup_error) = mark_webui_prompt_preparation_failed(
                    &app,
                    &state,
                    &controller_runtime,
                    &agent_runtime,
                    session_id,
                    &preparation_turn_id,
                    &error,
                ) {
                    eprintln!(
                        "[webui-service] cleanup failed WebUI prompt preparation failed: {cleanup_error}"
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
    commit_workspace_snapshot_and_emit_session_phase_delta(
        app,
        state,
        workspace_snapshot,
        session_id,
        SessionRunPhase::MemoryRecall,
    )?;
    spawn_webui_prompt_prepare_continue_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        agent_runtime.clone(),
        start,
        true,
    );
    webui_trace(
        "precheck_dispatch_spawned_task",
        format!("session={session_id}"),
    );
    Ok(true)
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
pub(crate) fn mark_webui_prompt_preparation_failed(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    failed_turn_id: &str,
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
                "提示词准备结果已忽略",
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
        session.focus = String::from("提示词准备结果无法应用，已停止本轮请求。");
        session.last_updated_label = String::from("刚刚");
    }
    // owning_project_id is derived from the validated failure draft before ownership is terminated.
    // owning_project_id 在终止所有权前从已校验失败草稿派生。
    let owning_project_id = owning_project_id_for_session(&workspace_snapshot, session_id);
    abandon_failed_webui_prompt_vmm_turn(
        state,
        controller_runtime,
        session_id,
        failed_turn_id,
        &now,
    )?;
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
    let registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .vmm_controller_registry
        .clone();
    // controller is the sole authority for active VMM logical-turn ownership.
    // controller 是 VMM 活动逻辑回合所有权的唯一权威。
    let controller = registry.get_or_open(&controller_runtime.data_root, session_id, updated_at)?;
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
