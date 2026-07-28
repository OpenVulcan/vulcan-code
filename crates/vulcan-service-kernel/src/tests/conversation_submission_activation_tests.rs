use super::*;
use crate::conversation_submission::{
    commit_materialized_submission_activation, prepare_materialized_submission_session,
};

/// Verifies existing-session activation initializes runtime state and clears its own navigation round.
/// 验证已有会话激活会初始化运行态并清理自身导航轮次。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails when activation is split across inconsistent state revisions.
/// 此测试无返回值；若激活流程被拆分到不一致的状态版本则失败。
#[test]
fn existing_session_activation_commits_summary_and_own_navigation_cleanup() {
    // session_id identifies the existing parent session being activated.
    // session_id 标识正在激活的已有父会话。
    let session_id = SessionId::new_v4();
    // unrelated_session_id guards the navigation round owned by another parent.
    // unrelated_session_id 保护另一个父会话拥有的导航轮次。
    let unrelated_session_id = SessionId::new_v4();
    // task_id identifies the child panel currently opened for the activated parent.
    // task_id 标识当前为已激活父会话打开的子任务面板。
    let task_id = SessionId::new_v4();
    // service_state owns the workbench and navigation state committed under one lock.
    // service_state 持有在同一锁内提交的工作台与导航状态。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .session_subagent_navigation_rounds
        .extend([session_id, unrelated_session_id]);
    service_state.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id: session_id,
        task_id,
    });
    // workspace_snapshot is the exact materialized revision passed into turn startup.
    // workspace_snapshot 是传入回合启动的精确已物化版本。
    let mut workspace_snapshot = service_state
        .workspace_snapshot
        .take()
        .expect("test workspace snapshot");
    // state exposes the live projection and navigation ownership to activation.
    // state 向激活阶段提供实时投影与导航所有权。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // materialized_session records that the authoritative materializer selected an existing session.
    // materialized_session 记录权威物化器选择了已有会话。
    let materialized_session = MaterializedPromptSession {
        session_id,
        project_id: Some(String::from("project-a")),
        session_title: String::from("父会话"),
        workspace_path: String::new(),
        kind: PromptSessionMaterializationKind::Existing,
    };
    // app discards the emitted event while preserving the production activation path.
    // app 丢弃发送事件，同时保留生产激活路径。
    let app = ServiceEventEmitter::new(Vec::new());

    // Preparation initializes only the detached workspace and cannot publish sidebar state.
    // 准备阶段只初始化分离工作区，不能发布侧栏状态。
    prepare_materialized_submission_session(&mut workspace_snapshot, &materialized_session)
        .expect("existing session preparation should succeed");
    {
        // guard proves detached preparation has not published a revision or cleared live navigation.
        // guard 证明分离准备尚未发布修订号或清理实时导航。
        let guard = state.lock().expect("service state lock");
        assert_eq!(guard.workbench_generation, 0);
        assert!(
            guard
                .session_subagent_navigation_rounds
                .contains(&session_id)
        );
        assert!(guard.active_subagent_panel.is_some());
    }
    // revision is the single workbench generation committed by post-workspace activation.
    // revision 是工作区提交后激活阶段提交的唯一工作台代际。
    let revision = commit_materialized_submission_activation(
        &app,
        &state,
        &workspace_snapshot,
        &materialized_session,
    )
    .expect("existing session activation should succeed");

    assert_eq!(revision, 1);
    assert!(
        workspace_snapshot
            .session_runtime_state
            .contains_key(&session_id)
    );
    // guard observes summary and navigation mutations from the same committed revision.
    // guard 观察同一已提交版本中的摘要与导航修改。
    let guard = state.lock().expect("service state lock");
    assert!(
        !guard
            .session_subagent_navigation_rounds
            .contains(&session_id)
    );
    assert!(
        guard
            .session_subagent_navigation_rounds
            .contains(&unrelated_session_id)
    );
    assert_eq!(guard.active_subagent_panel, None);
    assert_eq!(
        guard.workbench.selected_conversation_id.as_deref(),
        Some(session_id.to_string().as_str())
    );
    assert_eq!(guard.workbench.conversations[0].id, session_id.to_string());
}

/// Verifies existing-session activation preserves a child panel owned by another parent.
/// 验证已有会话激活会保留另一个父会话拥有的子任务面板。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails when activation clears cross-session navigation state.
/// 此测试无返回值；若激活清除了跨会话导航状态则失败。
#[test]
fn existing_session_activation_preserves_other_parent_panel() {
    // session_id identifies the existing parent session being activated.
    // session_id 标识正在激活的已有父会话。
    let session_id = SessionId::new_v4();
    // other_parent_session_id owns the panel that activation must preserve.
    // other_parent_session_id 拥有激活阶段必须保留的面板。
    let other_parent_session_id = SessionId::new_v4();
    // other_task_id identifies the child task selected under the other parent.
    // other_task_id 标识在另一个父会话下选中的子任务。
    let other_task_id = SessionId::new_v4();
    // expected_panel is the exact cross-session selection protected by ownership checks.
    // expected_panel 是所有权检查保护的精确跨会话选择。
    let expected_panel = SelectedSubagentTaskPanel {
        parent_session_id: other_parent_session_id,
        task_id: other_task_id,
    };
    // service_state owns the unrelated active panel.
    // service_state 持有无关的活动面板。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.active_subagent_panel = Some(expected_panel);
    // workspace_snapshot is the materialized existing-session revision.
    // workspace_snapshot 是已物化的已有会话版本。
    let mut workspace_snapshot = service_state
        .workspace_snapshot
        .take()
        .expect("test workspace snapshot");
    // state exposes the live panel ownership to activation.
    // state 向激活阶段提供实时面板所有权。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    // materialized_session carries the authoritative existing-session outcome.
    // materialized_session 携带权威已有会话结果。
    let materialized_session = MaterializedPromptSession {
        session_id,
        project_id: Some(String::from("project-a")),
        session_title: String::from("父会话"),
        workspace_path: String::new(),
        kind: PromptSessionMaterializationKind::Existing,
    };

    prepare_materialized_submission_session(&mut workspace_snapshot, &materialized_session)
        .expect("existing session preparation should succeed");
    commit_materialized_submission_activation(
        &ServiceEventEmitter::new(Vec::new()),
        &state,
        &workspace_snapshot,
        &materialized_session,
    )
    .expect("existing session activation should succeed");

    assert_eq!(
        state
            .lock()
            .expect("service state lock")
            .active_subagent_panel,
        Some(expected_panel)
    );
}
