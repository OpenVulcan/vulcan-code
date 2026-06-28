use super::*;

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

/// Provider runtime family aligned with the current project and host-controlled integration model.
/// 与当前项目及宿主控制接入模型对齐的供应商运行时族。
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
    /// OpenAI Responses-compatible HTTP runtime.
    /// OpenAI Responses 兼容 HTTP 运行时。
    #[serde(rename = "openai-responses-compatible")]
    OpenAiResponsesCompatible,
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

/// Canonical provider API family derived from one runtime kind and used to group truly shared protocol behavior.
/// 从运行时类型派生出的规范 Provider 接口族，用于归并真正共享的协议行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderApiFamily {
    /// OpenAI Responses API family.
    /// OpenAI Responses 接口族。
    OpenAiResponses,
    /// OpenAI Chat Completions compatible API family.
    /// OpenAI Chat Completions 兼容接口族。
    OpenAiChatCompatible,
    /// Anthropic Messages API family.
    /// Anthropic Messages 接口族。
    AnthropicMessages,
    /// Google Gemini GenerateContent API family.
    /// Google Gemini GenerateContent 接口族。
    GoogleGeminiGenerateContent,
    /// Cohere Chat API family.
    /// Cohere Chat 接口族。
    CohereChat,
    /// Reserved AWS Bedrock Converse API family.
    /// 预留的 AWS Bedrock Converse 接口族。
    BedrockConverse,
}

impl ProviderApiFamily {
    /// Returns one stable API-family key used by diagnostics, previews, and future protocol factories.
    /// 返回供诊断、预览与后续协议工厂复用的稳定接口族键值。
    pub const fn key(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai-responses",
            Self::OpenAiChatCompatible => "openai-chat-compatible",
            Self::AnthropicMessages => "anthropic-messages",
            Self::GoogleGeminiGenerateContent => "google-gemini-generate-content",
            Self::CohereChat => "cohere-chat",
            Self::BedrockConverse => "bedrock-converse",
        }
    }

    /// Returns one human-readable API-family label used by diagnostics and settings surfaces.
    /// 返回供诊断面板与设置界面展示的人类可读接口族标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "OpenAI Responses",
            Self::OpenAiChatCompatible => "OpenAI Chat Compatible",
            Self::AnthropicMessages => "Anthropic Messages",
            Self::GoogleGeminiGenerateContent => "Google Gemini GenerateContent",
            Self::CohereChat => "Cohere Chat",
            Self::BedrockConverse => "AWS Bedrock Converse",
        }
    }
}

impl ProviderRuntimeKind {
    /// Returns a concise runtime label for the configuration page.
    /// 返回配置页面使用的精简运行时标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => "OpenAI-Compatible",
            Self::OpenAi => "OpenAI",
            Self::OpenAiResponsesCompatible => "OpenAI Responses-Compatible",
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

    /// Returns the canonical provider API family behind the current runtime kind.
    /// 返回当前运行时类型背后的规范 Provider 接口族。
    pub const fn api_family(self) -> ProviderApiFamily {
        match self {
            Self::OpenAi | Self::OpenAiResponsesCompatible => ProviderApiFamily::OpenAiResponses,
            Self::OpenAiCompatible
            | Self::AzureOpenAi
            | Self::XAi
            | Self::Mistral
            | Self::Groq
            | Self::TogetherAi
            | Self::Perplexity
            | Self::OpenRouter => ProviderApiFamily::OpenAiChatCompatible,
            Self::Anthropic => ProviderApiFamily::AnthropicMessages,
            Self::GoogleGemini => ProviderApiFamily::GoogleGeminiGenerateContent,
            Self::Cohere => ProviderApiFamily::CohereChat,
        }
    }
}
