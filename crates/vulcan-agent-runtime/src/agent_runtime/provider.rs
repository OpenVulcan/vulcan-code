//! Provider/model runtime blueprint resolution used by the compose execution layer.
//! compose 执行层使用的 Provider/Model 运行时蓝图解析逻辑。

use crate::agent_runtime::provider_reasoning::{
    ProviderRuntimeReasoningResolution, resolve_provider_runtime_reasoning,
};
use crate::provider_capabilities::{
    ResolvedModelCapabilities, resolve_credential_source_summary,
    resolve_effective_model_capabilities, resolve_provider_api_key,
};
use crate::provider_config::{
    self, ModelInterleavedCapability, OpenRouterEndpointRouteSelection,
    PersistedAiToolConfiguration, PersistedProviderOptions, ProviderApiFamily,
    ProviderConfigurationFile, ProviderFieldKey, ProviderRuntimeKind, ReasoningEffortLevel,
};
use serde_json::Value;
use std::collections::BTreeMap;

/// Resolved provider/model runtime blueprint used by execution preparation.
/// 执行准备阶段使用的已解析 Provider/Model 运行时蓝图。
#[derive(Debug, Clone)]
pub struct ProviderRuntimeBlueprint {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Human-readable provider name.
    /// 人类可读的供应商名称。
    pub provider_name: String,
    /// Runtime family used by the currently selected provider.
    /// 当前选中供应商使用的运行时族。
    pub runtime_kind: ProviderRuntimeKind,
    /// Human-readable runtime label.
    /// 人类可读的运行时标签。
    pub runtime_label: String,
    /// Stable model identifier.
    /// 稳定模型标识。
    pub model_id: String,
    /// Human-readable model name.
    /// 人类可读的模型名称。
    pub model_name: String,
    /// Optional OpenRouter endpoint route locked for the current run.
    /// 为当前运行锁定的可选 OpenRouter 端点路由。
    pub openrouter_endpoint_route: Option<OpenRouterEndpointRouteSelection>,
    /// Resolved base URL when one is available.
    /// 当存在时的已解析接口基址。
    pub base_url: Option<String>,
    /// Whether the provider can actively refresh remote models.
    /// 该供应商是否可以主动刷新远端模型列表。
    pub supports_remote_model_refresh: bool,
    /// Human-readable credential source summary.
    /// 人类可读的凭据来源摘要。
    pub credential_source: String,
    /// Explicit user-agent string declared by the provider definition.
    /// 供应商定义声明的显式 User-Agent 字符串。
    pub user_agent: Option<String>,
    /// Additional HTTP headers configured by the provider definition.
    /// 供应商定义配置的附加 HTTP 头集合。
    pub extra_headers: BTreeMap<String, String>,
    /// Additional JSON body parameters configured by the provider definition.
    /// 供应商定义配置的附加 JSON Body 参数集合。
    pub extra_body: Value,
    /// Shared reasoning-resolution result derived from model overrides, provider overrides, and runtime defaults.
    /// 由模型覆盖、供应商覆盖与运行时默认值共同解析出的共享推理控制结果。
    pub reasoning: ProviderRuntimeReasoningResolution,
    /// Resolved host-consumed capability snapshot derived from the provider catalog and selected model entry.
    /// 从供应商目录与选中模型条目派生出的宿主消费能力快照。
    pub capabilities: ResolvedModelCapabilities,
    /// Optional model-scoped interleaved reasoning capability used by OpenAI-compatible transports.
    /// 供 OpenAI 兼容传输链路使用的可选模型级交错推理能力描述。
    pub interleaved_reasoning: Option<ModelInterleavedCapability>,
}

impl ProviderRuntimeBlueprint {
    /// Returns the canonical provider API family derived from the resolved runtime kind.
    /// 返回从已解析运行时类型派生出的规范 Provider 接口族。
    pub const fn api_family(&self) -> ProviderApiFamily {
        self.runtime_kind.api_family()
    }

    /// Returns the stable API-family key used by diagnostics and request previews.
    /// 返回供诊断与请求预览复用的稳定接口族键值。
    pub const fn api_family_key(&self) -> &'static str {
        self.api_family().key()
    }

    /// Returns the human-readable API-family label used by diagnostics surfaces.
    /// 返回供诊断界面使用的人类可读接口族标签。
    pub const fn api_family_label(&self) -> &'static str {
        self.api_family().label()
    }

    /// Returns a compact stable runtime-kind key used by diagnostics and future factories.
    /// 返回供诊断与后续工厂使用的紧凑稳定运行时键值。
    pub const fn runtime_kind_key(&self) -> &'static str {
        match self.runtime_kind {
            ProviderRuntimeKind::OpenAiCompatible => "openai-compatible",
            ProviderRuntimeKind::OpenAi => "openai",
            ProviderRuntimeKind::OpenAiResponsesCompatible => "openai-responses-compatible",
            ProviderRuntimeKind::Anthropic => "anthropic",
            ProviderRuntimeKind::AzureOpenAi => "azure-openai",
            ProviderRuntimeKind::GoogleGemini => "google-gemini",
            ProviderRuntimeKind::XAi => "xai",
            ProviderRuntimeKind::Mistral => "mistral",
            ProviderRuntimeKind::Groq => "groq",
            ProviderRuntimeKind::Cohere => "cohere",
            ProviderRuntimeKind::TogetherAi => "together-ai",
            ProviderRuntimeKind::Perplexity => "perplexity",
            ProviderRuntimeKind::OpenRouter => "openrouter",
        }
    }

    /// Returns a compact endpoint summary rendered in execution diagnostics.
    /// 返回展示在执行诊断中的紧凑接口摘要。
    pub fn endpoint_summary(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| String::from("由运行时自动解析"))
    }

    /// Returns whether the active model accepts direct tool exposure for the current turn.
    /// 返回当前激活模型是否接受本轮直接暴露工具。
    pub const fn supports_tool_call(&self) -> bool {
        self.capabilities.supports_tool_call
    }

    /// Returns whether the active model accepts explicit temperature overrides.
    /// 返回当前激活模型是否接受显式温度覆盖。
    pub const fn supports_temperature(&self) -> bool {
        self.capabilities.supports_temperature
    }
}

/// Resolves the active provider/model pair into one validated runtime blueprint using one explicit AI-tool configuration snapshot.
/// 使用一份显式 AI 工具配置快照，将当前激活的 Provider/Model 对解析为一份通过校验的运行时蓝图。
pub fn resolve_active_provider_blueprint_with_ai_tools(
    config: &ProviderConfigurationFile,
    requested_reasoning_effort: ReasoningEffortLevel,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Result<ProviderRuntimeBlueprint, String> {
    let active_model = config
        .model
        .as_deref()
        .ok_or_else(|| String::from("当前尚未配置激活模型，无法生成运行时蓝图。"))?;
    let (provider_id, model_id) = provider_config::parse_active_model_reference(active_model)
        .ok_or_else(|| String::from("当前激活模型格式无效，期望为 `provider/model`。"))?;
    let definition = provider_config::supported_provider_by_id(provider_id)
        .ok_or_else(|| format!("未找到供应商 `{provider_id}` 对应的运行时定义。"))?;
    let provider_entry = config.provider.get(provider_id).ok_or_else(|| {
        format!(
            "供应商 `{}` 当前尚未保存配置，无法生成运行时蓝图。",
            definition.name
        )
    })?;
    let options = &provider_entry.options;

    let selected_model = provider_config::effective_model_choices(provider_id, config)
        .into_iter()
        .find(|model| model.id == model_id);
    let effective_runtime = definition.effective_runtime_for_model(model_id);
    let capabilities = resolve_effective_model_capabilities(
        definition,
        selected_model.as_ref(),
        ai_tool_configuration,
    );
    validate_required_provider_fields(
        definition.input_fields().as_slice(),
        options,
        &capabilities,
        definition.name.as_str(),
    )?;
    let model_name = selected_model
        .as_ref()
        .map(|model| model.name.clone())
        .unwrap_or_else(|| model_id.to_string());
    let model_supports_reasoning = capabilities.supports_reasoning;
    let model_reasoning_profile = selected_model
        .as_ref()
        .and_then(|model| model.reasoning_profile.clone());
    let model_interleaved_reasoning = capabilities.interleaved_reasoning.clone();
    let credential_source = resolve_credential_source_summary(options, &capabilities);
    let provider_reasoning_profile = definition.reasoning_profile.clone();
    let openrouter_endpoint_route = (provider_id == "openrouter")
        .then(|| provider_config::openrouter_endpoint_route_for_model(config, model_id))
        .flatten();
    let reasoning = resolve_provider_runtime_reasoning(
        effective_runtime,
        provider_id,
        model_id,
        model_supports_reasoning,
        requested_reasoning_effort,
        provider_reasoning_profile.as_ref(),
        model_reasoning_profile.as_ref(),
    );

    Ok(ProviderRuntimeBlueprint {
        provider_id: provider_id.to_string(),
        provider_name: definition.name.clone(),
        runtime_kind: effective_runtime,
        runtime_label: effective_runtime.label().to_string(),
        model_id: model_id.to_string(),
        model_name,
        openrouter_endpoint_route,
        base_url: definition.resolved_base_url(options),
        supports_remote_model_refresh: definition.supports_remote_model_refresh(),
        credential_source,
        user_agent: definition.user_agent.clone(),
        extra_headers: definition.extra_headers.clone(),
        extra_body: Value::Object(definition.extra_body.clone()),
        reasoning,
        capabilities,
        interleaved_reasoning: model_interleaved_reasoning,
    })
}

/// Validates the required provider fields before execution starts.
/// 在执行开始前校验供应商要求的必填字段。
fn validate_required_provider_fields(
    fields: &[ProviderFieldKey],
    options: &PersistedProviderOptions,
    capabilities: &ResolvedModelCapabilities,
    provider_name: &str,
) -> Result<(), String> {
    for field in fields {
        match field {
            ProviderFieldKey::ApiKey => {
                if resolve_provider_api_key(options, capabilities)?.is_none() {
                    return Err(format!(
                        "供应商 `{provider_name}` 缺少可用的 API Key，无法启动执行。"
                    ));
                }
            }
            _ => {
                let value = options.get(*field);
                if value.trim().is_empty() {
                    return Err(format!(
                        "供应商 `{provider_name}` 缺少 `{}`，无法启动执行。",
                        field.label()
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_active_provider_blueprint_with_ai_tools;
    use crate::agent_runtime::ai_tool_router::{AiToolRouteKind, resolve_ai_tool_plan};
    use crate::agent_runtime::provider_native_tools::build_provider_native_ai_tool_additional_params;
    use crate::agent_runtime::provider_runtime_inputs::resolve_provider_additional_params;
    use crate::provider_config::{
        ModelInterleavedCapability, PersistedProviderEntry, PersistedProviderOptions,
        ProviderConfigurationFile, ReasoningEffortLevel,
    };
    use std::collections::BTreeMap;

    /// Verifies active provider-blueprint resolution preserves model-scoped interleaved reasoning metadata from the provider catalog.
    /// 验证激活 Provider 蓝图解析会保留供应商目录中声明的模型级交错推理元数据。
    #[test]
    fn active_provider_blueprint_preserves_interleaved_reasoning_capability() {
        let mut provider = BTreeMap::new();
        provider.insert(
            String::from("deepseek"),
            PersistedProviderEntry {
                name: Some(String::from("DeepSeek")),
                options: PersistedProviderOptions {
                    api_key: Some(String::from("test-key")),
                    ..PersistedProviderOptions::default()
                },
                ..PersistedProviderEntry::default()
            },
        );
        let config = ProviderConfigurationFile {
            model: Some(String::from("deepseek/deepseek-v4-pro")),
            provider,
            ..ProviderConfigurationFile::default()
        };

        let blueprint = resolve_active_provider_blueprint_with_ai_tools(
            &config,
            ReasoningEffortLevel::Off,
            &config.ai_tools,
        )
        .expect("provider blueprint should resolve");

        match blueprint.interleaved_reasoning {
            Some(ModelInterleavedCapability::Field(field)) => {
                assert_eq!(field.field, "reasoning_content");
            }
            other => panic!("unexpected interleaved reasoning capability: {other:?}"),
        }
        assert!(blueprint.capabilities.supports_tool_call);
        assert!(blueprint.capabilities.supports_temperature);
    }

    /// Verifies Alibaba Token Plan selects implicit native web search without injecting a web_search object into the OpenAI-compatible tools array.
    /// 验证阿里云 Token Plan 选择隐式原生联网搜索，同时不会向 OpenAI 兼容 tools 数组注入 web_search 对象。
    #[test]
    fn alibaba_token_plan_uses_implicit_native_web_search_without_tool_injection() {
        // provider contains the minimum persisted credential entry required to resolve the real embedded definition.
        // provider 包含解析真实内嵌定义所需的最小持久化凭据条目。
        let mut provider = BTreeMap::new();
        provider.insert(
            String::from("alibaba-token-plan-cn"),
            PersistedProviderEntry {
                name: Some(String::from("Alibaba Token Plan (China)")),
                options: PersistedProviderOptions {
                    api_key: Some(String::from("test-key")),
                    ..PersistedProviderOptions::default()
                },
                ..PersistedProviderEntry::default()
            },
        );
        // config selects the only Token Plan model while retaining the default Harness-tool routing configuration.
        // config 选择唯一的 Token Plan 模型，同时保留默认 Harness 工具路由配置。
        let config = ProviderConfigurationFile {
            model: Some(String::from("alibaba-token-plan-cn/qwen3.8-max-preview")),
            provider,
            ..ProviderConfigurationFile::default()
        };

        // blueprint follows the production resolver so the regression covers catalog parsing and effective capability resolution together.
        // blueprint 走生产解析器，因此该回归测试同时覆盖目录解析与生效能力解析。
        let blueprint = resolve_active_provider_blueprint_with_ai_tools(
            &config,
            ReasoningEffortLevel::XHigh,
            &config.ai_tools,
        )
        .expect("Token Plan provider blueprint should resolve");

        assert!(blueprint.capabilities.supports_tool_call);
        assert_eq!(blueprint.capabilities.ai_tools.len(), 1);
        assert!(blueprint.capabilities.ai_tools[0].native_provider_tool);
        assert!(
            blueprint.capabilities.ai_tools[0]
                .fallback_tool_id
                .is_none()
        );
        // ai_tool_plan must select the provider-native branch; only HostFallback decisions are registered as host tools.
        // ai_tool_plan 必须选择 Provider 原生分支；只有 HostFallback 决策才会注册为宿主工具。
        let ai_tool_plan = resolve_ai_tool_plan(&blueprint);
        assert_eq!(
            ai_tool_plan.decisions[0].route,
            AiToolRouteKind::ProviderNative
        );
        // native_patch is intentionally empty: it marks the server-side native route as available without mutating the request's tools array.
        // native_patch 刻意保持为空：它标记服务端原生路由可用，但不会修改请求中的 tools 数组。
        let native_patch = build_provider_native_ai_tool_additional_params(&blueprint)
            .expect("implicit native web search should resolve to an empty activation patch");
        assert_eq!(native_patch, serde_json::json!({}));
        // additional_params is the final provider-scoped fragment merged into the transport request and must not contain a tools array.
        // additional_params 是最终合并进传输请求的 Provider 作用域片段，绝不能包含 tools 数组。
        let additional_params = resolve_provider_additional_params(&blueprint)
            .expect("xhigh reasoning should produce additional parameters");
        assert_eq!(additional_params["reasoning_effort"], "xhigh");
        assert_eq!(additional_params["tool_stream"], true);
        assert!(additional_params.get("tools").is_none());
    }
}
