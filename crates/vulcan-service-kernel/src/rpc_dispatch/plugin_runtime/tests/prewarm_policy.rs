//! Real policy RPC values and strict inputs cover durable intent without plugin execution.
//! 真实策略 RPC 值及严格输入覆盖持久意图，不执行插件。

use super::*;

/// Reject positional, missing, null and unknown mutation fields before accessing uninitialized state.
/// 在访问未初始化状态前拒绝位置、缺失、空值及未知变更字段。
#[tokio::test]
async fn plugin_prewarm_policy_rpc_requires_explicit_named_rules() {
    // Unattached service state distinguishes parsing failures from manager access failures.
    // 未附加服务状态区分解析失败与管理器访问失败。
    let state = Arc::new(Mutex::new(test_service_state_with_parent_session(
        uuid::Uuid::new_v4(),
    )));
    let valid = json!({"updateId": uuid::Uuid::new_v4().to_string(), "runtimeId": "runtime", "expectedRevision": "revision", "rules": []});
    for rules in [
        Value::Null,
        json!({}),
        json!([["plugin", "tool", 1]]),
        json!([{"pluginId": "plugin", "toolId": "tool"}]),
        json!([{"pluginId": "plugin", "toolId": "tool", "targetIdle": 1, "force": true}]),
    ] {
        // Each malformed nested rule uses the same otherwise valid original request shape.
        // 每个畸形嵌套规则使用其他部分有效的同一原请求形状。
        let mut request = valid.clone();
        request["rules"] = rules;
        assert!(matches!(
            dispatch(
                state.clone(),
                "service_begin_plugin_prewarm_policy_update",
                &json!({"request": request})
            )
            .await,
            Err(WebuiServiceRpcError::InvalidParams(_))
        ));
    }
    let mut missing = valid;
    missing
        .as_object_mut()
        .expect("named request")
        .remove("rules");
    for (method, params) in [
        (
            "service_plugin_prewarm_policy_status",
            json!({"updateId": "ignored"}),
        ),
        ("service_plugin_prewarm_policy_status", json!([])),
        (
            "service_begin_plugin_prewarm_policy_update",
            json!({"request": missing}),
        ),
        (
            "service_begin_plugin_prewarm_policy_update",
            json!({"request": ["update", "runtime", "revision", []]}),
        ),
        ("service_resume_plugin_prewarm_policy_update", json!({})),
        (
            "service_cancel_plugin_prewarm_policy_update",
            json!({"updateId": "id", "force": true}),
        ),
        ("service_forget_plugin_prewarm_policy_update", json!(["id"])),
    ] {
        assert!(
            matches!(
                dispatch(state.clone(), method, &params).await,
                Err(WebuiServiceRpcError::InvalidParams(_))
            ),
            "{method} must reject ambiguous input"
        );
    }
}

/// Publish, recover and acknowledge the same durable rule request through actual top-level RPC dispatch.
/// 通过实际顶层 RPC 分发发布、恢复及确认同一持久规则请求。
#[tokio::test]
async fn plugin_prewarm_policy_rpc_preserves_original_receipt_without_execution() {
    // This real package throws on initialization, so success alone cannot hide accidental execution.
    // 此真实包在初始化时抛错，因此单凭成功不能掩盖意外执行。
    let fixture = Fixture::new();
    let initial = fixture
        .rpc("service_plugin_prewarm_policy_status", json!({}))
        .await
        .expect("read initial");
    assert_eq!(initial["rules"], json!([]));
    assert_eq!(initial["update"], Value::Null);
    assert!(
        !fixture
            .root
            .path()
            .join("state/plugin-runtime-settings.json")
            .exists()
    );
    // Persistent rules are desired declarations, not a request to execute an unavailable tool.
    // 持久规则为期望声明，并非执行不可用工具的请求。
    let request = json!({"updateId": uuid::Uuid::new_v4().to_string(), "runtimeId": initial["runtimeId"], "expectedRevision": initial["revision"], "rules": [{"pluginId": "control-plugin", "toolId": "control_tool", "targetIdle": 1}]});
    let receipt = fixture
        .rpc(
            "service_begin_plugin_prewarm_policy_update",
            json!({"request": request}),
        )
        .await
        .expect("publish");
    assert_eq!(receipt["phase"], "committed");
    assert_eq!(receipt["request"], request);
    assert_eq!(
        fixture
            .rpc(
                "service_begin_plugin_prewarm_policy_update",
                json!({"request": request})
            )
            .await
            .expect("same request"),
        receipt
    );
    let published = fixture
        .rpc("service_plugin_prewarm_policy_status", json!({}))
        .await
        .expect("published policy");
    assert_eq!(published["rules"], request["rules"]);
    assert_eq!(published["revision"], receipt["resultRevision"]);
    assert_eq!(published["update"], receipt);
    assert_eq!(
        fixture
            .runtime
            .formal_runtime_status()
            .expect("native status")
            .expect("attached")
            .resources
            .resident,
        0
    );
    assert_browser_policy_wire_fixture(&initial, &published);
    for method in [
        "service_resume_plugin_prewarm_policy_update",
        "service_cancel_plugin_prewarm_policy_update",
        "service_forget_plugin_prewarm_policy_update",
    ] {
        assert!(
            fixture
                .rpc(
                    method,
                    json!({"updateId": uuid::Uuid::new_v4().to_string()})
                )
                .await
                .is_err(),
            "{method} must retain exact receipt identity"
        );
    }
    assert_eq!(
        fixture
            .rpc(
                "service_resume_plugin_prewarm_policy_update",
                json!({"updateId": request["updateId"]})
            )
            .await
            .expect("resume original"),
        receipt
    );
    assert_eq!(
        fixture
            .rpc(
                "service_cancel_plugin_prewarm_policy_update",
                json!({"updateId": request["updateId"]})
            )
            .await
            .expect("terminal cancel observes original"),
        receipt
    );
    assert_eq!(
        fixture
            .rpc(
                "service_forget_plugin_prewarm_policy_update",
                json!({"updateId": request["updateId"]})
            )
            .await
            .expect("acknowledge"),
        Value::Null
    );
    let acknowledged = fixture
        .rpc("service_plugin_prewarm_policy_status", json!({}))
        .await
        .expect("acknowledged policy");
    assert_eq!(acknowledged["rules"], published["rules"]);
    assert_eq!(acknowledged["revision"], published["revision"]);
    assert_eq!(acknowledged["update"], Value::Null);
    assert!(
        fixture
            .rpc(
                "service_begin_plugin_prewarm_policy_update",
                json!({"request": request})
            )
            .await
            .is_err()
    );
    fixture.close().await;
}

/// Compare actual initial and published RPC values with the browser artifact; only observed opaque IDs change.
/// 将实际 initial 及 published RPC 值与浏览器产物比较；仅替换已观测不透明身份。
fn assert_browser_policy_wire_fixture(initial: &Value, published: &Value) {
    // UUID examples preserve the wire contract while removing per-run identity noise.
    // UUID 示例保留线协议契约，同时去除每轮身份差异。
    let tokens = [
        (
            initial["runtimeId"].as_str().expect("runtime"),
            "runtime-fixture",
        ),
        (
            initial["revision"].as_str().expect("before"),
            "00000000-0000-4000-8000-000000000001",
        ),
        (
            published["revision"].as_str().expect("after"),
            "00000000-0000-4000-8000-000000000002",
        ),
        (
            published["update"]["request"]["updateId"]
                .as_str()
                .expect("update"),
            "00000000-0000-4000-8000-000000000003",
        ),
    ];
    let mut actual = json!({"initial": initial, "published": published});
    normalize_wire_tokens(&mut actual, &tokens);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../apps/vulcan-code-web/src/service/__fixtures__/plugin-prewarm-policy-wire.json",
    );
    if std::env::var("VULCAN_UPDATE_PLUGIN_PREWARM_POLICY_WIRE_FIXTURE").as_deref() == Ok("1") {
        std::fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&actual).expect("wire JSON")
            ),
        )
        .expect("explicit fixture regeneration");
    }
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("browser wire fixture; regenerate this test with VULCAN_UPDATE_PLUGIN_PREWARM_POLICY_WIRE_FIXTURE=1")).expect("wire fixture JSON");
    assert_eq!(
        actual, expected,
        "RPC contract changed; inspect the browser decoder before explicitly regenerating its fixture"
    );
}
