//! Negative-chain regression for remote compaction retries.
//! 远端压缩重试的负向链回归。
//!
//! One compaction stream is exactly one physical HTTP request, so an owner that settles a rejected
//! attempt and retries must open a second, independently identified physical request. These tests
//! pin the transport contract the session owner's 401 recovery relies on.
//! 一次压缩流恰好对应一次真实 HTTP 请求；因此先结算被拒尝试再重试的所有者必须开启第二次身份
//! 独立的真实请求。以下测试固定会话所有者 401 恢复所依赖的传输契约。

use super::super::test_support::{bundled_manifest_route, minimal_request, read_test_http_request};
use super::*;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use vulcan_ai_protocols::ProtocolRegistry;

/// Returns one synthetic OAuth secret so both attempts represent the same frozen account.
/// 返回一份合成 OAuth 秘密，使两次尝试代表同一冻结账号。
fn compact_secret() -> CredentialExecutionSecret {
    crate::CredentialSecretEnvelope::new(crate::CredentialSecretMaterial::OpenAiCodexOAuth {
        tokens: crate::OAuthTokenSet {
            access_token: String::from("compact-fake-token"),
            refresh_token: String::from("compact-fake-refresh"),
            issued_at_unix_ms: 1,
            expires_at_unix_ms: 2,
            scopes: Default::default(),
            account_subject: None,
            email: None,
        },
        id_token: None,
        account_id: Some(String::from("compact-fake-account")),
    })
    .expect("synthetic OAuth secret")
    .execution_secret()
    .expect("synthetic execution secret")
}

/// Builds one production compact client over an isolated loopback transport.
/// 在隔离环回传输上构建一份生产压缩客户端。
fn compact_client() -> ProviderExecutionClient {
    ProviderExecutionClient::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        Arc::new(ProtocolRegistry::first_batch().unwrap()),
    )
}

/// Builds one Codex compaction route pinned to a single loopback attempt endpoint.
/// 构建一条固定到单个环回尝试端点的 Codex 压缩路由。
fn attempt_route(address: SocketAddr) -> ProviderExecutionRoute {
    let mut route = bundled_manifest_route(
        "openai",
        "codex-subscription",
        OPENAI_CODEX_RESPONSES_PROFILE_ID,
    );
    // baseUrlOverride replaces only the endpoint; every native Codex declaration stays authoritative.
    // baseUrlOverride 仅替换端点；全部原生 Codex 声明保持权威。
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    route
}

/// Serves exactly one physical compact attempt and reports whether that same identity asked twice.
/// 恰好服务一次真实压缩尝试，并报告同一身份是否请求了两次。
async fn serve_one_compact_attempt(
    status: u16,
    request_id: &'static str,
) -> (SocketAddr, tokio::task::JoinHandle<(String, bool)>) {
    // listener is the only reachable transport endpoint for this attempt identity.
    // listener 是该尝试身份唯一可达的传输端点。
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let received = read_test_http_request(&mut socket).await;
        // body echoes a fake secret on failure so production redaction is exercised on the same path.
        // body 在失败时回显假秘密，使生产脱敏在同一条路径上被验证。
        let body = if status == 200 {
            json!({
                "id": "compact-result",
                "output": [{"type": "compaction", "encrypted_content": "opaque"}],
                "usage": {"input_tokens": 3}
            })
            .to_string()
        } else {
            json!({"error": {"message": "rejected compact-fake-token"}}).to_string()
        };
        socket
            .write_all(
                format!(
                    "HTTP/1.1 {status} Test\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-request-id: {request_id}\r\nconnection: close\r\n\r\n",
                    body.len(),
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        socket.write_all(body.as_bytes()).await.unwrap();
        // A hidden retry inside one attempt would open a second connection to this exact identity.
        // 单次尝试内部的隐藏重试会向该精确身份打开第二条连接。
        let duplicated = tokio::time::timeout(Duration::from_millis(300), listener.accept())
            .await
            .is_ok();
        (received, duplicated)
    });
    (address, server)
}

/// Returns the JSON request body as the transport actually serialized it.
/// 返回传输层实际序列化出的 JSON 请求正文。
fn received_body(request: &str) -> Value {
    serde_json::from_str(
        request
            .split_once("\r\n\r\n")
            .expect("a complete HTTP request must contain a body separator")
            .1,
    )
    .expect("the compact request body must be JSON")
}

/// Verifies a rejected attempt and its same-account replay own separate physical identities and results.
/// 验证被拒尝试与其同账号重放各自持有独立的物理身份与结果。
#[tokio::test]
async fn compact_retry_attempts_keep_independent_physical_identity_and_results() {
    let client = compact_client();
    let (first_address, first_server) = serve_one_compact_attempt(401, "compact-attempt-1").await;
    let (second_address, second_server) = serve_one_compact_attempt(200, "compact-attempt-2").await;

    // firstAttempt is the rejected physical request that the session owner must settle before recovery.
    // firstAttempt 是会话所有者必须先结算再恢复的被拒物理请求。
    let mut first_stream = client.stream_responses_compact(
        attempt_route(first_address),
        minimal_request(),
        compact_secret(),
        None,
        // No response capture is requested; this test observes the attempt boundary itself.
        // 本测试不请求响应捕获；它直接观察尝试边界。
        None,
    );
    let first_observed = first_stream.next().await.unwrap().unwrap();
    let ResponsesCompactEvent::TransportObserved {
        http_status,
        provider_request_id,
        ..
    } = first_observed
    else {
        panic!("one physical attempt must observe its own headers before any result");
    };
    assert_eq!(http_status, 401);
    assert_eq!(provider_request_id.as_deref(), Some("compact-attempt-1"));
    let first_failure = first_stream.next().await.unwrap().unwrap_err();
    assert_eq!(first_failure.error.http_status, Some(401));
    assert_eq!(
        first_failure.error.dispatch_outcome,
        DispatchOutcome::Dispatched
    );
    // Only a physically rejected attempt with no visible output and no side effect may be replayed.
    // 只有没有可见输出与副作用的物理被拒尝试才允许重放。
    assert!(!first_failure.error.semantic_output_observed);
    assert!(!first_failure.error.side_effect_observed);
    assert!(!first_failure.error.message.contains("compact-fake-token"));
    // The rejected attempt can never surface a native result of its own.
    // 被拒尝试绝不会产出属于自身的原生结果。
    assert!(first_stream.next().await.is_none());

    // secondAttempt re-sends the same frozen projection through a new physical request after recovery.
    // secondAttempt 在恢复后通过新的物理请求重发同一冻结投影。
    let mut second_stream = client.stream_responses_compact(
        attempt_route(second_address),
        minimal_request(),
        compact_secret(),
        None,
        // The retried attempt also observes only its own attempt boundary.
        // 重试尝试同样只观察自身的尝试边界。
        None,
    );
    let second_observed = second_stream.next().await.unwrap().unwrap();
    let ResponsesCompactEvent::TransportObserved {
        http_status,
        provider_request_id,
        ..
    } = second_observed
    else {
        panic!("the second physical attempt must observe its own headers first");
    };
    assert_eq!(http_status, 200);
    assert_eq!(provider_request_id.as_deref(), Some("compact-attempt-2"));
    let second_event = second_stream.next().await.unwrap().unwrap();
    let ResponsesCompactEvent::Completed(result) = second_event else {
        panic!("the second physical attempt must yield its own completed result");
    };
    assert_eq!(
        result.output_items,
        vec![json!({"type": "compaction", "encrypted_content": "opaque"})]
    );
    assert_eq!(result.response_id.as_deref(), Some("compact-result"));
    assert!(second_stream.next().await.is_none());

    // Each endpoint proves it served exactly one physical request for its own identity.
    // 每个端点证明它只为自身身份服务了一次真实请求。
    let (first_received, first_duplicated) = first_server.await.unwrap();
    let (second_received, second_duplicated) = second_server.await.unwrap();
    assert!(
        !first_duplicated,
        "the rejected physical request must not be repeated inside one attempt"
    );
    assert!(
        !second_duplicated,
        "the retried physical request must not be repeated inside one attempt"
    );
    assert!(first_received.starts_with("POST /responses/compact HTTP/1.1\r\n"));
    assert!(second_received.starts_with("POST /responses/compact HTTP/1.1\r\n"));
    // The retry resends the frozen projection unchanged instead of mutating the rejected request.
    // 重试原样重发冻结投影，而不是修改被拒请求。
    assert_eq!(
        received_body(&first_received),
        received_body(&second_received)
    );
    assert_eq!(
        received_body(&first_received)["model"],
        received_body(&second_received)["model"]
    );
}
