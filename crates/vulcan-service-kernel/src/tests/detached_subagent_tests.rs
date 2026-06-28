//! detached subagent tests.
//! detached subagent tests。

use super::*;

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
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        parent_session_completed_wait_agent_selectors: BTreeMap::new(),
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
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        parent_session_completed_wait_agent_selectors: BTreeMap::new(),
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
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        parent_session_completed_wait_agent_selectors: BTreeMap::new(),
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
    assert_eq!(
        mailbox_inputs[0].payload["mailbox_event"]["payload"]["result"],
        "生成完成：a.jpg"
    );
}

/// Verifies detached subagent completion does not enqueue mailbox input while wait_agent is active.
/// 验证 wait_agent 活跃期间，分离子代理完成不会额外加入 mailbox 输入。
#[test]
fn subagent_completion_skips_session_mailbox_during_wait_agent() {
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
    .expect("skip mailbox input");

    assert!(!queued);
    let guard = state.lock().expect("state lock");
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
    assert!(runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry)));
}

/// Verifies child completion mailbox input stays suppressed after a completed wait_agent result.
/// 验证 wait_agent 已完成收集后，子代理完成 mailbox 输入仍保持抑制。
#[test]
fn subagent_completion_skips_session_mailbox_after_completed_wait_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .parent_session_completed_wait_agent_selectors
        .insert(parent_session_id, BTreeSet::new());
    let state = Arc::new(Mutex::new(service_state));
    let event = test_delegate_finished_event(task_session_id, "image-c", "生成完成：c.jpg");

    let queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &event,
        true,
        false,
    )
    .expect("skip mailbox input");

    assert!(!queued);
    let guard = state.lock().expect("state lock");
    let runtime_state = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&parent_session_id));
    assert!(runtime_state.is_none_or(|entry| !runtime_state_has_queued_pending_prompt_work(entry)));
}

/// Verifies a completed wait_agent only suppresses matching child-agent completions.
/// 验证已完成的 wait_agent 只抑制匹配的子代理完成事件。
#[test]
fn subagent_completion_after_completed_wait_agent_allows_unmatched_agent() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .parent_session_completed_wait_agent_selectors
        .insert(
            parent_session_id,
            BTreeSet::from([String::from("agent-waited")]),
        );
    let state = Arc::new(Mutex::new(service_state));
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
