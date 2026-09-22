//! Catalog-bound custom and system protocol selection with immutable variant projection.
//! 目录绑定的自定义与系统协议选择及不可变变体投影。

use crate::{
    ConnectionModel, CredentialConnection, ModelCapabilityEvidence, PersistedAiConfiguration,
    ProtocolModelVariant, ResolvedProviderManifest,
};
use sha2::{Digest, Sha256};
use vulcan_ai_contract::ModelCapabilities;

/// Resolves the model-level protocol binding for one custom-provider model.
/// 为一个自定义供应商模型解析模型级协议绑定。
///
/// # Parameters
/// 参数
/// `configuration` owns the persisted per-model preferences; `provider` and `entry` supply the
/// enabled protocol set; `model_available_protocol_profile_ids` supplies this model's exact
/// observed or manually declared variants; `model_id` identifies the exact model; and
/// `manifest_revision` identifies the activated custom-provider contract.
/// `configuration` 持有持久化逐模型偏好；`provider` 与 `entry` 提供启用协议集合；
/// `model_available_protocol_profile_ids` 提供当前模型已观测或手工声明的精确变体；
/// `model_id` 标识精确模型；`manifest_revision` 标识已激活自定义供应商契约。
///
/// # Returns
/// 返回值
/// The binding for custom providers, or `None` for providers outside the custom contract.
/// 自定义供应商返回绑定；自定义契约之外的供应商返回 `None`。
fn resolve_custom_provider_binding(
    configuration: &PersistedAiConfiguration,
    provider: &crate::ProviderDescriptor,
    entry: &crate::ServiceEntry,
    model_available_protocol_profile_ids: &[&str],
    model_id: &str,
    manifest_revision: u64,
) -> Result<Option<crate::provider_manifest::ModelProtocolBinding>, String> {
    if provider.kind != crate::ProviderKind::Custom {
        return Ok(None);
    }
    let enabled_protocols = entry
        .protocol_routes
        .iter()
        .map(|route| route.protocol_profile_id.as_str())
        .collect::<Vec<_>>();
    let preference_key =
        PersistedAiConfiguration::provider_model_key(provider.id.as_str(), model_id);
    let explicit_protocol = configuration
        .custom_model_protocol_preferences
        .get(preference_key.as_str())
        .and_then(|preference| preference.explicit_protocol())
        .map(|protocol| protocol.profile_id());
    crate::provider_manifest::resolve_custom_model_protocol_binding(
        enabled_protocols.as_slice(),
        model_available_protocol_profile_ids,
        model_id,
        manifest_revision,
        explicit_protocol,
    )
    .map(Some)
    .map_err(|error| format!("Model `{model_id}` is not executable: {error}"))
}

/// Freezes one custom provider's model-level protocol bindings into its catalog snapshot.
/// 把一个自定义供应商的模型级协议绑定冻结进其目录快照。
///
/// A binding that still matches the current preference and enabled protocol set is retained
/// without re-resolving Auto; only drifted bindings (preference change, explicit removal, or
/// enabled-protocol change) re-freeze at the activation boundary (plan §13.6).
/// 与当前偏好及启用协议集合仍一致的绑定被保留且不重新解析 Auto；只有漂移绑定
/// （偏好变更、显式偏好移除或启用协议变更）才在激活边界重新冻结（方案 §13.6）。
///
/// # Parameters
/// 参数
/// `configuration` owns the per-model preferences and the provider descriptor; `provider_id`
/// and `service_entry_id` select the exact entry; `manifest_revision` identifies the activated
/// custom contract; `catalog_generation` identifies the immutable catalog snapshot; `models`
/// receives the frozen bindings and compatibility projection.
/// `configuration` 持有逐模型偏好与供应商描述；`provider_id` 与 `service_entry_id` 选择
/// 精确入口；`manifest_revision` 标识已激活自定义契约；`catalog_generation` 标识不可变目录
/// 快照；`models` 接收冻结绑定与兼容投影。
///
/// # Returns
/// 返回值
/// Success after every drifted model carries its re-frozen binding; a missing provider or service
/// entry is an activation invariant violation, while models with invalid preferences keep `None`
/// so the UI and execution path report their controlled binding error.
/// 每个漂移模型携带其重新冻结绑定后返回成功；缺失供应商或服务入口属于激活不变量违规，
/// 偏好无效的模型保持 `None`，由 UI 与执行路径报告受控绑定错误。
pub(super) fn freeze_custom_provider_protocol_bindings(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    manifest_revision: u64,
    catalog_generation: u64,
    models: &mut [ConnectionModel],
) -> Result<(), String> {
    let provider = configuration
        .custom_providers
        .get(provider_id)
        .ok_or_else(|| {
            format!(
                "Custom provider `{provider_id}` disappeared before protocol bindings were frozen."
            )
        })?;
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == service_entry_id)
        .ok_or_else(|| {
            format!(
                "Custom provider `{provider_id}` service entry `{service_entry_id}` disappeared before protocol bindings were frozen."
            )
        })?;
    let enabled_protocols = entry
        .protocol_routes
        .iter()
        .map(|route| route.protocol_profile_id.as_str())
        .collect::<Vec<_>>();
    for model in models.iter_mut() {
        // availableProtocols is the exact model-level intersection authority. Auto must never
        // select a provider-enabled route that did not discover or explicitly declare this model.
        // availableProtocols 是精确的模型级交集权威。Auto 绝不能选择未发现或未显式声明当前
        // 模型的供应商已启用路由。
        let available_protocols = model.protocol_variants.keys().cloned().collect::<Vec<_>>();
        let available_protocol_refs = available_protocols
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let preference = configuration.custom_model_protocol_preferences.get(
            PersistedAiConfiguration::provider_model_key(provider_id, model.id.as_str()).as_str(),
        );
        let frozen_is_current = model
            .resolved_protocol_binding
            .as_ref()
            .map(|binding| {
                binding.manifest_revision == manifest_revision
                    && frozen_binding_is_current(
                        binding,
                        preference,
                        enabled_protocols.as_slice(),
                        available_protocol_refs.as_slice(),
                    )
            })
            .unwrap_or(false);
        if frozen_is_current {
            ensure_manual_custom_bound_variant(
                provider_id,
                service_entry_id,
                catalog_generation,
                model,
            );
            project_frozen_model_binding_compatibility(model)?;
            continue;
        }
        model.resolved_protocol_binding = resolve_custom_provider_binding(
            configuration,
            provider,
            entry,
            available_protocol_refs.as_slice(),
            model.id.as_str(),
            manifest_revision,
        )
        .unwrap_or_default();
        ensure_manual_custom_bound_variant(
            provider_id,
            service_entry_id,
            catalog_generation,
            model,
        );
        project_frozen_model_binding_compatibility(model)?;
    }
    Ok(())
}

/// Freezes provider-neutral manual evidence into one custom model's exact bound protocol.
/// 把供应商无关的手工证据冻结到一个自定义模型的精确绑定协议。
///
/// # Parameters
/// 参数
/// Provider, entry, and generation identify the immutable catalog snapshot; `model` receives a
/// synthetic manual-source variant only when it has a binding and no discovered variants.
/// 供应商、入口与代次标识不可变目录快照；仅当 `model` 有绑定且没有发现变体时，才接收一个
/// 手工来源合成变体。
fn ensure_manual_custom_bound_variant(
    provider_id: &str,
    service_entry_id: &str,
    catalog_generation: u64,
    model: &mut ConnectionModel,
) {
    let Some(binding) = model.resolved_protocol_binding.as_ref() else {
        return;
    };
    if !model.protocol_variants.is_empty() {
        return;
    }
    // Manual-only custom models have provider-neutral user evidence rather than a discovery
    // route. The stable digest identifies this explicit local source without pretending it was
    // observed from a remote endpoint.
    // 纯手工自定义模型只有供应商无关的用户证据而没有发现路由。稳定摘要标识该显式本地来源，
    // 不会伪装成来自远端端点的观测。
    let material = format!(
        "custom-manual\0{provider_id}\0{service_entry_id}\0{}\0{}",
        model.id, binding.protocol_profile_id
    );
    model.protocol_variants.insert(
        binding.protocol_profile_id.clone(),
        ProtocolModelVariant {
            capabilities: model.capabilities.clone(),
            capability_evidence: model.capability_evidence.clone(),
            discovery_signature: format!(
                "sha256:{}",
                hex::encode(Sha256::digest(material.as_bytes()))
            ),
            catalog_generation,
        },
    );
}

/// Freezes system-manifest bindings and their exact compatibility projections for one connection.
/// 为一个连接冻结系统 Manifest 绑定及其精确兼容投影。
///
/// # Parameters
/// 参数
/// `manifest` is the active compiled authority, `connection` supplies the exact entry/profile
/// scope, and `models` receives bindings or an explicit empty non-executable projection.
/// `manifest` 是当前编译权威，`connection` 提供精确入口/配置档作用域，`models` 接收绑定或
/// 显式空的不可执行投影。
///
/// # Returns
/// 返回值
/// Success after every model is either exactly bound or explicitly non-executable.
/// 每个模型都被精确绑定或被明确标记为不可执行后返回成功。
pub(super) fn freeze_system_provider_protocol_bindings(
    configuration: &PersistedAiConfiguration,
    manifest: &ResolvedProviderManifest,
    connection: &CredentialConnection,
    models: &mut [ConnectionModel],
) -> Result<(), String> {
    for model in models {
        // An unmatched identity may bind only through a persisted user declaration for this
        // exact service entry. Signed rules retain precedence when they exist.
        // 未命中规则的身份只能通过属于此精确服务入口的持久化用户声明绑定；已有签名规则优先。
        let manual_binding = configuration
            .custom_model(connection.provider_id.as_str(), model.id.as_str())
            .and_then(|definition| definition.system_binding.as_ref())
            .filter(|binding| binding.service_entry_id == connection.service_entry_id);
        let signed_rule = manifest.rule_for_model(
            connection.service_entry_id.as_str(),
            Some(connection.credential_profile_id.as_str()),
            model.id.as_str(),
        );
        if signed_rule.is_none()
            && let Some(binding) = manual_binding
        {
            model.resolved_protocol_binding =
                Some(crate::provider_manifest::ModelProtocolBinding {
                    protocol_profile_id: binding.protocol_profile_id.clone(),
                    source: crate::provider_manifest::ModelProtocolBindingSource::SystemUser,
                    manifest_revision: manifest.revision,
                    rule_id: String::new(),
                    codex_behavior: None,
                });
            project_frozen_model_binding_compatibility(model)?;
            continue;
        }
        match crate::provider_manifest::resolve_system_model_protocol_binding(
            manifest,
            connection.service_entry_id.as_str(),
            Some(connection.credential_profile_id.as_str()),
            model.id.as_str(),
        ) {
            Ok(binding) => model.resolved_protocol_binding = Some(binding),
            Err(crate::provider_manifest::ModelProtocolBindingError::Missing { .. }) => {
                model.resolved_protocol_binding = None;
            }
            Err(error) => {
                return Err(format!(
                    "System model `{}` binding projection failed: {error}",
                    model.id
                ));
            }
        }
        project_frozen_model_binding_compatibility(model)?;
    }
    Ok(())
}

/// Projects one frozen model binding into the read-only compatibility capability fields.
/// 把一个冻结模型绑定投影到只读兼容能力字段。
///
/// # Parameters
/// 参数
/// `model` owns the frozen binding, protocol variants, and compatibility fields.
/// `model` 持有冻结绑定、协议变体与兼容字段。
///
/// # Returns
/// 返回值
/// Success after projecting an exact variant or an empty non-executable view. A bound model
/// without its exact variant remains visible for status diagnostics and fails at execution.
/// 投影精确变体或空的不可执行视图后返回成功。缺少精确变体的已绑定模型保留用于状态诊断，
/// 并在执行时失败。
fn project_frozen_model_binding_compatibility(model: &mut ConnectionModel) -> Result<(), String> {
    let Some(binding) = model.resolved_protocol_binding.as_ref() else {
        model.capabilities = ModelCapabilities::default();
        model.capability_evidence = ModelCapabilityEvidence::default();
        return Ok(());
    };
    let Some(variant) = model
        .protocol_variants
        .get(binding.protocol_profile_id.as_str())
    else {
        // A discovery mismatch blocks only this model. Clearing the effective view prevents
        // workbench admission while preserving the frozen binding and observed variants for the
        // model-level status explanation.
        // 发现不一致只阻止当前模型。清空有效视图可阻止工作台准入，同时保留冻结绑定与已观测
        // 变体，用于模型级状态解释。
        model.capabilities = ModelCapabilities::default();
        model.capability_evidence = ModelCapabilityEvidence::default();
        return Ok(());
    };
    model.capabilities = variant.capabilities.clone();
    model.capability_evidence = variant.capability_evidence.clone();
    Ok(())
}

/// Reports whether one frozen model binding lacks its exact observed protocol variant.
/// 报告一个冻结模型绑定是否缺少其精确已观测协议变体。
///
/// # Parameters
/// 参数
/// `model` owns the frozen binding and immutable protocol-variant map.
/// `model` 持有冻结绑定与不可变协议变体映射。
///
/// # Returns
/// 返回值
/// `true` only for a bound, model-level blocked catalog row.
/// 仅对已绑定但在模型级被阻止的目录行返回 `true`。
pub(super) fn frozen_binding_variant_is_missing(model: &ConnectionModel) -> bool {
    model
        .resolved_protocol_binding
        .as_ref()
        .is_some_and(|binding| {
            !model
                .protocol_variants
                .contains_key(binding.protocol_profile_id.as_str())
        })
}

/// Reports whether one frozen binding still matches the current preference and model availability.
/// 报告一个冻结绑定是否仍匹配当前偏好与模型可用协议集合。
fn frozen_binding_is_current(
    binding: &crate::provider_manifest::ModelProtocolBinding,
    preference: Option<&crate::CustomModelProtocolPreference>,
    enabled_protocols: &[&str],
    model_available_protocols: &[&str],
) -> bool {
    // expectedBinding reuses the single authoritative resolver. A provider-wide route remaining
    // enabled is insufficient: this exact model must still expose the variant, and a newly
    // observed higher-priority variant must re-freeze Auto at the activation boundary.
    // expectedBinding 复用唯一权威解析器。供应商级路由仍启用并不足够：当前模型必须仍公开该
    // 变体；新观测到的更高优先级变体也必须在激活边界重新冻结 Auto。
    let explicit_protocol = preference
        .and_then(|preference| (*preference).explicit_protocol())
        .map(|protocol| protocol.profile_id());
    crate::provider_manifest::resolve_custom_model_protocol_binding(
        enabled_protocols,
        model_available_protocols,
        "frozen-model",
        binding.manifest_revision,
        explicit_protocol,
    )
    .ok()
    .is_some_and(|expected| {
        binding.source == expected.source
            && binding.protocol_profile_id == expected.protocol_profile_id
    })
}

#[cfg(test)]
mod tests;
