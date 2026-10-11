//! Provider-neutral reasoning preference.
//! 供应商无关的推理偏好。

use crate::provider_config::ReasoningEffortLevel;
use vulcan_ai_contract::{ModelCapabilities, ReasoningEffort};

/// Resolves one explicit preference or the model's concrete enabled default.
/// 解析一项显式偏好，或返回模型具体的默认启用档位。
///
/// # Parameters
/// 参数
///
/// `capabilities` is the exact resolved model contract and `requested_effort` is the optional
/// user preference before model compatibility is applied.
/// `capabilities` 是精确解析后的模型契约，`requested_effort` 是应用模型兼容性前的可选用户偏好。
///
/// # Returns
/// 返回值
///
/// A supported explicit effort, otherwise adaptive, medium, the first enabled effort, the only
/// disabled effort, or the provider-default sentinel when no effort contract exists.
/// 返回受支持的显式档位；否则依次返回自适应、中等、首个启用档位、唯一关闭档位；若不存在档位契约则返回供应商默认哨兵。
pub fn resolve_effective_reasoning_effort(
    capabilities: &ModelCapabilities,
    requested_effort: Option<ReasoningEffortLevel>,
) -> ReasoningEffortLevel {
    // Reapply the provider-runtime host default so direct callers cannot diverge from catalog resolution.
    // 重新应用供应商运行时的宿主默认策略，避免直接调用方与目录解析产生分叉。
    let mut effective_capabilities = capabilities.clone();
    vulcan_provider_adapters::apply_host_default_reasoning_policy(&mut effective_capabilities);
    let capabilities = &effective_capabilities;
    // Explicit unsupported/fixed-off metadata outranks stale effort entries from an older catalog snapshot.
    // 明确的不支持/固定关闭元数据优先于旧目录快照中残留的推理档位。
    if capabilities.reasoning == vulcan_ai_contract::CapabilitySupport::Unsupported
        || capabilities.reasoning_control == vulcan_ai_contract::ReasoningControlMode::FixedOff
    {
        return ReasoningEffortLevel::None;
    }
    // supportedEfforts is the closed model-owned effort set used by both UI projection and dispatch.
    // supportedEfforts 是界面投影与分发共同使用的模型自有封闭档位集合。
    let supported_efforts = capabilities.reasoning_efforts.as_slice();
    if supported_efforts.is_empty() {
        return ReasoningEffortLevel::Off;
    }

    // requestedPublicEffort converts only concrete public levels; Off means no explicit level.
    // requestedPublicEffort 仅转换具体公共档位；Off 表示没有显式档位。
    let requested_public_effort = requested_effort.and_then(public_reasoning_effort_from_level);
    if let Some(requested_public_effort) = requested_public_effort
        && supported_efforts.contains(&requested_public_effort)
    {
        return reasoning_effort_level_from_public(requested_public_effort);
    }

    // preferredEffort keeps default reasoning enabled while preferring provider-neutral adaptive or OpenAI medium.
    // preferredEffort 在优先采用供应商无关自适应或 OpenAI 中等档位时保持默认思考开启。
    let preferred_effort = [ReasoningEffort::Adaptive, ReasoningEffort::Medium]
        .into_iter()
        .find(|candidate| supported_efforts.contains(candidate))
        .or_else(|| {
            supported_efforts
                .iter()
                .copied()
                .find(|effort| *effort != ReasoningEffort::None)
        })
        .or_else(|| supported_efforts.first().copied());
    preferred_effort
        .map(reasoning_effort_level_from_public)
        .unwrap_or(ReasoningEffortLevel::Off)
}

/// Resolves the final provider-neutral wire effort for a public or restricted LLM request.
/// 为公共或受限 LLM 请求解析最终供应商无关 Wire 思考档位。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact model contract and `requested_effort` is the optional host choice.
/// `capabilities` 是精确模型契约，`requested_effort` 是可选的宿主选择。
///
/// # Returns
/// 返回值
/// A wire effort when reasoning is active, or `None` when the contract is explicitly disabled.
/// 思考处于启用状态时返回 Wire 档位；契约明确关闭时返回 `None`。
pub fn resolve_effective_wire_reasoning_effort(
    capabilities: &ModelCapabilities,
    requested_effort: Option<ReasoningEffortLevel>,
) -> Option<ReasoningEffort> {
    let mut effective_capabilities = capabilities.clone();
    vulcan_provider_adapters::apply_host_default_reasoning_policy(&mut effective_capabilities);
    // An explicit unsupported or fixed-off contract means the provider default is already off;
    // do not emit a synthetic `reasoning_effort=none` field on providers that reject it.
    // 明确不支持或固定关闭表示供应商默认已经关闭；不要在可能拒绝该字段的供应商上
    // 人为发送 `reasoning_effort=none`。
    if effective_capabilities.reasoning == vulcan_ai_contract::CapabilitySupport::Unsupported
        || effective_capabilities.reasoning_control
            == vulcan_ai_contract::ReasoningControlMode::FixedOff
    {
        return None;
    }
    let effective_effort =
        resolve_effective_reasoning_effort(&effective_capabilities, requested_effort);
    reasoning_effort_from_level(effective_effort)
}

/// Validates and converts an already selected reasoning level without choosing another level.
/// 校验并转换已经选定的思考档位，绝不选择其他档位。
///
/// # Parameters / 参数
/// `capabilities` is the exact frozen model contract and `requested_effort` is session authority.
/// `capabilities` 是精确冻结的模型契约，`requested_effort` 是会话权威值。
///
/// # Returns / 返回值
/// Returns the exact wire value, omission for the provider-default sentinel, or an explicit
/// incompatibility error. This function never falls back to a nearby level.
/// 返回精确 Wire 值、供应商默认哨兵对应的省略值，或明确的不兼容错误；本函数绝不回退到邻近档位。
pub fn validate_exact_wire_reasoning_effort(
    capabilities: &ModelCapabilities,
    requested_effort: ReasoningEffortLevel,
) -> Result<Option<ReasoningEffort>, String> {
    let mut effective_capabilities = capabilities.clone();
    vulcan_provider_adapters::apply_host_default_reasoning_policy(&mut effective_capabilities);
    if requested_effort == ReasoningEffortLevel::Off {
        return Ok(None);
    }
    if effective_capabilities.reasoning == vulcan_ai_contract::CapabilitySupport::Unsupported
        || effective_capabilities.reasoning_control
            == vulcan_ai_contract::ReasoningControlMode::FixedOff
    {
        return (requested_effort == ReasoningEffortLevel::None)
            .then_some(None)
            .ok_or_else(|| {
                format!(
                    "Selected reasoning effort `{}` is not supported by this model.",
                    requested_effort.compact_label()
                )
            });
    }
    let requested_public = public_reasoning_effort_from_level(requested_effort)
        .ok_or_else(|| String::from("Provider-default reasoning has no explicit wire value."))?;
    if !effective_capabilities
        .reasoning_efforts
        .contains(&requested_public)
    {
        return Err(format!(
            "Selected reasoning effort `{}` is not supported by this model.",
            requested_effort.compact_label()
        ));
    }
    Ok(Some(requested_public))
}

/// Resolves a restricted request that already stores a public reasoning effort.
/// 解析已经保存公共思考档位的受限请求。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact model contract and `requested_effort` preserves explicit `none`.
/// `capabilities` 是精确模型契约，`requested_effort` 保留显式 `none` 关闭选择。
///
/// # Returns
/// 返回值
/// The final provider-neutral wire effort or no field for an explicitly disabled contract.
/// 最终供应商无关 Wire 档位；契约明确关闭时不发送该字段。
pub fn resolve_effective_public_reasoning_effort(
    capabilities: &ModelCapabilities,
    requested_effort: Option<ReasoningEffort>,
) -> Option<ReasoningEffort> {
    resolve_effective_wire_reasoning_effort(
        capabilities,
        requested_effort.map(reasoning_effort_level_from_public),
    )
}

/// Converts a resolved workbench effort into the public wire domain.
/// 将已解析的工作台思考档位转换为公共 Wire 领域。
fn reasoning_effort_from_level(effort: ReasoningEffortLevel) -> Option<ReasoningEffort> {
    match effort {
        ReasoningEffortLevel::Off => None,
        ReasoningEffortLevel::None => Some(ReasoningEffort::None),
        ReasoningEffortLevel::Adaptive => Some(ReasoningEffort::Adaptive),
        ReasoningEffortLevel::Minimal => Some(ReasoningEffort::Minimal),
        ReasoningEffortLevel::Low => Some(ReasoningEffort::Low),
        ReasoningEffortLevel::Medium => Some(ReasoningEffort::Medium),
        ReasoningEffortLevel::High => Some(ReasoningEffort::High),
        ReasoningEffortLevel::XHigh => Some(ReasoningEffort::XHigh),
        ReasoningEffortLevel::Max => Some(ReasoningEffort::Max),
        ReasoningEffortLevel::Ultra => Some(ReasoningEffort::Ultra),
    }
}

/// Converts one concrete workbench effort into the public capability domain.
/// 将一个具体工作台档位转换到公共能力域。
///
/// # Parameters
/// 参数
///
/// `effort` is the workbench preference to convert.
/// `effort` 是要转换的工作台偏好。
///
/// # Returns
/// 返回值
///
/// The matching public effort, or `None` for the provider-default `Off` sentinel.
/// 返回匹配的公共档位；供应商默认 `Off` 哨兵则返回 `None`。
pub(crate) fn public_reasoning_effort_from_level(
    effort: ReasoningEffortLevel,
) -> Option<ReasoningEffort> {
    match effort {
        ReasoningEffortLevel::Off => None,
        ReasoningEffortLevel::None => Some(ReasoningEffort::None),
        ReasoningEffortLevel::Adaptive => Some(ReasoningEffort::Adaptive),
        ReasoningEffortLevel::Minimal => Some(ReasoningEffort::Minimal),
        ReasoningEffortLevel::Low => Some(ReasoningEffort::Low),
        ReasoningEffortLevel::Medium => Some(ReasoningEffort::Medium),
        ReasoningEffortLevel::High => Some(ReasoningEffort::High),
        ReasoningEffortLevel::XHigh => Some(ReasoningEffort::XHigh),
        ReasoningEffortLevel::Max => Some(ReasoningEffort::Max),
        ReasoningEffortLevel::Ultra => Some(ReasoningEffort::Ultra),
    }
}

/// Converts one public capability effort into the workbench preference domain.
/// 将一个公共能力档位转换到工作台偏好域。
///
/// # Parameters
/// 参数
///
/// `effort` is the public model effort to convert.
/// `effort` 是要转换的公共模型档位。
///
/// # Returns
/// 返回值
///
/// The matching workbench effort.
/// 返回匹配的工作台档位。
fn reasoning_effort_level_from_public(effort: ReasoningEffort) -> ReasoningEffortLevel {
    match effort {
        ReasoningEffort::None => ReasoningEffortLevel::None,
        ReasoningEffort::Adaptive => ReasoningEffortLevel::Adaptive,
        ReasoningEffort::Minimal => ReasoningEffortLevel::Minimal,
        ReasoningEffort::Low => ReasoningEffortLevel::Low,
        ReasoningEffort::Medium => ReasoningEffortLevel::Medium,
        ReasoningEffort::High => ReasoningEffortLevel::High,
        ReasoningEffort::XHigh => ReasoningEffortLevel::XHigh,
        ReasoningEffort::Max => ReasoningEffortLevel::Max,
        ReasoningEffort::Ultra => ReasoningEffortLevel::Ultra,
    }
}

/// Captures the host reasoning preference attached to one immutable public execution.
/// 捕获附着到一次不可变公共执行的宿主推理偏好。
#[derive(Debug, Clone)]
pub struct ProviderRuntimeReasoningResolution {
    /// Stores the workbench reasoning effort requested by the user.
    /// 保存用户通过工作台请求的推理强度。
    pub requested_effort: ReasoningEffortLevel,
    /// Describes the public configuration layer that supplied this preference.
    /// 描述提供该偏好的公共配置层。
    pub source_label: String,
    /// Stores an optional diagnostic note without changing request routing.
    /// 保存不改变请求路由的可选诊断说明。
    pub note: Option<String>,
}

impl Default for ProviderRuntimeReasoningResolution {
    /// Builds a preference that leaves all provider behavior unchanged.
    /// 构建一个不改变任何供应商行为的偏好。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A provider-default reasoning resolution.
    /// 返回遵循供应商默认值的推理解析结果。
    fn default() -> Self {
        Self {
            requested_effort: ReasoningEffortLevel::Off,
            source_label: String::from("public contract default"),
            note: None,
        }
    }
}

impl ProviderRuntimeReasoningResolution {
    /// Returns one compact human-readable diagnostic label.
    /// 返回一个紧凑的人类可读诊断标签。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The selected effort, source, and optional note.
    /// 返回选定强度、来源与可选说明。
    pub fn summary_label(&self) -> String {
        let mut label = format!(
            "Reasoning: {} · {}",
            self.requested_effort.compact_label(),
            self.source_label
        );
        if let Some(note) = self
            .note
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            label.push_str(" · ");
            label.push_str(note);
        }
        label
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_ai_contract::{CapabilitySupport, ReasoningControlMode};

    /// Builds the exact adjustable reasoning contract shared by resolver tests.
    /// 构建解析器测试共用的精确可调思考契约。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A model capability set with OpenAI-compatible effort levels.
    /// 返回包含 OpenAI 兼容档位的模型能力集合。
    fn openai_reasoning_capabilities() -> ModelCapabilities {
        ModelCapabilities {
            reasoning: CapabilitySupport::Native,
            reasoning_control: ReasoningControlMode::EffortSelection,
            reasoning_efforts: vec![
                ReasoningEffort::None,
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
                ReasoningEffort::Max,
            ],
            ..ModelCapabilities::default()
        }
    }

    /// Verifies an absent or legacy Off preference resolves to enabled OpenAI medium.
    /// 验证缺省或旧版 Off 偏好会解析为已启用的 OpenAI 中等档位。
    #[test]
    fn missing_preference_defaults_to_enabled_openai_medium() {
        // capabilities owns the exact effort order whose disabled first entry must not become the default.
        // capabilities 持有精确档位顺序，其首个关闭项不得成为默认值。
        let capabilities = openai_reasoning_capabilities();

        assert_eq!(
            resolve_effective_reasoning_effort(&capabilities, None),
            ReasoningEffortLevel::Medium
        );
        assert_eq!(
            resolve_effective_reasoning_effort(&capabilities, Some(ReasoningEffortLevel::Off)),
            ReasoningEffortLevel::Medium
        );
    }

    /// Verifies the shared host default also applies when a compatible conversation model is unknown.
    /// 验证兼容文本会话模型未知时也会应用共享宿主默认策略。
    #[test]
    fn unknown_text_conversation_defaults_to_enabled_medium() {
        let capabilities = ModelCapabilities {
            operations: vec![vulcan_ai_contract::AiOperation::ConversationRespond],
            ..ModelCapabilities::default()
        };

        assert_eq!(
            resolve_effective_reasoning_effort(&capabilities, None),
            ReasoningEffortLevel::Medium
        );
    }

    /// Verifies a legacy Native-only text contract does not fall back to disabled reasoning.
    /// 验证只有 Native 支持标记的旧文本契约不会回退为关闭思考。
    #[test]
    fn native_only_text_conversation_defaults_to_enabled_medium() {
        let capabilities = ModelCapabilities {
            operations: vec![vulcan_ai_contract::AiOperation::ConversationRespond],
            reasoning: CapabilitySupport::Native,
            ..ModelCapabilities::default()
        };

        assert_eq!(
            resolve_effective_reasoning_effort(&capabilities, None),
            ReasoningEffortLevel::Medium
        );
    }

    /// Verifies every representative explicit choice, including maximum effort, remains authoritative.
    /// 验证包括最高档在内的各代表性显式选择保持权威。
    #[test]
    fn explicit_supported_preference_remains_authoritative() {
        // capabilities is the closed model contract used to validate both explicit choices.
        // capabilities 是用于校验两项显式选择的封闭模型契约。
        let capabilities = openai_reasoning_capabilities();

        assert_eq!(
            resolve_effective_reasoning_effort(&capabilities, Some(ReasoningEffortLevel::None)),
            ReasoningEffortLevel::None
        );
        assert_eq!(
            resolve_effective_reasoning_effort(&capabilities, Some(ReasoningEffortLevel::High)),
            ReasoningEffortLevel::High
        );
        assert_eq!(
            resolve_effective_reasoning_effort(&capabilities, Some(ReasoningEffortLevel::Max)),
            ReasoningEffortLevel::Max
        );
        assert_eq!(
            resolve_effective_wire_reasoning_effort(&capabilities, Some(ReasoningEffortLevel::Max),),
            Some(ReasoningEffort::Max)
        );
    }

    /// Verifies explicit unsupported/fixed-off metadata wins over stale selectable levels.
    /// 验证明确的不支持/固定关闭元数据优先于旧的可选推理档位。
    #[test]
    fn explicit_disabled_contract_wins_over_stale_levels() {
        let capabilities = ModelCapabilities {
            reasoning: CapabilitySupport::Unsupported,
            reasoning_control: ReasoningControlMode::FixedOff,
            reasoning_efforts: vec![ReasoningEffort::None, ReasoningEffort::High],
            ..ModelCapabilities::default()
        };

        assert_eq!(
            resolve_effective_reasoning_effort(&capabilities, None),
            ReasoningEffortLevel::None
        );
        assert_eq!(
            resolve_effective_reasoning_effort(&capabilities, Some(ReasoningEffortLevel::High)),
            ReasoningEffortLevel::None
        );
    }

    /// Verifies fallback follows the exact model contract when medium is unavailable.
    /// 验证中等档位不可用时，回退遵循精确模型契约。
    #[test]
    fn fallback_uses_first_enabled_exact_model_effort() {
        // enabledOnly exposes one exact enabled level after the disabled choice.
        // enabledOnly 在关闭选项后仅暴露一个精确启用档位。
        let enabled_only = ModelCapabilities {
            reasoning_efforts: vec![ReasoningEffort::None, ReasoningEffort::High],
            ..ModelCapabilities::default()
        };
        // alwaysOn proves absence of the disabled choice still selects the declared enabled level.
        // alwaysOn 证明缺少关闭选项时仍会选择已声明的启用档位。
        let always_on = ModelCapabilities {
            reasoning_efforts: vec![ReasoningEffort::High],
            ..ModelCapabilities::default()
        };
        // disabledOnly proves an explicitly fixed-off contract remains disabled.
        // disabledOnly 证明显式固定关闭契约仍保持关闭。
        let disabled_only = ModelCapabilities {
            reasoning_efforts: vec![ReasoningEffort::None],
            ..ModelCapabilities::default()
        };
        // providerDefault proves a model without an effort contract does not receive an invented level.
        // providerDefault 证明没有档位契约的模型不会被虚构档位。
        let provider_default = ModelCapabilities::default();

        assert_eq!(
            resolve_effective_reasoning_effort(&enabled_only, Some(ReasoningEffortLevel::Ultra)),
            ReasoningEffortLevel::High
        );
        assert_eq!(
            resolve_effective_reasoning_effort(&always_on, None),
            ReasoningEffortLevel::High
        );
        assert_eq!(
            resolve_effective_reasoning_effort(&disabled_only, None),
            ReasoningEffortLevel::None
        );
        assert_eq!(
            resolve_effective_reasoning_effort(&provider_default, None),
            ReasoningEffortLevel::Off
        );
    }

    /// Verifies the final wire resolver enables reasoning for an unresolved text model.
    /// 验证最终 Wire 解析器会为未解析的文本模型启用默认思考。
    #[test]
    fn wire_resolver_defaults_unknown_text_model_to_medium() {
        let capabilities = ModelCapabilities {
            operations: vec![vulcan_ai_contract::AiOperation::ConversationRespond],
            ..ModelCapabilities::default()
        };

        assert_eq!(
            resolve_effective_wire_reasoning_effort(&capabilities, None),
            Some(ReasoningEffort::Medium)
        );
        assert_eq!(
            resolve_effective_public_reasoning_effort(&capabilities, None),
            Some(ReasoningEffort::Medium)
        );
    }

    /// Verifies an explicit public `none` choice remains an intentional disablement.
    /// 验证显式公共 `none` 选择仍然表示用户有意关闭思考。
    #[test]
    fn public_resolver_preserves_explicit_none() {
        let capabilities = openai_reasoning_capabilities();

        assert_eq!(
            resolve_effective_public_reasoning_effort(&capabilities, Some(ReasoningEffort::None),),
            Some(ReasoningEffort::None)
        );
    }

    /// Verifies an explicit unsupported contract emits no synthetic wire field.
    /// 验证显式不支持契约不会伪造发送 Wire 字段。
    #[test]
    fn wire_resolver_omits_explicitly_unsupported_reasoning() {
        let capabilities = ModelCapabilities {
            reasoning: CapabilitySupport::Unsupported,
            reasoning_control: ReasoningControlMode::FixedOff,
            reasoning_efforts: vec![ReasoningEffort::None, ReasoningEffort::High],
            ..ModelCapabilities::default()
        };

        assert_eq!(
            resolve_effective_wire_reasoning_effort(&capabilities, None),
            None
        );
        assert_eq!(
            resolve_effective_public_reasoning_effort(&capabilities, Some(ReasoningEffort::High),),
            None
        );
    }
}

/// Verifies a provider-adaptive model cannot resolve either an absent or stale disabled preference to off.
/// 验证供应商自适应模型不会把缺省或陈旧关闭偏好解析为 off。
#[test]
fn provider_adaptive_contract_always_resolves_to_adaptive() {
    let capabilities = ModelCapabilities {
        reasoning: vulcan_ai_contract::CapabilitySupport::Native,
        reasoning_control: vulcan_ai_contract::ReasoningControlMode::ProviderAdaptive,
        reasoning_efforts: vec![ReasoningEffort::Adaptive],
        ..ModelCapabilities::default()
    };

    assert_eq!(
        resolve_effective_reasoning_effort(&capabilities, None),
        ReasoningEffortLevel::Adaptive
    );
    assert_eq!(
        resolve_effective_reasoning_effort(&capabilities, Some(ReasoningEffortLevel::None)),
        ReasoningEffortLevel::Adaptive
    );
    assert_eq!(
        resolve_effective_wire_reasoning_effort(&capabilities, Some(ReasoningEffortLevel::Off),),
        Some(ReasoningEffort::Adaptive)
    );
}
