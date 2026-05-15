//! Provider capability resolution helpers shared by the runtime, diagnostics, and future UI guards.
//! 供运行时、诊断与后续界面防护共用的 Provider 能力解析辅助模块。

use crate::provider_config::{
    ModelChoice, ModelInterleavedCapability, PersistedProviderOptions, SupportedProviderDefinition,
};
use std::collections::BTreeSet;
use vulcan_core::types::ConversationAttachmentKind;

/// Resolved provider/model capability snapshot consumed by the host instead of raw catalog JSON.
/// 宿主消费的已解析 Provider/Model 能力快照，用于替代原始目录 JSON 细节。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedModelCapabilities {
    /// Ordered provider-declared credential environment keys used as fallback lookup candidates.
    /// 作为回退查询候选项使用的按顺序排列的供应商声明凭据环境变量键集合。
    pub credential_env_keys: Vec<String>,
    /// Whether the selected model declares reasoning support.
    /// 当前选中模型是否声明支持推理能力。
    pub supports_reasoning: bool,
    /// Whether the selected model declares tool-calling support.
    /// 当前选中模型是否声明支持工具调用能力。
    pub supports_tool_call: bool,
    /// Whether the selected model accepts explicit temperature overrides.
    /// 当前选中模型是否接受显式温度覆盖。
    pub supports_temperature: bool,
    /// Whether the selected model declares the catalog-level `attachment` flag kept for diagnostics and metadata parity.
    /// 当前选中模型是否声明目录级 `attachment` 标记，用于诊断与元数据对齐。
    pub supports_attachment: bool,
    /// Optional lifecycle status label such as `stable` or `deprecated`.
    /// 可选的生命周期状态标签，例如 `stable` 或 `deprecated`。
    pub status: Option<String>,
    /// Optional family label attached to the selected model.
    /// 附着在当前选中模型上的可选家族标签。
    pub family: Option<String>,
    /// Optional release-date label attached to the selected model.
    /// 附着在当前选中模型上的可选发布日期标签。
    pub release_date: Option<String>,
    /// Optional provider/model context-window limit.
    /// Provider/Model 可选上下文窗口上限。
    pub context_limit: Option<usize>,
    /// Optional provider/model input-token limit.
    /// Provider/Model 可选输入 Token 上限。
    pub input_limit: Option<usize>,
    /// Optional provider/model output-token limit.
    /// Provider/Model 可选输出 Token 上限。
    pub output_limit: Option<usize>,
    /// Optional model-scoped interleaved reasoning descriptor.
    /// 可选的模型级交错推理描述。
    pub interleaved_reasoning: Option<ModelInterleavedCapability>,
    /// Normalized input modalities declared by the selected model.
    /// 当前选中模型声明的规范化输入模态集合。
    pub input_modalities: Vec<String>,
    /// Normalized output modalities declared by the selected model.
    /// 当前选中模型声明的规范化输出模态集合。
    pub output_modalities: Vec<String>,
}

impl ResolvedModelCapabilities {
    /// Returns the effective input budget limit used by context-budget estimation and compaction checks.
    /// 返回供上下文预算估算与压缩检查使用的生效输入预算上限。
    pub fn effective_input_limit(&self) -> Option<usize> {
        self.input_limit.or(self.context_limit)
    }

    /// Returns the effective output reserve limit used by context-budget estimation.
    /// 返回供上下文预算估算使用的生效输出预留上限。
    pub fn effective_output_limit(&self) -> Option<usize> {
        self.output_limit
    }

    /// Returns whether the selected model still behaves as text-only from the host perspective.
    /// 返回从宿主视角看当前选中模型是否仍表现为纯文本模型。
    pub fn is_text_only(&self) -> bool {
        self.input_modalities.is_empty()
            || self
                .input_modalities
                .iter()
                .all(|modality| modality == "text")
    }

    /// Returns whether the selected model declares support for one specific normalized input modality.
    /// 返回当前选中模型是否声明支持某个指定的规范化输入模态。
    pub fn supports_input_modality(&self, modality: &str) -> bool {
        let normalized_modality = modality.trim().to_ascii_lowercase();
        if normalized_modality.is_empty() {
            return false;
        }
        if normalized_modality == "text" {
            return self.input_modalities.is_empty()
                || self
                    .input_modalities
                    .iter()
                    .any(|candidate| candidate == "text");
        }
        self.input_modalities
            .iter()
            .any(|candidate| candidate == &normalized_modality)
    }

    /// Returns whether the selected model declares support for the modality carried by one staged attachment kind.
    /// 返回当前选中模型是否声明支持某条已暂存附件类型所携带的模态。
    pub fn supports_attachment_kind(&self, kind: ConversationAttachmentKind) -> bool {
        self.supports_input_modality(kind.modality_name())
    }

    /// Builds one compact lifecycle summary string for diagnostics surfaces.
    /// 为诊断表面构建一条紧凑生命周期摘要字符串。
    pub fn lifecycle_summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(status) = self.status.as_deref() {
            parts.push(format!("状态 `{status}`"));
        }
        if let Some(family) = self.family.as_deref() {
            parts.push(format!("家族 `{family}`"));
        }
        if let Some(release_date) = self.release_date.as_deref() {
            parts.push(format!("发布日期 `{release_date}`"));
        }
        if parts.is_empty() {
            String::from("未声明生命周期元数据")
        } else {
            parts.join(" · ")
        }
    }
}

/// Resolves one stable provider/model capability snapshot from the catalog definition and optional model entry.
/// 根据目录定义与可选模型条目解析一份稳定的 Provider/Model 能力快照。
pub fn resolve_model_capabilities(
    definition: &SupportedProviderDefinition,
    model: Option<&ModelChoice>,
) -> ResolvedModelCapabilities {
    ResolvedModelCapabilities {
        credential_env_keys: normalize_environment_key_list(&definition.credential_env_keys),
        supports_reasoning: model.and_then(|entry| entry.reasoning).unwrap_or(false),
        supports_tool_call: model.and_then(|entry| entry.tool_call).unwrap_or(true),
        supports_temperature: model.and_then(|entry| entry.temperature).unwrap_or(true),
        supports_attachment: model.and_then(|entry| entry.attachment).unwrap_or(false),
        status: model.and_then(|entry| trimmed_option(entry.status.as_deref())),
        family: model.and_then(|entry| trimmed_option(entry.family.as_deref())),
        release_date: model.and_then(|entry| trimmed_option(entry.release_date.as_deref())),
        context_limit: model.and_then(|entry| entry.context_limit),
        input_limit: model.and_then(|entry| entry.input_limit),
        output_limit: model.and_then(|entry| entry.output_limit),
        interleaved_reasoning: model.and_then(|entry| entry.interleaved.clone()),
        input_modalities: model
            .map(|entry| normalize_modality_list(&entry.input_modalities))
            .unwrap_or_default(),
        output_modalities: model
            .map(|entry| normalize_modality_list(&entry.output_modalities))
            .unwrap_or_default(),
    }
}

/// Resolves the effective API key by preferring the persisted secret field and then falling back to provider-declared environment keys.
/// 通过优先使用持久化密钥字段、再回退到供应商声明环境变量键的顺序解析生效 API Key。
pub fn resolve_provider_api_key(
    options: &PersistedProviderOptions,
    capabilities: &ResolvedModelCapabilities,
) -> Result<Option<String>, String> {
    resolve_provider_api_key_with_lookup(options, capabilities, |env_name| {
        resolve_named_environment_secret(env_name, "API Key")
    })
}

/// Builds one human-readable credential-source summary that includes provider-declared environment-key fallback details.
/// 构建一条包含供应商声明环境变量回退细节的人类可读凭据来源摘要。
pub fn resolve_credential_source_summary(
    options: &PersistedProviderOptions,
    capabilities: &ResolvedModelCapabilities,
) -> String {
    if let Some(raw_value) = options
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return parse_env_placeholder_name(raw_value)
            .map(|env_name| format!("环境变量 `{env_name}`"))
            .unwrap_or_else(|| String::from("本地配置"));
    }

    if let Some(env_name) = first_available_environment_key(&capabilities.credential_env_keys) {
        return format!("环境变量 `{env_name}`");
    }

    if capabilities.credential_env_keys.is_empty() {
        String::from("未配置")
    } else {
        format!(
            "候选环境变量 {}",
            capabilities
                .credential_env_keys
                .iter()
                .map(|value| format!("`{value}`"))
                .collect::<Vec<_>>()
                .join("、")
        )
    }
}

/// Returns the first provider-declared environment key that currently resolves to a non-empty process value.
/// 返回当前能解析到非空进程值的第一条供应商声明环境变量键。
fn first_available_environment_key(env_names: &[String]) -> Option<&str> {
    env_names.iter().find_map(|env_name| {
        std::env::var(env_name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|_| env_name.as_str())
    })
}

/// Resolves one plain environment variable into a trimmed secret string while tolerating missing or empty variables as non-fatal fallback misses.
/// 将单个普通环境变量解析为去空白后的密钥字符串，并把缺失或空变量视为非致命回退未命中。
fn resolve_named_environment_secret(
    env_name: &str,
    field_label: &str,
) -> Result<Option<String>, String> {
    match std::env::var(env_name) {
        Ok(value) => {
            let resolved_value = value.trim();
            if resolved_value.is_empty() {
                Ok(None)
            } else {
                Ok(Some(resolved_value.to_string()))
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!(
            "{field_label} 的环境变量 `{env_name}` 不是合法 UTF-8，当前无法作为宿主凭据使用。"
        )),
    }
}

/// Resolves the effective API key with one caller-provided environment lookup so tests can verify fallback order without mutating global process state.
/// 通过调用方提供的环境变量查询函数解析生效 API Key，从而让测试无需修改全局进程状态也能验证回退顺序。
fn resolve_provider_api_key_with_lookup<F>(
    options: &PersistedProviderOptions,
    capabilities: &ResolvedModelCapabilities,
    mut lookup_env: F,
) -> Result<Option<String>, String>
where
    F: FnMut(&str) -> Result<Option<String>, String>,
{
    if let Some(api_key) = options.resolved_api_key()? {
        return Ok(Some(api_key));
    }

    for env_name in &capabilities.credential_env_keys {
        if let Some(api_key) = lookup_env(env_name)? {
            return Ok(Some(api_key));
        }
    }

    Ok(None)
}

/// Normalizes one ordered modality list by trimming values, lower-casing entries, and removing duplicates while preserving order.
/// 通过裁剪空白、统一转换为小写并按顺序去重来规范化一组有序模态列表。
fn normalize_modality_list(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();

    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = trimmed.to_ascii_lowercase();
        if seen.insert(candidate.clone()) {
            normalized.push(candidate);
        }
    }

    normalized
}

/// Normalizes one ordered environment-key list by trimming values and removing exact duplicates while preserving the declared case.
/// 通过裁剪空白并在保留声明大小写的前提下按顺序去除完全重复项来规范化一组有序环境变量键列表。
fn normalize_environment_key_list(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();

    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = trimmed.to_string();
        if seen.insert(candidate.clone()) {
            normalized.push(candidate);
        }
    }

    normalized
}

/// Returns one trimmed optional string clone when the source is not blank.
/// 当源值不为空白时返回一份经过裁剪的可选字符串副本。
fn trimmed_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

/// Parses one `{env:NAME}` placeholder into its referenced environment-variable name.
/// 将一个 `{env:NAME}` 占位符解析为它引用的环境变量名称。
fn parse_env_placeholder_name(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("{env:")?.strip_suffix('}')?.trim();
    (!inner.is_empty()).then_some(inner)
}

#[cfg(test)]
mod tests {
    //! Capability-resolution regression tests covering credential fallback and model field normalization.
    //! 覆盖凭据回退与模型字段规范化的能力解析回归测试。

    use super::{
        ResolvedModelCapabilities, resolve_credential_source_summary, resolve_model_capabilities,
        resolve_provider_api_key, resolve_provider_api_key_with_lookup,
    };
    use crate::provider_config::{
        ModelChoice, PersistedProviderOptions, ProviderBaseUrlConfig, ProviderBaseUrlKind,
        ProviderModelDiscoveryConfig, ProviderModelDiscoveryKind, ProviderRuntimeKind,
        SupportedProviderDefinition,
    };
    use std::collections::BTreeMap;

    /// Builds one minimal provider definition for capability-resolution tests.
    /// 为能力解析测试构建一份最小 Provider 定义。
    fn test_provider_definition() -> SupportedProviderDefinition {
        SupportedProviderDefinition {
            id: String::from("deepseek"),
            name: String::from("DeepSeek"),
            runtime: ProviderRuntimeKind::OpenAiCompatible,
            user_agent: None,
            credential_env_keys: vec![String::from("DEEPSEEK_API_KEY")],
            extra_headers: BTreeMap::new(),
            extra_body: serde_json::Map::new(),
            reasoning_profile: None,
            fields: Vec::new(),
            base_url: ProviderBaseUrlConfig {
                kind: ProviderBaseUrlKind::Fixed,
                allow_override: false,
                value: Some(String::from("https://api.example.test")),
                template: None,
                parameter: None,
            },
            model_discovery: ProviderModelDiscoveryConfig {
                kind: ProviderModelDiscoveryKind::OpenAiModels,
            },
            models: Vec::new(),
        }
    }

    /// Preserves declared model flags and normalizes modality strings into stable lowercase host values.
    /// 保留声明的模型能力标记，并把模态字符串规范化为稳定的小写宿主值。
    #[test]
    fn resolve_model_capabilities_preserves_flags_and_normalizes_modalities() {
        let definition = test_provider_definition();
        let model = ModelChoice {
            id: String::from("deepseek-v4-pro"),
            name: String::from("DeepSeek V4 Pro"),
            status: Some(String::from("stable")),
            family: Some(String::from("thinking")),
            release_date: Some(String::from("2026-04-24")),
            context_limit: Some(128_000),
            input_limit: Some(96_000),
            output_limit: Some(8_192),
            reasoning: Some(true),
            tool_call: Some(false),
            temperature: Some(false),
            attachment: Some(true),
            interleaved: None,
            reasoning_profile: None,
            input_modalities: vec![
                String::from("Text"),
                String::from(" Image "),
                String::from("text"),
            ],
            output_modalities: vec![String::from("Text"), String::from("text")],
        };

        let capabilities = resolve_model_capabilities(&definition, Some(&model));

        assert!(capabilities.supports_reasoning);
        assert!(!capabilities.supports_tool_call);
        assert!(!capabilities.supports_temperature);
        assert!(capabilities.supports_attachment);
        assert_eq!(capabilities.context_limit, Some(128_000));
        assert_eq!(capabilities.input_modalities, vec!["text", "image"]);
        assert_eq!(capabilities.output_modalities, vec!["text"]);
        assert!(capabilities.supports_input_modality("text"));
        assert!(capabilities.supports_input_modality("image"));
        assert!(!capabilities.supports_input_modality("video"));
        assert!(
            capabilities
                .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
        );
    }

    /// Treats `attachment` as catalog metadata while using input modalities as the host-side attachment gate.
    /// 将 `attachment` 视为目录元数据，同时使用输入模态作为宿主侧附件门控依据。
    #[test]
    fn resolve_model_capabilities_uses_modalities_as_attachment_gate() {
        let definition = test_provider_definition();
        let model = ModelChoice {
            id: String::from("qwen3.6-plus"),
            name: String::from("Qwen3.6 Plus"),
            status: None,
            family: None,
            release_date: None,
            context_limit: None,
            input_limit: None,
            output_limit: None,
            reasoning: Some(true),
            tool_call: Some(true),
            temperature: Some(true),
            attachment: Some(false),
            interleaved: None,
            reasoning_profile: None,
            input_modalities: vec![
                String::from("text"),
                String::from("image"),
                String::from("video"),
            ],
            output_modalities: vec![String::from("text")],
        };

        let capabilities = resolve_model_capabilities(&definition, Some(&model));

        assert!(!capabilities.supports_attachment);
        assert!(
            capabilities
                .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
        );
        assert!(
            capabilities
                .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Video)
        );
        assert!(
            !capabilities
                .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Audio)
        );
    }

    /// Falls back to provider-declared environment variables when no local API-key field is persisted.
    /// 当本地未持久化 API Key 字段时，回退到供应商声明的环境变量。
    #[test]
    fn resolve_provider_api_key_falls_back_to_provider_declared_env_key() {
        let definition = test_provider_definition();
        let capabilities = resolve_model_capabilities(&definition, None);
        let options = PersistedProviderOptions::default();
        let api_key = resolve_provider_api_key_with_lookup(&options, &capabilities, |env_name| {
            Ok((env_name == "DEEPSEEK_API_KEY").then(|| String::from("fallback-key")))
        })
        .expect("credential fallback");

        let public_api_key =
            resolve_provider_api_key(&options, &ResolvedModelCapabilities::default())
                .expect("public resolver stays non-fatal without fallback env");

        assert_eq!(public_api_key, None);
        assert_eq!(api_key.as_deref(), Some("fallback-key"));
    }

    /// Surfaces provider-declared fallback keys in the credential summary when no explicit local secret is configured.
    /// 当未配置显式本地密钥时，在凭据摘要中展示供应商声明的回退环境变量键。
    #[test]
    fn resolve_credential_source_summary_surfaces_fallback_env_keys() {
        let definition = test_provider_definition();
        let capabilities = resolve_model_capabilities(&definition, None);
        let options = PersistedProviderOptions::default();

        let summary = resolve_credential_source_summary(&options, &capabilities);

        assert!(summary.contains("DEEPSEEK_API_KEY"));
    }
}
