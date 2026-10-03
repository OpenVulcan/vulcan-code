//! Diagnostic RPCs read original ownership without creating native resources or granting mutation.
//! 诊断 RPC 读取原所有权，不创建原生资源或授予变更权限。

use super::*;

/// Reject malformed identities before runtime lookup; no inputs or return value are required.
/// 在运行时查找前拒绝畸形身份；无需输入或返回值。
#[tokio::test]
async fn plugin_diagnostic_rpc_rejects_ambiguous_parameters() {
    // The uninitialized service distinguishes parameter rejection from runtime-dependent failure.
    // 未初始化服务区分参数拒绝与依赖运行时的失败。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        uuid::Uuid::new_v4(),
    )));
    // Every malformed request retains the actual public method instead of falling through another router.
    // 每个畸形请求保留实际公开方法，不转入其他路由器。
    let malformed = [
        ("service_plugin_resource_diagnostics", json!(null)),
        (
            "service_plugin_resource_diagnostics",
            json!(["runtime", "plugin"]),
        ),
        (
            "service_plugin_resource_diagnostics",
            json!({"runtimeId": "runtime"}),
        ),
        (
            "service_plugin_resource_diagnostics",
            json!({"runtimeId": "", "pluginId": "plugin"}),
        ),
        (
            "service_plugin_resource_diagnostics",
            json!({"runtimeId": "runtime", "pluginId": "plugin", "cancel": true}),
        ),
        (
            "service_plugin_update_drain_diagnostics",
            json!({"runtimeId": "runtime", "pluginId": "plugin", "requestId": "invalid-uuid"}),
        ),
        (
            "service_plugin_uninstall_drain_diagnostics",
            json!({"runtimeId": "runtime", "pluginId": "plugin"}),
        ),
    ];
    // Loop variables name the exact contract and malformed carrier under test.
    // 循环变量标识被验证的精确契约及畸形载体。
    for (method, params) in malformed {
        // The top-level router must reject before touching the absent runtime owner.
        // 顶层路由器必须在访问缺失运行时所有者之前拒绝。
        let error = Box::pin(crate::rpc_dispatch::dispatch_service_rpc(
            ServiceEventEmitter::new(Vec::new()),
            state.clone(),
            method.to_owned(),
            Some(params),
        ))
        .await
        .expect_err("strict diagnostic parameters must be rejected");
        assert!(
            matches!(error, WebuiServiceRpcError::InvalidParams(_)),
            "{method}: {error:?}"
        );
    }
}

/// Read real metadata and reject stale identity without allocating a VM; no inputs or return value are required.
/// 读取真实元数据并拒绝过期身份，不分配 VM；无需输入或返回值。
#[tokio::test]
async fn plugin_diagnostic_rpc_preserves_parent_and_original_request_identity() {
    // The native fixture deliberately errors if metadata reads accidentally evaluate the Lua module.
    // 若元数据读取误执行 Lua 模块，原生夹具会特意报错。
    let fixture = Fixture::new();
    // Sample actual resources before any diagnostic read to detect unintended admission or retirement.
    // 在任何诊断读取前采样实际资源，检测非预期入场或退役。
    let before = fixture
        .runtime
        .formal_runtime_status()
        .expect("actual parent")
        .expect("attached parent");
    // The browser uses only this exact native parent identity.
    // 浏览器仅使用此精确原生父级身份。
    let params = json!({"runtimeId": before.runtime_id, "pluginId": "control-plugin"});
    // Empty retained ownership remains a real sample, not a newly prepared runtime instance.
    // 空保留所有权仍为真实采样，不是新准备的运行时实例。
    let report = fixture
        .rpc("service_plugin_resource_diagnostics", params.clone())
        .await
        .expect("read-only metadata");
    assert_eq!(report["runtimeId"], before.runtime_id);
    assert_eq!(report["pluginId"], "control-plugin");
    assert_eq!(report["pinnedSessionCount"], 0);
    assert_eq!(report["resources"], json!([]));
    assert_eq!(report["nonAtomicSnapshot"], true);
    assert!(
        fixture
            .rpc(
                "service_plugin_resource_diagnostics",
                json!({"runtimeId": "stale-parent", "pluginId": "control-plugin"}),
            )
            .await
            .is_err()
    );
    // A syntactically valid absent request cannot create an update drain or pretend it has completed.
    // 语法有效但缺失的请求不能创建更新排空，也不能假装已完成。
    let request = json!({
        "runtimeId": before.runtime_id,
        "pluginId": "control-plugin",
        "requestId": uuid::Uuid::new_v4().to_string(),
    });
    assert!(
        fixture
            .rpc("service_plugin_update_drain_diagnostics", request.clone())
            .await
            .expect("absent live update evidence")
            .is_null()
    );
    assert!(
        fixture
            .rpc("service_plugin_uninstall_drain_diagnostics", request)
            .await
            .is_err()
    );
    // Native usage must stay unchanged across successful and rejected management reads.
    // 成功及被拒绝的管理读取均不得改变原生用量。
    let after = fixture
        .runtime
        .formal_runtime_status()
        .expect("parent after reads")
        .expect("same attached parent");
    assert_eq!(after.runtime_id, before.runtime_id);
    assert_eq!(
        serde_json::to_value(after.resources).expect("after resource metadata"),
        serde_json::to_value(before.resources).expect("before resource metadata")
    );
    fixture.close().await;
}
