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
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; the shutdown state is latched for current and future server-task receivers.
    /// 不返回值；关闭状态会为当前及未来服务器任务接收端锁存。
    pub(super) fn shutdown(&self) {
        request_mcp_runtime_shutdown(&self.inner.shutdown_tx);
    }
}

/// Latches one MCP runtime shutdown request for current and future watch receivers.
/// 为当前及未来 watch 接收端锁存一次 MCP 运行时关闭请求。
///
/// # Parameters
/// 参数
/// `shutdown_tx` owns the single monotonic shutdown state shared by every server task.
/// `shutdown_tx` 持有全部服务器任务共享的单调关闭状态。
///
/// # Returns
/// 返回值
/// Returns no value; repeated requests keep the channel in the requested state.
/// 不返回值；重复请求会使通道继续保持已请求状态。
fn request_mcp_runtime_shutdown(shutdown_tx: &watch::Sender<bool>) {
    shutdown_tx.send_replace(true);
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
///
/// # Parameters
/// 参数
/// `inner` owns the shared registry, snapshots, transports, auth cache, and shutdown latch.
/// `inner` 持有共享注册表、快照、传输、认证缓存与关闭锁存状态。
/// `server` is the exact enabled server whose connection lifecycle this task owns.
/// `server` 是本任务持有连接生命周期的精确已启用服务器。
///
/// # Returns
/// 返回值
/// Returns no handle; the long-lived task exits through the shared latched shutdown state or terminal setup failure.
/// 不返回句柄；长期任务通过共享锁存关闭状态或终止性初始化失败退出。
fn spawn_server_runtime_task(inner: Arc<McpRuntimeInner>, server: McpServerConfig) {
    tokio::spawn(async move {
        let mut shutdown_rx = inner.shutdown_tx.subscribe();
        if *shutdown_rx.borrow() {
            clear_server_runtime_state(&inner, &server.id, None);
            return;
        }
        // authSessionResult remains interruptible while remote OAuth metadata and registration are prepared.
        // authSessionResult 在远端 OAuth 元数据与注册准备期间保持可被关闭信号中断。
        let auth_session_result = tokio::select! {
            _ = shutdown_rx.changed() => {
                clear_server_runtime_state(&inner, &server.id, None);
                return;
            }
            result = build_server_auth_session(&server) => result,
        };
        let auth_session = match auth_session_result {
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

            refresh_server_optional_capabilities(&inner, &server, transport.clone()).await;

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
                        refresh_server_optional_capabilities(&inner, &server, transport.clone())
                            .await;
                    }
                    Ok(McpTransportEvent::PromptsChanged)
                    | Ok(McpTransportEvent::ResourcesChanged) => {
                        refresh_server_optional_capabilities(&inner, &server, transport.clone())
                            .await;
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

/// Combined prompt/resource refresh result ready to apply to one connected server snapshot.
/// 可应用到单个已连接服务器快照的 prompt/resource 组合刷新结果。
#[derive(Debug, PartialEq, Eq)]
struct McpOptionalCapabilityRefreshOutcome {
    /// Latest prompt count when prompt discovery succeeded; failure preserves the prior snapshot count.
    /// prompt 发现成功时的最新计数；失败时保留快照中的既有计数。
    prompt_count: Option<usize>,
    /// Latest resource count when resource discovery succeeded; failure preserves the prior snapshot count.
    /// resource 发现成功时的最新计数；失败时保留快照中的既有计数。
    resource_count: Option<usize>,
    /// Combined current refresh warning, or `None` when both capabilities refreshed successfully.
    /// 当前组合刷新警告；两项能力均刷新成功时为 `None`。
    warning: Option<String>,
}

/// Builds one optional-capability outcome without converting failed discovery into zero counts.
/// 构建一份可选能力刷新结果，且不把发现失败转换成零计数。
///
/// # Parameters
/// 参数
/// `prompt_result` contains the current prompt count or its exact refresh failure.
/// `prompt_result` 包含当前 prompt 计数或其精确刷新失败。
/// `resource_result` contains the current resource count or its exact refresh failure.
/// `resource_result` 包含当前 resource 计数或其精确刷新失败。
///
/// # Returns
/// 返回值
/// Successful counts plus every current failure combined into one optional warning.
/// 返回成功计数以及合并全部当前失败后的可选警告。
fn build_server_optional_capability_refresh_outcome(
    prompt_result: Result<usize, String>,
    resource_result: Result<usize, String>,
) -> McpOptionalCapabilityRefreshOutcome {
    // errors retains every independent capability failure for one observable snapshot warning.
    // errors 保留每项独立能力失败，以生成单条可观察快照警告。
    let mut errors = Vec::new();
    // promptCount updates the snapshot only when prompt discovery returned authoritative data.
    // promptCount 仅在 prompt 发现返回权威数据时更新快照。
    let prompt_count = match prompt_result {
        Ok(count) => Some(count),
        Err(error) => {
            errors.push(error);
            None
        }
    };
    // resourceCount updates the snapshot only when resource discovery returned authoritative data.
    // resourceCount 仅在 resource 发现返回权威数据时更新快照。
    let resource_count = match resource_result {
        Ok(count) => Some(count),
        Err(error) => {
            errors.push(error);
            None
        }
    };
    // warning is absent only when both current capability refreshes succeeded.
    // warning 仅在当前两项能力刷新均成功时缺席。
    let warning = if errors.is_empty() {
        None
    } else {
        Some(errors.join(" "))
    };
    McpOptionalCapabilityRefreshOutcome {
        prompt_count,
        resource_count,
        warning,
    }
}

/// Refreshes both optional MCP capability counts and publishes their current warning state.
/// 刷新两项 MCP 可选能力计数并发布其当前警告状态。
///
/// # Parameters
/// 参数
/// `inner` owns the connected server snapshot being updated.
/// `inner` 持有需要更新的已连接服务器快照。
/// `server` supplies stable server identity and diagnostic context.
/// `server` 提供稳定服务器身份与诊断上下文。
/// `transport` is the active transport used for both discovery requests.
/// `transport` 是两项发现请求使用的活动传输。
///
/// # Returns
/// 返回值
/// Returns no value; successful counts and combined refresh warnings are committed to the snapshot.
/// 不返回值；成功计数与组合刷新警告会提交到快照。
async fn refresh_server_optional_capabilities(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    transport: Arc<dyn McpTransport>,
) {
    // promptResult and resourceResult are independent current capability observations.
    // promptResult 与 resourceResult 是两项相互独立的当前能力观察结果。
    let (prompt_result, resource_result) = tokio::join!(
        load_server_prompt_count(server, transport.as_ref()),
        load_server_resource_count(server, transport.as_ref()),
    );
    // outcome preserves successful counts and every failure without zero-value fallback.
    // outcome 保留成功计数与全部失败，不使用零值兜底。
    let outcome = build_server_optional_capability_refresh_outcome(prompt_result, resource_result);
    apply_server_optional_capability_refresh_outcome(inner, &server.id, outcome);
}

/// Loads the current prompt count for one server or returns its exact discovery failure.
/// 加载单个服务器的当前 prompt 计数，失败时返回精确发现错误。
///
/// # Parameters
/// 参数
/// `server` supplies the stable human-readable server identity for diagnostics.
/// `server` 提供诊断使用的稳定人类可读服务器身份。
/// `transport` is the active transport queried for prompt discovery.
/// `transport` 是用于 prompt 发现的活动传输。
///
/// # Returns
/// 返回值
/// The authoritative prompt count, or a contextualized transport/protocol error.
/// 返回权威 prompt 计数，或带上下文的传输/协议错误。
async fn load_server_prompt_count(
    server: &McpServerConfig,
    transport: &dyn McpTransport,
) -> Result<usize, String> {
    transport
        .list_prompts()
        .await
        .map(|prompts| prompts.len())
        .map_err(|error| {
            format!(
                "刷新 MCP 服务器 `{}` prompt 列表失败：{error}。",
                server.name
            )
        })
}

/// Loads the current resource count for one server or returns its exact discovery failure.
/// 加载单个服务器的当前 resource 计数，失败时返回精确发现错误。
///
/// # Parameters
/// 参数
/// `server` supplies the stable human-readable server identity for diagnostics.
/// `server` 提供诊断使用的稳定人类可读服务器身份。
/// `transport` is the active transport queried for resource discovery.
/// `transport` 是用于 resource 发现的活动传输。
///
/// # Returns
/// 返回值
/// The authoritative resource count, or a contextualized transport/protocol error.
/// 返回权威 resource 计数，或带上下文的传输/协议错误。
async fn load_server_resource_count(
    server: &McpServerConfig,
    transport: &dyn McpTransport,
) -> Result<usize, String> {
    transport
        .list_resources()
        .await
        .map(|resources| resources.len())
        .map_err(|error| {
            format!(
                "刷新 MCP 服务器 `{}` resource 列表失败：{error}。",
                server.name
            )
        })
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

/// Applies one complete optional-capability refresh outcome atomically to a connected server snapshot.
/// 将一份完整可选能力刷新结果原子应用到已连接服务器快照。
///
/// # Parameters
/// 参数
/// `inner` owns the shared runtime snapshot.
/// `inner` 持有共享运行时快照。
/// `server_id` identifies the exact server snapshot to update.
/// `server_id` 标识需要更新的精确服务器快照。
/// `outcome` contains every successful current count and the combined current warning.
/// `outcome` 包含全部成功的当前计数与组合当前警告。
///
/// # Returns
/// 返回值
/// Returns no value; a missing server entry remains unchanged.
/// 不返回值；服务器条目缺失时保持不变。
fn apply_server_optional_capability_refresh_outcome(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    outcome: McpOptionalCapabilityRefreshOutcome,
) {
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        if let Some(prompt_count) = outcome.prompt_count {
            server.prompt_count = prompt_count;
        }
        if let Some(resource_count) = outcome.resource_count {
            server.resource_count = resource_count;
        }
        server.last_error = outcome.warning;
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

#[cfg(test)]
mod optional_capability_tests {
    //! MCP optional-capability refresh outcome tests.
    //! MCP 可选能力刷新结果测试。

    use super::*;

    /// Verifies shutdown remains observable when it is requested before any server task subscribes.
    /// 验证关闭请求发生在任何服务器任务订阅前时，未来仍可观察该请求。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails if a zero-receiver request is lost or repetition reopens the channel.
    /// 此测试无返回值；若零接收端请求丢失或重复请求重新打开通道则测试失败。
    #[test]
    fn mcp_shutdown_request_is_latched_without_receivers() {
        // shutdownTx owns the monotonic value while initialReceiver proves the zero-receiver setup after drop.
        // shutdownTx 持有单调值，initialReceiver 被丢弃后形成零接收端测试场景。
        let (shutdown_tx, initial_receiver) = watch::channel(false);
        drop(initial_receiver);

        request_mcp_runtime_shutdown(&shutdown_tx);
        // futureReceiver subscribes only after the shutdown request has already been published.
        // futureReceiver 仅在关闭请求已经发布后才创建订阅。
        let future_receiver = shutdown_tx.subscribe();
        assert!(*future_receiver.borrow());

        request_mcp_runtime_shutdown(&shutdown_tx);
        assert!(*future_receiver.borrow());
    }

    /// Verifies successful counts survive beside one failure and dual failures remain combined.
    /// 验证单项失败时成功计数仍被保留，双项失败时两条诊断都会合并。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails if errors become zero counts or overwrite each other.
    /// 此测试无返回值；若错误被转换为零计数或相互覆盖则测试失败。
    #[test]
    fn optional_capability_refresh_outcome_preserves_counts_and_all_errors() {
        // partialOutcome preserves the successful resource count while prompt discovery fails.
        // partialOutcome 在 prompt 发现失败时保留成功的 resource 计数。
        let partial_outcome = build_server_optional_capability_refresh_outcome(
            Err(String::from("prompt refresh failed")),
            Ok(4),
        );
        assert_eq!(partial_outcome.prompt_count, None);
        assert_eq!(partial_outcome.resource_count, Some(4));
        assert_eq!(
            partial_outcome.warning.as_deref(),
            Some("prompt refresh failed")
        );

        // failedOutcome combines two independent current capability failures.
        // failedOutcome 合并两项相互独立的当前能力失败。
        let failed_outcome = build_server_optional_capability_refresh_outcome(
            Err(String::from("prompt refresh failed")),
            Err(String::from("resource refresh failed")),
        );
        // warning is the exact combined diagnostic published to the runtime snapshot.
        // warning 是发布到运行时快照的精确组合诊断。
        let warning = failed_outcome
            .warning
            .expect("dual failures should publish one warning");
        assert!(warning.contains("prompt refresh failed"));
        assert!(warning.contains("resource refresh failed"));

        // recoveredOutcome clears the warning only after both current refreshes succeed.
        // recoveredOutcome 仅在当前两项刷新均成功后清除警告。
        let recovered_outcome = build_server_optional_capability_refresh_outcome(Ok(2), Ok(3));
        assert_eq!(recovered_outcome.prompt_count, Some(2));
        assert_eq!(recovered_outcome.resource_count, Some(3));
        assert_eq!(recovered_outcome.warning, None);
    }
}
