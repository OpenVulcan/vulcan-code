//! Provider runtime input helpers shared by request-preview building and real transport dispatch.
//! 供请求预览构建与真实传输分发共享的 Provider 运行时输入辅助逻辑。

use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::agent_runtime::provider_native_tools::{
    build_provider_native_ai_tool_additional_params, merge_ai_tool_patch_into_additional_params,
};
use crate::agent_runtime::provider_reasoning::merge_additional_params;
use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::provider_config::{PersistedProviderOptions, ProviderConfigurationFile};
use async_stream::stream;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;

/// Resolves provider-scoped additional request parameters from the runtime blueprint payload.
/// 从运行时蓝图载荷中解析 Provider 作用域的附加请求参数。
pub(crate) fn resolve_provider_additional_params(
    blueprint: &ProviderRuntimeBlueprint,
) -> Option<Value> {
    let provider_defaults = blueprint
        .extra_body
        .as_object()
        .filter(|object| !object.is_empty())
        .map(|object| Value::Object(object.clone()));
    let base = merge_additional_params(
        provider_defaults,
        blueprint.reasoning.additional_params.clone(),
    );
    let merged = merge_ai_tool_patch_into_additional_params(
        base,
        build_provider_native_ai_tool_additional_params(blueprint),
    );
    merge_openrouter_endpoint_route_additional_params(merged, blueprint)
}

/// Merges one OpenRouter endpoint-routing override into the additional request parameters when the current run locks a specific upstream endpoint.
/// 当当前运行锁定了特定上游端点时，将一份 OpenRouter 端点路由覆盖合并进附加请求参数。
fn merge_openrouter_endpoint_route_additional_params(
    base: Option<Value>,
    blueprint: &ProviderRuntimeBlueprint,
) -> Option<Value> {
    let Some(route) = blueprint.openrouter_endpoint_route.as_ref() else {
        return base;
    };
    let route_patch = serde_json::json!({
        "provider": {
            "order": [route.tag.clone()],
            "allow_fallbacks": false,
        }
    });
    merge_additional_params(base, Some(route_patch))
}

/// Builds one request-header map from the configured strategy and provider-defined extra headers.
/// 根据配置策略与供应商定义的附加头构建一份请求头映射。
pub(crate) fn build_provider_request_headers(
    blueprint: &ProviderRuntimeBlueprint,
) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    let user_agent = blueprint
        .user_agent
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("vulcancode/{}", env!("CARGO_PKG_VERSION")));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&user_agent).map_err(|error| {
            format!(
                "构建供应商 `{}` 的 User-Agent 失败：{}。",
                blueprint.provider_name, error
            )
        })?,
    );

    for (header_name, header_value) in &blueprint.extra_headers {
        let parsed_name =
            HeaderName::from_bytes(header_name.trim().as_bytes()).map_err(|error| {
                format!(
                    "供应商 `{}` 的附加请求头名称 `{}` 无效：{}。",
                    blueprint.provider_name, header_name, error
                )
            })?;
        let parsed_value = HeaderValue::from_str(header_value.trim()).map_err(|error| {
            format!(
                "供应商 `{}` 的附加请求头 `{}` 值无效：{}。",
                blueprint.provider_name, header_name, error
            )
        })?;
        headers.insert(parsed_name, parsed_value);
    }

    Ok(headers)
}

/// Builds a one-shot provider stream that yields one terminal error and then closes immediately.
/// 构建一条一次性 Provider 流，使其仅产出一条最终错误后立即关闭。
pub(crate) fn one_shot_provider_error_stream(error: impl Into<String>) -> ProviderDeltaStream {
    let error = error.into();

    Box::pin(stream! {
        yield Err(error);
    })
}

/// Resolves the persisted provider options for one provider identifier.
/// 解析某个供应商标识对应的持久化 Provider 选项。
pub(crate) fn resolve_provider_options<'a>(
    config: &'a ProviderConfigurationFile,
    provider_id: &str,
) -> Result<&'a PersistedProviderOptions, String> {
    config
        .provider
        .get(provider_id)
        .map(|entry| &entry.options)
        .ok_or_else(|| format!("供应商 `{provider_id}` 当前尚未保存配置。"))
}

#[cfg(test)]
mod tests {
    use super::resolve_provider_additional_params;
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
    use crate::provider::capabilities::ResolvedModelCapabilities;
    use crate::provider_config::{
        OpenRouterEndpointRouteSelection, ProviderRuntimeKind,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    /// Builds one minimal runtime blueprint used by the OpenRouter endpoint-routing regression tests.
    /// 构建供 OpenRouter 端点路由回归测试使用的最小运行时蓝图。
    fn test_blueprint() -> ProviderRuntimeBlueprint {
        ProviderRuntimeBlueprint {
            provider_id: String::from("openrouter"),
            provider_name: String::from("OpenRouter"),
            runtime_kind: ProviderRuntimeKind::OpenRouter,
            runtime_label: String::from("OpenRouter"),
            model_id: String::from("openai/gpt-4o-mini"),
            model_name: String::from("GPT-4o Mini"),
            openrouter_endpoint_route: Some(OpenRouterEndpointRouteSelection {
                tag: String::from("openai"),
                provider_name: String::from("OpenAI"),
                endpoint_name: String::from("OpenAI"),
            }),
            base_url: Some(String::from("https://openrouter.ai/api/v1")),
            supports_remote_model_refresh: true,
            credential_source: String::from("api-key"),
            user_agent: None,
            extra_headers: BTreeMap::new(),
            extra_body: json!({
                "provider": {
                    "require_parameters": true
                }
            }),
            reasoning: ProviderRuntimeReasoningResolution::default(),
            capabilities: ResolvedModelCapabilities::default(),
            interleaved_reasoning: None,
        }
    }

    /// Verifies OpenRouter endpoint-routing rules merge into the additional-params object without discarding pre-existing provider fields.
    /// 验证 OpenRouter 端点路由规则会合并进附加参数对象，同时不会丢弃已有的 provider 字段。
    #[test]
    fn openrouter_endpoint_route_merges_into_additional_params() {
        let additional_params =
            resolve_provider_additional_params(&test_blueprint()).expect("expected params");
        let provider = additional_params
            .get("provider")
            .and_then(serde_json::Value::as_object)
            .expect("expected provider object");

        assert_eq!(
            provider
                .get("order")
                .and_then(serde_json::Value::as_array)
                .and_then(|value| value.first())
                .and_then(serde_json::Value::as_str),
            Some("openai")
        );
        assert_eq!(
            provider
                .get("allow_fallbacks")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            provider
                .get("require_parameters")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }
}
