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

/// Verifies every VMM LLM purpose requires an explicitly disabled reasoning route.
/// 验证每个 VMM LLM 用途都要求明确关闭思考的路由。
#[test]
fn vmm_llm_route_enforces_disabled_reasoning_for_every_purpose() {
    const VMM_LLM_PURPOSE_IDS: [&str; 6] = [
        "precheck_l1",
        "precheck_l2",
        "postaction_l1",
        "postaction_l2",
        "profile_instruction",
        "reserve",
    ];
    let fixed_on = ModelCapabilities {
        reasoning_control: ReasoningControlMode::FixedOn,
        reasoning_efforts: vec![ReasoningEffort::High],
        ..ModelCapabilities::default()
    };
    let fixed_off = ModelCapabilities {
        reasoning_control: ReasoningControlMode::FixedOff,
        ..ModelCapabilities::default()
    };
    for purpose_id in VMM_LLM_PURPOSE_IDS {
        assert!(
            validate_vmm_llm_reasoning_policy(
                VMM_CONSUMER_PROFILE_ID,
                Some(purpose_id),
                AiOperation::ConversationRespond,
                &fixed_on,
                Some(ReasoningEffort::High),
            )
            .is_err(),
            "VMM purpose `{purpose_id}` must reject a route with mandatory reasoning"
        );
        assert!(
            validate_vmm_llm_reasoning_policy(
                VMM_CONSUMER_PROFILE_ID,
                Some(purpose_id),
                AiOperation::ConversationRespond,
                &fixed_off,
                Some(ReasoningEffort::None),
            )
            .is_ok(),
            "VMM purpose `{purpose_id}` must accept an explicitly disabled reasoning route"
        );
    }
    assert!(
        validate_vmm_llm_reasoning_policy(
            "another-consumer",
            Some("any-purpose"),
            AiOperation::ConversationRespond,
            &fixed_on,
            None,
        )
        .is_ok()
    );
    assert!(
        validate_vmm_llm_reasoning_policy(
            VMM_CONSUMER_PROFILE_ID,
            None,
            AiOperation::EmbeddingCreate,
            &fixed_on,
            None,
        )
        .is_ok()
    );
}
