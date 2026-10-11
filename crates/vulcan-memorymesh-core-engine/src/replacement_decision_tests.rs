//! Replacement-review decisions: the similarity floor, the result window, and local hard dedupe.
//! 更替审核决策：相似度下限、结果窗口与本地硬去重。

use super::*;
use vulcan_memorymesh_core_contracts::{
    MemoryCategory, MmrPolicy, RetrievalPolicy, ScopeId, ScoreExplanation, SubjectId,
    ThresholdBinding,
};

/// Builds one active general memory with an optional stored embedding.
/// 构建一条带可选存储向量的活动通用记忆。
fn record(identity: &str, embedding: Option<Vec<f32>>) -> MemoryRecord {
    MemoryRecord {
        id: MemoryId::new(identity).unwrap_or_else(|_| std::process::abort()),
        access_class: MemoryAccessClass::SubjectPrivate,
        source_subject_id: Some(
            SubjectId::new("subject-replacement").unwrap_or_else(|_| std::process::abort()),
        ),
        scope_id: ScopeId::new("scope-replacement").unwrap_or_else(|_| std::process::abort()),
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
        embedding,
    }
}

/// Wraps one memory as a final-scored hit without a rerank stage.
/// 将一条记忆包装为未经重排阶段的最终评分命中。
fn hit(memory: MemoryRecord, score: f64) -> SearchHit {
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

/// Returns the default policy with the replacement floor bound to its own vector contract.
/// 返回默认策略，并将更替下限绑定到其自身向量契约。
fn policy_with_floor(floor: f64) -> RetrievalPolicy {
    let mut policy = RetrievalPolicy::v1();
    policy.replace_min_similarity = vec![ThresholdBinding {
        contract: policy.vector_contract.contract.clone(),
        threshold: floor,
    }];
    policy
}

/// Lists hit identities in their returned order.
/// 按返回顺序列出命中身份。
fn identities(hits: &[SearchHit]) -> Vec<String> {
    hits.iter().map(|hit| hit.memory.id.to_string()).collect()
}

/// Checks the replacement floor is inclusive and removes only hits strictly below it.
/// 检查更替下限为闭区间，只移除严格低于下限的命中。
#[test]
fn replacement_floor_keeps_hits_at_or_above_the_bound() {
    let ranked = vec![
        hit(record("a", None), 0.95),
        hit(record("b", None), 0.88),
        hit(record("c", None), 0.8),
    ];
    for (floor, expected) in [
        (0.0, vec!["a", "b", "c"]),
        (0.88, vec!["a", "b"]),
        (0.9, vec!["a"]),
        (1.0, vec![]),
    ] {
        let reviewed = review_similar_memories(ranked.clone(), &policy_with_floor(floor));
        assert_eq!(identities(&reviewed), expected, "floor {floor}");
    }
}

/// Checks review drops textless hits and repeated identities, then orders by score and identity.
/// 检查审核丢弃无文本命中与重复身份，并按评分及身份排序。
#[test]
fn review_drops_textless_and_repeated_hits_in_score_order() {
    let mut textless = record("empty", None);
    textless.summary = " ".to_owned();
    textless.content = String::new();
    let ranked = vec![
        hit(record("b", None), 0.9),
        hit(record("a", None), 0.9),
        hit(record("b", None), 0.7),
        hit(textless, 0.95),
        hit(record("c", None), 0.92),
    ];
    let reviewed = review_similar_memories(ranked, &policy_with_floor(0.0));
    assert_eq!(identities(&reviewed), vec!["c", "a", "b"]);
}

/// Checks the replacement window caps similar memories when MMR is disabled.
/// 检查 MMR 关闭时更替窗口限制相似记忆数量。
#[test]
fn replacement_window_caps_similar_memories() {
    let ranked = vec![
        hit(record("a", None), 0.95),
        hit(record("b", None), 0.9),
        hit(record("c", None), 0.85),
    ];
    let mmr = MmrPolicy {
        enabled: false,
        ..RetrievalPolicy::v1().mmr
    };
    for (limit, expected) in [
        (1, vec!["a"]),
        (2, vec!["a", "b"]),
        (5, vec!["a", "b", "c"]),
    ] {
        let selected = retrieval_kernel::mmr_select(ranked.clone(), limit, &mmr);
        assert_eq!(identities(&selected), expected, "limit {limit}");
    }
}

/// Checks the hard-dedupe threshold is inclusive and zero disables hard dedupe entirely.
/// 检查硬去重阈值为闭区间，且零值完全关闭硬去重。
#[test]
fn hard_duplicate_threshold_is_inclusive_and_zero_disables() {
    // The stored vector has cosine 0.6 against the candidate vector.
    // 存储向量与候选向量的余弦为 0.6。
    let pool = vec![record("near", Some(vec![0.6, 0.8]))];
    let candidate = [1.0_f32, 0.0];
    for (threshold, expected) in [
        (0.55, Some("near")),
        (0.6, Some("near")),
        (0.65, None),
        (0.0, None),
    ] {
        let selected = select_hard_duplicate(&pool, MemoryCategory::General, &candidate, threshold);
        assert_eq!(
            selected.map(|memory| memory.id.to_string()).as_deref(),
            expected,
            "threshold {threshold}"
        );
    }
}

/// Checks only active same-category records with stored vectors can be hard duplicates.
/// 检查只有带存储向量的活动同类别记录才能成为硬重复项。
#[test]
fn hard_duplicate_ignores_other_categories_inactive_and_unembedded_records() {
    let mut other_category = record("other-category", Some(vec![1.0, 0.0]));
    other_category.category = MemoryCategory::BusinessLogic;
    let mut superseded = record("superseded", Some(vec![1.0, 0.0]));
    superseded.state = MemoryState::Superseded;
    let unembedded = record("unembedded", None);
    let pool = vec![other_category, superseded, unembedded];
    assert!(select_hard_duplicate(&pool, MemoryCategory::General, &[1.0, 0.0], 0.5).is_none());
}

/// Checks the strongest cosine wins and equal cosines resolve to the smaller identity.
/// 检查余弦最强者胜出，余弦相同时取较小身份。
#[test]
fn hard_duplicate_prefers_strongest_cosine_then_smaller_identity() {
    let pool = vec![
        record("weaker", Some(vec![0.6, 0.8])),
        record("tie-b", Some(vec![1.0, 0.0])),
        record("tie-a", Some(vec![2.0, 0.0])),
    ];
    let selected = select_hard_duplicate(&pool, MemoryCategory::General, &[1.0, 0.0], 0.5);
    assert_eq!(
        selected.map(|memory| memory.id.to_string()).as_deref(),
        Some("tie-a")
    );
}

/// Checks the hard-dedupe pool spans the larger of its own size and the result limit, within the ceiling.
/// 检查硬去重池取自身大小与结果数量中的较大者，并受候选上限约束。
#[test]
fn hard_dedupe_pool_spans_the_larger_window_within_the_ceiling() {
    let ranked: Vec<SearchHit> = (0..40)
        .map(|index| hit(record(&format!("m{index:02}"), None), 0.5))
        .collect();
    for (pool_top_k, limit, ceiling, expected) in
        [(3, 1, 32, 3), (1, 1, 32, 1), (1, 5, 32, 5), (16, 1, 4, 4)]
    {
        let mut policy = RetrievalPolicy::v1();
        policy.hard_dedupe_pool_top_k = pool_top_k;
        policy.candidate_pool_max = ceiling;
        assert_eq!(
            hard_dedupe_pool(&ranked, limit, &policy).len(),
            expected,
            "pool {pool_top_k}, limit {limit}, ceiling {ceiling}"
        );
    }
}

/// Checks a duplicate ranked outside a narrow pool is not seen by hard dedupe.
/// 检查排名在窄预审池之外的重复项不会被硬去重看到。
#[test]
fn hard_duplicate_outside_the_pool_is_not_dropped() {
    let ranked = vec![
        hit(record("top", Some(vec![0.3, 0.954])), 0.95),
        hit(record("duplicate", Some(vec![0.6, 0.8])), 0.9),
    ];
    let candidate = [1.0_f32, 0.0];
    for (pool_top_k, expected) in [(1, None), (3, Some("duplicate"))] {
        let mut policy = RetrievalPolicy::v1();
        policy.hard_dedupe_pool_top_k = pool_top_k;
        let pool = hard_dedupe_pool(&ranked, 1, &policy);
        let selected = select_hard_duplicate(&pool, MemoryCategory::General, &candidate, 0.55);
        assert_eq!(
            selected.map(|memory| memory.id.to_string()).as_deref(),
            expected,
            "pool {pool_top_k}"
        );
    }
}
