//! Typed host effect and resume result contracts.
//! 强类型宿主 Effect 与恢复结果契约。

use crate::{
    AnalysisAttemptIdentity, CommandId, CommandResult, CommitReceipt, ConfigSnapshotId, EffectId,
    EntityRef, MemoryAccessClass, MemoryCategory, MemoryContextEvidence, MemoryId, MemoryRecord,
    MutationIdentity, PhysicalCandidate, PreCheckReviewCandidate, ProfileNode, ProfileNodeId,
    ProfileViewSection, Role, ScopeAccess, ScopeId, ScoreContractRef, SearchHit, SessionRecallMode,
    SessionSourceId, StructuredOutputContractId, SubjectId, TimelineItem, Turn, TurnId,
    TurnInputKind, TurnStatus, UnixNanos, VectorSpaceId,
};

/// Carries storage-owned compact-boundary metadata before Core attaches the commit receipt.
/// 携带 Core 附加提交收据前由存储拥有的压缩边界元数据。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCompactMutationResult {
    /// States whether the resolved session acknowledgement was accepted.
    /// 表示已解析会话确认是否被接受。
    pub accepted: bool,
    /// States whether durable boundary state changed.
    /// 表示持久边界状态是否发生变化。
    pub updated: bool,
    /// Identifies the active compact boundary when at least one turn exists.
    /// 当至少存在一个轮次时标识当前压缩边界。
    pub compacted_turn_id: Option<TurnId>,
}
use serde::{Deserialize, Serialize};

/// Classifies structured LLM work without exposing provider details.
/// 在不暴露 Provider 细节的前提下分类结构化 LLM 工作。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum LlmPurpose {
    /// Extracts recall intent and search queries.
    /// 提取召回意图与搜索查询。
    PreCheckIntent = 1,
    /// Reviews recalled memory candidates.
    /// 审核召回的记忆候选。
    PreCheckReview = 2,
    /// Analyzes a completed turn for memory and profile mutations.
    /// 分析完整轮次以产生记忆和画像变更。
    PostActionAnalysis = 3,
    /// Reviews extracted memory candidates against similar stored memories.
    /// 针对相似已存记忆审核提取的记忆候选。
    PostActionReview = 6,
    /// Reviews a natural-language profile instruction.
    /// 审核自然语言画像指令。
    ProfileInstruction = 4,
}

/// Carries one already-refined historical turn used only as analysis context.
/// 携带一个仅作为分析上下文使用的已提炼历史轮次。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostActionReferenceTurn {
    /// Identifies the historical turn.
    /// 标识历史轮次。
    pub turn_id: TurnId,
    /// Identifies the exact subject owner of the historical turn.
    /// 标识历史轮次的精确主体所有者。
    pub subject_id: SubjectId,
    /// Identifies the exact owning scope of the historical turn.
    /// 标识历史轮次的精确所属作用域。
    pub scope_id: ScopeId,
    /// Identifies the exact originating host session.
    /// 标识历史轮次的精确来源宿主会话。
    pub session_source_id: SessionSourceId,
    /// Carries persisted refined details instead of the historical raw transcript.
    /// 携带已持久化的提炼详情，而不是历史原始对话。
    pub details: String,
    /// Carries the canonical creation time used to validate reference ordering.
    /// 携带用于校验参考顺序的规范化创建时间。
    pub created_at_ns: i64,
}

/// Carries one recent direct write that excludes duplicate turn extraction.
/// 携带一条用于排除重复轮次提炼的近期主动写入。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostActionDirectWrite {
    /// Identifies the durable direct-write memory.
    /// 标识持久主动写入记忆。
    pub memory_id: MemoryId,
    /// Identifies its exact owning scope.
    /// 标识其精确所属作用域。
    pub scope_id: ScopeId,
    /// Classifies the host-agnostic lifecycle horizon independently from the physical scope identity.
    /// 独立于物理作用域身份，对与宿主层级无关的生命周期跨度进行分类。
    pub scope_class: crate::MemoryScopeClass,
    /// Identifies the exact originating host session.
    /// 标识精确来源宿主会话。
    pub session_source_id: SessionSourceId,
    /// Carries the compact memory summary.
    /// 携带紧凑记忆摘要。
    pub summary: String,
    /// Carries the complete durable memory content.
    /// 携带完整持久记忆内容。
    pub content: String,
    /// Carries the canonical creation time.
    /// 携带规范化创建时间。
    pub created_at_ns: i64,
}

/// Carries one provider-agnostic structured LLM request.
/// 携带一个与 Provider 无关的结构化 LLM 请求。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LlmEffect {
    /// Classifies the requested work.
    /// 分类请求的工作。
    pub purpose: LlmPurpose,
    /// Identifies the closed authoritative output contract.
    /// 标识封闭的权威输出契约。
    pub output_contract: StructuredOutputContractId,
    /// Carries the frozen output contract version.
    /// 携带冻结的输出契约版本。
    pub output_contract_version: u32,
    /// Carries the authoritative digest of the exact output contract AST.
    /// 携带精确输出契约 AST 的权威摘要。
    pub output_contract_digest: String,
    /// Carries the authoritative system prompt.
    /// 携带权威系统 Prompt。
    pub system_prompt: String,
    /// Carries typed request content rendered without a JSON runtime contract.
    /// 携带不依赖 JSON 运行时契约的类型化请求内容渲染。
    pub input: LlmInput,
}

/// Binds L2 review to its exact source without substituting analysis cutoff for host dispatch time.
/// 将L2审核绑定到精确来源，不以分析截止时间替代宿主分派时刻。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PostActionReviewContext {
    /// Retains the analyzed turn and explicit role bindings; the host separately records review dispatch time.
    /// 保留被分析轮次与显式角色绑定；宿主另行记录审核分派时刻。
    Turn {
        /// Carries the complete validated pending turn, including its authoritative creation time.
        /// 携带已校验待处理轮次的完整值及权威创建时间。
        target_turn: Box<Turn>,
        /// Carries the current L1 classification, not a previous persisted classification.
        /// 携带当前L1分类，而不是先前持久化的分类。
        user_input_kind: TurnInputKind,
        /// Binds each profile role to its exact command-owned section.
        /// 将每个画像角色绑定到命令中精确分区。
        profile_sections: Vec<ProfileViewSection>,
    },
    /// Retains explicit-write time; this source has no turn transcript or profile review.
    /// 保留显式写入时间；此来源没有轮次正文或画像审核。
    DirectWrite {
        /// Carries the original command time used for candidate date and current review time.
        /// 携带同时用于候选日期和当前审核时间的原命令时间。
        occurred_at_ns: UnixNanos,
    },
}

/// Enumerates typed LLM inputs.
/// 枚举强类型 LLM 输入。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LlmInput {
    /// Carries `PreCheck` intent input.
    /// 携带 `PreCheck` 意图输入。
    PreCheckIntent {
        /// Carries recent timeline context.
        /// 携带最近时间线上下文。
        recent_timeline: Vec<TimelineItem>,
        /// Carries current user content.
        /// 携带当前用户内容。
        user_content: String,
        /// Limits returned queries.
        /// 限制返回查询数量。
        max_search_queries: u32,
    },
    /// Carries `PreCheck` candidate review input.
    /// 携带 `PreCheck` 候选审核输入。
    PreCheckReview {
        /// Carries the command's frozen timestamp for temporal relevance decisions.
        /// 携带命令冻结时间戳，以便审核时判断时间相关性。
        current_timestamp_ns: i64,
        /// Carries the actual normalized queries used by retrieval in stable order.
        /// 按稳定顺序携带检索实际使用的归一化查询。
        search_queries: Vec<String>,
        /// Carries the normalized L1 rationale; forced recall explicitly has no L1 rationale.
        /// 携带归一化 L1 理由；强制召回未运行 L1，因此明确为空。
        intent_reason: Option<String>,
        /// Carries the current user content.
        /// 携带当前用户内容。
        user_content: String,
        /// Carries candidate hits.
        /// 携带候选命中。
        candidates: Vec<PreCheckReviewCandidate>,
    },
    /// Carries `PostAction` analysis input.
    /// 携带 `PostAction` 分析输入。
    PostActionAnalysis {
        /// Carries the single analysis-cutoff timestamp frozen for this attempt.
        /// 携带为本次分析尝试冻结的唯一分析截止时间戳。
        current_timestamp_ns: i64,
        /// Carries already-refined historical turns used only as reference context.
        /// 携带仅作为参考上下文使用的已提炼历史轮次。
        reference_turns: Vec<PostActionReferenceTurn>,
        /// Carries the only raw pending turn allowed to produce new facts.
        /// 携带唯一允许产出新事实的原始待处理轮次。
        target_turn: Box<Turn>,
        /// Carries recent direct writes that must not be extracted again.
        /// 携带不得被重复提取的近期主动写入记忆。
        recent_direct_writes: Vec<PostActionDirectWrite>,
    },
    /// Carries `PostAction` candidate review input.
    /// 携带 `PostAction` 候选审核输入。
    PostActionReview {
        /// Carries the exact source, temporal anchor and profile-role bindings of this review.
        /// 携带本次审核的精确来源、时间锚点及画像角色绑定。
        context: PostActionReviewContext,
        /// Carries each candidate together with only its own similar durable memories.
        /// 携带每条候选及仅属于该候选的相似持久记忆。
        memory_candidates: Vec<MemoryReviewCandidate>,
        /// Carries newly extracted profile candidates in stable candidate order.
        /// 以稳定候选顺序携带新提取的画像候选。
        profile_candidates: Vec<ProfileCandidate>,
        /// Carries the exact visible active profile nodes eligible for retirement.
        /// 携带可被退役的精确可见活跃画像节点。
        current_profile_nodes: Vec<ProfileNode>,
    },
    /// Carries profile instruction input.
    /// 携带画像指令输入。
    ProfileInstruction {
        /// Carries the exact host-resolved section reviewed by this instruction.
        /// 携带本次指令审核的精确宿主已解析分区。
        target_section: ProfileViewSection,
        /// Carries the instruction text.
        /// 携带指令文本。
        instruction: String,
        /// Carries currently visible profile nodes.
        /// 携带当前可见画像节点。
        current_nodes: Vec<ProfileNode>,
    },
}

/// Binds one new memory candidate to its candidate-local semantic review evidence.
/// 将一条新记忆候选绑定到其候选局部的语义审核证据。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryReviewCandidate {
    /// Carries the new candidate under review.
    /// 携带正在审核的新候选。
    pub candidate: MemoryCandidate,
    /// Carries only the durable memories recalled for this candidate.
    /// 仅携带为该候选召回的持久记忆。
    pub similar_memories: Vec<SearchHit>,
}

/// Classifies the closed admission decision of one extracted candidate.
/// 对一个提取候选的封闭准入决策分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum Admission {
    /// Indicates the candidate passed admission review.
    /// 表示候选通过准入审核。
    Accept = 1,
    /// Indicates the candidate was explicitly discarded.
    /// 表示候选被显式丢弃。
    Drop = 2,
}

/// Classifies the support or rebuttal direction of one context evidence relation.
/// 对一条情境证据关系的支持或反驳方向分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ContextEvidenceRelation {
    /// Indicates a supporting observation.
    /// 表示支持型观测。
    Support = 1,
    /// Indicates a rebutting observation.
    /// 表示反驳型观测。
    Rebuttal = 2,
}

/// Describes one memory candidate extracted from a turn.
/// 描述从对话轮次提取的一条记忆候选。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCandidate {
    /// Classifies the candidate using the authoritative VMM category.
    /// 使用 VMM 权威分类对候选分类。
    pub category: MemoryCategory,
    /// Carries the compact candidate summary.
    /// 携带候选紧凑摘要。
    pub summary: String,
    /// Carries the normalized statement.
    /// 携带规范化陈述。
    pub content: String,
    /// Carries the caller-normalized evidence source text.
    /// 携带由调用方规范化的证据来源文本。
    pub evidence_source: String,
    /// Carries the closed admission decision.
    /// 携带封闭准入决策。
    pub admission: Admission,
    /// Carries the explicit discard reason when dropped.
    /// 在被丢弃时携带显式丢弃原因。
    pub admission_reason: Option<String>,
}

/// Describes one profile candidate extracted from a conversation turn.
/// 描述从对话轮次提取的一个画像候选。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileCandidate {
    /// Selects the host-resolved generic profile section that owns this candidate.
    /// 选择拥有该候选的宿主已解析通用画像分区。
    pub section_role: crate::SectionRole,
    /// Carries the proposed atomic profile content.
    /// 携带建议的原子画像内容。
    pub content: String,
    /// Carries the caller-normalized evidence source text.
    /// 携带由调用方规范化的证据来源文本。
    pub evidence_source: String,
    /// Carries the closed admission decision.
    /// 携带封闭准入决策。
    pub admission: Admission,
    /// Carries the explicit discard reason when dropped.
    /// 在被丢弃时携带显式丢弃原因。
    pub admission_reason: Option<String>,
}

/// Names one accepted `PostAction` profile candidate with its reviewed lifecycle projection.
/// 命名一个已接受的 `PostAction` 画像候选及其经审核的生命周期投影。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedProfileCandidate {
    /// Identifies the zero-based profile candidate index.
    /// 标识从零开始的画像候选索引。
    pub candidate_index: u32,
    /// Carries the normalized durable profile value.
    /// 携带规范化持久画像值。
    pub normalized_content: String,
    /// Carries reviewed P2/P1/P0 priority as exactly 20/60/100.
    /// 以精确的 20/60/100 携带经审核的 P2/P1/P0 优先级。
    pub priority: u32,
    /// Classifies the reviewed profile level.
    /// 对经审核的画像等级分类。
    pub profile_level: crate::ProfileLevel,
    /// Carries the stable level reason when present.
    /// 在存在时携带稳定等级原因。
    pub level_reason: Option<String>,
    /// Carries exact active nodes superseded by this candidate.
    /// 携带被该候选替代的精确活跃节点。
    pub supersede_nodes: Vec<SupersedeNodeRef>,
}

/// Describes one context evidence observation attached to one extracted candidate.
/// 描述附着于一个提取候选的一条情境证据观测。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEvidenceCandidate {
    /// Identifies the zero-based owning candidate index.
    /// 标识从零开始的归属候选索引。
    pub candidate_index: u32,
    /// Carries the normalized context key.
    /// 携带规范化的情境键。
    pub context_key: String,
    /// Carries the normalized context value.
    /// 携带规范化的情境值。
    pub context_value: String,
    /// Classifies the support or rebuttal direction.
    /// 对支持或反驳方向分类。
    pub relation: ContextEvidenceRelation,
}

/// Names one accepted `PostAction` candidate with its explicit supersede set.
/// 命名一个已接受的 `PostAction` 候选及其显式替代集合。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedCandidate {
    /// Identifies the zero-based accepted candidate index.
    /// 标识从零开始的已接受候选索引。
    pub candidate_index: u32,
    /// Carries exact existing memory identities this candidate supersedes.
    /// 携带该候选替代的精确既有记忆标识。
    pub supersede_memory_ids: Vec<String>,
}

/// Names one discarded `PostAction` candidate and its deduplication source.
/// 命名一个被丢弃的 `PostAction` 候选及其去重来源。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedCandidate {
    /// Identifies the zero-based discarded candidate index.
    /// 标识从零开始的被丢弃候选索引。
    pub candidate_index: u32,
    /// Identifies the exact duplicate source when supplied; non-durable drops may have no existing target.
    /// 提供时标识精确重复来源；没有长期价值的丢弃可以不存在既有目标。
    pub dedupe_memory_id: Option<String>,
    /// Carries the optional audit reason for the drop.
    /// 携带可选的丢弃审计原因。
    pub reason: Option<String>,
}

/// Names one accepted profile instruction node with its lifecycle projection.
/// 命名一个已接受的画像指令节点及其生命周期投影。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedProfileNode {
    /// Carries the normalized profile content.
    /// 携带规范化画像内容。
    pub content: String,
    /// Carries requested P2/P1/P0 priority as exactly 20/60/100.
    /// 以精确的 20/60/100 携带请求的 P2/P1/P0 优先级。
    pub priority: u32,
    /// Classifies the requested profile level.
    /// 对请求的画像等级分类。
    pub profile_level: crate::ProfileLevel,
    /// Carries the stable level reason when present.
    /// 在存在时携带稳定的等级原因。
    pub level_reason: Option<String>,
    /// Carries nodes retired and replaced by this accepted node.
    /// 携带被该接受节点退役并替代的节点。
    pub supersede_nodes: Vec<SupersedeNodeRef>,
}

/// References one existing profile node retired with an explicit reason.
/// 引用一个以显式原因退役的既有画像节点。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetiredProfileNode {
    /// Identifies the visible active profile node.
    /// 标识可见的活跃画像节点。
    pub node_id: ProfileNodeId,
    /// Carries the explicit retirement reason.
    /// 携带显式退役原因。
    pub reason: String,
}

/// References one existing profile node superseded by an accepted node.
/// 引用一个被已接受节点替代的既有画像节点。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupersedeNodeRef {
    /// Identifies the visible active profile node replaced by the accepted node.
    /// 标识被接受节点替代的可见活跃画像节点。
    pub node_id: ProfileNodeId,
    /// Carries the explicit supersede reason.
    /// 携带显式替代原因。
    pub reason: String,
}

/// Enumerates typed structured LLM outputs.
/// 枚举强类型结构化 LLM 输出。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmOutput {
    /// Carries recall intent and queries.
    /// 携带召回意图与查询。
    PreCheckIntent {
        /// Carries the required concise intent rationale.
        /// 携带必需的简短意图理由。
        reason: String,
        /// States whether memory lookup is required.
        /// 表示是否需要记忆检索。
        need_memory: bool,
        /// Carries complete natural-language search queries.
        /// 携带完整自然语言搜索查询。
        queries: Vec<String>,
    },
    /// Carries accepted candidate identifiers in final order.
    /// 携带最终顺序中的已接受候选标识。
    PreCheckReview {
        /// Carries accepted memory identities.
        /// 携带已接受的记忆标识。
        accepted_memory_ids: Vec<MemoryId>,
    },
    /// Carries mutations extracted from a completed turn.
    /// 携带从完整轮次提取的变更。
    PostActionAnalysis {
        /// Classifies the dominant user act in the target turn.
        /// 对目标轮次中的主要用户行为分类。
        user_input_kind: TurnInputKind,
        /// Echoes the exact target turn identity for integrity validation.
        /// 回显精确目标轮次标识以供完整性校验。
        turn_id: crate::TurnId,
        /// Carries the compact extracted turn details.
        /// 携带紧凑的轮次提炼详情。
        details: String,
        /// Carries memory candidates.
        /// 携带记忆候选。
        memories: Vec<MemoryCandidate>,
        /// Carries profile candidates.
        /// 携带画像候选。
        profiles: Vec<ProfileCandidate>,
        /// Carries context evidence observations attached to memory candidates.
        /// 携带附着于记忆候选的情境证据观测。
        context_evidence: Vec<ContextEvidenceCandidate>,
    },
    /// Carries the closed `PostAction` review outcome.
    /// 携带封闭的 `PostAction` 审核结果。
    PostActionReview {
        /// Carries accepted candidates with supersede relationships.
        /// 携带带替代关系的已接受候选。
        accepted_candidates: Vec<AcceptedCandidate>,
        /// Carries dropped candidates with deduplication sources.
        /// 携带带去重来源的被丢弃候选。
        dropped_candidates: Vec<DroppedCandidate>,
        /// Carries accepted profile candidates with exact supersede identities.
        /// 携带带精确替代身份的已接受画像候选。
        accepted_profile_candidates: Vec<AcceptedProfileCandidate>,
        /// Carries rejected profile candidate indexes in stable order.
        /// 以稳定顺序携带被拒绝画像候选索引。
        invalid_profile_candidate_indexes: Vec<u32>,
        /// Carries active nodes retired without a replacement candidate.
        /// 携带在没有替代候选时退役的活跃节点。
        retired_profile_nodes: Vec<RetiredProfileNode>,
        /// Carries the review rationale for audit.
        /// 携带供审计使用的审核理由。
        reason: Option<String>,
    },
    /// Carries profile instruction review output.
    /// 携带画像指令审核输出。
    ProfileInstruction {
        /// Carries accepted profile nodes with lifecycle projection.
        /// 携带带生命周期投影的已接受画像节点。
        accepted_nodes: Vec<AcceptedProfileNode>,
        /// Carries explicit retired node identities with reasons.
        /// 携带带原因的显式退役节点标识。
        retired_nodes: Vec<RetiredProfileNode>,
        /// Carries the review rationale for audit and adapters.
        /// 携带供审计与适配器使用的审核理由。
        review_reason: String,
    },
}

/// Carries one batch embedding request.
/// 携带一次批量嵌入请求。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingEffect {
    /// Identifies the vector space.
    /// 标识向量空间。
    pub vector_space_id: VectorSpaceId,
    /// Carries the exact vector dimension the Host must return for every input.
    /// 携带 Host 必须为每个输入返回的精确向量维度。
    pub expected_dimension: u32,
    /// Carries all input texts in stable order.
    /// 以稳定顺序携带全部输入文本。
    pub inputs: Vec<String>,
    /// Allows only individually confirmed invalid inputs to be dropped during `PostAction` persistence.
    /// 仅允许在 `PostAction` 持久化时丢弃逐条确认非法的输入。
    pub allow_partial_invalid_inputs: bool,
}

/// Identifies the supported provider-confirmed permanent input rejection.
/// 标识受支持的 Provider 已确认永久输入拒绝。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingDropReason {
    /// Indicates that this particular input exceeds the provider input size limit.
    /// 表示此条输入超过 Provider 输入长度限制。
    InputTooLarge,
    /// Indicates an explicit item-scoped validation rejection without claiming a size-related cause.
    /// 表示明确的单项输入校验拒绝，不声称其原因与长度有关。
    ProviderConfirmedInvalidInput,
}

/// Audits one rejected input without treating provider outages as successful partial work.
/// 审计一条拒绝输入，不将 Provider 故障视为成功的部分工作。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingDroppedInput {
    /// Identifies the exact zero-based input position in the request.
    /// 标识请求中精确的零基输入位置。
    pub input_index: u32,
    /// Carries the typed permanent input rejection reason.
    /// 携带强类型永久输入拒绝原因。
    pub reason: EmbeddingDropReason,
}

/// Carries a complete embedding outcome with explicit indexed partial successes.
/// 携带完整嵌入结果，部分成功具有显式索引。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingBatchResult {
    /// Carries successful vectors in response order.
    /// 按响应顺序携带成功向量。
    pub vectors: Vec<Vec<f32>>,
    /// Maps each vector to its request input; empty is allowed only for a complete ordered result.
    /// 将每个向量映射到请求输入；仅完整有序结果允许为空。
    pub result_indices: Vec<u32>,
    /// Carries every permanently invalid input; successes and drops must partition the request.
    /// 携带每个永久非法输入；成功与丢弃必须构成请求的完整划分。
    pub dropped: Vec<EmbeddingDroppedInput>,
}

impl From<Vec<Vec<f32>>> for EmbeddingBatchResult {
    /// Wraps a complete input-ordered batch without allowing implicit partial success.
    /// 包装完整的输入有序批次，不允许隐式部分成功。
    fn from(vectors: Vec<Vec<f32>>) -> Self {
        Self {
            vectors,
            result_indices: Vec::new(),
            dropped: Vec::new(),
        }
    }
}

/// Carries one batch rerank request.
/// 携带一次批量重排请求。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerankEffect {
    /// Carries the natural-language query.
    /// 携带自然语言查询。
    pub query: String,
    /// Carries candidate documents in stable order.
    /// 以稳定顺序携带候选文档。
    pub documents: Vec<String>,
}

/// Preserves one provider-ranked occurrence and its exact index in the current rerank document list.
/// 保留一个提供方排名条目及其在当前重排文档列表中的精确索引，不使用数据库身份或原候选索引。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RerankEntry {
    /// Selects a document from this effect; the containing sequence supplies provider rank and may repeat an index.
    /// 选择本次效果中的文档；外层序列决定提供方名次，同一索引可按既有来源语义重复。
    pub input_index: u32,
    /// Preserves the finite binary64 provider score before Core applies its documented public bounds.
    /// 保留 Core 应用既定公开范围之前的有限双精度提供方评分。
    pub score: f64,
}

/// Binds the optional exact-session recall boundary of one candidate query.
/// 绑定一次候选查询的可选精确会话召回边界。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateQueryBoundary {
    /// Applies an independent scope and origin-session filter before physical candidate truncation.
    /// 在物理候选截断前应用独立作用域及来源会话过滤。
    pub candidate_scope: Option<crate::MemoryCandidateScope>,
    /// Identifies the exact current-session recall boundary when present.
    /// 存在时标识精确的当前会话召回边界。
    pub session_recall_boundary: Option<SessionRecallBoundary>,
}

/// Carries the exact current-session recall boundary requested from semantic storage.
/// 携带向语义存储请求的精确当前会话召回边界。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecallBoundary {
    /// Identifies the resolved host session whose turn evidence may be restricted.
    /// 标识其轮次证据可能受限的已解析宿主会话。
    pub session_source_id: SessionSourceId,
    /// Selects compact-aware or full current-session visibility.
    /// 选择压缩感知或完整的当前会话可见性。
    pub mode: SessionRecallMode,
}

/// Defines one semantic storage query.
/// 定义一次语义存储查询。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StorageQuery {
    /// Requests one complete physical observation without a cross-effect host cache.
    /// 请求一次完整物理观察，不依赖跨效果宿主缓存。
    CandidateObservation(crate::CandidateObservationQuery),
    /// Requests physical vector candidates with score-contract facts.
    /// 请求带评分契约事实的物理向量候选。
    VectorCandidates {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines scope boundaries.
        /// 定义作用域边界。
        scope_access: ScopeAccess,
        /// Carries one or more complete queries.
        /// 携带一条或多条完整查询。
        queries: Vec<String>,
        /// Carries one embedding per query.
        /// 每条查询携带一个嵌入。
        query_embeddings: Vec<Vec<f32>>,
        /// Identifies the required vector space.
        /// 标识所需向量空间。
        vector_space_id: VectorSpaceId,
        /// Carries the optional exact-session recall boundary.
        /// 携带可选的精确会话召回边界。
        boundary: CandidateQueryBoundary,
        /// Limits returned physical candidates.
        /// 限制返回的物理候选数。
        limit: u32,
        /// Declares the required vector score contract.
        /// 声明所需的向量评分契约。
        expected_contract: ScoreContractRef,
        /// Carries the frozen business observation time used for lifecycle filtering.
        /// 携带用于生命周期过滤的冻结业务观察时刻。
        now_ns: UnixNanos,
    },
    /// Requests physical lexical candidates with score-contract facts.
    /// 请求带评分契约事实的物理词法候选。
    LexicalCandidates {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines scope boundaries.
        /// 定义作用域边界。
        scope_access: ScopeAccess,
        /// Carries one or more complete queries.
        /// 携带一条或多条完整查询。
        queries: Vec<String>,
        /// Identifies the required vector space for candidate eligibility.
        /// 标识候选资格所需的向量空间。
        vector_space_id: VectorSpaceId,
        /// Carries the optional exact-session recall boundary.
        /// 携带可选的精确会话召回边界。
        boundary: CandidateQueryBoundary,
        /// Limits returned physical candidates.
        /// 限制返回的物理候选数。
        limit: u32,
        /// Declares the required lexical score contract.
        /// 声明所需的词法评分契约。
        expected_contract: ScoreContractRef,
        /// Carries the same frozen business observation time as the vector channel.
        /// 携带与向量通道相同的冻结业务观察时刻。
        now_ns: UnixNanos,
    },
    /// Reads complete memory records for exact identities.
    /// 按精确标识读取完整记忆记录。
    MemoryRecordBatch {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines exact visible scope boundaries.
        /// 定义精确可见作用域边界。
        scope_access: ScopeAccess,
        /// Carries requested memory identities in caller order.
        /// 按调用方顺序携带请求的记忆标识。
        memory_ids: Vec<MemoryId>,
    },
    /// Finds recent active direct writes by Core-owned semantic deduplication hashes.
    /// 按 Core 自有语义去重哈希查找近期活跃主动写入。
    RecentDirectWriteDedupe {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines exact visible scope boundaries.
        /// 定义精确可见作用域边界。
        scope_access: ScopeAccess,
        /// Identifies the exact host session that originated the writes.
        /// 标识产生这些写入的精确宿主会话。
        session_source_id: SessionSourceId,
        /// Carries caller-ordered lookup keys and ownership boundaries.
        /// 按调用方顺序携带查询键与归属边界。
        lookups: Vec<DirectWriteDedupeLookup>,
        /// Carries the inclusive creation-time lower boundary.
        /// 携带创建时间闭区间下界。
        created_not_before_ns: UnixNanos,
        /// Carries the deterministic evaluation time and inclusive upper boundary.
        /// 携带确定性评估时间与闭区间上界。
        evaluated_at_ns: UnixNanos,
    },
    /// Reads context evidence grouped by exact memory identities.
    /// 按精确记忆标识分组读取情境证据。
    ContextEvidenceBatch {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines exact visible scope boundaries.
        /// 定义精确可见作用域边界。
        scope_access: ScopeAccess,
        /// Carries requested memory identities in caller order.
        /// 按调用方顺序携带请求的记忆标识。
        memory_ids: Vec<MemoryId>,
    },
    /// Reads the persisted semantic-noise prototype bundle for one cache identity.
    /// 按一个缓存身份读取已持久化的语义 Noise 原型束。
    NoisePrototypeBundle {
        /// Identifies the complete prototype cache identity.
        /// 标识完整的原型缓存身份。
        identity: NoisePrototypeCacheIdentity,
    },
    /// Reads raw retention scan facts bounded by the requested batch.
    /// 读取按请求批次受限的原始保留扫描事实。
    RetentionScan {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines exact write scope boundaries.
        /// 定义精确写入作用域边界。
        scope_access: ScopeAccess,
        /// Carries the policy evaluation time.
        /// 携带策略评估时间。
        now_ns: i64,
        /// Carries the exact Core-owned policy that bounds each physical fact family.
        /// 携带对各物理事实族进行约束的精确 Core 自有策略。
        policy: crate::RetentionPolicy,
    },
    /// Reads one memory by identity and explicit visible scopes.
    /// 按标识和显式可见作用域读取一条记忆。
    MemoryDetails {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines scope boundaries.
        /// 定义作用域边界。
        scope_access: ScopeAccess,
        /// Identifies the requested memory.
        /// 标识请求的记忆。
        memory_id: MemoryId,
    },
    /// Reads caller-ordered memory and turn details in one host call.
    /// 在一次宿主调用中按调用方顺序读取记忆与轮次详情。
    DetailsBatch {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines exact visible scope boundaries.
        /// 定义精确可见作用域边界。
        scope_access: ScopeAccess,
        /// Carries requested memory identities.
        /// 携带请求的记忆标识。
        memory_ids: Vec<MemoryId>,
        /// Carries requested turn identities.
        /// 携带请求的轮次标识。
        turn_ids: Vec<TurnId>,
    },
    /// Reads the exact reference and direct-write facts for one pending-turn analysis.
    /// 读取一次待处理轮次分析所需的精确参考与主动写入事实。
    PostActionContext {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines exact visible and writable scope boundaries.
        /// 定义精确可见与可写作用域边界。
        scope_access: ScopeAccess,
        /// Identifies the exact host session.
        /// 标识精确宿主会话。
        session_source_id: SessionSourceId,
        /// Identifies the pending target turn.
        /// 标识待处理目标轮次。
        target_turn_id: TurnId,
        /// Limits already-refined reference turns before token-budget trimming.
        /// 限制在 Token 预算裁剪前加载的已提炼参考轮次数量。
        reference_turn_limit: u32,
        /// Excludes direct writes observed on or before this durable session watermark.
        /// 排除在该持久会话水位及之前已观测的主动写入。
        direct_write_observed_after_ns: i64,
        /// Freezes the inclusive upper time boundary for every returned fact.
        /// 冻结全部返回事实的闭区间时间上界。
        analysis_cutoff_ns: i64,
    },
    /// Reads visible profile nodes.
    /// 读取可见画像节点。
    ProfileNodes {
        /// Identifies the subject.
        /// 标识主体。
        subject_id: SubjectId,
        /// Defines scope boundaries.
        /// 定义作用域边界。
        scope_access: ScopeAccess,
        /// Carries the ordered role bindings that define subject isolation versus environment sharing.
        /// 携带定义主体隔离与环境共享边界的有序角色绑定。
        sections: Vec<ProfileViewSection>,
        /// Carries the deterministic lifecycle evaluation time.
        /// 携带确定性的生命周期评估时间。
        evaluated_at_ns: i64,
    },
}

/// Defines one caller-ordered recent direct-write deduplication lookup.
/// 定义一条按调用方顺序排列的近期主动写入去重查询。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectWriteDedupeLookup {
    /// Identifies the exact Host-resolved owning scope.
    /// 标识精确的 Host 已解析归属作用域。
    pub target_scope_id: ScopeId,
    /// Classifies the generic ownership horizon.
    /// 分类通用归属跨度。
    pub scope_class: crate::MemoryScopeClass,
    /// Carries the versioned Core-owned semantic deduplication hash.
    /// 携带版本化 Core 自有语义去重哈希。
    pub dedupe_hash: String,
}

/// Carries one context evidence group aligned with one requested memory identity.
/// 携带与一个请求记忆标识对齐的一个情境证据分组。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEvidenceGroup {
    /// Identifies the memory owning the group.
    /// 标识拥有该分组的记忆。
    pub memory_id: MemoryId,
    /// Carries evidence rows in stable key-value order.
    /// 以稳定键值顺序携带证据行。
    pub evidence: Vec<MemoryContextEvidence>,
}

/// Carries one complete retention scan fact batch from the storage layer.
/// 携带一批来自存储层的完整保留扫描事实。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetentionMemoryFact {
    /// Carries the complete memory evaluated by retention policy.
    /// 携带由保留策略评估的完整记忆。
    pub memory: MemoryRecord,
    /// States whether Host lifecycle rules classify this memory as protected-shared.
    /// 表示宿主生命周期规则是否把该记忆分类为受保护共享候选。
    pub protected_shared: bool,
}

/// Carries one session that currently exposes at least one recyclable cold turn.
/// 携带一个当前至少存在一个可回收冷 Turn 的会话。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionColdTurnCandidateFact {
    /// Identifies the exact host session.
    /// 标识精确宿主会话。
    pub session_source_id: SessionSourceId,
    /// Counts currently recyclable turns after Host reference and hot-window checks.
    /// 统计 Host 完成引用与热窗口检查后当前可回收的 Turn 数量。
    pub recyclable_turn_count: u32,
}

/// Carries one persisted cold-turn job visible to the current maintenance pass.
/// 携带一个当前维护轮次可见的持久冷 Turn 任务。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionColdTurnJobFact {
    /// Identifies the backend-owned durable job.
    /// 标识由后端拥有的持久任务。
    pub job_id: String,
    /// Identifies the exact host session.
    /// 标识精确宿主会话。
    pub session_source_id: SessionSourceId,
    /// Carries the next time this job may execute.
    /// 携带该任务下次可执行时间。
    pub next_run_at_ns: i64,
}

/// Carries one session candidate for idle-session compaction.
/// 携带一个闲置会话压缩候选。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionIdleSessionFact {
    /// Identifies the exact host session.
    /// 标识精确宿主会话。
    pub session_source_id: SessionSourceId,
    /// Carries the latest durable activity time.
    /// 携带最近持久活动时间。
    pub last_activity_ns: i64,
    /// States whether the session still owns a pending turn.
    /// 表示该会话是否仍拥有 Pending Turn。
    pub has_pending_turn: bool,
}

/// Carries one persisted vector-GC retry job.
/// 携带一个持久向量 GC 重试任务。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionVectorGcJobFact {
    /// Identifies the backend-owned durable job.
    /// 标识由后端拥有的持久任务。
    pub job_id: String,
    /// Identifies the obsolete sidecar vector.
    /// 标识过时旁路向量。
    pub vector_id: String,
    /// Carries the next time this job may execute.
    /// 携带该任务下次可执行时间。
    pub next_run_at_ns: i64,
}

/// Carries one completed recycle batch retained in the trash window.
/// 携带一个保留在回收站窗口内的已完成回收批次。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionTrashBatchFact {
    /// Identifies the backend-owned recycle batch.
    /// 标识由后端拥有的回收批次。
    pub batch_id: String,
    /// Carries the time at which the batch entered trash.
    /// 携带该批次进入回收站的时间。
    pub recycled_at_ns: i64,
}

/// Carries one complete retention scan fact batch from the storage layer.
/// 携带一批来自存储层的完整保留扫描事实。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetentionScanFacts {
    /// Echoes the exact subject identity used to read this retention fact batch.
    /// 回显读取此保留事实批次时使用的精确主体身份。
    pub subject_id: SubjectId,
    /// Echoes the exact write scope used to read this retention fact batch.
    /// 回显读取此保留事实批次时使用的精确写入作用域。
    pub scope_id: ScopeId,
    /// Carries terminal write-scope memories eligible for relational recycle evaluation.
    /// 携带可进入关系回收评估的写入作用域终态记忆。
    pub terminal_memories: Vec<RetentionMemoryFact>,
    /// Carries sessions discovered by the independent cold-turn scan.
    /// 携带由独立冷 Turn 扫描发现的会话。
    pub cold_turn_candidates: Vec<RetentionColdTurnCandidateFact>,
    /// Carries persisted cold-turn jobs available for due-time selection.
    /// 携带可用于到期选择的持久冷 Turn 任务。
    pub cold_turn_jobs: Vec<RetentionColdTurnJobFact>,
    /// Carries sessions available for idle compaction evaluation.
    /// 携带可进入闲置压缩评估的会话。
    pub idle_sessions: Vec<RetentionIdleSessionFact>,
    /// Carries persisted vector-GC jobs available for due-time selection.
    /// 携带可用于到期选择的持久向量 GC 任务。
    pub vector_gc_jobs: Vec<RetentionVectorGcJobFact>,
    /// Carries completed recycle batches retained for delayed purge.
    /// 携带为延迟清除而保留的已完成回收批次。
    pub trash_batches: Vec<RetentionTrashBatchFact>,
}

/// Carries one physical channel together with optional records materialized by the same read.
/// 携带一条物理通道，以及由同一次读取物化的可选记录。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateChannelResult {
    /// Carries physical candidates in producer order.
    /// 按生产者顺序携带物理候选。
    pub candidates: Vec<PhysicalCandidate>,
    /// Carries the exact candidate records observed by this channel; empty preserves detached backends.
    /// 携带本通道观察到的精确候选记录；空集合用于保留分离式后端。
    pub memories: Vec<MemoryRecord>,
}

/// Reports one storage query result.
/// 报告一次存储查询结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StorageQueryResult {
    /// Carries complete channels and records from the same physical observation.
    /// 携带同次物理观察的完整通道和记录。
    CandidateObservation(crate::CandidateObservation),
    /// Carries physical vector candidates and any records from that same observation.
    /// 携带物理向量候选及同次观察得到的记录。
    VectorCandidates(CandidateChannelResult),
    /// Carries physical lexical candidates and any records from that same observation.
    /// 携带物理词法候选及同次观察得到的记录。
    LexicalCandidates(CandidateChannelResult),
    /// Carries optional records aligned with the requested memory identities.
    /// 携带与请求记忆标识对齐的可选记录。
    MemoryRecordBatch(Vec<Option<MemoryRecord>>),
    /// Carries optional recent direct writes aligned with requested deduplication lookups.
    /// 携带与请求去重查询对齐的可选近期主动写入。
    RecentDirectWriteDedupe(Vec<Option<MemoryRecord>>),
    /// Carries context evidence groups aligned with the requested identities.
    /// 携带与请求标识对齐的情境证据分组。
    ContextEvidenceBatch(Vec<ContextEvidenceGroup>),
    /// Carries one persisted noise prototype bundle.
    /// 携带一个已持久化的 Noise 原型束。
    NoisePrototypes {
        /// Identifies the cache identity that was consulted.
        /// 标识被查询的缓存身份。
        identity: NoisePrototypeCacheIdentity,
        /// Carries the persisted prototypes or an empty vector on cache miss.
        /// 携带已持久化原型，或在缓存未命中时为空向量。
        prototypes: Vec<NoisePrototype>,
    },
    /// Carries one retention scan fact batch.
    /// 携带一批保留扫描事实。
    RetentionScan(RetentionScanFacts),
    /// Carries optional memory details.
    /// 携带可选记忆详情。
    MemoryDetails(Box<Option<MemoryRecord>>),
    /// Carries caller-ordered memory and turn details with explicit missing entries.
    /// 携带按调用方顺序排列且显式表示缺失项的记忆与轮次详情。
    DetailsBatch {
        /// Carries one optional record for every requested memory identity.
        /// 为每个请求的记忆标识携带一条可选记录。
        memories: Vec<Option<MemoryRecord>>,
        /// Carries one optional turn for every requested turn identity.
        /// 为每个请求的轮次标识携带一个可选轮次。
        turns: Vec<Option<Turn>>,
    },
    /// Carries the exact target, refined references, and direct-write exclusions.
    /// 携带精确目标轮次、已提炼参考轮次和主动写入排除项。
    PostActionContext {
        /// Carries the exact pending target turn.
        /// 携带精确待处理目标轮次。
        target_turn: Box<Turn>,
        /// Carries bounded already-refined reference turns in chronological order.
        /// 按时间顺序携带有界的已提炼参考轮次。
        reference_turns: Vec<PostActionReferenceTurn>,
        /// Carries recent direct writes in chronological order.
        /// 按时间顺序携带近期主动写入。
        recent_direct_writes: Vec<PostActionDirectWrite>,
    },
    /// Carries visible profile nodes.
    /// 携带可见画像节点。
    ProfileNodes(Vec<ProfileNode>),
}

/// Binds one mutation to the immutable access identity observed during its authorized preflight.
/// 将一个变更绑定到授权预检时观测到的不可变访问身份。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryAccessIdentity {
    /// Carries the exact observed access class.
    /// 携带精确观测到的访问类别。
    pub access_class: MemoryAccessClass,
    /// Carries the exact optional source subject, including explicit unknown for shared memories.
    /// 携带精确可选来源主体，包括共享记忆显式未知的情况。
    pub source_subject_id: Option<SubjectId>,
}

impl From<&MemoryRecord> for MemoryAccessIdentity {
    /// Copies the immutable access identity from one validated memory snapshot.
    /// 从一条已校验记忆快照复制不可变访问身份。
    fn from(value: &MemoryRecord) -> Self {
        Self {
            access_class: value.access_class,
            source_subject_id: value.source_subject_id.clone(),
        }
    }
}

/// Defines one atomic storage mutation item.
/// 定义一个原子存储变更项目。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Mutation {
    /// Stores one canonical turn through a command-local slot or an exact existing identity.
    /// 通过命令局部槽位或精确既有身份存储一个规范化对话轮次。
    PutTurn {
        /// References the existing turn or the command-local new slot.
        /// 引用既有轮次或命令局部新槽位。
        entity: EntityRef,
        /// Carries the turn; a new slot's provisional ID must be replaced by the Host binding.
        /// 携带轮次；新槽位中的临时 ID 必须由 Host 绑定替换，不能作为持久身份使用。
        turn: Turn,
    },
    /// Atomically transitions one turn between explicit durable states.
    /// 在显式持久状态之间原子迁移一个轮次。
    TransitionTurnStatus {
        /// Identifies the turn.
        /// 标识轮次。
        turn_id: TurnId,
        /// Requires the current durable status.
        /// 要求当前持久状态。
        from: TurnStatus,
        /// Publishes the next durable status.
        /// 发布下一持久状态。
        to: TurnStatus,
        /// Carries the dominant user act when the transition completes analysis.
        /// 当迁移完成分析时携带主要用户行为。
        user_input_kind: Option<TurnInputKind>,
        /// Carries compact analyzed details when the transition completes analysis.
        /// 当迁移完成分析时携带紧凑分析详情。
        analysis_details: Option<String>,
    },
    /// Atomically resolves the latest persisted session turn and advances its compact boundary.
    /// 原子解析会话最新持久化轮次并推进其压缩边界。
    AdvanceChatCompactBoundary {
        /// Identifies the resolved subject owner.
        /// 标识已解析主体所有者。
        subject_id: SubjectId,
        /// Identifies the exact session write scope.
        /// 标识精确的会话写入作用域。
        scope_id: crate::ScopeId,
        /// Identifies the host session source.
        /// 标识宿主会话来源。
        session_source_id: SessionSourceId,
        /// Carries the caller-observed compaction time.
        /// 携带调用方观察到的压缩时间。
        compacted_at_ns: i64,
    },
    /// Restores one exact compact boundary during portable import.
    /// 在可移植导入期间恢复一个精确压缩边界。
    PutChatCompactBoundary(crate::ChatCompactBoundary),
    /// Restores one exact memory snapshot during portable import.
    /// 在可移植导入期间恢复一条精确记忆快照。
    RestoreImportedMemory(MemoryRecord),
    /// Restores one exact profile-node snapshot during portable import.
    /// 在可移植导入期间恢复一个精确画像节点快照。
    RestoreImportedProfileNode(ProfileNode),
    /// Restores one exact context-evidence snapshot during portable import.
    /// 在可移植导入期间恢复一条精确情境证据快照。
    RestoreImportedContextEvidence(crate::MemoryContextEvidence),
    /// Creates one new memory through a command-local slot or replaces one existing record.
    /// 通过命令局部槽位创建新记忆，或替换一条既有记录。
    PutMemory {
        /// References the existing record or the command-local new slot.
        /// 引用既有记录或命令局部新槽位。
        entity: EntityRef,
        /// Carries the memory; a new slot's provisional ID must be replaced by the Host binding.
        /// 携带记忆；新槽位中的临时 ID 必须由 Host 绑定替换，不能作为持久身份使用。
        record: MemoryRecord,
    },
    /// Records one durable adoption of one recalled memory.
    /// 记录一条召回记忆的持久采纳。
    /// Advances recall/adoption/reinforcement totals and `max(refresh_weight, 1)` by one; only known different origin/current sessions count as cross-session.
    /// 递增召回/采纳/强化总量和至少为 1 的刷新权重；仅明确不同的来源/当前会话计为跨会话。
    ReinforceMemory {
        /// Identifies the subject performing the authorized adoption.
        /// 标识执行授权采纳的主体。
        actor_subject_id: SubjectId,
        /// Identifies the exact authorized owning scope.
        /// 标识精确获授权所属作用域。
        scope_id: ScopeId,
        /// Requires the immutable access identity observed during recall.
        /// 要求召回时观测到的不可变访问身份。
        expected_access: MemoryAccessIdentity,
        /// Identifies the recalled memory.
        /// 标识召回的记忆。
        memory_id: MemoryId,
        /// Requires the exact current version.
        /// 要求精确当前版本。
        expected_version: u64,
        /// Carries the recall observation time.
        /// 携带召回观测时间。
        recalled_at_ns: i64,
        /// Carries the adoption source session identity for cross-session accounting.
        /// 携带用于跨会话统计的采纳来源会话标识。
        source_session_id: Option<SessionSourceId>,
    },
    /// Retires one existing memory in favor of an accepted replacement.
    /// 退役一条既有记忆并启用已接受的替代者。
    SupersedeMemory {
        /// Identifies the subject performing the authorized replacement.
        /// 标识执行授权替代的主体。
        actor_subject_id: SubjectId,
        /// Binds the exact pre-existing target scope independently of the replacement's creation scope.
        /// 独立于替代记录的新建作用域绑定既有目标的精确作用域。
        scope_id: crate::ScopeId,
        /// Requires the immutable access identity observed during review.
        /// 要求审核时观测到的不可变访问身份。
        expected_access: MemoryAccessIdentity,
        /// Identifies the retired memory.
        /// 标识退役记忆。
        memory_id: MemoryId,
        /// Requires the exact current version.
        /// 要求精确当前版本。
        expected_version: u64,
        /// References the replacement memory domain identity or same-batch new slot.
        /// 引用替代记忆领域身份或同批次新槽位。
        superseded_by: EntityRef,
        /// Carries the stable supersede reason.
        /// 携带稳定的替代原因。
        status_reason: Option<String>,
    },
    /// Aggregates one context evidence observation onto one memory.
    /// 将一条情境证据观测聚合到一个记忆上。
    AggregateContextEvidence {
        /// References the owning memory domain identity or same-batch new slot.
        /// 引用所属记忆领域身份或同批次新槽位。
        memory: EntityRef,
        /// Carries the normalized context key.
        /// 携带规范化的情境键。
        context_key: String,
        /// Carries the normalized context value.
        /// 携带规范化的情境值。
        context_value: String,
        /// Classifies the support or rebuttal direction.
        /// 对支持或反驳方向分类。
        relation: ContextEvidenceRelation,
        /// Carries the observation time.
        /// 携带观测时间。
        occurred_at_ns: i64,
    },
    /// Deletes one memory.
    /// 删除一条记忆。
    DeleteMemory {
        /// Identifies the subject performing the authorized deletion.
        /// 标识执行授权删除的主体。
        actor_subject_id: SubjectId,
        /// Identifies the only allowed scope.
        /// 标识唯一允许的作用域。
        scope_id: crate::ScopeId,
        /// Requires the immutable access identity observed during preflight.
        /// 要求预检时观测到的不可变访问身份。
        expected_access: MemoryAccessIdentity,
        /// Identifies the memory.
        /// 标识记忆。
        memory_id: MemoryId,
        /// Carries an optional expected version.
        /// 携带可选期望版本。
        expected_version: Option<u64>,
    },
    /// Creates one new profile node through a command-local slot or replaces one existing node.
    /// 通过命令局部槽位创建新画像节点，或替换一个既有节点。
    PutProfileNode {
        /// References the existing node or the command-local new slot.
        /// 引用既有节点或命令局部新槽位。
        entity: EntityRef,
        /// Carries the profile; a new slot's provisional ID must be replaced by the Host binding.
        /// 携带画像；新槽位中的临时 ID 必须由 Host 绑定替换，不能作为持久身份使用。
        node: ProfileNode,
    },
    /// Retires one exactly owned profile node while retaining audit history.
    /// 退役一个精确所属画像节点，同时保留审计历史。
    RetireProfileNode {
        /// Identifies the observed originating subject when the durable node retained one.
        /// 在持久节点保留了来源主体时标识预检观测到的该主体。
        subject_id: Option<SubjectId>,
        /// Identifies the exact write scope.
        /// 标识精确写入作用域。
        scope_id: crate::ScopeId,
        /// Identifies the profile node.
        /// 标识画像节点。
        profile_node_id: crate::ProfileNodeId,
        /// Carries the required optimistic version.
        /// 携带必需乐观版本。
        expected_version: u64,
        /// References the superseding existing node or same-batch new node slot.
        /// 引用替代该节点的既有节点或同批次新节点槽位。
        superseded_by: Option<EntityRef>,
        /// Carries the stable retirement reason.
        /// 携带稳定的退役原因。
        status_reason: Option<String>,
    },
    /// Persists one authoritative analysis attempt identity before its first external effect.
    /// 在其首个外部 Effect 前持久化一条权威的分析尝试身份。
    PutAnalysisAttempt(AnalysisAttemptIdentity),
    /// Atomically replaces one complete semantic-noise prototype bundle.
    /// 原子替换一个完整的语义 Noise 原型束。
    PutNoisePrototypeBundle {
        /// Identifies the exact prototype cache identity.
        /// 标识精确的原型缓存身份。
        identity: NoisePrototypeCacheIdentity,
        /// Carries every normalized prototype row.
        /// 携带每个规范化原型行。
        prototypes: Vec<NoisePrototype>,
    },
    /// Marks one memory recycled without physical deletion.
    /// 将一条记忆标记为已回收而不做物理删除。
    RecycleMemory {
        /// Identifies the subject performing the authorized recycle.
        /// 标识执行授权回收的主体。
        actor_subject_id: SubjectId,
        /// Identifies the only allowed scope.
        /// 标识唯一允许的作用域。
        scope_id: crate::ScopeId,
        /// Requires the immutable access identity observed during retention planning.
        /// 要求保留规划时观测到的不可变访问身份。
        expected_access: MemoryAccessIdentity,
        /// Identifies the memory.
        /// 标识记忆。
        memory_id: MemoryId,
        /// Carries the expected version.
        /// 携带期望版本。
        expected_version: u64,
        /// Carries the stable recycle reason.
        /// 携带稳定的回收原因。
        status_reason: Option<String>,
    },
    /// Restores one recycled memory to active state.
    /// 将一条已回收记忆恢复为活跃状态。
    RestoreMemory {
        /// Identifies the subject performing the authorized restore.
        /// 标识执行授权恢复的主体。
        actor_subject_id: SubjectId,
        /// Identifies the exact write scope.
        /// 标识精确写入作用域。
        scope_id: crate::ScopeId,
        /// Requires the immutable access identity observed during maintenance preflight.
        /// 要求维护预检时观测到的不可变访问身份。
        expected_access: MemoryAccessIdentity,
        /// Identifies the memory.
        /// 标识记忆。
        memory_id: MemoryId,
        /// Carries the required optimistic version.
        /// 携带必需乐观版本。
        expected_version: u64,
    },
    /// Physically purges one recycled memory.
    /// 物理清除一条已回收记忆。
    PurgeMemory {
        /// Identifies the subject performing the authorized purge.
        /// 标识执行授权清除的主体。
        actor_subject_id: SubjectId,
        /// Identifies the exact write scope.
        /// 标识精确写入作用域。
        scope_id: crate::ScopeId,
        /// Requires the immutable access identity observed during maintenance preflight.
        /// 要求维护预检时观测到的不可变访问身份。
        expected_access: MemoryAccessIdentity,
        /// Identifies the memory.
        /// 标识记忆。
        memory_id: MemoryId,
        /// Carries the required optimistic version.
        /// 携带必需乐观版本。
        expected_version: u64,
    },
}

/// Carries one all-or-nothing storage mutation plan under its persistent host identity.
/// 携带一份在其宿主持久身份下的全有或全无存储变更计划。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MutationBatch {
    /// Carries the complete persistent host operation identity.
    /// 携带完整的宿主持久操作身份。
    pub identity: MutationIdentity,
    /// Carries ordered mutations applied atomically.
    /// 携带按顺序原子应用的变更。
    pub mutations: Vec<Mutation>,
}

/// Enumerates provider-agnostic host effects.
/// 枚举与 Provider 无关的宿主 Effect。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HostEffect {
    /// Requests structured LLM inference.
    /// 请求结构化 LLM 推理。
    Llm(LlmEffect),
    /// Requests batch embeddings.
    /// 请求批量嵌入。
    Embedding(EmbeddingEffect),
    /// Requests batch reranking.
    /// 请求批量重排。
    Rerank(RerankEffect),
    /// Requests a semantic storage query.
    /// 请求语义存储查询。
    StorageQuery(StorageQuery),
    /// Requests one atomic mutation batch.
    /// 请求一个原子变更批次。
    StorageMutation(MutationBatch),
}

/// Classifies the host deadline budget without embedding provider routing.
/// 在不嵌入 Provider 路由的前提下分类宿主截止时间预算。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum EffectDeadlineClass {
    /// Indicates an interactive request on the user-visible path.
    /// 表示用户可见路径上的交互请求。
    Interactive = 1,
    /// Indicates background analysis or writeback.
    /// 表示后台分析或写回。
    Background = 2,
    /// Indicates scheduled maintenance work.
    /// 表示调度维护工作。
    Maintenance = 3,
}

/// Associates one effect payload with a stable identity.
/// 将一个 Effect 载荷与稳定标识关联。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectRequest {
    /// Identifies the effect.
    /// 标识 Effect。
    pub effect_id: EffectId,
    /// Identifies the owning command for host telemetry and cancellation.
    /// 标识用于宿主遥测与取消的所属命令。
    pub command_id: CommandId,
    /// Identifies the frozen configuration snapshot governing this effect.
    /// 标识管理该 Effect 的冻结配置快照。
    pub config_snapshot_id: ConfigSnapshotId,
    /// Carries the persistent operation identity shared with the owning command.
    /// 携带与所属命令共享的持久操作身份。
    pub operation_id: Option<crate::OperationId>,
    /// Classifies the host deadline budget.
    /// 对宿主截止时间预算分类。
    pub deadline_class: EffectDeadlineClass,
    /// States whether the host may safely retry before returning a result.
    /// 表示宿主在返回结果前是否可安全重试。
    pub retryable: bool,
    /// States whether execution may produce durable side effects.
    /// 表示执行是否可能产生持久副作用。
    pub side_effect_possible: bool,
    /// Carries the typed effect payload.
    /// 携带强类型 Effect 载荷。
    pub effect: HostEffect,
}

/// Describes the durable outcome of one mutation batch.
/// 描述一个变更批次的持久结果。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MutationOutcome {
    /// Confirms the batch committed atomically and returns its durable receipt.
    /// 确认批次已原子提交并返回其持久收据。
    Committed {
        /// Carries the resulting monotonic storage version, including zero for an initial no-op.
        /// 携带最终单调存储版本；初始无操作提交时允许为零。
        storage_version: u64,
        /// Carries compact-boundary metadata only for an advance-boundary batch.
        /// 仅为推进边界批次携带压缩边界元数据。
        chat_compact: Option<ChatCompactMutationResult>,
        /// Carries the durable commit receipt with entity bindings.
        /// 携带带实体绑定的持久提交收据。
        receipt: CommitReceipt,
    },
    /// Confirms the batch was rejected without side effects.
    /// 确认批次在无副作用情况下被拒绝。
    Rejected {
        /// Carries a stable diagnostic reason.
        /// 携带稳定诊断原因。
        reason: String,
    },
    /// Indicates the host cannot determine whether the batch committed.
    /// 表示宿主无法确定批次是否已提交。
    Uncertain,
}

/// Classifies the failed effect surface.
/// 对失败的 Effect 表面分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectFailureKind {
    /// One structured LLM effect failed.
    /// 一个结构化 LLM Effect 失败。
    Llm = 1,
    /// One embedding effect failed.
    /// 一个嵌入 Effect 失败。
    Embedding = 2,
    /// One rerank effect failed.
    /// 一个重排 Effect 失败。
    Rerank = 3,
    /// One storage query effect failed.
    /// 一个存储查询 Effect 失败。
    StorageQuery = 5,
    /// One storage mutation effect failed.
    /// 一个存储变更 Effect 失败。
    StorageMutation = 6,
}

impl EffectFailureKind {
    /// Converts one closed discriminant into the effect failure kind.
    /// 将一个封闭判别值转换为 Effect 失败类别。
    #[must_use]
    pub const fn try_from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Llm),
            2 => Some(Self::Embedding),
            3 => Some(Self::Rerank),
            5 => Some(Self::StorageQuery),
            6 => Some(Self::StorageMutation),
            _ => None,
        }
    }

    /// Returns the stable numeric discriminant.
    /// 返回稳定数值判别值。
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Classifies why one effect failed.
/// 对一个 Effect 的失败原因分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureClass {
    /// The provider transport failed before producing a structured result.
    /// 提供方传输在产生结构化结果前失败。
    Transport = 1,
    /// The provider returned a structured contract violation.
    /// 提供方返回结构化契约违规。
    StructuredContract = 2,
    /// The provider failed while producing a structured result.
    /// 提供方在产生结构化结果期间失败。
    Provider = 3,
    /// The host storage backend failed.
    /// 宿主存储后端失败。
    Storage = 4,
}

impl FailureClass {
    /// Converts one closed discriminant into the failure class.
    /// 将一个封闭判别值转换为失败类别。
    #[must_use]
    pub const fn try_from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Transport),
            2 => Some(Self::StructuredContract),
            3 => Some(Self::Provider),
            4 => Some(Self::Storage),
            _ => None,
        }
    }

    /// Returns the stable numeric discriminant.
    /// 返回稳定数值判别值。
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Carries one structured host effect failure eligible for semantic degradation.
/// 携带一个可参与语义降级的结构化宿主 Effect 失败。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectFailure {
    /// Classifies the failed effect surface.
    /// 对失败的 Effect 表面分类。
    pub kind: EffectFailureKind,
    /// Classifies why the effect failed.
    /// 对失败原因分类。
    pub class: FailureClass,
    /// States whether retrying the same effect is safe.
    /// 表示重试同一 Effect 是否安全。
    pub retryable: bool,
    /// States whether Core may semantically degrade instead of failing.
    /// 表示 Core 是否可语义降级而非失败。
    pub allow_degrade: bool,
    /// Carries a bounded developer diagnostic.
    /// 携带有界开发者诊断。
    pub diagnostic: String,
}

/// Enumerates typed host effect results.
/// 枚举强类型宿主 Effect 结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HostEffectResult {
    /// Carries structured LLM output.
    /// 携带结构化 LLM 输出。
    Llm(LlmOutput),
    /// Carries vectors in the same order as embedding inputs.
    /// 按嵌入输入相同顺序携带向量。
    Embedding(EmbeddingBatchResult),
    /// Carries provider-ordered occurrences, permitting partial or empty results without fabricated scores.
    /// 携带按提供方排序的条目，允许部分或空结果，不伪造缺失评分。
    Rerank(Vec<RerankEntry>),
    /// Carries one storage query result.
    /// 携带一次存储查询结果。
    StorageQuery(StorageQueryResult),
    /// Carries one mutation outcome.
    /// 携带一次变更结果。
    StorageMutation(MutationOutcome),
    /// Carries one explicit host failure eligible for semantic degradation.
    /// 携带一个可参与语义降级的显式宿主失败。
    Failure(EffectFailure),
}

/// Associates one host result with the requested effect.
/// 将一个宿主结果与请求的 Effect 关联。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectResult {
    /// Identifies the completed effect.
    /// 标识已完成 Effect。
    pub effect_id: EffectId,
    /// Carries the typed result.
    /// 携带强类型结果。
    pub result: HostEffectResult,
}

/// Returns either the next effect batch or a final result.
/// 返回下一批 Effect 或最终结果。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CommandStep {
    /// Requests the host to execute all effects in order.
    /// 请求宿主按顺序执行全部 Effect。
    Effects(Vec<EffectRequest>),
    /// Reports final command completion.
    /// 报告命令最终完成。
    Complete(Box<CommandResult>),
}

/// Identifies which role a noise category applies to.
/// 标识噪声类别适用的角色。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoiseTarget {
    /// Carries the applicable role.
    /// 携带适用角色。
    pub role: Role,
}

/// Carries the complete invalidation identity of one semantic-noise prototype cache.
/// 携带一个语义 Noise 原型缓存的完整失效身份。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoisePrototypeCacheIdentity {
    /// Identifies the exact cache scope.
    /// 标识精确缓存作用域。
    pub scope_id: crate::ScopeId,
    /// Names the resolved language bundle.
    /// 命名已解析的语言 Bundle。
    pub language: String,
    /// Identifies the host-resolved embedding model fingerprint.
    /// 标识宿主解析的嵌入模型摘要。
    pub model: String,
    /// Carries the embedding dimension.
    /// 携带嵌入维度。
    pub dimension: u32,
    /// Carries the lowercase SHA-256 digest of the compiled noise rules.
    /// 携带已编译 Noise 规则的小写 SHA-256 摘要。
    pub rules_hash: String,
}

/// Carries one normalized semantic-noise prototype vector row.
/// 携带一个规范化的语义 Noise 原型向量行。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoisePrototype {
    /// Names the owning noise category.
    /// 命名归属的 Noise 类别。
    pub category: String,
    /// Carries the source phrase text.
    /// 携带来源短语文本。
    pub phrase: String,
    /// Carries the L2-normalized prototype vector.
    /// 携带 L2 规范化的原型向量。
    pub vector: Vec<f32>,
}
