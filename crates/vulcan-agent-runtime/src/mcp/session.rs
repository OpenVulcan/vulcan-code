//! Session MCP connections: temporary MCP servers that one session opens for itself through
//! `mcp_connect`. They never enter user configuration or the registered tool list; the agent
//! discovers their tools with `mcp_tools` and calls them with `mcp_call`, so the tool list stays
//! fixed. Connections belong to the root session and end when the agent disconnects them, when the
//! session is archived or when Vulcan Code exits; they are never reconnected automatically.
//! 会话 MCP 连接：一个会话通过 `mcp_connect` 为自身打开的临时 MCP 服务。它们从不进入用户配置或已注册
//! 工具列表；代理用 `mcp_tools` 发现其工具、用 `mcp_call` 调用，因此工具列表保持固定。连接属于根会话，
//! 在代理断开、会话归档或 Vulcan Code 退出时结束，且从不自动重连。

use crate::mcp::runtime::{
    MCP_INITIALIZE_TIMEOUT, MCP_TOOLS_LIST_TIMEOUT, mcp_transport_startup_semaphore,
    validate_mcp_tool_schema_list,
};
use crate::model::SessionId;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use vulcan_mcp_runtime::{
    McpServerIdentity, McpToolResultAudience, McpToolSchema, McpTransport, McpTransportEvent,
    McpTransportLaunchSpec, McpTransportResources,
};

/// Most connections one session may hold at once.
/// 一个会话同时最多持有的连接数。
pub const SESSION_MCP_MAX_CONNECTIONS: usize = 8;

/// Longest connection name, in Unicode scalar values.
/// 连接名称的最大长度，按 Unicode 标量值计。
pub const SESSION_MCP_NAME_MAX_CHARS: usize = 64;

/// How one session connection reaches its server.
/// 一个会话连接到达其服务的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMcpTransportKind {
    /// A child process speaking MCP over standard input and output.
    /// 通过标准输入输出使用 MCP 的子进程。
    Stdio,
    /// A streamable HTTP endpoint.
    /// Streamable HTTP 端点。
    Http,
    /// A legacy Server-Sent Events endpoint.
    /// 旧式 Server-Sent Events 端点。
    Sse,
}

impl SessionMcpTransportKind {
    /// Stable key used in tool arguments and results.
    /// 工具参数与结果中使用的稳定键。
    pub const fn key(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }
}

/// One connection to open.
/// 一个待打开的连接。
#[derive(Debug, Clone)]
pub struct SessionMcpConnectRequest {
    /// Name the agent refers to the connection by; unique within the session.
    /// 代理用来指代该连接的名称；在会话内唯一。
    pub name: String,
    /// Transport family.
    /// 传输类型。
    pub kind: SessionMcpTransportKind,
    /// Redacted target shown in results: a program name or an origin, never arguments or paths.
    /// 结果中显示的脱敏目标：程序名或源站，绝不包含参数或路径。
    pub target: String,
    /// Exact launch specification.
    /// 精确启动规格。
    pub launch: McpTransportLaunchSpec,
}

/// What one connection looks like now.
/// 一个连接的当前状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMcpConnectionSnapshot {
    /// Stable connection identifier.
    /// 稳定连接标识。
    pub id: String,
    /// Name given by the agent.
    /// 代理给定的名称。
    pub name: String,
    /// Transport family.
    /// 传输类型。
    pub kind: SessionMcpTransportKind,
    /// Redacted target.
    /// 脱敏目标。
    pub target: String,
    /// Server identity negotiated at initialize, when the transport exposes it.
    /// initialize 时协商的服务身份（传输提供时）。
    pub server: Option<McpServerIdentity>,
    /// Connection time in Unix milliseconds.
    /// 连接时间的 Unix 毫秒值。
    pub connected_at_ms: u64,
    /// Number of tools in the last successful tool list.
    /// 最近一次成功工具列表中的工具数量。
    pub tool_count: usize,
    /// Why the connection closed; absent while it is open.
    /// 连接关闭的原因；打开期间缺省。
    pub closed: Option<String>,
}

/// Mutable facts of one connection.
/// 一个连接的可变事实。
#[derive(Default)]
struct ConnectionState {
    /// Last successful tool list.
    /// 最近一次成功的工具列表。
    tools: Vec<McpToolSchema>,
    /// The server announced a tool-list change since the last list.
    /// 自上次列表以来服务宣告了工具列表变化。
    tools_stale: bool,
    /// Why the transport closed.
    /// 传输关闭的原因。
    closed: Option<String>,
}

/// One live session connection.
/// 一个活动会话连接。
struct SessionMcpConnection {
    /// Stable identifier.
    /// 稳定标识。
    id: String,
    /// Name given by the agent.
    /// 代理给定的名称。
    name: String,
    /// Transport family.
    /// 传输类型。
    kind: SessionMcpTransportKind,
    /// Redacted target.
    /// 脱敏目标。
    target: String,
    /// Connection time in Unix milliseconds.
    /// 连接时间的 Unix 毫秒值。
    connected_at_ms: u64,
    /// Initialized transport; dropping the last reference stops a stdio child.
    /// 已初始化的传输；释放最后一个引用会停止 stdio 子进程。
    transport: Arc<dyn McpTransport>,
    /// Mutable facts.
    /// 可变事实。
    state: Mutex<ConnectionState>,
    /// Task following transport events; aborted with the connection.
    /// 跟随传输事件的任务；随连接一起中止。
    watcher: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Drop for SessionMcpConnection {
    /// Stops following transport events.
    /// 停止跟随传输事件。
    fn drop(&mut self) {
        if let Some(watcher) = lock(&self.watcher).take() {
            watcher.abort();
        }
    }
}

impl SessionMcpConnection {
    /// Current snapshot.
    /// 当前快照。
    fn snapshot(&self) -> SessionMcpConnectionSnapshot {
        let state = lock(&self.state);
        SessionMcpConnectionSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind,
            target: self.target.clone(),
            server: self.transport.server_identity(),
            connected_at_ms: self.connected_at_ms,
            tool_count: state.tools.len(),
            closed: state.closed.clone(),
        }
    }

    /// Fails when the transport has closed.
    /// 传输已关闭时失败。
    fn ensure_open(&self) -> Result<(), String> {
        match &lock(&self.state).closed {
            Some(reason) => Err(format!(
                "MCP connection `{}` ({}) is closed: {reason}. Disconnect it and open a new one with `mcp_connect`.",
                self.id, self.name
            )),
            None => Ok(()),
        }
    }

    /// Lists the server's tools again and keeps the list when it is valid.
    /// 重新列出服务的工具，列表有效时保存。
    async fn refresh_tools(&self) -> Result<Vec<McpToolSchema>, String> {
        self.ensure_open()?;
        let schemas = tokio::time::timeout(MCP_TOOLS_LIST_TIMEOUT, self.transport.list_tools())
            .await
            .map_err(|_| {
                format!(
                    "MCP server `{}` tools/list timed out after {} seconds.",
                    self.name,
                    MCP_TOOLS_LIST_TIMEOUT.as_secs()
                )
            })?
            .map_err(|error| {
                format!(
                    "Failed to list the tools of MCP server `{}`: {error}",
                    self.name
                )
            })?;
        validate_mcp_tool_schema_list(&self.name, &schemas)?;
        let mut state = lock(&self.state);
        state.tools = schemas.clone();
        state.tools_stale = false;
        Ok(schemas)
    }

    /// The tool list, listed again first when the server announced a change or `refresh` is set.
    /// 工具列表；服务宣告变化或设置了 `refresh` 时先重新列出。
    async fn tools(&self, refresh: bool) -> Result<Vec<McpToolSchema>, String> {
        self.ensure_open()?;
        let cached = {
            let state = lock(&self.state);
            (!refresh && !state.tools_stale).then(|| state.tools.clone())
        };
        match cached {
            Some(tools) => Ok(tools),
            None => self.refresh_tools().await,
        }
    }

    /// One tool's schema; an unknown name triggers one fresh list before failing.
    /// 一个工具的 Schema；名称未知时先重新列出一次再失败。
    async fn tool(&self, name: &str) -> Result<McpToolSchema, String> {
        let find = |tools: &[McpToolSchema]| tools.iter().find(|tool| tool.name == name).cloned();
        if let Some(tool) = find(&self.tools(false).await?) {
            return Ok(tool);
        }
        let tools = self.refresh_tools().await?;
        find(&tools).ok_or_else(|| {
            let names = tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "MCP server `{}` has no tool `{name}`. Available tools: {}.",
                self.name,
                if names.is_empty() { "(none)" } else { &names }
            )
        })
    }
}

/// Session connections of every root session in this process.
/// 本进程中每个根会话的会话连接。
#[derive(Default)]
pub struct SessionMcpRegistry {
    /// Connections by root session, in the order they were opened.
    /// 按根会话分组的连接，按打开顺序排列。
    partitions: Mutex<HashMap<SessionId, Vec<Arc<SessionMcpConnection>>>>,
}

impl std::fmt::Debug for SessionMcpRegistry {
    /// Renders connection counts without targets.
    /// 渲染连接数量，不包含目标。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionMcpRegistry")
            .field("sessions", &lock(&self.partitions).len())
            .finish_non_exhaustive()
    }
}

impl SessionMcpRegistry {
    /// Builds an empty registry.
    /// 构建空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens one connection for `root`, waits for initialize and the first tool list, and returns
    /// the connection with its tools.
    /// 为 `root` 打开一个连接，等待 initialize 与首次工具列表完成，返回该连接及其工具。
    pub async fn connect(
        &self,
        root: SessionId,
        request: SessionMcpConnectRequest,
    ) -> Result<(SessionMcpConnectionSnapshot, Vec<McpToolSchema>), String> {
        self.check_room(root, &request.name)?;
        // The permit covers initialize and the first tool list, like configured servers.
        // 许可覆盖 initialize 与首次工具列表，与已配置服务一致。
        let _permit = mcp_transport_startup_semaphore()
            .acquire()
            .await
            .map_err(|_| String::from("The MCP startup gate is closed."))?;
        let (resources, _release) = McpTransportResources::new(());
        let transport = tokio::time::timeout(
            MCP_INITIALIZE_TIMEOUT,
            request.launch.launch_with_resources(None, resources),
        )
        .await
        .map_err(|_| {
            format!(
                "MCP server `{}` did not finish initializing within {} seconds.",
                request.name,
                MCP_INITIALIZE_TIMEOUT.as_secs()
            )
        })?
        .map_err(|error| {
            format!(
                "Failed to connect to MCP server `{}`: {error}",
                request.name
            )
        })?;
        self.register(root, request.name, request.kind, request.target, transport)
            .await
    }

    /// Refuses a name already used in the session or a session at its connection limit.
    /// 拒绝会话中已使用的名称，或已达连接上限的会话。
    fn check_room(&self, root: SessionId, name: &str) -> Result<(), String> {
        let partitions = lock(&self.partitions);
        let connections = partitions.get(&root).map(Vec::as_slice).unwrap_or_default();
        if let Some(existing) = connections
            .iter()
            .find(|connection| connection.name.eq_ignore_ascii_case(name))
        {
            return Err(format!(
                "This session already has an MCP connection named `{name}` (`{}`). Use it, or disconnect it first.",
                existing.id
            ));
        }
        if connections.len() >= SESSION_MCP_MAX_CONNECTIONS {
            return Err(format!(
                "This session already holds {SESSION_MCP_MAX_CONNECTIONS} MCP connections, the most it may hold. Disconnect one first."
            ));
        }
        Ok(())
    }

    /// Lists the tools of an initialized transport and keeps it as a connection of `root`. The
    /// transport is shut down when anything fails.
    /// 列出已初始化传输的工具，并将其保存为 `root` 的连接。任何一步失败都会关闭该传输。
    async fn register(
        &self,
        root: SessionId,
        name: String,
        kind: SessionMcpTransportKind,
        target: String,
        transport: Arc<dyn McpTransport>,
    ) -> Result<(SessionMcpConnectionSnapshot, Vec<McpToolSchema>), String> {
        // Subscribing before the first list keeps a disconnect during it from being missed.
        // 在首次列表前订阅，避免错过列表期间的断开。
        let events = transport.event_receiver();
        let connection = Arc::new(SessionMcpConnection {
            id: format!("mcp_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            name,
            kind,
            target,
            connected_at_ms: unix_millis(),
            transport: Arc::clone(&transport),
            state: Mutex::new(ConnectionState::default()),
            watcher: Mutex::new(None),
        });
        let registered = async {
            let tools = connection.refresh_tools().await?;
            let mut partitions = lock(&self.partitions);
            let connections = partitions.entry(root).or_default();
            // Another connect may have finished while this one initialized.
            // 本连接初始化期间可能有另一个连接完成。
            if connections
                .iter()
                .any(|existing| existing.name.eq_ignore_ascii_case(&connection.name))
                || connections.len() >= SESSION_MCP_MAX_CONNECTIONS
            {
                return Err(format!(
                    "Another MCP connection named `{}` was opened meanwhile, or the session reached {SESSION_MCP_MAX_CONNECTIONS} connections.",
                    connection.name
                ));
            }
            connections.push(Arc::clone(&connection));
            Ok(tools)
        }
        .await;
        match registered {
            Ok(tools) => {
                *lock(&connection.watcher) = Some(tokio::spawn(follow_transport_events(
                    Arc::downgrade(&connection),
                    events,
                )));
                Ok((connection.snapshot(), tools))
            }
            Err(error) => {
                drop(connection);
                let _ = transport.shutdown().await;
                Err(error)
            }
        }
    }

    /// Snapshots of every connection of `root`, in the order they were opened.
    /// `root` 每个连接的快照，按打开顺序排列。
    pub fn snapshots(&self, root: SessionId) -> Vec<SessionMcpConnectionSnapshot> {
        lock(&self.partitions)
            .get(&root)
            .map(|connections| {
                connections
                    .iter()
                    .map(|connection| connection.snapshot())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Snapshot of one connection of `root`.
    /// `root` 的一个连接的快照。
    pub fn snapshot(
        &self,
        root: SessionId,
        connection_id: &str,
    ) -> Result<SessionMcpConnectionSnapshot, String> {
        Ok(self.find(root, connection_id)?.snapshot())
    }

    /// Removes one connection of `root` and shuts its transport down.
    /// 移除 `root` 的一个连接并关闭其传输。
    pub async fn disconnect(
        &self,
        root: SessionId,
        connection_id: &str,
    ) -> Result<SessionMcpConnectionSnapshot, String> {
        let connection = {
            let mut partitions = lock(&self.partitions);
            let connections = partitions
                .get_mut(&root)
                .ok_or_else(|| missing_connection(connection_id))?;
            let index = connections
                .iter()
                .position(|connection| connection.id == connection_id)
                .ok_or_else(|| missing_connection(connection_id))?;
            let connection = connections.remove(index);
            if connections.is_empty() {
                partitions.remove(&root);
            }
            connection
        };
        let snapshot = connection.snapshot();
        connection.transport.shutdown().await.map_err(|error| {
            format!(
                "MCP connection `{connection_id}` was removed, but its server did not shut down cleanly: {error}"
            )
        })?;
        Ok(snapshot)
    }

    /// The tool list of one connection of `root`.
    /// `root` 的一个连接的工具列表。
    pub async fn tools(
        &self,
        root: SessionId,
        connection_id: &str,
        refresh: bool,
    ) -> Result<(SessionMcpConnectionSnapshot, Vec<McpToolSchema>), String> {
        let connection = self.find(root, connection_id)?;
        let tools = connection.tools(refresh).await?;
        Ok((connection.snapshot(), tools))
    }

    /// One tool of one connection of `root`.
    /// `root` 的一个连接中的一个工具。
    pub async fn tool(
        &self,
        root: SessionId,
        connection_id: &str,
        tool: &str,
    ) -> Result<(SessionMcpConnectionSnapshot, McpToolSchema), String> {
        let connection = self.find(root, connection_id)?;
        let schema = connection.tool(tool).await?;
        Ok((connection.snapshot(), schema))
    }

    /// Validates `arguments` against the tool's input schema and calls it.
    /// 按工具的输入 Schema 校验 `arguments` 并调用该工具。
    pub async fn call(
        &self,
        root: SessionId,
        connection_id: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        let connection = self.find(root, connection_id)?;
        let schema = connection.tool(tool).await?;
        validate_tool_arguments(&connection.name, &schema, &arguments)?;
        connection
            .transport
            .call_tool_with_audience(tool, arguments, McpToolResultAudience::Conversation)
            .await
            .map_err(|error| {
                let closed = connection
                    .ensure_open()
                    .err()
                    .map(|reason| format!(" {reason}"))
                    .unwrap_or_default();
                format!(
                    "MCP tool `{tool}` of `{}` failed: {error}.{closed}",
                    connection.name
                )
            })
    }

    /// Removes every connection of `root` and shuts them down; returns shutdown failures.
    /// 移除 `root` 的全部连接并关闭；返回关闭失败。
    pub async fn close_session(&self, root: SessionId) -> Vec<String> {
        let connections = lock(&self.partitions).remove(&root).unwrap_or_default();
        let results =
            futures_util::future::join_all(connections.iter().map(|connection| async move {
                connection
                    .transport
                    .shutdown()
                    .await
                    .map_err(|error| format!("MCP connection `{}`: {error}", connection.id))
            }))
            .await;
        results.into_iter().filter_map(Result::err).collect()
    }

    /// One connection of `root`.
    /// `root` 的一个连接。
    fn find(
        &self,
        root: SessionId,
        connection_id: &str,
    ) -> Result<Arc<SessionMcpConnection>, String> {
        lock(&self.partitions)
            .get(&root)
            .and_then(|connections| {
                connections
                    .iter()
                    .find(|connection| connection.id == connection_id)
            })
            .cloned()
            .ok_or_else(|| missing_connection(connection_id))
    }
}

/// Error for a connection the session does not hold.
/// 会话未持有某连接时的错误。
fn missing_connection(connection_id: &str) -> String {
    format!(
        "This session has no MCP connection `{connection_id}`. Connections end when they are disconnected or Vulcan Code restarts; list them with `mcp_connect` action `status`, or open a new one."
    )
}

/// Checks call arguments against one tool's input schema.
/// 按一个工具的输入 Schema 检查调用参数。
fn validate_tool_arguments(
    server_name: &str,
    schema: &McpToolSchema,
    arguments: &Value,
) -> Result<(), String> {
    if !arguments.is_object() {
        return Err(format!(
            "Arguments for MCP tool `{}` must be a JSON object.",
            schema.name
        ));
    }
    let validator = jsonschema::draft202012::new(&schema.args_schema).map_err(|_| {
        format!(
            "MCP tool `{server_name}/{}` has an input schema that cannot be compiled.",
            schema.name
        )
    })?;
    validator.validate(arguments).map_err(|error| {
        format!(
            "Arguments for MCP tool `{}` do not match its input schema: {error}. Read the schema with `mcp_tools` and `tool`.",
            schema.name
        )
    })
}

/// Follows transport events: a tool-list change marks the list stale, and a disconnect or a
/// closed event stream closes the connection. A disconnect also shuts the transport down, so a
/// transport that would reconnect on its own (legacy SSE) stops. Ends when the connection is dropped.
/// 跟随传输事件：工具列表变化将列表标记为过期，断开或事件流关闭则关闭连接。断开还会关闭传输，使会
/// 自行重连的传输（旧式 SSE）停止。连接释放时结束。
async fn follow_transport_events(
    connection: Weak<SessionMcpConnection>,
    mut events: broadcast::Receiver<McpTransportEvent>,
) {
    loop {
        let event = events.recv().await;
        let Some(connection) = connection.upgrade() else {
            return;
        };
        // The state lock is released before any await.
        // 状态锁在任何 await 之前释放。
        let disconnected = {
            let mut state = lock(&connection.state);
            match event {
                Ok(McpTransportEvent::ToolsChanged) | Err(RecvError::Lagged(_)) => {
                    state.tools_stale = true;
                    false
                }
                Ok(McpTransportEvent::Disconnected { reason }) => {
                    state.closed = Some(reason);
                    true
                }
                Ok(McpTransportEvent::PromptsChanged | McpTransportEvent::ResourcesChanged) => {
                    false
                }
                Err(RecvError::Closed) => {
                    state
                        .closed
                        .get_or_insert_with(|| String::from("the transport stopped"));
                    return;
                }
            }
        };
        if disconnected {
            // Session connections never reconnect; a failed shutdown leaves nothing to report to.
            // 会话连接绝不重连；关闭失败时没有可报告的对象。
            let _ = connection.transport.shutdown().await;
            return;
        }
    }
}

/// Locks one mutex; the guarded values stay consistent after a panic, so poison is ignored.
/// 锁定一个互斥锁；受保护的值在恐慌后仍保持一致，因此忽略中毒。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Current time in Unix milliseconds.
/// 当前时间的 Unix 毫秒值。
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
