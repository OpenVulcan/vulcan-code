use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use vulcan_core::types::ProviderUsageSnapshot;

/// Current schema version of the session-scoped physical Provider call ledger.
/// 会话级物理 Provider 调用账本的当前 Schema 版本。
pub const SESSION_PROVIDER_CALL_LEDGER_SCHEMA_VERSION: u32 = 2;

/// Lifecycle state of one physical Provider request.
/// 单次物理 Provider 请求的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionProviderCallStatus {
    /// The request identity exists durably before network dispatch.
    /// 请求身份已在网络分派前可靠存在。
    Prepared,
    /// The transport has proven that the request was dispatched.
    /// 传输层已证明请求完成分派。
    Dispatched,
    /// The Provider returned a successful terminal response.
    /// Provider 已返回成功终态响应。
    Succeeded,
    /// The request failed without a more specific terminal category.
    /// 请求失败且没有更具体的终态类别。
    Failed,
    /// The request was cancelled.
    /// 请求已取消。
    Cancelled,
    /// The request timed out.
    /// 请求已超时。
    TimedOut,
    /// The process restarted while the request was still open.
    /// 请求尚未闭合时进程已重启。
    Interrupted,
}

impl SessionProviderCallStatus {
    /// Returns whether this state is terminal and may never transition again.
    /// 返回此状态是否为不可再次转换的终态。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut | Self::Interrupted
        )
    }
}

/// Quality of Provider-reported usage evidence for one physical call.
/// 单次物理调用中 Provider 报告用量证据的质量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionProviderUsageAvailability {
    /// No usable Provider usage dimension was returned.
    /// Provider 未返回任何可用用量维度。
    #[default]
    Missing,
    /// Some usage dimensions were observed without final evidence.
    /// 已观测部分用量维度，但没有终态证据。
    Partial,
    /// Final Provider usage evidence was observed.
    /// 已观测到 Provider 终态用量证据。
    Final,
}

/// Ordered identity of one request or response projection profile.
/// 单个请求或响应投影 Profile 的有序身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderProfileRevision {
    /// Stable profile identifier.
    /// 稳定 Profile 标识。
    pub profile_id: String,
    /// Frozen profile revision.
    /// 冻结的 Profile 修订号。
    pub revision: u64,
}

/// Immutable Provider, model, route, and owner identity captured for one call.
/// 为单次调用捕获的不可变 Provider、模型、路由和归属身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderCallIdentity {
    /// Logical request identity shared by its physical attempts.
    /// 同一逻辑请求的多个物理 attempt 共享的身份。
    pub logical_request_id: String,
    /// Host execution identity.
    /// 宿主执行身份。
    pub execution_id: String,
    /// Optional canonical turn identity when the execution source supplies it.
    /// 执行来源提供时保存的可选规范回合身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Zero-based physical attempt index.
    /// 从零开始的物理 attempt 索引。
    pub attempt_index: u32,
    /// Completed transport retry count captured for this attempt.
    /// 为此次 attempt 捕获的已完成传输重试次数。
    pub retry_number: u32,
    /// Stable execution owner category.
    /// 稳定执行归属类别。
    pub owner_kind: String,
    /// Stable execution owner identity.
    /// 稳定执行归属身份。
    pub owner_id: String,
    /// Owner display name captured at call time.
    /// 调用时捕获的归属显示名称。
    pub owner_name_snapshot: String,
    /// Stable billing operation.
    /// 稳定计费操作。
    pub operation: String,
    /// Stable Provider identifier.
    /// 稳定 Provider 标识。
    pub provider_id: String,
    /// Provider display name captured at call time.
    /// 调用时捕获的 Provider 显示名称。
    pub provider_name_snapshot: String,
    /// Stable model identifier.
    /// 稳定模型标识。
    pub model_id: String,
    /// Model display name captured at call time.
    /// 调用时捕获的模型显示名称。
    pub model_name_snapshot: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    pub connection_id: String,
    /// Exact Provider service-entry identifier.
    /// 精确 Provider 服务入口标识。
    pub service_entry_id: String,
    /// Exact protocol profile identifier.
    /// 精确协议 Profile 标识。
    pub protocol_profile_id: String,
    /// Frozen Manifest revision.
    /// 冻结的 Manifest 修订号。
    pub manifest_revision: u64,
    /// Frozen process-local Manifest generation.
    /// 冻结的进程内 Manifest 代际。
    pub manifest_generation: u64,
    /// Stable source of the model protocol binding.
    /// 模型协议绑定的稳定来源。
    pub binding_source: String,
    /// Exact binding rule when one produced the route.
    /// 生成路由时使用的精确绑定规则。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_rule: Option<String>,
    /// Content-addressed frozen route revision.
    /// 内容寻址的冻结路由修订号。
    pub route_revision: String,
    /// Ordered request projection profiles.
    /// 有序请求投影 Profile。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub request_profiles: Vec<SessionProviderProfileRevision>,
    /// Ordered response projection profiles.
    /// 有序响应投影 Profile。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_profiles: Vec<SessionProviderProfileRevision>,
}

/// Provider-returned usage and extraction evidence for one physical call.
/// 单次物理调用的 Provider 返回用量及提取证据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderUsageEvidence {
    /// Evidence availability without converting missing values into zero.
    /// 不把缺失值转换为零的证据可用性。
    pub availability: SessionProviderUsageAvailability,
    /// Normalized Provider usage; character estimates are forbidden here.
    /// 规范化 Provider 用量；此处禁止写入字符估算。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_usage: Option<ProviderUsageSnapshot>,
}

/// Source of a context-budget accounting value.
/// 上下文预算记账值的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionProviderBudgetAccountingSource {
    /// The value is taken directly from Provider-reported usage.
    /// 该值直接来自 Provider 报告用量。
    ProviderReported,
    /// A matching Provider anchor is extended with a character-level delta.
    /// 匹配的 Provider 锚点叠加字符级增量。
    ProviderAnchorPlusCharacterDelta,
    /// No valid Provider anchor exists and the complete value is a character estimate.
    /// 不存在有效 Provider 锚点，完整值来自字符估算。
    CharacterEstimate,
}

/// Budget-only accounting kept separate from Provider usage evidence.
/// 与 Provider 用量证据分离保存的预算专用记账。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderBudgetAccounting {
    /// Explicit accounting source and quality.
    /// 明确的记账来源与质量。
    pub source: SessionProviderBudgetAccountingSource,
    /// Input-token value used by the budget decision.
    /// 预算决策使用的输入 Token 值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<usize>,
    /// Output-token value used by the budget decision.
    /// 预算决策使用的输出 Token 值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<usize>,
    /// Total-token value used by the budget decision.
    /// 预算决策使用的总 Token 值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<usize>,
    /// Stable warning code shown when the value is not exact Provider evidence.
    /// 值不是精确 Provider 证据时展示的稳定警告码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_code: Option<String>,
}

/// Non-sensitive size measurement of one assembled Provider request.
/// 单个已装配 Provider 请求的非敏感尺寸测量。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderRequestTextMeasurement {
    /// Unicode scalar count of retained textual input.
    /// 已保留文本输入的 Unicode 标量数量。
    pub character_count: usize,
    /// UTF-8 byte count of retained textual input.
    /// 已保留文本输入的 UTF-8 字节数。
    pub utf8_byte_count: usize,
    /// Explicit structural overhead units used by the estimator.
    /// 估算器使用的显式结构开销单位。
    pub structural_units: usize,
    /// Number of canonical history messages included in the request.
    /// 请求包含的规范历史消息数量。
    pub history_message_count: usize,
    /// Whether the latest prompt was already included in the measured canonical history.
    /// 最新提示是否已经包含在被测量的规范历史中。
    #[serde(default)]
    pub prompt_in_history: bool,
    /// Digest of the complete canonical history prefix without retaining its content.
    /// 不保留正文的完整规范历史前缀摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_fingerprint: Option<String>,
    /// Digest of request components that must remain unchanged for proven history-prefix reuse.
    /// 规范历史前缀复用时必须保持不变的请求组件摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_invariant_fingerprint: Option<String>,
    /// Optional category weights used to project a calibrated context-usage breakdown.
    /// 用于投影校准后上下文占用明细的可选分类权重。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_breakdown: Option<SessionProviderRequestContextBreakdown>,
}

/// Non-sensitive character-level weights for one Provider-visible request.
/// 单次 Provider 可见请求的非敏感字符级分类权重。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderRequestContextBreakdown {
    /// Canonical history and the distinct current prompt.
    /// 规范历史与未包含在历史中的当前提示词。
    pub message_units: usize,
    /// Host-owned and bundled tool schemas plus tool guidance.
    /// 宿主自有及随包工具 Schema 与工具指引。
    pub system_tool_units: usize,
    /// MCP-discovered tool schemas.
    /// MCP 发现的工具 Schema。
    pub mcp_tool_units: usize,
    /// LuaSkills tool schemas.
    /// LuaSkills 工具 Schema。
    pub skill_units: usize,
    /// Provider, agent, developer, personality, and output-style instructions.
    /// Provider、智能体、开发者、人格与输出风格指令。
    pub system_prompt_units: usize,
    /// Runtime context, continuation metadata, and remaining structured request data.
    /// 运行时上下文、续接元数据与其余结构化请求数据。
    pub other_units: usize,
}

impl SessionProviderRequestContextBreakdown {
    /// Returns the saturating sum used as the exact proportional denominator.
    /// 返回作为精确比例分母的饱和分类权重总和。
    pub fn total_units(&self) -> usize {
        self.message_units
            .saturating_add(self.system_tool_units)
            .saturating_add(self.mcp_tool_units)
            .saturating_add(self.skill_units)
            .saturating_add(self.system_prompt_units)
            .saturating_add(self.other_units)
    }
}

/// Complete compact record for one physical Provider call.
/// 单次物理 Provider 调用的完整紧凑记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderCallRecord {
    /// Stable physical call identifier and ledger key.
    /// 稳定物理调用标识及账本键。
    pub provider_call_id: String,
    /// Monotonic display and branch-order sequence.
    /// 用于展示与分支排序的单调序号。
    pub sequence: u64,
    /// Immutable call identity snapshot.
    /// 不可变调用身份快照。
    pub identity: SessionProviderCallIdentity,
    /// Current monotonic lifecycle state.
    /// 当前单调生命周期状态。
    pub status: SessionProviderCallStatus,
    /// Timestamp recorded before network dispatch.
    /// 网络分派前记录的时间戳。
    pub prepared_at_ms: i64,
    /// Timestamp when dispatch was proven.
    /// 分派得到证明时的时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched_at_ms: Option<i64>,
    /// Timestamp of the first transport event.
    /// 首个传输事件的时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_event_at_ms: Option<i64>,
    /// Timestamp of the first visible token.
    /// 首个可见 Token 的时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_token_at_ms: Option<i64>,
    /// Terminal timestamp.
    /// 终态时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
    /// Exact typed dispatch outcome.
    /// 精确强类型分派结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_outcome: Option<String>,
    /// Whether the dispatch outcome is proven rather than unknown.
    /// 分派结果是否得到证明而非未知。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_outcome_known: Option<bool>,
    /// Whether semantic output was observed.
    /// 是否已观测到语义输出。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_output_observed: Option<bool>,
    /// Whether semantic-output observation is a proven fact.
    /// 语义输出观测是否为已证明事实。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_output_observed_known: Option<bool>,
    /// Whether replay was proven safe.
    /// 是否已证明可以安全重放。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_safe: Option<bool>,
    /// Stable redacted error category.
    /// 稳定脱敏错误类别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_category: Option<String>,
    /// Stable redacted error code.
    /// 稳定脱敏错误代码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Provider-only usage evidence.
    /// 仅来自 Provider 的用量证据。
    pub provider_usage_evidence: SessionProviderUsageEvidence,
    /// Request assembly fingerprint without prompt content.
    /// 不含提示词正文的请求装配指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_assembly_fingerprint: Option<String>,
    /// Context generation that owned this request.
    /// 此请求所属的上下文代际。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_generation_id: Option<String>,
    /// Canonical context start record identity.
    /// 规范上下文起始记录身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_start_record_id: Option<String>,
    /// Canonical closing record identity.
    /// 规范闭合记录身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closing_record_id: Option<String>,
    /// Frozen model-route fingerprint.
    /// 冻结模型路由指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_route_fingerprint: Option<String>,
    /// Frozen toolset fingerprint.
    /// 冻结工具集指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolset_fingerprint: Option<String>,
    /// Frozen system-prompt fingerprint.
    /// 冻结系统提示词指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_fingerprint: Option<String>,
    /// Optional non-sensitive request measurement.
    /// 可选非敏感请求测量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_text_measurement: Option<SessionProviderRequestTextMeasurement>,
    /// Optional budget-only accounting, never Provider evidence.
    /// 可选预算专用记账，绝不作为 Provider 证据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_accounting: Option<SessionProviderBudgetAccounting>,
    /// Branch generation owning this record.
    /// 此记录所属的分支代际。
    pub branch_generation: u64,
    /// Whether this call belongs to the active branch.
    /// 此调用是否属于活动分支。
    pub active_branch_member: bool,
    /// Edit generation that superseded this call.
    /// 使此调用失效的编辑代际。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by_edit_generation: Option<u64>,
    /// Whether this record was inherited by a fork.
    /// 此记录是否由分叉继承。
    #[serde(default, skip_serializing_if = "is_false")]
    pub inherited_from_fork: bool,
    /// Original call identity when a fork rewrote the key.
    /// 分叉改写键时的原始调用身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_call_id: Option<String>,
}

/// Immutable facts required to create one prepared physical call record.
/// 创建一条已准备物理调用记录所需的不可变事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProviderCallStarted {
    /// Stable physical call identifier.
    /// 稳定物理调用标识。
    pub provider_call_id: String,
    /// Immutable call identity snapshot.
    /// 不可变调用身份快照。
    pub identity: SessionProviderCallIdentity,
    /// Timestamp captured before dispatch.
    /// 分派前捕获的时间戳。
    pub prepared_at_ms: i64,
    /// Request assembly fingerprint without retaining request content.
    /// 不保留请求正文的请求装配指纹。
    pub request_assembly_fingerprint: Option<String>,
    /// Context generation that owns this request.
    /// 拥有此请求的上下文代际。
    pub context_generation_id: Option<String>,
    /// Canonical context start record identity frozen before dispatch.
    /// 分派前冻结的规范上下文起始记录身份。
    pub context_start_record_id: Option<String>,
    /// Frozen model-route fingerprint.
    /// 冻结模型路由指纹。
    pub model_route_fingerprint: Option<String>,
    /// Frozen toolset fingerprint.
    /// 冻结工具集指纹。
    pub toolset_fingerprint: Option<String>,
    /// Frozen system-prompt fingerprint.
    /// 冻结系统提示词指纹。
    pub system_prompt_fingerprint: Option<String>,
    /// Non-sensitive measurement of the exact assembled request.
    /// 精确装配请求的非敏感尺寸测量。
    pub request_text_measurement: Option<SessionProviderRequestTextMeasurement>,
    /// Budget accounting that admitted this request.
    /// 准入当前请求的预算记账。
    pub budget_accounting: Option<SessionProviderBudgetAccounting>,
}

/// Terminal facts required to close one physical call record.
/// 闭合一条物理调用记录所需的终态事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProviderCallCompleted {
    /// Stable physical call identifier.
    /// 稳定物理调用标识。
    pub provider_call_id: String,
    /// Immutable identity used only when a start record was not observed.
    /// 仅在未观测到开始记录时使用的不可变身份。
    pub identity: SessionProviderCallIdentity,
    /// Exact terminal state.
    /// 精确终态。
    pub status: SessionProviderCallStatus,
    /// Provider-only usage evidence.
    /// 仅来自 Provider 的用量证据。
    pub provider_usage_evidence: SessionProviderUsageEvidence,
    /// Timestamp captured before dispatch.
    /// 分派前捕获的时间戳。
    pub prepared_at_ms: i64,
    /// Timestamp when dispatch was proven.
    /// 分派得到证明时的时间戳。
    pub dispatched_at_ms: Option<i64>,
    /// First transport event timestamp.
    /// 首个传输事件时间戳。
    pub first_event_at_ms: Option<i64>,
    /// First visible token timestamp.
    /// 首个可见 Token 时间戳。
    pub first_token_at_ms: Option<i64>,
    /// Terminal timestamp.
    /// 终态时间戳。
    pub completed_at_ms: i64,
    /// Exact typed dispatch outcome.
    /// 精确强类型分派结果。
    pub dispatch_outcome: Option<String>,
    /// Whether dispatch is proven.
    /// 分派是否得到证明。
    pub dispatch_outcome_known: Option<bool>,
    /// Whether semantic output was observed.
    /// 是否观测到语义输出。
    pub semantic_output_observed: Option<bool>,
    /// Whether semantic-output observation is proven.
    /// 语义输出观测是否得到证明。
    pub semantic_output_observed_known: Option<bool>,
    /// Whether replay is proven safe.
    /// 是否证明可以安全重放。
    pub replay_safe: Option<bool>,
    /// Stable redacted error category.
    /// 稳定脱敏错误类别。
    pub error_category: Option<String>,
    /// Stable redacted error code.
    /// 稳定脱敏错误代码。
    pub error_code: Option<String>,
}

/// Stable pointer to the latest Provider baseline usable by context accounting.
/// 指向上下文记账可用最近 Provider 基线的稳定指针。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderContextBaselineRef {
    /// Physical Provider call identity.
    /// 物理 Provider 调用身份。
    pub provider_call_id: String,
    /// Ledger revision at which the pointer was selected.
    /// 选择此指针时的账本修订号。
    pub ledger_revision: u64,
}

/// Durable eligibility cache for problem-turn recovery admission.
/// 问题回合恢复准入的可靠资格缓存。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContinuationState {
    /// Monotonic continuation generation.
    /// 单调继续代际。
    pub generation: u64,
    /// Cached eligibility; callers must revalidate its source revision.
    /// 缓存资格；调用方必须重新验证其来源修订。
    pub eligible: bool,
    /// Execution that produced the terminal boundary.
    /// 产生终止边界的执行身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_execution_id: Option<String>,
    /// Stable terminal reason.
    /// 稳定终止原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
    /// Terminal boundary timestamp.
    /// 终止边界时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_at_ms: Option<i64>,
    /// Canonical transcript tail kind.
    /// 规范转录尾部类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_tail_kind: Option<String>,
    /// Last stable call used by continuation admission.
    /// Continue 准入使用的最近稳定调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_stable_provider_call_id: Option<String>,
    /// Ledger revision expected by this cache.
    /// 此缓存预期的账本修订号。
    pub expected_ledger_revision: u64,
    /// Claim timestamp while continuation is active.
    /// Continue 活跃时的领取时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at_ms: Option<i64>,
    /// Execution that claimed this continuation generation.
    /// 领取此 Continue 代际的执行身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_execution_id: Option<String>,
    /// User submission that invalidated eligibility.
    /// 使资格失效的用户提交身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalidated_by_submission_id: Option<String>,
    /// Eligibility invalidation timestamp.
    /// 资格失效时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalidated_at_ms: Option<i64>,
}

impl SessionContinuationState {
    /// Returns whether no continuation lifecycle has been materialized.
    /// 返回是否尚未物化任何 Continue 生命周期。
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// Inclusive child-agent usage contribution retained by the parent session.
/// 父会话保留的子代理包容性用量贡献。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionChildUsageContribution {
    /// Stable child task identity.
    /// 稳定子任务身份。
    pub task_id: String,
    /// Stable child execution identity.
    /// 稳定子执行身份。
    pub execution_id: String,
    /// Parent turn receiving the contribution.
    /// 接收贡献的父回合。
    pub parent_turn_id: String,
    /// Monotonic child contribution revision.
    /// 单调子贡献修订号。
    pub revision: u64,
    /// Whether this contribution is partial or final.
    /// 此贡献是部分值还是终值。
    pub status: String,
    /// Inclusive usage of the child and all descendants.
    /// 子代理及其全部后代的包容性用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inclusive_usage: Option<ProviderUsageSnapshot>,
    /// Physical Provider call count represented by the contribution.
    /// 此贡献代表的物理 Provider 调用数量。
    pub provider_call_count: usize,
    /// Calls without usable Provider usage.
    /// 没有可用 Provider 用量的调用数量。
    pub unknown_usage_call_count: usize,
    /// Turn Stats record that produced this contribution.
    /// 产生此贡献的 Turn Stats 记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_turn_stats_record_id: Option<String>,
}

/// One-time import facts retained after a legacy session usage state is upgraded.
/// 旧会话用量状态升级后保留的一次性导入事实。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderLegacyImport {
    /// Number of bounded legacy segments imported.
    /// 导入的有界旧分段数量。
    pub imported_segment_count: usize,
    /// Whether older aggregate-only usage existed outside the ledger.
    /// 账本外是否存在更旧的仅聚合用量。
    pub aggregate_only_usage_observed: bool,
    /// Legacy physical-call count retained only as an aggregate audit fact.
    /// 仅作为聚合审计事实保留的旧物理调用数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_call_count: Option<usize>,
    /// Legacy open-call count retained without fabricating missing call records.
    /// 在不虚构缺失调用记录的前提下保留的旧未闭合调用数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_flight_call_count: Option<usize>,
    /// Legacy unknown-usage count retained as aggregate-only evidence.
    /// 作为仅聚合证据保留的旧未知用量调用数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_usage_call_count: Option<usize>,
    /// Legacy usage revision retained for migration diagnostics.
    /// 为迁移诊断保留的旧用量修订号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_revision: Option<u64>,
    /// Number of legacy seen call identifiers, whose missing records are never reconstructed.
    /// 旧版已见调用标识数量；缺失记录绝不据此重建。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_provider_call_id_count: Option<usize>,
    /// Latest legacy usage snapshot retained for display and diagnostics only.
    /// 仅为展示与诊断保留的旧版最近用量快照。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_usage: Option<ProviderUsageSnapshot>,
    /// Latest successful legacy usage retained without claiming fingerprint alignment.
    /// 在不宣称指纹对齐的前提下保留的旧版最近成功用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_usage: Option<ProviderUsageSnapshot>,
    /// Legacy cumulative usage retained as one indivisible aggregate fact.
    /// 作为不可拆分聚合事实保留的旧版累计用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cumulative_usage: Option<ProviderUsageSnapshot>,
}

/// Versioned authoritative ledger for every physical Provider call in one session.
/// 单个会话中每次物理 Provider 调用的版本化权威账本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionProviderCallLedger {
    /// Fail-closed ledger schema version.
    /// 封闭失败的账本 Schema 版本。
    pub schema_version: u32,
    /// Monotonic revision updated by every accepted mutation.
    /// 每次接受变化时更新的单调修订号。
    pub revision: u64,
    /// Next sequence assigned to a new physical call.
    /// 分配给下一条物理调用的序号。
    pub next_sequence: u64,
    /// Current context generation identity.
    /// 当前上下文代际身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_context_generation_id: Option<String>,
    /// Current branch generation assigned to newly dispatched calls.
    /// 分配给新分派调用的当前分支代际。
    #[serde(default)]
    pub active_branch_generation: u64,
    /// Calls keyed by their stable physical call identity.
    /// 按稳定物理调用身份索引的调用。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub calls: BTreeMap<String, SessionProviderCallRecord>,
    /// Latest usable context baseline pointer.
    /// 最近可用上下文基线指针。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_stable_context_baseline: Option<SessionProviderContextBaselineRef>,
    /// Problem-turn recovery admission lifecycle.
    /// 问题回合恢复准入生命周期。
    #[serde(default, skip_serializing_if = "SessionContinuationState::is_empty")]
    pub continuation: SessionContinuationState,
    /// Child-agent contributions keyed by task and execution identity.
    /// 按任务与执行身份索引的子代理贡献。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub child_contributions: BTreeMap<String, SessionChildUsageContribution>,
    /// One-time legacy import audit facts.
    /// 一次性旧状态导入审计事实。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_import: Option<SessionProviderLegacyImport>,
}

impl Default for SessionProviderCallLedger {
    /// Builds an empty current-version ledger.
    /// 构建一个当前版本的空账本。
    fn default() -> Self {
        Self {
            schema_version: SESSION_PROVIDER_CALL_LEDGER_SCHEMA_VERSION,
            revision: 0,
            next_sequence: 1,
            active_context_generation_id: None,
            active_branch_generation: 0,
            calls: BTreeMap::new(),
            last_stable_context_baseline: None,
            continuation: SessionContinuationState::default(),
            child_contributions: BTreeMap::new(),
            legacy_import: None,
        }
    }
}

impl SessionProviderCallLedger {
    /// Returns whether the ledger contains no observable runtime facts.
    /// 返回账本是否不包含任何可观察运行事实。
    pub fn is_empty(&self) -> bool {
        self.revision == 0
            && self.calls.is_empty()
            && self.active_context_generation_id.is_none()
            && self.active_branch_generation == 0
            && self.last_stable_context_baseline.is_none()
            && self.continuation.is_empty()
            && self.child_contributions.is_empty()
            && self.legacy_import.is_none()
    }

    /// Validates all fail-closed schema, identity, ordering, and lifecycle invariants.
    /// 校验全部封闭失败的 Schema、身份、排序和生命周期不变量。
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SESSION_PROVIDER_CALL_LEDGER_SCHEMA_VERSION {
            return Err(format!(
                "unsupported session Provider call ledger schema version {}",
                self.schema_version
            ));
        }
        let mut maximum_sequence = 0_u64;
        let mut observed_sequences = BTreeSet::new();
        for (call_id, record) in &self.calls {
            if call_id.trim().is_empty() || record.provider_call_id != *call_id {
                return Err(String::from(
                    "session Provider call ledger contains an invalid call identity",
                ));
            }
            if record.sequence == 0 {
                return Err(format!("Provider call `{call_id}` has sequence zero"));
            }
            if !observed_sequences.insert(record.sequence) {
                return Err(format!(
                    "Provider call `{call_id}` reuses sequence {}",
                    record.sequence
                ));
            }
            maximum_sequence = maximum_sequence.max(record.sequence);
            if record.status.is_terminal() != record.completed_at_ms.is_some() {
                return Err(format!(
                    "Provider call `{call_id}` has inconsistent terminal timing"
                ));
            }
        }
        if self.next_sequence <= maximum_sequence {
            return Err(String::from(
                "session Provider call ledger next sequence is not monotonic",
            ));
        }
        if let Some(baseline) = self.last_stable_context_baseline.as_ref()
            && !self.calls.contains_key(&baseline.provider_call_id)
        {
            return Err(String::from(
                "session Provider call ledger baseline references a missing call",
            ));
        }
        for (key, contribution) in &self.child_contributions {
            if *key != format!("{}:{}", contribution.task_id, contribution.execution_id) {
                return Err(format!(
                    "session child contribution `{key}` has an inconsistent identity"
                ));
            }
        }
        Ok(())
    }

    /// Imports top-level legacy usage summaries without turning aggregates into physical calls.
    /// 导入顶层旧版用量摘要，但不会把聚合数据伪造成物理调用。
    ///
    /// # Parameters
    /// 参数
    /// `last_usage` is the legacy latest usage snapshot.
    /// `last_usage` 是旧版最近用量快照。
    /// `last_successful_usage` is the legacy latest successful usage snapshot.
    /// `last_successful_usage` 是旧版最近成功用量快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns whether the ledger changed, or a conflict/exhaustion error.
    /// 返回账本是否变化，或冲突/计数耗尽错误。
    pub fn import_legacy_runtime_usage_summaries(
        &mut self,
        last_usage: Option<ProviderUsageSnapshot>,
        last_successful_usage: Option<ProviderUsageSnapshot>,
    ) -> Result<bool, String> {
        if last_usage.is_none() && last_successful_usage.is_none() {
            return Ok(false);
        }
        let current_last_usage = self
            .legacy_import
            .as_ref()
            .and_then(|legacy_import| legacy_import.last_usage.as_ref());
        let current_last_successful_usage = self
            .legacy_import
            .as_ref()
            .and_then(|legacy_import| legacy_import.last_successful_usage.as_ref());
        let merged_last_usage = merge_legacy_provider_usage_snapshot(
            current_last_usage,
            last_usage.as_ref(),
            "latest",
        )?;
        let merged_last_successful_usage = merge_legacy_provider_usage_snapshot(
            current_last_successful_usage,
            last_successful_usage.as_ref(),
            "successful",
        )?;
        let aggregate_only_usage_observed = self
            .legacy_import
            .as_ref()
            .is_some_and(|legacy_import| legacy_import.aggregate_only_usage_observed);
        let changed = current_last_usage != merged_last_usage.as_ref()
            || current_last_successful_usage != merged_last_successful_usage.as_ref()
            || !aggregate_only_usage_observed;
        if !changed {
            return Ok(false);
        }
        if self.revision == u64::MAX {
            return Err(String::from(
                "cannot import legacy Provider usage summaries after ledger revision exhaustion",
            ));
        }
        let legacy_import = self
            .legacy_import
            .get_or_insert_with(SessionProviderLegacyImport::default);
        legacy_import.last_usage = merged_last_usage;
        legacy_import.last_successful_usage = merged_last_successful_usage;
        legacy_import.aggregate_only_usage_observed = true;
        self.revision += 1;
        Ok(true)
    }

    /// Records one physical Provider request before dispatch exactly once.
    /// 在分派前按幂等方式记录一次物理 Provider 请求。
    pub fn observe_started(&mut self, started: SessionProviderCallStarted) -> bool {
        if started.provider_call_id.trim().is_empty()
            || self.calls.contains_key(&started.provider_call_id)
            || self.next_sequence == u64::MAX
            || self.revision == u64::MAX
        {
            return false;
        }
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.revision += 1;
        let call_id = started.provider_call_id.clone();
        self.calls.insert(
            call_id,
            SessionProviderCallRecord {
                provider_call_id: started.provider_call_id,
                sequence,
                identity: started.identity,
                status: SessionProviderCallStatus::Prepared,
                prepared_at_ms: started.prepared_at_ms,
                dispatched_at_ms: None,
                first_event_at_ms: None,
                first_token_at_ms: None,
                completed_at_ms: None,
                dispatch_outcome: None,
                dispatch_outcome_known: None,
                semantic_output_observed: None,
                semantic_output_observed_known: None,
                replay_safe: None,
                error_category: None,
                error_code: None,
                provider_usage_evidence: SessionProviderUsageEvidence::default(),
                request_assembly_fingerprint: started.request_assembly_fingerprint,
                context_generation_id: started
                    .context_generation_id
                    .or_else(|| self.active_context_generation_id.clone()),
                context_start_record_id: started.context_start_record_id,
                closing_record_id: None,
                model_route_fingerprint: started.model_route_fingerprint,
                toolset_fingerprint: started.toolset_fingerprint,
                system_prompt_fingerprint: started.system_prompt_fingerprint,
                request_text_measurement: started.request_text_measurement,
                budget_accounting: started.budget_accounting,
                branch_generation: self.active_branch_generation,
                active_branch_member: true,
                superseded_by_edit_generation: None,
                inherited_from_fork: false,
                origin_call_id: None,
            },
        );
        true
    }

    /// Checks whether a repeated started event carries the same immutable facts as its retained call.
    /// 检查重复的开始事件是否与已保留调用携带相同的不可变事实。
    ///
    /// # Parameters
    /// 参数
    /// `started` is the repeated pre-dispatch observation to compare with the ledger record.
    /// `started` 是待与账本记录比较的重复分派前观测。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when the call exists and every supplied immutable fact matches exactly.
    /// 仅当调用存在且所有已提供不可变事实完全一致时返回 `true`。
    pub fn started_event_is_idempotent(&self, started: &SessionProviderCallStarted) -> bool {
        self.calls
            .get(started.provider_call_id.as_str())
            .is_some_and(|record| {
                record.identity == started.identity
                    && record.prepared_at_ms == started.prepared_at_ms
                    && record.request_assembly_fingerprint == started.request_assembly_fingerprint
                    && started
                        .context_generation_id
                        .as_ref()
                        .is_none_or(|generation_id| {
                            record.context_generation_id.as_ref() == Some(generation_id)
                        })
                    && record.context_start_record_id == started.context_start_record_id
                    && record.model_route_fingerprint == started.model_route_fingerprint
                    && record.toolset_fingerprint == started.toolset_fingerprint
                    && record.system_prompt_fingerprint == started.system_prompt_fingerprint
                    && record.request_text_measurement == started.request_text_measurement
                    && record.budget_accounting == started.budget_accounting
            })
    }

    /// Records a first-event or first-token timing update idempotently.
    /// 按幂等方式记录首事件或首 Token 时间更新。
    pub fn observe_first_event(
        &mut self,
        provider_call_id: &str,
        first_event_at_ms: Option<i64>,
        first_token_at_ms: Option<i64>,
    ) -> bool {
        if self.revision == u64::MAX {
            return false;
        }
        let Some(record) = self.calls.get_mut(provider_call_id) else {
            return false;
        };
        if record.status.is_terminal() {
            return false;
        }
        let mut changed = false;
        if record.first_event_at_ms.is_none() && first_event_at_ms.is_some() {
            record.first_event_at_ms = first_event_at_ms;
            changed = true;
        }
        if record.first_token_at_ms.is_none() && first_token_at_ms.is_some() {
            record.first_token_at_ms = first_token_at_ms;
            changed = true;
        }
        if changed {
            self.revision += 1;
        }
        changed
    }

    /// Advances one prepared call to the durably observed dispatched state.
    /// 将一条 prepared 调用推进到已持久观测的 dispatched 状态。
    pub fn observe_dispatched(&mut self, provider_call_id: &str, dispatched_at_ms: i64) -> bool {
        if self.revision == u64::MAX {
            return false;
        }
        let Some(record) = self.calls.get_mut(provider_call_id) else {
            return false;
        };
        if record.status != SessionProviderCallStatus::Prepared {
            return false;
        }
        record.status = SessionProviderCallStatus::Dispatched;
        record.dispatched_at_ms = Some(dispatched_at_ms);
        record.dispatch_outcome = Some(String::from("dispatched"));
        record.dispatch_outcome_known = Some(true);
        self.revision += 1;
        true
    }

    /// Closes one physical Provider request exactly once and never regresses a terminal state.
    /// 按幂等方式闭合一次物理 Provider 请求，且绝不回退终态。
    pub fn observe_completed(&mut self, completed: SessionProviderCallCompleted) -> bool {
        let needs_start_record = !self.calls.contains_key(&completed.provider_call_id);
        if completed.provider_call_id.trim().is_empty()
            || !completed.status.is_terminal()
            || self.revision == u64::MAX
            || (needs_start_record && self.revision == u64::MAX - 1)
        {
            return false;
        }
        if needs_start_record {
            let started = SessionProviderCallStarted {
                provider_call_id: completed.provider_call_id.clone(),
                identity: completed.identity.clone(),
                prepared_at_ms: completed.prepared_at_ms,
                request_assembly_fingerprint: None,
                context_generation_id: self.active_context_generation_id.clone(),
                context_start_record_id: None,
                model_route_fingerprint: None,
                toolset_fingerprint: None,
                system_prompt_fingerprint: None,
                request_text_measurement: None,
                budget_accounting: None,
            };
            let _ = self.observe_started(started);
        }
        let Some(record) = self.calls.get_mut(&completed.provider_call_id) else {
            return false;
        };
        if record.status.is_terminal() {
            return false;
        }
        record.status = completed.status;
        record.dispatched_at_ms = completed.dispatched_at_ms;
        record.first_event_at_ms = completed.first_event_at_ms;
        record.first_token_at_ms = completed.first_token_at_ms;
        record.completed_at_ms = Some(completed.completed_at_ms);
        record.dispatch_outcome = completed.dispatch_outcome;
        record.dispatch_outcome_known = completed.dispatch_outcome_known;
        record.semantic_output_observed = completed.semantic_output_observed;
        record.semantic_output_observed_known = completed.semantic_output_observed_known;
        record.replay_safe = completed.replay_safe;
        record.error_category = completed.error_category;
        record.error_code = completed.error_code;
        record.provider_usage_evidence = completed.provider_usage_evidence;
        self.revision += 1;
        if record.active_branch_member
            && record.superseded_by_edit_generation.is_none()
            && record.provider_usage_evidence.availability
                != SessionProviderUsageAvailability::Missing
            && record
                .provider_usage_evidence
                .provider_usage
                .as_ref()
                .and_then(provider_input_tokens)
                .is_some()
        {
            self.last_stable_context_baseline = Some(SessionProviderContextBaselineRef {
                provider_call_id: record.provider_call_id.clone(),
                ledger_revision: self.revision,
            });
        }
        true
    }

    /// Checks whether a repeated completed event exactly matches the retained terminal facts.
    /// 检查重复的完成事件是否与已保留终态事实完全一致。
    ///
    /// # Parameters
    /// 参数
    /// `completed` is the repeated terminal observation to compare with the ledger record.
    /// `completed` 是待与账本记录比较的重复终态观测。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when the retained call is terminal and all terminal and identity facts match.
    /// 仅当已保留调用处于终态且全部终态与身份事实一致时返回 `true`。
    pub fn completed_event_is_idempotent(&self, completed: &SessionProviderCallCompleted) -> bool {
        self.calls
            .get(completed.provider_call_id.as_str())
            .is_some_and(|record| {
                record.status.is_terminal()
                    && record.identity == completed.identity
                    && record.status == completed.status
                    && record.provider_usage_evidence == completed.provider_usage_evidence
                    && record.prepared_at_ms == completed.prepared_at_ms
                    && record.dispatched_at_ms == completed.dispatched_at_ms
                    && record.first_event_at_ms == completed.first_event_at_ms
                    && record.first_token_at_ms == completed.first_token_at_ms
                    && record.completed_at_ms == Some(completed.completed_at_ms)
                    && record.dispatch_outcome == completed.dispatch_outcome
                    && record.dispatch_outcome_known == completed.dispatch_outcome_known
                    && record.semantic_output_observed == completed.semantic_output_observed
                    && record.semantic_output_observed_known
                        == completed.semantic_output_observed_known
                    && record.replay_safe == completed.replay_safe
                    && record.error_category == completed.error_category
                    && record.error_code == completed.error_code
            })
    }

    /// Returns all calls in deterministic sequence order.
    /// 按确定性的序号顺序返回全部调用。
    pub fn calls_in_sequence(&self) -> Vec<&SessionProviderCallRecord> {
        let mut calls = self.calls.values().collect::<Vec<_>>();
        calls.sort_by_key(|record| record.sequence);
        calls
    }

    /// Returns active-branch calls in deterministic sequence order.
    /// 按确定性序号顺序返回活动分支调用。
    pub fn active_calls_in_sequence(&self) -> Vec<&SessionProviderCallRecord> {
        self.calls_in_sequence()
            .into_iter()
            .filter(|record| {
                record.active_branch_member && record.superseded_by_edit_generation.is_none()
            })
            .collect()
    }

    /// Returns the number of all retained physical calls.
    /// 返回全部保留物理调用的数量。
    pub fn provider_call_count(&self) -> usize {
        self.calls.len()
    }

    /// Returns the number of calls that have not reached a terminal state.
    /// 返回尚未到达终态的调用数量。
    pub fn in_flight_call_count(&self) -> usize {
        self.calls
            .values()
            .filter(|record| !record.status.is_terminal())
            .count()
    }

    /// Returns the number of terminal calls without usable Provider usage.
    /// 返回没有可用 Provider 用量的终态调用数量。
    pub fn unknown_usage_call_count(&self) -> usize {
        self.calls
            .values()
            .filter(|record| {
                record.status.is_terminal()
                    && record.provider_usage_evidence.availability
                        == SessionProviderUsageAvailability::Missing
            })
            .count()
    }

    /// Returns the latest active-branch Provider usage without fabricating missing values.
    /// 返回活动分支最近 Provider 用量且不伪造缺失值。
    pub fn last_usage(&self) -> Option<ProviderUsageSnapshot> {
        self.active_calls_in_sequence()
            .into_iter()
            .rev()
            .find_map(|record| {
                record
                    .provider_usage_evidence
                    .provider_usage
                    .as_ref()
                    .filter(|usage| usage_has_any_value(usage))
                    .cloned()
            })
    }

    /// Dynamically aggregates active-branch Provider usage from raw call evidence.
    /// 从原始调用证据动态聚合活动分支 Provider 用量。
    pub fn cumulative_usage(&self) -> Option<ProviderUsageSnapshot> {
        let mut cumulative = ProviderUsageSnapshot::default();
        let mut observed = false;
        for record in self.active_calls_in_sequence() {
            let Some(usage) = record
                .provider_usage_evidence
                .provider_usage
                .as_ref()
                .filter(|usage| usage_has_any_value(usage))
            else {
                continue;
            };
            observed = true;
            add_optional(&mut cumulative.input_tokens, usage.input_tokens);
            add_optional(
                &mut cumulative.uncached_input_tokens,
                usage.uncached_input_tokens,
            );
            add_optional(&mut cumulative.output_tokens, usage.output_tokens);
            add_optional(&mut cumulative.reasoning_tokens, usage.reasoning_tokens);
            add_optional(&mut cumulative.total_tokens, usage.total_tokens);
            add_optional(
                &mut cumulative.cached_input_tokens,
                usage.cached_input_tokens,
            );
            add_optional(
                &mut cumulative.cache_creation_input_tokens,
                usage.cache_creation_input_tokens,
            );
            add_optional(&mut cumulative.audio_input_tokens, usage.audio_input_tokens);
            add_optional(
                &mut cumulative.audio_output_tokens,
                usage.audio_output_tokens,
            );
        }
        observed.then_some(cumulative)
    }

    /// Marks active calls after a sequence as superseded by one edit generation.
    /// 将指定序号后的活动调用标记为被某个编辑代际取代。
    pub fn supersede_after_sequence(&mut self, sequence: u64, edit_generation: u64) -> usize {
        if self.revision == u64::MAX {
            return 0;
        }
        let mut changed = 0_usize;
        for record in self.calls.values_mut() {
            if record.sequence > sequence
                && record.active_branch_member
                && record.superseded_by_edit_generation.is_none()
            {
                record.active_branch_member = false;
                record.superseded_by_edit_generation = Some(edit_generation);
                changed = changed.saturating_add(1);
            }
        }
        if changed > 0 {
            self.active_branch_generation = edit_generation;
            self.revision += 1;
            self.last_stable_context_baseline = self.latest_stable_baseline_ref();
        }
        changed
    }

    /// Advances one edit generation and supersedes active calls whose canonical closing record is outside its boundary.
    /// 推进一次编辑代际，并将规范闭合记录位于其边界之外的活动调用标记为已取代。
    ///
    /// `retained_record_ids` is the exact surviving canonical prefix, `legacy_cutoff_at_ms` supports
    /// records created before closing identities existed, `edit_generation` is the next branch,
    /// and `context_generation_id` invalidates every pre-edit Provider budget anchor.
    /// `retained_record_ids` 是精确保留的规范前缀，`legacy_cutoff_at_ms` 兼容尚无闭合身份的旧记录，
    /// `edit_generation` 是下一分支，`context_generation_id` 用于使全部编辑前 Provider 预算锚点失效。
    pub fn supersede_outside_record_boundary(
        &mut self,
        retained_record_ids: &BTreeSet<String>,
        legacy_cutoff_at_ms: i64,
        edit_generation: u64,
        context_generation_id: String,
    ) -> usize {
        if self.revision == u64::MAX
            || edit_generation <= self.active_branch_generation
            || context_generation_id.trim().is_empty()
        {
            return 0;
        }
        let mut changed = 0_usize;
        for record in self.calls.values_mut() {
            let retained = record
                .closing_record_id
                .as_ref()
                .map(|record_id| retained_record_ids.contains(record_id))
                .unwrap_or_else(|| record.prepared_at_ms < legacy_cutoff_at_ms);
            if !retained
                && record.active_branch_member
                && record.superseded_by_edit_generation.is_none()
            {
                record.active_branch_member = false;
                record.superseded_by_edit_generation = Some(edit_generation);
                changed = changed.saturating_add(1);
            }
        }
        // Every accepted edit creates a distinct branch even when no historical call lies after its anchor.
        // 每次已接受编辑都会创建独立分支，即使锚点之后没有历史调用。
        self.active_branch_generation = edit_generation;
        self.active_context_generation_id = Some(context_generation_id);
        self.revision += 1;
        self.last_stable_context_baseline = None;
        changed
    }

    /// Assigns one canonical closing record to every call in the latest active execution.
    /// 为最近一次活动执行中的每条调用分配同一个规范闭合记录。
    pub fn assign_closing_record_id_to_latest_execution(
        &mut self,
        closing_record_id: String,
    ) -> usize {
        self.assign_closing_record_id_to_latest_execution_for_owner(closing_record_id, None)
    }

    /// Assigns a canonical closing record to the latest active execution for one exact owner kind.
    /// 为一个精确归属类型下最近的活动执行分配规范闭合记录。
    pub fn assign_closing_record_id_to_latest_execution_for_owner(
        &mut self,
        closing_record_id: String,
        owner_kind: Option<&str>,
    ) -> usize {
        if closing_record_id.trim().is_empty() || self.revision == u64::MAX {
            return 0;
        }
        let Some(execution_id) = self
            .active_calls_in_sequence()
            .into_iter()
            .rev()
            .filter(|record| {
                owner_kind.is_none_or(|owner_kind| record.identity.owner_kind == owner_kind)
            })
            .map(|record| record.identity.execution_id.clone())
            .next()
        else {
            return 0;
        };
        let mut changed = 0_usize;
        for record in self.calls.values_mut() {
            if record.active_branch_member
                && record.identity.execution_id == execution_id
                && owner_kind.is_none_or(|owner_kind| record.identity.owner_kind == owner_kind)
                && record.closing_record_id.is_none()
            {
                record.closing_record_id = Some(closing_record_id.clone());
                changed = changed.saturating_add(1);
            }
        }
        if changed > 0 {
            self.revision += 1;
        }
        changed
    }

    /// Advances the context generation and invalidates the previous request baseline.
    /// 推进上下文代际并使旧请求基线失效。
    pub fn advance_context_generation(&mut self, context_generation_id: String) -> bool {
        if context_generation_id.trim().is_empty()
            || self.active_context_generation_id.as_deref() == Some(context_generation_id.as_str())
            || self.revision == u64::MAX
        {
            return false;
        }
        self.active_context_generation_id = Some(context_generation_id);
        self.last_stable_context_baseline = None;
        self.revision += 1;
        true
    }

    /// Converts a copied source ledger into the inherited prefix owned by a forked session.
    /// 将复制的源账本转换为分叉会话拥有的继承前缀。
    pub fn retain_inherited_through_timestamp(
        &mut self,
        cutoff_at_ms: i64,
        truncate_child_contributions: bool,
    ) -> bool {
        if self.revision == u64::MAX || self.active_branch_generation == u64::MAX {
            return false;
        }
        let retained_maximum_sequence = self
            .calls
            .values()
            .filter(|record| record.prepared_at_ms <= cutoff_at_ms)
            .map(|record| record.sequence)
            .max()
            .unwrap_or(0);
        if retained_maximum_sequence == u64::MAX {
            return false;
        }
        self.calls
            .retain(|_, record| record.prepared_at_ms <= cutoff_at_ms);
        for record in self.calls.values_mut() {
            record.inherited_from_fork = true;
            record.active_branch_member = true;
            record.superseded_by_edit_generation = None;
        }
        if truncate_child_contributions {
            self.child_contributions.clear();
        }
        self.active_branch_generation += 1;
        self.next_sequence = retained_maximum_sequence + 1;
        self.continuation = SessionContinuationState::default();
        self.revision += 1;
        self.last_stable_context_baseline = self.latest_stable_baseline_ref();
        true
    }

    /// Retains fork-inherited calls through a canonical record boundary.
    /// 保留截至规范记录边界的分叉继承调用。
    ///
    /// `retained_record_ids` and `legacy_cutoff_at_ms` define the inherited prefix,
    /// `truncate_child_contributions` controls child aggregates, and `context_generation_id`
    /// establishes the target-owned generation that invalidates source Provider anchors.
    /// `retained_record_ids` 与 `legacy_cutoff_at_ms` 定义继承前缀，
    /// `truncate_child_contributions` 控制子代理聚合，`context_generation_id`
    /// 建立目标会话拥有的新代际并使源 Provider 锚点失效。
    pub fn retain_inherited_through_record_boundary(
        &mut self,
        retained_record_ids: &BTreeSet<String>,
        legacy_cutoff_at_ms: i64,
        truncate_child_contributions: bool,
        context_generation_id: String,
    ) -> bool {
        if self.revision == u64::MAX
            || self.active_branch_generation == u64::MAX
            || context_generation_id.trim().is_empty()
        {
            return false;
        }
        let retained_maximum_sequence = self
            .calls
            .values()
            .filter(|record| {
                record
                    .closing_record_id
                    .as_ref()
                    .map(|record_id| retained_record_ids.contains(record_id))
                    .unwrap_or_else(|| record.prepared_at_ms < legacy_cutoff_at_ms)
            })
            .map(|record| record.sequence)
            .max()
            .unwrap_or(0);
        if retained_maximum_sequence == u64::MAX {
            return false;
        }
        self.calls.retain(|_, record| {
            record
                .closing_record_id
                .as_ref()
                .map(|record_id| retained_record_ids.contains(record_id))
                .unwrap_or_else(|| record.prepared_at_ms < legacy_cutoff_at_ms)
        });
        for record in self.calls.values_mut() {
            record.inherited_from_fork = true;
            record.active_branch_member = true;
            record.superseded_by_edit_generation = None;
        }
        if truncate_child_contributions {
            self.child_contributions.clear();
        }
        self.active_branch_generation += 1;
        self.active_context_generation_id = Some(context_generation_id);
        self.next_sequence = retained_maximum_sequence + 1;
        self.continuation = SessionContinuationState::default();
        self.revision += 1;
        self.last_stable_context_baseline = None;
        true
    }

    /// Rewrites session-scoped call, execution, turn, and record identities for a fork.
    /// 为分叉改写会话域内的调用、执行、回合与记录身份。
    pub fn rewrite_identities_for_fork(&mut self, replacements: &BTreeMap<String, String>) -> bool {
        if replacements.is_empty() {
            return true;
        }
        if self.revision == u64::MAX {
            return false;
        }
        let continuation_was_empty = self.continuation.is_empty();
        let mut rewritten_calls = BTreeMap::new();
        for source_record in self.calls.values() {
            let mut record = source_record.clone();
            let source_call_id = record.provider_call_id.clone();
            record.provider_call_id =
                rewrite_provider_ledger_identity_text(record.provider_call_id, replacements);
            if record.provider_call_id != source_call_id && record.origin_call_id.is_none() {
                record.origin_call_id = Some(source_call_id);
            }
            record.identity.logical_request_id = rewrite_provider_ledger_identity_text(
                record.identity.logical_request_id,
                replacements,
            );
            record.identity.execution_id =
                rewrite_provider_ledger_identity_text(record.identity.execution_id, replacements);
            record.identity.turn_id = record
                .identity
                .turn_id
                .map(|turn_id| rewrite_provider_ledger_identity_text(turn_id, replacements));
            record.identity.owner_id =
                rewrite_provider_ledger_identity_text(record.identity.owner_id, replacements);
            record.context_generation_id = record.context_generation_id.map(|generation_id| {
                rewrite_provider_ledger_identity_text(generation_id, replacements)
            });
            record.context_start_record_id = record
                .context_start_record_id
                .map(|record_id| rewrite_provider_ledger_identity_text(record_id, replacements));
            record.closing_record_id = record
                .closing_record_id
                .map(|record_id| rewrite_provider_ledger_identity_text(record_id, replacements));
            if rewritten_calls
                .insert(record.provider_call_id.clone(), record)
                .is_some()
            {
                return false;
            }
        }
        let mut rewritten_child_contributions = BTreeMap::new();
        for source_contribution in self.child_contributions.values() {
            let mut contribution = source_contribution.clone();
            contribution.task_id =
                rewrite_provider_ledger_identity_text(contribution.task_id, replacements);
            contribution.execution_id =
                rewrite_provider_ledger_identity_text(contribution.execution_id, replacements);
            contribution.parent_turn_id =
                rewrite_provider_ledger_identity_text(contribution.parent_turn_id, replacements);
            contribution.source_turn_stats_record_id = contribution
                .source_turn_stats_record_id
                .map(|record_id| rewrite_provider_ledger_identity_text(record_id, replacements));
            let key = format!("{}:{}", contribution.task_id, contribution.execution_id);
            if rewritten_child_contributions
                .insert(key, contribution)
                .is_some()
            {
                return false;
            }
        }
        self.calls = rewritten_calls;
        self.child_contributions = rewritten_child_contributions;
        self.active_context_generation_id =
            self.active_context_generation_id
                .take()
                .map(|generation_id| {
                    rewrite_provider_ledger_identity_text(generation_id, replacements)
                });
        if let Some(baseline) = self.last_stable_context_baseline.as_mut() {
            baseline.provider_call_id = rewrite_provider_ledger_identity_text(
                std::mem::take(&mut baseline.provider_call_id),
                replacements,
            );
        }
        self.continuation.terminal_execution_id = self
            .continuation
            .terminal_execution_id
            .take()
            .map(|execution_id| rewrite_provider_ledger_identity_text(execution_id, replacements));
        self.continuation.last_stable_provider_call_id = self
            .continuation
            .last_stable_provider_call_id
            .take()
            .map(|call_id| rewrite_provider_ledger_identity_text(call_id, replacements));
        self.continuation.claimed_execution_id = self
            .continuation
            .claimed_execution_id
            .take()
            .map(|execution_id| rewrite_provider_ledger_identity_text(execution_id, replacements));
        self.continuation.invalidated_by_submission_id = self
            .continuation
            .invalidated_by_submission_id
            .take()
            .map(|submission_id| {
                rewrite_provider_ledger_identity_text(submission_id, replacements)
            });
        self.revision += 1;
        if let Some(baseline) = self.last_stable_context_baseline.as_mut() {
            baseline.ledger_revision = self.revision;
        }
        if !continuation_was_empty {
            self.continuation.expected_ledger_revision = self.revision;
        }
        true
    }

    /// Returns the latest active terminal conversation call with complete input usage for one exact route.
    /// 返回指定精确路由下最近一条具有完整输入用量的活动终态会话调用。
    pub fn latest_budget_baseline_for_route(
        &self,
        provider_id: &str,
        model_id: &str,
        connection_id: &str,
        protocol_profile_id: &str,
        route_revision: &str,
    ) -> Option<&SessionProviderCallRecord> {
        self.active_calls_in_sequence()
            .into_iter()
            .rev()
            .find(|record| {
                record.status.is_terminal()
                    && record.identity.operation == "conversation"
                    && record.identity.provider_id == provider_id
                    && record.identity.model_id == model_id
                    && record.identity.connection_id == connection_id
                    && record.identity.protocol_profile_id == protocol_profile_id
                    && record.identity.route_revision == route_revision
                    && record
                        .provider_usage_evidence
                        .provider_usage
                        .as_ref()
                        .and_then(provider_input_tokens)
                        .is_some()
            })
    }

    /// Materializes continuation eligibility at one proven terminal conversation boundary.
    /// 在一个已证明的会话终止边界物化 Continue 资格。
    pub fn mark_continuation_terminal(
        &mut self,
        terminal_execution_id: Option<String>,
        terminal_reason: String,
        terminal_at_ms: i64,
        canonical_tail_kind: String,
        eligible: bool,
    ) -> bool {
        if self.revision == u64::MAX || self.continuation.generation == u64::MAX {
            return false;
        }
        self.revision += 1;
        self.continuation = SessionContinuationState {
            generation: self.continuation.generation + 1,
            eligible,
            terminal_execution_id,
            terminal_reason: Some(terminal_reason),
            terminal_at_ms: Some(terminal_at_ms),
            canonical_tail_kind: Some(canonical_tail_kind),
            last_stable_provider_call_id: self
                .last_stable_context_baseline
                .as_ref()
                .map(|baseline| baseline.provider_call_id.clone()),
            expected_ledger_revision: self.revision,
            claimed_at_ms: None,
            claimed_execution_id: None,
            invalidated_by_submission_id: None,
            invalidated_at_ms: None,
        };
        true
    }

    /// Atomically claims one exact continuation generation and ledger revision.
    /// 原子领取一个精确 Continue 代际与账本修订。
    pub fn claim_continuation(
        &mut self,
        expected_generation: u64,
        expected_ledger_revision: u64,
        execution_id: String,
        claimed_at_ms: i64,
    ) -> Result<(), String> {
        if !self.continuation.eligible
            || self.continuation.generation != expected_generation
            || self.continuation.expected_ledger_revision != expected_ledger_revision
            || self.revision != expected_ledger_revision
            || self.continuation.claimed_at_ms.is_some()
            || self.revision == u64::MAX
        {
            return Err(String::from("ERR_CONVERSATION_CONTINUE_STALE"));
        }
        self.continuation.eligible = false;
        self.continuation.claimed_at_ms = Some(claimed_at_ms);
        self.continuation.claimed_execution_id = Some(execution_id);
        self.revision += 1;
        Ok(())
    }

    /// Invalidates continuation after a real user prompt is accepted.
    /// 在真实用户提示词被接受后使 Continue 资格失效。
    pub fn invalidate_continuation_for_submission(
        &mut self,
        submission_id: String,
        invalidated_at_ms: i64,
    ) -> bool {
        if self.continuation.is_empty() || !self.continuation.eligible || self.revision == u64::MAX
        {
            return false;
        }
        self.continuation.eligible = false;
        self.continuation.invalidated_by_submission_id = Some(submission_id);
        self.continuation.invalidated_at_ms = Some(invalidated_at_ms);
        self.revision += 1;
        true
    }

    /// Closes all non-terminal calls as interrupted during startup reconciliation.
    /// 在启动恢复时把全部非终态调用闭合为 interrupted。
    pub fn close_open_calls_as_interrupted(&mut self, completed_at_ms: i64) -> usize {
        if self.revision == u64::MAX {
            return 0;
        }
        let mut changed = 0_usize;
        for record in self.calls.values_mut() {
            if !record.status.is_terminal() {
                record.status = SessionProviderCallStatus::Interrupted;
                record.completed_at_ms = Some(completed_at_ms);
                record.error_category = Some(String::from("process_interrupted"));
                changed = changed.saturating_add(1);
            }
        }
        if changed > 0 {
            self.revision += 1;
        }
        changed
    }

    /// Replaces a child contribution only when its revision is strictly newer.
    /// 仅在修订号严格更新时替换子代理贡献。
    pub fn observe_child_contribution(
        &mut self,
        contribution: SessionChildUsageContribution,
    ) -> bool {
        if self.revision == u64::MAX {
            return false;
        }
        let key = format!("{}:{}", contribution.task_id, contribution.execution_id);
        if self
            .child_contributions
            .get(&key)
            .is_some_and(|current| current.revision >= contribution.revision)
        {
            return false;
        }
        self.child_contributions.insert(key, contribution);
        self.revision += 1;
        true
    }

    /// Returns the latest stable baseline pointer for the active branch.
    /// 返回活动分支最近稳定基线指针。
    fn latest_stable_baseline_ref(&self) -> Option<SessionProviderContextBaselineRef> {
        self.active_calls_in_sequence()
            .into_iter()
            .rev()
            .find(|record| {
                record.status.is_terminal()
                    && record.provider_usage_evidence.availability
                        != SessionProviderUsageAvailability::Missing
                    && record
                        .provider_usage_evidence
                        .provider_usage
                        .as_ref()
                        .and_then(provider_input_tokens)
                        .is_some()
            })
            .map(|record| SessionProviderContextBaselineRef {
                provider_call_id: record.provider_call_id.clone(),
                ledger_revision: self.revision,
            })
    }
}

/// Rewrites one session-scoped identity with the exact fork replacement table.
/// 使用精确的分叉替换表改写一个会话域身份。
fn rewrite_provider_ledger_identity_text(
    mut value: String,
    replacements: &BTreeMap<String, String>,
) -> String {
    for (source, target) in replacements {
        value = value.replace(source, target);
    }
    value
}

/// Legacy bounded request segment accepted only during v1 state migration.
/// 仅在 v1 状态迁移期间接受的旧有界请求分段。
#[derive(Debug, Clone, Deserialize)]
struct LegacySessionProviderUsageSegment {
    /// Stable telemetry request identifier.
    /// 稳定遥测请求标识。
    provider_call_id: String,
    /// Stable execution owner category.
    /// 稳定执行归属类别。
    owner_kind: String,
    /// Legacy lifecycle state.
    /// 旧生命周期状态。
    status: LegacySessionProviderUsageSegmentStatus,
    /// Legacy normalized usage.
    /// 旧规范化用量。
    #[serde(default)]
    usage: Option<ProviderUsageSnapshot>,
    /// Legacy start timestamp.
    /// 旧开始时间戳。
    #[serde(default)]
    started_at_ms: Option<i64>,
    /// Legacy terminal timestamp.
    /// 旧终态时间戳。
    #[serde(default)]
    completed_at_ms: Option<i64>,
    /// Legacy error category.
    /// 旧错误类别。
    #[serde(default)]
    error_category: Option<String>,
    /// Legacy error code.
    /// 旧错误代码。
    #[serde(default)]
    error_code: Option<String>,
}

/// Legacy three-state lifecycle accepted only during migration.
/// 仅在迁移期间接受的旧三态生命周期。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacySessionProviderUsageSegmentStatus {
    /// Legacy in-flight state.
    /// 旧进行中状态。
    InFlight,
    /// Legacy successful terminal state.
    /// 旧成功终态。
    Succeeded,
    /// Legacy failed terminal state.
    /// 旧失败终态。
    Failed,
}

/// Wire shape supporting both current v2 ledgers and bounded legacy v1 states.
/// 同时支持当前 v2 账本与有界旧 v1 状态的线格式。
#[derive(Deserialize)]
struct SessionProviderCallLedgerWire {
    /// Optional schema version; absent only for legacy states.
    /// 可选 Schema 版本；仅旧状态缺失。
    #[serde(default)]
    schema_version: Option<u32>,
    /// Current revision.
    /// 当前修订号。
    #[serde(default)]
    revision: u64,
    /// Current next sequence.
    /// 当前下一序号。
    #[serde(default)]
    next_sequence: u64,
    /// Current context generation.
    /// 当前上下文代际。
    #[serde(default)]
    active_context_generation_id: Option<String>,
    /// Current branch generation.
    /// 当前分支代际。
    #[serde(default)]
    active_branch_generation: u64,
    /// Current calls.
    /// 当前调用。
    #[serde(default)]
    calls: BTreeMap<String, SessionProviderCallRecord>,
    /// Current baseline pointer.
    /// 当前基线指针。
    #[serde(default)]
    last_stable_context_baseline: Option<SessionProviderContextBaselineRef>,
    /// Current continuation state.
    /// 当前 Continue 状态。
    #[serde(default)]
    continuation: SessionContinuationState,
    /// Current child contributions.
    /// 当前子代理贡献。
    #[serde(default)]
    child_contributions: BTreeMap<String, SessionChildUsageContribution>,
    /// Current legacy audit facts.
    /// 当前旧版审计事实。
    #[serde(default)]
    legacy_import: Option<SessionProviderLegacyImport>,
    /// Legacy bounded segments.
    /// 旧有界分段。
    #[serde(default)]
    segments: Vec<LegacySessionProviderUsageSegment>,
    /// Legacy physical-call count.
    /// 旧版物理调用数量。
    #[serde(default)]
    provider_call_count: usize,
    /// Legacy open-call count.
    /// 旧版未闭合调用数量。
    #[serde(default)]
    in_flight_call_count: usize,
    /// Legacy unknown-usage call count.
    /// 旧版未知用量调用数量。
    #[serde(default)]
    unknown_usage_call_count: usize,
    /// Legacy latest usage snapshot.
    /// 旧版最近用量快照。
    #[serde(default)]
    last_usage: Option<ProviderUsageSnapshot>,
    /// Legacy cumulative usage snapshot.
    /// 旧版累计用量快照。
    #[serde(default)]
    cumulative_usage: Option<ProviderUsageSnapshot>,
    /// Legacy usage revision.
    /// 旧版用量修订号。
    #[serde(default)]
    usage_revision: u64,
    /// Legacy durable seen-call identifiers used only for count diagnostics.
    /// 仅用于数量诊断的旧版持久已见调用标识。
    #[serde(default)]
    seen_provider_call_ids: BTreeSet<String>,
}

impl<'de> Deserialize<'de> for SessionProviderCallLedger {
    /// Deserializes current state strictly and imports legacy bounded segments once.
    /// 严格反序列化当前状态并一次性导入旧有界分段。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SessionProviderCallLedgerWire::deserialize(deserializer)?;
        if let Some(schema_version) = wire.schema_version {
            let ledger = Self {
                schema_version,
                revision: wire.revision,
                next_sequence: wire.next_sequence,
                active_context_generation_id: wire.active_context_generation_id,
                active_branch_generation: wire.active_branch_generation,
                calls: wire.calls,
                last_stable_context_baseline: wire.last_stable_context_baseline,
                continuation: wire.continuation,
                child_contributions: wire.child_contributions,
                legacy_import: wire.legacy_import,
            };
            ledger.validate().map_err(D::Error::custom)?;
            return Ok(ledger);
        }
        let mut ledger = Self::default();
        let legacy_provider_call_count = wire.provider_call_count;
        let legacy_in_flight_call_count = wire.in_flight_call_count;
        let legacy_unknown_usage_call_count = wire.unknown_usage_call_count;
        let legacy_usage_revision = wire.usage_revision;
        let legacy_seen_provider_call_id_count = wire.seen_provider_call_ids.len();
        let legacy_last_usage = wire.last_usage;
        let legacy_cumulative_usage = wire.cumulative_usage;
        let imported_segment_count = wire.segments.len();
        for segment in wire.segments {
            let prepared_at_ms = segment.started_at_ms.unwrap_or_default();
            let identity = legacy_identity(segment.owner_kind);
            let status = match segment.status {
                LegacySessionProviderUsageSegmentStatus::InFlight => {
                    SessionProviderCallStatus::Interrupted
                }
                LegacySessionProviderUsageSegmentStatus::Succeeded => {
                    SessionProviderCallStatus::Succeeded
                }
                LegacySessionProviderUsageSegmentStatus::Failed => {
                    SessionProviderCallStatus::Failed
                }
            };
            let availability = legacy_usage_availability(segment.usage.as_ref());
            let completed_at_ms = segment.completed_at_ms.unwrap_or(prepared_at_ms);
            let _ = ledger.observe_completed(SessionProviderCallCompleted {
                provider_call_id: segment.provider_call_id,
                identity,
                status,
                provider_usage_evidence: SessionProviderUsageEvidence {
                    availability,
                    provider_usage: segment.usage,
                },
                prepared_at_ms,
                dispatched_at_ms: None,
                first_event_at_ms: None,
                first_token_at_ms: None,
                completed_at_ms,
                dispatch_outcome: None,
                dispatch_outcome_known: None,
                semantic_output_observed: None,
                semantic_output_observed_known: None,
                replay_safe: None,
                error_category: segment.error_category,
                error_code: segment.error_code,
            });
        }
        let aggregate_only_usage_observed = legacy_last_usage.is_some()
            || legacy_cumulative_usage.is_some()
            || legacy_provider_call_count > imported_segment_count
            || legacy_in_flight_call_count > 0
            || legacy_unknown_usage_call_count > 0
            || legacy_usage_revision > 0
            || legacy_seen_provider_call_id_count > imported_segment_count;
        if imported_segment_count > 0 || aggregate_only_usage_observed {
            ledger.legacy_import = Some(SessionProviderLegacyImport {
                imported_segment_count,
                aggregate_only_usage_observed,
                provider_call_count: (legacy_provider_call_count > 0)
                    .then_some(legacy_provider_call_count),
                in_flight_call_count: (legacy_in_flight_call_count > 0)
                    .then_some(legacy_in_flight_call_count),
                unknown_usage_call_count: (legacy_unknown_usage_call_count > 0)
                    .then_some(legacy_unknown_usage_call_count),
                usage_revision: (legacy_usage_revision > 0).then_some(legacy_usage_revision),
                seen_provider_call_id_count: (legacy_seen_provider_call_id_count > 0)
                    .then_some(legacy_seen_provider_call_id_count),
                last_usage: legacy_last_usage,
                last_successful_usage: None,
                cumulative_usage: legacy_cumulative_usage,
            });
            ledger.revision = ledger.revision.saturating_add(1);
        }
        ledger.validate().map_err(D::Error::custom)?;
        Ok(ledger)
    }
}

/// Builds the explicit unknown identity used only for one-time legacy import.
/// 构建仅用于一次性旧状态导入的显式未知身份。
fn legacy_identity(owner_kind: String) -> SessionProviderCallIdentity {
    SessionProviderCallIdentity {
        logical_request_id: String::from("legacy_unknown"),
        execution_id: String::from("legacy_unknown"),
        turn_id: None,
        attempt_index: 0,
        retry_number: 0,
        owner_kind,
        owner_id: String::from("legacy_unknown"),
        owner_name_snapshot: String::from("Legacy unknown owner"),
        operation: String::from("legacy_unknown"),
        provider_id: String::from("legacy_unknown"),
        provider_name_snapshot: String::from("Legacy unknown provider"),
        model_id: String::from("legacy_unknown"),
        model_name_snapshot: String::from("Legacy unknown model"),
        connection_id: String::from("legacy_unknown"),
        service_entry_id: String::from("legacy_unknown"),
        protocol_profile_id: String::from("legacy_unknown"),
        manifest_revision: 0,
        manifest_generation: 0,
        binding_source: String::from("legacy_unknown"),
        binding_rule: None,
        route_revision: String::from("legacy_unknown"),
        request_profiles: Vec::new(),
        response_profiles: Vec::new(),
    }
}

/// Merges two legacy snapshots that may represent different enrichment stages of one observation.
/// 合并可能代表同一次观测不同富化阶段的两份旧版快照。
///
/// # Parameters
/// 参数
/// `current` is the evidence already retained by the ledger import.
/// `current` 是账本导入区已经保留的证据。
/// `incoming` is the deprecated top-level evidence being consumed.
/// `incoming` 是正在消费的已废弃顶层证据。
/// `evidence_name` identifies the legacy summary in conflict diagnostics.
/// `evidence_name` 用于在冲突诊断中标识旧版摘要。
///
/// # Returns
/// 返回值
/// Returns the deterministic union, or an error when two known facts disagree.
/// 返回确定性的并集；当两个已知事实不一致时返回错误。
fn merge_legacy_provider_usage_snapshot(
    current: Option<&ProviderUsageSnapshot>,
    incoming: Option<&ProviderUsageSnapshot>,
    evidence_name: &str,
) -> Result<Option<ProviderUsageSnapshot>, String> {
    let (Some(current), Some(incoming)) = (current, incoming) else {
        return Ok(current.or(incoming).cloned());
    };
    let mut merged = current.clone();
    merge_legacy_optional_usage_field(
        &mut merged.update_kind,
        &incoming.update_kind,
        evidence_name,
        "update_kind",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.input_tokens,
        &incoming.input_tokens,
        evidence_name,
        "input_tokens",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.uncached_input_tokens,
        &incoming.uncached_input_tokens,
        evidence_name,
        "uncached_input_tokens",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.output_tokens,
        &incoming.output_tokens,
        evidence_name,
        "output_tokens",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.reasoning_tokens,
        &incoming.reasoning_tokens,
        evidence_name,
        "reasoning_tokens",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.total_tokens,
        &incoming.total_tokens,
        evidence_name,
        "total_tokens",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.cached_input_tokens,
        &incoming.cached_input_tokens,
        evidence_name,
        "cached_input_tokens",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.cache_creation_input_tokens,
        &incoming.cache_creation_input_tokens,
        evidence_name,
        "cache_creation_input_tokens",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.audio_input_tokens,
        &incoming.audio_input_tokens,
        evidence_name,
        "audio_input_tokens",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.audio_output_tokens,
        &incoming.audio_output_tokens,
        evidence_name,
        "audio_output_tokens",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.extraction_profile_id,
        &incoming.extraction_profile_id,
        evidence_name,
        "extraction_profile_id",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.extraction_profile_revision,
        &incoming.extraction_profile_revision,
        evidence_name,
        "extraction_profile_revision",
    )?;
    merge_legacy_usage_source_paths(
        &mut merged.source_paths,
        &incoming.source_paths,
        evidence_name,
    )?;
    merge_legacy_usage_set(&mut merged.validation_flags, &incoming.validation_flags);
    merge_legacy_optional_usage_field(
        &mut merged.response_model,
        &incoming.response_model,
        evidence_name,
        "response_model",
    )?;
    merge_legacy_optional_usage_field(
        &mut merged.response_id,
        &incoming.response_id,
        evidence_name,
        "response_id",
    )?;
    merge_legacy_usage_set(&mut merged.billing_details, &incoming.billing_details);
    Ok(Some(merged))
}

/// Merges one optional legacy usage fact while rejecting contradictory known values.
/// 合并一个可选旧版用量事实，同时拒绝相互矛盾的已知值。
///
/// # Parameters
/// 参数
/// `target` is the retained fact updated in place.
/// `target` 是原地更新的已保留事实。
/// `incoming` is the optional deprecated fact.
/// `incoming` 是可选的已废弃事实。
/// `evidence_name` identifies the legacy summary.
/// `evidence_name` 标识旧版摘要。
/// `field_name` identifies the conflicting dimension.
/// `field_name` 标识发生冲突的维度。
///
/// # Returns
/// 返回值
/// Returns success after a compatible merge, or a precise conflict error.
/// 兼容合并后返回成功，否则返回精确的冲突错误。
fn merge_legacy_optional_usage_field<T: Clone + Eq>(
    target: &mut Option<T>,
    incoming: &Option<T>,
    evidence_name: &str,
    field_name: &str,
) -> Result<(), String> {
    match (target.as_ref(), incoming.as_ref()) {
        (Some(current), Some(incoming)) if current != incoming => Err(format!(
            "legacy {evidence_name} Provider usage conflicts on `{field_name}`"
        )),
        (None, Some(incoming)) => {
            *target = Some(incoming.clone());
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Merges usage source paths while requiring each normalized dimension to keep one exact origin.
/// 合并用量来源路径，同时要求每个规范化维度保持唯一精确来源。
///
/// # Parameters
/// 参数
/// `target` is the retained source-path map updated in place.
/// `target` 是原地更新的已保留来源路径映射。
/// `incoming` is the deprecated source-path map.
/// `incoming` 是已废弃的来源路径映射。
/// `evidence_name` identifies the legacy summary.
/// `evidence_name` 标识旧版摘要。
///
/// # Returns
/// 返回值
/// Returns success after a compatible union, or an exact conflicting-key error.
/// 兼容并集后返回成功，否则返回精确的冲突键错误。
fn merge_legacy_usage_source_paths(
    target: &mut BTreeMap<String, String>,
    incoming: &BTreeMap<String, String>,
    evidence_name: &str,
) -> Result<(), String> {
    for (dimension, incoming_path) in incoming {
        match target.get(dimension) {
            Some(current_path) if current_path != incoming_path => {
                return Err(format!(
                    "legacy {evidence_name} Provider usage conflicts on `source_paths.{dimension}`"
                ));
            }
            Some(_) => {}
            None => {
                target.insert(dimension.clone(), incoming_path.clone());
            }
        }
    }
    Ok(())
}

/// Appends unique legacy usage annotations in stable existing-first order.
/// 按已保留项优先的稳定顺序追加唯一旧版用量注记。
///
/// # Parameters
/// 参数
/// `target` is the retained annotation sequence updated in place.
/// `target` 是原地更新的已保留注记序列。
/// `incoming` is the deprecated annotation sequence.
/// `incoming` 是已废弃的注记序列。
fn merge_legacy_usage_set<T: Clone + Eq>(target: &mut Vec<T>, incoming: &[T]) {
    for value in incoming {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

/// Resolves legacy evidence quality without promoting missing fields into final facts.
/// 解析旧证据质量，且不把缺失字段提升为终态事实。
fn legacy_usage_availability(
    usage: Option<&ProviderUsageSnapshot>,
) -> SessionProviderUsageAvailability {
    match usage {
        Some(usage) if usage_has_any_value(usage) => SessionProviderUsageAvailability::Partial,
        _ => SessionProviderUsageAvailability::Missing,
    }
}

/// Adds one optional token component while preserving unknown values.
/// 累加一个可选 Token 分项并保留未知值。
fn add_optional(target: &mut Option<usize>, value: Option<usize>) {
    if let Some(value) = value {
        *target = Some(target.unwrap_or_default().saturating_add(value));
    }
}

/// Returns a complete input dimension when the Provider supplied one.
/// 返回 Provider 已提供的完整输入维度。
fn provider_input_tokens(usage: &ProviderUsageSnapshot) -> Option<usize> {
    usage.input_tokens.or_else(
        || match (usage.uncached_input_tokens, usage.cached_input_tokens) {
            (Some(uncached), Some(cached)) => Some(uncached.saturating_add(cached)),
            _ => None,
        },
    )
}

/// Returns whether a usage snapshot contains at least one known token component.
/// 返回用量快照是否至少包含一个已知 Token 分项。
fn usage_has_any_value(usage: &ProviderUsageSnapshot) -> bool {
    usage.input_tokens.is_some()
        || usage.uncached_input_tokens.is_some()
        || usage.output_tokens.is_some()
        || usage.reasoning_tokens.is_some()
        || usage.total_tokens.is_some()
        || usage.cached_input_tokens.is_some()
        || usage.cache_creation_input_tokens.is_some()
        || usage.audio_input_tokens.is_some()
        || usage.audio_output_tokens.is_some()
}

/// Returns whether a boolean is false for serde omission.
/// 返回布尔值是否为 false，以供 serde 忽略字段。
fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Builds one deterministic identity for ledger tests.
    /// 为账本测试构建一个确定性身份。
    fn identity(execution_id: &str, attempt_index: u32) -> SessionProviderCallIdentity {
        SessionProviderCallIdentity {
            logical_request_id: execution_id.to_string(),
            execution_id: execution_id.to_string(),
            turn_id: Some(String::from("turn-1")),
            attempt_index,
            retry_number: attempt_index,
            owner_kind: String::from("main_agent"),
            owner_id: String::from("agent-1"),
            owner_name_snapshot: String::from("Agent One"),
            operation: String::from("conversation"),
            provider_id: String::from("provider-1"),
            provider_name_snapshot: String::from("Provider One"),
            model_id: String::from("model-1"),
            model_name_snapshot: String::from("Model One"),
            connection_id: String::from("connection-1"),
            service_entry_id: String::from("public-api"),
            protocol_profile_id: String::from("openai.chat"),
            manifest_revision: 1,
            manifest_generation: 2,
            binding_source: String::from("system_manifest"),
            binding_rule: Some(String::from("model-rule")),
            route_revision: String::from("sha256:route"),
            request_profiles: Vec::new(),
            response_profiles: Vec::new(),
        }
    }

    /// Builds one deterministic started observation.
    /// 构建一条确定性的开始观测。
    fn started(call_id: &str, attempt_index: u32) -> SessionProviderCallStarted {
        SessionProviderCallStarted {
            provider_call_id: call_id.to_string(),
            identity: identity("execution-1", attempt_index),
            prepared_at_ms: i64::from(attempt_index),
            request_assembly_fingerprint: None,
            context_generation_id: None,
            context_start_record_id: None,
            model_route_fingerprint: None,
            toolset_fingerprint: None,
            system_prompt_fingerprint: None,
            request_text_measurement: None,
            budget_accounting: None,
        }
    }

    /// Builds one deterministic terminal observation.
    /// 构建一条确定性的终态观测。
    fn completed(
        call_id: &str,
        attempt_index: u32,
        usage: Option<ProviderUsageSnapshot>,
        availability: SessionProviderUsageAvailability,
    ) -> SessionProviderCallCompleted {
        SessionProviderCallCompleted {
            provider_call_id: call_id.to_string(),
            identity: identity("execution-1", attempt_index),
            status: SessionProviderCallStatus::Succeeded,
            provider_usage_evidence: SessionProviderUsageEvidence {
                availability,
                provider_usage: usage,
            },
            prepared_at_ms: i64::from(attempt_index),
            dispatched_at_ms: Some(i64::from(attempt_index)),
            first_event_at_ms: Some(i64::from(attempt_index) + 1),
            first_token_at_ms: Some(i64::from(attempt_index) + 2),
            completed_at_ms: i64::from(attempt_index) + 3,
            dispatch_outcome: Some(String::from("dispatched")),
            dispatch_outcome_known: Some(true),
            semantic_output_observed: Some(true),
            semantic_output_observed_known: Some(true),
            replay_safe: Some(false),
            error_category: None,
            error_code: None,
        }
    }

    /// Verifies repeated started events are idempotent only when immutable facts match.
    /// 验证仅当不可变事实一致时重复开始事件才具有幂等性。
    #[test]
    fn started_idempotence_rejects_conflicting_identity() {
        let mut ledger = SessionProviderCallLedger::default();
        let original = started("call-1", 0);
        assert!(ledger.observe_started(original.clone()));
        assert!(ledger.started_event_is_idempotent(&original));

        let mut conflicting = original;
        conflicting.identity.model_id = String::from("model-2");
        assert!(!ledger.started_event_is_idempotent(&conflicting));
    }

    /// Verifies repeated completed events are idempotent only when every terminal fact matches.
    /// 验证仅当全部终态事实一致时重复完成事件才具有幂等性。
    #[test]
    fn completed_idempotence_rejects_conflicting_usage() {
        let mut ledger = SessionProviderCallLedger::default();
        let original = completed(
            "call-1",
            0,
            Some(ProviderUsageSnapshot {
                total_tokens: Some(10),
                ..ProviderUsageSnapshot::default()
            }),
            SessionProviderUsageAvailability::Final,
        );
        assert!(ledger.observe_started(started("call-1", 0)));
        assert!(ledger.observe_completed(original.clone()));
        assert!(ledger.completed_event_is_idempotent(&original));

        let mut conflicting = original;
        conflicting
            .provider_usage_evidence
            .provider_usage
            .as_mut()
            .expect("test usage should exist")
            .total_tokens = Some(11);
        assert!(!ledger.completed_event_is_idempotent(&conflicting));
    }

    /// Verifies ten thousand physical calls remain complete and idempotent.
    /// 验证一万次物理调用保持完整且幂等。
    #[test]
    fn ledger_retains_ten_thousand_calls_without_trimming_or_duplicates() {
        let mut ledger = SessionProviderCallLedger::default();
        for index in 0..10_000_u32 {
            let call_id = format!("call-{index}");
            assert!(ledger.observe_started(started(&call_id, index)));
            assert!(!ledger.observe_started(started(&call_id, index)));
            assert!(ledger.observe_completed(completed(
                &call_id,
                index,
                Some(ProviderUsageSnapshot {
                    input_tokens: Some(0),
                    total_tokens: Some(0),
                    ..ProviderUsageSnapshot::default()
                }),
                SessionProviderUsageAvailability::Final,
            )));
            assert!(!ledger.observe_completed(completed(
                &call_id,
                index,
                None,
                SessionProviderUsageAvailability::Missing,
            )));
        }
        assert_eq!(ledger.provider_call_count(), 10_000);
        assert_eq!(ledger.calls_in_sequence().len(), 10_000);
        assert_eq!(ledger.in_flight_call_count(), 0);
        assert_eq!(ledger.unknown_usage_call_count(), 0);
        ledger.validate().expect("ledger should remain valid");
        let serialize_started_at = Instant::now();
        let json = serde_json::to_vec(&ledger).expect("large ledger should serialize");
        let serialize_elapsed = serialize_started_at.elapsed();
        let deserialize_started_at = Instant::now();
        let restored: SessionProviderCallLedger =
            serde_json::from_slice(&json).expect("large ledger should deserialize");
        let deserialize_elapsed = deserialize_started_at.elapsed();
        assert_eq!(restored, ledger);
        eprintln!(
            "10k ledger bytes={}, serialize_ms={}, deserialize_ms={}",
            json.len(),
            serialize_elapsed.as_millis(),
            deserialize_elapsed.as_millis()
        );
    }

    /// Records the serialized size and round-trip time of a representative one-thousand-call ledger.
    /// 记录具有代表性的一千条调用账本的序列化体积与往返耗时。
    #[test]
    fn ledger_one_thousand_call_serialization_metrics_are_measured() {
        let mut ledger = SessionProviderCallLedger::default();
        for index in 0..1_000_u32 {
            let call_id = format!("call-{index}");
            assert!(ledger.observe_started(started(&call_id, index)));
            assert!(ledger.observe_completed(completed(
                &call_id,
                index,
                Some(ProviderUsageSnapshot {
                    input_tokens: Some(900),
                    output_tokens: Some(100),
                    total_tokens: Some(1_000),
                    ..ProviderUsageSnapshot::default()
                }),
                SessionProviderUsageAvailability::Final,
            )));
        }

        let serialize_started_at = Instant::now();
        let json = serde_json::to_vec(&ledger).expect("1k ledger should serialize");
        let serialize_elapsed = serialize_started_at.elapsed();
        let deserialize_started_at = Instant::now();
        let restored: SessionProviderCallLedger =
            serde_json::from_slice(&json).expect("1k ledger should deserialize");
        let deserialize_elapsed = deserialize_started_at.elapsed();

        assert_eq!(restored, ledger);
        eprintln!(
            "1k ledger bytes={}, serialize_ms={}, deserialize_ms={}",
            json.len(),
            serialize_elapsed.as_millis(),
            deserialize_elapsed.as_millis()
        );
    }

    /// Verifies reported zero and missing usage remain different facts.
    /// 验证已报告零值与缺失用量保持为不同事实。
    #[test]
    fn reported_zero_remains_distinct_from_missing_usage() {
        let mut ledger = SessionProviderCallLedger::default();
        assert!(ledger.observe_completed(completed(
            "zero",
            0,
            Some(ProviderUsageSnapshot {
                input_tokens: Some(0),
                total_tokens: Some(0),
                ..ProviderUsageSnapshot::default()
            }),
            SessionProviderUsageAvailability::Final,
        )));
        assert!(ledger.observe_completed(completed(
            "missing",
            1,
            None,
            SessionProviderUsageAvailability::Missing,
        )));
        assert_eq!(ledger.unknown_usage_call_count(), 1);
        assert_eq!(
            ledger.calls["zero"]
                .provider_usage_evidence
                .provider_usage
                .as_ref()
                .and_then(|usage| usage.total_tokens),
            Some(0)
        );
    }

    /// Verifies partial Provider evidence with a complete input-token dimension remains budget-stable.
    /// 验证具有完整输入 Token 维度的部分 Provider 证据仍可作为稳定预算基线。
    #[test]
    fn partial_usage_with_complete_input_tokens_becomes_stable_baseline() {
        let mut ledger = SessionProviderCallLedger::default();
        assert!(ledger.observe_completed(completed(
            "partial-input",
            0,
            Some(ProviderUsageSnapshot {
                input_tokens: Some(321),
                output_tokens: None,
                total_tokens: None,
                ..ProviderUsageSnapshot::default()
            }),
            SessionProviderUsageAvailability::Partial,
        )));

        let baseline = ledger
            .last_stable_context_baseline
            .as_ref()
            .expect("complete input usage should establish a stable baseline");
        assert_eq!(baseline.provider_call_id, "partial-input");
        assert_eq!(
            ledger
                .calls
                .get(&baseline.provider_call_id)
                .and_then(|record| record.provider_usage_evidence.provider_usage.as_ref())
                .and_then(provider_input_tokens),
            Some(321)
        );
    }

    /// Verifies a terminal call cannot regress or be counted twice.
    /// 验证终态调用不能回退或重复计数。
    #[test]
    fn terminal_call_never_regresses() {
        let mut ledger = SessionProviderCallLedger::default();
        assert!(ledger.observe_completed(completed(
            "call-1",
            0,
            None,
            SessionProviderUsageAvailability::Missing,
        )));
        let revision = ledger.revision;
        assert!(!ledger.observe_started(started("call-1", 0)));
        assert!(!ledger.observe_completed(completed(
            "call-1",
            0,
            Some(ProviderUsageSnapshot {
                total_tokens: Some(9),
                ..ProviderUsageSnapshot::default()
            }),
            SessionProviderUsageAvailability::Final,
        )));
        assert_eq!(ledger.revision, revision);
        assert_eq!(
            ledger.calls["call-1"].status,
            SessionProviderCallStatus::Succeeded
        );
    }

    /// Verifies the prepared-to-dispatched transition is explicit and idempotent.
    /// 验证 prepared 到 dispatched 的转换是显式且幂等的。
    #[test]
    fn dispatched_transition_is_explicit_and_idempotent() {
        let mut ledger = SessionProviderCallLedger::default();
        assert!(ledger.observe_started(started("call-dispatched", 0)));
        assert!(ledger.observe_dispatched("call-dispatched", 10));
        assert!(!ledger.observe_dispatched("call-dispatched", 11));
        let record = &ledger.calls["call-dispatched"];
        assert_eq!(record.status, SessionProviderCallStatus::Dispatched);
        assert_eq!(record.dispatched_at_ms, Some(10));
        assert_eq!(record.dispatch_outcome.as_deref(), Some("dispatched"));
        assert_eq!(record.dispatch_outcome_known, Some(true));
    }

    /// Verifies current ledgers serialize and deserialize without semantic loss.
    /// 验证当前账本序列化往返不发生语义丢失。
    #[test]
    fn current_ledger_round_trip_is_lossless() {
        let mut ledger = SessionProviderCallLedger::default();
        assert!(ledger.observe_completed(completed(
            "call-1",
            0,
            Some(ProviderUsageSnapshot {
                input_tokens: Some(7),
                output_tokens: Some(3),
                total_tokens: Some(10),
                ..ProviderUsageSnapshot::default()
            }),
            SessionProviderUsageAvailability::Final,
        )));
        let json = serde_json::to_string(&ledger).expect("ledger should serialize");
        let restored: SessionProviderCallLedger =
            serde_json::from_str(&json).expect("ledger should deserialize");
        assert_eq!(restored, ledger);
    }

    /// Verifies bounded legacy segments import once without restoring an open request.
    /// 验证有界旧分段只导入一次且不会恢复未闭合请求。
    #[test]
    fn legacy_segments_import_as_auditable_v2_records() {
        let json = serde_json::json!({
            "segments": [{
                "provider_call_id": "legacy-call",
                "owner_kind": "main_agent",
                "status": "in_flight",
                "started_at_ms": 9
            }]
        });
        let ledger: SessionProviderCallLedger =
            serde_json::from_value(json).expect("legacy state should migrate");
        assert_eq!(
            ledger.schema_version,
            SESSION_PROVIDER_CALL_LEDGER_SCHEMA_VERSION
        );
        assert_eq!(ledger.provider_call_count(), 1);
        assert_eq!(ledger.in_flight_call_count(), 0);
        assert_eq!(
            ledger.calls["legacy-call"].status,
            SessionProviderCallStatus::Interrupted
        );
        assert_eq!(
            ledger
                .legacy_import
                .as_ref()
                .map(|value| value.imported_segment_count),
            Some(1)
        );
    }

    /// Verifies trimmed v1 aggregates remain audit facts without fabricating missing calls.
    /// 验证被裁剪的 v1 聚合仍作为审计事实保留且不会虚构缺失调用。
    #[test]
    fn legacy_trimmed_aggregates_are_preserved_without_synthetic_calls() {
        let segments = (0..128)
            .map(|index| {
                serde_json::json!({
                    "provider_call_id": format!("legacy-{index}"),
                    "owner_kind": "main_agent",
                    "status": "succeeded",
                    "usage": { "total_tokens": 1 },
                    "started_at_ms": index,
                    "completed_at_ms": index + 1
                })
            })
            .collect::<Vec<_>>();
        let seen_provider_call_ids = (0..200)
            .map(|index| format!("legacy-{index}"))
            .collect::<Vec<_>>();
        let ledger: SessionProviderCallLedger = serde_json::from_value(serde_json::json!({
            "provider_call_count": 200,
            "in_flight_call_count": 3,
            "unknown_usage_call_count": 7,
            "last_usage": { "input_tokens": 90, "total_tokens": 100 },
            "cumulative_usage": { "input_tokens": 9000, "total_tokens": 10000 },
            "usage_revision": 412,
            "segments": segments,
            "seen_provider_call_ids": seen_provider_call_ids
        }))
        .expect("trimmed legacy state should migrate");

        assert_eq!(ledger.provider_call_count(), 128);
        let legacy = ledger
            .legacy_import
            .as_ref()
            .expect("legacy aggregate evidence should remain available");
        assert!(legacy.aggregate_only_usage_observed);
        assert_eq!(legacy.provider_call_count, Some(200));
        assert_eq!(legacy.in_flight_call_count, Some(3));
        assert_eq!(legacy.unknown_usage_call_count, Some(7));
        assert_eq!(legacy.usage_revision, Some(412));
        assert_eq!(legacy.seen_provider_call_id_count, Some(200));
        assert_eq!(
            legacy
                .cumulative_usage
                .as_ref()
                .and_then(|usage| usage.total_tokens),
            Some(10_000)
        );
    }

    /// Verifies a sparse top-level duplicate keeps the richer ledger-import evidence.
    /// 验证稀疏的顶层重复摘要会保留账本导入区中更丰富的证据。
    #[test]
    fn legacy_usage_import_keeps_existing_enrichment() {
        let rich_usage = ProviderUsageSnapshot {
            input_tokens: Some(9_000),
            uncached_input_tokens: Some(8_243),
            cached_input_tokens: Some(757),
            total_tokens: Some(9_100),
            ..ProviderUsageSnapshot::default()
        };
        let sparse_usage = ProviderUsageSnapshot {
            input_tokens: Some(9_000),
            cached_input_tokens: Some(757),
            total_tokens: Some(9_100),
            ..ProviderUsageSnapshot::default()
        };
        let mut ledger = SessionProviderCallLedger {
            revision: 1,
            legacy_import: Some(SessionProviderLegacyImport {
                aggregate_only_usage_observed: true,
                last_usage: Some(rich_usage.clone()),
                ..SessionProviderLegacyImport::default()
            }),
            ..SessionProviderCallLedger::default()
        };

        assert!(
            !ledger
                .import_legacy_runtime_usage_summaries(Some(sparse_usage), None)
                .expect("compatible sparse evidence should import")
        );
        assert_eq!(
            ledger
                .legacy_import
                .as_ref()
                .and_then(|legacy_import| legacy_import.last_usage.as_ref()),
            Some(&rich_usage)
        );
        assert_eq!(ledger.revision, 1);
    }

    /// Verifies richer top-level evidence deterministically enriches the ledger import once.
    /// 验证更丰富的顶层证据会确定性地且仅一次富化账本导入区。
    #[test]
    fn legacy_usage_import_accepts_additive_enrichment() {
        let sparse_usage = ProviderUsageSnapshot {
            input_tokens: Some(9_000),
            total_tokens: Some(9_100),
            ..ProviderUsageSnapshot::default()
        };
        let rich_usage = ProviderUsageSnapshot {
            input_tokens: Some(9_000),
            uncached_input_tokens: Some(8_243),
            total_tokens: Some(9_100),
            ..ProviderUsageSnapshot::default()
        };
        let mut ledger = SessionProviderCallLedger {
            revision: 1,
            legacy_import: Some(SessionProviderLegacyImport {
                aggregate_only_usage_observed: true,
                last_usage: Some(sparse_usage),
                ..SessionProviderLegacyImport::default()
            }),
            ..SessionProviderCallLedger::default()
        };

        assert!(
            ledger
                .import_legacy_runtime_usage_summaries(Some(rich_usage.clone()), None)
                .expect("additive evidence should enrich the import")
        );
        assert_eq!(
            ledger
                .legacy_import
                .as_ref()
                .and_then(|legacy_import| legacy_import.last_usage.as_ref()),
            Some(&rich_usage)
        );
        assert_eq!(ledger.revision, 2);
        assert!(
            !ledger
                .import_legacy_runtime_usage_summaries(Some(rich_usage), None)
                .expect("repeated enriched evidence should be idempotent")
        );
        assert_eq!(ledger.revision, 2);
    }

    /// Verifies known conflicting token facts still fail closed without mutating the ledger.
    /// 验证已知且冲突的 Token 事实仍会封闭失败且不会改变账本。
    #[test]
    fn legacy_usage_import_rejects_contradictory_known_values() {
        let mut ledger = SessionProviderCallLedger {
            revision: 1,
            legacy_import: Some(SessionProviderLegacyImport {
                aggregate_only_usage_observed: true,
                last_usage: Some(ProviderUsageSnapshot {
                    input_tokens: Some(9_000),
                    ..ProviderUsageSnapshot::default()
                }),
                ..SessionProviderLegacyImport::default()
            }),
            ..SessionProviderCallLedger::default()
        };
        let snapshot = ledger.clone();

        let error = ledger
            .import_legacy_runtime_usage_summaries(
                Some(ProviderUsageSnapshot {
                    input_tokens: Some(9_001),
                    ..ProviderUsageSnapshot::default()
                }),
                None,
            )
            .expect_err("contradictory known evidence must fail closed");

        assert!(error.contains("input_tokens"));
        assert_eq!(ledger, snapshot);
    }

    /// Verifies an already-imported duplicate remains readable after revision exhaustion.
    /// 验证修订号耗尽后已经导入的重复证据仍然可以读取。
    #[test]
    fn legacy_usage_duplicate_is_idempotent_after_revision_exhaustion() {
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(9_000),
            ..ProviderUsageSnapshot::default()
        };
        let mut ledger = SessionProviderCallLedger {
            revision: u64::MAX,
            legacy_import: Some(SessionProviderLegacyImport {
                aggregate_only_usage_observed: true,
                last_usage: Some(usage.clone()),
                ..SessionProviderLegacyImport::default()
            }),
            ..SessionProviderCallLedger::default()
        };

        assert!(
            !ledger
                .import_legacy_runtime_usage_summaries(Some(usage), None)
                .expect("an exact duplicate needs no exhausted revision increment")
        );
        assert_eq!(ledger.revision, u64::MAX);
    }

    /// Verifies unsupported future schema versions fail closed.
    /// 验证不支持的未来 Schema 版本会封闭失败。
    #[test]
    fn unsupported_schema_version_fails_closed() {
        let error = serde_json::from_value::<SessionProviderCallLedger>(serde_json::json!({
            "schema_version": 99,
            "revision": 0,
            "next_sequence": 1,
            "calls": {}
        }))
        .expect_err("unsupported schema must fail");
        assert!(
            error
                .to_string()
                .contains("unsupported session Provider call ledger")
        );
    }

    /// Verifies exhausted monotonic counters reject mutations without corrupting the ledger.
    /// 验证单调计数器耗尽时拒绝变更且不会破坏账本。
    #[test]
    fn saturated_counters_fail_closed_without_partial_mutation() {
        let mut exhausted_sequence = SessionProviderCallLedger {
            next_sequence: u64::MAX,
            ..SessionProviderCallLedger::default()
        };
        let snapshot = exhausted_sequence.clone();
        assert!(!exhausted_sequence.observe_started(started("call-max", 0)));
        assert_eq!(exhausted_sequence, snapshot);
        exhausted_sequence
            .validate()
            .expect("rejected sequence exhaustion should stay valid");

        let mut exhausted_revision = SessionProviderCallLedger {
            revision: u64::MAX - 1,
            ..SessionProviderCallLedger::default()
        };
        let snapshot = exhausted_revision.clone();
        assert!(!exhausted_revision.observe_completed(completed(
            "call-terminal",
            0,
            None,
            SessionProviderUsageAvailability::Missing,
        )));
        assert_eq!(exhausted_revision, snapshot);
    }

    /// Verifies canonical closing records override ambiguous equal-millisecond timestamps.
    /// 验证规范闭合记录会覆盖存在歧义的同毫秒时间戳。
    #[test]
    fn record_boundaries_drive_edit_and_fork_membership() {
        let mut ledger = SessionProviderCallLedger::default();
        for (call_id, closing_record_id) in [("before", "record-before"), ("after", "record-after")]
        {
            let mut started = started(call_id, 0);
            started.prepared_at_ms = 100;
            started.identity.execution_id = call_id.to_string();
            assert!(ledger.observe_started(started));
            ledger
                .calls
                .get_mut(call_id)
                .expect("call should exist")
                .closing_record_id = Some(closing_record_id.to_string());
        }
        let retained_record_ids = BTreeSet::from([String::from("record-before")]);
        let mut edited = ledger.clone();
        assert_eq!(
            edited.supersede_outside_record_boundary(
                &retained_record_ids,
                100,
                1,
                String::from("edit-context-1"),
            ),
            1
        );
        assert!(edited.calls["before"].active_branch_member);
        assert!(!edited.calls["after"].active_branch_member);
        assert_eq!(
            edited.active_context_generation_id.as_deref(),
            Some("edit-context-1")
        );
        assert!(edited.last_stable_context_baseline.is_none());

        let mut forked = ledger;
        assert!(forked.retain_inherited_through_record_boundary(
            &retained_record_ids,
            100,
            true,
            String::from("fork-context-1"),
        ));
        assert!(forked.calls.contains_key("before"));
        assert!(!forked.calls.contains_key("after"));
        assert!(forked.calls["before"].inherited_from_fork);
        assert_eq!(
            forked.active_context_generation_id.as_deref(),
            Some("fork-context-1")
        );
        assert!(forked.last_stable_context_baseline.is_none());
    }

    /// Verifies an edit creates a new branch generation even when every historical call remains before the anchor.
    /// 验证即使全部历史调用都位于锚点之前，编辑仍会创建新的分支代际。
    #[test]
    fn edit_without_superseded_calls_still_advances_branch_generation() {
        let mut ledger = SessionProviderCallLedger::default();
        assert_eq!(
            ledger.supersede_outside_record_boundary(
                &BTreeSet::new(),
                100,
                1,
                String::from("edit-context-1"),
            ),
            0
        );

        assert_eq!(ledger.active_branch_generation, 1);
        assert_eq!(
            ledger.active_context_generation_id.as_deref(),
            Some("edit-context-1")
        );
        assert!(ledger.last_stable_context_baseline.is_none());
        assert_eq!(ledger.revision, 1);
    }

    /// Verifies a fork rewrites all known session-scoped ledger references once.
    /// 验证分叉会一次性改写账本中所有已知的会话域引用。
    #[test]
    fn fork_identity_rewrite_covers_call_turn_record_and_child_references() {
        let source = "source-session";
        let target = "target-session";
        let mut ledger = SessionProviderCallLedger::default();
        let mut started = started(&format!("call-{source}"), 0);
        started.identity.logical_request_id = format!("logical-{source}");
        started.identity.execution_id = format!("execution-{source}");
        started.identity.turn_id = Some(format!("turn-{source}"));
        started.identity.owner_id = format!("owner-{source}");
        started.context_generation_id = Some(format!("generation-{source}"));
        started.context_start_record_id = Some(format!("start-{source}"));
        assert!(ledger.observe_started(started));
        ledger
            .calls
            .get_mut(&format!("call-{source}"))
            .expect("call should exist")
            .closing_record_id = Some(format!("closing-{source}"));
        assert!(
            ledger.observe_child_contribution(SessionChildUsageContribution {
                task_id: format!("task-{source}"),
                execution_id: format!("child-execution-{source}"),
                parent_turn_id: format!("parent-turn-{source}"),
                revision: 1,
                status: String::from("final"),
                inclusive_usage: None,
                provider_call_count: 1,
                unknown_usage_call_count: 1,
                source_turn_stats_record_id: Some(format!("stats-{source}")),
            })
        );
        assert!(ledger.rewrite_identities_for_fork(&BTreeMap::from([(
            source.to_string(),
            target.to_string(),
        )])));
        let call = &ledger.calls[&format!("call-{target}")];
        assert_eq!(call.origin_call_id.as_deref(), Some("call-source-session"));
        assert_eq!(call.identity.execution_id, "execution-target-session");
        assert_eq!(
            call.identity.turn_id.as_deref(),
            Some("turn-target-session")
        );
        assert_eq!(
            call.context_start_record_id.as_deref(),
            Some("start-target-session")
        );
        assert_eq!(
            call.closing_record_id.as_deref(),
            Some("closing-target-session")
        );
        assert!(
            ledger
                .child_contributions
                .contains_key("task-target-session:child-execution-target-session")
        );
    }

    /// Verifies child revisions and continuation claims are monotonic and idempotent.
    /// 验证子贡献修订与 Continue 认领保持单调且幂等。
    #[test]
    fn child_contribution_and_continuation_revisions_reject_stale_replays() {
        let mut ledger = SessionProviderCallLedger::default();
        let contribution = SessionChildUsageContribution {
            task_id: String::from("task-1"),
            execution_id: String::from("execution-1"),
            parent_turn_id: String::from("turn-1"),
            revision: 1,
            status: String::from("partial"),
            inclusive_usage: None,
            provider_call_count: 1,
            unknown_usage_call_count: 1,
            source_turn_stats_record_id: None,
        };
        assert!(ledger.observe_child_contribution(contribution.clone()));
        assert!(!ledger.observe_child_contribution(contribution.clone()));
        assert!(
            !ledger.observe_child_contribution(SessionChildUsageContribution {
                revision: 0,
                ..contribution.clone()
            })
        );
        assert!(
            ledger.observe_child_contribution(SessionChildUsageContribution {
                revision: 2,
                status: String::from("final"),
                ..contribution
            })
        );
        assert!(ledger.mark_continuation_terminal(
            Some(String::from("execution-1")),
            String::from("failed"),
            10,
            String::from("error"),
            true,
        ));
        let generation = ledger.continuation.generation;
        let expected_revision = ledger.continuation.expected_ledger_revision;
        assert!(
            ledger
                .claim_continuation(
                    generation,
                    expected_revision,
                    String::from("execution-2"),
                    11,
                )
                .is_ok()
        );
        assert_eq!(
            ledger.claim_continuation(
                generation,
                expected_revision,
                String::from("execution-3"),
                12,
            ),
            Err(String::from("ERR_CONVERSATION_CONTINUE_STALE"))
        );
    }
}
