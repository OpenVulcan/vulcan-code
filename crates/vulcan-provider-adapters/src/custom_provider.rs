//! User-defined OpenAI and Anthropic compatible provider contracts.
//! 用户自定义 OpenAI 与 Anthropic 兼容供应商契约。

use crate::{
    AccountUsageQueryKind, AuthenticationProjection, CredentialBillingKind, CredentialMode,
    CredentialProfile, ModelDiscoveryKind, ProtocolRoute, ProviderDescriptor, ProviderKind,
    ServiceEntry,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
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
}

impl CustomProviderAuthentication {
    /// Projects the public custom setting into the runtime authentication contract.
    /// 将公开自定义设置投影为运行时鉴权契约。
    fn projection(self) -> AuthenticationProjection {
        match self {
            Self::BearerAuthorization => AuthenticationProjection::BearerAuthorization,
            Self::XApiKey => AuthenticationProjection::XApiKey,
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
        }
    }

    /// Returns the provider endpoint path used by this protocol.
    /// 返回此协议使用的供应商端点路径。
    fn endpoint_path(self) -> &'static str {
        match self {
            Self::AnthropicMessages => "/messages",
            Self::OpenAiChat => "/chat/completions",
            Self::OpenAiResponses => "/responses",
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
        let base_url = self.base_url.trim().trim_end_matches('/').to_string();
        if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
            return Err(String::from(
                "Custom provider base URL must use HTTP or HTTPS.",
            ));
        }
        let model_discovery = match self.protocol {
            CustomProviderProtocol::AnthropicMessages => ModelDiscoveryKind::AnthropicCompatible,
            CustomProviderProtocol::OpenAiChat | CustomProviderProtocol::OpenAiResponses => {
                ModelDiscoveryKind::OpenAi
            }
        };
        let protocol_routes = vec![ProtocolRoute {
            protocol_profile_id: self.protocol.profile_id().to_string(),
            endpoint_path: Some(self.protocol.endpoint_path().to_string()),
            authentication: None,
            model_discovery: Some(model_discovery),
            verified: true,
        }];
        Ok(ProviderDescriptor {
            id,
            display_name,
            kind: ProviderKind::Custom,
            service_entries: vec![ServiceEntry {
                id: String::from("default"),
                display_name: String::from("Compatible API"),
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
                }],
                authentication: self.authentication.projection(),
                request_headers: BTreeMap::new(),
                model_discovery,
                protocol_routes,
            }],
        })
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
        }
        .into_descriptor()
        .expect("custom provider should be valid");
        let entry = &descriptor.service_entries[0];
        assert_eq!(entry.base_url, "https://example.com/v1");
        assert_eq!(entry.protocol_routes.len(), 1);
        assert_eq!(descriptor.kind, ProviderKind::Custom);
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
        }
        .into_descriptor()
        .expect_err("invalid custom provider should fail");
        assert!(error.contains("HTTP"));
    }
}
