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
