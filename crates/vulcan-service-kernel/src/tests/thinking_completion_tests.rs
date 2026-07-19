use super::*;

/// Captures streamed workbench events for lifecycle protocol assertions.
/// 为生命周期协议断言捕获流式工作台事件。
struct CapturingThinkingEventSink {
    /// Ordered service events emitted by the projection path.
    /// 投影路径发出的有序服务事件。
    events: Arc<Mutex<Vec<ServiceEvent>>>,
}

impl vulcan_service_api::ServiceEventSink for CapturingThinkingEventSink {
    /// Appends one exact service event to the shared ordered buffer.
    /// 将一条精确服务事件追加到共享有序缓冲区。
    ///
    /// # Parameters
    /// # 参数
    /// `event` is the exact event emitted by the production projection path.
    /// `event` 是生产投影路径发出的精确事件。
    ///
    /// # Returns
    /// # 返回值
    /// This method returns no value after the event is retained.
    /// 当前事件被保留后，此方法无返回值。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events
            .lock()
            .expect("thinking event sink should remain available")
            .push(event);
    }
}

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
    // state owns the minimal retained workspace used by the semantic completion operation.
    // state 持有语义完成操作使用的最小保留工作区。
    let state: SharedServiceBridgeState = Arc::new(Mutex::new(
        test_service_state_with_parent_session(session_id),
    ));
    // events captures both the delta-only active event and the authoritative sealed event.
    // events 同时捕获仅增量活跃事件与权威封口事件。
    let events = Arc::new(Mutex::new(Vec::new()));
    // app routes production projection events into the test-owned ordered buffer.
    // app 将生产投影事件路由到测试持有的有序缓冲区。
    let app = ServiceEventEmitter::new(vec![Arc::new(CapturingThinkingEventSink {
        events: events.clone(),
    })]);
    // record_ids binds completion to the exact thinking row created by prior deltas.
    // record_ids 将完成事件绑定到先前增量创建的精确思考行。
    let mut record_ids = SessionStreamingRecordIds::default();

    append_stream_content_delta(
        &app,
        &state,
        session_id,
        &mut record_ids,
        StreamContentKind::Thinking,
        "正在分析",
    )
    .expect("thinking delta should materialize one active stream row");
    let thinking_record_id = record_ids
        .thinking_record_id
        .clone()
        .expect("thinking delta should assign one record identity");

    complete_streaming_thinking_phase(&app, &state, session_id, &mut record_ids)
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
    assert_eq!(
        runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.id == thinking_record_id)
            .and_then(|record| record.metadata.as_ref())
            .and_then(|metadata| metadata.get(STREAMING_RECORD_STATE_METADATA_KEY))
            .and_then(serde_json::Value::as_str),
        Some(STREAMING_RECORD_STATE_SEALED)
    );
    drop(guard);

    // captured verifies the transport contract separately from retained-state assertions.
    // captured 独立于保留状态断言验证传输契约。
    let captured = events
        .lock()
        .expect("thinking event buffer should remain available");
    let stream_deltas = captured
        .iter()
        .filter_map(|event| {
            let ServiceEvent::WorkbenchDelta(envelope) = event else {
                return None;
            };
            let WorkbenchDelta::AppendMessageBody(delta) = &envelope.delta else {
                return None;
            };
            Some(delta)
        })
        .collect::<Vec<_>>();
    assert_eq!(stream_deltas.len(), 2);
    assert_eq!(
        stream_deltas[0].stream_state,
        Some(ConversationMessageStreamState::Active)
    );
    assert!(stream_deltas[0].current_body.is_none());
    assert!(stream_deltas[0].initial_message.is_some());
    assert_eq!(
        stream_deltas[1].stream_state,
        Some(ConversationMessageStreamState::Sealed)
    );
    assert_eq!(stream_deltas[1].current_body.as_deref(), Some("正在分析"));
    assert!(record_ids.thinking_record_id.is_none());
}

/// Verifies a failed seal restores the exact request-local identity for terminal retry.
/// 验证封口失败会恢复精确请求局部身份，以供终止路径重试。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails when an error loses the retry identity.
/// 此测试无返回值；错误路径丢失重试身份时通过断言失败。
#[test]
fn failed_content_seal_restores_record_identity() {
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state.workspace_snapshot = None;
    let state = Arc::new(Mutex::new(service_state));
    let app = ServiceEventEmitter::new(Vec::new());
    let mut record_ids = SessionStreamingRecordIds {
        thinking_record_id: Some(String::from("thinking-record")),
        output_record_id: None,
    };

    let error = seal_stream_content_kind(
        &app,
        &state,
        session_id,
        &mut record_ids,
        StreamContentKind::Thinking,
    )
    .expect_err("missing workspace should reject sealing");

    assert!(error.contains("workspace snapshot is not ready"));
    assert_eq!(
        record_ids.thinking_record_id.as_deref(),
        Some("thinking-record")
    );
}

/// Verifies a stop committed between generation validation and projection rejects a late delta.
/// 验证在代际校验与投影之间提交的停止状态会拒绝迟到增量。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails when stopped sessions accept stream content.
/// 此测试无返回值；已停止会话接受流内容时通过断言失败。
#[test]
fn stopped_session_rejects_late_stream_delta() {
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    service_state
        .workspace_snapshot
        .as_mut()
        .expect("workspace snapshot")
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    let state = Arc::new(Mutex::new(service_state));
    let app = ServiceEventEmitter::new(Vec::new());
    let mut record_ids = SessionStreamingRecordIds::default();

    let error = append_stream_content_delta(
        &app,
        &state,
        session_id,
        &mut record_ids,
        StreamContentKind::Output,
        "late",
    )
    .expect_err("stopped session should reject late stream content");

    assert!(error.contains("stopped session"));
    assert!(record_ids.output_record_id.is_none());
}
