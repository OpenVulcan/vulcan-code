//! Conversation run service handlers.
//! 会话运行服务处理器。

use super::*;
use crate::context_compaction::resolve_webui_context_compaction_model_target;
use crate::session_goal_lifecycle::{
    current_v1_goal_unix_millis, session_goal_summary_from_v1_handle, session_v1_handle_from_state,
};
use crate::session_request_config_v1::{VmmSessionPolicyField, set_vmm_session_policy_value_v1};
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};
use vulcan_session_core::v1::collaboration::{
    CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
};

mod child_question_stop;
mod question_stop;
pub(crate) use child_question_stop::stop_owned_child_question_execution;

/// Stops maintenance and question-only work across the actual child tree without reopening completed business.
/// 在实际子任务树中停止维护及仅答疑工作，不重开已完成业务。
/// `app`, `state`, and `runtime` supply service ownership; `session_id` selects the root and all descendants.
/// `app`、`state` 和 `runtime` 提供服务归属；`session_id` 选择根及全部后代。
/// Business tasks must already be stopped by the caller; errors retain the exact auxiliary repair owner.
/// 调用者必须已停止业务任务；错误保留精确辅助修复所有者。
pub(crate) async fn stop_owned_child_auxiliary_tree(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime,
    session_id: SessionId,
) -> Result<(), String> {
    // A maintenance storage failure cannot leave other selected question workers running.
    // 维护存储失败不能使其他已选答疑工作任务继续运行。
    let maintenance_failure =
        crate::agent_collaboration::child_maintenance::stop_tree(state, runtime, session_id)
            .await
            .err();
    // Completed business remains in this registry while its question-only execution is running.
    // 仅答疑执行运行期间，已完成业务仍保留在此注册表中。
    let registry = runtime.subagent_registry_snapshot()?;
    // A root can be an ordinary conversation or a nested child.
    // 根可以是普通会话或嵌套子任务。
    let mut pending = vec![session_id];
    // Visit each stable session only once, independent of panel display state.
    // 独立于面板展示状态，仅访问每个稳定会话一次。
    let mut visited = BTreeSet::new();
    // Drain unrelated selected children even if one requires retained terminal repair.
    // 即使某个子任务需要保留终态修复，也排空其他已选子任务。
    let mut failures = maintenance_failure.into_iter().collect::<Vec<_>>();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        pending.extend(
            registry
                .list_for_parent(current.into())
                .into_iter()
                .map(|entry| entry.session_id.as_uuid()),
        );
        if let Err(error) = stop_owned_child_question_execution(app, state, current).await {
            failures.push(format!("{current}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Child auxiliary stop requires repair: {}",
            failures.join("; ")
        ))
    }
}

/// Provider-visible semantic recorded when user Stop pauses autonomous Goal execution.
/// 用户停止操作暂停目标自动执行时记录的 Provider 可见语义。
const GOAL_STOPPED_PROVIDER_CONTEXT: &str = "The session goal was paused because the user stopped the run. Do not continue autonomous goal work until it is resumed.";

/// Requested stop behavior for a WebUI conversation run.
/// WebUI 会话运行请求的停止行为。
#[derive(Debug, Clone, Copy, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StopConversationRunMode {
    /// Ask the caller for confirmation when live child agents are still running.
    /// 当仍有运行中智能体时要求调用方先确认。
    #[default]
    AskIfRunningSubagents,
    /// Stop only the main conversation run and leave child agents running.
    /// 只停止主会话运行，并保留智能体继续运行。
    MainOnly,
    /// Stop the main conversation run and every running child agent.
    /// 停止主会话运行以及所有运行中的智能体。
    MainAndSubagents,
}
/// RPC response returned by one conversation stop request.
/// 单次会话停止请求返回的 RPC 响应。
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum StopConversationRunResponse {
    /// The stop request finished and carries the rebuilt workbench snapshot.
    /// 停止请求已完成，并携带重建后的工作台快照。
    Stopped {
        /// Full workbench snapshot after the stop mutation.
        /// 停止变更后的完整工作台快照。
        workbench: Box<WorkbenchSnapshot>,
    },
    /// The stop request needs a user decision before mutating runtime state.
    /// 停止请求需要用户决策后才能修改运行态。
    NeedsConfirmation {
        /// Running child agents that make the stop choice ambiguous.
        /// 导致停止选择存在歧义的运行中智能体列表。
        running_subagents: Vec<StopConversationRunSubagentSummary>,
    },
}

/// User-facing summary for a running child agent during stop confirmation.
/// 停止确认期间展示给用户的运行中智能体摘要。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct StopConversationRunSubagentSummary {
    /// Direct parent conversation id for the child agent task.
    /// 智能体任务的直接父会话标识。
    pub(crate) parent_conversation_id: String,
    /// Stable child task session id.
    /// 稳定的子任务会话标识。
    pub(crate) task_id: String,
    /// User-facing child agent label.
    /// 面向用户的智能体标签。
    pub(crate) label: String,
}

/// Terminates the exact active conversation Hook turn retained by a stopped execution.
/// 终止已停止执行所保留的精确活动会话 Hook 回合。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns Hook state, `session_id` selects the conversation, and `project_id` retains ownership metadata.
/// `agent_runtime` 持有 Hook 状态，`session_id` 选择会话，`project_id` 保留归属元数据。
/// `expected_turn_id` is the exact Hook owner captured when stop began.
/// `expected_turn_id` 是停止开始时捕获的精确 Hook 所有者。
/// `reason` is the stable termination explanation emitted to Hook observers.
/// `reason` 是发送给 Hook 观察者的稳定终止说明。
///
/// # Returns
/// 返回值
/// `true` when an active turn was terminated, `false` when already closed, or a lock error.
/// 活动回合被终止时返回 `true`，已经关闭时返回 `false`，否则返回锁错误。
pub(crate) fn terminate_active_conversation_hook_turn_after_stop(
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    project_id: Option<String>,
    expected_turn_id: Option<&str>,
    reason: &str,
) -> Result<bool, String> {
    // hookRuntime is the unique in-memory owner of logical conversation Hook turns.
    // hookRuntime 是逻辑会话 Hook 回合的唯一内存所有者。
    let mut hook_runtime = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?;
    // expectedTurnId must exist before stop may mutate the retained Hook owner.
    // expectedTurnId 必须存在，停止操作才可以变更保留的 Hook 所有者。
    let Some(expected_turn_id) = expected_turn_id else {
        return Ok(false);
    };
    if hook_runtime.active_turn_id(session_id) != Some(expected_turn_id) {
        return Ok(false);
    }
    hook_runtime.terminate_turn(session_id, project_id, reason, 0);
    Ok(true)
}

/// Starts prompt-based compaction for the selected WebUI conversation.
/// 为当前选中的 WebUI 会话启动提示词式上下文压缩。
pub(crate) async fn service_compact_selected_conversation_context(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, agent_runtime, luaskills_catalog, mcp_catalog, logs) =
        runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    webui_trace(
        "context_compaction_requested",
        format!("session={selected_session_id}"),
    );

    let (has_active_stream_task, has_active_kernel_turn, has_active_session_run) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        (
            guard.active_stream_tasks.contains_key(&selected_session_id),
            webui_session_kernel_has_active_turn(&guard, selected_session_id),
            webui_session_has_active_run(&guard, selected_session_id),
        )
    };
    if has_active_stream_task || has_active_kernel_turn || has_active_session_run {
        return Err(String::from(
            "当前会话仍有运行中的执行链路，暂时无法执行手动上下文压缩。",
        ));
    }

    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    hydrate_selected_session_runtime_if_needed(
        &state,
        controller_runtime.clone(),
        &mut workspace_snapshot,
        selected_session_id,
    )
    .await?;
    ensure_webui_session_runtime_state(&mut workspace_snapshot, selected_session_id, None)?;
    // modelTarget freezes the exact session route and adaptive tail policy before manual planning.
    // modelTarget 在手动规划前冻结精确会话路由与自适应尾部策略。
    let model_target = resolve_webui_context_compaction_model_target(
        &state,
        &agent_runtime,
        selected_session_id,
        None,
    )?;
    let compaction_plan = webui_context_compaction_plan_from_v1(
        &state,
        selected_session_id,
        model_target.context_compaction_policy,
        "当前会话暂时没有可压缩的历史上下文。",
    )?;

    let budget_note = context_compaction_budget_note_from_state(&state, selected_session_id);
    let previous_phase = workspace_snapshot
        .session_run_phases
        .get(&selected_session_id)
        .copied()
        .unwrap_or(SessionRunPhase::Ready);
    let status_message_id = append_webui_context_compaction_status_message_to_workspace(
        &mut workspace_snapshot,
        selected_session_id,
        &budget_note,
        MANUAL_CONTEXT_COMPACTION_TRIGGER,
    )?;
    workspace_snapshot
        .session_run_phases
        .insert(selected_session_id, SessionRunPhase::Preparing);
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == selected_session_id)
    {
        session.status = SessionState::Running;
        session.last_updated_label = String::from("刚刚");
        session.updated_at = current_unix_millis_text();
    }
    let turn_id = format!("context-compaction-{}", current_unix_millis_text());
    // expected_kernel_generation binds every later launch failure to this exact compaction turn.
    // expected_kernel_generation 将后续每个启动失败绑定到当前精确压缩回合。
    let expected_kernel_generation = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        start_webui_session_kernel_turn(
            &mut guard,
            selected_session_id,
            &turn_id,
            ActiveTurnKind::ContextCompaction,
        )?;
        webui_session_kernel_generation(&guard, selected_session_id)
    };
    // workbench commits the prepared workspace before active stream-task registration begins.
    // workbench 在活动流任务注册开始前提交已准备的工作区。
    let workbench = match store_and_emit_full_workbench_snapshot_for_cold_refresh(
        FullWorkbenchColdRefreshRequest {
            app: &app,
            state: &state,
            controller_runtime: &controller_runtime,
            luaskills_catalog: &luaskills_catalog,
            mcp_catalog: &mcp_catalog,
            provider_configuration: &agent_runtime.provider_configuration,
            public_ai_runtime: Some(agent_runtime.public_ai_runtime.as_ref()),
            workspace_source: FullWorkbenchColdRefreshWorkspaceSource::Candidate(Box::new(
                workspace_snapshot,
            )),
            logs: &logs,
            subagent_navigation_mode: SubagentNavigationRefreshMode::Rebuild,
        },
    ) {
        Ok(workbench) => workbench,
        Err(error) => {
            // cleanup_outcome proves compensation cannot terminate a replacement task or kernel generation.
            // cleanup_outcome 证明补偿不会终结替代任务或替代内核代际。
            let cleanup_outcome = handle_pre_registration_stream_task_launch_failure(
                &app,
                &state,
                selected_session_id,
                expected_kernel_generation,
            )
            .map_err(|cleanup_error| {
                format!("{error}; context compaction launch cleanup failed: {cleanup_error}")
            })?;
            webui_trace(
                "context_compaction_pre_registration_failure_handled",
                format!("session={selected_session_id} outcome={cleanup_outcome:?} error={error}"),
            );
            return Err(error);
        }
    };

    spawn_webui_context_compaction_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        agent_runtime,
        selected_session_id,
        previous_phase,
        status_message_id,
        budget_note,
        MANUAL_CONTEXT_COMPACTION_TRIGGER.to_string(),
        compaction_plan.plan,
        compaction_plan.full_context_state,
        compaction_plan.media_projection,
        compaction_plan.retained_from_seq,
        model_target,
    )?;
    Ok(workbench)
}

/// Stops one exact WebUI conversation run and clears its guided internal queue work.
/// 停止一个精确 WebUI 会话运行，并清理其已引导内部队列任务。
/// Parameters: `app` emits snapshots, `state` owns runtime data, `conversation_id` fixes the target, and `mode` selects child-agent handling.
/// 参数：`app` 发布快照，`state` 持有运行数据，`conversation_id` 固定目标，`mode` 选择子代理处理方式。
/// Returns: the confirmation payload or stopped workbench snapshot, or an explicit validation/runtime error.
/// 返回值：确认载荷或已停止工作台快照，或显式验证/运行错误。
pub(crate) async fn service_stop_conversation_run(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
    mode: Option<StopConversationRunMode>,
) -> Result<StopConversationRunResponse, String> {
    // stopTask transfers the complete Stop transaction to service ownership before its RPC can be cancelled.
    // stopTask 在 RPC 可能被取消前，把完整 Stop 事务转移给服务所有权。
    let stop_task = spawn_service_task(async move {
        service_stop_conversation_run_owned(app, state, conversation_id, mode).await
    });
    stop_task
        .await
        .map_err(|join_error| format!("ERR_CONVERSATION_STOP_TASK_FAILED: {join_error}"))?
}

/// Executes one service-owned conversation Stop transaction through durable terminal settlement.
/// 执行一个由服务持有的会话 Stop 事务，直至可靠终态结算完成。
///
/// # Parameters
/// 参数
/// `app` emits the committed workbench, `state` owns live runtime facts, `conversation_id` fixes the target, and `mode` selects child-agent handling.
/// `app` 发送已提交工作台，`state` 持有实时运行事实，`conversation_id` 固定目标，`mode` 选择子代理处理方式。
///
/// # Returns
/// 返回值
/// The confirmation payload or stopped workbench snapshot, or an explicit validation/runtime error.
/// 返回确认载荷或已停止工作台快照，或显式验证/运行错误。
async fn service_stop_conversation_run_owned(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: String,
    mode: Option<StopConversationRunMode>,
) -> Result<StopConversationRunResponse, String> {
    // target_session_id is the validated immutable target shared by preflight and final mutation.
    // target_session_id 是预检与最终变更共享的已验证不可变目标。
    let target_session_id = parse_session_id(conversation_id.trim())
        .ok_or_else(|| format!("无效的停止目标会话标识：{conversation_id}"))?;
    ensure_regular_session_exists(&state, target_session_id, "停止")?;
    let (controller_runtime, agent_runtime, luaskills_catalog, mcp_catalog, logs) =
        runtime_parts_from_state(&state)?;
    let mode = mode.unwrap_or_default();
    webui_trace(
        "stop_conversation_requested",
        format!("session={target_session_id} mode={mode:?}"),
    );
    // A durable terminal settlement from an earlier stream keeps the session closed; one controlled
    // idempotent retry lets a recovered store complete it before Stop owns the terminal boundary.
    // 早期流留下的可靠终态结算让会话保持关闭；Stop 持有终态边界前，一次受控幂等重试
    // 让恢复后的存储完成该结算。
    ensure_session_terminal_settlement_settled(&state, target_session_id).await?;
    let preflight_running_subagent_task_states = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        running_webui_subagent_task_states_for_tree_from_live_cache(&mut guard, target_session_id)
    };
    if mode == StopConversationRunMode::AskIfRunningSubagents
        && !preflight_running_subagent_task_states.is_empty()
    {
        return Ok(StopConversationRunResponse::NeedsConfirmation {
            running_subagents: stop_conversation_running_subagent_summaries(
                &preflight_running_subagent_task_states,
            ),
        });
    }
    // stopAdmissionGuard closes replacement-turn admission before the active stream registry is dismantled.
    // stopAdmissionGuard 在拆除活动流注册表前关闭替代回合准入。
    let stop_admission_guard = begin_session_run_stop_exclusion(&state, target_session_id)?;
    {
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::agent_collaboration_execution::preparation_tasks::validate_question_preparation_task(&guard, target_session_id)?;
    }
    // Persist the typed collaboration stop before teardown so late recovery cannot rely on live UI or backend state.
    // 在拆除执行链前持久化类型化协作停止状态，防止迟到恢复依赖实时 UI 或后端状态。
    let goal_handle = session_v1_handle_from_state(&state, target_session_id).await?;
    let stop_control = if mode == StopConversationRunMode::MainAndSubagents {
        CollaborationRunControl::MainAndSubagents
    } else {
        CollaborationRunControl::MainOnly
    };
    let stop_control_at_ms = current_v1_goal_unix_millis()?;
    let stop_control_operation_id = CollaborationOperationId::try_new(format!(
        "conversation-stop:{target_session_id}:{stop_control_at_ms}:{}",
        Uuid::new_v4()
    ))
    .map_err(|error| format!("failed to create collaboration stop operation identity: {error}"))?;
    let stop_control_receipt = goal_handle
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: stop_control_operation_id,
            state: stop_control,
            changed_at_ms: stop_control_at_ms,
        })
        .map_err(|error| format!("failed to persist collaboration stop control: {error}"))?;
    webui_trace(
        "stop_conversation_collaboration_control",
        format!(
            "session={target_session_id} state={stop_control:?} changed={} revision={}",
            stop_control_receipt.changed, stop_control_receipt.revision
        ),
    );
    // stoppedHookTurnId freezes the exact logical Hook owner before any stop teardown can race a replacement turn.
    // stoppedHookTurnId 在停止清理可能与替代回合竞争前冻结精确逻辑 Hook 所有者。
    let stopped_hook_turn_id = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?
        .active_turn_id(target_session_id)
        .map(str::to_string);
    let stop_running_subagents = mode == StopConversationRunMode::MainAndSubagents;
    // terminalPersistenceOwner is installed in the same transaction that dismantles live execution ownership.
    // terminalPersistenceOwner 与实时执行所有权拆除在同一个事务中安装。
    let terminal_persistence_owner = SessionTerminalPersistenceOwner::RunStop(Uuid::new_v4());
    // stopOwnershipTransition closes the former stream and kernel generation without an unowned terminal gap.
    // stopOwnershipTransition 关闭旧流与 kernel 代际，且不会留下无所有者终态空窗。
    let (active_task, preparation_task, had_active_kernel_turn, stopped_question_execution_id) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // activeTask is detached before the kernel stops, while the same state lock excludes replacement registration.
        // activeTask 在 kernel 停止前分离，同一状态锁同时排除替代登记。
        crate::agent_collaboration_execution::preparation_tasks::validate_question_preparation_task(&guard, target_session_id)?;
        register_session_terminal_persistence_owner(
            &mut guard,
            target_session_id,
            terminal_persistence_owner.clone(),
        )?;
        let active_task = guard.active_stream_tasks.remove(&target_session_id);
        let preparation_task = guard.question_preparation_tasks.remove(&target_session_id);
        // Capture the exact auxiliary identity before stopping the kernel removes its active turn.
        // 停止内核移除活动回合之前，捕获精确辅助身份。
        let stopped_question_execution_id = guard
            .session_runtime_kernels
            .get(&target_session_id)
            .and_then(|kernel| kernel.active_turn())
            .and_then(|turn| match &turn.kind {
                ActiveTurnKind::QuestionExecution { execution_id } => Some(execution_id.clone()),
                _ => None,
            });
        // hadActiveKernelTurn records the exact in-lock generation transition for Stop diagnostics.
        // hadActiveKernelTurn 记录 Stop 诊断所需的精确锁内代际转换。
        let had_active_kernel_turn =
            stop_webui_session_kernel_generation(&mut guard, target_session_id);
        (
            active_task,
            preparation_task,
            had_active_kernel_turn,
            stopped_question_execution_id,
        )
    };
    // terminalPersistenceGuard deliberately retains an unreleased marker on every early error or panic.
    // terminalPersistenceGuard 会在任何提前错误或 panic 时有意保留未释放标记。
    let terminal_persistence_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        target_session_id,
        terminal_persistence_owner,
    )?;
    let had_active_stream_task = active_task.is_some();
    // Cancel both physical owners before waiting, so neither can finish new preparation work during teardown.
    // 等待之前取消两个物理所有者，防止拆除期间继续完成新准备工作。
    if let Some(task) = &preparation_task {
        task.task.abort();
    }
    if let Some(task) = &active_task {
        task.abort();
    }
    if let Some(task) = preparation_task {
        if let Err(error) = task.task.abort_and_wait().await {
            eprintln!("[webui-service] stopped question preparation teardown warning: {error}");
        }
    }
    let active_task_join_error = if let Some(active_task) = active_task {
        active_task.abort_and_wait().await.err()
    } else {
        None
    };
    // Settle result workers before freezing the stop API identity; no replacement preparation is admitted.
    // 冻结停止 API 身份前结算结果工作器；此时不准入任何替代准备。
    let preparation_settlement = goal_handle.acquire_request_preparation_settlement().await;
    // Write QA intent before external cleanup can fail or its API can be interrupted.
    // 在外部清理可能失败或 API 可能被中断之前写入答疑意图。
    let prepared_question_scope = match stopped_question_execution_id.as_ref() {
        Some(execution_id) => Some(
            question_stop::persist_question_stop_intent(
                &state,
                &goal_handle,
                target_session_id,
                execution_id,
                terminal_persistence_guard.owner_generation(),
            )
            .await?,
        ),
        None => None,
    };
    // Stop owns the terminal boundary after aborting the stream, so no Finished event can
    // retire its Playwright process. Revoke the exact conversation grant while replacement
    // admission remains closed, leaving the Core browser itself available after cancellation.
    // Stop 中止流后持有终态边界，Finished 事件已无法退休 Playwright 进程；因此在替代
    // 准入仍关闭时撤销精确会话授权，并保留取消后的 Core 浏览器本身。
    let browser_manager = state
        .lock()
        .map(|guard| guard.browser_manager.clone())
        .map_err(|error| format!("停止浏览器授权时服务状态加锁失败：{error}"))?;
    browser_manager
        .retire_ai_connection_after_turn(target_session_id)
        .await
        .map_err(|error| format!("停止浏览器自动化授权失败：{error}"))?;
    // The aborted stream cannot publish its normal Finished event, so Stop owns the V1 API interruption.
    // 被中止的流无法发布正常 Finished 事件，因此 Stop 负责 V1 API 中断事实。
    let stop_interruption_at_ms = current_v1_goal_unix_millis()?;
    let interruption_receipt = goal_handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                // Scope is proven before interrupting an API; a different business API must remain untouched.
                // 中断 API 之前先证明范围；不同业务 API 必须保持不变。
                let question_scope = stopped_question_execution_id
                    .as_ref()
                    .map(|execution_id| question_stop::question_stop_scope(document, execution_id))
                    .transpose()?;
                if question_scope != prepared_question_scope {
                    return Err(
                        vulcan_session_core::v1::DomainError::CollaborationStateViolation {
                            field: "question_stop.prepared_scope_changed",
                        },
                    );
                }
                Ok(
                    match document.interrupt_active_api(stop_interruption_at_ms) {
                        Ok(stats_seq) => {
                            SessionMutationChange::Changed((Some(stats_seq), question_scope))
                        }
                        Err(vulcan_session_core::v1::DomainError::NoActiveApi) => {
                            SessionMutationChange::Unchanged((None, question_scope))
                        }
                        Err(error) => return Err(error),
                    },
                )
            },
        )
        .map_err(|error| format!("停止后持久化 V1 API 中断失败：{error}"))?;
    // Scoped terminal persistence reacquires this gate; retaining it would deadlock the same stop.
    // 带范围终态持久化会重新获取该关卡；继续持有会使同一次停止死锁。
    // Run-stop admission and terminal ownership still exclude all replacement requests.
    // RunStop 准入与终态所有者仍排除全部替代请求。
    drop(preparation_settlement);
    webui_trace(
        "stop_conversation_v1_boundary",
        format!(
            "session={target_session_id} interrupted_api_stats_seq={:?}",
            interruption_receipt.value.0.map(|seq| seq.value())
        ),
    );
    // The immediate stopped phase removes the stale running affordance before slower settlement can fail.
    // 即时停止阶段会在较慢结算可能失败前移除过期的运行中操作入口。
    commit_retained_workspace_session_phase_mutation_and_emit_delta(
        &app,
        &state,
        target_session_id,
        SessionRunPhase::Stopped,
        |workspace_snapshot| {
            mark_webui_session_stop_requested_in_workspace(workspace_snapshot, target_session_id);
            Ok(())
        },
    )?;
    let running_subagent_task_states = if stop_running_subagents {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        running_webui_subagent_task_states_for_tree_from_live_cache(&mut guard, target_session_id)
    } else {
        Vec::new()
    };
    webui_trace(
        "stop_conversation_state",
        format!(
            "session={target_session_id} had_active_kernel_turn={had_active_kernel_turn} had_active_stream_task={had_active_stream_task} stop_running_subagents={stop_running_subagents} running_subagents={}",
            running_subagent_task_states.len()
        ),
    );
    for (_, task_state) in &running_subagent_task_states {
        let _ = agent_runtime
            .task_runtime
            .stop_task_tree(task_state.task_id);
    }
    if stop_running_subagents {
        stop_owned_child_auxiliary_tree(
            &app,
            &state,
            &agent_runtime.task_runtime,
            target_session_id,
        )
        .await?;
    }
    if !running_subagent_task_states.is_empty() {
        let running_subagent_task_ids = running_subagent_task_states
            .iter()
            .map(|(direct_parent_session_id, state)| (*direct_parent_session_id, state.task_id))
            .collect::<Vec<_>>();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        mark_webui_subagent_tasks_stopped_in_state(&mut guard, &running_subagent_task_ids);
    }
    let now_ms = current_v1_goal_unix_millis()?;
    let goal_pause = goal_handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            move |document| {
                if document.metadata.mode.goal.enabled
                    && document.metadata.mode.goal.state
                        == vulcan_session_core::v1::config::GoalState::Active
                {
                    document.pause_goal(GOAL_STOPPED_PROVIDER_CONTEXT, now_ms)?;
                    Ok(SessionMutationChange::Changed(()))
                } else {
                    Ok(SessionMutationChange::Unchanged(()))
                }
            },
        )
        .map_err(|error| format!("停止后持久化 V1 Goal 状态失败：{error}"))?;
    if goal_pause.changed {
        let summary = session_goal_summary_from_v1_handle(goal_handle.as_ref(), target_session_id)?;
        cache_session_goal_summary(&state, target_session_id, summary)?;
    }
    // stopSettlement mutates the latest retained workspace only after every awaited pre-settlement step has finished.
    // stopSettlement 仅在所有结算前异步步骤结束后修改最新保留工作区。
    let stop_settlement = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let retained_workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let did_stop_work = stop_webui_session_work(
            retained_workspace,
            target_session_id,
            had_active_kernel_turn,
            had_active_stream_task,
            !running_subagent_task_states.is_empty(),
        )?;
        if did_stop_work {
            let owning_project_id =
                owning_project_id_for_session(retained_workspace, target_session_id);
            Some(owning_project_id)
        } else {
            None
        }
    };
    if let Some(owning_project_id) = stop_settlement {
        // A stop closes the retained Hook turn before a later Continue restores the same logical input.
        // 停止操作会在后续 Continue 恢复同一逻辑输入前关闭保留的 Hook 回合。
        terminate_active_conversation_hook_turn_after_stop(
            &agent_runtime,
            target_session_id,
            owning_project_id,
            stopped_hook_turn_id.as_deref(),
            "The user stopped the active conversation execution.",
        )?;
    }
    // Both the user interruption and optional Goal pause are durable before admission reopens.
    // 用户中断与可选 Goal 暂停都已可靠落盘后，才重新开放准入。
    if let Some(question_scope) = interruption_receipt.value.1 {
        // Stop settles only the captured auxiliary execution through the recoverable scope-aware intent.
        // Stop 通过可恢复的带范围意图，仅结算捕获的辅助执行。
        if !crate::agent_stream::settle_stream_terminal_authority_with_scope(
            &state,
            target_session_id,
            false,
            question_stop::QUESTION_STOP_REASON,
            Some(question_stop::QUESTION_STOP_DETAIL.to_owned()),
            terminal_persistence_guard,
            question_scope,
        )
        .await
        {
            return Err(String::from(
                "答疑停止结算尚未完成，已保留恢复记录与终态所有者。",
            ));
        }
    } else {
        terminal_persistence_guard.release()?;
    }
    // The Stop admission reservation is equally live state and must not contaminate the successful Continue summary.
    // Stop 准入预留同样属于实时状态，不得污染成功响应中的 Continue 摘要。
    drop(stop_admission_guard);
    // The stop can strand already accepted task messages before their next legal API boundary.
    // 停止可能让已接纳任务消息在下一合法 API 边界之前悬空。
    if let Err(error) = crate::agent_collaboration::reconcile_task_results_for_session(
        &state,
        target_session_id,
    )
    .await
    {
        tracing::warn!(%target_session_id, %error, "Stopped task message return remains pending");
    }
    let workbench =
        store_and_emit_full_workbench_snapshot_for_cold_refresh(FullWorkbenchColdRefreshRequest {
            app: &app,
            state: &state,
            controller_runtime: &controller_runtime,
            luaskills_catalog: &luaskills_catalog,
            mcp_catalog: &mcp_catalog,
            provider_configuration: &agent_runtime.provider_configuration,
            public_ai_runtime: Some(agent_runtime.public_ai_runtime.as_ref()),
            workspace_source: FullWorkbenchColdRefreshWorkspaceSource::Retained,
            logs: &logs,
            subagent_navigation_mode: SubagentNavigationRefreshMode::Rebuild,
        })?;
    if let Some(error) = active_task_join_error {
        eprintln!("[webui-service] stopped conversation task teardown warning: {error}");
    }
    Ok(StopConversationRunResponse::Stopped {
        workbench: Box::new(workbench),
    })
}

/// Converts running child-agent states into the confirmation dialog payload.
/// 将运行中的智能体状态转换为确认弹窗载荷。
fn stop_conversation_running_subagent_summaries(
    running_subagent_task_states: &[(SessionId, SubagentTaskPanelState)],
) -> Vec<StopConversationRunSubagentSummary> {
    running_subagent_task_states
        .iter()
        .map(
            |(direct_parent_session_id, state)| StopConversationRunSubagentSummary {
                parent_conversation_id: direct_parent_session_id.to_string(),
                task_id: state.task_id.to_string(),
                label: state.subagent_label.clone(),
            },
        )
        .collect()
}

/// Stops one live command session and rebuilds the WebUI workbench snapshot.
/// 计划模式状态变化后的刷新工作台快照。
pub(crate) async fn service_set_session_plan_mode(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionPlanModeUpdateRequest,
) -> Result<SessionAddonsChangedResponse, String> {
    let session_id = request
        .conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .ok_or_else(|| String::from("当前草稿会话尚未创建，暂时不能切换计划模式。"))?;
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
            return Err(format!("未找到要切换计划模式的会话：{session_id}"));
        }
    }
    let enabled = request.enabled;
    let changed = persist_session_plan_mode_v1(&state, session_id, enabled).await?;
    if changed {
        let updated_at = current_unix_millis_text();
        let next_plan_mode = SessionPlanModeState {
            prompt_injected: enabled,
            pending_execution_notice: false,
            artifact: None,
        };
        let work_mode = SessionWorkMode {
            execution_mode: if enabled {
                String::from("plan")
            } else {
                String::from("build")
            },
            plan_mode: next_plan_mode,
        };
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: updated_at.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        work_mode.apply_to_runtime_state(runtime_state);
        runtime_state.updated_at = updated_at;
    }
    emit_session_addons_changed_delta(&app, &state, session_id)
}

/// Persists one session-scoped VMM memory policy override.
/// 持久化单个会话级 VMM 记忆策略覆盖。
///
/// # Parameters
/// 参数
/// `app` emits addon and workbench delta events after the update.
/// `app` 在更新后发送附属状态与工作台增量事件。
/// `state` owns retained workspace and runtime snapshots.
/// `state` 持有保留的工作区与运行时快照。
/// `request` describes the policy key and tri-state mode selected by the user.
/// `request` 描述用户选择的策略键与三态模式。
///
/// # Returns
/// 返回值
/// The updated addon projection for the target session.
/// 返回目标会话更新后的附属状态投影。
pub(crate) async fn service_set_session_vmm_policy(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionVmmPolicyUpdateRequest,
) -> Result<SessionAddonsChangedResponse, String> {
    let session_id = request
        .conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .ok_or_else(|| String::from("当前草稿会话尚未创建，暂时不能切换 VMM 记忆策略。"))?;
    let policy_value = session_vmm_policy_mode_from_request(&request.mode)?;
    let policy_field = match request.policy.trim().to_ascii_lowercase().as_str() {
        "precheck" | "precheck_enabled" | "precheck-injection" | "precheck_injection" => {
            VmmSessionPolicyField::Precheck
        }
        "writeback" | "memory_writeback" | "memory_writeback_enabled" | "post_action" => {
            VmmSessionPolicyField::MemoryWriteback
        }
        other => return Err(format!("不支持的 VMM 记忆策略：{other}")),
    };
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
            return Err(format!("未找到要切换 VMM 记忆策略的会话：{session_id}"));
        }
    }
    let updated_at_ms = current_v1_goal_unix_millis()?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    set_vmm_session_policy_value_v1(handle.as_ref(), policy_field, policy_value, updated_at_ms)?;
    let persisted_policy = handle
        .read(|document| document.metadata.config.selection.vmm.clone())
        .map_err(|error| format!("读取 V1 VMM 会话策略失败：{error}"))?;
    let updated_at = updated_at_ms.to_string();
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let vmm_hook_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.vmm_hook_runtime.clone());
        {
            let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: updated_at.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            runtime_state.vmm.precheck_enabled = persisted_policy.precheck_enabled;
            runtime_state.vmm.memory_writeback_enabled = persisted_policy.memory_writeback_enabled;
            runtime_state.updated_at = updated_at.clone();
            if let Some(vmm_hook_runtime) = vmm_hook_runtime {
                refresh_exact_vmm_hook_session_projection(
                    &vmm_hook_runtime,
                    workspace,
                    session_id,
                )?;
            }
        }
    }
    emit_session_addons_changed_delta(&app, &state, session_id)
}

/// Persists one session context-compaction window node without changing the model hard window.
/// 持久化一个会话上下文压缩窗口节点，但不改变模型硬窗口。
///
/// # Parameters
/// 参数
/// `app` emits addon and Workbench updates, `state` owns the exact session and model runtime, and
/// `request` carries the immutable conversation id plus an optional stable node id.
/// `app` 发出 addon 与 Workbench 更新，`state` 持有精确会话与模型运行时，`request` 携带
/// 不可变会话标识及可选稳定节点标识。
///
/// # Returns
/// 返回值
/// The authoritative addon projection after durable persistence succeeds.
/// 持久化成功后返回权威 addon 投影。
pub(crate) async fn service_set_session_context_compaction_window(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionContextCompactionWindowUpdateRequest,
) -> Result<SessionAddonsChangedResponse, String> {
    let session_id = parse_session_id(&request.conversation_id)
        .ok_or_else(|| String::from("当前草稿会话尚未创建，暂时不能切换上下文压缩窗口。"))?;
    let node_id = request
        .node_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::parse::<vulcan_context::ContextCompactionWindowNodeId>)
        .transpose()?;
    let updated_at = current_unix_millis_text();
    let updated_at_ms = updated_at
        .parse::<i64>()
        .map_err(|error| format!("failed to parse current unix millis: {error}"))?;
    // configurationTransaction serializes this session node against every model-selection mutation.
    // configurationTransaction 将本次会话节点修改与全部模型选择变更串行化。
    let configuration_transaction = begin_public_ai_configuration_transaction().await;
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
            return Err(format!("未找到要切换上下文压缩窗口的会话：{session_id}"));
        }
        let runtime_state = workspace_snapshot.session_runtime_state.get(&session_id);
        if let Some(node_id) = node_id {
            let agent_runtime = guard.agent_runtime.as_ref().ok_or_else(|| {
                String::from("service is still starting; agent runtime is not ready")
            })?;
            let public_ai_runtime = agent_runtime.public_ai_runtime.as_ref();
            let configuration = public_ai_runtime.configuration_snapshot()?;
            let node_tokens = configuration
                .context_compaction_window_nodes
                .tokens_for(node_id)?;
            let selection = public_ai_workbench_selection(
                &configuration,
                public_ai_runtime,
                Some(session_id),
                runtime_state,
            )?
            .ok_or_else(|| String::from("当前会话没有可验证的模型选择。"))?;
            let model_context_limit =
                selection.model.capabilities.context_limit.ok_or_else(|| {
                    String::from("当前模型未声明上下文窗口，无法锁定上下文压缩节点。")
                })?;
            if node_tokens >= model_context_limit {
                return Err(format!(
                    "上下文压缩节点 `{}` 当前为 {} Token，必须严格小于模型上下文窗口 {} Token。",
                    node_id.as_str(),
                    node_tokens,
                    model_context_limit
                ));
            }
        }
    }
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    let mutation = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let changed =
                    document.set_context_compaction_window_node_id(node_id, updated_at_ms);
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .map_err(|error| format!("持久化 V1 上下文压缩窗口失败：{error}"))?;
    if mutation.changed {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        {
            let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
                String::from("service is still starting; workspace snapshot is not ready")
            })?;
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: updated_at.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            runtime_state.context_compaction_window_node_id = node_id;
            runtime_state.live_context_budget_fingerprint = None;
            runtime_state.anchor_context_budget_fingerprint = None;
            runtime_state.updated_at = updated_at.clone();
        }
        guard.live_context_budgets.remove(&session_id);
    }
    drop(configuration_transaction);
    emit_session_addons_changed_delta(&app, &state, session_id)
}

/// Persists a session output-style override without changing the active prompt prefix.
/// 持久化会话输出风格覆盖，但不直接改写当前提示词前缀。
pub(crate) async fn service_set_session_output_style(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionOutputStyleUpdateRequest,
) -> Result<SessionAddonsChangedResponse, String> {
    let session_id = parse_session_id(&request.conversation_id)
        .ok_or_else(|| String::from("当前草稿会话尚未创建，暂时不能切换输出风格。"))?;
    let global_selection = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
            return Err(format!("未找到要切换输出风格的会话：{session_id}"));
        }
        guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?
            .provider_configuration
            .prompt
            .output_style
    };
    let normalized_style_id = request
        .style_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    resolve_output_style_snapshot(normalized_style_id.as_deref(), global_selection)?;
    let updated_at = current_unix_millis_text();
    let updated_at_ms = updated_at
        .parse::<i64>()
        .map_err(|error| format!("failed to parse current unix millis: {error}"))?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    let mutation = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            {
                let normalized_style_id = normalized_style_id.clone();
                move |document| {
                    let changed = document.set_output_style(normalized_style_id, updated_at_ms)?;
                    Ok(if changed {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    })
                }
            },
        )
        .map_err(|error| format!("持久化 V1 输出风格失败：{error}"))?;
    if mutation.changed {
        // The legacy runtime field is a temporary WebUI projection only; V1 metadata is the sole durable authority.
        // 旧运行态字段仅是阶段内 WebUI 投影；V1 metadata 是唯一持久化权威。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: updated_at.clone(),
                ..SessionRuntimeStateEntry::default()
            });
        runtime_state.output_style.override_style_id = normalized_style_id;
        runtime_state.updated_at = updated_at.clone();
        drop(guard);
        emit_session_addon_changed_event(
            &app,
            session_id,
            &[SessionAddonNamespace::OutputStyle],
            mutation.revision,
            updated_at.clone(),
        );
    }
    emit_session_addons_changed_delta(&app, &state, session_id)
}

/// Parses one WebUI VMM policy mode into an optional layer value.
/// 将单个 WebUI VMM 策略模式解析为可选配置层值。
///
/// # Parameters
/// 参数
/// `mode` is the raw mode key sent by the WebUI.
/// `mode` 是 WebUI 发送的原始模式键。
///
/// # Returns
/// 返回值
/// `None` for inheritance, or `Some` for explicit enablement.
/// 继承时返回 `None`，显式开关时返回 `Some`。
fn session_vmm_policy_mode_from_request(mode: &str) -> Result<Option<bool>, String> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "inherit" | "inherit_project" | "inheritproject" => Ok(None),
        "enabled" | "on" | "true" => Ok(Some(true)),
        "disabled" | "off" | "false" => Ok(Some(false)),
        other => Err(format!("不支持的 VMM 记忆策略模式：{other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies stop confirmation summaries preserve backend task identity and labels.
    /// 验证停止确认摘要会保留后端任务身份与标签。
    #[test]
    fn stop_conversation_running_subagent_summaries_preserve_task_identity() {
        let parent_session_id = SessionId::new_v4();
        let task_id = SessionId::new_v4();
        let states = vec![(
            parent_session_id,
            SubagentTaskPanelState {
                task_id,
                public_task_id: String::from("agent-alpha"),
                parent_session_id,
                subagent_label: String::from("agent-alpha"),
                description: Some(String::from("running task")),
                run_state: SubagentTaskRunState::Running,
                result_text: String::new(),
                conversation_state: None,
            },
        )];

        let summaries = stop_conversation_running_subagent_summaries(&states);

        assert_eq!(summaries.len(), 1);
        assert_eq!(
            summaries[0].parent_conversation_id,
            parent_session_id.to_string()
        );
        assert_eq!(summaries[0].task_id, task_id.to_string());
        assert_eq!(summaries[0].label, "agent-alpha");
    }
}
