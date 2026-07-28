//! prompt runtime state tests.
//! prompt runtime state tests。

use super::*;
use vulcan_agent_runtime::turn_stats::build_session_turn_stats_from_state_with_segments;

/// Verifies the initial WebUI prompt snapshot keeps memory recall visible instead of downgrading to a generic request cue.
/// 验证 WebUI 提示词初始快照会保留记忆召回显示，而不是降级成通用请求提示。
#[test]
fn webui_prompt_snapshot_preparing_preserves_memory_recall_phase() {
    let session_id = SessionId::new_v4();
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "记忆召回测试",
            "",
            SessionState::Running,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, SessionRuntimeStateEntry::default())]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::MemoryRecall)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    mark_webui_prompt_snapshot_preparing(&mut workspace_snapshot, session_id);

    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::MemoryRecall)
    );
}

/// Verifies WebUI restores precheck metadata without fabricating dialogue context from display rows.
/// 验证 WebUI 会恢复 precheck 元数据，但不会从显示行伪造对话层上下文。
#[test]
fn ensure_webui_session_runtime_state_keeps_display_rows_out_of_dialogue_context() {
    let session_id = SessionId::new_v4();
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "新会话",
            "",
            SessionState::Running,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            session_id,
            vec![SessionMessageEntry::new(
                SessionMessageKind::User,
                "用户",
                "第一条提示词",
            )],
        )]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::MemoryRecall)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, Some("turn-1"))
        .expect("valid hydrated runtime identities");

    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state shell should be rebuilt");
    assert_eq!(
        runtime_state.active_precheck_turn_id.as_deref(),
        Some("turn-1")
    );
    assert!(!runtime_state_has_concrete_conversation_context(
        runtime_state
    ));
    assert!(
        !workspace_snapshot
            .hydrated_session_ids
            .contains(&session_id)
    );
    assert_eq!(
        workspace_snapshot
            .session_messages
            .get(&session_id)
            .map(Vec::len),
        Some(1)
    );
}

/// Verifies runtime-state refresh preserves Goal-bound canonical tool messages byte-for-byte.
/// 验证运行态刷新会逐字节保留绑定目标的规范工具消息。
#[test]
fn ensure_webui_session_runtime_state_preserves_goal_bound_canonical_tool_history() {
    use vulcan_agent_runtime::conversation_protocol::{
        SessionConversationMessage, SessionConversationPayload, SessionConversationToolCall,
    };

    // session_id identifies the runtime state that passes through the WebUI refresh boundary.
    // session_id 标识穿过 WebUI 刷新边界的运行态。
    let session_id = SessionId::new_v4();
    // goal_prompt models the concrete provider-visible prompt used by an automatic Goal continuation.
    // goal_prompt 模拟目标自动续轮使用的具体 Provider 可见提示词。
    let goal_prompt = String::from(
        "<thread_goal_context>\n<goal_version>goal-1</goal_version>\n</thread_goal_context>\n\nContinue working toward the active thread goal.",
    );
    // canonical_messages contains an exact assistant tool call and its matching result after the Goal prompt.
    // canonical_messages 包含目标提示词之后精确的助手工具调用及其匹配结果。
    let canonical_messages = vec![
        SessionConversationMessage {
            id: String::from("goal-prompt"),
            payload: SessionConversationPayload::UserText {
                text: goal_prompt,
                attachments: Vec::new(),
            },
        },
        SessionConversationMessage {
            id: String::from("assistant-tool-call"),
            payload: SessionConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::from("继续执行"),
                provider_message_id: Some(String::from("provider-message-1")),
                provider_message_state: Some(json!({"opaque": "state"})),
                usage: None,
                tool_calls: vec![SessionConversationToolCall {
                    id: String::from("tool-item-1"),
                    tool_call_id: Some(String::from("provider-tool-call-1")),
                    tool_name: String::from("shell_command"),
                    arguments: json!({"command": "sleep 10"}),
                }],
            },
        },
        SessionConversationMessage {
            id: String::from("tool-result"),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-tool-call-1")),
                text: String::from("completed"),
                is_error: false,
            },
        },
    ];
    // expected_messages captures the canonical provider history before any display projection runs.
    // expected_messages 捕获任何显示投影运行前的规范 Provider 历史。
    let expected_messages =
        serde_json::to_value(&canonical_messages).expect("canonical messages should serialize");
    // runtime_state owns the canonical history that must never be rebuilt from display rows.
    // runtime_state 持有绝不能从显示行重建的规范历史。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.conversation_state.messages = canonical_messages;
    // workspace_snapshot supplies the exact retained-state shape used by the production refresh helper.
    // workspace_snapshot 提供生产刷新辅助函数使用的精确保留状态形态。
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    ensure_webui_session_runtime_state(&mut workspace_snapshot, session_id, None)
        .expect("runtime-state refresh should preserve canonical history");

    // refreshed_messages is compared structurally so every tool identifier and provider state remains exact.
    // refreshed_messages 通过结构化比较确保每个工具标识与 Provider 状态保持精确。
    let refreshed_messages = serde_json::to_value(
        &workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .expect("refreshed runtime state")
            .conversation_state
            .messages,
    )
    .expect("refreshed canonical messages should serialize");
    assert_eq!(refreshed_messages, expected_messages);
}

/// Verifies display transcript rows alone never satisfy prompt-submission dialogue context.
/// 验证仅有显示层转录行时不能满足提示词提交所需的对话层上下文。
#[test]
fn retained_submission_context_rejects_display_only_messages() {
    // session_id identifies the existing conversation being submitted.
    // session_id 标识正在提交的既有会话。
    let session_id = SessionId::new_v4();
    // workspace_snapshot simulates a selected conversation whose UI window was loaded but runtime context is absent.
    // workspace_snapshot 模拟 UI 窗口已加载但运行态上下文缺失的选中会话。
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "显示层会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            session_id,
            vec![SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "助手",
                "只存在于显示层的正文",
            )],
        )]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::from([session_id]),
    };

    assert!(!retained_submission_snapshot_has_session_context(
        &workspace_snapshot,
        session_id
    ));
}

/// Verifies display records alone never satisfy provider-visible dialogue readiness.
/// 验证仅有显示记录时不能满足 Provider 可见对话就绪条件。
#[test]
fn retained_submission_context_rejects_record_only_runtime_state() {
    // session_id identifies the existing conversation being submitted.
    // session_id 标识正在提交的既有会话。
    let session_id = SessionId::new_v4();
    // runtime_state simulates a projection-only state whose canonical API messages were not restored.
    // runtime_state 模拟只恢复了投影记录但未恢复规范 API 消息的状态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("display-only-record"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: String::from("只存在于 records 的显示正文"),
            metadata: None,
            created_at: String::from("1000"),
        });
    // workspace_snapshot carries runtime records but no canonical messages.
    // workspace_snapshot 携带运行态 records，但不携带规范 messages。
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "仅显示记录会话",
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
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::from([session_id]),
    };

    assert!(!retained_submission_snapshot_has_session_context(
        &workspace_snapshot,
        session_id
    ));
}

/// Verifies prompt submission accepts retained snapshots only when runtime dialogue context exists.
/// 验证提示词提交仅在运行态对话层上下文存在时接受保留快照。
#[test]
fn retained_submission_context_accepts_runtime_dialogue_context() {
    // session_id identifies the existing conversation being submitted.
    // session_id 标识正在提交的既有会话。
    let session_id = SessionId::new_v4();
    // runtime_state stores the concrete dialogue context used for future API requests.
    // runtime_state 保存后续 API 请求使用的具体对话层上下文。
    let runtime_state =
        SessionRuntimeStateEntry {
            conversation_state: conversation_state_from_session_messages(&[
                SessionMessageEntry::new(SessionMessageKind::User, "用户", "继续之前的问题")
                    .with_runtime_metadata("dialogue-user-1", "1"),
            ])
            .expect("valid replayed conversation identities"),
            ..SessionRuntimeStateEntry::default()
        };
    // workspace_snapshot simulates a retained snapshot whose dialogue layer is ready.
    // workspace_snapshot 模拟对话层已就绪的保留快照。
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "对话层会话",
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
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert!(retained_submission_snapshot_has_session_context(
        &workspace_snapshot,
        session_id
    ));
}

/// Verifies WebUI display projection prefers retained display rows over dialogue context rows.
/// 验证 WebUI 显示投影优先使用保留的显示层行而不是对话层上下文行。
#[test]
fn display_session_messages_prefer_display_window_over_dialogue_context() {
    // session_id identifies the selected conversation.
    // session_id 标识当前选中的会话。
    let session_id = SessionId::new_v4();
    // display_entry represents the UI transcript window loaded from segmented turn rows.
    // display_entry 表示从分段 turn 行加载的 UI 转录窗口。
    let display_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "显示层正文");
    // runtime_state represents the compact dialogue context used for API requests.
    // runtime_state 表示 API 请求使用的压缩对话层上下文。
    let runtime_state =
        SessionRuntimeStateEntry {
            conversation_state: conversation_state_from_session_messages(&[
                SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "对话层正文")
                    .with_runtime_metadata("dialogue-assistant-1", "1"),
            ])
            .expect("valid replayed conversation identities"),
            ..SessionRuntimeStateEntry::default()
        };
    // workspace_snapshot carries both layers so the projection priority can be checked directly.
    // workspace_snapshot 同时携带两层状态，以便直接检查投影优先级。
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(session_id, vec![display_entry])]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::from([session_id]),
    };

    // display_messages should preserve the UI-visible transcript body.
    // display_messages 应保留 UI 可见转录正文。
    let display_messages = display_session_messages_from_workspace(&workspace_snapshot, session_id)
        .expect("display messages should exist");
    // compaction_messages should preserve the runtime context body.
    // compaction_messages 应保留运行态上下文正文。
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should exist");
    let compaction_messages =
        conversation_state_to_compaction_source_messages(&runtime_state.conversation_state);

    assert_eq!(
        display_messages
            .first()
            .map(|message| message.body.as_str()),
        Some("显示层正文")
    );
    assert_eq!(
        compaction_messages
            .first()
            .map(|message| message.body.as_str()),
        Some("对话层正文")
    );
}

/// Verifies hidden runtime records do not enter the user-facing display transcript.
/// 验证隐藏运行时记录不会进入面向用户的显示转录。
#[test]
fn display_transcript_filters_hidden_runtime_records() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("hidden-agent-message"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: false,
            title: String::from("子代理消息"),
            body: String::from("<agent_message>hidden</agent_message>"),
            metadata: None,
            created_at: String::from("100"),
        });
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("legacy-visible-agent-message"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("子代理消息"),
            body: String::from("<agent_message>legacy visible</agent_message>"),
            metadata: None,
            created_at: String::from("100"),
        });
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("visible-assistant-message"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("模型输出"),
            body: String::from("visible answer"),
            metadata: None,
            created_at: String::from("101"),
        });
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::from([session_id]),
    };

    let display_messages = display_session_messages_from_workspace(&workspace_snapshot, session_id)
        .expect("display messages should exist");

    assert_eq!(display_messages.len(), 1);
    assert_eq!(display_messages[0].id, "visible-assistant-message");
    assert_eq!(display_messages[0].body, "visible answer");
}

/// Verifies runtime-log hydrated messages can back compaction without display records.
/// 验证运行时日志水合消息在没有显示记录时也能支撑压缩。
#[test]
fn compaction_source_accepts_runtime_log_messages_without_records() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.conversation_state.messages = vec![
        SessionConversationMessage {
            id: String::from("runtime-user-1"),
            payload: SessionConversationPayload::UserText {
                text: String::from("运行时日志里的用户问题"),
                attachments: Vec::new(),
            },
        },
        SessionConversationMessage {
            id: String::from("runtime-assistant-1"),
            payload: SessionConversationPayload::Assistant {
                text: String::from("运行时日志里的助手回答"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        },
    ];
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should exist");
    let messages =
        conversation_state_to_compaction_source_messages(&runtime_state.conversation_state);

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, "runtime-user-1");
    assert!(messages[0].body.contains("运行时日志里的用户问题"));
    assert!(messages[1].body.contains("运行时日志里的助手回答"));
}

/// Verifies live goal-accounting turn stats include embedded auto-compaction usage.
/// 验证目标结算使用的实时轮次统计会包含嵌入式自动压缩用量。
#[test]
fn latest_session_turn_stats_includes_embedded_compaction_usage_segments() {
    let conversation_state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("session-user-2000"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("继续当前任务"),
                    attachments: Vec::new(),
                },
            },
            SessionConversationMessage {
                id: String::from("session-assistant-2001"),
                payload: SessionConversationPayload::Assistant {
                    text: String::from("已继续处理"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: Some(ProviderUsageSnapshot {
                        input_tokens: Some(40),
                        output_tokens: Some(12),
                        total_tokens: Some(52),
                        cached_input_tokens: Some(16),
                        ..ProviderUsageSnapshot::default()
                    }),
                    tool_calls: Vec::new(),
                },
            },
        ],
        records: vec![
            SessionConversationRecord {
                id: String::from("record-user-2000"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::new(),
                body: String::from("继续当前任务"),
                metadata: None,
                created_at: String::from("2000"),
            },
            SessionConversationRecord {
                id: String::from("record-summary-2002"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::SummaryOutput,
                visible: true,
                title: String::new(),
                body: String::from("已继续处理"),
                metadata: None,
                created_at: String::from("2002"),
            },
        ],
        continuation: Default::default(),
    };
    let mut compaction_segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::ContextCompaction,
        1,
        Some(&ProviderUsageSnapshot {
            input_tokens: Some(20),
            output_tokens: Some(4),
            total_tokens: Some(24),
            cached_input_tokens: Some(5),
            ..ProviderUsageSnapshot::default()
        }),
    );
    compaction_segment.trigger = Some(String::from("auto_budget"));

    let stats = build_session_turn_stats_from_state_with_segments(
        &conversation_state,
        "record-summary-2002",
        SessionTurnStatsStatus::Completed,
        Some(1),
        SessionTurnStatsKind::UserTurn,
        vec![compaction_segment],
    )
    .expect("latest turn stats should be built");

    assert_eq!(stats.provider_call_count, 2);
    assert_eq!(stats.input_tokens, 60);
    assert_eq!(stats.output_tokens, 16);
    assert_eq!(stats.total_tokens, 76);
    assert_eq!(stats.cached_input_tokens, Some(21));
    assert_eq!(stats.usage_segments.len(), 2);
    assert_eq!(
        stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
    );
    assert_eq!(
        stats.usage_segments[1].kind,
        SessionTurnUsageSegmentKind::ContextCompaction
    );
}

/// Verifies persisted provider counters project as cumulative goal details without double-counting reasoning.
/// 验证持久化 Provider 计数会投影为目标累计明细，且不会重复计算思考。
#[test]
fn persisted_goal_provider_usage_projects_cumulative_details() {
    // goal mirrors one request with 24 new input, 16 cached input, and 12 complete output tokens.
    // goal 模拟一次包含 24 个新增输入、16 个缓存输入和 12 个完整输出 Token 的请求。
    let goal = ThreadGoal {
        thread_id: SessionId::new_v4(),
        goal_id: String::from("goal-stage-current"),
        objective: String::from("验证逐请求累计用量"),
        status: ThreadGoalStatus::Active,
        token_budget: Some(1_000),
        tokens_used: 36,
        uncached_input_tokens: 24,
        cached_input_tokens: 16,
        output_tokens: 12,
        reasoning_tokens: 4,
        total_tokens: 52,
        time_used_seconds: 0,
        created_at_ms: 1,
        updated_at_ms: 2,
    };

    let summary = session_goal_summary_from_thread_goal(&goal);

    assert_eq!(summary.usage_tokens, 36);
    assert_eq!(summary.input_tokens, 24);
    assert_eq!(summary.cached_input_tokens, 16);
    assert_eq!(summary.output_tokens, 12);
    assert_eq!(summary.reasoning_tokens, 4);
    assert_eq!(summary.total_tokens, 52);
}

/// Verifies runtime-only compaction sources do not become display fallback rows.
/// 验证仅运行态压缩来源不会变成显示层回退行。
#[test]
fn display_session_messages_do_not_project_runtime_only_context() {
    // session_id identifies a runtime-only hydrated conversation.
    // session_id 标识只有运行态已水合的会话。
    let session_id = SessionId::new_v4();
    // runtime_state carries provider-visible messages but no display records.
    // runtime_state 携带 Provider 可见消息但不携带显示记录。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.conversation_state.messages = vec![SessionConversationMessage {
        id: String::from("runtime-user-1"),
        payload: SessionConversationPayload::UserText {
            text: String::from("只能用于运行态的正文"),
            attachments: Vec::new(),
        },
    }];
    // workspace_snapshot intentionally has no session_messages row for the display layer.
    // workspace_snapshot 有意不携带显示层 session_messages 行。
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert!(display_session_messages_from_workspace(&workspace_snapshot, session_id).is_none());
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should exist");
    assert!(
        !conversation_state_to_compaction_source_messages(&runtime_state.conversation_state)
            .is_empty()
    );
}

/// Verifies draft submissions can reuse the retained workspace instead of reloading every workspace session.
/// 验证草稿提交可以复用保留工作区，而不是重载所有工作区会话。
#[test]
fn retained_submission_snapshot_accepts_draft_submission() {
    let project_id = String::from("project-a");
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id.clone(),
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            Vec::new(),
        )],
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };

    assert!(retained_submission_snapshot_is_usable(
        &workspace_snapshot,
        Some(project_id.as_str()),
        None
    ));
    assert!(!retained_submission_snapshot_is_usable(
        &workspace_snapshot,
        Some("missing-project"),
        None
    ));
    assert!(retained_submission_snapshot_is_usable(
        &workspace_snapshot,
        None,
        None
    ));
}

/// Verifies retained old-session submissions require only the target session to exist in memory.
/// 验证保留的老会话提交只要求目标会话存在于内存快照中。
#[test]
fn retained_submission_snapshot_accepts_existing_session_only_when_present() {
    let session_id = SessionId::new_v4();
    let missing_session_id = SessionId::new_v4();
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "已有会话",
            "",
            SessionState::Draft,
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
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert!(retained_submission_snapshot_is_usable(
        &workspace_snapshot,
        None,
        Some(session_id)
    ));
    assert!(!retained_submission_snapshot_is_usable(
        &workspace_snapshot,
        None,
        Some(missing_session_id)
    ));
}

/// Verifies WebUI regular-session detection excludes persisted TaskSession identifiers from workspace-store writes.
/// 验证 WebUI 普通会话检测会把持久化 TaskSession 标识排除在 workspace-store 写入之外。
#[test]
fn webui_regular_session_detection_excludes_task_session_ids() {
    let active_session_id = SessionId::new_v4();
    let archived_session_id = SessionId::new_v4();
    let task_session_id = SessionId::new_v4();
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            active_session_id,
            Some(String::from("project-a")),
            "活动会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: vec![SessionSummary::new(
            archived_session_id,
            Some(String::from("project-a")),
            "归档会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: active_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert!(workspace_snapshot_contains_regular_session(
        &workspace_snapshot,
        active_session_id
    ));
    assert!(workspace_snapshot_contains_regular_session(
        &workspace_snapshot,
        archived_session_id
    ));
    assert!(!workspace_snapshot_contains_regular_session(
        &workspace_snapshot,
        task_session_id
    ));
}

/// Verifies conversation hydration reuses retained workspace snapshots only for known sessions.
/// 验证会话水合只会对已知会话复用保留工作区快照。
#[test]
fn retained_workspace_snapshot_for_hydration_requires_known_session_and_strips_payloads() {
    let known_session_id = SessionId::new_v4();
    let missing_session_id = SessionId::new_v4();
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            known_session_id,
            Some(String::from("project-a")),
            "已知会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            known_session_id,
            vec![SessionMessageEntry::new(
                SessionMessageKind::User,
                "用户",
                "保留消息",
            )],
        )]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(
            known_session_id,
            SessionRuntimeStateEntry {
                active_model_id: Some(String::from("retained-model")),
                ..SessionRuntimeStateEntry::default()
            },
        )]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(known_session_id, SessionRunPhase::MemoryRecall)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: known_session_id,
        hydrated_session_ids: BTreeSet::from([known_session_id]),
    };

    let retained_snapshot = retained_workspace_snapshot_for_session_hydration(
        Some(&workspace_snapshot),
        known_session_id,
    )
    .expect("known session should reuse retained snapshot");
    assert_eq!(retained_snapshot.sessions.len(), 1);
    assert_eq!(retained_snapshot.selected_session, known_session_id);
    assert_eq!(
        retained_snapshot.session_run_phases.get(&known_session_id),
        Some(&SessionRunPhase::MemoryRecall)
    );
    assert!(retained_snapshot.session_messages.is_empty());
    assert!(retained_snapshot.session_runtime_state.is_empty());
    assert!(retained_snapshot.hydrated_session_ids.is_empty());
    assert!(
        retained_workspace_snapshot_for_session_hydration(
            Some(&workspace_snapshot),
            missing_session_id,
        )
        .is_none()
    );
}

/// Verifies targeted refresh selection prefers the requested visible session over the index default.
/// 验证带目标刷新选择会优先使用请求的可见会话，而不是索引默认会话。
#[test]
fn targeted_refresh_selection_prefers_visible_requested_session() {
    let default_session_id = SessionId::new_v4();
    let requested_session_id = SessionId::new_v4();
    let refreshed_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![
            SessionSummary::new(
                default_session_id,
                Some(String::from("project-a")),
                "默认会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
            SessionSummary::new(
                requested_session_id,
                Some(String::from("project-a")),
                "请求会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
        ],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: default_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert_eq!(
        selected_session_id_after_refresh(&refreshed_snapshot, Some(requested_session_id)),
        requested_session_id
    );
}

/// Verifies targeted refresh selection does not select a non-visible session after the refresh.
/// 验证带目标刷新选择不会在刷新后选中不可见会话。
#[test]
fn targeted_refresh_selection_keeps_default_when_requested_session_is_not_visible() {
    let default_session_id = SessionId::new_v4();
    let archived_session_id = SessionId::new_v4();
    let refreshed_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            default_session_id,
            Some(String::from("project-a")),
            "默认会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: vec![SessionSummary::new(
            archived_session_id,
            Some(String::from("project-a")),
            "归档会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: default_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert_eq!(
        selected_session_id_after_refresh(&refreshed_snapshot, Some(archived_session_id)),
        default_session_id
    );
}

/// Verifies targeted refresh selection survives late in-memory run-state preservation.
/// 验证带目标刷新选择能抵抗后续内存运行态保留的旧选中会话覆盖。
#[test]
fn targeted_refresh_selection_recovers_after_late_run_state_preservation() {
    let old_session_id = SessionId::new_v4();
    let requested_session_id = SessionId::new_v4();
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            old_session_id,
            Some(String::from("project-a")),
            "旧会话",
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
        selected_session: old_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    let preserved_state =
        capture_preserved_workspace_run_state(Some(&previous_snapshot), &BTreeSet::new(), None);
    let mut refreshed_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![
            SessionSummary::new(
                old_session_id,
                Some(String::from("project-a")),
                "旧会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
            SessionSummary::new(
                requested_session_id,
                Some(String::from("project-a")),
                "请求会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
        ],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: requested_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    preserve_active_session_run_state(preserved_state.as_ref(), &mut refreshed_snapshot);
    refreshed_snapshot.selected_session =
        selected_session_id_after_refresh(&refreshed_snapshot, Some(requested_session_id));

    assert_eq!(refreshed_snapshot.selected_session, requested_session_id);
}

/// Verifies hydrated-session selection recovers the requested session after late run-state preservation.
/// 验证完整会话水合选择会在后续运行态保留后恢复请求会话。
#[test]
fn hydrated_session_selection_recovers_after_late_run_state_preservation() {
    let previous_session_id = SessionId::new_v4();
    let hydrated_session_id = SessionId::new_v4();
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            previous_session_id,
            Some(String::from("project-a")),
            "旧选中会话",
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
        selected_session: previous_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    let preserved_state =
        capture_preserved_workspace_run_state(Some(&previous_snapshot), &BTreeSet::new(), None);
    let mut hydrated_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![
            SessionSummary::new(
                previous_session_id,
                Some(String::from("project-a")),
                "旧选中会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
            SessionSummary::new(
                hydrated_session_id,
                Some(String::from("project-a")),
                "正在水合的会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
        ],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: hydrated_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    preserve_active_session_run_state(preserved_state.as_ref(), &mut hydrated_snapshot);
    hydrated_snapshot.selected_session =
        selected_session_id_after_refresh(&hydrated_snapshot, Some(hydrated_session_id));

    assert_eq!(hydrated_snapshot.selected_session, hydrated_session_id);
}

/// Verifies transport conversation identifiers resolve to explicit targets without draft fallbacks.
/// 验证传输层会话标识会解析为显式目标，不会回退为草稿。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if blank, malformed, or nil identifiers become create targets.
/// 此测试无返回值；若空白、畸形或 nil 标识变成新建目标则失败。
#[test]
fn prompt_session_target_parser_rejects_ambiguous_existing_identifiers() {
    // session_id is the valid persisted identity used by the successful continue case.
    // session_id 是成功继续场景使用的有效持久化身份。
    let session_id = SessionId::new_v4();
    // session_id_text is the exact transport representation of the valid identity.
    // session_id_text 是有效身份的精确传输层表示。
    let session_id_text = session_id.to_string();

    assert_eq!(
        parse_prompt_session_target(None).expect("absent id should explicitly create"),
        PromptSessionTarget::Create
    );
    assert_eq!(
        parse_prompt_session_target(Some(&session_id_text))
            .expect("valid id should explicitly continue"),
        PromptSessionTarget::Existing(session_id)
    );
    assert!(parse_prompt_session_target(Some("   ")).is_err());
    assert!(parse_prompt_session_target(Some("draft-local-id")).is_err());
    assert!(parse_prompt_session_target(Some(&SessionId::nil().to_string())).is_err());
}

/// Verifies archived and missing explicit targets cannot materialize replacement sessions.
/// 验证已归档和缺失的显式目标不能物化替代新会话。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails if either rejected target mutates active session state.
/// 此测试无返回值；若任一被拒绝目标修改活动会话状态则失败。
#[test]
fn materialize_prompt_session_rejects_archived_and_missing_targets_without_creation() {
    // archived_session_id identifies the archived row that must never be revived by submission.
    // archived_session_id 标识不得被提交复活的归档行。
    let archived_session_id = SessionId::new_v4();
    // workspace_snapshot contains only one archived regular session and no active session rows.
    // workspace_snapshot 只包含一个归档普通会话，不包含活动会话行。
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            "project-a",
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            Vec::new(),
        )],
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: vec![SessionSummary::new(
            archived_session_id,
            Some(String::from("project-a")),
            "归档会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };

    // archived_error proves an explicit archived target remains archived.
    // archived_error 证明显式归档目标保持归档状态。
    let archived_error = materialize_prompt_session(
        &mut workspace_snapshot,
        Some("project-a"),
        PromptSessionTarget::Existing(archived_session_id),
        "不得创建替代会话",
    )
    .err()
    .expect("archived target should be rejected");
    assert!(archived_error.contains("archived"));
    // missing_session_id identifies a valid UUID that owns no active or archived row.
    // missing_session_id 标识不拥有任何活动或归档行的有效 UUID。
    let missing_session_id = SessionId::new_v4();
    // missing_error proves a syntactically valid but absent target cannot become a draft.
    // missing_error 证明语法有效但缺失的目标不能变成草稿。
    let missing_error = materialize_prompt_session(
        &mut workspace_snapshot,
        Some("project-a"),
        PromptSessionTarget::Existing(missing_session_id),
        "不得创建替代会话",
    )
    .err()
    .expect("missing target should be rejected");
    assert!(missing_error.contains("not found"));
    assert!(workspace_snapshot.sessions.is_empty());
    assert!(workspace_snapshot.task_projects[0].session_ids.is_empty());
}

/// Verifies existing prompt sessions ignore blank project ids during materialization.
/// 验证已有提示词会话在物化时会忽略空白项目标识。
#[test]
fn materialize_prompt_session_ignores_blank_existing_session_project_id() {
    // session_id identifies the existing conversation selected by the WebUI.
    // session_id 标识 WebUI 选择的既有会话。
    let session_id = SessionId::new_v4();
    // workspace_snapshot contains a blank-id project that must not be treated as an owner.
    // workspace_snapshot 包含不应被当作所属项目的空白标识项目。
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            "   ",
            "Blank Project",
            "D:/blank-project",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("   ")),
            "既有会话",
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
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };

    // materialized_session carries the authoritative existing-or-created outcome.
    // materialized_session 携带权威的已有或新建结果。
    let materialized_session = materialize_prompt_session(
        &mut workspace_snapshot,
        None,
        PromptSessionTarget::Existing(session_id),
        "继续这个会话",
    )
    .expect("existing conversation should materialize");
    assert!(materialized_session.project_id.is_none());
    // resolved_session_id is the exact existing session selected by materialization.
    // resolved_session_id 是物化阶段选择的精确已有会话。
    let resolved_session_id = materialized_session.session_id;
    // session_title is the authoritative existing summary title.
    // session_title 是权威已有摘要标题。
    let session_title = materialized_session.session_title;
    // workspace_path is derived only from the existing session's owning project.
    // workspace_path 仅从已有会话所属项目派生。
    let workspace_path = materialized_session.workspace_path;

    assert_eq!(resolved_session_id, session_id);
    assert_eq!(
        materialized_session.kind,
        PromptSessionMaterializationKind::Existing
    );
    assert_eq!(session_title, "既有会话");
    assert_eq!(workspace_path, "");
    assert_eq!(workspace_snapshot.selected_session, session_id);
    assert_eq!(workspace_snapshot.sessions[0].status, SessionState::Running);
}

/// Verifies new prompt sessions fall back to the first active project when no project id is provided.
/// 验证新提示词会话在没有传入项目标识时会回退到第一个活动项目。
#[test]
fn materialize_prompt_session_uses_first_project_when_project_id_is_missing() {
    let first_project_id = String::from("project-a");
    let second_project_id = String::from("project-b");
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: vec![
            TaskProjectSummary::new(
                first_project_id.clone(),
                "Project A",
                "D:/project-a",
                false,
                0,
                None,
                None,
                Vec::new(),
            ),
            TaskProjectSummary::new(
                second_project_id,
                "Project B",
                "D:/project-b",
                false,
                1,
                None,
                None,
                Vec::new(),
            ),
        ],
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };

    // materialized_session carries the authoritative created-session result.
    // materialized_session 携带权威的新建会话结果。
    let materialized_session = materialize_prompt_session(
        &mut workspace_snapshot,
        None,
        PromptSessionTarget::Create,
        "首次启动后创建会话",
    )
    .expect("missing project id should use first active project");
    // session_id identifies the newly created session inserted into the first project.
    // session_id 标识插入第一个项目的新建会话。
    let session_id = materialized_session.session_id;

    assert_eq!(materialized_session.session_title, "首次启动后创建会话");
    assert_eq!(
        materialized_session.project_id.as_deref(),
        Some("project-a")
    );
    assert_eq!(materialized_session.workspace_path, "D:/project-a");
    assert_eq!(
        materialized_session.kind,
        PromptSessionMaterializationKind::Created
    );
    assert_eq!(workspace_snapshot.selected_session, session_id);
    assert_eq!(
        workspace_snapshot.task_projects[0].session_ids,
        vec![session_id]
    );
    assert_eq!(
        session_summary_project_id(&workspace_snapshot.sessions[0]),
        Some(first_project_id.as_str())
    );
}
