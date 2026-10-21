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

/// Authoritative VMM lifecycle state for one resolved conversation session.
/// 单个已解析会话的 VMM 权威生命周期状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryStatus {
    /// Normal recall and persistence are enabled.
    /// 启用正常召回与持久化。
    #[default]
    Active,
    /// Hidden from default management lists while runtime memory remains enabled.
    /// 从默认管理列表隐藏，但运行时记忆仍然启用。
    Archived,
    /// Recoverable data is in the recycle bin and runtime memory is paused.
    /// 可恢复数据位于回收站，运行时记忆已暂停。
    Recycled,
    /// Content was permanently purged and only a no-content tombstone remains.
    /// 内容已永久清理，仅保留无内容墓碑。
    Forgotten,
}

/// How one PreCheck recall ended, so callers can tell "not searched" from "searched, nothing found".
/// 一次 PreCheck 召回的结束方式，使调用方能区分“未检索”与“已检索但未找到”。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreCheckRecallOutcome {
    /// Memory was not needed or not allowed, so nothing was searched.
    /// 无需或不允许使用记忆，因此未执行检索。
    NotSearched,
    /// The search ran and at least one memory was selected for injection.
    /// 检索已执行，且至少选中一条待注入记忆。
    Matched,
    /// The search ran but no candidate passed the threshold or review.
    /// 检索已执行，但没有候选通过阈值或审核。
    NoMatch,
    /// Intent analysis, search or review failed, so the absence of memories is unknown.
    /// 意图分析、检索或审核失败，因此无法判断是否存在相关记忆。
    Failed,
}

impl SessionMemoryStatus {
    /// Returns whether recall and writeback are allowed in this lifecycle state.
    /// 返回当前生命周期状态是否允许召回与写回。
    pub const fn allows_runtime_memory(self) -> bool {
        matches!(self, Self::Active | Self::Archived)
    }
}

/// Request body for VMM PreCheck.
/// VMM PreCheck 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreCheckRequest {
    /// Stable host turn identity used by native Core mutation reconciliation.
    /// 原生 Core 变更对账使用的稳定宿主回合标识。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub operation_id: String,
    /// Host session identifier used by VMM as the conversation scope.
    /// VMM 用作对话作用域的宿主会话标识。
    pub session_id: String,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreCheckResponse {
    /// Exact source frozen by the executing client, including native linked scopes; not a remote wire field.
    /// 执行客户端冻结的准确来源，包含原生连带范围；不是远端传输字段。
    pub read_source: crate::VmmMemoryReadSource,
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
    /// Authoritative lifecycle state returned by VMM.
    /// VMM 返回的权威生命周期状态。
    #[serde(default)]
    pub session_memory_status: SessionMemoryStatus,
    /// How recall ended; required, because a missing outcome cannot be told apart from a skipped search.
    /// 召回的结束方式；必填，因为缺失的结果无法与跳过检索区分。
    pub recall_outcome: PreCheckRecallOutcome,
    /// Search queries actually run; empty when nothing was searched.
    /// 实际执行的检索词；未检索时为空。
    #[serde(default)]
    pub recall_queries: Vec<String>,
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
    /// Optional local admission guard frozen by asynchronous host delivery; never sent over gRPC.
    /// 异步宿主投递冻结的可选本地准入守卫；绝不发送到 gRPC。
    /// Direct synchronous callers may omit it; queued host deliveries must retain the exact source.
    /// 直接同步调用方可省略；排队的宿主投递必须保留准确来源。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source: Option<crate::VmmMemorySource>,
    /// Stable host turn identity used by native Core intake and analysis retries.
    /// 原生 Core 登记与分析重试使用的稳定宿主回合标识。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub operation_id: String,
    /// Host session identifier used by VMM as the conversation scope.
    /// VMM 用作对话作用域的宿主会话标识。
    pub session_id: String,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
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
    /// Authoritative lifecycle state returned by VMM.
    /// VMM 返回的权威生命周期状态。
    #[serde(default)]
    pub session_memory_status: SessionMemoryStatus,
}

/// Request body for VMM ChatCompact.
/// VMM ChatCompact 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompactRequest {
    /// Optional stable host compaction identity used by native Core reconciliation.
    /// 原生 Core 对账使用的可选稳定宿主压缩标识。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub operation_id: String,
    /// Host session identifier used by VMM as the conversation scope.
    /// VMM 用作对话作用域的宿主会话标识。
    pub session_id: String,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchMemoryEventsRequest {
    /// Identifies the invoking native runtime session; None explicitly denotes a management query.
    /// 标识发起调用的原生运行时会话；None 明确表示管理查询。
    /// External gRPC does not transmit this host-owned field.
    /// 外置 gRPC 不传输该宿主字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_id: Option<String>,
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
    /// Simple natural-language query list.
    /// 简单自然语言查询列表。
    #[serde(default)]
    pub queries: Vec<String>,
    /// Maximum hit count per query.
    /// 每条查询的最大命中数量。
    pub top_k: u32,
    /// Reviewer-facing relevance floor in `[0, 1]`; None applies the service's PreCheck recall threshold.
    /// 面向审核的相关度下限，取值 `[0, 1]`；None 表示使用服务自身的 PreCheck 召回阈值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_score: Option<f64>,
}

/// One AI-facing memory search hit returned by VMM.
/// VMM 返回的一条面向 AI 的记忆检索命中。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    /// Native observed retrieval facts; absent when the connected service does not supply them.
    /// 原生已观察检索事实；连接服务未提供时缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval: Option<MemorySearchRetrieval>,
}

/// Separates memory creation provenance from observed query stages and context evidence.
/// 区分记忆创建来源、已观察查询阶段及情境证据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemorySearchRetrieval {
    /// Canonical creation source, independent of the retrieval path.
    /// 独立于检索路径的规范创建来源。
    pub source_kind: String,
    /// Stable token describing only observed retrieval stages.
    /// 只描述已观察检索阶段的稳定标记。
    pub origin: String,
    /// Human-readable name of the observed retrieval path.
    /// 已观察检索路径的可读名称。
    pub origin_label: String,
    /// Model-facing explanation of recorded channel and ranking participation.
    /// 面向模型说明已记录通道及排序参与情况。
    pub origin_explanation: String,
    /// Final bounded decision score, not a raw vector or lexical producer score.
    /// 最终有界决策分值，而非向量或全文生产者原始分数。
    pub final_score: f64,
    /// True only when Core actually evaluated context evidence; false does not mean no stored evidence.
    /// 仅在 Core 实际评估情境证据时为真；为假不代表没有已存证据。
    pub context_observed: bool,
    /// Total recorded support observations across contexts.
    /// 跨情境记录的支持观测总数。
    pub support_count: u32,
    /// Total recorded rebuttal observations across contexts.
    /// 跨情境记录的反驳观测总数。
    pub rebuttal_count: u32,
    /// Exact normalized context labels matched by this query.
    /// 当前查询匹配的准确规范化情境标签。
    pub matched_context_values: Vec<String>,
    /// Support observations belonging only to matched context values.
    /// 仅属于匹配情境值的支持观测数。
    pub matched_context_support_count: u32,
    /// Rebuttal observations belonging only to matched context values.
    /// 仅属于匹配情境值的反驳观测数。
    pub matched_context_rebuttal_count: u32,
    /// Bounded net context adjustment when that stage actually ran.
    /// 情境阶段实际运行时产生的有界净调整。
    pub matched_context_score_delta: Option<f64>,
}

/// One grouped memory search result for a single query.
/// 单条查询对应的一组记忆检索结果。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    /// Ranked hits dropped because their reviewer-facing score fell below the applied threshold.
    /// 因面向审核的评分低于所应用阈值而被略去的已排名命中数。
    #[serde(default)]
    pub below_threshold_count: u32,
}

/// Response body for VMM SearchMemoryEvents.
/// VMM SearchMemoryEvents 响应体。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchMemoryEventsResponse {
    /// Grouped results for every accepted query.
    /// 每条有效查询对应的分组结果。
    #[serde(default)]
    pub results: Vec<MemorySearchGroupResult>,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
    /// Reviewer-facing threshold actually applied, echoed so omitted hits can be explained.
    /// 实际应用的面向审核阈值，回显以便说明被略去的命中。
    pub min_score: f64,
}

/// Request body for VMM GetTurnDetails.
/// VMM GetTurnDetails 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetTurnDetailsRequest {
    /// Identifies the invoking native runtime session; None explicitly denotes management inspection.
    /// 标识发起调用的原生运行时会话；None 明确表示管理查看。
    /// External gRPC does not transmit this host-owned field.
    /// 外置 gRPC 不传输该宿主字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_id: Option<String>,
    /// Numeric VMM user identifier required by the native authorization path.
    /// 原生授权路径要求的 VMM 数字用户标识。
    #[serde(default)]
    pub user_id: u64,
    /// Numeric VMM project identifier required by the native authorization path.
    /// 原生授权路径要求的 VMM 数字项目标识。
    #[serde(default)]
    pub project_id: u64,
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
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
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
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
    /// Identifies the invoking native runtime session; None explicitly denotes management deletion.
    /// 标识发起调用的原生运行时会话；None 明确表示管理删除。
    /// External gRPC does not transmit this host-owned field.
    /// 外置 gRPC 不传输该宿主字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_session_id: Option<String>,
    /// Numeric VMM user identifier used to scope the delete.
    /// 用于限定删除范围的 VMM 数字用户标识。
    pub user_id: u64,
    /// Numeric VMM project identifier used to scope the delete.
    /// 用于限定删除范围的 VMM 数字项目标识。
    pub project_id: u64,
    /// Stable VulcanCode project identity used only by the process-local native backend.
    /// 仅供进程内原生后端使用的稳定 VulcanCode 项目身份。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_project_id: String,
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
