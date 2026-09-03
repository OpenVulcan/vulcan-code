//! Source-shaped tests for query-local review projection, merging and private-record isolation.
//! 查询局部审核投影、合并及私有记录隔离的来源形状测试。

use crate::precheck::{
    ReviewAccumulator, accumulate_review_group, finish_review_candidates,
    validate_review_accumulator, validate_review_states,
};
use std::sync::Arc;
use vulcan_memorymesh_core_config::{ConfigBundle, ConfigRegistry, ConfigSnapshot, ResourceLayer};
use vulcan_memorymesh_core_contracts::{
    MemoryCategory, MemoryId, MemoryLevel, MemoryRecord, MemoryScopeClass, MemorySourceKind,
    MemoryStability, MemoryState, PreCheckPolicy, ScopeId, ScoreExplanation, SearchHit, SubjectId,
};

/// Builds an explicit raw record so tests can compare every original field after presentation changes.
/// 构造明确的原始记录，使测试可在展示变化后比较全部原字段。
fn hit(id: &str, score: f64, reranked: bool) -> Result<SearchHit, Box<dyn std::error::Error>> {
    Ok(SearchHit {
        memory: MemoryRecord {
            id: MemoryId::new(id)?,
            access_class: vulcan_memorymesh_core_contracts::MemoryAccessClass::SubjectPrivate,
            source_subject_id: Some(SubjectId::new("subject")?),
            scope_id: ScopeId::new("scope")?,
            scope_class: MemoryScopeClass::Environment,
            category: MemoryCategory::General,
            summary: id.to_owned(),
            content: id.to_owned(),
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
            created_at_ns: 1,
            updated_at_ns: 1,
            expires_at_ns: None,
            last_recalled_at_ns: None,
            reinforcement_count: 0,
            memory_level: MemoryLevel::Stable,
            refresh_weight: 1,
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
        },
        search_score: score,
        rerank_score: reranked.then_some(score),
        final_score: score,
        explanation: Some(ScoreExplanation {
            raw_vector_score: Some(score),
            vector_decision_score: Some(score),
            rerank_score: reranked.then_some(score),
            final_score: score,
            ..ScoreExplanation::default()
        }),
    })
}

/// Compiles a complete explicit rule layer so masking tests do not depend on unrelated bundled patterns.
/// 编译完整显式规则层，使脱敏测试不依赖无关内嵌正则规则。
fn snapshot(rules: &str) -> Result<Arc<ConfigSnapshot>, Box<dyn std::error::Error>> {
    let mut layer = ResourceLayer::default();
    for (language, definitions) in [("common", rules), ("zh-CN", "[]")] {
        layer.insert(
            format!("pii_rules/{language}.json"),
            format!(r#"{{"language":"{language}","version":"2.0.0","rules":{definitions}}}"#)
                .into_bytes(),
        )?;
    }
    Ok(ConfigRegistry::new()?.register(ConfigBundle {
        user_layer: layer,
        ..ConfigBundle::default()
    })?)
}

/// Preserves one top reranked candidate per query and stable first-seen ties after the 0.8 reviewer gate.
/// 在0.8审核门禁后保留每查询的第一条重排候选，并保持同分首见顺序。
#[test]
fn each_query_projects_before_threshold_and_merge() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = snapshot("[]")?;
    let policy = PreCheckPolicy {
        min_review_score: 0.8,
        ..PreCheckPolicy::default()
    };
    let mut accumulated = ReviewAccumulator::default();
    accumulate_review_group(
        &mut accumulated,
        vec![hit("20", 0.27, true)?, hit("10", 0.18, true)?],
        &policy,
    )?;
    accumulate_review_group(
        &mut accumulated,
        vec![
            hit("30", 0.11, true)?,
            hit("40", 0.8 - f64::EPSILON, false)?,
        ],
        &policy,
    )?;
    assert_eq!(accumulated.processed_groups, 2);
    let candidates = finish_review_candidates(accumulated, &policy, &snapshot.pii_scrubber)?;
    assert_eq!(
        candidates
            .iter()
            .map(|row| row.view.memory_id.as_str())
            .collect::<Vec<_>>(),
        ["20", "30"]
    );
    for row in candidates {
        assert_eq!(row.view.score.to_bits(), 1.0_f64.to_bits());
        assert_eq!(
            row.view.explanation.pre_review_score.map(f64::to_bits),
            Some(row.source.final_score.to_bits())
        );
        assert_ne!(row.view.score.to_bits(), row.source.final_score.to_bits());
    }
    Ok(())
}

/// Keeps richer merged text and maximum evidence in the redacted view while preserving the exact primary record/version.
/// 在脱敏视图中保留更完整文本和最大证据，同时完整保留主要原记录及版本。
#[test]
fn merged_view_does_not_rewrite_the_source_record() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = snapshot(r#"[{"name":"secret","pattern":"secret[12]","replacement":"MASK"}]"#)?;
    let policy = PreCheckPolicy::default();
    let mut primary = hit("same", 0.94, false)?;
    primary.memory.summary = "短".to_owned();
    primary.memory.content = "secret1".to_owned();
    let primary_explanation = primary.explanation.as_mut().ok_or("primary explanation")?;
    primary_explanation.context_matched_values = vec![" Build-Mode = secret1 ".to_owned()];
    primary_explanation.context_matched_support_count = 2;
    primary_explanation.context_delta = Some(0.1);
    let original = primary.clone();
    let mut secondary = hit("same", 0.93, false)?;
    secondary.memory.summary = "long secret2 summary".to_owned();
    secondary.memory.content = "longer secret2\n  detail".to_owned();
    secondary.memory.version = 99;
    let secondary_explanation = secondary
        .explanation
        .as_mut()
        .ok_or("secondary explanation")?;
    secondary_explanation.context_matched_values = vec!["build_mode=secret2".to_owned()];
    secondary_explanation.context_matched_support_count = 3;
    secondary_explanation.context_total_support_count = 7;
    secondary_explanation.context_delta = Some(-0.2);
    let mut accumulated = ReviewAccumulator::default();
    accumulate_review_group(&mut accumulated, vec![primary], &policy)?;
    accumulate_review_group(&mut accumulated, vec![secondary], &policy)?;
    let candidates = finish_review_candidates(accumulated, &policy, &snapshot.pii_scrubber)?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].source, original);
    let view = &candidates[0].view;
    assert_eq!(view.summary, "long MASK summary");
    assert_eq!(view.details, "longer MASK\n  detail");
    assert_eq!(view.explanation.context_matched_values, ["build_mode=mask"]);
    assert_eq!(view.explanation.context_matched_support_count, 3);
    assert_eq!(view.explanation.context_total_support_count, 7);
    assert_eq!(
        view.explanation.context_delta.map(f64::to_bits),
        Some((-0.2_f64).to_bits())
    );
    Ok(())
}

/// Does not re-project lower-ranked rows when the originally highest candidate disappears after redaction.
/// 原最高候选脱敏后消失时，不重新投影其余低排名候选。
#[test]
fn redaction_removal_never_reopens_the_query_rank_gate() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = snapshot(r#"[{"name":"drop","pattern":"secret","replacement":""}]"#)?;
    let policy = PreCheckPolicy {
        min_review_score: 0.8,
        ..PreCheckPolicy::default()
    };
    let mut removed = hit("removed", 0.2, true)?;
    removed.memory.summary = "secret".to_owned();
    removed.memory.content = "secret".to_owned();
    let mut accumulated = ReviewAccumulator::default();
    accumulate_review_group(
        &mut accumulated,
        vec![removed, hit("second", 0.2, true)?],
        &policy,
    )?;
    assert!(finish_review_candidates(accumulated, &policy, &snapshot.pii_scrubber)?.is_empty());
    Ok(())
}

/// Retains an untouched single-hit label verbatim while applying the final global reviewer limit only once.
/// 保留未合并单次命中标签的原写法，并仅一次应用最终全局送审上限。
#[test]
fn reviewer_limit_is_global_and_single_hit_labels_stay_verbatim()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot = snapshot("[]")?;
    let policy = PreCheckPolicy {
        recall_top_k: 2,
        review_candidate_limit: 5,
        min_review_score: 0.8,
    };
    let mut accumulated = ReviewAccumulator::default();
    for ids in [["50", "40"], ["30", "20"], ["10", "05"]] {
        let mut first = hit(ids[0], 0.9, false)?;
        first
            .explanation
            .as_mut()
            .ok_or("explanation")?
            .context_matched_values = vec![" Unchanged = Mixed/Case ".to_owned()];
        accumulate_review_group(
            &mut accumulated,
            vec![first, hit(ids[1], 0.9, false)?],
            &policy,
        )?;
    }
    let candidates = finish_review_candidates(accumulated, &policy, &snapshot.pii_scrubber)?;
    assert_eq!(
        candidates
            .iter()
            .map(|row| row.view.memory_id.as_str())
            .collect::<Vec<_>>(),
        ["50", "40", "30", "20", "10"]
    );
    assert_eq!(
        candidates[0].view.explanation.context_matched_values,
        [" Unchanged = Mixed/Case "]
    );
    Ok(())
}

/// Rejects inconsistent checkpoint merge counters, detached source identities and malformed reviewer scores.
/// 拒绝检查点合并计数不一致、来源身份脱离及非法审核分，不能依赖后续宿主效果才发现错误。
#[test]
fn review_continuation_invariants_reject_corruption() -> Result<(), Box<dyn std::error::Error>> {
    let policy = PreCheckPolicy::default();
    let snapshot = snapshot("[]")?;
    let mut accumulated = ReviewAccumulator::default();
    assert!(validate_review_accumulator(&accumulated, 0, &policy));
    accumulate_review_group(&mut accumulated, vec![hit("one", 0.9, false)?], &policy)?;
    assert!(validate_review_accumulator(&accumulated, 1, &policy));
    assert!(!validate_review_accumulator(&accumulated, 0, &policy));
    assert!(!validate_review_accumulator(&accumulated, 2, &policy));
    let valid = finish_review_candidates(accumulated, &policy, &snapshot.pii_scrubber)?;
    assert!(validate_review_states(&valid, &policy));
    let mut invalid = valid.clone();
    invalid[0].view.memory_id = MemoryId::new("detached")?;
    assert!(!validate_review_states(&invalid, &policy));
    invalid.clone_from(&valid);
    invalid[0].view.score = f64::NAN;
    assert!(!validate_review_states(&invalid, &policy));
    invalid.clone_from(&valid);
    invalid[0].view.explanation.final_score = 0.8;
    assert!(!validate_review_states(&invalid, &policy));
    invalid.clone_from(&valid);
    invalid.push(valid[0].clone());
    assert!(!validate_review_states(&invalid, &policy));
    Ok(())
}
