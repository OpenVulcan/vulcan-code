//! Conversation run service handlers.
//! 会话运行服务处理器。

use super::*;

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

/// Starts prompt-based compaction for the selected WebUI conversation.
/// 为当前选中的 WebUI 会话启动提示词式上下文压缩。
pub(crate) async fn service_compact_selected_conversation_context(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
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
        controller_runtime.clone(),
        &mut workspace_snapshot,
        selected_session_id,
    )
    .await?;
    ensure_webui_session_runtime_state(&mut workspace_snapshot, selected_session_id, None)?;
    let runtime_state_snapshot = workspace_snapshot
        .session_runtime_state
        .get(&selected_session_id)
        .cloned()
        .unwrap_or_default();
    let plan = webui_context_compaction_plan_from_conversation_state(
        selected_session_id,
        &runtime_state_snapshot.conversation_state,
        runtime_state_snapshot.context_start_message_id.as_deref(),
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
            token_budget_runtime: &token_budget_runtime,
            mcp_catalog: &mcp_catalog,
            provider_configuration: &agent_runtime.provider_configuration,
            public_ai_runtime: Some(agent_runtime.public_ai_runtime.as_ref()),
            workspace_snapshot,
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
        plan,
        runtime_state_snapshot.conversation_state.clone(),
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
    // target_session_id is the validated immutable target shared by preflight and final mutation.
    // target_session_id 是预检与最终变更共享的已验证不可变目标。
    let target_session_id = parse_session_id(conversation_id.trim())
        .ok_or_else(|| format!("无效的停止目标会话标识：{conversation_id}"))?;
    ensure_regular_session_exists(&state, target_session_id, "停止")?;
    let (
        controller_runtime,
        agent_runtime,
        luaskills_catalog,
        token_budget_runtime,
        mcp_catalog,
        logs,
    ) = runtime_parts_from_state(&state)?;
    let mode = mode.unwrap_or_default();
    webui_trace(
        "stop_conversation_requested",
        format!("session={target_session_id} mode={mode:?}"),
    );
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
    let stop_running_subagents = mode == StopConversationRunMode::MainAndSubagents;
    let active_task = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.active_stream_tasks.remove(&target_session_id)
    };
    let goal_before_stop = active_task
        .as_ref()
        .and_then(ActiveStreamTask::session_goal_id)
        .map(str::to_string);
    let had_active_kernel_turn = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        stop_webui_session_kernel_generation(&mut guard, target_session_id)
    };
    let had_active_stream_task = active_task.is_some();
    if let Some(active_task) = active_task {
        active_task.abort();
    }
    let mut workspace_snapshot = with_workspace_mutation_and_full_workbench_rebuild_for_cold_path(
        &app,
        &state,
        |workspace| {
            // now seals every active row before the stop snapshot becomes authoritative.
            // now 在停止快照成为权威状态前封口全部活跃行。
            let now = current_unix_millis_text();
            if let Some(runtime_state) = workspace.session_runtime_state.get_mut(&target_session_id)
            {
                seal_active_streaming_records_in_state(runtime_state, &now);
            }
            mark_webui_session_stop_requested_in_workspace(workspace, target_session_id);
        },
    )?;
    let running_subagent_task_states = if stop_running_subagents {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        running_webui_subagent_task_states_for_tree_from_cache(
            &mut guard,
            &workspace_snapshot,
            target_session_id,
        )
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
    for (direct_parent_session_id, task_state) in &running_subagent_task_states {
        let _ = agent_runtime
            .task_runtime
            .stop_task_tree(task_state.task_id);
        mark_webui_subagent_task_stopped_in_workspace(
            &mut workspace_snapshot,
            *direct_parent_session_id,
            task_state.task_id,
            Some(task_state),
        );
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
    let did_stop_work = stop_webui_session_work(
        &mut workspace_snapshot,
        target_session_id,
        had_active_kernel_turn,
        !running_subagent_task_states.is_empty(),
    )?;
    if did_stop_work
        && !workspace_snapshot
            .session_runtime_state
            .contains_key(&target_session_id)
    {
        return Err(String::from("停止后未找到会话运行时状态。"));
    }
    if did_stop_work && let Some(expected_goal_id) = goal_before_stop {
        let goal_transition_guard =
            begin_session_turn_start_exclusion(&state, [target_session_id], "暂停目标")?;
        match VulcanGoalStore::new(controller_runtime.clone())
            .pause_thread_goal(target_session_id, &expected_goal_id)
            .await?
        {
            Some(paused_goal)
                if paused_goal.goal_id == expected_goal_id
                    && paused_goal.status == ThreadGoalStatus::Paused =>
            {
                advance_session_goal_dispatch_version(&state, target_session_id)?;
                clear_session_goal_stage_runtime_state(
                    &state,
                    target_session_id,
                    &paused_goal.goal_id,
                )?;
                cache_session_goal_summary(&state, target_session_id, Some(paused_goal))?;
            }
            Some(_) => {
                return Err(String::from(
                    "停止运行后的目标状态结果与请求不一致，请刷新后重试。",
                ));
            }
            None => {
                webui_trace(
                    "stop_conversation_goal_changed",
                    format!("session={target_session_id} expected_goal_id={expected_goal_id}"),
                );
            }
        }
        drop(goal_transition_guard);
    }
    let workbench =
        store_and_emit_full_workbench_snapshot_for_cold_refresh(FullWorkbenchColdRefreshRequest {
            app: &app,
            state: &state,
            controller_runtime: &controller_runtime,
            luaskills_catalog: &luaskills_catalog,
            token_budget_runtime: &token_budget_runtime,
            mcp_catalog: &mcp_catalog,
            provider_configuration: &agent_runtime.provider_configuration,
            public_ai_runtime: Some(agent_runtime.public_ai_runtime.as_ref()),
            workspace_snapshot,
            logs: &logs,
            subagent_navigation_mode: SubagentNavigationRefreshMode::Rebuild,
        })?;
    if did_stop_work {
        let state_for_persist = state.clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        spawn_service_task(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!(
                        "[webui-service] stopped conversation persist snapshot failed: {error}"
                    );
                    return;
                }
            };
            if !snapshot
                .session_runtime_state
                .contains_key(&target_session_id)
            {
                return;
            }
            if let Err(error) = persist_workspace_session_update(
                Some(&state_for_persist),
                controller_runtime_for_persist,
                &snapshot,
                target_session_id,
            )
            .await
            {
                eprintln!(
                    "[webui-service] stopped conversation background persist failed: {error}"
                );
            }
        });
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
    let (controller_runtime, workspace_snapshot, existing_plan_mode) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
            return Err(format!("未找到要切换计划模式的会话：{session_id}"));
        }
        let existing_plan_mode = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .map(|runtime_state| runtime_state.plan_mode.clone())
            .unwrap_or_default();
        (
            controller_runtime,
            workspace_snapshot.clone(),
            existing_plan_mode,
        )
    };
    let updated_at = current_unix_millis_text();
    let mut next_plan_mode = existing_plan_mode;
    if request.enabled {
        ensure_session_can_enter_plan_mode(&controller_runtime, session_id).await?;
        next_plan_mode.pending_execution_notice = false;
    }
    if !request.enabled {
        next_plan_mode.pending_execution_notice = next_plan_mode.prompt_injected;
        next_plan_mode.prompt_injected = false;
    }
    next_plan_mode.artifact = None;
    let work_mode = SessionWorkMode {
        execution_mode: if request.enabled {
            String::from("plan")
        } else {
            String::from("build")
        },
        plan_mode: next_plan_mode.clone(),
    };
    let mut addon_state = SessionAddonState::new(session_id);
    let _ = addon_state.set_work_mode(&work_mode, updated_at.clone());
    invalidate_webui_session_addon_persist_version(
        &state,
        session_id,
        SessionAddonNamespace::WorkMode,
    )?;
    let addon_persist_lock = webui_workspace_persist_lock();
    let _addon_persist_guard = addon_persist_lock.lock().await;
    let persisted_addon_state = persist_workspace_session_addon_state_update(
        controller_runtime,
        workspace_snapshot,
        session_id,
        addon_state,
    )
    .await?;
    {
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
        runtime_state.updated_at = updated_at.clone();
    }
    emit_session_addon_changed_event(
        &app,
        session_id,
        &[SessionAddonNamespace::WorkMode],
        persisted_addon_state.revision,
        latest_session_addon_updated_at(&persisted_addon_state),
    );
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
    let (controller_runtime, workspace_snapshot, mut next_vmm) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
            return Err(format!("未找到要切换 VMM 记忆策略的会话：{session_id}"));
        }
        let next_vmm = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .map(|runtime_state| runtime_state.vmm.clone())
            .unwrap_or_default();
        (controller_runtime, workspace_snapshot.clone(), next_vmm)
    };
    match request.policy.trim().to_ascii_lowercase().as_str() {
        "precheck" | "precheck_enabled" | "precheck-injection" | "precheck_injection" => {
            next_vmm.precheck_enabled = policy_value;
        }
        "writeback" | "memory_writeback" | "memory_writeback_enabled" | "post_action" => {
            next_vmm.memory_writeback_enabled = policy_value;
        }
        other => return Err(format!("不支持的 VMM 记忆策略：{other}")),
    }
    let updated_at = current_unix_millis_text();
    let mut addon_state = SessionAddonState::new(session_id);
    let _ =
        addon_state.set_typed_namespace(SessionAddonNamespace::Vmm, &next_vmm, updated_at.clone());
    invalidate_webui_session_addon_persist_version(&state, session_id, SessionAddonNamespace::Vmm)?;
    let addon_persist_lock = webui_workspace_persist_lock();
    let _addon_persist_guard = addon_persist_lock.lock().await;
    persist_workspace_session_addon_state_update(
        controller_runtime,
        workspace_snapshot,
        session_id,
        addon_state,
    )
    .await?;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone());
        let vmm_hook_runtime = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.vmm_hook_runtime.clone());
        let refreshed_workspace_snapshot = {
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
            runtime_state.vmm = next_vmm;
            runtime_state.updated_at = updated_at.clone();
            workspace.clone()
        };
        if let (Some(vmm_hook_runtime), Some(provider_configuration)) =
            (vmm_hook_runtime, provider_configuration)
        {
            vmm_hook_runtime.replace_snapshot(build_vmm_hook_config_snapshot(
                &provider_configuration,
                &refreshed_workspace_snapshot,
            ))?;
        }
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

/// Builds one context-window-scoped model notice for session AI capability changes.
/// 为会话 AI 能力变化构建一条仅在当前上下文窗口内生效的模型通知。
///
/// # Parameters
/// 参数
/// `previous_configuration` is the session configuration before the save operation.
/// `previous_configuration` 是保存操作前的会话配置。
/// `next_configuration` is the session configuration after the save operation.
/// `next_configuration` 是保存操作后的会话配置。
/// `model_capabilities` describes native capability support for the active model.
/// `model_capabilities` 描述当前模型的原生能力支持。
///
/// # Returns
/// 返回值
/// A persistent hidden-user injection that clears on context compaction, or `None` when nothing model-visible changed.
/// 调用方进入附属状态写入队列后，持久化单个完整会话附属状态文档。
pub(crate) async fn persist_workspace_session_addon_state_update(
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    addon_state: SessionAddonState,
) -> Result<SessionAddonState, String> {
    let Some((session, archived)) =
        workspace_session_with_archive_status(&workspace_snapshot, session_id)
            .map(|(session, archived)| (session.clone(), archived))
    else {
        return Err(format!("未找到要保存附属状态的会话：{session_id}"));
    };
    let task_projects = workspace_task_projects_for_persistence(&workspace_snapshot);
    vulcan_workspace_store::workspace_store::persist_single_session_addon_state_update(
        controller_runtime,
        &task_projects,
        &session,
        archived,
        &addon_state,
    )
    .await
    .map_err(|error| format!("会话附属状态保存失败：{error}"))
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
