//! Signed model-profile precedence and frozen route resource constraints.
//! 签名模型 Profile 优先级与冻结路由资源约束。

/// Borrowed profile references effective for one exact model binding scope.
/// 一个精确模型绑定作用域内生效的 Profile 引用借用视图。
struct EffectiveModelProfileRefs<'a> {
    /// Ordered usage extraction profile identifiers.
    /// 有序用量提取 Profile 标识。
    usage: &'a [String],
    /// Whether the model/default layer replaces the selected protocol route usage chain.
    /// 模型/默认层是否替换所选协议路由的 usage 链。
    usage_overrides_route_default: bool,
    /// Ordered request projection profile identifiers.
    /// 有序请求投影 Profile 标识。
    request: &'a [String],
    /// Whether the model/default layer replaces the selected protocol route request chain.
    /// 模型/默认层是否替换所选协议路由的请求链。
    request_overrides_route_default: bool,
    /// Ordered response and error projection profile identifiers.
    /// 有序响应与错误投影 Profile 标识。
    response: &'a [String],
    /// Whether the model/default layer replaces the selected protocol route response chain.
    /// 模型/默认层是否替换所选协议路由的响应链。
    response_overrides_route_default: bool,
}

/// Reports whether one resolved model rule explicitly replaces a route-level profile chain.
/// 报告一条已解析模型规则是否显式替换路由级 Profile 链。
///
/// # Parameters
/// 参数
/// `instance`, `field`, and `references` identify the resolved field and its frozen provenance.
/// `instance`、`field` 与 `references` 标识已解析字段及其冻结来源。
///
/// # Returns
/// 返回值
/// `true` for a non-empty inherited model default or any exact/prefix rule declaration,
/// including an explicit empty list.
/// 对非空继承模型默认或任意 Exact/Prefix 规则声明（包括显式空列表）返回 `true`。
fn model_rule_replaces_route_profile_chain(
    instance: &crate::provider_manifest::ResolvedRuleInstance,
    field: &str,
    references: &[String],
) -> bool {
    !references.is_empty()
        || instance.field_provenance.get(field).is_some_and(|source| {
            matches!(
                source.source,
                crate::provider_manifest::FieldSourceKind::ModelRuleExact
                    | crate::provider_manifest::FieldSourceKind::ModelRulePrefix
            )
        })
}

/// Resolves model-rule or entry-default profile references from one immutable manifest snapshot.
/// 从一份不可变 Manifest 快照解析模型规则或入口默认 Profile 引用。
///
/// # Parameters
/// 参数
/// `manifest`, `service_entry_id`, `credential_profile_id`, and `model_id` identify the exact
/// signed binding scope.
/// `manifest`、`service_entry_id`、`credential_profile_id` 与 `model_id` 标识精确签名绑定作用域。
///
/// # Returns
/// 返回值
/// The winning rule's inherited references, the explicit unknown-model defaults, or empty
/// references when an unmatched model has no default-binding authority.
/// 返回胜出规则继承的引用、显式未知模型默认引用，或在未命中模型无默认绑定权限时返回空引用。
fn effective_model_profile_refs<'a>(
    manifest: &'a crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
) -> Result<EffectiveModelProfileRefs<'a>, String> {
    if let Some(instance) =
        manifest.rule_for_model(service_entry_id, credential_profile_id, model_id)
    {
        return Ok(EffectiveModelProfileRefs {
            usage: instance.usage_profile_refs.as_slice(),
            usage_overrides_route_default: model_rule_replaces_route_profile_chain(
                instance,
                "usage_profile_refs",
                instance.usage_profile_refs.as_slice(),
            ),
            request: instance.request_profile_refs.as_slice(),
            request_overrides_route_default: model_rule_replaces_route_profile_chain(
                instance,
                "request_profile_refs",
                instance.request_profile_refs.as_slice(),
            ),
            response: instance.response_profile_refs.as_slice(),
            response_overrides_route_default: model_rule_replaces_route_profile_chain(
                instance,
                "response_profile_refs",
                instance.response_profile_refs.as_slice(),
            ),
        });
    }
    if manifest.unknown_model_policy
        != crate::provider_manifest::UnknownModelPolicy::DefaultBindingAllowed
    {
        return Ok(EffectiveModelProfileRefs {
            usage: &[],
            usage_overrides_route_default: false,
            request: &[],
            request_overrides_route_default: false,
            response: &[],
            response_overrides_route_default: false,
        });
    }
    let defaults = manifest.model_defaults_for_entry(service_entry_id)?;
    Ok(EffectiveModelProfileRefs {
        usage: defaults.usage_profile_refs.as_slice(),
        usage_overrides_route_default: !defaults.usage_profile_refs.is_empty(),
        request: defaults.request_profile_refs.as_slice(),
        request_overrides_route_default: !defaults.request_profile_refs.is_empty(),
        response: defaults.response_profile_refs.as_slice(),
        response_overrides_route_default: !defaults.response_profile_refs.is_empty(),
    })
}

/// Resolves provider-wide route constraints with model-specific narrowing overrides.
/// 使用模型专属收紧覆盖解析供应商通用路由约束。
///
/// # Parameters
/// 参数
/// `manifest`, entry, credential, and model identity select the exact rule; `route_default`
/// is the selected protocol route's immutable provider-wide baseline.
/// `manifest`、入口、凭据与模型身份选择精确规则；`route_default` 是所选协议路由不可变的
/// 供应商通用基线。
///
/// # Returns
/// 返回值
/// A deterministic per-kind merge frozen into the execution route.
/// 返回冻结到执行路由的按资源类别确定性合并结果。
pub(super) fn resolve_input_resource_constraints(
    manifest: &crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    mut route_default: Vec<crate::InputResourceConstraint>,
) -> Vec<crate::InputResourceConstraint> {
    let Some(instance) = manifest.rule_for_model(service_entry_id, credential_profile_id, model_id)
    else {
        return route_default;
    };
    for constraint in &instance.input_resource_constraints {
        if let Some(existing) = route_default
            .iter_mut()
            .find(|existing| existing.resource_kind == constraint.resource_kind)
        {
            let mut resolved = constraint.clone();
            if resolved.upload_channel_refs.is_empty() {
                resolved.upload_channel_refs = existing.upload_channel_refs.clone();
            }
            *existing = resolved;
        } else {
            route_default.push(constraint.clone());
        }
    }
    route_default
}

/// Resolves the effective usage-extraction profile chain for one exact model.
/// 解析一个精确模型的生效用量提取 Profile 链。
///
/// # Parameters
/// 参数
/// `manifest`, entry, credential, and model identity select the exact signed references;
/// `protocol_profile_id` identifies the selected standard baseline and `route_default` is the
/// selected protocol route's immutable provider-specific baseline.
/// `manifest`、入口、凭据与模型身份选择精确签名引用；`route_default` 是所选协议路由的
/// 不可变供应商专属基线，`protocol_profile_id` 标识所选标准协议基线。
///
/// # Returns
/// 返回值
/// The route baseline when no model-level references exist, or the fully resolved replacement
/// chain. Any missing compiled payload fails closed.
/// 无模型级引用时返回路由基线，否则返回完整解析后的替换链；任何缺失的编译载荷均失败关闭。
pub(super) fn resolve_usage_extraction_chain(
    manifest: &crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    protocol_profile_id: &str,
    route_default: Vec<vulcan_llm_core::UsageExtractionProfile>,
) -> Result<Vec<vulcan_llm_core::UsageExtractionProfile>, String> {
    let refs =
        effective_model_profile_refs(manifest, service_entry_id, credential_profile_id, model_id)?;
    if !refs.usage_overrides_route_default {
        return Ok(route_default);
    }
    if refs.usage.is_empty() {
        let protocol = crate::execution::upstream_protocol_for_profile_id(protocol_profile_id)
            .ok_or_else(|| {
                format!(
                    "Model `{model_id}` clears route usage profiles but protocol profile `{protocol_profile_id}` has no registered standard usage baseline."
                )
            })?;
        return Ok(vulcan_llm_core::standard_usage_profiles(protocol));
    }
    refs.usage
        .iter()
        .map(|profile_id| {
            manifest
                .usage_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.clone())
                .ok_or_else(|| {
                    format!(
                        "Model `{model_id}` references missing compiled usage profile `{profile_id}`."
                    )
                })
        })
        .collect()
}

/// Resolves the effective request-projection profile chain for one exact model.
/// 解析一个精确模型的生效请求投影 Profile 链。
///
/// # Parameters
/// 参数
/// `manifest`, entry, credential, and model identity select the signed binding scope.
/// `manifest`、入口、凭据与模型身份选择签名绑定作用域。
/// `route_default` is retained unless an inherited default or explicit model rule replaces it.
/// `route_default` 保留到继承默认或显式模型规则将其替换；显式空列表会清除该链。
///
/// # Returns
/// 返回值
/// The exact route baseline or compiled replacement, including an explicit empty replacement.
/// 返回精确路由基线或已编译替换链，包括显式空替换。
/// Missing compiled payloads return an explicit error.
/// 缺少已编译载荷时返回显式错误。
pub(super) fn resolve_request_projection_chain(
    manifest: &crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    route_default: Vec<crate::provider_manifest::RequestProjectionProfile>,
) -> Result<Vec<crate::provider_manifest::RequestProjectionProfile>, String> {
    let refs =
        effective_model_profile_refs(manifest, service_entry_id, credential_profile_id, model_id)?;
    if !refs.request_overrides_route_default {
        return Ok(route_default);
    }
    refs.request
        .iter()
        .map(|profile_id| {
            manifest
                .request_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.clone())
                .ok_or_else(|| {
                    format!(
                        "Model `{model_id}` references missing compiled request profile `{profile_id}`."
                    )
                })
        })
        .collect()
}

/// Resolves the effective response-projection profile chain for one exact model.
/// 解析一个精确模型的生效响应投影 Profile 链。
///
/// # Parameters
/// 参数
/// `manifest`, entry, credential, and model identity select the signed binding scope.
/// `manifest`、入口、凭据与模型身份选择签名绑定作用域。
/// `route_default` is retained unless an inherited default or explicit model rule replaces it.
/// `route_default` 保留到继承默认或显式模型规则将其替换；显式空列表会清除该链。
///
/// # Returns
/// 返回值
/// The exact route baseline or compiled replacement, including an explicit empty replacement.
/// 返回精确路由基线或已编译替换链，包括显式空替换。
/// Missing compiled payloads return an explicit error.
/// 缺少已编译载荷时返回显式错误。
pub(super) fn resolve_response_projection_chain(
    manifest: &crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    route_default: Vec<crate::provider_manifest::ResponseProjectionProfile>,
) -> Result<Vec<crate::provider_manifest::ResponseProjectionProfile>, String> {
    let refs =
        effective_model_profile_refs(manifest, service_entry_id, credential_profile_id, model_id)?;
    if !refs.response_overrides_route_default {
        return Ok(route_default);
    }
    refs.response
        .iter()
        .map(|profile_id| {
            manifest
                .response_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.clone())
                .ok_or_else(|| {
                    format!(
                        "Model `{model_id}` references missing compiled response profile `{profile_id}`."
                    )
                })
        })
        .collect()
}

#[cfg(test)]
mod tests;
