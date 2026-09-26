//! Regression coverage for exact protocol-projected media candidate capabilities.
//! 精确协议投影后媒体候选能力的回归覆盖。

use super::*;

/// Verifies replacement candidates expose only media inputs encoded by the frozen protocol.
/// 验证模型代替候选只公开冻结协议能够编码的媒体输入。
#[test]
fn media_candidate_capabilities_intersect_model_and_protocol_support() {
    // capabilities deliberately over-declares every media kind so protocol projection is observable.
    // capabilities 有意声明全部媒体类型，使协议投影结果可观察。
    let capabilities = ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        input_resources: vec![
            ResourceKind::Image,
            ResourceKind::Video,
            ResourceKind::Audio,
            ResourceKind::File,
        ],
        ..ModelCapabilities::default()
    };

    // responses retains only its registered image projector and preserves unrelated file metadata.
    // responses 仅保留已注册图片编码器，并保持无关文件元数据。
    let responses = protocol_projected_media_capabilities(
        &capabilities,
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
    );
    assert_eq!(
        responses.input_resources,
        vec![ResourceKind::Image, ResourceKind::File]
    );
    // chat exposes all three media encoders used by model replacement.
    // chat 公开模型代替所需的三种媒体编码器。
    let chat = protocol_projected_media_capabilities(
        &capabilities,
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
    );
    assert_eq!(chat.input_resources, capabilities.input_resources);
}

/// Verifies the explicit Anthropic video profile cannot accidentally admit audio replacement.
/// 验证显式 Anthropic 视频协议不会意外准入音频模型代替。
#[test]
fn anthropic_video_candidate_excludes_unregistered_audio_input() {
    // capabilities represents a multimodal model whose wire route is narrower than its catalog metadata.
    // capabilities 表示 Wire 路由窄于目录元数据的多模态模型。
    let capabilities = ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        input_resources: vec![
            ResourceKind::Image,
            ResourceKind::Video,
            ResourceKind::Audio,
        ],
        structured_output: StructuredOutputCapability::JsonSchema,
        ..ModelCapabilities::default()
    };

    let projected = protocol_projected_media_capabilities(
        &capabilities,
        vulcan_llm_core::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    );
    assert_eq!(
        projected.input_resources,
        vec![ResourceKind::Image, ResourceKind::Video]
    );
    assert!(model_route_supports_media_understanding(
        &projected,
        ProviderToolCapabilityKind::VideoUnderstand,
    ));
    assert!(!model_route_supports_media_understanding(
        &projected,
        ProviderToolCapabilityKind::AudioUnderstand,
    ));
}

/// Verifies a media route remains eligible when its protocol requires host-wrapped plain text.
/// 验证协议需要宿主包装纯文本时，媒体路线仍保持可选。
#[test]
fn anthropic_candidate_can_use_host_wrapped_plain_text() {
    // capabilities uses the product default so the protocol projection must close the mismatch.
    // capabilities 使用产品默认值，因此协议投影必须关闭这一不匹配。
    let capabilities = ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        input_resources: vec![ResourceKind::Image],
        ..ModelCapabilities::default()
    };

    let projected = protocol_projected_media_capabilities(
        &capabilities,
        vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
    );
    assert_eq!(
        projected.structured_output,
        StructuredOutputCapability::Unsupported
    );
    assert!(model_route_supports_media_understanding(
        &projected,
        ProviderToolCapabilityKind::ImageUnderstand,
    ));
}
