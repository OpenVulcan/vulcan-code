use super::*;

#[cfg(test)]
mod original_inventory_tests;
#[cfg(test)]
pub(super) mod owner_drain_tests;
#[cfg(test)]
mod user_admission_tests;
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
pub(super) struct McpRuntimePartitionLease {
    /// Opaque identity belongs to this actual lease and survives every binding/access clone.
    /// 不透明身份属于此实际租约，并在每个绑定／访问克隆期间保持一致。
    pub(super) instance_id: uuid::Uuid,
    /// Live runtime retained until the last generation or in-flight tool binding releases the partition.
    /// 保留到最后一个代次或执行中工具绑定释放分区为止的实时运行时。
    pub(super) runtime: McpRuntimeHandle,
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

/// Retains cleanup observations independently of execution leases until successful terminal shutdown.
/// 独立于执行租约保留清理观测，直至成功完成终态关闭。
pub(super) type RetiredPluginPartitionRegistry =
    Arc<Mutex<BTreeMap<String, Vec<McpRuntimeHandle>>>>;

/// Shared registry for opt-in session-owned stdio MCP leases.
/// 供可选会话拥有 stdio MCP 租约使用的共享注册表。
type OwnedStdioPartitionRegistry =
    Arc<Mutex<BTreeMap<McpRuntimeOwnedPartitionKey, Arc<McpRuntimePartitionLease>>>>;

/// Shared plugin gate that rejects new owner-scoped MCP work while a plugin is draining.
/// 在插件排空期间拒绝新的所有者范围 MCP 工作的共享插件闸门。
pub(super) type BlockedOwnerPluginRegistry = Arc<Mutex<OwnerMcpAdmission>>;

/// Owns admission and original startup observations under the same synchronous lock.
/// 在同一同步锁内持有入场及原启动观察。
#[derive(Debug, Default)]
pub(super) struct OwnerMcpAdmission {
    /// Plugin identities whose owner admission remains closed.
    /// 所有者入场保持关闭的插件身份。
    blocked: BTreeSet<String>,
    /// Irreversible shutdown shared by old and current access generations.
    /// 新旧访问代次共享的不可逆关闭状态。
    pub(super) closed: bool,
    /// Every original instance, including simultaneous starts for one exact owner key.
    /// 每个原实例，包括同一精确所有者键的同时启动。
    instances: BTreeMap<McpRuntimeOwnedPartitionKey, Vec<OwnerMcpInstance>>,
    /// Global starts remain observable after readiness, including ready candidates not yet published by the service.
    /// 全局启动在就绪后仍可观察，包括服务尚未发布的就绪候选。
    pub(super) managed_globals: BTreeMap<String, Vec<McpRuntimeHandle>>,
    /// Actual initial and replacement user controls survive lost publication without retaining execution resources.
    /// 实际首次及替换用户控制跨发布丢失保留，不持有执行资源。
    user_instances: Vec<McpRuntimeHandle>,
}

/// Observes cleanup without retaining any execution lease or host resource bundle.
/// 观察清理，不保留任何执行租约或宿主资源包。
#[derive(Clone, Debug)]
struct OwnerMcpInstance {
    /// Control for requesting shutdown of this original instance.
    /// 用于请求关闭此原实例的控制句柄。
    runtime: McpRuntimeHandle,
    /// Evidence from the original aggregate resource scope.
    /// 原聚合资源范围产生的证据。
    released: vulcan_mcp_runtime::McpResourceRelease,
}

impl OwnerMcpAdmission {
    /// Returns whether this plugin is blocked by a drain or global shutdown.
    /// 返回此插件是否被排空或全局关闭封锁。
    pub(super) fn contains(&self, plugin_id: &str) -> bool {
        self.closed || self.blocked.contains(plugin_id)
    }

    /// Removes only instances whose original resource owners confirmed release.
    /// 仅移除原资源所有者已经确认释放的实例。
    pub(super) fn prune_released(&mut self) {
        self.instances.retain(|_, instances| {
            instances.retain(|instance| !instance.released.is_released());
            !instances.is_empty()
        });
        self.managed_globals.retain(|_, runtimes| {
            runtimes.retain(|runtime| !runtime.is_fully_stopped());
            !runtimes.is_empty()
        });
        self.user_instances
            .retain(|runtime| !runtime.is_fully_stopped());
    }
}

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
    pub(super) fn new(runtime: McpRuntimeHandle) -> Self {
        Self {
            instance_id: uuid::Uuid::new_v4(),
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
            instance_id: uuid::Uuid::new_v4(),
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
    pub(super) fn is_revoked(&self) -> bool {
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
    /// Retired and not-yet-ready global observations shared without extending execution leases.
    /// 不延长执行租约的共享观测，覆盖已退役及尚未就绪全局实例。
    pub(super) retired_plugin_partitions: RetiredPluginPartitionRegistry,
    /// Opt-in owner registry shared by every access generation derived from the same bootstrap access.
    /// 由同一引导访问面派生的所有访问代次共享的可选所有者注册表。
    owned_stdio_partitions: OwnedStdioPartitionRegistry,
    /// Shared lifecycle gate retained across published extension generations.
    /// 在已发布扩展代次之间持续共享的生命周期闸门。
    pub(super) blocked_owner_plugins: BlockedOwnerPluginRegistry,
    /// Plugin-lifecycle guards shared across every access generation and revoked on drain.
    /// 跨所有访问代次共享并在插件排空时撤销的插件生命周期闸门。
    owner_replacement_guards: OwnerReplacementGuardRegistry,
}

/// Original control lineage holds cleanup observations without extending any global or owner execution lease.
/// 原控制谱系持有清理观察，不延长任何全局或所有者执行租约。
pub(super) struct McpRuntimeObservationAccess {
    /// Original pending controls retain independent cleanup evidence until actual retirement.
    /// 原待处理控制保留独立清理证据，直至实际退役。
    pub(super) retired_plugin_partitions: RetiredPluginPartitionRegistry,
    /// Same original admission serializes publication against shutdown and plugin drain.
    /// 同一原入场将发布与关闭及插件排空串行化。
    pub(super) blocked_owner_plugins: BlockedOwnerPluginRegistry,
    /// Weak registry identity prevents historical controls from prolonging session-owned leases.
    /// 弱注册表身份防止历史控制延长会话拥有的租约。
    owned_stdio_partitions: std::sync::Weak<
        Mutex<BTreeMap<McpRuntimeOwnedPartitionKey, Arc<McpRuntimePartitionLease>>>,
    >,
    /// Weak original replacement identity participates in complete lineage comparison.
    /// 弱原替换身份参与完整谱系比较。
    owner_replacement_guards: std::sync::Weak<Mutex<BTreeMap<String, Arc<AtomicBool>>>>,
}

/// Explicit source intent fixes whether the existing plugin drain permits this sealed original preparation.
/// 明确来源用途固定既有插件排空是否允许此密封原准备。
enum PreparedPluginMcpAdmission {
    /// Original target handoff may prepare under plugin drain, while application closure still rejects it.
    /// 原目标交接可在插件排空下准备，但应用关闭仍拒绝它。
    TargetHandoff,
    /// Original old restoration may enter a locally drained plugin, but never global shutdown.
    /// 原旧恢复可以进入本地已排空插件，但绝不进入全局关闭。
    PredecessorRestoration,
    /// Ordinary published work requires both application and plugin admission to remain open.
    /// 普通已发布工作要求应用及插件入场均保持开放。
    OrdinaryPublished,
}

impl McpRuntimeObservationAccess {
    /// Compare every original registry against current without upgrading an execution-owning registry.
    /// 不提升拥有执行资源的注册表，对照 current 比较每个原注册表。
    /// Returns true only for this actual original lineage, including owner and replacement registries.
    /// 仅此实际原谱系返回真，包括所有者及替换注册表。
    pub(super) fn same_lineage(&self, current: &McpRuntimeAccess) -> bool {
        Arc::ptr_eq(
            &self.retired_plugin_partitions,
            &current.retired_plugin_partitions,
        ) && Arc::ptr_eq(&self.blocked_owner_plugins, &current.blocked_owner_plugins)
            && std::sync::Weak::ptr_eq(
                &self.owned_stdio_partitions,
                &Arc::downgrade(&current.owned_stdio_partitions),
            )
            && std::sync::Weak::ptr_eq(
                &self.owner_replacement_guards,
                &Arc::downgrade(&current.owner_replacement_guards),
            )
    }
}

impl McpRuntimeAccess {
    /// Retain only original observation and admission registries, without owning any execution partition.
    /// 仅保留原观察及入场注册表，不拥有任何执行分区。
    /// No inputs; returns a control-only lineage for historical preparation and publication controls.
    /// 无输入；返回供历史准备及发布控制使用的仅控制谱系。
    pub(super) fn observation_access(&self) -> McpRuntimeObservationAccess {
        McpRuntimeObservationAccess {
            retired_plugin_partitions: Arc::clone(&self.retired_plugin_partitions),
            blocked_owner_plugins: Arc::clone(&self.blocked_owner_plugins),
            owned_stdio_partitions: Arc::downgrade(&self.owned_stdio_partitions),
            owner_replacement_guards: Arc::downgrade(&self.owner_replacement_guards),
        }
    }

    /// Compare every original shared registry rather than accepting one equal route or admission field.
    /// 比较每个原共享注册表，不接受单个相同路由或入场字段。
    /// other is the candidate access; returns true only for this same actual bootstrap lineage.
    /// other 为候选访问；仅此同一实际引导谱系返回真。
    pub(super) fn same_lineage(&self, other: &Self) -> bool {
        Arc::ptr_eq(
            &self.retired_plugin_partitions,
            &other.retired_plugin_partitions,
        ) && Arc::ptr_eq(&self.owned_stdio_partitions, &other.owned_stdio_partitions)
            && Arc::ptr_eq(&self.blocked_owner_plugins, &other.blocked_owner_plugins)
            && Arc::ptr_eq(
                &self.owner_replacement_guards,
                &other.owner_replacement_guards,
            )
    }

    /// Check the exact actual lease selected by one immutable plugin partition.
    /// 检查单个不可变插件分区选定的精确实际租约。
    /// plugin_id and generation fix its key; expected is a non-owning lease identity, never a routing receipt.
    /// plugin_id 及 generation 固定其键；expected 为非拥有租约身份，绝非路由回执。
    /// Returns false for an absent or independently allocated lease, including equal configuration values.
    /// 租约缺失或独立分配时返回假，包括配置值相同的情况。
    pub(super) fn contains_exact_plugin_lease(
        &self,
        plugin_id: &str,
        generation: u64,
        expected: &std::sync::Weak<McpRuntimePartitionLease>,
    ) -> bool {
        self.live_partitions
            .get(&format!("plugin:{plugin_id}:{generation}"))
            .is_some_and(|actual| std::sync::Weak::ptr_eq(expected, &Arc::downgrade(actual)))
    }

    /// Builds one stable MCP runtime access surface from configured summary data and one optional live runtime handle.
    /// 根据配置摘要数据与一个可选实时运行时句柄构建单个稳定 MCP 运行时访问面。
    pub(super) fn new(
        configured_overview: McpRuntimeOverview,
        live_partitions: BTreeMap<String, McpRuntimeHandle>,
    ) -> Self {
        // Construct the original control inventory before sharing its mutex; registration cannot fail later.
        // 共享互斥锁之前构造原控制清单，避免之后发生登记失败。
        let mut admission = OwnerMcpAdmission::default();
        // The source-derived user partition has one original even when all its declarations are disabled.
        // 从来源派生的用户分区具有单个原实例，即使其全部声明已停用。
        if let Some(runtime) = live_partitions.get("user") {
            admission.user_instances.push(runtime.clone());
        }
        for runtime in live_partitions.values() {
            for plugin_id in runtime.plugin_source_ids() {
                // Route only from the actual immutable source and deduplicate by real runtime identity.
                // 仅按实际不可变来源归属，并按真实运行时身份去重。
                let originals = admission.managed_globals.entry(plugin_id).or_default();
                if !originals
                    .iter()
                    .any(|original| original.same_instance(runtime))
                {
                    originals.push(runtime.clone());
                }
            }
        }
        Self {
            configured_overview,
            live_partitions: live_partitions
                .into_iter()
                .map(|(key, runtime)| (key, Arc::new(McpRuntimePartitionLease::new(runtime))))
                .collect(),
            retired_plugin_partitions: Arc::new(Mutex::new(BTreeMap::new())),
            owned_stdio_partitions: Arc::new(Mutex::new(BTreeMap::new())),
            blocked_owner_plugins: Arc::new(Mutex::new(admission)),
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
    /// user_overview describes declared configuration; user_lease is the sole original registered before dispatch.
    /// user_overview 描述已声明配置；user_lease 为派发前登记的唯一原租约。
    /// Return the successor sharing the same shutdown and original observation lineage.
    /// 返回共享同一关闭及原观察谱系的后继。
    pub(super) fn with_replaced_user_partition(
        &self,
        user_overview: McpRuntimeOverview,
        user_lease: Arc<McpRuntimePartitionLease>,
    ) -> Self {
        // Transfer the already registered original lease; a second allocation would split execution identity.
        // 转交已经登记的原租约；再次分配会分裂执行身份。
        let mut live_partitions = self.live_partitions.clone();
        live_partitions.insert(String::from("user"), user_lease);
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

    /// Register one configured user original under shared admission before native dispatch.
    /// 在原生派发之前，在共享入场下登记单个已配置用户原实例。
    /// servers are the validated user declarations; return the sole registered lease or a pre-dispatch error.
    /// servers 为已校验用户声明；返回唯一已登记租约或派发前错误。
    pub(super) fn start_user_partition(
        &self,
        servers: Vec<McpServerConfig>,
    ) -> Result<Arc<McpRuntimePartitionLease>, String> {
        // Configured user work has no plugin resource bundle or sealed plugin source authority.
        // 已配置用户工作不带插件资源包或密封插件来源权威。
        let (resources, _) = vulcan_mcp_runtime::McpTransportResources::new(());
        // Pure allocation cannot start a producer before admission is checked.
        // 纯分配不能在检查入场之前启动生产者。
        let prepared = super::handle::prepare_managed_mcp_runtime(servers, resources)?;
        // Original shutdown and registration serialize on this existing shared lineage.
        // 原关闭及登记在此既有共享谱系上串行化。
        let mut admission = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("User MCP admission is poisoned."))?;
        if admission.closed {
            return Err(String::from("User MCP admission is globally closed."));
        }
        // Retain only the actual control in the inventory, never the execution lease or physical resources.
        // 清单仅保留实际控制，绝不保留执行租约或物理资源。
        let runtime = prepared.handle();
        // This exact execution identity is transferred through replacement without reallocation.
        // 此精确执行身份跨替换转交，不再次分配。
        let lease = Arc::new(McpRuntimePartitionLease::new(runtime.clone()));
        admission.prune_released();
        admission.user_instances.push(runtime);
        // Both admission and original control are established before any native lifecycle task exists.
        // 任何原生生命周期任务存在之前，入场及原控制均已建立。
        prepared.dispatch();
        Ok(lease)
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
        self.retain_removed_plugin_controls(plugin_id);
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
        self.retain_removed_plugin_controls(plugin_id);
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

    /// Replace only this target partition with its same prepared actual lease using the caller's current access.
    /// 使用调用方当前访问，仅将此目标分区替换为其相同已准备实际租约。
    /// Inputs fix exact plugin and generation; return a new access retaining every unrelated partition.
    /// 输入固定精确插件及代次；返回保留全部无关分区的新访问。
    pub(super) fn with_prepared_plugin_lease(
        &self,
        plugin_id: &str,
        generation: u64,
        lease: Arc<McpRuntimePartitionLease>,
    ) -> Self {
        self.retain_removed_plugin_controls(plugin_id);
        // Preserve original lease identity, including its closed gate, rather than wrapping a replacement.
        // 保留原租约身份及其关闭业务门，不包装替代租约。
        let prefix = format!("plugin:{plugin_id}:");
        // Derive from actual current partitions so unrelated concurrent user/plugin updates survive.
        // 从实际当前分区派生，使无关并发用户／插件更新保持存在。
        let mut live = self.live_partitions.clone();
        live.retain(|key, _| !key.starts_with(&prefix));
        live.insert(format!("{prefix}{generation}"), lease);
        // Overview comes from the same actual runtimes, without another frozen catalog.
        // 概览来自相同实际运行时，不创建另一冻结目录。
        let overview =
            merge_mcp_runtime_overviews(live.values().map(|lease| lease.runtime.overview()));
        Self::from_shared_partitions(
            overview,
            live,
            Arc::clone(&self.retired_plugin_partitions),
            Arc::clone(&self.owned_stdio_partitions),
            Arc::clone(&self.blocked_owner_plugins),
            Arc::clone(&self.owner_replacement_guards),
        )
    }

    /// Retain actual controls for partitions selected for removal before constructing another access generation.
    /// 构造另一访问代次之前，保留已选定待移除分区的实际控制句柄。
    /// Plugin_id selects the existing partition namespace; immutable runtime sources supply observation ownership.
    /// plugin_id 选择既有分区命名空间；不可变运行时来源提供观察归属。
    /// No execution lease is cloned or revoked; a poisoned registry fails under the existing infallible contract.
    /// 不克隆或撤销执行租约；注册表锁中毒按既有不可失败契约显式失败。
    fn retain_removed_plugin_controls(&self, plugin_id: &str) {
        // These constructors enter without admission or retirement locks; hold only observation storage here.
        // 这些构造器进入时不持有准入或退役锁；此处仅持有观察存储。
        let mut registry = self
            .retired_plugin_partitions
            .lock()
            .expect("retired MCP plugin partition registry poisoned");
        // Existing cleanup observers prune terminal success; registration itself takes no runtime-state locks.
        // 既有清理观察者清理成功终态；登记本身不取得运行时状态锁。
        // A publication caller may retain a business guard, so inspecting that gate here would risk lock reentry.
        // 发布调用方可能保留业务守卫，因此此处检查该门可能导致锁重入。
        // This is the same canonical namespace selector used by the actual removal below, not parsed provenance.
        // 这是下方实际移除使用的同一规范命名空间选择器，不是解析所得来源证明。
        let prefix = format!("plugin:{plugin_id}:");
        for (_, lease) in self
            .live_partitions
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
        {
            for source_plugin_id in lease.runtime.plugin_source_ids() {
                // Controls contain independent cleanup receipts and never own the native execution scope.
                // 控制包含独立清理回执，绝不拥有原生执行范围。
                let originals = registry.entry(source_plugin_id).or_default();
                if !originals
                    .iter()
                    .any(|original| original.same_instance(&lease.runtime))
                {
                    originals.push(lease.runtime.clone());
                }
            }
        }
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
            // A captured catalog must not advertise a partition whose admission is closed.
            // 已捕获的目录不得继续公开准入已关闭的分区。
            if lease.is_revoked() || lease.runtime.shutdown_requested() {
                continue;
            }
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

    /// Starts and registers one exact instance before yielding; returned resources cover publication.
    /// 在让出执行前启动并登记精确实例；返回的资源覆盖发布阶段。
    /// The key identifies the plugin generation and session; errors start no unobserved instance.
    /// 键标识插件代次及会话；错误不会启动未被观察的实例。
    fn start_owned_stdio_runtime(
        &self,
        key: &McpRuntimeOwnedPartitionKey,
        servers: Vec<McpServerConfig>,
    ) -> Result<(McpRuntimeHandle, vulcan_mcp_runtime::McpTransportResources), String> {
        // Keep one lock across admission, synchronous task launch and evidence registration.
        // 用同一锁覆盖入场、同步任务启动及证据登记。
        let mut admission = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
        if admission.contains(key.plugin_id()) {
            return Err(format!(
                "owner-scoped MCP access for plugin `{}` is draining",
                key.plugin_id()
            ));
        }
        admission.prune_released();
        // The returned owner covers the unpublished caller as well as real transport tasks.
        // 返回的所有者覆盖尚未发布的调用者及实际传输任务。
        let (resources, released) = vulcan_mcp_runtime::McpTransportResources::new(());
        // Pure preparation validates all servers; original ownership is registered before any lifecycle task can run.
        // 纯准备验证全部服务器；任何生命周期任务可运行之前登记原所有权。
        let prepared = super::handle::prepare_managed_mcp_runtime(servers, resources.clone())?;
        // The registered control is the same allocation dispatched and returned to the owner.
        // 已登记控制与派发并返回所有者的控制为相同分配。
        let runtime = prepared.handle();
        admission
            .instances
            .entry(key.clone())
            .or_default()
            .push(OwnerMcpInstance {
                runtime: runtime.clone(),
                released,
            });
        // Keep original admission locked through dispatch after its physical observation has been inserted.
        // 插入其物理观测之后，跨派发保持原准入锁定。
        prepared.dispatch();
        Ok((runtime, resources))
    }

    /// Waits for original resources of a blocked plugin within one total deadline.
    /// 在一个总期限内等待已封锁插件的原资源释放。
    /// Cancellation or timeout preserves admission and every original receipt for later observation.
    /// 取消或超时会保留入场状态及每份原回执，供后续观察。
    pub async fn wait_plugin_owner_drain(
        &self,
        plugin_id: &str,
        maximum_wait: Duration,
    ) -> Result<(), String> {
        if plugin_id.trim().is_empty() {
            return Err(String::from(
                "plugin owner MCP drain requires a non-empty plugin id",
            ));
        }
        // Capture receipts only while no new instance can enter this plugin.
        // 仅在此插件不能接纳新实例时捕获回执。
        let instances = {
            let admission = self
                .blocked_owner_plugins
                .lock()
                .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
            if !admission.contains(plugin_id) {
                return Err(format!(
                    "owner MCP admission for plugin `{plugin_id}` is not closed"
                ));
            }
            admission
                .instances
                .iter()
                .filter(|(key, _)| key.plugin_id() == plugin_id)
                .flat_map(|(key, instances)| {
                    instances
                        .iter()
                        .map(move |instance| (key.clone(), instance.released.clone()))
                })
                .collect::<Vec<_>>()
        };
        // All originals share this deadline; increasing owner count does not multiply it.
        // 全部原实例共享此期限；所有者数量增加不会成倍延长它。
        let deadline = tokio::time::Instant::now()
            .checked_add(maximum_wait)
            .ok_or_else(|| String::from("owner MCP drain deadline is out of range"))?;
        for (key, released) in instances {
            tokio::time::timeout_at(deadline, released.wait())
                .await
                .map_err(|_| {
                    format!(
                        "owner MCP resource drain timed out: plugin={}, generation={}, owner={}",
                        key.plugin_id(),
                        key.generation(),
                        key.owner_session_id()
                    )
                })?
                .map_err(|error| {
                    format!(
                        "owner MCP resource release failed for plugin `{}`, owner `{}`: {error}",
                        key.plugin_id(),
                        key.owner_session_id()
                    )
                })?;
        }
        // Another controller may have reopened the gate after the last receipt became ready.
        // 最后一份回执就绪后，另一控制者可能已经重新开放闸门。
        let mut admission = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
        if !admission.contains(plugin_id)
            || admission.instances.iter().any(|(key, instances)| {
                key.plugin_id() == plugin_id
                    && instances
                        .iter()
                        .any(|instance| !instance.released.is_released())
            })
        {
            return Err(format!(
                "owner MCP drain for plugin `{plugin_id}` changed while being observed"
            ));
        }
        admission.prune_released();
        Ok(())
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

        // Register this exact startup while admission is locked, before the first await.
        // 在入场锁内登记此精确启动，早于第一次等待。
        let (runtime, _startup_resources) = self.start_owned_stdio_runtime(&key, servers)?;
        // Cancellation must shut down the original instance even before publication.
        // 取消必须关闭原实例，即使尚未发布。
        let mut startup_guard = super::handle::McpRuntimeStartupGuard::new();
        startup_guard.track(&runtime);
        runtime
            .wait_until_ready(super::handle::MCP_PARTITION_READINESS_TIMEOUT)
            .await?;

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
            if blocked_plugins.contains(plugin_id) || new_lease.runtime.shutdown_requested() {
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
        if installed {
            startup_guard.mark_published();
        } else {
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
        // Register this exact startup while admission is locked, before the first await.
        // 在入场锁内登记此精确启动，早于第一次等待。
        let (runtime, _startup_resources) = self.start_owned_stdio_runtime(&key, servers)?;
        // Cancellation must shut down the original instance even before publication.
        // 取消必须关闭原实例，即使尚未发布。
        let mut startup_guard = super::handle::McpRuntimeStartupGuard::new();
        startup_guard.track(&runtime);
        runtime
            .wait_until_ready(super::handle::MCP_PARTITION_READINESS_TIMEOUT)
            .await?;
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
        if blocked_plugins.contains(plugin_id) || new_lease.runtime.shutdown_requested() {
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
        startup_guard.mark_published();
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

    /// Registers an unpublished global start in the shared original-instance observations.
    /// 将未发布全局启动登记到共享原实例观察中。
    /// The caller serializes global management operations; identity and runtime select one original.
    /// 调用方串行化全局管理操作；身份与运行时选择一个原实例。
    /// Returns an error without granting publication if the observation registry is unavailable.
    /// 观察注册表不可用时返回错误，不授予发布资格。
    pub(super) fn start_preparing_plugin_partition(
        &self,
        plugin_id: &str,
        servers: Vec<McpServerConfig>,
    ) -> Result<McpRuntimeHandle, String> {
        // Ordinary starts transfer an empty resource bundle through the exact shared admission path.
        // 普通启动通过精确共享入场路径移交空资源包。
        let (resources, _) = vulcan_mcp_runtime::McpTransportResources::new(());
        self.start_preparing_plugin_partition_with_resources(plugin_id, servers, resources, false)
    }

    /// Register one actual start with resources before waiting; target_preparation keeps business closed.
    /// 等待前登记带资源的单个实际启动；target_preparation 保持业务关闭。
    /// plugin_id and servers are exact inputs; return the original observed runtime or a pre-spawn error.
    /// plugin_id 及 servers 为精确输入；返回原已观察运行时或启动前错误。
    pub(super) fn start_preparing_plugin_partition_with_resources(
        &self,
        plugin_id: &str,
        servers: Vec<McpServerConfig>,
        resources: vulcan_mcp_runtime::McpTransportResources,
        target_preparation: bool,
    ) -> Result<McpRuntimeHandle, String> {
        // Application shutdown, construction and registration share the admission lock.
        // 应用关闭、构造及登记共享入场锁。
        let mut admission = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
        if admission.closed {
            return Err(String::from("MCP runtime access is globally closed"));
        }
        // Acquire observation storage before creating work so registration cannot fail afterwards.
        // 创建工作前取得观察存储，避免之后登记失败。
        let mut registry = self
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("retired MCP plugin partition registry poisoned"))?;
        // Ordinary managed starts carry an empty host bundle through the common constructor.
        // 普通受管启动通过公共构造器携带空宿主资源包。
        // Pure preparation retains exact input ownership without starting a task.
        // 纯准备保留精确输入所有权，不启动任务。
        let prepared = if target_preparation {
            super::handle::prepare_unproven_target_mcp_runtime(servers, resources)?
        } else {
            super::handle::prepare_managed_mcp_runtime(servers, resources)?
        };
        // Register the same original control before dispatch, without allocating a temporary execution lease.
        // 派发前登记相同原控制，不分配临时执行租约。
        let runtime = prepared.handle();
        // Controls do not retain actual execution resources after cancellation.
        // 取消后控制不会保留实际执行资源。
        let originals = registry.entry(plugin_id.to_owned()).or_default();
        originals.retain(|original| !original.is_fully_stopped());
        originals.push(runtime.clone());
        // This original survives successful readiness until its actual resources retire.
        // 此原实例跨成功就绪保留，直至实际资源退役。
        admission.prune_released();
        admission
            .managed_globals
            .entry(plugin_id.to_owned())
            .or_default()
            .push(runtime.clone());
        // Both admission and original inventory remain locked until the registered work has been dispatched.
        // 已登记工作完成派发之前，准入及原清单均保持锁定。
        prepared.dispatch();
        Ok(runtime)
    }

    /// Register an original proven ordinary control and its sole lease before any lifecycle task starts.
    /// 在任何生命周期任务启动前登记原已证明普通控制及其唯一租约。
    /// plugin_id, servers, resources and profile come from sealed materialization; return the same registered lease.
    /// plugin_id、servers、resources 及 profile 来自密封物化；返回同一已登记租约。
    /// Ordinary admission rejects both global closure and plugin drain; target preparation keeps its separate contract.
    /// 普通入场同时拒绝全局关闭及插件排空；目标准备保持其独立契约。
    pub(super) fn start_published_plugin_partition(
        &self,
        plugin_id: &str,
        servers: Vec<McpServerConfig>,
        resources: vulcan_mcp_runtime::McpTransportResources,
        profile: vulcan_plugin_runtime::VulcanPluginPublishedMcpProfile,
    ) -> Result<Arc<McpRuntimePartitionLease>, String> {
        // Allocate without effects, then register this original under the explicit ordinary admission policy.
        // 无副作用分配，然后按明确普通入场策略登记此原实例。
        let prepared = super::handle::prepare_published_mcp_runtime(servers, resources, profile)?;
        self.register_prepared_plugin_partition(
            plugin_id,
            prepared,
            PreparedPluginMcpAdmission::OrdinaryPublished,
        )
    }

    /// Allocate and register the original sealed target and sole actual lease before any task can exist.
    /// 在任何任务可能存在前，分配及登记原密封目标与唯一实际租约。
    /// plugin_id, servers, resources and profile come from sealed materialization; return the registered original lease.
    /// plugin_id、servers、resources 及 profile 来自密封物化；返回已登记原租约。
    pub(super) fn start_target_plugin_partition(
        &self,
        plugin_id: &str,
        servers: Vec<McpServerConfig>,
        resources: vulcan_mcp_runtime::McpTransportResources,
        profile: vulcan_plugin_runtime::VulcanPluginTargetMcpProfile,
    ) -> Result<Arc<McpRuntimePartitionLease>, String> {
        // Target source is fixed before dispatch and never receives ordinary published authority.
        // 目标来源在派发前固定，绝不获得普通已发布权威。
        let prepared = super::handle::prepare_target_mcp_runtime(servers, resources, profile)?;
        self.register_prepared_plugin_partition(
            plugin_id,
            prepared,
            PreparedPluginMcpAdmission::TargetHandoff,
        )
    }

    /// Register original old restoration before dispatch while preserving its local drain fence.
    /// 在派发前登记原旧恢复，同时保留其本地排空屏障。
    /// plugin_id, servers, resources and profile are sealed old inputs; return the sole actual lease.
    /// plugin_id、servers、resources 及 profile 为密封旧输入；返回唯一实际租约。
    pub(super) fn start_predecessor_plugin_partition(
        &self,
        plugin_id: &str,
        servers: Vec<McpServerConfig>,
        resources: vulcan_mcp_runtime::McpTransportResources,
        profile: vulcan_plugin_runtime::VulcanPluginPredecessorMcpProfile,
    ) -> Result<Arc<McpRuntimePartitionLease>, String> {
        // Reuse actual registration, observation ownership and dispatch without opening business.
        // 复用实际登记、观察所有权及派发，不打开业务。
        let prepared = super::handle::prepare_predecessor_mcp_runtime(servers, resources, profile)?;
        self.register_prepared_plugin_partition(
            plugin_id,
            prepared,
            PreparedPluginMcpAdmission::PredecessorRestoration,
        )
    }

    /// Register prepared under admission_policy before dispatch and preserve this actual original across lost returns.
    /// 按 admission_policy 在派发前登记 prepared，跨返回丢失保留此实际原实例。
    /// plugin_id selects the original observation slot; return its sole allocated lease or an error before dispatch.
    /// plugin_id 选择原观察槽；返回其唯一已分配租约或派发前错误。
    fn register_prepared_plugin_partition(
        &self,
        plugin_id: &str,
        prepared: super::handle::PreparedMcpRuntime,
        admission_policy: PreparedPluginMcpAdmission,
    ) -> Result<Arc<McpRuntimePartitionLease>, String> {
        // Local drain and original registration use the existing single admission lineage.
        // 本地排空及原登记使用既有单一入场谱系。
        let mut admission = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("Prepared MCP admission is poisoned."))?;
        if match admission_policy {
            PreparedPluginMcpAdmission::TargetHandoff
            | PreparedPluginMcpAdmission::PredecessorRestoration => admission.closed,
            PreparedPluginMcpAdmission::OrdinaryPublished => admission.contains(plugin_id),
        } {
            return Err(String::from("Prepared MCP plugin admission is closed."));
        }
        // Acquire the same original observation registry before allocating or dispatching work.
        // 分配或派发工作前取得同一原观察注册表。
        let mut registry = self
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("Prepared MCP observation registry is poisoned."))?;
        // Original control is retained in both existing tables before any producer can lose its return.
        // 任何生产者可能丢失返回之前，在两个既有表中保留原控制。
        let runtime = prepared.handle();
        // One lease is allocated before dispatch; subsequent publication must transfer this exact instance.
        // 派发前分配单个租约；后续发布必须转交此精确实例。
        let lease = Arc::new(McpRuntimePartitionLease::new(runtime.clone()));
        // Registration prunes only confirmed old release, never infers it from failed readiness.
        // 登记仅清理已确认旧释放，绝不从就绪失败推断释放。
        let originals = registry.entry(plugin_id.to_owned()).or_default();
        originals.retain(|original| !original.is_fully_stopped());
        originals.push(runtime.clone());
        admission.prune_released();
        admission
            .managed_globals
            .entry(plugin_id.to_owned())
            .or_default()
            .push(runtime);
        // Only now may native tasks exist; even a partial dispatch retains its original drain controls.
        // 仅此时原生任务才可存在；即使部分派发也保留其原排空控制。
        prepared.dispatch();
        Ok(lease)
    }

    /// Transfers a successful unpublished global start out of cleanup history under the same registry lock.
    /// 在同一注册表锁内将成功的未发布全局启动移出清理历史。
    /// Identity and runtime select the original; shutdown racing readiness must reject publication.
    /// 身份与运行时选择原实例；关闭与就绪竞争时必须拒绝发布。
    /// Returns success only while that original remains eligible for publication.
    /// 仅在此原实例仍可发布时返回成功。
    pub(super) fn finish_preparing_plugin_partition(
        &self,
        plugin_id: &str,
        runtime: &McpRuntimeHandle,
    ) -> Result<(), String> {
        // Drain and publication must observe the same shutdown decision while transferring ownership.
        // 排空与发布在交接所有权时必须观察相同关闭决策。
        let mut registry = self
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("retired MCP plugin partition registry poisoned"))?;
        if runtime.shutdown_requested() {
            return Err(format!(
                "Plugin `{plugin_id}` MCP startup was drained before publication"
            ));
        }
        // Successful transfer requires the original registration, never a guessed replacement.
        // 成功交接要求原登记存在，绝不猜测替代实例。
        let originals = registry
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Plugin `{plugin_id}` MCP startup observation is missing"))?;
        if !originals
            .iter()
            .any(|original| original.same_instance(runtime))
        {
            return Err(format!(
                "Plugin `{plugin_id}` original MCP startup observation is missing"
            ));
        }
        originals.retain(|original| !original.same_instance(runtime));
        if originals.is_empty() {
            registry.remove(plugin_id);
        }
        Ok(())
    }

    /// Closes one plugin's owner admission and stops its current and retired global partitions.
    /// 关闭一个插件的所有者入场，并停止它的当前及已退役全局分区。
    /// Receives an exact plugin identity; returns failure without erasing original release evidence.
    /// 接收精确插件身份；失败时不删除原始释放证据。
    ///
    /// Existing bindings retain their lease reference while the runtime observes the shutdown request;
    /// the revoked latch prevents queued calls from starting after this method returns.
    /// 已存在的绑定会在运行时处理关闭请求期间保留租约引用；撤销锁存位会阻止本方法返回后的排队调用启动。
    pub fn begin_plugin_mcp_drain(&self, plugin_id: &str) -> Result<(), String> {
        self.begin_plugin_owner_drain(plugin_id)?;
        // Current and already-retired global instances belong to the same explicit plugin.
        // 当前及已经退役的全局实例属于同一个明确插件。
        let prefix = format!("plugin:{plugin_id}:");
        // Keep ready but unpublished global candidates visible even from the old service access.
        // 即使从旧服务访问，也保持已就绪但尚未发布全局候选可见。
        let admission = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
        // The original receipt must outlive removal of every execution lease and catalog access.
        // 原回执必须比全部执行租约及目录访问的移除存活更久。
        let mut retired = self
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("retired MCP plugin partition registry poisoned"))?;
        // A prior grace timer may already have registered this exact original.
        // 先前宽限计时器可能已经登记此精确原实例。
        let originals = retired.entry(plugin_id.to_owned()).or_default();
        originals.retain(|runtime| !runtime.is_fully_stopped());
        for (key, lease) in &self.live_partitions {
            if key.starts_with(&prefix) {
                lease.revoke();
                if !originals
                    .iter()
                    .any(|runtime| runtime.same_instance(&lease.runtime))
                {
                    originals.push(lease.runtime.clone());
                }
            }
        }
        if let Some(managed) = admission.managed_globals.get(plugin_id) {
            for runtime in managed {
                if !originals
                    .iter()
                    .any(|original| original.same_instance(runtime))
                {
                    originals.push(runtime.clone());
                }
            }
        }
        for runtime in originals {
            runtime.shutdown();
        }
        Ok(())
    }

    /// Waits for owned and global originals of this plugin within one shared total budget.
    /// 在同一总预算内等待此插件的所有者及全局原实例。
    /// The caller closes admission first; timeout and cancellation retain all unconfirmed originals.
    /// 调用者先关闭入场；超时及取消保留全部未确认原实例。
    pub async fn wait_plugin_mcp_drain(
        &self,
        plugin_id: &str,
        maximum_wait: Duration,
    ) -> Result<(), String> {
        // One absolute deadline prevents the number of partitions from multiplying the budget.
        // 一个绝对期限防止分区数量成倍放大预算。
        let deadline = tokio::time::Instant::now()
            .checked_add(maximum_wait)
            .ok_or_else(|| String::from("plugin MCP drain deadline is out of range"))?;
        self.wait_plugin_owner_drain(
            plugin_id,
            deadline.saturating_duration_since(tokio::time::Instant::now()),
        )
        .await?;
        // Clone only control observations; real resources remain with their original owners.
        // 仅克隆控制观察；真实资源仍属于原所有者。
        let mut originals = self
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("retired MCP plugin partition registry poisoned"))?
            .get(plugin_id)
            .cloned()
            .unwrap_or_default();
        // Readiness does not discard ownership observations for an unpublished candidate.
        // 就绪不会丢弃未发布候选的所有权观察。
        {
            // Capture under the original admission lock without retaining it across waits.
            // 在原入场锁内捕获，不跨等待保留锁。
            let admission = self
                .blocked_owner_plugins
                .lock()
                .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
            if let Some(managed) = admission.managed_globals.get(plugin_id) {
                for runtime in managed {
                    if !originals
                        .iter()
                        .any(|original| original.same_instance(runtime))
                    {
                        originals.push(runtime.clone());
                    }
                }
            }
        }
        // The live view is authoritative even if a caller only closed the owner gate.
        // 即使调用者仅关闭所有者闸门，当前视图仍然是权威。
        let prefix = format!("plugin:{plugin_id}:");
        for (key, lease) in &self.live_partitions {
            if key.starts_with(&prefix)
                && !originals
                    .iter()
                    .any(|runtime| runtime.same_instance(&lease.runtime))
            {
                originals.push(lease.runtime.clone());
            }
        }
        for runtime in originals {
            if !runtime.shutdown_requested() {
                return Err(format!(
                    "MCP partition for plugin `{plugin_id}` has not entered drain"
                ));
            }
            runtime
                .wait_until_drained(deadline)
                .await
                .map_err(|error| format!("Plugin `{plugin_id}` MCP drain failed: {error}"))?;
        }
        // Recheck original observations after asynchronous waiting instead of clearing the whole namespace.
        // 异步等待后重新检查原观察，而不是清空整个命名空间。
        if self.has_usable_retired_plugin_partitions(plugin_id) {
            return Err(format!(
                "Plugin `{plugin_id}` MCP originals changed during drain"
            ));
        }
        // A later global original must not be missed after waiting on the captured set.
        // 等待捕获集合之后，不能遗漏更晚出现的全局原实例。
        {
            // Retain unresolved originals; successful observations can be pruned independently.
            // 保留未解决原实例；成功观察可以独立清理。
            let mut admission = self
                .blocked_owner_plugins
                .lock()
                .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
            if admission
                .managed_globals
                .get(plugin_id)
                .is_some_and(|runtimes| runtimes.iter().any(|runtime| !runtime.is_fully_stopped()))
            {
                return Err(format!(
                    "Plugin `{plugin_id}` managed MCP originals changed during drain"
                ));
            }
            admission.prune_released();
        }
        // Keep the owner gate and its original receipts valid after the global waits.
        // 全局等待完成后，继续确认所有者闸门及其原回执有效。
        self.wait_plugin_owner_drain(
            plugin_id,
            deadline.saturating_duration_since(tokio::time::Instant::now()),
        )
        .await
    }

    /// Closes owner admission and requests cleanup for every original session-owned instance.
    /// 关闭所有者入场，并为每个原会话所属实例请求清理。
    /// Repeated calls preserve original evidence and never reopen admission.
    /// 重复调用保留原证据，绝不重新开放入场。
    pub fn begin_plugin_owner_drain(&self, plugin_id: &str) -> Result<(), String> {
        if plugin_id.trim().is_empty() {
            return Err(String::from(
                "plugin owner MCP drain requires a non-empty plugin id",
            ));
        }
        // Serialize shutdown with registration and publication, including repeated drain attempts.
        // 将关闭与登记和发布串行化，包括重复排空尝试。
        let mut admission = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
        admission.blocked.insert(plugin_id.to_owned());
        // Request every original instance before touching registries that can independently fail.
        // 在访问可能独立失败的注册表之前，请求关闭每个原实例。
        for (key, instances) in &admission.instances {
            if key.plugin_id() == plugin_id {
                for instance in instances {
                    instance.runtime.shutdown();
                }
            }
        }
        // Invalidate replacement-following capabilities without reopening them on retry.
        // 撤销跟随替换的能力，重试时不重新开放。
        let mut guards = self
            .owner_replacement_guards
            .lock()
            .map_err(|_| String::from("owner MCP replacement guard registry poisoned"))?;
        if let Some(guard) = guards.remove(plugin_id) {
            guard.store(false, Ordering::Release);
        }
        // Published leases lose admission immediately; original receipts stay in admission.
        // 已发布租约立即失去入场资格；原回执继续保留在入场状态中。
        let mut owners = self
            .owned_stdio_partitions
            .lock()
            .map_err(|_| String::from("owned MCP stdio partition registry poisoned"))?;
        owners.retain(|key, lease| {
            if key.plugin_id() != plugin_id {
                return true;
            }
            lease.revoke();
            lease.runtime.shutdown();
            false
        });
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
        // Check evidence under the same lock that prevents a late start from overtaking drain.
        // 在防止迟到启动越过排空的同一锁内检查证据。
        let mut admission = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("blocked MCP owner plugin registry poisoned"))?;
        if admission.closed {
            return Err(String::from("owner MCP runtime has been shut down"));
        }
        if !admission.blocked.contains(plugin_id) {
            return Ok(());
        }
        admission.prune_released();
        // Include exact owner identity without exposing command arguments or credentials.
        // 包含精确所有者身份，不暴露命令参数或凭据。
        let pending = admission
            .instances
            .keys()
            .filter(|key| key.plugin_id() == plugin_id)
            .map(|key| format!("{}:{}", key.generation(), key.owner_session_id()))
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            return Err(format!(
                "owner MCP resources for plugin `{plugin_id}` are still draining: {}",
                pending.join(", ")
            ));
        }
        admission.blocked.remove(plugin_id);
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
        // Latch closure before touching any lease so old access cannot create new owner work.
        // 接触任何租约之前锁存关闭，防止旧访问创建新的所有者工作。
        let mut admission = self
            .blocked_owner_plugins
            .lock()
            .expect("blocked MCP owner plugin registry poisoned");
        admission.closed = true;
        for instances in admission.instances.values() {
            for instance in instances {
                instance.runtime.shutdown();
            }
        }
        // Ready candidates can still be absent from the service's published access.
        // 已就绪候选仍可能不在服务发布访问中。
        for runtime in admission.managed_globals.values().flatten() {
            runtime.shutdown();
        }
        // User originals may have been replaced or published through a later access while this guard stays armed.
        // 此守卫保持生效期间，用户原实例可能已被替换或经后续访问发布。
        for runtime in &admission.user_instances {
            runtime.shutdown();
        }
        // Global startup originals may not yet belong to this published access.
        // 原全局启动可能尚未属于此发布访问。
        let originals = self
            .retired_plugin_partitions
            .lock()
            .expect("retired MCP plugin partition registry poisoned");
        for runtime in originals.values().flatten() {
            runtime.shutdown();
        }
        drop(originals);
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

    /// Wait for physical retirement of every original user runtime in this shared shutdown lineage.
    /// 等待此共享关闭谱系内每个原用户运行时物理退役。
    /// maximum_wait bounds the whole captured set; no admission lock survives an await.
    /// maximum_wait 限制整个捕获集合；入场锁不跨 await 存续。
    /// Return success only after original cleanup and physical release, rejecting unrequested or later live work.
    /// 仅在原清理及物理释放后返回成功，拒绝未请求关闭或更晚的存活工作。
    pub async fn wait_user_mcp_drain(&self, maximum_wait: Duration) -> Result<(), String> {
        // One checked absolute deadline prevents partition count from multiplying the budget.
        // 单个经检查的绝对期限防止分区数量成倍放大预算。
        let deadline = tokio::time::Instant::now()
            .checked_add(maximum_wait)
            .ok_or_else(|| String::from("User MCP drain deadline is out of range."))?;
        // Clone non-owning actual controls while registration and shutdown cannot change the inventory.
        // 在登记及关闭无法改变清单期间，克隆非拥有实际控制。
        let originals = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("User MCP admission is poisoned."))?
            .user_instances
            .clone();
        // Every actual original shares the same total deadline and requires an explicit prior shutdown request.
        // 每个实际原实例共享同一总期限，并要求此前明确请求关闭。
        for runtime in originals {
            if !runtime.shutdown_requested() {
                return Err(String::from(
                    "An original user MCP runtime has not entered drain.",
                ));
            }
            runtime.wait_until_drained(deadline).await?;
        }
        // Recheck the actual original inventory and preserve every unresolved control across failures.
        // 重新检查实际原清单，并跨失败保留每个未解决控制。
        let mut admission = self
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("User MCP admission is poisoned."))?;
        if admission
            .user_instances
            .iter()
            .any(|runtime| !runtime.is_fully_stopped())
        {
            return Err(String::from("User MCP originals changed during drain."));
        }
        admission.prune_released();
        Ok(())
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
        // Select the exact source identity once for both cleanup observation and deadline control.
        // 为清理观测及截止控制一次性选择精确来源身份。
        let partition_prefix = format!("plugin:{plugin_id}:");
        let retired_partitions = self
            .live_partitions
            .iter()
            .filter(|(key, _)| key.starts_with(&partition_prefix))
            .map(|(_, lease)| lease)
            .collect::<Vec<_>>();
        if retired_partitions.is_empty() {
            return;
        }
        // Keep status alive without keeping the execution lease alive: its Drop still requests shutdown.
        // 保留状态而不保留执行租约：租约析构仍会请求关闭。
        {
            // Prune successful history on every retirement so repeated updates cannot accumulate it.
            // 每次退役都清理已成功的历史，避免重复更新使历史不断积累。
            let mut registry = self
                .retired_plugin_partitions
                .lock()
                .expect("retired MCP plugin partition registry poisoned");
            let runtimes = registry.entry(plugin_id.to_string()).or_default();
            runtimes.retain(|runtime| !runtime.is_fully_stopped());
            for lease in &retired_partitions {
                if !runtimes
                    .iter()
                    .any(|runtime| runtime.same_instance(&lease.runtime))
                {
                    runtimes.push(lease.runtime.clone());
                }
            }
        }
        // The grace timer must not extend a caller's right to execute old-generation tools.
        // 宽限计时器不得延长调用方执行旧代次工具的权利。
        let retired_leases = retired_partitions
            .into_iter()
            .map(Arc::downgrade)
            .collect::<Vec<_>>();
        tokio::spawn(async move {
            tokio::time::sleep(maximum_drain).await;
            for weak_lease in retired_leases {
                if let Some(lease) = weak_lease.upgrade() {
                    lease.runtime.shutdown();
                }
            }
        });
    }

    /// Returns whether a retired plugin partition lacks successful terminal cleanup confirmation.
    /// 返回已退役插件分区是否仍缺少成功完成终态清理的确认。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the exact package whose removal safety is being checked.
    /// `plugin_id` 选择正在检查删除安全性的精确包。
    ///
    /// # Returns
    /// 返回值
    /// Returns true while any recorded runtime has pending or failed cleanup, even after its last caller leaves.
    /// 任一登记运行时清理未完成或失败时返回 true，即使最后调用方已经退出。
    pub fn has_usable_retired_plugin_partitions(&self, plugin_id: &str) -> bool {
        // Release an observation only after the same runtime confirms successful terminal cleanup.
        // 仅当同一运行时确认成功完成终态清理后释放其观测。
        let mut registry = self
            .retired_plugin_partitions
            .lock()
            .expect("retired MCP plugin partition registry poisoned");
        let has_unconfirmed_cleanup = registry.get_mut(plugin_id).is_some_and(|runtimes| {
            runtimes.retain(|runtime| !runtime.is_fully_stopped());
            !runtimes.is_empty()
        });
        if !has_unconfirmed_cleanup {
            registry.remove(plugin_id);
        }
        has_unconfirmed_cleanup
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
    pub(super) fn new(
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
        if self.runtime.is_revoked() || self.runtime.runtime.shutdown_requested() {
            return Err(String::from(
                "mcp_partition_draining: this MCP binding has been revoked",
            ));
        }
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
