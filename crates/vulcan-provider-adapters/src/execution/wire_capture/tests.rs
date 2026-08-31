//! Regression coverage for partial response publication when capture scope ends.
//! 捕获作用域结束时发布部分响应的回归覆盖。

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
