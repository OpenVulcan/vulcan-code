//! Regression coverage for exact no-reasoning policy boundaries.
//! 精确无思考策略边界的回归覆盖。

use super::*;

/// Verifies fixed-off routes accept only the explicit no-reasoning effort without a declared effort list.
/// 验证固定关闭路由无需声明档位列表，但只接受显式无思考档位。
#[test]
fn fixed_off_reasoning_accepts_only_none() {
    // capabilities models a route whose control mode itself is the complete authority.
    // capabilities 模拟一条由控制模式本身提供完整权威的路由。
    let capabilities = ModelCapabilities {
        reasoning_control: ReasoningControlMode::FixedOff,
        reasoning_efforts: Vec::new(),
        ..ModelCapabilities::default()
    };

    assert!(validate_internal_reasoning_effort(&capabilities, ReasoningEffort::None).is_ok());
    assert!(validate_internal_reasoning_effort(&capabilities, ReasoningEffort::Low).is_err());
}

/// Verifies VMM LLM route validation rejects models that cannot disable reasoning.
/// 验证 VMM LLM 路由校验拒绝无法关闭思考的模型。
#[test]
fn vmm_llm_route_requires_disabled_reasoning_capability() {
    let fixed_on = ModelCapabilities {
        reasoning_control: ReasoningControlMode::FixedOn,
        reasoning_efforts: vec![ReasoningEffort::High],
        ..ModelCapabilities::default()
    };
    assert!(
        validate_vmm_llm_reasoning_policy(
            VMM_CONSUMER_PROFILE_ID,
            AiOperation::ConversationRespond,
            &fixed_on,
        )
        .is_err()
    );
    assert!(
        validate_vmm_llm_reasoning_policy(
            "another-consumer",
            AiOperation::ConversationRespond,
            &fixed_on,
        )
        .is_ok()
    );
    assert!(
        validate_vmm_llm_reasoning_policy(
            VMM_CONSUMER_PROFILE_ID,
            AiOperation::EmbeddingCreate,
            &fixed_on,
        )
        .is_ok()
    );
}
