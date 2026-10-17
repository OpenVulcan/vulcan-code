//! Behavioral coverage for exact Console amounts, nullable subscriptions, and authenticated reads.
//! 精确 Console 金额、可空订阅及已鉴权读取的行为覆盖。

use super::*;
use crate::open_design::test_server;
use crate::{CredentialSecretEnvelope, CredentialSecretMaterial, OAuthTokenSet};
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Duration};

/// Creates a synthetic workspace credential; no parameters, returns validated execution material.
/// 创建合成工作区凭据；无参数，返回已验证执行材料。
fn secret() -> CredentialExecutionSecret {
    CredentialSecretEnvelope::new(CredentialSecretMaterial::OpencodeConsoleOAuth {
        tokens: OAuthTokenSet {
            access_token: String::from("synthetic-access"),
            refresh_token: String::from("synthetic-refresh"),
            issued_at_unix_ms: 1,
            expires_at_unix_ms: 2,
            scopes: BTreeSet::new(),
            account_subject: Some(String::from("user_demo")),
            email: None,
        },
        organization_id: String::from("org_demo"),
    })
    .unwrap()
    .execution_secret()
    .unwrap()
}

/// Creates the production subscription response shape; returns all three named windows.
/// 创建生产订阅响应结构；返回全部三个具名窗口。
fn subscription() -> Value {
    json!({"product":"go-plus","access":{"meters":{
        "fiveHour":{"startsAt":null,"resetsAt":null,"limitMicroCents":"9007199254740993","usedMicroCents":"1"},
        "week":{"startsAt":"2026-10-12T00:00:00.000Z","resetsAt":"2026-10-19T00:00:00.000Z","limitMicroCents":"3000000000","usedMicroCents":"3000000001"},
        "month":{"resetsAt":"2026-11-17T00:00:00.000Z","limitMicroCents":"6000000000","usedMicroCents":"125000000"}
    }}})
}

/// Verifies integers beyond binary floating-point precision and independent negative balances.
/// 验证超出二进制浮点精度的整数及独立负余额。
#[test]
fn opencode_go_amounts_and_windows_remain_exact() {
    // snapshot includes an unused window, an over-limit window, and an active monthly window.
    // snapshot 包含未使用窗口、超限窗口及有效月窗口。
    let snapshot = normalize(
        Some(serde_json::from_value(subscription()).unwrap()),
        Some(Balance {
            balance_micro_cents: String::from("-100000001"),
        }),
    )
    .unwrap();
    assert_eq!(snapshot.plan_id.as_deref(), Some("go-plus"));
    assert_eq!(snapshot.balance_amount.as_deref(), Some("-1.00000001"));
    let five = snapshot
        .quota_windows
        .iter()
        .find(|window| window.id == "five-hour")
        .expect("expected five-hour window identity may be outdated");
    assert_eq!(five.total.as_deref(), Some("90071992.54740993"));
    assert_eq!(five.remaining.as_deref(), Some("90071992.54740992"));
    assert_eq!(five.starts_at, None);
    let week = snapshot
        .quota_windows
        .iter()
        .find(|window| window.id == "week")
        .expect("expected weekly window identity may be outdated");
    assert_eq!(week.remaining.as_deref(), Some("0"));
}

/// Retains absent and expired subscription states without fabricating full quota windows.
/// 保留缺失与过期订阅状态，不虚构完整额度窗口。
#[test]
fn opencode_go_inactive_access_does_not_become_unused_quota() {
    // A null response has no plan; an expired paid subscription retains its product identity.
    // null 响应没有套餐；过期付费订阅保留产品身份。
    let absent = normalize(
        None,
        Some(Balance {
            balance_micro_cents: String::from("0"),
        }),
    )
    .unwrap();
    assert!(absent.quota_windows.is_empty());
    assert_eq!(absent.plan_id, None);
    let expired = normalize(
        Some(serde_json::from_value(json!({"product":"go","access":null})).unwrap()),
        Some(Balance {
            balance_micro_cents: String::from("0"),
        }),
    )
    .unwrap();
    assert!(expired.quota_windows.is_empty());
    assert_eq!(expired.plan_id.as_deref(), Some("go"));
}

/// Rejects alternate numeric encodings, invalid timestamps, and unsupported response field aliases.
/// 拒绝候选数字编码、无效时间戳与未经支持的响应字段别名。
#[test]
fn opencode_go_invalid_wire_shapes_fail_explicitly() {
    // Every mutation changes a field guaranteed by the published Console schema.
    // 每次变更仅修改 Console 公布 Schema 保证的字段。
    let mut value = subscription();
    value["access"]["meters"]["fiveHour"]["limitMicroCents"] = json!(123);
    assert!(serde_json::from_value::<Subscription>(value).is_err());
    // Required timestamps cannot disappear or become null outside the five-hour window.
    // 必填时间戳不得消失，且五小时窗口之外不得变为 null。
    let mut value = subscription();
    value["access"]["meters"]["fiveHour"]
        .as_object_mut()
        .unwrap()
        .remove("resetsAt");
    assert!(serde_json::from_value::<Subscription>(value).is_err());
    // Weekly reset is non-null in the authoritative response schema.
    // 权威响应 Schema 中每周重置时间不可为 null。
    let mut value = subscription();
    value["access"]["meters"]["week"]["resetsAt"] = Value::Null;
    assert!(serde_json::from_value::<Subscription>(value).is_err());
    let mut value = subscription();
    value["access"]["meters"]["month"]["usedMicroCents"] = json!("-1");
    assert!(
        normalize(
            Some(serde_json::from_value(value).unwrap()),
            Some(Balance {
                balance_micro_cents: String::from("0")
            })
        )
        .is_err()
    );
    let mut value = subscription();
    value["access"]["meters"]["week"]["resetsAt"] = json!("invalid");
    assert!(
        normalize(
            Some(serde_json::from_value(value).unwrap()),
            Some(Balance {
                balance_micro_cents: String::from("0")
            })
        )
        .is_err()
    );
    assert!(serde_json::from_value::<Subscription>(json!({"plan":"go","rollingUsage":0})).is_err());
    assert!(serde_json::from_value::<Subscription>(json!({"product":"go"})).is_err());
}

/// Verifies both production paths receive the same bearer token and billing workspace.
/// 验证两个生产路径接收同一 Bearer 令牌与计费工作区。
#[tokio::test]
async fn opencode_go_queries_use_exact_authenticated_scope() {
    // replies preserve the actual subscription and independent balance contracts.
    // replies 保留真实订阅与独立余额契约。
    let (origin, server) = test_server(vec![
        (200, subscription().to_string(), Duration::ZERO),
        (
            200,
            json!({"balanceMicroCents":"123456789"}).to_string(),
            Duration::ZERO,
        ),
    ])
    .await;
    let snapshot = query_at(&opencode::client().unwrap(), &origin, &secret())
        .await
        .unwrap();
    assert_eq!(snapshot.balance_amount.as_deref(), Some("1.23456789"));
    let requests = server.await.unwrap();
    assert!(
        requests
            .iter()
            .any(|request| request.starts_with("GET /api/go/status "))
    );
    assert!(
        requests
            .iter()
            .any(|request| request.starts_with("GET /api/billing/status "))
    );
    for request in requests {
        assert!(request.contains("authorization: Bearer synthetic-access\r\n"));
        assert!(request.contains("x-org-id: org_demo\r\n"));
        assert!(request.contains(&format!(
            "user-agent: {}\r\n",
            crate::execution::VULCAN_CODE_USER_AGENT
        )));
        assert!(!request.contains("chatgpt-account-id"));
    }
}

/// Queries only Go status for a service key, with no organization override or fabricated balance.
/// 服务密钥只查询 Go 状态，不覆盖组织或虚构余额。
/// No parameters; verifies the normalized windows and the single captured HTTP request.
/// 无参数；验证归一化窗口及捕获到的唯一 HTTP 请求。
#[tokio::test]
async fn opencode_go_api_key_reads_only_subscription() {
    // A second HTTP request would fail because the synthetic server serves exactly one reply.
    // 合成服务只提供一个响应，第二次 HTTP 请求会失败。
    let (origin, server) =
        test_server(vec![(200, subscription().to_string(), Duration::ZERO)]).await;
    // key uses synthetic service-account material with no OAuth workspace identity.
    // key 使用不携带 OAuth 工作区身份的合成服务账号材料。
    let key = CredentialSecretEnvelope::new_api_key(String::from("oc_sk_synthetic"))
        .unwrap()
        .execution_secret()
        .unwrap();
    // snapshot must retain all plan windows even though the balance endpoint is never queried.
    // snapshot 必须保留全部套餐窗口，即使从未查询余额端点。
    let snapshot = query_at(&opencode::client().unwrap(), &origin, &key)
        .await
        .unwrap();
    assert_eq!(snapshot.availability, AccountUsageAvailability::Supported);
    assert_eq!(snapshot.balance_amount, None);
    assert_eq!(snapshot.quota_windows.len(), 3);
    // requests verifies the actual wire boundary rather than merely the credential-selection branch.
    // requests 验证真实线协议边界，而非仅验证凭据选择分支。
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 1);
    for request in requests {
        assert!(request.starts_with("GET /api/go/status "));
        assert!(request.contains("authorization: Bearer oc_sk_synthetic\r\n"));
        assert!(request.contains(&format!(
            "user-agent: {}\r\n",
            crate::execution::VULCAN_CODE_USER_AGENT
        )));
        assert!(!request.contains("x-org-id:"));
        assert!(!request.contains("chatgpt-account-id:"));
    }
}

/// Retains inactive API-key subscription states without querying or inventing a credit balance.
/// 保留 API Key 的无效订阅状态，不查询或虚构余额。
/// No parameters; verifies both absent subscriptions and expired paid access through HTTP.
/// 无参数；通过 HTTP 验证缺失订阅与付费访问过期两种情况。
#[tokio::test]
async fn opencode_go_api_key_inactive_access_has_no_balance() {
    for body in [Value::Null, json!({"product":"go","access":null})] {
        // server supplies one authoritative inactive status, never a billing response.
        // server 仅提供一个权威无效状态，绝不提供计费响应。
        let (origin, server) = test_server(vec![(200, body.to_string(), Duration::ZERO)]).await;
        // key is independent from renewable account identity and uses the normal API-key projection.
        // key 独立于可续期账号身份，使用普通 API Key 投影。
        let key = CredentialSecretEnvelope::new_api_key(String::from("oc_sk_synthetic"))
            .unwrap()
            .execution_secret()
            .unwrap();
        // snapshot reports the supported inactive state without generating full unused windows.
        // snapshot 报告受支持的无效状态，不生成全额未使用窗口。
        let snapshot = query_at(&opencode::client().unwrap(), &origin, &key)
            .await
            .unwrap();
        assert_eq!(snapshot.availability, AccountUsageAvailability::Supported);
        assert!(snapshot.quota_windows.is_empty());
        assert_eq!(snapshot.balance_amount, None);
        assert_eq!(server.await.unwrap().len(), 1);
    }
}

/// Keeps OAuth billing errors explicit after a successful subscription response.
/// 在订阅响应成功后保持 OAuth 余额错误显式可见。
/// No parameters; verifies HTTP 403 does not become a successful partial OAuth snapshot.
/// 无参数；验证 HTTP 403 不会变成成功的 OAuth 部分快照。
#[tokio::test]
async fn opencode_go_oauth_balance_failure_remains_an_error() {
    // server returns a valid plan followed by an inaccessible independent balance.
    // server 先返回有效套餐，再返回不可访问的独立余额。
    let (origin, server) = test_server(vec![
        (200, subscription().to_string(), Duration::ZERO),
        (403, String::from("private-billing-error"), Duration::ZERO),
    ])
    .await;
    // error must retain the status without disclosing upstream bodies or silently omitting balance.
    // error 必须保留状态，不泄露上游正文，也不静默省略余额。
    let error = query_at(&opencode::client().unwrap(), &origin, &secret())
        .await
        .unwrap_err();
    assert!(error.contains("403"));
    assert!(!error.contains("private-billing-error"));
    assert_eq!(server.await.unwrap().len(), 2);
}

/// Prevents redirects and private error-body disclosure for both supported credential modes.
/// 对两种受支持凭据模式阻止重定向及私有错误正文泄露。
#[tokio::test]
async fn opencode_go_transport_failures_do_not_return_partial_usage() {
    // key exercises the same transport protections without adding a billing request.
    // key 在不增加余额请求的情况下验证相同传输保护。
    let key = CredentialSecretEnvelope::new_api_key(String::from("oc_sk_synthetic"))
        .unwrap()
        .execution_secret()
        .unwrap();
    for (status, credential) in [302, 401, 403]
        .into_iter()
        .flat_map(|status| [(status, secret()), (status, key.clone())])
    {
        // Each mock is an isolated endpoint with a deliberately sensitive synthetic error body.
        // 每个模拟服务都是独立端点，携带故意敏感的合成错误正文。
        let (origin, server) = test_server(vec![(
            status,
            String::from("private-error-body"),
            Duration::ZERO,
        )])
        .await;
        let error = query_at(&opencode::client().unwrap(), &origin, &credential)
            .await
            .unwrap_err();
        assert!(!error.contains("private-error-body"));
        assert!(error.contains(&status.to_string()));
        server.await.unwrap();
    }
}
