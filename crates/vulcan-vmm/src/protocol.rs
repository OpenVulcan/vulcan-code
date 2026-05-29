//! Typed VMM request and response models mirroring the current gRPC contract.
//! 与当前 gRPC 契约保持一致的 VMM 请求与响应类型。

use serde::{Deserialize, Serialize};

/// Health check response returned by VMM.
/// VMM 返回的健康检查响应。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthzResponse {
    /// Human-readable service status.
    /// 人类可读的服务状态。
    #[serde(default)]
    pub status: String,
}

/// One canonical VMM project row returned by management calls.
/// 管理调用返回的一条标准 VMM 项目记录。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Numeric VMM team identifier.
    /// VMM 数字团队标识。
    pub team_id: u64,
    /// Numeric VMM space identifier.
    /// VMM 数字空间标识。
    pub space_id: u64,
    /// Team name used in the canonical display path.
    /// 标准展示路径中的团队名称。
    #[serde(default)]
    pub team_name: String,
    /// Space name used in the canonical display path.
    /// 标准展示路径中的空间名称。
    #[serde(default)]
    pub space_name: String,
    /// Project name used in the canonical display path.
    /// 标准展示路径中的项目名称。
    #[serde(default)]
    pub project_name: String,
    /// Human-readable Team/Space/Project display path.
    /// 人类可读的 Team/Space/Project 展示路径。
    #[serde(default)]
    pub display_path: String,
}

/// One durable VMM user row returned by management calls.
/// 管理调用返回的一条长期 VMM 用户记录。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserEntry {
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Durable VMM user name.
    /// 长期 VMM 用户名。
    #[serde(default)]
    pub user_name: String,
}

/// Response body for VMM ListProjects.
/// VMM ListProjects 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListProjectsResponse {
    /// Project entries returned by VMM.
    /// VMM 返回的项目记录集合。
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM ResolveProject.
/// VMM ResolveProject 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveProjectRequest {
    /// Numeric id or canonical Team/Space/Project path.
    /// 数字 ID 或标准 Team/Space/Project 路径。
    pub project_ref: String,
}

/// Response body for VMM ResolveProject.
/// VMM ResolveProject 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveProjectResponse {
    /// Resolved project entry when present.
    /// 解析成功时返回的项目记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectEntry>,
    /// Stable service message.
    /// 稳定服务消息。
    #[serde(default)]
    pub message: String,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM EnsureProject.
/// VMM EnsureProject 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnsureProjectRequest {
    /// Canonical Team/Space/Project path to resolve or create.
    /// 需要解析或创建的标准 Team/Space/Project 路径。
    pub project_path: String,
    /// Whether missing nodes may be created.
    /// 是否允许创建缺失节点。
    pub confirm_create: bool,
}

/// Response body for VMM EnsureProject.
/// VMM EnsureProject 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnsureProjectResponse {
    /// Resolved or created project entry when present.
    /// 解析或创建成功时返回的项目记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectEntry>,
    /// Stable service message.
    /// 稳定服务消息。
    #[serde(default)]
    pub message: String,
    /// Whether the project path already existed.
    /// 项目路径是否已存在。
    pub exists: bool,
    /// Whether another call with confirmation is required.
    /// 是否需要再次确认后调用。
    pub needs_confirm: bool,
    /// Whether the team row was created.
    /// 是否创建了团队记录。
    pub created_team: bool,
    /// Whether the space row was created.
    /// 是否创建了空间记录。
    pub created_space: bool,
    /// Whether the project row was created.
    /// 是否创建了项目记录。
    pub created_project: bool,
    /// Whether the referenced team was missing.
    /// 引用的团队是否缺失。
    pub missing_team: bool,
    /// Whether the referenced space was missing.
    /// 引用的空间是否缺失。
    pub missing_space: bool,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM ResolveUser.
/// VMM ResolveUser 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveUserRequest {
    /// Numeric id or durable user name.
    /// 数字 ID 或长期用户名。
    pub user_ref: String,
    /// Whether a missing user may be created.
    /// 是否允许创建缺失用户。
    pub confirm_create: bool,
}

/// Response body for VMM ResolveUser.
/// VMM ResolveUser 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveUserResponse {
    /// Resolved or created user entry when present.
    /// 解析或创建成功时返回的用户记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<UserEntry>,
    /// Stable service message.
    /// 稳定服务消息。
    #[serde(default)]
    pub message: String,
    /// Whether the user row was created.
    /// 是否创建了用户记录。
    pub created: bool,
    /// Whether the user row already existed.
    /// 用户记录是否已存在。
    pub exists: bool,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Response body for VMM ListUsers.
/// VMM ListUsers 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListUsersResponse {
    /// User entries returned by VMM.
    /// VMM 返回的用户记录集合。
    #[serde(default)]
    pub users: Vec<UserEntry>,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Profile bundle shape requested from VMM.
/// 向 VMM 请求的画像 bundle 形态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProfileBundleMode {
    /// Let the server choose the default bundle shape.
    /// 由服务端选择默认 bundle 形态。
    #[default]
    ProfileBundleModeUnspecified,
    /// Request one combined prompt text.
    /// 请求一段组合提示词文本。
    ProfileBundleModeFull,
    /// Request split TEAM/SPACE/PROJECT/USER texts.
    /// 请求拆分后的 TEAM/SPACE/PROJECT/USER 文本。
    ProfileBundleModeSplit,
}

/// Request body for VMM GetProfileBundle.
/// VMM GetProfileBundle 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetProfileBundleRequest {
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Requested bundle shape.
    /// 请求的 bundle 形态。
    pub mode: ProfileBundleMode,
    /// Whether explanation text should be included.
    /// 是否包含解释文本。
    pub include_explanation: Option<bool>,
}

/// Response body for VMM GetProfileBundle.
/// VMM GetProfileBundle 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetProfileBundleResponse {
    /// Actual bundle shape returned by VMM.
    /// VMM 实际返回的 bundle 形态。
    pub mode: ProfileBundleMode,
    /// Whether explanation text is included.
    /// 是否包含解释文本。
    pub include_explanation: bool,
    /// Optional explanation text.
    /// 可选解释文本。
    #[serde(default)]
    pub explanation_text: String,
    /// Environment priority text returned by VMM.
    /// VMM 返回的环境优先级文本。
    #[serde(default)]
    pub environment_priority_text: String,
    /// Combined prompt bundle text.
    /// 组合提示词 bundle 文本。
    #[serde(default)]
    pub combined_text: String,
    /// Team profile section text.
    /// 团队画像段落文本。
    #[serde(default)]
    pub team_profile: String,
    /// Space profile section text.
    /// 空间画像段落文本。
    #[serde(default)]
    pub space_profile: String,
    /// Project profile section text.
    /// 项目画像段落文本。
    #[serde(default)]
    pub project_profile: String,
    /// User profile section text.
    /// 用户画像段落文本。
    #[serde(default)]
    pub user_profile: String,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Memory recall mode used by one PreCheck request.
/// 单次 PreCheck 请求使用的记忆召回模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreCheckRecallMode {
    /// Legacy recall that ignores compact boundaries.
    /// 忽略压缩边界的旧版召回。
    #[default]
    PreCheckRecallModeLegacy,
    /// Session compact-aware recall that respects the latest ChatCompact anchor.
    /// 感知当前会话 ChatCompact 锚点的压缩召回。
    PreCheckRecallModeSessionCompact,
}

impl PreCheckRecallMode {
    /// Returns the current proto enum label.
    /// 返回当前 proto 枚举标签。
    pub const fn proto_label(self) -> &'static str {
        match self {
            Self::PreCheckRecallModeLegacy => "PRE_CHECK_RECALL_MODE_LEGACY",
            Self::PreCheckRecallModeSessionCompact => "PRE_CHECK_RECALL_MODE_SESSION_COMPACT",
        }
    }
}

/// Request body for VMM PreCheck.
/// VMM PreCheck 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreCheckRequest {
    /// Host session identifier used by VMM as the conversation scope.
    /// VMM 用作对话作用域的宿主会话标识。
    pub session_id: String,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Current user prompt content.
    /// 当前用户提示词内容。
    pub user_content: String,
    /// Requested recall mode.
    /// 请求使用的召回模式。
    pub recall_mode: PreCheckRecallMode,
}

/// One structured context item returned by PreCheck.
/// PreCheck 返回的一条结构化上下文项。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextItem {
    /// Provider-visible memory text.
    /// Provider 可见的记忆文本。
    #[serde(default)]
    pub text: String,
    /// VMM relevance score.
    /// VMM 相关性分数。
    #[serde(default)]
    pub score: f64,
    /// Source turn id when the item came from a dialogue turn.
    /// 当该项来自对话 turn 时的来源 turn 标识。
    #[serde(default)]
    pub turn_id: u64,
    /// Whether the item can be expanded through GetTurnDetails.
    /// 该项是否可通过 GetTurnDetails 展开。
    #[serde(default)]
    pub has_dialogue: bool,
    /// Human-readable creation datetime returned by VMM.
    /// VMM 返回的人类可读创建时间。
    #[serde(default)]
    pub created_datetime: String,
}

/// Response body for VMM PreCheck.
/// VMM PreCheck 响应体。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PreCheckResponse {
    /// Whether the caller should inject returned context items.
    /// 调用方是否应注入返回的上下文项。
    #[serde(default)]
    pub should_inject: bool,
    /// Structured context items selected by VMM.
    /// VMM 选出的结构化上下文项。
    #[serde(default)]
    pub context_items: Vec<ContextItem>,
    /// Whether VMM degraded recall quality while still returning a response.
    /// VMM 是否在降级召回质量后仍返回响应。
    #[serde(default)]
    pub degraded: bool,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// VMM timeline item role accepted by PostAction.
/// PostAction 接收的 VMM 时间线条目角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PostActionTimelineType {
    /// User-authored middle timeline content.
    /// 用户产生的中间时间线内容。
    User,
    /// Assistant-authored middle timeline content.
    /// 助手产生的中间时间线内容。
    Assistant,
}

impl PostActionTimelineType {
    /// Returns the wire role name accepted by VMM.
    /// 返回 VMM 接收的线级角色名称。
    pub const fn as_wire_type(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

/// One middle timeline item sent to PostAction.
/// 发送给 PostAction 的一条中间时间线项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostActionTimelineItem {
    /// Timeline item role.
    /// 时间线项角色。
    #[serde(rename = "type")]
    pub kind: PostActionTimelineType,
    /// Timeline item content.
    /// 时间线项内容。
    pub content: String,
}

/// Request body for VMM PostAction.
/// VMM PostAction 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostActionRequest {
    /// Host session identifier used by VMM as the conversation scope.
    /// VMM 用作对话作用域的宿主会话标识。
    pub session_id: String,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// First user prompt for the completed logical turn.
    /// 已完成逻辑回合的首个用户提示词。
    pub user_content: String,
    /// Final assistant answer for the completed logical turn.
    /// 已完成逻辑回合的最终助手回答。
    pub assistant_content: String,
    /// Middle timeline items between user_content and assistant_content.
    /// 位于 user_content 与 assistant_content 之间的中间时间线项。
    #[serde(default)]
    pub timeline: Vec<PostActionTimelineItem>,
}

/// Response body for VMM PostAction.
/// VMM PostAction 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostActionResponse {
    /// Whether VMM accepted and durably queued the turn.
    /// VMM 是否接受并稳定入队该 turn。
    #[serde(default)]
    pub accepted: bool,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM ChatCompact.
/// VMM ChatCompact 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompactRequest {
    /// Host session identifier used by VMM as the conversation scope.
    /// VMM 用作对话作用域的宿主会话标识。
    pub session_id: String,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
}

/// Response body for VMM ChatCompact.
/// VMM ChatCompact 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompactResponse {
    /// Whether VMM accepted the compact acknowledgement.
    /// VMM 是否接受这次压缩确认。
    #[serde(default)]
    pub accepted: bool,
    /// Whether the stored compact anchor changed.
    /// 已保存压缩锚点是否发生变化。
    #[serde(default)]
    pub updated: bool,
    /// Turn id currently used as compact boundary.
    /// 当前作为压缩边界的 turn 标识。
    #[serde(default)]
    pub compacted_turn_id: u64,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM SearchMemoryEvents.
/// VMM SearchMemoryEvents 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMemoryEventsRequest {
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Simple natural-language query list.
    /// 简单自然语言查询列表。
    #[serde(default)]
    pub queries: Vec<String>,
    /// Maximum hit count per query.
    /// 每条查询的最大命中数量。
    pub top_k: u32,
}

/// One AI-facing memory search hit returned by VMM.
/// VMM 返回的一条面向 AI 的记忆检索命中。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySearchHit {
    /// Durable memory identifier.
    /// 长期记忆标识。
    pub memory_id: u64,
    /// Source turn id when the memory can be expanded.
    /// 记忆可展开时对应的来源 turn 标识。
    pub source_turn_id: u64,
    /// Compact memory abstract.
    /// 精简记忆摘要。
    #[serde(default)]
    pub abstract_text: String,
    /// Details preview for quick model inspection.
    /// 供模型快速查看的详情预览。
    #[serde(default)]
    pub details_preview: String,
    /// Memory category label.
    /// 记忆分类标签。
    #[serde(default)]
    pub category: String,
    /// Human-readable creation datetime returned by VMM.
    /// VMM 返回的人类可读创建时间。
    #[serde(default)]
    pub created_datetime: String,
}

/// One grouped memory search result for a single query.
/// 单条查询对应的一组记忆检索结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySearchGroupResult {
    /// Query index echoed from the request order.
    /// 按请求顺序回显的查询索引。
    pub query_index: u32,
    /// Normalized query text returned by VMM.
    /// VMM 返回的规范化查询文本。
    #[serde(default)]
    pub query: String,
    /// Hits produced for the query.
    /// 该查询产生的命中列表。
    #[serde(default)]
    pub hits: Vec<MemorySearchHit>,
}

/// Response body for VMM SearchMemoryEvents.
/// VMM SearchMemoryEvents 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMemoryEventsResponse {
    /// Grouped results for every accepted query.
    /// 每条有效查询对应的分组结果。
    #[serde(default)]
    pub results: Vec<MemorySearchGroupResult>,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM GetTurnDetails.
/// VMM GetTurnDetails 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetTurnDetailsRequest {
    /// Durable VMM turn identifiers to inspect.
    /// 需要查看的长期 VMM turn 标识集合。
    #[serde(default)]
    pub turn_ids: Vec<u64>,
}

/// One AI-ready turn detail entry returned by VMM.
/// VMM 返回的一条适合 AI 消费的 turn 详情。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnDetailEntry {
    /// Durable VMM turn identifier.
    /// 长期 VMM turn 标识。
    pub turn_id: u64,
    /// First user question recorded for the turn.
    /// 该 turn 记录的首个用户问题。
    #[serde(default)]
    pub user_question: String,
    /// Middle timeline items between user and final assistant answer.
    /// 用户问题与最终助手回答之间的中间时间线项。
    #[serde(default)]
    pub timeline: Vec<PostActionTimelineItem>,
    /// Final assistant answer recorded for the turn.
    /// 该 turn 记录的最终助手回答。
    #[serde(default)]
    pub assistant_answer: String,
    /// Optional detailed text returned by VMM.
    /// VMM 返回的可选详情文本。
    #[serde(default)]
    pub detail: String,
    /// Neighboring previous turn ids.
    /// 相邻的前置 turn 标识。
    #[serde(default)]
    pub previous_turn_ids: Vec<u64>,
    /// Neighboring next turn ids.
    /// 相邻的后续 turn 标识。
    #[serde(default)]
    pub next_turn_ids: Vec<u64>,
}

/// Response body for VMM GetTurnDetails.
/// VMM GetTurnDetails 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetTurnDetailsResponse {
    /// Turn details returned by VMM.
    /// VMM 返回的 turn 详情集合。
    #[serde(default)]
    pub turns: Vec<TurnDetailEntry>,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// One direct AI-written memory item.
/// 一条 AI 主动写入的记忆项。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteMemoryItem {
    /// Scope level accepted by VMM.
    /// VMM 接收的作用域级别。
    pub scope_level: i32,
    /// Compact memory abstract.
    /// 精简记忆摘要。
    #[serde(default)]
    pub abstract_text: String,
    /// Full memory details.
    /// 完整记忆详情。
    #[serde(default)]
    pub details: String,
    /// Numeric memory category.
    /// 数字记忆分类。
    pub category: i32,
    /// Numeric memory priority.
    /// 数字记忆优先级。
    pub priority: i32,
    /// Numeric memory level.
    /// 数字记忆层级。
    pub memory_level: i32,
}

/// Request body for VMM WriteMemories.
/// VMM WriteMemories 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteMemoriesRequest {
    /// Host session identifier used for attribution.
    /// 用于归因的宿主会话标识。
    pub session_id: String,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Direct memory items to persist.
    /// 需要持久化的主动记忆项。
    #[serde(default)]
    pub items: Vec<WriteMemoryItem>,
}

/// One direct-write result item returned by VMM.
/// VMM 返回的一条主动写入结果项。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteMemoryResultItem {
    /// Created or reused durable memory identifier.
    /// 新建或复用的长期记忆标识。
    pub memory_id: u64,
    /// Whether the item reused an existing memory.
    /// 该项是否复用了已有记忆。
    pub deduped: bool,
}

/// Response body for VMM WriteMemories.
/// VMM WriteMemories 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteMemoriesResponse {
    /// Ordered direct-write results.
    /// 按顺序返回的主动写入结果。
    #[serde(default)]
    pub items: Vec<WriteMemoryResultItem>,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}
