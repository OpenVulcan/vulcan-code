use super::*;

/// User-facing provider picker option.
/// 面向用户的供应商选择项。
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
    /// Optional lifecycle status.
    /// 可选的生命周期状态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional family label.
    /// 可选的家族标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Optional release-date label.
    /// 可选的发布日期标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    /// Optional context-window limit.
    /// 可选的上下文窗口上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Optional input-token limit.
    /// 可选的输入 Token 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_limit: Option<usize>,
    /// Optional output-token limit.
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
    /// Optional temperature capability flag.
    /// 可选的温度能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<bool>,
    /// Optional attachment capability flag.
    /// 可选的附件能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    /// Optional interleaved reasoning descriptor.
    /// 可选的交错推理描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interleaved: Option<ModelInterleavedCapability>,
    /// Optional model-scoped reasoning profile.
    /// 可选的模型级推理档位配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<ReasoningProfileConfig>,
    /// Optional cached AI-specialized tool declarations.
    /// 可选的缓存 AI 专用工具声明集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_tools: Vec<AiToolDeclaration>,
    /// Optional input modalities.
    /// 可选的输入模态集合。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub input_modalities: Vec<String>,
    /// Optional output modalities.
    /// 可选的输出模态集合。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output_modalities: Vec<String>,
    /// Whether the model belongs to the provider-declared default-enabled subset.
    /// 当前模型是否属于供应商声明的默认启用子集。
    #[serde(default, skip_serializing_if = "is_false")]
    pub default_enabled: bool,
}

/// Persisted user override metadata for one model entry.
/// 单个模型条目的持久化用户覆盖元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedModelOverrideEntry {
    /// Optional reasoning capability override.
    /// 可选的推理能力覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Optional attachment capability override kept for diagnostics and metadata parity.
    /// 供诊断与元数据对齐使用的可选附件能力覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    /// Optional context-window limit override.
    /// 可选的上下文窗口上限覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Optional input-token limit override.
    /// 可选的输入 Token 上限覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_limit: Option<usize>,
    /// Optional output-token limit override.
    /// 可选的输出 Token 上限覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<usize>,
    /// Optional reasoning-profile override.
    /// 可选的推理档位配置覆盖值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<ReasoningProfileConfig>,
    /// Optional input-modality override list.
    /// 可选的输入模态覆盖列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_modalities: Option<Vec<String>>,
    /// Optional output-modality override list.
    /// 可选的输出模态覆盖列表。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<String>>,
}

impl PersistedModelOverrideEntry {
    /// Returns whether the override entry is effectively empty and can be skipped during serialization.
    /// 返回该覆盖条目是否实际上为空，从而可在序列化时跳过。
    pub fn is_empty(&self) -> bool {
        self.reasoning.is_none()
            && self.attachment.is_none()
            && self.context_limit.is_none()
            && self.input_limit.is_none()
            && self.output_limit.is_none()
            && self.reasoning_profile.is_none()
            && self.input_modalities.is_none()
            && self.output_modalities.is_none()
    }
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
    /// Deprecated remote model cache retained only for backward-compatible deserialization.
    /// 仅为向后兼容反序列化而保留的废弃远端模型缓存。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, PersistedModelEntry>,
    /// Explicitly enabled model identifiers, where `None` preserves legacy "all enabled" compatibility.
    /// 显式启用的模型标识集合，其中 `None` 用于保留旧版“全部启用”的兼容语义。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_models: Option<Vec<String>>,
    /// User-authored per-model metadata overrides.
    /// 用户编写的逐模型元数据覆盖配置。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_overrides: BTreeMap<String, PersistedModelOverrideEntry>,
}

impl PersistedProviderEntry {
    /// Replaces the explicit enabled-model set with one stable deduplicated list.
    /// 使用一份稳定去重后的列表替换显式启用模型集合。
    pub fn set_enabled_models<I>(&mut self, model_ids: I)
    where
        I: IntoIterator<Item = String>,
    {
        let ordered = model_ids.into_iter().collect::<BTreeSet<_>>();
        self.enabled_models = Some(ordered.into_iter().collect());
    }
}

/// Stable transcript turn-page size options exposed by the settings page.
/// 设置页面暴露的稳定转录轮次分页选项集合。
pub const SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS: [usize; 4] = [20, 30, 40, 50];

/// Default transcript turn-page size used when the shared configuration file does not specify an override.
/// 当共享配置文件未指定覆盖值时使用的默认转录轮次分页大小。
pub const SESSION_TRANSCRIPT_TURN_PAGE_SIZE_DEFAULT: usize = 30;

/// Persisted provider retry behavior shared by provider transports.
/// 由 Provider 传输链路共享的持久化请求重试行为配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedProviderRetryConfiguration {
    /// Whether automatic retry is enabled for retryable provider failures.
    /// 是否为可重试的 Provider 失败启用自动重试。
    #[serde(default = "default_provider_retry_enabled")]
    pub enabled: bool,
    /// Maximum number of retries after the initial request attempt.
    /// 初始请求失败后允许追加重试的最大次数。
    #[serde(rename = "maxRetries", default = "default_provider_retry_max_retries")]
    pub max_retries: u32,
    /// Initial retry delay in milliseconds before exponential backoff is applied.
    /// 应用指数退避前的初始重试等待毫秒数。
    #[serde(
        rename = "initialDelayMs",
        default = "default_provider_retry_initial_delay_ms"
    )]
    pub initial_delay_ms: u64,
    /// Maximum retry delay in milliseconds when no provider header asks for a longer wait.
    /// 在没有 Provider 响应头要求更久等待时使用的最大重试等待毫秒数。
    #[serde(rename = "maxDelayMs", default = "default_provider_retry_max_delay_ms")]
    pub max_delay_ms: u64,
    /// Exponential backoff multiplier applied after each failed attempt.
    /// 每次失败后应用的指数退避倍数。
    #[serde(
        rename = "backoffFactor",
        default = "default_provider_retry_backoff_factor"
    )]
    pub backoff_factor: f64,
    /// Deterministic jitter percentage used to avoid synchronized retry bursts.
    /// 用于避免同步重试尖峰的确定性抖动百分比。
    #[serde(
        rename = "jitterPercent",
        default = "default_provider_retry_jitter_percent"
    )]
    pub jitter_percent: u8,
    /// Whether `Retry-After` style provider hints should override local backoff.
    /// 是否允许 `Retry-After` 类型的 Provider 提示覆盖本地退避时间。
    #[serde(
        rename = "respectRetryAfter",
        default = "default_provider_retry_respect_retry_after"
    )]
    pub respect_retry_after: bool,
    /// Whether HTTP 429 should be retried when it is not a quota-depleted failure.
    /// 当 HTTP 429 不是配额耗尽错误时是否允许重试。
    #[serde(rename = "retry429", default = "default_provider_retry_429")]
    pub retry_429: bool,
}

impl Default for PersistedProviderRetryConfiguration {
    /// Builds the safe default provider retry configuration.
    /// 构建安全的默认 Provider 重试配置。
    fn default() -> Self {
        Self {
            enabled: default_provider_retry_enabled(),
            max_retries: default_provider_retry_max_retries(),
            initial_delay_ms: default_provider_retry_initial_delay_ms(),
            max_delay_ms: default_provider_retry_max_delay_ms(),
            backoff_factor: default_provider_retry_backoff_factor(),
            jitter_percent: default_provider_retry_jitter_percent(),
            respect_retry_after: default_provider_retry_respect_retry_after(),
            retry_429: default_provider_retry_429(),
        }
    }
}

impl PersistedProviderRetryConfiguration {
    /// Returns whether the persisted retry block is equal to the built-in default.
    /// 返回当前持久化重试配置块是否等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Returns the default automatic provider retry enablement.
/// 返回默认的 Provider 自动重试启用状态。
pub(super) fn default_provider_retry_enabled() -> bool {
    true
}

/// Returns the default maximum provider retry count.
/// 返回默认的 Provider 最大重试次数。
pub(super) fn default_provider_retry_max_retries() -> u32 {
    5
}

/// Returns the default initial provider retry delay in milliseconds.
/// 返回默认的 Provider 初始重试等待毫秒数。
pub(super) fn default_provider_retry_initial_delay_ms() -> u64 {
    2_000
}

/// Returns the default maximum provider retry delay in milliseconds.
/// 返回默认的 Provider 最大重试等待毫秒数。
pub(super) fn default_provider_retry_max_delay_ms() -> u64 {
    30_000
}

/// Returns the default provider retry exponential backoff factor.
/// 返回默认的 Provider 重试指数退避倍数。
pub(super) fn default_provider_retry_backoff_factor() -> f64 {
    2.0
}

/// Returns the default deterministic provider retry jitter percentage.
/// 返回默认的 Provider 重试确定性抖动百分比。
pub(super) fn default_provider_retry_jitter_percent() -> u8 {
    20
}

/// Returns the default provider retry-after handling switch.
/// 返回默认的 Provider Retry-After 处理开关。
pub(super) fn default_provider_retry_respect_retry_after() -> bool {
    true
}

/// Returns the default provider HTTP 429 retry switch.
/// 返回默认的 Provider HTTP 429 重试开关。
pub(super) fn default_provider_retry_429() -> bool {
    true
}
