//! Explicit descriptor-only custom-provider v0 compatibility converter.
//! 显式的仅描述符自定义供应商 v0 兼容转换器。
//!
//! This module accepts only the historical user-owned standalone descriptor schema. Bundled
//! system manifests are never generated from Rust descriptors and remain the single authority.
//! 本模块只接受历史用户自有独立描述符 Schema。内置系统 Manifest 绝不从 Rust 描述符生成，
//! 并始终保持唯一权威。

use super::{
    CatalogMembershipPolicy, EngineContract, ManifestCapabilityPatch, ManifestCategory,
    ManifestCredentialProfile, ManifestDirectoryVisibility, ManifestDisplay, ManifestEntryKind,
    ManifestModelCatalog, ManifestProtocolRoute, ManifestProvenance, ManifestServiceEntry,
    ModelDefaults, ModelDefaultsOverlay, ModelSourcePolicy, ProvenanceStatus, ProviderManifestV1,
    UnknownModelPolicy,
};
use crate::{
    CredentialDirectoryVisibility, ProviderCategory, ProviderDescriptor, ProviderKind, ServiceEntry,
};

/// Converts one descriptor-only custom-provider v0 record into a current manifest document.
/// 把一条仅描述符自定义供应商 v0 记录转换为当前 Manifest 文档。
///
/// # Parameters
/// 参数
/// `provider` is the exact historical custom descriptor and must use the reserved custom prefix.
/// `provider` 是精确的历史自定义描述符，且必须使用保留的自定义前缀。
///
/// # Returns
/// 返回值
/// A current data-owned manifest shell, or an explicit ownership error for non-custom input.
/// 返回当前数据持有的 Manifest 外壳；非自定义输入返回显式所有权错误。
pub fn legacy_custom_descriptor_to_manifest(
    provider: &ProviderDescriptor,
) -> Result<ProviderManifestV1, String> {
    let has_custom_prefix = provider.id.starts_with("custom:");
    if provider.kind != ProviderKind::Custom {
        return Err(format!(
            "Legacy descriptor conversion accepts only custom providers: `{}`.",
            provider.id
        ));
    }
    if !has_custom_prefix {
        return Err(format!(
            "Custom provider descriptor must use the custom identifier prefix: `{}`.",
            provider.id
        ));
    }

    let services = provider
        .service_entries
        .iter()
        .map(convert_service_entry)
        .collect::<Vec<_>>();
    // catalogMembershipPolicy preserves descriptor-v0 discovery authority. Those records carried
    // no exact model declarations, so DeclaredOnly would erase every successfully discovered model
    // immediately after standalone-manifest migration.
    // catalogMembershipPolicy 保留描述符 v0 的发现权威。旧记录没有精确模型声明，因此若使用
    // DeclaredOnly，会在迁移为独立 Manifest 后立即删除全部已成功发现模型。
    let catalog_membership_policy = if services
        .iter()
        .filter_map(|entry| entry.model_catalog.as_ref())
        .any(|catalog| catalog.source_policy.includes_discovery())
    {
        CatalogMembershipPolicy::DiscoveryEvidence
    } else {
        CatalogMembershipPolicy::DeclaredOnly
    };

    let mut supported_protocols: Vec<String> = Vec::new();
    let mut selected_protocol: Option<String> = None;
    for entry in &services {
        for route in &entry.protocol_routes {
            let profile = route.protocol_profile_id.clone();
            if !supported_protocols.contains(&profile) {
                supported_protocols.push(profile.clone());
            }
            if selected_protocol.is_none() && route.verified {
                selected_protocol = Some(profile);
            }
        }
    }
    if selected_protocol.is_none() {
        selected_protocol = supported_protocols.first().cloned();
    }
    // defaultCapabilities is the conservative signed snapshot required by migrated custom
    // conversation bindings; discovery may narrow explicit limits and modalities later.
    // defaultCapabilities 是迁移后自定义会话绑定所需的保守签名快照；发现结果随后可收紧
    // 明确限制与模态。
    let default_capabilities = (!supported_protocols.is_empty()).then(|| ManifestCapabilityPatch {
        operations: Some(vec![vulcan_ai_contract::AiOperation::ConversationRespond]),
        evidence_revision: Some(String::from("custom-legacy.default-capabilities.v1")),
        ..ManifestCapabilityPatch::default()
    });

    let revision = 1;
    Ok(ProviderManifestV1 {
        schema_version: super::MANIFEST_SCHEMA_VERSION.to_string(),
        provider_id: provider.id.clone(),
        revision,
        published_at: String::from("2026-08-06T00:00:00Z"),
        engine_contract: EngineContract {
            minimum_manifest_engine: super::CURRENT_MANIFEST_ENGINE,
            required_protocol_profiles: supported_protocols.clone(),
            required_execution_primitives: Vec::new(),
            required_upload_primitives: Vec::new(),
            required_transformers: Vec::new(),
        },
        display: ManifestDisplay {
            name: provider.display_name.clone(),
            category: match provider.category {
                ProviderCategory::Unclassified => ManifestCategory::Unclassified,
                ProviderCategory::Official => ManifestCategory::Official,
                ProviderCategory::CloudMaaS => ManifestCategory::CloudMaaS,
                ProviderCategory::Relay => ManifestCategory::Relay,
            },
            credential_directory_visibility: match provider.credential_directory_visibility {
                CredentialDirectoryVisibility::ConfiguredOnly => {
                    ManifestDirectoryVisibility::ConfiguredOnly
                }
                CredentialDirectoryVisibility::PinnedOnboarding => {
                    ManifestDirectoryVisibility::PinnedOnboarding
                }
            },
            promotion: provider
                .promotion
                .as_ref()
                .map(|promotion| super::ManifestPromotion {
                    signup_url: promotion.signup_url.clone(),
                    referral_url: promotion.referral_url.clone(),
                    campaign_id: promotion.campaign_id.clone(),
                    partnership_kind: promotion.partnership_kind,
                    disclosure_key: promotion.disclosure_key,
                }),
        },
        credential_registration: provider.credential_registration,
        services,
        model_defaults: ModelDefaults {
            // Descriptor-only v0 custom providers had no model-rule layer. Conversation entries
            // therefore retain their declared default protocol, while provider-tools-only records
            // expose no model binding.
            // 仅描述符 v0 自定义供应商没有模型规则层。因此会话入口保留其声明的默认协议，
            // 纯 Provider Tool 记录则不暴露任何模型绑定。
            default_enabled: true,
            catalog_membership_policy,
            unknown_model_policy: if supported_protocols.is_empty() {
                UnknownModelPolicy::DeclaredRuleRequired
            } else {
                UnknownModelPolicy::DefaultBindingAllowed
            },
            supported_protocols,
            selected_protocol,
            capabilities: default_capabilities,
            default_usage_profile_refs: Vec::new(),
            default_request_profile_refs: Vec::new(),
            default_response_profile_refs: Vec::new(),
            default_operation_refs: Vec::new(),
            provenance_ref: None,
        },
        model_rules: Vec::new(),
        usage_profiles: Vec::new(),
        request_profiles: Vec::new(),
        response_profiles: Vec::new(),
        upload_channels: Vec::new(),
        operations: Vec::new(),
        tool_targets: Vec::new(),
        tool_target_templates: Vec::new(),
        tool_target_template_bindings: Vec::new(),
        provenance: ManifestProvenance {
            status: ProvenanceStatus::Migrated,
            sources: Vec::new(),
            verified_at: None,
        },
    })
}

/// Converts one service entry.
/// 转换一个服务入口。
fn convert_service_entry(entry: &ServiceEntry) -> ManifestServiceEntry {
    // supportedProtocols and selectedProtocol retain the exact entry-local route authority.
    // supportedProtocols 与 selectedProtocol 保留精确入口局部路由权威。
    let supported_protocols = entry
        .protocol_routes
        .iter()
        .map(|route| route.protocol_profile_id.clone())
        .collect::<Vec<_>>();
    let selected_protocol = entry
        .protocol_routes
        .iter()
        .find(|route| route.verified)
        .or_else(|| entry.protocol_routes.first())
        .map(|route| route.protocol_profile_id.clone());
    ManifestServiceEntry {
        service_entry_id: entry.id.clone(),
        display_name: entry.display_name.clone(),
        kind: match entry.kind {
            crate::ServiceEntryKind::Conversation => ManifestEntryKind::Conversation,
            crate::ServiceEntryKind::ProviderTools => ManifestEntryKind::ProviderTools,
        },
        base_url: entry.base_url.clone(),
        credential_profiles: entry
            .credential_profiles
            .iter()
            .map(convert_credential_profile)
            .collect(),
        default_authentication: Some(entry.authentication),
        request_headers: entry.request_headers.clone(),
        service_origin_override_policy: entry.service_origin_override_policy.clone(),
        native_inference_bindings: entry.native_inference_bindings.clone(),
        request_projector: Some(entry.request_projector),
        model_catalog: match entry.kind {
            crate::ServiceEntryKind::Conversation => Some(ManifestModelCatalog {
                source_policy: match entry.model_discovery {
                    crate::ModelDiscoveryKind::Manual => ModelSourcePolicy::ManualOnly,
                    crate::ModelDiscoveryKind::ManifestStatic => ModelSourcePolicy::ManifestOnly,
                    crate::ModelDiscoveryKind::ProviderToolsOnly => ModelSourcePolicy::ManualOnly,
                    crate::ModelDiscoveryKind::OpenRouter
                    | crate::ModelDiscoveryKind::OpenAi
                    | crate::ModelDiscoveryKind::OpenAiCodex
                    | crate::ModelDiscoveryKind::Anthropic
                    | crate::ModelDiscoveryKind::AnthropicCompatible
                    | crate::ModelDiscoveryKind::Gemini
                    | crate::ModelDiscoveryKind::GoogleCodeAssist => {
                        ModelSourcePolicy::DiscoveryAndManual
                    }
                },
                discovery_kind: Some(entry.model_discovery),
                path: None,
                models: None,
            }),
            crate::ServiceEntryKind::ProviderTools => None,
        },
        protocol_routes: entry
            .protocol_routes
            .iter()
            .map(convert_protocol_route)
            .collect(),
        model_defaults_override: (entry.kind == crate::ServiceEntryKind::Conversation).then_some(
            ModelDefaultsOverlay {
                supported_protocols: Some(supported_protocols),
                selected_protocol,
            },
        ),
    }
}

/// Converts one credential profile.
/// 转换一个凭据配置档。
fn convert_credential_profile(profile: &crate::CredentialProfile) -> ManifestCredentialProfile {
    ManifestCredentialProfile {
        credential_profile_id: profile.id.clone(),
        display_name: profile.display_name.clone(),
        credential_mode: profile.credential_mode,
        oauth_provider: profile.oauth_provider,
        billing_kind: profile.billing_kind,
        account_usage_query: profile.account_usage_query,
        authentication: profile.authentication,
        key_prefix: profile.key_prefix.clone(),
        credential_role: profile.credential_role.clone(),
        allowed_usage_scopes: profile.allowed_usage_scopes.clone(),
    }
}

/// Converts one protocol route.
/// 转换一条协议路由。
fn convert_protocol_route(route: &crate::ProtocolRoute) -> ManifestProtocolRoute {
    ManifestProtocolRoute {
        protocol_profile_id: route.protocol_profile_id.clone(),
        endpoint_path: route.endpoint_path.clone(),
        base_url_override: route.base_url_override.clone(),
        authentication: route.authentication,
        request_headers: route.request_headers.clone(),
        model_discovery: route.model_discovery,
        model_discovery_path: route.model_discovery_path.clone(),
        model_discovery_base_url_override: route.model_discovery_base_url_override.clone(),
        responses_features: route.responses_features,
        input_resource_constraints: route.input_resource_constraints.clone(),
        usage_profile_refs: Vec::new(),
        request_profile_refs: Vec::new(),
        response_profile_refs: Vec::new(),
        provider_request_id_headers: route.provider_request_id_headers.clone(),
        verified: route.verified,
    }
}
