//! Exact internal-model override and reasoning-policy validation.
//! 精确内部模型覆盖与思考策略校验。

use crate::configured_inference::{
    VMM_CONSUMER_PROFILE_ID, model_route_supports_json_schema_contract,
    model_supports_disabled_reasoning, vmm_purpose_requires_disabled_reasoning,
};
use crate::permission_judge::PERMISSION_REVIEW_CONSUMER_PROFILE_ID;
use vulcan_ai_contract::{AiOperation, ModelCapabilities, ReasoningControlMode, ReasoningEffort};
use vulcan_provider_adapters::{
    InferenceRouteBinding, InferenceRouteConfiguration, InternalModelOverrides,
};

/// Validates one complete internal override against one exact model route.
/// 根据一条精确模型路由校验一份完整内部覆盖。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact model contract, `route` selects the operation/binding, and `overrides` carries user defaults.
/// `capabilities` 是精确模型契约，`route` 选择操作与绑定，`overrides` 携带用户默认值。
///
/// # Returns
/// 返回值
/// Returns success only for compatible parameter dependencies, limits, and operation-specific defaults.
/// 仅当参数依赖、上限和操作专属默认值兼容时返回成功。
pub(super) fn validate_internal_model_overrides(
    capabilities: &ModelCapabilities,
    route: &InferenceRouteConfiguration,
    overrides: &InternalModelOverrides,
) -> Result<(), String> {
    validate_internal_model_parameters(capabilities, route, overrides)?;
    match route.operation {
        AiOperation::ConversationRespond => {
            ensure_no_embedding_overrides(overrides)?;
            ensure_no_rerank_overrides(overrides)?;
            validate_positive_limit(
                "Internal context limit",
                overrides.context_limit,
                capabilities.context_limit,
            )?;
            validate_positive_limit(
                "Internal output limit",
                overrides.output_limit,
                capabilities.output_limit,
            )?;
            if let Some(effort) = overrides.reasoning_effort {
                validate_internal_reasoning_effort(capabilities, effort)?;
            }
        }
        AiOperation::EmbeddingCreate => {
            ensure_no_llm_overrides(overrides)?;
            ensure_no_rerank_overrides(overrides)?;
            let embedding = capabilities.embedding.as_ref().ok_or_else(|| {
                String::from("The selected route does not declare embedding capabilities.")
            })?;
            if let Some(task) = overrides.embedding_task
                && !embedding.tasks.contains(&task)
            {
                return Err(format!(
                    "Embedding task `{:?}` is not supported by the selected model.",
                    task
                ));
            }
            if let Some(dimensions) = overrides.embedding_dimensions
                && !embedding.supports_dimensions(dimensions)
            {
                return Err(format!(
                    "Embedding dimensions {dimensions} are not supported by the selected model."
                ));
            }
            if let Some(output_kind) = overrides.embedding_output_kind
                && !embedding.output_kinds.contains(&output_kind)
            {
                return Err(format!(
                    "Embedding output kind `{:?}` is not supported by the selected model.",
                    output_kind
                ));
            }
            if let Some(encoding) = overrides.embedding_encoding
                && !embedding.encodings.contains(&encoding)
            {
                return Err(format!(
                    "Embedding encoding `{:?}` is not supported by the selected model.",
                    encoding
                ));
            }
        }
        AiOperation::RerankDocuments => {
            ensure_no_llm_overrides(overrides)?;
            ensure_no_embedding_overrides(overrides)?;
            let rerank = capabilities.rerank.as_ref().ok_or_else(|| {
                String::from("The selected route does not declare rerank capabilities.")
            })?;
            if let Some(top_n) = overrides.rerank_top_n {
                if top_n == 0 {
                    return Err(String::from("Rerank top N must be greater than zero."));
                }
                if rerank
                    .maximum_candidates
                    .is_some_and(|maximum| top_n > maximum)
                {
                    return Err(format!(
                        "Rerank top N {top_n} exceeds the selected model limit."
                    ));
                }
            }
            if let Some(truncation) = overrides.rerank_truncation
                && !rerank.truncation.contains(&truncation)
            {
                return Err(format!(
                    "Rerank truncation `{:?}` is not supported by the selected model.",
                    truncation
                ));
            }
            if overrides.rerank_return_content == Some(true) && !rerank.supports_return_content {
                return Err(String::from(
                    "The selected rerank model cannot return candidate content.",
                ));
            }
        }
        _ => {
            return Err(format!(
                "Operation `{}` cannot own internal model overrides.",
                route.operation.as_str()
            ));
        }
    }
    Ok(())
}

/// Enforces the purpose-scoped VMM reasoning policy for one resolved route.
/// 对一条已解析路由强制执行用途级 VMM 思考策略。
///
/// # Parameters
/// 参数
/// `consumer_profile_id`, `purpose_id`, and `operation` identify the profile slot;
/// `capabilities` and `reasoning_effort` are the exact model contract and requested level.
/// `consumer_profile_id`、`purpose_id` 与 `operation` 标识配置档槽位；
/// `capabilities` 和 `reasoning_effort` 是准确模型契约与请求档位。
///
/// # Returns
/// 返回值
/// Success outside VMM LLM slots or when the selected effort satisfies the exact purpose policy.
/// 非 VMM LLM 槽位或所选档位满足准确用途策略时返回成功。
pub(super) fn validate_vmm_llm_reasoning_policy(
    consumer_profile_id: &str,
    purpose_id: Option<&str>,
    operation: AiOperation,
    capabilities: &ModelCapabilities,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<(), String> {
    if operation != AiOperation::ConversationRespond {
        return Ok(());
    }
    // The permission reviewer may think; an explicit effort must exist in the model contract.
    // 权限复核器可以思考；显式档位必须存在于模型契约中。
    if consumer_profile_id == PERMISSION_REVIEW_CONSUMER_PROFILE_ID {
        return match reasoning_effort {
            Some(effort) if !reasoning_effort_supported(capabilities, effort) => Err(format!(
                "The permission reviewer cannot use reasoning effort `{}` with the selected model contract.",
                effort.as_str()
            )),
            _ => Ok(()),
        };
    }
    // The session title generator may think too, with the same contract check.
    // 会话标题生成器同样可以思考，并做相同的契约检查。
    if consumer_profile_id == crate::session_title::SESSION_TITLE_CONSUMER_PROFILE_ID {
        return match reasoning_effort {
            Some(effort) if !reasoning_effort_supported(capabilities, effort) => Err(format!(
                "The session title generator cannot use reasoning effort `{}` with the selected model contract.",
                effort.as_str()
            )),
            _ => Ok(()),
        };
    }
    if consumer_profile_id != VMM_CONSUMER_PROFILE_ID {
        return Ok(());
    }
    let purpose_id = purpose_id
        .ok_or_else(|| String::from("VMM LLM routes require one exact purpose identifier."))?;
    let reasoning_effort = reasoning_effort.unwrap_or(ReasoningEffort::None);
    if vmm_purpose_requires_disabled_reasoning(consumer_profile_id, purpose_id)
        && (reasoning_effort != ReasoningEffort::None
            || !model_supports_disabled_reasoning(capabilities))
    {
        return Err(String::from(
            "VMM LLM routes require a model and purpose setting that explicitly disable reasoning.",
        ));
    }
    if reasoning_effort_supported(capabilities, reasoning_effort) {
        return Ok(());
    }
    Err(format!(
        "VMM purpose `{purpose_id}` cannot use reasoning effort `{}` with the selected model contract.",
        reasoning_effort.as_str()
    ))
}

/// Enforces the provider JSON contract required by every native VMM and permission-review LLM route.
/// 对每条原生 VMM 与权限复核 LLM 路由强制执行提供方 JSON 契约。
///
/// # Parameters
/// 参数
/// `consumer_profile_id` and `operation` identify the profile slot; `route` and `capabilities`
/// provide the exact protocol and model authorities.
/// `consumer_profile_id` 与 `operation` 标识配置档槽位；`route` 与 `capabilities`
/// 提供准确协议与模型权威。
///
/// # Returns
/// 返回值
/// Success outside VMM LLM slots or when provider-side JSON control remains enforceable.
/// 非 VMM LLM 槽位或提供方侧 JSON 控制仍可强制时返回成功。
pub(super) fn validate_vmm_llm_structured_output_policy(
    consumer_profile_id: &str,
    operation: AiOperation,
    route: &InferenceRouteConfiguration,
    capabilities: &ModelCapabilities,
) -> Result<(), String> {
    // The permission reviewer answers through the same validated JSON Schema path as VMM.
    // 权限复核器与 VMM 一样通过已校验的 JSON Schema 路径作答。
    if (consumer_profile_id != VMM_CONSUMER_PROFILE_ID
        && consumer_profile_id != PERMISSION_REVIEW_CONSUMER_PROFILE_ID)
        || operation != AiOperation::ConversationRespond
    {
        return Ok(());
    }
    let InferenceRouteBinding::Protocol {
        protocol_profile_id,
    } = &route.binding
    else {
        return Err(String::from(
            "Structured-output LLM routes require one exact protocol binding.",
        ));
    };
    if model_route_supports_json_schema_contract(protocol_profile_id, capabilities) {
        return Ok(());
    }
    Err(String::from(
        "Structured-output LLM routes require a model and protocol that preserve provider-side JSON output and host-side Schema validation.",
    ))
}

/// Reports whether one exact model contract accepts one reasoning effort.
/// 判断一份准确模型契约是否接受一个思考档位。
///
/// # Parameters
/// 参数
/// `capabilities` is the frozen model contract and `reasoning_effort` is the requested level.
/// `capabilities` 是冻结模型契约，`reasoning_effort` 是请求档位。
///
/// # Returns
/// 返回值
/// `true` only when the model control mode and declared effort set accept the request.
/// 仅当模型控制模式与声明档位集合接受该请求时返回 `true`。
fn reasoning_effort_supported(
    capabilities: &ModelCapabilities,
    reasoning_effort: ReasoningEffort,
) -> bool {
    match capabilities.reasoning_control {
        ReasoningControlMode::FixedOff => reasoning_effort == ReasoningEffort::None,
        ReasoningControlMode::FixedOn => {
            reasoning_effort != ReasoningEffort::None
                && capabilities.reasoning_efforts.contains(&reasoning_effort)
        }
        ReasoningControlMode::Toggle
        | ReasoningControlMode::EffortSelection
        | ReasoningControlMode::ProviderAdaptive => {
            capabilities.reasoning_efforts.contains(&reasoning_effort)
        }
        ReasoningControlMode::Unknown => false,
    }
}

/// Validates registered model parameters and their dependency graph for one exact route.
/// 校验一条精确路由已注册的模型参数及其依赖图。
///
/// # Parameters
/// 参数
/// `capabilities`, `route`, and `overrides` bind each parameter to the selected model and protocol.
/// `capabilities`、`route` 与 `overrides` 将每项参数绑定到所选模型与协议。
///
/// # Returns
/// 返回值
/// Returns success when all declared parameter values, required peers, and conflicts are valid.
/// 当全部声明参数值、依赖项和互斥关系有效时返回成功。
fn validate_internal_model_parameters(
    capabilities: &ModelCapabilities,
    route: &InferenceRouteConfiguration,
    overrides: &InternalModelOverrides,
) -> Result<(), String> {
    let protocol_id = match &route.binding {
        InferenceRouteBinding::Protocol {
            protocol_profile_id,
        } => Some(protocol_profile_id.as_str()),
        InferenceRouteBinding::Action { .. } => None,
    };
    for (parameter_id, value) in &overrides.model_parameters {
        let definition = capabilities
            .model_parameters
            .iter()
            .find(|definition| {
                definition.id == *parameter_id
                    && (definition.model_ids.is_empty()
                        || definition.model_ids.contains(&route.model_id))
                    && (definition.protocol_ids.is_empty()
                        || protocol_id.is_some_and(|protocol_id| {
                            definition
                                .protocol_ids
                                .iter()
                                .any(|candidate| candidate == protocol_id)
                        }))
            })
            .ok_or_else(|| {
                format!(
                    "Model parameter `{parameter_id}` is not registered for the selected route."
                )
            })?;
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

/// Validates one positive local limit against an exact provider maximum.
/// 根据精确供应商上限校验一个正数本地限制。
///
/// # Parameters
/// 参数
/// `label` names the diagnostic, `requested` is the optional user limit, and `maximum` is the declared provider bound.
/// `label` 标识诊断，`requested` 是可选用户上限，`maximum` 是供应商声明边界。
///
/// # Returns
/// 返回值
/// Returns success for an absent limit or a positive value within the explicitly known maximum.
/// 未提供上限，或正数值不超过明确已知上限时返回成功。
fn validate_positive_limit(
    label: &str,
    requested: Option<u64>,
    maximum: Option<u64>,
) -> Result<(), String> {
    let Some(requested) = requested else {
        return Ok(());
    };
    if requested == 0 {
        return Err(format!("{label} must be greater than zero."));
    }
    let maximum = maximum.ok_or_else(|| {
        format!("{label} cannot be overridden because the provider did not declare a maximum.")
    })?;
    if requested > maximum {
        return Err(format!(
            "{label} {requested} exceeds the provider maximum {maximum}."
        ));
    }
    Ok(())
}

/// Validates one explicit internal reasoning default against exact control metadata.
/// 根据精确控制元数据校验一个显式内部推理默认值。
///
/// # Parameters
/// 参数
/// `capabilities` supplies the exact reasoning control mode and declared efforts; `effort` is the explicit choice.
/// `capabilities` 提供精确思考控制模式与声明档位，`effort` 是显式选择。
///
/// # Returns
/// 返回值
/// Returns success only when that control mode admits the requested effort.
/// 仅当该控制模式允许请求档位时返回成功。
pub(super) fn validate_internal_reasoning_effort(
    capabilities: &ModelCapabilities,
    effort: ReasoningEffort,
) -> Result<(), String> {
    let valid = match capabilities.reasoning_control {
        ReasoningControlMode::FixedOff => effort == ReasoningEffort::None,
        ReasoningControlMode::FixedOn => {
            effort != ReasoningEffort::None && capabilities.reasoning_efforts.contains(&effort)
        }
        ReasoningControlMode::Toggle
        | ReasoningControlMode::EffortSelection
        | ReasoningControlMode::ProviderAdaptive => {
            capabilities.reasoning_efforts.contains(&effort)
        }
        ReasoningControlMode::Unknown => false,
    };
    valid.then_some(()).ok_or_else(|| {
        format!(
            "Reasoning effort `{}` is not declared by the selected model route.",
            effort.as_str()
        )
    })
}

/// Rejects LLM-only fields on non-LLM operations.
/// 拒绝非 LLM 操作上的 LLM 专属字段。
///
/// # Parameters
/// 参数
/// `overrides` is the shared default set being validated for a non-LLM operation.
/// `overrides` 是正在为非LLM操作校验的共享默认值集合。
///
/// # Returns
/// 返回值
/// Returns an error if any LLM-only limit or reasoning choice is present.
/// 存在任意LLM专属上限或思考选择时返回错误。
fn ensure_no_llm_overrides(overrides: &InternalModelOverrides) -> Result<(), String> {
    if overrides.context_limit.is_some()
        || overrides.output_limit.is_some()
        || overrides.reasoning_effort.is_some()
    {
        return Err(String::from(
            "LLM limits and reasoning defaults cannot be saved for this operation.",
        ));
    }
    Ok(())
}

/// Rejects embedding-only fields on other operations.
/// 拒绝其他操作上的向量专属字段。
///
/// # Parameters
/// 参数
/// `overrides` is the shared default set being validated outside embedding execution.
/// `overrides` 是正在向量执行以外场景中校验的共享默认值集合。
///
/// # Returns
/// 返回值
/// Returns an error if any embedding-only field is present.
/// 存在任意向量专属字段时返回错误。
fn ensure_no_embedding_overrides(overrides: &InternalModelOverrides) -> Result<(), String> {
    if overrides.embedding_task.is_some()
        || overrides.embedding_dimensions.is_some()
        || overrides.embedding_output_kind.is_some()
        || overrides.embedding_encoding.is_some()
    {
        return Err(String::from(
            "Embedding defaults cannot be saved for this operation.",
        ));
    }
    Ok(())
}

/// Rejects rerank-only fields on other operations.
/// 拒绝其他操作上的重排专属字段。
///
/// # Parameters
/// 参数
/// `overrides` is the shared default set being validated outside rerank execution.
/// `overrides` 是正在重排执行以外场景中校验的共享默认值集合。
///
/// # Returns
/// 返回值
/// Returns an error if any rerank-only field is present.
/// 存在任意重排专属字段时返回错误。
fn ensure_no_rerank_overrides(overrides: &InternalModelOverrides) -> Result<(), String> {
    if overrides.rerank_top_n.is_some()
        || overrides.rerank_truncation.is_some()
        || overrides.rerank_return_content.is_some()
    {
        return Err(String::from(
            "Rerank defaults cannot be saved for this operation.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
