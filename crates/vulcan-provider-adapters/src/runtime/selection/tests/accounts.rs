//! Exact account ownership, access admission, and shared model settings.
//! 精确账号所有权、访问准入与共用模型设置。

use crate::runtime::PublicAiRuntime;
use crate::runtime::test_support::{
    catalog, connection, custom_connection, install_current_custom_provider,
    install_source_catalog, multi_protocol_custom_definition, two_account_runtime,
    with_protocol_variants,
};
use crate::{
    ActiveAiModelSelection, ConnectionModel, ConnectionModelCatalog, ModelExecutionOverrides,
    PersistedAiConfiguration, parse_openai_model_catalog,
};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{AiOperation, CapabilitySupport, ModelCapabilities, ReasoningControlMode};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Verifies execution ignores persisted system-provider reasoning contract overrides.
/// 验证执行忽略持久化的系统供应商思考契约覆盖。
#[test]
fn selection_ignores_persisted_system_reasoning_contract_overrides() {
    let (runtime, mut configuration) = two_account_runtime();
    configuration.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("openrouter", "openai/gpt-5.6-sol"),
        crate::ModelCapabilityOverrides {
            reasoning: Some(CapabilitySupport::Native),
            reasoning_control: Some(ReasoningControlMode::EffortSelection),
            reasoning_efforts: Some(vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Medium,
            ]),
            request_projection: Some(crate::CustomModelRequestProjection {
                reasoning: BTreeMap::from([(
                    vulcan_ai_contract::ReasoningEffort::Medium,
                    crate::CustomRequestMutation {
                        set: BTreeMap::from([(
                            String::from("thinking.type"),
                            serde_json::Value::String(String::from("enabled")),
                        )]),
                        delete: vec![String::from("reasoning_effort")],
                    },
                )]),
                ..crate::CustomModelRequestProjection::default()
            }),
            ..crate::ModelCapabilityOverrides::default()
        },
    );

    let mut stale_catalog = catalog("one");
    let stale_model = stale_catalog
        .models
        .first_mut()
        .expect("model fixture should exist");
    stale_model.capabilities.reasoning = CapabilitySupport::Unsupported;
    stale_model.capabilities.reasoning_control = ReasoningControlMode::FixedOff;
    stale_model.capabilities.reasoning_efforts = vec![vulcan_ai_contract::ReasoningEffort::None];
    runtime
        .replace_catalog(stale_catalog)
        .expect("stale catalog should install");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("system capability generation should publish");

    let selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("selection should use the system-provider catalog contract");
    assert_eq!(
        selection.model.capabilities.reasoning,
        CapabilitySupport::Native
    );
    assert_eq!(
        selection.model.capabilities.reasoning_control,
        ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        selection.model.capabilities.reasoning_efforts,
        crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS
    );
    assert!(
        selection.route.custom_model_projection.is_none(),
        "system-provider request projection overrides must not reach execution"
    );
}

/// Verifies provider default selection is used until a model-specific account is remembered.
/// 验证在记住模型专属账号前使用供应商默认账号。
#[test]
fn model_selection_prefers_remembered_account_over_provider_default() {
    let (runtime, mut configuration) = two_account_runtime();
    let initial = runtime
        .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
        .expect("provider default should resolve");
    assert_eq!(initial.route.connection.id, "one");

    configuration
        .connection_graph
        .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "two")
        .expect("model account should be remembered");
    runtime
        .replace_configuration(configuration.clone())
        .expect("remembered account should cross the activation boundary");
    let remembered = runtime
        .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
        .expect("remembered account should resolve");
    assert_eq!(remembered.route.connection.id, "two");
}

/// Verifies missing preferences fall back to the first stable visible account.
/// 验证偏好缺失时回退到首个稳定可见账号。
#[test]
fn model_selection_uses_first_stable_account_without_preference() {
    let (runtime, mut configuration) = two_account_runtime();
    configuration
        .connection_graph
        .account_preferences
        .provider_defaults
        .clear();
    runtime
        .replace_configuration(configuration.clone())
        .expect("cleared preference should cross the activation boundary");
    let selection = runtime
        .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
        .expect("stable first account should resolve");
    assert_eq!(selection.route.connection.id, "one");
}

/// Verifies pending credentials are removed from preferences and all normal resolution paths.
/// 验证待验证凭据会从偏好中移除，并被全部常规解析路径隔离。
#[test]
fn pending_validation_connection_is_isolated_from_normal_routing() {
    let (runtime, mut configuration) = two_account_runtime();
    configuration
        .connection_graph
        .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "one")
        .expect("model preference should be recorded");
    configuration.active = Some(ActiveAiModelSelection {
        provider_id: String::from("openrouter"),
        model_id: String::from("openai/gpt-5.6-sol"),
        connection_id: String::from("one"),
    });

    configuration
        .set_connection_pending_validation("one", true)
        .expect("configured connection should enter pending state");
    runtime
        .replace_configuration(configuration.clone())
        .expect("pending validation should cross the activation boundary");

    assert!(configuration.active.is_none());
    assert!(
        !configuration
            .connection_graph
            .account_preferences
            .provider_defaults
            .contains_key("openrouter")
    );
    assert!(
        configuration
            .connection_graph
            .account_preferences
            .model_last_connections
            .is_empty()
    );
    let fallback = runtime
        .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
        .expect("eligible account should replace the pending preference");
    assert_eq!(fallback.route.connection.id, "two");
    let Err(exact_error) = runtime.resolve_connection_model_selection(
        &configuration,
        "one",
        "openrouter",
        "openai/gpt-5.6-sol",
    ) else {
        panic!("pending exact account must not execute");
    };
    assert!(exact_error.contains("pending explicit model validation"));
}

/// Verifies a disabled credential behaves like an absent account until explicitly enabled again.
/// 验证已停用凭据在再次显式启用前与不存在的账号行为一致。
#[test]
fn disabled_connection_is_excluded_from_fallback_and_exact_routing() {
    // runtime and configuration contain two credentials exposing the same model.
    // runtime 与 configuration 包含两个公开同一模型的凭据。
    let (runtime, mut configuration) = two_account_runtime();
    configuration
        .connection_graph
        .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "one")
        .expect("model preference should be recorded");
    configuration.active = Some(ActiveAiModelSelection {
        provider_id: String::from("openrouter"),
        model_id: String::from("openai/gpt-5.6-sol"),
        connection_id: String::from("one"),
    });

    configuration
        .set_connection_enabled("one", false)
        .expect("configured connection should be disabled");
    runtime
        .replace_configuration(configuration.clone())
        .expect("disabled state should cross the activation boundary");

    assert!(configuration.active.is_none());
    let fallback = runtime
        .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
        .expect("enabled account should replace the disabled preference");
    assert_eq!(fallback.route.connection.id, "two");
    let Err(exact_error) = runtime.resolve_connection_model_selection(
        &configuration,
        "one",
        "openrouter",
        "openai/gpt-5.6-sol",
    ) else {
        panic!("disabled exact account must not execute");
    };
    assert!(exact_error.contains("disabled"));
}

/// Verifies workbench selection rejects a discovered operation-only model.
/// 验证工作台选择会拒绝发现结果中仅支持内部操作的模型。
#[test]
fn workbench_models_require_bidirectional_text_conversation() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            connection("one"),
        )
        .expect("connection should register");
    configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
    configuration.set_model_enabled("openrouter", "openai/gpt-5.6-terra", true);
    install_source_catalog(
        &runtime,
        with_protocol_variants(
            ConnectionModelCatalog {
                connection_id: String::from("one"),
                models: vec![
                    ConnectionModel {
                        id: String::from("openai/gpt-5.6-sol"),
                        display_name: String::from("Chat"),
                        capabilities: ModelCapabilities {
                            operations: vec![AiOperation::ConversationRespond],
                            ..ModelCapabilities::default()
                        },
                        capability_evidence: crate::ModelCapabilityEvidence::default(),
                        protocol_variants: BTreeMap::new(),
                        resolved_protocol_binding: None,
                    },
                    ConnectionModel {
                        id: String::from("openai/gpt-5.6-terra"),
                        display_name: String::from("Embedding"),
                        capabilities: ModelCapabilities {
                            operations: vec![AiOperation::EmbeddingCreate],
                            ..ModelCapabilities::default()
                        },
                        capability_evidence: crate::ModelCapabilityEvidence::default(),
                        protocol_variants: BTreeMap::new(),
                        resolved_protocol_binding: None,
                    },
                ],
                generation: 1,
                page_count: 1,
                discovery_schema_revision: String::from("test.mixed.models.v1"),
            },
            &[
                OPENAI_CHAT_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
                ANTHROPIC_MESSAGES_PROFILE_ID,
            ],
        ),
    );
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("mixed-model generation should publish");

    let selectors = runtime
        .available_model_selectors(&configuration)
        .expect("selectors should resolve");
    assert!(selectors.contains("openrouter/openai/gpt-5.6-sol"));
    assert!(!selectors.contains("openrouter/openai/gpt-5.6-terra"));
    let Err(error) = runtime.resolve_connection_model_selection(
        &configuration,
        "one",
        "openrouter",
        "openai/gpt-5.6-terra",
    ) else {
        panic!("embedding-only discovery must not resolve as a workbench model");
    };
    assert!(error.contains("does not expose bidirectional text conversation"));
}

/// Verifies a removed remembered account degrades to another account exposing the same model.
/// 验证已记住账号被移除后会降级到另一个暴露相同模型的账号。
#[test]
fn model_selection_degrades_after_remembered_account_is_removed() {
    let (runtime, mut configuration) = two_account_runtime();
    configuration
        .connection_graph
        .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "two")
        .expect("model account should be remembered");
    configuration.connection_graph.connections.remove("two");
    runtime
        .replace_configuration(configuration.clone())
        .expect("connection removal should cross the activation boundary");

    let selection = runtime
        .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
        .expect("remaining account should resolve");
    assert_eq!(selection.route.connection.id, "one");
}

/// Verifies a session-selected connection overrides the model-wide remembered account.
/// 验证会话选中的连接会覆盖模型级记忆账号。
#[test]
fn connection_model_selection_uses_exact_session_account() {
    let (runtime, mut configuration) = two_account_runtime();
    configuration
        .connection_graph
        .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "one")
        .expect("model account should be remembered");

    let selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "two",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("exact session account should resolve");
    assert_eq!(selection.route.connection.id, "two");
    assert!(selection.route_revision.starts_with("sha256:"));

    let other_selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("other exact account should resolve");
    assert_ne!(selection.route_revision, other_selection.route_revision);
}

/// Verifies settings publication affects new selections without changing an already frozen policy.
/// 验证设置发布只影响新选择，不改变已经冻结的策略。
#[test]
fn media_delivery_policy_is_frozen_separately_from_route_revision() {
    // The fixture owns an offline exact account and catalog.
    // 夹具持有离线精确账号及目录。
    let (runtime, mut configuration) = two_account_runtime();
    // The original selection keeps the default media policy snapshot.
    // 原始选择保持默认媒体策略快照。
    let original = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("media policy fixture or operation must be valid");
    // Change only an extension threshold, without changing model, route or manifests.
    // 仅修改扩展阈值，不修改模型、路由或清单。
    let mut policy = configuration.effective_media_delivery_policy();
    policy.independent_extension.video.without_upload_max_bytes = 1234;
    configuration
        .set_media_delivery_policy(policy)
        .expect("media policy fixture or operation must be valid");
    runtime
        .replace_configuration(configuration.clone())
        .expect("media policy fixture or operation must be valid");
    // A new selection captures the new settings while retaining the same route identity.
    // 新选择捕获新设置，同时保留相同路由身份。
    let updated = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("media policy fixture or operation must be valid");
    assert_eq!(
        original.media_delivery_policy,
        crate::MediaDeliveryPolicy::default()
    );
    assert_eq!(updated.media_delivery_policy, policy);
    assert_eq!(original.route_revision, updated.route_revision);
    assert_ne!(
        original
            .media_delivery_policy
            .fingerprint()
            .expect("media policy fixture or operation must be valid"),
        updated
            .media_delivery_policy
            .fingerprint()
            .expect("media policy fixture or operation must be valid")
    );
    // A direct runtime caller cannot bypass validation by assigning the public persisted field.
    // 直接运行时调用者不能通过赋值公开持久化字段绕过校验。
    let previous = runtime
        .configuration_snapshot()
        .expect("media policy fixture or operation must be valid");
    policy.version += 1;
    configuration.media_delivery_policy = Some(policy);
    assert!(runtime.replace_configuration(configuration).is_err());
    assert_eq!(
        runtime
            .configuration_snapshot()
            .expect("media policy fixture or operation must be valid"),
        previous
    );
}

/// Verifies one active selection remains bound to its exact account snapshot.
/// 验证活动选择始终绑定其精确账号快照。
#[test]
fn active_selection_uses_exact_configured_connection() {
    let (runtime, mut configuration) = two_account_runtime();
    configuration.active = Some(ActiveAiModelSelection {
        provider_id: String::from("openrouter"),
        model_id: String::from("openai/gpt-5.6-sol"),
        connection_id: String::from("two"),
    });
    runtime
        .replace_configuration(configuration.clone())
        .expect("active selection should cross the activation boundary");
    let selection = runtime
        .resolve_active_selection(&configuration)
        .expect("active account should resolve");
    assert_eq!(selection.route.connection.id, "two");
}

/// Verifies accounts sharing a provider, model, and protocol receive the same override.
/// 验证共享供应商、模型与协议的账号取得同一份覆盖。
#[test]
fn model_overrides_are_shared_across_matching_accounts() {
    let (runtime, mut configuration) = two_account_runtime();
    let key = PersistedAiConfiguration::provider_model_protocol_key(
        "openrouter",
        "openai/gpt-5.6-sol",
        OPENAI_RESPONSES_PROFILE_ID,
    );
    configuration.model_overrides.insert(
        key,
        ModelExecutionOverrides {
            context_limit: Some(4096),
            ..ModelExecutionOverrides::default()
        },
    );

    for connection_id in ["one", "two"] {
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: connection_id.to_string(),
        });
        runtime
            .replace_configuration(configuration.clone())
            .expect("active override selection should cross the activation boundary");
        let selection = runtime
            .resolve_active_selection(&configuration)
            .expect("shared model override should resolve");
        assert_eq!(selection.overrides.context_limit, Some(4096));
    }
}

/// Verifies three custom providers stay fully isolated across models and credentials.
/// 验证三个自定义供应商在模型与凭据上完全隔离。
#[test]
fn three_custom_providers_stay_isolated_from_each_other() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    // provider-a owns only the chat protocol; provider-b only anthropic; provider-c only gemini.
    // provider-a 仅拥有聊天协议；provider-b 仅 anthropic；provider-c 仅 gemini。
    let provider_a = multi_protocol_custom_definition(
        "custom:alpha",
        &[crate::CustomProviderProtocol::OpenAiChat],
    );
    let provider_b = multi_protocol_custom_definition(
        "custom:beta",
        &[crate::CustomProviderProtocol::AnthropicMessages],
    );
    let provider_c = multi_protocol_custom_definition(
        "custom:gamma",
        &[crate::CustomProviderProtocol::GeminiGenerateContent],
    );
    install_current_custom_provider(&mut configuration, provider_a);
    install_current_custom_provider(&mut configuration, provider_b);
    install_current_custom_provider(&mut configuration, provider_c);
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("alpha-account", "custom:alpha"),
        )
        .expect("alpha connection should register");
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("beta-account", "custom:beta"),
        )
        .expect("beta connection should register");
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("gamma-account", "custom:gamma"),
        )
        .expect("gamma connection should register");
    // Every provider owns the same model identifier; the identities must never leak.
    // 每个供应商拥有相同模型标识；身份绝不能相互泄漏。
    let catalog = parse_openai_model_catalog(
        "alpha-account",
        1,
        r#"{"data":[{"id":"shared/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
    )
    .expect("alpha catalog should parse");
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("alpha catalog should install");
    configuration.set_model_enabled("custom:alpha", "shared/model", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("custom-provider isolation generation should publish");

    let alpha = runtime
        .resolve_connection_model_selection(
            &configuration,
            "alpha-account",
            "custom:alpha",
            "shared/model",
        )
        .expect("alpha model must resolve on its own provider");
    assert_eq!(
        alpha.route.protocol_route.protocol_profile_id,
        OPENAI_CHAT_PROFILE_ID
    );
    let binding = &alpha.protocol_binding;
    assert_eq!(binding.protocol_profile_id, OPENAI_CHAT_PROFILE_ID);

    // beta and gamma are absent from the alpha catalog and must never resolve there.
    // beta 与 gamma 不在 alpha 目录中，绝不能在此解析。
    let beta_error = runtime
        .resolve_connection_model_selection(
            &configuration,
            "alpha-account",
            "custom:beta",
            "shared/model",
        )
        .map(|_| ())
        .expect_err("beta provider must not resolve on the alpha account");
    assert!(
        beta_error.contains("does not belong to provider")
            || beta_error.contains("not configured")
            || beta_error.contains("not registered"),
        "unexpected isolation error: {beta_error}"
    );
}
