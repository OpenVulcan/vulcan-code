//! Session hydration tests.
//! 会话水合测试。

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;
use vulcan_context::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
};
use vulcan_workbench_model::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};

use super::*;

/// Verifies selected-session hydration preserves an already loaded display window.
/// 验证选中会话水合会保留已加载的显示窗口。
#[test]
fn selected_session_runtime_hydration_preserves_existing_display_window() {
    // session_id identifies the selected conversation being hydrated.
    // session_id 标识正在水合的选中会话。
    let session_id = SessionId::new_v4();
    // display_entry represents the already-loaded s001 display layer.
    // display_entry 表示已经加载的 s001 显示层。
    let display_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "已有显示层正文")
            .with_runtime_metadata("display-entry", "1000");
    // orphanedDisplayEntry proves the retained WebUI row is normalized together with runtime JSONL.
    // orphanedDisplayEntry 证明保留的 WebUI 行会与运行态 JSONL 一同被归一化。
    let mut orphaned_display_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "", "")
            .with_runtime_metadata("orphaned-request-card", "1003");
    orphaned_display_entry.record_kind = SessionMessageRecordKind::ToolCallOutput;
    orphaned_display_entry.metadata = Some(json!({
        "request_stream_id": "orphaned-stream",
        "tool_call_id": "orphaned-call",
        "tool_name": "submit_plan",
        "tool_display_state": "receiving",
        "request_revision": 1,
    }));
    // hydrated_display_entry represents the lazy hydration window that must not overwrite display_entry.
    // hydrated_display_entry 表示不应覆盖 display_entry 的懒加载显示窗口。
    let hydrated_display_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "助手", "水合显示正文")
            .with_runtime_metadata("hydrated-display-entry", "1001");
    // hidden_tool_detail_entry is loaded with the same visible window but must not be displayed directly.
    // hidden_tool_detail_entry 随同一可见窗口加载，但不能直接显示。
    let mut hidden_tool_detail_entry =
        SessionMessageEntry::new(SessionMessageKind::Assistant, "工具详情", "隐藏详情正文")
            .with_runtime_metadata("tool-detail-entry", "1002");
    hidden_tool_detail_entry.record_kind = SessionMessageRecordKind::ToolDetailOutput;
    hidden_tool_detail_entry.visible = false;
    // runtime_state represents the runtime/r001 JSONL conversation state.
    // runtime_state 表示 runtime/r001 JSONL 对应的会话运行态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.conversation_state.messages = vec![SessionConversationMessage {
        id: String::from("runtime-user-1"),
        payload: SessionConversationPayload::UserText {
            text: String::from("运行态正文"),
            attachments: Vec::new(),
        },
    }];
    // orphanedRequest models a persisted request-stage card without any retained live owner.
    // orphanedRequest 模拟一张已持久化但不再拥有保留实时所有者的请求阶段卡片。
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: String::from("orphaned-request-card"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::new(),
            body: String::new(),
            metadata: Some(json!({
                "request_stream_id": "orphaned-stream",
                "tool_call_id": "orphaned-call",
                "tool_name": "submit_plan",
                "tool_display_state": "receiving",
                "request_revision": 1,
            })),
            created_at: String::from("1003"),
        });
    runtime_state.tool_request_record_ids.insert(
        String::from("orphaned-stream"),
        String::from("orphaned-request-card"),
    );
    runtime_state.tool_call_record_ids.insert(
        String::from("orphaned-call"),
        String::from("orphaned-request-card"),
    );
    let hydration = SessionHydrationSnapshot {
        session_id,
        messages: vec![hydrated_display_entry],
        tool_detail_messages: vec![hidden_tool_detail_entry.clone()],
        compactions: Vec::new(),
        runtime_state: Some(runtime_state),
        earliest_ordinal: None,
        has_more_history: false,
    };
    // workspace_snapshot already has the display layer but lacks runtime context.
    // workspace_snapshot 已有显示层但缺少运行态上下文。
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            session_id,
            vec![display_entry, orphaned_display_entry],
        )]),
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

    apply_selected_session_runtime_hydration(&mut workspace_snapshot, session_id, hydration)
        .expect("valid selected-session hydration identities");

    assert_eq!(
        workspace_snapshot
            .session_messages
            .get(&session_id)
            .and_then(|messages| messages.first())
            .map(|message| message.body.as_str()),
        Some("已有显示层正文")
    );
    let retained_messages = workspace_snapshot
        .session_messages
        .get(&session_id)
        .expect("retained messages should exist");
    assert!(retained_messages.iter().any(|message| {
        message.id == hidden_tool_detail_entry.id
            && message.record_kind == SessionMessageRecordKind::ToolDetailOutput
            && !message.visible
    }));
    let retained_orphaned_request = retained_messages
        .iter()
        .find(|message| message.id == "orphaned-request-card")
        .expect("retained orphaned request card should remain visible");
    assert_eq!(
        retained_orphaned_request
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_display_state"))
            .and_then(serde_json::Value::as_str),
        Some("stopped")
    );
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should exist");
    assert_eq!(
        runtime_state
            .conversation_state
            .messages
            .first()
            .and_then(|message| match &message.payload {
                SessionConversationPayload::UserText { text, .. } => Some(text.as_str()),
                _ => None,
            }),
        Some("运行态正文")
    );
    let orphaned_request = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.id == "orphaned-request-card")
        .expect("orphaned request card should remain visible");
    assert_eq!(
        orphaned_request
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_display_state"))
            .and_then(serde_json::Value::as_str),
        Some("stopped")
    );
    assert!(runtime_state.tool_request_record_ids.is_empty());
    assert!(runtime_state.tool_call_record_ids.is_empty());
    assert!(
        workspace_snapshot
            .hydrated_session_ids
            .contains(&session_id)
    );
}

/// Verifies display hydration readiness remains distinct from strict submission context readiness.
/// 验证显示层水合就绪状态与严格的提交上下文就绪状态保持分离。
#[test]
fn display_hydration_readiness_remains_distinct_from_submission_context() {
    // session_id identifies the hydrated conversation under validation.
    // session_id 标识正在校验的已水合会话。
    let session_id = SessionId::new_v4();
    // display_only_snapshot simulates a loaded UI window without runtime dialogue context.
    // display_only_snapshot 模拟只有 UI 窗口而缺少运行态对话层上下文的状态。
    let mut display_only_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            session_id,
            vec![SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "助手",
                "显示层正文",
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
        hydrated_session_ids: BTreeSet::new(),
    };
    // ready_runtime_state stores concrete runtime context for the positive case.
    // ready_runtime_state 保存正向场景所需的具体运行态上下文。
    let mut ready_runtime_state = SessionRuntimeStateEntry::default();
    ready_runtime_state.conversation_state.messages = vec![SessionConversationMessage {
        id: String::from("runtime-user-2"),
        payload: SessionConversationPayload::UserText {
            text: String::from("真实对话层正文"),
            attachments: Vec::new(),
        },
    }];
    // ready_snapshot simulates a fully hydrated session with dialogue context.
    // ready_snapshot 模拟带有对话层上下文的完整水合会话。
    let mut ready_snapshot = WorkspaceSnapshot {
        session_runtime_state: BTreeMap::from([(session_id, ready_runtime_state)]),
        ..display_only_snapshot.clone()
    };

    assert!(ensure_hydrated_session_dialogue_context(&display_only_snapshot, session_id).is_err());
    assert!(!sync_hydrated_session_dialogue_ready_marker(
        &mut display_only_snapshot,
        session_id
    ));
    assert!(
        !display_only_snapshot
            .hydrated_session_ids
            .contains(&session_id)
    );
    assert!(ensure_hydrated_session_dialogue_context(&ready_snapshot, session_id).is_ok());
    assert!(sync_hydrated_session_dialogue_ready_marker(
        &mut ready_snapshot,
        session_id
    ));
    assert!(ready_snapshot.hydrated_session_ids.contains(&session_id));
}
