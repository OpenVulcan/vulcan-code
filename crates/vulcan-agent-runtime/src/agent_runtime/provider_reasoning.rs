//! Shared provider reasoning-profile resolution and request-patch merge helpers.
//! 共享的 Provider 推理档位解析与请求 Patch 合并辅助模块。

use crate::provider_config::{
    ProviderApiFamily, ProviderRuntimeKind, ReasoningEffortLevel, ReasoningProfileConfig,
};
use serde_json::{Value, json};

/// Resolved provider reasoning controls attached to one runtime blueprint after the shared fallback chain has been applied.
/// 在共享回退链应用完成后附着到单个运行时蓝图上的已解析 Provider 推理控制结果。
#[derive(Debug, Clone)]
pub struct ProviderRuntimeReasoningResolution {
    /// Shared reasoning-effort preset requested by the workbench run controls.
    /// 工作台运行控制区请求的共享推理档位预设。
    pub requested_effort: ReasoningEffortLevel,
    /// Human-readable source label describing which layer produced the effective request patch.
    /// 描述哪一层产出了生效请求 Patch 的人类可读来源标签。
    pub source_label: String,
    /// Provider-specific request patch that should be merged into the final additional-params payload.
    /// 需要合并进最终附加参数载荷中的 Provider 专属请求 Patch。
    pub additional_params: Option<Value>,
    /// Optional diagnostics note that explains one capability downgrade or ignored preset.
    /// 用于解释能力降级或忽略预设的可选诊断说明。
    pub note: Option<String>,
}

impl Default for ProviderRuntimeReasoningResolution {
    /// Builds one no-op reasoning resolution that keeps the provider defaults untouched.
    /// 构建一份不改变 Provider 默认行为的空操作推理解析结果。
    fn default() -> Self {
        Self {
            requested_effort: ReasoningEffortLevel::Off,
            source_label: String::from("接口默认"),
            additional_params: None,
            note: None,
        }
    }
}

impl ProviderRuntimeReasoningResolution {
    /// Returns one compact human-readable summary row used by runtime diagnostics and request previews.
    /// 返回运行时诊断与请求预览使用的一条紧凑人类可读摘要行。
    pub fn summary_label(&self) -> String {
        format!(
            "推理：{} · 来源 {}",
            self.requested_effort, self.source_label
        )
    }
}

/// Resolves the effective provider reasoning controls from the shared workbench preset plus model, provider, and runtime defaults.
/// 根据共享工作台预设、模型覆盖、供应商覆盖与运行时默认值解析生效中的 Provider 推理控制。
pub fn resolve_provider_runtime_reasoning(
    runtime_kind: ProviderRuntimeKind,
    provider_id: &str,
    model_id: &str,
    model_supports_reasoning: bool,
    requested_effort: ReasoningEffortLevel,
    provider_profile: Option<&ReasoningProfileConfig>,
    model_profile: Option<&ReasoningProfileConfig>,
) -> ProviderRuntimeReasoningResolution {
    if reasoning_profile_uses_default_only(model_profile)
        || reasoning_profile_uses_default_only(provider_profile)
    {
        return ProviderRuntimeReasoningResolution {
            requested_effort,
            source_label: String::from("模型默认"),
            additional_params: None,
            note: None,
        };
    }

    if let Some(patch) =
        model_profile.and_then(|profile| profile.patch_for_effort(requested_effort))
    {
        return ProviderRuntimeReasoningResolution {
            requested_effort,
            source_label: String::from("模型覆盖"),
            additional_params: Some(patch),
            note: build_reasoning_profile_note(model_profile, requested_effort),
        };
    }

    if let Some(patch) =
        provider_profile.and_then(|profile| profile.patch_for_effort(requested_effort))
    {
        return ProviderRuntimeReasoningResolution {
            requested_effort,
            source_label: String::from("供应商覆盖"),
            additional_params: Some(patch),
            note: build_reasoning_profile_note(provider_profile, requested_effort),
        };
    }

    if !model_supports_reasoning && requested_effort.requests_explicit_reasoning() {
        return ProviderRuntimeReasoningResolution {
            requested_effort,
            source_label: String::from("接口默认"),
            additional_params: None,
            note: Some(format!(
                "模型 `{model_id}` 当前未声明推理能力，已忽略 `{}` 档位覆盖。",
                requested_effort
            )),
        };
    }

    if let Some(patch) = resolve_runtime_default_reasoning_patch(
        runtime_kind,
        provider_id,
        model_id,
        requested_effort,
    ) {
        return ProviderRuntimeReasoningResolution {
            requested_effort,
            source_label: String::from("接口默认"),
            additional_params: Some(patch),
            note: build_runtime_reasoning_note(runtime_kind, model_id, requested_effort),
        };
    }

    ProviderRuntimeReasoningResolution {
        requested_effort,
        source_label: String::from("接口默认"),
        additional_params: None,
        note: build_runtime_reasoning_note(runtime_kind, model_id, requested_effort),
    }
}

/// Returns whether the current provider or model reasoning profile should always preserve the upstream default behavior.
/// 返回当前供应商或模型思考配置是否应始终保留上游默认行为。
fn reasoning_profile_uses_default_only(profile: Option<&ReasoningProfileConfig>) -> bool {
    matches!(
        profile.and_then(|item| item.kind),
        Some(crate::provider_config::ReasoningCapabilityKind::DefaultOnly)
    )
}

/// Deep-merges one optional provider-default JSON payload with one optional reasoning patch while preserving nested object fields.
/// 在保留嵌套对象字段的前提下，深度合并可选的 Provider 默认 JSON 载荷与可选推理 Patch。
pub(crate) fn merge_additional_params(
    base: Option<Value>,
    overlay: Option<Value>,
) -> Option<Value> {
    match (base, overlay) {
        (None, None) => None,
        (Some(base), None) => Some(base),
        (None, Some(overlay)) => Some(overlay),
        (Some(base), Some(overlay)) => Some(merge_json_values(base, overlay)),
    }
}

/// Deep-merges two JSON values and lets the overlay win whenever the current branch is not an object on both sides.
/// 深度合并两个 JSON 值，并在当前分支两侧不同时为对象时让覆盖值优先生效。
fn merge_json_values(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_object), Value::Object(overlay_object)) => {
            for (key, overlay_value) in overlay_object {
                let merged_value = match base_object.remove(&key) {
                    Some(base_value) => merge_json_values(base_value, overlay_value),
                    None => overlay_value,
                };
                base_object.insert(key, merged_value);
            }
            Value::Object(base_object)
        }
        (_, overlay) => overlay,
    }
}

/// Builds one optional diagnostics note for provider or model override profiles whose effective semantics collapse multiple workbench levels into one provider-specific switch.
/// 为会把多个工作台档位塌缩到同一 Provider 专用开关的供应商或模型覆盖配置构建可选诊断说明。
fn build_reasoning_profile_note(
    profile: Option<&ReasoningProfileConfig>,
    requested_effort: ReasoningEffortLevel,
) -> Option<String> {
    let Some(profile) = profile else {
        return None;
    };

    match profile.kind {
        Some(crate::provider_config::ReasoningCapabilityKind::Binary)
        | Some(crate::provider_config::ReasoningCapabilityKind::BinaryWithBudget)
            if requested_effort.requests_explicit_reasoning() =>
        {
            Some(String::from(
                "当前供应商覆盖配置使用开关式思考控制，不同高档位可能落到同一生效模式。",
            ))
        }
        Some(crate::provider_config::ReasoningCapabilityKind::ProxyTiered)
            if requested_effort.requests_explicit_reasoning() =>
        {
            Some(String::from(
                "当前供应商覆盖配置属于代理分档控制，下游模型可能把多个档位塌缩为同一实际效果。",
            ))
        }
        _ => None,
    }
}

/// Builds one optional diagnostics note for runtime-default mappings whose native provider semantics differ from the workbench’s shared five-level control model.
/// 为原生 Provider 语义与工作台共享五档控制模型存在差异的运行时默认映射构建可选诊断说明。
fn build_runtime_reasoning_note(
    runtime_kind: ProviderRuntimeKind,
    model_id: &str,
    requested_effort: ReasoningEffortLevel,
) -> Option<String> {
    match runtime_kind {
        ProviderRuntimeKind::GoogleGemini
            if model_id.to_ascii_lowercase().starts_with("gemini-3")
                && requested_effort == ReasoningEffortLevel::XHigh =>
        {
            Some(String::from(
                "当前 Gemini 3 原生接口仅支持 `低` 与 `高` 两档思考级别，更高档位会压缩到最高可用等级。",
            ))
        }
        ProviderRuntimeKind::Cohere if requested_effort.requests_explicit_reasoning() => Some(
            String::from("当前 Cohere 原生接口仅支持关闭或开启两档思考，其它档位会映射到开启。"),
        ),
        _ => None,
    }
}

/// Resolves one runtime-family default reasoning patch for the current provider and model when no explicit model or provider override exists.
/// 在不存在显式模型或供应商覆盖时，为当前供应商与模型解析一份运行时家族默认推理 Patch。
fn resolve_runtime_default_reasoning_patch(
    runtime_kind: ProviderRuntimeKind,
    _provider_id: &str,
    model_id: &str,
    requested_effort: ReasoningEffortLevel,
) -> Option<Value> {
    match runtime_kind.api_family() {
        ProviderApiFamily::OpenAiResponses | ProviderApiFamily::OpenAiChatCompatible => {
            resolve_openai_family_reasoning_patch(requested_effort)
        }
        ProviderApiFamily::AnthropicMessages => resolve_anthropic_reasoning_patch(requested_effort),
        ProviderApiFamily::GoogleGeminiGenerateContent => {
            resolve_google_gemini_reasoning_patch(model_id, requested_effort)
        }
        ProviderApiFamily::CohereChat => resolve_cohere_reasoning_patch(requested_effort),
        ProviderApiFamily::BedrockConverse => None,
    }
}

/// Resolves the shared OpenAI-family reasoning patch used by OpenAI, OpenAI-compatible, OpenRouter, Azure OpenAI, and compatible proxy runtimes.
/// 解析 OpenAI、OpenAI-compatible、OpenRouter、Azure OpenAI 及兼容代理运行时共用的 OpenAI 家族推理 Patch。
fn resolve_openai_family_reasoning_patch(requested_effort: ReasoningEffortLevel) -> Option<Value> {
    let effort = match requested_effort {
        ReasoningEffortLevel::Off => return None,
        ReasoningEffortLevel::Low => "minimal",
        ReasoningEffortLevel::Medium => "low",
        ReasoningEffortLevel::High => "medium",
        ReasoningEffortLevel::XHigh => "high",
    };

    Some(json!({
        "reasoning": {
            "effort": effort
        }
    }))
}

/// Resolves the Anthropic thinking-budget patch used by the host-owned Anthropic runtime.
/// 解析宿主自有 Anthropic 运行时使用的思考预算 Patch。
fn resolve_anthropic_reasoning_patch(requested_effort: ReasoningEffortLevel) -> Option<Value> {
    let budget_tokens = match requested_effort {
        ReasoningEffortLevel::Off => return None,
        ReasoningEffortLevel::Low => 1024,
        ReasoningEffortLevel::Medium => 4096,
        ReasoningEffortLevel::High => 8192,
        ReasoningEffortLevel::XHigh => 16384,
    };

    Some(json!({
        "thinking": {
            "type": "enabled",
            "budget_tokens": budget_tokens
        }
    }))
}

/// Resolves the Google Gemini native reasoning patch by choosing the correct native thinking control for the concrete model family.
/// 通过为具体模型家族选择正确的原生思考控制，解析 Google Gemini 原生推理 Patch。
fn resolve_google_gemini_reasoning_patch(
    model_id: &str,
    requested_effort: ReasoningEffortLevel,
) -> Option<Value> {
    let normalized_model_id = model_id.to_ascii_lowercase();

    if normalized_model_id.starts_with("gemini-3") {
        return resolve_gemini_three_reasoning_patch(requested_effort);
    }

    if normalized_model_id.starts_with("gemini-2.5-pro") {
        return resolve_gemini_two_five_pro_reasoning_patch(requested_effort);
    }

    if normalized_model_id.starts_with("gemini-2.5-") {
        return resolve_gemini_two_five_flash_reasoning_patch(requested_effort);
    }

    resolve_generic_google_reasoning_patch(requested_effort)
}

/// Resolves the Gemini 2.5 Flash-family native reasoning patch.
/// 解析 Gemini 2.5 Flash 家族的原生推理 Patch。
fn resolve_gemini_two_five_flash_reasoning_patch(
    requested_effort: ReasoningEffortLevel,
) -> Option<Value> {
    let thinking_budget = match requested_effort {
        ReasoningEffortLevel::Off => 0,
        ReasoningEffortLevel::Low => 1024,
        ReasoningEffortLevel::Medium => 8192,
        ReasoningEffortLevel::High | ReasoningEffortLevel::XHigh => 24576,
    };

    Some(build_google_gemini_budget_patch(
        thinking_budget,
        requested_effort.requests_explicit_reasoning(),
    ))
}

/// Resolves the Gemini 2.5 Pro native reasoning patch.
/// 解析 Gemini 2.5 Pro 的原生推理 Patch。
fn resolve_gemini_two_five_pro_reasoning_patch(
    requested_effort: ReasoningEffortLevel,
) -> Option<Value> {
    let thinking_budget = match requested_effort {
        ReasoningEffortLevel::Off | ReasoningEffortLevel::Low => 1024,
        ReasoningEffortLevel::Medium => 8192,
        ReasoningEffortLevel::High | ReasoningEffortLevel::XHigh => 32768,
    };

    Some(build_google_gemini_budget_patch(thinking_budget, true))
}

/// Resolves the Gemini 3-family native reasoning patch.
/// 解析 Gemini 3 家族的原生推理 Patch。
fn resolve_gemini_three_reasoning_patch(requested_effort: ReasoningEffortLevel) -> Option<Value> {
    let thinking_level = match requested_effort {
        ReasoningEffortLevel::Off | ReasoningEffortLevel::Low | ReasoningEffortLevel::Medium => {
            "low"
        }
        ReasoningEffortLevel::High | ReasoningEffortLevel::XHigh => "high",
    };

    Some(json!({
        "generationConfig": {
            "thinkingConfig": {
                "includeThoughts": true,
                "thinkingLevel": thinking_level
            }
        }
    }))
}

/// Resolves the generic Google native reasoning patch used for reasoning-capable Gemini family members without an explicit family-specific override.
/// 解析没有显式家族级覆盖时供具备思考能力的 Google 模型使用的通用原生推理 Patch。
fn resolve_generic_google_reasoning_patch(requested_effort: ReasoningEffortLevel) -> Option<Value> {
    let thinking_budget = match requested_effort {
        ReasoningEffortLevel::Off => 0,
        ReasoningEffortLevel::Low => 1024,
        ReasoningEffortLevel::Medium => 4096,
        ReasoningEffortLevel::High => 8192,
        ReasoningEffortLevel::XHigh => 16384,
    };

    Some(build_google_gemini_budget_patch(
        thinking_budget,
        requested_effort.requests_explicit_reasoning(),
    ))
}

/// Builds one Gemini native `thinkingConfig` patch from a budget value and an include-thoughts switch.
/// 根据预算值与是否回传思考片段的开关构建一份 Gemini 原生 `thinkingConfig` Patch。
fn build_google_gemini_budget_patch(thinking_budget: i64, include_thoughts: bool) -> Value {
    json!({
        "generationConfig": {
            "thinkingConfig": {
                "includeThoughts": include_thoughts,
                "thinkingBudget": thinking_budget
            }
        }
    })
}

/// Resolves the Cohere native reasoning patch.
/// 解析 Cohere 的原生推理 Patch。
fn resolve_cohere_reasoning_patch(requested_effort: ReasoningEffortLevel) -> Option<Value> {
    if requested_effort == ReasoningEffortLevel::Off {
        return Some(json!({
            "thinking": {
                "type": "disabled"
            }
        }));
    }

    let token_budget = match requested_effort {
        ReasoningEffortLevel::Low => 4096,
        ReasoningEffortLevel::Medium => 8192,
        ReasoningEffortLevel::High => 16384,
        ReasoningEffortLevel::XHigh => 31744,
        ReasoningEffortLevel::Off => 0,
    };

    Some(json!({
        "thinking": {
            "type": "enabled",
            "token_budget": token_budget
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        ProviderRuntimeReasoningResolution, merge_additional_params,
        resolve_provider_runtime_reasoning,
    };
    use crate::provider_config::{
        ProviderRuntimeKind, ReasoningCapabilityKind, ReasoningEffortLevel, ReasoningProfileConfig,
    };
    use serde_json::json;

    /// Verifies provider or model reasoning patches deep-merge with provider defaults instead of replacing sibling nested fields.
    /// 验证推理 Patch 会与 Provider 默认值进行深度合并，而不是替换掉同级嵌套字段。
    #[test]
    fn merges_reasoning_patch_deeply() {
        let merged = merge_additional_params(
            Some(json!({
                "generationConfig": {
                    "responseModalities": ["TEXT"]
                }
            })),
            Some(json!({
                "generationConfig": {
                    "thinkingConfig": {
                        "thinkingBudget": 2048
                    }
                }
            })),
        )
        .expect("merged payload should exist");

        assert_eq!(
            merged,
            json!({
                "generationConfig": {
                    "responseModalities": ["TEXT"],
                    "thinkingConfig": {
                        "thinkingBudget": 2048
                    }
                }
            })
        );
    }

    /// Verifies model-scoped reasoning overrides win over provider defaults when both layers define a patch for the same requested effort.
    /// 验证当模型层与供应商层同时为同一请求档位定义 Patch 时，模型层覆盖会优先生效。
    #[test]
    fn model_reasoning_profile_overrides_provider_defaults() {
        let provider_profile = ReasoningProfileConfig {
            kind: Some(ReasoningCapabilityKind::Binary),
            low: Some(json!({"enable_thinking": true})),
            ..ReasoningProfileConfig::default()
        };
        let model_profile = ReasoningProfileConfig {
            low: Some(json!({"reasoning": {"effort": "minimal"}})),
            ..ReasoningProfileConfig::default()
        };

        let resolution = resolve_provider_runtime_reasoning(
            ProviderRuntimeKind::OpenAiCompatible,
            "bailing",
            "ring-1t",
            true,
            ReasoningEffortLevel::Low,
            Some(&provider_profile),
            Some(&model_profile),
        );

        assert_eq!(resolution.source_label, "模型覆盖");
        assert_eq!(
            resolution.additional_params,
            Some(json!({"reasoning": {"effort": "minimal"}}))
        );
    }

    /// Verifies Gemini 2.5 Flash falls back to the native `thinkingBudget` patch family when no model or provider override is configured.
    /// 验证当不存在模型或供应商覆盖时，Gemini 2.5 Flash 会回退到原生 `thinkingBudget` Patch 家族。
    #[test]
    fn gemini_runtime_defaults_build_native_reasoning_patch() {
        let resolution: ProviderRuntimeReasoningResolution = resolve_provider_runtime_reasoning(
            ProviderRuntimeKind::GoogleGemini,
            "google",
            "gemini-2.5-flash",
            true,
            ReasoningEffortLevel::High,
            None,
            None,
        );

        assert_eq!(resolution.source_label, "接口默认");
        assert_eq!(
            resolution.additional_params,
            Some(json!({
                "generationConfig": {
                    "thinkingConfig": {
                        "includeThoughts": true,
                        "thinkingBudget": 24576
                    }
                }
            }))
        );
    }

    /// Verifies default-only reasoning profiles preserve upstream provider defaults instead of falling through to generic runtime patches.
    /// 验证默认思考配置会保留上游 Provider 默认行为，而不会继续落到通用运行时 Patch。
    #[test]
    fn default_only_reasoning_profile_preserves_provider_defaults() {
        let model_profile = ReasoningProfileConfig {
            kind: Some(ReasoningCapabilityKind::DefaultOnly),
            ..ReasoningProfileConfig::default()
        };

        let resolution = resolve_provider_runtime_reasoning(
            ProviderRuntimeKind::OpenAiCompatible,
            "alibaba-cn",
            "future-ali-model",
            true,
            ReasoningEffortLevel::Medium,
            None,
            Some(&model_profile),
        );

        assert_eq!(resolution.source_label, "模型默认");
        assert_eq!(resolution.additional_params, None);
        assert_eq!(resolution.note, None);
    }
}
