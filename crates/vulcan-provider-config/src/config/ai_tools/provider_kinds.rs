use super::*;

/// User-selected routing mode for the unified web-search capability.
/// 用户为统一网页搜索能力选择的路由模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebSearchRoutingMode {
    /// Only expose provider-native web search; unsupported providers keep the tool inactive.
    /// 仅暴露供应商原生网页搜索；不支持时保持工具未激活。
    NativeOnly,
    /// Prefer provider-native web search, then use the configured host provider chain.
    /// 优先使用供应商原生网页搜索，不支持时使用已配置的宿主供应商链。
    NativeFirst,
    /// Prefer the configured host provider chain, then fall back to provider-native search when no host chain is available.
    /// 优先使用已配置的第三方搜索链；当第三方链不可用时回退到供应商原生搜索。
    #[serde(alias = "provider-first")]
    ThirdPartyFirst,
    /// Only expose the configured host provider chain.
    /// 仅暴露已配置的第三方搜索链。
    #[serde(alias = "provider-only")]
    ThirdPartyOnly,
    /// Disable unified web search completely.
    /// 完全关闭统一网页搜索。
    Disabled,
}

impl WebSearchRoutingMode {
    /// All routing modes shown by the settings picker in stable order.
    /// 设置选择器中按稳定顺序展示的全部路由模式。
    pub const ALL: [Self; 4] = [
        Self::NativeOnly,
        Self::NativeFirst,
        Self::ThirdPartyFirst,
        Self::ThirdPartyOnly,
    ];

    /// Returns the compact settings label for this routing mode.
    /// 返回当前路由模式在设置界面使用的紧凑标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::NativeOnly => "仅使用供应商原生",
            Self::NativeFirst => "原生优先，第三方兜底",
            Self::ThirdPartyFirst => "第三方优先，原生兜底",
            Self::ThirdPartyOnly => "仅使用第三方搜索",
            Self::Disabled => "彻底关闭 web-search",
        }
    }

    /// Returns one explanatory sentence for settings and diagnostics surfaces.
    /// 返回用于设置与诊断界面的一句解释说明。
    pub const fn description(self) -> &'static str {
        match self {
            Self::NativeOnly => {
                "仅当当前模型声明支持 Provider Hosted Search 时，才在请求中声明启用。"
            }
            Self::NativeFirst => {
                "能用模型原生 Hosted Search 就声明启用；否则使用下方第三方搜索链。"
            }
            Self::ThirdPartyFirst => {
                "优先暴露宿主 web_search；第三方链未配置或不可用时才声明模型原生搜索。"
            }
            Self::ThirdPartyOnly => "只暴露宿主 web_search；第三方搜索未配置时不会启用搜索。",
            Self::Disabled => {
                "不注入统一 web_search，也不声明模型内建搜索；用户可自行通过 Skill 或 MCP 暴露搜索工具。"
            }
        }
    }
}

impl Default for WebSearchRoutingMode {
    /// Returns the conservative default that preserves provider-native behavior.
    /// 返回保守默认值，以保留供应商原生行为。
    fn default() -> Self {
        Self::NativeFirst
    }
}

impl fmt::Display for WebSearchRoutingMode {
    /// Formats one routing mode as a user-facing label.
    /// 将单个路由模式格式化为面向用户的标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Supported external web-search provider families for the host fallback chain.
/// 宿主回退链支持的第三方网页搜索供应商类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebSearchProviderDispatchMode {
    /// Try configured providers in order and stop on the first successful response.
    /// 按配置顺序依次尝试供应商，并在首次成功响应后停止。
    PriorityFailover,
    /// Rotate the starting provider across runs while still allowing per-run fallback to the rest of the chain.
    /// 在多次调用之间轮换起始供应商，同时允许单次调用继续回退到链上其他条目。
    Balanced,
}

#[allow(dead_code)]
impl WebSearchProviderDispatchMode {
    /// All dispatch modes shown by the settings page in stable order.
    /// 设置页面按稳定顺序展示的全部调度模式。
    pub const ALL: [Self; 2] = [Self::PriorityFailover, Self::Balanced];

    /// Returns the compact settings label for the dispatch mode.
    /// 返回当前调度模式在设置界面显示的紧凑标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::PriorityFailover => "顺序优先，失败兜底",
            Self::Balanced => "均衡轮询，失败兜底",
        }
    }

    /// Returns one explanatory sentence for settings and diagnostics surfaces.
    /// 返回用于设置与诊断界面的一句解释说明。
    pub const fn description(self) -> &'static str {
        match self {
            Self::PriorityFailover => "始终从列表顶部开始尝试；当前供应商失败后自动切换到下一个。",
            Self::Balanced => {
                "在多次调用之间轮换起始供应商；单次失败时仍继续尝试链上的其他供应商。"
            }
        }
    }
}

impl Default for WebSearchProviderDispatchMode {
    /// Returns the conservative default dispatch mode.
    /// 返回保守的默认调度模式。
    fn default() -> Self {
        Self::PriorityFailover
    }
}

impl fmt::Display for WebSearchProviderDispatchMode {
    /// Formats one dispatch mode as a user-facing label.
    /// 将单个调度模式格式化为面向用户的标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Supported third-party web-search provider families for the host fallback chain.
/// 宿主回退链支持的第三方网页搜索供应商类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebSearchProviderKind {
    /// DuckDuckGo HTML search without an API key.
    /// 无需 API Key 的 DuckDuckGo HTML 搜索。
    DuckDuckGo,
    /// Brave Search API.
    /// Brave Search API。
    Brave,
    /// Tavily Search API.
    /// Tavily Search API。
    Tavily,
    /// Exa Search API.
    /// Exa Search API。
    Exa,
    /// Firecrawl Search API.
    /// Firecrawl Search API。
    Firecrawl,
    /// Gemini grounding search via Google Gemini API.
    /// 通过 Google Gemini API 调用的 Gemini 搜索。
    Gemini,
    /// Grok web-grounded search via xAI.
    /// 通过 xAI 调用的 Grok 联网搜索。
    Grok,
    /// Kimi web search via Moonshot chat-completions.
    /// 通过 Moonshot chat-completions 调用的 Kimi 联网搜索。
    Kimi,
    /// MiniMax search endpoint backed by the Token Plan key and credits system.
    /// 由 Token Plan Key 与 Credits 体系承载的 MiniMax 搜索端点。
    MiniMax,
    /// Ollama web search endpoint exposed by the local or cloud host.
    /// 由本地或云端 Ollama 宿主暴露的联网搜索端点。
    Ollama,
    /// Perplexity structured search endpoint.
    /// Perplexity 结构化搜索端点。
    Perplexity,
    /// SearXNG self-hosted meta search.
    /// SearXNG 自托管聚合搜索。
    Searxng,
    /// SerpAPI Google search endpoint.
    /// SerpAPI Google 搜索端点。
    SerpApi,
}

/// Supported third-party image-understanding provider families for the standalone capability layer.
/// 独立能力层支持的第三方图片理解提供方类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnderstandImageProviderKind {
    /// MiniMax VLM image-understanding endpoint.
    /// MiniMax VLM 图片理解端点。
    MiniMax,
}

#[allow(dead_code)]
impl UnderstandImageProviderKind {
    /// All supported image-understanding provider kinds in stable persistence order.
    /// 全部受支持的图片理解提供方类型，按稳定持久化顺序排列。
    pub const ALL: [Self; 1] = [Self::MiniMax];

    /// Returns the stable provider kind key used for persistence and target routing.
    /// 返回持久化与目标路由使用的稳定提供方类型键。
    pub const fn key(self) -> &'static str {
        match self {
            Self::MiniMax => "minimax",
        }
    }

    /// Returns the resource-backed provider definition associated with the current kind.
    /// 返回与当前类型关联的资源化提供方定义。
    fn definition(
        self,
    ) -> &'static ai_capability_provider_catalog::UnderstandImageProviderDefinition {
        ai_capability_provider_catalog::understand_image_provider_definition_by_id(self.key())
            .unwrap_or_else(|| {
                panic!(
                    "missing understand-image provider definition for `{}`",
                    self.key()
                )
            })
    }

    /// Returns the user-facing provider label.
    /// 返回面向用户的提供方标签。
    pub fn label(self) -> &'static str {
        self.definition().name.as_str()
    }

    /// Returns the stable default target alias exposed below one configured provider instance.
    /// 返回暴露在单个配置提供方实例下方的稳定默认目标别名。
    pub fn default_target_alias(self) -> &'static str {
        self.definition().target_alias.as_str()
    }

    /// Returns the default endpoint used when the user leaves the base URL empty.
    /// 返回用户留空基址时使用的默认端点。
    pub fn default_base_url(self) -> &'static str {
        self.definition().default_base_url.as_str()
    }

    /// Returns the label shown above the API key field.
    /// 返回显示在 API Key 字段上方的标签。
    pub fn api_key_label(self) -> &'static str {
        self.definition().api_key_label.as_str()
    }

    /// Returns the placeholder shown inside the API key field.
    /// 返回显示在 API Key 字段中的占位提示。
    pub fn api_key_placeholder(self) -> &'static str {
        self.definition().api_key_placeholder.as_str()
    }

    /// Returns whether the provider exposes one configurable region field.
    /// 返回当前提供方是否暴露可配置区域字段。
    pub fn supports_region_field(self) -> bool {
        self.definition().supports_region_field
    }

    /// Returns the label shown above the region field.
    /// 返回显示在区域字段上方的标签。
    pub fn region_label(self) -> &'static str {
        self.definition().region_label.as_str()
    }

    /// Returns the placeholder shown inside the region field.
    /// 返回显示在区域字段中的占位提示。
    pub fn region_placeholder(self) -> &'static str {
        self.definition().region_placeholder.as_str()
    }

    /// Returns the default region value suggested for the provider.
    /// 返回为当前提供方建议的默认区域值。
    pub fn default_region(self) -> &'static str {
        self.definition().default_region.as_str()
    }

    /// Returns one provider-specific configuration note shown in the settings UI.
    /// 返回显示在设置界面中的提供方专属配置说明。
    pub fn configuration_note(self) -> Option<&'static str> {
        self.definition().configuration_note.as_deref()
    }
}

impl fmt::Display for UnderstandImageProviderKind {
    /// Formats one provider kind as a user-facing label.
    /// 将单个提供方类型格式化为面向用户的标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[allow(dead_code)]
impl WebSearchProviderKind {
    /// All supported third-party web-search provider kinds in stable persistence order.
    /// 全部受支持的第三方网页搜索提供方类型，按稳定持久化顺序排列。
    pub const ALL: [Self; 13] = [
        Self::DuckDuckGo,
        Self::Brave,
        Self::Tavily,
        Self::Exa,
        Self::Firecrawl,
        Self::Gemini,
        Self::Grok,
        Self::Kimi,
        Self::MiniMax,
        Self::Ollama,
        Self::Perplexity,
        Self::Searxng,
        Self::SerpApi,
    ];

    /// Returns the stable provider kind key used for generated ids.
    /// 返回生成标识使用的稳定供应商类型键。
    pub const fn key(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "duckduckgo",
            Self::Brave => "brave",
            Self::Tavily => "tavily",
            Self::Exa => "exa",
            Self::Firecrawl => "firecrawl",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
            Self::Kimi => "kimi",
            Self::MiniMax => "minimax",
            Self::Ollama => "ollama",
            Self::Perplexity => "perplexity",
            Self::Searxng => "searxng",
            Self::SerpApi => "serpapi",
        }
    }

    /// Resolves one provider kind from its stable persisted key.
    /// 根据稳定持久化键解析单个提供方类型。
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// Returns the resource-backed provider definition associated with the current kind.
    /// 返回与当前类型关联的资源化提供方定义。
    fn definition(self) -> &'static ai_capability_provider_catalog::WebSearchProviderDefinition {
        ai_capability_provider_catalog::web_search_provider_definition_by_id(self.key())
            .unwrap_or_else(|| {
                panic!(
                    "missing web-search provider definition for `{}`",
                    self.key()
                )
            })
    }

    /// Returns the user-facing provider name.
    /// 返回面向用户的供应商名称。
    pub fn label(self) -> &'static str {
        self.definition().name.as_str()
    }

    /// Returns the stable default target alias exposed beneath one concrete provider instance.
    /// 返回暴露在单个具体供应商实例下方的稳定默认目标别名。
    pub fn default_target_alias(self) -> &'static str {
        self.definition().target_alias.as_str()
    }

    /// Returns the default base URL used when the user leaves the field empty.
    /// 返回用户留空接口地址时使用的默认基址。
    pub fn default_base_url(self) -> &'static str {
        self.definition().default_base_url.as_str()
    }

    /// Returns whether the provider requires a non-empty API key before it can be used.
    /// 返回当前供应商在使用前是否要求填写非空 API Key。
    pub fn requires_api_key(self) -> bool {
        self.definition().api_key_requirement.requires_api_key()
    }

    /// Returns whether the provider can optionally use an API key even when it is not mandatory.
    /// 返回当前供应商是否允许在非强制前提下可选填写 API Key。
    pub fn api_key_optional(self) -> bool {
        self.definition().api_key_requirement.api_key_optional()
    }

    /// Returns whether the provider exposes one configurable model field.
    /// 返回当前供应商是否暴露可配置模型字段。
    pub fn supports_model_field(self) -> bool {
        self.definition().supports_model_field
    }

    /// Returns whether the provider exposes one configurable region-like field.
    /// 返回当前供应商是否暴露可配置区域类字段。
    pub fn supports_region_field(self) -> bool {
        self.definition().supports_region_field
    }

    /// Returns the user-facing label shown above the API key field.
    /// 返回显示在 API Key 字段上方的面向用户标签。
    pub fn api_key_label(self) -> &'static str {
        self.definition().api_key_label.as_str()
    }

    /// Returns the placeholder shown inside the API key field.
    /// 返回显示在 API Key 字段内的占位提示。
    pub fn api_key_placeholder(self) -> &'static str {
        self.definition().api_key_placeholder.as_str()
    }

    /// Returns the default model id suggested for model-capable providers.
    /// 返回为支持模型字段的供应商建议的默认模型标识。
    pub fn default_model(self) -> &'static str {
        self.definition().default_model.as_str()
    }

    /// Returns the placeholder shown inside the model field.
    /// 返回显示在模型字段内的占位提示。
    pub fn model_placeholder(self) -> &'static str {
        self.definition().model_placeholder.as_str()
    }

    /// Returns the label shown above the model field.
    /// 返回显示在模型字段上方的标签。
    pub fn model_label(self) -> &'static str {
        self.definition().model_label.as_str()
    }

    /// Returns the default region or locale hint for region-aware providers.
    /// 返回支持区域字段的供应商使用的默认区域或地区提示。
    pub fn default_region(self) -> &'static str {
        self.definition().default_region.as_str()
    }

    /// Returns the placeholder shown inside the region field.
    /// 返回显示在区域字段内的占位提示。
    pub fn region_placeholder(self) -> &'static str {
        self.definition().region_placeholder.as_str()
    }

    /// Returns the label shown above the region field.
    /// 返回显示在区域字段上方的标签。
    pub fn region_label(self) -> &'static str {
        self.definition().region_label.as_str()
    }

    /// Returns one provider-specific configuration hint shown in the settings UI when extra clarification is helpful.
    /// 返回在设置界面需要额外澄清时展示的供应商专属配置提示。
    pub fn configuration_note(self) -> Option<&'static str> {
        self.definition().configuration_note.as_deref()
    }

    /// Returns the per-provider hard result-count ceiling.
    /// 返回当前供应商允许的结果数量硬上限。
    pub fn max_results_limit(self) -> usize {
        self.definition().max_results_limit
    }
}

impl fmt::Display for WebSearchProviderKind {
    /// Formats one provider kind as a user-facing name.
    /// 将单个供应商类型格式化为面向用户的名称。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}
