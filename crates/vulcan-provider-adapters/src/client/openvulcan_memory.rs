//! Authenticated transport for the OpenVulcan memory-only service.
//! OpenVulcan 专用记忆服务的认证传输。

use std::{
    collections::BTreeSet,
    sync::LazyLock,
    time::{Duration, SystemTime},
};

use reqwest::{Method, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use vulcan_ai_contract::RetryAfterHint;

use crate::CredentialExecutionSecret;

/// Wire version of the deployed memory-only contract.
/// 已部署专用记忆协议的线协议版本。
const VERSION: u32 = 1;
/// Caps memory responses, including ordered embedding batches.
/// 限制记忆响应大小，包括有序向量批次。
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Prevents automatic retries and credential forwarding through redirects.
/// 防止自动重试以及通过重定向转发凭据。
static HTTP: LazyLock<Result<reqwest::Client, reqwest::Error>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
});

/// Closed set of purposes accepted by the memory endpoint.
/// 专用记忆端点接受的封闭用途集合。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPurpose {
    /// Precheck intent and search planning.
    /// 预检意图及检索规划。
    Pcl1,
    /// Precheck review of retrieved memories.
    /// 对检索到的记忆执行预检复核。
    Pcl2,
    /// Postaction extraction of memory candidates.
    /// 事后提取记忆候选。
    Ptl1,
    /// Postaction review of memory and profile candidates.
    /// 事后复核记忆与画像候选。
    Ptl2,
    /// Explicit profile instruction.
    /// 显式画像指令。
    ProfileInstruction,
    /// Dense query or document embeddings.
    /// 查询或文档的稠密向量。
    Embedding,
    /// Reranking of retrieved candidates.
    /// 对检索候选执行重排。
    Rerank,
}

impl MemoryPurpose {
    /// Lists generation purposes covered by a shared inference disclosure.
    /// 列出共用推理披露涵盖的生成用途。
    pub const GENERATION: [Self; 5] = [
        Self::Pcl1,
        Self::Pcl2,
        Self::Ptl1,
        Self::Ptl2,
        Self::ProfileInstruction,
    ];

    /// Returns the stable wire and usage identifier of this purpose.
    /// 返回此用途的稳定线协议与用量标识。
    pub const fn id(self) -> &'static str {
        match self {
            Self::Pcl1 => "pcl1",
            Self::Pcl2 => "pcl2",
            Self::Ptl1 => "ptl1",
            Self::Ptl2 => "ptl2",
            Self::ProfileInstruction => "profile_instruction",
            Self::Embedding => "embedding",
            Self::Rerank => "rerank",
        }
    }
}

/// Provider disclosure categories delivered by the deployment.
/// 部署端提供的供应商披露类别。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCapability {
    /// Shared or per-phase structured inference.
    /// 共用或分阶段的结构化推理。
    Inference,
    /// Vector creation for retrieval.
    /// 为检索创建向量。
    Embedding,
    /// Optional relevance reranking.
    /// 可选的相关性重排。
    Rerank,
}

/// Privacy information for the exact upstream model used by a set of purposes.
/// 一组用途所用的确切上游模型及隐私信息。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MemoryDisclosure {
    /// Operation category and, for split inference routes, its covered purposes.
    /// 操作类别以及拆分推理路由覆盖的用途。
    pub capability: MemoryCapability,
    /// Exact upstream model and provider label.
    /// 确切的上游模型及供应商标签。
    pub model: String,
    /// Provider name presented to the user.
    /// 向用户展示的供应商名称。
    pub provider: String,
    /// Processing region and administrator-supplied policy disclosure.
    /// 处理地区及管理员提供的隐私说明。
    pub region: String,
    /// Provider's data handling statement for this route.
    /// 供应商对此路由的数据处理说明。
    pub data_policy: String,
    /// HTTPS link to the provider's privacy policy.
    /// 指向供应商隐私政策的 HTTPS 链接。
    pub privacy_url: String,
    /// Explicit covered phases for split inference, empty for a shared route.
    /// 拆分推理明确涵盖的阶段，共用路由则为空。
    #[serde(default)]
    pub purposes: Vec<MemoryPurpose>,
}

impl MemoryDisclosure {
    /// Reports whether this exact disclosure covers the supplied purpose.
    /// 判断这份准确披露是否涵盖指定用途。
    fn covers(&self, purpose: MemoryPurpose) -> bool {
        match self.capability {
            MemoryCapability::Embedding => purpose == MemoryPurpose::Embedding,
            MemoryCapability::Rerank => purpose == MemoryPurpose::Rerank,
            MemoryCapability::Inference => {
                MemoryPurpose::GENERATION.contains(&purpose)
                    && (self.purposes.is_empty() || self.purposes.contains(&purpose))
            }
        }
    }
}

/// Selectable offer bound to a server-generated consent revision.
/// 绑定服务端生成同意版本的可选模型。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MemoryOffer {
    /// Published identity, display name, and exact consent revision.
    /// 发布标识、显示名称和准确同意版本。
    pub id: String,
    /// Administrator-configured display name.
    /// 管理员配置的显示名称。
    pub name: String,
    /// Server-issued revision covering the offer's routes and disclosures.
    /// 涵盖模型路由及披露内容的服务端版本。
    pub consent_revision: String,
    /// Dense vector width and the complete upstream disclosures.
    /// 稠密向量宽度及全部上游披露。
    pub embedding_dimension: u32,
    /// Complete consent scope, including the optional reranker.
    /// 完整的同意范围，包括可选重排模型。
    pub providers: Vec<MemoryDisclosure>,
}

impl MemoryOffer {
    /// Returns the single provider covered by the consent for a purpose, if present.
    /// 如存在，返回同意范围内负责此用途的唯一供应商。
    pub fn provider_for(&self, purpose: MemoryPurpose) -> Option<&MemoryDisclosure> {
        self.providers.iter().find(|route| route.covers(purpose))
    }

    /// Checks complete and unambiguous routes before any consent or request is accepted.
    /// 在接受任何确认或请求前检查路由是否完整且无歧义。
    pub fn is_valid(&self) -> bool {
        (1..=64).contains(&self.id.len())
            && self.id.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || (index > 0 && matches!(byte, b'-' | b'_'))
            })
            && !self.name.trim().is_empty()
            && self.consent_revision.len() == 64
            && self
                .consent_revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && (1..=4096).contains(&self.embedding_dimension)
            && self.providers.iter().all(|route| {
                !route.model.is_empty()
                    && !route.provider.trim().is_empty()
                    && !route.region.trim().is_empty()
                    && !route.data_policy.trim().is_empty()
                    && reqwest::Url::parse(&route.privacy_url).is_ok_and(|url| {
                        url.scheme() == "https"
                            && url.host_str().is_some()
                            && url.username().is_empty()
                            && url.password().is_none()
                    })
                    && route.purposes.iter().all(|purpose| {
                        route.capability == MemoryCapability::Inference
                            && MemoryPurpose::GENERATION.contains(purpose)
                    })
            })
            && MemoryPurpose::GENERATION
                .into_iter()
                .chain([MemoryPurpose::Embedding])
                .all(|purpose| {
                    self.providers
                        .iter()
                        .filter(|route| route.covers(purpose))
                        .count()
                        == 1
                })
            && self
                .providers
                .iter()
                .filter(|route| route.covers(MemoryPurpose::Rerank))
                .count()
                <= 1
    }
}

/// Catalog carries only metadata and a suggested default.
/// 模型目录仅携带元数据和建议默认项。
#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryCatalog {
    /// Version checked before rendering the offers.
    /// 展示模型前校验的版本。
    contract_version: u32,
    /// Suggested default, empty when the site publishes no models.
    /// 建议默认项，站点未发布模型时为空。
    pub default_id: String,
    /// Available models in the site's order.
    /// 按站点顺序排列的可用模型。
    pub models: Vec<MemoryOffer>,
}

/// Reports only token counts actually returned by the server.
/// 只记录服务端实际返回的 Token 数。
#[derive(Debug, Deserialize)]
pub struct MemoryUsage {
    /// Total input and output, as reported by the upstream.
    /// 上游报告的输入和输出总量。
    pub input_tokens: Option<u64>,
    /// Provider-reported output count when available.
    /// 供应商报告的输出 Token 数，如有提供。
    pub output_tokens: Option<u64>,
    /// Provider-reported aggregate, independent of the component fields.
    /// 上游报告的合计，独立于分项字段。
    pub total_tokens: Option<u64>,
    /// Read and created cache tokens are included in input when reported.
    /// 缓存读取和创建 Token 在报告时已包含于输入。
    pub cached_input_tokens: Option<u64>,
    /// Tokens used to create an explicit cache when reported.
    /// 报告时用于创建显式缓存的 Token 数。
    pub cache_creation_input_tokens: Option<u64>,
}

/// Result and optional actual usage returned by the dedicated channel.
/// 专用通道返回的结果和可选实际用量。
#[derive(Debug, Deserialize)]
pub struct MemoryResponse {
    /// Purpose-specific checked JSON result.
    /// 按用途校验的 JSON 结果。
    pub result: Value,
    /// Usage remains absent when the upstream did not report it.
    /// 上游未报告时用量保持缺失。
    pub usage: Option<MemoryUsage>,
}

/// Stable transport outcomes that carry no upstream body or input text.
/// 不携带上游响应正文或输入文本的稳定传输结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryTransportError {
    /// Sign-in is required for this native endpoint.
    /// 此原生端点要求登录。
    SignIn,
    /// The accepted offer or its privacy revision changed.
    /// 已确认模型或隐私版本发生变更。
    SelectionChanged,
    /// The site's phase quota was reached.
    /// 达到站点阶段限额。
    RateLimited {
        /// The wait advertised by the memory service before another request may be admitted.
        /// 记忆服务声明的下次请求可准入前的等待时间。
        retry_after: Option<RetryAfterHint>,
    },
    /// The input exceeded the site's limit.
    /// 输入超过站点限额。
    InputLimit,
    /// The site failed to return a usable response.
    /// 站点未返回可用响应。
    Unavailable,
}

/// Exact versioned request envelope containing business data only.
/// 只包含业务数据的精确版本化请求封套。
#[derive(Serialize)]
struct InferRequest<'a> {
    /// Wire version and user-confirmed selection.
    /// 线协议版本及用户确认的选择。
    contract_version: u32,
    model_id: &'a str,
    consent_revision: &'a str,
    /// Closed route and its already-filtered input.
    /// 封闭路由及已完成过滤的输入。
    purpose: MemoryPurpose,
    input: Value,
}

/// Fetches the catalog from the signed-in account's exact regional origin.
/// 从已登录账号的准确区域来源获取目录。
pub async fn request_openvulcan_memory_catalog(
    secret: &CredentialExecutionSecret,
) -> Result<MemoryCatalog, MemoryTransportError> {
    let catalog: MemoryCatalog = send(secret, "capabilities", Duration::from_secs(8), None).await?;
    let ids = catalog
        .models
        .iter()
        .map(|model| model.id.as_str())
        .collect::<BTreeSet<_>>();
    if catalog.contract_version != VERSION
        || ids.len() != catalog.models.len()
        || !catalog.models.iter().all(MemoryOffer::is_valid)
        || (!catalog.default_id.is_empty() && !ids.contains(catalog.default_id.as_str()))
    {
        return Err(MemoryTransportError::Unavailable);
    }
    Ok(catalog)
}

/// Sends one filtered operation on the explicitly consented offer.
/// 按用户明确同意的模型发送一次已过滤的操作。
pub async fn request_openvulcan_memory_inference(
    secret: &CredentialExecutionSecret,
    offer: &MemoryOffer,
    purpose: MemoryPurpose,
    input: Value,
) -> Result<MemoryResponse, MemoryTransportError> {
    send(
        secret,
        "infer",
        Duration::from_secs(35),
        Some(InferRequest {
            contract_version: VERSION,
            model_id: &offer.id,
            consent_revision: &offer.consent_revision,
            purpose,
            input,
        }),
    )
    .await
}

/// Performs one regional request, enforcing the response bound and decoding its contract.
/// 执行一次区域请求，限制响应大小并按协议解码。
async fn send<T: DeserializeOwned>(
    secret: &CredentialExecutionSecret,
    route: &str,
    timeout: Duration,
    input: Option<InferRequest<'_>>,
) -> Result<T, MemoryTransportError> {
    use MemoryTransportError as Error;

    let region = secret.openvulcan_region().ok_or(Error::SignIn)?;
    let url = format!(
        "{}/api/v1/vulcancode/memory-inference/{route}",
        region.auth_origin()
    );
    let authorization =
        crate::openvulcan::authorization_header(secret, &url).map_err(|_| Error::SignIn)?;
    let http = HTTP.as_ref().map_err(|_| Error::Unavailable)?;
    let method = if input.is_some() {
        Method::POST
    } else {
        Method::GET
    };
    let mut request = http
        .request(method, url)
        .timeout(timeout)
        .header(header::AUTHORIZATION, authorization)
        .header(header::ACCEPT, "application/json");
    if let Some(input) = input {
        request = request.json(&input);
    }
    let mut response = request.send().await.map_err(|_| Error::Unavailable)?;
    match response.status().as_u16() {
        200 => {}
        401 => return Err(Error::SignIn),
        409 => return Err(Error::SelectionChanged),
        413 => return Err(Error::InputLimit),
        429 => {
            return Err(Error::RateLimited {
                retry_after: crate::execution::parse_retry_after_headers(
                    response.headers(),
                    SystemTime::now(),
                ),
            });
        }
        _ => return Err(Error::Unavailable),
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Unavailable)? {
        if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
            return Err(Error::Unavailable);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Error::Unavailable)
}
