use super::*;

/// MCP service settings as shown on the settings page; token digests never leave the service.
/// 设置页展示的 MCP 服务设置；令牌摘要绝不离开服务。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServiceSnapshot {
    /// Whether the endpoint accepts authenticated requests.
    /// 端点是否接受已认证请求。
    pub enabled: bool,
    /// Exact endpoint URL of the running service, absent before the listener is bound.
    /// 运行中服务的精确端点地址，监听器绑定前为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_url: Option<String>,
    /// Registered clients in creation order.
    /// 按创建顺序排列的已登记客户端。
    pub clients: Vec<McpServiceClientSummary>,
    /// Media targets that may be granted, one row per account and target.
    /// 可授权的媒体目标，每个账号与目标一行。
    pub media_candidates: Vec<McpMediaTargetCandidate>,
    /// Bounds the settings page enforces before saving.
    /// 设置页在保存前执行的边界。
    pub limits: McpServiceLimits,
    /// Available tools and knowledge libraries for the local authorization editor.
    /// 供本地授权编辑器使用的工具及知识库清单。
    pub tool_candidates: Vec<String>,
    /// Active knowledge library names keyed by stable ID.
    /// 以稳定 ID 为键的活动知识库名称。
    pub knowledge_candidates: std::collections::BTreeMap<String, String>,
}

/// Bounds of the MCP service settings, derived from the single persisted definitions.
/// MCP 服务设置的边界，派生自唯一持久化定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServiceLimits {
    /// Largest number of clients.
    /// 最大客户端数。
    pub max_clients: usize,
    /// Longest client name in Unicode scalar values.
    /// 最长客户端名称（按 Unicode 标量值计）。
    pub max_name_chars: usize,
    /// Largest explicit project list.
    /// 最大显式项目清单。
    pub max_projects: usize,
    /// Largest media target list.
    /// 最大媒体目标清单。
    pub max_media_targets: usize,
}

/// One registered client without its token digest.
/// 一个不含令牌摘要的已登记客户端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServiceClientSummary {
    /// Whether this client records tool calls.
    /// 是否记录此客户端的工具调用。
    #[serde(default)]
    pub record_calls: bool,
    /// Stable client identifier.
    /// 稳定客户端标识。
    pub id: String,
    /// User-chosen display name.
    /// 用户选择的显示名称。
    pub name: String,
    /// Last characters of the token.
    /// 令牌末尾几个字符。
    pub token_hint: String,
    /// Creation time in Unix milliseconds.
    /// 创建时间（Unix 毫秒）。
    pub created_at_ms: u64,
    /// Whether the client may authenticate.
    /// 客户端是否可以认证。
    pub enabled: bool,
    /// Granted session operations.
    /// 已授予的会话操作。
    pub sessions: McpSessionGrantEntry,
    /// Granted media targets.
    /// 已授予的媒体目标。
    pub media_targets: Vec<McpMediaTargetEntry>,
    /// Last authenticated call since the service started.
    /// 本次服务启动以来最近一次已认证调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at_ms: Option<u64>,
    /// Tool calls since the service started.
    /// 本次服务启动以来的工具调用次数。
    pub call_count: u64,
    /// Effective individual tool grants shown in the local editor.
    /// 在本地授权编辑器显示的有效独立工具授权。
    pub tool_ids: Vec<String>,
    /// Optional project restriction of the workspace tools; empty is unrestricted.
    /// 工作区工具可选的项目限制；空清单表示不限定项目。
    pub tool_project_ids: Vec<String>,
    /// Knowledge library IDs and their write flags.
    /// 知识库 ID 及其写权限标记。
    pub knowledge_libraries: std::collections::BTreeMap<String, bool>,
}

/// Session operations granted to one client.
/// 授予一个客户端的会话操作。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpSessionGrantEntry {
    /// List, read, and wait.
    /// 列出、读取与等待。
    pub read: bool,
    /// Send messages to existing sessions.
    /// 向已有会话发送消息。
    pub send: bool,
    /// Create sessions.
    /// 新建会话。
    pub create: bool,
    /// Stop running sessions.
    /// 停止运行中的会话。
    pub stop: bool,
    /// Answer questions sessions ask, never permission approvals.
    /// 回答会话提问，绝不包括权限审批。
    pub answer_questions: bool,
    /// Covered projects.
    /// 覆盖的项目。
    pub projects: McpProjectScopeEntry,
}

/// Projects covered by a session grant.
/// 会话授权覆盖的项目。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum McpProjectScopeEntry {
    /// Every project.
    /// 全部项目。
    #[default]
    All,
    /// Only the listed projects.
    /// 仅列出的项目。
    Selected {
        /// Exact project identifiers.
        /// 精确项目标识。
        project_ids: Vec<String>,
    },
}

/// One granted media target.
/// 一个已授予的媒体目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpMediaTargetEntry {
    /// Credential connection.
    /// 凭据连接。
    pub connection_id: String,
    /// Manifest target identifier.
    /// Manifest 目标标识。
    pub target_id: String,
}

/// One media target that may be granted to MCP clients.
/// 一个可授予 MCP 客户端的媒体目标。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpMediaTargetCandidate {
    /// Credential connection.
    /// 凭据连接。
    pub connection_id: String,
    /// Manifest target identifier.
    /// Manifest 目标标识。
    pub target_id: String,
    /// Stable capability identifier such as `image_generate`.
    /// 稳定能力标识，如 `image_generate`。
    pub capability: String,
    /// Public `provider/model` identifier MCP clients pass.
    /// MCP 客户端传入的公开 `provider/model` 标识。
    pub model: String,
    /// Human-readable target name.
    /// 人类可读目标名称。
    pub display_name: String,
    /// Human-readable account name.
    /// 人类可读账号名称。
    pub account_name: String,
}

/// Creates (no `client_id`) or updates one client.
/// 新建（无 `client_id`）或更新一个客户端。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServiceClientSaveRequest {
    /// Explicit logging preference; omission preserves the current setting.
    /// 显式日志偏好；省略时保留当前设置。
    #[serde(default)]
    pub record_calls: Option<bool>,
    /// Existing client to update; absent creates a client and returns its token once.
    /// 要更新的已有客户端；缺省时新建客户端并一次性返回其令牌。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Display name.
    /// 显示名称。
    pub name: String,
    /// Whether the client may authenticate.
    /// 客户端是否可以认证。
    pub enabled: bool,
    /// Granted session operations.
    /// 已授予的会话操作。
    pub sessions: McpSessionGrantEntry,
    /// Granted media targets.
    /// 已授予的媒体目标。
    pub media_targets: Vec<McpMediaTargetEntry>,
    /// Explicit selection; older clients may omit it to retain their saved grants.
    /// 显式选择；旧版客户端可省略以保留已有授权。
    #[serde(default)]
    pub tool_ids: Option<Vec<String>>,
    /// Optional project restriction; empty clears it and omission retains the saved value.
    /// 可选项目限制；空清单清除限制，省略时保留已保存的值。
    #[serde(default)]
    pub tool_project_ids: Option<Vec<String>>,
    /// Knowledge access; omission retains the saved value.
    /// 知识库权限；省略时保留已保存的值。
    #[serde(default)]
    pub knowledge_libraries: Option<std::collections::BTreeMap<String, bool>>,
}

/// Filters and pages the local MCP call history.
/// 筛选与分页读取本地 MCP 调用历史。
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct McpCallLogQuery {
    /// Optional client identifier.
    /// 可选客户端标识。
    pub client_id: Option<String>,
    /// Optional short MCP project identifier.
    /// 可选 MCP 项目短标识。
    pub project_id: Option<String>,
    /// Optional short MCP session identifier.
    /// 可选 MCP 会话短标识。
    pub sid: Option<String>,
    /// Last call from the previous page.
    /// 上一页的末条调用。
    pub cursor: Option<i64>,
    /// Shows oldest calls first if true.
    /// 为真时从最早调用开始显示。
    pub ascending: bool,
}

/// Settings snapshot plus a freshly issued token, shown to the user exactly once.
/// 设置快照以及新签发的令牌，只向用户展示一次。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServiceIssuedToken {
    /// Settings after the change.
    /// 变更后的设置。
    pub snapshot: McpServiceSnapshot,
    /// Client that owns the token.
    /// 拥有该令牌的客户端。
    pub client_id: String,
    /// Bearer token; the service keeps only its digest.
    /// Bearer 令牌；服务只保留其摘要。
    pub token: String,
}
/// Model-facing visibility and project scope for a configured MCP server.
/// 已配置 MCP 服务器面向模型的公开模式和项目范围。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerAccessEntry {
    /// Whether to include the server's tool schemas in each request.
    /// 是否在每次请求中包含服务器的工具 Schema。
    pub exposure: McpToolExposureEntry,
    /// Projects that may use this server.
    /// 可以使用此服务器的项目。
    pub projects: McpProjectScopeEntry,
    /// User-authored guidance on when to use this server.
    /// 用户填写的服务器使用时机说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_hint: Option<String>,
}

/// Wire representation of model-facing MCP tool exposure.
/// 面向模型的 MCP 工具公开模式的线协议表示。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpToolExposureEntry {
    /// Discover on demand.
    /// 按需发现。
    #[default]
    Implicit,
    /// Include complete tool definitions in the request.
    /// 在请求中包含完整工具定义。
    Explicit,
}
