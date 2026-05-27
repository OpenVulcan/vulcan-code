//! Provider-native AI-tool request patching utilities.
//! Provider 原生 AI 工具请求补丁辅助逻辑。

use crate::agent_runtime::ai_tool_router::{AiToolRouteDecision, resolve_ai_tool_plan};
use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::provider_config::{AiToolKind, ProviderApiFamily};
use serde_json::{Map as JsonMap, Value, json};

/// Builds one additional-params payload that injects provider-native AI tools for the active model.
/// 为当前激活模型构建一份注入 Provider 原生 AI 工具的附加参数载荷。
pub(crate) fn build_provider_native_ai_tool_additional_params(
    blueprint: &ProviderRuntimeBlueprint,
) -> Option<Value> {
    let plan = resolve_ai_tool_plan(blueprint);
    build_provider_native_ai_tool_patch(blueprint.api_family(), &plan.provider_native_decisions())
}

/// Merges one AI-tool patch into existing additional params while appending tool arrays.
/// 将 AI 工具补丁合并进已有附加参数，同时追加工具数组。
pub(crate) fn merge_ai_tool_patch_into_additional_params(
    base: Option<Value>,
    patch: Option<Value>,
) -> Option<Value> {
    match (base, patch) {
        (None, None) => None,
        (Some(base), None) => Some(base),
        (None, Some(patch)) => Some(patch),
        (Some(base), Some(patch)) => Some(merge_ai_tool_json_values(base, patch)),
    }
}

/// Builds the provider-family-specific JSON patch for provider-native AI tools.
/// 为 Provider 原生 AI 工具构建接口族专属 JSON 补丁。
fn build_provider_native_ai_tool_patch(
    api_family: ProviderApiFamily,
    decisions: &[&AiToolRouteDecision],
) -> Option<Value> {
    decisions
        .iter()
        .filter_map(|decision| provider_native_ai_tool_patch(api_family, decision))
        .reduce(merge_ai_tool_json_values)
}

/// Converts one provider-native AI-tool decision into the concrete provider request patch.
/// 将单条 Provider 原生 AI 工具决策转换为具体 Provider 请求补丁。
fn provider_native_ai_tool_patch(
    api_family: ProviderApiFamily,
    decision: &AiToolRouteDecision,
) -> Option<Value> {
    if let Some(patch) = decision.native_request_patch.clone() {
        return Some(patch);
    }

    match (api_family, decision.kind) {
        (ProviderApiFamily::OpenAiResponses, AiToolKind::WebSearch) => {
            let tool_type = decision
                .native_tool_type
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("web_search_preview");
            Some(json!({ "tools": [{ "type": tool_type }] }))
        }
        (ProviderApiFamily::GoogleGeminiGenerateContent, AiToolKind::WebSearch) => None,
        (ProviderApiFamily::AnthropicMessages, AiToolKind::WebSearch) => {
            let tool_type = decision
                .native_tool_type
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("web_search_20250305");
            Some(json!({
                "tools": [{
                    "type": tool_type,
                    "name": "web_search"
                }]
            }))
        }
        _ => None,
    }
}

/// Deep-merges two JSON values and appends provider tool arrays instead of overwriting them.
/// 深度合并两个 JSON 值，并追加 Provider 工具数组而不是覆盖它们。
fn merge_ai_tool_json_values(base: Value, patch: Value) -> Value {
    match (base, patch) {
        (Value::Object(mut base_object), Value::Object(patch_object)) => {
            for (key, patch_value) in patch_object {
                if key == "tools" || key == "include" {
                    append_json_array_field(&mut base_object, key, patch_value);
                    continue;
                }

                let merged_value = match base_object.remove(&key) {
                    Some(base_value) => merge_ai_tool_json_values(base_value, patch_value),
                    None => patch_value,
                };
                base_object.insert(key, merged_value);
            }
            Value::Object(base_object)
        }
        (_, patch) => patch,
    }
}

/// Appends a JSON array field while preserving caller-provided entries first.
/// 追加 JSON 数组字段，并保持调用方已有条目位于前面。
fn append_json_array_field(object: &mut JsonMap<String, Value>, key: String, patch_value: Value) {
    let mut combined = object
        .remove(&key)
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    if let Value::Array(mut patch_items) = patch_value {
        combined.append(&mut patch_items);
    }
    object.insert(key, Value::Array(combined));
}

#[cfg(test)]
mod tests {
    //! Provider-native AI-tool patch regression tests.
    //! Provider 原生 AI 工具补丁回归测试。

    use super::{
        build_provider_native_ai_tool_additional_params, merge_ai_tool_patch_into_additional_params,
    };
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
    use crate::provider_capabilities::{ResolvedAiToolCapability, ResolvedModelCapabilities};
    use crate::provider_config::{
        AiToolKind, AiToolPolicy, ProviderRuntimeKind, ReasoningEffortLevel,
    };
    use serde_json::json;

    /// Appends native tools without replacing user-provided tools.
    /// 追加原生工具时不会替换用户已经提供的工具。
    #[test]
    fn merge_ai_tool_patch_appends_tools_array() {
        let merged = merge_ai_tool_patch_into_additional_params(
            Some(json!({ "tools": [{ "type": "existing" }] })),
            Some(json!({ "tools": [{ "type": "web_search_preview" }] })),
        )
        .expect("merged params");

        assert_eq!(
            merged["tools"],
            json!([{ "type": "existing" }, { "type": "web_search_preview" }])
        );
    }

    /// Uses catalog-declared native request patches before applying built-in provider defaults.
    /// 优先使用目录声明的原生请求补丁，而不是内置 Provider 默认值。
    #[test]
    fn native_ai_tool_patch_prefers_declared_request_patch() {
        let blueprint = ProviderRuntimeBlueprint {
            provider_id: String::from("alibaba-cn"),
            provider_name: String::from("Alibaba"),
            runtime_kind: ProviderRuntimeKind::OpenAiResponsesCompatible,
            runtime_label: String::from("OpenAI Responses-Compatible"),
            model_id: String::from("qwen3.6-plus"),
            model_name: String::from("Qwen 3.6 Plus"),
            openrouter_endpoint_route: None,
            base_url: Some(String::from(
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
            )),
            supports_remote_model_refresh: false,
            credential_source: String::from("test"),
            user_agent: None,
            extra_headers: Default::default(),
            extra_body: json!({}),
            reasoning: ProviderRuntimeReasoningResolution {
                requested_effort: ReasoningEffortLevel::Off,
                source_label: String::from("接口默认"),
                additional_params: None,
                note: None,
            },
            capabilities: ResolvedModelCapabilities {
                supports_tool_call: true,
                ai_tools: vec![ResolvedAiToolCapability {
                    kind: AiToolKind::WebSearch,
                    policy: AiToolPolicy::ForceNative,
                    native_provider_tool: true,
                    native_tool_type: None,
                    native_request_patch: Some(json!({
                        "tools": [{ "type": "web_search" }]
                    })),
                    fallback_tool_id: None,
                    requires_approval: false,
                    result_modalities: vec![String::from("text")],
                    citation_support: true,
                    source_label: String::from("模型声明"),
                }],
                ..ResolvedModelCapabilities::default()
            },
            interleaved_reasoning: None,
        };

        let patch = build_provider_native_ai_tool_additional_params(&blueprint).expect("patch");

        assert_eq!(patch["tools"][0]["type"], "web_search");
    }

    /// Does not generate Google Gemini native search patches because they conflict with coding function tools.
    /// 不生成 Google Gemini 原生搜索补丁，因为它会与编码函数工具冲突。
    #[test]
    fn native_ai_tool_patch_omits_google_gemini_search() {
        let blueprint = ProviderRuntimeBlueprint {
            provider_id: String::from("google"),
            provider_name: String::from("Google"),
            runtime_kind: ProviderRuntimeKind::GoogleGemini,
            runtime_label: String::from("Google Gemini"),
            model_id: String::from("gemini-2.5-flash"),
            model_name: String::from("Gemini 2.5 Flash"),
            openrouter_endpoint_route: None,
            base_url: None,
            supports_remote_model_refresh: false,
            credential_source: String::from("test"),
            user_agent: None,
            extra_headers: Default::default(),
            extra_body: json!({}),
            reasoning: ProviderRuntimeReasoningResolution {
                requested_effort: ReasoningEffortLevel::Off,
                source_label: String::from("接口默认"),
                additional_params: None,
                note: None,
            },
            capabilities: ResolvedModelCapabilities {
                supports_tool_call: true,
                ai_tools: vec![ResolvedAiToolCapability {
                    kind: AiToolKind::WebSearch,
                    policy: AiToolPolicy::ForceNative,
                    native_provider_tool: true,
                    native_tool_type: Some(String::from("google_search")),
                    native_request_patch: None,
                    fallback_tool_id: None,
                    requires_approval: false,
                    result_modalities: vec![String::from("text")],
                    citation_support: true,
                    source_label: String::from("模型声明"),
                }],
                ..ResolvedModelCapabilities::default()
            },
            interleaved_reasoning: None,
        };

        let patch = build_provider_native_ai_tool_additional_params(&blueprint);

        assert!(patch.is_none());
    }
}
