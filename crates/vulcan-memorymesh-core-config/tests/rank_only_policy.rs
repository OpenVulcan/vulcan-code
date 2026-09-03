//! Freezes rank-only compatibility and snapshot identity without database-specific formulas.
//! 本文件属于配置验证层，冻结仅排名模式的策略兼容性与快照身份，不引入数据库公式。

use vulcan_memorymesh_core_config::{ConfigBundle, ConfigRegistry};
use vulcan_memorymesh_core_contracts::{
    CandidateReadMode, DecisionScoreMode, ErrorCode, RrfMathProfile, RrfScoreMode,
    SeparateChannelFallbackPolicy, ThresholdBinding,
};

/// Builds one otherwise valid complete-observation policy with distinct split-read producers.
/// 构建一份其余字段合法、带不同分离读取生产者的完整观察策略；返回配置输入。
fn fallback_bundle() -> ConfigBundle {
    let mut bundle = ConfigBundle::default();
    let retrieval = &mut bundle.memory_policy.retrieval_policy;
    retrieval.candidate_read_mode = CandidateReadMode::SingleObservation;
    retrieval.rrf_score_mode = RrfScoreMode::FusedRankOnly;
    let mut vector = retrieval.vector_contract.clone();
    "fallback-vector".clone_into(&mut vector.contract.id);
    let mut lexical = retrieval.lexical_contract.clone();
    "fallback-lexical".clone_into(&mut lexical.contract.id);
    retrieval.separate_channel_fallback = Some(SeparateChannelFallbackPolicy {
        vector_contract: vector,
        lexical_contract: lexical,
        rrf_math_profile: RrfMathProfile::Signed64WrappingThenFloat64V1,
        dual_channel_bonus: 0.03,
    });
    bundle
}

/// Rejects score-consuming configurations while permitting an explicitly inactive lexical contract.
/// 拒绝需要通道决策分的配置，同时允许明确停用的词法契约；返回验证结果。
#[test]
fn rank_only_requires_compatible_consumers() -> Result<(), Box<dyn std::error::Error>> {
    for case in 0..6 {
        let mut bundle = ConfigBundle::default();
        let policy = &mut bundle.memory_policy.retrieval_policy;
        policy.lexical_contract.decision_score_mode = DecisionScoreMode::RankOnly;
        match case {
            0 => {}
            1 => policy.rrf_score_mode = RrfScoreMode::FusedRankOnly,
            2 => policy.hybrid_enabled = false,
            3 => {
                policy.rrf_score_mode = RrfScoreMode::FusedRankOnly;
                policy.min_similarity.push(ThresholdBinding {
                    contract: policy.lexical_contract.contract.clone(),
                    threshold: 0.5,
                });
            }
            4 => {
                policy.hybrid_enabled = false;
                policy.vector_contract.decision_score_mode = DecisionScoreMode::RankOnly;
            }
            5 => {
                policy.rrf_score_mode = RrfScoreMode::FusedRankOnly;
                policy.min_similarity.push(ThresholdBinding {
                    contract: policy.vector_contract.contract.clone(),
                    threshold: 0.5,
                });
            }
            _ => unreachable!(),
        }
        let result = ConfigRegistry::new()?.register(bundle);
        if [1, 2, 5].contains(&case) {
            result?;
        } else {
            assert_eq!(
                result.err().ok_or("invalid policy accepted")?.code,
                ErrorCode::InvalidConfiguration
            );
        }
    }
    Ok(())
}

/// Binds availability into immutable identity independently of all other producer and policy fields.
/// 将分值可用性独立绑定到不可变身份，其他生产者和策略字段保持不变；返回验证结果。
#[test]
fn score_presence_changes_snapshot_digest() -> Result<(), Box<dyn std::error::Error>> {
    let mut bundle = ConfigBundle::default();
    bundle.memory_policy.retrieval_policy.rrf_score_mode = RrfScoreMode::FusedRankOnly;
    let projected = ConfigRegistry::new()?.register(bundle.clone())?;
    bundle
        .memory_policy
        .retrieval_policy
        .lexical_contract
        .decision_score_mode = DecisionScoreMode::RankOnly;
    let rank_only = ConfigRegistry::new()?.register(bundle)?;
    assert_ne!(projected.id, rank_only.id);
    assert_ne!(
        projected.memory_policy_digest,
        rank_only.memory_policy_digest
    );
    Ok(())
}

/// Accepts only the explicit single-observation recovery shape and rejects every ambiguous variant.
/// 仅接受显式单次观察恢复形状，并拒绝每一种含糊变体。
#[test]
fn separate_fallback_requires_closed_host_projected_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    ConfigRegistry::new()?.register(fallback_bundle())?;
    for case in 0..7 {
        let mut bundle = fallback_bundle();
        let retrieval = &mut bundle.memory_policy.retrieval_policy;
        if case < 3 {
            match case {
                0 => retrieval.candidate_read_mode = CandidateReadMode::SeparateChannels,
                1 => retrieval.hybrid_enabled = false,
                2 => retrieval.rrf_score_mode = RrfScoreMode::RankNormalized,
                _ => return Err("unknown fallback validation case".into()),
            }
        } else {
            let fallback = retrieval
                .separate_channel_fallback
                .as_mut()
                .ok_or("fallback missing")?;
            match case {
                3 => fallback.vector_contract.decision_score_mode = DecisionScoreMode::RankOnly,
                4 => {
                    fallback.lexical_contract.contract = fallback.vector_contract.contract.clone();
                }
                5 => fallback.lexical_contract.contract.version = 0,
                6 => fallback.dual_channel_bonus = f64::NAN,
                _ => return Err("unknown fallback validation case".into()),
            }
        }
        let error = ConfigRegistry::new()?
            .register(bundle)
            .err()
            .ok_or("invalid fallback accepted")?;
        if error.code != ErrorCode::InvalidConfiguration {
            return Err("invalid fallback returned the wrong error".into());
        }
    }
    Ok(())
}

/// Binds fallback presence, producer identity, arithmetic and bonus into immutable snapshot identity.
/// 将降级存在性、生产者身份、算术与加成绑定进不可变快照身份。
#[test]
fn separate_fallback_changes_snapshot_digest() -> Result<(), Box<dyn std::error::Error>> {
    let base = fallback_bundle();
    let original = ConfigRegistry::new()?.register(base.clone())?;
    for case in 0..4 {
        let mut changed = base.clone();
        let fallback = changed
            .memory_policy
            .retrieval_policy
            .separate_channel_fallback
            .as_mut()
            .ok_or("fallback missing")?;
        match case {
            0 => fallback.vector_contract.contract.id.push_str("-changed"),
            1 => fallback.lexical_contract.contract.version += 1,
            2 => fallback.rrf_math_profile = RrfMathProfile::Float64AddV1,
            3 => fallback.dual_channel_bonus = 0.031,
            _ => return Err("unknown fallback digest case".into()),
        }
        let snapshot = ConfigRegistry::new()?.register(changed)?;
        if snapshot.id == original.id
            || snapshot.memory_policy_digest == original.memory_policy_digest
        {
            return Err("fallback change did not alter frozen identity".into());
        }
    }
    Ok(())
}
