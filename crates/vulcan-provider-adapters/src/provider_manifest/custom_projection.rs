//! Current custom-provider descriptor to Manifest projector.
//! 当前自定义供应商描述符到 Manifest 的投影器。
//!
//! This module accepts only the current user-owned descriptor schema. Bundled system manifests
//! are never generated from Rust descriptors and remain the single authority.
//! 本模块只接受当前用户自有描述符 Schema。内置系统 Manifest 绝不从 Rust 描述符生成，并始终
//! 保持唯一权威。

use super::{
    CatalogMembershipPolicy, EngineContract, ManifestCapabilityPatch, ManifestCategory,
    ManifestCredentialProfile, ManifestDirectoryVisibility, ManifestDisplay, ManifestEntryKind,
    ManifestModelCatalog, ManifestProtocolRoute, ManifestProvenance, ManifestServiceEntry,
    ModelDefaults, ModelDefaultsOverlay, ModelSourcePolicy, ProvenanceStatus, ProviderManifestV1,
    UnknownModelPolicy, UsageProfileDeclaration,
};
use crate::{
    CredentialDirectoryVisibility, ProviderCategory, ProviderDescriptor, ProviderKind, ServiceEntry,
};

/// Projects one current custom-provider descriptor into its Manifest document.
/// 把一条当前自定义供应商描述符投影为 Manifest 文档。
///
/// # Parameters
/// 参数
/// `provider` is the exact current custom descriptor and must use the reserved custom prefix.
/// `provider` 是精确的当前自定义描述符，且必须使用保留的自定义前缀。
///
/// # Returns
/// 返回值
/// A current data-owned manifest shell, or an explicit ownership error for non-custom input.
/// 返回当前数据持有的 Manifest 外壳；非自定义输入返回显式所有权错误。
pub fn project_custom_descriptor_manifest(
    provider: &ProviderDescriptor,
) -> Result<ProviderManifestV1, String> {
    if provider.credential_directory_visibility == CredentialDirectoryVisibility::CoreAccount {
        return Err(String::from(
            "Core accounts can only be declared by system provider manifests.",
        ));
    }
    let has_custom_prefix = provider.id.starts_with("custom:");
    if provider.kind != ProviderKind::Custom {
        return Err(format!(
            "Custom descriptor projection accepts only custom providers: `{}`.",
            provider.id
        ));
    }
    if !has_custom_prefix {
        return Err(format!(
            "Custom provider descriptor must use the custom identifier prefix: `{}`.",
            provider.id
        ));
    }

    let mut services = provider
        .service_entries
        .iter()
        .map(convert_service_entry)
        .collect::<Vec<_>>();
    let usage_profiles = materialize_standard_usage_profiles(&mut services);
    // catalogMembershipPolicy preserves the current descriptor's discovery authority. Custom
    // descriptors carry no exact model declarations, so DeclaredOnly would erase every
    // successfully discovered model.
    // catalogMembershipPolicy 保留当前描述符的发现权威。自定义描述符不携带精确模型声明，
    // 因此 DeclaredOnly 会删除全部已成功发现模型。
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
    // defaultCapabilities is the conservative snapshot required by current custom conversation
    // bindings; discovery may narrow explicit limits and modalities later.
    // defaultCapabilities 是当前自定义会话绑定所需的保守快照；发现结果随后可收紧明确限制与模态。
    let default_capabilities = (!supported_protocols.is_empty()).then(|| ManifestCapabilityPatch {
        operations: Some(vec![vulcan_ai_contract::AiOperation::ConversationRespond]),
        evidence_revision: Some(String::from("custom-descriptor.default-capabilities.v1")),
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
                CredentialDirectoryVisibility::CoreAccount => {
                    ManifestDirectoryVisibility::CoreAccount
                }
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
            directory_rank: provider.directory_rank,
        },
        credential_registration: provider.credential_registration,
        services,
        model_defaults: ModelDefaults {
            // Current custom descriptors have no model-rule layer. Conversation entries therefore
            // retain their declared default protocol, while provider-tools-only records expose no
            // model binding.
            // 当前自定义描述符没有模型规则层。因此会话入口保留其声明的默认协议，纯 Provider Tool
            // 记录则不暴露任何模型绑定。
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
            tool_schema_projection: super::ToolSchemaProjectionMode::Lightweight,
            tool_mode: super::ToolMode::Direct,
            default_usage_profile_refs: Vec::new(),
            default_request_profile_refs: Vec::new(),
            default_response_profile_refs: Vec::new(),
            default_operation_refs: Vec::new(),
            provenance_ref: None,
        },
        model_rules: Vec::new(),
        usage_profiles,
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

/// Compares the descriptor-owned custom-provider facts while excluding Manifest-owned profiles.
/// 比较描述符持有的自定义供应商事实，同时排除 Manifest 持有的 Profile。
///
/// # Parameters
/// 参数
/// `manifest_descriptor` is the descriptor compiled from the complete standalone Manifest.
/// `manifest_descriptor` 是从完整独立 Manifest 编译出的描述符。
/// `activated_descriptor` is the UI/configuration projection activated for the same provider.
/// `activated_descriptor` 是为同一供应商激活的 UI/配置投影。
///
/// # Returns
/// 返回值
/// `true` only when every descriptor-owned fact matches. Usage, request, and response Profile
/// payloads are excluded because their stable references and payloads are owned exclusively by
/// the standalone Manifest and intentionally do not round-trip through the descriptor editor.
/// 仅当全部描述符持有事实一致时返回 `true`。用量、请求与响应 Profile 载荷被排除，因为其
/// 稳定引用和载荷仅由独立 Manifest 持有，且刻意不经过描述符编辑器往返。
pub fn custom_manifest_matches_activated_descriptor(
    manifest_descriptor: &ProviderDescriptor,
    activated_descriptor: &ProviderDescriptor,
) -> bool {
    let mut manifest_projection = manifest_descriptor.clone();
    let mut activated_projection = activated_descriptor.clone();
    clear_manifest_owned_protocol_profiles(&mut manifest_projection);
    clear_manifest_owned_protocol_profiles(&mut activated_projection);
    manifest_projection == activated_projection
}

/// Clears protocol payloads whose authority belongs to the standalone custom Manifest.
/// 清除权威归属于独立自定义 Manifest 的协议载荷。
///
/// # Parameters
/// 参数
/// `descriptor` is a temporary comparison projection and is never published.
/// `descriptor` 是临时比较投影，绝不会被发布。
///
/// # Returns
/// 返回值
/// This function mutates the temporary projection and returns no value.
/// 此函数修改临时投影且不返回值。
fn clear_manifest_owned_protocol_profiles(descriptor: &mut ProviderDescriptor) {
    // Service tags are derived from the manifest's entries and tool families at compile time.
    // 服务标记在编译时由 Manifest 的入口与工具族推导。
    descriptor.service_tags.clear();
    for service_entry in &mut descriptor.service_entries {
        for route in &mut service_entry.protocol_routes {
            route.usage_extraction_profiles.clear();
            route.request_projection_profiles.clear();
            route.response_projection_profiles.clear();
        }
    }
}

/// Materializes protocol-specific standard Usage profiles into one standalone custom manifest.
/// 将协议专属标准用量 Profile 物化到一份独立自定义 Manifest 中。
///
/// `services` contains the exact custom service routes and is updated with explicit profile refs.
/// `services` 包含精确自定义服务路由，并会被写入显式 Profile 引用。
/// Returns de-duplicated named payload declarations owned by the same provider document.
/// 返回由同一供应商文档持有的去重具名载荷声明。
fn materialize_standard_usage_profiles(
    services: &mut [ManifestServiceEntry],
) -> Vec<UsageProfileDeclaration> {
    let mut declarations = Vec::<UsageProfileDeclaration>::new();
    for service in services {
        for route in &mut service.protocol_routes {
            let Some(protocol) = crate::execution::upstream_protocol_for_profile_id(
                route.protocol_profile_id.as_str(),
            ) else {
                continue;
            };
            let profiles = vulcan_llm_core::standard_usage_profiles(protocol);
            route.usage_profile_refs = profiles
                .iter()
                .map(|profile| profile.profile_id.clone())
                .collect();
            for payload in profiles {
                if declarations
                    .iter()
                    .any(|declaration| declaration.profile_id == payload.profile_id)
                {
                    continue;
                }
                declarations.push(UsageProfileDeclaration {
                    profile_id: payload.profile_id.clone(),
                    description: format!(
                        "Materialized standard Usage compatibility for protocol `{}`.",
                        route.protocol_profile_id
                    ),
                    payload: Some(payload),
                });
            }
        }
    }
    declarations
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
        session_header: entry.session_header.clone(),
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
                    | crate::ModelDiscoveryKind::OpenVulcan
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
        authorization: profile.authorization.clone(),
        billing_kind: profile.billing_kind,
        account_usage_query: profile.account_usage_query,
        account_model_access: profile.account_model_access.clone(),
        authentication: profile.authentication,
        key_prefix: profile.key_prefix.clone(),
        credential_role: profile.credential_role.clone(),
        default_usage_scopes: profile.default_usage_scopes.clone(),
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
        // The compiled route carries the already frozen contract back into the authoring shape.
        // 已编译路由把此前冻结的契约带回作者形态。
        reasoning_replay_contract: route.reasoning_replay_contract,
    }
}
