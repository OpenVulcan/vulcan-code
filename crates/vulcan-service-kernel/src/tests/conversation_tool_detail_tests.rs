//! Conversation tool-detail lookup tests.
//! 会话工具详情查找测试。

use super::*;
use crate::conversation_history::{find_tool_detail_message, find_tool_detail_record};

/// Builds one displayed message candidate for tool-detail lookup.
/// 构建一条用于工具详情查找的显示消息候选项。
///
/// # Parameters
/// 参数
/// `message_id` is the stable displayed message id.
/// `message_id` 是稳定的显示消息标识。
/// `record_kind` is the transcript record kind attached to the message.
/// `record_kind` 是消息携带的转录记录类型。
/// `metadata` is the tool metadata attached to the message.
/// `metadata` 是消息携带的工具元数据。
///
/// # Returns
/// 返回值
/// A displayed session message candidate.
/// 返回一条显示会话消息候选项。
fn test_tool_detail_message(
    message_id: &str,
    record_kind: SessionMessageRecordKind,
    metadata: serde_json::Value,
) -> SessionMessageEntry {
    // Message candidate with deterministic body and timestamp.
    // 带确定性正文与时间戳的消息候选项。
    let mut message =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "工具详情", message_id)
            .with_runtime_metadata(message_id, "100");
    message.record_kind = record_kind;
    message.metadata = Some(metadata);
    message
}

/// Builds one runtime record candidate for tool-detail lookup.
/// 构建一条用于工具详情查找的运行时记录候选项。
///
/// # Parameters
/// 参数
/// `record_id` is the stable runtime record id.
/// `record_id` 是稳定的运行时记录标识。
/// `record_kind` is the transcript record kind attached to the record.
/// `record_kind` 是记录携带的转录记录类型。
/// `metadata` is the tool metadata attached to the record.
/// `metadata` 是记录携带的工具元数据。
///
/// # Returns
/// 返回值
/// A runtime conversation record candidate.
/// 返回一条运行时会话记录候选项。
fn test_tool_detail_record(
    record_id: &str,
    record_kind: SessionMessageRecordKind,
    metadata: serde_json::Value,
) -> SessionConversationRecord {
    SessionConversationRecord {
        id: String::from(record_id),
        kind: SessionMessageKind::Assistant,
        record_kind,
        visible: false,
        title: String::from("工具详情"),
        body: String::from(record_id),
        metadata: Some(metadata),
        created_at: String::from("100"),
    }
}

/// Verifies direct record-id lookup wins before tool-call fallback lookup.
/// 验证直接 record-id 查找优先于 tool-call 回退查找。
#[test]
fn tool_detail_lookup_prefers_record_id_before_tool_call_fallback() {
    // Fallback message would match by tool-call id if direct id lookup were absent.
    // 如果没有直接 id 查找，该回退消息会通过工具调用标识匹配。
    let fallback_message = test_tool_detail_message(
        "fallback-message",
        SessionMessageRecordKind::ToolResultOutput,
        serde_json::json!({ "tool_call_id": "call-shared" }),
    );
    // Direct message should win even though it has a different tool-call id.
    // 直接消息应胜出，即使它携带不同的工具调用标识。
    let direct_message = test_tool_detail_message(
        "direct-message",
        SessionMessageRecordKind::ToolDetailOutput,
        serde_json::json!({ "tool_call_id": "call-direct" }),
    );
    // Ordered message set used by the lookup function.
    // 查找函数使用的有序消息集合。
    let messages = [fallback_message, direct_message];

    // Matched message resolved through trimmed direct record id.
    // 通过去空白后的直接记录标识解析出的匹配消息。
    let matched_message = find_tool_detail_message(
        messages.iter(),
        Some(" direct-message "),
        Some("call-shared"),
        None,
    )
    .expect("direct record-id lookup should match");

    assert_eq!(matched_message.id, "direct-message");
}

/// Verifies indexed lookup is only a fast path and falls back to full record-id scanning.
/// 验证索引查找只是快速路径，并会回退到完整 record-id 扫描。
#[test]
fn tool_detail_lookup_scans_record_id_when_index_is_stale() {
    // First runtime record intentionally does not match the direct id.
    // 第一条运行时记录故意不匹配直接 id。
    let first_record = test_tool_detail_record(
        "first-record",
        SessionMessageRecordKind::ToolDetailOutput,
        serde_json::json!({ "tool_call_id": "call-first" }),
    );
    // Second runtime record is the expected direct-id match.
    // 第二条运行时记录是预期的直接 id 匹配项。
    let second_record = test_tool_detail_record(
        "second-record",
        SessionMessageRecordKind::ToolDetailOutput,
        serde_json::json!({ "tool_call_id": "call-second" }),
    );
    // Ordered record set used by the lookup function.
    // 查找函数使用的有序记录集合。
    let records = [first_record, second_record];

    // Matched record resolved by scanning after a stale index misses.
    // 陈旧索引未命中后通过扫描解析出的匹配记录。
    let matched_record = find_tool_detail_record(
        records.iter(),
        Some("second-record"),
        Some("call-first"),
        Some(0),
    )
    .expect("record-id scan should recover after stale index");

    assert_eq!(matched_record.id, "second-record");
}

/// Verifies message and record fallback lookup share tool-call metadata alias handling.
/// 验证消息与记录回退查找共享工具调用元数据别名处理。
#[test]
fn tool_detail_lookup_supports_snake_and_camel_tool_call_aliases() {
    // Message candidate uses the camel-case WebUI alias.
    // 消息候选项使用 camel-case WebUI 别名。
    let message = test_tool_detail_message(
        "camel-message",
        SessionMessageRecordKind::ToolResultOutput,
        serde_json::json!({ "toolCallId": "call-camel" }),
    );
    // Record candidate uses the snake-case runtime alias.
    // 记录候选项使用 snake-case 运行时别名。
    let record = test_tool_detail_record(
        "snake-record",
        SessionMessageRecordKind::ToolDetailOutput,
        serde_json::json!({ "tool_call_id": "call-snake" }),
    );

    assert_eq!(
        find_tool_detail_message([&message], None, Some("call-camel"), None)
            .map(|message| message.id.as_str()),
        Some("camel-message")
    );
    assert_eq!(
        find_tool_detail_record([&record], None, Some("call-snake"), None)
            .map(|record| record.id.as_str()),
        Some("snake-record")
    );
}

/// Verifies tool-call fallback rejects ordinary transcript rows.
/// 验证 tool-call 回退查找会拒绝普通转录行。
#[test]
fn tool_detail_lookup_rejects_non_detail_tool_call_rows() {
    // Ordinary message carries a matching tool-call id but cannot expose tool detail.
    // 普通消息携带匹配工具调用标识，但不能暴露工具详情。
    let message = test_tool_detail_message(
        "ordinary-message",
        SessionMessageRecordKind::Transcript,
        serde_json::json!({ "tool_call_id": "call-ordinary" }),
    );
    // Ordinary record carries a matching tool-call id but cannot expose tool detail.
    // 普通记录携带匹配工具调用标识，但不能暴露工具详情。
    let record = test_tool_detail_record(
        "ordinary-record",
        SessionMessageRecordKind::Transcript,
        serde_json::json!({ "tool_call_id": "call-ordinary" }),
    );

    assert!(find_tool_detail_message([&message], None, Some("call-ordinary"), None).is_none());
    assert!(find_tool_detail_record([&record], None, Some("call-ordinary"), None).is_none());
}
