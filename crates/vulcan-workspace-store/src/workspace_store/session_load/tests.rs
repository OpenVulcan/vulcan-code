//! Session-load tests covering transcript hydration behavior for segmented sessions.
//! 面向分段会话的转录水合行为测试。

use super::*;
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
    session_boundary_line_for_turn(seq, thread_id, "turn-1", kind, visibility, text)
}

/// Builds one persisted session boundary line for a specific turn id.
/// 为指定轮次标识构建一条已持久化 会话边界行。
fn session_boundary_line_for_turn(
    seq: u64,
    thread_id: SessionNodeId,
    turn_id: &str,
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
            "turn_id": turn_id,
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

/// Verifies raw process rows remain visible when boundary projection provides canonical transcript state.
/// 验证 boundary 投影提供规范转录状态时，raw 过程行仍保留在主时间线可见。
#[test]
fn boundary_window_keeps_raw_timeline_process_rows() {
    let thread_id = SessionNodeId::new_v4();
    let mut boundary_window = session_boundary_lines_to_window_messages(
        &[
            session_boundary_line(
                1,
                thread_id,
                SessionBoundaryEventKind::UserInput,
                SessionEventVisibility::Visible,
                "run command",
            ),
            session_boundary_line(
                2,
                thread_id,
                SessionBoundaryEventKind::AgentMessage,
                SessionEventVisibility::Visible,
                "done",
            ),
        ],
        false,
    )
    .expect("project session boundary")
    .expect("visible transcript");
    let mut raw_tool_call = test_message(
        "raw-tool-call",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolCallOutput,
        "",
    );
    raw_tool_call.metadata = Some(json!({
        "tool_call_id": "call-1",
        "tool_name": "shell_command",
    }));
    let raw_messages = vec![
        test_message(
            "raw-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "run command",
        ),
        raw_tool_call.clone(),
        test_message(
            "raw-assistant",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "done",
        ),
    ];
    let raw_window = SegmentedWindowMessages {
        messages: raw_messages.clone(),
        tool_detail_messages: vec![raw_tool_call],
        message_start_index: Some(0),
        earliest_ordinal: Some(10),
        has_more_history: true,
        full_messages: raw_messages,
        boundary_conversation_state: None,
    };

    merge_raw_timeline_window_into_boundary_window(&mut boundary_window, raw_window);

    assert_eq!(boundary_window.messages.len(), 3);
    assert!(matches!(
        boundary_window.messages[0].record_kind,
        SessionMessageRecordKind::Transcript
    ));
    assert!(matches!(
        boundary_window.messages[1].record_kind,
        SessionMessageRecordKind::ToolCallOutput
    ));
    assert!(matches!(
        boundary_window.messages[2].record_kind,
        SessionMessageRecordKind::AssistantOutput
    ));
    assert_eq!(boundary_window.tool_detail_messages.len(), 1);
    assert_eq!(boundary_window.earliest_ordinal, Some(10));
    assert!(boundary_window.has_more_history);
    assert!(boundary_window.boundary_conversation_state.is_some());
}

/// Verifies canonical boundary process order cannot be replaced by a stale raw display window.
/// 验证规范边界过程顺序不会被陈旧的原始展示窗口替换。
#[test]
fn boundary_process_order_wins_over_stale_raw_window() {
    let thread_id = SessionNodeId::new_v4();
    let mut boundary_tool = session_boundary_line(
        2,
        thread_id,
        SessionBoundaryEventKind::ItemStarted,
        SessionEventVisibility::Visible,
        "boundary tool",
    );
    boundary_tool.payload["payload"] = json!({
        "text": "boundary tool",
        "tool_name": "shell_command",
    });
    let mut boundary_window = session_boundary_lines_to_window_messages(
        &[
            session_boundary_line(
                1,
                thread_id,
                SessionBoundaryEventKind::UserInput,
                SessionEventVisibility::Visible,
                "run command",
            ),
            boundary_tool,
            session_boundary_line(
                3,
                thread_id,
                SessionBoundaryEventKind::AgentMessage,
                SessionEventVisibility::Visible,
                "done",
            ),
        ],
        false,
    )
    .expect("project canonical boundary order")
    .expect("visible canonical boundary transcript");
    let mut stale_raw_tool = test_message(
        "stale-tool",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolCallOutput,
        "",
    );
    stale_raw_tool.metadata = Some(json!({
        "tool_call_id": "call-stale",
        "tool_name": "shell_command",
    }));
    let stale_raw_messages = vec![
        test_message(
            "stale-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "run command",
        ),
        test_message(
            "stale-assistant",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "done",
        ),
        stale_raw_tool.clone(),
    ];
    let stale_raw_window = SegmentedWindowMessages {
        messages: stale_raw_messages.clone(),
        tool_detail_messages: vec![stale_raw_tool],
        message_start_index: Some(40),
        earliest_ordinal: Some(100),
        has_more_history: true,
        full_messages: stale_raw_messages,
        boundary_conversation_state: None,
    };

    merge_raw_timeline_window_into_boundary_window(&mut boundary_window, stale_raw_window);

    assert!(matches!(
        boundary_window.messages[0].record_kind,
        SessionMessageRecordKind::Transcript
    ));
    assert!(matches!(
        boundary_window.messages[1].record_kind,
        SessionMessageRecordKind::ToolCallOutput
    ));
    assert!(matches!(
        boundary_window.messages[2].record_kind,
        SessionMessageRecordKind::AssistantOutput
    ));
    assert_eq!(boundary_window.message_start_index, Some(40));
    assert_eq!(boundary_window.earliest_ordinal, Some(1));
    assert!(boundary_window.has_more_history);
}

/// Verifies a simple canonical window inherits only the matching raw absolute coordinate.
/// 验证简单规范窗口仅继承行数匹配的原始绝对坐标。
#[test]
fn simple_boundary_window_inherits_matching_raw_message_coordinate() {
    let thread_id = SessionNodeId::new_v4();
    let mut boundary_window = session_boundary_lines_to_window_messages(
        &[
            session_boundary_line(
                1,
                thread_id,
                SessionBoundaryEventKind::UserInput,
                SessionEventVisibility::Visible,
                "question",
            ),
            session_boundary_line(
                2,
                thread_id,
                SessionBoundaryEventKind::AgentMessage,
                SessionEventVisibility::Visible,
                "answer",
            ),
        ],
        false,
    )
    .expect("project simple boundary order")
    .expect("visible simple boundary transcript");
    let raw_messages = vec![
        test_message(
            "raw-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "question",
        ),
        test_message(
            "raw-assistant",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "answer",
        ),
    ];
    let raw_window = SegmentedWindowMessages {
        messages: raw_messages.clone(),
        tool_detail_messages: Vec::new(),
        message_start_index: Some(20),
        earliest_ordinal: Some(100),
        has_more_history: true,
        full_messages: raw_messages,
        boundary_conversation_state: None,
    };

    merge_raw_timeline_window_into_boundary_window(&mut boundary_window, raw_window);

    assert_eq!(boundary_window.messages[0].body, "question");
    assert_eq!(boundary_window.messages[1].body, "answer");
    assert_eq!(boundary_window.message_start_index, Some(20));
    assert_eq!(boundary_window.earliest_ordinal, Some(1));
}

/// Verifies boundary-turn reads skip over large non-boundary tails without losing the latest turn.
/// 验证边界轮次读取会跳过大量非边界尾部事件，不丢失最新轮次。
#[test]
fn session_boundary_turn_window_skips_non_boundary_tail_noise() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-session-boundary-noise-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7015);
    let thread_id = SessionNodeId::from(session_id);
    let project_tag = "project-noise";
    let request = super::super::segmented_session_store::SegmentedSessionCreateRequest {
        session_id: session_id.to_string(),
        project_tag: project_tag.to_string(),
        created_date: String::from("2026-07-05"),
        created_at: String::from("100"),
        project_id: Some(project_tag.to_string()),
        title: Some(String::from("边界噪声")),
        title_source: Some(String::from("test")),
        segment_policy: super::super::segmented_session_store::SegmentPolicy::default(),
    };
    let session_dir = super::super::segmented_session_store::create_segmented_session_in_location(
        &root, false, request,
    )
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
        .with_payload(json!({ "prompt": "第一轮输入" })),
        SessionBoundaryEvent::new(
            "turn-1-agent",
            thread_id,
            1,
            SessionBoundaryEventKind::AgentMessage,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            102,
        )
        .with_turn_id("turn-1")
        .with_payload(json!({ "text": "第一轮输出" })),
        SessionBoundaryEvent::new(
            "turn-2-user",
            thread_id,
            2,
            SessionBoundaryEventKind::UserInput,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            201,
        )
        .with_turn_id("turn-2")
        .with_payload(json!({ "prompt": "第二轮输入" })),
        SessionBoundaryEvent::new(
            "turn-2-agent",
            thread_id,
            3,
            SessionBoundaryEventKind::AgentMessage,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            202,
        )
        .with_turn_id("turn-2")
        .with_payload(json!({ "text": "第二轮输出" })),
    ];
    super::super::segmented_session_store::append_segmented_boundary_events(
        &session_dir,
        &boundary_events,
    )
    .expect("append boundary events");
    let noise_events = (0..120)
        .map(
            |index| super::super::segmented_session_store::SegmentedSessionEventInput {
                timestamp: (300 + index).to_string(),
                kind: SegmentedSessionEventKind::DisplayEvent,
                payload: json!({ "noise": index }),
                turn_index: None,
            },
        )
        .collect::<Vec<_>>();
    super::super::segmented_session_store::append_segmented_session_events(
        &session_dir,
        &noise_events,
    )
    .expect("append non-boundary noise");

    let (boundary_lines, has_more_history) =
        read_segmented_session_boundary_turn_window(&session_dir, None, 1)
            .expect("read boundary turn window");
    let window = session_boundary_lines_to_window_messages(&boundary_lines, has_more_history)
        .expect("project boundary window")
        .expect("latest boundary turn");

    assert_eq!(window.messages.len(), 2);
    assert_eq!(window.messages[0].body, "第二轮输入");
    assert_eq!(window.messages[1].body, "第二轮输出");
    assert!(window.has_more_history);

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies archived and restored session directories still hydrate display text from session boundary events.
/// 验证归档与恢复后的会话目录仍然从 会话边界事件水合显示正文。
/// Verifies deterministic archive and restore preserve the session-boundary display projection.
/// 验证确定性归档与恢复会保留会话边界展示投影。
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
    let session_dir = super::super::segmented_session_store::create_segmented_session_in_location(
        &root,
        false,
        request.clone(),
    )
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

    let archived_dir =
        super::super::segmented_session_store::archive_segmented_session_by_created_date(
            &root,
            project_tag,
            &request.created_date,
            &request.session_id,
            "200",
        )
        .expect("archive session");
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

    let restored_dir =
        super::super::segmented_session_store::restore_segmented_session_by_created_date(
            &root,
            project_tag,
            &request.created_date,
            &request.session_id,
            "300",
        )
        .expect("restore session");
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

/// Verifies hydration leaves runtime state absent when only display transcript rows exist.
/// 验证只有展示转录行时，水合会保持运行态为空。
#[test]
fn hydrate_session_runtime_state_does_not_rebuild_from_segmented_window() {
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
            &transcript_messages,
        ))
        .expect("hydrate runtime state");

    assert!(hydrated.is_none());
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
        .block_on(hydrate_session_runtime_state(session_id, &[]))
        .expect("hydrate empty runtime state");

    assert!(hydrated.is_none());
}

/// Verifies startup hydration rebuilds the visible transcript from runtime context when turn-window rows are temporarily absent.
/// 验证当 turn 窗口行暂时缺失时，启动水合会从运行时上下文回建可见转录。
#[test]
fn restore_hydration_window_from_runtime_state_if_empty_rebuilds_visible_messages() {
    let runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&[
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
        ])
        .expect("valid replayed conversation identities"),
        ..Default::default()
    };

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

/// Verifies runtime-state hydration appends hidden tool details even when visible rows already loaded.
/// 验证运行态水合在可见行已加载时仍会追加隐藏工具详情。
#[test]
fn restore_hydration_window_from_runtime_state_if_empty_appends_tool_details_when_messages_exist() {
    let mut hidden_tool_detail = test_message(
        "tool-detail-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::ToolDetailOutput,
        "完整工具输出",
    );
    hidden_tool_detail.visible = false;
    let runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&[hidden_tool_detail])
            .expect("valid replayed conversation identities"),
        ..Default::default()
    };
    let mut messages = vec![test_message(
        "visible-1",
        SessionMessageKind::Assistant,
        SessionMessageRecordKind::AssistantOutput,
        "已经从 turn 窗口加载的正文",
    )];
    let mut tool_detail_messages = Vec::new();

    restore_hydration_window_from_runtime_state_if_empty(
        &mut messages,
        &mut tool_detail_messages,
        Some(&runtime_state),
    );

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "visible-1");
    assert_eq!(tool_detail_messages.len(), 1);
    assert_eq!(tool_detail_messages[0].id, "tool-detail-1");
    assert_eq!(
        tool_detail_messages[0].record_kind,
        SessionMessageRecordKind::ToolDetailOutput
    );
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
    )])
    .expect("valid replayed conversation identities");
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
