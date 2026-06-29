//! mailbox tests.
//! mailbox tests。

use super::*;

/// Verifies subagent mailbox input uses the generation captured when the child was spawned.
/// 验证子代理 mailbox 输入使用子任务创建时捕获的父级代际。
#[test]
fn webui_subagent_mailbox_rejects_child_result_from_stale_parent_generation() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "subagent_mailbox_event": {
                "kind": "completed",
                "payload": {
                    "result_text": "old child done"
                }
            }
        }
    });

    start_webui_session_kernel_user_turn(&mut state, parent_session_id, "turn-a")
        .expect("first parent turn should start");
    bind_webui_subagent_parent_turn(&mut state, parent_session_id, task_session_id);
    let old_generation =
        webui_session_kernel_generation(&state, parent_session_id).expect("generation");
    stop_webui_session_kernel_generation(&mut state, parent_session_id);
    start_webui_session_kernel_user_turn(&mut state, parent_session_id, "turn-b")
        .expect("second parent turn should start");

    assert_ne!(
        old_generation,
        webui_session_kernel_generation(&state, parent_session_id).expect("new generation")
    );
    assert!(!enqueue_webui_subagent_mailbox_input(
        &mut state,
        parent_session_id,
        task_session_id,
        &metadata,
    ));
    let kernel = state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 0);
}

/// Verifies a mailbox-triggered kernel turn accepts queued subagent mailbox input.
/// 验证 mailbox 触发的内核回合会接收已排队的子代理 mailbox 输入。
#[test]
fn webui_session_kernel_mailbox_turn_accepts_subagent_input() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "subagent_mailbox_event": {
                "kind": "completed",
                "payload": {
                    "result_text": "done"
                }
            }
        }
    });

    assert!(enqueue_webui_subagent_mailbox_input(
        &mut state,
        parent_session_id,
        task_session_id,
        &metadata,
    ));
    start_webui_session_kernel_turn(
        &mut state,
        parent_session_id,
        "mailbox-turn",
        ActiveTurnKind::MailboxTriggered,
    )
    .expect("mailbox turn should start");

    let kernel = state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 0);
    assert_eq!(kernel.input_queue().current_turn_len(), 1);
    assert_eq!(
        kernel.input_queue().get_pending_input()[0].kind,
        TurnInputKind::SubagentMessage
    );
}

/// Verifies sibling subagents spawned by one parent turn can still report during a mailbox-triggered turn.
/// 验证同一父回合派生的兄弟子代理在 mailbox 回流回合期间仍可完成回报。
#[test]
fn webui_subagent_mailbox_accepts_sibling_during_mailbox_turn() {
    let parent_session_id = SessionId::new_v4();
    let first_task_session_id = SessionId::new_v4();
    let second_task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let first_metadata = serde_json::json!({
        "payload": {
            "subagent_mailbox_event": {
                "kind": "completed",
                "payload": {
                    "result_text": "first child done"
                }
            }
        }
    });
    let second_metadata = serde_json::json!({
        "payload": {
            "subagent_mailbox_event": {
                "kind": "completed",
                "payload": {
                    "result_text": "second child done"
                }
            }
        }
    });

    start_webui_session_kernel_user_turn(&mut state, parent_session_id, "turn-a")
        .expect("parent user turn should start");
    bind_webui_subagent_parent_turn(&mut state, parent_session_id, first_task_session_id);
    bind_webui_subagent_parent_turn(&mut state, parent_session_id, second_task_session_id);
    let parent_generation =
        webui_session_kernel_generation(&state, parent_session_id).expect("generation");
    finish_webui_session_kernel_turn(&mut state, parent_session_id, TurnTerminalStatus::Completed);
    assert!(enqueue_webui_subagent_mailbox_input(
        &mut state,
        parent_session_id,
        first_task_session_id,
        &first_metadata,
    ));
    start_webui_session_kernel_turn(
        &mut state,
        parent_session_id,
        "mailbox-turn",
        ActiveTurnKind::MailboxTriggered,
    )
    .expect("mailbox turn should start");

    assert_eq!(
        webui_session_kernel_generation(&state, parent_session_id),
        Some(parent_generation)
    );
    assert!(enqueue_webui_subagent_mailbox_input(
        &mut state,
        parent_session_id,
        second_task_session_id,
        &second_metadata,
    ));
    let kernel = state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    let active_inputs = kernel.input_queue().get_unconsumed_mailbox_input();
    assert_eq!(active_inputs.len(), 2);
    assert!(active_inputs.iter().any(
        |input| input.payload["mailbox_event"]["payload"]["result_text"] == "second child done"
    ));
}

/// Verifies wait_agent completion leaves matching mailbox input for parent delivery.
/// 验证 wait_agent 完成后保留匹配 mailbox 输入供父会话投递。
#[test]
fn wait_agent_completion_leaves_matching_mailbox_input() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "subagent_mailbox_event": {
                "kind": "completed",
                "child_thread_id": task_session_id.to_string(),
                "payload": {
                    "task_name": "agent-review",
                    "subagent_task_id": task_session_id.to_string(),
                    "result_text": "done"
                }
            }
        }
    });
    assert!(enqueue_webui_subagent_mailbox_input(
        &mut service_state,
        parent_session_id,
        task_session_id,
        &metadata,
    ));
    let state = Arc::new(Mutex::new(service_state));
    let started_event = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "wait_agent started",
        "waiting",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "wait_agent",
        "tool_call_id": "wait-call-1",
        "arguments": {
            "task_name": "agent-review"
        }
    })));
    update_webui_wait_agent_runtime_state(&state, parent_session_id, &started_event);
    let finished_event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "wait_agent finished",
        "finished",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "wait_agent",
        "tool_call_id": "wait-call-1",
        "host_result": {
            "kind": "subagent_status",
            "payload": {
                "completed": true,
                "agents": [{
                    "task_name": "agent-review",
                    "subagent_task_id": task_session_id.to_string(),
                    "status": "completed"
                }]
            }
        }
    })));

    update_webui_wait_agent_runtime_state(&state, parent_session_id, &finished_event);

    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
    assert_eq!(kernel.input_queue().consumed_input_count(), 0);
    assert_eq!(kernel.wait_scope_count(), 0);
}

/// Verifies wait_agent completion leaves mailbox input when host result selectors are absent.
/// 验证 wait_agent 完成且宿主结果无选择器时仍保留 mailbox 输入。
#[test]
fn wait_agent_completion_leaves_mailbox_input_without_selectors() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "subagent_mailbox_event": {
                "kind": "completed",
                "child_thread_id": task_session_id.to_string(),
                "payload": {
                    "task_name": "agent-image",
                    "subagent_task_id": task_session_id.to_string(),
                    "result_text": "done"
                }
            }
        }
    });
    assert!(enqueue_webui_subagent_mailbox_input(
        &mut service_state,
        parent_session_id,
        task_session_id,
        &metadata,
    ));
    let state = Arc::new(Mutex::new(service_state));
    let started_event = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "wait_agent started",
        "waiting",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "wait_agent",
        "tool_call_id": "wait-call-no-selectors"
    })));
    update_webui_wait_agent_runtime_state(&state, parent_session_id, &started_event);
    let finished_event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "wait_agent finished",
        "finished",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "wait_agent",
        "tool_call_id": "wait-call-no-selectors",
        "host_result": {
            "kind": "subagent_status",
            "payload": {
                "completed": true
            }
        }
    })));

    update_webui_wait_agent_runtime_state(&state, parent_session_id, &finished_event);

    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
    assert_eq!(kernel.input_queue().consumed_input_count(), 0);
    assert_eq!(kernel.wait_scope_count(), 0);
}

/// Verifies a late delegate-finished mailbox input is queued after wait_agent already completed.
/// 验证 wait_agent 已完成后迟到的委派完成 mailbox 输入仍会入队。
#[test]
fn session_mailbox_queues_late_delegate_finished_after_completed_wait_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let event = test_delegate_finished_event(task_session_id, "agent-review", "done");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("late mailbox input should be checked");

    assert!(queued);
    let guard = state.lock().expect("state lock");
    assert!(
        guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .is_some_and(|kernel| kernel.input_queue().mailbox_len() == 1)
    );
}

/// Verifies stopped parent sessions never enqueue session mailbox input.
/// 验证已停止的父会话绝不会加入会话 mailbox 输入。
#[test]
fn session_mailbox_skips_stopped_parent_session() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let event = test_delegate_finished_event(task_session_id, "agent-review", "done");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        true,
    )
    .expect("stopped mailbox input should be checked");

    assert!(!queued);
    let guard = state.lock().expect("state lock");
    assert!(
        guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .is_none_or(|kernel| kernel.input_queue().mailbox_len() == 0)
    );
}

/// Verifies provider-loop mailbox intervention consumes unclaimed child-agent messages.
/// 验证 Provider 循环 mailbox 干预会消费未被 wait 接管的子代理消息。
#[test]
fn mailbox_intervention_prompt_consumes_unclaimed_mailbox_input() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "subagent_mailbox_event": {
                "kind": "completed",
                "child_thread_id": task_session_id.to_string(),
                "payload": {
                    "task_name": "agent-image",
                    "subagent_task_id": task_session_id.to_string(),
                    "result_text": "image done"
                }
            }
        }
    });
    assert!(enqueue_webui_subagent_mailbox_input(
        &mut service_state,
        parent_session_id,
        task_session_id,
        &metadata,
    ));
    let state = Arc::new(Mutex::new(service_state));

    let prompt = take_webui_mailbox_intervention_prompt(&state, parent_session_id)
        .expect("mailbox intervention should read")
        .expect("mailbox intervention prompt should exist");

    assert!(prompt.contains("<subagent_mailbox_event>"));
    assert!(prompt.contains("image done"));
    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
    assert_eq!(kernel.input_queue().consumed_input_count(), 1);
}

/// Verifies provider-loop mailbox intervention leaves wait-agent-owned messages untouched.
/// 验证 Provider 循环 mailbox 干预不会消费 wait_agent 接管的消息。
#[test]
fn mailbox_intervention_prompt_skips_active_wait_scope() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "subagent_mailbox_event": {
                "kind": "completed",
                "child_thread_id": task_session_id.to_string(),
                "payload": {
                    "task_name": "agent-image",
                    "subagent_task_id": task_session_id.to_string(),
                    "result_text": "image done"
                }
            }
        }
    });
    assert!(enqueue_webui_subagent_mailbox_input(
        &mut service_state,
        parent_session_id,
        task_session_id,
        &metadata,
    ));
    session_runtime_kernel_mut(&mut service_state, parent_session_id)
        .register_wait_scope(
            "wait-agent",
            [task_session_id.to_string()],
            current_unix_millis_u64(),
        )
        .expect("wait scope should register");
    let state = Arc::new(Mutex::new(service_state));

    let prompt = take_webui_mailbox_intervention_prompt(&state, parent_session_id)
        .expect("mailbox intervention should read");

    assert!(prompt.is_none());
    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
    assert_eq!(kernel.input_queue().consumed_input_count(), 0);
}

/// Verifies stopped subagent metadata enters the parent mailbox as a child-agent message.
/// 验证已停止子代理元数据会作为子代理消息进入父会话 mailbox。
#[test]
fn stopped_subagent_metadata_enqueues_parent_mailbox_notification() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = build_stopped_task_session_metadata(
        None,
        None,
        task_session_id,
        SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
        "1000",
    );

    assert!(enqueue_webui_subagent_mailbox_input(
        &mut service_state,
        parent_session_id,
        task_session_id,
        &metadata,
    ));
    let kernel = service_state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    let mailbox_inputs = kernel.input_queue().get_mailbox_input();

    assert_eq!(mailbox_inputs.len(), 1);
    assert_eq!(mailbox_inputs[0].kind, TurnInputKind::SubagentMessage);
    assert!(mailbox_inputs[0].trigger_turn);
    assert_eq!(
        mailbox_inputs[0].payload["mailbox_event"]["payload"]["execution_status"],
        "stopped"
    );
    assert_eq!(
        mailbox_inputs[0].payload["mailbox_event"]["payload"]["result"],
        SUBAGENT_TASK_STOPPED_SUMMARY_TEXT
    );
}
