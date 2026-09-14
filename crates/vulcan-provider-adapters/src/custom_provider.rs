//! User-defined OpenAI, Anthropic, and Gemini compatible provider contracts.
//! 用户自定义 OpenAI、Anthropic 与 Gemini 兼容供应商契约。

use crate::{
    AccountUsageQueryKind, AuthenticationProjection, CredentialBillingKind,
    CredentialDirectoryVisibility, CredentialMode, CredentialProfile, ModelDiscoveryKind,
    ProtocolRoute, ProviderCategory, ProviderDescriptor, ProviderKind, ResponsesRouteFeatures,
    ServiceEntry, provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::InferenceUsageScope;
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Identifies one protocol exposed by a user-defined compatible provider.
/// 标识用户自定义兼容供应商公开的一种协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomProviderProtocol {
    /// Anthropic Messages protocol.
    /// Anthropic Messages 协议。
    AnthropicMessages,
    /// OpenAI Chat Completions protocol.
    /// OpenAI Chat Completions 协议。
    OpenAiChat,
    /// OpenAI Responses protocol.
    /// OpenAI Responses 协议。
    OpenAiResponses,
    /// OpenAI Codex Responses protocol with explicit Codex request identity.
    /// 带显式 Codex 请求身份的 OpenAI Codex Responses 协议。
    OpenAiCodexResponses,
    /// Google Gemini generateContent protocol.
    /// Google Gemini generateContent 协议。
    GeminiGenerateContent,
}

/// Identifies the API-key projection selected for one custom provider.
/// 标识一个自定义供应商选择的 API Key 投影。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomProviderAuthentication {
    /// Sends the key through `Authorization: Bearer`.
    /// 通过 `Authorization: Bearer` 发送 Key。
    BearerAuthorization,
    /// Sends the key through `x-api-key`.
    /// 通过 `x-api-key` 发送 Key。
    XApiKey,
    /// Sends the key through the Gemini API-key projection.
    /// 通过 Gemini API Key 投影发送 Key。
    GoogleApiKey,
}

/// Selects how one custom provider exposes its model catalog.
/// 选择一个自定义供应商如何公开模型目录。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CustomProviderModelDiscoveryMode {
    /// Uses the protocol-standard models endpoint.
    /// 使用协议标准模型目录端点。
    #[default]
    Standard,
    /// Uses one user-declared same-origin relative path.
    /// 使用用户声明的同源相对路径。
    CustomPath,
    /// Skips remote discovery and relies on manual model declarations.
    /// 跳过远程发现并依赖手工模型声明。
    Manual,
}

impl CustomProviderAuthentication {
    /// Projects the public custom setting into the runtime authentication contract.
    /// 将公开自定义设置投影为运行时鉴权契约。
    fn projection(self) -> AuthenticationProjection {
        match self {
            Self::BearerAuthorization => AuthenticationProjection::BearerAuthorization,
            Self::XApiKey => AuthenticationProjection::XApiKey,
            Self::GoogleApiKey => AuthenticationProjection::GoogleApiKey,
        }
    }
}

impl CustomProviderProtocol {
    /// Returns the runtime protocol profile identifier.
    /// 返回运行时协议 Profile 标识。
    pub fn profile_id(self) -> &'static str {
        match self {
            Self::AnthropicMessages => ANTHROPIC_MESSAGES_PROFILE_ID,
            Self::OpenAiChat => OPENAI_CHAT_PROFILE_ID,
            Self::OpenAiResponses => OPENAI_RESPONSES_PROFILE_ID,
            Self::OpenAiCodexResponses => OPENAI_CODEX_RESPONSES_PROFILE_ID,
            Self::GeminiGenerateContent => GEMINI_GENERATE_CONTENT_PROFILE_ID,
        }
    }

    /// Returns the provider endpoint path used by this protocol.
    /// 返回此协议使用的供应商端点路径。
    fn endpoint_path(self) -> Option<&'static str> {
        match self {
            Self::AnthropicMessages => Some("/messages"),
            Self::OpenAiChat => Some("/chat/completions"),
            Self::OpenAiResponses => Some("/responses"),
            Self::OpenAiCodexResponses => Some("/responses"),
            Self::GeminiGenerateContent => None,
        }
    }
}

/// Describes one per-protocol override for a user-defined compatible provider.
/// 描述用户自定义兼容供应商的逐协议覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomProtocolRoute {
    /// The exact protocol this route overrides.
    /// 此路由覆盖的精确协议。
    pub protocol: CustomProviderProtocol,
    /// Route-specific base URL; inherits the shared provider base URL when omitted.
    /// 路由专属基础 URL；省略时继承供应商共享基础 URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Route-specific endpoint path; uses the protocol default when omitted.
    /// 路由专属端点路径；省略时使用协议默认端点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_path: Option<String>,
    /// Route-specific API-key projection; inherits the provider projection when omitted.
    /// 路由专属 API Key 投影；省略时继承供应商投影。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<CustomProviderAuthentication>,
    /// Route-specific model-discovery mode; inherits the provider mode when omitted.
    /// 路由专属模型发现模式；省略时继承供应商模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery_mode: Option<CustomProviderModelDiscoveryMode>,
    /// Route-specific same-origin discovery path.
    /// 路由专属同源发现路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery_path: Option<String>,
    /// Bounded non-sensitive fixed request headers for this route only.
    /// 仅作用于该路由的有界非敏感固定请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub request_headers: BTreeMap<String, String>,
}

/// Describes one complete user-defined compatible provider.
/// 描述一个完整的用户自定义兼容供应商。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomProviderDefinition {
    /// Stable provider identifier with the required `custom:` prefix.
    /// 带必需 `custom:` 前缀的稳定供应商标识。
    pub id: String,
    /// User-visible provider name.
    /// 用户可见供应商名称。
    pub display_name: String,
    /// Exact shared API base URL.
    /// 精确共享 API 基础 URL。
    pub base_url: String,
    /// The verified compatible protocols served by this provider.
    /// 该供应商提供的已验证兼容协议集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocols: Vec<CustomProviderProtocol>,
    /// Exact API-key header projection.
    /// 精确 API Key 请求头投影。
    pub authentication: CustomProviderAuthentication,
    /// Selected remote-model discovery behavior.
    /// 选定的远程模型发现行为。
    #[serde(default)]
    pub model_discovery_mode: CustomProviderModelDiscoveryMode,
    /// Same-origin relative model-discovery path used only by custom-path mode.
    /// 仅由自定义路径模式使用的同源相对模型目录路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery_path: Option<String>,
    /// Per-protocol route overrides; each protocol may appear at most once.
    /// 逐协议路由覆盖；每个协议最多出现一次。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocol_routes: Vec<CustomProtocolRoute>,
    /// Bounded non-sensitive fixed request headers shared by every route.
    /// 全部路由共享的有界非敏感固定请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub request_headers: BTreeMap<String, String>,
    /// Optional header name that receives the current regular-conversation session identifier.
    /// 可选的请求头名称；配置后接收当前常规会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_header: Option<String>,
}

/// Validates one exact user-owned provider identifier against the shared manifest contract.
/// 根据共享 Manifest 契约校验一个精确的用户自有供应商标识。
///
/// # Parameters
/// 参数
/// `provider_id` is the exact persisted or request-scoped identity without normalization.
/// `provider_id` 是未经规范化的精确持久化或请求级身份。
///
/// # Returns
/// 返回值
/// Success only for a bounded `custom:` identifier using the stable custom-provider alphabet.
/// 仅当标识有界、使用 `custom:` 前缀及稳定自定义供应商字符集时返回成功。
pub fn validate_custom_provider_identifier(provider_id: &str) -> Result<(), String> {
    let suffix = provider_id.strip_prefix("custom:").ok_or_else(|| {
        String::from("Custom provider identifiers must use a non-empty `custom:` prefix.")
    })?;
    if suffix.is_empty() {
        return Err(String::from(
            "Custom provider identifiers must use a non-empty `custom:` prefix.",
        ));
    }
    if provider_id.len() > MAX_PROVIDER_IDENTIFIER_LEN {
        return Err(format!(
            "Custom provider identifiers must not exceed {MAX_PROVIDER_IDENTIFIER_LEN} ASCII bytes."
        ));
    }
    if !suffix
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(String::from(
            "Custom provider identifiers must use `custom:` followed by `[A-Za-z0-9._-]`.",
        ));
    }
    Ok(())
}

impl CustomProviderDefinition {
    /// Returns the enabled protocol set.
    /// 返回启用的协议集合。
    ///
    /// # Returns
    /// 返回值
    /// The exact declared protocol collection.
    /// 返回精确声明的协议集合。
    pub fn effective_protocols(&self) -> Vec<CustomProviderProtocol> {
        self.protocols.clone()
    }

    /// Validates and projects this definition into the runtime provider descriptor.
    /// 校验此定义并投影为运行时供应商描述。
    ///
    /// # Returns
    /// 返回值
    /// A strict descriptor accepted by the effective provider registry.
    /// 返回有效供应商注册表接受的严格描述。
    pub fn into_descriptor(self) -> Result<ProviderDescriptor, String> {
        let id = self.id.trim().to_string();
        validate_custom_provider_identifier(id.as_str())?;
        let display_name = self.display_name.trim().to_string();
        if display_name.is_empty() {
            return Err(String::from("Custom provider display name is required."));
        }
        let base_url = validate_custom_provider_base_url(self.base_url.as_str())?;
        let protocols = self.effective_protocols();
        if protocols.is_empty() {
            return Err(String::from(
                "Custom provider must declare at least one compatible protocol.",
            ));
        }
        let mut seen_protocols = BTreeSet::new();
        for protocol in &protocols {
            if !seen_protocols.insert(*protocol) {
                return Err(format!(
                    "Custom provider protocol `{}` is declared more than once.",
                    protocol.profile_id()
                ));
            }
        }
        let session_header = self
            .session_header
            .as_deref()
            .map(str::trim)
            .filter(|header| !header.is_empty())
            .map(str::to_owned);
        validate_custom_provider_session_header(session_header.as_deref())?;
        // route_overrides indexes route declarations by protocol; duplicates are rejected below.
        // route_overrides 按协议索引路由声明；重复项在下面拒绝。
        let mut route_overrides = BTreeMap::new();
        for route in &self.protocol_routes {
            if !protocols.contains(&route.protocol) {
                return Err(format!(
                    "Custom provider route protocol `{}` is not part of the enabled protocol set.",
                    route.protocol.profile_id()
                ));
            }
            if route_overrides.insert(route.protocol, route).is_some() {
                return Err(format!(
                    "Custom provider route protocol `{}` is declared more than once.",
                    route.protocol.profile_id()
                ));
            }
        }
        validate_custom_provider_fixed_headers(&self.request_headers)?;
        // The entry-level discovery is the shared legacy fallback; multi-protocol routes always
        // declare their own discovery, so the entry value follows the first enabled protocol.
        // 入口级发现是共享的旧版回退；多协议路由总是声明自己的发现，
        // 因此入口值跟随第一个启用的协议。
        let model_discovery = self.resolve_discovery(
            self.model_discovery_mode,
            self.model_discovery_path.as_deref(),
            protocols.first(),
        )?;
        let protocol_routes = protocols
            .iter()
            .map(|protocol| {
                let override_route = route_overrides.get(protocol);
                let route_base_url = override_route
                    .and_then(|route| route.base_url.as_deref())
                    .map(validate_custom_provider_base_url)
                    .transpose()?;
                let endpoint_path = override_route
                    .and_then(|route| route.endpoint_path.as_deref())
                    .map(validate_custom_provider_endpoint_path)
                    .transpose()?;
                let route_headers = override_route
                    .map(|route| route.request_headers.clone())
                    .unwrap_or_default();
                validate_custom_provider_fixed_headers(&route_headers)?;
                let route_discovery = self.resolve_discovery(
                    override_route
                        .and_then(|route| route.model_discovery_mode)
                        .unwrap_or(self.model_discovery_mode),
                    override_route
                        .and_then(|route| route.model_discovery_path.as_deref())
                        .or(self.model_discovery_path.as_deref()),
                    Some(protocol),
                )?;
                // Route-level authentication overrides the shared provider projection; an
                // absent route value inherits the entry-level projection through `None`.
                // 路由级鉴权覆盖共享供应商投影；路由缺省时通过 `None` 继承入口级投影。
                let route_authentication = override_route
                    .and_then(|route| route.authentication)
                    .map(|authentication| authentication.projection());
                Ok(ProtocolRoute {
                    protocol_profile_id: protocol.profile_id().to_string(),
                    endpoint_path: endpoint_path
                        .or_else(|| protocol.endpoint_path().map(str::to_string)),
                    base_url_override: route_base_url,
                    request_headers: route_headers,
                    authentication: route_authentication,
                    model_discovery: Some(route_discovery.1),
                    model_discovery_path: route_discovery.0,
                    model_discovery_base_url_override: None,
                    responses_features: (*protocol == CustomProviderProtocol::OpenAiCodexResponses)
                        .then_some(ResponsesRouteFeatures::default()),
                    input_resource_constraints: Vec::new(),
                    usage_extraction_profiles: Vec::new(),
                    request_projection_profiles: Vec::new(),
                    response_projection_profiles: Vec::new(),
                    provider_request_id_headers: Vec::new(),
                    verified: true,
                    // User-owned custom routes never declare a bundled reasoning replay contract,
                    // so their request bytes stay exactly as they were.
                    // 用户自有自定义路由绝不声明随包推理回传契约，因此其请求字节保持原样。
                    reasoning_replay_contract: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ProviderDescriptor {
            id,
            display_name,
            kind: ProviderKind::Custom,
            category: ProviderCategory::Unclassified,
            credential_directory_visibility: CredentialDirectoryVisibility::ConfiguredOnly,
            credential_registration: crate::CredentialRegistrationKind::Direct,
            promotion: None,
            service_entries: vec![ServiceEntry {
                id: String::from("default"),
                display_name: String::from("Compatible API"),
                kind: crate::ServiceEntryKind::Conversation,
                base_url,
                credential_profiles: vec![CredentialProfile {
                    id: String::from("api-key"),
                    display_name: String::from("API Key"),
                    credential_mode: CredentialMode::Token,
                    authorization: None,
                    billing_kind: CredentialBillingKind::ProviderManaged,
                    account_usage_query: AccountUsageQueryKind::Unsupported,
                    account_model_access: None,
                    authentication: None,
                    key_prefix: None,
                    credential_role: None,
                    default_usage_scopes: BTreeSet::from([
                        InferenceUsageScope::InteractiveAgent,
                        InferenceUsageScope::InternalBackground,
                        InferenceUsageScope::ExternalService,
                    ]),
                }],
                authentication: self.authentication.projection(),
                request_headers: self.request_headers,
                session_header,
                service_origin_override_policy: None,
                native_inference_bindings: Vec::new(),
                model_discovery: model_discovery.1,
                request_projector: crate::RequestProjectorKind::Direct,
                protocol_routes,
                default_usage_scopes: BTreeSet::from([
                    InferenceUsageScope::InteractiveAgent,
                    InferenceUsageScope::InternalBackground,
                    InferenceUsageScope::ExternalService,
                ]),
            }],
        })
    }

    /// Resolves one effective discovery declaration from provider and route layers.
    /// 从供应商层与路由层解析一项生效的发现声明。
    ///
    /// # Parameters
    /// 参数
    /// `mode` is the effective mode, `path` the effective same-origin path, and `protocol`
    /// the target protocol used for the protocol-standard discovery kind.
    /// `mode` 为生效模式，`path` 为生效同源路径，`protocol` 为目标协议（用于推导协议标准发现类型）。
    ///
    /// # Returns
    /// 返回值
    /// `(discovery path, discovery kind)` with the protocol-standard kind for manual mode.
    /// 返回 `(发现路径, 发现类型)`；手动模式下返回协议标准类型。
    fn resolve_discovery(
        &self,
        mode: CustomProviderModelDiscoveryMode,
        path: Option<&str>,
        protocol: Option<&CustomProviderProtocol>,
    ) -> Result<(Option<String>, ModelDiscoveryKind), String> {
        let protocol_discovery = match protocol {
            Some(CustomProviderProtocol::AnthropicMessages) => {
                ModelDiscoveryKind::AnthropicCompatible
            }
            Some(
                CustomProviderProtocol::OpenAiChat
                | CustomProviderProtocol::OpenAiResponses
                | CustomProviderProtocol::OpenAiCodexResponses,
            ) => ModelDiscoveryKind::OpenAi,
            Some(CustomProviderProtocol::GeminiGenerateContent) => ModelDiscoveryKind::Gemini,
            None => ModelDiscoveryKind::OpenAi,
        };
        match mode {
            CustomProviderModelDiscoveryMode::Standard => Ok((None, protocol_discovery)),
            CustomProviderModelDiscoveryMode::CustomPath => Ok((
                Some(validate_custom_model_discovery_path(path.ok_or_else(
                    || String::from("Custom model discovery path is required."),
                )?)?),
                protocol_discovery,
            )),
            CustomProviderModelDiscoveryMode::Manual => Ok((None, ModelDiscoveryKind::Manual)),
        }
    }
}

/// Validates and normalizes one user-declared compatible API base URL.
/// 校验并规范化一个用户声明的兼容 API 基础 URL。
///
/// # Parameters
/// 参数
/// `candidate` is the exact base URL entered by the user.
/// `candidate` 是用户输入的精确基础 URL。
///
/// # Returns
/// 返回值
/// A normalized HTTP(S) URL without credentials, query, fragment, or trailing slash.
/// 返回不含凭据、查询串、片段或末尾斜杠的规范化 HTTP(S) URL。
fn validate_custom_provider_base_url(candidate: &str) -> Result<String, String> {
    let candidate = candidate.trim();
    // parsed makes the authority boundary explicit before this value enters persistence or probing.
    // parsed 在该值进入持久化或探测前明确解析其权限边界。
    let parsed = reqwest::Url::parse(candidate)
        .map_err(|_| String::from("Custom provider base URL must be a valid HTTP or HTTPS URL."))?;
    let has_explicit_authority = candidate
        .split_once("://")
        .is_some_and(|(_, authority)| !authority.is_empty() && !authority.starts_with('/'));
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !has_explicit_authority
    {
        return Err(String::from(
            "Custom provider base URL must be a valid HTTP or HTTPS URL with a host.",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(String::from(
            "Custom provider base URL must not contain embedded credentials.",
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(String::from(
            "Custom provider base URL must not contain a query or fragment.",
        ));
    }
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

/// Header names that a user-owned provider may never fix, regardless of casing.
/// 用户自有供应商绝不能固定的请求头名称（不区分大小写）。
const FORBIDDEN_CUSTOM_PROVIDER_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "www-authenticate",
    "x-api-key",
    "api-key",
    "x-goog-api-key",
    "x-auth-token",
    "cookie",
    "set-cookie",
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
    "expect",
    "x-request-id",
    "x-correlation-id",
];

/// Maximum accepted bytes for one fixed non-sensitive header name.
/// 单个固定非敏感头名称的最大字节数。
const CUSTOM_PROVIDER_HEADER_NAME_MAX_BYTES: usize = 64;
/// Maximum accepted bytes for one fixed non-sensitive header value.
/// 单个固定非敏感头值的最大字节数。
const CUSTOM_PROVIDER_HEADER_VALUE_MAX_BYTES: usize = 256;

/// Validates one bounded fixed-header map for a user-owned provider.
/// 校验一个用户自有供应商的有界固定请求头映射。
///
/// # Parameters
/// 参数
/// `headers` is the exact map persisted with the provider or one of its routes.
/// `headers` 是与供应商或其路由一起持久化的精确映射。
///
/// # Returns
/// 返回值
/// Success only when every header name and value stays within the bounded non-sensitive contract.
/// 仅当每个头名称与值都处于有界非敏感契约内时返回成功。
pub(crate) fn validate_custom_provider_fixed_headers(
    headers: &BTreeMap<String, String>,
) -> Result<(), String> {
    for (name, value) in headers {
        if name.is_empty()
            || name.len() > CUSTOM_PROVIDER_HEADER_NAME_MAX_BYTES
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(String::from(
                "Custom provider fixed header names must be ASCII letters, digits, or hyphens within 64 bytes.",
            ));
        }
        if FORBIDDEN_CUSTOM_PROVIDER_HEADERS
            .iter()
            .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
            || name.to_ascii_lowercase().starts_with("x-vulcan-")
        {
            return Err(format!(
                "Custom provider fixed request headers must not include reserved or host-owned names, found `{name}`."
            ));
        }
        if value.is_empty()
            || value.len() > CUSTOM_PROVIDER_HEADER_VALUE_MAX_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(String::from(
                "Custom provider fixed header values must be non-empty, free of control characters, and within 256 bytes.",
            ));
        }
    }
    Ok(())
}

/// Validates one optional regular-conversation session-header name.
/// 校验一个可选的常规会话请求头名称。
fn validate_custom_provider_session_header(header: Option<&str>) -> Result<(), String> {
    let Some(header) = header else {
        return Ok(());
    };
    if !crate::provider_manifest::is_valid_session_header_name(header) {
        return Err(format!(
            "Custom provider session header name must use ASCII letters, digits, hyphens, underscores, or dots within {} bytes.",
            crate::provider_manifest::MAX_SESSION_HEADER_NAME_BYTES,
        ));
    }
    Ok(())
}

/// Validates one user-declared compatible endpoint path.
/// 校验一个用户声明的兼容端点路径。
///
/// # Parameters
/// 参数
/// `candidate` is the exact endpoint path entered for one protocol route.
/// `candidate` 是为一个协议路由输入的精确端点路径。
///
/// # Returns
/// 返回值
/// The trimmed path when it cannot change origin, traverse parents, or inject URL suffixes.
/// 当路径不会改变来源、遍历父级或注入 URL 后缀时返回去除空白后的路径。
fn validate_custom_provider_endpoint_path(candidate: &str) -> Result<String, String> {
    validate_custom_model_discovery_path(candidate).map_err(|_| {
        String::from(
            "Custom provider endpoint path must be a same-origin absolute path without parent traversal, query, or fragment.",
        )
    })
}

/// Revalidates one persisted custom-provider descriptor before runtime registration.
/// 在运行时注册前重新校验一个已持久化的自定义供应商描述。
///
/// # Parameters
/// 参数
/// `provider` is the complete descriptor loaded from persisted configuration.
/// `provider` 是从持久化配置加载的完整描述。
///
/// # Returns
/// 返回值
/// Success only when the descriptor remains inside the user-configurable custom-provider contract.
/// 仅当描述仍处于用户可配置的自定义供应商契约内时返回成功。
pub(crate) fn validate_persisted_custom_provider_descriptor(
    provider: &ProviderDescriptor,
) -> Result<(), String> {
    if provider.display_name.trim().is_empty() {
        return Err(String::from(
            "Persisted custom provider display name is required.",
        ));
    }
    if provider.service_entries.is_empty() {
        return Err(String::from(
            "Persisted custom provider must declare at least one conversation service entry.",
        ));
    }
    for entry in &provider.service_entries {
        validate_persisted_custom_provider_service_entry(entry)?;
    }
    Ok(())
}

/// Revalidates one persisted custom-provider service entry and every protocol route.
/// 重新校验一个已持久化的自定义供应商服务入口及其全部协议路由。
///
/// # Parameters
/// 参数
/// `entry` is one conversation entry owned by the custom provider.
/// `entry` 是该自定义供应商拥有的一个会话服务入口。
///
/// # Returns
/// 返回值
/// Success only when the entry and all routes stay inside the custom-provider contract.
/// 仅当入口与全部路由都处于自定义供应商契约内时返回成功。
fn validate_persisted_custom_provider_service_entry(
    entry: &crate::ServiceEntry,
) -> Result<(), String> {
    if entry.kind != crate::ServiceEntryKind::Conversation
        || entry.request_projector != crate::RequestProjectorKind::Direct
    {
        return Err(String::from(
            "Persisted custom provider must use the direct conversation projector.",
        ));
    }
    validate_custom_provider_base_url(entry.base_url.as_str())?;
    validate_custom_provider_fixed_headers(&entry.request_headers)?;
    validate_custom_provider_session_header(entry.session_header.as_deref())?;
    if !matches!(
        entry.authentication,
        AuthenticationProjection::BearerAuthorization
            | AuthenticationProjection::XApiKey
            | AuthenticationProjection::GoogleApiKey
    ) {
        return Err(String::from(
            "Persisted custom provider uses an unsupported authentication projection.",
        ));
    }
    let [profile] = entry.credential_profiles.as_slice() else {
        return Err(String::from(
            "Persisted custom provider must contain exactly one API-key credential profile.",
        ));
    };
    if profile.credential_mode != CredentialMode::Token
        || profile.authorization.is_some()
        || profile.billing_kind != CredentialBillingKind::ProviderManaged
        || profile.account_usage_query != AccountUsageQueryKind::Unsupported
        || profile.authentication.is_some()
        || profile.key_prefix.is_some()
    {
        return Err(String::from(
            "Persisted custom provider credential profile is outside the API-key contract.",
        ));
    }
    if entry.protocol_routes.is_empty() {
        return Err(String::from(
            "Persisted custom provider must declare at least one protocol route.",
        ));
    }
    let mut seen_routes = BTreeSet::new();
    for route in &entry.protocol_routes {
        validate_persisted_custom_provider_route(entry, route, &mut seen_routes)?;
    }
    Ok(())
}

/// Revalidates one persisted custom-provider protocol route.
/// 重新校验一条已持久化的自定义供应商协议路由。
///
/// # Parameters
/// 参数
/// `entry` owns the shared discovery contract, `route` is the exact route under validation, and
/// `seen_routes` rejects duplicate protocol declarations.
/// `entry` 持有共享发现契约，`route` 是接受校验的精确路由，`seen_routes` 拒绝重复协议声明。
///
/// # Returns
/// 返回值
/// Success only when the route stays inside the verified custom-provider contract.
/// 仅当路由处于已验证的自定义供应商契约内时返回成功。
fn validate_persisted_custom_provider_route(
    entry: &crate::ServiceEntry,
    route: &ProtocolRoute,
    seen_routes: &mut BTreeSet<String>,
) -> Result<(), String> {
    let protocol = match route.protocol_profile_id.as_str() {
        ANTHROPIC_MESSAGES_PROFILE_ID => CustomProviderProtocol::AnthropicMessages,
        OPENAI_CHAT_PROFILE_ID => CustomProviderProtocol::OpenAiChat,
        OPENAI_RESPONSES_PROFILE_ID => CustomProviderProtocol::OpenAiResponses,
        OPENAI_CODEX_RESPONSES_PROFILE_ID => CustomProviderProtocol::OpenAiCodexResponses,
        GEMINI_GENERATE_CONTENT_PROFILE_ID => CustomProviderProtocol::GeminiGenerateContent,
        _ => {
            return Err(String::from(
                "Persisted custom provider uses an unsupported protocol route.",
            ));
        }
    };
    if !seen_routes.insert(route.protocol_profile_id.clone()) {
        return Err(format!(
            "Persisted custom provider declares protocol route `{}` more than once.",
            route.protocol_profile_id
        ));
    }
    if let Some(endpoint_path) = route.endpoint_path.as_deref()
        && endpoint_path != protocol.endpoint_path().unwrap_or_default()
    {
        validate_custom_provider_endpoint_path(endpoint_path)?;
    }
    if let Some(base_url_override) = route.base_url_override.as_deref() {
        validate_custom_provider_base_url(base_url_override)?;
    }
    // Route-level authentication is now part of the custom-provider contract; only the
    // key-projection categories are valid for a user-managed API key, never the external
    // account-bound OAuth projections.
    // 路由级鉴权现属于自定义供应商契约的一部分；用户管理 API Key 只接受 Key 投影类别，
    // 绝不允许外部账号绑定的 OAuth 投影。
    if let Some(authentication) = route.authentication.as_ref()
        && !matches!(
            authentication,
            crate::AuthenticationProjection::XApiKey
                | crate::AuthenticationProjection::BearerAuthorization
                | crate::AuthenticationProjection::GoogleApiKey
        )
    {
        return Err(String::from(
            "Persisted custom provider route authentication must use an API-key projection.",
        ));
    }
    if route.model_discovery_base_url_override.is_some() {
        return Err(String::from(
            "Persisted custom provider route must not declare a discovery base URL override.",
        ));
    }
    validate_custom_provider_fixed_headers(&route.request_headers)?;
    if !route.verified {
        return Err(String::from(
            "Persisted custom provider protocol route must stay verified.",
        ));
    }
    if !matches!(protocol, CustomProviderProtocol::OpenAiCodexResponses)
        && route.responses_features.is_some()
    {
        return Err(String::from(
            "Persisted non-Codex custom provider must not declare Responses capabilities.",
        ));
    }
    let expected_discovery = match protocol {
        CustomProviderProtocol::AnthropicMessages => ModelDiscoveryKind::AnthropicCompatible,
        CustomProviderProtocol::OpenAiChat
        | CustomProviderProtocol::OpenAiResponses
        | CustomProviderProtocol::OpenAiCodexResponses => ModelDiscoveryKind::OpenAi,
        CustomProviderProtocol::GeminiGenerateContent => ModelDiscoveryKind::Gemini,
    };
    let route_discovery = route.model_discovery.ok_or_else(|| {
        String::from("Persisted custom provider protocol route must declare model discovery.")
    })?;
    if !matches!(route_discovery, ModelDiscoveryKind::Manual)
        && route_discovery != expected_discovery
    {
        return Err(String::from(
            "Persisted custom provider model discovery does not match its protocol.",
        ));
    }
    match route.model_discovery_path.as_deref() {
        Some(_) if route_discovery == ModelDiscoveryKind::Manual => {
            return Err(String::from(
                "Persisted manual custom provider must not declare a model-discovery path.",
            ));
        }
        Some(path) => {
            validate_custom_model_discovery_path(path)?;
        }
        None => {}
    }
    // Entry-level discovery must stay consistent with the route set: the shared legacy fallback
    // either is manual or appears among the routes actually declared by this entry.
    // 入口级发现必须与路由集合保持一致：共享旧版回退要么是手动，要么出现在该入口实际声明的路由中。
    let entry_consistent = entry.model_discovery == ModelDiscoveryKind::Manual
        || entry.protocol_routes.iter().any(|route| {
            route.model_discovery == Some(entry.model_discovery)
                || route.model_discovery == Some(ModelDiscoveryKind::Manual)
        });
    if !entry_consistent {
        return Err(String::from(
            "Persisted custom provider entry discovery is not declared by any protocol route.",
        ));
    }
    Ok(())
}

/// Validates one same-origin absolute model-discovery path.
/// 校验一个同源绝对模型发现路径。
///
/// # Parameters
/// 参数
/// `candidate` is the path entered for custom model discovery.
/// `candidate` 是为自定义模型发现输入的路径。
///
/// # Returns
/// 返回值
/// The trimmed path when it cannot change origin, traverse parents, or inject URL suffixes.
/// 当路径不会改变来源、遍历父级或注入 URL 后缀时返回去除空白后的路径。
fn validate_custom_model_discovery_path(candidate: &str) -> Result<String, String> {
    let path = candidate.trim();
    // decoded_segments checks both direct and encoded separators before the path reaches the URL client.
    // decoded_segments 在路径进入 URL 客户端前同时检查直接分隔符与编码分隔符。
    let Some(decoded_segments) = path
        .split('/')
        .map(percent_decode_path_segment)
        .collect::<Option<Vec<_>>>()
    else {
        return Err(String::from(
            "Custom model discovery path must contain only valid URL percent escapes.",
        ));
    };
    let has_parent_traversal = decoded_segments.iter().any(|segment| {
        segment
            .split(['/', '\\'])
            .any(|component| component == "..")
    });
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains("://")
        || path.contains(['?', '#', '\\'].as_slice())
        || path.chars().any(char::is_control)
        || has_parent_traversal
    {
        return Err(String::from(
            "Custom model discovery path must be a same-origin absolute path without parent traversal, query, or fragment.",
        ));
    }
    reqwest::Url::parse(format!("https://validation.invalid{path}").as_str())
        .map_err(|_| String::from("Custom model discovery path must be a valid URL path."))?;
    Ok(path.to_string())
}

/// Percent-decodes one path segment only when every escape is valid UTF-8 data.
/// 仅当每个转义均为有效 UTF-8 数据时，对一个路径片段执行百分号解码。
///
/// # Parameters
/// 参数
/// `segment` is one slash-delimited URL path segment.
/// `segment` 是一个由斜杠分隔的 URL 路径片段。
///
/// # Returns
/// 返回值
/// The decoded segment, or `None` when an escape sequence or UTF-8 payload is invalid.
/// 返回解码后的片段；转义序列或 UTF-8 载荷无效时返回 `None`。
fn percent_decode_path_segment(segment: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(segment.len());
    let source = segment.as_bytes();
    let mut index = 0;
    while index < source.len() {
        if source[index] == b'%' {
            let high = source.get(index + 1).and_then(|value| hex_value(*value))?;
            let low = source.get(index + 2).and_then(|value| hex_value(*value))?;
            bytes.push((high << 4) | low);
            index += 3;
        } else {
            bytes.push(source[index]);
            index += 1;
        }
    }
    String::from_utf8(bytes).ok()
}

/// Converts one ASCII hexadecimal digit into its numeric value.
/// 将一个 ASCII 十六进制数字转换为数值。
///
/// # Parameters
/// 参数
/// `value` is one raw ASCII byte.
/// `value` 是一个原始 ASCII 字节。
///
/// # Returns
/// 返回值
/// The hexadecimal value, or `None` for non-hexadecimal input.
/// 返回十六进制值；输入不是十六进制数字时返回 `None`。
fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies one custom definition projects exactly one selected protocol.
    /// 验证一个自定义定义只投影一个选定协议。
    #[test]
    fn custom_provider_projects_selected_protocols() {
        let descriptor = CustomProviderDefinition {
            id: String::from("custom:example"),
            display_name: String::from("Example"),
            base_url: String::from("https://example.com/v1/"),
            protocols: vec![CustomProviderProtocol::OpenAiResponses],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect("custom provider should be valid");
        let entry = &descriptor.service_entries[0];
        assert_eq!(entry.base_url, "https://example.com/v1");
        assert_eq!(entry.protocol_routes.len(), 1);
        assert_eq!(descriptor.kind, ProviderKind::Custom);
    }

    /// Verifies a provider-level session header is trimmed and projected to the conversation entry.
    /// 验证供应商级 Session Header 会去除首尾空白并投影到会话入口。
    #[test]
    fn custom_provider_projects_regular_conversation_session_header() {
        let descriptor = CustomProviderDefinition {
            id: String::from("custom:session-example"),
            display_name: String::from("Session example"),
            base_url: String::from("https://example.com/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiChat],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: Some(String::from("  x-custom-session  ")),
        }
        .into_descriptor()
        .expect("custom provider session header should be valid");

        assert_eq!(
            descriptor.service_entries[0].session_header.as_deref(),
            Some("x-custom-session")
        );
        validate_persisted_custom_provider_descriptor(&descriptor)
            .expect("custom provider session header should remain valid after projection");
    }

    /// Verifies a custom Gemini provider owns native discovery, authentication, and dynamic model paths.
    /// 验证自定义 Gemini 供应商持有原生发现、鉴权与动态模型路径。
    #[test]
    fn custom_provider_projects_gemini_generate_content() {
        // descriptor is the complete runtime contract produced from one browser-editable definition.
        // descriptor 是从浏览器可编辑定义生成的完整运行时契约。
        let descriptor = CustomProviderDefinition {
            id: String::from("custom:gemini-relay"),
            display_name: String::from("Gemini Relay"),
            base_url: String::from("https://example.com/v1beta"),
            protocols: vec![CustomProviderProtocol::GeminiGenerateContent],
            authentication: CustomProviderAuthentication::GoogleApiKey,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect("custom Gemini provider should be valid");
        // entry is the sole exact service route accepted by the custom-provider boundary.
        // entry 是自定义供应商边界接受的唯一精确服务路由。
        let entry = &descriptor.service_entries[0];
        // route carries the dynamic Gemini protocol without a misleading fixed endpoint path.
        // route 承载动态 Gemini 协议，不声明误导性的固定端点路径。
        let route = &entry.protocol_routes[0];

        assert_eq!(entry.authentication, AuthenticationProjection::GoogleApiKey);
        assert_eq!(entry.model_discovery, ModelDiscoveryKind::Gemini);
        assert_eq!(
            route.protocol_profile_id,
            GEMINI_GENERATE_CONTENT_PROFILE_ID
        );
        assert_eq!(route.endpoint_path, None);
        validate_persisted_custom_provider_descriptor(&descriptor)
            .expect("persisted Gemini descriptor should revalidate");
    }

    /// Verifies Codex custom providers carry explicit optional capabilities with safe defaults.
    /// 验证 Codex 自定义供应商携带显式可选能力，并使用安全默认值。
    #[test]
    fn custom_codex_provider_projects_disabled_optional_capabilities() {
        let descriptor = CustomProviderDefinition {
            id: String::from("custom:codex-example"),
            display_name: String::from("Codex example"),
            base_url: String::from("https://example.com/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiCodexResponses],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect("Codex custom provider should be valid");
        let route = &descriptor.service_entries[0].protocol_routes[0];
        let features = route
            .responses_features
            .expect("Codex route should declare optional capabilities");
        assert_eq!(
            features.remote_compaction,
            crate::ResponsesRemoteCompactionPolicy::Disabled
        );
        validate_persisted_custom_provider_descriptor(&descriptor)
            .expect("Codex descriptor with disabled capabilities should persist");

        let mut legacy_descriptor = descriptor;
        legacy_descriptor.service_entries[0].protocol_routes[0].responses_features = None;
        validate_persisted_custom_provider_descriptor(&legacy_descriptor)
            .expect("legacy Codex descriptor without capability fields should default disabled");
    }

    /// Verifies custom provider identities cannot escape their bounded stable namespace.
    /// 验证自定义供应商身份不能逃逸其有界稳定命名空间。
    #[test]
    fn custom_provider_identifier_validation_is_exact_and_bounded() {
        validate_custom_provider_identifier("custom:stable.Provider-1")
            .expect("stable custom identity should pass");
        for invalid in [
            "openai",
            "custom:",
            "custom:path/escape",
            "custom:has space",
        ] {
            validate_custom_provider_identifier(invalid)
                .expect_err("invalid custom identity must fail closed");
        }
        let boundary = format!(
            "custom:{}",
            "a".repeat(MAX_PROVIDER_IDENTIFIER_LEN - "custom:".len())
        );
        validate_custom_provider_identifier(boundary.as_str())
            .expect("filesystem-safe boundary identity should pass");
        let oversized = format!("{boundary}a");
        let error = validate_custom_provider_identifier(oversized.as_str())
            .expect_err("oversized custom identity must fail closed");
        assert!(error.contains("exceed"), "{error}");
    }

    /// Verifies an invalid transport URL is rejected before persistence.
    /// 验证无效传输 URL 会在持久化前被拒绝。
    #[test]
    fn custom_provider_rejects_invalid_base_url() {
        let error = CustomProviderDefinition {
            id: String::from("custom:example"),
            display_name: String::from("Example"),
            base_url: String::from("example.com/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiChat],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect_err("invalid custom provider should fail");
        assert!(error.contains("HTTP"));
    }

    /// Verifies transport URLs cannot smuggle credentials or URL suffixes into persistence.
    /// 验证传输 URL 不能把凭据或 URL 后缀夹带进持久化。
    #[test]
    fn custom_provider_rejects_unsafe_base_url_components() {
        for (base_url, expected) in [
            ("https://user:secret@example.com/v1", "credentials"),
            ("https://example.com/v1?token=secret", "query"),
            ("https://example.com/v1#private", "query"),
            ("http:///v1", "valid HTTP"),
        ] {
            let error = CustomProviderDefinition {
                id: String::from("custom:unsafe-example"),
                display_name: String::from("Unsafe example"),
                base_url: String::from(base_url),
                protocols: vec![CustomProviderProtocol::OpenAiChat],
                authentication: CustomProviderAuthentication::BearerAuthorization,
                model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
                model_discovery_path: None,
                protocol_routes: Vec::new(),
                request_headers: BTreeMap::new(),
                session_header: None,
            }
            .into_descriptor()
            .expect_err("unsafe custom provider URL must fail");
            assert!(error.contains(expected), "unexpected error: {error}");
            assert!(!error.contains("secret"));
        }
    }

    /// Verifies a custom model directory remains a same-origin absolute path.
    /// 验证自定义模型目录会保持为同源绝对路径。
    #[test]
    fn custom_provider_accepts_only_safe_model_discovery_paths() {
        let descriptor = CustomProviderDefinition {
            id: String::from("custom:path-example"),
            display_name: String::from("Path example"),
            base_url: String::from("https://example.com/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiChat],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::CustomPath,
            model_discovery_path: Some(String::from("/catalog/models")),
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect("same-origin model path should be valid");
        assert_eq!(
            descriptor.service_entries[0].protocol_routes[0]
                .model_discovery_path
                .as_deref(),
            Some("/catalog/models")
        );

        let error = CustomProviderDefinition {
            id: String::from("custom:path-example"),
            display_name: String::from("Path example"),
            base_url: String::from("https://example.com/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiChat],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::CustomPath,
            model_discovery_path: Some(String::from("/../models")),
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect_err("parent traversal must be rejected");
        assert!(error.contains("same-origin absolute path"));

        for unsafe_path in [
            "//other.example/models",
            "/%2e%2e/models",
            "/catalog/models?token=secret",
            "/catalog/models#private",
            "/catalog\\models",
            "/catalog/\nmodels",
        ] {
            let error = CustomProviderDefinition {
                id: String::from("custom:path-example"),
                display_name: String::from("Path example"),
                base_url: String::from("https://example.com/v1"),
                protocols: vec![CustomProviderProtocol::OpenAiChat],
                authentication: CustomProviderAuthentication::BearerAuthorization,
                model_discovery_mode: CustomProviderModelDiscoveryMode::CustomPath,
                model_discovery_path: Some(String::from(unsafe_path)),
                protocol_routes: Vec::new(),
                request_headers: BTreeMap::new(),
                session_header: None,
            }
            .into_descriptor()
            .expect_err("unsafe model-discovery path must fail");
            assert!(error.contains("same-origin absolute path"));
            assert!(!error.contains("secret"));
            assert!(!error.contains("private"));
        }
    }

    /// Verifies persisted descriptors cannot bypass creation-time route and URL validation.
    /// 验证持久化描述不能绕过创建时的路由与 URL 校验。
    #[test]
    fn persisted_custom_provider_revalidates_security_boundaries() {
        let descriptor = CustomProviderDefinition {
            id: String::from("custom:persisted-example"),
            display_name: String::from("Persisted example"),
            base_url: String::from("https://example.com/v1"),
            protocols: vec![CustomProviderProtocol::OpenAiChat],
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::CustomPath,
            model_discovery_path: Some(String::from("/catalog/models")),
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
            session_header: None,
        }
        .into_descriptor()
        .expect("fixture custom provider should be valid");
        validate_persisted_custom_provider_descriptor(&descriptor)
            .expect("unchanged persisted descriptor should remain valid");

        let mut unsafe_url = descriptor.clone();
        unsafe_url.service_entries[0].base_url = String::from("https://user:secret@example.com/v1");
        let error = validate_persisted_custom_provider_descriptor(&unsafe_url)
            .expect_err("embedded credentials must fail after reload");
        assert!(error.contains("credentials"));
        assert!(!error.contains("secret"));

        let mut unsafe_header = descriptor.clone();
        unsafe_header.service_entries[0]
            .request_headers
            .insert(String::from("authorization"), String::from("private"));
        let error = validate_persisted_custom_provider_descriptor(&unsafe_header)
            .expect_err("persisted request-header injection must fail");
        assert!(error.contains("request headers"));
        assert!(!error.contains("private"));

        let mut cross_origin_route = descriptor;
        cross_origin_route.service_entries[0].protocol_routes[0]
            .model_discovery_base_url_override = Some(String::from("https://other.example"));
        let error = validate_persisted_custom_provider_descriptor(&cross_origin_route)
            .expect_err("persisted cross-origin discovery must fail");
        assert!(error.contains("discovery base URL override"));
        assert!(!error.contains("other.example"));
    }
}
