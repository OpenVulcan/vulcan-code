//! detached subagent tests.
//! detached subagent tests。

use super::*;
use vulcan_workbench_model::SessionTurnUsageGroupKind;

/// Returns rebuilt subagent navigation and fails the test on registry projection errors.
/// 返回重建后的智能体导航，并在注册表投影错误时让测试失败。
fn subagent_navigation_snapshot_from_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
) -> Option<SubagentNavigationSnapshot> {
    super::subagent_navigation_snapshot_from_state(guard, workspace_snapshot)
        .expect("subagent navigation projection should succeed")
}

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
        session_goal_dispatch_versions: BTreeMap::new(),
        budget_limit_reported_goal_ids: BTreeSet::new(),
        webui_preloaded_unclosed_subagent_roots: BTreeSet::new(),
        subagent_surface_detail_cache: BTreeMap::new(),
        session_subagent_navigation_rounds: BTreeSet::new(),
        active_subagent_panel: None,
        detached_subagent_task_listeners: BTreeMap::new(),
        subagent_parent_turn_bindings: BTreeMap::new(),
        subagent_completion_gate_ignored_tasks: BTreeSet::new(),
        parent_sessions_waiting_for_subagents: BTreeSet::new(),
        active_stream_tasks: BTreeMap::new(),
        session_runtime_kernels: BTreeMap::new(),
        session_turn_admission_reservations: BTreeMap::new(),
        project_lifecycle_transition_reservations: BTreeMap::new(),
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
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    let applied = apply_task_session_execution_report_to_memory(&state, report)
        .expect("terminal task-session report should apply to memory");

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
    let guard = state
        .lock()
        .expect("service state lock should be available after report apply");
    assert!(
        guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.hydrated_session_ids.contains(&task_session_id))
    );
}

/// Verifies TaskSession terminal reports project provider usage into live panel turn stats.
/// 验证智能体终态报告会把 Provider 用量投影到实时面板轮次统计。
#[test]
fn task_session_execution_report_fast_path_projects_live_usage_stats() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let provider_usage = ProviderUsageSnapshot {
        input_tokens: Some(31_295),
        output_tokens: Some(16),
        reasoning_tokens: Some(14),
        total_tokens: Some(31_311),
        cached_input_tokens: Some(5_888),
        cache_creation_input_tokens: None,
    };
    let report = ExecutionRunReport {
        session_id: task_session_id,
        preparation: None,
        assistant_output: Some(String::from("abcd")),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 1,
        last_successful_provider_usage: Some(provider_usage.clone()),
        cumulative_successful_provider_usage: Some(provider_usage),
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    let applied = apply_task_session_execution_report_to_memory(&state, report)
        .expect("terminal task-session report should apply live usage stats");
    let messages = applied
        .workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("task session report should materialize messages");
    let stats_row = messages
        .iter()
        .rev()
        .find(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("task session should show one turn stats row");
    let stats: SessionTurnStats = serde_json::from_value(
        stats_row
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("turn_stats"))
            .expect("turn stats metadata should exist")
            .clone(),
    )
    .expect("turn stats metadata should parse");

    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 31_295);
    assert_eq!(stats.output_tokens, 16);
    assert_eq!(stats.reasoning_tokens, 14);
    assert_eq!(stats.total_tokens, 31_311);
    assert_eq!(stats.cached_input_tokens, Some(5_888));
    assert_eq!(stats.usage_segments.len(), 1);
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
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
        workspace_snapshot.session_runtime_state.insert(
            task_session_id,
            SessionRuntimeStateEntry {
                conversation_state: SessionConversationState {
                    messages: Vec::new(),
                    records: vec![
                        SessionConversationRecord {
                            id: String::from("inherited-parent-user"),
                            kind: SessionMessageKind::User,
                            record_kind: SessionMessageRecordKind::Transcript,
                            visible: true,
                            title: String::from("用户指令"),
                            body: String::from("Parent visible prompt must stay out."),
                            metadata: None,
                            created_at: String::from("0"),
                        },
                        SessionConversationRecord {
                            id: String::from("inherited-parent-output"),
                            kind: SessionMessageKind::Assistant,
                            record_kind: SessionMessageRecordKind::AssistantOutput,
                            visible: true,
                            title: String::from("输出"),
                            body: String::from("Parent visible output must stay out."),
                            metadata: None,
                            created_at: String::from("0"),
                        },
                        SessionConversationRecord {
                            id: String::from("inherited-parent-stats"),
                            kind: SessionMessageKind::Assistant,
                            record_kind: SessionMessageRecordKind::TurnStats,
                            visible: true,
                            title: String::from("Turn Stats · completed"),
                            body: String::from("Parent visible stats must stay out."),
                            metadata: None,
                            created_at: String::from("0"),
                        },
                    ],
                    continuation: Default::default(),
                },
                ..SessionRuntimeStateEntry::default()
            },
        );
    }
    let state = Arc::new(Mutex::new(service_state));
    let report = ExecutionRunReport {
        session_id: task_session_id,
        preparation: None,
        assistant_output: Some(String::from("{\"ok\":true}")),
        thinking_output: None,
        conversation_state: Some(SessionConversationState {
            messages: Vec::new(),
            records: vec![
                // hidden-terminal-runtime-record represents a runtime-only row that must not enter the display cache.
                // hidden-terminal-runtime-record 表示不得进入显示缓存的仅运行态记录。
                SessionConversationRecord {
                    id: String::from("hidden-terminal-runtime-record"),
                    kind: SessionMessageKind::Assistant,
                    record_kind: SessionMessageRecordKind::SummaryOutput,
                    visible: false,
                    title: String::from("内部摘要"),
                    body: String::from("hidden terminal runtime record"),
                    metadata: None,
                    created_at: String::from("0"),
                },
                SessionConversationRecord {
                    id: String::from("canonical-user-prompt"),
                    kind: SessionMessageKind::User,
                    record_kind: SessionMessageRecordKind::Transcript,
                    visible: true,
                    title: String::from("用户指令"),
                    body: prompt.to_string(),
                    metadata: None,
                    created_at: String::from("1"),
                },
            ],
            continuation: Default::default(),
        }),
        provider_request_count: 1,
        last_successful_provider_usage: None,
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    let applied = apply_task_session_execution_report_to_memory(&state, report)
        .expect("terminal task-session report should replace provisional prompt");
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
            .all(|message| message.id != "hidden-terminal-runtime-record")
    );
    assert!(
        messages
            .iter()
            .all(|message| !message.body.contains("hidden terminal runtime record"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.kind == SessionMessageKind::Assistant
                && message.body.contains("{\"ok\":true}"))
    );
    assert!(messages.iter().all(|message| {
        !message
            .body
            .contains("Parent visible prompt must stay out.")
            && !message
                .body
                .contains("Parent visible output must stay out.")
            && !message.body.contains("Parent visible stats must stay out.")
    }));
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
            .count(),
        1
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

    apply_checkpoint_conversation_state(runtime_state, checkpoint_state, "3")
        .expect("valid checkpoint conversation identities");
    let runtime_state_snapshot = workspace_snapshot
        .session_runtime_state
        .get(&task_session_id)
        .expect("runtime state should exist")
        .clone();
    replace_task_display_messages_with_runtime_state_records(
        workspace_snapshot,
        task_session_id,
        &runtime_state_snapshot,
        Some(prompt),
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
                id: String::from("hydrated-parent-user"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: String::from("Hydrated parent prompt must stay out."),
                metadata: None,
                created_at: String::from("1"),
            },
            SessionConversationRecord {
                id: String::from("hydrated-parent-stats"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::TurnStats,
                visible: true,
                title: String::from("Turn Stats · completed"),
                body: String::from("Hydrated parent stats must stay out."),
                metadata: None,
                created_at: String::from("1"),
            },
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
        Some(prompt),
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
    assert!(messages.iter().all(|message| {
        !message
            .body
            .contains("Hydrated parent prompt must stay out.")
            && !message
                .body
                .contains("Hydrated parent stats must stay out.")
    }));
}

/// Verifies anchorless cloned child state starts display at the latest child task prompt.
/// 验证缺少锚点的克隆子状态会从最新子任务提示开始显示。
#[test]
fn subagent_surface_conversation_state_without_anchor_drops_inherited_parent_history() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![
            SessionConversationRecord {
                id: String::from("cloned-parent-user"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: String::from("Parent prompt cloned into runtime only."),
                metadata: None,
                created_at: String::from("1"),
            },
            SessionConversationRecord {
                id: String::from("cloned-parent-assistant"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("Parent answer must not be shown in child panel."),
                metadata: None,
                created_at: String::from("2"),
            },
            SessionConversationRecord {
                id: String::from("latest-child-prompt"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户指令"),
                body: String::from(
                    "Recall inherited runtime context, but do not show parent chat.",
                ),
                metadata: None,
                created_at: String::from("3"),
            },
            SessionConversationRecord {
                id: String::from("latest-child-output"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("Only child-owned display records remain."),
                metadata: None,
                created_at: String::from("4"),
            },
        ],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
        None,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("scoped child messages should exist");
    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["latest-child-prompt", "latest-child-output"]
    );
    assert!(messages.iter().all(|message| {
        !message.body.contains("Parent prompt") && !message.body.contains("Parent answer")
    }));
}

/// Verifies subagent hydration uses canonical display-only records while dropping stale records.
/// 验证子智能体水合使用规范仅显示记录，同时丢弃陈旧记录。
#[test]
fn subagent_surface_conversation_state_uses_canonical_display_only_agent_messages() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    let stale_record =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "stale output")
            .with_runtime_metadata("stale-output", "1780000000001");
    workspace_snapshot
        .session_messages
        .insert(task_session_id, vec![stale_record]);
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![
            SessionConversationRecord {
                id: String::from("display-note-send-message"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::DisplayNote,
                visible: true,
                title: String::from("智能体消息"),
                body: String::from("请用中文重新输出结论。"),
                metadata: Some(json!({
                    "display_note": true,
                    "kind": "subagent_message",
                    "tool": "send_message",
                    "delivery_id": "display-note-send-message",
                })),
                created_at: String::from("1780000000000"),
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
        None,
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
    assert!(
        !messages
            .iter()
            .any(|message| message.body == "stale output")
    );
}

/// Verifies runtime refresh drops stale parent-to-child visible message notes.
/// 验证运行态刷新会丢弃陈旧的父到子可见消息记录。
#[test]
fn subagent_surface_conversation_state_drops_stale_send_message_notes() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    let mut send_note = SessionMessageEntry::new(
        SessionMessageKind::User,
        "智能体消息",
        "你之前做了什么？请回忆并列出。",
    )
    .with_runtime_metadata("delivery-followup-1", "1780000000001");
    send_note.record_kind = SessionMessageRecordKind::DisplayNote;
    send_note.metadata = Some(json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": "send_message",
        (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
        "delivery_id": "delivery-followup-1",
    }));
    let stale_record =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "stale output")
            .with_runtime_metadata("stale-output", "1780000000000");
    workspace_snapshot
        .session_messages
        .insert(task_session_id, vec![stale_record, send_note]);
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![SessionConversationRecord {
            id: String::from("canonical-assistant-output"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: String::from("我只输出过 abcd。"),
            metadata: None,
            created_at: String::from("1780000000003"),
        }],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
        None,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("hydrated child messages should exist");
    assert_eq!(
        messages
            .iter()
            .map(|message| message.body.as_str())
            .collect::<Vec<_>>(),
        vec!["我只输出过 abcd。"]
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.body == "stale output")
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.body == "你之前做了什么？请回忆并列出。")
    );
}

/// Verifies pending parent-to-child notes stay visible until runtime owns them.
/// 验证待处理的父到子消息在运行时接管前保持可见。
#[test]
fn subagent_surface_conversation_state_preserves_pending_send_message_notes() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let workspace_snapshot = service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot should exist");
    let mut send_note =
        SessionMessageEntry::new(SessionMessageKind::User, "智能体消息", "请继续输出 1234。")
            .with_runtime_metadata("delivery-followup-2", "1780000000005");
    send_note.record_kind = SessionMessageRecordKind::DisplayNote;
    send_note.metadata = Some(json!({
        "display_note": true,
        "kind": "subagent_message",
        "tool": "send_message",
        (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
        "delivery_id": "delivery-followup-2",
    }));
    let previous_output =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "输出", "previous output")
            .with_runtime_metadata("previous-output", "1780000000001");
    workspace_snapshot
        .session_messages
        .insert(task_session_id, vec![previous_output, send_note]);
    let canonical_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![SessionConversationRecord {
            id: String::from("canonical-previous-output"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: String::from("previous output"),
            metadata: None,
            created_at: String::from("1780000000001"),
        }],
        continuation: Default::default(),
    };

    replace_webui_subagent_task_conversation_state(
        workspace_snapshot,
        task_session_id,
        canonical_state,
        None,
    );

    let messages = workspace_snapshot
        .session_messages
        .get(&task_session_id)
        .expect("hydrated child messages should exist");
    assert_eq!(
        messages
            .iter()
            .map(|message| message.body.as_str())
            .collect::<Vec<_>>(),
        vec!["previous output", "请继续输出 1234。"]
    );
}

/// Verifies render cache alone does not create subagent navigation membership.
/// 验证仅有渲染缓存不会创建子代理导航成员关系。
#[test]
fn delegate_started_cache_without_registry_does_not_create_navigation_membership() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let task_states = vec![SubagentTaskPanelState {
        task_id: task_session_id,
        public_task_id: String::from("slow-worker"),
        parent_session_id,
        subagent_label: String::from("慢速子代理"),
        description: Some(String::from("等待 30 秒")),
        run_state: SubagentTaskRunState::Running,
        result_text: String::new(),
        conversation_state: None,
    }];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    guard
        .session_subagent_navigation_rounds
        .insert(parent_session_id);
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();

    let navigation = subagent_navigation_snapshot_from_state(&mut guard, &workspace_snapshot);

    assert!(
        navigation.is_none(),
        "render-only cache must not create subagent navigation membership without a registry entry"
    );
}

/// Verifies missing persistence runtime rejects send-message projection before any in-memory commit.
/// 验证缺少持久化运行时时会在任何内存提交前拒绝 send-message 投影。
#[test]
fn send_message_projection_preflight_failure_preserves_in_memory_state() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    let task_states = vec![SubagentTaskPanelState {
        task_id: task_session_id,
        public_task_id: String::from("output_abcd"),
        parent_session_id,
        subagent_label: String::from("output_abcd"),
        description: Some(String::from("输出 abcd")),
        run_state: SubagentTaskRunState::Completed,
        result_text: String::from("abcd"),
        conversation_state: None,
    }];
    cache_test_subagent_surface_detail_states(&mut service_state, &task_states);
    // original_cache captures the loaded child surface before the preflight failure.
    // original_cache 捕获预检失败前已加载的子会话表面。
    let original_cache = service_state
        .subagent_surface_detail_cache
        .get(&task_session_id)
        .expect("loaded child detail cache should exist")
        .clone();
    // original_workspace_messages captures the retained transcript map before projection.
    // original_workspace_messages 捕获投影前保留的转录映射。
    let original_workspace_messages = serde_json::to_value(
        &service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace snapshot")
            .session_messages,
    )
    .expect("workspace messages should serialize");
    let state = Arc::new(Mutex::new(service_state));
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-send-cache",
        "queued",
    )
    .with_metadata(Some(serde_json::json!({
        "tool_name": "send_message",
        "tool_call_id": "call-send-cache",
        "host_result": {
            "kind": "subagent_message",
            "payload": {
                "tool": "send_message",
                "parent_session_id": parent_session_id.to_string(),
                "target": "output_abcd",
                (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
                "delivery_id": "delivery-cache-followup",
                "message": "你之前做了什么？请回忆并列出。",
                "queued": true,
                "delivery_mode": "queue",
                "interrupted": false
            }
        }
    })));

    let projection_result =
        append_runtime_event_projection(&app, &state, parent_session_id, &event);
    let error = projection_result.expect_err("missing controller runtime must fail preflight");
    assert!(error.contains("controller runtime is not ready"));

    let guard = state.lock().expect("service state lock");
    let retained_cache = guard
        .subagent_surface_detail_cache
        .get(&task_session_id)
        .expect("loaded child detail cache should remain");
    assert_eq!(
        serde_json::to_value(&retained_cache.task_state)
            .expect("retained task state should serialize"),
        serde_json::to_value(&original_cache.task_state)
            .expect("original task state should serialize")
    );
    assert_eq!(
        serde_json::to_value(&retained_cache.messages)
            .expect("retained cache messages should serialize"),
        serde_json::to_value(&original_cache.messages)
            .expect("original cache messages should serialize")
    );
    assert_eq!(retained_cache.last_access_ms, original_cache.last_access_ms);
    assert_eq!(
        serde_json::to_value(
            &guard
                .workspace_snapshot
                .as_ref()
                .expect("workspace snapshot should remain")
                .session_messages,
        )
        .expect("retained workspace messages should serialize"),
        original_workspace_messages
    );
}

/// Verifies a live detached listener keeps the parent subagent navigation visible after the parent stream finishes.
/// 验证父级流结束后，实时分离监听器仍会让父级子代理导航保持可见。
#[test]
fn webui_detached_listener_keeps_running_subagent_navigation_visible() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let task_states = vec![SubagentTaskPanelState {
        task_id: task_session_id,
        public_task_id: String::from("image-worker"),
        parent_session_id,
        subagent_label: String::from("图片子代理"),
        description: Some(String::from("生成图片")),
        run_state: SubagentTaskRunState::Running,
        result_text: String::new(),
        conversation_state: None,
    }];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_session_id, &task_states);
    guard
        .session_subagent_navigation_rounds
        .insert(parent_session_id);
    guard.detached_subagent_task_listeners.insert(
        task_session_id,
        String::from("navigation-visible-execution"),
    );
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();

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

/// Verifies a terminal listener can clear only the execution generation it owns.
/// 验证终态监听器只能清理其拥有的执行代际。
#[test]
fn detached_listener_cleanup_preserves_replacement_execution() {
    // parent_session_id owns the child task surface used by the test state.
    // parent_session_id 持有测试状态使用的子任务界面。
    let parent_session_id = SessionId::from_u128(0x1180);
    // task_id is reused by old and replacement execution attempts.
    // task_id 被旧执行与替代执行尝试复用。
    let task_id = SessionId::from_u128(0x1181);
    // state stores the replacement execution as the current listener owner.
    // state 将替代执行存储为当前监听器所有者。
    let mut state = test_service_state_with_parent_session(parent_session_id);
    state
        .detached_subagent_task_listeners
        .insert(task_id, String::from("execution-new"));

    let stale_cleared = clear_webui_detached_listener_execution_if_current(
        &mut state,
        task_id,
        "execution-old",
        true,
    );

    assert!(!stale_cleared);
    assert_eq!(
        state.detached_subagent_task_listeners.get(&task_id),
        Some(&String::from("execution-new"))
    );
    let current_cleared = clear_webui_detached_listener_execution_if_current(
        &mut state,
        task_id,
        "execution-new",
        true,
    );
    assert!(current_cleared);
    assert!(
        !state
            .detached_subagent_task_listeners
            .contains_key(&task_id)
    );
}

/// Verifies an old listener terminal event cannot overwrite the replacement execution projection.
/// 验证旧监听器终态事件不能覆盖替代执行投影。
#[test]
fn stale_detached_terminal_event_skips_replacement_projection() {
    // parent_session_id owns the stable child task across both executions.
    // parent_session_id 跨两次执行持有稳定子任务。
    let parent_session_id = SessionId::from_u128(0x1182);
    // task_id is reused by the stale and replacement executions.
    // task_id 被过期执行与替代执行复用。
    let task_id = SessionId::from_u128(0x1183);
    // state declares the replacement execution as the only current listener owner.
    // state 将替代执行声明为唯一当前监听器所有者。
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .detached_subagent_task_listeners
        .insert(task_id, String::from("execution-new"));
    // state exposes the shared projection state consumed by the ingest function.
    // state 暴露 ingest 函数使用的共享投影状态。
    let state = Arc::new(Mutex::new(service_state));
    // app collects no external sinks while exercising projection ownership.
    // app 在执行投影所有权测试时不收集外部接收器。
    let app = ServiceEventEmitter::new(Vec::new());
    // runtime supplies the existing ingest signature without external provider work.
    // runtime 提供现有 ingest 签名且不触发外部 Provider 工作。
    let runtime = test_agent_runtime_bridge();
    // event is the stale execution's terminal delegate result.
    // event 是过期执行的终态委派结果。
    let event = test_delegate_finished_event(task_id, "repeat-agent", "old result");

    let consumed = ingest_webui_subagent_runtime_event_for_listener_execution(
        &app,
        &state,
        &runtime,
        parent_session_id,
        &event,
        Some("execution-old"),
    )
    .expect("stale listener event should be safely consumed");

    assert!(consumed);
    let guard = state.lock().expect("service state lock");
    assert_eq!(
        guard
            .detached_subagent_task_listeners
            .get(&task_id)
            .map(String::as_str),
        Some("execution-new")
    );
    assert!(!guard.subagent_surface_detail_cache.contains_key(&task_id));
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
        let mut guard = state
            .lock()
            .expect("service state lock should be available for terminal cleanup setup");
        let task_states = vec![SubagentTaskPanelState {
            task_id: task_session_id,
            public_task_id: String::from("image-worker"),
            parent_session_id,
            subagent_label: String::from("图片子代理"),
            description: Some(String::from("生成图片")),
            run_state: SubagentTaskRunState::Completed,
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
        }];
        cache_test_subagent_surface_detail_states(&mut guard, &task_states);
        seed_test_live_subagent_registry_from_panel_states(
            &mut guard,
            parent_session_id,
            &task_states,
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
            .insert(task_session_id, String::from("terminal-cleanup-execution"));
    }
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
        events: Arc::clone(&emitted_events),
    })]);

    {
        let mut guard = state
            .lock()
            .expect("service state lock should be available for listener cleanup");
        guard
            .detached_subagent_task_listeners
            .remove(&task_session_id);
    }
    refresh_webui_subagent_navigation_round_visibility(&state, parent_session_id)
        .expect("terminal cleanup should refresh navigation visibility");
    let navigation =
        emit_webui_subagent_navigation_changed_delta_for_parent(&app, &state, parent_session_id)
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

    let navigation =
        emit_webui_subagent_navigation_changed_delta_for_parent(&app, &state, parent_session_id)
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

/// Verifies background empty subagent navigation deltas keep their requested parent id.
/// 验证后台空子智能体导航增量会保留被请求的父会话标识。
#[test]
fn background_empty_subagent_navigation_delta_preserves_requested_parent_conversation_id() {
    let selected_session_id = SessionId::new_v4();
    let background_parent_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        selected_session_id,
    )));
    {
        let mut guard = state.lock().expect("service state lock");
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace snapshot");
        workspace.sessions.push(SessionSummary::new(
            background_parent_session_id,
            Some(String::from("project-a")),
            "后台父会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        ));
        workspace
            .session_run_phases
            .insert(background_parent_session_id, SessionRunPhase::Ready);
    }
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingServiceEventSink {
        events: Arc::clone(&emitted_events),
    })]);

    let navigation = emit_webui_subagent_navigation_changed_delta_for_parent(
        &app,
        &state,
        background_parent_session_id,
    )
    .expect("background empty navigation should emit");

    assert!(navigation.is_none());
    let expected_parent_conversation_id = background_parent_session_id.to_string();
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
    cache_test_subagent_surface_detail_states(
        &mut guard,
        &[SubagentTaskPanelState {
            task_id: task_session_id,
            public_task_id: String::from("code-review"),
            parent_session_id,
            subagent_label: String::from("代码审核"),
            description: Some(String::from("审核代码")),
            run_state: SubagentTaskRunState::Running,
            result_text: String::new(),
            conversation_state: None,
        }],
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
    let mut guard = test_service_state_with_parent_session(parent_session_id);
    let mut task_states = vec![
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
    ];
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_session_id, &task_states);
    guard
        .session_subagent_navigation_rounds
        .insert(parent_session_id);
    guard.detached_subagent_task_listeners.extend([
        (
            first_task_session_id,
            String::from("first-completion-execution"),
        ),
        (
            second_task_session_id,
            String::from("second-completion-execution"),
        ),
    ]);
    let workspace_snapshot = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot")
        .clone();

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

    for task_state in &mut task_states {
        task_state.run_state = SubagentTaskRunState::Completed;
        task_state.result_text = format!("任务 {} 已完成", task_state.task_id);
        guard
            .detached_subagent_task_listeners
            .remove(&task_state.task_id);
    }
    cache_test_subagent_surface_detail_states(&mut guard, &task_states);
    seed_test_live_subagent_registry_from_panel_states(&mut guard, parent_session_id, &task_states);

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
        let mut guard = state
            .lock()
            .expect("service state lock should be available for stale payload setup");
        guard
            .detached_subagent_task_listeners
            .insert(task_session_id, String::from("stale-running-execution"));
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
            "execution_id": "test-execution-stale-running-worker",
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
    let guard = state
        .lock()
        .expect("service state lock should be available after stale payload ingest");
    assert!(
        !guard
            .detached_subagent_task_listeners
            .contains_key(&task_session_id)
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
    cache_test_subagent_surface_detail_states(
        &mut guard,
        &[closed_state.clone(), retained_state.clone()],
    );
    seed_test_live_subagent_registry_from_panel_states(
        &mut guard,
        parent_session_id,
        &[retained_state],
    );
    guard
        .detached_subagent_task_listeners
        .insert(closed_task_id, String::from("closed-task-execution"));
    guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id,
        task_id: closed_task_id,
    });

    assert!(remove_webui_closed_subagent_tasks_in_state(
        &mut guard,
        &[closed_task_id]
    ));

    assert!(
        !guard
            .subagent_surface_detail_cache
            .contains_key(&closed_task_id)
    );
    assert!(
        guard
            .subagent_surface_detail_cache
            .contains_key(&retained_task_id)
    );
    assert!(
        !guard
            .detached_subagent_task_listeners
            .contains_key(&closed_task_id)
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

/// Verifies child completion feedback does not close the parent completed-turn hook.
/// 验证智能体完成反馈不会闭合父级完成回合 hook。
#[test]
fn subagent_completion_feedback_keeps_parent_hook_turn_open() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let runtime = test_agent_runtime_bridge();
    {
        let mut guard = state
            .lock()
            .expect("service state lock should be available for awaiting hook setup");
        let task_states = vec![SubagentTaskPanelState {
            task_id: task_session_id,
            public_task_id: String::from("review-worker"),
            parent_session_id,
            subagent_label: String::from("Review Worker"),
            description: Some(String::from("审核代码")),
            run_state: SubagentTaskRunState::Completed,
            result_text: String::from("审核完成"),
            conversation_state: None,
        }];
        cache_test_subagent_surface_detail_states(&mut guard, &task_states);
        seed_test_live_subagent_registry_from_panel_states(
            &mut guard,
            parent_session_id,
            &task_states,
        );
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .expect("workspace should exist");
        let runtime_state = workspace
            .session_runtime_state
            .entry(parent_session_id)
            .or_default();
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
        let mut hook_runtime = runtime
            .conversation_hook_runtime
            .lock()
            .expect("conversation hook runtime lock should be available for setup");
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
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "review-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-review-worker",
            "execution_status": "completed",
            "description": "审核代码",
            "subagent": "review-worker",
            "result": "审核完成"
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );
    assert!(
        runtime
            .conversation_hook_runtime
            .lock()
            .expect("conversation hook runtime lock should be available after feedback")
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
    // ignored_execution_id identifies the exact send_message-resumed execution.
    // ignored_execution_id 标识由 send_message 续跑的精确执行。
    let ignored_execution_id = "ignored-completion-execution";
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
            .insert(task_session_id, String::from(ignored_execution_id));
        let task_states = vec![SubagentTaskPanelState {
            task_id: task_session_id,
            public_task_id: String::from("reviewer"),
            parent_session_id,
            subagent_label: String::from("Reviewer"),
            description: Some(String::from("Review code")),
            run_state: SubagentTaskRunState::Running,
            result_text: String::new(),
            conversation_state: None,
        }];
        cache_test_subagent_surface_detail_states(&mut guard, &task_states);
        seed_test_live_subagent_registry_from_panel_states(
            &mut guard,
            parent_session_id,
            &task_states,
        );
    }

    let default_gate = webui_turn_completion_gate_from_state(&state, &runtime, parent_session_id)
        .expect("default gate should read");
    assert_eq!(default_gate.pending_subagent_count, 1);

    {
        let mut guard = state.lock().expect("state lock");
        mark_webui_subagent_completion_gate_ignored(
            &mut guard,
            task_session_id,
            ignored_execution_id,
        );
    }

    let ignored_gate = webui_turn_completion_gate_from_state(&state, &runtime, parent_session_id)
        .expect("ignored gate should read");
    assert_eq!(ignored_gate.pending_subagent_count, 0);

    {
        let mut guard = state.lock().expect("state lock");
        guard
            .detached_subagent_task_listeners
            .insert(task_session_id, String::from("replacement-execution"));
    }

    let replacement_gate =
        webui_turn_completion_gate_from_state(&state, &runtime, parent_session_id)
            .expect("replacement gate should read");
    assert_eq!(replacement_gate.pending_subagent_count, 1);
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

/// Verifies detached mailbox delivery is idempotent per execution, not per long-lived task.
/// 验证分离 mailbox 投递按单次执行幂等，而不是按长期任务幂等。
#[test]
fn subagent_completion_queues_each_distinct_execution_for_same_task() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let first_event =
        test_delegate_finished_event(task_session_id, "repeat-agent-run-1", "第一次结果");
    let second_event =
        test_delegate_finished_event(task_session_id, "repeat-agent-run-2", "第二次结果");

    let first_queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &first_event,
        true,
        false,
    )
    .expect("first mailbox input should inspect");
    let duplicate_first = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &first_event,
        true,
        false,
    )
    .expect("duplicate mailbox input should inspect");
    let second_queued = enqueue_webui_session_mailbox_input_if_needed(
        &state,
        parent_session_id,
        &second_event,
        true,
        false,
    )
    .expect("second mailbox input should inspect");

    assert!(first_queued);
    assert!(!duplicate_first);
    assert!(second_queued);
    let mailbox_prompt = webui_session_mailbox_input_from_kernel(
        &state,
        parent_session_id,
        Some(&task_session_id.to_string()),
    )
    .expect("mailbox prompt should read")
    .expect("mailbox prompt should exist");
    assert!(mailbox_prompt.contains("第一次结果"));
    assert!(mailbox_prompt.contains("第二次结果"));
    let guard = state.lock().expect("state lock");
    let kernel = guard
        .session_runtime_kernels
        .get(&parent_session_id)
        .expect("parent session kernel should exist");
    let mailbox_inputs = kernel.input_queue().get_mailbox_input();
    assert_eq!(mailbox_inputs.len(), 2);
    assert!(mailbox_inputs.iter().any(|input| {
        input.payload["execution_id"] == "test-execution-repeat-agent-run-1"
            && input.payload["mailbox_event"]["payload"]["result"] == "第一次结果"
    }));
    assert!(mailbox_inputs.iter().any(|input| {
        input.payload["execution_id"] == "test-execution-repeat-agent-run-2"
            && input.payload["mailbox_event"]["payload"]["result"] == "第二次结果"
    }));
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

/// Verifies child-agent completion appends independent feedback rows instead of parent turn usage.
/// 验证智能体完成时追加独立反馈记录，而不是写入父回合用量。
#[test]
fn subagent_completion_appends_independent_parent_feedback_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "image-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-image-worker",
            "execution_status": "completed",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": "生成完成：a.jpg",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "image-worker",
                "provider_call_count": 1,
                "tool_call_count": 0,
                "input_tokens": 100,
                "output_tokens": 25,
                "reasoning_tokens": 5,
                "total_tokens": 130,
                "cached_input_tokens": 40
            }]
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );
    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("duplicate delegate finished should ingest without appending")
    );

    let guard = state.lock().expect("service state lock");
    let messages = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .expect("parent should contain display feedback rows");
    let feedback_rows = messages
        .iter()
        .filter(|message| {
            message.metadata.as_ref().is_some_and(|metadata| {
                metadata
                    .get("subagent_feedback_record")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(feedback_rows.len(), 2);
    let feedback_row = feedback_rows
        .iter()
        .find(|message| message.record_kind == SessionMessageRecordKind::DisplayNote)
        .expect("one feedback row should carry child-agent result metadata");
    assert_eq!(feedback_row.body, "智能体返回处理结果");
    assert_eq!(
        feedback_row
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("host_result"))
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("result"))
            .and_then(serde_json::Value::as_str),
        Some("生成完成：a.jpg")
    );
    let stats_row = feedback_rows
        .iter()
        .find(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("one feedback row should carry child-agent stats");
    let stats: SessionTurnStats = serde_json::from_value(
        stats_row
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("turn_stats"))
            .expect("stats metadata should exist")
            .clone(),
    )
    .expect("stats metadata should parse");

    assert_eq!(stats.turn_kind, SessionTurnStatsKind::SubagentFeedback);
    assert_eq!(stats.provider_call_count, 1);
    assert_eq!(stats.input_tokens, 100);
    assert_eq!(stats.output_tokens, 25);
    assert_eq!(stats.reasoning_tokens, 5);
    assert_eq!(stats.total_tokens, 130);
    assert_eq!(stats.cached_input_tokens, Some(40));
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::SubagentTask
    );
    assert_eq!(stats.usage_groups.len(), 1);
    assert_eq!(
        stats.usage_groups[0].kind,
        SessionTurnUsageGroupKind::Subagent
    );
    assert_eq!(stats.usage_groups[0].total_tokens, 130);
    assert_eq!(stats.usage_groups[0].cached_input_tokens, Some(40));
}

/// Verifies active parent user turns still receive independent child feedback display rows.
/// 验证活动父级用户回合仍会接收独立智能体反馈显示行。
#[test]
fn active_parent_turn_appends_independent_subagent_feedback_display_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    start_webui_session_kernel_user_turn(&mut service_state, parent_session_id, "turn-a")
        .expect("parent user turn should start");
    bind_webui_subagent_parent_turn(
        &mut service_state,
        parent_session_id,
        task_session_id,
        "test-execution-image-worker-active",
    );
    let state = Arc::new(Mutex::new(service_state));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "image-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-image-worker-active",
            "execution_status": "completed",
            "description": "生成图片",
            "subagent": "image-worker",
            "result": "生成完成：a.jpg",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "image-worker",
                "provider_call_count": 1,
                "tool_call_count": 0,
                "input_tokens": 100,
                "output_tokens": 25,
                "reasoning_tokens": 5,
                "total_tokens": 130,
                "cached_input_tokens": 40
            }]
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );

    let guard = state.lock().expect("service state lock");
    let feedback_rows = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(
        feedback_rows.len(),
        2,
        "active parent user turns without wait_agent ownership should append independent feedback and stats rows"
    );
    for row in feedback_rows {
        let metadata = row
            .metadata
            .as_ref()
            .expect("feedback metadata should exist");
        assert_eq!(
            metadata
                .get("parent_turn_id")
                .and_then(serde_json::Value::as_str),
            Some("turn-a")
        );
        assert_eq!(
            metadata
                .get("parent_turn_binding")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert!(
            metadata
                .get("source_generation")
                .and_then(serde_json::Value::as_u64)
                .is_some()
        );
    }
}

/// Verifies active workspace run phases still receive independent child feedback display rows.
/// 验证活跃工作区运行阶段仍会接收独立智能体反馈显示行。
#[test]
fn active_parent_workspace_phase_appends_independent_subagent_feedback_display_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot should exist")
        .session_run_phases
        .insert(parent_session_id, SessionRunPhase::Requesting);
    let state = Arc::new(Mutex::new(service_state));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = test_delegate_finished_event(task_session_id, "phase-worker", "阶段中完成");

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );

    let guard = state.lock().expect("service state lock");
    let feedback_rows = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(
        feedback_rows.len(),
        2,
        "active parent phases without wait_agent ownership should append independent feedback and stats rows"
    );
}

/// Verifies wait_agent-owned feedback suppresses independent parent feedback rows.
/// 验证 wait_agent 接管的反馈会抑制独立父级反馈记录。
#[test]
fn wait_agent_feedback_suppresses_independent_parent_feedback_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let second_task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    service_state
        .parent_sessions_waiting_for_subagents
        .insert(parent_session_id);
    session_runtime_kernel_mut(&mut service_state, parent_session_id)
        .register_wait_scope(
            "wait-agent-active",
            [
                task_session_id.to_string(),
                second_task_session_id.to_string(),
            ],
            current_unix_millis_u64(),
        )
        .expect("wait scope should register");
    let state = Arc::new(Mutex::new(service_state));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "wait-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-wait-worker",
            "execution_status": "completed",
            "description": "等待反馈",
            "subagent": "wait-worker",
            "result": "wait 已拿到正文",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "wait-worker",
                "provider_call_count": 2,
                "tool_call_count": 1,
                "input_tokens": 200,
                "output_tokens": 50,
                "reasoning_tokens": 10,
                "total_tokens": 260,
                "cached_input_tokens": 80
            }]
        }
    })));
    let second_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "第二个智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "wait-worker-b",
            (SUBAGENT_TASK_ID_KEY): second_task_session_id.to_string(),
            "execution_id": "test-execution-wait-worker-b",
            "execution_status": "completed",
            "description": "等待反馈 B",
            "subagent": "wait-worker-b",
            "result": "wait 已拿到第二条正文",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "wait-worker-b",
                "provider_call_count": 3,
                "tool_call_count": 0,
                "input_tokens": 300,
                "output_tokens": 75,
                "reasoning_tokens": 15,
                "total_tokens": 390,
                "cached_input_tokens": 120
            }]
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delegate finished should ingest")
    );
    assert!(
        ingest_webui_subagent_runtime_event(
            &app,
            &state,
            &runtime,
            parent_session_id,
            &second_event,
        )
        .expect("second delegate finished should ingest")
    );

    let guard = state.lock().expect("service state lock");
    let feedback_rows = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        feedback_rows.is_empty(),
        "wait_agent-owned feedback must stay inside the wait_agent tool result instead of creating separate parent rows"
    );
}

/// Verifies feedback consumed by a completed wait_agent suppresses delayed parent feedback rows.
/// 验证已完成 wait_agent 消费过的反馈会抑制迟到的父级反馈记录。
#[test]
fn consumed_wait_agent_feedback_suppresses_delayed_parent_feedback_rows() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(parent_session_id);
    assert!(consume_webui_subagent_mailbox_input_for_execution(
        &mut service_state,
        parent_session_id,
        task_session_id,
        "delayed-wait-worker-run",
    ));
    let state = Arc::new(Mutex::new(service_state));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "迟到的智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "delayed-wait-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "delayed-wait-worker-run",
            "execution_status": "completed",
            "description": "迟到反馈",
            "subagent": "delayed-wait-worker",
            "result": "wait 已经展示过这段正文",
            "usage_segments": [{
                "kind": "subagent_task",
                "trigger": "delayed-wait-worker",
                "provider_call_count": 1,
                "tool_call_count": 0,
                "input_tokens": 10,
                "output_tokens": 2,
                "reasoning_tokens": 1,
                "total_tokens": 13,
                "cached_input_tokens": 4
            }]
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect("delayed delegate finished should ingest")
    );

    let guard = state.lock().expect("service state lock");
    let feedback_rows = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        feedback_rows.is_empty(),
        "delayed wait-owned feedback must not create separate parent rows"
    );
}

/// Verifies feedback deduplication is scoped to one concrete child-agent execution.
/// 验证反馈去重只作用于一次具体智能体执行。
#[test]
fn subagent_feedback_dedupes_by_execution_id_not_task_id_only() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let first_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体第一次任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "repeat-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "repeat-worker-run-1",
            "execution_status": "completed",
            "description": "重复智能体",
            "subagent": "repeat-worker",
            "result": "第一次完成",
            "usage_segments": []
        }
    })));
    let second_event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体第二次任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "repeat-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "repeat-worker-run-2",
            "execution_status": "completed",
            "description": "重复智能体",
            "subagent": "repeat-worker",
            "result": "第二次完成",
            "usage_segments": []
        }
    })));

    assert!(
        ingest_webui_subagent_runtime_event(
            &app,
            &state,
            &runtime,
            parent_session_id,
            &first_event
        )
        .expect("first delegate finished should ingest")
    );
    assert!(
        ingest_webui_subagent_runtime_event(
            &app,
            &state,
            &runtime,
            parent_session_id,
            &first_event
        )
        .expect("duplicate delegate finished should ingest without appending")
    );
    assert!(
        ingest_webui_subagent_runtime_event(
            &app,
            &state,
            &runtime,
            parent_session_id,
            &second_event,
        )
        .expect("second execution should append independent feedback")
    );

    let guard = state.lock().expect("service state lock");
    let messages = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .expect("parent should contain both execution feedback rows");
    let feedback_rows = messages
        .iter()
        .filter(|message| {
            message.metadata.as_ref().is_some_and(|metadata| {
                metadata
                    .get("subagent_feedback_record")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(feedback_rows.len(), 4);
    let feedback_results = feedback_rows
        .iter()
        .filter_map(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("host_result"))
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("result"))
                .and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>();
    assert!(feedback_results.contains(&"第一次完成"));
    assert!(feedback_results.contains(&"第二次完成"));
}

/// Verifies malformed child-agent usage data does not append zero-token feedback stats.
/// 验证格式错误的智能体用量数据不会追加 0-token 反馈统计。
#[test]
fn malformed_subagent_feedback_usage_segments_do_not_append_zero_stats() {
    let parent_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let runtime = test_agent_runtime_bridge();
    let app = ServiceEventEmitter::new(Vec::new());
    let event = RuntimeEvent::new(
        RuntimeEventKind::DelegateFinished,
        "智能体完成",
        "智能体任务完成。",
    )
    .with_metadata(Some(serde_json::json!({
        "kind": SUBAGENT_TASK_KIND,
        "payload": {
            "task_id": "bad-usage-worker",
            (SUBAGENT_TASK_ID_KEY): task_session_id.to_string(),
            "execution_id": "test-execution-bad-usage-worker",
            "execution_status": "completed",
            "description": "返回坏统计",
            "subagent": "bad-usage-worker",
            "result": "完成但统计坏了",
            "usage_segments": {
                "kind": "subagent_task",
                "input_tokens": 100
            }
        }
    })));

    let error =
        ingest_webui_subagent_runtime_event(&app, &state, &runtime, parent_session_id, &event)
            .expect_err("malformed usage segments must reject the feedback projection");

    assert!(error.contains("failed to parse subagent feedback usage segments"));
    let guard = state.lock().expect("service state lock");
    let feedback_count = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_messages.get(&parent_session_id))
        .map(|messages| {
            messages
                .iter()
                .filter(|message| {
                    message.metadata.as_ref().is_some_and(|metadata| {
                        metadata
                            .get("subagent_feedback_record")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false)
                    })
                })
                .count()
        })
        .unwrap_or(0);
    assert_eq!(feedback_count, 0);
}

/// Verifies multiple detached subagent completions render as one blank-line-separated mailbox prompt.
/// 验证多条分离子代理完成消息会渲染为一个使用空行分隔的 mailbox 提示词。
#[test]
fn subagent_completion_mailbox_prompt_joins_multiple_inputs() {
    let parent_session_id = SessionId::new_v4();
    let first_task_session_id = SessionId::new_v4();
    let second_task_session_id = SessionId::new_v4();
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        parent_session_id,
    )));
    let first_event =
        test_delegate_finished_event(first_task_session_id, "image-a", "生成完成：a.jpg");
    let second_event =
        test_delegate_finished_event(second_task_session_id, "image-b", "生成完成：b.jpg");

    assert!(
        enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &first_event,
            true,
            false,
        )
        .expect("first mailbox input should enqueue")
    );
    assert!(
        enqueue_webui_session_mailbox_input_if_needed(
            &state,
            parent_session_id,
            &second_event,
            true,
            false,
        )
        .expect("second mailbox input should enqueue")
    );

    let mailbox_prompt = webui_session_mailbox_input_from_kernel(&state, parent_session_id, None)
        .expect("mailbox prompt should read")
        .expect("mailbox prompt should exist");

    assert!(mailbox_prompt.contains("生成完成：a.jpg"));
    assert!(mailbox_prompt.contains("生成完成：b.jpg"));
    assert_eq!(mailbox_prompt.matches("<agent_message>").count(), 2);
    assert!(mailbox_prompt.contains("</agent_message>\n\n<agent_message>"));
}
