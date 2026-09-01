//! Browser snapshot redaction, isolation, and editable catalog projection regressions.
//! 浏览器快照脱敏、隔离与可编辑目录投影回归。

use super::*;

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
            prompt_cache_by_protocol: BTreeMap::new(),
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
        prompt_cache_by_protocol: BTreeMap::new(),
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
