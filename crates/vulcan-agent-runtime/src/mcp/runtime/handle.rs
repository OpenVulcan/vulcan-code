use super::*;

#[derive(Clone)]
pub(super) struct McpRuntimeHandle {
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
    /// Mutable OAuth-session cache keyed by server identifier for host-driven remote authorization flows.
    /// 以服务器标识为键保存的可变 OAuth 会话缓存，供宿主驱动的远端授权流程复用。
    auth_sessions: AsyncRwLock<BTreeMap<String, Arc<McpOAuthSession>>>,
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
    fn snapshot(&self) -> McpRuntimeSnapshot {
        self.inner
            .snapshot
            .read()
            .expect("mcp runtime snapshot poisoned")
            .clone()
    }

    /// Returns one stable MCP runtime overview for upper runtime surfaces.
    /// 返回供上层运行时表面消费的单份稳定 MCP 运行时概览。
    pub(super) fn overview(&self) -> McpRuntimeOverview {
        self.snapshot().overview()
    }

    /// Resolves the currently active transport for one connected server.
    /// 解析某个已连接服务器当前活动的传输实例。
    fn transport_for_server(&self, server_id: &str) -> Option<Arc<dyn McpTransport>> {
        self.inner
            .transports
            .read()
            .expect("mcp runtime transport map poisoned")
            .get(server_id)
            .cloned()
    }

    /// Resolves one configured server entry by identifier from the immutable registry snapshot.
    /// 从不可变注册表快照中根据标识解析单个已配置服务器条目。
    fn server_config(&self, server_id: &str) -> Option<McpServerConfig> {
        self.inner.registry.server_by_id(server_id).cloned()
    }

    /// Resolves or lazily creates one reusable OAuth session for the target configured remote MCP server.
    /// 为目标已配置远端 MCP 服务器解析或延迟创建一个可复用 OAuth 会话。
    async fn auth_session_for_server(
        &self,
        server_id: &str,
    ) -> Result<Option<Arc<McpOAuthSession>>, String> {
        if let Some(session) = self
            .inner
            .auth_sessions
            .read()
            .await
            .get(server_id)
            .cloned()
        {
            return Ok(Some(session));
        }

        let Some(server) = self.server_config(server_id) else {
            return Err(format!("未找到 MCP 服务器 `{server_id}`。"));
        };
        let Some(session) = build_server_auth_session(&server).await? else {
            return Ok(None);
        };
        self.inner
            .auth_sessions
            .write()
            .await
            .insert(server_id.to_string(), session.clone());
        Ok(Some(session))
    }

    /// Returns currently available MCP tool views filtered by one allowlist.
    /// 返回按单份白名单过滤后的当前可用 MCP 工具视图集合。
    pub(super) fn available_tool_views_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Vec<McpRuntimeToolView> {
        self.snapshot().filtered_tool_views(allowlist)
    }

    /// Returns currently available MCP tool bindings filtered by one allowlist.
    /// 返回按单份白名单过滤后的当前可用 MCP 工具绑定集合。
    pub(super) fn available_tool_bindings_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Vec<McpRuntimeToolBinding> {
        self.available_tool_views_filtered(allowlist)
            .into_iter()
            .map(|view| McpRuntimeToolBinding::new(view, self.clone()))
            .collect()
    }

    /// Calls one MCP tool through its stable host-facing tool identifier.
    /// 通过稳定宿主侧工具标识调用单个 MCP 工具。
    pub async fn call_tool_by_id(
        &self,
        tool_id: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let Some(descriptor) = self
            .snapshot()
            .tool_descriptors()
            .into_iter()
            .find(|tool| tool.qualified_name() == tool_id)
        else {
            return Err(format!("未找到 MCP 工具 `{tool_id}`。"));
        };
        let Some(transport) = self.transport_for_server(descriptor.server_id()) else {
            return Err(format!(
                "MCP 服务器 `{}` 当前未连接，无法调用工具 `{}`。",
                descriptor.server_id(),
                descriptor.tool_name()
            ));
        };
        transport
            .call_tool(descriptor.tool_name(), args)
            .await
            .map_err(|error| {
                format!(
                    "MCP 工具 `{}` 调用失败：{}。",
                    descriptor.tool_name(),
                    error
                )
            })
    }

    /// Returns the current OAuth status label for one configured MCP server.
    /// 返回单个已配置 MCP 服务器的当前 OAuth 状态标签。
    pub(super) async fn server_auth_status_label(&self, server_id: &str) -> Result<String, String> {
        let Some(session) = self.auth_session_for_server(server_id).await? else {
            return Ok(String::from("未配置 OAuth"));
        };
        Ok(session.auth_status().await.label().to_string())
    }

    /// Starts one host-driven OAuth authorization flow for the target MCP server.
    /// 为目标 MCP 服务器启动一条宿主驱动的 OAuth 授权流程。
    pub(super) async fn start_server_auth(
        &self,
        server_id: &str,
        client_name: &str,
    ) -> Result<McpAuthChallenge, String> {
        let Some(session) = self.auth_session_for_server(server_id).await? else {
            return Err(format!("MCP 服务器 `{server_id}` 未配置 OAuth。"));
        };
        session
            .start_authorization(client_name)
            .await
            .map_err(|error| error.to_string())
    }

    /// Finishes one host-driven OAuth authorization flow for the target MCP server.
    /// 为目标 MCP 服务器完成一条宿主驱动的 OAuth 授权流程。
    pub(super) async fn finish_server_auth(
        &self,
        server_id: &str,
        authorization_code: &str,
        oauth_state: &str,
    ) -> Result<McpAuthStatus, String> {
        let Some(session) = self.auth_session_for_server(server_id).await? else {
            return Err(format!("MCP 服务器 `{server_id}` 未配置 OAuth。"));
        };
        session
            .finish_authorization(authorization_code, oauth_state)
            .await
            .map_err(|error| error.to_string())
    }

    /// Removes every stored OAuth credential retained for the target MCP server.
    /// 移除目标 MCP 服务器当前保留的全部 OAuth 凭据。
    pub(super) async fn remove_server_auth(&self, server_id: &str) -> Result<(), String> {
        let Some(session) = self.auth_session_for_server(server_id).await? else {
            return Err(format!("MCP 服务器 `{server_id}` 未配置 OAuth。"));
        };
        session.remove_authorization().await;
        Ok(())
    }

    /// Requests all background MCP runtime tasks to stop and tear down their transports.
    /// 请求所有后台 MCP 运行时任务停止并拆除各自传输。
    pub(super) fn shutdown(&self) {
        let _ = self.inner.shutdown_tx.send(true);
    }
}

/// Boots one long-lived MCP runtime handle from the persisted provider configuration.
/// 根据持久化供应商配置引导一个长期 MCP 运行时句柄。
async fn bootstrap_mcp_runtime(
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
        auth_sessions: AsyncRwLock::new(BTreeMap::new()),
        snapshot: RwLock::new(snapshot),
        shutdown_tx,
    });

    for server in registry.enabled_servers() {
        spawn_server_runtime_task(inner.clone(), server.clone());
    }

    Ok(McpRuntimeHandle { inner })
}

/// Boots one stable MCP runtime access surface from the persisted provider configuration.
/// 根据持久化供应商配置引导一个稳定 MCP 运行时访问面。
pub async fn bootstrap_mcp_runtime_access(
    config: ProviderConfigurationFile,
) -> Result<McpRuntimeAccess, String> {
    let live_runtime = bootstrap_mcp_runtime(config.clone()).await?;
    Ok(McpRuntimeAccess::new(
        configured_mcp_registry_overview(&config),
        Some(live_runtime),
    ))
}

/// Builds one reusable host-owned OAuth session for a configured remote MCP server when the server exposes OAuth metadata.
/// 当服务器暴露 OAuth 元数据时，为已配置的远端 MCP 服务器构建一个可复用的宿主自有 OAuth 会话。
async fn build_server_auth_session(
    server: &McpServerConfig,
) -> Result<Option<Arc<McpOAuthSession>>, String> {
    let Some(oauth) = server.oauth.clone() else {
        return Ok(None);
    };
    let Some(uri) = server.url.clone() else {
        return Err(format!(
            "MCP 服务器 `{}` 配置了 OAuth，但缺少远端 URL。",
            server.name
        ));
    };

    let session = McpOAuthSession::new(uri, oauth.into())
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(Arc::new(session)))
}

/// Spawns one background lifecycle task for a single enabled MCP server.
/// 为单个已启用 MCP 服务器启动一个后台生命周期任务。
fn spawn_server_runtime_task(inner: Arc<McpRuntimeInner>, server: McpServerConfig) {
    tokio::spawn(async move {
        let mut shutdown_rx = inner.shutdown_tx.subscribe();
        let auth_session = match build_server_auth_session(&server).await {
            Ok(session) => session,
            Err(error) => {
                clear_server_runtime_state(&inner, &server.id, Some(error));
                return;
            }
        };
        update_server_auth_session_cache(&inner, &server.id, auth_session.clone()).await;

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
                result = inner.registry.spawn_transport(&server.id, auth_session.clone()) => result,
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

            let _ = refresh_server_prompts(&inner, &server, transport.clone()).await;
            let _ = refresh_server_resources(&inner, &server, transport.clone()).await;

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
                    Ok(McpTransportEvent::PromptsChanged) => {
                        let _ = refresh_server_prompts(&inner, &server, transport.clone()).await;
                    }
                    Ok(McpTransportEvent::ResourcesChanged) => {
                        let _ = refresh_server_resources(&inner, &server, transport.clone()).await;
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

/// Updates the shared OAuth-session cache for one server after startup or teardown decisions have been made.
/// 在完成启动或拆除决策后，更新单个服务器对应的共享 OAuth 会话缓存。
async fn update_server_auth_session_cache(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    auth_session: Option<Arc<McpOAuthSession>>,
) {
    let mut sessions = inner.auth_sessions.write().await;
    match auth_session {
        Some(session) => {
            sessions.insert(server_id.to_string(), session);
        }
        None => {
            sessions.remove(server_id);
        }
    }
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
        .map(|schema| {
            McpRuntimeToolDescriptor::new(
                server.id.clone(),
                schema.name,
                schema.description,
                schema.args_schema,
            )
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

/// Refreshes one server prompt snapshot after the connection is established or a prompt-list change arrives.
/// 在连接建立后或收到 prompt 列表变化通知时刷新单个服务器的 prompt 快照。
async fn refresh_server_prompts(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    transport: Arc<dyn McpTransport>,
) -> Result<(), String> {
    let prompt_count = match transport.list_prompts().await {
        Ok(prompts) => prompts.len(),
        Err(_) => return Ok(()),
    };

    update_server_prompt_count(inner, &server.id, prompt_count);
    Ok(())
}

/// Refreshes one server resource snapshot after the connection is established or a resource-list change arrives.
/// 在连接建立后或收到资源列表变化通知时刷新单个服务器的资源快照。
async fn refresh_server_resources(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    transport: Arc<dyn McpTransport>,
) -> Result<(), String> {
    let resource_count = match transport.list_resources().await {
        Ok(resources) => resources.len(),
        Err(_) => return Ok(()),
    };

    update_server_resource_count(inner, &server.id, resource_count);
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
    update_server_prompt_count(inner, server_id, 0);
    update_server_resource_count(inner, server_id, 0);
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
        if status == McpRuntimeServerStatus::Connected {
            server.cached_tools = tools.clone();
            server.tools = tools;
        } else {
            server.tools = tools;
        }
    }
}

/// Updates one server snapshot in-place with the latest prompt count.
/// 使用最新 prompt 数量原地更新单个服务器快照。
fn update_server_prompt_count(inner: &Arc<McpRuntimeInner>, server_id: &str, prompt_count: usize) {
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        server.prompt_count = prompt_count;
    }
}

/// Updates one server snapshot in-place with the latest resource count.
/// 使用最新资源数量原地更新单个服务器快照。
fn update_server_resource_count(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    resource_count: usize,
) {
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        server.resource_count = resource_count;
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
        cached_tools: Vec::new(),
        prompt_count: 0,
        resource_count: 0,
    }
}
