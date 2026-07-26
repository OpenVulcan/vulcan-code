use super::super::{
    AiCapabilityVendorConnectionMode, AiToolKind, PersistedAiCapabilityRouteConfiguration,
    PersistedAiCapabilityTargetBinding, PersistedAiCapabilityVendorEntry,
    PersistedAiToolConfiguration, PersistedWebSearchConfiguration, PersistedWebSearchProviderEntry,
    WebSearchProviderKind, ai_capability_vendor_target_choices,
};
use std::collections::BTreeSet;

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
            selected_target: Some(String::from("ghost/missing")),
            ..PersistedWebSearchConfiguration::default()
        },
        image_generation: PersistedAiCapabilityRouteConfiguration {
            selected_target: Some(String::from("minimax/unsupported-target")),
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
    assert!(configuration.web_search.selected_target.is_none());
    assert!(configuration.image_generation.selected_target.is_none());
}

/// Verifies the shared MiniMax vendor exposes generation-capability targets through the unified target-choice catalog.
/// 验证共享 MiniMax 供应商会通过统一目标选项目录暴露生成类能力目标。
#[test]
fn ai_capability_vendor_target_choices_include_minimax_generation_targets() {
    let choices = ai_capability_vendor_target_choices("minimax");

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
    let choices = ai_capability_vendor_target_choices("kimi");

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
            selected_target: Some(String::from("kimi/kimi")),
            ..PersistedWebSearchConfiguration::default()
        },
        ..PersistedAiToolConfiguration::default()
    };

    assert!(configuration.normalize());
    assert_eq!(configuration.vendors.len(), 1);
    assert!(configuration.vendors[0].targets.is_empty());
    assert!(configuration.web_search.selected_target.is_none());
}

/// Verifies unbound configured targets are never projected into an implicit execution binding.
/// 验证未绑定的已配置目标绝不会被投影成隐式执行绑定。
#[test]
fn usable_targets_for_kind_requires_one_explicit_binding() {
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
    assert!(usable_targets.is_empty());
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
