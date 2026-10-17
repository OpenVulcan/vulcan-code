//! Synthetic device-grant, cancellation, workspace, and refresh regression coverage.
//! 合成设备授权、取消、工作区与刷新回归覆盖。

use super::*;
use crate::open_design::test_server;
use serde_json::{Value, json};

/// Creates a synthetic device response; no parameters, returns the exact upstream shape.
/// 创建合成设备响应；无参数，返回精确上游结构。
fn challenge() -> Value {
    json!({"device_code":"private-device", "user_code":"PUBLIC-CODE", "verification_uri_complete":"verify?user_code=PUBLIC-CODE", "expires_in":60, "interval":1})
}

/// Creates a synthetic successful exchange; `org` selects its explicit workspace.
/// 创建合成成功交换；`org` 选择其明确工作区。
/// Returns no real account material.
/// 不返回真实账号材料。
fn token(org: &str) -> Value {
    json!({"access_token":"synthetic-access", "refresh_token":"synthetic-refresh", "expires_in":3600, "org_id":org})
}

/// Waits for one bounded terminal transaction; `runtime` and `id` identify the attempt.
/// 等待单个有界终态事务；`runtime` 与 `id` 标识尝试。
/// Returns its final snapshot or fails after five seconds.
/// 返回最终快照，或在五秒后使测试失败。
async fn terminal(runtime: &OAuthAuthorizationRuntime, id: &str) -> OAuthTransactionSnapshot {
    timeout(Duration::from_secs(5), async {
        loop {
            // snapshot is re-read by stable identity rather than inferred from the initial challenge.
            // snapshot 按稳定身份重新读取，不从初始挑战推断。
            let snapshot = runtime.status(id).await.unwrap();
            if snapshot.phase != OAuthTransactionPhase::WaitingForUserAuthorization {
                return snapshot;
            }
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("device attempt must terminate")
}

/// Verifies pending polling, workspace binding, private-proof separation, and single-use completion.
/// 验证等待轮询、工作区绑定、私有证明分离及单次完成消费。
#[tokio::test]
async fn opencode_device_authorization_preserves_workspace_and_identity() {
    // origin serves only the exact four protocol operations, including one pending response.
    // origin 仅提供精确协议操作，包含一次等待响应。
    let (origin, server) = test_server(vec![
        (200, challenge().to_string(), Duration::ZERO),
        (
            400,
            json!({"error":"authorization_pending"}).to_string(),
            Duration::ZERO,
        ),
        (200, token("org_demo").to_string(), Duration::ZERO),
        (
            200,
            json!({"id":"user_demo","email":"user@example.invalid"}).to_string(),
            Duration::ZERO,
        ),
        (
            200,
            json!([{"id":"org_other"},{"id":"org_demo"}]).to_string(),
            Duration::ZERO,
        ),
    ])
    .await;
    // runtime uses the production transaction and cancellation machinery.
    // runtime 使用生产事务与取消机制。
    let runtime = OAuthAuthorizationRuntime::new(Client::new());
    // challenge contains only the public browser instructions for this transaction.
    // challenge 只包含此事务的公开浏览器操作指引。
    let challenge = runtime
        .start_opencode_at(String::from("exact-profile"), &origin)
        .await
        .unwrap();
    // public is the exact serialized challenge exposed to clients.
    // public 是向客户端公开的精确序列化挑战。
    let public = serde_json::to_string(&challenge).unwrap();
    assert!(!public.contains("private-device"));
    assert_eq!(challenge.flow_kind, OAuthAuthorizationFlowKind::DeviceCode);
    assert_eq!(challenge.transaction.authorization_binding, "exact-profile");
    assert_eq!(
        terminal(&runtime, &challenge.transaction.transaction_id)
            .await
            .phase,
        OAuthTransactionPhase::Succeeded
    );
    // material is consumed only through the shared successful-transaction boundary.
    // material 仅经共享成功事务边界消费。
    let material = runtime
        .take_succeeded(&challenge.transaction.transaction_id)
        .await
        .unwrap();
    let CredentialSecretMaterial::OpencodeConsoleOAuth {
        tokens,
        organization_id,
    } = material
    else {
        panic!("expected Console material")
    };
    assert_eq!(organization_id, "org_demo");
    assert_eq!(tokens.account_subject.as_deref(), Some("user_demo"));
    assert!(
        runtime
            .take_succeeded(&challenge.transaction.transaction_id)
            .await
            .is_err()
    );
    // Requests are located by protocol identity, never by an evolving array position.
    // 按协议身份定位请求，绝不依赖会演进的数组位置。
    let requests = server.await.unwrap();
    // authorize locates the device challenge operation by its stable HTTP identity.
    // authorize 按稳定 HTTP 身份定位设备挑战操作。
    let authorize = requests
        .iter()
        .find(|request| request.starts_with("POST /auth/device/code "))
        .expect("expected device-code endpoint may be outdated");
    // body verifies the application identifies itself throughout the device protocol.
    // body 验证应用在整个设备协议中使用自有身份。
    let body: Value = serde_json::from_str(authorize.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["client_id"], CLIENT_ID);
    assert_eq!(body["supports_org_scope"], true);
    for request in &requests {
        assert!(request.contains(&format!(
            "user-agent: {}\r\n",
            crate::execution::VULCAN_CODE_USER_AGENT
        )));
        if request.starts_with("POST /auth/device/token ") {
            // body is the private synthetic token poll, which uses the identical public client ID.
            // body 是私有合成令牌轮询，使用相同的公开客户端 ID。
            let body: Value =
                serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body["client_id"], CLIENT_ID);
        }
    }
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST /auth/device/token "))
            .count(),
        2
    );
}

/// Verifies cancellation aborts the private polling task and never yields credentials.
/// 验证取消会中止私有轮询任务，且绝不产出凭据。
#[tokio::test]
async fn opencode_device_cancellation_cannot_complete_later() {
    // The mock remains ready for a token request that cancellation must prevent.
    // 模拟服务等待令牌请求，而取消必须阻止该请求。
    let (origin, server) = test_server(vec![
        (200, challenge().to_string(), Duration::ZERO),
        (200, token("org_demo").to_string(), Duration::ZERO),
    ])
    .await;
    let runtime = OAuthAuthorizationRuntime::new(Client::new());
    let challenge = runtime
        .start_opencode_at(String::from("cancel-profile"), &origin)
        .await
        .unwrap();
    runtime
        .cancel(&challenge.transaction.transaction_id)
        .await
        .unwrap();
    sleep(Duration::from_millis(20)).await;
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
}

/// Rejects ambiguous organizations when the authority explicitly omits workspace selection.
/// 在授权端明确省略工作区选择时拒绝有歧义的组织。
#[tokio::test]
async fn opencode_requires_unambiguous_workspace_selection() {
    // Both workspaces are real members in the synthetic protocol; neither is an implicit default.
    // 两个工作区均为合成协议中的真实成员；任何一个都不是隐式默认值。
    let (origin, server) = test_server(vec![
        (
            200,
            json!({"id":"user_demo","email":"user@example.invalid"}).to_string(),
            Duration::ZERO,
        ),
        (
            200,
            json!([{"id":"org_a"},{"id":"org_b"}]).to_string(),
            Duration::ZERO,
        ),
    ])
    .await;
    let mut response: Token = serde_json::from_value(token("org_a")).unwrap();
    response.org_id = None;
    assert!(
        material(&crate::opencode::client().unwrap(), &origin, response)
            .await
            .is_err()
    );
    server.await.unwrap();
}

/// Verifies refresh rotation retains identity and rejects a workspace change.
/// 验证刷新轮换保留身份并拒绝工作区变更。
#[tokio::test]
async fn opencode_refresh_rotates_tokens_without_switching_workspaces() {
    // Existing identity is retained across both server responses.
    // 既有身份在两份服务端响应间保持不变。
    let tokens = token_set(
        &serde_json::from_value(token("org_demo")).unwrap(),
        Some(String::from("user_demo")),
        Some(String::from("user@example.invalid")),
    )
    .unwrap();
    // rotated changes both private values so the test proves actual token replacement.
    // rotated 更改两个私有值，使测试证明实际令牌替换。
    let mut rotated = token("org_demo");
    rotated["access_token"] = json!("rotated-access");
    rotated["refresh_token"] = json!("rotated-refresh");
    // origin returns a valid rotation followed by a forbidden workspace change.
    // origin 返回有效轮换，随后返回禁止的工作区变更。
    let (origin, server) = test_server(vec![
        (200, rotated.to_string(), Duration::ZERO),
        (200, token("org_changed").to_string(), Duration::ZERO),
    ])
    .await;
    let http = crate::opencode::client().unwrap();
    let renewed = refresh_at(&http, &origin, &tokens, "org_demo")
        .await
        .unwrap();
    // renewed_tokens must replace both token values while retaining the original user and workspace.
    // renewed_tokens 必须替换两个令牌值，同时保留原用户与工作区。
    let CredentialSecretMaterial::OpencodeConsoleOAuth {
        tokens: renewed_tokens,
        organization_id,
    } = renewed
    else {
        panic!("expected Console material")
    };
    assert_eq!(organization_id, "org_demo");
    assert_eq!(renewed_tokens.access_token, "rotated-access");
    assert_eq!(renewed_tokens.refresh_token, "rotated-refresh");
    assert_eq!(renewed_tokens.account_subject, tokens.account_subject);
    assert_eq!(renewed_tokens.email, tokens.email);
    assert!(
        refresh_at(&http, &origin, &tokens, "org_demo")
            .await
            .is_err()
    );
    let requests = server.await.unwrap();
    for request in requests {
        // body is synthetic request JSON and cannot contain a real refresh token.
        // body 是合成请求 JSON，不可能包含真实刷新令牌。
        let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["grant_type"], "refresh_token");
        assert_eq!(body["refresh_token"], tokens.refresh_token);
        assert_eq!(body["client_id"], CLIENT_ID);
    }
}

/// Rejects invalid lifetimes before arithmetic and rejects a cross-origin browser URL.
/// 在算术运算前拒绝无效有效期，并拒绝跨来源浏览器地址。
#[tokio::test]
async fn opencode_rejects_untrusted_navigation_and_invalid_token_lifetimes() {
    // value reproduces a valid token before deliberately changing only its lifetime.
    // value 先复现有效令牌，再仅故意更改其有效期。
    let mut value: Token = serde_json::from_value(token("org_demo")).unwrap();
    value.expires_in = 0;
    assert!(token_set(&value, None, None).is_err());
    value.expires_in = u64::MAX;
    assert!(token_set(&value, None, None).is_err());
    let mut response = challenge();
    response["verification_uri_complete"] = json!("https://example.invalid/steal");
    let (origin, server) = test_server(vec![(200, response.to_string(), Duration::ZERO)]).await;
    let runtime = OAuthAuthorizationRuntime::new(Client::new());
    assert!(
        runtime
            .start_opencode_at(String::from("profile"), &origin)
            .await
            .is_err()
    );
    server.await.unwrap();
}
