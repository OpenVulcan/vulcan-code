//! Deterministic provider execution routing and SSE transport.
//! 确定性的供应商执行路由与 SSE 传输。

use crate::media::{MediaUploadCancellation, MediaUploadProgressCallback};
use crate::{
    AuthenticationProjection, CredentialConnection, CredentialExecutionSecret,
    CustomModelRequestProjection, MINIMAX_REASONING_SPLIT_PARAMETER_ID,
    OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID, ProtocolRoute, ProviderDescriptor,
    ProviderToolTarget, RequestProjectorKind, SAMPLING_TEMPERATURE_PARAMETER_ID,
    SecondaryRouteSelection, ServiceEntry,
};
use async_stream::{stream, try_stream};
use futures_util::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionError, AiExecutionRequest, AiStreamEvent, AiStreamStateMachine,
    DispatchOutcome, InputTokenCount, ModelParameterValue, PromptCachePolicy, ReasoningEffort,
    ResourceKind, RetryAfterHint, RetryAfterSource, StructuredOutputPolicy,
};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    CLAUDE_CODE_MESSAGES_PROFILE_ID, EncodedWireRequest, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID, ProtocolConversionError,
    ProtocolRegistry, ProtocolStreamFraming, ProtocolTransportProfile,
    decode_gemini_count_tokens_response, encode_gemini_count_tokens_request,
};

/// MiniMax documented maximum JSON request-body size.
/// MiniMax 文档化的 JSON 请求体最大字节数。
const MINIMAX_REQUEST_BODY_BYTE_LIMIT: usize = 64_000_000;

/// Immutable route selected before an execution starts.
/// 在执行开始前选择的不可变路由。
#[derive(Debug, Clone)]
pub struct ProviderExecutionRoute {
    /// Provider descriptor snapshot.
    /// 供应商描述快照。
    pub provider: ProviderDescriptor,
    /// Service entry snapshot.
    /// 服务入口快照。
    pub entry: ServiceEntry,
    /// Exact credential connection snapshot without secret material.
    /// 不含秘钥材料的精确凭据连接快照。
    pub connection: CredentialConnection,
    /// Exact verified protocol route.
    /// 精确的已验证协议路由。
    pub protocol_route: ProtocolRoute,
    /// Optional provider-declared secondary route.
    /// 可选供应商声明二级路由。
    pub secondary_route: SecondaryRouteSelection,
    /// Optional user-owned safe projection for one exact custom model.
    /// 可选的单个精确自定义模型用户自有安全投影。
    pub custom_model_projection: Option<CustomModelRequestProjection>,
}

impl ProviderExecutionRoute {
    /// Resolves an immutable execution route from registered snapshots.
    /// 从已注册快照解析不可变执行路由。
    ///
    /// # Parameters
    /// 参数
    /// `provider`, `entry`, and `connection` are exact configuration snapshots;
    /// `secondary_route` is the user-selected provider-internal route.
    /// `provider`、`entry` 与 `connection` 是精确配置快照；`secondary_route` 是用户选择的供应商内部路由。
    ///
    /// # Returns
    /// 返回值
    /// A validated route or a controlled pre-dispatch failure.
    /// 返回已校验路由或受控的分派前失败。
    pub fn resolve(
        provider: ProviderDescriptor,
        entry: ServiceEntry,
        connection: CredentialConnection,
        secondary_route: SecondaryRouteSelection,
    ) -> Result<Self, ProviderExecutionFailure> {
        if provider.id != connection.provider_id {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` does not belong to provider `{}`.",
                connection.id, provider.id
            )));
        }
        if entry.id != connection.service_entry_id {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` does not use service entry `{}`.",
                connection.id, entry.id
            )));
        }
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                ))
            })?;
        if credential_profile.credential_mode != connection.credential_mode {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` credential mode does not match profile `{}`.",
                connection.id, credential_profile.id
            )));
        }
        let protocol_route = entry
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == connection.selected_protocol)
            .cloned()
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Protocol `{}` is not registered for service entry `{}`.",
                    connection.selected_protocol, entry.id
                ))
            })?;
        if !protocol_route.verified
            || !connection
                .verified_protocols
                .contains(&connection.selected_protocol)
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol `{}` is not verified for connection `{}`.",
                connection.selected_protocol, connection.id
            )));
        }
        if !matches!(secondary_route, SecondaryRouteSelection::Automatic)
            && provider.id != "openrouter"
        {
            return Err(pre_dispatch_failure(format!(
                "Provider `{}` does not register secondary routing.",
                provider.id
            )));
        }
        Ok(Self {
            provider,
            entry,
            connection,
            protocol_route,
            secondary_route,
            custom_model_projection: None,
        })
    }

    /// Returns the exact request URL for this immutable route.
    /// 返回该不可变路由的精确请求 URL。
    ///
    /// # Returns
    /// 返回值
    /// The service base URL joined with the registered endpoint path.
    /// 返回服务基础 URL 与已注册端点路径的拼接结果。
    pub fn request_url(&self, encoded_path: &str) -> String {
        format!(
            "{}{}",
            crate::connection_endpoint::resolved_connection_protocol_base_url(
                &self.connection,
                &self.entry,
                &self.protocol_route,
            )
            .trim_end_matches('/'),
            ensure_leading_slash(encoded_path)
        )
    }
}

/// Carries one structured provider execution failure.
/// 承载一个结构化供应商执行失败。
#[derive(Debug, Clone)]
pub struct ProviderExecutionFailure {
    /// Normalized public error.
    /// 归一化公共错误。
    pub error: Box<AiExecutionError>,
}

impl std::fmt::Display for ProviderExecutionFailure {
    /// Formats the controlled execution message.
    /// 格式化受控执行消息。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.error.message.as_str())
    }
}

impl std::error::Error for ProviderExecutionFailure {}

/// Stream of normalized public execution events.
/// 归一化公共执行事件流。
pub type ProviderExecutionStream =
    Pin<Box<dyn Stream<Item = Result<AiStreamEvent, ProviderExecutionFailure>> + Send>>;

/// One normalized generated resource returned by a provider tool.
/// 供应商工具返回的一项归一化生成资源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolGeneratedResource {
    /// Stable output identifier scoped to this invocation.
    /// 作用域限定在当前调用内的稳定输出标识。
    pub output_id: String,
    /// Unified semantic resource kind.
    /// 统一语义资源类别。
    pub kind: String,
    /// Exact MIME type.
    /// 精确 MIME 类型。
    pub mime_type: String,
    /// Private provider bytes when returned inline.
    /// 供应商以内联形式返回的私有字节。
    pub bytes: Option<Vec<u8>>,
    /// Public provider download URL when returned by reference.
    /// 供应商以引用形式返回的公开下载 URL。
    pub download_url: Option<String>,
}

/// One normalized provider web-search result.
/// 一条归一化供应商网页搜索结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolSearchResult {
    /// One-based provider result rank.
    /// 从一开始的供应商结果排名。
    pub rank: usize,
    /// Provider-returned result title.
    /// 供应商返回的结果标题。
    pub title: String,
    /// Validated absolute HTTP(S) result URL.
    /// 已校验的绝对 HTTP(S) 结果地址。
    pub url: String,
    /// Normalized source hostname.
    /// 归一化来源主机名。
    pub source_domain: String,
    /// Provider-returned result excerpt.
    /// 供应商返回的结果摘要。
    pub snippet: String,
}

/// Unified structured provider web-search output.
/// 统一结构化供应商网页搜索输出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolSearchOutput {
    /// Exact submitted query.
    /// 精确提交的查询。
    pub query: String,
    /// Ordered normalized results.
    /// 有序归一化结果。
    pub results: Vec<ProviderToolSearchResult>,
}

/// Closed normalized output returned by provider-native tools.
/// 供应商原生工具返回的封闭归一化输出。
/// One normalized successfully extracted web page.
/// 一条归一化的成功网页提取结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExtractResult {
    /// Validated absolute HTTPS source URL.
    /// 已验证的绝对 HTTPS 来源地址。
    pub url: String,
    /// Provider-returned Markdown or plain-text content.
    /// 供应商返回的 Markdown 或纯文本内容。
    pub content: String,
    /// Validated absolute HTTPS image URLs returned for this page.
    /// 当前页面返回的已验证绝对 HTTPS 图片地址。
    pub images: Vec<String>,
    /// Optional validated absolute HTTPS favicon URL.
    /// 可选的已验证绝对 HTTPS 站点图标地址。
    pub favicon: Option<String>,
}

/// One normalized provider-reported extraction failure.
/// 一条归一化的供应商报告提取失败。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExtractFailure {
    /// Validated absolute HTTPS source URL.
    /// 已验证的绝对 HTTPS 来源地址。
    pub url: String,
    /// Controlled provider failure message.
    /// 受控的供应商失败消息。
    pub error: String,
}

/// Unified structured provider web-extraction output.
/// 统一结构化供应商网页提取输出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExtractOutput {
    /// Ordered successful extraction results.
    /// 有序的成功提取结果。
    pub results: Vec<ProviderToolExtractResult>,
    /// Ordered per-URL failures retained for user visibility.
    /// 为用户可见性保留的有序逐 URL 失败。
    pub failures: Vec<ProviderToolExtractFailure>,
}

/// One normalized embedding vector and its source index.
/// 一条归一化向量及其来源索引。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolEmbeddingVector {
    /// Zero-based input index associated with this vector.
    /// 与该向量关联的从零开始输入索引。
    pub index: usize,
    /// Ordered floating-point vector values returned by the provider.
    /// 供应商返回的有序浮点向量值。
    pub values: Vec<f64>,
}

/// Unified embedding output independent of provider wire fields.
/// 与供应商线级字段无关的统一向量输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolEmbeddingOutput {
    /// Optional exact embedding model reported by the provider.
    /// 供应商报告的可选精确向量模型。
    pub model: Option<String>,
    /// Ordered vectors matched to their original input indexes.
    /// 与原始输入索引匹配的有序向量。
    pub vectors: Vec<ProviderToolEmbeddingVector>,
}

/// One normalized reranked candidate.
/// 一条归一化重排候选项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolRerankResult {
    /// Original zero-based candidate index.
    /// 原始的从零开始候选项索引。
    pub index: usize,
    /// Provider-reported relevance score.
    /// 供应商报告的相关性分数。
    pub score: f64,
    /// Optional provider-returned candidate text.
    /// 供应商可选返回的候选文本。
    pub document: Option<String>,
}

/// Unified rerank output independent of provider wire fields.
/// 与供应商线级字段无关的统一重排输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolRerankOutput {
    /// Optional exact rerank model reported by the provider.
    /// 供应商报告的可选精确重排模型。
    pub model: Option<String>,
    /// Ordered candidates in provider-ranked order.
    /// 按供应商排序顺序排列的候选项。
    pub results: Vec<ProviderToolRerankResult>,
}

/// Flat composite output used when one provider operation returns multiple semantic kinds.
/// 当一个供应商操作返回多种语义类型时使用的扁平复合输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolCompositeOutput {
    /// Ordered text fragments.
    /// 有序文本片段。
    pub text: Vec<String>,
    /// Ordered generated resources.
    /// 有序生成资源。
    pub resources: Vec<ProviderToolGeneratedResource>,
    /// Ordered embedding result groups.
    /// 有序向量结果组。
    pub embeddings: Vec<ProviderToolEmbeddingOutput>,
    /// Ordered rerank result groups.
    /// 有序重排结果组。
    pub reranks: Vec<ProviderToolRerankOutput>,
    /// Ordered search result groups.
    /// 有序搜索结果组。
    pub searches: Vec<ProviderToolSearchOutput>,
    /// Ordered extraction result groups.
    /// 有序提取结果组。
    pub extracts: Vec<ProviderToolExtractOutput>,
}

/// Closed normalized output returned by provider-native tools.
/// 供应商原生工具返回的封闭归一化输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ProviderToolNormalizedOutput {
    /// Provider-generated plain text.
    /// 供应商生成的纯文本。
    Text(String),
    /// Ordered generated resources.
    /// 有序生成资源。
    Resources(Vec<ProviderToolGeneratedResource>),
    /// Structured web-search evidence.
    /// 结构化网页搜索证据。
    Search(ProviderToolSearchOutput),
    /// Structured web-extraction content and per-URL failures.
    /// 结构化网页提取内容与逐 URL 失败。
    Extract(ProviderToolExtractOutput),
    /// Embedding vectors with stable source indexes.
    /// 带稳定来源索引的向量结果。
    Embedding(ProviderToolEmbeddingOutput),
    /// Ranked candidates with provider relevance scores.
    /// 带供应商相关性分数的重排候选项。
    Rerank(ProviderToolRerankOutput),
    /// Multiple normalized semantic output kinds from one invocation.
    /// 一次调用返回的多种归一化语义输出。
    Composite(ProviderToolCompositeOutput),
}

/// Unified typed result produced by one provider tool adapter.
/// 一个供应商工具适配器产生的统一强类型结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolExecutionResult {
    /// Optional provider trace or task identifier.
    /// 可选供应商追踪或任务标识。
    pub upstream_response_id: Option<String>,
    /// Closed normalized output independent of the provider wire shape.
    /// 与供应商 Wire 结构无关的封闭归一化输出。
    pub output: ProviderToolNormalizedOutput,
}

/// Executes immutable provider routes through public protocol profiles.
/// 通过公共协议 Profile 执行不可变供应商路由。
#[derive(Clone)]
pub struct ProviderExecutionClient {
    /// Shared configured HTTP client.
    /// 共享的已配置 HTTP 客户端。
    pub(crate) http: reqwest::Client,
    /// Strict protocol profile registry.
    /// 严格协议 Profile 注册表。
    protocols: Arc<ProtocolRegistry>,
    /// Process-local prepared media references shared by preflight and dispatch clones.
    /// 由分派前准备与实际分派副本共享的进程内媒体引用缓存。
    prepared_media: Arc<StdMutex<BTreeMap<String, PreparedMediaCacheEntry>>>,
}

/// Stores one temporary provider media reference until the matching request is dispatched.
/// 在匹配请求分派前保存一个临时供应商媒体引用。
#[derive(Clone)]
struct PreparedMediaCacheEntry {
    /// Provider-native URL or URI used by the exact request projector.
    /// 精确请求投影器使用的供应商原生 URL 或 URI。
    url: String,
    /// Expiry prevents an abandoned submission from retaining a reference forever.
    /// 过期时间防止放弃的提交永久保留远端引用。
    expires_at: Instant,
    /// Source metadata prevents a rewritten file from reusing an older provider reference.
    /// 源文件元数据防止被改写的文件复用旧供应商引用。
    source_fingerprint: PreparedMediaSourceFingerprint,
}

/// Identifies the source-file revision used to create one prepared media reference.
/// 标识创建媒体准备引用时所使用的源文件修订版本。
#[derive(Clone, Debug, Eq, PartialEq)]
struct PreparedMediaSourceFingerprint {
    /// Source byte length at the time the reference was remembered.
    /// 记住引用时的源文件字节长度。
    byte_len: u64,
    /// Last modification time when the host exposes it.
    /// 宿主可以提供时记录源文件最后修改时间。
    modified_at: Option<SystemTime>,
}

/// Reads the stable host metadata used to validate one cached media reference.
/// 读取用于校验媒体缓存引用的稳定宿主元数据。
fn prepared_media_source_fingerprint(local_path: &Path) -> Option<PreparedMediaSourceFingerprint> {
    let metadata = std::fs::metadata(local_path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(PreparedMediaSourceFingerprint {
        byte_len: metadata.len(),
        modified_at: metadata.modified().ok(),
    })
}

/// Builds the process-local cache key for one exact provider media projection.
/// 构建一个精确供应商媒体投影的进程内缓存键。
fn prepared_media_key(
    route: &ProviderExecutionRoute,
    model_id: &str,
    local_path: &Path,
    kind: ResourceKind,
    mime_type: &str,
) -> String {
    format!(
        "{}|{}|{}|{:?}|{}|{}",
        route.provider.id,
        route.connection.id,
        model_id,
        kind,
        mime_type,
        local_path.to_string_lossy(),
    )
}

impl ProviderExecutionClient {
    /// Creates one execution client.
    /// 创建一个执行客户端。
    ///
    /// # Parameters
    /// 参数
    /// `http` is the shared HTTP client and `protocols` is the strict profile registry.
    /// `http` 是共享 HTTP 客户端，`protocols` 是严格 Profile 注册表。
    ///
    /// # Returns
    /// 返回值
    /// A reusable execution client.
    /// 返回可复用执行客户端。
    pub fn new(http: reqwest::Client, protocols: Arc<ProtocolRegistry>) -> Self {
        Self {
            http,
            protocols,
            prepared_media: Arc::new(StdMutex::new(BTreeMap::new())),
        }
    }

    /// Uploads or pre-reads one local media resource before the conversation is materialized.
    /// 在会话物化前上传或预读取一个本地媒体资源。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `model_id`, and `secret` freeze the exact provider account and model contract.
    /// `route`、`model_id` 与 `secret` 固定精确的供应商账号及模型契约。
    /// `local_path` and `mime_type` identify the validated host attachment.
    /// `local_path` 与 `mime_type` 标识已校验的宿主附件。
    /// `progress` receives source-byte progress and `cancellation` owns the submit cancel path.
    /// `progress` 接收源字节进度，`cancellation` 持有提交取消路径。
    ///
    /// # Returns
    /// 返回值
    /// A provider-native reference when the provider has an upload contract, otherwise `None`
    /// after the local inline payload has been completely pre-read.
    /// 供应商存在上传契约时返回原生引用；否则在完整预读取本地内联载荷后返回 `None`。
    pub async fn prepare_local_media(
        &self,
        route: &ProviderExecutionRoute,
        model_id: &str,
        secret: &CredentialExecutionSecret,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        progress: Option<MediaUploadProgressCallback>,
        cancellation: &MediaUploadCancellation,
    ) -> Result<Option<String>, String> {
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(|error| format!("Media source metadata could not be read: {error}"))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(String::from("Media source must be a non-empty file."));
        }
        // The request must never bind a provider reference to a source that changed during preparation.
        // 请求绝不能把供应商引用绑定到准备期间已经发生变化的源文件。
        let source_fingerprint = PreparedMediaSourceFingerprint {
            byte_len: metadata.len(),
            modified_at: metadata.modified().ok(),
        };
        let total_bytes = Some(metadata.len());
        let native_reference = match route.provider.id.as_str() {
            "kimi" if kind == ResourceKind::Video => Some(
                self.upload_multipart_media(
                    route,
                    secret,
                    local_path,
                    mime_type,
                    "/files",
                    "video",
                    "ms://",
                    progress.clone(),
                    cancellation,
                )
                .await?,
            ),
            "minimax" if kind == ResourceKind::Video => Some(
                self.upload_multipart_media(
                    route,
                    secret,
                    local_path,
                    mime_type,
                    "/v1/files/upload",
                    "retrieval",
                    "mm_file://",
                    progress.clone(),
                    cancellation,
                )
                .await?,
            ),
            "alibaba" if matches!(kind, ResourceKind::Image | ResourceKind::Video) => Some(
                crate::alibaba_provider_tools::upload_user_media(
                    &self.http,
                    &route.entry,
                    model_id,
                    secret,
                    local_path,
                    mime_type,
                    metadata.len(),
                    progress.clone(),
                    cancellation,
                )
                .await?,
            ),
            _ => None,
        };
        if native_reference.is_none() {
            self.pre_read_inline_media(local_path, total_bytes, progress, cancellation)
                .await?;
        }
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        let current_source_fingerprint = prepared_media_source_fingerprint(local_path)
            .ok_or_else(|| String::from("Media source disappeared during preparation."))?;
        if current_source_fingerprint != source_fingerprint {
            return Err(String::from(
                "Media source changed during preparation; please retry the submission.",
            ));
        }
        Ok(native_reference)
    }

    /// Remembers one provider-native reference for the exact route and local source.
    /// 为精确路由与本地源记住一个供应商原生引用。
    pub fn remember_prepared_media(
        &self,
        route: &ProviderExecutionRoute,
        model_id: &str,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        url: String,
    ) {
        let Some(source_fingerprint) = prepared_media_source_fingerprint(local_path) else {
            return;
        };
        let key = prepared_media_key(route, model_id, local_path, kind, mime_type);
        if let Ok(mut cache) = self.prepared_media.lock() {
            cache.insert(
                key,
                PreparedMediaCacheEntry {
                    url,
                    expires_at: Instant::now() + Duration::from_secs(30 * 60),
                    source_fingerprint,
                },
            );
        }
    }

    /// Returns a still-valid provider-native reference for the exact local source.
    /// 返回精确本地源仍然有效的供应商原生引用。
    pub fn prepared_media_url(
        &self,
        route: &ProviderExecutionRoute,
        model_id: &str,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
    ) -> Option<String> {
        let source_fingerprint = prepared_media_source_fingerprint(local_path)?;
        let key = prepared_media_key(route, model_id, local_path, kind, mime_type);
        let mut cache = self.prepared_media.lock().ok()?;
        let entry = cache.get(&key)?.clone();
        if entry.expires_at <= Instant::now() || entry.source_fingerprint != source_fingerprint {
            cache.remove(&key);
            return None;
        }
        Some(entry.url)
    }

    /// Streams one provider multipart upload while reporting bytes and honoring cancellation.
    /// 流式执行一次供应商 Multipart 上传，同时报告字节数并响应取消。
    async fn upload_multipart_media(
        &self,
        route: &ProviderExecutionRoute,
        secret: &CredentialExecutionSecret,
        local_path: &Path,
        mime_type: &str,
        path: &str,
        purpose: &str,
        reference_prefix: &str,
        progress: Option<MediaUploadProgressCallback>,
        cancellation: &MediaUploadCancellation,
    ) -> Result<String, String> {
        let metadata = tokio::fs::metadata(local_path)
            .await
            .map_err(|error| format!("Media upload source metadata could not be read: {error}"))?;
        let total_bytes = metadata.len();
        let file = tokio::fs::File::open(local_path)
            .await
            .map_err(|error| format!("Media upload source cannot be opened: {error}"))?;
        let file_name = local_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("resource.bin")
            .to_string();
        let upload_cancellation = cancellation.clone();
        let upload_progress = progress.clone();
        let stream = stream! {
            let mut source = tokio_util::io::ReaderStream::new(file);
            let mut sent = 0_u64;
            while let Some(chunk) = source.next().await {
                if upload_cancellation.is_cancelled() {
                    yield Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "media upload cancelled",
                    ));
                    return;
                }
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        yield Err(error);
                        return;
                    }
                };
                sent = sent.saturating_add(chunk.len() as u64);
                if let Some(callback) = upload_progress.as_ref() {
                    callback(sent, Some(total_bytes));
                }
                yield Ok(chunk);
            }
            if sent != total_bytes {
                yield Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "media upload source length changed during transfer",
                ));
            }
        };
        let part = reqwest::multipart::Part::stream_with_length(
            reqwest::Body::wrap_stream(stream),
            total_bytes,
        )
        .file_name(file_name)
        .mime_str(mime_type)
        .map_err(|error| format!("Media upload MIME type is invalid: {error}"))?;
        let form = reqwest::multipart::Form::new()
            .text("purpose", purpose.to_string())
            .part("file", part);
        let url = route.request_url(path);
        let mut headers = execution_headers(route, &BTreeMap::new(), secret)
            .map_err(|error| error.to_string())?;
        headers.remove(reqwest::header::CONTENT_TYPE);
        let response = tokio::select! {
            response = self.http.post(url.as_str()).headers(headers).multipart(form).send() => response
                .map_err(|error| {
                    if cancellation.is_cancelled() {
                        String::from("media upload cancelled")
                    } else {
                        format!("Media upload to `{url}` failed: {error}")
                    }
                })?,
            _ = cancellation.wait() => return Err(String::from("media upload cancelled")),
        };
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        let status = response.status();
        let body = tokio::select! {
            body = crate::provider_tool_execution::read_bounded_provider_response(
                response,
                "Media upload",
            ) => body.map_err(|error| format!("Media upload response from `{url}` could not be read: {error}"))?,
            _ = cancellation.wait() => return Err(String::from("media upload cancelled")),
        };
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        if !status.is_success() {
            return Err(format!(
                "Media upload failed with HTTP {}: {}",
                status.as_u16(),
                body
            ));
        }
        let body: Value = serde_json::from_str(body.as_str())
            .map_err(|error| format!("Media upload response JSON is invalid: {error}"))?;
        let id = body
            .pointer("/id")
            .and_then(Value::as_str)
            .or_else(|| body.pointer("/file/file_id").and_then(Value::as_str))
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| String::from("Media upload response did not contain a file id."))?;
        Ok(format!("{reference_prefix}{id}"))
    }

    /// Reads an inline media source once before dispatch so every path exposes the same progress lifecycle.
    /// 在分派前读取一次内联媒体源，使所有路径暴露一致的进度生命周期。
    async fn pre_read_inline_media(
        &self,
        local_path: &Path,
        total_bytes: Option<u64>,
        progress: Option<MediaUploadProgressCallback>,
        cancellation: &MediaUploadCancellation,
    ) -> Result<(), String> {
        let file = tokio::fs::File::open(local_path)
            .await
            .map_err(|error| format!("Inline media source cannot be opened: {error}"))?;
        let mut source = tokio_util::io::ReaderStream::new(file);
        let mut processed = 0_u64;
        while let Some(chunk) = source.next().await {
            if cancellation.is_cancelled() {
                return Err(String::from("media upload cancelled"));
            }
            let chunk =
                chunk.map_err(|error| format!("Inline media source could not be read: {error}"))?;
            processed = processed.saturating_add(chunk.len() as u64);
            if let Some(callback) = progress.as_ref() {
                callback(processed, total_bytes);
            }
        }
        Ok(())
    }

    /// Executes one frozen provider-native tool target and normalizes its output.
    /// 执行一个已冻结的供应商原生工具目标并归一化其输出。
    ///
    /// # Parameters
    /// 参数
    /// `entry`, `target`, and `secret` are immutable snapshots resolved before invocation;
    /// `arguments` already match the selected target's exact AI-visible schema.
    /// `entry`、`target` 与 `secret` 是调用前解析的不可变快照；
    /// `arguments` 已符合选定目标的精确 AI 可见 Schema。
    ///
    /// # Returns
    /// 返回值
    /// A unified typed result or an explicit provider/contract failure.
    /// 返回统一强类型结果或明确的供应商与契约错误。
    pub async fn execute_provider_tool(
        &self,
        entry: &ServiceEntry,
        target: &ProviderToolTarget,
        arguments: Value,
        secret: &CredentialExecutionSecret,
    ) -> Result<ProviderToolExecutionResult, String> {
        if target.service_entry_id != entry.id {
            return Err(format!(
                "Provider tool target `{}` does not belong to service entry `{}`.",
                target.target_id, entry.id
            ));
        }
        crate::provider_tool_execution::execute_provider_tool(
            &self.http, entry, target, arguments, secret,
        )
        .await
    }

    /// Encodes one immutable execution request without dispatching it.
    /// 编码一个不可变执行请求，但不进行网络分派。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the frozen provider route and `request` is the provider-neutral request.
    /// `route` 是冻结的供应商路由，`request` 是供应商无关请求。
    ///
    /// # Returns
    /// 返回值
    /// The exact projected wire request or a controlled validation failure.
    /// 返回精确投影的线级请求或受控校验失败。
    pub fn encode_request(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let profile = self
            .protocols
            .require(route.protocol_route.protocol_profile_id.as_str())
            .map_err(protocol_failure)?;
        validate_request_policy(route, request)?;
        let encoded = profile.encode(request).map_err(protocol_failure)?;
        project_provider_request(route, request, encoded)
    }

    /// Encodes one immutable request with credential-bound non-secret identity projection.
    /// 编码一个不可变请求，并投影凭据绑定的非秘密身份信息。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` form the public route projection; `secret` supplies only the
    /// account metadata required in the wire body.
    /// `route` 与 `request` 构成公共路由投影；`secret` 仅提供线级请求体所需的账号元数据。
    ///
    /// # Returns
    /// 返回值
    /// The exact request body used by dispatch and wire preview, without authentication headers.
    /// 返回分派与线级预览共同使用的精确请求体，不包含认证请求头。
    pub fn encode_request_with_credential(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let encoded = self.encode_request(route, request)?;
        project_credential_request(route, secret, encoded)
    }

    /// Counts one complete Gemini API request with the provider-authoritative tokenizer.
    /// 使用供应商权威 Tokenizer 统计一个完整 Gemini API 请求。
    ///
    /// # Parameters
    /// 参数
    /// `route` must be the verified public Gemini GenerateContent route, `request` is the
    /// immutable provider-neutral request, and `secret` is its exact selected credential.
    /// `route` 必须是已验证的公共 Gemini GenerateContent 路由，`request` 是不可变的
    /// 供应商无关请求，`secret` 是其精确选定凭据。
    ///
    /// # Returns
    /// 返回值
    /// The exact provider count and modality details, or a controlled pre-dispatch,
    /// transport, HTTP, or protocol failure.
    /// 返回精确供应商统计与模态明细，或受控的分派前、传输、HTTP 或协议错误。
    pub async fn count_input_tokens(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<InputTokenCount, ProviderExecutionFailure> {
        if route.provider.id != "google"
            || !matches!(route.entry.id.as_str(), "gemini-api" | "gemini-oauth")
            || route.protocol_route.protocol_profile_id
                != vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID
        {
            return Err(pre_dispatch_failure(
                "Provider-authoritative CountTokens is only registered for the public Gemini GenerateContent route.",
            ));
        }
        validate_request_policy(route, request)?;
        let encoded = encode_gemini_count_tokens_request(request).map_err(protocol_failure)?;
        let url = route.request_url(encoded.path.as_str());
        let headers = execution_headers(route, &encoded.headers, secret)?;
        let response = self
            .http
            .post(url.as_str())
            .headers(headers)
            .json(&encoded.body)
            .send()
            .await
            .map_err(|error| {
                transport_failure(format!(
                    "Provider CountTokens request to `{url}` failed: {error}"
                ))
            })?;
        let status = response.status();
        if !status.is_success() {
            let retry_after = parse_retry_after_headers(response.headers(), SystemTime::now());
            let body = response.text().await.map_err(|error| {
                transport_failure(format!(
                    "Provider CountTokens error response from `{url}` could not be read: {error}"
                ))
            })?;
            return Err(http_failure(status.as_u16(), retry_after, body.as_str()));
        }
        let body = response.json::<Value>().await.map_err(|error| {
            protocol_failure(ProtocolConversionError::new(format!(
                "Gemini CountTokens response JSON is invalid: {error}"
            )))
        })?;
        decode_gemini_count_tokens_response(&body).map_err(protocol_failure)
    }

    /// Starts one immutable execution without protocol or account fallback.
    /// 启动一次不可变执行，不进行协议或账号回退。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the resolved immutable route, `request` is the public request,
    /// `secret` is the exact host-loaded credential, and `execution_id` is stable.
    /// `route` 是解析后的不可变路由，`request` 是公共请求，`secret` 是宿主加载的精确凭据，
    /// `execution_id` 是稳定标识。
    ///
    /// # Returns
    /// 返回值
    /// A normalized event stream.
    /// 返回归一化事件流。
    pub fn execute(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
    ) -> ProviderExecutionStream {
        let client = self.clone();
        Box::pin(try_stream! {
            let encoded = client.encode_request_with_credential(&route, &request, &secret)?;
            let profile = client
                .protocols
                .require(route.protocol_route.protocol_profile_id.as_str())
                .map_err(protocol_failure)?;
            let transport = profile.transport_profile();
            let url = route.request_url(encoded.path.as_str());
            let mut headers = execution_headers(&route, &encoded.headers, &secret)?;
            // Alibaba requires an explicit resolver switch whenever the request carries an OSS reference.
            // 阿里云请求携带 OSS 引用时必须显式开启对象解析开关。
            if route.provider.id == "alibaba"
                && serde_json::to_string(&encoded.body)
                    .is_ok_and(|body| body.contains("oss://"))
            {
                headers.insert(
                    HeaderName::from_static("x-dashscope-ossresource-resolve"),
                    HeaderValue::from_static("enable"),
                );
            }
            let response = tokio::time::timeout(
                Duration::from_millis(transport.response_headers_timeout_ms),
                client
                    .http
                    .post(url.as_str())
                    .headers(headers)
                    .json(&encoded.body)
                    .send(),
            )
            .await
            .map_err(|_| {
                transport_failure(format!(
                    "Provider request to `{url}` exceeded the registered {} ms response-header timeout.",
                    transport.response_headers_timeout_ms
                ))
            })?
            .map_err(|error| transport_failure(format!(
                "Provider request to `{url}` failed: {error}"
            )))?;
            let status = response.status();
            let response = if status.is_success() {
                response
            } else {
                let retry_after =
                    parse_retry_after_headers(response.headers(), SystemTime::now());
                let body = read_bounded_response_text(
                    response,
                    transport.max_error_body_bytes,
                    url.as_str(),
                    status.as_u16(),
                    retry_after.clone(),
                )
                .await?;
                Err(http_failure(
                    status.as_u16(),
                    retry_after,
                    body.as_str(),
                ))?
            };
            validate_response_content_type(
                response.headers(),
                &transport,
                url.as_str(),
            )?;
            let mut decoder = profile.create_stream_decoder(execution_id, &request);
            let mut parser = IncrementalFrameParser::new(transport);
            let mut bytes = response.bytes_stream();
            let mut semantic_output_observed = false;
            let mut stream_state = AiStreamStateMachine::default();
            loop {
                let next_chunk = tokio::time::timeout(
                    Duration::from_millis(transport.stream_idle_timeout_ms),
                    bytes.next(),
                )
                .await
                .map_err(|_| {
                    stream_transport_failure(
                        format!(
                            "Provider stream from `{url}` exceeded the registered {} ms idle timeout.",
                            transport.stream_idle_timeout_ms
                        ),
                        semantic_output_observed,
                    )
                })?;
                let Some(chunk) = next_chunk else {
                    break;
                };
                let chunk = chunk.map_err(|error| {
                    stream_transport_failure(
                        format!("Provider stream from `{url}` failed: {error}"),
                        semantic_output_observed,
                    )
                })?;
                for frame in parser
                    .push(chunk.as_ref())
                    .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
                {
                    for mut event in decoder
                        .push_sse(frame.event_name.as_deref(), frame.data.as_str())
                        .map_err(|error| {
                            stream_protocol_failure(error, semantic_output_observed)
                        })?
                    {
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut event,
                        );
                        validate_stream_event(&mut stream_state, &event, semantic_output_observed)?;
                        yield event;
                    }
                }
            }
            for frame in parser
                .finish()
                .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
            {
                for mut event in decoder
                    .push_sse(frame.event_name.as_deref(), frame.data.as_str())
                    .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
                {
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut event,
                    );
                    validate_stream_event(&mut stream_state, &event, semantic_output_observed)?;
                    yield event;
                }
            }
            for mut event in decoder
                .finish()
                .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
            {
                update_semantic_output_observation(
                    &mut semantic_output_observed,
                    &mut event,
                );
                validate_stream_event(&mut stream_state, &event, semantic_output_observed)?;
                yield event;
            }
        })
    }
}

/// Tracks semantic output and annotates terminal public failures with the observed state.
/// 跟踪语义输出并用已观测状态标注公共终态失败。
fn update_semantic_output_observation(observed: &mut bool, event: &mut AiStreamEvent) {
    match event {
        AiStreamEvent::ResponseTextDelta { .. }
        | AiStreamEvent::ReasoningDelta { .. }
        | AiStreamEvent::ToolCallStarted { .. }
        | AiStreamEvent::ResourceProgress { .. } => {
            *observed = true;
        }
        AiStreamEvent::ResponseFailed { error } => {
            error.semantic_output_observed = *observed;
        }
        _ => {}
    }
}

/// Applies the production stream state machine and preserves the physical request state on failure.
/// 应用生产流状态机，并在失败时保留物理请求状态。
///
/// # Parameters
/// 参数
/// `state` owns lifecycle validation, `event` is the next normalized event, and `observed`
/// records whether semantic output has already escaped the transport boundary.
/// `state` 持有生命周期校验，`event` 是下一条规范化事件，`observed`
/// 记录语义输出是否已经越过传输边界。
///
/// # Returns
/// 返回值
/// `Ok(())` for a valid transition or a structured dispatched protocol failure.
/// 有效转换返回 `Ok(())`，否则返回结构化的已分派协议失败。
fn validate_stream_event(
    state: &mut AiStreamStateMachine,
    event: &AiStreamEvent,
    observed: bool,
) -> Result<(), ProviderExecutionFailure> {
    state
        .apply(event)
        .map_err(|message| stream_protocol_failure(ProtocolConversionError::new(message), observed))
}

/// Applies route-scoped request policy before encoding.
/// 在编码前应用路由作用域请求策略。
fn validate_request_policy(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> Result<(), ProviderExecutionFailure> {
    for parameter_id in request.model_parameters.keys() {
        let registered_temperature_route = parameter_id == SAMPLING_TEMPERATURE_PARAMETER_ID
            && ((route.entry.request_projector == RequestProjectorKind::MiniMax
                && matches!(
                    route.protocol_route.protocol_profile_id.as_str(),
                    vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
                        | vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID
                ))
                || (route.entry.request_projector == RequestProjectorKind::DeepSeek
                    && matches!(
                        route.protocol_route.protocol_profile_id.as_str(),
                        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
                            | vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID
                    ))
                || route.entry.request_projector == RequestProjectorKind::OpenRouter);
        let registered_minimax_reasoning_split = parameter_id
            == MINIMAX_REASONING_SPLIT_PARAMETER_ID
            && route.entry.request_projector == RequestProjectorKind::MiniMax
            && route.protocol_route.protocol_profile_id
                == vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID;
        let registered_openrouter_reasoning_budget = parameter_id
            == OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID
            && route.entry.request_projector == RequestProjectorKind::OpenRouter;
        let registered_custom_parameter = route
            .custom_model_projection
            .as_ref()
            .is_some_and(|projection| projection.model_parameter_paths.contains_key(parameter_id));
        if !registered_temperature_route
            && !registered_minimax_reasoning_split
            && !registered_openrouter_reasoning_budget
            && !registered_custom_parameter
        {
            return Err(pre_dispatch_failure(format!(
                "Model-specific parameter `{parameter_id}` has no registered route projector."
            )));
        }
    }
    if !matches!(request.prompt_cache, PromptCachePolicy::ProviderDefault)
        && route.entry.request_projector == RequestProjectorKind::OpenRouter
    {
        return Err(pre_dispatch_failure(
            "OpenRouter explicit prompt caching requires a verified model-endpoint capability.",
        ));
    }
    if request.reasoning_effort.is_some()
        && request
            .model_parameters
            .contains_key(OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID)
    {
        return Err(pre_dispatch_failure(
            "OpenRouter reasoning effort and reasoning max_tokens are mutually exclusive.",
        ));
    }
    Ok(())
}

/// Applies provider-only fields after protocol encoding.
/// 在协议编码后应用仅供应商字段。
fn project_provider_request(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    mut encoded: EncodedWireRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    if let Some(endpoint_path) = route.protocol_route.endpoint_path.as_ref() {
        encoded.path = endpoint_path.clone();
    }
    // DeepSeek V4 enables thinking by default, where temperature is documented as ineffective.
    // DeepSeek V4 默认启用思考模式，而官方文档说明该模式下温度参数不生效。
    if route.entry.request_projector != RequestProjectorKind::DeepSeek
        || request.reasoning_effort == Some(ReasoningEffort::None)
    {
        project_sampling_temperature(request, &mut encoded)?;
    }
    match route.entry.request_projector {
        RequestProjectorKind::Direct | RequestProjectorKind::Google => {}
        RequestProjectorKind::MiniMax => {
            project_minimax_request(route, request, &mut encoded)?;
        }
        RequestProjectorKind::OpenRouter => {
            project_openrouter_request(route, request, &mut encoded)?;
        }
        RequestProjectorKind::Kimi => {
            project_kimi_request(route, request, &mut encoded)?;
        }
        RequestProjectorKind::DeepSeek => {
            project_deepseek_request(route, request, &mut encoded)?;
        }
        RequestProjectorKind::Alibaba => {
            project_alibaba_request(route, request, &mut encoded)?;
        }
    }
    if let Some(projection) = route.custom_model_projection.as_ref() {
        project_custom_model_request(projection, request, &mut encoded)?;
    }
    validate_structured_output_projection(route, request, &encoded)?;
    Ok(encoded)
}

/// Verifies protocol-native JSON Schema fields survive provider-specific request projection.
/// 验证协议原生 JSON Schema 字段在供应商专属请求投影后仍然存在。
///
/// # Parameters
/// 参数
/// `route` identifies the exact protocol, `request` carries the requested schema, and `encoded`
/// is the final request after all provider and custom projections.
/// `route` 标识精确协议，`request` 携带请求 Schema，`encoded` 是完成全部供应商与自定义投影后的最终请求。
///
/// # Returns
/// 返回值
/// Success when the protocol carries the exact requested schema, or a pre-dispatch failure when
/// a supported Chat/Responses/Messages projection dropped or changed it.
/// 当协议携带精确请求 Schema 时返回成功；支持的 Chat、Responses、Messages 投影丢失或改写 Schema 时返回分派前失败。
fn validate_structured_output_projection(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    let StructuredOutputPolicy::JsonSchema { schema, .. } = &request.structured_output else {
        return Ok(());
    };
    let protocol_id = route.protocol_route.protocol_profile_id.as_str();
    let (projected_type, projected_schema) = match protocol_id {
        ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | CLAUDE_CODE_MESSAGES_PROFILE_ID => (
            encoded
                .body
                .pointer("/output_config/format/type")
                .and_then(Value::as_str),
            encoded.body.pointer("/output_config/format/schema"),
        ),
        OPENAI_CHAT_PROFILE_ID => (
            encoded
                .body
                .pointer("/response_format/type")
                .and_then(Value::as_str),
            encoded.body.pointer("/response_format/json_schema/schema"),
        ),
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => (
            encoded
                .body
                .pointer("/text/format/type")
                .and_then(Value::as_str),
            encoded.body.pointer("/text/format/schema"),
        ),
        _ => return Ok(()),
    };
    if projected_type == Some("json_schema") && projected_schema == Some(schema) {
        return Ok(());
    }
    Err(pre_dispatch_failure(format!(
        "Protocol `{protocol_id}` dropped or changed the requested structured-output JSON Schema.",
    )))
}

/// Applies one validated custom model projection after standard protocol encoding.
/// 在标准协议编码后应用一个已校验的自定义模型投影。
fn project_custom_model_request(
    projection: &CustomModelRequestProjection,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    for (path, value) in &projection.defaults {
        set_json_path(&mut encoded.body, path, value.clone(), false)?;
    }
    for (parameter_id, path) in &projection.model_parameter_paths {
        if let Some(value) = request.model_parameters.get(parameter_id) {
            let value = model_parameter_json_value(value)?;
            set_json_path(&mut encoded.body, path, value, true)?;
        }
    }
    if let Some(reasoning_effort) = request.reasoning_effort
        && let Some(mutation) = projection.reasoning.get(&reasoning_effort)
    {
        for path in &mutation.delete {
            delete_json_path(&mut encoded.body, path)?;
        }
        for (path, value) in &mutation.set {
            set_json_path(&mut encoded.body, path, value.clone(), true)?;
        }
    }
    for path in &projection.delete {
        delete_json_path(&mut encoded.body, path)?;
    }
    for (path, value) in &projection.overrides {
        set_json_path(&mut encoded.body, path, value.clone(), true)?;
    }
    Ok(())
}

/// Converts one strongly typed model value into its provider JSON value.
/// 将一个强类型模型值转换为供应商 JSON 值。
fn model_parameter_json_value(
    value: &ModelParameterValue,
) -> Result<Value, ProviderExecutionFailure> {
    match value {
        ModelParameterValue::Boolean(value) => Ok(Value::Bool(*value)),
        ModelParameterValue::Integer(value) => Ok(Value::Number((*value).into())),
        ModelParameterValue::Decimal(value) => serde_json::from_str(value).map_err(|error| {
            pre_dispatch_failure(format!(
                "Custom decimal model parameter `{value}` is invalid JSON: {error}"
            ))
        }),
        ModelParameterValue::String(value) => Ok(Value::String(value.clone())),
        ModelParameterValue::Array(values) => values
            .iter()
            .map(model_parameter_json_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        ModelParameterValue::Object(values) => values
            .iter()
            .map(|(key, value)| Ok((key.clone(), model_parameter_json_value(value)?)))
            .collect::<Result<serde_json::Map<_, _>, ProviderExecutionFailure>>()
            .map(Value::Object),
    }
}

/// Sets one validated dot-separated object path.
/// 设置一条已校验的点分隔对象路径。
fn set_json_path(
    root: &mut Value,
    path: &str,
    value: Value,
    replace: bool,
) -> Result<(), ProviderExecutionFailure> {
    let segments = path.split('.').collect::<Vec<_>>();
    let (leaf, parents) = segments
        .split_last()
        .ok_or_else(|| pre_dispatch_failure("Custom request path is empty."))?;
    let mut current = root;
    for segment in parents {
        let object = current.as_object_mut().ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Custom request path `{path}` crosses a non-object value."
            ))
        })?;
        current = object
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    let object = current.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure(format!(
            "Custom request path `{path}` targets a non-object parent."
        ))
    })?;
    if object.contains_key(*leaf) && !replace {
        return Ok(());
    }
    object.insert((*leaf).to_string(), value);
    Ok(())
}

/// Deletes one validated dot-separated object path when present.
/// 在存在时删除一条已校验的点分隔对象路径。
fn delete_json_path(root: &mut Value, path: &str) -> Result<(), ProviderExecutionFailure> {
    let segments = path.split('.').collect::<Vec<_>>();
    let (leaf, parents) = segments
        .split_last()
        .ok_or_else(|| pre_dispatch_failure("Custom request path is empty."))?;
    let mut current = root;
    for segment in parents {
        let Some(object) = current.as_object_mut() else {
            return Ok(());
        };
        let Some(next) = object.get_mut(*segment) else {
            return Ok(());
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut() {
        object.remove(*leaf);
    }
    Ok(())
}

/// Projects the shared sampling-temperature parameter into one registered provider body.
/// 将共享采样温度参数投影到一个已注册供应商请求体。
///
/// # Parameters
/// 参数
/// `request` contains the typed parameter and `encoded` is the mutable wire request.
/// `request` 包含强类型参数，`encoded` 是可变 Wire 请求。
///
/// # Returns
/// 返回值
/// Success when absent or projected without conflict.
/// 缺失或无冲突投影时返回成功。
fn project_sampling_temperature(
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    let Some(value) = request
        .model_parameters
        .get(SAMPLING_TEMPERATURE_PARAMETER_ID)
    else {
        return Ok(());
    };
    let ModelParameterValue::Decimal(decimal) = value else {
        return Err(pre_dispatch_failure(
            "The registered sampling temperature value must be a decimal string.",
        ));
    };
    let temperature = serde_json::from_str::<Value>(decimal).map_err(|error| {
        pre_dispatch_failure(format!(
            "Sampling temperature could not be projected as JSON: {error}"
        ))
    })?;
    let body = request_body_mut(encoded, "Protocol")?;
    if body.contains_key("temperature") {
        return Err(pre_dispatch_failure(
            "Sampling temperature projection conflicts with an existing protocol field.",
        ));
    }
    body.insert(String::from("temperature"), temperature);
    Ok(())
}

/// Applies MiniMax-only request fields and request-size policy.
/// 应用 MiniMax 专属请求字段与请求尺寸策略。
///
/// # Parameters
/// 参数
/// `route` freezes the selected protocol, `request` carries typed model parameters,
/// and `encoded` is the protocol request.
/// `route` 冻结所选协议，`request` 承载强类型模型参数，`encoded` 是协议请求。
///
/// # Returns
/// 返回值
/// Success for the documented provider-controlled reasoning shape.
/// 对文档化的供应商控制思考形态返回成功。
fn project_minimax_request(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    if let Some(effort) = request.reasoning_effort {
        let thinking_type = match effort {
            ReasoningEffort::None => "disabled",
            ReasoningEffort::Adaptive => "adaptive",
            unsupported => {
                return Err(pre_dispatch_failure(format!(
                    "MiniMax-M3 exposes only `none` and `adaptive` reasoning controls; received `{}`.",
                    unsupported.as_str()
                )));
            }
        };
        let body = request_body_mut(encoded, "MiniMax")?;
        match route.protocol_route.protocol_profile_id.as_str() {
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID => {
                // MiniMax Anthropic uses `thinking.type`; generic Anthropic `output_config.effort` is not accepted.
                // MiniMax Anthropic 使用 `thinking.type`；通用 Anthropic `output_config.effort` 不被接受。
                project_anthropic_output_config_effort(body, None, "MiniMax Anthropic")?;
                body.insert(String::from("thinking"), json!({ "type": thinking_type }));
            }
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => {
                // MiniMax OpenAI Chat uses `thinking.type`; `reasoning_effort` belongs to other OpenAI-compatible APIs.
                // MiniMax OpenAI Chat 使用 `thinking.type`；`reasoning_effort` 属于其他 OpenAI 兼容 API。
                body.remove("reasoning_effort");
                body.insert(String::from("thinking"), json!({ "type": thinking_type }));
            }
            protocol_id => {
                return Err(pre_dispatch_failure(format!(
                    "MiniMax reasoning projection is not registered for protocol `{protocol_id}`."
                )));
            }
        }
    }
    if let Some(value) = request
        .model_parameters
        .get(MINIMAX_REASONING_SPLIT_PARAMETER_ID)
    {
        if route.protocol_route.protocol_profile_id != vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID {
            return Err(pre_dispatch_failure(
                "MiniMax reasoning_split is registered only for OpenAI Chat.",
            ));
        }
        let ModelParameterValue::Boolean(reasoning_split) = value else {
            return Err(pre_dispatch_failure(
                "The registered MiniMax reasoning split value must be boolean.",
            ));
        };
        request_body_mut(encoded, "MiniMax")?.insert(
            String::from("reasoning_split"),
            Value::Bool(*reasoning_split),
        );
    }
    let body_size = serde_json::to_vec(&encoded.body)
        .map_err(|error| {
            pre_dispatch_failure(format!(
                "MiniMax request body could not be measured before dispatch: {error}"
            ))
        })?
        .len();
    if body_size > MINIMAX_REQUEST_BODY_BYTE_LIMIT {
        return Err(pre_dispatch_failure(format!(
            "MiniMax request body is {body_size} bytes and exceeds the documented {MINIMAX_REQUEST_BODY_BYTE_LIMIT}-byte limit."
        )));
    }
    Ok(())
}

/// Applies OpenRouter reasoning and pinned secondary-provider fields.
/// 应用 OpenRouter 思考与固定二级供应商字段。
///
/// # Parameters
/// 参数
/// `route` contains the frozen protocol and endpoint selection, `request` contains typed
/// reasoning controls, and `encoded` is the protocol request.
/// `route` 包含冻结协议与端点选择，`request` 包含强类型思考控制，
/// `encoded` 是协议请求。
///
/// # Returns
/// 返回值
/// Success when every requested field has an exact registered projection.
/// 每个请求字段都有精确注册投影时返回成功。
fn project_openrouter_request(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    if let Some(value) = request
        .model_parameters
        .get(OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID)
    {
        let ModelParameterValue::Integer(max_tokens) = value else {
            return Err(pre_dispatch_failure(
                "The registered OpenRouter reasoning token budget must be an integer.",
            ));
        };
        let reasoning = request_body_mut(encoded, "OpenRouter")?
            .entry(String::from("reasoning"))
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| {
                pre_dispatch_failure("OpenRouter reasoning projection must be a JSON object.")
            })?;
        reasoning.insert(String::from("max_tokens"), json!(max_tokens));
    }
    if let Some(effort) = request.reasoning_effort {
        let body = request_body_mut(encoded, "OpenRouter")?;
        match route.protocol_route.protocol_profile_id.as_str() {
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => {
                body.remove("reasoning_effort");
            }
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID => {
                project_anthropic_output_config_effort(body, None, "OpenRouter Anthropic")?;
            }
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID => {}
            protocol_id => {
                return Err(pre_dispatch_failure(format!(
                    "OpenRouter reasoning projection is not registered for protocol `{protocol_id}`."
                )));
            }
        }
        let reasoning = body
            .entry(String::from("reasoning"))
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| {
                pre_dispatch_failure("OpenRouter reasoning projection must be a JSON object.")
            })?;
        if effort == ReasoningEffort::Adaptive {
            // OpenRouter uses `enabled` when discovery exposes reasoning without selectable effort levels.
            // 当发现结果只暴露推理开关而没有强度档位时，OpenRouter 使用 `enabled`。
            reasoning.remove("effort");
            reasoning.insert(String::from("enabled"), Value::Bool(true));
        } else {
            reasoning.remove("enabled");
            reasoning.insert(String::from("effort"), json!(effort.as_str()));
        }
    }
    if let SecondaryRouteSelection::PinnedEndpoint { route_id, .. } = &route.secondary_route {
        let body = request_body_mut(encoded, "OpenRouter")?;
        if body.contains_key("provider") {
            return Err(pre_dispatch_failure(
                "OpenRouter secondary-route projection conflicts with an existing provider field.",
            ));
        }
        body.insert(
            String::from("provider"),
            json!({
                "order": [route_id],
                "allow_fallbacks": false,
            }),
        );
    }
    Ok(())
}

/// Applies Kimi Coding Plan thinking controls.
/// 应用 Kimi Coding Plan 思考控制。
///
/// # Parameters
/// 参数
/// `route`, `request`, and `encoded` are the immutable route, public semantics, and wire request.
/// `route`、`request` 与 `encoded` 分别是不可变路由、公共语义和 Wire 请求。
///
/// # Returns
/// 返回值
/// Success for the exact registered Chat projection.
/// 对精确注册的 Chat 投影返回成功。
fn project_kimi_request(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    if route.protocol_route.protocol_profile_id != vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID {
        return Ok(());
    }
    let force_coding_model = matches!(
        request.model_id.as_str(),
        "k3" | "k3-256k" | "kimi-for-coding" | "kimi-for-coding-highspeed"
    );
    let reasoning_requested = request
        .reasoning_effort
        .is_some_and(|effort| effort != ReasoningEffort::None);
    let body = request_body_mut(encoded, "Kimi")?;
    body.remove("reasoning_effort");
    if force_coding_model || reasoning_requested {
        body.insert(String::from("thinking"), json!({ "type": "enabled" }));
    }
    Ok(())
}

/// Applies DeepSeek protocol-specific reasoning controls.
/// 应用 DeepSeek 协议专属思考控制。
///
/// # Parameters
/// 参数
/// `route`, `request`, and `encoded` are the frozen route, public request, and wire request.
/// `route`、`request` 与 `encoded` 是冻结路由、公共请求与 Wire 请求。
///
/// # Returns
/// 返回值
/// Success for one exact documented effort mapping.
/// 对一个精确文档化档位映射返回成功。
fn project_deepseek_request(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    if route.protocol_route.protocol_profile_id == vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
    {
        let body = request_body_mut(encoded, "DeepSeek Responses")?;
        // DeepSeek documents these Responses fields as unsupported even though it may ignore them.
        // DeepSeek 文档明确列出这些 Responses 字段不支持，即使上游可能静默忽略它们。
        body.remove("store");
        body.remove("include");
        return Ok(());
    }
    let Some(effort) = request.reasoning_effort else {
        return Ok(());
    };
    match route.protocol_route.protocol_profile_id.as_str() {
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => {
            let body = request_body_mut(encoded, "DeepSeek")?;
            match effort {
                ReasoningEffort::None => {
                    body.remove("reasoning_effort");
                    body.insert(String::from("thinking"), json!({ "type": "disabled" }));
                }
                ReasoningEffort::High | ReasoningEffort::Max => {
                    body.insert(String::from("thinking"), json!({ "type": "enabled" }));
                }
                unsupported => {
                    return Err(pre_dispatch_failure(format!(
                        "DeepSeek does not expose reasoning effort `{}`.",
                        unsupported.as_str()
                    )));
                }
            }
        }
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID => {
            let body = request_body_mut(encoded, "DeepSeek Anthropic")?;
            match effort {
                ReasoningEffort::None => {
                    project_anthropic_output_config_effort(body, None, "DeepSeek Anthropic")?;
                    body.insert(String::from("thinking"), json!({ "type": "disabled" }));
                }
                ReasoningEffort::High | ReasoningEffort::Max => {
                    body.insert(String::from("thinking"), json!({ "type": "enabled" }));
                    project_anthropic_output_config_effort(
                        body,
                        Some(effort.as_str()),
                        "DeepSeek Anthropic",
                    )?;
                }
                unsupported => {
                    return Err(pre_dispatch_failure(format!(
                        "DeepSeek Anthropic does not expose reasoning effort `{}`.",
                        unsupported.as_str()
                    )));
                }
            }
        }
        protocol_id => {
            return Err(pre_dispatch_failure(format!(
                "DeepSeek reasoning projection is not registered for protocol `{protocol_id}`."
            )));
        }
    }
    Ok(())
}

/// Applies Alibaba protocol-specific thinking controls.
/// 应用阿里云协议专属思考控制。
///
/// # Parameters
/// 参数
/// `route`, `request`, and `encoded` are the frozen route, public request, and wire request.
/// `route`、`request` 与 `encoded` 是冻结路由、公共请求与 Wire 请求。
///
/// # Returns
/// 返回值
/// Success for disabled or adaptive provider thinking.
/// 对关闭或供应商自适应思考返回成功。
fn project_alibaba_request(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    let Some(effort) = request.reasoning_effort else {
        return Ok(());
    };
    let enabled = match effort {
        ReasoningEffort::None => false,
        ReasoningEffort::Adaptive => true,
        unsupported => {
            return Err(pre_dispatch_failure(format!(
                "Alibaba compatible routes do not expose reasoning effort `{}`.",
                unsupported.as_str()
            )));
        }
    };
    let body = request_body_mut(encoded, "Alibaba")?;
    match route.protocol_route.protocol_profile_id.as_str() {
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => {
            body.remove("reasoning_effort");
            body.insert(String::from("enable_thinking"), Value::Bool(enabled));
        }
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID => {
            project_anthropic_output_config_effort(body, None, "Alibaba Anthropic")?;
            body.insert(
                String::from("thinking"),
                json!({ "type": if enabled { "enabled" } else { "disabled" } }),
            );
        }
        protocol_id => {
            return Err(pre_dispatch_failure(format!(
                "Alibaba reasoning projection is not registered for protocol `{protocol_id}`."
            )));
        }
    }
    Ok(())
}

/// Borrows one encoded request body as a JSON object with a provider-specific diagnostic.
/// 以供应商专属诊断把编码请求体借用为 JSON 对象。
///
/// # Parameters
/// 参数
/// `encoded` is the mutable wire request and `label` identifies its projector.
/// `encoded` 是可变 Wire 请求，`label` 标识其投影器。
///
/// # Returns
/// 返回值
/// The mutable JSON object, or a controlled pre-dispatch failure.
/// 返回可变 JSON 对象，或受控分派前失败。
fn request_body_mut<'a>(
    encoded: &'a mut EncodedWireRequest,
    label: &str,
) -> Result<&'a mut serde_json::Map<String, Value>, ProviderExecutionFailure> {
    encoded
        .body
        .as_object_mut()
        .ok_or_else(|| pre_dispatch_failure(format!("{label} request body must be a JSON object.")))
}

/// Updates only Anthropic reasoning effort while preserving native structured-output format fields.
/// 仅更新 Anthropic 的思考强度，同时保留原生结构化输出格式字段。
///
/// # Parameters
/// 参数
/// `body` is the encoded Anthropic request, `effort` is the provider-supported value or removal,
/// and `label` identifies the exact provider projection in controlled failures.
/// `body` 是已编码的 Anthropic 请求，`effort` 是供应商支持的值或移除指令，`label` 用于在受控错误中标识精确供应商投影。
///
/// # Returns
/// 返回值
/// Success after preserving sibling fields such as `format`, or a pre-dispatch shape failure.
/// 保留 `format` 等同级字段后返回成功，或者返回分派前结构错误。
fn project_anthropic_output_config_effort(
    body: &mut serde_json::Map<String, Value>,
    effort: Option<&str>,
    label: &str,
) -> Result<(), ProviderExecutionFailure> {
    if effort.is_none() && !body.contains_key("output_config") {
        return Ok(());
    }
    let output_config = body
        .entry(String::from("output_config"))
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            pre_dispatch_failure(format!(
                "{label} output_config projection must be a JSON object."
            ))
        })?;
    match effort {
        Some(value) => {
            output_config.insert(String::from("effort"), Value::String(value.to_string()));
        }
        None => {
            output_config.remove("effort");
        }
    }
    if output_config.is_empty() {
        body.remove("output_config");
    }
    Ok(())
}

/// Applies credential-bound non-secret identity fields immediately before dispatch.
/// 在分派前立即应用凭据绑定的非秘密身份字段。
///
/// # Parameters
/// 参数
/// `route` fixes the exact protocol, `secret` carries validated account metadata, and
/// `encoded` is the provider-projected request.
/// `route` 固定精确协议，`secret` 承载已验证账号元数据，`encoded` 是供应商投影请求。
///
/// # Returns
/// 返回值
/// A request containing required credential identity without exposing access material in JSON.
/// 返回包含所需凭据身份且不在 JSON 中暴露访问材料的请求。
fn project_credential_request(
    route: &ProviderExecutionRoute,
    secret: &CredentialExecutionSecret,
    mut encoded: EncodedWireRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    if route.protocol_route.protocol_profile_id
        != vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID
    {
        return Ok(encoded);
    }
    let project_id = secret.project_id().ok_or_else(|| {
        pre_dispatch_failure(
            "Google Code Assist execution requires a validated credential project id.",
        )
    })?;
    let body = encoded.body.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure("Google Code Assist request body must be a JSON object.")
    })?;
    if body.contains_key("project") {
        return Err(pre_dispatch_failure(
            "Google Code Assist project projection conflicts with a protocol field.",
        ));
    }
    body.insert(
        String::from("project"),
        Value::String(project_id.to_string()),
    );
    Ok(encoded)
}

/// Builds provider authentication and public application headers.
/// 构建供应商鉴权与公共应用请求头。
///
/// # Parameters
/// 参数
/// `route` identifies the exact provider, `protocol_headers` contains non-secret Profile
/// requirements, and `secret` is the selected credential material.
/// `route` 标识精确供应商，`protocol_headers` 包含非秘密 Profile 要求，
/// `secret` 是选中的凭据材料。
///
/// # Returns
/// 返回值
/// A validated merged header map or a controlled pre-dispatch failure.
/// 返回已校验的合并请求头，或受控的分派前失败。
pub(crate) fn execution_headers(
    route: &ProviderExecutionRoute,
    protocol_headers: &std::collections::BTreeMap<String, String>,
    secret: &CredentialExecutionSecret,
) -> Result<HeaderMap, ProviderExecutionFailure> {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    // Streaming requests require byte-transparent SSE framing; compressed error responses
    // are still decoded by the configured reqwest content decoders if an upstream ignores this.
    // 流式请求要求 SSE 分帧保持字节透明；若上游忽略该设置，已配置的 reqwest 内容
    // 解码器仍会解码压缩错误响应。
    headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        HeaderValue::from_static("identity"),
    );
    for (name, value) in protocol_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol profile produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol profile must not provide credential header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol profile produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    for (name, value) in &route.entry.request_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Service entry produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
            || name == HeaderName::from_static("chatgpt-account-id")
            || name == HeaderName::from_static("x-goog-user-project")
            || name == reqwest::header::ACCEPT_ENCODING
            || name == reqwest::header::CONTENT_TYPE
        {
            return Err(pre_dispatch_failure(format!(
                "Service entry must not override reserved header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Service entry produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    for (name, value) in &route.protocol_route.request_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol route produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
            || name == HeaderName::from_static("chatgpt-account-id")
            || name == HeaderName::from_static("x-goog-user-project")
            || name == reqwest::header::ACCEPT_ENCODING
            || name == reqwest::header::CONTENT_TYPE
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol route must not override reserved header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol route produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    let credential_profile = route
        .entry
        .credential_profile(route.connection.credential_profile_id.as_str())
        .ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Credential profile `{}` is not registered for service entry `{}`.",
                route.connection.credential_profile_id, route.entry.id
            ))
        })?;
    match route.protocol_route.resolved_authentication(
        route.entry.authentication,
        credential_profile.authentication,
    ) {
        AuthenticationProjection::XApiKey => {
            headers.insert(
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_str(secret.expose_value()).map_err(|_| {
                    pre_dispatch_failure("API key contains invalid header characters.")
                })?,
            );
        }
        AuthenticationProjection::BearerAuthorization => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Bearer credential contains invalid header characters.",
                        )
                    })?,
            );
        }
        AuthenticationProjection::OpenAiCodexBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Codex bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            let account_id = secret.account_id().ok_or_else(|| {
                pre_dispatch_failure("Codex credential does not contain an account identity.")
            })?;
            headers.insert(
                HeaderName::from_static("chatgpt-account-id"),
                HeaderValue::from_str(account_id).map_err(|_| {
                    pre_dispatch_failure(
                        "Codex account identity contains invalid header characters.",
                    )
                })?,
            );
        }
        AuthenticationProjection::GoogleCodeAssistBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Google bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            if let Some(quota_project_id) = secret.quota_project_id() {
                headers.insert(
                    HeaderName::from_static("x-goog-user-project"),
                    HeaderValue::from_str(quota_project_id).map_err(|_| {
                        pre_dispatch_failure(
                            "Google quota project contains invalid header characters.",
                        )
                    })?,
                );
            }
        }
        AuthenticationProjection::GoogleGeminiBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Google Gemini bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            let quota_project_id = secret.quota_project_id().ok_or_else(|| {
                pre_dispatch_failure(
                    "Public Gemini OAuth credential does not contain a Google Cloud project id.",
                )
            })?;
            headers.insert(
                HeaderName::from_static("x-goog-user-project"),
                HeaderValue::from_str(quota_project_id).map_err(|_| {
                    pre_dispatch_failure(
                        "Google Gemini quota project contains invalid header characters.",
                    )
                })?,
            );
        }
        AuthenticationProjection::GoogleApiKey => {
            headers.insert(
                HeaderName::from_static("x-goog-api-key"),
                HeaderValue::from_str(secret.expose_value()).map_err(|_| {
                    pre_dispatch_failure("Google API key contains invalid header characters.")
                })?,
            );
        }
    }
    if route.provider.id == "kimi" && route.entry.id == "coding-plan" {
        let device_name = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| String::from("unknown"));
        let device_model = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        for (name, value) in [
            ("user-agent", "CLIProxyAPI/dev"),
            ("x-msh-platform", "CLIProxyAPI"),
            ("x-msh-version", "dev"),
            ("x-msh-device-name", device_name.as_str()),
            ("x-msh-device-model", device_model.as_str()),
            (
                "x-msh-device-id",
                secret
                    .provider_device_id()
                    .unwrap_or("cli-proxy-api-device"),
            ),
        ] {
            headers.insert(
                HeaderName::from_static(name),
                HeaderValue::from_str(value).map_err(|_| {
                    pre_dispatch_failure("Kimi execution device identity is invalid.")
                })?,
            );
        }
    }
    Ok(headers)
}

/// One parsed transport frame and its optional SSE event name.
/// 一条解析后的传输帧及其可选 SSE 事件名称。
#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamFrame {
    /// Optional event name.
    /// 可选事件名称。
    event_name: Option<String>,
    /// Complete frame data.
    /// 完整帧数据。
    data: String,
}

/// Incrementally frames arbitrary chunks with bounded retained memory.
/// 使用有界保留内存对任意分片进行增量分帧。
#[derive(Debug)]
struct IncrementalFrameParser {
    /// Registered framing dialect.
    /// 已注册的分帧方言。
    framing: ProtocolStreamFraming,
    /// Maximum complete frame size.
    /// 完整帧最大尺寸。
    max_frame_bytes: usize,
    /// Maximum unconsumed buffer size.
    /// 未消费缓冲区最大尺寸。
    max_buffer_bytes: usize,
    /// Unprocessed bytes retained across transport chunks.
    /// 在传输分片间保留的未处理字节。
    buffer: Vec<u8>,
    /// First unconsumed byte.
    /// 首个未消费字节位置。
    consumed: usize,
}

impl IncrementalFrameParser {
    /// Creates a bounded parser from one immutable transport profile.
    /// 根据一个不可变传输 Profile 创建有界解析器。
    ///
    /// # Parameters
    /// 参数
    /// `profile` provides the exact framing and byte limits.
    /// `profile` 提供精确分帧方式与字节上限。
    ///
    /// # Returns
    /// 返回值
    /// A new empty parser.
    /// 返回一个新的空解析器。
    fn new(profile: ProtocolTransportProfile) -> Self {
        Self {
            framing: profile.framing,
            max_frame_bytes: profile.max_frame_bytes,
            max_buffer_bytes: profile.max_buffer_bytes,
            buffer: Vec::new(),
            consumed: 0,
        }
    }

    /// Pushes arbitrary transport bytes.
    /// 推入任意传输字节。
    ///
    /// # Parameters
    /// 参数
    /// `chunk` is one arbitrary network byte chunk.
    /// `chunk` 是一个任意网络字节分片。
    ///
    /// # Returns
    /// 返回值
    /// Complete frames, or a deterministic framing limit/encoding error.
    /// 返回完整帧，或确定性的分帧上限/编码错误。
    fn push(&mut self, mut chunk: &[u8]) -> Result<Vec<StreamFrame>, ProtocolConversionError> {
        let mut frames = Vec::new();
        while !chunk.is_empty() {
            self.compact_if_worthwhile();
            let retained = self.retained_len();
            if retained >= self.max_buffer_bytes {
                return Err(self.buffer_limit_error());
            }
            let capacity = self.max_buffer_bytes - retained;
            let take = capacity.min(chunk.len());
            self.buffer.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
            frames.extend(self.drain_frames(false)?);
            if self.retained_len() >= self.max_buffer_bytes && !chunk.is_empty() {
                return Err(self.buffer_limit_error());
            }
        }
        Ok(frames)
    }

    /// Finishes framing and includes a final non-empty record.
    /// 完成分帧并包含最后一条非空记录。
    ///
    /// # Returns
    /// 返回值
    /// Final complete frames, or a deterministic truncation/encoding error.
    /// 返回最终完整帧，或确定性的截断/编码错误。
    fn finish(&mut self) -> Result<Vec<StreamFrame>, ProtocolConversionError> {
        self.drain_frames(true)
    }

    /// Drains complete records from the internal byte buffer.
    /// 从内部字节缓冲区提取完整记录。
    ///
    /// # Parameters
    /// 参数
    /// `include_tail` controls whether the final unterminated record is accepted.
    /// `include_tail` 控制是否接受最后一条未带分隔符的记录。
    ///
    /// # Returns
    /// 返回值
    /// Complete frames in transport order.
    /// 返回按传输顺序排列的完整帧。
    fn drain_frames(
        &mut self,
        include_tail: bool,
    ) -> Result<Vec<StreamFrame>, ProtocolConversionError> {
        let mut frames = Vec::new();
        while let Some((record_end, delimiter_len)) = find_record_end(self.framing, self.retained())
        {
            self.validate_frame_size(record_end)?;
            let start = self.consumed;
            let end = start + record_end;
            let record = &self.buffer[start..end];
            if let Some(frame) = parse_stream_record(self.framing, record)? {
                frames.push(frame);
            }
            self.consumed = end + delimiter_len;
        }
        if include_tail && self.retained_len() > 0 {
            let record_len = self.retained_len();
            self.validate_frame_size(record_len)?;
            let record = self.retained();
            if let Some(frame) = parse_stream_record(self.framing, record)? {
                frames.push(frame);
            }
            self.consumed = self.buffer.len();
        }
        self.compact_if_worthwhile();
        Ok(frames)
    }

    /// Returns the unconsumed suffix.
    /// 返回未消费的后缀。
    ///
    /// # Returns
    /// 返回值
    /// Borrowed pending bytes.
    /// 返回借用的待处理字节。
    fn retained(&self) -> &[u8] {
        &self.buffer[self.consumed..]
    }

    /// Returns the number of unconsumed bytes.
    /// 返回未消费字节数。
    ///
    /// # Returns
    /// 返回值
    /// Pending byte count.
    /// 返回待处理字节数。
    fn retained_len(&self) -> usize {
        self.buffer.len().saturating_sub(self.consumed)
    }

    /// Compacts only after consumed storage is materially large.
    /// 仅在已消费存储达到显著规模后执行压缩。
    fn compact_if_worthwhile(&mut self) {
        if self.consumed == self.buffer.len() {
            self.buffer.clear();
            self.consumed = 0;
        } else if self.consumed >= 64 * 1024 && self.consumed >= self.buffer.len() / 2 {
            self.buffer.copy_within(self.consumed.., 0);
            self.buffer.truncate(self.buffer.len() - self.consumed);
            self.consumed = 0;
        }
    }

    /// Rejects one oversized complete or pending frame.
    /// 拒绝一个过大的完整帧或待处理帧。
    ///
    /// # Parameters
    /// 参数
    /// `frame_bytes` is the exact candidate frame size.
    /// `frame_bytes` 是候选帧的精确尺寸。
    ///
    /// # Returns
    /// 返回值
    /// Success within the profile limit, otherwise a controlled error.
    /// 在 Profile 上限内返回成功，否则返回受控错误。
    fn validate_frame_size(&self, frame_bytes: usize) -> Result<(), ProtocolConversionError> {
        if frame_bytes <= self.max_frame_bytes {
            return Ok(());
        }
        Err(ProtocolConversionError::new(format!(
            "Provider stream frame exceeded the registered {} byte limit.",
            self.max_frame_bytes
        )))
    }

    /// Builds one controlled pending-buffer limit error.
    /// 构建一个受控的待处理缓冲区上限错误。
    ///
    /// # Returns
    /// 返回值
    /// A deterministic protocol conversion failure.
    /// 返回确定性的协议转换失败。
    fn buffer_limit_error(&self) -> ProtocolConversionError {
        ProtocolConversionError::new(format!(
            "Provider stream buffer exceeded the registered {} byte limit.",
            self.max_buffer_bytes
        ))
    }
}

/// Finds the first complete delimiter for one registered framing dialect.
/// 查找一种已注册分帧方言的首个完整分隔符。
///
/// # Parameters
/// 参数
/// `framing` is the exact dialect and `buffer` is the unconsumed suffix.
/// `framing` 是精确方言，`buffer` 是未消费后缀。
///
/// # Returns
/// 返回值
/// Record-end offset and delimiter length, or `None`.
/// 返回记录结束偏移与分隔符长度，否则返回 `None`。
fn find_record_end(framing: ProtocolStreamFraming, buffer: &[u8]) -> Option<(usize, usize)> {
    match framing {
        ProtocolStreamFraming::Sse => {
            for index in 0..buffer.len() {
                let Some(first_len) = sse_line_ending_len(buffer, index) else {
                    continue;
                };
                let second_start = index + first_len;
                if let Some(second_len) = sse_line_ending_len(buffer, second_start) {
                    return Some((index, first_len + second_len));
                }
            }
            None
        }
        ProtocolStreamFraming::JsonLines | ProtocolStreamFraming::Ndjson => buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| (index, 1)),
        ProtocolStreamFraming::SingleJson => None,
    }
}

/// Returns the byte length of one SSE line ending at an exact offset.
/// 返回精确偏移处一项 SSE 行结束符的字节长度。
///
/// # Parameters
/// 参数
/// `buffer` is the pending transport data and `index` is the candidate line-ending offset.
/// `buffer` 是待处理传输数据，`index` 是候选行结束符偏移。
///
/// # Returns
/// 返回值
/// Two bytes for CRLF, one byte for CR or LF, otherwise `None`.
/// CRLF 返回两个字节，CR 或 LF 返回一个字节，否则返回 `None`。
fn sse_line_ending_len(buffer: &[u8], index: usize) -> Option<usize> {
    match buffer.get(index) {
        Some(b'\r') if buffer.get(index + 1) == Some(&b'\n') => Some(2),
        Some(b'\r' | b'\n') => Some(1),
        _ => None,
    }
}

/// Parses one complete record according to its registered framing.
/// 根据已注册分帧方式解析一条完整记录。
///
/// # Parameters
/// 参数
/// `framing` is the exact dialect and `record` contains one complete frame.
/// `framing` 是精确方言，`record` 包含一个完整帧。
///
/// # Returns
/// 返回值
/// A decoded frame, no frame for heartbeat/blank records, or a controlled error.
/// 返回解码帧；心跳/空记录返回空；编码错误返回受控错误。
fn parse_stream_record(
    framing: ProtocolStreamFraming,
    record: &[u8],
) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    match framing {
        ProtocolStreamFraming::Sse => parse_sse_record(record),
        ProtocolStreamFraming::JsonLines | ProtocolStreamFraming::Ndjson => {
            parse_json_line_record(record)
        }
        ProtocolStreamFraming::SingleJson => parse_single_json_record(record),
    }
}

/// Parses one complete SSE record.
/// 解析一条完整 SSE 记录。
///
/// # Parameters
/// 参数
/// `record` contains one complete SSE record without its blank-line delimiter.
/// `record` 包含一条不带空行分隔符的完整 SSE 记录。
///
/// # Returns
/// 返回值
/// A semantic data frame, no frame for comments, or a controlled UTF-8 error.
/// 返回语义数据帧；注释返回空；UTF-8 错误返回受控错误。
fn parse_sse_record(record: &[u8]) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    let text = std::str::from_utf8(record).map_err(|error| {
        ProtocolConversionError::new(format!("Provider SSE record is not valid UTF-8: {error}"))
    })?;
    let mut event_name = None;
    let mut data_lines = Vec::new();
    // SSE permits CR, LF, and CRLF independently, so normalize all three before field parsing.
    // SSE 独立允许 CR、LF 与 CRLF，因此在字段解析前统一三种换行。
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for line in normalized.lines() {
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event_name = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    if data_lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(StreamFrame {
        event_name,
        data: data_lines.join("\n"),
    }))
}

/// Parses one JSONL or NDJSON record.
/// 解析一条 JSONL 或 NDJSON 记录。
///
/// # Parameters
/// 参数
/// `record` is one line without its LF delimiter.
/// `record` 是不带 LF 分隔符的一行。
///
/// # Returns
/// 返回值
/// A trimmed JSON frame, no frame for a blank line, or a controlled UTF-8 error.
/// 返回去除首尾空白的 JSON 帧；空行返回空；UTF-8 错误返回受控错误。
fn parse_json_line_record(record: &[u8]) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    let text = std::str::from_utf8(record).map_err(|error| {
        ProtocolConversionError::new(format!("Provider JSON line is not valid UTF-8: {error}"))
    })?;
    let data = text.trim_end_matches('\r').trim();
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(data).map_err(|error| {
        ProtocolConversionError::new(format!("Provider JSON line is invalid: {error}"))
    })?;
    Ok(Some(StreamFrame {
        event_name: None,
        data: data.to_string(),
    }))
}

/// Parses one complete single-JSON response.
/// 解析一份完整的单 JSON 响应。
///
/// # Parameters
/// 参数
/// `record` is the complete response body.
/// `record` 是完整响应体。
///
/// # Returns
/// 返回值
/// A validated JSON frame, no frame for an empty body, or a controlled error.
/// 返回已校验 JSON 帧；空响应体返回空；错误返回受控结果。
fn parse_single_json_record(record: &[u8]) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    let text = std::str::from_utf8(record).map_err(|error| {
        ProtocolConversionError::new(format!(
            "Provider JSON response is not valid UTF-8: {error}"
        ))
    })?;
    let data = text.trim();
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(data).map_err(|error| {
        ProtocolConversionError::new(format!("Provider JSON response is invalid: {error}"))
    })?;
    Ok(Some(StreamFrame {
        event_name: None,
        data: data.to_string(),
    }))
}

/// Ensures one endpoint path begins with a slash.
/// 确保一个端点路径以斜杠开头。
fn ensure_leading_slash(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// Creates a structured pre-dispatch failure.
/// 创建结构化分派前失败。
pub(crate) fn pre_dispatch_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("route_validation")),
            stage: String::from("prepare"),
            http_status: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Converts a deterministic protocol failure.
/// 转换确定性协议失败。
fn protocol_failure(error: ProtocolConversionError) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("protocol"),
            http_status: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: error.message,
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Converts one protocol failure observed after the upstream request was dispatched.
/// 转换一项在上游请求已经分派后观察到的协议失败。
///
/// # Parameters
/// 参数
/// `error` is the decoder or lifecycle failure and `semantic_output_observed` records whether
/// user-visible or tool-visible semantic output was already emitted.
/// `error` 是解码器或生命周期失败，`semantic_output_observed` 记录是否已经发出
/// 用户可见或工具可见的语义输出。
///
/// # Returns
/// 返回值
/// A terminal, non-retryable protocol failure with exact dispatch state.
/// 返回携带精确分派状态的终态、不可重试协议失败。
fn stream_protocol_failure(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("response_stream"),
            http_status: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: error.message,
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Creates a structured transport failure.
/// 创建结构化传输失败。
pub(crate) fn transport_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport")),
            stage: String::from("transport"),
            http_status: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Unknown,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: true,
            retry_after: None,
        }),
    }
}

/// Creates one transport failure after response dispatch while preserving semantic-output safety.
/// 创建一项响应分派后的传输失败，并保留语义输出安全状态。
///
/// # Parameters
/// 参数
/// `message` is the controlled transport diagnostic and `semantic_output_observed` records
/// whether transparent replay would duplicate visible semantics.
/// `message` 是受控传输诊断，`semantic_output_observed` 记录透明重放是否会重复可见语义。
///
/// # Returns
/// 返回值
/// A structured transport failure retryable only before semantic output.
/// 返回仅在语义输出前可重试的结构化传输失败。
fn stream_transport_failure(
    message: impl Into<String>,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport")),
            stage: String::from("response_stream"),
            http_status: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed,
            side_effect_observed: false,
            terminal: true,
            retryable: !semantic_output_observed,
            retry_after: None,
        }),
    }
}

/// Validates a successful response content type against the immutable protocol profile.
/// 根据不可变协议 Profile 校验成功响应的内容类型。
///
/// # Parameters
/// 参数
/// `headers` are the exact upstream headers, `profile` is the registered transport declaration,
/// and `url` is used only in the controlled diagnostic.
/// `headers` 是精确上游响应头，`profile` 是已注册传输声明，`url` 仅用于受控诊断。
///
/// # Returns
/// 返回值
/// Success for an exact accepted media type, otherwise a dispatched protocol failure.
/// 精确匹配允许媒体类型时返回成功，否则返回已分派协议失败。
fn validate_response_content_type(
    headers: &HeaderMap,
    profile: &ProtocolTransportProfile,
    url: &str,
) -> Result<(), ProviderExecutionFailure> {
    let value = headers
        .get(reqwest::header::CONTENT_TYPE)
        .ok_or_else(|| {
            response_headers_protocol_failure(format!(
                "Provider response from `{url}` omitted the required Content-Type header."
            ))
        })?
        .to_str()
        .map_err(|_| {
            response_headers_protocol_failure(format!(
                "Provider response from `{url}` contained a non-ASCII Content-Type header."
            ))
        })?;
    let media_type = value.split(';').next().map(str::trim).unwrap_or_default();
    if profile
        .accepted_response_content_types
        .iter()
        .any(|accepted| media_type.eq_ignore_ascii_case(accepted))
    {
        return Ok(());
    }
    Err(response_headers_protocol_failure(format!(
        "Provider response from `{url}` used unregistered Content-Type `{media_type}`; expected one of [{}].",
        profile.accepted_response_content_types.join(", ")
    )))
}

/// Creates a dispatched protocol failure for invalid successful response headers.
/// 为无效的成功响应头创建已分派协议失败。
///
/// # Parameters
/// 参数
/// `message` is the controlled diagnostic.
/// `message` 是受控诊断。
///
/// # Returns
/// 返回值
/// A terminal non-retryable protocol failure.
/// 返回终结且不可重试的协议失败。
pub(crate) fn response_headers_protocol_failure(
    message: impl Into<String>,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("response_content_type")),
            stage: String::from("response_headers"),
            http_status: Some(200),
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Reads a non-success response body without exceeding the registered memory limit.
/// 在不超过已注册内存上限的情况下读取非成功响应体。
///
/// # Parameters
/// 参数
/// `response` is the exact response, `max_bytes` is the profile limit, `url` is diagnostic
/// context, and `status` plus `retry_after` preserve typed retry semantics on overflow.
/// `response` 是精确响应，`max_bytes` 是 Profile 上限，`url` 是诊断上下文，
/// `status` 与 `retry_after` 在溢出时保留强类型重试语义。
///
/// # Returns
/// 返回值
/// The complete UTF-8-lossy body within the limit, or a structured failure.
/// 返回上限内完整的 UTF-8 宽容解码正文，或结构化失败。
pub(crate) async fn read_bounded_response_text(
    response: reqwest::Response,
    max_bytes: usize,
    url: &str,
    status: u16,
    retry_after: Option<RetryAfterHint>,
) -> Result<String, ProviderExecutionFailure> {
    let mut bytes = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.map_err(|error| {
            stream_transport_failure(
                format!("Provider error response from `{url}` could not be read: {error}"),
                false,
            )
        })?;
        if body.len().saturating_add(chunk.len()) > max_bytes {
            let mut failure = http_failure(status, retry_after, "");
            failure.error.subcategory = Some(String::from("error_body_too_large"));
            failure.error.stage = String::from("error_response_body");
            failure.error.message = format!(
                "Provider error response from `{url}` exceeded the registered {max_bytes} byte limit."
            );
            return Err(failure);
        }
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(String::from_utf8_lossy(body.as_slice()).into_owned())
}

/// Parses the standard Retry-After header without discarding fractional precision or provenance.
/// 解析标准 Retry-After 响应头，且不丢弃小数精度或来源。
///
/// # Parameters
/// 参数
/// `headers` is the exact upstream response header map and `now` is the comparison instant for
/// HTTP-date values.
/// `headers` 是精确上游响应头集合，`now` 是 HTTP 日期值使用的比较时刻。
///
/// # Returns
/// 返回值
/// A normalized retry hint for valid seconds, fractional seconds, or HTTP dates.
/// 对合法秒数、小数秒或 HTTP 日期返回归一化重试提示。
pub(crate) fn parse_retry_after_headers(
    headers: &HeaderMap,
    now: SystemTime,
) -> Option<RetryAfterHint> {
    let value = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Some(delay_milliseconds) = parse_decimal_seconds_milliseconds(value) {
        return Some(RetryAfterHint {
            delay_milliseconds,
            source: RetryAfterSource::HeaderSeconds,
        });
    }
    let retry_at = httpdate::parse_http_date(value).ok()?;
    let delay_milliseconds = retry_at
        .duration_since(now)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    Some(RetryAfterHint {
        delay_milliseconds,
        source: RetryAfterSource::HeaderHttpDate,
    })
}

/// Converts a non-negative decimal seconds string into milliseconds without floating-point rounding.
/// 将非负十进制秒数字符串转换为毫秒，且不进行浮点舍入。
///
/// # Parameters
/// 参数
/// `value` is the trimmed Retry-After header value.
/// `value` 是去除首尾空白后的 Retry-After 响应头值。
///
/// # Returns
/// 返回值
/// Exact milliseconds rounded up to the next millisecond for sub-millisecond fractions.
/// 对亚毫秒小数向上取整后的精确毫秒数。
fn parse_decimal_seconds_milliseconds(value: &str) -> Option<u64> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole_milliseconds = whole.parse::<u64>().ok()?.checked_mul(1_000)?;
    if fraction.is_empty() {
        return Some(whole_milliseconds);
    }
    let mut digits = fraction.bytes();
    let hundreds = u64::from(digits.next().unwrap_or(b'0') - b'0');
    let tens = u64::from(digits.next().unwrap_or(b'0') - b'0');
    let ones = u64::from(digits.next().unwrap_or(b'0') - b'0');
    let rounded_fraction =
        hundreds * 100 + tens * 10 + ones + u64::from(digits.any(|digit| digit != b'0'));
    whole_milliseconds.checked_add(rounded_fraction)
}

/// Converts one non-success HTTP response.
/// 转换一个非成功 HTTP 响应。
pub(crate) fn http_failure(
    status: u16,
    retry_after: Option<RetryAfterHint>,
    body: &str,
) -> ProviderExecutionFailure {
    let parsed = serde_json::from_str::<Value>(body).ok();
    let error = parsed
        .as_ref()
        .map(|value| value.get("error").unwrap_or(value));
    let upstream_code = error.and_then(|value| {
        value
            .get("code")
            .and_then(|code| match code {
                Value::String(code) => Some(code.clone()),
                Value::Number(code) => Some(code.to_string()),
                _ => None,
            })
            .or_else(|| {
                value
                    .get("error_type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .or_else(|| {
                value
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
    });
    let error_type = error
        .and_then(|value| value.get("error_type").and_then(Value::as_str))
        .or_else(|| error.and_then(|value| value.get("type").and_then(Value::as_str)));
    let category = match error_type {
        Some("authentication" | "authentication_error") => AiErrorCategory::Authentication,
        Some("permission_denied" | "permission_error") => AiErrorCategory::PermissionDenied,
        Some("payment_required") => AiErrorCategory::PaymentRequired,
        Some("rate_limit_exceeded" | "rate_limit_error") => AiErrorCategory::RateLimited,
        Some("provider_overloaded" | "overloaded_error") => AiErrorCategory::ProviderOverloaded,
        Some("provider_unavailable" | "server" | "api_error") => {
            AiErrorCategory::ProviderUnavailable
        }
        Some("timeout") => AiErrorCategory::Timeout,
        Some("content_policy_violation") => AiErrorCategory::ContentPolicy,
        Some(
            "invalid_request"
            | "invalid_request_error"
            | "context_length_exceeded"
            | "max_tokens_exceeded"
            | "token_limit_exceeded"
            | "string_too_long",
        ) => AiErrorCategory::InvalidRequest,
        _ => match status {
            401 => AiErrorCategory::Authentication,
            403 => AiErrorCategory::PermissionDenied,
            402 => AiErrorCategory::PaymentRequired,
            408 => AiErrorCategory::Timeout,
            429 => AiErrorCategory::RateLimited,
            500..=599 => AiErrorCategory::ProviderUnavailable,
            _ => AiErrorCategory::InvalidRequest,
        },
    };
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category,
            subcategory: error_type
                .map(str::to_string)
                .or_else(|| Some(String::from("http"))),
            stage: String::from("response_headers"),
            http_status: Some(status),
            upstream_code,
            upstream_type: error_type.map(str::to_string),
            upstream_parameter: error
                .and_then(|value| value.get("param").and_then(Value::as_str))
                .map(str::to_string),
            upstream_message: error
                .and_then(|value| value.get("message").and_then(Value::as_str))
                .map(str::to_string),
            request_id: parsed.as_ref().and_then(|value| {
                value
                    .get("request_id")
                    .and_then(Value::as_str)
                    .or_else(|| value.pointer("/error/request_id").and_then(Value::as_str))
                    .map(str::to_string)
            }),
            provider_id: None,
            entry_id: None,
            model_id: parsed.as_ref().and_then(|value| {
                value
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
            message: error
                .and_then(|value| value.get("message").and_then(Value::as_str))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if body.is_empty() {
                        format!("Provider returned HTTP {status} with an empty error response.")
                    } else {
                        body.chars().take(2_000).collect()
                    }
                }),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: matches!(
                category,
                AiErrorCategory::RateLimited
                    | AiErrorCategory::ProviderOverloaded
                    | AiErrorCategory::ProviderUnavailable
                    | AiErrorCategory::Timeout
            ),
            retry_after,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CredentialMode,
        providers::{
            alibaba_descriptor, deepseek_descriptor, kimi_descriptor, minimax_descriptor,
            openrouter_descriptor,
        },
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::Duration;
    use vulcan_ai_contract::{
        AiMessage, AiOperation, ContentItem, MessageRole, StructuredOutputPolicy,
    };
    use vulcan_ai_protocols::{
        ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
    };

    /// Verifies an empty upstream error body retains the exact HTTP status.
    /// 验证空上游错误正文仍会保留精确 HTTP 状态。
    #[test]
    fn empty_http_error_response_reports_status() {
        let failure = http_failure(404, None, "");

        assert_eq!(failure.error.http_status, Some(404));
        assert_eq!(
            failure.error.message,
            "Provider returned HTTP 404 with an empty error response."
        );
    }

    /// Builds one minimal provider-neutral conversation request.
    /// 构建一个最小供应商无关对话请求。
    fn minimal_request() -> AiExecutionRequest {
        AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("openai/gpt-4.1"),
            cache_scope_id: None,
            continuation: None,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("hello"),
                }],
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: Some(512),
        }
    }

    /// Resolves one pinned OpenRouter route for an exact verified protocol.
    /// 为一个精确的已验证协议解析一条固定二级节点的 OpenRouter 路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` is one exact protocol profile registered by OpenRouter.
    /// `protocol_id` 是 OpenRouter 注册的一个精确协议 Profile。
    ///
    /// # Returns
    /// 返回值
    /// A frozen route with fallback disabled.
    /// 返回禁用回退的冻结路由。
    fn pinned_openrouter_route(protocol_id: &str) -> ProviderExecutionRoute {
        let provider = openrouter_descriptor();
        let entry = provider
            .service_entries
            .first()
            .expect("OpenRouter public entry must exist")
            .clone();
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Primary"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: None,
            verified_protocols: BTreeSet::from([protocol_id.to_string()]),
            selected_protocol: protocol_id.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        ProviderExecutionRoute::resolve(
            provider,
            entry,
            connection,
            SecondaryRouteSelection::PinnedEndpoint {
                route_id: String::from("anthropic"),
                provider_display_name: String::from("Anthropic"),
                endpoint_display_name: String::from("Anthropic"),
            },
        )
        .expect("route should resolve")
    }

    /// Resolves one MiniMax route for an exact compatible protocol.
    /// 为一个精确兼容协议解析 MiniMax 路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` selects the credential-bound Anthropic or OpenAI route.
    /// `protocol_id` 选择凭据绑定的 Anthropic 或 OpenAI 路由。
    ///
    /// # Returns
    /// 返回值
    /// A frozen MiniMax international API route.
    /// 返回冻结的 MiniMax 国际 API 路由。
    fn minimax_route(protocol_id: &str) -> ProviderExecutionRoute {
        let provider = minimax_descriptor();
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == "global")
            .expect("MiniMax global API entry must exist")
            .clone();
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: String::from("payg-api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Primary"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: None,
            verified_protocols: BTreeSet::from([protocol_id.to_string()]),
            selected_protocol: protocol_id.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        ProviderExecutionRoute::resolve(
            provider,
            entry,
            connection,
            SecondaryRouteSelection::Automatic,
        )
        .expect("MiniMax route should resolve")
    }

    /// Resolves one DeepSeek official API route for an exact compatible protocol.
    /// 为一个精确兼容协议解析 DeepSeek 官方 API 路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` selects the credential-bound Anthropic or OpenAI route.
    /// `protocol_id` 选择凭据绑定的 Anthropic 或 OpenAI 路由。
    ///
    /// # Returns
    /// 返回值
    /// A frozen DeepSeek official API route.
    /// 返回冻结的 DeepSeek 官方 API 路由。
    fn deepseek_route(protocol_id: &str) -> ProviderExecutionRoute {
        let provider = deepseek_descriptor();
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == "official-api")
            .expect("DeepSeek official API entry must exist")
            .clone();
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Primary"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: None,
            verified_protocols: BTreeSet::from([protocol_id.to_string()]),
            selected_protocol: protocol_id.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        ProviderExecutionRoute::resolve(
            provider,
            entry,
            connection,
            SecondaryRouteSelection::Automatic,
        )
        .expect("DeepSeek route should resolve")
    }

    /// Resolves one exact Kimi Coding Plan route.
    /// 解析一条精确 Kimi Coding Plan 路由。
    fn kimi_coding_route() -> ProviderExecutionRoute {
        let provider = kimi_descriptor();
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == "coding-plan")
            .expect("Kimi Coding Plan entry")
            .clone();
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: String::from("coding-plan-login"),
            credential_mode: CredentialMode::Auth,
            credential_id: String::from("credential"),
            display_name: String::from("Kimi"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: None,
            verified_protocols: BTreeSet::from([String::from(OPENAI_CHAT_PROFILE_ID)]),
            selected_protocol: String::from(OPENAI_CHAT_PROFILE_ID),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        ProviderExecutionRoute::resolve(
            provider,
            entry,
            connection,
            SecondaryRouteSelection::Automatic,
        )
        .expect("Kimi route should resolve")
    }

    /// Resolves one Alibaba Model Studio route with a credential-scoped workspace origin.
    /// 使用凭据作用域工作空间源地址解析一条阿里云百炼路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` selects the exact OpenAI or Anthropic compatibility path.
    /// `protocol_id` 选择精确 OpenAI 或 Anthropic 兼容路径。
    ///
    /// # Returns
    /// 返回值
    /// A frozen Singapore route whose public origin is replaced by the credential setting.
    /// 返回公共源地址已被凭据设置替换的冻结新加坡路由。
    fn alibaba_workspace_route(protocol_id: &str) -> ProviderExecutionRoute {
        let provider = alibaba_descriptor();
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == "model-studio-singapore")
            .expect("Model Studio Singapore entry")
            .clone();
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Workspace"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: Some(String::from(
                "https://ws-example.ap-southeast-1.maas.aliyuncs.com",
            )),
            verified_protocols: BTreeSet::from([protocol_id.to_string()]),
            selected_protocol: protocol_id.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        ProviderExecutionRoute::resolve(
            provider,
            entry,
            connection,
            SecondaryRouteSelection::Automatic,
        )
        .expect("Alibaba workspace route should resolve")
    }

    /// Verifies one workspace origin derives the correct protocol-specific request roots.
    /// 验证同一个工作空间源地址派生正确的协议专属请求根地址。
    #[test]
    fn alibaba_workspace_origin_is_protocol_scoped() {
        let chat = alibaba_workspace_route(OPENAI_CHAT_PROFILE_ID);
        assert_eq!(
            chat.request_url("/chat/completions"),
            "https://ws-example.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1/chat/completions"
        );
        let anthropic = alibaba_workspace_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        assert_eq!(
            anthropic.request_url("/v1/messages"),
            "https://ws-example.ap-southeast-1.maas.aliyuncs.com/apps/anthropic/v1/messages"
        );
    }

    /// Verifies SSE framing survives arbitrary UTF-8 and record chunk boundaries.
    /// 验证 SSE 分帧可以跨越任意 UTF-8 与记录分片边界。
    #[test]
    fn sse_parser_handles_arbitrary_chunks() {
        let bytes = "event: delta\r\ndata: {\"text\":\"你好\"}\r\n\r\n".as_bytes();
        let split = bytes
            .windows(3)
            .position(|window| window == "你".as_bytes())
            .expect("UTF-8 text must exist")
            + 1;
        let mut parser = IncrementalFrameParser::new(ProtocolTransportProfile::bounded_sse());
        assert!(
            parser
                .push(&bytes[..split])
                .expect("first chunk should buffer")
                .is_empty()
        );
        let frames = parser
            .push(&bytes[split..])
            .expect("second chunk should complete");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event_name.as_deref(), Some("delta"));
        assert!(frames[0].data.contains("你好"));
    }

    /// Verifies SSE framing accepts CR-only and mixed line endings permitted by the event-stream grammar.
    /// 验证 SSE 分帧接受事件流语法允许的单 CR 与混合行结束符。
    #[test]
    fn sse_parser_accepts_cr_only_and_mixed_line_endings() {
        for bytes in [
            b"event: delta\rdata: {\"one\":1}\r\r".as_slice(),
            b"event: delta\r\ndata: {\"two\":2}\r\n\n".as_slice(),
            b"event: delta\ndata: {\"three\":3}\n\r\n".as_slice(),
        ] {
            let mut parser = IncrementalFrameParser::new(ProtocolTransportProfile::bounded_sse());
            let split = bytes.len() - 1;
            let mut frames = parser
                .push(&bytes[..split])
                .expect("first mixed-ending chunk should parse or buffer");
            frames.extend(
                parser
                    .push(&bytes[split..])
                    .expect("second mixed-ending chunk should parse"),
            );
            frames.extend(parser.finish().expect("mixed-ending stream should finish"));
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].event_name.as_deref(), Some("delta"));
            assert!(frames[0].data.starts_with('{'));
        }
    }

    /// Verifies every registered non-SSE framing handles arbitrary chunk boundaries.
    /// 验证每种已注册非 SSE 分帧方式都能处理任意分片边界。
    #[test]
    fn bounded_parser_supports_json_lines_ndjson_and_single_json() {
        for framing in [
            ProtocolStreamFraming::JsonLines,
            ProtocolStreamFraming::Ndjson,
        ] {
            let mut profile = ProtocolTransportProfile::bounded_sse();
            profile.framing = framing;
            let mut parser = IncrementalFrameParser::new(profile);
            let mut frames = parser
                .push(b"{\"one\":1}\n\n{\"two\"")
                .expect("first lines should parse");
            frames.extend(parser.push(b":2}\r\n").expect("split line should complete"));
            frames.extend(parser.finish().expect("line parser should finish"));
            assert_eq!(
                frames
                    .iter()
                    .map(|frame| frame.data.as_str())
                    .collect::<Vec<_>>(),
                vec![r#"{"one":1}"#, r#"{"two":2}"#]
            );
        }

        let mut profile = ProtocolTransportProfile::bounded_sse();
        profile.framing = ProtocolStreamFraming::SingleJson;
        let mut parser = IncrementalFrameParser::new(profile);
        assert!(
            parser
                .push(b"{\"complete\"")
                .expect("single JSON should buffer")
                .is_empty()
        );
        assert!(
            parser
                .push(b":true}")
                .expect("single JSON should remain buffered")
                .is_empty()
        );
        assert_eq!(
            parser.finish().expect("single JSON should finish")[0].data,
            r#"{"complete":true}"#
        );
    }

    /// Verifies frame and pending-buffer limits fail deterministically.
    /// 验证帧与待处理缓冲区上限会确定性失败。
    #[test]
    fn bounded_parser_rejects_oversized_frames_and_buffers() {
        let mut frame_profile = ProtocolTransportProfile::bounded_sse();
        frame_profile.max_frame_bytes = 4;
        frame_profile.max_buffer_bytes = 32;
        let mut frame_parser = IncrementalFrameParser::new(frame_profile);
        let frame_error = frame_parser
            .push(b"data: {}\n\n")
            .expect_err("oversized frame should fail");
        assert!(frame_error.message.contains("4 byte limit"));

        let mut buffer_profile = ProtocolTransportProfile::bounded_sse();
        buffer_profile.max_frame_bytes = 32;
        buffer_profile.max_buffer_bytes = 8;
        let mut buffer_parser = IncrementalFrameParser::new(buffer_profile);
        let buffer_error = buffer_parser
            .push(b"123456789")
            .expect_err("unterminated buffer should fail");
        assert!(buffer_error.message.contains("8 byte limit"));
    }

    /// Verifies successful responses require one exact registered media type.
    /// 验证成功响应必须使用一个精确注册的媒体类型。
    #[test]
    fn response_content_type_is_strict_and_parameter_tolerant() {
        let profile = ProtocolTransportProfile::bounded_sse();
        let mut valid = HeaderMap::new();
        valid.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        validate_response_content_type(&valid, &profile, "https://example.test")
            .expect("registered media type should pass");

        let mut invalid = HeaderMap::new();
        invalid.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let failure = validate_response_content_type(&invalid, &profile, "https://example.test")
            .expect_err("JSON must not enter the SSE decoder");
        assert_eq!(
            failure.error.subcategory.as_deref(),
            Some("response_content_type")
        );
        assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);
    }

    /// Verifies numeric Retry-After values retain fractional-second precision.
    /// 验证数值型 Retry-After 保留小数秒精度。
    #[test]
    fn retry_after_numeric_values_preserve_milliseconds() {
        assert_eq!(parse_decimal_seconds_milliseconds("5"), Some(5_000));
        assert_eq!(parse_decimal_seconds_milliseconds("2.5"), Some(2_500));
        assert_eq!(parse_decimal_seconds_milliseconds("0.0001"), Some(1));
        assert_eq!(parse_decimal_seconds_milliseconds("1.0019"), Some(1_002));
        assert_eq!(parse_decimal_seconds_milliseconds("-1"), None);
        assert_eq!(parse_decimal_seconds_milliseconds("1.2.3"), None);
    }

    /// Verifies Retry-After HTTP dates preserve their source and exact relative delay.
    /// 验证 Retry-After HTTP 日期保留来源与精确相对延迟。
    #[test]
    fn retry_after_http_date_preserves_source() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let retry_at = now + Duration::from_secs(17);
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_str(httpdate::fmt_http_date(retry_at).as_str())
                .expect("valid HTTP date"),
        );

        let hint = parse_retry_after_headers(&headers, now).expect("valid retry hint");
        assert_eq!(hint.delay_milliseconds, 17_000);
        assert_eq!(hint.source, RetryAfterSource::HeaderHttpDate);
    }

    /// Verifies malformed Retry-After values are rejected instead of guessed.
    /// 验证畸形 Retry-After 值会被拒绝而不是被猜测。
    #[test]
    fn retry_after_malformed_value_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_static("later"),
        );
        assert!(parse_retry_after_headers(&headers, SystemTime::now()).is_none());
    }

    /// Verifies capture encoding includes provider-specific secondary routing.
    /// 验证捕获编码包含供应商特有的二级路由字段。
    #[test]
    fn encode_request_projects_pinned_openrouter_route_for_every_protocol() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        for (protocol_id, expected_path) in [
            (OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
            (OPENAI_RESPONSES_PROFILE_ID, "/responses"),
            (ANTHROPIC_MESSAGES_PROFILE_ID, "/messages"),
        ] {
            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &minimal_request())
                .expect("request should encode");

            assert_eq!(encoded.path, expected_path);
            assert_eq!(encoded.body["model"], "openai/gpt-4.1");
            assert_eq!(encoded.body["provider"]["order"], json!(["anthropic"]));
            assert_eq!(encoded.body["provider"]["allow_fallbacks"], false);
        }
    }

    /// Verifies Chat, Responses, and Anthropic routes retain one exact structured-output schema.
    /// 验证 Chat、Responses 与 Anthropic 路由都会保留同一份精确结构化输出 Schema。
    #[test]
    fn encode_request_preserves_structured_output_schema_for_supported_protocols() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let schema = json!({
            "type": "object",
            "properties": { "accepted": { "type": "boolean" } },
            "required": ["accepted"],
            "additionalProperties": false,
        });

        for protocol_id in [
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ] {
            let mut request = minimal_request();
            request.structured_output = StructuredOutputPolicy::JsonSchema {
                name: String::from("result"),
                description: Some(String::from("Structured result")),
                schema: schema.clone(),
                strict: true,
            };
            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &request)
                .expect("supported protocol should preserve structured output schema");
            let projected_schema = match protocol_id {
                OPENAI_CHAT_PROFILE_ID => {
                    encoded.body.pointer("/response_format/json_schema/schema")
                }
                OPENAI_RESPONSES_PROFILE_ID => encoded.body.pointer("/text/format/schema"),
                ANTHROPIC_MESSAGES_PROFILE_ID => {
                    encoded.body.pointer("/output_config/format/schema")
                }
                _ => unreachable!("the loop only contains supported protocol profiles"),
            };
            assert_eq!(projected_schema, Some(&schema));
        }
    }

    /// Verifies a custom projector cannot silently remove the required Anthropic schema.
    /// 验证自定义投影器不能静默删除 Anthropic 所需的 Schema。
    #[test]
    fn encode_request_rejects_structured_schema_loss_after_custom_projection() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        route.custom_model_projection = Some(CustomModelRequestProjection {
            delete: vec![String::from("output_config")],
            ..Default::default()
        });
        let mut request = minimal_request();
        request.structured_output = StructuredOutputPolicy::JsonSchema {
            name: String::from("result"),
            description: None,
            schema: json!({
                "type": "object",
                "properties": { "accepted": { "type": "boolean" } },
                "required": ["accepted"],
                "additionalProperties": false,
            }),
            strict: true,
        };

        let error = client
            .encode_request(&route, &request)
            .expect_err("schema loss must fail before dispatch");
        assert!(error.error.message.contains("dropped or changed"));
    }

    /// Verifies Kimi Coding projection uses thinking.type and the OAuth-bound device identity.
    /// 验证 Kimi Coding 投影使用 thinking.type 与 OAuth 绑定设备身份。
    #[test]
    fn kimi_coding_projection_uses_thinking_and_device_identity() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("kimi-for-coding");
        request.reasoning_effort = Some(ReasoningEffort::High);
        let route = kimi_coding_route();
        let encoded = client
            .encode_request(&route, &request)
            .expect("Kimi request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "enabled");
        assert!(encoded.body.get("reasoning_effort").is_none());
        let secret = crate::CredentialSecretEnvelope::new(
            crate::CredentialSecretMaterial::KimiCodingPlanOAuth {
                tokens: crate::OAuthTokenSet {
                    access_token: String::from("access"),
                    refresh_token: String::from("refresh"),
                    issued_at_unix_ms: 1,
                    expires_at_unix_ms: 2,
                    scopes: BTreeSet::new(),
                    account_subject: None,
                    email: None,
                },
                device_id: String::from("123e4567-e89b-42d3-a456-426614174000"),
            },
        )
        .expect("Kimi material")
        .execution_secret()
        .expect("Kimi execution secret");
        let headers = execution_headers(&route, &encoded.headers, &secret).expect("Kimi headers");
        assert_eq!(
            headers
                .get("x-msh-device-id")
                .and_then(|value| value.to_str().ok()),
            Some("123e4567-e89b-42d3-a456-426614174000")
        );
        assert_eq!(
            headers
                .get("x-msh-platform")
                .and_then(|value| value.to_str().ok()),
            Some("CLIProxyAPI")
        );
    }

    /// Verifies MiniMax Chat projects adaptive thinking separately from reasoning output splitting.
    /// 验证 MiniMax Chat 将自适应思考与思考输出拆分分别投影。
    #[test]
    fn minimax_openai_chat_projects_adaptive_thinking() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");
        request.reasoning_effort = Some(ReasoningEffort::Adaptive);
        request.model_parameters.insert(
            MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
            ModelParameterValue::Boolean(true),
        );

        let encoded = client
            .encode_request(&minimax_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect("MiniMax Chat request should encode");

        assert_eq!(encoded.body["reasoning_split"], true);
        assert_eq!(encoded.body["stream_options"]["include_usage"], true);
        assert_eq!(encoded.body["thinking"]["type"], "adaptive");
        assert!(encoded.body.get("reasoning_effort").is_none());
    }

    /// Verifies MiniMax Anthropic projects the documented disabled thinking shape.
    /// 验证 MiniMax Anthropic 投影文档化的关闭思考结构。
    #[test]
    fn minimax_anthropic_projects_disabled_thinking() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");
        request.reasoning_effort = Some(ReasoningEffort::None);

        let encoded = client
            .encode_request(&minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
            .expect("MiniMax Anthropic request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert!(encoded.body.get("output_config").is_none());
    }

    /// Verifies MiniMax thinking projection preserves Anthropic native JSON Schema output format.
    /// 验证 MiniMax 思考参数投影会保留 Anthropic 原生 JSON Schema 输出格式。
    #[test]
    fn minimax_anthropic_preserves_structured_output_format() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");
        request.reasoning_effort = Some(ReasoningEffort::None);
        request.structured_output = StructuredOutputPolicy::JsonSchema {
            name: String::from("vmm_turn_analysis"),
            description: Some(String::from("VMM turn analysis")),
            schema: json!({
                "type": "object",
                "properties": { "turn_id": { "type": "integer" } },
                "required": ["turn_id"],
                "additionalProperties": false,
            }),
            strict: true,
        };

        let encoded = client
            .encode_request(&minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
            .expect("MiniMax Anthropic structured request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert_eq!(
            encoded.body["output_config"]["format"]["type"],
            "json_schema"
        );
        assert_eq!(
            encoded.body["output_config"]["format"]["schema"]["required"][0],
            "turn_id"
        );
        assert!(encoded.body["output_config"].get("effort").is_none());
    }

    /// Verifies MiniMax rejects undocumented strength controls before dispatch.
    /// 验证 MiniMax 在分派前拒绝未文档化的强度控制。
    #[test]
    fn minimax_rejects_undocumented_reasoning_effort() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");
        request.reasoning_effort = Some(ReasoningEffort::High);

        let failure = client
            .encode_request(&minimax_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect_err("MiniMax high effort is not documented");
        assert!(
            failure
                .error
                .message
                .contains("exposes only `none` and `adaptive`")
        );
    }

    /// Verifies DeepSeek Chat projects the documented disabled thinking shape.
    /// 验证 DeepSeek Chat 投影文档化的关闭思考结构。
    #[test]
    fn deepseek_openai_chat_projects_disabled_thinking() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-v4-pro");
        request.reasoning_effort = Some(ReasoningEffort::None);
        request.model_parameters.insert(
            String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
            ModelParameterValue::Decimal(String::from("0.2")),
        );

        let encoded = client
            .encode_request(&deepseek_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect("DeepSeek Chat request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert_eq!(encoded.body["temperature"], 0.2);
        assert!(encoded.body.get("reasoning_effort").is_none());
    }

    /// Verifies DeepSeek thinking requests omit temperature because the provider ignores it.
    /// 验证 DeepSeek 思考请求省略供应商不会生效的温度参数。
    #[test]
    fn deepseek_thinking_omits_ineffective_temperature() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-v4-flash");
        request.reasoning_effort = Some(ReasoningEffort::High);
        request.model_parameters.insert(
            String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
            ModelParameterValue::Decimal(String::from("0.2")),
        );

        let encoded = client
            .encode_request(&deepseek_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect("DeepSeek thinking request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "enabled");
        assert!(encoded.body.get("temperature").is_none());
    }

    /// Verifies DeepSeek Anthropic projects both disabled and documented effort controls.
    /// 验证 DeepSeek Anthropic 同时投影关闭思考与文档化档位控制。
    #[test]
    fn deepseek_anthropic_projects_disabled_and_enabled_thinking() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let route = deepseek_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        let mut disabled = minimal_request();
        disabled.model_id = String::from("deepseek-v4-pro");
        disabled.reasoning_effort = Some(ReasoningEffort::None);
        disabled.model_parameters.insert(
            String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
            ModelParameterValue::Decimal(String::from("1.3")),
        );
        let disabled = client
            .encode_request(&route, &disabled)
            .expect("DeepSeek Anthropic disabled request should encode");
        assert_eq!(disabled.body["thinking"]["type"], "disabled");
        assert_eq!(disabled.body["temperature"], 1.3);
        assert!(disabled.body.get("output_config").is_none());

        let mut enabled = minimal_request();
        enabled.model_id = String::from("deepseek-v4-pro");
        enabled.reasoning_effort = Some(ReasoningEffort::Max);
        let enabled = client
            .encode_request(&route, &enabled)
            .expect("DeepSeek Anthropic enabled request should encode");
        assert_eq!(enabled.body["thinking"]["type"], "enabled");
        assert_eq!(enabled.body["output_config"]["effort"], "max");
    }

    /// Verifies DeepSeek Responses projects native web search and removes unsupported fields.
    /// 验证 DeepSeek Responses 投影原生网页搜索并移除不支持字段。
    #[test]
    fn deepseek_responses_projects_native_web_search() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-v4-flash");
        request.reasoning_effort = Some(ReasoningEffort::Max);
        request.native_tools = vec![vulcan_ai_contract::NativeToolKind::WebSearch];
        let encoded = client
            .encode_request(&deepseek_route(OPENAI_RESPONSES_PROFILE_ID), &request)
            .expect("DeepSeek Responses request should encode");
        assert_eq!(encoded.path, "/responses");
        assert_eq!(encoded.body["tools"][0]["type"], "web_search");
        assert!(encoded.body.get("store").is_none());
        assert!(encoded.body.get("include").is_none());
        assert_eq!(encoded.body["reasoning"]["effort"], "max");
    }

    /// Verifies every OpenRouter protocol route converges on the unified reasoning object.
    /// 验证每条 OpenRouter 协议路由都会收敛到统一 reasoning 对象。
    #[test]
    fn encode_request_normalizes_openrouter_reasoning_for_every_protocol() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        for protocol_id in [
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ] {
            let mut request = minimal_request();
            request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Max);
            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &request)
                .expect("reasoning request should encode");
            assert_eq!(encoded.body["reasoning"]["effort"], "max");
            assert!(encoded.body.get("reasoning_effort").is_none());
            assert!(encoded.body.get("output_config").is_none());
        }
    }

    /// Verifies OpenRouter provider-adaptive reasoning uses its unified enable switch.
    /// 验证 OpenRouter 供应商自适应推理使用其统一启用开关。
    #[test]
    fn encode_request_projects_openrouter_adaptive_reasoning_as_enabled() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        for protocol_id in [
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ] {
            let mut request = minimal_request();
            request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Adaptive);

            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &request)
                .expect("adaptive reasoning request should encode");
            assert_eq!(encoded.body["reasoning"]["enabled"], true);
            assert!(encoded.body["reasoning"].get("effort").is_none());
        }
    }

    /// Verifies OpenRouter rejects simultaneous effort and token-budget reasoning controls.
    /// 验证 OpenRouter 会拒绝同时设置推理档位和 Token 预算。
    #[test]
    fn encode_request_rejects_conflicting_openrouter_reasoning_controls() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::High);
        request.model_parameters.insert(
            String::from(OPENROUTER_REASONING_MAX_TOKENS_PARAMETER_ID),
            ModelParameterValue::Integer(2048),
        );
        let error = client
            .encode_request(&pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect_err("conflicting reasoning controls should fail");
        assert!(error.error.message.contains("mutually exclusive"));
    }

    /// Verifies protocol-required headers are merged while credential headers remain adapter-owned.
    /// 验证协议必需请求头会被合并，同时凭据请求头仍由适配器独占管理。
    #[test]
    fn execution_headers_merge_protocol_headers_and_reject_credential_overrides() {
        let route = pinned_openrouter_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let encoded = client
            .encode_request(&route, &minimal_request())
            .expect("Anthropic request should encode");
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");
        let headers = execution_headers(&route, &encoded.headers, &secret)
            .expect("non-secret protocol headers should merge");
        assert_eq!(
            headers
                .get("anthropic-version")
                .and_then(|value| value.to_str().ok()),
            Some("2023-06-01")
        );
        assert_eq!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer secret")
        );
        assert_eq!(
            headers
                .get(reqwest::header::ACCEPT_ENCODING)
                .and_then(|value| value.to_str().ok()),
            Some("identity")
        );

        let forbidden =
            BTreeMap::from([(String::from("authorization"), String::from("Bearer other"))]);
        assert!(execution_headers(&route, &forbidden, &secret).is_err());
    }

    /// Verifies one MiniMax credential selects protocol-specific paths and authentication.
    /// 验证同一个 MiniMax 凭据会选择协议专属路径与鉴权方式。
    #[test]
    fn minimax_protocol_selection_controls_wire_route_and_authentication() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");

        let anthropic_route = minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        let anthropic_request = minimal_request();
        let anthropic = client
            .encode_request(&anthropic_route, &anthropic_request)
            .expect("Anthropic request should encode");
        let anthropic_headers = execution_headers(&anthropic_route, &anthropic.headers, &secret)
            .expect("Anthropic headers should project");
        assert_eq!(anthropic.path, "/anthropic/v1/messages");
        assert_eq!(
            anthropic_headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("secret")
        );
        assert!(!anthropic_headers.contains_key(reqwest::header::AUTHORIZATION));

        let openai_route = minimax_route(OPENAI_CHAT_PROFILE_ID);
        let mut openai_request = minimal_request();
        openai_request.model_parameters.insert(
            MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
            ModelParameterValue::Boolean(true),
        );
        let openai = client
            .encode_request(&openai_route, &openai_request)
            .expect("OpenAI request should encode");
        let openai_headers = execution_headers(&openai_route, &openai.headers, &secret)
            .expect("OpenAI headers should project");
        assert_eq!(openai.path, "/v1/chat/completions");
        assert_eq!(openai.body["reasoning_split"], true);
        assert_eq!(
            openai_headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer secret")
        );
        assert!(!openai_headers.contains_key("x-api-key"));
    }

    /// Verifies custom model projections apply typed parameters, reasoning, deletion, and overrides deterministically.
    /// 验证自定义模型投影会确定性地应用强类型参数、推理、删除与强制覆盖。
    #[test]
    fn custom_model_projection_applies_registered_request_mutations() {
        let mut request = minimal_request();
        request.reasoning_effort = Some(ReasoningEffort::High);
        request.model_parameters.insert(
            String::from("vendor.temperature"),
            ModelParameterValue::Decimal(String::from("0.3")),
        );
        let projection = CustomModelRequestProjection {
            reasoning: BTreeMap::from([(
                ReasoningEffort::High,
                crate::CustomRequestMutation {
                    set: BTreeMap::from([(
                        String::from("thinking.mode"),
                        Value::String(String::from("enabled")),
                    )]),
                    delete: Vec::new(),
                },
            )]),
            model_parameter_paths: BTreeMap::from([(
                String::from("vendor.temperature"),
                String::from("sampling.temperature"),
            )]),
            defaults: BTreeMap::from([(String::from("vendor.keep"), Value::Bool(true))]),
            overrides: BTreeMap::from([(
                String::from("vendor.forced"),
                Value::String(String::from("yes")),
            )]),
            delete: vec![String::from("remove_me")],
        };
        let mut encoded = EncodedWireRequest {
            path: String::from("/v1/chat/completions"),
            headers: BTreeMap::new(),
            body: json!({"model": "custom-model", "messages": [], "remove_me": true}),
        };

        project_custom_model_request(&projection, &request, &mut encoded)
            .expect("custom projection should succeed");

        assert_eq!(encoded.body["thinking"]["mode"], "enabled");
        assert_eq!(encoded.body["sampling"]["temperature"], json!(0.3));
        assert_eq!(encoded.body["vendor"]["keep"], true);
        assert_eq!(encoded.body["vendor"]["forced"], "yes");
        assert!(encoded.body.get("remove_me").is_none());
    }
}
