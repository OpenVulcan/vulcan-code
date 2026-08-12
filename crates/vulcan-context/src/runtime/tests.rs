//! Runtime log projection regression tests.
//! 运行时日志投影回归测试。

use super::*;
use crate::protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationState,
    SessionConversationToolCall,
};
use serde_json::{Value, json};

/// Builds one assistant tool call for projection tests.
/// 为投影测试构建一条助手工具调用。
fn tool_call(id: &str) -> SessionConversationToolCall {
    SessionConversationToolCall {
        id: id.to_string(),
        tool_call_id: Some(id.to_string()),
        tool_name: String::from("demo"),
        arguments: json!({ "value": 1 }),
    }
}

/// Builds one deterministic assistant runtime line for strict replay tests.
/// 为严格重放测试构建一条确定性助手运行时日志行。
///
/// # Parameters
/// 参数
/// - `seq`: Runtime sequence assigned to the line.
/// - `seq`：分配给日志行的运行时序号。
/// - `message_id`: Stable local assistant message identifier.
/// - `message_id`：稳定本地助手消息标识。
///
/// # Returns
/// 返回值
/// A schema-current assistant runtime event line.
/// 返回一条使用当前结构版本的助手运行时事件行。
fn runtime_assistant_line(seq: u64, message_id: &str) -> RuntimeLogEventLine {
    RuntimeLogEventLine::new(
        seq,
        "1",
        RuntimeLogEvent::AssistantMessage(RuntimeAssistantMessageEvent {
            turn_id: Some(message_id.to_string()),
            message_id: message_id.to_string(),
            text: String::new(),
            reasoning: String::new(),
            provider_id: None,
            model_id: None,
            provider_message_id: None,
            provider_message_state: None,
            usage: None,
        }),
    )
}

/// Builds one deterministic user runtime line for strict replay tests.
/// 为严格重放测试构建一条确定性用户运行时日志行。
///
/// # Parameters
/// 参数
/// - `seq`: Runtime sequence assigned to the line.
/// - `seq`：分配给日志行的运行时序号。
/// - `message_id`: Stable local user message identifier.
/// - `message_id`：稳定本地用户消息标识。
///
/// # Returns
/// 返回值
/// A schema-current user runtime event line.
/// 返回一条使用当前结构版本的用户运行时事件行。
fn runtime_user_line(seq: u64, message_id: &str) -> RuntimeLogEventLine {
    RuntimeLogEventLine::new(
        seq,
        "1",
        RuntimeLogEvent::UserMessage(RuntimeUserMessageEvent {
            turn_id: Some(message_id.to_string()),
            message_id: message_id.to_string(),
            text: String::from("prompt"),
            attachments: Vec::new(),
        }),
    )
}

/// Builds one deterministic tool-batch start line for strict replay tests.
/// 为严格重放测试构建一条确定性工具批次开始日志行。
///
/// # Parameters
/// 参数
/// - `seq`: Runtime sequence assigned to the line.
/// - `seq`：分配给日志行的运行时序号。
/// - `tool_batch_id`: Stable runtime batch identifier.
/// - `tool_batch_id`：稳定运行时批次标识。
/// - `assistant_message_id`: Assistant message that owns the batch.
/// - `assistant_message_id`：拥有该批次的助手消息。
///
/// # Returns
/// 返回值
/// A schema-current tool-batch start runtime line.
/// 返回一条使用当前结构版本的工具批次开始运行时日志行。
fn runtime_tool_batch_started_line(
    seq: u64,
    tool_batch_id: &str,
    assistant_message_id: &str,
) -> RuntimeLogEventLine {
    RuntimeLogEventLine::new(
        seq,
        "1",
        RuntimeLogEvent::ToolBatchStarted(RuntimeToolBatchStartedEvent {
            turn_id: Some(assistant_message_id.to_string()),
            tool_batch_id: tool_batch_id.to_string(),
            assistant_message_id: assistant_message_id.to_string(),
        }),
    )
}

/// Builds one deterministic tool-batch completion line for strict replay tests.
/// 为严格重放测试构建一条确定性工具批次完成日志行。
///
/// # Parameters
/// 参数
/// - `seq`: Runtime sequence assigned to the line.
/// - `seq`：分配给日志行的运行时序号。
/// - `tool_batch_id`: Stable runtime batch identifier.
/// - `tool_batch_id`：稳定运行时批次标识。
/// - `tool_call_count`: Call count declared by the completion event.
/// - `tool_call_count`：完成事件声明的调用数量。
/// - `tool_result_count`: Matched-result count declared by the completion event.
/// - `tool_result_count`：完成事件声明的已匹配结果数量。
/// - `complete`: Whether the event declares every call matched.
/// - `complete`：事件是否声明每个调用均已匹配。
///
/// # Returns
/// 返回值
/// A schema-current tool-batch completion runtime line.
/// 返回一条使用当前结构版本的工具批次完成运行时日志行。
fn runtime_tool_batch_completed_line(
    seq: u64,
    tool_batch_id: &str,
    tool_call_count: usize,
    tool_result_count: usize,
    complete: bool,
) -> RuntimeLogEventLine {
    RuntimeLogEventLine::new(
        seq,
        "1",
        RuntimeLogEvent::ToolBatchCompleted(RuntimeToolBatchCompletedEvent {
            turn_id: None,
            tool_batch_id: tool_batch_id.to_string(),
            tool_call_count,
            tool_result_count,
            complete,
        }),
    )
}

/// Builds one deterministic tool-call runtime line for strict replay tests.
/// 为严格重放测试构建一条确定性工具调用运行时日志行。
///
/// # Parameters
/// 参数
/// - `seq`: Runtime sequence assigned to the line.
/// - `seq`：分配给日志行的运行时序号。
/// - `assistant_message_id`: Assistant message that owns the call.
/// - `assistant_message_id`：拥有该调用的助手消息。
/// - `runtime_tool_call_id`: Stable runtime-owned call identifier.
/// - `runtime_tool_call_id`：稳定运行时自有调用标识。
/// - `provider_tool_call_id`: Optional provider-visible call identifier.
/// - `provider_tool_call_id`：可选 Provider 可见调用标识。
/// - `tool_batch_id`: Runtime batch that owns the call.
/// - `tool_batch_id`：拥有该调用的运行时批次。
///
/// # Returns
/// 返回值
/// A schema-current tool-call runtime event line.
/// 返回一条使用当前结构版本的工具调用运行时事件行。
fn runtime_tool_call_line(
    seq: u64,
    assistant_message_id: &str,
    runtime_tool_call_id: &str,
    provider_tool_call_id: Option<&str>,
    tool_batch_id: &str,
) -> RuntimeLogEventLine {
    RuntimeLogEventLine::new(
        seq,
        "1",
        RuntimeLogEvent::ToolCall(RuntimeToolCallEvent {
            turn_id: Some(assistant_message_id.to_string()),
            tool_batch_id: tool_batch_id.to_string(),
            assistant_message_id: assistant_message_id.to_string(),
            runtime_tool_call_id: runtime_tool_call_id.to_string(),
            provider_tool_call_id: provider_tool_call_id.map(str::to_string),
            tool_name: String::from("demo"),
            arguments: json!({}),
        }),
    )
}

/// Builds one deterministic tool-result runtime line for strict replay tests.
/// 为严格重放测试构建一条确定性工具结果运行时日志行。
///
/// # Parameters
/// 参数
/// - `seq`: Runtime sequence assigned to the line.
/// - `seq`：分配给日志行的运行时序号。
/// - `message_id`: Stable local tool-result message identifier.
/// - `message_id`：稳定本地工具结果消息标识。
/// - `runtime_tool_call_id`: Optional runtime-owned call identifier.
/// - `runtime_tool_call_id`：可选运行时自有调用标识。
/// - `provider_tool_call_id`: Optional provider-visible call identifier.
/// - `provider_tool_call_id`：可选 Provider 可见调用标识。
/// - `tool_batch_id`: Optional batch declared by the result.
/// - `tool_batch_id`：结果声明的可选批次。
/// - `status`: Matched, failed, or explicitly orphaned result status.
/// - `status`：已匹配、失败或明确孤儿结果状态。
///
/// # Returns
/// 返回值
/// A schema-current tool-result runtime event line.
/// 返回一条使用当前结构版本的工具结果运行时事件行。
fn runtime_tool_result_line(
    seq: u64,
    message_id: &str,
    runtime_tool_call_id: Option<&str>,
    provider_tool_call_id: Option<&str>,
    tool_batch_id: Option<&str>,
    status: RuntimeToolResultStatus,
) -> RuntimeLogEventLine {
    RuntimeLogEventLine::new(
        seq,
        "1",
        RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
            turn_id: Some(message_id.to_string()),
            message_id: message_id.to_string(),
            tool_batch_id: tool_batch_id.map(str::to_string),
            runtime_tool_call_id: runtime_tool_call_id.map(str::to_string),
            provider_tool_call_id: provider_tool_call_id.map(str::to_string),
            text: String::from("result"),
            status,
            error: None,
        }),
    )
}

#[test]
fn runtime_log_projection_preserves_parallel_tool_batch() {
    let state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("user-1"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("run"),
                    attachments: Vec::new(),
                },
            },
            SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![tool_call("call-a"), tool_call("call-b")],
                },
            },
            SessionConversationMessage {
                id: String::from("result-a"),
                payload: SessionConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("call-a")),
                    is_error: false,
                    text: String::from("ok"),
                },
            },
            SessionConversationMessage {
                id: String::from("result-b"),
                payload: SessionConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("call-b")),
                    is_error: false,
                    text: String::from("ok"),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    };

    let lines = runtime_log_lines_from_conversation_state(&state, "1", 1)
        .expect("project parallel tool batch");
    let batch_started = lines
        .iter()
        .filter(|line| matches!(line.event, RuntimeLogEvent::ToolBatchStarted(_)))
        .count();
    let batch_completed = lines
        .iter()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::ToolBatchCompleted(event) => Some(event),
            _ => None,
        })
        .expect("batch completed");

    assert_eq!(batch_started, 1);
    assert_eq!(batch_completed.tool_call_count, 2);
    assert_eq!(batch_completed.tool_result_count, 2);
    assert!(batch_completed.complete);
}

#[test]
fn runtime_log_projection_marks_orphan_tool_result() {
    let state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("result-x"),
            payload: SessionConversationPayload::ToolResult {
                tool_call_id: Some(String::from("missing")),
                is_error: false,
                text: String::from("orphan"),
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };

    let lines = runtime_log_lines_from_conversation_state(&state, "1", 1)
        .expect("project orphan tool result");
    let status = lines
        .iter()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::ToolResult(event) => Some(&event.status),
            _ => None,
        })
        .expect("tool result");

    assert_eq!(status, &RuntimeToolResultStatus::Orphan);
}

/// Verifies sequence exhaustion rejects the complete projection instead of duplicating `u64::MAX`.
/// 验证序号耗尽会拒绝完整投影，而不是重复使用 `u64::MAX`。
#[test]
fn runtime_log_projection_rejects_sequence_exhaustion() {
    // state requires two event sequences so the second append reaches the exhausted boundary.
    // state 需要两个事件序号，使第二次追加到达耗尽边界。
    let state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("user-1"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("first"),
                    attachments: Vec::new(),
                },
            },
            SessionConversationMessage {
                id: String::from("user-2"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("second"),
                    attachments: Vec::new(),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    };

    // error proves the infallible projection contract has been replaced by explicit exhaustion.
    // error 证明原先永不失败的投影契约已被明确耗尽错误取代。
    let error = runtime_log_lines_from_conversation_state(&state, "1", u64::MAX - 1)
        .expect_err("projection must reject an unrepresentable next sequence");

    assert_eq!(
        error,
        RuntimeLogProjectionError::SequenceExhausted { sequence: u64::MAX }
    );
}

/// Verifies duplicate runtime tool-call identifiers fail before a batch can be returned.
/// 验证重复运行时工具调用标识会在批次可返回前失败。
#[test]
fn runtime_log_projection_rejects_duplicate_tool_call_identifiers() {
    // state contains two calls that resolve to the same provider-visible identity.
    // state 包含两个解析为相同 Provider 可见身份的调用。
    let state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("assistant-1"),
            payload: SessionConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![tool_call("call-a"), tool_call("call-a")],
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };

    // error preserves the exact conflicting normalized identifier.
    // error 保留发生冲突的精确规范化标识。
    let error = runtime_log_lines_from_conversation_state(&state, "1", 1)
        .expect_err("duplicate tool-call identifiers must fail projection");

    assert_eq!(
        error,
        RuntimeLogProjectionError::DuplicateToolCallIdentifier {
            tool_call_id: String::from("call-a"),
        }
    );
}

/// Verifies a tool call without either stable identifier is rejected instead of emitting an empty id.
/// 验证同时缺少两类稳定标识的工具调用会被拒绝，而不是发出空标识。
#[test]
fn runtime_log_projection_rejects_missing_tool_call_identifier() {
    // malformed_call has no provider id and only whitespace in its local stable id.
    // malformed_call 没有 Provider 标识，且本地稳定标识只有空白。
    let malformed_call = SessionConversationToolCall {
        id: String::from("   "),
        tool_call_id: None,
        tool_name: String::from("demo"),
        arguments: json!({}),
    };
    // state places the malformed call on the only assistant message.
    // state 将异常调用放在唯一助手消息中。
    let state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("assistant-1"),
            payload: SessionConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: vec![malformed_call],
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };

    // error identifies the malformed tool without manufacturing a candidate call id.
    // error 标识异常工具，且不会臆造候选调用标识。
    let error = runtime_log_lines_from_conversation_state(&state, "1", 1)
        .expect_err("missing tool-call identity must fail projection");

    assert_eq!(
        error,
        RuntimeLogProjectionError::MissingToolCallIdentifier {
            tool_name: String::from("demo"),
        }
    );
}

/// Verifies blank provider ids fall back to a normalized local id without persisting blank identity.
/// 验证空白 Provider 标识会回退到规范化本地标识，且不会持久化空白身份。
#[test]
fn runtime_log_projection_normalizes_tool_call_identity() {
    // state pairs a blank provider call id with a whitespace-padded stable local id and result id.
    // state 将空白 Provider 调用标识与带空白的稳定本地标识及结果标识配对。
    let state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![SessionConversationToolCall {
                        id: String::from("  local-call  "),
                        tool_call_id: Some(String::from("   ")),
                        tool_name: String::from("demo"),
                        arguments: json!({}),
                    }],
                },
            },
            SessionConversationMessage {
                id: String::from("result-1"),
                payload: SessionConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("  local-call  ")),
                    is_error: false,
                    text: String::from("ok"),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    };

    // lines expose normalized identities on both the call and result events.
    // lines 在调用与结果事件上暴露规范化身份。
    let lines = runtime_log_lines_from_conversation_state(&state, "1", 1)
        .expect("project normalized tool-call identity");
    // projected_call is the emitted tool call after strict identity resolution.
    // projected_call 是严格身份解析后发出的工具调用。
    let projected_call = lines
        .iter()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::ToolCall(event) => Some(event),
            _ => None,
        })
        .expect("projected tool call");
    // projected_result is the matching emitted result after lookup normalization.
    // projected_result 是查找规范化后发出的匹配结果。
    let projected_result = lines
        .iter()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::ToolResult(event) => Some(event),
            _ => None,
        })
        .expect("projected tool result");

    assert_eq!(projected_call.runtime_tool_call_id, "local-call");
    assert_eq!(projected_call.provider_tool_call_id, None);
    assert_eq!(
        projected_result.runtime_tool_call_id.as_deref(),
        Some("local-call")
    );
    assert_eq!(
        projected_result.provider_tool_call_id.as_deref(),
        Some("local-call")
    );
    assert_eq!(projected_result.status, RuntimeToolResultStatus::Completed);
}

/// Verifies repeated canonical message ids are rejected before runtime index entries can collide.
/// 验证重复规范消息标识会在运行时索引条目发生冲突前被拒绝。
#[test]
fn runtime_log_projection_rejects_duplicate_message_identifiers() {
    // state reuses one id across assistant and user payload kinds.
    // state 在助手与用户载荷类型之间复用同一标识。
    let state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: SessionConversationPayload::UserText {
                    text: String::from("conflicting user"),
                    attachments: Vec::new(),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    };

    // error identifies the assistant id that would otherwise create ambiguous replay ownership.
    // error 标识原本会造成重放归属歧义的助手标识。
    let error = runtime_log_lines_from_conversation_state(&state, "1", 1)
        .expect_err("duplicate assistant identifiers must fail projection");

    assert_eq!(
        error,
        RuntimeLogProjectionError::DuplicateMessageIdentifier {
            message_id: String::from("assistant-1"),
        }
    );
}

/// Verifies canonical runtime projection rejects missing and whitespace-padded message identities.
/// 验证规范运行时投影会拒绝缺失及带首尾空白的消息身份。
#[test]
fn runtime_log_projection_rejects_noncanonical_message_identifiers() {
    // missing_state carries no stable local identity for its user message.
    // missing_state 的用户消息没有稳定本地身份。
    let missing_state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from("   "),
            payload: SessionConversationPayload::UserText {
                text: String::from("prompt"),
                attachments: Vec::new(),
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };
    // padded_state proves projection never silently rewrites a caller-owned identity.
    // padded_state 证明投影绝不会静默改写调用方持有的身份。
    let padded_state = SessionConversationState {
        messages: vec![SessionConversationMessage {
            id: String::from(" user-1 "),
            payload: SessionConversationPayload::UserText {
                text: String::from("prompt"),
                attachments: Vec::new(),
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };

    let missing_error = runtime_log_lines_from_conversation_state(&missing_state, "1", 1)
        .expect_err("blank canonical message id must fail projection");
    let padded_error = runtime_log_lines_from_conversation_state(&padded_state, "1", 1)
        .expect_err("padded canonical message id must fail projection");

    assert_eq!(
        missing_error,
        RuntimeLogProjectionError::MissingMessageIdentifier {
            message_kind: "user",
        }
    );
    assert_eq!(
        padded_error,
        RuntimeLogProjectionError::NonCanonicalMessageIdentifier {
            message_kind: "user",
            message_id: String::from(" user-1 "),
        }
    );
}

/// Verifies one local message id is the only canonical identity of a tool-result message.
/// 验证一个本地消息标识是工具结果消息唯一的规范身份。
#[test]
fn runtime_log_round_trip_uses_single_tool_result_message_identifier() {
    // state identifies the result entity once and links it separately to its owning tool call.
    // state 只标识一次结果实体，并通过独立字段关联所属工具调用。
    let state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![tool_call("call-1")],
                },
            },
            SessionConversationMessage {
                id: String::from("message-result-1"),
                payload: SessionConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("call-1")),
                    is_error: false,
                    text: String::from("done"),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    };

    let lines = runtime_log_lines_from_conversation_state(&state, "1", 1)
        .expect("project tool-result message identity");
    let projected_result = lines
        .iter()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::ToolResult(event) => Some(event),
            _ => None,
        })
        .expect("tool result event");
    let replayed = conversation_state_from_runtime_log_lines(&lines)
        .expect("replay tool-result message identity");
    // serialized_result proves the clean runtime v1 schema has one canonical result identity.
    // serialized_result 证明干净 runtime v1 结构只有一个规范结果身份。
    let serialized_result = serde_json::to_value(projected_result).expect("serialize tool result");

    assert_eq!(projected_result.message_id, "message-result-1");
    assert!(serialized_result.get("tool_result_id").is_none());
    assert!(
        lines
            .iter()
            .all(|line| line.schema_version == RUNTIME_LOG_SCHEMA_VERSION)
    );
    assert_eq!(replayed.messages[1].id, "message-result-1");
    assert!(matches!(
        &replayed.messages[1].payload,
        SessionConversationPayload::ToolResult { tool_call_id, .. }
            if tool_call_id.as_deref() == Some("call-1")
    ));
}

/// Verifies each tool call consumes at most one completed result inside its batch.
/// 验证每个工具调用在所属批次内最多消费一条已完成结果。
#[test]
fn runtime_log_projection_marks_repeated_tool_result_as_orphan() {
    // state repeats a result for the same call after its first successful match.
    // state 在同一调用首次成功匹配后重复提供结果。
    let state = SessionConversationState {
        messages: vec![
            SessionConversationMessage {
                id: String::from("assistant-1"),
                payload: SessionConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![tool_call("call-a")],
                },
            },
            SessionConversationMessage {
                id: String::from("result-a"),
                payload: SessionConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("call-a")),
                    is_error: false,
                    text: String::from("first"),
                },
            },
            SessionConversationMessage {
                id: String::from("result-a-duplicate"),
                payload: SessionConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("call-a")),
                    is_error: false,
                    text: String::from("duplicate"),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    };

    // lines contain one completed match, one orphan, and one exact batch summary.
    // lines 包含一次完成匹配、一个孤儿结果和一条精确批次汇总。
    let lines = runtime_log_lines_from_conversation_state(&state, "1", 1)
        .expect("project repeated tool result");
    // statuses preserve result order for exact first-match assertions.
    // statuses 保留结果顺序，以精确断言首次匹配语义。
    let statuses = lines
        .iter()
        .filter_map(|line| match &line.event {
            RuntimeLogEvent::ToolResult(event) => Some(event.status.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    // completed summarizes unique calls and their single consumed result.
    // completed 汇总唯一调用及其消费的单条结果。
    let completed = lines
        .iter()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::ToolBatchCompleted(event) => Some(event),
            _ => None,
        })
        .expect("completed tool batch");

    assert_eq!(
        statuses,
        vec![
            RuntimeToolResultStatus::Completed,
            RuntimeToolResultStatus::Orphan,
        ]
    );
    assert_eq!(completed.tool_call_count, 1);
    assert_eq!(completed.tool_result_count, 1);
    assert!(completed.complete);
}

/// Verifies agent mailbox messages are replayed as provider-facing user messages.
/// 验证代理 mailbox 消息会按面向 Provider 的用户消息回放。
#[test]
fn runtime_log_projection_projects_agent_messages_as_user_messages() {
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
        continuation: Default::default(),
    };

    let lines =
        runtime_log_lines_from_conversation_state(&state, "1", 1).expect("project agent message");
    let message = lines
        .iter()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::UserMessage(event) => Some(event),
            _ => None,
        })
        .expect("agent message should project as user message");

    assert_eq!(message.message_id, "agent-message-1");
    assert!(message.text.contains("<agent_message>"));
    assert!(message.text.contains("review complete"));
    assert!(
        !lines
            .iter()
            .any(|line| matches!(line.event, RuntimeLogEvent::AssistantMessage(_)))
    );
}

/// Verifies replay rejects unsupported schemas and non-contiguous sequence order.
/// 验证重放会拒绝不受支持的结构版本与不连续序号顺序。
#[test]
fn runtime_log_replay_rejects_schema_and_sequence_corruption() {
    // unsupported_line carries a schema version outside the current replay contract.
    // unsupported_line 携带当前重放契约之外的结构版本。
    let mut unsupported_line = runtime_assistant_line(7, "assistant-1");
    unsupported_line.schema_version = RUNTIME_LOG_SCHEMA_VERSION + 1;
    // schema_error proves schema validation happens before event application.
    // schema_error 证明结构版本验证发生在事件应用前。
    let schema_error = conversation_state_from_runtime_log_lines(&[unsupported_line])
        .expect_err("unsupported runtime schema must fail replay");
    // sequence_error captures the exact second-line gap from an arbitrary valid starting sequence.
    // sequence_error 捕获从任意有效起始序号开始的精确第二行缺口。
    let sequence_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(7, "assistant-1"),
        runtime_assistant_line(9, "assistant-2"),
    ])
    .expect_err("runtime sequence gap must fail replay");
    // exhaustion_error proves a non-final maximum sequence is rejected before successor replay.
    // exhaustion_error 证明非末尾最大序号会在后继重放前被拒绝。
    let exhaustion_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(u64::MAX, "assistant-1"),
        runtime_assistant_line(0, "assistant-2"),
    ])
    .expect_err("non-final maximum sequence must fail replay");

    assert_eq!(
        schema_error,
        RuntimeLogReplayError::UnsupportedSchemaVersion {
            line_number: 1,
            schema_version: RUNTIME_LOG_SCHEMA_VERSION + 1,
        }
    );
    assert_eq!(
        sequence_error,
        RuntimeLogReplayError::SequenceMismatch {
            line_number: 2,
            expected: 8,
            actual: 9,
        }
    );
    assert_eq!(
        exhaustion_error,
        RuntimeLogReplayError::SequenceExhausted {
            line_number: 1,
            sequence: u64::MAX,
        }
    );
}

/// Verifies replay rejects missing and non-canonical local protocol-message identities.
/// 验证重放会拒绝缺失及非规范的本地协议消息身份。
#[test]
fn runtime_log_replay_rejects_noncanonical_message_identifiers() {
    // missing_error proves blank ids fail before they can enter the conversation vector or index.
    // missing_error 证明空标识会在进入会话向量或索引前失败。
    let missing_error = conversation_state_from_runtime_log_lines(&[runtime_user_line(1, "   ")])
        .expect_err("blank runtime message id must fail replay");
    // padded_error proves replay never trims persisted identity corruption into a different key.
    // padded_error 证明重放绝不会把持久化身份损坏修剪成另一个键。
    let padded_error =
        conversation_state_from_runtime_log_lines(&[runtime_assistant_line(1, " assistant-1 ")])
            .expect_err("padded runtime message id must fail replay");

    assert_eq!(
        missing_error,
        RuntimeLogReplayError::MissingMessageIdentifier {
            message_kind: "user",
        }
    );
    assert_eq!(
        padded_error,
        RuntimeLogReplayError::NonCanonicalMessageIdentifier {
            message_kind: "assistant",
            message_id: String::from(" assistant-1 "),
        }
    );
}

/// Verifies replay rejects tool calls whose owning assistant is absent.
/// 验证重放会拒绝所属助手缺失的工具调用。
#[test]
fn runtime_log_replay_rejects_missing_tool_call_assistant() {
    // error captures the exact call and missing assistant relationship.
    // error 捕获精确调用与缺失助手之间的关系。
    let error = conversation_state_from_runtime_log_lines(&[runtime_tool_call_line(
        1,
        "assistant-missing",
        "call-a",
        Some("provider-a"),
        "batch-a",
    )])
    .expect_err("tool call without owning assistant must fail replay");
    // missing_id_error proves blank runtime identity is rejected before assistant lookup.
    // missing_id_error 证明空白运行时身份会在助手查找前被拒绝。
    let missing_id_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_call_line(2, "assistant-1", "   ", None, "batch-a"),
    ])
    .expect_err("blank runtime tool-call identity must fail replay");

    assert_eq!(
        error,
        RuntimeLogReplayError::MissingToolCallAssistant {
            tool_call_id: String::from("call-a"),
            assistant_message_id: String::from("assistant-missing"),
        }
    );
    assert_eq!(
        missing_id_error,
        RuntimeLogReplayError::MissingToolCallIdentifier {
            assistant_message_id: String::from("assistant-1"),
        }
    );
}

/// Verifies runtime-owned and provider-visible tool-call identities remain globally unique.
/// 验证运行时自有与 Provider 可见工具调用身份保持全局唯一。
#[test]
fn runtime_log_replay_rejects_duplicate_tool_call_identities() {
    // duplicate_message_error proves one id cannot cross user and assistant namespaces.
    // duplicate_message_error 证明一个标识不能跨越用户与助手命名空间。
    let duplicate_message_error = conversation_state_from_runtime_log_lines(&[
        runtime_user_line(1, "assistant-1"),
        runtime_assistant_line(2, "assistant-1"),
    ])
    .expect_err("cross-kind duplicate message id must fail replay");
    // duplicate_runtime_error proves runtime-owned ids cannot silently overwrite replay state.
    // duplicate_runtime_error 证明运行时自有标识不能静默覆盖重放状态。
    let duplicate_runtime_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", Some("provider-a"), "batch-a"),
        runtime_tool_call_line(4, "assistant-1", "call-a", Some("provider-b"), "batch-a"),
    ])
    .expect_err("duplicate runtime tool-call id must fail replay");
    // duplicate_provider_error proves provider-visible ids cannot ambiguously resolve results.
    // duplicate_provider_error 证明 Provider 可见标识不能让结果产生歧义解析。
    let duplicate_provider_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", Some("provider-a"), "batch-a"),
        runtime_tool_call_line(4, "assistant-1", "call-b", Some("provider-a"), "batch-a"),
    ])
    .expect_err("duplicate provider tool-call id must fail replay");
    // cross_namespace_error proves runtime and provider indexes cannot claim the same key.
    // cross_namespace_error 证明运行时与 Provider 索引不能占用同一个键。
    let cross_namespace_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "runtime-a", Some("provider-a"), "batch-a"),
        runtime_tool_call_line(4, "assistant-1", "runtime-b", Some("runtime-a"), "batch-a"),
    ])
    .expect_err("cross-namespace tool-call identity must fail replay");

    assert_eq!(
        duplicate_message_error,
        RuntimeLogReplayError::DuplicateMessageIdentifier {
            message_id: String::from("assistant-1"),
        }
    );
    assert_eq!(
        duplicate_runtime_error,
        RuntimeLogReplayError::DuplicateToolCallIdentifier {
            tool_call_id: String::from("call-a"),
        }
    );
    assert_eq!(
        duplicate_provider_error,
        RuntimeLogReplayError::DuplicateProviderToolCallIdentifier {
            provider_tool_call_id: String::from("provider-a"),
        }
    );
    assert_eq!(
        cross_namespace_error,
        RuntimeLogReplayError::AmbiguousToolCallIdentifier {
            tool_call_id: String::from("runtime-a"),
        }
    );
}

/// Verifies every replayed tool call owns a non-empty runtime batch identifier.
/// 验证每个被重放的工具调用都拥有非空运行时批次标识。
#[test]
fn runtime_log_replay_rejects_missing_tool_batch_identifier() {
    // error captures the exact runtime call whose required batch is blank.
    // error 捕获必需批次为空的精确运行时调用。
    let error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_call_line(2, "assistant-1", "call-a", None, "   "),
    ])
    .expect_err("blank tool-batch identifier must fail replay");

    assert_eq!(
        error,
        RuntimeLogReplayError::MissingToolBatchIdentifier {
            tool_call_id: String::from("call-a"),
        }
    );
}

/// Verifies batch-start controls have unique identity, valid ownership, and precede their calls.
/// 验证批次开始控制事件具有唯一身份、有效归属并先于所属调用。
#[test]
fn runtime_log_replay_validates_tool_batch_start_control() {
    // blank_start_error proves control identity is mandatory even before a call appears.
    // blank_start_error 证明即使调用尚未出现，控制身份仍是必需字段。
    let blank_start_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "   ", "assistant-1"),
    ])
    .expect_err("blank batch-start id must fail replay");
    // missing_assistant_error proves batch ownership cannot target an absent assistant.
    // missing_assistant_error 证明批次归属不能指向缺失助手。
    let missing_assistant_error =
        conversation_state_from_runtime_log_lines(&[runtime_tool_batch_started_line(
            1,
            "batch-a",
            "assistant-missing",
        )])
        .expect_err("batch start without assistant must fail replay");
    // duplicate_start_error proves a later start cannot reset existing batch state.
    // duplicate_start_error 证明后续开始事件不能重置既有批次状态。
    let duplicate_start_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_batch_started_line(3, "batch-a", "assistant-1"),
    ])
    .expect_err("duplicate batch start must fail replay");
    // missing_start_error proves a tool call cannot manufacture its own batch state.
    // missing_start_error 证明工具调用不能自行制造批次状态。
    let missing_start_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_call_line(2, "assistant-1", "call-a", None, "batch-a"),
    ])
    .expect_err("tool call without batch start must fail replay");
    // owner_mismatch_error proves call ownership must agree with the batch-start event.
    // owner_mismatch_error 证明调用归属必须与批次开始事件一致。
    let owner_mismatch_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_assistant_line(2, "assistant-2"),
        runtime_tool_batch_started_line(3, "batch-a", "assistant-1"),
        runtime_tool_call_line(4, "assistant-2", "call-a", None, "batch-a"),
    ])
    .expect_err("tool-call and batch assistant mismatch must fail replay");

    assert_eq!(
        blank_start_error,
        RuntimeLogReplayError::MissingToolBatchControlIdentifier {
            control_event: "tool_batch_started",
        }
    );
    assert_eq!(
        missing_assistant_error,
        RuntimeLogReplayError::MissingToolBatchAssistant {
            tool_batch_id: String::from("batch-a"),
            assistant_message_id: String::from("assistant-missing"),
        }
    );
    assert_eq!(
        duplicate_start_error,
        RuntimeLogReplayError::DuplicateToolBatchIdentifier {
            tool_batch_id: String::from("batch-a"),
        }
    );
    assert_eq!(
        missing_start_error,
        RuntimeLogReplayError::MissingToolBatchStart {
            tool_call_id: String::from("call-a"),
            tool_batch_id: String::from("batch-a"),
        }
    );
    assert_eq!(
        owner_mismatch_error,
        RuntimeLogReplayError::ToolCallBatchAssistantMismatch {
            tool_call_id: String::from("call-a"),
            tool_batch_id: String::from("batch-a"),
            expected_assistant_message_id: String::from("assistant-1"),
            actual_assistant_message_id: String::from("assistant-2"),
        }
    );
}

/// Verifies every present batch completion exactly matches observed call and result state.
/// 验证每条已存在批次完成事件都精确匹配观察到的调用与结果状态。
#[test]
fn runtime_log_replay_validates_tool_batch_completion_summary() {
    // complete_state proves an exact completion remains conversation-replayable.
    // complete_state 证明精确完成事件仍可重放为会话。
    let complete_state = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", None, "batch-a"),
        runtime_tool_result_line(
            4,
            "result-a",
            Some("call-a"),
            None,
            Some("batch-a"),
            RuntimeToolResultStatus::Completed,
        ),
        runtime_tool_batch_completed_line(5, "batch-a", 1, 1, true),
    ])
    .expect("exact tool-batch completion should replay");
    // unknown_completion_error proves completion cannot create an implicit batch.
    // unknown_completion_error 证明完成事件不能隐式创建批次。
    let unknown_completion_error =
        conversation_state_from_runtime_log_lines(&[runtime_tool_batch_completed_line(
            1,
            "batch-missing",
            1,
            0,
            false,
        )])
        .expect_err("unknown batch completion must fail replay");
    // empty_completion_error distinguishes a recoverable open prefix from an impossible completion.
    // empty_completion_error 区分可恢复的开放前缀与不可能的空完成事件。
    let empty_completion_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_batch_completed_line(3, "batch-a", 0, 0, true),
    ])
    .expect_err("completion without calls must fail replay");
    // summary_error proves persisted counts cannot override exact observed state.
    // summary_error 证明持久化计数不能覆盖精确观察状态。
    let summary_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", None, "batch-a"),
        runtime_tool_result_line(
            4,
            "result-a",
            Some("call-a"),
            None,
            Some("batch-a"),
            RuntimeToolResultStatus::Completed,
        ),
        runtime_tool_batch_completed_line(5, "batch-a", 1, 0, false),
    ])
    .expect_err("incorrect batch summary must fail replay");
    // completion_after_audit_error proves audit replay permanently closes completion ordering.
    // completion_after_audit_error 证明审计重放会永久关闭完成事件顺序。
    let completion_after_audit_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", None, "batch-a"),
        RuntimeLogEventLine::new(
            4,
            "1",
            RuntimeLogEvent::RuntimeRepair(RuntimeRepairEvent {
                repair_id: String::from("repair-a"),
                source: String::from("test"),
                detail: String::from("tail repair"),
                downgraded_tool_segments: 1,
            }),
        ),
        runtime_tool_batch_completed_line(5, "batch-a", 1, 0, false),
    ])
    .expect_err("batch completion after audit must fail replay");

    assert_eq!(complete_state.messages.len(), 2);
    assert_eq!(
        unknown_completion_error,
        RuntimeLogReplayError::UnknownToolBatchCompletion {
            tool_batch_id: String::from("batch-missing"),
        }
    );
    assert_eq!(
        empty_completion_error,
        RuntimeLogReplayError::EmptyToolBatchCompletion {
            tool_batch_id: String::from("batch-a"),
        }
    );
    assert_eq!(
        summary_error,
        RuntimeLogReplayError::ToolBatchSummaryMismatch {
            tool_batch_id: String::from("batch-a"),
            expected_tool_call_count: 1,
            actual_tool_call_count: 1,
            expected_tool_result_count: 1,
            actual_tool_result_count: 0,
            expected_complete: true,
            actual_complete: false,
        }
    );
    assert_eq!(
        completion_after_audit_error,
        RuntimeLogReplayError::ProjectionEventAfterRuntimeAudit {
            later_event: "tool_batch_completed",
        }
    );
}

/// Verifies no call, matched result, or second completion can mutate a closed batch.
/// 验证调用、已匹配结果或第二条完成事件均不能修改已关闭批次。
#[test]
fn runtime_log_replay_rejects_events_after_tool_batch_completion() {
    // duplicate_completion_error proves a closed batch cannot be closed again.
    // duplicate_completion_error 证明已关闭批次不能再次关闭。
    let duplicate_completion_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", None, "batch-a"),
        runtime_tool_batch_completed_line(4, "batch-a", 1, 0, false),
        runtime_tool_batch_completed_line(5, "batch-a", 1, 0, false),
    ])
    .expect_err("duplicate batch completion must fail replay");
    // late_call_error proves completion freezes call cardinality.
    // late_call_error 证明完成事件会冻结调用基数。
    let late_call_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", None, "batch-a"),
        runtime_tool_batch_completed_line(4, "batch-a", 1, 0, false),
        runtime_tool_call_line(5, "assistant-1", "call-b", None, "batch-a"),
    ])
    .expect_err("tool call after batch completion must fail replay");
    // late_result_error proves completion freezes matched-result cardinality.
    // late_result_error 证明完成事件会冻结已匹配结果基数。
    let late_result_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", None, "batch-a"),
        runtime_tool_batch_completed_line(4, "batch-a", 1, 0, false),
        runtime_tool_result_line(
            5,
            "result-a",
            Some("call-a"),
            None,
            Some("batch-a"),
            RuntimeToolResultStatus::Completed,
        ),
    ])
    .expect_err("matched result after batch completion must fail replay");
    // late_orphan_error proves explicit orphan downgrade cannot reopen conversation-event replay.
    // late_orphan_error 证明明确孤儿降级不能重新打开会话事件重放。
    let late_orphan_error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", None, "batch-a"),
        runtime_tool_batch_completed_line(4, "batch-a", 1, 0, false),
        runtime_tool_result_line(
            5,
            "result-orphan",
            Some("unknown-call"),
            None,
            None,
            RuntimeToolResultStatus::Orphan,
        ),
    ])
    .expect_err("orphan result after completion phase must fail replay");

    assert_eq!(
        duplicate_completion_error,
        RuntimeLogReplayError::DuplicateToolBatchCompletion {
            tool_batch_id: String::from("batch-a"),
        }
    );
    assert_eq!(
        late_call_error,
        RuntimeLogReplayError::ConversationEventAfterToolBatchCompletion {
            later_event: "tool_call",
        }
    );
    assert_eq!(
        late_result_error,
        RuntimeLogReplayError::ConversationEventAfterToolBatchCompletion {
            later_event: "tool_result",
        }
    );
    assert_eq!(
        late_orphan_error,
        RuntimeLogReplayError::ConversationEventAfterToolBatchCompletion {
            later_event: "tool_result",
        }
    );
}

/// Verifies matched results require a unique preceding call while explicit orphans still downgrade.
/// 验证已匹配结果必须拥有唯一前置调用，而明确孤儿结果仍可降级。
#[test]
fn runtime_log_replay_distinguishes_unknown_match_from_explicit_orphan() {
    // matched_error proves Completed cannot masquerade as a historical orphan.
    // matched_error 证明 Completed 不能伪装成历史孤儿结果。
    let matched_error = conversation_state_from_runtime_log_lines(&[runtime_tool_result_line(
        1,
        "result-missing",
        Some("call-missing"),
        None,
        None,
        RuntimeToolResultStatus::Completed,
    )])
    .expect_err("matched result without a prior call must fail replay");
    // missing_identity_error distinguishes absent identity from an unknown explicit identity.
    // missing_identity_error 区分身份缺失与明确但未知的身份。
    let missing_identity_error =
        conversation_state_from_runtime_log_lines(&[runtime_tool_result_line(
            1,
            "result-without-id",
            None,
            None,
            None,
            RuntimeToolResultStatus::Failed,
        )])
        .expect_err("matched result without identity must fail replay");
    // orphan_state proves an explicitly orphaned result remains recoverable as historical text.
    // orphan_state 证明明确孤儿结果仍可恢复为历史文本。
    let orphan_state = conversation_state_from_runtime_log_lines(&[runtime_tool_result_line(
        1,
        "result-orphan",
        Some("call-missing"),
        None,
        None,
        RuntimeToolResultStatus::Orphan,
    )])
    .expect("explicit orphan result should downgrade");
    // matching_orphan_state proves explicit Orphan cannot be rematched by protocol normalization.
    // matching_orphan_state 证明明确 Orphan 不会被协议归一化重新匹配。
    let matching_orphan_state = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", Some("call-a"), "batch-a"),
        runtime_tool_result_line(
            4,
            "result-orphan-matching-id",
            Some("call-a"),
            Some("call-a"),
            Some("batch-a"),
            RuntimeToolResultStatus::Orphan,
        ),
    ])
    .expect("matching explicit orphan should remain historical text");

    assert_eq!(
        matched_error,
        RuntimeLogReplayError::UnknownMatchedToolCall {
            message_id: String::from("result-missing"),
            tool_call_id: String::from("call-missing"),
        }
    );
    assert_eq!(
        missing_identity_error,
        RuntimeLogReplayError::MissingMatchedToolResultIdentifier {
            message_id: String::from("result-without-id"),
        }
    );
    assert_eq!(orphan_state.messages.len(), 1);
    assert!(matches!(
        orphan_state.messages[0].payload,
        SessionConversationPayload::UserText { .. }
    ));
    assert_eq!(matching_orphan_state.messages.len(), 2);
    assert!(
        matching_orphan_state
            .messages
            .iter()
            .all(|message| matches!(message.payload, SessionConversationPayload::UserText { .. }))
    );
}

/// Verifies a failed provider-only result resolves to its unique preceding call.
/// 验证仅含 Provider 身份的失败结果会解析到其唯一前置调用。
#[test]
fn runtime_log_replay_matches_provider_only_failed_result() {
    // state proves Failed is a matched model-visible result rather than an orphan.
    // state 证明 Failed 是已匹配的模型可见结果，而不是孤儿结果。
    let state = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "runtime-a", Some("provider-a"), "batch-a"),
        runtime_tool_result_line(
            4,
            "result-a",
            None,
            Some("provider-a"),
            Some("batch-a"),
            RuntimeToolResultStatus::Failed,
        ),
    ])
    .expect("provider-only failed result should match");

    assert_eq!(state.messages.len(), 2);
    assert!(matches!(
        state.messages[1].payload,
        SessionConversationPayload::ToolResult {
            ref tool_call_id,
            ..
        } if tool_call_id.as_deref() == Some("provider-a")
    ));
}

/// Verifies runtime-only result events replay with the call's stable provider-visible id.
/// 验证仅含运行时标识的结果事件会使用调用对应的稳定 Provider 可见标识重放。
#[test]
fn runtime_log_replay_canonicalizes_runtime_only_result_identity() {
    let state = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "runtime-a", Some("provider-a"), "batch-a"),
        runtime_tool_result_line(
            4,
            "result-a",
            Some("runtime-a"),
            None,
            Some("batch-a"),
            RuntimeToolResultStatus::Completed,
        ),
    ])
    .expect("runtime-only result identity should match the canonical call");

    assert_eq!(state.messages.len(), 2);
    let SessionConversationPayload::Assistant { tool_calls, .. } = &state.messages[0].payload
    else {
        panic!("first message should remain an assistant tool-call batch");
    };
    assert_eq!(tool_calls[0].tool_call_id.as_deref(), Some("provider-a"));
    assert!(matches!(
        state.messages[1].payload,
        SessionConversationPayload::ToolResult {
            ref tool_call_id,
            ..
        } if tool_call_id.as_deref() == Some("provider-a")
    ));
}

/// Verifies one runtime call cannot consume two matched results.
/// 验证一个运行时调用不能消费两条已匹配结果。
#[test]
fn runtime_log_replay_rejects_duplicate_matched_result() {
    // error identifies the later result and the already consumed runtime call.
    // error 标识后续结果与已经消费的运行时调用。
    let error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", Some("provider-a"), "batch-a"),
        runtime_tool_result_line(
            4,
            "result-a",
            Some("call-a"),
            Some("provider-a"),
            Some("batch-a"),
            RuntimeToolResultStatus::Completed,
        ),
        runtime_tool_result_line(
            5,
            "result-b",
            Some("call-a"),
            Some("provider-a"),
            Some("batch-a"),
            RuntimeToolResultStatus::Failed,
        ),
    ])
    .expect_err("second matched result must fail replay");

    assert_eq!(
        error,
        RuntimeLogReplayError::DuplicateMatchedToolResult {
            message_id: String::from("result-b"),
            tool_call_id: String::from("call-a"),
        }
    );
}

/// Verifies runtime-owned and provider-visible result fields cannot resolve to different calls.
/// 验证结果的运行时自有与 Provider 可见字段不能解析到不同调用。
#[test]
fn runtime_log_replay_rejects_conflicting_result_identities() {
    // error preserves both resolved runtime calls for actionable corruption diagnosis.
    // error 保留两个已解析运行时调用，用于可操作的损坏诊断。
    let error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "runtime-a", Some("provider-a"), "batch-a"),
        runtime_tool_call_line(4, "assistant-1", "runtime-b", Some("provider-b"), "batch-a"),
        runtime_tool_result_line(
            5,
            "result-conflict",
            Some("runtime-a"),
            Some("provider-b"),
            Some("batch-a"),
            RuntimeToolResultStatus::Completed,
        ),
    ])
    .expect_err("conflicting result identities must fail replay");

    assert_eq!(
        error,
        RuntimeLogReplayError::ConflictingMatchedToolCallIdentifiers {
            message_id: String::from("result-conflict"),
            runtime_tool_call_id: String::from("runtime-a"),
            provider_tool_call_id: String::from("runtime-b"),
        }
    );
}

/// Verifies a matched result cannot claim a batch different from its owning call.
/// 验证已匹配结果不能声明不同于所属调用的批次。
#[test]
fn runtime_log_replay_rejects_tool_result_batch_mismatch() {
    // error preserves both authoritative call batch and conflicting result batch.
    // error 同时保留权威调用批次与冲突结果批次。
    let error = conversation_state_from_runtime_log_lines(&[
        runtime_assistant_line(1, "assistant-1"),
        runtime_tool_batch_started_line(2, "batch-a", "assistant-1"),
        runtime_tool_call_line(3, "assistant-1", "call-a", None, "batch-a"),
        runtime_tool_result_line(
            4,
            "result-a",
            Some("call-a"),
            None,
            Some("batch-b"),
            RuntimeToolResultStatus::Completed,
        ),
    ])
    .expect_err("tool-result batch mismatch must fail replay");

    assert_eq!(
        error,
        RuntimeLogReplayError::ToolResultBatchMismatch {
            message_id: String::from("result-a"),
            expected_batch_id: String::from("batch-a"),
            actual_batch_id: String::from("batch-b"),
        }
    );
}

/// Verifies repaired runtime replay groups adjacent display-derived tool-call messages.
/// 验证修复后的 runtime 重放会合并相邻的展示派生工具调用消息。
#[test]
fn runtime_log_replay_groups_adjacent_tool_call_messages() {
    let lines = vec![
        RuntimeLogEventLine::new(
            1,
            "1",
            RuntimeLogEvent::AssistantMessage(RuntimeAssistantMessageEvent {
                turn_id: Some(String::from("assistant-1")),
                message_id: String::from("assistant-1"),
                text: String::new(),
                reasoning: String::new(),
                provider_id: None,
                model_id: None,
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
            }),
        ),
        runtime_tool_batch_started_line(2, "batch-1", "assistant-1"),
        RuntimeLogEventLine::new(
            3,
            "1",
            RuntimeLogEvent::ToolCall(RuntimeToolCallEvent {
                turn_id: Some(String::from("assistant-1")),
                tool_batch_id: String::from("batch-1"),
                assistant_message_id: String::from("assistant-1"),
                runtime_tool_call_id: String::from("call-a"),
                provider_tool_call_id: Some(String::from("call-a")),
                tool_name: String::from("first_tool"),
                arguments: json!({ "value": "a" }),
            }),
        ),
        RuntimeLogEventLine::new(
            4,
            "1",
            RuntimeLogEvent::AssistantMessage(RuntimeAssistantMessageEvent {
                turn_id: Some(String::from("assistant-2")),
                message_id: String::from("assistant-2"),
                text: String::new(),
                reasoning: String::new(),
                provider_id: None,
                model_id: None,
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
            }),
        ),
        runtime_tool_batch_started_line(5, "batch-2", "assistant-2"),
        RuntimeLogEventLine::new(
            6,
            "1",
            RuntimeLogEvent::ToolCall(RuntimeToolCallEvent {
                turn_id: Some(String::from("assistant-2")),
                tool_batch_id: String::from("batch-2"),
                assistant_message_id: String::from("assistant-2"),
                runtime_tool_call_id: String::from("call-b"),
                provider_tool_call_id: Some(String::from("call-b")),
                tool_name: String::from("second_tool"),
                arguments: json!({ "value": "b" }),
            }),
        ),
        RuntimeLogEventLine::new(
            7,
            "1",
            RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                turn_id: Some(String::from("result-a")),
                message_id: String::from("result-a"),
                tool_batch_id: Some(String::from("batch-1")),
                runtime_tool_call_id: Some(String::from("call-a")),
                provider_tool_call_id: Some(String::from("call-a")),
                text: String::from("first result"),
                status: RuntimeToolResultStatus::Completed,
                error: None,
            }),
        ),
        RuntimeLogEventLine::new(
            8,
            "1",
            RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                turn_id: Some(String::from("result-b")),
                message_id: String::from("result-b"),
                tool_batch_id: Some(String::from("batch-2")),
                runtime_tool_call_id: Some(String::from("call-b")),
                provider_tool_call_id: Some(String::from("call-b")),
                text: String::from("second result"),
                status: RuntimeToolResultStatus::Completed,
                error: None,
            }),
        ),
    ];

    let state = conversation_state_from_runtime_log_lines(&lines)
        .expect("replay adjacent tool-call messages");

    assert_eq!(state.messages.len(), 3);
    let SessionConversationPayload::Assistant { tool_calls, .. } = &state.messages[0].payload
    else {
        panic!("first replayed message should be a grouped assistant tool batch");
    };
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0].tool_call_id.as_deref(), Some("call-a"));
    assert_eq!(tool_calls[1].tool_call_id.as_deref(), Some("call-b"));
    assert!(matches!(
        state.messages[1].payload,
        SessionConversationPayload::ToolResult { .. }
    ));
    assert!(matches!(
        state.messages[2].payload,
        SessionConversationPayload::ToolResult { .. }
    ));
}

/// Verifies audit-only runtime events never become visible conversation messages.
/// 验证仅审计运行时事件绝不会变成可见会话消息。
#[test]
fn runtime_log_projection_ignores_control_and_audit_events() {
    let lines = vec![
        RuntimeLogEventLine::new(
            1,
            "1",
            RuntimeLogEvent::SystemUpdate(RuntimeSystemUpdateEvent {
                turn_id: Some(String::from("turn-1")),
                update_id: String::from("system-update-1"),
                effective_after_event_id: None,
                text: String::from("Use terse answers."),
                trust: RuntimeSystemUpdateTrust::TrustedHost,
                lowering_hint: RuntimeSystemUpdateLoweringHint::Auto,
                fingerprint: Some(String::from("sys-fp")),
                source: Some(String::from("test")),
            }),
        ),
        RuntimeLogEventLine::new(
            2,
            "1",
            RuntimeLogEvent::InjectionCancelled(RuntimeInjectionCancelledEvent {
                turn_id: Some(String::from("turn-1")),
                cancellation_id: String::from("cancel-1"),
                source_node_ids: vec![String::from("node-1")],
                previous_persistent_fingerprint: Some(String::from("old-persistent")),
                previous_transient_fingerprint: None,
                reason: Some(String::from("removed")),
            }),
        ),
        RuntimeLogEventLine::new(
            3,
            "1",
            RuntimeLogEvent::InjectionMerged(RuntimeInjectionMergedEvent {
                turn_id: Some(String::from("turn-1")),
                merge_id: String::from("merge-1"),
                compaction_id: Some(String::from("compact-1")),
                previous_persistent_fingerprint: Some(String::from("old")),
                merged_baseline_fingerprint: Some(String::from("new")),
                source_node_ids: vec![String::from("node-2")],
            }),
        ),
        RuntimeLogEventLine::new(
            4,
            "1",
            RuntimeLogEvent::ProviderRequestCheckpoint(RuntimeProviderRequestCheckpointEvent {
                turn_id: Some(String::from("turn-1")),
                checkpoint_id: String::from("request-1"),
                provider_id: String::from("anthropic"),
                model_id: String::from("claude"),
                provider_family: String::from("anthropic"),
                capability_fingerprint: Some(String::from("cap")),
                prompt_fingerprint: Some(String::from("prompt")),
                injection_fingerprint: Some(String::from("inject")),
                request_fingerprint: Some(String::from("request")),
                cache_policy_fingerprint: Some(String::from("cache")),
                cache_policy_diagnostic: Some(String::from("cache_hints_unsupported")),
                converter_version: 1,
            }),
        ),
        RuntimeLogEventLine::new(
            5,
            "1",
            RuntimeLogEvent::ProviderResponseCheckpoint(RuntimeProviderResponseCheckpointEvent {
                turn_id: Some(String::from("turn-1")),
                checkpoint_id: String::from("response-1"),
                provider_id: String::from("anthropic"),
                model_id: String::from("claude"),
                provider_family: String::from("anthropic"),
                finish_reason: Some(String::from("stop")),
                native_state: Some(RuntimeProviderNativeStateEnvelope::new(
                    "anthropic",
                    "anthropic",
                    "claude",
                    json!({ "content": [] }),
                )),
                usage: None,
            }),
        ),
        RuntimeLogEventLine::new(
            6,
            "1",
            RuntimeLogEvent::ProviderLoweringRepair(RuntimeProviderLoweringRepairEvent {
                turn_id: Some(String::from("turn-1")),
                repair_id: String::from("repair-1"),
                provider_id: String::from("anthropic"),
                model_id: String::from("claude"),
                provider_family: String::from("anthropic"),
                action: String::from("fallback"),
                reason: String::from("unsupported system update"),
                affected_items: 1,
            }),
        ),
    ];

    let state =
        conversation_state_from_runtime_log_lines(&lines).expect("replay control and audit events");

    assert!(state.messages.is_empty());
}

/// Verifies the clean runtime schema is v1 and carries the complete ToolResult payload.
/// 验证干净运行时结构为 v1 且能够承载完整 ToolResult 载荷。
#[test]
fn runtime_log_decoder_accepts_clean_v1_tool_result() {
    // source uses the only supported ToolResult shape and its local message identity.
    // source 使用唯一受支持的 ToolResult 形态及其本地消息身份。
    let source = json!({
        "seq": 1,
        "schema_version": 1,
        "created_at": "1",
        "event": {
            "kind": "tool_result",
            "message_id": "message-result-1",
            "tool_batch_id": "batch-1",
            "runtime_tool_call_id": "runtime-call-1",
            "provider_tool_call_id": "provider-call-1",
            "text": "done",
            "status": "completed"
        }
    })
    .to_string();

    // decoded is the exact current v1 line without migration or fallback.
    // decoded 是未经迁移或回退的精确当前 v1 日志行。
    let decoded = decode_runtime_log_event_line_json(&source).expect("decode clean v1 line");
    // serialized proves the removed legacy field cannot reappear during round trip.
    // serialized 证明已删除的旧字段不会在往返转换中重新出现。
    let serialized = serde_json::to_value(&decoded).expect("serialize clean v1 line");

    assert_eq!(RUNTIME_LOG_SCHEMA_VERSION, 1);
    assert_eq!(decoded.schema_version, 1);
    assert_eq!(
        serialized["event"]["message_id"],
        Value::String(String::from("message-result-1"))
    );
    assert!(serialized["event"].get("tool_result_id").is_none());
}

/// Verifies unknown versions remain explicit hard failures under the clean v1 contract.
/// 验证未知版本在干净 v1 契约下保持显式硬失败。
#[test]
fn runtime_log_decoder_rejects_unknown_schema() {
    // unsupported_version is the first value outside the single supported schema.
    // unsupported_version 是唯一受支持结构之外的第一个值。
    let unsupported_version = RUNTIME_LOG_SCHEMA_VERSION + 1;
    // source is syntactically valid so the failure belongs only to version selection.
    // source 语法有效，因此失败只归属于版本选择。
    let source = json!({
        "seq": 1,
        "schema_version": unsupported_version,
        "created_at": "1",
        "event": {
            "kind": "user_message",
            "message_id": "user-1",
            "text": "hello"
        }
    })
    .to_string();

    // error must not be downgraded into corrupt-suffix repair.
    // error 不能被降级为损坏尾部修复。
    let error =
        decode_runtime_log_event_line_json(&source).expect_err("unknown schema must be rejected");

    assert_eq!(
        error,
        RuntimeLogEventLineDecodeError::UnsupportedSchemaVersion {
            schema_version: unsupported_version,
        }
    );
    assert!(!error.is_invalid_json());
}

/// Verifies the removed ToolResult layout is rejected even when it reuses schema number one.
/// 验证已删除 ToolResult 形态即使复用版本号一也会被拒绝。
#[test]
fn runtime_log_decoder_rejects_legacy_v1_tool_result_shape() {
    // source deliberately uses the abandoned field and omits the required current message_id.
    // source 故意使用已放弃字段，并缺少当前必需的 message_id。
    let source = json!({
        "seq": 1,
        "schema_version": 1,
        "created_at": "1",
        "event": {
            "kind": "tool_result",
            "tool_result_id": "legacy-result-1",
            "text": "done"
        }
    })
    .to_string();

    // error proves the decoder never probes legacy fields as compatibility candidates.
    // error 证明解码器绝不会将旧字段作为兼容候选进行探测。
    let error = decode_runtime_log_event_line_json(&source)
        .expect_err("legacy ToolResult shape must be rejected");

    assert!(matches!(
        error,
        RuntimeLogEventLineDecodeError::InvalidCurrentPayload { .. }
    ));
    assert!(!error.is_invalid_json());
}

/// Verifies only malformed JSON is eligible for torn-suffix repair.
/// 验证只有畸形 JSON 才能进入写入中断尾部修复。
#[test]
fn runtime_log_decoder_classifies_malformed_json_for_suffix_repair() {
    // error comes from syntax parsing before any schema or payload interpretation.
    // error 在任何结构或载荷解释前产生于语法解析。
    let error = decode_runtime_log_event_line_json("{ broken")
        .expect_err("malformed JSON must be rejected");

    assert!(matches!(
        error,
        RuntimeLogEventLineDecodeError::InvalidJson { .. }
    ));
    assert!(error.is_invalid_json());
}
