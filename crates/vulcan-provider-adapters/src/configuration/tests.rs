use super::*;
use crate::custom_provider::{
    CustomProviderAuthentication, CustomProviderDefinition, CustomProviderModelDiscoveryMode,
    CustomProviderProtocol,
};
use crate::{
    CUSTOM_MODEL_TEMPLATE_REVISION, CustomModelProtocolPreference, CustomModelTemplateSelection,
};

/// Builds one complete custom provider descriptor for configuration-cleanup tests.
/// 为配置清理测试构建一个完整的自定义供应商描述。
///
/// # Parameters
/// 参数
/// `provider_id` is the exact custom provider identity used by the test.
/// `provider_id` 是测试使用的精确自定义供应商标识。
///
/// # Returns
/// 返回值
/// A valid single-protocol custom provider descriptor.
/// 返回有效的单协议自定义供应商描述。
fn custom_provider_descriptor(provider_id: &str) -> ProviderDescriptor {
    CustomProviderDefinition {
        id: provider_id.to_string(),
        display_name: provider_id.to_string(),
        base_url: String::from("https://example.com/v1"),
        protocols: vec![CustomProviderProtocol::OpenAiResponses],
        authentication: CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("custom provider descriptor should be valid")
}

/// Verifies fresh configurations use the thirty-minute usage interval and sparse priorities.
/// 验证新配置使用三十分钟用量刷新间隔与稀疏供应商优先级。
#[test]
fn default_configuration_uses_thirty_minute_usage_refresh_and_no_priorities() {
    // configuration is the canonical first-run configuration under test.
    // configuration 是本测试验证的权威首次运行配置。
    let configuration = PersistedAiConfiguration::default();

    assert_eq!(configuration.usage_refresh_interval_seconds, 1_800);
    assert!(configuration.provider_priority_overrides.is_empty());
}

/// Builds one deterministic credential connection for configuration projection tests.
/// 为配置投影测试构建一个确定性凭据连接。
///
/// # Parameters
/// 参数
/// `connection_id` and `provider_id` identify the exact account and owning provider.
/// `connection_id` 与 `provider_id` 标识精确账号及其所属供应商。
///
/// # Returns
/// 返回值
/// A non-secret connection record using the custom provider's standard service entry.
/// 返回使用自定义供应商标准服务入口的不含秘密连接记录。
fn custom_provider_connection(
    connection_id: &str,
    provider_id: &str,
) -> crate::CredentialConnection {
    crate::CredentialConnection {
        id: connection_id.to_string(),
        provider_id: provider_id.to_string(),
        service_entry_id: String::from("public-api"),
        credential_profile_id: String::from("api-key"),
        credential_mode: crate::CredentialMode::Token,
        credential_id: format!("credential-{connection_id}"),
        display_name: connection_id.to_string(),
        secret_reference: format!("secret-{connection_id}"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    }
}

/// Verifies runtime model catalogs cannot appear in persisted AI JSON.
/// 验证运行时模型目录不会出现在持久化 AI JSON 中。
#[test]
fn persisted_ai_configuration_has_no_runtime_model_catalog() {
    let encoded =
        serde_json::to_string(&PersistedAiConfiguration::default()).expect("must serialize");
    assert!(!encoded.contains("catalog"));
    assert!(!encoded.contains("connection_model_catalog"));
}

/// Verifies an isolated custom provider is removed only from a detached runtime projection.
/// 验证已隔离自定义供应商只会从独立运行时投影中移除。
#[test]
fn isolated_custom_provider_runtime_projection_preserves_repair_authority() {
    let healthy_provider_id = "custom:healthy";
    let isolated_provider_id = "custom:isolated";
    let healthy_connection_id = "healthy-connection";
    let isolated_connection_id = "isolated-connection";
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_provider_file_refs.extend([
        healthy_provider_id.to_string(),
        isolated_provider_id.to_string(),
    ]);
    configuration.custom_providers.insert(
        healthy_provider_id.to_string(),
        custom_provider_descriptor(healthy_provider_id),
    );
    configuration.connection_graph.connections.extend([
        (
            healthy_connection_id.to_string(),
            custom_provider_connection(healthy_connection_id, healthy_provider_id),
        ),
        (
            isolated_connection_id.to_string(),
            custom_provider_connection(isolated_connection_id, isolated_provider_id),
        ),
    ]);
    configuration
        .connection_graph
        .account_preferences
        .provider_defaults
        .extend([
            (
                healthy_provider_id.to_string(),
                healthy_connection_id.to_string(),
            ),
            (
                isolated_provider_id.to_string(),
                isolated_connection_id.to_string(),
            ),
        ]);
    configuration.active = Some(ActiveAiModelSelection {
        provider_id: isolated_provider_id.to_string(),
        model_id: String::from("isolated-model"),
        connection_id: isolated_connection_id.to_string(),
    });
    configuration.model_enabled_overrides.extend([
        (
            PersistedAiConfiguration::provider_model_key(healthy_provider_id, "healthy-model"),
            true,
        ),
        (
            PersistedAiConfiguration::provider_model_key(isolated_provider_id, "isolated-model"),
            true,
        ),
    ]);

    let projection = configuration.without_isolated_custom_provider_runtime_state();

    assert!(
        configuration
            .custom_provider_file_refs
            .contains(isolated_provider_id)
    );
    assert!(
        configuration
            .connection_graph
            .connections
            .contains_key(isolated_connection_id)
    );
    assert!(configuration.active.is_some());
    assert_eq!(
        projection
            .connection_graph
            .connections
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![healthy_connection_id.to_string()]
    );
    assert_eq!(
        projection
            .connection_graph
            .account_preferences
            .provider_defaults,
        BTreeMap::from([(
            healthy_provider_id.to_string(),
            healthy_connection_id.to_string(),
        )])
    );
    assert!(projection.active.is_none());
    assert!(
        projection
            .custom_provider_file_refs
            .contains(isolated_provider_id)
    );
    assert_eq!(projection.model_enabled_overrides.len(), 1);
    assert!(projection.model_enabled_overrides.contains_key(
        PersistedAiConfiguration::provider_model_key(healthy_provider_id, "healthy-model").as_str()
    ));
}

/// Verifies explicit system-provider isolation removes only runtime-owned references.
/// 验证显式系统供应商隔离只移除运行时自有引用。
#[test]
fn isolated_system_provider_runtime_projection_preserves_persisted_repair_state() {
    let healthy_provider_id = "openrouter";
    let isolated_provider_id = "deepseek";
    let healthy_connection_id = "openrouter-account";
    let isolated_connection_id = "deepseek-account";
    let mut configuration = PersistedAiConfiguration::default();
    configuration.connection_graph.connections.extend([
        (
            healthy_connection_id.to_string(),
            custom_provider_connection(healthy_connection_id, healthy_provider_id),
        ),
        (
            isolated_connection_id.to_string(),
            custom_provider_connection(isolated_connection_id, isolated_provider_id),
        ),
    ]);
    configuration
        .connection_graph
        .account_preferences
        .provider_defaults
        .extend([
            (
                healthy_provider_id.to_string(),
                healthy_connection_id.to_string(),
            ),
            (
                isolated_provider_id.to_string(),
                isolated_connection_id.to_string(),
            ),
        ]);
    configuration.active = Some(ActiveAiModelSelection {
        provider_id: isolated_provider_id.to_string(),
        model_id: String::from("deepseek-chat"),
        connection_id: isolated_connection_id.to_string(),
    });
    configuration.model_enabled_overrides.extend([
        (
            PersistedAiConfiguration::provider_model_key(healthy_provider_id, "openrouter/auto"),
            true,
        ),
        (
            PersistedAiConfiguration::provider_model_key(isolated_provider_id, "deepseek-chat"),
            true,
        ),
    ]);
    let isolated_provider_ids = BTreeSet::from([isolated_provider_id.to_string()]);

    let projection = configuration.without_provider_runtime_state(&isolated_provider_ids);

    assert!(
        configuration
            .connection_graph
            .connections
            .contains_key(isolated_connection_id),
        "the authoritative image must retain the isolated account for repair"
    );
    assert_eq!(
        projection
            .connection_graph
            .connections
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![healthy_connection_id.to_string()]
    );
    assert!(projection.active.is_none());
    assert_eq!(projection.model_enabled_overrides.len(), 1);
    assert!(
        projection.model_enabled_overrides.contains_key(
            PersistedAiConfiguration::provider_model_key(healthy_provider_id, "openrouter/auto")
                .as_str()
        )
    );
}

/// Verifies explicit deletion can remove an isolated reference without a hydrated descriptor.
/// 验证显式删除可在没有已补水描述符时移除一项隔离引用。
#[test]
fn isolated_custom_provider_reference_can_be_explicitly_removed() {
    let provider_id = "custom:isolated-removal";
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_provider_file_refs
        .insert(provider_id.to_string());
    configuration.custom_provider_manifest_failures = vec![CustomProviderManifestFailure {
        provider_id: provider_id.to_string(),
        message: String::from("invalid standalone document"),
    }];
    configuration.custom_provider_manifest_failure_total_count = 1;

    let removed = configuration.remove_custom_provider_owned_state(provider_id);

    assert!(removed.is_none());
    assert!(
        !configuration
            .custom_provider_file_refs
            .contains(provider_id)
    );
    assert!(configuration.custom_provider_manifest_failures.is_empty());
    assert_eq!(
        configuration.custom_provider_manifest_failure_total_count,
        0
    );
    assert!(!configuration.custom_provider_manifest_failures_truncated);
}

/// Verifies provider and replacement routes remain independently reusable when the active mode changes.
/// 验证切换当前模式时供应商专用与模型代替路由仍可独立复用。
#[test]
fn media_understanding_backend_modes_keep_independent_routes() {
    let capability = ProviderToolCapabilityKind::VideoUnderstand;
    let provider_selection = ProviderToolTargetSelection {
        connection_id: String::from("provider-account"),
        target_id: String::from("provider-video"),
    };
    let replacement_selection = ActiveAiModelSelection {
        provider_id: String::from("replacement-provider"),
        model_id: String::from("replacement-video-model"),
        connection_id: String::from("replacement-account"),
    };
    let mut configuration = PersistedAiConfiguration::default();
    let mut backend_options = MediaUnderstandingBackendOptions::default();
    backend_options.update_from_selection(&MediaUnderstandingBackendSelection::ProviderTool {
        selection: provider_selection.clone(),
    });
    backend_options.update_from_selection(&MediaUnderstandingBackendSelection::ModelReplacement {
        selection: replacement_selection.clone(),
        reasoning_effort: Some(ReasoningEffort::Medium),
    });
    configuration
        .media_understanding_backend_options
        .insert(capability, backend_options);

    configuration
        .apply_media_understanding_mode(capability, MediaUnderstandingBackendMode::ModelReplacement)
        .expect("replacement route should be selectable");
    assert_eq!(
        configuration
            .media_understanding_selections
            .get(&capability),
        Some(&MediaUnderstandingBackendSelection::ModelReplacement {
            selection: replacement_selection,
            reasoning_effort: Some(ReasoningEffort::Medium),
        })
    );

    configuration
        .apply_media_understanding_mode(capability, MediaUnderstandingBackendMode::ProviderTool)
        .expect("provider route should remain selectable");
    assert_eq!(
        configuration
            .media_understanding_selections
            .get(&capability),
        Some(&MediaUnderstandingBackendSelection::ProviderTool {
            selection: provider_selection,
        })
    );
}

/// Verifies conversation publication and non-conversation internal use are independent states.
/// 验证会话发布与非会话内部使用是相互独立的状态。
#[test]
fn conversation_and_internal_model_states_are_independent() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.set_model_enabled("opencode-go", "deepseek-v4-flash", false);
    configuration.set_internal_model_enabled("opencode-go", "deepseek-v4-flash", true);

    assert!(!configuration.model_is_enabled("opencode-go", "deepseek-v4-flash"));
    assert!(configuration.internal_model_is_enabled("opencode-go", "deepseek-v4-flash", false,));

    configuration.set_model_enabled("opencode-go", "deepseek-v4-flash", true);
    configuration.set_internal_model_enabled("opencode-go", "deepseek-v4-flash", false);
    assert!(configuration.model_is_enabled("opencode-go", "deepseek-v4-flash"));
    assert!(!configuration.internal_model_is_enabled("opencode-go", "deepseek-v4-flash", true,));

    configuration.restore_internal_model_enabled("opencode-go", "deepseek-v4-flash");
    assert!(configuration.internal_model_is_enabled("opencode-go", "deepseek-v4-flash", true,));
}

/// Verifies model overrides are independent from credential connection identifiers.
/// 验证模型覆盖独立于凭据连接标识。
#[test]
fn model_override_key_uses_provider_model_and_protocol_scope() {
    let model_key = PersistedAiConfiguration::provider_model_key("custom:迁移", "model:alpha");
    assert_eq!(
        PersistedAiConfiguration::provider_model_key_parts(model_key.as_str()),
        Some(("custom:迁移", "model:alpha"))
    );
    assert!(
        PersistedAiConfiguration::provider_model_key_parts(
            format!("{model_key}:trailing").as_str()
        )
        .is_none()
    );

    let key = PersistedAiConfiguration::provider_model_protocol_key(
        "custom:迁移",
        "model:alpha",
        "anthropic-messages.v1",
    );
    assert_eq!(
        PersistedAiConfiguration::provider_model_protocol_key_parts(key.as_str()),
        Some(("custom:迁移", "model:alpha", "anthropic-messages.v1"))
    );
    assert!(
        PersistedAiConfiguration::provider_model_protocol_key_parts(
            format!("{key}:trailing").as_str()
        )
        .is_none()
    );
}

/// Verifies candidate switches and replacement backends are removed with their owning model.
/// 验证候选开关与替代后端会随其所属模型一并移除。
#[test]
fn model_removal_cleans_multimodal_candidate_and_replacement_backend() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.set_multimodal_understanding_candidate_enabled(
        "custom:vision",
        "vision-model",
        true,
    );
    configuration.media_understanding_selections.insert(
        ProviderToolCapabilityKind::VideoUnderstand,
        MediaUnderstandingBackendSelection::ModelReplacement {
            selection: ActiveAiModelSelection {
                provider_id: String::from("custom:vision"),
                model_id: String::from("vision-model"),
                connection_id: String::from("vision-account"),
            },
            reasoning_effort: Some(ReasoningEffort::Medium),
        },
    );

    configuration.remove_model_owned_settings("custom:vision", "vision-model");

    assert!(
        !configuration
            .multimodal_understanding_candidate_is_enabled("custom:vision", "vision-model")
    );
    assert!(
        !configuration
            .media_understanding_selections
            .contains_key(&ProviderToolCapabilityKind::VideoUnderstand)
    );
}

/// Verifies disabling one candidate clears only replacement routes owned by that exact model.
/// 验证禁用一个候选模型只会清理该精确模型持有的替代路由。
#[test]
fn candidate_disable_requires_exact_model_reselection() {
    // configuration contains two admitted models and one unrelated provider-specific backend.
    // configuration 包含两个已准入模型与一个无关的供应商专用后端。
    let mut configuration = PersistedAiConfiguration::default();
    for model_id in ["disabled-model", "retained-model"] {
        configuration.set_multimodal_understanding_candidate_enabled(
            "media-provider",
            model_id,
            true,
        );
    }
    for (capability, model_id) in [
        (
            ProviderToolCapabilityKind::ImageUnderstand,
            "disabled-model",
        ),
        (
            ProviderToolCapabilityKind::VideoUnderstand,
            "retained-model",
        ),
    ] {
        configuration.media_understanding_selections.insert(
            capability,
            MediaUnderstandingBackendSelection::ModelReplacement {
                selection: ActiveAiModelSelection {
                    provider_id: String::from("media-provider"),
                    model_id: model_id.to_string(),
                    connection_id: String::from("media-account"),
                },
                reasoning_effort: None,
            },
        );
    }
    configuration.media_understanding_selections.insert(
        ProviderToolCapabilityKind::AudioUnderstand,
        MediaUnderstandingBackendSelection::ProviderTool {
            selection: ProviderToolTargetSelection {
                connection_id: String::from("media-account"),
                target_id: String::from("audio-target"),
            },
        },
    );

    configuration.set_multimodal_understanding_candidate_enabled(
        "media-provider",
        "disabled-model",
        false,
    );

    assert!(
        !configuration
            .media_understanding_selections
            .contains_key(&ProviderToolCapabilityKind::ImageUnderstand)
    );
    assert!(
        configuration
            .media_understanding_selections
            .contains_key(&ProviderToolCapabilityKind::VideoUnderstand)
    );
    assert!(
        configuration
            .media_understanding_selections
            .contains_key(&ProviderToolCapabilityKind::AudioUnderstand)
    );
    assert!(
        configuration
            .multimodal_understanding_candidate_is_enabled("media-provider", "retained-model")
    );
}

/// Verifies provider removal cleans both provider-tool and model-replacement media backends.
/// 验证移除供应商会同时清理供应商工具与模型代替媒体后端。
#[test]
fn provider_removal_cleans_every_owned_media_backend() {
    let provider_id = "custom:media-owner";
    let mut configuration = PersistedAiConfiguration::default();
    configuration.connection_graph.connections.insert(
        String::from("media-owner-account"),
        custom_provider_connection("media-owner-account", provider_id),
    );
    configuration.set_multimodal_understanding_candidate_enabled(provider_id, "media-model", true);
    configuration.media_understanding_selections.insert(
        ProviderToolCapabilityKind::ImageUnderstand,
        MediaUnderstandingBackendSelection::ProviderTool {
            selection: ProviderToolTargetSelection {
                connection_id: String::from("media-owner-account"),
                target_id: String::from("image-target"),
            },
        },
    );
    configuration.media_understanding_selections.insert(
        ProviderToolCapabilityKind::AudioUnderstand,
        MediaUnderstandingBackendSelection::ModelReplacement {
            selection: ActiveAiModelSelection {
                provider_id: provider_id.to_string(),
                model_id: String::from("media-model"),
                connection_id: String::from("media-owner-account"),
            },
            reasoning_effort: None,
        },
    );

    assert!(configuration.provider_has_model_customizations(provider_id));
    configuration.remove_provider_owned_model_settings(provider_id);

    assert!(
        configuration
            .multimodal_understanding_candidate_models
            .is_empty()
    );
    assert!(configuration.media_understanding_selections.is_empty());
}

/// Verifies a confirmed protocol switch removes only incompatible route settings for that provider.
/// 验证已确认协议切换只会移除该供应商不兼容的路由设置。
#[test]
fn protocol_switch_cleanup_preserves_target_and_other_provider_settings() {
    let mut configuration = PersistedAiConfiguration::default();
    for (provider_id, protocol_id) in [
        ("custom:test", "openai-chat-completions.v1"),
        ("custom:test", "openai-responses.v1"),
        ("custom:other", "openai-chat-completions.v1"),
    ] {
        configuration.model_overrides.insert(
            PersistedAiConfiguration::provider_model_protocol_key(
                provider_id,
                "private/chat",
                protocol_id,
            ),
            ModelExecutionOverrides {
                prompt_cache: Some(PromptCachePolicy::ProviderDefault),
                ..ModelExecutionOverrides::default()
            },
        );
    }

    assert_eq!(
        configuration.inactive_provider_model_override_count("custom:test", "openai-responses.v1"),
        1
    );
    configuration.remove_inactive_provider_model_overrides("custom:test", "openai-responses.v1");

    assert_eq!(configuration.model_overrides.len(), 2);
    assert!(
        configuration.model_overrides.contains_key(
            PersistedAiConfiguration::provider_model_protocol_key(
                "custom:test",
                "private/chat",
                "openai-responses.v1"
            )
            .as_str()
        )
    );
    assert!(
        configuration.model_overrides.contains_key(
            PersistedAiConfiguration::provider_model_protocol_key(
                "custom:other",
                "private/chat",
                "openai-chat-completions.v1"
            )
            .as_str()
        )
    );
}

/// Verifies the removed-protocol set cleanup deletes only that protocol's overrides,
/// while explicit preferences pointing at the removed protocol survive so binding keeps
/// failing closed until the user re-selects (plan §R6.8).
/// 验证被删除协议集合的清理只删除该协议拥有的覆盖；指向被删协议的显式偏好被保留，
/// 使绑定持续失败封闭直到用户重选（方案 §R6.8）。
#[test]
fn removed_protocol_cleanup_is_scoped_to_the_removed_set() {
    let mut configuration = PersistedAiConfiguration::default();
    for (provider_id, protocol_id) in [
        ("custom:test", "openai-chat-completions.v1"),
        ("custom:test", "openai-responses.v1"),
        ("custom:test", "anthropic-messages.v1"),
        ("custom:other", "openai-chat-completions.v1"),
    ] {
        configuration.model_overrides.insert(
            PersistedAiConfiguration::provider_model_protocol_key(
                provider_id,
                "private/chat",
                protocol_id,
            ),
            ModelExecutionOverrides {
                prompt_cache: Some(PromptCachePolicy::ProviderDefault),
                ..ModelExecutionOverrides::default()
            },
        );
    }
    configuration.custom_model_protocol_preferences.insert(
        PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
        crate::CustomModelProtocolPreference::Explicit {
            protocol: crate::CustomProviderProtocol::OpenAiChat,
        },
    );
    configuration.custom_model_protocol_preferences.insert(
        PersistedAiConfiguration::provider_model_key("custom:test", "private/chat2"),
        crate::CustomModelProtocolPreference::Explicit {
            protocol: crate::CustomProviderProtocol::OpenAiResponses,
        },
    );
    let removed = std::collections::BTreeSet::from([String::from("openai-chat-completions.v1")]);

    assert_eq!(
        configuration.removed_protocol_model_override_count("custom:test", &removed),
        1,
        "only the chat-completions override is owned by the removed protocol"
    );
    configuration.remove_removed_protocol_model_overrides("custom:test", &removed);
    assert_eq!(configuration.model_overrides.len(), 3);
    assert!(
        configuration.model_overrides.contains_key(
            PersistedAiConfiguration::provider_model_protocol_key(
                "custom:test",
                "private/chat",
                "openai-responses.v1"
            )
            .as_str()
        ),
        "retained protocols keep their overrides"
    );
    assert!(
        configuration.model_overrides.contains_key(
            PersistedAiConfiguration::provider_model_protocol_key(
                "custom:test",
                "private/chat",
                "anthropic-messages.v1"
            )
            .as_str()
        )
    );
    assert!(
        configuration.model_overrides.contains_key(
            PersistedAiConfiguration::provider_model_protocol_key(
                "custom:other",
                "private/chat",
                "openai-chat-completions.v1"
            )
            .as_str()
        ),
        "other providers are never touched"
    );

    // The explicit preference for the removed protocol survives: dropping it would
    // silently fall back to Auto, while retaining it keeps the model blocked
    // (ExplicitProtocolDisabled) until the user re-selects (plan §R6.8).
    // 指向被删协议的显式偏好被保留：删除它会静默回退 Auto，保留它则使模型保持
    // blocked（ExplicitProtocolDisabled）直到用户重选（方案 §R6.8）。
    assert!(
        configuration
            .custom_model_protocol_preferences
            .contains_key(
                PersistedAiConfiguration::provider_model_key("custom:test", "private/chat")
                    .as_str()
            ),
        "explicit preference for the removed protocol survives to keep the model blocked"
    );
    assert!(
        configuration
            .custom_model_protocol_preferences
            .contains_key(
                PersistedAiConfiguration::provider_model_key("custom:test", "private/chat2")
                    .as_str()
            ),
        "explicit preference for a retained protocol survives"
    );
}

/// Verifies configuration contains no provider-owned default policy.
/// 验证配置层不包含供应商拥有的默认策略。
#[test]
fn model_access_defaults_are_provider_neutral() {
    let configuration = PersistedAiConfiguration::default();
    assert!(configuration.model_is_enabled("openrouter", "vendor/unreviewed-model"));
    assert!(!configuration.model_is_enabled_with_default(
        "openrouter",
        "vendor/unreviewed-model",
        false
    ));
}

/// Verifies persisted access choices override either manifest-owned inherited default.
/// 验证持久化访问选择会覆盖任一清单拥有的继承默认值。
#[test]
fn model_access_overrides_win_over_inherited_defaults() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.set_model_enabled("openrouter", "vendor/unreviewed-model", true);
    assert!(configuration.model_is_enabled_with_default(
        "openrouter",
        "vendor/unreviewed-model",
        false
    ));
    assert_eq!(configuration.model_enabled_overrides.len(), 1);

    configuration.set_model_enabled("openrouter", "vendor/unreviewed-model", false);
    assert_eq!(configuration.model_enabled_overrides.len(), 1);
    assert!(!configuration.model_is_enabled_with_default(
        "openrouter",
        "vendor/unreviewed-model",
        true
    ));
    configuration.restore_model_enabled("openrouter", "vendor/unreviewed-model");
    assert!(configuration.model_enabled_overrides.is_empty());
}

/// Verifies one account stores only model decisions that differ from its inherited default.
/// 验证一个账号仅保存与其继承默认值不同的模型决定。
#[test]
fn connection_model_access_policy_persists_explicit_decisions() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.set_connection_model_enabled("account-a", "model-a", false);
    assert!(!configuration.connection_model_is_enabled("account-a", "model-a"));
    assert!(configuration.connection_model_is_enabled("account-a", "model-b"));
    assert_eq!(
        configuration
            .connection_model_access_policies
            .get("account-a")
            .expect("account policy must exist")
            .model_enabled_overrides,
        BTreeMap::from([(String::from("model-a"), false)])
    );

    configuration.set_connection_model_enabled("account-a", "model-a", true);
    assert!(
        configuration.connection_model_access_policies["account-a"].model_enabled_overrides["model-a"]
    );
    assert!(!configuration.connection_model_is_enabled_with_default(
        "account-a",
        "new-model",
        false,
    ));
}

/// Verifies account choices remain explicit while new signed models inherit their own defaults.
/// 验证账号选择保持显式，而新增签名模型继承各自的默认值。
#[test]
fn account_model_policy_keeps_signed_defaults_independent() {
    let mut policy = ConnectionModelAccessPolicy::default();
    assert!(policy.model_is_enabled_with_default("recommended", true));
    assert!(!policy.model_is_enabled_with_default("media", false));
    policy.set_model_enabled("recommended", false);
    policy.set_model_enabled("media", true);
    assert!(!policy.model_is_enabled_with_default("recommended", true));
    assert!(policy.model_is_enabled_with_default("media", false));
    assert!(policy.model_is_enabled_with_default("new-recommended", true));
    assert!(!policy.model_is_enabled_with_default("new-media", false));
    policy.set_all_models_enabled(true);
    assert!(policy.model_is_enabled_with_default("new-media", false));
    policy.set_all_models_enabled(false);
    assert!(!policy.model_is_enabled_with_default("new-recommended", true));
}

/// Verifies a bulk-disabled account keeps future discovered models disabled until the user changes it.
/// 验证批量禁用账号会让未来发现的模型继续保持禁用，直到用户主动更改。
#[test]
fn connection_model_access_default_applies_to_future_models() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.set_connection_models_enabled("account-a", false);
    assert!(!configuration.connection_model_is_enabled("account-a", "future-model"));

    configuration.set_connection_model_enabled("account-a", "future-model", true);
    assert!(configuration.connection_model_is_enabled("account-a", "future-model"));
    assert!(!configuration.connection_model_is_enabled("account-a", "another-future-model"));
}

/// Verifies disabling a credential is sparse, reversible, and clears every routing preference that referenced it.
/// 验证停用凭据采用稀疏持久化、可逆，并清理引用它的全部路由偏好。
#[test]
fn connection_enabled_state_is_sparse_reversible_and_removes_routing_preferences() {
    // connection is a real graph member so the state API cannot accept an orphan identifier.
    // connection 是真实连接图成员，因此状态接口不会接受孤立标识。
    let connection = custom_provider_connection("account-a", "custom:provider-a");
    // configuration carries all preference shapes that must stop referring to a disabled account.
    // configuration 承载停用账号后必须停止引用它的全部偏好形状。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .connections
        .insert(connection.id.clone(), connection.clone());
    configuration
        .connection_graph
        .account_preferences
        .provider_defaults
        .insert(connection.provider_id.clone(), connection.id.clone());
    configuration
        .connection_graph
        .account_preferences
        .model_last_connections
        .insert(String::from("model-key"), connection.id.clone());
    configuration.active = Some(ActiveAiModelSelection {
        provider_id: connection.provider_id.clone(),
        model_id: String::from("model-a"),
        connection_id: connection.id.clone(),
    });

    configuration
        .set_connection_enabled(connection.id.as_str(), false)
        .expect("configured connection should be disabled");
    assert!(!configuration.connection_is_enabled(connection.id.as_str()));
    assert!(configuration.active.is_none());
    assert!(
        configuration
            .connection_graph
            .account_preferences
            .provider_defaults
            .is_empty()
    );
    assert!(
        configuration
            .connection_graph
            .account_preferences
            .model_last_connections
            .is_empty()
    );

    configuration
        .set_connection_enabled(connection.id.as_str(), true)
        .expect("disabled connection should be enabled again");
    assert!(configuration.connection_is_enabled(connection.id.as_str()));
    assert!(configuration.disabled_connection_ids.is_empty());
    assert!(
        configuration
            .set_connection_enabled("missing-account", false)
            .is_err()
    );
}

/// Verifies explicit consumer dimensions survive serialization without changing peer profiles.
/// 验证消费者的明确维度在序列化后保留，且不会改变其他配置档。
///
/// # Returns
/// 返回值
/// Completes when the test consumer retains its numeric choice and legacy VMM keeps an absent choice.
/// 当测试消费者保留数值选择、旧 VMM 保持未设置时正常完成。
#[test]
fn inference_profile_preserves_explicit_consumer_embedding_dimensions() {
    // Each profile owns its choice; a missing peer value must not inherit the test consumer's setting.
    // 每个配置档独立拥有选择；缺少设置的配置档不能继承测试消费者的维度。
    let profiles: BTreeMap<String, InferenceProfile> = serde_json::from_value(serde_json::json!({
        "test-consumer": { "embedding_dimensions": 1024 },
        "vmm": {}
    }))
    .expect("valid consumer profiles");
    // Re-serialization exposes accidental field loss without relying on a new Rust field.
    // 再次序列化用于发现字段丢失，而不依赖新增 Rust 字段访问。
    let serialized = serde_json::to_value(&profiles).expect("serializable consumer profiles");
    assert_eq!(serialized["test-consumer"]["embedding_dimensions"], 1024);
    assert!(serialized["vmm"].get("embedding_dimensions").is_none());
}

/// Verifies a current-schema file missing a required system inference profile is rejected.
/// 验证缺少必需系统推理配置档的当前 Schema 文件会被拒绝。
#[test]
fn current_schema_rejects_missing_system_inference_profile() {
    // The retired consumer must not be recreated by in-memory defaults.
    // 内存默认值不能重新创建已经退役的消费者。
    assert!(
        !PersistedAiConfiguration::default()
            .inference_profiles
            .contains_key("vce")
    );
    // Configuration omits the sole remaining required system consumer.
    // 配置删除唯一仍然必需的系统消费者。
    let mut configuration = PersistedAiConfiguration::default();
    configuration.inference_profiles.remove("vmm");
    assert!(configuration.validate_current_schema().is_err());
}

/// Verifies protocol execution settings reject model limits as unknown fields.
/// 验证协议执行设置会把模型限制作为未知字段拒绝。
#[test]
fn protocol_execution_overrides_reject_model_limits() {
    let error = serde_json::from_str::<ModelExecutionOverrides>(r#"{"context_limit":8192}"#)
        .expect_err("protocol-scoped limits must be rejected");
    assert!(error.to_string().contains("unknown field"));
}

/// Verifies user-owned non-text output declarations in a current document are rejected on load.
/// 验证当前文档中用户持有的非文本输出声明会在加载时被拒绝。
#[test]
fn custom_non_text_output_declarations_are_rejected_on_load() {
    let provider_id = "custom:image-output";
    let model_id = "image-model";
    let mut with_model = PersistedAiConfiguration::default();
    with_model.custom_models.insert(
        String::from(provider_id),
        BTreeMap::from([(
            String::from(model_id),
            CustomModelDefinition {
                id: String::from(model_id),
                display_name: String::from("Image Model"),
                enabled: true,
                capabilities: ModelCapabilities {
                    output_resources: vec![ResourceKind::Image],
                    ..ModelCapabilities::default()
                },
                request_projection: Default::default(),
                tool_schema_projection: Default::default(),
                system_binding: None,
            },
        )]),
    );
    assert!(with_model.validate_current_schema().is_err());

    let mut with_template = PersistedAiConfiguration::default();
    with_template.custom_model_template_decisions.insert(
        PersistedAiConfiguration::provider_model_key(provider_id, model_id),
        CustomModelTemplateDecision::Applied {
            selection: crate::CustomModelTemplateSelection {
                template_key: String::from("openai/gpt-image-1"),
                template_revision: String::from(crate::CUSTOM_MODEL_TEMPLATE_REVISION),
                context_limit: 32_768,
                supports_text_input: true,
                supports_text_output: false,
                input_resources: Vec::new(),
                output_resources: vec![ResourceKind::Image],
            },
        },
    );
    assert!(with_template.validate_current_schema().is_err());
}

/// Verifies custom model availability uses the definition default and persists only deviations.
/// 验证自定义模型可用性使用定义默认值，并且仅持久化偏差。
#[test]
fn custom_model_access_uses_definition_default() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_models.insert(
        String::from("custom-provider"),
        BTreeMap::from([(
            String::from("private-model"),
            CustomModelDefinition {
                id: String::from("private-model"),
                display_name: String::from("Private Model"),
                enabled: false,
                capabilities: ModelCapabilities::default(),
                request_projection: Default::default(),
                tool_schema_projection: Default::default(),
                system_binding: None,
            },
        )]),
    );

    assert!(!configuration.model_is_enabled("custom-provider", "private-model"));
    configuration.set_model_enabled("custom-provider", "private-model", true);
    assert!(configuration.model_is_enabled("custom-provider", "private-model"));
    assert_eq!(configuration.model_enabled_overrides.len(), 1);
    configuration.set_model_enabled("custom-provider", "private-model", false);
    assert_eq!(configuration.model_enabled_overrides.len(), 1);
    configuration.restore_model_enabled("custom-provider", "private-model");
    assert!(configuration.model_enabled_overrides.is_empty());
}

/// Verifies takeover notices survive persistence and exact acknowledgement preserves others.
/// 验证接管提示可持久化，且精确确认不会清除其他提示。
#[test]
fn pending_system_model_takeover_notices_are_durable_and_exact() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.record_pending_system_model_takeover_notice("openrouter", "model-one");
    configuration.record_pending_system_model_takeover_notice("openrouter", "model-two");
    let serialized = serde_json::to_value(&configuration).expect("serialize pending notices");
    let mut reloaded: PersistedAiConfiguration =
        serde_json::from_value(serialized).expect("reload pending notices");
    reloaded.acknowledge_system_model_takeover_notices(&[(
        String::from("openrouter"),
        String::from("model-one"),
    )]);
    assert_eq!(
        reloaded.pending_system_model_takeover_notices["openrouter"],
        BTreeSet::from([String::from("model-two")])
    );
}

/// Verifies signed takeover removes manual parameters while retaining provider and account switches.
/// 验证签名接管移除手工参数，同时保留供应商及账号开关。
#[test]
fn signed_model_takeover_preserves_enabled_decisions() {
    let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
    let mut manifest = catalog
        .manifest("openrouter")
        .expect("bundled OpenRouter manifest")
        .clone();
    let model_id = "openai/gpt-5.6-sol";
    // The signed account default can differ from the previously enabled manual model.
    // 签名账号默认值可以与此前已启用的手工模型不同。
    for rule in &mut manifest.rule_instances {
        if rule.matcher.value == model_id {
            rule.default_enabled = false;
        }
    }
    let mut configuration = PersistedAiConfiguration::default();
    for connection_id in ["one", "two", "three"] {
        configuration.connection_graph.connections.insert(
            connection_id.to_string(),
            custom_provider_connection(connection_id, "openrouter"),
        );
    }
    configuration.custom_models.insert(
        String::from("openrouter"),
        BTreeMap::from([(
            model_id.to_string(),
            CustomModelDefinition {
                id: model_id.to_string(),
                display_name: String::from("Old manual entry"),
                enabled: true,
                capabilities: ModelCapabilities::default(),
                request_projection: Default::default(),
                tool_schema_projection: Default::default(),
                system_binding: None,
            },
        )]),
    );
    configuration
        .custom_model_connection_enabled_overrides
        .insert(
            PersistedAiConfiguration::connection_model_key("one", model_id),
            false,
        );
    configuration.set_connection_model_enabled("two", model_id, false);
    let model_key = PersistedAiConfiguration::provider_model_key("openrouter", model_id);
    configuration.model_capability_overrides.insert(
        model_key.clone(),
        ModelCapabilityOverrides {
            context_limit: Some(8_192),
            ..ModelCapabilityOverrides::default()
        },
    );

    assert_eq!(
        configuration.reconcile_signed_manual_model_collisions(&manifest),
        vec![model_id]
    );
    assert!(configuration.custom_model("openrouter", model_id).is_none());
    assert!(
        !configuration
            .model_capability_overrides
            .contains_key(&model_key)
    );
    assert!(configuration.model_is_enabled("openrouter", model_id));
    assert!(!configuration.connection_model_is_enabled("one", model_id));
    assert!(!configuration.connection_model_is_enabled("two", model_id));
    assert!(configuration.connection_model_is_enabled("three", model_id));
    assert!(configuration.connection_model_is_enabled("three", "openai/gpt-5.6-terra"));
    assert!(
        configuration
            .custom_model_connection_enabled_overrides
            .is_empty()
    );
    assert!(
        configuration
            .reconcile_signed_manual_model_collisions(&manifest)
            .is_empty()
    );
}

/// Verifies removed signed identities lose stale choices while manual identities retain theirs.
/// 验证消失的签名身份清除过期选择，而手工身份保留自己的选择。
#[test]
fn signed_inventory_cleanup_preserves_manual_models() {
    let catalog = crate::provider_manifest::load_bundled_resolved_catalog();
    let manifest = catalog
        .manifest("openrouter")
        .expect("bundled OpenRouter manifest");
    let mut configuration = PersistedAiConfiguration::default();
    configuration.connection_graph.connections.insert(
        String::from("account"),
        custom_provider_connection("account", "openrouter"),
    );
    configuration.system_signed_model_inventory.insert(
        String::from("openrouter"),
        BTreeSet::from([
            String::from("retired-model"),
            String::from("retained-manual"),
        ]),
    );
    configuration.custom_models.insert(
        String::from("openrouter"),
        BTreeMap::from([(
            String::from("retained-manual"),
            CustomModelDefinition {
                id: String::from("retained-manual"),
                display_name: String::from("Retained Manual"),
                enabled: true,
                capabilities: ModelCapabilities::default(),
                request_projection: Default::default(),
                tool_schema_projection: Default::default(),
                system_binding: None,
            },
        )]),
    );
    for model_id in ["retired-model", "retained-manual"] {
        configuration.set_connection_model_enabled("account", model_id, false);
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("openrouter", model_id),
            ModelCapabilityOverrides {
                context_limit: Some(8_192),
                ..Default::default()
            },
        );
    }
    assert!(configuration.reconcile_signed_model_inventory(manifest));
    let account_policy = &configuration.connection_model_access_policies["account"];
    assert!(
        !account_policy
            .model_enabled_overrides
            .contains_key("retired-model")
    );
    assert_eq!(
        account_policy
            .model_enabled_overrides
            .get("retained-manual"),
        Some(&false)
    );
    assert!(
        configuration
            .model_capability_overrides("openrouter", "retired-model")
            .is_none()
    );
    assert!(
        configuration
            .model_capability_overrides("openrouter", "retained-manual")
            .is_some()
    );
    assert!(!configuration.reconcile_signed_model_inventory(manifest));
}

/// Verifies capability differences replace only declared fields on the effective model copy.
/// 验证能力差异仅替换有效模型副本上明确声明的字段。
#[test]
fn custom_capability_overrides_leave_immutable_source_fields_untouched() {
    let mut configuration = PersistedAiConfiguration::default();
    let key = PersistedAiConfiguration::provider_model_key("custom:provider", "model");
    configuration.model_capability_overrides.insert(
        key,
        ModelCapabilityOverrides {
            context_limit: Some(32_000),
            output_limit: None,
            input_resources: Some(vec![ResourceKind::Image]),
            structured_output: Some(StructuredOutputCapability::JsonSchema),
            ..ModelCapabilityOverrides::default()
        },
    );
    let mut effective = ModelCapabilities {
        context_limit: Some(200_000),
        input_resources: Vec::new(),
        output_resources: vec![ResourceKind::File],
        structured_output: StructuredOutputCapability::Unsupported,
        ..ModelCapabilities::default()
    };

    configuration.apply_model_capability_overrides("custom:provider", "model", &mut effective);

    assert_eq!(effective.context_limit, Some(32_000));
    assert_eq!(effective.input_resources, vec![ResourceKind::Image]);
    assert_eq!(effective.output_resources, vec![ResourceKind::File]);
    assert_eq!(
        effective.structured_output,
        StructuredOutputCapability::JsonSchema
    );

    configuration
        .model_capability_overrides
        .get_mut(PersistedAiConfiguration::provider_model_key("custom:provider", "model").as_str())
        .expect("model capability overrides must remain available")
        .structured_output = Some(StructuredOutputCapability::Unsupported);
    configuration.apply_model_capability_overrides("custom:provider", "model", &mut effective);
    assert_eq!(
        effective.structured_output,
        StructuredOutputCapability::Unsupported
    );
}

/// Verifies system providers ignore all persisted model capability overrides.
/// 验证系统供应商忽略全部持久化模型能力覆盖。
#[test]
fn system_provider_capability_contract_is_not_user_overridable() {
    assert!(
        ModelCapabilityOverrides {
            context_limit: Some(200_000),
            output_limit: Some(32_000),
            ..Default::default()
        }
        .validate_for_provider("system-provider")
        .is_err()
    );
    let mut configuration = PersistedAiConfiguration::default();
    let key = PersistedAiConfiguration::provider_model_key("minimax", "MiniMax-M2.7");
    let stale = ModelCapabilityOverrides {
        context_limit: Some(128_000),
        input_resources: Some(vec![ResourceKind::Image]),
        structured_output: Some(StructuredOutputCapability::JsonSchema),
        reasoning: Some(CapabilitySupport::Unsupported),
        reasoning_control: Some(ReasoningControlMode::FixedOff),
        reasoning_efforts: Some(vec![ReasoningEffort::None]),
        request_projection: Some(CustomModelRequestProjection::default()),
        ..ModelCapabilityOverrides::default()
    };
    assert!(stale.validate_for_provider("minimax").is_err());
    configuration
        .model_capability_overrides
        .insert(key.clone(), stale);
    let mut effective = ModelCapabilities {
        context_limit: Some(204_800),
        input_resources: Vec::new(),
        structured_output: StructuredOutputCapability::JsonObject,
        reasoning: CapabilitySupport::Native,
        reasoning_control: ReasoningControlMode::ProviderAdaptive,
        reasoning_efforts: vec![ReasoningEffort::Adaptive],
        ..ModelCapabilities::default()
    };

    configuration.apply_model_capability_overrides("minimax", "MiniMax-M2.7", &mut effective);

    assert_eq!(effective.context_limit, Some(204_800));
    assert!(effective.input_resources.is_empty());
    assert_eq!(
        effective.structured_output,
        StructuredOutputCapability::JsonObject
    );
    assert_eq!(effective.reasoning, CapabilitySupport::Native);
    assert_eq!(
        effective.reasoning_control,
        ReasoningControlMode::ProviderAdaptive
    );
    assert_eq!(effective.reasoning_efforts, [ReasoningEffort::Adaptive]);
    assert!(configuration.validate_current_schema().is_err());
}

/// Verifies misspelled capability fields fail instead of appearing to persist successfully.
/// 验证拼错的能力字段会直接失败，而不是表现为已成功持久化。
#[test]
fn capability_overrides_reject_unknown_fields() {
    let error = serde_json::from_str::<ModelCapabilityOverrides>(r#"{"context_limt":32000}"#)
        .expect_err("unknown capability field must be rejected");

    assert!(error.to_string().contains("unknown field"));
}

/// Verifies deleting a model cannot resurrect stale account, protocol, or internal settings later.
/// 验证删除模型后不会在未来重新激活过期账号、协议或内部设置。
#[test]
fn removing_model_clears_every_model_owned_non_route_setting() {
    let mut configuration = PersistedAiConfiguration::default();
    let provider_id = "custom-provider";
    let model_id = "private-model";
    let provider_model_key = PersistedAiConfiguration::provider_model_key(provider_id, model_id);
    let connection_model_key =
        PersistedAiConfiguration::connection_model_key("connection", model_id);
    configuration.connection_graph.connections.insert(
        String::from("connection"),
        crate::CredentialConnection {
            id: String::from("connection"),
            provider_id: String::from(provider_id),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: crate::CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Account"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        },
    );
    configuration
        .connection_graph
        .account_preferences
        .model_last_connections
        .insert(provider_model_key.clone(), String::from("connection"));
    configuration
        .model_enabled_overrides
        .insert(provider_model_key.clone(), true);
    configuration.model_capability_overrides.insert(
        provider_model_key.clone(),
        ModelCapabilityOverrides::default(),
    );
    configuration.model_overrides.insert(
        PersistedAiConfiguration::provider_model_protocol_key(provider_id, model_id, "openai_chat"),
        ModelExecutionOverrides::default(),
    );
    configuration.internal_model_enabled_overrides.insert(
        PersistedAiConfiguration::provider_model_key(provider_id, model_id),
        true,
    );
    configuration
        .secondary_routes
        .insert(connection_model_key, SecondaryRouteSelection::Automatic);
    configuration.custom_model_template_decisions.insert(
        provider_model_key,
        CustomModelTemplateDecision::Rejected {
            template_key: String::from("private-model"),
            template_revision: String::from(CUSTOM_MODEL_TEMPLATE_REVISION),
        },
    );
    configuration
        .custom_model_connection_enabled_overrides
        .insert(
            PersistedAiConfiguration::connection_model_key("connection", model_id),
            false,
        );

    configuration.remove_model_owned_settings(provider_id, model_id);

    assert!(configuration.model_enabled_overrides.is_empty());
    assert!(configuration.model_capability_overrides.is_empty());
    assert!(configuration.custom_model_template_decisions.is_empty());
    assert!(
        configuration
            .custom_model_connection_enabled_overrides
            .is_empty()
    );
    assert!(configuration.model_overrides.is_empty());
    assert!(configuration.internal_model_enabled_overrides.is_empty());
    assert!(configuration.secondary_routes.is_empty());
    assert!(
        configuration
            .connection_graph
            .account_preferences
            .model_last_connections
            .is_empty()
    );
}

/// Verifies provider removal warnings detect user-owned model information before cascade cleanup.
/// 验证供应商移除提示会在级联清理前检测用户自有模型信息。
#[test]
fn provider_model_customization_detection_reports_owned_state() {
    let provider_id = "custom-provider";
    let model_id = "private-model";
    let mut configuration = PersistedAiConfiguration::default();
    assert!(!configuration.provider_has_model_customizations(provider_id));

    configuration.custom_models.insert(
        String::from(provider_id),
        BTreeMap::from([(
            String::from(model_id),
            CustomModelDefinition {
                id: String::from(model_id),
                display_name: String::from("Private Model"),
                enabled: true,
                capabilities: ModelCapabilities::default(),
                request_projection: Default::default(),
                tool_schema_projection: Default::default(),
                system_binding: None,
            },
        )]),
    );
    assert!(configuration.provider_has_model_customizations(provider_id));
}

/// Verifies one explicit model protocol choice alone is disclosed as provider-owned state.
/// 验证仅有一项显式模型协议选择时也会披露为供应商自有状态。
#[test]
fn provider_model_customization_detection_includes_protocol_preference() {
    // provider_id and model_id identify the sole provider-owned customization below.
    // provider_id 与 model_id 标识下方唯一的供应商自有定制。
    let provider_id = "custom:protocol-only";
    let model_id = "private-model";
    // configuration intentionally contains no other model or route customization.
    // configuration 有意不包含其他模型或路由定制。
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_model_protocol_preferences.insert(
        PersistedAiConfiguration::provider_model_key(provider_id, model_id),
        CustomModelProtocolPreference::Explicit {
            protocol: CustomProviderProtocol::AnthropicMessages,
        },
    );

    assert!(configuration.provider_has_model_customizations(provider_id));
    assert!(!configuration.provider_has_model_customizations("custom:peer"));
}

/// Verifies an inactive media backend route is disclosed before provider cascade cleanup.
/// 验证未激活的媒体后端路由会在供应商级联清理前被披露。
#[test]
fn provider_model_customization_detection_includes_inactive_media_backend_route() {
    let provider_id = "custom:media-route";
    let model_id = "video-model";
    let mut configuration = PersistedAiConfiguration::default();
    configuration.media_understanding_backend_options.insert(
        ProviderToolCapabilityKind::VideoUnderstand,
        MediaUnderstandingBackendOptions {
            model_replacement: Some(MediaUnderstandingModelReplacementSelection {
                selection: ActiveAiModelSelection {
                    provider_id: String::from(provider_id),
                    model_id: String::from(model_id),
                    connection_id: String::from("media-account"),
                },
                reasoning_effort: None,
            }),
            ..MediaUnderstandingBackendOptions::default()
        },
    );

    assert!(configuration.provider_has_model_customizations(provider_id));
    assert!(!configuration.provider_has_model_customizations("custom:peer"));
}

/// Verifies deleting a custom provider clears every hydrated field without touching peers.
/// 验证删除自定义供应商会清理全部已补水字段且不会影响其他供应商。
#[test]
fn removing_custom_provider_clears_all_owned_state_without_touching_peers() {
    let mut configuration = PersistedAiConfiguration::default();
    let removed_provider = "custom:provider";
    let retained_provider = "custom:other-provider";
    // These descriptors own the removable and retained standalone manifest documents.
    // 这些描述分别持有待删除与保留的独立 Manifest 文档。
    let removed_descriptor = custom_provider_descriptor(removed_provider);
    let retained_descriptor = custom_provider_descriptor(retained_provider);
    // These manifests prove the hydrated runtime map follows the descriptor lifecycle.
    // 这些 Manifest 证明已补水运行时映射会遵循描述的生命周期。
    let removed_manifest = crate::project_custom_descriptor_manifest(&removed_descriptor)
        .expect("removed custom manifest should build");
    let retained_manifest = crate::project_custom_descriptor_manifest(&retained_descriptor)
        .expect("retained custom manifest should build");
    configuration
        .custom_providers
        .insert(String::from(removed_provider), removed_descriptor);
    configuration
        .custom_providers
        .insert(String::from(retained_provider), retained_descriptor);
    configuration.custom_provider_file_refs.extend([
        String::from(removed_provider),
        String::from(retained_provider),
    ]);
    configuration
        .custom_provider_manifests
        .insert(String::from(removed_provider), removed_manifest);
    configuration
        .custom_provider_manifests
        .insert(String::from(retained_provider), retained_manifest);
    configuration.provider_priority_overrides.extend([
        (String::from(removed_provider), 20),
        (String::from(retained_provider), 10),
    ]);
    let removed_key =
        PersistedAiConfiguration::provider_model_key(removed_provider, "private-model");
    let retained_key =
        PersistedAiConfiguration::provider_model_key(retained_provider, "public-model");
    // removed_internal_key and retained_internal_key cover operation-scoped model defaults.
    // removed_internal_key 与 retained_internal_key 覆盖操作作用域模型默认值。
    let removed_internal_key = PersistedAiConfiguration::internal_model_operation_key(
        removed_provider,
        "private-model",
        AiOperation::EmbeddingCreate,
    );
    let retained_internal_key = PersistedAiConfiguration::internal_model_operation_key(
        retained_provider,
        "public-model",
        AiOperation::EmbeddingCreate,
    );
    configuration.custom_models.insert(
        String::from(removed_provider),
        BTreeMap::from([(
            String::from("private-model"),
            CustomModelDefinition {
                id: String::from("private-model"),
                display_name: String::from("Private Model"),
                enabled: true,
                capabilities: ModelCapabilities::default(),
                request_projection: Default::default(),
                tool_schema_projection: Default::default(),
                system_binding: None,
            },
        )]),
    );
    configuration.custom_models.insert(
        String::from(retained_provider),
        BTreeMap::from([(
            String::from("public-model"),
            CustomModelDefinition {
                id: String::from("public-model"),
                display_name: String::from("Public Model"),
                enabled: true,
                capabilities: ModelCapabilities::default(),
                request_projection: Default::default(),
                tool_schema_projection: Default::default(),
                system_binding: None,
            },
        )]),
    );
    configuration
        .model_enabled_overrides
        .insert(removed_key.clone(), true);
    configuration
        .model_enabled_overrides
        .insert(retained_key.clone(), true);
    configuration
        .model_capability_overrides
        .insert(removed_key.clone(), ModelCapabilityOverrides::default());
    configuration.custom_provider_model_defaults.insert(
        String::from(removed_provider),
        CustomProviderModelDefaults::default(),
    );
    configuration.custom_model_template_decisions.insert(
        removed_key.clone(),
        CustomModelTemplateDecision::Rejected {
            template_key: String::from("private-model"),
            template_revision: String::from(CUSTOM_MODEL_TEMPLATE_REVISION),
        },
    );
    configuration.model_overrides.insert(
        PersistedAiConfiguration::provider_model_protocol_key(
            removed_provider,
            "private-model",
            "openai_chat",
        ),
        ModelExecutionOverrides::default(),
    );
    configuration
        .internal_model_enabled_overrides
        .insert(removed_internal_key.clone(), true);
    configuration
        .internal_model_overrides
        .insert(removed_internal_key, InternalModelOverrides::default());
    configuration.internal_model_overrides.insert(
        retained_internal_key.clone(),
        InternalModelOverrides::default(),
    );
    configuration
        .connection_graph
        .account_preferences
        .model_last_connections
        .insert(removed_key, String::from("removed-connection"));
    configuration.active = Some(ActiveAiModelSelection {
        provider_id: String::from(removed_provider),
        model_id: String::from("private-model"),
        connection_id: String::from("removed-connection"),
    });
    // removed_llm_route and retained_llm_route prove provider filtering preserves peer routes.
    // removed_llm_route 与 retained_llm_route 验证供应商过滤会保留其他供应商路由。
    let removed_llm_route = InferenceRouteConfiguration {
        provider_id: String::from(removed_provider),
        service_entry_id: String::from("removed-entry"),
        connection_id: String::from("removed-connection"),
        model_id: String::from("private-model"),
        binding: InferenceRouteBinding::Protocol {
            protocol_profile_id: String::from("removed-protocol"),
        },
        operation: AiOperation::ConversationRespond,
        usage_scope: InferenceUsageScope::InternalBackground,
    };
    let retained_llm_route = InferenceRouteConfiguration {
        provider_id: String::from(retained_provider),
        service_entry_id: String::from("retained-entry"),
        connection_id: String::from("retained-connection"),
        model_id: String::from("public-model"),
        binding: InferenceRouteBinding::Protocol {
            protocol_profile_id: String::from("retained-protocol"),
        },
        operation: AiOperation::ConversationRespond,
        usage_scope: InferenceUsageScope::InternalBackground,
    };
    // removed_embedding_route and retained_rerank_route cover the two optional route slots.
    // removed_embedding_route 与 retained_rerank_route 覆盖两个可选路由槽位。
    let removed_embedding_route = InferenceRouteConfiguration {
        provider_id: String::from(removed_provider),
        service_entry_id: String::from("removed-entry"),
        connection_id: String::from("removed-connection"),
        model_id: String::from("private-model"),
        binding: InferenceRouteBinding::Action {
            action_binding_id: String::from("removed-embedding-action"),
        },
        operation: AiOperation::EmbeddingCreate,
        usage_scope: InferenceUsageScope::InternalBackground,
    };
    let retained_rerank_route = InferenceRouteConfiguration {
        provider_id: String::from(retained_provider),
        service_entry_id: String::from("retained-entry"),
        connection_id: String::from("retained-connection"),
        model_id: String::from("public-model"),
        binding: InferenceRouteBinding::Action {
            action_binding_id: String::from("retained-rerank-action"),
        },
        operation: AiOperation::RerankDocuments,
        usage_scope: InferenceUsageScope::InternalBackground,
    };
    // removed_decision_route proves the decision slot is cleared with its provider.
    // removed_decision_route 验证判定槽位随其供应商一并清除。
    let removed_decision_route = InferenceRouteConfiguration {
        provider_id: String::from(removed_provider),
        service_entry_id: String::from("removed-entry"),
        connection_id: String::from("removed-connection"),
        model_id: String::from("private-model"),
        binding: InferenceRouteBinding::Action {
            action_binding_id: String::from("removed-decision-action"),
        },
        operation: AiOperation::DecisionEvaluate,
        usage_scope: InferenceUsageScope::InternalBackground,
    };
    configuration.inference_profiles.insert(
        String::from("mixed-profile"),
        InferenceProfile {
            llm_routes: BTreeMap::from([
                (String::from("removed-purpose"), removed_llm_route),
                (String::from("retained-purpose"), retained_llm_route),
            ]),
            llm_reasoning_efforts: BTreeMap::from([
                (String::from("removed-purpose"), ReasoningEffort::None),
                (String::from("retained-purpose"), ReasoningEffort::None),
            ]),
            embedding: Some(removed_embedding_route),
            embedding_dimensions: None,
            rerank: Some(retained_rerank_route),
            decision: Some(removed_decision_route),
        },
    );

    // The removed descriptor proves the comprehensive cleanup path owned the exact provider.
    // 被移除描述证明综合清理路径持有精确供应商。
    let removed_descriptor = configuration
        .remove_custom_provider_owned_state(removed_provider)
        .expect("custom provider should be removed");

    assert_eq!(removed_descriptor.id, removed_provider);
    assert_eq!(
        configuration
            .custom_providers
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![String::from(retained_provider)]
    );
    assert_eq!(
        configuration
            .custom_provider_file_refs
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec![String::from(retained_provider)]
    );
    assert_eq!(
        configuration
            .custom_provider_manifests
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![String::from(retained_provider)]
    );
    assert_eq!(
        configuration.provider_priority_overrides,
        BTreeMap::from([(String::from(retained_provider), 10)])
    );
    assert_eq!(
        configuration
            .custom_models
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![String::from(retained_provider)]
    );
    assert_eq!(
        configuration.model_enabled_overrides,
        BTreeMap::from([(retained_key, true)])
    );
    assert!(configuration.model_capability_overrides.is_empty());
    assert!(configuration.custom_provider_model_defaults.is_empty());
    assert!(configuration.custom_model_template_decisions.is_empty());
    assert!(configuration.model_overrides.is_empty());
    assert!(configuration.internal_model_enabled_overrides.is_empty());
    assert_eq!(
        configuration.internal_model_overrides,
        BTreeMap::from([(retained_internal_key, InternalModelOverrides::default())])
    );
    // profile is the surviving mixed profile after provider-scoped route cleanup.
    // profile 是按供应商清理路由后保留下来的混合配置档。
    let profile = configuration
        .inference_profiles
        .get("mixed-profile")
        .expect("mixed inference profile");
    assert_eq!(
        profile.llm_routes.keys().cloned().collect::<Vec<_>>(),
        vec![String::from("retained-purpose")]
    );
    assert_eq!(
        profile
            .llm_reasoning_efforts
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![String::from("retained-purpose")]
    );
    assert!(profile.embedding.is_none());
    assert!(profile.decision.is_none());
    assert_eq!(
        profile
            .rerank
            .as_ref()
            .map(|route| route.provider_id.as_str()),
        Some(retained_provider)
    );
    assert!(
        configuration
            .connection_graph
            .account_preferences
            .model_last_connections
            .is_empty()
    );
    assert!(configuration.active.is_none());
    assert!(!configuration.provider_has_model_customizations(removed_provider));
    assert!(configuration.provider_has_model_customizations(retained_provider));
}

/// Verifies loading invents no model overlays and custom-model overlays round-trip exactly.
/// 验证加载时不会虚构模型覆盖，并且自定义模型覆盖能够精确往返。
#[test]
fn model_overlay_round_trip_is_exact() {
    let mut historical = PersistedAiConfiguration::default();
    historical
        .validate_current_schema()
        .expect("current schema must load");
    assert!(historical.custom_models.is_empty());
    assert!(historical.model_capability_overrides.is_empty());

    let model = CustomModelDefinition {
        id: String::from("private-model"),
        display_name: String::from("Private Model"),
        enabled: true,
        capabilities: ModelCapabilities {
            context_limit: Some(200_000),
            input_resources: vec![ResourceKind::Image],
            ..ModelCapabilities::default()
        },
        request_projection: Default::default(),
        tool_schema_projection: Default::default(),
        system_binding: None,
    };
    historical.custom_models.insert(
        String::from("custom-provider"),
        BTreeMap::from([(model.id.clone(), model)]),
    );
    historical.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("custom-provider", "private-model"),
        ModelCapabilityOverrides {
            context_limit: Some(64_000),
            output_limit: None,
            input_resources: Some(Vec::new()),
            ..ModelCapabilityOverrides::default()
        },
    );

    let encoded = serde_json::to_string(&historical).expect("configuration must serialize");
    let decoded: PersistedAiConfiguration =
        serde_json::from_str(encoded.as_str()).expect("configuration must deserialize");
    assert_eq!(decoded.custom_models, historical.custom_models);
    assert_eq!(
        decoded.model_capability_overrides,
        historical.model_capability_overrides
    );
}

/// Verifies template selections preserve their exact text modality decisions through JSON.
/// 验证模板选择通过 JSON 往返时会精确保留文本模态决策。
#[test]
fn custom_model_template_text_capabilities_round_trip_exactly() {
    let key = PersistedAiConfiguration::provider_model_key("custom-provider", "image-model");
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_model_template_decisions.insert(
        key.clone(),
        CustomModelTemplateDecision::Applied {
            selection: CustomModelTemplateSelection {
                template_key: String::from("image-model"),
                template_revision: String::from(CUSTOM_MODEL_TEMPLATE_REVISION),
                context_limit: 200_000,
                supports_text_input: true,
                supports_text_output: false,
                input_resources: vec![ResourceKind::Image],
                output_resources: vec![ResourceKind::Image],
            },
        },
    );

    let encoded = serde_json::to_string(&configuration).expect("configuration must serialize");
    let decoded: PersistedAiConfiguration =
        serde_json::from_str(encoded.as_str()).expect("configuration must deserialize");

    assert_eq!(
        decoded.custom_model_template_decisions.get(key.as_str()),
        configuration
            .custom_model_template_decisions
            .get(key.as_str())
    );
}

/// Verifies every provider and model uses the same global percentage policy.
/// 验证所有供应商与模型使用同一份全局百分比策略。
#[test]
fn context_window_policy_is_identical_for_all_routes() {
    let configuration = PersistedAiConfiguration {
        context_window_policy_overrides: ContextWindowPolicyOverrides {
            effective_context_window_percent: Some(95),
            auto_compact_percent: Some(88),
            auto_compact_token_limit: Some(123_456),
            ..Default::default()
        },
        ..Default::default()
    };
    for (provider_id, model_id) in [("system", "model-a"), ("custom:proxy", "model-b")] {
        let policy = configuration
            .resolve_context_compaction_policy(provider_id, model_id, Some(500_000), None, None)
            .expect("global policy must resolve for every route");
        assert_eq!(policy.window.effective_context_window_percent, 95);
        assert_eq!(policy.window.auto_compact_percent, 88);
        assert_eq!(policy.window.auto_compact_token_limit, Some(123_456));
        assert_eq!(policy.tail_context_limit, Some(500_000));
    }
}

/// Verifies the five fixed node ids and their default low-to-high token values.
/// 验证五个固定节点标识及其默认从低到高 Token 值。
#[test]
fn context_compaction_window_nodes_default_to_the_fixed_contract() {
    let configuration = ContextCompactionWindowConfiguration::default();

    assert!(configuration.validate().is_ok());
    assert_eq!(
        configuration.nodes.map(|node| (node.id, node.tokens)),
        [
            (ContextCompactionWindowNodeId::CompressionWindow1, 200_000),
            (ContextCompactionWindowNodeId::CompressionWindow2, 256_000),
            (ContextCompactionWindowNodeId::CompressionWindow3, 272_000),
            (ContextCompactionWindowNodeId::CompressionWindow4, 373_000),
            (ContextCompactionWindowNodeId::CompressionWindow5, 500_000),
        ]
    );
}

/// Verifies new conversations inherit the configured node only below the real model limit.
/// 验证新会话仅在节点低于模型真实上限时继承所配节点。
#[test]
fn new_session_window_default_respects_model_hard_limit() {
    let mut configuration = PersistedAiConfiguration::default();
    assert_eq!(
        configuration
            .new_session_context_compaction_window_node_for_model(Some(1_000_000))
            .expect("follow-model default should resolve"),
        None
    );
    configuration.set_new_session_context_compaction_window_node_id(Some(
        ContextCompactionWindowNodeId::CompressionWindow3,
    ));
    assert_eq!(
        configuration
            .new_session_context_compaction_window_node_for_model(Some(1_000_000))
            .expect("node should fit a 1M model"),
        Some(ContextCompactionWindowNodeId::CompressionWindow3)
    );
    for limit in [Some(272_000), Some(250_000), None] {
        assert_eq!(
            configuration
                .new_session_context_compaction_window_node_for_model(limit)
                .expect("unsupported node should follow the model"),
            None
        );
    }
}

/// Verifies fixed nodes reject id reassignment and non-ascending token values.
/// 验证固定节点会拒绝标识重分配及非递增 Token 值。
#[test]
fn context_compaction_window_nodes_reject_identity_and_order_changes() {
    let mut wrong_id = ContextCompactionWindowConfiguration::default();
    wrong_id.nodes[0].id = ContextCompactionWindowNodeId::CompressionWindow2;
    assert!(
        wrong_id
            .validate()
            .expect_err("id change must fail")
            .contains("must use id")
    );

    let mut wrong_order = ContextCompactionWindowConfiguration::default();
    wrong_order.nodes[2].tokens = wrong_order.nodes[1].tokens;
    assert!(
        wrong_order
            .validate()
            .expect_err("equal node values must fail")
            .contains("strictly ascending")
    );
}

/// Verifies a session node changes only the resolved compaction base and retained-tail policy.
/// 验证会话节点只改变已解析压缩基准与尾部保留策略。
#[test]
fn context_compaction_window_node_resolves_below_the_model_hard_window() {
    let configuration = PersistedAiConfiguration::default();

    let policy = configuration
        .resolve_context_compaction_policy(
            "provider",
            "model",
            Some(1_000_000),
            Some(ContextCompactionWindowNodeId::CompressionWindow2),
            None,
        )
        .expect("256K node should resolve below a 1M model");

    assert_eq!(
        policy.compression_window_node_id,
        Some(ContextCompactionWindowNodeId::CompressionWindow2)
    );
    assert_eq!(policy.compression_window_limit, Some(256_000));
    assert_eq!(policy.tail_context_limit, Some(256_000));
}

/// Verifies equal or larger node values are rejected for the selected model.
/// 验证等于或大于所选模型窗口的节点值会被拒绝。
#[test]
fn context_compaction_window_node_must_be_strictly_lower_than_model_window() {
    let configuration = PersistedAiConfiguration::default();

    let error = configuration
        .resolve_context_compaction_policy(
            "provider",
            "model",
            Some(256_000),
            Some(ContextCompactionWindowNodeId::CompressionWindow2),
            None,
        )
        .expect_err("equal node and model windows must fail");

    assert!(error.contains("must be lower than model context limit"));
}

/// Verifies a child-task behavior override keeps the selected route window policy.
/// 验证子任务行为覆盖只改变压缩行为而仍继承选定路由的窗口策略。
#[test]
fn behavior_only_session_policy_inherits_selected_route_window() {
    let provider_id = "custom:proxy";
    let model_id = "gpt-5.6-luna";
    let configuration = PersistedAiConfiguration {
        context_window_policy_overrides: ContextWindowPolicyOverrides {
            effective_context_window_percent: Some(92),
            ..ContextWindowPolicyOverrides::default()
        },
        ..PersistedAiConfiguration::default()
    };

    let behavior_only = ContextCompactionPolicy {
        auto: false,
        ..ContextCompactionPolicy::default()
    };
    let policy = configuration
        .resolve_context_compaction_policy(
            provider_id,
            model_id,
            Some(300_000),
            None,
            Some(behavior_only),
        )
        .expect("behavior-only policy should resolve");

    assert!(!policy.auto);
    assert_eq!(policy.window.effective_context_window_percent, 92);
    assert!(policy.window_policy_is_explicit);
    assert_eq!(policy.tail_context_limit, Some(300_000));
    assert_eq!(policy.tail_turns, 6);
    assert_eq!(policy.preserve_recent_tokens, 15_000);
}

/// Verifies global ordering is atomic and scoped policy fields are rejected at decoding.
/// 验证全局顺序校验为原子操作，并在解码时拒绝局部策略字段。
#[test]
fn context_window_policy_rejects_invalid_global_ordering() {
    let mut configuration = PersistedAiConfiguration::default();
    let previous = configuration.context_window_policy_overrides;
    let error = configuration
        .set_context_window_policy_overrides(ContextWindowPolicyOverrides {
            effective_context_window_percent: Some(95),
            auto_compact_percent: Some(96),
            ..Default::default()
        })
        .expect_err("compaction cannot exceed the effective window");
    assert!(error.contains("cannot exceed"));
    assert_eq!(configuration.context_window_policy_overrides, previous);
    assert!(
        serde_json::from_value::<ModelCapabilityOverrides>(serde_json::json!({
            "context_window_policy": {"auto_compact_percent": 90}
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<CustomProviderModelDefaults>(serde_json::json!({
            "context_window_policy": {"auto_compact_percent": 90}
        }))
        .is_err()
    );
}

/// Verifies every non-current persisted schema generation, older or newer, is rejected.
/// 验证任何非当前持久化 Schema 代次（无论更旧或更新）均被拒绝。
#[test]
fn non_current_configuration_schema_is_rejected() {
    for schema_version in [
        0,
        PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION - 1,
        PERSISTED_AI_CONFIGURATION_SCHEMA_VERSION + 1,
    ] {
        let configuration = PersistedAiConfiguration {
            schema_version,
            ..PersistedAiConfiguration::default()
        };

        configuration
            .validate_current_schema()
            .expect_err("a non-current version must fail");
    }
}
