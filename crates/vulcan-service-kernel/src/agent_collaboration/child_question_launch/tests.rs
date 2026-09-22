use super::*;

/// Auxiliary preparation creates only child runtime state and rejects a replaced kernel generation.
/// 辅助准备只创建子运行态，并拒绝已被替换的内核代际。
#[test]
fn child_question_launch_projects_runtime_without_ordinary_session() {
    // Reuse the real domain-backed auxiliary execution and its published service document.
    // 复用真实领域辅助执行及已发布的服务文档。
    let (state, handle, execution) = child_execution_ownership::tests::child_ownership_fixture();
    let child_id = handle.session_id();
    {
        let mut service = state.lock().unwrap();
        let workspace = service.workspace_snapshot.as_mut().unwrap();
        workspace.sessions.clear();
        workspace.session_runtime_state.remove(&child_id);
    }
    // Submission admission is identical to production and remains held through phase publication.
    // 提交准入与生产相同，并在阶段发布期间持续持有。
    let admission =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            &state, child_id,
        )
        .unwrap()
        .unwrap();
    let reservation_id = admission
        .existing_submission_authorization(child_id)
        .unwrap()
        .reservation_id;
    let generation = {
        let mut service = state.lock().unwrap();
        crate::start_reserved_webui_session_kernel_turn(
            &mut service,
            child_id,
            "qa-preparing",
            ActiveTurnKind::QuestionExecution {
                execution_id: execution.execution_id.clone(),
            },
            reservation_id,
        )
        .unwrap();
        crate::webui_session_kernel_generation(&service, child_id).unwrap()
    };
    // The real event emitter needs no connected UI for retained-state verification.
    // 验证保留状态时，真实事件发送器不需要已连接界面。
    let app = crate::ServiceEventEmitter::new(Vec::new());
    publish_child_question_preparing(
        &app,
        &state,
        child_id,
        "qa-preparing",
        generation,
        &execution,
    )
    .unwrap();
    {
        let service = state.lock().unwrap();
        let workspace = service.workspace_snapshot.as_ref().unwrap();
        assert!(workspace.sessions.is_empty());
        assert_eq!(
            workspace.session_runtime_state[&child_id]
                .active_precheck_turn_id
                .as_deref(),
            Some("qa-preparing")
        );
        assert_eq!(
            workspace.session_run_phases.get(&child_id),
            Some(&SessionRunPhase::Preparing)
        );
    }
    assert!(
        publish_child_question_preparing(
            &app,
            &state,
            child_id,
            "wrong-turn",
            generation,
            &execution
        )
        .is_err()
    );
    assert!(
        publish_child_question_preparing(
            &app,
            &state,
            child_id,
            "qa-preparing",
            generation + 1,
            &execution
        )
        .is_err()
    );
    assert_eq!(
        handle
            .read(|document| document.collaboration_run_control())
            .unwrap(),
        CollaborationRunControl::NaturallyIdle
    );
}
