//! Memory domain entities independent of host hierarchy and transport.
//! 与宿主层级和传输协议无关的记忆领域实体。

use crate::{
    MemoryId, ProfileNodeId, ProfileSourceId, ScopeId, SessionSourceId, SubjectId, TurnId,
    VectorSpaceId,
};
use serde::{Deserialize, Serialize};

/// Lowest normalized priority labelled P0.
/// 标为 P0 的最低规范化优先级。
pub const PRIORITY_P0_FLOOR: u32 = 80;

/// Lowest normalized priority labelled P1; anything lower is P2.
/// 标为 P1 的最低规范化优先级；更低的为 P2。
pub const PRIORITY_P1_FLOOR: u32 = 50;

/// Maps a normalized priority onto the P0/P1/P2 label every surface shows.
/// 将规范化优先级映射为所有界面共用的 P0/P1/P2 标签。
#[must_use]
pub const fn priority_band_label(priority: u32) -> &'static str {
    if priority >= PRIORITY_P0_FLOOR {
        "P0"
    } else if priority >= PRIORITY_P1_FLOOR {
        "P1"
    } else {
        "P2"
    }
}

/// Defines the maximum number of visible scopes carried by one public command.
/// 定义一个公共命令可携带的最大可见作用域数量。
pub const MAX_VISIBLE_SCOPES: usize = 256;

/// Defines the maximum number of searchable tags carried by one memory.
/// 定义一条记忆可携带的最大可搜索标签数量。
pub const MAX_MEMORY_TAGS: usize = 256;

/// Defines the maximum UTF-8 byte length of one searchable memory tag.
/// 定义一个可搜索记忆标签的最大 UTF-8 字节长度。
pub const MAX_MEMORY_TAG_BYTES: usize = 512;

/// Defines the maximum UTF-8 byte length of one public domain text field.
/// 定义一个公共领域文本字段的最大 UTF-8 字节长度。
pub const MAX_DOMAIN_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// Defines the maximum item count of one public durable-record collection.
/// 定义一个公共持久记录集合的最大项目数量。
pub const MAX_DOMAIN_COLLECTION_ITEMS: usize = 10_000;

/// Represents Unix time in nanoseconds.
/// 表示纳秒级Unix时间。
pub type UnixNanos = i64;

/// Describes the author role of one timeline item.
/// 描述一条时间线项目的作者角色。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u32)]
pub enum Role {
    /// Represents user-authored content.
    /// 表示用户编写的内容。
    User = 1,
    /// Represents assistant-authored content.
    /// 表示助手编写的内容。
    Assistant = 2,
    /// Represents system-authored content.
    /// 表示系统编写的内容。
    System = 3,
    /// Represents tool-authored content.
    /// 表示工具编写的内容。
    Tool = 4,
}

/// Carries one canonical timeline item.
/// 携带一条规范化时间线项目。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineItem {
    /// Identifies the author role.
    /// 标识作者角色。
    pub role: Role,
    /// Carries normalized UTF-8 text.
    /// 携带规范化 UTF-8 文本。
    pub content: String,
    /// Carries the event time in Unix nanoseconds.
    /// 携带纳秒级Unix事件时间。
    pub occurred_at_ns: UnixNanos,
}

/// Describes the durable processing state of one canonical turn.
/// 描述一个规范化对话轮次的持久处理状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum TurnStatus {
    /// Indicates a durably persisted turn awaiting background analysis.
    /// 表示已持久化并等待后台分析的轮次。
    Pending = 1,
    /// Indicates a turn whose analysis and derived writes completed atomically.
    /// 表示分析与派生写入已原子完成的轮次。
    Completed = 2,
    /// Indicates a historical turn known to be processed while its successful-analysis metadata remains incomplete or unknowable.
    /// 表示已知完成处理、但成功分析元数据不完整或不可恢复的历史轮次。
    ProcessedUnknown = 3,
    /// Indicates a turn released after repeated analysis failures under an explicit Host policy.
    /// 表示在明确宿主策略下因重复分析失败而放行的轮次。
    AnalysisBypassed = 4,
    /// Indicates a turn whose durable payload was unreadable and has been isolated from analysis.
    /// 表示长期载荷不可读并已从分析流程隔离的轮次。
    Corrupted = 5,
}

/// Classifies the dominant user act reported by the authoritative turn-analysis contract.
/// 对权威轮次分析契约报告的主要用户行为进行分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum TurnInputKind {
    /// Indicates a question-dominant turn.
    /// 表示以提问为主的轮次。
    Question = 1,
    /// Indicates a statement, confirmation, or correction-dominant turn.
    /// 表示以陈述、确认或纠正为主的轮次。
    Statement = 2,
    /// Indicates a turn mixing multiple user acts.
    /// 表示混合多类用户行为的轮次。
    Mixed = 3,
}

/// Describes one canonical conversation turn.
/// 描述一个规范化对话轮次。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// Identifies the turn.
    /// 标识对话轮次。
    pub id: TurnId,
    /// Identifies the resolved subject.
    /// 标识已解析主体。
    pub subject_id: SubjectId,
    /// Identifies the write scope.
    /// 标识写入作用域。
    pub scope_id: ScopeId,
    /// Identifies the host session source.
    /// 标识宿主会话来源。
    pub session_source_id: SessionSourceId,
    /// Carries ordered timeline items.
    /// 携带有序时间线项目。
    pub timeline: Vec<TimelineItem>,
    /// Carries caller-authoritative preceding turn identities in display order.
    /// 按展示顺序携带调用方权威的前序轮次标识。
    pub previous_turn_ids: Vec<TurnId>,
    /// Carries caller-authoritative following turn identities in display order.
    /// 按展示顺序携带调用方权威的后序轮次标识。
    pub next_turn_ids: Vec<TurnId>,
    /// Carries the canonical creation time.
    /// 携带规范化创建时间。
    pub created_at_ns: UnixNanos,
    /// Carries the durable processing state of the turn.
    /// 携带轮次的持久处理状态。
    pub status: TurnStatus,
    /// Carries the dominant user act after successful analysis.
    /// 在成功分析后携带主要用户行为。
    pub user_input_kind: Option<TurnInputKind>,
    /// Carries extracted details after analysis, which may be empty when the result has no new information.
    /// 携带分析后的提炼详情，无新增信息时允许为空字符串。
    pub analysis_details: Option<String>,
}

impl Turn {
    /// Checks the analysis metadata against this turn's state and the persisted text bounds.
    /// 按当前轮次状态和持久文本边界校验分析元数据。
    /// An explicit empty result is valid for a completed analysis; missing metadata still fails.
    /// 已完成的分析允许显式空结果，但仍拒绝缺失元数据。
    #[must_use]
    pub fn has_valid_analysis_metadata(&self) -> bool {
        let details = self.analysis_details.as_deref();
        let status_matches = match self.status {
            TurnStatus::Pending | TurnStatus::AnalysisBypassed | TurnStatus::Corrupted => {
                self.user_input_kind.is_none() && details.is_none()
            }
            TurnStatus::Completed => self.user_input_kind.is_some() && details.is_some(),
            TurnStatus::ProcessedUnknown => self.user_input_kind.is_none(),
        };
        status_matches
            && details.is_none_or(|text| {
                text.len() <= crate::MAX_DOMAIN_TEXT_BYTES
                    && !text.contains('\0')
                    && (!text.trim().is_empty()
                        || (self.status == TurnStatus::Completed && text.is_empty()))
            })
    }
}

/// Classifies the representation carried by one recent turn in `PreCheck`.
/// 对 `PreCheck` 最近轮次携带的内容表示进行分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum PreCheckTurnContentType {
    /// Carries the successful compact extraction of one completed turn.
    /// 携带一个已完成轮次成功生成的紧凑提炼内容。
    Details = 1,
    /// Carries compact raw turn JSON when no successful extraction is available.
    /// 在没有成功提炼结果时携带紧凑原始轮次 JSON。
    RawTurn = 2,
}

impl PreCheckTurnContentType {
    /// Returns the stable prompt token understood by the bundled VMM prompts.
    /// 返回内置 VMM 提示词理解的稳定标记。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Details => "DETAILS",
            Self::RawTurn => "RAW_TURN",
        }
    }

    /// Resolves one canonical discriminant without accepting aliases.
    /// 解析一个规范判别值且不接受别名。
    #[must_use]
    pub const fn try_from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Details),
            2 => Some(Self::RawTurn),
            _ => None,
        }
    }
}

/// Carries one same-session historical turn into the first `PreCheck` stage.
/// 将一个同会话历史轮次携带到 `PreCheck` 第一阶段。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreCheckTurnContext {
    /// Identifies the exact durable turn represented by this context.
    /// 标识该上下文表示的准确耐久轮次。
    pub turn_id: TurnId,
    /// Identifies whether content is extracted detail or compact raw turn JSON.
    /// 标识内容是提炼详情还是紧凑原始轮次 JSON。
    pub content_type: PreCheckTurnContentType,
    /// Carries the bounded context text selected by the host.
    /// 携带宿主选出的有界上下文文本。
    pub content: String,
}

/// Carries the durable compact boundary for one resolved host session.
/// 携带一个已解析宿主会话的持久压缩边界。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompactBoundary {
    /// Identifies the resolved subject owner.
    /// 标识已解析主体所有者。
    pub subject_id: SubjectId,
    /// Identifies the exact session write scope.
    /// 标识精确的会话写入作用域。
    pub scope_id: ScopeId,
    /// Identifies the host session source.
    /// 标识宿主会话来源。
    pub session_source_id: SessionSourceId,
    /// Identifies the latest persisted turn included by compaction.
    /// 标识压缩所包含的最新持久化轮次。
    pub compacted_turn_id: TurnId,
    /// Carries the caller-observed compaction time in Unix nanoseconds.
    /// 携带调用方观察到的纳秒级Unix压缩时间。
    pub compacted_at_ns: UnixNanos,
}

/// Classifies durable memory records using the authoritative VMM category set.
/// 使用 VMM 权威分类集合对持久记忆记录进行分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MemoryCategory {
    /// Represents general background facts.
    /// 表示通用背景事实。
    General = 0,
    /// Represents architecture decisions and rationale.
    /// 表示架构决策及其原因。
    ArchitectureDecision = 1,
    /// Represents technical specifications and APIs.
    /// 表示技术规格与接口。
    TechSpecApi = 2,
    /// Represents durable business logic and formulas.
    /// 表示持久业务逻辑与公式。
    BusinessLogic = 3,
    /// Represents requirements, TODOs, and commitments.
    /// 表示需求、待办与承诺。
    RequirementTodo = 4,
    /// Represents project and deployment context.
    /// 表示项目与部署情境。
    ProjectContext = 5,
    /// Represents known defects and technical debt.
    /// 表示已知缺陷与技术债。
    LogicalBugDebt = 6,
    /// Represents security boundaries and policy constraints.
    /// 表示安全边界与策略约束。
    SecurityPolicy = 7,
}

/// Classifies the authoritative origin of one durable memory.
/// 对一条持久记忆的权威来源进行分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MemorySourceKind {
    /// Indicates extraction from one canonical conversation turn.
    /// 表示从一个规范化对话轮次提取。
    Turn = 1,
    /// Indicates an explicit direct-memory write requested by the host.
    /// 表示由宿主请求的显式主动记忆写入。
    Direct = 2,
    /// Indicates restoration from one validated portable import.
    /// 表示从一份已校验的可移植导入中恢复。
    Import = 3,
    /// Indicates one deterministic system seed retained with its original provenance.
    /// 表示一条保留原始来源的确定性系统种子。
    SystemSeed = 4,
    /// Indicates one record migrated from the legacy VMM memory-entry store.
    /// 表示一条从旧版 VMM 记忆条目存储迁移而来的记录。
    Legacy = 5,
}

/// Classifies how strongly one memory should survive ordinary retention.
/// 对一条记忆应在常规保留策略中存续的强度进行分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MemoryStability {
    /// Indicates short-lived working memory.
    /// 表示短期工作记忆。
    Working = 1,
    /// Indicates durable memory expected to survive ordinary maintenance.
    /// 表示预期经受常规维护的持久记忆。
    Stable = 2,
    /// Indicates archival memory requiring explicit lifecycle action.
    /// 表示需要显式生命周期操作的归档记忆。
    Archival = 3,
}

/// Classifies the host-agnostic ownership horizon of one direct memory.
/// 对一条主动记忆与宿主层级无关的归属跨度进行分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MemoryScopeClass {
    /// Indicates memory owned only by one resolved session scope.
    /// 表示仅归属于一个已解析会话作用域的记忆。
    Session = 1,
    /// Indicates memory shared by one resolved environment scope.
    /// 表示由一个已解析环境作用域共享的记忆。
    Environment = 2,
    /// Indicates memory shared across environments for one resolved subject.
    /// 表示为一个已解析主体跨环境共享的记忆。
    Subject = 3,
}

/// Classifies how one durable memory is authorized against the requesting subject.
/// 分类一条持久记忆相对于请求主体的授权方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MemoryAccessClass {
    /// Requires the source subject to exist and equal the requesting subject.
    /// 要求来源主体存在并与请求主体相同。
    SubjectPrivate = 1,
    /// Uses the authorized owning scope and may retain an optional known source subject.
    /// 使用已授权所属作用域，并可保留一个可选的已知来源主体。
    ScopeShared = 2,
}

/// Classifies the lifecycle level of one durable memory.
/// 对一条持久记忆的生命周期等级分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MemoryLevel {
    /// Indicates session-scoped working memory.
    /// 表示会话级工作记忆。
    Session = 1,
    /// Indicates phase-bound memory inside one task arc.
    /// 表示单个任务弧线内的阶段记忆。
    Phase = 2,
    /// Indicates stable memory expected to outlive sessions.
    /// 表示预期跨越会话存续的稳定记忆。
    Stable = 3,
    /// Indicates persistent memory with the strongest retention.
    /// 表示具有最强保留的持久记忆。
    Persistent = 4,
}

/// Classifies the lifecycle level of one profile node.
/// 对一个画像节点的生命周期等级分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ProfileLevel {
    /// Indicates transient profile state.
    /// 表示瞬态画像状态。
    Transient = 1,
    /// Indicates situational profile state.
    /// 表示情境画像状态。
    Situational = 2,
    /// Indicates stable profile state.
    /// 表示稳定画像状态。
    Stable = 3,
    /// Indicates persistent profile state.
    /// 表示持久画像状态。
    Persistent = 4,
}

/// Describes the lifecycle state of one memory.
/// 描述一条记忆的生命周期状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MemoryState {
    /// Indicates an active memory visible to recall.
    /// 表示可被召回的活跃记忆。
    Active = 1,
    /// Indicates a superseded memory retained for evidence.
    /// 表示为证据保留的已替代记忆。
    Superseded = 2,
    /// Indicates a logically deleted memory retained until retention recycle.
    /// 表示在 retention 回收前保留的逻辑删除记忆。
    Deleted = 3,
    /// Indicates an expired memory retained until retention recycle.
    /// 表示在 retention 回收前保留的已过期记忆。
    Expired = 4,
    /// Indicates a memory moved into the finite trash window.
    /// 表示已迁入有限回收站窗口的记忆。
    Recycled = 5,
}

/// Describes one evidence reference supporting a memory.
/// 描述支撑记忆的一条证据引用。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// Identifies the source turn.
    /// 标识来源对话轮次。
    pub turn_id: TurnId,
    /// Carries a bounded evidence excerpt.
    /// 携带有界的证据摘录。
    pub excerpt: String,
    /// Carries the evidence event time.
    /// 携带证据事件时间。
    pub occurred_at_ns: UnixNanos,
}

/// Describes one durable memory record.
/// 描述一条持久记忆记录。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecord {
    /// Identifies the memory.
    /// 标识记忆。
    pub id: MemoryId,
    /// Classifies whether subject equality or authorized scope grants access to this memory.
    /// 分类该记忆是通过主体等值还是已授权作用域获得访问权。
    pub access_class: MemoryAccessClass,
    /// Identifies the originating subject when known; private memories require this value.
    /// 在已知时标识来源主体；私有记忆必须携带该值。
    pub source_subject_id: Option<SubjectId>,
    /// Identifies the owning scope.
    /// 标识所属作用域。
    pub scope_id: ScopeId,
    /// Classifies the generic ownership horizon used by read-time lifecycle policy.
    /// 分类读时生命周期策略使用的通用归属跨度。
    pub scope_class: MemoryScopeClass,
    /// Classifies the memory using the authoritative VMM category.
    /// 使用 VMM 权威分类对记忆分类。
    pub category: MemoryCategory,
    /// Carries a compact caller-facing summary.
    /// 携带面向调用方的紧凑摘要。
    pub summary: String,
    /// Carries the normalized memory statement.
    /// 携带规范化记忆陈述。
    pub content: String,
    /// Identifies the authoritative memory origin.
    /// 标识权威记忆来源。
    pub source_kind: MemorySourceKind,
    /// Identifies the originating host session when the source belongs to a session flow.
    /// 当来源属于会话流程时标识其宿主来源会话。
    pub origin_session_source_id: Option<SessionSourceId>,
    /// Carries host-normalized priority from zero through one hundred.
    /// 携带由宿主规范化的零到一百优先级。
    pub priority: u32,
    /// Classifies retention stability without embedding host hierarchy.
    /// 在不嵌入宿主层级的前提下分类保留稳定性。
    pub stability: MemoryStability,
    /// Carries searchable labels.
    /// 携带可搜索标签。
    pub tags: Vec<String>,
    /// Carries supporting evidence.
    /// 携带支撑证据。
    pub evidence: Vec<EvidenceRef>,
    /// Carries the caller-normalized evidence source text when the memory derived from one.
    /// 当记忆由候选派生时携带调用方规范化的证据来源文本。
    pub evidence_source: Option<String>,
    /// Carries the current lifecycle state.
    /// 携带当前生命周期状态。
    pub state: MemoryState,
    /// Carries normalized importance in the inclusive range zero to one.
    /// 携带零到一闭区间内的规范化重要度。
    pub importance: f32,
    /// Carries the last confidence score when the durable source recorded one.
    /// 在持久来源曾记录置信度时，携带零到一闭区间内的最新值。
    pub confidence: Option<f32>,
    /// Carries the creation time.
    /// 携带创建时间。
    pub created_at_ns: UnixNanos,
    /// Carries the last update time.
    /// 携带最后更新时间。
    pub updated_at_ns: UnixNanos,
    /// Carries an optional absolute expiration time.
    /// 携带可选的绝对过期时间。
    pub expires_at_ns: Option<UnixNanos>,
    /// Carries the last successful recall time when known.
    /// 携带已知的最后一次成功召回时间。
    pub last_recalled_at_ns: Option<UnixNanos>,
    /// Counts durable reinforcement events.
    /// 统计持久强化事件数。
    pub reinforcement_count: u64,
    /// Classifies the memory lifecycle level.
    /// 对记忆生命周期等级分类。
    pub memory_level: MemoryLevel,
    /// Carries the retention refresh weight from zero upward.
    /// 携带自零向上的保留刷新权重。
    pub refresh_weight: u64,
    /// Carries the deterministic hard-deduplication hash when computed.
    /// 在已计算时携带确定性硬去重哈希。
    pub dedupe_hash: Option<String>,
    /// Counts successful recall events.
    /// 统计成功召回事件数。
    pub recalled_count: u64,
    /// Counts durable adoption events.
    /// 统计持久采纳事件数。
    pub adopted_count: u64,
    /// Counts cross-session adoption events.
    /// 统计跨会话采纳事件数。
    pub cross_session_adopted_count: u64,
    /// States whether read-time decay is disabled for this memory.
    /// 表示该记忆是否禁用读时衰减。
    pub decay_disabled: bool,
    /// Carries the stable lifecycle status reason when present.
    /// 在存在时携带稳定的生命周期状态原因。
    pub status_reason: Option<String>,
    /// Carries the last adoption time when known.
    /// 在已知时携带最后采纳时间。
    pub last_adopted_at_ns: Option<UnixNanos>,
    /// Carries the last reinforcement time when known.
    /// 在已知时携带最后强化时间。
    pub last_reinforced_at_ns: Option<UnixNanos>,
    /// Identifies the originating turn for turn-derived memories.
    /// 为源自轮次的记忆标识来源轮次。
    pub source_turn_id: Option<TurnId>,
    /// Carries the optimistic concurrency version.
    /// 携带乐观并发版本。
    pub version: u64,
    /// Carries the vector space used by the optional embedding.
    /// 携带可选嵌入所使用的向量空间。
    pub vector_space_id: Option<VectorSpaceId>,
    /// Carries the optional embedding vector.
    /// 携带可选嵌入向量。
    pub embedding: Option<Vec<f32>>,
}

impl MemoryRecord {
    /// Reports whether the access class and optional source subject form one valid ownership identity.
    /// 报告访问类别与可选来源主体是否构成一个合法归属身份。
    #[must_use]
    pub const fn has_valid_access_identity(&self) -> bool {
        match self.access_class {
            MemoryAccessClass::SubjectPrivate => self.source_subject_id.is_some(),
            MemoryAccessClass::ScopeShared => true,
        }
    }

    /// Reports whether this memory is visible to one subject after the caller separately authorizes its scope.
    /// 在调用方已独立授权作用域后，报告该记忆是否对某主体可见。
    #[must_use]
    pub fn is_visible_to_subject(&self, subject_id: &SubjectId) -> bool {
        match self.access_class {
            MemoryAccessClass::SubjectPrivate => {
                self.source_subject_id.as_ref() == Some(subject_id)
            }
            MemoryAccessClass::ScopeShared => true,
        }
    }

    /// Reports whether two snapshots describe the same immutable access identity.
    /// 报告两个快照是否描述同一个不可变访问身份。
    #[must_use]
    pub fn has_same_access_identity(&self, other: &Self) -> bool {
        self.access_class == other.access_class
            && self.source_subject_id == other.source_subject_id
            && self.scope_id == other.scope_id
    }
}

/// Describes one authoritative memory-to-context-label evidence relation.
/// 描述一条权威的记忆到情境标签证据关系。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryContextEvidence {
    /// Identifies the owning memory.
    /// 标识所属记忆。
    pub memory_id: MemoryId,
    /// Carries the normalized context key.
    /// 携带规范化的情境键。
    pub context_key: String,
    /// Carries the normalized context value.
    /// 携带规范化的情境值。
    pub context_value: String,
    /// Counts durable support observations.
    /// 统计持久支持观测数。
    pub support_count: u32,
    /// Counts durable rebuttal observations.
    /// 统计持久反驳观测数。
    pub rebuttal_count: u32,
    /// Carries the last support time when known.
    /// 在已知时携带最后支持时间。
    pub last_supported_at_ns: Option<UnixNanos>,
    /// Carries the last rebuttal time when known.
    /// 在已知时携带最后反驳时间。
    pub last_rebutted_at_ns: Option<UnixNanos>,
    /// Carries the optimistic concurrency version.
    /// 携带乐观并发版本。
    pub version: u64,
}

/// Classifies one optional memory-to-memory graph relation kept separate from context evidence.
/// 对一个可选的、与情境证据分离的记忆到记忆图关系分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MemoryRelationKind {
    /// Indicates a supporting graph relation.
    /// 表示支持型图关系。
    Supports = 1,
    /// Indicates a related graph relation.
    /// 表示相关型图关系。
    Related = 2,
    /// Indicates a contradicting graph relation requiring host review.
    /// 表示需要宿主审核的矛盾型图关系。
    Contradicts = 3,
}

/// Describes one optional memory-to-memory graph relation with a dedicated contract.
/// 描述一条使用独立契约的可选记忆到记忆图关系。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryRelationEdge {
    /// Identifies the source memory.
    /// 标识源记忆。
    pub source_memory_id: MemoryId,
    /// Identifies the target memory.
    /// 标识目标记忆。
    pub target_memory_id: MemoryId,
    /// Classifies the graph relation.
    /// 对图关系分类。
    pub kind: MemoryRelationKind,
    /// Carries relation strength in the inclusive range zero to one.
    /// 携带零到一闭区间内的关系强度。
    pub strength: f32,
}

/// Describes the lifecycle state of one profile node.
/// 描述一个画像节点的生命周期状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ProfileNodeState {
    /// Indicates an active profile node.
    /// 表示活跃画像节点。
    Active = 1,
    /// Indicates an inactive profile node retained for audit.
    /// 表示为审计保留的非活跃画像节点。
    Inactive = 2,
    /// Retains extracted evidence that has not received an authoritative review decision.
    /// 保留尚未获得权威评审决定的提炼证据，不参与活跃画像渲染。
    Pending = 3,
    /// Indicates a node whose absolute lifecycle deadline has elapsed.
    /// 表示绝对生命周期截止时间已经到期的画像节点。
    Expired = 4,
}

/// Classifies the durable origin of one profile node without exposing host hierarchy.
/// 在不暴露宿主层级的前提下分类一个画像节点的持久来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ProfileSourceKind {
    /// Indicates extraction from one analyzed canonical turn.
    /// 表示从一个已分析规范轮次中提取。
    TurnExtract = 1,
    /// Indicates creation by one explicit profile instruction.
    /// 表示由一条显式画像指令创建。
    ManualInstruction = 2,
    /// Indicates insertion by a deterministic system seed or administrative import.
    /// 表示由确定性系统种子或管理导入写入。
    SystemSeed = 3,
    /// Indicates a shared node retained after its original subject was deleted.
    /// 表示原始主体删除后仍保留的共享画像节点。
    RetainedAfterSubjectDelete = 4,
}

/// Describes one stable profile node.
/// 描述一个稳定画像节点。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileNode {
    /// Identifies the profile node.
    /// 标识画像节点。
    pub id: ProfileNodeId,
    /// Identifies the originating subject when the durable source retained one.
    /// 在持久来源保留了来源主体时标识该主体。
    pub subject_id: Option<SubjectId>,
    /// Identifies the owning scope.
    /// 标识所属作用域。
    pub scope_id: ScopeId,
    /// Carries one atomic durable profile statement.
    /// 携带一条原子化持久画像陈述。
    pub content: String,
    /// Carries confidence when the durable source recorded one.
    /// 在持久来源曾记录置信度时，携带零到一闭区间内的值。
    pub confidence: Option<f32>,
    /// Carries the current lifecycle state.
    /// 携带当前生命周期状态。
    pub state: ProfileNodeState,
    /// Carries supporting turn identifiers.
    /// 携带支撑对话轮次标识。
    pub source_turn_ids: Vec<TurnId>,
    /// Classifies the authoritative durable source of this node.
    /// 分类该节点的权威持久来源。
    pub source_kind: ProfileSourceKind,
    /// Identifies the exact source event or record when the source kind carries one.
    /// 当来源类别携带精确来源时标识对应事件或记录。
    pub source_id: Option<ProfileSourceId>,
    /// Carries the normalized evidence source declared by the analyzed turn.
    /// 携带分析轮次声明的规范化证据来源。
    pub evidence_source: Option<String>,
    /// Carries the host-normalized priority from zero through one hundred.
    /// 携带由宿主规范化的零到一百优先级。
    pub priority: u32,
    /// Classifies the profile lifecycle level.
    /// 对画像生命周期等级分类。
    pub profile_level: ProfileLevel,
    /// Carries the stable level reason when present.
    /// 在存在时携带稳定的等级原因。
    pub level_reason: Option<String>,
    /// Carries the retention refresh weight from zero upward.
    /// 携带自零向上的保留刷新权重。
    pub refresh_weight: u32,
    /// Carries the effective profile date when present.
    /// 在存在时携带画像生效日期。
    pub effective_date_ns: Option<UnixNanos>,
    /// Carries an optional absolute expiration time.
    /// 携带可选的绝对过期时间。
    pub expires_at_ns: Option<UnixNanos>,
    /// Carries the stable lifecycle status reason when present.
    /// 在存在时携带稳定的生命周期状态原因。
    pub status_reason: Option<String>,
    /// Identifies the superseding profile node when retired by replacement.
    /// 当因替换退役时标识替代画像节点。
    pub superseded_by: Option<ProfileNodeId>,
    /// Carries the optimistic concurrency version.
    /// 携带乐观并发版本。
    pub version: u64,
    /// Carries the durable creation time used as the final timeline tie-break.
    /// 携带用作时间线最终并列判定依据的持久创建时间。
    pub created_at_ns: UnixNanos,
    /// Carries the last update time.
    /// 携带最后更新时间。
    pub updated_at_ns: UnixNanos,
}

/// Groups profile nodes for rendering and instruction application.
/// 对画像节点分组，以便渲染和应用指令。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileBundle {
    /// Identifies the owner subject.
    /// 标识所属主体。
    pub subject_id: SubjectId,
    /// Carries the ordered visible scopes used by this bundle.
    /// 携带该画像包使用的有序可见作用域。
    pub visible_scopes: Vec<ScopeId>,
    /// Carries active nodes in deterministic content and identity order.
    /// 以确定性的内容与身份顺序携带活跃节点。
    pub nodes: Vec<ProfileNode>,
    /// Carries a human-readable rendering for prompt injection.
    /// 携带用于 Prompt 注入的人类可读渲染。
    pub rendered: String,
    /// Carries ordered rendered sections when a host-resolved view was supplied.
    /// 在提供宿主解析视图时携带有序渲染分区。
    pub sections: Vec<ProfileSectionBundle>,
}

/// Renders one profile view section with its nodes.
/// 渲染一个含节点的画像视图分区。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileSectionBundle {
    /// Identifies the exact scope rendered by this section.
    /// 标识该分区渲染的精确作用域。
    pub scope_id: ScopeId,
    /// Classifies the section role.
    /// 对分区角色分类。
    pub role: SectionRole,
    /// Carries the host display label.
    /// 携带宿主显示标签。
    pub display_label: String,
    /// Carries nodes belonging to this section in stable order.
    /// 以稳定顺序携带属于该分区的节点。
    pub nodes: Vec<ProfileNode>,
    /// Carries the section rendering.
    /// 携带该分区渲染。
    pub rendered: String,
}

/// Classifies one host-resolved profile view section role.
/// 对一个由宿主解析的画像视图分区角色分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum SectionRole {
    /// Indicates an environment section shared across subjects.
    /// 表示跨主体共享的环境分区。
    Environment = 1,
    /// Indicates the subject-owned section.
    /// 表示主体自有分区。
    Subject = 2,
}

/// Selects the profile bundle rendering mode.
/// 选择画像包渲染模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ProfileViewMode {
    /// Renders one combined bundle across all sections.
    /// 渲染跨全部分区的单一组合包。
    Full = 1,
    /// Renders one independent bundle per section.
    /// 按分区渲染各自独立的包。
    Split = 2,
}

/// Carries one host-resolved ordered profile section.
/// 携带一个由宿主解析的有序画像分区。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileViewSection {
    /// Identifies the exact scope rendered by this section.
    /// 标识该分区渲染的精确作用域。
    pub scope_id: ScopeId,
    /// Classifies the section role.
    /// 对分区角色分类。
    pub role: SectionRole,
    /// Carries the host display label.
    /// 携带宿主显示标签。
    pub display_label: String,
}

/// Carries the host-resolved profile view request.
/// 携带由宿主解析的画像视图请求。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileView {
    /// Selects the rendering mode.
    /// 选择渲染模式。
    pub mode: ProfileViewMode,
    /// Carries ordered host-resolved sections.
    /// 携带由宿主解析的有序分区。
    pub sections: Vec<ProfileViewSection>,
    /// States whether lifecycle explanations are included in renderings.
    /// 表示渲染是否包含生命周期说明。
    pub include_explanations: bool,
    /// Names the explicit IANA display timezone, including its historical daylight-saving rules.
    /// 指定显式 IANA 显示时区，包含该时区的历史夏令时规则。
    pub display_timezone: String,
}

/// Defines the explicit scope boundary for one command.
/// 定义一个命令的显式作用域边界。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeAccess {
    /// Identifies the primary write scope; commands with explicit multi-scope authorization may extend it.
    /// 标识主要写入作用域；具备显式多作用域授权的命令可以扩展该边界。
    pub write_scope: ScopeId,
    /// Carries ordered scopes visible to reads.
    /// 携带读取可见的有序作用域。
    pub visible_scopes: Vec<ScopeId>,
}

impl ScopeAccess {
    /// Validates scope isolation and deterministic precedence.
    /// 校验作用域隔离和确定性优先级。
    ///
    /// # Errors
    /// Returns an error when visible scopes are empty, oversized, duplicated, or omit the write scope.
    /// 当可见作用域为空、超限、重复或遗漏写入作用域时返回错误。
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.visible_scopes.is_empty() {
            return Err("visible scopes must not be empty");
        }
        if self.visible_scopes.len() > MAX_VISIBLE_SCOPES {
            return Err("visible scopes exceed the supported limit");
        }
        if !self.visible_scopes.contains(&self.write_scope) {
            return Err("visible scopes must include the write scope");
        }
        // Use a sorted set only for validation so the caller-provided precedence remains unchanged.
        // 仅使用有序集合进行校验，确保调用方提供的优先级保持不变。
        let unique = self
            .visible_scopes
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        if unique.len() != self.visible_scopes.len() {
            return Err("visible scopes must not contain duplicates");
        }
        Ok(())
    }
}

/// Carries one scored memory search hit.
/// 携带一条带评分的记忆搜索命中。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Carries the matched memory.
    /// 携带匹配的记忆。
    pub memory: MemoryRecord,
    /// Carries the host search score.
    /// 携带宿主搜索评分。
    pub search_score: f64,
    /// Carries the optional rerank score.
    /// 携带可选的重排评分。
    pub rerank_score: Option<f64>,
    /// Carries the final Core score.
    /// 携带最终 Core 评分。
    pub final_score: f64,
    /// Carries the complete staged score explanation when produced by the retrieval kernel.
    /// 当由检索内核产生时携带完整的分阶段评分解释。
    pub explanation: Option<crate::ScoreExplanation>,
}

/// Carries all domain data included by one portable export.
/// 携带一次可移植导出包含的全部领域数据。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportDataset {
    /// Carries canonical turns.
    /// 携带规范化对话轮次。
    pub turns: Vec<Turn>,
    /// Carries durable memories.
    /// 携带持久记忆。
    pub memories: Vec<MemoryRecord>,
    /// Carries profile nodes.
    /// 携带画像节点。
    pub profile_nodes: Vec<ProfileNode>,
    /// Carries memory context evidence.
    /// 携带记忆情境证据。
    pub context_evidence: Vec<MemoryContextEvidence>,
    /// Carries durable session compact boundaries.
    /// 携带持久会话压缩边界。
    pub chat_compact_boundaries: Vec<ChatCompactBoundary>,
}

impl ExportDataset {
    /// Creates an empty deterministic dataset.
    /// 创建一个空的确定性数据集。
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            turns: Vec::new(),
            memories: Vec::new(),
            profile_nodes: Vec::new(),
            context_evidence: Vec::new(),
            chat_compact_boundaries: Vec::new(),
        }
    }
}
