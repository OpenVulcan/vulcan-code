//! Provider connection graph contracts.
//! 供应商连接图契约。

use crate::OAuthProviderKind;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{ModelCapabilities, PromptCachePolicy};

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
    /// Verified protocol routes available through this entry.
    /// 此入口可用的已验证协议路由。
    pub protocol_routes: Vec<ProtocolRoute>,
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
}

/// Identifies one registered model-discovery contract.
/// 标识一个已注册模型发现契约。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDiscoveryKind {
    /// MiniMax Anthropic-compatible paginated models.
    /// MiniMax Anthropic 兼容分页模型。
    MiniMaxAnthropic,
    /// MiniMax OpenAI-compatible model list.
    /// MiniMax OpenAI 兼容模型列表。
    MiniMaxOpenAi,
    /// OpenRouter public model catalog.
    /// OpenRouter 公共模型目录。
    OpenRouter,
    /// OpenAI public models endpoint.
    /// OpenAI 公共模型端点。
    OpenAi,
    /// OpenAI Codex subscription model catalog.
    /// OpenAI Codex 订阅模型目录。
    OpenAiCodex,
    /// Anthropic public paginated models endpoint.
    /// Anthropic 公共分页模型端点。
    Anthropic,
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
    /// OpenRouter current-key metadata endpoint.
    /// OpenRouter 当前密钥元数据端点。
    OpenRouterCurrentKey,
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
    /// Optional route-specific authentication projection.
    /// 可选的路由专属鉴权投影。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authentication: Option<AuthenticationProjection>,
    /// Optional route-specific model-discovery implementation.
    /// 可选的路由专属模型发现实现。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_discovery: Option<ModelDiscoveryKind>,
    /// Whether this route has completed real verification.
    /// 此路由是否已完成真实验证。
    pub verified: bool,
}

impl ProtocolRoute {
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
}

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
