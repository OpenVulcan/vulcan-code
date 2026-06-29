//! session kernel tests.
//! session kernel tests。

use super::*;

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

/// Verifies a stopped kernel suppresses stale legacy stream handles for active-run decisions.
/// 验证已停止内核会在活动运行判断中抑制过期旧流任务句柄。
#[tokio::test]
async fn webui_session_stopped_kernel_suppresses_stale_stream_handle() {
    let session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(session_id);

    start_webui_session_kernel_user_turn(&mut state, session_id, "turn-a")
        .expect("kernel turn should start");
    state
        .active_stream_tasks
        .insert(session_id, spawn_service_task(async {}));
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

    state
        .active_stream_tasks
        .insert(session_id, spawn_service_task(async {}));

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
