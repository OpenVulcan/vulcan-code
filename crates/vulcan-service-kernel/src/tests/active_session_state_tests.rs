//! active session state tests.
//! active session state tests。

use super::*;

/// Metadata-only model controls survive index reloads without claiming that history was hydrated.
/// 纯 metadata 模型控件跨索引重载保留，但不声称历史已水合。
#[test]
fn preserve_active_session_run_state_keeps_metadata_only_model_selection() {
    // The cold session has no active kernel, loaded history, or conversation context.
    // 冷会话没有活动内核、已加载历史或会话上下文。
    let session_id = SessionId::from_u128(0x7120);
    // Only request configuration has been restored before the next generic index refresh.
    // 下一次通用索引刷新之前，仅恢复了请求配置。
    let previous = WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            None,
            "Cold",
            "",
            SessionState::Completed,
            0,
            0,
            "",
        )],
        archived_sessions: Vec::new(),
        session_runtime_state: BTreeMap::from([(
            session_id,
            SessionRuntimeStateEntry {
                active_provider_id: Some("kimi".into()),
                active_connection_id: Some("kimi-account".into()),
                active_model_id: Some("k3-256k".into()),
                ..Default::default()
            },
        )]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    // The SQLite index intentionally has no legacy runtime adapters.
    // SQLite 索引刻意不包含旧运行态适配器。
    let mut refreshed = previous.clone();
    refreshed.session_runtime_state.clear();
    // Capture follows the production generic-refresh path, without submitted/active exceptions.
    // 捕获遵循生产通用刷新路径，不使用已提交或活动中的例外。
    let preserved = capture_preserved_workspace_run_state(Some(&previous), &BTreeSet::new(), None);
    preserve_active_session_run_state(preserved.as_ref(), &mut refreshed);
    assert_eq!(
        refreshed.session_runtime_state[&session_id]
            .active_model_id
            .as_deref(),
        Some("k3-256k")
    );
    assert_eq!(
        refreshed.session_runtime_state[&session_id]
            .active_connection_id
            .as_deref(),
        Some("kimi-account")
    );
    assert!(refreshed.hydrated_session_ids.is_empty());
    assert!(!runtime_state_has_concrete_conversation_context(
        &refreshed.session_runtime_state[&session_id]
    ));
}

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
}

/// Verifies hydrated idle sessions keep their hydration marker across index refreshes.
/// 验证已水合的空闲会话在索引刷新后保留其水合标记。
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

/// Verifies hydrated retained state cannot resurrect a session already archived by the authoritative refresh.
/// 验证已水合保留态不能复活已被权威刷新归档的会话。
#[test]
fn preserve_active_session_run_state_does_not_resurrect_archived_session() {
    // session_id identifies the session moved from the active index into the archived index.
    // session_id 标识从活动索引移动到归档索引的会话。
    let session_id = SessionId::new_v4();
    // project_id owns the session before and after archival.
    // project_id 在归档前后均归属该会话。
    let project_id = String::from("project-a");
    // session is the retained summary captured before the archive transaction commits.
    // session 是归档事务提交前捕获的保留摘要。
    let session = SessionSummary::new(
        session_id,
        Some(project_id.clone()),
        "待归档会话",
        "已水合正文",
        SessionState::Completed,
        0,
        0,
        "刚刚",
    );
    // previous_snapshot models the selected hydrated WebUI state immediately before archival.
    // previous_snapshot 模拟归档前一刻已选中且已水合的 WebUI 状态。
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
        sessions: vec![session.clone()],
        archived_sessions: Vec::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::from([session_id]),
    };
    // refreshed_snapshot is the authoritative post-archive SQLite and directory projection.
    // refreshed_snapshot 是归档后的权威 SQLite 与目录投影。
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
        archived_sessions: vec![session],
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: SessionId::nil(),
        hydrated_session_ids: BTreeSet::new(),
    };
    // preserved_workspace_run_state captures the stale hydrated row that previously overrode archival.
    // preserved_workspace_run_state 捕获此前会覆盖归档事实的过期水合行。
    let preserved_workspace_run_state =
        capture_preserved_workspace_run_state(Some(&previous_snapshot), &BTreeSet::new(), None);

    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );

    assert!(refreshed_snapshot.sessions.is_empty());
    assert_eq!(refreshed_snapshot.archived_sessions.len(), 1);
    assert_eq!(refreshed_snapshot.selected_session, SessionId::nil());
    assert!(
        !refreshed_snapshot
            .hydrated_session_ids
            .contains(&session_id)
    );
}

/// Verifies retained hydration state cannot resurrect a session whose owning project was removed.
/// 验证保留的水合状态不能复活所属项目已经移除的会话。
#[test]
fn preserve_active_session_run_state_does_not_resurrect_removed_project_session() {
    // session_id identifies the retained session excluded by the removed-project tombstone.
    // session_id 标识被项目删除墓碑排除的保留会话。
    let session_id = SessionId::from_u128(0x93);
    // project_id is the exact owner that disappears from the authoritative refresh.
    // project_id 是从权威刷新中消失的精确所属项目。
    let project_id = String::from("removed-project");
    // previous_snapshot models a selected hydrated session immediately before project removal.
    // previous_snapshot 模拟项目移除前一刻选中且已水合的会话。
    let previous_snapshot = WorkspaceSnapshot {
        task_projects: vec![TaskProjectSummary::new(
            project_id.clone(),
            "Removed Project",
            "D:/removed-project",
            false,
            0,
            None,
            None,
            vec![session_id],
        )],
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(project_id),
            "项目删除会话",
            "已水合正文",
            SessionState::Completed,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::from([session_id]),
    };
    // refreshed_snapshot mirrors the loader result after the owner row has been deleted.
    // refreshed_snapshot 镜像所属项目行删除后的加载器结果。
    let mut refreshed_snapshot = previous_snapshot.clone();
    refreshed_snapshot.task_projects.clear();
    refreshed_snapshot.sessions.clear();
    refreshed_snapshot.selected_session = SessionId::nil();
    refreshed_snapshot.hydrated_session_ids.clear();
    // preserved_workspace_run_state captures the stale selected and hydrated in-memory row.
    // preserved_workspace_run_state 捕获过期的选中且已水合内存行。
    let preserved_workspace_run_state =
        capture_preserved_workspace_run_state(Some(&previous_snapshot), &BTreeSet::new(), None);

    preserve_active_session_run_state(
        preserved_workspace_run_state.as_ref(),
        &mut refreshed_snapshot,
    );

    assert!(refreshed_snapshot.sessions.is_empty());
    assert_eq!(refreshed_snapshot.selected_session, SessionId::nil());
}
