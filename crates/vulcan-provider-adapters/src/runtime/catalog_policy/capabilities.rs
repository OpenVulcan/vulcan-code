//! Model capability defaults, explicit evidence precedence, and effective limit validation.
//! 模型能力默认、显式证据优先级与生效限制校验。

use crate::{ConnectionModelCatalog, PersistedAiConfiguration};
use vulcan_ai_contract::{AiOperation, CapabilitySupport, ModelCapabilities, ReasoningControlMode};

/// Identifies the host-owned default reasoning policy applied to unknown text models.
/// 标识应用于未知文本模型的宿主默认思考策略。
const HOST_DEFAULT_REASONING_EVIDENCE_REVISION: &str = "host-default-reasoning.v1";

/// Reports whether one model contract supports bidirectional text conversation.
/// 报告一个模型契约是否支持双向文本会话。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact provider-neutral model contract.
/// `capabilities` 是精确的供应商无关模型契约。
///
/// # Returns
/// 返回值
/// `true` only when the model explicitly exposes the conversation operation; text is the base
/// payload of that operation while resource vectors intentionally contain only non-text media.
/// 仅当模型显式公开会话操作时返回 `true`；文本是该操作的基础载荷，资源向量按契约仅保存非文本媒体。
pub fn model_supports_text_conversation(capabilities: &ModelCapabilities) -> bool {
    capabilities
        .operations
        .contains(&AiOperation::ConversationRespond)
}

/// Reports whether one model contract exposes an enabled reasoning capability to the host.
/// 报告一个模型契约是否向宿主公开已启用的思考能力。
///
/// # Parameters
/// 参数
/// `capabilities` is the provider-neutral capability contract after discovery and user overlays.
/// `capabilities` 是应用发现结果与用户覆盖后的供应商无关能力契约。
///
/// # Returns
/// 返回值
/// `true` when the model is a text conversation model whose reasoning contract is supported and
/// is not explicitly fixed off. Unknown text models receive the host's enabled default policy.
/// 当模型是文本会话模型、其思考契约受支持且没有明确固定关闭时返回 `true`；未知文本模型使用宿主的默认开启策略。
pub fn model_supports_reasoning(capabilities: &ModelCapabilities) -> bool {
    let mut effective = capabilities.clone();
    apply_host_default_reasoning_policy(&mut effective);
    effective
        .operations
        .contains(&AiOperation::ConversationRespond)
        && effective.reasoning.is_supported()
        && effective.reasoning_control != ReasoningControlMode::FixedOff
}

/// Applies the host default reasoning contract only to unknown text-conversation models.
/// 仅为能力未知的文本会话模型应用宿主默认思考契约。
///
/// # Parameters
/// 参数
///
/// `capabilities` is the fully merged model capability snapshot before model-level user overrides.
/// `capabilities` 是应用模型级用户覆盖前的完整合并模型能力快照。
///
/// # Returns
/// 返回值
///
/// `true` when the host policy filled an otherwise unknown reasoning contract.
/// 当宿主策略填充了原本未知的思考契约时返回 `true`。
pub fn apply_host_default_reasoning_policy(capabilities: &mut ModelCapabilities) -> bool {
    let is_text_conversation = capabilities
        .operations
        .contains(&AiOperation::ConversationRespond);
    // A text model remains unresolved when it has no selectable levels and no explicit control
    // mode. Unknown or legacy supported markers can use the host default; Unsupported/FixedOff
    // is an authoritative provider or user declaration and must never be silently re-enabled.
    // 文本模型没有可选档位且没有明确控制模式时仍未解析。未知或旧版支持标记可以使用宿主默认；
    // Unsupported/FixedOff 是供应商或用户的权威声明，绝不能被宿主默认静默重新开启。
    let is_unresolved_reasoning = capabilities.reasoning_efforts.is_empty()
        && capabilities.reasoning_control == ReasoningControlMode::Unknown
        && matches!(
            capabilities.reasoning,
            CapabilitySupport::Unknown
                | CapabilitySupport::Native
                | CapabilitySupport::Equivalent
                | CapabilitySupport::Lossy
        );
    if !is_text_conversation || !is_unresolved_reasoning {
        return false;
    }

    // Keep this standard set shared with custom compatible providers so the editor,
    // workbench selector, and execution resolver expose one closed vocabulary.
    // 与自定义兼容供应商共享这组标准档位，确保编辑器、工作台选择器和执行解析器使用同一封闭词表。
    capabilities.reasoning = CapabilitySupport::Native;
    capabilities.reasoning_control = ReasoningControlMode::EffortSelection;
    capabilities.reasoning_efforts = crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS.to_vec();
    capabilities.evidence_revision = format!(
        "{}+{}",
        capabilities.evidence_revision, HOST_DEFAULT_REASONING_EVIDENCE_REVISION
    );
    true
}

/// Builds the protocol-aware in-memory capability baseline for one exact protocol variant.
/// 为一个精确协议变体构建协议感知的内存能力基线。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` is the variant protocol; `None` retains the conservative compatible
/// default used before an exact connection is available.
/// `protocol_profile_id` 是变体协议；`None` 保留精确连接可用前使用的保守兼容默认值。
///
/// # Returns
/// 返回值
/// A baseline that preserves Gemini reasoning evidence and otherwise uses the compatible default.
/// 返回一份为 Gemini 保留思考证据、其他协议使用兼容默认值的基线。
pub(super) fn custom_provider_system_baseline_for_protocol(
    protocol_profile_id: Option<&str>,
) -> crate::CustomProviderModelDefaults {
    let mut baseline = crate::CustomProviderModelDefaults::default();
    if protocol_profile_id == Some(vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID) {
        baseline.reasoning = CapabilitySupport::Unknown;
        baseline.reasoning_control = ReasoningControlMode::Unknown;
        baseline.reasoning_efforts.clear();
    }
    baseline
}

/// Clamps one effective output ceiling to the model's effective context window.
/// 将一个有效输出上限收敛到模型的有效上下文窗口。
///
/// Discovery metadata is retained separately, while executable capability projections must use
/// a request-safe output ceiling when an upstream reports an inconsistent pair of limits.
/// 发现元数据仍然单独保留；当上游报告不一致的限制组合时，可执行能力投影必须使用安全的输出上限。
pub(super) fn clamp_output_limit_to_context_window(capabilities: &mut ModelCapabilities) {
    if let (Some(context_limit), Some(output_limit)) =
        (capabilities.context_limit, capabilities.output_limit)
        && output_limit > context_limit
    {
        capabilities.output_limit = Some(context_limit);
    }
}

/// Clamps every effective model and protocol variant in one projected catalog.
/// 收敛一个有效投影目录中的所有模型及协议变体。
///
/// The source catalog remains untouched so provider-reported metadata stays available for
/// diagnostics; only the post-policy executable projection is narrowed.
/// 原始目录保持不变以保留供应商报告的元数据供诊断使用；仅收窄应用策略后的可执行投影。
pub(super) fn clamp_catalog_output_limits_to_context_window(catalog: &mut ConnectionModelCatalog) {
    for model in &mut catalog.models {
        clamp_output_limit_to_context_window(&mut model.capabilities);
        for variant in model.protocol_variants.values_mut() {
            clamp_output_limit_to_context_window(&mut variant.capabilities);
        }
    }
}

/// Validates the final context and output limits resolved for one account model.
/// 校验一个账号模型最终解析得到的上下文与输出限制。
///
/// # Parameters
/// 参数
/// `connection_id` and `model_id` identify the exact projection, while `capabilities` contains
/// the fully resolved provider evidence, template/default values, and user differences.
/// `connection_id` 与 `model_id` 标识精确投影，`capabilities` 包含完整解析后的供应商证据、
/// 模板或默认值以及用户差异。
///
/// # Returns
/// 返回值
/// Success when every present limit is positive; inconsistent pairs are normalized before use.
/// 当所有已存在限制均为正数时返回成功；不一致组合会在使用前收窄。
pub(in crate::runtime) fn validate_resolved_model_capability_limits(
    connection_id: &str,
    model_id: &str,
    capabilities: &ModelCapabilities,
) -> Result<(), String> {
    if capabilities.context_limit == Some(0) {
        return Err(format!(
            "Model `{model_id}` on connection `{connection_id}` has an effective context limit of zero."
        ));
    }
    if capabilities.output_limit == Some(0) {
        return Err(format!(
            "Model `{model_id}` on connection `{connection_id}` has an effective output limit of zero."
        ));
    }
    Ok(())
}

/// Validates every final model limit in one credential-scoped catalog.
/// 校验一个凭据作用域目录中的每个最终模型限制。
///
/// # Parameters
/// 参数
/// `catalog` is a fully overlaid account projection.
/// `catalog` 是已经完整应用覆盖的账号投影。
///
/// # Returns
/// 返回值
/// Success only when every model has positive declared limits.
/// 仅当每个模型声明的限制均为正数时返回成功。
pub(in crate::runtime) fn validate_resolved_catalog_limits(
    catalog: &ConnectionModelCatalog,
) -> Result<(), String> {
    if catalog.models.len() > crate::PROVIDER_MODEL_CATALOG_MAX_MODELS {
        return Err(format!(
            "Connection `{}` exceeds the {}-model runtime catalog boundary.",
            catalog.connection_id,
            crate::PROVIDER_MODEL_CATALOG_MAX_MODELS
        ));
    }
    for model in &catalog.models {
        validate_resolved_model_capability_limits(
            catalog.connection_id.as_str(),
            model.id.as_str(),
            &model.capabilities,
        )?;
    }
    Ok(())
}

/// Applies only explicit advanced fields from a manual declaration to a discovered model.
/// 仅将手工声明中的显式高级字段应用到已发现模型。
///
/// # Parameters
/// 参数
/// `declared` is the user-owned advanced declaration and `resolved` retains provider-owned basics.
/// `declared` 是用户持有的高级声明，`resolved` 保留供应商持有的基础事实。
pub(super) fn apply_custom_model_advanced_declaration(
    declared: &ModelCapabilities,
    resolved: &mut ModelCapabilities,
) {
    if declared.reasoning != vulcan_ai_contract::CapabilitySupport::Unknown {
        resolved.reasoning = declared.reasoning;
    }
    if declared.reasoning_control != vulcan_ai_contract::ReasoningControlMode::Unknown {
        resolved.reasoning_control = declared.reasoning_control;
        resolved.reasoning_efforts = declared.reasoning_efforts.clone();
    } else if !declared.reasoning_efforts.is_empty() {
        resolved.reasoning_efforts = declared.reasoning_efforts.clone();
    }
    if declared.prompt_cache != vulcan_ai_contract::PromptCacheCapability::default() {
        resolved.prompt_cache = declared.prompt_cache.clone();
    }
    for parameter in &declared.model_parameters {
        if let Some(existing) = resolved
            .model_parameters
            .iter_mut()
            .find(|existing| existing.id == parameter.id)
        {
            *existing = parameter.clone();
        } else {
            resolved.model_parameters.push(parameter.clone());
        }
    }
    if declared.reasoning != vulcan_ai_contract::CapabilitySupport::Unknown
        || declared.reasoning_control != vulcan_ai_contract::ReasoningControlMode::Unknown
        || !declared.reasoning_efforts.is_empty()
        || declared.prompt_cache != vulcan_ai_contract::PromptCacheCapability::default()
        || !declared.model_parameters.is_empty()
    {
        resolved.evidence_revision = format!(
            "{}+{}",
            resolved.evidence_revision, declared.evidence_revision
        );
    }
}

/// Resolves custom-provider source fallbacks, system policy, and explicit provider settings.
/// 解析自定义供应商的来源回退、系统策略与显式供应商设置。
///
/// # Parameters
/// 参数
/// `configuration`, `provider_id`, and `model_id` identify persisted choices; `defaults` and
/// `capabilities` are the fallback contract and credential-scoped model snapshot.
/// `configuration`、`provider_id` 与 `model_id` 标识持久化选择；`defaults` 与
/// `capabilities` 分别是回退契约和凭据作用域模型快照。
pub(super) fn apply_custom_model_defaults(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
    defaults: &crate::CustomProviderModelDefaults,
    provider_settings: Option<&crate::CustomProviderModelDefaults>,
    evidence: &crate::ModelCapabilityEvidence,
    capabilities: &mut ModelCapabilities,
) {
    let selection = if let Some(crate::CustomModelTemplateDecision::Applied { selection }) =
        configuration.custom_model_template_decision(provider_id, model_id)
    {
        selection.apply_missing(capabilities, evidence);
        Some(selection)
    } else {
        None
    };
    defaults.apply_missing(capabilities, evidence);
    // The system policy fills only fields the catalog did not declare.
    // 系统策略只填充目录未声明的字段。
    defaults.apply_system_capability_policy(capabilities);
    if let Some(settings) = provider_settings {
        settings.apply_provider_settings(capabilities, evidence);
    }

    let existing_conversation = capabilities
        .operations
        .contains(&AiOperation::ConversationRespond);
    let has_explicit_non_conversation_operation = capabilities
        .operations
        .iter()
        .any(|operation| *operation != AiOperation::ConversationRespond);
    let implicit_text_default = existing_conversation || !has_explicit_non_conversation_operation;
    let supports_text_input = evidence
        .supports_text_input
        .or_else(|| selection.map(|value| value.supports_text_input))
        .unwrap_or(implicit_text_default);
    let supports_text_output = evidence
        .supports_text_output
        .or_else(|| selection.map(|value| value.supports_text_output))
        .unwrap_or(implicit_text_default);
    set_custom_conversation_contract(capabilities, supports_text_input && supports_text_output);
}

/// Captures the explicit basic-capability facts carried by one manual model definition.
/// 捕获一个手工模型定义携带的显式基础能力事实。
///
/// # Parameters
/// 参数
/// `capabilities` is the user-owned provider-neutral declaration.
/// `capabilities` 是用户持有的供应商无关能力声明。
///
/// # Returns
/// 返回值
/// Evidence that prevents provider defaults from replacing intentional manual values.
/// 返回用于阻止供应商默认值替换手工明确值的证据。
pub(super) fn manual_model_capability_evidence(
    capabilities: &ModelCapabilities,
) -> crate::ModelCapabilityEvidence {
    let supports_text = capabilities
        .operations
        .contains(&AiOperation::ConversationRespond);
    crate::ModelCapabilityEvidence {
        context_limit_declared: capabilities.context_limit.is_some(),
        output_limit_declared: capabilities.output_limit.is_some(),
        input_modalities_declared: true,
        output_modalities_declared: true,
        structured_output_declared: true,
        supports_text_input: Some(supports_text),
        supports_text_output: Some(supports_text),
    }
}

/// Closes one custom-provider model to the generic text-conversation contract.
/// 将一个自定义供应商模型收敛到通用文本会话契约。
///
/// # Parameters
/// 参数
/// `capabilities` is mutated in place and `supported` is the final bidirectional-text result.
/// `capabilities` 会被原地修改，`supported` 是最终双向文本支持结果。
fn set_custom_conversation_contract(capabilities: &mut ModelCapabilities, supported: bool) {
    capabilities.output_resources.clear();
    capabilities.operations.clear();
    if supported {
        capabilities
            .operations
            .push(AiOperation::ConversationRespond);
    }
}

#[cfg(test)]
mod tests;
