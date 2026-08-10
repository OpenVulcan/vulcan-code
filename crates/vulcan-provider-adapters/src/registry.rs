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

/// Identifies one closed credential-onboarding primitive selected by a provider manifest.
/// 标识由供应商清单选择的封闭凭据接入原语。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialRegistrationKind {
    /// Persists the submitted credential against the explicitly selected service entry.
    /// 把提交的凭据持久化到显式选择的服务入口。
    #[default]
    Direct,
    /// Resolves a MiniMax API key to its authoritative regional service entry before persistence.
    /// 在持久化前把 MiniMax API Key 解析到其权威区域服务入口。
    MiniMaxRegionalApiKey,
}

/// Describes one registered provider adapter.
/// 描述一个已注册供应商适配器。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Closed credential registration primitive selected by the manifest.
    /// 由清单选择的封闭凭据注册原语。
    #[serde(default)]
    pub credential_registration: CredentialRegistrationKind,
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

/// Providers that must stay on the legacy descriptor path until their external facts land.
/// 外部事实到齐前必须停留在旧描述路径的供应商。
///
/// APIKEY.FUN stays legacy until the user provides the model-to-protocol mapping (plan stage
/// 13); cutting over would disable every currently usable model under the zero-guess rule.
/// APIKEY.FUN 在用户提供模型-协议映射前保持旧路径（方案阶段 13）；
/// 在零猜测规则下切换会让当前全部可用模型停用。
const LEGACY_FALLBACK_PROVIDERS: &[&str] = &["apikeyfun"];

/// Reports whether one provider id is on the explicit legacy-fallback allowlist.
/// 报告一个供应商标识是否位于显式旧版回退允许名单。
///
/// Only providers on this allowlist may resolve through the old connection protocol when
/// their manifest is missing or invalid; every other cut-over system provider must fail
/// closed so the system resolver never reads the legacy connection protocol.
/// 只有本允许名单内的供应商在 Manifest 缺失或无效时可以走旧连接协议解析；
/// 其余已切换的系统供应商必须失败封闭，使系统解析器绝不读取旧连接协议。
pub fn is_legacy_fallback_provider(provider_id: &str) -> bool {
    LEGACY_FALLBACK_PROVIDERS.contains(&provider_id)
}

impl ProviderRegistry {
    /// Builds the enabled public-provider registry from the single authoritative catalog.
    /// 从唯一权威目录构建已启用公共供应商注册表。
    ///
    /// The catalog is compiled exactly once by the runtime and shared with binding,
    /// discovery, and execution; the registry derives from it instead of re-loading and
    /// re-compiling the bundled files (plan §17.6). Providers on the legacy fallback
    /// allowlist keep their code-owned descriptor; any other missing or invalid manifest
    /// fails closed so the system resolver never guesses a protocol (plan §13.1).
    /// 目录由运行时恰好编译一次，并与绑定、发现、执行共享；注册表从目录派生，而不是
    /// 重新加载并重新编译内置文件（方案 §17.6）。旧版回退允许名单内的供应商保留代码持有
    /// 描述；其余供应商的清单缺失或无效时失败封闭，使系统解析器绝不猜测协议（方案 §13.1）。
    ///
    /// # Returns
    /// 返回值
    /// A strict registry containing every currently implemented provider.
    /// 返回包含每个当前已实现供应商的严格注册表。
    pub fn first_batch() -> Result<Self, String> {
        Self::from_catalog(&crate::provider_manifest::try_load_bundled_resolved_catalog()?)
    }

    /// Builds the registry from one compiled resolved-provider catalog.
    /// 从一份已编译的已解析供应商目录构建注册表。
    ///
    /// # Parameters
    /// 参数
    /// `catalog` is the single authoritative compiled manifest set.
    /// `catalog` 是唯一权威的已编译清单集合。
    ///
    /// # Returns
    /// 返回值
    /// A strict registry; missing non-allowlist manifests fail closed.
    /// 返回严格注册表；允许名单之外清单缺失时失败封闭。
    pub fn from_catalog(
        catalog: &crate::provider_manifest::ResolvedProviderCatalog,
    ) -> Result<Self, String> {
        let mut registry = Self::default();
        // The explicit compatibility allowlist is registered first because APIKEY.FUN must
        // remain usable even when its placeholder manifest is absent. No other provider may
        // enter through this code-owned path.
        // 先注册显式兼容允许名单，因为即使 APIKEY.FUN 的占位清单缺失也必须保持可用；
        // 其他任何供应商都不得通过这条代码持有路径进入注册表。
        for provider_id in LEGACY_FALLBACK_PROVIDERS {
            registry.register(legacy_descriptor_by_id(provider_id)?)?;
        }

        for (provider_id, resolved) in &catalog.manifests {
            if resolved.descriptor.kind != ProviderKind::System {
                continue;
            }
            if resolved.descriptor.id != *provider_id {
                return Err(format!(
                    "bundled manifest file `{provider_id}` declares provider `{}`",
                    resolved.descriptor.id
                ));
            }
            if is_legacy_fallback_provider(provider_id) {
                continue;
            }
            registry.register(resolved.descriptor.clone())?;
        }
        Ok(registry)
    }

    /// Builds the registry from caller-provided bundled manifest texts.
    /// 从调用方提供的内置清单文本构建注册表。
    ///
    /// # Parameters
    /// 参数
    /// `manifests` maps provider ids to exact manifest JSON documents.
    /// `manifests` 将供应商 ID 映射到精确清单 JSON 文档。
    ///
    /// # Returns
    /// 返回值
    /// A registry built entirely from validated bundled manifests.
    /// 返回完全由已验证内置清单构建的注册表。
    pub fn from_bundled_manifests(
        manifests: &std::collections::BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let mut registry = Self::default();
        for (provider_id, text) in manifests {
            let resolved = crate::provider_manifest::compile_manifest_text(
                text,
                crate::provider_manifest::ManifestOwnership::System,
            )
            .map_err(|error| format!("bundled manifest for `{provider_id}` failed: {error}"))?;
            if resolved.descriptor.id != *provider_id {
                return Err(format!(
                    "bundled manifest file `{provider_id}` declares provider `{}`",
                    resolved.descriptor.id
                ));
            }
            registry.register(resolved.descriptor)?;
        }
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
        Self::effective_from_system(Self::first_batch()?, custom_providers)
    }

    /// Builds the effective registry from one caller-provided system registry and custom providers.
    /// 从调用方提供的系统注册表与自定义供应商构建有效注册表。
    ///
    /// # Parameters
    /// 参数
    /// `system` is the system registry derived from the single authoritative catalog;
    /// `custom_providers` contains user-owned compatibility descriptors keyed by their exact identifier.
    /// `system` 是从唯一权威目录派生的系统注册表；`custom_providers` 包含按精确标识索引的
    /// 用户自有兼容供应商描述。
    ///
    /// # Returns
    /// 返回值
    /// A validated registry containing both ownership classes.
    /// 返回包含两种所有权类别的已校验注册表。
    pub fn effective_from_system(
        system: Self,
        custom_providers: &BTreeMap<String, ProviderDescriptor>,
    ) -> Result<Self, String> {
        let mut registry = system;
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
            crate::custom_provider::validate_persisted_custom_provider_descriptor(provider)?;
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
        validate_provider_execution_scope(&provider)?;
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

/// Verifies custom providers cannot select system-only execution primitives.
/// 验证自定义供应商不能选择仅供系统使用的执行原语。
///
/// # Parameters
/// 参数
/// `provider` is the complete descriptor before registry insertion.
/// `provider` 是注册表插入前的完整描述。
///
/// # Returns
/// 返回值
/// Success when a custom provider uses direct credential registration and direct request projectors;
/// system manifests may select any closed registered primitive and are constrained separately by
/// manifest validation and trust ceilings.
/// 自定义供应商使用直接凭据注册与直接请求投影器时成功；系统清单可选择任意封闭注册原语，
/// 并由清单校验与信任上限另行约束。
fn validate_provider_execution_scope(provider: &ProviderDescriptor) -> Result<(), String> {
    if provider.kind == ProviderKind::Custom {
        if provider.credential_registration != CredentialRegistrationKind::Direct {
            return Err(format!(
                "Custom provider `{}` must use direct credential registration.",
                provider.id
            ));
        }
        for entry in &provider.service_entries {
            if entry.request_projector == RequestProjectorKind::Direct {
                continue;
            }
            return Err(format!(
                "Custom provider `{}` entry `{}` must use the direct request projector.",
                provider.id, entry.id
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

/// Resolves one explicitly permitted legacy descriptor by provider id.
/// 按供应商 ID 解析一个被显式允许的旧描述。
///
/// # Parameters
/// 参数
/// `provider_id` is the exact identifier on the legacy-fallback allowlist.
/// `provider_id` 是旧版回退允许名单中的精确标识。
///
/// # Returns
/// 返回值
/// The compatibility descriptor, or an error for every non-allowlisted provider.
/// 返回兼容描述；任何不在允许名单内的供应商都会返回错误。
fn legacy_descriptor_by_id(provider_id: &str) -> Result<ProviderDescriptor, String> {
    let descriptor = match provider_id {
        "apikeyfun" => crate::providers::apikeyfun_descriptor(),
        _ => return Err(format!("unknown legacy provider id `{provider_id}`")),
    };
    Ok(descriptor)
}

#[cfg(test)]
mod tests {
    //! Strict provider execution-scope registration tests.
    //! 严格供应商执行作用域注册测试。

    use super::{
        CredentialDirectoryVisibility, CredentialRegistrationKind, ProviderCategory,
        ProviderPartnershipKind, ProviderPromotionDisclosureKey, ProviderRegistry,
    };
    use crate::{
        AccountUsageQueryKind, AuthenticationProjection, CustomProviderAuthentication,
        CustomProviderDefinition, CustomProviderModelDiscoveryMode, CustomProviderProtocol,
        RequestProjectorKind,
    };
    use std::collections::BTreeMap;

    /// Verifies a system descriptor may select any registered projector without a provider-id table.
    /// 验证系统描述符无需供应商 ID 映射表即可选择任意已注册投影器。
    #[test]
    fn accepts_manifest_selected_system_projector() {
        // provider is compiled from the authoritative bundled manifest before the projector mutation.
        // provider 在投影器变更前从权威内置 Manifest 编译得到。
        let mut provider = crate::provider_manifest::load_bundled_resolved_catalog()
            .manifest("minimax")
            .expect("bundled MiniMax manifest must exist")
            .descriptor
            .clone();
        provider.service_entries[0].request_projector = RequestProjectorKind::RoutedCompatible;
        let mut registry = ProviderRegistry::default();
        registry
            .register(provider)
            .expect("closed system projector should be manifest-selected");
    }

    /// Verifies persisted custom providers cannot activate a system credential-onboarding primitive.
    /// 验证持久化自定义供应商不能激活系统凭据接入原语。
    #[test]
    fn rejects_system_credential_registration_for_custom_provider() {
        let mut provider = crate::custom_provider::CustomProviderDefinition {
            id: String::from("custom:credential-scope"),
            display_name: String::from("Credential scope"),
            base_url: String::from("https://example.com/v1"),
            protocol: Some(crate::custom_provider::CustomProviderProtocol::OpenAiChat),
            authentication:
                crate::custom_provider::CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: crate::custom_provider::CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocols: Vec::new(),
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("custom provider fixture should compile");
        provider.credential_registration = CredentialRegistrationKind::MiniMaxRegionalApiKey;

        let error = ProviderRegistry::effective(&BTreeMap::from([(provider.id.clone(), provider)]))
            .expect_err("custom provider must not activate a system onboarding primitive");
        assert!(error.contains("direct credential registration"), "{error}");
    }

    /// Verifies effective-registry construction revalidates persisted custom-provider security.
    /// 验证有效注册表构建会重新校验持久化自定义供应商安全边界。
    #[test]
    fn effective_registry_rejects_tampered_custom_provider() {
        let mut descriptor = CustomProviderDefinition {
            id: String::from("custom:tampered"),
            display_name: String::from("Tampered"),
            base_url: String::from("https://example.com/v1"),
            protocol: Some(CustomProviderProtocol::OpenAiChat),
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
            protocols: Vec::new(),
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("custom provider fixture should be valid");
        descriptor.service_entries[0].base_url =
            String::from("https://user:private@example.com/v1");

        let error =
            ProviderRegistry::effective(&BTreeMap::from([(descriptor.id.clone(), descriptor)]))
                .expect_err("tampered persisted provider must fail registry construction");

        assert!(error.contains("credentials"));
        assert!(!error.contains("private"));
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

    /// Verifies every promoted onboarding provider exposes the exact disclosed invitation link.
    /// 验证每个推广引导供应商都暴露精确且已披露的邀请链接。
    #[test]
    fn native_onboarding_referrals_are_disclosed_exactly() {
        let registry = ProviderRegistry::first_batch().expect("built-in registry should be valid");
        for (provider_id, referral_url, campaign_id) in [
            (
                "apikeyfun",
                "https://apikey.fun/register?aff=LY4DT3DCTM9N",
                "LY4DT3DCTM9N",
            ),
            (
                "claudeapi",
                "https://console.apito.ai/register?aff=8jvf",
                "8jvf",
            ),
            ("qiniu", "https://s.qiniu.com/reuYBn", "reuYBn"),
            ("fennoai", "https://api.fenno.ai/s/v59N", "v59N"),
            (
                "code0",
                "https://console.code0.ai/register?aff=il6p",
                "il6p",
            ),
        ] {
            let promotion = registry
                .provider(provider_id)
                .and_then(|provider| provider.promotion.as_ref())
                .expect("promoted onboarding provider should expose metadata");
            assert_eq!(promotion.referral_url.as_deref(), Some(referral_url));
            assert_eq!(promotion.campaign_id.as_deref(), Some(campaign_id));
            assert_eq!(
                promotion.partnership_kind,
                ProviderPartnershipKind::Referral
            );
            assert_eq!(
                promotion.disclosure_key,
                ProviderPromotionDisclosureKey::ReferralRevenue
            );
        }
    }

    /// Verifies every pinned provider exposes only the explicitly registered verified protocol set.
    /// 验证每个固定供应商只暴露显式注册且已验证的协议集合。
    #[test]
    fn native_onboarding_providers_expose_exact_verified_routes() {
        let registry = ProviderRegistry::first_batch().expect("built-in registry should be valid");
        // Expected routes are closed per provider and must not inherit another provider's capabilities.
        // 预期路由按供应商封闭，禁止继承其他供应商能力。
        let expected_routes = [
            ("apikeyfun", 2, vec!["openai-chat-completions.v1"]),
            ("claudeapi", 1, vec!["anthropic-messages.v1"]),
            (
                "qiniu",
                1,
                vec!["openai-chat-completions.v1", "anthropic-messages.v1"],
            ),
            (
                "fennoai",
                1,
                vec!["openai-chat-completions.v1", "anthropic-messages.v1"],
            ),
            (
                "code0",
                1,
                vec!["openai-chat-completions.v1", "anthropic-messages.v1"],
            ),
        ];

        for (provider_id, expected_entry_count, mut expected_profile_ids) in expected_routes {
            let provider = registry
                .provider(provider_id)
                .expect("pinned provider should be registered");
            assert_eq!(provider.service_entries.len(), expected_entry_count);
            assert!(provider.service_entries.iter().all(|entry| {
                entry.base_url.starts_with("https://")
                    && entry.credential_profiles.len() == 1
                    && entry.credential_profiles[0].account_usage_query
                        == AccountUsageQueryKind::Unsupported
                    && entry.protocol_routes.iter().all(|route| route.verified)
            }));
            let mut actual_profile_ids = provider
                .service_entries
                .iter()
                .flat_map(|entry| entry.protocol_routes.iter())
                .map(|route| route.protocol_profile_id.as_str())
                .collect::<Vec<_>>();
            actual_profile_ids.sort_unstable();
            actual_profile_ids.dedup();
            expected_profile_ids.sort_unstable();
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

    /// Verifies APIKEY.FUN exposes its default and image-disabled SLB endpoints independently.
    /// 验证 APIKEY.FUN 独立暴露默认入口与禁用生图的 SLB 入口。
    #[test]
    fn apikeyfun_exposes_exact_default_and_slb_service_entries() {
        let registry = ProviderRegistry::first_batch().expect("built-in registry should be valid");
        let provider = registry
            .provider("apikeyfun")
            .expect("APIKEY.FUN provider should be registered");
        let entries = provider
            .service_entries
            .iter()
            .map(|entry| {
                let route = entry
                    .protocol_routes
                    .iter()
                    .find(|route| route.protocol_profile_id == "openai-chat-completions.v1")
                    .expect("APIKEY.FUN entry should expose the OpenAI Chat route");
                (
                    entry.id.as_str(),
                    entry.display_name.as_str(),
                    entry.base_url.as_str(),
                    route.endpoint_path.as_deref(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            entries,
            vec![
                (
                    "public-api",
                    "Default API",
                    "https://api.apikey.fun/v1",
                    Some("/chat/completions"),
                ),
                (
                    "slb-api",
                    "SLB API (Image Generation Disabled)",
                    "https://slb.apikey.fun/v1",
                    Some("/chat/completions"),
                ),
            ]
        );
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
            .disclosure_key = ProviderPromotionDisclosureKey::NeutralRegistration;
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
        let promotion = provider
            .promotion
            .as_mut()
            .expect("APIKEY.FUN should expose registration metadata");
        promotion.partnership_kind = ProviderPartnershipKind::None;
        promotion.disclosure_key = ProviderPromotionDisclosureKey::NeutralRegistration;
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
            "settings.providerPromotion.referralDisclosure"
        );
        assert_eq!(value["promotion"]["partnership_kind"], "referral");
        assert_eq!(
            value["promotion"]["referral_url"],
            "https://s.qiniu.com/reuYBn"
        );
    }
}
