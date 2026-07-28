//! Provider HTTP clients for discovery and account usage.
//! 用于模型发现和账号用量的供应商 HTTP 客户端。

use crate::{
    AccountUsageQueryKind, AuthenticationProjection, ConnectionModel, ConnectionModelCatalog,
    CredentialConnection, CredentialExecutionSecret, ModelDiscoveryKind, ProviderDescriptor,
    SecondaryRouteOption, ServiceEntry, build_anthropic_model_catalog, build_gemini_model_catalog,
    build_minimax_model_catalog, parse_anthropic_model_page, parse_gemini_model_page,
    parse_google_code_assist_model_catalog, parse_minimax_model_page,
    parse_minimax_openai_model_catalog, parse_openai_codex_model_catalog,
    parse_openai_model_catalog, parse_openrouter_model_catalog,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Maximum wall time for one provider metadata or account-usage HTTP request.
/// 单次供应商元数据或账号用量 HTTP 请求的最大墙钟时间。
const PROVIDER_METADATA_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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
        _provider: &ProviderDescriptor,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        secret: &CredentialExecutionSecret,
        generation: u64,
    ) -> Result<ConnectionModelCatalog, String>;

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
        _provider: &ProviderDescriptor,
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
        .send()
        .await
        .map_err(|error| format!("catalog request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("catalog response failed: {error}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    let catalog = parse_openai_model_catalog("minimax-region-probe", 1, body.as_str())
        .map_err(|error| format!("invalid catalog: {error}"))?;
    if catalog.models.is_empty() {
        return Err(String::from("catalog contained no models"));
    }
    Ok(())
}

#[async_trait]
impl ProviderMetadataClient for HttpProviderMetadataClient {
    async fn discover_models(
        &self,
        _provider: &ProviderDescriptor,
        entry: &ServiceEntry,
        connection: &CredentialConnection,
        secret: &CredentialExecutionSecret,
        generation: u64,
    ) -> Result<ConnectionModelCatalog, String> {
        let protocol_route = entry
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == connection.selected_protocol)
            .ok_or_else(|| {
                format!(
                    "Protocol `{}` is not registered for service entry `{}`.",
                    connection.selected_protocol, entry.id
                )
            })?;
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                )
            })?;
        let authentication = protocol_route
            .resolved_authentication(entry.authentication, credential_profile.authentication);
        match protocol_route.resolved_model_discovery(entry.model_discovery) {
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
                let url = format!("{}/v1/models", entry.base_url.trim_end_matches('/'));
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
            }
            ModelDiscoveryKind::OpenRouter => {
                let url = format!("{}/models", entry.base_url.trim_end_matches('/'));
                let body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    url.as_str(),
                    secret,
                    "model discovery",
                )
                .await?;
                parse_openrouter_model_catalog(connection.id.as_str(), generation, body.as_str())
            }
            ModelDiscoveryKind::OpenAi => {
                let url = format!("{}/models", entry.base_url.trim_end_matches('/'));
                let body = send_metadata_get_with_authentication(
                    &self.http,
                    entry,
                    authentication,
                    url.as_str(),
                    secret,
                    "model discovery",
                )
                .await?;
                parse_openai_model_catalog(connection.id.as_str(), generation, body.as_str())
            }
            ModelDiscoveryKind::OpenAiCodex => {
                let url = format!(
                    "{}/models?client_version=0.144.1",
                    entry.base_url.trim_end_matches('/')
                );
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
            }
            ModelDiscoveryKind::Anthropic => {
                discover_anthropic_models(&self.http, entry, connection, secret, generation).await
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
        }
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
            AccountUsageQueryKind::OpenRouterCurrentKey => {
                query_openrouter_current_key(&self.http, entry, secret.expose_value()).await
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
) -> Result<ConnectionModelCatalog, String> {
    let project_id = secret.project_id().ok_or_else(|| {
        String::from("Google Code Assist model discovery requires a validated project id.")
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
    let response = request
        .send()
        .await
        .map_err(|error| format!("Google Code Assist model discovery failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Google Code Assist model discovery response failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "Google Code Assist model discovery returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_body(body.as_str())
        ));
    }
    parse_google_code_assist_model_catalog(connection.id.as_str(), generation, body.as_str())
}

/// Sends one authenticated metadata GET and returns a controlled response body.
/// 发送一次已鉴权元数据 GET 并返回受控响应正文。
async fn send_metadata_get(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    url: &str,
    secret: &CredentialExecutionSecret,
    operation: &str,
) -> Result<String, String> {
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
) -> Result<String, String> {
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
            return Err(format!(
                "Service entry must not override reserved metadata header `{name}`."
            ));
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
                String::from("OpenAI Codex metadata request requires a verified account identity.")
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
                String::from(
                    "Public Gemini OAuth metadata request requires a Google Cloud project id.",
                )
            })?;
            request = request.header("x-goog-user-project", quota_project_id);
        }
        AuthenticationProjection::XApiKey
        | AuthenticationProjection::BearerAuthorization
        | AuthenticationProjection::GoogleApiKey => {}
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("{operation} request to `{url}` failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("{operation} response from `{url}` failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "{operation} request to `{url}` returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_body(body.as_str())
        ));
    }
    Ok(body)
}

/// Fetches every documented Anthropic model page.
/// 获取每一页文档化 Anthropic 模型。
async fn discover_anthropic_models(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    connection: &CredentialConnection,
    secret: &CredentialExecutionSecret,
    generation: u64,
) -> Result<ConnectionModelCatalog, String> {
    let mut models = Vec::<ConnectionModel>::new();
    let mut after_id = None::<String>;
    loop {
        let mut url = reqwest::Url::parse(
            format!("{}/v1/models", entry.base_url.trim_end_matches('/')).as_str(),
        )
        .map_err(|_| String::from("Anthropic model discovery URL is invalid."))?;
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
        let page = parse_anthropic_model_page(body.as_str())?;
        models.extend(page.models);
        if !page.has_more {
            break;
        }
        let next = page.last_id.ok_or_else(|| {
            String::from("Anthropic model discovery reported more pages without `last_id`.")
        })?;
        if after_id.as_deref() == Some(next.as_str()) {
            return Err(String::from(
                "Anthropic model discovery repeated the same pagination cursor.",
            ));
        }
        after_id = Some(next);
    }
    Ok(build_anthropic_model_catalog(
        connection.id.as_str(),
        generation,
        models,
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
) -> Result<ConnectionModelCatalog, String> {
    let mut models = Vec::<ConnectionModel>::new();
    let mut page_token = None::<String>;
    loop {
        let mut url = reqwest::Url::parse(
            format!("{}/models", entry.base_url.trim_end_matches('/')).as_str(),
        )
        .map_err(|_| String::from("Gemini model discovery URL is invalid."))?;
        url.query_pairs_mut().append_pair("pageSize", "1000");
        if let Some(token) = page_token.as_deref() {
            url.query_pairs_mut().append_pair("pageToken", token);
        }
        let body =
            send_metadata_get(http, entry, url.as_str(), secret, "Gemini model discovery").await?;
        let page = parse_gemini_model_page(body.as_str())?;
        models.extend(page.models);
        let Some(next) = page.next_page_token.filter(|value| !value.is_empty()) else {
            break;
        };
        if page_token.as_deref() == Some(next.as_str()) {
            return Err(String::from(
                "Gemini model discovery repeated the same pagination token.",
            ));
        }
        page_token = Some(next);
    }
    Ok(build_gemini_model_catalog(
        connection.id.as_str(),
        generation,
        models,
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
    let body = response.text().await.map_err(|error| {
        format!("OpenRouter secondary route response from `{url}` failed: {error}")
    })?;
    if !status.is_success() {
        return Err(format!(
            "OpenRouter secondary route request to `{url}` returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_body(body.as_str())
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
    let body = response
        .text()
        .await
        .map_err(|error| format!("MiniMax Token Plan usage response failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "MiniMax Token Plan usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_body(body.as_str())
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

/// Queries the documented OpenRouter current-key endpoint.
/// 查询文档化的 OpenRouter 当前密钥端点。
async fn query_openrouter_current_key(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let url = format!("{}/key", entry.base_url.trim_end_matches('/'));
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("OpenRouter account usage request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("OpenRouter account usage response failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "OpenRouter account usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_body(body.as_str())
        ));
    }
    let response: OpenRouterKeyResponse = serde_json::from_str(body.as_str())
        .map_err(|error| format!("OpenRouter account usage JSON is invalid: {error}"))?;
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: response
            .data
            .is_free_tier
            .map(|is_free| if is_free { "Free" } else { "Paid" }.to_string()),
        used_amount: response.data.usage.map(|value| value.to_string()),
        remaining_amount: response.data.limit_remaining.map(|value| value.to_string()),
        currency: Some(String::from("USD")),
        balance_amount: None,
        quota_windows: Vec::new(),
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
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
) -> Result<ConnectionModelCatalog, String> {
    let url = format!(
        "{}/anthropic/v1/models",
        entry.base_url.trim_end_matches('/')
    );
    let mut after_id: Option<String> = None;
    let mut models = Vec::new();
    let mut model_ids = std::collections::BTreeSet::new();
    loop {
        let mut page_url = reqwest::Url::parse(url.as_str())
            .map_err(|error| format!("MiniMax model discovery URL is invalid: {error}"))?;
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
        let page = parse_minimax_model_page(body.as_str())?;
        for model in page.models {
            if !model_ids.insert(model.id.clone()) {
                return Err(format!(
                    "MiniMax model discovery returned duplicate model `{}` across pages.",
                    model.id
                ));
            }
            models.push(model);
        }
        if !page.has_more {
            break;
        }
        let next_cursor = page
            .last_id
            .filter(|cursor| !cursor.trim().is_empty())
            .ok_or_else(|| {
                String::from(
                    "MiniMax model discovery reported `has_more=true` without a `last_id` cursor.",
                )
            })?;
        if after_id.as_deref() == Some(next_cursor.as_str()) {
            return Err(format!(
                "MiniMax model discovery repeated pagination cursor `{next_cursor}`."
            ));
        }
        after_id = Some(next_cursor);
    }
    Ok(build_minimax_model_catalog(
        connection.id.as_str(),
        generation,
        models,
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
        used: (!input.unlimited).then(|| input.total.saturating_sub(input.remaining).to_string()),
        remaining: (!input.unlimited).then(|| input.remaining.to_string()),
        total: (!input.unlimited).then(|| input.total.to_string()),
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

/// Limits an upstream error body before presenting it to callers.
/// 在向调用方展示前限制上游错误正文长度。
fn controlled_error_body(body: &str) -> String {
    body.chars().take(2_000).collect()
}

/// OpenRouter current-key response.
/// OpenRouter 当前密钥响应。
#[derive(Debug, Deserialize)]
struct OpenRouterKeyResponse {
    /// Current key metadata.
    /// 当前密钥元数据。
    data: OpenRouterKeyData,
}

/// OpenRouter current-key metadata used by the public usage contract.
/// 公共用量契约使用的 OpenRouter 当前密钥元数据。
#[derive(Debug, Deserialize)]
struct OpenRouterKeyData {
    /// Total usage amount.
    /// 总使用金额。
    usage: Option<serde_json::Number>,
    /// Remaining configured limit.
    /// 剩余配置限额。
    limit_remaining: Option<serde_json::Number>,
    /// Whether this key is on the free tier.
    /// 此密钥是否属于免费层。
    is_free_tier: Option<bool>,
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

#[cfg(test)]
mod tests {
    use super::{
        MiniMaxQuotaModelRemain, minimax_key_profile_id, minimax_quota_windows,
        resolve_minimax_region,
    };

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
}
