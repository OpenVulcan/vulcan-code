use super::*;
use crate::conversation_submission::{
    commit_created_submission_workspace_slice, commit_materialized_submission_activation,
    prepare_materialized_submission_session,
};
use vulcan_service_api::ServiceEventSink;

/// Captures activation events emitted to one simulated WebUI client.
/// 捕获发送给单个模拟 WebUI 客户端的激活事件。
struct CapturingActivationEventSink {
    /// Shared ordered event buffer used by activation assertions.
    /// 激活断言使用的共享有序事件缓冲区。
    events: Arc<Mutex<Vec<ServiceEvent>>>,
}

impl ServiceEventSink for CapturingActivationEventSink {
    /// Stores one activation event in delivery order.
    /// 按投递顺序保存单个激活事件。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the service event delivered by the production emitter.
    /// `event` 是生产事件发送器投递的服务事件。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events
            .lock()
            .expect("activation event buffer should lock")
            .push(event);
    }
}

/// Verifies a created conversation publishes navigation and model-sensitive controls atomically.
/// 验证新建会话会原子发布导航状态与模型敏感控制项。
///
/// # Parameters
/// 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// 返回值
/// This test returns no value and fails when draft materialization emits only a partial summary delta.
/// 此测试无返回值；当草稿物化只发送局部摘要增量时失败。
#[test]
fn created_session_activation_emits_one_full_workbench_snapshot() {
    // historical_session_id keeps one unrelated retained conversation in the live workspace.
    // historical_session_id 在实时工作区中保留一个无关的既有会话。
    let historical_session_id = SessionId::new_v4();
    // service_state is the live workspace that will receive the bounded created-session slice.
    // service_state 是接收有界新建会话切片的实时工作区。
    let mut service_state = test_service_state_with_parent_session(historical_session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot")
        .task_projects
        .push(TaskProjectSummary::new(
            "project-a",
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            Vec::new(),
        ));
    // workspace_snapshot is the detached submission slice used to materialize the new conversation.
    // workspace_snapshot 是用于物化新会话的分离提交切片。
    let mut workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("test workspace snapshot")
        .clone();
    // materialized_session is the exact draft-to-session transition under test.
    // materialized_session 是本测试关注的精确草稿到会话转换。
    let materialized_session = materialize_prompt_session(
        &mut workspace_snapshot,
        Some("project-a"),
        PromptSessionTarget::Create,
        "Model A conversation",
    )
    .expect("created conversation should materialize");
    prepare_materialized_submission_session(&mut workspace_snapshot, &materialized_session)
        .expect("created conversation runtime state should initialize");
    // state exposes the live workbench to the production commit and activation path.
    // state 向生产提交与激活路径暴露实时工作台。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    commit_created_submission_workspace_slice(&state, &workspace_snapshot, &materialized_session)
        .expect("created conversation slice should commit");
    // events records whether activation used an atomic full snapshot or a stale partial delta.
    // events 记录激活阶段使用了原子完整快照还是过期的局部增量。
    let events = Arc::new(Mutex::new(Vec::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingActivationEventSink {
        events: Arc::clone(&events),
    })]);

    commit_materialized_submission_activation(
        &app,
        &state,
        &workspace_snapshot,
        &materialized_session,
    )
    .expect("created conversation activation should succeed");

    let captured = events.lock().expect("activation event buffer should lock");
    assert_eq!(captured.len(), 1);
    let ServiceEvent::Workbench(workbench) = &captured[0] else {
        panic!("created conversation activation must emit a full workbench snapshot");
    };
    assert_eq!(
        workbench.selected_conversation_id.as_deref(),
        Some(materialized_session.session_id.to_string().as_str())
    );
    assert!(
        workbench
            .conversations
            .iter()
            .any(|conversation| conversation.id == materialized_session.session_id.to_string())
    );
}

/// Verifies a background submission creates its conversation without moving the user's selection.
/// 验证后台提交会创建会话，但不会移动用户当前的选中状态。
#[test]
fn background_created_session_activation_keeps_the_selected_conversation() {
    let historical_session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(historical_session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("test workspace snapshot")
        .task_projects
        .push(TaskProjectSummary::new(
            "project-a",
            "Project A",
            "D:/project-a",
            false,
            0,
            None,
            None,
            Vec::new(),
        ));
    let selected_before = service_state.workbench.selected_conversation_id.clone();
    let workspace_selected_before = service_state
        .workspace_snapshot
        .as_ref()
        .expect("test workspace snapshot")
        .selected_session;
    let mut workspace_snapshot = service_state
        .workspace_snapshot
        .as_ref()
        .expect("test workspace snapshot")
        .clone();
    let mut materialized_session = materialize_prompt_session(
        &mut workspace_snapshot,
        Some("project-a"),
        PromptSessionTarget::Create,
        "Background conversation",
    )
    .expect("created conversation should materialize");
    materialized_session.activation = crate::SubmissionActivation::Background;
    prepare_materialized_submission_session(&mut workspace_snapshot, &materialized_session)
        .expect("created conversation runtime state should initialize");
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));
    commit_created_submission_workspace_slice(&state, &workspace_snapshot, &materialized_session)
        .expect("created conversation slice should commit");
    let events = Arc::new(Mutex::new(Vec::new()));
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingActivationEventSink {
        events: Arc::clone(&events),
    })]);

    commit_materialized_submission_activation(
        &app,
        &state,
        &workspace_snapshot,
        &materialized_session,
    )
    .expect("background activation should succeed");

    let guard = state.lock().expect("service state lock");
    assert_eq!(guard.workbench.selected_conversation_id, selected_before);
    assert_eq!(
        guard
            .workspace_snapshot
            .as_ref()
            .expect("live workspace snapshot")
            .selected_session,
        workspace_selected_before
    );
    assert!(
        guard
            .workbench
            .conversations
            .iter()
            .any(|conversation| conversation.id == materialized_session.session_id.to_string())
    );
    drop(guard);
    let captured = events.lock().expect("activation event buffer should lock");
    assert!(
        captured.iter().all(|event| match event {
            ServiceEvent::Workbench(_) => false,
            ServiceEvent::WorkbenchDelta(envelope) => !matches!(
                &envelope.delta,
                WorkbenchDelta::ConversationSummaryChanged(delta)
                    if delta.selected_conversation_id.is_some()
            ),
            _ => true,
        }),
        "a background activation must not publish a selection change"
    );
}

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
    service_state.session_subagent_navigation_rounds.extend([
        (session_id, BTreeSet::from([task_id])),
        (unrelated_session_id, BTreeSet::new()),
    ]);
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
        activation: crate::SubmissionActivation::Foreground,
        kind: PromptSessionMaterializationKind::Existing,
    };
    // emitted_events records the summary delta followed by the exact navigation cleanup delta.
    // emitted_events 记录摘要增量及其后的精确导航清理增量。
    let emitted_events = Arc::new(Mutex::new(Vec::<ServiceEvent>::new()));
    // app preserves production event ordering for the simulated WebUI client.
    // app 为模拟 WebUI 客户端保留生产事件顺序。
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingActivationEventSink {
        events: Arc::clone(&emitted_events),
    })]);

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
                .contains_key(&session_id)
        );
        assert!(guard.active_subagent_panel.is_some());
    }
    // revision includes both the summary update and the focused navigation cleanup.
    // revision 同时包含摘要更新与聚焦导航清理。
    let revision = commit_materialized_submission_activation(
        &app,
        &state,
        &workspace_snapshot,
        &materialized_session,
    )
    .expect("existing session activation should succeed");

    assert_eq!(revision, 2);
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
            .contains_key(&session_id)
    );
    assert!(
        guard
            .session_subagent_navigation_rounds
            .contains_key(&unrelated_session_id)
    );
    assert_eq!(guard.active_subagent_panel, None);
    assert_eq!(
        guard.workbench.selected_conversation_id.as_deref(),
        Some(session_id.to_string().as_str())
    );
    assert_eq!(guard.workbench.conversations[0].id, session_id.to_string());
    drop(guard);
    let events = emitted_events
        .lock()
        .expect("activation event buffer should lock");
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        ServiceEvent::WorkbenchDelta(envelope)
            if matches!(&envelope.delta, WorkbenchDelta::ConversationSummaryChanged(_))
    ));
    assert!(matches!(
        &events[1],
        ServiceEvent::WorkbenchDelta(envelope)
            if envelope.generation == revision
                && matches!(
                    &envelope.delta,
                    WorkbenchDelta::SubagentNavigationChanged(delta)
                        if delta.parent_conversation_id.as_deref()
                            == Some(session_id.to_string().as_str())
                            && delta.subagent_navigation.is_none()
                )
    ));
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
        activation: crate::SubmissionActivation::Foreground,
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
