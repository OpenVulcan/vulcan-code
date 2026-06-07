//! Workspace-store regression tests extracted from the storage implementation module.
//! 从存储实现模块中抽取的工作区存储回归测试。

use super::segmented_session_store::{
    SegmentPolicy, SegmentedSessionCreateRequest, SegmentedSessionEventKind,
    create_segmented_session, read_segmented_session_event_window, read_state,
};
use super::*;
use crate::model::{PendingPromptPanelEntry, PendingPromptPanelEntryState};
use vulcan_codex_session_port::{
    CodexEventScope, CodexEventVisibility, CodexExternalStreamTurnRunner,
    CodexExternalTurnBeginRequest, CodexExternalTurnFinishRequest, CodexHookAdapter,
    CodexSessionEvent, CodexSessionEventKind, CodexTurnTerminalStatus,
};
use vulcan_session_core::SessionNodeId;

/// No-op Codex hook adapter used by workspace-store Codex runner tests.
/// workspace-store Codex runner 测试使用的空 hook 适配器。
struct NoopCodexHook;

impl CodexHookAdapter for NoopCodexHook {
    /// Ignores appended Codex events.
    /// 忽略已追加的 Codex 事件。
    fn after_event_appended(&self, _event: &CodexSessionEvent) -> Result<(), String> {
        Ok(())
    }
}

/// Verifies segmented context APIs write state and turn-context anchors into the new session directory.
/// 验证分段上下文 API 会把状态与轮次上下文锚点写入新会话目录。
#[test]
fn segmented_context_public_apis_write_state_and_turn_context() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-segmented-context-api-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7001);
    let session_dir = create_segmented_session(
        &root,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-07"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("上下文测试")),
            title_source: Some(String::from("manual")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create segmented session");
    let message = SessionMessageEntry {
        id: String::from("assistant-context"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("AI"),
        body: String::from("当前上下文"),
        metadata: None,
        created_at: String::from("101"),
    };
    let runtime_state = SessionRuntimeStateEntry {
        conversation_state: conversation_state_from_session_messages(&[message]),
        updated_at: String::from("101"),
        ..SessionRuntimeStateEntry::default()
    };

    persist_segmented_session_context_current_document(
        &root,
        "project-context",
        session_id,
        &runtime_state,
    )
    .expect("persist segmented current context");
    snapshot_segmented_session_context_before_user_prompt(
        &root,
        "project-context",
        session_id,
        &runtime_state,
    )
    .expect("snapshot segmented context");

    let state = read_state(&session_dir)
        .expect("read segmented state")
        .expect("expected state document");
    assert_eq!(state.session_id, session_id.to_string());
    assert!(!state.current_context.is_null());
    let lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read segmented events");
    assert!(
        lines
            .iter()
            .any(|line| line.kind == SegmentedSessionEventKind::TurnContext)
    );

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies Codex boundary events create and append to the segmented session stream.
/// 验证 Codex 边界事件会创建并追加到分段会话流。
#[test]
fn segmented_codex_boundary_public_api_appends_events() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-segmented-codex-boundary-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7002);
    let event = CodexSessionEvent::new(
        "boundary-1",
        SessionNodeId::from(session_id),
        0,
        CodexSessionEventKind::TurnStarted,
        CodexEventScope::Turn,
        CodexEventVisibility::Hidden,
        100,
    )
    .with_turn_id("turn-1")
    .with_payload(serde_json::json!({ "source": "user_followup" }));

    append_segmented_session_codex_boundary_events(
        &root,
        "project-context",
        session_id,
        "Codex 边界",
        &[event],
    )
    .expect("append codex boundary events");

    let session_dir =
        find_existing_segmented_context_session_dir(&root, "project-context", session_id)
            .expect("find session dir")
            .expect("session dir should exist");
    let lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read segmented events");
    assert!(
        lines
            .iter()
            .any(|line| line.kind == SegmentedSessionEventKind::CodexBoundary
                && line.payload["event_id"] == "boundary-1")
    );

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies Codex boundary payload sequence is assigned by the segmented store.
/// 验证 Codex 边界 payload 序号由分段存储层统一分配。
#[test]
fn segmented_codex_boundary_payload_sequence_matches_jsonl_sequence() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-segmented-codex-boundary-seq-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7003);
    let thread_id = SessionNodeId::from(session_id);
    let first_turn_events = vec![
        CodexSessionEvent::new(
            "turn-1-started",
            thread_id,
            0,
            CodexSessionEventKind::TurnStarted,
            CodexEventScope::Turn,
            CodexEventVisibility::Hidden,
            100,
        )
        .with_turn_id("turn-1"),
        CodexSessionEvent::new(
            "turn-1-input",
            thread_id,
            1,
            CodexSessionEventKind::UserInput,
            CodexEventScope::Turn,
            CodexEventVisibility::Visible,
            101,
        )
        .with_turn_id("turn-1"),
    ];
    let second_turn_events = vec![
        CodexSessionEvent::new(
            "turn-2-started",
            thread_id,
            0,
            CodexSessionEventKind::TurnStarted,
            CodexEventScope::Turn,
            CodexEventVisibility::Hidden,
            200,
        )
        .with_turn_id("turn-2"),
        CodexSessionEvent::new(
            "turn-2-input",
            thread_id,
            1,
            CodexSessionEventKind::UserInput,
            CodexEventScope::Turn,
            CodexEventVisibility::Visible,
            201,
        )
        .with_turn_id("turn-2"),
    ];

    append_segmented_session_codex_boundary_events(
        &root,
        "project-context",
        session_id,
        "Codex 边界序号",
        &first_turn_events,
    )
    .expect("append first codex boundary turn");
    append_segmented_session_codex_boundary_events(
        &root,
        "project-context",
        session_id,
        "Codex 边界序号",
        &second_turn_events,
    )
    .expect("append second codex boundary turn");

    let session_dir =
        find_existing_segmented_context_session_dir(&root, "project-context", session_id)
            .expect("find session dir")
            .expect("session dir should exist");
    let lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read segmented events");
    let codex_lines = lines
        .iter()
        .filter(|line| line.kind == SegmentedSessionEventKind::CodexBoundary)
        .collect::<Vec<_>>();
    assert_eq!(codex_lines.len(), 4);
    for line in &codex_lines {
        assert_eq!(line.payload["sequence"], serde_json::json!(line.seq));
        assert_eq!(line.payload["thread_id"], serde_json::json!(thread_id));
    }
    let restored_events =
        load_segmented_session_codex_boundary_events(&root, "project-context", session_id, None, 8)
            .expect("restore codex boundary events");
    assert_eq!(restored_events.len(), 4);
    assert_eq!(restored_events[0].thread_id, thread_id);
    assert_eq!(restored_events[0].sequence, codex_lines[0].seq);
    assert_eq!(restored_events[3].turn_id.as_deref(), Some("turn-2"));

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies the workspace Codex store adapter preserves non-Codex state fields.
/// 验证工作区 Codex 存储适配器会保留非 Codex 状态字段。
#[test]
fn workspace_segmented_codex_store_runs_external_lifecycle_without_overwriting_runtime_state() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-codex-store-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7004);
    let thread_id = SessionNodeId::from(session_id);
    let session_dir = create_segmented_session(
        &root,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-07"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("Codex runner")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    write_state(
        &session_dir,
        &super::segmented_session_store::SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: session_id.to_string(),
            updated_at: String::from("101"),
            runtime_state: serde_json::json!({ "phase": "existing-runtime" }),
            current_context: serde_json::json!({ "cursor": 42 }),
            codex_thread_state: serde_json::Value::Null,
        },
    )
    .expect("write existing state");
    let store = WorkspaceSegmentedCodexSessionStore::new(
        &root,
        "project-context",
        session_id,
        "Codex runner",
    );
    let runner = CodexExternalStreamTurnRunner::new(store, NoopCodexHook);

    runner
        .begin_turn(CodexExternalTurnBeginRequest {
            thread_id,
            turn_id: String::from("turn-1"),
            accepted_events: vec![
                CodexSessionEvent::new(
                    "turn-started",
                    thread_id,
                    0,
                    CodexSessionEventKind::TurnStarted,
                    CodexEventScope::Turn,
                    CodexEventVisibility::Hidden,
                    200,
                )
                .with_turn_id("turn-1"),
            ],
        })
        .expect("begin external lifecycle");
    runner
        .finish_turn(CodexExternalTurnFinishRequest {
            thread_id,
            turn_id: String::from("turn-1"),
            terminal_status: CodexTurnTerminalStatus::Completed,
            final_events: Vec::new(),
            terminal_event: Some(
                CodexSessionEvent::new(
                    "turn-completed",
                    thread_id,
                    0,
                    CodexSessionEventKind::TurnCompleted,
                    CodexEventScope::Turn,
                    CodexEventVisibility::Hidden,
                    201,
                )
                .with_turn_id("turn-1"),
            ),
        })
        .expect("finish external lifecycle");

    let state = read_state(&session_dir)
        .expect("state should read")
        .expect("state should exist");
    assert_eq!(state.runtime_state["phase"], "existing-runtime");
    assert_eq!(state.current_context["cursor"], 42);
    assert_eq!(state.codex_thread_state["status"], "idle");
    let restored_events =
        load_segmented_session_codex_boundary_events(&root, "project-context", session_id, None, 8)
            .expect("codex events should restore");
    assert_eq!(restored_events.len(), 2);
    assert_eq!(restored_events[0].sequence, 1);
    assert_eq!(restored_events[1].sequence, 2);

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies that default transcript windows keep tool details out of the mainline query profile.
/// 验证默认转录窗口会将工具明细排除在主线查询画像之外。
#[test]
fn default_transcript_window_filters_tool_detail_kinds() {
    assert!(persisted_kind_in_default_transcript_window(
        "assistant-output"
    ));
    assert!(persisted_kind_in_default_transcript_window(
        "summary-output"
    ));
    assert!(!persisted_kind_in_default_transcript_window(
        "tool-call-output"
    ));
    assert!(!persisted_kind_in_default_transcript_window(
        "tool-result-output"
    ));
}

/// Verifies that the on-demand tool-detail stream accepts only tool call and result rows.
/// 验证按需工具明细流只接收工具调用与工具结果行。
#[test]
fn tool_detail_stream_filters_only_tool_detail_kinds() {
    assert!(persisted_kind_in_tool_detail_stream("tool-call-output"));
    assert!(persisted_kind_in_tool_detail_stream("tool-result-output"));
    assert!(!persisted_kind_in_tool_detail_stream("assistant-output"));
    assert!(!persisted_kind_in_tool_detail_stream("summary-output"));
}

/// Verifies that context windows never request negative turn indexes.
/// 验证上下文窗口不会请求负数轮次索引。
#[test]
fn session_history_context_turn_bounds_clamp_lower_edge() {
    assert_eq!(session_history_context_turn_bounds(1, 2), (0, 3));
    assert_eq!(session_history_context_turn_bounds(-5, 2), (0, 2));
}

/// Verifies that context windows keep a stable inclusive turn span around the anchor.
/// 验证上下文窗口会围绕锚点保留稳定的闭区间轮次范围。
#[test]
fn session_history_context_turn_bounds_expand_around_anchor() {
    assert_eq!(session_history_context_turn_bounds(12, 2), (10, 14));
    assert_eq!(session_history_context_turn_bounds(12, -1), (12, 12));
}

/// Verifies that visible pending prompt-panel rows keep the transcript UI state persistable across restarts.
/// 验证可见待发送提示词面板行会让转录界面状态跨重启保持可持久化。
#[test]
fn session_ui_state_json_preserves_pending_prompt_panel_queue() {
    let state = SessionTranscriptUiState {
        pending_prompt_panel_queue: vec![PendingPromptPanelEntry {
            id: String::from("panel-1"),
            prompt: String::from("继续排队消息"),
            preview: String::from("继续排队消息"),
            attachments: Vec::new(),
            model_reference: None,
            runtime_options: crate::model::PendingPromptRuntimeOptions::default(),
            state: PendingPromptPanelEntryState::Ready,
            created_at: String::from("1"),
        }],
        ..SessionTranscriptUiState::default()
    };

    let json_payload = serialize_session_ui_state_to_json(&state)
        .expect("pending panel queue should make UI state persistable");
    let parsed = parse_session_ui_state_from_json(&json_payload)
        .expect("pending panel queue UI state should parse");

    assert_eq!(parsed.pending_prompt_panel_queue.len(), 1);
    assert_eq!(parsed.pending_prompt_panel_queue[0].prompt, "继续排队消息");
}

/// Verifies that session-scoped composer draft state round-trips through the persisted transcript UI-state JSON payload.
/// 验证按会话归属的输入框草稿状态可以在持久化转录界面状态 JSON 载荷中正确往返。
#[test]
fn session_ui_state_json_preserves_composer_draft() {
    let state = SessionTranscriptUiState {
        composer_draft: crate::model::SessionComposerDraftState {
            text: String::from("请继续修复这个问题"),
            attachments: Vec::new(),
            scroll_offset: 24.0,
        },
        ..SessionTranscriptUiState::default()
    };

    let json_payload = serialize_session_ui_state_to_json(&state)
        .expect("composer draft should make UI state persistable");
    let parsed = parse_session_ui_state_from_json(&json_payload)
        .expect("composer draft UI state should parse");

    assert_eq!(parsed.composer_draft.text, "请继续修复这个问题");
    assert_eq!(parsed.composer_draft.scroll_offset, 24.0);
}

/// Verifies that rebuilding runtime state from recent messages preserves provider continuation.
/// 验证基于最近消息重建运行时状态时会保留 Provider continuation。
#[test]
fn runtime_state_rebuild_from_recent_messages_preserves_continuation() {
    let runtime_state = SessionRuntimeStateEntry {
        conversation_state: SessionConversationState {
            continuation: SessionProviderContinuationState {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp-1")),
                updated_at: Some(String::from("123")),
            },
            ..SessionConversationState::default()
        },
        ..SessionRuntimeStateEntry::default()
    };
    let messages = vec![SessionMessageEntry {
        id: String::from("message-1"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("用户指令"),
        body: String::from("继续。"),
        metadata: None,
        created_at: String::from("1"),
    }];

    let continuation = runtime_state.conversation_state.continuation.clone();
    let mut rebuilt = runtime_state;
    rebuilt.conversation_state = conversation_state_from_session_messages(&messages);
    rebuilt.conversation_state.continuation = continuation;

    assert_eq!(rebuilt.conversation_state.records.len(), 1);
    assert_eq!(
        rebuilt
            .conversation_state
            .continuation
            .last_provider_message_id
            .as_deref(),
        Some("resp-1")
    );
}
