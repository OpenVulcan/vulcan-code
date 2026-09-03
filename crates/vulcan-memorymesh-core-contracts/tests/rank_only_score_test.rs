//! Verifies absence, zero and producer-mode invariants at the physical-score boundary.
//! 本文件属于契约测试层，验证物理分边界的缺失、零值及生产者模式不变量。

use vulcan_memorymesh_core_contracts::{
    DecisionScoreMode, ErrorCode, PhysicalCandidate, PhysicalCandidateScore, RawScoreKind,
    ScoreContractDescriptor, ScoreContractRef, ScoreDirection, validate_candidate_channel,
    validate_candidate_channel_contract,
};

/// Supplies a finite PG-style lexical fact with explicitly selected decision availability.
/// 提供有限的 PG 风格词法事实，参数选择决策分存在性，返回独立候选。
fn candidate(decision_score: Option<f64>) -> PhysicalCandidate {
    PhysicalCandidate {
        memory_id: "memory".to_owned(),
        physical_rank: 0,
        score: PhysicalCandidateScore {
            raw_value: 12.0,
            raw_score_kind: RawScoreKind::Bm25,
            direction: ScoreDirection::HigherIsBetter,
            decision_score,
            contract: ScoreContractRef {
                id: "rank-only-fixture".to_owned(),
                version: 1,
            },
        },
    }
}

/// Distinguishes a legitimate projected zero from missing data and forbids synthetic rank-only decisions.
/// 区分合法投影零分与缺失数据，并禁止仅排名通道携带合成决策分；返回校验结果。
#[test]
fn absence_and_zero_obey_the_frozen_descriptor() -> Result<(), Box<dyn std::error::Error>> {
    for (mode, value, accepted) in [
        (DecisionScoreMode::RankOnly, None, true),
        (DecisionScoreMode::RankOnly, Some(0.0), false),
        (DecisionScoreMode::HostProjected, None, false),
        (DecisionScoreMode::HostProjected, Some(0.0), true),
    ] {
        let row = candidate(value);
        let descriptor = ScoreContractDescriptor {
            contract: row.score.contract.clone(),
            raw_score_kind: row.score.raw_score_kind,
            direction: row.score.direction,
            decision_score_mode: mode,
        };
        validate_candidate_channel(std::slice::from_ref(&row))?;
        assert_eq!(
            validate_candidate_channel_contract(&[row], &descriptor).is_ok(),
            accepted
        );
    }
    assert!(candidate(None).score.require_decision_score().is_err());
    assert_eq!(
        candidate(Some(0.0))
            .score
            .require_decision_score()?
            .to_bits(),
        0.0_f64.to_bits()
    );
    Ok(())
}

/// Keeps raw ordering and mode uniformity mandatory even when no decision values exist.
/// 即使没有决策分，也强制原始排序和模式一致性；返回校验结果。
#[test]
fn rank_only_channels_reject_mixed_presence_and_nonfinite_raw_facts()
-> Result<(), Box<dyn std::error::Error>> {
    let first = candidate(None);
    let mut second = first.clone();
    second.memory_id = "second".to_owned();
    second.physical_rank = 2;
    second.score.raw_value = 10.0;
    validate_candidate_channel(&[first.clone(), second.clone()])?;
    second.score.decision_score = Some(0.0);
    assert_eq!(
        validate_candidate_channel(&[first, second])
            .err()
            .ok_or("mixed mode accepted")?
            .code,
        ErrorCode::ScoreContractViolation
    );
    for raw in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut row = candidate(None);
        row.score.raw_value = raw;
        assert!(validate_candidate_channel(&[row]).is_err());
    }
    Ok(())
}
