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

/// Builds one minimal subagent-completed metadata payload for kernel restore tests.
/// 为内核恢复测试构建一个最小子代理完成元数据载荷。
fn restored_kernel_subagent_metadata(result: &str) -> serde_json::Value {
    serde_json::json!({
        "payload": {
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
    let input_id = format!("subagent-mailbox-{parent_session_id}-{task_session_id}");

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
