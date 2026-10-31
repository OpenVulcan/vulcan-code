//! Regression coverage for custom-provider identity, discovery acceptance, and exact route migration.
//! 自定义供应商身份、发现接纳与精确路由迁移的回归覆盖。

use super::*;
use vulcan_ai_contract::InferenceUsageScope;
use vulcan_provider_adapters::{CredentialMode, InferenceProfile};

/// Verifies removal accepts an exact isolated custom reference but never a system namespace.
/// 验证删除接受精确隔离自定义引用，但绝不接受系统命名空间。
#[test]
fn isolated_custom_provider_removal_identity_is_namespace_safe() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_provider_file_refs
        .insert(String::from("custom:isolated-removal"));
    configuration
        .custom_provider_file_refs
        .insert(String::from("openai"));

    assert_eq!(
        validate_removable_custom_provider_identity(&configuration, "custom:isolated-removal"),
        Ok(true)
    );
    let error = validate_removable_custom_provider_identity(&configuration, "openai")
        .expect_err("system provider identity must never enter custom removal");
    assert!(error.contains("custom:"), "{error}");
}

/// Verifies explicit Codex capability switches survive the custom-provider candidate projection.
/// 验证显式 Codex 能力开关可完整通过自定义供应商候选投影。
#[test]
fn custom_codex_provider_candidate_preserves_explicit_responses_capabilities() {
    // request represents a user explicitly enabling both opt-in Codex capabilities.
    // request 表示用户明确启用两项可选 Codex 能力。
    let request = PublicAiCustomProviderSaveRequest {
        provider_id: None,
        display_name: String::from("Codex Proxy"),
        base_url: String::from("https://proxy.example.test/v1"),
        protocols: vec![CustomProviderProtocol::OpenAiCodexResponses],
        remote_compaction_policy: ResponsesRemoteCompactionPolicy::EnabledWithLocalFallback,
        websocket_policy: ResponsesWebsocketPolicy::UnverifiedOverride,
        authentication: CustomProviderAuthentication::BearerAuthorization,
        credential_display_name: Some(String::from("Test Account")),
        secret: Some(String::from("test-secret")),
        validation_connection_id: None,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: Some(String::from("x-custom-session")),
        allow_manual_fallback: false,
        allow_pending_validation: false,
    };
    // candidate is the exact non-persisted descriptor produced by the save boundary.
    // candidate 是保存边界生成的精确未持久化描述符。
    let candidate =
        prepare_custom_provider_candidate(&request, &PersistedAiConfiguration::default())
            .expect("explicit Codex capability candidate should compile");
    // features is the route-scoped persisted capability projection under audit.
    // features 是本轮审核的路由级持久化能力投影。
    let features = candidate.descriptor.service_entries[0].protocol_routes[0]
        .responses_features
        .expect("Codex route should preserve explicit capability switches");

    assert_eq!(
        features.remote_compaction,
        ResponsesRemoteCompactionPolicy::EnabledWithLocalFallback
    );
    assert_eq!(
        features.websocket,
        ResponsesWebsocketPolicy::UnverifiedOverride
    );
    assert_eq!(
        candidate.descriptor.service_entries[0]
            .session_header
            .as_deref(),
        Some("x-custom-session")
    );
}

/// Verifies execution-only Codex capability switches do not trigger credential revalidation.
/// 验证只影响执行的 Codex 能力开关不会触发凭据重新验证。
#[test]
fn custom_codex_capability_switch_is_not_a_connection_contract_change() {
    // providerId identifies the exact credential-free custom provider being edited.
    // providerId 标识正在编辑的精确无凭据自定义供应商。
    let provider_id = String::from("custom:codex-capability-switch");
    // descriptor is the persisted Codex route with both optional capabilities disabled by default.
    // descriptor 是两项可选能力默认关闭的持久化 Codex 路由。
    let descriptor = CustomProviderDefinition {
        id: provider_id.clone(),
        display_name: String::from("Codex Proxy"),
        base_url: String::from("https://proxy.example.test/v1"),
        protocols: vec![CustomProviderProtocol::OpenAiCodexResponses],
        authentication: CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("default Codex provider should compile");
    // configuration owns the exact persisted descriptor used as the comparison baseline.
    // configuration 持有作为比较基线的精确持久化描述符。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_providers
        .insert(provider_id.clone(), descriptor.clone());
    // request changes only the two explicit execution capabilities.
    // request 仅修改两项显式执行能力。
    let request = PublicAiCustomProviderSaveRequest {
        provider_id: Some(provider_id),
        display_name: String::from("Codex Proxy"),
        base_url: String::from("https://proxy.example.test/v1"),
        protocols: vec![CustomProviderProtocol::OpenAiCodexResponses],
        remote_compaction_policy: ResponsesRemoteCompactionPolicy::EnabledWithLocalFallback,
        websocket_policy: ResponsesWebsocketPolicy::UnverifiedOverride,
        authentication: CustomProviderAuthentication::BearerAuthorization,
        credential_display_name: None,
        secret: None,
        validation_connection_id: None,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
        allow_manual_fallback: false,
        allow_pending_validation: false,
    };
    // candidate preserves the changed feature values while retaining the same connection contract.
    // candidate 保留已变更能力值，同时维持相同连接契约。
    let candidate = prepare_custom_provider_candidate(&request, &configuration)
        .expect("capability-only edit should compile");

    assert_ne!(
        descriptor.service_entries,
        candidate.descriptor.service_entries
    );
    assert!(
        !custom_provider_connection_contract_changed(&configuration, &candidate)
            .expect("capability-only edit should compare")
    );
}

/// Verifies protocol edits reuse secrets, migrate every credential, and reject partial invalid migrations.
/// 验证协议编辑会复用秘密、迁移全部凭据，并拒绝部分无效迁移。
#[test]
fn custom_provider_edit_atomically_migrates_every_existing_credential() {
    let provider_id = String::from("custom:test-provider");
    let descriptor = CustomProviderDefinition {
        id: provider_id.clone(),
        display_name: String::from("Test Provider"),
        base_url: String::from("https://old.example.test/v1"),
        protocols: vec![CustomProviderProtocol::OpenAiResponses],
        authentication: CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("custom provider fixture should compile");
    let entry = descriptor
        .service_entries
        .first()
        .expect("custom provider entry should exist");
    let profile = entry
        .credential_profiles
        .first()
        .expect("custom provider credential profile should exist");
    let route = entry
        .protocol_routes
        .first()
        .expect("custom provider protocol route should exist");
    let existing_protocol_id = route.protocol_profile_id.clone();
    let existing = CredentialConnection {
        id: String::from("connection-existing"),
        provider_id: provider_id.clone(),
        service_entry_id: entry.id.clone(),
        credential_profile_id: profile.id.clone(),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("credential-existing"),
        display_name: String::from("Existing Account"),
        secret_reference: String::from("secret-existing"),
        management_secret_reference: None,
        service_origin_override: None,
    };
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_providers
        .insert(provider_id.clone(), descriptor);
    configuration
        .connection_graph
        .connections
        .insert(existing.id.clone(), existing.clone());
    let request = PublicAiCustomProviderSaveRequest {
        provider_id: Some(provider_id.clone()),
        display_name: String::from("Renamed Provider"),
        base_url: String::from("https://new.example.test/v1"),
        protocols: vec![CustomProviderProtocol::OpenAiChat],
        remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
        websocket_policy: ResponsesWebsocketPolicy::Disabled,
        authentication: CustomProviderAuthentication::BearerAuthorization,
        credential_display_name: None,
        secret: None,
        validation_connection_id: Some(existing.id.clone()),
        model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
        allow_manual_fallback: false,
        allow_pending_validation: false,
    };

    let candidate = prepare_custom_provider_candidate(&request, &configuration)
        .expect("editing should reuse the selected account");
    assert!(candidate.updating_existing);
    assert!(candidate.temporary_secret.is_none());
    // candidate_connection is the selected account's isolated metadata-validation copy.
    // candidate_connection 是所选账号用于隔离元数据验证的副本。
    let candidate_connection = candidate
        .validation_connection
        .as_ref()
        .expect("selected credential should create a validation connection");
    assert_ne!(candidate_connection.id, existing.id);
    assert_eq!(
        candidate_connection.secret_reference,
        existing.secret_reference
    );
    assert_eq!(candidate_connection.credential_id, existing.credential_id);
    assert_eq!(candidate_connection.display_name, existing.display_name);
    assert_ne!(
        candidate.activation_target.default_protocol_profile_id,
        existing_protocol_id
    );
    assert!(
        custom_provider_connection_contract_changed(&configuration, &candidate)
            .expect("protocol edit should compare")
    );
    // cosmetic_request changes only the provider-facing display name.
    // cosmetic_request 仅修改面向用户的供应商展示名称。
    let cosmetic_request = PublicAiCustomProviderSaveRequest {
        provider_id: Some(provider_id.clone()),
        display_name: String::from("Cosmetic Rename"),
        base_url: String::from("https://old.example.test/v1"),
        protocols: vec![CustomProviderProtocol::OpenAiResponses],
        remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
        websocket_policy: ResponsesWebsocketPolicy::Disabled,
        authentication: CustomProviderAuthentication::BearerAuthorization,
        credential_display_name: None,
        secret: None,
        validation_connection_id: Some(existing.id.clone()),
        model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
        allow_manual_fallback: false,
        allow_pending_validation: false,
    };
    // cosmetic_candidate proves display-only edits do not disturb credential state.
    // cosmetic_candidate 验证仅展示信息编辑不会扰动凭据状态。
    let cosmetic_candidate = prepare_custom_provider_candidate(&cosmetic_request, &configuration)
        .expect("cosmetic edit should compile");
    assert!(
        !custom_provider_connection_contract_changed(&configuration, &cosmetic_candidate)
            .expect("cosmetic edit should compare")
    );
    // credential_free_configuration represents the legal state after the provider's last account is removed.
    // credential_free_configuration 表示供应商最后一个账号被移除后的合法状态。
    let mut credential_free_configuration = configuration.clone();
    credential_free_configuration
        .connection_graph
        .connections
        .clear();
    let credential_free_request = PublicAiCustomProviderSaveRequest {
        validation_connection_id: None,
        ..request.clone()
    };
    // credential_free_candidate keeps the target protocol exact without inventing secret-bearing state.
    // credential_free_candidate 在不虚构秘密状态的前提下保持目标协议精确。
    let credential_free_candidate =
        prepare_custom_provider_candidate(&credential_free_request, &credential_free_configuration)
            .expect("credential-free provider should remain editable");
    assert!(credential_free_candidate.validation_connection.is_none());
    assert!(
        custom_provider_edit_validation_candidates(
            &credential_free_configuration,
            &credential_free_candidate,
        )
        .expect("credential-free edit has no account to validate")
        .is_empty()
    );

    // sibling proves the provider protocol changes as one provider-wide operation.
    // sibling 验证供应商协议会作为供应商级操作整体切换。
    let mut sibling = existing.clone();
    sibling.id = String::from("connection-sibling");
    sibling.credential_id = String::from("credential-sibling");
    sibling.secret_reference = String::from("secret-sibling");
    configuration
        .connection_graph
        .connections
        .insert(sibling.id.clone(), sibling.clone());
    // inference_route represents a durable VMM route bound to the provider's old single protocol.
    // inference_route 表示绑定到供应商旧唯一协议的持久化 VMM 路由。
    let inference_route = InferenceRouteConfiguration {
        provider_id: provider_id.clone(),
        service_entry_id: existing.service_entry_id.clone(),
        connection_id: existing.id.clone(),
        model_id: String::from("internal-model"),
        binding: InferenceRouteBinding::Protocol {
            protocol_profile_id: existing_protocol_id,
        },
        operation: AiOperation::ConversationRespond,
        usage_scope: InferenceUsageScope::ExternalService,
    };
    configuration.inference_profiles.insert(
        String::from("vmm"),
        InferenceProfile {
            llm_routes: BTreeMap::from([(String::from("precheck_l1"), inference_route)]),
            ..InferenceProfile::default()
        },
    );
    // validation_candidates proves provider edits validate every credential without sharing secrets.
    // validation_candidates 验证供应商编辑会逐份验证凭据且不会共享秘密。
    let validation_candidates =
        custom_provider_edit_validation_candidates(&configuration, &candidate)
            .expect("every credential must receive a validation candidate");
    assert_eq!(validation_candidates.len(), 2);
    for (source_connection_id, validation_candidate) in &validation_candidates {
        // source_connection is the exact durable account mirrored by this temporary candidate.
        // source_connection 是此临时候选项所镜像的精确持久化账号。
        let source_connection =
            &configuration.connection_graph.connections[source_connection_id.as_str()];
        // validation_connection is the exact isolated copy for this source account.
        // validation_connection 是此源账号的精确隔离副本。
        let validation_connection = validation_candidate
            .validation_connection
            .as_ref()
            .expect("existing credential should create a validation connection");
        assert_ne!(validation_connection.id, source_connection.id);
        assert_eq!(
            validation_connection.secret_reference,
            source_connection.secret_reference
        );
        assert_eq!(
            validation_connection.service_entry_id,
            candidate.activation_target.service_entry_id
        );
        assert_eq!(
            validation_connection.credential_profile_id,
            candidate.activation_target.credential_profile_id
        );
    }
    // providers is the effective registry containing only the newly selected custom protocol.
    // providers 是仅包含新选自定义协议的有效供应商注册表。
    let providers = ProviderRegistry::effective(&BTreeMap::from([(
        provider_id.clone(),
        candidate.descriptor.clone(),
    )]))
    .expect("new custom provider descriptor must register");
    // validation_outcomes proves each credential carries its own verification result into migration.
    // validation_outcomes 验证每份凭据都会把自身验证结果带入迁移。
    let validation_outcomes = BTreeMap::from([
        (
            existing.id.clone(),
            CustomProviderConnectionValidationOutcome {
                pending_validation: false,
                catalog: None,
                status: CatalogDiscoveryStatus {
                    state: CatalogDiscoveryState::Ready,
                    failure_kind: None,
                    message: None,
                    model_count: 1,
                    generation: 1,
                    observed_at_ms: 1,
                },
                refresh_error: None,
            },
        ),
        (
            sibling.id.clone(),
            CustomProviderConnectionValidationOutcome {
                pending_validation: true,
                catalog: None,
                status: CatalogDiscoveryStatus {
                    state: CatalogDiscoveryState::Ready,
                    failure_kind: None,
                    message: None,
                    model_count: 1,
                    generation: 1,
                    observed_at_ms: 1,
                },
                refresh_error: None,
            },
        ),
    ]);
    migrate_custom_provider_connections(
        &providers,
        &mut configuration,
        provider_id.as_str(),
        &candidate.activation_target,
        &validation_outcomes,
    )
    .expect("every compatible credential must migrate");
    for (connection_id, secret_reference) in [
        (existing.id.as_str(), existing.secret_reference.as_str()),
        (sibling.id.as_str(), sibling.secret_reference.as_str()),
    ] {
        let migrated = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .expect("migrated connection must remain addressable");
        assert_eq!(
            migrated.service_entry_id,
            candidate.activation_target.service_entry_id
        );
        assert_eq!(
            migrated.credential_profile_id,
            candidate.activation_target.credential_profile_id
        );
        assert_eq!(migrated.secret_reference, secret_reference);
    }
    // migrated_inference_route proves internal consumers remain executable after the protocol switch.
    // migrated_inference_route 验证内部消费者在协议切换后仍保持可执行。
    let migrated_inference_route =
        &configuration.inference_profiles["vmm"].llm_routes["precheck_l1"];
    assert_eq!(
        migrated_inference_route.service_entry_id,
        candidate.activation_target.service_entry_id
    );
    assert_eq!(
        migrated_inference_route.binding,
        InferenceRouteBinding::Protocol {
            protocol_profile_id: candidate
                .activation_target
                .default_protocol_profile_id
                .clone(),
        }
    );

    // failing_configuration contains one credential mode the new token profile cannot accept.
    // failing_configuration 包含一个新 Token 配置档无法接受的凭据模式。
    let mut failing_configuration = PersistedAiConfiguration::default();
    failing_configuration
        .connection_graph
        .connections
        .insert(existing.id.clone(), existing);
    let mut incompatible = sibling;
    incompatible.credential_mode = CredentialMode::Auth;
    failing_configuration
        .connection_graph
        .connections
        .insert(incompatible.id.clone(), incompatible);
    let original_graph = failing_configuration.connection_graph.clone();
    assert!(
        migrate_custom_provider_connections(
            &providers,
            &mut failing_configuration,
            provider_id.as_str(),
            &candidate.activation_target,
            &validation_outcomes,
        )
        .is_err()
    );
    assert_eq!(failing_configuration.connection_graph, original_graph);

    let invalid_request = PublicAiCustomProviderSaveRequest {
        secret: Some(String::from("replacement-secret")),
        ..request
    };
    assert!(
        prepare_custom_provider_candidate(&invalid_request, &configuration)
            .err()
            .expect("editing must reject replacement secret material")
            .contains("cannot create a new secret")
    );
}

/// Verifies every discovery state accepts only its exact explicit save path.
/// 验证每个发现状态只接受其精确对应的显式保存路径。
#[test]
fn custom_provider_observation_acceptance_is_closed_and_explicit() {
    assert!(custom_provider_observation_is_accepted(
        CatalogDiscoveryState::Ready,
        false,
        false,
    ));
    for state in [
        CatalogDiscoveryState::Empty,
        CatalogDiscoveryState::Unsupported,
        CatalogDiscoveryState::InvalidResponse,
    ] {
        assert!(!custom_provider_observation_is_accepted(
            state, false, false,
        ));
        assert!(custom_provider_observation_is_accepted(state, true, false));
        assert!(!custom_provider_observation_is_accepted(state, false, true,));
    }
    for state in [
        CatalogDiscoveryState::RateLimited,
        CatalogDiscoveryState::TemporarilyUnavailable,
        CatalogDiscoveryState::Failed,
    ] {
        assert!(!custom_provider_observation_is_accepted(
            state, false, false,
        ));
        assert!(!custom_provider_observation_is_accepted(state, true, false,));
        assert!(custom_provider_observation_is_accepted(state, false, true));
    }
    for state in [
        CatalogDiscoveryState::NotChecked,
        CatalogDiscoveryState::AuthenticationFailed,
        CatalogDiscoveryState::Stale,
    ] {
        assert!(!custom_provider_observation_is_accepted(
            state, false, false,
        ));
        assert!(!custom_provider_observation_is_accepted(state, true, true,));
    }
}

/// Verifies only accepted non-ready states are marked pending validation.
/// 验证只有被接受的未就绪状态才会标记为待验证。
#[test]
fn custom_provider_pending_validation_matches_non_ready_acceptance() {
    assert!(!custom_provider_save_requires_pending_validation(
        CatalogDiscoveryState::Ready,
        false,
        false,
    ));
    for state in [
        CatalogDiscoveryState::Empty,
        CatalogDiscoveryState::Unsupported,
        CatalogDiscoveryState::InvalidResponse,
    ] {
        assert!(!custom_provider_save_requires_pending_validation(
            state, false, false,
        ));
        assert!(custom_provider_save_requires_pending_validation(
            state, true, false,
        ));
    }
    for state in [
        CatalogDiscoveryState::RateLimited,
        CatalogDiscoveryState::TemporarilyUnavailable,
        CatalogDiscoveryState::Failed,
    ] {
        assert!(!custom_provider_save_requires_pending_validation(
            state, false, false,
        ));
        assert!(custom_provider_save_requires_pending_validation(
            state, false, true,
        ));
    }
    for state in [
        CatalogDiscoveryState::NotChecked,
        CatalogDiscoveryState::AuthenticationFailed,
        CatalogDiscoveryState::Stale,
    ] {
        assert!(!custom_provider_save_requires_pending_validation(
            state, true, true,
        ));
    }
}

/// Verifies multi-protocol activation never replaces a model binding with the provider default.
/// 验证多协议激活绝不会用供应商默认值替换模型绑定。
#[test]
fn multi_protocol_migration_preserves_exact_internal_route_binding() {
    // target exposes Responses first while retaining Chat as another enabled protocol.
    // target 首选 Responses，同时保留 Chat 作为另一项启用协议。
    let target = CustomProviderActivationTarget {
        service_entry_id: String::from("custom-entry-v2"),
        credential_profile_id: String::from("api-key"),
        default_protocol_profile_id: String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID),
        protocol_profile_ids: BTreeSet::from([
            String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID),
            String::from(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        ]),
    };
    // route represents a model explicitly bound to Chat before the provider edit.
    // route 表示供应商编辑前显式绑定到 Chat 的模型。
    let mut route = InferenceRouteConfiguration {
        provider_id: String::from("custom:multi-route"),
        service_entry_id: String::from("custom-entry-v1"),
        connection_id: String::from("custom-account"),
        model_id: String::from("chat-only-model"),
        binding: InferenceRouteBinding::Protocol {
            protocol_profile_id: String::from(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
        },
        operation: AiOperation::ConversationRespond,
        usage_scope: InferenceUsageScope::ExternalService,
    };
    // owned_connections scopes migration to the exact provider account.
    // owned_connections 将迁移限定到精确供应商账号。
    let owned_connections = BTreeSet::from([String::from("custom-account")]);

    migrate_custom_provider_inference_route(
        &mut route,
        "custom:multi-route",
        &owned_connections,
        &target,
    )
    .expect("multi-protocol route migration should remain structurally valid");

    assert_eq!(route.service_entry_id, "custom-entry-v2");
    assert_eq!(
        route.binding,
        InferenceRouteBinding::Protocol {
            protocol_profile_id: String::from(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,),
        },
        "the provider default must not replace an exact model route"
    );
}

/// Verifies a multi-protocol custom provider projects one verified route per protocol.
/// 验证多协议自定义供应商为每个协议投影一条已验证路由。
#[test]
fn multi_protocol_custom_provider_projects_one_route_per_protocol() {
    let request = PublicAiCustomProviderSaveRequest {
        provider_id: None,
        display_name: String::from("Multi Protocol"),
        base_url: String::from("https://multi.example.test/v1"),
        remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
        websocket_policy: ResponsesWebsocketPolicy::Disabled,
        authentication: CustomProviderAuthentication::BearerAuthorization,
        credential_display_name: Some(String::from("Account")),
        secret: Some(String::from("secret")),
        validation_connection_id: None,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocols: vec![
            CustomProviderProtocol::OpenAiResponses,
            CustomProviderProtocol::AnthropicMessages,
            CustomProviderProtocol::OpenAiChat,
        ],
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
        allow_manual_fallback: false,
        allow_pending_validation: false,
    };
    let candidate =
        prepare_custom_provider_candidate(&request, &PersistedAiConfiguration::default())
            .expect("multi-protocol candidate should compile");
    let entry = &candidate.descriptor.service_entries[0];
    assert_eq!(entry.protocol_routes.len(), 3);
    assert_eq!(
        candidate.activation_target.protocol_profile_ids.len(),
        3,
        "every enabled protocol must stay verified for the connection"
    );
    assert_eq!(
        candidate.activation_target.default_protocol_profile_id,
        entry.protocol_routes[0].protocol_profile_id
    );
}

/// Verifies the connection contract comparison normalizes every route, not only the first.
/// 验证连接契约比较会归一化每一条路由，而不只是第一条。
#[test]
fn connection_contract_comparison_normalizes_every_route() {
    let mut current = PersistedAiConfiguration::default();
    let existing_descriptor = CustomProviderDefinition {
        id: String::from("custom:multi"),
        display_name: String::from("Multi Protocol"),
        base_url: String::from("https://multi.example.test/v1"),
        protocols: vec![
            CustomProviderProtocol::OpenAiResponses,
            CustomProviderProtocol::OpenAiChat,
        ],
        authentication: CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("existing descriptor should compile");
    current
        .custom_providers
        .insert(String::from("custom:multi"), existing_descriptor.clone());
    // candidate keeps every credential-facing route identical but carries an explicit
    // execution-only Codex-style Responses feature on the second route.
    // candidate 保持全部面向凭据的路由一致，但第二条路由携带显式且只影响执行的
    // Responses 能力开关。
    let mut candidate_descriptor = existing_descriptor;
    if let Some(route) = candidate_descriptor.service_entries[0]
        .protocol_routes
        .iter_mut()
        .find(|route| route.protocol_profile_id == "openai-chat-completions.v1")
    {
        route.responses_features = Some(vulcan_provider_adapters::ResponsesRouteFeatures {
            remote_compaction:
                vulcan_provider_adapters::ResponsesRemoteCompactionPolicy::EnabledWithLocalFallback,
            websocket: vulcan_provider_adapters::ResponsesWebsocketPolicy::Disabled,
            recovery: Default::default(),
        });
    }
    let candidate = PreparedCustomProviderCandidate {
        provider_id: String::from("custom:multi"),
        updating_existing: true,
        descriptor: candidate_descriptor,
        validation_connection: None,
        temporary_secret: None,
        activation_target: CustomProviderActivationTarget {
            service_entry_id: String::from("default"),
            credential_profile_id: String::from("api-key"),
            default_protocol_profile_id: String::from("openai-responses.v1"),
            protocol_profile_ids: BTreeSet::from([
                String::from("openai-responses.v1"),
                String::from("openai-chat-completions.v1"),
            ]),
        },
    };
    let changed = custom_provider_connection_contract_changed(&current, &candidate)
        .expect("contract comparison should resolve");
    assert!(
        !changed,
        "execution-only Responses features must not count as a credential contract change on any route"
    );
}

/// Verifies bounded non-sensitive fixed headers survive prepare and registry revalidation.
/// 验证有界非敏感固定请求头可通过保存与注册表重验。
#[test]
fn bounded_fixed_headers_survive_prepare_and_registry_revalidation() {
    let request = PublicAiCustomProviderSaveRequest {
        provider_id: None,
        display_name: String::from("Header Provider"),
        base_url: String::from("https://headers.example.test/v1"),
        remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
        websocket_policy: ResponsesWebsocketPolicy::Disabled,
        authentication: CustomProviderAuthentication::BearerAuthorization,
        credential_display_name: Some(String::from("Account")),
        secret: Some(String::from("secret")),
        validation_connection_id: None,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocols: vec![CustomProviderProtocol::OpenAiChat],
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::from([
            (String::from("accept"), String::from("application/json")),
            (String::from("x-provider-version"), String::from("2026-08")),
        ]),
        session_header: None,
        allow_manual_fallback: false,
        allow_pending_validation: false,
    };
    let candidate =
        prepare_custom_provider_candidate(&request, &PersistedAiConfiguration::default())
            .expect("bounded fixed headers should survive prepare");
    // Runtime revalidation must accept the same headers after a reload.
    // 运行时重验必须在重载后仍接受相同的请求头。
    ProviderRegistry::effective(&BTreeMap::from([(
        candidate.descriptor.id.clone(),
        candidate.descriptor,
    )]))
    .expect("registry revalidation must accept bounded fixed headers");
}

/// Verifies secret-header injection is rejected at the Service save boundary.
/// 验证服务保存边界拒绝 Secret 请求头注入。
#[test]
fn secret_fixed_headers_are_rejected_at_service_save_boundary() {
    for forbidden in ["authorization", "x-api-key", "cookie", "host"] {
        let request = PublicAiCustomProviderSaveRequest {
            provider_id: None,
            display_name: String::from("Unsafe Header"),
            base_url: String::from("https://unsafe.example.test/v1"),
            remote_compaction_policy: ResponsesRemoteCompactionPolicy::Disabled,
            websocket_policy: ResponsesWebsocketPolicy::Disabled,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            credential_display_name: Some(String::from("Account")),
            secret: Some(String::from("secret")),
            validation_connection_id: None,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
            model_discovery_protocol:
                vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
            model_discovery_path: None,
            protocols: vec![CustomProviderProtocol::OpenAiChat],
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::from([(String::from(forbidden), String::from("private"))]),
            session_header: None,
            allow_manual_fallback: false,
            allow_pending_validation: false,
        };
        let error =
            prepare_custom_provider_candidate(&request, &PersistedAiConfiguration::default())
                .map(|_| ())
                .expect_err("secret fixed headers must fail at the save boundary");
        assert!(
            error.contains("request headers"),
            "unexpected rejection message: {error}"
        );
    }
}
