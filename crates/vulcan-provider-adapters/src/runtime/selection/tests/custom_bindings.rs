//! Custom protocol preferences, frozen bindings, and explicit re-selection.
//! 自定义协议偏好、冻结绑定与显式重新选择。

use crate::runtime::PublicAiRuntime;
use crate::runtime::test_support::{
    custom_connection, install_current_custom_provider, install_source_catalog,
    multi_protocol_custom_definition, with_protocol_variants,
};
use crate::{ModelCapabilityEvidence, PersistedAiConfiguration, parse_openai_model_catalog};
use std::sync::Arc;
use vulcan_ai_contract::{CapabilitySupport, ModelCapabilities};
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_RESPONSES_PROFILE_ID,
};

/// Verifies multi-protocol Auto resolves responses first, then anthropic, then chat.
/// 验证多协议 Auto 依次解析 responses、anthropic、chat。
#[test]
fn custom_multi_protocol_auto_resolves_by_fixed_priority() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = multi_protocol_custom_definition(
        "custom:multi",
        &[
            crate::CustomProviderProtocol::OpenAiChat,
            crate::CustomProviderProtocol::AnthropicMessages,
            crate::CustomProviderProtocol::OpenAiResponses,
        ],
    );
    install_current_custom_provider(&mut configuration, provider);
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("multi-account", "custom:multi"),
        )
        .expect("multi connection should register");
    let catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "multi-account",
            1,
            r#"{"data":[{"id":"auto/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("auto catalog should parse"),
        &[
            OPENAI_CHAT_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
        ],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("auto catalog should install");
    configuration.set_model_enabled("custom:multi", "auto/model", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("custom auto generation should publish");

    let published_catalogs = runtime
        .catalog_snapshot()
        .expect("published custom catalogs should remain readable");
    let published_model = published_catalogs
        .get("multi-account")
        .and_then(|catalog| catalog.models.first())
        .expect("the custom model must remain published");
    let published_binding = published_model
        .resolved_protocol_binding
        .as_ref()
        .expect("the custom catalog must freeze its auto binding");
    assert_eq!(
        published_binding.protocol_profile_id,
        OPENAI_RESPONSES_PROFILE_ID
    );
    assert_eq!(published_model.protocol_variants.len(), 3);
    let published_variant = published_model
        .protocol_variants
        .get(OPENAI_RESPONSES_PROFILE_ID)
        .expect("the auto-bound Responses variant must exist");
    assert_eq!(published_model.capabilities, published_variant.capabilities);
    assert_eq!(
        published_model.capability_evidence,
        published_variant.capability_evidence
    );

    let auto = runtime
        .resolve_connection_model_selection(
            &configuration,
            "multi-account",
            "custom:multi",
            "auto/model",
        )
        .expect("auto model must resolve");
    let binding = &auto.protocol_binding;
    assert_eq!(
        binding.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
        "auto must pick the highest fixed-priority protocol even when declared last"
    );
    assert_eq!(
        binding.source,
        crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
    );
    assert_eq!(
        auto.route.protocol_route.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
        "the effective execution protocol must follow the auto binding"
    );
}

/// Verifies Auto intersects provider protocols with this exact model's discovered variants.
/// 验证 Auto 会把供应商协议与当前模型实际发现的协议变体取交集。
#[test]
fn custom_multi_protocol_auto_uses_exact_model_available_protocols() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = multi_protocol_custom_definition(
        "custom:model-intersection",
        &[
            crate::CustomProviderProtocol::OpenAiResponses,
            crate::CustomProviderProtocol::AnthropicMessages,
        ],
    );
    install_current_custom_provider(&mut configuration, provider);
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("model-intersection-account", "custom:model-intersection"),
        )
        .expect("multi-protocol connection should register");
    let catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "model-intersection-account",
            1,
            r#"{"data":[{"id":"anthropic-only/model","input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("model-intersection catalog should parse"),
        &[ANTHROPIC_MESSAGES_PROFILE_ID],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("model-intersection catalog should install");
    configuration.set_model_enabled("custom:model-intersection", "anthropic-only/model", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("model-intersection catalog should publish");

    let selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "model-intersection-account",
            "custom:model-intersection",
            "anthropic-only/model",
        )
        .expect("the model's discovered Anthropic route must remain executable");
    let binding = selection.protocol_binding;
    assert_eq!(
        binding.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID,
        "provider-wide Responses support must not fabricate Responses support for this model"
    );
    assert_eq!(
        binding.source,
        crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
    );
    let published_catalogs = runtime.catalog_snapshot().expect("catalog snapshot");
    let published_model = published_catalogs["model-intersection-account"]
        .models
        .first()
        .expect("published model");
    assert_eq!(published_model.protocol_variants.len(), 1);
    assert!(
        published_model
            .protocol_variants
            .contains_key(ANTHROPIC_MESSAGES_PROFILE_ID)
    );
}

/// Verifies a custom Gemini binding projects Gemini evidence instead of the connection default.
/// 验证自定义 Gemini 绑定投影 Gemini 证据，而不是连接默认协议证据。
#[test]
fn custom_protocol_variant_policy_follows_effective_binding() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = multi_protocol_custom_definition(
        "custom:variant-policy",
        &[
            crate::CustomProviderProtocol::OpenAiResponses,
            crate::CustomProviderProtocol::GeminiGenerateContent,
        ],
    );
    install_current_custom_provider(&mut configuration, provider);
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("variant-policy-account", "custom:variant-policy"),
        )
        .expect("custom connection should register");
    configuration.custom_model_protocol_preferences.insert(
        PersistedAiConfiguration::provider_model_key("custom:variant-policy", "variant/model"),
        crate::CustomModelProtocolPreference::Explicit {
            protocol: crate::CustomProviderProtocol::GeminiGenerateContent,
        },
    );
    let catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "variant-policy-account",
            1,
            r#"{"data":[{"id":"variant/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("custom catalog should parse"),
        &[
            OPENAI_RESPONSES_PROFILE_ID,
            GEMINI_GENERATE_CONTENT_PROFILE_ID,
        ],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("custom catalog should install");
    configuration.set_model_enabled("custom:variant-policy", "variant/model", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("protocol-aware variants should publish");

    let published_catalogs = runtime
        .catalog_snapshot()
        .expect("published catalogs should remain readable");
    let model = published_catalogs
        .get("variant-policy-account")
        .and_then(|catalog| catalog.models.first())
        .expect("the custom model must remain published");
    let binding = model
        .resolved_protocol_binding
        .as_ref()
        .expect("the explicit Gemini binding must freeze");
    assert_eq!(
        binding.protocol_profile_id,
        GEMINI_GENERATE_CONTENT_PROFILE_ID
    );
    let responses = model
        .protocol_variants
        .get(OPENAI_RESPONSES_PROFILE_ID)
        .expect("the Responses variant must exist");
    let gemini = model
        .protocol_variants
        .get(GEMINI_GENERATE_CONTENT_PROFILE_ID)
        .expect("the Gemini variant must exist");
    assert_eq!(responses.capabilities.reasoning, CapabilitySupport::Native);
    assert_ne!(gemini.capabilities.reasoning, CapabilitySupport::Native);
    assert_eq!(model.capabilities, gemini.capabilities);
    assert_eq!(model.capability_evidence, gemini.capability_evidence);
}

/// Verifies custom bindings freeze at the catalog activation boundary: a preference
/// change without a catalog rebuild never re-binds an in-flight session, and the next
/// activation-boundary rebuild applies it (plan §13.6).
/// 验证自定义绑定在目录激活边界冻结：未重建目录的偏好变更绝不会重新绑定进行中的
/// 会话，下一次激活边界重建才应用它（方案 §13.6）。
#[test]
fn frozen_custom_binding_only_rebinds_at_activation_boundary() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = multi_protocol_custom_definition(
        "custom:frozen",
        &[
            crate::CustomProviderProtocol::OpenAiChat,
            crate::CustomProviderProtocol::AnthropicMessages,
            crate::CustomProviderProtocol::OpenAiResponses,
        ],
    );
    install_current_custom_provider(&mut configuration, provider);
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("frozen-account", "custom:frozen"),
        )
        .expect("frozen connection should register");
    let catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "frozen-account",
            1,
            r#"{"data":[{"id":"frozen/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("frozen catalog should parse"),
        &[
            OPENAI_CHAT_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
        ],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("frozen catalog should install");
    configuration.set_model_enabled("custom:frozen", "frozen/model", true);
    assert!(
        runtime
            .catalog_snapshot()
            .expect("published catalogs should remain readable")
            .is_empty(),
        "a staged effective catalog must not leak through the published snapshot"
    );
    assert!(
        runtime
            .source_catalog_snapshot()
            .expect("published source catalogs should remain readable")
            .is_empty(),
        "a staged source catalog must not leak through the published snapshot"
    );
    assert!(
        runtime
            .staged_source_catalog("frozen-account")
            .expect("staged validation catalog should remain readable")
            .is_some(),
        "the validation-only staged accessor must retain the exact candidate"
    );
    assert!(
        !runtime
            .model_is_available(&configuration, "custom:frozen", "frozen/model")
            .expect("published availability should resolve"),
        "model availability must not combine staged catalogs with published manifests"
    );
    assert!(
        runtime
            .available_model_selectors(&configuration)
            .expect("published selectors should resolve")
            .is_empty(),
        "selector enumeration must not expose an unactivated catalog"
    );
    assert!(
        runtime
            .resolved_model_catalog_snapshot(&configuration)
            .expect("published overlay reconstruction should remain readable")
            .is_empty(),
        "support-catalog reconstruction must not read a staged discovery source"
    );
    assert!(
        !runtime
            .provider_conversation_model_is_supported(
                &configuration,
                "custom:frozen",
                "frozen/model",
            )
            .expect("published provider support should resolve"),
        "provider support must not combine staged sources with a published manifest generation"
    );
    // The activation boundary freezes the auto binding (highest fixed priority).
    // 激活边界冻结 Auto 绑定（最高固定优先级）。
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("overlays should reapply");
    let binding = runtime
        .resolve_connection_model_selection(
            &configuration,
            "frozen-account",
            "custom:frozen",
            "frozen/model",
        )
        .expect("frozen model must resolve")
        .protocol_binding;
    assert_eq!(binding.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID);

    // A preference edit remains pending until the explicit activation below rebuilds its binding.
    // 偏好编辑保持待处理状态，直到下方显式激活重建其绑定。
    configuration.custom_model_protocol_preferences.insert(
        PersistedAiConfiguration::provider_model_key("custom:frozen", "frozen/model"),
        crate::CustomModelProtocolPreference::Explicit {
            protocol: crate::CustomProviderProtocol::AnthropicMessages,
        },
    );
    // The preference only takes effect after the next explicit activation boundary.
    // 偏好只在下一次显式激活边界后生效。
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("overlays should reapply");
    let rebound = runtime
        .resolve_connection_model_selection(
            &configuration,
            "frozen-account",
            "custom:frozen",
            "frozen/model",
        )
        .expect("frozen model must rebind")
        .protocol_binding;
    assert_eq!(
        rebound.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID,
        "a preference drift must re-freeze the binding at the activation boundary"
    );
    assert_eq!(
        rebound.source,
        crate::provider_manifest::ModelProtocolBindingSource::CustomUser
    );

    // Removing the explicit preference re-freezes Auto; the explicit binding never
    // survives a configuration activation that no longer declares it.
    // 移除显式偏好会重新冻结 Auto；显式绑定绝不存活于不再声明它的配置激活。
    configuration.custom_model_protocol_preferences.remove(
        PersistedAiConfiguration::provider_model_key("custom:frozen", "frozen/model").as_str(),
    );
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("overlays should reapply");
    let auto_again = runtime
        .resolve_connection_model_selection(
            &configuration,
            "frozen-account",
            "custom:frozen",
            "frozen/model",
        )
        .expect("frozen model must resolve again")
        .protocol_binding;
    assert_eq!(
        auto_again.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
        "removing the explicit preference must re-freeze the fixed-priority auto binding"
    );
    assert_eq!(
        auto_again.source,
        crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
    );

    // A frozen binding whose protocol leaves the enabled set is never re-selected.
    // 协议离开启用集合的冻结绑定绝不被重新选择。
    let mut provider = configuration
        .custom_providers
        .remove("custom:frozen")
        .expect("frozen provider must exist");
    provider.service_entries[0]
        .protocol_routes
        .retain(|route| route.protocol_profile_id != OPENAI_RESPONSES_PROFILE_ID);
    install_current_custom_provider(&mut configuration, provider);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("overlays should reapply");
    let narrowed = runtime
        .resolve_connection_model_selection(
            &configuration,
            "frozen-account",
            "custom:frozen",
            "frozen/model",
        )
        .expect("frozen model must resolve under the narrowed set")
        .protocol_binding;
    assert_ne!(
        narrowed.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
        "a frozen binding outside the enabled set must be re-frozen away"
    );

    // Reintroducing the higher-priority protocol is another activation-boundary change:
    // Auto must return to Responses even though the lower-priority binding remains enabled.
    // 重新引入更高优先级协议是另一次激活边界变更：即使较低优先级绑定仍然启用，
    // Auto 也必须返回 Responses。
    let widened = multi_protocol_custom_definition(
        "custom:frozen",
        &[
            crate::CustomProviderProtocol::OpenAiChat,
            crate::CustomProviderProtocol::AnthropicMessages,
            crate::CustomProviderProtocol::OpenAiResponses,
        ],
    );
    install_current_custom_provider(&mut configuration, widened);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("overlays should reapply after widening the protocol set");
    let widened_auto = runtime
        .resolve_connection_model_selection(
            &configuration,
            "frozen-account",
            "custom:frozen",
            "frozen/model",
        )
        .expect("frozen model must resolve under the widened set")
        .protocol_binding;
    assert_eq!(
        widened_auto.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
        "Auto must re-freeze to the newly enabled highest-priority protocol"
    );
    assert_eq!(
        widened_auto.source,
        crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
    );
}

/// Verifies an explicit preference is honored, never refreshed away, and requires re-selection
/// when the referenced protocol leaves the enabled set (plan §13.3.4/13.3.6).
/// 验证显式偏好被遵守、不会被刷新覆盖，且引用协议离开启用集合时必须重新选择（方案 §13.3.4/13.3.6）。
#[test]
fn custom_explicit_preference_survives_and_requires_reselection_after_protocol_removal() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = multi_protocol_custom_definition(
        "custom:explicit",
        &[
            crate::CustomProviderProtocol::OpenAiChat,
            crate::CustomProviderProtocol::AnthropicMessages,
            crate::CustomProviderProtocol::OpenAiResponses,
        ],
    );
    install_current_custom_provider(&mut configuration, provider);
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("explicit-account", "custom:explicit"),
        )
        .expect("explicit connection should register");
    let catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "explicit-account",
            1,
            r#"{"data":[{"id":"explicit/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("explicit catalog should parse"),
        &[
            OPENAI_CHAT_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
        ],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("explicit catalog should install");
    configuration.set_model_enabled("custom:explicit", "explicit/model", true);
    // The user explicitly pins anthropic despite the auto default being responses.
    // 尽管 Auto 默认是 responses，用户显式固定 anthropic。
    configuration.custom_model_protocol_preferences.insert(
        PersistedAiConfiguration::provider_model_key("custom:explicit", "explicit/model"),
        crate::CustomModelProtocolPreference::Explicit {
            protocol: crate::CustomProviderProtocol::AnthropicMessages,
        },
    );
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("explicit custom generation should publish");

    let selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "explicit-account",
            "custom:explicit",
            "explicit/model",
        )
        .expect("explicit model must resolve");
    let binding = &selection.protocol_binding;
    assert_eq!(
        binding.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID,
        "the explicit user choice must never be refreshed away"
    );
    assert_eq!(
        binding.source,
        crate::provider_manifest::ModelProtocolBindingSource::CustomUser
    );
    assert_eq!(
        selection.route.protocol_route.protocol_profile_id,
        ANTHROPIC_MESSAGES_PROFILE_ID
    );

    // Removing the anthropic protocol makes the explicit model require re-selection; the
    // resolver must never silently move it to the next protocol.
    // 移除 anthropic 协议后显式模型必须重新选择；解析器绝不能静默移到下一协议。
    let narrowed = multi_protocol_custom_definition(
        "custom:explicit",
        &[crate::CustomProviderProtocol::OpenAiChat],
    );
    install_current_custom_provider(&mut configuration, narrowed);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("the invalid explicit preference must publish a blocked catalog state");
    let blocked_catalogs = runtime
        .catalog_snapshot()
        .expect("blocked custom catalogs should remain readable");
    let blocked_model = blocked_catalogs
        .get("explicit-account")
        .and_then(|catalog| catalog.models.first())
        .expect("the blocked model must remain visible for re-selection");
    assert!(blocked_model.resolved_protocol_binding.is_none());
    assert_eq!(blocked_model.capabilities, ModelCapabilities::default());
    assert_eq!(
        blocked_model.capability_evidence,
        ModelCapabilityEvidence::default()
    );
    let error = runtime
        .resolve_connection_model_selection(
            &configuration,
            "explicit-account",
            "custom:explicit",
            "explicit/model",
        )
        .map(|_| ())
        .expect_err("explicit model referencing a removed protocol must fail");
    assert!(
        error.contains("re-selection")
            || error.contains("not executable")
            || error.contains("no protocol binding")
            || error.contains("not registered"),
        "unexpected re-selection error: {error}"
    );

    // After the user moves the preference to Auto, the same narrowed provider binds chat.
    // 用户将偏好改为 Auto 后，同一收窄供应商绑定 chat。
    configuration.custom_model_protocol_preferences.insert(
        PersistedAiConfiguration::provider_model_key("custom:explicit", "explicit/model"),
        crate::CustomModelProtocolPreference::Auto,
    );
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("auto custom generation should publish");
    let auto = runtime
        .resolve_connection_model_selection(
            &configuration,
            "explicit-account",
            "custom:explicit",
            "explicit/model",
        )
        .expect("auto preference must rebind on the narrowed provider");
    assert_eq!(
        auto.route.protocol_route.protocol_profile_id,
        OPENAI_CHAT_PROFILE_ID
    );
}

/// R5/§13.3: request execution never resolves a missing custom binding from live settings.
/// R5/§13.3：请求执行绝不从实时设置解析缺失的自定义绑定。
///
/// A malformed activated catalog intentionally lacks the activation-time binding. Even though
/// the current provider configuration can produce an Auto choice, selection must fail closed.
/// 一份异常已激活目录刻意缺少激活时绑定。即使当前供应商配置可产生 Auto 选择，模型选择也
/// 必须失败关闭。
#[test]
fn missing_custom_catalog_binding_never_resolves_live_at_request_time() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = multi_protocol_custom_definition(
        "custom:missing-binding",
        &[
            crate::CustomProviderProtocol::OpenAiChat,
            crate::CustomProviderProtocol::OpenAiResponses,
        ],
    );
    install_current_custom_provider(&mut configuration, provider);
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("missing-binding-account", "custom:missing-binding"),
        )
        .expect("custom connection should register");
    let catalog = parse_openai_model_catalog(
        "missing-binding-account",
        1,
        r#"{"data":[{"id":"missing/binding","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
    )
    .expect("raw catalog should parse");
    configuration.set_model_enabled("custom:missing-binding", "missing/binding", true);
    // candidateGeneration injects one malformed activated catalog while preserving the
    // immutable generation boundary used by request selection.
    // candidateGeneration 注入一份异常已激活目录，同时保留请求选择使用的不可变代际边界。
    let mut candidate_generation = runtime
        .selection_catalog_snapshot()
        .expect("published generation should load")
        .as_ref()
        .clone();
    candidate_generation.configuration = Arc::new(configuration.clone());
    candidate_generation
        .activated_effective_catalogs
        .replace(catalog);
    *runtime
        .resolved_provider_generation
        .write()
        .expect("published generation lock should remain healthy") = Arc::new(candidate_generation);
    let error = runtime
        .resolve_connection_model_selection(
            &configuration,
            "missing-binding-account",
            "custom:missing-binding",
            "missing/binding",
        )
        .map(|_| ())
        .expect_err("request-time live binding resolution must remain forbidden");
    assert!(
        error.contains("no protocol binding frozen"),
        "unexpected missing-binding error: {error}"
    );
}
