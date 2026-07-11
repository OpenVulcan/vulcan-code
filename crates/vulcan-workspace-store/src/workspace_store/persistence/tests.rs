use super::*;

/// Builds one compact session summary used by persistence helper tests.
/// 为持久化辅助测试构建一条紧凑会话摘要。
fn test_session_summary(id: SessionId, project_id: &str, title: &str) -> SessionSummary {
    SessionSummary::new(
        id,
        Some(project_id.to_string()),
        title.to_string(),
        String::new(),
        SessionState::Draft,
        0,
        0,
        String::from("刚刚"),
    )
}

/// Verifies segmented session directory dates are derived from stable Unix-milliseconds text.
/// 验证分段会话目录日期由稳定的 Unix 毫秒文本推导。
#[test]
fn segmented_session_date_uses_unix_millis_text() {
    assert_eq!(
        session_directory_date_from_unix_millis_text("0").expect("zero timestamp should decode"),
        "1970-01-01"
    );
    assert_eq!(
        session_directory_date_from_unix_millis_text("86400000")
            .expect("one-day timestamp should decode"),
        "1970-01-02"
    );
    assert_eq!(
        session_directory_date_from_unix_millis_text(" 1722470400000 ")
            .expect("surrounding whitespace should be ignored"),
        "2024-08-01"
    );
}

/// Verifies invalid persisted timestamps fail instead of silently targeting the Unix epoch.
/// 验证无效持久化时间戳会失败，而不是静默定位到 Unix 纪元。
#[test]
fn segmented_session_date_rejects_invalid_timestamp_text() {
    // blank_error proves missing locator identity is not rewritten as zero milliseconds.
    // blank_error 证明缺失的定位身份不会被改写为零毫秒。
    let blank_error = session_directory_date_from_unix_millis_text("   ")
        .expect_err("blank timestamp should fail");
    // malformed_error proves non-numeric locator identity remains an explicit corruption error.
    // malformed_error 证明非数字定位身份保持为显式损坏错误。
    let malformed_error = session_directory_date_from_unix_millis_text("not-a-time")
        .expect_err("malformed timestamp should fail");

    assert!(blank_error.contains("不能为空"));
    assert!(malformed_error.contains("not-a-time"));
}

/// Verifies segmented locators keep creation time stable when a session is updated later.
/// 验证分段定位信息在会话后续更新时仍保持创建时间稳定。
#[test]
fn segmented_session_locator_uses_created_at_not_updated_at() {
    let session = test_session_summary(SessionId::from_u128(410), "project-a", "Stable")
        .with_created_at("0")
        .with_updated_at("86400000");

    let locator = segmented_session_locator_from_summary(&session, "project-a", false);

    assert_eq!(locator.created_at, "0");
}

/// Verifies runtime-state persistence preserves caller-retained display rows.
/// 验证运行态持久化会保留调用方持有的显示行。
#[test]
fn effective_session_messages_for_persistence_preserves_display_history() {
    // user_entry represents a visible prompt already present in the WebUI transcript.
    // user_entry 表示 WebUI 转录中已经存在的可见提示词。
    let mut user_entry =
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "循环调用工具");
    user_entry.id = String::from("user-1");
    // tool_entry represents process detail that compacted runtime state no longer contains.
    // tool_entry 表示压缩后运行态已不再包含的过程细节。
    let mut tool_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "", "已执行 sleep 5");
    tool_entry.id = String::from("tool-1");
    tool_entry.record_kind = SessionMessageRecordKind::ToolCallOutput;
    // interrupt_entry represents interjected user content that must remain visible after reload.
    // interrupt_entry 表示重载后必须继续可见的插队用户内容。
    let mut interrupt_entry =
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "不用继续了");
    interrupt_entry.id = String::from("interrupt-1");
    interrupt_entry.metadata = Some(serde_json::json!({
        "source": "user_interrupt",
        "starts_user_turn": false,
        "turn_process": true,
        "editable": false,
    }));
    // runtime_state simulates a compacted provider context containing only the summary.
    // runtime_state 模拟只包含摘要的压缩后 Provider 上下文。
    let runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&[SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "上下文摘要",
            "已总结旧过程",
        )
        .with_runtime_metadata("summary-record-1", "1")])
        .expect("valid replayed conversation identities"),
        ..Default::default()
    };
    // stored_messages is the full visible transcript retained by the service layer.
    // stored_messages 是 service 层保留的完整可见转录。
    let stored_messages = vec![
        user_entry.clone(),
        tool_entry.clone(),
        interrupt_entry.clone(),
    ];

    // messages should keep visible history and append or refresh runtime-owned summary rows.
    // messages 应保留可见历史，并追加或刷新运行态拥有的摘要行。
    let messages =
        effective_session_messages_for_persistence(stored_messages.as_slice(), &runtime_state);

    assert!(messages.iter().any(|message| message.id == user_entry.id));
    assert!(messages.iter().any(|message| message.id == tool_entry.id));
    assert!(
        messages
            .iter()
            .any(|message| message.id == interrupt_entry.id)
    );
    assert!(
        messages
            .iter()
            .any(|message| message.body.contains("已总结旧过程"))
    );
}
