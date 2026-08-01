//! Strict provider adapter registry.
//! 严格供应商适配器注册表。

use crate::{RequestProjectorKind, ServiceEntry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Identifies whether a provider definition is code-owned or user-owned.
/// 标识供应商定义由代码持有还是由用户持有。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Immutable provider implemented by Vulcan Code.
    /// 由 Vulcan Code 实现的不可变供应商。
    System,
    /// Editable compatibility provider created by the user.
    /// 由用户创建且可编辑的兼容供应商。
    Custom,
}

/// Identifies the provider's user-facing business relationship.
/// 标识供应商面向用户的业务关系类别。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCategory {
    /// The relationship has not been classified, including user-defined providers.
    /// 尚未分类的关系，包括用户自定义供应商。
    #[default]
    Unclassified,
    /// An official model-provider endpoint.
    /// 模型厂商官方入口。
    Official,
    /// An official cloud or MaaS service.
    /// 官方云服务或 MaaS 服务。
    #[serde(rename = "cloud_maas")]
    CloudMaaS,
    /// A third-party relay or aggregation service.
    /// 第三方中转或聚合服务。
    Relay,
}

/// Controls whether an unconfigured provider remains visible in credential management.
/// 控制未配置供应商是否仍显示在凭据管理中。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialDirectoryVisibility {
    /// Shows the provider only after at least one credential exists.
    /// 仅在至少存在一个凭据后显示供应商。
    #[default]
    ConfiguredOnly,
    /// Keeps a provider-owned onboarding entry visible without creating a fake credential.
    /// 在不创建虚假凭据的情况下固定显示供应商引导入口。
    PinnedOnboarding,
}

/// Identifies the commercial relationship disclosed beside one registration link.
/// 标识在注册链接旁披露的商业合作关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderPartnershipKind {
    /// A normal registration link without revenue attribution.
    /// 不包含收益归因的普通注册链接。
    None,
    /// A referral link that may generate revenue for Vulcan.
    /// 可能为 Vulcan 带来收益的推广链接。
    Referral,
    /// A provider-sponsored relationship.
    /// 供应商赞助合作关系。
    Sponsored,
}

/// Identifies the exact localized disclosure rendered beside a provider promotion.
/// 标识供应商推广入口旁展示的精确本地化披露文案。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderPromotionDisclosureKey {
    /// Discloses a neutral registration link without Vulcan revenue attribution.
    /// 披露不包含 Vulcan 收益归因的普通注册链接。
    #[serde(rename = "settings.providerPromotion.neutralDisclosure")]
    NeutralRegistration,
    /// Discloses a Vulcan-owned referral relationship.
    /// 披露由 Vulcan 持有的推广返利关系。
    #[serde(rename = "settings.providerPromotion.referralDisclosure")]
    ReferralRevenue,
    /// Discloses a provider-sponsored relationship.
    /// 披露供应商赞助合作关系。
    #[serde(rename = "settings.providerPromotion.sponsoredDisclosure")]
    SponsoredRelationship,
}

/// Describes public registration and referral metadata for one provider.
/// 描述一个供应商的公开注册与推广元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPromotion {
    /// Neutral HTTPS registration or product URL.
    /// 不带归因的 HTTPS 注册或产品入口。
    pub signup_url: String,
    /// Optional Vulcan-owned HTTPS referral URL.
    /// 可选的 Vulcan 自有 HTTPS 推广链接。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referral_url: Option<String>,
    /// Optional public campaign identifier.
    /// 可选的公开推广活动标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub campaign_id: Option<String>,
    /// Commercial relationship disclosed to the user.
    /// 向用户披露的商业合作关系。
    pub partnership_kind: ProviderPartnershipKind,
    /// Stable localization key for the user-facing relationship disclosure.
    /// 面向用户的合作关系披露稳定本地化键。
    pub disclosure_key: ProviderPromotionDisclosureKey,
}

/// Describes one registered provider adapter.
/// 描述一个已注册供应商适配器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub id: String,
    /// User-visible provider name.
    /// 用户可见供应商名称。
    pub display_name: String,
    /// Definition ownership and editability kind.
    /// 定义所有权与可编辑性类别。
    pub kind: ProviderKind,
    /// User-facing provider business category.
    /// 面向用户的供应商业务类别。
    #[serde(default)]
    pub category: ProviderCategory,
    /// Visibility policy for the credential directory.
    /// 凭据目录可见性策略。
    #[serde(default)]
    pub credential_directory_visibility: CredentialDirectoryVisibility,
    /// Optional public registration and referral metadata.
    /// 可选的公开注册与推广元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion: Option<ProviderPromotion>,
    /// Registered service entries.
    /// 已注册服务入口。
    pub service_entries: Vec<ServiceEntry>,
}

/// Stores strict provider registrations.
/// 保存严格供应商注册项。
#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    /// Registered providers keyed by stable identifier.
    /// 按稳定标识索引的已注册供应商。
    providers: BTreeMap<String, ProviderDescriptor>,
}

impl ProviderRegistry {
    /// Builds the enabled public-provider registry.
    /// 构建已启用公共供应商注册表。
    ///
    /// # Returns
    /// 返回值
    /// A strict registry containing every currently implemented provider.
    /// 返回包含每个当前已实现供应商的严格注册表。
    pub fn first_batch() -> Result<Self, String> {
        let mut registry = Self::default();
        registry.register(crate::providers::minimax_descriptor())?;
        registry.register(crate::providers::kimi_descriptor())?;
        registry.register(crate::providers::deepseek_descriptor())?;
        registry.register(crate::providers::alibaba_descriptor())?;
        registry.register(crate::providers::openrouter_descriptor())?;
        registry.register(crate::providers::apikeyfun_descriptor())?;
        registry.register(crate::providers::claudeapi_descriptor())?;
        registry.register(crate::providers::qiniu_descriptor())?;
        registry.register(crate::providers::fennoai_descriptor())?;
        registry.register(crate::providers::code0_descriptor())?;
        registry.register(crate::providers::tavily_descriptor())?;
        registry.register(crate::providers::openai_descriptor())?;
        registry.register(crate::providers::anthropic_descriptor())?;
        registry.register(crate::providers::google_descriptor())?;
        Ok(registry)
    }

    /// Builds the effective registry from immutable system providers and persisted custom providers.
    /// 根据不可变系统供应商与持久化自定义供应商构建有效注册表。
    ///
    /// # Parameters
    /// 参数
    /// `custom_providers` contains user-owned compatibility descriptors keyed by their exact identifier.
    /// `custom_providers` 包含按精确标识索引的用户自有兼容供应商描述。
    ///
    /// # Returns
    /// 返回值
    /// A validated registry containing both ownership classes.
    /// 返回包含两种所有权类别的已校验注册表。
    pub fn effective(
        custom_providers: &BTreeMap<String, ProviderDescriptor>,
    ) -> Result<Self, String> {
        let mut registry = Self::first_batch()?;
        for (provider_id, provider) in custom_providers {
            if provider_id != &provider.id {
                return Err(format!(
                    "Custom provider map key `{provider_id}` does not match descriptor `{}`.",
                    provider.id
                ));
            }
            if provider.kind != ProviderKind::Custom {
                return Err(format!(
                    "Persisted provider `{provider_id}` is not marked as custom."
                ));
            }
            if !provider.id.starts_with("custom:") {
                return Err(format!(
                    "Custom provider `{provider_id}` must use the `custom:` identifier prefix."
                ));
            }
            registry.register(provider.clone())?;
        }
        Ok(registry)
    }

    /// Registers one unique provider.
    /// 注册一个唯一供应商。
    ///
    /// # Parameters
    /// 参数
    /// `provider` is the provider descriptor.
    /// `provider` 是供应商描述。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when unique, otherwise a duplicate error.
    /// 唯一时返回 `Ok(())`，否则返回重复错误。
    pub fn register(&mut self, provider: ProviderDescriptor) -> Result<(), String> {
        if self.providers.contains_key(provider.id.as_str()) {
            return Err(format!(
                "Provider adapter `{}` is already registered.",
                provider.id
            ));
        }
        validate_provider_promotion(&provider)?;
        validate_request_projector_registration(&provider)?;
        self.providers.insert(provider.id.clone(), provider);
        Ok(())
    }

    /// Lists registered providers in stable identifier order.
    /// 按稳定标识顺序列出已注册供应商。
    ///
    /// # Returns
    /// 返回值
    /// Borrowed provider descriptors.
    /// 借用的供应商描述集合。
    pub fn providers(&self) -> Vec<&ProviderDescriptor> {
        self.providers.values().collect()
    }

    /// Returns one provider by its stable identifier.
    /// 按稳定标识返回一个供应商。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact registered identifier.
    /// `provider_id` 是精确的已注册标识。
    ///
    /// # Returns
    /// 返回值
    /// The provider descriptor when registered.
    /// 返回已注册的供应商描述。
    pub fn provider(&self, provider_id: &str) -> Option<&ProviderDescriptor> {
        self.providers.get(provider_id)
    }
}

/// Verifies every provider entry references the one registered projector owned by its definition.
/// 验证每个供应商入口引用其定义唯一持有的已注册投影器。
///
/// # Parameters
/// 参数
/// `provider` is the complete descriptor before registry insertion.
/// `provider` 是注册表插入前的完整描述。
///
/// # Returns
/// 返回值
/// Success when every entry matches the closed projector registry.
/// 每个入口匹配封闭投影器注册表时返回成功。
fn validate_request_projector_registration(provider: &ProviderDescriptor) -> Result<(), String> {
    let expected = match provider.kind {
        ProviderKind::Custom => RequestProjectorKind::Direct,
        ProviderKind::System => match provider.id.as_str() {
            "minimax" => RequestProjectorKind::MiniMax,
            "openrouter" => RequestProjectorKind::OpenRouter,
            "kimi" => RequestProjectorKind::Kimi,
            "deepseek" => RequestProjectorKind::DeepSeek,
            "alibaba" => RequestProjectorKind::Alibaba,
            "google" => RequestProjectorKind::Google,
            "tavily" | "openai" | "anthropic" | "apikeyfun" | "claudeapi" | "qiniu" | "fennoai"
            | "code0" => RequestProjectorKind::Direct,
            provider_id => {
                return Err(format!(
                    "System provider `{provider_id}` has no registered request projector owner."
                ));
            }
        },
    };
    for entry in &provider.service_entries {
        if entry.request_projector != expected {
            return Err(format!(
                "Provider `{}` entry `{}` declares projector `{:?}` but registry owner is `{:?}`.",
                provider.id, entry.id, entry.request_projector, expected
            ));
        }
    }
    Ok(())
}

/// Validates public registration metadata before exposing it to the desktop shell.
/// 在向桌面壳暴露公开注册元数据前执行校验。
///
/// # Parameters
/// 参数
/// `provider` is the complete descriptor being registered.
/// `provider` 是正在注册的完整供应商描述。
///
/// # Returns
/// 返回值
/// Success when every declared URL is HTTPS and referral metadata is internally consistent.
/// 所有已声明 URL 均为 HTTPS 且推广元数据内部一致时返回成功。
fn validate_provider_promotion(provider: &ProviderDescriptor) -> Result<(), String> {
    let Some(promotion) = provider.promotion.as_ref() else {
        return Ok(());
    };
    validate_public_provider_url(
        provider.id.as_str(),
        "signup",
        promotion.signup_url.as_str(),
    )?;
    if let Some(referral_url) = promotion.referral_url.as_deref() {
        validate_public_provider_url(provider.id.as_str(), "referral", referral_url)?;
    }
    match (promotion.partnership_kind, promotion.referral_url.is_some()) {
        (ProviderPartnershipKind::Referral, false) => {
            return Err(format!(
                "Provider `{}` declares a referral partnership without a referral URL.",
                provider.id
            ));
        }
        (ProviderPartnershipKind::None | ProviderPartnershipKind::Sponsored, true) => {
            return Err(format!(
                "Provider `{}` declares a referral URL without a referral partnership.",
                provider.id
            ));
        }
        _ => {}
    }
    // The relationship kind and localized disclosure must remain one closed semantic pair.
    // 合作关系类型与本地化披露文案必须保持为一个封闭语义对。
    let expected_disclosure_key = match promotion.partnership_kind {
        ProviderPartnershipKind::None => ProviderPromotionDisclosureKey::NeutralRegistration,
        ProviderPartnershipKind::Referral => ProviderPromotionDisclosureKey::ReferralRevenue,
        ProviderPartnershipKind::Sponsored => ProviderPromotionDisclosureKey::SponsoredRelationship,
    };
    if promotion.disclosure_key != expected_disclosure_key {
        return Err(format!(
            "Provider `{}` declares a promotion disclosure key that does not match its partnership kind.",
            provider.id
        ));
    }
    Ok(())
}

/// Validates one provider-owned external URL.
/// 校验一个由供应商描述持有的外部 URL。
///
/// # Parameters
/// 参数
/// `provider_id`, `field`, and `value` identify the exact descriptor field.
/// `provider_id`、`field` 与 `value` 标识精确的描述字段。
///
/// # Returns
/// 返回值
/// Success only for absolute HTTPS URLs without embedded credentials.
/// 仅绝对 HTTPS URL 且不包含嵌入凭据时返回成功。
fn validate_public_provider_url(provider_id: &str, field: &str, value: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value)
        .map_err(|error| format!("Provider `{provider_id}` {field} URL is invalid: {error}"))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(format!(
            "Provider `{provider_id}` {field} URL must be an absolute HTTPS URL."
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!(
            "Provider `{provider_id}` {field} URL must not embed credentials."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Strict request-projector registration tests.
    //! 严格请求投影器注册测试。

    use super::{
        CredentialDirectoryVisibility, ProviderCategory, ProviderPartnershipKind, ProviderRegistry,
    };
    use crate::{AccountUsageQueryKind, AuthenticationProjection, RequestProjectorKind};

    /// Verifies a mismatched built-in projector is rejected before runtime dispatch.
    /// 验证不匹配的内置投影器会在运行时分派前被拒绝。
    #[test]
    fn rejects_mismatched_system_projector() {
        let mut provider = crate::providers::minimax_descriptor();
        provider.service_entries[0].request_projector = RequestProjectorKind::OpenRouter;
        let mut registry = ProviderRegistry::default();
        let error = registry
            .register(provider)
            .expect_err("mismatched projector must fail");
        assert!(error.contains("registry owner"));
    }

    /// Verifies native onboarding providers have exact business classifications without fake credentials.
    /// 验证原生引导供应商具有精确业务分类且不需要虚假凭据。
    #[test]
    fn native_onboarding_providers_have_exact_categories() {
        let registry = ProviderRegistry::first_batch().expect("built-in registry should be valid");
        for provider_id in ["apikeyfun", "claudeapi", "fennoai", "code0"] {
            let provider = registry
                .provider(provider_id)
                .expect("relay provider should be registered");
            assert_eq!(provider.category, ProviderCategory::Relay);
            assert_eq!(
                provider.credential_directory_visibility,
                CredentialDirectoryVisibility::PinnedOnboarding
            );
            assert!(provider.promotion.is_some());
        }
        let qiniu = registry
            .provider("qiniu")
            .expect("Qiniu provider should be registered");
        assert_eq!(qiniu.category, ProviderCategory::CloudMaaS);
        assert_eq!(
            qiniu.credential_directory_visibility,
            CredentialDirectoryVisibility::PinnedOnboarding
        );
        assert_eq!(
            registry
                .provider("openrouter")
                .expect("OpenRouter should be registered")
                .category,
            ProviderCategory::Relay
        );
    }

    /// Verifies every pinned provider exposes only the explicitly registered verified protocol set.
    /// 验证每个固定供应商只暴露显式注册且已验证的协议集合。
    #[test]
    fn native_onboarding_providers_expose_exact_verified_routes() {
        let registry = ProviderRegistry::first_batch().expect("built-in registry should be valid");
        // Expected routes are closed per provider and must not inherit another provider's capabilities.
        // 预期路由按供应商封闭，禁止继承其他供应商能力。
        let expected_routes = [
            ("apikeyfun", vec!["openai-chat-completions.v1"]),
            ("claudeapi", vec!["anthropic-messages.v1"]),
            (
                "qiniu",
                vec!["openai-chat-completions.v1", "anthropic-messages.v1"],
            ),
            (
                "fennoai",
                vec!["openai-chat-completions.v1", "anthropic-messages.v1"],
            ),
            (
                "code0",
                vec!["openai-chat-completions.v1", "anthropic-messages.v1"],
            ),
        ];

        for (provider_id, expected_profile_ids) in expected_routes {
            let provider = registry
                .provider(provider_id)
                .expect("pinned provider should be registered");
            assert_eq!(provider.service_entries.len(), 1);
            let entry = &provider.service_entries[0];
            assert!(entry.base_url.starts_with("https://"));
            assert_eq!(entry.credential_profiles.len(), 1);
            assert_eq!(
                entry.credential_profiles[0].account_usage_query,
                AccountUsageQueryKind::Unsupported
            );
            assert!(entry.protocol_routes.iter().all(|route| route.verified));
            let actual_profile_ids = entry
                .protocol_routes
                .iter()
                .map(|route| route.protocol_profile_id.as_str())
                .collect::<Vec<_>>();
            assert_eq!(actual_profile_ids, expected_profile_ids);
        }
    }

    /// Verifies every native provider keeps its exact protocol path, authentication, and base URL.
    /// 验证每个原生供应商保持精确的协议路径、鉴权方式与基础 URL。
    #[test]
    fn native_onboarding_provider_routes_match_exact_contracts() {
        let registry = ProviderRegistry::first_batch().expect("built-in registry should be valid");
        // Each tuple closes one provider route contract so a future generic merge cannot change it silently.
        // 每个元组封闭一条供应商路由契约，避免后续通用合并静默改变它。
        let expected_routes = [
            (
                "apikeyfun",
                "https://api.apikey.fun/v1",
                "openai-chat-completions.v1",
                "/chat/completions",
                None,
                AuthenticationProjection::BearerAuthorization,
                None,
            ),
            (
                "claudeapi",
                "https://gw.claudeapi.com/v1",
                "anthropic-messages.v1",
                "/v1/messages",
                Some("https://gw.claudeapi.com"),
                AuthenticationProjection::XApiKey,
                Some("2023-06-01"),
            ),
            (
                "qiniu",
                "https://api.qnaigc.com/v1",
                "anthropic-messages.v1",
                "/messages",
                None,
                AuthenticationProjection::BearerAuthorization,
                Some("2023-06-01"),
            ),
            (
                "fennoai",
                "https://api.fenno.ai/v1",
                "anthropic-messages.v1",
                "/messages",
                None,
                AuthenticationProjection::XApiKey,
                Some("2023-06-01"),
            ),
            (
                "code0",
                "https://hk.code0.ai/v1",
                "anthropic-messages.v1",
                "/v1/messages",
                Some("https://hk.code0.ai"),
                AuthenticationProjection::XApiKey,
                Some("2023-06-01"),
            ),
        ];

        for (
            provider_id,
            expected_base_url,
            profile_id,
            expected_path,
            expected_base_override,
            expected_authentication,
            expected_anthropic_version,
        ) in expected_routes
        {
            let entry = &registry
                .provider(provider_id)
                .expect("native provider should be registered")
                .service_entries[0];
            let route = entry
                .protocol_routes
                .iter()
                .find(|route| route.protocol_profile_id == profile_id)
                .expect("expected protocol route should exist");
            assert_eq!(entry.base_url, expected_base_url);
            assert_eq!(route.endpoint_path.as_deref(), Some(expected_path));
            assert_eq!(route.base_url_override.as_deref(), expected_base_override);
            assert_eq!(
                route.authentication.unwrap_or(entry.authentication),
                expected_authentication
            );
            assert_eq!(
                route
                    .request_headers
                    .get("anthropic-version")
                    .map(String::as_str),
                expected_anthropic_version
            );
        }
    }

    /// Verifies promotion relationship kinds cannot silently present a different disclosure.
    /// 验证推广合作类型不能静默展示另一种披露文案。
    #[test]
    fn rejects_mismatched_promotion_disclosure() {
        let mut provider = crate::providers::apikeyfun_descriptor();
        provider
            .promotion
            .as_mut()
            .expect("APIKEY.FUN should expose registration metadata")
            .partnership_kind = ProviderPartnershipKind::Sponsored;
        let mut registry = ProviderRegistry::default();
        let error = registry
            .register(provider)
            .expect_err("mismatched promotion disclosure must fail");
        assert!(error.contains("does not match its partnership kind"));
    }

    /// Verifies a referral URL cannot be hidden behind neutral registration disclosure.
    /// 验证推广链接不能隐藏在普通注册披露之后。
    #[test]
    fn rejects_undisclosed_referral_url() {
        let mut provider = crate::providers::apikeyfun_descriptor();
        provider
            .promotion
            .as_mut()
            .expect("APIKEY.FUN should expose registration metadata")
            .referral_url = Some(String::from("https://apikey.fun/register?aff=vulcan"));
        let mut registry = ProviderRegistry::default();
        let error = registry
            .register(provider)
            .expect_err("undisclosed referral URL must fail");
        assert!(error.contains("without a referral partnership"));
    }

    /// Verifies public provider metadata serializes with the stable frontend field contract.
    /// 验证公开供应商元数据使用稳定的前端字段契约完成序列化。
    #[test]
    fn provider_presentation_metadata_has_stable_json_names() {
        let registry = ProviderRegistry::first_batch().expect("built-in registry should be valid");
        let qiniu = registry
            .provider("qiniu")
            .expect("Qiniu provider should be registered");
        let value = serde_json::to_value(qiniu).expect("provider should serialize");
        assert_eq!(value["category"], "cloud_maas");
        assert_eq!(
            value["credential_directory_visibility"],
            "pinned_onboarding"
        );
        assert_eq!(
            value["promotion"]["disclosure_key"],
            "settings.providerPromotion.neutralDisclosure"
        );
    }
}
