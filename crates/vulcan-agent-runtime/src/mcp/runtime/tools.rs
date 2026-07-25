use super::*;
use crate::mcp::config::runtime_mcp_server_configs;
use crate::provider_config::ProviderConfigurationFile;

/// 暴露给宿主运行时与执行规划器的稳定 MCP 工具 Schema。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpRuntimeToolSchema {
    /// Stable MCP tool name.
    /// 稳定 MCP 工具名称。
    name: String,
    /// Human-readable MCP tool description.
    /// 人类可读的 MCP 工具描述。
    description: String,
    /// JSON Schema describing the tool arguments.
    /// 描述工具参数的 JSON Schema。
    args_schema: serde_json::Value,
}

/// Stable MCP tool descriptor exposed to the host runtime and execution planner.
/// 暴露给宿主运行时与执行规划器的稳定 MCP 工具描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpRuntimeToolDescriptor {
    /// Stable server identifier that currently owns the tool.
    /// 当前拥有该工具的稳定服务器标识。
    server_id: String,
    /// Tool schema discovered from the remote MCP endpoint.
    /// 从远端 MCP 端点发现得到的工具 schema。
    schema: McpRuntimeToolSchema,
}

impl McpRuntimeToolDescriptor {
    /// Builds one stable MCP runtime tool descriptor from raw transport schema parts.
    /// 根据原始传输 schema 片段构建单个稳定 MCP 运行时工具描述。
    pub(super) fn new(
        server_id: impl Into<String>,
        tool_name: impl Into<String>,
        tool_description: impl Into<String>,
        args_schema: serde_json::Value,
    ) -> Self {
        Self {
            server_id: server_id.into(),
            schema: McpRuntimeToolSchema {
                name: tool_name.into(),
                description: tool_description.into(),
                args_schema,
            },
        }
    }

    /// Returns the stable server identifier that currently owns the tool.
    /// 返回当前拥有该工具的稳定服务器标识。
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    /// Returns one preferred public tool name before conflict resolution is applied.
    /// 返回应用冲突消解前的首选公开工具名。
    pub(super) fn preferred_public_tool_name(&self) -> String {
        build_prefixed_public_tool_name("mcp", &[self.server_id(), self.tool_name()])
    }

    /// Returns one host-facing read-only tool view derived from the runtime descriptor and one resolved public identifier.
    /// 根据运行时描述与一份已解析公开标识生成宿主侧只读工具视图。
    pub(super) fn host_tool_view(&self, public_tool_id: String) -> McpRuntimeToolView {
        McpRuntimeToolView {
            id: public_tool_id.clone(),
            name: public_tool_id,
            description: self.tool_description().to_string(),
            args_schema: self.args_schema().clone(),
            origin_tool_id: self.qualified_name(),
        }
    }

    /// Returns the stable MCP tool name.
    /// 返回稳定 MCP 工具名称。
    pub(super) fn tool_name(&self) -> &str {
        &self.schema.name
    }

    /// Returns the human-readable MCP tool description.
    /// 返回人类可读的 MCP 工具描述。
    pub(super) fn tool_description(&self) -> &str {
        &self.schema.description
    }

    /// Returns the JSON Schema describing the tool arguments.
    /// 返回描述工具参数的 JSON Schema。
    pub(super) fn args_schema(&self) -> &serde_json::Value {
        &self.schema.args_schema
    }

    /// Returns the host-facing qualified tool name composed from server and tool identifiers.
    /// 返回由服务器与工具标识组合而成的宿主侧限定工具名。
    pub(super) fn qualified_name(&self) -> String {
        format!("{}/{}", self.server_id(), self.tool_name())
    }

    /// Returns whether the descriptor matches one optional server/tool allowlist pair.
    /// 返回该描述是否匹配一组选填的服务器与工具白名单。
    pub fn matches_allowed_sets(
        &self,
        allowed_server_ids: Option<&BTreeSet<String>>,
        allowed_tool_names: Option<&BTreeSet<String>>,
    ) -> bool {
        let server_allowed =
            allowed_server_ids.is_none_or(|allowed| allowed.contains(self.server_id()));
        let tool_allowed = allowed_tool_names.is_none_or(|allowed| {
            allowed.contains(self.tool_name()) || allowed.contains(&self.qualified_name())
        });
        server_allowed && tool_allowed
    }
}

/// Host-facing read-only MCP tool view used by upper runtime projections and bridge layers.
/// 供上层运行时投影与桥接层使用的宿主侧只读 MCP 工具视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpRuntimeToolView {
    /// Stable host-facing tool identifier.
    /// 稳定的宿主侧工具标识。
    id: String,
    /// Human-readable tool name.
    /// 人类可读工具名称。
    name: String,
    /// Human-readable tool description.
    /// 人类可读工具描述。
    description: String,
    /// JSON Schema describing the tool arguments.
    /// 描述工具参数的 JSON Schema。
    args_schema: serde_json::Value,
    /// Original qualified MCP tool identifier preserved for live invocation.
    /// 为实时调用保留的原始限定 MCP 工具标识。
    origin_tool_id: String,
}

impl McpRuntimeToolView {
    /// Returns the stable host-facing tool identifier.
    /// 返回稳定的宿主侧工具标识。
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    /// Returns the human-readable tool name.
    /// 返回人类可读工具名称。
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    /// Returns the human-readable tool description.
    /// 返回人类可读工具描述。
    pub(super) fn description(&self) -> &str {
        &self.description
    }

    /// Returns the JSON Schema describing the tool arguments.
    /// 返回描述工具参数的 JSON Schema。
    pub(super) fn args_schema(&self) -> &serde_json::Value {
        &self.args_schema
    }

    /// Returns the original qualified MCP tool identifier preserved for live invocation.
    /// 返回为实时调用保留的原始限定 MCP 工具标识。
    pub(super) fn origin_tool_id(&self) -> &str {
        &self.origin_tool_id
    }
}

/// Stable MCP runtime tool binding that couples one host-facing tool view with its live invocation capability.
/// 将宿主侧工具视图与实时调用能力耦合在一起的稳定 MCP 运行时工具绑定。
#[derive(Debug, Clone)]
pub struct McpRuntimeToolBinding {
    /// Host-facing read-only tool view.
    /// 宿主侧只读工具视图。
    view: McpRuntimeToolView,
    /// Live runtime handle used to resolve the owning transport at invocation time.
    /// 在调用时用于解析所属传输的实时运行时句柄。
    runtime: Arc<McpRuntimePartitionLease>,
    /// Stable partition identity that prevents same-server-id source confusion.
    /// 防止同服务器标识来源混淆的稳定分区身份。
    partition_key: String,
}

/// Reference-counted ownership boundary for one independently replaceable MCP runtime partition.
/// 单个可独立替换 MCP 运行时分区的引用计数所有权边界。
struct McpRuntimePartitionLease {
    /// Live runtime retained until the last generation or in-flight tool binding releases the partition.
    /// 保留到最后一个代次或执行中工具绑定释放分区为止的实时运行时。
    runtime: McpRuntimeHandle,
}

/// Shared weak registry used to reject package removal while retired plugin leases can still execute.
/// 用于在已退休插件租约仍可执行时拒绝删除包的共享弱引用注册表。
type RetiredPluginPartitionRegistry =
    Arc<Mutex<BTreeMap<String, Vec<Weak<McpRuntimePartitionLease>>>>>;

impl std::fmt::Debug for McpRuntimePartitionLease {
    /// Formats only the lease type so transport configuration and credential-bearing state never enter logs.
    /// 仅格式化租约类型，避免传输配置与含凭据状态进入日志。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpRuntimePartitionLease")
            .finish_non_exhaustive()
    }
}

impl Drop for McpRuntimePartitionLease {
    /// Stops the partition when no published generation or in-flight binding still owns its lease.
    /// 当已发布代次与执行中绑定均不再持有租约时停止该分区。
    fn drop(&mut self) {
        self.runtime.shutdown();
    }
}

impl McpRuntimePartitionLease {
    /// Wraps one newly booted runtime in a shared lifecycle lease.
    /// 将一个新引导的运行时包装为共享生命周期租约。
    fn new(runtime: McpRuntimeHandle) -> Self {
        Self { runtime }
    }
}

/// Stable MCP runtime access surface that resolves live-versus-configured reads inside the MCP domain.
/// 在 MCP 域内部解析实时与配置读取来源的稳定 MCP 运行时访问面。
#[derive(Debug, Clone)]
pub struct McpRuntimeAccess {
    /// Lightweight configured MCP overview retained for fallback reads.
    /// 为回退读取保留的轻量 MCP 配置概览。
    configured_overview: McpRuntimeOverview,
    /// Independently replaceable live runtime partitions keyed by stable source identity.
    /// 以稳定来源身份为键、可独立替换的实时运行时分区。
    live_partitions: BTreeMap<String, Arc<McpRuntimePartitionLease>>,
    /// Weak retired-lease registry shared by every generation derived from the same bootstrap access.
    /// 由同一引导访问面派生的每个代次共享的弱退休租约注册表。
    retired_plugin_partitions: RetiredPluginPartitionRegistry,
}

impl McpRuntimeAccess {
    /// Builds one stable MCP runtime access surface from configured summary data and one optional live runtime handle.
    /// 根据配置摘要数据与一个可选实时运行时句柄构建单个稳定 MCP 运行时访问面。
    pub(super) fn new(
        configured_overview: McpRuntimeOverview,
        live_partitions: BTreeMap<String, McpRuntimeHandle>,
    ) -> Self {
        Self {
            configured_overview,
            live_partitions: live_partitions
                .into_iter()
                .map(|(key, runtime)| (key, Arc::new(McpRuntimePartitionLease::new(runtime))))
                .collect(),
            retired_plugin_partitions: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Builds one access generation from already shared partition leases.
    /// 根据已经共享的分区租约构建一个访问代次。
    fn from_shared_partitions(
        configured_overview: McpRuntimeOverview,
        live_partitions: BTreeMap<String, Arc<McpRuntimePartitionLease>>,
        retired_plugin_partitions: RetiredPluginPartitionRegistry,
    ) -> Self {
        Self {
            configured_overview,
            live_partitions,
            retired_plugin_partitions,
        }
    }

    /// Builds a new access generation by replacing only the user-configured partition.
    /// 仅替换用户配置分区并构建新的访问代次。
    pub(super) fn with_replaced_user_partition(
        &self,
        user_overview: McpRuntimeOverview,
        user_runtime: McpRuntimeHandle,
    ) -> Self {
        let mut live_partitions = self.live_partitions.clone();
        live_partitions.insert(
            String::from("user"),
            Arc::new(McpRuntimePartitionLease::new(user_runtime)),
        );
        let configured_overview = merge_mcp_runtime_overviews(
            std::iter::once(user_overview).chain(
                live_partitions
                    .iter()
                    .filter(|(key, _)| key.as_str() != "user")
                    .map(|(_, lease)| lease.runtime.overview()),
            ),
        );
        Self::from_shared_partitions(
            configured_overview,
            live_partitions,
            Arc::clone(&self.retired_plugin_partitions),
        )
    }

    /// Builds a new access generation by replacing only the host-managed Codebase Memory partition.
    /// 仅替换宿主管理的 Codebase Memory 分区并构建新的访问代次。
    pub(super) fn with_replaced_managed_codebase_memory_partition(
        &self,
        managed_overview: McpRuntimeOverview,
        managed_runtime: McpRuntimeHandle,
    ) -> Self {
        let partition_key = crate::mcp::config::HOST_MANAGED_CODEBASE_MEMORY_PARTITION_KEY;
        let mut live_partitions = self.live_partitions.clone();
        live_partitions.insert(
            String::from(partition_key),
            Arc::new(McpRuntimePartitionLease::new(managed_runtime)),
        );
        let configured_overview = merge_mcp_runtime_overviews(
            std::iter::once(managed_overview).chain(
                live_partitions
                    .iter()
                    .filter(|(key, _)| key.as_str() != partition_key)
                    .map(|(_, lease)| lease.runtime.overview()),
            ),
        );
        Self::from_shared_partitions(
            configured_overview,
            live_partitions,
            Arc::clone(&self.retired_plugin_partitions),
        )
    }

    /// Builds a new access generation without the host-managed Codebase Memory partition.
    /// 构建不包含宿主管理 Codebase Memory 分区的新访问代次。
    pub fn without_managed_codebase_memory_partition(&self) -> Self {
        let mut live_partitions = self.live_partitions.clone();
        live_partitions.remove(crate::mcp::config::HOST_MANAGED_CODEBASE_MEMORY_PARTITION_KEY);
        let configured_overview = merge_mcp_runtime_overviews(
            live_partitions
                .values()
                .map(|lease| lease.runtime.overview()),
        );
        Self::from_shared_partitions(
            configured_overview,
            live_partitions,
            Arc::clone(&self.retired_plugin_partitions),
        )
    }

    /// Builds a new access generation by replacing every prior generation of one plugin partition.
    /// 通过替换单个插件分区的全部旧代次来构建新的访问代次。
    pub(super) fn with_replaced_plugin_partition(
        &self,
        plugin_id: &str,
        generation: u64,
        plugin_overview: McpRuntimeOverview,
        plugin_runtime: McpRuntimeHandle,
    ) -> Self {
        let partition_prefix = format!("plugin:{plugin_id}:");
        let mut live_partitions = self.live_partitions.clone();
        live_partitions.retain(|key, _| !key.starts_with(&partition_prefix));
        live_partitions.insert(
            format!("{partition_prefix}{generation}"),
            Arc::new(McpRuntimePartitionLease::new(plugin_runtime)),
        );
        let configured_overview = merge_mcp_runtime_overviews(
            std::iter::once(plugin_overview).chain(
                live_partitions
                    .iter()
                    .filter(|(key, _)| !key.starts_with(&partition_prefix))
                    .map(|(_, lease)| lease.runtime.overview()),
            ),
        );
        Self::from_shared_partitions(
            configured_overview,
            live_partitions,
            Arc::clone(&self.retired_plugin_partitions),
        )
    }

    /// Builds a new access generation without any live partition owned by one plugin.
    /// 构建一个不包含指定插件任何实时分区的新访问代次。
    pub(super) fn without_plugin_partition(&self, plugin_id: &str) -> Self {
        let partition_prefix = format!("plugin:{plugin_id}:");
        let mut live_partitions = self.live_partitions.clone();
        live_partitions.retain(|key, _| !key.starts_with(&partition_prefix));
        let configured_overview = merge_mcp_runtime_overviews(
            live_partitions
                .values()
                .map(|lease| lease.runtime.overview()),
        );
        Self::from_shared_partitions(
            configured_overview,
            live_partitions,
            Arc::clone(&self.retired_plugin_partitions),
        )
    }

    /// Returns one stable MCP runtime overview backed by live runtime state when available, or configured fallback data otherwise.
    /// 返回一份在实时运行时可用时优先使用实时状态、否则回退到配置摘要数据的稳定 MCP 运行时概览。
    pub fn overview(&self) -> McpRuntimeOverview {
        if self.live_partitions.is_empty() {
            return self.configured_overview.clone();
        }
        merge_mcp_runtime_overviews(
            self.live_partitions
                .values()
                .map(|lease| lease.runtime.overview()),
        )
    }

    /// Returns structured server states merged from every current live partition.
    /// 返回从当前每个实时分区合并得到的结构化服务器状态。
    pub fn server_states(&self) -> Vec<McpRuntimeServerState> {
        self.live_partitions
            .values()
            .flat_map(|lease| lease.runtime.server_states())
            .collect()
    }

    /// Returns currently available MCP tool bindings filtered by one allowlist when live runtime state is available.
    /// 当实时运行时状态可用时，返回按单份白名单过滤后的当前可用 MCP 工具绑定集合。
    pub fn available_tool_bindings_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Vec<McpRuntimeToolBinding> {
        let mut occupied_public_names = BTreeSet::new();
        let mut bindings = Vec::new();
        for (partition_key, lease) in &self.live_partitions {
            for mut view in lease.runtime.available_tool_views_filtered(allowlist) {
                let public_name =
                    allocate_unique_public_tool_name(view.id(), &mut occupied_public_names);
                view.id = public_name.clone();
                view.name = public_name;
                bindings.push(McpRuntimeToolBinding::new(
                    view,
                    Arc::clone(lease),
                    partition_key.clone(),
                ));
            }
        }
        bindings
    }

    /// Requests the live MCP runtime to stop when one real runtime handle is currently available.
    /// 当当前存在真实运行时句柄时，请求实时 MCP 运行时停止。
    pub fn shutdown(&self) {
        for lease in self.live_partitions.values() {
            lease.runtime.shutdown();
        }
    }

    /// Requests only the stable user-configured runtime partition to stop.
    /// 仅请求稳定的用户配置运行时分区停止。
    pub fn shutdown_user_partition(&self) {
        if let Some(lease) = self.live_partitions.get("user") {
            lease.runtime.shutdown();
        }
    }

    /// Requests only the host-managed Codebase Memory runtime partition to stop.
    /// 仅请求宿主管理的 Codebase Memory 运行时分区停止。
    pub fn shutdown_managed_codebase_memory_partition(&self) {
        if let Some(lease) = self
            .live_partitions
            .get(crate::mcp::config::HOST_MANAGED_CODEBASE_MEMORY_PARTITION_KEY)
        {
            lease.runtime.shutdown();
        }
    }

    /// Schedules a forced stop for the retired user partition without extending its natural lease lifetime.
    /// 为已退休用户分区安排强制停止，同时不延长其自然租约生命周期。
    ///
    /// # Parameters
    /// 参数
    /// `maximum_drain` is the longest time an in-flight execution may retain the retired partition.
    /// `maximum_drain` 是执行中任务可继续保留已退休分区的最长时间。
    pub fn retire_user_partition_after(&self, maximum_drain: std::time::Duration) {
        let Some(lease) = self.live_partitions.get("user") else {
            return;
        };
        // weakLease lets normal generation release stop the runtime immediately instead of keeping it alive for the timeout.
        // weakLease 允许代次正常释放时立即停止运行时，而不会为了超时窗口延长其生命期。
        let weak_lease = Arc::downgrade(lease);
        tokio::spawn(async move {
            tokio::time::sleep(maximum_drain).await;
            if let Some(lease) = weak_lease.upgrade() {
                lease.runtime.shutdown();
            }
        });
    }

    /// Schedules forced retirement for every prior runtime generation owned by one plugin.
    /// 为指定插件持有的全部旧运行时代次安排强制退休。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the exact plugin namespace and `maximum_drain` limits in-flight access to old generations.
    /// `plugin_id` 选择精确插件命名空间，`maximum_drain` 限制执行中任务访问旧代次的时长。
    pub fn retire_plugin_partitions_after(
        &self,
        plugin_id: &str,
        maximum_drain: std::time::Duration,
    ) {
        let partition_prefix = format!("plugin:{plugin_id}:");
        let retired_leases = self
            .live_partitions
            .iter()
            .filter(|(key, _)| key.starts_with(&partition_prefix))
            .map(|(_, lease)| Arc::downgrade(lease))
            .collect::<Vec<_>>();
        if retired_leases.is_empty() {
            return;
        }
        self.retired_plugin_partitions
            .lock()
            .expect("retired MCP plugin partition registry poisoned")
            .entry(plugin_id.to_string())
            .or_default()
            .extend(retired_leases.iter().cloned());
        tokio::spawn(async move {
            tokio::time::sleep(maximum_drain).await;
            for weak_lease in retired_leases {
                if let Some(lease) = weak_lease.upgrade() {
                    lease.runtime.shutdown();
                }
            }
        });
    }

    /// Returns whether a retired partition for one plugin still owns a usable runtime lease.
    /// 返回指定插件的已退休分区是否仍持有可用运行时租约。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the exact package whose removal safety is being checked.
    /// `plugin_id` 选择正在检查删除安全性的精确包。
    ///
    /// # Returns
    /// 返回值
    /// Returns true until all leases are released or their thirty-second forced shutdown has fired.
    /// 在全部租约释放或其三十秒强制关闭触发前返回 true。
    pub fn has_usable_retired_plugin_partitions(&self, plugin_id: &str) -> bool {
        let mut registry = self
            .retired_plugin_partitions
            .lock()
            .expect("retired MCP plugin partition registry poisoned");
        let has_usable_leases = registry.get_mut(plugin_id).is_some_and(|leases| {
            leases.retain(|weak_lease| {
                weak_lease
                    .upgrade()
                    .is_some_and(|lease| !lease.runtime.is_fully_stopped())
            });
            !leases.is_empty()
        });
        if !has_usable_leases {
            registry.remove(plugin_id);
        }
        has_usable_leases
    }

    /// Returns the current OAuth status label for one configured MCP server when live runtime state is available.
    /// 当实时运行时状态可用时，返回单个已配置 MCP 服务器的当前 OAuth 状态标签。
    pub async fn server_oauth_status_label(&self, server_id: &str) -> Result<String, String> {
        let Some(runtime) = self.partition_for_server(server_id) else {
            return Ok(String::from("运行时未就绪"));
        };
        runtime.server_auth_status_label(server_id).await
    }

    /// Starts one host-driven OAuth flow for the target MCP server and returns the browser challenge payload.
    /// 为目标 MCP 服务器启动一条宿主驱动的 OAuth 流程，并返回浏览器挑战载荷。
    pub async fn start_server_oauth(
        &self,
        server_id: &str,
        client_name: &str,
    ) -> Result<McpAuthChallenge, String> {
        let Some(runtime) = self.partition_for_server(server_id) else {
            return Err(String::from(
                "MCP 运行时尚未就绪，暂时无法启动 OAuth 授权。",
            ));
        };
        runtime.start_server_auth(server_id, client_name).await
    }

    /// Finishes one host-driven OAuth flow for the target MCP server with the returned authorization code and state.
    /// 使用回传的授权码与 state 为目标 MCP 服务器完成一条宿主驱动的 OAuth 流程。
    pub async fn finish_server_oauth(
        &self,
        server_id: &str,
        authorization_code: &str,
        oauth_state: &str,
    ) -> Result<McpAuthStatus, String> {
        let Some(runtime) = self.partition_for_server(server_id) else {
            return Err(String::from(
                "MCP 运行时尚未就绪，暂时无法完成 OAuth 授权。",
            ));
        };
        runtime
            .finish_server_auth(server_id, authorization_code, oauth_state)
            .await
    }

    /// Removes stored OAuth credentials for the target MCP server when live runtime state is available.
    /// 当实时运行时状态可用时，为目标 MCP 服务器移除已存储的 OAuth 凭据。
    pub async fn remove_server_oauth(&self, server_id: &str) -> Result<(), String> {
        let Some(runtime) = self.partition_for_server(server_id) else {
            return Err(String::from(
                "MCP 运行时尚未就绪，暂时无法清理 OAuth 凭据。",
            ));
        };
        runtime.remove_server_auth(server_id).await
    }

    /// Resolves the unique live partition that owns one exact server identifier.
    /// 解析持有某个精确服务器标识的唯一实时分区。
    fn partition_for_server(&self, server_id: &str) -> Option<&McpRuntimeHandle> {
        self.live_partitions
            .values()
            .find(|lease| lease.runtime.contains_server(server_id))
            .map(|lease| &lease.runtime)
    }
}

/// Builds one stable MCP runtime access surface from provider configuration without a live runtime handle.
/// 根据供应商配置构建一个不包含实时运行时句柄的稳定 MCP 运行时访问面。
pub fn configured_mcp_runtime_access(config: &ProviderConfigurationFile) -> McpRuntimeAccess {
    match runtime_mcp_server_configs(config) {
        Ok(servers) => {
            McpRuntimeAccess::new(configured_mcp_registry_overview(servers), BTreeMap::new())
        }
        Err(error) => McpRuntimeAccess::new(
            McpRuntimeOverview::new(
                vec![format!("MCP configuration is invalid: {error}")],
                Vec::new(),
                McpRuntimeDiscoveryReport {
                    discovery_errors: vec![error],
                    ..McpRuntimeDiscoveryReport::default()
                },
            ),
            BTreeMap::new(),
        ),
    }
}

/// Merges independently owned MCP partition overviews into one deterministic access surface.
/// 将独立持有的 MCP 分区概览合并为一个确定性的访问表面。
fn merge_mcp_runtime_overviews(
    overviews: impl IntoIterator<Item = McpRuntimeOverview>,
) -> McpRuntimeOverview {
    let mut summary_rows = Vec::new();
    let mut active_summary_rows = Vec::new();
    let mut report = McpRuntimeDiscoveryReport::default();
    for overview in overviews {
        summary_rows.extend(overview.summary_rows);
        active_summary_rows.extend(overview.active_summary_rows);
        report
            .event_projections
            .extend(overview.discovery_report.event_projections);
        report
            .discovery_rows
            .extend(overview.discovery_report.discovery_rows);
        report
            .discovery_errors
            .extend(overview.discovery_report.discovery_errors);
        report
            .discovery_warnings
            .extend(overview.discovery_report.discovery_warnings);
    }
    McpRuntimeOverview::new(summary_rows, active_summary_rows, report)
}

impl McpRuntimeToolBinding {
    /// Builds one MCP runtime tool binding from one host-facing tool view and one live runtime handle.
    /// 根据一份宿主侧工具视图与一份实时运行时句柄构建单个 MCP 运行时工具绑定。
    fn new(
        view: McpRuntimeToolView,
        runtime: Arc<McpRuntimePartitionLease>,
        partition_key: String,
    ) -> Self {
        Self {
            view,
            runtime,
            partition_key,
        }
    }

    /// Returns the stable source partition that owns this binding.
    /// 返回拥有当前绑定的稳定来源分区。
    pub fn partition_key(&self) -> &str {
        &self.partition_key
    }

    /// Returns the stable host-facing tool identifier.
    /// 返回稳定的宿主侧工具标识。
    pub fn id(&self) -> &str {
        self.view.id()
    }

    /// Returns the human-readable tool name.
    /// 返回人类可读工具名称。
    pub fn name(&self) -> &str {
        self.view.name()
    }

    /// Returns the human-readable tool description.
    /// 返回人类可读工具描述。
    pub fn description(&self) -> &str {
        self.view.description()
    }

    /// Returns the JSON Schema describing the tool arguments.
    /// 返回描述工具参数的 JSON Schema。
    pub fn args_schema(&self) -> &serde_json::Value {
        self.view.args_schema()
    }

    /// Returns the original runtime-qualified MCP tool identifier.
    /// 返回原始运行时限定 MCP 工具标识。
    pub fn origin_tool_id(&self) -> &str {
        self.view.origin_tool_id()
    }

    /// Returns the source MCP server identifier parsed from the original tool id.
    /// 返回从原始工具标识解析出的来源 MCP 服务标识。
    pub fn server_id(&self) -> Option<&str> {
        self.view
            .origin_tool_id()
            .split_once('/')
            .map(|(server_id, _)| server_id)
            .filter(|server_id| !server_id.is_empty())
    }

    /// Returns the source MCP tool name parsed from the original tool id.
    /// 返回从原始工具标识解析出的来源 MCP 工具名称。
    pub fn origin_tool_name(&self) -> Option<&str> {
        self.view
            .origin_tool_id()
            .split_once('/')
            .map(|(_, tool_name)| tool_name)
            .filter(|tool_name| !tool_name.is_empty())
    }

    /// Invokes the bound MCP tool through its stable host-facing tool identifier.
    /// 通过稳定宿主侧工具标识调用当前绑定的 MCP 工具。
    pub async fn invoke(&self, args: serde_json::Value) -> Result<serde_json::Value, String> {
        self.runtime
            .runtime
            .call_tool_by_id(self.view.origin_tool_id(), args)
            .await
    }
}
