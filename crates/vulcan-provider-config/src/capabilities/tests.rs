//! Capability-resolution regression tests covering credential fallback and model field normalization.
//! 覆盖凭据回退与模型字段规范化的能力解析回归测试。

use super::{
    ResolvedModelCapabilities, apply_ai_tool_runtime_configuration,
    resolve_credential_source_summary, resolve_effective_model_capabilities,
    resolve_model_capabilities, resolve_provider_api_key, resolve_provider_api_key_with_lookup,
};
use crate::provider_config::{
    AiToolDeclaration, AiToolKind, AiToolPolicy, ModelChoice, PersistedAiToolConfiguration,
    PersistedProviderOptions, PersistedUnderstandImageConfiguration,
    PersistedWebSearchConfiguration, ProviderBaseUrlConfig, ProviderBaseUrlKind,
    ProviderModelDiscoveryConfig, ProviderModelDiscoveryKind, ProviderRuntimeKind,
    SupportedProviderDefinition, UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID,
    WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID, WebSearchRoutingMode,
};
use std::collections::BTreeMap;

/// Builds one minimal provider definition for capability-resolution tests.
/// 为能力解析测试构建一份最小 Provider 定义。
fn test_provider_definition() -> SupportedProviderDefinition {
    SupportedProviderDefinition {
        id: String::from("deepseek"),
        name: String::from("DeepSeek"),
        runtime: ProviderRuntimeKind::OpenAiCompatible,
        user_agent: None,
        credential_env_keys: vec![String::from("DEEPSEEK_API_KEY")],
        extra_headers: BTreeMap::new(),
        extra_body: serde_json::Map::new(),
        usage_capture: crate::provider_config::ProviderUsageCaptureConfig::default(),
        model_runtime_overrides: BTreeMap::new(),
        reasoning_profile: None,
        ai_tools: Vec::new(),
        fields: Vec::new(),
        base_url: ProviderBaseUrlConfig {
            kind: ProviderBaseUrlKind::Fixed,
            allow_override: false,
            value: Some(String::from("https://api.example.test")),
            template: None,
            parameter: None,
        },
        model_discovery: ProviderModelDiscoveryConfig {
            kind: ProviderModelDiscoveryKind::OpenAiModels,
        },
        models: Vec::new(),
    }
}

/// Preserves declared model flags and normalizes modality strings into stable lowercase host values.
/// 保留声明的模型能力标记，并把模态字符串规范化为稳定的小写宿主值。
#[test]
fn resolve_model_capabilities_preserves_flags_and_normalizes_modalities() {
    let definition = test_provider_definition();
    let model = ModelChoice {
        id: String::from("deepseek-v4-pro"),
        name: String::from("DeepSeek V4 Pro"),
        default_enabled: false,
        status: Some(String::from("stable")),
        family: Some(String::from("thinking")),
        release_date: Some(String::from("2026-04-24")),
        context_limit: Some(128_000),
        input_limit: Some(96_000),
        output_limit: Some(8_192),
        reasoning: Some(true),
        tool_call: Some(false),
        temperature: Some(false),
        attachment: Some(true),
        interleaved: None,
        reasoning_profile: None,
        ai_tools: Vec::new(),
        input_modalities: vec![
            String::from("Text"),
            String::from(" Image "),
            String::from("text"),
        ],
        output_modalities: vec![String::from("Text"), String::from("text")],
    };

    let capabilities = resolve_model_capabilities(&definition, Some(&model));

    assert!(capabilities.supports_reasoning);
    assert!(!capabilities.supports_tool_call);
    assert!(!capabilities.supports_temperature);
    assert!(capabilities.supports_attachment);
    assert_eq!(capabilities.context_limit, Some(128_000));
    assert_eq!(capabilities.input_modalities, vec!["text", "image"]);
    assert_eq!(capabilities.output_modalities, vec!["text"]);
    assert!(capabilities.supports_input_modality("text"));
    assert!(capabilities.supports_input_modality("image"));
    assert!(!capabilities.supports_input_modality("video"));
    assert!(
        capabilities
            .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
    );
}

/// Treats `attachment` as catalog metadata while using input modalities as the host-side attachment gate.
/// 将 `attachment` 视为目录元数据，同时使用输入模态作为宿主侧附件门控依据。
#[test]
fn resolve_model_capabilities_uses_modalities_as_attachment_gate() {
    let definition = test_provider_definition();
    let model = ModelChoice {
        id: String::from("qwen3.6-plus"),
        name: String::from("Qwen3.6 Plus"),
        default_enabled: false,
        status: None,
        family: None,
        release_date: None,
        context_limit: None,
        input_limit: None,
        output_limit: None,
        reasoning: Some(true),
        tool_call: Some(true),
        temperature: Some(true),
        attachment: Some(false),
        interleaved: None,
        reasoning_profile: None,
        ai_tools: Vec::new(),
        input_modalities: vec![
            String::from("text"),
            String::from("image"),
            String::from("video"),
        ],
        output_modalities: vec![String::from("text")],
    };

    let capabilities = resolve_model_capabilities(&definition, Some(&model));

    assert!(!capabilities.supports_attachment);
    assert!(
        capabilities
            .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
    );
    assert!(
        capabilities
            .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Video)
    );
    assert!(
        !capabilities
            .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Audio)
    );
}

/// Falls back to provider-declared environment variables when no local API-key field is persisted.
/// 当本地未持久化 API Key 字段时，回退到供应商声明的环境变量。
#[test]
fn resolve_provider_api_key_falls_back_to_provider_declared_env_key() {
    let definition = test_provider_definition();
    let capabilities = resolve_model_capabilities(&definition, None);
    let options = PersistedProviderOptions::default();
    let api_key = resolve_provider_api_key_with_lookup(&options, &capabilities, |env_name| {
        Ok((env_name == "DEEPSEEK_API_KEY").then(|| String::from("fallback-key")))
    })
    .expect("credential fallback");

    let public_api_key = resolve_provider_api_key(&options, &ResolvedModelCapabilities::default())
        .expect("public resolver stays non-fatal without fallback env");

    assert_eq!(public_api_key, None);
    assert_eq!(api_key.as_deref(), Some("fallback-key"));
}

/// Surfaces provider-declared fallback keys in the credential summary when no explicit local secret is configured.
/// 当未配置显式本地密钥时，在凭据摘要中展示供应商声明的回退环境变量键。
#[test]
fn resolve_credential_source_summary_surfaces_fallback_env_keys() {
    let definition = test_provider_definition();
    let capabilities = resolve_model_capabilities(&definition, None);
    let options = PersistedProviderOptions::default();

    let summary = resolve_credential_source_summary(&options, &capabilities);

    assert!(summary.contains("DEEPSEEK_API_KEY"));
}

/// Infers provider-native web search for APIs that expose dedicated server-side search tools.
/// 会为具备专用服务端搜索工具的接口族推导 Provider 原生网页搜索能力。
#[test]
fn resolve_model_capabilities_infers_native_web_search_for_openai_responses() {
    let mut definition = test_provider_definition();
    definition.id = String::from("openai");
    definition.runtime = ProviderRuntimeKind::OpenAi;
    let model = ModelChoice {
        id: String::from("gpt-5"),
        name: String::from("GPT-5"),
        default_enabled: true,
        status: None,
        family: None,
        release_date: None,
        context_limit: None,
        input_limit: None,
        output_limit: None,
        reasoning: None,
        tool_call: Some(true),
        temperature: None,
        attachment: None,
        interleaved: None,
        reasoning_profile: None,
        ai_tools: Vec::new(),
        input_modalities: vec![String::from("text")],
        output_modalities: vec![String::from("text")],
    };

    let capabilities = resolve_model_capabilities(&definition, Some(&model));
    let web_search = capabilities
        .ai_tools
        .iter()
        .find(|tool| tool.kind == AiToolKind::WebSearch)
        .expect("OpenAI Responses should infer web search");

    assert!(web_search.native_provider_tool);
    assert_eq!(
        web_search.native_tool_type.as_deref(),
        Some("web_search_preview")
    );
}

/// Does not infer provider-native web search when the model entry is missing.
/// 模型条目缺失时不推导 Provider 原生网页搜索能力。
#[test]
fn resolve_model_capabilities_does_not_infer_native_web_search_without_model() {
    let mut definition = test_provider_definition();
    definition.id = String::from("openai");
    definition.runtime = ProviderRuntimeKind::OpenAi;

    let capabilities = resolve_model_capabilities(&definition, None);

    assert!(
        capabilities
            .ai_tools
            .iter()
            .all(|tool| tool.kind != AiToolKind::WebSearch)
    );
}

/// Does not infer Google Gemini native search because it cannot be combined with coding function tools.
/// 不推导 Google Gemini 原生搜索，因为它无法与编码函数工具组合使用。
#[test]
fn resolve_model_capabilities_does_not_infer_native_web_search_for_google_gemini() {
    let mut definition = test_provider_definition();
    definition.id = String::from("google");
    definition.runtime = ProviderRuntimeKind::GoogleGemini;

    let capabilities = resolve_model_capabilities(&definition, None);

    assert!(
        capabilities
            .ai_tools
            .iter()
            .all(|tool| tool.kind != AiToolKind::WebSearch)
    );
}

/// Does not infer provider-native hosted tools for third-party compatible providers without explicit catalog declarations.
/// 对未显式声明目录能力的三方兼容供应商，不会自动推导 Provider 原生 Hosted Tool。
#[test]
fn resolve_model_capabilities_does_not_infer_native_web_search_for_compatible_proxy() {
    let mut definition = test_provider_definition();
    definition.id = String::from("vivgrid");
    definition.runtime = ProviderRuntimeKind::OpenAi;

    let capabilities = resolve_model_capabilities(&definition, None);

    assert!(
        capabilities
            .ai_tools
            .iter()
            .all(|tool| tool.kind != AiToolKind::WebSearch)
    );
}

/// Configures host web-search fallback when the active model lacks provider-native search.
/// 当激活模型缺少 Provider 原生搜索时，配置宿主侧网页搜索回退。
#[test]
fn ai_tool_configuration_adds_host_web_search_for_provider_chain() {
    let definition = test_provider_definition();
    let mut capabilities = resolve_model_capabilities(&definition, None);
    let configuration = PersistedAiToolConfiguration {
        vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
            id: String::from("brave"),
            vendor_id: String::from("brave"),
            name: String::from("Brave Search"),
            enabled: true,
            api_key: String::from("token"),
            connection_mode: None,
            base_url: String::new(),
            region: String::new(),
            targets: vec![
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::WebSearch,
                    "brave",
                    "brave",
                ),
            ],
        }],
        web_search: PersistedWebSearchConfiguration {
            mode: WebSearchRoutingMode::ThirdPartyOnly,
            target_order: vec![String::from("brave/brave")],
            dispatch_mode: crate::provider_config::WebSearchProviderDispatchMode::default(),
            max_results: 5,
            providers: Vec::new(),
        },
        ..PersistedAiToolConfiguration::default()
    };

    apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

    let web_search = capabilities
        .ai_tools
        .iter()
        .find(|tool| tool.kind == AiToolKind::WebSearch)
        .expect("configured provider chain should expose host web search");
    assert_eq!(web_search.policy, AiToolPolicy::ForceHost);
    assert_eq!(
        web_search.fallback_tool_id.as_deref(),
        Some(WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID)
    );
    assert!(!web_search.native_provider_tool);
}

/// Keeps provider-native search as the fallback when provider-first mode has no usable external provider.
/// 当外部供应商链不可用时，在外部优先模式下保留 Provider 原生搜索作为兜底。
#[test]
fn ai_tool_configuration_uses_native_fallback_when_provider_chain_is_missing() {
    let mut definition = test_provider_definition();
    definition.id = String::from("openai");
    definition.runtime = ProviderRuntimeKind::OpenAi;
    let model = ModelChoice {
        id: String::from("gpt-5"),
        name: String::from("GPT-5"),
        default_enabled: true,
        status: None,
        family: None,
        release_date: None,
        context_limit: None,
        input_limit: None,
        output_limit: None,
        reasoning: None,
        tool_call: Some(true),
        temperature: None,
        attachment: None,
        interleaved: None,
        reasoning_profile: None,
        ai_tools: Vec::new(),
        input_modalities: vec![String::from("text")],
        output_modalities: vec![String::from("text")],
    };
    let mut capabilities = resolve_model_capabilities(&definition, Some(&model));
    let configuration = PersistedAiToolConfiguration {
        web_search: PersistedWebSearchConfiguration {
            mode: WebSearchRoutingMode::ThirdPartyFirst,
            target_order: Vec::new(),
            providers: Vec::new(),
            dispatch_mode: crate::provider_config::WebSearchProviderDispatchMode::default(),
            max_results: 5,
        },
        ..PersistedAiToolConfiguration::default()
    };

    apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

    let web_search = capabilities
        .ai_tools
        .iter()
        .find(|tool| tool.kind == AiToolKind::WebSearch)
        .expect("native web search should still be available");
    assert_eq!(web_search.policy, AiToolPolicy::ForceNative);
    assert!(web_search.fallback_tool_id.is_none());
}

/// Adds host image-understanding fallback when the selected model lacks native image input but the standalone capability layer is configured.
/// 当所选模型缺少原生图片输入且已配置独立能力层时，添加宿主侧图片理解回退。
#[test]
fn ai_tool_configuration_adds_host_understand_image_for_provider_chain() {
    let definition = test_provider_definition();
    let mut capabilities = resolve_model_capabilities(&definition, None);
    let configuration = PersistedAiToolConfiguration {
        vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
            id: String::from("minimax"),
            vendor_id: String::from("minimax"),
            name: String::from("MiniMax"),
            enabled: true,
            api_key: String::from("token"),
            connection_mode: None,
            base_url: String::new(),
            region: String::from("global"),
            targets: vec![
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::UnderstandImage,
                    "minimax",
                    "minimax",
                ),
            ],
        }],
        understand_image: PersistedUnderstandImageConfiguration {
            target_order: vec![String::from("minimax/minimax")],
            providers: Vec::new(),
        },
        ..PersistedAiToolConfiguration::default()
    };

    apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

    let understand_image = capabilities
        .ai_tools
        .iter()
        .find(|tool| tool.kind == AiToolKind::UnderstandImage)
        .expect("configured provider chain should expose host image understanding");
    assert_eq!(understand_image.policy, AiToolPolicy::ForceHost);
    assert_eq!(
        understand_image.fallback_tool_id.as_deref(),
        Some(UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID)
    );
    assert!(!understand_image.native_provider_tool);
    assert!(
        capabilities
            .allows_staged_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
    );
}

/// Resolves one shared effective capability snapshot that already includes user-configured host AI-tool overlays.
/// 解析一份已经包含用户配置宿主 AI 工具覆盖的共享生效能力快照。
#[test]
fn resolve_effective_model_capabilities_applies_host_ai_tool_overlays() {
    let definition = test_provider_definition();
    let configuration = PersistedAiToolConfiguration {
        vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
            id: String::from("minimax"),
            vendor_id: String::from("minimax"),
            name: String::from("MiniMax"),
            enabled: true,
            api_key: String::from("token"),
            connection_mode: None,
            base_url: String::new(),
            region: String::from("global"),
            targets: vec![
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::UnderstandImage,
                    "minimax",
                    "minimax",
                ),
            ],
        }],
        understand_image: PersistedUnderstandImageConfiguration {
            target_order: vec![String::from("minimax/minimax")],
            providers: Vec::new(),
        },
        ..PersistedAiToolConfiguration::default()
    };

    let capabilities = resolve_effective_model_capabilities(&definition, None, &configuration);

    assert!(
        capabilities
            .ai_tools
            .iter()
            .any(|tool| tool.kind == AiToolKind::UnderstandImage)
    );
    assert!(
        capabilities
            .allows_staged_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
    );
}

/// Removes host image-understanding fallback when the active model already supports native image input.
/// 当当前模型已经支持原生图片输入时，移除宿主侧图片理解回退。
#[test]
fn ai_tool_configuration_omits_understand_image_for_native_vision_models() {
    let definition = test_provider_definition();
    let model = ModelChoice {
        id: String::from("vision-model"),
        name: String::from("Vision Model"),
        default_enabled: false,
        input_modalities: vec![String::from("text"), String::from("image")],
        output_modalities: vec![String::from("text")],
        status: None,
        family: None,
        release_date: None,
        context_limit: None,
        input_limit: None,
        output_limit: None,
        reasoning: Some(true),
        tool_call: Some(true),
        temperature: Some(true),
        attachment: Some(true),
        interleaved: None,
        reasoning_profile: None,
        ai_tools: Vec::new(),
    };
    let mut capabilities = resolve_model_capabilities(&definition, Some(&model));
    let configuration = PersistedAiToolConfiguration {
        vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
            id: String::from("minimax"),
            vendor_id: String::from("minimax"),
            name: String::from("MiniMax"),
            enabled: true,
            api_key: String::from("token"),
            connection_mode: None,
            base_url: String::new(),
            region: String::from("global"),
            targets: vec![
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::UnderstandImage,
                    "minimax",
                    "minimax",
                ),
            ],
        }],
        understand_image: PersistedUnderstandImageConfiguration {
            target_order: vec![String::from("minimax/minimax")],
            providers: Vec::new(),
        },
        ..PersistedAiToolConfiguration::default()
    };

    apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

    assert!(
        capabilities
            .ai_tools
            .iter()
            .all(|tool| tool.kind != AiToolKind::UnderstandImage)
    );
    assert!(
        capabilities
            .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
    );
}

/// Adds host-only MiniMax generation capabilities and unlocks bridgeable image/audio attachments for the matching semantic tools.
/// 添加宿主侧 MiniMax 生成类能力，并为对应语义工具放开可桥接的图片与音频附件。
#[test]
fn ai_tool_configuration_adds_host_generation_capabilities_for_configured_targets() {
    let definition = test_provider_definition();
    let mut capabilities = resolve_model_capabilities(&definition, None);
    let configuration = PersistedAiToolConfiguration {
        vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
            id: String::from("minimax"),
            vendor_id: String::from("minimax"),
            name: String::from("MiniMax"),
            enabled: true,
            api_key: String::from("token"),
            connection_mode: None,
            base_url: String::new(),
            region: String::from("global"),
            targets: vec![
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::ImageGeneration,
                    "image-01",
                    "image-01",
                ),
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::VideoGeneration,
                    "MiniMax-Hailuo-2.3",
                    "MiniMax-Hailuo-2.3",
                ),
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::SpeechSynthesis,
                    "speech-2.8-hd",
                    "speech-2.8-hd",
                ),
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::MusicGeneration,
                    "music-2.6",
                    "music-2.6",
                ),
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::MusicCover,
                    "music-cover",
                    "music-cover",
                ),
            ],
        }],
        ..PersistedAiToolConfiguration::default()
    };

    apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

    let image_generation = capabilities
        .ai_tools
        .iter()
        .find(|tool| tool.kind == AiToolKind::ImageGeneration)
        .expect("configured image generation should be exposed");
    assert_eq!(image_generation.policy, AiToolPolicy::ForceHost);
    assert_eq!(
        image_generation.fallback_tool_id.as_deref(),
        Some(crate::provider_config::IMAGE_GENERATION_PROVIDER_CHAIN_TOOL_ID)
    );

    let music_cover = capabilities
        .ai_tools
        .iter()
        .find(|tool| tool.kind == AiToolKind::MusicCover)
        .expect("configured music cover should be exposed");
    assert_eq!(music_cover.policy, AiToolPolicy::ForceHost);
    assert_eq!(
        music_cover.fallback_tool_id.as_deref(),
        Some(crate::provider_config::MUSIC_COVER_PROVIDER_CHAIN_TOOL_ID)
    );

    assert!(
        capabilities
            .allows_staged_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
    );
    assert!(
        capabilities
            .allows_staged_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Audio)
    );
}

/// Keeps provider-native media capabilities first and only adds configured host targets as fallback routes.
/// 保持 Provider 原生媒体能力优先，并仅把已配置宿主目标作为兜底路由加入。
#[test]
fn ai_tool_configuration_preserves_native_generation_with_host_fallback() {
    let mut definition = test_provider_definition();
    definition.ai_tools.push(AiToolDeclaration::provider_native(
        AiToolKind::ImageGeneration,
        "native_image_generation",
    ));
    let mut capabilities = resolve_model_capabilities(&definition, None);
    let configuration = PersistedAiToolConfiguration {
        vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
            id: String::from("minimax"),
            vendor_id: String::from("minimax"),
            name: String::from("MiniMax"),
            enabled: true,
            api_key: String::from("token"),
            connection_mode: None,
            base_url: String::new(),
            region: String::from("global"),
            targets: vec![
                crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::ImageGeneration,
                    "image-01",
                    "image-01",
                ),
            ],
        }],
        ..PersistedAiToolConfiguration::default()
    };

    apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

    let image_generation = capabilities
        .ai_tools
        .iter()
        .find(|tool| tool.kind == AiToolKind::ImageGeneration)
        .expect("native image generation should remain exposed");
    assert_eq!(image_generation.policy, AiToolPolicy::Auto);
    assert!(image_generation.native_provider_tool);
    assert_eq!(
        image_generation.native_tool_type.as_deref(),
        Some("native_image_generation")
    );
    assert_eq!(
        image_generation.fallback_tool_id.as_deref(),
        Some(crate::provider_config::IMAGE_GENERATION_PROVIDER_CHAIN_TOOL_ID)
    );
}

/// Leaves provider-native media capabilities untouched when no host target is configured.
/// 未配置宿主目标时保持 Provider 原生媒体能力不变。
#[test]
fn ai_tool_configuration_keeps_native_generation_without_host_targets() {
    let mut definition = test_provider_definition();
    definition.ai_tools.push(AiToolDeclaration::provider_native(
        AiToolKind::ImageGeneration,
        "native_image_generation",
    ));
    let mut capabilities = resolve_model_capabilities(&definition, None);

    apply_ai_tool_runtime_configuration(
        &mut capabilities,
        &PersistedAiToolConfiguration::default(),
    );

    let image_generation = capabilities
        .ai_tools
        .iter()
        .find(|tool| tool.kind == AiToolKind::ImageGeneration)
        .expect("native image generation should remain exposed");
    assert_eq!(image_generation.policy, AiToolPolicy::Auto);
    assert!(image_generation.native_provider_tool);
    assert_eq!(
        image_generation.fallback_tool_id.as_deref(),
        None,
        "host fallback should not be invented without configured targets"
    );
}
