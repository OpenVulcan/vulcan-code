//! Protocol replay normalization regression tests.
//! 协议重放归一化回归测试。

use super::*;
use crate::transcript::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use serde_json::{Value, json};

/// Builds one transcript entry for replay-normalization tests.
/// 为重放归一化测试构建一条转录项。
fn transcript_entry(
    id: &str,
    kind: SessionMessageKind,
    record_kind: SessionMessageRecordKind,
    body: &str,
    metadata: Option<Value>,
) -> SessionMessageEntry {
    SessionMessageEntry {
        id: id.to_string(),
        kind,
        record_kind,
        visible: true,
        title: String::new(),
        body: body.to_string(),
        metadata,
        created_at: String::from("1"),
    }
}

/// Builds metadata for one visible tool-call transcript row.
/// 为一条可见工具调用转录行构建元数据。
fn tool_call_metadata(tool_name: &str, tool_call_id: &str) -> Value {
    json!({
        "tool_name": tool_name,
        "tool_call_id": tool_call_id,
        "arguments": { "value": tool_call_id }
    })
}

/// Builds metadata for one visible tool-result transcript row.
/// 为一条可见工具结果转录行构建元数据。
fn tool_result_metadata(tool_name: &str, tool_call_id: &str) -> Value {
    json!({
        "tool_name": tool_name,
        "tool_call_id": tool_call_id,
        "tool_result_id": format!("tool-result-{tool_call_id}")
    })
}

/// Verifies display rows with parallel tool calls replay as one legal assistant batch.
/// 验证展示层并行工具调用行会重放为一个合法助手批次。
#[test]
fn transcript_replay_groups_parallel_tool_rows_into_one_batch() {
    let messages = vec![
        transcript_entry(
            "user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "run tools",
            None,
        ),
        transcript_entry(
            "call-row-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "{}",
            Some(tool_call_metadata("first_tool", "call-1")),
        ),
        transcript_entry(
            "call-row-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "{}",
            Some(tool_call_metadata("second_tool", "call-2")),
        ),
        transcript_entry(
            "result-row-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            "first result",
            Some(tool_result_metadata("first_tool", "call-1")),
        ),
        transcript_entry(
            "result-row-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            "second result",
            Some(tool_result_metadata("second_tool", "call-2")),
        ),
    ];

    let state = conversation_state_from_session_messages(&messages);

    assert_eq!(state.messages.len(), 4);
    let SessionConversationPayload::Assistant { tool_calls, .. } = &state.messages[1].payload
    else {
        panic!("second message should be a grouped assistant tool batch");
    };
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0].tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(tool_calls[1].tool_call_id.as_deref(), Some("call-2"));
    assert!(matches!(
        state.messages[2].payload,
        SessionConversationPayload::ToolResult { .. }
    ));
    assert!(matches!(
        state.messages[3].payload,
        SessionConversationPayload::ToolResult { .. }
    ));
}

/// Verifies interrupted tool calls replay as plain historical text before the next user turn.
/// 验证被中断的工具调用会在下一条用户回合前重放为普通历史文本。
#[test]
fn transcript_replay_downgrades_interrupted_tool_row_to_text() {
    let messages = vec![
        transcript_entry(
            "call-row-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "{}",
            Some(tool_call_metadata("first_tool", "call-1")),
        ),
        transcript_entry(
            "user-2",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "continue after interruption",
            None,
        ),
    ];

    let state = conversation_state_from_session_messages(&messages);

    assert_eq!(state.messages.len(), 2);
    let SessionConversationPayload::UserText { text, attachments } = &state.messages[0].payload
    else {
        panic!("interrupted tool call should be downgraded to user text");
    };
    assert!(attachments.is_empty());
    assert!(text.contains("Historical tool call"));
    assert!(text.contains("call-1"));
    let SessionConversationPayload::UserText { text, .. } = &state.messages[1].payload else {
        panic!("next message should remain user text");
    };
    assert_eq!(text, "continue after interruption");
}

/// Verifies runtime display-record projection updates rows without dropping loaded history rows.
/// 验证运行态显示记录投影会更新行，同时不会丢弃已加载的历史行。
#[test]
fn merge_session_messages_with_conversation_state_records_preserves_loaded_history() {
    // existing_messages simulates the display window loaded from raw segmented turns.
    // existing_messages 模拟从原始分段 turn 加载出的显示窗口。
    let existing_messages = vec![
        transcript_entry(
            "old-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "之前的问题",
            None,
        ),
        transcript_entry(
            "old-assistant",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::Transcript,
            "之前的回答",
            None,
        ),
    ];
    // runtime_state carries only the newly materialized visible record for the active turn.
    // runtime_state 只携带当前回合新物化的可见记录。
    let runtime_state = conversation_state_from_session_messages(&[transcript_entry(
        "new-user",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "继续提问",
        None,
    )]);

    let merged_messages =
        merge_session_messages_with_conversation_state_records(&existing_messages, &runtime_state);

    assert_eq!(merged_messages.len(), 3);
    assert_eq!(merged_messages[0].body, "之前的问题");
    assert_eq!(merged_messages[1].body, "之前的回答");
    assert_eq!(merged_messages[2].body, "继续提问");
}

/// Verifies runtime records restore thinking/tool interleaving after tool rows reached the display layer first.
/// 验证工具行先进入显示层后，运行态记录会恢复思考与工具交错顺序。
#[test]
fn merge_session_messages_with_conversation_state_records_restores_runtime_order() {
    // existing_messages mirrors the incomplete display order produced before checkpoint reconciliation.
    // existing_messages 模拟 checkpoint 对账前产生的不完整显示顺序。
    let existing_messages = vec![
        transcript_entry(
            "old-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "再测试下",
            None,
        ),
        transcript_entry(
            "tool-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "工具 1 旧状态",
            None,
        ),
        transcript_entry(
            "tool-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "工具 2 旧状态",
            None,
        ),
        transcript_entry(
            "display-tail",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::TurnStats,
            "显示层尾部统计",
            None,
        ),
    ];
    // runtime_state carries the canonical order collected by the execution runtime.
    // runtime_state 携带执行运行时收集到的规范顺序。
    let runtime_state = conversation_state_from_session_messages(&[
        transcript_entry(
            "thinking-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ThinkingOutput,
            "第一段思考",
            None,
        ),
        transcript_entry(
            "tool-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "工具 1 已完成",
            None,
        ),
        transcript_entry(
            "thinking-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ThinkingOutput,
            "第二段思考",
            None,
        ),
        transcript_entry(
            "tool-2",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "工具 2 已完成",
            None,
        ),
    ]);

    // merged_messages must use runtime order from the first shared tool while retaining unrelated history and tail rows.
    // merged_messages 必须从首个共享工具开始采用运行态顺序，同时保留无关历史与尾部记录。
    let merged_messages =
        merge_session_messages_with_conversation_state_records(&existing_messages, &runtime_state);

    assert_eq!(
        merged_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "old-user",
            "thinking-1",
            "tool-1",
            "thinking-2",
            "tool-2",
            "display-tail",
        ]
    );
    assert_eq!(merged_messages[2].body, "工具 1 已完成");
    assert_eq!(merged_messages[4].body, "工具 2 已完成");
}

/// Verifies compaction planning projects runtime-log messages.
/// 验证压缩规划会投影运行时日志消息。
#[test]
fn compaction_source_projects_runtime_messages() {
    let state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("user-runtime-1"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("运行时用户问题"),
                    attachments: Vec::new(),
                },
            },
            SessionConversationMessage {
                id: String::from("assistant-runtime-1"),
                payload: SessionConversationPayload::Assistant {
                    text: String::from("运行时助手回答"),
                    reasoning: String::from("简短推理"),
                    provider_message_id: Some(String::from("provider-runtime-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
        ],
        records: Vec::new(),
        continuation: SessionProviderContinuationState::default(),
    };

    let messages = conversation_state_to_compaction_source_messages(&state);

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, "user-runtime-1");
    assert_eq!(messages[0].kind, SessionMessageKind::User);
    assert!(messages[0].body.contains("运行时用户问题"));
    assert_eq!(messages[1].kind, SessionMessageKind::Assistant);
    assert!(messages[1].body.contains("运行时助手回答"));
    assert!(messages[1].body.contains("简短推理"));
    assert_eq!(
        messages[1]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("provider_message_id"))
            .and_then(Value::as_str),
        Some("provider-runtime-1")
    );
}

/// Verifies agent mailbox messages keep their provider-facing user role during compaction.
/// 验证代理 mailbox 消息在压缩来源中保持面向 Provider 的用户角色。
#[test]
fn compaction_source_projects_agent_messages_as_user_messages() {
    let state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("agent-message-1"),
            payload: SessionConversationPayload::AgentMessage {
                message_type: String::from("FINAL_ANSWER"),
                task_name: String::from("code-review"),
                sender: String::from("code-review"),
                recipient: String::from("parent"),
                payload: String::from("review complete"),
                trigger_turn: true,
                metadata: None,
            },
        }],
        records: Vec::new(),
        continuation: SessionProviderContinuationState::default(),
    };

    let messages = conversation_state_to_compaction_source_messages(&state);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "agent-message-1");
    assert_eq!(messages[0].kind, SessionMessageKind::User);
    assert!(messages[0].body.contains("<agent_message>"));
    assert!(messages[0].body.contains("review complete"));
}

/// Verifies display records cannot become compaction source messages.
/// 验证显示记录不能成为压缩来源消息。
#[test]
fn compaction_source_ignores_display_records() {
    let mut state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("runtime-user"),
            payload: SessionConversationPayload::UserText {
                text: String::from("运行时正文"),
                attachments: Vec::new(),
            },
        }],
        records: Vec::new(),
        continuation: SessionProviderContinuationState::default(),
    };
    state.records.push(SessionConversationRecord {
        id: String::from("record-user"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("USER"),
        body: String::from("记录流正文"),
        metadata: None,
        created_at: String::new(),
    });

    let messages = conversation_state_to_compaction_source_messages(&state);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "runtime-user");
    assert_eq!(messages[0].body, "运行时正文");
}

/// Verifies record-only states do not produce compaction sources.
/// 验证仅有记录流的状态不会产出压缩来源。
#[test]
fn compaction_source_rejects_record_only_state() {
    let mut state = SessionConversationState::default();
    state.records.push(SessionConversationRecord {
        id: String::from("record-user"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("USER"),
        body: String::from("记录流正文"),
        metadata: None,
        created_at: String::new(),
    });

    let messages = conversation_state_to_compaction_source_messages(&state);

    assert!(messages.is_empty());
}
