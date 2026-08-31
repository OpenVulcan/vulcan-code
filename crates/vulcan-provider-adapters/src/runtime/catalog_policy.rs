//! Stateless model-catalog membership, capability, account-access, and binding projection.
//! 无状态的模型目录成员、能力、账号访问与绑定投影。

use super::catalog_sources::{
    build_manifest_identity_catalog, merge_manifest_identity_catalog,
    project_manifest_protocol_variants_for_connection, resolve_custom_model_service_entry,
};
use super::model_overrides::resolve_request_projection;
use crate::{
    ConnectionModel, ConnectionModelCatalog, CredentialConnection, ModelSourcePolicy,
    PersistedAiConfiguration, ProtocolModelVariant, ResolvedProviderManifest,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use vulcan_ai_contract::{AccountUsageSnapshot, AiOperation, ModelCapabilities};

mod account_access;
mod bindings;
mod capabilities;

use account_access::{account_usage_unlocks_catalog, model_rule_allows_account_access};
use bindings::{
    freeze_custom_provider_protocol_bindings, freeze_system_provider_protocol_bindings,
    frozen_binding_variant_is_missing,
};
use capabilities::{
    apply_custom_model_advanced_declaration, apply_custom_model_defaults,
    custom_provider_system_baseline_for_protocol, manual_model_capability_evidence,
};
pub use capabilities::{
    apply_host_default_reasoning_policy, model_supports_reasoning, model_supports_text_conversation,
};
pub(super) use capabilities::{
    validate_resolved_catalog_limits, validate_resolved_model_capability_limits,
};

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(super) use test_support::apply_model_catalog_overlays;

/// Stable schema marker for a source catalog that contains only user-managed manual identities.
/// 仅包含用户管理手工身份的源目录稳定 Schema 标记。
pub(super) const MANUAL_MODEL_CATALOG_SCHEMA_REVISION: &str = "manual.models.v1";

/// Builds an empty discovery baseline only when one provider owns manual model definitions.
/// 仅在某个供应商拥有手工模型定义时构建空的发现基线。
///
/// # Parameters
/// 参数
/// `configuration` owns custom definitions, `connection` identifies the target credential, and
/// `generation` is the process-local catalog generation.
/// `configuration` 持有自定义定义，`connection` 标识目标凭据，`generation` 为进程内目录代次。
///
/// # Returns
/// 返回值
/// A credential-scoped baseline when manual definitions or confirmed template snapshots exist;
/// otherwise `None`.
/// 存在手工定义或已确认模板快照时返回凭据作用域基线，否则返回 `None`。
pub(super) fn manual_custom_catalog(
    configuration: &PersistedAiConfiguration,
    connection: &CredentialConnection,
    generation: u64,
) -> Option<ConnectionModelCatalog> {
    manual_custom_catalog_for_provider(
        configuration,
        connection.provider_id.as_str(),
        connection.id.as_str(),
        generation,
    )
}

/// Builds a persisted-evidence manual baseline for an exact provider and catalog scope.
/// 为精确供应商与目录作用域构建基于持久化证据的手工基线。
///
/// # Parameters
/// 参数
/// `configuration` owns custom definitions, `provider_id` selects them, `connection_id` labels
/// the isolated catalog, and `generation` is process-local.
/// `configuration` 持有自定义定义，`provider_id` 选择这些定义，`connection_id` 标记隔离目录，
/// `generation` 仅存在于进程内。
///
/// # Returns
/// 返回值
/// A baseline containing confirmed custom-provider template identities when available, an empty
/// baseline for explicit manual definitions, or `None` without either source of persisted evidence.
/// 存在已确认自定义供应商模板身份时返回包含这些身份的基线；仅有显式手工定义时返回空基线；
/// 两类持久化证据均不存在时返回 `None`。
pub(super) fn manual_custom_catalog_for_provider(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    connection_id: &str,
    generation: u64,
) -> Option<ConnectionModelCatalog> {
    // ownsManualDefinitions records explicit user-authored model declarations that overlays add below.
    // ownsManualDefinitions 记录后续由覆盖层加入的显式用户模型声明。
    let owns_manual_definitions = configuration
        .custom_models
        .get(provider_id)
        .is_some_and(|definitions| !definitions.is_empty());
    // templateModels retains only user-confirmed immutable template snapshots for this exact custom provider.
    // templateModels 仅保留属于该精确自定义供应商、且由用户确认的不可变模板快照。
    let template_models = if provider_id.starts_with("custom:") {
        configuration
            .custom_model_template_decisions
            .iter()
            .filter_map(|(key, decision)| {
                let (decision_provider_id, model_id) =
                    PersistedAiConfiguration::provider_model_key_parts(key.as_str())?;
                if decision_provider_id != provider_id
                    || !matches!(decision, crate::CustomModelTemplateDecision::Applied { .. })
                {
                    return None;
                }
                Some(ConnectionModel {
                    id: model_id.to_string(),
                    display_name: model_id.to_string(),
                    capabilities: ModelCapabilities::default(),
                    capability_evidence: crate::ModelCapabilityEvidence::default(),
                    protocol_variants: BTreeMap::new(),
                    resolved_protocol_binding: None,
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if !owns_manual_definitions && template_models.is_empty() {
        return None;
    }
    Some(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models: template_models,
        generation,
        page_count: 0,
        discovery_schema_revision: String::from(MANUAL_MODEL_CATALOG_SCHEMA_REVISION),
    })
}

/// Resolves one source catalog through membership, capabilities, user state, and account access.
/// 通过成员关系、能力、用户状态与账号访问规则解析一个来源目录。
///
/// # Parameters
/// 参数
/// `configuration` owns user state; provider and service identifiers select the exact Manifest
/// scope; `account_usage` supplies credential evidence; `source_policy` and `manifest` freeze
/// system authority; `catalog` is replaced in place.
/// `configuration` 持有用户状态；供应商与服务标识选择精确 Manifest 作用域；`account_usage`
/// 提供凭据证据；`source_policy` 与 `manifest` 冻结系统权威；`catalog` 会被原位替换。
///
/// # Returns
/// 返回值
/// Success after the complete effective catalog is projected, or an exact validation failure.
/// 完整生效目录投影完成后返回成功，否则返回精确校验错误。
pub(super) fn apply_model_catalog_overlays_with_account_access(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    account_usage: Option<&AccountUsageSnapshot>,
    source_policy: ModelSourcePolicy,
    manifest: Option<&ResolvedProviderManifest>,
    catalog: &mut ConnectionModelCatalog,
) -> Result<(), String> {
    let custom_provider = provider_id.starts_with("custom:");
    // serviceEntryId resolves the credential-free provider-settings path once, then every source,
    // variant, binding, and projection layer shares the same exact entry.
    // serviceEntryId 仅解析一次无凭据供应商设置路径，随后来源、变体、绑定与投影层共享同一精确入口。
    let resolved_service_entry_id = if custom_provider {
        resolve_custom_model_service_entry(configuration, provider_id, service_entry_id)?
            .id
            .clone()
    } else {
        service_entry_id.to_string()
    };
    let service_entry_id = resolved_service_entry_id.as_str();
    let source_is_manifest = catalog.discovery_schema_revision
        == crate::client::MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION;
    let source_is_manual =
        catalog.discovery_schema_revision == MANUAL_MODEL_CATALOG_SCHEMA_REVISION;
    let source_is_discovery = !source_is_manifest && !source_is_manual;
    // sourceAllowed prevents a previously published catalog from leaking identities after a hot
    // Manifest changes this entry's source authority.
    // sourceAllowed 防止热更新 Manifest 改变入口来源权威后，先前发布的目录继续泄漏身份。
    let source_allowed = if source_is_manifest {
        source_policy.includes_manifest()
    } else if source_is_manual {
        source_policy.includes_manual()
    } else {
        source_policy.includes_discovery()
    };
    if !source_allowed {
        catalog.models.clear();
        catalog.page_count = 0;
    }
    if let Some(manifest) = manifest {
        // Membership authority is independent from source availability. `declared_only` retains
        // remote evidence only for identities covered by an exact catalog record or scoped rule;
        // manual user identities remain governed solely by the explicit source policy.
        // 成员权限独立于来源可用性。`declared_only` 仅保留被精确目录记录或作用域规则覆盖的
        // 远端证据；用户手工身份仍只由显式来源策略管理。
        if source_is_discovery
            && !manifest
                .catalog_membership_policy
                .admits_unmatched_discovery()
        {
            let connection = configuration
                .connection_graph
                .connections
                .get(catalog.connection_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Credential connection `{}` is not configured for catalog membership projection.",
                        catalog.connection_id
                    )
                })?;
            catalog.models.retain(|model| {
                manifest.declares_model_identity(
                    service_entry_id,
                    Some(connection.credential_profile_id.as_str()),
                    model.id.as_str(),
                )
            });
        } else if source_is_manifest
            && !manifest
                .catalog_membership_policy
                .includes_declared_identities()
        {
            catalog.models.clear();
            catalog.page_count = 0;
        }
    }
    if source_policy.includes_manifest() {
        let manifest = manifest.ok_or_else(|| {
            format!(
                "Provider `{provider_id}` source policy includes Manifest identity without an active Manifest."
            )
        })?;
        if manifest
            .catalog_membership_policy
            .includes_declared_identities()
        {
            let connection = configuration
                .connection_graph
                .connections
                .get(catalog.connection_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Credential connection `{}` is not configured for Manifest identity projection.",
                        catalog.connection_id
                    )
                })?;
            let manifest_catalog =
                build_manifest_identity_catalog(manifest, connection, catalog.generation)?;
            merge_manifest_identity_catalog(catalog, manifest_catalog);
        }
    }
    if custom_provider
        && let Some(provider) = configuration.custom_providers.get(provider_id)
        && let Some(entry) = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == service_entry_id)
    {
        // Only provider-neutral manual evidence, or a service entry with one uniquely provable
        // route, may synthesize missing variants. A multi-protocol remote catalog without its
        // route evidence must remain unbound rather than pretending every enabled route observed
        // the model.
        // 只有供应商无关的手工证据，或仅有一条可唯一证明路由的服务入口，才可合成缺失变体。
        // 缺少路由证据的多协议远端目录必须保持未绑定，不能伪装成每条启用路由都观测到该模型。
        let seed_routes = if source_is_manual || entry.protocol_routes.len() == 1 {
            entry.protocol_routes.as_slice()
        } else {
            &[]
        };
        for model in &mut catalog.models {
            if !model.protocol_variants.is_empty() {
                continue;
            }
            for route in seed_routes {
                let material = format!(
                    "custom-manual\0{provider_id}\0{service_entry_id}\0{}\0{}",
                    model.id, route.protocol_profile_id
                );
                model.protocol_variants.insert(
                    route.protocol_profile_id.clone(),
                    ProtocolModelVariant {
                        capabilities: model.capabilities.clone(),
                        capability_evidence: model.capability_evidence.clone(),
                        discovery_signature: format!(
                            "sha256:{}",
                            hex::encode(Sha256::digest(material.as_bytes()))
                        ),
                        catalog_generation: catalog.generation,
                    },
                );
            }
        }
    }
    // custom_baseline is deliberately protocol-neutral. Exact protocol behavior is applied only
    // to per-protocol variants below, then the frozen model binding projects one variant into the
    // effective model view. Connection credentials never carry protocol authority.
    // custom_baseline 有意保持协议中立。下方只对逐协议变体应用精确协议行为，再由冻结模型绑定
    // 把唯一变体投影到有效模型视图；连接凭据绝不携带协议权威。
    let custom_baseline =
        custom_provider.then(|| custom_provider_system_baseline_for_protocol(None));
    // custom_provider_settings is the optional user-owned provider layer above catalog declarations.
    // custom_provider_settings 是位于目录声明之上的可选用户供应商层。
    let custom_provider_settings = custom_provider
        .then(|| {
            configuration
                .custom_provider_model_defaults
                .get(provider_id)
        })
        .flatten();
    if let Some(settings) = custom_provider_settings {
        settings.validate()?;
    }
    let mut models = catalog
        .models
        .drain(..)
        .map(|mut model| {
            let model_id = model.id.clone();
            if custom_provider {
                configuration.apply_provider_capability_defaults(
                    provider_id,
                    &model.capability_evidence,
                    &mut model.capabilities,
                );
                apply_custom_model_defaults(
                    configuration,
                    provider_id,
                    model.id.as_str(),
                    custom_baseline
                        .as_ref()
                        .expect("custom baseline must exist"),
                    custom_provider_settings,
                    &model.capability_evidence,
                    &mut model.capabilities,
                );
                // Every discovered protocol variant resolves the same ordered policy layers
                // against its own evidence. Applying them only to the top-level compatibility
                // view would discard provider/model settings when execution selects a variant.
                // 每个已发现协议变体都依据自身证据解析同一有序策略层。若只应用到顶层兼容视图，
                // 执行选择变体时会丢失供应商/模型设置。
                for (protocol_profile_id, variant) in &mut model.protocol_variants {
                    let baseline = custom_provider_system_baseline_for_protocol(Some(
                        protocol_profile_id.as_str(),
                    ));
                    configuration.apply_provider_capability_defaults(
                        provider_id,
                        &variant.capability_evidence,
                        &mut variant.capabilities,
                    );
                    apply_custom_model_defaults(
                        configuration,
                        provider_id,
                        model_id.as_str(),
                        &baseline,
                        custom_provider_settings,
                        &variant.capability_evidence,
                        &mut variant.capabilities,
                    );
                }
            } else {
                configuration.apply_provider_capability_defaults(
                    provider_id,
                    &model.capability_evidence,
                    &mut model.capabilities,
                );
                crate::apply_common_model_context_default(
                    model.id.as_str(),
                    &mut model.capabilities,
                );
                apply_host_default_reasoning_policy(&mut model.capabilities);
                configuration.apply_model_capability_overrides(
                    provider_id,
                    model.id.as_str(),
                    &mut model.capabilities,
                );
                for variant in model.protocol_variants.values_mut() {
                    configuration.apply_provider_capability_defaults(
                        provider_id,
                        &variant.capability_evidence,
                        &mut variant.capabilities,
                    );
                    crate::apply_common_model_context_default(
                        model_id.as_str(),
                        &mut variant.capabilities,
                    );
                    apply_host_default_reasoning_policy(&mut variant.capabilities);
                    configuration.apply_model_capability_overrides(
                        provider_id,
                        model_id.as_str(),
                        &mut variant.capabilities,
                    );
                }
            }
            (model.id.clone(), model)
        })
        .collect::<BTreeMap<_, _>>();
    if source_policy.includes_manual()
        && let Some(custom_models) = configuration.custom_models.get(provider_id)
    {
        for definition in custom_models.values() {
            definition.validate()?;
            let availability = configuration.custom_model_connection_enabled_override(
                catalog.connection_id.as_str(),
                definition.id.as_str(),
            );
            // An explicit connection-level denial is the only rule that may remove a manual
            // identity. Discovery owns observed fields for collisions, but its result set never
            // erases the independently authorized manual source in a union policy (plan §14.1).
            // 只有显式的连接级禁用规则可移除手工模型身份。发现在同 ID 冲突时拥有
            // 观测字段，但在并集策略下绝不得删除独立授权的手工来源（方案 §14.1）。
            if availability == Some(false) {
                continue;
            }
            // A successful provider catalog remains authoritative for identity, limits, modalities,
            // and operations. The manual declaration may still add explicit advanced behavior that
            // `/models` cannot describe, such as reasoning projections and model parameters.
            // 成功的供应商目录仍是身份、限制、多模态和操作的权威来源。手工声明仍可补充
            // `/models` 无法表达的显式高级行为，例如思考投影和模型参数。
            if let Some(discovered_model) = models.get_mut(definition.id.as_str()) {
                discovered_model.display_name = definition.display_name.clone();
                apply_custom_model_advanced_declaration(
                    &definition.capabilities,
                    &mut discovered_model.capabilities,
                );
                // Manual advanced fields are an allowed post-Manifest user layer and therefore
                // must reach every executable protocol variant, not only the effective view.
                // 手工高级字段是允许的 Manifest 后用户层，因此必须进入每个可执行协议变体，
                // 而不是只修改有效视图。
                for variant in discovered_model.protocol_variants.values_mut() {
                    apply_custom_model_advanced_declaration(
                        &definition.capabilities,
                        &mut variant.capabilities,
                    );
                }
                continue;
            }
            let mut capabilities = definition.capabilities.clone();
            let capability_evidence = manual_model_capability_evidence(&capabilities);
            if custom_provider {
                configuration.apply_provider_capability_defaults(
                    provider_id,
                    &capability_evidence,
                    &mut capabilities,
                );
                apply_custom_model_defaults(
                    configuration,
                    provider_id,
                    definition.id.as_str(),
                    custom_baseline
                        .as_ref()
                        .expect("custom baseline must exist"),
                    custom_provider_settings,
                    &capability_evidence,
                    &mut capabilities,
                );
                // The manual advanced declaration is a model-owned layer above the provider policy,
                // including for models admitted without a matching discovery record.
                // 手工高级声明是位于供应商策略之上的模型自有层，也适用于无对应发现记录的准入模型。
                apply_custom_model_advanced_declaration(
                    &definition.capabilities,
                    &mut capabilities,
                );
            } else {
                configuration.apply_provider_capability_defaults(
                    provider_id,
                    &capability_evidence,
                    &mut capabilities,
                );
                crate::apply_common_model_context_default(
                    definition.id.as_str(),
                    &mut capabilities,
                );
                apply_host_default_reasoning_policy(&mut capabilities);
            }
            if !custom_provider {
                configuration.apply_model_capability_overrides(
                    provider_id,
                    definition.id.as_str(),
                    &mut capabilities,
                );
            }
            // model receives signed protocol variants for system policies before the allowed
            // manual advanced layer is projected onto each variant.
            // model 在允许的手工高级层投影到每个变体前，先接收系统策略的签名协议变体。
            let mut model = ConnectionModel {
                id: definition.id.clone(),
                display_name: definition.display_name.clone(),
                capabilities,
                capability_evidence: capability_evidence.clone(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            };
            if !custom_provider {
                let manifest = manifest.ok_or_else(|| {
                    format!(
                        "System provider `{provider_id}` admitted a manual model without an active Manifest."
                    )
                })?;
                let connection = configuration
                    .connection_graph
                    .connections
                    .get(catalog.connection_id.as_str())
                    .ok_or_else(|| {
                        format!(
                            "Credential connection `{}` is not configured for manual model projection.",
                            catalog.connection_id
                        )
                    })?;
                project_manifest_protocol_variants_for_connection(
                    manifest,
                    connection,
                    catalog.generation,
                    &mut model,
                    true,
                )?;
                apply_custom_model_advanced_declaration(
                    &definition.capabilities,
                    &mut model.capabilities,
                );
                for variant in model.protocol_variants.values_mut() {
                    configuration.apply_provider_capability_defaults(
                        provider_id,
                        &variant.capability_evidence,
                        &mut variant.capabilities,
                    );
                    crate::apply_common_model_context_default(
                        definition.id.as_str(),
                        &mut variant.capabilities,
                    );
                    apply_host_default_reasoning_policy(&mut variant.capabilities);
                    apply_custom_model_advanced_declaration(
                        &definition.capabilities,
                        &mut variant.capabilities,
                    );
                    configuration.apply_model_capability_overrides(
                        provider_id,
                        definition.id.as_str(),
                        &mut variant.capabilities,
                    );
                }
            } else {
                let provider = configuration
                    .custom_providers
                    .get(provider_id)
                    .ok_or_else(|| format!("Custom provider `{provider_id}` is not configured."))?;
                let entry = provider
                    .service_entries
                    .iter()
                    .find(|entry| entry.id == service_entry_id)
                    .ok_or_else(|| {
                        format!(
                            "Custom provider `{provider_id}` has no service entry `{service_entry_id}`."
                        )
                    })?;
                // A manual identity has provider-neutral evidence. Materialize one independent
                // variant per enabled protocol before binding freeze so protocol-specific defaults
                // and later user overrides cannot leak between variants.
                // 手工身份具有供应商无关证据。在绑定冻结前为每个启用协议物化独立变体，使协议专属
                // 默认值与后续用户覆盖不会在变体间泄漏。
                for route in &entry.protocol_routes {
                    let mut variant_capabilities = definition.capabilities.clone();
                    configuration.apply_provider_capability_defaults(
                        provider_id,
                        &capability_evidence,
                        &mut variant_capabilities,
                    );
                    let baseline = custom_provider_system_baseline_for_protocol(Some(
                        route.protocol_profile_id.as_str(),
                    ));
                    apply_custom_model_defaults(
                        configuration,
                        provider_id,
                        definition.id.as_str(),
                        &baseline,
                        custom_provider_settings,
                        &capability_evidence,
                        &mut variant_capabilities,
                    );
                    apply_custom_model_advanced_declaration(
                        &definition.capabilities,
                        &mut variant_capabilities,
                    );
                    let material = format!(
                        "custom-manual\0{provider_id}\0{service_entry_id}\0{}\0{}",
                        definition.id, route.protocol_profile_id
                    );
                    model.protocol_variants.insert(
                        route.protocol_profile_id.clone(),
                        ProtocolModelVariant {
                            capabilities: variant_capabilities,
                            capability_evidence: capability_evidence.clone(),
                            discovery_signature: format!(
                                "sha256:{}",
                                hex::encode(Sha256::digest(material.as_bytes()))
                            ),
                            catalog_generation: catalog.generation,
                        },
                    );
                }
            }
            models.insert(definition.id.clone(), model);
        }
    }
    if custom_provider {
        // Direct model settings are the strongest user layer and must follow any advanced manual declaration.
        // 直接模型设置是最高用户层，必须在任何高级手工声明之后应用。
        for model in models.values_mut() {
            configuration.apply_model_capability_overrides(
                provider_id,
                model.id.as_str(),
                &mut model.capabilities,
            );
            for variant in model.protocol_variants.values_mut() {
                configuration.apply_model_capability_overrides(
                    provider_id,
                    model.id.as_str(),
                    &mut variant.capabilities,
                );
            }
        }
    }
    // Account access is evaluated after support membership and capability resolution so it can
    // hide every fixed, discovered, or manually admitted model without mutating source evidence.
    // 账号访问策略在支持成员关系与能力解析后求值，因此可以隐藏任意固定、发现或手工准入模型，
    // 同时不会修改原始证据。
    models.retain(|model_id, _| {
        configuration.connection_model_is_enabled(catalog.connection_id.as_str(), model_id)
    });
    if let (Some(account_usage), Some(manifest), Some(connection)) = (
        account_usage,
        manifest,
        configuration
            .connection_graph
            .connections
            .get(catalog.connection_id.as_str()),
    ) {
        let account_model_access = manifest
            .descriptor
            .service_entries
            .iter()
            .find(|entry| entry.id == service_entry_id)
            .and_then(|entry| entry.credential_profile(connection.credential_profile_id.as_str()))
            .and_then(|profile| profile.account_model_access.as_ref());
        if let Some(policy) = account_model_access
            && !account_usage_unlocks_catalog(policy, account_usage)
        {
            // Missing or unknown account evidence cannot invent a tier. Exact declared rules are
            // filtered only by configured dimensions; unmatched discovered models remain visible.
            // 缺失或未知账号证据不能臆造套餐。精确声明规则只按已配置维度过滤；未匹配发现模型保持可见。
            models.retain(|model_id, _| {
                manifest
                    .rule_for_model(
                        service_entry_id,
                        Some(connection.credential_profile_id.as_str()),
                        model_id,
                    )
                    .is_none_or(|rule| {
                        model_rule_allows_account_access(rule, policy, account_usage, model_id)
                    })
            });
        }
    }
    catalog.models = models.into_values().collect();
    // Custom-provider protocol bindings freeze at this catalog activation boundary so
    // execution never re-resolves Auto on every request; enabled-protocol changes and
    // preference saves rebuild the catalog, which re-freezes every binding (plan §13.6).
    // 自定义供应商协议绑定在该目录激活边界冻结，执行绝不每次请求重新解析 Auto；
    // 启用协议变更与偏好保存会重建目录，从而重新冻结全部绑定（方案 §13.6）。
    if custom_provider {
        freeze_custom_provider_protocol_bindings(
            configuration,
            provider_id,
            service_entry_id,
            manifest.map_or(0, |manifest| manifest.revision),
            catalog.generation,
            &mut catalog.models,
        )?;
    } else if let Some(manifest) = manifest {
        let connection = configuration
            .connection_graph
            .connections
            .get(catalog.connection_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential connection `{}` is not configured for system binding projection.",
                    catalog.connection_id
                )
            })?;
        freeze_system_provider_protocol_bindings(manifest, connection, &mut catalog.models)?;
    }
    if custom_provider {
        // Custom providers are conversation-only adapters; the effective bound variant decides
        // support, never an arbitrary discovery route's compatibility fields. An unbound model
        // or a binding whose exact variant was not discovered remains visible with its empty
        // projection so the UI can explain the blocked state.
        // 自定义供应商是纯会话适配器；支持状态由生效绑定变体决定，绝不读取任意发现路由的兼容
        // 字段。无绑定模型或未发现精确变体的绑定以空投影保持可见，使 UI 能解释阻止状态。
        catalog.models.retain(|model| {
            model.resolved_protocol_binding.is_none()
                || frozen_binding_variant_is_missing(model)
                || model
                    .capabilities
                    .operations
                    .contains(&AiOperation::ConversationRespond)
        });
    }
    // Validate request projection against the effective bound-model view after binding
    // freeze, so a projection cannot be accepted using another protocol's capabilities.
    // 在绑定冻结后依据有效绑定模型视图校验请求投影，避免使用另一协议能力错误放行投影。
    for model in &catalog.models {
        let manifest_managed_unbound =
            (custom_provider || manifest.is_some()) && model.resolved_protocol_binding.is_none();
        if manifest_managed_unbound || frozen_binding_variant_is_missing(model) {
            continue;
        }
        if let Some(projection) =
            resolve_request_projection(configuration, provider_id, model.id.as_str())?
        {
            projection.validate_for_capabilities(&model.capabilities)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
