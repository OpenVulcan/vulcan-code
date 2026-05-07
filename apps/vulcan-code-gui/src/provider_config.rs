//! Provider configuration catalog, persistence, and remote model helpers.
//! 模型供应商配置目录、持久化与远端模型辅助逻辑。

use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Embedded provider catalog exported from the OpenCode analysis snapshot.
/// 从 OpenCode 分析快照导出的内置供应商目录。
const PROVIDER_CATALOG_JSON: &str = include_str!("../resources/provider-catalog.json");

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFieldKey {
    /// Shared API key or token input.
    /// 通用 API Key 或 Token 输入项。
    ApiKey,
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
}

/// Supported protocol families mirrored from the OpenCode provider catalog.
/// 从 OpenCode 供应商目录镜像过来的受支持协议族。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedProtocol {
    /// OpenAI-compatible HTTP protocol.
    /// OpenAI 兼容 HTTP 协议。
    OpenAiCompatible,
    /// Native OpenAI protocol.
    /// 原生 OpenAI 协议。
    OpenAi,
    /// Native Anthropic protocol.
    /// 原生 Anthropic 协议。
    Anthropic,
    /// Azure OpenAI protocol.
    /// Azure OpenAI 协议。
    Azure,
    /// Native Google Gemini protocol.
    /// 原生 Google Gemini 协议。
    Google,
    /// Native xAI protocol.
    /// 原生 xAI 协议。
    XAi,
    /// Native Mistral protocol.
    /// 原生 Mistral 协议。
    Mistral,
    /// Native Groq protocol.
    /// 原生 Groq 协议。
    Groq,
    /// Native Cohere protocol.
    /// 原生 Cohere 协议。
    Cohere,
    /// Native Together protocol.
    /// 原生 Together 协议。
    TogetherAi,
    /// Native Perplexity protocol.
    /// 原生 Perplexity 协议。
    Perplexity,
    /// OpenRouter protocol.
    /// OpenRouter 协议。
    OpenRouter,
}

impl SupportedProtocol {
    /// Maps one npm provider package into the supported runtime protocol family.
    /// 将一个 npm provider 包映射为受支持的运行时协议族。
    pub fn from_npm(npm: &str) -> Option<Self> {
        match npm {
            "@ai-sdk/openai-compatible" => Some(Self::OpenAiCompatible),
            "@ai-sdk/openai" => Some(Self::OpenAi),
            "@ai-sdk/anthropic" => Some(Self::Anthropic),
            "@ai-sdk/azure" => Some(Self::Azure),
            "@ai-sdk/google" => Some(Self::Google),
            "@ai-sdk/xai" => Some(Self::XAi),
            "@ai-sdk/mistral" => Some(Self::Mistral),
            "@ai-sdk/groq" => Some(Self::Groq),
            "@ai-sdk/cohere" => Some(Self::Cohere),
            "@ai-sdk/togetherai" => Some(Self::TogetherAi),
            "@ai-sdk/perplexity" => Some(Self::Perplexity),
            "@openrouter/ai-sdk-provider" => Some(Self::OpenRouter),
            _ => None,
        }
    }

    /// Returns a concise user-facing protocol label.
    /// 返回面向用户的精简协议标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI-Compatible",
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Azure => "Azure OpenAI",
            Self::Google => "Google Gemini",
            Self::XAi => "xAI",
            Self::Mistral => "Mistral",
            Self::Groq => "Groq",
            Self::Cohere => "Cohere",
            Self::TogetherAi => "Together AI",
            Self::Perplexity => "Perplexity",
            Self::OpenRouter => "OpenRouter",
        }
    }

    /// Returns whether the protocol can try a standard `/models` endpoint refresh.
    /// 返回该协议是否可以尝试标准 `/models` 端点刷新。
    pub const fn supports_model_fetch(self) -> bool {
        matches!(
            self,
            Self::OpenAiCompatible | Self::OpenAi | Self::OpenRouter
        )
    }
}

/// Static model source mode used by the provider definition.
/// 供应商定义使用的静态模型来源模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSourceMode {
    /// Uses the embedded model catalog directly.
    /// 直接使用内置模型目录。
    EmbeddedStatic,
    /// Tries remote refresh first and falls back to the embedded model catalog.
    /// 优先尝试远端刷新，失败后回退到内置模型目录。
    TryRemoteThenFallback,
}

impl ModelSourceMode {
    /// Returns a compact display label for the configuration page.
    /// 返回配置页面使用的紧凑展示标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::EmbeddedStatic => "内置静态目录",
            Self::TryRemoteThenFallback => "尝试远端刷新，失败回退静态目录",
        }
    }
}

/// One selectable model item shown in the provider configuration page.
/// 展示在供应商配置页面中的单个可选模型项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChoice {
    /// Stable model identifier persisted into the configuration file.
    /// 持久化到配置文件中的稳定模型标识。
    pub id: String,
    /// Human-readable model name rendered in the UI.
    /// 渲染在界面中的可读模型名称。
    pub name: String,
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

/// One supported provider definition derived from the embedded OpenCode catalog.
/// 从内置 OpenCode 目录派生出的单个受支持供应商定义。
#[derive(Debug, Clone)]
pub struct SupportedProviderDefinition {
    /// Stable provider identifier used in config and runtime routing.
    /// 用于配置与运行时路由的稳定供应商标识。
    pub id: String,
    /// Display name shown to the user.
    /// 展示给用户的显示名称。
    pub name: String,
    /// Supported protocol family mirrored from the catalog.
    /// 从目录镜像过来的受支持协议族。
    pub protocol: SupportedProtocol,
    /// Default API base URL when the provider exposes one in the catalog.
    /// 当目录中存在默认接口地址时使用的默认 API 基址。
    pub default_api: Option<String>,
    /// Environment variable names mirrored from the OpenCode catalog.
    /// 从 OpenCode 目录镜像过来的环境变量名称列表。
    pub env: Vec<String>,
    /// Static model source strategy used by the provider page.
    /// 供应商页面使用的静态模型来源策略。
    pub model_source_mode: ModelSourceMode,
    /// Fields that should be rendered for this provider and only this provider.
    /// 仅对该供应商渲染且必须遵循其逻辑的字段集合。
    pub fields: Vec<ProviderFieldKey>,
    /// Embedded fallback models shipped with the app.
    /// 随应用一起分发的内置回退模型列表。
    pub static_models: Vec<ModelChoice>,
    /// Short notes describing why the provider behaves specially.
    /// 描述该供应商特殊行为原因的简短说明。
    pub notes: Vec<String>,
    /// Original npm provider package used by OpenCode.
    /// OpenCode 使用的原始 npm provider 包名。
    pub npm: String,
}

impl SupportedProviderDefinition {
    /// Returns the placeholder text used by one provider-specific field.
    /// 返回某个供应商专属字段使用的占位提示文本。
    pub fn field_placeholder(&self, key: ProviderFieldKey) -> String {
        match key {
            ProviderFieldKey::ApiKey => self
                .env
                .first()
                .cloned()
                .unwrap_or_else(|| String::from("请输入 API Key 或 Token")),
            ProviderFieldKey::ResourceName => String::from("例如：my-models"),
            ProviderFieldKey::AccountId => String::from("例如：1234567890abcdef1234567890abcdef"),
        }
    }

    /// Returns the form label used by one provider-specific field.
    /// 返回某个供应商专属字段使用的表单标签。
    pub fn field_label(&self, key: ProviderFieldKey) -> String {
        match key {
            ProviderFieldKey::ApiKey => {
                if self
                    .env
                    .iter()
                    .any(|item| item.contains("TOKEN") && !item.contains("KEY"))
                {
                    String::from("访问 Token")
                } else {
                    String::from("API Key")
                }
            }
            ProviderFieldKey::ResourceName => String::from("Resource Name"),
            ProviderFieldKey::AccountId => String::from("Account ID"),
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

/// Persisted provider option values aligned with the OpenCode-style options object.
/// 与 OpenCode 风格 options 对象对齐的持久化供应商选项值。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedProviderOptions {
    /// Stored API key or token.
    /// 已保存的 API Key 或 Token。
    #[serde(rename = "apiKey", skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Stored custom base URL when one is explicitly needed.
    /// 显式需要时保存的自定义基址。
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
            ProviderFieldKey::ResourceName => self.resource_name = next,
            ProviderFieldKey::AccountId => self.account_id = next,
        }
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
    /// Provider-level API URL override.
    /// 供应商级接口地址覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    /// Original provider package marker mirrored from OpenCode.
    /// 从 OpenCode 镜像过来的原始 provider 包标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,
    /// Provider option object aligned with the OpenCode schema.
    /// 与 OpenCode Schema 对齐的供应商 options 对象。
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

/// Loads the supported provider catalog derived from the embedded OpenCode snapshot.
/// 加载从内置 OpenCode 快照派生出来的受支持供应商目录。
pub fn supported_providers() -> &'static [SupportedProviderDefinition] {
    static PROVIDERS: OnceLock<Vec<SupportedProviderDefinition>> = OnceLock::new();

    PROVIDERS.get_or_init(|| {
        let mut entries = serde_json::from_str::<Vec<CatalogProviderEntry>>(PROVIDER_CATALOG_JSON)
            .expect("provider catalog resource must stay valid");

        entries.sort_by(|left, right| provider_sort_key(left).cmp(&provider_sort_key(right)));

        entries
            .into_iter()
            .filter_map(SupportedProviderDefinition::from_catalog)
            .collect()
    })
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
    if let Some(cached) = config.provider.get(provider_id) {
        if !cached.models.is_empty() {
            let mut items = cached
                .models
                .iter()
                .map(|(id, model)| ModelChoice {
                    id: id.clone(),
                    name: model.name.clone().unwrap_or_else(|| id.clone()),
                })
                .collect::<Vec<_>>();
            items.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
            return items;
        }
    }

    supported_provider_by_id(provider_id)
        .map(|definition| definition.static_models.clone())
        .unwrap_or_default()
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

    if let Some(api_key) = options
        .api_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        let header_value = HeaderValue::from_str(&format!("Bearer {}", api_key.trim()))
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

/// Catalog entry used only for parsing the embedded provider snapshot.
/// 仅用于解析内置供应商快照的目录条目。
#[derive(Debug, Clone, Deserialize)]
struct CatalogProviderEntry {
    /// Stable provider identifier from the embedded snapshot.
    /// 来自内置快照的稳定供应商标识。
    id: String,
    /// Human-readable provider name from the snapshot.
    /// 来自快照的人类可读供应商名称。
    name: String,
    /// Default API address from the snapshot.
    /// 来自快照的默认接口地址。
    default_api: Option<String>,
    /// npm provider package name from the snapshot.
    /// 来自快照的 npm provider 包名。
    npm: String,
    /// Environment variable names from the snapshot.
    /// 来自快照的环境变量名称列表。
    #[serde(default)]
    env: Vec<String>,
    /// Embedded static model list from the snapshot.
    /// 来自快照的内置静态模型列表。
    #[serde(default)]
    models: Vec<ModelChoice>,
    /// Snapshot notes describing special runtime behavior.
    /// 描述特殊运行时行为的快照说明。
    #[serde(default)]
    special_runtime_notes: Vec<String>,
    /// Snapshot notes describing passive default rules.
    /// 描述被动默认规则的快照说明。
    #[serde(default)]
    passive_default_rules: Vec<String>,
}

impl SupportedProviderDefinition {
    /// Converts one raw catalog entry into a supported provider definition when its protocol is supported.
    /// 当协议受支持时，将一个原始目录条目转换为受支持的供应商定义。
    fn from_catalog(entry: CatalogProviderEntry) -> Option<Self> {
        if UNSUPPORTED_PROVIDER_IDS
            .iter()
            .any(|unsupported| *unsupported == entry.id)
        {
            return None;
        }

        let protocol = SupportedProtocol::from_npm(&entry.npm)?;
        let model_source_mode = if protocol.supports_model_fetch() && entry.default_api.is_some() {
            ModelSourceMode::TryRemoteThenFallback
        } else {
            ModelSourceMode::EmbeddedStatic
        };

        let mut notes = entry.special_runtime_notes;
        notes.extend(entry.passive_default_rules);

        Some(Self {
            id: entry.id.clone(),
            name: entry.name,
            protocol,
            default_api: entry.default_api,
            env: entry.env,
            model_source_mode,
            fields: provider_fields_for_id(&entry.id),
            static_models: entry.models,
            notes,
            npm: entry.npm,
        })
    }
}

/// Returns the provider-specific fields that should be exposed in the configuration page.
/// 返回应在配置页面中暴露的供应商专属字段。
fn provider_fields_for_id(provider_id: &str) -> Vec<ProviderFieldKey> {
    match provider_id {
        "azure" | "azure-cognitive-services" => {
            vec![ProviderFieldKey::ResourceName, ProviderFieldKey::ApiKey]
        }
        "cloudflare-workers-ai" => {
            vec![ProviderFieldKey::AccountId, ProviderFieldKey::ApiKey]
        }
        _ => vec![ProviderFieldKey::ApiKey],
    }
}

/// Returns a sortable provider key that keeps common providers near the top of the picker.
/// 返回可排序的供应商键值，以便将常用供应商放到选择器顶部。
fn provider_sort_key(provider: &CatalogProviderEntry) -> (u8, String) {
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

/// Builds the remote `/models` endpoint URL when the provider can try a standard refresh flow.
/// 当供应商可以尝试标准刷新流程时，构建远端 `/models` 端点地址。
fn remote_models_url(
    definition: &SupportedProviderDefinition,
    options: &PersistedProviderOptions,
) -> Option<String> {
    if !definition.protocol.supports_model_fetch() {
        return None;
    }

    let base = options
        .base_url
        .clone()
        .or_else(|| definition.default_api.clone())?;

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
                });
            }

            let object = item.as_object()?;
            let id = object.get("id")?.as_str()?.to_string();
            let name = object
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&id)
                .to_string();

            Some(ModelChoice { id, name })
        })
        .collect()
}
