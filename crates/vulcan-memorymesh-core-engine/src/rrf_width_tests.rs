//! Verifies wide RRF arithmetic against frozen VMM integer-source results and explicit binary64 expressions.
//! 本文件属于引擎测试层，对照冻结VMM整型来源结果及明确双精度表达式验证宽RRF算术。

use vulcan_memorymesh_core_contracts::{
    PhysicalCandidate, PhysicalCandidateScore, RetrievalPolicy, RrfMathProfile,
    ScoreContractDescriptor,
};

/// Builds equal-score physical rows for explicit numeric identities; the descriptor fixes provenance, not denominator math.
/// 根据明确数字身份构造等分物理行；描述符固定来源而非分母算术，返回独立候选数组。
fn rows(ids: &[u8], descriptor: &ScoreContractDescriptor) -> Vec<PhysicalCandidate> {
    ids.iter()
        .zip(0_u32..)
        .map(|(id, rank)| PhysicalCandidate {
            memory_id: format!("memory-{id}"),
            physical_rank: rank,
            score: PhysicalCandidateScore {
                raw_value: 0.4,
                decision_score: Some(0.4),
                raw_score_kind: descriptor.raw_score_kind,
                direction: descriptor.direction,
                contract: descriptor.contract.clone(),
            },
        })
        .collect()
}

/// Freezes one exact source configuration, its physical order and observable final output.
/// 冻结一组精确来源配置、物理顺序及可观察最终输出。
struct SourceCase<'a> {
    /// Carries the source parameter and both ordered physical channels.
    /// 携带来源参数及两路有序物理通道。
    parameter: u64,
    vector: &'a [u8],
    lexical: &'a [u8],
    /// Carries actual VMM identity order and public scores measured before Core changes.
    /// 携带 Core 修改前测得的实际 VMM 身份顺序及公开分值。
    order: &'a [u8],
    scores: &'a [f64],
}

/// Verifies real source output at the u32, binary64 and signed-64 boundaries; errors mean a source observation cannot be reproduced.
/// 验证u32、双精度及有符号64位边界的真实来源输出；无输入，返回无法重现来源观察的错误。
#[test]
fn wide_integer_rrf_preserves_source_results() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        SourceCase {
            parameter: 4_294_967_295,
            vector: &[1, 2, 3, 4],
            lexical: &[2, 3, 1, 4],
            order: &[2, 1, 3, 4],
            scores: &[1.0, 1.0, 0.905, 0.655],
        },
        SourceCase {
            parameter: 4_294_967_296,
            vector: &[1, 2, 3, 4],
            lexical: &[2, 3, 1, 4],
            order: &[2, 1, 3, 4],
            scores: &[1.0, 1.0, 0.905, 0.655],
        },
        SourceCase {
            parameter: 9_007_199_254_740_993,
            vector: &[1, 2, 3, 4],
            lexical: &[2, 3, 1, 4],
            order: &[1, 2, 3, 4],
            scores: &[1.0, 1.0, 0.905, 0.655],
        },
        SourceCase {
            parameter: 9_007_199_254_740_995,
            vector: &[1, 2, 3, 4],
            lexical: &[1, 3, 4, 2],
            order: &[1, 3, 2, 4],
            scores: &[1.0, 0.905, 0.905, 0.78],
        },
        SourceCase {
            parameter: 9_223_372_036_854_775_806,
            vector: &[1, 2],
            lexical: &[2, 3],
            order: &[1, 2, 3],
            scores: &[1.0, 1.0, 0.75],
        },
        SourceCase {
            parameter: 9_223_372_036_854_775_807,
            vector: &[1, 2],
            lexical: &[2, 3],
            order: &[1, 3, 2],
            scores: &[1.0, 0.833_333_333_333_333_4, 1.0],
        },
    ];
    for case in cases {
        let mut policy = RetrievalPolicy::v1();
        policy.rrf_k = case.parameter;
        let result = crate::rrf_fusion(
            &rows(case.vector, &policy.vector_contract),
            &rows(case.lexical, &policy.lexical_contract),
            &policy,
            4,
            true,
        )?;
        assert_eq!(
            result.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            case.order
                .iter()
                .map(|id| format!("memory-{id}"))
                .collect::<Vec<_>>()
        );
        for ((_, explanation), expected) in result.iter().zip(case.scores) {
            assert_eq!(explanation.final_score.to_bits(), expected.to_bits());
            let sum = explanation
                .vector_channel
                .as_ref()
                .map_or(0.0, |channel| channel.rrf_contribution)
                + explanation
                    .lexical_channel
                    .as_ref()
                    .map_or(0.0, |channel| channel.rrf_contribution);
            assert_eq!(
                explanation.rrf_score.ok_or("missing RRF sum")?.to_bits(),
                sum.to_bits()
            );
        }
    }
    Ok(())
}

/// Proves denominator profiles can change ordering while score projection remains fixed, and checks exact explanation bits.
/// 证明计分投影固定时分母配置仍可改变排序，并核对精确解释位模式；无输入，返回契约错误。
#[test]
fn rrf_math_profile_is_independent_and_explained() -> Result<(), Box<dyn std::error::Error>> {
    let mut policy = RetrievalPolicy::v1();
    policy.rrf_k = 9_007_199_254_740_993;
    let vector = rows(&[1, 2, 3, 4], &policy.vector_contract);
    let lexical = rows(&[2, 3, 1, 4], &policy.lexical_contract);
    let integer = crate::rrf_fusion(&vector, &lexical, &policy, 4, true)?;
    policy.rrf_math_profile = RrfMathProfile::Float64AddV1;
    let floating = crate::rrf_fusion(&vector, &lexical, &policy, 4, true)?;
    assert_eq!(integer[0].0, "memory-1");
    assert_eq!(floating[0].0, "memory-2");
    assert_eq!(
        integer[0]
            .1
            .vector_channel
            .as_ref()
            .ok_or("integer channel")?
            .rrf_contribution
            .to_bits(),
        0x3c9f_ffff_ffff_fffe
    );
    let memory_one = floating
        .iter()
        .find(|(id, _)| id == "memory-1")
        .ok_or("floating memory")?;
    assert_eq!(
        memory_one
            .1
            .vector_channel
            .as_ref()
            .ok_or("floating channel")?
            .rrf_contribution
            .to_bits(),
        0x3ca0_0000_0000_0000
    );
    Ok(())
}

/// Rejects parameters outside the source domain even through the direct Rust scoring entry point.
/// 即使通过直接Rust评分入口也拒绝来源值域之外的参数；无输入，断言错误分类。
#[test]
fn rrf_rejects_out_of_source_range() {
    for value in [0, i64::MAX.unsigned_abs() + 1, u64::MAX] {
        let mut policy = RetrievalPolicy::v1();
        policy.rrf_k = value;
        assert!(crate::rrf_fusion(&[], &[], &policy, 4, true).is_err());
    }
}
