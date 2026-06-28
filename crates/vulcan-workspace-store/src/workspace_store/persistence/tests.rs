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
        persistence_segmented_session_date_from_unix_millis_text("0"),
        "1970-01-01"
    );
    assert_eq!(
        persistence_segmented_session_date_from_unix_millis_text("86400000"),
        "1970-01-02"
    );
    assert_eq!(
        persistence_segmented_session_date_from_unix_millis_text(" 1722470400000 "),
        "2024-08-01"
    );
}

/// Verifies bulk snapshot mirroring only touches sessions with a loaded transcript window.
/// 验证批量快照镜像只会处理已有已加载转录窗口的会话。
#[test]
fn snapshot_session_transcript_is_loaded_requires_messages_or_hydration() {
    let loaded_by_messages = SessionId::from_u128(401);
    let loaded_by_hydration = SessionId::from_u128(402);
    let not_loaded = SessionId::from_u128(403);
    let mut session_messages = BTreeMap::new();
    session_messages.insert(loaded_by_messages, Vec::new());
    let hydrated_session_ids = BTreeSet::from([loaded_by_hydration]);

    assert!(snapshot_session_transcript_is_loaded(
        loaded_by_messages,
        &session_messages,
        &hydrated_session_ids
    ));
    assert!(snapshot_session_transcript_is_loaded(
        loaded_by_hydration,
        &session_messages,
        &hydrated_session_ids
    ));
    assert!(!snapshot_session_transcript_is_loaded(
        not_loaded,
        &session_messages,
        &hydrated_session_ids
    ));
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
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.conversation_state =
        conversation_state_from_session_messages(&[SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "上下文摘要",
            "已总结旧过程",
        )]);
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

/// Verifies bulk persistence rejects explicit session project ids when project membership is missing.
/// 验证批量持久化在项目成员关系缺失时会拒绝会话自身项目标识回退。
#[test]
fn required_session_project_owner_rejects_session_project_id_without_membership() {
    let session = test_session_summary(SessionId::from_u128(501), "explicit-project", "Loose");
    let owner_map = BTreeMap::new();

    let error = required_session_project_owner(&owner_map, &session, "测试索引")
        .expect_err("missing project membership should fail");

    assert!(error.contains("未找到会话所属项目"));
    assert!(error.contains(&session.id.to_string()));
}

/// Verifies bulk persistence reads project ownership only from project membership.
/// 验证批量持久化只从项目成员关系读取项目归属。
#[test]
fn required_session_project_owner_returns_project_membership() {
    let session_id = SessionId::from_u128(502);
    let session = test_session_summary(session_id, "stale-project", "Owned");
    let mut owner_map = BTreeMap::new();
    owner_map.insert(session_id, (String::from("owner-project"), 7));

    let (project_id, sort_order) = required_session_project_owner(&owner_map, &session, "测试索引")
        .expect("project membership should resolve");

    assert_eq!(project_id, "owner-project");
    assert_eq!(sort_order, 7);
}
