//! Session-load tests covering transcript hydration behavior for segmented sessions.
//! 面向分段会话的转录水合行为测试。

use super::*;
use crate::conversation_protocol::SessionConversationRecord;
use serde_json::json;
use vulcan_context::SessionMessageKind;
use vulcan_session_boundary::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
};
use vulcan_session_core::SessionNodeId;

/// Builds one transcript message entry for session-load regression tests.
/// 为会话加载回归测试构建一条转录消息项。
fn test_message(
    id: &str,
    kind: SessionMessageKind,
    record_kind: SessionMessageRecordKind,
    body: &str,
) -> SessionMessageEntry {
    SessionMessageEntry {
        id: id.to_string(),
        kind,
        record_kind,
        visible: true,
        title: id.to_string(),
        body: body.to_string(),
        metadata: None,
        created_at: String::from("1"),
    }
}

/// Builds one persisted session boundary line for session-load regression tests.
/// 为会话加载回归测试构建一条已持久化 会话边界行。
fn session_boundary_line(
    seq: u64,
    thread_id: SessionNodeId,
    kind: SessionBoundaryEventKind,
    visibility: SessionEventVisibility,
    text: &str,
) -> super::super::segmented_session_store::SegmentedSessionEventLine {
    let display_payload = if matches!(kind, SessionBoundaryEventKind::UserInput) {
        json!({ "prompt": text })
    } else {
        json!({ "text": text })
    };
    super::super::segmented_session_store::SegmentedSessionEventLine {
        seq,
        timestamp: (1000 + seq).to_string(),
        kind: SegmentedSessionEventKind::SessionBoundary,
        payload: json!({
            "event_id": format!("event-{seq}"),
            "thread_id": thread_id,
            "turn_id": "turn-1",
            "item_id": serde_json::Value::Null,
            "kind": kind,
            "scope": SessionEventScope::Turn,
            "visibility": visibility,
            "sequence": seq,
            "payload": display_payload,
        }),
    }
}

/// Verifies session boundary events become the primary hydrated transcript source.
/// 验证 会话边界事件会成为水合转录正文的首要来源。
#[test]
fn session_boundary_window_projects_visible_transcript() {
    let thread_id = SessionNodeId::new_v4();
    let window = session_boundary_lines_to_window_messages(
        &[
            session_boundary_line(
                1,
                thread_id,
                SessionBoundaryEventKind::UserInput,
                SessionEventVisibility::Visible,
                "用户输入",
            ),
            session_boundary_line(
                2,
                thread_id,
                SessionBoundaryEventKind::AgentMessage,
                SessionEventVisibility::Visible,
                "助手输出",
            ),
        ],
        false,
    )
    .expect("project session boundary")
    .expect("visible transcript");

    assert_eq!(window.messages.len(), 2);
    assert_eq!(window.messages[0].body, "用户输入");
    assert_eq!(window.messages[1].body, "助手输出");
    assert_eq!(window.earliest_ordinal, Some(1));
    assert!(window.boundary_conversation_state.is_some());
}

/// Verifies ordinary startup hydration preserves display events instead of preferring boundary projection.
/// 验证普通启动水合会保留展示事件，而不是优先使用边界投影。
#[test]
fn default_session_hydration_prefers_raw_display_turns() {
    assert_eq!(
        default_session_hydration_window_source_preference(),
        SegmentedWindowSourcePreference::RawTurnOnly
    );
}

/// Verifies archived and restored session directories still hydrate display text from session boundary events.
/// 验证归档与恢复后的会话目录仍然从 会话边界事件水合显示正文。
#[test]
fn session_boundary_archive_restore_keeps_display_projection() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-session-boundary-archive-restore-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7005);
    let thread_id = SessionNodeId::from(session_id);
    let project_tag = "project-context";
    let request = super::super::segmented_session_store::SegmentedSessionCreateRequest {
        session_id: session_id.to_string(),
        project_tag: project_tag.to_string(),
        created_date: String::from("2026-06-07"),
        created_at: String::from("100"),
        project_id: Some(project_tag.to_string()),
        title: Some(String::from("归档恢复")),
        title_source: Some(String::from("test")),
        segment_policy: super::super::segmented_session_store::SegmentPolicy::default(),
    };
    let session_dir =
        super::super::segmented_session_store::create_segmented_session(&root, request.clone())
            .expect("create segmented session");
    let boundary_events = vec![
        SessionBoundaryEvent::new(
            "turn-1-user",
            thread_id,
            0,
            SessionBoundaryEventKind::UserInput,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            101,
        )
        .with_turn_id("turn-1")
        .with_payload(json!({ "prompt": "用户归档前输入" })),
        SessionBoundaryEvent::new(
            "turn-1-agent",
            thread_id,
            0,
            SessionBoundaryEventKind::AgentMessage,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            102,
        )
        .with_turn_id("turn-1")
        .with_payload(json!({ "text": "助手归档后输出" })),
    ];
    super::super::segmented_session_store::append_segmented_boundary_events(
        &session_dir,
        &boundary_events,
    )
    .expect("append session boundary events");

    let archived_dir = super::super::segmented_session_store::archive_segmented_session_by_id(
        &root,
        project_tag,
        &request.session_id,
        "200",
    )
    .expect("archive session")
    .expect("archived directory");
    let archived_lines =
        read_segmented_session_event_window(&archived_dir, None, 8).expect("read archived");
    let archived_boundary_lines = archived_lines
        .into_iter()
        .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
        .collect::<Vec<_>>();
    let archived_window =
        session_boundary_lines_to_window_messages(&archived_boundary_lines, false)
            .expect("project archived boundary")
            .expect("archived projection");
    assert_eq!(archived_window.messages[0].body, "用户归档前输入");
    assert_eq!(archived_window.messages[1].body, "助手归档后输出");

    let restored_dir = super::super::segmented_session_store::restore_segmented_session_by_id(
        &root,
        project_tag,
        &request.session_id,
        "300",
    )
    .expect("restore session")
    .expect("restored directory");
    let restored_lines =
        read_segmented_session_event_window(&restored_dir, None, 8).expect("read restored");
    let restored_boundary_lines = restored_lines
        .into_iter()
        .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
        .collect::<Vec<_>>();
    let restored_window =
        session_boundary_lines_to_window_messages(&restored_boundary_lines, false)
            .expect("project restored boundary")
            .expect("restored projection");
    assert_eq!(restored_window.messages[0].body, "用户归档前输入");
    assert_eq!(restored_window.messages[1].body, "助手归档后输出");

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies session boundary projection overwrites transcript records while preserving runtime metadata.
/// 验证 会话边界投影会覆盖转录正文，同时保留运行时元数据。
#[test]
fn boundary_projection_replaces_runtime_transcript_only() {
    let mut runtime_state = SessionRuntimeStateEntry {
        active_provider_id: Some(String::from("provider-a")),
        ..SessionRuntimeStateEntry::default()
    };
    runtime_state.conversation_state = conversation_state_from_session_messages(&[test_message(
        "old-user",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "旧正文",
    )]);
    let boundary_state = conversation_state_from_session_messages(&[test_message(
        "session-assistant",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "会话边界正文",
    )]);

    apply_boundary_conversation_projection_to_runtime_state(&mut runtime_state, boundary_state);

    assert_eq!(
        runtime_state.active_provider_id.as_deref(),
        Some("provider-a")
    );
    assert_eq!(runtime_state.conversation_state.records.len(), 1);
    assert_eq!(
        runtime_state.conversation_state.records[0].body,
        "会话边界正文"
    );
}

/// Verifies hydration can rebuild runtime state from the segmented transcript window when no overwrite state exists.
/// 验证没有覆盖式状态时，水合能从分段转录窗口重建运行态。
#[test]
fn hydrate_session_runtime_state_rebuilds_from_segmented_window() {
    let session_id = SessionId::from_u128(99);
    let transcript_messages = vec![test_message(
        "user-1",
        SessionMessageKind::User,
        SessionMessageRecordKind::Transcript,
        "仅转录消息",
    )];

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current-thread tokio runtime");
    let hydrated = runtime
        .block_on(hydrate_session_runtime_state(
            session_id,
            None,
            &transcript_messages,
        ))
        .expect("hydrate runtime state")
        .expect("expected hydrated runtime state");

    assert_eq!(hydrated.conversation_state.messages.len(), 1);
    assert_eq!(hydrated.conversation_state.records.len(), 1);
}

/// Verifies hydration no longer falls back to legacy context snapshots when segmented state is absent.
/// 验证分段状态缺失时水合不再回退旧上下文快照。
#[test]
fn hydrate_session_runtime_state_without_segmented_inputs_returns_empty() {
    let session_id = SessionId::from_u128(119);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current-thread tokio runtime");
    let hydrated = runtime
        .block_on(hydrate_session_runtime_state(session_id, None, &[]))
        .expect("hydrate empty runtime state");

    assert!(hydrated.is_none());
}

/// Verifies startup hydration rebuilds the visible transcript from runtime context when turn-window rows are temporarily absent.
/// 验证当 turn 窗口行暂时缺失时，启动水合会从运行时上下文回建可见转录。
#[test]
fn restore_hydration_window_from_runtime_state_if_empty_rebuilds_visible_messages() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.conversation_state = conversation_state_from_session_messages(&[
        test_message(
            "user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "启动后要能看到我",
        ),
        test_message(
            "assistant-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "当然可以",
        ),
    ]);

    let mut messages = Vec::new();
    let mut tool_detail_messages = Vec::new();
    restore_hydration_window_from_runtime_state_if_empty(
        &mut messages,
        &mut tool_detail_messages,
        Some(&runtime_state),
    );

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].body, "启动后要能看到我");
    assert_eq!(messages[1].body, "当然可以");
    assert!(tool_detail_messages.is_empty());
}

/// Verifies segmented current-context fallback can restore runtime conversation state without a full runtime-state blob.
/// 验证分段当前上下文兜底能在缺少完整运行态 blob 时恢复运行时会话状态。
#[test]
fn segmented_current_context_restores_runtime_state_without_full_blob() {
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let conversation_state = conversation_state_from_session_messages(&[test_message(
        "assistant-context",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "分段上下文正文",
    )]);
    let current_context = serde_json::json!({
        "conversation_state": conversation_state,
        "last_compaction_id": "compaction-segment-1",
        "context_start_message_id": "context-start-1",
        "anchor_context_budget_fingerprint": "budget-fp",
        "anchor_context_assembly_fingerprint": "assembly-fp",
        "last_provider_request_preview_markdown": "# Segmented Preview",
        "updated_at": "9001"
    });

    let applied = apply_segmented_current_context(&mut runtime_state, &current_context)
        .expect("apply segmented current context");

    assert!(applied);
    assert_eq!(runtime_state.conversation_state.messages.len(), 1);
    assert_eq!(
        runtime_state.last_compaction_id.as_deref(),
        Some("compaction-segment-1")
    );
    assert_eq!(
        runtime_state.context_start_message_id.as_deref(),
        Some("context-start-1")
    );
    assert_eq!(
        runtime_state.anchor_context_budget_fingerprint.as_deref(),
        Some("budget-fp")
    );
    assert_eq!(
        runtime_state.anchor_context_assembly_fingerprint.as_deref(),
        Some("assembly-fp")
    );
    assert_eq!(
        runtime_state
            .last_provider_request_preview_markdown
            .as_deref(),
        Some("# Segmented Preview")
    );
    assert_eq!(runtime_state.updated_at, "9001");
}

/// Verifies segmented replay results can hydrate the existing runtime-state structure.
/// 验证分段重放结果可以水合到现有运行态结构中。
#[test]
fn segmented_replay_result_maps_to_runtime_state() {
    let conversation_state = conversation_state_from_session_messages(&[test_message(
        "assistant-replay",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "重放正文",
    )]);
    let runtime_state = runtime_state_entry_from_segmented_replay(SegmentedSessionReplayResult {
        conversation_state,
        last_compaction_id: Some("segmented-compaction-9".to_string()),
        context_start_message_id: Some("assistant-replay".to_string()),
        updated_at: Some("9900".to_string()),
    });

    assert_eq!(runtime_state.conversation_state.messages.len(), 1);
    assert_eq!(
        runtime_state.last_compaction_id.as_deref(),
        Some("segmented-compaction-9")
    );
    assert_eq!(
        runtime_state.context_start_message_id.as_deref(),
        Some("assistant-replay")
    );
    assert_eq!(runtime_state.updated_at, "9900");
}

/// Verifies hydration repairs duplicate canonical record ids that were previously persisted by the broken runtime id generator.
/// 验证水合过程会修复旧版错误运行时标识生成器持久化下来的重复规范记录标识。
#[test]
fn repair_duplicate_conversation_record_ids_rewrites_later_duplicates() {
    let session_id = SessionId::from_u128(0x30cf28c9);
    let mut conversation_state = SessionConversationState {
        messages: Vec::new(),
        records: vec![
            SessionConversationRecord {
                id: String::from("session-old-message-100-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::SummaryOutput,
                visible: false,
                title: String::from("Summary"),
                body: String::from("done"),
                metadata: None,
                created_at: String::from("100"),
            },
            SessionConversationRecord {
                id: String::from("session-old-message-100-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::TurnStats,
                visible: true,
                title: String::from("Turn Stats"),
                body: String::from("stats"),
                metadata: None,
                created_at: String::from("100"),
            },
        ],
        continuation: Default::default(),
    };

    let repaired = repair_duplicate_conversation_record_ids(session_id, &mut conversation_state);

    assert!(repaired);
    assert_eq!(
        conversation_state.records[0].id,
        "session-old-message-100-1"
    );
    assert_eq!(
        conversation_state.records[1].id,
        format!("session-{session_id}-message-100-1")
    );
}
