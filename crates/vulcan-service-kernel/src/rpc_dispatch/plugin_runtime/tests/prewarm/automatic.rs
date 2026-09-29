//! Management can inspect and cancel an automatic target without impersonating its lifecycle owner.
//! 管理端可以检查及取消自动目标，但不能冒充其生命周期所有者。

use super::*;
use vulcan_plugin_runtime::{VulcanPluginAutomaticPrewarmOrigin, VulcanPluginPrewarmTargetRequest};

/// Real target provenance crosses RPC; manual startup is refused and original cancellation remains available.
/// 真实目标来源跨越 RPC；拒绝手动启动，同时保留原取消能力。
#[tokio::test]
async fn plugin_automatic_prewarm_receipt_retains_origin_and_rejects_manual_start() {
    // Existing real fixtures supply indexed workspace ownership and the production module declaration.
    // 既有真实夹具提供索引工作区归属及生产模块声明。
    let fixture = Fixture::new();
    let owner = uuid::Uuid::new_v4();
    let workspace = fixture.attach_workspace(owner);
    // Real indexed storage lets manual admission reach the automatic-origin authorization boundary.
    // 真实索引存储使手动准入能够到达自动来源授权边界。
    let storage = TestHostRuntimeRoot::new(owner);
    fixture.state.lock().expect("state").controller_runtime =
        Some(storage.attached_host_runtime_handle().await);
    let generation = fixture.enable_prewarm();
    let policy = fixture
        .rpc("service_plugin_prewarm_policy_status", json!({}))
        .await
        .expect("policy");
    let committed = fixture.rpc("service_begin_plugin_prewarm_policy_update", json!({"request": {
        "updateId": uuid::Uuid::new_v4().to_string(), "runtimeId": policy["runtimeId"],
        "expectedRevision": policy["revision"], "rules": [{"pluginId": "control-plugin", "toolId": "control_ping", "targetIdle": 1}]
    }})).await.expect("explicit policy");
    // This direct trusted-host preparation tests receipt projection, not an already-wired lifecycle trigger.
    // 此直接可信宿主准备测试回执投影，并非已经接线的生命周期触发。
    let origin = VulcanPluginAutomaticPrewarmOrigin {
        trigger_id: uuid::Uuid::new_v4().to_string(),
        policy_revision: committed["resultRevision"]
            .as_str()
            .expect("revision")
            .into(),
    };
    let prepared = fixture
        .runtime
        .prepare_automatic_tool_prewarm(
            VulcanPluginPrewarmTargetRequest {
                runtime_id: policy["runtimeId"].as_str().expect("runtime").into(),
                plugin_id: "control-plugin".into(),
                tool_id: "control_ping".into(),
                plugin_generation: generation.parse().expect("exact generation"),
                owner_session_id: owner.to_string(),
                workspace_root: workspace,
                target_idle: 1,
            },
            origin.clone(),
        )
        .expect("trusted automatic preparation");
    let identity = json!({"runtimeId": policy["runtimeId"], "ownerSessionId": owner.to_string(), "invocationId": prepared.invocation_id});
    let projected = fixture
        .rpc("service_plugin_prewarm_status", identity.clone())
        .await
        .expect("original receipt");
    assert_eq!(
        projected["automatic"],
        serde_json::to_value(&origin).expect("origin wire")
    );
    assert_eq!(projected["automatic"]["triggerId"], origin.trigger_id);
    assert_eq!(
        projected["automatic"]["policyRevision"],
        origin.policy_revision
    );
    assert!(!projected["started"].as_bool().expect("unstarted"));
    // Assert the actual authorization reason rather than accepting an unrelated workspace failure.
    // 断言真实授权原因，避免将无关工作区失败当作通过。
    let rejected = fixture
        .rpc("service_start_plugin_prewarm", identity.clone())
        .await
        .expect_err("manual authority rejected")
        .to_string();
    assert!(rejected.contains("original host lifecycle"), "{rejected}");
    assert_eq!(
        fixture
            .runtime
            .formal_runtime_status()
            .expect("status")
            .expect("parent")
            .resources
            .resident,
        0
    );
    let cancelled = fixture
        .rpc("service_cancel_plugin_prewarm", identity.clone())
        .await
        .expect("cancel original");
    assert_eq!(cancelled["automatic"], projected["automatic"]);
    assert_eq!(cancelled["retired"], true);
    assert_eq!(
        fixture
            .rpc("service_forget_plugin_prewarm", identity)
            .await
            .expect("acknowledge cancellation"),
        Value::Null
    );
    fixture.close().await;
}
