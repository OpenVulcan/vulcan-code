//! Long-lived MCP runtime manager that keeps server connections hot for tool registration.
//! 用于保持服务器长连接并支撑工具注册的长期 MCP 运行时管理器。

use crate::mcp::config::McpServerConfig;
use crate::mcp::registry::{McpServerRegistry, configured_mcp_registry_overview};
use crate::provider_config::ProviderConfigurationFile;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::watch;
use vulcan_mcp_runtime::{McpTransport, McpTransportEvent};

/// Stable MCP tool schema exposed to the host runtime and execution planner.
/// 暴露给宿主运行时与执行规划器的稳定 MCP 工具 Schema。
#[derive(Debug, Clone, PartialEq, Eq)]
struct McpRuntimeToolSchema {
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
struct McpRuntimeToolDescriptor {
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
    fn new(
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

    /// Returns one host-facing read-only tool view derived from the runtime descriptor.
    /// 返回一份从运行时描述派生出的宿主侧只读工具视图。
    fn host_tool_view(&self) -> McpRuntimeToolView {
        McpRuntimeToolView {
            id: self.qualified_name(),
            name: self.tool_name().to_string(),
            description: self.tool_description().to_string(),
            args_schema: self.args_schema().clone(),
        }
    }

    /// Returns the stable MCP tool name.
    /// 返回稳定 MCP 工具名称。
    fn tool_name(&self) -> &str {
        &self.schema.name
    }

    /// Returns the human-readable MCP tool description.
    /// 返回人类可读的 MCP 工具描述。
    fn tool_description(&self) -> &str {
        &self.schema.description
    }

    /// Returns the JSON Schema describing the tool arguments.
    /// 返回描述工具参数的 JSON Schema。
    fn args_schema(&self) -> &serde_json::Value {
        &self.schema.args_schema
    }

    /// Returns the host-facing qualified tool name composed from server and tool identifiers.
    /// 返回由服务器与工具标识组合而成的宿主侧限定工具名。
    fn qualified_name(&self) -> String {
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
struct McpRuntimeToolView {
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
}

impl McpRuntimeToolView {
    /// Returns the stable host-facing tool identifier.
    /// 返回稳定的宿主侧工具标识。
    fn id(&self) -> &str {
        &self.id
    }

    /// Returns the human-readable tool name.
    /// 返回人类可读工具名称。
    fn name(&self) -> &str {
        &self.name
    }

    /// Returns the human-readable tool description.
    /// 返回人类可读工具描述。
    fn description(&self) -> &str {
        &self.description
    }

    /// Returns the JSON Schema describing the tool arguments.
    /// 返回描述工具参数的 JSON Schema。
    fn args_schema(&self) -> &serde_json::Value {
        &self.args_schema
    }
}

/// Stable MCP runtime tool binding that couples one host-facing tool view with its live invocation capability.
/// 将宿主侧工具视图与实时调用能力耦合在一起的稳定 MCP 运行时工具绑定。
#[derive(Debug, Clone)]
pub(crate) struct McpRuntimeToolBinding {
    /// Host-facing read-only tool view.
    /// 宿主侧只读工具视图。
    view: McpRuntimeToolView,
    /// Live runtime handle used to resolve the owning transport at invocation time.
    /// 在调用时用于解析所属传输的实时运行时句柄。
    runtime: McpRuntimeHandle,
}

/// Stable MCP runtime access surface that resolves live-versus-configured reads inside the MCP domain.
/// 在 MCP 域内部解析实时与配置读取来源的稳定 MCP 运行时访问面。
#[derive(Debug, Clone)]
pub(crate) struct McpRuntimeAccess {
    /// Lightweight configured MCP overview retained for fallback reads.
    /// 为回退读取保留的轻量 MCP 配置概览。
    configured_overview: McpRuntimeOverview,
    /// Optional live MCP runtime handle used when real runtime state is available.
    /// 在真实运行时状态可用时使用的可选实时 MCP 运行时句柄。
    live_runtime: Option<McpRuntimeHandle>,
}

impl McpRuntimeAccess {
    /// Builds one stable MCP runtime access surface from configured summary data and one optional live runtime handle.
    /// 根据配置摘要数据与一个可选实时运行时句柄构建单个稳定 MCP 运行时访问面。
    fn new(
        configured_overview: McpRuntimeOverview,
        live_runtime: Option<McpRuntimeHandle>,
    ) -> Self {
        Self {
            configured_overview,
            live_runtime,
        }
    }

    /// Returns one stable MCP runtime overview backed by live runtime state when available, or configured fallback data otherwise.
    /// 返回一份在实时运行时可用时优先使用实时状态、否则回退到配置摘要数据的稳定 MCP 运行时概览。
    pub(crate) fn overview(&self) -> McpRuntimeOverview {
        self.live_runtime
            .as_ref()
            .map(McpRuntimeHandle::overview)
            .unwrap_or_else(|| self.configured_overview.clone())
    }

    /// Returns currently available MCP tool bindings filtered by one allowlist when live runtime state is available.
    /// 当实时运行时状态可用时，返回按单份白名单过滤后的当前可用 MCP 工具绑定集合。
    pub(crate) fn available_tool_bindings_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Vec<McpRuntimeToolBinding> {
        self.live_runtime
            .as_ref()
            .map(|runtime| runtime.available_tool_bindings_filtered(allowlist))
            .unwrap_or_default()
    }

    /// Requests the live MCP runtime to stop when one real runtime handle is currently available.
    /// 当当前存在真实运行时句柄时，请求实时 MCP 运行时停止。
    pub(crate) fn shutdown(&self) {
        if let Some(runtime) = self.live_runtime.as_ref() {
            runtime.shutdown();
        }
    }
}

/// Builds one stable MCP runtime access surface from provider configuration without a live runtime handle.
/// 根据供应商配置构建一个不包含实时运行时句柄的稳定 MCP 运行时访问面。
pub(crate) fn configured_mcp_runtime_access(
    config: &ProviderConfigurationFile,
) -> McpRuntimeAccess {
    McpRuntimeAccess::new(configured_mcp_registry_overview(config), None)
}

impl McpRuntimeToolBinding {
    /// Builds one MCP runtime tool binding from one host-facing tool view and one live runtime handle.
    /// 根据一份宿主侧工具视图与一份实时运行时句柄构建单个 MCP 运行时工具绑定。
    fn new(view: McpRuntimeToolView, runtime: McpRuntimeHandle) -> Self {
        Self { view, runtime }
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

    /// Invokes the bound MCP tool through its stable host-facing tool identifier.
    /// 通过稳定宿主侧工具标识调用当前绑定的 MCP 工具。
    pub async fn invoke(&self, args: serde_json::Value) -> Result<serde_json::Value, String> {
        self.runtime.call_tool_by_id(self.id(), args).await
    }
}

/// Stable MCP tool allowlist consumed by runtime filtering surfaces.
/// 供运行时筛选表面消费的稳定 MCP 工具白名单。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct McpRuntimeToolAllowlist {
    /// Optional server identifier allowlist.
    /// 可选的服务器标识白名单。
    allowed_server_ids: Option<BTreeSet<String>>,
    /// Optional tool-name or qualified-name allowlist.
    /// 可选的工具名或限定工具名白名单。
    allowed_tool_names: Option<BTreeSet<String>>,
}

impl McpRuntimeToolAllowlist {
    /// Builds one MCP runtime tool allowlist from optional server and tool allowlists.
    /// 根据可选服务器与工具白名单构建单个 MCP 运行时工具白名单。
    pub fn new(
        allowed_server_ids: Option<BTreeSet<String>>,
        allowed_tool_names: Option<BTreeSet<String>>,
    ) -> Self {
        Self {
            allowed_server_ids,
            allowed_tool_names,
        }
    }

    /// Returns the optional server identifier allowlist.
    /// 返回可选服务器标识白名单。
    pub fn allowed_server_ids(&self) -> Option<&BTreeSet<String>> {
        self.allowed_server_ids.as_ref()
    }

    /// Returns the optional tool-name or qualified-name allowlist.
    /// 返回可选工具名或限定工具名白名单。
    pub fn allowed_tool_names(&self) -> Option<&BTreeSet<String>> {
        self.allowed_tool_names.as_ref()
    }
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
struct McpRuntimeServerSnapshot {
    /// Human-readable server name.
    /// 人类可读服务器名称。
    name: String,
    /// Whether the server is enabled in configuration.
    /// 该服务器在配置中是否启用。
    enabled: bool,
    /// Transport-family label used for diagnostics.
    /// 用于诊断的传输类型标签。
    transport_label: String,
    /// Compact endpoint or command summary shown in the workbench.
    /// 展示在工作台中的紧凑端点或命令摘要。
    endpoint_summary: String,
    /// Current runtime connectivity status.
    /// 当前运行时连通状态。
    status: McpRuntimeServerStatus,
    /// Latest connection or refresh warning, if any.
    /// 最近一次连接或刷新警告（如果存在）。
    last_error: Option<String>,
    /// Currently registered tool descriptors for this server.
    /// 当前为该服务器注册的工具描述集合。
    tools: Vec<McpRuntimeToolDescriptor>,
}

/// Lightweight runtime-facing diagnostic view for one enabled MCP server.
/// 面向运行时的单个已启用 MCP 服务器轻量诊断视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRuntimeServerDiagnostic {
    /// Human-readable server name.
    /// 人类可读服务器名称。
    name: String,
    /// Current runtime connectivity status.
    /// 当前运行时连通状态。
    status: McpRuntimeServerStatus,
    /// Number of currently registered tools.
    /// 当前已注册工具数量。
    tool_count: usize,
    /// Latest connection or refresh warning, if any.
    /// 最近一次连接或刷新警告（如果存在）。
    last_error: Option<String>,
}

impl McpRuntimeServerDiagnostic {
    /// Returns the human-readable server name.
    /// 返回人类可读服务器名称。
    pub fn server_name(&self) -> &str {
        &self.name
    }

    /// Returns the current runtime connectivity status.
    /// 返回当前运行时连通状态。
    pub fn status(&self) -> McpRuntimeServerStatus {
        self.status
    }

    /// Returns the number of currently registered tools.
    /// 返回当前已注册工具数量。
    pub fn tool_count(&self) -> usize {
        self.tool_count
    }

    /// Returns one human-readable runtime summary sentence.
    /// 返回单条人类可读的运行时摘要语句。
    pub fn summary_text(&self) -> String {
        format!(
            "服务器 `{}` 当前状态为 `{}`，已注册 {} 个工具。",
            self.server_name(),
            self.status().label(),
            self.tool_count()
        )
    }

    /// Returns the latest warning sentence when the server has a refresh or connection error.
    /// 当服务器存在刷新或连接错误时返回最近一次警告语句。
    pub fn warning_text(&self) -> Option<String> {
        self.last_error
            .as_ref()
            .map(|error| format!("{} 最近一次错误：{error}", self.summary_text()))
    }

    /// Returns the recommended runtime event title for UI projection.
    /// 返回供界面投影使用的建议运行时事件标题。
    pub fn runtime_event_title(&self) -> String {
        if self.last_error.is_some() {
            format!("MCP 服务器离线 · {}", self.server_name())
        } else {
            format!("MCP 服务器在线 · {}", self.server_name())
        }
    }
}

/// Lightweight MCP runtime event severity used by upper-layer projection adapters.
/// 供上层投影适配器使用的轻量 MCP 运行时事件严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpRuntimeEventProjectionKind {
    /// Event represents a healthy informational server state.
    /// 事件表示健康的提示性服务器状态。
    Informational,
    /// Event represents a warning or failure server state.
    /// 事件表示告警或失败服务器状态。
    Warning,
}

/// Stable MCP runtime event projection exported to upper runtime layers.
/// 导出给上层运行时的稳定 MCP 运行时事件投影视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRuntimeEventProjection {
    /// Recommended event kind derived from the current server diagnostic state.
    /// 根据当前服务器诊断状态推导得到的建议事件种类。
    kind: McpRuntimeEventProjectionKind,
    /// Recommended event title for UI projection.
    /// 供界面投影使用的建议事件标题。
    title: String,
    /// Recommended event body for UI projection.
    /// 供界面投影使用的建议事件正文。
    body: String,
}

impl McpRuntimeEventProjection {
    /// Returns the recommended event title for UI projection.
    /// 返回供界面投影使用的建议事件标题。
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the recommended event body for UI projection.
    /// 返回供界面投影使用的建议事件正文。
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns whether the projected event should be treated as a warning/failure path.
    /// 返回该投影视图是否应被视为告警或失败路径。
    pub fn is_warning(&self) -> bool {
        self.kind == McpRuntimeEventProjectionKind::Warning
    }
}

/// Stable MCP runtime discovery report exported to upper runtime bootstrap flows.
/// 导出给上层运行时 bootstrap 流程的稳定 MCP 运行时发现报告。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct McpRuntimeDiscoveryReport {
    /// Recommended UI-facing runtime event projections.
    /// 建议用于界面展示的运行时事件投影视图集合。
    event_projections: Vec<McpRuntimeEventProjection>,
    /// Human-readable discovery summary rows.
    /// 人类可读的发现摘要行集合。
    discovery_rows: Vec<String>,
    /// Human-readable discovery error rows.
    /// 人类可读的发现错误行集合。
    discovery_errors: Vec<String>,
    /// Human-readable discovery warning rows.
    /// 人类可读的发现告警行集合。
    discovery_warnings: Vec<String>,
}

impl McpRuntimeDiscoveryReport {
    /// Returns whether the discovery report contains no event projections, summary rows, errors, or warnings.
    /// 返回该发现报告是否不包含任何事件投影、摘要行、错误或告警。
    pub fn is_empty(&self) -> bool {
        self.event_projections.is_empty()
            && self.discovery_rows.is_empty()
            && self.discovery_errors.is_empty()
            && self.discovery_warnings.is_empty()
    }

    /// Returns the recommended runtime event projections.
    /// 返回建议的运行时事件投影视图集合。
    pub fn event_projections(&self) -> &[McpRuntimeEventProjection] {
        &self.event_projections
    }

    /// Returns the human-readable discovery summary rows.
    /// 返回人类可读的发现摘要行集合。
    pub fn discovery_rows(&self) -> &[String] {
        &self.discovery_rows
    }

    /// Returns the human-readable discovery error rows.
    /// 返回人类可读的发现错误行集合。
    pub fn discovery_errors(&self) -> &[String] {
        &self.discovery_errors
    }

    /// Returns the human-readable discovery warning rows.
    /// 返回人类可读的发现告警行集合。
    pub fn discovery_warnings(&self) -> &[String] {
        &self.discovery_warnings
    }
}

/// Stable MCP runtime overview exported to upper runtime layers.
/// 导出给上层运行时的稳定 MCP 运行时概览。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct McpRuntimeOverview {
    /// Human-readable summary rows covering all configured runtime-visible servers.
    /// 覆盖全部已配置运行时可见服务器的人类可读摘要行集合。
    summary_rows: Vec<String>,
    /// Human-readable summary rows covering only active runtime-visible servers.
    /// 仅覆盖活动运行时可见服务器的人类可读摘要行集合。
    active_summary_rows: Vec<String>,
    /// Stable discovery report exported to upper bootstrap and diagnostics flows.
    /// 导出给上层 bootstrap 与诊断流程的稳定发现报告。
    discovery_report: McpRuntimeDiscoveryReport,
}

impl McpRuntimeOverview {
    /// Builds one stable MCP runtime overview from summary rows and one discovery report.
    /// 根据摘要行与单份发现报告构建单个稳定 MCP 运行时概览。
    pub fn new(
        summary_rows: Vec<String>,
        active_summary_rows: Vec<String>,
        discovery_report: McpRuntimeDiscoveryReport,
    ) -> Self {
        Self {
            summary_rows,
            active_summary_rows,
            discovery_report,
        }
    }

    /// Returns the human-readable summary rows covering all configured runtime-visible servers.
    /// 返回覆盖全部已配置运行时可见服务器的人类可读摘要行集合。
    pub fn summary_rows(&self) -> &[String] {
        &self.summary_rows
    }

    /// Returns the human-readable summary rows covering only active runtime-visible servers.
    /// 返回仅覆盖活动运行时可见服务器的人类可读摘要行集合。
    pub fn active_summary_rows(&self) -> &[String] {
        &self.active_summary_rows
    }

    /// Returns the stable discovery report exported to upper bootstrap and diagnostics flows.
    /// 返回导出给上层 bootstrap 与诊断流程的稳定发现报告。
    pub fn discovery_report(&self) -> &McpRuntimeDiscoveryReport {
        &self.discovery_report
    }
}

impl McpRuntimeServerDiagnostic {
    /// Converts the current server diagnostic into one stable runtime event projection.
    /// 将当前服务器诊断转换为单条稳定的运行时事件投影视图。
    fn to_runtime_event_projection(&self) -> McpRuntimeEventProjection {
        let body = self.warning_text().unwrap_or_else(|| self.summary_text());
        let kind = if self.last_error.is_some() {
            McpRuntimeEventProjectionKind::Warning
        } else {
            McpRuntimeEventProjectionKind::Informational
        };
        McpRuntimeEventProjection {
            kind,
            title: self.runtime_event_title(),
            body,
        }
    }
}

/// Aggregated MCP runtime snapshot consumed by UI summaries and execution planning.
/// 供界面摘要与执行规划消费的聚合 MCP 运行时快照。
#[derive(Debug, Clone, Default)]
struct McpRuntimeSnapshot {
    /// Server snapshots keyed by stable server identifier.
    /// 以稳定服务器标识为键保存的服务器快照集合。
    servers: BTreeMap<String, McpRuntimeServerSnapshot>,
}

impl McpRuntimeSnapshot {
    /// Returns lightweight server summary rows for UI surfaces.
    /// 返回供界面表面使用的轻量服务器摘要行。
    fn summary_rows(&self) -> Vec<String> {
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

    /// Returns lightweight summary rows for runtime-visible servers that are actively connecting or already connected.
    /// 返回处于连接中或已连接状态的运行时可见服务器轻量摘要行集合。
    fn active_summary_rows(&self) -> Vec<String> {
        self.servers
            .values()
            .filter(|server| {
                matches!(
                    server.status,
                    McpRuntimeServerStatus::Connecting | McpRuntimeServerStatus::Connected
                )
            })
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
    fn tool_descriptors(&self) -> Vec<McpRuntimeToolDescriptor> {
        self.servers
            .values()
            .filter(|server| server.status == McpRuntimeServerStatus::Connected)
            .flat_map(|server| server.tools.clone())
            .collect()
    }

    /// Returns host-facing tool views filtered by one allowlist.
    /// 返回按单份白名单过滤后的宿主侧工具视图集合。
    fn filtered_tool_views(&self, allowlist: &McpRuntimeToolAllowlist) -> Vec<McpRuntimeToolView> {
        self.tool_descriptors()
            .into_iter()
            .filter(|tool| {
                tool.matches_allowed_sets(
                    allowlist.allowed_server_ids(),
                    allowlist.allowed_tool_names(),
                )
            })
            .map(|tool| tool.host_tool_view())
            .collect()
    }

    /// Returns lightweight diagnostics for enabled MCP servers only.
    /// 仅返回已启用 MCP 服务器的轻量诊断信息。
    fn enabled_server_diagnostics(&self) -> Vec<McpRuntimeServerDiagnostic> {
        self.servers
            .values()
            .filter(|server| server.enabled)
            .map(|server| McpRuntimeServerDiagnostic {
                name: server.name.clone(),
                status: server.status,
                tool_count: server.tools.len(),
                last_error: server.last_error.clone(),
            })
            .collect()
    }

    /// Builds one stable discovery report for enabled MCP servers only.
    /// 为已启用 MCP 服务器构建一份稳定发现报告。
    fn enabled_server_discovery_report(&self) -> McpRuntimeDiscoveryReport {
        let diagnostics = self.enabled_server_diagnostics();
        let mut event_projections = Vec::with_capacity(diagnostics.len());
        let mut discovery_rows = Vec::with_capacity(diagnostics.len());
        let mut discovery_errors = Vec::new();
        let mut discovery_warnings = Vec::new();

        for diagnostic in diagnostics {
            let summary = diagnostic.summary_text();
            discovery_rows.push(summary);
            if let Some(warning) = diagnostic.warning_text() {
                discovery_errors.push(warning.clone());
                discovery_warnings.push(warning);
            }
            event_projections.push(diagnostic.to_runtime_event_projection());
        }

        McpRuntimeDiscoveryReport {
            event_projections,
            discovery_rows,
            discovery_errors,
            discovery_warnings,
        }
    }

    /// Builds one stable runtime overview that groups summary rows and the enabled-server discovery report.
    /// 构建一份聚合摘要行与已启用服务器发现报告的稳定运行时概览。
    fn overview(&self) -> McpRuntimeOverview {
        McpRuntimeOverview::new(
            self.summary_rows(),
            self.active_summary_rows(),
            self.enabled_server_discovery_report(),
        )
    }
}

/// Shared host-side handle used to inspect active MCP tools and resolve live transports at call time.
/// 用于检查活动 MCP 工具并在调用时解析实时传输的共享宿主句柄。
#[derive(Clone)]
struct McpRuntimeHandle {
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
    fn snapshot(&self) -> McpRuntimeSnapshot {
        self.inner
            .snapshot
            .read()
            .expect("mcp runtime snapshot poisoned")
            .clone()
    }

    /// Returns one stable MCP runtime overview for upper runtime surfaces.
    /// 返回供上层运行时表面消费的单份稳定 MCP 运行时概览。
    fn overview(&self) -> McpRuntimeOverview {
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

    /// Returns currently available MCP tool views filtered by one allowlist.
    /// 返回按单份白名单过滤后的当前可用 MCP 工具视图集合。
    fn available_tool_views_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Vec<McpRuntimeToolView> {
        self.snapshot().filtered_tool_views(allowlist)
    }

    /// Returns currently available MCP tool bindings filtered by one allowlist.
    /// 返回按单份白名单过滤后的当前可用 MCP 工具绑定集合。
    fn available_tool_bindings_filtered(
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

    /// Requests all background MCP runtime tasks to stop and tear down their transports.
    /// 请求所有后台 MCP 运行时任务停止并拆除各自传输。
    fn shutdown(&self) {
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
pub(crate) async fn bootstrap_mcp_runtime_access(
    config: ProviderConfigurationFile,
) -> Result<McpRuntimeAccess, String> {
    let live_runtime = bootstrap_mcp_runtime(config.clone()).await?;
    Ok(McpRuntimeAccess::new(
        configured_mcp_registry_overview(&config),
        Some(live_runtime),
    ))
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

#[cfg(test)]
mod tests {
    use super::{
        McpRuntimeEventProjection, McpRuntimeEventProjectionKind, McpRuntimeServerDiagnostic,
        McpRuntimeServerSnapshot, McpRuntimeServerStatus, McpRuntimeSnapshot,
        McpRuntimeToolAllowlist, McpRuntimeToolDescriptor,
    };
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn active_summary_rows_only_keep_connecting_and_connected_servers() {
        let snapshot = McpRuntimeSnapshot {
            servers: BTreeMap::from([
                (
                    String::from("connected"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Connected Server"),
                        enabled: true,
                        transport_label: String::from("http"),
                        endpoint_summary: String::from("https://connected.test/mcp"),
                        status: McpRuntimeServerStatus::Connected,
                        last_error: None,
                        tools: Vec::new(),
                    },
                ),
                (
                    String::from("connecting"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Connecting Server"),
                        enabled: true,
                        transport_label: String::from("stdio"),
                        endpoint_summary: String::from("connecting-mcp"),
                        status: McpRuntimeServerStatus::Connecting,
                        last_error: None,
                        tools: Vec::new(),
                    },
                ),
                (
                    String::from("disconnected"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Disconnected Server"),
                        enabled: true,
                        transport_label: String::from("sse"),
                        endpoint_summary: String::from("https://disconnected.test/sse"),
                        status: McpRuntimeServerStatus::Disconnected,
                        last_error: Some(String::from("offline")),
                        tools: Vec::new(),
                    },
                ),
            ]),
        };

        assert_eq!(
            snapshot.active_summary_rows(),
            vec![
                String::from("在线 · Connected Server · http · https://connected.test/mcp"),
                String::from("连接中 · Connecting Server · stdio · connecting-mcp"),
            ]
        );
    }

    #[test]
    fn enabled_server_diagnostics_only_keep_enabled_servers() {
        let snapshot = McpRuntimeSnapshot {
            servers: BTreeMap::from([
                (
                    String::from("enabled"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Enabled Server"),
                        enabled: true,
                        transport_label: String::from("stdio"),
                        endpoint_summary: String::from("enabled-mcp"),
                        status: McpRuntimeServerStatus::Connected,
                        last_error: None,
                        tools: vec![],
                    },
                ),
                (
                    String::from("disabled"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Disabled Server"),
                        enabled: false,
                        transport_label: String::from("http"),
                        endpoint_summary: String::from("https://disabled.test/mcp"),
                        status: McpRuntimeServerStatus::Disabled,
                        last_error: None,
                        tools: vec![],
                    },
                ),
            ]),
        };

        assert_eq!(
            snapshot.enabled_server_diagnostics(),
            vec![McpRuntimeServerDiagnostic {
                name: String::from("Enabled Server"),
                status: McpRuntimeServerStatus::Connected,
                tool_count: 0,
                last_error: None,
            }]
        );
    }

    #[test]
    fn runtime_server_diagnostic_exposes_stable_summary_and_warning_text() {
        let diagnostic = McpRuntimeServerDiagnostic {
            name: String::from("Enabled Server"),
            status: McpRuntimeServerStatus::Disconnected,
            tool_count: 2,
            last_error: Some(String::from("offline")),
        };

        assert_eq!(
            diagnostic.summary_text(),
            String::from("服务器 `Enabled Server` 当前状态为 `离线`，已注册 2 个工具。")
        );
        assert_eq!(
            diagnostic.warning_text(),
            Some(String::from(
                "服务器 `Enabled Server` 当前状态为 `离线`，已注册 2 个工具。 最近一次错误：offline"
            ))
        );
        assert_eq!(
            diagnostic.runtime_event_title(),
            String::from("MCP 服务器离线 · Enabled Server")
        );
    }

    #[test]
    fn enabled_server_discovery_report_preserves_rows_warnings_and_event_projections() {
        let snapshot = McpRuntimeSnapshot {
            servers: BTreeMap::from([
                (
                    String::from("healthy"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Healthy Server"),
                        enabled: true,
                        transport_label: String::from("http"),
                        endpoint_summary: String::from("https://healthy.test/mcp"),
                        status: McpRuntimeServerStatus::Connected,
                        last_error: None,
                        tools: vec![],
                    },
                ),
                (
                    String::from("offline"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Offline Server"),
                        enabled: true,
                        transport_label: String::from("stdio"),
                        endpoint_summary: String::from("offline-mcp"),
                        status: McpRuntimeServerStatus::Disconnected,
                        last_error: Some(String::from("dial tcp timeout")),
                        tools: vec![],
                    },
                ),
            ]),
        };

        let report = snapshot.enabled_server_discovery_report();

        assert_eq!(
            report.discovery_rows(),
            &[
                String::from("服务器 `Healthy Server` 当前状态为 `在线`，已注册 0 个工具。"),
                String::from("服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具。"),
            ]
        );
        assert_eq!(
            report.discovery_errors(),
            &[String::from(
                "服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具。 最近一次错误：dial tcp timeout"
            )]
        );
        assert_eq!(
            report.discovery_warnings(),
            &[String::from(
                "服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具。 最近一次错误：dial tcp timeout"
            )]
        );
        assert_eq!(
            report.event_projections(),
            &[
                McpRuntimeEventProjection {
                    kind: McpRuntimeEventProjectionKind::Informational,
                    title: String::from("MCP 服务器在线 · Healthy Server"),
                    body: String::from(
                        "服务器 `Healthy Server` 当前状态为 `在线`，已注册 0 个工具。"
                    ),
                },
                McpRuntimeEventProjection {
                    kind: McpRuntimeEventProjectionKind::Warning,
                    title: String::from("MCP 服务器离线 · Offline Server"),
                    body: String::from(
                        "服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具。 最近一次错误：dial tcp timeout"
                    ),
                },
            ]
        );
    }

    #[test]
    fn overview_groups_summary_rows_active_rows_and_discovery_report() {
        let snapshot = McpRuntimeSnapshot {
            servers: BTreeMap::from([
                (
                    String::from("healthy"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Healthy Server"),
                        enabled: true,
                        transport_label: String::from("http"),
                        endpoint_summary: String::from("https://healthy.test/mcp"),
                        status: McpRuntimeServerStatus::Connected,
                        last_error: None,
                        tools: vec![],
                    },
                ),
                (
                    String::from("offline"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Offline Server"),
                        enabled: true,
                        transport_label: String::from("stdio"),
                        endpoint_summary: String::from("offline-mcp"),
                        status: McpRuntimeServerStatus::Disconnected,
                        last_error: Some(String::from("dial tcp timeout")),
                        tools: vec![],
                    },
                ),
            ]),
        };

        let overview = snapshot.overview();

        assert_eq!(
            overview.summary_rows(),
            &[
                String::from("在线 · Healthy Server · http · https://healthy.test/mcp"),
                String::from("离线 · Offline Server · stdio · offline-mcp · dial tcp timeout"),
            ]
        );
        assert_eq!(
            overview.active_summary_rows(),
            &[String::from(
                "在线 · Healthy Server · http · https://healthy.test/mcp"
            )]
        );
        assert_eq!(
            overview.discovery_report().discovery_errors(),
            &[String::from(
                "服务器 `Offline Server` 当前状态为 `离线`，已注册 0 个工具。 最近一次错误：dial tcp timeout"
            )]
        );
    }

    #[test]
    fn tool_allowlist_preserves_optional_server_and_tool_sets() {
        let allowlist = McpRuntimeToolAllowlist::new(
            Some(BTreeSet::from([String::from("server-a")])),
            Some(BTreeSet::from([String::from("server-a/read_file")])),
        );

        assert_eq!(
            allowlist.allowed_server_ids(),
            Some(&BTreeSet::from([String::from("server-a")]))
        );
        assert_eq!(
            allowlist.allowed_tool_names(),
            Some(&BTreeSet::from([String::from("server-a/read_file")]))
        );
    }

    #[test]
    fn host_tool_view_preserves_host_visible_descriptor_fields() {
        let descriptor = McpRuntimeToolDescriptor::new(
            String::from("server-a"),
            String::from("read_file"),
            String::from("Read one file"),
            serde_json::json!({"type":"object"}),
        );
        let view = descriptor.host_tool_view();

        assert_eq!(view.id(), "server-a/read_file");
        assert_eq!(view.name(), "read_file");
        assert_eq!(view.description(), "Read one file");
        assert_eq!(view.args_schema(), &serde_json::json!({"type":"object"}));
    }

    #[test]
    fn filtered_tool_views_apply_server_and_tool_allowlists() {
        let tool_a = McpRuntimeToolDescriptor::new(
            String::from("server-a"),
            String::from("read_file"),
            String::from("Read one file"),
            serde_json::json!({}),
        );
        let tool_b = McpRuntimeToolDescriptor::new(
            String::from("server-b"),
            String::from("edit_file"),
            String::from("Edit one file"),
            serde_json::json!({}),
        );
        let snapshot = McpRuntimeSnapshot {
            servers: BTreeMap::from([
                (
                    String::from("server-a"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Server A"),
                        enabled: true,
                        transport_label: String::from("stdio"),
                        endpoint_summary: String::from("server-a"),
                        status: McpRuntimeServerStatus::Connected,
                        last_error: None,
                        tools: vec![tool_a.clone()],
                    },
                ),
                (
                    String::from("server-b"),
                    McpRuntimeServerSnapshot {
                        name: String::from("Server B"),
                        enabled: true,
                        transport_label: String::from("http"),
                        endpoint_summary: String::from("server-b"),
                        status: McpRuntimeServerStatus::Connected,
                        last_error: None,
                        tools: vec![tool_b.clone()],
                    },
                ),
            ]),
        };

        assert_eq!(
            snapshot.filtered_tool_views(&McpRuntimeToolAllowlist::default()),
            vec![tool_a.host_tool_view(), tool_b.host_tool_view()]
        );
        assert_eq!(
            snapshot.filtered_tool_views(&McpRuntimeToolAllowlist::new(
                Some(BTreeSet::from([String::from("server-b")])),
                None,
            )),
            vec![tool_b.host_tool_view()]
        );
        assert_eq!(
            snapshot.filtered_tool_views(&McpRuntimeToolAllowlist::new(
                None,
                Some(BTreeSet::from([String::from("server-a/read_file")])),
            )),
            vec![tool_a.host_tool_view()]
        );
    }
}
