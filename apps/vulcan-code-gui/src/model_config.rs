//! Dynamic model registry persistence and provider model discovery helpers.
//! 动态模型注册表持久化与供应商模型发现辅助逻辑。

use crate::model::Provider;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Persistent model registry stored beside the user configuration directory.
/// 持久化在用户配置目录旁的模型注册表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRegistryConfig {
    /// File schema version used for future migrations.
    /// 供未来迁移使用的文件结构版本号。
    pub version: u32,
    /// Configured provider instances created by the user.
    /// 由用户创建的供应商实例列表。
    pub providers: Vec<ConfiguredProvider>,
    /// Default selected model reference in `provider_id/model_id` form.
    /// 以 `provider_id/model_id` 形式存放的默认模型引用。
    pub default_model: Option<String>,
}

impl Default for ModelRegistryConfig {
    /// Builds the empty registry used before any provider instance is configured.
    /// 构建尚未配置任何供应商实例前使用的空注册表。
    fn default() -> Self {
        Self {
            version: 1,
            providers: Vec::new(),
            default_model: None,
        }
    }
}

impl ModelRegistryConfig {
    /// Loads the persisted registry and returns an optional status note for the UI.
    /// 加载持久化注册表，并返回供界面展示的可选状态说明。
    pub fn load_with_status() -> (Self, Option<String>) {
        let path = config_path();

        if !path.exists() {
            return (
                Self::default(),
                Some(format!(
                    "尚未找到模型配置文件，将在首次保存后创建：{}",
                    path.display()
                )),
            );
        }

        match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Self>(&raw) {
                Ok(config) => (config, None),
                Err(error) => (
                    Self::default(),
                    Some(format!("模型配置文件解析失败，已回退为空配置：{}", error)),
                ),
            },
            Err(error) => (
                Self::default(),
                Some(format!("模型配置文件读取失败，已回退为空配置：{}", error)),
            ),
        }
    }

    /// Saves the registry into the shared user configuration path.
    /// 将注册表保存到共享用户配置路径中。
    pub fn save(&self) -> Result<(), String> {
        let path = config_path();

        ensure_parent_directory(&path)?;

        let serialized = serde_json::to_string_pretty(self)
            .map_err(|error| format!("模型配置序列化失败：{error}"))?;

        fs::write(&path, serialized).map_err(|error| format!("模型配置写入失败：{error}"))
    }

    /// Returns the stable configuration path used by the registry.
    /// 返回注册表使用的稳定配置文件路径。
    pub fn path() -> PathBuf {
        config_path()
    }

    /// Returns the number of configured provider instances.
    /// 返回已配置的供应商实例数量。
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Returns the total number of configured models across all provider instances.
    /// 返回所有供应商实例下配置模型的总数量。
    pub fn model_count(&self) -> usize {
        self.providers
            .iter()
            .map(|provider| provider.models.len())
            .sum()
    }

    /// Resolves one provider instance by its stable identifier.
    /// 通过稳定标识解析单个供应商实例。
    pub fn provider(&self, provider_id: &str) -> Option<&ConfiguredProvider> {
        self.providers
            .iter()
            .find(|provider| provider.id == provider_id)
    }

    /// Resolves one configured model under the given provider identifier.
    /// 在给定供应商标识下解析单个已配置模型。
    pub fn model(&self, provider_id: &str, model_id: &str) -> Option<&ConfiguredModel> {
        self.provider(provider_id)
            .and_then(|provider| provider.model(model_id))
    }

    /// Returns the first configured provider identifier when available.
    /// 在可用时返回首个已配置供应商标识。
    pub fn first_provider_id(&self) -> Option<String> {
        self.providers.first().map(|provider| provider.id.clone())
    }

    /// Returns the first model identifier under one provider when available.
    /// 在可用时返回某个供应商下的首个模型标识。
    pub fn first_model_id_for_provider(&self, provider_id: &str) -> Option<String> {
        self.provider(provider_id)
            .and_then(|provider| provider.models.first())
            .map(|model| model.id.clone())
    }

    /// Returns the default selection as provider and model identifiers.
    /// 以供应商标识与模型标识形式返回默认选择。
    pub fn default_selection(&self) -> Option<(String, String)> {
        self.default_model
            .as_deref()
            .and_then(parse_model_reference)
            .map(|(provider_id, model_id)| (provider_id.to_string(), model_id.to_string()))
    }

    /// Upserts one provider instance and refreshes the default selection when needed.
    /// 插入或更新单个供应商实例，并在需要时刷新默认选择。
    pub fn upsert_provider(&mut self, provider: ConfiguredProvider) {
        let provider_id = provider.id.clone();
        let first_model_id = provider.models.first().map(|model| model.id.clone());

        if let Some(existing) = self
            .providers
            .iter_mut()
            .find(|existing| existing.id == provider_id)
        {
            *existing = provider;
        } else {
            self.providers.push(provider);
        }

        if self.default_model.is_none() {
            if let Some(model_id) = first_model_id {
                self.default_model = Some(model_reference(&provider_id, &model_id));
            }
        }
    }

    /// Stores the default selected provider and model reference.
    /// 存储默认选择的供应商与模型引用。
    pub fn set_default_selection(
        &mut self,
        provider_id: impl AsRef<str>,
        model_id: impl AsRef<str>,
    ) {
        self.default_model = Some(model_reference(provider_id.as_ref(), model_id.as_ref()));
    }
}

/// One configured provider instance persisted by the GUI.
/// 由界面持久化保存的单个供应商实例。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfiguredProvider {
    /// Stable provider instance identifier used by the GUI.
    /// 被界面使用的稳定供应商实例标识。
    pub id: String,
    /// Human-readable provider instance name.
    /// 人类可读的供应商实例名称。
    pub name: String,
    /// Catalog provider kind mapped to Rig support lanes.
    /// 映射到 Rig 支持通道的目录供应商类型。
    pub kind: Provider,
    /// Base URL used to access the remote model surface.
    /// 访问远程模型接口所使用的基础 URL。
    pub base_url: String,
    /// API key persisted for the provider instance.
    /// 为该供应商实例持久化保存的 API Key。
    pub api_key: String,
    /// Configured models available under the provider instance.
    /// 该供应商实例下可用的配置模型列表。
    pub models: Vec<ConfiguredModel>,
}

impl ConfiguredProvider {
    /// Resolves one configured model by its stable identifier.
    /// 通过稳定标识解析单个已配置模型。
    pub fn model(&self, model_id: &str) -> Option<&ConfiguredModel> {
        self.models.iter().find(|model| model.id == model_id)
    }
}

/// One configured model stored under a provider instance.
/// 存放在某个供应商实例下的单个已配置模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfiguredModel {
    /// Stable model identifier passed to the provider runtime.
    /// 传给供应商运行时的稳定模型标识。
    pub id: String,
    /// Human-readable model name shown in the GUI.
    /// 展示在界面中的人类可读模型名称。
    pub name: String,
    /// Optional context-window limit when known.
    /// 在已知时记录的可选上下文窗口上限。
    pub context_limit: Option<u64>,
    /// Optional output-token limit when known.
    /// 在已知时记录的可选输出 Token 上限。
    pub output_limit: Option<u64>,
    /// Optional tool-calling capability flag when known.
    /// 在已知时记录的可选工具调用能力标记。
    pub supports_tools: Option<bool>,
    /// Optional reasoning capability flag when known.
    /// 在已知时记录的可选推理能力标记。
    pub supports_reasoning: Option<bool>,
}

/// One remote model-discovery request built from the provider configuration form.
/// 从供应商配置表单构建出的单个远程模型发现请求。
#[derive(Debug, Clone)]
pub struct ProviderModelFetchRequest {
    /// Provider kind selected in the configurator.
    /// 在配置器中选中的供应商类型。
    pub kind: Provider,
    /// Human-readable provider instance name entered by the user.
    /// 由用户输入的人类可读供应商实例名称。
    pub name: String,
    /// Base URL entered by the user.
    /// 由用户输入的基础 URL。
    pub base_url: String,
    /// API key entered by the user.
    /// 由用户输入的 API Key。
    pub api_key: String,
}

/// One remote model-discovery response returned to the GUI.
/// 返回给界面的单个远程模型发现响应。
#[derive(Debug, Clone)]
pub struct ProviderModelFetchResponse {
    /// Provider kind associated with the fetch response.
    /// 与本次拉取结果关联的供应商类型。
    pub kind: Provider,
    /// Provider instance name copied from the fetch request.
    /// 从拉取请求中复制过来的供应商实例名称。
    pub name: String,
    /// Base URL copied from the fetch request.
    /// 从拉取请求中复制过来的基础 URL。
    pub base_url: String,
    /// Fetch strategy actually used during the request.
    /// 本次请求实际使用的拉取策略。
    pub strategy: FetchStrategy,
    /// Models discovered from the remote surface.
    /// 从远程接口面发现的模型列表。
    pub models: Vec<ConfiguredModel>,
}

/// Supported model-fetch strategies used by the configurator.
/// 配置器使用的模型拉取策略类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchStrategy {
    /// OpenAI-compatible `GET /models` listing strategy.
    /// OpenAI 兼容的 `GET /models` 列表策略。
    OpenAiCompatible,
    /// Ollama `GET /api/tags` listing strategy.
    /// Ollama 的 `GET /api/tags` 列表策略。
    OllamaTags,
    /// Manual entry fallback when no public listing endpoint is supported.
    /// 当没有公开列表接口时使用的手动录入回退策略。
    ManualOnly,
}

impl fmt::Display for FetchStrategy {
    /// Renders the user-facing fetch-strategy label.
    /// 渲染面向用户的拉取策略标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::OpenAiCompatible => "OpenAI-Compatible /models",
            Self::OllamaTags => "Ollama /api/tags",
            Self::ManualOnly => "Manual Entry",
        };

        formatter.write_str(label)
    }
}

/// Fetches the remote model list using the provider-specific discovery strategy.
/// 使用供应商对应的发现策略拉取远程模型列表。
pub async fn fetch_provider_models(
    request: ProviderModelFetchRequest,
) -> Result<ProviderModelFetchResponse, String> {
    let strategy = fetch_strategy_for_provider(request.kind);

    match strategy {
        FetchStrategy::OpenAiCompatible => {
            let models =
                fetch_openai_compatible_models(&request.base_url, &request.api_key).await?;

            Ok(ProviderModelFetchResponse {
                kind: request.kind,
                name: request.name,
                base_url: request.base_url,
                strategy,
                models,
            })
        }
        FetchStrategy::OllamaTags => {
            let models = fetch_ollama_models(&request.base_url).await?;

            Ok(ProviderModelFetchResponse {
                kind: request.kind,
                name: request.name,
                base_url: request.base_url,
                strategy,
                models,
            })
        }
        FetchStrategy::ManualOnly => Err(format!(
            "{} 当前没有统一的公开模型列表接口，请直接手动填写模型标识后加入配置。",
            request.kind
        )),
    }
}

/// Returns the provider-specific model-fetch strategy used by the configurator.
/// 返回配置器为指定供应商使用的模型拉取策略。
pub const fn fetch_strategy_for_provider(provider: Provider) -> FetchStrategy {
    match provider {
        Provider::Anthropic
        | Provider::AzureOpenAi
        | Provider::ChatGpt
        | Provider::Cohere
        | Provider::Copilot
        | Provider::Gemini
        | Provider::VoyageAi => FetchStrategy::ManualOnly,
        Provider::Ollama => FetchStrategy::OllamaTags,
        Provider::OpenAi
        | Provider::DeepSeek
        | Provider::Galadriel
        | Provider::Groq
        | Provider::HuggingFace
        | Provider::Hyperbolic
        | Provider::Llamafile
        | Provider::MiniMax
        | Provider::Mira
        | Provider::Mistral
        | Provider::Moonshot
        | Provider::OpenRouter
        | Provider::Perplexity
        | Provider::Together
        | Provider::Xai
        | Provider::XiaomiMimo
        | Provider::ZAi
        | Provider::OpenAiCompatibleLocal => FetchStrategy::OpenAiCompatible,
    }
}

/// Returns the default base-URL hint shown in the configuration form.
/// 返回展示在配置表单中的默认基础 URL 提示。
pub const fn default_base_url_hint(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => "https://api.openai.com/v1",
        Provider::Anthropic => "https://api.anthropic.com",
        Provider::AzureOpenAi => "https://{resource}.openai.azure.com/openai",
        Provider::ChatGpt => "https://chatgpt.com/backend-api",
        Provider::Cohere => "https://api.cohere.com/v2",
        Provider::Copilot => "https://api.githubcopilot.com",
        Provider::DeepSeek => "https://api.deepseek.com/v1",
        Provider::Galadriel => "https://api.galadriel.com/v1",
        Provider::Gemini => "https://generativelanguage.googleapis.com",
        Provider::Groq => "https://api.groq.com/openai/v1",
        Provider::HuggingFace => "https://router.huggingface.co/v1",
        Provider::Hyperbolic => "https://api.hyperbolic.xyz/v1",
        Provider::Llamafile => "http://127.0.0.1:8080/v1",
        Provider::MiniMax => "https://api.minimax.chat/v1",
        Provider::Mira => "https://api.mira.network/v1",
        Provider::Mistral => "https://api.mistral.ai/v1",
        Provider::Moonshot => "https://api.moonshot.cn/v1",
        Provider::Ollama => "http://127.0.0.1:11434",
        Provider::OpenRouter => "https://openrouter.ai/api/v1",
        Provider::Perplexity => "https://api.perplexity.ai",
        Provider::Together => "https://api.together.xyz/v1",
        Provider::VoyageAi => "https://api.voyageai.com/v1",
        Provider::Xai => "https://api.x.ai/v1",
        Provider::XiaomiMimo => "https://api.mimo.xiaomi.com/v1",
        Provider::ZAi => "https://api.z.ai/api/paas/v1",
        Provider::OpenAiCompatibleLocal => "http://127.0.0.1:8000/v1",
    }
}

/// Returns the provider-specific configuration hint shown under the form.
/// 返回显示在表单下方的供应商专属配置提示。
pub const fn provider_configuration_hint(provider: Provider) -> &'static str {
    match fetch_strategy_for_provider(provider) {
        FetchStrategy::OpenAiCompatible => {
            "该供应商当前按 OpenAI 兼容协议处理，点击“获取模型列表”会读取远端的 /models 接口。"
        }
        FetchStrategy::OllamaTags => {
            "该供应商当前按 Ollama 处理，点击“获取模型列表”会读取本地或远端的 /api/tags 接口。"
        }
        FetchStrategy::ManualOnly => {
            "该供应商当前没有统一自动拉取入口，请填写至少一个模型标识后再加入配置。"
        }
    }
}

/// Builds a stable provider-instance identifier from a human-readable name.
/// 从人类可读名称构建稳定的供应商实例标识。
pub fn slugify_identifier(value: &str) -> String {
    let mut slug = String::new();

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if (character == '-' || character == '_' || character.is_ascii_whitespace())
            && !slug.ends_with('-')
        {
            slug.push('-');
        }
    }

    let normalized = slug.trim_matches('-').to_string();

    if normalized.is_empty() {
        String::from("provider")
    } else {
        normalized
    }
}

/// Builds the stable `provider_id/model_id` reference string.
/// 构建稳定的 `provider_id/model_id` 引用字符串。
pub fn model_reference(provider_id: &str, model_id: &str) -> String {
    format!("{provider_id}/{model_id}")
}

/// Parses one `provider_id/model_id` reference into its two parts.
/// 将单个 `provider_id/model_id` 引用解析为两段内容。
pub fn parse_model_reference(reference: &str) -> Option<(&str, &str)> {
    reference.split_once('/')
}

/// Fetches models from an OpenAI-compatible `/models` endpoint.
/// 从 OpenAI 兼容的 `/models` 接口拉取模型列表。
async fn fetch_openai_compatible_models(
    base_url: &str,
    api_key: &str,
) -> Result<Vec<ConfiguredModel>, String> {
    let normalized_base_url = normalize_base_url(base_url)?;
    let endpoint = format!("{normalized_base_url}/models");
    let client = reqwest::Client::new();
    let mut request = client.get(endpoint);

    if !api_key.trim().is_empty() {
        request = request.bearer_auth(api_key.trim());
    }

    let response = request
        .send()
        .await
        .map_err(|error| format!("模型列表请求失败：{error}"))?;
    let response = response
        .error_for_status()
        .map_err(|error| format!("模型列表返回失败状态：{error}"))?;
    let payload = response
        .json::<OpenAiCompatibleModelsResponse>()
        .await
        .map_err(|error| format!("模型列表解析失败：{error}"))?;

    let mut models = payload
        .data
        .into_iter()
        .map(|item| ConfiguredModel {
            id: item.id.clone(),
            name: item.id,
            context_limit: None,
            output_limit: None,
            supports_tools: None,
            supports_reasoning: None,
        })
        .collect::<Vec<_>>();

    models.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(models)
}

/// Fetches models from an Ollama `/api/tags` endpoint.
/// 从 Ollama 的 `/api/tags` 接口拉取模型列表。
async fn fetch_ollama_models(base_url: &str) -> Result<Vec<ConfiguredModel>, String> {
    let normalized_base_url = normalize_base_url(base_url)?;
    let endpoint = format!("{normalized_base_url}/api/tags");
    let client = reqwest::Client::new();
    let response = client
        .get(endpoint)
        .send()
        .await
        .map_err(|error| format!("Ollama 模型列表请求失败：{error}"))?;
    let response = response
        .error_for_status()
        .map_err(|error| format!("Ollama 模型列表返回失败状态：{error}"))?;
    let payload = response
        .json::<OllamaTagsResponse>()
        .await
        .map_err(|error| format!("Ollama 模型列表解析失败：{error}"))?;

    let mut models = payload
        .models
        .into_iter()
        .map(|item| ConfiguredModel {
            id: item.name.clone(),
            name: item.name,
            context_limit: None,
            output_limit: None,
            supports_tools: None,
            supports_reasoning: None,
        })
        .collect::<Vec<_>>();

    models.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(models)
}

/// Normalizes one base URL before model discovery requests are issued.
/// 在发起模型发现请求前规范化单个基础 URL。
fn normalize_base_url(base_url: &str) -> Result<String, String> {
    let normalized = base_url.trim().trim_end_matches('/').to_string();

    if normalized.is_empty() {
        Err(String::from("请先填写基础 URL。"))
    } else {
        Ok(normalized)
    }
}

/// Ensures the parent configuration directory exists before saving.
/// 在保存前确保父级配置目录已经存在。
fn ensure_parent_directory(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }

    Ok(())
}

/// Resolves the shared persistent configuration path used by the GUI.
/// 解析 GUI 使用的共享持久化配置路径。
fn config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    home.join(".config")
        .join("openvulcan")
        .join("vulcan-code")
        .join("models.json")
}

/// OpenAI-compatible `/models` response payload.
/// OpenAI 兼容 `/models` 接口的响应载荷。
#[derive(Debug, Deserialize)]
struct OpenAiCompatibleModelsResponse {
    /// Remote model entries returned by the API.
    /// 接口返回的远端模型条目列表。
    data: Vec<OpenAiCompatibleModelEntry>,
}

/// One OpenAI-compatible remote model entry.
/// 单个 OpenAI 兼容远端模型条目。
#[derive(Debug, Deserialize)]
struct OpenAiCompatibleModelEntry {
    /// Stable remote model identifier.
    /// 稳定的远端模型标识。
    id: String,
}

/// Ollama `/api/tags` response payload.
/// Ollama `/api/tags` 接口的响应载荷。
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    /// Remote model entries returned by Ollama.
    /// 由 Ollama 返回的远端模型条目列表。
    models: Vec<OllamaModelEntry>,
}

/// One remote model entry returned by Ollama.
/// Ollama 返回的单个远端模型条目。
#[derive(Debug, Deserialize)]
struct OllamaModelEntry {
    /// Stable remote model identifier.
    /// 稳定的远端模型标识。
    name: String,
}
