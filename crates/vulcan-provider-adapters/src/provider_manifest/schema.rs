//! Provider Manifest v1 schema DTOs with strict unknown-field rejection.
//! Provider Manifest v1 Schema DTO，严格拒绝未知字段。

use crate::{
    AccountUsageQueryKind, AuthenticationProjection, CredentialBillingKind, CredentialMode,
    InputResourceConstraint, ModelDiscoveryKind, OAuthProviderKind, ProviderPartnershipKind,
    ProviderPromotionDisclosureKey, ProviderToolCapabilityKind, ProviderToolExecutionContract,
    ProviderToolOutputContract, ProviderToolResourceEncoding, ResponsesRouteFeatures,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    AiOperation, CapabilitySupport, EmbeddingCapabilities, InferenceUsageScope,
    ModelParameterDefinition, NativeToolKind, PromptCacheCapability, ReasoningControlMode,
    ReasoningEffort, RerankCapabilities, ResourceKind,
};

use super::{MANIFEST_SCHEMA_VERSION, RuleMatcher};

/// Root document of one Provider Manifest v1 file.
/// 单份 Provider Manifest v1 文件的根文档。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderManifestV1 {
    /// Fixed schema revision identifier.
    /// 固定 Schema 修订标识。
    pub schema_version: String,
    /// Stable provider identifier; must equal the file identity.
    /// 稳定供应商标识；必须等于文件身份。
    pub provider_id: String,
    /// Provider-scoped monotonically increasing revision.
    /// 供应商作用域内单调递增修订号。
    pub revision: u64,
    /// UTC publication timestamp used only for audit.
    /// 仅用于审计的 UTC 发布时间。
    pub published_at: String,
    /// Minimum engine contract and required primitives.
    /// 最低引擎契约与所需原语。
    pub engine_contract: EngineContract,
    /// User-facing provider display metadata.
    /// 面向用户的供应商展示元数据。
    pub display: ManifestDisplay,
    /// Closed credential registration primitive used before connection persistence.
    /// 在连接持久化前使用的封闭凭据注册原语。
    #[serde(default)]
    pub credential_registration: crate::CredentialRegistrationKind,
    /// Service entries, credential profiles, protocol routes, and discovery.
    /// 服务入口、凭据档、协议路由与发现。
    pub services: Vec<ManifestServiceEntry>,
    /// Provider-level default capabilities and model policy.
    /// 供应商级默认能力与模型策略。
    pub model_defaults: ModelDefaults,
    /// Exact/Prefix model rules with scoped overrides.
    /// Exact/Prefix 模型规则与作用域覆盖。
    #[serde(default)]
    pub model_rules: Vec<ModelRule>,
    /// Named usage-extraction profile declarations.
    /// 具名 usage 提取 Profile 声明。
    #[serde(default)]
    pub usage_profiles: Vec<UsageProfileDeclaration>,
    /// Named request-projection profile declarations.
    /// 具名请求投影 Profile 声明。
    #[serde(default)]
    pub request_profiles: Vec<RequestProfileDeclaration>,
    /// Named response/error profile declarations.
    /// 具名响应/错误 Profile 声明。
    #[serde(default)]
    pub response_profiles: Vec<ResponseProfileDeclaration>,
    /// Named upload channel declarations.
    /// 具名上传通道声明。
    #[serde(default)]
    pub upload_channels: Vec<UploadChannelDeclaration>,
    /// Named operation template declarations.
    /// 具名操作模板声明。
    #[serde(default)]
    pub operations: Vec<OperationDeclaration>,
    /// Named provider-tool target declarations.
    /// 具名供应商工具目标声明。
    #[serde(default)]
    pub tool_targets: Vec<ToolTargetDeclaration>,
    /// Preauthorized provider-tool target templates available to catalog models.
    /// 可供目录模型使用的预授权供应商工具目标模板。
    #[serde(default)]
    pub tool_target_templates: Vec<ToolTargetTemplateDeclaration>,
    /// Data-only bindings that instantiate preauthorized templates for exact upstream models.
    /// 为精确上游模型实例化预授权模板的纯数据绑定。
    #[serde(default)]
    pub tool_target_template_bindings: Vec<ToolTargetTemplateBindingDeclaration>,
    /// Source, evidence, and verification provenance.
    /// 来源、证据与验证溯源。
    pub provenance: ManifestProvenance,
}

impl ProviderManifestV1 {
    /// Reports whether this document uses the schema revision fixed by this crate.
    /// 报告该文档是否使用当前 crate 固定的 Schema 修订。
    pub fn has_current_schema(&self) -> bool {
        self.schema_version == MANIFEST_SCHEMA_VERSION
    }
}

/// Declares the minimum engine contract required to activate one manifest.
/// 声明激活一份 Manifest 所需的最低引擎契约。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineContract {
    /// Minimum manifest engine version understood by the host.
    /// 宿主理解的最低 Manifest 引擎版本。
    pub minimum_manifest_engine: u32,
    /// Protocol profile identifiers that must be registered by the host.
    /// 宿主必须已注册的协议 Profile 标识。
    #[serde(default)]
    pub required_protocol_profiles: Vec<String>,
    /// Generic execution primitive identifiers that must be registered.
    /// 必须已注册的通用执行原语标识。
    #[serde(default)]
    pub required_execution_primitives: Vec<String>,
    /// Upload primitive identifiers that must be registered.
    /// 必须已注册的上传原语标识。
    #[serde(default)]
    pub required_upload_primitives: Vec<String>,
    /// Field-mapping transformer identifiers that must be registered.
    /// 必须已注册的字段映射转换器标识。
    #[serde(default)]
    pub required_transformers: Vec<String>,
}

/// User-facing provider display metadata.
/// 面向用户的供应商展示元数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDisplay {
    /// User-visible provider name.
    /// 用户可见供应商名称。
    pub name: String,
    /// Provider business category.
    /// 供应商业务类别。
    #[serde(default)]
    pub category: ManifestCategory,
    /// Credential-directory visibility policy.
    /// 凭据目录可见性策略。
    #[serde(default)]
    pub credential_directory_visibility: ManifestDirectoryVisibility,
    /// Optional public registration and referral metadata.
    /// 可选的公开注册与推广元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promotion: Option<ManifestPromotion>,
}

/// Provider business category projected from the manifest.
/// 从 Manifest 投影的供应商业务类别。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestCategory {
    /// The relationship has not been classified.
    /// 尚未分类的关系。
    #[default]
    Unclassified,
    /// An official model-provider endpoint.
    /// 模型厂商官方入口。
    Official,
    /// An official cloud or MaaS service.
    /// 官方云服务或 MaaS 服务。
    CloudMaaS,
    /// A third-party relay or aggregation service.
    /// 第三方中转或聚合服务。
    Relay,
}

/// Credential-directory visibility projected from the manifest.
/// 从 Manifest 投影的凭据目录可见性。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestDirectoryVisibility {
    /// Shows the provider only after at least one credential exists.
    /// 仅在至少存在一个凭据后显示供应商。
    #[default]
    ConfiguredOnly,
    /// Keeps a provider-owned onboarding entry visible.
    /// 固定显示供应商引导入口。
    PinnedOnboarding,
}

/// Public registration and referral metadata declared by one manifest.
/// 由 Manifest 声明的公开注册与推广元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPromotion {
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
    /// Stable localization key for the disclosure.
    /// 披露的稳定本地化键。
    pub disclosure_key: ProviderPromotionDisclosureKey,
}

/// One service entry declared by the manifest.
/// 由 Manifest 声明的单个服务入口。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestServiceEntry {
    /// Stable entry identifier scoped to the provider.
    /// 供应商作用域内稳定的入口标识。
    pub service_entry_id: String,
    /// User-visible entry name.
    /// 用户可见入口名称。
    pub display_name: String,
    /// Business kind: conversation or provider-tools.
    /// 业务种类：对话或供应商工具。
    #[serde(default)]
    pub kind: ManifestEntryKind,
    /// Inference base URL.
    /// 推理基础 URL。
    pub base_url: String,
    /// Credential acquisition and billing profiles.
    /// 凭据获取与计费档。
    pub credential_profiles: Vec<ManifestCredentialProfile>,
    /// Default authentication projection for this entry.
    /// 此入口的默认鉴权投影。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_authentication: Option<AuthenticationProjection>,
    /// Fixed non-secret headers added to every request for this entry.
    /// 为此入口每个请求添加的固定非敏感请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub request_headers: BTreeMap<String, String>,
    /// Optional credential-scoped service-origin override contract.
    /// 可选的凭据作用域服务源地址覆盖契约。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_origin_override_policy: Option<crate::ServiceOriginOverridePolicy>,
    /// Closed native inference actions selected by this entry.
    /// 此入口选择的封闭原生推理动作。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_inference_bindings: Vec<crate::NativeInferenceBinding>,
    /// Registered request projector; omitted means the direct projector.
    /// 已注册请求投影器；省略表示直接投影器。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_projector: Option<crate::RequestProjectorKind>,
    /// Model-catalog source policy and discovery declaration.
    /// 模型目录来源策略与发现声明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_catalog: Option<ManifestModelCatalog>,
    /// Verified protocol routes available through this entry.
    /// 此入口可用的已验证协议路由。
    pub protocol_routes: Vec<ManifestProtocolRoute>,
    /// Optional entry-scoped model-defaults overlay.
    /// 可选入口作用域模型默认覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_defaults_override: Option<ModelDefaultsOverlay>,
}

/// Business kind of one manifest service entry.
/// 单个 Manifest 服务入口的业务种类。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestEntryKind {
    /// The entry exposes conversation models and wire protocols.
    /// 当前入口暴露对话模型与 Wire 协议。
    #[default]
    Conversation,
    /// The entry exposes provider-native AI tools without a chat model.
    /// 当前入口暴露不依赖聊天模型的供应商原生 AI 工具。
    ProviderTools,
}

/// One credential acquisition and billing profile declared by the manifest.
/// 由 Manifest 声明的单个凭据获取与计费档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestCredentialProfile {
    /// Stable profile identifier scoped to one service entry.
    /// 服务入口作用域内稳定的配置档标识。
    pub credential_profile_id: String,
    /// User-visible profile name.
    /// 用户可见配置档名称。
    pub display_name: String,
    /// Exact credential acquisition mode.
    /// 精确凭据获取方式。
    #[serde(default)]
    pub credential_mode: CredentialMode,
    /// Optional OAuth flow for renewable credentials.
    /// 可续期凭据使用的可选 OAuth 流程。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_provider: Option<OAuthProviderKind>,
    /// Exact billing semantics for this credential.
    /// 当前凭据的精确计费语义。
    #[serde(default)]
    pub billing_kind: CredentialBillingKind,
    /// Credential-scoped account usage query kind.
    /// 凭据作用域的账号用量查询类别。
    #[serde(default)]
    pub account_usage_query: AccountUsageQueryKind,
    /// Optional credential-specific authentication override.
    /// 可选的凭据专属鉴权覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationProjection>,
    /// Required literal key prefix for token credentials when declared.
    /// Token 凭据声明时要求的字面 Key 前缀。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_prefix: Option<String>,
    /// Optional declared credential role consumed by the host secret projection.
    /// 可选声明的凭据角色，供宿主秘密投影消费。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_role: Option<String>,
    /// Usage scopes permitted by the exact provider product contract.
    /// 精确供应商产品契约允许的使用场景。
    #[serde(default)]
    pub allowed_usage_scopes: BTreeSet<InferenceUsageScope>,
}

/// Model-catalog source policy and discovery declaration for one entry.
/// 单个入口的模型目录来源策略与发现声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestModelCatalog {
    /// Explicit model source policy; manual is never an implicit failure fallback.
    /// 显式模型来源策略；手动模式绝不是隐式失败回退。
    pub source_policy: ModelSourcePolicy,
    /// Optional discovery implementation kind.
    /// 可选的发现实现类别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_kind: Option<ModelDiscoveryKind>,
    /// Optional discovery path relative to the resolved base URL.
    /// 相对于已解析基础 URL 的可选发现路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional authoritative model inventory for manifest-backed static catalogs.
    /// 用于 Manifest 驱动静态目录的可选权威模型清单。
    ///
    /// `ManifestStatic` requires `Some`, including an explicit empty list, as the complete
    /// service-entry inventory. Other discovery kinds may omit it when no manifest inventory exists.
    /// `ManifestStatic` 要求使用 `Some`（包括显式空列表）作为完整服务入口清单；其他发现类别
    /// 在不存在 Manifest 清单时可以省略此字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<ManifestCatalogModel>>,
}

/// Declares one model identity in an authoritative manifest-backed catalog.
/// 在权威 Manifest 驱动目录中声明一个模型身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestCatalogModel {
    /// Exact upstream model identifier.
    /// 精确的上游模型标识。
    pub model_id: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    pub display_name: String,
}

/// Selects the model-catalog source policy for one service entry.
/// 为单个服务入口选择模型目录来源策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSourcePolicy {
    /// Only manifest-declared models are visible.
    /// 仅 Manifest 声明模型可见。
    ManifestOnly,
    /// Only remote discovery evidence is visible.
    /// 仅远程发现证据可见。
    DiscoveryOnly,
    /// Manifest-declared models and discovery evidence are merged.
    /// Manifest 声明模型与发现证据合并。
    ManifestAndDiscovery,
    /// Only user-managed manual models are visible.
    /// 仅用户管理的手动模型可见。
    ManualOnly,
    /// Remote discovery and manual models are merged.
    /// 远程发现与手动模型合并。
    DiscoveryAndManual,
    /// Manifest, discovery, and manual models are merged.
    /// Manifest、发现与手动模型合并。
    ManifestDiscoveryAndManual,
}

impl ModelSourcePolicy {
    /// Reports whether manifest-declared model identities participate in this policy.
    /// 报告 Manifest 声明的模型身份是否参与此策略。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the three closed policies that explicitly include Manifest identity.
    /// 仅对三个显式包含 Manifest 身份的封闭策略返回 `true`。
    pub const fn includes_manifest(self) -> bool {
        matches!(
            self,
            Self::ManifestOnly | Self::ManifestAndDiscovery | Self::ManifestDiscoveryAndManual
        )
    }

    /// Reports whether remote discovery evidence participates in this policy.
    /// 报告远端发现证据是否参与此策略。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the four closed policies that explicitly include discovery.
    /// 仅对四个显式包含发现的封闭策略返回 `true`。
    pub const fn includes_discovery(self) -> bool {
        matches!(
            self,
            Self::DiscoveryOnly
                | Self::ManifestAndDiscovery
                | Self::DiscoveryAndManual
                | Self::ManifestDiscoveryAndManual
        )
    }

    /// Reports whether user-managed manual model identities participate in this policy.
    /// 报告用户管理的手工模型身份是否参与此策略。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the three closed policies that explicitly include manual identity.
    /// 仅对三个显式包含手工身份的封闭策略返回 `true`。
    pub const fn includes_manual(self) -> bool {
        matches!(
            self,
            Self::ManualOnly | Self::DiscoveryAndManual | Self::ManifestDiscoveryAndManual
        )
    }
}

/// One verified protocol route declared by the manifest.
/// 由 Manifest 声明的单条已验证协议路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestProtocolRoute {
    /// Complete stable protocol profile identifier.
    /// 完整稳定协议 Profile 标识。
    pub protocol_profile_id: String,
    /// Optional provider endpoint override; absent values retain the protocol-encoded path.
    /// 可选供应商端点覆盖；缺失时保留协议编码路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_path: Option<String>,
    /// Optional protocol-specific base URL for separated-endpoint providers.
    /// 分离端点供应商使用的可选协议专属基础 URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url_override: Option<String>,
    /// Optional route-specific authentication projection.
    /// 可选的路由专属鉴权投影。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationProjection>,
    /// Fixed non-secret headers scoped only to this protocol route.
    /// 仅作用于当前协议路由的固定非敏感请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub request_headers: BTreeMap<String, String>,
    /// Optional route-specific model-discovery implementation.
    /// 可选的路由专属模型发现实现。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery: Option<ModelDiscoveryKind>,
    /// Optional protocol-specific discovery path.
    /// 可选的协议专属发现路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery_path: Option<String>,
    /// Optional discovery base URL when metadata and execution use different roots.
    /// 元数据发现与执行使用不同根地址时的可选发现基础 URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery_base_url_override: Option<String>,
    /// Optional user-declared Responses transport and compaction capabilities.
    /// 可选的用户声明 Responses 传输与压缩能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responses_features: Option<ResponsesRouteFeatures>,
    /// Input resource restrictions frozen into this protocol route.
    /// 冻结到当前协议路由的输入资源限制。
    #[serde(default)]
    pub input_resource_constraints: Vec<InputResourceConstraint>,
    /// Referenced usage-extraction profile declarations.
    /// 引用的 usage 提取 Profile 声明。
    #[serde(default)]
    pub usage_profile_refs: Vec<String>,
    /// Referenced request-projection profile declarations.
    /// 引用的请求投影 Profile 声明。
    #[serde(default)]
    pub request_profile_refs: Vec<String>,
    /// Referenced response/error profile declarations.
    /// 引用的响应/错误 Profile 声明。
    #[serde(default)]
    pub response_profile_refs: Vec<String>,
    /// Ordered response-header names that may carry the upstream provider request identifier.
    /// 可承载上游供应商请求标识的有序响应头名称。
    #[serde(default)]
    pub provider_request_id_headers: Vec<String>,
    /// Whether this route has completed real verification.
    /// 此路由是否已完成真实验证。
    #[serde(default)]
    pub verified: bool,
}

/// Deserializes one nullable patch field so `null` becomes `Some(None)` (Clear) while a
/// missing field stays `None` (Inherit).
/// 反序列化一个可空补丁字段，使 `null` 变为 `Some(None)`（Clear），缺失字段保持 `None`（Inherit）。
fn deserialize_double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

/// Manifest capability patch: field-level presence for defaults and rule overrides.
/// Manifest 能力补丁：默认层与规则覆盖的字段级 presence。
///
/// JSON presence semantics (plan §6.4): an omitted field parses to `None` and inherits the
/// lower layer (Inherit); a `null` on a nullable field parses to `Some(None)` and clears the
/// lower layer (Clear); any other value parses to `Some(Some(value))` and replaces the lower
/// layer (Set). Non-nullable fields reject `null` in the JSON Schema before deserialization.
/// JSON presence 语义（方案 §6.4）：字段缺失解析为 `None`，继承下层（Inherit）；
/// 可空字段的 `null` 解析为 `Some(None)`，清除下层（Clear）；其他值解析为
/// `Some(Some(value))`，替换下层（Set）。非可空字段的 `null` 在 JSON Schema 层
/// 反序列化前被拒绝。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ManifestCapabilityPatch {
    /// Supported operations; omitted inherits.
    /// 支持的操作；省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operations: Option<Vec<AiOperation>>,
    /// Supported input resource kinds; omitted inherits.
    /// 支持的输入资源类别；省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_resources: Option<Vec<ResourceKind>>,
    /// Supported output resource kinds; omitted inherits.
    /// 支持的输出资源类别；省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_resources: Option<Vec<ResourceKind>>,
    /// Strong embedding capabilities; `null` clears, omitted inherits.
    /// 强类型向量能力；`null` 清除，省略继承。
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub embedding: Option<Option<EmbeddingCapabilities>>,
    /// Strong rerank capabilities; `null` clears, omitted inherits.
    /// 强类型重排能力；`null` 清除，省略继承。
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub rerank: Option<Option<RerankCapabilities>>,
    /// Reasoning-content support; omitted inherits.
    /// 推理内容支持；省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<CapabilitySupport>,
    /// Provider-native tools; an explicit empty list clears, omitted inherits.
    /// 供应商原生工具；显式空列表清除，省略继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_native_tools: Option<Vec<NativeToolKind>>,
    /// Exact reasoning control behavior; omitted inherits.
    /// 精确推理控制行为；省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_control: Option<ReasoningControlMode>,
    /// User-selectable reasoning-effort levels; omitted inherits.
    /// 用户可选推理强度列表；省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_efforts: Option<Vec<ReasoningEffort>>,
    /// Context limit; `null` clears, omitted inherits.
    /// 上下文限制；`null` 清除，省略继承。
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_limit: Option<Option<u64>>,
    /// Output limit; `null` clears, omitted inherits.
    /// 输出限制；`null` 清除，省略继承。
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_limit: Option<Option<u64>>,
    /// Prompt-cache capability; omitted inherits.
    /// 提示词缓存能力；省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache: Option<PromptCacheCapability>,
    /// Model-specific parameter definitions; an explicit empty list clears, omitted inherits.
    /// 模型独立参数定义；显式空列表清除，省略继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_parameters: Option<Vec<ModelParameterDefinition>>,
    /// Evidence revision for this capability set; omitted inherits.
    /// 此能力集合的证据修订号；省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_revision: Option<String>,
}

/// Provider-level default capabilities and model policy.
/// 供应商级默认能力与模型策略。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDefaults {
    /// Default enabled state for models without an explicit user override.
    /// 无显式用户覆盖时模型的默认启用状态。
    #[serde(default = "default_true")]
    pub default_enabled: bool,
    /// Catalog membership policy for models discovered outside manifest rules.
    /// 对 Manifest 规则之外发现模型的目录成员策略。
    #[serde(default)]
    pub catalog_membership_policy: CatalogMembershipPolicy,
    /// Behavior for models that match no Exact/Prefix rule.
    /// 未命中任何 Exact/Prefix 规则模型的行为。
    #[serde(default)]
    pub unknown_model_policy: UnknownModelPolicy,
    /// Default protocol candidate set inherited by matched rules.
    /// 已命中规则继承的默认协议候选集合。
    #[serde(default)]
    pub supported_protocols: Vec<String>,
    /// Default selected protocol; only effective under `default_binding_allowed`.
    /// 默认生效协议；仅在 `default_binding_allowed` 下生效。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_protocol: Option<String>,
    /// Default capability patch inherited by matched rules; omitted fields stay unknown.
    /// 已命中规则继承的默认能力补丁；省略字段保持未知。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ManifestCapabilityPatch>,
    /// Default usage-extraction profile references.
    /// 默认 usage 提取 Profile 引用。
    #[serde(default)]
    pub default_usage_profile_refs: Vec<String>,
    /// Default request-projection profile references.
    /// 默认请求投影 Profile 引用。
    #[serde(default)]
    pub default_request_profile_refs: Vec<String>,
    /// Default response/error profile references.
    /// 默认响应/错误 Profile 引用。
    #[serde(default)]
    pub default_response_profile_refs: Vec<String>,
    /// Default operation template references.
    /// 默认操作模板引用。
    #[serde(default)]
    pub default_operation_refs: Vec<String>,
    /// Optional exact reference to one top-level provenance source inherited by defaults.
    /// 默认层继承的、指向一个顶层来源证据的可选精确引用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Returns the default `true` for serde defaults.
/// 为 serde 默认值返回 `true`。
fn default_true() -> bool {
    true
}

/// Entry-scoped overlay over the provider-level model defaults.
/// 入口作用域内对供应商级模型默认的覆盖。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDefaultsOverlay {
    /// Overrides the default protocol candidate set for this entry.
    /// 覆盖此入口的默认协议候选集合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_protocols: Option<Vec<String>>,
    /// Overrides the default selected protocol for this entry.
    /// 覆盖此入口的默认生效协议。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_protocol: Option<String>,
}

/// Catalog membership policy for models discovered outside manifest rules.
/// 对 Manifest 规则之外发现模型的目录成员策略。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogMembershipPolicy {
    /// Discovered models enter the catalog only when a manifest rule declares them.
    /// 仅当 Manifest 规则声明时才让发现模型进入目录。
    #[default]
    DeclaredOnly,
    /// Discovered models enter the catalog with discovery evidence only.
    /// 发现模型以发现证据进入目录。
    DiscoveryEvidence,
    /// Declared models and discovery evidence both enter the catalog.
    /// 声明模型与发现证据都进入目录。
    DeclaredAndDiscovery,
}

impl CatalogMembershipPolicy {
    /// Reports whether manifest catalog identities participate in the effective model catalog.
    /// 报告 Manifest 目录身份是否参与生效模型目录。
    ///
    /// # Returns
    /// 返回值
    /// `true` for policies that admit explicit manifest catalog records.
    /// 对允许显式 Manifest 目录记录的策略返回 `true`。
    pub const fn includes_declared_identities(self) -> bool {
        matches!(self, Self::DeclaredOnly | Self::DeclaredAndDiscovery)
    }

    /// Reports whether discovery may add a model absent from every scoped manifest declaration.
    /// 报告发现结果能否加入未被任何作用域 Manifest 声明覆盖的模型。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when discovery evidence is an independent membership source.
    /// 仅当发现证据是独立成员来源时返回 `true`。
    pub const fn admits_unmatched_discovery(self) -> bool {
        matches!(self, Self::DiscoveryEvidence | Self::DeclaredAndDiscovery)
    }
}

/// Behavior for models that match no Exact/Prefix rule.
/// 未命中任何 Exact/Prefix 规则模型的行为。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownModelPolicy {
    /// Unmatched models resolve to `model_protocol_binding_missing`; model defaults are
    /// only an inheritance base for matched rules and never a fallback binding.
    /// 未命中模型解析为 `model_protocol_binding_missing`；模型默认只是已命中规则的
    /// 继承基座，绝不充当未命中兜底绑定。
    #[default]
    DeclaredRuleRequired,
    /// Maintainers explicitly allow unmatched models to inherit the default selected protocol
    /// and protocol-scoped capabilities. This is manifest fact, not runtime probing.
    /// 维护者显式允许未命中模型继承默认生效协议与协议作用域能力。
    /// 这是清单事实，不是运行时探测。
    DefaultBindingAllowed,
}

/// One Exact/Prefix model rule with scoped overrides.
/// 单条 Exact/Prefix 模型规则与作用域覆盖。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRule {
    /// Stable rule identifier unique within the manifest.
    /// 清单内唯一的稳定规则标识。
    pub rule_id: String,
    /// Optional service/credential scope; omitted means the whole provider scope.
    /// 可选服务/凭据作用域；省略表示整个供应商作用域。
    #[serde(default)]
    pub scope: RuleScope,
    /// Exact or longest-prefix matcher.
    /// 精确或最长前缀匹配器。
    pub matcher: RuleMatcher,
    /// Optional user-visible model name owned by this exact rule.
    /// 此精确规则持有的可选用户可见模型名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Supported protocol candidate set for this rule.
    /// 此规则支持的协议候选集合。
    #[serde(default)]
    pub supported_protocols: Vec<String>,
    /// Unique selected protocol for this rule.
    /// 此规则的唯一生效协议。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_protocol: Option<String>,
    /// Protocol-independent capability patch; omitted fields inherit.
    /// 协议无关能力补丁；省略字段继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ManifestCapabilityPatch>,
    /// Protocol-scoped capability patches keyed by protocol profile id.
    /// 按协议 Profile 标识索引的协议作用域能力补丁。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub protocol_capabilities: BTreeMap<String, ManifestCapabilityPatch>,
    /// Model-specific resource constraints that narrow the selected route defaults.
    /// 收紧所选路由默认值的模型专属资源约束。
    #[serde(default)]
    pub input_resource_constraints: Vec<InputResourceConstraint>,
    /// Referenced usage-extraction profile override; an explicit empty list clears the
    /// inherited provider special case while an omitted field inherits it.
    /// 引用的 usage 提取 Profile 覆盖；显式空列表清除继承的供应商特例，字段省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_profile_refs: Option<Vec<String>>,
    /// Referenced request-projection profile override; an explicit empty list clears while
    /// an omitted field inherits.
    /// 引用的请求投影 Profile 覆盖；显式空列表清除，字段省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_profile_refs: Option<Vec<String>>,
    /// Referenced response/error profile override; an explicit empty list clears while an
    /// omitted field inherits.
    /// 引用的响应/错误 Profile 覆盖；显式空列表清除，字段省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_profile_refs: Option<Vec<String>>,
    /// Registered model-specific parameter definitions.
    /// 已注册的模型独立参数定义。
    #[serde(default)]
    pub model_parameters: Vec<ModelParameterDefinition>,
    /// Referenced operation-template override; an explicit empty list clears while an omitted
    /// field inherits.
    /// 引用的操作模板覆盖；显式空列表清除，字段省略则继承。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_refs: Option<Vec<String>>,
    /// Optional account-plan names allowed to expose this matched model.
    /// 允许暴露此已匹配模型的可选账号套餐名称。
    #[serde(default)]
    pub allowed_account_plan_names: Vec<String>,
    /// Default-enabled override for this rule.
    /// 此规则的默认启用覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_enabled: Option<bool>,
    /// Evidence and source provenance for this rule.
    /// 此规则的证据与来源溯源。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
}

/// Service and credential scope of one model rule.
/// 单条模型规则的服务与凭据作用域。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleScope {
    /// Target service entry identifiers; empty means all entries.
    /// 目标服务入口标识；空表示全部入口。
    #[serde(default)]
    pub service_entry_ids: Vec<String>,
    /// Target credential profile identifiers; empty means all profiles.
    /// 目标凭据配置档标识；空表示全部配置档。
    #[serde(default)]
    pub credential_profile_ids: Vec<String>,
}

impl RuleScope {
    /// Reports whether this scope targets the given service entry.
    /// 报告该作用域是否覆盖给定服务入口。
    pub fn covers_service_entry(&self, service_entry_id: &str) -> bool {
        self.service_entry_ids.is_empty()
            || self
                .service_entry_ids
                .iter()
                .any(|id| id == service_entry_id)
    }

    /// Reports whether this scope targets the given credential profile.
    /// 报告该作用域是否覆盖给定凭据配置档。
    pub fn covers_credential_profile(&self, credential_profile_id: &str) -> bool {
        self.credential_profile_ids.is_empty()
            || self
                .credential_profile_ids
                .iter()
                .any(|id| id == credential_profile_id)
    }
}

/// One named usage-extraction profile declaration.
/// 单条具名 usage 提取 Profile 声明。
///
/// The full payload shape is expanded by plan stage 7; stage 1 only carries the stable
/// identifier needed for reference-closure validation.
/// 完整载荷结构由方案阶段 7 扩展；阶段 1 只承载引用闭合校验所需的稳定标识。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageProfileDeclaration {
    /// Stable profile identifier referenced by routes and rules.
    /// 被路由与规则引用的稳定 Profile 标识。
    pub profile_id: String,
    /// Human-readable purpose description.
    /// 人类可读的用途描述。
    #[serde(default)]
    pub description: String,
    /// Optional full usage-extraction profile payload (plan stage 7).
    /// 可选的完整 usage 提取 Profile 载荷（方案阶段 7）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<vulcan_llm_core::UsageExtractionProfile>,
}

/// One named request-projection profile declaration.
/// 单条具名请求投影 Profile 声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestProfileDeclaration {
    /// Stable profile identifier referenced by routes and rules.
    /// 被路由与规则引用的稳定 Profile 标识。
    pub profile_id: String,
    /// Human-readable purpose description.
    /// 人类可读的用途描述。
    #[serde(default)]
    pub description: String,
    /// Full closed request-projection payload (plan §6.6).
    /// 完整的封闭请求投影载荷（方案 §6.6）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<RequestProjectionProfile>,
}

/// One named response/error profile declaration.
/// 单条具名响应/错误 Profile 声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseProfileDeclaration {
    /// Stable profile identifier referenced by routes and rules.
    /// 被路由与规则引用的稳定 Profile 标识。
    pub profile_id: String,
    /// Human-readable purpose description.
    /// 人类可读的用途描述。
    #[serde(default)]
    pub description: String,
    /// Full closed response-projection payload (plan §6.6).
    /// 完整的封闭响应投影载荷（方案 §6.6）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<ResponseProjectionProfile>,
}

/// One frozen request-projection profile payload (plan §6.6).
/// 一份冻结的请求投影 Profile 载荷（方案 §6.6）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestProjectionProfile {
    /// Stable profile identifier mirroring the declaration.
    /// 与声明一致的稳定 Profile 标识。
    pub profile_id: String,
    /// Immutable monotonic revision for profile telemetry.
    /// 用于 Profile 遥测的不可变单调修订号。
    pub revision: u64,
    /// Ordered closed mapping steps applied to the encoded wire body.
    /// 顺序应用于编码 Wire 请求体的封闭映射步骤。
    #[serde(default)]
    pub steps: Vec<MappingStep>,
}

/// One frozen response-projection profile payload (plan §6.6).
/// 一份冻结的响应投影 Profile 载荷（方案 §6.6）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseProjectionProfile {
    /// Stable profile identifier mirroring the declaration.
    /// 与声明一致的稳定 Profile 标识。
    pub profile_id: String,
    /// Immutable monotonic revision for profile telemetry.
    /// 用于 Profile 遥测的不可变单调修订号。
    pub revision: u64,
    /// Ordered closed mapping steps applied to the decoded response value.
    /// 顺序应用于解码响应值的封闭映射步骤。
    #[serde(default)]
    pub steps: Vec<MappingStep>,
}

/// One closed projection mapping step (plan §6.6).
/// 一个封闭投影映射步骤（方案 §6.6）。
///
/// The step family is closed on purpose: arbitrary scripts, expressions, reflective field
/// guessing, and runtime-downloaded code are forbidden (plan §6.6). Steps run in declaration
/// order against the encoded wire request body; host-authoritative roots are rejected during
/// validation (plan §R8.3).
/// 步骤族刻意封闭：禁止任意脚本、任意表达式、反射式字段猜测与运行时下载代码（方案 §6.6）。
/// 步骤按声明顺序作用于编码后的 Wire 请求体；宿主权威根在验证期被拒绝（方案 §R8.3）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "op", rename_all = "snake_case")]
pub enum MappingStep {
    /// Overwrites one JSON pointer path with a literal value.
    /// 用字面量覆盖一个 JSON Pointer 路径。
    SetLiteral {
        /// Target JSON pointer path.
        /// 目标 JSON Pointer 路径。
        path: String,
        /// Literal value written into the target path.
        /// 写入目标路径的字面量值。
        value: Value,
    },
    /// Writes a literal only when the target path is absent.
    /// 仅当目标路径缺失时写入字面量。
    SetDefaultIfMissing {
        /// Target JSON pointer path.
        /// 目标 JSON Pointer 路径。
        path: String,
        /// Literal value written when the target is missing.
        /// 目标缺失时写入的字面量值。
        value: Value,
    },
    /// Copies one JSON pointer value to another body path.
    /// 把一个 JSON Pointer 值复制到另一个请求体路径。
    CopyJsonPointer {
        /// Source JSON pointer path.
        /// 源 JSON Pointer 路径。
        from: String,
        /// Destination JSON pointer path.
        /// 目标 JSON Pointer 路径。
        to: String,
    },
    /// Renames one JSON pointer path within the body.
    /// 在请求体内重命名一个 JSON Pointer 路径。
    RenameJsonPointer {
        /// Current JSON pointer path.
        /// 当前 JSON Pointer 路径。
        from: String,
        /// New JSON pointer path.
        /// 新的 JSON Pointer 路径。
        to: String,
    },
    /// Removes one JSON pointer path.
    /// 移除一个 JSON Pointer 路径。
    DeleteJsonPointer {
        /// Target JSON pointer path to delete.
        /// 要删除的目标 JSON Pointer 路径。
        path: String,
    },
    /// Maps a bounded source value through an explicit literal table.
    /// 通过显式字面量表映射一个有界源值。
    EnumMap {
        /// Target JSON pointer path receiving the mapped value.
        /// 接收映射值的目标 JSON Pointer 路径。
        path: String,
        /// Source location: a body JSON pointer or a validated host request field.
        /// 源位置：请求体 JSON Pointer 或已校验宿主请求字段。
        source: MappingStepSource,
        /// Source path: `/...` for `BodyJsonPointer` or `@field` / `@model_parameters.<id>` form.
        /// 源路径：`BodyJsonPointer` 使用 `/...`，或 `@field` / `@model_parameters.<id>` 形式。
        source_path: String,
        /// Explicit literal mapping table; keys are the source string values.
        /// 显式字面量映射表；键是源字符串值。
        mapping: BTreeMap<String, Value>,
        /// Behavior when the source value is not present in the mapping table.
        /// 源值不在映射表中时的行为。
        #[serde(default)]
        unmatched: EnumMapUnmatched,
        /// Value written when the source field is absent; omitted keeps the target untouched.
        /// 源字段缺失时写入的值；省略则保持目标不变。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<Value>,
    },
    /// Wraps a scalar body value into a single-element array.
    /// 把标量请求体值包装为单元素数组。
    WrapSingletonArray {
        /// Target JSON pointer path.
        /// 目标 JSON Pointer 路径。
        path: String,
    },
    /// Unwraps a single-element array into its scalar value.
    /// 把单元素数组解包为标量值。
    UnwrapSingletonArray {
        /// Target JSON pointer path.
        /// 目标 JSON Pointer 路径。
        path: String,
    },
    /// Writes a bounded template string into one body path.
    /// 把一个有界模板字符串写入一个请求体路径。
    ///
    /// Only the exact `{model_id}` placeholder is substituted; every other character is
    /// literal. The rendered value is bounded by the manifest validation byte cap.
    /// 仅替换精确 `{model_id}` 占位符；其余字符保持字面量。渲染值受 Manifest 验证字节
    /// 上限约束。
    StringTemplate {
        /// Target JSON pointer path.
        /// 目标 JSON Pointer 路径。
        path: String,
        /// Template with optional `{model_id}` placeholder.
        /// 含可选 `{model_id}` 占位符的模板。
        template: String,
    },
    /// Copies one validated host request field into the wire body (bounded allowlist).
    /// 把一个已校验宿主请求字段复制到 Wire 请求体（有界白名单）。
    CopyRequestField {
        /// Target JSON pointer path.
        /// 目标 JSON Pointer 路径。
        path: String,
        /// Bounded host field: `reasoning_effort`, `model_id`, `output_token_limit`,
        /// `cache_scope_id`, or `model_parameters.<id>`.
        /// 有界宿主字段：`reasoning_effort`、`model_id`、`output_token_limit`、
        /// `cache_scope_id` 或 `model_parameters.<id>`。
        field: String,
    },
    /// Writes one value when the source equals a condition, and another otherwise.
    /// 当源值等于条件时写入一个值，否则写入另一个值。
    ///
    /// A `null` branch value deletes the target path; a missing source leaves the target
    /// untouched. This registered primitive expresses adaptive-vs-fixed reasoning duality
    /// (plan §6.6 "other registered converters").
    /// `null` 分支值删除目标路径；源缺失时保持目标不变。该已注册原语表达
    /// 自适应与固定推理二象性（方案 §6.6「其他已注册转换器」）。
    ConditionalSet {
        /// Target JSON pointer path.
        /// 目标 JSON Pointer 路径。
        path: String,
        /// Source location: a body JSON pointer or a validated host request field.
        /// 源位置：请求体 JSON Pointer 或已校验宿主请求字段。
        source: MappingStepSource,
        /// Source path in the `@field` / `@model_parameters.<id>` or body pointer form.
        /// `@field` / `@model_parameters.<id>` 或请求体指针形式的源路径。
        source_path: String,
        /// Exact string the source value must equal for the `then` branch.
        /// 触发 `then` 分支时源值必须精确相等的字符串。
        equals: String,
        /// Value written when the source equals the condition; `null` deletes the target.
        /// 源等于条件时写入的值；`null` 删除目标。
        then: Value,
        /// Value written otherwise; `null` deletes the target.
        /// 否则写入的值；`null` 删除目标。
        otherwise: Value,
    },
    /// Deletes one target path only when a source value equals an exact condition.
    /// 仅当源值精确等于条件时删除一个目标路径。
    DeleteIfEquals {
        /// Target JSON pointer path to delete.
        /// 要删除的目标 JSON Pointer 路径。
        path: String,
        /// Source location.
        /// 源位置。
        source: MappingStepSource,
        /// Source path.
        /// 源路径。
        source_path: String,
        /// Exact string the source value must equal to trigger the deletion.
        /// 触发删除时源值必须精确相等的字符串。
        equals: String,
    },
    /// Deletes one target path unless a source equals an exact condition.
    /// 除非源值精确等于条件，否则删除一个目标路径。
    ///
    /// An absent source also deletes the target. This bounded primitive models provider fields
    /// that are valid only under one explicitly selected mode without embedding provider names
    /// in the execution engine.
    /// 源值缺失时同样删除目标。此有界原语用于表达仅在一个显式选择模式下有效的供应商字段，
    /// 无需在执行引擎中嵌入供应商名称。
    DeleteUnlessEquals {
        /// Target JSON pointer path to delete.
        /// 要删除的目标 JSON Pointer 路径。
        path: String,
        /// Source location.
        /// 源位置。
        source: MappingStepSource,
        /// Source path.
        /// 源路径。
        source_path: String,
        /// Exact string the source value must equal to retain the target.
        /// 保留目标时源值必须精确等于的字符串。
        equals: String,
    },
    /// Removes one container path when it holds an empty object or array.
    /// 当一个容器路径持有空对象或空数组时移除它。
    RemoveIfEmpty {
        /// Target JSON pointer path to remove when empty.
        /// 为空时要移除的目标 JSON Pointer 路径。
        path: String,
    },
    /// Rejects the request when a source field is present (bounded fail-closed guard).
    /// 当源字段存在时拒绝请求（有界失败封闭守卫）。
    RejectIfPresent {
        /// Source location.
        /// 源位置。
        source: MappingStepSource,
        /// Source path.
        /// 源路径。
        source_path: String,
        /// Static controlled failure message.
        /// 静态受控失败消息。
        message: String,
    },
    /// Rejects the request when every bounded source condition is present.
    /// 当全部有界来源条件均存在时拒绝请求。
    RejectIfAllPresent {
        /// Closed source conditions that must all be present to reject.
        /// 必须全部存在才拒绝的封闭来源条件。
        conditions: Vec<MappingPresenceCondition>,
        /// Static controlled failure message.
        /// 静态受控失败消息。
        message: String,
    },
    /// Replaces the complete JSON body through a bounded declarative template.
    /// 通过一份有界声明式模板替换完整 JSON 正文。
    ///
    /// The template language is closed to literal JSON plus the registered `$vulcan_ref`,
    /// `$vulcan_model_id`, `$vulcan_map`, `$vulcan_enum`, `$vulcan_if_equals`,
    /// `$vulcan_if_present`, `$vulcan_if_array_length`, `$vulcan_concat`, `$vulcan_array_concat`,
    /// `$vulcan_coalesce`, and `$vulcan_trim` forms. It cannot execute code, read files, or
    /// choose network targets.
    /// 模板语言封闭为字面 JSON 与已注册的 `$vulcan_ref`、`$vulcan_model_id`、
    /// `$vulcan_map`、`$vulcan_enum`、`$vulcan_if_equals`、`$vulcan_concat`、
    /// `$vulcan_if_present`、`$vulcan_if_array_length`、`$vulcan_concat`、`$vulcan_array_concat`、
    /// `$vulcan_coalesce` 与 `$vulcan_trim` 形式；它不能执行代码、读取文件或选择网络目标。
    RebuildJsonTemplate {
        /// Complete bounded JSON template used to rebuild the body.
        /// 用于重建正文的完整有界 JSON 模板。
        template: Value,
    },
}

/// Stable engine-contract identifiers for every installed closed mapping transformer.
/// 每个已安装封闭映射转换器的稳定引擎契约标识。
pub(crate) const REGISTERED_MAPPING_TRANSFORMER_IDS: &[&str] = &[
    "set_literal",
    "set_default_if_missing",
    "copy_json_pointer",
    "rename_json_pointer",
    "delete_json_pointer",
    "enum_map",
    "wrap_singleton_array",
    "unwrap_singleton_array",
    "string_template",
    "copy_request_field",
    "conditional_set",
    "delete_if_equals",
    "delete_unless_equals",
    "remove_if_empty",
    "reject_if_present",
    "reject_if_all_present",
    "rebuild_json_template",
];

impl MappingStep {
    /// Returns the exact installed transformer identifier required to execute this step.
    /// 返回执行当前步骤所需的精确已安装转换器标识。
    ///
    /// # Returns
    /// 返回值
    /// The stable snake-case engine-contract identifier corresponding to this closed enum case.
    /// 返回与当前封闭枚举分支对应的稳定蛇形命名引擎契约标识。
    pub(crate) fn transformer_id(&self) -> &'static str {
        match self {
            Self::SetLiteral { .. } => "set_literal",
            Self::SetDefaultIfMissing { .. } => "set_default_if_missing",
            Self::CopyJsonPointer { .. } => "copy_json_pointer",
            Self::RenameJsonPointer { .. } => "rename_json_pointer",
            Self::DeleteJsonPointer { .. } => "delete_json_pointer",
            Self::EnumMap { .. } => "enum_map",
            Self::WrapSingletonArray { .. } => "wrap_singleton_array",
            Self::UnwrapSingletonArray { .. } => "unwrap_singleton_array",
            Self::StringTemplate { .. } => "string_template",
            Self::CopyRequestField { .. } => "copy_request_field",
            Self::ConditionalSet { .. } => "conditional_set",
            Self::DeleteIfEquals { .. } => "delete_if_equals",
            Self::DeleteUnlessEquals { .. } => "delete_unless_equals",
            Self::RemoveIfEmpty { .. } => "remove_if_empty",
            Self::RejectIfPresent { .. } => "reject_if_present",
            Self::RejectIfAllPresent { .. } => "reject_if_all_present",
            Self::RebuildJsonTemplate { .. } => "rebuild_json_template",
        }
    }
}

/// One bounded presence condition used by a closed rejection step.
/// 封闭拒绝步骤使用的一条有界存在条件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingPresenceCondition {
    /// Source location.
    /// 源位置。
    pub source: MappingStepSource,
    /// Source path using an absolute JSON Pointer or a registered `@request_field`.
    /// 使用绝对 JSON Pointer 或已注册 `@request_field` 的来源路径。
    pub source_path: String,
}

/// Source location of one enum-map step input.
/// 一个枚举映射步骤输入的源位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingStepSource {
    /// Read the value from the encoded wire body JSON pointer.
    /// 从编码 Wire 请求体 JSON Pointer 读取值。
    BodyJsonPointer,
    /// Read the value from a validated host request field.
    /// 从已校验宿主请求字段读取值。
    RequestField,
}

/// Behavior of an enum-map step when the source value is unmatched.
/// 枚举映射步骤在源值不匹配时的行为。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnumMapUnmatched {
    /// Remove the target path.
    /// 移除目标路径。
    Remove,
    /// Fail the request with a controlled provider-specific diagnostic.
    /// 以受控的供应商专属诊断使请求失败。
    Error,
    /// Keep the existing target value untouched.
    /// 保持现有目标值不变。
    #[default]
    KeepLiteral,
}

/// One named upload channel declaration.
/// 单条具名上传通道声明。
///
/// The full payload shape is expanded by plan stage 9.
/// Closed upload primitive family (plan §16.3).
/// 封闭上传原语族（方案 §16.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadPrimitiveKind {
    /// Inline data URL resource encoding.
    /// 内联 Data URL 资源编码。
    InlineDataUrl,
    /// Inline base64 resource encoding.
    /// 内联 Base64 资源编码。
    InlineBase64,
    /// Remote URL passthrough with mandatory runtime HTTPS and host-allowlist revalidation.
    /// 强制在运行时重新校验 HTTPS 与宿主允许列表的远端 URL 直通。
    RemoteUrlPassthrough,
    /// Multipart file field upload.
    /// Multipart 文件字段上传。
    MultipartFile,
    /// Provider file-create endpoint upload.
    /// 供应商文件创建端点上传。
    ProviderFileCreate,
    /// Presigned form upload.
    /// 预签名表单上传。
    PresignedFormUpload,
    /// Signed object upload referencing a registered signing primitive.
    /// 引用已注册签名原语的签名对象上传。
    SignedObjectUpload,
    /// Managed asset reference used only by internal hosted-asset conversion.
    /// 仅供内部托管资产转换使用的托管资产引用。
    ManagedAssetReference,
}

/// Selects one registered signing handshake behind a signed-object upload channel.
/// 选择签名对象上传通道背后的一种已注册签名握手。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadSigningPrimitiveKind {
    /// Obtains a bounded temporary OSS policy and uploads the object through that policy.
    /// 获取有界临时 OSS 策略并通过该策略上传对象。
    TemporaryOssPolicy,
}

/// One named upload channel declaration with its full bounded payload (plan §16.3).
/// 单条具名上传通道声明及其完整有界载荷（方案 §16.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadChannelDeclaration {
    /// Stable upload channel identifier referenced by operations.
    /// 被操作引用的稳定上传通道标识。
    pub upload_channel_id: String,
    /// Human-readable purpose description.
    /// 人类可读的用途描述。
    #[serde(default)]
    pub description: String,
    /// Closed upload primitive family.
    /// 封闭上传原语族。
    #[serde(default)]
    pub primitive: Option<UploadPrimitiveKind>,
    /// Registered signing handshake required by signed-object primitives.
    /// 签名对象原语所需的已注册签名握手。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing_primitive: Option<UploadSigningPrimitiveKind>,
    /// HTTPS endpoint host the channel may contact.
    /// 通道可联系的 HTTPS 端点宿主。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_host: Option<String>,
    /// Absolute endpoint path below the host.
    /// 宿主下的绝对端点路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_path: Option<String>,
    /// Allowed HTTPS host suffixes for provider-returned signed upload destinations.
    /// 供应商返回的签名上传目标允许使用的 HTTPS 宿主后缀。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_host_suffixes: Vec<String>,
    /// Allowed exact or dot-prefixed DNS suffixes for HTTPS source URL passthrough.
    /// HTTPS 来源 URL 直通允许使用的精确 DNS 宿主或点前缀后缀。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remote_url_host_suffixes: Vec<String>,
    /// HTTP method used by this channel.
    /// 此通道使用的 HTTP 方法。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Credential role projected for this channel.
    /// 为此通道投影的凭据角色。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_role: Option<String>,
    /// Multipart file field name when the primitive is multipart.
    /// Multipart 原语使用的 Multipart 文件字段名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_field: Option<String>,
    /// Accepted resource MIME types.
    /// 接受的资源 MIME 类型。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_mime_types: Vec<String>,
    /// Maximum accepted bytes per upload.
    /// 单次上传最大字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    /// Maximum accepted file count per upload.
    /// 单次上传最大文件数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_count: Option<u64>,
    /// Maximum wall-clock time for the complete network upload lifecycle in milliseconds.
    /// 完整网络上传生命周期允许的最大墙钟时间（毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// JSON Pointer of the response identifier.
    /// 响应标识的 JSON Pointer。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id_pointer: Option<String>,
    /// JSON Pointer of the response URL.
    /// 响应 URL 的 JSON Pointer。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_url_pointer: Option<String>,
    /// Reference template consumed by the operation request.
    /// 操作请求消费的引用模板。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_template: Option<String>,
    /// Whether the channel reports progress callbacks.
    /// 通道是否上报进度回调。
    #[serde(default)]
    pub progress_supported: bool,
    /// Whether the channel supports cancellation.
    /// 通道是否支持取消。
    #[serde(default)]
    pub cancellation_supported: bool,
    /// Fixed non-secret multipart form fields sent with every upload (plan §R9.3).
    /// 每次上传随附的固定非秘密 Multipart 表单字段（方案 §R9.3）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fixed_fields: BTreeMap<String, String>,
    /// Optional cache duration in milliseconds for one uploaded object.
    /// 单个上传对象的可选缓存时长（毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_duration_ms: Option<u64>,
    /// Optional reference encoding consumed by the operation request.
    /// 操作请求消费的可选引用编码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_encoding: Option<UploadReferenceEncoding>,
}

/// One bounded reference encoding for an uploaded object (plan §R9.3).
/// 上传对象的一种有界引用编码（方案 §R9.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadReferenceEncoding {
    /// The response identifier is used directly as the provider object reference.
    /// 响应标识直接作为供应商对象引用。
    ProviderFileId,
    /// The response URL is used as the reference.
    /// 响应 URL 作为引用。
    ResponseUrl,
    /// The bounded `reference_template` renders the reference with `{id}` / `{url}`.
    /// 有界 `reference_template` 以 `{id}` / `{url}` 渲染引用。
    Template,
}

/// Binds one canonical provider-tool argument resource to declared upload channels.
/// 将一个规范供应商工具参数资源绑定到声明的上传通道。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationUploadBinding {
    /// Absolute JSON Pointer of one string or string-array source argument.
    /// 一个字符串或字符串数组源参数的绝对 JSON Pointer。
    pub source_path: String,
    /// Absolute JSON Pointer receiving one materialized reference or reference array.
    /// 接收一个已物化引用或引用数组的绝对 JSON Pointer。
    pub target_path: String,
    /// Optional relative JSON Pointer selecting one source field inside every array item.
    /// 可选相对 JSON Pointer，用于选择数组每一项内部的一个源字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_source_path: Option<String>,
    /// Optional relative JSON Pointer receiving the materialized value inside the same array item.
    /// 可选相对 JSON Pointer，用于在同一数组项内部接收已物化值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_target_path: Option<String>,
    /// Optional relative JSON Pointer selecting an item discriminator before materialization.
    /// 可选相对 JSON Pointer，用于在物化前选择数组项判别字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_match_path: Option<String>,
    /// Optional exact discriminator value required at `item_match_path`.
    /// `item_match_path` 处要求的可选精确判别值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_match_value: Option<Value>,
    /// Ordered upload-channel references eligible for this exact resource field.
    /// 对该精确资源字段合格的有序上传通道引用。
    pub upload_channel_refs: Vec<String>,
    /// Exact MIME type for every source, when the field accepts only one type.
    /// 字段仅接受一种类型时，每个源使用的精确 MIME 类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Lowercase extension (without dot) to exact MIME mappings for multi-format fields.
    /// 多格式字段使用的小写扩展名（不含点）到精确 MIME 映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mime_types_by_extension: BTreeMap<String, String>,
    /// Whether an absent source path is a pre-dispatch error.
    /// 源路径缺失时是否构成分派前错误。
    #[serde(default)]
    pub required: bool,
    /// Whether the canonical source field is removed after a different target field is written.
    /// 写入不同目标字段后是否移除规范源字段。
    #[serde(default)]
    pub remove_source: bool,
    /// Closed shape written to the target path after materialization.
    /// 物化后写入目标路径的封闭结构。
    #[serde(default)]
    pub target_encoding: OperationUploadTargetEncoding,
}

/// Closed target shapes produced by one generic operation upload binding.
/// 一条通用操作上传绑定可生成的封闭目标结构。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationUploadTargetEncoding {
    /// Preserve the legacy scalar or scalar-array provider reference shape.
    /// 保留旧有标量或标量数组供应商引用结构。
    #[default]
    Reference,
    /// Emit a provider-neutral descriptor with kind, MIME, reference, and optional Base64 payload.
    /// 生成包含类别、MIME、引用与可选 Base64 载荷的供应商中性描述符。
    ResourceDescriptor,
}

/// One named operation template declaration.
/// 单条具名操作模板声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationDeclaration {
    /// Stable operation template identifier referenced by rules.
    /// 被规则引用的稳定操作模板标识。
    pub operation_id: String,
    /// Canonical AI operation projected by this template.
    /// 此模板投影的规范 AI 操作。
    pub operation: String,
    /// Carrier used to execute this operation.
    /// 执行此操作使用的载体。
    pub carrier: OperationCarrier,
    /// Input resource kinds consumed by this operation.
    /// 此操作消费的输入资源类别。
    #[serde(default)]
    pub input_resources: Vec<ResourceKind>,
    /// Output resource kinds produced by this operation.
    /// 此操作产生的输出资源类别。
    #[serde(default)]
    pub output_resources: Vec<ResourceKind>,
    /// Referenced provider-tool targets used when the carrier is `provider_tool`.
    /// 载体为 `provider_tool` 时使用的供应商工具目标引用。
    #[serde(default)]
    pub tool_target_refs: Vec<String>,
    /// Referenced preauthorized target templates whose instances this operation owns.
    /// 当前操作持有其实例的预授权目标模板引用。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_target_template_refs: Vec<String>,
    /// Referenced request-projection profiles for the wire request.
    /// 线级请求使用的请求投影 Profile 引用。
    #[serde(default)]
    pub request_profile_refs: Vec<String>,
    /// Referenced response-projection profiles for the wire response.
    /// 线级响应使用的响应投影 Profile 引用。
    #[serde(default)]
    pub response_profile_refs: Vec<String>,
    /// Referenced upload channels used to materialize input resources.
    /// 用于物化输入资源的上传通道引用。
    #[serde(default)]
    pub upload_channel_refs: Vec<String>,
    /// Exact canonical-argument resource bindings executed before tool dispatch.
    /// 工具分派前执行的精确规范参数资源绑定。
    #[serde(default)]
    pub upload_bindings: Vec<OperationUploadBinding>,
    /// Optional asynchronous execution definition (plan §R9.6).
    /// 可选异步执行定义（方案 §R9.6）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub async_execution: Option<AsyncOperationDeclaration>,
    /// Maximum total execution time in milliseconds for one operation.
    /// 单次操作的最大总执行时间（毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Whether one operation may be safely retried after an interrupted dispatch.
    /// 中断分派后单次操作是否可安全重试。
    #[serde(default)]
    pub idempotent: bool,
}

/// One asynchronous execution definition (plan §R9.6).
/// 一个异步执行定义（方案 §R9.6）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsyncOperationDeclaration {
    /// Poll interval in milliseconds.
    /// 轮询间隔（毫秒）。
    pub poll_interval_ms: u64,
    /// Maximum delay in milliseconds after consecutive observation failures.
    /// 连续观测失败后的最大退避毫秒数。
    pub max_backoff_ms: u64,
    /// Absolute timeout in milliseconds.
    /// 绝对超时（毫秒）。
    pub timeout_ms: u64,
    /// Whether cancellation is supported by the provider.
    /// 供应商是否支持取消。
    #[serde(default)]
    pub cancellation_supported: bool,
    /// Optional closed generic JSON task lifecycle used by the `generic_json` adapter.
    /// `generic_json` 适配器使用的可选封闭通用 JSON 任务生命周期。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic_json: Option<GenericJsonAsyncLifecycleDeclaration>,
}

/// One closed JSON start/poll/cancel task contract for adapter-free manifest operations.
/// 一份供无具名适配器 Manifest 操作使用的封闭 JSON 启动/轮询/取消任务契约。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenericJsonAsyncLifecycleDeclaration {
    /// Relative HTTPS endpoint path used to observe one task.
    /// 用于观测单个任务的相对 HTTPS 端点路径。
    pub poll_endpoint_path: String,
    /// Closed HTTP method used by the poll endpoint.
    /// 轮询端点使用的封闭 HTTP 方法。
    pub poll_method: GenericJsonAsyncRequestMethod,
    /// Optional relative HTTPS endpoint path used to cancel one task.
    /// 用于取消单个任务的可选相对 HTTPS 端点路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_endpoint_path: Option<String>,
    /// Optional closed HTTP method used by the cancellation endpoint.
    /// 取消端点使用的可选封闭 HTTP 方法。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_method: Option<GenericJsonAsyncRequestMethod>,
    /// Optional stable path segment appended after the task identifier for cancellation.
    /// 取消时追加在任务标识之后的可选稳定路径段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_task_id_path_suffix: Option<String>,
    /// JSON Pointer extracting the provider task identifier from the start response.
    /// 从启动响应提取供应商任务标识的 JSON Pointer。
    pub task_id_pointer: String,
    /// Exact query parameter carrying the task identifier to poll and cancel endpoints.
    /// 向轮询与取消端点携带任务标识的精确查询参数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id_query_parameter: Option<String>,
    /// Whether the task identifier is appended as one percent-encoded endpoint path segment.
    /// 是否把任务标识作为一个经百分号编码的端点路径段追加。
    #[serde(default)]
    pub task_id_in_path: bool,
    /// Optional JSON Pointer extracting the start response status.
    /// 从启动响应提取状态的可选 JSON Pointer。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_status_pointer: Option<String>,
    /// Exact provider values accepted as successful task creation.
    /// 接受为任务创建成功的精确供应商值。
    #[serde(default)]
    pub start_success_statuses: Vec<String>,
    /// Optional JSON Pointer correlating every poll response with the frozen task identifier.
    /// 用于把每次轮询响应与冻结任务标识关联的可选 JSON Pointer。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_task_id_pointer: Option<String>,
    /// JSON Pointer extracting the exact provider lifecycle status from a poll response.
    /// 从轮询响应提取精确供应商生命周期状态的 JSON Pointer。
    pub status_pointer: String,
    /// Exact provider values mapped to the queued host phase.
    /// 映射到宿主排队阶段的精确供应商值。
    #[serde(default)]
    pub queued_statuses: Vec<String>,
    /// Exact provider values mapped to the running host phase.
    /// 映射到宿主运行阶段的精确供应商值。
    #[serde(default)]
    pub running_statuses: Vec<String>,
    /// Exact provider values mapped to successful completion.
    /// 映射到成功完成的精确供应商值。
    pub success_statuses: Vec<String>,
    /// Exact provider values mapped to terminal failure.
    /// 映射到终态失败的精确供应商值。
    pub failure_statuses: Vec<String>,
    /// Exact provider values mapped to confirmed cancellation.
    /// 映射到确认取消的精确供应商值。
    #[serde(default)]
    pub cancelled_statuses: Vec<String>,
    /// Optional response requirements applied after task creation projection.
    /// 任务创建响应投影后应用的可选响应要求。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_requirements: Option<GenericJsonResponseRequirementsDeclaration>,
    /// Optional response requirements applied to every poll observation.
    /// 应用于每次轮询观测的可选响应要求。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_requirements: Option<GenericJsonResponseRequirementsDeclaration>,
    /// Optional response requirements applied only to a successful poll observation.
    /// 仅应用于成功轮询观测的可选响应要求。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_success_requirements: Option<GenericJsonResponseRequirementsDeclaration>,
    /// Optional response requirements applied to a cancellation confirmation.
    /// 应用于取消确认的可选响应要求。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_requirements: Option<GenericJsonResponseRequirementsDeclaration>,
    /// Optional canonical argument pointer whose ordered strings are fingerprinted for recovery.
    /// 可选规范参数 Pointer，其有序字符串将被生成恢复指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_identity_source_pointer: Option<String>,
    /// Optional bounded request issued after a successful poll to resolve the final payload.
    /// 成功轮询后用于解析最终载荷的可选有界请求。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_follow_up: Option<GenericJsonAsyncSuccessFollowUpDeclaration>,
    /// Optional ordered batch follow-up used to fetch and join one result per submitted identity.
    /// 可选有序批量后续请求，用于按每个已提交身份拉取并拼接一个结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_batch_follow_up: Option<GenericJsonAsyncBatchFollowUpDeclaration>,
}

/// Closed string requirements evaluated against one projected generic JSON response.
/// 针对一份已投影通用 JSON 响应求值的封闭字符串要求。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenericJsonResponseRequirementsDeclaration {
    /// JSON Pointers that must resolve to non-empty, already-trimmed strings.
    /// 必须解析为非空且无需裁剪字符串的 JSON Pointer。
    #[serde(default)]
    pub non_empty_string_pointers: Vec<String>,
    /// JSON Pointers that must be absent, null, or empty strings.
    /// 必须缺失、为 null 或为空字符串的 JSON Pointer。
    #[serde(default)]
    pub empty_or_missing_string_pointers: Vec<String>,
}

/// One closed follow-up request that converts a successful task observation into its final payload.
/// 一份把成功任务观测转换为最终载荷的封闭后续请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenericJsonAsyncSuccessFollowUpDeclaration {
    /// Relative HTTPS endpoint path used to retrieve the final payload.
    /// 用于获取最终载荷的相对 HTTPS 端点路径。
    pub endpoint_path: String,
    /// Closed bodyless HTTP method used by the follow-up endpoint.
    /// 后续端点使用的封闭无正文 HTTP 方法。
    pub method: GenericJsonAsyncRequestMethod,
    /// Closed source of the follow-up identifier.
    /// 后续请求标识的封闭来源。
    #[serde(default)]
    pub identifier_source: GenericJsonAsyncFollowUpIdentifierSource,
    /// Optional JSON Pointer extracting the follow-up identifier from the successful poll response.
    /// 从成功轮询响应提取后续请求标识的可选 JSON Pointer。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier_pointer: Option<String>,
    /// Optional exact query parameter carrying the extracted identifier.
    /// 携带所提取标识的可选精确查询参数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier_query_parameter: Option<String>,
    /// Whether the extracted identifier is appended as one percent-encoded path segment.
    /// 是否把提取的标识作为一个经百分号编码的路径段追加。
    #[serde(default)]
    pub identifier_in_path: bool,
    /// Optional stable path segment appended after a path-carried identifier.
    /// 在通过路径携带的标识之后追加的可选稳定路径段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier_path_suffix: Option<String>,
    /// Closed response body projection used by the follow-up request.
    /// 后续请求使用的封闭响应正文投影。
    #[serde(default)]
    pub response_body: GenericJsonAsyncFollowUpResponseBody,
}

/// Closed source for one successful asynchronous follow-up identifier.
/// 一次成功异步后续请求标识的封闭来源。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenericJsonAsyncFollowUpIdentifierSource {
    /// Read the identifier from the successful poll response using a declared JSON Pointer.
    /// 使用已声明 JSON Pointer 从成功轮询响应读取标识。
    #[default]
    ResponsePointer,
    /// Reuse the frozen provider task identifier returned by task creation.
    /// 复用任务创建返回的冻结供应商任务标识。
    ProviderTaskId,
}

/// Closed response body projection for one successful asynchronous follow-up.
/// 一次成功异步后续请求的封闭响应正文投影。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenericJsonAsyncFollowUpResponseBody {
    /// Decode and project one bounded JSON document.
    /// 解码并投影一份有界 JSON 文档。
    #[default]
    Json,
    /// Materialize one bounded binary resource from the authenticated response body.
    /// 从已鉴权响应正文物化一项有界二进制资源。
    BinaryResource,
}

/// One bounded public-HTTPS fan-out that preserves submitted identity order.
/// 一项保留已提交身份顺序的有界公共 HTTPS 扇出声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenericJsonAsyncBatchFollowUpDeclaration {
    /// JSON Pointer selecting the terminal result array.
    /// 选择终态结果数组的 JSON Pointer。
    pub items_pointer: String,
    /// Relative JSON Pointer selecting each result identity.
    /// 选择每个结果身份的相对 JSON Pointer。
    pub item_identity_pointer: String,
    /// Relative JSON Pointer selecting each result status.
    /// 选择每个结果状态的相对 JSON Pointer。
    pub item_status_pointer: String,
    /// Exact item statuses accepted for follow-up retrieval.
    /// 接受执行后续拉取的精确项目状态。
    pub item_success_statuses: Vec<String>,
    /// Relative JSON Pointer selecting each public follow-up URL.
    /// 选择每个公共后续 URL 的相对 JSON Pointer。
    pub item_url_pointer: String,
    /// Closed URL encoding accepted for each follow-up locator.
    /// 每个后续定位符接受的封闭 URL 编码。
    pub url_encoding: ProviderToolResourceEncoding,
    /// Exact hosts or dot-prefixed DNS suffixes authorized for every follow-up URL.
    /// 每个后续 URL 获准使用的精确宿主或点前缀 DNS 后缀。
    pub url_host_suffixes: Vec<String>,
    /// JSON Pointer selecting the fetched response item array.
    /// 选择拉取响应项目数组的 JSON Pointer。
    pub response_items_pointer: String,
    /// Relative JSON Pointer selecting text from every fetched response item.
    /// 从每个拉取响应项目中选择文本的相对 JSON Pointer。
    pub response_text_pointer: String,
    /// Separator inserted between text channels from one fetched response.
    /// 在单份拉取响应的文本通道之间插入的分隔符。
    #[serde(default)]
    pub response_item_separator: String,
    /// Separator inserted between ordered fetched responses.
    /// 在有序拉取响应之间插入的分隔符。
    #[serde(default)]
    pub result_separator: String,
    /// Hard maximum terminal items accepted by this declaration.
    /// 此声明允许的终态项目硬上限。
    pub max_items: u16,
}

/// Closed HTTP methods accepted by generic JSON asynchronous task endpoints.
/// 通用 JSON 异步任务端点接受的封闭 HTTP 方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenericJsonAsyncRequestMethod {
    /// HTTP GET without a request body.
    /// 无请求体的 HTTP GET。
    Get,
    /// HTTP POST without replaying the task-creation body.
    /// 不重放任务创建正文的 HTTP POST。
    Post,
    /// HTTP DELETE without a request body.
    /// 无请求体的 HTTP DELETE。
    Delete,
}

/// Carrier used to execute one operation template.
/// 执行一个操作模板使用的载体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationCarrier {
    /// The operation is carried by the conversation wire protocol.
    /// 操作由对话 Wire 协议承载。
    ConversationProtocol,
    /// The operation is carried by a provider-tool target.
    /// 操作由供应商工具目标承载。
    ProviderTool,
    /// The operation is carried by a registered native action.
    /// 操作由已注册内部动作承载。
    NativeAction,
    /// The operation is explicitly unsupported.
    /// 操作明确不可用。
    Unsupported,
}

/// One named provider-tool target declaration.
/// 单条具名供应商工具目标声明。
///
/// The declaration carries the complete runtime target payload (plan §R9.2): the AI-visible
/// contract and the exact provider execution contract compile into a `ProviderToolTarget`
/// owned by the manifest's descriptor. Code-owned targets remain only as the migration fallback
/// (plan §R9.9).
/// 声明携带完整运行时目标载荷（方案 §R9.2）：AI 可见契约与精确供应商执行契约编译为
/// Manifest 描述持有的 `ProviderToolTarget`。代码持有目标仅保留为迁移期回退（方案 §R9.9）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolTargetDeclaration {
    /// Stable tool-target identifier referenced by operations.
    /// 被操作引用的稳定工具目标标识。
    pub target_id: String,
    /// Human-readable purpose description.
    /// 人类可读的用途描述。
    #[serde(default)]
    pub description: String,
    /// Service entry that exposes this target; empty resolves to the first conversation entry.
    /// 暴露此目标的服务入口；空解析为首个会话入口。
    #[serde(default)]
    pub service_entry_id: String,
    /// Credential profile identifiers allowed to execute this target; empty means every profile.
    /// 允许执行当前目标的凭据配置档标识；空表示全部配置档。
    #[serde(default)]
    pub credential_profile_ids: BTreeSet<String>,
    /// Exact upstream model or service identifier.
    /// 精确的上游模型或服务标识。
    pub upstream_target_id: String,
    /// Optional catalog model ids of which at least one gates this target.
    /// 可选目录模型标识；至少一个存在时才开放当前目标。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub availability_model_ids_any: BTreeSet<String>,
    /// User-visible target name.
    /// 用户可见目标名称。
    pub display_name: String,
    /// Exact AI-visible tool id exposed to the model.
    /// 暴露给模型的精确 AI 可见工具标识。
    pub tool_id: String,
    /// Exact AI-visible semantic capability.
    /// 精确 AI 可见语义能力。
    pub capability: ProviderToolCapabilityKind,
    /// Exact AI-visible input JSON Schema.
    /// 精确 AI 可见输入 JSON Schema。
    pub input_schema: Value,
    /// Host-only bounded product assertions kept outside the AI-visible JSON Schema.
    /// 位于 AI 可见 JSON Schema 之外的仅宿主可见有界乘积断言。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_numeric_product_ranges: Vec<crate::ProviderToolNumericProductRange>,
    /// Required model-discovery profile for a stable model-extension target.
    /// 稳定模型扩展目标必需的模型查询说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_profile: Option<crate::ProviderToolExtensionProfile>,
    /// Exact output contract.
    /// 精确输出契约。
    pub output: ProviderToolOutputContract,
    /// Exact provider execution contract.
    /// 精确供应商执行契约。
    pub execution: ProviderToolExecutionContract,
}

/// One immutable preauthorized provider-tool target template.
/// 一条不可变的预授权供应商工具目标模板。
///
/// A catalog model may only supply its identity and display name. Every request schema, output
/// decoder, endpoint, authentication rule, header, and execution primitive remains bundled data.
/// 目录模型只能提供自身标识与显示名称；全部请求 Schema、输出解码、端点、鉴权规则、请求头
/// 与执行原语仍由内置数据持有。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolTargetTemplateDeclaration {
    /// Stable bundled template identifier referenced by models and one owning operation.
    /// 被模型与唯一所属操作引用的稳定内置模板标识。
    pub template_id: String,
    /// Human-readable purpose description inherited by every instance.
    /// 每个实例继承的人类可读用途描述。
    #[serde(default)]
    pub description: String,
    /// User-visible suffix appended to the catalog model display name.
    /// 追加到目录模型显示名称后的用户可见后缀。
    #[serde(default)]
    pub display_name_suffix: String,
    /// Service entries authorized to instantiate this template.
    /// 被授权实例化此模板的服务入口。
    pub allowed_service_entry_ids: BTreeSet<String>,
    /// Credential profile identifiers allowed to execute instances; empty means every profile.
    /// 允许执行实例的凭据配置档标识；空表示全部配置档。
    #[serde(default)]
    pub credential_profile_ids: BTreeSet<String>,
    /// Exact AI-visible tool id exposed to the model.
    /// 暴露给模型的精确 AI 可见工具标识。
    pub tool_id: String,
    /// Exact AI-visible semantic capability.
    /// 精确 AI 可见语义能力。
    pub capability: ProviderToolCapabilityKind,
    /// Exact AI-visible input JSON Schema.
    /// 精确 AI 可见输入 JSON Schema。
    pub input_schema: Value,
    /// Host-only bounded product assertions kept outside the AI-visible JSON Schema.
    /// 位于 AI 可见 JSON Schema 之外的仅宿主可见有界乘积断言。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_numeric_product_ranges: Vec<crate::ProviderToolNumericProductRange>,
    /// Required model-discovery profile inherited by every model-extension instance.
    /// 每个模型扩展实例继承的必需模型查询说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_profile: Option<crate::ProviderToolExtensionProfile>,
    /// Exact output contract inherited by every instance.
    /// 每个实例继承的精确输出契约。
    pub output: ProviderToolOutputContract,
    /// Exact execution authority inherited by every instance.
    /// 每个实例继承的精确执行权限。
    pub execution: ProviderToolExecutionContract,
}

/// One data-only model binding to one or more immutable target templates.
/// 一条把模型纯数据绑定到一个或多个不可变目标模板的声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolTargetTemplateBindingDeclaration {
    /// Stable service entry that owns the upstream model.
    /// 持有上游模型的稳定服务入口。
    pub service_entry_id: String,
    /// Exact upstream model identifier inserted only into reviewed template slots.
    /// 仅插入已审核模板槽位的精确上游模型标识。
    pub model_id: String,
    /// User-visible model name used by instantiated targets.
    /// 实例化目标使用的用户可见模型名称。
    pub display_name: String,
    /// Immutable template identifiers instantiated for this exact model.
    /// 为此精确模型实例化的不可变模板标识。
    pub template_refs: Vec<String>,
}

/// Source, evidence, and verification provenance for one manifest.
/// 单份 Manifest 的来源、证据与验证溯源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestProvenance {
    /// Verification status of this manifest document.
    /// 此清单文档的验证状态。
    pub status: ProvenanceStatus,
    /// Ordered evidence sources consulted when writing this manifest.
    /// 编写此清单时参考的有序证据来源。
    #[serde(default)]
    pub sources: Vec<ProvenanceSource>,
    /// Optional UTC verification timestamp.
    /// 可选的 UTC 验证时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
}

/// Verification status of one manifest document.
/// 单份清单文档的验证状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceStatus {
    /// Document exists only as a structural example.
    /// 文档仅作为结构示例存在。
    ExampleOnly,
    /// Document mirrors real provider facts with evidence.
    /// 文档以证据镜像真实供应商事实。
    Verified,
    /// Document is derived from migrated legacy configuration.
    /// 文档由旧版配置迁移推导。
    Migrated,
}

/// One evidence source consulted when writing the manifest.
/// 编写清单时参考的单个证据来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceSource {
    /// Official documentation or fixture URL.
    /// 官方文档或 fixture URL。
    pub url: String,
    /// Short note describing what this source establishes.
    /// 描述此来源确立内容的简短说明。
    #[serde(default)]
    pub note: String,
}
