//! A real legacy SSE server verifies shutdown after publication and retained transport borrowing.
//! 真实旧式 SSE 服务器验证发布后关闭及传输借用保留。

use super::*;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// One bounded deadline for the real remote lifecycle fixture.
/// 真实远程生命周期夹具共用的一项有界期限。
const REMOTE_OBSERVE: Duration = Duration::from_secs(5);

/// Executor loss releases the registry's actual SSE client even when diagnostic state is retained.
/// 即使诊断状态被保留，执行器丢失仍释放注册表的实际 SSE 客户端。
/// No inputs or return; a real OS file lease and a second executor observe final aggregate release.
/// 无输入与返回值；真实操作系统文件租约及第二执行器观察最终聚合释放。
#[test]
fn lost_executor_revokes_actual_sse_owner_from_retained_diagnostics() {
    // A separately opened actual file detects resource ownership independently of runtime snapshots.
    // 单独打开的实际文件独立于运行时快照检测资源所有权。
    let witness = tempfile::NamedTempFile::new().expect("SSE authority witness");
    // This exclusive owner moves through the exact lifecycle and real HTTP client.
    // 此排他所有者经精确生命周期移入真实 HTTP 客户端。
    let lease = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(witness.path())
        .expect("actual SSE host lease");
    lease.lock().expect("exclusive host lease");
    // Aggregate ownership is not retained by the release observer.
    // 聚合所有权不由释放观察者保留。
    let (resources, release) = vulcan_mcp_runtime::McpTransportResources::new(lease);
    // The first executor owns the original production connection and the loopback fixture.
    // 第一执行器拥有原生产连接及回环夹具。
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("original remote executor");
    // Keep historical state after all tasks belonging to its original executor are disposed.
    // 原执行器的全部任务销毁后，仍保留历史状态。
    let (runtime, fixture) = executor.block_on(async {
        // Use the real legacy initialize and tools/list exchange.
        // 使用真实旧式初始化及工具发现交互。
        let fixture = LegacyServer::start().await;
        // This isolated declaration uses the same candidate intent without pretending to be a plugin package.
        // 此隔离声明使用相同候选用途，不冒充插件包。
        let configuration = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: "lost-executor-sse".into(),
            name: "Lost executor SSE".into(),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Sse {
                url: fixture.uri.clone(),
                bearer_token: None,
                headers: BTreeMap::new(),
                headers_from_env: BTreeMap::new(),
                oauth: None,
            },
        };
        // The exact supplied authority follows the actual registered transport.
        // 精确传入权威跟随实际注册传输。
        let runtime = create_mcp_runtime(
            vec![configuration],
            resources,
            McpRuntimePurpose::CandidateReadiness,
        )
        .expect("remote lifecycle");
        runtime
            .wait_until_ready(REMOTE_OBSERVE)
            .await
            .expect("real SSE readiness");
        (runtime, fixture)
    });
    assert!(matches!(
        witness.as_file().try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(executor);
    drop(fixture);
    // A different executor queries the original receipt while runtime diagnostics still exist.
    // 不同执行器在运行时诊断仍存在时查询原回执。
    let recovery = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("remote recovery executor");
    // Store the result so cleanup still drops retained foreground state on a regression.
    // 保存结果，使回归发生时清理仍销毁保留的前台状态。
    let released =
        recovery.block_on(async { tokio::time::timeout(REMOTE_OBSERVE, release.wait()).await });
    // Actual physical disposal after executor loss cannot manufacture successful original cleanup.
    // 执行器丢失后，实际物理销毁不能制造成功原清理。
    let fully_stopped = runtime.is_fully_stopped();
    let drain_result = recovery.block_on(runtime.wait_until_drained(tokio::time::Instant::now()));
    // The original state must retain abnormal failure rather than stale connected readiness.
    // 原状态必须保留异常失败，而非过期的已连接就绪。
    let states = runtime.server_states();
    drop(runtime);
    released
        .expect("actual SSE client released after executor loss")
        .expect("aggregate release evidence");
    assert!(
        !fully_stopped,
        "executor loss must retain negative cleanup authority"
    );
    assert!(
        drain_result.is_err(),
        "physical release must not promote the lost original cleanup operation"
    );
    assert!(
        states
            .iter()
            .all(|state| state.status == McpRuntimeServerStatus::Failed)
    );
    witness
        .as_file()
        .try_lock()
        .expect("real host authority was released");
}

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
