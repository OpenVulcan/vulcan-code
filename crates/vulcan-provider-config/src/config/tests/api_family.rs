use super::super::{ProviderApiFamily, ProviderRuntimeKind};

/// Verifies runtime kinds map into the expected canonical provider API families.
/// 验证运行时类型会映射到预期的规范 Provider 接口族。
#[test]
fn runtime_kind_maps_into_expected_api_family() {
    assert_eq!(
        ProviderRuntimeKind::OpenAi.api_family(),
        ProviderApiFamily::OpenAiResponses
    );
    assert_eq!(
        ProviderRuntimeKind::OpenAiResponsesCompatible.api_family(),
        ProviderApiFamily::OpenAiResponses
    );
    assert_eq!(
        ProviderRuntimeKind::OpenAiCompatible.api_family(),
        ProviderApiFamily::OpenAiChatCompatible
    );
    assert_eq!(
        ProviderRuntimeKind::OpenRouter.api_family(),
        ProviderApiFamily::OpenAiChatCompatible
    );
    assert_eq!(
        ProviderRuntimeKind::Anthropic.api_family(),
        ProviderApiFamily::AnthropicMessages
    );
    assert_eq!(
        ProviderRuntimeKind::GoogleGemini.api_family(),
        ProviderApiFamily::GoogleGeminiGenerateContent
    );
    assert_eq!(
        ProviderRuntimeKind::Cohere.api_family(),
        ProviderApiFamily::CohereChat
    );
}

/// Verifies canonical provider API families expose stable labels and keys for diagnostics surfaces.
/// 验证规范 Provider 接口族会暴露稳定的标签与键值，供诊断界面复用。
#[test]
fn provider_api_family_exposes_stable_label_and_key() {
    assert_eq!(ProviderApiFamily::OpenAiResponses.key(), "openai-responses");
    assert_eq!(
        ProviderApiFamily::OpenAiResponses.label(),
        "OpenAI Responses"
    );
    assert_eq!(
        ProviderApiFamily::AnthropicMessages.key(),
        "anthropic-messages"
    );
    assert_eq!(
        ProviderApiFamily::AnthropicMessages.label(),
        "Anthropic Messages"
    );
    assert_eq!(
        ProviderApiFamily::GoogleGeminiGenerateContent.key(),
        "google-gemini-generate-content"
    );
}
