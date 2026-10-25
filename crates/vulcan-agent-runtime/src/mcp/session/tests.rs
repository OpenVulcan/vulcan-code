use super::*;
use async_trait::async_trait;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use vulcan_mcp_runtime::McpRuntimeError;

/// In-process MCP server: two tools, a call log, and an event channel the test drives.
/// 进程内 MCP 服务：两个工具、调用日志，以及由测试驱动的事件通道。
struct FakeTransport {
    tools: Mutex<Vec<McpToolSchema>>,
    calls: Mutex<Vec<(String, Value)>>,
    lists: AtomicUsize,
    shutdowns: AtomicUsize,
    events: broadcast::Sender<McpTransportEvent>,
}

impl FakeTransport {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            tools: Mutex::new(vec![
                tool(
                    "read_page",
                    json!({"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}),
                ),
                tool("click", json!({"type": "object"})),
            ]),
            calls: Mutex::new(Vec::new()),
            lists: AtomicUsize::new(0),
            shutdowns: AtomicUsize::new(0),
            events: broadcast::channel(8).0,
        })
    }
}

fn tool(name: &str, args_schema: Value) -> McpToolSchema {
    McpToolSchema {
        name: name.to_string(),
        description: format!("{name} description"),
        args_schema,
        result_schema: Value::Null,
    }
}

#[async_trait]
impl McpTransport for FakeTransport {
    fn endpoint(&self) -> &str {
        "fake"
    }

    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        Ok(lock(&self.tools).clone())
    }

    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
        self.events.subscribe()
    }

    async fn shutdown(&self) -> Result<(), McpRuntimeError> {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpRuntimeError> {
        lock(&self.calls).push((name.to_string(), args));
        Ok(json!("done"))
    }
}

/// Registers one fake connection named `name` for `root`.
/// 为 `root` 注册一个名为 `name` 的假连接。
async fn open(
    registry: &SessionMcpRegistry,
    root: SessionId,
    name: &str,
    transport: &Arc<FakeTransport>,
) -> Result<SessionMcpConnectionSnapshot, String> {
    registry
        .register(
            root,
            name.to_string(),
            SessionMcpTransportKind::Stdio,
            String::from("fake-server"),
            Arc::clone(transport) as Arc<dyn McpTransport>,
        )
        .await
        .map(|(snapshot, _)| snapshot)
}

#[tokio::test]
async fn connections_belong_to_their_session_and_close_with_it() {
    let registry = SessionMcpRegistry::new();
    let (owner, other) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2));
    let transport = FakeTransport::new();
    let snapshot = open(&registry, owner, "browser", &transport).await.unwrap();
    assert_eq!(snapshot.tool_count, 2);
    assert!(snapshot.id.starts_with("mcp_"));
    assert!(registry.snapshot(other, &snapshot.id).is_err());
    assert!(
        registry
            .call(other, &snapshot.id, "click", json!({}))
            .await
            .is_err()
    );

    assert!(registry.close_session(owner).await.is_empty());
    assert_eq!(transport.shutdowns.load(Ordering::SeqCst), 1);
    assert!(registry.snapshots(owner).is_empty());
    assert!(registry.snapshot(owner, &snapshot.id).is_err());
}

#[tokio::test]
async fn names_are_unique_and_connections_are_bounded() {
    let registry = SessionMcpRegistry::new();
    let root = uuid::Uuid::from_u128(3);
    let transport = FakeTransport::new();
    open(&registry, root, "Docs", &transport).await.unwrap();
    assert!(open(&registry, root, "docs", &transport).await.is_err());
    for index in 1..SESSION_MCP_MAX_CONNECTIONS {
        open(&registry, root, &format!("server-{index}"), &transport)
            .await
            .unwrap();
    }
    let refused = FakeTransport::new();
    assert!(
        open(&registry, root, "one-too-many", &refused)
            .await
            .is_err()
    );
    assert_eq!(
        refused.shutdowns.load(Ordering::SeqCst),
        1,
        "a refused transport must be shut down"
    );
    assert_eq!(registry.snapshots(root).len(), SESSION_MCP_MAX_CONNECTIONS);
}

#[tokio::test]
async fn calls_are_checked_against_the_tool_schema() {
    let registry = SessionMcpRegistry::new();
    let root = uuid::Uuid::from_u128(4);
    let transport = FakeTransport::new();
    let id = open(&registry, root, "pages", &transport).await.unwrap().id;

    assert!(
        registry
            .call(root, &id, "read_page", json!({}))
            .await
            .is_err()
    );
    assert!(
        registry
            .call(root, &id, "read_page", json!("x"))
            .await
            .is_err()
    );
    assert!(
        registry
            .call(root, &id, "missing", json!({}))
            .await
            .is_err()
    );
    assert!(
        lock(&transport.calls).is_empty(),
        "invalid calls must not reach the server"
    );

    let output = registry
        .call(
            root,
            &id,
            "read_page",
            json!({"url": "https://example.com"}),
        )
        .await
        .unwrap();
    assert_eq!(output, json!("done"));
    assert_eq!(lock(&transport.calls).len(), 1);
}

#[tokio::test]
async fn tool_changes_relist_and_disconnects_close_the_connection() {
    let registry = SessionMcpRegistry::new();
    let root = uuid::Uuid::from_u128(5);
    let transport = FakeTransport::new();
    let id = open(&registry, root, "live", &transport).await.unwrap().id;
    let lists_after_connect = transport.lists.load(Ordering::SeqCst);

    registry.tools(root, &id, false).await.unwrap();
    assert_eq!(transport.lists.load(Ordering::SeqCst), lists_after_connect);

    lock(&transport.tools).push(tool("scroll", json!({"type": "object"})));
    transport
        .events
        .send(McpTransportEvent::ToolsChanged)
        .unwrap();
    let relisted = wait_until(|| async {
        let (_, tools) = registry.tools(root, &id, false).await.unwrap();
        (tools.len() == 3).then_some(tools)
    })
    .await;
    assert_eq!(relisted.len(), 3);

    transport
        .events
        .send(McpTransportEvent::Disconnected {
            reason: String::from("server exited"),
        })
        .unwrap();
    let closed = wait_until(|| async { registry.snapshot(root, &id).unwrap().closed }).await;
    assert_eq!(closed, "server exited");
    assert!(registry.call(root, &id, "click", json!({})).await.is_err());

    registry.disconnect(root, &id).await.unwrap();
    assert!(registry.snapshots(root).is_empty());
}

/// Polls `probe` until it yields a value; the watcher task runs on the same test runtime.
/// 轮询 `probe` 直至其产出值；监视任务运行在同一测试运行时上。
async fn wait_until<T, F, Fut>(probe: F) -> T
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    for _ in 0..200 {
        if let Some(value) = probe().await {
            return value;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("condition was not reached");
}
