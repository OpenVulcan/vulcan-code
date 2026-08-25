//! WebUI chained-turn dispatch after a parent turn reaches an idle point.
//! WebUI 父级回合到达空闲点后的链式回合分发逻辑。

use vulcan_service_api::ServiceEventEmitter;
use vulcan_workbench_model::{
    PendingPromptRuntimeOptions, PendingSessionPromptEntry, PendingSessionPromptSource, SessionId,
};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle,
    workspace_store::{ThreadGoalStatus, VulcanGoalStore},
};

use super::pending_prompts::enqueue_internal_pending_prompt_entry;
use super::webui_prompt_lifecycle::{
    maybe_prepare_next_webui_pending_prompt, maybe_start_webui_mailbox_turn_if_ready,
};
use super::{
    AgentRuntimeBridge, SharedServiceBridgeState, cache_session_goal_summary,
    current_unix_millis_text, current_unix_millis_value,
    runtime_state_has_queued_pending_prompt_work, session_goal_dispatch_version,
    spawn_workspace_session_runtime_state_persist_task, webui_session_has_active_run,
    webui_session_kernel_has_active_turn, webui_trace, webui_trace_if_slow,
};

/// Enqueues one durable hidden continuation entry when the session queue remains idle.
/// 当会话队列仍为空闲时，加入一条可持久化的隐藏续跑条目。
///
/// # Parameters
/// 参数
/// `runtime_state` owns the authoritative pending queue, `goal_id` identifies the active Goal
/// stage, and `created_at` is the stable enqueue timestamp.
/// `runtime_state` 持有权威待执行队列，`goal_id` 标识活动 Goal 阶段，`created_at` 是稳定入队时间。
///
/// # Returns
/// 返回值
/// `true` when a new entry was queued; `false` when existing queued work must retain priority.
/// 新条目入队时返回 `true`；已有排队工作必须保持优先级时返回 `false`。
fn enqueue_session_goal_continuation_entry_if_idle(
    runtime_state: &mut vulcan_workbench_model::SessionRuntimeStateEntry,
    goal_id: &str,
    created_at: String,
) -> Result<bool, String> {
    if runtime_state_has_queued_pending_prompt_work(runtime_state) {
        return Ok(false);
    }
    enqueue_internal_pending_prompt_entry(
        runtime_state,
        PendingSessionPromptEntry {
            id: format!(
                "webui-goal-auto-continuation-{goal_id}-{}",
                SessionId::new_v4()
            ),
            prompt: String::from("Continue working toward the active thread goal."),
            hidden_model_guidance: None,
            preview: String::from("继续执行目标"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: PendingPromptRuntimeOptions::default(),
            source: PendingSessionPromptSource::GoalContinuation,
            task_id: Some(goal_id.to_string()),
            panel_entry_id: None,
            created_at: created_at.clone(),
        },
        false,
        created_at,
        "目标自动续跑入队失败",
    )?;
    Ok(true)
}

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
    let (has_active_run, has_active_kernel_turn, has_pending_prompt) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let has_pending_prompt = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.session_runtime_state.get(&session_id))
            .is_some_and(runtime_state_has_queued_pending_prompt_work);
        (
            webui_session_has_active_run(&guard, session_id),
            webui_session_kernel_has_active_turn(&guard, session_id),
            has_pending_prompt,
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
            "session={session_id} has_active_run={has_active_run} has_active_kernel_turn={has_active_kernel_turn} has_pending_prompt={has_pending_prompt}"
        ),
    );
    if has_active_run || has_active_kernel_turn || has_pending_prompt {
        return Ok(false);
    }
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let goal = loop {
        let version_before_read = session_goal_dispatch_version(state, session_id)?;
        let goal = goal_store.get_thread_goal(session_id).await?;
        let version_after_read = session_goal_dispatch_version(state, session_id)?;
        if version_before_read == version_after_read {
            break goal;
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
    let goal_id = goal.goal_id.clone();
    cache_session_goal_summary(state, session_id, Some(goal))?;
    let enqueued = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .entry(session_id)
            .or_default();
        enqueue_session_goal_continuation_entry_if_idle(
            runtime_state,
            &goal_id,
            current_unix_millis_text(),
        )?
    };
    if !enqueued {
        return Ok(false);
    }
    // The queue entry survives temporary blockers such as an unresolved plan decision and is retried by the existing dispatcher.
    // 队列条目会跨越计划决策未完成等暂态阻塞，并由现有分发器继续重试。
    spawn_workspace_session_runtime_state_persist_task(
        state.clone(),
        controller_runtime.clone(),
        session_id,
        Some(String::from("goal_auto_continuation_queued")),
    );
    let started = maybe_prepare_next_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        false,
    )
    .await?;
    webui_trace(
        "goal_continuation_queued",
        format!("session={session_id} started={started}"),
    );
    Ok(started)
}

#[cfg(test)]
mod tests {
    //! Goal continuation queue tests.
    //! Goal 续跑队列测试。

    use super::*;

    /// Verifies autonomous Goal work becomes one hidden durable queue owner and never duplicates itself.
    /// 验证目标自动工作会成为唯一隐藏持久队列所有者，且不会重复入队。
    #[test]
    fn goal_continuation_is_queued_once_for_idle_runtime_state() {
        let mut runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry::default();

        assert!(
            enqueue_session_goal_continuation_entry_if_idle(
                &mut runtime_state,
                "goal-stage-1",
                String::from("1000"),
            )
            .expect("idle Goal continuation should enqueue")
        );
        assert!(
            !enqueue_session_goal_continuation_entry_if_idle(
                &mut runtime_state,
                "goal-stage-1",
                String::from("1001"),
            )
            .expect("existing Goal continuation should retain ownership")
        );
        assert_eq!(runtime_state.ui_state.pending_prompt_queue.len(), 1);
        let entry = &runtime_state.ui_state.pending_prompt_queue[0];
        assert_eq!(entry.source, PendingSessionPromptSource::GoalContinuation);
        assert_eq!(entry.task_id.as_deref(), Some("goal-stage-1"));
        assert!(entry.panel_entry_id.is_none());

        // first_entry_id proves repeated autonomous turns do not reuse one queue identity.
        // first_entry_id 证明重复自动续跑不会复用同一个队列身份。
        let first_entry_id = entry.id.clone();
        runtime_state.ui_state.pending_prompt_queue.clear();
        assert!(
            enqueue_session_goal_continuation_entry_if_idle(
                &mut runtime_state,
                "goal-stage-1",
                String::from("1002"),
            )
            .expect("the next completed Goal turn should enqueue another continuation")
        );
        assert_ne!(
            runtime_state.ui_state.pending_prompt_queue[0].id,
            first_entry_id
        );
    }
}
