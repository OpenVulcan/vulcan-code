//! detached subagent tests.
//! detached subagent tests。

use super::*;

/// Captures emitted service events for projection tests.
/// 为投影测试捕获已发出的服务事件。
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

/// Verifies TaskSession terminal reports use the memory-only fast path without completed-turn hooks.
/// 验证 TaskSession 终态报告使用仅内存快路径且不会生成完成回合 hook。
#[test]
fn task_session_execution_report_fast_path_skips_completion_hook() {
    let root_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            root_session_id,
            Some(String::from("project-a")),
            "主会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: root_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    let state = Arc::new(Mutex::new(ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: Some(workspace_snapshot),
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        session_subagent_tasks: BTreeMap::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeSet::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeSet::new(),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    }));
    let report = ExecutionRunReport {
        session_id: task_session_id,
        preparation: None,
        assistant_output: Some(String::from("子代理完成")),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    let applied = apply_task_session_execution_report_to_memory(&state, report).unwrap();

    assert!(applied.completion_request.is_none());
    assert!(!workspace_snapshot_contains_regular_session(
        &applied.workspace_snapshot,
        task_session_id
    ));
    assert_eq!(
        applied
            .workspace_snapshot
            .session_run_phases
            .get(&task_session_id),
        Some(&SessionRunPhase::Ready)
    );
    let messages = applied
        .workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("task session report should materialize messages");
    assert!(
        messages
            .iter()
            .any(|message| message.body.contains("子代理完成"))
    );
    let guard = state.lock().unwrap();
    assert!(
        guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.hydrated_session_ids.contains(&task_session_id))
    );
}

/// Verifies a terminal TaskSession report replaces the provisional prompt row with canonical records.
/// 验证 TaskSession 终态报告会用规范记录替换临时提示词行。
#[test]
fn task_session_terminal_report_replaces_seeded_prompt_with_canonical_records() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let prompt = "Return exactly this JSON only.";
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    {
        let workspace_snapshot = service_state
            .workspace_snapshot
            .as_mut()
            .expect("test workspace snapshot should exist");
        seed_webui_subagent_task_prompt_if_needed(workspace_snapshot, task_session_id, prompt);
    }
    let state = Arc::new(Mutex::new(service_state));
    let report = ExecutionRunReport {
        session_id: task_session_id,
        preparation: None,
        assistant_output: Some(String::from("{\"ok\":true}")),
        thinking_output: None,
        conversation_state: Some(SessionConversationState {
            messages: Vec::new(),
            records: vec![SessionConversationRecord {
                id: String::from("canonical-user-prompt"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: prompt.to_string(),
                metadata: None,
                created_at: String::from("1"),
            }],
            continuation: Default::default(),
        }),
        provider_request_count: 1,
        last_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    let applied = apply_task_session_execution_report_to_memory(&state, report).unwrap();
    let messages = applied
        .workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("task session report should materialize canonical messages");
    let prompt_messages = messages
        .iter()
        .filter(|message| message.kind == SessionMessageKind::User && message.body == prompt)
        .collect::<Vec<_>>();

    assert_eq!(prompt_messages.len(), 1);
    assert_eq!(prompt_messages[0].id, "canonical-user-prompt");
    assert!(
        messages
            .iter()
            .all(|message| !message.id.contains("subagent-prompt"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.kind == SessionMessageKind::Assistant
                && message.body.contains("{\"ok\":true}"))
    );
}

/// Verifies a TaskSession checkpoint replaces provisional prompt rows with runtime records.
/// 验证 TaskSession 检查点会用运行态记录替换临时提示词行。
#[test]
fn task_session_checkpoint_replaces_seeded_prompt_with_runtime_records() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let prompt = "Return exactly this JSON only.";
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    seed_webui_subagent_task_prompt_if_needed(workspace_snapshot, task_session_id, prompt);
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(task_session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                messages: Vec::new(),
                records: vec![SessionConversationRecord {
                    id: String::from("streaming-assistant-output"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::AssistantOutput,
                    visible: true,
                    title: String::from("输出"),
                    body: String::from("{\"ok\":true}"),
                    metadata: None,
                    created_at: String::from("2"),
                }],
                continuation: Default::default(),
            },
            ..SessionRuntimeStateEntry::default()
        });
    let checkpoint_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![SessionConversationRecord {
            id: String::from("canonical-user-prompt"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: prompt.to_string(),
            metadata: None,
            created_at: String::from("1"),
        }],
        continuation: Default::default(),
    };

    apply_checkpoint_conversation_state(runtime_state, checkpoint_state, "3");
    let runtime_state_snapshot = workspace_snapshot
        .session_runtime_state
        .get(&task_session_id)
        .expect("runtime state should exist")
        .clone();
    replace_display_messages_with_runtime_state_records(
        workspace_snapshot,
        task_session_id,
        &runtime_state_snapshot,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("checkpoint records should be visible");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, "canonical-user-prompt");
    assert_eq!(messages[1].id, "streaming-assistant-output");
    assert!(
        messages
            .iter()
            .all(|message| !message.id.contains("subagent-prompt"))
    );
}

/// Verifies subagent surface cache replacement never preserves stale child records.
/// 验证子代理表面缓存替换不会保留陈旧的子记录。
#[test]
fn subagent_surface_conversation_state_replaces_seeded_prompt_without_preserving_stale_records() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let prompt = "Return exactly this JSON only.";
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    seed_webui_subagent_task_prompt_if_needed(workspace_snapshot, task_session_id, prompt);
    workspace_snapshot.session_runtime_state.insert(
        task_session_id,
        SessionRuntimeStateEntry {
            conversation_state: SessionConversationState {
                messages: Vec::new(),
                records: vec![SessionConversationRecord {
                    id: String::from("stale-assistant-output"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::AssistantOutput,
                    visible: true,
                    title: String::from("输出"),
                    body: String::from("stale"),
                    metadata: None,
                    created_at: String::from("1"),
                }],
                continuation: Default::default(),
            },
            ..SessionRuntimeStateEntry::default()
        },
    );
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![
            SessionConversationRecord {
                id: String::from("canonical-user-prompt"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: prompt.to_string(),
                metadata: None,
                created_at: String::from("2"),
            },
            SessionConversationRecord {
                id: String::from("canonical-assistant-output"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("{\"ok\":true}"),
                metadata: None,
                created_at: String::from("3"),
            },
        ],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("canonical child messages should be visible");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, "canonical-user-prompt");
    assert_eq!(messages[1].id, "canonical-assistant-output");
    assert!(
        messages
            .iter()
            .all(|message| !message.id.contains("subagent-prompt") && message.body != "stale")
    );
}

/// Verifies subagent hydration keeps display-only inter-agent prompts while dropping stale records.
/// 验证子智能体水合会保留仅显示的智能体消息，同时丢弃陈旧记录。
#[test]
fn subagent_surface_conversation_state_preserves_display_only_agent_messages() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    let mut display_note = SessionMessageEntry::new(
        SessionMessageKind::User,
        "智能体消息",
        "请用中文重新输出结论。",
    )
    .with_runtime_metadata("display-note-send-message", "1780000000000");
    display_note.record_kind = SessionMessageRecordKind::DisplayNote;
    display_note.metadata = Some(json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": "send_message",
    }));
    let stale_record =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "stale output")
            .with_runtime_metadata("stale-output", "1780000000001");
    workspace_snapshot
        .session_messages
        .insert(task_session_id, vec![display_note.clone(), stale_record]);
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![
            SessionConversationRecord {
                id: String::from("internal-inbox"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: format!(
                    "[Inter-agent messages]\n\nMessage 1 from parent to {task_session_id} via send_message at 1780000000000:\n请用中文重新输出结论。\n"
                ),
                metadata: Some(json!({
                    "source": "user_interrupt",
                    "starts_user_turn": false,
                    "turn_process": true,
                    "editable": false,
                })),
                created_at: String::from("1780000000002"),
            },
            SessionConversationRecord {
                id: String::from("canonical-assistant-output"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("已按中文重新输出。"),
                metadata: None,
                created_at: String::from("1780000000003"),
            },
        ],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("hydrated child messages should exist");
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().any(|message| {
        message.record_kind == SessionMessageRecordKind::DisplayNote
            && message.body == "请用中文重新输出结论。"
    }));
    assert!(messages.iter().any(|message| {
        message.record_kind == SessionMessageRecordKind::AssistantOutput
            && message.body == "已按中文重新输出。"
    }));
    assert!(!messages.iter().any(|message| {
        message.body.contains("[Inter-agent messages]") || message.body == "stale output"
    }));
}

/// Verifies a live detached listener keeps the parent subagent navigation visible after the parent stream finishes.
/// 验证父级流结束后，实时分离监听器仍会让父级子代理导航保持可见。
#[test]
fn webui_detached_listener_keeps_running_subagent_navigation_visible() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_tasks: BTreeMap::from([(
            parent_session_id,
            BTreeMap::from([(
                task_session_id,
                SubagentTaskPanelState {
                    task_id: task_session_id,
                    public_task_id: String::from("image-worker"),
                    parent_session_id,
                    subagent_label: String::from("图片子代理"),
                    description: Some(String::from("生成图片")),
                    run_state: SubagentTaskRunState::Running,
                    result_text: String::new(),
                    conversation_state: None,
                },
            )]),
        )]),
        session_subagent_navigation_rounds: BTreeSet::from([parent_session_id]),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeSet::from([task_session_id]),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    };
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(String::from("project-a")),
            "父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(parent_session_id, SessionRunPhase::Ready)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot)
        .expect("running subagent navigation should stay visible");

    assert_eq!(
        navigation.parent_conversation_id,
        parent_session_id.to_string()
    );
    assert_eq!(
        navigation.active_conversation_id,
        parent_session_id.to_string()
    );
    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
    assert!(navigation.tasks[0].is_running);
    assert!(session_has_running_subagent_tasks_from_cache(
        &mut guard,
        &workspace_snapshot,
        parent_session_id
    ));
}

/// Verifies terminal listener cleanup immediately emits a non-running subagent navigation delta.
/// 验证终态监听器清理后会立即发出非运行中的子代理导航增量。
#[test]
fn terminal_listener_cleanup_emits_non_running_subagent_navigation_delta() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    {
        let mut guard = state.lock().unwrap();
        guard
            .session_subagent_tasks
            .entry(parent_session_id)
            .or_default()
            .insert(
                task_session_id,
                SubagentTaskPanelState {
                    task_id: task_session_id,
                    public_task_id: String::from("image-worker"),
                    parent_session_id,
                    subagent_label: String::from("图片子代理"),
                    description: Some(String::from("生成图片")),
                    run_state: SubagentTaskRunState::Running,
                    result_text: String::from("图片生成完成"),
                    conversation_state: Some(SessionConversationState {
                        messages: vec![SessionConversationMessage {
                            id: String::from("child-assistant-1"),
                            payload: SessionConversationPayload::Assistant {
                                text: String::from("图片生成完成"),
                                reasoning: String::new(),
                                provider_message_id: None,
                                provider_message_state: None,
                                usage: None,
                                tool_calls: Vec::new(),
                            },
                        }],
                        ..SessionConversationState::default()
                    }),
                },
            );
        guard
            .session_subagent_navigation_rounds
            .insert(parent_session_id);
        guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
            parent_session_id,
            task_id: task_session_id,
        });
        guard
            .detached_subagent_task_listeners
            .insert(task_session_id);
    }
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
        events: Arc::clone(&emitted_events),
    })]);

    {
        let mut guard = state.lock().unwrap();
        guard
            .detached_subagent_task_listeners
            .remove(&task_session_id);
    }
    refresh_webui_subagent_navigation_round_visibility(&state, parent_session_id)
        .expect("terminal cleanup should refresh navigation visibility");
    let navigation = emit_webui_subagent_navigation_changed_delta(&app, &state)
        .expect("terminal cleanup should emit navigation")
        .expect("active subagent panel should keep navigation mounted");

    assert_eq!(navigation.tasks.len(), 1);
    assert_eq!(navigation.tasks[0].task_id, task_session_id.to_string());
    assert_eq!(navigation.tasks[0].run_state, "completed");
    assert!(!navigation.tasks[0].is_running);
    let events = emitted_events.lock().expect("event sink lock");
    assert!(events.iter().any(|event| matches!(
        event,
        ServiceEvent::WorkbenchDelta(envelope)
            if matches!(
                &envelope.delta,
                WorkbenchDelta::SubagentNavigationChanged(delta)
                    if delta
                        .subagent_navigation
                        .as_ref()
                        .and_then(|navigation| navigation.tasks.first())
                        .is_some_and(|task| !task.is_running && task.run_state == "completed")
            )
    )));
}

/// Verifies empty subagent navigation deltas still identify the parent conversation to clear stale UI cache.
/// 验证空子智能体导航增量仍携带父会话标识，以清理前端旧缓存。
#[test]
fn empty_subagent_navigation_delta_preserves_parent_conversation_id() {
    let parent_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
        events: Arc::clone(&emitted_events),
    })]);

    let navigation = emit_webui_subagent_navigation_changed_delta(&app, &state)
        .expect("empty navigation should emit");

    assert!(navigation.is_none());
    let expected_parent_conversation_id = parent_session_id.to_string();
    let events = emitted_events.lock().expect("event sink lock");
    assert!(events.iter().any(|event| matches!(
        event,
        ServiceEvent::WorkbenchDelta(envelope)
            if matches!(
                &envelope.delta,
                WorkbenchDelta::SubagentNavigationChanged(delta)
                    if delta.parent_conversation_id.as_deref()
                        == Some(expected_parent_conversation_id.as_str())
                        && delta.subagent_navigation.is_none()
            )
    )));
}

/// Verifies child task tool events can patch known subagent transcripts even when the parent is active.
/// 验证即使父会话处于活动界面，子任务工具事件也允许修补已知子代理转录。
#[test]
fn child_runtime_tool_event_patch_gate_accepts_known_subagent_transcript() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    guard
        .session_subagent_tasks
        .entry(parent_session_id)
        .or_default()
        .insert(
            task_session_id,
            SubagentTaskPanelState {
                task_id: task_session_id,
                public_task_id: String::from("code-review"),
                parent_session_id,
                subagent_label: String::from("代码审核"),
                description: Some(String::from("审核代码")),
                run_state: SubagentTaskRunState::Running,
                result_text: String::new(),
                conversation_state: None,
            },
        );

    assert!(workbench_should_emit_runtime_event_message_patch(
        &guard,
        &task_session_id.to_string(),
        task_session_id
    ));
    assert!(!workbench_should_emit_runtime_event_message_patch(
        &guard,
        &SessionId::new_v4().to_string(),
        SessionId::new_v4()
    ));
}

/// Verifies two detached WebUI subagents stop blocking the parent turn after both reach terminal states.
/// 验证两个 WebUI 分离子代理全部进入终态后不再阻塞父级回合收尾。
#[test]
fn webui_completion_gate_clears_after_two_detached_subagents_finish() {
    let parent_session_id = SessionId::new_v4();
    let first_task_session_id = SessionId::new_v4();
    let second_task_session_id = SessionId::new_v4();
    let mut guard = ServiceBridgeState {
        shell_host: test_service_shell_host(),
        startup: startup_snapshot_with_phase("ready", Vec::new(), &[]),
        workbench: fallback_workbench_snapshot(&[]),
        workbench_generation: 0,
        workspace_snapshot: None,
        hydrated_conversation_windows: BTreeMap::new(),
        agent_runtime: None,
        service_runtime: None,
        webui_service_discovery: None,
        controller_runtime: None,
        luaskills_runtime: None,
        cached_luaskills_catalog: None,
        token_budget_runtime: None,
        mcp_runtime: None,
        cached_mcp_catalog: None,
        live_context_budgets: BTreeMap::new(),
        session_goal_summaries: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_tasks: BTreeMap::from([(
            parent_session_id,
            BTreeMap::from([
                (
                    first_task_session_id,
                    SubagentTaskPanelState {
                        task_id: first_task_session_id,
                        public_task_id: String::from("image-worker-a"),
                        parent_session_id,
                        subagent_label: String::from("图片子代理 A"),
                        description: Some(String::from("生成第一张图片")),
                        run_state: SubagentTaskRunState::Running,
                        result_text: String::new(),
                        conversation_state: None,
                    },
                ),
                (
                    second_task_session_id,
                    SubagentTaskPanelState {
                        task_id: second_task_session_id,
                        public_task_id: String::from("image-worker-b"),
                        parent_session_id,
                        subagent_label: String::from("图片子代理 B"),
                        description: Some(String::from("生成第二张图片")),
                        run_state: SubagentTaskRunState::Running,
                        result_text: String::new(),
                        conversation_state: None,
                    },
                ),
            ]),
        )]),
        session_subagent_navigation_rounds: BTreeSet::from([parent_session_id]),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeSet::from([
            first_task_session_id,
            second_task_session_id,
        ]),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        pending_subagent_usage_segments: BTreeMap::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_persist_versions: BTreeMap::new(),
        session_transcript_persist_versions: BTreeMap::new(),
        session_addon_persist_versions: BTreeMap::new(),
    };
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            parent_session_id,
            Some(String::from("project-a")),
            "父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(parent_session_id, SessionRunPhase::Ready)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let running_tasks = running_webui_subagent_task_states_for_tree_from_cache(
        &mut guard,
        &workspace_snapshot,
        parent_session_id,
    );
    assert_eq!(running_tasks.len(), 2);
    let running_gate = WebuiTurnCompletionGate {
        pending_subagent_count: running_tasks.len(),
        has_active_hook_turn: false,
    };
    assert!(matches!(
        running_gate.completion_status(),
        ConversationTurnCompletionStatus::AwaitingSubagents
    ));

    for task_session_id in [first_task_session_id, second_task_session_id] {
        let task_state = guard
            .session_subagent_tasks
            .get_mut(&parent_session_id)
            .and_then(|tasks| tasks.get_mut(&task_session_id))
            .expect("test subagent state should exist");
        task_state.run_state = SubagentTaskRunState::Completed;
        task_state.result_text = format!("任务 {task_session_id} 已完成");
        guard
            .detached_subagent_task_listeners
            .remove(&task_session_id);
    }

    let finished_tasks = running_webui_subagent_task_states_for_tree_from_cache(
        &mut guard,
        &workspace_snapshot,
        parent_session_id,
    );
    assert!(finished_tasks.is_empty());
    assert!(!session_has_running_subagent_tasks_from_cache(
        &mut guard,
        &workspace_snapshot,
        parent_session_id
    ));
    let finished_gate = WebuiTurnCompletionGate {
        pending_subagent_count: finished_tasks.len(),
        has_active_hook_turn: false,
    };
    assert!(matches!(
        finished_gate.completion_status(),
        ConversationTurnCompletionStatus::Completed
    ));
}

/// Verifies delegate-finished events force stale running payloads into a terminal panel state.
/// 验证 delegate-finished 事件会将陈旧 running 载荷强制归一为终态面板状态。
#[test]
fn delegate_finished_ingest_terminalizes_stale_running_payload() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    {
        let mut guard = state.lock().unwrap();
        guard
            .detached_subagent_task_listeners
            .insert(task_session_id);
    }
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "子代理完成",
        "子代理任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "stale-running-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_status": "running",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": "已完成但状态载荷陈旧",
        }
    })));

    let consumed =
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest");

    assert!(consumed);
    let guard = state.lock().unwrap();
    let task_state = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .and_then(|tasks| tasks.get(&task_session_id))
        .expect("terminal task state should be cached");
    assert_eq!(task_state.run_state, SubagentTaskRunState::Completed);
    assert!(
        !guard
            .detached_subagent_task_listeners
            .contains(&task_session_id)
    );
}

/// Verifies close-agent cleanup removes closed tasks while preserving other unclosed agents.
/// 验证关闭代理清理会移除已关闭任务，同时保留其他未关闭代理。
#[test]
fn close_agent_cleanup_removes_closed_webui_subagent_cache_entries() {
    let parent_session_id = SessionId::new_v4();
    let closed_task_id = SessionId::new_v4();
    let retained_task_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let closed_state = test_completed_subagent_panel_state(parent_session_id, closed_task_id);
    let retained_state = test_completed_subagent_panel_state(parent_session_id, retained_task_id);
    guard.session_subagent_tasks.insert(
        parent_session_id,
        BTreeMap::from([
            (closed_task_id, closed_state.clone()),
            (retained_task_id, retained_state),
        ]),
    );
    guard.subagent_surface_detail_cache.insert(
        closed_task_id,
        WebuiSubagentSurfaceDetailCacheEntry {
            parent_session_id,
            task_state: closed_state,
            messages: Vec::new(),
            last_access_ms: current_unix_millis_value(),
        },
    );
    guard
        .detached_subagent_task_listeners
        .insert(closed_task_id);
    guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id,
        task_id: closed_task_id,
    });

    assert!(remove_webui_closed_subagent_tasks_in_state(
        &mut guard,
        &[closed_task_id]
    ));

    let tasks = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .expect("retained task bucket should remain");
    assert!(!tasks.contains_key(&closed_task_id));
    assert!(tasks.contains_key(&retained_task_id));
    assert!(
        !guard
            .subagent_surface_detail_cache
            .contains_key(&closed_task_id)
    );
    assert!(
        !guard
            .detached_subagent_task_listeners
            .contains(&closed_task_id)
    );
    assert!(guard.active_subagent_panel.is_none());
}

/// Builds one completed subagent panel state for cache mutation tests.
/// 为缓存变更测试构建一条已完成子代理面板状态。
fn test_completed_subagent_panel_state(
    parent_session_id: SessionId,
    task_id: SessionId,
) -> SubagentTaskPanelState {
    SubagentTaskPanelState {
        task_id,
        public_task_id: task_id.to_string(),
        parent_session_id,
        subagent_label: String::from("Review Worker"),
        description: Some(String::from("审核代码")),
        run_state: SubagentTaskRunState::Completed,
        result_text: String::from("审核完成"),
        conversation_state: None,
    }
}

/// Verifies the final child completion closes a parent hook turn that was awaiting subagents.
/// 验证最后一个子代理完成后会闭合等待子代理的父级 hook 回合。
#[test]
fn final_subagent_completion_closes_parent_awaiting_hook_turn() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let runtime = test_agent_runtime_bridge();
    {
        let mut guard = state.lock().unwrap();
        guard
            .session_subagent_tasks
            .entry(parent_session_id)
            .or_default()
            .insert(
                task_session_id,
                SubagentTaskPanelState {
                    task_id: task_session_id,
                    public_task_id: String::from("review-worker"),
                    parent_session_id,
                    subagent_label: String::from("Review Worker"),
                    description: Some(String::from("审核代码")),
                    run_state: SubagentTaskRunState::Completed,
                    result_text: String::from("审核完成"),
                    conversation_state: None,
                },
            );
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace should exist");
        let runtime_state = workspace
            .session_runtime_state
            .entry(parent_session_id)
            .or_insert_with(SessionRuntimeStateEntry::default);
        runtime_state.last_safe_provider_request_count = Some(2);
        runtime_state
            .conversation_state
            .messages
            .push(SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: SessionConversationPayload::Assistant {
                    text: String::from("父代理已等待子代理结果。"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            });
    }
    {
        let mut hook_runtime = runtime.conversation_hook_runtime.lock().unwrap();
        hook_runtime.record_queued_user_prompt(
            parent_session_id,
            "prompt-1",
            Some(String::from("project-a")),
            String::from("D:\\openvulcan\\vulcan-code"),
            PendingSessionPromptSource::UserFollowup,
            "请调用子代理审核代码",
        );
        assert!(hook_runtime.has_active_turn(parent_session_id));
    }

    let completion_request = maybe_prepare_webui_parent_completion_after_subagent_finished(
        &state,
        &runtime,
        parent_session_id,
    )
    .expect("completion preparation should not fail");

    assert!(completion_request.is_some());
    assert!(
        !runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .has_active_turn(parent_session_id)
    );
}

/// Verifies stopped parents never dispatch queued work when detached subagents report terminal events.
/// 验证已停止父会话收到分离子代理终态事件时绝不会调度排队工作。
#[test]
fn stopped_detached_parent_event_skips_pending_prompt_dispatch() {
    assert!(webui_detached_parent_event_should_continue_pending_prompt(
        true, false
    ));
    assert!(!webui_detached_parent_event_should_continue_pending_prompt(
        true, true
    ));
    assert!(!webui_detached_parent_event_should_continue_pending_prompt(
        false, false
    ));
    assert!(webui_detached_parent_event_should_persist_terminal_projection(true, true, false));
    assert!(!webui_detached_parent_event_should_persist_terminal_projection(true, true, true));
    assert!(!webui_detached_parent_event_should_persist_terminal_projection(false, true, false));
}

/// Verifies send_message-resumed child work no longer blocks the parent completion gate.
/// 验证由 send_message 续跑的子任务不会继续阻塞父级完成闸门。
#[test]
fn completion_gate_ignores_send_message_resumed_child_task() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let runtime = test_agent_runtime_bridge();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    {
        let mut guard = state.lock().expect("state lock");
        guard
            .detached_subagent_task_listeners
            .insert(task_session_id);
        guard.session_subagent_tasks.insert(
            parent_session_id,
            BTreeMap::from([(
                task_session_id,
                SubagentTaskPanelState {
                    task_id: task_session_id,
                    public_task_id: String::from("reviewer"),
                    parent_session_id,
                    subagent_label: String::from("Reviewer"),
                    description: Some(String::from("Review code")),
                    run_state: SubagentTaskRunState::Running,
                    result_text: String::new(),
                    conversation_state: None,
                },
            )]),
        );
    }

    let default_gate = webui_turn_completion_gate_from_state(&state, &runtime, parent_session_id)
        .expect("default gate should read");
    assert_eq!(default_gate.pending_subagent_count, 1);

    {
        let mut guard = state.lock().expect("state lock");
        mark_webui_subagent_completion_gate_ignored(&mut guard, task_session_id);
    }

    let ignored_gate = webui_turn_completion_gate_from_state(&state, &runtime, parent_session_id)
        .expect("ignored gate should read");
    assert_eq!(ignored_gate.pending_subagent_count, 0);
}

/// Verifies live transcript persistence versioning does not invalidate session persistence.
/// 验证运行中转录持久化版本不会使会话持久化版本失效。
#[test]
fn live_transcript_persist_version_is_independent_from_session_persist_version() {
    // session_id identifies the session whose persistence lanes are compared.
    // session_id 标识需要比较持久化通道的会话。
    let session_id = SessionId::new_v4();
    // state carries independent version tables for session and live transcript writes.
    // state 携带会话写入与运行中转录写入的独立版本表。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        session_id,
    )));

    // session_version belongs to the full session/runtime persistence lane.
    // session_version 属于完整会话与运行态持久化通道。
    let session_version = next_webui_session_persist_version(&state, session_id)
        .expect("session persist version should allocate");
    // transcript_version belongs to the live transcript-only persistence lane.
    // transcript_version 属于仅运行中转录持久化通道。
    let transcript_version = next_webui_session_transcript_persist_version(&state, session_id)
        .expect("transcript persist version should allocate");

    assert!(webui_session_persist_version_is_current(
        &state,
        session_id,
        session_version
    ));
    assert!(webui_session_transcript_persist_version_is_current(
        &state,
        session_id,
        transcript_version
    ));
    assert!(!webui_session_persist_version_is_current(
        &state,
        session_id,
        session_version.saturating_add(1)
    ));
}

/// Verifies detached subagent completion enqueues parent mailbox input without using the pending prompt queue.
/// 验证没有 wait_agent 接管时，分离子代理完成会写入父级 mailbox 而不使用 pending prompt 队列。
#[test]
fn subagent_completion_queues_session_mailbox_without_wait_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let event = test_delegate_finished_event(task_session_id, "image-a", "生成完成：a.jpg");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("enqueue mailbox input");

    assert!(queued);
    let mailbox_prompt = webui_session_mailbox_input_from_kernel(
        &state,
        parent_session_id,
        Some(&task_session_id.to_string()),
    )
    .expect("mailbox prompt should read")
    .expect("mailbox prompt should exist");
    assert!(mailbox_prompt.contains("生成完成：a.jpg"));
    assert!(mailbox_prompt.contains("<message_type>FINAL_ANSWER</message_type>"));
    assert!(!mailbox_prompt.contains("<subagent_mailbox_event>"));
    let guard = state.lock().expect("state lock");
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
    assert!(runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry)));
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("parent session kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    let mailbox_inputs = kernel.input_queue().get_mailbox_input();
    assert_eq!(mailbox_inputs.len(), 1);
    assert_eq!(mailbox_inputs[0].kind, TurnInputKind::SubagentMessage);
    assert!(mailbox_inputs[0].trigger_turn);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
    assert_eq!(
        mailbox_inputs[0].payload["mailbox_event"]["payload"]["result"],
        "生成完成：a.jpg"
    );
}

/// Verifies detached subagent completion still enqueues mailbox input while wait_agent is active.
/// 验证 wait_agent 活跃期间，分离子代理完成仍会加入 mailbox 输入。
#[test]
fn subagent_completion_queues_session_mailbox_during_wait_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .parent_sessions_waiting_for_subagents
        .insert(parent_session_id);
    let state = Arc::new(Mutex::new(service_state));
    let event = test_delegate_finished_event(task_session_id, "image-b", "生成完成：b.jpg");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("queue mailbox input");

    assert!(queued);
    let guard = state.lock().expect("state lock");
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
    assert!(runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry)));
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("parent session kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(!kernel.input_queue().has_trigger_turn_mailbox_items());
}

/// Verifies child completion mailbox input queues after wait_agent coordination ends.
/// 验证 wait_agent 协调结束后，子代理完成 mailbox 输入仍会入队。
#[test]
fn subagent_completion_queues_session_mailbox_after_completed_wait_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let event = test_delegate_finished_event(task_session_id, "image-c", "生成完成：c.jpg");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("mailbox input should queue");

    assert!(queued);
    let guard = state.lock().expect("state lock");
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
    assert!(runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry)));
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("parent session kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
}

/// Verifies child completion mailbox input no longer depends on legacy wait-agent selectors.
/// 验证子代理完成 mailbox 输入不再依赖旧 wait_agent 选择器。
#[test]
fn subagent_completion_after_completed_wait_agent_allows_any_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let event = test_delegate_finished_event(task_session_id, "agent-unmatched", "done");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("unmatched child should enqueue mailbox input");

    assert!(queued);
    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("kernel should exist");
    assert_eq!(kernel.input_queue().mailbox_len(), 1);
    assert!(kernel.input_queue().has_trigger_turn_mailbox_items());
}

/// Verifies child-agent usage segments are drained once before parent turn statistics aggregate them.
/// 验证子代理用量分段会在父会话回合统计聚合前被一次性取走。
#[test]
fn pending_subagent_usage_segments_drain_once_and_merge_into_turn_stats() {
    let parent_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let mut segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::SubagentTask,
        1,
        Some(&vulcan_core::types::ProviderUsageSnapshot {
            input_tokens: Some(100),
            output_tokens: Some(25),
            reasoning_tokens: Some(5),
            total_tokens: Some(130),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
        }),
    );
    segment.trigger = Some(String::from("output-1-100"));
    service_state
        .pending_subagent_usage_segments
        .insert(parent_session_id, vec![segment]);
    let state = Arc::new(Mutex::new(service_state));

    let first = take_pending_webui_subagent_usage_segments(&state, parent_session_id)
        .expect("pending usage segments should drain");
    let second = take_pending_webui_subagent_usage_segments(&state, parent_session_id)
        .expect("pending usage segments should stay drained");

    assert_eq!(first.len(), 1);
    assert!(second.is_empty());

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
    let stats = latest_session_turn_stats_from_state(&conversation_state, Some(0), first)
        .expect("explicit subagent usage should produce turn stats");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 100);
    assert_eq!(stats.output_tokens, 25);
    assert_eq!(stats.reasoning_tokens, 5);
    assert_eq!(stats.total_tokens, 130);
    assert_eq!(stats.cached_input_tokens, Some(40));
    assert_eq!(goal_token_delta_for_turn_stats(&stats), Some(90));
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::SubagentTask
    );
}
