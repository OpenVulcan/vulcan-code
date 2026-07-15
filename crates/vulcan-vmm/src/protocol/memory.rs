//! VMM memory and turn protocol models.
//! VMM 记忆与回合协议模型。

use serde::{Deserialize, Serialize};

/// Memory recall mode used by one PreCheck request.
/// 单次 PreCheck 请求使用的记忆召回模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreCheckRecallMode {
    /// Server-selected safe default used when callers omit the recall mode.
    /// 调用方省略召回模式时，由服务端选择安全默认策略。
    #[default]
    PreCheckRecallModeUnspecified,
    /// Session compact-aware recall that respects the latest ChatCompact anchor.
    /// 感知当前会话 ChatCompact 锚点的压缩召回。
    PreCheckRecallModeSessionCompact,
    /// Full recall that explicitly disables the current-session compact boundary.
    /// 显式关闭当前会话压缩边界的完整召回。
    PreCheckRecallModeFull,
}

impl PreCheckRecallMode {
    /// Returns the current proto enum label.
    /// 返回当前 proto 枚举标签。
    pub const fn proto_label(self) -> &'static str {
        match self {
            Self::PreCheckRecallModeUnspecified => "PRE_CHECK_RECALL_MODE_UNSPECIFIED",
            Self::PreCheckRecallModeSessionCompact => "PRE_CHECK_RECALL_MODE_SESSION_COMPACT",
            Self::PreCheckRecallModeFull => "PRE_CHECK_RECALL_MODE_FULL",
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
    /// Durable memory id when the context item came from a memory row.
    /// 当上下文项来自记忆行时对应的长期记忆标识。
    #[serde(default)]
    pub memory_id: u64,
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

/// Request body for VMM DeleteMemories.
/// VMM DeleteMemories 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteMemoriesRequest {
    /// Numeric VMM user identifier used to scope the delete.
    /// 用于限定删除范围的 VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier used to scope the delete.
    /// 用于限定删除范围的 VMM 数字项目标识。
    pub project_id: u64,
    /// Durable memory ids requested for deletion.
    /// 请求删除的长期记忆标识集合。
    #[serde(default)]
    pub memory_ids: Vec<u64>,
    /// Human or AI supplied deletion reason.
    /// 人类或 AI 提供的删除原因。
    #[serde(default)]
    pub reason: String,
}

/// Response body for VMM DeleteMemories.
/// VMM DeleteMemories 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteMemoriesResponse {
    /// Memory ids that were marked deleted by VMM.
    /// 已被 VMM 标记为删除的记忆标识集合。
    #[serde(default)]
    pub deleted_memory_ids: Vec<u64>,
    /// Memory ids that were missing, already deleted, inactive, or outside scope.
    /// 不存在、已删除、非 active 或不属于当前范围的记忆标识集合。
    #[serde(default)]
    pub not_found_memory_ids: Vec<u64>,
    /// Vector rows deleted immediately by VMM.
    /// VMM 即时删除的向量行数。
    pub deleted_vector_rows: u64,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}
