//! Public metadata contracts, shared client ownership, and operation dispatch.
//! 公开元数据契约、共享客户端归属与操作分派。

use crate::{
    AccountUsageQueryKind, ConnectionModelCatalog, CredentialConnection, CredentialExecutionSecret,
    ProtocolRoute, ProviderDescriptor, ResolvedProviderManifest, SecondaryRouteOption,
    ServiceEntry,
};
use async_trait::async_trait;
use std::collections::BTreeMap;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot};

mod account_usage;
mod http;
mod minimax_registration;
mod model_discovery;
mod openvulcan;
pub use openvulcan::{
    OpenVulcanAccountIdentity, OpenVulcanUsagePage, query_openvulcan_identity,
    query_openvulcan_usage_page,
};
mod secondary_routes;

use account_usage::{
    query_command_code_usage, query_deepseek_balance, query_kimi_coding_plan,
    query_minimax_token_plan, query_open_design_wallet, query_openai_codex_usage,
    query_opencode_go, query_openrouter_credits, query_tavily_credits,
};
pub use minimax_registration::MiniMaxKeyRegistration;
use model_discovery::{
    default_discovery_path, manifest_protocol_authorizes_model_variant,
    validate_discovered_catalog_bounds,
};
use secondary_routes::query_openrouter_model_endpoints;

/// Discovery schema revision identifying a catalog whose complete authority comes from Manifest.
/// 标识完整权威来自 Manifest 的目录发现 Schema 修订号。
pub(crate) const MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION: &str =
    "provider-manifest.static-models.v1";

/// Closed failure categories produced at the model-discovery boundary.
/// 在模型发现边界产出的封闭失败分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelDiscoveryFailureKind {
    /// The selected endpoint explicitly does not expose a model catalog.
    /// 所选入口明确不提供模型目录。
    Unsupported,
    /// The selected credential was rejected by the provider.
    /// 所选凭据被供应商拒绝。
    AuthenticationFailed,
    /// The provider rate-limited the catalog request.
    /// 供应商限制了目录请求速率。
    RateLimited,
    /// The provider or network is temporarily unavailable.
    /// 供应商或网络暂时不可用。
    TemporarilyUnavailable,
    /// The provider returned a response that violates the registered catalog schema.
    /// 供应商返回了违反已登记目录 Schema 的响应。
    InvalidResponse,
    /// Local route configuration or an unclassified provider failure prevented discovery.
    /// 本地路由配置或未分类供应商失败阻止了发现。
    Failed,
}

/// Typed model-discovery failure with a controlled diagnostic message.
/// 带受控诊断消息的强类型模型发现失败。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelDiscoveryError {
    /// Closed machine-readable failure category.
    /// 封闭的机器可读失败分类。
    pub kind: ModelDiscoveryFailureKind,
    /// Controlled human-readable diagnostic without secret material.
    /// 不含秘密材料的受控人类可读诊断。
    pub message: String,
}

impl ModelDiscoveryError {
    /// Builds one typed discovery failure from an explicit category and controlled message.
    /// 使用明确分类和受控消息构建一个强类型发现失败。
    ///
    /// # Parameters
    /// 参数
    /// `kind` is the closed failure category and `message` is safe diagnostic text.
    /// `kind` 是封闭失败分类，`message` 是安全诊断文本。
    ///
    /// # Returns
    /// 返回值
    /// A typed model-discovery error.
    /// 返回强类型模型发现错误。
    pub fn new(kind: ModelDiscoveryFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Builds one invalid-response failure from a parser or pagination diagnostic.
    /// 使用解析器或分页诊断构建无效响应失败。
    ///
    /// # Parameters
    /// 参数
    /// `message` is the controlled contract violation.
    /// `message` 是受控契约违规消息。
    ///
    /// # Returns
    /// 返回值
    /// A typed invalid-response error.
    /// 返回强类型无效响应错误。
    pub fn invalid_response(message: impl Into<String>) -> Self {
        Self::new(ModelDiscoveryFailureKind::InvalidResponse, message)
    }

    /// Builds one local or unclassified discovery failure.
    /// 构建一个本地或未分类发现失败。
    ///
    /// # Parameters
    /// 参数
    /// `message` is the controlled diagnostic.
    /// `message` 是受控诊断。
    ///
    /// # Returns
    /// 返回值
    /// A typed generic discovery error.
    /// 返回强类型通用发现错误。
    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(ModelDiscoveryFailureKind::Failed, message)
    }

    /// Classifies one exact HTTP response without parsing error prose.
    /// 在不解析错误文案的情况下分类一个精确 HTTP 响应。
    ///
    /// # Parameters
    /// 参数
    /// `status` is the exact HTTP status and `message` is a controlled response diagnostic.
    /// `status` 是精确 HTTP 状态，`message` 是受控响应诊断。
    ///
    /// # Returns
    /// 返回值
    /// A typed discovery error with the matching closed category.
    /// 返回带匹配封闭分类的强类型发现错误。
    pub fn from_http_status(status: u16, message: impl Into<String>) -> Self {
        let kind = match status {
            404 | 405 => ModelDiscoveryFailureKind::Unsupported,
            401 | 403 => ModelDiscoveryFailureKind::AuthenticationFailed,
            429 => ModelDiscoveryFailureKind::RateLimited,
            408 => ModelDiscoveryFailureKind::TemporarilyUnavailable,
            500..=599 => ModelDiscoveryFailureKind::TemporarilyUnavailable,
            _ => ModelDiscoveryFailureKind::Failed,
        };
        Self::new(kind, message)
    }
}

impl std::fmt::Display for ModelDiscoveryError {
    /// Formats only the controlled diagnostic message.
    /// 仅格式化受控诊断消息。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message.as_str())
    }
}

impl std::error::Error for ModelDiscoveryError {}

/// Performs credential-scoped provider metadata operations.
/// 执行凭据作用域的供应商元数据操作。
#[async_trait]
pub trait ProviderMetadataClient: Send + Sync {
    /// Returns the bundled resolved manifest for one provider when available.
    /// 可用时返回一个供应商的内置已解析清单。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` identifies the exact system provider.
    /// `provider_id` 标识精确系统供应商。
    ///
    /// # Returns
    /// 返回值
    /// The compiled manifest, or `None` for legacy/custom providers without bundled manifests.
    /// 返回已编译清单；无内置清单的旧版/自定义供应商返回 `None`。
    fn bundled_manifest(
        &self,
        _provider_id: &str,
    ) -> Option<std::sync::Arc<ResolvedProviderManifest>> {
        None
    }

    /// Discovers models visible to one exact credential.
    /// 发现一个精确凭据可见的模型。
    ///
    /// # Parameters
    /// 参数
    /// `provider`, `entry`, and `connection` are the resolved configuration snapshot;
    /// `secret` is loaded by the host secret store; `generation` is memory-local.
    /// `provider`、`entry` 与 `connection` 是解析后的配置快照；`secret` 由宿主秘钥库加载；
    /// `generation` 仅存在于内存。
    ///
    /// # Returns
    /// 返回值
    /// A non-persistent connection model catalog.
    /// 返回不持久化的连接模型目录。
    async fn discover_models(
        &self,
        provider: &ProviderDescriptor,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        secret: &CredentialExecutionSecret,
        manifest: Option<std::sync::Arc<ResolvedProviderManifest>>,
        generation: u64,
    ) -> Result<ConnectionModelCatalog, ModelDiscoveryError>;

    /// Queries account usage for one exact credential.
    /// 查询一个精确凭据的账号用量。
    ///
    /// # Parameters
    /// 参数
    /// `entry` and `connection` are the resolved configuration snapshot; `secret` is loaded by
    /// the host secret store.
    /// `entry` 与 `connection` 是解析后的配置快照；`secret` 由宿主秘钥库加载。
    ///
    /// # Returns
    /// 返回值
    /// A normalized account usage observation.
    /// 返回归一化账号用量观测。
    async fn query_account_usage(
        &self,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        secret: &CredentialExecutionSecret,
    ) -> AccountUsageSnapshot;

    /// Discovers secondary routes for one exact model and credential.
    /// 发现一个精确模型与凭据可用的二级路由。
    ///
    /// # Parameters
    /// 参数
    /// `provider`, `entry`, and `connection` identify the fixed route; `model_id` is exact and
    /// `secret` is loaded through the host secret boundary.
    /// `provider`、`entry` 与 `connection` 标识固定路由；`model_id` 是精确模型标识，
    /// `secret` 通过宿主秘密边界加载。
    ///
    /// # Returns
    /// 返回值
    /// Provider-reported route options, or an explicit unsupported/discovery error.
    /// 返回供应商报告的路由选项，或显式的不支持/发现错误。
    async fn discover_secondary_routes(
        &self,
        provider: &ProviderDescriptor,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        model_id: &str,
        secret: &CredentialExecutionSecret,
    ) -> Result<Vec<SecondaryRouteOption>, String>;
}

/// Default reqwest-backed metadata client.
/// 默认的 reqwest 元数据客户端。
#[derive(Debug, Clone)]
pub struct HttpProviderMetadataClient {
    /// Shared HTTP client.
    /// 共享 HTTP 客户端。
    http: reqwest::Client,
    /// Single authoritative resolved-provider catalog shared with the runtime; hot
    /// replacements are immediately visible without a second load or clone (plan §17.6).
    /// 与运行时共享的唯一权威已解析供应商目录；热替换立即可见，无需二次加载或克隆
    /// （方案 §17.6）。
    bundled_manifests: std::sync::Arc<
        std::sync::RwLock<std::sync::Arc<crate::provider_manifest::ResolvedProviderCatalog>>,
    >,
}

impl HttpProviderMetadataClient {
    /// Creates a metadata client from an existing HTTP client.
    /// 从已有 HTTP 客户端创建元数据客户端。
    ///
    /// # Parameters
    /// 参数
    /// `http` is the shared configured reqwest client.
    /// `http` 是共享且已配置的 reqwest 客户端。
    ///
    /// # Returns
    /// 返回值
    /// A provider metadata client.
    /// 返回供应商元数据客户端。
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            bundled_manifests: std::sync::Arc::new(std::sync::RwLock::new(std::sync::Arc::new(
                crate::provider_manifest::ResolvedProviderCatalog::default(),
            ))),
        }
    }

    /// Creates a metadata client sharing one authoritative resolved-provider catalog.
    /// 创建与调用方共享唯一权威已解析供应商目录的元数据客户端。
    ///
    /// # Parameters
    /// 参数
    /// `http` is the shared configured reqwest client; `catalog` is the single catalog
    /// instance also held by the runtime so hot replacements reach discovery immediately.
    /// `http` 是共享且已配置的 reqwest 客户端；`catalog` 是与运行时持有的同一目录实例，
    /// 使热替换立即可达发现路径。
    ///
    /// # Returns
    /// 返回值
    /// A provider metadata client with manifest enrichment enabled.
    /// 返回启用 Manifest 补全的供应商元数据客户端。
    pub fn with_bundled_manifests(
        http: reqwest::Client,
        catalog: std::sync::Arc<
            std::sync::RwLock<std::sync::Arc<crate::provider_manifest::ResolvedProviderCatalog>>,
        >,
    ) -> Self {
        Self {
            http,
            bundled_manifests: catalog,
        }
    }
}

/// Reports whether one service route may participate in model discovery.
/// 报告一条服务路由是否可以参与模型发现。
///
/// `route` is the candidate service route compiled from the active Provider Manifest.
/// `route` 是从活动 Provider Manifest 编译得到的候选服务路由。
///
/// Returns true exactly when the active Manifest marks the route as verified.
/// 仅当活动 Manifest 把该路由标记为已验证时返回 true。
pub(crate) fn route_is_available_for_discovery(route: &ProtocolRoute) -> bool {
    route.verified
}

#[async_trait]
impl ProviderMetadataClient for HttpProviderMetadataClient {
    async fn discover_models(
        &self,
        provider: &ProviderDescriptor,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        secret: &CredentialExecutionSecret,
        manifest: Option<std::sync::Arc<ResolvedProviderManifest>>,
        generation: u64,
    ) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                ModelDiscoveryError::failed(format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                ))
            })?;
        // The active Provider Manifest owns the complete verified route set for both system and
        // custom providers. Shared signatures are discovered once and then projected into
        // per-model protocol variants (plan §14.2).
        // 活动 Provider Manifest 为系统与自定义供应商持有完整的已验证路由集合。共享签名只发现
        // 一次，随后投影为逐模型协议变体（方案 §14.2）。
        let mut groups: BTreeMap<String, Vec<&ProtocolRoute>> = BTreeMap::new();
        for route in entry
            .protocol_routes
            .iter()
            .filter(|route| route_is_available_for_discovery(route))
        {
            let authentication = route
                .resolved_authentication(entry.authentication, credential_profile.authentication);
            let url = route.model_discovery_url(
                entry,
                default_discovery_path(route.resolved_model_discovery(entry.model_discovery)),
            );
            let signature = crate::discovery_signature::discovery_signature(
                route.resolved_model_discovery(entry.model_discovery),
                url.as_str(),
                connection.credential_profile_id.as_str(),
                &authentication,
                &route.request_headers,
            );
            groups.entry(signature).or_default().push(route);
        }
        if groups.is_empty() {
            return Err(ModelDiscoveryError::failed(format!(
                "Service entry `{}` has no verified protocol route for discovery.",
                entry.id
            )));
        }

        let mut merged = ConnectionModelCatalog {
            connection_id: connection.id.clone(),
            models: Vec::new(),
            generation,
            page_count: 0,
            discovery_schema_revision: String::new(),
        };
        for (signature, routes) in groups {
            // The shared network result is fetched exactly once per signature; each protocol
            // route then receives its own capability/evidence enrichment and variant.
            // 每个签名的共享网络结果只获取一次；随后每条协议路由获得各自的能力/证据补全与变体。
            let raw = self
                .discover_for_route(
                    entry,
                    connection,
                    secret,
                    manifest.as_deref(),
                    generation,
                    routes[0],
                )
                .await?;
            if merged.discovery_schema_revision.is_empty() {
                merged.discovery_schema_revision = raw.discovery_schema_revision.clone();
            }
            // Raw identities enter the diagnostic inventory independently from executable
            // protocol variants. This keeps an unmatched system model visible as "awaiting
            // mapping" without granting it any request route.
            // 原始身份独立于可执行协议变体进入诊断目录，使未匹配系统模型仍显示为“等待映射”，
            // 但不会因此获得任何请求路由。
            for raw_model in &raw.models {
                if merged
                    .models
                    .iter()
                    .all(|existing| existing.id != raw_model.id)
                {
                    let mut identity = raw_model.clone();
                    identity.protocol_variants.clear();
                    identity.resolved_protocol_binding = None;
                    merged.models.push(identity);
                }
            }
            for route in routes.iter() {
                // Enrich the raw shared result independently for this exact protocol route so
                // protocol-scoped rules and evidence never leak across routes (plan §14.2).
                // 对这份原始共享结果按当前精确协议路由独立补全，使协议作用域规则与证据
                // 绝不在路由间泄漏（方案 §14.2）。
                let mut protocol_catalog = raw.clone();
                self.enrich_catalog_for_protocol(
                    manifest.as_deref(),
                    provider,
                    entry,
                    connection.credential_profile_id.as_str(),
                    route.protocol_profile_id.as_str(),
                    &mut protocol_catalog,
                )?;
                for model in protocol_catalog.models.drain(..) {
                    if manifest.as_deref().is_some_and(|manifest| {
                        !manifest_protocol_authorizes_model_variant(
                            manifest,
                            entry.id.as_str(),
                            connection.credential_profile_id.as_str(),
                            route.protocol_profile_id.as_str(),
                            model.id.as_str(),
                        )
                    }) {
                        continue;
                    }
                    let variant = crate::ProtocolModelVariant {
                        capabilities: model.capabilities.clone(),
                        capability_evidence: model.capability_evidence.clone(),
                        discovery_signature: signature.clone(),
                        catalog_generation: generation,
                    };
                    let protocol_id = route.protocol_profile_id.clone();
                    match merged
                        .models
                        .iter_mut()
                        .find(|existing| existing.id == model.id)
                    {
                        Some(existing) => {
                            if existing.protocol_variants.is_empty() {
                                existing.display_name = model.display_name.clone();
                                existing.capabilities = model.capabilities.clone();
                                existing.capability_evidence = model.capability_evidence.clone();
                            }
                            existing.protocol_variants.insert(protocol_id, variant);
                        }
                        None => {
                            // The first route to cover the model keeps the top-level
                            // compatibility projection; every route contributes its variant.
                            // 首个覆盖该模型的路由保留顶层兼容投影；每条路由贡献各自变体。
                            let mut model = model;
                            model.protocol_variants.insert(protocol_id, variant);
                            merged.models.push(model);
                        }
                    }
                }
            }
            merged.page_count += raw.page_count;
        }
        validate_discovered_catalog_bounds(merged, connection.id.as_str())
    }

    async fn query_account_usage(
        &self,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        secret: &CredentialExecutionSecret,
    ) -> AccountUsageSnapshot {
        let credential_profile =
            match entry.credential_profile(connection.credential_profile_id.as_str()) {
                Some(profile) => profile,
                None => {
                    return AccountUsageSnapshot {
                        credential_id: connection.credential_id.clone(),
                        availability: AccountUsageAvailability::TemporarilyUnavailable,
                        plan_name: None,
                        plan_id: None,
                        used_amount: None,
                        remaining_amount: None,
                        currency: None,
                        balance_amount: None,
                        quota_windows: Vec::new(),
                        observed_at: None,
                        refresh_message: Some(format!(
                            "Credential profile `{}` is not registered for service entry `{}`.",
                            connection.credential_profile_id, entry.id
                        )),
                    };
                }
            };
        let result = match credential_profile.account_usage_query {
            AccountUsageQueryKind::OpencodeGo => query_opencode_go(secret).await,
            AccountUsageQueryKind::OpenDesignWallet => query_open_design_wallet(secret).await,
            AccountUsageQueryKind::OpenVulcan => {
                openvulcan::query_usage(&self.http, entry, secret).await
            }
            AccountUsageQueryKind::Unsupported => {
                return unsupported_usage(connection.credential_id.as_str());
            }
            AccountUsageQueryKind::MiniMaxTokenPlan => {
                query_minimax_token_plan(&self.http, entry, secret.expose_value()).await
            }
            AccountUsageQueryKind::OpenRouterCredits => {
                query_openrouter_credits(&self.http, entry, secret.expose_value()).await
            }
            AccountUsageQueryKind::TavilyCredits => {
                query_tavily_credits(&self.http, entry, secret.expose_value()).await
            }
            AccountUsageQueryKind::DeepSeekBalance => {
                query_deepseek_balance(&self.http, entry, secret.expose_value()).await
            }
            AccountUsageQueryKind::KimiCodingPlan => {
                query_kimi_coding_plan(
                    &self.http,
                    secret.expose_value(),
                    secret.provider_device_id(),
                )
                .await
            }
            AccountUsageQueryKind::CommandCodeUsage => {
                query_command_code_usage(&self.http, secret.expose_value()).await
            }
            AccountUsageQueryKind::OpenAiCodexUsage => {
                query_openai_codex_usage(&self.http, entry, secret).await
            }
        };
        match result {
            Ok(mut snapshot) => {
                snapshot.credential_id = connection.credential_id.clone();
                snapshot
            }
            Err(message) => AccountUsageSnapshot {
                credential_id: connection.credential_id.clone(),
                availability: AccountUsageAvailability::TemporarilyUnavailable,
                plan_name: None,
                plan_id: None,
                used_amount: None,
                remaining_amount: None,
                currency: None,
                balance_amount: None,
                quota_windows: Vec::new(),
                observed_at: None,
                refresh_message: Some(message),
            },
        }
    }

    async fn discover_secondary_routes(
        &self,
        provider: &ProviderDescriptor,
        entry: &ServiceEntry,
        _connection: &CredentialConnection,
        model_id: &str,
        secret: &CredentialExecutionSecret,
    ) -> Result<Vec<SecondaryRouteOption>, String> {
        if entry.request_projector != crate::RequestProjectorKind::RoutedCompatible {
            return Err(format!(
                "Service entry `{}` for provider `{}` does not register secondary route discovery.",
                entry.id, provider.id
            ));
        }
        query_openrouter_model_endpoints(&self.http, entry, model_id, secret.expose_value()).await
    }

    fn bundled_manifest(
        &self,
        provider_id: &str,
    ) -> Option<std::sync::Arc<ResolvedProviderManifest>> {
        self.bundled_manifests
            .read()
            .ok()
            .and_then(|catalog| catalog.manifest_arc(provider_id))
    }
}

impl HttpProviderMetadataClient {
    /// Returns the current catalog generation observed by this client.
    /// 返回该客户端观察到的当前目录代数。
    ///
    /// # Returns
    /// 返回值
    /// The generation of the shared catalog at this instant; runtime and discovery client
    /// must observe the same number after every replacement (plan §R3.8).
    /// 此刻共享目录的代数；每次替换后运行时与发现客户端必须观察到相同数字（方案 §R3.8）。
    pub fn catalog_generation(&self) -> u64 {
        self.bundled_manifests
            .read()
            .map(|catalog| catalog.generation)
            .unwrap_or(0)
    }
}

/// Builds an explicit unsupported usage snapshot.
/// 构建明确的不支持用量快照。
fn unsupported_usage(credential_id: &str) -> AccountUsageSnapshot {
    AccountUsageSnapshot {
        credential_id: credential_id.to_string(),
        availability: AccountUsageAvailability::Unsupported,
        plan_name: None,
        plan_id: None,
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: Vec::new(),
        observed_at: None,
        refresh_message: None,
    }
}
