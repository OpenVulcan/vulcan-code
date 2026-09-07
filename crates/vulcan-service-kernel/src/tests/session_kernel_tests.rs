//! session kernel tests.
//! session kernel tests。

use super::*;
use crate::pending_prompts::dispatch_queued_pending_prompt_hook;
use crate::session_goal_lifecycle::session_goal_dispatch_authority_from_v1_handle;
use crate::session_runtime_registry_v1::{SessionHandle, SessionPersistenceBackend};
use crate::webui_prompt_lifecycle::commit_prompt_preparation_failure_boundary;
use vulcan_session_core::v1::document::{FlushCandidate, SessionDocument};
use vulcan_workspace_store::workspace_store::{SessionV1FlushResult, SessionV1StoreError};

/// Read-only persistence seam for restart-derived Goal dispatch authority tests.
/// 用于重启派生 Goal 分发权威测试的只读持久化接缝。
struct RestartGoalReadOnlyBackend;

impl SessionPersistenceBackend for RestartGoalReadOnlyBackend {
    /// Rejects writes because loaded Goal admission must remain read-only until Provider start.
    /// 拒绝写入，因为已加载 Goal 的准入在 Provider 启动前必须保持只读。
    fn flush(
        &self,
        _candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        panic!("restart Goal dispatch authority must not flush before Provider start")
    }
}

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

/// Verifies Goal mutation and queued dispatch reservations cannot overlap kernel registration.
/// 验证 Goal 变更预留与排队分发预留不能重叠进行 kernel 注册。
#[test]
fn goal_dispatch_registration_requires_the_current_prompt_reservation() {
    // session_id identifies the Goal session whose dispatch is being admitted.
    // session_id 标识正在准入 Goal 分发的会话。
    let session_id = SessionId::from_u128(0x95);
    // state owns the exclusion reservation and runtime kernel atomically.
    // state 原子持有互斥预留与运行时 kernel。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));
    // transition_guard prevents registration while the durable Goal mutation is incomplete.
    // transition_guard 在持久 Goal 变更尚未完成时阻止回合注册。
    let transition_guard = begin_session_turn_start_exclusion(&state, [session_id], "更新目标状态")
        .expect("Goal mutation should reserve turn registration");
    {
        // guard exposes the shared service state for the registration attempt under test.
        // guard 为测试中的注册尝试提供共享服务状态。
        let mut guard = state.lock().expect("service state lock");
        let blocked = start_reserved_webui_session_kernel_turn(
            &mut guard,
            session_id,
            "turn-during-mutation",
            ActiveTurnKind::GoalContinuation,
            Uuid::new_v4(),
        );
        assert!(blocked.is_err());
        assert!(!webui_session_kernel_has_active_turn(&guard, session_id));
    }
    drop(transition_guard);
    let dispatch_guard = try_begin_pending_prompt_dispatch_exclusion(&state, session_id)
        .expect("Goal dispatch reservation should be readable")
        .expect("idle Goal session should be reserved");
    let authorization = dispatch_guard
        .existing_submission_authorization(session_id)
        .expect("dispatch guard should expose its exact reservation");
    // guard owns atomic reservation validation and kernel registration after mutation completion.
    // guard 在变更完成后持有原子预留校验与 kernel 注册。
    let mut guard = state.lock().expect("service state lock");
    let stale = start_reserved_webui_session_kernel_turn(
        &mut guard,
        session_id,
        "turn-after-mutation",
        ActiveTurnKind::GoalContinuation,
        Uuid::new_v4(),
    );
    assert!(stale.is_err());
    assert!(!webui_session_kernel_has_active_turn(&guard, session_id));
    start_reserved_webui_session_kernel_turn(
        &mut guard,
        session_id,
        "turn-current-authority",
        ActiveTurnKind::GoalContinuation,
        authorization.reservation_id,
    )
    .expect("current Goal dispatch owner should register exactly one turn");
    assert!(webui_session_kernel_has_active_turn(&guard, session_id));
    drop(guard);
    drop(dispatch_guard);
    assert!(
        try_begin_pending_prompt_dispatch_exclusion(&state, session_id)
            .expect("running Goal continuation admission should be readable")
            .is_none(),
        "an already registered Goal continuation must not receive a second PromptSubmission owner"
    );
}

/// A restarted goal stays paused until Play, then grants exactly one concurrent dispatch owner.
/// 重启目标保持暂停直至点击播放，随后只授予一个并发分发所有者。
#[test]
fn restarted_open_goal_continuation_admits_exactly_one_dispatch_owner() {
    let session_id = SessionId::from_u128(0x9610);
    let mut document = SessionDocument::new(session_id.to_string(), "project-a", "Goal", 1);
    let transition = document
        .create_goal(
            "resume after restart",
            Some(10_000),
            "Continue the active goal after restart.",
            2,
        )
        .expect("Goal creation should open one continuation Turn");
    let interrupted_turn_id = transition
        .continuation_turn_id
        .expect("new Goal should own one continuation Turn");
    document
        .project_persisted_interruptions()
        .expect("restart should pause the goal");
    assert_eq!(
        document.metadata.mode.goal.state,
        vulcan_session_core::v1::config::GoalState::Paused
    );
    let resumed = document
        .resume_goal("resume after restart", Some(10_000), "resume", 50_000)
        .expect("Play resumes the goal");
    let expected_turn_id = resumed
        .continuation_turn_id
        .expect("Play opens a continuation");
    assert_ne!(expected_turn_id, interrupted_turn_id);
    let handle =
        SessionHandle::with_backend(session_id, document, Arc::new(RestartGoalReadOnlyBackend));
    let service_state = test_service_state_with_parent_session(session_id);
    service_state
        .session_runtime_registry
        .register(handle.clone())
        .expect("loaded V1 handle should register once");
    let authority = session_goal_dispatch_authority_from_v1_handle(handle.as_ref(), session_id)
        .expect("loaded Goal authority should project")
        .expect("open GoalContinuation should be dispatchable");
    assert_eq!(authority.turn_id, expected_turn_id);
    let state = Arc::new(Mutex::new(service_state));

    let first_state = state.clone();
    let second_state = state.clone();
    let first = std::thread::spawn(move || {
        try_begin_pending_prompt_dispatch_exclusion(&first_state, session_id)
    });
    let second = std::thread::spawn(move || {
        try_begin_pending_prompt_dispatch_exclusion(&second_state, session_id)
    });
    let mut owners = [
        first.join().expect("first admission thread should finish"),
        second
            .join()
            .expect("second admission thread should finish"),
    ]
    .into_iter()
    .filter_map(|result| result.expect("concurrent Goal admission should be readable"))
    .collect::<Vec<_>>();
    assert_eq!(owners.len(), 1, "only one PromptSubmission owner may win");
    let owner = owners.pop().expect("one exact dispatch owner");
    let authorization = owner
        .existing_submission_authorization(session_id)
        .expect("winning owner should expose its exact reservation");
    let turn_label = format!("v1-goal-turn-{}", authority.turn_id.value());
    {
        let mut guard = state.lock().expect("service state lock");
        start_reserved_webui_session_kernel_turn(
            &mut guard,
            session_id,
            turn_label.as_str(),
            ActiveTurnKind::GoalContinuation,
            authorization.reservation_id,
        )
        .expect("winning Goal owner should register the formal turn");
    }
    drop(owner);
    assert!(
        try_begin_pending_prompt_dispatch_exclusion(&state, session_id)
            .expect("registered Goal dispatch should remain readable")
            .is_none(),
        "restart reconciliation must not schedule the same open Turn twice"
    );
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
    seed_test_running_subagent_for_parent(&mut state, session_id);
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
    seed_test_running_subagent_for_parent(&mut state, session_id);
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

    let workspace_path = workspace_path_for_session(
        state
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot should exist"),
        session_id,
    );
    enqueue_webui_pending_prompt_from_submission(
        &mut state,
        &agent_runtime,
        session_id,
        &workspace_path,
        "queued-wait-agent-action",
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
    // workspace_path is the authoritative session path captured by the event descriptor.
    // workspace_path 是事件描述捕获的权威会话路径。
    let workspace_path = workspace_path_for_session(
        service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace fixture should exist"),
        session_id,
    );
    // hook_event describes the queued prompt without mutating Hook runtime.
    // hook_event 描述排队提示词，但不修改 Hook 运行时。
    let hook_event = enqueue_webui_pending_prompt_from_submission(
        &mut service_state,
        &agent_runtime,
        session_id,
        &workspace_path,
        "queued-hook-after-commit-action",
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
    // workspace_path is the exact session path captured with the queued entry.
    // workspace_path 是随排队条目捕获的精确会话路径。
    let workspace_path = workspace_path_for_session(
        service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace fixture should exist"),
        session_id,
    );
    // hook_event describes the queue mutation already present in the workspace.
    // hook_event 描述工作区中已经存在的队列变更。
    let hook_event = enqueue_webui_pending_prompt_from_submission(
        &mut service_state,
        &agent_runtime,
        session_id,
        &workspace_path,
        "queued-hook-failure-action",
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
        service_state
            .pending_prompt_process_mirrors
            .get(&session_id)
            .map(|mirror| mirror.ui_state.pending_prompt_panel_queue.len()),
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
    seed_test_running_subagent_for_parent(&mut state, session_id);
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

/// Verifies terminal-persistence admission remains blocked until the exact owner explicitly releases it.
/// 验证终态持久化准入会持续阻塞，直至精确所有者显式释放。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if an implicit drop opens admission or an exact release does not.
/// 此测试无返回值；若隐式析构开放准入，或精确释放未开放准入，则测试失败。
#[test]
fn terminal_persistence_owner_requires_explicit_release_before_kernel_start() {
    // session_id identifies the exact regular conversation protected by the terminal boundary.
    // session_id 标识受终态边界保护的精确普通会话。
    let session_id = SessionId::new_v4();
    // stream_task_id identifies the only terminal owner permitted to release the marker.
    // stream_task_id 标识唯一获准释放标记的终态所有者。
    let stream_task_id = Uuid::new_v4();
    // terminal_owner is the typed identity shared by registration, adoption, and release.
    // terminal_owner 是登记、接管与释放共同使用的类型化身份。
    let terminal_owner = SessionTerminalPersistenceOwner::StreamTask(stream_task_id);
    // service_state begins idle so the terminal marker is the sole admission blocker.
    // service_state 初始为空闲状态，使终态标记成为唯一准入阻塞项。
    let mut service_state = test_service_state_with_parent_session(session_id);
    register_session_terminal_persistence_owner(&mut service_state, session_id, terminal_owner)
        .expect("terminal owner should register");
    // state exposes the same owner table to the guard and kernel admission gate.
    // state 向守卫与 kernel 准入闸门暴露同一所有者表。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    // unreleased_guard simulates cancellation or an early error after ownership adoption.
    // unreleased_guard 模拟所有权接管后的取消或提前错误。
    let unreleased_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        terminal_owner,
    )
    .expect("registered owner should be adoptable");
    drop(unreleased_guard);
    {
        // guard proves ordinary drop retained the blocker and kernel admission reports the concrete boundary.
        // guard 证明普通析构保留阻塞项，且 kernel 准入报告具体边界。
        let mut guard = state.lock().expect("service state lock");
        let error = start_webui_session_kernel_user_turn(
            &mut guard,
            session_id,
            "blocked-before-persistence",
        )
        .expect_err("unreleased terminal persistence must block kernel start");
        assert!(error.contains("committing its terminal state"));
    }

    // releasing_guard represents a successfully persisted exact terminal boundary.
    // releasing_guard 表示已经可靠持久化的精确终态边界。
    let releasing_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        terminal_owner,
    )
    .expect("retained owner should remain adoptable");
    releasing_guard
        .release()
        .expect("exact owner should release its marker");
    // guard proves the same session can start only after explicit release.
    // guard 证明同一会话只能在显式释放后启动。
    let mut guard = state.lock().expect("service state lock");
    start_webui_session_kernel_user_turn(&mut guard, session_id, "after-persistence")
        .expect("kernel start should succeed after exact release");
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
            &ServiceEventEmitter::new(Vec::new()),
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
            .and_then(|workspace| workspace_session_summary_for_session(workspace, session_id))
            .map(|session| session.status),
        Some(SessionState::Blocked)
    );
    assert_eq!(
        guard.session_terminal_persistence_owners.get(&session_id),
        Some(&SessionTerminalPersistenceOwner::StreamTask(task_id))
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
            &ServiceEventEmitter::new(Vec::new()),
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
    assert_eq!(
        guard.session_terminal_persistence_owners.get(&session_id),
        Some(&SessionTerminalPersistenceOwner::KernelGeneration(
            expected_generation
        ))
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
    assert_eq!(
        guard.session_terminal_persistence_owners.get(&session_id),
        Some(&SessionTerminalPersistenceOwner::StreamTask(
            active_stream_task_id
        ))
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

/// Verifies missing retained phase state still closes the exact owner behind a persistence marker.
/// 验证缺失保留阶段状态时仍会在持久化标记后闭合精确所有者。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if fail-closed cleanup leaks task ownership or omits its marker.
/// 此测试无返回值；若失败关闭清理泄漏任务所有权或遗漏其标记则失败。
#[tokio::test]
async fn prepared_failure_missing_workspace_retains_terminal_persistence_owner() {
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
        PreparedStreamFailureCommit::Committed
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
    assert_eq!(
        guard.session_terminal_persistence_owners.get(&session_id),
        Some(&SessionTerminalPersistenceOwner::StreamTask(
            active_stream_task_id
        ))
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

/// Verifies direct stream registration cannot bypass an exact terminal-persistence owner.
/// 验证直接流登记无法绕过精确终态持久化所有者。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This asynchronous test returns no value and fails if a replacement entry appears before terminal settlement.
/// 此异步测试无返回值；若终态结算前出现替代条目则测试失败。
#[tokio::test]
async fn active_stream_registration_rejects_terminal_persistence_owner() {
    // session_id identifies the stream registry slot protected by terminal settlement.
    // session_id 标识受终态结算保护的流注册表槽位。
    let session_id = SessionId::new_v4();
    // terminal_task_id identifies the completed stream that still owns persistence.
    // terminal_task_id 标识仍拥有持久化权的已完成流。
    let terminal_task_id = Uuid::new_v4();
    // replacement_task_id identifies the unauthorized direct registration attempt.
    // replacement_task_id 标识未获授权的直接登记尝试。
    let replacement_task_id = Uuid::new_v4();
    // terminal_owner is the exact typed boundary expected in the registration error.
    // terminal_owner 是登记错误中预期的精确类型化边界。
    let terminal_owner = SessionTerminalPersistenceOwner::StreamTask(terminal_task_id);
    // service_state contains no active stream, making terminal persistence the sole registry blocker.
    // service_state 不含活动流，使终态持久化成为唯一注册表阻塞项。
    let mut service_state = test_service_state_with_parent_session(session_id);
    register_session_terminal_persistence_owner(&mut service_state, session_id, terminal_owner)
        .expect("terminal owner should register");
    // state exposes the protected registry to the production registration gate.
    // state 向生产登记闸门暴露受保护注册表。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    let error = spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        replacement_task_id,
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        async {},
    )
    .expect_err("terminal persistence must reject replacement registration");
    assert!(matches!(
        error,
        ActiveStreamTaskRegistrationError::TerminalPersistenceInFlight { owner }
            if owner == terminal_owner
    ));
    // guard proves rejection did not mutate either the terminal owner or active registry.
    // guard 证明拒绝未修改终态所有者或活动注册表。
    let guard = state.lock().expect("service state lock");
    assert_eq!(
        guard.session_terminal_persistence_owners.get(&session_id),
        Some(&terminal_owner)
    );
    assert!(!guard.active_stream_tasks.contains_key(&session_id));
}

/// Verifies RunStop wins the pre-registration gap without allowing stale preparation state or a stream owner to reappear.
/// 验证 RunStop 会赢得流登记前窗口，且不允许过期准备状态或流所有者重新出现。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This asynchronous test returns no value and fails if any owner-checked write bypasses the Stop reservation.
/// 此异步测试无返回值；若任一所有者校验写入绕过 Stop 预留则测试失败。
#[tokio::test]
async fn run_stop_reservation_blocks_stale_preparation_commit_and_stream_registration() {
    // sessionId identifies the exact preparation/Stop race under test.
    // sessionId 标识被测的精确准备/Stop 竞态。
    let session_id = SessionId::new_v4();
    // serviceState begins with one accepted kernel turn and a visible Ready projection.
    // serviceState 从一个已接纳 kernel 回合与可见 Ready 投影开始。
    let mut service_state = test_service_state_with_parent_session(session_id);
    insert_test_workbench_conversation(&mut service_state, session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-stop-race")
        .expect("kernel turn should start");
    // expectedGeneration is the immutable generation captured by deferred preparation.
    // expectedGeneration 是延迟准备捕获的不可变代际。
    let expected_generation = webui_session_kernel_generation(&service_state, session_id)
        .expect("kernel generation should exist");
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace should exist")
        .session_run_phases
        .insert(session_id, SessionRunPhase::Ready);
    // staleCandidate simulates the detached workspace produced just before Stop acquired admission.
    // staleCandidate 模拟 Stop 取得准入前刚生成的分离工作区。
    let mut stale_candidate = service_state
        .workspace_snapshot
        .clone()
        .expect("workspace should exist");
    stale_candidate
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    service_state.session_turn_admission_reservations.insert(
        session_id,
        SessionTurnAdmissionReservation {
            reservation_id: Uuid::new_v4(),
            kind: SessionTurnAdmissionReservationKind::RunStop,
        },
    );
    // state exposes the exact gap where Stop owns admission but has not yet stopped the kernel.
    // state 暴露 Stop 已拥有准入但尚未停止 kernel 的精确窗口。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // initialRevision proves rejected stale work does not even advance the visible event clock.
    // initialRevision 证明被拒绝的过期工作甚至不会推进可见事件时钟。
    let initial_revision = state
        .lock()
        .expect("service state lock")
        .workbench_generation;

    // app would receive a phase delta only if the stale checked commit bypassed RunStop.
    // app 仅会在过期校验提交绕过 RunStop 时收到阶段增量。
    let app = ServiceEventEmitter::new(Vec::new());
    assert!(
        !commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
            &app,
            &state,
            stale_candidate,
            session_id,
            SessionRunPhase::Preparing,
            expected_generation,
            "turn-stop-race",
        )
        .expect("stale phase commit should be rejected cleanly")
    );
    // registrationError must name the concrete admission owner rather than falling through to an occupied/no-op state.
    // registrationError 必须指出具体准入所有者，不能退化为占用或无操作状态。
    let registration_error = spawn_registered_kernel_stream_task_with_mode(
        &state,
        session_id,
        Uuid::new_v4(),
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        expected_generation,
        "turn-stop-race",
        async {},
    )
    .expect_err("RunStop admission should reject stale stream registration");
    assert!(matches!(
        registration_error,
        ActiveStreamTaskRegistrationError::AdmissionInFlight {
            kind: SessionTurnAdmissionReservationKind::RunStop
        }
    ));

    // guard proves every authoritative surface still reflects the pre-candidate state.
    // guard 证明每个权威界面仍反映候选提交前状态。
    let guard = state.lock().expect("service state lock");
    assert_eq!(guard.workbench_generation, initial_revision);
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
        Some(&SessionRunPhase::Ready)
    );
    assert!(!guard.active_stream_tasks.contains_key(&session_id));
}

/// Verifies a checked preparation phase commit preserves newer transcript UI state from the retained workspace.
/// 验证经过所有者校验的准备阶段提交会保留工作区中更新的转录 UI 状态。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if a detached preparation candidate erases concurrent UI state.
/// 此测试无返回值；若分离准备候选擦除并发 UI 状态则测试失败。
#[test]
fn prompt_preparation_phase_commit_preserves_newer_transcript_ui_state() {
    // sessionId identifies the exact preparation owner and UI-state target.
    // sessionId 标识精确准备所有者与 UI 状态目标。
    let session_id = SessionId::new_v4();
    // serviceState starts one accepted kernel turn before the detached candidate is cloned.
    // serviceState 在克隆分离候选前启动一个已接纳 kernel 回合。
    let mut service_state = test_service_state_with_parent_session(session_id);
    insert_test_workbench_conversation(&mut service_state, session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "turn-ui-merge")
        .expect("kernel turn should start");
    // expectedGeneration is the immutable owner required by the checked phase commit.
    // expectedGeneration 是校验阶段提交所需的不可变所有者。
    let expected_generation = webui_session_kernel_generation(&service_state, session_id)
        .expect("kernel generation should exist");
    // detachedCandidate intentionally predates the concurrent transcript UI mutation.
    // detachedCandidate 特意早于并发转录 UI 变更创建。
    let mut detached_candidate = service_state
        .workspace_snapshot
        .clone()
        .expect("workspace should exist");
    detached_candidate
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    // latestRuntimeState receives one UI-owned fact after the detached clone was captured.
    // latestRuntimeState 在分离克隆捕获后接收一个由 UI 拥有的事实。
    let latest_runtime_state = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace should exist")
        .session_runtime_state
        .get_mut(&session_id)
        .expect("runtime state should exist");
    latest_runtime_state
        .ui_state
        .collapsed_message_ids
        .insert(String::from("newer-ui-state"));
    // state exposes both the current owner and newer UI state to the atomic production commit.
    // state 向原子生产提交暴露当前所有者与更新的 UI 状态。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app records the phase event emitted only after the merged workspace commits.
    // app 记录仅在合并工作区提交后发出的阶段事件。
    let app = ServiceEventEmitter::new(Vec::new());

    assert!(
        commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
            &app,
            &state,
            detached_candidate,
            session_id,
            SessionRunPhase::Preparing,
            expected_generation,
            "turn-ui-merge",
        )
        .expect("checked phase commit should succeed")
    );

    // guard proves the preparation phase and the independent UI fact committed together.
    // guard 证明准备阶段与独立 UI 事实一同提交。
    let guard = state.lock().expect("service state lock");
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace should remain available");
    assert_eq!(
        workspace.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Preparing)
    );
    assert!(
        workspace
            .session_runtime_state
            .get(&session_id)
            .expect("runtime state should remain available")
            .ui_state
            .collapsed_message_ids
            .contains("newer-ui-state")
    );
}

/// Rejects final profile publication after a configured source change without mutating retained state.
/// 配置来源变化后拒绝最终画像发布，且不改变保留状态。
#[test]
fn prompt_profile_phase_commit_revalidates_source_before_any_state_change() {
    // Each case uses local synthetic transport identity; no gRPC request or paid inference is performed.
    // 各场景使用本地合成传输身份，不执行 gRPC 请求或付费推理。
    for mutation in [
        "endpoint",
        "credential",
        "user",
        "project",
        "disabled",
        "backend",
        "timeout",
    ] {
        let session_id = SessionId::new_v4();
        let mut service = test_service_state_with_parent_session(session_id);
        insert_test_workbench_conversation(&mut service, session_id);
        start_webui_session_kernel_user_turn(&mut service, session_id, "profile-source-commit")
            .expect("start turn");
        let generation = webui_session_kernel_generation(&service, session_id).expect("generation");
        let bridge = test_agent_runtime_bridge();
        let mut layer = vulcan_vmm::VmmConfigLayer {
            enabled: Some(true),
            backend_mode: Some(vulcan_vmm::VmmBackendMode::ExternalGrpc),
            grpc_target: Some("127.0.0.1:50051".to_owned()),
            user_id: Some(1),
            project_id: Some(7),
            ..Default::default()
        };
        bridge
            .vmm_hook_runtime
            .replace_global_projection(&layer)
            .expect("initial policy");
        let source = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::external(
                &vulcan_vmm::VmmTransportConfig::from_effective(
                    &vulcan_vmm::VmmEffectiveConfig::resolve(&layer, None, None),
                ),
                1,
                7,
            )
            .expect("expected owner"),
            Vec::new(),
        )
        .expect("read source");
        let mut candidate = service.workspace_snapshot.clone().expect("candidate");
        candidate
            .session_runtime_state
            .get_mut(&session_id)
            .expect("runtime")
            .vmm_profile_sync = vulcan_workbench_model::VmmProfileSyncState {
            read_source: Some(source),
            base_profile_content: Some("prepared-profile-content".to_owned()),
            ..Default::default()
        };
        match mutation {
            "endpoint" => layer.grpc_target = Some("127.0.0.1:50052".to_owned()),
            "credential" => layer.grpc_api_key = Some("synthetic-credential".to_owned()),
            "user" => layer.user_id = Some(2),
            "project" => layer.project_id = Some(8),
            "disabled" => layer.enabled = Some(false),
            "backend" => layer.backend_mode = Some(vulcan_vmm::VmmBackendMode::Native),
            "timeout" => layer.handshake_timeout_ms = Some(45_000),
            _ => unreachable!("closed test case list"),
        }
        bridge
            .vmm_hook_runtime
            .replace_global_projection(&layer)
            .expect("policy changed before commit");
        service.agent_runtime = Some(bridge);
        let before_generation = service.workbench_generation;
        let before_profile = service
            .workspace_snapshot
            .as_ref()
            .expect("retained")
            .session_runtime_state[&session_id]
            .vmm_profile_sync
            .clone();
        let state: SharedServiceBridgeState = Arc::new(Mutex::new(service));
        let app = ServiceEventEmitter::new(Vec::new());
        let result = commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
            &app,
            &state,
            candidate,
            session_id,
            SessionRunPhase::Preparing,
            generation,
            "profile-source-commit",
        );
        let retained = state.lock().expect("retained state");
        if mutation == "timeout" {
            assert!(result.expect("non-identity tuning can commit"));
            assert_eq!(retained.workbench_generation, before_generation + 1);
        } else {
            assert!(
                result
                    .expect_err("source must reject before merge")
                    .contains("profile source changed"),
                "{mutation}"
            );
            assert_eq!(
                retained.workbench_generation, before_generation,
                "{mutation}"
            );
            assert_eq!(
                retained
                    .workspace_snapshot
                    .as_ref()
                    .expect("workspace")
                    .session_runtime_state[&session_id]
                    .vmm_profile_sync,
                before_profile,
                "{mutation}"
            );
        }
    }
}

/// Verifies Stop's immediate phase mutation operates on the latest retained workspace instead of replacing it with a detached clone.
/// 验证 Stop 的即时阶段变更作用于最新保留工作区，而不是使用分离克隆替换它。
#[test]
fn stop_phase_commit_mutates_latest_retained_workspace_atomically() {
    // sessionId selects the stopped session and its retained UI state.
    // sessionId 选择被停止的会话及其保留 UI 状态。
    let session_id = SessionId::new_v4();
    // serviceState carries a UI-owned fact that the Stop phase transaction must preserve.
    // serviceState 携带 Stop 阶段事务必须保留的 UI 所有事实。
    let mut service_state = test_service_state_with_parent_session(session_id);
    insert_test_workbench_conversation(&mut service_state, session_id);
    // retainedWorkspace materializes the target runtime entry before adding its independent UI fact.
    // retainedWorkspace 在添加独立 UI 事实前物化目标运行态条目。
    let retained_workspace = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace should exist");
    retained_workspace
        .session_runtime_state
        .entry(session_id)
        .or_default()
        .ui_state
        .collapsed_message_ids
        .insert(String::from("latest-before-stop"));
    // state exposes the retained workspace to the production in-lock mutation helper.
    // state 向生产锁内变更辅助函数暴露保留工作区。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // app accepts the phase delta without requiring a runtime transport.
    // app 接收阶段增量且不要求运行时传输。
    let app = ServiceEventEmitter::new(Vec::new());

    commit_retained_workspace_session_phase_mutation_and_emit_delta(
        &app,
        &state,
        session_id,
        SessionRunPhase::Stopped,
        |workspace| {
            workspace
                .session_runtime_state
                .get_mut(&session_id)
                .expect("runtime state should exist")
                .ui_state
                .collapsed_message_ids
                .insert(String::from("stop-mutation"));
            Ok(())
        },
    )
    .expect("retained Stop phase mutation should commit");

    // guard proves both the prior retained fact and the Stop-owned fact survived one atomic phase commit.
    // guard 证明先前保留事实与 Stop 所有事实均在一次原子阶段提交后存活。
    let guard = state.lock().expect("service state lock");
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace should remain available");
    let collapsed_message_ids = &workspace
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should remain available")
        .ui_state
        .collapsed_message_ids;
    assert!(collapsed_message_ids.contains("latest-before-stop"));
    assert!(collapsed_message_ids.contains("stop-mutation"));
    assert_eq!(
        workspace.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
}

/// Verifies a same-turn-id restart cannot create an ABA match for stale stream registration.
/// 验证相同回合标识的重启无法为过期流登记制造 ABA 匹配。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This asynchronous test returns no value and fails if generation identity is ignored.
/// 此异步测试无返回值；若代际身份被忽略则测试失败。
#[tokio::test]
async fn kernel_bound_stream_registration_rejects_same_turn_id_aba_generation() {
    // sessionId owns both the stale and replacement generations.
    // sessionId 同时拥有过期代际与替代代际。
    let session_id = SessionId::new_v4();
    // serviceState starts and completes the original logical turn.
    // serviceState 启动并完成原始逻辑回合。
    let mut service_state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "same-turn")
        .expect("original kernel turn should start");
    // staleGeneration is retained by the delayed original preparation task.
    // staleGeneration 由延迟的原始准备任务保留。
    let stale_generation = webui_session_kernel_generation(&service_state, session_id)
        .expect("original generation should exist");
    finish_webui_session_kernel_turn(
        &mut service_state,
        session_id,
        TurnTerminalStatus::Completed,
    );
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "same-turn")
        .expect("replacement kernel turn should start");
    // currentGeneration proves the logical id was reused under a distinct owner generation.
    // currentGeneration 证明逻辑标识在不同所有者代际下被复用。
    let current_generation = webui_session_kernel_generation(&service_state, session_id)
        .expect("replacement generation should exist");
    assert_ne!(stale_generation, current_generation);
    // state exposes the replacement owner to the production registration fence.
    // state 向生产登记栅栏暴露替代所有者。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

    let error = spawn_registered_kernel_stream_task_with_mode(
        &state,
        session_id,
        Uuid::new_v4(),
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        stale_generation,
        "same-turn",
        async {},
    )
    .expect_err("stale generation should not register under the reused logical turn id");
    assert!(matches!(
        error,
        ActiveStreamTaskRegistrationError::KernelOwnerStale {
            expected_generation,
            current_generation: Some(observed_generation),
            ref expected_turn_id,
            ref current_turn_id,
            stopped: false,
        } if expected_generation == stale_generation
            && observed_generation == current_generation
            && expected_turn_id == "same-turn"
            && current_turn_id.as_deref() == Some("same-turn")
    ));
    assert!(
        !state
            .lock()
            .expect("service state lock")
            .active_stream_tasks
            .contains_key(&session_id)
    );
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

#[test]
fn preparation_failure_boundary_holds_terminal_owner_until_explicit_release() {
    // session_id identifies the exact preparation failure transaction.
    // session_id 标识精确的准备失败事务。
    let session_id = SessionId::new_v4();
    // failed_turn_id is shared by the active kernel and cleanup ownership proof.
    // failed_turn_id 由活动 kernel 与清理所有权证明共享。
    let failed_turn_id = "preparation-turn-a";
    // state contains one active preparation kernel and its mirrored runtime state.
    // state 包含一个活动准备 kernel 及其镜像运行态。
    let mut state = test_service_state_with_parent_session(session_id);
    insert_test_workbench_conversation(&mut state, session_id);
    start_webui_session_kernel_user_turn(&mut state, session_id, failed_turn_id)
        .expect("preparation kernel should start");
    // shared_state exposes the production-shaped atomic owner transaction.
    // shared_state 暴露生产形态的原子所有者事务。
    let shared_state = Arc::new(Mutex::new(state));

    commit_prompt_preparation_failure_boundary(
        &shared_state,
        session_id,
        failed_turn_id,
        "controlled preparation failure",
    )
    .expect("failure boundary should commit");

    // terminal_owner is the unique marker that must survive until durable settlement releases it.
    // terminal_owner 是必须存续到可靠结算显式释放的唯一标记。
    let terminal_owner = {
        let guard = shared_state.lock().expect("inspect failure boundary");
        assert!(!webui_session_kernel_has_active_turn(&guard, session_id));
        assert_eq!(
            guard
                .workspace_snapshot
                .as_ref()
                .and_then(|workspace| workspace.session_run_phases.get(&session_id)),
            Some(&SessionRunPhase::Stopped)
        );
        let runtime_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
            .expect("failure runtime state");
        assert!(
            runtime_state
                .conversation_state
                .records
                .iter()
                .any(|record| {
                    record.kind == SessionMessageKind::Error
                        && record.body.contains("controlled preparation failure")
                })
        );
        let owner = *guard
            .session_terminal_persistence_owners
            .get(&session_id)
            .expect("terminal persistence owner");
        assert!(matches!(
            owner,
            SessionTerminalPersistenceOwner::PromptPreparation(_)
        ));
        owner
    };
    {
        let mut guard = shared_state.lock().expect("reject replacement turn");
        assert!(
            start_webui_session_kernel_user_turn(&mut guard, session_id, "replacement-turn")
                .is_err()
        );
    }
    // terminal_guard models successful durable settlement and opens admission only for the exact owner.
    // terminal_guard 模拟可靠结算成功，并且仅为精确所有者开放准入。
    let terminal_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        shared_state.clone(),
        session_id,
        terminal_owner,
    )
    .expect("adopt preparation terminal owner");
    terminal_guard
        .release()
        .expect("release preparation terminal owner");
    let mut guard = shared_state.lock().expect("start replacement turn");
    start_webui_session_kernel_user_turn(&mut guard, session_id, "replacement-turn")
        .expect("replacement turn should start after durable release");
}

/// Verifies a finished pre-API failure persists one V1 error and releases the exact admission owner.
/// 验证 API 前结束失败会持久化一条 V1 错误，并释放精确准入 owner。
#[tokio::test]
async fn finished_pre_api_failure_persists_error_and_reopens_admission() {
    let session_id = SessionId::from_u128(0x9608);
    let mut document =
        SessionDocument::new(session_id.to_string(), "project-a", "Pre-API failure", 1);
    document
        .submit_root_input(
            vulcan_session_core::v1::ClientInputSubmission::new(
                vulcan_session_core::v1::ClientActionId::try_new("pre-api-root")
                    .expect("valid client action id"),
                "run provider request",
                Vec::new(),
            ),
            2,
        )
        .expect("root input should open one V1 turn");
    let mut service_state = test_service_state_with_parent_session(session_id);
    let handle = register_test_session_v1_document(&mut service_state, session_id, document);
    let first_owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    register_session_terminal_persistence_owner(&mut service_state, session_id, first_owner)
        .expect("first terminal owner should register");
    let state = Arc::new(Mutex::new(service_state));
    let first_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        first_owner,
    )
    .expect("first terminal owner should be adoptable");

    assert!(
        settle_stream_terminal_authority(
            &state,
            session_id,
            false,
            "stream_finished_without_v1_api_terminal",
            Some("original preparation failure".to_owned()),
            first_guard,
        )
        .await
    );
    {
        let guard = state
            .lock()
            .expect("terminal owner table should remain readable");
        assert!(
            guard
                .session_terminal_persistence_owners
                .get(&session_id)
                .is_none()
        );
    }
    let first_revision = handle
        .read(SessionDocument::memory_revision)
        .expect("V1 revision should remain readable");
    let matching_notes = handle
        .read(|document| {
            document
                .events()
                .iter()
                .filter(|event| {
                    matches!(
                        &event.event,
                        vulcan_session_core::v1::EventBody::DisplayNote(note)
                            if note.notice_kind
                                == vulcan_session_core::v1::DisplayNoticeKind::SystemError
                                && note.reason_code == "stream_finished_without_v1_api_terminal"
                                && note.redacted_detail.as_deref() == Some("original preparation failure")
                    )
                })
                .count()
        })
        .expect("V1 error projection should remain readable");
    assert_eq!(matching_notes, 1);

    // Kernel-owner release alone is insufficient: the durable logical Turn must be closed too.
    // 仅释放 kernel owner 不够：可靠逻辑回合也必须闭合。
    handle
        .read(|document| {
            assert!(document.active_turn().is_none_or(|turn| turn.closed));
            let mut reloaded = document.clone();
            reloaded.rebuild_derived_state().unwrap();
            reloaded
                .submit_root_input(
                    vulcan_session_core::v1::ClientInputSubmission::new(
                        vulcan_session_core::v1::ClientActionId::try_new("after-pre-api-failure")
                            .unwrap(),
                        "new prompt",
                        Vec::new(),
                    ),
                    10,
                )
                .unwrap();
        })
        .unwrap();

    let second_owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    {
        let mut guard = state.lock().expect("second terminal owner should register");
        register_session_terminal_persistence_owner(&mut guard, session_id, second_owner)
            .expect("second terminal owner should register exactly");
    }
    let second_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        second_owner,
    )
    .expect("second terminal owner should be adoptable");
    assert!(
        settle_stream_terminal_authority(
            &state,
            session_id,
            false,
            "stream_finished_without_v1_api_terminal",
            Some("original preparation failure".to_owned()),
            second_guard,
        )
        .await
    );
    assert_eq!(
        handle
            .read(SessionDocument::memory_revision)
            .expect("idempotent V1 revision should remain readable"),
        first_revision
    );
    let mut guard = state
        .lock()
        .expect("replacement admission should be inspectable");
    start_webui_session_kernel_user_turn(&mut guard, session_id, "replacement-turn")
        .expect("replacement turn should start after durable terminal settlement");
}

/// Verifies stale preparation cleanup cannot stop a replacement kernel turn.
/// 验证过期准备清理不能停止替代 kernel 回合。
#[test]
fn preparation_failure_boundary_rejects_replacement_kernel_turn() {
    // session_id identifies the session whose replacement owner must survive.
    // session_id 标识替代所有者必须存续的会话。
    let session_id = SessionId::new_v4();
    // state contains a newer active turn than the failed cleanup attempts to terminalize.
    // state 包含一个比失败清理目标更新的活动回合。
    let mut state = test_service_state_with_parent_session(session_id);
    start_webui_session_kernel_user_turn(&mut state, session_id, "replacement-turn")
        .expect("replacement kernel should start");
    // shared_state exposes the concurrent replacement facts to cleanup.
    // shared_state 向清理暴露并发替代事实。
    let shared_state = Arc::new(Mutex::new(state));

    let error = commit_prompt_preparation_failure_boundary(
        &shared_state,
        session_id,
        "stale-turn",
        "stale preparation failure",
    )
    .expect_err("stale cleanup should be rejected");

    assert!(error.contains("turn mismatch"));
    let guard = shared_state.lock().expect("inspect replacement owner");
    assert!(webui_session_kernel_has_active_turn(&guard, session_id));
    assert!(
        guard
            .session_terminal_persistence_owners
            .get(&session_id)
            .is_none()
    );
}
