//! Synthetic transport and lifecycle tests for native OpenDesign authorization.
//! 原生 OpenDesign 授权的合成传输与生命周期测试。

use super::*;
use crate::open_design::test_server;
use serde_json::{Value, json};

/// Builds a future challenge with synthetic identity; `lifetime_ms` controls server expiry.
/// 使用合成身份构建未来挑战；`lifetime_ms` 控制服务端过期时间。
/// Returns protocol JSON without real account or device material.
/// 返回不含真实账户或设备材料的协议 JSON。
fn challenge(lifetime_ms: i64) -> Value {
    json!({
        "deviceId":"synthetic-device", "deviceSecret":"private-device-proof", "userCode":"ABC123",
        "activationUrl":"https://open-design.ai/cloud/cli/activate?deviceId=synthetic-device&userCode=ABC123",
        "expiresAt":(OffsetDateTime::now_utc() + time::Duration::milliseconds(lifetime_ms)).format(&Rfc3339).unwrap(),
        "pollIntervalSeconds":1, "status":"pending"
    })
}

/// Returns an approved synthetic pair with the official control and runtime destinations.
/// 返回带官方控制及运行目标的获批合成凭据对。
fn approved() -> Value {
    json!({"status":"approved", "controlKey":"private-control-key", "runtimeKey":"private-runtime-key",
        "apiUrl":CONTROL_ORIGIN, "linkUrl":LINK_ORIGIN, "user":{"id":"synthetic-user","email":"test@example.invalid"}})
}

/// Waits for one terminal state; `runtime` and `id` identify the transaction under test.
/// 等待一个终态；`runtime` 与 `id` 标识被测事务。
/// Returns its snapshot or fails the test after a bounded wait.
/// 返回其快照，或在有界等待后使测试失败。
async fn terminal(runtime: &OAuthAuthorizationRuntime, id: &str) -> OAuthTransactionSnapshot {
    timeout(Duration::from_secs(5), async {
        loop {
            // snapshot is refreshed instead of treating the original challenge as live state.
            // snapshot 会被刷新，不把初始挑战当作实时状态。
            let snapshot = runtime.status(id).await.unwrap();
            if matches!(
                snapshot.phase,
                OAuthTransactionPhase::Succeeded
                    | OAuthTransactionPhase::Failed
                    | OAuthTransactionPhase::Cancelled
            ) {
                return snapshot;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("device flow must terminate")
}

/// Verifies request shape, pending polling, secret separation, and single-use consumption.
/// 验证请求形状、等待轮询、秘密分离及单次消费。
#[tokio::test]
async fn open_design_device_flow_keeps_credential_roles_separate() {
    // replies reproduce the official challenge, waiting state, and approved token envelope.
    // replies 复现官方挑战、等待状态及获批令牌封装。
    let (origin, server) = test_server(vec![
        (201, challenge(60000).to_string(), Duration::ZERO),
        (
            200,
            json!({"status":"pending", "controlKey":null, "runtimeKey":null, "user":null})
                .to_string(),
            Duration::ZERO,
        ),
        (200, approved().to_string(), Duration::ZERO),
    ])
    .await;
    let runtime = OAuthAuthorizationRuntime::new(Client::new());
    let challenge = runtime
        .start_open_design_at(String::from("profile-binding"), &origin)
        .await
        .unwrap();
    let public_json = serde_json::to_string(&challenge).unwrap();
    assert!(!public_json.contains("private-device-proof"));
    assert_eq!(challenge.flow_kind, OAuthAuthorizationFlowKind::DeviceCode);
    assert_eq!(
        challenge.transaction.authorization_binding,
        "profile-binding"
    );
    assert_eq!(
        terminal(&runtime, &challenge.transaction.transaction_id)
            .await
            .phase,
        OAuthTransactionPhase::Succeeded
    );
    let material = runtime
        .take_succeeded(&challenge.transaction.transaction_id)
        .await
        .unwrap();
    let envelope = CredentialSecretEnvelope::new(material).unwrap();
    assert_eq!(envelope.credential_mode(), crate::CredentialMode::Auth);
    assert!(!envelope.is_renewable_oauth());
    assert!(!envelope.requires_refresh(u64::MAX, 1000));
    let execution = envelope.execution_secret().unwrap();
    assert_eq!(execution.expose_value(), "private-runtime-key");
    assert_eq!(
        execution.open_design_control_key().unwrap(),
        "private-control-key"
    );
    assert!(execution.account_id().is_none());
    assert!(
        runtime
            .take_succeeded(&challenge.transaction.transaction_id)
            .await
            .is_err()
    );
    let requests = server.await.unwrap();
    let create = requests
        .iter()
        .find(|request| request.starts_with(&format!("POST {DEVICE_PATH} HTTP/1.1")))
        .unwrap();
    let create_body: Value =
        serde_json::from_str(create.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        create_body,
        json!({"cliVersion":env!("CARGO_PKG_VERSION"),"host":"Vulcan Code","profile":"prod"})
    );
    let polls: Vec<_> = requests
        .iter()
        .filter(|request| {
            request.starts_with(&format!(
                "POST {DEVICE_PATH}/synthetic-device/token HTTP/1.1"
            ))
        })
        .collect();
    assert_eq!(polls.len(), 2);
    for request in polls {
        assert_eq!(
            serde_json::from_str::<Value>(request.split_once("\r\n\r\n").unwrap().1).unwrap(),
            json!({"deviceSecret":"private-device-proof"})
        );
    }
}

/// Verifies untrusted activation targets, mismatched identity, and invalid lifetimes fail closed.
/// 验证不可信激活目标、身份不匹配及无效生命周期均失败关闭。
#[test]
fn open_design_rejects_invalid_device_challenges() {
    for (field, value) in [
        (
            "activationUrl",
            json!(
                "https://evil.example/cloud/cli/activate?deviceId=synthetic-device&userCode=ABC123"
            ),
        ),
        (
            "activationUrl",
            json!("https://open-design.ai/cloud/cli/activate?deviceId=other&userCode=ABC123"),
        ),
        ("deviceId", json!("../wrong")),
        ("deviceSecret", json!("")),
        ("status", json!("approved")),
        ("expiresAt", json!("2020-01-01T00:00:00Z")),
        ("pollIntervalSeconds", json!(0)),
        ("pollIntervalSeconds", json!(u64::MAX)),
    ] {
        // invalid changes exactly one field of an otherwise valid challenge.
        // invalid 仅改变原本有效挑战中的一个字段。
        let mut invalid = challenge(60000);
        invalid[field] = value;
        assert!(
            validate_challenge(
                &serde_json::from_value(invalid).unwrap(),
                unix_millis().unwrap()
            )
            .is_err(),
            "invalid {field} must fail"
        );
    }
}

/// Verifies cancellation cannot publish late credentials and server expiry terminates waiting.
/// 验证取消不能发布迟到凭据，服务端过期会终止等待。
#[tokio::test]
async fn open_design_cancel_and_expiration_are_terminal() {
    // A delayed success simulates browser approval arriving after cancellation.
    // 延迟成功模拟取消之后才到达的浏览器批准。
    let (origin, server) = test_server(vec![
        (201, challenge(60000).to_string(), Duration::ZERO),
        (200, approved().to_string(), Duration::from_millis(100)),
    ])
    .await;
    let runtime = OAuthAuthorizationRuntime::new(Client::new());
    let challenge = runtime
        .start_open_design_at(String::from("cancel-binding"), &origin)
        .await
        .unwrap();
    sleep(Duration::from_millis(20)).await;
    runtime
        .cancel(&challenge.transaction.transaction_id)
        .await
        .unwrap();
    sleep(Duration::from_millis(150)).await;
    assert_eq!(
        runtime
            .status(&challenge.transaction.transaction_id)
            .await
            .unwrap()
            .phase,
        OAuthTransactionPhase::Cancelled
    );
    assert!(
        runtime
            .take_succeeded(&challenge.transaction.transaction_id)
            .await
            .is_err()
    );
    server.abort();
    // A short server expiry bounds even a pending response with a longer polling interval.
    // 较短的服务端过期时间也能约束轮询间隔较长的等待响应。
    let (origin, server) = test_server(vec![
        (201, self::challenge(200).to_string(), Duration::ZERO),
        (200, json!({"status":"pending"}).to_string(), Duration::ZERO),
    ])
    .await;
    let challenge = runtime
        .start_open_design_at(String::from("expiry-binding"), &origin)
        .await
        .unwrap();
    assert_eq!(
        terminal(&runtime, &challenge.transaction.transaction_id)
            .await
            .phase,
        OAuthTransactionPhase::Failed
    );
    server.await.unwrap();
}

/// Verifies unknown states and credential redirection fail without echoing secret bodies.
/// 验证未知状态及凭据目标重定向失败，且不回显秘密正文。
#[tokio::test]
async fn open_design_rejects_untrusted_token_responses() {
    // wrong_origin retains valid credentials to ensure endpoint validation runs before acceptance.
    // wrong_origin 保留有效凭据，确保接纳前先校验端点。
    let mut wrong_origin = approved();
    wrong_origin["apiUrl"] = json!("https://evil.example");
    for (status, body) in [
        (200, wrong_origin.to_string()),
        (200, json!({"status":"private-control-key"}).to_string()),
        (200, json!({"status":"expired"}).to_string()),
        (401, approved().to_string()),
        (302, approved().to_string()),
    ] {
        // Each transport is isolated, so a failed flow cannot reuse another device transaction.
        // 每次传输均隔离，失败流程不能复用其他设备事务。
        let (origin, server) = test_server(vec![(status, body, Duration::ZERO)]).await;
        let error = poll(
            &crate::open_design::client().unwrap(),
            &origin,
            "private-device-proof",
            Duration::from_secs(1),
        )
        .await
        .err()
        .unwrap();
        assert!(!error.contains("private-control-key") && !error.contains("private-runtime-key"));
        assert_eq!(server.await.unwrap().len(), 1);
    }
}
