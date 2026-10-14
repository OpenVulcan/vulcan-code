//! V1 domain-contract golden tests and negative parse tests.
//! V1 领域契约 golden 测试与负向解析测试。
//!
//! Every event kind pins one exact serialized JSON line so the on-disk
//! contract can never drift silently. Negative tests prove unknown kinds,
//! unknown fields, and shape violations fail instead of degrading.
//! 每种事件种类固定一条精确序列化 JSON 行，使磁盘契约永远不会静默漂移。
//! 负向测试证明未知 kind、未知字段与结构违反都会失败而不是降级。

use crate::v1::context::{
    CompactionNode, CompactionsDocument, LockedPromptTarget, LockedSource, LockedSourceKind,
};
use crate::v1::event::*;
use crate::v1::ids::{
    ApiCallId, ClientActionId, CompactionId, PendingInputId, RequestStreamId, Seq, SubagentTaskId,
    ToolCallId, TurnId,
};
use crate::v1::stats::{StatBucket, TurnStatBuckets, UsageSnapshot};
use serde_json::{Value, json};
use vulcan_change_set::{
    ChangeSetFileChangeKind, ChangeSetMode, PersistentChangeSet, PersistentChangeSetFile,
    PersistentChangeSetHunk, PersistentChangeSetLine, PersistentTextFormat,
};

/// Request-stream identities reject missing semantic content at construction and decode time.
/// request-stream 身份在构造与解码时都拒绝缺失的语义内容。
#[test]
fn request_stream_identity_is_required_and_non_empty() {
    assert!(RequestStreamId::try_new("").is_err());
    assert!(RequestStreamId::try_new("   ").is_err());
    assert!(serde_json::from_str::<RequestStreamId>(r#"" ""#).is_err());
    assert_eq!(
        serde_json::from_str::<RequestStreamId>(r#""stream-1""#)
            .expect("valid request stream id")
            .as_str(),
        "stream-1"
    );
}

/// Subagent task identities reject empty, padded, and oversized durable values.
/// 子代理任务身份拒绝空白、首尾空白与超长持久化值。
#[test]
fn subagent_task_identity_is_strict_and_bounded() {
    assert!(SubagentTaskId::try_new("").is_err());
    assert!(SubagentTaskId::try_new(" task-1").is_err());
    assert!(
        SubagentTaskId::try_new("x".repeat(crate::v1::ids::SUBAGENT_TASK_ID_MAX_BYTES + 1))
            .is_err()
    );
    assert!(serde_json::from_str::<SubagentTaskId>(r#"" ""#).is_err());
    assert_eq!(
        serde_json::from_str::<SubagentTaskId>(r#""task-1""#)
            .expect("valid subagent task id")
            .as_str(),
        "task-1"
    );
}

/// Fixed semantic timestamp used by golden fixtures.
/// golden 夹具使用的固定语义时间戳。
const T: i64 = 1_700_000_000_000;

/// Builds one envelope-wrapper event with default mutable state.
/// 构建一条使用默认可变状态的 envelope 包装事件。
fn envelope(seq: u64, visibility: EventVisibility, body: EventBody) -> ConversationEvent {
    ConversationEvent {
        seq: Seq::try_new(seq).unwrap(),
        turn_id: None,
        created_at_ms: T,
        updated_at_ms: T,
        event_revision: crate::v1::event::EVENT_REVISION_INITIAL,
        visibility,
        event: body,
    }
}

/// Builds one UiOnly event that still carries a turn id, matching the
/// exact shape the runtime writer emits for accounting facts.
/// 构建一条仍携带 turn id 的 UiOnly 事件，与运行时写入器为统计事实发出的
/// 精确形态一致。
fn ui_turn(seq: u64, turn_id: u64, body: EventBody) -> ConversationEvent {
    ConversationEvent {
        turn_id: Some(TurnId::try_new(turn_id).unwrap()),
        ..envelope(seq, EventVisibility::UiOnly, body)
    }
}

fn turn(seq: u64, turn_id: u64, body: EventBody) -> ConversationEvent {
    ConversationEvent {
        turn_id: Some(TurnId::try_new(turn_id).unwrap()),
        ..envelope(seq, EventVisibility::Provider, body)
    }
}

/// Asserts that one event serializes to the exact golden JSON line.
/// 断言一条事件序列化为精确的 golden JSON 行。
fn assert_golden(event: &ConversationEvent, expected: &str) {
    let actual = serde_json::to_string(event).unwrap();
    assert_eq!(
        actual,
        expected,
        "golden mismatch for kind {}",
        event.event.kind_name()
    );
}

/// System-context golden line.
/// 系统上下文 golden 行。
#[test]
fn golden_system_context() {
    let event = envelope(
        1,
        EventVisibility::Provider,
        EventBody::SystemContext(SystemContextEvent {
            context_key: "goal_state".to_string(),
            operation: SystemContextOperation::Upsert,
            scope: SystemContextScope::Session,
            ttl_turns: Some(2),
            provider_role_hint: Some(SystemRoleHint::Developer),
            content: "continue toward the goal".to_string(),
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":1,"turn_id":null,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"provider","event":{"kind":"system_context","context_key":"goal_state","operation":"upsert","scope":"session","ttl_turns":2,"provider_role_hint":"developer","content":"continue toward the goal"}}"#,
    );
}

/// User-input golden line with queued-scheduled source and omitted resources.
/// 携带 queued-scheduled 来源且省略资源的用户输入 golden 行。
#[test]
fn golden_user_input() {
    let event = turn(
        2,
        1,
        EventBody::UserInput(UserInputEvent {
            parameters: None,
            client_action_id: ClientActionId::try_new("golden-user-input").unwrap(),
            input_id: PendingInputId::try_new(3).unwrap(),
            input_source: UserInputSource::QueuedScheduled,
            text: "list the files".to_string(),
            resource_ids: Vec::new(),
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":2,"turn_id":1,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"provider","event":{"kind":"user_input","client_action_id":"golden-user-input","input_id":3,"input_source":"queued_scheduled","text":"list the files"}}"#,
    );
}

/// Assistant-reasoning golden line while still streaming.
/// 仍在流式时的 assistant 推理 golden 行。
#[test]
fn golden_assistant_reasoning() {
    let event = turn(
        3,
        1,
        EventBody::AssistantReasoning(AssistantReasoningEvent {
            api_call_id: ApiCallId::try_new(4).unwrap(),
            text: "thinking".to_string(),
            status: AssistantSemanticStatus::Streaming,
            provider_source: None,
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":3,"turn_id":1,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"provider","event":{"kind":"assistant_reasoning","api_call_id":4,"text":"thinking","status":"streaming"}}"#,
    );
}

/// Assistant-text golden line with an attached provider source.
/// 携带 provider_source 的 assistant 正文 golden 行。
#[test]
fn golden_assistant_text_with_provider_source() {
    let event = turn(
        4,
        1,
        EventBody::AssistantText(AssistantTextEvent {
            api_call_id: ApiCallId::try_new(4).unwrap(),
            text: "hello".to_string(),
            status: AssistantSemanticStatus::Completed,
            provider_source: Some(ProviderSource {
                fingerprint_version:
                    crate::v1::event::ProviderFingerprintVersion::ImmutableProjectionV1,
                provider: "openai".to_string(),
                profile: Some("responses".to_string()),
                model: "gpt-5".to_string(),
                connection_id: None,
                message_id: Some("msg_1".to_string()),
                response_id: None,
                replay: None,
                covered_until_seq: Seq::try_new(4).unwrap(),
                fingerprint_sha256: "abc123".to_string(),
            }),
            code_snapshots: Vec::new(),
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":4,"turn_id":1,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"provider","event":{"kind":"assistant_text","api_call_id":4,"text":"hello","status":"completed","provider_source":{"provider":"openai","profile":"responses","model":"gpt-5","message_id":"msg_1","covered_until_seq":4,"fingerprint_version":"immutable_projection_v1","fingerprint_sha256":"abc123"}}}"#,
    );
}

/// Tool-call golden line with a canonical persisted change-set.
/// 携带 canonical 持久化 change-set 的工具调用 golden 行。
#[test]
fn golden_tool_call_with_changeset() {
    let changeset = PersistentChangeSet {
        mode: ChangeSetMode::Applied,
        summary: None,
        files: vec![PersistentChangeSetFile {
            resource_id: None,
            change: ChangeSetFileChangeKind::Modify,
            path: Some("a.txt".to_string()),
            old_path: None,
            new_path: None,
            hunks: vec![PersistentChangeSetHunk {
                before: String::new(),
                after: String::new(),
                deleted: vec![PersistentChangeSetLine {
                    line: 1,
                    content: "old".to_string(),
                }],
                inserted: vec![PersistentChangeSetLine {
                    line: 1,
                    content: "new".to_string(),
                }],
            }],
            lifecycle_content: None,
            before_text_format: None,
            after_text_format: Some(PersistentTextFormat {
                encoding: Some("utf-8".to_string()),
                encoding_source: None,
                encoding_certain: None,
                bom: None,
                line_ending: Some("LF".to_string()),
                preferred_line_ending: None,
                mixed_line_endings: None,
                final_newline: None,
            }),
        }],
        diagnostics: Vec::new(),
    };
    let event = turn(
        5,
        1,
        EventBody::ToolCall(ToolCallEvent {
            api_call_id: ApiCallId::try_new(4).unwrap(),
            tool_call_id: ToolCallId::try_new(5).unwrap(),
            request_stream_id: RequestStreamId::try_new("request-file-edit").unwrap(),
            tool_name: "file_edit".to_string(),
            input: ToolPayload::try_new(json!({"path": "a.txt"})).unwrap(),
            output: Some(ToolPayload::try_new(json!({"ok": true})).unwrap()),
            changeset: Some(changeset),
            status: ToolCallStatus::Completed,
            started_at_ms: Some(T),
            finished_at_ms: Some(T),
            provider_tool_call_id: None,
            external_refs: Vec::new(),
            generated_resources: Vec::new(),
            provider_source: None,
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":5,"turn_id":1,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"provider","event":{"kind":"tool_call","api_call_id":4,"tool_call_id":5,"request_stream_id":"request-file-edit","tool_name":"file_edit","input":{"path":"a.txt"},"output":{"ok":true},"changeset":{"mode":"applied","files":[{"change":"modify","path":"a.txt","hunks":[{"before":"","after":"","deleted":[{"line":1,"content":"old"}],"inserted":[{"line":1,"content":"new"}]}],"after_text_format":{"encoding":"utf-8","line_ending":"LF"}}]},"status":"completed","started_at_ms":1700000000000,"finished_at_ms":1700000000000}}"#,
    );
}

/// API-statistics golden line.
/// API 统计 golden 行。
#[test]
fn golden_api_stats() {
    let event = ui_turn(
        6,
        1,
        EventBody::ApiStats(ApiStatsEvent {
            api_call_id: ApiCallId::try_new(4).unwrap(),
            purpose: ApiPurposeKind::MainGeneration,
            provider: "openai".to_string(),
            profile: Some("responses".to_string()),
            model: "gpt-5".to_string(),
            connection_id: None,
            thinking_level: Some("low".to_string()),
            temperature: None,
            usage: UsageSnapshot {
                input_tokens: Some(10),
                output_tokens: Some(20),
                total_tokens: Some(30),
                ..UsageSnapshot::default()
            },
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings {
                prepared_at_ms: Some(T),
                first_token_at_ms: Some(T),
                completed_at_ms: Some(T),
                ..ApiTimings::default()
            },
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":6,"turn_id":1,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"ui_only","event":{"kind":"api_stats","api_call_id":4,"purpose":"main_generation","provider":"openai","profile":"responses","model":"gpt-5","thinking_level":"low","usage":{"input_tokens":10,"output_tokens":20,"total_tokens":30},"status":"completed","end_reason":"completed_successfully","continuation_eligibility":"forbidden","timing":{"prepared_at_ms":1700000000000,"first_token_at_ms":1700000000000,"completed_at_ms":1700000000000}}}"#,
    );
}

/// Turn-boundary golden line.
/// 回合闭合 golden 行。
#[test]
fn golden_turn_boundary() {
    let event = ui_turn(
        7,
        1,
        EventBody::TurnBoundary(TurnBoundaryEvent {
            reason: TurnClosureReason::Completed,
            outcome: TurnOutcome::Completed,
            purpose: TurnBoundaryPurpose::Business,
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":7,"turn_id":1,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"ui_only","event":{"kind":"turn_boundary","reason":"completed","outcome":"completed","purpose":"business"}}"#,
    );
}

/// Turn-statistics golden line with direct/subagent/total buckets.
/// 携带 direct/subagent/total 桶的回合统计 golden 行。
#[test]
fn golden_turn_stats() {
    let usage = UsageSnapshot {
        input_tokens: Some(10),
        output_tokens: Some(20),
        ..UsageSnapshot::default()
    };
    let bucket = StatBucket::from_api_usages([&usage], 1, 2, 0);
    let event = ui_turn(
        8,
        1,
        EventBody::TurnStats(TurnStatsEvent {
            start_seq: Seq::try_new(2).unwrap(),
            end_seq: Seq::try_new(8).unwrap(),
            outcome: TurnOutcome::Completed,
            duration_ms: Some(1000),
            buckets: TurnStatBuckets::combine(bucket, StatBucket::default()),
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":8,"turn_id":1,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"ui_only","event":{"kind":"turn_stats","start_seq":2,"end_seq":8,"outcome":"completed","duration_ms":1000,"buckets":{"direct":{"api_call_count":1,"unknown_usage_api_call_count":0,"tool_call_count":2,"subagent_return_count":0,"usage":{"input_tokens":10,"output_tokens":20}},"subagent":{"api_call_count":0,"unknown_usage_api_call_count":0,"tool_call_count":0,"subagent_return_count":0,"usage":{}},"total":{"api_call_count":1,"unknown_usage_api_call_count":0,"tool_call_count":2,"subagent_return_count":0,"usage":{"input_tokens":10,"output_tokens":20}}}}}"#,
    );
}

/// Subagent-return golden line.
/// 子代理回流 golden 行。
#[test]
fn golden_subagent_return() {
    let event = turn(
        9,
        1,
        EventBody::SubagentReturn(SubagentReturnEvent {
            child_segment_generation: 1,
            child_root_action_id: ClientActionId::try_new("child-execution-9").unwrap(),
            parent_session_id: "parent-1".to_string(),
            child_session_id: "child-1".to_string(),
            child_task_id: SubagentTaskId::try_new("task-9").unwrap(),
            content: "summary: done".to_string(),
            source_assistant_text_seq: Seq::try_new(11).unwrap(),
            source_from_seq: Seq::try_new(1).unwrap(),
            source_to_seq: Seq::try_new(12).unwrap(),
            child_turn_stats_seq: Seq::try_new(12).unwrap(),
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":9,"turn_id":1,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"provider","event":{"kind":"subagent_return","child_segment_generation":1,"child_root_action_id":"child-execution-9","parent_session_id":"parent-1","child_session_id":"child-1","child_task_id":"task-9","content":"summary: done","source_assistant_text_seq":11,"source_from_seq":1,"source_to_seq":12,"child_turn_stats_seq":12}}"#,
    );
}

/// Subagent return reload requires every exact identity/source fact.
/// 子代理回流重载要求全部精确身份/来源事实。
#[test]
fn subagent_return_reload_rejects_missing_identity_and_source() {
    let event = turn(
        9,
        1,
        EventBody::SubagentReturn(SubagentReturnEvent {
            child_segment_generation: 1,
            child_root_action_id: ClientActionId::try_new("child-execution-9").unwrap(),
            parent_session_id: "parent-1".to_string(),
            child_session_id: "child-1".to_string(),
            child_task_id: SubagentTaskId::try_new("task-9").unwrap(),
            content: "summary: done".to_string(),
            source_assistant_text_seq: Seq::try_new(11).unwrap(),
            source_from_seq: Seq::try_new(1).unwrap(),
            source_to_seq: Seq::try_new(12).unwrap(),
            child_turn_stats_seq: Seq::try_new(12).unwrap(),
        }),
    );
    let baseline = serde_json::to_value(event).expect("subagent return JSON");
    for field in [
        "child_task_id",
        "source_assistant_text_seq",
        "child_segment_generation",
        "child_root_action_id",
    ] {
        let mut missing = baseline.clone();
        missing["event"]
            .as_object_mut()
            .expect("event object")
            .remove(field);
        assert!(
            serde_json::from_value::<ConversationEvent>(missing).is_err(),
            "missing {field} must fail"
        );
    }
}

/// Display-note golden line (ui_only visibility).
/// 显示通知 golden 行(ui_only 可见性)。
#[test]
fn golden_display_note() {
    let event = envelope(
        11,
        EventVisibility::UiOnly,
        EventBody::DisplayNote(DisplayNoteEvent {
            notice_kind: DisplayNoticeKind::RetryExhausted,
            severity: NoticeSeverity::Error,
            reason_code: "retry_exhausted".to_string(),
            redacted_detail: Some("retries 3/3 failed".to_string()),
            api_call_id: None,
            retry_count: Some(3),
            retry_limit: Some(3),
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":11,"turn_id":null,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"ui_only","event":{"kind":"display_note","notice_kind":"retry_exhausted","severity":"error","reason_code":"retry_exhausted","redacted_detail":"retries 3/3 failed","retry_count":3,"retry_limit":3}}"#,
    );
}

/// Execution-control golden line (host_only visibility).
/// 执行控制 golden 行(host_only 可见性)。
#[test]
fn golden_execution_control() {
    let event = envelope(
        12,
        EventVisibility::HostOnly,
        EventBody::ExecutionControl(ExecutionControlEvent {
            control_kind: ExecutionControlKind::TurnContinueRequested,
            api_call_id: None,
            client_action_id: Some(ClientActionId::try_new("golden-continue").unwrap()),
        }),
    );
    assert_golden(
        &event,
        r#"{"seq":12,"turn_id":null,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"host_only","event":{"kind":"execution_control","control_kind":"turn_continue_requested","client_action_id":"golden-continue"}}"#,
    );
}

/// Every golden line round-trips through its own JSON.
/// 每条 golden 行都能通过其自身 JSON 往返解析。
#[test]
fn all_kinds_round_trip() {
    let events = vec![
        envelope(
            1,
            EventVisibility::Provider,
            EventBody::UserInput(UserInputEvent {
                parameters: None,
                client_action_id: ClientActionId::try_new("round-trip-root").unwrap(),
                input_id: PendingInputId::try_new(1).unwrap(),
                input_source: UserInputSource::Root,
                text: "hi".to_string(),
                resource_ids: Vec::new(),
            }),
        ),
        envelope(
            2,
            EventVisibility::Provider,
            EventBody::SystemContext(SystemContextEvent {
                context_key: "k".to_string(),
                operation: SystemContextOperation::Remove,
                scope: SystemContextScope::Turn,
                ttl_turns: None,
                provider_role_hint: None,
                content: String::new(),
            }),
        ),
        envelope(
            3,
            EventVisibility::Provider,
            EventBody::AssistantText(AssistantTextEvent {
                api_call_id: ApiCallId::try_new(1).unwrap(),
                text: String::new(),
                status: AssistantSemanticStatus::Interrupted,
                provider_source: None,
                code_snapshots: Vec::new(),
            }),
        ),
        envelope(
            4,
            EventVisibility::UiOnly,
            EventBody::DisplayNote(DisplayNoteEvent {
                notice_kind: DisplayNoticeKind::Stopped,
                severity: NoticeSeverity::Info,
                reason_code: "stopped".to_string(),
                redacted_detail: None,
                api_call_id: None,
                retry_count: None,
                retry_limit: None,
            }),
        ),
        envelope(
            5,
            EventVisibility::HostOnly,
            EventBody::ExecutionControl(ExecutionControlEvent {
                control_kind: ExecutionControlKind::Recovered,
                api_call_id: None,
                client_action_id: None,
            }),
        ),
    ];
    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let decoded: ConversationEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, event);
    }
}

/// An unknown event kind is rejected instead of degrading to a fallback.
/// 未知事件 kind 被拒绝而不是降级到 fallback。
#[test]
fn unknown_event_kind_is_rejected() {
    let value: Value = serde_json::from_str(
        r#"{"seq":1,"turn_id":null,"created_at_ms":1,"updated_at_ms":1,"visibility":"provider","event":{"kind":"alien_kind","text":"x"}}"#,
    )
    .unwrap();
    let decoded: Result<ConversationEvent, _> = serde_json::from_value(value);
    assert!(decoded.is_err());
}

/// An unknown payload field is rejected by the strict contract.
/// 未知载荷字段被严格契约拒绝。
#[test]
fn unknown_payload_field_is_rejected() {
    let event = envelope(
        1,
        EventVisibility::Provider,
        EventBody::SystemContext(SystemContextEvent {
            context_key: "k".to_string(),
            operation: SystemContextOperation::Upsert,
            scope: SystemContextScope::Session,
            ttl_turns: None,
            provider_role_hint: None,
            content: "c".to_string(),
        }),
    );
    let mut value: Value = serde_json::to_value(&event).unwrap();
    value["event"]["surprise"] = Value::Bool(true);
    let decoded: Result<ConversationEvent, _> = serde_json::from_value(value);
    assert!(decoded.is_err());
}

/// Unknown fields at every nested persistent ChangeSet layer are rejected during tool-call JSONL reload.
/// 工具调用 JSONL 重载时会拒绝持久化 ChangeSet 每个嵌套层级的未知字段。
#[test]
fn tool_call_changeset_unknown_fields_are_rejected_at_every_nested_layer() {
    let baseline: Value = serde_json::from_str(
        r#"{"seq":5,"turn_id":1,"created_at_ms":1700000000000,"updated_at_ms":1700000000000,"event_revision":1,"visibility":"provider","event":{"kind":"tool_call","api_call_id":4,"tool_call_id":5,"request_stream_id":"request-file-edit","tool_name":"file_edit","input":{"path":"a.txt"},"changeset":{"mode":"applied","files":[{"change":"modify","path":"a.txt","hunks":[{"before":"","after":"","deleted":[{"line":1,"content":"old"}],"inserted":[{"line":1,"content":"new"}]}]}]},"status":"completed"}}"#,
    )
    .expect("valid tool-call JSONL baseline");
    let mut cases = Vec::new();

    let mut changeset_unknown = baseline.clone();
    changeset_unknown["event"]["changeset"]
        .as_object_mut()
        .expect("changeset object")
        .insert("unexpected".to_string(), Value::Bool(true));
    cases.push(("changeset", changeset_unknown));

    let mut file_unknown = baseline.clone();
    file_unknown["event"]["changeset"]["files"][0]
        .as_object_mut()
        .expect("change-set file object")
        .insert("unexpected".to_string(), Value::Bool(true));
    cases.push(("file", file_unknown));

    let mut hunk_unknown = baseline.clone();
    hunk_unknown["event"]["changeset"]["files"][0]["hunks"][0]
        .as_object_mut()
        .expect("change-set hunk object")
        .insert("unexpected".to_string(), Value::Bool(true));
    cases.push(("hunk", hunk_unknown));

    let mut line_unknown = baseline;
    line_unknown["event"]["changeset"]["files"][0]["hunks"][0]["deleted"][0]
        .as_object_mut()
        .expect("change-set line object")
        .insert("unexpected".to_string(), Value::Bool(true));
    cases.push(("line", line_unknown));

    for (layer, value) in cases {
        let decoded: Result<ConversationEvent, _> = serde_json::from_value(value);
        assert!(decoded.is_err(), "unknown {layer} field must be rejected");
    }
}

/// An unknown visibility value is rejected.
/// 未知 visibility 值被拒绝。
#[test]
fn unknown_visibility_is_rejected() {
    let event = envelope(
        1,
        EventVisibility::Provider,
        EventBody::UserInput(UserInputEvent {
            parameters: None,
            client_action_id: ClientActionId::try_new("unknown-visibility-root").unwrap(),
            input_id: PendingInputId::try_new(1).unwrap(),
            input_source: UserInputSource::Root,
            text: "hi".to_string(),
            resource_ids: Vec::new(),
        }),
    );
    let mut value: Value = serde_json::to_value(&event).unwrap();
    value["visibility"] = Value::String("sometimes".to_string());
    let decoded: Result<ConversationEvent, _> = serde_json::from_value(value);
    assert!(decoded.is_err());
}

/// Missing mandatory fields (for example seq) are rejected.
/// 缺失必需字段(例如 seq)被拒绝。
#[test]
fn missing_mandatory_field_is_rejected() {
    let json = r#"{"turn_id":null,"created_at_ms":1,"updated_at_ms":1,"visibility":"provider","event":{"kind":"user_input","input_id":1,"input_source":"root","text":"hi"}}"#;
    let decoded: Result<ConversationEvent, _> = serde_json::from_str(json);
    assert!(decoded.is_err());
}

/// The compactions document golden line is stable.
/// 压缩节点文档 golden 行保持稳定。
#[test]
fn golden_compactions_document() {
    let document = CompactionsDocument {
        nodes: vec![CompactionNode {
            compaction_id: CompactionId::try_new(1).unwrap(),
            api_call_id: ApiCallId::try_new(4).unwrap(),
            api_stats_seq: Seq::try_new(101).unwrap(),
            occurred_at_seq: Seq::try_new(100).unwrap(),
            retained_from_seq: Seq::try_new(60).unwrap(),
            content: crate::v1::context::CompactionContent::Summary {
                text: "compacted".to_string(),
            },
            created_at_ms: T,
        }],
        ..CompactionsDocument::new("sess-1")
    };
    let json = serde_json::to_string(&document).unwrap();
    assert_eq!(
        json,
        r#"{"contract":"vulcan.session-compactions.v1","schema_version":1,"session_id":"sess-1","nodes":[{"compaction_id":1,"api_call_id":4,"api_stats_seq":101,"occurred_at_seq":100,"retained_from_seq":60,"content":{"kind":"summary","text":"compacted"},"created_at_ms":1700000000000}]}"#
    );
}

/// Locked sources serialize with snake-case kinds.
/// 锁定来源使用 snake_case 种类序列化。
#[test]
fn golden_locked_source_kinds() {
    let sources = [
        LockedSource {
            node_id: "memory-profile".to_string(),
            order: 0,
            target: LockedPromptTarget::System,
            kind: LockedSourceKind::MemoryProfile,
            source_label: Some("memory".to_string()),
            source_id: None,
            content: "m".to_string(),
        },
        LockedSource {
            node_id: "agents-project".to_string(),
            order: 1,
            target: LockedPromptTarget::Developer,
            kind: LockedSourceKind::AgentsMd,
            source_label: Some("AGENTS.md".to_string()),
            source_id: None,
            content: "rules".to_string(),
        },
        LockedSource {
            node_id: "baseline-hooks".to_string(),
            order: 2,
            target: LockedPromptTarget::Developer,
            kind: LockedSourceKind::BaselineHooks,
            source_label: None,
            source_id: None,
            content: String::new(),
        },
    ];
    let kinds: Vec<String> = sources
        .iter()
        .map(|source| serde_json::to_string(&source.kind).unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec![
            r#""memory_profile""#.to_string(),
            r#""agents_md""#.to_string(),
            r#""baseline_hooks""#.to_string(),
        ]
    );
}
/// One tool event with an attached provider source round-trips through JSON.
/// 携带 provider_source 的工具事件通过 JSON 往返。
#[test]
fn tool_call_with_provider_source_round_trips() {
    let event = envelope(
        1,
        EventVisibility::Provider,
        EventBody::ToolCall(ToolCallEvent {
            api_call_id: ApiCallId::try_new(4).unwrap(),
            tool_call_id: ToolCallId::try_new(5).unwrap(),
            request_stream_id: RequestStreamId::try_new("request-1").unwrap(),
            tool_name: "shell".to_string(),
            input: ToolPayload::try_new(json!({"cmd": "ls"})).unwrap(),
            output: None,
            changeset: None,
            status: ToolCallStatus::Completed,
            started_at_ms: Some(T),
            finished_at_ms: Some(T),
            provider_tool_call_id: Some("pt-1".to_string()),
            external_refs: Vec::new(),
            generated_resources: Vec::new(),
            provider_source: Some(ProviderSource {
                fingerprint_version:
                    crate::v1::event::ProviderFingerprintVersion::ImmutableProjectionV1,
                provider: "openai".to_string(),
                profile: None,
                model: "gpt-5".to_string(),
                connection_id: None,
                message_id: Some("m1".to_string()),
                response_id: None,
                replay: None,
                covered_until_seq: Seq::try_new(1).unwrap(),
                fingerprint_sha256: "f".to_string(),
            }),
        }),
    );
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains(r#""provider_source""#));
    let decoded: ConversationEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, event);
}

/// Tool payload deserialization accepts only JSON objects and never bypasses
/// the producer-side validation contract.
/// 工具载荷反序列化只接受 JSON 对象，绝不绕过生产侧校验契约。
#[test]
fn tool_payload_deserialization_enforces_object_shape() {
    let decoded: ToolPayload = serde_json::from_value(json!({"path": "a.txt"})).unwrap();
    assert_eq!(
        decoded,
        ToolPayload::try_new(json!({"path": "a.txt"})).unwrap()
    );

    for invalid in [
        Value::Null,
        json!(true),
        json!(1),
        json!("text"),
        json!(["value"]),
    ] {
        let error = serde_json::from_value::<ToolPayload>(invalid).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("tool payload must be a JSON object")
        );
    }
}

/// The empty payload constructor is non-fallible because it constructs the
/// canonical object shape directly.
/// 空载荷构造器直接生成规范对象结构，因此不会失败或 panic。
#[test]
fn empty_tool_payload_round_trips_as_object() {
    let payload = ToolPayload::empty();
    assert_eq!(serde_json::to_value(&payload).unwrap(), json!({}));
    assert_eq!(
        serde_json::from_value::<ToolPayload>(json!({})).unwrap(),
        payload
    );
}
