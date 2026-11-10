//! Regression coverage for partial response publication when capture scope ends.
//! 捕获作用域结束时发布部分响应的回归覆盖。

use super::super::failure::{
    TransportFailureEvidence, TransportFailurePhase, response_headers_timeout_evidence,
};
use super::*;

/// Verifies dropping an unfinished capture retains bytes and marks an explicit cancellation stage.
/// 验证未完成捕获被释放时会保留字节并标记明确的取消阶段。
#[tokio::test]
async fn unfinished_wire_response_capture_records_cancelled_stage() {
    let (sender, receiver) = oneshot::channel();
    {
        let mut capture = ProviderWireResponseCaptureGuard::new("http_sse", Some(sender));
        capture.observe_chunk(b"partial-response");
    }

    let captured = receiver.await.expect("capture should publish on drop");
    assert_eq!(captured.body, b"partial-response");
    assert_eq!(captured.received_bytes, 16);
    assert_eq!(captured.outcome, "stream_failed_or_cancelled");
    assert_eq!(captured.failure_stage.as_deref(), Some("cancelled"));
    assert!(captured.failure_message.is_some());
    assert_eq!(
        captured.body_sha256,
        hex::encode(Sha256::digest(b"partial-response"))
    );
}

/// Verifies the first transport-failure evidence wins and later causes never overwrite it.
/// 验证首个传输失败证据胜出，后续原因不会覆盖它。
#[tokio::test]
async fn wire_response_capture_keeps_first_transport_failure_evidence() {
    let (sender, receiver) = oneshot::channel();
    {
        let mut capture = ProviderWireResponseCaptureGuard::new("http_sse", Some(sender));
        capture.observe_transport_failure(&TransportFailureEvidence {
            phase: TransportFailurePhase::Body,
            causes: vec![String::from("connection closed before message completed")],
            timeout_ms: None,
        });
        capture.observe_transport_failure(&response_headers_timeout_evidence(60_000));
    }

    let captured = receiver.await.expect("capture should publish on drop");
    assert_eq!(captured.transport_failure_phase.as_deref(), Some("body"));
    assert_eq!(
        captured.transport_failure_causes,
        Some(vec![String::from(
            "connection closed before message completed"
        )])
    );
    // the later timeout evidence must not replace the first recorded phase or add its bound.
    // 后续的超时证据不得替换首个阶段，也不得补写它的时限。
    assert_eq!(captured.transport_failure_timeout_ms, None);
}
