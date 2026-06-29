//! stop task mirror tests.
//! stop task mirror tests。

use super::*;

/// Verifies WebUI stop projects the same stopped turn records as the WebUI transcript chain.
/// 验证 WebUI 停止流程投影与 WebUI 转录链条一致的已停止轮次记录。
#[test]
fn webui_stop_session_work_appends_stopped_turn_projection_once() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.last_safe_provider_request_count = Some(2);
    let user_entry =
        SessionMessageEntry::new(SessionMessageKind::User, "用户指令", "运行一个长任务")
            .with_runtime_metadata("user-1", "1000");
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: user_entry.id.clone(),
            kind: user_entry.kind,
            record_kind: user_entry.record_kind,
            visible: user_entry.visible,
            title: user_entry.title.clone(),
            body: user_entry.body.clone(),
            metadata: user_entry.metadata.clone(),
            created_at: user_entry.created_at.clone(),
        },
    );

    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![
            SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "停止测试",
                "正在运行",
                SessionState::Running,
                0,
                0,
                "刚刚",
            )
            .with_updated_at("1000"),
        ],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let did_stop =
        stop_webui_session_work(&mut workspace_snapshot, session_id, true, true).unwrap();

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    let messages = workspace_snapshot
        .session_messages
        .get(&session_id)
        .expect("stopped projection should be materialized");
    assert!(messages.iter().any(|message| {
        message.visible && message.title == "执行已停止" && message.body.contains("当前 AI 调用")
    }));
    assert!(messages.iter().any(|message| {
        !message.visible && message.record_kind == SessionMessageRecordKind::TurnTerminated
    }));
    let turn_stats = messages
        .iter()
        .find(|message| message.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("stopped turn stats should be projected");
    assert!(turn_stats.visible);
    assert_eq!(turn_stats.title, "Turn Stats · stopped");
    assert!(turn_stats.body.contains("状态：已中止"));
    assert!(turn_stats.body.contains("Provider 调用：2"));
    let stopped_message_count = messages.len();

    let did_stop_again =
        stop_webui_session_work(&mut workspace_snapshot, session_id, false, false).unwrap();

    assert!(!did_stop_again);
    let messages_after_second_stop = workspace_snapshot
        .session_messages
        .get(&session_id)
        .expect("stopped projection should still exist");
    assert_eq!(messages_after_second_stop.len(), stopped_message_count);
}

/// Verifies a kernel-active turn still receives a stopped projection when its stream handle is gone.
/// 验证内核活动回合即使流句柄已丢失也仍会收到停止投影。
#[test]
fn webui_stop_session_work_uses_kernel_active_state_without_stream_handle() {
    let session_id = SessionId::new_v4();
    let runtime_state = SessionRuntimeStateEntry {
        last_safe_provider_request_count: Some(1),
        ..SessionRuntimeStateEntry::default()
    };
    let mut workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "停止测试",
            "正在运行",
            SessionState::Running,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let did_stop =
        stop_webui_session_work(&mut workspace_snapshot, session_id, true, false).unwrap();

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    let messages = workspace_snapshot
        .session_messages
        .get(&session_id)
        .expect("kernel-active stop should be projected");
    assert!(messages.iter().any(|message| {
        message.visible && message.title == "执行已停止" && message.body.contains("当前 AI 调用")
    }));
}

/// Verifies persisted completed task mirrors repair stale spawn-agent display metadata.
/// 验证持久化的已完成任务镜像会修复陈旧的 spawn_agent 展示元数据。
#[test]
fn task_mirror_completed_status_repairs_stale_running_tool_card() {
    let task_id = SessionId::new_v4();
    let mut messages = vec![
        test_spawn_agent_tool_message(task_id, "running", SessionMessageRecordKind::ToolCallOutput),
        test_spawn_agent_tool_message(
            task_id,
            "running",
            SessionMessageRecordKind::ToolResultOutput,
        ),
    ];
    let mirrors = vec![SessionTaskMirrorSnapshot {
        task_session_id: task_id,
        status: String::from("completed"),
        line_count: 3,
        updated_at: String::from("1780000000000"),
    }];

    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        None,
        mirrors.as_slice(),
        |_| false,
    );

    for message in messages {
        let metadata = message.metadata.as_ref().expect("metadata");
        assert_eq!(
            task_session_execution_status_from_tool_metadata(metadata),
            Some("completed")
        );
        assert_eq!(
            metadata
                .get("tool_display_state")
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        assert_eq!(message.title, "子代理 image-worker");
    }
}

/// Verifies live runtime state overrides stale terminal task mirrors.
/// 验证实时运行态会覆盖陈旧的终态任务镜像。
#[test]
fn task_mirror_display_keeps_running_when_task_is_live() {
    let task_id = SessionId::new_v4();
    let mut messages = vec![test_spawn_agent_tool_message(
        task_id,
        "completed",
        SessionMessageRecordKind::ToolCallOutput,
    )];
    let mirrors = vec![SessionTaskMirrorSnapshot {
        task_session_id: task_id,
        status: String::from("completed"),
        line_count: 3,
        updated_at: String::from("1780000000000"),
    }];

    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        None,
        mirrors.as_slice(),
        |_| true,
    );

    let metadata = messages[0].metadata.as_ref().expect("metadata");
    assert_eq!(
        task_session_execution_status_from_tool_metadata(metadata),
        Some("running")
    );
    assert_eq!(
        metadata
            .get("tool_display_state")
            .and_then(serde_json::Value::as_str),
        Some("running")
    );
}

/// Verifies ordinary tool rows do not trigger persisted task mirror loading.
/// 验证普通工具行不会触发持久化任务镜像加载。
#[test]
fn task_mirror_gate_ignores_non_task_tool_rows() {
    let mut message = SessionMessageEntry::new(
        SessionMessageKind::Assistant,
        "调用 shell_command 工具",
        "ok",
    )
    .with_runtime_metadata("tool-shell", "1780000000000");
    message.record_kind = SessionMessageRecordKind::ToolCallOutput;
    message.metadata = Some(json!({
        "tool_name": "shell_command",
        "display_state": "completed"
    }));
    let messages = vec![message];

    assert!(!surface_contains_task_session_tool_card(
        messages.iter(),
        None
    ));
}

/// Verifies task-session tool rows pass the persisted mirror repair gate.
/// 验证 task-session 工具行会通过持久化镜像修复门禁。
#[test]
fn task_mirror_gate_accepts_task_session_tool_rows() {
    let messages = vec![test_spawn_agent_tool_message(
        SessionId::new_v4(),
        "running",
        SessionMessageRecordKind::ToolCallOutput,
    )];

    assert!(surface_contains_task_session_tool_card(
        messages.iter(),
        None
    ));
}
