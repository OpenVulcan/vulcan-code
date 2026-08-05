//! session kernel tests.
//! session kernel tests。

use super::*;
use crate::pending_prompts::dispatch_queued_pending_prompt_hook;
use vulcan_session_boundary::{
    SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
};

/// Inserts one Workbench conversation row matching an existing test workspace session.
/// 插入一条与现有测试工作区会话匹配的 Workbench 会话行。
///
/// # Parameters
/// 参数
/// `state` owns the retained Workbench projection and `session_id` identifies the matching session.
/// `state` 持有保留 Workbench 投影，`session_id` 标识对应会话。
///
/// # Returns
/// 返回值
/// This helper returns no value after inserting the deterministic conversation fixture.
/// 该助手插入确定性的会话夹具后不返回值。
fn insert_test_workbench_conversation(state: &mut ServiceBridgeState, session_id: SessionId) {
    state.workbench.conversations.push(ConversationSummary {
        id: session_id.to_string(),
        title: String::from("测试会话"),
        status: SessionState::Completed.label().to_string(),
        run_phase: SessionRunPhase::Ready.to_string(),
        last_updated_label: String::from("刚刚"),
        last_updated_at: String::from("100"),
        unread: false,
        attention: None,
        project_id: Some(String::from("project-a")),
        fork_source: None,
    });
}

/// Verifies the WebUI session kernel helper enforces one active turn.
/// 验证 WebUI 会话内核辅助函数会强制单活动回合。
#[test]
fn webui_session_kernel_rejects_second_active_turn() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);

    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("first kernel turn should start");
    let second_start = start_webui_session_kernel_user_turn(&mut state, session_id, "turn-b");

    assert!(second_start.is_err());
    assert!(
        state
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies an authoritative lifecycle transition blocks every new kernel turn for its session.
/// 验证权威生命周期转换会阻止对应会话启动任何新内核回合。
#[test]
fn webui_session_kernel_rejects_turn_during_lifecycle_transition() {
    // session_id identifies the session reserved by the lifecycle transition.
    // session_id 标识被生命周期转换预留的会话。
    let session_id = SessionId::from_u128(0x94);
    // state owns both the lifecycle reservation and runtime kernel table under test.
    // state 持有测试中的生命周期预留状态与运行内核表。
    let mut state = test_service_state_with_parent_session(session_id);
    state.session_turn_admission_reservations.insert(
        session_id,
        SessionTurnAdmissionReservation {
            reservation_id: Uuid::new_v4(),
            kind: SessionTurnAdmissionReservationKind::LifecycleTransition,
        },
    );

    let start_result = start_webui_session_kernel_user_turn(&mut state, session_id, "blocked-turn");

    assert!(start_result.is_err());
    assert!(!webui_session_kernel_has_active_turn(&state, session_id));
}

/// Verifies a Goal mutation blocks registration in-flight and permanently retires captured authority.
/// 验证 Goal 变更会阻止进行中的注册，并永久退役已捕获的权威版本。
#[test]
fn goal_dispatch_authority_rejects_turns_during_and_after_mutation() {
    // session_id identifies the Goal session whose dispatch authority changes.
    // session_id 标识分发权威发生变化的 Goal 会话。
    let session_id = SessionId::from_u128(0x95);
    // state owns the exclusion reservation, authority version, and runtime kernel atomically.
    // state 原子持有互斥预留、权威版本与运行时 kernel。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    // captured_version represents a Goal dispatch validated before the mutation begins.
    // captured_version 表示在变更开始前完成校验的 Goal 分发版本。
    let captured_version = session_goal_dispatch_version(&state, session_id)
        .expect("initial Goal dispatch version should be readable");
    // transition_guard prevents registration while the durable Goal mutation is incomplete.
    // transition_guard 在持久 Goal 变更尚未完成时阻止回合注册。
    let transition_guard = begin_session_turn_start_exclusion(&state, [session_id], "更新目标状态")
        .expect("Goal mutation should reserve turn registration");
    {
        // guard exposes the shared service state for the registration attempt under test.
        // guard 为测试中的注册尝试提供共享服务状态。
        let mut guard = state.lock().expect("service state lock");
        let blocked = start_webui_goal_session_kernel_turn(
            &mut guard,
            session_id,
            captured_version,
            "turn-during-mutation",
            ActiveTurnKind::UserPrompt,
        );
        assert!(blocked.is_err());
        assert!(!webui_session_kernel_has_active_turn(&guard, session_id));
    }
    // current_version commits the in-process invalidation paired with the durable mutation.
    // current_version 提交与持久变更配对的进程内失效版本。
    let current_version = advance_session_goal_dispatch_version(&state, session_id)
        .expect("Goal dispatch version should advance");
    drop(transition_guard);
    // guard owns the atomic version check and kernel registration after mutation completion.
    // guard 在变更完成后持有原子版本校验与 kernel 注册。
    let mut guard = state.lock().expect("service state lock");
    let stale = start_webui_goal_session_kernel_turn(
        &mut guard,
        session_id,
        captured_version,
        "turn-after-mutation",
        ActiveTurnKind::UserPrompt,
    );
    assert!(stale.is_err());
    assert!(!webui_session_kernel_has_active_turn(&guard, session_id));
    start_webui_goal_session_kernel_turn(
        &mut guard,
        session_id,
        current_version,
        "turn-current-authority",
        ActiveTurnKind::UserPrompt,
    )
    .expect("current Goal authority should register exactly one turn");
    assert!(webui_session_kernel_has_active_turn(&guard, session_id));
}

/// Verifies a new user turn clears stale wait-agent coordination from earlier turns.
/// 验证新的用户回合会清理旧回合残留的 wait_agent 协调状态。
#[test]
fn webui_session_kernel_user_turn_clears_wait_agent_coordination() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);
    state
        .parent_sessions_waiting_for_subagents
        .insert(session_id);
    session_runtime_kernel_mut(&mut state, session_id)
        .register_wait_scope("wait-a", vec![String::from("agent-a")], 10)
        .expect("wait scope should register");

    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("kernel turn should start");

    assert!(
        !state
            .parent_sessions_waiting_for_subagents
            .contains(&session_id)
    );
    assert_eq!(
        state
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .wait_scope_count(),
        0
    );
}

/// Verifies starting a normal user turn does not pre-seed wait_agent interruption.
/// 验证普通用户回合启动不会预置 wait_agent 打断信号。
#[tokio::test]
async fn webui_session_kernel_user_turn_does_not_preinterrupt_wait_agent() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);
    state.agent_runtime = Some(test_agent_runtime_bridge());
    let task_runtime = state
        .agent_runtime
        .as_ref()
        .expect("agent runtime should be installed")
        .task_runtime
        .clone();

    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("kernel turn should start");
    let wait_result = tokio::time::timeout(
        std::time::Duration::from_millis(50),
        task_runtime.invoke_wait_agent(session_id, serde_json::json!({ "timeout_ms": 10000 })),
    )
    .await;

    assert!(wait_result.is_err());
}

/// Verifies a normally queued prompt does not interrupt an active wait_agent call.
/// 验证普通排队提示词不会打断正在活动的 wait_agent 调用。
#[tokio::test]
async fn webui_pending_prompt_enqueue_does_not_interrupt_active_wait_agent() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);
    state.agent_runtime = Some(test_agent_runtime_bridge());
    let agent_runtime = state
        .agent_runtime
        .as_ref()
        .expect("agent runtime should be installed")
        .clone();
    let task_runtime = agent_runtime.task_runtime.clone();
    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("kernel turn should start");
    state
        .parent_sessions_waiting_for_subagents
        .insert(session_id);
    session_runtime_kernel_mut(&mut state, session_id)
        .register_wait_scope("wait-a", Vec::<String>::new(), 10)
        .expect("wait scope should register");

    let mut wait_task = tokio::spawn(async move {
        task_runtime
            .invoke_wait_agent(session_id, serde_json::json!({ "timeout_ms": 10000 }))
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;

    let workspace_snapshot = state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist");
    let workspace_path = workspace_path_for_session(workspace_snapshot, session_id);
    enqueue_webui_pending_prompt_from_submission(
        workspace_snapshot,
        &agent_runtime,
        session_id,
        &workspace_path,
        String::from("queued follow-up"),
        None,
        Vec::new(),
        None,
        None,
    )
    .expect("queued prompt should be stored");

    let wait_result =
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut wait_task).await;
    assert!(wait_result.is_err());
    wait_task.abort();
}

/// Verifies a queued workspace draft cannot create a Hook turn before explicit post-commit dispatch.
/// 验证排队工作区草稿无法在显式提交后分发前创建 Hook 回合。
#[test]
fn queued_prompt_hook_waits_for_authoritative_workspace_commit() {
    // session_id identifies the regular session receiving the queued follow-up.
    // session_id 标识接收排队后续提示的普通会话。
    let session_id = SessionId::new_v4();
    // service_state supplies the detached workspace draft used by queue construction.
    // service_state 提供队列构建使用的分离工作区草稿。
    let mut service_state = test_service_state_with_parent_session(session_id);
    // agent_runtime owns the Hook runtime inspected before and after dispatch.
    // agent_runtime 持有分发前后检查的 Hook 运行时。
    let agent_runtime = test_agent_runtime_bridge();
    // workspace is the detached queue draft that is not authoritative yet.
    // workspace 是尚未成为权威状态的分离队列草稿。
    let workspace = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace fixture should exist");
    // workspace_path is the authoritative session path captured by the event descriptor.
    // workspace_path 是事件描述捕获的权威会话路径。
    let workspace_path = workspace_path_for_session(workspace, session_id);
    // hook_event describes the queued prompt without mutating Hook runtime.
    // hook_event 描述排队提示词，但不修改 Hook 运行时。
    let hook_event = enqueue_webui_pending_prompt_from_submission(
        workspace,
        &agent_runtime,
        session_id,
        &workspace_path,
        String::from("queued after commit"),
        None,
        Vec::new(),
        None,
        None,
    )
    .expect("queued prompt draft should be built");

    assert!(
        !agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("hook runtime lock")
            .has_active_turn(session_id)
    );
    dispatch_queued_pending_prompt_hook(&agent_runtime, hook_event)
        .expect("committed queue event should dispatch");
    assert!(
        agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("hook runtime lock")
            .has_active_turn(session_id)
    );
}

/// Verifies Hook lock failure cannot mutate or remove an already constructed queue entry.
/// 验证 Hook 锁失败无法修改或移除已经构建的队列条目。
#[test]
fn queued_prompt_hook_lock_failure_preserves_queue_entry() {
    // session_id identifies the regular session receiving the queued follow-up.
    // session_id 标识接收排队后续提示的普通会话。
    let session_id = SessionId::new_v4();
    // service_state supplies the workspace queue state inspected after Hook failure.
    // service_state 提供 Hook 失败后检查的工作区队列状态。
    let mut service_state = test_service_state_with_parent_session(session_id);
    // agent_runtime owns the Hook mutex deliberately poisoned below.
    // agent_runtime 持有下方刻意中毒的 Hook 互斥锁。
    let agent_runtime = test_agent_runtime_bridge();
    // workspace is the detached queue state preserved independently of Hook notification.
    // workspace 是独立于 Hook 通知保留的分离队列状态。
    let workspace = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace fixture should exist");
    // workspace_path is the exact session path captured with the queued entry.
    // workspace_path 是随排队条目捕获的精确会话路径。
    let workspace_path = workspace_path_for_session(workspace, session_id);
    // hook_event describes the queue mutation already present in the workspace.
    // hook_event 描述工作区中已经存在的队列变更。
    let hook_event = enqueue_webui_pending_prompt_from_submission(
        workspace,
        &agent_runtime,
        session_id,
        &workspace_path,
        String::from("queue survives hook failure"),
        None,
        Vec::new(),
        None,
        None,
    )
    .expect("queued prompt draft should be built");
    // poisoned_hook_runtime shares the exact mutex used by post-commit dispatch.
    // poisoned_hook_runtime 共享提交后分发使用的精确互斥锁。
    let poisoned_hook_runtime = Arc::clone(&agent_runtime.conversation_hook_runtime);
    // poison_result proves the deliberate panic poisoned the Hook mutex.
    // poison_result 证明刻意 panic 已使 Hook 互斥锁中毒。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // _guard keeps the mutex held until the deliberate panic poisons it.
        // _guard 将互斥锁保持到刻意 panic，使其被标记为中毒。
        let _guard = poisoned_hook_runtime
            .lock()
            .expect("hook runtime should lock before deliberate panic");
        panic!("poison queued prompt hook runtime");
    }));
    assert!(poison_result.is_err());

    // error is the explicit post-commit Hook diagnostic.
    // error 是显式的提交后 Hook 诊断。
    let error = dispatch_queued_pending_prompt_hook(&agent_runtime, hook_event)
        .expect_err("poisoned Hook runtime should reject dispatch");
    assert!(error.contains("failed to lock conversation hook runtime"));
    assert_eq!(
        workspace
            .session_runtime_state
            .get(&session_id)
            .map(|runtime_state| runtime_state.ui_state.pending_prompt_panel_queue.len()),
        Some(1)
    );
}

/// Verifies runtime-event Hook routing reports poisoned tool dispatch while ignoring unrelated events before locking.
/// 验证运行时事件 Hook 路由会报告中毒的工具派发，并在加锁前忽略无关事件。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if target diagnostics or pre-lock filtering regress.
/// 此测试无返回值；目标诊断或加锁前过滤退化时测试失败。
#[test]
fn runtime_event_hook_lock_failure_is_explicit_only_for_tool_events() {
    // session_id identifies the authoritative event owner used in diagnostics.
    // session_id 标识诊断中使用的权威事件所有者。
    let session_id = SessionId::new_v4();
    // agent_runtime owns the Hook mutex deliberately poisoned below.
    // agent_runtime 持有下方刻意中毒的 Hook 互斥锁。
    let agent_runtime = test_agent_runtime_bridge();
    // tool_event requires Hook dispatch and therefore must observe lock failure.
    // tool_event 需要 Hook 派发，因此必须观察到锁失败。
    let tool_event = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "tool started",
        "Tool started",
    );
    // unrelated_event must be filtered without touching the poisoned Hook mutex.
    // unrelated_event 必须在不触碰中毒 Hook 互斥锁的情况下被过滤。
    let unrelated_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateStarted,
        "delegate started",
        "Delegate started",
    );
    // poisoned_hook_runtime shares the exact mutex used by runtime-event dispatch.
    // poisoned_hook_runtime 共享运行时事件派发使用的精确互斥锁。
    let poisoned_hook_runtime = Arc::clone(&agent_runtime.conversation_hook_runtime);
    // poison_result proves the deliberate panic poisoned the Hook mutex.
    // poison_result 证明刻意 panic 已使 Hook 互斥锁中毒。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // _guard keeps the mutex held until the deliberate panic poisons it.
        // _guard 将互斥锁保持到刻意 panic，使其被标记为中毒。
        let _guard = poisoned_hook_runtime
            .lock()
            .expect("hook runtime should lock before deliberate panic");
        panic!("poison runtime event hook runtime");
    }));
    assert!(poison_result.is_err());

    // explicit_execution_error proves detached correlation failures preserve the exact generation.
    // explicit_execution_error 证明分离关联失败保留精确执行代次。
    let explicit_execution_error =
        dispatch_runtime_event_hook(&agent_runtime, session_id, &tool_event, "execution-27")
            .expect_err("poisoned Hook runtime should reject explicit-execution tool dispatch");
    assert!(explicit_execution_error.contains("execution-27"));
    dispatch_runtime_event_hook(&agent_runtime, session_id, &unrelated_event, "execution-27")
        .expect("unrelated runtime events should not acquire the poisoned Hook lock");
}

/// Verifies stopping an active waiting turn interrupts the active wait_agent call.
/// 验证停止活跃等待回合会打断正在等待的 wait_agent 调用。
#[tokio::test]
async fn webui_session_kernel_stop_interrupts_active_wait_agent() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);
    state.agent_runtime = Some(test_agent_runtime_bridge());
    let task_runtime = state
        .agent_runtime
        .as_ref()
        .expect("agent runtime should be installed")
        .task_runtime
        .clone();
    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("kernel turn should start");
    state
        .parent_sessions_waiting_for_subagents
        .insert(session_id);
    session_runtime_kernel_mut(&mut state, session_id)
        .register_wait_scope("wait-a", Vec::<String>::new(), 10)
        .expect("wait scope should register");

    let wait_task = tokio::spawn(async move {
        task_runtime
            .invoke_wait_agent(session_id, serde_json::json!({ "timeout_ms": 10000 }))
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;

    assert!(stop_webui_session_kernel_generation(&mut state, session_id));
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), wait_task)
        .await
        .expect("wait task should be woken")
        .expect("wait task should not panic")
        .expect("wait_agent should return a tool result");

    assert_eq!(result.model_output["interrupted"], true);
    assert_eq!(
        result.model_output["message"],
        serde_json::json!("Wait interrupted by new input.")
    );
}

/// Verifies kernel-active sessions are considered actively running without legacy stream handles.
/// 验证即使没有旧流任务句柄，内核活动中的会话也会被视为正在运行。
#[test]
fn webui_session_active_run_uses_kernel_state_without_stream_handle() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);

    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("kernel turn should start");

    assert!(!state.active_stream_tasks.contains_key(&session_id));
    assert!(webui_session_has_active_run(&state, session_id));
}

/// Verifies an active stream task retains the Goal identity captured at launch.
/// 验证活动流任务会保留启动时捕获的 Goal 身份。
#[tokio::test]
async fn active_stream_task_retains_launch_goal_identity() {
    let task = ActiveStreamTask::new(
        Uuid::new_v4(),
        spawn_service_task(async {}),
        Some(String::from("goal-bound-to-stream")),
    );

    assert_eq!(task.session_goal_id(), Some("goal-bound-to-stream"));
}

/// Verifies an exact active stream can bind a Goal created after execution launch.
/// 验证精确活动流可以绑定执行启动后创建的目标。
#[tokio::test]
async fn active_stream_task_binds_goal_created_during_execution() {
    // session_id identifies the active stream registry slot.
    // session_id 标识活动流注册槽位。
    let session_id = SessionId::new_v4();
    // task_id identifies the exact execution allowed to update its Goal binding.
    // task_id 标识获准更新目标绑定的精确执行。
    let task_id = Uuid::new_v4();
    // stale_task_id proves a superseded execution cannot modify the retained owner.
    // stale_task_id 证明已被替代的执行不能修改保留所有者。
    let stale_task_id = Uuid::new_v4();
    // state starts with an execution that had no Goal at launch.
    // state 初始包含一个启动时尚无目标的执行。
    let mut state = test_service_state_with_parent_session(session_id);
    state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(task_id, spawn_service_task(async {}), None),
    );

    assert!(!bind_active_stream_task_goal_if_id(
        &mut state,
        session_id,
        stale_task_id,
        String::from("goal-stale"),
    ));
    assert!(bind_active_stream_task_goal_if_id(
        &mut state,
        session_id,
        task_id,
        String::from("goal-created-during-run"),
    ));
    assert_eq!(
        state
            .active_stream_tasks
            .get(&session_id)
            .and_then(ActiveStreamTask::session_goal_id),
        Some("goal-created-during-run")
    );
    // removedTask proves terminal handling can recover the binding before another Provider request starts.
    // removedTask 证明终态处理可在下一次 Provider 请求开始前恢复该绑定。
    let removed_task = remove_active_stream_task_if_id(&mut state, session_id, task_id)
        .expect("exact active task should be removable");
    assert_eq!(
        removed_task.session_goal_id(),
        Some("goal-created-during-run")
    );
}

/// Verifies stale stream cleanup cannot remove a newer task in the same session slot.
/// 验证过期流清理无法删除同一会话槽位中的新任务。
#[tokio::test]
async fn active_stream_task_cleanup_preserves_replacement_identity() {
    let session_id = SessionId::new_v4();
    let stale_task_id = Uuid::new_v4();
    let replacement_task_id = Uuid::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);
    state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(
            replacement_task_id,
            spawn_service_task(async {}),
            Some(String::from("goal-new")),
        ),
    );

    assert!(remove_active_stream_task_if_id(&mut state, session_id, stale_task_id).is_none());
    assert_eq!(
        state
            .active_stream_tasks
            .get(&session_id)
            .and_then(ActiveStreamTask::session_goal_id),
        Some("goal-new")
    );
    assert!(remove_active_stream_task_if_id(&mut state, session_id, replacement_task_id).is_some());
    assert!(!state.active_stream_tasks.contains_key(&session_id));
}

/// Verifies early stream exhaustion removes the exact owner and stops its active kernel turn.
/// 验证流提前耗尽会移除精确所有者并停止其活动内核回合。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This asynchronous test returns no value and fails when cleanup ownership or kernel terminalization is incorrect.
/// 此异步测试无返回值；清理所有权或内核终结不正确时失败。
#[tokio::test]
async fn early_stream_exhaustion_cleans_exact_owner_and_kernel_turn() {
    // session_id identifies the regular session whose stream ends early.
    // session_id 标识流提前结束的常规会话。
    let session_id = SessionId::new_v4();
    // task_id is the exact identity registered for the exiting stream.
    // task_id 是为退出流注册的精确身份。
    let task_id = Uuid::new_v4();
    // service_state owns the active kernel turn and matching stream entry before cleanup.
    // service_state 在清理前持有活动内核回合与匹配流条目。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-a")
        .expect("kernel turn should start");
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(task_id, spawn_service_task(async {}), None),
    );
    // state exposes the authoritative registry to the exit handler.
    // state 向退出处理器提供权威注册表。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    assert_eq!(
        handle_stream_task_exit(
            &state,
            session_id,
            task_id,
            StreamTaskExitReason::StreamExhausted,
        ),
        StreamTaskExitOutcome::EarlyTerminationCleaned
    );

    // guard exposes the retained registry and kernel result after cleanup.
    // guard 在清理后提供保留注册表与内核结果访问。
    let guard = state.lock().expect("service state lock");
    assert!(!guard.active_stream_tasks.contains_key(&session_id));
    assert!(
        !guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies stale-generation exit cleanup cannot remove or terminalize a replacement task.
/// 验证过期代际退出清理无法移除或终结替代任务。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This asynchronous test returns no value and fails if replacement ownership is mutated.
/// 此异步测试无返回值；若替代任务所有权被修改则失败。
#[tokio::test]
async fn stale_generation_exit_preserves_replacement_owner_and_kernel_turn() {
    // session_id identifies the shared session slot occupied by the replacement.
    // session_id 标识替代任务占用的共享会话槽位。
    let session_id = SessionId::new_v4();
    // stale_task_id identifies the exiting task that no longer owns the slot.
    // stale_task_id 标识不再拥有槽位的退出任务。
    let stale_task_id = Uuid::new_v4();
    // replacement_task_id identifies the current authoritative stream owner.
    // replacement_task_id 标识当前权威流所有者。
    let replacement_task_id = Uuid::new_v4();
    // service_state owns the replacement entry and still-active kernel turn.
    // service_state 持有替代条目与仍活动的内核回合。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-a")
        .expect("kernel turn should start");
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(
            replacement_task_id,
            spawn_service_task(async {}),
            Some(String::from("goal-new")),
        ),
    );
    // state exposes the replacement-owned registry to stale cleanup.
    // state 向过期清理提供由替代任务拥有的注册表。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    assert_eq!(
        handle_stream_task_exit(
            &state,
            session_id,
            stale_task_id,
            StreamTaskExitReason::StaleKernelGeneration,
        ),
        StreamTaskExitOutcome::StaleOwner
    );

    // guard exposes the registry and kernel state that stale cleanup must preserve.
    // guard 提供过期清理必须保留的注册表与内核状态。
    let guard = state.lock().expect("service state lock");
    assert_eq!(
        guard
            .active_stream_tasks
            .get(&session_id)
            .and_then(ActiveStreamTask::session_goal_id),
        Some("goal-new")
    );
    assert!(
        guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies an owned start-gate registration failure atomically fails its phase and kernel turn.
/// 验证拥有所有权的启动闸门注册失败会原子地失败其阶段与内核回合。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if owned failure terminalization is incomplete.
/// 此测试无返回值；若所有者失败终结不完整则失败。
#[test]
fn owned_registration_failure_finalizes_failed_phase_and_kernel_turn() {
    // session_id identifies the regular session whose task registration failed.
    // session_id 标识任务注册失败的常规会话。
    let session_id = SessionId::new_v4();
    // service_state owns the active kernel turn before registration failure handling.
    // service_state 在注册失败处理前持有活动内核回合。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-a")
        .expect("kernel turn should start");
    // expected_generation binds cleanup to the exact failed kernel turn.
    // expected_generation 将清理绑定到精确失败内核回合。
    let expected_generation =
        webui_session_kernel_generation(&service_state, session_id).expect("kernel generation");
    // state exposes the retained transaction boundary to registration failure handling.
    // state 向注册失败处理提供保留事务边界。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app discards the emitted Failed delta because retained state is authoritative in this test.
    // app 丢弃发送的 Failed 增量，因为本测试以保留状态为权威。
    let app = ServiceEventEmitter::new(Vec::new());
    // registration_error proves exact cleanup removed the failed task's installed entry.
    // registration_error 证明精确清理移除了失败任务安装的条目。
    let registration_error = ActiveStreamTaskRegistrationError::start_gate_closed(true);

    assert_eq!(
        handle_stream_task_registration_failure(
            &app,
            &state,
            session_id,
            Some(expected_generation),
            &registration_error,
        )
        .expect("owned registration failure should project"),
        StreamTaskLaunchFailureOutcome::FailedOwnerFinalized
    );

    // guard exposes the atomically retained Failed phase and terminal kernel result.
    // guard 提供原子保留的 Failed 阶段与终态内核结果。
    let guard = state.lock().expect("service state lock");
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
        Some(&SessionRunPhase::Failed)
    );
    assert!(
        !guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies a pre-registration compaction failure closes its exact kernel turn and phase.
/// 验证注册前压缩失败会关闭其精确内核回合与阶段。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if admission cleanup leaves an active kernel turn.
/// 此测试无返回值；若准入清理遗留活动内核回合则失败。
#[test]
fn pre_registration_compaction_failure_finalizes_owned_kernel_turn() {
    // session_id identifies the regular session whose cold projection failed before registration.
    // session_id 标识冷投影在注册前失败的普通会话。
    let session_id = SessionId::new_v4();
    // service_state owns the compaction kernel turn before launch compensation.
    // service_state 在启动补偿前持有压缩内核回合。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_turn(
        &mut service_state,
        session_id,
        "context-compaction-test",
        ActiveTurnKind::ContextCompaction,
    )
    .expect("compaction kernel turn should start");
    // expected_generation binds compensation to the exact pre-registration turn.
    // expected_generation 将补偿绑定到精确的注册前回合。
    let expected_generation =
        webui_session_kernel_generation(&service_state, session_id).expect("kernel generation");
    // state exposes the retained launch transaction to the shared failure finalizer.
    // state 向共享失败终结器提供保留启动事务。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app discards the emitted Failed delta because retained state is authoritative in this test.
    // app 丢弃发送的 Failed 增量，因为本测试以保留状态为权威。
    let app = ServiceEventEmitter::new(Vec::new());

    assert_eq!(
        handle_pre_registration_stream_task_launch_failure(
            &app,
            &state,
            session_id,
            Some(expected_generation),
        )
        .expect("owned pre-registration failure should project"),
        StreamTaskLaunchFailureOutcome::FailedOwnerFinalized
    );

    // guard exposes the atomically retained Failed phase and terminal kernel result.
    // guard 提供原子保留的 Failed 阶段与终态内核结果。
    let guard = state.lock().expect("service state lock");
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
        Some(&SessionRunPhase::Failed)
    );
    assert!(!guard.active_stream_tasks.contains_key(&session_id));
    assert!(
        !guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies an exactly owned Prepared failure removes its task, closes the kernel, and projects Failed.
/// 验证精确归属的 Prepared 失败会移除任务、关闭 kernel 并投影 Failed。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if any terminal Prepared state remains incomplete.
/// 此测试无返回值；任一 Prepared 终态不完整时测试失败。
#[tokio::test]
async fn prepared_failure_terminalizes_exact_active_stream_owner() {
    // session_id identifies the regular session whose Prepared commit failed.
    // session_id 标识 Prepared 提交失败的常规会话。
    let session_id = SessionId::new_v4();
    // active_stream_task_id identifies the exact failing stream generation.
    // active_stream_task_id 标识失败的精确流代次。
    let active_stream_task_id = Uuid::new_v4();
    // service_state owns the active kernel, stream registry, and retained phase before failure.
    // service_state 在失败前持有活动 kernel、流注册表与保留阶段。
    let mut service_state = test_service_state_with_parent_session(session_id);
    insert_test_workbench_conversation(&mut service_state, session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "prepared-failure")
        .expect("kernel turn should start");
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(active_stream_task_id, spawn_service_task(async {}), None),
    );
    // state exposes the exact Prepared failure transaction boundary.
    // state 提供精确 Prepared 失败事务边界。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app discards emitted deltas because retained state is authoritative in this test.
    // app 丢弃发送的增量，因为本测试以保留状态为权威。
    let app = ServiceEventEmitter::new(Vec::new());

    assert_eq!(
        commit_prepared_stream_failure(
            &app,
            &state,
            session_id,
            active_stream_task_id,
            "test Prepared failure",
        ),
        PreparedStreamFailureCommit::Committed
    );

    // guard exposes the atomically retained terminal ownership state.
    // guard 提供原子保留的终止所有权状态。
    let guard = state.lock().expect("service state lock");
    assert!(!guard.active_stream_tasks.contains_key(&session_id));
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
        Some(&SessionRunPhase::Failed)
    );
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace
                .sessions
                .iter()
                .find(|session| session.id == session_id))
            .map(|session| session.status),
        Some(SessionState::Blocked)
    );
    assert_eq!(
        guard
            .workbench
            .conversations
            .iter()
            .find(|conversation| conversation.id == session_id.to_string())
            .map(|conversation| conversation.status.as_str()),
        Some(SessionState::Blocked.label())
    );
    assert!(
        !guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies every run phase updates workspace, Workbench, and delta status from one shared mapping.
/// 验证每个运行阶段都通过同一共享映射更新工作区、Workbench 与增量状态。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails when any phase projection surface diverges.
/// 此测试无返回值；任一阶段投影界面不一致时测试失败。
#[test]
fn session_phase_projection_keeps_all_status_surfaces_aligned() {
    // session_id identifies the conversation updated across the complete phase matrix.
    // session_id 标识在完整阶段矩阵中被更新的会话。
    let session_id = SessionId::new_v4();
    // state owns the workspace and Workbench surfaces under direct test.
    // state 持有被直接测试的工作区与 Workbench 界面。
    let mut state = test_service_state_with_parent_session(session_id);
    insert_test_workbench_conversation(&mut state, session_id);
    // phase_cases encode the complete authoritative mapping contract.
    // phase_cases 编码完整的权威映射契约。
    let phase_cases = [
        (SessionRunPhase::Idle, SessionState::Completed),
        (SessionRunPhase::Ready, SessionState::Completed),
        (SessionRunPhase::Preparing, SessionState::Running),
        (SessionRunPhase::MemoryRecall, SessionState::Running),
        (SessionRunPhase::Requesting, SessionState::Running),
        (SessionRunPhase::Thinking, SessionState::Running),
        (SessionRunPhase::ToolRunning, SessionState::Running),
        (SessionRunPhase::Failed, SessionState::Blocked),
        (SessionRunPhase::Stopped, SessionState::Blocked),
    ];

    for (phase, expected_state) in phase_cases {
        // phase_event exposes the client-facing status produced by the retained mutation.
        // phase_event 暴露保留状态变更生成的客户端状态。
        let phase_event = apply_session_phase_to_state(&mut state, session_id, phase)
            .expect("phase projection should succeed");
        // workspace proves the durable session summary and run phase remain aligned.
        // workspace 证明可持久化会话摘要与运行阶段保持一致。
        let workspace = state
            .workspace_snapshot
            .as_ref()
            .expect("workspace should remain available");
        assert_eq!(workspace.session_run_phases.get(&session_id), Some(&phase));
        assert_eq!(
            workspace
                .sessions
                .iter()
                .find(|session| session.id == session_id)
                .map(|session| session.status),
            Some(expected_state)
        );
        // conversation proves the retained Workbench row uses the same status label.
        // conversation 证明保留 Workbench 行使用相同状态标签。
        let conversation = state
            .workbench
            .conversations
            .iter()
            .find(|conversation| conversation.id == session_id.to_string())
            .expect("conversation should remain available");
        assert_eq!(conversation.run_phase, phase.to_string());
        assert_eq!(conversation.status, expected_state.label());
        // delta proves connected clients receive the same explicit status rather than preserving stale state.
        // delta 证明已连接客户端收到相同显式状态，而非保留过期状态。
        let WorkbenchDelta::UpdateConversationRunPhase(delta) = phase_event.delta else {
            panic!("phase projection should produce a run-phase delta");
        };
        assert_eq!(delta.run_phase, phase.to_string());
        assert_eq!(delta.status.as_deref(), Some(expected_state.label()));
    }
}

/// Verifies the exact active stream owner can commit one Requesting phase transition.
/// 验证精确活动流所有者可以提交一次 Requesting 阶段转换。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if exact ownership does not commit aligned phase state.
/// 此测试无返回值；若精确所有权未提交对齐阶段状态则失败。
#[tokio::test]
async fn active_stream_phase_commit_updates_exact_owner_state() {
    // session_id identifies the exact active stream session.
    // session_id 标识精确活动流会话。
    let session_id = SessionId::new_v4();
    // active_task_id identifies the task allowed to commit Requesting.
    // active_task_id 标识获准提交 Requesting 的任务。
    let active_task_id = Uuid::new_v4();
    // service_state owns matching workspace, Workbench, and active task fixtures.
    // service_state 持有匹配的工作区、Workbench 与活动任务夹具。
    let mut service_state = test_service_state_with_parent_session(session_id);
    insert_test_workbench_conversation(&mut service_state, session_id);
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(active_task_id, spawn_service_task(async {}), None),
    );
    // state exposes the exact phase ownership transaction.
    // state 暴露精确阶段所有权事务。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    assert_eq!(
        apply_active_stream_session_phase(
            &ServiceEventEmitter::new(Vec::new()),
            &state,
            session_id,
            active_task_id,
            SessionRunPhase::Requesting,
        )
        .expect("exact owner phase commit should succeed"),
        ActiveStreamPhaseCommit::Committed
    );

    // guard proves all retained phase surfaces changed while task ownership remained intact.
    // guard 证明全部保留阶段界面已变更，同时任务所有权保持完整。
    let guard = state.lock().expect("service state lock");
    assert!(active_stream_task_matches_id(
        &guard,
        session_id,
        active_task_id
    ));
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
        Some(&SessionRunPhase::Requesting)
    );
    assert_eq!(guard.workbench_generation, 1);
}

/// Verifies a superseded stream cannot overwrite its replacement's retained phase state.
/// 验证已被替代的流无法覆盖替代者的保留阶段状态。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if stale phase handling changes replacement state.
/// 此测试无返回值；若过期阶段处理修改替代状态则失败。
#[tokio::test]
async fn active_stream_phase_commit_preserves_replacement_owner() {
    // session_id identifies the shared session slot.
    // session_id 标识共享会话槽位。
    let session_id = SessionId::new_v4();
    // stale_task_id identifies the obsolete request-phase event owner.
    // stale_task_id 标识过期请求阶段事件所有者。
    let stale_task_id = Uuid::new_v4();
    // replacement_task_id identifies the authoritative replacement stream.
    // replacement_task_id 标识权威替代流。
    let replacement_task_id = Uuid::new_v4();
    // service_state owns the replacement task and Ready baseline.
    // service_state 持有替代任务与 Ready 基线。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(replacement_task_id, spawn_service_task(async {}), None),
    );
    // original_generation proves stale handling creates no Workbench revision.
    // original_generation 证明过期处理不会创建 Workbench 修订。
    let original_generation = service_state.workbench_generation;
    // state exposes the replacement ownership boundary.
    // state 暴露替代所有权边界。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    assert_eq!(
        apply_active_stream_session_phase(
            &ServiceEventEmitter::new(Vec::new()),
            &state,
            session_id,
            stale_task_id,
            SessionRunPhase::Requesting,
        )
        .expect("stale phase commit should classify ownership"),
        ActiveStreamPhaseCommit::Superseded
    );

    // guard proves the replacement task and Ready phase remain authoritative.
    // guard 证明替代任务与 Ready 阶段保持权威。
    let guard = state.lock().expect("service state lock");
    assert!(active_stream_task_matches_id(
        &guard,
        session_id,
        replacement_task_id
    ));
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
        Some(&SessionRunPhase::Ready)
    );
    assert_eq!(guard.workbench_generation, original_generation);
}

/// Verifies a superseded task cannot enter the execution-preparation mutation closure.
/// 验证已被替代的任务无法进入执行准备变更闭包。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if stale ownership changes retained preparation state.
/// 此测试无返回值；若过期所有权修改保留准备状态则失败。
#[tokio::test]
async fn execution_preparation_commit_rejects_superseded_task_before_mutation() {
    // session_id identifies the session slot now owned by a replacement stream.
    // session_id 标识当前由替代流拥有的会话槽位。
    let session_id = SessionId::new_v4();
    // stale_task_id identifies the obsolete Prepared event attempting to commit.
    // stale_task_id 标识尝试提交的过期 Prepared 事件。
    let stale_task_id = Uuid::new_v4();
    // replacement_task_id identifies the authoritative current stream.
    // replacement_task_id 标识权威当前流。
    let replacement_task_id = Uuid::new_v4();
    // service_state owns the replacement task and baseline revision before the stale commit.
    // service_state 在过期提交前持有替代任务与基线修订。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(replacement_task_id, spawn_service_task(async {}), None),
    );
    // original_generation proves the rejected closure never mutated Workbench state.
    // original_generation 证明被拒绝闭包从未修改 Workbench 状态。
    let original_generation = service_state.workbench_generation;
    // state exposes exact task ownership to the preparation transaction helper.
    // state 向准备事务助手暴露精确任务所有权。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    // error is the explicit ownership rejection returned before the mutation closure executes.
    // error 是在变更闭包执行前返回的显式所有权拒绝。
    let error = commit_owned_execution_preparation_state(
        &state,
        session_id,
        stale_task_id,
        |service_state| {
            service_state.workbench_generation = service_state.workbench_generation.wrapping_add(1);
            Ok(())
        },
    )
    .expect_err("superseded preparation owner should be rejected");
    assert!(error.contains("ownership was superseded"));

    // guard proves the replacement entry and every retained preparation surface remain unchanged.
    // guard 证明替代条目与全部保留准备界面保持不变。
    let guard = state.lock().expect("service state lock");
    assert!(active_stream_task_matches_id(
        &guard,
        session_id,
        replacement_task_id
    ));
    assert_eq!(guard.workbench_generation, original_generation);
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
        Some(&SessionRunPhase::Ready)
    );
    assert!(guard.live_context_budgets.is_empty());
}

/// Verifies a superseded root task cannot enter the runtime-event projection mutation closure.
/// 验证已被替换的根任务无法进入运行时事件投影变更闭包。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if stale ownership changes retained projection state.
/// 此测试无返回值；若过期所有权修改保留投影状态则失败。
#[tokio::test]
async fn runtime_event_projection_commit_rejects_superseded_task_before_mutation() {
    // session_id identifies the session whose root stream has been replaced.
    // session_id 标识根流已被替换的会话。
    let session_id = SessionId::new_v4();
    // stale_task_id identifies the obsolete runtime-event producer.
    // stale_task_id 标识已过期的运行时事件生产者。
    let stale_task_id = Uuid::new_v4();
    // replacement_task_id identifies the only task currently authorized to project events.
    // replacement_task_id 标识当前唯一获准投影事件的任务。
    let replacement_task_id = Uuid::new_v4();
    // service_state retains the replacement task and a baseline Workbench generation.
    // service_state 保留替代任务与工作台基线代次。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(replacement_task_id, spawn_service_task(async {}), None),
    );
    // original_generation proves the rejected projection closure never ran.
    // original_generation 证明被拒绝的投影闭包从未运行。
    let original_generation = service_state.workbench_generation;
    // state exposes the exact root-task registry to the projection transaction.
    // state 向投影事务暴露精确根任务注册表。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    // error records the ownership rejection returned before any retained mutation.
    // error 记录在任何保留状态变更前返回的所有权拒绝。
    let error = commit_runtime_event_projection_state(
        &state,
        session_id,
        RuntimeEventProjectionAuthority::ActiveStreamTask(stale_task_id),
        |service_state| {
            service_state.workbench_generation = service_state.workbench_generation.wrapping_add(1);
            Ok(())
        },
    )
    .expect_err("superseded runtime-event projection owner should be rejected");
    assert!(error.contains("runtime event projection ownership was superseded"));

    // guard proves the replacement task and retained generation remain authoritative.
    // guard 证明替代任务与保留代次仍保持权威。
    let guard = state.lock().expect("service state lock");
    assert!(active_stream_task_matches_id(
        &guard,
        session_id,
        replacement_task_id
    ));
    assert_eq!(guard.workbench_generation, original_generation);
}

/// Verifies missing workspace state rejects execution preparation before its mutation closure runs.
/// 验证缺失工作区状态会在执行准备变更闭包运行前拒绝提交。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if workspace preflight permits partial preparation state.
/// 此测试无返回值；若工作区预检允许局部准备状态则失败。
#[tokio::test]
async fn execution_preparation_commit_rejects_missing_workspace_before_mutation() {
    // session_id identifies the exact preparation owner lacking retained workspace state.
    // session_id 标识缺失保留工作区状态的精确准备所有者。
    let session_id = SessionId::new_v4();
    // active_task_id identifies the authoritative current stream.
    // active_task_id 标识权威当前流。
    let active_task_id = Uuid::new_v4();
    // service_state retains ownership but deliberately removes only the workspace snapshot.
    // service_state 保留所有权，但刻意只移除工作区快照。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(active_task_id, spawn_service_task(async {}), None),
    );
    service_state.workspace_snapshot = None;
    // original_generation proves the mutation closure cannot change Workbench state.
    // original_generation 证明变更闭包无法修改 Workbench 状态。
    let original_generation = service_state.workbench_generation;
    // state exposes the workspace preflight boundary.
    // state 暴露工作区预检边界。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    // error is the explicit workspace rejection returned before mutation.
    // error 是变更前返回的显式工作区拒绝。
    let error = commit_owned_execution_preparation_state(
        &state,
        session_id,
        active_task_id,
        |service_state| {
            service_state.workbench_generation = service_state.workbench_generation.wrapping_add(1);
            Ok(())
        },
    )
    .expect_err("missing workspace should reject preparation commit");
    assert!(error.contains("workspace snapshot is not ready"));

    // guard proves ownership remains available for the caller's explicit failure cleanup.
    // guard 证明所有权仍可供调用方执行显式失败清理。
    let mut guard = state.lock().expect("service state lock");
    assert!(active_stream_task_matches_id(
        &guard,
        session_id,
        active_task_id
    ));
    assert_eq!(guard.workbench_generation, original_generation);
    assert!(guard.live_context_budgets.is_empty());
    assert!(remove_active_stream_task_if_id(&mut guard, session_id, active_task_id).is_some());
}

/// Verifies a stale Prepared failure cannot terminalize a replacement stream in the same session.
/// 验证过期 Prepared 失败无法终结同一会话中的替代流。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if stale ownership changes replacement state.
/// 此测试无返回值；过期所有权修改替代状态时测试失败。
#[tokio::test]
async fn prepared_failure_preserves_replacement_stream_owner() {
    // session_id identifies the session whose stream owner was replaced.
    // session_id 标识流所有者已被替换的会话。
    let session_id = SessionId::new_v4();
    // stale_stream_task_id identifies the obsolete failing stream generation.
    // stale_stream_task_id 标识过期的失败流代次。
    let stale_stream_task_id = Uuid::new_v4();
    // replacement_stream_task_id identifies the authoritative replacement generation.
    // replacement_stream_task_id 标识权威替代流代次。
    let replacement_stream_task_id = Uuid::new_v4();
    // service_state owns the replacement kernel and stream registry before stale failure handling.
    // service_state 在处理过期失败前持有替代 kernel 与流注册表。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "replacement-turn")
        .expect("replacement kernel turn should start");
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(
            replacement_stream_task_id,
            spawn_service_task(async {}),
            None,
        ),
    );
    // state exposes the replacement ownership boundary to stale failure handling.
    // state 向过期失败处理提供替代所有权边界。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app discards emitted deltas; no delta should be produced for superseded failure.
    // app 丢弃发送的增量；已被替代的失败不应产生增量。
    let app = ServiceEventEmitter::new(Vec::new());

    assert_eq!(
        commit_prepared_stream_failure(
            &app,
            &state,
            session_id,
            stale_stream_task_id,
            "stale Prepared failure",
        ),
        PreparedStreamFailureCommit::Superseded
    );

    // guard proves the replacement task, Ready phase, and active kernel remain authoritative.
    // guard 证明替代任务、Ready 阶段与活动 kernel 仍保持权威。
    let mut guard = state.lock().expect("service state lock");
    assert!(guard.active_stream_tasks.contains_key(&session_id));
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
        Some(&SessionRunPhase::Ready)
    );
    assert!(
        guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
    assert!(
        remove_active_stream_task_if_id(&mut guard, session_id, replacement_stream_task_id)
            .is_some()
    );
}

/// Verifies missing retained phase state is reported explicitly while the exact ending owner is still cleaned.
/// 验证缺失保留阶段状态会被显式报告，同时仍清理精确的结束中所有者。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if unavailable state is misclassified or leaks task ownership.
/// 此测试无返回值；若不可用状态被错误分类或泄漏任务所有权则失败。
#[tokio::test]
async fn prepared_failure_classifies_missing_workspace_as_state_unavailable() {
    // session_id identifies the exact Prepared stream whose retained workspace disappears.
    // session_id 标识保留工作区消失的精确 Prepared 流。
    let session_id = SessionId::new_v4();
    // active_stream_task_id identifies the ending stream owner that still requires cleanup.
    // active_stream_task_id 标识仍需清理的结束中流所有者。
    let active_stream_task_id = Uuid::new_v4();
    // service_state starts from a valid active turn before the test removes only retained workspace state.
    // service_state 从有效活动回合开始，随后测试仅移除保留工作区状态。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "missing-workspace")
        .expect("kernel turn should start");
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(active_stream_task_id, spawn_service_task(async {}), None),
    );
    service_state.workspace_snapshot = None;
    // original_generation proves no Failed phase revision can be generated without retained workspace state.
    // original_generation 证明缺失保留工作区时不会生成 Failed 阶段修订。
    let original_generation = service_state.workbench_generation;
    // state exposes the unavailable retained-state boundary to Prepared failure handling.
    // state 向 Prepared 失败处理暴露不可用的保留状态边界。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app has no sinks because unavailable phase state must not produce a phase delta.
    // app 不包含 sink，因为不可用阶段状态不得产生阶段增量。
    let app = ServiceEventEmitter::new(Vec::new());

    assert_eq!(
        commit_prepared_stream_failure(
            &app,
            &state,
            session_id,
            active_stream_task_id,
            "missing workspace Prepared failure",
        ),
        PreparedStreamFailureCommit::StateUnavailable
    );

    // guard proves cleanup completed without fabricating a retained phase revision.
    // guard 证明清理已完成，且未伪造保留阶段修订。
    let guard = state.lock().expect("service state lock");
    assert!(!guard.active_stream_tasks.contains_key(&session_id));
    assert_eq!(guard.workbench_generation, original_generation);
    assert!(
        !guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies a domain commit error still closes an exactly owned failed registration kernel turn.
/// 验证领域提交错误仍会关闭精确归属的注册失败内核回合。
#[test]
fn owned_registration_domain_error_does_not_leak_kernel_turn() {
    // session_id identifies the exact failed registration owner.
    // session_id 标识精确的注册失败所有者。
    let session_id = SessionId::new_v4();
    // service_state owns the active kernel turn before the injected domain error.
    // service_state 在注入领域错误前持有活动内核回合。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_turn(
        &mut service_state,
        session_id,
        "context-compaction-domain-error",
        ActiveTurnKind::ContextCompaction,
    )
    .expect("compaction kernel turn should start");
    // expected_generation binds cleanup to the exact active turn.
    // expected_generation 将清理绑定到精确活动回合。
    let expected_generation =
        webui_session_kernel_generation(&service_state, session_id).expect("kernel generation");
    // state exposes retained ownership to the generic domain commit boundary.
    // state 向通用领域提交边界提供保留所有权。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // registration_error proves exact start-gate cleanup ownership.
    // registration_error 证明精确启动闸门清理所有权。
    let registration_error = ActiveStreamTaskRegistrationError::start_gate_closed(true);

    // error is the injected domain failure propagated after kernel terminalization.
    // error 是在内核终结后传播的注入领域失败。
    let error = match commit_stream_task_registration_failure(
        &state,
        session_id,
        Some(expected_generation),
        &registration_error,
        |_| Err::<(), String>(String::from("domain failure commit rejected")),
        |_, ()| Ok(()),
    ) {
        Ok(_) => panic!("domain commit should fail"),
        Err(error) => error,
    };
    assert_eq!(error, "domain failure commit rejected");

    // guard proves the exactly owned kernel was closed despite the domain error.
    // guard 证明尽管领域提交失败，精确归属的内核仍已关闭。
    let guard = state.lock().expect("service state lock");
    assert!(
        !guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies a replacement stream blocks stale registration failure terminalization in the same kernel generation.
/// 验证替代流会阻止同一内核代际中的过期注册失败终结。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if stale failure handling mutates replacement state.
/// 此测试无返回值；若过期失败处理修改替代状态则失败。
#[tokio::test]
async fn replacement_stream_blocks_stale_registration_failure_terminalization() {
    // session_id identifies the shared slot reused by the replacement stream.
    // session_id 标识替代流复用的共享槽位。
    let session_id = SessionId::new_v4();
    // replacement_task_id identifies the authoritative task installed after failed cleanup.
    // replacement_task_id 标识失败清理后安装的权威任务。
    let replacement_task_id = Uuid::new_v4();
    // service_state owns the active kernel turn and replacement task entry.
    // service_state 持有活动内核回合与替代任务条目。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-a")
        .expect("kernel turn should start");
    // expected_generation captures the generation shared by the failed attempt and replacement.
    // expected_generation 捕获失败尝试与替代任务共享的代际。
    let expected_generation =
        webui_session_kernel_generation(&service_state, session_id).expect("kernel generation");
    service_state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(
            replacement_task_id,
            spawn_service_task(async {}),
            Some(String::from("goal-new")),
        ),
    );
    // state exposes replacement-owned retained state to stale failure handling.
    // state 向过期失败处理提供由替代任务拥有的保留状态。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app would emit only if stale handling incorrectly passed ownership checks.
    // app 仅会在过期处理错误通过所有权检查时发送增量。
    let app = ServiceEventEmitter::new(Vec::new());
    // registration_error records that the earlier failed entry was removed before replacement installation.
    // registration_error 记录更早的失败条目已在替代任务安装前移除。
    let registration_error = ActiveStreamTaskRegistrationError::start_gate_closed(true);

    assert_eq!(
        handle_stream_task_registration_failure(
            &app,
            &state,
            session_id,
            Some(expected_generation),
            &registration_error,
        )
        .expect("stale registration failure should be rejected cleanly"),
        StreamTaskLaunchFailureOutcome::StaleOwner
    );

    // guard exposes the replacement state that stale failure handling must preserve.
    // guard 提供过期失败处理必须保留的替代状态。
    let guard = state.lock().expect("service state lock");
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
        Some(&SessionRunPhase::Ready)
    );
    assert_eq!(
        guard
            .active_stream_tasks
            .get(&session_id)
            .and_then(ActiveStreamTask::session_goal_id),
        Some("goal-new")
    );
    assert!(
        guard
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies a registered stream task cannot execute before its registry entry is visible.
/// 验证已注册流任务无法在其注册表条目可见之前执行。
#[tokio::test]
async fn active_stream_task_start_gate_registers_before_execution() {
    let session_id = SessionId::new_v4();
    let task_id = Uuid::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    let state_for_task = state.clone();
    let (observed_sender, observed_receiver) = tokio::sync::oneshot::channel::<bool>();

    spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        task_id,
        Some(String::from("goal-gated")),
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        async move {
            let registered = state_for_task.lock().is_ok_and(|guard| {
                guard
                    .active_stream_tasks
                    .get(&session_id)
                    .and_then(ActiveStreamTask::session_goal_id)
                    == Some("goal-gated")
            });
            let _ = observed_sender.send(registered);
        },
    )
    .expect("gated task should register");

    assert!(observed_receiver.await.expect("task observation"));
    let mut guard = state.lock().expect("service state lock");
    assert!(remove_active_stream_task_if_id(&mut guard, session_id, task_id).is_some());
}

/// Verifies ordinary registration rejects a second owner without aborting the first task.
/// 验证普通注册会拒绝第二个所有者，且不会中止第一个任务。
#[tokio::test]
async fn active_stream_task_rejects_second_owner_without_replacement() {
    // session_id identifies the single-owner session slot under test.
    // session_id 标识被测的单所有者会话槽位。
    let session_id = SessionId::new_v4();
    // first_task_id remains the owner after the rejected registration attempt.
    // first_task_id 在第二次注册被拒绝后仍然是所有者。
    let first_task_id = Uuid::new_v4();
    let second_task_id = Uuid::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));

    spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        first_task_id,
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        async {
            std::future::pending::<()>().await;
        },
    )
    .expect("first owner should register");

    let error = spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        second_task_id,
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        async {},
    )
    .expect_err("second ordinary owner should be rejected");
    assert!(matches!(
        error,
        ActiveStreamTaskRegistrationError::Occupied {
            existing_task_id
        } if existing_task_id == first_task_id
    ));

    let mut guard = state.lock().expect("service state lock");
    assert_eq!(
        guard
            .active_stream_tasks
            .get(&session_id)
            .map(ActiveStreamTask::task_id),
        Some(first_task_id)
    );
    assert!(remove_active_stream_task_if_id(&mut guard, session_id, first_task_id).is_some());
}

/// Verifies a stopped kernel suppresses stale legacy stream handles for active-run decisions.
/// 验证已停止内核会在活动运行判断中抑制过期旧流任务句柄。
#[tokio::test]
async fn webui_session_stopped_kernel_suppresses_stale_stream_handle() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);

    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("kernel turn should start");
    state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(Uuid::new_v4(), spawn_service_task(async {}), None),
    );
    stop_webui_session_kernel_generation(&mut state, session_id);

    assert!(state.active_stream_tasks.contains_key(&session_id));
    assert!(!webui_session_has_active_run(&state, session_id));
}

/// Verifies legacy stream handles alone cannot become the active-run fact source.
/// 验证旧流任务句柄本身不能成为活动运行事实源。
#[tokio::test]
async fn webui_session_active_run_ignores_stream_handle_without_kernel_turn() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);

    state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(Uuid::new_v4(), spawn_service_task(async {}), None),
    );

    assert!(state.active_stream_tasks.contains_key(&session_id));
    assert!(!webui_session_has_active_run(&state, session_id));
}

/// Verifies finishing a WebUI session kernel turn clears the active turn.
/// 验证结束 WebUI 会话内核回合会清理活动回合。
#[test]
fn webui_session_kernel_finish_clears_active_turn() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);

    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("kernel turn should start");
    finish_webui_session_kernel_turn(&mut state, session_id, TurnTerminalStatus::Completed);

    assert!(
        !state
            .session_runtime_kernels
            .get(&session_id)
            .expect("kernel should exist")
            .has_active_turn()
    );
}

/// Verifies lifecycle terminal status parsing accepts only explicit known statuses.
/// 验证生命周期终态状态解析只接受显式已知状态。
#[test]
fn session_lifecycle_terminal_status_requires_explicit_known_status() {
    let completed_event = test_terminal_boundary_event(serde_json::json!({
        "status": "completed"
    }));
    let failed_event = test_terminal_boundary_event(serde_json::json!({
        "status": "failed"
    }));
    let missing_status_event = test_terminal_boundary_event(serde_json::json!({}));
    let unknown_status_event = test_terminal_boundary_event(serde_json::json!({
        "status": "stopped"
    }));

    assert_eq!(
        session_lifecycle_terminal_status_from_event(&completed_event)
            .expect("completed status should parse"),
        SessionTurnTerminalStatus::Completed
    );
    assert_eq!(
        session_lifecycle_terminal_status_from_event(&failed_event)
            .expect("failed status should parse"),
        SessionTurnTerminalStatus::Failed
    );
    assert!(
        session_lifecycle_terminal_status_from_event(&missing_status_event)
            .expect_err("missing status must not default to completed")
            .contains("missing payload.status")
    );
    assert!(
        session_lifecycle_terminal_status_from_event(&unknown_status_event)
            .expect_err("unknown status must not default to failed or completed")
            .contains("unknown payload.status")
    );
}

/// Builds a turn-completed session boundary event for lifecycle parsing tests.
/// 为生命周期解析测试构建回合完成会话边界事件。
///
/// # Parameters
/// 参数
/// `payload` is attached as the terminal event payload.
/// `payload` 会作为终态事件载荷写入。
///
/// # Returns
/// 返回值
/// A terminal boundary event with a stable test turn id.
/// 返回带稳定测试轮次标识的终态边界事件。
fn test_terminal_boundary_event(payload: serde_json::Value) -> SessionBoundaryEvent {
    SessionBoundaryEvent::new(
        "terminal-test",
        SessionNodeId::from(SessionId::new_v4()),
        1,
        SessionBoundaryEventKind::TurnCompleted,
        SessionEventScope::Turn,
        SessionEventVisibility::Hidden,
        10,
    )
    .with_turn_id("turn-a")
    .with_payload(payload)
}

/// Verifies stopping a WebUI session kernel generation clears the active turn and marks stopped.
/// 验证停止 WebUI 会话内核代际会清理活动回合并标记停止。
#[test]
fn webui_session_kernel_stop_clears_active_turn_and_marks_stopped() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);

    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("kernel turn should start");
    stop_webui_session_kernel_generation(&mut state, session_id);

    let kernel = state
        .session_runtime_kernels
        .get(&session_id)
        .expect("kernel should exist");
    assert!(!kernel.has_active_turn());
    assert!(kernel.is_stopped());
}

/// Verifies stream events are accepted only while their captured kernel generation is current.
/// 验证流事件仅在捕获的内核代际仍为当前代际时被接受。
#[test]
fn webui_stream_event_generation_gate_rejects_stopped_generation() {
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-a")
        .expect("kernel turn should start");
    let expected_generation =
        webui_session_kernel_generation(&service_state, session_id).expect("generation");
    let state = Arc::new(Mutex::new(service_state));
    let event = ExecutionStreamEvent::RequestStarted { session_id };

    assert!(webui_stream_event_matches_kernel_generation(
        &state,
        &event,
        Some(expected_generation)
    ));
    {
        let mut guard = state.lock().expect("state lock");
        stop_webui_session_kernel_generation(&mut guard, session_id);
    }

    assert!(!webui_stream_event_matches_kernel_generation(
        &state,
        &event,
        Some(expected_generation)
    ));
}

/// Builds one minimal subagent-completed metadata payload for kernel restore tests.
/// 为内核恢复测试构建一个最小子代理完成元数据载荷。
fn restored_kernel_subagent_metadata(result: &str) -> serde_json::Value {
    serde_json::json!({
        "payload": {
            "execution_id": format!("restored-kernel-{result}"),
            "subagent_mailbox_event": {
                "kind": "completed",
                "payload": {
                    "task_name": "agent-review",
                    "result_text": result
                }
            }
        }
    })
}

/// Restores a service state from the current workspace snapshot but without live kernels.
/// 从当前工作区快照恢复服务状态，但不携带实时内核。
fn service_state_restored_from_workspace_snapshot(
    source: &ServiceBridgeState,
    session_id: SessionId,
) -> ServiceBridgeState {
    let mut restored = test_service_state_with_parent_session(session_id);
    restored.workspace_snapshot = source.workspace_snapshot.clone();
    restored
}

/// Verifies unconsumed persisted mailbox input restores as a triggerable kernel mailbox item.
/// 验证未消费的持久化 mailbox 输入会恢复为可触发的内核 mailbox 项。
#[test]
fn persisted_session_kernel_restores_unconsumed_mailbox_input() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);

    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            task_session_id,
            &restored_kernel_subagent_metadata("done"),
        )
        .is_queued()
    );
    let mut restored = service_state_restored_from_workspace_snapshot(&state, parent_session_id);
    let kernel = session_runtime_kernel_mut(&mut restored, parent_session_id);

    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert_eq!(kernel.input_queue().unconsumed_mailbox_len(), 1);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
}

/// Verifies an existing live kernel is not replaced by a persisted workspace kernel.
/// 验证已有实时内核不会被工作区中的持久化内核替换。
#[test]
fn session_runtime_kernel_mut_preserves_existing_live_kernel() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut persisted_source = test_service_state_with_parent_session(parent_session_id);

    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut persisted_source,
            parent_session_id,
            task_session_id,
            &restored_kernel_subagent_metadata("stale"),
        )
        .is_queued()
    );

    let mut state =
        service_state_restored_from_workspace_snapshot(&persisted_source, parent_session_id);
    state.session_runtime_kernels.insert(
        parent_session_id,
        SessionRuntimeKernel::new(SessionNodeId::from(parent_session_id)),
    );

    let kernel = session_runtime_kernel_mut(&mut state, parent_session_id);

    assert_eq!(kernel.input_queue().mailbox_len(), 0);
}

/// Verifies consumed persisted mailbox input does not trigger another restored parent turn.
/// 验证已消费的持久化 mailbox 输入不会再次触发恢复后的父级回合。
#[test]
fn persisted_session_kernel_restores_consumed_mailbox_cursor() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let input_id =
        format!("subagent-mailbox-{parent_session_id}-{task_session_id}-restored-kernel-waited");

    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            task_session_id,
            &restored_kernel_subagent_metadata("waited"),
        )
        .is_queued()
    );
    {
        let kernel = session_runtime_kernel_mut(&mut state, parent_session_id);
        kernel
            .register_wait_scope("wait-agent-1", [task_session_id.to_string()], 10)
            .expect("wait scope should register");
        assert!(kernel.consume_mailbox_input_for_wait_scope("wait-agent-1", &input_id));
    }
    sync_webui_session_runtime_kernel_to_workspace_state(&mut state, parent_session_id);

    let mut restored = service_state_restored_from_workspace_snapshot(&state, parent_session_id);
    let kernel = session_runtime_kernel_mut(&mut restored, parent_session_id);

    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert_eq!(kernel.input_queue().unconsumed_mailbox_len(), 0);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
}
