//! Browser snapshot redaction, isolation, and editable catalog projection regressions.
//! 浏览器快照脱敏、隔离与可编辑目录投影回归。

use super::*;

/// The real settings projection reveals a core system provider only while its global account exists.
/// 真实设置投影仅在全局账户存在时显示核心系统供应商。
#[test]
fn openvulcan_core_provider_follows_global_account_visibility() {
    // runtime uses bundled declarations and cannot contact a live provider.
    // runtime 使用内置声明，不能访问真实供应商。
    let runtime = PublicAiRuntime::unavailable().expect("offline runtime");
    let mut configuration = PersistedAiConfiguration::default();
    let providers = runtime.effective_providers(&configuration).unwrap();
    let provider = providers
        .providers()
        .into_iter()
        .find(|provider| {
            provider.credential_directory_visibility
                == vulcan_provider_adapters::CredentialDirectoryVisibility::CoreAccount
        })
        .expect("core account manifest must be registered");
    let provider_id = provider.id.clone();
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| {
            entry.credential_profiles.iter().any(|profile| {
                matches!(
                    profile.authorization,
                    Some(
                        vulcan_provider_adapters::CredentialAuthorization::RegisteredOauth {
                            provider: vulcan_provider_adapters::OAuthProviderKind::OpenVulcanChina
                        }
                    )
                )
            })
        })
        .expect("China account entry");
    let profile = entry
        .credential_profiles
        .iter()
        .find(|profile| profile.credential_mode == CredentialMode::Auth)
        .expect("renewable profile");
    let initial = build_public_ai_settings_snapshot(&runtime, &configuration).unwrap();
    assert!(
        !initial
            .providers
            .iter()
            .any(|provider| provider.id == provider_id)
    );
    let connection = CredentialConnection {
        id: String::from("core-fixture"),
        provider_id: provider_id.clone(),
        service_entry_id: entry.id.clone(),
        credential_profile_id: profile.id.clone(),
        credential_mode: CredentialMode::Auth,
        credential_id: String::from("fixture-credential"),
        display_name: String::from("Fixture"),
        secret_reference: String::from("fixture-secret"),
        management_secret_reference: None,
        service_origin_override: None,
    };
    configuration
        .connection_graph
        .add_connection(&providers, connection.clone())
        .unwrap();
    // Activation mirrors the real account commit before the runtime publishes a coherent catalog generation.
    // 激活步骤模拟真实账户提交，然后运行时发布一致的目录代际。
    runtime
        .replace_configuration(configuration.clone())
        .unwrap();
    let connected = build_public_ai_settings_snapshot(&runtime, &configuration).unwrap();
    assert!(
        connected
            .providers
            .iter()
            .any(|provider| provider.id == provider_id
                && provider.kind == vulcan_provider_adapters::ProviderKind::System)
    );
    super::super::connections::remove_connection_from_configuration(
        &mut configuration,
        &connection.id,
        None,
    )
    .unwrap();
    // Logout publishes the cleared graph before any subsequent directory observation.
    // 退出在后续目录观测前发布已清空的连接图。
    runtime
        .replace_configuration(configuration.clone())
        .unwrap();
    let disconnected = build_public_ai_settings_snapshot(&runtime, &configuration).unwrap();
    assert!(
        !disconnected
            .providers
            .iter()
            .any(|provider| provider.id == provider_id)
    );
    assert!(
        !disconnected
            .connections
            .iter()
            .any(|item| item.id == connection.id)
    );
}

/// Verifies the real browser snapshot retains all media fields and does not persist defaults.
/// 验证真实浏览器快照保留全部媒体字段，且不持久化默认值。
#[test]
fn settings_snapshot_projects_exact_media_delivery_policy() {
    // This runtime uses bundled declarations only and cannot send paid requests.
    // 此运行时仅使用内置声明，不能发送付费请求。
    let runtime =
        PublicAiRuntime::unavailable().expect("media policy fixture or operation must be valid");
    // The configuration initially has no explicit media policy.
    // 配置最初没有显式媒体策略。
    let mut configuration = PersistedAiConfiguration::default();
    // The actual full settings builder must publish the same nested wire shape as the client types.
    // 实际完整设置构建器必须发布与客户端类型一致的嵌套线协议结构。
    let initial = serde_json::to_value(
        build_public_ai_settings_snapshot(&runtime, &configuration)
            .expect("media policy fixture or operation must be valid"),
    )
    .expect("media policy fixture or operation must be valid");
    assert_eq!(
        initial["mediaDelivery"]["policy"]["conversation"]["image"]["without_upload_max_bytes"],
        524_288
    );
    assert!(configuration.media_delivery_policy.is_none());
    // Zero remains an explicit saved value instead of being treated as a missing field.
    // 零保持为显式保存值，不被当成缺失字段。
    let mut policy = configuration.effective_media_delivery_policy();
    policy.conversation.image.without_upload_max_bytes = 0;
    configuration
        .set_media_delivery_policy(policy)
        .expect("media policy fixture or operation must be valid");
    runtime
        .replace_configuration(configuration.clone())
        .expect("media policy fixture or operation must be valid");
    // The published snapshot is built from the exact updated owner.
    // 已发布快照从精确更新后的所有者构建。
    let updated = serde_json::to_value(
        build_public_ai_settings_snapshot(&runtime, &configuration)
            .expect("media policy fixture or operation must be valid"),
    )
    .expect("media policy fixture or operation must be valid");
    assert_eq!(
        updated["mediaDelivery"]["policy"],
        serde_json::to_value(policy).expect("media policy fixture or operation must be valid")
    );
    assert_eq!(
        updated["mediaDelivery"]["revision"],
        policy
            .fingerprint()
            .expect("media policy fixture or operation must be valid")
    );
    assert_ne!(
        initial["mediaDelivery"]["revision"],
        updated["mediaDelivery"]["revision"]
    );
}

/// Verifies browser settings retain diagnostics without projecting an isolated provider account.
/// 验证浏览器设置会保留诊断，同时不投影已隔离供应商账号。
#[test]
fn settings_snapshot_excludes_isolated_custom_provider_connection() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let provider_id = "custom:isolated-settings";
    let connection_id = "isolated-settings-account";
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_provider_file_refs
        .insert(provider_id.to_string());
    configuration.connection_graph.connections.insert(
        connection_id.to_string(),
        CredentialConnection {
            id: connection_id.to_string(),
            provider_id: provider_id.to_string(),
            service_entry_id: String::from("default"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("isolated-settings-credential"),
            display_name: String::from("Isolated Settings Account"),
            secret_reference: String::from("isolated-settings-secret"),
            management_secret_reference: None,
            service_origin_override: None,
        },
    );
    configuration.custom_provider_manifest_failures = vec![CustomProviderManifestFailure {
        provider_id: provider_id.to_string(),
        message: String::from("invalid standalone document"),
    }];
    configuration.custom_provider_manifest_failure_total_count = 1;

    let snapshot = build_public_ai_settings_snapshot(&runtime, &configuration)
        .expect("healthy settings snapshot should remain available");

    assert!(snapshot.connections.is_empty());
    assert!(snapshot.catalogs.is_empty());
    assert_eq!(snapshot.custom_provider_manifest_failure_total_count, 1);
    assert_eq!(
        snapshot.custom_provider_manifest_failures[0].provider_id,
        provider_id
    );
    assert!(
        configuration
            .connection_graph
            .connections
            .contains_key(connection_id),
        "the persisted repair authority must remain unchanged"
    );
}

/// Verifies persisted direct settings remain visible without an account catalog.
/// 验证没有账号目录时持久化直接设置仍保持可见。
#[test]
fn settings_snapshot_projects_capability_overrides_without_catalogs() {
    // runtime supplies the built-in provider registry without requiring network state.
    // runtime 提供内置供应商注册表且不需要网络状态。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // configuration represents a credential-free custom provider with one saved model difference.
    // configuration 表示一个无凭据自定义供应商及一项已保存模型差异。
    let mut configuration = PersistedAiConfiguration::default();
    configuration.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
        ModelCapabilityOverrides {
            context_limit: Some(16_384),
            output_limit: Some(4_096),
            ..ModelCapabilityOverrides::default()
        },
    );
    runtime
        .replace_configuration(configuration.clone())
        .expect("capability overrides should cross the settings activation boundary");

    // snapshot must expose the record directly instead of depending on a visible connection model.
    // snapshot 必须直接公开该记录，而不是依赖可见的连接模型。
    let snapshot = build_public_ai_settings_snapshot(&runtime, &configuration)
        .expect("settings snapshot should build");

    assert_eq!(snapshot.model_capability_override_records.len(), 1);
    let record = &snapshot.model_capability_override_records[0];
    assert_eq!(record.provider_id, "custom:test");
    assert_eq!(record.model_id, "private/chat");
    assert_eq!(record.overrides.context_limit, Some(16_384));
    assert_eq!(record.overrides.output_limit, Some(4_096));
}

/// Verifies provider-tool template models remain extension-manageable without becoming conversation models.
/// 验证供应商工具模板模型保持可由扩展管理且不会成为会话模型。
#[test]
fn settings_snapshot_separates_provider_tool_templates_from_conversation_access() {
    // runtime supplies the bundled Alibaba template catalog without network access.
    // runtime 在不访问网络的情况下提供内置阿里云模板目录。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // configuration owns one eligible Model Studio credential and one explicit model closure.
    // configuration 持有一个合格百炼凭据与一个显式模型关闭选择。
    let mut configuration = PersistedAiConfiguration::default();
    // connection binds the exact service and credential profile that own the reviewed templates.
    // connection 绑定拥有已审核模板的精确服务与凭据配置档。
    let connection = CredentialConnection {
        id: String::from("alibaba-template-account"),
        provider_id: String::from("alibaba"),
        service_entry_id: String::from("model-studio-cn"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("alibaba-template-credential"),
        display_name: String::from("Alibaba template account"),
        secret_reference: String::from("alibaba-template-secret"),
        management_secret_reference: None,
        service_origin_override: None,
    };
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            connection,
        )
        .expect("Alibaba template connection should register");
    configuration.set_model_enabled("alibaba", "qwen-image-2.0-pro", false);
    runtime
        .replace_configuration(configuration.clone())
        .expect("template access should cross the settings activation boundary");

    // snapshot must project declarations and selectable extension targets from one generation.
    // snapshot 必须从同一代际投影声明与可选择的扩展目标。
    let snapshot = build_public_ai_settings_snapshot(&runtime, &configuration)
        .expect("settings snapshot should build");
    assert!(
        snapshot.provider_tool_template_models.iter().any(|model| {
            model.provider_id == "alibaba" && model.model_id == "qwen-image-2.0-pro"
        })
    );
    assert!(!snapshot.model_access_records.iter().any(|record| {
        record.provider_id == "alibaba" && record.model_id == "qwen-image-2.0-pro"
    }));
    assert!(snapshot.model_route_records.iter().all(|record| {
        record.provider_id != "alibaba" || record.model_id != "qwen-image-2.0-pro"
    }));
    assert!(
        snapshot
            .provider_tool_targets
            .iter()
            .any(|available| { available.target.upstream_target_id == "qwen-image-2.0-pro" })
    );
}

/// Verifies raw discovery catalogs reach the browser only for custom-provider connections.
/// 验证原始发现目录只对自定义供应商连接进入浏览器快照。
#[test]
fn settings_snapshot_limits_source_catalogs_to_custom_providers() {
    // runtime uses bundled declarations and cannot contact a live provider.
    // runtime 使用内置声明，不能访问真实供应商。
    let runtime = PublicAiRuntime::unavailable().expect("offline runtime");
    let custom_provider_id = String::from("custom:source-catalog-scope");
    let descriptor = vulcan_provider_adapters::CustomProviderDefinition {
        id: custom_provider_id.clone(),
        display_name: String::from("Source Catalog Scope"),
        base_url: String::from("https://source-catalog-scope.example.test/v1"),
        authentication: vulcan_provider_adapters::CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: vulcan_provider_adapters::CustomProviderModelDiscoveryMode::Standard,
        model_discovery_protocol: vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocols: vec![vulcan_provider_adapters::CustomProviderProtocol::OpenAiResponses],
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("custom provider should compile");
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_providers
        .insert(custom_provider_id.clone(), descriptor);
    let providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    // connection builds one account per provider family so only the provider kind differs.
    // connection 为每类供应商构建一个账号，使两者只在供应商类型上不同。
    let connection = |id: &str, provider_id: &str, service_entry_id: &str| CredentialConnection {
        id: id.to_string(),
        provider_id: provider_id.to_string(),
        service_entry_id: service_entry_id.to_string(),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: format!("{id}-credential"),
        display_name: id.to_string(),
        secret_reference: format!("{id}-secret"),
        management_secret_reference: None,
        service_origin_override: None,
    };
    for account in [
        connection("custom-source-account", &custom_provider_id, "default"),
        connection("system-source-account", "openrouter", "public-api"),
    ] {
        configuration
            .connection_graph
            .add_connection(&providers, account)
            .expect("connection should register");
    }
    for connection_id in ["custom-source-account", "system-source-account"] {
        runtime
            .stage_source_catalog_for_testing(ConnectionModelCatalog {
                connection_id: connection_id.to_string(),
                models: Vec::new(),
                generation: 1,
                page_count: 1,
                discovery_schema_revision: String::from("source-catalog-scope-v1"),
            })
            .expect("source catalog should stage");
    }
    runtime
        .replace_configuration(configuration.clone())
        .expect("configuration should activate");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("overlays should activate");

    let snapshot = build_public_ai_settings_snapshot(&runtime, &configuration)
        .expect("settings snapshot should build");

    assert!(
        snapshot
            .source_catalogs
            .contains_key("custom-source-account")
    );
    assert!(
        !snapshot
            .source_catalogs
            .contains_key("system-source-account")
    );
}
