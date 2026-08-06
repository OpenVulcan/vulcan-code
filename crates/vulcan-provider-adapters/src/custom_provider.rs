//! User-defined OpenAI, Anthropic, and Gemini compatible provider contracts.
//! 用户自定义 OpenAI、Anthropic 与 Gemini 兼容供应商契约。

use crate::{
    AccountUsageQueryKind, AuthenticationProjection, CredentialBillingKind,
    CredentialDirectoryVisibility, CredentialMode, CredentialProfile, ModelDiscoveryKind,
    ProtocolRoute, ProviderCategory, ProviderDescriptor, ProviderKind, ResponsesRouteFeatures,
    ServiceEntry,
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
    /// Exact API base URL.
    /// 精确 API 基础 URL。
    pub base_url: String,
    /// The single verified compatible protocol served by this base URL.
    /// 此基础 URL 提供的唯一已验证兼容协议。
    pub protocol: CustomProviderProtocol,
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
}

impl CustomProviderDefinition {
    /// Validates and projects this definition into the runtime provider descriptor.
    /// 校验此定义并投影为运行时供应商描述。
    ///
    /// # Returns
    /// 返回值
    /// A strict descriptor accepted by the effective provider registry.
    /// 返回有效供应商注册表接受的严格描述。
    pub fn into_descriptor(self) -> Result<ProviderDescriptor, String> {
        let id = self.id.trim().to_string();
        if !id.starts_with("custom:") || id.len() == "custom:".len() {
            return Err(String::from(
                "Custom provider identifiers must use a non-empty `custom:` prefix.",
            ));
        }
        let display_name = self.display_name.trim().to_string();
        if display_name.is_empty() {
            return Err(String::from("Custom provider display name is required."));
        }
        let base_url = validate_custom_provider_base_url(self.base_url.as_str())?;
        let protocol_discovery = match self.protocol {
            CustomProviderProtocol::AnthropicMessages => ModelDiscoveryKind::AnthropicCompatible,
            CustomProviderProtocol::OpenAiChat
            | CustomProviderProtocol::OpenAiResponses
            | CustomProviderProtocol::OpenAiCodexResponses => ModelDiscoveryKind::OpenAi,
            CustomProviderProtocol::GeminiGenerateContent => ModelDiscoveryKind::Gemini,
        };
        let model_discovery = match self.model_discovery_mode {
            CustomProviderModelDiscoveryMode::Standard
            | CustomProviderModelDiscoveryMode::CustomPath => protocol_discovery,
            CustomProviderModelDiscoveryMode::Manual => ModelDiscoveryKind::Manual,
        };
        let model_discovery_path = match self.model_discovery_mode {
            CustomProviderModelDiscoveryMode::CustomPath => {
                let path = self
                    .model_discovery_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|path| !path.is_empty())
                    .ok_or_else(|| String::from("Custom model discovery path is required."))?;
                Some(validate_custom_model_discovery_path(path)?)
            }
            CustomProviderModelDiscoveryMode::Standard
            | CustomProviderModelDiscoveryMode::Manual => None,
        };
        let protocol_routes = vec![ProtocolRoute {
            protocol_profile_id: self.protocol.profile_id().to_string(),
            endpoint_path: self.protocol.endpoint_path().map(str::to_string),
            base_url_override: None,
            request_headers: BTreeMap::new(),
            authentication: None,
            model_discovery: Some(model_discovery),
            model_discovery_path,
            model_discovery_base_url_override: None,
            responses_features: (self.protocol == CustomProviderProtocol::OpenAiCodexResponses)
                .then_some(ResponsesRouteFeatures::default()),
            usage_extraction_profiles: Vec::new(),
            provider_request_id_headers: Vec::new(),
            verified: true,
        }];
        Ok(ProviderDescriptor {
            id,
            display_name,
            kind: ProviderKind::Custom,
            category: ProviderCategory::Unclassified,
            credential_directory_visibility: CredentialDirectoryVisibility::ConfiguredOnly,
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
                    oauth_provider: None,
                    billing_kind: CredentialBillingKind::ProviderManaged,
                    account_usage_query: AccountUsageQueryKind::Unsupported,
                    authentication: None,
                    key_prefix: None,
                    allowed_usage_scopes: BTreeSet::from([
                        InferenceUsageScope::InteractiveAgent,
                        InferenceUsageScope::InternalBackground,
                        InferenceUsageScope::ExternalService,
                    ]),
                }],
                authentication: self.authentication.projection(),
                request_headers: BTreeMap::new(),
                model_discovery,
                request_projector: crate::RequestProjectorKind::Direct,
                protocol_routes,
                allowed_usage_scopes: BTreeSet::from([
                    InferenceUsageScope::InteractiveAgent,
                    InferenceUsageScope::InternalBackground,
                    InferenceUsageScope::ExternalService,
                ]),
            }],
        })
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
    let [entry] = provider.service_entries.as_slice() else {
        return Err(String::from(
            "Persisted custom provider must contain exactly one conversation service entry.",
        ));
    };
    if entry.kind != crate::ServiceEntryKind::Conversation
        || entry.request_projector != crate::RequestProjectorKind::Direct
    {
        return Err(String::from(
            "Persisted custom provider must use the direct conversation projector.",
        ));
    }
    validate_custom_provider_base_url(entry.base_url.as_str())?;
    if !entry.request_headers.is_empty() {
        return Err(String::from(
            "Persisted custom provider must not inject service-entry request headers.",
        ));
    }
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
        || profile.oauth_provider.is_some()
        || profile.billing_kind != CredentialBillingKind::ProviderManaged
        || profile.account_usage_query != AccountUsageQueryKind::Unsupported
        || profile.authentication.is_some()
        || profile.key_prefix.is_some()
    {
        return Err(String::from(
            "Persisted custom provider credential profile is outside the API-key contract.",
        ));
    }
    let [route] = entry.protocol_routes.as_slice() else {
        return Err(String::from(
            "Persisted custom provider must contain exactly one protocol route.",
        ));
    };
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
    if route.endpoint_path.as_deref() != protocol.endpoint_path()
        || route.base_url_override.is_some()
        || !route.request_headers.is_empty()
        || route.authentication.is_some()
        || route.model_discovery_base_url_override.is_some()
        || !route.verified
    {
        return Err(String::from(
            "Persisted custom provider protocol route is outside the verified single-route contract.",
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
    if route_discovery != entry.model_discovery
        || !matches!(route_discovery, ModelDiscoveryKind::Manual)
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
            protocol: CustomProviderProtocol::OpenAiResponses,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
        }
        .into_descriptor()
        .expect("custom provider should be valid");
        let entry = &descriptor.service_entries[0];
        assert_eq!(entry.base_url, "https://example.com/v1");
        assert_eq!(entry.protocol_routes.len(), 1);
        assert_eq!(descriptor.kind, ProviderKind::Custom);
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
            protocol: CustomProviderProtocol::GeminiGenerateContent,
            authentication: CustomProviderAuthentication::GoogleApiKey,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
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
            protocol: CustomProviderProtocol::OpenAiCodexResponses,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
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

    /// Verifies an invalid transport URL is rejected before persistence.
    /// 验证无效传输 URL 会在持久化前被拒绝。
    #[test]
    fn custom_provider_rejects_invalid_base_url() {
        let error = CustomProviderDefinition {
            id: String::from("custom:example"),
            display_name: String::from("Example"),
            base_url: String::from("example.com/v1"),
            protocol: CustomProviderProtocol::OpenAiChat,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
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
                protocol: CustomProviderProtocol::OpenAiChat,
                authentication: CustomProviderAuthentication::BearerAuthorization,
                model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
                model_discovery_path: None,
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
            protocol: CustomProviderProtocol::OpenAiChat,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::CustomPath,
            model_discovery_path: Some(String::from("/catalog/models")),
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
            protocol: CustomProviderProtocol::OpenAiChat,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::CustomPath,
            model_discovery_path: Some(String::from("/../models")),
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
                protocol: CustomProviderProtocol::OpenAiChat,
                authentication: CustomProviderAuthentication::BearerAuthorization,
                model_discovery_mode: CustomProviderModelDiscoveryMode::CustomPath,
                model_discovery_path: Some(String::from(unsafe_path)),
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
            protocol: CustomProviderProtocol::OpenAiChat,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::CustomPath,
            model_discovery_path: Some(String::from("/catalog/models")),
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
        assert!(error.contains("single-route contract"));
        assert!(!error.contains("other.example"));
    }
}
