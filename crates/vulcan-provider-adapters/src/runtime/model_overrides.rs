//! Shared user request projections, explicit parameter defaults, and model-override validation.
//! 共用用户请求投影、显式参数默认值与模型覆盖校验。

use crate::PersistedAiConfiguration;
use vulcan_ai_contract::PromptCachePolicy;

/// Resolves provider and model request projections from low to high precedence.
/// 按从低到高的优先级解析供应商与模型请求投影。
///
/// # Parameters
/// 参数
/// `configuration` owns sparse layers, while `provider_id` and `model_id` identify the route.
/// `configuration` 持有稀疏配置层，`provider_id` 与 `model_id` 标识目标路由。
///
/// # Returns
/// 返回值
/// A merged projection, or `None` when no layer declares one.
/// 返回合并后的投影；没有任何层声明投影时返回 `None`。
pub(super) fn resolve_request_projection(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<crate::CustomModelRequestProjection>, String> {
    // System-provider request projection is immutable Manifest data and cannot be supplied by user configuration.
    // 系统供应商请求投影属于不可变 Manifest 数据，不能由用户配置提供。
    if !provider_id.starts_with("custom:") {
        return Ok(None);
    }
    let mut merged = crate::CustomModelRequestProjection::default();
    let mut found = false;

    // Custom-provider defaults are the only provider-wide user projection layer.
    // 自定义供应商默认值是唯一的供应商级用户投影层。
    if let Some(projection) = configuration
        .custom_provider_model_defaults
        .get(provider_id)
        .and_then(|defaults| defaults.request_projection.as_ref())
    {
        merged.merge_overlay(projection)?;
        found = true;
    }
    if let Some(projection) = configuration
        .model_capability_overrides(provider_id, model_id)
        .and_then(|overrides| overrides.request_projection.as_ref())
    {
        merged.merge_overlay(projection)?;
        found = true;
    }
    if let Some(definition) = configuration.custom_model(provider_id, model_id) {
        merged.merge_overlay(&definition.request_projection)?;
        found = true;
    }
    Ok(found.then_some(merged))
}

/// Applies closed adapter-declared defaults before freezing an execution selection.
/// 在冻结执行选择前应用适配器声明的封闭默认值。
///
/// # Parameters
/// 参数
/// `model` is the exact discovered model, `protocol_id` is the selected route,
/// and `overrides` receives only applicable explicit defaults.
/// `model` 是精确发现的模型，`protocol_id` 是所选路由，
/// `overrides` 仅接收适用的显式默认值。
///
/// # Returns
/// 返回值
/// Success when every applicable required parameter is present.
/// 当每个适用的必填参数都存在时返回成功。
pub(super) fn apply_model_parameter_defaults(
    model: &crate::ConnectionModel,
    protocol_id: &str,
    overrides: &mut crate::ModelExecutionOverrides,
) -> Result<(), String> {
    for definition in &model.capabilities.model_parameters {
        let applies_to_protocol = definition.protocol_ids.is_empty()
            || definition.protocol_ids.iter().any(|id| id == protocol_id);
        let applies_to_model = definition.model_ids.is_empty()
            || definition.model_ids.iter().any(|id| id == &model.id);
        if !applies_to_protocol
            || !applies_to_model
            || overrides.model_parameters.contains_key(&definition.id)
        {
            continue;
        }
        match &definition.default {
            vulcan_ai_contract::ModelParameterDefault::ExplicitValue { value } => {
                overrides
                    .model_parameters
                    .insert(definition.id.clone(), value.clone());
            }
            vulcan_ai_contract::ModelParameterDefault::RequiredValue {} => {
                return Err(format!(
                    "Model parameter `{}` requires an explicit value for model `{}`.",
                    definition.id, model.id
                ));
            }
            vulcan_ai_contract::ModelParameterDefault::Omit {}
            | vulcan_ai_contract::ModelParameterDefault::ProviderDefault {} => {}
        }
    }
    Ok(())
}

/// Validates local limit overrides, cache policy, and closed model parameters.
/// 校验本地限制覆盖、缓存策略与封闭模型参数。
///
/// # Parameters
/// 参数
/// `model` owns the exact capabilities, `protocol_id` scopes parameters, and `overrides` is the user layer.
/// `model` 持有精确能力，`protocol_id` 限定参数范围，`overrides` 是用户层。
///
/// # Returns
/// 返回值
/// Success only when declared limits, cache policy, dependencies, and conflicts are satisfied.
/// 仅在满足声明上限、缓存策略、依赖与冲突约束时返回成功。
pub(super) fn validate_model_overrides(
    model: &crate::ConnectionModel,
    protocol_id: &str,
    overrides: &crate::ModelExecutionOverrides,
) -> Result<(), String> {
    if let Some(context_limit) = overrides.context_limit {
        if context_limit == 0 {
            return Err(format!(
                "Model `{}` context override must be greater than zero.",
                model.id
            ));
        }
        if model
            .capabilities
            .context_limit
            .is_some_and(|advertised| context_limit > advertised)
        {
            return Err(format!(
                "Model `{}` context override exceeds its discovered limit.",
                model.id
            ));
        }
    }
    if let Some(output_limit) = overrides.output_limit {
        if output_limit == 0 {
            return Err(format!(
                "Model `{}` output override must be greater than zero.",
                model.id
            ));
        }
        if model
            .capabilities
            .output_limit
            .is_some_and(|advertised| output_limit > advertised)
        {
            return Err(format!(
                "Model `{}` output override exceeds its discovered limit.",
                model.id
            ));
        }
        let effective_context_limit = overrides.context_limit.or(model.capabilities.context_limit);
        if effective_context_limit.is_some_and(|context_limit| output_limit > context_limit) {
            return Err(format!(
                "Model `{}` output override exceeds its effective context limit.",
                model.id
            ));
        }
    }
    let cache_policy = overrides
        .prompt_cache
        .as_ref()
        .unwrap_or(&PromptCachePolicy::ProviderDefault);
    model
        .capabilities
        .prompt_cache
        .validate_policy(cache_policy)?;
    for (parameter_id, value) in &overrides.model_parameters {
        let definition = model
            .capabilities
            .model_parameters
            .iter()
            .find(|definition| definition.id == *parameter_id)
            .ok_or_else(|| {
                format!(
                    "Model parameter `{parameter_id}` is not registered for model `{}`.",
                    model.id
                )
            })?;
        if !definition.protocol_ids.is_empty()
            && !definition.protocol_ids.iter().any(|id| id == protocol_id)
        {
            return Err(format!(
                "Model parameter `{parameter_id}` is not registered for protocol `{protocol_id}`."
            ));
        }
        if !definition.model_ids.is_empty()
            && !definition.model_ids.iter().any(|id| id == &model.id)
        {
            return Err(format!(
                "Model parameter `{parameter_id}` is not registered for model `{}`.",
                model.id
            ));
        }
        definition.validate_value(value)?;
        for dependency in &definition.dependent_required {
            if !overrides.model_parameters.contains_key(dependency) {
                return Err(format!(
                    "Model parameter `{parameter_id}` requires `{dependency}`."
                ));
            }
        }
        for conflict in &definition.conflicts {
            if overrides.model_parameters.contains_key(conflict) {
                return Err(format!(
                    "Model parameter `{parameter_id}` conflicts with `{conflict}`."
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
