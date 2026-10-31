//! NODEAI wallet projection, public endpoint authentication, and failure regressions.
//! NODEAI 钱包投影、公共端点鉴权及失败回归测试。

use super::*;
use crate::{
    CredentialConnection, CredentialMode, CredentialSecretEnvelope, ProviderMetadataClient,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// Verifies wallet money comes from balance and the API unit, not a key quota or UI symbol.
/// 验证钱包金额来自 balance 与 API 单位，而不是密钥限额或界面符号。
#[test]
fn wallet_balance_preserves_reported_amount_and_unit() {
    // snapshot deliberately receives a different key allowance to detect accidental field substitution.
    // snapshot 刻意接收不同的密钥额度，以识别错误字段替代。
    let snapshot = decode_nodeai_balance(
        r#"{"mode":"unrestricted","isValid":true,"balance":13.33928437,"unit":"CNY","remaining":999,"quota":{"limit":1000}}"#,
    ).unwrap();
    assert_eq!(snapshot.availability, AccountUsageAvailability::Supported);
    assert_eq!(snapshot.balance_amount.as_deref(), Some("13.33928437"));
    assert_eq!(snapshot.remaining_amount, snapshot.balance_amount);
    assert_eq!(snapshot.currency.as_deref(), Some("CNY"));
    assert!(snapshot.used_amount.is_none());
    assert!(snapshot.quota_windows.is_empty());
    assert!(snapshot.observed_at.is_some());
}

/// Verifies exhausted and overdrawn accounts retain their real wallet amount.
/// 验证耗尽和透支账号保留真实钱包金额。
#[test]
fn wallet_balance_keeps_zero_and_negative_amounts() {
    for amount in ["0", "-0.25"] {
        // body varies only the wallet amount while keeping the observed response contract.
        // body 仅改变钱包金额，同时保持已验证的响应契约。
        let body =
            format!(r#"{{"mode":"unrestricted","isValid":true,"balance":{amount},"unit":"USD"}}"#);
        assert_eq!(
            decode_nodeai_balance(&body)
                .unwrap()
                .balance_amount
                .as_deref(),
            Some(amount)
        );
    }
}

/// Verifies unknown, invalid, quota-only and subscription-only observations never manufacture money.
/// 验证未知、无效、纯限额和纯订阅观测不会伪造钱包金额。
#[test]
fn wallet_balance_rejects_incomplete_or_unusable_observations() {
    for body in [
        r#"{"mode":"unrestricted","isValid":false,"balance":12,"unit":"USD"}"#,
        r#"{"mode":"quota_limited","isValid":true,"remaining":12,"unit":"USD"}"#,
        r#"{"mode":"quota_limited","isValid":true,"balance":12,"unit":"USD"}"#,
        r#"{"mode":"unrestricted","isValid":true,"subscription":{},"unit":"USD"}"#,
        r#"{"mode":"unrestricted","isValid":true,"balance":null,"unit":"USD"}"#,
        r#"{"mode":"unrestricted","isValid":true,"balance":12,"unit":" "}"#,
        r#"{"mode":"unrestricted","isValid":true,"balance":12}"#,
        r#"{"mode":"unrestricted","isValid":true,"balance":"12","unit":"USD"}"#,
        r#"{"mode":"unrestricted","balance":12,"unit":"USD"}"#,
        r#"{"isValid":true,"balance":12,"unit":"USD"}"#,
        r#"{"mode":"future_mode","isValid":true,"balance":12,"unit":"USD"}"#,
    ] {
        assert!(
            decode_nodeai_balance(body).is_err(),
            "invalid wallet shape was accepted: {body}"
        );
    }
}

/// Calls the real metadata dispatcher against one local server and captures its single request.
/// 对本地服务器调用真实元数据分派器并捕获单次请求。
///
/// `entry` selects an inference group, `status` and `body` define the fixture response.
/// Returns the normalized account observation and captured HTTP headers; no real key is used.
/// `entry` 选择推理分组，`status` 与 `body` 定义夹具响应。
/// 返回规范化账号观测和捕获的 HTTP 请求头；不使用真实密钥。
async fn query_fixture(
    entry: &ServiceEntry,
    status: &str,
    body: &str,
) -> (AccountUsageSnapshot, String) {
    // listener binds an ephemeral loopback port so tests never contact the real provider.
    // listener 绑定回环临时端口，确保测试不访问真实供应商。
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    // entry preserves the actual profile and inference authentication while replacing only the origin.
    // entry 保留真实配置档与推理鉴权，仅替换服务源地址。
    let mut entry = entry.clone();
    entry.base_url = format!("http://{}/v1/", listener.local_addr().unwrap());
    // response is one bounded fixture reply and closes the socket after delivery.
    // response 为单个有界夹具响应，发送后关闭套接字。
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    // server returns the captured headers and cannot hang longer than the metadata request boundary.
    // server 返回捕获的请求头，等待不会超过元数据请求边界。
    let server = tokio::spawn(async move {
        tokio::time::timeout(PROVIDER_METADATA_REQUEST_TIMEOUT, async move {
            // stream is the single accepted loopback request.
            // stream 为唯一接受的回环请求。
            let (mut stream, _) = listener.accept().await.unwrap();
            // request accumulates headers only; the billing protocol carries no request body.
            // request 仅累积请求头；余额查询协议没有请求体。
            let mut request = Vec::new();
            // buffer limits each read while detecting the HTTP header terminator.
            // buffer 限定单次读取并用于检测 HTTP 请求头结束标记。
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                // count confirms the peer did not close before the complete request arrived.
                // count 确认对端未在完整请求到达前关闭。
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
            }
            stream.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8(request).unwrap()
        })
        .await
        .unwrap()
    });
    // profile follows the entry's declared wallet capability rather than a catalog array position.
    // profile 按入口声明的钱包能力定位，不依赖目录数组位置。
    let profile = entry
        .credential_profiles
        .iter()
        .find(|profile| profile.account_usage_query == crate::AccountUsageQueryKind::NodeAiBalance)
        .expect("NODEAI wallet declaration is missing; provider metadata may have changed.");
    // connection keeps credential identity separate from the shared provider balance.
    // connection 将凭据身份与供应商公共余额分离。
    let connection = CredentialConnection {
        id: String::from("wallet-connection"),
        provider_id: String::from("nodeai"),
        service_entry_id: entry.id.clone(),
        credential_profile_id: profile.id.clone(),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("wallet-credential"),
        display_name: String::from("Synthetic wallet"),
        secret_reference: String::from("fixture-secret"),
        management_secret_reference: None,
        service_origin_override: None,
    };
    // secret is synthetic and never comes from the user's environment.
    // secret 为合成密钥，绝不来自用户环境变量。
    let secret = CredentialSecretEnvelope::new_api_key(String::from("synthetic-nodeai-key"))
        .unwrap()
        .execution_secret()
        .unwrap();
    // snapshot exercises the registered dispatch and error projection, not just the JSON decoder.
    // snapshot 验证已注册分派及错误投影，而不仅是 JSON 解码器。
    let snapshot = crate::HttpProviderMetadataClient::new(reqwest::Client::new())
        .query_account_usage(&entry, &connection, &secret)
        .await;
    (snapshot, server.await.unwrap())
}

/// Verifies every NODEAI group shares the public wallet endpoint and Bearer key authentication.
/// 验证每个 NODEAI 分组共用公共钱包端点与 Bearer 密钥鉴权。
#[tokio::test]
async fn wallet_query_is_shared_across_inference_groups() {
    // catalog and manifest provide the current declarations without freezing model inventory contents.
    // catalog 与 manifest 提供当前声明，不冻结模型目录内容。
    let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
    let manifest = catalog
        .manifest_arc("nodeai")
        .expect("NODEAI manifest may need updating.");
    assert!(!manifest.descriptor.service_entries.is_empty());
    for entry in &manifest.descriptor.service_entries {
        // snapshot and request expose both the public result and the exact on-wire protocol.
        // snapshot 与 request 同时公开公共结果和精确线上协议。
        let (snapshot, request) = query_fixture(
            entry,
            "200 OK",
            r#"{"mode":"unrestricted","isValid":true,"balance":13.25,"unit":"USD"}"#,
        )
        .await;
        assert_eq!(snapshot.availability, AccountUsageAvailability::Supported);
        assert_eq!(snapshot.credential_id, "wallet-credential");
        assert_eq!(snapshot.balance_amount.as_deref(), Some("13.25"));
        assert!(request.starts_with("GET /v1/usage HTTP/1.1\r\n"));
        assert!(request.contains("authorization: Bearer synthetic-nodeai-key\r\n"));
        assert!(!request.contains("x-api-key:"));
        assert!(!request.contains("anthropic-version:"));
    }
}

/// Verifies authentication and malformed-response failures remain explicit and redact echoed secrets.
/// 验证鉴权与响应格式失败保持明确，并脱敏回显密钥。
#[tokio::test]
async fn wallet_query_projects_failures_without_fallback_or_secret_leaks() {
    // catalog locates the wallet entry by capability instead of a mutable group index.
    // catalog 按能力定位钱包入口，不依赖可变分组下标。
    let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
    let manifest = catalog.manifest_arc("nodeai").unwrap();
    let entry = manifest
        .descriptor
        .service_entries
        .iter()
        .find(|entry| {
            entry.credential_profiles.iter().any(|profile| {
                profile.account_usage_query == crate::AccountUsageQueryKind::NodeAiBalance
            })
        })
        .expect("NODEAI wallet declaration may need updating.");
    for (status, body) in [
        (
            "401 Unauthorized",
            r#"{"error":{"message":"synthetic-nodeai-key rejected"}}"#,
        ),
        ("404 Not Found", "404 page not found"),
        (
            "200 OK",
            r#"{"mode":"unrestricted","isValid":"synthetic-nodeai-key","unit":"USD"}"#,
        ),
    ] {
        // snapshot must expose a failed refresh without inventing any balance.
        // snapshot 必须显示刷新失败，不能伪造余额。
        let (snapshot, _) = query_fixture(entry, status, body).await;
        assert_eq!(
            snapshot.availability,
            AccountUsageAvailability::TemporarilyUnavailable
        );
        assert!(snapshot.balance_amount.is_none());
        assert!(
            !snapshot
                .refresh_message
                .unwrap()
                .contains("synthetic-nodeai-key")
        );
    }
}
