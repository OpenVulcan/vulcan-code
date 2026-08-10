//! Provider connection graph contracts.
//! 供应商连接图契约。

use crate::OAuthProviderKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{
    AiOperation, InferenceUsageScope, ModelCapabilities, PromptCachePolicy, RerankTruncation,
    ResourceKind,
};
use vulcan_llm_core::UsageExtractionProfile;

/// Identifies one credential acquisition mode.
/// 标识一种凭据获取方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialMode {
    /// Renewable account authorization.
    /// 可续期账号授权。
    Auth,
    /// User-managed API token.
    /// 用户管理的 API Token。
    #[default]
    Token,
}

/// Identifies the billing semantics attached to one credential profile.
/// 标识一个凭据配置档关联的计费语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
    #[default]
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
    /// Optional declared credential role consumed by upload and secret projection guards.
    /// 可选声明凭据角色，供上传与秘密投影守卫消费。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_role: Option<String>,
    /// Usage scopes permitted by the exact provider product contract.
    /// 精确供应商产品契约允许的使用场景。
    pub allowed_usage_scopes: BTreeSet<InferenceUsageScope>,
}

/// Describes one provider service entry.
/// 描述一个供应商服务入口。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Optional credential-scoped service-origin override contract.
    /// 可选的凭据作用域服务源地址覆盖契约。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_origin_override_policy: Option<ServiceOriginOverridePolicy>,
    /// Closed native inference actions selected by this service entry.
    /// 此服务入口选择的封闭原生推理动作。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_inference_bindings: Vec<NativeInferenceBinding>,
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

/// Declares the bounded host and path contract for one credential-scoped service origin.
/// 声明一个凭据作用域服务源地址的有界主机与路径契约。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceOriginOverridePolicy {
    /// Required DNS suffix following the single workspace label.
    /// 单一工作区标签之后必须匹配的 DNS 后缀。
    pub host_suffix: String,
    /// Accepted input paths that normalize back to the canonical HTTPS origin.
    /// 可被规范化回标准 HTTPS 源地址的输入路径。
    pub allowed_input_paths: BTreeSet<String>,
    /// Protocol-profile-specific path appended to the canonical origin at execution time.
    /// 执行时按协议配置追加到标准源地址的路径。
    pub protocol_base_paths: BTreeMap<String, String>,
    /// Non-secret example shown by the connection editor.
    /// 连接编辑器展示的非敏感示例。
    pub example_origin: String,
}

/// Declares one manifest-selected native embedding or rerank execution binding.
/// 声明一个由 Manifest 选择的原生向量或重排执行绑定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeInferenceBinding {
    /// Provider-neutral operation owned by this binding.
    /// 此绑定持有的供应商无关操作。
    pub operation: AiOperation,
    /// Stable action binding persisted in internal inference routes.
    /// 持久化到内部推理路由的稳定动作绑定标识。
    pub binding_id: String,
    /// Relative JSON endpoint appended to the selected API root.
    /// 追加到所选 API 根地址的相对 JSON 端点。
    pub endpoint_path: String,
    /// Closed request/response projection primitive.
    /// 封闭的请求与响应投影原语。
    pub projection: NativeInferenceProjectionKind,
    /// Optional origin-relative API root used instead of the conversation protocol root.
    /// 可选的源地址相对 API 根路径，用于替代对话协议根路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_origin_base_path: Option<String>,
    /// Whether execution requires a configured credential-scoped service origin.
    /// 执行是否要求已配置凭据作用域服务源地址。
    #[serde(default)]
    pub requires_service_origin_override: bool,
    /// Exact rerank truncation contract when this is a rerank binding.
    /// 当此绑定用于重排时的精确截断契约。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_truncation: Option<RerankTruncation>,
    /// Reviewed provider-reported aliases keyed by the public requested model id.
    /// 按公开请求模型标识索引的已审核供应商回报别名。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub response_model_aliases: BTreeMap<String, BTreeSet<String>>,
}

/// Selects one closed provider-neutral native inference wire projection.
/// 选择一个封闭且供应商无关的原生推理 Wire 投影。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeInferenceProjectionKind {
    /// Plain compatible JSON without an upstream routing policy.
    /// 不携带上游路由策略的普通兼容 JSON。
    Compatible,
    /// Compatible JSON with explicit no-fallback secondary routing.
    /// 携带显式禁止降级二级路由的兼容 JSON。
    RoutedCompatible,
}

/// Identifies one closed provider request projector implementation.
/// 标识一个封闭的供应商请求投影器实现。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestProjectorKind {
    /// No provider-only mutation beyond the selected wire protocol.
    /// 除所选 Wire 协议外不执行供应商专属变更。
    Direct,
    /// Compatible JSON whose provider differences are declared by frozen request Profiles.
    /// 供应商差异由冻结请求 Profile 声明的兼容 JSON。
    Compatible,
    /// Compatible JSON with an explicit no-fallback secondary-route primitive.
    /// 带显式禁止降级二级路由原语的兼容 JSON。
    RoutedCompatible,
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
    /// OpenRouter public model catalog.
    /// OpenRouter 公共模型目录。
    OpenRouter,
    /// Manifest-owned static catalog without a remote catalog request.
    /// 不请求远程目录、完全由 Manifest 持有的静态目录。
    ManifestStatic,
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
    /// Sends bearer authorization plus the registered coding-device identity headers.
    /// 发送 Bearer 授权以及已注册的编程设备身份请求头。
    CodingDeviceBearer,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountUsageQueryKind {
    /// Account usage is not exposed by this entry.
    /// 此入口不公开账号用量。
    #[default]
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

impl AccountUsageQueryKind {
    /// Reports whether this usage primitive requires a separate provider-management credential.
    /// 报告当前用量原语是否需要独立的供应商管理凭据。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for primitives whose registered transport cannot use the inference secret.
    /// 仅当已注册传输不能使用推理密钥时返回 `true`。
    pub fn requires_management_credential(self) -> bool {
        matches!(self, Self::OpenRouterCredits)
    }
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
#[serde(deny_unknown_fields)]
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

/// Defines which source representations one input resource kind may use on a frozen route.
/// 定义冻结路由上的一种输入资源允许使用哪些来源表示形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceReferencePolicy {
    /// Local paths and remote URLs are both accepted.
    /// 同时接受本地路径与远程 URL。
    #[default]
    Any,
    /// Only host-resolved local resources are accepted before request materialization.
    /// 请求物化前只接受由宿主解析的本地资源。
    LocalOnly,
    /// Only pre-existing remote URLs are accepted.
    /// 只接受预先存在的远程 URL。
    RemoteOnly,
}

/// Constrains one input resource kind for an immutable protocol route snapshot.
/// 约束不可变协议路由快照中的一种输入资源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputResourceConstraint {
    /// Resource kind governed by this constraint.
    /// 当前约束管辖的资源类别。
    pub resource_kind: ResourceKind,
    /// Accepted MIME types; an empty list means every MIME type accepted by the projector.
    /// 接受的 MIME 类型；空列表表示接受投影器支持的全部 MIME 类型。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_mime_types: Vec<String>,
    /// Optional maximum local payload size in bytes.
    /// 可选的本地载荷最大字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    /// Optional maximum number of resources of this kind in one message.
    /// 可选的单条消息内该类资源最大数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_count: Option<u64>,
    /// Allowed source representation before the host materializes the request.
    /// 宿主物化请求前允许的来源表示形式。
    #[serde(default)]
    pub reference_policy: ResourceReferencePolicy,
    /// Ordered manifest upload channels used to materialize local resources for this route.
    /// 为当前路由物化本地资源时使用的有序 Manifest 上传通道。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upload_channel_refs: Vec<String>,
}

/// Describes one verified protocol route.
/// 描述一条已验证协议路由。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Manifest-driven input resource constraints frozen with this route.
    /// 随当前路由冻结的清单驱动输入资源约束。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_resource_constraints: Vec<InputResourceConstraint>,
    /// Versioned raw usage extraction profiles frozen with this route.
    /// 随当前路由冻结的版本化原始用量提取 Profile。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usage_extraction_profiles: Vec<UsageExtractionProfile>,
    /// Ordered request-projection profile payloads frozen from the route default refs.
    /// 从路由默认引用冻结的有序请求投影 Profile 载荷。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub request_projection_profiles: Vec<crate::provider_manifest::RequestProjectionProfile>,
    /// Ordered response-projection profile payloads frozen from the route default refs.
    /// 从路由默认引用冻结的有序响应投影 Profile 载荷。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_projection_profiles: Vec<crate::provider_manifest::ResponseProjectionProfile>,
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
    /// Manifest-derived compatibility protocol anchor.
    /// Manifest 推导的兼容协议锚点。
    ///
    /// This field is retained only for persisted-configuration shape and rollback compatibility.
    /// New writes derive it from the service entry's declared route order. Discovery and execution
    /// must never consult it as protocol authority (plan §12.2).
    /// 本字段仅为持久化配置形状与版本回滚兼容而保留。新写入值从服务入口声明的路由顺序
    /// 推导；发现与执行绝不能把它作为协议权威（方案 §12.2）。
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
    /// Per-protocol capability variants discovered from each distinct protocol route.
    /// Each variant keeps its own capability evidence and discovery signature; the top-level
    /// `capabilities` field is only a read-only compatibility projection of the effective
    /// binding's variant and is never an execution authority for protocol guessing.
    /// 每个不同协议路由发现的按协议能力变体。每个变体保留自己的能力证据与发现签名；
    /// 顶层 `capabilities` 字段只是有效 binding 所指变体的只读兼容投影，绝不是协议猜测的执行权威。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub protocol_variants: BTreeMap<String, ProtocolModelVariant>,
    /// Protocol binding frozen at the catalog activation boundary for manifest-backed system
    /// providers and custom providers. Explicit legacy catalogs and blocked models have no
    /// binding and expose an empty non-executable compatibility projection (plan §13.1/§13.6).
    /// Manifest 驱动的系统供应商与自定义供应商都在目录激活边界冻结协议绑定。显式旧版目录和
    /// 被阻止模型没有绑定，并暴露空的不可执行兼容投影（方案 §13.1/§13.6）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_protocol_binding: Option<crate::provider_manifest::ModelProtocolBinding>,
}

/// One protocol-scoped capability snapshot with its own discovery evidence.
/// 一份带独立发现证据的协议作用域能力快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolModelVariant {
    /// Capabilities observed through the exact protocol route.
    /// 通过精确协议路由观测到的能力。
    pub capabilities: ModelCapabilities,
    /// Field-presence evidence captured through the exact protocol route.
    /// 通过精确协议路由捕获的字段存在性证据。
    #[serde(default)]
    pub capability_evidence: ModelCapabilityEvidence,
    /// Stable hash of the discovery signature that produced this variant.
    /// 产生此变体的发现签名稳定哈希。
    pub discovery_signature: String,
    /// Catalog generation that observed this variant.
    /// 观测到此变体的目录代次。
    pub catalog_generation: u64,
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
    /// Extends the duration of an existing video.
    /// 延长现有视频的时长。
    VideoExtend,
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

/// Identifies one closed resource carrier decoded from provider JSON.
/// 标识一种从供应商 JSON 解码的封闭资源载体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderToolResourceEncoding {
    /// Absolute credential-free HTTPS URL.
    /// 不含凭据的绝对 HTTPS 地址。
    HttpsUrl,
    /// Absolute credential-free HTTP or HTTPS URL for explicitly reviewed public artifacts.
    /// 用于已明确审核公共产物的不含凭据绝对 HTTP 或 HTTPS 地址。
    PublicHttpUrl,
    /// RFC-style Data URL with an explicit MIME type and Base64 payload.
    /// 带显式 MIME 类型与 Base64 载荷的 RFC 风格 Data URL。
    DataUrl,
    /// Raw standard-Base64 bytes stored in one JSON string.
    /// 存放在单个 JSON 字符串中的原始标准 Base64 字节。
    Base64,
    /// Raw hexadecimal bytes stored in one JSON string.
    /// 存放在单个 JSON 字符串中的原始十六进制字节。
    Hex,
}

/// Declares one bounded provider-response resource extractor.
/// 声明一个有界的供应商响应资源提取器。
///
/// # Contract
/// 契约
/// `source_path` is an absolute JSON Pointer and may resolve to one string or an array of strings.
/// Exactly one MIME authority is used: `mime_type`, an explicit request-value mapping, or the MIME
/// embedded in a Data URL. Raw byte carriers require a byte ceiling.
/// `source_path` 是绝对 JSON Pointer，可解析为一个字符串或字符串数组。MIME 权威只能来自
/// `mime_type`、显式请求值映射或 Data URL 内嵌 MIME；原始字节载体必须声明字节上限。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolResourceDecoder {
    /// Exact response JSON Pointer containing one resource or an ordered resource array.
    /// 包含一个资源或有序资源数组的精确响应 JSON Pointer。
    pub source_path: String,
    /// Closed carrier encoding used to decode every selected value.
    /// 用于解码每个选中值的封闭载体编码。
    pub encoding: ProviderToolResourceEncoding,
    /// Exact hosts or dot-prefixed DNS suffixes authorized for URL download carriers.
    /// URL 下载载体获准使用的精确宿主或点前缀 DNS 后缀。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub download_host_suffixes: Vec<String>,
    /// Exact normalized resource kind, such as image, audio, video, or file.
    /// 精确的归一化资源类别，例如 image、audio、video 或 file。
    pub resource_kind: String,
    /// Optional fixed MIME type for URL or raw-byte carriers.
    /// URL 或原始字节载体使用的可选固定 MIME 类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional canonical request JSON Pointer selecting a MIME mapping entry.
    /// 用于选择 MIME 映射项的可选规范请求 JSON Pointer。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type_request_path: Option<String>,
    /// Exact request scalar-to-MIME mapping used with `mime_type_request_path`.
    /// 与 `mime_type_request_path` 配套使用的精确请求标量到 MIME 映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mime_types_by_request_value: BTreeMap<String, String>,
    /// Whether an absent, null, empty string, or empty array source is allowed.
    /// 是否允许来源缺失、为 null、空字符串或空数组。
    #[serde(default)]
    pub optional: bool,
    /// Maximum decoded byte count for Data URL, Base64, or hexadecimal carriers.
    /// Data URL、Base64 或十六进制载体允许的最大解码字节数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    /// Stable output identifier prefix; defaults to `resource` when omitted.
    /// 稳定输出标识前缀；省略时默认为 `resource`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_id_prefix: Option<String>,
}

/// Describes how one provider-native response is normalized for model output and tool cards.
/// 描述如何将一个供应商原生响应归一化为模型输出与工具卡片数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Optional ordered resource decoders for mixed carriers and request-selected MIME values.
    /// 用于混合载体与请求选择 MIME 的可选有序资源解码器。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_decoders: Vec<ProviderToolResourceDecoder>,
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
    /// Host-only bounded product assertions applied after JSON Schema defaults.
    /// 在 JSON Schema 默认值之后应用的仅宿主可见有界乘积断言。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_numeric_product_ranges: Vec<ProviderToolNumericProductRange>,
    /// Closed provider-response normalization contract.
    /// 封闭的供应商响应归一化契约。
    pub output: ProviderToolOutputContract,
}

/// One host-only inclusive product range over two integer input fields.
/// 一条针对两个整数输入字段的仅宿主可见闭区间乘积约束。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolNumericProductRange {
    /// Exact root-level JSON Pointer paths for the two integer factors.
    /// 两个整数因子的精确根级 JSON Pointer 路径。
    pub factor_paths: [String; 2],
    /// Inclusive minimum product.
    /// 乘积闭区间下限。
    pub minimum: u64,
    /// Inclusive maximum product.
    /// 乘积闭区间上限。
    pub maximum: u64,
    /// Whether either factor requires the other factor to be present.
    /// 任一因子存在时是否要求另一个因子同时存在。
    #[serde(default)]
    pub require_all_or_none: bool,
}

/// Identifies one closed provider-native tool request/response adapter.
/// 标识一种封闭的供应商原生工具请求与响应适配器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderToolAdapterKind {
    /// Manifest-complete generic JSON request/response codec.
    /// Manifest 完整定义的通用 JSON 请求/响应编解码器。
    GenericJson,
}

/// Describes the exact execution route for one provider-native tool target.
/// 描述一个供应商原生工具目标的精确执行路由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Generic execution family driving the transport primitive (plan §16.2).
    /// 驱动传输原语的通用执行族（方案 §16.2）。
    pub execution_kind: ProviderToolExecutionKind,
    /// Exact non-sensitive fixed headers required by this target.
    /// 当前目标要求的精确非敏感固定请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fixed_headers: BTreeMap<String, String>,
    /// Exact non-sensitive headers used only by asynchronous task creation.
    /// 仅用于异步任务创建的精确非敏感请求头。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub start_fixed_headers: BTreeMap<String, String>,
    /// Bounded non-sensitive start headers enabled by canonical argument predicates.
    /// 由规范参数断言启用的有界非敏感启动请求头。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub start_conditional_headers: Vec<ProviderToolConditionalHeader>,
    /// Optional closed Streamable HTTP MCP call contract for the MCP execution family.
    /// MCP 执行族使用的可选封闭 Streamable HTTP MCP 调用契约。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_http_call: Option<ProviderToolMcpHttpCallContract>,
}

/// Declares one bounded Streamable HTTP MCP call and embedded text-JSON aggregation contract.
/// 声明一条有界 Streamable HTTP MCP 调用与嵌入文本 JSON 聚合契约。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolMcpHttpCallContract {
    /// Exact MCP protocol revision sent during initialization.
    /// 初始化时发送的精确 MCP 协议修订版。
    pub protocol_version: String,
    /// JSON Pointer selecting an array from each non-empty MCP text content block.
    /// 从每个非空 MCP 文本内容块选择数组的 JSON Pointer。
    pub content_json_array_pointer: String,
    /// Maximum MCP content blocks accepted from one tools/call result.
    /// 单次 tools/call 结果允许的最大 MCP 内容块数量。
    pub max_content_blocks: u16,
    /// Maximum aggregated embedded JSON items accepted across all content blocks.
    /// 所有内容块合计允许的最大嵌入 JSON 条目数量。
    pub max_items: u16,
}

/// One non-sensitive start header guarded by a bounded OR predicate set.
/// 一项由有界 OR 断言集合保护的非敏感启动请求头。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolConditionalHeader {
    /// Exact HTTP header name.
    /// 精确 HTTP 请求头名称。
    pub name: String,
    /// Exact non-sensitive HTTP header value.
    /// 精确非敏感 HTTP 请求头值。
    pub value: String,
    /// Predicates evaluated with OR semantics against post-upload canonical arguments.
    /// 针对上传后规范参数以 OR 语义求值的断言。
    pub when_any: Vec<ProviderToolArgumentPredicate>,
}

/// One bounded canonical-argument predicate with optional exact scalar comparison.
/// 一项带可选精确标量比较的有界规范参数断言。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderToolArgumentPredicate {
    /// Absolute JSON Pointer pattern whose `*` segment ranges over array items.
    /// 使用 `*` 段遍历数组项的绝对 JSON Pointer 模式。
    pub pointer_pattern: String,
    /// Optional exact scalar value; omission tests only for a non-empty present value.
    /// 可选精确标量值；省略时仅测试非空存在值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<Value>,
}

/// Closed generic execution family that replaces provider-named adapters (plan §16.2).
/// 替代供应商命名适配器的封闭通用执行族（方案 §16.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderToolExecutionKind {
    /// Synchronous JSON request/response execution.
    /// 同步 JSON 请求/响应执行。
    HttpJsonSync,
    /// Synchronous binary request or response execution.
    /// 同步二进制请求或响应执行。
    HttpBinarySync,
    /// Conversation-carrier media analysis with the verified protocol encoder.
    /// 使用已验证协议编码器的会话载体媒体分析。
    ConversationMediaAnalyze,
    /// Asynchronous start/poll/cancel task execution.
    /// 异步启动/轮询/取消任务执行。
    HttpJsonAsyncPoll,
    /// Stateful Streamable HTTP MCP initialization, notification, and one tool call.
    /// 有状态 Streamable HTTP MCP 初始化、通知与单次工具调用。
    McpHttpCall,
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

    /// Verifies credential separation is owned by the registered usage primitive.
    /// 验证凭据分离语义由已注册用量原语拥有。
    #[test]
    fn account_usage_primitive_declares_management_credential_requirement() {
        assert!(AccountUsageQueryKind::OpenRouterCredits.requires_management_credential());
        assert!(!AccountUsageQueryKind::MiniMaxTokenPlan.requires_management_credential());
        assert!(!AccountUsageQueryKind::Unsupported.requires_management_credential());
    }

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
