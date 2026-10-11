use super::*;

/// Verifies selecting a running session remains a read-only navigation operation.
/// 验证选择运行中会话仍属于只读导航操作。
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
            last_updated_label: String::from("just now"),
            last_updated_at: String::from("1"),
            unread: false,
            attention: None,
            project_id: Some(String::from("project-a")),
        });
    start_webui_session_kernel_user_turn(&mut service_state, session_id, "active-selection-turn")
        .expect("active session turn should start");
    let state = Arc::new(Mutex::new(service_state));

    let selected = service_select_conversation(state, session_id.to_string())
        .await
        .expect("running session selection must remain readable");
    // expected_session_id keeps the owned display form alive for the borrowed comparison.
    // expected_session_id 让拥有所有权的展示形式在借用比较期间保持存活。
    let expected_session_id = session_id.to_string();

    assert_eq!(
        selected.selected_conversation_id.as_deref(),
        Some(expected_session_id.as_str())
    );
}
