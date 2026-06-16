use super::{
    AiCapabilityVendorConnectionMode, AiToolDeclaration, AiToolKind, ModelChoice,
    ModelInterleavedCapability, ModelInterleavedField, PersistedAiCapabilityRouteConfiguration,
    PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
    PersistedAiToolConfiguration, PersistedLuaSkillToolRegistrationState,
    PersistedLuaSkillsConfiguration, PersistedPromptSkillState, PersistedPromptSkillsConfiguration,
    PersistedProviderEntry, PersistedWebSearchConfiguration, PersistedWebSearchProviderEntry,
    ProjectVulcanCodeConfigurationFile, ProviderApiFamily, ProviderBaseUrlConfig,
    ProviderBaseUrlKind, ProviderConfigurationFile, ProviderModelDiscoveryConfig,
    ProviderModelDiscoveryKind, ProviderRuntimeKind, ProviderUsageCaptureConfig,
    ReasoningCapabilityKind, ReasoningEffortLevel, ReasoningProfileConfig,
    SupportedProviderDefinition, WebSearchProviderDispatchMode, WebSearchProviderKind,
    WebSearchRoutingMode, apply_provider_reasoning_model_overrides, catalog_model_choices,
    effective_model_choices, extract_models_from_array, filter_remote_model_choices,
    load_project_configuration_file, merge_embedded_model_metadata, merge_refreshed_models,
    normalize_provider_configuration_for_save, persist_remote_models,
    project_configuration_file_path, remote_models_url, save_project_configuration_file,
    supported_provider_by_id,
};
use serde_json::{Map as JsonMap, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_vmm::VmmConfigLayer;

/// Builds a unique temporary directory path for provider configuration tests.
/// 为 Provider 配置测试构建一个唯一临时目录路径。
fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{label}-{}-{nanos}", std::process::id()))
}

/// Builds one minimal supported-provider definition used by the reasoning-selection tests.
/// 构建供思考选择测试复用的最小受支持 Provider 定义。
fn test_provider_definition(
    runtime: ProviderRuntimeKind,
    provider_id: &str,
    reasoning_profile: Option<ReasoningProfileConfig>,
) -> SupportedProviderDefinition {
    SupportedProviderDefinition {
        id: provider_id.to_string(),
        name: provider_id.to_string(),
        runtime,
        user_agent: None,
        credential_env_keys: Vec::new(),
        extra_headers: Default::default(),
        extra_body: JsonMap::new(),
        usage_capture: ProviderUsageCaptureConfig::default(),
        model_runtime_overrides: BTreeMap::new(),
        reasoning_profile,
        ai_tools: Vec::new(),
        fields: Vec::new(),
        base_url: ProviderBaseUrlConfig {
            kind: ProviderBaseUrlKind::Native,
            value: None,
            template: None,
            parameter: None,
            allow_override: false,
        },
        model_discovery: ProviderModelDiscoveryConfig {
            kind: ProviderModelDiscoveryKind::None,
        },
        models: Vec::new(),
    }
}

/// Builds one minimal model choice used by the reasoning-selection tests.
/// 构建供思考选择测试复用的最小模型选项。
fn test_model_choice(
    model_id: &str,
    supports_reasoning: bool,
    reasoning_profile: Option<ReasoningProfileConfig>,
) -> ModelChoice {
    ModelChoice {
        id: model_id.to_string(),
        name: model_id.to_string(),
        default_enabled: false,
        status: None,
        family: None,
        release_date: None,
        context_limit: None,
        input_limit: None,
        output_limit: None,
        reasoning: Some(supports_reasoning),
        tool_call: Some(true),
        temperature: Some(true),
        attachment: None,
        interleaved: None,
        reasoning_profile,
        ai_tools: Vec::new(),
        input_modalities: Vec::new(),
        output_modalities: Vec::new(),
    }
}

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

/// Verifies binary reasoning profiles collapse the selector into the expected on/off pair and remap higher remembered tiers to “开启”.
/// 验证二值思考配置会把选择器收缩为预期的开关二项，并将更高的已记忆档位回退为“开启”。
#[test]
fn binary_reasoning_profile_builds_on_off_selector_and_downgrades_high_to_enabled() {
    let provider = test_provider_definition(
        ProviderRuntimeKind::OpenAiCompatible,
        "bailing",
        Some(ReasoningProfileConfig {
            kind: Some(ReasoningCapabilityKind::Binary),
            off: Some(json!({ "enable_thinking": false })),
            low: Some(json!({ "enable_thinking": true })),
            medium: Some(json!({ "enable_thinking": true })),
            ..ReasoningProfileConfig::default()
        }),
    );
    let model = test_model_choice("qwen-coder", true, None);

    let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::High);

    assert_eq!(view.options.len(), 2);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.options[1].label, "开启");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Low);
    assert_eq!(
        view.note.as_deref(),
        Some("当前模型 `qwen-coder` 不支持 `高` 思考档位，已自动切换到 `开启`。")
    );
}

/// Verifies Alibaba China provider definitions expose one binary reasoning selector instead of falling back to tiered controls.
/// 验证阿里百炼中国区供应商定义会暴露二值思考选择器，而不会回退成多档位控制。
#[test]
fn alibaba_cn_provider_uses_binary_reasoning_selector() {
    let provider = supported_provider_by_id("alibaba-cn").expect("alibaba-cn provider must exist");
    let model = provider
        .models
        .iter()
        .find(|item| item.id == "qwen3.6-plus")
        .expect("qwen3.6-plus model must exist");

    let view = provider.reasoning_selection_view(Some(model), ReasoningEffortLevel::High);

    assert_eq!(view.options.len(), 2);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.options[1].label, "开启");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Low);
}

/// Verifies Alibaba Coding Plan China provider definitions also expose the binary reasoning selector.
/// 验证阿里 Coding Plan 中国区供应商定义同样会暴露二值思考选择器。
#[test]
fn alibaba_coding_plan_cn_provider_uses_binary_reasoning_selector() {
    let provider = supported_provider_by_id("alibaba-coding-plan-cn")
        .expect("alibaba-coding-plan-cn provider must exist");
    let model = provider
        .models
        .iter()
        .find(|item| item.id == "qwen3.6-plus")
        .expect("qwen3.6-plus model must exist");

    let view = provider.reasoning_selection_view(Some(model), ReasoningEffortLevel::High);

    assert_eq!(view.options.len(), 2);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.options[1].label, "开启");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Low);
}

/// Verifies DeepSeek official provider definitions expose the expected three-state selector and remap the shared medium default onto `高`.
/// 验证 DeepSeek 官方供应商定义会暴露预期的三态选择器，并将共享中档默认映射到 `高`。
#[test]
fn deepseek_provider_uses_off_high_max_selector() {
    let provider = supported_provider_by_id("deepseek").expect("deepseek provider must exist");
    let model = provider
        .models
        .iter()
        .find(|item| item.id == "deepseek-v4-pro")
        .expect("deepseek-v4-pro model must exist");

    let view = provider.reasoning_selection_view(Some(model), ReasoningEffortLevel::Medium);

    assert_eq!(view.options.len(), 3);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.options[1].label, "高");
    assert_eq!(view.options[2].label, "Max");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::High);
}

/// Verifies custom reasoning labels override the shared compact labels when one provider declares them explicitly.
/// 验证当供应商显式声明自定义思考标签时，会覆盖共享的紧凑标签。
#[test]
fn custom_reasoning_labels_override_shared_labels() {
    let provider = test_provider_definition(
        ProviderRuntimeKind::OpenAiCompatible,
        "custom-provider",
        Some(ReasoningProfileConfig {
            off: Some(json!({"thinking": {"type": "disabled"}})),
            high: Some(json!({"thinking": {"type": "enabled"}, "reasoning_effort": "high"})),
            x_high: Some(json!({"thinking": {"type": "enabled"}, "reasoning_effort": "max"})),
            high_label: Some(String::from("高")),
            x_high_label: Some(String::from("Max")),
            ..ReasoningProfileConfig::default()
        }),
    );
    let model = test_model_choice("custom-model", true, None);

    let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::High);

    assert_eq!(view.options[1].label, "高");
    assert_eq!(view.options[2].label, "Max");
}

/// Verifies default-only reasoning profiles collapse the selector into one non-editable “默认” entry without forcing explicit patches.
/// 验证默认思考配置会把选择器收缩成一个不可编辑的“默认”项，而不会强制显式 Patch。
#[test]
fn default_only_reasoning_profile_builds_default_selector() {
    let provider = test_provider_definition(
        ProviderRuntimeKind::OpenAiCompatible,
        "default-provider",
        Some(ReasoningProfileConfig {
            kind: Some(ReasoningCapabilityKind::DefaultOnly),
            ..ReasoningProfileConfig::default()
        }),
    );
    let model = test_model_choice("default-model", true, None);

    let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::High);

    assert_eq!(view.options.len(), 1);
    assert_eq!(view.options[0].label, "默认");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Off);
}

/// Verifies Alibaba-family models outside the maintained binary-toggle matrix fall back to the non-editable default reasoning mode.
/// 验证未进入已维护二值开关矩阵的阿里系模型会回退到不可编辑的默认思考模式。
#[test]
fn alibaba_unmaintained_model_falls_back_to_default_only_reasoning_mode() {
    let mut models = vec![test_model_choice("future-ali-model", false, None)];
    apply_provider_reasoning_model_overrides("alibaba-cn", &mut models);
    let model = models.pop().expect("model override result");

    assert_eq!(model.reasoning, Some(true));
    assert_eq!(
        model
            .reasoning_profile
            .as_ref()
            .and_then(|profile| profile.kind),
        Some(ReasoningCapabilityKind::DefaultOnly)
    );
}

/// Verifies a model that does not support reasoning forces the selector into a disabled off-only state and reports the downgrade.
/// 验证不支持思考的模型会强制将选择器收缩为仅关闭状态，并报告回退说明。
#[test]
fn non_reasoning_model_forces_selector_to_off_only() {
    let provider = test_provider_definition(ProviderRuntimeKind::OpenAi, "openai", None);
    let model = test_model_choice("gpt-lite", false, None);

    let view = provider.reasoning_selection_view(Some(&model), ReasoningEffortLevel::Medium);

    assert_eq!(view.options.len(), 1);
    assert_eq!(view.options[0].label, "关闭");
    assert_eq!(view.selected_effort, ReasoningEffortLevel::Off);
    assert_eq!(
        view.note.as_deref(),
        Some("当前模型 `gpt-lite` 不支持思考能力，已自动切换到关闭思考。")
    );
}

/// Verifies Gemini 3 model families expose only the runtime-supported low/high compatibility tiers by default.
/// 验证 Gemini 3 模型家族默认仅暴露运行时真正支持的低/高兼容档位。
#[test]
fn gemini_three_defaults_only_expose_low_and_high() {
    let provider = test_provider_definition(ProviderRuntimeKind::GoogleGemini, "google", None);
    let model = test_model_choice("gemini-3-pro", true, None);

    let supported = provider.supported_reasoning_efforts(Some(&model));

    assert_eq!(
        supported,
        vec![ReasoningEffortLevel::Low, ReasoningEffortLevel::High]
    );
}

/// Verifies the remembered workbench reasoning preference keeps the stable camelCase field name and round-trips through the shared provider configuration file.
/// 验证已记忆的工作台思考偏好会保持稳定的 camelCase 字段名，并且能够经由共享 Provider 配置文件完成往返序列化。
#[test]
fn provider_configuration_round_trips_workbench_reasoning_effort() {
    let config = ProviderConfigurationFile {
        workbench_reasoning_effort: Some(ReasoningEffortLevel::High),
        ..ProviderConfigurationFile::default()
    };

    let value = serde_json::to_value(&config).expect("provider configuration must serialize");
    assert_eq!(value.get("workbenchReasoningEffort"), Some(&json!("high")));

    let restored: ProviderConfigurationFile =
        serde_json::from_value(value).expect("provider configuration must deserialize");
    assert_eq!(
        restored.workbench_reasoning_effort,
        Some(ReasoningEffortLevel::High)
    );
}

/// Verifies the persisted GUI locale keeps the stable `uiLocale` field name and round-trips through the shared provider configuration file.
/// 验证持久化图形界面语言会保持稳定的 `uiLocale` 字段名，并且能够经由共享 Provider 配置文件完成往返序列化。
#[test]
fn provider_configuration_round_trips_ui_locale() {
    let config = ProviderConfigurationFile {
        ui_locale: Some(String::from("en-US")),
        ..ProviderConfigurationFile::default()
    };

    let value = serde_json::to_value(&config).expect("provider configuration must serialize");
    assert_eq!(value.get("uiLocale"), Some(&json!("en-US")));

    let restored: ProviderConfigurationFile =
        serde_json::from_value(value).expect("provider configuration must deserialize");
    assert_eq!(restored.ui_locale.as_deref(), Some("en-US"));
}

/// Verifies the persisted transcript turn-page size keeps the stable `sessionTranscriptTurnPageSize` field name and round-trips through the shared provider configuration file.
/// 验证持久化转录轮次分页大小会保持稳定的 `sessionTranscriptTurnPageSize` 字段名，并且能够经由共享 Provider 配置文件完成往返序列化。
#[test]
fn provider_configuration_round_trips_session_transcript_turn_page_size() {
    let config = ProviderConfigurationFile {
        session_transcript_turn_page_size: Some(40),
        ..ProviderConfigurationFile::default()
    };

    let value = serde_json::to_value(&config).expect("provider configuration must serialize");
    assert_eq!(value.get("sessionTranscriptTurnPageSize"), Some(&json!(40)));

    let restored: ProviderConfigurationFile =
        serde_json::from_value(value).expect("provider configuration must deserialize");
    assert_eq!(restored.session_transcript_turn_page_size, Some(40));
}

/// Verifies the persisted default command shell keeps the stable `defaultCommandShell` field name and round-trips through the shared provider configuration file.
/// 验证持久化默认命令 shell 会保持稳定的 `defaultCommandShell` 字段名，并且能够经由共享 Provider 配置文件完成往返序列化。
#[test]
fn provider_configuration_round_trips_default_command_shell() {
    let config = ProviderConfigurationFile {
        default_command_shell: Some(String::from("pwsh")),
        ..ProviderConfigurationFile::default()
    };

    let value = serde_json::to_value(&config).expect("provider configuration must serialize");
    assert_eq!(value.get("defaultCommandShell"), Some(&json!("pwsh")));

    let restored: ProviderConfigurationFile =
        serde_json::from_value(value).expect("provider configuration must deserialize");
    assert_eq!(restored.default_command_shell.as_deref(), Some("pwsh"));
}

/// Verifies global VMM configuration keeps the stable root `vmm` field and skips empty defaults.
/// 验证全局 VMM 配置保持稳定的根级 `vmm` 字段，并跳过空默认值。
#[test]
fn provider_configuration_round_trips_vmm_configuration() {
    let default_value = serde_json::to_value(ProviderConfigurationFile::default())
        .expect("provider configuration must serialize");
    assert!(default_value.get("vmm").is_none());

    let config = ProviderConfigurationFile {
        vmm: VmmConfigLayer {
            enabled: Some(true),
            grpc_target: Some(String::from("127.0.0.1:50051")),
            receive_timeout_ms: Some(15_000),
            user_id: Some(7),
            project_id: Some(42),
            ai_tools_enabled: Some(false),
            ..VmmConfigLayer::default()
        },
        ..ProviderConfigurationFile::default()
    };

    let value = serde_json::to_value(&config).expect("provider configuration must serialize");
    assert_eq!(value["vmm"]["grpc_target"], json!("127.0.0.1:50051"));
    assert_eq!(value["vmm"]["receive_timeout_ms"], json!(15_000));
    assert_eq!(value["vmm"]["ai_tools_enabled"], json!(false));

    let restored: ProviderConfigurationFile =
        serde_json::from_value(value).expect("provider configuration must deserialize");
    assert_eq!(restored.vmm, config.vmm);
}

/// Verifies API-native tool configuration keeps the stable root `aiTools.webSearch` shape through serialization.
/// 验证 API 原生工具配置在序列化过程中保持稳定的根级 `aiTools.webSearch` 形状。
#[test]
fn provider_configuration_round_trips_web_search_routing_configuration() {
    let config = ProviderConfigurationFile {
        ai_tools: PersistedAiToolConfiguration {
            vendors: vec![PersistedAiCapabilityVendorEntry {
                id: String::from("brave"),
                vendor_id: String::from("brave"),
                name: String::from("Brave"),
                enabled: true,
                api_key: String::from("{env:BRAVE_SEARCH_API_KEY}"),
                connection_mode: None,
                base_url: String::new(),
                region: String::new(),
                targets: vec![PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::WebSearch,
                    "brave",
                    "brave",
                )],
            }],
            web_search: PersistedWebSearchConfiguration {
                mode: WebSearchRoutingMode::ThirdPartyFirst,
                target_order: vec![String::from("brave/brave")],
                providers: vec![PersistedWebSearchProviderEntry {
                    id: String::from("brave"),
                    name: String::from("Brave Search"),
                    kind: WebSearchProviderKind::Brave,
                    target_alias: String::from("brave"),
                    enabled: true,
                    api_key: String::from("{env:BRAVE_SEARCH_API_KEY}"),
                    base_url: String::new(),
                    model: String::new(),
                    region: String::new(),
                    max_results: 8,
                }],
                dispatch_mode: WebSearchProviderDispatchMode::Balanced,
                max_results: 8,
            },
            ..PersistedAiToolConfiguration::default()
        },
        ..ProviderConfigurationFile::default()
    };

    let value = serde_json::to_value(&config).expect("provider configuration must serialize");
    assert_eq!(
        value["aiTools"]["webSearch"]["mode"],
        json!("third-party-first")
    );
    assert_eq!(
        value["aiTools"]["webSearch"]["dispatchMode"],
        json!("balanced")
    );
    assert_eq!(
        value["aiTools"]["webSearch"]["providers"][0]["kind"],
        json!("brave")
    );

    let restored: ProviderConfigurationFile =
        serde_json::from_value(value).expect("provider configuration must deserialize");
    assert_eq!(
        restored.ai_tools.web_search.mode,
        WebSearchRoutingMode::ThirdPartyFirst
    );
    assert_eq!(
        restored.ai_tools.web_search.dispatch_mode,
        WebSearchProviderDispatchMode::Balanced
    );
    assert_eq!(restored.ai_tools.web_search.providers.len(), 1);
    assert!(restored.ai_tools.web_search.providers[0].enabled);
}

/// Verifies the provider configuration file round-trips the dedicated host-managed LuaSkills registration block.
/// 验证供应商配置文件可以完整往返独立的宿主管理 LuaSkills 注册配置块。
#[test]
fn provider_configuration_round_trips_luaskills_registration_configuration() {
    let config = ProviderConfigurationFile {
        luaskills: PersistedLuaSkillsConfiguration {
            tool_registration_by_skill: BTreeMap::from([(
                String::from("skill-a"),
                BTreeMap::from([
                    (
                        String::from("tool-a"),
                        PersistedLuaSkillToolRegistrationState::Implicit,
                    ),
                    (
                        String::from("tool-b"),
                        PersistedLuaSkillToolRegistrationState::Disabled,
                    ),
                ]),
            )]),
            disabled_tools_by_skill: BTreeMap::new(),
        },
        ..ProviderConfigurationFile::default()
    };

    let value = serde_json::to_value(&config).expect("provider configuration must serialize");
    assert_eq!(
        value["luaskills"]["toolRegistrationBySkill"]["skill-a"]["tool-a"],
        json!("implicit")
    );
    assert_eq!(
        value["luaskills"]["toolRegistrationBySkill"]["skill-a"]["tool-b"],
        json!("disabled")
    );

    let restored: ProviderConfigurationFile =
        serde_json::from_value(value).expect("provider configuration must deserialize");
    assert_eq!(
        restored
            .luaskills
            .tool_registration_by_skill
            .get("skill-a")
            .and_then(|tools| tools.get("tool-a"))
            .copied(),
        Some(PersistedLuaSkillToolRegistrationState::Implicit)
    );
}

/// Verifies prompt skills configuration round-trips independently from LuaSkills.
/// 验证提示词技能配置可以独立于 LuaSkills 完整往返。
#[test]
fn provider_configuration_round_trips_prompt_skills_configuration() {
    let config = ProviderConfigurationFile {
        prompt_skills: PersistedPromptSkillsConfiguration {
            enabled: true,
            user_skill_root: Some(PathBuf::from("C:/Users/example/.vulcan/skills")),
            default_skill_state: PersistedPromptSkillState::Enabled,
            skill_overrides: BTreeMap::from([(
                String::from("demo-skill"),
                PersistedPromptSkillState::Disabled,
            )]),
        },
        ..ProviderConfigurationFile::default()
    };

    let value = serde_json::to_value(&config).expect("provider configuration must serialize");
    assert_eq!(
        value["promptSkills"]["skillOverrides"]["demo-skill"],
        json!("disabled")
    );

    let restored: ProviderConfigurationFile =
        serde_json::from_value(value).expect("provider configuration must deserialize");
    assert_eq!(
        restored
            .prompt_skills
            .skill_overrides
            .get("demo-skill")
            .copied(),
        Some(PersistedPromptSkillState::Disabled)
    );
}

/// Verifies the dedicated LuaSkills registration configuration keeps only non-default tool entries and migrates legacy hidden entries to implicit mode.
/// 验证独立 LuaSkills 注册配置只保留非默认工具条目，并会将旧版隐藏条目迁移为隐式模式。
#[test]
fn luaskills_registration_configuration_only_persists_non_default_tools() {
    let mut configuration = PersistedLuaSkillsConfiguration {
        tool_registration_by_skill: BTreeMap::from([(
            String::from("skill-a"),
            BTreeMap::from([(
                String::from("tool-c"),
                PersistedLuaSkillToolRegistrationState::Disabled,
            )]),
        )]),
        disabled_tools_by_skill: BTreeMap::from([
            (
                String::from(" skill-a "),
                vec![
                    String::from("tool-a"),
                    String::from("tool-a"),
                    String::from(" "),
                ],
            ),
            (String::new(), vec![String::from("tool-x")]),
        ]),
    };

    assert!(configuration.normalize());
    assert_eq!(
        configuration.tool_registration_state("skill-a", "tool-a"),
        PersistedLuaSkillToolRegistrationState::Implicit
    );
    assert!(configuration.is_tool_explicitly_registered("skill-a", "tool-b"));
    assert!(configuration.set_tool_disabled("skill-a", "tool-b", true));
    assert_eq!(
        configuration.tool_registration_state("skill-a", "tool-b"),
        PersistedLuaSkillToolRegistrationState::Implicit
    );
    assert!(configuration.set_tool_disabled("skill-a", "tool-a", false));
    assert!(configuration.is_tool_explicitly_registered("skill-a", "tool-a"));
    assert!(configuration.set_tool_registration_state(
        "skill-a",
        "tool-b",
        PersistedLuaSkillToolRegistrationState::Disabled
    ));
    assert!(configuration.is_tool_disabled("skill-a", "tool-b"));
    assert_eq!(
        configuration
            .tool_registration_by_skill
            .get("skill-a")
            .cloned()
            .unwrap_or_default(),
        BTreeMap::from([
            (
                String::from("tool-b"),
                PersistedLuaSkillToolRegistrationState::Disabled,
            ),
            (
                String::from("tool-c"),
                PersistedLuaSkillToolRegistrationState::Disabled,
            ),
        ])
    );
    assert!(configuration.disabled_tools_by_skill.is_empty());
}

/// Verifies MiniMax web-search hints stay aligned with the official Token Plan CLI naming.
/// 验证 MiniMax 网页搜索提示与官方 Token Plan CLI 命名保持一致。
#[test]
fn minimax_web_search_configuration_hints_match_token_plan_cli() {
    assert_eq!(WebSearchProviderKind::MiniMax.label(), "MiniMax Search");
    assert_eq!(
        WebSearchProviderKind::MiniMax.api_key_label(),
        "Token Plan Key / {env:MINIMAX_API_KEY}"
    );
    assert_eq!(
        WebSearchProviderKind::MiniMax.api_key_placeholder(),
        "{env:MINIMAX_API_KEY}"
    );
    assert!(
        WebSearchProviderKind::MiniMax
            .configuration_note()
            .expect("MiniMax note must exist")
            .contains("Credits")
    );
}

/// Verifies web-search provider instances expose stable `instance/target` selectors for semantic routing.
/// 验证网页搜索供应商实例会暴露稳定的 `实例/目标` 选择器以供语义路由使用。
#[test]
fn web_search_provider_entry_builds_qualified_target() {
    let entry = PersistedWebSearchProviderEntry::new("minimax", WebSearchProviderKind::MiniMax);

    assert_eq!(entry.instance_alias(), "minimax");
    assert_eq!(entry.target_alias(), "minimax");
    assert_eq!(entry.qualified_target(), "minimax/minimax");
    assert!(entry.matches_target("minimax/minimax"));
}

/// Verifies web-search configuration normalization removes duplicate provider kinds and restores canonical provider identifiers.
/// 验证网页搜索配置归一化会移除重复提供方类型并恢复规范提供方标识。
#[test]
fn web_search_configuration_normalize_deduplicates_provider_kinds() {
    let mut configuration = PersistedWebSearchConfiguration {
        providers: vec![
            PersistedWebSearchProviderEntry {
                id: String::from("tavily-2"),
                name: String::from(""),
                kind: WebSearchProviderKind::Tavily,
                target_alias: String::from("tavily"),
                enabled: true,
                api_key: String::from("{env:TAVILY_API_KEY}"),
                base_url: String::new(),
                model: String::new(),
                region: String::new(),
                max_results: 5,
            },
            PersistedWebSearchProviderEntry {
                id: String::from("tavily-3"),
                name: String::from("备用 Tavily"),
                kind: WebSearchProviderKind::Tavily,
                target_alias: String::from("tavily"),
                enabled: true,
                api_key: String::from("{env:TAVILY_API_KEY}"),
                base_url: String::new(),
                model: String::new(),
                region: String::new(),
                max_results: 8,
            },
        ],
        ..PersistedWebSearchConfiguration::default()
    };

    assert!(configuration.normalize());
    assert_eq!(configuration.providers.len(), 1);
    assert_eq!(configuration.providers[0].id, "tavily");
    assert_eq!(configuration.providers[0].name, "Tavily");
    assert_eq!(
        configuration.providers[0].qualified_target(),
        "tavily/tavily"
    );
}

/// Verifies web-search normalization removes every provider kind that is no longer enabled by the shared capability catalog.
/// 验证网页搜索归一化会移除所有已不再由共享能力目录启用的提供方类型。
#[test]
fn web_search_configuration_normalize_removes_unsupported_provider_kinds() {
    let mut configuration = PersistedWebSearchConfiguration {
        providers: vec![
            PersistedWebSearchProviderEntry {
                id: String::from("duckduckgo"),
                name: String::from("DuckDuckGo"),
                kind: WebSearchProviderKind::DuckDuckGo,
                target_alias: String::from("duckduckgo"),
                enabled: true,
                api_key: String::new(),
                base_url: String::new(),
                model: String::new(),
                region: String::new(),
                max_results: 5,
            },
            PersistedWebSearchProviderEntry {
                id: String::from("brave"),
                name: String::from("Brave"),
                kind: WebSearchProviderKind::Brave,
                target_alias: String::from("brave"),
                enabled: true,
                api_key: String::from("{env:BRAVE_SEARCH_API_KEY}"),
                base_url: String::new(),
                model: String::new(),
                region: String::new(),
                max_results: 8,
            },
        ],
        ..PersistedWebSearchConfiguration::default()
    };

    assert!(configuration.normalize());
    assert_eq!(configuration.providers.len(), 1);
    assert_eq!(
        configuration.providers[0].kind,
        WebSearchProviderKind::Brave
    );
}

/// Verifies AI capability normalization removes unsupported vendors, unsupported targets, and stale route selectors in one pass.
/// 验证 AI 能力归一化会一次性移除未支持的供应商、未支持的目标以及过期的路由选择器。
#[test]
fn ai_tool_configuration_normalize_removes_unsupported_vendors_targets_and_routes() {
    let mut configuration = PersistedAiToolConfiguration {
        vendors: vec![
            PersistedAiCapabilityVendorEntry {
                id: String::from("duckduckgo"),
                vendor_id: String::from("duckduckgo"),
                name: String::from("DuckDuckGo"),
                enabled: true,
                api_key: String::new(),
                connection_mode: None,
                base_url: String::new(),
                region: String::new(),
                targets: vec![PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::WebSearch,
                    "duckduckgo",
                    "duckduckgo",
                )],
            },
            PersistedAiCapabilityVendorEntry {
                id: String::from("minimax"),
                vendor_id: String::from("minimax"),
                name: String::from("MiniMax"),
                enabled: true,
                api_key: String::from("{env:MINIMAX_API_KEY}"),
                connection_mode: None,
                base_url: String::new(),
                region: String::from("global"),
                targets: vec![
                    PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::WebSearch,
                        "minimax",
                        "minimax",
                    ),
                    PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::ImageGeneration,
                        "unsupported-target",
                        "unsupported-target",
                    ),
                ],
            },
        ],
        web_search: PersistedWebSearchConfiguration {
            target_order: vec![
                String::from("duckduckgo/duckduckgo"),
                String::from("minimax/minimax"),
                String::from("ghost/missing"),
            ],
            ..PersistedWebSearchConfiguration::default()
        },
        image_generation: PersistedAiCapabilityRouteConfiguration {
            target_order: vec![String::from("minimax/unsupported-target")],
        },
        ..PersistedAiToolConfiguration::default()
    };

    assert!(configuration.normalize());
    assert_eq!(configuration.vendors.len(), 1);
    assert_eq!(configuration.vendors[0].vendor_id, "minimax");
    assert_eq!(configuration.vendors[0].targets.len(), 1);
    assert_eq!(
        configuration.vendors[0].targets[0].target_alias(),
        "minimax"
    );
    assert_eq!(
        configuration.web_search.target_order,
        vec![String::from("minimax/minimax")]
    );
    assert!(configuration.image_generation.target_order.is_empty());
}

/// Verifies the shared MiniMax vendor exposes generation-capability targets through the unified target-choice catalog.
/// 验证共享 MiniMax 供应商会通过统一目标选项目录暴露生成类能力目标。
#[test]
fn ai_capability_vendor_target_choices_include_minimax_generation_targets() {
    let choices = super::ai_capability_vendor_target_choices("minimax");

    assert!(choices.iter().any(|choice| {
        choice.capability_kind == AiToolKind::ImageGeneration && choice.target_alias == "image-01"
    }));
    assert!(choices.iter().any(|choice| {
        choice.capability_kind == AiToolKind::VideoGeneration
            && choice.target_alias == "MiniMax-Hailuo-2.3"
    }));
    assert!(choices.iter().any(|choice| {
        choice.capability_kind == AiToolKind::SpeechSynthesis
            && choice.target_alias == "speech-2.8-hd"
    }));
    assert!(choices.iter().any(|choice| {
        choice.capability_kind == AiToolKind::MusicGeneration && choice.target_alias == "music-2.6"
    }));
    assert!(choices.iter().any(|choice| {
        choice.capability_kind == AiToolKind::MusicCover && choice.target_alias == "music-cover"
    }));
}

/// Verifies the shared Kimi vendor exposes each supported web-search model as one independently selectable target.
/// 验证共享 Kimi 供应商会把每个受支持的联网搜索模型都暴露成独立可选目标。
#[test]
fn ai_capability_vendor_target_choices_flatten_kimi_search_models() {
    let choices = super::ai_capability_vendor_target_choices("kimi");

    assert!(choices.iter().any(|choice| {
        choice.capability_kind == AiToolKind::WebSearch
            && choice.target_alias == "kimi-k2.5"
            && choice.default_model_id == "kimi-k2.5"
    }));
    assert!(choices.iter().any(|choice| {
        choice.capability_kind == AiToolKind::WebSearch
            && choice.target_alias == "kimi-k2-turbo-preview"
            && choice.default_model_id == "kimi-k2-turbo-preview"
    }));
    assert!(choices.iter().any(|choice| {
        choice.capability_kind == AiToolKind::WebSearch
            && choice.target_alias == "kimi-k2-thinking"
            && choice.default_model_id == "kimi-k2-thinking"
    }));
    assert!(choices.iter().any(|choice| {
        choice.capability_kind == AiToolKind::WebSearch
            && choice.target_alias == "kimi-k2-thinking-turbo"
            && choice.default_model_id == "kimi-k2-thinking-turbo"
    }));
}

/// Verifies deprecated single-target Kimi bindings are filtered out instead of being migrated for compatibility.
/// 验证废弃的单目标 Kimi 绑定会被直接过滤，而不是为了兼容继续迁移。
#[test]
fn normalize_filters_legacy_kimi_single_target_binding() {
    let mut configuration = PersistedAiToolConfiguration {
        vendors: vec![PersistedAiCapabilityVendorEntry {
            id: String::from("kimi"),
            vendor_id: String::from("kimi"),
            name: String::from("Kimi"),
            enabled: true,
            api_key: String::from("{env:MOONSHOT_API_KEY}"),
            connection_mode: None,
            base_url: String::new(),
            region: String::new(),
            targets: vec![PersistedAiCapabilityTargetBinding::new(
                AiToolKind::WebSearch,
                "kimi",
                "kimi-k2-thinking",
            )],
        }],
        web_search: PersistedWebSearchConfiguration {
            target_order: vec![String::from("kimi/kimi")],
            ..PersistedWebSearchConfiguration::default()
        },
        ..PersistedAiToolConfiguration::default()
    };

    assert!(configuration.normalize());
    assert_eq!(configuration.vendors.len(), 1);
    assert!(configuration.vendors[0].targets.is_empty());
    assert!(configuration.web_search.target_order.is_empty());
}

/// Verifies one semantic capability never exposes more than twenty enabled route targets to the runtime order.
/// 验证单个语义能力对运行时暴露的已启用途径目标永远不会超过二十条。
#[test]
fn usable_targets_for_kind_is_capped_at_twenty() {
    let mut vendor = PersistedAiCapabilityVendorEntry::new(String::from("limit-test"));
    vendor.enabled = true;
    for index in 0..25usize {
        vendor.targets.push(PersistedAiCapabilityTargetBinding::new(
            AiToolKind::WebSearch,
            format!("target-{index:02}"),
            format!("model-{index:02}"),
        ));
    }
    let configuration = PersistedAiToolConfiguration {
        vendors: vec![vendor],
        ..PersistedAiToolConfiguration::default()
    };

    let usable_targets = configuration.usable_targets_for_kind(AiToolKind::WebSearch);
    assert_eq!(
        usable_targets.len(),
        super::MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND
    );
    assert_eq!(
        usable_targets.first().map(String::as_str),
        Some("limit_test/target-00")
    );
    assert_eq!(
        usable_targets.last().map(String::as_str),
        Some("limit_test/target-19")
    );
}

/// Verifies only workbench quick-toggle kinds are filtered while explicit supported disables are preserved.
/// 验证仅过滤非工作台快捷开关类型，同时保留受支持能力的显式关闭状态。
#[test]
fn normalize_filters_unsupported_disabled_capability_kinds() {
    let mut configuration = PersistedAiToolConfiguration {
        disabled_capability_kinds: BTreeSet::from([
            AiToolKind::WebSearch,
            AiToolKind::UnderstandImage,
            AiToolKind::FileSearch,
            AiToolKind::CodeInterpreter,
        ]),
        ..PersistedAiToolConfiguration::default()
    };

    assert!(configuration.normalize());
    assert_eq!(
        configuration.disabled_capability_kinds,
        BTreeSet::from([AiToolKind::WebSearch, AiToolKind::UnderstandImage])
    );
}

/// Verifies explicit non-web disables are preserved when a matching host target is still configured.
/// 验证仍配置匹配宿主目标时，会保留非联网能力的显式关闭状态。
#[test]
fn normalize_preserves_non_web_disabled_capability_with_host_target() {
    let mut vendor = PersistedAiCapabilityVendorEntry::new(String::from("minimax"));
    vendor.enabled = true;
    vendor.targets.push(PersistedAiCapabilityTargetBinding::new(
        AiToolKind::ImageGeneration,
        "image-01",
        "image-01",
    ));
    let mut configuration = PersistedAiToolConfiguration {
        disabled_capability_kinds: BTreeSet::from([AiToolKind::ImageGeneration]),
        vendors: vec![vendor],
        ..PersistedAiToolConfiguration::default()
    };

    let _ = configuration.normalize();
    assert_eq!(
        configuration.disabled_capability_kinds,
        BTreeSet::from([AiToolKind::ImageGeneration])
    );
}

/// Verifies legacy Ollama vendor rows that only contain an API key automatically prefer the official cloud mode until the user makes an explicit selection.
/// 验证仅填写 API Key 的旧版 Ollama 供应商条目会自动优先切到官方云端模式，直到用户显式做出选择。
#[test]
fn ollama_vendor_with_api_key_defaults_to_cloud_mode() {
    let vendor = PersistedAiCapabilityVendorEntry {
        id: String::from("ollama"),
        vendor_id: String::from("ollama"),
        name: String::from("Ollama"),
        enabled: true,
        api_key: String::from("{env:OLLAMA_API_KEY}"),
        connection_mode: None,
        base_url: String::new(),
        region: String::new(),
        targets: vec![PersistedAiCapabilityTargetBinding::new(
            AiToolKind::WebSearch,
            "ollama",
            "ollama",
        )],
    };

    assert_eq!(
        vendor.resolved_connection_mode(),
        AiCapabilityVendorConnectionMode::Cloud
    );
    assert_eq!(vendor.resolved_vendor_base_url(), "https://ollama.com");
}

/// Verifies explicit Ollama local mode keeps the self-hosted endpoint and falls back to the bundled localhost host when no override is entered.
/// 验证显式 Ollama 本地模式会保留自托管端点，并在未填写覆盖地址时回退到内置 localhost 宿主。
#[test]
fn ollama_vendor_local_mode_resolves_local_endpoint() {
    let vendor = PersistedAiCapabilityVendorEntry {
        id: String::from("ollama"),
        vendor_id: String::from("ollama"),
        name: String::from("Ollama"),
        enabled: true,
        api_key: String::new(),
        connection_mode: Some(AiCapabilityVendorConnectionMode::Local),
        base_url: String::from("http://nasbox:11434"),
        region: String::new(),
        targets: vec![PersistedAiCapabilityTargetBinding::new(
            AiToolKind::WebSearch,
            "ollama",
            "ollama",
        )],
    };

    assert_eq!(
        vendor.resolved_connection_mode(),
        AiCapabilityVendorConnectionMode::Local
    );
    assert_eq!(vendor.resolved_vendor_base_url(), "http://nasbox:11434");

    let mut fallback_vendor = vendor.clone();
    fallback_vendor.base_url.clear();
    assert_eq!(
        fallback_vendor.resolved_vendor_base_url(),
        "http://127.0.0.1:11434"
    );
}

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

/// Verifies project-local configuration files round-trip through the shared provider config helpers.
/// 验证项目本地配置文件可以通过共享 Provider 配置辅助函数完成读写往返。
#[test]
fn project_configuration_file_roundtrips_prompt_settings() {
    let root = unique_temp_dir("vulcan-project-config-roundtrip");
    let config = ProjectVulcanCodeConfigurationFile {
        prompt: Some(super::PersistedPromptConfiguration {
            project_prompt_enabled: false,
            project_prompt_path: String::from(".vulcan/code/local-prompt.md"),
            ..super::PersistedPromptConfiguration::default()
        }),
        prompt_skills: Default::default(),
        vmm: Default::default(),
    };

    save_project_configuration_file(&root, &config).expect("save project config");
    let loaded = load_project_configuration_file(&root).expect("load project config");

    assert_eq!(loaded, config);
    assert!(
        project_configuration_file_path(&root)
            .to_string_lossy()
            .ends_with(".vulcan\\code\\config.json")
            || project_configuration_file_path(&root)
                .to_string_lossy()
                .ends_with(".vulcan/code/config.json")
    );

    let _ = fs::remove_dir_all(root);
}

/// Verifies project-local VMM overrides round-trip through the shared project configuration helpers.
/// 验证项目级 VMM 覆盖配置可以通过共享项目配置辅助函数完成读写往返。
#[test]
fn project_configuration_file_roundtrips_vmm_settings() {
    let root = unique_temp_dir("vulcan-project-vmm-config-roundtrip");
    let config = ProjectVulcanCodeConfigurationFile {
        prompt: None,
        prompt_skills: Default::default(),
        vmm: VmmConfigLayer {
            enabled: Some(false),
            project_id: Some(1001),
            receive_timeout_ms: Some(30_000),
            implicit_memory_turns: Some(5),
            ..VmmConfigLayer::default()
        },
    };

    save_project_configuration_file(&root, &config).expect("save project config");
    let loaded = load_project_configuration_file(&root).expect("load project config");

    assert_eq!(loaded, config);
    assert_eq!(loaded.vmm.project_id, Some(1001));
    assert_eq!(loaded.vmm.receive_timeout_ms, Some(30_000));

    let _ = fs::remove_dir_all(root);
}

/// Verifies remote catalog refresh only auto-enables newly discovered default models and keeps previously disabled default models disabled.
/// 验证远端目录刷新只会自动启用新发现的默认模型，并保持此前被禁用的默认模型继续处于关闭状态。
#[test]
fn merge_refreshed_models_only_auto_enables_new_default_models() {
    let mut old_default_model = test_model_choice("default-model", true, None);
    old_default_model.default_enabled = true;
    let manual_model = test_model_choice("manual-model", true, None);
    let mut new_default_model = test_model_choice("new-default-model", true, None);
    new_default_model.default_enabled = true;

    let mut entry = PersistedProviderEntry::default();
    entry.models = persist_remote_models(&[old_default_model.clone(), manual_model.clone()]);
    entry.enabled_models = Some(vec![String::from("manual-model")]);

    merge_refreshed_models(
        &mut entry,
        &[old_default_model, manual_model, new_default_model],
    );

    assert_eq!(
        entry.enabled_models,
        Some(vec![
            String::from("manual-model"),
            String::from("new-default-model"),
        ])
    );
}
