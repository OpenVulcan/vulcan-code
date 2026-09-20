//! Tests for catalog projection, access-independent support, and candidate validation.
//! 目录投影、独立于访问开关的支持目录与候选校验测试。

use super::super::test_support::{
    catalog, connection, custom_connection, custom_definition_with_discovery_mode,
    install_source_catalog, multi_protocol_custom_definition, two_account_runtime,
    with_protocol_variants,
};
use super::*;
use crate::{
    ProviderToolCapabilityKind, ProviderToolTargetSelection, parse_openrouter_model_catalog,
};
use vulcan_ai_contract::{AiOperation, ModelCapabilities};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Verifies rebuilding user overlays never mutates the immutable synchronized catalog snapshot.
/// 验证重建用户覆盖绝不会修改不可变的已同步目录快照。
#[test]
fn system_model_overlay_rebuild_preserves_immutable_source_capabilities() {
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
    let source = catalog("one");
    runtime
        .source_catalogs
        .write()
        .expect("source catalog lock should remain healthy")
        .replace(source.clone());
    runtime
        .replace_catalog(source)
        .expect("effective source catalog should install");
    configuration.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("openrouter", "openai/gpt-5.6-sol"),
        crate::ModelCapabilityOverrides {
            context_limit: Some(4_096),
            output_limit: None,
            input_resources: Some(vec![vulcan_ai_contract::ResourceKind::Image]),
            output_resources: None,
            ..crate::ModelCapabilityOverrides::default()
        },
    );

    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("overlay rebuild should succeed");

    let immutable_context = runtime
        .source_catalogs
        .read()
        .expect("source catalog lock should remain healthy")
        .get("one")
        .and_then(|catalog| catalog.models.first())
        .and_then(|model| model.capabilities.context_limit);
    let effective = runtime
        .catalog_snapshot()
        .expect("effective catalog should remain readable");
    let effective_model = effective
        .get("one")
        .and_then(|catalog| catalog.models.first())
        .expect("effective model should exist");
    assert_eq!(immutable_context, Some(8_192));
    assert_eq!(effective_model.capabilities.context_limit, Some(4_096));
    assert_eq!(
        effective_model.capabilities.input_resources,
        Vec::<vulcan_ai_contract::ResourceKind>::new()
    );
}

/// Verifies disabling one account-model pair leaves a sibling account and source evidence unchanged.
/// 验证禁用一个账号模型组合不会影响同供应商其他账号或原始证据。
#[test]
fn connection_model_access_isolated_between_accounts() {
    let (runtime, mut configuration) = two_account_runtime();
    configuration.set_connection_model_enabled("one", "openai/gpt-5.6-sol", false);

    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("account access overlay should resolve");

    let effective = runtime
        .catalog_snapshot()
        .expect("effective catalogs should remain readable");
    assert!(
        effective
            .get("one")
            .expect("first account catalog should exist")
            .models
            .iter()
            .all(|model| model.id != "openai/gpt-5.6-sol")
    );
    assert!(
        effective
            .get("two")
            .expect("second account catalog should exist")
            .models
            .iter()
            .any(|model| model.id == "openai/gpt-5.6-sol")
    );

    let source = runtime
        .source_catalog_snapshot()
        .expect("source catalogs should remain readable");
    assert_eq!(
        source
            .get("one")
            .expect("first source catalog should exist")
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["openai/gpt-5.6-sol"]
    );
}

/// Verifies a disabled account can opt into a newly discovered model without changing its peer.
/// 验证已关闭账号可选择新发现模型，且不会改变同供应商的其他账号。
#[test]
fn disabled_connection_default_can_enable_future_discovery() {
    let (runtime, mut configuration) = two_account_runtime();
    configuration.set_connection_models_enabled("one", false);
    let future_catalog = parse_openrouter_model_catalog(
        "one",
        2,
        r#"{"data":[{"id":"vendor/future-model","name":"Future Model","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
    )
    .expect("future model fixture should parse");
    install_source_catalog(&runtime, future_catalog.clone());
    runtime
        .replace_catalog(future_catalog)
        .expect("future source catalog should install");

    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("future model overlay should resolve");
    assert!(
        runtime
            .catalog_snapshot()
            .expect("effective catalogs should remain readable")
            .get("one")
            .expect("first account catalog should exist")
            .models
            .is_empty()
    );

    configuration.set_connection_model_enabled("one", "vendor/future-model", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("explicit future model enable should resolve");
    assert_eq!(
        runtime
            .catalog_snapshot()
            .expect("effective catalogs should remain readable")
            .get("one")
            .expect("first account catalog should exist")
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["vendor/future-model"]
    );
    // The support catalog retains discovered rows independently from account access.
    // 支持目录独立于账号访问状态保留发现模型行。
    let settings = runtime
        .settings_catalog_snapshot(&configuration)
        .expect("settings catalog should resolve");
    let support = settings
        .model_support_catalogs
        .get("one")
        .expect("first account support catalog should exist");
    assert_eq!(support.models.len(), 1);
    assert!(
        support
            .models
            .iter()
            .any(|model| model.id == "vendor/future-model")
    );
}

/// Verifies OpenRouter's 17 signed defaults initialize each account without hiding new models.
/// 验证 OpenRouter 的 17 项签名默认值分别初始化账号，且不隐藏新发现模型。
#[test]
fn openrouter_account_starts_with_signed_models_and_can_select_discovery() {
    let (runtime, configuration) = two_account_runtime();
    let manifest_generation = runtime
        .selection_catalog_snapshot()
        .expect("published manifests should resolve");
    let openrouter = manifest_generation
        .manifests
        .manifest("openrouter")
        .expect("OpenRouter manifest should be present");
    assert_eq!(openrouter.rule_instances.len(), 17);

    let discovered = parse_openrouter_model_catalog(
        "one",
        2,
        r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"GPT-5.6 Sol","context_length":8192,"architecture":{"input_modalities":["text"],"output_modalities":["text"]}},{"id":"vendor/future-model","name":"Future Model","context_length":8192,"architecture":{"input_modalities":["text"],"output_modalities":["text"]}}]}"#,
    )
    .expect("discovered models should parse");
    install_source_catalog(&runtime, discovered.clone());
    runtime
        .replace_catalog(discovered)
        .expect("discovered catalog should publish");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("initial account access should resolve");

    let settings = runtime
        .settings_catalog_snapshot(&configuration)
        .expect("settings should include account defaults");
    let first_support = settings
        .model_support_catalogs
        .get("one")
        .expect("first support catalog should exist");
    assert_eq!(first_support.models.len(), 2);
    let initial = settings
        .connection_model_initial_policies
        .get("one")
        .expect("first account should have an initial policy");
    assert!(!initial.default_enabled);
    assert_eq!(initial.model_enabled_overrides.len(), 17);
    assert!(initial.model_is_enabled("openai/gpt-5.6-sol"));
    assert!(!initial.model_is_enabled("vendor/future-model"));
    assert_eq!(
        settings.model_default_enabled_by_key.get(
            PersistedAiConfiguration::provider_model_key("openrouter", "vendor/future-model")
                .as_str()
        ),
        Some(&true),
        "conversation purpose must not inherit the account's unchecked default"
    );
    let initial_effective = runtime
        .catalog_snapshot()
        .expect("effective catalogs should resolve");
    assert_eq!(
        initial_effective
            .get("one")
            .expect("first account should exist")
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["openai/gpt-5.6-sol"]
    );

    let mut selected = configuration.clone();
    selected
        .connection_model_access_policies
        .insert(String::from("one"), initial.clone());
    selected.set_connection_model_enabled("one", "vendor/future-model", true);
    runtime
        .reapply_model_catalog_overlays(&selected)
        .expect("one account's explicit selection should resolve");
    let enabled = runtime
        .catalog_snapshot()
        .expect("effective catalogs should resolve");
    assert_eq!(
        enabled
            .get("one")
            .expect("first account should exist")
            .models
            .len(),
        2
    );
    assert_eq!(
        enabled
            .get("two")
            .expect("second account should exist")
            .models
            .len(),
        1
    );
}

/// Verifies dedicated media discovery survives catalog publication without chat-limit diagnostics.
/// 验证专用媒体发现结果经过目录发布后不产生会话限制诊断。
#[test]
fn dedicated_media_models_skip_conversation_limits_through_catalog_projection() {
    // These exact models reproduce the three dedicated media kinds shown by settings.
    // 这些精确模型覆盖设置页中出现的三种专用媒体类型。
    let (runtime, mut configuration) = two_account_runtime();
    // The fixture uses declared modalities rather than inferring capabilities from model names.
    // 夹具使用明确模态，不根据模型名称推断能力。
    let cases = [
        ("google/veo-3.1", vec!["text", "image"], vec!["video"]),
        ("minimax/speech-2.8-turbo", vec!["text"], vec!["audio"]),
        ("openai/whisper-large-v3-turbo", vec!["audio"], vec!["text"]),
    ];
    // records preserve zero context and a positive completion fact to detect unwanted clamping.
    // records 保留零上下文与正数输出事实，以检测不应发生的收敛。
    let records = cases
        .iter()
        .map(|(id, input, output)| {
            configuration.set_connection_model_enabled("one", id, true);
            serde_json::json!({
                "id": id, "context_length": 0,
                "architecture": {"input_modalities": input, "output_modalities": output},
                "top_provider": {"context_length": 0, "max_completion_tokens": 4096}
            })
        })
        .collect::<Vec<_>>();
    // discovered follows the production parser and exact per-protocol evidence projection.
    // discovered 使用生产解析器及精确的逐协议证据投影。
    let mut discovered = with_protocol_variants(
        parse_openrouter_model_catalog("one", 2, &serde_json::json!({"data": records}).to_string())
            .expect("dedicated media records should parse"),
        &[OPENAI_CHAT_PROFILE_ID],
    );
    for model in &mut discovered.models {
        assert_eq!(model.capabilities.output_limit, Some(4096));
        for variant in model.protocol_variants.values_mut() {
            variant.capability_evidence = model.capability_evidence.clone();
        }
    }
    install_source_catalog(&runtime, discovered);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("media catalog should publish");
    // settings exercises the same management snapshot consumed by the model-list UI.
    // settings 覆盖模型列表界面实际使用的管理快照。
    let settings = runtime
        .settings_catalog_snapshot(&configuration)
        .expect("settings should resolve");
    for (id, _, _) in cases {
        // model must remain enabled with original limits and without conversation eligibility.
        // model 必须保持启用及原始限制，且不获得会话资格。
        let model = settings.catalogs["one"]
            .models
            .iter()
            .find(|model| model.id == id)
            .expect("enabled dedicated media model must remain available");
        assert_eq!(model.capabilities.context_limit, Some(0));
        assert_eq!(model.capabilities.output_limit, Some(4096));
        assert!(!model_supports_text_conversation(&model.capabilities));
        assert!(
            crate::model_capability_limit_issue(
                "one",
                id,
                &model.capabilities,
                &model.capability_evidence
            )
            .is_none()
        );
        assert_eq!(
            settings.source_catalogs["one"]
                .models
                .iter()
                .find(|model| model.id == id)
                .unwrap()
                .capabilities
                .output_limit,
            Some(4096)
        );
    }
}

/// Verifies zero token metadata does not quarantine a media-only model or alter source evidence.
/// 验证零令牌元数据不会隔离纯媒体模型，也不会改写来源证据。
#[test]
fn zero_limit_openrouter_media_model_remains_available() {
    let (runtime, mut configuration) = two_account_runtime();
    // OpenRouter's closed account default requires one explicit account enablement.
    // OpenRouter 的账号默认关闭策略要求明确启用一个账号的该模型。
    configuration.set_connection_model_enabled("one", "google/veo-3.1", true);
    let mut discovered = with_protocol_variants(
        parse_openrouter_model_catalog(
            "one",
            2,
            r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"GPT-5.6 Sol","context_length":8192,"architecture":{"input_modalities":["text"],"output_modalities":["text"]}},{"id":"google/veo-3.1","name":"Google: Veo 3.1","context_length":0,"architecture":{"input_modalities":["text","image"],"output_modalities":["video"]},"top_provider":{"context_length":0,"max_completion_tokens":0}}]}"#,
        )
        .expect("OpenRouter discovery facts should parse"),
        &[
            ANTHROPIC_MESSAGES_PROFILE_ID,
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
        ],
    );
    // Production route discovery copies each model's observed output evidence into its variant.
    // 生产路由发现会把模型观测到的输出证据复制到各协议变体。
    for model in &mut discovered.models {
        for variant in model.protocol_variants.values_mut() {
            variant.capability_evidence = model.capability_evidence.clone();
        }
    }
    install_source_catalog(&runtime, discovered);

    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("media token metadata must not abort the connection projection");
    let settings = runtime
        .settings_catalog_snapshot(&configuration)
        .expect("management projection should retain the media model");
    let effective = settings
        .catalogs
        .get("one")
        .expect("account catalog should exist");
    assert_eq!(
        effective
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["google/veo-3.1", "openai/gpt-5.6-sol"],
    );
    let support = settings
        .model_support_catalogs
        .get("one")
        .expect("account management catalog should exist");
    let media = support
        .models
        .iter()
        .find(|model| model.id == "google/veo-3.1")
        .expect("video model should remain visible");
    assert_eq!(media.capabilities.context_limit, Some(0));
    assert!(
        crate::model_capability_limit_issue(
            "one",
            media.id.as_str(),
            &media.capabilities,
            &media.capability_evidence,
        )
        .is_none()
    );
    assert!(
        settings
            .source_catalogs
            .get("one")
            .expect("source evidence should remain visible")
            .models
            .iter()
            .any(|model| model.id == "google/veo-3.1")
    );

    // An explicit account denial removes only that account's effective media access.
    // 显式账号拒绝仅移除该账号的媒体模型有效访问。
    let mut disabled = configuration.clone();
    disabled.set_connection_model_enabled("one", "google/veo-3.1", false);
    runtime
        .reapply_model_catalog_overlays(&disabled)
        .expect("a disabled media model must not block publication");
    let disabled_settings = runtime
        .settings_catalog_snapshot(&disabled)
        .expect("management snapshot should still resolve");
    assert!(
        disabled_settings
            .model_support_catalogs
            .get("one")
            .expect("management catalog should remain present")
            .models
            .iter()
            .any(|model| model.id == "google/veo-3.1")
    );
    assert!(
        !disabled_settings
            .catalogs
            .get("one")
            .expect("effective catalog should remain present")
            .models
            .iter()
            .any(|model| model.id == "google/veo-3.1")
    );

    // A deliberate user override still changes only the effective capability projection.
    // 用户显式覆盖仍然只改变有效能力投影。
    let mut repaired = configuration.clone();
    repaired.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("openrouter", "google/veo-3.1"),
        crate::ModelCapabilityOverrides {
            context_limit: Some(8_192),
            output_limit: Some(1_024),
            ..crate::ModelCapabilityOverrides::default()
        },
    );
    runtime
        .reapply_model_catalog_overlays(&repaired)
        .expect("a deliberate positive override should allow publication");
    let repaired_settings = runtime
        .settings_catalog_snapshot(&repaired)
        .expect("repaired settings should resolve");
    assert!(
        repaired_settings
            .catalogs
            .get("one")
            .expect("repaired account catalog should exist")
            .models
            .iter()
            .any(|model| model.id == "google/veo-3.1")
    );
    assert!(
        repaired_settings
            .source_catalogs
            .get("one")
            .expect("source evidence should remain")
            .models
            .iter()
            .any(
                |model| model.id == "google/veo-3.1" && model.capabilities.context_limit == Some(0)
            )
    );
}

/// Verifies candidate capability differences are checked after inherited limits resolve.
/// 验证候选能力差异会在继承限制解析完成后接受校验。
#[test]
fn capability_override_candidate_validates_final_resolved_limits() {
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
    install_source_catalog(&runtime, catalog("one"));

    for (overrides, expected_error) in [(
        crate::ModelCapabilityOverrides {
            output_limit: Some(0),
            ..crate::ModelCapabilityOverrides::default()
        },
        "output limit must be greater than zero",
    )] {
        let error = runtime
            .validate_model_capability_override_candidate(
                &configuration,
                "openrouter",
                "openai/gpt-5.6-sol",
                Some(&overrides),
            )
            .expect_err("invalid final limits must be rejected");
        assert!(
            error.contains(expected_error),
            "unexpected validation error: {error}"
        );
    }

    runtime
        .validate_model_capability_override_candidate(
            &configuration,
            "openrouter",
            "openai/gpt-5.6-sol",
            Some(&crate::ModelCapabilityOverrides {
                context_limit: Some(8_192),
                output_limit: Some(4_096),
                ..crate::ModelCapabilityOverrides::default()
            }),
        )
        .expect("valid final limits should be accepted");
    runtime
        .validate_model_capability_override_candidate(
            &configuration,
            "openrouter",
            "openai/gpt-5.6-sol",
            Some(&crate::ModelCapabilityOverrides {
                context_limit: Some(2_048),
                ..crate::ModelCapabilityOverrides::default()
            }),
        )
        .expect("an inherited output limit above the narrowed context should be clamped");
    runtime
        .validate_model_capability_override_candidate(
            &configuration,
            "openrouter",
            "openai/gpt-5.6-sol",
            Some(&crate::ModelCapabilityOverrides {
                output_limit: Some(9_000),
                ..crate::ModelCapabilityOverrides::default()
            }),
        )
        .expect("an output limit above the context should be clamped");
}

/// Verifies provider-level model management ignores account visibility while preserving route validation.
/// 验证供应商级模型管理忽略账号可见性，同时保留路由校验。
#[test]
fn provider_model_management_uses_support_catalog_when_account_is_disabled() {
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
    install_source_catalog(&runtime, catalog("one"));
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("support catalog should cross the explicit activation boundary");
    configuration.set_connection_models_enabled("one", false);

    assert!(
        runtime
            .provider_model_is_supported(&configuration, "openrouter", "openai/gpt-5.6-sol")
            .expect("provider support should resolve")
    );
    runtime
        .validate_model_capability_override_candidate(
            &configuration,
            "openrouter",
            "openai/gpt-5.6-sol",
            Some(&crate::ModelCapabilityOverrides {
                context_limit: Some(8_192),
                output_limit: Some(4_096),
                ..crate::ModelCapabilityOverrides::default()
            }),
        )
        .expect("account visibility must not block provider capability settings");
    runtime
        .validate_model_execution_override_candidate(
            &configuration,
            "openrouter",
            "openai/gpt-5.6-sol",
            OPENAI_RESPONSES_PROFILE_ID,
            &crate::ModelExecutionOverrides {
                prompt_cache: Some(vulcan_ai_contract::PromptCachePolicy::ProviderDefault),
                ..crate::ModelExecutionOverrides::default()
            },
        )
        .expect("account visibility must not block the frozen model protocol settings");
}

/// Verifies a credential-free custom provider can retain direct settings for its manual model.
/// 验证无凭据自定义供应商可以保留其手工模型的直接设置。
#[test]
fn credential_free_manual_model_accepts_direct_capability_settings() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = custom_definition_with_discovery_mode(
        "custom:test",
        &[crate::CustomProviderProtocol::OpenAiChat],
        crate::CustomProviderModelDiscoveryMode::Manual,
    );
    configuration
        .custom_providers
        .insert(provider.id.clone(), provider);
    configuration.custom_models.insert(
        String::from("custom:test"),
        BTreeMap::from([(
            String::from("private/chat"),
            crate::CustomModelDefinition {
                id: String::from("private/chat"),
                display_name: String::from("Private Chat"),
                enabled: true,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    ..ModelCapabilities::default()
                },
                request_projection: crate::CustomModelRequestProjection::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
            },
        )]),
    );

    runtime
        .validate_model_capability_override_candidate(
            &configuration,
            "custom:test",
            "private/chat",
            Some(&crate::ModelCapabilityOverrides {
                context_limit: Some(8_192),
                output_limit: Some(1_024),
                ..crate::ModelCapabilityOverrides::default()
            }),
        )
        .expect("manual provider-only validation should not require a credential");
}

/// Verifies a provider-wide difference remains usable when one account has a smaller context window.
/// 验证供应商级差异在账号上下文窗口较小时仍然可用。
#[test]
fn capability_override_candidate_clamps_each_account_independently() {
    let (runtime, configuration) = two_account_runtime();
    install_source_catalog(&runtime, catalog("one"));
    install_source_catalog(
        &runtime,
        with_protocol_variants(
            parse_openrouter_model_catalog(
                "two",
                1,
                r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"Model","context_length":2048,"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":1024}}]}"#,
            )
            .expect("second account source should parse"),
            &[
                OPENAI_CHAT_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
                ANTHROPIC_MESSAGES_PROFILE_ID,
            ],
        ),
    );

    runtime
        .validate_model_capability_override_candidate(
            &configuration,
            "openrouter",
            "openai/gpt-5.6-sol",
            Some(&crate::ModelCapabilityOverrides {
                output_limit: Some(3_000),
                ..crate::ModelCapabilityOverrides::default()
            }),
        )
        .expect("the smaller account context should clamp the effective output ceiling");
}

/// Verifies ordinary catalog rebuilds clamp an already persisted output difference.
/// 验证常规目录重建会收窄已经持久化的输出差异。
#[test]
fn model_overlay_rebuild_clamps_invalid_final_limits() {
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
    install_source_catalog(&runtime, catalog("one"));
    configuration.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("openrouter", "openai/gpt-5.6-sol"),
        crate::ModelCapabilityOverrides {
            output_limit: Some(9_000),
            ..crate::ModelCapabilityOverrides::default()
        },
    );

    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("invalid final limits must be narrowed instead of stopping the rebuild");

    let effective_catalogs = runtime
        .catalog_snapshot()
        .expect("effective catalog should remain readable");
    let model = effective_catalogs
        .get("one")
        .and_then(|catalog| catalog.models.first())
        .expect("effective model should remain visible");
    assert_eq!(model.capabilities.context_limit, Some(8_192));
    assert_eq!(model.capabilities.output_limit, Some(8_192));
}

/// Verifies removing a manual-only definition removes the model from its exact connection.
/// 验证移除仅手工定义后模型会从其精确连接消失。
#[test]
fn connection_catalog_membership_tracks_manual_definition_removal() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = custom_definition_with_discovery_mode(
        "custom:test",
        &[crate::CustomProviderProtocol::OpenAiChat],
        crate::CustomProviderModelDiscoveryMode::Manual,
    );
    configuration
        .custom_providers
        .insert(provider.id.clone(), provider);
    let custom_connection = custom_connection("custom-account", "custom:test");
    configuration
        .connection_graph
        .connections
        .insert(custom_connection.id.clone(), custom_connection);
    configuration
        .custom_models
        .entry(String::from("custom:test"))
        .or_default()
        .insert(
            String::from("manual/chat"),
            crate::CustomModelDefinition {
                id: String::from("manual/chat"),
                display_name: String::from("Manual Chat"),
                enabled: true,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    context_limit: Some(200_000),
                    ..ModelCapabilities::default()
                },
                request_projection: crate::CustomModelRequestProjection::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
            },
        );

    assert!(
        runtime
            .connection_catalog_contains_model(&configuration, "custom-account", "manual/chat",)
            .expect("manual catalog membership should resolve")
    );
    configuration.custom_models.remove("custom:test");
    assert!(
        !runtime
            .connection_catalog_contains_model(&configuration, "custom-account", "manual/chat",)
            .expect("removed manual model should resolve as absent")
    );
}

/// Verifies removing a user overlay does not hide a model retained by immutable discovery.
/// 验证移除用户覆盖不会隐藏不可变发现目录仍保留的模型。
#[test]
fn connection_catalog_membership_retains_discovered_model_after_overlay_removal() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = multi_protocol_custom_definition(
        "custom:test",
        &[crate::CustomProviderProtocol::OpenAiChat],
    );
    configuration
        .custom_providers
        .insert(provider.id.clone(), provider);
    let custom_connection = custom_connection("custom-account", "custom:test");
    configuration
        .connection_graph
        .connections
        .insert(custom_connection.id.clone(), custom_connection);
    let source = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"openai/gpt-5.6-sol"}]}"#,
    )
    .expect("compatible source catalog should parse");
    install_source_catalog(&runtime, source);
    configuration
        .custom_models
        .entry(String::from("custom:test"))
        .or_default()
        .insert(
            String::from("openai/gpt-5.6-sol"),
            crate::CustomModelDefinition {
                id: String::from("openai/gpt-5.6-sol"),
                display_name: String::from("Reviewed Model"),
                enabled: true,
                capabilities: ModelCapabilities::default(),
                request_projection: crate::CustomModelRequestProjection::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
            },
        );

    configuration.custom_models.remove("custom:test");
    assert!(
        runtime
            .connection_catalog_contains_model(
                &configuration,
                "custom-account",
                "openai/gpt-5.6-sol",
            )
            .expect("source-backed membership should resolve")
    );
}

/// Verifies the manifest-first enabled resolution order: user override, manual model, then
/// bundled manifest default; undeclared providers fail closed.
/// 验证 manifest-first 启用解析顺序：用户覆盖、手动模型、内置清单默认；未声明供应商失败关闭。
#[test]
fn runtime_model_is_enabled_follows_manifest_first_precedence() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();

    // OpenRouter's 17 account defaults do not disable other supported conversation models.
    // OpenRouter 的 17 项账号默认值不会关闭其他受支持模型的会话用途。
    assert!(runtime.model_is_enabled(&configuration, "openrouter", "openai/gpt-5.6-sol"));
    assert!(runtime.model_is_enabled(&configuration, "openrouter", "openai/gpt-4o"));

    // An explicit user override wins over the manifest default in both directions.
    // 显式用户覆盖在两个方向都优先于清单默认。
    configuration.set_model_enabled("openrouter", "openai/gpt-4o", true);
    assert!(runtime.model_is_enabled(&configuration, "openrouter", "openai/gpt-4o"));
    configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", false);
    assert!(!runtime.model_is_enabled(&configuration, "openrouter", "openai/gpt-5.6-sol"));

    // Manual models keep their declared enabled state without manifest defaults.
    // 手动模型保留声明的启用状态，不参与清单默认。
    let mut manual = PersistedAiConfiguration::default();
    manual.custom_models.insert(
        String::from("openrouter"),
        BTreeMap::from([(
            String::from("user/private-model"),
            crate::CustomModelDefinition {
                id: String::from("user/private-model"),
                display_name: String::from("Private Model"),
                enabled: true,
                capabilities: ModelCapabilities::default(),
                request_projection: crate::CustomModelRequestProjection::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
            },
        )]),
    );
    assert!(runtime.model_is_enabled(&manual, "openrouter", "user/private-model"));

    // Providers without a bundled manifest or manual declaration fail closed.
    // 无内置 Manifest 且无手工声明的供应商失败关闭。
    assert!(!runtime.model_is_enabled(&configuration, "unknown-provider", "any-model"));
}

/// Verifies one missing bundled system provider is excluded without suppressing healthy peers.
/// 验证一个缺失的内置系统供应商会被排除，且不会压制健康同级供应商。
#[test]
fn runtime_settings_projection_isolates_one_failed_system_provider() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let current = runtime
        .selection_catalog_snapshot()
        .expect("published generation should resolve");
    let mut manifests = current.manifests.manifests.clone();
    manifests.remove("deepseek");
    let mut bundled_failures = current.manifests.bundled_failures.clone();
    bundled_failures.insert(
        String::from("deepseek"),
        crate::provider_manifest::BundledProviderManifestFailure {
            failure_kind: String::from("bundled_baseline_invalid"),
            message: String::from("isolated test baseline"),
        },
    );
    let isolated_catalog = Arc::new(
        crate::provider_manifest::ResolvedProviderCatalog::build_with_bundled_failures(
            manifests,
            bundled_failures,
            current.manifests.generation + 1,
        )
        .expect("provider-local failed catalog should build"),
    );
    let mut isolated_generation = current.as_ref().clone();
    isolated_generation.manifests = isolated_catalog;
    *runtime
        .resolved_provider_generation
        .write()
        .expect("published generation lock should remain healthy") = Arc::new(isolated_generation);

    let mut configuration = PersistedAiConfiguration::default();
    let healthy_connection = connection("openrouter-account");
    let mut isolated_connection = connection("deepseek-account");
    isolated_connection.provider_id = String::from("deepseek");
    isolated_connection.service_entry_id = String::from("official-api");
    configuration.connection_graph.connections.extend([
        (healthy_connection.id.clone(), healthy_connection),
        (isolated_connection.id.clone(), isolated_connection),
    ]);
    // configuredGeneration installs the intentional isolated-provider fixture as one image.
    // configuredGeneration 把刻意构造的隔离供应商夹具作为一份完整镜像安装。
    let mut configured_generation = runtime
        .selection_catalog_snapshot()
        .expect("isolated generation should remain published")
        .as_ref()
        .clone();
    configured_generation.configuration = Arc::new(configuration.clone());
    *runtime
        .resolved_provider_generation
        .write()
        .expect("published generation lock should remain healthy") =
        Arc::new(configured_generation);

    let snapshot = runtime
        .settings_catalog_snapshot(&configuration)
        .expect("healthy provider settings must remain available");

    assert!(
        snapshot
            .runtime_configuration
            .connection_graph
            .connections
            .contains_key("openrouter-account")
    );
    assert!(
        !snapshot
            .runtime_configuration
            .connection_graph
            .connections
            .contains_key("deepseek-account")
    );
    assert!(snapshot.providers.provider("openrouter").is_some());
    assert!(snapshot.providers.provider("deepseek").is_none());
    assert!(
        runtime
            .provider_manifest_runtime_snapshot("openrouter", &configuration)
            .expect("healthy provider diagnostics must ignore the isolated peer")
            .is_some()
    );
}

/// Verifies APIKEY.FUN fails closed when its authoritative bundled Manifest is unavailable.
/// 验证 APIKEY.FUN 的权威内置 Manifest 不可用时会失败封闭。
#[test]
fn runtime_settings_projection_isolates_invalid_apikeyfun_manifest() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let current = runtime
        .selection_catalog_snapshot()
        .expect("published generation should resolve");
    let mut manifests = current.manifests.manifests.clone();
    manifests.remove("apikeyfun");
    let mut bundled_failures = current.manifests.bundled_failures.clone();
    bundled_failures.insert(
        String::from("apikeyfun"),
        crate::provider_manifest::BundledProviderManifestFailure {
            failure_kind: String::from("bundled_baseline_invalid"),
            message: String::from("isolated authoritative baseline"),
        },
    );
    let isolated_catalog = Arc::new(
        crate::provider_manifest::ResolvedProviderCatalog::build_with_bundled_failures(
            manifests,
            bundled_failures,
            current.manifests.generation + 1,
        )
        .expect("provider-local failed catalog should build"),
    );
    let mut isolated_generation = current.as_ref().clone();
    isolated_generation.manifests = isolated_catalog;
    *runtime
        .resolved_provider_generation
        .write()
        .expect("published generation lock should remain healthy") = Arc::new(isolated_generation);

    let mut configuration = PersistedAiConfiguration::default();
    // apikeyfunConnection represents persisted account state that must be removed from the
    // executable projection when its signed provider authority is unavailable.
    // apikeyfunConnection 表示持久化账号状态；其已签名供应商权威不可用时，必须从可执行投影移除。
    let mut apikeyfun_connection = connection("apikeyfun-account");
    apikeyfun_connection.provider_id = String::from("apikeyfun");
    apikeyfun_connection.service_entry_id = String::from("public-api");
    configuration
        .connection_graph
        .connections
        .insert(apikeyfun_connection.id.clone(), apikeyfun_connection);
    // configuredGeneration installs the intentional isolated-provider fixture as one image.
    // configuredGeneration 把刻意构造的隔离供应商夹具作为一份完整镜像安装。
    let mut configured_generation = runtime
        .selection_catalog_snapshot()
        .expect("isolated generation should remain published")
        .as_ref()
        .clone();
    configured_generation.configuration = Arc::new(configuration.clone());
    *runtime
        .resolved_provider_generation
        .write()
        .expect("published generation lock should remain healthy") =
        Arc::new(configured_generation);

    let snapshot = runtime
        .settings_catalog_snapshot(&configuration)
        .expect("healthy provider settings should remain available");

    assert!(
        !snapshot
            .runtime_configuration
            .connection_graph
            .connections
            .contains_key("apikeyfun-account")
    );
    assert!(snapshot.providers.provider("apikeyfun").is_none());
}

/// Verifies template-only media models ignore conversation publication and remain extension targets.
/// 验证仅模板媒体模型忽略会话发布状态并保持为扩展目标。
#[test]
fn provider_tool_template_models_are_independent_from_conversation_access() {
    // runtime owns the bundled provider registry and one immutable publication generation.
    // runtime 持有内置供应商注册表与一个不可变发布代际。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // configuration owns one eligible Alibaba Model Studio credential.
    // configuration 持有一个合格的阿里云百炼凭据。
    let mut configuration = PersistedAiConfiguration::default();
    // alibaba_connection reuses the complete test credential shape with exact Alibaba identities.
    // alibaba_connection 复用完整测试凭据结构并改为精确阿里云身份。
    let mut alibaba_connection = connection("alibaba-media-account");
    alibaba_connection.provider_id = String::from("alibaba");
    alibaba_connection.service_entry_id = String::from("model-studio-cn");
    alibaba_connection.credential_profile_id = String::from("api-key");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            alibaba_connection,
        )
        .expect("Alibaba media connection should register");

    // directory resolves the authoritative bundled manifest fixture.
    // directory 解析权威内置 Manifest 测试夹具。
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    // text contains the reviewed Alibaba manifest bytes.
    // text 保存已审核的阿里云 Manifest 原文。
    let text = std::fs::read_to_string(directory.join("alibaba.json"))
        .expect("bundled Alibaba manifest must exist");
    // document disables inherited models while preserving every reviewed target and binding.
    // document 在保留全部已审核目标与绑定的同时关闭继承模型。
    let mut document: serde_json::Value =
        serde_json::from_str(&text).expect("Alibaba manifest should parse");
    document["model_defaults"]["default_enabled"] = serde_json::json!(false);
    // compiled freezes the disabled-by-default signed media catalog.
    // compiled 冻结默认关闭的签名媒体目录。
    let compiled = crate::provider_manifest::compile_manifest_text(
        &serde_json::to_string(&document).expect("Alibaba manifest should serialize"),
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("disabled-by-default Alibaba manifest should compile");
    // wan_target_id selects the exact generated video target used for direct-resolution rejection.
    // wan_target_id 选择用于直接解析拒绝验证的精确生成视频目标。
    let wan_target_id = compiled
        .tool_target_template_instances
        .values()
        .find(|instance| instance.model_id == "wan2.7-t2v")
        .map(|instance| instance.target_id.clone())
        .expect("Wan template target should compile");
    runtime
        .replace_system_manifest("alibaba", Arc::new(compiled))
        .expect("Alibaba replacement should stage");
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test async runtime should initialize")
        .block_on(runtime.activate_staged_manifest_catalogs(&configuration, &[]))
        .expect("Alibaba template generation should publish");

    // declared proves disabled template identities remain present in the management inventory.
    // declared 证明关闭的模板身份仍存在于管理目录。
    let declared = runtime
        .provider_tool_template_models(&configuration)
        .expect("template model inventory should resolve");
    assert!(declared.iter().any(|model| {
        model.provider_id == "alibaba"
            && model.model_id == "qwen-image-2.0-pro"
            && model
                .capabilities
                .contains(&ProviderToolCapabilityKind::ImageGenerate)
    }));
    assert!(declared.iter().any(|model| {
        model.provider_id == "alibaba"
            && model.model_id == "wan2.7-t2v"
            && model
                .capabilities
                .contains(&ProviderToolCapabilityKind::VideoGenerate)
    }));
    assert!(
        runtime
            .provider_model_is_supported(&configuration, "alibaba", "wan2.7-t2v")
            .expect("template model support should resolve")
    );
    assert!(
        !runtime
            .provider_conversation_model_is_supported(&configuration, "alibaba", "wan2.7-t2v",)
            .expect("conversation support should resolve")
    );

    // extension_targets proves the extension inventory remains selectable even though the
    // same provider/model conversation publication switches are disabled.
    // extension_targets 证明即使同一供应商/模型的会话发布开关已关闭，扩展清单仍可选择。
    let extension_targets = runtime
        .available_provider_tool_targets(&configuration)
        .expect("extension target inventory should resolve");
    assert!(
        extension_targets
            .iter()
            .any(|available| available.target.upstream_target_id == "qwen-image-2.0-pro")
    );
    assert!(
        extension_targets
            .iter()
            .any(|available| available.target.upstream_target_id == "wan2.7-t2v")
    );
    // extension_selection resolves the exact target that a validated extension target set owns.
    // extension_selection 解析已验证扩展目标集合持有的精确目标。
    let extension_selection = ProviderToolTargetSelection {
        connection_id: String::from("alibaba-media-account"),
        target_id: wan_target_id,
    };
    let resolved = runtime
        .resolve_provider_tool_target(
            &configuration,
            ProviderToolCapabilityKind::VideoGenerate,
            &extension_selection,
        )
        .expect("extension target should resolve independently from conversation access");
    assert_eq!(resolved.target.upstream_target_id, "wan2.7-t2v");
}
