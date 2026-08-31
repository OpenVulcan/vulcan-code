//! Regression tests for the streaming boundary.
//! streaming边界的回归测试。

use super::*;

/// Verifies explicit response completion terminates the adapter without requiring transport EOF.
/// 验证显式响应完成会终止适配器，而不要求传输层先到达 EOF。
#[test]
fn response_completion_is_a_protocol_terminal_boundary() {
    assert!(public_stream_event_is_terminal(
        &AiStreamEvent::ResponseCompleted
    ));
    assert!(public_stream_event_is_terminal(
        &AiStreamEvent::ResponseCancelled
    ));
    assert!(!public_stream_event_is_terminal(
        &AiStreamEvent::ResponseStarted {
            execution_id: String::from("response-1"),
        }
    ));
}
