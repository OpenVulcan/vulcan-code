use super::*;

/// Host-owned timeout for optional MCP prompt and resource discovery requests.
/// MCP 可选 prompt 与 resource 发现请求使用的宿主持有超时。
const MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// Process-local monotonic catalog identity; it contains no configuration or credential material.
/// 进程内单调目录身份，不包含配置或凭据内容。
static MCP_INSPECTION_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

/// Allocates an identity for a newly initialized or replaced catalog without persisting user state.
/// 为新初始化或替换的目录分配身份，不持久化用户状态。
fn next_inspection_generation() -> u64 {
    MCP_INSPECTION_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

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
    /// Consecutive automatic connection failures keyed by exact server identifier.
    /// 以精确服务器标识为键保存的连续自动连接失败次数。
    retry_counts: RwLock<BTreeMap<String, u8>>,
    /// Shared shutdown signal observed by all background MCP runtime tasks.
    /// 被所有后台 MCP 运行时任务共同观察的关闭信号。
    shutdown_tx: watch::Sender<bool>,
}

impl McpRuntimeHandle {
    /// Returns whether this runtime partition owns the exact server identifier.
    /// 返回当前运行时分区是否持有精确服务器标识。
    pub(super) fn contains_server(&self, server_id: &str) -> bool {
        self.inner.registry.server_by_id(server_id).is_some()
    }

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

    /// Returns structured runtime states for every server owned by this partition.
    /// 返回当前分区持有的每个服务器的结构化运行时状态。
    pub(super) fn server_states(&self) -> Vec<McpRuntimeServerState> {
        let snapshot = self.snapshot();
        self.states_from_snapshot(&snapshot)
    }

    /// Projects states from the caller-owned snapshot so schemas and status share one capture.
    /// 从调用者持有的快照投影状态，使 Schema 与状态共享同次采集。
    fn states_from_snapshot(&self, snapshot: &McpRuntimeSnapshot) -> Vec<McpRuntimeServerState> {
        let retry_counts = self
            .inner
            .retry_counts
            .read()
            .expect("mcp runtime retry count map poisoned");
        // transports is cloned once so diagnostic reads do not repeatedly acquire the shared map lock.
        // transports 只克隆一次，避免诊断读取反复获取共享映射锁。
        let transports = self
            .inner
            .transports
            .read()
            .expect("mcp runtime transport map poisoned")
            .clone();
        snapshot
            .servers
            .iter()
            .filter_map(|(server_id, server)| {
                let config = self.inner.registry.server_by_id(server_id)?;
                let (source, plugin_id, generation) = match &config.source {
                    crate::mcp::config::McpServerSource::UserConfigured => {
                        (String::from("user"), None, None)
                    }
                    crate::mcp::config::McpServerSource::Plugin {
                        plugin_id,
                        generation,
                    } => (
                        String::from("plugin"),
                        Some(plugin_id.clone()),
                        Some(*generation),
                    ),
                };
                // identity is read from the live transport so reconnects cannot retain stale initialize metadata.
                // identity 从实时传输读取，避免重连后继续保留过期的 initialize 元数据。
                let identity = transports
                    .get(server_id)
                    .and_then(|transport| transport.server_identity());
                Some(McpRuntimeServerState {
                    id: server_id.clone(),
                    name: server.name.clone(),
                    source,
                    plugin_id,
                    generation,
                    transport: server.transport_label.clone(),
                    endpoint: server.endpoint_summary.clone(),
                    status: server.status,
                    retry_count: retry_counts.get(server_id).copied().unwrap_or(0),
                    protocol_version: identity
                        .as_ref()
                        .map(|value| value.protocol_version.clone()),
                    server_product: identity.as_ref().map(|value| value.product.clone()),
                    server_version: identity.as_ref().map(|value| value.version.clone()),
                    tool_count: server.visible_tool_descriptors().len(),
                    prompt_count: server.prompt_count,
                    resource_count: server.resource_count,
                    last_error: server.last_error.clone(),
                    recent_logs: transports
                        .get(server_id)
                        .map(|transport| transport.recent_logs())
                        .unwrap_or_else(|| server.recent_logs.clone()),
                })
            })
            .collect()
    }

    /// Returns full schemas and states from the same immutable snapshot.
    /// 从同一不可变快照返回完整 Schema 与状态。
    pub(super) fn server_inspections(&self) -> Vec<McpRuntimeServerInspection> {
        let snapshot = self.snapshot();
        self.states_from_snapshot(&snapshot)
            .into_iter()
            .map(|state| {
                let tools = snapshot.servers[&state.id]
                    .visible_tool_descriptors()
                    .iter()
                    .map(|tool| tool.inspection())
                    .collect();
                McpRuntimeServerInspection {
                    inspection_generation: snapshot.servers[&state.id].inspection_generation,
                    state,
                    tools,
                }
            })
            .collect()
    }

    /// Queries metadata on an already connected transport and rejects a replaced connection.
    /// 在已连接传输查询元数据，并拒绝连接已替换时的过期结果。
    pub(super) async fn server_capabilities(
        &self,
        server_id: &str,
    ) -> Result<serde_json::Value, String> {
        let server = self
            .server_config(server_id)
            .ok_or_else(|| String::from("未找到 MCP 服务器。"))?;
        if self
            .snapshot()
            .servers
            .get(server_id)
            .is_none_or(|entry| entry.status != McpRuntimeServerStatus::Connected)
        {
            return Err(String::from("MCP 服务器尚未连接；详情读取不会自动连接。"));
        }
        let transport = self
            .transport_for_server(server_id)
            .ok_or_else(|| String::from("MCP 连接不可用。"))?;
        let (prompts, resources) = tokio::join!(
            run_bounded_optional_capability_discovery(
                &server,
                "prompt",
                MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT,
                transport.list_prompts()
            ),
            run_bounded_optional_capability_discovery(
                &server,
                "resource",
                MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT,
                transport.list_resources()
            )
        );
        if self
            .transport_for_server(server_id)
            .is_none_or(|current| !Arc::ptr_eq(&current, &transport))
            || self
                .snapshot()
                .servers
                .get(server_id)
                .is_none_or(|entry| entry.status != McpRuntimeServerStatus::Connected)
        {
            return Err(String::from("MCP 连接已经变化，请重新读取详情。"));
        }
        Ok(serde_json::json!({
            "prompts": prompts.as_ref().ok(), "promptError": prompts.as_ref().err().map(|error| server.sanitize_diagnostic(error)),
            "resources": resources.as_ref().ok(), "resourceError": resources.as_ref().err().map(|error| server.sanitize_diagnostic(error))
        }))
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

    /// Returns whether every server task has confirmed terminal transport teardown.
    /// 返回是否每个服务器任务都已确认终止传输拆除。
    pub(super) fn is_fully_stopped(&self) -> bool {
        self.snapshot()
            .servers
            .values()
            .all(|server| server.status == McpRuntimeServerStatus::Stopped)
    }

    /// Waits until every enabled server in this partition has completed initialize and atomic tool discovery.
    /// 等待当前分区内每个已启用服务器完成 initialize 与原子工具发现。
    ///
    /// # Parameters
    /// 参数
    /// `maximum_wait` is the host-owned readiness deadline and cannot be expanded by plugin input.
    /// `maximum_wait` 是宿主持有的就绪期限，插件输入不能放大该期限。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only for a fully connected partition; terminal failure or deadline expiry returns a diagnostic error.
    /// 仅对完全连接的分区返回成功；终止失败或期限到期会返回诊断错误。
    async fn wait_until_ready(&self, maximum_wait: Duration) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + maximum_wait;
        loop {
            let snapshot = self.snapshot();
            let enabled_servers = snapshot
                .servers
                .values()
                .filter(|server| server.enabled)
                .collect::<Vec<_>>();
            if enabled_servers
                .iter()
                .all(|server| server.status == McpRuntimeServerStatus::Connected)
            {
                return Ok(());
            }
            if let Some(server) = enabled_servers
                .iter()
                .find(|server| server.status == McpRuntimeServerStatus::Failed)
            {
                return Err(server.last_error.clone().unwrap_or_else(|| {
                    format!("MCP server `{}` entered Failed state.", server.name)
                }));
            }
            if tokio::time::Instant::now() >= deadline {
                let pending_servers = enabled_servers
                    .iter()
                    .filter(|server| server.status != McpRuntimeServerStatus::Connected)
                    .map(|server| format!("{}={}", server.name, server.status.label()))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "MCP partition readiness timed out after {} seconds: {pending_servers}",
                    maximum_wait.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Waits until every server task has completed transport teardown after shutdown.
    /// 等待全部服务器任务在收到关闭请求后完成传输拆除。
    ///
    /// # Parameters
    /// 参数
    /// `maximum_wait` is the host-owned teardown deadline.
    /// `maximum_wait` 是宿主持有的拆除期限。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only after every server snapshot reaches the terminal stopped state.
    /// 仅在每个服务器快照到达终止的已停止状态后返回成功。
    async fn wait_until_stopped(&self, maximum_wait: Duration) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + maximum_wait;
        loop {
            // snapshot captures one internally consistent teardown observation.
            // snapshot 捕获一份内部一致的拆除状态观察。
            let snapshot = self.snapshot();
            if snapshot
                .servers
                .values()
                .all(|server| server.status == McpRuntimeServerStatus::Stopped)
            {
                if let Some(error) = snapshot
                    .servers
                    .values()
                    .find_map(|server| server.last_error.clone())
                {
                    return Err(error);
                }
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                // pending_servers identifies every process whose shutdown could not be confirmed.
                // pending_servers 标识每个无法确认已关闭的进程。
                let pending_servers = snapshot
                    .servers
                    .values()
                    .filter(|server| server.status != McpRuntimeServerStatus::Stopped)
                    .map(|server| format!("{}={}", server.name, server.status.label()))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "MCP partition shutdown timed out after {} seconds: {pending_servers}",
                    maximum_wait.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
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

/// Boots one long-lived MCP runtime handle from source-neutral server definitions.
/// 根据中立服务器定义引导一个长期 MCP 运行时句柄。
async fn bootstrap_mcp_runtime(servers: Vec<McpServerConfig>) -> Result<McpRuntimeHandle, String> {
    let registry = McpServerRegistry::new(servers);
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
        retry_counts: RwLock::new(
            registry
                .servers()
                .iter()
                .map(|server| (server.id.clone(), 0))
                .collect(),
        ),
        shutdown_tx,
    });

    for server in registry.enabled_servers() {
        spawn_server_runtime_task(inner.clone(), server.clone());
    }

    Ok(McpRuntimeHandle { inner })
}

/// Boots one stable MCP runtime access surface from source-neutral server definitions.
/// 根据中立服务器定义引导一个稳定 MCP 运行时访问面。
pub async fn bootstrap_mcp_runtime_access(
    servers: Vec<McpServerConfig>,
) -> Result<McpRuntimeAccess, String> {
    let overview = configured_mcp_registry_overview(servers.clone());
    let mut grouped = BTreeMap::<String, Vec<McpServerConfig>>::new();
    for server in servers {
        grouped
            .entry(server.source.partition_key())
            .or_default()
            .push(server);
    }
    let mut partitions = BTreeMap::<String, McpRuntimeHandle>::new();
    for (partition_key, partition_servers) in grouped {
        match bootstrap_mcp_runtime(partition_servers).await {
            Ok(runtime) => {
                if partition_key.starts_with("plugin:")
                    && let Err(error) = runtime.wait_until_ready(Duration::from_secs(15)).await
                {
                    runtime.shutdown();
                    for existing_runtime in partitions.values() {
                        existing_runtime.shutdown();
                    }
                    return Err(error);
                }
                partitions.insert(partition_key, runtime);
            }
            Err(error) => {
                for runtime in partitions.values() {
                    runtime.shutdown();
                }
                return Err(error);
            }
        }
    }
    Ok(McpRuntimeAccess::new(overview, partitions))
}

/// Replaces only the user-configured MCP runtime partition while retaining plugin partitions.
/// 仅替换用户配置 MCP 运行时分区，同时保留插件分区。
///
/// # Parameters
/// 参数
/// `current` owns retained plugin partitions and `servers` must contain only user definitions.
/// `current` 持有保留的插件分区，`servers` 必须只包含用户定义。
///
/// # Returns
/// 返回值
/// Returns a new access generation whose unrelated plugin processes were not restarted.
/// 返回一个未重启无关插件进程的新访问代次。
pub async fn replace_user_mcp_runtime_partition(
    current: &McpRuntimeAccess,
    servers: Vec<McpServerConfig>,
) -> Result<McpRuntimeAccess, String> {
    if servers
        .iter()
        .any(|server| server.source != crate::mcp::config::McpServerSource::UserConfigured)
    {
        return Err(String::from(
            "The user MCP partition accepts only user-configured server definitions.",
        ));
    }
    let overview = configured_mcp_registry_overview(servers.clone());
    let runtime = bootstrap_mcp_runtime(servers).await?;
    Ok(current.with_replaced_user_partition(overview, runtime))
}

/// Prepares and atomically projects one plugin MCP partition while retaining user and unrelated plugin partitions.
/// 准备并原子投影单个插件 MCP 分区，同时保留用户与无关插件分区。
///
/// # Parameters
/// 参数
/// `current` owns retained partitions, `plugin_id` and `generation` define the exact source identity, and `servers` must belong only to that identity.
/// `current` 持有保留分区，`plugin_id` 与 `generation` 定义精确来源身份，`servers` 必须仅属于该身份。
///
/// # Returns
/// 返回值
/// Returns a new access generation only after every candidate server passes initialize and complete tool discovery.
/// 仅在每个候选服务器通过 initialize 与完整工具发现后返回新的访问代次。
pub async fn replace_plugin_mcp_runtime_partition(
    current: &McpRuntimeAccess,
    plugin_id: &str,
    generation: u64,
    servers: Vec<McpServerConfig>,
) -> Result<McpRuntimeAccess, String> {
    if servers.iter().any(|server| {
        !matches!(
            &server.source,
            crate::mcp::config::McpServerSource::Plugin {
                plugin_id: source_plugin_id,
                generation: source_generation,
            } if source_plugin_id == plugin_id && *source_generation == generation
        )
    }) {
        return Err(format!(
            "Plugin MCP partition `{plugin_id}:{generation}` received a server from another source."
        ));
    }
    let overview = configured_mcp_registry_overview(servers.clone());
    let runtime = bootstrap_mcp_runtime(servers).await?;
    if let Err(error) = runtime.wait_until_ready(Duration::from_secs(15)).await {
        runtime.shutdown();
        return Err(error);
    }
    Ok(current.with_replaced_plugin_partition(plugin_id, generation, overview, runtime))
}

/// Starts, validates, and fully stops one isolated plugin MCP partition without publishing it.
/// 启动、校验并完全停止一个隔离的插件 MCP 分区，且不发布该分区。
///
/// # Parameters
/// 参数
/// `plugin_id` and `generation` define the exact candidate source identity, while `servers` contains only local installation candidates.
/// `plugin_id` 与 `generation` 定义精确候选来源身份，`servers` 只包含本地安装候选服务器。
///
/// # Returns
/// 返回值
/// Returns success only after initialize, complete tool discovery, schema validation, and confirmed process teardown all succeed.
/// 仅在初始化、完整工具发现、Schema 校验与进程拆除确认全部成功后返回成功。
pub async fn validate_plugin_mcp_runtime_partition(
    plugin_id: &str,
    generation: u64,
    servers: Vec<McpServerConfig>,
) -> Result<(), String> {
    if servers.iter().any(|server| {
        !matches!(
            &server.source,
            crate::mcp::config::McpServerSource::Plugin {
                plugin_id: source_plugin_id,
                generation: source_generation,
            } if source_plugin_id == plugin_id && *source_generation == generation
        )
    }) {
        return Err(format!(
            "Plugin MCP validation partition `{plugin_id}:{generation}` received a server from another source."
        ));
    }
    // runtime remains isolated from every published MCP access generation.
    // runtime 与所有已发布 MCP 访问代次保持隔离。
    let runtime = bootstrap_mcp_runtime(servers).await?;
    // validation_result preserves readiness failure while teardown still runs unconditionally.
    // validation_result 保留就绪失败，同时无条件执行拆除。
    let validation_result = runtime.wait_until_ready(Duration::from_secs(15)).await;
    runtime.shutdown();
    // shutdown_result proves local processes released staging files before installation continues.
    // shutdown_result 证明本地进程已在安装继续前释放暂存文件。
    let shutdown_result = runtime.wait_until_stopped(Duration::from_secs(7)).await;
    match (validation_result, shutdown_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(validation_error), Ok(())) => Err(validation_error),
        (Ok(()), Err(shutdown_error)) => Err(shutdown_error),
        (Err(validation_error), Err(shutdown_error)) => Err(format!(
            "{validation_error}; candidate teardown also failed: {shutdown_error}"
        )),
    }
}

/// Removes one plugin MCP partition from the next access generation without stopping old in-flight leases immediately.
/// 从下一访问代次移除单个插件 MCP 分区，同时不立即停止旧执行中的租约。
///
/// # Parameters
/// 参数
/// `current` owns all current partitions and `plugin_id` selects the exact plugin namespace to withdraw.
/// `current` 持有全部当前分区，`plugin_id` 选择需要撤销的精确插件命名空间。
///
/// # Returns
/// 返回值
/// Returns a new access generation that retains every unrelated partition.
/// 返回保留全部无关分区的新访问代次。
pub fn remove_plugin_mcp_runtime_partition(
    current: &McpRuntimeAccess,
    plugin_id: &str,
) -> McpRuntimeAccess {
    current.without_plugin_partition(plugin_id)
}

/// Builds one reusable host-owned OAuth session for a configured remote MCP server when the server exposes OAuth metadata.
/// 当服务器暴露 OAuth 元数据时，为已配置的远端 MCP 服务器构建一个可复用的宿主自有 OAuth 会话。
async fn build_server_auth_session(
    server: &McpServerConfig,
) -> Result<Option<Arc<McpOAuthSession>>, String> {
    let Some(oauth) = server.oauth().cloned() else {
        return Ok(None);
    };
    let Some(uri) = server.remote_url().map(str::to_owned) else {
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
        // consecutiveFailures gates automatic restart so a permanently invalid server cannot spin forever.
        // consecutiveFailures 限制自动重启，避免永久无效的服务器无限循环。
        let mut consecutive_failures = 0_u8;
        // authSession is initialized inside the bounded retry loop because metadata discovery can fail transiently.
        // authSession 在有限重试循环内初始化，因为元数据发现可能发生瞬时失败。
        let mut auth_session: Option<Option<Arc<McpOAuthSession>>> = None;
        loop {
            if *shutdown_rx.borrow() {
                shutdown_server_runtime_state(&inner, &server.id).await;
                break;
            }

            if auth_session.is_none() {
                // authSessionResult remains interruptible while remote OAuth metadata and registration are prepared.
                // authSessionResult 在远端 OAuth 元数据与注册准备期间保持可被关闭信号中断。
                let auth_session_result = tokio::select! {
                    _ = shutdown_rx.changed() => {
                        shutdown_server_runtime_state(&inner, &server.id).await;
                        return;
                    }
                    result = build_server_auth_session(&server) => result,
                };
                match auth_session_result {
                    Ok(session) => {
                        update_server_auth_session_cache(&inner, &server.id, session.clone()).await;
                        auth_session = Some(session);
                    }
                    Err(error) => {
                        if record_server_failure_and_should_stop(
                            &inner,
                            &server,
                            &mut consecutive_failures,
                            None,
                            error,
                        )
                        .await
                        {
                            return;
                        }
                        if wait_for_retry_or_shutdown(
                            &mut shutdown_rx,
                            retry_delay(&server.id, consecutive_failures),
                        )
                        .await
                        {
                            shutdown_server_runtime_state(&inner, &server.id).await;
                            return;
                        }
                        continue;
                    }
                }
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
                    shutdown_server_runtime_state(&inner, &server.id).await;
                    break;
                }
                result = async {
                    tokio::time::timeout(
                        Duration::from_secs(15),
                        inner.registry.spawn_transport(
                            &server.id,
                            auth_session.as_ref().cloned().flatten(),
                        ),
                    )
                    .await
                    .map_err(|_| {
                        format!(
                            "MCP server `{}` initialize timed out after 15 seconds.",
                            server.name
                        )
                    })?
                } => result,
            };

            let transport = match transport_result {
                Ok(transport) => transport,
                Err(error) => {
                    if record_server_failure_and_should_stop(
                        &inner,
                        &server,
                        &mut consecutive_failures,
                        None,
                        error,
                    )
                    .await
                    {
                        return;
                    }
                    if wait_for_retry_or_shutdown(
                        &mut shutdown_rx,
                        retry_delay(&server.id, consecutive_failures),
                    )
                    .await
                    {
                        shutdown_server_runtime_state(&inner, &server.id).await;
                        break;
                    }
                    continue;
                }
            };

            if let Err(error) = refresh_server_tools(&inner, &server, transport.clone()).await {
                if record_server_failure_and_should_stop(
                    &inner,
                    &server,
                    &mut consecutive_failures,
                    Some(transport.clone()),
                    error,
                )
                .await
                {
                    return;
                }
                if wait_for_retry_or_shutdown(
                    &mut shutdown_rx,
                    retry_delay(&server.id, consecutive_failures),
                )
                .await
                {
                    shutdown_server_runtime_state(&inner, &server.id).await;
                    break;
                }
                continue;
            }

            refresh_server_optional_capabilities(&inner, &server, transport.clone()).await;

            // connectedAt distinguishes a stable connection from a short-lived crash loop.
            // connectedAt 用于区分稳定连接与短生命周期崩溃循环。
            let connected_at = tokio::time::Instant::now();
            let mut event_rx = transport.event_receiver();
            loop {
                let event = tokio::select! {
                    _ = shutdown_rx.changed() => {
                        shutdown_server_runtime_state(&inner, &server.id).await;
                        return;
                    }
                    result = event_rx.recv() => result,
                };

                match event {
                    Ok(McpTransportEvent::ToolsChanged) => {
                        if let Err(error) =
                            refresh_server_tools(&inner, &server, transport.clone()).await
                        {
                            clear_server_runtime_state(&inner, &server.id, None, Some(error)).await;
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
                        clear_server_runtime_state(&inner, &server.id, None, Some(reason)).await;
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        clear_server_runtime_state(
                            &inner,
                            &server.id,
                            None,
                            Some(format!("MCP 服务器 `{}` 的事件通道已关闭。", server.name)),
                        )
                        .await;
                        break;
                    }
                }
            }

            if connected_at.elapsed() >= Duration::from_secs(60) {
                consecutive_failures = 0;
            }
            consecutive_failures = consecutive_failures.saturating_add(1);
            update_server_retry_count(&inner, &server.id, consecutive_failures);
            if consecutive_failures >= 5 {
                mark_server_failed(
                    &inner,
                    &server.id,
                    None,
                    format!(
                        "MCP server `{}` exhausted 5 consecutive automatic connection attempts.",
                        server.name
                    ),
                )
                .await;
                return;
            }
            if wait_for_retry_or_shutdown(
                &mut shutdown_rx,
                retry_delay(&server.id, consecutive_failures),
            )
            .await
            {
                shutdown_server_runtime_state(&inner, &server.id).await;
                break;
            }
        }
    });
}

/// Records one startup or discovery failure and transitions the server to terminal Failed after five consecutive attempts.
/// 记录一次启动或发现失败，并在连续五次尝试失败后将服务器转换为终止性 Failed 状态。
///
/// # Parameters
/// 参数
/// `inner` owns runtime state, `server` identifies the failure, `consecutive_failures` stores the bounded counter,
/// `unpublished_transport` retains a failed setup candidate, and `error` is the sanitized failure text.
/// `inner` 持有运行时状态，`server` 标识失败，`consecutive_failures` 保存有限计数，
/// `unpublished_transport` 保留失败的初始化候选传输，`error` 是已脱敏失败文本。
///
/// # Returns
/// 返回值
/// Returns true when automatic retry must stop because the failure limit was reached.
/// 达到失败上限、必须停止自动重试时返回 true。
async fn record_server_failure_and_should_stop(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    consecutive_failures: &mut u8,
    unpublished_transport: Option<Arc<dyn McpTransport>>,
    error: String,
) -> bool {
    *consecutive_failures = consecutive_failures.saturating_add(1);
    update_server_retry_count(inner, &server.id, *consecutive_failures);
    if *consecutive_failures >= 5 {
        mark_server_failed(
            inner,
            &server.id,
            unpublished_transport,
            format!(
                "MCP server `{}` exhausted 5 consecutive automatic connection attempts. Last error: {error}",
                server.name
            ),
        )
        .await;
        true
    } else {
        clear_server_runtime_state(inner, &server.id, unpublished_transport, Some(error)).await;
        false
    }
}

/// Calculates bounded exponential reconnect delay with deterministic per-server jitter.
/// 计算带确定性单服务器抖动的有界指数重连延迟。
///
/// # Parameters
/// 参数
/// `server_id` provides a stable jitter seed and `consecutive_failures` selects the 1/2/4/8/16/30-second backoff step.
/// `server_id` 提供稳定抖动种子，`consecutive_failures` 选择 1/2/4/8/16/30 秒退避步骤。
///
/// # Returns
/// 返回值
/// Returns a delay capped at 30 seconds plus no more than 250 milliseconds of jitter.
/// 返回上限为 30 秒且抖动不超过 250 毫秒的延迟。
fn retry_delay(server_id: &str, consecutive_failures: u8) -> Duration {
    const BACKOFF_SECONDS: [u64; 6] = [1, 2, 4, 8, 16, 30];
    let backoff_index = usize::from(consecutive_failures.saturating_sub(1))
        .min(BACKOFF_SECONDS.len().saturating_sub(1));
    let jitter_millis = server_id
        .bytes()
        .fold(u64::from(consecutive_failures), |accumulator, byte| {
            accumulator.wrapping_mul(33).wrapping_add(u64::from(byte))
        })
        % 251;
    Duration::from_secs(BACKOFF_SECONDS[backoff_index])
        .saturating_add(Duration::from_millis(jitter_millis))
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

/// Waits for the selected bounded reconnect backoff unless the runtime has already been asked to shut down.
/// 在运行时已经收到关闭请求前提下提前退出，否则等待选定的有界重连退避窗口。
///
/// # Parameters
/// 参数
/// `shutdown_rx` observes the latched partition shutdown signal and `delay` is the precomputed exponential backoff with jitter.
/// `shutdown_rx` 观察锁存的分区关闭信号，`delay` 是预先计算并带抖动的指数退避时长。
///
/// # Returns
/// 返回值
/// Returns true when shutdown won the race and false when the retry delay elapsed.
/// 关闭信号先到达时返回 true，重试等待到期时返回 false。
async fn wait_for_retry_or_shutdown(
    shutdown_rx: &mut watch::Receiver<bool>,
    delay: Duration,
) -> bool {
    tokio::select! {
        _ = shutdown_rx.changed() => true,
        _ = tokio::time::sleep(delay) => false,
    }
}

/// Refreshes one server tool snapshot after the connection is established or a tool-list change arrives.
/// 在连接建立后或收到工具列表变化通知时刷新单个服务器的工具快照。
async fn refresh_server_tools(
    inner: &Arc<McpRuntimeInner>,
    server: &McpServerConfig,
    transport: Arc<dyn McpTransport>,
) -> Result<(), String> {
    let schemas = tokio::time::timeout(Duration::from_secs(10), transport.list_tools())
        .await
        .map_err(|_| {
            format!(
                "MCP server `{}` tools/list timed out after 10 seconds.",
                server.name
            )
        })?
        .map_err(|error| {
            format!(
                "刷新 MCP 服务器 `{}` 工具列表失败：{}。",
                server.name, error
            )
        })?;
    validate_mcp_tool_schema_list(&server.name, &schemas)?;
    let tools = schemas
        .into_iter()
        .map(|schema| {
            McpRuntimeToolDescriptor::new(
                server.id.clone(),
                schema.name,
                schema.description,
                schema.args_schema,
            )
            .with_result_schema(schema.result_schema)
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

/// Validates one complete MCP tool list before any part of it replaces the last successful snapshot.
/// 在完整 MCP 工具列表的任何部分替换上次成功快照前对其进行校验。
///
/// # Parameters
/// 参数
/// `server_name` identifies the diagnostic owner and `schemas` is the exact complete list returned by one `tools/list` response.
/// `server_name` 标识诊断归属，`schemas` 是单次 `tools/list` 响应返回的精确完整列表。
///
/// # Returns
/// 返回值
/// Returns success only when names are non-empty and unique and every declared input/output JSON Schema is structurally valid.
/// 仅当名称非空且唯一，并且所有已声明输入/输出 JSON Schema 结构有效时返回成功。
fn validate_mcp_tool_schema_list(
    server_name: &str,
    schemas: &[vulcan_mcp_runtime::McpToolSchema],
) -> Result<(), String> {
    // occupiedNames prevents ambiguous invocation ownership within one server snapshot.
    // occupiedNames 防止单个服务器快照内出现含糊的调用归属。
    let mut occupied_names = BTreeSet::new();
    for schema in schemas {
        let tool_name = schema.name.trim();
        if tool_name.is_empty() {
            return Err(format!(
                "MCP server `{server_name}` returned a tool with an empty name."
            ));
        }
        if !occupied_names.insert(tool_name) {
            return Err(format!(
                "MCP server `{server_name}` returned duplicate tool name `{tool_name}`."
            ));
        }
        if !schema.args_schema.is_object() {
            return Err(format!(
                "MCP tool `{server_name}/{tool_name}` input schema must be a JSON object schema."
            ));
        }
        validate_mcp_schema_document(server_name, tool_name, "input", &schema.args_schema)?;
        if !schema.result_schema.is_null() {
            validate_mcp_schema_document(server_name, tool_name, "output", &schema.result_schema)?;
        }
    }
    Ok(())
}

/// Validates and compiles one untrusted MCP tool schema as fixed JSON Schema Draft 2020-12.
/// 按固定 JSON Schema Draft 2020-12 校验并编译单个不受信任的 MCP 工具 Schema。
///
/// # Parameters
/// 参数
/// `server_name` and `tool_name` identify diagnostics, `schema_role` identifies input or output, and `schema` is the complete schema document.
/// `server_name` 与 `tool_name` 标识诊断，`schema_role` 标识输入或输出，`schema` 是完整 Schema 文档。
///
/// # Returns
/// 返回值
/// Returns success only after fixed-draft meta-validation, controlled-reference validation, and compilation all succeed.
/// 仅在固定方言元校验、受控引用校验与编译全部成功后返回成功。
fn validate_mcp_schema_document(
    server_name: &str,
    tool_name: &str,
    schema_role: &str,
    schema: &serde_json::Value,
) -> Result<(), String> {
    jsonschema::draft202012::meta::validate(schema).map_err(|error| {
        format!("MCP tool `{server_name}/{tool_name}` {schema_role} schema is invalid: {error}")
    })?;
    validate_mcp_schema_references(server_name, tool_name, schema_role, schema)?;
    jsonschema::draft202012::new(schema).map_err(|_| {
        format!(
            "MCP tool `{server_name}/{tool_name}` {schema_role} schema could not be compiled as JSON Schema Draft 2020-12."
        )
    })?;
    Ok(())
}

/// Rejects schema references that would require resolution outside the received MCP schema document.
/// 拒绝需要在收到的 MCP Schema 文档之外解析的 Schema 引用。
///
/// # Parameters
/// 参数
/// `server_name` and `tool_name` identify the diagnostic owner, `schema_role` identifies input or output, and `schema` is the exact untrusted schema document.
/// `server_name` 与 `tool_name` 标识诊断归属，`schema_role` 标识输入或输出，`schema` 是精确的未受信任 Schema 文档。
///
/// # Returns
/// 返回值
/// Returns success only when every reference-bearing keyword contains a local fragment reference beginning with `#`.
/// 仅当每个带引用语义的关键字都包含以 `#` 开头的本地片段引用时返回成功。
fn validate_mcp_schema_references(
    server_name: &str,
    tool_name: &str,
    schema_role: &str,
    schema: &serde_json::Value,
) -> Result<(), String> {
    let Some(schema_object) = schema.as_object() else {
        return Ok(());
    };
    for keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
        if let Some(value) = schema_object.get(keyword) {
            let is_local_fragment = value
                .as_str()
                .is_some_and(|reference| reference.starts_with('#'));
            if !is_local_fragment {
                // Never include the reference value because an untrusted URI may embed credentials.
                // 绝不包含引用值，因为不受信任的 URI 可能嵌入凭据。
                return Err(format!(
                    "MCP tool `{server_name}/{tool_name}` {schema_role} schema contains an external or invalid `{keyword}`; only local fragment references are allowed."
                ));
            }
        }
    }

    // directSchemaKeywords contains keywords whose value is exactly one child schema.
    // directSchemaKeywords 包含值恰好为单个子 Schema 的关键字。
    for keyword in [
        "additionalProperties",
        "contains",
        "contentSchema",
        "else",
        "if",
        "items",
        "not",
        "propertyNames",
        "then",
        "unevaluatedItems",
        "unevaluatedProperties",
    ] {
        if let Some(child_schema) = schema_object.get(keyword) {
            validate_mcp_schema_references(server_name, tool_name, schema_role, child_schema)?;
        }
    }

    // schemaMapKeywords contains keywords whose object values are independently evaluated child schemas.
    // schemaMapKeywords 包含对象值会作为独立子 Schema 求值的关键字。
    for keyword in [
        "$defs",
        "dependentSchemas",
        "patternProperties",
        "properties",
    ] {
        if let Some(children) = schema_object
            .get(keyword)
            .and_then(serde_json::Value::as_object)
        {
            for child_schema in children.values() {
                validate_mcp_schema_references(server_name, tool_name, schema_role, child_schema)?;
            }
        }
    }

    // schemaArrayKeywords contains applicators whose array items are child schemas.
    // schemaArrayKeywords 包含数组元素均为子 Schema 的应用器关键字。
    for keyword in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(children) = schema_object
            .get(keyword)
            .and_then(serde_json::Value::as_array)
        {
            for child_schema in children {
                validate_mcp_schema_references(server_name, tool_name, schema_role, child_schema)?;
            }
        }
    }

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
    run_bounded_optional_capability_discovery(
        server,
        "prompt",
        MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT,
        transport.list_prompts(),
    )
    .await
    .map(|prompts| prompts.len())
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
    run_bounded_optional_capability_discovery(
        server,
        "resource",
        MCP_OPTIONAL_CAPABILITY_DISCOVERY_TIMEOUT,
        transport.list_resources(),
    )
    .await
    .map(|resources| resources.len())
}

/// Runs one optional MCP capability discovery request under an immutable host-owned deadline.
/// 在不可变的宿主持有期限内运行单个 MCP 可选能力发现请求。
///
/// # Parameters
/// 参数
/// `server` provides stable diagnostics, `capability` names the protocol surface, `maximum_wait` is the host deadline,
/// and `request` is the exact transport future being bounded.
/// `server` 提供稳定诊断，`capability` 命名协议表面，`maximum_wait` 是宿主期限，
/// `request` 是被限制的精确传输 Future。
///
/// # Returns
/// 返回值
/// Returns the authoritative response or a contextual transport/timeout diagnostic without blocking lifecycle shutdown indefinitely.
/// 返回权威响应或带上下文的传输/超时诊断，且不会无限阻塞生命周期关闭。
async fn run_bounded_optional_capability_discovery<T, F>(
    server: &McpServerConfig,
    capability: &str,
    maximum_wait: Duration,
    request: F,
) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, vulcan_mcp_runtime::McpRuntimeError>>,
{
    tokio::time::timeout(maximum_wait, request)
        .await
        .map_err(|_| {
            format!(
                "MCP server `{}` {capability} discovery timed out after {} seconds.",
                server.name,
                maximum_wait.as_secs()
            )
        })?
        .map_err(|error| {
            format!(
                "刷新 MCP 服务器 `{}` {capability} 列表失败：{error}。",
                server.name
            )
        })
}

/// Removes and explicitly shuts down one server transport before publishing its disconnected state.
/// 移除并显式关闭单个服务器传输，然后发布其断开状态。
///
/// # Parameters
/// 参数
/// `inner` owns registered transports and snapshots, `server_id` selects one server,
/// `unpublished_transport` retains a just-connected transport that failed before registration, and `error` is the primary diagnostic.
/// `inner` 持有已注册传输与快照，`server_id` 选择单个服务器，
/// `unpublished_transport` 保留注册前失败的刚连接传输，`error` 是主要诊断。
async fn clear_server_runtime_state(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    unpublished_transport: Option<Arc<dyn McpTransport>>,
    error: Option<String>,
) {
    let (recent_logs, shutdown_error) =
        remove_and_shutdown_server_transport(inner, server_id, unpublished_transport).await;
    update_server_status(
        inner,
        server_id,
        McpRuntimeServerStatus::Disconnected,
        combine_mcp_runtime_errors(error, shutdown_error),
        Vec::new(),
    );
    update_server_recent_logs(inner, server_id, recent_logs);
    update_server_prompt_count(inner, server_id, 0);
    update_server_resource_count(inner, server_id, 0);
}

/// Gracefully shuts down one registered transport before publishing the terminal stopped state.
/// 在发布终止的已停止状态前优雅关闭单个已注册传输。
///
/// # Parameters
/// 参数
/// `inner` owns the transport and snapshot while `server_id` selects the exact server task.
/// `inner` 持有传输与快照，`server_id` 选择精确服务器任务。
async fn shutdown_server_runtime_state(inner: &Arc<McpRuntimeInner>, server_id: &str) {
    let (recent_logs, shutdown_error) =
        remove_and_shutdown_server_transport(inner, server_id, None).await;
    update_server_status(
        inner,
        server_id,
        McpRuntimeServerStatus::Stopped,
        shutdown_error,
        Vec::new(),
    );
    update_server_recent_logs(inner, server_id, recent_logs);
    update_server_prompt_count(inner, server_id, 0);
    update_server_resource_count(inner, server_id, 0);
}

/// Clears one server transport and publishes a terminal bounded-retry failure.
/// 清除单个服务器传输并发布终止性的有限重试失败状态。
///
/// # Parameters
/// 参数
/// `inner` owns transport state, `server_id` identifies the server, `unpublished_transport` retains a failed setup candidate,
/// and `error` is the final sanitized diagnostic.
/// `inner` 持有传输状态，`server_id` 标识服务器，`unpublished_transport` 保留失败的初始化候选传输，
/// `error` 是最终已脱敏诊断。
///
/// # Returns
/// 返回值
/// Returns no value; the server stays failed until a new runtime partition is explicitly published.
/// 不返回值；服务器会保持失败，直到显式发布新的运行时分区。
async fn mark_server_failed(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    unpublished_transport: Option<Arc<dyn McpTransport>>,
    error: String,
) {
    let (recent_logs, shutdown_error) =
        remove_and_shutdown_server_transport(inner, server_id, unpublished_transport).await;
    update_server_status(
        inner,
        server_id,
        McpRuntimeServerStatus::Failed,
        combine_mcp_runtime_errors(Some(error), shutdown_error),
        Vec::new(),
    );
    if !recent_logs.is_empty() {
        update_server_recent_logs(inner, server_id, recent_logs);
    }
    update_server_prompt_count(inner, server_id, 0);
    update_server_resource_count(inner, server_id, 0);
}

/// Removes one registered or unpublished transport and explicitly waits for its bounded shutdown.
/// 移除单个已注册或尚未发布的传输，并显式等待其有限关闭过程。
///
/// # Parameters
/// 参数
/// `inner` owns the transport registry, `server_id` selects one registered transport, and
/// `unpublished_transport` supplies the exact setup candidate when registration never completed.
/// `inner` 持有传输注册表，`server_id` 选择单个已注册传输，
/// `unpublished_transport` 在注册未完成时提供精确的初始化候选传输。
///
/// # Returns
/// 返回值
/// Returns the bounded sanitized log snapshot and an optional shutdown diagnostic.
/// 返回有限脱敏日志快照与可选关闭诊断。
async fn remove_and_shutdown_server_transport(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    unpublished_transport: Option<Arc<dyn McpTransport>>,
) -> (Vec<String>, Option<String>) {
    // transport is removed before awaiting shutdown so no new call can bind to a draining resource.
    // transport 会在等待关闭前被移除，确保新调用不能绑定到正在排空的资源。
    let registered_transport = inner
        .transports
        .write()
        .expect("mcp runtime transport map poisoned")
        .remove(server_id);
    // registered ownership is authoritative; the fallback exists only before initial publication.
    // 已注册所有权是权威来源；回退值只存在于首次发布之前。
    let transport = registered_transport.or(unpublished_transport);
    let recent_logs = transport
        .as_ref()
        .map(|transport| transport.recent_logs())
        .unwrap_or_default();
    let shutdown_error = match transport {
        Some(transport) => transport
            .shutdown()
            .await
            .err()
            .map(|error| error.to_string()),
        None => None,
    };
    (recent_logs, shutdown_error)
}

/// Combines one primary lifecycle error with a later transport shutdown failure without losing either cause.
/// 合并一个主要生命周期错误与后续传输关闭失败，且不丢失任一原因。
///
/// # Parameters
/// 参数
/// `primary_error` is the operation failure and `shutdown_error` is the bounded cleanup failure.
/// `primary_error` 是操作失败，`shutdown_error` 是有限清理失败。
///
/// # Returns
/// 返回值
/// Returns no diagnostic when both inputs are absent, otherwise one complete ordered diagnostic.
/// 两项输入均不存在时不返回诊断，否则返回一条完整有序诊断。
fn combine_mcp_runtime_errors(
    primary_error: Option<String>,
    shutdown_error: Option<String>,
) -> Option<String> {
    match (primary_error, shutdown_error) {
        (Some(primary), Some(shutdown)) => Some(format!(
            "{primary}; MCP transport shutdown also failed: {shutdown}"
        )),
        (Some(primary), None) => Some(primary),
        (None, Some(shutdown)) => Some(shutdown),
        (None, None) => None,
    }
}

/// Replaces the retained sanitized transport diagnostics for one server snapshot.
/// 替换单个服务器快照中保留的脱敏传输诊断。
///
/// # Parameters
/// 参数
/// `inner` owns the shared snapshot, `server_id` selects the server, and `recent_logs` is already bounded and sanitized by the transport.
/// `inner` 持有共享快照，`server_id` 选择服务器，`recent_logs` 已由传输执行有界与脱敏处理。
fn update_server_recent_logs(
    inner: &Arc<McpRuntimeInner>,
    server_id: &str,
    recent_logs: Vec<String>,
) {
    if let Some(server) = inner
        .snapshot
        .write()
        .expect("mcp runtime snapshot poisoned")
        .servers
        .get_mut(server_id)
    {
        server.recent_logs = recent_logs;
    }
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
    // error is sanitized against the exact server configuration before entering shared observable state.
    // error 在进入共享可观察状态前，依据精确服务器配置完成脱敏。
    let error = error.map(|error| {
        inner
            .registry
            .server_by_id(server_id)
            .map_or(error.clone(), |server| server.sanitize_diagnostic(&error))
    });
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
            server.inspection_generation = next_inspection_generation();
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

/// Updates the observable consecutive retry count for one exact server.
/// 更新单个精确服务器的可观察连续重试次数。
///
/// # Parameters
/// 参数
/// `inner` owns the retry map, `server_id` selects one configured server, and `retry_count` is the authoritative current value.
/// `inner` 持有重试映射，`server_id` 选择单个已配置服务器，`retry_count` 是权威当前值。
///
/// # Returns
/// 返回值
/// Returns no value; missing identifiers are inserted only for runtime-owned configured servers.
/// 不返回值；缺失标识仅会为运行时持有的已配置服务器插入。
fn update_server_retry_count(inner: &Arc<McpRuntimeInner>, server_id: &str, retry_count: u8) {
    inner
        .retry_counts
        .write()
        .expect("mcp runtime retry count map poisoned")
        .insert(server_id.to_string(), retry_count);
}

/// Builds the initial snapshot for one configured MCP server before any connection attempt begins.
/// 在任何连接尝试开始之前，为单个已配置 MCP 服务器构建初始快照。
fn initial_server_snapshot(server: &McpServerConfig) -> McpRuntimeServerSnapshot {
    McpRuntimeServerSnapshot {
        inspection_generation: next_inspection_generation(),
        name: server.name.clone(),
        enabled: server.enabled,
        transport_label: server.transport_kind().label().to_string(),
        endpoint_summary: server.command_summary(),
        status: if server.enabled {
            McpRuntimeServerStatus::Connecting
        } else {
            McpRuntimeServerStatus::Disabled
        },
        last_error: None,
        recent_logs: Vec::new(),
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
    use async_trait::async_trait;
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::broadcast;
    use vulcan_mcp_runtime::{McpRuntimeError, McpToolSchema};

    /// Test transport that records every explicit shutdown request.
    /// 记录每次显式关闭请求的测试传输。
    struct ShutdownTrackingTransport {
        /// Shared exact shutdown invocation count.
        /// 共享的精确关闭调用次数。
        shutdown_count: Arc<AtomicUsize>,
        /// Event channel required by the transport contract.
        /// 传输契约要求的事件通道。
        event_tx: broadcast::Sender<McpTransportEvent>,
    }

    #[async_trait]
    impl McpTransport for ShutdownTrackingTransport {
        /// Returns the deterministic test endpoint identifier.
        /// 返回确定性的测试端点标识。
        fn endpoint(&self) -> &str {
            "test://shutdown-tracking"
        }

        /// Returns an empty valid tool list for this lifecycle-only fixture.
        /// 为仅测试生命周期的夹具返回空的有效工具列表。
        async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
            Ok(Vec::new())
        }

        /// Subscribes to the fixture event channel.
        /// 订阅夹具事件通道。
        fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
            self.event_tx.subscribe()
        }

        /// Records one explicit host-owned resource teardown.
        /// 记录一次宿主持有的显式资源拆除。
        async fn shutdown(&self) -> Result<(), McpRuntimeError> {
            self.shutdown_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        /// Fails if metadata inspection accidentally crosses the execution boundary.
        /// 元数据检查意外跨越执行边界时使测试失败。
        async fn call_tool(&self, _name: &str, _args: Value) -> Result<Value, McpRuntimeError> {
            panic!("metadata inspection must never execute tools")
        }
    }

    /// Verifies full schemas, real connectivity, and metadata-only reads on an isolated transport.
    /// 在隔离传输上验证完整 Schema、真实连接状态与仅元数据读取。
    #[tokio::test]
    async fn mcp_inspection_preserves_schemas_and_never_executes_tools() {
        // server and inner exist only in this test and never start a process or network connection.
        // server 与 inner 仅存在于本测试中，从不启动进程或网络连接。
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("inspection"),
            name: String::from("Inspection"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        let mut inner = empty_runtime_inner();
        Arc::get_mut(&mut inner).expect("unique fixture").registry =
            McpServerRegistry::new(vec![server.clone()]);
        // input and output exercise unknown extension keywords and nested schemas without normalization.
        // input 与 output 使用未知扩展关键字和嵌套 Schema 验证不发生归一化。
        let input = json!({"type":"object", "properties":{"nested":{"type":"array","items":{"$ref":"#/$defs/item"}}}, "$defs":{"item":{"enum":["x","y"]}}, "x-extension":{"preserve":true}});
        let output =
            json!({"oneOf":[{"type":"string"},{"type":"null"}], "description":"original output"});
        let mut snapshot = initial_server_snapshot(&server);
        snapshot.status = McpRuntimeServerStatus::Connected;
        snapshot.tools.push(
            McpRuntimeToolDescriptor::new("inspection", "inspect", "original", input.clone())
                .with_result_schema(output.clone()),
        );
        inner
            .snapshot
            .write()
            .expect("snapshot lock")
            .servers
            .insert(server.id.clone(), snapshot);
        let (event_tx, _) = broadcast::channel(1);
        inner.transports.write().expect("transport lock").insert(
            server.id.clone(),
            Arc::new(ShutdownTrackingTransport {
                shutdown_count: Arc::new(AtomicUsize::new(0)),
                event_tx,
            }),
        );
        let handle = McpRuntimeHandle { inner };
        let inspection = handle.server_inspections();
        assert_eq!(
            inspection[0].state.status,
            McpRuntimeServerStatus::Connected
        );
        assert_eq!(inspection[0].tools[0].input_schema, input);
        assert_eq!(inspection[0].tools[0].output_schema, output);
        // A replacement catalog gets a new identity even when the server ID and connectivity remain unchanged.
        // 即使服务器标识与连接状态不变，替换后的目录仍获得新身份。
        let retained_tools = handle.snapshot().servers[&server.id].tools.clone();
        update_server_status(
            &handle.inner,
            &server.id,
            McpRuntimeServerStatus::Connected,
            None,
            retained_tools,
        );
        assert_ne!(
            handle.server_inspections()[0].inspection_generation,
            inspection[0].inspection_generation
        );
        let metadata = handle
            .server_capabilities(&server.id)
            .await
            .expect("list metadata on existing transport");
        assert_eq!(metadata["prompts"], json!([]));
        assert_eq!(metadata["resources"], json!([]));
        handle
            .inner
            .snapshot
            .write()
            .expect("snapshot lock")
            .servers
            .get_mut(&server.id)
            .expect("server")
            .status = McpRuntimeServerStatus::Disconnected;
        assert!(
            handle
                .server_capabilities(&server.id)
                .await
                .expect_err("disconnected reads must not connect")
                .contains("尚未连接")
        );
    }

    /// Builds empty shared MCP runtime state for deterministic teardown tests.
    /// 为确定性拆除测试构建空的共享 MCP 运行时状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns independently owned state with no background server tasks.
    /// 返回不含后台服务器任务的独立状态。
    fn empty_runtime_inner() -> Arc<McpRuntimeInner> {
        let (shutdown_tx, _) = watch::channel(false);
        Arc::new(McpRuntimeInner {
            registry: McpServerRegistry::new(Vec::new()),
            transports: RwLock::new(BTreeMap::new()),
            auth_sessions: AsyncRwLock::new(BTreeMap::new()),
            snapshot: RwLock::new(McpRuntimeSnapshot {
                servers: BTreeMap::new(),
            }),
            retry_counts: RwLock::new(BTreeMap::new()),
            shutdown_tx,
        })
    }

    /// Verifies registered and pre-publication transports both receive explicit shutdown before removal completes.
    /// 验证已注册与发布前传输都会在移除完成前收到显式关闭。
    #[tokio::test]
    async fn failed_transport_teardown_is_explicit_for_registered_and_unpublished_instances() {
        // inner owns the exact registered transport map under test.
        // inner 持有测试中的精确已注册传输映射。
        let inner = empty_runtime_inner();
        // registered_count observes the transport that already entered the runtime map.
        // registered_count 观察已经进入运行时映射的传输。
        let registered_count = Arc::new(AtomicUsize::new(0));
        // registered_events keeps the transport event contract alive for the fixture lifetime.
        // registered_events 在夹具生命周期内保持传输事件契约有效。
        let (registered_events, _) = broadcast::channel(1);
        inner
            .transports
            .write()
            .expect("lock registered transport map")
            .insert(
                String::from("registered"),
                Arc::new(ShutdownTrackingTransport {
                    shutdown_count: registered_count.clone(),
                    event_tx: registered_events,
                }),
            );
        remove_and_shutdown_server_transport(&inner, "registered", None).await;
        assert_eq!(registered_count.load(Ordering::SeqCst), 1);
        assert!(
            !inner
                .transports
                .read()
                .expect("read registered transport map")
                .contains_key("registered")
        );

        // unpublished_count observes a connection that failed before successful tool publication.
        // unpublished_count 观察在工具成功发布前失败的连接。
        let unpublished_count = Arc::new(AtomicUsize::new(0));
        // unpublished_events keeps the fallback transport event contract valid.
        // unpublished_events 保持回退传输的事件契约有效。
        let (unpublished_events, _) = broadcast::channel(1);
        // unpublished_transport is supplied directly because it never entered the runtime map.
        // unpublished_transport 被直接提供，因为它从未进入运行时映射。
        let unpublished_transport: Arc<dyn McpTransport> = Arc::new(ShutdownTrackingTransport {
            shutdown_count: unpublished_count.clone(),
            event_tx: unpublished_events,
        });
        remove_and_shutdown_server_transport(&inner, "unpublished", Some(unpublished_transport))
            .await;
        assert_eq!(unpublished_count.load(Ordering::SeqCst), 1);
    }

    /// Verifies an unresponsive optional discovery request cannot hold the server lifecycle task indefinitely.
    /// 验证无响应的可选能力发现请求不能无限占用服务器生命周期任务。
    #[tokio::test]
    async fn optional_capability_discovery_has_a_host_owned_deadline() {
        // server supplies deterministic identity for the timeout diagnostic.
        // server 为超时诊断提供确定性身份。
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("hanging-optional"),
            name: String::from("Hanging Optional"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
        };
        // hanging_request models a protocol peer that never answers prompt or resource discovery.
        // hanging_request 模拟永不响应 prompt 或 resource 发现的协议对端。
        let hanging_request =
            std::future::pending::<Result<(), vulcan_mcp_runtime::McpRuntimeError>>();
        // error proves the deadline is owned by the host rather than the transport implementation.
        // error 证明期限由宿主持有，而不是依赖传输实现。
        let error = run_bounded_optional_capability_discovery(
            &server,
            "prompt",
            Duration::from_millis(1),
            hanging_request,
        )
        .await
        .expect_err("time out unresponsive optional discovery");
        assert!(error.contains("prompt discovery timed out"));
    }

    /// Builds one tool schema fixture with caller-selected input and output schemas.
    /// 构建一份由调用方选择输入与输出 Schema 的工具 Schema 夹具。
    ///
    /// # Parameters
    /// 参数
    /// `name` is the exact tool name, while `args_schema` and `result_schema` are validated as one list entry.
    /// `name` 是精确工具名，`args_schema` 与 `result_schema` 作为单个列表项接受校验。
    ///
    /// # Returns
    /// 返回值
    /// Returns one host-owned MCP tool schema fixture.
    /// 返回一份宿主持有的 MCP 工具 Schema 夹具。
    fn tool_schema(
        name: &str,
        args_schema: serde_json::Value,
        result_schema: serde_json::Value,
    ) -> McpToolSchema {
        McpToolSchema {
            name: name.to_string(),
            description: String::from("test tool"),
            args_schema,
            result_schema,
        }
    }

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

    /// Verifies a latched shutdown request is not treated as completed transport teardown.
    /// 验证锁存的关闭请求不会被当作已经完成的传输拆除。
    #[tokio::test]
    async fn mcp_partition_reports_stopped_only_after_terminal_server_state() {
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("pending"),
            name: String::from("Pending"),
            enabled: false,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
        };
        let runtime = bootstrap_mcp_runtime(vec![server])
            .await
            .expect("build disabled test partition");
        runtime.shutdown();
        assert!(
            !runtime.is_fully_stopped(),
            "a shutdown request must not prove server teardown"
        );
        update_server_status(
            &runtime.inner,
            "pending",
            McpRuntimeServerStatus::Stopped,
            None,
            Vec::new(),
        );
        assert!(runtime.is_fully_stopped());
    }

    /// Verifies a replacement generation shares retired-lease safety and blocks uninstall until the old partition is terminally stopped.
    /// 验证替换代次共享退休租约安全状态，并在旧分区最终停止前持续阻止卸载。
    #[tokio::test]
    async fn retired_plugin_partition_remains_usable_until_terminal_stop() {
        // server models one disabled bootstrap partition that is manually advanced to an in-flight connecting state.
        // server 模拟一个停用引导分区，并由测试手动推进到执行中的连接状态。
        let server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::Plugin {
                plugin_id: String::from("fixture"),
                generation: 1,
            },
            id: String::from("fixture:server"),
            name: String::from("Fixture"),
            enabled: false,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("unused"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: false,
            },
        };
        // runtime owns the old generation partition whose terminal state controls package-removal safety.
        // runtime 持有旧代次分区，其最终状态控制插件包删除安全性。
        let runtime = bootstrap_mcp_runtime(vec![server])
            .await
            .expect("build retired plugin partition fixture");
        update_server_status(
            &runtime.inner,
            "fixture:server",
            McpRuntimeServerStatus::Connecting,
            None,
            Vec::new(),
        );
        // previousGeneration owns the retiring partition and the shared weak-lease registry.
        // previousGeneration 持有即将退休的分区及共享弱租约注册表。
        let previous_generation = McpRuntimeAccess::new(
            runtime.overview(),
            BTreeMap::from([(String::from("plugin:fixture:1"), runtime.clone())]),
        );
        // replacementGeneration removes the live partition while retaining the same retirement registry.
        // replacementGeneration 移除实时分区，同时保留同一退休注册表。
        let replacement_generation = previous_generation.without_plugin_partition("fixture");
        previous_generation.retire_plugin_partitions_after("fixture", Duration::from_secs(3_600));

        assert!(replacement_generation.has_usable_retired_plugin_partitions("fixture"));
        runtime.shutdown();
        assert!(
            replacement_generation.has_usable_retired_plugin_partitions("fixture"),
            "a shutdown request alone must not make package removal safe"
        );

        update_server_status(
            &runtime.inner,
            "fixture:server",
            McpRuntimeServerStatus::Stopped,
            None,
            Vec::new(),
        );
        assert!(!replacement_generation.has_usable_retired_plugin_partitions("fixture"));
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

    /// Verifies exponential reconnect delay follows the exact bounded sequence with deterministic sub-second jitter.
    /// 验证指数重连延迟遵循精确有界序列，并附带确定性的亚秒抖动。
    #[test]
    fn retry_delay_is_exponential_deterministic_and_capped() {
        let bases = [1_u64, 2, 4, 8, 16, 30, 30];
        for (attempt, base) in (1_u8..=7).zip(bases) {
            let first = retry_delay("plugin-a:server", attempt);
            let second = retry_delay("plugin-a:server", attempt);
            assert_eq!(first, second);
            assert!(first >= Duration::from_secs(base));
            assert!(first <= Duration::from_secs(base) + Duration::from_millis(250));
        }
    }

    /// Verifies a complete tool list accepts local references and rejects duplicates, external references, and structurally invalid schemas.
    /// 验证完整工具列表接受本地引用，并拒绝重复名称、外部引用和结构无效的 Schema。
    #[test]
    fn tool_schema_list_validation_rejects_every_atomicity_hazard() {
        let valid = tool_schema(
            "lookup",
            json!({
                "type": "object",
                "properties": {"query": {"$ref": "#/$defs/query"}},
                "$defs": {"query": {"type": "string"}}
            }),
            json!({"type": "object"}),
        );
        validate_mcp_tool_schema_list("fixture", std::slice::from_ref(&valid))
            .expect("valid complete list");

        let annotation_data = tool_schema(
            "annotation-data",
            json!({
                "type": "object",
                "default": {"$ref": "https://example.test/instance-value"},
                "examples": [{"$dynamicRef": "relative-instance-value"}]
            }),
            serde_json::Value::Null,
        );
        validate_mcp_tool_schema_list("fixture", &[annotation_data])
            .expect("reference-shaped annotation data is not a schema reference");

        let duplicate_error =
            validate_mcp_tool_schema_list("fixture", &[valid.clone(), valid.clone()])
                .expect_err("duplicate names must fail the whole list");
        assert!(duplicate_error.contains("duplicate tool name"));

        let non_object_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema("scalar", json!(true), serde_json::Value::Null)],
        )
        .expect_err("input schema must be an object schema document");
        assert!(non_object_error.contains("must be a JSON object schema"));

        // referenceKeywords covers every JSON Schema keyword whose value can escape the received document.
        // referenceKeywords 覆盖值可能逸出当前接收文档的每个 JSON Schema 引用关键字。
        for reference_keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
            let external_reference_error = validate_mcp_tool_schema_list(
                "fixture",
                &[tool_schema(
                    "external-reference",
                    json!({
                        "type": "object",
                        "properties": {
                            "secret": {(reference_keyword): "https://user:secret@example.test/schema.json"}
                        }
                    }),
                    serde_json::Value::Null,
                )],
            )
            .expect_err("external input references must fail the whole list");
            assert!(external_reference_error.contains("only local fragment references"));
            assert!(external_reference_error.contains(reference_keyword));
            assert!(!external_reference_error.contains("user:secret"));
            assert!(!external_reference_error.contains("example.test"));
        }

        let external_output_reference_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "external-output-reference",
                json!({"type": "object"}),
                json!({"$ref": "relative/schema.json"}),
            )],
        )
        .expect_err("external output references must fail the whole list");
        assert!(external_output_reference_error.contains("output schema"));
        assert!(external_output_reference_error.contains("only local fragment references"));

        let unresolved_local_reference_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "unresolved-local-reference",
                json!({"type": "object", "properties": {"missing": {"$ref": "#/$defs/missing"}}}),
                serde_json::Value::Null,
            )],
        )
        .expect_err("unresolved local references must fail schema compilation");
        assert!(unresolved_local_reference_error.contains("could not be compiled"));
        assert!(unresolved_local_reference_error.contains("Draft 2020-12"));

        let legacy_items_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "legacy-items-array",
                json!({"type": "object", "items": [{"type": "string"}]}),
                serde_json::Value::Null,
            )],
        )
        .expect_err("legacy draft syntax must not weaken the fixed Draft 2020-12 contract");
        assert!(legacy_items_error.contains("input schema is invalid"));

        let invalid_input_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "invalid-input",
                json!({"type": "not-a-json-schema-type"}),
                serde_json::Value::Null,
            )],
        )
        .expect_err("invalid input meta-schema must fail");
        assert!(invalid_input_error.contains("input schema is invalid"));

        let invalid_output_error = validate_mcp_tool_schema_list(
            "fixture",
            &[tool_schema(
                "invalid-output",
                json!({"type": "object"}),
                json!({"type": "not-a-json-schema-type"}),
            )],
        )
        .expect_err("invalid output meta-schema must fail");
        assert!(invalid_output_error.contains("output schema is invalid"));
    }

    /// Verifies isolated plugin validation accepts an empty exact partition and rejects foreign sources before launch.
    /// 验证隔离插件校验接受空的精确分区，并在启动前拒绝外部来源。
    #[tokio::test]
    async fn isolated_plugin_validation_enforces_exact_source_identity() {
        validate_plugin_mcp_runtime_partition("fixture", 7, Vec::new())
            .await
            .expect("empty exact plugin partition should validate and stop");

        // foreign_server must be rejected before its intentionally unusable process command is observed.
        // foreign_server 必须在其刻意不可用的进程命令被使用前遭到拒绝。
        let foreign_server = McpServerConfig {
            source: crate::mcp::config::McpServerSource::UserConfigured,
            id: String::from("foreign"),
            name: String::from("Foreign"),
            enabled: true,
            transport: crate::mcp::config::McpServerTransportConfig::Stdio {
                command: String::from("must-not-launch"),
                args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                env_passthrough: Vec::new(),
                inherit_environment: true,
            },
        };
        let error = validate_plugin_mcp_runtime_partition("fixture", 7, vec![foreign_server])
            .await
            .expect_err("foreign source must fail before launch");
        assert!(error.contains("another source"));
    }
}
