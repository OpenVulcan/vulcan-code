//! Exact operation identity protection for cooperative cancellation.
//! 协作取消的精确操作身份保护。

use super::*;

/// Verifies a stale cancellation request cannot stop a newer knowledge operation.
/// 验证过期取消请求不能停止更新的知识库操作。
#[test]
fn cancellation_requires_exact_active_job_identity() {
    let signal = Arc::new(AtomicBool::new(false));
    let active = ActiveKnowledgeOperation {
        job_id: String::from("550e8400-e29b-41d4-a716-446655440000"),
        signal: Arc::clone(&signal),
    };
    assert!(!signal_matching_operation_cancellation(
        Some(&active),
        "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
    ));
    assert!(!signal.load(Ordering::Acquire));
    assert!(signal_matching_operation_cancellation(
        Some(&active),
        "550e8400-e29b-41d4-a716-446655440000"
    ));
    assert!(signal.load(Ordering::Acquire));
}
