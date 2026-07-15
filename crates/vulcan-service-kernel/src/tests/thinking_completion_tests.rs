use super::*;

/// Verifies that one thinking completion seals the active row before restoring the requesting phase.
/// 验证单次思考完成会先封口活动思考行，再恢复请求阶段。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if retained collapse state or the session phase is incorrect.
/// 此测试无返回值；若保留折叠状态或会话阶段不正确则失败。
#[test]
fn thinking_completion_collapses_active_record_and_restores_request_phase() {
    // session_id identifies the single regular session owned by the test state.
    // session_id 标识测试状态持有的唯一常规会话。
    let session_id = Uuid::new_v4();
    // thinking_record_id identifies the active streamed thinking row to seal.
    // thinking_record_id 标识需要封口的活动流式思考行。
    let thinking_record_id = String::from("thinking-record");
    // state owns the minimal retained workspace used by the semantic completion operation.
    // state 持有语义完成操作使用的最小保留工作区。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        test_service_state_with_parent_session(session_id),
    ));
    // app discards emitted deltas because this test asserts the authoritative retained result.
    // app 丢弃发送的增量，因为本测试断言权威保留结果。
    let app = ServiceEventEmitter::new(Vec::new());
    // record_ids binds completion to the exact thinking row created by prior deltas.
    // record_ids 将完成事件绑定到先前增量创建的精确思考行。
    let record_ids = SessionStreamingRecordIds {
        thinking_record_id: Some(thinking_record_id.clone()),
        ..SessionStreamingRecordIds::default()
    };

    complete_streaming_thinking_phase(&app, &state, session_id, &record_ids)
        .expect("thinking completion should update retained state");

    // guard exposes the retained result after the semantic operation returns.
    // guard 在语义操作返回后提供保留结果访问。
    let guard = state.lock().expect("service state should remain available");
    // workspace is the authoritative retained workspace mutated by the completion operation.
    // workspace 是完成操作修改的权威保留工作区。
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .expect("workspace snapshot should remain available");
    // runtime_state owns the collapsed-message set for the completed session.
    // runtime_state 持有已完成会话的折叠消息集合。
    let runtime_state = workspace
        .session_runtime_state
        .get(&session_id)
        .expect("thinking completion should materialize runtime state");

    assert!(
        runtime_state
            .ui_state
            .collapsed_message_ids
            .contains(&thinking_record_id)
    );
    assert_eq!(
        workspace.session_run_phases.get(&session_id),
        Some(&SessionRunPhase::Requesting)
    );
}
