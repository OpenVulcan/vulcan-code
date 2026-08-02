//! Provider HTTP clients for discovery and account usage.
//! 用于模型发现和账号用量的供应商 HTTP 客户端。

use crate::{
    AccountUsageQueryKind, AuthenticationProjection, ConnectionModel, ConnectionModelCatalog,
    CredentialConnection, CredentialExecutionSecret, ModelDiscoveryKind,
    PROVIDER_MODEL_CATALOG_MAX_MODELS, PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES,
    PROVIDER_MODEL_IDENTIFIER_MAX_BYTES, ProviderDescriptor, SecondaryRouteOption, ServiceEntry,
    build_alibaba_coding_plan_catalog, build_alibaba_model_studio_catalog,
    build_alibaba_token_plan_catalog, build_anthropic_model_catalog, build_gemini_model_catalog,
    build_minimax_model_catalog, enrich_compatible_model_catalog, parse_anthropic_model_page,
    parse_gemini_model_page, parse_google_code_assist_model_catalog, parse_minimax_model_page,
    parse_minimax_openai_model_catalog, parse_openai_codex_model_catalog,
    parse_openai_model_catalog, parse_openrouter_complete_model_catalog,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Maximum wall time for one provider metadata or account-usage HTTP request.
/// 单次供应商元数据或账号用量 HTTP 请求的最大墙钟时间。
const PROVIDER_METADATA_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Maximum pages accepted from one credential-scoped remote model catalog.
/// 单次凭据作用域远程模型目录允许接受的最大分页数。
const MODEL_DISCOVERY_MAX_PAGES: u32 = 1_024;

/// Maximum decoded bytes accepted from one remote model-catalog response page.
/// 单个远程模型目录响应分页允许接受的最大解码字节数。
const MODEL_DISCOVERY_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Maximum decoded bytes accepted from one non-catalog provider metadata response.
/// 单个非目录供应商元数据响应允许接受的最大解码字节数。
const PROVIDER_METADATA_MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Closed failure variants produced while reading a bounded HTTP response body.
/// 读取有界 HTTP 响应正文时产出的封闭失败变体。
#[derive(Debug)]
enum BoundedResponseReadError {
    /// The transport failed before the complete response body arrived.
    /// 完整响应正文到达前传输失败。
    Transport(String),
    /// The response violated its byte or UTF-8 boundary.
    /// 响应违反字节或 UTF-8 边界。
    Invalid(String),
}

impl BoundedResponseReadError {
    /// Converts the shared body-reader failure into the model-discovery error contract.
    /// 将共享正文读取失败转换为模型发现错误契约。
    ///
    /// # Returns
    /// 返回值
    /// A transport or invalid-response model-discovery failure preserving the controlled message.
    /// 返回保留受控消息的传输失败或无效响应模型发现错误。
    fn into_model_discovery_error(self) -> ModelDiscoveryError {
        match self {
            Self::Transport(message) => {
                ModelDiscoveryError::new(ModelDiscoveryFailureKind::TemporarilyUnavailable, message)
            }
            Self::Invalid(message) => ModelDiscoveryError::invalid_response(message),
        }
    }
}

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
}

/// Resolves the immutable MiniMax registration dimensions derived from one user key.
/// 表示从一个用户 Key 解析出的不可变 MiniMax 注册维度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniMaxKeyRegistration {
    /// Exact regional service-entry identifier.
    /// 精确的区域服务入口标识。
    pub service_entry_id: String,
    /// Exact credential-profile identifier derived from the reviewed key prefix.
    /// 从已审核 Key 前缀解析出的精确凭据配置档标识。
    pub credential_profile_id: String,
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
        Self { http }
    }

    /// Classifies one MiniMax key and probes its exact region without model inference.
    /// 对一个 MiniMax Key 分类并在不进行模型推理的情况下探测其精确区域。
    ///
    /// # Parameters
    /// 参数
    /// `key` is the complete user-managed MiniMax key retained only for the requests.
    /// `key` 是完整的用户管理 MiniMax Key，仅用于请求。
    ///
    /// # Returns
    /// 返回值
    /// The exact regional entry and billing profile when exactly one catalog accepts the key.
    /// 当且仅当一个区域目录接受 Key 时返回精确区域入口与计费配置档。
    pub async fn register_minimax_key(&self, key: &str) -> Result<MiniMaxKeyRegistration, String> {
        let credential_profile_id = minimax_key_profile_id(key)?;
        let (global, china) = tokio::join!(
            probe_minimax_region(&self.http, "https://api.minimax.io", key),
            probe_minimax_region(&self.http, "https://api.minimaxi.com", key),
        );
        let service_entry_id = resolve_minimax_region(global, china)?;
        Ok(MiniMaxKeyRegistration {
            service_entry_id: service_entry_id.to_string(),
            credential_profile_id: credential_profile_id.to_string(),
        })
    }
}

/// Resolves a MiniMax region only from two explicit catalog probe outcomes.
/// 仅根据两个明确的目录探测结果解析 MiniMax 区域。
///
/// # Parameters
/// 参数
/// `global` and `china` are independent reviewed regional probe results.
/// `global` 与 `china` 是相互独立且已审核的区域探测结果。
///
/// # Returns
/// 返回值
/// One exact entry identifier only when exactly one region accepts the key.
/// 仅当一个区域接受 Key 时返回一个精确入口标识。
fn resolve_minimax_region(
    global: Result<(), String>,
    china: Result<(), String>,
) -> Result<&'static str, String> {
    match (global, china) {
        (Ok(()), Err(_)) => Ok("global"),
        (Err(_), Ok(())) => Ok("cn"),
        (Ok(()), Ok(())) => Err(String::from(
            "MiniMax key was accepted by both regional catalogs; registration is ambiguous.",
        )),
        (Err(global_error), Err(china_error)) => Err(format!(
            "MiniMax key was rejected by both regional catalogs. International: {global_error} China: {china_error}"
        )),
    }
}

/// Classifies the exact reviewed MiniMax key prefixes.
/// 对精确且已审核的 MiniMax Key 前缀进行分类。
///
/// # Parameters
/// 参数
/// `key` is the complete user-managed MiniMax credential.
/// `key` 是完整的用户管理 MiniMax 凭据。
///
/// # Returns
/// 返回值
/// The exact credential profile or a controlled unsupported-prefix failure.
/// 返回精确凭据配置档或受控的不支持前缀错误。
fn minimax_key_profile_id(key: &str) -> Result<&'static str, String> {
    if key.starts_with("sk-api-") {
        Ok("payg-api-key")
    } else if key.starts_with("sk-cp-") {
        Ok("token-plan-key")
    } else {
        Err(String::from(
            "MiniMax key must use the reviewed `sk-api-` or `sk-cp-` prefix.",
        ))
    }
}

/// Probes one MiniMax regional model catalog using the reviewed Bearer projection.
/// 使用已审核的 Bearer 投影探测一个 MiniMax 区域模型目录。
///
/// # Parameters
/// 参数
/// `http` is the shared client, `base_url` is a fixed reviewed region, and `key` is user secret material.
/// `http` 是共享客户端，`base_url` 是固定已审核区域，`key` 是用户秘密材料。
///
/// # Returns
/// 返回值
/// Success only for HTTP success containing a valid non-empty OpenAI-compatible catalog.
/// 仅 HTTP 成功且包含有效非空 OpenAI 兼容目录时返回成功。
async fn probe_minimax_region(
    http: &reqwest::Client,
    base_url: &str,
    key: &str,
) -> Result<(), String> {
    let response = http
        .get(format!("{base_url}/v1/models"))
        .bearer_auth(key)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("catalog request failed: {error}"))?;
    let status = response.status();
    let url = format!("{base_url}/v1/models");
    let body_result =
        read_bounded_model_discovery_body(response, "MiniMax region probe", url.as_str()).await;
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    let body = body_result.map_err(|error| error.message)?;
    let catalog = validate_discovered_catalog_bounds(
        parse_openai_model_catalog("minimax-region-probe", 1, body.as_str())
            .map_err(|error| format!("invalid catalog: {error}"))?,
        "minimax-region-probe",
    )
    .map_err(|error| error.message)?;
    if catalog.models.is_empty() {
        return Err(String::from("catalog contained no models"));
    }
    Ok(())
}

#[async_trait]
impl ProviderMetadataClient for HttpProviderMetadataClient {
    async fn discover_models(
        &self,
        provider: &ProviderDescriptor,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        secret: &CredentialExecutionSecret,
        generation: u64,
    ) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
        let protocol_route = entry
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == connection.selected_protocol)
            .ok_or_else(|| {
                ModelDiscoveryError::failed(format!(
                    "Protocol `{}` is not registered for service entry `{}`.",
                    connection.selected_protocol, entry.id
                ))
            })?;
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                ModelDiscoveryError::failed(format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                ))
            })?;
        let authentication = protocol_route
            .resolved_authentication(entry.authentication, credential_profile.authentication);
        let catalog = match protocol_route.resolved_model_discovery(entry.model_discovery) {
            ModelDiscoveryKind::Manual => Ok(ConnectionModelCatalog {
                connection_id: connection.id.clone(),
                models: Vec::new(),
                generation,
                page_count: 0,
                discovery_schema_revision: String::from("manual.models.v1"),
            }),
            ModelDiscoveryKind::ProviderToolsOnly => Err(ModelDiscoveryError::new(
                ModelDiscoveryFailureKind::Unsupported,
                format!(
                    "Provider-tools service entry `{}` does not expose a conversation model catalog.",
                    entry.id
                ),
            )),
            ModelDiscoveryKind::MiniMaxAnthropic => {
                discover_minimax_models(
                    &self.http,
                    entry,
                    connection,
                    authentication,
                    secret,
                    generation,
                )
                .await
            }
            ModelDiscoveryKind::MiniMaxOpenAi => {
                let url = protocol_route.model_discovery_url(entry, "/v1/models");
                let body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    url.as_str(),
                    secret,
                    "model discovery",
                )
                .await?;
                parse_minimax_openai_model_catalog(
                    connection.id.as_str(),
                    generation,
                    body.as_str(),
                )
                .map_err(ModelDiscoveryError::invalid_response)
            }
            ModelDiscoveryKind::OpenRouter => {
                let conversation_url = protocol_route.model_discovery_url(entry, "/models/user");
                let conversation_body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    conversation_url.as_str(),
                    secret,
                    "account-visible model discovery",
                )
                .await?;
                let embedding_url = protocol_route.model_discovery_url(entry, "/embeddings/models");
                let embedding_body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    embedding_url.as_str(),
                    secret,
                    "embedding model discovery",
                )
                .await?;
                let complete_url =
                    protocol_route.model_discovery_url(entry, "/models?output_modalities=all");
                let complete_body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    complete_url.as_str(),
                    secret,
                    "complete model discovery",
                )
                .await?;
                parse_openrouter_complete_model_catalog(
                    connection.id.as_str(),
                    generation,
                    conversation_body.as_str(),
                    embedding_body.as_str(),
                    complete_body.as_str(),
                )
                .map_err(ModelDiscoveryError::invalid_response)
            }
            ModelDiscoveryKind::AlibabaCodingPlanStatic => build_alibaba_coding_plan_catalog(
                connection.id.as_str(),
                entry.id.as_str(),
                generation,
            )
            .map_err(ModelDiscoveryError::failed),
            ModelDiscoveryKind::AlibabaTokenPlanStatic => build_alibaba_token_plan_catalog(
                connection.id.as_str(),
                entry.id.as_str(),
                connection.selected_protocol.as_str(),
                generation,
            )
            .map_err(ModelDiscoveryError::failed),
            ModelDiscoveryKind::AlibabaModelStudioStatic => build_alibaba_model_studio_catalog(
                connection.id.as_str(),
                entry.id.as_str(),
                connection.selected_protocol.as_str(),
                generation,
            )
            .map_err(ModelDiscoveryError::failed),
            ModelDiscoveryKind::OpenAi => {
                let url = protocol_route.model_discovery_url(entry, "/models");
                let body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    url.as_str(),
                    secret,
                    "model discovery",
                )
                .await?;
                let mut catalog =
                    parse_openai_model_catalog(connection.id.as_str(), generation, body.as_str())
                        .map_err(ModelDiscoveryError::invalid_response)?;
                enrich_compatible_model_catalog(
                    provider.id.as_str(),
                    entry.id.as_str(),
                    connection.selected_protocol.as_str(),
                    &mut catalog,
                );
                Ok(catalog)
            }
            ModelDiscoveryKind::OpenAiCodex => {
                let url =
                    protocol_route.model_discovery_url(entry, "/models?client_version=0.144.1");
                let body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    url.as_str(),
                    secret,
                    "model discovery",
                )
                .await?;
                parse_openai_codex_model_catalog(connection.id.as_str(), generation, body.as_str())
                    .map_err(ModelDiscoveryError::invalid_response)
            }
            ModelDiscoveryKind::Anthropic => {
                discover_anthropic_models(&self.http, entry, connection, secret, generation).await
            }
            ModelDiscoveryKind::AnthropicCompatible => {
                // url preserves the reviewed protocol-route catalog path instead of rebuilding `/models`.
                // url 保留已审核的协议路由目录路径，而不是重新拼接 `/models`。
                let url = protocol_route.model_discovery_url(entry, "/models");
                discover_anthropic_compatible_models(
                    &self.http,
                    entry,
                    connection,
                    secret,
                    generation,
                    url.as_str(),
                )
                .await
            }
            ModelDiscoveryKind::Gemini => {
                discover_gemini_models(&self.http, entry, connection, secret, generation).await
            }
            ModelDiscoveryKind::GoogleCodeAssist => {
                discover_google_code_assist_models(
                    &self.http, entry, connection, secret, generation,
                )
                .await
            }
        }?;
        validate_discovered_catalog_bounds(catalog, connection.id.as_str())
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
        if provider.id != "openrouter" {
            return Err(format!(
                "Provider `{}` does not register secondary route discovery.",
                provider.id
            ));
        }
        query_openrouter_model_endpoints(&self.http, entry, model_id, secret.expose_value()).await
    }
}

/// Fetches the exact model map visible to one Google Code Assist credential.
/// 获取一个 Google Code Assist 凭据可见的精确模型映射。
///
/// # Parameters
/// 参数
/// `entry`, `connection`, and `secret` form the immutable credential-scoped route.
/// `entry`、`connection` 与 `secret` 组成不可变凭据作用域路由。
///
/// # Returns
/// 返回值
/// A process-memory-only model catalog.
/// 返回仅存在于进程内存的模型目录。
async fn discover_google_code_assist_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    secret: &CredentialExecutionSecret,
    generation: u64,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    let project_id = secret.project_id().ok_or_else(|| {
        ModelDiscoveryError::failed(
            "Google Code Assist model discovery requires a validated project id.",
        )
    })?;
    let url = format!(
        "{}/v1internal:fetchAvailableModels",
        entry.base_url.trim_end_matches('/')
    );
    let mut request = http
        .post(url.as_str())
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .bearer_auth(secret.expose_value())
        .header("content-type", "application/json")
        .header("user-agent", "vulcan-code/0.1")
        .json(&serde_json::json!({ "project": project_id }));
    if let Some(quota_project_id) = secret.quota_project_id() {
        request = request.header("x-goog-user-project", quota_project_id);
    }
    let response = request.send().await.map_err(|error| {
        ModelDiscoveryError::new(
            ModelDiscoveryFailureKind::TemporarilyUnavailable,
            format!("Google Code Assist model discovery failed: {error}"),
        )
    })?;
    let status = response.status();
    let body_result = read_bounded_model_discovery_body(
        response,
        "Google Code Assist model discovery",
        url.as_str(),
    )
    .await;
    if !status.is_success() {
        let diagnostic = body_result
            .as_ref()
            .map(|body| controlled_error_summary(body.as_str(), secret.expose_value()))
            .unwrap_or_else(|error| error.message.clone());
        return Err(ModelDiscoveryError::from_http_status(
            status.as_u16(),
            format!(
                "Google Code Assist model discovery returned HTTP {}: {}",
                status.as_u16(),
                diagnostic
            ),
        ));
    }
    let body = body_result?;
    parse_google_code_assist_model_catalog(connection.id.as_str(), generation, body.as_str())
        .map_err(ModelDiscoveryError::invalid_response)
}

/// Sends one authenticated metadata GET and returns a controlled response body.
/// 发送一次已鉴权元数据 GET 并返回受控响应正文。
async fn send_metadata_get(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    url: &str,
    secret: &CredentialExecutionSecret,
    operation: &str,
) -> Result<String, ModelDiscoveryError> {
    send_metadata_get_with_authentication(http, entry, entry.authentication, url, secret, operation)
        .await
}

/// Sends one metadata GET with the exact protocol-route authentication projection.
/// 使用精确协议路由鉴权投影发送一次元数据 GET。
///
/// # Parameters
/// 参数
/// `http` is the shared client, `entry` contributes trusted headers,
/// `authentication` is resolved from the selected route, `url` is exact,
/// `secret` is the credential snapshot, and `operation` labels controlled errors.
/// `http` 是共享客户端，`entry` 提供可信请求头，`authentication` 来自所选路由，
/// `url` 是精确地址，`secret` 是凭据快照，`operation` 标记受控错误。
///
/// # Returns
/// 返回值
/// The successful response body or a controlled metadata failure.
/// 返回成功响应正文或受控元数据失败。
async fn send_metadata_get_with_authentication(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    authentication: AuthenticationProjection,
    url: &str,
    secret: &CredentialExecutionSecret,
    operation: &str,
) -> Result<String, ModelDiscoveryError> {
    let mut request = http.get(url).timeout(PROVIDER_METADATA_REQUEST_TIMEOUT);
    for (name, value) in &entry.request_headers {
        let normalized_name = name.to_ascii_lowercase();
        if matches!(
            normalized_name.as_str(),
            "authorization"
                | "x-api-key"
                | "x-goog-api-key"
                | "chatgpt-account-id"
                | "x-goog-user-project"
        ) {
            return Err(ModelDiscoveryError::failed(format!(
                "Service entry must not override reserved metadata header `{name}`."
            )));
        }
        request = request.header(name, value);
    }
    request = match authentication {
        AuthenticationProjection::XApiKey => request.header("x-api-key", secret.expose_value()),
        AuthenticationProjection::BearerAuthorization
        | AuthenticationProjection::OpenAiCodexBearer
        | AuthenticationProjection::GoogleCodeAssistBearer
        | AuthenticationProjection::GoogleGeminiBearer => {
            request.bearer_auth(secret.expose_value())
        }
        AuthenticationProjection::GoogleApiKey => {
            request.header("x-goog-api-key", secret.expose_value())
        }
    };
    match authentication {
        AuthenticationProjection::OpenAiCodexBearer => {
            let account_id = secret.account_id().ok_or_else(|| {
                ModelDiscoveryError::failed(
                    "OpenAI Codex metadata request requires a verified account identity.",
                )
            })?;
            request = request.header("chatgpt-account-id", account_id);
        }
        AuthenticationProjection::GoogleCodeAssistBearer => {
            if let Some(quota_project_id) = secret.quota_project_id() {
                request = request.header("x-goog-user-project", quota_project_id);
            }
        }
        AuthenticationProjection::GoogleGeminiBearer => {
            let quota_project_id = secret.quota_project_id().ok_or_else(|| {
                ModelDiscoveryError::failed(
                    "Public Gemini OAuth metadata request requires a Google Cloud project id.",
                )
            })?;
            request = request.header("x-goog-user-project", quota_project_id);
        }
        AuthenticationProjection::XApiKey
        | AuthenticationProjection::BearerAuthorization
        | AuthenticationProjection::GoogleApiKey => {}
    }
    let response = request.send().await.map_err(|error| {
        ModelDiscoveryError::new(
            ModelDiscoveryFailureKind::TemporarilyUnavailable,
            format!("{operation} request to `{url}` failed: {error}"),
        )
    })?;
    let status = response.status();
    let body_result = read_bounded_model_discovery_body(response, operation, url).await;
    if !status.is_success() {
        let diagnostic = body_result
            .as_ref()
            .map(|body| controlled_error_summary(body.as_str(), secret.expose_value()))
            .unwrap_or_else(|error| error.message.clone());
        return Err(ModelDiscoveryError::from_http_status(
            status.as_u16(),
            format!(
                "{operation} request to `{url}` returned HTTP {}: {}",
                status.as_u16(),
                diagnostic
            ),
        ));
    }
    body_result
}

/// Reads one model-discovery response through a strict decoded-byte boundary.
/// 通过严格的解码字节边界读取一个模型发现响应。
///
/// # Parameters
/// 参数
/// `response` is the exact upstream response, while `operation` and `url` scope controlled errors.
/// `response` 是精确上游响应，`operation` 与 `url` 用于限定受控错误范围。
///
/// # Returns
/// 返回值
/// A strict UTF-8 body that fits the catalog response boundary.
/// 返回符合目录响应边界的严格 UTF-8 正文。
async fn read_bounded_model_discovery_body(
    response: reqwest::Response,
    operation: &str,
    url: &str,
) -> Result<String, ModelDiscoveryError> {
    read_bounded_utf8_response_body(
        response,
        operation,
        url,
        MODEL_DISCOVERY_MAX_RESPONSE_BYTES,
        "catalog",
    )
    .await
    .map_err(BoundedResponseReadError::into_model_discovery_error)
}

/// Reads one non-catalog provider metadata response through a strict decoded-byte boundary.
/// 通过严格的解码字节边界读取一个非目录供应商元数据响应。
///
/// # Parameters
/// 参数
/// `response` is the exact upstream response, while `operation` and `url` scope controlled errors.
/// `response` 是精确上游响应，`operation` 与 `url` 用于限定受控错误范围。
///
/// # Returns
/// 返回值
/// A strict UTF-8 body within the metadata boundary, or a controlled diagnostic.
/// 返回元数据边界内的严格 UTF-8 正文，或受控诊断。
async fn read_bounded_provider_metadata_body(
    response: reqwest::Response,
    operation: &str,
    url: &str,
) -> Result<String, String> {
    read_bounded_utf8_response_body(
        response,
        operation,
        url,
        PROVIDER_METADATA_MAX_RESPONSE_BYTES,
        "metadata",
    )
    .await
    .map_err(|error| match error {
        BoundedResponseReadError::Transport(message)
        | BoundedResponseReadError::Invalid(message) => message,
    })
}

/// Reads one decoded HTTP response without allowing content length or compression to bypass limits.
/// 读取一个解码后的 HTTP 响应，不允许内容长度或压缩绕过限制。
///
/// # Parameters
/// 参数
/// `response` is consumed exactly once; `operation`, `url`, and `boundary_label` scope diagnostics;
/// `limit` is the maximum decoded byte count.
/// `response` 仅消费一次；`operation`、`url` 与 `boundary_label` 限定诊断；`limit`
/// 是最大解码字节数。
///
/// # Returns
/// 返回值
/// A strict UTF-8 body or a typed transport/invalid-response failure.
/// 返回严格 UTF-8 正文，或强类型传输/无效响应失败。
async fn read_bounded_utf8_response_body(
    mut response: reqwest::Response,
    operation: &str,
    url: &str,
    limit: usize,
    boundary_label: &str,
) -> Result<String, BoundedResponseReadError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(BoundedResponseReadError::Invalid(format!(
            "{operation} response from `{url}` exceeds the {limit}-byte {boundary_label} boundary."
        )));
    }
    // body accumulates decoded chunks, so compressed responses cannot bypass the in-memory limit.
    // body 累积解码后的数据块，因此压缩响应无法绕过内存限制。
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        BoundedResponseReadError::Transport(format!(
            "{operation} response from `{url}` failed: {error}"
        ))
    })? {
        append_bounded_response_chunk(&mut body, chunk.as_ref(), limit, operation, boundary_label)?;
    }
    String::from_utf8(body).map_err(|_| {
        BoundedResponseReadError::Invalid(format!(
            "{operation} response from `{url}` is not valid UTF-8 JSON."
        ))
    })
}

/// Appends one decoded response chunk without crossing the explicit memory boundary.
/// 在不越过显式内存边界的前提下追加一个解码响应块。
///
/// # Parameters
/// 参数
/// `target` owns accepted bytes, `chunk` is the next decoded fragment, `limit` is explicit, and
/// `operation` scopes the controlled diagnostic.
/// `target` 持有已接受字节，`chunk` 是下一段解码数据，`limit` 是显式边界，`operation`
/// 限定受控诊断范围。
///
/// # Returns
/// 返回值
/// Success after the atomic append, or an invalid-response failure before allocation growth.
/// 原子追加后返回成功；扩容前发现越界时返回无效响应失败。
#[cfg(test)]
fn append_bounded_model_discovery_chunk(
    target: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    operation: &str,
) -> Result<(), ModelDiscoveryError> {
    append_bounded_response_chunk(target, chunk, limit, operation, "catalog")
        .map_err(BoundedResponseReadError::into_model_discovery_error)
}

/// Appends one decoded response chunk without crossing the caller-owned byte boundary.
/// 在不越过调用方持有的字节边界前提下追加一个解码响应块。
///
/// # Parameters
/// 参数
/// `target` owns accepted bytes, `chunk` is the next fragment, `limit` is explicit, and the
/// operation and boundary label scope diagnostics.
/// `target` 持有已接受字节，`chunk` 是下一段数据，`limit` 是显式边界，操作与边界标签限定诊断。
///
/// # Returns
/// 返回值
/// Success after the atomic append, or an invalid-response failure before allocation growth.
/// 原子追加后返回成功；扩容前发现越界时返回无效响应失败。
fn append_bounded_response_chunk(
    target: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    operation: &str,
    boundary_label: &str,
) -> Result<(), BoundedResponseReadError> {
    if target
        .len()
        .checked_add(chunk.len())
        .is_none_or(|length| length > limit)
    {
        return Err(BoundedResponseReadError::Invalid(format!(
            "{operation} response exceeds the {limit}-byte {boundary_label} boundary."
        )));
    }
    target.extend_from_slice(chunk);
    Ok(())
}

/// Validates aggregate catalog identity and size limits after protocol parsing completes.
/// 在协议解析完成后校验目录聚合身份与大小边界。
///
/// # Parameters
/// 参数
/// `catalog` is the complete parsed result and `connection_id` is the expected credential owner.
/// `catalog` 是完整解析结果，`connection_id` 是预期凭据所有者。
///
/// # Returns
/// 返回值
/// The unchanged catalog when every bound holds, or a typed invalid-response failure.
/// 全部边界成立时返回原目录，否则返回强类型无效响应失败。
fn validate_discovered_catalog_bounds(
    catalog: ConnectionModelCatalog,
    connection_id: &str,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    if catalog.connection_id != connection_id {
        return Err(ModelDiscoveryError::invalid_response(
            "Model discovery returned a catalog for a different credential connection.",
        ));
    }
    if catalog.page_count > MODEL_DISCOVERY_MAX_PAGES {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "Model discovery exceeded the {MODEL_DISCOVERY_MAX_PAGES}-page safety boundary."
        )));
    }
    if catalog.models.len() > PROVIDER_MODEL_CATALOG_MAX_MODELS {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "Model discovery exceeded the {PROVIDER_MODEL_CATALOG_MAX_MODELS}-model safety boundary."
        )));
    }
    // model_ids rejects duplicates in non-paginated parsers as well as malformed identities.
    // model_ids 同时拒绝非分页解析器中的重复项与格式异常标识。
    let mut model_ids = BTreeSet::new();
    for (index, model) in catalog.models.iter().enumerate() {
        if model.id.trim().is_empty()
            || model.id.len() > PROVIDER_MODEL_IDENTIFIER_MAX_BYTES
            || model.id.chars().any(char::is_control)
        {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "Model discovery returned an invalid model identifier at index {index}."
            )));
        }
        if !model_ids.insert(model.id.as_str()) {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "Model discovery returned a duplicate model identifier at index {index}."
            )));
        }
        if model.display_name.len() > PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES
            || model.display_name.chars().any(char::is_control)
        {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "Model discovery returned an invalid display name at index {index}."
            )));
        }
    }
    Ok(catalog)
}

/// Appends one discovery page while rejecting duplicate model identities across pages.
/// 追加一页发现结果，同时拒绝跨页重复的模型标识。
///
/// # Parameters
/// 参数
/// `target` owns accepted models, `seen_ids` tracks exact identities, `page` is the new page, and
/// `operation` scopes the controlled diagnostic.
/// `target` 持有已接受模型，`seen_ids` 跟踪精确标识，`page` 是新分页，`operation` 限定受控诊断。
///
/// # Returns
/// 返回值
/// Success after an atomic page append, or a duplicate-model contract error without partial append.
/// 原子追加分页后返回成功；发现重复模型时不进行部分追加并返回契约错误。
fn append_unique_discovered_models(
    target: &mut Vec<ConnectionModel>,
    seen_ids: &mut BTreeSet<String>,
    page: Vec<ConnectionModel>,
    operation: &str,
) -> Result<(), ModelDiscoveryError> {
    if target
        .len()
        .checked_add(page.len())
        .is_none_or(|length| length > PROVIDER_MODEL_CATALOG_MAX_MODELS)
    {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "{operation} exceeded the {PROVIDER_MODEL_CATALOG_MAX_MODELS}-model safety boundary."
        )));
    }
    let mut page_ids = BTreeSet::new();
    for (index, model) in page.iter().enumerate() {
        if model.id.trim().is_empty()
            || model.id.len() > PROVIDER_MODEL_IDENTIFIER_MAX_BYTES
            || model.id.chars().any(char::is_control)
        {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "{operation} returned an invalid model identifier at page index {index}."
            )));
        }
        if model.display_name.len() > PROVIDER_MODEL_DISPLAY_NAME_MAX_BYTES
            || model.display_name.chars().any(char::is_control)
        {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "{operation} returned an invalid display name at page index {index}."
            )));
        }
        if seen_ids.contains(model.id.as_str()) || !page_ids.insert(model.id.clone()) {
            return Err(ModelDiscoveryError::invalid_response(format!(
                "{operation} returned duplicate model `{}` across pages.",
                model.id
            )));
        }
    }
    seen_ids.extend(page_ids);
    target.extend(page);
    Ok(())
}

/// Registers one next-page cursor and rejects every repeated value in the current traversal.
/// 登记一个下一页游标，并拒绝当前遍历中出现过的任意重复值。
///
/// # Parameters
/// 参数
/// `seen` owns prior cursors, `next` is the provider-returned cursor, and `operation` scopes errors.
/// `seen` 持有历史游标，`next` 是供应商返回的游标，`operation` 限定错误范围。
///
/// # Returns
/// 返回值
/// The unchanged cursor after first registration, or a deterministic loop diagnostic.
/// 首次登记时返回原游标；重复时返回确定性的循环诊断。
fn register_pagination_cursor(
    seen: &mut BTreeSet<String>,
    next: String,
    operation: &str,
) -> Result<String, ModelDiscoveryError> {
    if !seen.insert(next.clone()) {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "{operation} repeated pagination cursor `{next}`."
        )));
    }
    Ok(next)
}

/// Rejects an upstream catalog that continues beyond the explicit traversal boundary.
/// 拒绝超过明确遍历边界后仍继续的上游目录。
///
/// # Parameters
/// 参数
/// `page_count` is the number already consumed, `has_more` is authoritative, and `operation`
/// scopes the controlled diagnostic.
/// `page_count` 是已经消费的分页数，`has_more` 是权威继续标记，`operation` 限定受控诊断。
///
/// # Returns
/// 返回值
/// Success when traversal may stop or continue safely, otherwise an invalid-response failure.
/// 遍历可以安全停止或继续时返回成功，否则返回无效响应失败。
fn validate_pagination_boundary(
    page_count: u32,
    has_more: bool,
    operation: &str,
) -> Result<(), ModelDiscoveryError> {
    if has_more && page_count >= MODEL_DISCOVERY_MAX_PAGES {
        return Err(ModelDiscoveryError::invalid_response(format!(
            "{operation} exceeded the {MODEL_DISCOVERY_MAX_PAGES}-page safety boundary."
        )));
    }
    Ok(())
}

/// Fetches every documented Anthropic model page.
/// 获取每一页文档化 Anthropic 模型。
async fn discover_anthropic_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    secret: &CredentialExecutionSecret,
    generation: u64,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    let mut models = Vec::<ConnectionModel>::new();
    let mut model_ids = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    let mut after_id = None::<String>;
    let mut page_count = 0_u32;
    loop {
        let mut url = reqwest::Url::parse(
            format!("{}/v1/models", entry.base_url.trim_end_matches('/')).as_str(),
        )
        .map_err(|_| ModelDiscoveryError::failed("Anthropic model discovery URL is invalid."))?;
        url.query_pairs_mut().append_pair("limit", "1000");
        if let Some(cursor) = after_id.as_deref() {
            url.query_pairs_mut().append_pair("after_id", cursor);
        }
        let body = send_metadata_get(
            http,
            entry,
            url.as_str(),
            secret,
            "Anthropic model discovery",
        )
        .await?;
        let page = parse_anthropic_model_page(body.as_str(), true)
            .map_err(ModelDiscoveryError::invalid_response)?;
        page_count = page_count.saturating_add(1);
        append_unique_discovered_models(
            &mut models,
            &mut model_ids,
            page.models,
            "Anthropic model discovery",
        )?;
        validate_pagination_boundary(page_count, page.has_more, "Anthropic model discovery")?;
        if !page.has_more {
            break;
        }
        let next = page.last_id.ok_or_else(|| {
            ModelDiscoveryError::invalid_response(
                "Anthropic model discovery reported more pages without `last_id`.",
            )
        })?;
        after_id = Some(register_pagination_cursor(
            &mut seen_cursors,
            next,
            "Anthropic model discovery",
        )?);
    }
    Ok(build_anthropic_model_catalog(
        connection.id.as_str(),
        generation,
        models,
        page_count,
        "anthropic.models.v1",
    ))
}

/// Fetches every Anthropic-compatible model page relative to a versioned base URL.
/// 相对于带版本基础 URL 获取每一页 Anthropic 兼容模型。
///
/// # Parameters
/// 参数
/// `http`, `entry`, `connection`, and `secret` define the exact credential request;
/// `generation` identifies the in-memory revision and `discovery_url` is the reviewed first-page URL.
/// `http`、`entry`、`connection` 与 `secret` 定义精确凭据请求；
/// `generation` 标识内存修订，`discovery_url` 是已审核的第一页地址。
///
/// # Returns
/// 返回值
/// A complete credential-scoped catalog or one typed discovery failure.
/// 返回完整的凭据作用域目录或一个强类型发现失败。
async fn discover_anthropic_compatible_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    secret: &CredentialExecutionSecret,
    generation: u64,
    discovery_url: &str,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    let mut models = Vec::<ConnectionModel>::new();
    let mut model_ids = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    let mut after_id = None::<String>;
    let mut page_count = 0_u32;
    loop {
        let mut url = reqwest::Url::parse(discovery_url).map_err(|_| {
            ModelDiscoveryError::failed("Anthropic-compatible model discovery URL is invalid.")
        })?;
        url.query_pairs_mut().append_pair("limit", "1000");
        if let Some(cursor) = after_id.as_deref() {
            url.query_pairs_mut().append_pair("after_id", cursor);
        }
        let body = send_metadata_get(
            http,
            entry,
            url.as_str(),
            secret,
            "Anthropic-compatible model discovery",
        )
        .await?;
        let page = parse_anthropic_model_page(body.as_str(), false)
            .map_err(ModelDiscoveryError::invalid_response)?;
        page_count = page_count.saturating_add(1);
        append_unique_discovered_models(
            &mut models,
            &mut model_ids,
            page.models,
            "Anthropic-compatible model discovery",
        )?;
        validate_pagination_boundary(
            page_count,
            page.has_more,
            "Anthropic-compatible model discovery",
        )?;
        if !page.has_more {
            break;
        }
        let next = page.last_id.ok_or_else(|| {
            ModelDiscoveryError::invalid_response(
                "Anthropic-compatible model discovery reported more pages without `last_id`.",
            )
        })?;
        after_id = Some(register_pagination_cursor(
            &mut seen_cursors,
            next,
            "Anthropic-compatible model discovery",
        )?);
    }
    Ok(build_anthropic_model_catalog(
        connection.id.as_str(),
        generation,
        models,
        page_count,
        "anthropic-compatible.models.v1",
    ))
}

/// Fetches every documented Gemini model page.
/// 获取每一页文档化 Gemini 模型。
async fn discover_gemini_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    secret: &CredentialExecutionSecret,
    generation: u64,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    let mut models = Vec::<ConnectionModel>::new();
    let mut model_ids = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    let mut page_token = None::<String>;
    let mut page_count = 0_u32;
    loop {
        let mut url = reqwest::Url::parse(
            format!("{}/models", entry.base_url.trim_end_matches('/')).as_str(),
        )
        .map_err(|_| ModelDiscoveryError::failed("Gemini model discovery URL is invalid."))?;
        url.query_pairs_mut().append_pair("pageSize", "1000");
        if let Some(token) = page_token.as_deref() {
            url.query_pairs_mut().append_pair("pageToken", token);
        }
        let body =
            send_metadata_get(http, entry, url.as_str(), secret, "Gemini model discovery").await?;
        let page = parse_gemini_model_page(body.as_str())
            .map_err(ModelDiscoveryError::invalid_response)?;
        page_count = page_count.saturating_add(1);
        append_unique_discovered_models(
            &mut models,
            &mut model_ids,
            page.models,
            "Gemini model discovery",
        )?;
        validate_pagination_boundary(
            page_count,
            page.next_page_token
                .as_ref()
                .is_some_and(|value| !value.is_empty()),
            "Gemini model discovery",
        )?;
        let Some(next) = page.next_page_token.filter(|value| !value.is_empty()) else {
            break;
        };
        page_token = Some(register_pagination_cursor(
            &mut seen_cursors,
            next,
            "Gemini model discovery",
        )?);
    }
    Ok(build_gemini_model_catalog(
        connection.id.as_str(),
        generation,
        models,
        page_count,
    ))
}

/// Queries OpenRouter endpoint metadata for one exact model.
/// 查询一个精确模型的 OpenRouter 端点元数据。
///
/// # Parameters
/// 参数
/// `http` is the shared client; `entry` supplies the configured base URL; `model_id` is exact;
/// `secret` is the selected credential.
/// `http` 是共享客户端；`entry` 提供配置基址；`model_id` 是精确模型标识；
/// `secret` 是选定凭据。
///
/// # Returns
/// 返回值
/// Stable, sorted, de-duplicated endpoint options.
/// 返回稳定排序并去重的端点选项。
async fn query_openrouter_model_endpoints(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    model_id: &str,
    secret: &str,
) -> Result<Vec<SecondaryRouteOption>, String> {
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return Err(String::from(
            "OpenRouter secondary route discovery requires an exact model id.",
        ));
    }
    let url = format!(
        "{}/models/{model_id}/endpoints",
        entry.base_url.trim_end_matches('/')
    );
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            format!("OpenRouter secondary route request to `{url}` failed: {error}")
        })?;
    let status = response.status();
    let body = read_bounded_provider_metadata_body(
        response,
        "OpenRouter secondary route request",
        url.as_str(),
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "OpenRouter secondary route request to `{url}` returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    let payload = serde_json::from_str::<OpenRouterModelEndpointsResponse>(&body)
        .map_err(|error| format!("OpenRouter secondary route response is invalid: {error}"))?;
    let mut options = payload
        .data
        .endpoints
        .into_iter()
        .filter_map(openrouter_endpoint_option)
        .collect::<Vec<_>>();
    options.sort_by(|left, right| {
        left.provider_display_name
            .cmp(&right.provider_display_name)
            .then_with(|| left.route_id.cmp(&right.route_id))
    });
    options.dedup_by(|left, right| left.route_id == right.route_id);
    Ok(options)
}

/// Converts one OpenRouter endpoint record into the public secondary-route contract.
/// 将一条 OpenRouter 端点记录转换为公共二级路由契约。
///
/// # Parameters
/// 参数
/// `record` is one provider response row.
/// `record` 是一条供应商响应记录。
///
/// # Returns
/// 返回值
/// A normalized option when the stable tag exists, otherwise `None`.
/// 稳定标签存在时返回归一化选项，否则返回 `None`。
fn openrouter_endpoint_option(
    record: OpenRouterModelEndpointRecord,
) -> Option<SecondaryRouteOption> {
    let route_id = record.tag?.trim().to_string();
    if route_id.is_empty() {
        return None;
    }
    let provider_display_name = non_empty_text(record.provider_name.as_deref())
        .unwrap_or(route_id.as_str())
        .to_string();
    let endpoint_display_name = non_empty_text(record.name.as_deref())
        .unwrap_or(provider_display_name.as_str())
        .to_string();
    Some(SecondaryRouteOption {
        route_id,
        provider_display_name,
        endpoint_display_name,
        prompt_price_per_token: record
            .pricing
            .as_ref()
            .and_then(|value| value.prompt.clone()),
        completion_price_per_token: record
            .pricing
            .as_ref()
            .and_then(|value| value.completion.clone()),
        cache_read_price_per_token: record
            .pricing
            .as_ref()
            .and_then(|value| value.input_cache_read.clone()),
        latency_milliseconds: openrouter_metric(record.latency_last_30m.as_ref()),
        uptime_percent: openrouter_metric(record.uptime_last_30m.as_ref()),
        throughput_tokens_per_second: openrouter_metric(record.throughput_last_30m.as_ref()),
    })
}

/// Returns a trimmed non-empty text reference.
/// 返回去除首尾空白后的非空文本引用。
fn non_empty_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Reads one OpenRouter scalar or percentile metric.
/// 读取一个 OpenRouter 标量或百分位指标。
fn openrouter_metric(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => text.trim().parse::<f64>().ok(),
        Some(Value::Object(object)) => object
            .get("p50")
            .or_else(|| object.get("median"))
            .and_then(|value| openrouter_metric(Some(value))),
        _ => None,
    }
}

/// Queries the documented MiniMax Token Plan quota endpoint.
/// 查询文档化的 MiniMax Token Plan 配额端点。
async fn query_minimax_token_plan(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let url = format!(
        "{}/v1/token_plan/remains",
        entry.base_url.trim_end_matches('/')
    );
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("MiniMax Token Plan usage request failed: {error}"))?;
    let status = response.status();
    let body =
        read_bounded_provider_metadata_body(response, "MiniMax Token Plan usage", url.as_str())
            .await?;
    if !status.is_success() {
        return Err(format!(
            "MiniMax Token Plan usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    let response: MiniMaxQuotaResponse = serde_json::from_str(body.as_str())
        .map_err(|error| format!("MiniMax Token Plan usage JSON is invalid: {error}"))?;
    if response.base_resp.status_code != 0 {
        return Err(format!(
            "MiniMax Token Plan usage returned business status {}: {}",
            response.base_resp.status_code, response.base_resp.status_msg
        ));
    }
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(String::from("Token Plan")),
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: minimax_quota_windows(response.model_remains)?,
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Queries Kimi Coding Plan membership and rolling usage with the verified CLI identity.
/// 使用已验证 CLI 身份查询 Kimi Coding Plan 会员与滚动用量。
async fn query_kimi_coding_plan(
    http: &reqwest::Client,
    secret: &str,
    device_id: Option<&str>,
) -> Result<AccountUsageSnapshot, String> {
    // url is the verified fixed Kimi Coding Plan usage endpoint.
    // url 是已验证的固定 Kimi Coding Plan 用量入口。
    let url = "https://api.kimi.com/coding/v1/usages";
    let response = http
        .get(url)
        .bearer_auth(secret)
        .header("user-agent", "KimiCLI/1.10.6")
        .header("x-msh-platform", "kimi_cli")
        .header("x-msh-version", "1.10.6")
        .header("x-msh-device-name", kimi_device_name())
        .header(
            "x-msh-device-model",
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        )
        .header(
            "x-msh-device-id",
            device_id.unwrap_or("cli-proxy-api-device"),
        )
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("Kimi Coding Plan usage request failed: {error}"))?;
    let status = response.status();
    let body = read_bounded_provider_metadata_body(response, "Kimi Coding Plan usage", url).await?;
    if !status.is_success() {
        return Err(format!(
            "Kimi Coding Plan usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    decode_kimi_coding_plan_usage(body.as_str())
}

/// Decodes one exact Kimi Coding Plan usage response into provider-neutral windows.
/// 将一份精确 Kimi Coding Plan 用量响应解码为供应商无关窗口。
fn decode_kimi_coding_plan_usage(body: &str) -> Result<AccountUsageSnapshot, String> {
    let response: KimiUsageResponse = serde_json::from_str(body)
        .map_err(|error| format!("Kimi Coding Plan usage JSON is invalid: {error}"))?;
    let mut windows =
        Vec::with_capacity(response.limits.len() + usize::from(response.usage.is_some()));
    if let Some(usage) = response.usage
        && let Some(window) = kimi_usage_window("weekly_usage", "Weekly usage", usage, None)?
    {
        windows.push(window);
    }
    for (index, item) in response.limits.into_iter().enumerate() {
        let name = [
            item.name.as_deref(),
            item.title.as_deref(),
            item.scope.as_deref(),
            item.detail
                .as_ref()
                .and_then(|detail| detail.name.as_deref()),
            item.detail
                .as_ref()
                .and_then(|detail| detail.title.as_deref()),
        ]
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Limit {}", index + 1));
        let id = format!("kimi_{}", normalize_usage_identifier(name.as_str(), index));
        let detail = item.detail.unwrap_or(item.inline);
        if let Some(window) = kimi_usage_window(id.as_str(), name.as_str(), detail, item.window)? {
            windows.push(window);
        }
    }
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: response
            .user
            .membership
            .level
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some(String::from("Kimi Coding Plan"))),
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: windows,
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Converts one Kimi counter into a neutral window without inventing absent limits.
/// 将一个 Kimi 计数转换为中立窗口且不臆造缺失上限。
fn kimi_usage_window(
    id: &str,
    display_name: &str,
    detail: KimiUsageDetail,
    window: Option<KimiUsageWindow>,
) -> Result<Option<AccountUsageWindow>, String> {
    let used = detail.used.map(KimiDecimal::exact).transpose()?;
    let total = detail.limit.map(KimiDecimal::exact).transpose()?;
    let remaining = match detail.remaining.map(KimiDecimal::exact).transpose()? {
        Some(value) => Some(value),
        None => match (total.as_deref(), used.as_deref()) {
            (Some(total), Some(used)) => Some(subtract_non_negative_decimal(total, used)?),
            _ => None,
        },
    };
    if used.is_none() && total.is_none() && remaining.is_none() {
        return Ok(None);
    }
    let reset = [
        detail.reset_at,
        detail.reset_at_snake,
        detail.reset_time,
        detail.reset_time_snake,
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.trim().is_empty());
    let bucket_name = window.map(|window| {
        format!(
            "{} {}",
            window
                .duration
                .map_or_else(String::new, KimiDecimal::lossless),
            window.time_unit.unwrap_or_default()
        )
        .trim()
        .to_string()
    });
    Ok(Some(AccountUsageWindow {
        id: id.to_string(),
        display_name: display_name.to_string(),
        bucket_name: bucket_name.filter(|value| !value.is_empty()),
        used,
        remaining,
        total,
        remaining_percent: None,
        unit: String::from("provider_defined"),
        starts_at: None,
        resets_at: reset,
        unlimited: false,
        available: true,
    }))
}

/// Returns a stable UI-safe identifier from one provider metric label.
/// 从一个供应商指标标签返回稳定且适合 UI 的标识。
fn normalize_usage_identifier(value: &str, fallback_index: usize) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if normalized.is_empty() {
        format!("limit_{}", fallback_index + 1)
    } else {
        normalized
    }
}

/// Resolves the exact current device name used in Kimi metadata headers.
/// 解析 Kimi 元数据请求头使用的精确当前设备名称。
fn kimi_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| String::from("unknown"))
}

/// 使用管理密钥查询文档化的 OpenRouter 余额端点。
/// Queries the documented Tavily account and API-key credit usage endpoint.
/// 查询 Tavily 官方文档定义的账号与 API Key Credit 用量端点。
///
/// # Parameters
/// 参数
/// `http` is the shared metadata client, `entry` owns the exact service endpoint,
/// and `secret` is the credential being validated.
/// `http` 是共享元数据客户端，`entry` 持有精确服务端点，`secret` 是待验证凭据。
///
/// # Returns
/// 返回值
/// A provider-neutral usage snapshot that preserves Tavily's integer credit accounting.
/// 返回保留 Tavily 整数 Credit 计量语义的供应商无关用量快照。
async fn query_tavily_credits(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let url = format!("{}/usage", entry.base_url.trim_end_matches('/'));
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("Tavily account usage request failed: {error}"))?;
    let status = response.status();
    let body =
        read_bounded_provider_metadata_body(response, "Tavily account usage", url.as_str()).await?;
    if !status.is_success() {
        return Err(format!(
            "Tavily account usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    let response: TavilyUsageResponse = serde_json::from_str(body.as_str())
        .map_err(|error| format!("Tavily account usage JSON is invalid: {error}"))?;
    let account_plan = tavily_usage_window(
        "tavily-account-plan",
        "Account plan",
        response.account.plan_usage,
        response.account.plan_limit,
        false,
    )?;
    let account_paygo = tavily_usage_window(
        "tavily-account-paygo",
        "Pay as you go",
        response.account.paygo_usage,
        response.account.paygo_limit,
        response.account.paygo_limit.is_none(),
    )?;
    let key_total = tavily_usage_window(
        "tavily-api-key",
        "API key",
        response.key.usage,
        response.key.limit,
        false,
    )?;
    let account_remaining = account_plan.remaining.clone();
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(response.account.current_plan),
        used_amount: Some(response.account.plan_usage.to_string()),
        remaining_amount: account_remaining,
        currency: None,
        balance_amount: None,
        quota_windows: vec![
            key_total,
            account_plan,
            account_paygo,
            tavily_usage_window(
                "tavily-key-search",
                "API key search",
                response.key.search_usage,
                None,
                false,
            )?,
            tavily_usage_window(
                "tavily-key-extract",
                "API key extract",
                response.key.extract_usage,
                None,
                false,
            )?,
            tavily_usage_window(
                "tavily-account-search",
                "Account search",
                response.account.search_usage,
                None,
                false,
            )?,
            tavily_usage_window(
                "tavily-account-extract",
                "Account extract",
                response.account.extract_usage,
                None,
                false,
            )?,
        ],
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Builds one strict Tavily credit window without interpreting a missing limit as zero.
/// 构建一个严格的 Tavily Credit 窗口，且不会把缺失上限解释为零。
///
/// # Parameters
/// 参数
/// The identifiers describe the UI row, `used` and `limit` are provider counters,
/// and `unlimited` is true only for the documented unbounded PAYGO case.
/// 标识描述 UI 行，`used` 与 `limit` 是供应商计数，`unlimited` 仅用于文档明确的无界 PAYGO 情况。
///
/// # Returns
/// 返回值
/// A normalized usage window or an error when usage exceeds a reported finite limit.
/// 返回归一化用量窗口；若用量超过供应商报告的有限上限则返回错误。
fn tavily_usage_window(
    id: &str,
    display_name: &str,
    used: u64,
    limit: Option<u64>,
    unlimited: bool,
) -> Result<AccountUsageWindow, String> {
    let remaining = match limit {
        Some(limit) if used <= limit => Some((limit - used).to_string()),
        Some(limit) => {
            return Err(format!(
                "Tavily usage `{id}` reports used credits {used} above limit {limit}."
            ));
        }
        None => None,
    };
    Ok(AccountUsageWindow {
        id: id.to_string(),
        display_name: display_name.to_string(),
        bucket_name: None,
        used: Some(used.to_string()),
        remaining,
        total: limit.map(|value| value.to_string()),
        remaining_percent: None,
        unit: String::from("credits"),
        starts_at: None,
        resets_at: None,
        unlimited,
        available: true,
    })
}

/// Queries the documented OpenRouter credits endpoint with a management key.
/// 使用管理密钥查询 OpenRouter 官方文档定义的余额端点。
///
/// # Parameters
/// 参数
/// `http` is the shared metadata client, `entry` owns the exact service endpoint,
/// and `secret` is the management credential used by the credits API.
/// `http` 是共享元数据客户端，`entry` 持有精确服务端点，`secret` 是余额 API 使用的管理凭据。
///
/// # Returns
/// 返回值
/// A provider-neutral account balance snapshot that retains decimal precision.
/// 返回保留十进制精度的供应商无关账号余额快照。
async fn query_openrouter_credits(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let url = format!("{}/credits", entry.base_url.trim_end_matches('/'));
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("OpenRouter account usage request failed: {error}"))?;
    let status = response.status();
    let body =
        read_bounded_provider_metadata_body(response, "OpenRouter account usage", url.as_str())
            .await?;
    if !status.is_success() {
        return Err(format!(
            "OpenRouter account usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    let response: OpenRouterCreditsResponse = serde_json::from_str(body.as_str())
        .map_err(|error| format!("OpenRouter account usage JSON is invalid: {error}"))?;
    let total_credits = response.data.total_credits.to_string();
    let total_usage = response.data.total_usage.to_string();
    let remaining = subtract_non_negative_decimal(total_credits.as_str(), total_usage.as_str())?;
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(String::from("OpenRouter Credits")),
        used_amount: Some(total_usage),
        remaining_amount: Some(remaining.clone()),
        currency: Some(String::from("USD")),
        balance_amount: Some(remaining),
        quota_windows: Vec::new(),
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Queries the official DeepSeek account-balance endpoint.
/// 查询 DeepSeek 官方账号余额端点。
///
/// # Parameters
/// 参数
/// `http` is the shared metadata client, `entry` owns the official origin, and
/// `secret` is the exact API key being observed.
/// `http` 是共享元数据客户端，`entry` 持有官方 Origin，`secret` 是被观测的精确 API Key。
///
/// # Returns
/// 返回值
/// A lossless provider-neutral snapshot retaining every provider-reported currency.
/// 返回保留供应商报告全部币种的无损供应商无关快照。
async fn query_deepseek_balance(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let url = format!("{}/user/balance", entry.base_url.trim_end_matches('/'));
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("DeepSeek account balance request failed: {error}"))?;
    let status = response.status();
    let body =
        read_bounded_provider_metadata_body(response, "DeepSeek account balance", url.as_str())
            .await?;
    if !status.is_success() {
        return Err(format!(
            "DeepSeek account balance returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    decode_deepseek_balance(body.as_str())
}

/// Decodes one DeepSeek balance response without losing currency or decimal precision.
/// 解码一份 DeepSeek 余额响应且不丢失币种或十进制精度。
///
/// # Parameters
/// 参数
/// `body` is the complete successful provider JSON response.
/// `body` 是完整的供应商成功 JSON 响应。
///
/// # Returns
/// 返回值
/// One provider-neutral balance snapshot.
/// 返回一份供应商无关余额快照。
fn decode_deepseek_balance(body: &str) -> Result<AccountUsageSnapshot, String> {
    let response: DeepSeekBalanceResponse = serde_json::from_str(body)
        .map_err(|error| format!("DeepSeek account balance JSON is invalid: {error}"))?;
    if response.balance_infos.is_empty() {
        return Err(String::from(
            "DeepSeek account balance omitted its currency balances.",
        ));
    }
    for balance in &response.balance_infos {
        validate_deepseek_decimal(balance.total_balance.as_str())?;
        validate_deepseek_decimal(balance.granted_balance.as_str())?;
        validate_deepseek_decimal(balance.topped_up_balance.as_str())?;
        if !matches!(balance.currency.as_str(), "CNY" | "USD") {
            return Err(String::from(
                "DeepSeek account balance returned an unsupported currency.",
            ));
        }
    }
    let single_balance = (response.balance_infos.len() == 1).then(|| &response.balance_infos[0]);
    let quota_windows = response
        .balance_infos
        .iter()
        .map(|balance| AccountUsageWindow {
            id: format!("balance_{}", balance.currency.to_ascii_lowercase()),
            display_name: format!("{} balance", balance.currency),
            bucket_name: Some(String::from("Account balance")),
            used: None,
            remaining: Some(balance.total_balance.clone()),
            total: None,
            remaining_percent: None,
            unit: balance.currency.clone(),
            starts_at: None,
            resets_at: None,
            unlimited: false,
            available: response.is_available,
        })
        .collect();
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(String::from("DeepSeek Balance")),
        used_amount: None,
        remaining_amount: single_balance.map(|balance| balance.total_balance.clone()),
        currency: single_balance.map(|balance| balance.currency.clone()),
        balance_amount: single_balance.map(|balance| balance.total_balance.clone()),
        quota_windows,
        observed_at: current_unix_milliseconds(),
        refresh_message: (!response.is_available)
            .then(|| String::from("DeepSeek reports that the account balance is unavailable.")),
    })
}

/// Validates one non-negative DeepSeek decimal string without converting it to floating point.
/// 在不转换为浮点数的情况下验证一个非负 DeepSeek 十进制字符串。
///
/// # Parameters
/// 参数
/// `value` is one provider-reported exact balance component.
/// `value` 是供应商报告的一个精确余额分量。
///
/// # Returns
/// 返回值
/// Success for one canonical non-negative decimal string.
/// 对规范非负十进制字符串返回成功。
fn validate_deepseek_decimal(value: &str) -> Result<(), String> {
    normalized_unsigned_decimal_digits(value)
        .map(|_| ())
        .map_err(|_| String::from("DeepSeek account balance contains an invalid decimal value."))
}

/// Discovers every MiniMax model page through the documented `after_id` cursor.
/// 通过文档化的 `after_id` 游标发现全部 MiniMax 模型分页。
///
/// # Parameters
/// 参数
/// `http` is the shared client, `entry` owns the endpoint, `connection` owns the catalog,
/// `authentication` is the resolved profile projection, `secret` is the exact credential,
/// and `generation` identifies this in-memory refresh.
/// `http` 是共享客户端，`entry` 持有端点，`connection` 持有目录，
/// `authentication` 是已解析的配置档投影，`secret` 是精确凭据，
/// `generation` 标识本次内存刷新。
///
/// # Returns
/// 返回值
/// A complete memory-only catalog, or a controlled pagination/HTTP contract error.
/// 返回完整的纯内存目录，或受控的分页/HTTP 契约错误。
async fn discover_minimax_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    authentication: AuthenticationProjection,
    secret: &CredentialExecutionSecret,
    generation: u64,
) -> Result<ConnectionModelCatalog, ModelDiscoveryError> {
    let url = format!(
        "{}/anthropic/v1/models",
        entry.base_url.trim_end_matches('/')
    );
    let mut after_id: Option<String> = None;
    let mut models = Vec::new();
    let mut model_ids = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    let mut page_count = 0_u32;
    loop {
        let mut page_url = reqwest::Url::parse(url.as_str()).map_err(|error| {
            ModelDiscoveryError::failed(format!("MiniMax model discovery URL is invalid: {error}"))
        })?;
        if let Some(cursor) = after_id.as_deref() {
            page_url.query_pairs_mut().append_pair("after_id", cursor);
        }
        let body = send_metadata_get_with_authentication(
            http,
            entry,
            authentication,
            page_url.as_str(),
            secret,
            "MiniMax model discovery",
        )
        .await?;
        let page = parse_minimax_model_page(body.as_str())
            .map_err(ModelDiscoveryError::invalid_response)?;
        page_count = page_count.saturating_add(1);
        append_unique_discovered_models(
            &mut models,
            &mut model_ids,
            page.models,
            "MiniMax model discovery",
        )?;
        validate_pagination_boundary(page_count, page.has_more, "MiniMax model discovery")?;
        if !page.has_more {
            break;
        }
        let next_cursor = page
            .last_id
            .filter(|cursor| !cursor.trim().is_empty())
            .ok_or_else(|| {
                ModelDiscoveryError::invalid_response(
                    "MiniMax model discovery reported `has_more=true` without a `last_id` cursor.",
                )
            })?;
        after_id = Some(register_pagination_cursor(
            &mut seen_cursors,
            next_cursor,
            "MiniMax model discovery",
        )?);
    }
    Ok(build_minimax_model_catalog(
        connection.id.as_str(),
        generation,
        models,
        page_count,
    ))
}

/// Builds an explicit unsupported usage snapshot.
/// 构建明确的不支持用量快照。
fn unsupported_usage(credential_id: &str) -> AccountUsageSnapshot {
    AccountUsageSnapshot {
        credential_id: credential_id.to_string(),
        availability: AccountUsageAvailability::Unsupported,
        plan_name: None,
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: Vec::new(),
        observed_at: None,
        refresh_message: None,
    }
}

/// Normalizes MiniMax model buckets into explicit current and weekly windows.
/// 把 MiniMax 模型额度桶归一化为明确的当前与每周窗口。
///
/// # Parameters
/// 参数
/// `models` is the strict response collection mirrored from the official MiniMax CLI.
/// `models` 是从 MiniMax 官方 CLI 镜像的严格响应集合。
///
/// # Returns
/// 返回值
/// Exact count windows, or a controlled invariant error.
/// 返回精确次数窗口，或受控不变量错误。
fn minimax_quota_windows(
    models: Vec<MiniMaxQuotaModelRemain>,
) -> Result<Vec<AccountUsageWindow>, String> {
    let mut windows = Vec::with_capacity(models.len().saturating_mul(2));
    for model in models {
        let unavailable = model.current_interval_total_count == 0
            && model.current_weekly_total_count == 0
            && model.current_interval_status == Some(3)
            && model.current_weekly_status == Some(3);
        windows.push(minimax_quota_window(MiniMaxQuotaWindowInput {
            id: format!("{}:current", model.model_name),
            display_name: String::from("Current rolling window"),
            bucket_name: model.model_name.clone(),
            total: model.current_interval_total_count,
            remaining: model.current_interval_usage_count,
            remaining_percent: model.current_interval_remaining_percent,
            starts_at: model.start_time,
            resets_at: model.end_time,
            unlimited: false,
            available: !unavailable,
        })?);
        windows.push(minimax_quota_window(MiniMaxQuotaWindowInput {
            id: format!("{}:weekly", model.model_name),
            display_name: String::from("Weekly window"),
            bucket_name: model.model_name,
            total: model.current_weekly_total_count,
            remaining: model.current_weekly_usage_count,
            remaining_percent: model.current_weekly_remaining_percent,
            starts_at: model.weekly_start_time,
            resets_at: model.weekly_end_time,
            unlimited: !unavailable && model.current_weekly_status == Some(3),
            available: !unavailable,
        })?);
    }
    Ok(windows)
}

/// Carries the exact source fields for one MiniMax quota window.
/// 承载一个 MiniMax 额度窗口的精确来源字段。
struct MiniMaxQuotaWindowInput {
    /// Stable normalized window identifier.
    /// 稳定的归一化窗口标识。
    id: String,
    /// User-visible window name.
    /// 用户可见窗口名称。
    display_name: String,
    /// Provider model bucket name.
    /// 供应商模型额度桶名称。
    bucket_name: String,
    /// Provider-reported total request count.
    /// 供应商报告的请求总次数。
    total: u64,
    /// Provider-reported remaining request count.
    /// 供应商报告的剩余请求次数。
    remaining: u64,
    /// Provider-reported remaining percentage.
    /// 供应商报告的剩余百分比。
    remaining_percent: Option<u64>,
    /// Exact provider-reported window start.
    /// 供应商报告的精确窗口起始时间。
    starts_at: u64,
    /// Exact provider-reported window reset time.
    /// 供应商报告的精确窗口重置时间。
    resets_at: u64,
    /// Whether the provider marks this window as unlimited.
    /// 供应商是否将此窗口标记为无限。
    unlimited: bool,
    /// Whether this bucket is included in the current account plan.
    /// 当前账号套餐是否包含该额度桶。
    available: bool,
}

/// Builds one exact MiniMax count window.
/// 构建一个精确 MiniMax 次数窗口。
///
/// # Parameters
/// 参数
/// `input` contains the exact fields reported for one quota bucket.
/// `input` 包含一个额度桶报告的精确字段。
///
/// # Returns
/// 返回值
/// The normalized account window or a controlled invariant failure.
/// 返回归一化账号窗口或受控不变量错误。
fn minimax_quota_window(input: MiniMaxQuotaWindowInput) -> Result<AccountUsageWindow, String> {
    if !input.unlimited && input.remaining > input.total {
        return Err(format!(
            "MiniMax quota window `{}` reports remaining count above its total.",
            input.id
        ));
    }
    Ok(AccountUsageWindow {
        id: input.id,
        display_name: input.display_name,
        bucket_name: Some(input.bucket_name),
        used: (input.available && !input.unlimited)
            .then(|| input.total.saturating_sub(input.remaining).to_string()),
        remaining: (input.available && !input.unlimited).then(|| input.remaining.to_string()),
        total: (input.available && !input.unlimited).then(|| input.total.to_string()),
        remaining_percent: input
            .available
            .then_some(input.remaining_percent)
            .flatten()
            .map(|value| value.to_string()),
        unit: String::from("requests"),
        starts_at: Some(input.starts_at.to_string()),
        resets_at: Some(input.resets_at.to_string()),
        unlimited: input.unlimited,
        available: input.available,
    })
}

/// Returns the current Unix timestamp in milliseconds without manufacturing an epoch value.
/// 返回当前 Unix 毫秒时间戳，且不会伪造纪元零值。
///
/// # Returns
/// 返回值
/// The exact timestamp when the system clock is valid, otherwise `None`.
/// 系统时钟有效时返回精确时间戳，否则返回 `None`。
fn current_unix_milliseconds() -> Option<String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .ok()
}

/// Extracts registered diagnostic fields without exposing one raw upstream response body.
/// 提取已登记诊断字段且不公开原始上游响应正文。
///
/// # Parameters
/// 参数
/// `body` is the exact provider response and `sensitive_value` is the credential used by the request.
/// `body` 是精确供应商响应，`sensitive_value` 是请求所使用的凭据。
///
/// # Returns
/// 返回值
/// A bounded structured summary with the exact credential redacted, or a fixed omission notice.
/// 返回已精确遮盖凭据的有界结构化摘要；无法识别时返回固定省略提示。
fn controlled_error_summary(body: &str, sensitive_value: &str) -> String {
    // payload accepts only structured JSON before any provider text may enter a public diagnostic.
    // payload 只接受结构化 JSON，之后供应商文本才可能进入公开诊断。
    let Ok(payload) = serde_json::from_str::<Value>(body) else {
        return String::from("Provider returned an unstructured error response; body omitted.");
    };
    // diagnostics preserves only the explicitly registered common error fields in stable order.
    // diagnostics 只按稳定顺序保留显式登记的公共错误字段。
    let mut diagnostics = Vec::new();
    if let Some(error) = payload.get("error") {
        match error {
            Value::String(message) => diagnostics.push(format!("error={message}")),
            Value::Object(fields) => {
                append_controlled_error_field(&mut diagnostics, "error.code", fields.get("code"));
                append_controlled_error_field(&mut diagnostics, "error.type", fields.get("type"));
                append_controlled_error_field(
                    &mut diagnostics,
                    "error.message",
                    fields.get("message"),
                );
            }
            _ => {}
        }
    }
    append_controlled_error_field(&mut diagnostics, "code", payload.get("code"));
    append_controlled_error_field(&mut diagnostics, "type", payload.get("type"));
    append_controlled_error_field(&mut diagnostics, "message", payload.get("message"));
    // summary never falls back to serialized JSON, so unregistered response fields remain private.
    // summary 绝不回退到序列化 JSON，因此未登记响应字段保持私密。
    let summary = if diagnostics.is_empty() {
        String::from(
            "Provider returned a structured error response without registered diagnostics; body omitted.",
        )
    } else {
        diagnostics.join(" | ")
    };
    // redacted removes the exact request credential before applying the public length boundary.
    // redacted 在应用公开长度边界前移除精确请求凭据。
    let redacted = if sensitive_value.is_empty() {
        summary
    } else {
        summary.replace(sensitive_value, "[REDACTED]")
    };
    redacted.chars().take(2_000).collect()
}

/// Appends one registered scalar error field to a controlled diagnostic summary.
/// 向受控诊断摘要追加一个已登记的标量错误字段。
///
/// # Parameters
/// 参数
/// `target` owns ordered diagnostic fragments, `label` is code-owned, and `value` is one JSON field.
/// `target` 持有有序诊断片段，`label` 由代码持有，`value` 是一个 JSON 字段。
///
/// # Returns
/// 返回值
/// This helper mutates `target` only when `value` is a non-empty scalar.
/// 仅当 `value` 是非空标量时，此辅助函数才修改 `target`。
fn append_controlled_error_field(target: &mut Vec<String>, label: &str, value: Option<&Value>) {
    // text accepts only scalar JSON values; objects and arrays are intentionally omitted.
    // text 只接受 JSON 标量值；对象与数组会被有意省略。
    let text = match value {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(boolean)) => Some(boolean.to_string()),
        _ => None,
    };
    if let Some(text) = text.filter(|text| !text.trim().is_empty()) {
        target.push(format!("{label}={text}"));
    }
}

/// DeepSeek official account balance response.
/// DeepSeek 官方账号余额响应。
#[derive(Debug, Deserialize)]
struct DeepSeekBalanceResponse {
    /// Whether the provider reports usable account balance.
    /// 供应商是否报告账号余额可用。
    is_available: bool,
    /// Exact balances grouped by provider currency.
    /// 按供应商币种分组的精确余额。
    balance_infos: Vec<DeepSeekBalanceInfo>,
}

/// One DeepSeek currency balance.
/// 一项 DeepSeek 币种余额。
#[derive(Debug, Deserialize)]
struct DeepSeekBalanceInfo {
    /// Provider currency code.
    /// 供应商币种代码。
    currency: String,
    /// Total available balance.
    /// 总可用余额。
    total_balance: String,
    /// Unexpired granted balance.
    /// 未过期赠金余额。
    granted_balance: String,
    /// Topped-up balance.
    /// 充值余额。
    topped_up_balance: String,
}

/// Tavily usage response containing API-key and account-level credit counters.
/// Tavily 用量响应，包含 API Key 与账号级 Credit 计数。
#[derive(Debug, Deserialize)]
struct TavilyUsageResponse {
    /// Usage counters scoped to the validated API key.
    /// 绑定到待验证 API Key 的用量计数。
    key: TavilyKeyUsage,
    /// Usage counters and plan metadata scoped to the owning account.
    /// 绑定到所属账号的用量计数与套餐元数据。
    account: TavilyAccountUsage,
}

/// Tavily API-key credit counters required by the public usage card.
/// 公共用量卡片所需的 Tavily API Key Credit 计数。
#[derive(Debug, Deserialize)]
struct TavilyKeyUsage {
    /// Credits consumed by this API key.
    /// 此 API Key 已消耗的 Credit。
    usage: u64,
    /// Credit limit assigned to this API key.
    /// 分配给此 API Key 的 Credit 上限。
    limit: Option<u64>,
    /// Search credits consumed by this API key.
    /// 此 API Key 已消耗的搜索 Credit。
    search_usage: u64,
    /// Extraction credits consumed by this API key.
    /// 此 API Key 已消耗的提取 Credit。
    extract_usage: u64,
}

/// Tavily account plan metadata required by the public usage card.
/// 公共用量卡片所需的 Tavily 账号套餐元数据。
#[derive(Debug, Deserialize)]
struct TavilyAccountUsage {
    /// Current provider plan display name.
    /// 当前供应商套餐显示名称。
    current_plan: String,
    /// Credits consumed from the current plan.
    /// 当前套餐已经消耗的 Credit。
    plan_usage: u64,
    /// Total credits provided by the current plan.
    /// 当前套餐提供的 Credit 总量。
    plan_limit: Option<u64>,
    /// Pay-as-you-go credits consumed by the account.
    /// 账号已消耗的按量付费 Credit。
    paygo_usage: u64,
    /// Optional provider-reported pay-as-you-go limit.
    /// 供应商可选报告的按量付费上限。
    paygo_limit: Option<u64>,
    /// Search credits consumed by the account.
    /// 账号已消耗的搜索 Credit。
    search_usage: u64,
    /// Extraction credits consumed by the account.
    /// 账号已消耗的提取 Credit。
    extract_usage: u64,
}

/// OpenRouter credits response.
/// OpenRouter 余额响应。
#[derive(Debug, Deserialize)]
struct OpenRouterCreditsResponse {
    /// Account credit metadata.
    /// 账号余额元数据。
    data: OpenRouterCreditsData,
}

/// OpenRouter account credit metadata used by the public usage contract.
/// 公共用量契约使用的 OpenRouter 账号余额元数据。
#[derive(Debug, Deserialize)]
struct OpenRouterCreditsData {
    /// Total purchased credits.
    /// 累计购买额度。
    total_credits: serde_json::Number,
    /// Total consumed credits.
    /// 累计消耗额度。
    total_usage: serde_json::Number,
}

/// Subtracts two non-negative JSON decimal strings without floating-point loss.
/// 在不产生浮点精度损失的情况下对两个非负 JSON 十进制字符串做减法。
fn subtract_non_negative_decimal(minuend: &str, subtrahend: &str) -> Result<String, String> {
    let (mut left, left_scale) = normalized_unsigned_decimal_digits(minuend)?;
    let (mut right, right_scale) = normalized_unsigned_decimal_digits(subtrahend)?;
    let scale = left_scale.max(right_scale);
    left.extend(std::iter::repeat_n(0, scale - left_scale));
    right.extend(std::iter::repeat_n(0, scale - right_scale));
    let width = left.len().max(right.len());
    left.splice(0..0, std::iter::repeat_n(0, width - left.len()));
    right.splice(0..0, std::iter::repeat_n(0, width - right.len()));
    if left < right {
        return Err(String::from(
            "OpenRouter credits response reports total_usage greater than total_credits.",
        ));
    }
    let mut borrow = 0_i16;
    let mut result = vec![0_u8; width];
    for index in (0..width).rev() {
        let mut digit = left[index] as i16 - borrow - right[index] as i16;
        if digit < 0 {
            digit += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result[index] = digit as u8;
    }
    format_unsigned_decimal_digits(result, scale)
}

/// Normalizes one non-negative JSON number into base-10 digits and a fractional scale.
/// 将一个非负 JSON 数字归一化为十进制数字及小数位数。
fn normalized_unsigned_decimal_digits(value: &str) -> Result<(Vec<u8>, usize), String> {
    if value.starts_with('-') {
        return Err(String::from(
            "OpenRouter credits values cannot be negative.",
        ));
    }
    let (mantissa, exponent) = match value.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (
            mantissa,
            exponent
                .parse::<i32>()
                .map_err(|_| String::from("OpenRouter credits exponent is invalid."))?,
        ),
        None => (value, 0),
    };
    let fractional_digits = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits = mantissa
        .chars()
        .filter(|character| *character != '.')
        .map(|character| {
            character
                .to_digit(10)
                .map(|digit| digit as u8)
                .ok_or_else(|| String::from("OpenRouter credits value is not a JSON decimal."))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if digits.is_empty() {
        return Err(String::from("OpenRouter credits value is empty."));
    }
    let resulting_scale = fractional_digits as i32 - exponent;
    if resulting_scale < 0 {
        let mut expanded = digits;
        expanded.extend(std::iter::repeat_n(0, (-resulting_scale) as usize));
        Ok((expanded, 0))
    } else {
        Ok((digits, resulting_scale as usize))
    }
}

/// Formats normalized decimal digits while retaining exact fractional significance.
/// 格式化归一化十进制数字并保留精确的小数有效位。
fn format_unsigned_decimal_digits(mut digits: Vec<u8>, scale: usize) -> Result<String, String> {
    while digits.len() > scale.saturating_add(1) && digits.first() == Some(&0) {
        digits.remove(0);
    }
    if digits.len() <= scale {
        digits.splice(0..0, std::iter::repeat_n(0, scale + 1 - digits.len()));
    }
    let mut text = digits
        .into_iter()
        .map(|digit| char::from(b'0' + digit))
        .collect::<String>();
    if scale > 0 {
        text.insert(text.len() - scale, '.');
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    Ok(text)
}

/// OpenRouter model-endpoints response envelope.
/// OpenRouter 模型端点响应信封。
#[derive(Debug, Deserialize)]
struct OpenRouterModelEndpointsResponse {
    /// Model endpoint collection.
    /// 模型端点集合。
    data: OpenRouterModelEndpointsData,
}

/// OpenRouter model-endpoints response body.
/// OpenRouter 模型端点响应正文。
#[derive(Debug, Deserialize)]
struct OpenRouterModelEndpointsData {
    /// Available endpoint records.
    /// 可用端点记录。
    #[serde(default)]
    endpoints: Vec<OpenRouterModelEndpointRecord>,
}

/// OpenRouter endpoint metadata used by secondary routing.
/// 二级路由使用的 OpenRouter 端点元数据。
#[derive(Debug, Deserialize)]
struct OpenRouterModelEndpointRecord {
    /// Stable route tag.
    /// 稳定路由标签。
    tag: Option<String>,
    /// Upstream provider display name.
    /// 上游供应商显示名。
    provider_name: Option<String>,
    /// Endpoint display name.
    /// 端点显示名。
    name: Option<String>,
    /// Recent latency metadata.
    /// 近期延迟元数据。
    latency_last_30m: Option<Value>,
    /// Recent uptime metadata.
    /// 近期可用性元数据。
    uptime_last_30m: Option<Value>,
    /// Recent throughput metadata.
    /// 近期吞吐元数据。
    throughput_last_30m: Option<Value>,
    /// Endpoint pricing metadata.
    /// 端点价格元数据。
    pricing: Option<OpenRouterEndpointPricingRecord>,
}

/// OpenRouter endpoint prices retained as exact strings.
/// 以精确字符串保留的 OpenRouter 端点价格。
#[derive(Debug, Deserialize)]
struct OpenRouterEndpointPricingRecord {
    /// Prompt price per token.
    /// 每 Token 输入价格。
    prompt: Option<String>,
    /// Completion price per token.
    /// 每 Token 输出价格。
    completion: Option<String>,
    /// Cache-read price per token.
    /// 每 Token 缓存读取价格。
    input_cache_read: Option<String>,
}

/// MiniMax Token Plan quota response mirrored from the official MiniMax CLI.
/// 从 MiniMax 官方 CLI 镜像的 Token Plan 额度响应。
#[derive(Debug, Deserialize)]
struct MiniMaxQuotaResponse {
    /// Required MiniMax business-status envelope.
    /// 必需的 MiniMax 业务状态封装。
    base_resp: MiniMaxQuotaBaseResponse,
    /// Independently reported model or capability buckets.
    /// 独立报告的模型或能力额度桶。
    model_remains: Vec<MiniMaxQuotaModelRemain>,
}

/// Strict MiniMax Token Plan business status.
/// 严格的 MiniMax Token Plan 业务状态。
#[derive(Debug, Deserialize)]
struct MiniMaxQuotaBaseResponse {
    /// Business status code where zero means success.
    /// 业务状态码，零表示成功。
    status_code: i64,
    /// Controlled provider status message.
    /// 供应商受控状态消息。
    #[serde(default)]
    status_msg: String,
}

/// One strict MiniMax Token Plan quota bucket.
/// 一个严格的 MiniMax Token Plan 额度桶。
#[derive(Debug, Deserialize)]
struct MiniMaxQuotaModelRemain {
    /// Provider-owned bucket name.
    /// 供应商拥有的额度桶名称。
    model_name: String,
    /// Current-window start in provider Unix milliseconds.
    /// 供应商 Unix 毫秒表示的当前窗口开始时间。
    start_time: u64,
    /// Current-window end in provider Unix milliseconds.
    /// 供应商 Unix 毫秒表示的当前窗口结束时间。
    end_time: u64,
    /// Provider-reported current-window total request count.
    /// 供应商报告的当前窗口请求总数。
    current_interval_total_count: u64,
    /// Provider-misnamed current-window remaining request count.
    /// 供应商误命名的当前窗口剩余请求数。
    current_interval_usage_count: u64,
    /// Optional current-window status where three means unlimited.
    /// 可选当前窗口状态，其中三表示无限。
    current_interval_status: Option<u8>,
    /// Current-window remaining percentage when count totals are not exposed.
    /// 未公开次数总量时的当前窗口剩余百分比。
    current_interval_remaining_percent: Option<u64>,
    /// Provider-reported weekly total request count.
    /// 供应商报告的每周请求总数。
    current_weekly_total_count: u64,
    /// Provider-misnamed weekly remaining request count.
    /// 供应商误命名的每周剩余请求数。
    current_weekly_usage_count: u64,
    /// Optional weekly status where three means unlimited.
    /// 可选每周状态，其中三表示无限。
    current_weekly_status: Option<u8>,
    /// Weekly remaining percentage when count totals are not exposed.
    /// 未公开次数总量时的每周剩余百分比。
    current_weekly_remaining_percent: Option<u64>,
    /// Weekly-window start in provider Unix milliseconds.
    /// 供应商 Unix 毫秒表示的每周窗口开始时间。
    weekly_start_time: u64,
    /// Weekly-window end in provider Unix milliseconds.
    /// 供应商 Unix 毫秒表示的每周窗口结束时间。
    weekly_end_time: u64,
}

/// Accepts Kimi's exact number-or-string decimal representation.
/// 接受 Kimi 精确的数字或字符串十进制表示。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum KimiDecimal {
    /// JSON number representation.
    /// JSON 数字表示。
    Number(serde_json::Number),
    /// JSON string representation.
    /// JSON 字符串表示。
    String(String),
}

impl KimiDecimal {
    /// Returns one validated non-negative exact decimal string.
    /// 返回一个经过校验的非负精确十进制字符串。
    fn exact(self) -> Result<String, String> {
        let value = self.lossless();
        normalized_unsigned_decimal_digits(value.as_str())
            .map(|_| value)
            .map_err(|_| String::from("Kimi Coding Plan usage contains an invalid decimal value."))
    }

    /// Preserves the provider's lossless decimal spelling.
    /// 保留供应商无损十进制拼写。
    fn lossless(self) -> String {
        match self {
            Self::Number(value) => value.to_string(),
            Self::String(value) => value,
        }
    }
}

/// Mirrors one exact Kimi usage-counter subset.
/// 镜像一组精确的 Kimi 用量计数字段子集。
#[derive(Debug, Default, Deserialize)]
struct KimiUsageDetail {
    /// Consumed provider quantity.
    /// 已消费供应商数量。
    #[serde(default)]
    used: Option<KimiDecimal>,
    /// Maximum provider quantity.
    /// 供应商数量上限。
    #[serde(default)]
    limit: Option<KimiDecimal>,
    /// Provider-reported available quantity.
    /// 供应商报告的可用数量。
    #[serde(default)]
    remaining: Option<KimiDecimal>,
    /// Optional metric name.
    /// 可选指标名称。
    #[serde(default)]
    name: Option<String>,
    /// Optional display title.
    /// 可选显示标题。
    #[serde(default)]
    title: Option<String>,
    /// Camel-case absolute reset time.
    /// 驼峰形式绝对重置时间。
    #[serde(default, rename = "resetAt")]
    reset_at: Option<String>,
    /// Snake-case absolute reset time.
    /// 下划线形式绝对重置时间。
    #[serde(default, rename = "reset_at")]
    reset_at_snake: Option<String>,
    /// Alternate camel-case reset time.
    /// 备用驼峰形式重置时间。
    #[serde(default, rename = "resetTime")]
    reset_time: Option<String>,
    /// Alternate snake-case reset time.
    /// 备用下划线形式重置时间。
    #[serde(default, rename = "reset_time")]
    reset_time_snake: Option<String>,
}

/// Mirrors one Kimi rolling-window descriptor.
/// 镜像一个 Kimi 滚动窗口描述。
#[derive(Debug, Deserialize)]
struct KimiUsageWindow {
    /// Positive duration magnitude.
    /// 正数时长数值。
    #[serde(default)]
    duration: Option<KimiDecimal>,
    /// Provider duration unit.
    /// 供应商时长单位。
    #[serde(default, rename = "timeUnit", alias = "time_unit")]
    time_unit: Option<String>,
}

/// Mirrors one named Kimi usage limit.
/// 镜像一个具名 Kimi 用量限制。
#[derive(Debug, Deserialize)]
struct KimiUsageLimit {
    /// Inline legacy counter fields.
    /// 内联旧版计数字段。
    #[serde(flatten)]
    inline: KimiUsageDetail,
    /// Optional fallback scope name.
    /// 可选备用 Scope 名称。
    #[serde(default)]
    scope: Option<String>,
    /// Optional nested counter fields.
    /// 可选嵌套计数字段。
    #[serde(default)]
    detail: Option<KimiUsageDetail>,
    /// Optional explicit rolling-window descriptor.
    /// 可选显式滚动窗口描述。
    #[serde(default)]
    window: Option<KimiUsageWindow>,
    /// Optional inline name copied before nested-detail selection.
    /// 在嵌套 Detail 选择前复制的可选内联名称。
    #[serde(default)]
    name: Option<String>,
    /// Optional inline title copied before nested-detail selection.
    /// 在嵌套 Detail 选择前复制的可选内联标题。
    #[serde(default)]
    title: Option<String>,
}

/// Mirrors the exact Kimi account usage response.
/// 镜像精确的 Kimi 账号用量响应。
#[derive(Debug, Deserialize)]
struct KimiUsageResponse {
    /// Optional account-summary counter.
    /// 可选账号汇总计数。
    #[serde(default)]
    usage: Option<KimiUsageDetail>,
    /// Detailed usage windows.
    /// 详细用量窗口。
    #[serde(default)]
    limits: Vec<KimiUsageLimit>,
    /// Account membership observation.
    /// 账号会员观测。
    #[serde(default)]
    user: KimiUsageUser,
}

/// Mirrors the exact Kimi account user subset.
/// 镜像精确的 Kimi 账号用户字段子集。
#[derive(Debug, Default, Deserialize)]
struct KimiUsageUser {
    /// Current membership facts.
    /// 当前会员事实。
    #[serde(default)]
    membership: KimiUsageMembership,
}

/// Mirrors the exact Kimi membership subset.
/// 镜像精确的 Kimi 会员字段子集。
#[derive(Debug, Default, Deserialize)]
struct KimiUsageMembership {
    /// Provider-owned membership level.
    /// 供应商拥有的会员等级。
    #[serde(default)]
    level: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        HttpProviderMetadataClient, MiniMaxQuotaModelRemain, ModelDiscoveryFailureKind,
        ProviderMetadataClient, append_bounded_model_discovery_chunk,
        append_unique_discovered_models, controlled_error_summary, decode_deepseek_balance,
        decode_kimi_coding_plan_usage, minimax_key_profile_id, minimax_quota_windows,
        register_pagination_cursor, resolve_minimax_region, subtract_non_negative_decimal,
        tavily_usage_window, validate_discovered_catalog_bounds, validate_pagination_boundary,
    };
    use crate::{
        ConnectionModel, ConnectionModelCatalog, CredentialConnection, CredentialMode,
        CredentialSecretEnvelope, CustomProviderAuthentication, CustomProviderDefinition,
        CustomProviderModelDiscoveryMode, CustomProviderProtocol, ModelCapabilityEvidence,
        PROVIDER_MODEL_IDENTIFIER_MAX_BYTES,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::oneshot,
        task::JoinHandle,
    };
    use vulcan_ai_contract::ModelCapabilities;
    use vulcan_ai_protocols::{ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID};

    /// Starts one local HTTP fixture that captures the request and returns one JSON response.
    /// 启动一个本地 HTTP 夹具，捕获请求并返回一项 JSON 响应。
    ///
    /// # Parameters
    /// 参数
    /// `status` is the complete HTTP status text and `body` is the exact response body.
    /// `status` 是完整 HTTP 状态文本，`body` 是精确响应正文。
    ///
    /// # Returns
    /// 返回值
    /// The server origin, captured-request receiver, and bounded server task.
    /// 返回服务源地址、请求捕获接收器与有界服务任务。
    async fn start_metadata_fixture(
        status: &str,
        body: &str,
    ) -> (String, oneshot::Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("local metadata fixture should bind");
        let address = listener
            .local_addr()
            .expect("local metadata fixture address");
        let status = status.to_string();
        let body = body.to_string();
        let (request_sender, request_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("metadata fixture should receive one request");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream
                    .read(&mut buffer)
                    .await
                    .expect("metadata fixture should read request headers");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            let _ = request_sender.send(String::from_utf8_lossy(&request).into_owned());
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("metadata fixture should write response");
        });
        (format!("http://{address}"), request_receiver, task)
    }

    /// Starts one local HTTP fixture that returns a bounded sequence of paginated responses.
    /// 启动一个返回有界分页响应序列的本地 HTTP 夹具。
    ///
    /// # Parameters
    /// 参数
    /// `responses` contains exact HTTP status text and response bodies in request order.
    /// `responses` 按请求顺序包含精确 HTTP 状态文本和响应正文。
    ///
    /// # Returns
    /// 返回值
    /// The server origin, captured request list, and bounded server task.
    /// 返回服务源地址、已捕获请求列表与有界服务任务。
    async fn start_metadata_sequence_fixture(
        responses: Vec<(&str, &str)>,
    ) -> (String, oneshot::Receiver<Vec<String>>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("local metadata sequence fixture should bind");
        let address = listener
            .local_addr()
            .expect("local metadata sequence fixture address");
        let responses = responses
            .into_iter()
            .map(|(status, body)| (status.to_string(), body.to_string()))
            .collect::<Vec<_>>();
        let (request_sender, request_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut requests = Vec::with_capacity(responses.len());
            for (status, body) in responses {
                let Ok(accepted) =
                    tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
                        .await
                else {
                    break;
                };
                let (mut stream, _) =
                    accepted.expect("metadata sequence fixture should receive one request");
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let read = stream
                        .read(&mut buffer)
                        .await
                        .expect("metadata sequence fixture should read request headers");
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                }
                requests.push(String::from_utf8_lossy(&request).into_owned());
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len(),
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("metadata sequence fixture should write response");
            }
            let _ = request_sender.send(requests);
        });
        (format!("http://{address}"), request_receiver, task)
    }

    /// Builds one exact custom OpenAI-compatible connection for HTTP discovery tests.
    /// 为 HTTP 发现测试构建一个精确的自定义 OpenAI 兼容连接。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `base_url` define the custom provider identity and endpoint;
    /// `model_discovery_path` optionally selects a reviewed same-origin catalog path.
    /// `provider_id` 与 `base_url` 定义自定义供应商身份和端点；
    /// `model_discovery_path` 可选指定一个已审核的同源目录路径。
    ///
    /// # Returns
    /// 返回值
    /// A validated descriptor and matching credential connection.
    /// 返回已验证描述及匹配的凭据连接。
    fn custom_openai_fixture(
        provider_id: &str,
        base_url: &str,
    ) -> (crate::ProviderDescriptor, CredentialConnection) {
        let provider = CustomProviderDefinition {
            id: provider_id.to_string(),
            display_name: String::from("Local Fixture"),
            base_url: base_url.to_string(),
            protocol: CustomProviderProtocol::OpenAiChat,
            authentication: CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
            model_discovery_path: None,
        }
        .into_descriptor()
        .expect("custom provider fixture should project");
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider_id.to_string(),
            service_entry_id: String::from("default"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Fixture account"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: None,
            verified_protocols: BTreeSet::from([OPENAI_CHAT_PROFILE_ID.to_string()]),
            selected_protocol: OPENAI_CHAT_PROFILE_ID.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        (provider, connection)
    }

    /// Builds one exact custom Anthropic-compatible connection for pagination tests.
    /// 为分页测试构建一个精确的自定义 Anthropic 兼容连接。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `base_url` define the custom provider identity and endpoint.
    /// `provider_id` 与 `base_url` 定义自定义供应商身份和端点。
    ///
    /// # Returns
    /// 返回值
    /// A validated descriptor and matching credential connection.
    /// 返回已验证描述及匹配的凭据连接。
    fn custom_anthropic_fixture(
        provider_id: &str,
        base_url: &str,
        model_discovery_path: Option<&str>,
    ) -> (crate::ProviderDescriptor, CredentialConnection) {
        let provider = CustomProviderDefinition {
            id: provider_id.to_string(),
            display_name: String::from("Local Anthropic Fixture"),
            base_url: base_url.to_string(),
            protocol: CustomProviderProtocol::AnthropicMessages,
            authentication: CustomProviderAuthentication::XApiKey,
            model_discovery_mode: if model_discovery_path.is_some() {
                CustomProviderModelDiscoveryMode::CustomPath
            } else {
                CustomProviderModelDiscoveryMode::Standard
            },
            model_discovery_path: model_discovery_path.map(str::to_string),
        }
        .into_descriptor()
        .expect("custom Anthropic provider fixture should project");
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider_id.to_string(),
            service_entry_id: String::from("default"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Fixture account"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: None,
            verified_protocols: BTreeSet::from([ANTHROPIC_MESSAGES_PROFILE_ID.to_string()]),
            selected_protocol: ANTHROPIC_MESSAGES_PROFILE_ID.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        (provider, connection)
    }

    /// Runs one complete custom Anthropic discovery against a bounded local page sequence.
    /// 针对有界本地分页序列运行一次完整自定义 Anthropic 发现。
    ///
    /// # Parameters
    /// 参数
    /// `responses` contains exact ordered HTTP pages.
    /// `responses` 包含精确有序 HTTP 分页。
    ///
    /// # Returns
    /// 返回值
    /// The typed discovery result and every captured request.
    /// 返回强类型发现结果与全部已捕获请求。
    async fn discover_custom_anthropic_pages(
        responses: Vec<(&str, &str)>,
    ) -> (
        Result<crate::ConnectionModelCatalog, super::ModelDiscoveryError>,
        Vec<String>,
    ) {
        let (origin, request_receiver, server_task) =
            start_metadata_sequence_fixture(responses).await;
        let (provider, connection) = custom_anthropic_fixture(
            "custom:anthropic-fixture",
            format!("{origin}/v1").as_str(),
            None,
        );
        let entry = provider
            .service_entries
            .first()
            .expect("custom Anthropic provider entry");
        let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
            .expect("test credential envelope")
            .execution_secret()
            .expect("test execution secret");
        let result = HttpProviderMetadataClient::new(reqwest::Client::new())
            .discover_models(&provider, entry, &connection, &secret, 1)
            .await;
        let requests = request_receiver
            .await
            .expect("metadata sequence fixture should publish requests");
        server_task
            .await
            .expect("metadata sequence fixture should finish");
        (result, requests)
    }

    /// Builds one minimal model-catalog record for pagination helper tests.
    /// 为分页辅助函数测试构建一个最小模型目录记录。
    fn pagination_model(model_id: &str) -> ConnectionModel {
        ConnectionModel {
            id: model_id.to_string(),
            display_name: model_id.to_string(),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
        }
    }

    /// Verifies duplicate identities reject the complete incoming page without partial mutation.
    /// 验证重复标识会拒绝完整的新分页且不会发生部分变更。
    #[test]
    fn paginated_model_append_is_atomic_and_unique() {
        let mut target = vec![pagination_model("existing")];
        let mut seen = BTreeSet::from([String::from("existing")]);
        let error = append_unique_discovered_models(
            &mut target,
            &mut seen,
            vec![pagination_model("new"), pagination_model("existing")],
            "Test discovery",
        )
        .expect_err("duplicate model must reject the complete page");

        assert!(error.message.contains("duplicate model `existing`"));
        assert_eq!(target.len(), 1);
        assert!(!seen.contains("new"));
    }

    /// Verifies non-adjacent cursor cycles are rejected deterministically.
    /// 验证非相邻游标循环会被确定性拒绝。
    #[test]
    fn pagination_cursor_registry_rejects_non_adjacent_cycles() {
        let mut seen = BTreeSet::new();
        assert_eq!(
            register_pagination_cursor(&mut seen, String::from("A"), "Test discovery")
                .expect("first cursor must register"),
            "A"
        );
        register_pagination_cursor(&mut seen, String::from("B"), "Test discovery")
            .expect("second cursor must register");
        let error = register_pagination_cursor(&mut seen, String::from("A"), "Test discovery")
            .expect_err("a non-adjacent cursor cycle must fail");
        assert!(error.message.contains("repeated pagination cursor `A`"));
    }

    /// Verifies a provider cannot continue beyond the explicit model-catalog page boundary.
    /// 验证供应商不能在明确模型目录分页边界之后继续。
    #[test]
    fn pagination_boundary_rejects_unbounded_unique_cursors() {
        assert!(validate_pagination_boundary(1_023, true, "Test discovery").is_ok());
        assert!(validate_pagination_boundary(1_024, false, "Test discovery").is_ok());
        // error proves a unique-cursor stream still terminates at the fixed safety boundary.
        // error 证明即使游标始终唯一，数据流也会在固定安全边界终止。
        let error = validate_pagination_boundary(1_024, true, "Test discovery")
            .expect_err("continued traversal at the boundary must fail");
        assert!(error.message.contains("1024-page safety boundary"));
    }

    /// Verifies decoded response chunks are rejected before crossing their memory boundary.
    /// 验证解码响应块会在越过内存边界前被拒绝。
    #[test]
    fn bounded_model_discovery_chunk_rejects_overflow_before_append() {
        let mut body = vec![b'a'; 4];

        let error = append_bounded_model_discovery_chunk(&mut body, b"b", 4, "model discovery")
            .expect_err("overflowing chunk must fail");

        assert_eq!(body, b"aaaa");
        assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);
        assert!(error.message.contains("4-byte catalog boundary"));
    }

    /// Verifies non-paginated catalogs cannot bypass aggregate model and identity validation.
    /// 验证非分页目录无法绕过聚合模型与身份校验。
    #[test]
    fn discovered_catalog_bounds_reject_duplicate_and_oversized_identities() {
        let duplicate = ConnectionModelCatalog {
            connection_id: String::from("account"),
            models: vec![
                ConnectionModel {
                    id: String::from("same"),
                    display_name: String::from("One"),
                    capabilities: ModelCapabilities::default(),
                    capability_evidence: ModelCapabilityEvidence::default(),
                },
                ConnectionModel {
                    id: String::from("same"),
                    display_name: String::from("Two"),
                    capabilities: ModelCapabilities::default(),
                    capability_evidence: ModelCapabilityEvidence::default(),
                },
            ],
            generation: 1,
            page_count: 1,
            discovery_schema_revision: String::from("test"),
        };
        let duplicate_error = validate_discovered_catalog_bounds(duplicate, "account")
            .expect_err("duplicate identities must fail");
        assert!(
            duplicate_error
                .message
                .contains("duplicate model identifier")
        );

        let oversized = ConnectionModelCatalog {
            connection_id: String::from("account"),
            models: vec![ConnectionModel {
                id: "x".repeat(PROVIDER_MODEL_IDENTIFIER_MAX_BYTES + 1),
                display_name: String::from("Model"),
                capabilities: ModelCapabilities::default(),
                capability_evidence: ModelCapabilityEvidence::default(),
            }],
            generation: 1,
            page_count: 1,
            discovery_schema_revision: String::from("test"),
        };
        let oversized_error = validate_discovered_catalog_bounds(oversized, "account")
            .expect_err("oversized identities must fail");
        assert!(oversized_error.message.contains("invalid model identifier"));
    }

    /// Verifies public provider diagnostics expose only registered fields and redact the exact key.
    /// 验证公开供应商诊断只暴露已登记字段并遮盖精确 Key。
    #[test]
    fn controlled_error_summary_is_structured_and_secret_safe() {
        // secret is deliberately echoed by the fixture to prove exact redaction.
        // secret 会被夹具有意回显，用于证明精确遮盖有效。
        let secret = "sk-private-fixture";
        // summary must retain actionable fields but omit every unregistered payload field.
        // summary 必须保留可操作字段，同时省略每个未登记载荷字段。
        let summary = controlled_error_summary(
            r#"{"error":{"code":"invalid_key","message":"rejected sk-private-fixture"},"debug":{"raw":"private-upstream-body"}}"#,
            secret,
        );

        assert!(summary.contains("error.code=invalid_key"));
        assert!(summary.contains("error.message=rejected [REDACTED]"));
        assert!(!summary.contains(secret));
        assert!(!summary.contains("private-upstream-body"));
    }

    /// Verifies non-JSON provider bodies never enter the public diagnostic channel.
    /// 验证非 JSON 供应商正文绝不会进入公开诊断通道。
    #[test]
    fn controlled_error_summary_omits_unstructured_body() {
        // summary is fixed text and must not reproduce the raw upstream HTML.
        // summary 是固定文本，不能复现原始上游 HTML。
        let summary = controlled_error_summary("<html>private diagnostic</html>", "secret");

        assert_eq!(
            summary,
            "Provider returned an unstructured error response; body omitted."
        );
        assert!(!summary.contains("private diagnostic"));
    }

    /// Verifies Anthropic-compatible discovery follows every page and forwards the exact cursor.
    /// 验证 Anthropic 兼容发现会读取全部分页并转发精确游标。
    #[tokio::test]
    async fn custom_anthropic_discovery_collects_multiple_pages() {
        let (result, requests) = discover_custom_anthropic_pages(vec![
            (
                "200 OK",
                r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":true,"last_id":"claude-a"}"#,
            ),
            (
                "200 OK",
                r#"{"data":[{"id":"claude-b","display_name":"Claude B"}],"has_more":false,"last_id":"claude-b"}"#,
            ),
        ])
        .await;
        let catalog = result.expect("two valid pages should discover successfully");

        assert_eq!(catalog.page_count, 2);
        assert_eq!(
            catalog
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["claude-a", "claude-b"]
        );
        assert_eq!(requests.len(), 2);
        assert!(requests[0].starts_with("GET /v1/models?limit=1000 HTTP/1.1\r\n"));
        assert!(requests[1].contains("after_id=claude-a"));
    }

    /// Verifies Anthropic-compatible discovery honors one reviewed same-origin catalog path.
    /// 验证 Anthropic 兼容发现会遵守已审核的同源模型目录路径。
    #[tokio::test]
    async fn custom_anthropic_discovery_uses_custom_catalog_path() {
        // fixture records the exact HTTP path requested by the discovery client.
        // fixture 记录发现客户端请求的精确 HTTP 路径。
        let (origin, request_receiver, server_task) = start_metadata_sequence_fixture(vec![(
            "200 OK",
            r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":false,"last_id":"claude-a"}"#,
        )])
        .await;
        // provider selects a catalog path outside the versioned inference base URL.
        // provider 选择位于版本化推理基础地址之外的目录路径。
        let (provider, connection) = custom_anthropic_fixture(
            "custom:anthropic-custom-path",
            format!("{origin}/v1").as_str(),
            Some("/catalog/models"),
        );
        // entry is the exact projected service entry used by the metadata client.
        // entry 是元数据客户端使用的精确投影服务入口。
        let entry = provider
            .service_entries
            .first()
            .expect("custom Anthropic provider entry");
        // secret supplies the exact X-API-Key projection expected by the custom provider.
        // secret 提供自定义供应商所需的精确 X-API-Key 投影。
        let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
            .expect("test credential envelope")
            .execution_secret()
            .expect("test execution secret");

        // catalog is the completed one-page discovery result.
        // catalog 是已完成的单页发现结果。
        let catalog = HttpProviderMetadataClient::new(reqwest::Client::new())
            .discover_models(&provider, entry, &connection, &secret, 1)
            .await
            .expect("custom Anthropic discovery should honor the configured path");
        // requests contains every HTTP request observed by the bounded fixture.
        // requests 包含有界夹具观测到的每个 HTTP 请求。
        let requests = request_receiver
            .await
            .expect("metadata sequence fixture should publish requests");
        server_task
            .await
            .expect("metadata sequence fixture should finish");

        assert_eq!(catalog.models.len(), 1);
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /v1/catalog/models?limit=1000 HTTP/1.1\r\n"));
    }

    /// Verifies a duplicate Anthropic model identity across pages rejects the complete traversal.
    /// 验证 Anthropic 跨页重复模型标识会拒绝完整遍历。
    #[tokio::test]
    async fn custom_anthropic_discovery_rejects_duplicate_models() {
        let (result, requests) = discover_custom_anthropic_pages(vec![
            (
                "200 OK",
                r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":true,"last_id":"cursor-a"}"#,
            ),
            (
                "200 OK",
                r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":false,"last_id":"cursor-b"}"#,
            ),
        ])
        .await;
        let error = result.expect_err("duplicate model identity must reject discovery");

        assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);
        assert!(
            error.message.contains("duplicate model `claude-a`"),
            "{}",
            error.message
        );
        assert_eq!(requests.len(), 2);
    }

    /// Verifies `has_more=true` without a cursor is an invalid response rather than an empty page.
    /// 验证缺少游标的 `has_more=true` 属于无效响应而非空分页。
    #[tokio::test]
    async fn custom_anthropic_discovery_requires_cursor_for_more_pages() {
        let (result, requests) = discover_custom_anthropic_pages(vec![(
            "200 OK",
            r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":true}"#,
        )])
        .await;
        let error = result.expect_err("missing cursor must reject discovery");

        assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);
        assert!(error.message.contains("without `last_id`"));
        assert_eq!(requests.len(), 1);
    }

    /// Verifies a repeated Anthropic cursor terminates before a third request can loop forever.
    /// 验证 Anthropic 重复游标会在第三次请求前终止，避免无限循环。
    #[tokio::test]
    async fn custom_anthropic_discovery_rejects_cursor_cycles() {
        let (result, requests) = discover_custom_anthropic_pages(vec![
            (
                "200 OK",
                r#"{"data":[{"id":"claude-a","display_name":"Claude A"}],"has_more":true,"last_id":"cursor-a"}"#,
            ),
            (
                "200 OK",
                r#"{"data":[{"id":"claude-b","display_name":"Claude B"}],"has_more":true,"last_id":"cursor-a"}"#,
            ),
        ])
        .await;
        let error = result.expect_err("repeated cursor must reject discovery");

        assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);
        assert!(
            error
                .message
                .contains("repeated pagination cursor `cursor-a`")
        );
        assert_eq!(requests.len(), 2);
    }

    /// Verifies custom discovery calls the standard endpoint, sends Bearer auth, and tolerates extensions.
    /// 验证自定义发现会调用标准端点、发送 Bearer 鉴权并容忍扩展字段。
    #[tokio::test]
    async fn custom_openai_discovery_uses_standard_models_endpoint() {
        let (origin, request_receiver, server_task) = start_metadata_fixture(
            "200 OK",
            r#"{"object":"list","first_id":"model-a","data":[{"id":"model-a","object":"model","owned_by":"fixture"}]}"#,
        )
        .await;
        let (provider, connection) =
            custom_openai_fixture("custom:local-fixture", format!("{origin}/v1").as_str());
        let entry = provider
            .service_entries
            .first()
            .expect("custom provider entry");
        let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
            .expect("test credential envelope")
            .execution_secret()
            .expect("test execution secret");
        let client = HttpProviderMetadataClient::new(reqwest::Client::new());

        let catalog = client
            .discover_models(&provider, entry, &connection, &secret, 7)
            .await
            .expect("custom model discovery should succeed");
        let request = request_receiver
            .await
            .expect("metadata fixture should capture the request");
        server_task.await.expect("metadata fixture should finish");

        assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer test-key")
        );
        assert_eq!(catalog.connection_id, "connection");
        assert_eq!(catalog.generation, 7);
        assert_eq!(catalog.page_count, 1);
        assert_eq!(catalog.models.len(), 1);
        assert_eq!(catalog.models[0].id, "model-a");
    }

    /// Verifies a provider HTTP authentication failure remains observable for typed classification.
    /// 验证供应商 HTTP 鉴权失败会保持可观测，以供类型化分类。
    #[tokio::test]
    async fn custom_openai_discovery_preserves_authentication_status() {
        let (origin, _request_receiver, server_task) = start_metadata_fixture(
            "401 Unauthorized",
            r#"{"error":{"message":"invalid credential"}}"#,
        )
        .await;
        let (provider, connection) =
            custom_openai_fixture("custom:auth-fixture", format!("{origin}/v1").as_str());
        let entry = provider
            .service_entries
            .first()
            .expect("custom provider entry");
        let secret = CredentialSecretEnvelope::new_api_key(String::from("invalid-key"))
            .expect("test credential envelope")
            .execution_secret()
            .expect("test execution secret");
        let client = HttpProviderMetadataClient::new(reqwest::Client::new());

        let error = client
            .discover_models(&provider, entry, &connection, &secret, 1)
            .await
            .expect_err("authentication failure must reject discovery");
        server_task.await.expect("metadata fixture should finish");

        assert_eq!(error.kind, ModelDiscoveryFailureKind::AuthenticationFailed);
        assert!(error.message.contains("HTTP 401"));
        assert!(error.message.contains("invalid credential"));
    }

    /// Verifies every documented HTTP discovery class is assigned at the response boundary.
    /// 验证每一种文档化 HTTP 发现分类都会在响应边界直接赋值。
    #[tokio::test]
    async fn custom_discovery_http_statuses_have_exact_typed_failures() {
        for (status, expected) in [
            (
                "403 Forbidden",
                ModelDiscoveryFailureKind::AuthenticationFailed,
            ),
            ("404 Not Found", ModelDiscoveryFailureKind::Unsupported),
            (
                "405 Method Not Allowed",
                ModelDiscoveryFailureKind::Unsupported,
            ),
            (
                "429 Too Many Requests",
                ModelDiscoveryFailureKind::RateLimited,
            ),
            (
                "408 Request Timeout",
                ModelDiscoveryFailureKind::TemporarilyUnavailable,
            ),
            (
                "500 Internal Server Error",
                ModelDiscoveryFailureKind::TemporarilyUnavailable,
            ),
            (
                "503 Service Unavailable",
                ModelDiscoveryFailureKind::TemporarilyUnavailable,
            ),
        ] {
            let (origin, _request_receiver, server_task) =
                start_metadata_fixture(status, r#"{"error":"controlled"}"#).await;
            let (provider, connection) =
                custom_openai_fixture("custom:status-fixture", format!("{origin}/v1").as_str());
            let entry = provider
                .service_entries
                .first()
                .expect("custom provider entry");
            let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
                .expect("test credential envelope")
                .execution_secret()
                .expect("test execution secret");
            let error = HttpProviderMetadataClient::new(reqwest::Client::new())
                .discover_models(&provider, entry, &connection, &secret, 1)
                .await
                .expect_err("non-success status must reject discovery");
            server_task.await.expect("metadata fixture should finish");

            assert_eq!(error.kind, expected, "unexpected class for {status}");
        }
    }

    /// Verifies malformed success payloads fail while an explicit empty catalog remains successful.
    /// 验证格式错误的成功载荷会失败，而显式空目录仍保持成功。
    #[tokio::test]
    async fn custom_discovery_distinguishes_invalid_json_from_empty_catalog() {
        let (invalid_origin, _request_receiver, invalid_server) =
            start_metadata_fixture("200 OK", "{not-json").await;
        let (invalid_provider, invalid_connection) = custom_openai_fixture(
            "custom:invalid-fixture",
            format!("{invalid_origin}/v1").as_str(),
        );
        let invalid_entry = invalid_provider
            .service_entries
            .first()
            .expect("custom provider entry");
        let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
            .expect("test credential envelope")
            .execution_secret()
            .expect("test execution secret");
        let client = HttpProviderMetadataClient::new(reqwest::Client::new());
        let error = client
            .discover_models(
                &invalid_provider,
                invalid_entry,
                &invalid_connection,
                &secret,
                1,
            )
            .await
            .expect_err("malformed JSON must reject discovery");
        invalid_server
            .await
            .expect("invalid metadata fixture should finish");
        assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);

        let (empty_origin, _request_receiver, empty_server) =
            start_metadata_fixture("200 OK", r#"{"object":"list","data":[]}"#).await;
        let (empty_provider, empty_connection) = custom_openai_fixture(
            "custom:empty-fixture",
            format!("{empty_origin}/v1").as_str(),
        );
        let empty_entry = empty_provider
            .service_entries
            .first()
            .expect("custom provider entry");
        let catalog = client
            .discover_models(&empty_provider, empty_entry, &empty_connection, &secret, 2)
            .await
            .expect("empty catalog is a successful observed state");
        empty_server
            .await
            .expect("empty metadata fixture should finish");
        assert!(catalog.models.is_empty());
        assert_eq!(catalog.page_count, 1);
    }

    /// Verifies a transport failure is retryable and never masquerades as unsupported discovery.
    /// 验证传输失败可重试，且绝不会伪装成目录不受支持。
    #[tokio::test]
    async fn custom_discovery_transport_failure_is_temporarily_unavailable() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("temporary endpoint should bind");
        let address = listener.local_addr().expect("temporary endpoint address");
        drop(listener);
        let (provider, connection) = custom_openai_fixture(
            "custom:transport-fixture",
            format!("http://{address}/v1").as_str(),
        );
        let entry = provider
            .service_entries
            .first()
            .expect("custom provider entry");
        let secret = CredentialSecretEnvelope::new_api_key(String::from("test-key"))
            .expect("test credential envelope")
            .execution_secret()
            .expect("test execution secret");
        let error = HttpProviderMetadataClient::new(reqwest::Client::new())
            .discover_models(&provider, entry, &connection, &secret, 1)
            .await
            .expect_err("closed endpoint must reject discovery");

        assert_eq!(
            error.kind,
            ModelDiscoveryFailureKind::TemporarilyUnavailable
        );
    }

    /// Verifies MiniMax billing profiles are selected only from reviewed literal prefixes.
    /// 验证 MiniMax 计费配置档仅从已审核的字面前缀中选择。
    #[test]
    fn minimax_key_prefixes_select_exact_profiles() {
        assert_eq!(
            minimax_key_profile_id("sk-api-example").expect("API key should classify"),
            "payg-api-key"
        );
        assert_eq!(
            minimax_key_profile_id("sk-cp-example").expect("Plan key should classify"),
            "token-plan-key"
        );
        assert!(minimax_key_profile_id("sk-example").is_err());
        assert!(minimax_key_profile_id("SK-CP-example").is_err());
    }

    /// Verifies DeepSeek balances preserve every currency and exact decimal string.
    /// 验证 DeepSeek 余额会保留每种币种及精确十进制字符串。
    #[test]
    fn deepseek_balance_preserves_multiple_currencies() {
        let snapshot = decode_deepseek_balance(
            r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"},{"currency":"USD","total_balance":"2.50","granted_balance":"0","topped_up_balance":"2.50"}]}"#,
        )
        .expect("valid DeepSeek balance");
        assert_eq!(snapshot.quota_windows.len(), 2);
        assert_eq!(
            snapshot.quota_windows[0].remaining.as_deref(),
            Some("110.00")
        );
        assert_eq!(snapshot.quota_windows[1].unit, "USD");
        assert!(snapshot.balance_amount.is_none());
        assert!(snapshot.currency.is_none());
    }

    /// Verifies Kimi preserves membership, nested counters, exact decimals, and reset evidence.
    /// 验证 Kimi 保留会员、嵌套计数、精确十进制及重置证据。
    #[test]
    fn kimi_usage_preserves_membership_and_nested_windows() {
        let snapshot = decode_kimi_coding_plan_usage(
            r#"{"usage":{"used":"1.25","limit":"10"},"limits":[{"name":"5 hour usage","detail":{"used":2,"limit":8,"remaining":6,"resetAt":"2026-07-29T10:00:00Z"},"window":{"duration":5,"timeUnit":"HOURS"}}],"user":{"membership":{"level":"moderato"}}}"#,
        )
        .expect("valid Kimi usage");
        assert_eq!(snapshot.plan_name.as_deref(), Some("moderato"));
        assert_eq!(snapshot.quota_windows.len(), 2);
        assert_eq!(snapshot.quota_windows[0].remaining.as_deref(), Some("8.75"));
        assert_eq!(snapshot.quota_windows[1].used.as_deref(), Some("2"));
        assert_eq!(snapshot.quota_windows[1].remaining.as_deref(), Some("6"));
        assert_eq!(
            snapshot.quota_windows[1].resets_at.as_deref(),
            Some("2026-07-29T10:00:00Z")
        );
        assert_eq!(
            snapshot.quota_windows[1].bucket_name.as_deref(),
            Some("5 HOURS")
        );
    }

    /// Verifies regional registration rejects ambiguous and fully rejected probe outcomes.
    /// 验证区域注册会拒绝歧义结果与全部拒绝结果。
    #[test]
    fn minimax_region_requires_exactly_one_successful_probe() {
        assert_eq!(
            resolve_minimax_region(Ok(()), Err(String::from("rejected"))).expect("global region"),
            "global"
        );
        assert_eq!(
            resolve_minimax_region(Err(String::from("rejected")), Ok(())).expect("China region"),
            "cn"
        );
        assert!(resolve_minimax_region(Ok(()), Ok(())).is_err());
        assert!(
            resolve_minimax_region(
                Err(String::from("global rejected")),
                Err(String::from("China rejected"))
            )
            .is_err()
        );
    }

    /// Verifies MiniMax's zero-allocation status-three bucket is unavailable, not unlimited.
    /// 验证 MiniMax 零分配且状态为三的额度桶表示不可用而不是无限。
    #[test]
    fn minimax_zero_allocation_bucket_is_unavailable() {
        let windows = minimax_quota_windows(vec![MiniMaxQuotaModelRemain {
            model_name: String::from("video"),
            start_time: 1,
            end_time: 2,
            current_interval_total_count: 0,
            current_interval_usage_count: 0,
            current_interval_status: Some(3),
            current_interval_remaining_percent: Some(100),
            current_weekly_total_count: 0,
            current_weekly_usage_count: 0,
            current_weekly_status: Some(3),
            current_weekly_remaining_percent: Some(100),
            weekly_start_time: 1,
            weekly_end_time: 2,
        }])
        .expect("quota windows should normalize");
        assert_eq!(windows.len(), 2);
        assert!(windows.iter().all(|window| !window.available));
        assert!(windows.iter().all(|window| !window.unlimited));
    }

    /// Verifies a non-empty weekly status-three bucket remains explicitly unlimited.
    /// 验证非空且周状态为三的额度桶仍明确表示无限。
    #[test]
    fn minimax_weekly_status_three_remains_unlimited() {
        let windows = minimax_quota_windows(vec![MiniMaxQuotaModelRemain {
            model_name: String::from("general"),
            start_time: 1,
            end_time: 2,
            current_interval_total_count: 100,
            current_interval_usage_count: 90,
            current_interval_status: Some(1),
            current_interval_remaining_percent: Some(90),
            current_weekly_total_count: 100,
            current_weekly_usage_count: 90,
            current_weekly_status: Some(3),
            current_weekly_remaining_percent: Some(90),
            weekly_start_time: 1,
            weekly_end_time: 2,
        }])
        .expect("quota windows should normalize");
        assert!(windows.iter().all(|window| window.available));
        assert!(!windows[0].unlimited);
        assert!(windows[1].unlimited);
    }

    /// Verifies OpenRouter credit balances retain decimal precision and exponent semantics.
    /// 验证 OpenRouter 余额保留十进制精度及指数语义。
    #[test]
    fn openrouter_credit_subtraction_is_exact() {
        assert_eq!(
            subtract_non_negative_decimal("100.10", "0.09").expect("decimal subtraction"),
            "100.01"
        );
        assert_eq!(
            subtract_non_negative_decimal("1e2", "2.5").expect("exponent subtraction"),
            "97.5"
        );
        assert!(subtract_non_negative_decimal("1", "2").is_err());
    }

    /// Verifies a missing Tavily limit remains unknown rather than becoming zero.
    /// 验证缺失的 Tavily 上限保持未知而不会被转换为零。
    #[test]
    fn tavily_missing_limit_is_not_zero() {
        let window = tavily_usage_window("search", "Search", 12, None, false)
            .expect("unbounded provider counter");
        assert_eq!(window.used.as_deref(), Some("12"));
        assert!(window.total.is_none());
        assert!(window.remaining.is_none());
        assert!(!window.unlimited);
    }

    /// Verifies only the documented Tavily PAYGO null limit is represented as unlimited.
    /// 验证仅文档明确的 Tavily PAYGO 空上限会表示为无限。
    #[test]
    fn tavily_paygo_null_limit_is_unlimited() {
        let window =
            tavily_usage_window("paygo", "PAYGO", 3, None, true).expect("unlimited paygo counter");
        assert!(window.unlimited);
        assert!(window.total.is_none());
        assert!(window.remaining.is_none());
    }
}
