//! V1 metadata document: session mode, config, input state, and split map.
//! V1 元数据文档：会话模式、配置、输入区状态与分片映射。
//!
//! `metadata.json` stays lightweight: mode switches, the latest user
//! selections, request configuration for the active node and next admitted user boundary,
//! input drafts and queues, and the physical split map. It never stores the
//! main event stream, transcripts, recovery copies, or statistics ledgers.
//! `metadata.json` 保持轻量：模式开关、用户最新选择、活动节点及下一个已准入用户边界
//! 使用的请求配置、输入草稿与队列、物理分片映射。它绝不保存主事件流、
//! 转录、恢复副本或统计账本。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::ContextCompactionWindowNodeId;

use crate::v1::collaboration::{CollaborationExecutionId, CollaborationMetadata};
use crate::v1::ids::{ApiCallId, ClientActionId, PendingInputId, ResourceId, Seq, TurnId};

/// File contract of the V1 session metadata document.
/// V1 会话元数据文档的文件契约。
pub const METADATA_CONTRACT: &str = "vulcan.session.v1";

/// Current metadata schema; storage migrations are required whenever this version increases.
/// 当前元数据模式版本；每次递增都必须提供存储迁移。
pub const METADATA_SCHEMA_VERSION: u32 = 3;

/// Maximum number of Unicode scalar values accepted in one Goal objective.
/// 一个 Goal 目标内容允许的 Unicode 标量值最大数量。
pub const MAX_GOAL_OBJECTIVE_CHARS: usize = 4_000;

/// Returns whether one serialized boolean is false.
/// 返回一个序列化布尔值是否为 false。
fn is_false(value: &bool) -> bool {
    !*value
}

/// Current state of one Goal configuration.
/// 一个 Goal 配置的当前状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalState {
    /// Goal is enabled and auto-continue may schedule turns.
    /// Goal 已启用，自动续跑可以调度回合。
    Active,
    /// Goal auto-continue is paused.
    /// Goal 自动续跑已暂停。
    #[default]
    Paused,
    /// Goal completed its objective.
    /// Goal 已完成目标。
    Completed,
    /// Goal is blocked and cannot continue.
    /// Goal 被阻塞，无法继续。
    Blocked,
    /// Goal budget was exhausted.
    /// Goal 预算耗尽。
    BudgetExhausted,
}

/// Cached Goal budget counters with their covered statistics seq.
/// 带覆盖统计 seq 的 Goal 预算缓存计数。
///
/// This is only an accelerator cache. On mismatch the cache must be
/// recomputed from main events and must never become a second fact source.
/// 这只是加速缓存。失配时必须从主事件重算，绝不能成为第二事实源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalBudgetCache {
    /// Main-event seq of the last statistic covered by this cache.
    /// 本缓存覆盖到的最后一个统计的主事件 seq。
    pub covered_until_seq: Seq,
    /// Cached direct-scope budget tokens.
    /// 缓存的 direct 口径预算 token。
    pub direct_total_tokens: u64,
    /// Cached subagent-scope budget tokens.
    /// 缓存的 subagent 口径预算 token。
    pub subagent_total_tokens: u64,
    /// Cached merged total budget tokens.
    /// 缓存的归并 total 口径预算 token。
    pub total_tokens: u64,
    /// Last modified time in Unix milliseconds.
    /// 最后修改时间(Unix 毫秒)。
    pub updated_at_ms: i64,
}

/// Goal configuration persisted inside `mode.goal`.
/// 保存在 `mode.goal` 内的 Goal 配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalConfig {
    /// Whether Goal mode is enabled.
    /// Goal 模式是否启用。
    pub enabled: bool,
    /// Goal objective text.
    /// 目标内容。
    pub objective: String,
    /// Optional token budget for auto-continue decisions.
    /// 可选的自动续跑 token 预算。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u64>,
    /// Turn that started the Goal execution, if any.
    /// 启动 Goal 执行的回合(如有)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_turn_id: Option<TurnId>,
    /// Main-event seq where the Goal started, if any.
    /// Goal 启动处的主事件 seq(如有)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_seq: Option<Seq>,
    /// Current Goal state.
    /// 当前 Goal 状态。
    pub state: GoalState,
    /// Optional budget accelerator cache.
    /// 可选预算加速缓存。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_cache: Option<GoalBudgetCache>,
    /// Root lifetime counters captured when this Goal generation was created.
    /// 创建当前 Goal 代际时记录的根会话生命周期累计。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_baseline: Option<crate::v1::lifetime_usage::GoalUsageSnapshot>,
}

/// Plan-mode configuration persisted inside `mode.plan`.
/// 保存在 `mode.plan` 内的 Plan 模式配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanModeConfig {
    /// Whether Plan mode is enabled.
    /// Plan 模式是否开启。
    pub enabled: bool,
}

/// Future session-level access authorizations.
/// 未来会话级授权选择。
///
/// Only real implemented fields may be added; no speculative configuration is
/// created ahead of the product feature.
/// 只允许加入真实已实现的字段；不为尚未到来的产品功能提前创造配置。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessConfig {}

/// Session mode switches.
/// 会话模式开关。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeConfig {
    /// Goal-mode configuration.
    /// Goal 模式配置。
    pub goal: GoalConfig,
    /// Plan-mode configuration.
    /// Plan 模式配置。
    pub plan: PlanModeConfig,
    /// Session-level access selections.
    /// 会话级授权选择。
    pub access: AccessConfig,
}

/// Request configuration effective for the active execution node.
/// 对当前活动执行节点生效的请求配置。
///
/// This is the default for the next physical request; it is never a copy of
/// the execution scene of an in-flight request. Actual per-request usage is
/// recorded only in that request's `api_stats`.
/// 这是下一物理请求的默认配置；它绝不是某个在途请求的执行现场副本。
/// 每次请求实际使用的配置只记录在该请求的 `api_stats` 中。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveRequestConfig {
    /// Provider name in effect.
    /// 生效的 Provider 名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Non-secret connection id in effect.
    /// 生效的非秘密连接 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Model in effect.
    /// 生效的模型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Thinking level in effect.
    /// 生效的思考级别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    /// Temperature in effect.
    /// 生效的温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

/// Returns the canonical comparison key for the reasoning encodings emitted by session settings and API facts.
/// 返回会话设置与 API 事实已使用的思考编码对应的规范比较键。
/// `level` is the optional persisted key; `off` equals absence and legacy `xhigh` equals `x-high`.
/// `level` 为可选持久化键；`off` 等价于缺省，旧 `xhigh` 等价于 `x-high`。
/// All other keys retain their exact identity; this does not rewrite persisted request facts.
/// 其他键保持精确身份；该函数不会改写持久化请求事实。
pub fn canonical_request_thinking_level(level: Option<&str>) -> Option<&str> {
    match level {
        Some("off") | None => None,
        Some("xhigh") => Some("x-high"),
        other => other,
    }
}

/// Pending request-configuration change applied only at the next admitted user or Continue boundary.
/// 仅在下一个已准入用户输入或 Continue 边界生效的请求配置变更。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRequestConfig {
    /// Pending provider name.
    /// 待生效 Provider 名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Pending non-secret connection id.
    /// 待生效非秘密连接 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Pending model.
    /// 待生效模型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Pending thinking level.
    /// 待生效思考级别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    /// Pending temperature.
    /// 待生效温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// When the change was requested in Unix milliseconds.
    /// 变更请求时刻(Unix 毫秒)。
    pub set_at_ms: i64,
}

/// One prompt-skill session-level enable/disable override.
/// 一个提示词技能的会话级启停覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptSkillOverride {
    /// Skill id being overridden.
    /// 被覆盖的技能 id。
    pub skill_id: String,
    /// Whether the skill is enabled for this session.
    /// 该技能在本会话是否启用。
    pub enabled: bool,
}

/// One exact provider-tool target selected without carrying Provider-specific schemas.
/// 一个不携带 Provider 专属 Schema 的精确供应商工具目标选择。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolTargetConfig {
    /// Credential connection exposing the selected target.
    /// 暴露所选目标的凭据连接。
    pub connection_id: String,
    /// Provider-declared target identity.
    /// Provider 声明的目标身份。
    pub target_id: String,
}

/// One direct capability-to-target selection in canonical capability order.
/// 一项按规范能力顺序排列的直接能力到目标选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolCapabilitySelection {
    /// Stable semantic capability key.
    /// 稳定的语义能力键。
    pub capability: String,
    /// Exact selected target.
    /// 精确选中的目标。
    pub target: ProviderToolTargetConfig,
}

/// Backend family selected for one media-understanding capability.
/// 一项媒体理解能力选择的后端类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMediaUnderstandingMode {
    /// Use the Provider-owned understanding tool.
    /// 使用 Provider 持有的理解工具。
    ProviderTool,
    /// Use a replacement multimodal model.
    /// 使用替代多模态模型。
    ModelReplacement,
}

/// Exact non-secret model route selected for one session media-understanding capability.
/// 为一项会话媒体理解能力选择的精确非敏感模型路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderMediaUnderstandingModelTargetConfig {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Exact provider model identifier.
    /// 精确供应商模型标识。
    pub model_id: String,
    /// Explicit credential connection identifier.
    /// 显式凭据连接标识。
    pub connection_id: String,
}

/// Exact backend route selected for one session media-understanding capability.
/// 为一项会话媒体理解能力选择的精确后端路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProviderMediaUnderstandingRouteConfig {
    /// Exact Manifest-owned provider tool target.
    /// 精确的 Manifest 供应商工具目标。
    ProviderTool(ProviderToolTargetConfig),
    /// Exact replacement model and account route.
    /// 精确的替代模型与账号路由。
    ModelReplacement(ProviderMediaUnderstandingModelTargetConfig),
}

/// One provider-neutral reasoning effort persisted for a session media replacement.
/// 为会话媒体替代模型持久化的一项供应商无关推理档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMediaUnderstandingReasoningEffort {
    /// Explicitly disable reasoning.
    /// 显式关闭推理。
    None,
    /// Use provider-managed adaptive reasoning.
    /// 使用供应商管理的自适应推理。
    Adaptive,
    /// Use the smallest declared reasoning effort.
    /// 使用已声明的最小推理档位。
    Minimal,
    /// Use a low reasoning effort.
    /// 使用低推理档位。
    Low,
    /// Use a balanced reasoning effort.
    /// 使用中等推理档位。
    Medium,
    /// Use a high reasoning effort.
    /// 使用高推理档位。
    High,
    /// Use an extra-high reasoning effort.
    /// 使用超高推理档位。
    #[serde(rename = "xhigh", alias = "x_high", alias = "x-high")]
    XHigh,
    /// Use the provider-declared maximum reasoning effort.
    /// 使用供应商声明的最大推理档位。
    Max,
    /// Use a provider-specific ultra reasoning effort.
    /// 使用供应商特有的极限推理档位。
    Ultra,
}

/// One media capability backend selection in canonical capability order.
/// 一项按规范能力顺序排列的媒体能力后端选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderMediaUnderstandingSelection {
    /// Stable semantic capability key.
    /// 稳定的语义能力键。
    pub capability: String,
    /// Exact backend family selected for the capability.
    /// 为该能力选择的精确后端类型。
    pub mode: ProviderMediaUnderstandingMode,
    /// Exact route owned by this session; absent only for legacy mode-only metadata.
    /// 当前会话持有的精确路由；仅旧版只含模式的元数据可以缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<ProviderMediaUnderstandingRouteConfig>,
    /// Optional session override for the selected replacement model's reasoning effort.
    /// 所选替代模型的可选会话级推理档位覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ProviderMediaUnderstandingReasoningEffort>,
}

/// One native model-extension capability and its ordered target set.
/// 一项原生模型扩展能力及其有序目标集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderExtensionTargetSetConfig {
    /// Stable semantic extension capability key.
    /// 稳定的语义扩展能力键。
    pub capability: String,
    /// Canonically ordered exact targets enabled for this capability.
    /// 为该能力启用的按规范顺序排列的精确目标。
    pub targets: Vec<ProviderToolTargetConfig>,
}

/// Complete provider-tool selection owned by one session.
/// 一个会话持有的完整供应商工具选择。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolSelectionConfig {
    /// Whether global defaults have been materialized into this session.
    /// 全局默认值是否已经物化到当前会话。
    #[serde(default, skip_serializing_if = "is_false")]
    pub initialized: bool,
    /// Whether this session selection was explicitly saved after default materialization.
    /// 当前会话选择是否在默认值物化后被显式保存。
    #[serde(default, skip_serializing_if = "is_false")]
    pub overridden: bool,
    /// Enabled semantic capability keys in canonical order.
    /// 按规范顺序排列的已启用语义能力键。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enabled_capabilities: Vec<String>,
    /// Direct capability target selections in canonical order.
    /// 按规范顺序排列的直接能力目标选择。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selections: Vec<ProviderToolCapabilitySelection>,
    /// Media backend selections in canonical order.
    /// 按规范顺序排列的媒体后端选择。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[serde(alias = "media_understanding_modes")]
    pub media_understanding_selections: Vec<ProviderMediaUnderstandingSelection>,
    /// Native extension target sets in canonical order.
    /// 按规范顺序排列的原生扩展目标集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extension_target_sets: Vec<ProviderExtensionTargetSetConfig>,
}

impl ProviderToolSelectionConfig {
    /// Validates this selection without loading a session document or any Turn.
    /// 校验当前选择，不加载会话文档或任何回合。
    /// Returns the same canonical-configuration error used by runtime mutations.
    /// 返回与运行时修改相同的规范配置错误。
    pub fn validate(&self) -> Result<(), super::errors::DomainError> {
        super::document::validate_provider_tool_selection_config(self)
    }

    /// Returns whether no provider-tool selection fact has been materialized.
    /// 返回是否尚未物化任何供应商工具选择事实。
    pub fn is_empty(&self) -> bool {
        !self.initialized
            && !self.overridden
            && self.enabled_capabilities.is_empty()
            && self.selections.is_empty()
            && self.media_understanding_selections.is_empty()
            && self.extension_target_sets.is_empty()
    }
}

/// Session policy for selecting project-authorized knowledge libraries.
/// 会话选择项目已授权知识库时使用的策略。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeLibrarySelectionMode {
    /// Inherit the project default-enabled libraries.
    /// 继承项目默认启用的知识库。
    #[default]
    Inherit,
    /// Use only the explicitly selected authorized libraries.
    /// 仅使用显式选择且已授权的知识库。
    Explicit,
}

/// Session-selected retrieval policy for knowledge tools.
/// 会话为知识工具选择的检索策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRetrievalMode {
    /// Return raw BM25 matches.
    /// 返回原始 BM25 匹配结果。
    RawBm25,
    /// Return display-oriented BM25 matches.
    /// 返回面向展示的 BM25 匹配结果。
    DisplayBm25,
    /// Return display-oriented hybrid matches.
    /// 返回面向展示的混合检索结果。
    DisplayHybrid,
}

/// Complete session-owned knowledge-library selection.
/// 当前会话持有的完整知识库选择。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeLibrarySelectionConfig {
    /// Selection mode for project-authorized libraries.
    /// 项目已授权知识库的选择模式。
    pub mode: KnowledgeLibrarySelectionMode,
    /// Canonically ordered explicit library identifiers.
    /// 按规范顺序排列的显式知识库标识。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub library_ids: Vec<String>,
    /// Optional retrieval policy selected for knowledge tools.
    /// 为知识工具选择的可选检索策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_mode: Option<KnowledgeRetrievalMode>,
}

impl KnowledgeLibrarySelectionConfig {
    /// Returns whether this selection is the project-inherited default.
    /// 返回该选择是否为继承项目配置的默认状态。
    pub fn is_default(&self) -> bool {
        self.mode == KnowledgeLibrarySelectionMode::Inherit
            && self.library_ids.is_empty()
            && self.retrieval_mode.is_none()
    }
}

/// Session-level VMM behavior overrides owned by metadata.
/// 由 metadata 持有的会话级 VMM 行为覆盖。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmmSessionPolicyConfig {
    /// Optional precheck-memory injection override.
    /// 可选的预检记忆注入覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precheck_enabled: Option<bool>,
    /// Optional completed-turn memory writeback override.
    /// 可选的已完成回合记忆回写覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_writeback_enabled: Option<bool>,
}

impl VmmSessionPolicyConfig {
    /// Returns whether this session inherits both VMM behavior values.
    /// 返回该会话是否同时继承两项 VMM 行为配置。
    pub fn is_empty(&self) -> bool {
        self.precheck_enabled.is_none() && self.memory_writeback_enabled.is_none()
    }
}

/// Latest user selections for capabilities and personalities.
/// 用户在能力与人格上的最新选择。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionConfig {
    /// Latest agent-persona selection.
    /// 最新 Agent 人格选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_persona: Option<String>,
    /// Latest output-style selection.
    /// 最新输出风格选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_style: Option<String>,
    /// Complete session-owned knowledge-library selection.
    /// 当前会话持有的完整知识库选择。
    #[serde(
        default,
        skip_serializing_if = "KnowledgeLibrarySelectionConfig::is_default"
    )]
    pub knowledge_libraries: KnowledgeLibrarySelectionConfig,
    /// Complete session-owned Provider tool configuration.
    /// 当前会话持有的完整 Provider 工具配置。
    #[serde(default, skip_serializing_if = "ProviderToolSelectionConfig::is_empty")]
    pub provider_tools: ProviderToolSelectionConfig,
    /// Explicit capability enablements.
    /// 显式启用的能力集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub explicit_capabilities: Vec<String>,
    /// Media-understanding backend selection.
    /// 媒体理解后端选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_understanding_backend: Option<String>,
    /// Prompt-skill session-level overrides.
    /// 提示词技能会话级覆盖。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompt_skill_overrides: Vec<PromptSkillOverride>,
    /// Session-level VMM behavior overrides.
    /// 会话级 VMM 行为覆盖。
    #[serde(default, skip_serializing_if = "VmmSessionPolicyConfig::is_empty")]
    pub vmm: VmmSessionPolicyConfig,
    /// Web-search enable flag.
    /// 网页搜索启用标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_search_enabled: Option<bool>,
    /// Media understanding enable flag.
    /// 媒体理解启用标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_understanding_enabled: Option<bool>,
}

/// Session-level configuration persisted inside `metadata.config`.
/// 保存在 `metadata.config` 内的会话级配置。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionConfig {
    /// Desired HTML answer preference and the last request-preparation snapshot.
    /// HTML 回答期望偏好和最近一次请求准备快照。
    pub html_answers: HtmlAnswerMode,
    /// Request configuration effective for the active execution node.
    /// 对当前活动执行节点生效的请求配置。
    pub active_request: ActiveRequestConfig,
    /// Pending request change applied at the next admitted user or Continue boundary.
    /// 将在下一个已准入用户输入或 Continue 边界生效的请求变更。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_request: Option<PendingRequestConfig>,
    /// Latest user selections.
    /// 用户最新选择。
    pub selection: SelectionConfig,
    /// Stable context-compaction window node selected for the next safe request boundary.
    /// 为下一个安全请求边界选择的稳定上下文压缩窗口节点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_compaction_window_node_id: Option<ContextCompactionWindowNodeId>,
}

/// Immutable HTML preference captured at a business request's preparation boundary.
/// 在业务请求准备边界捕获的不可变 HTML 偏好。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HtmlAnswerPreparedMode {
    /// Whether the prepared request allows optional HTML answers.
    /// 准备好的请求是否允许按需 HTML 回答。
    pub enabled: bool,
    /// Exact preference revision captured for this preparation.
    /// 此次准备捕获的精确偏好修订号。
    pub revision: u64,
}

/// Session-owned desired preference, independent of Goal/Plan and of an already active API.
/// 会话持有的期望偏好，独立于 Goal、Plan 和已经活动的 API。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "HtmlAnswerModeWire")]
pub struct HtmlAnswerMode {
    /// Latest explicitly saved user preference.
    /// 最近显式保存的用户偏好。
    pub enabled: bool,
    /// Monotonic compare-and-set revision; defaults to zero only for newly created state.
    /// 单调比较交换修订号；仅新建状态默认为零。
    pub revision: u64,
    /// Last business preparation, absent until this session prepares its first request.
    /// 最近业务请求准备；此会话首次准备请求之前为空。
    pub prepared: Option<HtmlAnswerPreparedMode>,
}

/// Required wire fields are separate from validation; no serde defaults repair malformed v3 data.
/// 必填传输字段与校验分开；不以 serde 默认值修复损坏的 v3 数据。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HtmlAnswerModeWire {
    /// Serialized desired preference.
    /// 序列化的期望偏好。
    enabled: bool,
    /// Serialized compare-and-set revision.
    /// 序列化的比较交换修订号。
    revision: u64,
    /// Serialized explicit preparation state; the field itself is required.
    /// 序列化的显式准备状态；字段自身必填。
    #[serde(deserialize_with = "deserialize_required_html_preparation")]
    prepared: Option<HtmlAnswerPreparedMode>,
}

/// Decode a present nullable preparation field; missing state is a schema error.
/// 解码存在且可为空的准备字段；缺失状态是模式错误。
fn deserialize_required_html_preparation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<HtmlAnswerPreparedMode>, D::Error> {
    Option::deserialize(deserializer)
}

impl TryFrom<HtmlAnswerModeWire> for HtmlAnswerMode {
    type Error = &'static str;

    /// Validate JS-safe revisions and agreement at an identical desired/prepared revision.
    /// 校验 JavaScript 安全修订号，以及相同期望和准备修订下的一致性。
    /// Returns the typed mode or a stable corruption diagnostic, without changing stored values.
    /// 返回强类型模式或稳定的损坏诊断，不改变存储值。
    fn try_from(wire: HtmlAnswerModeWire) -> Result<Self, Self::Error> {
        if wire.revision > crate::v1::ids::JS_SAFE_INTEGER_MAX
            || wire.prepared.is_some_and(|prepared| {
                prepared.revision > wire.revision
                    || (prepared.revision == wire.revision && prepared.enabled != wire.enabled)
            })
        {
            return Err("invalid HTML answer preference revision");
        }
        Ok(Self {
            enabled: wire.enabled,
            revision: wire.revision,
            prepared: wire.prepared,
        })
    }
}

/// Stable identity of one queued input across metadata mutations.
/// 一条排队输入在元数据修改间保持稳定的身份。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedInputIdentity {
    /// External client action that originally enqueued the input.
    /// 最初把该输入加入队列的外部客户端动作。
    pub client_action_id: ClientActionId,
    /// Session-local pending-input identity.
    /// 会话本地待处理输入身份。
    pub input_id: PendingInputId,
}

/// Input draft state.
/// 输入草稿状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftState {
    /// Immutable prompt preparation retained for the same ordered attachment intentions, including text-only edits.
    /// 为相同有序附件意图保留的不可变提示词准备，包括仅正文编辑。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<crate::v1::UserPromptParameters>,
    /// Draft text.
    /// 草稿正文。
    pub text: String,
    /// Ordered project resource ids attached to the draft.
    /// 附着在草稿上的有序项目资源 id。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_ids: Vec<ResourceId>,
    /// Exact staged asset id per resource occurrence; empty means legacy or project-only attachments without staged lineage.
    /// 每个资源出现的精确暂存资产ID；空数组表示旧记录或没有暂存来源的纯项目附件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub staged_attachment_ids: Vec<Option<String>>,
    /// Exact accepted-input source per project-resource occurrence; empty means no verifiable source references.
    /// 每个项目资源出现的精确已接纳输入来源；空数组表示没有可验证来源引用。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_sources: Vec<Option<crate::v1::UserPromptResourceSource>>,
    /// Queue identity retained only while this exact draft was restored from a queued input.
    /// 仅当当前精确草稿由排队输入恢复时保留的队列身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_from_queue: Option<QueuedInputIdentity>,
}

/// One in-turn supplementary input bound to the open turn.
/// 一条绑定开放回合的过程内补充输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingInTurnInput {
    /// Prompt-owned preparation captured at admission, never recomputed from the later active model.
    /// 准入时捕获的提示词所有准备，绝不根据后续活动模型重算。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<crate::v1::UserPromptParameters>,
    /// Exact request configuration bound when this user input was accepted for interjection.
    /// 本用户输入被接纳插队时绑定的精确请求配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_request_config: Option<ActiveRequestConfig>,
    /// Stable external idempotency identity of the client action.
    /// 客户端动作的稳定外部幂等身份。
    pub client_action_id: ClientActionId,
    /// Stable input id.
    /// 稳定输入 id。
    pub input_id: PendingInputId,
    /// Open turn this input belongs to.
    /// 该输入所属的开放回合。
    pub turn_id: TurnId,
    /// Supplementary text.
    /// 补充正文。
    pub text: String,
    /// Ordered project resource ids.
    /// 有序项目资源 id。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_ids: Vec<ResourceId>,
    /// Creation time in Unix milliseconds.
    /// 创建时间(Unix 毫秒)。
    pub created_at_ms: i64,
}

/// One queued future-turn input.
/// 一条排队中的未来回合输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedInput {
    /// Prompt-owned preparation captured before queueing; missing legacy data stays unknown.
    /// 排队之前捕获的提示词所有准备；缺失旧数据保持未知。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<crate::v1::UserPromptParameters>,
    /// Exact request configuration bound when this future user Turn entered the queue.
    /// 本未来用户回合进入队列时绑定的精确请求配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_request_config: Option<ActiveRequestConfig>,
    /// Stable external idempotency identity of the client action.
    /// 客户端动作的稳定外部幂等身份。
    pub client_action_id: ClientActionId,
    /// Stable input id.
    /// 稳定输入 id。
    pub input_id: PendingInputId,
    /// Queued text.
    /// 排队正文。
    pub text: String,
    /// Ordered project resource ids.
    /// 有序项目资源 id。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_ids: Vec<ResourceId>,
    /// Creation time in Unix milliseconds.
    /// 创建时间(Unix 毫秒)。
    pub created_at_ms: i64,
}

/// Input area state persisted inside `metadata.input_state`.
/// 保存在 `metadata.input_state` 内的输入区状态。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputState {
    /// Input draft text and ordered resources.
    /// 输入草稿正文与有序资源。
    pub draft: DraftState,
    /// Supplementary inputs bound to the open turn, not yet streamed.
    /// 绑定开放回合、尚未流入主事件的过程内补充。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_in_turn_inputs: Vec<PendingInTurnInput>,
    /// Future-turn queued inputs.
    /// 未来回合的排队输入。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued_inputs: Vec<QueuedInput>,
    /// Resolved queue actions whose transient attachment owners still require durable release.
    /// 队列已处置、瞬时附件所有者仍需可靠释放的动作身份。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub pending_staging_release_actions: BTreeSet<ClientActionId>,
    /// Durable terminal settlement intent, present only while one exact terminal boundary has not finished settling.
    /// 可靠终态结算意图；仅当一个精确终态边界尚未完成结算时存在。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_settlement: Option<TerminalSettlementRecord>,
}

/// Recoverable state of one durable terminal settlement.
/// 一次可靠终态结算的可恢复状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalSettlementState {
    /// The settlement intent is durable and its terminal write has not succeeded yet.
    /// 结算意图已可靠落盘，其终态写入尚未成功。
    #[default]
    Pending,
    /// A terminal write failed and the exact session needs an explicit controlled retry.
    /// 终态写入失败，该精确会话需要一次显式受控重试。
    RepairRequired,
}

/// Exact execution scope allowed to settle after a stream failure.
/// 流失败后允许结算的精确执行范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalSettlementScope {
    /// Settle the original business turn through its existing failure reducer.
    /// 通过既有失败归约器结算原业务回合。
    BusinessTurn,
    /// Settle only the named auxiliary execution, retaining any parent business turn.
    /// 仅结算指定辅助执行，保留其父业务回合。
    QuestionExecution {
        /// Durable execution identity captured before stream ownership is released.
        /// 释放流所有权前捕获的可靠执行身份。
        execution_id: CollaborationExecutionId,
        /// Exact unfinished physical API, or none when no API remains active.
        /// 精确未完成物理 API；无活动 API 时为空。
        api_call_id: Option<ApiCallId>,
    },
}

/// Durable, recoverable settlement intent for one exact terminal boundary.
/// 一个精确终态边界的可靠可恢复结算意图。
///
/// The intent is written before the dangerous terminal persistence phase, so an
/// interrupted process restart never has to guess whether the terminal fact
/// landed: the document itself carries the pending settlement and keeps the
/// session closed until an idempotent retry resolves it.
/// 该意图在危险的终态持久化阶段之前写入，因此被中断的进程重启绝不需猜测终态
/// 是否落盘：文档自身携带待结算项，并在幂等重试完成前保持会话关闭。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalSettlementRecord {
    /// Stable diagnostic identity of this settlement intent.
    /// 本结算意图的稳定诊断身份。
    pub settlement_id: String,
    /// Exact session owning the settlement.
    /// 持有本次结算的精确会话。
    pub session_id: String,
    /// Exact logical turn whose terminal fact is being settled.
    /// 正在结算终态事实的精确逻辑回合。
    pub turn_id: TurnId,
    /// Required business or auxiliary scope; never inferred from mutable UI state.
    /// 必需的业务或辅助范围，绝不根据可变 UI 状态推断。
    pub scope: TerminalSettlementScope,
    /// Exact in-process terminal-persistence owner generation identity.
    /// 精确的进程内终态持久化所有者代际身份。
    pub owner_generation: String,
    /// Current recoverable settlement state.
    /// 当前可恢复结算状态。
    pub state: TerminalSettlementState,
    /// Stable redacted failure identity used by the idempotent retry.
    /// 幂等重试使用的稳定脱敏失败标识。
    pub reason_code: String,
    /// Last settlement update time in Unix milliseconds.
    /// 最后结算更新时间(Unix 毫秒)。
    pub updated_at_ms: i64,
}

/// One physical segment row of the current generation split map.
/// 当前 generation 分片映射中的一行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentInfo {
    /// 1-based segment number within the generation.
    /// generation 内以 1 为起点的分片序号。
    pub segment_number: u64,
    /// Exact segment file name.
    /// 精确分片文件名。
    pub file_name: String,
    /// First event seq of this segment.
    /// 该分片的第一个事件 seq。
    pub start_seq: Seq,
    /// Terminal seq of a sealed segment, or `None` for the active tail.
    /// 封口分片的终止 seq；活动尾段为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_seq: Option<Seq>,
    /// Event count of this segment.
    /// 该分片事件数。
    pub event_count: u64,
    /// Byte length of this segment file.
    /// 该分片文件字节数。
    pub byte_len: u64,
    /// Next local id after all event-referenced ids in this segment prefix.
    /// 该分片前缀中全部事件引用 id 之后的下一个本地 id。
    pub next_event_local_id: u64,
}

/// Cached id/seq frontier used only as an allocator accelerator.
/// 仅作分配器加速用的 id/seq frontier 缓存。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdFrontier {
    /// Next available local entity id.
    /// 下一个可用本地实体 id。
    pub next_local_id: u64,
    /// Next available stream seq.
    /// 下一个可用流 seq。
    pub next_seq: u64,
}

/// Immutable identity of the exact source boundary that created a forked session.
/// 创建分叉会话的精确来源边界不可变身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForkOrigin {
    /// Parent session UUID.
    /// 父会话 UUID。
    pub source_session_id: String,
    /// Parent event-tree generation observed by the fork request.
    /// 分叉请求观察到的父事件树 generation。
    pub source_segment_generation: u64,
    /// Retained parent terminal TurnStats seq that became the child completed-history boundary.
    /// 成为子会话完整历史边界的父级终态 TurnStats seq。
    pub source_target_seq: Seq,
}

/// The full V1 session metadata document.
/// 完整 V1 会话元数据文档。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataDocument {
    /// File contract marking the V1 metadata format.
    /// 标记 V1 元数据格式的文件契约。
    pub contract: String,
    /// Schema version of the document.
    /// 文档的 schema 版本。
    pub schema_version: u32,
    /// Cross-system session UUID.
    /// 跨系统会话 UUID。
    pub session_id: String,
    /// Immutable source boundary for a forked session; root sessions store `None`.
    /// 分叉会话的不可变来源边界；根会话保存 `None`。
    pub fork_origin: Option<ForkOrigin>,
    /// Project tag used for human-readable partitioning.
    /// 用于人类可读分区的项目标识。
    pub project_tag: String,
    /// Creation time in Unix milliseconds.
    /// 创建时间(Unix 毫秒)。
    pub created_at_ms: i64,
    /// Last metadata write time in Unix milliseconds.
    /// 最后写入时间(Unix 毫秒)。可落后于已原子发布的活动尾段，不可反向截断。
    pub updated_at_ms: i64,
    /// Exact complete document revision published with this metadata snapshot.
    /// 与该 metadata 快照一同完整发布的精确文档修订号。
    pub document_revision: u64,
    /// Archive time in Unix milliseconds, if archived.
    /// 归档时间(Unix 毫秒)；未归档时为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at_ms: Option<i64>,
    /// Session display title.
    /// 会话显示标题。
    pub title: String,
    /// Session mode switches.
    /// 会话模式开关。
    pub mode: ModeConfig,
    /// Session-level configuration.
    /// 会话级配置。
    pub config: SessionConfig,
    /// Input-area state.
    /// 输入区状态。
    pub input_state: InputState,
    /// Current generation id of the segment tree.
    /// 分片树的当前 generation id。
    pub segment_generation: u64,
    /// Current immutable context generation containing runtime and compactions.
    /// 当前包含 runtime 与 compactions 的不可变上下文 generation。
    pub active_context_generation: u64,
    /// Cached start seq of the currently open Turn, if one crosses the active
    /// tail boundary. The event stream remains authoritative; this cursor only
    /// bounds precise startup reads.
    /// 当前开放回合的起始 seq 缓存；当回合跨越活动尾段边界时用于精确限定启动读取。
    /// 主事件流仍是权威事实，该游标不保存任何转录内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_turn_start_seq: Option<Seq>,
    /// Exact split map of the current generation.
    /// 当前 generation 的精确分片映射。
    pub split_list: Vec<SegmentInfo>,
    /// Allocator frontier cache.
    /// 分配器 frontier 缓存。
    pub id_frontier: IdFrontier,
    /// Required typed collaboration recovery state and event coverage marker.
    /// 协作领域必需的强类型恢复状态与事件覆盖标记。
    pub collaboration: CollaborationMetadata,
    /// Latest typed state for each request dynamic-context producer.
    /// 每个请求动态上下文生产者的最新强类型状态。
    pub dynamic_context: BTreeMap<String, super::dynamic_context::DynamicContextState>,
    /// Resolved child execution profile, absent for ordinary sessions and unprepared children.
    /// 已解析子执行配置；普通会话及尚未准备的子代理没有此记录。
    pub child_execution_profile: Option<super::child_execution::ChildExecutionProfile>,
    /// Latest auxiliary maintenance ownership; business completion remains independent.
    /// 最近辅助维护归属；业务完成保持独立。
    pub context_maintenance: Option<super::context_maintenance::ContextMaintenanceExecution>,
    /// Initial child creation sources published atomically with their parent reservations.
    /// 与父预留原子发布的初始子创建来源。
    pub child_creation_intents: BTreeMap<
        super::collaboration::CollaborationExecutionId,
        super::child_execution::ChildCreationIntent,
    >,
    /// Required main-session Hook admission binding; empty until its first preparation.
    /// 必需主会话 Hook 准入绑定；首次准备之前为空。
    pub business_hook: super::business_hook::BusinessHookMetadata,
}

impl MetadataDocument {
    /// Builds a fresh empty metadata document with the current contract.
    /// 使用当前契约构建一份全新的空元数据文档。
    pub fn new(
        session_id: impl Into<String>,
        project_tag: impl Into<String>,
        title: impl Into<String>,
        created_at_ms: i64,
    ) -> Self {
        Self {
            contract: METADATA_CONTRACT.to_string(),
            schema_version: METADATA_SCHEMA_VERSION,
            session_id: session_id.into(),
            fork_origin: None,
            project_tag: project_tag.into(),
            created_at_ms,
            updated_at_ms: created_at_ms,
            document_revision: 0,
            archived_at_ms: None,
            title: title.into(),
            mode: ModeConfig::default(),
            config: SessionConfig::default(),
            input_state: InputState::default(),
            segment_generation: 1,
            active_context_generation: 1,
            open_turn_start_seq: None,
            split_list: Vec::new(),
            id_frontier: IdFrontier {
                next_local_id: 1,
                next_seq: 1,
            },
            collaboration: CollaborationMetadata::new(),
            dynamic_context: BTreeMap::new(),
            child_execution_profile: None,
            context_maintenance: None,
            child_creation_intents: BTreeMap::new(),
            business_hook: super::business_hook::BusinessHookMetadata::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The metadata document serializes with the exact frozen contract values.
    /// 元数据文档使用精确冻结的契约值序列化。
    #[test]
    fn metadata_contract_round_trip() {
        let document = MetadataDocument::new(
            "session-uuid",
            "project-tag",
            "default title",
            1_700_000_000_000,
        );
        let json = serde_json::to_string(&document).unwrap();
        assert!(json.contains(r#""contract":"vulcan.session.v1""#));
        assert_eq!(document.schema_version, METADATA_SCHEMA_VERSION);
        let decoded: MetadataDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, document);
    }

    /// Unknown fields in the metadata document are rejected.
    /// 元数据文档中的未知字段被拒绝。
    #[test]
    fn metadata_rejects_unknown_fields() {
        let document = MetadataDocument::new("s", "p", "t", 1);
        let mut json: serde_json::Value = serde_json::to_value(&document).unwrap();
        json["unexpected_field"] = serde_json::Value::Bool(true);
        let result: Result<MetadataDocument, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }
}
