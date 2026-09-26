use super::*;
use crate::mcp::config::runtime_mcp_server_configs;
use crate::mcp::config::{McpServerSource, McpServerTransportConfig};
use crate::model::SessionId;
use crate::provider_config::ProviderConfigurationFile;
use std::sync::atomic::{AtomicBool, Ordering};

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
    /// Original optional result schema retained for read-only inspection.
    /// 为只读检查保留的原始可选结果 Schema。
    result_schema: serde_json::Value,
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
                result_schema: serde_json::Value::Null,
            },
        }
    }

    /// Retains the transport result schema without altering execution bindings.
    /// 保留传输层结果 Schema，不改变执行绑定。
    pub(super) fn with_result_schema(mut self, schema: serde_json::Value) -> Self {
        self.schema.result_schema = schema;
        self
    }

    /// Returns a complete metadata-only view without invoking the tool.
    /// 返回完整的纯元数据视图，不调用工具。
    pub(super) fn inspection(&self) -> McpRuntimeToolInspection {
        McpRuntimeToolInspection {
            name: self.schema.name.clone(),
            description: self.schema.description.clone(),
            input_schema: self.schema.args_schema.clone(),
            output_schema: self.schema.result_schema.clone(),
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
    /// Shared owner registry used to resolve the current lease after browser-grant replacement.
    /// 用于浏览器授权替换后解析当前租约的共享所有者注册表。
    owner_access: Option<McpRuntimeAccess>,
    /// Runtime-only owner key retained for exact invocation authorization.
    /// 仅运行时使用的所有者键，用于精确调用授权。
    owner_key: Option<McpRuntimeOwnedPartitionKey>,
    /// Optional plugin-lifecycle guard allowing this owner's browser tools to follow grant replacements.
    /// 可选插件生命周期闸门，允许该所有者的浏览器工具跟随授权替换。
    owner_replacement_guard: Option<Arc<AtomicBool>>,
}

/// Stable runtime-only identity for one plugin generation owned by one trusted workbench session.
/// 仅供运行时使用的稳定身份，用于标识某个插件代次及其可信工作台会话所有者。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct McpRuntimeOwnedPartitionKey {
    /// Stable plugin identifier from the immutable plugin source declaration.
    /// 来自不可变插件来源声明的稳定插件标识。
    plugin_id: String,
    /// Immutable plugin generation used to prevent replacement cross-talk.
    /// 用于阻止代次串扰的不可变插件代次。
    generation: u64,
    /// Trusted workbench session identity supplied by the host boundary.
    /// 由宿主边界提供的可信工作台会话身份。
    owner_session_id: SessionId,
}

impl McpRuntimeOwnedPartitionKey {
    /// Creates an owned partition key after rejecting ambiguous plugin or owner identities.
    /// 在拒绝含糊的插件或所有者身份后创建会话拥有的分区键。
    pub fn try_new(
        plugin_id: impl Into<String>,
        generation: u64,
        owner_session_id: SessionId,
    ) -> Result<Self, String> {
        let plugin_id = plugin_id.into();
        if plugin_id.trim().is_empty() {
            return Err(String::from(
                "owned MCP partition requires a non-empty plugin id",
            ));
        }
        if owner_session_id.is_nil() {
            return Err(String::from(
                "owned MCP partition requires a non-nil trusted owner session id",
            ));
        }
        Ok(Self {
            plugin_id,
            generation,
            owner_session_id,
        })
    }

    /// Returns the exact plugin identifier encoded in this runtime key.
    /// 返回此运行时键中编码的精确插件标识。
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Returns the immutable plugin generation encoded in this runtime key.
    /// 返回此运行时键中编码的不可变插件代次。
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the trusted owner session identity encoded in this runtime key.
    /// 返回此运行时键中编码的可信所有者会话身份。
    pub const fn owner_session_id(&self) -> SessionId {
        self.owner_session_id
    }

    /// Returns the source partition label without exposing the owner in public tool metadata.
    /// 返回不把所有者暴露到公开工具元数据中的来源分区标签。
    fn source_partition_key(&self) -> String {
        format!("plugin:{}:{}", self.plugin_id, self.generation)
    }
}

/// Capability handle for one session-owned stdio MCP partition.
/// 某个会话拥有的 stdio MCP 分区能力句柄。
#[derive(Debug, Clone)]
pub struct McpRuntimeOwnedStdioPartition {
    /// Shared MCP access registry used to resolve the current lease by exact owner key.
    /// 用于按精确所有者键解析当前租约的共享 MCP 访问注册表。
    access: McpRuntimeAccess,
    /// Runtime-only identity that never changes the public tool name or manifest schema.
    /// 不改变公开工具名称或 manifest schema 的仅运行时身份。
    key: McpRuntimeOwnedPartitionKey,
}

/// Reference-counted ownership boundary for one independently replaceable MCP runtime partition.
/// 单个可独立替换 MCP 运行时分区的引用计数所有权边界。
struct McpRuntimePartitionLease {
    /// Live runtime retained until the last generation or in-flight tool binding releases the partition.
    /// 保留到最后一个代次或执行中工具绑定释放分区为止的实时运行时。
    runtime: McpRuntimeHandle,
    /// Optional owner scope that turns this lease into a session-owned execution capability.
    /// 可选的所有者范围，用于把该租约变成会话拥有的执行能力。
    owner_key: Option<McpRuntimeOwnedPartitionKey>,
    /// Monotonic revocation latch checked before every owner-scoped invocation.
    /// 在每次所有者范围调用前检查的单调撤销锁存位。
    revoked: AtomicBool,
    /// Whether calls sharing this exact owner partition must keep arrival order.
    /// 共享该精确所有者分区的调用是否必须保持到达顺序。
    ordered_calls: bool,
    /// Serializes the owner-scoped revocation check so a queued call cannot bypass it.
    /// 串行化所有者范围撤销检查，避免排队调用绕过该检查。
    owner_call_gate: Option<Arc<tokio::sync::Mutex<()>>>,
}

/// Shared weak registry used to reject package removal while retired plugin leases can still execute.
/// 用于在已退休插件租约仍可执行时拒绝删除包的共享弱引用注册表。
type RetiredPluginPartitionRegistry =
    Arc<Mutex<BTreeMap<String, Vec<Weak<McpRuntimePartitionLease>>>>>;

/// Shared registry for opt-in session-owned stdio MCP leases.
/// 供可选会话拥有 stdio MCP 租约使用的共享注册表。
type OwnedStdioPartitionRegistry =
    Arc<Mutex<BTreeMap<McpRuntimeOwnedPartitionKey, Arc<McpRuntimePartitionLease>>>>;

/// Shared plugin gate that rejects new owner-scoped MCP work while a plugin is draining.
/// 在插件排空期间拒绝新的所有者范围 MCP 工作的共享插件闸门。
type BlockedOwnerPluginRegistry = Arc<Mutex<BTreeSet<String>>>;

/// Shared revocation guards for owner bindings that intentionally follow browser grant replacements.
/// 供有意跟随浏览器授权替换的所有者绑定共享的撤销闸门。
type OwnerReplacementGuardRegistry = Arc<Mutex<BTreeMap<String, Arc<AtomicBool>>>>;

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
        Self {
            runtime,
            owner_key: None,
            revoked: AtomicBool::new(false),
            ordered_calls: false,
            owner_call_gate: None,
        }
    }

    /// Wraps one independently booted runtime in a session-owned lease.
    /// 将一个独立引导的运行时包装为会话拥有的租约。
    fn new_owned(
        runtime: McpRuntimeHandle,
        owner_key: McpRuntimeOwnedPartitionKey,
        ordered_calls: bool,
    ) -> Self {
        Self {
            runtime,
            owner_key: Some(owner_key),
            revoked: AtomicBool::new(false),
            ordered_calls,
            owner_call_gate: Some(Arc::new(tokio::sync::Mutex::new(()))),
        }
    }

    /// Returns whether this lease is scoped to a session owner.
    /// 返回该租约是否绑定了会话所有者。
    fn is_owner_scoped(&self) -> bool {
        self.owner_key.is_some()
    }

    /// Marks this lease revoked before it can be removed from the owner registry.
    /// 在从所有者注册表移除前先将此租约标记为已撤销。
    fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }

    /// Returns whether this lease has been revoked or otherwise invalidated.
    /// 返回该租约是否已撤销或以其他方式失效。
    fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }

    /// Verifies one caller is the exact trusted owner and that the lease remains live.
    /// 校验调用者是精确的可信所有者且租约仍然有效。
    fn require_owner(&self, owner_session_id: SessionId) -> Result<(), String> {
        if self.is_revoked() {
            return Err(String::from("owned MCP tool binding has been revoked"));
        }
        let Some(owner_key) = self.owner_key.as_ref() else {
            return Err(String::from(
                "MCP tool binding is not scoped to a session owner",
            ));
        };
        if owner_key.owner_session_id != owner_session_id {
            return Err(String::from(
                "MCP tool binding owner does not match the caller",
            ));
        }
        Ok(())
    }

    /// Invokes one tool after a revocation check that queued calls cannot bypass.
    /// 在排队调用无法绕过的撤销检查之后调用单个工具。
    ///
    /// Browser page tools retain the gate through the remote call to preserve page-operation
    /// order. Independent tools release it after the revocation check and remain parallel.
    /// A call admitted before revocation may still complete or observe transport teardown.
    /// 浏览器页面工具在远端调用期间保持闸门以维护页面操作顺序；独立工具在撤销校验后
    /// 释放闸门并继续并行。撤销前已获准的调用仍可能完成或观察到传输拆除。
    async fn call_tool_for_owner(
        &self,
        owner_session_id: SessionId,
        tool_id: &str,
        args: serde_json::Value,
        audience: vulcan_mcp_runtime::McpToolResultAudience,
    ) -> Result<serde_json::Value, String> {
        let Some(owner_call_gate) = self.owner_call_gate.as_ref() else {
            return Err(String::from(
                "MCP tool binding is not scoped to a session owner",
            ));
        };
        // Ordered page tools retain the fair gate through the remote call; independent tools
        // release it after the revocation check and remain parallel.
        // 有序页面工具在远端调用期间保持公平闸门；独立工具在撤销校验后释放闸门并继续并行。
        let gate = owner_call_gate.lock().await;
        self.require_owner(owner_session_id)?;
        if self.ordered_calls {
            return self
                .runtime
                .call_tool_by_id_for_audience(tool_id, args, audience)
                .await;
        }
        drop(gate);
        self.runtime
            .call_tool_by_id_for_audience(tool_id, args, audience)
            .await
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
    /// Opt-in owner registry shared by every access generation derived from the same bootstrap access.
    /// 由同一引导访问面派生的所有访问代次共享的可选所有者注册表。
    owned_stdio_partitions: OwnedStdioPartitionRegistry,
    /// Shared lifecycle gate retained across published extension generations.
    /// 在已发布扩展代次之间持续共享的生命周期闸门。
    blocked_owner_plugins: BlockedOwnerPluginRegistry,
    /// Plugin-lifecycle guards shared across every access generation and revoked on drain.
    /// 跨所有访问代次共享并在插件排空时撤销的插件生命周期闸门。
    owner_replacement_guards: OwnerReplacementGuardRegistry,
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
            owned_stdio_partitions: Arc::new(Mutex::new(BTreeMap::new())),
            blocked_owner_plugins: Arc::new(Mutex::new(BTreeSet::new())),
            owner_replacement_guards: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Builds one access generation from already shared partition leases.
    /// 根据已经共享的分区租约构建一个访问代次。
    fn from_shared_partitions(
        configured_overview: McpRuntimeOverview,
        live_partitions: BTreeMap<String, Arc<McpRuntimePartitionLease>>,
        retired_plugin_partitions: RetiredPluginPartitionRegistry,
        owned_stdio_partitions: OwnedStdioPartitionRegistry,
        blocked_owner_plugins: BlockedOwnerPluginRegistry,
        owner_replacement_guards: OwnerReplacementGuardRegistry,
    ) -> Self {
        Self {
            configured_overview,
            live_partitions,
            retired_plugin_partitions,
            owned_stdio_partitions,
            blocked_owner_plugins,
            owner_replacement_guards,
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
            Arc::clone(&self.owned_stdio_partitions),
            Arc::clone(&self.blocked_owner_plugins),
            Arc::clone(&self.owner_replacement_guards),
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
            Arc::clone(&self.owned_stdio_partitions),
            Arc::clone(&self.blocked_owner_plugins),
            Arc::clone(&self.owner_replacement_guards),
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
            Arc::clone(&self.owned_stdio_partitions),
            Arc::clone(&self.blocked_owner_plugins),
            Arc::clone(&self.owner_replacement_guards),
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

    /// Captures complete per-server catalogs from current live partitions only.
    /// 仅从当前实时分区采集各服务器的完整目录。
    pub fn server_inspections(&self) -> Vec<McpRuntimeServerInspection> {
        self.live_partitions
            .values()
            .flat_map(|lease| lease.runtime.server_inspections())
            .collect()
    }

    /// Lists optional metadata on one existing connection without connecting or executing tools.
    /// 在单个现有连接列出可选元数据，不创建连接或执行工具。
    pub async fn server_capabilities(&self, server_id: &str) -> Result<serde_json::Value, String> {
        let runtime = self
            .partition_for_server(server_id)
            .ok_or_else(|| String::from("MCP 运行时未就绪。"))?;
        runtime.server_capabilities(server_id).await
    }

    /// Checks whether two access generations still own the same exact server lease.
    /// 检查两个访问代次是否仍持有同一精确服务器租约。
    pub fn shares_server_partition(&self, other: &Self, server_id: &str) -> bool {
        let lease = self
            .live_partitions
            .values()
            .find(|lease| lease.runtime.contains_server(server_id));
        let current = other
            .live_partitions
            .values()
            .find(|lease| lease.runtime.contains_server(server_id));
        matches!((lease, current), (Some(left), Some(right)) if Arc::ptr_eq(left, right))
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

    /// Opens or reuses one independently booted stdio partition for an exact plugin generation and owner session.
    /// 为精确插件代次及所有者会话启动或复用一个独立引导的 stdio 分区。
    ///
    /// The owner key is runtime-only; it is never added to public tool names, schemas, or manifests.
    /// 所有者键仅存在于运行时，不会加入公开工具名称、Schema 或 manifest。
    pub async fn open_owned_stdio_partition(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        servers: Vec<McpServerConfig>,
    ) -> Result<McpRuntimeOwnedStdioPartition, String> {
        self.open_owned_stdio_partition_with_ordering(
            plugin_id,
            generation,
            owner_session_id,
            servers,
            false,
        )
        .await
    }

    /// Opens a session-owned stdio partition whose page operations execute in arrival order.
    /// 打开一个按到达顺序执行页面操作的会话自有 stdio 分区。
    pub async fn open_ordered_owned_stdio_partition(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        servers: Vec<McpServerConfig>,
    ) -> Result<McpRuntimeOwnedStdioPartition, String> {
        self.open_owned_stdio_partition_with_ordering(
            plugin_id,
            generation,
            owner_session_id,
            servers,
            true,
        )
        .await
    }

    /// Opens or reuses one owner partition with an explicit tool ordering contract.
    /// 按显式工具顺序契约打开或复用一个所有者分区。
    async fn open_owned_stdio_partition_with_ordering(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        servers: Vec<McpServerConfig>,
        ordered_calls: bool,
    ) -> Result<McpRuntimeOwnedStdioPartition, String> {
        let key = McpRuntimeOwnedPartitionKey::try_new(plugin_id, generation, owner_session_id)?;
        validate_owned_stdio_partition(&key, &servers)?;
        self.ensure_owner_plugin_admission(plugin_id)?;

        if let Some(existing) = self.active_owned_stdio_lease(&key) {
            if existing.ordered_calls != ordered_calls {
                return Err(String::from(
                    "owned MCP partition call ordering cannot change while active",
                ));
            }
            return Ok(McpRuntimeOwnedStdioPartition {
                access: self.clone(),
                key,
            });
        }

        let runtime = bootstrap_owned_stdio_runtime(servers).await?;

        let new_lease = Arc::new(McpRuntimePartitionLease::new_owned(
            runtime,
            key.clone(),
            ordered_calls,
        ));
        let mut replaced_lease = None;
        let installed = {
            // Keep the lifecycle gate locked while publishing the lease so a disable that
            // starts during bootstrap cannot be overtaken by this late owner registration.
            // 在发布租约期间保持生命周期闸门锁定，避免引导期间开始的停用被迟到的所有者注册反超。
            let blocked_plugins = self
                .blocked_owner_plugins
                .lock()
                .expect("blocked MCP owner plugin registry poisoned");
            if blocked_plugins.contains(plugin_id) {
                new_lease.revoke();
                return Err(format!(
                    "owner-scoped MCP access for plugin `{plugin_id}` is draining"
                ));
            }
            let mut registry = self
                .owned_stdio_partitions
                .lock()
                .expect("owned MCP stdio partition registry poisoned");
            if let Some(existing) = registry.get(&key).cloned() {
                if existing.is_revoked() || existing.runtime.is_fully_stopped() {
                    replaced_lease = registry.insert(key.clone(), Arc::clone(&new_lease));
                    true
                } else {
                    if existing.ordered_calls != ordered_calls {
                        new_lease.revoke();
                        return Err(String::from(
                            "owned MCP partition call ordering cannot change while active",
                        ));
                    }
                    false
                }
            } else {
                registry.insert(key.clone(), Arc::clone(&new_lease));
                true
            }
        };
        if let Some(replaced_lease) = replaced_lease {
            replaced_lease.revoke();
        }
        if !installed {
            new_lease.revoke();
        }

        Ok(McpRuntimeOwnedStdioPartition {
            access: self.clone(),
            key,
        })
    }

    /// Replaces one owner partition only after the new stdio runtime is ready for execution.
    /// 仅在新的 stdio 运行时准备好执行后替换一个所有者分区。
    ///
    /// The old lease is revoked before shutdown so bindings captured from the previous browser grant fail closed.
    /// 旧租约会先撤销再关闭，因此从上一次浏览器授权捕获的绑定会安全失败。
    pub async fn replace_owned_stdio_partition(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        servers: Vec<McpServerConfig>,
    ) -> Result<McpRuntimeOwnedStdioPartition, String> {
        self.replace_owned_stdio_partition_with_ordering(
            plugin_id,
            generation,
            owner_session_id,
            servers,
            false,
        )
        .await
    }

    /// Replaces one owner partition with ordered page operations after readiness succeeds.
    /// 在就绪成功后以有序页面操作替换一个所有者分区。
    pub async fn replace_ordered_owned_stdio_partition(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        servers: Vec<McpServerConfig>,
    ) -> Result<McpRuntimeOwnedStdioPartition, String> {
        self.replace_owned_stdio_partition_with_ordering(
            plugin_id,
            generation,
            owner_session_id,
            servers,
            true,
        )
        .await
    }

    /// Replaces one ready owner partition under an explicit tool ordering contract.
    /// 按显式工具顺序契约替换一个就绪的所有者分区。
    async fn replace_owned_stdio_partition_with_ordering(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        servers: Vec<McpServerConfig>,
        ordered_calls: bool,
    ) -> Result<McpRuntimeOwnedStdioPartition, String> {
        let key = McpRuntimeOwnedPartitionKey::try_new(plugin_id, generation, owner_session_id)?;
        validate_owned_stdio_partition(&key, &servers)?;
        self.ensure_owner_plugin_admission(plugin_id)?;
        let runtime = bootstrap_owned_stdio_runtime(servers).await?;
        let new_lease = Arc::new(McpRuntimePartitionLease::new_owned(
            runtime,
            key.clone(),
            ordered_calls,
        ));
        // The gate remains held through replacement publication, so disable cannot race a
        // completed bootstrap into installing a new owner partition after revocation.
        // 替换发布期间持续持有闸门，避免停用在撤销后又竞态安装新的所有者分区。
        let blocked_plugins = self
            .blocked_owner_plugins
            .lock()
            .expect("blocked MCP owner plugin registry poisoned");
        if blocked_plugins.contains(plugin_id) {
            new_lease.revoke();
            new_lease.runtime.shutdown();
            return Err(format!(
                "owner-scoped MCP access for plugin `{plugin_id}` is draining"
            ));
        }
        let old_lease = self
            .owned_stdio_partitions
            .lock()
            .expect("owned MCP stdio partition registry poisoned")
            .insert(key.clone(), Arc::clone(&new_lease));
        if let Some(old_lease) = old_lease {
            old_lease.revoke();
            old_lease.runtime.shutdown();
        }
        Ok(McpRuntimeOwnedStdioPartition {
            access: self.clone(),
            key,
        })
    }

    /// Resolves and invokes one raw server/tool id through the current exact owner lease.
    /// 通过当前精确所有者租约解析并调用一个原始 server/tool 标识。
    ///
    /// This path never consults global or another-owner partitions after a browser grant changes.
    /// 浏览器授权变化后，此路径绝不查询全局分区或其他所有者分区。
    pub async fn invoke_owned_stdio_tool(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        origin_tool_id: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let key = McpRuntimeOwnedPartitionKey::try_new(plugin_id, generation, owner_session_id)?;
        let lease = self
            .active_owned_stdio_lease(&key)
            .ok_or_else(|| String::from("owned MCP stdio partition is unavailable"))?;
        lease
            .call_tool_for_owner(
                owner_session_id,
                origin_tool_id,
                args,
                vulcan_mcp_runtime::McpToolResultAudience::Machine,
            )
            .await
    }

    /// Blocks new owner-scoped MCP calls for one plugin and revokes every published owner lease.
    /// 封锁指定插件的新所有者范围 MCP 调用，并撤销全部已发布的所有者租约。
    ///
    /// Existing bindings retain their lease reference while the runtime observes the shutdown request;
    /// the revoked latch prevents queued calls from starting after this method returns.
    /// 已存在的绑定会在运行时处理关闭请求期间保留租约引用；撤销锁存位会阻止本方法返回后的排队调用启动。
    pub fn begin_plugin_owner_drain(&self, plugin_id: &str) -> Result<(), String> {
        if plugin_id.trim().is_empty() {
            return Err(String::from(
                "plugin owner MCP drain requires a non-empty plugin id",
            ));
        }
        let blocked_plugins = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
        if blocked_plugins.contains(plugin_id) {
            return Ok(());
        }
        let mut guards = self
            .owner_replacement_guards
            .lock()
            .map_err(|_| String::from("owner MCP replacement guard registry poisoned"))?;
        let mut blocked_plugins = blocked_plugins;
        blocked_plugins.insert(plugin_id.to_owned());
        if let Some(guard) = guards.remove(plugin_id) {
            guard.store(false, Ordering::Release);
        }
        drop(guards);
        let mut owner_registry = self
            .owned_stdio_partitions
            .lock()
            .map_err(|_| String::from("owned MCP stdio partition registry poisoned"))?;
        // Hold the gate and owner registry together so no new owner partition can be published
        // between the block and the lease removal.
        // 同时持有闸门与所有者注册表，确保封锁与租约移除之间不会发布新分区。
        let revoked_keys = owner_registry
            .keys()
            .filter(|key| key.plugin_id() == plugin_id)
            .cloned()
            .collect::<Vec<_>>();
        let revoked_leases = revoked_keys
            .into_iter()
            .filter_map(|key| owner_registry.remove(&key))
            .collect::<Vec<_>>();
        drop(owner_registry);
        drop(blocked_plugins);
        for lease in revoked_leases {
            lease.revoke();
            // Stop each removed owner runtime explicitly because the registry removal below can make later keyed cleanup a no-op.
            // 显式停止每个已移除的所有者运行时，因为注册表移除后后续按键清理可能变成空操作。
            lease.runtime.shutdown();
        }
        Ok(())
    }

    /// Reopens owner-scoped MCP admission after a plugin enable transition is ready.
    /// 在插件启用转换准备完成后重新开放所有者范围 MCP 准入。
    pub fn finish_plugin_owner_drain(&self, plugin_id: &str) -> Result<(), String> {
        if plugin_id.trim().is_empty() {
            return Err(String::from(
                "plugin owner MCP admission requires a non-empty plugin id",
            ));
        }
        self.blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?
            .remove(plugin_id);
        Ok(())
    }

    /// Resolves one live owner partition only when the exact owner key matches the registry entry.
    /// 仅当精确所有者键匹配注册表条目时解析一个仍有效的所有者分区。
    pub fn owned_stdio_partition(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
    ) -> Result<McpRuntimeOwnedStdioPartition, String> {
        let key = McpRuntimeOwnedPartitionKey::try_new(plugin_id, generation, owner_session_id)?;
        if self.active_owned_stdio_lease(&key).is_none() {
            return Err(String::from("owned MCP stdio partition is unavailable"));
        }
        Ok(McpRuntimeOwnedStdioPartition {
            access: self.clone(),
            key,
        })
    }

    /// Reports whether the exact owner partition is currently published and usable.
    /// 报告精确的所有者分区当前是否已发布且可用。
    ///
    /// This query is intended for connection-slot reuse decisions. A ready browser connection
    /// is not sufficient after plugin drain because the owner lease may have been revoked and its
    /// MCP process may already be shutting down.
    /// 此查询专门用于连接槽复用判定。插件排空后，即使浏览器连接仍标记为 ready，也不能代表所有者租约仍然存在，
    /// 因为该租约可能已经撤销且其 MCP 进程可能正在停止。
    pub fn has_active_owned_stdio_partition(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
    ) -> Result<bool, String> {
        let key = McpRuntimeOwnedPartitionKey::try_new(plugin_id, generation, owner_session_id)?;
        Ok(self.active_owned_stdio_lease(&key).is_some())
    }

    /// Returns owner-scoped bindings only after resolving the exact trusted owner session key.
    /// 仅在解析精确可信所有者会话键后返回所有者范围内的工具绑定。
    pub fn owned_stdio_tool_bindings(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        self.owned_stdio_tool_bindings_with_guard(
            plugin_id,
            generation,
            owner_session_id,
            allowlist,
            None,
        )
    }

    /// Discovers browser bindings that may follow a new grant for the same owner and plugin generation.
    /// 发现可跟随同一所有者及插件代次的新授权的浏览器绑定。
    ///
    /// The guard is permanently revoked when the plugin enters lifecycle drain.
    /// 插件进入生命周期排空时，该闸门会被永久撤销。
    pub fn owned_stdio_tool_bindings_following_replacement(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        let guard = {
            let blocked_plugins = self
                .blocked_owner_plugins
                .lock()
                .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
            if blocked_plugins.contains(plugin_id) {
                return Err(format!(
                    "owner-scoped MCP access for plugin `{plugin_id}` is draining"
                ));
            }
            self.owner_replacement_guards
                .lock()
                .map_err(|_| String::from("owner MCP replacement guard registry poisoned"))?
                .entry(String::from(plugin_id))
                .or_insert_with(|| Arc::new(AtomicBool::new(true)))
                .clone()
        };
        self.owned_stdio_tool_bindings_with_guard(
            plugin_id,
            generation,
            owner_session_id,
            allowlist,
            Some(guard),
        )
    }

    /// Builds one owner tool catalog with an explicit replacement policy.
    /// 按明确的替换策略构建所有者工具目录。
    fn owned_stdio_tool_bindings_with_guard(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
        allowlist: &McpRuntimeToolAllowlist,
        replacement_guard: Option<Arc<AtomicBool>>,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        let key = McpRuntimeOwnedPartitionKey::try_new(plugin_id, generation, owner_session_id)?;
        let lease = self
            .active_owned_stdio_lease(&key)
            .ok_or_else(|| String::from("owned MCP stdio partition is unavailable"))?;
        if replacement_guard
            .as_ref()
            .is_some_and(|guard| !guard.load(Ordering::Acquire))
        {
            return Err(String::from(
                "owned MCP tool binding was revoked by plugin drain",
            ));
        }
        let mut occupied_public_names = BTreeSet::new();
        let mut bindings = Vec::new();
        for mut view in lease.runtime.available_tool_views_filtered(allowlist) {
            let public_name =
                allocate_unique_public_tool_name(view.id(), &mut occupied_public_names);
            view.id = public_name.clone();
            view.name = public_name;
            bindings.push(McpRuntimeToolBinding::new_owned(
                view,
                lease.clone(),
                self.clone(),
                key.clone(),
                replacement_guard.clone(),
            ));
        }
        Ok(bindings)
    }

    /// Revokes one owner partition before dropping its registry lease, making all old bindings fail closed.
    /// 在丢弃注册表租约前撤销一个所有者分区，使所有旧绑定安全失败。
    pub fn revoke_owned_stdio_partition(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
    ) -> Result<(), String> {
        let key = McpRuntimeOwnedPartitionKey::try_new(plugin_id, generation, owner_session_id)?;
        if let Some(lease) = self.remove_owned_stdio_lease(&key) {
            lease.revoke();
        }
        Ok(())
    }

    /// Revokes and requests shutdown for one exact owner partition.
    /// 撤销并请求关闭一个精确的所有者分区。
    pub fn shutdown_owned_stdio_partition(
        &self,
        plugin_id: &str,
        generation: u64,
        owner_session_id: SessionId,
    ) -> Result<(), String> {
        let key = McpRuntimeOwnedPartitionKey::try_new(plugin_id, generation, owner_session_id)?;
        if let Some(lease) = self.remove_owned_stdio_lease(&key) {
            lease.revoke();
            lease.runtime.shutdown();
        }
        Ok(())
    }

    /// Resolves an active owner lease while removing stale or revoked registry entries.
    /// 解析有效的所有者租约，同时移除过期或已撤销的注册表条目。
    fn active_owned_stdio_lease(
        &self,
        key: &McpRuntimeOwnedPartitionKey,
    ) -> Option<Arc<McpRuntimePartitionLease>> {
        let blocked_plugins = self
            .blocked_owner_plugins
            .lock()
            .expect("blocked MCP owner plugin registry poisoned");
        if blocked_plugins.contains(key.plugin_id()) {
            return None;
        }
        let stale_lease = {
            let mut registry = self
                .owned_stdio_partitions
                .lock()
                .expect("owned MCP stdio partition registry poisoned");
            let Some(existing) = registry.get(key).cloned() else {
                return None;
            };
            if !existing.is_revoked() && !existing.runtime.is_fully_stopped() {
                return Some(existing);
            }
            registry.remove(key)
        };
        if let Some(stale_lease) = stale_lease {
            stale_lease.revoke();
        }
        None
    }

    /// Rejects owner admission while the exact plugin is in a lifecycle drain.
    /// 在精确插件处于生命周期排空期间拒绝所有者准入。
    fn ensure_owner_plugin_admission(&self, plugin_id: &str) -> Result<(), String> {
        let blocked_plugins = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
        if blocked_plugins.contains(plugin_id) {
            return Err(format!(
                "owner-scoped MCP access for plugin `{plugin_id}` is draining"
            ));
        }
        Ok(())
    }

    /// Removes one owner lease without touching any binding that may still hold its reference.
    /// 移除一个所有者租约，同时不触碰可能仍持有其引用的绑定。
    fn remove_owned_stdio_lease(
        &self,
        key: &McpRuntimeOwnedPartitionKey,
    ) -> Option<Arc<McpRuntimePartitionLease>> {
        self.owned_stdio_partitions
            .lock()
            .expect("owned MCP stdio partition registry poisoned")
            .remove(key)
    }

    /// Installs a prebuilt owner runtime for lifecycle tests without launching a child process.
    /// 为生命周期测试安装预构建的所有者运行时，避免启动子进程。
    #[cfg(test)]
    pub(super) fn register_owned_stdio_partition_for_test(
        &self,
        key: McpRuntimeOwnedPartitionKey,
        runtime: McpRuntimeHandle,
    ) -> McpRuntimeOwnedStdioPartition {
        self.register_owned_stdio_partition_with_ordering_for_test(key, runtime, false)
    }

    /// Installs a prebuilt owner runtime with explicit call ordering for concurrency tests.
    /// 为并发测试安装带显式调用顺序契约的预构建所有者运行时。
    #[cfg(test)]
    pub(super) fn register_owned_stdio_partition_with_ordering_for_test(
        &self,
        key: McpRuntimeOwnedPartitionKey,
        runtime: McpRuntimeHandle,
        ordered_calls: bool,
    ) -> McpRuntimeOwnedStdioPartition {
        let lease = Arc::new(McpRuntimePartitionLease::new_owned(
            runtime,
            key.clone(),
            ordered_calls,
        ));
        if let Some(previous) = self
            .owned_stdio_partitions
            .lock()
            .expect("owned MCP stdio partition registry poisoned")
            .insert(key.clone(), Arc::clone(&lease))
        {
            previous.revoke();
        }
        McpRuntimeOwnedStdioPartition {
            access: self.clone(),
            key,
        }
    }

    /// Requests the live MCP runtime to stop when one real runtime handle is currently available.
    /// 当当前存在真实运行时句柄时，请求实时 MCP 运行时停止。
    pub fn shutdown(&self) {
        let guards = std::mem::take(
            &mut *self
                .owner_replacement_guards
                .lock()
                .expect("owner MCP replacement guard registry poisoned"),
        );
        for guard in guards.into_values() {
            guard.store(false, Ordering::Release);
        }
        for lease in self.live_partitions.values() {
            lease.runtime.shutdown();
        }
        let owned_leases = {
            let mut registry = self
                .owned_stdio_partitions
                .lock()
                .expect("owned MCP stdio partition registry poisoned");
            std::mem::take(&mut *registry)
                .into_values()
                .collect::<Vec<_>>()
        };
        for lease in owned_leases {
            lease.revoke();
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

impl McpRuntimeOwnedStdioPartition {
    /// Returns the exact runtime-only owner key for this capability handle.
    /// 返回此能力句柄的精确仅运行时所有者键。
    pub fn key(&self) -> &McpRuntimeOwnedPartitionKey {
        &self.key
    }

    /// Returns the plugin identifier owned by this capability handle.
    /// 返回此能力句柄所属的插件标识。
    pub fn plugin_id(&self) -> &str {
        self.key.plugin_id()
    }

    /// Returns the immutable plugin generation owned by this capability handle.
    /// 返回此能力句柄所属的不可变插件代次。
    pub const fn generation(&self) -> u64 {
        self.key.generation()
    }

    /// Returns the trusted owner session identity for this capability handle.
    /// 返回此能力句柄对应的可信所有者会话身份。
    pub const fn owner_session_id(&self) -> SessionId {
        self.key.owner_session_id()
    }

    /// Queries owner-scoped bindings without publishing owner identity in tool metadata.
    /// 查询所有者范围内的工具绑定，同时不在工具元数据中发布所有者身份。
    pub fn available_tool_bindings_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        self.access.owned_stdio_tool_bindings(
            self.key.plugin_id(),
            self.key.generation(),
            self.key.owner_session_id(),
            allowlist,
        )
    }

    /// Revokes this capability and makes every binding cloned from it fail closed.
    /// 撤销此能力，并使从中克隆的所有绑定安全失败。
    pub fn revoke(&self) -> Result<(), String> {
        self.access.revoke_owned_stdio_partition(
            self.key.plugin_id(),
            self.key.generation(),
            self.key.owner_session_id(),
        )
    }

    /// Revokes this capability and requests shutdown of its dedicated process set.
    /// 撤销此能力，并请求关闭其专属进程集合。
    pub fn shutdown(&self) -> Result<(), String> {
        self.access.shutdown_owned_stdio_partition(
            self.key.plugin_id(),
            self.key.generation(),
            self.key.owner_session_id(),
        )
    }

    /// Invokes one raw server/tool id through the current owner lease after checking the caller identity.
    /// 在校验调用者身份后，通过当前所有者租约调用原始 server/tool 标识。
    ///
    /// `owner_session_id` must be the trusted session identity supplied by the host boundary.
    /// `owner_session_id` 必须是宿主边界提供的可信会话身份。
    pub async fn invoke_tool(
        &self,
        owner_session_id: SessionId,
        origin_tool_id: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.access
            .invoke_owned_stdio_tool(
                self.key.plugin_id(),
                self.key.generation(),
                owner_session_id,
                origin_tool_id,
                args,
            )
            .await
    }
}

/// Boots one owner-scoped stdio runtime and waits for complete tool discovery before publishing it.
/// 引导一个所有者范围 stdio 运行时，并在发布前等待完整工具发现。
async fn bootstrap_owned_stdio_runtime(
    servers: Vec<McpServerConfig>,
) -> Result<McpRuntimeHandle, String> {
    let runtime = super::handle::bootstrap_mcp_runtime(servers).await?;
    // startup_guard closes the unpublished child when this readiness wait is cancelled.
    // startup_guard 在这次就绪等待被取消时关闭尚未发布的子进程。
    let mut startup_guard = super::handle::McpRuntimeStartupGuard::new();
    startup_guard.track(&runtime);
    if let Err(error) = runtime.wait_until_ready(Duration::from_secs(15)).await {
        return Err(error);
    }
    startup_guard.mark_published();
    Ok(runtime)
}

/// Validates the exact plugin source and stdio-only transport contract before any child process launch.
/// 在启动任何子进程前校验精确插件来源及仅 stdio 传输契约。
fn validate_owned_stdio_partition(
    key: &McpRuntimeOwnedPartitionKey,
    servers: &[McpServerConfig],
) -> Result<(), String> {
    for server in servers {
        match (&server.source, &server.transport) {
            (
                McpServerSource::Plugin {
                    plugin_id,
                    generation,
                },
                McpServerTransportConfig::Stdio { .. },
            ) if plugin_id == key.plugin_id() && *generation == key.generation() => {}
            (
                McpServerSource::Plugin {
                    plugin_id,
                    generation,
                },
                _,
            ) if plugin_id == key.plugin_id() && *generation == key.generation() => {
                return Err(format!(
                    "owned MCP partition `{plugin_id}:{generation}` accepts stdio servers only; `{}` is not stdio",
                    server.id
                ));
            }
            _ => {
                return Err(format!(
                    "owned MCP partition `{}` received server `{}` from another source",
                    key.source_partition_key(),
                    server.id
                ));
            }
        }
    }
    Ok(())
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
            owner_access: None,
            owner_key: None,
            owner_replacement_guard: None,
        }
    }

    /// Builds an owner-scoped binding tied to one exact owner lease identity.
    /// 构建一个绑定到精确所有者租约身份的所有者范围绑定。
    fn new_owned(
        view: McpRuntimeToolView,
        runtime: Arc<McpRuntimePartitionLease>,
        access: McpRuntimeAccess,
        owner_key: McpRuntimeOwnedPartitionKey,
        owner_replacement_guard: Option<Arc<AtomicBool>>,
    ) -> Self {
        Self {
            view,
            runtime,
            partition_key: owner_key.source_partition_key(),
            owner_access: Some(access),
            owner_key: Some(owner_key),
            owner_replacement_guard,
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

    /// Invokes an owner-scoped binding through the current exact owner lease.
    /// 通过当前精确所有者租约调用所有者范围绑定。
    ///
    /// Owner bindings validate the shared registry and captured lease identity on every call.
    /// 所有者绑定在每次调用时校验共享注册表与捕获的租约身份。
    /// Returns the owner session for a session-scoped binding, without changing its public tool identity.
    /// 返回会话范围绑定的所有者会话，同时不改变其公开工具身份。
    pub fn owner_session_id(&self) -> Option<SessionId> {
        self.owner_key
            .as_ref()
            .map(McpRuntimeOwnedPartitionKey::owner_session_id)
    }

    /// Invokes the bound MCP tool through its stable host-facing tool identifier.
    /// 通过稳定宿主侧工具标识调用当前绑定的 MCP 工具。
    pub async fn invoke(&self, args: serde_json::Value) -> Result<serde_json::Value, String> {
        self.invoke_for_audience(args, vulcan_mcp_runtime::McpToolResultAudience::Machine)
            .await
    }

    /// Invokes an unowned binding with an explicit result audience and the existing owner guard.
    /// 使用明确结果消费者调用非所有者绑定，并保留既有所有者守卫。
    /// Returns the selected result without changing machine-facing invoke.
    /// 返回选定结果，不改变机器侧 invoke。
    pub(crate) async fn invoke_for_audience(
        &self,
        args: serde_json::Value,
        audience: vulcan_mcp_runtime::McpToolResultAudience,
    ) -> Result<serde_json::Value, String> {
        if self.runtime.is_owner_scoped() {
            return Err(String::from(
                "session-owned MCP binding requires invoke_for_owner",
            ));
        }
        if audience == vulcan_mcp_runtime::McpToolResultAudience::Machine {
            return self
                .runtime
                .runtime
                .call_tool_by_id(self.view.origin_tool_id(), args)
                .await;
        }
        self.runtime
            .runtime
            .call_tool_by_id_for_audience(self.view.origin_tool_id(), args, audience)
            .await
    }

    /// Invokes an owner-scoped binding only through its still-current exact owner lease.
    /// 仅通过仍然当前的精确所有者租约调用所有者范围绑定。
    ///
    /// Normal bindings reject replacements; explicitly guarded browser bindings follow only the same owner and plugin generation.
    /// 普通绑定拒绝替换；显式受闸门保护的浏览器绑定仅跟随同一所有者与插件代次。
    pub async fn invoke_for_owner(
        &self,
        owner_session_id: SessionId,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.invoke_for_owner_and_audience(
            owner_session_id,
            args,
            vulcan_mcp_runtime::McpToolResultAudience::Machine,
        )
        .await
    }

    /// Preserves every owner/revocation check while selecting only the final reading projection.
    /// 保留所有所有者及撤销检查，仅选择最终阅读投影。
    /// Inputs add the result audience to the existing owner invocation; returns its exact result.
    /// 输入在既有所有者调用上增加结果消费者；返回对应精确结果。
    pub(crate) async fn invoke_for_owner_and_audience(
        &self,
        owner_session_id: SessionId,
        args: serde_json::Value,
        audience: vulcan_mcp_runtime::McpToolResultAudience,
    ) -> Result<serde_json::Value, String> {
        if let Some(owner_key) = self.owner_key.as_ref() {
            if owner_key.owner_session_id() != owner_session_id {
                return Err(String::from(
                    "MCP tool binding owner does not match the caller",
                ));
            }
            let Some(access) = self.owner_access.as_ref() else {
                return Err(String::from("owned MCP binding has no owner registry"));
            };
            if self
                .owner_replacement_guard
                .as_ref()
                .is_some_and(|guard| !guard.load(Ordering::Acquire))
            {
                return Err(String::from(
                    "owned MCP tool binding was revoked by plugin drain",
                ));
            }
            let Some(current_lease) = access.active_owned_stdio_lease(owner_key) else {
                return Err(String::from("owned MCP stdio partition is unavailable"));
            };
            if !Arc::ptr_eq(&current_lease, &self.runtime) && self.owner_replacement_guard.is_none()
            {
                return Err(String::from(
                    "owned MCP tool binding is stale after owner lease replacement",
                ));
            }
            if self
                .owner_replacement_guard
                .as_ref()
                .is_some_and(|guard| !guard.load(Ordering::Acquire))
            {
                return Err(String::from(
                    "owned MCP tool binding was revoked by plugin drain",
                ));
            }
            return current_lease
                .call_tool_for_owner(owner_session_id, self.view.origin_tool_id(), args, audience)
                .await;
        }
        self.runtime
            .call_tool_for_owner(owner_session_id, self.view.origin_tool_id(), args, audience)
            .await
    }
}
