//! Long-lived MCP runtime manager that keeps server connections hot for tool registration.
//! 用于保持服务器长连接并支撑工具注册的长期 MCP 运行时管理器。

use crate::mcp::config::McpServerConfig;
use crate::mcp::registry::McpServerRegistry;
use crate::provider_config::ProviderConfigurationFile;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::watch;
use vulcan_mcp_runtime::{McpTransport, McpTransportEvent};

/// Stable MCP tool schema exposed to the host runtime and execution planner.
/// 暴露给宿主运行时与执行规划器的稳定 MCP 工具 Schema。
#[derive(Debug, Clone)]
pub struct McpRuntimeToolSchema {
    /// Stable MCP tool name.
    /// 稳定 MCP 工具名称。
    pub name: String,
    /// Human-readable MCP tool description.
    /// 人类可读的 MCP 工具描述。
    pub description: String,
    /// JSON Schema describing the tool arguments.
    /// 描述工具参数的 JSON Schema。
    pub args_schema: serde_json::Value,
}

/// Stable MCP tool descriptor exposed to the host runtime and execution planner.
/// 暴露给宿主运行时与执行规划器的稳定 MCP 工具描述。
#[derive(Debug, Clone)]
pub struct McpRuntimeToolDescriptor {
    /// Stable server identifier that currently owns the tool.
    /// 当前拥有该工具的稳定服务器标识。
    pub server_id: String,
    /// Tool schema discovered from the remote MCP endpoint.
    /// 从远端 MCP 端点发现得到的工具 schema。
    pub schema: McpRuntimeToolSchema,
}

/// Runtime connectivity state of one configured MCP server.
/// 某个已配置 MCP 服务器的运行时连通状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpRuntimeServerStatus {
    /// Server is disabled in configuration and therefore does not participate in runtime registration.
    /// 服务器在配置中被禁用，因此不参与运行时注册。
    Disabled,
    /// Server is enabled and currently attempting to establish a transport connection.
    /// 服务器已启用，当前正在尝试建立传输连接。
    Connecting,
    /// Server is enabled and currently has an active transport connection.
    /// 服务器已启用，当前拥有活动传输连接。
    Connected,
    /// Server is enabled but currently disconnected or unavailable.
    /// 服务器已启用，但当前处于断开或不可用状态。
    Disconnected,
}

impl McpRuntimeServerStatus {
    /// Returns a compact human-readable status label.
    /// 返回紧凑的人类可读状态标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "禁用",
            Self::Connecting => "连接中",
            Self::Connected => "在线",
            Self::Disconnected => "离线",
        }
    }
}

/// Runtime snapshot of one configured MCP server.
/// 某个已配置 MCP 服务器的运行时快照。
#[derive(Debug, Clone)]
pub struct McpRuntimeServerSnapshot {
    /// Human-readable server name.
    /// 人类可读服务器名称。
    pub name: String,
    /// Whether the server is enabled in configuration.
    /// 该服务器在配置中是否启用。
    pub enabled: bool,
    /// Transport-family label used for diagnostics.
    /// 用于诊断的传输类型标签。
    pub transport_label: String,
    /// Compact endpoint or command summary shown in the workbench.
    /// 展示在工作台中的紧凑端点或命令摘要。
    pub endpoint_summary: String,
    /// Current runtime connectivity status.
    /// 当前运行时连通状态。
    pub status: McpRuntimeServerStatus,
    /// Latest connection or refresh warning, if any.
    /// 最近一次连接或刷新警告（如果存在）。
    pub last_error: Option<String>,
    /// Currently registered tool descriptors for this server.
    /// 当前为该服务器注册的工具描述集合。
    pub tools: Vec<McpRuntimeToolDescriptor>,
}

/// Aggregated MCP runtime snapshot consumed by UI summaries and execution planning.
/// 供界面摘要与执行规划消费的聚合 MCP 运行时快照。
#[derive(Debug, Clone, Default)]
pub struct McpRuntimeSnapshot {
    /// Server snapshots keyed by stable server identifier.
    /// 以稳定服务器标识为键保存的服务器快照集合。
    pub servers: BTreeMap<String, McpRuntimeServerSnapshot>,
}

impl McpRuntimeSnapshot {
    /// Returns lightweight server summary rows for UI surfaces.
    /// 返回供界面表面使用的轻量服务器摘要行。
    pub fn summary_rows(&self) -> Vec<String> {
        self.servers
            .values()
            .map(|server| {
                let base = format!(
                    "{} · {} · {} · {}",
                    server.status.label(),
                    server.name,
                    server.transport_label,
                    server.endpoint_summary
                );
                match &server.last_error {
                    Some(error) => format!("{base} · {error}"),
                    None => base,
                }
            })
            .collect()
    }

    /// Returns one flat tool-descriptor list for all currently connected servers.
    /// 返回当前所有已连接服务器的扁平工具描述列表。
    pub fn tool_descriptors(&self) -> Vec<McpRuntimeToolDescriptor> {
        self.servers
            .values()
            .filter(|server| server.status == McpRuntimeServerStatus::Connected)
            .flat_map(|server| server.tools.clone())
            .collect()
    }
}

/// Shared host-side handle used to inspect active MCP tools and resolve live transports at call time.
/// 用于检查活动 MCP 工具并在调用时解析实时传输的共享宿主句柄。
#[derive(Clone)]
pub struct McpRuntimeHandle {
    /// Shared mutable inner state.
    /// 共享可变内部状态。
    inner: Arc<McpRuntimeInner>,
}

impl std::fmt::Debug for McpRuntimeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpRuntimeHandle").finish_non_exhaustive()
    }
}

/// Shared mutable state behind one long-lived MCP runtime handle.
/// 单个长期 MCP 运行时句柄背后的共享可变状态。
struct McpRuntimeInner {
    /// Runtime registry containing the configured MCP server set.
    /// 包含已配置 MCP 服务器集合的运行时注册表。
    registry: McpServerRegistry,
    /// Mutable transport map keyed by server identifier.
    /// 以服务器标识为键保存的可变传输映射。
    transports: RwLock<BTreeMap<String, Arc<dyn McpTransport>>>,
    /// Mutable snapshot published to the host runtime.
    /// 发布给宿主运行时的可变快照。
    snapshot: RwLock<McpRuntimeSnapshot>,
    /// Shared shutdown signal observed by all background MCP runtime tasks.
    /// 被所有后台 MCP 运行时任务共同观察的关闭信号。
    shutdown_tx: watch::Sender<bool>,
}

impl McpRuntimeHandle {
    /// Returns the latest immutable runtime snapshot.
    /// 返回最新的不可变运行时快照。
    pub fn snapshot(&self) -> McpRuntimeSnapshot {
        self.inner
            .snapshot
            .read()
            .expect("mcp runtime snapshot poisoned")
            .clone()
    }

    /// Resolves the currently active transport for one connected server.
    /// 解析某个已连接服务器当前活动的传输实例。
    pub fn transport_for_server(&self, server_id: &str) -> Option<Arc<dyn McpTransport>> {
        self.inner
            .transports
            .read()
            .expect("mcp runtime transport map poisoned")
            .get(server_id)
            .cloned()
    }

    /// Returns the currently available MCP tool descriptors across all connected servers.
    /// 返回当前所有已连接服务器的可用 MCP 工具描述集合。
    pub fn available_tool_descriptors(&self) -> Vec<McpRuntimeToolDescriptor> {
        self.snapshot().tool_descriptors()
    }

    /// Requests all background MCP runtime tasks to stop and tear down their transports.
    /// 请求所有后台 MCP 运行时任务停止并拆除各自传输。
    pub fn shutdown(&self) {
        let _ = self.inner.shutdown_tx.send(true);
    }
}

/// Boots one long-lived MCP runtime handle from the persisted provider configuration.
/// 根据持久化供应商配置引导一个长期 MCP 运行时句柄。
pub async fn bootstrap_mcp_runtime(
    config: ProviderConfigurationFile,
) -> Result<McpRuntimeHandle, String> {
    let registry = McpServerRegistry::from_provider_configuration(&config);
    let (shutdown_tx, _) = watch::channel(false);
    let snapshot = McpRuntimeSnapshot {
        servers: registry
            .servers()
            .iter()
            .map(|server| (server.id.clone(), initial_server_snapshot(server)))
            .collect(),
    };
    let inner = Arc::new(McpRuntimeInner {
        registry: registry.clone(),
        transports: RwLock::new(BTreeMap::new()),
        snapshot: RwLock::new(snapshot),
        shutdown_tx,
    });

    for server in registry.enabled_servers() {
        spawn_server_runtime_task(inner.clone(), server.clone());
    }

    Ok(McpRuntimeHandle { inner })
}

/// Spawns one background lifecycle task for a single enabled MCP server.
/// 为单个已启用 MCP 服务器启动一个后台生命周期任务。
fn spawn_server_runtime_task(inner: Arc<McpRuntimeInner>, server: McpServerConfig) {
    tokio::spawn(async move {
        let mut shutdown_rx = inner.shutdown_tx.subscribe();

        loop {
            if *shutdown_rx.borrow() {
                clear_server_runtime_state(&inner, &server.id, None);
                break;
            }

            update_server_status(
                &inner,
                &server.id,
                McpRuntimeServerStatus::Connecting,
                None,
                Vec::new(),
            );

            let transport_result = tokio::select! {
                _ = shutdown_rx.changed() => {
                    clear_server_runtime_state(&inner, &server.id, None);
                    break;
                }
                result = inner.registry.spawn_transport(&server.id) => result,
            };

            let transport = match transport_result {
                Ok(transport) => transport,
                Err(error) => {
                    clear_server_runtime_state(&inner, &server.id, Some(error));
                    if wait_for_retry_or_shutdown(&mut shutdown_rx).await {
                        clear_server_runtime_state(&inner, &server.id, None);
                        break;
                    }
                    continue;
                }
            };

            if let Err(error) = refresh_server_tools(&inner, &server, transport.clone()).await {
                clear_server_runtime_state(&inner, &server.id, Some(error));
                if wait_for_retry_or_shutdown(&mut shutdown_rx).await {
                    clear_server_runtime_state(&inner, &server.id, None);
                    break;
                }
                continue;
            }

            let mut event_rx = transport.event_receiver();
            loop {
                let event = tokio::select! {
                    _ = shutdown_rx.changed() => {
                        clear_server_runtime_state(&inner, &server.id, None);
                        return;
                    }
                    result = event_rx.recv() => result,
                };

                match event {
                    Ok(McpTransportEvent::ToolsChanged) => {
                        if let Err(error) =
                            refresh_server_tools(&inner, &server, transport.clone()).await
                        {
                            clear_server_runtime_state(&inner, &server.id, Some(error));
                            break;
                        }
                    }
                    Ok(McpTransportEvent::Disconnected { reason }) => {
                        clear_server_runtime_state(&inner, &server.id, Some(reason));
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        clear_server_runtime_state(
                            &inner,
                            &server.id,
                            Some(format!("MCP 服务器 `{}` 的事件通道已关闭。", server.name)),
                        );
                        break;
                    }
                }
            }

            if wait_for_retry_or_shutdown(&mut shutdown_rx).await {
                clear_server_runtime_state(&inner, &server.id, None);
                break;
            }
        }
    });
}

/// Waits for a short reconnect backoff unless the runtime has already been asked to shut down.
/// 在运行时已经收到关闭请求前提下提前退出，否则等待一个简短的重连退避窗口。
async fn wait_for_retry_or_shutdown(shutdown_rx: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = shutdown_rx.changed() => true,
        _ = tokio::time::sleep(Duration::from_secs(2)) => false,
    }
}

/// Refreshes one server tool snapshot after the connection is established or a tool-list change arrives.
/// 在连接建立后或收到工具列表变化通知时刷新单个服务器的工具快照。
async fn refresh_server_tools(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    transport: Arc<dyn McpTransport>,
) -> Result<(), String> {
    let schemas = transport.list_tools().await.map_err(|error| {
        format!(
            "刷新 MCP 服务器 `{}` 工具列表失败：{}。",
            server.name, error
        )
    })?;
    let tools = schemas
        .into_iter()
        .map(|schema| McpRuntimeToolDescriptor {
            server_id: server.id.clone(),
            schema: McpRuntimeToolSchema {
                name: schema.name,
                description: schema.description,
                args_schema: schema.args_schema,
            },
        })
        .collect::<Vec<_>>();

    inner
        .transports
        .write()
        .expect("mcp runtime transport map poisoned")
        .insert(server.id.clone(), transport);
    update_server_status(
        inner,
        &server.id,
        McpRuntimeServerStatus::Connected,
        None,
        tools,
    );
    Ok(())
}

/// Clears one server transport registration and marks it as disconnected in the shared runtime snapshot.
/// 清除单个服务器的传输注册，并在共享运行时快照中将其标记为断开状态。
fn clear_server_runtime_state(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    error: Option<String>,
) {
    inner
        .transports
        .write()
        .expect("mcp runtime transport map poisoned")
        .remove(server_id);
    update_server_status(
        inner,
        server_id,
        McpRuntimeServerStatus::Disconnected,
        error,
        Vec::new(),
    );
}

/// Updates one server snapshot in-place with the latest runtime status, warning, and tool set.
/// 使用最新运行时状态、警告与工具集合原地更新单个服务器快照。
fn update_server_status(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    status: McpRuntimeServerStatus,
    error: Option<String>,
    tools: Vec<McpRuntimeToolDescriptor>,
) {
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        server.status = status;
        server.last_error = error;
        server.tools = tools;
    }
}

/// Builds the initial snapshot for one configured MCP server before any connection attempt begins.
/// 在任何连接尝试开始之前，为单个已配置 MCP 服务器构建初始快照。
fn initial_server_snapshot(server: &McpServerConfig) -> McpRuntimeServerSnapshot {
    McpRuntimeServerSnapshot {
        name: server.name.clone(),
        enabled: server.enabled,
        transport_label: server.transport.label().to_string(),
        endpoint_summary: server.command_summary(),
        status: if server.enabled {
            McpRuntimeServerStatus::Connecting
        } else {
            McpRuntimeServerStatus::Disabled
        },
        last_error: None,
        tools: Vec::new(),
    }
}
