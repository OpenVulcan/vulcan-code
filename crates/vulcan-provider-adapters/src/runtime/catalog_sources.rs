//! Exact model-source authority and signed identity/variant catalog construction.
//! 精确模型来源权威与签名身份／变体目录构建。

use crate::{
    ConnectionModel, ConnectionModelCatalog, CredentialConnection, ModelCapabilityEvidence,
    ModelDiscoveryKind, ModelSourcePolicy, PersistedAiConfiguration, ProtocolModelVariant,
    ResolvedProviderManifest,
};
use std::collections::BTreeMap;

/// Resolves the exact model-source policy for one active provider service entry.
/// 解析一个生效供应商服务入口的精确模型来源策略。
///
/// # Parameters
/// 参数
/// `configuration` owns custom descriptors, `provider_id` and `service_entry_id` select the
/// exact entry, and `manifest` is the optional compiled authority for that provider.
/// `configuration` 持有自定义描述；`provider_id` 与 `service_entry_id` 选择精确入口；
/// `manifest` 是该供应商可选的编译权威。
///
/// # Returns
/// 返回值
/// Returns the explicit Manifest policy or the closed custom-provider policy derived from its
/// declared discovery mode. A missing non-custom authority fails closed.
/// 返回显式 Manifest 策略，或从自定义供应商已声明发现模式派生的封闭策略；
/// 缺失的非自定义权威一律失败关闭。
pub(super) fn resolved_model_source_policy(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    manifest: Option<&ResolvedProviderManifest>,
) -> Result<ModelSourcePolicy, String> {
    if let Some(manifest) = manifest {
        return manifest.model_source_policy(service_entry_id);
    }
    // entry owns the single catalog source independently from its conversation routes.
    // entry 独立于会话路由持有唯一目录来源。
    let entry = resolve_custom_model_service_entry(configuration, provider_id, service_entry_id)?;
    match entry.model_discovery {
        ModelDiscoveryKind::Manual => Ok(ModelSourcePolicy::ManualOnly),
        ModelDiscoveryKind::OpenAi
        | ModelDiscoveryKind::AnthropicCompatible
        | ModelDiscoveryKind::Gemini => Ok(ModelSourcePolicy::DiscoveryAndManual),
        _ => Err(format!(
            "Custom provider `{provider_id}` service entry `{service_entry_id}` uses an unsupported catalog source."
        )),
    }
}

/// Resolves the exact custom-provider conversation entry used by model catalog projection.
/// 解析模型目录投影使用的精确自定义供应商会话入口。
///
/// # Parameters
/// 参数
/// `configuration` owns custom descriptors, `provider_id` selects one provider, and
/// `service_entry_id` is exact for credential-scoped calls or empty only for provider-level
/// validation without a credential.
/// `configuration` 持有自定义描述；`provider_id` 选择供应商；`service_entry_id` 在凭据作用域
/// 调用中必须精确，仅无凭据的供应商级校验可传空值。
///
/// # Returns
/// 返回值
/// The exact entry, or an explicit ambiguity/missing-authority error.
/// 返回精确入口，或明确的歧义/缺少权威错误。
pub(super) fn resolve_custom_model_service_entry<'a>(
    configuration: &'a PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
) -> Result<&'a crate::ServiceEntry, String> {
    let provider = configuration.custom_providers.get(provider_id).ok_or_else(|| {
        format!(
            "Provider `{provider_id}` has no active Manifest or custom descriptor for model-source resolution."
        )
    })?;
    if !service_entry_id.is_empty() {
        return provider
            .service_entries
            .iter()
            .find(|entry| entry.id == service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Custom provider `{provider_id}` has no service entry `{service_entry_id}` for model-source resolution."
                )
            });
    }
    // Provider-level validation is credential-free only when exactly one conversation entry owns
    // the model namespace; selecting an arbitrary entry would make capability validation diverge
    // from request execution.
    // 仅当恰好一个会话入口拥有模型命名空间时，供应商级校验才可不依赖凭据；任意选择入口会使
    // 能力校验与请求执行发生偏离。
    let mut conversation_entries = provider
        .service_entries
        .iter()
        .filter(|entry| entry.kind == crate::ServiceEntryKind::Conversation);
    let entry = conversation_entries.next().ok_or_else(|| {
        format!(
            "Custom provider `{provider_id}` has no conversation service entry for provider-level model-source validation."
        )
    })?;
    if conversation_entries.next().is_some() {
        return Err(format!(
            "Custom provider `{provider_id}` has multiple conversation service entries; provider-level model-source validation requires an exact credential scope."
        ));
    }
    Ok(entry)
}

/// Reports whether one discovery implementation performs remote provider I/O.
/// 报告一种发现实现是否执行远端供应商 I/O。
///
/// # Parameters
/// 参数
/// `kind` is the exact compiled discovery implementation.
/// `kind` 是精确编译后的发现实现。
///
/// # Returns
/// 返回值
/// `true` only for network-backed discovery kinds.
/// 仅对网络驱动的发现类别返回 `true`。
pub(super) const fn model_discovery_is_remote(kind: ModelDiscoveryKind) -> bool {
    // Keep this match exhaustive so each new discovery kind must declare whether it sends remote requests.
    // 保持穷尽匹配，确保每种新增发现类型都必须明确声明是否发送远端请求。
    match kind {
        ModelDiscoveryKind::Manual
        | ModelDiscoveryKind::ProviderToolsOnly
        | ModelDiscoveryKind::ManifestStatic => false,
        ModelDiscoveryKind::OpenVulcan
        | ModelDiscoveryKind::OpenRouter
        | ModelDiscoveryKind::OpenAi
        | ModelDiscoveryKind::OpenAiCodex
        | ModelDiscoveryKind::Anthropic
        | ModelDiscoveryKind::AnthropicCompatible
        | ModelDiscoveryKind::Gemini
        | ModelDiscoveryKind::GoogleCodeAssist => true,
    }
}

/// Builds an empty credential-scoped source catalog with one exact source marker.
/// 使用一个精确来源标记构建空的凭据作用域源目录。
///
/// # Parameters
/// 参数
/// `connection_id`, `generation`, and `schema_revision` are the exact runtime identities.
/// `connection_id`、`generation` 与 `schema_revision` 是精确运行时身份。
///
/// # Returns
/// 返回值
/// A bounded empty catalog ready for deterministic overlay reconstruction.
/// 返回可用于确定性覆盖重建的有界空目录。
pub(super) fn empty_model_source_catalog(
    connection_id: &str,
    generation: u64,
    schema_revision: &str,
) -> ConnectionModelCatalog {
    ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models: Vec::new(),
        generation,
        page_count: 0,
        discovery_schema_revision: schema_revision.to_string(),
    }
}

/// Projects signed Manifest protocol variants onto one exact model identity and credential scope.
/// 把签名 Manifest 协议变体投影到一个精确模型身份与凭据作用域。
///
/// # Parameters
/// 参数
/// `manifest` owns routing/security facts; `connection` selects the exact entry/profile;
/// `generation` freezes catalog provenance; `model` supplies discovery or manual evidence;
/// `preserve_display_name` retains a user-owned manual display label after signed capability projection.
/// `manifest` 持有路由/安全事实；`connection` 选择精确入口/配置档；`generation` 冻结目录来源；
/// `model` 提供发现或手工证据；`preserve_display_name` 在签名能力投影后保留用户持有的手工显示名。
///
/// # Returns
/// 返回值
/// The number of verified supported variants projected; zero means the identity has no executable
/// signed binding under `declared_rule_required`.
/// 返回已投影的已验证受支持变体数量；零表示该身份在 `declared_rule_required` 下没有可执行
/// 签名绑定。
pub(super) fn project_manifest_protocol_variants_for_connection(
    manifest: &ResolvedProviderManifest,
    connection: &CredentialConnection,
    generation: u64,
    model: &mut ConnectionModel,
    preserve_display_name: bool,
) -> Result<usize, String> {
    let entry = manifest
        .descriptor
        .service_entries
        .iter()
        .find(|entry| entry.id == connection.service_entry_id)
        .ok_or_else(|| {
            format!(
                "Provider `{}` has no service entry `{}` for Manifest protocol projection.",
                manifest.descriptor.id, connection.service_entry_id
            )
        })?;
    let credential_profile = entry
        .credential_profile(connection.credential_profile_id.as_str())
        .ok_or_else(|| {
            format!(
                "Credential profile `{}` is not registered for service entry `{}`.",
                connection.credential_profile_id, entry.id
            )
        })?;
    // entryDefaults includes the exact service-level protocol override compiled with this manifest.
    // entryDefaults 包含与该 Manifest 共同编译的精确服务入口级协议覆盖。
    let entry_defaults = manifest.model_defaults_for_entry(entry.id.as_str())?;
    let rule = manifest.rule_for_model(
        entry.id.as_str(),
        Some(connection.credential_profile_id.as_str()),
        model.id.as_str(),
    );
    if rule.is_none()
        && entry_defaults.unknown_model_policy
            == crate::provider_manifest::UnknownModelPolicy::DeclaredRuleRequired
    {
        model.protocol_variants.clear();
        return Ok(0);
    }
    let supported_protocols = rule
        .map(|rule| rule.supported_protocols.as_slice())
        .unwrap_or(entry_defaults.supported_protocols.as_slice());
    let original_display_name = model.display_name.clone();
    let mut variants = BTreeMap::new();
    let mut compatibility_projection = None;
    for route in entry.protocol_routes.iter().filter(|route| route.verified) {
        if !supported_protocols
            .iter()
            .any(|protocol| protocol == &route.protocol_profile_id)
        {
            continue;
        }
        // protocolModel is isolated so protocol-scoped capability fields cannot cross routes.
        // protocolModel 相互隔离，使协议作用域能力字段不能跨路由泄漏。
        let mut protocol_model = model.clone();
        protocol_model.protocol_variants.clear();
        if rule.is_some() {
            if !crate::discovery::apply_manifest_model_rule(
                manifest,
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                route.protocol_profile_id.as_str(),
                &mut protocol_model,
            ) {
                return Err(format!(
                    "Manifest model `{}` lost its compiled rule in service entry `{}`.",
                    model.id, entry.id
                ));
            }
        } else if let Some(default_capabilities) = &entry_defaults.capabilities {
            // Unknown-model default binding is an explicit signed authority; its complete compiled
            // capability snapshot wins over untrusted manual evidence.
            // 未知模型默认绑定是显式签名权威；其完整编译能力快照高于不可信手工证据。
            protocol_model.capabilities = default_capabilities.clone();
        }
        if preserve_display_name {
            protocol_model.display_name = original_display_name.clone();
        }
        let authentication =
            route.resolved_authentication(entry.authentication, credential_profile.authentication);
        let discovery_url = route.model_discovery_url(entry, "");
        let discovery_signature = crate::discovery_signature::discovery_signature(
            ModelDiscoveryKind::ManifestStatic,
            discovery_url.as_str(),
            connection.credential_profile_id.as_str(),
            &authentication,
            &route.request_headers,
        );
        variants.insert(
            route.protocol_profile_id.clone(),
            ProtocolModelVariant {
                capabilities: protocol_model.capabilities.clone(),
                capability_evidence: protocol_model.capability_evidence.clone(),
                discovery_signature,
                catalog_generation: generation,
            },
        );
        compatibility_projection.get_or_insert(protocol_model);
    }
    if let Some(projected) = compatibility_projection {
        model.display_name = projected.display_name;
        model.capabilities = projected.capabilities;
        model.capability_evidence = projected.capability_evidence;
    }
    model.protocol_variants = variants;
    Ok(model.protocol_variants.len())
}

/// Builds the complete Manifest-owned identity catalog for one exact credential connection.
/// 为一个精确凭据连接构建完整的 Manifest 持有身份目录。
///
/// # Parameters
/// 参数
/// `manifest` is the immutable identity and capability authority; `connection` supplies the exact
/// entry/profile scope; `generation` is the process-local catalog generation.
/// `manifest` 是不可变身份与能力权威；`connection` 提供精确入口/配置档作用域；
/// `generation` 是进程内目录代次。
///
/// # Returns
/// 返回值
/// A static catalog with one independent protocol variant per verified supported route.
/// 返回静态目录，其中每条已验证且受支持的路由都有独立协议变体。
pub(super) fn build_manifest_identity_catalog(
    manifest: &ResolvedProviderManifest,
    connection: &CredentialConnection,
    generation: u64,
) -> Result<ConnectionModelCatalog, String> {
    let entry = manifest
        .descriptor
        .service_entries
        .iter()
        .find(|entry| entry.id == connection.service_entry_id)
        .ok_or_else(|| {
            format!(
                "Provider `{}` has no service entry `{}` for Manifest identity projection.",
                manifest.descriptor.id, connection.service_entry_id
            )
        })?;
    // declaredModels is an explicit empty inventory when the policy includes no static identities.
    // declaredModels 在策略不含任何静态身份时是显式空清单。
    let declared_models = manifest
        .catalog_models
        .get(entry.id.as_str())
        .map(Vec::as_slice)
        .unwrap_or_default();
    let entry_defaults = manifest.model_defaults_for_entry(entry.id.as_str())?;
    // models is keyed by exact upstream identity to guarantee deterministic merging.
    // models 按精确上游身份索引，以保证确定性合并。
    let mut models = BTreeMap::<String, ConnectionModel>::new();
    for declared in declared_models.iter().filter(|declared| {
        declared.credential_profile_ids.is_empty()
            || declared
                .credential_profile_ids
                .iter()
                .any(|profile_id| profile_id == &connection.credential_profile_id)
    }) {
        // model starts from the compiled default and each route receives its own cloned projection.
        // model 从编译默认值开始，每条路由接收自己的克隆投影。
        let mut model = ConnectionModel {
            id: declared.model_id.clone(),
            display_name: declared.display_name.clone(),
            capabilities: entry_defaults.capabilities.clone().unwrap_or_default(),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        if project_manifest_protocol_variants_for_connection(
            manifest, connection, generation, &mut model, false,
        )? == 0
        {
            return Err(format!(
                "Manifest model `{}` has no verified supported protocol variant in service entry `{}`.",
                declared.model_id, entry.id
            ));
        }
        models.insert(model.id.clone(), model);
    }
    // hasModels preserves the credential-scoped page-count fact before consuming the map.
    // hasModels 在消费映射前保留凭据作用域分页计数事实。
    let has_models = !models.is_empty();
    Ok(ConnectionModelCatalog {
        connection_id: connection.id.clone(),
        models: models.into_values().collect(),
        generation,
        page_count: u32::from(has_models),
        discovery_schema_revision: String::from(
            crate::client::MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION,
        ),
    })
}

/// Merges Manifest-owned identities into a source-derived catalog without replacing discovery evidence.
/// 将 Manifest 持有身份合并进来源目录，且不替换发现证据。
///
/// # Parameters
/// 参数
/// `catalog` owns discovery/manual evidence and `manifest_catalog` owns signed identity variants.
/// `catalog` 持有发现/手工证据；`manifest_catalog` 持有签名身份变体。
pub(super) fn merge_manifest_identity_catalog(
    catalog: &mut ConnectionModelCatalog,
    manifest_catalog: ConnectionModelCatalog,
) {
    // models preserves source-derived top-level evidence and fills only absent signed variants.
    // models 保留来源派生的顶层证据，仅补入缺失的签名变体。
    let mut models = catalog
        .models
        .drain(..)
        .map(|model| (model.id.clone(), model))
        .collect::<BTreeMap<_, _>>();
    for manifest_model in manifest_catalog.models {
        match models.get_mut(manifest_model.id.as_str()) {
            Some(source_model) => {
                for (protocol_id, variant) in manifest_model.protocol_variants {
                    source_model
                        .protocol_variants
                        .entry(protocol_id)
                        .or_insert(variant);
                }
            }
            None => {
                models.insert(manifest_model.id.clone(), manifest_model);
            }
        }
    }
    catalog.models = models.into_values().collect();
}

#[cfg(test)]
mod tests;
