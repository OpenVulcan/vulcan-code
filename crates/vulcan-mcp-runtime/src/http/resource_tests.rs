//! Response streams must retain the exact host authority after every client handle is dropped.
//! 全部客户端句柄释放后，响应流必须保留精确宿主权威。

use super::*;
use crate::remote_test_support::{HeldHttpServer, OBSERVE, host_lease};
use futures::StreamExt;
use std::fs::TryLockError;

/// Cancellation reaches a response body that has already returned its headers.
/// 取消到达已经返回响应头的响应体。
/// No inputs; a genuine pending SSE read ends, then actual stream/client disposal releases the OS lease.
/// 无输入；真实挂起 SSE 读取结束，随后实际流及客户端销毁释放操作系统租约。
#[tokio::test]
async fn cancelled_response_body_releases_its_original_host_lease() {
    // Valid response headers arrive, but no SSE body event is ever sent.
    // 有效响应头已到达，但始终不发送 SSE 响应体事件。
    let mut server = HeldHttpServer::start(Some(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
    ))
    .await;
    let (witness, resources, release) = host_lease();
    let client =
        AuthenticatedStreamableHttpClient::new(None, resources).expect("retained HTTP client");
    // Getting the stream proves that cancellation below happens after response headers.
    // 取得流证明下方取消发生在响应头之后。
    let mut stream = tokio::time::timeout(
        OBSERVE,
        client.get_stream(server.uri.clone().into(), None, None, None, HashMap::new()),
    )
    .await
    .expect("header deadline")
    .expect("real SSE response");
    assert_eq!(server.request_line().await, "GET /mcp HTTP/1.1");
    assert!(matches!(
        witness.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    client.cancellation.cancel();
    assert!(
        tokio::time::timeout(OBSERVE, stream.next())
            .await
            .expect("cancelled body deadline")
            .is_none()
    );
    drop(client);
    // The completed wrapper still owns its response until it is actually disposed.
    // 已结束包装仍拥有响应，直至实际销毁。
    assert!(matches!(
        witness.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(stream);
    tokio::time::timeout(OBSERVE, release.wait())
        .await
        .expect("body release deadline")
        .expect("body release receipt");
    witness
        .as_file()
        .try_lock()
        .expect("actual body owner released");
}

/// Graceful session deletion remains possible after ordinary requests and streams are cancelled.
/// 普通请求及流取消后，仍可执行优雅会话删除。
/// No inputs; witnesses a real DELETE request and successful response under the cancelled worker token.
/// 无输入；在已取消工作令牌下见证真实 DELETE 请求及成功响应。
#[tokio::test]
async fn worker_cancellation_preserves_session_delete() {
    // The official DELETE implementation accepts this actual completed response.
    // 官方 DELETE 实现接受此实际完成响应。
    let mut server = HeldHttpServer::start(Some(
        b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n",
    ))
    .await;
    let (witness, resources, release) = host_lease();
    let client =
        AuthenticatedStreamableHttpClient::new(None, resources).expect("retained HTTP client");
    client.cancellation.cancel();
    tokio::time::timeout(
        OBSERVE,
        client.delete_session(
            server.uri.clone().into(),
            Arc::from("original-session"),
            None,
            HashMap::new(),
        ),
    )
    .await
    .expect("DELETE response deadline")
    .expect("original session deletion");
    assert_eq!(server.request_line().await, "DELETE /mcp HTTP/1.1");
    assert!(matches!(
        witness.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(client);
    tokio::time::timeout(OBSERVE, release.wait())
        .await
        .expect("DELETE lease deadline")
        .expect("DELETE lease released");
    witness
        .as_file()
        .try_lock()
        .expect("actual DELETE client released");
}

/// A real GET response keeps its host lease while independent from all client clones.
/// 真实 GET 响应独立于全部客户端克隆时仍保留宿主租约。
/// No inputs; verifies actual event delivery, lock retention and final release.
/// 无输入；验证实际事件交付、锁保留及最终释放。
#[tokio::test]
async fn get_response_retains_original_host_lease() {
    response_retains_original_host_lease(false).await;
}

/// A real streaming POST response keeps its host lease after its initiating client is gone.
/// 真实流式 POST 响应在发起客户端消失后仍保留宿主租约。
/// No inputs; verifies the official POST response variant and independent stream lifetime.
/// 无输入；验证官方 POST 响应变体及独立流生命周期。
#[tokio::test]
async fn post_response_retains_original_host_lease() {
    response_retains_original_host_lease(true).await;
}

/// Exercises the actual HTTP stream path selected by post, with a real OS lease witness.
/// 以真实操作系统租约见证，检验 post 选择的实际 HTTP 流路径。
/// Post selects POST or GET; returns only after event delivery and lease release are proven.
/// post 选择 POST 或 GET；仅在事件交付及租约释放已证实后返回。
async fn response_retains_original_host_lease(post: bool) {
    // This valid HTTP response deliberately stays open after its first SSE event.
    // 此有效 HTTP 响应在首条 SSE 事件后故意保持打开。
    let mut server = HeldHttpServer::start(Some(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: lease-held\n\n"
    )).await;
    // Only the client and its actual response may own the transferred lease.
    // 仅客户端及其实际响应可以拥有已移交租约。
    let (witness, resources, release) = host_lease();
    let client =
        AuthenticatedStreamableHttpClient::new(None, resources).expect("retained HTTP client");
    let cloned_client = client.clone();
    // The returned stream is independent of the borrow used to issue the request.
    // 返回流独立于发起请求时使用的借用。
    let mut stream = tokio::time::timeout(OBSERVE, async {
        if post {
            // Ping is an actual MCP client request, encoded by the official SDK.
            // ping 是实际 MCP 客户端请求，由官方 SDK 编码。
            let message = serde_json::from_value(serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "ping"
            }))
            .expect("official ping request");
            let response = client
                .post_message(
                    server.uri.clone().into(),
                    message,
                    None,
                    None,
                    HashMap::new(),
                )
                .await
                .expect("actual streaming POST");
            match response {
                rmcp::transport::streamable_http_client::StreamableHttpPostResponse::Sse(
                    stream,
                    _,
                ) => stream,
                other => panic!("expected a streaming POST response, got {other:?}"),
            }
        } else {
            client
                .get_stream(server.uri.clone().into(), None, None, None, HashMap::new())
                .await
                .expect("actual streaming GET")
        }
    })
    .await
    .expect("response headers deadline");
    assert_eq!(
        server.request_line().await,
        if post {
            "POST /mcp HTTP/1.1"
        } else {
            "GET /mcp HTTP/1.1"
        }
    );
    drop(client);
    assert!(matches!(
        witness.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(cloned_client);
    assert!(
        matches!(witness.as_file().try_lock(), Err(TryLockError::WouldBlock)),
        "an actual response stream must retain the original host lease"
    );
    // Reading after dropping every client proves this is a functioning response, not an inert marker.
    // 全部客户端释放后读取，证明这是真实可用响应，而非静态标记。
    let event = tokio::time::timeout(OBSERVE, stream.next())
        .await
        .expect("SSE event deadline")
        .expect("SSE event")
        .expect("valid SSE event");
    assert_eq!(event.data.as_deref(), Some("lease-held"));
    drop(stream);
    tokio::time::timeout(OBSERVE, release.wait())
        .await
        .expect("final release deadline")
        .expect("original release receipt");
    witness
        .as_file()
        .try_lock()
        .expect("actual host file released");
}
