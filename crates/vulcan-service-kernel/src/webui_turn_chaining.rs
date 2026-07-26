//! WebUI chained-turn dispatch after a parent turn reaches an idle point.
//! WebUI 父级回合到达空闲点后的链式回合分发逻辑。

use std::collections::{BTreeMap, BTreeSet};

use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptPrecheckPlan, SharedPromptTurnBeginInput, begin_shared_prompt_turn,
};
use vulcan_service_api::ServiceEventEmitter;
use vulcan_session_core::ActiveTurnKind;
use vulcan_workbench_model::{PendingSessionPromptSource, SessionId, SessionRunPhase};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle,
    workspace_store::{ThreadGoalStatus, VulcanGoalStore},
};

use super::conversation_submission::effective_session_disabled_ai_capability_kinds;
use super::session_hydration::{
    capture_preserved_workspace_run_state, load_workspace_for_submission,
    preserve_active_session_run_state, retained_submission_snapshot_has_session_context,
};
use super::webui_prompt_lifecycle::{
    maybe_prepare_next_webui_pending_prompt, maybe_start_webui_mailbox_turn_if_ready,
    start_webui_precheck_or_continue,
};
use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, active_kernel_session_ids,
    active_reasoning_selection_effort, apply_webui_prompt_begin_outer_steps,
    build_vmm_hook_config_snapshot, cache_session_goal_summary,
    commit_workspace_snapshot_and_emit_session_phase_delta, current_unix_millis_value,
    ensure_webui_goal_session_kernel_turn_start_allowed, ensure_webui_session_runtime_state,
    runtime_state_has_queued_pending_prompt_work, session_goal_dispatch_version,
    session_has_pending_plan_artifact, start_webui_goal_session_kernel_turn,
    webui_session_has_active_run, webui_session_kernel_has_active_turn, webui_trace,
    webui_trace_if_slow, workspace_path_for_session,
};

/// Starts the next WebUI chained turn after one stream finishes.
/// 在单个执行流结束后启动下一条 WebUI 链式回合。
///
/// # Parameters
/// 参数
/// `app` emits workbench updates caused by chained turn startup.
/// `app` 发出链式回合启动产生的工作台更新。
/// `state` owns retained service state and runtime kernels.
/// `state` 持有保留服务状态与运行时内核。
/// `controller_runtime` loads workspace and goal state when retained state is insufficient.
/// `controller_runtime` 在保留状态不足时加载工作区与目标状态。
/// `agent_runtime` owns prompt-turn hook and provider configuration handles.
/// `agent_runtime` 持有提示词回合 hook 与 Provider 配置句柄。
/// `finished_session_id` identifies the session whose previous turn has finished.
/// `finished_session_id` 标识刚结束上一回合的会话。
///
/// # Returns
/// 返回值
/// `true` when mailbox, pending prompt, or goal continuation starts a chained turn.
/// 当 mailbox、待发送提示词或目标续跑启动链式回合时返回 `true`。
pub(super) async fn maybe_start_webui_turn_chain_after_finished(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    finished_session_id: SessionId,
) -> bool {
    // Mailbox input has priority because it represents completed child-agent work for the parent turn.
    // mailbox 输入优先，因为它表示父回合等待的子代理工作已经完成。
    let mailbox_turn_check_started_at = current_unix_millis_value();
    let started_mailbox_turn = match maybe_start_webui_mailbox_turn_if_ready(
        app,
        state,
        controller_runtime,
        agent_runtime,
        finished_session_id,
    )
    .await
    {
        Ok(started) => started,
        Err(error) => {
            eprintln!("[webui-service] prepare mailbox turn after stream failed: {error}");
            false
        }
    };
    webui_trace_if_slow(
        "stream_mailbox_turn_check_slow",
        mailbox_turn_check_started_at,
        format!("session={finished_session_id} started={started_mailbox_turn}"),
    );

    // Pending prompts are considered only after mailbox-triggered parent work stays idle.
    // 只有 mailbox 触发的父级工作未启动时，才检查待发送提示词。
    let next_prompt_check_started_at = current_unix_millis_value();
    let started_next_prompt = if started_mailbox_turn {
        true
    } else {
        match maybe_prepare_next_webui_pending_prompt(
            app,
            state,
            controller_runtime,
            agent_runtime,
            finished_session_id,
            false,
        )
        .await
        {
            Ok(started) => started,
            Err(error) => {
                eprintln!("[webui-service] prepare next pending prompt failed: {error}");
                false
            }
        }
    };
    webui_trace_if_slow(
        "stream_next_prompt_check_slow",
        next_prompt_check_started_at,
        format!("session={finished_session_id} started={started_next_prompt}"),
    );

    // Goal continuation is hidden work and must yield to user-visible or mailbox-driven queues.
    // 目标续跑属于隐藏工作，必须让位于用户可见队列与 mailbox 驱动队列。
    let goal_continuation_check_started_at = current_unix_millis_value();
    let started_goal_continuation = if started_next_prompt {
        false
    } else {
        match maybe_start_session_goal_continuation_if_idle(
            app,
            state,
            controller_runtime,
            agent_runtime,
            finished_session_id,
        )
        .await
        {
            Ok(started) => started,
            Err(error) => {
                eprintln!("[webui-service] prepare goal continuation failed: {error}");
                false
            }
        }
    };
    webui_trace_if_slow(
        "stream_goal_continuation_check_slow",
        goal_continuation_check_started_at,
        format!("session={finished_session_id} started={started_goal_continuation}"),
    );
    let started_next_prompt = started_next_prompt || started_goal_continuation;
    webui_trace(
        "stream_finished_next_prompt",
        format!("session={finished_session_id} started_next_prompt={started_next_prompt}"),
    );
    started_next_prompt
}

/// Starts one hidden session-goal continuation turn when visible queues stay idle.
/// 当可见队列保持空闲时启动一个隐藏的会话目标续跑回合。
pub(super) async fn maybe_start_session_goal_continuation_if_idle(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
) -> Result<bool, String> {
    let continuation_prepare_started_at = current_unix_millis_value();
    let (
        has_active_run,
        has_active_kernel_turn,
        has_pending_prompt,
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
        let has_pending_prompt = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.session_runtime_state.get(&session_id))
            .is_some_and(runtime_state_has_queued_pending_prompt_work);
        let has_pending_plan_artifact = guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|snapshot| session_has_pending_plan_artifact(snapshot, session_id));
        let retained_workspace_snapshot = guard.workspace_snapshot.as_ref().and_then(|snapshot| {
            retained_submission_snapshot_has_session_context(snapshot, session_id)
                .then(|| snapshot.clone())
        });
        (
            webui_session_has_active_run(&guard, session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
            has_pending_prompt,
            has_pending_plan_artifact,
            preserved_workspace_run_state,
            retained_workspace_snapshot,
        )
    };
    webui_trace_if_slow(
        "goal_continuation_prepare_slow",
        continuation_prepare_started_at,
        format!("session={session_id}"),
    );
    webui_trace(
        "goal_continuation_prepare_check",
        format!(
            "session={session_id} has_active_run={has_active_run} has_active_kernel_turn={has_active_kernel_turn} has_pending_prompt={has_pending_prompt} has_pending_plan_artifact={has_pending_plan_artifact}"
        ),
    );
    if has_active_run || has_active_kernel_turn || has_pending_prompt || has_pending_plan_artifact {
        return Ok(false);
    }
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let (goal, goal_dispatch_version) = loop {
        let version_before_read = session_goal_dispatch_version(state, session_id)?;
        let goal = goal_store.get_thread_goal(session_id).await?;
        let version_after_read = session_goal_dispatch_version(state, session_id)?;
        if version_before_read == version_after_read {
            break (goal, version_after_read);
        }
    };
    let Some(goal) = goal else {
        cache_session_goal_summary(state, session_id, None)?;
        webui_trace("goal_continuation_no_goal", format!("session={session_id}"));
        return Ok(false);
    };
    if goal.status != ThreadGoalStatus::Active {
        cache_session_goal_summary(state, session_id, Some(goal))?;
        webui_trace(
            "goal_continuation_inactive_goal",
            format!("session={session_id}"),
        );
        return Ok(false);
    }
    cache_session_goal_summary(state, session_id, Some(goal))?;
    let (mut workspace_snapshot, workspace_source) =
        if let Some(workspace_snapshot) = retained_workspace_snapshot {
            (workspace_snapshot, "retained")
        } else {
            (
                load_workspace_for_submission(controller_runtime.clone(), Some(session_id)).await?,
                "disk",
            )
        };
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut workspace_snapshot,
    );
    ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None)?;
    // disabled_ai_capability_kinds keeps the provider-visible tool catalog identical to a normal turn using the same session defaults.
    // disabled_ai_capability_kinds 使 Provider 可见工具目录与使用相同会话默认值的普通回合保持一致。
    let disabled_ai_capability_kinds = effective_session_disabled_ai_capability_kinds(
        &workspace_snapshot,
        session_id,
        &agent_runtime.ai_tool_configuration,
        None,
    )?;
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
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        ensure_webui_goal_session_kernel_turn_start_allowed(
            &guard,
            session_id,
            goal_dispatch_version,
        )?;
        let start = begin_shared_prompt_turn(
            &mut workspace_snapshot,
            &mut hook_runtime,
            SharedPromptTurnBeginInput {
                session_id,
                session_title: &session_title,
                workspace_path: &workspace_path,
                prompt_entry_id: format!("webui-goal-continuation-{}", SessionId::new_v4()),
                prompt: "Continue working toward the active thread goal.",
                hidden_model_guidance: None,
                provider_visible_agent_messages: Vec::new(),
                attachments: Vec::new(),
                append_canonical_user_message: false,
                append_visible_user_prompt_record: false,
                source: PendingSessionPromptSource::GoalContinuation,
                execution_mode: String::from("build"),
                reasoning_effort: active_reasoning_selection_effort(
                    &agent_runtime.provider_configuration,
                ),
                disabled_ai_capability_kinds,
                disabled_ai_capability_target_selectors: BTreeSet::new(),
                ai_capability_target_bindings: BTreeMap::new(),
            },
        )?;
        start_webui_goal_session_kernel_turn(
            &mut guard,
            session_id,
            goal_dispatch_version,
            &start.preparation_turn_id,
            ActiveTurnKind::GoalContinuation,
        )?;
        start
    };
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
        ))?;
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
        "goal_continuation_started",
        format!("session={session_id} source={workspace_source}"),
    );
    Ok(true)
}
