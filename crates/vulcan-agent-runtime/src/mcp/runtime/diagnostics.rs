use super::*;

/// Optional MCP runtime tool allowlist used when projecting host-visible MCP bindings.
/// 投影宿主可见 MCP 绑定时使用的可选 MCP 运行时工具白名单。
#[derive(Debug, Clone, Default)]
pub struct McpRuntimeToolAllowlist {
    /// Optional server identifier allowlist.
    /// 可选的服务器标识白名单。
    pub(super) allowed_server_ids: Option<BTreeSet<String>>,
    /// Optional tool-name or qualified-name allowlist.
    /// 可选的工具名或限定工具名白名单。
    pub(super) allowed_tool_names: Option<BTreeSet<String>>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
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
    /// Server exhausted its bounded automatic retries and requires an explicit lifecycle action.
    /// 服务器已耗尽有限自动重试，需要显式生命周期操作才能恢复。
    Failed,
    /// Server task completed host-requested transport teardown and will not reconnect.
    /// 服务器任务已完成宿主请求的传输拆除，且不会重新连接。
    Stopped,
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
            Self::Failed => "失败",
            Self::Stopped => "已停止",
        }
    }
}

/// Structured source-aware runtime state for one MCP server exposed to service diagnostics.
/// 暴露给服务诊断的单个 MCP 服务器结构化来源感知运行时状态。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRuntimeServerState {
    /// Exact globally unique runtime server identifier.
    /// 精确且全局唯一的运行时服务器标识。
    pub id: String,
    /// User-facing server name.
    /// 面向用户的服务器名称。
    pub name: String,
    /// Stable source label: `user` or `plugin`.
    /// 稳定来源标签：`user` 或 `plugin`。
    pub source: String,
    /// Owning plugin identifier for plugin partitions.
    /// 插件分区的所属插件标识。
    pub plugin_id: Option<String>,
    /// Owning plugin content generation for plugin partitions.
    /// 插件分区的所属插件内容代次。
    pub generation: Option<u64>,
    /// Stable transport-family label.
    /// 稳定传输类型标签。
    pub transport: String,
    /// Credential-free endpoint or command summary.
    /// 不含凭据的端点或命令摘要。
    pub endpoint: String,
    /// Current connectivity state.
    /// 当前连通状态。
    pub status: McpRuntimeServerStatus,
    /// Consecutive automatic retry count for the current unstable period.
    /// 当前不稳定周期内的连续自动重试次数。
    pub retry_count: u8,
    /// MCP protocol version negotiated during initialize.
    /// initialize 阶段协商得到的 MCP 协议版本。
    pub protocol_version: Option<String>,
    /// Server implementation product name negotiated during initialize.
    /// initialize 阶段协商得到的服务器实现产品名称。
    pub server_product: Option<String>,
    /// Server implementation binary version negotiated during initialize.
    /// initialize 阶段协商得到的服务器实现二进制版本。
    pub server_version: Option<String>,
    /// Number of currently visible tools from the complete last valid snapshot.
    /// 当前完整且最近有效快照中可见工具的数量。
    pub tool_count: usize,
    /// Number of currently discovered prompts.
    /// 当前已发现 Prompt 数量。
    pub prompt_count: usize,
    /// Number of currently discovered resources.
    /// 当前已发现 Resource 数量。
    pub resource_count: usize,
    /// Latest sanitized connection or discovery error.
    /// 最近一次已脱敏的连接或发现错误。
    pub last_error: Option<String>,
    /// Bounded sanitized transport diagnostics, newest entries last.
    /// 有界脱敏传输诊断，较新的条目位于末尾。
    pub recent_logs: Vec<String>,
}

/// Read-only catalog and state captured from the same runtime snapshot.
/// 从同一运行时快照采集的只读目录与状态。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRuntimeServerInspection {
    /// Process-local catalog revision changed on reconnect or tool catalog replacement.
    /// 重连或工具目录替换时更新的进程内目录版本。
    pub inspection_generation: u64,
    /// Live server state, including bounded server-specific diagnostics.
    /// 实时服务器状态，包含有界的服务器专属诊断。
    pub state: McpRuntimeServerState,
    /// Original schemas; discovery does not imply current AI authorization.
    /// 原始 Schema；发现工具不代表当前 AI 获得调用授权。
    pub tools: Vec<McpRuntimeToolInspection>,
}

/// Original tool metadata retained without schema reduction.
/// 完整保留 Schema 的原始工具元数据。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRuntimeToolInspection {
    /// Original server-owned tool name.
    /// 服务器拥有的原始工具名称。
    pub name: String,
    /// Original tool description.
    /// 原始工具描述。
    pub description: String,
    /// Complete argument JSON Schema.
    /// 完整参数 JSON Schema。
    pub input_schema: serde_json::Value,
    /// Complete result JSON Schema, or null when absent.
    /// 完整结果 JSON Schema，未声明时为空。
    pub output_schema: serde_json::Value,
}

/// Runtime snapshot of one configured MCP server.
/// 某个已配置 MCP 服务器的运行时快照。
#[derive(Debug, Clone)]
pub(super) struct McpRuntimeServerSnapshot {
    /// Process-local identity of the current catalog capture.
    /// 当前目录采集的进程内身份。
    pub(super) inspection_generation: u64,
    /// Human-readable server name.
    /// 人类可读服务器名称。
    pub(super) name: String,
    /// Whether the server is enabled in configuration.
    /// 该服务器在配置中是否启用。
    pub(super) enabled: bool,
    /// Transport-family label used for diagnostics.
    /// 用于诊断的传输类型标签。
    pub(super) transport_label: String,
    /// Compact endpoint or command summary shown in the workbench.
    /// 展示在工作台中的紧凑端点或命令摘要。
    pub(super) endpoint_summary: String,
    /// Current runtime connectivity status.
    /// 当前运行时连通状态。
    pub(super) status: McpRuntimeServerStatus,
    /// Latest connection or refresh warning, if any.
    /// 最近一次连接或刷新警告（如果存在）。
    pub(super) last_error: Option<String>,
    /// Last bounded sanitized transport logs retained across disconnect.
    /// 跨断连保留的最近有界脱敏传输日志。
    pub(super) recent_logs: Vec<String>,
    /// Currently live tool descriptors registered from the active transport.
    /// 当前从活动传输注册得到的实时工具描述集合。
    pub(super) tools: Vec<McpRuntimeToolDescriptor>,
    /// Last successfully discovered tool descriptors retained across reconnect attempts.
    /// 在重连尝试之间保留的最后一次成功发现的工具描述集合。
    pub(super) cached_tools: Vec<McpRuntimeToolDescriptor>,
    /// Number of prompts currently reported by this server.
    /// 当前由该服务器报告的 prompt 数量。
    pub(super) prompt_count: usize,
    /// Number of resources currently reported by this server.
    /// 当前由该服务器报告的资源数量。
    pub(super) resource_count: usize,
}

impl McpRuntimeServerSnapshot {
    /// Returns the tool descriptors that should stay visible to upper host layers for the current connectivity state.
    /// 返回在当前连通状态下应继续对上层宿主可见的工具描述集合。
    pub(super) fn visible_tool_descriptors(&self) -> &[McpRuntimeToolDescriptor] {
        match self.status {
            McpRuntimeServerStatus::Connected => &self.tools,
            McpRuntimeServerStatus::Connecting | McpRuntimeServerStatus::Disconnected => {
                if self.cached_tools.is_empty() {
                    &self.tools
                } else {
                    &self.cached_tools
                }
            }
            McpRuntimeServerStatus::Disabled
            | McpRuntimeServerStatus::Failed
            | McpRuntimeServerStatus::Stopped => &self.tools,
        }
    }
}

/// Lightweight runtime-facing diagnostic view for one enabled MCP server.
/// 面向运行时的单个已启用 MCP 服务器轻量诊断视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRuntimeServerDiagnostic {
    /// Human-readable server name.
    /// 人类可读服务器名称。
    pub(super) name: String,
    /// Current runtime connectivity status.
    /// 当前运行时连通状态。
    pub(super) status: McpRuntimeServerStatus,
    /// Number of currently registered tools.
    /// 当前已注册工具数量。
    pub(super) tool_count: usize,
    /// Number of currently discovered prompts.
    /// 当前已发现 prompt 数量。
    pub(super) prompt_count: usize,
    /// Number of currently discovered resources.
    /// 当前已发现资源数量。
    pub(super) resource_count: usize,
    /// Latest connection or refresh warning, if any.
    /// 最近一次连接或刷新警告（如果存在）。
    pub(super) last_error: Option<String>,
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
            "服务器 `{}` 当前状态为 `{}`，已注册 {} 个工具、{} 个 prompt 与 {} 个资源。",
            self.server_name(),
            self.status().label(),
            self.tool_count(),
            self.prompt_count,
            self.resource_count
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
    ///
    /// # Returns
    /// 返回值
    /// A title derived from connectivity status; refresh warnings do not misclassify connected servers as offline.
    /// 返回根据连接状态派生的标题；刷新警告不会把已连接服务器误判为离线。
    pub fn runtime_event_title(&self) -> String {
        match self.status() {
            McpRuntimeServerStatus::Disabled => {
                format!("MCP 服务器已停用 · {}", self.server_name())
            }
            McpRuntimeServerStatus::Connecting => {
                format!("MCP 服务器连接中 · {}", self.server_name())
            }
            McpRuntimeServerStatus::Connected => {
                format!("MCP 服务器在线 · {}", self.server_name())
            }
            McpRuntimeServerStatus::Disconnected => {
                format!("MCP 服务器离线 · {}", self.server_name())
            }
            McpRuntimeServerStatus::Failed => {
                format!("MCP 服务器失败 · {}", self.server_name())
            }
            McpRuntimeServerStatus::Stopped => {
                format!("MCP 服务器已停止 · {}", self.server_name())
            }
        }
    }
}

/// Lightweight MCP runtime event severity used by upper-layer projection adapters.
/// 供上层投影适配器使用的轻量 MCP 运行时事件严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum McpRuntimeEventProjectionKind {
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
    pub(super) kind: McpRuntimeEventProjectionKind,
    /// Recommended event title for UI projection.
    /// 供界面投影使用的建议事件标题。
    pub(super) title: String,
    /// Recommended event body for UI projection.
    /// 供界面投影使用的建议事件正文。
    pub(super) body: String,
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
pub struct McpRuntimeDiscoveryReport {
    /// Recommended UI-facing runtime event projections.
    /// 建议用于界面展示的运行时事件投影视图集合。
    pub(super) event_projections: Vec<McpRuntimeEventProjection>,
    /// Human-readable discovery summary rows.
    /// 人类可读的发现摘要行集合。
    pub(super) discovery_rows: Vec<String>,
    /// Human-readable discovery error rows.
    /// 人类可读的发现错误行集合。
    pub(super) discovery_errors: Vec<String>,
    /// Human-readable discovery warning rows.
    /// 人类可读的发现告警行集合。
    pub(super) discovery_warnings: Vec<String>,
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
pub struct McpRuntimeOverview {
    /// Human-readable summary rows covering all configured runtime-visible servers.
    /// 覆盖全部已配置运行时可见服务器的人类可读摘要行集合。
    pub(super) summary_rows: Vec<String>,
    /// Human-readable summary rows covering only active runtime-visible servers.
    /// 仅覆盖活动运行时可见服务器的人类可读摘要行集合。
    pub(super) active_summary_rows: Vec<String>,
    /// Stable discovery report exported to upper bootstrap and diagnostics flows.
    /// 导出给上层 bootstrap 与诊断流程的稳定发现报告。
    pub(super) discovery_report: McpRuntimeDiscoveryReport,
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
    pub(super) fn to_runtime_event_projection(&self) -> McpRuntimeEventProjection {
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
