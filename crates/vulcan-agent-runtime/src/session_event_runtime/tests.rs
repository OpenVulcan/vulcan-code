use super::*;
use crate::conversation_protocol::SessionConversationState;
use serde_json::json;
use vulcan_core::types::ProviderUsageSnapshot;

/// Builds one deterministic test session identifier.
/// 构建一个确定性的测试会话标识。
fn test_session_id() -> SessionId {
    "11111111-1111-1111-1111-111111111111"
        .parse()
        .expect("test session id should be a valid UUID")
}

/// Verifies every Provider retry becomes an independent visible note outside future model context.
/// 验证每次 Provider 重试都会成为独立可见且不进入后续模型上下文的备注。
#[test]
fn provider_retry_projects_visible_display_note() {
    // runtimeState owns the durable transcript projection under test.
    // runtimeState 持有被测的持久转录投影。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    // sessionId provides the stable namespace for the new retry record.
    // sessionId 为新的重试记录提供稳定命名空间。
    let session_id = test_session_id();
    // retryEvent models one finite rate-limit wait emitted before sleeping.
    // retryEvent 模拟一条在等待前发出的有限限流重试事件。
    let retry_event = RuntimeEvent::new(
        RuntimeEventKind::Retrying,
        "Provider 自动重试",
        "Kimi 第 6/100000 次重试将在 32 秒后开始：Provider rate limit",
    )
    .with_metadata(Some(json!({
        "attempt": 6,
        "max_retries": 100_000,
        "unbounded": false,
        "delay_ms": 32_000,
        "delay_seconds": 32,
    })));

    assert!(
        append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3500",
            &retry_event,
        )
        .expect("retry projection should succeed")
    );
    // record is the exact display-only row created for this physical failure.
    // record 是为本次真实请求失败创建的精确纯显示行。
    let record = runtime_state
        .conversation_state
        .records
        .last()
        .expect("retry projection should append one record");
    assert_eq!(record.kind, SessionMessageKind::Assistant);
    assert_eq!(record.record_kind, SessionMessageRecordKind::DisplayNote);
    assert!(record.visible);
    assert_eq!(record.title, "Provider 自动重试");
    assert!(record.body.contains("第 6/100000 次重试将在 32 秒后开始"));
    assert_eq!(
        record
            .metadata
            .as_ref()
            .and_then(|value| value["notice_kind"].as_str()),
        Some("provider_retry")
    );
    assert_eq!(
        record
            .metadata
            .as_ref()
            .and_then(|value| value["provider_retry_state"].as_str()),
        Some("retrying")
    );
    assert_eq!(
        record
            .metadata
            .as_ref()
            .and_then(|value| value["unbounded"].as_bool()),
        Some(false)
    );
    assert!(!record.record_kind.contributes_to_context());

    let succeeded_event = RuntimeEvent::new(
        RuntimeEventKind::ProviderRetryStatusChanged,
        "Provider 重试成功",
        String::new(),
    )
    .with_metadata(Some(json!({
        "provider_retry_state": "succeeded",
    })));
    assert!(
        append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3501",
            &succeeded_event,
        )
        .expect("retry success should update the retained note")
    );
    assert_eq!(runtime_state.conversation_state.records.len(), 1);
    let updated_record = runtime_state
        .conversation_state
        .records
        .last()
        .expect("updated retry record should remain retained");
    assert_eq!(updated_record.title, "Provider 重试成功");
    assert_eq!(
        updated_record
            .metadata
            .as_ref()
            .and_then(|value| value["provider_retry_state"].as_str()),
        Some("succeeded")
    );
}

/// Verifies retry start supersedes only request cards that never crossed ToolStarted.
/// 验证重试开始只会取代尚未跨过 ToolStarted 的请求卡片。
#[test]
fn provider_retry_supersedes_ready_pre_execution_tool_request() {
    // runtimeState receives one request-stage card with both physical and canonical aliases.
    // runtimeState 接收一张同时具有真实请求与规范调用别名的请求阶段卡片。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let session_id = test_session_id();
    let request_started = RuntimeEvent::new(
        RuntimeEventKind::ToolRequestStarted,
        "工具请求 · pending-call · vulcan_file_read",
        String::new(),
    )
    .with_metadata(Some(json!({
        "request_stream_id": "request-stream-1",
        "tool_call_id": "pending-call",
        "tool_name": "vulcan_file_read",
        "tool_display_state": "receiving",
        "received_bytes": 12,
        "received_characters": 12,
        "request_revision": 1,
    })));
    assert!(
        append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3600",
            &request_started,
        )
        .expect("request start should create the exact pre-execution card")
    );
    let ready = RuntimeEvent::new(
        RuntimeEventKind::ToolRequestReady,
        "工具请求 · canonical-call · vulcan_file_read",
        String::new(),
    )
    .with_metadata(Some(json!({
        "request_stream_id": "request-stream-1",
        "tool_call_id": "canonical-call",
        "tool_name": "vulcan_file_read",
        "tool_display_state": "ready",
        "received_bytes": 99,
        "received_characters": 99,
        "request_revision": 2,
    })));
    assert!(
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3601", &ready,)
            .expect("ready request should promote the canonical alias")
    );

    // completed_started proves a separate tool that crossed ToolStarted remains terminal and unchanged.
    // completed_started 证明另一个已经跨过 ToolStarted 的工具会保持终态且不受影响。
    let completed_started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · completed-call · vulcan_file_list",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "completed-call",
        "tool_name": "vulcan_file_list",
    })));
    assert!(
        append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3601.1",
            &completed_started,
        )
        .expect("started tool should create an independent execution card")
    );
    // completed_finished closes the independent execution before the Provider retry boundary.
    // completed_finished 在 Provider 重试边界之前闭合独立执行记录。
    let completed_finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · completed-call",
        "## Tool Result\n\nlisted files",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "completed-call",
        "tool_name": "vulcan_file_list",
        "display_state": "completed",
    })));
    assert!(
        append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3601.2",
            &completed_finished,
        )
        .expect("finished tool should close its independent execution card")
    );

    let retry_event = RuntimeEvent::new(
        RuntimeEventKind::Retrying,
        "Provider 自动重试",
        "Provider 第 1/5 次重试将在 3 秒后开始",
    )
    .with_metadata(Some(json!({
        "attempt": 1,
        "max_retries": 5,
        "delay_ms": 3_000,
        "delay_seconds": 3,
    })));
    assert!(
        append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3602",
            &retry_event,
        )
        .expect("retry projection should supersede the ready request")
    );
    assert!(runtime_state.tool_request_record_ids.is_empty());
    assert!(runtime_state.tool_call_record_ids.is_empty());
    let request_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("request card should remain visible as a terminal historical row");
    assert_eq!(
        request_record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata["tool_display_state"].as_str()),
        Some("superseded")
    );
    assert_eq!(
        request_record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata["request_terminal_reason"].as_str()),
        Some("provider_retry_superseded")
    );
    assert!(
        runtime_state
            .conversation_state
            .records
            .iter()
            .any(|record| {
                record
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_call_id"))
                    .and_then(serde_json::Value::as_str)
                    == Some("completed-call")
                    && record
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("result"))
                        .and_then(|result| result.get("status"))
                        .and_then(serde_json::Value::as_str)
                        == Some("completed")
            })
    );
}

/// Verifies a slow Provider first-event status remains transient and never enters the durable transcript.
/// 验证 Provider 首事件缓慢状态保持瞬时性，绝不进入持久转录。
#[test]
fn provider_first_event_waiting_stays_out_of_durable_transcript() {
    // runtimeState owns the durable transcript projection under test.
    // runtimeState 持有被测的持久转录投影。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    // sessionId identifies the session whose transcript must remain unchanged.
    // sessionId 标识必须保持不变的会话转录。
    let session_id = test_session_id();
    // waitingEvent models one request that produced no stream event within the visible threshold.
    // waitingEvent 模拟请求在可见阈值内没有产生流事件的情况。
    let waiting_event = RuntimeEvent::new(
        RuntimeEventKind::ProviderWaiting,
        "等待 Provider 响应",
        "请求已发送，正在等待 Provider 返回首个响应事件。",
    )
    .with_metadata(Some(json!({
        "stage": "awaiting_first_provider_event",
        "elapsed_ms": 5_000,
    })));

    assert!(
        !append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3502",
            &waiting_event,
        )
        .expect("provider waiting projection should be classified successfully")
    );
    assert!(runtime_state.conversation_state.records.is_empty());
}

/// Builds one successful terminal report with deterministic provider usage.
/// 构建一份带确定性 Provider 用量的成功终态报告。
#[allow(clippy::too_many_arguments)]
fn usage_report(
    session_id: SessionId,
    assistant_output: &str,
    input_tokens: usize,
    output_tokens: usize,
    reasoning_tokens: usize,
    total_tokens: usize,
    cached_input_tokens: Option<usize>,
) -> ExecutionRunReport {
    let provider_usage = ProviderUsageSnapshot {
        input_tokens: Some(input_tokens),
        uncached_input_tokens: None,
        output_tokens: Some(output_tokens),
        reasoning_tokens: Some(reasoning_tokens),
        total_tokens: Some(total_tokens),
        cached_input_tokens,
        cache_creation_input_tokens: None,
        ..ProviderUsageSnapshot::default()
    };
    ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: Some(assistant_output.to_string()),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 1,
        last_successful_provider_usage: Some(provider_usage.clone()),
        cumulative_successful_provider_usage: Some(provider_usage),
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    }
}

/// Extracts the latest visible turn-statistics metadata from a runtime state.
/// 从运行态中提取最近一条可见轮次统计元数据。
fn latest_turn_stats(
    runtime_state: &SessionRuntimeStateEntry,
) -> vulcan_workbench_model::SessionTurnStats {
    let stats_record = runtime_state
        .conversation_state
        .records
        .iter()
        .rev()
        .find(|record| record.record_kind == SessionMessageRecordKind::TurnStats)
        .expect("turn stats record should exist");
    let stats_value = stats_record
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("turn_stats"))
        .expect("turn stats metadata should exist")
        .clone();
    serde_json::from_value(stats_value).expect("turn stats metadata should deserialize")
}

/// Verifies canonical checkpoints do not replace visible transcript projections.
/// 验证规范检查点不会替换可见转录投影。
#[test]
fn checkpoint_preserves_visible_records() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "1000", "介绍下你自己")
        .expect("valid user prompt identity");
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1001",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "我是 VulcanCode。",
        false,
    )
    .expect("valid streaming record identity");
    let records_before_checkpoint = visible_record_projection(&runtime_state);

    apply_checkpoint_conversation_state(
        &mut runtime_state,
        SessionConversationState::default(),
        "1002",
    )
    .expect("valid checkpoint conversation identities");

    assert_eq!(
        visible_record_projection(&runtime_state),
        records_before_checkpoint
    );
    assert_eq!(runtime_state.updated_at, "1002");
}

/// Verifies an external stop seals only explicitly active stream records.
/// 验证外部停止仅封口显式处于活跃状态的流记录。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails when active lifecycle sealing is incomplete or non-idempotent.
/// 此测试无返回值；活跃生命周期封口不完整或不幂等时通过断言失败。
#[test]
fn external_stop_seals_all_active_stream_records() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut thinking_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1050",
        &mut thinking_record_id,
        SessionMessageRecordKind::ThinkingOutput,
        "思考",
        "分析中",
        true,
    )
    .expect("valid thinking stream identity");
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1051",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "回答中",
        false,
    )
    .expect("valid output stream identity");
    append_user_prompt_record(&mut runtime_state, session_id, "1052", "保留稳定记录")
        .expect("valid stable user record identity");

    assert_eq!(
        seal_active_streaming_records_in_state(&mut runtime_state, "1053"),
        2
    );
    assert_eq!(runtime_state.updated_at, "1053");
    let stream_states = runtime_state
        .conversation_state
        .records
        .iter()
        .filter_map(|record| {
            record
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(STREAMING_RECORD_STATE_METADATA_KEY))
                .and_then(serde_json::Value::as_str)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        stream_states,
        vec![STREAMING_RECORD_STATE_SEALED, STREAMING_RECORD_STATE_SEALED]
    );
    assert_eq!(
        seal_active_streaming_records_in_state(&mut runtime_state, "1054"),
        0
    );
    assert_eq!(runtime_state.updated_at, "1053");
}

/// Verifies interrupted provider drafts are removed without touching stable transcript rows.
/// 验证中断的 Provider 草稿会被移除，且不会触碰稳定转录行。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails when draft cleanup removes or retains the wrong identity.
/// 此测试无返回值；草稿清理移除或保留错误身份时通过断言失败。
#[test]
fn interrupted_provider_drafts_are_discarded_before_retry() {
    // sessionId owns the streamed and stable records used by this lifecycle test.
    // sessionId 持有当前生命周期测试使用的流式记录与稳定记录。
    let session_id = test_session_id();
    // runtimeState starts with one active thinking draft, one active output draft, and one stable user row.
    // runtimeState 初始包含一条活跃思考草稿、一条活跃输出草稿与一条稳定用户行。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut thinking_record_id = None;
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1050",
        &mut thinking_record_id,
        SessionMessageRecordKind::ThinkingOutput,
        "思考",
        "分析中",
        true,
    )
    .expect("valid thinking stream identity");
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1051",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "未完成回答",
        false,
    )
    .expect("valid output stream identity");
    append_user_prompt_record(&mut runtime_state, session_id, "1052", "保留稳定记录")
        .expect("valid stable user record identity");

    // discardedIds prove the exact active identities were removed in deterministic order.
    // discardedIds 证明精确活跃身份已按确定顺序移除。
    let discarded_ids = discard_active_streaming_records_in_state(&mut runtime_state, "1053");
    assert_eq!(discarded_ids.len(), 2);
    assert!(
        thinking_record_id
            .as_ref()
            .is_some_and(|record_id| discarded_ids.contains(record_id))
    );
    assert!(
        output_record_id
            .as_ref()
            .is_some_and(|record_id| discarded_ids.contains(record_id))
    );
    assert_eq!(runtime_state.conversation_state.records.len(), 1);
    assert_eq!(
        runtime_state.conversation_state.records[0].kind,
        SessionMessageKind::User
    );
    assert_eq!(runtime_state.updated_at, "1053");
    assert!(discarded_ids.iter().all(|record_id| {
        !runtime_state
            .ui_state
            .collapsed_message_ids
            .contains(record_id)
    }));
}

/// Verifies a stale request-local identity cannot append after authoritative sealing.
/// 验证过期的请求局部身份无法在权威封口后继续追加。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails when terminal records accept late text.
/// 此测试无返回值；终态记录接受迟到文本时通过断言失败。
#[test]
fn sealed_stream_record_rejects_late_append() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let mut record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1060",
        &mut record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "完整正文",
        false,
    )
    .expect("valid active stream identity");
    let stale_record_id = record_id.clone().expect("stream record identity");
    seal_streaming_record_in_state(&mut runtime_state, "1061", &stale_record_id)
        .expect("valid stream lifecycle metadata")
        .expect("stream record should exist");
    assert!(
        seal_streaming_record_in_state(&mut runtime_state, "1061", &stale_record_id)
            .expect("repeat sealing should remain valid")
            .is_none()
    );

    let error = append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1062",
        &mut record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "不应追加",
        false,
    )
    .expect_err("sealed stream identity must reject late append");

    assert!(error.contains("non-active streaming record"));
    assert_eq!(
        runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.id == stale_record_id)
            .map(|record| record.body.as_str()),
        Some("完整正文")
    );
}

/// Verifies checkpoint-created guided user records are merged into visible transcript projections.
/// 验证检查点创建的引导用户记录会合并进可见转录投影。
#[test]
fn checkpoint_merges_guided_user_record() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "1100", "先调用工具")
        .expect("valid user prompt identity");
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1101",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "工具已经完成。",
        false,
    )
    .expect("valid streaming record identity");
    let mut later_output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "1104",
        &mut later_output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "后续请求已经开始输出。",
        false,
    )
    .expect("valid streaming record identity");
    let mut checkpoint_state = runtime_state.conversation_state.clone();
    checkpoint_state.records.push(SessionConversationRecord {
        id: String::from("core-guided-user-record"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("用户输入"),
        body: String::from("不需要配图了"),
        metadata: Some(json!({
            "canonical_text": "不需要配图了",
            "canonical_user_text": "不需要配图了",
            "source": "user_interrupt",
            "starts_user_turn": false,
            "turn_process": true,
            "collapsed_by_default": true,
            "editable": false,
        })),
        created_at: String::from("1102"),
    });

    apply_checkpoint_conversation_state(&mut runtime_state, checkpoint_state, "1103")
        .expect("valid checkpoint conversation identities");

    let visible_bodies = runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| record.body.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        visible_bodies,
        vec![
            "先调用工具",
            "工具已经完成。",
            "不需要配图了",
            "后续请求已经开始输出。"
        ]
    );
    let guided_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.id == "core-guided-user-record")
        .expect("guided user record should be merged");
    let guided_metadata = guided_record
        .metadata
        .as_ref()
        .expect("guided user record metadata should be preserved");
    assert_eq!(
        guided_metadata
            .get("source")
            .and_then(serde_json::Value::as_str),
        Some("user_interrupt")
    );
    assert_eq!(
        guided_metadata
            .get("starts_user_turn")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        guided_metadata
            .get("turn_process")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        guided_metadata
            .get("editable")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

/// Verifies internal mailbox checkpoint prompts never become visible user transcript rows.
/// 验证内部 mailbox 检查点提示词不会进入可见用户转录行。
#[test]
fn checkpoint_skips_session_mailbox_user_record() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "1200", "启动两个子代理")
        .expect("valid user prompt identity");
    let mut checkpoint_state = runtime_state.conversation_state.clone();
    checkpoint_state.records.push(SessionConversationRecord {
        id: String::from("core-mailbox-user-record"),
        kind: SessionMessageKind::User,
        record_kind: SessionMessageRecordKind::Transcript,
        visible: true,
        title: String::from("用户输入"),
        body: String::from(
            "<subagent_mailbox_event>\n{\"kind\":\"completed\"}\n</subagent_mailbox_event>",
        ),
        metadata: None,
        created_at: String::from("1201"),
    });

    apply_checkpoint_conversation_state(&mut runtime_state, checkpoint_state, "1202")
        .expect("valid checkpoint conversation identities");

    let visible_bodies = runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| record.body.as_str())
        .collect::<Vec<_>>();
    assert_eq!(visible_bodies, vec!["启动两个子代理"]);
}

/// Verifies terminal reports keep streamed records when they also carry a canonical checkpoint.
/// 验证终态报告携带规范检查点时仍会保留已流式写入的记录。
#[test]
fn terminal_report_preserves_streamed_records() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2000", "随便调用两个工具")
        .expect("valid user prompt identity");
    let mut thinking_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "2001",
        &mut thinking_record_id,
        SessionMessageRecordKind::ThinkingOutput,
        "思考",
        "我需要先判断可用工具。",
        true,
    )
    .expect("valid streaming record identity");
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "2002",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "已完成，两个工具均正常。",
        false,
    )
    .expect("valid streaming record identity");

    let report = ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: Some(String::from("报告中的完整输出不应覆盖已流式记录。")),
        thinking_output: Some(String::from("报告中的思考不应重复追加。")),
        conversation_state: Some(SessionConversationState::default()),
        provider_request_count: 1,
        last_successful_provider_usage: None,
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2003", &report)
        .expect("valid terminal report identities");

    let visible_bodies: Vec<&str> = runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| record.body.as_str())
        .collect();
    assert!(visible_bodies.contains(&"随便调用两个工具"));
    assert!(visible_bodies.contains(&"我需要先判断可用工具。"));
    assert!(visible_bodies.contains(&"已完成，两个工具均正常。"));
    assert!(!visible_bodies.contains(&"报告中的完整输出不应覆盖已流式记录。"));
    let canonical_texts = runtime_state
        .conversation_state
        .messages
        .iter()
        .filter_map(|message| match &message.payload {
            SessionConversationPayload::UserText { text, .. } => Some(text.as_str()),
            SessionConversationPayload::Assistant { text, .. } => Some(text.as_str()),
            SessionConversationPayload::ToolResult { .. }
            | SessionConversationPayload::AgentMessage { .. } => None,
        })
        .collect::<Vec<_>>();
    assert!(canonical_texts.contains(&"随便调用两个工具"));
    assert!(canonical_texts.contains(&"报告中的完整输出不应覆盖已流式记录。"));
}

/// Verifies terminal reports keep new assistant messages free of duplicated Provider usage.
/// 验证终态报告不会向新助手消息重复写入 Provider 用量。
#[test]
fn terminal_report_keeps_appended_assistant_usage_in_the_ledger_only() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2100", "只输出 abc")
        .expect("valid user prompt identity");
    let report = usage_report(session_id, "abc", 31, 2, 1, 34, Some(7));

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2101", &report)
        .expect("valid terminal report identities");

    let assistant_usage = runtime_state
        .conversation_state
        .messages
        .iter()
        .find_map(|message| match &message.payload {
            SessionConversationPayload::Assistant { text, usage, .. } if text == "abc" => {
                usage.as_ref()
            }
            SessionConversationPayload::Assistant { .. }
            | SessionConversationPayload::UserText { .. }
            | SessionConversationPayload::ToolResult { .. }
            | SessionConversationPayload::AgentMessage { .. } => None,
        });
    assert!(assistant_usage.is_none());

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.input_tokens, 31);
    assert_eq!(turn_stats.output_tokens, 2);
    assert_eq!(turn_stats.reasoning_tokens, 1);
    assert_eq!(turn_stats.total_tokens, 34);
    assert_eq!(turn_stats.cached_input_tokens, Some(7));
}

/// Verifies child-agent usage is included once in totals and remains display-separated.
/// 验证子代理用量只汇入总计一次并保持独立展示分组。
#[test]
fn terminal_report_includes_embedded_subagent_usage_once_in_parent_totals() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2150", "创建智能体")
        .expect("valid user prompt identity");
    let mut report = usage_report(session_id, "智能体完成。", 31, 2, 1, 34, Some(7));
    report
        .embedded_turn_usage_segments
        .push(SessionTurnUsageSegment {
            kind: SessionTurnUsageSegmentKind::SubagentTask,
            trigger: Some(String::from("worker-a")),
            execution_profile: None,
            provider_id: Some(String::from("deepseek")),
            provider_name_snapshot: Some(String::from("DeepSeek")),
            model_id: Some(String::from("deepseek-v4-pro")),
            model_name_snapshot: Some(String::from("DeepSeek V4 Pro")),
            provider_call_count: 1,
            tool_call_count: 0,
            input_tokens: 100,
            uncached_input_tokens: Some(60),
            output_tokens: 25,
            reasoning_tokens: 5,
            total_tokens: 130,
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
        });

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2151", &report)
        .expect("valid terminal report identities");

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 2);
    assert_eq!(turn_stats.input_tokens, 131);
    assert_eq!(turn_stats.output_tokens, 27);
    assert_eq!(turn_stats.reasoning_tokens, 6);
    assert_eq!(turn_stats.total_tokens, 164);
    assert_eq!(turn_stats.cached_input_tokens, Some(47));
    assert_eq!(turn_stats.usage_segments.len(), 2);
    let subagent_group = turn_stats
        .usage_groups
        .iter()
        .find(|group| group.kind == vulcan_workbench_model::SessionTurnUsageGroupKind::Subagent)
        .expect("subagent usage group should be retained");
    assert_eq!(subagent_group.provider_call_count, 1);
    assert_eq!(subagent_group.input_tokens, 100);
    assert_eq!(subagent_group.output_tokens, 25);
    assert_eq!(subagent_group.reasoning_tokens, 5);
    assert_eq!(subagent_group.total_tokens, 130);
    assert_eq!(subagent_group.cached_input_tokens, Some(40));
    assert_eq!(subagent_group.uncached_input_tokens, Some(60));
}

/// Verifies terminal reports leave existing assistant usage empty while materializing Turn Stats.
/// 验证终态报告在物化 Turn Stats 时仍保持已有助手消息用量为空。
#[test]
fn terminal_report_keeps_existing_assistant_usage_in_the_ledger_only() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2200", "只输出 xyz")
        .expect("valid user prompt identity");
    runtime_state
        .conversation_state
        .messages
        .push(SessionConversationMessage {
            id: String::from("existing-assistant-message"),
            payload: SessionConversationPayload::Assistant {
                text: String::from("xyz"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        });
    let report = usage_report(session_id, "xyz", 13, 5, 0, 18, None);

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2201", &report)
        .expect("valid terminal report identities");

    let assistant_messages = runtime_state
        .conversation_state
        .messages
        .iter()
        .filter_map(|message| match &message.payload {
            SessionConversationPayload::Assistant { text, usage, .. } if text == "xyz" => {
                Some(usage)
            }
            SessionConversationPayload::Assistant { .. }
            | SessionConversationPayload::UserText { .. }
            | SessionConversationPayload::ToolResult { .. }
            | SessionConversationPayload::AgentMessage { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(assistant_messages.len(), 1);
    assert!(assistant_messages[0].is_none());

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.input_tokens, 13);
    assert_eq!(turn_stats.output_tokens, 5);
    assert_eq!(turn_stats.total_tokens, 18);
}

/// Verifies one multi-call run does not append a cumulative assistant duplicate or count its usage twice.
/// 验证一次多调用执行不会追加累计助手副本，也不会重复统计其用量。
#[test]
fn terminal_report_does_not_duplicate_multi_message_assistant_output_usage() {
    // sessionId provides one stable identity for the complete multi-call run.
    // sessionId 为完整多调用执行提供一个稳定身份。
    let session_id = test_session_id();
    // runtimeState owns the canonical user turn and both real Provider assistant messages.
    // runtimeState 持有规范用户轮次和两条真实 Provider 助手消息。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2250", "连续调用工具后总结")
        .expect("valid user prompt identity");
    // firstUsage is the authoritative usage reported by the first Provider response.
    // firstUsage 是第一次 Provider 响应报告的权威用量。
    let first_usage = ProviderUsageSnapshot {
        input_tokens: Some(100),
        uncached_input_tokens: None,
        output_tokens: Some(10),
        reasoning_tokens: Some(3),
        total_tokens: Some(110),
        cached_input_tokens: Some(80),
        cache_creation_input_tokens: None,
        ..ProviderUsageSnapshot::default()
    };
    // finalUsage is the authoritative usage reported by the final Provider response.
    // finalUsage 是最后一次 Provider 响应报告的权威用量。
    let final_usage = ProviderUsageSnapshot {
        input_tokens: Some(120),
        uncached_input_tokens: None,
        output_tokens: Some(12),
        reasoning_tokens: Some(4),
        total_tokens: Some(132),
        cached_input_tokens: Some(100),
        cache_creation_input_tokens: None,
        ..ProviderUsageSnapshot::default()
    };
    runtime_state.conversation_state.messages.extend([
        SessionConversationMessage {
            id: String::from("multi-call-first-assistant"),
            payload: SessionConversationPayload::Assistant {
                text: String::from("先执行工具。"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: Some(Box::new(first_usage)),
                tool_calls: Vec::new(),
            },
        },
        SessionConversationMessage {
            id: String::from("multi-call-final-assistant"),
            payload: SessionConversationPayload::Assistant {
                text: String::from("工具执行完成。"),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: Some(Box::new(final_usage.clone())),
                tool_calls: Vec::new(),
            },
        },
    ]);
    // cumulativeUsage is the exact sum of both physical Provider responses.
    // cumulativeUsage 是两次真实 Provider 响应的准确合计。
    let cumulative_usage = ProviderUsageSnapshot {
        input_tokens: Some(220),
        uncached_input_tokens: None,
        output_tokens: Some(22),
        reasoning_tokens: Some(7),
        total_tokens: Some(242),
        cached_input_tokens: Some(180),
        cache_creation_input_tokens: None,
        ..ProviderUsageSnapshot::default()
    };
    // report carries the cumulative visible output while retaining physical per-message usage.
    // report 携带累计可见输出，同时保留每条真实消息的逐次用量。
    let report = ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: Some(String::from("先执行工具。工具执行完成。")),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 2,
        last_successful_provider_usage: Some(final_usage),
        cumulative_successful_provider_usage: Some(cumulative_usage),
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2251", &report)
        .expect("valid terminal report identities");

    // assistantTexts exposes every canonical assistant text retained for future Provider context.
    // assistantTexts 暴露为后续 Provider 上下文保留的每条规范助手文本。
    let assistant_texts = runtime_state
        .conversation_state
        .messages
        .iter()
        .filter_map(|message| match &message.payload {
            SessionConversationPayload::Assistant { text, .. } => Some(text.as_str()),
            SessionConversationPayload::UserText { .. }
            | SessionConversationPayload::ToolResult { .. }
            | SessionConversationPayload::AgentMessage { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(assistant_texts, vec!["先执行工具。", "工具执行完成。"]);
    // turnStats must equal the two physical Provider responses without a synthetic third usage.
    // turnStats 必须等于两次真实 Provider 响应，不得包含合成的第三份用量。
    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 2);
    assert_eq!(turn_stats.input_tokens, 220);
    assert_eq!(turn_stats.output_tokens, 22);
    assert_eq!(turn_stats.reasoning_tokens, 7);
    assert_eq!(turn_stats.total_tokens, 242);
    assert_eq!(turn_stats.cached_input_tokens, Some(180));
}

/// Verifies terminal reports still project provider usage when canonical messages lost per-message usage.
/// 验证规范消息丢失逐消息用量时，终态报告仍会投影 Provider 用量。
#[test]
fn terminal_report_backfills_usage_when_checkpoint_messages_lack_usage() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2300", "只输出 123")
        .expect("valid user prompt identity");
    let report = ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: None,
        thinking_output: None,
        conversation_state: Some(SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: String::from("checkpoint-assistant-without-usage"),
                payload: SessionConversationPayload::Assistant {
                    text: String::from("123"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }],
            records: Vec::new(),
            continuation: Default::default(),
        }),
        provider_request_count: 1,
        last_successful_provider_usage: Some(ProviderUsageSnapshot {
            input_tokens: Some(101),
            uncached_input_tokens: None,
            output_tokens: Some(3),
            reasoning_tokens: Some(2),
            total_tokens: Some(106),
            cached_input_tokens: Some(40),
            cache_creation_input_tokens: None,
            ..ProviderUsageSnapshot::default()
        }),
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2301", &report)
        .expect("valid terminal report identities");

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 1);
    assert_eq!(turn_stats.input_tokens, 101);
    assert_eq!(turn_stats.output_tokens, 3);
    assert_eq!(turn_stats.reasoning_tokens, 2);
    assert_eq!(turn_stats.total_tokens, 106);
    assert_eq!(turn_stats.cached_input_tokens, Some(40));
    assert_eq!(turn_stats.usage_segments.len(), 1);
    assert_eq!(
        turn_stats.usage_segments[0].kind,
        SessionTurnUsageSegmentKind::AssistantResponse
    );
    assert_eq!(turn_stats.usage_segments[0].provider_call_count, 1);
}

/// Verifies multi-call terminal reports use cumulative usage when checkpoint messages lost per-call usage.
/// 验证多次调用终态报告在检查点消息丢失逐调用用量时会使用累计用量。
#[test]
fn terminal_report_backfills_multi_call_usage_from_cumulative_snapshot() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "2400", "连续调用工具后总结")
        .expect("valid user prompt identity");
    let report = ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: None,
        thinking_output: None,
        conversation_state: Some(SessionConversationState {
            messages: vec![SessionConversationMessage {
                id: String::from("checkpoint-final-assistant-without-usage"),
                payload: SessionConversationPayload::Assistant {
                    text: String::from("完成"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }],
            records: Vec::new(),
            continuation: Default::default(),
        }),
        provider_request_count: 7,
        last_successful_provider_usage: Some(ProviderUsageSnapshot {
            input_tokens: Some(26_442),
            uncached_input_tokens: None,
            output_tokens: Some(171),
            reasoning_tokens: Some(20),
            total_tokens: Some(26_633),
            cached_input_tokens: Some(25_856),
            cache_creation_input_tokens: None,
            ..ProviderUsageSnapshot::default()
        }),
        cumulative_successful_provider_usage: Some(ProviderUsageSnapshot {
            input_tokens: Some(185_094),
            uncached_input_tokens: None,
            output_tokens: Some(1_197),
            reasoning_tokens: Some(140),
            total_tokens: Some(186_431),
            cached_input_tokens: Some(180_992),
            cache_creation_input_tokens: None,
            ..ProviderUsageSnapshot::default()
        }),
        embedded_turn_usage_segments: Vec::new(),
        mcp_discovery_rows: Vec::new(),
        mcp_discovery_errors: Vec::new(),
        warnings: Vec::new(),
        error: None,
        controller_events: Vec::new(),
    };

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "2401", &report)
        .expect("valid terminal report identities");

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.provider_call_count, 7);
    assert_eq!(turn_stats.input_tokens, 185_094);
    assert_eq!(turn_stats.output_tokens, 1_197);
    assert_eq!(turn_stats.reasoning_tokens, 140);
    assert_eq!(turn_stats.total_tokens, 186_431);
    assert_eq!(turn_stats.cached_input_tokens, Some(180_992));
    assert_eq!(turn_stats.usage_groups.len(), 1);
    assert_eq!(turn_stats.usage_groups[0].provider_call_count, 7);
    assert_eq!(turn_stats.usage_groups[0].input_tokens, 185_094);
}

/// Verifies a tool result projection preserves runtime event order instead of mutating the stored transcript order.
/// 验证工具返回投影保留运行时事件顺序，而不是改写已存储转录顺序。
#[test]
fn tool_result_projection_preserves_runtime_event_order() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "3000", "调用工具")
        .expect("valid user prompt identity");
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-1 · vulcan_file_list",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-1",
        "tool_name": "vulcan_file_list",
    })));
    assert!(
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3001", &started,)
            .expect("valid runtime event record identities")
    );
    let mut output_record_id = None;
    append_stream_delta_record_to_state(
        &mut runtime_state,
        session_id,
        "3002",
        &mut output_record_id,
        SessionMessageRecordKind::AssistantOutput,
        "输出",
        "工具调用后继续输出。",
        false,
    )
    .expect("valid streaming record identity");
    // finished carries only the authoritative call identity and structured completion state.
    // finished 只携带权威调用身份与结构化完成状态。
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-1",
        "## Tool Result\n\nlisted files",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-1",
        "tool_name": "vulcan_file_list",
        "display_state": "completed",
    })));
    let (projected_runtime_state, changed_record_ids) =
        append_runtime_event_projection_to_owned_state(
            runtime_state,
            session_id,
            "3003",
            &finished,
        )
        .map_err(|(_, error)| error)
        .expect("valid runtime event record identities");
    runtime_state = projected_runtime_state;
    assert_eq!(changed_record_ids.len(), 2);
    assert_eq!(
        runtime_state
            .conversation_state
            .records
            .iter()
            .filter(|record| changed_record_ids.contains(&record.id))
            .map(|record| record.record_kind)
            .collect::<Vec<_>>(),
        vec![
            SessionMessageRecordKind::ToolCallOutput,
            SessionMessageRecordKind::ToolDetailOutput,
        ]
    );

    let visible_kinds = runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| record.record_kind)
        .collect::<Vec<_>>();
    assert_eq!(
        visible_kinds,
        vec![
            SessionMessageRecordKind::Transcript,
            SessionMessageRecordKind::ToolCallOutput,
            SessionMessageRecordKind::AssistantOutput,
        ]
    );
    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("tool call record should remain visible");
    assert!(tool_record.title.is_empty());
    assert!(tool_record.body.is_empty());
    let metadata = tool_record.metadata.as_ref().expect("tool metadata");
    assert_eq!(
        metadata
            .get("result")
            .and_then(|result| result.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("completed")
    );
    assert!(
        !metadata
            .get("result")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|result| result.contains_key("tool_result_id"))
    );
    assert_eq!(
        metadata
            .get("result")
            .and_then(|result| result.get("detail_ref"))
            .and_then(|detail_ref| detail_ref.get("message_index"))
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );
    assert!(
        metadata
            .get("result")
            .and_then(|result| result.get("preview"))
            .is_none()
    );
    let detail_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        .expect("tool detail record should be hidden");
    assert!(!detail_record.visible);
    assert_eq!(detail_record.body, "listed files");
    let detail_metadata = detail_record.metadata.as_ref().expect("detail metadata");
    assert!(detail_metadata.get("model_output").is_none());
    assert!(
        detail_metadata
            .get("display")
            .and_then(|display| display.get("kind"))
            .and_then(serde_json::Value::as_str)
            .is_some()
    );
}

/// Verifies an empty host result still creates one immutable terminal detail boundary.
/// 验证空宿主结果仍会创建一条不可变的终态详情边界。
#[test]
fn empty_tool_result_creates_terminal_detail_boundary() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-empty · file",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-empty",
        "tool_name": "file",
    })));
    append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3010", &started)
        .expect("tool start should project");
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-empty",
        String::new(),
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-empty",
        "tool_name": "file",
        "display_state": "completed",
    })));

    let (runtime_state, changed_record_ids) = append_runtime_event_projection_to_owned_state(
        runtime_state,
        session_id,
        "3011",
        &finished,
    )
    .map_err(|(_, error)| error)
    .expect("empty tool result should project");

    assert_eq!(changed_record_ids.len(), 2);
    let detail_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        .expect("empty result should retain a terminal detail record");
    assert!(detail_record.body.is_empty());
    assert_eq!(
        detail_record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("display"))
            .and_then(|display| display.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("completed")
    );
}

/// Verifies one request card survives request progress, canonical promotion, execution, and completion.
/// 验证同一张请求卡片会贯穿请求进度、规范身份提升、执行与完成阶段。
#[test]
fn tool_request_projection_preserves_one_card_through_execution() {
    // sessionId owns every lifecycle event in this deterministic projection.
    // sessionId 持有该确定性投影中的全部生命周期事件。
    let session_id = test_session_id();
    // runtimeState starts without any pre-existing transcript or correlation aliases.
    // runtimeState 初始不含任何既有转录或关联别名。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let request_started = RuntimeEvent::new(
        RuntimeEventKind::ToolRequestStarted,
        "工具请求 · pending-stream-1 · submit_plan",
        String::new(),
    )
    .with_metadata(Some(json!({
        "request_stream_id": "stream-1",
        "tool_call_id": "pending-stream-1",
        "tool_name": "submit_plan",
        "tool_display_state": "receiving",
        "received_bytes": 9,
        "received_characters": 9,
        "request_revision": 1,
    })));
    assert!(
        append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3050",
            &request_started,
        )
        .expect("request start projection should succeed")
    );
    let request_record_id = runtime_state
        .tool_request_record_ids
        .get("stream-1")
        .cloned()
        .expect("request stream should own one visible card");

    let progress = RuntimeEvent::new(
        RuntimeEventKind::ToolRequestProgress,
        "工具请求 · pending-stream-1 · submit_plan",
        String::new(),
    )
    .with_metadata(Some(json!({
        "request_stream_id": "stream-1",
        "tool_call_id": "pending-stream-1",
        "tool_name": "submit_plan",
        "tool_display_state": "receiving",
        "received_bytes": 64,
        "received_characters": 52,
        "request_revision": 2,
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3051",
        &progress,
    )
    .expect("new progress revision should succeed"));
    // staleProgress proves an older provider revision cannot roll the visible card backward.
    // staleProgress 证明较旧 Provider 修订无法让可见卡片倒退。
    let mut stale_progress = progress.clone();
    stale_progress.metadata.as_mut().expect("metadata")["request_revision"] = json!(1);
    assert!(
        !append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3052",
            &stale_progress,
        )
        .expect("stale progress should be ignored")
    );

    let ready = RuntimeEvent::new(
        RuntimeEventKind::ToolRequestReady,
        "工具请求 · call-canonical-1 · submit_plan",
        String::new(),
    )
    .with_metadata(Some(json!({
        "request_stream_id": "stream-1",
        "tool_call_id": "call-canonical-1",
        "tool_name": "submit_plan",
        "tool_display_state": "ready",
        "received_bytes": 96,
        "received_characters": 80,
        "request_revision": 3,
    })));
    assert!(
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3053", &ready,)
            .expect("ready projection should promote canonical identity")
    );
    assert_eq!(
        runtime_state.tool_request_record_ids.get("stream-1"),
        Some(&request_record_id)
    );
    assert_eq!(
        runtime_state.tool_call_record_ids.get("call-canonical-1"),
        Some(&request_record_id)
    );
    assert!(
        !runtime_state
            .tool_call_record_ids
            .contains_key("pending-stream-1")
    );
    let final_ready_progress = RuntimeEvent::new(
        RuntimeEventKind::ToolRequestProgress,
        "工具请求 · call-canonical-1 · submit_plan",
        String::new(),
    )
    .with_metadata(Some(json!({
        "request_stream_id": "stream-1",
        "tool_call_id": "call-canonical-1",
        "tool_name": "submit_plan",
        "tool_display_state": "ready",
        "received_bytes": 96,
        "received_characters": 80,
        "request_revision": 4,
    })));
    assert!(
        append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3054",
            &final_ready_progress,
        )
        .expect("final ready-state byte/character progress should update the same card")
    );
    // staleStart proves an older physical retry cannot replace the live stream correlation.
    // staleStart 证明较旧物理重试不能替换当前有效流关联。
    let stale_start = RuntimeEvent::new(
        RuntimeEventKind::ToolRequestStarted,
        "工具请求 · call-canonical-1 · submit_plan",
        String::new(),
    )
    .with_metadata(Some(json!({
        "request_stream_id": "stream-stale",
        "tool_call_id": "call-canonical-1",
        "tool_name": "submit_plan",
        "tool_display_state": "receiving",
        "request_revision": 2,
    })));
    assert!(
        !append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3054-stale-start",
            &stale_start,
        )
        .expect("stale request start should be ignored")
    );
    assert_eq!(
        runtime_state.tool_request_record_ids.get("stream-1"),
        Some(&request_record_id)
    );
    assert!(
        !runtime_state
            .tool_request_record_ids
            .contains_key("stream-stale")
    );
    // staleInvalid proves an older terminal event cannot release correlations or revive as a second card.
    // staleInvalid 证明较旧终态事件不能释放关联，也不能随后复活为第二张卡片。
    let stale_invalid = RuntimeEvent::new(
        RuntimeEventKind::ToolRequestInvalid,
        "工具请求 · call-canonical-1 · submit_plan",
        String::new(),
    )
    .with_metadata(Some(json!({
        "request_stream_id": "stream-1",
        "tool_call_id": "call-canonical-1",
        "tool_name": "submit_plan",
        "tool_display_state": "failed",
        "request_revision": 3,
    })));
    assert!(
        !append_runtime_event_projection_to_state(
            &mut runtime_state,
            session_id,
            "3054-stale",
            &stale_invalid,
        )
        .expect("stale invalid request state should be ignored")
    );
    assert_eq!(
        runtime_state.tool_request_record_ids.get("stream-1"),
        Some(&request_record_id)
    );
    assert_eq!(
        runtime_state.tool_call_record_ids.get("call-canonical-1"),
        Some(&request_record_id)
    );

    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-canonical-1 · submit_plan",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-canonical-1",
        "tool_name": "submit_plan",
    })));
    assert!(
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3055", &started,)
            .expect("execution start should upgrade the request card")
    );
    assert!(
        !runtime_state
            .tool_request_record_ids
            .contains_key("stream-1")
    );
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-canonical-1",
        "## Tool Result\n\nplan saved",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-canonical-1",
        "tool_name": "submit_plan",
        "display_state": "completed",
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3056",
        &finished,
    )
    .expect("tool completion should attach to the same card"));
    let tool_cards = runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .collect::<Vec<_>>();
    assert_eq!(tool_cards.len(), 1);
    assert_eq!(tool_cards[0].id, request_record_id);
    assert_eq!(
        tool_cards[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("tool_display_state"))
            .and_then(serde_json::Value::as_str),
        Some("completed")
    );
    assert_eq!(
        tool_cards[0]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("result"))
            .and_then(|result| result.get("status"))
            .and_then(serde_json::Value::as_str),
        Some("completed")
    );
}

/// Verifies hydration closes both receiving and ready cards without touching an executing card.
/// 验证水合会闭合接收中与就绪卡片，同时不会触碰已经执行的卡片。
#[test]
fn orphaned_tool_request_normalization_closes_only_pre_execution_cards() {
    // runtimeState models a persisted crash snapshot with two orphaned requests and one real execution.
    // runtimeState 模拟包含两个孤儿请求与一个真实执行的崩溃持久化快照。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    for (record_id, call_id, display_state) in [
        ("record-receiving", "pending-1", "receiving"),
        ("record-ready", "call-ready", "ready"),
        ("record-running", "call-running", "running"),
    ] {
        runtime_state
            .conversation_state
            .records
            .push(SessionConversationRecord {
                id: record_id.to_string(),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::new(),
                body: String::new(),
                metadata: Some(json!({
                    "tool_call_id": call_id,
                    "tool_name": "submit_plan",
                    "tool_display_state": display_state,
                })),
                created_at: String::from("3056"),
            });
        runtime_state
            .tool_call_record_ids
            .insert(call_id.to_string(), record_id.to_string());
    }
    runtime_state
        .tool_request_record_ids
        .insert(String::from("stream-1"), String::from("record-receiving"));

    assert_eq!(
        normalize_orphaned_tool_request_records(&mut runtime_state, "3057"),
        2
    );
    assert!(runtime_state.tool_request_record_ids.is_empty());
    assert!(!runtime_state.tool_call_record_ids.contains_key("pending-1"));
    assert!(
        !runtime_state
            .tool_call_record_ids
            .contains_key("call-ready")
    );
    assert_eq!(
        runtime_state.tool_call_record_ids.get("call-running"),
        Some(&String::from("record-running"))
    );
    let display_state = |record_id: &str| {
        runtime_state
            .conversation_state
            .records
            .iter()
            .find(|record| record.id == record_id)
            .and_then(|record| record.metadata.as_ref())
            .and_then(|metadata| metadata.get("tool_display_state"))
            .and_then(serde_json::Value::as_str)
    };
    assert_eq!(display_state("record-receiving"), Some("stopped"));
    assert_eq!(display_state("record-ready"), Some("stopped"));
    assert_eq!(display_state("record-running"), Some("running"));
    assert_eq!(runtime_state.updated_at, "3057");
}

/// Verifies display summaries collapse whitespace and truncate without changing visible semantics.
/// 验证展示摘要会折叠空白并在不改变可见语义的前提下截断。
#[test]
fn compact_display_summary_collapses_whitespace_and_truncates() {
    assert_eq!(
        compact_display_summary("  alpha\n\tbeta   gamma  "),
        "alpha beta gamma"
    );
    // Long source whose compacted form exceeds the display summary limit.
    // 压缩后超过展示摘要长度上限的长文本来源。
    let long_source = format!("{} tail", "a".repeat(241));
    // Expected text mirrors the existing first-240-characters truncation rule.
    // 期望文本保持既有的前 240 个字符截断规则。
    let expected = format!("{}...", "a".repeat(240));

    assert_eq!(compact_display_summary(&long_source), expected);
}

/// Verifies WebUI visible tool cards keep raw change-set host metadata for expandable file-change rendering.
/// 验证 WebUI 可见工具卡片会保留原始 change-set 宿主元数据，用于文件变更展开渲染。
#[test]
fn tool_result_projection_preserves_raw_change_set_host_result() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-change · file",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-change",
        "tool_name": "file",
        "arguments": {
            "action": "write",
            "path": "src/demo.lua",
            "content": "return 2\nreturn 3\n"
        },
    })));
    assert!(
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3100", &started,)
            .expect("valid runtime event record identities")
    );
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-change",
        "## Result ID\n\n`tool-result-call-change`\n\n## Tool Result\n\ncreated file",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-change",
        "tool_name": "file",
        "display_state": "completed",
        "host_result": {
            "kind": "change_set",
            "payload": {
                "mode": "applied",
                "files": [{
                    "resource_id": "resource-1",
                    "change": "modify",
                    "path": "src/demo.lua",
                    "hunks": [{
                        "before": "",
                        "delete": [
                            {"line": 1, "content": "return 1"},
                            {"line": 2, "content": "return old"}
                        ],
                        "insert": [
                            {"line": 1, "content": "return 2"},
                            {"line": 2, "content": "return 3"}
                        ],
                        "after": ""
                    }]
                }]
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3101",
        &finished,
    ).expect("valid runtime event record identities"));

    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("tool call record should remain visible");
    let metadata = tool_record.metadata.as_ref().expect("tool metadata");
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("change_set")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("files"))
            .and_then(serde_json::Value::as_array)
            .and_then(|files| files.first())
            .and_then(|file| file.pointer("/hunks/0/delete/1/content"))
            .and_then(serde_json::Value::as_str),
        Some("return old")
    );
    let detail_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        .expect("immutable tool detail should be appended");
    assert_eq!(
        detail_record
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("host_result"))
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("files"))
            .and_then(serde_json::Value::as_array)
            .and_then(|files| files.first())
            .and_then(|file| file.pointer("/hunks/0/insert/1/content"))
            .and_then(serde_json::Value::as_str),
        Some("return 3")
    );
}

/// Verifies compact Lua host results remain available to specialized transcript cards.
/// 验证紧凑 Lua 宿主结果会保留给专用转录卡片使用。
#[test]
fn lua_host_result_kinds_are_visible() {
    assert!(raw_host_result_kind_is_visible(Some("lua_execution")));
    assert!(raw_host_result_kind_is_visible(Some("lua_runtime_session")));
    assert!(raw_host_result_kind_is_visible(Some(
        "generated_attachments"
    )));
    assert!(raw_host_result_kind_is_visible(Some(
        "provider_tool_output"
    )));
}

/// Verifies wait_agent keeps child usage metadata on the visible tool row without copying child output text.
/// 验证 wait_agent 会在可见工具行保留子智能体用量元数据，同时不复制子智能体输出正文。
#[test]
fn tool_result_projection_preserves_compact_subagent_wait_usage() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-wait · wait_agent",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-wait",
        "tool_name": "wait_agent",
        "arguments": {
            "target": "reviewer"
        }
    })));
    assert!(
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3170", &started,)
            .expect("valid runtime event record identities")
    );
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-wait",
        "## Result ID\n\n`tool-result-call-wait`\n\n## Tool Result\n\nreviewer [completed]: large child output",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-wait",
        "tool_name": "wait_agent",
        "display_state": "completed",
        "host_result": {
            "kind": "subagent_wait",
            "payload": {
                "tool": "wait_agent",
                "message": "Subagent terminal result received.",
                "activity_status": "terminal_update",
                "terminal_event_count": 1,
                "message_event_count": 0,
                "status_event_count": 0,
                "result": "reviewer [completed]: large child output",
                "agents": [{
                    "agent_id": "reviewer",
                    "subagent_task_id": "22222222-2222-2222-2222-222222222222",
                    "execution_id": "exec-1",
                    "kind": "completed",
                    "status": "completed",
                    "display_title": "reviewer",
                    "result": "large child output",
                    "usage_segments": [{
                        "kind": "subagent_task",
                        "trigger": "reviewer",
                        "execution_profile": null,
                        "provider_id": "deepseek",
                        "model_id": "deepseek-v4-pro",
                        "provider_call_count": 2,
                        "tool_call_count": 1,
                        "input_tokens": 100,
                        "output_tokens": 20,
                        "reasoning_tokens": 3,
                        "total_tokens": 123,
                        "cached_input_tokens": 50,
                        "cache_creation_input_tokens": null
                    }]
                }]
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3171",
        &finished,
    ).expect("valid runtime event record identities"));

    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("wait_agent tool call record should remain visible");
    let metadata = tool_record.metadata.as_ref().expect("wait metadata");
    let host_payload = metadata
        .get("host_result")
        .and_then(|host_result| host_result.get("payload"))
        .expect("compact host payload should be retained");
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("subagent_wait")
    );
    assert_eq!(
        host_payload.get("tool").and_then(serde_json::Value::as_str),
        Some("wait_agent")
    );
    assert_eq!(
        host_payload
            .get("activity_status")
            .and_then(serde_json::Value::as_str),
        Some("terminal_update")
    );
    assert_eq!(
        host_payload
            .get("terminal_event_count")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert!(host_payload.get("result").is_none());
    assert!(host_payload.get("message").is_none());
    let agent = host_payload
        .get("agents")
        .and_then(serde_json::Value::as_array)
        .and_then(|agents| agents.first())
        .expect("compact agent should be retained");
    assert!(agent.get("result").is_none());
    assert_eq!(
        agent
            .get("usage_segments")
            .and_then(serde_json::Value::as_array)
            .and_then(|segments| segments.first())
            .and_then(|segment| segment.get("total_tokens"))
            .and_then(serde_json::Value::as_u64),
        Some(123)
    );
}

/// Verifies wait_agent child usage contributes to the parent turn usage groups.
/// 验证 wait_agent 子智能体用量会并入父级轮次用量分组。
#[test]
fn terminal_report_groups_compact_wait_agent_child_usage() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "3180", "等待智能体")
        .expect("valid user prompt identity");
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-wait-stats · wait_agent",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-wait-stats",
        "tool_name": "wait_agent",
    })));
    assert!(
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3181", &started,)
            .expect("valid runtime event record identities")
    );
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-wait-stats",
        "## Result ID\n\n`tool-result-call-wait-stats`\n\n## Tool Result\n\nworker [completed]: ok",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-wait-stats",
        "tool_name": "wait_agent",
        "display_state": "completed",
        "host_result": {
            "kind": "subagent_wait",
            "payload": {
                "tool": "wait_agent",
                "agents": [{
                    "agent_id": "worker",
                    "subagent_task_id": "22222222-2222-2222-2222-222222222222",
                    "execution_id": "exec-2",
                    "kind": "completed",
                    "status": "completed",
                    "usage_segments": [{
                        "kind": "subagent_task",
                        "trigger": "worker",
                        "execution_profile": null,
                        "provider_id": "deepseek",
                        "model_id": "deepseek-v4-pro",
                        "provider_call_count": 1,
                        "tool_call_count": 0,
                        "input_tokens": 80,
                        "output_tokens": 10,
                        "reasoning_tokens": 1,
                        "total_tokens": 91,
                        "cached_input_tokens": 30,
                        "cache_creation_input_tokens": null
                    }]
                }]
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3182",
        &finished,
    ).expect("valid runtime event record identities"));
    let report = usage_report(session_id, "已收到。", 40, 4, 0, 44, Some(12));

    apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "3183", &report)
        .expect("valid terminal report identities");

    let turn_stats = latest_turn_stats(&runtime_state);
    assert_eq!(turn_stats.total_tokens, 44);
    let subagent_group = turn_stats
        .usage_groups
        .iter()
        .find(|group| group.kind == vulcan_workbench_model::SessionTurnUsageGroupKind::Subagent)
        .expect("wait_agent child usage should be retained");
    assert_eq!(subagent_group.provider_call_count, 1);
    assert_eq!(subagent_group.input_tokens, 80);
    assert_eq!(subagent_group.output_tokens, 10);
    assert_eq!(subagent_group.reasoning_tokens, 1);
    assert_eq!(subagent_group.total_tokens, 91);
    assert_eq!(subagent_group.cached_input_tokens, Some(30));
}

/// Verifies ask answers are persisted as structured display data instead of requiring Markdown parsing.
/// 验证 ask 答案会以结构化展示数据持久化，而不需要从 Markdown 文本反解析。
#[test]
fn tool_result_projection_preserves_structured_ask_display() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-ask · ask",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-ask",
        "tool_name": "ask",
        "arguments": {
            "questions": [{
                "header": "确认",
                "question": "是否继续？"
            }]
        }
    })));
    assert!(
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3200", &started,)
            .expect("valid runtime event record identities")
    );
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-ask",
        "## Result ID\n\n`tool-result-call-ask`\n\n## Tool Result\n\n## User Answers\n\n- 确认: 继续\n\nContinue with these answers.",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-ask",
        "tool_name": "ask",
        "display_state": "completed",
        "host_result": {
            "kind": "ask",
            "payload": {
                "answers": [["继续"]],
                "display": {
                    "kind": "ask_question",
                    "title": "向用户提问",
                    "ignored_answer_text": "用户忽略回答",
                    "questions": [{
                        "header": "确认",
                        "question": "是否继续？",
                        "answers": ["继续"],
                        "answered": true
                    }]
                }
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3201",
        &finished,
    ).expect("valid runtime event record identities"));

    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("ask tool call record should remain visible");
    let metadata = tool_record.metadata.as_ref().expect("ask metadata");
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("ask")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("answers"))
            .and_then(serde_json::Value::as_array)
            .and_then(|answers| answers.first())
            .and_then(serde_json::Value::as_array)
            .and_then(|answers| answers.first())
            .and_then(serde_json::Value::as_str),
        Some("继续")
    );

    let detail_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolDetailOutput)
        .expect("ask detail record should be hidden");
    let detail_metadata = detail_record
        .metadata
        .as_ref()
        .expect("ask detail metadata");
    assert_eq!(
        detail_metadata
            .get("display")
            .and_then(|display| display.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("ask_question")
    );
    assert_eq!(
        detail_metadata
            .get("display")
            .and_then(|display| display.get("questions"))
            .and_then(serde_json::Value::as_array)
            .and_then(|questions| questions.first())
            .and_then(|question| question.get("answers"))
            .and_then(serde_json::Value::as_array)
            .and_then(|answers| answers.first())
            .and_then(serde_json::Value::as_str),
        Some("继续")
    );
}

/// Verifies submitted Plan artifacts stay visible so WebUI can render the confirmation card.
/// 验证已提交计划产物保留在可见元数据中，使 WebUI 能渲染确认卡片。
#[test]
fn tool_result_projection_preserves_plan_artifact_host_result() {
    let session_id = test_session_id();
    let mut runtime_state = SessionRuntimeStateEntry::default();
    let started = RuntimeEvent::new(
        RuntimeEventKind::ToolStarted,
        "工具调用 · call-plan · submit_plan",
        "## Tool Call",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-plan",
        "tool_name": "submit_plan",
        "arguments": {
            "name": "Test Plan"
        }
    })));
    assert!(
        append_runtime_event_projection_to_state(&mut runtime_state, session_id, "3300", &started,)
            .expect("valid runtime event record identities")
    );
    let finished = RuntimeEvent::new(
        RuntimeEventKind::ToolFinished,
        "工具返回 · call-plan",
        "## Result ID\n\n`tool-result-call-plan`\n\n## Tool Result\n\nPlan artifact saved.",
    )
    .with_metadata(Some(json!({
        "tool_call_id": "call-plan",
        "tool_name": "submit_plan",
        "display_state": "completed",
        "host_result": {
            "kind": "plan_artifact",
            "payload": {
                "name": "Test Plan",
                "path": "docs/plan/test-plan.md",
                "content": "# Test Plan\n\n- Step one",
                "updatedAt": "3301"
            }
        }
    })));
    assert!(append_runtime_event_projection_to_state(
        &mut runtime_state,
        session_id,
        "3301",
        &finished,
    ).expect("valid runtime event record identities"));

    let tool_record = runtime_state
        .conversation_state
        .records
        .iter()
        .find(|record| record.record_kind == SessionMessageRecordKind::ToolCallOutput)
        .expect("submit_plan tool call record should remain visible");
    let metadata = tool_record.metadata.as_ref().expect("plan metadata");
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("plan_artifact")
    );
    assert_eq!(
        metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("content"))
            .and_then(serde_json::Value::as_str),
        Some("# Test Plan\n\n- Step one")
    );
}

/// Verifies returned tool payloads are displayed as completed even when legacy host-result hints describe a semantic failure.
/// 验证已返回工具载荷会显示为完成，即使旧宿主结果提示描述了语义失败。
#[test]
fn returned_tool_payload_wins_over_host_failure_hint() {
    let metadata = json!({
        "host_result": {
            "kind": "tool_error",
            "payload": {
                "status": "failed",
            },
        },
    });
    assert_eq!(
        tool_display_state_for_finished_event(Some(&metadata), false),
        "completed"
    );
}

/// Verifies a running TaskSession acknowledgement stays running even though the spawn-agent call returned.
/// 验证 TaskSession 运行中确认在 spawn_agent 调用返回后仍保持运行态。
#[test]
fn running_task_session_result_keeps_running_display_state() {
    let metadata = json!({
        "host_result": {
            "kind": "task_session",
            "payload": {
                "task_session_id": test_session_id().to_string(),
                "execution_status": "running",
            },
        },
        "display_state": "completed",
    });

    assert_eq!(
        tool_display_state_for_finished_event(Some(&metadata), true),
        "running"
    );
}

/// Verifies duplicate prompt identity rejection preserves every runtime field changed by a successful append.
/// 验证重复提示词身份被拒绝时，会保留成功追加可能修改的每个运行态字段。
#[test]
fn user_prompt_append_is_atomic_on_duplicate_message_identity() {
    // session_id fixes the generated protocol identity for both append attempts.
    // session_id 固定两次追加生成的协议身份。
    let session_id = test_session_id();
    // runtime_state receives one valid prompt before the duplicate attempt is prepared.
    // runtime_state 会先接收一条有效提示词，再准备重复追加。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_user_prompt_record(&mut runtime_state, session_id, "5000", "first")
        .expect("first prompt identity should be valid");
    runtime_state.ui_state.unread = true;
    runtime_state.updated_at = String::from("before-duplicate");
    // original_state captures the complete runtime transaction boundary before failure.
    // original_state 捕获失败前的完整运行态事务边界。
    let original_state =
        serde_json::to_value(&runtime_state).expect("runtime state should serialize");

    // error proves the same generated identity is rejected before transcript or runtime flags change.
    // error 证明相同生成身份会在转录或运行标记变化前被拒绝。
    let error = append_user_prompt_record(&mut runtime_state, session_id, "5000", "duplicate")
        .expect_err("duplicate prompt identity must fail");

    assert!(error.contains("duplicates id"));
    assert_eq!(
        serde_json::to_value(&runtime_state).expect("runtime state should serialize after failure"),
        original_state
    );
}

/// Verifies a terminal assistant identity collision leaves the complete runtime state unchanged.
/// 验证终态助手身份冲突会使完整运行态保持不变。
#[test]
fn terminal_report_apply_is_atomic_on_assistant_identity_collision() {
    // session_id and terminal timestamp define the exact assistant identity used by the report.
    // session_id 与终态时间戳定义报告使用的准确助手身份。
    let session_id = test_session_id();
    // runtime_state already owns the terminal identity before the report transaction starts.
    // runtime_state 在报告事务开始前已经持有该终态身份。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    append_session_conversation_message(
        &mut runtime_state.conversation_state,
        SessionConversationMessage {
            id: format!("session-{session_id}-protocol-assistant-6000"),
            payload: SessionConversationPayload::UserText {
                text: String::from("collision owner"),
                attachments: Vec::new(),
            },
        },
    )
    .expect("initial collision owner should be valid");
    runtime_state.updated_at = String::from("before-terminal");
    // original_state captures messages, records, usage, and UI fields before draft application.
    // original_state 捕获草稿应用前的消息、记录、用量与界面字段。
    let original_state =
        serde_json::to_value(&runtime_state).expect("runtime state should serialize");
    // report requires a new assistant message with the already-owned identity.
    // report 要求使用已经被占用的身份创建新助手消息。
    let report = usage_report(session_id, "terminal output", 10, 2, 0, 12, None);

    // error proves the detached terminal draft is discarded in full.
    // error 证明分离的终态草稿会被整体丢弃。
    let error =
        apply_terminal_execution_report_to_state(&mut runtime_state, session_id, "6000", &report)
            .expect_err("terminal assistant identity collision must fail");

    assert!(error.contains("duplicates id"));
    assert_eq!(
        serde_json::to_value(&runtime_state).expect("runtime state should serialize after failure"),
        original_state
    );
}

/// Verifies an owned runtime-event projection restores exact open cards after an append collision.
/// 验证自有运行时事件投影在追加身份冲突后会恢复精确开放卡片。
#[test]
fn owned_runtime_event_projection_rolls_back_without_cloning_history() {
    // sessionId and now determine the exact provider-retry record identity that will collide.
    // sessionId 与 now 决定将发生冲突的精确 Provider 重试记录身份。
    let session_id = test_session_id();
    let now = "7000";
    let colliding_id = build_session_record_message_id(session_id, now, 1);
    // runtimeState owns one open request card whose id deliberately matches the next append identity.
    // runtimeState 持有一张开放请求卡片，其标识被刻意设置为下一追加身份。
    let mut runtime_state = SessionRuntimeStateEntry::default();
    runtime_state
        .conversation_state
        .records
        .push(SessionConversationRecord {
            id: colliding_id.clone(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolCallOutput,
            visible: true,
            title: String::new(),
            body: String::new(),
            metadata: Some(json!({
                "request_stream_id": "request-1",
                "request_revision": 1,
                "tool_call_id": "call-1",
                "tool_name": "shell_command",
                "tool_display_state": "receiving"
            })),
            created_at: String::from("6999"),
        });
    runtime_state
        .tool_request_record_ids
        .insert(String::from("request-1"), colliding_id.clone());
    runtime_state
        .tool_call_record_ids
        .insert(String::from("call-1"), colliding_id);
    runtime_state.updated_at = String::from("before-retry");
    // originalState is the exact transaction boundary that must survive the failed projection.
    // originalState 是失败投影后必须保持不变的精确事务边界。
    let original_state =
        serde_json::to_value(&runtime_state).expect("runtime state should serialize");
    let event = RuntimeEvent::new(RuntimeEventKind::Retrying, "Provider retry", "retrying");

    // restoredState must carry the pre-event row and correlations after the generated id collides.
    // restoredState 必须在生成身份冲突后携带事件前记录与关联。
    let (restored_state, error) =
        append_runtime_event_projection_to_owned_state(runtime_state, session_id, now, &event)
            .expect_err("duplicate retry-note identity must fail");

    assert!(error.contains("duplicates id"));
    assert_eq!(
        serde_json::to_value(&restored_state)
            .expect("restored runtime state should serialize after failure"),
        original_state
    );
}

/// Projects visible records into comparable test tuples.
/// 将可见记录投影成可比较的测试元组。
fn visible_record_projection(
    runtime_state: &SessionRuntimeStateEntry,
) -> Vec<(SessionMessageKind, SessionMessageRecordKind, String)> {
    runtime_state
        .conversation_state
        .records
        .iter()
        .filter(|record| record.visible)
        .map(|record| (record.kind, record.record_kind, record.body.clone()))
        .collect()
}
/// Verifies shell-command display projection preserves ordered generic plugin rewrite badges.
/// 验证 shell 命令展示投影会保留有序的通用插件改写徽标。
#[test]
fn shell_command_display_preserves_multiple_plugin_contributions() {
    let metadata = json!({
        "host_result": {
            "kind": "command_run",
            "payload": {
                "hook_transform": {
                    "original_command": "git diff",
                    "effective_command": "rtk git diff",
                    "contributions": [
                        { "badge": "RTK", "label": "Command router", "value": "vulcan-rtk (git)" },
                        { "badge": "VMM", "label": "Memory context", "value": "vmm-memory" }
                    ]
                }
            }
        }
    });
    let display = super::tool_display_payloads::shell_command_display_payload(
        "completed",
        Some(&metadata),
        "Command: rtk git diff\nStatus: completed",
    );
    assert_eq!(display["command"], "git diff");
    assert_eq!(display["effective_command"], "rtk git diff");
    assert_eq!(display["plugin_contributions"][0]["badge"], "RTK");
    assert_eq!(display["plugin_contributions"][1]["badge"], "VMM");
}
