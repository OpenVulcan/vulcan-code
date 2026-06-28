use super::super::{
    AiToolDeclaration, AiToolKind, ModelChoice, ModelInterleavedCapability, ModelInterleavedField,
    PersistedProviderEntry, ProviderBaseUrlConfig, ProviderBaseUrlKind, ProviderConfigurationFile,
    ProviderModelDiscoveryConfig, ProviderModelDiscoveryKind, ProviderRuntimeKind,
    ProviderUsageCaptureConfig, ReasoningCapabilityKind, ReasoningProfileConfig,
    SupportedProviderDefinition, catalog_model_choices, effective_model_choices,
    extract_models_from_array, filter_remote_model_choices, merge_embedded_model_metadata,
    normalize_provider_configuration_for_save, persist_remote_models, remote_models_url,
};
use super::helpers::test_model_choice;
use serde_json::{Map as JsonMap, json};
use std::collections::BTreeMap;

/// Verifies remote model extraction preserves declared flat capability metadata and field-mapped interleaved settings.
/// 验证远端模型提取会保留已声明的扁平能力元数据与字段映射型交错设置。
#[test]
fn extract_models_from_array_reads_flat_remote_metadata() {
    let models = extract_models_from_array(&[json!({
        "id": "deepseek-v4-pro",
        "name": "DeepSeek V4 Pro",
        "status": "stable",
        "family": "deepseek",
        "releaseDate": "2026-05-01",
        "contextLimit": 131072,
        "inputLimit": "120000",
        "outputLimit": 8192,
        "reasoning": true,
        "toolCall": "true",
        "temperature": false,
        "attachment": false,
        "interleaved": { "field": "reasoning_content" },
        "reasoningProfile": {
            "kind": "binary",
            "off": { "thinking": false },
            "low": { "thinking": true }
        },
        "aiTools": [
            {
                "kind": "web_search",
                "nativeProviderTool": true,
                "nativeToolType": "web_search_preview",
                "citationSupport": true
            }
        ],
        "inputModalities": ["text", "image"],
        "outputModalities": ["text"]
    })]);

    assert_eq!(models.len(), 1);
    let model = &models[0];
    assert_eq!(model.id, "deepseek-v4-pro");
    assert_eq!(model.name, "DeepSeek V4 Pro");
    assert_eq!(model.status.as_deref(), Some("stable"));
    assert_eq!(model.family.as_deref(), Some("deepseek"));
    assert_eq!(model.release_date.as_deref(), Some("2026-05-01"));
    assert_eq!(model.context_limit, Some(131072));
    assert_eq!(model.input_limit, Some(120000));
    assert_eq!(model.output_limit, Some(8192));
    assert_eq!(model.reasoning, Some(true));
    assert_eq!(model.tool_call, Some(true));
    assert_eq!(model.temperature, Some(false));
    assert_eq!(model.attachment, Some(false));
    assert_eq!(
        model.interleaved,
        Some(ModelInterleavedCapability::Field(ModelInterleavedField {
            field: String::from("reasoning_content"),
        }))
    );
    assert_eq!(model.input_modalities, vec!["text", "image"]);
    assert_eq!(model.output_modalities, vec!["text"]);
    assert!(model.reasoning_profile.is_some());
    assert_eq!(model.ai_tools.len(), 1);
    assert_eq!(model.ai_tools[0].kind, AiToolKind::WebSearch);
}

/// Verifies remote model extraction accepts nested modality declarations when the provider returns a `modalities` object instead of flat arrays.
/// 验证当供应商返回 `modalities` 对象而非扁平数组时，远端模型提取仍可接受嵌套模态声明。
#[test]
fn extract_models_from_array_reads_nested_modalities() {
    let models = extract_models_from_array(&[json!({
        "id": "qwen3.6-plus",
        "modalities": {
            "input": ["text", "image", "video"],
            "output": ["text"]
        }
    })]);

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].input_modalities, vec!["text", "image", "video"]);
    assert_eq!(models[0].output_modalities, vec!["text"]);
}

/// Verifies remote model extraction accepts OpenRouter-style `architecture` modality metadata and upgrades it into normalized lists.
/// 验证远端模型提取可接受 OpenRouter 风格的 `architecture` 模态元数据，并将其升级为规范化列表。
#[test]
fn extract_models_from_array_reads_architecture_modalities() {
    let models = extract_models_from_array(&[json!({
        "id": "qwen/qwen3-vl-32b-instruct",
        "architecture": {
            "modality": "text+image->text",
            "input_modalities": ["text", "image"],
            "output_modalities": ["text"]
        }
    })]);

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].input_modalities, vec!["text", "image"]);
    assert_eq!(models[0].output_modalities, vec!["text"]);
}

/// Verifies the OpenRouter remote refresh keeps conversation-capable models while hiding OCR, embeddings, rerank, and image-generation entries by default.
/// 验证 OpenRouter 远端刷新默认保留可对话模型，同时隐藏 OCR、向量、重排与图像生成条目。
#[test]
fn filter_remote_model_choices_keeps_only_openrouter_conversation_models() {
    let filtered = filter_remote_model_choices(
        "openrouter",
        vec![
            ModelChoice {
                id: String::from("openai/gpt-4.1"),
                name: String::from("GPT-4.1"),
                default_enabled: false,
                status: None,
                family: None,
                release_date: None,
                context_limit: None,
                input_limit: None,
                output_limit: None,
                reasoning: None,
                tool_call: None,
                temperature: None,
                attachment: None,
                interleaved: None,
                reasoning_profile: None,
                ai_tools: Vec::new(),
                input_modalities: vec![String::from("text")],
                output_modalities: vec![String::from("text")],
            },
            ModelChoice {
                id: String::from("qwen/qwen3-vl-32b-instruct"),
                name: String::from("Qwen3 VL 32B Instruct"),
                default_enabled: false,
                status: None,
                family: None,
                release_date: None,
                context_limit: None,
                input_limit: None,
                output_limit: None,
                reasoning: None,
                tool_call: None,
                temperature: None,
                attachment: None,
                interleaved: None,
                reasoning_profile: None,
                ai_tools: Vec::new(),
                input_modalities: vec![String::from("text"), String::from("image")],
                output_modalities: vec![String::from("text")],
            },
            ModelChoice {
                id: String::from("baidu/qianfan-ocr-fast"),
                name: String::from("Qianfan OCR Fast"),
                default_enabled: false,
                status: None,
                family: None,
                release_date: None,
                context_limit: None,
                input_limit: None,
                output_limit: None,
                reasoning: None,
                tool_call: None,
                temperature: None,
                attachment: None,
                interleaved: None,
                reasoning_profile: None,
                ai_tools: Vec::new(),
                input_modalities: vec![String::from("text"), String::from("image")],
                output_modalities: vec![String::from("text")],
            },
            ModelChoice {
                id: String::from("google/gemini-2.5-flash-image"),
                name: String::from("Gemini 2.5 Flash Image"),
                default_enabled: false,
                status: None,
                family: None,
                release_date: None,
                context_limit: None,
                input_limit: None,
                output_limit: None,
                reasoning: None,
                tool_call: None,
                temperature: None,
                attachment: None,
                interleaved: None,
                reasoning_profile: None,
                ai_tools: Vec::new(),
                input_modalities: vec![String::from("text"), String::from("image")],
                output_modalities: vec![String::from("text"), String::from("image")],
            },
            ModelChoice {
                id: String::from("cohere/rerank-v3.5"),
                name: String::from("Cohere Rerank v3.5"),
                default_enabled: false,
                status: None,
                family: None,
                release_date: None,
                context_limit: None,
                input_limit: None,
                output_limit: None,
                reasoning: None,
                tool_call: None,
                temperature: None,
                attachment: None,
                interleaved: None,
                reasoning_profile: None,
                ai_tools: Vec::new(),
                input_modalities: vec![String::from("text")],
                output_modalities: vec![String::from("text")],
            },
            ModelChoice {
                id: String::from("openai/text-embedding-3-large"),
                name: String::from("text-embedding-3-large"),
                default_enabled: false,
                status: None,
                family: None,
                release_date: None,
                context_limit: None,
                input_limit: None,
                output_limit: None,
                reasoning: None,
                tool_call: None,
                temperature: None,
                attachment: None,
                interleaved: None,
                reasoning_profile: None,
                ai_tools: Vec::new(),
                input_modalities: vec![String::from("text")],
                output_modalities: vec![String::from("embedding")],
            },
        ],
    );

    let filtered_ids = filtered
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();
    assert_eq!(
        filtered_ids,
        vec![
            String::from("openai/gpt-4.1"),
            String::from("qwen/qwen3-vl-32b-instruct"),
        ]
    );
}

/// Verifies the OpenRouter remote refresh URL explicitly requests text-output models to keep discovery aligned with conversation pickers.
/// 验证 OpenRouter 远端刷新 URL 会显式请求文本输出模型，以保持发现结果与对话模型选择器对齐。
#[test]
fn remote_models_url_requests_text_outputs_for_openrouter() {
    let definition = SupportedProviderDefinition {
        id: String::from("openrouter"),
        name: String::from("OpenRouter"),
        runtime: ProviderRuntimeKind::OpenRouter,
        user_agent: None,
        credential_env_keys: Vec::new(),
        extra_headers: BTreeMap::new(),
        extra_body: JsonMap::new(),
        usage_capture: ProviderUsageCaptureConfig::default(),
        model_runtime_overrides: BTreeMap::new(),
        reasoning_profile: None,
        ai_tools: Vec::new(),
        fields: Vec::new(),
        base_url: ProviderBaseUrlConfig {
            kind: ProviderBaseUrlKind::Fixed,
            value: Some(String::from("https://openrouter.ai/api/v1")),
            template: None,
            parameter: None,
            allow_override: false,
        },
        model_discovery: ProviderModelDiscoveryConfig {
            kind: ProviderModelDiscoveryKind::OpenAiModels,
        },
        models: Vec::new(),
    };

    let url = remote_models_url(&definition, &Default::default())
        .expect("openrouter remote refresh url must exist");
    assert_eq!(
        url,
        "https://openrouter.ai/api/v1/models?output_modalities=text"
    );
}

/// Verifies embedded metadata only fills fields that the dynamically discovered remote model did not already declare explicitly.
/// 验证内置元数据只会补全动态发现模型未显式声明的字段，而不会覆盖远端已声明值。
#[test]
fn merge_embedded_model_metadata_keeps_remote_values_and_fills_blanks() {
    let discovered_models = vec![ModelChoice {
        id: String::from("qwen3.6-plus"),
        name: String::from("Qwen 3.6 Plus Remote"),
        default_enabled: false,
        status: None,
        family: Some(String::from("qwen")),
        release_date: None,
        context_limit: Some(262144),
        input_limit: None,
        output_limit: Some(8192),
        reasoning: Some(true),
        tool_call: None,
        temperature: Some(false),
        attachment: Some(false),
        interleaved: Some(ModelInterleavedCapability::Enabled(true)),
        reasoning_profile: None,
        ai_tools: Vec::new(),
        input_modalities: vec![String::from("text"), String::from("image")],
        output_modalities: Vec::new(),
    }];
    let embedded_models = vec![ModelChoice {
        id: String::from("qwen3.6-plus"),
        name: String::from("Qwen 3.6 Plus Embedded"),
        default_enabled: false,
        status: Some(String::from("preview")),
        family: Some(String::from("qwen-embedded")),
        release_date: Some(String::from("2026-04-20")),
        context_limit: Some(128000),
        input_limit: Some(120000),
        output_limit: Some(4096),
        reasoning: Some(false),
        tool_call: Some(true),
        temperature: Some(true),
        attachment: Some(true),
        interleaved: Some(ModelInterleavedCapability::Field(ModelInterleavedField {
            field: String::from("reasoning_details"),
        })),
        reasoning_profile: Some(ReasoningProfileConfig {
            kind: Some(ReasoningCapabilityKind::Binary),
            off: Some(json!({ "thinking": false })),
            low: Some(json!({ "thinking": true })),
            ..ReasoningProfileConfig::default()
        }),
        ai_tools: vec![AiToolDeclaration::provider_native(
            AiToolKind::WebSearch,
            "web_search_preview",
        )],
        input_modalities: vec![
            String::from("text"),
            String::from("image"),
            String::from("video"),
        ],
        output_modalities: vec![String::from("text")],
    }];

    let merged = merge_embedded_model_metadata(discovered_models, &embedded_models);

    assert_eq!(merged.len(), 1);
    let model = &merged[0];
    assert_eq!(model.name, "Qwen 3.6 Plus Remote");
    assert_eq!(model.status.as_deref(), Some("preview"));
    assert_eq!(model.family.as_deref(), Some("qwen"));
    assert_eq!(model.release_date.as_deref(), Some("2026-04-20"));
    assert_eq!(model.context_limit, Some(262144));
    assert_eq!(model.input_limit, Some(120000));
    assert_eq!(model.output_limit, Some(8192));
    assert_eq!(model.reasoning, Some(true));
    assert_eq!(model.tool_call, Some(true));
    assert_eq!(model.temperature, Some(false));
    assert_eq!(model.attachment, Some(false));
    assert_eq!(
        model.interleaved,
        Some(ModelInterleavedCapability::Enabled(true))
    );
    assert!(model.reasoning_profile.is_some());
    assert_eq!(model.input_modalities, vec!["text", "image"]);
    assert_eq!(model.output_modalities, vec!["text"]);
    assert_eq!(model.ai_tools.len(), 1);
}

/// Verifies legacy provider entries without any persisted enabled-model state still expose the full embedded catalog instead of collapsing to new default selections.
/// 验证未持久化启用模型状态的旧版供应商条目仍会暴露完整内置目录，而不会收缩到新的默认勾选集合。
#[test]
fn effective_model_choices_keep_full_catalog_for_legacy_entries_without_enablement_state() {
    let provider_id = "openrouter";
    let expected_ids = catalog_model_choices(provider_id, &ProviderConfigurationFile::default())
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();
    let mut config = ProviderConfigurationFile::default();
    let entry = config
        .provider
        .entry(String::from(provider_id))
        .or_insert_with(PersistedProviderEntry::default);
    entry.enabled_models = None;

    let effective_ids = effective_model_choices(provider_id, &config)
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();

    assert_eq!(effective_ids, expected_ids);
}

/// Verifies deprecated remote caches no longer override the embedded provider catalog during model resolution.
/// 验证废弃远端缓存不会再在模型解析阶段覆盖内置供应商目录。
#[test]
fn catalog_model_choices_ignore_deprecated_remote_cache() {
    let provider_id = "openrouter";
    let mut config = ProviderConfigurationFile::default();
    let fake_remote_model = test_model_choice("fake-remote-model", true, None);
    let entry = config
        .provider
        .entry(String::from(provider_id))
        .or_insert_with(PersistedProviderEntry::default);
    entry.models = persist_remote_models(&[fake_remote_model]);

    let catalog_ids = catalog_model_choices(provider_id, &config)
        .into_iter()
        .map(|model| model.id)
        .collect::<Vec<_>>();

    assert!(
        !catalog_ids
            .iter()
            .any(|model_id| model_id == "fake-remote-model")
    );
    assert!(!catalog_ids.is_empty());
}

/// Verifies configuration serialization normalization strips deprecated remote caches before the file hits disk.
/// 验证配置序列化规范化会在文件落盘前移除废弃远端缓存。
#[test]
fn normalize_provider_configuration_for_save_clears_remote_caches() {
    let mut config = ProviderConfigurationFile::default();
    let cached_model = test_model_choice("cached-model", true, None);
    let entry = config
        .provider
        .entry(String::from("openrouter"))
        .or_insert_with(PersistedProviderEntry::default);
    entry.models = persist_remote_models(&[cached_model]);

    let normalized = normalize_provider_configuration_for_save(&config);

    assert!(
        config
            .provider
            .get("openrouter")
            .is_some_and(|entry| !entry.models.is_empty())
    );
    assert!(
        normalized
            .provider
            .get("openrouter")
            .is_some_and(|entry| entry.models.is_empty())
    );
}
