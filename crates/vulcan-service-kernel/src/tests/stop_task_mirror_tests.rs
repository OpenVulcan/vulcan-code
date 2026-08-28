//! stop task mirror tests.
//! stop task mirror tests。

use super::*;
use crate::conversation_run_services::terminate_active_conversation_hook_turn_after_stop;

/// Verifies stopping an execution closes the retained Hook turn idempotently before Continue.
/// 验证停止执行会在 Continue 前幂等关闭保留的 Hook 回合。
#[test]
fn stop_closes_retained_conversation_hook_turn_idempotently() {
    // sessionId identifies the exact stopped logical conversation.
    // sessionId 标识精确的已停止逻辑会话。
    let session_id = SessionId::new_v4();
    // agentRuntime owns the real Hook runtime used by the stop service.
    // agentRuntime 持有停止服务使用的真实 Hook 运行时。
    let agent_runtime = test_agent_runtime_bridge();
    // retainedTurnId freezes the owner that the stop operation is allowed to terminate.
    // retainedTurnId 冻结停止操作获准终止的所有者。
    let retained_turn_id = {
        // hookRuntime seeds one active turn without starting Provider execution.
        // hookRuntime 在不启动 Provider 执行的情况下播种一个活动回合。
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime fixture should lock");
        hook_runtime.record_queued_user_prompt(
            session_id,
            "prompt-a",
            Some(String::from("project-a")),
            String::from("D:\\workspace"),
            PendingSessionPromptSource::UserFollowup,
            "continue this task",
        );
        hook_runtime
            .active_turn_id(session_id)
            .expect("queued prompt should open one Hook turn")
            .to_string()
    };

    assert!(
        terminate_active_conversation_hook_turn_after_stop(
            &agent_runtime,
            session_id,
            Some(String::from("project-a")),
            Some(retained_turn_id.as_str()),
            "test stop",
        )
        .expect("first stop should close the Hook turn")
    );
    assert!(
        !terminate_active_conversation_hook_turn_after_stop(
            &agent_runtime,
            session_id,
            Some(String::from("project-a")),
            Some(retained_turn_id.as_str()),
            "test repeated stop",
        )
        .expect("repeated stop should remain idempotent")
    );
    assert!(
        !agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime should remain readable")
            .has_active_turn(session_id)
    );
}

/// Verifies stop cleanup never terminates a replacement Hook turn with a different owner id.
/// 验证停止清理绝不会终止具有不同所有者标识的替代 Hook 回合。
#[test]
fn stop_preserves_replacement_conversation_hook_turn() {
    // sessionId identifies the conversation whose Hook owner changes during stop.
    // sessionId 标识停止期间 Hook 所有者发生变化的会话。
    let session_id = SessionId::new_v4();
    // agentRuntime owns the real Hook owner map used by the stop helper.
    // agentRuntime 持有停止辅助逻辑使用的真实 Hook 所有者映射。
    let agent_runtime = test_agent_runtime_bridge();
    // staleTurnId represents the owner captured by the earlier stop request.
    // staleTurnId 表示较早停止请求捕获的所有者。
    let stale_turn_id = String::from("stale-turn-id");
    // replacementTurnId is the current owner established after the stale capture.
    // replacementTurnId 是过期捕获后建立的当前所有者。
    let replacement_turn_id = {
        // hookRuntime creates the replacement owner under the production mutex.
        // hookRuntime 在生产互斥锁下创建替代所有者。
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime fixture should lock");
        hook_runtime.record_queued_user_prompt(
            session_id,
            "prompt-replacement",
            Some(String::from("project-a")),
            String::from("D:\\workspace"),
            PendingSessionPromptSource::UserFollowup,
            "replacement prompt",
        );
        hook_runtime
            .active_turn_id(session_id)
            .expect("replacement prompt should own one Hook turn")
            .to_string()
    };

    assert!(
        !terminate_active_conversation_hook_turn_after_stop(
            &agent_runtime,
            session_id,
            Some(String::from("project-a")),
            Some(stale_turn_id.as_str()),
            "stale stop",
        )
        .expect("stale stop should be rejected without mutation")
    );
    assert_eq!(
        agent_runtime
            .conversation_hook_runtime
            .lock()
            .expect("Hook runtime should remain readable")
            .active_turn_id(session_id),
        Some(replacement_turn_id.as_str())
    );
}

/// Builds one minimal retained Running session for stop-state convergence tests.
/// 为停止状态收敛测试构建一个最小的保留 Running 会话。
fn running_stop_workspace_snapshot(
    session_id: SessionId,
    run_phase: SessionRunPhase,
) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: vec![SessionSummary::new(
            session_id,
            Some(String::from("project-a")),
            "停止收敛测试",
            "正在运行",
            SessionState::Running,
            0,
            0,
            "刚刚",
        )],
        archived_sessions: Vec::new(),
        session_runtime_state: BTreeMap::from([(session_id, SessionRuntimeStateEntry::default())]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, run_phase)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    }
}

/// Verifies a live stream task is sufficient to terminalize a session whose kernel turn is missing.
/// 验证实时流任务足以终结内核回合已经缺失的会话。
#[test]
fn webui_stop_session_work_uses_stream_owner_without_kernel_turn() {
    let session_id = SessionId::new_v4();
    let mut workspace_snapshot =
        running_stop_workspace_snapshot(session_id, SessionRunPhase::Requesting);

    let did_stop = stop_webui_session_work(&mut workspace_snapshot, session_id, false, true, false)
        .expect("stream-owned stop should converge every session status surface");

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    assert_eq!(
        workspace_session_summary_for_session(&workspace_snapshot, session_id)
            .map(|session| session.status),
        Some(SessionState::Blocked)
    );
}

/// Verifies Stop repairs a retained Running summary after both live owners have already disappeared.
/// 验证两个实时所有者均已消失后，Stop 仍会修复残留的 Running 会话摘要。
#[test]
fn webui_stop_session_work_repairs_orphaned_running_summary() {
    let session_id = SessionId::new_v4();
    let mut workspace_snapshot =
        running_stop_workspace_snapshot(session_id, SessionRunPhase::Stopped);

    let did_stop =
        stop_webui_session_work(&mut workspace_snapshot, session_id, false, false, false)
            .expect("orphaned Running summary should be repaired idempotently");

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    assert_eq!(
        workspace_session_summary_for_session(&workspace_snapshot, session_id)
            .map(|session| session.status),
        Some(SessionState::Blocked)
    );
}

/// Verifies WebUI stop projects the same stopped turn records as the WebUI transcript chain.
/// 验证 WebUI 停止流程投影与 WebUI 转录链条一致的已停止轮次记录。
#[test]
fn webui_stop_session_work_appends_stopped_turn_projection_once() {
    let session_id = SessionId::new_v4();
    let mut runtime_state = SessionRuntimeStateEntry::default();
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
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let did_stop = stop_webui_session_work(&mut workspace_snapshot, session_id, true, true, true)
        .expect("active stop should append stopped projection");

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    let runtime_records = &workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("runtime state should exist")
        .conversation_state
        .records;
    assert!(runtime_records.iter().any(|record| {
        !record.visible && record.record_kind == SessionMessageRecordKind::TurnTerminated
    }));
    let turn_stats = runtime_records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("stopped turn stats should be retained in process-local runtime state");
    assert!(turn_stats.visible);
    assert_eq!(turn_stats.title, "Turn Stats · stopped");
    assert!(turn_stats.body.contains("状态：已中止"));
    assert!(turn_stats.body.contains("Provider 调用：0"));
    let stopped_record_count = runtime_records.len();

    let did_stop_again =
        stop_webui_session_work(&mut workspace_snapshot, session_id, false, false, false)
            .expect("repeated stop should remain idempotent");

    assert!(!did_stop_again);
    let records_after_second_stop = &workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("stopped runtime state should still exist")
        .conversation_state
        .records;
    assert_eq!(records_after_second_stop.len(), stopped_record_count);
}

/// Verifies a kernel-active turn still receives a stopped projection when its stream handle is gone.
/// 验证内核活动回合即使流句柄已丢失也仍会收到停止投影。
#[test]
fn webui_stop_session_work_uses_kernel_active_state_without_stream_handle() {
    let session_id = SessionId::new_v4();
    let runtime_state = SessionRuntimeStateEntry::default();
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
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Requesting)]),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };

    let did_stop = stop_webui_session_work(&mut workspace_snapshot, session_id, true, false, false)
        .expect("kernel-active stop should append stopped projection");

    assert!(did_stop);
    assert_eq!(
        workspace_snapshot.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Stopped)
    );
    let records = &workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .expect("kernel-active stop should retain runtime state")
        .conversation_state
        .records;
    assert!(records.iter().any(|record| {
        !record.visible && record.record_kind == SessionMessageRecordKind::TurnTerminated
    }));
}
