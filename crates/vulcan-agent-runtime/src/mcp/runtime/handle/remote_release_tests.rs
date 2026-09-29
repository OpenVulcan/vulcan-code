//! A real legacy SSE server verifies shutdown after publication and retained transport borrowing.
//! 真实旧式 SSE 服务器验证发布后关闭及传输借用保留。

use super::*;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// One bounded deadline for the real remote lifecycle fixture.
/// 真实远程生命周期夹具共用的一项有界期限。
const REMOTE_OBSERVE: Duration = Duration::from_secs(5);

/// Owns the actual listener and every accepted fixture connection.
/// 拥有实际监听器及全部已接受夹具连接。
struct LegacyServer {
    /// Exact loopback URL supplied to the production registry.
    /// 传给生产注册表的精确回环 URL。
    uri: String,
    /// Aborting this original task drops its JoinSet and all accepted connections.
    /// 中止此原任务会销毁其 JoinSet 及全部已接受连接。
    task: tokio::task::JoinHandle<()>,
}

impl LegacyServer {
    /// Starts an actual legacy MCP endpoint with initialize, notification and tool discovery support.
    /// 启动支持初始化、通知及工具发现的实际旧式 MCP 端点。
    /// No inputs; returns the OS-allocated endpoint and its sole fixture owner.
    /// 无输入；返回操作系统分配的端点及其唯一夹具所有者。
    async fn start() -> Self {
        // Let the OS choose an unused loopback port.
        // 由操作系统选择未使用回环端口。
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("legacy fixture listener");
        let uri = format!(
            "http://{}/sse",
            listener.local_addr().expect("legacy fixture address")
        );
        // The set owns every accepted socket task, including the persistent SSE stream.
        // 集合拥有全部已接受套接字任务，包括持久 SSE 流。
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (socket, _) = accepted.expect("legacy connection");
                        connections.spawn(serve_connection(socket));
                    }
                    joined = connections.join_next(), if !connections.is_empty() => {
                        joined.expect("accepted connection task").expect("legacy request completed");
                    }
                }
            }
        });
        Self { uri, task }
    }
}

impl Drop for LegacyServer {
    /// Stops the exact fixture task even if a test assertion fails.
    /// 即使测试断言失败，也停止精确夹具任务。
    /// No inputs or return value; dropping the JoinSet then aborts its socket owners.
    /// 无输入或返回值；随后销毁 JoinSet 会中止其套接字所有者。
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Serves one real request; SSE stays open and each POST receives a complete protocol response.
/// 服务一个真实请求；SSE 保持打开，每次 POST 接收完整协议响应。
/// Socket is the accepted owner; returns after a POST or remains owned until fixture cancellation.
/// socket 为已接受所有者；POST 后返回，或保持拥有直至夹具取消。
async fn serve_connection(mut socket: tokio::net::TcpStream) {
    // Read only through the exact delimiter so the subsequent JSON body remains intact.
    // 仅读取至精确分隔符，保持后续 JSON 请求体完整。
    let mut headers = Vec::new();
    let mut byte = [0_u8; 1];
    while !headers.ends_with(b"\r\n\r\n") {
        assert!(headers.len() < 8192, "fixture headers unexpectedly large");
        socket.read_exact(&mut byte).await.expect("legacy headers");
        headers.push(byte[0]);
    }
    let headers = String::from_utf8(headers).expect("ASCII fixture headers");
    let request_line = headers.lines().next().expect("HTTP request line");
    if request_line == "GET /sse HTTP/1.1" {
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\nevent: endpoint\ndata: /message\n\n").await.expect("legacy endpoint event");
        std::future::pending::<()>().await;
        drop(socket);
        return;
    }
    assert_eq!(request_line, "POST /message HTTP/1.1");
    // Every official client JSON POST supplies an authoritative Content-Length header.
    // 每个官方客户端 JSON POST 均提供权威 Content-Length 请求头。
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("JSON request length"))
        })
        .expect("POST content length");
    let mut body = vec![0; length];
    socket
        .read_exact(&mut body)
        .await
        .expect("legacy request body");
    let message: Value = serde_json::from_slice(&body).expect("legacy JSON request");
    let method = message["method"].as_str().expect("declared MCP method");
    if method == "notifications/initialized" {
        socket
            .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .expect("notification response");
    } else {
        // Match only the exact methods declared by this fixture; unexpected calls fail loudly.
        // 仅匹配此夹具声明的精确方法；意外调用明确失败。
        let result = match method {
            "initialize" => json!({
                "protocolVersion": message["params"]["protocolVersion"].as_str().expect("client protocol version"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "legacy-release-fixture", "version": "1.0.0"}
            }),
            "tools/list" => json!({"tools": []}),
            other => panic!("unexpected fixture MCP method: {other}"),
        };
        let response = serde_json::to_vec(&json!({
            "jsonrpc": "2.0", "id": message.get("id").expect("request identity"), "result": result
        }))
        .expect("protocol response");
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.len()
        );
        socket
            .write_all(headers.as_bytes())
            .await
            .expect("response headers");
        socket.write_all(&response).await.expect("response JSON");
    }
    socket.shutdown().await.expect("response finished");
}

/// A connected SSE transport must release the lifecycle's own Arc before waiting on real client disposal.
/// 已连接 SSE 传输必须先释放生命周期自身 Arc，再等待实际客户端销毁。
/// No inputs; a retained caller prevents stopping, and its release allows the original cleanup to finish.
/// 无输入；保留调用方阻止停止，释放后允许原清理完成。
#[tokio::test]
async fn connected_sse_waits_for_actual_transport_release() {
    // Exercise the production registry, initialize handshake and tools/list discovery.
    // 检验生产注册表、初始化握手及 tools/list 发现。
    let server = LegacyServer::start().await;
    let configuration = McpServerConfig {
        source: crate::mcp::config::McpServerSource::UserConfigured,
        id: "actual-sse-release".into(),
        name: "Actual SSE release".into(),
        enabled: true,
        transport: crate::mcp::config::McpServerTransportConfig::Sse {
            url: server.uri.clone(),
            bearer_token: None,
            headers: BTreeMap::new(),
            headers_from_env: BTreeMap::new(),
            oauth: None,
        },
    };
    let runtime = bootstrap_mcp_runtime(vec![configuration.clone()])
        .await
        .expect("actual SSE runtime");
    let mut guard = McpRuntimeStartupGuard::new();
    guard.track(&runtime);
    runtime
        .wait_until_ready(REMOTE_OBSERVE)
        .await
        .expect("actual SSE initialize and discovery");
    // Retain a real caller's transport, whose Self still owns the actual reqwest client.
    // 保留真实调用方传输，其自身仍拥有实际 reqwest 客户端。
    let borrowed = runtime
        .transport_for_server(&configuration.id)
        .expect("published SSE transport");
    runtime.shutdown();
    runtime
        .wait_until_stopped(Duration::from_millis(50))
        .await
        .expect_err("caller still retains the actual SSE client");
    assert_eq!(
        runtime.snapshot().servers[&configuration.id].status,
        McpRuntimeServerStatus::Stopping
    );
    drop(borrowed);
    runtime
        .wait_until_stopped(REMOTE_OBSERVE)
        .await
        .expect("original SSE cleanup after last caller release");
    assert!(runtime.is_fully_stopped());
    assert!(
        runtime
            .inner
            .resource_releases
            .read()
            .expect("release map")
            .is_empty()
    );
}
