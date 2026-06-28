//! active session state tests.
//! active session state tests。

use super::*;

/// Verifies stale workspace refreshes cannot drop an in-memory prompt turn that has not reached disk.
/// 验证陈旧工作区刷新不能丢弃尚未落盘的内存提示词回合。
#[test]
fn preserve_active_session_run_state_keeps_new_precheck_runtime_state() {
    let session_id = SessionId::new_v4();
    let project_id = String::from("project-a");
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.active_precheck_turn_id = Some(String::from("turn-1"));
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id.clone(),
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(project_id.clone()),
            "新会话",
            "正在回忆",
            SessionState::Running,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            session_id,
            vec![SessionMessageEntry::new(
                SessionMessageKind::User,
                "用户",
                "新提示词",
            )],
        )]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::MemoryRecall)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::from([session_id]),
    };
    let mut refreshed_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id,
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            Vec::new(),
        )],
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };

    let preserved_workspace_run_state = capture_preserved_workspace_run_state(
        Some(&previous_snapshot),
        &BTreeSet::from([session_id]),
        Some(session_id),
    );
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );

    assert_eq!(refreshed_snapshot.selected_session, session_id);
    assert!(
        refreshed_snapshot
            .sessions
            .iter()
            .any(|session| session.id == session_id)
    );
    assert_eq!(
        refreshed_snapshot
            .session_runtime_state
            .get(&session_id)
            .and_then(|state| state.active_precheck_turn_id.as_deref()),
        Some("turn-1")
    );
    assert_eq!(
        refreshed_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::MemoryRecall)
    );
}

/// Verifies submitted sessions preserve in-memory context even without an active kernel turn.
/// 验证提交目标会话即使没有活动内核回合也会保留内存上下文。
#[test]
fn preserve_active_session_run_state_keeps_submitted_session_context() {
    let session_id = SessionId::new_v4();
    let project_id = String::from("project-a");
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id.clone(),
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(project_id.clone()),
            "内存会话",
            "刚输入",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            session_id,
            vec![SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "助手",
                "尚未落盘的回答",
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
        hydrated_session_ids: BTreeSet::from([session_id]),
    };
    let mut refreshed_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id,
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            Vec::new(),
        )],
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };

    let preserved_workspace_run_state = capture_preserved_workspace_run_state(
        Some(&previous_snapshot),
        &BTreeSet::new(),
        Some(session_id),
    );
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );

    assert_eq!(refreshed_snapshot.selected_session, session_id);
    assert!(
        refreshed_snapshot.sessions.iter().any(|session| {
            session.id == session_id && session.status == SessionState::Completed
        })
    );
    assert_eq!(
        refreshed_snapshot
            .task_projects
            .first()
            .map(|project| project.session_ids.as_slice()),
        Some(&[session_id][..])
    );
    assert_eq!(
        refreshed_snapshot
            .session_messages
            .get(&session_id)
            .map(Vec::len),
        Some(1)
    );
}

/// Verifies hydrated idle sessions keep their retained transcript window across index refreshes.
/// 验证已水合的空闲会话在索引刷新后保留其转录窗口。
#[test]
fn preserve_active_session_run_state_keeps_hydrated_idle_session_context() {
    let session_id = SessionId::new_v4();
    let project_id = String::from("project-a");
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id.clone(),
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(project_id.clone()),
            "已加载会话",
            "已有正文",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([(
            session_id,
            vec![SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "助手",
                "已保留的正文",
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
        hydrated_session_ids: BTreeSet::from([session_id]),
    };
    let mut refreshed_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id,
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "已加载会话",
            "刷新索引",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };

    let preserved_workspace_run_state =
        capture_preserved_workspace_run_state(Some(&previous_snapshot), &BTreeSet::new(), None);
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );

    assert_eq!(
        refreshed_snapshot
            .session_messages
            .get(&session_id)
            .map(Vec::len),
        Some(1)
    );
    assert!(
        refreshed_snapshot
            .hydrated_session_ids
            .contains(&session_id)
    );
}

/// Verifies concrete runtime dialogue context survives an index refresh even when the marker is absent.
/// 验证即使水合标记缺失，具体运行态对话上下文也会在索引刷新后保留。
#[test]
fn preserve_active_session_run_state_keeps_runtime_context_without_hydrated_marker() {
    let session_id = SessionId::new_v4();
    let project_id = String::from("project-a");
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state.conversation_state.messages = vec![SessionConversationMessage {
        id: String::from("runtime-user-1"),
        payload: SessionConversationPayload::UserText {
            text: String::from("新对话的运行态正文"),
            attachments: Vec::new(),
        },
    }];
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id.clone(),
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(project_id.clone()),
            "新对话",
            "已有运行态",
            SessionState::Completed,
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
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    let mut refreshed_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id,
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "新对话",
            "刷新索引",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };

    let preserved_workspace_run_state =
        capture_preserved_workspace_run_state(Some(&previous_snapshot), &BTreeSet::new(), None);
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );

    let runtime_state = refreshed_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime context should survive refresh");
    assert_eq!(runtime_state.conversation_state.messages.len(), 1);
    assert!(
        refreshed_snapshot
            .hydrated_session_ids
            .contains(&session_id)
    );
}

/// Verifies WebUI snapshots include every Rust-retained hydrated conversation body.
/// 验证 WebUI 快照会包含 Rust 保留的全部已水合会话正文。
#[test]
fn append_hydrated_workspace_messages_keeps_idle_loaded_conversations_visible() {
    let active_session_id = SessionId::new_v4();
    let idle_session_id = SessionId::new_v4();
    let mut messages = conversation_messages_from_session(
        &active_session_id.to_string(),
        &[SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "助手",
            "活动正文",
        )],
        false,
    );
    let workspace_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([
            (
                active_session_id,
                vec![SessionMessageEntry::new(
                    SessionMessageKind::Assistant,
                    "助手",
                    "活动正文",
                )],
            ),
            (
                idle_session_id,
                vec![SessionMessageEntry::new(
                    SessionMessageKind::Assistant,
                    "助手",
                    "空闲正文",
                )],
            ),
        ]),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: active_session_id,
        hydrated_session_ids: BTreeSet::from([active_session_id, idle_session_id]),
    };

    append_hydrated_workspace_messages(&mut messages, &workspace_snapshot);
    let conversation_ids = messages
        .iter()
        .map(|message| message.conversation_id.as_str())
        .collect::<BTreeSet<_>>();

    assert!(conversation_ids.contains(active_session_id.to_string().as_str()));
    assert!(conversation_ids.contains(idle_session_id.to_string().as_str()));
}

/// Verifies stale running markers do not revive a session without an active kernel turn.
/// 验证没有活动内核回合时过时运行标记不会复活会话。
#[test]
fn preserve_active_session_run_state_ignores_stale_running_without_kernel() {
    let session_id = SessionId::new_v4();
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "旧运行会话",
            "仍显示运行",
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
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    let mut refreshed_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "旧运行会话",
            "已刷新",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };

    let preserved_workspace_run_state =
        capture_preserved_workspace_run_state(Some(&previous_snapshot), &BTreeSet::new(), None);
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );

    let session = refreshed_snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .expect("session should still exist");
    assert_eq!(session.status, SessionState::Completed);
    assert!(
        !refreshed_snapshot
            .session_run_phases
            .contains_key(&session_id)
    );
}
