//! Session MCP tools: `mcp_connect` opens, inspects and closes temporary MCP connections of the
//! session, `mcp_tools` reads their tools and parameter schemas, and `mcp_call` calls one tool.
//! The tool list stays fixed however many servers the session connects.
//! 会话 MCP 工具：`mcp_connect` 打开、查看并关闭会话的临时 MCP 连接，`mcp_tools` 读取其工具与参数
//! Schema，`mcp_call` 调用一个工具。无论会话连接多少服务，工具列表都保持固定。

use super::*;
use crate::agent_runtime::task_runtime::TaskRuntime;
use crate::mcp::config::{redacted_remote_endpoint, redacted_stdio_program};
use crate::mcp::session::{
    SESSION_MCP_MAX_CONNECTIONS, SESSION_MCP_NAME_MAX_CHARS, SessionMcpConnectRequest,
    SessionMcpConnectionSnapshot, SessionMcpRegistry, SessionMcpTransportKind,
};
use std::sync::Weak;
use vulcan_mcp_runtime::{McpToolSchema, McpTransportLaunchSpec};

/// Stable identifier of the tool that manages session MCP connections.
/// 管理会话 MCP 连接的工具的稳定标识。
pub const MCP_CONNECT_TOOL_ID: &str = "mcp_connect";

/// Stable identifier of the tool that reads the tools of one session MCP connection.
/// 读取一个会话 MCP 连接工具的工具的稳定标识。
pub const MCP_TOOLS_TOOL_ID: &str = "mcp_tools";

/// Stable identifier of the tool that calls one tool of a session MCP connection.
/// 调用会话 MCP 连接中一个工具的工具的稳定标识。
pub const MCP_CALL_TOOL_ID: &str = "mcp_call";

/// Longest tool description line shown in a tool list, in Unicode scalar values.
/// 工具列表中显示的工具描述行的最大长度，按 Unicode 标量值计。
const TOOL_LIST_DESCRIPTION_MAX_CHARS: usize = 200;
/// Maximum number of configured tool summaries per model-facing page.
/// 面向模型的配置工具列表每页最多返回的摘要数。
const CONFIGURED_TOOL_PAGE_SIZE: usize = 40;

/// Session MCP access handed to one native tool call.
/// 交给一次原生工具调用的会话 MCP 访问。
#[derive(Clone)]
pub struct NativeSessionMcpAccess {
    /// Process-wide session connections.
    /// 进程级会话连接。
    registry: Arc<SessionMcpRegistry>,
    /// Task runtime that knows which sessions are spawned agents; weak, because it owns the registry.
    /// 知道哪些会话是派生代理的任务运行时；使用弱引用，因为它持有注册表。
    task_runtime: Weak<TaskRuntime>,
    /// Approved implicit bindings captured for this request's project and agent.
    /// 为本次请求所属项目与代理捕获的获准隐式绑定。
    configured_tools: Arc<[crate::mcp::McpRuntimeToolBinding]>,
}

impl fmt::Debug for NativeSessionMcpAccess {
    /// Renders a label without exposing connections.
    /// 渲染不暴露连接的标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeSessionMcpAccess")
            .finish_non_exhaustive()
    }
}

impl NativeSessionMcpAccess {
    /// Pairs the registry with the runtime that resolves root sessions.
    /// 将注册表与解析根会话的运行时配对。
    pub(crate) fn new(registry: Arc<SessionMcpRegistry>, task_runtime: Weak<TaskRuntime>) -> Self {
        Self {
            registry,
            task_runtime,
            configured_tools: Arc::from([]),
        }
    }

    /// Returns request-scoped MCP access with the configured bindings prepared for this project.
    /// 返回带本项目已准备配置绑定的请求级 MCP 访问。
    pub(crate) fn with_configured_tools(
        mut self,
        bindings: Vec<crate::mcp::McpRuntimeToolBinding>,
    ) -> Self {
        self.configured_tools = bindings.into();
        self
    }

    /// The root session whose connections `session_id` uses, and whether `session_id` is a
    /// spawned agent.
    /// `session_id` 使用其连接的根会话，以及 `session_id` 是否为派生代理。
    fn scope(&self, session_id: SessionId) -> Result<(SessionId, bool), NativeToolError> {
        let task_runtime = self
            .task_runtime
            .upgrade()
            .ok_or_else(|| NativeToolError::new("The task runtime is shutting down."))?;
        let root = task_runtime
            .root_session_of(session_id)
            .map_err(NativeToolError::new)?;
        Ok((root, root != session_id))
    }
}

/// Joins a stdio launch into the command line the permission gate reads.
/// 将 stdio 启动拼接为权限闸门读取的命令行。
pub(crate) fn session_mcp_command_line(command: &str, args: &[String]) -> String {
    std::iter::once(command)
        .chain(args.iter().map(String::as_str))
        .map(|part| {
            if part.is_empty() || part.chars().any(char::is_whitespace) {
                format!("\"{part}\"")
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds the model-visible `mcp_connect` tool.
/// 构建模型可见的 `mcp_connect` 工具。
pub(crate) fn build_mcp_connect_tool() -> Arc<NativeToolDefinition> {
    build_session_mcp_tool(
        MCP_CONNECT_TOOL_ID,
        format!(
            "Open a temporary MCP server connection for this session, without changing the user's configuration. Its tools are not added to your tool list: read them with `mcp_tools` and call them with `mcp_call`.\n\
- `connect`: start a stdio server (`command`, `args`, `cwd`) or reach an `http`/`sse` endpoint (`url`). Returns a `connection_id` and the server's tool names. Credentials come from Vulcan environment variables: stdio servers inherit them, and remote servers take `bearer_token_env` and `headers_from_env` by variable name. Never put a secret in the arguments; ask the user with `env` action `request` instead.\n\
- `status`: one connection with `connection_id`, or all of them.\n\
- `disconnect`: close a connection you no longer need.\n\
A session holds at most {SESSION_MCP_MAX_CONNECTIONS} connections. They end when Vulcan Code restarts and are never reconnected automatically. Subagents use the connections of their root session and cannot open or close them."
        ),
        false,
        mcp_connect_args_schema(),
        |request| Box::pin(async move { execute_mcp_connect(request).await }),
    )
}

/// Builds the model-visible `mcp_tools` tool.
/// 构建模型可见的 `mcp_tools` 工具。
pub(crate) fn build_mcp_tools_tool() -> Arc<NativeToolDefinition> {
    build_session_mcp_tool(
        MCP_TOOLS_TOOL_ID,
        String::from(
            "With no arguments, discover the configured MCP servers available to this request by name and ID. Pass one of server_name or server_id to list that server's tools, or connection_id for a temporary connection. Set tool to read one tool's description and schema. For configured servers, query filters tools and offset requests the next page.",
        ),
        true,
        json!({
            "type": "object",
            "properties": {
                "connection_id": { "type": "string", "description": "Connection returned by `mcp_connect`." },
                "server_id": { "type": "string", "description": "Configured MCP server ID advertised for this project." },
                "server_name": { "type": "string", "description": "Exact name of the configured MCP server to inspect." },
                "tool": { "type": "string", "description": "Tool whose schema to read; omit to list every tool." },
                "query": { "type": "string", "description": "Optional name or description search for configured MCP tools." },
                "offset": { "type": "integer", "minimum": 0, "description": "Zero-based offset from next_offset for configured MCP tools." },
                "refresh": { "type": "boolean", "description": "List the tools again from the server. Defaults to false." }
            },
            "additionalProperties": false
        }),
        |request| Box::pin(async move { execute_mcp_tools(request).await }),
    )
}

/// Builds the model-visible `mcp_call` tool.
/// 构建模型可见的 `mcp_call` 工具。
pub(crate) fn build_mcp_call_tool() -> Arc<NativeToolDefinition> {
    build_session_mcp_tool(
        MCP_CALL_TOOL_ID,
        String::from(
            "Call an exact MCP tool on a configured server (server_id) or temporary connection (connection_id). Read its schema with mcp_tools first; arguments must match the schema.",
        ),
        true,
        json!({
            "type": "object",
            "properties": {
                "connection_id": { "type": "string", "description": "Connection returned by `mcp_connect`." },
                "server_id": { "type": "string", "description": "Configured MCP server ID advertised for this project." },
                "tool": { "type": "string", "description": "Exact tool name on that server." },
                "arguments": { "type": "object", "description": "Tool arguments matching its input schema. Defaults to an empty object." }
            },
            "required": ["tool"],
            "oneOf": [{"required": ["connection_id"]}, {"required": ["server_id"]}],
            "additionalProperties": false
        }),
        |request| Box::pin(async move { execute_mcp_call(request).await }),
    )
}

/// Builds one session MCP tool definition.
/// 构建一个会话 MCP 工具定义。
fn build_session_mcp_tool(
    id: &str,
    description: String,
    available_in_plan: bool,
    args_schema: Value,
    handler: impl Fn(NativeToolExecutionRequest) -> NativeToolFuture + Send + Sync + 'static,
) -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        NativeToolDescriptor {
            id: id.to_string(),
            name: id.to_string(),
            description,
            help_text: None,
            ai_visible: true,
            available_in_plan,
        },
        args_schema,
        handler,
    )
}

/// JSON Schema of the `mcp_connect` arguments.
/// `mcp_connect` 参数的 JSON Schema。
fn mcp_connect_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["connect", "status", "disconnect"],
                "description": "Operation to perform."
            },
            "name": {
                "type": "string",
                "description": format!("connect: short name for the connection, unique in this session (at most {SESSION_MCP_NAME_MAX_CHARS} characters).")
            },
            "transport": {
                "type": "string",
                "enum": ["stdio", "http", "sse"],
                "description": "connect: how to reach the server."
            },
            "command": { "type": "string", "description": "connect stdio: program to run." },
            "args": {
                "type": "array",
                "items": { "type": "string" },
                "description": "connect stdio: program arguments."
            },
            "cwd": {
                "type": "string",
                "description": "connect stdio: working directory, relative to the workspace root. Defaults to the workspace root."
            },
            "url": { "type": "string", "description": "connect http/sse: endpoint URL." },
            "bearer_token_env": {
                "type": "string",
                "description": "connect http/sse: name of the environment variable holding a bearer token."
            },
            "headers_from_env": {
                "type": "object",
                "additionalProperties": { "type": "string" },
                "description": "connect http/sse: request headers, each mapped to the name of the environment variable holding its value."
            },
            "connection_id": { "type": "string", "description": "status/disconnect: the connection." }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

/// Arguments accepted by `mcp_connect`.
/// `mcp_connect` 接受的参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct McpConnectArgs {
    action: McpConnectAction,
    name: Option<String>,
    transport: Option<McpConnectTransport>,
    command: Option<String>,
    args: Option<Vec<String>>,
    cwd: Option<String>,
    url: Option<String>,
    bearer_token_env: Option<String>,
    headers_from_env: Option<BTreeMap<String, String>>,
    connection_id: Option<String>,
}

/// One action of `mcp_connect`.
/// `mcp_connect` 的一种动作。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum McpConnectAction {
    Connect,
    Status,
    Disconnect,
}

/// Transport named in `mcp_connect` arguments.
/// `mcp_connect` 参数中指定的传输。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum McpConnectTransport {
    Stdio,
    Http,
    Sse,
}

/// Arguments accepted by `mcp_tools`.
/// `mcp_tools` 接受的参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct McpToolsArgs {
    /// Session-owned temporary connection ID.
    /// 会话持有的临时连接 ID。
    connection_id: Option<String>,
    /// User-configured server ID.
    /// 用户配置服务器 ID。
    server_id: Option<String>,
    /// Exact display name of a configured server.
    /// 已配置服务器的精确显示名称。
    server_name: Option<String>,
    tool: Option<String>,
    refresh: Option<bool>,
    /// Optional case-insensitive configured tool search.
    /// 可选的不区分大小写的配置工具搜索。
    query: Option<String>,
    /// Zero-based configured tool page offset.
    /// 配置工具列表从零开始的分页偏移。
    offset: Option<usize>,
}

/// Arguments accepted by `mcp_call`.
/// `mcp_call` 接受的参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct McpCallArgs {
    /// Session-owned temporary connection ID.
    /// 会话持有的临时连接 ID。
    connection_id: Option<String>,
    /// User-configured server ID.
    /// 用户配置服务器 ID。
    server_id: Option<String>,
    tool: String,
    arguments: Option<Value>,
}

/// Resolves the configured-server envelope and passes only the original MCP arguments onward.
/// 解析已配置服务器调用封装，只向后传递原 MCP 参数。
pub(crate) fn configured_mcp_call(arguments: &Value) -> Option<(String, String, Value)> {
    let McpCallArgs {
        connection_id: None,
        server_id: Some(server_id),
        tool,
        arguments,
    } = serde_json::from_value(arguments.clone()).ok()?
    else {
        return None;
    };
    Some((server_id, tool, arguments.unwrap_or_else(|| json!({}))))
}

/// Parses one tool's arguments.
/// 解析一个工具的参数。
fn parse_args<T: serde::de::DeserializeOwned>(
    tool_id: &str,
    request: &NativeToolExecutionRequest,
) -> Result<T, NativeToolError> {
    serde_json::from_value(request.arguments.clone())
        .map_err(|error| NativeToolError::new(format!("Invalid `{tool_id}` arguments: {error}")))
}

/// The session access and the calling session of one request.
/// 一次请求的会话访问与调用会话。
fn session_of(
    tool_id: &str,
    request: &NativeToolExecutionRequest,
) -> Result<(NativeSessionMcpAccess, SessionId), NativeToolError> {
    let access = request.session_mcp.clone().ok_or_else(|| {
        NativeToolError::new("Session MCP connections are not available in this run.")
    })?;
    let session_id = request
        .session_id
        .ok_or_else(|| NativeToolError::new(format!("`{tool_id}` needs a session.")))?;
    Ok((access, session_id))
}

/// Executes one `mcp_connect` call.
/// 执行一次 `mcp_connect` 调用。
async fn execute_mcp_connect(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: McpConnectArgs = parse_args(MCP_CONNECT_TOOL_ID, &request)?;
    let (access, session_id) = session_of(MCP_CONNECT_TOOL_ID, &request)?;
    let (root, child) = access.scope(session_id)?;
    match args.action {
        McpConnectAction::Status => match args.connection_id {
            Some(connection_id) => {
                let snapshot = access
                    .registry
                    .snapshot(root, &connection_id)
                    .map_err(NativeToolError::new)?;
                Ok(session_mcp_result(
                    MCP_CONNECT_TOOL_ID,
                    connection_line(&snapshot),
                    json!({ "action": "status", "connections": [connection_payload(&snapshot)] }),
                ))
            }
            None => {
                let snapshots = access.registry.snapshots(root);
                let mut lines = vec![format!("Session MCP connections: {}", snapshots.len())];
                if snapshots.is_empty() {
                    lines.push(String::from("(none)"));
                }
                lines.extend(snapshots.iter().map(connection_line));
                Ok(session_mcp_result(
                    MCP_CONNECT_TOOL_ID,
                    lines.join("\n"),
                    json!({
                        "action": "status",
                        "connections": snapshots.iter().map(connection_payload).collect::<Vec<_>>(),
                    }),
                ))
            }
        },
        McpConnectAction::Disconnect => {
            if child {
                return Err(NativeToolError::new(
                    "Subagents cannot close MCP connections; the parent agent owns them.",
                ));
            }
            let connection_id = args
                .connection_id
                .ok_or_else(|| NativeToolError::new("`disconnect` requires `connection_id`."))?;
            let snapshot = access
                .registry
                .disconnect(root, &connection_id)
                .await
                .map_err(NativeToolError::new)?;
            Ok(session_mcp_result(
                MCP_CONNECT_TOOL_ID,
                format!("Disconnected `{}` ({}).", snapshot.id, snapshot.name),
                json!({ "action": "disconnect", "connections": [connection_payload(&snapshot)] }),
            ))
        }
        McpConnectAction::Connect => {
            if child {
                return Err(NativeToolError::new(
                    "Subagents cannot open MCP connections. Use a connection of the parent agent (see `status`), or ask the parent agent to open one.",
                ));
            }
            let connect = connect_request(&request, args).await?;
            let (snapshot, tools) = access
                .registry
                .connect(root, connect)
                .await
                .map_err(NativeToolError::new)?;
            let server = snapshot
                .server
                .as_ref()
                .map(|server| {
                    format!(
                        ", server {} {}, protocol {}",
                        server.product, server.version, server.protocol_version
                    )
                })
                .unwrap_or_default();
            let names = tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Ok(session_mcp_result(
                MCP_CONNECT_TOOL_ID,
                format!(
                    "Connected `{}` as `{}` ({}{server}).\nTools ({}): {}\nRead a tool's input schema with `mcp_tools` before calling it with `mcp_call`.",
                    snapshot.name,
                    snapshot.id,
                    snapshot.kind.key(),
                    tools.len(),
                    if names.is_empty() { "(none)" } else { &names },
                ),
                json!({ "action": "connect", "connections": [connection_payload(&snapshot)] }),
            ))
        }
    }
}

/// Validates `connect` arguments into one launch.
/// 将 `connect` 参数校验为一次启动。
async fn connect_request(
    request: &NativeToolExecutionRequest,
    args: McpConnectArgs,
) -> Result<SessionMcpConnectRequest, NativeToolError> {
    let name = args.name.as_deref().map(str::trim).unwrap_or_default();
    if name.is_empty() || name.chars().count() > SESSION_MCP_NAME_MAX_CHARS {
        return Err(NativeToolError::new(format!(
            "`connect` requires a `name` of 1 to {SESSION_MCP_NAME_MAX_CHARS} characters."
        )));
    }
    let name = name.to_string();
    let endpoint = format!("session-mcp:{name}");
    let transport = args
        .transport
        .ok_or_else(|| NativeToolError::new("`connect` requires `transport`."))?;
    let remote = matches!(
        transport,
        McpConnectTransport::Http | McpConnectTransport::Sse
    );
    if remote && (args.command.is_some() || args.args.is_some() || args.cwd.is_some()) {
        return Err(NativeToolError::new(
            "`command`, `args` and `cwd` apply to stdio servers only.",
        ));
    }
    if !remote
        && (args.url.is_some()
            || args.bearer_token_env.is_some()
            || args.headers_from_env.is_some())
    {
        return Err(NativeToolError::new(
            "`url`, `bearer_token_env` and `headers_from_env` apply to http and sse servers only.",
        ));
    }
    let variables = child_process_environment(request).await?;
    if !remote {
        let command = args.command.as_deref().map(str::trim).unwrap_or_default();
        if command.is_empty() {
            return Err(NativeToolError::new(
                "A stdio `connect` requires `command`.",
            ));
        }
        return Ok(SessionMcpConnectRequest {
            target: redacted_stdio_program(command),
            launch: McpTransportLaunchSpec::Stdio {
                endpoint,
                program: command.to_string(),
                args: args.args.unwrap_or_default(),
                cwd: stdio_working_directory(request, args.cwd.as_deref())?,
                env: variables.into_iter().collect(),
                inherit_environment: true,
            },
            name,
            kind: SessionMcpTransportKind::Stdio,
        });
    }
    let url = args.url.as_deref().map(str::trim).unwrap_or_default();
    let parsed = reqwest::Url::parse(url)
        .map_err(|error| NativeToolError::new(format!("Invalid `url`: {error}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(NativeToolError::new("`url` must use http or https."));
    }
    let lookup = |variable: &str| -> Result<String, NativeToolError> {
        variables
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(variable))
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var(variable).ok())
            .ok_or_else(|| {
                NativeToolError::new(format!(
                    "The environment variable `{variable}` is not set. Ask the user for it with `env` action `request`."
                ))
            })
    };
    let bearer_token = args.bearer_token_env.as_deref().map(lookup).transpose()?;
    let headers = args
        .headers_from_env
        .unwrap_or_default()
        .into_iter()
        .map(|(header, variable)| lookup(&variable).map(|value| (header, value)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let target = redacted_remote_endpoint(url);
    let (kind, launch) = match transport {
        McpConnectTransport::Sse => (
            SessionMcpTransportKind::Sse,
            McpTransportLaunchSpec::Sse {
                endpoint,
                uri: url.to_string(),
                bearer_token,
                headers,
                oauth: None,
            },
        ),
        _ => (
            SessionMcpTransportKind::Http,
            McpTransportLaunchSpec::Http {
                endpoint,
                uri: url.to_string(),
                bearer_token,
                headers,
                oauth: None,
            },
        ),
    };
    Ok(SessionMcpConnectRequest {
        name,
        kind,
        target,
        launch,
    })
}

/// Working directory of a stdio server: `cwd` against the workspace root, or the root itself.
/// stdio 服务的工作目录：相对工作区根目录解析 `cwd`，或使用根目录本身。
fn stdio_working_directory(
    request: &NativeToolExecutionRequest,
    cwd: Option<&str>,
) -> Result<Option<PathBuf>, NativeToolError> {
    let root = request.workspace_root.clone();
    match cwd.map(str::trim).filter(|cwd| !cwd.is_empty()) {
        None => Ok(root),
        Some(cwd) if Path::new(cwd).is_absolute() => Ok(Some(PathBuf::from(cwd))),
        Some(cwd) => root.map(|root| Some(root.join(cwd))).ok_or_else(|| {
            NativeToolError::new("A relative `cwd` needs a workspace; give an absolute path.")
        }),
    }
}

/// Executes one `mcp_tools` call.
/// 执行一次 `mcp_tools` 调用。
async fn execute_mcp_tools(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: McpToolsArgs = parse_args(MCP_TOOLS_TOOL_ID, &request)?;
    let (access, session_id) = session_of(MCP_TOOLS_TOOL_ID, &request)?;
    // Only a single selector is accepted, keeping temporary links separate from configured servers.
    // 仅接受单一选择器，使临时连接与已配置服务器保持独立。
    let connection_id = match (
        args.connection_id.as_deref(),
        args.server_id.as_deref(),
        args.server_name.as_deref(),
    ) {
        (None, Some(server), None) => return configured_tools_result(&access, server, &args),
        (None, None, Some(name)) => {
            let server = configured_server_id(&access, name)?;
            return configured_tools_result(&access, server, &args);
        }
        (Some(connection), None, None) => connection,
        (None, None, None)
            if args.tool.is_none()
                && args.query.is_none()
                && args.offset.is_none()
                && args.refresh.is_none() =>
        {
            return Ok(configured_servers_result(&access));
        }
        _ => {
            return Err(NativeToolError::new(
                "Omit arguments to discover servers, or specify one of server_name, server_id and connection_id.",
            ));
        }
    };
    if args.query.is_some() || args.offset.is_some() {
        return Err(NativeToolError::new(
            "query and offset require server_name or server_id.",
        ));
    }
    let (root, _) = access.scope(session_id)?;
    if let Some(tool) = args.tool {
        if args.refresh == Some(true) {
            access
                .registry
                .tools(root, connection_id, true)
                .await
                .map_err(NativeToolError::new)?;
        }
        let (snapshot, schema) = access
            .registry
            .tool(root, connection_id, &tool)
            .await
            .map_err(NativeToolError::new)?;
        return Ok(session_mcp_result(
            MCP_TOOLS_TOOL_ID,
            tool_detail_text(&snapshot, &schema),
            json!({ "connection_id": snapshot.id, "tool": schema.name }),
        ));
    }
    let (snapshot, tools) = access
        .registry
        .tools(root, connection_id, args.refresh.unwrap_or(false))
        .await
        .map_err(NativeToolError::new)?;
    let mut lines = vec![format!(
        "Tools of `{}` ({}): {}",
        snapshot.name,
        snapshot.id,
        tools.len()
    )];
    if tools.is_empty() {
        lines.push(String::from("(none)"));
    }
    for tool in &tools {
        let summary = tool
            .description
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default();
        let summary = if summary.chars().count() > TOOL_LIST_DESCRIPTION_MAX_CHARS {
            let cut: String = summary
                .chars()
                .take(TOOL_LIST_DESCRIPTION_MAX_CHARS)
                .collect();
            format!("{cut}…")
        } else {
            summary.to_string()
        };
        lines.push(if summary.is_empty() {
            format!("- {}", tool.name)
        } else {
            format!("- {}: {summary}", tool.name)
        });
    }
    lines.push(String::from(
        "Read one tool's input schema with `mcp_tools` and `tool` before calling it.",
    ));
    Ok(session_mcp_result(
        MCP_TOOLS_TOOL_ID,
        lines.join("\n"),
        json!({
            "connection_id": snapshot.id,
            "tools": tools.iter().map(|tool| tool.name.as_str()).collect::<Vec<_>>(),
        }),
    ))
}

/// Returns one name/ID pair per authorized live configured server, without listing its tools.
/// 每个已获准的实时配置服务器仅返回一组名称和 ID，无需列出工具。
fn configured_servers_result(access: &NativeSessionMcpAccess) -> VulcanToolInvocationResult {
    let servers = access
        .configured_tools
        .iter()
        .filter(|binding| binding.inspection_available())
        .filter_map(|binding| Some((binding.server_id()?, binding.server_name()?)))
        .collect::<BTreeMap<_, _>>();
    VulcanToolInvocationResult::from_model_output(json!({
        "servers": servers.into_iter().map(|(id, name)| json!({ "server_id": id, "server_name": name })).collect::<Vec<_>>(),
    }))
}

/// Resolves an exact display name in the approved request catalog and rejects duplicate names.
/// 在本次请求已获准的目录中解析精确显示名称，并拒绝同名歧义。
fn configured_server_id<'a>(
    access: &'a NativeSessionMcpAccess,
    name: &str,
) -> Result<&'a str, NativeToolError> {
    let mut matches = access
        .configured_tools
        .iter()
        .filter(|binding| binding.server_name() == Some(name) && binding.inspection_available())
        .filter_map(crate::mcp::McpRuntimeToolBinding::server_id);
    let id = matches.next().ok_or_else(|| {
        NativeToolError::new("This MCP server is not available to the current request.")
    })?;
    if matches.any(|other| other != id) {
        return Err(NativeToolError::new(
            "More than one available MCP server has this name. Use server_id.",
        ));
    }
    Ok(id)
}

/// Lists one page of approved tool summaries or reads one exact schema from the bound runtime.
/// 列出一页获准工具摘要，或从绑定运行时读取单个工具的精确 Schema。
fn configured_tools_result(
    access: &NativeSessionMcpAccess,
    server: &str,
    args: &McpToolsArgs,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    if args.refresh == Some(true) {
        return Err(NativeToolError::new(
            "refresh applies only to temporary connections.",
        ));
    }
    let mut tools = access
        .configured_tools
        .iter()
        .filter(|binding| binding.server_id() == Some(server) && binding.inspection_available())
        .collect::<Vec<_>>();
    if tools.is_empty() {
        return Err(NativeToolError::new(
            "This MCP server is not available to the current request.",
        ));
    }
    if let Some(name) = &args.tool {
        let schema = tools
            .iter()
            .find(|binding| binding.origin_tool_name() == Some(name))
            .and_then(|binding| binding.inspection())
            .ok_or_else(|| {
                NativeToolError::new("This MCP tool is not available to the current request.")
            })?;
        return Ok(VulcanToolInvocationResult::from_model_output(json!({
            "server_id": server, "tool": schema,
        })));
    }
    if let Some(query) = &args.query {
        let query = query.to_lowercase();
        tools.retain(|binding| {
            binding
                .origin_tool_name()
                .is_some_and(|name| name.to_lowercase().contains(&query))
                || binding.description().to_lowercase().contains(&query)
        });
    }
    tools.sort_by_key(|binding| binding.origin_tool_name());
    let offset = args.offset.unwrap_or(0);
    let next = offset.saturating_add(CONFIGURED_TOOL_PAGE_SIZE);
    let page = tools.iter().skip(offset).take(CONFIGURED_TOOL_PAGE_SIZE)
        .map(|binding| json!({
            "name": binding.origin_tool_name(),
            "description": binding.description().lines().map(str::trim).find(|line| !line.is_empty())
                .unwrap_or_default().chars().take(TOOL_LIST_DESCRIPTION_MAX_CHARS).collect::<String>(),
        })).collect::<Vec<_>>();
    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "server_id": server, "tools": page, "total": tools.len(),
        "next_offset": (next < tools.len()).then_some(next),
    })))
}

/// Full description and schemas of one tool.
/// 一个工具的完整描述与 Schema。
fn tool_detail_text(snapshot: &SessionMcpConnectionSnapshot, schema: &McpToolSchema) -> String {
    let pretty = |value: &Value| serde_json::to_string_pretty(value).unwrap_or_default();
    let mut text = format!(
        "Tool `{}` of `{}` ({})\n",
        schema.name, snapshot.name, snapshot.id
    );
    if !schema.description.trim().is_empty() {
        text.push_str(schema.description.trim());
        text.push('\n');
    }
    text.push_str(&format!(
        "Input schema:\n```json\n{}\n```",
        pretty(&schema.args_schema)
    ));
    if !schema.result_schema.is_null() {
        text.push_str(&format!(
            "\nOutput schema:\n```json\n{}\n```",
            pretty(&schema.result_schema)
        ));
    }
    text
}

/// Executes one `mcp_call` call.
/// 执行一次 `mcp_call` 调用。
async fn execute_mcp_call(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: McpCallArgs = parse_args(MCP_CALL_TOOL_ID, &request)?;
    let (access, session_id) = session_of(MCP_CALL_TOOL_ID, &request)?;
    let arguments = args.arguments.unwrap_or_else(|| json!({}));
    if args.server_id.is_some() {
        return Err(NativeToolError::new(
            "This configured MCP tool is not available to the current request.",
        ));
    }
    let connection_id = args
        .connection_id
        .as_deref()
        .filter(|_| args.server_id.is_none())
        .ok_or_else(|| {
            NativeToolError::new("Specify exactly one of server_id and connection_id.")
        })?;
    let (root, _) = access.scope(session_id)?;
    let output = access
        .registry
        .call(root, connection_id, &args.tool, arguments)
        .await
        .map_err(NativeToolError::new)?;
    Ok(VulcanToolInvocationResult::from_model_output(output))
}

/// One status line of a connection.
/// 一个连接的状态行。
fn connection_line(snapshot: &SessionMcpConnectionSnapshot) -> String {
    let state = match &snapshot.closed {
        Some(reason) => format!("closed: {reason}"),
        None => String::from("connected"),
    };
    format!(
        "- {} `{}` ({}: {}, {} tools, {state})",
        snapshot.id,
        snapshot.name,
        snapshot.kind.key(),
        snapshot.target,
        snapshot.tool_count
    )
}

/// Host payload of one connection.
/// 一个连接的宿主载荷。
fn connection_payload(snapshot: &SessionMcpConnectionSnapshot) -> Value {
    json!({
        "id": snapshot.id,
        "name": snapshot.name,
        "transport": snapshot.kind.key(),
        "target": snapshot.target,
        "toolCount": snapshot.tool_count,
        "connectedAtMs": snapshot.connected_at_ms,
        "closed": snapshot.closed,
    })
}

/// Wraps one text result with its host payload.
/// 为一条文本结果附上宿主载荷。
fn session_mcp_result(kind: &str, text: String, payload: Value) -> VulcanToolInvocationResult {
    VulcanToolInvocationResult::from_model_output(Value::String(text)).with_host_result(
        VulcanToolHostResult {
            kind: kind.to_string(),
            payload,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Arguments with whitespace stay one word, so high-risk rules see the real command line.
    /// 含空白的参数保持为一个词，使高风险规则看到真实命令行。
    #[test]
    fn command_line_keeps_arguments_whole() {
        assert_eq!(
            session_mcp_command_line("npx", &[String::from("-y"), String::from("my server")]),
            "npx -y \"my server\""
        );
    }
}
