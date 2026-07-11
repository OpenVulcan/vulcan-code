//! stop task mirror tests.
//! stop task mirror tests。

use super::*;

/// Verifies WebUI stop projects the same stopped turn records as the WebUI transcript chain.
/// 验证 WebUI 停止流程投影与 WebUI 转录链条一致的已停止轮次记录。
#[test]
fn webui_stop_session_work_appends_stopped_turn_projection_once() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry {
        last_safe_provider_request_count: Some(2),
        ..SessionRuntimeStateEntry::default()
    };
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
    )
    .expect("valid stopped-turn record identity");

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

    let did_stop = stop_webui_session_work(&mut workspace_snapshot, session_id, true, true)
        .expect("active stop should append stopped projection");

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
        message.visible && message.title == "执行已停止" && message.body.contains("运行中的智能体")
    }));
    let runtime_records = &workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should exist")
        .conversation_state
        .records;
    assert!(runtime_records.iter().any(|record| {
        !record.visible && record.record_kind == SessionMessageRecordKind::TurnTerminated
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

    let did_stop_again = stop_webui_session_work(&mut workspace_snapshot, session_id, false, false)
        .expect("repeated stop should remain idempotent");

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

    let did_stop = stop_webui_session_work(&mut workspace_snapshot, session_id, true, false)
        .expect("kernel-active stop should append stopped projection");

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
    assert!(messages.iter().all(|message| {
        !(message.visible
            && message.title == "执行已停止"
            && message.body.contains("运行中的智能体"))
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

/// Verifies task mirror status repair keeps projected messages and runtime records synchronized.
/// 验证任务镜像状态修复会同步投影消息与运行时记录。
#[test]
fn task_mirror_completed_status_repairs_projected_card_and_runtime_record() {
    // Child task identity shared by the projected card and canonical runtime record.
    // 投影卡片与规范运行时记录共享的子任务身份。
    let task_id = SessionId::new_v4();
    // Projected parent task card shown in the WebUI transcript.
    // WebUI 转录中展示的父级任务卡片。
    let mut message =
        test_spawn_agent_tool_message(task_id, "running", SessionMessageRecordKind::ToolCallOutput);
    message.id = String::from("shared-task-card");
    // Runtime state holding the canonical parent conversation record for the same card.
    // 持有同一任务卡片规范父级会话记录的运行时状态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: message.id.clone(),
            kind: message.kind,
            record_kind: message.record_kind,
            visible: message.visible,
            title: message.title.clone(),
            body: message.body.clone(),
            metadata: message.metadata.clone(),
            created_at: message.created_at.clone(),
        },
    )
    .expect("valid task-card record identity");
    let mut messages = vec![message];
    let mirrors = vec![SessionTaskMirrorSnapshot {
        task_session_id: task_id,
        status: String::from("completed"),
        line_count: 3,
        updated_at: String::from("1780000000000"),
    }];

    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        Some(&mut runtime_state),
        mirrors.as_slice(),
        |_| false,
    );

    let projected_metadata = messages[0].metadata.as_ref().expect("projected metadata");
    let runtime_metadata = runtime_state.conversation_state.records[0]
        .metadata
        .as_ref()
        .expect("runtime metadata");
    assert_eq!(
        task_session_execution_status_from_tool_metadata(projected_metadata),
        Some("completed")
    );
    assert_eq!(
        task_session_execution_status_from_tool_metadata(runtime_metadata),
        Some("completed")
    );
}

/// Verifies active task mirrors without live runtime are shown as stopped instead of completed.
/// 验证没有实时运行态的活动任务镜像会显示为已停止，而不是伪装成已完成。
#[test]
fn task_mirror_active_status_without_live_runtime_repairs_to_stopped() {
    let task_id = SessionId::new_v4();
    let mut messages = vec![test_spawn_agent_tool_message(
        task_id,
        "running",
        SessionMessageRecordKind::ToolCallOutput,
    )];
    let mirrors = vec![SessionTaskMirrorSnapshot {
        task_session_id: task_id,
        status: String::from("running"),
        line_count: 3,
        updated_at: String::from("1780000000000"),
    }];

    apply_task_mirror_statuses_to_parent_messages(
        messages.as_mut_slice(),
        None,
        mirrors.as_slice(),
        |_| false,
    );

    let metadata = messages[0].metadata.as_ref().expect("metadata");
    assert_eq!(
        task_session_execution_status_from_tool_metadata(metadata),
        Some("stopped")
    );
    assert_eq!(
        metadata
            .get("tool_display_state")
            .and_then(serde_json::Value::as_str),
        Some("stopped")
    );
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

/// Verifies WebUI task-card lookup keeps newest projected messages before runtime records.
/// 验证 WebUI task 卡片查找保持投影消息最新优先且先于运行时记录。
#[test]
fn webui_task_session_lookup_keeps_newest_projected_message_priority() {
    // Parent session that owns the visible spawn-agent task card.
    // 拥有可见 spawn-agent task 卡片的父会话。
    let parent_session_id = SessionId::new_v4();
    // Child task identity shared by every matching parent card candidate.
    // 每个父级卡片候选项共享的子任务身份。
    let task_id = SessionId::new_v4();
    // Lookup payload emitted by delegate task-session metadata.
    // 由委派 task-session 元数据发出的查找载荷。
    let payload = json!({
        "task_id": "image-worker",
        (SUBAGENT_TASK_ID_KEY): task_id.to_string(),
    });
    // Older projected card that must lose to the newer projected card.
    // 必须让位于较新投影卡片的较旧投影卡片。
    let mut older_message =
        test_spawn_agent_tool_message(task_id, "running", SessionMessageRecordKind::ToolCallOutput);
    older_message.id = String::from("older-message");
    // Newer projected card that should win the best-match lookup.
    // 应在最佳匹配查找中胜出的较新投影卡片。
    let mut newer_message =
        test_spawn_agent_tool_message(task_id, "running", SessionMessageRecordKind::ToolCallOutput);
    newer_message.id = String::from("newer-message");
    // Runtime-only card that the all-match lookup must still include.
    // 全量匹配查找仍然必须包含的仅运行时卡片。
    let mut runtime_message = test_spawn_agent_tool_message(
        task_id,
        "running",
        SessionMessageRecordKind::ToolResultOutput,
    );
    runtime_message.id = String::from("runtime-record");
    // Runtime state holding the canonical parent conversation record.
    // 持有父级规范会话记录的运行时状态。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_record(
        &mut runtime_state.conversation_state,
        SessionConversationRecord {
            id: runtime_message.id.clone(),
            kind: runtime_message.kind,
            record_kind: runtime_message.record_kind,
            visible: runtime_message.visible,
            title: runtime_message.title.clone(),
            body: runtime_message.body.clone(),
            metadata: runtime_message.metadata.clone(),
            created_at: runtime_message.created_at.clone(),
        },
    )
    .expect("valid runtime task record identity");
    // Workspace snapshot with both projected messages and canonical runtime records.
    // 同时包含投影消息与规范运行时记录的工作区快照。
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(parent_session_id, vec![older_message, newer_message])]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(parent_session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: parent_session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    assert_eq!(
        find_webui_task_session_tool_message_id_for_payload(
            &workspace_snapshot,
            parent_session_id,
            &payload
        )
        .as_deref(),
        Some("newer-message")
    );
    assert_eq!(
        find_webui_task_session_tool_message_ids_for_payload(
            &workspace_snapshot,
            parent_session_id,
            &payload
        ),
        BTreeSet::from([
            String::from("newer-message"),
            String::from("older-message"),
            String::from("runtime-record"),
        ])
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
    let messages = [message];

    assert!(!surface_contains_task_session_tool_card(
        messages.iter(),
        None
    ));
}

/// Verifies task-session tool rows pass the persisted mirror repair gate.
/// 验证 task-session 工具行会通过持久化镜像修复门禁。
#[test]
fn task_mirror_gate_accepts_task_session_tool_rows() {
    let messages = [test_spawn_agent_tool_message(
        SessionId::new_v4(),
        "running",
        SessionMessageRecordKind::ToolCallOutput,
    )];

    assert!(surface_contains_task_session_tool_card(
        messages.iter(),
        None
    ));
}
