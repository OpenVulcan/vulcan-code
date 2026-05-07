//! Provider catalog, persistence, and remote model helper logic.
//! 模型供应商目录、持久化与远端模型辅助逻辑。

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Provider identifiers that are intentionally excluded because their protocol or auth flow is not supported yet.
/// 因当前尚未支持其协议或认证流程而被刻意排除的供应商标识集合。
const UNSUPPORTED_PROVIDER_IDS: &[&str] = &[
    "amazon-bedrock",
    "cloudflare-ai-gateway",
    "github-copilot",
    "gitlab",
    "google-vertex",
    "google-vertex-anthropic",
    "kiro",
    "opencode",
    "opencode-go",
    "sap-ai-core",
    "v0",
    "vercel",
];

/// Field identifiers rendered in the provider configuration form.
/// 渲染在供应商配置表单中的字段标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderFieldKey {
    /// Shared API key or token input.
    /// 通用 API Key 或 Token 输入项。
    ApiKey,
    /// Optional manual base URL override input.
    /// 可选的手动基址覆盖输入项。
    BaseUrl,
    /// Azure resource name input.
    /// Azure 资源名称输入项。
    ResourceName,
    /// Cloudflare account identifier input.
    /// Cloudflare 账户标识输入项。
    AccountId,
}

impl ProviderFieldKey {
    /// Returns whether the input should be rendered as a secret field.
    /// 返回该输入是否应渲染为密文输入框。
    pub const fn is_secret(self) -> bool {
        matches!(self, Self::ApiKey)
    }

    /// Returns the stable user-facing field label.
    /// 返回稳定的面向用户字段标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::ApiKey => "API Key",
            Self::BaseUrl => "Base URL",
            Self::ResourceName => "Resource Name",
            Self::AccountId => "Account ID",
        }
    }

    /// Returns the default placeholder text for the field.
    /// 返回该字段的默认占位提示文本。
    pub const fn placeholder(self) -> &'static str {
        match self {
            Self::ApiKey => "请输入 API Key 或 Token",
            Self::BaseUrl => "https://api.example.com/v1",
            Self::ResourceName => "例如：my-models",
            Self::AccountId => "例如：1234567890abcdef1234567890abcdef",
        }
    }
}

/// Provider runtime family aligned with the current project and rig-facing integration model.
/// 与当前项目及 rig 接入模型对齐的供应商运行时族。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderRuntimeKind {
    /// OpenAI-compatible HTTP runtime.
    /// OpenAI 兼容 HTTP 运行时。
    #[serde(rename = "openai-compatible")]
    OpenAiCompatible,
    /// Native OpenAI runtime.
    /// 原生 OpenAI 运行时。
    #[serde(rename = "openai")]
    OpenAi,
    /// Native Anthropic runtime.
    /// 原生 Anthropic 运行时。
    #[serde(rename = "anthropic")]
    Anthropic,
    /// Native Azure OpenAI runtime.
    /// 原生 Azure OpenAI 运行时。
    #[serde(rename = "azure-openai")]
    AzureOpenAi,
    /// Native Google Gemini runtime.
    /// 原生 Google Gemini 运行时。
    #[serde(rename = "google-gemini")]
    GoogleGemini,
    /// Native xAI runtime.
    /// 原生 xAI 运行时。
    #[serde(rename = "xai")]
    XAi,
    /// Native Mistral runtime.
    /// 原生 Mistral 运行时。
    #[serde(rename = "mistral")]
    Mistral,
    /// Native Groq runtime.
    /// 原生 Groq 运行时。
    #[serde(rename = "groq")]
    Groq,
    /// Native Cohere runtime.
    /// 原生 Cohere 运行时。
    #[serde(rename = "cohere")]
    Cohere,
    /// Native Together AI runtime.
    /// 原生 Together AI 运行时。
    #[serde(rename = "together-ai")]
    TogetherAi,
    /// Native Perplexity runtime.
    /// 原生 Perplexity 运行时。
    #[serde(rename = "perplexity")]
    Perplexity,
    /// Native OpenRouter runtime.
    /// 原生 OpenRouter 运行时。
    #[serde(rename = "openrouter")]
    OpenRouter,
}

impl ProviderRuntimeKind {
    /// Returns a concise runtime label for the configuration page.
    /// 返回配置页面使用的精简运行时标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI-Compatible",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::AzureOpenAi => "Azure OpenAI",
            Self::GoogleGemini => "Google Gemini",
            Self::XAi => "xAI",
            Self::Mistral => "Mistral",
            Self::Groq => "Groq",
            Self::Cohere => "Cohere",
            Self::TogetherAi => "Together AI",
            Self::Perplexity => "Perplexity",
            Self::OpenRouter => "OpenRouter",
        }
    }
}

/// Base URL strategy used by one provider definition.
/// 单个供应商定义使用的基址策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderBaseUrlKind {
    /// The runtime resolves the endpoint internally.
    /// 由运行时内部解析接口地址。
    Native,
    /// A fixed default base URL is available.
    /// 存在固定的默认基址。
    Fixed,
    /// A base URL template is resolved from one form field.
    /// 基址模板需要从某个表单字段解析。
    Template,
}

/// Structured base URL configuration for one provider.
/// 单个供应商的结构化基址配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderBaseUrlConfig {
    /// Base URL resolution kind.
    /// 基址解析方式。
    pub kind: ProviderBaseUrlKind,
    /// Fixed default base URL when the provider has one.
    /// 当供应商存在固定默认基址时使用的默认值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Template string used to resolve a provider-specific endpoint.
    /// 用于解析供应商特定接口地址的模板字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Field key used by the template strategy.
    /// 模板策略使用的字段键。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<ProviderFieldKey>,
    /// Whether the project chooses to expose a manual override input for this provider.
    /// 当前项目是否选择为该供应商暴露手动覆盖输入项。
    pub allow_override: bool,
}

/// Structured model discovery strategy for one provider.
/// 单个供应商的结构化模型发现策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelDiscoveryConfig {
    /// Discovery kind used by the provider page.
    /// 供应商页面使用的发现方式。
    pub kind: ProviderModelDiscoveryKind,
}

/// Supported model discovery kinds used by the current GUI implementation.
/// 当前 GUI 实现支持的模型发现方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderModelDiscoveryKind {
    /// No remote discovery is attempted and the embedded catalog is used directly.
    /// 不尝试远端发现，直接使用内置目录。
    #[serde(rename = "none")]
    None,
    /// Uses a standard OpenAI-style `/models` endpoint.
    /// 使用标准 OpenAI 风格 `/models` 端点。
    #[serde(rename = "openai-models")]
    OpenAiModels,
}

impl ProviderModelDiscoveryKind {
    /// Returns whether the discovery strategy can actively refresh models from a remote endpoint.
    /// 返回该发现策略是否可以主动从远端端点刷新模型。
    pub const fn supports_remote_refresh(self) -> bool {
        matches!(self, Self::OpenAiModels)
    }
}

/// One selectable model item shown in the provider configuration page.
/// 展示在供应商配置页面中的单个可选模型项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    /// Stable model identifier persisted into the configuration file.
    /// 持久化到配置文件中的稳定模型标识。
    pub id: String,
    /// Human-readable model name rendered in the UI.
    /// 渲染在界面中的可读模型名称。
    pub name: String,
    /// Optional model lifecycle status.
    /// 可选的模型生命周期状态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional model family label.
    /// 可选的模型家族标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Optional model release date string.
    /// 可选的模型发布日期字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    /// Optional context window limit.
    /// 可选的上下文窗口上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Optional input token limit.
    /// 可选的输入 Token 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_limit: Option<usize>,
    /// Optional output token limit.
    /// 可选的输出 Token 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<usize>,
    /// Optional reasoning capability flag.
    /// 可选的推理能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Optional tool-calling capability flag.
    /// 可选的工具调用能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<bool>,
    /// Optional temperature support flag.
    /// 可选的温度参数支持标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<bool>,
    /// Optional attachment support flag.
    /// 可选的附件支持标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    /// Optional interleaved reasoning descriptor.
    /// 可选的交错推理描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interleaved: Option<ModelInterleavedCapability>,
    /// Optional input modality list.
    /// 可选的输入模态列表。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub input_modalities: Vec<String>,
    /// Optional output modality list.
    /// 可选的输出模态列表。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output_modalities: Vec<String>,
}

impl fmt::Display for ModelChoice {
    /// Renders a compact model label that keeps name and identifier inspectable.
    /// 渲染兼顾名称与标识可辨识度的紧凑模型标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.name == self.id {
            formatter.write_str(&self.name)
        } else {
            write!(formatter, "{} ({})", self.name, self.id)
        }
    }
}

/// Structured interleaved reasoning capability for one model entry.
/// 单个模型条目的结构化交错推理能力描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelInterleavedCapability {
    /// Boolean flag style interleaved capability.
    /// 布尔标记风格的交错推理能力。
    Enabled(bool),
    /// Field-mapped interleaved capability.
    /// 基于字段映射的交错推理能力。
    Field(ModelInterleavedField),
}

/// Field descriptor used by structured interleaved reasoning capabilities.
/// 结构化交错推理能力使用的字段描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInterleavedField {
    /// Response field carrying interleaved reasoning payloads.
    /// 承载交错推理载荷的响应字段。
    pub field: String,
}

/// One supported provider definition loaded from the split resource catalog.
/// 从分离资源目录中载入的单个受支持供应商定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedProviderDefinition {
    /// Stable provider identifier used in config and runtime routing.
    /// 用于配置与运行时路由的稳定供应商标识。
    pub id: String,
    /// Display name shown to the user.
    /// 展示给用户的显示名称。
    pub name: String,
    /// Provider runtime family aligned with the project integration model.
    /// 与项目接入模型对齐的供应商运行时族。
    pub runtime: ProviderRuntimeKind,
    /// Environment keys that may provide credentials for the provider.
    /// 可能为该供应商提供凭据的环境变量键集合。
    #[serde(default)]
    pub credential_env_keys: Vec<String>,
    /// Ordered field list exposed by the provider form.
    /// 供应商表单暴露的有序字段列表。
    #[serde(default)]
    pub fields: Vec<ProviderFieldKey>,
    /// Structured base URL resolution configuration.
    /// 结构化的基址解析配置。
    pub base_url: ProviderBaseUrlConfig,
    /// Structured model discovery configuration.
    /// 结构化的模型发现配置。
    pub model_discovery: ProviderModelDiscoveryConfig,
    /// Embedded fallback model catalog.
    /// 内置回退模型目录。
    #[serde(default)]
    pub models: Vec<ModelChoice>,
}

impl SupportedProviderDefinition {
    /// Returns the effective ordered field list used by the configuration form.
    /// 返回配置表单使用的生效有序字段列表。
    pub fn input_fields(&self) -> Vec<ProviderFieldKey> {
        let mut fields = self.fields.clone();

        if self.base_url.allow_override && !fields.contains(&ProviderFieldKey::BaseUrl) {
            fields.push(ProviderFieldKey::BaseUrl);
        }

        fields
    }

    /// Returns the placeholder text used by one provider-specific field.
    /// 返回某个供应商专属字段使用的占位提示文本。
    pub fn field_placeholder(&self, key: ProviderFieldKey) -> String {
        match key {
            ProviderFieldKey::BaseUrl => self
                .base_url
                .value
                .clone()
                .or_else(|| {
                    self.base_url
                        .template
                        .clone()
                        .map(|template| template.replace('{', "").replace('}', ""))
                })
                .unwrap_or_else(|| ProviderFieldKey::BaseUrl.placeholder().to_string()),
            _ => key.placeholder().to_string(),
        }
    }

    /// Returns the form label used by one provider-specific field.
    /// 返回某个供应商专属字段使用的表单标签。
    pub const fn field_label(&self, key: ProviderFieldKey) -> &'static str {
        key.label()
    }

    /// Returns whether the provider currently supports a remote model refresh path.
    /// 返回该供应商当前是否支持远端模型刷新路径。
    pub const fn supports_remote_model_refresh(&self) -> bool {
        self.model_discovery.kind.supports_remote_refresh()
    }

    /// Resolves the current base URL using the persisted options and provider strategy.
    /// 使用持久化选项与供应商策略解析当前基址。
    pub fn resolved_base_url(&self, options: &PersistedProviderOptions) -> Option<String> {
        if let Some(override_base_url) = options
            .base_url
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            return Some(override_base_url.to_string());
        }

        match self.base_url.kind {
            ProviderBaseUrlKind::Native => self.base_url.value.clone(),
            ProviderBaseUrlKind::Fixed => self.base_url.value.clone(),
            ProviderBaseUrlKind::Template => {
                let template = self.base_url.template.as_ref()?;
                let parameter = self.base_url.parameter?;
                let parameter_value = options.get(parameter).trim().to_string();

                if parameter_value.is_empty() {
                    return None;
                }

                let token = format!("{{{}}}", template_parameter_name(parameter));
                Some(template.replace(&token, &parameter_value))
            }
        }
    }
}

/// Provider selection option rendered in the provider pick list.
/// 渲染在供应商下拉选择框中的供应商选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderChoice {
    /// Stable provider identifier.
    /// 稳定的供应商标识。
    pub id: String,
    /// Display label shown in the picker.
    /// 展示在选择框中的显示标签。
    pub name: String,
}

impl fmt::Display for ProviderChoice {
    /// Renders a user-facing provider picker label.
    /// 渲染面向用户的供应商选择框标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

/// Persisted provider option values aligned with the Vulcan configuration file.
/// 与 Vulcan 配置文件对齐的持久化供应商选项值。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedProviderOptions {
    /// Stored API key or token.
    /// 已保存的 API Key 或 Token。
    #[serde(rename = "apiKey", skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Stored custom base URL override when the project enables it.
    /// 当项目启用时保存的自定义基址覆盖值。
    #[serde(rename = "baseURL", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Stored Azure resource name.
    /// 已保存的 Azure 资源名称。
    #[serde(rename = "resourceName", skip_serializing_if = "Option::is_none")]
    pub resource_name: Option<String>,
    /// Stored Cloudflare account identifier.
    /// 已保存的 Cloudflare 账户标识。
    #[serde(rename = "accountId", skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

impl PersistedProviderOptions {
    /// Returns whether the options object is effectively empty.
    /// 返回该 options 对象是否实际上为空。
    pub fn is_empty(&self) -> bool {
        self.api_key.is_none()
            && self.base_url.is_none()
            && self.resource_name.is_none()
            && self.account_id.is_none()
    }

    /// Reads one field value using the normalized provider form key.
    /// 使用标准化的供应商表单字段键读取一个字段值。
    pub fn get(&self, key: ProviderFieldKey) -> String {
        match key {
            ProviderFieldKey::ApiKey => self.api_key.clone().unwrap_or_default(),
            ProviderFieldKey::BaseUrl => self.base_url.clone().unwrap_or_default(),
            ProviderFieldKey::ResourceName => self.resource_name.clone().unwrap_or_default(),
            ProviderFieldKey::AccountId => self.account_id.clone().unwrap_or_default(),
        }
    }

    /// Writes one field value using the normalized provider form key.
    /// 使用标准化的供应商表单字段键写入一个字段值。
    pub fn set(&mut self, key: ProviderFieldKey, value: String) {
        let trimmed = value.trim().to_string();
        let next = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };

        match key {
            ProviderFieldKey::ApiKey => self.api_key = next,
            ProviderFieldKey::BaseUrl => self.base_url = next,
            ProviderFieldKey::ResourceName => self.resource_name = next,
            ProviderFieldKey::AccountId => self.account_id = next,
        }
    }

    /// Resolves the stored API key into a runtime-ready credential string.
    /// 将已保存的 API Key 解析为运行时可直接使用的凭据字符串。
    pub fn resolved_api_key(&self) -> Result<Option<String>, String> {
        resolve_env_backed_secret(self.api_key.as_deref(), "API Key")
    }
}

/// Persisted model metadata cached after a successful remote refresh.
/// 成功远端刷新后缓存下来的持久化模型元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedModelEntry {
    /// Human-readable model name.
    /// 人类可读的模型名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Persisted provider entry written into the Vulcan configuration file.
/// 写入 Vulcan 配置文件的持久化供应商条目。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedProviderEntry {
    /// Human-readable provider name.
    /// 人类可读的供应商名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Provider option object aligned with the current project schema.
    /// 与当前项目 Schema 对齐的供应商 options 对象。
    #[serde(default, skip_serializing_if = "PersistedProviderOptions::is_empty")]
    pub options: PersistedProviderOptions,
    /// Cached model list used when the provider supports remote refresh.
    /// 当供应商支持远端刷新时缓存下来的模型列表。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, PersistedModelEntry>,
}

/// Root provider configuration file saved in the Vulcan home directory.
/// 保存到 Vulcan 家目录中的根级供应商配置文件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderConfigurationFile {
    /// Currently selected global model in `provider/model` form.
    /// 当前以 `provider/model` 形式保存的全局激活模型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Persisted provider map keyed by provider identifier.
    /// 以供应商标识为键保存的供应商映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider: BTreeMap<String, PersistedProviderEntry>,
}

/// Loads the supported provider catalog derived from the split provider resources.
/// 加载从分离供应商资源目录派生出来的受支持供应商目录。
pub fn supported_providers() -> &'static [SupportedProviderDefinition] {
    static PROVIDERS: OnceLock<Vec<SupportedProviderDefinition>> = OnceLock::new();

    PROVIDERS.get_or_init(load_supported_providers)
}

/// Returns the persisted Vulcan configuration file path.
/// 返回持久化 Vulcan 配置文件路径。
pub fn configuration_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_path_buf()
    });

    home.join(".vulcan").join("vulcan-code.json")
}

/// Loads the persisted provider configuration file from disk.
/// 从磁盘加载持久化的供应商配置文件。
pub fn load_configuration_file() -> Result<ProviderConfigurationFile, String> {
    let path = configuration_file_path();

    if !path.exists() {
        return Ok(ProviderConfigurationFile::default());
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取供应商配置文件失败：{}。", error))?;

    serde_json::from_str(&text).map_err(|error| format!("解析供应商配置文件失败：{}。", error))
}

/// Saves the provider configuration file back to disk, creating parent directories when needed.
/// 将供应商配置文件写回磁盘，并在需要时创建父目录。
pub fn save_configuration_file(config: &ProviderConfigurationFile) -> Result<(), String> {
    let path = configuration_file_path();
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    fs::create_dir_all(&parent).map_err(|error| format!("创建配置目录失败：{}。", error))?;

    let content = serde_json::to_string_pretty(config)
        .map_err(|error| format!("序列化供应商配置失败：{}。", error))?;

    fs::write(&path, content).map_err(|error| format!("写入供应商配置失败：{}。", error))
}

/// Resolves the supported provider definition by identifier.
/// 根据标识解析受支持的供应商定义。
pub fn supported_provider_by_id(provider_id: &str) -> Option<&'static SupportedProviderDefinition> {
    supported_providers()
        .iter()
        .find(|definition| definition.id == provider_id)
}

/// Builds picker choices from the supported provider catalog.
/// 从受支持供应商目录构建选择器选项。
pub fn supported_provider_choices() -> Vec<ProviderChoice> {
    supported_providers()
        .iter()
        .map(|definition| ProviderChoice {
            id: definition.id.clone(),
            name: definition.name.clone(),
        })
        .collect()
}

/// Resolves the effective model choices for one provider using persisted cache or embedded fallback models.
/// 使用持久化缓存或内置回退模型为某个供应商解析生效模型列表。
pub fn effective_model_choices(
    provider_id: &str,
    config: &ProviderConfigurationFile,
) -> Vec<ModelChoice> {
    let embedded_models = supported_provider_by_id(provider_id)
        .map(|definition| definition.models.clone())
        .unwrap_or_default();

    if let Some(cached) = config.provider.get(provider_id) {
        if !cached.models.is_empty() {
            let items = cached
                .models
                .iter()
                .map(|(id, model)| ModelChoice {
                    id: id.clone(),
                    name: model.name.clone().unwrap_or_else(|| id.clone()),
                    status: None,
                    family: None,
                    release_date: None,
                    context_limit: None,
                    input_limit: None,
                    output_limit: None,
                    reasoning: None,
                    tool_call: None,
                    temperature: None,
                    attachment: None,
                    interleaved: None,
                    input_modalities: Vec::new(),
                    output_modalities: Vec::new(),
                })
                .collect::<Vec<_>>();
            let mut items = merge_embedded_model_metadata(items, &embedded_models);
            items.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
            return items;
        }
    }

    embedded_models
}

/// Parses the active provider/model tuple from the persisted global model string.
/// 从持久化的全局模型字符串中解析激活的供应商与模型二元组。
pub fn parse_active_model_reference(model: &str) -> Option<(&str, &str)> {
    let (provider_id, model_id) = model.split_once('/')?;
    Some((provider_id, model_id))
}

/// Fetches a remote model list using the provider definition and the current persisted options.
/// 使用供应商定义与当前持久化选项拉取远端模型列表。
pub async fn fetch_remote_models(
    definition: &SupportedProviderDefinition,
    options: &PersistedProviderOptions,
) -> Result<Vec<ModelChoice>, String> {
    let base_url = remote_models_url(definition, options)
        .ok_or_else(|| String::from("当前供应商不支持远端模型刷新。"))?;

    let mut headers = HeaderMap::new();

    if let Some(api_key) = options.resolved_api_key()? {
        let header_value = HeaderValue::from_str(&format!("Bearer {}", api_key))
            .map_err(|error| format!("构建认证请求头失败：{}。", error))?;
        headers.insert(AUTHORIZATION, header_value);
    }

    let response = reqwest::Client::new()
        .get(base_url)
        .headers(headers)
        .send()
        .await
        .map_err(|error| format!("请求模型列表失败：{}。", error))?;

    if !response.status().is_success() {
        return Err(format!("模型列表接口返回状态异常：{}。", response.status()));
    }

    let payload = response
        .json::<serde_json::Value>()
        .await
        .map_err(|error| format!("解析模型列表响应失败：{}。", error))?;

    let choices = extract_model_choices_from_value(&payload);

    if choices.is_empty() {
        return Err(String::from("未从远端接口解析到任何模型。"));
    }

    Ok(choices)
}

/// Converts a successful remote model refresh into a persisted provider cache map.
/// 将一次成功的远端模型刷新结果转换为可持久化的供应商缓存映射。
pub fn persist_remote_models(models: &[ModelChoice]) -> BTreeMap<String, PersistedModelEntry> {
    models
        .iter()
        .map(|model| {
            (
                model.id.clone(),
                PersistedModelEntry {
                    name: Some(model.name.clone()),
                },
            )
        })
        .collect()
}

/// Returns a human-friendly provider summary used in status text and logs.
/// 返回用于状态文本与日志的人类可读供应商摘要。
pub fn provider_summary(provider_id: &str) -> String {
    supported_provider_by_id(provider_id)
        .map(|definition| definition.name.clone())
        .unwrap_or_else(|| provider_id.to_string())
}

/// Returns a human-friendly model summary used in status text and logs.
/// 返回用于状态文本与日志的人类可读模型摘要。
pub fn model_summary(
    provider_id: &str,
    model_id: &str,
    config: &ProviderConfigurationFile,
) -> String {
    effective_model_choices(provider_id, config)
        .into_iter()
        .find(|model| model.id == model_id)
        .map(|model| model.to_string())
        .unwrap_or_else(|| model_id.to_string())
}

/// Loads and sorts the split provider catalog entries from the resources directory.
/// 从资源目录中加载并排序分离的供应商目录条目。
fn load_supported_providers() -> Vec<SupportedProviderDefinition> {
    let directory = provider_catalog_directory_path();
    let mut definitions = fs::read_dir(&directory)
        .unwrap_or_else(|error| {
            panic!(
                "provider catalog directory must exist at `{}`: {error}",
                directory.display()
            )
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|entry| {
            let path = entry.path();
            let content = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "provider catalog resource `{}` must stay readable: {error}",
                    path.display()
                )
            });
            let definition = serde_json::from_str::<SupportedProviderDefinition>(&content)
                .unwrap_or_else(|error| {
                    panic!(
                        "provider catalog resource `{}` must stay valid: {error}",
                        path.display()
                    )
                });

            if UNSUPPORTED_PROVIDER_IDS
                .iter()
                .any(|unsupported| *unsupported == definition.id)
            {
                None
            } else {
                Some(definition)
            }
        })
        .collect::<Vec<_>>();

    definitions.sort_by(|left, right| provider_sort_key(left).cmp(&provider_sort_key(right)));
    definitions
}

/// Returns the directory path containing split provider catalog files.
/// 返回包含分离供应商目录文件的目录路径。
fn provider_catalog_directory_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("providers")
}

/// Returns a sortable provider key that keeps common providers near the top of the picker.
/// 返回可排序的供应商键值，以便将常用供应商放到选择器顶部。
fn provider_sort_key(provider: &SupportedProviderDefinition) -> (u8, String) {
    let priority = match provider.id.as_str() {
        "openai" => 0,
        "anthropic" => 1,
        "deepseek" => 2,
        "openrouter" => 3,
        "google" => 4,
        "azure" => 5,
        _ => 99,
    };

    (priority, provider.name.clone())
}

/// Returns the token name used inside one base URL template.
/// 返回单个基址模板内部使用的参数占位符名称。
fn template_parameter_name(field: ProviderFieldKey) -> &'static str {
    match field {
        ProviderFieldKey::ApiKey => "apiKey",
        ProviderFieldKey::BaseUrl => "baseUrl",
        ProviderFieldKey::ResourceName => "resourceName",
        ProviderFieldKey::AccountId => "accountId",
    }
}

/// Builds the remote `/models` endpoint URL when the provider can try a standard refresh flow.
/// 当供应商可以尝试标准刷新流程时，构建远端 `/models` 端点地址。
fn remote_models_url(
    definition: &SupportedProviderDefinition,
    options: &PersistedProviderOptions,
) -> Option<String> {
    if !definition.supports_remote_model_refresh() {
        return None;
    }

    let base = definition.resolved_base_url(options)?;
    let normalized = base
        .trim_end_matches('/')
        .trim_end_matches("/chat/completions")
        .trim_end_matches("/responses")
        .trim_end_matches("/completions")
        .trim_end_matches("/messages")
        .to_string();

    Some(format!("{}/models", normalized.trim_end_matches('/')))
}

/// Extracts model choices from a loosely shaped OpenAI-style `/models` response payload.
/// 从宽松形状的 OpenAI 风格 `/models` 响应载荷中提取模型项。
fn extract_model_choices_from_value(value: &serde_json::Value) -> Vec<ModelChoice> {
    let mut choices = match value {
        serde_json::Value::Object(object) => object
            .get("data")
            .and_then(serde_json::Value::as_array)
            .map(|items| extract_models_from_array(items))
            .unwrap_or_default(),
        serde_json::Value::Array(array) => extract_models_from_array(array),
        _ => Vec::new(),
    };

    choices.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    choices.dedup_by(|left, right| left.id == right.id);
    choices
}

/// Resolves a secret field that may reference one environment variable placeholder.
/// 解析一个可能引用环境变量占位符的敏感字段。
fn resolve_env_backed_secret(
    raw_value: Option<&str>,
    field_label: &str,
) -> Result<Option<String>, String> {
    let Some(trimmed_value) = raw_value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let Some(env_name) = parse_env_placeholder(trimmed_value) else {
        if looks_like_env_placeholder(trimmed_value) {
            return Err(format!(
                "{field_label} 的环境变量占位符格式无效，请使用 `{{env:NAME}}`。"
            ));
        }

        return Ok(Some(trimmed_value.to_string()));
    };

    let env_value = std::env::var(env_name).map_err(|_| {
        format!("{field_label} 引用了环境变量 `{env_name}`，但当前进程中未找到该变量。")
    })?;
    let resolved_value = env_value.trim();

    if resolved_value.is_empty() {
        return Err(format!(
            "{field_label} 引用了环境变量 `{env_name}`，但该变量当前为空。"
        ));
    }

    Ok(Some(resolved_value.to_string()))
}

/// Parses one `{env:NAME}` placeholder into the referenced environment variable name.
/// 将一个 `{env:NAME}` 占位符解析为引用的环境变量名称。
fn parse_env_placeholder(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("{env:")?.strip_suffix('}')?.trim();

    if inner.is_empty() { None } else { Some(inner) }
}

/// Returns whether one value looks like an environment placeholder envelope.
/// 返回某个值是否看起来像环境变量占位符外壳。
fn looks_like_env_placeholder(value: &str) -> bool {
    value.starts_with("{env:") && value.ends_with('}')
}

/// Extracts model choices from one response array node.
/// 从单个响应数组节点中提取模型项。
fn extract_models_from_array(array: &[serde_json::Value]) -> Vec<ModelChoice> {
    array
        .iter()
        .filter_map(|item| {
            if let Some(id) = item.as_str() {
                return Some(ModelChoice {
                    id: id.to_string(),
                    name: id.to_string(),
                    status: None,
                    family: None,
                    release_date: None,
                    context_limit: None,
                    input_limit: None,
                    output_limit: None,
                    reasoning: None,
                    tool_call: None,
                    temperature: None,
                    attachment: None,
                    interleaved: None,
                    input_modalities: Vec::new(),
                    output_modalities: Vec::new(),
                });
            }

            let object = item.as_object()?;
            let id = object.get("id")?.as_str()?.to_string();
            let name = object
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&id)
                .to_string();

            Some(ModelChoice {
                id,
                name,
                status: None,
                family: None,
                release_date: None,
                context_limit: None,
                input_limit: None,
                output_limit: None,
                reasoning: None,
                tool_call: None,
                temperature: None,
                attachment: None,
                interleaved: None,
                input_modalities: Vec::new(),
                output_modalities: Vec::new(),
            })
        })
        .collect()
}

/// Merges embedded model metadata back into dynamically discovered model entries by stable model id.
/// 依据稳定模型标识将内置模型元数据重新合并到动态发现的模型条目中。
fn merge_embedded_model_metadata(
    discovered_models: Vec<ModelChoice>,
    embedded_models: &[ModelChoice],
) -> Vec<ModelChoice> {
    let embedded_lookup = embedded_models
        .iter()
        .map(|model| (model.id.as_str(), model))
        .collect::<BTreeMap<_, _>>();

    discovered_models
        .into_iter()
        .map(|model| {
            let Some(embedded) = embedded_lookup.get(model.id.as_str()) else {
                return model;
            };

            ModelChoice {
                id: model.id,
                name: model.name,
                status: embedded.status.clone(),
                family: embedded.family.clone(),
                release_date: embedded.release_date.clone(),
                context_limit: embedded.context_limit,
                input_limit: embedded.input_limit,
                output_limit: embedded.output_limit,
                reasoning: embedded.reasoning,
                tool_call: embedded.tool_call,
                temperature: embedded.temperature,
                attachment: embedded.attachment,
                interleaved: embedded.interleaved.clone(),
                input_modalities: embedded.input_modalities.clone(),
                output_modalities: embedded.output_modalities.clone(),
            }
        })
        .collect()
}
