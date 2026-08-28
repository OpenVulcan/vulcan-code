//! Workspace-store regression tests extracted from the storage implementation module.
//! 从存储实现模块中抽取的工作区存储回归测试。

use super::*;
/// Verifies configured-inference traces are atomically isolated by their authenticated consumer profile.
/// 验证已配置推理追踪会按已认证消费者配置档原子隔离。
#[test]
fn configured_inference_provider_request_trace_is_consumer_scoped() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-configured-inference-trace-test-{}",
        uuid::Uuid::new_v4()
    ));
    let trace = serde_json::json!({
        "schema_version": "vulcan.configured-inference-provider-request-trace.v1",
        "api_request": {"body": {"reasoning": {"effort": "none"}}},
    });

    let trace_path = persist_configured_inference_provider_request_trace(&root, "vmm", 1, &trace)
        .expect("persist configured-inference provider request trace");

    assert_eq!(
        trace_path,
        root.join("diagnostics")
            .join(CONFIGURED_INFERENCE_PROVIDER_REQUEST_TRACE_DIRECTORY)
            .join("vmm")
            .join("slot-01.json")
    );
    let restored = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(&trace_path).expect("read configured-inference trace"),
    )
    .expect("decode configured-inference trace");
    assert_eq!(restored, trace);
    assert!(
        persist_configured_inference_provider_request_trace(&root, "../escape", 0, &trace,)
            .is_err(),
        "path-like consumer profiles must be rejected"
    );

    std::fs::remove_dir_all(&root).expect("remove configured-inference trace test root");
}
