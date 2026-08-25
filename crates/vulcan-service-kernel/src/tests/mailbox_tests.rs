//! mailbox tests.
//! mailbox tests。

use super::*;

/// Captures emitted service events for mailbox projection tests.
/// 为 mailbox 投影测试捕获已发出的服务事件。
struct CapturingServiceEventSink {
    /// Shared event buffer populated by the sink.
    /// sink 写入的共享事件缓冲区。
    events: Arc<Mutex<Vec<ServiceEvent>>>,
}

impl vulcan_service_api::ServiceEventSink for CapturingServiceEventSink {
    /// Appends one emitted service event to the shared test buffer.
    /// 将单个已发出的服务事件追加到共享测试缓冲区。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events.lock().expect("event sink lock").push(event);
    }
}

/// Builds one minimal core controller for mailbox bridge assertions.
/// 为 mailbox bridge 断言构建一个最小 core 控制器。
fn test_mailbox_controller() -> vulcan_core::controller::VulcanCoreController {
    vulcan_core::controller::VulcanCoreController::new(
        vulcan_core::types::ControllerConfig {
            provider_id: String::from("test"),
            model_id: String::from("test-model"),
            system_prompt: String::from("test"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::default(),
            temperature: None,
            max_turns: 4,
            additional_params: None,
            context_injection: None,
        },
        vulcan_core::types::ConversationSnapshot::default(),
    )
    .expect("valid core conversation snapshot")
}

/// Registers the exact root stream task used by wait-agent runtime-event tests.
/// 注册 wait-agent 运行时事件测试使用的精确根流任务。
///
/// # Parameters
/// # 参数
/// `state` owns the active-stream registry receiving the test task.
/// `state` 持有接收测试任务的活跃流注册表。
/// `session_id` identifies the root session owned by the test task.
/// `session_id` 标识测试任务持有的根会话。
///
/// # Returns
/// # 返回值
/// Returns the exact task identifier required by wait-agent state commits.
/// 返回 wait-agent 状态提交所需的精确任务标识。
fn register_wait_agent_test_stream(state: &mut ServiceBridgeState, session_id: SessionId) -> Uuid {
    // task_id is the unique root-stream authority installed for this test.
    // task_id 是为当前测试安装的唯一根流权限标识。
    let task_id = Uuid::new_v4();
    state.active_stream_tasks.insert(
        session_id,
        ActiveStreamTask::new(task_id, spawn_service_task(async {}), None),
    );
    task_id
}

/// Verifies guided prompt intervention removes the consumed visible queue row immediately.
/// 验证引导提示词干预会立即移除已消费的可见队列行。
#[test]
fn guided_prompt_intervention_emits_consumed_pending_prompt_delta() {
    let parent_session_id = SessionId::new_v4();
    let panel_entry_id = String::from("guided-panel-a");
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state.workbench.selected_conversation_id = Some(parent_session_id.to_string());
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist");
    let session = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == parent_session_id)
        .expect("parent session should exist");
    session.status = SessionState::Running;
    workspace_snapshot
        .session_run_phases
        .insert(parent_session_id, SessionRunPhase::ToolRunning);
    let mut runtime_state = SessionRuntimeStateEntry {
        updated_at: String::from("1000"),
        ..SessionRuntimeStateEntry::default()
    };
    runtime_state.ui_state.pending_prompt_panel_queue.push(
        vulcan_workbench_model::PendingPromptPanelEntry {
            id: panel_entry_id.clone(),
            prompt: String::from("请只给建议"),
            hidden_model_guidance: None,
            preview: String::from("请只给建议"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: vulcan_workbench_model::PendingPromptRuntimeOptions::default(),
            state: vulcan_workbench_model::PendingPromptPanelEntryState::Ready,
            created_at: String::from("1000"),
        },
    );
    vulcan_workbench_model::route_pending_prompt_panel_entry_into_internal_queue(
        &mut runtime_state.ui_state,
        &panel_entry_id,
        String::from("guided-internal-a"),
        String::from("1001"),
        PendingSessionPromptSource::UserInterrupt,
    )
    .expect("visible prompt should route into internal guidance queue");
    service_state.workbench.pending_prompts =
        pending_prompt_summaries_from_runtime_state(&runtime_state);
    workspace_snapshot
        .session_runtime_state
        .insert(parent_session_id, runtime_state);
    let state = Arc::new(Mutex::new(service_state));
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
        events: Arc::clone(&emitted_events),
    })]);
    let intervention = build_webui_prompt_intervention(app, Arc::clone(&state), None);
    let mut controller = test_mailbox_controller();

    assert!(
        intervention(parent_session_id, &mut controller).expect("valid guided prompt intervention")
    );

    assert_eq!(controller.snapshot().messages.len(), 1);
    match &controller.snapshot().messages[0].payload {
        vulcan_core::types::ConversationPayload::UserText { text, attachments } => {
            assert_eq!(text, "请只给建议");
            assert!(attachments.is_empty());
        }
        other => panic!("expected guided user text, got {other:?}"),
    }
    let guard = state.lock().expect("state lock");
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id))
        .expect("runtime state should remain available");
    assert!(runtime_state.ui_state.pending_prompt_queue.is_empty());
    assert!(runtime_state.ui_state.pending_prompt_panel_queue.is_empty());
    assert!(guard.workbench.pending_prompts.is_empty());
    let events = emitted_events.lock().expect("event sink lock");
    assert!(events.iter().any(|event| matches!(
        event,
        ServiceEvent::WorkbenchDelta(envelope)
            if matches!(
                &envelope.delta,
                WorkbenchDelta::PendingPromptsChanged(delta)
                    if delta.conversation_id == parent_session_id.to_string()
                        && delta.pending_prompts.is_empty()
            )
    )));
}

/// Verifies WebUI intervention conversion failures preserve both Core state and the live mailbox queue.
/// 验证 WebUI 干预转换失败时会同时保留 Core 状态与实时 mailbox 队列。
#[test]
fn webui_prompt_intervention_is_atomic_on_invalid_mailbox_payload() {
    let parent_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    start_webui_session_kernel_user_turn(&mut service_state, parent_session_id, "turn-a")
        .expect("parent turn should start");
    // invalid_input lacks the required mailbox event structure and must fail before the draft drains.
    // invalid_input 缺少必需的 mailbox 事件结构，必须在草稿取出前失败。
    let invalid_input = TurnInput::new(
        "invalid-mailbox-input",
        SessionNodeId::from(parent_session_id),
        TurnInputKind::SubagentMessage,
        false,
        current_unix_millis_u64(),
        serde_json::json!({ "invalid": true }),
    );
    session_runtime_kernel_mut(&mut service_state, parent_session_id)
        .input_queue_mut()
        .enqueue_current_turn_input(invalid_input);
    let state = Arc::new(Mutex::new(service_state));
    let intervention = build_webui_prompt_intervention(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&state),
        None,
    );
    let mut controller = test_mailbox_controller();
    // original_controller captures every Core field before the rejected intervention.
    // original_controller 捕获被拒绝干预前的全部 Core 字段。
    let original_controller = serde_json::to_value(controller.snapshot())
        .expect("original controller snapshot should serialize");

    let error = intervention(parent_session_id, &mut controller)
        .expect_err("invalid mailbox payload must propagate");

    let RuntimePromptInterventionError::InvalidInput { message } = error else {
        panic!("invalid mailbox payload should remain an input protocol error");
    };
    assert!(message.contains("mailbox"));
    assert_eq!(
        serde_json::to_value(controller.snapshot())
            .expect("controller snapshot should serialize after rejection"),
        original_controller
    );
    let guard = state.lock().expect("state lock");
    assert_eq!(
        guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should remain present")
            .input_queue()
            .current_turn_len(),
        1
    );
}

/// Verifies an accepted model switch suspends the frozen route before any queued guidance is consumed.
/// 验证已接受的模型切换会在消费任何排队引导前暂停冻结路由。
#[test]
fn pending_model_switch_preempts_prompt_intervention_without_draining_guidance() {
    // parentSessionId identifies the exact running session whose next Provider request must switch routes.
    // parentSessionId 标识下一次 Provider 请求必须切换路由的精确运行会话。
    let parent_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let runtime_state = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist")
        .session_runtime_state
        .entry(parent_session_id)
        .or_insert_with(SessionRuntimeStateEntry::default);
    runtime_state.pending_model_selection =
        Some(vulcan_workbench_model::SessionPendingModelSelection {
            provider_id: String::from("command-code"),
            connection_id: String::from("command-code-account"),
            model_id: String::from("deepseek"),
        });
    runtime_state.ui_state.pending_prompt_queue.push(
        vulcan_workbench_model::PendingSessionPromptEntry {
            id: String::from("guided-after-switch"),
            prompt: String::from("补充说明"),
            hidden_model_guidance: None,
            preview: String::from("补充说明"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: vulcan_workbench_model::PendingPromptRuntimeOptions::default(),
            source: PendingSessionPromptSource::UserInterrupt,
            task_id: None,
            panel_entry_id: None,
            created_at: String::from("1000"),
        },
    );
    let state = Arc::new(Mutex::new(service_state));
    let intervention = build_webui_prompt_intervention(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&state),
        None,
    );
    let mut controller = test_mailbox_controller();

    let error = intervention(parent_session_id, &mut controller)
        .expect_err("pending route must suspend the frozen Provider loop");

    assert!(matches!(
        error,
        RuntimePromptInterventionError::ProviderRouteSwitchRequired { .. }
    ));
    assert!(controller.snapshot().messages.is_empty());
    let guard = state.lock().expect("state lock");
    let retained_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id))
        .expect("runtime state should remain present");
    assert_eq!(retained_state.ui_state.pending_prompt_queue.len(), 1);
    assert!(retained_state.pending_model_selection.is_some());
}

/// Verifies a live context-node change suspends the frozen budget before the next Provider request.
/// 验证实时上下文节点变更会在下一次 Provider 请求前暂停冻结预算。
#[test]
fn live_context_node_change_requests_execution_policy_refresh() {
    let parent_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let runtime_state = service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist")
        .session_runtime_state
        .entry(parent_session_id)
        .or_insert_with(SessionRuntimeStateEntry::default);
    runtime_state.context_compaction_window_node_id = Some(
        "compression-window-4"
            .parse()
            .expect("known context node should parse"),
    );
    let state = Arc::new(Mutex::new(service_state));
    let frozen_node_id = Some(
        "compression-window-3"
            .parse()
            .expect("known context node should parse"),
    );
    let intervention = build_webui_prompt_intervention(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&state),
        frozen_node_id,
    );
    let mut controller = test_mailbox_controller();

    let error = intervention(parent_session_id, &mut controller)
        .expect_err("changed context node must suspend the frozen execution policy");

    assert!(matches!(
        error,
        RuntimePromptInterventionError::ContextCompactionPolicyRefreshRequired { .. }
    ));
    assert!(controller.snapshot().messages.is_empty());
    let guard = state.lock().expect("state lock");
    let retained_node_id = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id))
        .and_then(|runtime_state| runtime_state.context_compaction_window_node_id)
        .expect("retained context node should remain present");
    assert_eq!(retained_node_id.as_str(), "compression-window-4");
}

/// Builds one completed subagent mailbox metadata object with the required execution identity.
/// 构建一份带必需执行身份的已完成智能体 mailbox 元数据对象。
fn completed_subagent_mailbox_metadata(
    execution_id: &str,
    task_name: Option<&str>,
    task_session_id: SessionId,
    result_text: &str,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "subagent_task_id": task_session_id.to_string(),
        "result_text": result_text,
    });
    if let Some(task_name) = task_name {
        payload["task_name"] = serde_json::Value::String(task_name.to_string());
    }
    serde_json::json!({
        "payload": {
            "execution_id": execution_id,
            "subagent_mailbox_event": {
                "kind": "completed",
                "child_thread_id": task_session_id.to_string(),
                "payload": payload
            }
        }
    })
}

/// Verifies subagent mailbox input uses the generation captured when the child was spawned.
/// 验证子代理 mailbox 输入使用子任务创建时捕获的父级代际。
#[test]
fn webui_subagent_mailbox_rejects_child_result_from_stale_parent_generation() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "execution_id": "stale-generation-run",
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
    bind_webui_subagent_parent_turn(
        &mut state,
        parent_session_id,
        task_session_id,
        "stale-generation-run",
    );
    let old_generation =
        webui_session_kernel_generation(&state, parent_session_id).expect("generation");
    stop_webui_session_kernel_generation(&mut state, parent_session_id);
    start_webui_session_kernel_user_turn(&mut state, parent_session_id, "turn-b")
        .expect("second parent turn should start");

    assert_ne!(
        old_generation,
        webui_session_kernel_generation(&state, parent_session_id).expect("new generation")
    );
    let outcome = enqueue_webui_subagent_mailbox_input(
        &mut state,
        parent_session_id,
        task_session_id,
        &metadata,
    );
    assert!(matches!(
        outcome,
        SubagentMailboxEnqueueOutcome::Rejected(
            SubagentMailboxEnqueueRejection::GenerationClosed {
                source_generation,
                ..
            }
        ) if source_generation == old_generation
    ));
    let kernel = state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 0);
}

/// Verifies one reused task keeps independent parent bindings for old and new executions.
/// 验证一个复用任务会为旧执行与新执行保留独立父级绑定。
#[test]
fn subagent_mailbox_resolves_parent_binding_by_execution_id() {
    // parent_session_id owns two consecutive parent kernel generations.
    // parent_session_id 持有两个连续的父级 kernel 代际。
    let parent_session_id = SessionId::from_u128(0x1190);
    // task_session_id is intentionally reused across both child executions.
    // task_session_id 被两次子执行刻意复用。
    let task_session_id = SessionId::from_u128(0x1191);
    // state owns both execution-scoped bindings and the parent mailbox.
    // state 持有两条执行级绑定与父级 mailbox。
    let mut state = test_service_state_with_parent_session(parent_session_id);
    start_webui_session_kernel_user_turn(&mut state, parent_session_id, "turn-old")
        .expect("old parent turn should start");
    bind_webui_subagent_parent_turn(
        &mut state,
        parent_session_id,
        task_session_id,
        "execution-old",
    );
    // old_generation is the delivery owner captured by the old child execution.
    // old_generation 是旧子执行捕获的投递所有者。
    let old_generation =
        webui_session_kernel_generation(&state, parent_session_id).expect("old generation");
    finish_webui_session_kernel_turn(&mut state, parent_session_id, TurnTerminalStatus::Completed);
    start_webui_session_kernel_user_turn(&mut state, parent_session_id, "turn-new")
        .expect("new parent turn should start");
    bind_webui_subagent_parent_turn(
        &mut state,
        parent_session_id,
        task_session_id,
        "execution-new",
    );
    // new_generation is the distinct delivery owner captured by the replacement execution.
    // new_generation 是替代执行捕获的不同投递所有者。
    let new_generation =
        webui_session_kernel_generation(&state, parent_session_id).expect("new generation");
    assert_ne!(old_generation, new_generation);
    assert_eq!(state.subagent_parent_turn_bindings.len(), 2);
    // old_metadata identifies a terminal result from the retired execution.
    // old_metadata 标识退役执行的终态结果。
    let old_metadata = completed_subagent_mailbox_metadata(
        "execution-old",
        Some("worker"),
        task_session_id,
        "old result",
    );
    // new_metadata identifies a terminal result from the current execution.
    // new_metadata 标识当前执行的终态结果。
    let new_metadata = completed_subagent_mailbox_metadata(
        "execution-new",
        Some("worker"),
        task_session_id,
        "new result",
    );

    let old_outcome = enqueue_webui_subagent_mailbox_input(
        &mut state,
        parent_session_id,
        task_session_id,
        &old_metadata,
    );
    assert!(matches!(
        old_outcome,
        SubagentMailboxEnqueueOutcome::Rejected(
            SubagentMailboxEnqueueRejection::GenerationClosed {
                source_generation,
                ..
            }
        ) if source_generation == old_generation
    ));
    let new_outcome = enqueue_webui_subagent_mailbox_input(
        &mut state,
        parent_session_id,
        task_session_id,
        &new_metadata,
    );
    assert!(new_outcome.is_queued());
    assert_eq!(
        state
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("parent kernel")
            .input_queue()
            .mailbox_len(),
        1
    );
    clear_webui_subagent_parent_turn_binding(&mut state, task_session_id, "execution-old");
    assert!(
        !state
            .subagent_parent_turn_bindings
            .contains_key(&SubagentExecutionKey::new(task_session_id, "execution-old"))
    );
    assert!(
        state
            .subagent_parent_turn_bindings
            .contains_key(&SubagentExecutionKey::new(task_session_id, "execution-new"))
    );
}

/// Verifies stale child results surface as service-level mailbox delivery errors.
/// 验证过期子代理结果会在服务层表现为 mailbox 投递错误。
#[test]
fn session_mailbox_errors_on_stale_parent_generation() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let event = test_delegate_finished_event(task_session_id, "agent-review", "done");

    start_webui_session_kernel_user_turn(&mut service_state, parent_session_id, "turn-a")
        .expect("first parent turn should start");
    bind_webui_subagent_parent_turn(
        &mut service_state,
        parent_session_id,
        task_session_id,
        "test-execution-agent-review",
    );
    stop_webui_session_kernel_generation(&mut service_state, parent_session_id);
    start_webui_session_kernel_user_turn(&mut service_state, parent_session_id, "turn-b")
        .expect("second parent turn should start");

    let state = Arc::new(Mutex::new(service_state));
    let error = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect_err("stale generation should be surfaced as a delivery error");

    assert!(error.contains("generation_closed"));
    let guard = state.lock().expect("state lock");
    assert!(
        guard
            .subagent_parent_turn_bindings
            .contains_key(&SubagentExecutionKey::new(
                task_session_id,
                "test-execution-agent-review"
            ))
    );
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 0);
}

/// Verifies repeated child results are treated as idempotent mailbox skips.
/// 验证重复子代理结果会被视为幂等 mailbox 跳过。
#[test]
fn webui_subagent_mailbox_treats_duplicate_input_as_idempotent_skip() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "execution_id": "duplicate-run",
            "subagent_mailbox_event": {
                "kind": "completed",
                "payload": {
                    "result_text": "child done once"
                }
            }
        }
    });

    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );
    let duplicate = enqueue_webui_subagent_mailbox_input(
        &mut state,
        parent_session_id,
        task_session_id,
        &metadata,
    );

    assert!(matches!(
        duplicate,
        SubagentMailboxEnqueueOutcome::AlreadyPresentOrConsumed { .. }
    ));
    let kernel = state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
}

/// Verifies repeated task ids can enqueue distinct mailbox inputs for distinct executions.
/// 验证同一任务标识在不同执行轮次下可以加入不同 mailbox 输入。
#[test]
fn webui_subagent_mailbox_allows_same_task_with_distinct_execution_ids() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let first_metadata = completed_subagent_mailbox_metadata(
        "same-task-run-1",
        Some("repeat-agent"),
        task_session_id,
        "first result",
    );
    let second_metadata = completed_subagent_mailbox_metadata(
        "same-task-run-2",
        Some("repeat-agent"),
        task_session_id,
        "second result",
    );

    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            task_session_id,
            &first_metadata,
        )
        .is_queued()
    );
    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            task_session_id,
            &second_metadata,
        )
        .is_queued()
    );

    let kernel = state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    let mailbox_inputs = kernel.input_queue().get_mailbox_input();
    assert_eq!(mailbox_inputs.len(), 2);
    assert!(mailbox_inputs.iter().any(|input| {
        input.payload["execution_id"] == "same-task-run-1"
            && input.payload["mailbox_event"]["payload"]["result_text"] == "first result"
    }));
    assert!(mailbox_inputs.iter().any(|input| {
        input.payload["execution_id"] == "same-task-run-2"
            && input.payload["mailbox_event"]["payload"]["result_text"] == "second result"
    }));
}

/// Verifies child-agent mailbox delivery refuses events without an explicit terminal status.
/// 验证智能体 mailbox 投递会拒绝没有明确终态状态的事件。
#[test]
fn webui_subagent_mailbox_rejects_missing_terminal_status() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "execution_id": "missing-terminal-run",
            "subagent_mailbox_event": {
                "payload": {
                    "result_text": "child text without status"
                }
            }
        }
    });

    let outcome = enqueue_webui_subagent_mailbox_input(
        &mut state,
        parent_session_id,
        task_session_id,
        &metadata,
    );

    assert!(matches!(
        outcome,
        SubagentMailboxEnqueueOutcome::Rejected(
            SubagentMailboxEnqueueRejection::InvalidTerminalStatus { status: None }
        )
    ));
    let kernel = state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 0);
}

/// Verifies legacy mailbox inputs without explicit status render as messages, not final answers.
/// 验证没有明确状态的旧 mailbox 输入会渲染为普通消息，而不是最终答案。
#[test]
fn mailbox_message_rendering_does_not_default_missing_status_to_final_answer() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let input = TurnInput::new(
        String::from("legacy-mailbox-input"),
        SessionNodeId::from(parent_session_id),
        TurnInputKind::SubagentMessage,
        true,
        current_unix_millis_u64(),
        serde_json::json!({
            "task_id": task_session_id.to_string(),
            "parent_session_id": parent_session_id.to_string(),
            "mailbox_event": {
                "payload": {
                    "task_name": "legacy-agent",
                    "result_text": "legacy text without status"
                }
            },
            "trigger_parent_turn": true
        }),
    );

    let rendered = webui_agent_message_text_from_turn_input(parent_session_id, &input);

    assert!(rendered.contains("<message_type>MESSAGE</message_type>"));
    assert!(!rendered.contains("<message_type>FINAL_ANSWER</message_type>"));
}

/// Verifies send_message tool completion is mirrored into the child transcript exactly once.
/// 验证 send_message 工具完成会且只会一次镜像到子会话转录。
#[test]
fn send_message_tool_finished_appends_child_transcript_prompt_once() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let workspace = state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist");
    workspace.hydrated_session_ids.insert(task_session_id);
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · send_message",
        "queued",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "send_message",
        "tool_call_id": "send-followup-1",
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": "send_message",
                "parent_session_id": parent_session_id.to_string(),
                "target": "code-review",
                (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
                "delivery_id": "delivery-followup-1",
                "message": "请用中文重新输出结论。",
                "queued": true,
                "delivery_mode": "queue",
                "interrupted": false
            }
        }
    })));

    assert_eq!(
        append_webui_subagent_send_message_from_event_if_needed(workspace, &event)
            .expect("send_message projection should parse"),
        Some(task_session_id)
    );
    assert_eq!(
        append_webui_subagent_send_message_from_event_if_needed(workspace, &event)
            .expect("duplicate send_message projection should parse"),
        None
    );
    let messages = workspace
        .session_messages
        .get(&task_session_id)
        .expect("child transcript should exist");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "delivery-followup-1");
    assert_eq!(messages[0].title, "智能体消息");
    assert_eq!(messages[0].body, "请用中文重新输出结论。");
    assert_eq!(
        messages[0].record_kind,
        SessionMessageRecordKind::DisplayNote
    );
    assert!(!workspace.hydrated_session_ids.contains(&task_session_id));
}

/// Verifies send_message projection can use the standard tool-finished title id when metadata omits it.
/// 验证 send_message 投影在元数据缺少工具调用标识时可使用标准工具完成标题标识。
#[test]
fn send_message_projection_uses_tool_finished_title_id_when_metadata_omits_call_id() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let workspace = state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist");
    workspace.hydrated_session_ids.insert(task_session_id);
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-send-title-1",
        "queued",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "send_message",
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": "send_message",
                "parent_session_id": parent_session_id.to_string(),
                "target": "code-review",
                (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
                "message": "追加一条没有 metadata call id 的消息。",
                "queued": true,
                "delivery_mode": "queue",
                "interrupted": false
            }
        }
    })));

    assert_eq!(
        append_webui_subagent_send_message_from_event_if_needed(workspace, &event)
            .expect("send_message projection should parse"),
        Some(task_session_id)
    );
    assert_eq!(
        append_webui_subagent_send_message_from_event_if_needed(workspace, &event)
            .expect("duplicate send_message projection should parse"),
        None
    );
    let messages = workspace
        .session_messages
        .get(&task_session_id)
        .expect("child transcript should exist");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "追加一条没有 metadata call id 的消息。");
    assert!(!workspace.hydrated_session_ids.contains(&task_session_id));
}

/// Verifies malformed send_message payloads report errors instead of dropping the visible child note.
/// 验证格式错误的 send_message 载荷会显式报错，而不是丢弃可见子会话记录。
#[test]
fn send_message_projection_reports_invalid_task_session_id() {
    let parent_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let workspace = state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist");
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · send-invalid-task",
        "queued",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "send_message",
        "tool_call_id": "send-invalid-task",
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": "send_message",
                "parent_session_id": parent_session_id.to_string(),
                "target": "code-review",
                (SUBAGENT_TASK_ID_KEY): "not-a-uuid",
                "delivery_id": "delivery-invalid-task",
                "message": "这条消息不能无声丢失。",
                "queued": true,
                "delivery_mode": "queue",
                "interrupted": false
            }
        }
    })));

    let error = append_webui_subagent_send_message_from_event_if_needed(workspace, &event)
        .expect_err("invalid task_session_id should be reported");

    assert!(error.contains("invalid task_session_id"));
    assert!(workspace.session_messages.is_empty());
}

/// Verifies stopped notice projection is idempotent and synchronizes the child dialogue-ready marker.
/// 验证停止提示投影保持幂等，并会同步子任务对话层就绪标记。
#[test]
fn subagent_stop_notice_projection_syncs_dialogue_ready_marker_once() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let workspace = state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist");
    workspace.hydrated_session_ids.insert(task_session_id);

    append_webui_subagent_stop_notice_if_needed(workspace, task_session_id);
    append_webui_subagent_stop_notice_if_needed(workspace, task_session_id);

    let messages = workspace
        .session_messages
        .get(&task_session_id)
        .expect("child transcript should exist");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].title, "执行已停止");
    assert_eq!(messages[0].body, SUBAGENT_TASK_STOPPED_SUMMARY_TEXT);
    assert!(!workspace.hydrated_session_ids.contains(&task_session_id));
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
            "execution_id": "kernel-mailbox-run",
            "subagent_mailbox_event": {
                "kind": "completed",
                "payload": {
                    "result_text": "done"
                }
            }
        }
    });

    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );
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
    assert!(kernel.input_queue().get_pending_input()[0].trigger_turn);
}

/// Verifies mailbox-triggered turns keep child-agent usage out of provider-bound mailbox input.
/// 验证 mailbox 触发回合不会把智能体用量写入发往 Provider 的 mailbox 输入。
#[test]
fn mailbox_trigger_turn_keeps_subagent_usage_out_of_mailbox_payload() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "agent-image",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "usage-free-mailbox-run",
            "execution_status": "completed",
            "result": "image done",
            "subagent_mailbox_event": {
                "kind": "completed",
                "payload": {
                    "task_name": "agent-image",
                    "subagent_task_id": task_session_id.to_string(),
                    "result_text": "image done"
                }
            },
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "agent-image",
                "provider_call_count": 1,
                "tool_call_count": 0,
                "input_tokens": 42,
                "output_tokens": 7,
                "reasoning_tokens": 3,
                "total_tokens": 52,
                "cached_input_tokens": 11
            }]
        }
    });

    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );
    start_webui_session_kernel_turn(
        &mut state,
        parent_session_id,
        "mailbox-turn",
        ActiveTurnKind::MailboxTriggered,
    )
    .expect("mailbox turn should start");
    let consumed_inputs = state
        .session_runtime_kernels
        .get_mut(&parent_session_id)
        .expect("kernel should exist")
        .input_queue_mut()
        .drain_current_turn_inputs();

    assert_eq!(consumed_inputs.len(), 1);
    assert!(consumed_inputs[0].payload.get("usage_segments").is_none());
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
            "execution_id": "sibling-first-run",
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
            "execution_id": "sibling-second-run",
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
    bind_webui_subagent_parent_turn(
        &mut state,
        parent_session_id,
        first_task_session_id,
        "sibling-first-run",
    );
    bind_webui_subagent_parent_turn(
        &mut state,
        parent_session_id,
        second_task_session_id,
        "sibling-second-run",
    );
    let parent_generation =
        webui_session_kernel_generation(&state, parent_session_id).expect("generation");
    finish_webui_session_kernel_turn(&mut state, parent_session_id, TurnTerminalStatus::Completed);
    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            first_task_session_id,
            &first_metadata,
        )
        .is_queued()
    );
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
    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut state,
            parent_session_id,
            second_task_session_id,
            &second_metadata,
        )
        .is_queued()
    );
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

/// Verifies active user-prompt turns defer child-agent feedback to the mailbox boundary.
/// 验证活动用户提示词回合会将智能体反馈推迟到 mailbox 边界。
#[test]
fn active_user_prompt_turn_defers_subagent_feedback_to_mailbox() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = completed_subagent_mailbox_metadata(
        "active-parent-send-message-run",
        Some("repeat-agent"),
        task_session_id,
        "ABCD",
    );

    start_webui_session_kernel_user_turn(&mut service_state, parent_session_id, "turn-a")
        .expect("parent user turn should start");
    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );

    {
        let kernel = service_state
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        assert_eq!(kernel.input_queue().current_turn_len(), 0);
        assert_eq!(kernel.input_queue().mailbox_len(), 1);
        assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
    }

    let state = Arc::new(Mutex::new(service_state));
    let mut controller = test_mailbox_controller();
    let appended =
        append_webui_mailbox_intervention_messages(&state, parent_session_id, &mut controller)
            .expect("mailbox intervention should inspect current-turn feedback");

    assert!(!appended);
    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().current_turn_len(), 0);
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
    assert!(controller.snapshot().messages.is_empty());
}

/// Verifies a send_message terminal tool result consumes the matching mailbox input before enqueue.
/// 验证 send_message 终态工具结果会在入队前消费匹配的 mailbox 输入。
#[test]
fn terminal_send_message_resume_preconsumes_matching_mailbox_input() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let execution_id = "send-message-terminal-run";
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = completed_subagent_mailbox_metadata(
        execution_id,
        Some("repeat-agent"),
        task_session_id,
        "1234",
    );

    assert!(consume_webui_subagent_mailbox_input_for_execution(
        &mut service_state,
        parent_session_id,
        task_session_id,
        execution_id,
    ));
    let outcome = enqueue_webui_subagent_mailbox_input(
        &mut service_state,
        parent_session_id,
        task_session_id,
        &metadata,
    );

    assert!(matches!(
        outcome,
        SubagentMailboxEnqueueOutcome::AlreadyPresentOrConsumed { .. }
    ));
    let kernel = service_state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 0);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
}

/// Verifies terminal send_message resume metadata resolves the exact child execution.
/// 验证 send_message 终态续跑元数据能解析到精确的子执行。
#[test]
fn terminal_send_message_resume_event_parser_resolves_child_execution() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · send-message-terminal",
        "queued",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": SEND_MESSAGE_TOOL,
        "tool_call_id": "send-message-terminal",
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": SEND_MESSAGE_TOOL,
                "parent_session_id": parent_session_id.to_string(),
                "target": "repeat-agent",
                (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
                "delivery_id": "delivery-terminal",
                "message": "输出1234",
                "queued": true,
                "delivery_mode": "queue",
                "interrupted": false,
                "delivery_status": "completed",
                "resumed_execution_id": "send-message-terminal-run",
                "resumed_execution_status": "completed"
            }
        }
    })));

    let resolved = terminal_subagent_send_message_resume_from_event(&event)
        .expect("terminal send_message resume should parse")
        .expect("terminal send_message resume should be present");

    assert_eq!(resolved.0, task_session_id);
    assert_eq!(resolved.1, "send-message-terminal-run");
}

/// Verifies only a concrete running send_message resume produces a completion-gate identity.
/// 验证只有具体且仍在运行的 send_message 续跑才会生成完成闸门身份。
#[test]
fn running_send_message_resume_parser_rejects_queue_only_delivery() {
    // parent_session_id identifies the parent that sends both test messages.
    // parent_session_id 标识发送两条测试消息的父会话。
    let parent_session_id = SessionId::new_v4();
    // task_session_id identifies the stable child target.
    // task_session_id 标识稳定的子任务目标。
    let task_session_id = SessionId::new_v4();
    // base_payload contains the shared protocol fields for a child delivery.
    // base_payload 包含子任务投递共享的协议字段。
    let base_payload = serde_json::json!({
        "tool": SEND_MESSAGE_TOOL,
        "parent_session_id": parent_session_id.to_string(),
        "target": "repeat-agent",
        (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
        "delivery_id": "delivery-running",
        "message": "继续检查",
        "queued": true,
        "delivery_mode": "queue",
        "interrupted": false
    });
    // queued_event represents delivery to an execution that was already active.
    // queued_event 表示投递给一个已经活动的执行。
    let queued_event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · send-message-queued",
        "queued",
    )
    .with_metadata(Some(serde_json::json!({
        "host_result": {
            "kind": "subagent_message",
            "payload": base_payload.clone()
        }
    })));

    assert!(
        running_subagent_send_message_resume_from_event(&queued_event)
            .expect("queue-only send_message should parse")
            .is_none()
    );

    // running_payload extends the delivery with the exact newly resumed execution.
    // running_payload 使用精确的新续跑执行扩展投递载荷。
    let mut running_payload = base_payload;
    running_payload["resumed_execution_id"] = serde_json::json!("send-message-running-run");
    running_payload["resumed_execution_status"] = serde_json::json!("running");
    // running_event represents an idle child synchronously resumed into a new execution.
    // running_event 表示空闲子任务被同步续跑为一个新执行。
    let running_event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · send-message-running",
        "running",
    )
    .with_metadata(Some(serde_json::json!({
        "host_result": {
            "kind": "subagent_message",
            "payload": running_payload
        }
    })));
    // resolved identifies the exact execution eligible for gate suppression.
    // resolved 标识符合完成闸门放行条件的精确执行。
    let resolved = running_subagent_send_message_resume_from_event(&running_event)
        .expect("running send_message should parse")
        .expect("running send_message should expose its execution");

    assert_eq!(resolved.0, task_session_id);
    assert_eq!(resolved.1, "send-message-running-run");
}

/// Verifies wait_agent terminal feedback metadata resolves consumed child executions.
/// 验证 wait_agent 终态反馈元数据能解析已消费的子执行。
#[test]
fn terminal_wait_agent_event_parser_resolves_consumed_child_executions() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · wait-agent-terminal",
        "received",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": WAIT_AGENT_TOOL,
        "tool_call_id": "wait-agent-terminal",
        "host_result": {
            "kind": "subagent_wait",
            "payload": {
                "tool": WAIT_AGENT_TOOL,
                "parent_session_id": parent_session_id.to_string(),
                "agents": [{
                    "subagent_task_id": task_session_id.to_string(),
                    "execution_id": "wait-agent-terminal-run",
                    "kind": "completed",
                    "status": "completed",
                    "result": "done",
                    "terminal": true
                }]
            }
        }
    })));

    let resolved = terminal_subagent_wait_consumed_executions_from_event(&event)
        .expect("wait_agent consumed execution should parse");

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].0, task_session_id);
    assert_eq!(resolved[0].1, "wait-agent-terminal-run");
}

/// Verifies wait_agent completion leaves matching mailbox input for parent delivery.
/// 验证 wait_agent 完成后保留匹配 mailbox 输入供父会话投递。
#[tokio::test]
async fn wait_agent_completion_leaves_matching_mailbox_input() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "execution_id": "wait-agent-matching-run",
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
    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );
    // active_stream_task_id authorizes both wait-agent lifecycle events in this test.
    // active_stream_task_id 授权当前测试中的两个 wait-agent 生命周期事件。
    let active_stream_task_id =
        register_wait_agent_test_stream(&mut service_state, parent_session_id);
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
    assert_eq!(
        update_webui_wait_agent_runtime_state(
            &state,
            parent_session_id,
            active_stream_task_id,
            &started_event,
        )
        .expect("wait-agent start should commit"),
        WaitAgentRuntimeStateCommit::Committed
    );
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

    assert_eq!(
        update_webui_wait_agent_runtime_state(
            &state,
            parent_session_id,
            active_stream_task_id,
            &finished_event,
        )
        .expect("wait-agent finish should commit"),
        WaitAgentRuntimeStateCommit::Committed
    );

    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().current_turn_len(), 1);
    assert_eq!(kernel.input_queue().mailbox_len(), 0);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
    assert_eq!(kernel.input_queue().consumed_input_count(), 0);
    assert_eq!(kernel.wait_scope_count(), 0);
}

/// Verifies wait_agent completion leaves mailbox input when host result selectors are absent.
/// 验证 wait_agent 完成且宿主结果无选择器时仍保留 mailbox 输入。
#[tokio::test]
async fn wait_agent_completion_leaves_mailbox_input_without_selectors() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "execution_id": "wait-agent-no-selector-run",
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
    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );
    // active_stream_task_id authorizes both wait-agent lifecycle events in this test.
    // active_stream_task_id 授权当前测试中的两个 wait-agent 生命周期事件。
    let active_stream_task_id =
        register_wait_agent_test_stream(&mut service_state, parent_session_id);
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
    assert_eq!(
        update_webui_wait_agent_runtime_state(
            &state,
            parent_session_id,
            active_stream_task_id,
            &started_event,
        )
        .expect("wait-agent start should commit"),
        WaitAgentRuntimeStateCommit::Committed
    );
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

    assert_eq!(
        update_webui_wait_agent_runtime_state(
            &state,
            parent_session_id,
            active_stream_task_id,
            &finished_event,
        )
        .expect("wait-agent finish should commit"),
        WaitAgentRuntimeStateCommit::Committed
    );

    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().current_turn_len(), 1);
    assert_eq!(kernel.input_queue().mailbox_len(), 0);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
    assert_eq!(kernel.input_queue().consumed_input_count(), 0);
    assert_eq!(kernel.wait_scope_count(), 0);
}

/// Verifies a fresh wait_agent invocation replaces stale wait scopes before finishing.
/// 验证新的 wait_agent 调用会先替换旧等待范围，再在结束时清干净。
#[tokio::test]
async fn wait_agent_start_replaces_stale_wait_scope() {
    let parent_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    session_runtime_kernel_mut(&mut service_state, parent_session_id)
        .register_wait_scope("stale-wait-scope", Vec::<String>::new(), 10)
        .expect("stale wait scope should register");
    // active_stream_task_id authorizes replacement and completion of the stale wait scope.
    // active_stream_task_id 授权替换并完成旧等待范围。
    let active_stream_task_id =
        register_wait_agent_test_stream(&mut service_state, parent_session_id);
    let state = Arc::new(Mutex::new(service_state));
    let started_event = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "wait_agent started",
        "waiting",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "wait_agent",
        "tool_call_id": "fresh-wait-call"
    })));

    assert_eq!(
        update_webui_wait_agent_runtime_state(
            &state,
            parent_session_id,
            active_stream_task_id,
            &started_event,
        )
        .expect("wait-agent start should replace the stale scope"),
        WaitAgentRuntimeStateCommit::Committed
    );

    {
        let guard = state.lock().expect("state lock");
        let kernel = guard
            .session_runtime_kernels
            .get(&parent_session_id)
            .expect("kernel should exist");
        assert_eq!(kernel.wait_scope_count(), 1);
        assert!(
            guard
                .parent_sessions_waiting_for_subagents
                .contains(&parent_session_id)
        );
    }

    let finished_event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "wait_agent finished",
        "finished",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "wait_agent",
        "tool_call_id": "fresh-wait-call"
    })));

    assert_eq!(
        update_webui_wait_agent_runtime_state(
            &state,
            parent_session_id,
            active_stream_task_id,
            &finished_event,
        )
        .expect("wait-agent finish should clear the replacement scope"),
        WaitAgentRuntimeStateCommit::Committed
    );

    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.wait_scope_count(), 0);
    assert!(
        !guard
            .parent_sessions_waiting_for_subagents
            .contains(&parent_session_id)
    );
}

/// Verifies a superseded root task cannot modify wait-agent coordination state.
/// 验证已被替换的根任务无法修改 wait-agent 协调状态。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if a stale event changes retained wait-agent state.
/// 此测试无返回值；若旧事件修改保留的 wait-agent 状态则失败。
#[tokio::test]
async fn superseded_wait_agent_start_preserves_replacement_coordination_state() {
    // parent_session_id identifies the root session now owned by a replacement stream.
    // parent_session_id 标识当前由替代流持有的根会话。
    let parent_session_id = SessionId::new_v4();
    // stale_task_id identifies the obsolete wait-agent event producer.
    // stale_task_id 标识已过期的 wait-agent 事件生产者。
    let stale_task_id = Uuid::new_v4();
    // service_state retains one replacement-owned scope before the stale event arrives.
    // service_state 在旧事件到达前保留一个由替代任务持有的等待范围。
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    session_runtime_kernel_mut(&mut service_state, parent_session_id)
        .register_wait_scope(
            "replacement-wait-scope",
            vec![String::from("replacement-agent")],
            10,
        )
        .expect("replacement wait scope should register");
    // replacement_task_id is the only current authority for wait-agent state.
    // replacement_task_id 是当前 wait-agent 状态的唯一权限标识。
    let replacement_task_id =
        register_wait_agent_test_stream(&mut service_state, parent_session_id);
    // state exposes the replacement registry and retained kernel to the stale event.
    // state 向旧事件暴露替代任务注册表与保留内核。
    let state = Arc::new(Mutex::new(service_state));
    // stale_started_event attempts to replace the authoritative wait scope.
    // stale_started_event 尝试替换权威等待范围。
    let stale_started_event = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "wait_agent started",
        "waiting",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "wait_agent",
        "tool_call_id": "stale-wait-call"
    })));

    assert_eq!(
        update_webui_wait_agent_runtime_state(
            &state,
            parent_session_id,
            stale_task_id,
            &stale_started_event,
        )
        .expect("stale wait-agent event should be classified"),
        WaitAgentRuntimeStateCommit::Superseded
    );

    // guard proves every replacement-owned coordination surface remains unchanged.
    // guard 证明替代任务持有的全部协调状态表面保持不变。
    let guard = state.lock().expect("state lock");
    assert!(active_stream_task_matches_id(
        &guard,
        parent_session_id,
        replacement_task_id
    ));
    assert!(
        !guard
            .parent_sessions_waiting_for_subagents
            .contains(&parent_session_id)
    );
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("replacement kernel should remain");
    assert_eq!(kernel.wait_scope_count(), 1);
    assert!(kernel.wait_scope_targets_agent("replacement-agent"));
    assert!(
        guard
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot")
            .session_runtime_state
            .is_empty()
    );
}

/// Verifies wait-agent state lock failure is returned explicitly.
/// 验证 wait-agent 状态锁失败会被明确返回。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if a poisoned state lock is silently ignored.
/// 此测试无返回值；若中毒状态锁被静默忽略则失败。
#[tokio::test]
async fn wait_agent_state_lock_failure_is_explicit() {
    // parent_session_id identifies the session whose shared state lock will be poisoned.
    // parent_session_id 标识共享状态锁将被中毒的会话。
    let parent_session_id = SessionId::new_v4();
    // service_state receives an exact active task before lock poisoning.
    // service_state 在状态锁中毒前接收一个精确活跃任务。
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    // active_stream_task_id would authorize the event if the state lock remained available.
    // active_stream_task_id 在状态锁仍可用时会授权该事件。
    let active_stream_task_id =
        register_wait_agent_test_stream(&mut service_state, parent_session_id);
    // state is the exact shared mutex acquired by the wait-agent transaction.
    // state 是 wait-agent 事务获取的精确共享互斥锁。
    let state = Arc::new(Mutex::new(service_state));
    // poisoned_state shares the mutex deliberately poisoned below.
    // poisoned_state 共享下方将被刻意中毒的互斥锁。
    let poisoned_state = Arc::clone(&state);
    // poison_result proves the deliberate panic poisoned the shared state mutex.
    // poison_result 证明刻意 panic 已使共享状态互斥锁中毒。
    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // _guard keeps the state mutex held until the deliberate panic occurs.
        // _guard 将持续持有状态互斥锁直到刻意 panic 发生。
        let _guard = poisoned_state.lock().expect("state lock before poisoning");
        panic!("poison wait-agent service state");
    }));
    assert!(poison_result.is_err());
    // started_event requires the poisoned state lock and therefore must return an error.
    // started_event 需要获取已中毒的状态锁，因此必须返回错误。
    let started_event = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "wait_agent started",
        "waiting",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "wait_agent",
        "tool_call_id": "poisoned-wait-call"
    })));

    let error = update_webui_wait_agent_runtime_state(
        &state,
        parent_session_id,
        active_stream_task_id,
        &started_event,
    )
    .expect_err("poisoned wait-agent state lock should fail explicitly");
    assert!(error.contains("failed to lock service state"));
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

/// Verifies provider-loop mailbox intervention appends typed child-agent messages.
/// 验证 Provider 循环 mailbox 干预会追加类型化子代理消息。
#[test]
fn mailbox_intervention_appends_typed_agent_message() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "execution_id": "intervention-typed-run",
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
    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );
    start_webui_session_kernel_user_turn(&mut service_state, parent_session_id, "turn-a")
        .expect("parent turn should start");
    let state = Arc::new(Mutex::new(service_state));
    let mut controller = test_mailbox_controller();

    let appended =
        append_webui_mailbox_intervention_messages(&state, parent_session_id, &mut controller)
            .expect("mailbox intervention should append");

    assert!(appended);
    assert_eq!(controller.snapshot().messages.len(), 1);
    let vulcan_core::types::ConversationPayload::AgentMessage {
        message_type,
        task_name,
        payload,
        ..
    } = &controller.snapshot().messages[0].payload
    else {
        panic!("expected typed agent message");
    };
    assert_eq!(message_type, "FINAL_ANSWER");
    assert_eq!(task_name, "agent-image");
    assert_eq!(payload, "image done");
    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().current_turn_len(), 0);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
}

/// Verifies consumed subagent mailbox inputs do not account child usage on the parent turn.
/// 验证已消费的智能体 mailbox 输入不会把智能体用量计入父回合。
#[test]
fn mailbox_intervention_keeps_subagent_usage_out_of_parent_turn_stats() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "agent-image",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "intervention-usage-run",
            "execution_status": "completed",
            "result": "image done",
            "subagent_mailbox_event": {
                "kind": "completed",
                "child_thread_id": task_session_id.to_string(),
                "payload": {
                    "task_name": "agent-image",
                    "subagent_task_id": task_session_id.to_string(),
                    "result_text": "image done"
                }
            },
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "agent-image",
                "provider_call_count": 2,
                "tool_call_count": 1,
                "input_tokens": 100,
                "output_tokens": 25,
                "reasoning_tokens": 5,
                "total_tokens": 130,
                "cached_input_tokens": 40
            }]
        }
    });
    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );
    start_webui_session_kernel_user_turn(&mut service_state, parent_session_id, "turn-a")
        .expect("parent turn should start");
    let state = Arc::new(Mutex::new(service_state));
    let mut controller = test_mailbox_controller();

    let appended =
        append_webui_mailbox_intervention_messages(&state, parent_session_id, &mut controller)
            .expect("mailbox intervention should append");

    assert!(appended);
    assert_eq!(controller.snapshot().messages.len(), 1);

    let conversation_state =
        vulcan_agent_runtime::conversation_protocol::SessionConversationState {
            messages: Vec::new(),
            records: vec![
                vulcan_agent_runtime::conversation_protocol::SessionConversationRecord {
                    id: String::from("record-summary-1"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::SummaryOutput,
                    visible: true,
                    title: String::new(),
                    body: String::from("done"),
                    metadata: None,
                    created_at: String::from("1"),
                },
            ],
            continuation: Default::default(),
        };
    let stats =
        vulcan_agent_runtime::turn_stats::build_session_turn_stats_from_state_with_segments(
            &conversation_state,
            "record-summary-1",
            SessionTurnStatsStatus::Completed,
            Some(1),
            SessionTurnStatsKind::UserTurn,
            Vec::new(),
        )
        .expect("parent turn stats should ignore child-agent mailbox usage");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.tool_call_count, 0);
    assert_eq!(stats.input_tokens, 0);
    assert_eq!(stats.output_tokens, 0);
    assert_eq!(stats.reasoning_tokens, 0);
    assert_eq!(stats.total_tokens, 0);
    assert_eq!(stats.cached_input_tokens, None);
}

/// Verifies provider-loop mailbox intervention leaves active wait-scope messages untouched.
/// 验证 Provider 循环 mailbox 干预不会处理仍处于活动等待范围内的消息。
#[test]
fn mailbox_intervention_skips_active_wait_scope() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "execution_id": "intervention-wait-run",
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
    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );
    start_webui_session_kernel_user_turn(&mut service_state, parent_session_id, "turn-a")
        .expect("parent turn should start");
    session_runtime_kernel_mut(&mut service_state, parent_session_id)
        .register_wait_scope(
            "wait-agent",
            [task_session_id.to_string()],
            current_unix_millis_u64(),
        )
        .expect("wait scope should register");
    let state = Arc::new(Mutex::new(service_state));
    let mut controller = test_mailbox_controller();

    let appended =
        append_webui_mailbox_intervention_messages(&state, parent_session_id, &mut controller)
            .expect("mailbox intervention should inspect");

    assert!(!appended);
    assert!(controller.snapshot().messages.is_empty());
    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().current_turn_len(), 1);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
    assert_eq!(kernel.input_queue().consumed_input_count(), 0);
}

/// Verifies active wait scopes suppress automatic mailbox-turn wakeups.
/// 验证活跃等待范围会抑制自动 mailbox 回合唤醒。
#[test]
fn active_wait_scope_suppresses_subagent_mailbox_trigger() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let metadata = serde_json::json!({
        "payload": {
            "execution_id": "active-wait-trigger-run",
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
    session_runtime_kernel_mut(&mut service_state, parent_session_id)
        .register_wait_scope(
            "wait-agent",
            [task_session_id.to_string()],
            current_unix_millis_u64(),
        )
        .expect("wait scope should register");

    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );

    let kernel = service_state
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    let mailbox_inputs = kernel.input_queue().get_mailbox_input();
    assert_eq!(mailbox_inputs.len(), 1);
    assert!(!mailbox_inputs[0].trigger_turn);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
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

    assert!(
        enqueue_webui_subagent_mailbox_input(
            &mut service_state,
            parent_session_id,
            task_session_id,
            &metadata,
        )
        .is_queued()
    );
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
