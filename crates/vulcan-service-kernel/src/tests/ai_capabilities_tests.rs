//! ai capabilities tests.
//! ai capabilities tests。

use super::*;

/// Verifies image understanding remains available through the native model path when no host target is configured.
/// 验证未配置宿主目标时，图片理解仍会通过模型原生路径显示为可用。
#[test]
fn ai_capabilities_marks_understand_image_native_for_vision_model() {
    let configuration = PersistedAiToolConfiguration::default();
    let model_capabilities = ResolvedModelCapabilities {
        input_modalities: vec![String::from("text"), String::from("image")],
        ..ResolvedModelCapabilities::default()
    };
    let summaries = ai_capabilities(&configuration, Some(&model_capabilities));
    let image_capability = summaries
        .iter()
        .find(|summary| summary.kind == AiToolKind::UnderstandImage.key())
        .expect("image-understanding capability summary should exist");

    assert!(image_capability.available);
    assert!(image_capability.enabled);
    assert!(image_capability.native);
    assert!(!image_capability.bridged);
    assert!(image_capability.targets.is_empty());
}

/// Verifies an exact host binding remains visible beside native image capability evidence.
/// 验证精确宿主绑定会与原生图片能力证据同时可见。
#[test]
fn ai_capabilities_marks_exact_host_binding_for_vision_model() {
    let configuration = test_understand_image_ai_tool_configuration();
    let model_capabilities = ResolvedModelCapabilities {
        input_modalities: vec![String::from("text"), String::from("image")],
        ..ResolvedModelCapabilities::default()
    };
    let summaries = ai_capabilities(&configuration, Some(&model_capabilities));
    let image_capability = summaries
        .iter()
        .find(|summary| summary.kind == AiToolKind::UnderstandImage.key())
        .expect("image-understanding capability summary should exist");

    assert!(image_capability.available);
    assert!(image_capability.enabled);
    assert!(image_capability.native);
    assert!(image_capability.bridged);
    assert_eq!(image_capability.targets.len(), 1);
    assert!(image_capability.targets[0].selected);
}

/// Verifies disabling a capability also disables its native path for the session.
/// 验证禁用一项能力也会在当前会话中禁用其原生路径。
#[test]
fn ai_capabilities_disables_native_understand_image_when_explicitly_disabled() {
    let mut configuration = test_understand_image_ai_tool_configuration();
    configuration
        .disabled_capability_kinds
        .insert(AiToolKind::UnderstandImage);
    let model_capabilities = ResolvedModelCapabilities {
        input_modalities: vec![String::from("text"), String::from("image")],
        ..ResolvedModelCapabilities::default()
    };
    let summaries = ai_capabilities(&configuration, Some(&model_capabilities));
    let image_capability = summaries
        .iter()
        .find(|summary| summary.kind == AiToolKind::UnderstandImage.key())
        .expect("image-understanding capability summary should exist");

    assert!(image_capability.available);
    assert!(!image_capability.enabled);
    assert!(image_capability.native);
    assert!(image_capability.bridged);
}

/// Verifies image understanding reports the host bridge path when a configured target exists for a text-only model.
/// 验证纯文本模型存在已配置目标时，图片理解会报告宿主桥接路径。
#[test]
fn ai_capabilities_marks_understand_image_bridged_for_text_model() {
    let configuration = test_understand_image_ai_tool_configuration();
    let model_capabilities = ResolvedModelCapabilities {
        input_modalities: vec![String::from("text")],
        ..ResolvedModelCapabilities::default()
    };
    let summaries = ai_capabilities(&configuration, Some(&model_capabilities));
    let image_capability = summaries
        .iter()
        .find(|summary| summary.kind == AiToolKind::UnderstandImage.key())
        .expect("image-understanding capability summary should exist");

    assert!(image_capability.available);
    assert!(image_capability.enabled);
    assert!(!image_capability.native);
    assert!(image_capability.bridged);
    assert_eq!(image_capability.targets.len(), 1);
}

/// Verifies retaining the same exact target binding does not create a model-facing notice.
/// 验证保持相同精确目标绑定时不会创建面向模型的通知。
#[test]
fn ai_capability_update_injection_ignores_unchanged_exact_binding() {
    let previous_configuration = test_web_search_ai_tool_configuration("search/minimax");
    let next_configuration = test_web_search_ai_tool_configuration("search/minimax");

    let injection = session_ai_capability_configuration_update_injection(
        &previous_configuration,
        &next_configuration,
        None,
    );

    assert!(
        injection.is_none(),
        "an unchanged exact binding should not affect model-facing tool guidance"
    );
}

/// Verifies disabling an explicitly selected image capability remains model-visible for native models.
/// 验证对原生模型禁用显式选择的图片能力时，变化仍对模型可见。
#[test]
fn ai_capability_update_injection_reports_image_disable_for_native_model() {
    let previous_configuration = test_understand_image_ai_tool_configuration();
    let mut next_configuration = previous_configuration.clone();
    next_configuration
        .disabled_capability_kinds
        .insert(AiToolKind::UnderstandImage);
    let model_capabilities = ResolvedModelCapabilities {
        input_modalities: vec![String::from("text"), String::from("image")],
        ..ResolvedModelCapabilities::default()
    };

    let injection = session_ai_capability_configuration_update_injection(
        &previous_configuration,
        &next_configuration,
        Some(&model_capabilities),
    );

    let injection = injection.expect("explicit disable should create a model-facing notice");
    assert!(
        injection.items[0]
            .content
            .contains("state changed: DISABLED")
    );
}

/// Verifies exact host binding changes remain visible when the model also has native web search.
/// 验证模型同时具备原生联网搜索时，精确宿主绑定变化仍保持可见。
#[test]
fn ai_capability_update_injection_reports_web_search_binding_change_for_native_model() {
    let previous_configuration = test_web_search_ai_tool_configuration("search/minimax");
    let next_configuration = test_web_search_ai_tool_configuration("search/ollama");
    let model_capabilities = ResolvedModelCapabilities {
        ai_tools: vec![ResolvedAiToolCapability {
            kind: AiToolKind::WebSearch,
            policy: AiToolPolicy::ForceNative,
            native_provider_tool: true,
            native_tool_type: Some(String::from("web_search_preview")),
            native_request_patch: None,
            host_tool_id: None,
            requires_approval: false,
            result_modalities: Vec::new(),
            citation_support: true,
            source_label: String::from("test-native"),
        }],
        ..ResolvedModelCapabilities::default()
    };

    let injection = session_ai_capability_configuration_update_injection(
        &previous_configuration,
        &next_configuration,
        Some(&model_capabilities),
    );

    let injection = injection.expect("explicit host binding change should create a notice");
    assert!(injection.items[0].content.contains("default route changed"));
}

/// Verifies changing the exact bound target creates one target-binding notice.
/// 验证变更精确绑定目标时会创建一条目标绑定通知。
#[test]
fn ai_capability_update_injection_reports_default_route_change() {
    let previous_configuration = test_web_search_ai_tool_configuration("search/minimax");
    let next_configuration = test_web_search_ai_tool_configuration("search/ollama");

    let injection = session_ai_capability_configuration_update_injection(
        &previous_configuration,
        &next_configuration,
        None,
    )
    .expect("default route change should create injection");
    let content = &injection.items[0].content;

    assert!(content.contains("default route changed"));
    assert!(content.contains("`search/minimax` using model `minimax-model`"));
    assert!(content.contains("`search/ollama` using model `ollama-model`"));
    assert_eq!(
        content
            .matches("- `web_search` (网页搜索): default route changed")
            .count(),
        1
    );
}

/// Verifies one capability combines switch and default-route changes into one compaction-scoped persistent injection.
/// 验证同一能力会将开关与默认路由变化合并进一条压缩作用域持久注入。
#[test]
fn ai_capability_update_injection_combines_state_and_default_route_changes_until_compaction() {
    let previous_configuration = test_web_search_ai_tool_configuration("search/minimax");
    let mut next_configuration = test_web_search_ai_tool_configuration("search/ollama");
    next_configuration
        .disabled_capability_kinds
        .insert(AiToolKind::WebSearch);

    let injection = session_ai_capability_configuration_update_injection(
        &previous_configuration,
        &next_configuration,
        None,
    )
    .expect("combined change should create injection");
    let content = &injection.items[0].content;

    assert_eq!(injection.kind, InjectionKind::Persistent);
    assert_eq!(injection.ttl_turns, None);
    assert_eq!(
        injection.compaction_policy,
        InjectionCompactionPolicy::Clear
    );
    assert_eq!(content.matches("- `web_search`").count(), 1);
    assert!(content.contains("state changed: DISABLED"));
    assert!(content.contains("default route changed"));

    let mut state = ContextInjectionState::default();
    state.apply_insert(injection, "1");
    let group = state
        .groups
        .get(SESSION_AI_CAPABILITY_UPDATE_INJECTION_TAG)
        .expect("injection group should exist before compaction");
    let item = group.items.values().next().expect("injection item");
    assert_eq!(group.kind, InjectionKind::Persistent);
    assert_eq!(item.ttl_turns, None);
    assert_eq!(item.compaction_policy, InjectionCompactionPolicy::Clear);

    state.apply_compaction();

    assert!(
        !state
            .groups
            .contains_key(SESSION_AI_CAPABILITY_UPDATE_INJECTION_TAG),
        "context compaction should clear runtime capability update notices"
    );
}

/// Verifies session dialog disabled target selectors update matching target bindings case-insensitively.
/// 验证会话弹窗禁用目标选择器会以大小写不敏感方式更新匹配的目标 binding。
#[test]
fn session_ai_capability_dialog_draft_disables_matching_target_selector() {
    let mut configuration = test_understand_image_ai_tool_configuration();

    apply_session_ai_capability_dialog_draft(
        &mut configuration,
        &std::collections::BTreeSet::new(),
        &std::collections::BTreeSet::from([String::from("MINIMAX/MINIMAX")]),
        &std::collections::BTreeMap::new(),
    )
    .expect("valid dialog draft should apply");

    let minimax_vendor = configuration
        .vendors
        .iter()
        .find(|vendor| vendor.vendor_id == "minimax")
        .expect("minimax vendor should remain configured");
    let minimax_binding = minimax_vendor
        .targets
        .iter()
        .find(|binding| binding.target_alias() == "minimax")
        .expect("minimax target should remain configured");

    assert!(!minimax_binding.enabled);
}

/// Builds a minimal usable image-understanding host route for WebUI capability summary tests.
/// 为 WebUI 能力摘要测试构建一条最小可用的图片理解宿主路由。
fn test_understand_image_ai_tool_configuration() -> PersistedAiToolConfiguration {
    PersistedAiToolConfiguration {
        vendors: vec![PersistedAiCapabilityVendorEntry {
            id: String::from("minimax"),
            vendor_id: String::from("minimax"),
            name: String::from("MiniMax"),
            enabled: true,
            api_key: String::from("token"),
            connection_mode: None,
            base_url: String::new(),
            region: String::from("global"),
            targets: vec![PersistedAiCapabilityTargetBinding::new(
                AiToolKind::UnderstandImage,
                "minimax",
                "minimax",
            )],
        }],
        understand_image: vulcan_provider_config::config::PersistedUnderstandImageConfiguration {
            selected_target: Some(String::from("minimax/minimax")),
            ..vulcan_provider_config::config::PersistedUnderstandImageConfiguration::default()
        },
        ..PersistedAiToolConfiguration::default()
    }
}

/// Builds a web-search configuration with one explicitly selected host target.
/// 构建一份带有单个显式选择宿主目标的网页搜索配置。
fn test_web_search_ai_tool_configuration(selected_target: &str) -> PersistedAiToolConfiguration {
    PersistedAiToolConfiguration {
        vendors: vec![PersistedAiCapabilityVendorEntry {
            id: String::from("search"),
            vendor_id: String::from("search"),
            name: String::from("Search"),
            enabled: true,
            api_key: String::from("token"),
            connection_mode: None,
            base_url: String::new(),
            region: String::from("global"),
            targets: vec![
                PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::WebSearch,
                    "minimax",
                    "minimax-model",
                ),
                PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::WebSearch,
                    "ollama",
                    "ollama-model",
                ),
                PersistedAiCapabilityTargetBinding::new(
                    AiToolKind::WebSearch,
                    "tavily",
                    "tavily-model",
                ),
            ],
        }],
        web_search: vulcan_provider_config::config::PersistedWebSearchConfiguration {
            selected_target: Some(selected_target.to_string()),
            ..vulcan_provider_config::config::PersistedWebSearchConfiguration::default()
        },
        ..PersistedAiToolConfiguration::default()
    }
}
