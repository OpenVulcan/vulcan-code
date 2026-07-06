//! active session state tests.
//! active session state tests。

use super::*;
use crate::session_hydration::capture_preserved_workspace_run_state_for_session_hydration;

/// Verifies stale workspace refreshes cannot drop an in-memory prompt turn that has not reached disk.
/// 验证陈旧工作区刷新不能丢弃尚未落盘的内存提示词回合。
#[test]
fn preserve_active_session_run_state_keeps_new_precheck_runtime_state() {
    let session_id = SessionId::new_v4();
    let project_id = String::from("project-a");
    let runtime_state = SessionRuntimeStateEntry {
        active_precheck_turn_id: Some(String::from("turn-1")),
        ..SessionRuntimeStateEntry::default()
    };
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

/// Verifies live pending-question projections do not broadcast unrelated hydrated transcript bodies.
/// 验证实时待回答问题投影不会广播无关已水合会话正文。
#[test]
fn live_pending_question_workbench_keeps_messages_on_selected_surface() {
    let active_session_id = SessionId::new_v4();
    let idle_session_id = SessionId::new_v4();
    let active_conversation_id = active_session_id.to_string();
    let idle_conversation_id = idle_session_id.to_string();
    let mut service_state = test_service_state_with_parent_session(active_session_id);
    service_state.workbench.selected_conversation_id = Some(active_conversation_id.clone());
    service_state.workbench.messages = conversation_messages_from_session(
        &active_conversation_id,
        &[SessionMessageEntry::new(
            SessionMessageKind::Assistant,
            "助手",
            "活动正文",
        )],
        false,
    );
    {
        let workspace_snapshot = service_state
            .workspace_snapshot
            .as_mut()
            .expect("test state should contain a workspace snapshot");
        workspace_snapshot.sessions.push(SessionSummary::new(
            idle_session_id,
            Some(String::from("project-a")),
            "空闲会话",
            "",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        ));
        workspace_snapshot.session_messages.insert(
            idle_session_id,
            vec![SessionMessageEntry::new(
                SessionMessageKind::Assistant,
                "助手",
                "空闲正文",
            )],
        );
        workspace_snapshot
            .hydrated_session_ids
            .extend([active_session_id, idle_session_id]);
        retain_hydrated_conversation_windows_for_workspace(
            &mut service_state.hydrated_conversation_windows,
            workspace_snapshot,
        );
    }

    let workbench = workbench_with_live_pending_question(&service_state);
    let conversation_ids = workbench
        .messages
        .iter()
        .map(|message| message.conversation_id.as_str())
        .collect::<BTreeSet<_>>();

    assert!(conversation_ids.contains(active_conversation_id.as_str()));
    assert!(!conversation_ids.contains(idle_conversation_id.as_str()));
    assert_eq!(workbench.hydrated_conversation_windows.len(), 2);
}

/// Verifies display hydration does not copy unrelated hydrated transcript windows.
/// 验证显示水合不会复制无关的已水合转录窗口。
#[test]
fn session_hydration_state_preserves_only_requested_transcript_window() {
    let selected_session_id = SessionId::new_v4();
    let unrelated_session_id = SessionId::new_v4();
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![
            SessionSummary::new(
                selected_session_id,
                None,
                "当前会话",
                "已完成",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
            SessionSummary::new(
                unrelated_session_id,
                None,
                "无关会话",
                "已完成",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            ),
        ],
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::from([
            (
                selected_session_id,
                vec![SessionMessageEntry::new(
                    SessionMessageKind::Assistant,
                    "助手",
                    "当前正文",
                )],
            ),
            (
                unrelated_session_id,
                vec![SessionMessageEntry::new(
                    SessionMessageKind::Assistant,
                    "助手",
                    "无关正文",
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
        selected_session: selected_session_id,
        hydrated_session_ids: BTreeSet::from([selected_session_id, unrelated_session_id]),
    };
    let mut refreshed_snapshot = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: previous_snapshot.sessions.clone(),
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

    let preserved_workspace_run_state = capture_preserved_workspace_run_state_for_session_hydration(
        Some(&previous_snapshot),
        &BTreeSet::new(),
        selected_session_id,
    );
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );

    assert_eq!(refreshed_snapshot.selected_session, selected_session_id);
    assert!(
        refreshed_snapshot
            .session_messages
            .contains_key(&selected_session_id)
    );
    assert!(
        !refreshed_snapshot
            .session_messages
            .contains_key(&unrelated_session_id)
    );
    assert!(
        refreshed_snapshot
            .hydrated_session_ids
            .contains(&selected_session_id)
    );
    assert!(
        !refreshed_snapshot
            .hydrated_session_ids
            .contains(&unrelated_session_id)
    );
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

/// Verifies preserved sessions with blank project ids do not rejoin blank project nodes.
/// 验证带空白项目标识的保留会话不会重新加入空白项目节点。
#[test]
fn preserve_active_session_run_state_ignores_blank_project_membership() {
    // session_id identifies the active session retained across refresh.
    // session_id 标识刷新期间被保留的活动会话。
    let session_id = SessionId::new_v4();
    // previous_snapshot contains a blank project id that must be treated as no project.
    // previous_snapshot 包含必须视为无项目的空白项目标识。
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            "   ",
            "Blank Project",
            "D:/blank-project",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("   ")),
            "空白项目会话",
            "正在刷新",
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
        session_runtime_state: BTreeMap::from([(session_id, SessionRuntimeStateEntry::default())]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    // refreshed_snapshot still contains the blank project node, making accidental membership visible.
    // refreshed_snapshot 仍包含空白项目节点，以便观察误加入成员关系。
    let mut refreshed_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            "   ",
            "Blank Project",
            "D:/blank-project",
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
        None,
    );
    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );

    assert!(
        refreshed_snapshot
            .sessions
            .iter()
            .any(|session| session.id == session_id)
    );
    assert!(refreshed_snapshot.task_projects[0].session_ids.is_empty());
}
