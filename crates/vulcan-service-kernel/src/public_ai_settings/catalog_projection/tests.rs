//! Directory aggregation and blocked model-route diagnostics regressions.
//! 目录聚合与受阻模型路由诊断回归。

use super::*;
use vulcan_ai_contract::ModelCapabilities;
use vulcan_provider_adapters::{
    CredentialMode, CustomModelProtocolPreference, CustomProviderAuthentication,
    CustomProviderDefinition, CustomProviderModelDiscoveryMode, CustomProviderProtocol,
    PublicAiRuntime,
};

/// Verifies blocked custom models remain visible with an exact backend-owned reason.
/// 验证被阻止的自定义模型仍保持可见，并携带精确的后端原因。
#[test]
fn custom_model_route_status_exposes_model_specific_protocol_block() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let provider_id = String::from("custom:model-route-status");
    let descriptor = CustomProviderDefinition {
        id: provider_id.clone(),
        display_name: String::from("Model Route Status"),
        base_url: String::from("https://model-route-status.example.test/v1"),
        authentication: CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
        model_discovery_path: None,
        protocols: vec![
            CustomProviderProtocol::OpenAiResponses,
            CustomProviderProtocol::AnthropicMessages,
        ],
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("multi-protocol custom provider should compile");
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_providers
        .insert(provider_id.clone(), descriptor);
    let connection = CredentialConnection {
        id: String::from("model-route-status-account"),
        provider_id: provider_id.clone(),
        service_entry_id: String::from("default"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("model-route-status-credential"),
        display_name: String::from("Model Route Status Account"),
        secret_reference: String::from("model-route-status-secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    let providers = runtime
        .effective_providers(&configuration)
        .expect("effective custom providers should resolve");
    configuration
        .connection_graph
        .add_connection(&providers, connection.clone())
        .expect("custom connection should register");
    let model_id = String::from("anthropic-only/model");
    configuration.custom_model_protocol_preferences.insert(
        PersistedAiConfiguration::provider_model_key(provider_id.as_str(), model_id.as_str()),
        CustomModelProtocolPreference::Explicit {
            protocol: CustomProviderProtocol::OpenAiResponses,
        },
    );
    let model = vulcan_provider_adapters::ConnectionModel {
        id: model_id.clone(),
        display_name: String::from("Anthropic-only Model"),
        capabilities: ModelCapabilities::default(),
        capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
        protocol_variants: BTreeMap::from([(
            String::from(vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID),
            vulcan_provider_adapters::ProtocolModelVariant {
                capabilities: ModelCapabilities::default(),
                capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
                discovery_signature: String::from("anthropic-only-route-status"),
                catalog_generation: 1,
            },
        )]),
        resolved_protocol_binding: None,
    };
    let catalogs = BTreeMap::from([(
        connection.id.clone(),
        ConnectionModelCatalog {
            connection_id: connection.id,
            models: vec![model],
            generation: 1,
            page_count: 1,
            discovery_schema_revision: String::from("custom-route-status-v1"),
        },
    )]);

    let records = build_public_ai_model_route_records(
        &configuration,
        &providers,
        &catalogs,
        &BTreeMap::from([(provider_id, 0)]),
    );
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert!(!record.executable);
    assert!(record.protocol_id.is_none());
    assert!(record.binding_source.is_none());
    assert_eq!(
        record.available_protocols,
        vec![String::from(
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
        )]
    );
    assert!(
        record
            .unavailable_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("unavailable for this model"))
    );
}

/// Verifies a system connection derives its directory kind from Manifest routes alone.
/// 验证系统连接只从 Manifest 路由派生目录类型。
#[test]
fn system_connection_directory_uses_manifest_routes() {
    // providers supplies one exact built-in route for the isolation boundary.
    // providers 提供一条精确内置路由，用于验证隔离边界。
    let providers = ProviderRegistry::first_batch().expect("built-in provider registry");
    // provider is the exact MiniMax descriptor used by the fixture connection.
    // provider 是测试连接使用的精确 MiniMax 描述。
    let provider = providers.provider("minimax").expect("MiniMax provider");
    // entry supplies the exact service and credential identifiers without guessing names.
    // entry 提供精确服务与凭据标识，不猜测名称。
    let entry = provider
        .service_entries
        .first()
        .expect("MiniMax service entry");
    // profile supplies the exact credential profile registered under the service entry.
    // profile 提供服务入口下注册的精确凭据配置档。
    let profile = entry
        .credential_profiles
        .first()
        .expect("MiniMax credential profile");
    // connection contains no protocol state by design.
    // connection 按设计不包含协议状态。
    let connection = CredentialConnection {
        id: String::from("legacy-incomplete"),
        provider_id: provider.id.clone(),
        service_entry_id: entry.id.clone(),
        credential_profile_id: profile.id.clone(),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("legacy-credential"),
        display_name: String::from("Legacy account"),
        secret_reference: String::from("legacy-secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };

    assert_eq!(
        connection_model_directory_kind(&providers, &connection),
        PublicAiModelDirectoryKind::Dynamic
    );
}

/// Verifies account synchronization remains available when any protocol route is remote.
/// 验证任一协议路由为远端发现时账号同步仍保持可用。
#[test]
fn multi_protocol_directory_kind_aggregates_route_discovery() {
    // provider declares a manual first route and a remote second route.
    // provider 声明一条手工首路由与一条远端次路由。
    let provider = vulcan_provider_adapters::CustomProviderDefinition {
        id: String::from("custom:mixed-directory"),
        display_name: String::from("Mixed Directory"),
        base_url: String::from("https://mixed-directory.example.test/v1"),
        protocols: vec![
            CustomProviderProtocol::OpenAiResponses,
            CustomProviderProtocol::OpenAiChat,
        ],
        authentication: CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
        model_discovery_path: None,
        protocol_routes: vec![vulcan_provider_adapters::CustomProtocolRoute {
            protocol: CustomProviderProtocol::OpenAiChat,
            base_url: None,
            endpoint_path: None,
            authentication: None,
            model_discovery_mode: Some(CustomProviderModelDiscoveryMode::Standard),
            model_discovery_path: None,
            request_headers: BTreeMap::new(),
        }],
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("mixed directory provider should project");
    // entry supplies the exact generated service and credential identifiers.
    // entry 提供精确生成的服务与凭据标识。
    let entry = provider
        .service_entries
        .first()
        .expect("custom provider should expose one entry");
    let profile = entry
        .credential_profiles
        .first()
        .expect("custom provider should expose one credential profile");
    // connection carries credential identity only; the entry retains both manual and remote routes.
    // connection 只携带凭据身份；入口同时保留手工与远端路由。
    let connection = CredentialConnection {
        id: String::from("mixed-directory-account"),
        provider_id: provider.id.clone(),
        service_entry_id: entry.id.clone(),
        credential_profile_id: profile.id.clone(),
        credential_mode: profile.credential_mode,
        credential_id: String::from("mixed-directory-credential"),
        display_name: String::from("Mixed directory account"),
        secret_reference: String::from("mixed-directory-secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    // providers is the exact effective registry consumed by the browser projection.
    // providers 是浏览器投影消费的精确有效注册表。
    let providers = ProviderRegistry::effective(&BTreeMap::from([(provider.id.clone(), provider)]))
        .expect("mixed directory registry should build");

    assert_eq!(
        connection_model_directory_kind(&providers, &connection),
        PublicAiModelDirectoryKind::Dynamic,
    );
}
