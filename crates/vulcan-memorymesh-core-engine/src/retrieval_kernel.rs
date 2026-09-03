//! Pure deterministic retrieval scoring kernel executed exclusively by Core.
//! 完全由 Core 执行的纯确定性检索评分内核。

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use vulcan_memorymesh_core_contracts::{
    ContextEvidencePolicy, CoreError, DecisionScoreMode, MemoryContextEvidence, MemoryLevel,
    MemoryRecord, MemoryScopeClass, MmrPolicy, PhysicalCandidate, PhysicalChannelExplanation,
    RetrievalPolicy, RrfMathProfile, RrfScoreMode, ScoreContractDescriptor, ScoreContractRef,
    ScoreExplanation, SearchHit, ThresholdBinding, WeibullDecayPolicy,
};

/// Matches the same Han and ASCII lexical surfaces used by the frozen VMM tokenizer.
/// 匹配冻结 VMM 分词器使用的相同汉字与 ASCII 词法表面。
static CONTEXT_TOKEN_PATTERN: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"[\p{Han}]+|[a-zA-Z0-9_\-\.]+").ok());

/// Carries the complete deterministic context-scoring observation for one memory.
/// 携带一条记忆的完整确定性情境评分观测。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextScore {
    /// Carries the bounded score delta from matched rows.
    /// 携带由命中行产生的有界评分增量。
    pub delta: f64,
    /// Carries sorted and deduplicated rendered `key=value` labels for matched rows.
    /// 携带已排序去重的命中行 `key=value` 渲染标签。
    pub matched_values: Vec<String>,
    /// Counts support observations from matched rows.
    /// 统计命中行中的支持观测。
    pub matched_support_count: u32,
    /// Counts rebuttal observations from matched rows.
    /// 统计命中行中的反驳观测。
    pub matched_rebuttal_count: u32,
    /// Counts support observations from every loaded row.
    /// 统计全部已加载行中的支持观测。
    pub total_support_count: u32,
    /// Counts rebuttal observations from every loaded row.
    /// 统计全部已加载行中的反驳观测。
    pub total_rebuttal_count: u32,
}

/// Resolves the shared physical window from enabled stages and the final/replacement result limit.
/// 根据已启用阶段及最终/更替结果数量计算共同物理窗口；仅更替检索纳入硬去重下限。
///
/// The policy must be validated; the returned count is bounded by its candidate ceiling.
/// 策略必须已校验；返回数量受该策略的候选上限约束。
#[must_use]
pub fn candidate_pool_limit(final_limit: u32, hard_dedupe: bool, policy: &RetrievalPolicy) -> u32 {
    let final_limit = if final_limit == 0 {
        policy.top_k_default
    } else {
        final_limit
    };
    let mut pool = final_limit;
    if policy.hybrid_enabled {
        pool = pool.max(policy.lexical_candidate_floor);
    }
    pool = pool.max(policy.rerank_top_n);
    if policy.mmr.enabled {
        pool = pool.max(final_limit.saturating_mul(2));
    }
    if hard_dedupe {
        pool = pool.max(policy.hard_dedupe_pool_top_k);
    }
    pool.min(policy.candidate_pool_max)
}

/// Fuses physical channels inside this query's shared window using the explicitly selected score mode.
/// 在本次查询共同窗口内按明确选择的计分模式融合物理通道，不按生产者身份猜测算法。
///
/// Inputs are validated materialized channels, policy and pre-materialization lexical presence; at most `pool_limit` rows survive.
/// 输入为已校验物化通道、策略及物化前词法存在性；返回至多 `pool_limit` 条候选及评分解释。
///
/// # Errors
/// Returns a contract error when a score-consuming mode receives an absent or invalid host projection.
/// 当消费决策分的模式收到缺失或非法宿主投影时返回契约错误。
#[allow(clippy::too_many_lines)]
#[allow(
    clippy::cast_precision_loss,
    // Exact source equality intentionally treats signed zeros alike; an epsilon would change identity tie-breaking.
    // 来源精确相等有意将正负零视为相同；引入误差范围会改变身份平局规则。
    clippy::float_cmp,
    clippy::single_match_else,
    clippy::type_complexity
)]
pub fn rrf_fusion(
    vector: &[PhysicalCandidate],
    lexical: &[PhysicalCandidate],
    policy: &RetrievalPolicy,
    pool_limit: u32,
    lexical_was_nonempty: bool,
) -> Result<Vec<(String, ScoreExplanation)>, CoreError> {
    /// Retains one candidate's physical facts while RRF establishes order.
    /// 在 RRF 建立顺序期间保留一条候选的物理事实。
    #[derive(Default)]
    struct FusedCandidate {
        rrf_score: f64,
        vector_channel: Option<PhysicalChannelExplanation>,
        lexical_channel: Option<PhysicalChannelExplanation>,
        raw_vector_score: Option<f64>,
        raw_lexical_score: Option<f64>,
        vector_decision_score: Option<f64>,
        lexical_decision_score: Option<f64>,
        base_decision_score: f64,
        channel_count: u32,
    }

    // Reject unrepresentable source parameters before arithmetic; never recover a lost integer by clamping.
    // 运算前拒绝来源无法表示的参数，不以夹紧值弥补丢失的整数。
    let rrf_k = i64::try_from(policy.rrf_k)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            CoreError::new(
                vulcan_memorymesh_core_contracts::ErrorCode::InvalidConfiguration,
                vulcan_memorymesh_core_contracts::ErrorStage::Effect,
                "RRF constant must be a positive signed-64 source integer",
            )
        })?;

    // Source materialization sorts by decision score then domain identity before split fusion or pure-vector truncation.
    // 来源物化在分离式融合或纯向量截断之前按决策分及领域身份排序；不能重写已校验的物理排名。
    let ordered_vector = order_vector_candidates(
        vector,
        !policy.hybrid_enabled || policy.rrf_score_mode == RrfScoreMode::RankNormalized,
    )?;
    let vector = &ordered_vector;

    // Source bypass uses the storage result, not the later hot guard; complete late retirement still runs fusion.
    // 来源旁路依据存储返回而非后续热路径过滤；原结果非空但后续全部失效时仍执行融合。
    if !policy.hybrid_enabled
        || (!lexical_was_nonempty && policy.rrf_score_mode == RrfScoreMode::RankNormalized)
    {
        return vector
            .iter()
            .take(pool_limit.min(policy.candidate_pool_max) as usize)
            .map(|candidate| {
                let decision = candidate.score.require_decision_score()?;
                let channel = channel_explanation(
                    candidate,
                    policy.vector_contract.decision_score_mode,
                    None,
                    rrf_k,
                    policy.rrf_math_profile,
                );
                Ok((
                    candidate.memory_id.clone(),
                    ScoreExplanation {
                        vector_channel: Some(channel),
                        raw_vector_score: Some(candidate.score.raw_value),
                        vector_decision_score: Some(decision),
                        base_decision_score: Some(decision),
                        final_score: decision,
                        ..ScoreExplanation::default()
                    },
                ))
            })
            .collect();
    }
    // Accumulate RRF independently from the caller-facing zero-to-one decision score.
    // 将 RRF 累计值与面向调用方的零到一决策分严格分离。
    let mut entries = BTreeMap::<String, FusedCandidate>::new();
    for (candidate, rank) in vector.iter().zip(0_u32..) {
        let entry = entries.entry(candidate.memory_id.clone()).or_default();
        let fusion_rank = match policy.rrf_score_mode {
            RrfScoreMode::RankNormalized => rank,
            RrfScoreMode::FusedRankOnly => candidate.physical_rank,
        };
        let channel = channel_explanation(
            candidate,
            policy.vector_contract.decision_score_mode,
            Some(fusion_rank),
            rrf_k,
            policy.rrf_math_profile,
        );
        entry.rrf_score += channel.rrf_contribution;
        entry.vector_channel = Some(channel);
        entry.raw_vector_score = Some(candidate.score.raw_value);
        entry.vector_decision_score = candidate.score.decision_score;
        if policy.rrf_score_mode == RrfScoreMode::RankNormalized {
            entry.base_decision_score = entry
                .base_decision_score
                .max(candidate.score.require_decision_score()?)
                .max(normalized_rank_score(rank as usize + 1, vector.len()));
        }
        entry.channel_count = entry.channel_count.saturating_add(1);
    }
    for (candidate, rank) in lexical.iter().zip(0_u32..) {
        let entry = entries.entry(candidate.memory_id.clone()).or_default();
        let fusion_rank = match policy.rrf_score_mode {
            RrfScoreMode::RankNormalized => rank,
            RrfScoreMode::FusedRankOnly => candidate.physical_rank,
        };
        let channel = channel_explanation(
            candidate,
            policy.lexical_contract.decision_score_mode,
            Some(fusion_rank),
            rrf_k,
            policy.rrf_math_profile,
        );
        entry.rrf_score += channel.rrf_contribution;
        entry.lexical_channel = Some(channel);
        entry.raw_lexical_score = Some(candidate.score.raw_value);
        entry.lexical_decision_score = candidate.score.decision_score;
        if policy.rrf_score_mode == RrfScoreMode::RankNormalized {
            entry.base_decision_score = entry
                .base_decision_score
                .max(candidate.score.require_decision_score()?)
                .max(normalized_rank_score(rank as usize + 1, lexical.len()));
        }
        entry.channel_count = entry.channel_count.saturating_add(1);
    }

    let mut fused = entries.into_iter().collect::<Vec<_>>();
    fused.sort_by(|(left_id, left), (right_id, right)| {
        right
            .rrf_score
            .total_cmp(&left.rrf_score)
            .then_with(|| match policy.rrf_score_mode {
                RrfScoreMode::RankNormalized => right
                    .base_decision_score
                    .total_cmp(&left.base_decision_score),
                RrfScoreMode::FusedRankOnly => std::cmp::Ordering::Equal,
            })
            .then_with(|| left_id.cmp(right_id))
    });
    fused.truncate(pool_limit.min(policy.candidate_pool_max) as usize);
    let total = fused.len();
    Ok(fused
        .into_iter()
        .enumerate()
        .map(|(rank, (memory_id, candidate))| {
            // Fused-rank-only mode projects the final rank; physical scores cannot become hidden tie-breakers or floors.
            // 仅融合排名模式投影最终名次；物理分不能变成隐藏的平局键或评分下限。
            let base = match policy.rrf_score_mode {
                RrfScoreMode::RankNormalized => {
                    let value = candidate
                        .base_decision_score
                        .max(normalized_rank_score(rank + 1, total));
                    if candidate.channel_count > 1 {
                        (value + policy.dual_channel_bonus).min(1.0)
                    } else {
                        value
                    }
                }
                RrfScoreMode::FusedRankOnly => normalized_rank_score(rank + 1, total),
            };
            (
                memory_id,
                ScoreExplanation {
                    vector_channel: candidate.vector_channel,
                    lexical_channel: candidate.lexical_channel,
                    raw_vector_score: candidate.raw_vector_score,
                    raw_lexical_score: candidate.raw_lexical_score,
                    vector_decision_score: candidate.vector_decision_score,
                    lexical_decision_score: candidate.lexical_decision_score,
                    rrf_score: Some(candidate.rrf_score),
                    base_decision_score: Some(base),
                    final_score: base,
                    ..ScoreExplanation::default()
                },
            )
        })
        .collect())
}

/// Orders materialized vectors only when the selected business mode consumes host decisions.
/// 仅在业务模式消费宿主决策分时排序已物化向量，缺分显式报错而非伪造零分。
/// The input is a validated channel; the output retains references and never rewrites physical ranks.
/// 输入为已校验通道；返回原候选引用，不改写物理名次。
#[allow(clippy::float_cmp)]
fn order_vector_candidates(
    vector: &[PhysicalCandidate],
    require_score: bool,
) -> Result<Vec<&PhysicalCandidate>, CoreError> {
    if !require_score {
        return Ok(vector.iter().collect());
    }
    let mut scored = vector
        .iter()
        .map(|candidate| Ok((candidate, candidate.score.require_decision_score()?)))
        .collect::<Result<Vec<_>, CoreError>>()?;
    scored.sort_by(|(left, left_score), (right, right_score)| {
        if left_score == right_score {
            left.memory_id.cmp(&right.memory_id)
        } else {
            right_score.total_cmp(left_score)
        }
    });
    Ok(scored.into_iter().map(|(candidate, _)| candidate).collect())
}

/// Copies verified physical provenance before RRF discards the source channel arrays.
/// 在 RRF 丢弃来源通道数组前复制已校验的物理事实，保持最终结果可审计。
#[allow(clippy::cast_precision_loss)]
fn channel_explanation(
    candidate: &PhysicalCandidate,
    decision_score_mode: DecisionScoreMode,
    rrf_rank: Option<u32>,
    rrf_k: i64,
    math_profile: RrfMathProfile,
) -> PhysicalChannelExplanation {
    PhysicalChannelExplanation {
        descriptor: ScoreContractDescriptor {
            contract: candidate.score.contract.clone(),
            raw_score_kind: candidate.score.raw_score_kind,
            direction: candidate.score.direction,
            decision_score_mode,
        },
        physical_rank: candidate.physical_rank,
        rrf_rank,
        rrf_contribution: rrf_rank.map_or(0.0, |rank| rrf_contribution(rrf_k, rank, math_profile)),
    }
}

/// Computes one channel contribution from a validated positive source constant and zero-based rank using the frozen arithmetic profile.
/// 使用冻结算术配置，根据已校验的来源正常数及零基名次计算单通道贡献，返回未经业务分投影的双精度值。
#[allow(clippy::cast_precision_loss)]
fn rrf_contribution(rrf_k: i64, rank: u32, math_profile: RrfMathProfile) -> f64 {
    // Source integer overflow is observable in ordering; changing it requires a separate semantic migration, not a wider field cast.
    // 来源整数溢出会影响排序；改变它需要独立语义迁移，不能借扩大字段转换悄悄修正。
    let one_based_rank = i64::from(rank) + 1;
    let denominator = match math_profile {
        RrfMathProfile::Signed64WrappingThenFloat64V1 => rrf_k.wrapping_add(one_based_rank) as f64,
        RrfMathProfile::Float64AddV1 => rrf_k as f64 + one_based_rank as f64,
    };
    1.0 / denominator
}

/// Converts a one-based rank into the frozen VMM caller-facing zero-to-one score.
/// 将从一开始的排名转换为冻结 VMM 面向调用方的零到一分值。
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn normalized_rank_score(rank: usize, total: usize) -> f64 {
    if rank == 0 || total == 0 {
        return 0.0;
    }
    (1.0 - (rank.saturating_sub(1) as f64 / (total.saturating_mul(2) as f64))).clamp(0.0, 1.0)
}

/// Applies the authoritative Weibull survival decay to one memory.
/// 对一条记忆应用权威 Weibull 生存衰减。
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
#[allow(
    clippy::cast_lossless,
    clippy::imprecise_flops,
    clippy::suboptimal_flops
)]
pub fn weibull_multiplier(memory: &MemoryRecord, policy: &WeibullDecayPolicy, now_ns: i64) -> f64 {
    if !policy.enabled || memory.decay_disabled {
        return 1.0;
    }
    // Anchor the survival clock to the most recent meaningful lifecycle event.
    // 将生存时钟锚定到最近的有意义生命周期事件。
    let anchor = [
        Some(memory.created_at_ns),
        memory.last_reinforced_at_ns,
        memory.last_adopted_at_ns,
        memory.last_recalled_at_ns,
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(memory.created_at_ns);
    let age_hours = source_duration_hours(anchor, now_ns);
    if age_hours <= 0.0 {
        return 1.0;
    }
    let level_index = match memory.memory_level {
        MemoryLevel::Session => 0,
        MemoryLevel::Phase => 1,
        MemoryLevel::Stable => 2,
        MemoryLevel::Persistent => 3,
    };
    let level_scale = policy.level_scales.get(level_index).copied().unwrap_or(1.0);
    let priority_index = if memory.priority >= 80 {
        0
    } else if memory.priority >= 50 {
        1
    } else {
        2
    };
    let priority_scale = policy
        .priority_scales
        .get(priority_index)
        .copied()
        .unwrap_or(1.0);
    let scope_index = match memory.scope_class {
        MemoryScopeClass::Session => 0,
        MemoryScopeClass::Environment => 1,
        MemoryScopeClass::Subject => 2,
    };
    let scope_scale = policy.scope_scales.get(scope_index).copied().unwrap_or(1.0);
    // Preserve the source multiplication order and valid sub-hour scales; reassociation changes boundary decisions.
    // 保留原生乘法顺序及合法小数小时尺度，重排乘法或抬高下限会改变边界决策。
    let mut scale = policy.scale_hours;
    scale *= level_scale;
    scale *= priority_scale;
    scale *= scope_scale;
    // Cumulative values retain all integer bits until the same binary64 conversion used by the source formula.
    // 累计值在进入来源公式同样的双精度转换之前保留全部整数位，不按旧 u32 字段截断。
    scale *= 1.0 + policy.refresh_weight_factor * memory.refresh_weight as f64;
    scale *= 1.0
        + policy.reinforce_weight * crate::weibull_math::log1p(memory.reinforcement_count as f64);
    scale *= 1.0 + policy.cross_session_boost * memory.cross_session_adopted_count as f64;
    if scale <= 0.0 {
        return 1.0;
    }
    let survival = crate::weibull_math::exp(
        -crate::weibull_math::pow_nonnegative(age_hours / scale, policy.shape, policy.math_profile),
        policy.math_profile,
    );
    if !survival.is_finite() {
        return 1.0;
    }
    survival.clamp(policy.min_multiplier, 1.0)
}

/// Returns nonnegative source-equivalent age hours for exact nanosecond timestamps.
/// 根据精确纳秒时间戳返回与来源等价的非负年龄小时。
/// Go Time.Sub saturates a signed nanosecond Duration before Hours splits its quotient and remainder.
/// Go Time.Sub 先饱和为有符号纳秒时长，再由 Hours 拆分商与余数；不能在毫秒范围饱和。
#[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
fn source_duration_hours(anchor_ns: i64, now_ns: i64) -> f64 {
    // i128 covers the difference of any two signed nanosecond instants before applying Go's duration bound.
    // i128可无溢出地容纳任意两个有符号纳秒瞬间之差，再应用Go时长边界。
    const NANOS_PER_HOUR: i128 = 3_600_000_000_000;
    let elapsed_ns = (i128::from(now_ns) - i128::from(anchor_ns)).clamp(0, i128::from(i64::MAX));
    // Both bounded integers are exactly representable in binary64; preserve the source addition order.
    // 两个有界整数均可被双精度精确表示，保留来源相加顺序，不改成一次大整数除法。
    (elapsed_ns / NANOS_PER_HOUR) as f64
        + (elapsed_ns % NANOS_PER_HOUR) as f64 / 3_600_000_000_000.0
}

/// Calculates the complete bounded context-evidence observation over matching values.
/// 针对命中的情境值计算完整的有界情境证据观测。
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
pub fn context_score(
    evidence: &[MemoryContextEvidence],
    query: &str,
    policy: &ContextEvidencePolicy,
) -> ContextScore {
    let mut result = ContextScore {
        total_support_count: evidence
            .iter()
            .fold(0_u32, |total, row| total.saturating_add(row.support_count)),
        total_rebuttal_count: evidence
            .iter()
            .fold(0_u32, |total, row| total.saturating_add(row.rebuttal_count)),
        ..ContextScore::default()
    };
    if !policy.enabled {
        return result;
    }
    let normalized_query = normalize_context_value(query);
    let tokens = tokenize_context_query(query);
    let mut phrases = BTreeSet::new();
    if !normalized_query.is_empty() {
        phrases.insert(normalized_query.clone());
    }
    for token in &tokens {
        phrases.insert(normalize_context_value(token));
    }
    for window_size in 2..=4 {
        for window in tokens.windows(window_size) {
            phrases.insert(normalize_context_value(&window.join(" ")));
        }
    }
    let mut labels = BTreeSet::new();
    for row in evidence {
        // Only the normalized context value participates; keys are labels, never query signals.
        // 仅规范化情境值参与匹配；键是标签，绝不是查询信号。
        let value = normalize_context_value(&row.context_value);
        if value.is_empty()
            || (!phrases.contains(&value) && !contains_bounded_phrase(&normalized_query, &value))
        {
            continue;
        }
        result.matched_support_count = result
            .matched_support_count
            .saturating_add(row.support_count);
        result.matched_rebuttal_count = result
            .matched_rebuttal_count
            .saturating_add(row.rebuttal_count);
        let key = normalize_context_key(&row.context_key);
        labels.insert(if key.is_empty() {
            value
        } else {
            format!("{key}={value}")
        });
    }
    let support =
        (f64::from(result.matched_support_count) * policy.support_step).min(policy.support_cap);
    let rebuttal =
        (f64::from(result.matched_rebuttal_count) * policy.rebuttal_step).min(policy.rebuttal_cap);
    result.delta = support - rebuttal;
    result.matched_values = labels.into_iter().collect();
    result
}

/// Normalizes one context value onto the frozen VMM lexical surface.
/// 将一个情境值规范化到冻结 VMM 词法表面。
#[must_use]
pub fn normalize_context_value(value: &str) -> String {
    crate::algorithms::simple_lowercase(value.trim())
        .replace(['_', '-', '/', '\\', '.'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalizes one context key onto the frozen VMM label surface.
/// 将一个情境键规范化到冻结 VMM 标签表面。
#[must_use]
pub fn normalize_context_key(value: &str) -> String {
    normalize_context_value(value).replace(' ', "_")
}

/// Tokenizes one query with the frozen VMM Han and ASCII token contract.
/// 使用冻结 VMM 的汉字与 ASCII 词元契约对查询分词。
pub fn tokenize_context_query(query: &str) -> Vec<String> {
    let Some(pattern) = CONTEXT_TOKEN_PATTERN.as_ref() else {
        return Vec::new();
    };
    pattern
        .find_iter(&crate::algorithms::simple_lowercase(query))
        .map(|value| value.as_str().trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect()
}

/// Reports whether one normalized phrase appears with whitespace boundaries.
/// 报告一个规范化短语是否以空白边界出现。
fn contains_bounded_phrase(text: &str, phrase: &str) -> bool {
    text == phrase || format!(" {text} ").contains(&format!(" {phrase} "))
}

/// Checks whether one physical decision score clears a threshold bound to the same contract identity.
/// 检查一个物理决策分值是否通过绑定相同契约身份的阈值。
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn passes_threshold(
    score: f64,
    contract: &ScoreContractRef,
    bindings: &[ThresholdBinding],
) -> bool {
    for binding in bindings {
        if binding.contract == *contract {
            return score >= binding.threshold;
        }
    }
    // An unbound threshold never filters; only exact contract identities gate decisions.
    // 未绑定阈值从不过滤；仅精确契约身份门控决策。
    true
}

/// Reports the hard-deduplication threshold bound to one exact contract identity.
/// 报告绑定到一个精确契约身份的硬去重阈值。
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn hard_duplicate_similarity(
    bindings: &[ThresholdBinding],
    contract: &ScoreContractRef,
) -> Option<f64> {
    resolve_threshold(bindings, contract)
}

/// Resolves one similarity floor bound to an exact contract identity.
/// 解析绑定到一个精确契约身份的一个相似度下限。
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn resolve_threshold(
    bindings: &[ThresholdBinding],
    contract: &ScoreContractRef,
) -> Option<f64> {
    bindings
        .iter()
        .find_map(|binding| (binding.contract == *contract).then_some(binding.threshold))
}

/// Selects final candidates with maximal marginal relevance.
/// 使用最大边际相关性选择最终候选。
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::suboptimal_flops
)]
pub fn mmr_select(
    mut candidates: Vec<SearchHit>,
    limit: usize,
    policy: &MmrPolicy,
) -> Vec<SearchHit> {
    if !policy.enabled || candidates.len() <= 1 {
        candidates.truncate(limit);
        return candidates;
    }
    let lambda = policy.lambda;
    let mut selected: Vec<SearchHit> = Vec::with_capacity(limit);
    let mut remaining = candidates;
    while selected.len() < limit && !remaining.is_empty() {
        let mut best_index = 0usize;
        let mut best_value = f64::MIN;
        for (index, candidate) in remaining.iter().enumerate() {
            let redundancy = selected
                .iter()
                .map(|selected| cosine_between(candidate, selected))
                .fold(0.0_f64, f64::max);
            let value = lambda * candidate.final_score - (1.0 - lambda) * redundancy;
            let current = &remaining[best_index];
            if value > best_value
                || (value == best_value
                    && (candidate.final_score > current.final_score
                        || (candidate.final_score == current.final_score
                            && candidate.memory.id < current.memory.id)))
            {
                best_value = value;
                best_index = index;
            }
        }
        let mut selected_hit = remaining.remove(best_index);
        // Source MMR tracks chosen memory identities, so provider repetitions cannot consume another final slot.
        // 来源 MMR 跟踪已选记忆身份，提供方重复条目不能再占一个最终槽位；关闭 MMR 时仍保留原序列。
        remaining.retain(|candidate| candidate.memory.id != selected_hit.memory.id);
        if let Some(explanation) = selected_hit.explanation.as_mut() {
            explanation.mmr_selected = Some(true);
        }
        selected.push(selected_hit);
    }
    selected
}

/// Estimates embedding cosine similarity between two final hits.
/// 估计两个最终命中之间的嵌入余弦相似度。
#[allow(clippy::cast_precision_loss, clippy::single_match_else)]
fn cosine_between(left: &SearchHit, right: &SearchHit) -> f64 {
    match (&left.memory.embedding, &right.memory.embedding) {
        (Some(left_vector), Some(right_vector)) => cosine(left_vector, right_vector).unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Calculates cosine similarity for equal finite non-zero vectors.
/// 为相同长度、有限且非零的向量计算余弦相似度。
#[allow(clippy::suboptimal_flops)]
pub fn cosine(left: &[f32], right: &[f32]) -> Option<f64> {
    if left.len() != right.len()
        || left.is_empty()
        || left.iter().chain(right).any(|value| !value.is_finite())
    {
        return None;
    }
    // Promote source elements before ordinary products; fused multiply-add is not the frozen VMM algorithm.
    // 原始元素先升双精度再普通乘加；融合乘加不是冻结 VMM 算法，小非零向量也不能被 epsilon 丢弃。
    let (dot, left_norm, right_norm) = left.iter().zip(right).fold(
        (0.0_f64, 0.0_f64, 0.0_f64),
        |(dot, left_norm, right_norm), (left, right)| {
            let left = f64::from(*left);
            let right = f64::from(*right);
            (
                dot + left * right,
                left_norm + left * left,
                right_norm + right * right,
            )
        },
    );
    if left_norm == 0.0 || right_norm == 0.0 {
        return None;
    }
    Some(dot / (left_norm * right_norm).sqrt())
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_pool_limit, context_score, cosine, mmr_select, rrf_fusion, source_duration_hours,
        weibull_multiplier,
    };
    use vulcan_memorymesh_core_contracts::{
        MemoryCategory, MemoryContextEvidence, MemoryId, MemoryLevel, MemoryRecord,
        MemoryScopeClass, MemorySourceKind, MemoryStability, MemoryState, PhysicalCandidate,
        PhysicalCandidateScore, RawScoreKind, RetrievalPolicy, RrfScoreMode, ScopeId,
        ScoreContractRef, ScoreDirection, ScoreExplanation, SearchHit, SubjectId,
    };

    /// Builds one fully specified memory for deterministic retrieval-kernel tests.
    /// 为确定性检索内核测试构建一条字段完整的记忆。
    fn memory(identity: &str, score: f64) -> SearchHit {
        let memory = MemoryRecord {
            id: MemoryId::new(identity).unwrap_or_else(|_| std::process::abort()),
            access_class: vulcan_memorymesh_core_contracts::MemoryAccessClass::SubjectPrivate,
            source_subject_id: Some(
                SubjectId::new("subject-kernel").unwrap_or_else(|_| std::process::abort()),
            ),
            scope_id: ScopeId::new("scope-kernel").unwrap_or_else(|_| std::process::abort()),
            scope_class: MemoryScopeClass::Environment,
            category: MemoryCategory::General,
            summary: identity.to_owned(),
            content: identity.to_owned(),
            source_kind: MemorySourceKind::Direct,
            origin_session_source_id: None,
            priority: 50,
            stability: MemoryStability::Stable,
            tags: Vec::new(),
            evidence: Vec::new(),
            evidence_source: None,
            state: MemoryState::Active,
            importance: 0.5,
            confidence: Some(1.0),
            created_at_ns: 0,
            updated_at_ns: 0,
            expires_at_ns: None,
            last_recalled_at_ns: None,
            reinforcement_count: 0,
            memory_level: MemoryLevel::Stable,
            refresh_weight: 0,
            dedupe_hash: None,
            recalled_count: 0,
            adopted_count: 0,
            cross_session_adopted_count: 0,
            decay_disabled: false,
            status_reason: None,
            last_adopted_at_ns: None,
            last_reinforced_at_ns: None,
            source_turn_id: None,
            version: 1,
            vector_space_id: None,
            embedding: None,
        };
        SearchHit {
            memory,
            search_score: score,
            rerank_score: None,
            final_score: score,
            explanation: Some(ScoreExplanation {
                final_score: score,
                ..ScoreExplanation::default()
            }),
        }
    }

    /// Builds one physical candidate using the reference vector contract.
    /// 使用参考向量契约构建一条物理候选。
    fn candidate(identity: &str, rank: u32, decision_score: f64) -> PhysicalCandidate {
        PhysicalCandidate {
            memory_id: identity.to_owned(),
            score: PhysicalCandidateScore {
                raw_value: decision_score,
                raw_score_kind: RawScoreKind::CosineSimilarity,
                direction: ScoreDirection::HigherIsBetter,
                decision_score: Some(decision_score),
                contract: ScoreContractRef {
                    id: "reference-cosine".to_owned(),
                    version: 1,
                },
            },
            physical_rank: rank,
        }
    }

    /// Keeps hard-dedupe and MMR cosine finite and invariant to valid vector magnitudes.
    /// 保证硬去重与 MMR 余弦有限，并不随合法向量量级改变。
    #[test]
    fn cosine_preserves_finite_scale_and_rejects_invalid_vectors() {
        for magnitude in [1.0_f32, 1e-30, 1e30, f32::MAX] {
            let score = cosine(&[magnitude, magnitude], &[magnitude, magnitude]);
            assert!(score.is_some_and(|value| value.is_finite() && (0.999..=1.0).contains(&value)));
            assert!(
                cosine(&[magnitude], &[-magnitude])
                    .is_some_and(|value| (value + 1.0).abs() < f64::EPSILON)
            );
        }
        for values in [vec![f32::NAN], vec![f32::INFINITY], vec![0.0], Vec::new()] {
            assert!(cosine(&values, &values).is_none());
        }
        assert!(cosine(&[1.0], &[1.0, 1.0]).is_none());
    }

    /// Checks source-turn selection, evidence tie-breakers, and stable first-position retention.
    /// 检查来源轮次选择、证据平局规则及首个位置保留。
    #[test]
    fn precheck_display_chooses_the_richer_same_turn_without_losing_adoption()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut first = memory("first", 0.8);
        first.memory.source_turn_id = Some(vulcan_memorymesh_core_contracts::TurnId::new("turn")?);
        let mut richer = first.clone();
        richer.memory.id = MemoryId::new("richer")?;
        richer.memory.summary = "better summary".to_owned();
        richer.memory.content = "better detail".to_owned();
        richer
            .explanation
            .as_mut()
            .ok_or("missing explanation")?
            .context_delta = Some(-0.1);
        let middle = memory("middle", 0.7);
        // Keep the simulated L2 order, attaching distinct views without altering source records.
        // 保留模拟 L2 的选择顺序，附加独立视图，不修改来源记录。
        let accepted = vec![first, middle, richer]
            .into_iter()
            .map(|hit| {
                let view = vulcan_memorymesh_core_contracts::PreCheckReviewCandidate {
                    memory_id: hit.memory.id.clone(),
                    source_turn_id: hit.memory.source_turn_id.clone(),
                    created_at_ns: hit.memory.created_at_ns,
                    source_kind: hit.memory.source_kind,
                    scope_class: hit.memory.scope_class,
                    category: hit.memory.category,
                    summary: hit.memory.summary.clone(),
                    details: hit.memory.content.clone(),
                    score: hit.final_score,
                    explanation: hit.explanation.clone().ok_or("fixture explanation")?,
                };
                Ok(crate::precheck::ReviewCandidateState { source: hit, view })
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        let (items, text) = crate::precheck::assemble_context(&accepted);
        assert_eq!(accepted.len(), 3);
        assert_eq!(
            items
                .iter()
                .map(|hit| hit.memory_id.as_str())
                .collect::<Vec<_>>(),
            vec!["richer", "middle"]
        );
        assert_eq!(text, "- better summary\nbetter detail\n- middle");
        Ok(())
    }

    /// Verifies RRF controls order while the public base score remains in the VMM rank range.
    /// 验证 RRF 控制顺序，而公开基础分仍处于 VMM 排名分范围。
    #[test]
    fn rrf_order_and_public_base_score_are_separate() -> Result<(), Box<dyn std::error::Error>> {
        let policy = RetrievalPolicy::v1();
        let vector = vec![
            candidate("memory-a", 0, 0.95),
            candidate("memory-b", 1, 0.9),
        ];
        vulcan_memorymesh_core_contracts::validate_candidate_channel(&vector)?;
        let lexical = vec![PhysicalCandidate {
            score: PhysicalCandidateScore {
                raw_score_kind: RawScoreKind::TokenOverlap,
                contract: ScoreContractRef {
                    id: "reference-lexical".to_owned(),
                    version: 1,
                },
                ..candidate("memory-a", 0, 0.3).score
            },
            ..candidate("memory-a", 0, 0.3)
        }];
        let fused = rrf_fusion(&vector, &lexical, &policy, policy.candidate_pool_max, true)?;
        assert_eq!(fused[0].0, "memory-a");
        assert!(fused[0].1.rrf_score.is_some_and(|score| score < 0.1));
        assert_eq!(fused[0].1.base_decision_score, Some(1.0));
        assert_eq!(fused[1].1.base_decision_score, Some(0.9));
        let first = &fused[0].1;
        let vector_channel = first.vector_channel.as_ref().ok_or("vector provenance")?;
        let lexical_channel = first.lexical_channel.as_ref().ok_or("lexical provenance")?;
        assert_eq!(vector_channel.descriptor.contract, vector[0].score.contract);
        assert_eq!(vector_channel.physical_rank, vector[0].physical_rank);
        assert_eq!(
            lexical_channel.descriptor.contract,
            lexical[0].score.contract
        );
        assert_eq!(
            lexical_channel.descriptor.direction,
            lexical[0].score.direction
        );
        assert!(
            (vector_channel.rrf_contribution + lexical_channel.rrf_contribution
                - first.rrf_score.unwrap_or(0.0))
            .abs()
                < f64::EPSILON
        );
        assert!(fused[1].1.vector_channel.is_some());
        assert!(fused[1].1.lexical_channel.is_none());
        Ok(())
    }

    /// Freezes the source-derived common-window matrix, including disabled stages and the hard-dedupe-only expansion.
    /// 冻结源码推导的共同窗口矩阵，覆盖关闭阶段及仅更替检索的硬去重扩容。
    #[test]
    fn candidate_windows_follow_enabled_stage_maxima() {
        // Columns are final, hybrid, lexical floor, rerank, MMR, hard-dedupe, expected physical count.
        // 各列为最终数量、混合开关、词法下限、重排、多样性、硬去重开关、预期物理数量。
        let cases = [
            (2, false, 8, 0, false, false, 2),
            (2, false, 8, 0, false, true, 16),
            (2, true, 8, 0, false, false, 8),
            (2, true, 8, 0, false, true, 16),
            (2, false, 8, 11, false, false, 11),
            (2, true, 8, 11, false, false, 11),
            (2, true, 8, 11, true, true, 16),
            (7, false, 8, 0, true, false, 14),
            (7, true, 8, 11, true, false, 14),
            (7, true, 19, 11, true, false, 19),
            (7, true, 19, 11, true, true, 19),
            (17, true, 8, 11, true, false, 32),
            (32, false, 8, 0, false, false, 32),
            (0, false, 8, 0, false, false, 8),
        ];
        for (limit, hybrid, lexical, rerank, mmr, hard, expected) in cases {
            let mut policy = RetrievalPolicy::v1();
            policy.hybrid_enabled = hybrid;
            policy.lexical_candidate_floor = lexical;
            policy.rerank_top_n = rerank;
            policy.mmr.enabled = mmr;
            assert_eq!(candidate_pool_limit(limit, hard, &policy), expected);
        }
    }

    /// Checks fusion trims to the query window before deriving its normalized rank score.
    /// 检查融合在计算归一排名分之前按本次查询窗口截断，而非按全局上限截断。
    #[test]
    fn fusion_uses_the_query_window_not_the_global_ceiling()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = RetrievalPolicy::v1();
        let vector = vec![candidate("a", 0, 0.2), candidate("b", 1, 0.1)];
        let lexical = vec![candidate("a", 0, 0.2), candidate("c", 1, 0.1)];
        let fused = rrf_fusion(&vector, &lexical, &policy, 2, true)?;
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].0, "a");
        assert_eq!(fused[1].0, "b");
        assert_eq!(fused[1].1.final_score.to_bits(), 0.75_f64.to_bits());
        assert!(super::passes_threshold(
            fused[0].1.final_score,
            &policy.vector_contract.contract,
            &policy.replace_min_similarity,
        ));
        Ok(())
    }

    /// Preserves vector ordering and threshold-relevant values when no lexical evidence exists.
    /// 词法证据缺席时保留向量顺序及影响阈值的原始决策数值。
    #[test]
    fn absent_lexical_channel_never_inflates_vector_decisions()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut policy = RetrievalPolicy::v1();
        let vector = vec![candidate("z", 0, 0.79), candidate("a", 1, 0.3)];
        for enabled in [true, false] {
            policy.hybrid_enabled = enabled;
            let fused = rrf_fusion(&vector, &[], &policy, 2, false)?;
            assert_eq!(fused[0].0, "z");
            assert_eq!(fused[0].1.final_score.to_bits(), 0.79_f64.to_bits());
            assert_eq!(fused[1].1.final_score.to_bits(), 0.3_f64.to_bits());
            assert!(fused.iter().all(|(_, score)| score.rrf_score.is_none()));
            assert!(!super::passes_threshold(
                fused[0].1.final_score,
                &policy.vector_contract.contract,
                &policy.replace_min_similarity
            ));
        }
        Ok(())
    }

    /// Keeps successful lexical presence distinct from later complete materialization loss.
    /// 将成功词法存在性与后续物化全部失效区分；输入相同存活向量，返回精确分值断言。
    #[test]
    fn lexical_retirement_does_not_erase_successful_channel()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = RetrievalPolicy::v1();
        let vector = vec![candidate("002", 0, 0.5), candidate("010", 1, 0.4)];
        let retired = rrf_fusion(&vector, &[], &policy, 2, true)?;
        let absent = rrf_fusion(&vector, &[], &policy, 2, false)?;
        for (index, expected) in [1.0_f64, 0.75].into_iter().enumerate() {
            assert_eq!(retired[index].1.final_score.to_bits(), expected.to_bits());
            assert!(retired[index].1.rrf_score.is_some());
            assert!(retired[index].1.lexical_channel.is_none());
            assert_eq!(
                absent[index].1.final_score.to_bits(),
                vector[index].score.require_decision_score()?.to_bits()
            );
            assert!(absent[index].1.rrf_score.is_none());
        }
        Ok(())
    }

    /// Reproduces source materialization ordering while retaining sparse physical evidence.
    /// 复现来源物化排序并保留稀疏物理证据；输入固定候选，返回断言结果。
    #[test]
    fn split_fusion_sorts_tied_vectors_and_compresses_only_business_ranks()
    -> Result<(), Box<dyn std::error::Error>> {
        let policy = RetrievalPolicy::v1();
        let vector = vec![candidate("010", 4, 0.5), candidate("002", 6, 0.5)];
        let lexical = vec![candidate("030", 9, 1.0)];
        let before = vector.clone();
        let fused = rrf_fusion(&vector, &lexical, &policy, 3, true)?;
        assert_eq!(
            fused.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["002", "030", "010"]
        );
        assert_eq!(
            fused[0]
                .1
                .vector_channel
                .as_ref()
                .map(|channel| channel.physical_rank),
            Some(6)
        );
        assert_eq!(fused[0].1.rrf_score, Some(1.0 / 61.0));
        assert_eq!(fused[1].1.rrf_score, Some(1.0 / 61.0));
        assert_eq!(fused[2].1.rrf_score, Some(1.0 / 62.0));
        assert_eq!(vector, before);
        Ok(())
    }

    /// Requires pure-vector truncation to follow source score/identity order, including equal signed zeros.
    /// 要求纯向量截断遵循来源分数/身份顺序，包含数值相等的正负零；无返回值。
    #[test]
    fn pure_vector_sorts_before_truncation_and_treats_signed_zero_as_equal()
    -> Result<(), Box<dyn std::error::Error>> {
        for mode in [RrfScoreMode::RankNormalized, RrfScoreMode::FusedRankOnly] {
            for enabled in [false, true] {
                if enabled && mode == RrfScoreMode::FusedRankOnly {
                    continue;
                }
                let mut policy = RetrievalPolicy::v1();
                policy.rrf_score_mode = mode;
                policy.hybrid_enabled = enabled;
                for value in [0.5_f64, 0.0] {
                    let vector = vec![
                        candidate("010", 0, value),
                        candidate("002", 1, if value == 0.0 { -0.0 } else { value }),
                    ];
                    let fused = rrf_fusion(&vector, &[], &policy, 1, false)?;
                    assert_eq!(fused[0].0, "002");
                    assert_eq!(
                        fused[0].1.final_score.to_bits(),
                        vector[1].score.require_decision_score()?.to_bits()
                    );
                    assert!(fused[0].1.rrf_score.is_none());
                }
            }
        }
        Ok(())
    }

    /// Preserves fused-only rank projection and identity ties without applying split-channel score floors.
    /// 保留仅融合后排名投影及身份平局，不施加分离式通道评分下限。
    #[test]
    fn fused_rank_only_keeps_source_projection_and_ties() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut policy = RetrievalPolicy::v1();
        policy.rrf_score_mode = RrfScoreMode::FusedRankOnly;
        let vector = vec![candidate("030", 0, 0.99), candidate("010", 1, 0.98)];
        let lexical = vec![candidate("020", 0, 0.8), candidate("002", 1, 0.7)];
        let fused = rrf_fusion(&vector, &lexical, &policy, 4, true)?;
        assert_eq!(
            fused.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["020", "030", "002", "010"]
        );
        for ((_, explanation), expected) in fused.iter().zip([1.0_f64, 0.875, 0.75, 0.625]) {
            assert_eq!(explanation.final_score.to_bits(), expected.to_bits());
        }
        let truncated = rrf_fusion(&vector, &lexical, &policy, 2, true)?;
        assert_eq!(truncated[1].1.final_score.to_bits(), 0.75_f64.to_bits());
        policy.rrf_score_mode = RrfScoreMode::RankNormalized;
        let split = rrf_fusion(&vector, &lexical, &policy, 4, true)?;
        assert_eq!(split[2].0, "010");
        assert_eq!(split[2].1.final_score.to_bits(), 0.98_f64.to_bits());
        Ok(())
    }

    /// Distinguishes empty lexical results from disabled hybrid retrieval and projects only the fused output rank.
    /// 区分词法空结果与混合检索关闭，仅对实际融合输出排名进行投影。
    #[test]
    fn fused_rank_only_handles_single_and_empty_channels() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut policy = RetrievalPolicy::v1();
        policy.rrf_score_mode = RrfScoreMode::FusedRankOnly;
        let channel = vec![
            candidate("001", 0, 0.9),
            candidate("002", 1, 0.8),
            candidate("003", 2, 0.7),
        ];
        for (vector, lexical) in [
            (&channel[..], &[][..]),
            (&[][..], &channel[..]),
            (&channel[..], &channel[..]),
        ] {
            let fused = rrf_fusion(vector, lexical, &policy, 3, !lexical.is_empty())?;
            for ((_, explanation), expected) in
                fused
                    .iter()
                    .zip([1.0_f64, 0.833_333_333_333_333_4, 0.666_666_666_666_666_7])
            {
                assert_eq!(explanation.final_score.to_bits(), expected.to_bits());
                assert!(explanation.rrf_score.is_some());
            }
        }
        assert!(rrf_fusion(&[], &[], &policy, 3, false)?.is_empty());
        policy.hybrid_enabled = false;
        let vector_only = rrf_fusion(&channel, &[], &policy, 3, false)?;
        assert_eq!(vector_only[1].1.final_score.to_bits(), 0.8_f64.to_bits());
        assert_eq!(vector_only[1].1.rrf_score, None);
        Ok(())
    }

    /// Proves raw magnitude cannot become a decision floor while physical rank still changes the result.
    /// 证明原分大小不会成为决策下限，而物理排名变化仍改变结果；返回断言结果。
    #[test]
    fn absent_decisions_preserve_rank_authority() -> Result<(), Box<dyn std::error::Error>> {
        let mut policy = RetrievalPolicy::v1();
        policy.rrf_score_mode = RrfScoreMode::FusedRankOnly;
        policy.lexical_contract.decision_score_mode =
            vulcan_memorymesh_core_contracts::DecisionScoreMode::RankOnly;
        let vector = [candidate("010", 0, 0.99)];
        let mut lexical = [candidate("002", 0, 0.5)];
        lexical[0].score.decision_score = None;
        for raw in [0.0, -0.0, 0.5, 123.0, f64::MAX] {
            lexical[0].score.raw_value = raw;
            let result = rrf_fusion(&vector, &lexical, &policy, 2, true)?;
            assert_eq!(result[0].0, "002");
            assert_eq!(result[0].1.final_score.to_bits(), 1.0_f64.to_bits());
            assert_eq!(result[1].1.final_score.to_bits(), 0.75_f64.to_bits());
            assert_eq!(result[0].1.lexical_decision_score, None);
            assert_eq!(
                result[0]
                    .1
                    .raw_lexical_score
                    .ok_or("raw fact lost")?
                    .to_bits(),
                raw.to_bits()
            );
        }
        lexical[0].physical_rank = 4;
        let result = rrf_fusion(&vector, &lexical, &policy, 1, true)?;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "010");
        policy.rrf_score_mode = RrfScoreMode::RankNormalized;
        assert!(rrf_fusion(&vector, &lexical, &policy, 2, true).is_err());
        policy.hybrid_enabled = false;
        assert!(rrf_fusion(&lexical, &[], &policy, 1, false).is_err());
        Ok(())
    }

    /// Keeps sub-f32 score differences visible to contract-bound thresholds and MMR relevance.
    /// 保持小于单精度间隔的评分差异可见，使契约阈值与多样性相关分不会错误合并。
    #[test]
    fn binary64_scores_and_fractional_thresholds_drive_decisions() {
        let mut policy = RetrievalPolicy::v1();
        policy.replace_min_similarity[0].threshold = 0.8;
        let contract = &policy.vector_contract.contract;
        assert!(!super::passes_threshold(
            0.799_999_99,
            contract,
            &policy.replace_min_similarity
        ));
        assert!(super::passes_threshold(
            0.800_000_01,
            contract,
            &policy.replace_min_similarity
        ));
        policy.replace_min_similarity[0].threshold = 0.800_009;
        assert!(!super::passes_threshold(
            0.800_005,
            contract,
            &policy.replace_min_similarity
        ));
        assert!(super::passes_threshold(
            0.800_010,
            contract,
            &policy.replace_min_similarity
        ));
        let scores = vec![
            memory("a-lower", 0.800_000_001),
            memory("z-higher", 0.800_000_002),
        ];
        let selected = mmr_select(scores, 1, &policy.mmr);
        assert_eq!(selected[0].memory.id.as_str(), "z-higher");
    }

    /// Matches the literal multiplier captured from the native VMM quarter-hour regression case.
    /// 匹配从原生 VMM 四分之一小时回归用例采集的字面量倍率，不接受一小时下限或误差放行。
    #[test]
    fn fractional_hour_decay_matches_native_vmm_bits() {
        let mut hit = memory("quarter-hour", 1.0);
        hit.memory.created_at_ns = 0;
        hit.memory.memory_level = MemoryLevel::Phase;
        hit.memory.priority = 20;
        hit.memory.refresh_weight = 0;
        hit.memory.reinforcement_count = 0;
        hit.memory.cross_session_adopted_count = 0;
        let mut policy = RetrievalPolicy::v1().decay;
        policy.shape = 1.0;
        policy.scale_hours = 0.25;
        policy.min_multiplier = 0.0;
        policy.reinforce_weight = 0.0;
        policy.cross_session_boost = 0.0;
        let multiplier = weibull_multiplier(&hit.memory, &policy, 3_600_000_000_000);
        assert_eq!(multiplier.to_bits(), 0x3f9a_faf7_0fa7_9f13);
    }

    /// Matches source `time.Sub().Hours()` bits at the nanosecond saturation boundary and integer extremes.
    /// 在纳秒时长饱和边界及整数极值匹配来源 `time.Sub().Hours()` 位模式，未来锚点按业务规则返回零年龄。
    #[test]
    fn duration_hours_preserve_source_saturation() {
        for (anchor, now, bits) in [
            (1_000_000, 2_000_000, 0x3e92_a42f_961f_79b9),
            (1_000_000, 3_600_002_000_000, 0x3ff0_0000_4a90_be58),
            (0, 9_223_372_036_854_000_000, 0x4143_8bff_e4dd_acef),
            (0, i64::MAX, 0x4143_8bff_e4dd_aebe),
            (i64::MIN, 0, 0x4143_8bff_e4dd_aebe),
            (i64::MIN, 1, 0x4143_8bff_e4dd_aebe),
            (i64::MIN, i64::MAX, 0x4143_8bff_e4dd_aebe),
            (i64::MAX, i64::MIN, 0),
            (1_000_000, 999_999, 0),
            (1_000_000, 1_000_000, 0),
        ] {
            assert_eq!(source_duration_hours(anchor, now).to_bits(), bits);
        }
    }

    /// Matches the actual VMM multiplier before and after the source duration cap, with no tolerance.
    /// 无误差容忍地匹配来源时长上限前后的真实 VMM 倍率，避免超长时段改变审核阈值。
    #[test]
    fn long_duration_decay_matches_native_vmm_bits() {
        let mut hit = memory("long-duration", 1.0);
        hit.memory.created_at_ns = 0;
        hit.memory.memory_level = MemoryLevel::Phase;
        hit.memory.priority = 20;
        let mut policy = RetrievalPolicy::v1().decay;
        policy.shape = 1.0;
        policy.scale_hours = 10_000_000.0;
        policy.min_multiplier = 0.0;
        policy.reinforce_weight = 0.0;
        policy.cross_session_boost = 0.0;
        for (now, bits) in [
            (9_223_372_036_854_000_000, 0x3fe9_59e2_43aa_ae86),
            (i64::MAX, 0x3fe9_59e2_43aa_adfa),
            (i64::MAX, 0x3fe9_59e2_43aa_adfa),
            (i64::MAX, 0x3fe9_59e2_43aa_adfa),
        ] {
            assert_eq!(
                weibull_multiplier(&hit.memory, &policy, now).to_bits(),
                bits
            );
        }
    }

    /// Compares against 24 literal native VMM results, not a Rust implementation used to manufacture its own oracle.
    /// 对照 24 条原生 VMM 字面量结果，不用 Rust 实现生成自己的预期值。
    #[test]
    fn weibull_math_matches_native_vmm_matrix() {
        let expected = [
            0x3f76_57ef_2ad6_5781_u64,
            0x3f9b_3825_6988_bd0c,
            0x3fe3_3202_761a_5031,
            0x3fe6_6bad_83ea_6979,
            0x3fee_6717_e492_947a,
            0x3fee_e127_04b5_892d,
            0x3da3_6281_dd64_a54e,
            0x3ece_ee20_d545_b145,
            0x3fe8_7923_e110_10d2,
            0x3fec_0c72_469c_d733,
            0x3fef_e7e6_f760_f489,
            0x3fef_f425_b608_5680,
            0x38cf_b184_9a9a_3a54,
            0x3d33_ab5e_a9eb_80c1,
            0x3feb_0508_f1d6_55d4,
            0x3fed_fda7_58ca_61d0,
            0x3fef_fcdc_77f6_658b,
            0x3fef_fecb_f740_90b0,
            0,
            0x2564_12bb_109d_c21b,
            0x3fee_62e0_e078_3b0e,
            0x3fef_aab5_828c_5675,
            0x3fef_fffb_c280_15c6,
            0x3fef_ffff_2472_9e9a,
        ];
        let mut index = 0;
        let mut mismatches = Vec::new();
        for shape in [0.51, 1.0, 1.35, 2.25] {
            for scale in [0.25, 23.731, 2_160.123_456] {
                for reinforced in [false, true] {
                    let mut hit = memory("native-oracle", 1.0);
                    hit.memory.created_at_ns = 0;
                    hit.memory.memory_level = MemoryLevel::Phase;
                    hit.memory.priority = 20;
                    hit.memory.refresh_weight = if reinforced { 2 } else { 0 };
                    hit.memory.reinforcement_count = if reinforced { 4 } else { 0 };
                    hit.memory.cross_session_adopted_count = if reinforced { 3 } else { 0 };
                    let mut policy = RetrievalPolicy::v1().decay;
                    policy.shape = shape;
                    policy.scale_hours = scale;
                    policy.min_multiplier = 0.0;
                    policy.reinforce_weight = 0.180_000_123;
                    policy.cross_session_boost = 0.120_000_123;
                    let actual =
                        weibull_multiplier(&hit.memory, &policy, 7 * 3_600_000_000_000).to_bits();
                    if actual != expected[index] {
                        mismatches.push(format!("shape={shape} scale={scale} reinforced={reinforced}: {actual:016x} != {:016x}", expected[index]));
                    }
                    index += 1;
                }
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    /// Carries one checked-in native oracle with exact bit strings instead of decimal float parsing.
    /// 携带一个检入的原生预期用例，使用精确位模式字符串而非十进制浮点解析。
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NativeWeibullCase {
        shape_bits: String,
        scale_bits: String,
        age_ms: i64,
        refresh: u32,
        reinforcements: u32,
        cross_session: u32,
        expected_bits: String,
    }

    /// Binds the source commit, actual toolchain and process arithmetic switch to the captured cases.
    /// 将来源提交、实际工具链及进程算术开关绑定到采集用例。
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NativeWeibullCorpus {
        fixture_schema: u32,
        source_commit: String,
        producer_test: String,
        math_profile: vulcan_memorymesh_core_contracts::WeibullMathProfile,
        arithmetic_control: String,
        go_version: String,
        go_arch: String,
        go_debug: String,
        cases: Vec<NativeWeibullCase>,
    }

    /// Replays 384 captured production VMM outcomes across both explicitly selected arithmetic profiles.
    /// 在两种明确选择的算术配置下重放 384 条从生产 VMM 函数采集的输出。
    #[test]
    fn weibull_matches_both_native_oracle_corpora() -> Result<(), Box<dyn std::error::Error>> {
        let mut mismatches = Vec::new();
        for encoded in [
            include_str!("../resources/weibull_math_fma.json"),
            include_str!("../resources/weibull_math_sse2.json"),
        ] {
            let corpus: NativeWeibullCorpus = serde_json::from_str(encoded)?;
            assert_eq!(corpus.fixture_schema, 1);
            assert_eq!(
                corpus.source_commit,
                "82f05fa5e3c7657e770fefd84e220fba5654ce07"
            );
            assert_eq!(
                corpus.producer_test,
                "internal/app/usecase/core_stage0_numeric_precision_test.go::TestCoreStage0WeibullOracleCorpus"
            );
            assert_eq!(corpus.go_version, "go1.27.0");
            assert_eq!(corpus.go_arch, "amd64");
            assert_eq!(corpus.go_debug, "");
            assert_eq!(
                corpus.arithmetic_control,
                "version-pinned-explicit-stdlib-test-override"
            );
            assert_eq!(corpus.cases.len(), 192);
            for (index, case) in corpus.cases.iter().enumerate() {
                let mut hit = memory("native-corpus", 1.0);
                hit.memory.created_at_ns = 0;
                hit.memory.memory_level = MemoryLevel::Phase;
                hit.memory.priority = 20;
                hit.memory.refresh_weight = u64::from(case.refresh);
                hit.memory.reinforcement_count = u64::from(case.reinforcements);
                hit.memory.cross_session_adopted_count = u64::from(case.cross_session);
                let mut policy = RetrievalPolicy::v1().decay;
                policy.math_profile = corpus.math_profile;
                policy.shape = f64::from_bits(u64::from_str_radix(&case.shape_bits, 16)?);
                policy.scale_hours = f64::from_bits(u64::from_str_radix(&case.scale_bits, 16)?);
                policy.min_multiplier = 0.0;
                policy.reinforce_weight = 0.180_000_123;
                policy.cross_session_boost = 0.120_000_123;
                let age_ns = i64::try_from(
                    (i128::from(case.age_ms) * 1_000_000).clamp(0, i128::from(i64::MAX)),
                )?;
                let actual = weibull_multiplier(&hit.memory, &policy, age_ns).to_bits();
                let expected = u64::from_str_radix(&case.expected_bits, 16)?;
                if actual != expected {
                    mismatches.push(format!(
                        "profile={:?} case={index} {actual:016x} != {expected:016x}",
                        corpus.math_profile
                    ));
                }
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
        Ok(())
    }

    /// Keeps context identity normalization compatible with Go's per-rune lowercase mapping.
    /// 保持情境身份归一化与 Go 逐字符小写映射一致，禁止上下文 Sigma 或额外组合字符。
    #[test]
    fn context_normalization_preserves_source_unicode_case() {
        assert_eq!(super::normalize_context_value(" ΟΣ_İ \u{a0} "), "οσ i");
        assert_eq!(super::normalize_context_key(" ΟΣ.İ "), "οσ_i");
        let row = MemoryContextEvidence {
            memory_id: MemoryId::new("unicode-context").unwrap_or_else(|_| std::process::abort()),
            context_key: "label".to_owned(),
            context_value: "οσ".to_owned(),
            support_count: 1,
            rebuttal_count: 0,
            last_supported_at_ns: Some(1),
            last_rebutted_at_ns: None,
            version: 1,
        };
        let score = context_score(&[row], "ΟΣ", &RetrievalPolicy::v1().context);
        assert_eq!(score.matched_values, vec!["label=οσ"]);
        assert_eq!(score.matched_support_count, 1);
    }

    /// Verifies context keys never match and matched values retain complete counts and labels.
    /// 验证情境键绝不参与匹配，且命中值保留完整计数与标签。
    #[test]
    fn context_matching_uses_values_and_preserves_explanation_facts() {
        let memory_id = MemoryId::new("memory-context").unwrap_or_else(|_| std::process::abort());
        let evidence = vec![
            MemoryContextEvidence {
                memory_id: memory_id.clone(),
                context_key: "rust".to_owned(),
                context_value: "irrelevant".to_owned(),
                support_count: 9,
                rebuttal_count: 1,
                last_supported_at_ns: None,
                last_rebutted_at_ns: None,
                version: 1,
            },
            MemoryContextEvidence {
                memory_id,
                context_key: "city".to_owned(),
                context_value: "beijing trip".to_owned(),
                support_count: 2,
                rebuttal_count: 1,
                last_supported_at_ns: None,
                last_rebutted_at_ns: None,
                version: 1,
            },
        ];
        let score = context_score(
            &evidence,
            "plan a beijing trip tomorrow",
            &RetrievalPolicy::v1().context,
        );
        assert_eq!(score.matched_values, vec!["city=beijing trip"]);
        assert_eq!(score.matched_support_count, 2);
        assert_eq!(score.matched_rebuttal_count, 1);
        assert_eq!(score.total_support_count, 11);
        assert_eq!(score.total_rebuttal_count, 2);
        assert!((score.delta - 0.02).abs() < f64::EPSILON);
        let key_only = context_score(&evidence, "rust", &RetrievalPolicy::v1().context);
        assert!(key_only.matched_values.is_empty());
    }

    /// Verifies reinforcement and broader scope extend Weibull scale without additive boosts.
    /// 验证强化与更广作用域只延长 Weibull 尺度而不产生加性提升。
    #[test]
    fn weibull_uses_reinforcement_and_scope_scale() {
        let policy = RetrievalPolicy::v1().decay;
        let mut session = memory("memory-session", 1.0).memory;
        session.scope_class = MemoryScopeClass::Session;
        let mut subject = session.clone();
        subject.id = MemoryId::new("memory-subject").unwrap_or_else(|_| std::process::abort());
        subject.scope_class = MemoryScopeClass::Subject;
        subject.reinforcement_count = 4;
        subject.cross_session_adopted_count = 2;
        let now_ns = 120_i64 * 24 * 3_600_000;
        assert!(
            weibull_multiplier(&subject, &policy, now_ns)
                > weibull_multiplier(&session, &policy, now_ns)
        );
    }

    /// Verifies enabled MMR runs even without truncation and never invents lexical similarity.
    /// 验证启用的 MMR 即使不截断也会运行，且绝不虚构词法相似度。
    #[test]
    fn mmr_runs_for_complete_set_without_lexical_fallback() {
        let selected = mmr_select(
            vec![memory("memory-a", 1.0), memory("memory-b", 0.9)],
            2,
            &RetrievalPolicy::v1().mmr,
        );
        assert_eq!(selected[0].memory.id.as_str(), "memory-a");
        assert_eq!(selected[1].memory.id.as_str(), "memory-b");
        assert!(selected.iter().all(|hit| {
            hit.explanation
                .as_ref()
                .is_some_and(|value| value.mmr_selected == Some(true))
        }));
    }
}
