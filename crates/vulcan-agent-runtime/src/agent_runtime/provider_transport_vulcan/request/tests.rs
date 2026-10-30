//! Regression tests for the request boundary.
//! request边界的回归测试。

use super::*;
use vulcan_core::types::{ConversationPayload as CoreConversationPayload, ProviderContinuation};

/// Verifies the closed configuration enum maps directly onto the three shared projection profiles.
/// 验证封闭配置枚举直接映射到三项共享投影配置。
#[test]
fn configured_tool_schema_projection_modes_map_without_heuristics() {
    for (mode, expected) in [
        (
            ToolSchemaProjectionMode::Lightweight,
            VulcanToolSchemaProjectionProfile::StandardObjectRoot,
        ),
        (
            ToolSchemaProjectionMode::Passthrough,
            VulcanToolSchemaProjectionProfile::PassThrough,
        ),
        (
            ToolSchemaProjectionMode::Strict,
            VulcanToolSchemaProjectionProfile::GeminiSlim,
        ),
    ] {
        assert_eq!(configured_tool_schema_projection_profile(mode), expected);
    }
}

/// Verifies a cursor produced by another protocol profile is never relabeled and reused.
/// 验证由其他协议配置产生的游标绝不会被重新标记并复用。
#[test]
fn continuation_rejects_protocol_profile_switch() {
    let config = CoreControllerConfig {
        provider_id: String::from("minimax"),
        model_id: String::from("MiniMax-M3"),
        system_prompt: String::from("system"),
        ..CoreControllerConfig::default()
    };
    let snapshot = CoreConversationSnapshot {
        messages: vec![vulcan_core::types::ConversationMessage {
            id: String::from("assistant-1"),
            payload: CoreConversationPayload::Assistant {
                text: String::from("answer"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("response-1")),
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        }],
        continuation: ProviderContinuation {
            provider_id: Some(String::from("minimax")),
            protocol_profile_id: Some(String::from(vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID)),
            model_id: Some(String::from("MiniMax-M3")),
            last_provider_message_id: Some(String::from("response-1")),
            last_local_message_id: Some(String::from("assistant-1")),
            context_rewrite_fingerprint: Some(config.provider_rewrite_fingerprint()),
            ..ProviderContinuation::default()
        },
        ..CoreConversationSnapshot::default()
    };

    // A matching route is the positive control: rejection must be caused by the protocol switch.
    // 匹配路由是正向对照：拒绝必须由协议切换引起。
    assert!(
        build_public_continuation(
            &snapshot,
            &config,
            "minimax",
            vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
            "MiniMax-M3",
            1,
        )
        .is_some()
    );
    assert!(
        build_public_continuation(
            &snapshot,
            &config,
            "minimax",
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            "MiniMax-M3",
            1,
        )
        .is_none()
    );
}
