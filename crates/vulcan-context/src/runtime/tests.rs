//! Runtime log projection regression tests.
//! 运行时日志投影回归测试。

use super::*;
use crate::protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationState,
    SessionConversationToolCall,
};
use serde_json::json;

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
                    tool_result_id: String::from("result-a"),
                    tool_call_id: Some(String::from("call-a")),
                    text: String::from("ok"),
                },
            },
            SessionConversationMessage {
                id: String::from("result-b"),
                payload: SessionConversationPayload::ToolResult {
                    tool_result_id: String::from("result-b"),
                    tool_call_id: Some(String::from("call-b")),
                    text: String::from("ok"),
                },
            },
        ],
        records: Vec::new(),
        continuation: Default::default(),
    };

    let lines = runtime_log_lines_from_conversation_state(&state, "1", 1);
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
                tool_result_id: String::from("result-x"),
                tool_call_id: Some(String::from("missing")),
                text: String::from("orphan"),
            },
        }],
        records: Vec::new(),
        continuation: Default::default(),
    };

    let lines = runtime_log_lines_from_conversation_state(&state, "1", 1);
    let status = lines
        .iter()
        .find_map(|line| match &line.event {
            RuntimeLogEvent::ToolResult(event) => Some(&event.status),
            _ => None,
        })
        .expect("tool result");

    assert_eq!(status, &RuntimeToolResultStatus::Orphan);
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

    let lines = runtime_log_lines_from_conversation_state(&state, "1", 1);
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
        RuntimeLogEventLine::new(
            2,
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
            3,
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
        RuntimeLogEventLine::new(
            4,
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
            5,
            "1",
            RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                turn_id: Some(String::from("result-a")),
                tool_batch_id: Some(String::from("batch-1")),
                runtime_tool_call_id: Some(String::from("call-a")),
                provider_tool_call_id: Some(String::from("call-a")),
                tool_result_id: String::from("result-a"),
                text: String::from("first result"),
                status: RuntimeToolResultStatus::Completed,
                error: None,
            }),
        ),
        RuntimeLogEventLine::new(
            6,
            "1",
            RuntimeLogEvent::ToolResult(RuntimeToolResultEvent {
                turn_id: Some(String::from("result-b")),
                tool_batch_id: Some(String::from("batch-2")),
                runtime_tool_call_id: Some(String::from("call-b")),
                provider_tool_call_id: Some(String::from("call-b")),
                tool_result_id: String::from("result-b"),
                text: String::from("second result"),
                status: RuntimeToolResultStatus::Completed,
                error: None,
            }),
        ),
    ];

    let state = conversation_state_from_runtime_log_lines(&lines);

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

    let state = conversation_state_from_runtime_log_lines(&lines);

    assert!(state.messages.is_empty());
}
