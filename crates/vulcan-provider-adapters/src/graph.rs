//! Provider connection graph contracts.
//! 供应商连接图契约。

use crate::OAuthProviderKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{InferenceUsageScope, ModelCapabilities, PromptCachePolicy};
use vulcan_llm_core::UsageExtractionProfile;

/// Identifies one credential acquisition mode.
/// 标识一种凭据获取方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialMode {
    /// Renewable account authorization.
    /// 可续期账号授权。
    Auth,
    /// User-managed API token.
    /// 用户管理的 API Token。
    Token,
}

/// Identifies the billing semantics attached to one credential profile.
/// 标识一个凭据配置档关联的计费语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialBillingKind {
    /// Provider-metered pay-as-you-go billing.
    /// 供应商按量计费。
    PayAsYouGo,
    /// Prepaid or periodic Token Plan billing.
    /// 预付费或周期性的 Token Plan 计费。
    TokenPlan,
    /// Provider-specific billing without a stronger local classification.
    /// 未在本地进一步分类的供应商专属计费。
    ProviderManaged,
}

/// Describes one exact credential acquisition and billing profile.
/// 描述一种精确的凭据获取及计费配置档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialProfile {
    /// Stable profile identifier scoped to one service entry.
    /// 作用域限定在服务入口内的稳定配置档标识。
    pub id: String,
    /// User-visible profile name.
    /// 用户可见的配置档名称。
    pub display_name: String,
    /// Exact credential acquisition mode.
    /// 精确的凭据获取方式。
    pub credential_mode: CredentialMode,
    /// Exact OAuth flow for renewable credentials.
    /// 可续期凭据使用的精确 OAuth 流程。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_provider: Option<OAuthProviderKind>,
    /// Exact billing semantics for this credential.
    /// 当前凭据的精确计费语义。
    pub billing_kind: CredentialBillingKind,
    /// Credential-scoped account usage query kind.
    /// 凭据作用域的账号用量查询类别。
    pub account_usage_query: AccountUsageQueryKind,
    /// Optional credential-specific authentication override.
    /// 可选的凭据专属鉴权覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationProjection>,
    /// Required literal key prefix for token credentials when declared.
    /// Token 凭据声明时要求的字面 Key 前缀。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_prefix: Option<String>,
    /// Usage scopes permitted by the exact provider product contract.
    /// 精确供应商产品契约允许的使用场景。
    pub allowed_usage_scopes: BTreeSet<InferenceUsageScope>,
}

/// Describes one provider service entry.
/// 描述一个供应商服务入口。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// Stable entry identifier.
    /// 稳定入口标识。
    pub id: String,
    /// User-visible name.
    /// 用户可见名称。
    pub display_name: String,
    /// Whether this entry exposes conversation models or provider-native tools only.
    /// 当前入口暴露对话模型，还是仅暴露供应商原生工具。
    #[serde(default)]
    pub kind: ServiceEntryKind,
    /// Base URL.
    /// 基础 URL。
    pub base_url: String,
    /// Exact credential profiles available through this regional or product entry.
    /// 当前区域或产品入口可用的精确凭据配置档。
    pub credential_profiles: Vec<CredentialProfile>,
    /// Exact authentication projection owned by this service entry.
    /// 此服务入口所有的精确鉴权投影。
    pub authentication: AuthenticationProjection,
    /// Trusted non-secret headers added to every request for this entry.
    /// 为此入口每个请求添加的可信非秘密请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub request_headers: BTreeMap<String, String>,
    /// Exact model-discovery implementation for this entry.
    /// 此入口使用的精确模型发现实现。
    pub model_discovery: ModelDiscoveryKind,
    /// Exact registered request projector for this provider product entry.
    /// 当前供应商产品入口使用的精确已注册请求投影器。
    pub request_projector: RequestProjectorKind,
    /// Verified protocol routes available through this entry.
    /// 此入口可用的已验证协议路由。
    pub protocol_routes: Vec<ProtocolRoute>,
    /// Usage scopes permitted by this exact product or regional service entry.
    /// 当前精确产品或区域服务入口允许的使用场景。
    pub allowed_usage_scopes: BTreeSet<InferenceUsageScope>,
}

/// Identifies one closed provider request projector implementation.
/// 标识一个封闭的供应商请求投影器实现。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestProjectorKind {
    /// No provider-only mutation beyond the selected wire protocol.
    /// 除所选 Wire 协议外不执行供应商专属变更。
    Direct,
    /// MiniMax regional API and Token Plan projection.
    /// MiniMax 区域 API 与 Token Plan 投影。
    MiniMax,
    /// OpenRouter routing and reasoning projection.
    /// OpenRouter 路由与思考投影。
    OpenRouter,
    /// Kimi public API and Coding Plan projection.
    /// Kimi 开放平台与 Coding Plan 投影。
    Kimi,
    /// DeepSeek official API projection.
    /// DeepSeek 官方 API 投影。
    DeepSeek,
    /// Alibaba Token Plan and Model Studio projection.
    /// 阿里云 Token Plan 与百炼投影。
    Alibaba,
    /// Google public API and Code Assist projection.
    /// Google 公共 API 与 Code Assist 投影。
    Google,
}

/// Identifies the execution surface owned by one service entry.
/// 标识一个服务入口持有的执行界面。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceEntryKind {
    /// The entry exposes conversation models and wire protocols.
    /// 当前入口暴露对话模型与 Wire 协议。
    #[default]
    Conversation,
    /// The entry exposes provider-native AI tools without a chat model.
    /// 当前入口暴露不依赖聊天模型的供应商原生 AI 工具。
    ProviderTools,
}

impl ServiceEntry {
    /// Resolves one exact credential profile by its stable identifier.
    /// 按稳定标识解析一个精确凭据配置档。
    ///
    /// # Parameters
    /// 参数
    /// `profile_id` is the service-entry-scoped credential profile identifier.
    /// `profile_id` 是服务入口作用域内的凭据配置档标识。
    ///
    /// # Returns
    /// 返回值
    /// The declared profile when it exists.
    /// 配置档存在时返回其声明。
    pub fn credential_profile(&self, profile_id: &str) -> Option<&CredentialProfile> {
        self.credential_profiles
            .iter()
            .find(|profile| profile.id == profile_id)
    }

    /// Resolves the effective usage scopes for one exact credential connection.
    /// 解析一个精确凭据连接的有效使用场景。
    ///
    /// # Parameters
    /// 参数
    /// `connection` must bind this service entry and one declared credential profile.
    /// `connection` 必须绑定当前服务入口及其一个已声明凭据配置档。
    ///
    /// # Returns
    /// 返回值
    /// The product/profile intersection narrowed by the optional user override.
    /// 返回产品与配置档交集，并由可选用户覆盖进一步收窄。
    pub fn effective_usage_scopes(
        &self,
        connection: &CredentialConnection,
    ) -> Result<BTreeSet<InferenceUsageScope>, String> {
        if connection.service_entry_id != self.id {
            return Err(format!(
                "Connection `{}` does not bind service entry `{}`.",
                connection.id, self.id
            ));
        }
        let profile = self
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, self.id
                )
            })?;
        let product_scopes: BTreeSet<_> = profile
            .allowed_usage_scopes
            .intersection(&self.allowed_usage_scopes)
            .copied()
            .collect();
        let Some(override_scopes) = connection.usage_scope_override.as_ref() else {
            return Ok(product_scopes);
        };
        if !override_scopes.is_subset(&product_scopes) {
            return Err(format!(
                "Connection `{}` usage-scope override exceeds product `{}` permissions.",
                connection.id, self.id
            ));
        }
        Ok(override_scopes.clone())
    }
}

/// Identifies one registered model-discovery contract.
/// 标识一个已注册模型发现契约。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDiscoveryKind {
    /// User-managed models without a remote catalog request.
    /// 不请求远程目录、完全由用户管理的模型。
    Manual,
    /// No conversation-model discovery exists for provider-tools-only entries.
    /// 仅供应商工具入口不存在对话模型发现。
    ProviderToolsOnly,
    /// MiniMax Anthropic-compatible paginated models.
    /// MiniMax Anthropic 兼容分页模型。
    MiniMaxAnthropic,
    /// MiniMax OpenAI-compatible model list.
    /// MiniMax OpenAI 兼容模型列表。
    MiniMaxOpenAi,
    /// OpenRouter public model catalog.
    /// OpenRouter 公共模型目录。
    OpenRouter,
    /// Alibaba Coding Plan code-owned static catalog.
    /// 阿里云 Coding Plan 代码持有的静态目录。
    AlibabaCodingPlanStatic,
    /// Alibaba Token Plan product-scoped static catalog.
    /// 阿里云 Token Plan 产品作用域静态目录。
    AlibabaTokenPlanStatic,
    /// Alibaba Model Studio region-scoped static catalog.
    /// 阿里云百炼 Model Studio 区域作用域静态目录。
    AlibabaModelStudioStatic,
    /// OpenAI public models endpoint.
    /// OpenAI 公共模型端点。
    OpenAi,
    /// OpenAI Codex subscription model catalog.
    /// OpenAI Codex 订阅模型目录。
    OpenAiCodex,
    /// Anthropic public paginated models endpoint.
    /// Anthropic 公共分页模型端点。
    Anthropic,
    /// Anthropic-compatible models endpoint relative to a versioned API base URL.
    /// 相对于带版本 API 基础 URL 的 Anthropic 兼容模型端点。
    AnthropicCompatible,
    /// Google Gemini public paginated models endpoint.
    /// Google Gemini 公共分页模型端点。
    Gemini,
    /// Google Code Assist credential-scoped model catalog.
    /// Google Code Assist 凭据作用域模型目录。
    GoogleCodeAssist,
}

/// Identifies one exact service-entry authentication projection.
/// 标识一种精确服务入口鉴权投影。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationProjection {
    /// Sends the resolved value through `x-api-key`.
    /// 通过 `x-api-key` 发送解析值。
    XApiKey,
    /// Sends the resolved value through `Authorization: Bearer`.
    /// 通过 `Authorization: Bearer` 发送解析值。
    BearerAuthorization,
    /// Sends Codex bearer authorization and its verified account identity.
    /// 发送 Codex Bearer 授权及其已验证账号身份。
    OpenAiCodexBearer,
    /// Sends Google Code Assist bearer authorization and optional quota project.
    /// 发送 Google Code Assist Bearer 授权及可选配额项目。
    GoogleCodeAssistBearer,
    /// Sends public Gemini OAuth bearer authorization and its required billing project.
    /// 发送公共 Gemini OAuth Bearer 授权及其必需的计费项目。
    GoogleGeminiBearer,
    /// Sends the resolved value through `x-goog-api-key`.
    /// 通过 `x-goog-api-key` 发送解析值。
    GoogleApiKey,
}

/// Identifies one registered account-usage query implementation.
/// 标识一个已注册账号用量查询实现。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountUsageQueryKind {
    /// Account usage is not exposed by this entry.
    /// 此入口不公开账号用量。
    Unsupported,
    /// MiniMax Token Plan quota endpoint.
    /// MiniMax Token Plan 配额端点。
    MiniMaxTokenPlan,
    /// OpenRouter account credits endpoint using an optional management key.
    /// 使用可选管理密钥的 OpenRouter 账号余额端点。
    OpenRouterCredits,
    /// Tavily API-key and account credit usage endpoint.
    /// Tavily API Key 与账号 Credit 用量端点。
    TavilyCredits,
    /// DeepSeek official account balance endpoint.
    /// DeepSeek 官方账号余额端点。
    DeepSeekBalance,
    /// Kimi Coding Plan membership and rolling usage endpoint.
    /// Kimi Coding Plan 会员与滚动用量端点。
    KimiCodingPlan,
}

/// Selects whether one Responses route may use the standalone remote compact endpoint.
/// 选择一条 Responses 路由是否可以使用独立远端压缩端点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResponsesRemoteCompactionPolicy {
    /// Keeps local prompt-based compaction as the only compaction path.
    /// 仅使用本地提示词压缩路径。
    #[default]
    Disabled,
    /// Uses remote compaction and falls back to local compaction when safe.
    /// 使用远端压缩，并在安全时回退到本地压缩。
    EnabledWithLocalFallback,
}

/// Selects the explicitly verified Responses WebSocket capability for one route.
/// 选择一条路由显式验证的 Responses WebSocket 能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResponsesWebsocketPolicy {
    /// Keeps HTTP/SSE as the only allowed transport.
    /// 仅允许 HTTP/SSE 传输。
    #[default]
    Disabled,
    /// Enables WebSocket because this exact route has passed real verification.
    /// 当前精确路由已通过真实验证，因此启用 WebSocket。
    Verified,
    /// Enables an advanced user override while retaining unverified telemetry status.
    /// 启用高级用户覆盖，同时在遥测中保留未验证状态。
    #[serde(alias = "preferred_with_http_fallback")]
    UnverifiedOverride,
}

/// Carries explicit optional capabilities for one OpenAI Responses route.
/// 承载一条 OpenAI Responses 路由的显式可选能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResponsesRouteFeatures {
    /// Standalone remote compaction selection.
    /// 独立远端压缩选择。
    #[serde(default)]
    pub remote_compaction: ResponsesRemoteCompactionPolicy,
    /// Responses WebSocket capability for this exact route.
    /// 当前精确路由的 Responses WebSocket 能力。
    #[serde(default)]
    pub websocket: ResponsesWebsocketPolicy,
}

/// Describes one verified protocol route.
/// 描述一条已验证协议路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolRoute {
    /// Stable protocol profile identifier.
    /// 稳定协议 Profile 标识。
    pub protocol_profile_id: String,
    /// Optional provider endpoint override; absent values retain the protocol-encoded path.
    /// 可选供应商端点覆盖；缺失时保留协议编码路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_path: Option<String>,
    /// Optional protocol-specific base URL used when one product exposes protocols on different roots.
    /// 当一个产品在不同根地址暴露协议时使用的可选协议专属基础 URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url_override: Option<String>,
    /// Fixed non-secret headers scoped only to this protocol route.
    /// 仅作用于当前协议路由的固定非敏感请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub request_headers: BTreeMap<String, String>,
    /// Optional route-specific authentication projection.
    /// 可选的路由专属鉴权投影。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationProjection>,
    /// Optional route-specific model-discovery implementation.
    /// 可选的路由专属模型发现实现。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery: Option<ModelDiscoveryKind>,
    /// Optional protocol-specific model-discovery path relative to the resolved base URL.
    /// 相对于已解析基础 URL 的可选协议专属模型发现路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery_path: Option<String>,
    /// Optional model-discovery base URL when metadata and execution use different protocol roots.
    /// 当元数据发现与执行使用不同协议根地址时采用的可选模型发现基础 URL。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery_base_url_override: Option<String>,
    /// Optional user-declared Responses transport and compaction capabilities.
    /// 可选的用户声明 Responses 传输与压缩能力。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responses_features: Option<ResponsesRouteFeatures>,
    /// Versioned raw usage extraction profiles frozen with this route.
    /// 随当前路由冻结的版本化原始用量提取 Profile。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usage_extraction_profiles: Vec<UsageExtractionProfile>,
    /// Ordered response-header names that may carry the upstream provider request identifier.
    /// 可承载上游供应商请求标识的有序响应头名称。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_request_id_headers: Vec<String>,
    /// Whether this route has completed real verification.
    /// 此路由是否已完成真实验证。
    pub verified: bool,
}

impl ProtocolRoute {
    /// Reports whether this exact route explicitly enables Responses WebSocket transport.
    /// 报告当前精确路由是否显式启用 Responses WebSocket 传输。
    pub fn responses_websocket_enabled(&self) -> bool {
        self.responses_features.as_ref().is_some_and(|features| {
            matches!(
                features.websocket,
                ResponsesWebsocketPolicy::Verified | ResponsesWebsocketPolicy::UnverifiedOverride
            )
        })
    }

    /// Reports whether this exact route explicitly enables standalone remote Responses compaction.
    /// 报告当前精确路由是否显式启用独立远端 Responses 压缩。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for an explicit enabled-with-safe-local-fallback policy.
    /// 仅在显式选择启用并允许安全本地回退策略时返回 `true`。
    pub fn remote_compaction_enabled(&self) -> bool {
        self.responses_features.as_ref().is_some_and(|features| {
            features.remote_compaction == ResponsesRemoteCompactionPolicy::EnabledWithLocalFallback
        })
    }

    /// Resolves the protocol-specific base URL or the service-entry default.
    /// 解析协议专属基础 URL，缺失时返回服务入口默认值。
    ///
    /// # Parameters
    /// 参数
    /// `entry` is the exact service entry owning this protocol route.
    /// `entry` 是拥有当前协议路由的精确服务入口。
    ///
    /// # Returns
    /// 返回值
    /// The exact base URL used for request and metadata routing.
    /// 返回请求与元数据路由使用的精确基础 URL。
    pub fn resolved_base_url<'a>(&'a self, entry: &'a ServiceEntry) -> &'a str {
        self.base_url_override
            .as_deref()
            .unwrap_or(entry.base_url.as_str())
    }

    /// Builds the exact model-discovery URL for this protocol route.
    /// 为当前协议路由构建精确模型发现 URL。
    ///
    /// # Parameters
    /// 参数
    /// `entry` owns the default base URL and `default_path` belongs to the discovery implementation.
    /// `entry` 持有默认基础 URL，`default_path` 属于发现实现。
    ///
    /// # Returns
    /// 返回值
    /// The complete discovery URL.
    /// 返回完整发现 URL。
    pub fn model_discovery_url(&self, entry: &ServiceEntry, default_path: &str) -> String {
        let path = self.model_discovery_path.as_deref().unwrap_or(default_path);
        let base_url = self
            .model_discovery_base_url_override
            .as_deref()
            .unwrap_or_else(|| self.resolved_base_url(entry));
        format!(
            "{}{}",
            base_url.trim_end_matches('/'),
            if path.starts_with('/') {
                path.to_string()
            } else {
                format!("/{path}")
            }
        )
    }

    /// Resolves the exact authentication projection for this protocol route.
    /// 解析当前协议路由的精确鉴权投影。
    ///
    /// # Parameters
    /// 参数
    /// `entry_default` is the service-entry projection and `credential_override` is the selected
    /// credential profile projection used when the route itself has no override.
    /// `entry_default` 是服务入口投影，`credential_override` 是路由自身未覆盖时使用的所选
    /// 凭据配置档投影。
    ///
    /// # Returns
    /// 返回值
    /// The route override or the exact service-entry default.
    /// 返回路由覆盖值或精确的服务入口默认值。
    pub fn resolved_authentication(
        &self,
        entry_default: AuthenticationProjection,
        credential_override: Option<AuthenticationProjection>,
    ) -> AuthenticationProjection {
        self.authentication
            .or(credential_override)
            .unwrap_or(entry_default)
    }

    /// Resolves the exact model-discovery implementation for this protocol route.
    /// 解析当前协议路由的精确模型发现实现。
    ///
    /// # Parameters
    /// 参数
    /// `entry_default` is the service-entry discovery kind used when the route has no override.
    /// `entry_default` 是路由未覆盖时使用的服务入口发现类别。
    ///
    /// # Returns
    /// 返回值
    /// The route override or the exact service-entry default.
    /// 返回路由覆盖值或精确的服务入口默认值。
    pub fn resolved_model_discovery(
        &self,
        entry_default: ModelDiscoveryKind,
    ) -> ModelDiscoveryKind {
        self.model_discovery.unwrap_or(entry_default)
    }
}

/// Stores one user credential connection without secret material.
/// 保存一个不含秘密材料的用户凭据连接。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialConnection {
    /// Stable connection identifier.
    /// 稳定连接标识。
    pub id: String,
    /// Provider identifier.
    /// 供应商标识。
    pub provider_id: String,
    /// Service entry identifier.
    /// 服务入口标识。
    pub service_entry_id: String,
    /// Credential profile identifier scoped to the selected service entry.
    /// 作用域限定在所选服务入口内的凭据配置档标识。
    pub credential_profile_id: String,
    /// Credential mode.
    /// 凭据方式。
    pub credential_mode: CredentialMode,
    /// Stable credential identifier.
    /// 稳定凭据标识。
    pub credential_id: String,
    /// User-visible credential name.
    /// 用户可见凭据名称。
    pub display_name: String,
    /// Secret-store reference.
    /// 秘密存储引用。
    pub secret_reference: String,
    /// Optional secret-store reference used only for provider account management queries.
    /// 仅用于供应商账号管理查询的可选秘密存储引用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub management_secret_reference: Option<String>,
    /// Optional credential-scoped service origin for a provider-defined configurable entry.
    /// 供应商声明可配置入口使用的可选凭据作用域服务源地址。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_origin_override: Option<String>,
    /// Verified protocol profile identifiers.
    /// 已验证协议 Profile 标识。
    pub verified_protocols: BTreeSet<String>,
    /// Currently selected protocol profile.
    /// 当前选定协议 Profile。
    pub selected_protocol: String,
    /// Protocol-scoped prompt-cache settings.
    /// 协议作用域的提示词缓存设置。
    #[serde(default)]
    pub prompt_cache_by_protocol: BTreeMap<String, PromptCachePolicy>,
    /// Optional user narrowing of the credential-profile usage scopes.
    /// 用户对凭据配置档使用场景的可选收窄。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_scope_override: Option<BTreeSet<InferenceUsageScope>>,
}

/// Maximum models accepted in one credential-scoped runtime catalog.
/// 单个凭据作用域运行时目录允许接受的最大模型数量。
pub const PROVIDER_MODEL_CATALOG_MAX_MODELS: usize = 20_000;

/// Maximum UTF-8 bytes accepted for one provider model identifier.
/// 单个供应商模型标识允许接受的最大 UTF-8 字节数。
pub const PROVIDER_MODEL_IDENTIFIER_MAX_BYTES: usize = 2_048;

/// Maximum UTF-8 bytes accepted for one provider model display name.
/// 单个供应商模型显示名称允许接受的最大 UTF-8 字节数。
pub const PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES: usize = 4_096;

/// Describes one connection-scoped model.
/// 描述一个连接作用域模型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionModel {
    /// Exact model identifier.
    /// 精确模型标识。
    pub id: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    pub display_name: String,
    /// Resolved model capabilities.
    /// 解析后的模型能力。
    pub capabilities: ModelCapabilities,
    /// Field-presence evidence captured directly from this credential's model directory.
    /// 直接从当前凭据模型目录捕获的字段存在性证据。
    #[serde(default)]
    pub capability_evidence: ModelCapabilityEvidence,
}

/// Records which basic capability fields were explicitly declared by one model directory.
/// 记录一个模型目录明确声明了哪些基础能力字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ModelCapabilityEvidence {
    /// Whether the directory explicitly declared a context limit.
    /// 目录是否明确声明了上下文上限。
    #[serde(default)]
    pub context_limit_declared: bool,
    /// Whether the directory explicitly declared an output-token limit.
    /// 目录是否明确声明了输出 Token 上限。
    #[serde(default)]
    pub output_limit_declared: bool,
    /// Whether the directory explicitly declared input modalities.
    /// 目录是否明确声明了输入模态。
    #[serde(default)]
    pub input_modalities_declared: bool,
    /// Whether the directory explicitly declared output modalities.
    /// 目录是否明确声明了输出模态。
    #[serde(default)]
    pub output_modalities_declared: bool,
    /// Explicit text-input support, or `None` when the directory omitted input modalities.
    /// 明确的文本输入支持；目录未声明输入模态时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_text_input: Option<bool>,
    /// Explicit text-output support, or `None` when the directory omitted output modalities.
    /// 明确的文本输出支持；目录未声明输出模态时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_text_output: Option<bool>,
}

/// Stores a connection-scoped in-memory model catalog.
/// 保存连接作用域的内存模型目录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionModelCatalog {
    /// Owning connection identifier.
    /// 所属连接标识。
    pub connection_id: String,
    /// Models returned for the exact credential.
    /// 针对该精确凭据返回的模型。
    pub models: Vec<ConnectionModel>,
    /// Monotonic in-process refresh generation.
    /// 进程内单调刷新代次。
    pub generation: u64,
    /// Number of provider pages atomically represented by this catalog.
    /// 此目录原子表示的供应商分页数量。
    pub page_count: u32,
    /// Exact parser contract revision used for the source response.
    /// 源响应所使用的精确解析器契约修订号。
    pub discovery_schema_revision: String,
}

/// Identifies one semantic provider tool capability.
/// 标识一种语义化供应商工具能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderToolCapabilityKind {
    /// Searches public web content.
    /// 搜索公开网页内容。
    WebSearch,
    /// Extracts structured content from public web pages.
    /// 从公开网页提取结构化内容。
    WebExtract,
    /// Analyzes one or more media resources.
    /// 分析一个或多个媒体资源。
    MediaAnalyze,
    /// Generates images.
    /// 生成图片。
    ImageGenerate,
    /// Edits images.
    /// 编辑图片。
    ImageEdit,
    /// Generates video.
    /// 生成视频。
    VideoGenerate,
    /// Edits an existing video.
    /// 编辑现有视频。
    VideoEdit,
    /// Synthesizes speech.
    /// 合成语音。
    SpeechSynthesize,
    /// Transcribes speech or audio.
    /// 转录语音或音频。
    SpeechTranscribe,
    /// Generates music.
    /// 生成音乐。
    MusicGenerate,
    /// Produces a music cover from an input recording.
    /// 根据输入录音生成音乐翻唱。
    MusicCover,
    /// Retired migration sentinel for historical AI-visible embedding selections.
    /// 历史 AI 可见向量选择使用的已退役迁移哨兵。
    EmbeddingCreate,
    /// Retired migration sentinel for historical AI-visible rerank selections.
    /// 历史 AI 可见重排选择使用的已退役迁移哨兵。
    RerankDocuments,
}

impl ProviderToolCapabilityKind {
    /// Reports whether this capability may produce an AI-visible provider tool.
    /// 报告此能力是否可以生成 AI 可见供应商工具。
    ///
    /// # Returns
    /// 返回值
    /// `false` for retired embedding and rerank migration sentinels.
    /// 对已退役向量与重排迁移哨兵返回 `false`。
    pub const fn is_ai_visible(self) -> bool {
        !matches!(self, Self::EmbeddingCreate | Self::RerankDocuments)
    }
}

/// Identifies the closed unified output shape used by provider tool cards.
/// 标识供应商工具卡片使用的封闭统一输出形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderToolOutputKind {
    /// Plain text output.
    /// 纯文本输出。
    Text,
    /// Downloaded or remotely referenced resources.
    /// 已下载或远程引用的资源。
    Resources,
    /// Embedding vectors.
    /// 向量嵌入结果。
    Embedding,
    /// Ranked candidates.
    /// 排序后的候选项。
    Rerank,
    /// Structured search evidence.
    /// 结构化搜索证据。
    Search,
    /// Structured extracted pages.
    /// 结构化网页提取结果。
    Extract,
    /// A bounded combination of typed outputs.
    /// 有界的多种强类型输出组合。
    Composite,
}

/// Describes how one provider-native response is normalized for model output and tool cards.
/// 描述如何将一个供应商原生响应归一化为模型输出与工具卡片数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolOutputContract {
    /// Closed normalized output kind.
    /// 封闭的归一化输出类别。
    pub kind: ProviderToolOutputKind,
    /// Exact provider-response path consumed by the adapter.
    /// 适配器消费的精确供应商响应路径。
    pub source_path: String,
    /// Stable normalized result field exposed to the model and card renderer.
    /// 暴露给模型与卡片渲染器的稳定归一化结果字段。
    pub normalized_result_path: String,
    /// Optional exact resource kind for resource outputs.
    /// 资源输出使用的可选精确资源类别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_kind: Option<String>,
    /// Optional exact MIME types accepted from the provider response.
    /// 供应商响应允许的可选精确 MIME 类型。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mime_types: Vec<String>,
}

/// Declares one exact AI-visible tool contract owned by a provider model or service.
/// 声明由一个供应商模型或服务持有的精确 AI 可见工具契约。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolContract {
    /// Stable public tool identifier.
    /// 稳定公开工具标识。
    pub tool_id: String,
    /// English model-facing tool description.
    /// 面向模型的英文工具说明。
    pub description: String,
    /// Semantic capability controlled by the single-target selector.
    /// 由单目标选择器控制的语义能力。
    pub capability: ProviderToolCapabilityKind,
    /// Exact model-native JSON Schema registered for this selected target.
    /// 为当前选定目标注册的精确模型原生 JSON Schema。
    pub input_schema: Value,
    /// Closed provider-response normalization contract.
    /// 封闭的供应商响应归一化契约。
    pub output: ProviderToolOutputContract,
}

/// Identifies one closed provider-native tool request/response adapter.
/// 标识一种封闭的供应商原生工具请求与响应适配器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderToolAdapterKind {
    /// MiniMax synchronous image generation.
    /// MiniMax 同步图片生成。
    MiniMaxImageGenerate,
    /// MiniMax Coding Plan image understanding.
    /// MiniMax Coding Plan 图片理解。
    MiniMaxCodingPlanVision,
    /// MiniMax synchronous speech synthesis.
    /// MiniMax 同步语音合成。
    MiniMaxSpeechSynthesize,
    /// MiniMax synchronous music generation.
    /// MiniMax 同步音乐生成。
    MiniMaxMusicGenerate,
    /// MiniMax asynchronous video generation with host polling.
    /// MiniMax 异步视频生成与宿主轮询。
    MiniMaxVideoGenerate,
    /// MiniMax Coding Plan structured web search.
    /// MiniMax Coding Plan 结构化网页搜索。
    MiniMaxCodingPlanSearch,
    /// Alibaba Model Studio MCP WebSearch.
    /// 阿里云百炼 Model Studio MCP 网页搜索。
    AlibabaModelStudioSearch,
    /// Alibaba synchronous Qwen image generation.
    /// 阿里云同步 Qwen 图片生成。
    AlibabaQwenImageGenerate,
    /// Alibaba synchronous Qwen image editing.
    /// 阿里云同步 Qwen 图片编辑。
    AlibabaQwenImageEdit,
    /// Alibaba synchronous Wan image generation.
    /// 阿里云同步 Wan 图片生成。
    AlibabaWanImageGenerate,
    /// Alibaba synchronous CosyVoice speech synthesis.
    /// 阿里云同步 CosyVoice 语音合成。
    AlibabaCosyVoiceSynthesize,
    /// Alibaba synchronous Qwen3 speech synthesis.
    /// 阿里云同步 Qwen3 语音合成。
    AlibabaQwenSpeechSynthesize,
    /// Alibaba synchronous Qwen3 speech transcription.
    /// 阿里云同步 Qwen3 语音转录。
    AlibabaQwenSpeechTranscribe,
    /// Alibaba asynchronous Fun-ASR transcription with host polling.
    /// 阿里云异步 Fun-ASR 转录与宿主轮询。
    AlibabaFunAsrTranscribe,
    /// Alibaba Qwen Omni media analysis through the verified Chat carrier.
    /// 阿里云 Qwen Omni 通过已验证 Chat 载体执行媒体分析。
    AlibabaQwenOmniMediaAnalyze,
    /// Alibaba asynchronous HappyHorse video generation with host polling.
    /// 阿里云异步 HappyHorse 视频生成与宿主轮询。
    AlibabaHappyHorseVideoGenerate,
    /// Alibaba asynchronous HappyHorse video editing with host polling.
    /// 阿里云异步 HappyHorse 视频编辑与宿主轮询。
    AlibabaHappyHorseVideoEdit,
    /// Alibaba Wan 2.7 asynchronous video generation with host polling.
    /// 阿里云 Wan 2.7 异步视频生成与宿主轮询。
    AlibabaWanVideoGenerate,
    /// Alibaba synchronous Fun-Music generation.
    /// 阿里云同步 Fun-Music 音乐生成。
    AlibabaFunMusicGenerate,
    /// Tavily structured web search.
    /// Tavily 结构化网页搜索。
    TavilySearch,
    /// Tavily structured web extraction.
    /// Tavily 结构化网页提取。
    TavilyExtract,
}

/// Describes the exact execution route for one provider-native tool target.
/// 描述一个供应商原生工具目标的精确执行路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExecutionContract {
    /// Optional exact native API origin when it differs from the conversation protocol root.
    /// 当原生 API 源与会话协议根地址不同时使用的可选精确源地址。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_base_url: Option<String>,
    /// Relative endpoint path below the owning service entry.
    /// 所属服务入口下的相对端点路径。
    pub endpoint_path: String,
    /// Exact authentication projection for this provider tool route.
    /// 当前供应商工具路由的精确鉴权投影。
    pub authentication: AuthenticationProjection,
    /// Closed request and response adapter implementation.
    /// 封闭的请求与响应适配器实现。
    pub adapter: ProviderToolAdapterKind,
}

/// Describes one provider tool target without merging it with other model contracts.
/// 描述一个供应商工具目标，且不会把它与其他模型契约合并。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolTarget {
    /// Stable target identifier scoped to one provider.
    /// 作用域限定在一个供应商内的稳定目标标识。
    pub target_id: String,
    /// Provider identifier.
    /// 供应商标识。
    pub provider_id: String,
    /// Service-entry identifier.
    /// 服务入口标识。
    pub service_entry_id: String,
    /// Credential profile identifiers allowed to execute this target.
    /// 允许执行当前目标的凭据配置档标识。
    pub credential_profile_ids: BTreeSet<String>,
    /// Exact upstream model or service identifier.
    /// 精确的上游模型或服务标识。
    pub upstream_target_id: String,
    /// User-visible target name.
    /// 用户可见目标名称。
    pub display_name: String,
    /// Exact AI-visible tool contract for this target only.
    /// 仅属于当前目标的精确 AI 可见工具契约。
    pub contract: ProviderToolContract,
    /// Exact provider execution contract for this target.
    /// 当前目标的精确供应商执行契约。
    pub execution: ProviderToolExecutionContract,
}

/// Stores the one explicit target selected for a semantic capability.
/// 保存一个语义能力唯一显式选中的目标。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProviderToolTargetSelection {
    /// Credential connection used by this capability.
    /// 当前能力使用的凭据连接。
    pub connection_id: String,
    /// Exact provider-owned tool target.
    /// 精确的供应商自有工具目标。
    pub target_id: String,
}

/// Stores account-selection preferences without enabling runtime failover.
/// 保存账号选择偏好且不启用运行时故障转移。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AccountSelectionPreferences {
    /// Default connection per provider.
    /// 每个供应商的默认连接。
    #[serde(default)]
    pub provider_defaults: BTreeMap<String, String>,
    /// Last selected connection per provider and model composite key.
    /// 每个供应商与模型组合键最近选择的连接。
    #[serde(default)]
    pub model_last_connections: BTreeMap<String, String>,
}

impl AccountSelectionPreferences {
    /// Builds one stable provider-model preference key.
    /// 构建一个稳定的供应商模型偏好键。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the provider identifier and `model_id` is the exact model identifier.
    /// `provider_id` 是供应商标识，`model_id` 是精确模型标识。
    ///
    /// # Returns
    /// 返回值
    /// A collision-resistant length-prefixed key.
    /// 一个抗冲突的长度前缀键。
    pub fn model_key(provider_id: &str, model_id: &str) -> String {
        format!(
            "{}:{provider_id}:{}:{model_id}",
            provider_id.len(),
            model_id.len()
        )
    }
}

/// Represents one provider secondary-route selection.
/// 表示一个供应商二级路由选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SecondaryRouteSelection {
    /// Uses provider-managed routing.
    /// 使用供应商管理的路由。
    #[default]
    Automatic,
    /// Pins one exact provider endpoint.
    /// 固定一个精确供应商端点。
    PinnedEndpoint {
        /// Stable endpoint route identifier.
        /// 稳定端点路由标识。
        route_id: String,
        /// Provider display name.
        /// 供应商显示名。
        provider_display_name: String,
        /// Endpoint display name.
        /// 端点显示名。
        endpoint_display_name: String,
    },
}

/// Describes one provider-reported secondary execution endpoint.
/// 描述一个供应商报告的二级执行端点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecondaryRouteOption {
    /// Stable route identifier forwarded to the provider.
    /// 转发给供应商的稳定路由标识。
    pub route_id: String,
    /// Human-readable upstream provider name.
    /// 人类可读的上游供应商名称。
    pub provider_display_name: String,
    /// Human-readable endpoint name.
    /// 人类可读的端点名称。
    pub endpoint_display_name: String,
    /// Exact prompt price per token when reported.
    /// 供应商报告时的精确每 Token 输入价格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_price_per_token: Option<String>,
    /// Exact completion price per token when reported.
    /// 供应商报告时的精确每 Token 输出价格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_price_per_token: Option<String>,
    /// Exact cache-read price per token when reported.
    /// 供应商报告时的精确每 Token 缓存读取价格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_price_per_token: Option<String>,
    /// Recent median latency in milliseconds when reported.
    /// 供应商报告时的近期中位延迟毫秒数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_milliseconds: Option<f64>,
    /// Recent uptime percentage when reported.
    /// 供应商报告时的近期可用性百分比。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_percent: Option<f64>,
    /// Recent throughput in tokens per second when reported.
    /// 供应商报告时的近期每秒 Token 吞吐量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput_tokens_per_second: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the retired preferred-with-fallback value remains explicitly unverified.
    /// 验证已退役的优先 WS 并允许回退值仍保持显式未验证语义。
    #[test]
    fn websocket_policy_reads_legacy_preferred_with_http_fallback() {
        let policy: ResponsesWebsocketPolicy =
            serde_json::from_str(r#""preferred_with_http_fallback""#)
                .expect("legacy websocket policy should deserialize");

        assert_eq!(policy, ResponsesWebsocketPolicy::UnverifiedOverride);
        assert_eq!(
            serde_json::to_string(&policy).expect("websocket policy should serialize"),
            r#""unverified_override""#
        );
    }
}
