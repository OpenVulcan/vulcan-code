//! Shared catalog source policy and credential-free non-remote refresh regressions.
//! 共享目录来源策略与无需凭据的非远端刷新回归测试。

use super::super::PublicAiRuntime;
use super::super::catalog_policy::MANUAL_MODEL_CATALOG_SCHEMA_REVISION;
use super::*;
use vulcan_ai_contract::{AiOperation, ModelCapabilities};

/// Verifies only the shared custom catalog determines whether discovery is enabled.
/// 验证仅共享自定义目录决定是否启用模型发现。
#[test]
fn custom_catalog_source_ignores_conversation_route_discovery() {
    // provider declares one shared manual catalog and multiple independently configured conversation routes.
    // provider 声明唯一共享手工目录及多条独立配置的会话路由。
    let mut provider = crate::CustomProviderDefinition {
        id: String::from("custom:mixed-discovery"),
        display_name: String::from("Mixed Discovery"),
        base_url: String::from("https://mixed-discovery.example.test/v1"),
        protocols: vec![
            crate::CustomProviderProtocol::OpenAiResponses,
            crate::CustomProviderProtocol::AnthropicMessages,
        ],
        authentication: crate::CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: crate::CustomProviderModelDiscoveryMode::Manual,
        model_discovery_protocol: crate::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("custom provider should project");
    // entry retains an existing route-level declaration to verify that it cannot override catalog ownership.
    // entry 保留既有路由级声明，以验证它不能覆盖目录归属。
    let entry = provider.service_entries.first_mut().expect("custom entry");
    for route in &mut entry.protocol_routes {
        route.model_discovery = Some(ModelDiscoveryKind::AnthropicCompatible);
    }
    // configuration exercises the resolver both with and without a compiled manifest.
    // configuration 同时覆盖有无已编译清单的解析路径。
    let mut configuration = PersistedAiConfiguration::default();
    for (kind, expected) in [
        (ModelDiscoveryKind::Manual, ModelSourcePolicy::ManualOnly),
        (
            ModelDiscoveryKind::OpenAi,
            ModelSourcePolicy::DiscoveryAndManual,
        ),
    ] {
        provider
            .service_entries
            .first_mut()
            .expect("custom entry")
            .model_discovery = kind;
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider.clone());
        assert_eq!(
            resolved_model_source_policy(&configuration, &provider.id, "default", None).unwrap(),
            expected
        );
        // manifest verifies that cold compilation agrees with the descriptor's shared catalog.
        // manifest 验证冷编译与描述符共享目录保持一致。
        let manifest =
            crate::project_custom_descriptor_manifest(&provider).expect("project manifest");
        let resolved = crate::compile_manifest(&manifest, crate::ManifestOwnership::Custom)
            .expect("compile manifest");
        assert_eq!(
            resolved_model_source_policy(&configuration, &provider.id, "default", Some(&resolved))
                .unwrap(),
            expected
        );
    }
}

/// Verifies Manifest-only refresh succeeds without reading a secret or contacting metadata.
/// 验证 Manifest-only 刷新无需读取秘密或访问元数据即可成功。
#[tokio::test]
async fn manifest_only_refresh_performs_no_credential_or_remote_discovery() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let generation = runtime
        .selection_catalog_snapshot()
        .expect("manifest generation must be available");
    let manifest = generation
        .manifests
        .manifest("alibaba")
        .expect("Alibaba Manifest must be bundled");
    let entry = manifest
        .descriptor
        .service_entries
        .iter()
        .find(|entry| {
            entry.kind == crate::ServiceEntryKind::Conversation
                && manifest
                    .model_source_policy(entry.id.as_str())
                    .is_ok_and(|policy| policy == ModelSourcePolicy::ManifestOnly)
        })
        .expect("Alibaba must expose a Manifest-only conversation entry");
    let profile = entry
        .credential_profiles
        .first()
        .expect("Manifest-only entry must declare a credential profile");
    let _route = entry
        .protocol_routes
        .iter()
        .find(|route| route.verified)
        .expect("Manifest-only entry must declare a verified route");
    let connection = CredentialConnection {
        id: String::from("connection-manifest-only"),
        provider_id: String::from("alibaba"),
        service_entry_id: entry.id.clone(),
        credential_profile_id: profile.id.clone(),
        credential_mode: profile.credential_mode,
        credential_id: String::from("credential-manifest-only"),
        display_name: String::from("Manifest Only"),
        secret_reference: String::from("unavailable-secret-must-not-be-read"),
        management_secret_reference: None,
        service_origin_override: None,
    };
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .connections
        .insert(connection.id.clone(), connection.clone());

    let catalog = runtime
        .refresh_connection_catalog(&configuration, connection.id.as_str())
        .await
        .expect("Manifest-only refresh must not require the unavailable secret store");
    assert!(!catalog.models.is_empty());
    assert_eq!(
        catalog.discovery_schema_revision,
        crate::client::MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION
    );
}

/// Verifies custom manual-only refresh succeeds without a `/models` request or secret read.
/// 验证自定义 manual-only 刷新无需 `/models` 请求或秘密读取即可成功。
#[tokio::test]
async fn custom_manual_only_refresh_performs_no_credential_or_remote_discovery() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let provider = crate::CustomProviderDefinition {
        id: String::from("custom:manual-only-probe"),
        display_name: String::from("Manual Only Probe"),
        base_url: String::from("https://manual-only.example.test/v1"),
        protocols: vec![crate::CustomProviderProtocol::OpenAiChat],
        authentication: crate::CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: crate::CustomProviderModelDiscoveryMode::Manual,
        model_discovery_protocol: crate::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("manual-only custom provider must project");
    let entry = provider
        .service_entries
        .first()
        .expect("custom provider must expose one entry");
    let profile = entry
        .credential_profiles
        .first()
        .expect("custom provider must expose one profile");
    let _route = entry
        .protocol_routes
        .first()
        .expect("custom provider must expose one route");
    let connection = CredentialConnection {
        id: String::from("connection-custom-manual-only"),
        provider_id: provider.id.clone(),
        service_entry_id: entry.id.clone(),
        credential_profile_id: profile.id.clone(),
        credential_mode: profile.credential_mode,
        credential_id: String::from("credential-custom-manual-only"),
        display_name: String::from("Custom Manual Only"),
        secret_reference: String::from("unavailable-secret-must-not-be-read"),
        management_secret_reference: None,
        service_origin_override: None,
    };
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_providers
        .insert(provider.id.clone(), provider.clone());
    configuration
        .connection_graph
        .connections
        .insert(connection.id.clone(), connection.clone());
    configuration.custom_models.insert(
        provider.id.clone(),
        BTreeMap::from([(
            String::from("manual-model"),
            crate::CustomModelDefinition {
                id: String::from("manual-model"),
                display_name: String::from("Manual Model"),
                enabled: true,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    ..ModelCapabilities::default()
                },
                request_projection: Default::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                system_binding: None,
            },
        )]),
    );

    let catalog = runtime
        .refresh_connection_catalog(&configuration, connection.id.as_str())
        .await
        .expect("manual-only refresh must not require the unavailable secret store");
    assert_eq!(
        catalog
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["manual-model"]
    );
    assert_eq!(
        catalog.discovery_schema_revision,
        MANUAL_MODEL_CATALOG_SCHEMA_REVISION
    );
}
