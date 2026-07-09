//! Workspace-store regression tests extracted from the storage implementation module.
//! 从存储实现模块中抽取的工作区存储回归测试。

use super::segmented_session_store::{
    SegmentPolicy, SegmentedSessionCreateRequest, SegmentedSessionEventKind,
    append_segmented_session_message_update, create_segmented_session_in_location,
    ensure_runtime_log_from_segmented_context, read_segmented_session_event_window, read_state,
};
use super::session_addon_store::{read_session_addon_state, write_session_addon_state};
use super::*;
use crate::conversation_protocol::SessionProviderContinuationState;
use crate::model::{SessionAddonNamespace, SessionAddonState, SessionRuntimeControls};
use vulcan_context::{SessionConversationRecord, SessionMessageKind};
use vulcan_session_boundary::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
    SessionLifecycleBeginRequest, SessionLifecycleFinishRequest, SessionLifecycleHook,
    SessionLifecycleRunner, SessionTurnTerminalStatus,
};
use vulcan_session_core::SessionNodeId;

/// No-op session lifecycle hook adapter used by workspace-store session lifecycle runner tests.
/// workspace-store session lifecycle runner 测试使用的空 hook 适配器。
struct NoopSessionLifecycleHook;

impl SessionLifecycleHook for NoopSessionLifecycleHook {
    /// Ignores appended session boundary events.
    /// 忽略已追加的 会话边界事件。
    fn after_event_appended(&self, _event: &SessionBoundaryEvent) -> Result<(), String> {
        Ok(())
    }
}

/// Verifies freshly loaded runtime state drops stale process-scoped precheck latches.
/// 验证刚加载的运行态会丢弃陈旧的进程级预检查闩锁。
#[test]
fn loaded_runtime_state_drops_process_scoped_precheck_latch() {
    let mut runtime_state = SessionRuntimeStateEntry {
        active_model_id: Some(String::from("model-alpha")),
        context_start_message_id: Some(String::from("message-1")),
        active_precheck_turn_id: Some(String::from("turn-stale")),
        updated_at: String::from("100"),
        ..SessionRuntimeStateEntry::default()
    };

    let changed = sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state);

    assert!(changed);
    assert_eq!(
        runtime_state.active_model_id.as_deref(),
        Some("model-alpha")
    );
    assert_eq!(
        runtime_state.context_start_message_id.as_deref(),
        Some("message-1")
    );
    assert!(runtime_state.active_precheck_turn_id.is_none());
}

/// Verifies persistence preview snippets keep line joining, fallback compaction, and truncation rules.
/// 验证持久化预览片段保持行拼接、回退压缩与截断规则。
#[test]
fn truncate_preview_lines_preserves_compaction_and_truncation_rules() {
    assert_eq!(
        truncate_preview_lines("  first line  \n\n second line \n third line ", 2, 80),
        "first line / second line"
    );
    assert_eq!(
        truncate_preview_lines(" alpha   beta\n\tgamma ", 0, 80),
        "alpha beta gamma"
    );
    assert_eq!(truncate_preview_lines("abcdef", 1, 4), "abc…");
}

/// Verifies session addon persistence writes only the addon document beside segmented state.
/// 验证会话附属状态持久化只写入分段状态旁的附属文档。
#[test]
fn session_addon_state_write_does_not_rewrite_segmented_state_document() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-session-addon-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7011);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-addon"),
            created_date: String::from("2026-06-16"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-addon")),
            title: Some(String::from("addon persist")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    let state_document = state_document_from_runtime_state(
        &session_id.to_string(),
        &SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("old-provider")),
            active_model_id: Some(String::from("old-model")),
            updated_at: String::from("200"),
            ..SessionRuntimeStateEntry::default()
        },
    )
    .expect("state document");
    write_state(&session_dir, &state_document).expect("write state");
    let before_state = std::fs::read_to_string(session_dir.join("state.json")).expect("read state");

    let mut addon_state = SessionAddonState::new(session_id);
    addon_state.set_runtime_controls(
        &SessionRuntimeControls {
            active_agent_id: None,
            active_provider_id: Some(String::from("new-provider")),
            active_model_id: Some(String::from("new-model")),
            active_openrouter_endpoint_route: None,
            active_reasoning_effort: None,
            active_temperature: None,
        },
        String::from("300"),
    );
    write_session_addon_state(&session_dir, &addon_state).expect("write addon");

    let after_state = std::fs::read_to_string(session_dir.join("state.json")).expect("read state");
    let restored_addon = read_session_addon_state(&session_dir)
        .expect("read addon")
        .expect("addon exists");

    assert_eq!(before_state, after_state);
    assert!(
        restored_addon
            .namespace(SessionAddonNamespace::RuntimeControls)
            .is_some()
    );
    assert_eq!(
        restored_addon
            .runtime_controls()
            .and_then(|controls| controls.active_provider_id),
        Some(String::from("new-provider"))
    );
}

/// Verifies runtime recovery does not synthesize provider context from segmented display JSONL.
/// 验证运行时恢复不会从分段展示 JSONL 合成 Provider 上下文。
#[test]
fn session_hydration_does_not_rebuild_runtime_log_from_segmented_jsonl() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-session-store-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7010);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-08"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("startup hydrate")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    let user_message = SessionMessageEntry {
        id: String::from("message-user"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("User"),
        body: String::from("调用两个子代理"),
        metadata: None,
        created_at: String::from("101"),
    };
    let assistant_message = SessionMessageEntry {
        id: String::from("message-assistant"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("AI"),
        body: String::from("两个子代理均已完成"),
        metadata: None,
        created_at: String::from("102"),
    };
    append_segmented_session_message_update(
        &session_dir,
        &[user_message.clone(), assistant_message.clone()],
    )
    .expect("append transcript");
    let restored_conversation_state =
        ensure_runtime_log_from_segmented_context(&session_dir).expect("rehydrate runtime log");

    assert!(restored_conversation_state.is_none());

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies records-only runtime state is not upgraded from display rows into provider-visible messages.
/// 验证仅有 records 的运行态不会从展示行升级为 Provider 可见消息。
#[test]
fn records_only_state_session_hydration_does_not_rebuild_provider_messages_from_segmented_jsonl() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-record-only-replay-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7012);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-24"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("record only replay")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    let user_message = SessionMessageEntry {
        id: String::from("message-user"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("User"),
        body: String::from("之前的问题"),
        metadata: None,
        created_at: String::from("101"),
    };
    let assistant_message = SessionMessageEntry {
        id: String::from("message-assistant"),
        kind: SessionMessageKind::Assistant,
        record_kind: SessionMessageRecordKind::AssistantOutput,
        visible: true,
        title: String::from("AI"),
        body: String::from("之前的回答"),
        metadata: None,
        created_at: String::from("102"),
    };
    append_segmented_session_message_update(
        &session_dir,
        &[user_message.clone(), assistant_message.clone()],
    )
    .expect("append transcript");
    let records_only_runtime_state = SessionRuntimeStateEntry {
        active_model_id: Some(String::from("model-alpha")),
        conversation_state: SessionConversationState {
            messages: Vec::new(),
            records: vec![SessionConversationRecord {
                id: String::from("display-only-record"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("AI"),
                body: String::from("仅显示记录"),
                metadata: None,
                created_at: String::from("99"),
            }],
            continuation: Default::default(),
        },
        updated_at: String::from("103"),
        ..SessionRuntimeStateEntry::default()
    };

    let restored_conversation_state =
        ensure_runtime_log_from_segmented_context(&session_dir).expect("rehydrate runtime log");

    assert_eq!(
        records_only_runtime_state.active_model_id.as_deref(),
        Some("model-alpha")
    );
    assert!(restored_conversation_state.is_none());
    assert_eq!(
        records_only_runtime_state.conversation_state.messages.len(),
        0
    );
    assert_eq!(
        records_only_runtime_state.conversation_state.records.len(),
        1
    );
    assert_eq!(
        records_only_runtime_state.conversation_state.records[0].body,
        "仅显示记录"
    );

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies records-only runtime state is preserved when no segmented dialogue replay exists.
/// 验证没有分段对话可重放时会保留仅有 records 的运行态。
#[test]
fn records_only_state_without_replay_preserves_existing_records() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-record-only-no-replay-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7013);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-24"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("record only no replay")),
            title_source: Some(String::from("test")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create session");
    let records_only_runtime_state = SessionRuntimeStateEntry {
        conversation_state: SessionConversationState {
            messages: Vec::new(),
            records: vec![SessionConversationRecord {
                id: String::from("display-only-record"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("AI"),
                body: String::from("仅显示记录"),
                metadata: None,
                created_at: String::from("99"),
            }],
            continuation: Default::default(),
        },
        updated_at: String::from("103"),
        ..SessionRuntimeStateEntry::default()
    };

    let restored_conversation_state =
        ensure_runtime_log_from_segmented_context(&session_dir).expect("rehydrate runtime log");

    assert!(restored_conversation_state.is_none());

    assert!(
        records_only_runtime_state
            .conversation_state
            .messages
            .is_empty()
    );
    assert_eq!(
        records_only_runtime_state.conversation_state.records.len(),
        1
    );
    assert_eq!(
        records_only_runtime_state.conversation_state.records[0].body,
        "仅显示记录"
    );

    let _ = std::fs::remove_dir_all(root);
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
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
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

/// Verifies session rename updates only metadata without appending segmented JSONL rows.
/// 验证会话重命名只更新元数据，不追加分段 JSONL 行。
#[test]
fn segmented_session_rename_updates_metadata_without_touching_segments() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-segmented-rename-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7008);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-rename"),
            created_date: String::from("2026-06-08"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-rename")),
            title: Some(String::from("旧标题")),
            title_source: Some(String::from("generated")),
            segment_policy: SegmentPolicy::default(),
        },
    )
    .expect("create segmented session");
    let before_lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read before lines");

    rename_segmented_session_metadata(&root, session_id, "新标题", "200").expect("rename metadata");

    let metadata_text =
        std::fs::read_to_string(session_dir.join("metadata.json")).expect("read metadata");
    let metadata: serde_json::Value = serde_json::from_str(&metadata_text).expect("parse metadata");
    assert_eq!(metadata["title"], "新标题");
    assert_eq!(metadata["title_source"], "manual");
    assert_eq!(metadata["updated_at"], "200");
    let after_lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read after lines");
    assert_eq!(after_lines.len(), before_lines.len());

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies session boundary events create and append to the segmented session stream.
/// 验证 会话边界事件会创建并追加到分段会话流。
#[test]
fn segmented_session_boundary_public_api_appends_events() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-segmented-session-boundary-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7002);
    let event = SessionBoundaryEvent::new(
        "boundary-1",
        SessionNodeId::from(session_id),
        0,
        SessionBoundaryEventKind::TurnStarted,
        SessionEventScope::Turn,
        SessionEventVisibility::Hidden,
        100,
    )
    .with_turn_id("turn-1")
    .with_payload(serde_json::json!({ "source": "user_followup" }));

    append_segmented_session_boundary_events(
        &root,
        "project-context",
        session_id,
        "会话边界",
        &[event],
    )
    .expect("append session boundary events");

    let session_dir =
        find_existing_segmented_context_session_dir(&root, "project-context", session_id)
            .expect("find session dir")
            .expect("session dir should exist");
    let lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read segmented events");
    assert!(lines.iter().any(
        |line| line.kind == SegmentedSessionEventKind::SessionBoundary
            && line.payload["event_id"] == "boundary-1"
    ));

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies session boundary payload sequence is assigned by the segmented store.
/// 验证 会话边界 payload 序号由分段存储层统一分配。
#[test]
fn segmented_session_boundary_payload_sequence_matches_jsonl_sequence() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-segmented-session-boundary-seq-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7003);
    let thread_id = SessionNodeId::from(session_id);
    let first_turn_events = vec![
        SessionBoundaryEvent::new(
            "turn-1-started",
            thread_id,
            0,
            SessionBoundaryEventKind::TurnStarted,
            SessionEventScope::Turn,
            SessionEventVisibility::Hidden,
            100,
        )
        .with_turn_id("turn-1"),
        SessionBoundaryEvent::new(
            "turn-1-input",
            thread_id,
            1,
            SessionBoundaryEventKind::UserInput,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            101,
        )
        .with_turn_id("turn-1"),
    ];
    let second_turn_events = vec![
        SessionBoundaryEvent::new(
            "turn-2-started",
            thread_id,
            0,
            SessionBoundaryEventKind::TurnStarted,
            SessionEventScope::Turn,
            SessionEventVisibility::Hidden,
            200,
        )
        .with_turn_id("turn-2"),
        SessionBoundaryEvent::new(
            "turn-2-input",
            thread_id,
            1,
            SessionBoundaryEventKind::UserInput,
            SessionEventScope::Turn,
            SessionEventVisibility::Visible,
            201,
        )
        .with_turn_id("turn-2"),
    ];

    append_segmented_session_boundary_events(
        &root,
        "project-context",
        session_id,
        "会话边界序号",
        &first_turn_events,
    )
    .expect("append first session boundary turn");
    append_segmented_session_boundary_events(
        &root,
        "project-context",
        session_id,
        "会话边界序号",
        &second_turn_events,
    )
    .expect("append second session boundary turn");

    let session_dir =
        find_existing_segmented_context_session_dir(&root, "project-context", session_id)
            .expect("find session dir")
            .expect("session dir should exist");
    let lines =
        read_segmented_session_event_window(&session_dir, None, 8).expect("read segmented events");
    let boundary_lines = lines
        .iter()
        .filter(|line| line.kind == SegmentedSessionEventKind::SessionBoundary)
        .collect::<Vec<_>>();
    assert_eq!(boundary_lines.len(), 4);
    for line in &boundary_lines {
        assert_eq!(line.payload["sequence"], serde_json::json!(line.seq));
        assert_eq!(line.payload["thread_id"], serde_json::json!(thread_id));
    }
    let restored_events =
        load_segmented_session_boundary_events(&root, "project-context", session_id, None, 8)
            .expect("restore session boundary events");
    assert_eq!(restored_events.len(), 4);
    assert_eq!(restored_events[0].thread_id, thread_id);
    assert_eq!(restored_events[0].sequence, boundary_lines[0].seq);
    assert_eq!(restored_events[3].turn_id.as_deref(), Some("turn-2"));

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies the workspace session boundary store adapter preserves host state fields.
/// 验证工作区会话边界存储适配器会保留宿主状态字段。
#[test]
fn workspace_segmented_boundary_store_runs_external_lifecycle_without_overwriting_runtime_state() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-workspace-session-store-test-{}",
        uuid::Uuid::new_v4()
    ));
    let session_id = SessionId::from_u128(7004);
    let thread_id = SessionNodeId::from(session_id);
    let session_dir = create_segmented_session_in_location(
        &root,
        false,
        SegmentedSessionCreateRequest {
            session_id: session_id.to_string(),
            project_tag: String::from("project-context"),
            created_date: String::from("2026-06-07"),
            created_at: String::from("100"),
            project_id: Some(String::from("project-context")),
            title: Some(String::from("session lifecycle runner")),
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
            session_boundary_state: serde_json::Value::Null,
        },
    )
    .expect("write existing state");
    let store = WorkspaceSegmentedSessionBoundaryStore::new(
        &root,
        "project-context",
        session_id,
        "session lifecycle runner",
    );
    let runner = SessionLifecycleRunner::new(store, NoopSessionLifecycleHook);

    runner
        .begin_turn(SessionLifecycleBeginRequest {
            thread_id,
            turn_id: String::from("turn-1"),
            accepted_events: vec![
                SessionBoundaryEvent::new(
                    "turn-started",
                    thread_id,
                    0,
                    SessionBoundaryEventKind::TurnStarted,
                    SessionEventScope::Turn,
                    SessionEventVisibility::Hidden,
                    200,
                )
                .with_turn_id("turn-1"),
            ],
        })
        .expect("begin external lifecycle");
    runner
        .finish_turn(SessionLifecycleFinishRequest {
            thread_id,
            turn_id: String::from("turn-1"),
            terminal_status: SessionTurnTerminalStatus::Completed,
            final_events: Vec::new(),
            terminal_event: Some(
                SessionBoundaryEvent::new(
                    "turn-completed",
                    thread_id,
                    0,
                    SessionBoundaryEventKind::TurnCompleted,
                    SessionEventScope::Turn,
                    SessionEventVisibility::Hidden,
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
    assert_eq!(state.session_boundary_state["status"], "idle");
    let restored_events =
        load_segmented_session_boundary_events(&root, "project-context", session_id, None, 8)
            .expect("session events should restore");
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
    assert!(!persisted_kind_in_default_transcript_window(
        "tool-detail-output"
    ));
}

/// Verifies that the on-demand tool-detail stream accepts tool rows and hidden detail rows.
/// 验证按需工具明细流接收工具行与隐藏详情行。
#[test]
fn tool_detail_stream_filters_only_tool_detail_kinds() {
    assert!(persisted_kind_in_tool_detail_stream("tool-call-output"));
    assert!(persisted_kind_in_tool_detail_stream("tool-result-output"));
    assert!(persisted_kind_in_tool_detail_stream("tool-detail-output"));
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
