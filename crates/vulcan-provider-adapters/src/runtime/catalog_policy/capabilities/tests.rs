//! Reasoning defaults and explicit protocol-evidence regressions.
//! 思考默认值与显式协议证据回归测试。

use super::super::test_support::apply_model_catalog_overlays;
use super::*;
use crate::runtime::test_support::catalog;

/// Ensures transcription skips chat limits while mixed conversational models stay guarded.
/// 确保转写跳过会话限制，同时保留混合会话模型的校验。
#[test]
fn transcription_does_not_use_conversation_token_limits() {
    // Exercise both discovered modality evidence and an explicitly declared operation.
    // 同时覆盖发现模态证据与显式操作声明两种来源。
    for (operations, evidence) in [
        (
            Vec::new(),
            ModelCapabilityEvidence {
                supports_text_input: Some(false),
                supports_text_output: Some(true),
                ..ModelCapabilityEvidence::default()
            },
        ),
        (
            vec![AiOperation::SpeechTranscribe],
            ModelCapabilityEvidence::default(),
        ),
    ] {
        // A positive output limit must survive an irrelevant zero context window unchanged.
        // 有效输出上限必须保持原值，不被无关的零上下文窗口收敛。
        let mut capabilities = ModelCapabilities {
            operations,
            context_limit: Some(0),
            output_limit: Some(4096),
            ..ModelCapabilities::default()
        };
        clamp_output_limit_to_context_window(&mut capabilities, &evidence);
        assert_eq!(capabilities.output_limit, Some(4096));
        assert!(
            validate_resolved_model_capability_limits(
                "one",
                "transcribe",
                &capabilities,
                &evidence
            )
            .is_ok()
        );
        capabilities.output_limit = Some(0);
        assert!(
            validate_resolved_model_capability_limits(
                "one",
                "transcribe",
                &capabilities,
                &evidence
            )
            .is_ok()
        );
        capabilities
            .operations
            .push(AiOperation::ConversationRespond);
        assert!(
            validate_resolved_model_capability_limits("one", "mixed", &capabilities, &evidence)
                .is_err()
        );
    }
}

/// Verifies zero token metadata is irrelevant only to explicitly media-only output.
/// 验证零令牌元数据仅对明确的纯媒体输出无关。
#[test]
fn zero_token_limits_only_exempt_media_output() {
    for resource in [
        vulcan_ai_contract::ResourceKind::Image,
        vulcan_ai_contract::ResourceKind::Audio,
        vulcan_ai_contract::ResourceKind::Video,
    ] {
        // evidence records the upstream's explicit absence of text output.
        // evidence 记录上游明确不输出文本的事实。
        let evidence = ModelCapabilityEvidence {
            supports_text_output: Some(false),
            ..ModelCapabilityEvidence::default()
        };
        let mut capabilities = ModelCapabilities {
            context_limit: Some(0),
            output_limit: Some(0),
            output_resources: vec![resource],
            ..ModelCapabilities::default()
        };
        assert!(
            validate_resolved_model_capability_limits("one", "media", &capabilities, &evidence)
                .is_ok()
        );
        clamp_output_limit_to_context_window(&mut capabilities, &evidence);
        assert_eq!(capabilities.output_limit, Some(0));
        capabilities
            .operations
            .push(AiOperation::ConversationRespond);
        assert!(
            validate_resolved_model_capability_limits("one", "mixed", &capabilities, &evidence)
                .is_err()
        );
    }

    let unresolved = ModelCapabilities {
        context_limit: Some(0),
        ..ModelCapabilities::default()
    };
    assert!(
        validate_resolved_model_capability_limits(
            "one",
            "unknown",
            &unresolved,
            &ModelCapabilityEvidence::default(),
        )
        .is_err()
    );
    let text = ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        output_limit: Some(0),
        ..ModelCapabilities::default()
    };
    assert!(
        validate_resolved_model_capability_limits(
            "one",
            "text",
            &text,
            &ModelCapabilityEvidence::default(),
        )
        .is_err()
    );

    let mixed_output = ModelCapabilities {
        context_limit: Some(0),
        output_resources: vec![vulcan_ai_contract::ResourceKind::Image],
        ..ModelCapabilities::default()
    };
    let mixed_evidence = ModelCapabilityEvidence {
        supports_text_output: Some(true),
        ..ModelCapabilityEvidence::default()
    };
    assert!(
        validate_resolved_model_capability_limits(
            "one",
            "mixed-output",
            &mixed_output,
            &mixed_evidence,
        )
        .is_err()
    );

    let declared_media = ModelCapabilities {
        operations: vec![AiOperation::VideoGenerate],
        context_limit: Some(0),
        output_resources: vec![vulcan_ai_contract::ResourceKind::Video],
        ..ModelCapabilities::default()
    };
    assert!(
        validate_resolved_model_capability_limits(
            "one",
            "declared-media",
            &declared_media,
            &ModelCapabilityEvidence::default(),
        )
        .is_ok()
    );
}

/// Verifies unknown text models use the host's enabled reasoning default in public capability views.
/// 验证未知文本模型在公共能力视图中使用宿主默认的开启思考策略。
#[test]
fn unknown_text_model_reasoning_is_enabled_by_default() {
    let capabilities = ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        reasoning: CapabilitySupport::Unknown,
        reasoning_control: ReasoningControlMode::Unknown,
        reasoning_efforts: Vec::new(),
        ..ModelCapabilities::default()
    };

    assert!(model_supports_reasoning(&capabilities));

    // The discovery fixture intentionally omits reasoning metadata, matching the real
    // `/models` payload that previously surfaced as Unsupported/FixedOff.
    // 发现夹具故意省略思考元数据，模拟此前会被显示为 Unsupported/FixedOff 的真实 `/models` 响应。
    let discovered = catalog("one")
        .models
        .into_iter()
        .next()
        .expect("model fixture should exist");
    assert!(model_supports_reasoning(&discovered.capabilities));
}

/// Verifies an explicit fixed-off contract remains authoritative over the host default.
/// 验证明确固定关闭契约仍然高于宿主默认策略。
#[test]
fn explicit_fixed_off_reasoning_remains_disabled() {
    let mut capabilities = catalog("one")
        .models
        .into_iter()
        .next()
        .expect("model fixture should exist")
        .capabilities;
    capabilities.reasoning = CapabilitySupport::Unsupported;
    capabilities.reasoning_control = ReasoningControlMode::FixedOff;
    capabilities.reasoning_efforts = vec![vulcan_ai_contract::ReasoningEffort::None];

    assert!(!model_supports_reasoning(&capabilities));
}

/// Verifies a fixed-off declaration without stale effort entries is still authoritative.
/// 验证即使没有残留档位，固定关闭声明仍然具有权威性。
#[test]
fn explicit_fixed_off_without_efforts_remains_disabled() {
    let capabilities = ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        reasoning: CapabilitySupport::Unsupported,
        reasoning_control: ReasoningControlMode::FixedOff,
        ..ModelCapabilities::default()
    };

    assert!(!model_supports_reasoning(&capabilities));
}

/// Verifies a custom Gemini variant does not inherit OpenAI-only reasoning levels.
/// 验证自定义 Gemini 变体不会继承 OpenAI 专属思考档位。
#[test]
fn custom_gemini_system_baseline_preserves_protocol_reasoning_evidence() {
    // baseline consumes the exact variant protocol and no connection-level compatibility field.
    // baseline 消费精确变体协议，不读取任何连接级兼容字段。
    let baseline = custom_provider_system_baseline_for_protocol(Some(
        vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
    ));

    assert_eq!(baseline.reasoning, CapabilitySupport::Unknown);
    assert_eq!(baseline.reasoning_control, ReasoningControlMode::Unknown);
    assert!(baseline.reasoning_efforts.is_empty());
}

/// Verifies unknown compatible text models receive the host default reasoning contract.
/// 验证未知兼容文本模型会获得宿主默认思考契约。
#[test]
fn unknown_compatible_conversation_defaults_to_enabled_reasoning() {
    let configuration = PersistedAiConfiguration::default();
    let mut catalog = crate::parse_openai_model_catalog(
        "apikeyfan-account",
        1,
        r#"{"data":[{"id":"gpt-5.6-luna"}]}"#,
    )
    .expect("compatible source catalog should parse");

    apply_model_catalog_overlays(&configuration, "apikeyfan", &mut catalog)
        .expect("host reasoning defaults should resolve");

    let model = catalog.models.first().expect("model should remain visible");
    assert_eq!(model.capabilities.reasoning, CapabilitySupport::Native);
    assert_eq!(
        model.capabilities.reasoning_control,
        ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        model.capabilities.reasoning_efforts,
        crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS
    );
    assert!(
        model
            .capabilities
            .evidence_revision
            .contains(HOST_DEFAULT_REASONING_EVIDENCE_REVISION)
    );
}

/// Ensures Anthropic Messages routes gain managed caching while other protocols stay unchanged.
/// 确保 Anthropic Messages 路由获得托管缓存能力，其他协议保持不变。
#[test]
fn anthropic_messages_routes_gain_managed_prompt_cache() {
    let mut anthropic = ModelCapabilities::default();
    apply_protocol_prompt_cache_capability("anthropic-messages.v1", &mut anthropic);
    assert!(anthropic.prompt_cache.supports_managed_explicit);
    assert_eq!(
        anthropic
            .prompt_cache
            .profile_default_ttl_option_id
            .as_deref(),
        Some("5m")
    );
    let option_ids: Vec<&str> = anthropic
        .prompt_cache
        .supported_ttl_options
        .iter()
        .map(|option| option.id.as_str())
        .collect();
    assert_eq!(option_ids, ["5m", "1h"]);

    let mut chat = ModelCapabilities::default();
    apply_protocol_prompt_cache_capability("openai-chat-completions.v1", &mut chat);
    assert_eq!(chat.prompt_cache, ModelCapabilities::default().prompt_cache);
}
