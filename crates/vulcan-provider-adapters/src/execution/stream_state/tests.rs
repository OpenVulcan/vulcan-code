//! Regression coverage for semantic output and transparent replay safety.
//! 语义输出与透明重放安全性的回归覆盖。

use super::super::failure::stream_transport_failure;
use super::*;

/// Verifies an emitted usage fact closes the transparent-replay safety window.
/// 验证已发出的用量事实会关闭透明重放安全窗口。
#[test]
fn usage_output_marks_subsequent_transport_failure_retry_unsafe() {
    let mut observed = false;
    let mut usage = AiStreamEvent::UsageUpdated {
        usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
    };
    update_semantic_output_observation(&mut observed, &mut usage);
    assert!(observed);

    let failure = stream_transport_failure("stream lost", observed);
    assert!(failure.error.semantic_output_observed);
    assert!(!failure.error.retryable);
}
