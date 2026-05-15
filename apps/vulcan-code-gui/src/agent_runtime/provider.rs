//! Provider/model runtime blueprint resolution used by the compose execution layer.
//! compose 执行层使用的 Provider/Model 运行时蓝图解析逻辑。

use crate::agent_runtime::provider_reasoning::{
    ProviderRuntimeReasoningResolution, resolve_provider_runtime_reasoning,
};
use crate::provider_capabilities::{
    ResolvedModelCapabilities, resolve_credential_source_summary, resolve_model_capabilities,
    resolve_provider_api_key,
};
use crate::provider_config::{
    self, ModelInterleavedCapability, PersistedProviderOptions, ProviderApiFamily,
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

/// Resolves the active provider/model pair into one validated runtime blueprint.
/// 将当前激活的 Provider/Model 对解析为一份通过校验的运行时蓝图。
pub fn resolve_active_provider_blueprint(
    config: &ProviderConfigurationFile,
    requested_reasoning_effort: ReasoningEffortLevel,
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
    let capabilities = resolve_model_capabilities(definition, selected_model.as_ref());
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
    let reasoning = resolve_provider_runtime_reasoning(
        definition.runtime,
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
        runtime_kind: definition.runtime,
        runtime_label: definition.runtime.label().to_string(),
        model_id: model_id.to_string(),
        model_name,
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
    use super::resolve_active_provider_blueprint;
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

        let blueprint = resolve_active_provider_blueprint(&config, ReasoningEffortLevel::Off)
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
}
