//! Structured-output resource-bound regression tests.
//! 结构化输出资源边界回归测试。

use vulcan_memorymesh_core_contracts::{
    DroppedCandidate, ErrorCode, LlmOutput, LlmPurpose, validate_llm_output,
};

/// Verifies a direct typed Host cannot bypass the dropped-reason bound enforced by JSON parsing.
/// 验证直接使用强类型 Host 时无法绕过 JSON 解析所执行的丢弃原因边界。
#[test]
fn postaction_review_rejects_oversized_dropped_reason() {
    let output = LlmOutput::PostActionReview {
        accepted_candidates: Vec::new(),
        dropped_candidates: vec![DroppedCandidate {
            candidate_index: 0,
            dedupe_memory_id: Some("memory-existing".to_owned()),
            reason: Some("x".repeat(2_049)),
        }],
        accepted_profile_candidates: Vec::new(),
        invalid_profile_candidate_indexes: Vec::new(),
        retired_profile_nodes: Vec::new(),
        reason: None,
    };
    let error_code = validate_llm_output(LlmPurpose::PostActionReview, &output)
        .err()
        .map(|error| error.code);
    assert_eq!(
        error_code,
        Some(ErrorCode::StructuredOutputContractViolation)
    );
}

/// Verifies bounded provider text rejects embedded NUL even when its character count is valid.
/// 验证有界 Provider 文本即使字符数合法也会拒绝内嵌 NUL。
#[test]
fn structured_output_text_rejects_embedded_nul() {
    let output = LlmOutput::ProfileInstruction {
        accepted_nodes: Vec::new(),
        retired_nodes: Vec::new(),
        review_reason: "valid\0hidden".to_owned(),
    };
    let error_code = validate_llm_output(LlmPurpose::ProfileInstruction, &output)
        .err()
        .map(|error| error.code);
    assert_eq!(
        error_code,
        Some(ErrorCode::StructuredOutputContractViolation)
    );
}
/// Accepts every legal opaque memory identity and rejects byte-overlong review references.
/// 接受全部合法不透明记忆身份，并拒绝按字节超长的审核引用。
#[test]
fn review_references_use_the_public_identity_bound() {
    for (identity, valid) in [
        ("x".repeat(512), true),
        ("界".repeat(170), true),
        ("x".repeat(513), false),
        ("界".repeat(171), false),
    ] {
        let output = vulcan_memorymesh_core_contracts::LlmOutput::PostActionReview {
            accepted_candidates: vec![vulcan_memorymesh_core_contracts::AcceptedCandidate {
                candidate_index: 0,
                supersede_memory_ids: vec![identity],
            }],
            dropped_candidates: Vec::new(),
            accepted_profile_candidates: Vec::new(),
            invalid_profile_candidate_indexes: Vec::new(),
            retired_profile_nodes: Vec::new(),
            reason: None,
        };
        assert_eq!(
            vulcan_memorymesh_core_contracts::validate_llm_output(
                vulcan_memorymesh_core_contracts::LlmPurpose::PostActionReview,
                &output
            )
            .is_ok(),
            valid
        );
    }
}
