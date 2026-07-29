//! Built-in first-batch provider descriptors.
//! 内置首批供应商描述。

use crate::{
    AccountUsageQueryKind, AuthenticationProjection, CredentialBillingKind, CredentialMode,
    CredentialProfile, ModelDiscoveryKind, OAuthProviderKind, ProtocolRoute, ProviderDescriptor,
    ProviderKind, RequestProjectorKind, ServiceEntry, ServiceEntryKind,
};
use std::collections::BTreeMap;
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, CLAUDE_CODE_MESSAGES_PROFILE_ID,
    GEMINI_GENERATE_CONTENT_PROFILE_ID, GEMINI_INTERACTIONS_PROFILE_ID,
    GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_CODEX_RESPONSES_PROFILE_ID,
    OPENAI_RESPONSES_PROFILE_ID,
};

/// Creates the first-batch MiniMax provider descriptor.
/// 创建首批 MiniMax 供应商描述。
///
/// # Returns
/// 返回值
/// A provider descriptor with declared service entries.
/// 一个带已声明服务入口的供应商描述。
pub fn minimax_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("minimax"),
        display_name: String::from("MiniMax"),
        kind: ProviderKind::System,
        service_entries: vec![
            service_entry(ServiceEntrySpec {
                id: "global",
                request_projector: RequestProjectorKind::MiniMax,
                display_name: "International",
                base_url: "https://api.minimax.io",
                credential_profiles: minimax_credential_profiles(
                    OAuthProviderKind::MiniMaxGlobalTokenPlan,
                ),
                authentication: AuthenticationProjection::XApiKey,
                request_headers: [],
                model_discovery: ModelDiscoveryKind::MiniMaxAnthropic,
                protocol_routes: minimax_protocol_routes(),
            }),
            service_entry(ServiceEntrySpec {
                id: "cn",
                request_projector: RequestProjectorKind::MiniMax,
                display_name: "China",
                base_url: "https://api.minimaxi.com",
                credential_profiles: minimax_credential_profiles(
                    OAuthProviderKind::MiniMaxChinaTokenPlan,
                ),
                authentication: AuthenticationProjection::XApiKey,
                request_headers: [],
                model_discovery: ModelDiscoveryKind::MiniMaxAnthropic,
                protocol_routes: minimax_protocol_routes(),
            }),
        ],
    }
}

/// Creates the Kimi public-platform and Coding Plan provider descriptor.
/// 创建 Kimi 开放平台与 Coding Plan 供应商描述。
///
/// # Returns
/// 返回值
/// A provider whose product entries expose only their independently supported wire protocols.
/// 返回仅暴露各产品已独立支持 Wire 协议的供应商。
pub fn kimi_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("kimi"),
        display_name: String::from("Kimi"),
        kind: ProviderKind::System,
        service_entries: vec![
            service_entry(ServiceEntrySpec {
                id: "platform-cn",
                request_projector: RequestProjectorKind::Kimi,
                display_name: "Kimi Open Platform China",
                base_url: "https://api.moonshot.cn/v1",
                credential_profiles: vec![token_profile(
                    "api-key",
                    "API Key",
                    CredentialBillingKind::PayAsYouGo,
                    AccountUsageQueryKind::Unsupported,
                    Some("sk-"),
                )],
                authentication: AuthenticationProjection::BearerAuthorization,
                request_headers: [],
                model_discovery: ModelDiscoveryKind::OpenAi,
                protocol_routes: [protocol_route(OPENAI_CHAT_PROFILE_ID, "/chat/completions")],
            }),
            service_entry(ServiceEntrySpec {
                id: "platform-global",
                request_projector: RequestProjectorKind::Kimi,
                display_name: "Kimi Open Platform International",
                base_url: "https://api.moonshot.ai/v1",
                credential_profiles: vec![token_profile(
                    "api-key",
                    "API Key",
                    CredentialBillingKind::PayAsYouGo,
                    AccountUsageQueryKind::Unsupported,
                    Some("sk-"),
                )],
                authentication: AuthenticationProjection::BearerAuthorization,
                request_headers: [],
                model_discovery: ModelDiscoveryKind::OpenAi,
                protocol_routes: [protocol_route(OPENAI_CHAT_PROFILE_ID, "/chat/completions")],
            }),
            service_entry(ServiceEntrySpec {
                id: "coding-plan",
                request_projector: RequestProjectorKind::Kimi,
                display_name: "Kimi Code",
                base_url: "https://api.kimi.com/coding/v1",
                credential_profiles: vec![
                    token_profile(
                        "coding-api-key",
                        "Coding API Key",
                        CredentialBillingKind::TokenPlan,
                        AccountUsageQueryKind::KimiCodingPlan,
                        None,
                    ),
                    oauth_profile(
                        "coding-plan-login",
                        "Coding Plan Login",
                        OAuthProviderKind::KimiCodingPlan,
                        CredentialBillingKind::TokenPlan,
                        AccountUsageQueryKind::KimiCodingPlan,
                        Some(AuthenticationProjection::BearerAuthorization),
                    ),
                ],
                authentication: AuthenticationProjection::BearerAuthorization,
                request_headers: [],
                model_discovery: ModelDiscoveryKind::OpenAi,
                protocol_routes: [
                    protocol_route(OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
                    protocol_route_with_overrides(
                        ANTHROPIC_MESSAGES_PROFILE_ID,
                        "/messages",
                        None,
                        Some(AuthenticationProjection::BearerAuthorization),
                        [
                            ("anthropic-version", "2023-06-01"),
                            (
                                "anthropic-beta",
                                "oauth-2025-04-20,interleaved-thinking-2025-05-14",
                            ),
                        ],
                        None,
                    ),
                ],
            }),
        ],
    }
}

/// Creates the DeepSeek official API provider descriptor.
/// 创建 DeepSeek 官方 API 供应商描述。
///
/// # Returns
/// 返回值
/// A provider using the verified OpenAI Chat-compatible route and official model catalog endpoint.
/// 返回使用已验证 OpenAI Chat 兼容路由与官方模型目录端点的供应商。
pub fn deepseek_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("deepseek"),
        display_name: String::from("DeepSeek"),
        kind: ProviderKind::System,
        service_entries: vec![service_entry(ServiceEntrySpec {
            id: "official-api",
            request_projector: RequestProjectorKind::DeepSeek,
            display_name: "DeepSeek API",
            base_url: "https://api.deepseek.com",
            credential_profiles: vec![token_profile(
                "api-key",
                "API Key",
                CredentialBillingKind::PayAsYouGo,
                AccountUsageQueryKind::DeepSeekBalance,
                Some("sk-"),
            )],
            authentication: AuthenticationProjection::BearerAuthorization,
            request_headers: [],
            model_discovery: ModelDiscoveryKind::OpenAi,
            protocol_routes: [
                protocol_route(OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
                protocol_route_with_overrides(
                    ANTHROPIC_MESSAGES_PROFILE_ID,
                    "/anthropic/v1/messages",
                    None,
                    Some(AuthenticationProjection::XApiKey),
                    [("anthropic-version", "2023-06-01")],
                    None,
                ),
            ],
        })],
    }
}

/// Creates the Alibaba Token Plan and Model Studio provider descriptor.
/// 创建阿里云 Token Plan 与百炼 Model Studio 供应商描述。
///
/// # Returns
/// 返回值
/// A provider that keeps OpenAI, Anthropic, and supported Responses routes independently selectable.
/// 返回可独立选择 OpenAI、Anthropic 及已支持 Responses 路由的供应商。
pub fn alibaba_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("alibaba"),
        display_name: String::from("Alibaba Cloud Model Studio"),
        kind: ProviderKind::System,
        service_entries: vec![
            alibaba_token_plan_entry(
                "token-plan-personal-cn",
                "Token Plan Personal China",
                "https://token-plan.cn-beijing.maas.aliyuncs.com",
            ),
            alibaba_token_plan_entry(
                "token-plan-team-cn",
                "Token Plan Team China",
                "https://token-plan.cn-beijing.maas.aliyuncs.com",
            ),
            alibaba_token_plan_entry(
                "token-plan-team-global",
                "Token Plan Team International",
                "https://token-plan.ap-southeast-1.maas.aliyuncs.com",
            ),
            alibaba_model_studio_entry(
                "model-studio-cn",
                "Model Studio China",
                "https://dashscope.aliyuncs.com",
            ),
            alibaba_model_studio_entry(
                "model-studio-singapore",
                "Model Studio Singapore",
                "https://dashscope-intl.aliyuncs.com",
            ),
        ],
    }
}

/// Creates the first-batch OpenRouter provider descriptor.
/// 创建首批 OpenRouter 供应商描述。
///
/// # Returns
/// 返回值
/// A provider descriptor for the public API.
/// 一个公共 API 供应商描述。
pub fn openrouter_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("openrouter"),
        display_name: String::from("OpenRouter"),
        kind: ProviderKind::System,
        service_entries: vec![service_entry(ServiceEntrySpec {
            id: "public-api",
            request_projector: RequestProjectorKind::OpenRouter,
            display_name: "Public API",
            base_url: "https://openrouter.ai/api/v1",
            credential_profiles: vec![token_profile(
                "api-key",
                "API Key",
                CredentialBillingKind::ProviderManaged,
                AccountUsageQueryKind::OpenRouterCredits,
                None,
            )],
            authentication: AuthenticationProjection::BearerAuthorization,
            request_headers: [
                ("http-referer", "https://code.openvulcan.com"),
                ("x-openrouter-title", "Vulcan Code"),
                ("x-openrouter-metadata", "enabled"),
            ],
            model_discovery: ModelDiscoveryKind::OpenRouter,
            protocol_routes: [
                protocol_route(ANTHROPIC_MESSAGES_PROFILE_ID, "/messages"),
                protocol_route(OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
                protocol_route(OPENAI_RESPONSES_PROFILE_ID, "/responses"),
            ],
        })],
    }
}

/// Creates the Tavily provider-tools-only descriptor.
/// 创建仅包含供应商工具的 Tavily 描述。
///
/// # Returns
/// 返回值
/// One API-key entry that exposes no conversation protocol or model catalog.
/// 返回一个不暴露对话协议或模型目录的 API Key 入口。
pub fn tavily_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("tavily"),
        display_name: String::from("Tavily"),
        kind: ProviderKind::System,
        service_entries: vec![ServiceEntry {
            id: String::from("public-api"),
            display_name: String::from("Tavily API"),
            kind: ServiceEntryKind::ProviderTools,
            base_url: String::from("https://api.tavily.com"),
            credential_profiles: vec![token_profile(
                "api-key",
                "API Key",
                CredentialBillingKind::ProviderManaged,
                AccountUsageQueryKind::TavilyCredits,
                Some("tvly-"),
            )],
            authentication: AuthenticationProjection::BearerAuthorization,
            request_headers: BTreeMap::new(),
            model_discovery: ModelDiscoveryKind::ProviderToolsOnly,
            request_projector: RequestProjectorKind::Direct,
            protocol_routes: Vec::new(),
        }],
    }
}

/// Creates the OpenAI public API provider descriptor.
/// 创建 OpenAI 公共 API 供应商描述。
///
/// # Returns
/// 返回值
/// A provider whose credential chooses Chat Completions or Responses before execution.
/// 返回在执行前由凭据选择 Chat Completions 或 Responses 的供应商。
pub fn openai_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("openai"),
        display_name: String::from("OpenAI"),
        kind: ProviderKind::System,
        service_entries: vec![
            service_entry(ServiceEntrySpec {
                id: "public-api",
                request_projector: RequestProjectorKind::Direct,
                display_name: "OpenAI API",
                base_url: "https://api.openai.com/v1",
                credential_profiles: vec![token_profile(
                    "api-key",
                    "API Key",
                    CredentialBillingKind::PayAsYouGo,
                    AccountUsageQueryKind::Unsupported,
                    None,
                )],
                authentication: AuthenticationProjection::BearerAuthorization,
                request_headers: [],
                model_discovery: ModelDiscoveryKind::OpenAi,
                protocol_routes: [
                    protocol_route(OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
                    protocol_route(OPENAI_RESPONSES_PROFILE_ID, "/responses"),
                ],
            }),
            service_entry(ServiceEntrySpec {
                id: "codex-subscription",
                request_projector: RequestProjectorKind::Direct,
                display_name: "OpenAI Codex Subscription",
                base_url: "https://chatgpt.com/backend-api/codex",
                credential_profiles: vec![oauth_profile(
                    "codex-oauth",
                    "Codex Login",
                    OAuthProviderKind::OpenAiCodex,
                    CredentialBillingKind::ProviderManaged,
                    AccountUsageQueryKind::Unsupported,
                    None,
                )],
                authentication: AuthenticationProjection::OpenAiCodexBearer,
                request_headers: [
                    ("originator", "codex_cli_rs"),
                    (
                        "user-agent",
                        "codex_cli_rs/0.144.1 (Windows 11; x86_64) vulcan-code/0.1",
                    ),
                ],
                model_discovery: ModelDiscoveryKind::OpenAiCodex,
                protocol_routes: [protocol_route(
                    OPENAI_CODEX_RESPONSES_PROFILE_ID,
                    "/responses",
                )],
            }),
        ],
    }
}

/// Creates the Anthropic public API provider descriptor.
/// 创建 Anthropic 公共 API 供应商描述。
///
/// # Returns
/// 返回值
/// A provider using API-key authentication and the exact Messages profile.
/// 返回使用 API Key 鉴权及精确 Messages Profile 的供应商。
pub fn anthropic_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("anthropic"),
        display_name: String::from("Anthropic"),
        kind: ProviderKind::System,
        service_entries: vec![
            service_entry(ServiceEntrySpec {
                id: "public-api",
                request_projector: RequestProjectorKind::Direct,
                display_name: "Claude API",
                base_url: "https://api.anthropic.com",
                credential_profiles: vec![token_profile(
                    "api-key",
                    "API Key",
                    CredentialBillingKind::PayAsYouGo,
                    AccountUsageQueryKind::Unsupported,
                    None,
                )],
                authentication: AuthenticationProjection::XApiKey,
                request_headers: [("anthropic-version", "2023-06-01")],
                model_discovery: ModelDiscoveryKind::Anthropic,
                protocol_routes: [protocol_route(
                    ANTHROPIC_MESSAGES_PROFILE_ID,
                    "/v1/messages",
                )],
            }),
            service_entry(ServiceEntrySpec {
                id: "claude-code-subscription",
                request_projector: RequestProjectorKind::Direct,
                display_name: "Claude Code Subscription",
                base_url: "https://api.anthropic.com",
                credential_profiles: vec![oauth_profile(
                    "claude-code-oauth",
                    "Claude Code Login",
                    OAuthProviderKind::ClaudeCode,
                    CredentialBillingKind::ProviderManaged,
                    AccountUsageQueryKind::Unsupported,
                    None,
                )],
                authentication: AuthenticationProjection::BearerAuthorization,
                request_headers: [
                    ("anthropic-version", "2023-06-01"),
                    (
                        "anthropic-beta",
                        "claude-code-20250219,oauth-2025-04-20,interleaved-thinking-2025-05-14",
                    ),
                ],
                model_discovery: ModelDiscoveryKind::Anthropic,
                protocol_routes: [protocol_route(
                    CLAUDE_CODE_MESSAGES_PROFILE_ID,
                    "/v1/messages?beta=true",
                )],
            }),
        ],
    }
}

/// Creates the Google Gemini public API provider descriptor.
/// 创建 Google Gemini 公共 API 供应商描述。
///
/// # Returns
/// 返回值
/// A provider using AI Studio API keys and dynamic protocol-owned model paths.
/// 返回使用 AI Studio API Key 及协议所有动态模型路径的供应商。
pub fn google_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("google"),
        display_name: String::from("Google"),
        kind: ProviderKind::System,
        service_entries: vec![
            service_entry(ServiceEntrySpec {
                id: "gemini-api",
                request_projector: RequestProjectorKind::Google,
                display_name: "Gemini API Key",
                base_url: "https://generativelanguage.googleapis.com/v1beta",
                credential_profiles: vec![token_profile(
                    "api-key",
                    "API Key",
                    CredentialBillingKind::PayAsYouGo,
                    AccountUsageQueryKind::Unsupported,
                    None,
                )],
                authentication: AuthenticationProjection::GoogleApiKey,
                request_headers: [],
                model_discovery: ModelDiscoveryKind::Gemini,
                protocol_routes: [
                    protocol_profile_route(GEMINI_GENERATE_CONTENT_PROFILE_ID),
                    protocol_profile_route(GEMINI_INTERACTIONS_PROFILE_ID),
                ],
            }),
            service_entry(ServiceEntrySpec {
                id: "gemini-oauth",
                request_projector: RequestProjectorKind::Google,
                display_name: "Gemini OAuth",
                base_url: "https://generativelanguage.googleapis.com/v1beta",
                credential_profiles: vec![oauth_profile(
                    "gemini-oauth",
                    "Gemini Login",
                    OAuthProviderKind::GoogleGemini,
                    CredentialBillingKind::ProviderManaged,
                    AccountUsageQueryKind::Unsupported,
                    None,
                )],
                authentication: AuthenticationProjection::GoogleGeminiBearer,
                request_headers: [],
                model_discovery: ModelDiscoveryKind::Gemini,
                protocol_routes: [
                    protocol_profile_route(GEMINI_GENERATE_CONTENT_PROFILE_ID),
                    protocol_profile_route(GEMINI_INTERACTIONS_PROFILE_ID),
                ],
            }),
            service_entry(ServiceEntrySpec {
                id: "code-assist",
                request_projector: RequestProjectorKind::Google,
                display_name: "Google Code Assist",
                base_url: "https://cloudcode-pa.googleapis.com",
                credential_profiles: vec![oauth_profile(
                    "code-assist-oauth",
                    "Code Assist Login",
                    OAuthProviderKind::GoogleCodeAssist,
                    CredentialBillingKind::ProviderManaged,
                    AccountUsageQueryKind::Unsupported,
                    None,
                )],
                authentication: AuthenticationProjection::GoogleCodeAssistBearer,
                request_headers: [("user-agent", "vulcan-code/0.1")],
                model_discovery: ModelDiscoveryKind::GoogleCodeAssist,
                protocol_routes: [protocol_route(
                    GOOGLE_CODE_ASSIST_PROFILE_ID,
                    "/v1internal:streamGenerateContent?alt=sse",
                )],
            }),
        ],
    }
}

/// Creates one Alibaba Token Plan entry with independently rooted protocol routes.
/// 创建一个具有独立协议根地址的阿里云 Token Plan 入口。
///
/// # Parameters
/// 参数
/// `id`, `display_name`, and `origin` identify the exact regional product.
/// `id`、`display_name` 与 `origin` 标识精确的区域产品。
///
/// # Returns
/// 返回值
/// A Token Plan service entry exposing Chat, Anthropic Messages, and provider-hosted Responses.
/// 返回暴露 Chat、Anthropic Messages 与供应商托管 Responses 的 Token Plan 服务入口。
fn alibaba_token_plan_entry(id: &str, display_name: &str, origin: &str) -> ServiceEntry {
    let openai_base_url = format!("{}/compatible-mode/v1", origin.trim_end_matches('/'));
    let anthropic_base_url = format!("{}/apps/anthropic", origin.trim_end_matches('/'));
    ServiceEntry {
        id: id.to_string(),
        display_name: display_name.to_string(),
        kind: ServiceEntryKind::Conversation,
        base_url: openai_base_url.clone(),
        credential_profiles: vec![token_profile(
            "token-plan-api-key",
            "Token Plan API Key",
            CredentialBillingKind::TokenPlan,
            AccountUsageQueryKind::Unsupported,
            Some("sk-"),
        )],
        authentication: AuthenticationProjection::BearerAuthorization,
        request_headers: BTreeMap::new(),
        model_discovery: ModelDiscoveryKind::OpenAi,
        request_projector: RequestProjectorKind::Alibaba,
        protocol_routes: vec![
            protocol_route(OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
            protocol_route_with_overrides(
                ANTHROPIC_MESSAGES_PROFILE_ID,
                "/v1/messages",
                Some(anthropic_base_url.as_str()),
                Some(AuthenticationProjection::XApiKey),
                [("anthropic-version", "2023-06-01")],
                Some(openai_base_url.as_str()),
            ),
        ],
    }
}

/// Creates one Alibaba Model Studio API entry with OpenAI and Anthropic compatibility.
/// 创建一个同时兼容 OpenAI 与 Anthropic 的阿里云百炼 API 入口。
///
/// # Parameters
/// 参数
/// `id`, `display_name`, and `origin` identify the exact regional API product.
/// `id`、`display_name` 与 `origin` 标识精确的区域 API 产品。
///
/// # Returns
/// 返回值
/// A pay-as-you-go service entry with protocol-specific roots and authentication.
/// 返回具有协议专属根地址与鉴权方式的按量计费服务入口。
fn alibaba_model_studio_entry(id: &str, display_name: &str, origin: &str) -> ServiceEntry {
    let openai_base_url = format!("{}/compatible-mode/v1", origin.trim_end_matches('/'));
    let anthropic_base_url = format!("{}/apps/anthropic", origin.trim_end_matches('/'));
    ServiceEntry {
        id: id.to_string(),
        display_name: display_name.to_string(),
        kind: ServiceEntryKind::Conversation,
        base_url: openai_base_url.clone(),
        credential_profiles: vec![token_profile(
            "api-key",
            "API Key",
            CredentialBillingKind::PayAsYouGo,
            AccountUsageQueryKind::Unsupported,
            Some("sk-"),
        )],
        authentication: AuthenticationProjection::BearerAuthorization,
        request_headers: BTreeMap::new(),
        model_discovery: ModelDiscoveryKind::OpenAi,
        request_projector: RequestProjectorKind::Alibaba,
        protocol_routes: vec![
            protocol_route(OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
            protocol_route_with_overrides(
                ANTHROPIC_MESSAGES_PROFILE_ID,
                "/v1/messages",
                Some(anthropic_base_url.as_str()),
                Some(AuthenticationProjection::XApiKey),
                [("anthropic-version", "2023-06-01")],
                Some(openai_base_url.as_str()),
            ),
        ],
    }
}

/// Creates the three MiniMax credential profiles shared by one regional entry.
/// 创建一个 MiniMax 区域入口共享的三种凭据配置档。
///
/// # Parameters
/// 参数
/// `oauth_provider` is the exact regional MiniMax Device Flow implementation.
/// `oauth_provider` 是精确的区域 MiniMax Device Flow 实现。
///
/// # Returns
/// 返回值
/// Pay-as-you-go key, Token Plan key, and Token Plan OAuth profiles.
/// 返回按量 Key、Token Plan Key 与 Token Plan OAuth 配置档。
fn minimax_credential_profiles(oauth_provider: OAuthProviderKind) -> Vec<CredentialProfile> {
    vec![
        token_profile(
            "payg-api-key",
            "Pay-as-you-go API Key",
            CredentialBillingKind::PayAsYouGo,
            AccountUsageQueryKind::Unsupported,
            Some("sk-api-"),
        ),
        token_profile(
            "token-plan-key",
            "Token Plan Key",
            CredentialBillingKind::TokenPlan,
            AccountUsageQueryKind::MiniMaxTokenPlan,
            Some("sk-cp-"),
        ),
        oauth_profile(
            "token-plan-oauth",
            "Token Plan Login",
            oauth_provider,
            CredentialBillingKind::TokenPlan,
            AccountUsageQueryKind::MiniMaxTokenPlan,
            Some(AuthenticationProjection::BearerAuthorization),
        ),
    ]
}

/// Creates one token credential profile.
/// 创建一个 Token 凭据配置档。
///
/// # Parameters
/// 参数
/// The arguments declare stable identity, display name, billing, usage, and optional key prefix.
/// 参数声明稳定标识、显示名、计费、用量与可选 Key 前缀。
///
/// # Returns
/// 返回值
/// A complete token credential profile.
/// 一个完整的 Token 凭据配置档。
fn token_profile(
    id: &str,
    display_name: &str,
    billing_kind: CredentialBillingKind,
    account_usage_query: AccountUsageQueryKind,
    key_prefix: Option<&str>,
) -> CredentialProfile {
    CredentialProfile {
        id: id.to_string(),
        display_name: display_name.to_string(),
        credential_mode: CredentialMode::Token,
        oauth_provider: None,
        billing_kind,
        account_usage_query,
        authentication: None,
        key_prefix: key_prefix.map(str::to_string),
    }
}

/// Creates one renewable OAuth credential profile.
/// 创建一个可续期 OAuth 凭据配置档。
///
/// # Parameters
/// 参数
/// The arguments declare stable identity, OAuth flow, billing, and usage semantics.
/// 参数声明稳定标识、OAuth 流程、计费与用量语义。
///
/// # Returns
/// 返回值
/// A complete renewable credential profile.
/// 一个完整的可续期凭据配置档。
fn oauth_profile(
    id: &str,
    display_name: &str,
    oauth_provider: OAuthProviderKind,
    billing_kind: CredentialBillingKind,
    account_usage_query: AccountUsageQueryKind,
    authentication: Option<AuthenticationProjection>,
) -> CredentialProfile {
    CredentialProfile {
        id: id.to_string(),
        display_name: display_name.to_string(),
        credential_mode: CredentialMode::Auth,
        oauth_provider: Some(oauth_provider),
        billing_kind,
        account_usage_query,
        authentication,
        key_prefix: None,
    }
}

/// Carries one complete declarative provider service-entry specification.
/// 承载一项完整的声明式供应商服务入口规格。
struct ServiceEntrySpec<const H: usize, P> {
    /// Stable service-entry identifier.
    /// 稳定服务入口标识。
    id: &'static str,
    /// Registered provider request projector.
    /// 已注册供应商请求投影器。
    request_projector: RequestProjectorKind,
    /// User-visible service-entry name.
    /// 用户可见服务入口名称。
    display_name: &'static str,
    /// Exact provider base URL.
    /// 精确供应商基础 URL。
    base_url: &'static str,
    /// Exact credential profiles available through this entry.
    /// 当前入口可用的精确凭据配置档。
    credential_profiles: Vec<CredentialProfile>,
    /// Default authentication projection.
    /// 默认鉴权投影。
    authentication: AuthenticationProjection,
    /// Fixed non-secret request headers.
    /// 固定非秘密请求头。
    request_headers: [(&'static str, &'static str); H],
    /// Default model discovery strategy.
    /// 默认模型发现策略。
    model_discovery: ModelDiscoveryKind,
    /// Exact protocol routes exposed by this entry.
    /// 该入口暴露的精确协议路由。
    protocol_routes: P,
}

/// Creates one provider service entry from a complete declarative specification.
/// 根据完整声明式规格创建一个供应商服务入口。
///
/// # Parameters
/// 参数
/// `spec` contains every immutable service-entry field.
/// `spec` 包含服务入口的全部不可变字段。
///
/// # Returns
/// 返回值
/// A normalized service entry ready for graph validation.
/// 返回可供图校验使用的规范化服务入口。
fn service_entry<const H: usize, P>(spec: ServiceEntrySpec<H, P>) -> ServiceEntry
where
    P: IntoIterator<Item = ProtocolRoute>,
{
    ServiceEntry {
        id: spec.id.to_string(),
        display_name: spec.display_name.to_string(),
        kind: ServiceEntryKind::Conversation,
        base_url: spec.base_url.to_string(),
        credential_profiles: spec.credential_profiles,
        authentication: spec.authentication,
        request_headers: BTreeMap::from(
            spec.request_headers
                .map(|(name, value)| (name.to_string(), value.to_string())),
        ),
        model_discovery: spec.model_discovery,
        request_projector: spec.request_projector,
        protocol_routes: spec.protocol_routes.into_iter().collect(),
    }
}

/// Creates one verified protocol route.
/// 创建一条已验证协议路由。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` is the public protocol profile and `endpoint_path` is the exact path.
/// `protocol_profile_id` 是公共协议 Profile，`endpoint_path` 是精确路径。
///
/// # Returns
/// 返回值
/// A verified protocol route.
/// 一条已验证协议路由。
fn protocol_route(protocol_profile_id: &str, endpoint_path: &str) -> ProtocolRoute {
    ProtocolRoute {
        protocol_profile_id: protocol_profile_id.to_string(),
        endpoint_path: Some(endpoint_path.to_string()),
        base_url_override: None,
        request_headers: BTreeMap::new(),
        authentication: None,
        model_discovery: None,
        model_discovery_path: None,
        model_discovery_base_url_override: None,
        verified: true,
    }
}

/// Creates one verified protocol route with exact per-protocol overrides.
/// 创建一个具有精确协议级覆盖项的已验证路由。
///
/// # Parameters
/// 参数
/// The arguments declare endpoint, base URL, authentication, headers, and discovery base.
/// 参数声明端点、基础 URL、鉴权、请求头与模型发现基础地址。
///
/// # Returns
/// 返回值
/// A complete protocol route whose fields have a single declared source.
/// 返回字段均具有唯一声明来源的完整协议路由。
fn protocol_route_with_overrides<const H: usize>(
    protocol_profile_id: &str,
    endpoint_path: &str,
    base_url_override: Option<&str>,
    authentication: Option<AuthenticationProjection>,
    request_headers: [(&str, &str); H],
    model_discovery_base_url_override: Option<&str>,
) -> ProtocolRoute {
    ProtocolRoute {
        protocol_profile_id: protocol_profile_id.to_string(),
        endpoint_path: Some(endpoint_path.to_string()),
        base_url_override: base_url_override.map(str::to_string),
        request_headers: BTreeMap::from(
            request_headers.map(|(name, value)| (name.to_string(), value.to_string())),
        ),
        authentication,
        model_discovery: Some(ModelDiscoveryKind::OpenAi),
        model_discovery_path: None,
        model_discovery_base_url_override: model_discovery_base_url_override.map(str::to_string),
        verified: true,
    }
}

/// Creates one verified route that retains the protocol-encoded dynamic path.
/// 创建一条保留协议编码动态路径的已验证路由。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` is the exact public protocol profile.
/// `protocol_profile_id` 是精确公共协议 Profile。
///
/// # Returns
/// 返回值
/// A verified route without a provider path override.
/// 返回不包含供应商路径覆盖的已验证路由。
fn protocol_profile_route(protocol_profile_id: &str) -> ProtocolRoute {
    ProtocolRoute {
        protocol_profile_id: protocol_profile_id.to_string(),
        endpoint_path: None,
        base_url_override: None,
        request_headers: BTreeMap::new(),
        authentication: None,
        model_discovery: None,
        model_discovery_path: None,
        model_discovery_base_url_override: None,
        verified: true,
    }
}

/// Creates MiniMax text routes whose wire protocols require different authentication and discovery.
/// 创建鉴权与发现方式随 Wire 协议变化的 MiniMax 文本路由。
///
/// # Returns
/// 返回值
/// Verified Anthropic and OpenAI-compatible routes for one MiniMax service entry.
/// 返回一个 MiniMax 服务入口的已验证 Anthropic 与 OpenAI 兼容路由。
fn minimax_protocol_routes() -> [ProtocolRoute; 2] {
    [
        protocol_route(ANTHROPIC_MESSAGES_PROFILE_ID, "/anthropic/v1/messages"),
        ProtocolRoute {
            protocol_profile_id: String::from(OPENAI_CHAT_PROFILE_ID),
            endpoint_path: Some(String::from("/v1/chat/completions")),
            base_url_override: None,
            request_headers: BTreeMap::new(),
            authentication: Some(AuthenticationProjection::BearerAuthorization),
            model_discovery: Some(ModelDiscoveryKind::MiniMaxOpenAi),
            model_discovery_path: None,
            model_discovery_base_url_override: None,
            verified: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{kimi_descriptor, minimax_descriptor};
    use crate::{
        AccountUsageQueryKind, AuthenticationProjection, CredentialMode, OAuthProviderKind,
    };

    /// Verifies MiniMax regions own shared credential profiles instead of duplicated product entries.
    /// 验证 MiniMax 区域持有共享凭据配置档而不是重复产品入口。
    #[test]
    fn minimax_descriptor_separates_region_from_credentials() {
        let descriptor = minimax_descriptor();
        assert_eq!(
            descriptor
                .service_entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["global", "cn"]
        );
        for entry in &descriptor.service_entries {
            assert_eq!(entry.credential_profiles.len(), 3);
            assert_eq!(
                entry
                    .credential_profile("payg-api-key")
                    .expect("pay-as-you-go profile")
                    .account_usage_query,
                AccountUsageQueryKind::Unsupported
            );
            assert_eq!(
                entry
                    .credential_profile("token-plan-key")
                    .expect("Plan key profile")
                    .account_usage_query,
                AccountUsageQueryKind::MiniMaxTokenPlan
            );
            assert_eq!(
                entry
                    .credential_profile("token-plan-oauth")
                    .expect("Plan OAuth profile")
                    .credential_mode,
                CredentialMode::Auth
            );
            assert_eq!(
                entry
                    .credential_profile("token-plan-oauth")
                    .expect("Plan OAuth profile")
                    .authentication,
                Some(AuthenticationProjection::BearerAuthorization)
            );
        }
        assert_eq!(
            descriptor.service_entries[0]
                .credential_profile("token-plan-oauth")
                .and_then(|profile| profile.oauth_provider),
            Some(OAuthProviderKind::MiniMaxGlobalTokenPlan)
        );
        assert_eq!(
            descriptor.service_entries[1]
                .credential_profile("token-plan-oauth")
                .and_then(|profile| profile.oauth_provider),
            Some(OAuthProviderKind::MiniMaxChinaTokenPlan)
        );
    }

    /// Verifies Kimi Coding Plan exposes both static keys and renewable browser login.
    /// 验证 Kimi Coding Plan 同时暴露静态 Key 与可续期浏览器登录。
    #[test]
    fn kimi_coding_plan_registers_login_and_usage() {
        let descriptor = kimi_descriptor();
        let entry = descriptor
            .service_entries
            .iter()
            .find(|entry| entry.id == "coding-plan")
            .expect("Kimi Coding Plan entry");
        let api_key = entry
            .credential_profile("coding-api-key")
            .expect("Kimi Coding API key");
        assert_eq!(
            api_key.account_usage_query,
            AccountUsageQueryKind::KimiCodingPlan
        );
        let login = entry
            .credential_profile("coding-plan-login")
            .expect("Kimi Coding Plan login");
        assert_eq!(login.credential_mode, CredentialMode::Auth);
        assert_eq!(
            login.oauth_provider,
            Some(OAuthProviderKind::KimiCodingPlan)
        );
        assert_eq!(
            login.account_usage_query,
            AccountUsageQueryKind::KimiCodingPlan
        );
    }
}
