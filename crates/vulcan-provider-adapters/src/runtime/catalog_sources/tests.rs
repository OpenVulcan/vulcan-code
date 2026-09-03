//! Source-policy route aggregation and credential-free non-remote refresh regressions.
//! 来源策略路由聚合与无需凭据的非远端刷新回归测试。

use super::super::PublicAiRuntime;
use super::super::catalog_policy::MANUAL_MODEL_CATALOG_SCHEMA_REVISION;
use super::*;
use vulcan_ai_contract::{AiOperation, ModelCapabilities};

/// Verifies a later remote route keeps discovery enabled when the first route is manual.
/// 验证首条路由为手工发现时，后续远端路由仍会保持发现能力。
#[test]
fn mixed_custom_protocol_discovery_aggregates_every_verified_route() {
    // provider declares Responses as manual first and Chat as a route-specific remote source.
    // provider 首先声明手工 Responses，并把 Chat 声明为路由专属远端来源。
    let provider = crate::CustomProviderDefinition {
        id: String::from("custom:mixed-discovery"),
        display_name: String::from("Mixed Discovery"),
        base_url: String::from("https://mixed-discovery.example.test/v1"),
        protocols: vec![
            crate::CustomProviderProtocol::OpenAiResponses,
            crate::CustomProviderProtocol::OpenAiChat,
        ],
        authentication: crate::CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: crate::CustomProviderModelDiscoveryMode::Manual,
        model_discovery_path: None,
        protocol_routes: vec![crate::CustomProtocolRoute {
            protocol: crate::CustomProviderProtocol::OpenAiChat,
            base_url: None,
            endpoint_path: None,
            authentication: None,
            model_discovery_mode: Some(crate::CustomProviderModelDiscoveryMode::Standard),
            model_discovery_path: None,
            request_headers: BTreeMap::new(),
        }],
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("mixed custom provider should project");
    // entry retains Manual only as a compatibility projection of the first route.
    // entry 仅把 Manual 保留为首条路由的兼容投影。
    let entry = provider
        .service_entries
        .first()
        .expect("custom provider should expose one entry");
    assert_eq!(entry.model_discovery, ModelDiscoveryKind::Manual);
    // configuration supplies the exact descriptor to the production source-policy resolver.
    // configuration 向生产来源策略解析器提供精确描述符。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_providers
        .insert(provider.id.clone(), provider.clone());

    assert_eq!(
        resolved_model_source_policy(
            &configuration,
            provider.id.as_str(),
            entry.id.as_str(),
            None,
        )
        .expect("mixed route source policy should resolve"),
        ModelSourcePolicy::DiscoveryAndManual,
    );
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
        prompt_cache_by_protocol: BTreeMap::new(),
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
        prompt_cache_by_protocol: BTreeMap::new(),
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
