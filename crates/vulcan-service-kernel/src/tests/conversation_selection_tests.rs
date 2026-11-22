use super::*;

/// Verifies navigation stays read-only and returns the original construction clock of a running session.
/// 验证导航保持只读，且返回运行中会话的原始构造计时起点。
#[tokio::test]
async fn selecting_active_session_does_not_require_lifecycle_quiescence() {
    // session_id identifies the exact running session selected by the user.
    // session_id 标识用户选择的精确运行中会话。
    let session_id = SessionId::from_u128(0x51ec7100);
    // controller_root owns a production-shaped local V1 storage fixture for selection preflight.
    // controller_root 为选择预检持有生产形态的本地 V1 存储夹具。
    let controller_root = TestHostRuntimeRoot::new(session_id);
    // controller_client keeps the real isolated controller alive throughout selection.
    // controller_runtime 在整个选择过程维持真实隔离宿主存储存活。
    let controller_runtime = controller_root.attached_host_runtime_handle().await;
    // service_state combines the retained session, V1 controller, and one active kernel Turn.
    // service_state 组合保留会话、V1 控制器与一个活动 kernel Turn。
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.controller_runtime = Some(controller_runtime);
    // retained_conversation mirrors the already hydrated row selected by the navigation path.
    // retained_conversation 模拟导航路径选择的已水合会话行。
    service_state
        .workbench
        .conversations
        .push(ConversationSummary {
            id: session_id.to_string(),
            title: String::from("Running session"),
            status: SessionState::Completed.label().to_string(),
            run_phase: SessionRunPhase::Requesting.to_string(),
            tool_request_started_at_ms: None,
            last_updated_label: String::from("just now"),
            last_updated_at: String::from("1"),
            created_at: String::from("1"),
            last_request_at: String::from("1"),
            manual_position: None,
            unread: false,
            attention: None,
            project_id: Some(String::from("project-a")),
            dedicated: false,
            assistant_name: None,
        });
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "active-selection-turn")
        .expect("active session turn should start");
    crate::agent_stream::apply_session_phase_to_state(
        &mut service_state,
        session_id,
        SessionRunPhase::ToolCalling,
    )
    .expect("tool request phase should start");
    // The clock belongs to the selected session even when other rows are present.
    // 即使存在其他会话行，计时仍属于选中的会话。
    let expected_session_id = session_id.to_string();
    // An older retained clock ensures a same-millisecond update cannot hide a reset in this test.
    // 使用较早的保留起点，避免同一毫秒内的更新掩盖计时重置。
    let conversation = service_state
        .workbench
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == expected_session_id)
        .expect("retained row");
    let start_ms = conversation
        .tool_request_started_at_ms
        .expect("Core should stamp the initial phase")
        - 5_000;
    conversation.tool_request_started_at_ms = Some(start_ms);
    // A later argument fragment republishes the phase with the same start.
    // 后续参数片段重新发布同一阶段时沿用原始起点。
    let progress = crate::agent_stream::apply_session_phase_to_state(
        &mut service_state,
        session_id,
        SessionRunPhase::ToolCalling,
    )
    .expect("tool request phase should advance");
    let WorkbenchDelta::UpdateConversationRunPhase(progress) = progress.delta else {
        panic!("expected a conversation phase update");
    };
    assert_eq!(progress.tool_request_started_at_ms, Some(start_ms));
    // A full projection starts without runtime-only clocks and reconciles them from retained state.
    // 完整投影起初没有仅运行时的计时起点，随后从保留状态协调补齐。
    let mut rebuilt = service_state.workbench.clone();
    for conversation in &mut rebuilt.conversations {
        conversation.tool_request_started_at_ms = None;
    }
    crate::workbench_refresh::reconcile_workbench_conversation_lifecycle_projection(
        &mut rebuilt,
        &service_state,
        service_state
            .workspace_snapshot
            .as_ref()
            .expect("workspace retained"),
    )
    .expect("full projection should reconcile live phase");
    assert!(
        rebuilt
            .conversations
            .iter()
            .any(|conversation| conversation.id == expected_session_id
                && conversation.tool_request_started_at_ms == Some(start_ms))
    );
    let state = Arc::new(Mutex::new(service_state));

    let selected = service_select_conversation(
        ServiceEventEmitter::new(Vec::new()),
        state.clone(),
        session_id.to_string(),
    )
    .await
    .expect("running session selection must remain readable");
    assert_eq!(
        selected.selected_conversation_id.as_deref(),
        Some(expected_session_id.as_str())
    );
    assert!(
        selected
            .conversations
            .iter()
            .any(|conversation| conversation.id == expected_session_id
                && conversation.tool_request_started_at_ms == Some(start_ms))
    );
    // Execution ends the construction phase and retires its clock.
    // 开始执行工具后结束构造阶段并清空计时起点。
    let mut service_state = state.lock().expect("service state available");
    crate::agent_stream::apply_session_phase_to_state(
        &mut service_state,
        session_id,
        SessionRunPhase::ToolRunning,
    )
    .expect("tool execution phase should replace construction");
    assert!(
        service_state
            .workbench
            .conversations
            .iter()
            .any(|conversation| conversation.id == expected_session_id
                && conversation.tool_request_started_at_ms.is_none())
    );
}
