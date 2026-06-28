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

/// Verifies configured bridge targets do not mark image understanding as bridged when native image input is active.
/// 验证原生图片输入启用时，已配置桥接目标不会让图片理解被标记为桥接态。
#[test]
fn ai_capabilities_marks_understand_image_native_not_bridged_for_vision_model() {
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
    assert!(!image_capability.bridged);
    assert_eq!(image_capability.targets.len(), 1);
}

/// Verifies disabling the bridge does not disable native image-understanding capability summaries.
/// 验证关闭桥接不会禁用原生图片理解能力摘要。
#[test]
fn ai_capabilities_keeps_understand_image_enabled_when_native_bridge_disabled() {
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
    assert!(image_capability.enabled);
    assert!(image_capability.native);
    assert!(!image_capability.bridged);
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

/// Verifies reordering non-default targets does not create a model-facing runtime notice.
/// 验证仅重排非默认目标时不会创建面向模型的运行时通知。
#[test]
fn ai_capability_update_injection_ignores_non_default_route_reorder() {
    let previous_configuration = test_web_search_ai_tool_configuration(&[
        "search/minimax",
        "search/ollama",
        "search/tavily",
    ]);
    let next_configuration = test_web_search_ai_tool_configuration(&[
        "search/minimax",
        "search/tavily",
        "search/ollama",
    ]);

    let injection = session_ai_capability_configuration_update_injection(
        &previous_configuration,
        &next_configuration,
        None,
    );

    assert!(
        injection.is_none(),
        "non-default route swaps should not affect model-facing tool guidance"
    );
}

/// Verifies bridge setting changes do not create notices when the active model has native image input.
/// 验证当前模型具备原生图片输入时，桥接设置变化不会创建通知。
#[test]
fn ai_capability_update_injection_ignores_image_bridge_changes_for_native_model() {
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

    assert!(
        injection.is_none(),
        "native image input should make bridge toggles model-invisible"
    );
}

/// Verifies default-route changes do not create notices when the active model has native web search.
/// 验证当前模型具备原生联网搜索时，默认路由变化不会创建通知。
#[test]
fn ai_capability_update_injection_ignores_web_search_route_changes_for_native_model() {
    let previous_configuration =
        test_web_search_ai_tool_configuration(&["search/minimax", "search/ollama"]);
    let next_configuration =
        test_web_search_ai_tool_configuration(&["search/ollama", "search/minimax"]);
    let model_capabilities = ResolvedModelCapabilities {
        ai_tools: vec![ResolvedAiToolCapability {
            kind: AiToolKind::WebSearch,
            policy: AiToolPolicy::ForceNative,
            native_provider_tool: true,
            native_tool_type: Some(String::from("web_search_preview")),
            native_request_patch: None,
            fallback_tool_id: None,
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

    assert!(
        injection.is_none(),
        "native web search should make bridge route changes model-invisible"
    );
}

/// Verifies changing the first enabled target creates one default-route notice.
/// 验证首个已启用目标变化时会创建一条默认路由通知。
#[test]
fn ai_capability_update_injection_reports_default_route_change() {
    let previous_configuration =
        test_web_search_ai_tool_configuration(&["search/minimax", "search/ollama"]);
    let next_configuration =
        test_web_search_ai_tool_configuration(&["search/ollama", "search/minimax"]);

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
    let previous_configuration =
        test_web_search_ai_tool_configuration(&["search/minimax", "search/ollama"]);
    let mut next_configuration =
        test_web_search_ai_tool_configuration(&["search/ollama", "search/minimax"]);
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
        state
            .groups
            .get(SESSION_AI_CAPABILITY_UPDATE_INJECTION_TAG)
            .is_none(),
        "context compaction should clear runtime capability update notices"
    );
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
            target_order: vec![String::from("minimax/minimax")],
            ..vulcan_provider_config::config::PersistedUnderstandImageConfiguration::default()
        },
        ..PersistedAiToolConfiguration::default()
    }
}

/// Builds a web-search configuration with ordered enabled host targets.
/// 构建一份带有有序启用宿主目标的网页搜索配置。
fn test_web_search_ai_tool_configuration(target_order: &[&str]) -> PersistedAiToolConfiguration {
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
            target_order: target_order
                .iter()
                .map(|target| target.to_string())
                .collect(),
            ..vulcan_provider_config::config::PersistedWebSearchConfiguration::default()
        },
        ..PersistedAiToolConfiguration::default()
    }
}
