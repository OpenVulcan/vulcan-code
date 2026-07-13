use super::super::{
    AiToolKind, PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
    PersistedAiToolConfiguration, PersistedLuaSkillToolRegistrationState,
    PersistedLuaSkillsConfiguration, PersistedPromptSkillState, PersistedPromptSkillsConfiguration,
    PersistedWebSearchConfiguration, PersistedWebSearchProviderEntry, ProviderConfigurationFile,
    ReasoningEffortLevel, WebSearchProviderDispatchMode, WebSearchProviderKind,
    WebSearchRoutingMode, WorkbenchThinkingDisplayMode,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_vmm::VmmConfigLayer;

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

/// Verifies the remembered workbench thinking display mode keeps the stable camelCase field name and round-trips through the shared provider configuration file.
/// 验证已记忆的工作台思考展示模式会保持稳定的 camelCase 字段名，并且能够经由共享 Provider 配置文件完成往返序列化。
#[test]
fn provider_configuration_round_trips_workbench_thinking_display_mode() {
    let config = ProviderConfigurationFile {
        workbench_thinking_display_mode: Some(WorkbenchThinkingDisplayMode::FirstInlineWithPopup),
        ..ProviderConfigurationFile::default()
    };

    let value = serde_json::to_value(&config).expect("provider configuration must serialize");
    assert_eq!(
        value.get("workbenchThinkingDisplayMode"),
        Some(&json!("first_inline_with_popup"))
    );

    let restored: ProviderConfigurationFile =
        serde_json::from_value(value).expect("provider configuration must deserialize");
    assert_eq!(
        restored.workbench_thinking_display_mode,
        Some(WorkbenchThinkingDisplayMode::FirstInlineWithPopup)
    );
}

/// Verifies the persisted WebUI locale keeps the stable `uiLocale` field name and round-trips through the shared provider configuration file.
/// 验证持久化WebUI语言会保持稳定的 `uiLocale` 字段名，并且能够经由共享 Provider 配置文件完成往返序列化。
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
            disabled_system_plugins: BTreeSet::from([String::from("test-system-plugin")]),
            enabled_system_plugins: BTreeSet::from([String::from("opt-in-system-plugin")]),
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
    assert_eq!(
        value["luaskills"]["disabledSystemPlugins"],
        json!(["test-system-plugin"])
    );
    assert_eq!(
        value["luaskills"]["enabledSystemPlugins"],
        json!(["opt-in-system-plugin"])
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
    assert!(
        !restored
            .luaskills
            .system_plugin_enabled_or_default("test-system-plugin", true)
    );
    assert!(
        restored
            .luaskills
            .system_plugin_enabled_or_default("opt-in-system-plugin", false)
    );
}

/// Verifies prompt skills configuration round-trips independently from LuaSkills.
/// 验证提示词技能配置可以独立于 LuaSkills 完整往返。
#[test]
fn provider_configuration_round_trips_prompt_skills_configuration() {
    let config = ProviderConfigurationFile {
        prompt_skills: PersistedPromptSkillsConfiguration {
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
        disabled_system_plugins: BTreeSet::from([
            String::new(),
            String::from(" test-system-plugin "),
        ]),
        enabled_system_plugins: BTreeSet::new(),
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
    assert_eq!(
        configuration.disabled_system_plugins,
        BTreeSet::from([String::from("test-system-plugin")])
    );
    assert!(!configuration.system_plugin_enabled_or_default("test-system-plugin", true));
    assert!(configuration.set_system_plugin_enabled_with_default("test-system-plugin", true, true));
    assert!(configuration.system_plugin_enabled_or_default("test-system-plugin", true));
    assert!(configuration.set_system_plugin_enabled_with_default(
        "opt-in-system-plugin",
        true,
        false
    ));
    assert!(configuration.system_plugin_enabled_or_default("opt-in-system-plugin", false));
}
