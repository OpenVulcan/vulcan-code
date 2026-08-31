//! Regression tests for the trace boundary.
//! trace边界的回归测试。

use super::*;

/// Verifies trace headers preserve ordinary protocol fields while removing access material.
/// 验证追踪请求头保留普通协议字段，同时移除访问凭据材料。
#[test]
fn provider_request_trace_headers_redact_access_material() {
    let headers = BTreeMap::from([
        (
            String::from("content-type"),
            String::from("application/json"),
        ),
        (String::from("Authorization"), String::from("Bearer secret")),
        (
            String::from("x-session-token"),
            String::from("secret-token"),
        ),
    ]);

    let traced = provider_request_trace_headers(&headers);

    assert_eq!(
        traced.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        traced.get("Authorization").map(String::as_str),
        Some("<redacted>")
    );
    assert_eq!(
        traced.get("x-session-token").map(String::as_str),
        Some("<redacted>")
    );
}

/// Verifies host-side failures fill the trace stage only when no lower transport failure exists.
/// 验证仅当不存在更底层传输失败时，宿主侧失败才会补充追踪阶段。
#[test]
fn provider_api_trace_failure_selection_preserves_the_earliest_boundary() {
    let processing_failure = ProviderApiTraceProcessingFailure {
        stage: String::from("tool_request_coordinator"),
        message: String::from("tool stream ordering failed"),
    };

    let (stage, message, local_processing_failed) =
        select_provider_api_trace_failure(None, None, Some(&processing_failure));
    assert_eq!(stage.as_deref(), Some("tool_request_coordinator"));
    assert_eq!(message.as_deref(), Some("tool stream ordering failed"));
    assert!(local_processing_failed);

    let (stage, message, local_processing_failed) = select_provider_api_trace_failure(
        Some("protocol_decoder"),
        Some("invalid frame"),
        Some(&processing_failure),
    );
    assert_eq!(stage.as_deref(), Some("protocol_decoder"));
    assert_eq!(message.as_deref(), Some("invalid frame"));
    assert!(!local_processing_failed);
}

/// Verifies tool construction events are attributed to the request coordinator trace stage.
/// 验证工具构造事件会归属到工具请求协调器追踪阶段。
#[test]
fn provider_api_trace_classifies_tool_request_coordinator_events() {
    let event = AiStreamEvent::ToolCallStarted {
        item_id: String::from("item-1"),
        call_id: String::from("call-1"),
        tool_name: String::from("file_edit"),
        index: 0,
    };

    assert_eq!(
        provider_trace_processing_stage_for_event(&event),
        "tool_request_coordinator"
    );
    assert_eq!(
        provider_trace_processing_stage_for_event(&AiStreamEvent::ResponseCompleted),
        "response_projection"
    );
}

/// Verifies the detached trace writer receives one controlled host-side processing failure.
/// 验证独立追踪写入器会收到一项受控的宿主侧处理失败。
#[tokio::test]
async fn provider_api_trace_processing_failure_sender_publishes_once() {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let mut publisher = ProviderApiTraceProcessingFailureSender::new(Some(sender));
    let error = provider_failure("provider", "tool stream ordering failed");

    publisher.observe_failure("tool_request_coordinator", &error);
    publisher.observe_failure("response_projection", &error);

    let failure = receiver
        .await
        .expect("the first processing failure should be published");
    assert_eq!(failure.stage, "tool_request_coordinator");
    assert!(failure.message.contains("tool stream ordering failed"));
}

/// Verifies diagnostic draining consumes events after the host has already rejected a turn.
/// 验证诊断排空会在宿主已拒绝回合后继续消费剩余事件。
#[tokio::test]
async fn provider_trace_drain_consumes_the_remaining_stream() {
    // tailObserved reports that the detached diagnostic consumer reached the simulated body tail.
    // tailObserved 用于报告分离式诊断消费者已经到达模拟正文尾部。
    let (tail_observed_sender, tail_observed_receiver) = tokio::sync::oneshot::channel();
    // remainingStream models normalized events still backed by unread Provider response bytes.
    // remainingStream 模拟仍由未读取 Provider 响应字节支撑的规范化事件。
    let remaining_stream: ProviderExecutionStream = Box::pin(async_stream::stream! {
        yield Ok(AiStreamEvent::ResponseStarted {
            execution_id: String::from("trace-drain-response"),
        });
        let _ = tail_observed_sender.send(());
        yield Ok(AiStreamEvent::ResponseCompleted);
    });

    drain_provider_stream_for_trace(remaining_stream);

    // observedTail fails if the detached drain stops after the first remaining event.
    // observedTail 在分离式排空于首个剩余事件后停止时失败。
    tokio::time::timeout(std::time::Duration::from_secs(1), tail_observed_receiver)
        .await
        .expect("trace drain should reach the response tail")
        .expect("trace drain should keep the tail observer alive");
}
