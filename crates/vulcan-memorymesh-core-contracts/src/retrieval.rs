//! Physical score contracts, candidate facts, retrieval policy, and score explanations.
//! 物理评分契约、候选事实、检索策略与评分解释。

use crate::{CoreError, ErrorCode, ErrorStage};
use serde::{Deserialize, Serialize};

/// Classifies the physical raw score space of one storage producer.
/// 对一个存储生产者的物理原始分值空间分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum RawScoreKind {
    /// Indicates a cosine distance where smaller is closer.
    /// 表示越小越接近的余弦距离。
    CosineDistance = 1,
    /// Indicates a cosine similarity where larger is closer.
    /// 表示越大越接近的余弦相似度。
    CosineSimilarity = 2,
    /// Indicates a BM25-style lexical score with producer-declared direction.
    /// 表示方向由生产者声明的 BM25 风格词法分值。
    Bm25 = 3,
    /// Indicates a normalized token-overlap lexical score.
    /// 表示规范化词元重合度词法分值。
    TokenOverlap = 4,
    /// Indicates a trigram similarity score.
    /// 表示三元组相似度分值。
    TrigramSimilarity = 5,
    /// Indicates a reciprocal-rank fusion score space.
    /// 表示倒数排名融合分值空间。
    ReciprocalRankFusion = 6,
    /// Indicates native L2 distance; the Host contract fixes its exact scale and projection.
    /// 表示原生 L2 距离；其精确尺度及投影由 Host 契约固定。
    L2Distance = 7,
}

/// Declares the optimization direction of one raw score space.
/// 声明一个原始分值空间的优化方向。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ScoreDirection {
    /// Indicates smaller raw values rank higher.
    /// 表示更小的原始值排名更高。
    LowerIsBetter = 1,
    /// Indicates larger raw values rank higher.
    /// 表示更大的原始值排名更高。
    HigherIsBetter = 2,
}

/// Identifies one versioned score contract declared by the host adapter.
/// 标识一个由宿主适配器声明的版本化评分契约。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreContractRef {
    /// Carries the host-declared closed contract identity.
    /// 携带由宿主声明的封闭契约身份。
    pub id: String,
    /// Carries the contract version.
    /// 携带契约版本。
    pub version: u32,
}

/// Declares whether a physical producer supplies a normalized decision value or only ordering facts.
/// 声明物理生产者提供归一化决策值还是仅提供排序事实，不从数据库名称或分值猜测。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum DecisionScoreMode {
    /// Requires an explicit finite host-projected value, including a meaningful zero.
    /// 要求明确的有限宿主投影值，包括有真实含义的零分。
    HostProjected = 1,
    /// Forbids a fabricated per-channel decision value; fusion consumes physical rank instead.
    /// 禁止伪造单通道决策值；融合改为消费物理排名。
    RankOnly = 2,
}

/// Declares the immutable raw-score semantics attached to one versioned producer contract.
/// 声明绑定到一个版本化生产者契约的不可变原始评分语义。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreContractDescriptor {
    /// Identifies the exact producer contract.
    /// 标识精确的生产者契约。
    pub contract: ScoreContractRef,
    /// Declares the raw score space produced by the host adapter.
    /// 声明宿主适配器产生的原始评分空间。
    pub raw_score_kind: RawScoreKind,
    /// Declares how raw values are physically ordered by the producer.
    /// 声明生产者对原始值采用的物理排序方向。
    pub direction: ScoreDirection,
    /// Declares the required presence or absence of a per-candidate decision score.
    /// 声明每条候选决策分必须存在或必须不存在。
    pub decision_score_mode: DecisionScoreMode,
}

/// Carries one complete physical candidate score fact.
/// 携带一个完整的物理候选评分事实。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicalCandidateScore {
    /// Carries the raw backend score value.
    /// 携带后端原始分值。
    pub raw_value: f64,
    /// Classifies the raw score space.
    /// 对原始分值空间分类。
    pub raw_score_kind: RawScoreKind,
    /// Declares the raw score direction.
    /// 声明原始分值方向。
    pub direction: ScoreDirection,
    /// Carries an explicit host projection, or no value for a declared rank-only producer.
    /// 携带明确宿主投影；声明为仅排名的生产者不提供该值，缺省不等于零。
    pub decision_score: Option<f64>,
    /// Identifies the versioned contract that produced the decision score.
    /// 标识产生该决策分值的版本化契约。
    pub contract: ScoreContractRef,
}

impl PhysicalCandidateScore {
    /// Returns the normalized value required by a score-consuming stage.
    /// 返回评分消费阶段所需的归一值；仅排名事实不能用于该阶段。
    ///
    /// # Errors
    /// Rejects absent, nonfinite or out-of-range values without inventing a fallback score.
    /// 对缺失、非有限或越界值明确报错，不编造替代分数。
    pub fn require_decision_score(&self) -> Result<f64, CoreError> {
        self.decision_score
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(|| {
                retrieval_error(
                    ErrorCode::ScoreContractViolation,
                    "this stage requires a finite host-projected decision score",
                )
            })
    }
}

/// Carries one physical retrieval candidate with its score fact and physical rank.
/// 携带一个带评分事实与物理排名的物理检索候选。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicalCandidate {
    /// Identifies the candidate memory.
    /// 标识候选记忆。
    pub memory_id: String,
    /// Carries the complete score fact.
    /// 携带完整评分事实。
    pub score: PhysicalCandidateScore,
    /// Carries the original zero-based physical rank; relational filtering may leave gaps.
    /// 携带原始零基物理排名；关系过滤可留下空洞，不能改写为业务融合位置。
    pub physical_rank: u32,
}

/// Validates one physical candidate score against the closed score contract.
/// 针对封闭评分契约校验一个物理候选评分。
///
/// # Errors
/// Returns a stable error for out-of-range decision scores, non-finite values, or invalid contracts.
/// 当决策分值越界、值非有限或契约无效时返回稳定错误。
pub fn validate_physical_candidate_score(score: &PhysicalCandidateScore) -> Result<(), CoreError> {
    if score.contract.id.trim().is_empty()
        || score.contract.id.len() > 128
        || score.contract.version == 0
    {
        return Err(retrieval_error(
            ErrorCode::ScoreContractViolation,
            "score contract identity is empty, oversized, or unversioned",
        ));
    }
    if !score.raw_value.is_finite() || score.decision_score.is_some_and(|value| !value.is_finite())
    {
        return Err(retrieval_error(
            ErrorCode::ScoreContractViolation,
            "physical candidate score is not finite",
        ));
    }
    if score
        .decision_score
        .is_some_and(|value| !(0.0..=1.0).contains(&value))
    {
        return Err(retrieval_error(
            ErrorCode::ScoreContractViolation,
            "decision score must stay inside the inclusive range zero to one",
        ));
    }
    Ok(())
}

/// Validates one ordered candidate channel for contract and rank monotonicity.
/// 校验一个有序候选通道的契约与排名单调性。
///
/// # Errors
/// Returns a stable error when ranks are not strictly increasing or any score is invalid.
/// 当排名非严格递增或任一评分无效时返回稳定错误。
pub fn validate_candidate_channel(candidates: &[PhysicalCandidate]) -> Result<(), CoreError> {
    let mut memory_ids = std::collections::BTreeSet::new();
    for candidate in candidates {
        if crate::MemoryId::new(&candidate.memory_id).is_err()
            || !memory_ids.insert(candidate.memory_id.as_str())
        {
            return Err(retrieval_error(
                ErrorCode::ScoreContractViolation,
                "candidate memory identities must be valid and unique within one channel",
            ));
        }
        validate_physical_candidate_score(&candidate.score)?;
    }
    // Every candidate inside one channel must come from the same score contract and share
    // one raw kind and direction; otherwise the fusion rank is meaningless. An empty channel
    // is a legitimate no-results outcome.
    // 同一通道内每个候选必须来自同一评分契约并共享一种原始类别与方向；
    // 否则融合排名无意义。空通道是合法的无结果结局。
    let Some(reference) = candidates.first().map(|candidate| {
        (
            candidate.score.contract.clone(),
            candidate.score.raw_score_kind,
            candidate.score.direction,
            candidate.score.decision_score.is_some(),
        )
    }) else {
        return Ok(());
    };
    for candidate in candidates {
        let score = &candidate.score;
        if score.contract != reference.0
            || score.raw_score_kind != reference.1
            || score.direction != reference.2
            || score.decision_score.is_some() != reference.3
        {
            return Err(retrieval_error(
                ErrorCode::ScoreContractViolation,
                "candidate channel mixes score contracts, raw kinds, or directions",
            ));
        }
    }
    // Decision scores must not increase with the physical rank because rank zero is the best
    // candidate; ties are allowed, backsliding is not.
    // 决策分值不得随物理排名升高而升高，因为零号排名是最优候选；允许并列，不允许回退。
    for window in candidates.windows(2) {
        let [head, tail] = window else { break };
        if tail.physical_rank <= head.physical_rank {
            return Err(retrieval_error(
                ErrorCode::ScoreContractViolation,
                "physical ranks must be strictly increasing; gaps retain filtered source positions",
            ));
        }
        let raw_backslides = match head.score.direction {
            ScoreDirection::HigherIsBetter => tail.score.raw_value > head.score.raw_value,
            ScoreDirection::LowerIsBetter => tail.score.raw_value < head.score.raw_value,
        };
        if raw_backslides {
            return Err(retrieval_error(
                ErrorCode::ScoreContractViolation,
                "raw scores contradict the declared direction and physical rank",
            ));
        }
        if matches!((tail.score.decision_score, head.score.decision_score), (Some(tail), Some(head)) if tail > head)
        {
            return Err(retrieval_error(
                ErrorCode::ScoreContractViolation,
                "decision scores must be monotonically non-increasing along the channel",
            ));
        }
    }
    Ok(())
}

/// Validates that every candidate in one channel was scored by the requested contract.
/// 校验一个通道内每个候选都由请求的契约评分。
///
/// # Errors
/// Returns a stable error when any candidate contract differs from the expectation.
/// 当任一候选契约与期望不同时返回稳定错误。
pub fn validate_candidate_channel_contract(
    candidates: &[PhysicalCandidate],
    expected: &ScoreContractDescriptor,
) -> Result<(), CoreError> {
    for candidate in candidates {
        if candidate.score.contract != expected.contract
            || candidate.score.raw_score_kind != expected.raw_score_kind
            || candidate.score.direction != expected.direction
            || candidate.score.decision_score.is_some()
                != (expected.decision_score_mode == DecisionScoreMode::HostProjected)
        {
            return Err(retrieval_error(
                ErrorCode::ScoreContractViolation,
                format!(
                    "candidate score semantics {:?}/{:?}/{:?}/present={} differ from the requested {:?}/{:?}/{:?}/{:?}",
                    candidate.score.contract,
                    candidate.score.raw_score_kind,
                    candidate.score.direction,
                    candidate.score.decision_score.is_some(),
                    expected.contract,
                    expected.raw_score_kind,
                    expected.direction,
                    expected.decision_score_mode,
                ),
            ));
        }
    }
    Ok(())
}

/// Creates one retrieval-contract error.
/// 创建一个检索契约错误。
fn retrieval_error(code: ErrorCode, diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(code, ErrorStage::Effect, diagnostic)
}

/// Binds one business threshold to the exact versioned score contract it may be applied to.
/// 将一个业务阈值绑定到它可能被应用的精确版本化评分契约。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThresholdBinding {
    /// Identifies the required contract.
    /// 标识所需契约。
    pub contract: ScoreContractRef,
    /// Carries the threshold as binary64 values of the inclusive zero-to-one score range.
    /// 携带以零到一闭区间评分范围双精度阈值。
    pub threshold: f64,
}

/// Freezes the source arithmetic sequence independently of the machine restoring a command.
/// 独立于恢复命令的机器冻结来源算术序列，禁止恢复时按当前处理器重新选择。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
#[repr(u32)]
pub enum WeibullMathProfile {
    /// Reproduces the frozen Go amd64 sequence with separate multiply/add operations.
    /// 复现冻结 Go amd64 的分离乘加序列。
    GoAmd64Sse2V1 = 1,
    /// Reproduces the frozen Go amd64 sequence with its explicit fused operations.
    /// 复现冻结 Go amd64 中明确采用单次舍入的融合运算序列。
    GoAmd64FmaV1 = 2,
}

impl From<WeibullMathProfile> for u32 {
    /// Returns the closed wire discriminant without architecture-dependent inference.
    /// 返回封闭协议判别值，不进行架构相关推断。
    fn from(value: WeibullMathProfile) -> Self {
        value as Self
    }
}

impl TryFrom<u32> for WeibullMathProfile {
    type Error = &'static str;
    /// Rejects missing or unknown arithmetic profiles rather than choosing the local processor's behavior.
    /// 拒绝缺失或未知算术配置，不擅自选择本机处理器行为。
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::GoAmd64Sse2V1),
            2 => Ok(Self::GoAmd64FmaV1),
            _ => Err("unsupported Weibull arithmetic profile"),
        }
    }
}

/// Carries Weibull decay strategy parameters in binary64 form.
/// 以双精度形式携带 Weibull 衰减策略参数。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeibullDecayPolicy {
    /// Binds the exact versioned floating-point evaluation sequence to the immutable snapshot.
    /// 将精确版本化浮点求值序列绑定到不可变快照。
    pub math_profile: WeibullMathProfile,
    /// States whether read-time decay is enabled.
    /// 表示是否启用读时衰减。
    pub enabled: bool,
    /// Carries the distribution shape as binary64 values.
    /// 携带双精度分布形状。
    pub shape: f64,
    /// Carries the distribution scale in hours.
    /// 携带以小时表示的分布尺度。
    pub scale_hours: f64,
    /// Carries the minimum multiplier floor as binary64 values.
    /// 携带双精度最小倍率下限。
    pub min_multiplier: f64,
    /// Carries the reinforcement weight as binary64 values.
    /// 携带双精度强化权重。
    pub reinforce_weight: f64,
    /// Carries the cross-session adoption boost as binary64 values.
    /// 携带双精度跨会话采纳加成。
    pub cross_session_boost: f64,
    /// Carries decay scale multipliers by memory level discriminant as binary64 values.
    /// 携带按记忆等级判别值双精度衰减尺度倍率。
    pub level_scales: Vec<f64>,
    /// Carries decay scale multipliers by priority band as binary64 values.
    /// 携带按优先级档位双精度衰减尺度倍率。
    pub priority_scales: Vec<f64>,
    /// Carries decay scale multipliers by generic scope class as binary64 values.
    /// 携带按通用作用域分类双精度衰减尺度倍率。
    pub scope_scales: Vec<f64>,
    /// Carries the refresh-weight contribution as binary64 values.
    /// 携带双精度刷新权重贡献。
    pub refresh_weight_factor: f64,
}

/// Carries context-evidence scoring weights in binary64 form.
/// 以双精度形式携带情境证据评分权重。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextEvidencePolicy {
    /// States whether context evidence participates in ranking.
    /// 表示情境证据是否参与排序。
    pub enabled: bool,
    /// Carries the per-support increment as binary64 values.
    /// 携带双精度每次支持增量。
    pub support_step: f64,
    /// Carries the support cap as binary64 values.
    /// 携带双精度支持上限。
    pub support_cap: f64,
    /// Carries the per-rebuttal penalty as binary64 values.
    /// 携带双精度每次反驳惩罚。
    pub rebuttal_step: f64,
    /// Carries the rebuttal cap as binary64 values.
    /// 携带双精度反驳上限。
    pub rebuttal_cap: f64,
}

/// Carries MMR diversity parameters in binary64 form.
/// 以双精度形式携带 MMR 多样性参数。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MmrPolicy {
    /// States whether MMR diversity selection runs.
    /// 表示是否执行 MMR 多样性选择。
    pub enabled: bool,
    /// Carries the relevance weight lambda as binary64 values.
    /// 携带双精度相关性权重 lambda。
    pub lambda: f64,
}

/// Selects the explicit fusion score and tie-break semantics independently of any physical producer.
/// 独立于物理生产者选择明确的融合计分及平局语义，不通过数据库名称推断行为。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum RrfScoreMode {
    /// Preserves rank-normalized decision floors and the configured dual-channel bonus.
    /// 保留排名归一化的决策下限与配置的双通道增益。
    RankNormalized = 1,
    /// Projects only the fused rank, with identity-only RRF ties and no channel floors or bonus.
    /// 仅投影融合后排名，RRF 平局仅按身份排序，不使用通道分下限或加分。
    FusedRankOnly = 2,
}

/// Freezes denominator arithmetic independently of score projection and the processor restoring the command.
/// 独立于计分投影及恢复命令的处理器，冻结分母算术顺序。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum RrfMathProfile {
    /// Adds the one-based rank with signed-64 wrapping before binary64 conversion, preserving the observed source integer behavior.
    /// 将一基名次按有符号64位回绕相加后再转双精度，保留已观察到的来源整型行为。
    Signed64WrappingThenFloat64V1 = 1,
    /// Converts the parameter and one-based rank separately to binary64 before addition.
    /// 将参数和一基名次分别转成双精度后再相加。
    Float64AddV1 = 2,
}

/// Defines one complete split-read fallback after an indivisible candidate observation fails.
/// 定义一次不可分割候选观察失败后的完整分离读取恢复；类型本身固定为分离式排名归一化融合。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeparateChannelFallbackPolicy {
    /// Names the vector producer used only after the observation failure.
    /// 命名仅在组合观察失败后使用的向量生产者。
    pub vector_contract: ScoreContractDescriptor,
    /// Names the lexical producer and its host-projected rank score used by split fusion.
    /// 命名分离融合使用的词法生产者及其宿主投影排名分。
    pub lexical_contract: ScoreContractDescriptor,
    /// Freezes split-source denominator arithmetic independently of the failed observation.
    /// 独立于失败的组合观察冻结分离来源分母算术。
    pub rrf_math_profile: RrfMathProfile,
    /// Carries the split-source shared-channel bonus.
    /// 携带分离来源的双通道加成。
    pub dual_channel_bonus: f64,
}

/// Carries the complete host-agnostic retrieval strategy evaluated by Core.
/// 携带由 Core 评估的完整宿主无关检索策略。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrievalPolicy {
    /// Identifies the closed policy schema understood by this Core version.
    /// 标识当前 Core 版本理解的封闭策略 Schema。
    pub schema_version: u32,
    /// Selects separate reads or one complete physical observation independently of fusion math.
    /// 独立于融合数学选择分离读取或一次完整物理观察。
    pub candidate_read_mode: crate::CandidateReadMode,
    /// States whether vector and lexical channels are fused.
    /// 表示是否融合向量与词法通道。
    pub hybrid_enabled: bool,
    /// Carries the exact positive source fusion constant, bounded by the signed-64 source configuration domain.
    /// 携带精确的来源正融合常数，上界为来源配置的有符号64位值域。
    pub rrf_k: u64,
    /// Binds denominator arithmetic without deriving it from storage or score projection.
    /// 绑定分母算术，不从存储或计分投影推断。
    pub rrf_math_profile: RrfMathProfile,
    /// Binds fusion behavior explicitly; disabling hybrid retrieval bypasses either fusion mode.
    /// 显式绑定融合行为；关闭混合检索时跳过两种融合模式。
    pub rrf_score_mode: RrfScoreMode,
    /// Carries the shared-signal bonus scaled as binary64 values applied to dual-channel hits.
    /// 携带以双精度表示并同时命中双通道的加成。
    pub dual_channel_bonus: f64,
    /// Carries the minimum final decision score before a candidate may survive.
    /// 携带候选得以保留前的最小最终决策分值。
    pub min_similarity: Vec<ThresholdBinding>,
    /// Carries the minimum decision score required before a candidate may replace an existing memory.
    /// 携带候选可替代既有记忆前要求的最小决策分值。
    pub replace_min_similarity: Vec<ThresholdBinding>,
    /// Carries the exact cosine similarity treated as hard duplication.
    /// 携带被视为硬重复的精确余弦相似度。
    pub hard_dedupe: Vec<ThresholdBinding>,
    /// Carries the hard-deduplication pre-review window size.
    /// 携带硬去重预审窗口大小。
    pub hard_dedupe_pool_top_k: u32,
    /// Carries the default public result size.
    /// 携带默认公共结果大小。
    pub top_k_default: u32,
    /// Carries the maximum public result size.
    /// 携带最大公共结果大小。
    pub top_k_max: u32,
    /// Carries the maximum number of grouped queries.
    /// 携带分组查询的最大数量。
    pub max_query_items: u32,
    /// Carries the candidate pool ceiling.
    /// 携带候选池上限。
    pub candidate_pool_max: u32,
    /// Expands the shared physical pool when hybrid retrieval is enabled; both channels use it.
    /// 混合检索启用时扩容共同物理候选池，两个通道共用，而非词法单独截断。
    pub lexical_candidate_floor: u32,
    /// Limits reviewer-visible similar memories for both direct writes and pending-turn analysis.
    /// 限制主动写入和待处理轮次分析共同的审核可见相似记忆数，独立于硬去重窗口。
    pub replacement_top_k: u32,
    /// Names the exact versioned score contract the vector channel must serve.
    /// 命名向量通道必须服务的精确版本化评分契约。
    pub vector_contract: ScoreContractDescriptor,
    /// Names the exact versioned score contract the lexical channel must serve.
    /// 命名词法通道必须服务的精确版本化评分契约。
    pub lexical_contract: ScoreContractDescriptor,
    /// Enables one explicit split-read fallback only for a failed complete observation.
    /// 仅为失败的完整观察启用一条显式分离读取恢复；缺省表示该失败仍向上返回。
    pub separate_channel_fallback: Option<SeparateChannelFallbackPolicy>,
    /// Carries Weibull decay strategy parameters.
    /// 携带 Weibull 衰减策略参数。
    pub decay: WeibullDecayPolicy,
    /// Carries context-evidence scoring weights.
    /// 携带情境证据评分权重。
    pub context: ContextEvidencePolicy,
    /// Carries MMR diversity parameters.
    /// 携带 MMR 多样性参数。
    pub mmr: MmrPolicy,
    /// Declares the rerank candidate window; zero disables the optional stage.
    /// 声明重排候选窗口；零表示禁用该可选阶段。
    pub rerank_top_n: u32,
}

impl RetrievalPolicy {
    /// Returns the frozen first-release v1 retrieval policy matching the authoritative VMM baseline.
    /// 返回与权威 VMM 基线一致的首次发布 v1 冻结检索策略。
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn v1() -> Self {
        Self {
            schema_version: 8,
            candidate_read_mode: crate::CandidateReadMode::SeparateChannels,
            hybrid_enabled: true,
            rrf_k: 60,
            rrf_math_profile: RrfMathProfile::Signed64WrappingThenFloat64V1,
            rrf_score_mode: RrfScoreMode::RankNormalized,
            dual_channel_bonus: 0.03,
            min_similarity: Vec::new(),
            replace_min_similarity: vec![ThresholdBinding {
                contract: ScoreContractRef {
                    id: "reference-cosine".to_owned(),
                    version: 1,
                },
                threshold: 0.8,
            }],
            hard_dedupe: vec![ThresholdBinding {
                contract: ScoreContractRef {
                    id: "reference-cosine".to_owned(),
                    version: 1,
                },
                threshold: 0.99,
            }],
            hard_dedupe_pool_top_k: 16,
            vector_contract: ScoreContractDescriptor {
                decision_score_mode: DecisionScoreMode::HostProjected,
                contract: ScoreContractRef {
                    id: "reference-cosine".to_owned(),
                    version: 1,
                },
                raw_score_kind: RawScoreKind::CosineSimilarity,
                direction: ScoreDirection::HigherIsBetter,
            },
            lexical_contract: ScoreContractDescriptor {
                decision_score_mode: DecisionScoreMode::HostProjected,
                contract: ScoreContractRef {
                    id: "reference-lexical".to_owned(),
                    version: 1,
                },
                raw_score_kind: RawScoreKind::TokenOverlap,
                direction: ScoreDirection::HigherIsBetter,
            },
            separate_channel_fallback: None,
            top_k_default: 8,
            top_k_max: 32,
            max_query_items: 16,
            candidate_pool_max: 32,
            lexical_candidate_floor: 8,
            replacement_top_k: 5,
            decay: WeibullDecayPolicy {
                math_profile: WeibullMathProfile::GoAmd64FmaV1,
                enabled: true,
                shape: 1.35,
                scale_hours: 2160.0,
                min_multiplier: 0.4,
                reinforce_weight: 0.18,
                cross_session_boost: 0.12,
                level_scales: vec![0.8, 1.0, 1.5, 2.4],
                priority_scales: vec![1.35, 1.15, 1.0],
                scope_scales: vec![0.9, 1.1, 1.25],
                refresh_weight_factor: 0.08,
            },
            context: ContextEvidencePolicy {
                enabled: true,
                support_step: 0.025,
                support_cap: 0.12,
                rebuttal_step: 0.03,
                rebuttal_cap: 0.14,
            },
            mmr: MmrPolicy {
                enabled: true,
                lambda: 0.75,
            },
            rerank_top_n: 32,
        }
    }
}

/// Preserves the producer identity and exact RRF input for one present physical channel.
/// 为一个实际存在的物理通道保留生产者身份及精确 RRF 输入，供最终结果审计。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicalChannelExplanation {
    /// Identifies the producer version, raw score kind, and optimization direction.
    /// 标识生产者版本、原始分值类别及优化方向。
    pub descriptor: ScoreContractDescriptor,
    /// Retains the original zero-based physical rank, not the final result position.
    /// 保留原始从零开始的物理排名，而不是最终结果位置。
    pub physical_rank: u32,
    /// Retains the zero-based rank actually consumed by RRF; absent when fusion did not run.
    /// 保留 RRF 实际使用的零基名次；未执行融合时为空，与物理来源排名独立。
    pub rrf_rank: Option<u32>,
    /// Retains the raw reciprocal contribution, including signed-source overflow; zero means fusion was bypassed.
    /// 保留含来源有符号溢出的原始倒数贡献，可为负数而非业务决策分；跳过融合时为零。
    pub rrf_contribution: f64,
}

/// Carries the complete per-stage score explanation of one final retrieval decision.
/// 携带一个最终检索决策的完整分阶段评分解释。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScoreExplanation {
    /// Preserves vector-channel provenance only when this candidate was returned by that channel.
    /// 仅当向量通道返回该候选时保留其来源事实。
    pub vector_channel: Option<PhysicalChannelExplanation>,
    /// Preserves lexical-channel provenance only when this candidate was returned by that channel.
    /// 仅当词法通道返回该候选时保留其来源事实。
    pub lexical_channel: Option<PhysicalChannelExplanation>,
    /// Carries the raw vector score when the vector channel produced this candidate.
    /// 当向量通道产生该候选时携带原始向量分值。
    pub raw_vector_score: Option<f64>,
    /// Carries the raw lexical score when the lexical channel produced this candidate.
    /// 当词法通道产生该候选时携带原始词法分值。
    pub raw_lexical_score: Option<f64>,
    /// Carries the vector channel decision score.
    /// 携带向量通道决策分值。
    pub vector_decision_score: Option<f64>,
    /// Carries the lexical channel decision score.
    /// 携带词法通道决策分值。
    pub lexical_decision_score: Option<f64>,
    /// Carries the reciprocal-rank fusion score.
    /// 携带倒数排名融合分值。
    pub rrf_score: Option<f64>,
    /// Carries the base decision score before decay and boosts.
    /// 携带衰减与加成前的基础决策分值。
    pub base_decision_score: Option<f64>,
    /// Carries the Weibull decay multiplier in the inclusive range zero to one.
    /// 携带零到一闭区间内的 Weibull 衰减倍率。
    pub decay_multiplier: Option<f64>,
    /// Carries the context-evidence delta applied to the base score.
    /// 携带应用于基础分值的情境证据增量。
    pub context_delta: Option<f64>,
    /// Carries the provider rerank score when reranking ran.
    /// 当执行重排时携带 Provider 重排分值。
    pub rerank_score: Option<f64>,
    /// States whether MMR selected this candidate.
    /// 表示 MMR 是否选择了该候选。
    pub mmr_selected: Option<bool>,
    /// Carries the deterministic matched context labels in sorted order.
    /// 按排序顺序携带确定性命中的情境标签。
    pub context_matched_values: Vec<String>,
    /// Counts support observations from matched context rows.
    /// 统计命中情境行中的支持观测。
    pub context_matched_support_count: u32,
    /// Counts rebuttal observations from matched context rows.
    /// 统计命中情境行中的反驳观测。
    pub context_matched_rebuttal_count: u32,
    /// Counts support observations across every loaded context row for this memory.
    /// 统计该记忆全部已加载情境行中的支持观测。
    pub context_total_support_count: u32,
    /// Counts rebuttal observations across every loaded context row for this memory.
    /// 统计该记忆全部已加载情境行中的反驳观测。
    pub context_total_rebuttal_count: u32,
    /// Preserves the retrieval score before the optional reviewer-only rank floor projection.
    /// 保留可选审核专用排名下限投影之前的检索分值。
    pub pre_review_score: Option<f64>,
    /// Carries the final published score.
    /// 携带最终发布分值。
    pub final_score: f64,
}
