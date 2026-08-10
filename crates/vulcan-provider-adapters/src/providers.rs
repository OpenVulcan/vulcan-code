//! Explicit legacy provider descriptors that cannot yet move to manifest authority.
//! 尚不能迁移到 Manifest 权威的显式旧版供应商描述。

use crate::{
    AccountUsageQueryKind, AuthenticationProjection, CredentialBillingKind,
    CredentialDirectoryVisibility, CredentialMode, CredentialProfile, CredentialRegistrationKind,
    ModelDiscoveryKind, ProtocolRoute, ProviderCategory, ProviderDescriptor, ProviderKind,
    ProviderPartnershipKind, ProviderPromotion, ProviderPromotionDisclosureKey,
    RequestProjectorKind, ServiceEntry, ServiceEntryKind,
};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::InferenceUsageScope;
use vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID;

/// Creates the temporary APIKEY.FUN legacy descriptor retained until its authoritative
/// model-to-protocol mapping is supplied.
/// 创建临时保留的 APIKEY.FUN 旧版描述，直至其权威模型到协议映射被提供。
///
/// # Returns
/// 返回值
/// The exact relay descriptor used only by the explicit legacy-fallback allowlist.
/// 仅由显式旧版回退允许名单使用的精确中转供应商描述。
pub fn apikeyfun_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        id: String::from("apikeyfun"),
        display_name: String::from("APIKEY.FUN"),
        kind: ProviderKind::System,
        category: ProviderCategory::Relay,
        credential_directory_visibility: CredentialDirectoryVisibility::PinnedOnboarding,
        credential_registration: CredentialRegistrationKind::Direct,
        promotion: Some(apikeyfun_referral_promotion()),
        service_entries: vec![
            apikeyfun_service_entry("public-api", "Default API", "https://api.apikey.fun/v1"),
            apikeyfun_service_entry(
                "slb-api",
                "SLB API (Image Generation Disabled)",
                "https://slb.apikey.fun/v1",
            ),
        ],
    }
}

/// Creates one APIKEY.FUN OpenAI-compatible service entry.
/// 创建一条 APIKEY.FUN OpenAI 兼容服务入口。
///
/// # Parameters
/// 参数
/// `id`, `display_name`, and `base_url` are the exact legacy entry identity, user-facing name,
/// and HTTPS origin.
/// `id`、`display_name` 与 `base_url` 分别是精确旧版入口身份、用户可见名称及 HTTPS 源。
///
/// # Returns
/// 返回值
/// A conversation entry whose protocol remains the existing OpenAI Chat route without guessing
/// any model-specific mapping.
/// 返回保留现有 OpenAI Chat 路由且不猜测任何模型专属映射的会话入口。
fn apikeyfun_service_entry(id: &str, display_name: &str, base_url: &str) -> ServiceEntry {
    ServiceEntry {
        id: id.to_string(),
        display_name: display_name.to_string(),
        kind: ServiceEntryKind::Conversation,
        base_url: base_url.to_string(),
        credential_profiles: vec![apikeyfun_credential_profile()],
        authentication: AuthenticationProjection::BearerAuthorization,
        request_headers: BTreeMap::new(),
        service_origin_override_policy: None,
        native_inference_bindings: Vec::new(),
        model_discovery: ModelDiscoveryKind::OpenAi,
        request_projector: RequestProjectorKind::Direct,
        protocol_routes: vec![apikeyfun_protocol_route()],
        allowed_usage_scopes: all_legacy_usage_scopes(),
    }
}

/// Creates the APIKEY.FUN API-key credential profile.
/// 创建 APIKEY.FUN API Key 凭据配置档。
///
/// # Returns
/// 返回值
/// A provider-managed bearer-token profile with no unsupported account-usage query.
/// 返回由供应商管理、使用 Bearer Token 且不声明不受支持用量查询的配置档。
fn apikeyfun_credential_profile() -> CredentialProfile {
    CredentialProfile {
        id: String::from("api-key"),
        display_name: String::from("API Key"),
        credential_mode: CredentialMode::Token,
        oauth_provider: None,
        billing_kind: CredentialBillingKind::ProviderManaged,
        account_usage_query: AccountUsageQueryKind::Unsupported,
        authentication: None,
        key_prefix: None,
        credential_role: None,
        allowed_usage_scopes: all_legacy_usage_scopes(),
    }
}

/// Creates the existing APIKEY.FUN OpenAI Chat protocol route.
/// 创建现有 APIKEY.FUN OpenAI Chat 协议路由。
///
/// # Returns
/// 返回值
/// A verified chat-completions route retained as legacy authority until exact model facts arrive.
/// 返回在精确模型事实到齐前作为旧版权威保留的已验证 Chat Completions 路由。
fn apikeyfun_protocol_route() -> ProtocolRoute {
    ProtocolRoute {
        protocol_profile_id: String::from(OPENAI_CHAT_PROFILE_ID),
        endpoint_path: Some(String::from("/chat/completions")),
        base_url_override: None,
        request_headers: BTreeMap::new(),
        authentication: None,
        model_discovery: None,
        model_discovery_path: None,
        model_discovery_base_url_override: None,
        responses_features: None,
        input_resource_constraints: Vec::new(),
        usage_extraction_profiles: Vec::new(),
        request_projection_profiles: Vec::new(),
        response_projection_profiles: Vec::new(),
        provider_request_id_headers: Vec::new(),
        verified: true,
    }
}

/// Creates the disclosed APIKEY.FUN referral metadata.
/// 创建已披露的 APIKEY.FUN 邀请推广元数据。
///
/// # Returns
/// 返回值
/// The exact public signup, referral, campaign, relationship, and disclosure facts.
/// 返回精确公开的注册、邀请、活动、合作关系及披露事实。
fn apikeyfun_referral_promotion() -> ProviderPromotion {
    ProviderPromotion {
        signup_url: String::from("https://apikey.fun/register"),
        referral_url: Some(String::from("https://apikey.fun/register?aff=LY4DT3DCTM9N")),
        campaign_id: Some(String::from("LY4DT3DCTM9N")),
        partnership_kind: ProviderPartnershipKind::Referral,
        disclosure_key: ProviderPromotionDisclosureKey::ReferralRevenue,
    }
}

/// Returns the complete usage-scope set historically granted to the legacy relay entry.
/// 返回历史上授予旧版中转入口的完整用量作用域集合。
///
/// # Returns
/// 返回值
/// Interactive, internal-background, and external-service inference scopes.
/// 返回交互、内部后台及外部服务推理作用域。
fn all_legacy_usage_scopes() -> BTreeSet<InferenceUsageScope> {
    [
        InferenceUsageScope::InteractiveAgent,
        InferenceUsageScope::InternalBackground,
        InferenceUsageScope::ExternalService,
    ]
    .into_iter()
    .collect()
}
