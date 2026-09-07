//! Public lifecycle serialization contract without legacy state inference.
//! 不推断旧状态的公共生命周期序列化契约。

use vulcan_memorymesh_core::{StorageMemorySession, StorageSessionStatus};

/// Requires explicit lifecycle persistence and rejects unknown states instead of activating them.
/// 要求显式持久化生命周期，拒绝未知状态而不是将其激活。
#[test]
fn session_snapshot_requires_explicit_known_lifecycle() {
    // Model the exact persisted session schema without a lifecycle value first.
    // 首先构造缺少生命周期值的准确持久会话结构。
    let mut value = serde_json::json!({
        "subject_id": "vmm-subject:1", "scope_id": "vmm-project:2",
        "session_source_id": "state-fixture", "last_extraction_observed_at_ns": 0,
        "last_extraction_completed_at_ns": null
    });
    assert!(serde_json::from_value::<StorageMemorySession>(value.clone()).is_err());
    for (label, expected) in [
        ("active", StorageSessionStatus::Active),
        ("archived", StorageSessionStatus::Archived),
        ("recycled", StorageSessionStatus::Recycled),
        ("forgotten", StorageSessionStatus::Forgotten),
    ] {
        value["status"] = serde_json::json!(label);
        // Validate exact round-trip fields, not just successful deserialization.
        // 验证准确往返字段，而不只验证反序列化成功。
        let session: StorageMemorySession =
            serde_json::from_value(value.clone()).expect("known status");
        assert_eq!(session.status, expected);
        assert_eq!(serde_json::to_value(session).expect("round trip"), value);
    }
    value["status"] = serde_json::json!("invalid");
    assert!(serde_json::from_value::<StorageMemorySession>(value).is_err());
}
