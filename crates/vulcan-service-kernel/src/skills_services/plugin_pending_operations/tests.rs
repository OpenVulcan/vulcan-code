//! Wire contract of the unified pending-operation list and continuation entry.
//! 统一待处理操作列表与继续入口的线协议契约。

use super::*;
use serde_json::json;

/// Canonical UUID used as every fixture identity.
/// 用作所有夹具身份的规范 UUID。
const OPERATION: &str = "7d2c1a40-5a8f-4f62-9c8e-2f3b6d1e0a11";

/// Decode one continuation exactly as the RPC boundary does.
/// 按 RPC 边界的方式解码单个继续请求。
fn decode(value: serde_json::Value) -> Result<PluginPendingContinuation, serde_json::Error> {
    serde_json::from_value(value)
}

/// Build one prewarm receipt in phase, optionally carrying live persistence uncertainty.
/// 以 phase 构建单个预热回执，可选携带活动持久化不确定性。
fn prewarm_receipt(
    phase: &str,
    persistence_error: Option<&str>,
) -> VulcanPluginPrewarmPolicyRecord {
    let mut value = json!({
        "request": { "updateId": OPERATION, "runtimeId": "runtime", "expectedRevision": "before", "rules": [] },
        "phase": phase,
        "resultRevision": "3b0f6f62-1d55-4f0e-8a51-6c0f5c9b2e7d",
        "error": null,
    });
    if let Some(error) = persistence_error {
        value["persistenceError"] = json!(error);
    }
    serde_json::from_value(value).expect("prewarm receipt fixture")
}

/// Collect only the offered action names, preserving their declared order.
/// 仅收集提供的动作名称，保留声明顺序。
fn offered(operation: &PluginPendingOperation) -> Vec<PluginPendingAction> {
    operation.actions.iter().map(|offer| offer.action).collect()
}

#[test]
fn continuation_accepts_each_kind_only_with_its_declared_actions() {
    for (kind, action) in [
        ("update", "resume"),
        ("update", "abort"),
        ("uninstall", "resume"),
    ] {
        decode(json!({ "kind": kind, "operationId": OPERATION, "pluginId": "sample", "action": action }))
            .unwrap_or_else(|error| panic!("{kind}/{action} must decode: {error}"));
    }
    for kind in ["capacityPolicy", "prewarmPolicy"] {
        for action in ["resume", "cancel", "forget"] {
            decode(json!({ "kind": kind, "operationId": OPERATION, "action": action }))
                .unwrap_or_else(|error| panic!("{kind}/{action} must decode: {error}"));
        }
    }
    for (kind, action) in [
        ("update", "cancel"),
        ("update", "forget"),
        ("uninstall", "abort"),
        ("uninstall", "forget"),
        ("capacityPolicy", "abort"),
        ("prewarmPolicy", "abort"),
    ] {
        let mut request = json!({ "kind": kind, "operationId": OPERATION, "action": action });
        if matches!(kind, "update" | "uninstall") {
            request["pluginId"] = json!("sample");
        }
        assert!(decode(request).is_err(), "{kind}/{action} must be rejected");
    }
}

#[test]
fn continuation_requires_exactly_the_identities_of_its_kind() {
    assert!(
        decode(json!({ "kind": "update", "operationId": OPERATION, "action": "resume" })).is_err(),
        "update continuations need the owning namespace"
    );
    assert!(
        decode(json!({ "kind": "capacityPolicy", "operationId": OPERATION, "pluginId": "sample", "action": "resume" }))
            .is_err(),
        "policy continuations carry no namespace"
    );
    assert!(
        decode(json!({ "kind": "uninstall", "operationId": OPERATION, "pluginId": "sample", "action": "resume", "extra": true }))
            .is_err(),
        "unknown fields are rejected"
    );
    let decoded = decode(json!({
        "kind": "uninstall", "operationId": OPERATION, "pluginId": "sample", "action": "resume",
        "acknowledgeExternalEffects": true,
    }))
    .expect("acknowledged uninstall continuation");
    assert!(matches!(
        decoded,
        PluginPendingContinuation::Uninstall {
            acknowledge_external_effects: true,
            ..
        }
    ));
}

#[test]
fn policy_receipts_offer_forget_only_once_durably_terminal() {
    use PluginPendingAction::{Cancel, Forget, Resume};
    let cases = [
        (prewarm_receipt("prepared", None), vec![Cancel, Resume]),
        (prewarm_receipt("publishing", None), vec![Resume]),
        (prewarm_receipt("committed", None), vec![Forget]),
        (prewarm_receipt("interrupted", None), vec![Forget]),
        (
            prewarm_receipt("committed", Some("disk full")),
            vec![Resume],
        ),
    ];
    for (record, expected) in cases {
        let phase = record.phase;
        let operation = PluginPendingOperation::prewarm_policy(record);
        assert_eq!(
            offered(&operation),
            expected,
            "prewarm receipt in {phase:?}"
        );
        assert!(
            operation
                .actions
                .iter()
                .all(|offer| !offer.requires_acknowledgement)
        );
    }
}

#[test]
fn listed_operations_serialize_kind_identity_actions_and_detail() {
    let operation = PluginPendingOperation::prewarm_policy(prewarm_receipt("prepared", None));
    let value = serde_json::to_value(&operation).expect("operation serializes");
    assert_eq!(value["kind"], "prewarmPolicy");
    assert_eq!(value["operationId"], OPERATION);
    assert!(value["pluginId"].is_null());
    assert_eq!(value["detail"]["phase"], "prepared");
    assert_eq!(
        value["actions"],
        json!([
            { "action": "cancel", "requiresAcknowledgement": false },
            { "action": "resume", "requiresAcknowledgement": false },
        ])
    );
}
