//! Protocol replay normalization regression tests.
//! 协议重放归一化回归测试。

use super::projection::normalize_replayed_protocol_messages;
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
        "is_error": false,
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

    let state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");

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

/// Verifies transcript-only recovery preserves the diagnostic that forbids executing malformed tool arguments.
/// 验证仅依赖转录的恢复会保留禁止执行异常工具参数的诊断。
#[test]
fn transcript_replay_preserves_tool_argument_parse_error() {
    // decoderMessage mirrors the controlled protocol failure stored on the visible tool-call row.
    // decoderMessage 镜像保存在可见工具调用行上的受控协议失败。
    let decoder_message = "OpenAI Responses tool input ended before valid JSON was complete";
    // callMetadata extends the ordinary tool identity with the durable pre-execution rejection fact.
    // callMetadata 在普通工具身份上扩展可靠的执行前拒绝事实。
    let mut call_metadata = tool_call_metadata("file_edit", "call-invalid");
    call_metadata["arguments"] = Value::Null;
    call_metadata["argument_parse_error"] = Value::String(decoder_message.to_string());
    let messages = vec![
        transcript_entry(
            "call-row-invalid",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "null",
            Some(call_metadata),
        ),
        transcript_entry(
            "result-row-invalid",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            "Tool was not executed.",
            Some(json!({
                "tool_name": "file_edit",
                "tool_call_id": "call-invalid",
                "is_error": true,
            })),
        ),
    ];

    let state = conversation_state_from_session_messages(&messages)
        .expect("malformed tool diagnostic should reconstruct");
    let SessionConversationPayload::Assistant { tool_calls, .. } = &state.messages[0].payload
    else {
        panic!("first reconstructed message should be the assistant tool call");
    };
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].arguments, Value::Null);
    assert_eq!(
        tool_calls[0].argument_parse_error.as_deref(),
        Some(decoder_message)
    );
}

/// Verifies legacy local result aliases are rewritten to one stable model-visible call id.
/// 验证旧本地结果别名会被改写为一个稳定的模型可见调用标识。
#[test]
fn replay_normalization_canonicalizes_legacy_tool_result_alias() {
    let messages = vec![
        SessionConversationMessage {
            id: String::from("assistant-1"),
            payload: SessionConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![SessionConversationToolCall {
                    id: String::from("runtime-call-1"),
                    tool_call_id: Some(String::from("provider-call-1")),
                    tool_name: String::from("demo"),
                    arguments: json!({}),
                    argument_parse_error: None,
                }],
            },
        },
        SessionConversationMessage {
            id: String::from("result-1"),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: Some(String::from("runtime-call-1")),
                is_error: false,
                text: String::from("ok"),
            },
        },
    ];

    let normalized = normalize_replayed_protocol_messages(messages);

    assert_eq!(normalized.len(), 2);
    let SessionConversationPayload::Assistant { tool_calls, .. } = &normalized[0].payload else {
        panic!("first message should remain an assistant tool-call batch");
    };
    assert_eq!(
        tool_calls[0].tool_call_id.as_deref(),
        Some("provider-call-1")
    );
    assert!(matches!(
        normalized[1].payload,
        SessionConversationPayload::ToolResult {
            ref tool_call_id,
            ..
        } if tool_call_id.as_deref() == Some("provider-call-1")
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

    let state = conversation_state_from_session_messages(&messages)
        .expect("valid replayed conversation identities");

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

    let messages = conversation_state_to_compaction_source_messages(&state)
        .expect("valid compaction source messages");

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

/// Verifies compaction planning receives exact tool text while protocol identity stays structured.
/// 验证压缩规划取得精确工具正文，同时协议身份保持结构化。
#[test]
fn compaction_source_keeps_raw_tool_result_and_structured_identity() {
    let state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("tool-result-runtime-1"),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: Some(String::from("provider-call-1")),
                is_error: true,
                text: String::from("stderr line one\r\nerror conclusion"),
            },
        }],
        records: Vec::new(),
        continuation: SessionProviderContinuationState::default(),
    };

    let messages = conversation_state_to_compaction_source_messages(&state)
        .expect("valid compaction source messages");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "stderr line one\r\nerror conclusion");
    assert_eq!(
        messages[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_call_id"))
            .and_then(Value::as_str),
        Some("provider-call-1")
    );
    assert_eq!(
        messages[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("is_error"))
            .and_then(Value::as_bool),
        Some(true)
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
                kind: vulcan_ai_contract::HostMessageKind::TaskResult,
                message_type: String::from("FINAL_ANSWER"),
                task_id: String::from("task-code-review"),
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

    let messages = conversation_state_to_compaction_source_messages(&state)
        .expect("valid agent message envelope");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "agent-message-1");
    assert_eq!(messages[0].kind, SessionMessageKind::User);
    assert!(messages[0].body.contains("<vulcan_agent_message"));
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

    let messages = conversation_state_to_compaction_source_messages(&state)
        .expect("valid compaction source messages");

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

    let messages = conversation_state_to_compaction_source_messages(&state)
        .expect("valid compaction source messages");

    assert!(messages.is_empty());
}

/// Builds one minimal canonical record for identity-validation tests.
/// 为身份校验测试构建一条最小规范记录。
fn identity_test_record(record_id: &str) -> SessionConversationRecord {
    SessionConversationRecord {
        id: record_id.to_string(),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::new(),
        body: String::new(),
        metadata: None,
        created_at: String::from("1"),
    }
}

/// Verifies canonical record identities reject duplicate ownership instead of renaming records.
/// 验证规范记录身份会拒绝重复归属，而不是重命名记录。
#[test]
fn record_identity_validation_rejects_duplicates() {
    let records = vec![
        identity_test_record("record-1"),
        identity_test_record("record-1"),
    ];

    let error = validate_session_conversation_record_identities(&records)
        .expect_err("duplicate record identity must be rejected");

    assert_eq!(
        error,
        SessionConversationRecordIdentityError::Duplicate {
            record_index: 1,
            record_id: String::from("record-1"),
        }
    );
}

/// Verifies canonical record identities reject empty and padded values.
/// 验证规范记录身份会拒绝空值与带首尾空白的值。
#[test]
fn record_identity_validation_rejects_non_canonical_text() {
    assert!(matches!(
        validate_session_conversation_record_identities(&[identity_test_record("  ")]),
        Err(SessionConversationRecordIdentityError::Empty { record_index: 0 })
    ));
    assert!(matches!(
        validate_session_conversation_record_identities(&[identity_test_record(" record-1 ")]),
        Err(
            SessionConversationRecordIdentityError::SurroundingWhitespace {
                record_index: 0,
                ..
            }
        )
    ));
}

/// Builds one minimal canonical message for identity-validation tests.
/// 为身份校验测试构建一条最小规范消息。
fn identity_test_message(message_id: &str) -> SessionConversationMessage {
    SessionConversationMessage {
        id: message_id.to_string(),
        payload: SessionConversationPayload::UserText {
            text: String::new(),
            attachments: Vec::new(),
        },
    }
}

/// Verifies canonical message identities reject duplicate ownership across the active window.
/// 验证规范消息身份会拒绝活动窗口内的重复归属。
#[test]
fn message_identity_validation_rejects_duplicates() {
    // messages contains the exact conflicting ownership that the validator must reject without mutation.
    // messages 包含校验器必须无修改拒绝的精确冲突归属。
    let messages = vec![
        identity_test_message("message-1"),
        identity_test_message("message-1"),
    ];

    // error captures the later conflicting position and the shared identifier.
    // error 捕获后出现冲突的位置与共享标识。
    let error = validate_session_conversation_message_identities(&messages)
        .expect_err("duplicate message identity must be rejected");

    assert_eq!(
        error,
        SessionConversationMessageIdentityError::Duplicate {
            message_index: 1,
            message_id: String::from("message-1"),
        }
    );
}

/// Verifies canonical message identities reject empty and padded values.
/// 验证规范消息身份会拒绝空值与带首尾空白的值。
#[test]
fn message_identity_validation_rejects_non_canonical_text() {
    assert!(matches!(
        validate_session_conversation_message_identities(&[identity_test_message("  ")]),
        Err(SessionConversationMessageIdentityError::Empty { message_index: 0 })
    ));
    assert!(matches!(
        validate_session_conversation_message_identities(&[identity_test_message(" message-1 ")]),
        Err(
            SessionConversationMessageIdentityError::SurroundingWhitespace {
                message_index: 0,
                ..
            }
        )
    ));
}

/// Verifies user-message append rejects a duplicate identity without changing the active window.
/// 验证用户消息追加会拒绝重复身份且不修改活动窗口。
#[test]
fn user_message_append_is_atomic_on_identity_failure() {
    // state owns one valid message whose identity the candidate attempts to reuse.
    // state 持有一条有效消息，候选项将尝试复用其身份。
    let mut state = SessionConversationState {
        messages: vec![identity_test_message("message-1")],
        ..SessionConversationState::default()
    };
    // original_state captures every canonical field before the rejected mutation.
    // original_state 捕获被拒绝修改前的全部规范字段。
    let original_state = serde_json::to_value(&state).expect("state should serialize");

    // error proves the append boundary reports the precise duplicate instead of committing it.
    // error 证明追加边界会报告准确重复项，而不是提交它。
    let error = append_user_text_message(&mut state, "message-1", "duplicate", Vec::new())
        .expect_err("duplicate append must fail");

    assert!(matches!(
        error,
        SessionConversationMessageIdentityError::Duplicate {
            message_index: 1,
            ..
        }
    ));
    assert_eq!(
        serde_json::to_value(&state).expect("state should serialize after failure"),
        original_state
    );
}

/// Verifies record append rejects a duplicate identity without changing canonical state.
/// 验证记录追加会拒绝重复身份且不修改规范状态。
#[test]
fn record_append_is_atomic_on_identity_failure() {
    // record is the valid retained row whose identity the candidate attempts to reuse.
    // record 是有效保留行，候选记录将尝试复用其身份。
    let record = SessionConversationRecord {
        id: String::from("record-1"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("Assistant"),
        body: String::from("first"),
        metadata: None,
        created_at: String::from("1"),
    };
    // state starts with one valid record and must remain byte-equivalent after rejection.
    // state 初始包含一条有效记录，拒绝后必须保持字节等价。
    let mut state = SessionConversationState {
        records: vec![record.clone()],
        ..SessionConversationState::default()
    };
    let original_state = serde_json::to_value(&state).expect("state should serialize");

    let error = append_record(&mut state, record).expect_err("duplicate append must fail");

    assert!(matches!(
        error,
        SessionConversationRecordIdentityError::Duplicate {
            record_index: 1,
            ..
        }
    ));
    assert_eq!(
        serde_json::to_value(&state).expect("state should serialize after failure"),
        original_state
    );
}

/// Verifies transcript replay rejects duplicate record identities before returning partial state.
/// 验证转录复放会拒绝重复记录身份，且不会返回局部状态。
#[test]
fn transcript_replay_rejects_duplicate_record_identity() {
    let messages = [
        transcript_entry(
            "record-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "first",
            None,
        ),
        transcript_entry(
            "record-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "duplicate",
            None,
        ),
    ];

    let error = conversation_state_from_session_messages(&messages)
        .expect_err("duplicate replay must fail");

    assert!(error.contains("conversation record 1 duplicates id \"record-1\""));
}
