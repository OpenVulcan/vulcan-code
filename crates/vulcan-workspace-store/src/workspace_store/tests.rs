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

/// Verifies rejected VMM structured outputs round-trip through bounded slots and reject overflow.
/// 验证被拒绝的 VMM 结构化输出经有界槽位往返，并拒绝越界与超限记录。
#[test]
fn vmm_structured_output_failure_slots_are_bounded() {
    let root = std::env::temp_dir().join(format!(
        "vulcan-vmm-structured-output-failure-test-{}",
        uuid::Uuid::new_v4()
    ));
    let record = serde_json::json!({
        "schema_version": "vulcan.vmm-structured-output-failure.v1",
        "raw_output": "{\"memory\":{}}",
    });

    let path = persist_vmm_structured_output_failure(&root, 3, &record)
        .expect("persist VMM structured-output failure");

    assert_eq!(
        path,
        root.join("diagnostics")
            .join(VMM_STRUCTURED_OUTPUT_FAILURE_DIRECTORY)
            .join("slot-03.json")
    );
    let restored = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(&path).expect("read VMM structured-output failure"),
    )
    .expect("decode VMM structured-output failure");
    assert_eq!(restored, record);
    assert!(
        persist_vmm_structured_output_failure(
            &root,
            VMM_STRUCTURED_OUTPUT_FAILURE_SLOT_COUNT,
            &record
        )
        .is_err(),
        "slot indexes at or above the slot count must be rejected"
    );
    let oversized_len = usize::try_from(VMM_STRUCTURED_OUTPUT_FAILURE_MAX_BYTES)
        .expect("slot capacity fits usize")
        + 1;
    let oversized = serde_json::json!({ "raw_output": "x".repeat(oversized_len) });
    assert!(
        persist_vmm_structured_output_failure(&root, 0, &oversized).is_err(),
        "records above the slot capacity must be rejected"
    );

    std::fs::remove_dir_all(&root).expect("remove VMM structured-output failure test root");
}
