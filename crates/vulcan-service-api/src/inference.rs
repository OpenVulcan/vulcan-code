//! Versioned loopback inference service transport contracts.
//! 版本化回环推理服务传输契约。

use crate::ResponsesCreateRequest;
use reqwest::{Method, StatusCode, Url, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fmt,
    net::IpAddr,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use vulcan_ai_contract::{
    EmbeddingRequest, EmbeddingResponse, InferenceCapabilitySnapshot, RerankRequest, RerankResponse,
};

/// Identifies the first controlled loopback inference API version.
/// 标识首个受控回环推理 API 版本。
pub const INFERENCE_SERVICE_API_VERSION: u32 = 2;

/// Resolves the default protected inference discovery file path.
/// 解析默认受保护推理发现文件路径。
///
/// # Parameters
/// 参数
/// `home_dir` is the optional user home and `temp_dir` is the process fallback.
/// `home_dir` 是可选用户目录，`temp_dir` 是进程回退目录。
///
/// # Returns
/// 返回值
/// A deterministic user-scoped path preferred over the temporary fallback.
/// 返回优先于临时回退的确定性用户作用域路径。
pub fn inference_service_discovery_snapshot_path(
    home_dir: Option<PathBuf>,
    temp_dir: PathBuf,
) -> PathBuf {
    home_dir
        .map(|home| {
            home.join(".vulcan")
                .join("code")
                .join("inference-service.json")
        })
        .unwrap_or_else(|| temp_dir.join("vulcan-code-inference-service.json"))
}

/// Carries the current process-local inference endpoint and ephemeral access grant.
/// 承载当前进程内推理端点与临时访问授权。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceServiceDiscoverySnapshot {
    /// Exact discovery document version.
    /// 精确发现文档版本。
    pub api_version: u32,
    /// Current readiness state.
    /// 当前就绪状态。
    pub status: String,
    /// Loopback HTTP origin shared with the WebUI service.
    /// 与 WebUI 服务共享的回环 HTTP 源。
    pub base_url: String,
    /// Host process identifier.
    /// 宿主进程标识。
    pub process_id: u32,
    /// Stable caller identifier bound to this grant.
    /// 当前授权绑定的稳定调用方标识。
    pub caller_id: String,
    /// Consumer profile bound to this grant.
    /// 当前授权绑定的消费者配置档。
    pub consumer_profile_id: String,
    /// Opaque bearer token returned only through the protected discovery file.
    /// 仅通过受保护发现文件返回的不透明 Bearer Token。
    pub access_token: String,
    /// Absolute Unix expiration in milliseconds.
    /// Unix 毫秒绝对过期时间。
    pub expires_at_unix_ms: u64,
    /// Process start timestamp text used for stale-file diagnostics.
    /// 用于陈旧文件诊断的进程启动时间文本。
    pub started_at_unix_ms: String,
}

impl fmt::Debug for InferenceServiceDiscoverySnapshot {
    /// Formats discovery metadata while redacting the bearer token.
    /// 格式化发现元数据，同时脱敏 Bearer Token。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives only non-secret discovery fields and one redaction marker.
    /// `formatter` 仅接收非秘密发现字段与一个脱敏标记。
    ///
    /// # Returns
    /// 返回值
    /// Standard debug formatting completion.
    /// 标准调试格式化完成结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InferenceServiceDiscoverySnapshot")
            .field("api_version", &self.api_version)
            .field("status", &self.status)
            .field("base_url", &self.base_url)
            .field("process_id", &self.process_id)
            .field("caller_id", &self.caller_id)
            .field("consumer_profile_id", &self.consumer_profile_id)
            .field("access_token", &"<redacted>")
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .field("started_at_unix_ms", &self.started_at_unix_ms)
            .finish()
    }
}

/// Exposes non-secret inference service health information.
/// 暴露不含秘密的推理服务健康信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceServiceHealthSnapshot {
    /// Exact API version.
    /// 精确 API 版本。
    pub api_version: u32,
    /// Current readiness state.
    /// 当前就绪状态。
    pub status: String,
    /// Host process identifier.
    /// 宿主进程标识。
    pub process_id: u32,
    /// Host process start timestamp used with the process identifier.
    /// 与进程标识共同使用的宿主进程启动时间戳。
    pub started_at_unix_ms: String,
}

/// Selects one closed inference HTTP operation for transport-independent dispatch.
/// 为传输无关分派选择一个封闭推理 HTTP 操作。
#[derive(Debug, Clone)]
pub enum InferenceServiceRequest {
    /// Lists consumer-authorized routable conversation models.
    /// 列出消费者已授权且可路由的对话模型。
    ListModels,
    /// Starts one standard Responses Create execution for SSE delivery.
    /// 启动一个用于 SSE 投递的标准 Responses Create 执行。
    StartResponse {
        /// Validated standard request body.
        /// 已校验的标准请求体。
        request: ResponsesCreateRequest,
    },
    /// Executes and aggregates one standard Responses Create request.
    /// 执行并聚合一个标准 Responses Create 请求。
    CreateResponse {
        /// Validated standard request body.
        /// 已校验的标准请求体。
        request: ResponsesCreateRequest,
    },
    /// Reads one authorized consumer capability snapshot.
    /// 读取一个已授权消费者能力快照。
    Capabilities {
        /// Exact consumer profile identifier.
        /// 精确消费者配置档标识。
        consumer_profile_id: String,
    },
    /// Starts one restricted LLM execution.
    /// 启动一项受限 LLM 执行。
    /// Creates one complete embedding batch.
    /// 创建一批完整向量。
    CreateEmbeddings {
        /// Closed embedding request.
        /// 封闭向量请求。
        request: EmbeddingRequest,
    },
    /// Reranks one complete candidate set.
    /// 重排一组完整候选。
    Rerank {
        /// Closed rerank request.
        /// 封闭重排请求。
        request: RerankRequest,
    },
}

/// Carries one controlled inference HTTP failure.
/// 承载一项受控推理 HTTP 失败。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceServiceHttpError {
    /// Concrete HTTP status code.
    /// 明确 HTTP 状态码。
    pub status: u16,
    /// Stable machine-readable error code.
    /// 稳定机器可读错误码。
    pub code: String,
    /// Controlled human-readable diagnostic.
    /// 受控人类可读诊断。
    pub message: String,
}

/// Wraps one successful inference HTTP response.
/// 包装一项成功的推理 HTTP 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InferenceServiceSuccessEnvelope {
    /// Exact API version.
    /// 精确 API 版本。
    pub api_version: u32,
    /// Serialized operation result.
    /// 序列化操作结果。
    pub result: serde_json::Value,
}

/// Wraps one failed inference HTTP response.
/// 包装一项失败的推理 HTTP 响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceServiceErrorEnvelope {
    /// Exact API version.
    /// 精确 API 版本。
    pub api_version: u32,
    /// Stable machine-readable error code.
    /// 稳定机器可读错误码。
    pub code: String,
    /// Controlled human-readable diagnostic.
    /// 受控人类可读诊断。
    pub message: String,
}

/// Reports one controlled loopback inference client failure.
/// 表示一项受控回环推理客户端失败。
#[derive(Debug)]
pub enum InferenceServiceClientError {
    /// Discovery data or a requested path violated the closed loopback contract.
    /// 发现数据或请求路径违反封闭回环契约。
    InvalidContract(String),
    /// The loopback HTTP transport failed before a valid response was decoded.
    /// 回环 HTTP 传输在解码有效响应前失败。
    Transport(reqwest::Error),
    /// The host returned one versioned controlled failure.
    /// 宿主返回了一项版本化受控失败。
    Service(InferenceServiceHttpError),
}

impl fmt::Display for InferenceServiceClientError {
    /// Formats a credential-safe client diagnostic.
    /// 格式化不包含凭据的客户端诊断。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives the controlled diagnostic.
    /// `formatter` 接收受控诊断。
    ///
    /// # Returns
    /// 返回值
    /// Standard formatting completion.
    /// 标准格式化完成结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidContract(message) => formatter.write_str(message),
            Self::Transport(error) => {
                write!(formatter, "Inference loopback transport failed: {error}")
            }
            Self::Service(error) => write!(
                formatter,
                "Inference service rejected the request with {} ({}): {}",
                error.status, error.code, error.message
            ),
        }
    }
}

impl std::error::Error for InferenceServiceClientError {}

impl From<reqwest::Error> for InferenceServiceClientError {
    /// Converts a request failure without exposing the bearer token.
    /// 转换请求失败且不暴露 Bearer Token。
    ///
    /// # Parameters
    /// 参数
    /// `error` is the original request failure.
    /// `error` 是原始请求失败。
    ///
    /// # Returns
    /// 返回值
    /// A transport-scoped inference client error.
    /// 返回传输作用域的推理客户端错误。
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error)
    }
}

/// Executes the versioned inference contract through one protected loopback discovery snapshot.
/// 通过一份受保护回环发现快照执行版本化推理契约。
#[derive(Clone)]
pub struct InferenceServiceClient {
    /// Shared bounded HTTP client.
    /// 共享有界 HTTP 客户端。
    http: reqwest::Client,
    /// Validated loopback base URL.
    /// 已验证的回环基础 URL。
    base_url: Url,
    /// Ephemeral caller-bound bearer token.
    /// 临时且绑定调用方的 Bearer Token。
    access_token: String,
    /// Consumer profile bound to the discovery grant.
    /// 发现授权绑定的消费者配置档。
    consumer_profile_id: String,
    /// Stable caller identity bound to the discovery grant.
    /// 发现授权绑定的稳定调用方身份。
    caller_id: String,
    /// Host process identity captured by the discovery snapshot.
    /// 发现快照捕获的宿主进程身份。
    process_id: u32,
    /// Host process start time used together with the process identifier.
    /// 与进程标识共同使用的宿主进程启动时间。
    started_at_unix_ms: u64,
    /// Absolute expiration of the immutable bearer grant.
    /// 不可变 Bearer 授权的绝对过期时间。
    expires_at_unix_ms: u64,
}

impl InferenceServiceClient {
    /// Builds a client from one current protected discovery snapshot.
    /// 根据一份当前受保护发现快照构建客户端。
    ///
    /// # Parameters
    /// 参数
    /// `http` owns connection pooling and `discovery` binds endpoint, profile, and token.
    /// `http` 负责连接池，`discovery` 绑定端点、配置档和令牌。
    ///
    /// # Returns
    /// 返回值
    /// A client only when the discovery document is ready, current-version, and loopback-only.
    /// 仅当发现文档就绪、版本匹配且仅使用回环地址时返回客户端。
    pub fn from_discovery(
        http: reqwest::Client,
        discovery: InferenceServiceDiscoverySnapshot,
    ) -> Result<Self, InferenceServiceClientError> {
        if discovery.api_version != INFERENCE_SERVICE_API_VERSION {
            return Err(InferenceServiceClientError::InvalidContract(format!(
                "Inference discovery API version {} is unsupported.",
                discovery.api_version
            )));
        }
        if discovery.status != "ready" {
            return Err(InferenceServiceClientError::InvalidContract(format!(
                "Inference discovery status `{}` is not ready.",
                discovery.status
            )));
        }
        if discovery.access_token.trim().is_empty()
            || discovery.consumer_profile_id.trim().is_empty()
            || discovery.caller_id.trim().is_empty()
        {
            return Err(InferenceServiceClientError::InvalidContract(String::from(
                "Inference discovery omitted its access token, caller, or consumer profile.",
            )));
        }
        if discovery.process_id == 0 {
            return Err(InferenceServiceClientError::InvalidContract(String::from(
                "Inference discovery process identifier must be non-zero.",
            )));
        }
        let started_at_unix_ms = discovery
            .started_at_unix_ms
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                InferenceServiceClientError::InvalidContract(String::from(
                    "Inference discovery process start timestamp must be a positive Unix millisecond value.",
                ))
            })?;
        let now_unix_ms = current_unix_millis()?;
        if discovery.expires_at_unix_ms <= now_unix_ms {
            return Err(InferenceServiceClientError::InvalidContract(String::from(
                "Inference discovery access grant has expired.",
            )));
        }
        let mut base_url = Url::parse(discovery.base_url.as_str()).map_err(|error| {
            InferenceServiceClientError::InvalidContract(format!(
                "Inference discovery base URL is invalid: {error}"
            ))
        })?;
        let host = base_url.host_str().ok_or_else(|| {
            InferenceServiceClientError::InvalidContract(String::from(
                "Inference discovery base URL omitted its host.",
            ))
        })?;
        let loopback = host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback());
        if base_url.scheme() != "http"
            || !loopback
            || base_url.username() != ""
            || base_url.password().is_some()
            || base_url.path() != "/"
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err(InferenceServiceClientError::InvalidContract(String::from(
                "Inference discovery base URL must be an unauthenticated root loopback HTTP origin.",
            )));
        }
        base_url.set_path("/");
        base_url.set_query(None);
        base_url.set_fragment(None);
        Ok(Self {
            http,
            base_url,
            access_token: discovery.access_token,
            consumer_profile_id: discovery.consumer_profile_id,
            caller_id: discovery.caller_id,
            process_id: discovery.process_id,
            started_at_unix_ms,
            expires_at_unix_ms: discovery.expires_at_unix_ms,
        })
    }

    /// Returns the stable caller identity bound to this immutable client grant.
    /// 返回当前不可变客户端授权绑定的稳定调用方身份。
    ///
    /// # Returns
    /// 返回值
    /// The exact discovery-bound caller identifier.
    /// 返回精确的发现绑定调用方标识。
    pub fn caller_id(&self) -> &str {
        self.caller_id.as_str()
    }

    /// Returns the consumer profile bound to this immutable client grant.
    /// 返回当前不可变客户端授权绑定的消费者配置档。
    ///
    /// # Returns
    /// 返回值
    /// The exact discovery-bound profile identifier.
    /// 返回精确的发现绑定配置档标识。
    pub fn consumer_profile_id(&self) -> &str {
        self.consumer_profile_id.as_str()
    }

    /// Returns the host instance identity captured by this immutable client.
    /// 返回当前不可变客户端捕获的宿主实例身份。
    ///
    /// # Returns
    /// 返回值
    /// The process identifier and process start timestamp that must be compared together.
    /// 返回必须共同比较的进程标识与进程启动时间戳。
    pub fn instance_identity(&self) -> (u32, u64) {
        (self.process_id, self.started_at_unix_ms)
    }

    /// Returns the absolute bearer-grant expiration.
    /// 返回 Bearer 授权的绝对过期时间。
    ///
    /// # Returns
    /// 返回值
    /// The Unix millisecond deadline after which a fresh discovery snapshot is required.
    /// 返回必须重新读取发现快照的 Unix 毫秒截止时间。
    pub fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }

    /// Reads non-secret health and verifies the currently reachable host instance.
    /// 读取非秘密健康信息并验证当前可达的宿主实例。
    ///
    /// # Returns
    /// 返回值
    /// Current health only when its API revision and process identity match this client snapshot.
    /// 仅当 API 修订和进程身份与当前客户端快照匹配时返回健康信息。
    pub async fn health(
        &self,
    ) -> Result<InferenceServiceHealthSnapshot, InferenceServiceClientError> {
        let url = self.endpoint("inference/v1/health")?;
        let response = self
            .http
            .get(url)
            .header(header::ACCEPT, "application/json")
            .send()
            .await?
            .error_for_status()?;
        let health: InferenceServiceHealthSnapshot = response.json().await?;
        validate_response_version(health.api_version)?;
        let health_started_at_unix_ms = health
            .started_at_unix_ms
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                InferenceServiceClientError::InvalidContract(String::from(
                    "Inference health process start timestamp must be a positive Unix millisecond value.",
                ))
            })?;
        if health.process_id != self.process_id
            || health_started_at_unix_ms != self.started_at_unix_ms
        {
            return Err(InferenceServiceClientError::InvalidContract(format!(
                "Inference health instance {}@{} does not match discovery instance {}@{}.",
                health.process_id,
                health_started_at_unix_ms,
                self.process_id,
                self.started_at_unix_ms
            )));
        }
        Ok(health)
    }

    /// Reads the capability snapshot for the discovery-bound consumer profile.
    /// 读取发现绑定消费者配置档的能力快照。
    ///
    /// # Returns
    /// 返回值
    /// The current configured operation routes and strong capabilities.
    /// 返回当前已配置操作路由与强类型能力。
    pub async fn capabilities(
        &self,
    ) -> Result<InferenceCapabilitySnapshot, InferenceServiceClientError> {
        let path = format!(
            "inference/v1/capabilities/{}",
            encode_path_segment(self.consumer_profile_id.as_str())
        );
        self.request(Method::GET, path.as_str(), Option::<&()>::None)
            .await
    }

    /// Creates one complete embedding batch through the configured profile.
    /// 通过已配置档创建一批完整向量。
    ///
    /// # Parameters
    /// 参数
    /// `request` contains bounded text inputs and no provider route override.
    /// `request` 包含有界文本输入且不含供应商路由覆盖。
    ///
    /// # Returns
    /// 返回值
    /// Ordered vectors, usage, immutable route, and the embedding-space fingerprint.
    /// 返回有序向量、用量、不可变路由与向量空间指纹。
    pub async fn create_embeddings(
        &self,
        request: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, InferenceServiceClientError> {
        self.validate_request_profile(request.consumer_profile_id.as_str())?;
        self.request(Method::POST, "inference/v1/embeddings", Some(request))
            .await
    }

    /// Reranks one complete candidate set through the configured profile.
    /// 通过已配置档重排一组完整候选。
    ///
    /// # Parameters
    /// 参数
    /// `request` contains one query and an ordered bounded candidate set.
    /// `request` 包含一条查询和一组有序有界候选。
    ///
    /// # Returns
    /// 返回值
    /// Normalized ranks, original indices, scores, route, usage, and profile fingerprint.
    /// 返回归一化排名、原始索引、分数、路由、用量与配置档指纹。
    pub async fn rerank(
        &self,
        request: &RerankRequest,
    ) -> Result<RerankResponse, InferenceServiceClientError> {
        self.validate_request_profile(request.consumer_profile_id.as_str())?;
        self.request(Method::POST, "inference/v1/rerank", Some(request))
            .await
    }

    /// Validates that a closed request cannot escape the discovery-bound profile.
    /// 验证封闭请求不能逃逸发现绑定配置档。
    ///
    /// # Parameters
    /// 参数
    /// `consumer_profile_id` is the profile requested by the operation.
    /// `consumer_profile_id` 是操作请求的配置档。
    ///
    /// # Returns
    /// 返回值
    /// Success only for an exact profile match.
    /// 仅在配置档精确匹配时返回成功。
    fn validate_request_profile(
        &self,
        consumer_profile_id: &str,
    ) -> Result<(), InferenceServiceClientError> {
        if consumer_profile_id != self.consumer_profile_id {
            return Err(InferenceServiceClientError::InvalidContract(format!(
                "Inference request profile `{consumer_profile_id}` does not match the discovery-bound profile `{}`.",
                self.consumer_profile_id
            )));
        }
        Ok(())
    }

    /// Sends one versioned authenticated request and decodes its exact typed result.
    /// 发送一项版本化鉴权请求并解码其精确强类型结果。
    ///
    /// # Parameters
    /// 参数
    /// `method`, `path`, and optional `body` form one closed client operation.
    /// `method`、`path` 与可选 `body` 构成一项封闭客户端操作。
    ///
    /// # Returns
    /// 返回值
    /// The typed success result or one controlled transport/service failure.
    /// 返回强类型成功结果或一项受控传输/服务失败。
    async fn request<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, InferenceServiceClientError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = self.endpoint(path)?;
        let mut request = self
            .http
            .request(method, url)
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", self.access_token),
            )
            .header(header::ACCEPT, "application/json");
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(decode_service_error(status, bytes.as_ref()));
        }
        let envelope: InferenceServiceSuccessEnvelope = serde_json::from_slice(bytes.as_ref())
            .map_err(|error| {
                InferenceServiceClientError::InvalidContract(format!(
                    "Inference success envelope is invalid JSON: {error}"
                ))
            })?;
        validate_response_version(envelope.api_version)?;
        serde_json::from_value(envelope.result).map_err(|error| {
            InferenceServiceClientError::InvalidContract(format!(
                "Inference result does not match the requested operation: {error}"
            ))
        })
    }

    /// Resolves one relative closed API path below the validated loopback origin.
    /// 解析已验证回环源下的一条相对封闭 API 路径。
    ///
    /// # Parameters
    /// 参数
    /// `path` is a code-owned relative path without a query or fragment.
    /// `path` 是代码持有且不含查询或片段的相对路径。
    ///
    /// # Returns
    /// 返回值
    /// The absolute loopback endpoint.
    /// 返回绝对回环端点。
    fn endpoint(&self, path: &str) -> Result<Url, InferenceServiceClientError> {
        self.base_url.join(path).map_err(|error| {
            InferenceServiceClientError::InvalidContract(format!(
                "Inference endpoint path is invalid: {error}"
            ))
        })
    }
}

/// Reads the current Unix timestamp for discovery-expiration validation.
/// 读取当前 Unix 时间戳以校验发现授权是否过期。
///
/// # Returns
/// 返回值
/// Current Unix milliseconds or a controlled clock diagnostic.
/// 返回当前 Unix 毫秒数或一项受控时钟诊断。
fn current_unix_millis() -> Result<u64, InferenceServiceClientError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            InferenceServiceClientError::InvalidContract(format!(
                "System clock is before the Unix epoch: {error}"
            ))
        })?;
    u64::try_from(elapsed.as_millis()).map_err(|_| {
        InferenceServiceClientError::InvalidContract(String::from(
            "Current Unix timestamp exceeds the supported millisecond range.",
        ))
    })
}

/// Encodes one opaque identifier as a single URL path segment.
/// 将一个不透明标识编码为单个 URL 路径段。
///
/// # Parameters
/// 参数
/// `value` is the exact identifier text.
/// `value` 是精确标识文本。
///
/// # Returns
/// 返回值
/// Percent-encoded UTF-8 that cannot add another path segment.
/// 返回无法新增路径段的 UTF-8 百分号编码文本。
fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(*byte));
        } else {
            encoded.push_str(format!("%{byte:02X}").as_str());
        }
    }
    encoded
}

/// Validates one host response against the client contract revision.
/// 验证一项宿主响应是否符合客户端契约修订。
///
/// # Parameters
/// 参数
/// `api_version` is the response-envelope version.
/// `api_version` 是响应信封版本。
///
/// # Returns
/// 返回值
/// Success only for the exact supported revision.
/// 仅在精确支持的修订上返回成功。
fn validate_response_version(api_version: u32) -> Result<(), InferenceServiceClientError> {
    if api_version != INFERENCE_SERVICE_API_VERSION {
        return Err(InferenceServiceClientError::InvalidContract(format!(
            "Inference response API version {api_version} is unsupported."
        )));
    }
    Ok(())
}

/// Decodes one non-success response without leaking request credentials.
/// 解码一项非成功响应且不泄露请求凭据。
///
/// # Parameters
/// 参数
/// `status` is the HTTP status and `bytes` is the bounded response body.
/// `status` 是 HTTP 状态，`bytes` 是有界响应正文。
///
/// # Returns
/// 返回值
/// A controlled service failure or a malformed-envelope contract error.
/// 返回受控服务失败或格式错误信封契约错误。
fn decode_service_error(status: StatusCode, bytes: &[u8]) -> InferenceServiceClientError {
    let envelope: InferenceServiceErrorEnvelope = match serde_json::from_slice(bytes) {
        Ok(envelope) => envelope,
        Err(error) => {
            return InferenceServiceClientError::InvalidContract(format!(
                "Inference error envelope is invalid JSON for HTTP {}: {error}",
                status.as_u16()
            ));
        }
    };
    if let Err(error) = validate_response_version(envelope.api_version) {
        return error;
    }
    InferenceServiceClientError::Service(InferenceServiceHttpError {
        status: status.as_u16(),
        code: envelope.code,
        message: envelope.message,
    })
}

#[cfg(test)]
mod inference_client_tests {
    use super::{
        INFERENCE_SERVICE_API_VERSION, InferenceServiceClient, InferenceServiceClientError,
        InferenceServiceDiscoverySnapshot, encode_path_segment,
    };

    /// Builds one valid discovery fixture for client boundary tests.
    /// 构建一份用于客户端边界测试的有效发现夹具。
    ///
    /// # Returns
    /// 返回值
    /// A ready loopback snapshot with an opaque test-only token.
    /// 返回包含不透明测试令牌的就绪回环快照。
    fn discovery_fixture() -> InferenceServiceDiscoverySnapshot {
        InferenceServiceDiscoverySnapshot {
            api_version: INFERENCE_SERVICE_API_VERSION,
            status: String::from("ready"),
            base_url: String::from("http://127.0.0.1:17891"),
            process_id: 42,
            caller_id: String::from("vmm"),
            consumer_profile_id: String::from("vmm"),
            access_token: String::from("test-token"),
            expires_at_unix_ms: u64::MAX,
            started_at_unix_ms: String::from("1"),
        }
    }

    /// Verifies discovery debug formatting never exposes the bearer token.
    /// 验证发现快照的调试格式绝不暴露 Bearer Token。
    #[test]
    fn discovery_debug_redacts_access_token() {
        let discovery = discovery_fixture();
        let debug = format!("{discovery:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(discovery.access_token.as_str()));
    }

    /// Verifies only an explicit loopback origin can produce a client.
    /// 验证只有显式回环源才能生成客户端。
    #[test]
    fn discovery_rejects_non_loopback_origin() {
        let mut discovery = discovery_fixture();
        discovery.base_url = String::from("https://example.com");
        assert!(matches!(
            InferenceServiceClient::from_discovery(reqwest::Client::new(), discovery),
            Err(InferenceServiceClientError::InvalidContract(_))
        ));
    }

    /// Verifies discovery cannot hide a non-root route below an otherwise valid loopback origin.
    /// 验证发现地址不能在有效回环源下隐藏非根路径。
    #[test]
    fn discovery_rejects_non_root_origin() {
        let mut discovery = discovery_fixture();
        discovery.base_url = String::from("http://127.0.0.1:17891/untrusted");
        assert!(matches!(
            InferenceServiceClient::from_discovery(reqwest::Client::new(), discovery),
            Err(InferenceServiceClientError::InvalidContract(_))
        ));
    }

    /// Verifies an expired access grant cannot create a client.
    /// 验证已经过期的访问授权不能创建客户端。
    #[test]
    fn discovery_rejects_expired_grant() {
        let mut discovery = discovery_fixture();
        discovery.expires_at_unix_ms = 1;
        assert!(matches!(
            InferenceServiceClient::from_discovery(reqwest::Client::new(), discovery),
            Err(InferenceServiceClientError::InvalidContract(_))
        ));
    }

    /// Verifies caller and process identity are mandatory discovery facts.
    /// 验证调用方与进程身份是发现文档的必填事实。
    #[test]
    fn discovery_rejects_missing_instance_identity() {
        let mut discovery = discovery_fixture();
        discovery.process_id = 0;
        assert!(matches!(
            InferenceServiceClient::from_discovery(reqwest::Client::new(), discovery),
            Err(InferenceServiceClientError::InvalidContract(_))
        ));

        let mut discovery = discovery_fixture();
        discovery.caller_id.clear();
        assert!(matches!(
            InferenceServiceClient::from_discovery(reqwest::Client::new(), discovery),
            Err(InferenceServiceClientError::InvalidContract(_))
        ));

        let mut discovery = discovery_fixture();
        discovery.started_at_unix_ms = String::from("invalid");
        assert!(matches!(
            InferenceServiceClient::from_discovery(reqwest::Client::new(), discovery),
            Err(InferenceServiceClientError::InvalidContract(_))
        ));
    }

    /// Verifies immutable discovery identity remains available to rotation managers.
    /// 验证不可变发现身份可供授权轮换管理器读取。
    #[test]
    fn client_exposes_discovery_identity() {
        let client =
            InferenceServiceClient::from_discovery(reqwest::Client::new(), discovery_fixture())
                .expect("valid loopback discovery");

        assert_eq!(client.caller_id(), "vmm");
        assert_eq!(client.consumer_profile_id(), "vmm");
        assert_eq!(client.instance_identity(), (42, 1));
        assert_eq!(client.expires_at_unix_ms(), u64::MAX);
    }

    /// Verifies a request cannot substitute another consumer profile.
    /// 验证请求不能替换成其他消费者配置档。
    #[test]
    fn client_rejects_profile_escape() {
        let client =
            InferenceServiceClient::from_discovery(reqwest::Client::new(), discovery_fixture())
                .expect("valid loopback discovery");
        assert!(matches!(
            client.validate_request_profile("other"),
            Err(InferenceServiceClientError::InvalidContract(_))
        ));
    }

    /// Verifies opaque identifiers cannot inject another path segment.
    /// 验证不透明标识不能注入其他路径段。
    #[test]
    fn path_segment_encoding_blocks_injection() {
        assert_eq!(encode_path_segment("execution/a b"), "execution%2Fa%20b");
    }
}
