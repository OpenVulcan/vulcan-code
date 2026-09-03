//! Contract-bound candidate channel validation tests.
//! 契约绑定的候选通道校验测试。

#![allow(clippy::expect_used)]

use vulcan_memorymesh_core_contracts::{
    ErrorCode, ErrorStage, PhysicalCandidate, PhysicalCandidateScore, RawScoreKind,
    ScoreContractDescriptor, ScoreContractRef, ScoreDirection, validate_candidate_channel,
    validate_candidate_channel_contract,
};

/// Builds one physical candidate with the shared contract and direction.
/// 使用共享契约与方向构建一个物理候选。
fn candidate(
    memory_id: &str,
    rank: u32,
    decision_score: f64,
    contract: &ScoreContractRef,
    direction: ScoreDirection,
) -> PhysicalCandidate {
    PhysicalCandidate {
        memory_id: memory_id.to_owned(),
        score: PhysicalCandidateScore {
            raw_value: decision_score,
            raw_score_kind: RawScoreKind::CosineSimilarity,
            direction,
            decision_score: Some(decision_score),
            contract: contract.clone(),
        },
        physical_rank: rank,
    }
}

/// Verifies a well-ordered channel passes with a monotonic decision sequence.
/// 验证有序且决策序列单调的通道通过校验。
#[test]
fn well_ordered_channel_passes() {
    let contract = ScoreContractRef {
        id: "reference-cosine".to_owned(),
        version: 1,
    };
    let channel = vec![
        candidate("a", 0, 0.95, &contract, ScoreDirection::HigherIsBetter),
        candidate("b", 1, 0.90, &contract, ScoreDirection::HigherIsBetter),
        candidate("c", 2, 0.90, &contract, ScoreDirection::HigherIsBetter),
    ];
    validate_candidate_channel(&channel).expect("ordered channel must pass");
    let descriptor = ScoreContractDescriptor {
        decision_score_mode: vulcan_memorymesh_core_contracts::DecisionScoreMode::HostProjected,
        contract,
        raw_score_kind: RawScoreKind::CosineSimilarity,
        direction: ScoreDirection::HigherIsBetter,
    };
    validate_candidate_channel_contract(&channel, &descriptor)
        .expect("matching contract semantics must pass");
}

/// Verifies an empty channel is a legitimate no-results outcome.
/// 验证空通道是合法的无结果结局。
#[test]
fn empty_channel_passes() {
    validate_candidate_channel(&[]).expect("empty channel must pass");
}

/// Preserves sparse source positions after relational binding without accepting duplicate or reversed ranks.
/// 保留关系绑定后的稀疏来源位置，同时拒绝重复或倒退的排名；输入固定通道，无返回值。
#[test]
fn sparse_source_ranks_pass_but_duplicates_and_reversal_fail() {
    let contract = ScoreContractRef {
        id: "physical-source".to_owned(),
        version: 1,
    };
    let mut channel = vec![
        candidate("a", 4, 0.9, &contract, ScoreDirection::HigherIsBetter),
        candidate("b", 6, 0.8, &contract, ScoreDirection::HigherIsBetter),
    ];
    validate_candidate_channel(&channel).expect("filtered source positions remain valid");
    channel[1].physical_rank = u32::MAX;
    validate_candidate_channel(&channel).expect("largest source position is representable");
    for invalid in [4, 3] {
        channel[1].physical_rank = invalid;
        assert_eq!(
            validate_candidate_channel(&channel)
                .expect_err("rank order")
                .code,
            ErrorCode::ScoreContractViolation
        );
    }
}

/// Verifies decision scores must not increase with the physical rank.
/// 验证决策分值不得随物理排名升高而升高。
#[test]
fn decision_score_backsliding_is_rejected() {
    let contract = ScoreContractRef {
        id: "reference-cosine".to_owned(),
        version: 1,
    };
    let mut channel = vec![
        candidate("a", 0, 0.70, &contract, ScoreDirection::HigherIsBetter),
        candidate("b", 1, 0.95, &contract, ScoreDirection::HigherIsBetter),
    ];
    channel[0].score.raw_value = 0.95;
    channel[1].score.raw_value = 0.70;
    let error = validate_candidate_channel(&channel)
        .expect_err("backsliding decision score must be rejected");
    assert_eq!(error.code, ErrorCode::ScoreContractViolation);
    assert_eq!(error.stage, ErrorStage::Effect);
    assert!(error.diagnostic.contains("monotonically"));
}

/// Verifies a channel mixing contracts, raw kinds, or directions is rejected.
/// 验证混合契约、原始类别或方向的通道被拒绝。
#[test]
fn mixed_contract_or_direction_is_rejected() {
    let contract_a = ScoreContractRef {
        id: "reference-cosine".to_owned(),
        version: 1,
    };
    let contract_b = ScoreContractRef {
        id: "custom-vector".to_owned(),
        version: 2,
    };
    let mixed_contract = vec![
        candidate("a", 0, 0.95, &contract_a, ScoreDirection::HigherIsBetter),
        candidate("b", 1, 0.90, &contract_b, ScoreDirection::HigherIsBetter),
    ];
    let error =
        validate_candidate_channel(&mixed_contract).expect_err("mixed contracts must be rejected");
    assert!(error.diagnostic.contains("mixes"));

    let mixed_direction = vec![
        candidate("a", 0, 0.95, &contract_a, ScoreDirection::HigherIsBetter),
        candidate("b", 1, 0.90, &contract_a, ScoreDirection::LowerIsBetter),
    ];
    let error = validate_candidate_channel(&mixed_direction)
        .expect_err("mixed directions must be rejected");
    assert!(error.diagnostic.contains("mixes"));
}

/// Verifies a channel scored by a different contract than requested is rejected.
/// 验证由与请求不同契约评分的通道被拒绝。
#[test]
fn returned_contract_must_match_expected() {
    let expected = ScoreContractDescriptor {
        decision_score_mode: vulcan_memorymesh_core_contracts::DecisionScoreMode::HostProjected,
        contract: ScoreContractRef {
            id: "reference-cosine".to_owned(),
            version: 1,
        },
        raw_score_kind: RawScoreKind::CosineSimilarity,
        direction: ScoreDirection::HigherIsBetter,
    };
    let actual = ScoreContractRef {
        id: "custom-vector".to_owned(),
        version: 2,
    };
    let channel = vec![candidate(
        "a",
        0,
        0.95,
        &actual,
        ScoreDirection::HigherIsBetter,
    )];
    let error = validate_candidate_channel_contract(&channel, &expected)
        .expect_err("mismatched contract must be rejected");
    assert_eq!(error.code, ErrorCode::ScoreContractViolation);
    assert!(error.diagnostic.contains("differ from the requested"));
}

/// Verifies one matching identity cannot lie about its raw kind or direction.
/// 验证一个匹配身份不能谎报其原始类别或方向。
#[test]
fn returned_raw_semantics_must_match_expected_descriptor() {
    let contract = ScoreContractRef {
        id: "vmm-pgvector-cosine-distance".to_owned(),
        version: 1,
    };
    let expected = ScoreContractDescriptor {
        decision_score_mode: vulcan_memorymesh_core_contracts::DecisionScoreMode::HostProjected,
        contract: contract.clone(),
        raw_score_kind: RawScoreKind::CosineDistance,
        direction: ScoreDirection::LowerIsBetter,
    };
    let channel = vec![candidate(
        "a",
        0,
        0.95,
        &contract,
        ScoreDirection::HigherIsBetter,
    )];
    let error = validate_candidate_channel_contract(&channel, &expected)
        .expect_err("same identity with false raw semantics must be rejected");
    assert_eq!(error.code, ErrorCode::ScoreContractViolation);
    assert!(error.diagnostic.contains("differ from the requested"));
}
