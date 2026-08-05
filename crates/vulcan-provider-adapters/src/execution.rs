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
use futures_util::{SinkExt, Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::header::{HeaderName as WebSocketHeaderName, HeaderValue as WebSocketHeaderValue},
    },
};
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionError, AiExecutionRequest, AiStreamEvent, AiStreamStateMachine,
    DispatchOutcome, InputTokenCount, ModelParameterValue, ParallelToolCallPolicy,
    PromptCachePolicy, ReasoningEffort, ResourceKind, RetryAfterHint, RetryAfterSource,
    StructuredOutputPolicy, canonicalize_codex_responses_replay_items,
    is_replayable_codex_responses_item,
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

/// OpenAI beta header required by the current Codex Responses WebSocket v2 contract.
/// 当前 Codex Responses WebSocket v2 契约要求携带的 OpenAI Beta 请求头。
const OPENAI_RESPONSES_WEBSOCKET_BETA_HEADER_NAME: &str = "openai-beta";

/// OpenAI beta header value used by the current official Codex Rust client.
/// 当前官方 Codex Rust 客户端使用的 OpenAI Beta 请求头值。
const OPENAI_RESPONSES_WEBSOCKET_BETA_HEADER_VALUE: &str = "responses_websockets=2026-02-06";

/// Official wrapped WebSocket error code indicating that a connection has reached its lifetime limit.
/// 官方包装式 WebSocket 错误码，表示连接已经达到生命周期上限。
const OPENAI_RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE: &str = "websocket_connection_limit_reached";

/// Official wrapped WebSocket error code requesting a replay without the stale response cursor.
/// 官方包装式 WebSocket 错误码，要求在不携带过期响应游标的情况下重放。
const OPENAI_RESPONSES_PREVIOUS_RESPONSE_NOT_FOUND_CODE: &str = "previous_response_not_found";

/// Official Codex multiplier from stream idle timeout to the complete unary compact timeout.
/// 官方 Codex 从流空闲超时换算完整一元压缩超时所使用的倍数。
const RESPONSES_COMPACT_TIMEOUT_IDLE_MULTIPLIER: u32 = 4;

/// Opt-in environment switch for secret-safe provider transport timing diagnostics.
/// 用于启用不含密钥的供应商传输计时诊断的可选环境变量。
const PROVIDER_TRANSPORT_TRACE_ENV: &str = "VULCAN_PROVIDER_TRANSPORT_TRACE";

/// Maximum process-local session scopes that can remember a WebSocket-to-HTTP switch.
/// 进程内最多记忆的 WebSocket 到 HTTP 会话切换范围数量。
const MAX_REMEMBERED_RESPONSES_WEBSOCKET_FALLBACK_SCOPES: usize = 2_048;

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

/// Carries one validated non-streaming Responses compaction result.
/// 承载一份已验证的非流式 Responses 远端压缩结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesCompactResult {
    /// Ordered provider-native output items preserved for the next Responses request.
    /// 为下一次 Responses 请求保留的有序供应商原生输出项。
    pub output_items: Vec<Value>,
    /// Optional provider-reported usage object retained without lossy projection.
    /// 可选的供应商用量对象，以无损形式保留。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    /// Optional upstream response identifier.
    /// 可选的上游响应标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
}

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
    /// Bounded session scopes that have switched from Responses WebSocket to HTTP.
    /// 已从 Responses WebSocket 切换到 HTTP 的有界会话范围。
    responses_websocket_http_fallback_scopes: Arc<StdMutex<BTreeMap<String, Instant>>>,
    /// Bounded safe WebSocket failure counters awaiting host-controlled retries.
    /// 等待宿主控制重试的有界安全 WebSocket 失败计数器。
    responses_websocket_safe_failure_scopes:
        Arc<StdMutex<BTreeMap<String, ResponsesWebsocketSafeFailureScope>>>,
}

/// Tracks consecutive replay-safe WebSocket failures for one exact session route.
/// 跟踪一个精确会话路由连续发生的可安全重放 WebSocket 失败。
#[derive(Clone, Copy)]
struct ResponsesWebsocketSafeFailureScope {
    /// Consecutive safe failure count since the last healthy WebSocket response.
    /// 自上一次健康 WebSocket 响应以来的连续安全失败次数。
    failures: u32,
    /// Last observation time used to evict the oldest bounded scope.
    /// 用于淘汰最旧有界范围的最后观察时间。
    observed_at: Instant,
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

/// Builds the bounded session key that owns one Codex Responses transport fallback decision.
/// 构建拥有一次 Codex Responses 传输回退决策的有界会话键。
///
/// # Parameters
/// 参数
/// `route` freezes provider and connection identity; `request` supplies the exact model and stable
/// cache scope that the runtime derives from the task session id.
/// `route` 固定供应商与连接身份；`request` 提供精确模型，以及运行时由任务会话标识派生的稳定缓存范围。
///
/// # Returns
/// 返回值
/// A process-local key only for Codex Responses requests with a stable session scope.
/// 仅为具有稳定会话范围的 Codex Responses 请求返回进程内键。
fn responses_websocket_fallback_scope_key(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> Option<String> {
    if route.protocol_route.protocol_profile_id != OPENAI_CODEX_RESPONSES_PROFILE_ID {
        return None;
    }
    let cache_scope_id = request.cache_scope_id.as_deref()?;
    Some(format!(
        "{}|{}|{}|{}|{}|{}",
        route.provider.id,
        route.connection.id,
        route.protocol_route.protocol_profile_id,
        route.request_url("/responses"),
        request.model_id,
        cache_scope_id
    ))
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
            responses_websocket_http_fallback_scopes: Arc::new(StdMutex::new(BTreeMap::new())),
            responses_websocket_safe_failure_scopes: Arc::new(StdMutex::new(BTreeMap::new())),
        }
    }

    /// Returns whether one exact Codex session already switched to HTTP after a safe WS failure.
    /// 返回一个精确 Codex 会话是否已经在安全 WS 失败后切换到 HTTP。
    pub fn responses_websocket_http_fallback_active(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
    ) -> bool {
        let Some(scope_key) = responses_websocket_fallback_scope_key(route, request) else {
            return false;
        };
        self.responses_websocket_http_fallback_scopes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(scope_key.as_str())
    }

    /// Returns whether the exact request currently selects Codex Responses WebSocket transport.
    /// 返回精确请求当前是否选择 Codex Responses WebSocket 传输。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` identify the immutable provider route and session-scoped request.
    /// `route` 与 `request` 标识不可变供应商路由和会话级请求。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when WebSocket is configured and the session has not switched to HTTP.
    /// 仅当已配置 WebSocket 且会话尚未切换到 HTTP 时返回 `true`。
    pub fn responses_websocket_selected_for_request(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
    ) -> bool {
        route_prefers_codex_responses_websocket(route)
            && !self.responses_websocket_http_fallback_active(route, request)
    }

    /// Remembers one safe WS-to-HTTP switch for the exact Codex session scope.
    /// 为精确 Codex 会话范围记忆一次安全的 WS 到 HTTP 切换。
    fn remember_responses_websocket_http_fallback(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
    ) {
        let Some(scope_key) = responses_websocket_fallback_scope_key(route, request) else {
            return;
        };
        self.responses_websocket_safe_failure_scopes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(scope_key.as_str());
        let mut scopes = self
            .responses_websocket_http_fallback_scopes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !scopes.contains_key(scope_key.as_str())
            && scopes.len() >= MAX_REMEMBERED_RESPONSES_WEBSOCKET_FALLBACK_SCOPES
            && let Some(oldest_scope_key) = scopes
                .iter()
                .min_by_key(|(_, remembered_at)| **remembered_at)
                .map(|(scope_key, _)| scope_key.clone())
        {
            scopes.remove(oldest_scope_key.as_str());
        }
        scopes.insert(scope_key, Instant::now());
    }

    /// Records one replay-safe WebSocket failure and reports whether its retry budget is exhausted.
    /// 记录一次可安全重放的 WebSocket 失败，并报告其重试预算是否已经耗尽。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` identify the exact session route; `retry_budget` is the configured
    /// number of host-controlled retries allowed after the initial attempt.
    /// `route` 与 `request` 标识精确会话路由；`retry_budget` 是初始尝试后允许的宿主控制重试次数。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the next safe action is HTTP fallback, or when no stable session scope exists.
    /// 当下一步安全动作应为 HTTP 回退，或不存在稳定会话范围时返回 `true`。
    fn record_responses_websocket_safe_failure(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        retry_budget: u32,
    ) -> bool {
        let Some(scope_key) = responses_websocket_fallback_scope_key(route, request) else {
            return true;
        };
        let now = Instant::now();
        let mut scopes = self
            .responses_websocket_safe_failure_scopes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !scopes.contains_key(scope_key.as_str())
            && scopes.len() >= MAX_REMEMBERED_RESPONSES_WEBSOCKET_FALLBACK_SCOPES
            && let Some(oldest_scope_key) = scopes
                .iter()
                .min_by_key(|(_, scope)| scope.observed_at)
                .map(|(scope_key, _)| scope_key.clone())
        {
            scopes.remove(oldest_scope_key.as_str());
        }
        let scope = scopes
            .entry(scope_key)
            .or_insert(ResponsesWebsocketSafeFailureScope {
                failures: 0,
                observed_at: now,
            });
        scope.failures = scope.failures.saturating_add(1);
        scope.observed_at = now;
        scope.failures > retry_budget
    }

    /// Clears pending safe-failure accounting after a healthy or semantic WebSocket response.
    /// 在 WebSocket 健康完成或产生语义输出后清除待处理的安全失败计数。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` identify the exact session route whose transient count is reset.
    /// `route` 与 `request` 标识需要重置瞬态计数的精确会话路由。
    ///
    /// # Returns
    /// 返回值
    /// This function mutates process-local state and returns no value.
    /// 此函数修改进程内状态且不返回值。
    fn clear_responses_websocket_safe_failures(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
    ) {
        let Some(scope_key) = responses_websocket_fallback_scope_key(route, request) else {
            return;
        };
        self.responses_websocket_safe_failure_scopes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(scope_key.as_str());
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
    // Keep route, model, credential, media, progress, and cancellation boundaries explicit for callers.
    // 保持路由、模型、凭据、媒体、进度和取消边界显式，便于调用方准确控制生命周期。
    #[allow(clippy::too_many_arguments)]
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
    // Keep multipart transport inputs explicit because each field maps to an independent provider contract value.
    // 保持 Multipart 传输输入显式，因为每个字段都对应供应商契约中的独立值。
    #[allow(clippy::too_many_arguments)]
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

    /// Encodes the exact primary wire body selected for this route and session without credentials.
    /// 在不投影凭据的情况下编码该路由与会话选中的精确主传输 Wire 正文。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the frozen provider route and `request` is the immutable canonical request.
    /// `route` 是冻结的供应商路由，`request` 是不可变规范请求。
    ///
    /// # Returns
    /// 返回值
    /// A WebSocket response.create body when WS remains active for the session, otherwise the full
    /// HTTP Responses body with every WebSocket-only continuation field removed.
    /// 会话仍启用 WS 时返回 WebSocket response.create 正文；否则返回移除全部 WS 专属延续字段的
    /// 完整 HTTP Responses 正文。
    pub fn encode_request_for_selected_transport(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let websocket_enabled = route_prefers_codex_responses_websocket(route)
            && !self.responses_websocket_http_fallback_active(route, request);
        let transport_request = if websocket_enabled {
            request.clone()
        } else {
            prepare_http_execution_request(route, request.clone())
        };
        let mut encoded = self.encode_request(route, &transport_request)?;
        if websocket_enabled {
            encoded.body = project_codex_responses_websocket_body(&transport_request, encoded.body);
        }
        Ok(encoded)
    }

    /// Encodes the exact selected transport body and then applies credential-owned projections.
    /// 编码精确选中的传输正文，然后应用凭据拥有的投影。
    pub fn encode_request_for_selected_transport_with_credential(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let encoded = self.encode_request_for_selected_transport(route, request)?;
        project_credential_request(route, secret, encoded)
    }

    /// Executes the standalone Codex Responses compaction endpoint without streaming.
    /// 执行独立 Codex Responses 远端压缩端点，且不使用流式传输。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` are the exact immutable provider route and conversation projection.
    /// `route` 与 `request` 是精确不可变供应商路由和会话投影。
    /// `secret` supplies only the credential-bound authentication projection.
    /// `secret` 仅提供凭据绑定的鉴权投影。
    ///
    /// # Returns
    /// 返回值
    /// A bounded result whose ordered output items can be atomically persisted as a route-bound checkpoint.
    /// 一份有界结果；其有序输出项可作为路由绑定检查点原子持久化。
    pub async fn execute_responses_compact(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
    ) -> Result<ResponsesCompactResult, ProviderExecutionFailure> {
        // traceStartedAt measures the complete unary compact attempt, including pre-dispatch validation.
        // traceStartedAt 衡量完整的一元压缩尝试，包括分派前校验。
        let trace_started_at = Instant::now();
        // traceId reuses the non-secret cache scope so concurrent session compactions remain distinguishable.
        // traceId 复用不含密钥的缓存作用域，使并发会话压缩仍可区分。
        let trace_id = request
            .cache_scope_id
            .clone()
            .unwrap_or_else(|| String::from("responses-compact"));
        emit_provider_transport_trace(
            trace_id.as_str(),
            "responses_compact",
            "execution_started",
            &trace_started_at,
        );
        // result keeps every early failure inside the same secret-safe diagnostic boundary.
        // result 将所有提前失败保留在同一个不含密钥的诊断边界内。
        let result = async {
            if route.protocol_route.protocol_profile_id != OPENAI_CODEX_RESPONSES_PROFILE_ID {
                return Err(pre_dispatch_failure(
                    "Standalone remote compaction requires the Codex Responses protocol.",
                ));
            }
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "route_validated",
                &trace_started_at,
            );
            // profile is the exact registered Codex Responses implementation used for both encoding and transport limits.
            // profile 是同时用于编码与传输限制的精确已注册 Codex Responses 实现。
            let profile = self
                .protocols
                .require(route.protocol_route.protocol_profile_id.as_str())
                .map_err(protocol_failure)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "protocol_ready",
                &trace_started_at,
            );
            validate_request_policy(&route, &request)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "request_policy_validated",
                &trace_started_at,
            );
            // encoded preserves the normal request projection order before the compact-only field filter runs.
            // encoded 在压缩专用字段过滤执行前保留普通请求的投影顺序。
            let encoded = profile.encode(&request).map_err(protocol_failure)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "protocol_encoded",
                &trace_started_at,
            );
            // encoded applies provider-owned model and request overrides without credential material.
            // encoded 应用供应商持有的模型与请求覆盖，但不包含凭据材料。
            let encoded = project_provider_request(&route, &request, encoded)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "provider_projected",
                &trace_started_at,
            );
            // encoded adds the exact credential projection only after all public request checks pass.
            // encoded 仅在全部公共请求检查通过后加入精确凭据投影。
            let mut encoded = project_credential_request(&route, &secret, encoded)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "credential_projected",
                &trace_started_at,
            );
            if encoded.path != "/responses" {
                return Err(pre_dispatch_failure(format!(
                    "Codex Responses compaction requires the `/responses` endpoint, got `{}`.",
                    encoded.path
                )));
            }
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "endpoint_validated",
                &trace_started_at,
            );
            prepare_responses_compact_body(&mut encoded.body)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "body_prepared",
                &trace_started_at,
            );
            encoded.path = String::from("/responses/compact");
            let url = route.request_url(encoded.path.as_str());
            let headers = execution_headers(&route, &encoded.headers, &secret)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "headers_ready",
                &trace_started_at,
            );
            // transport freezes the same profile's response limits for the unary HTTP call.
            // transport 为一元 HTTP 调用冻结同一协议实现的响应限制。
            let transport = profile.transport_profile();
            // compactRequestStartedAt and compactRequestTimeout bound both headers and the complete body.
            // compactRequestStartedAt 与 compactRequestTimeout 同时约束响应头和完整正文。
            let compact_request_started_at = Instant::now();
            let compact_request_timeout = Duration::from_millis(
                transport
                    .stream_idle_timeout_ms
                    .saturating_mul(u64::from(RESPONSES_COMPACT_TIMEOUT_IDLE_MULTIPLIER)),
            );
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "transport_ready",
                &trace_started_at,
            );
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "request_dispatched",
                &trace_started_at,
            );
            let response = tokio::time::timeout(
                Duration::from_millis(transport.response_headers_timeout_ms),
                self.http
                    .post(url.as_str())
                    .headers(headers)
                    .json(&encoded.body)
                    .send(),
            )
            .await
            .map_err(|_| {
                transport_failure(format!(
                    "Provider compaction request to `{url}` exceeded the registered {} ms response-header timeout.",
                    transport.response_headers_timeout_ms
                ))
            })?
            .map_err(|error| {
                http_send_transport_failure(
                    &error,
                    format!("Provider compaction request to `{url}` failed: {error}"),
                )
            })?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "response_headers_received",
                &trace_started_at,
            );
            let status = response.status();
            emit_provider_transport_http_status_trace(
                trace_id.as_str(),
                "responses_compact",
                status.as_u16(),
                &trace_started_at,
            );
            if !status.is_success() {
                let retry_after = parse_retry_after_headers(response.headers(), SystemTime::now());
                let body = read_responses_compact_body_with_timeout(
                    response,
                    transport.max_error_body_bytes,
                    url.as_str(),
                    status.as_u16(),
                    retry_after.clone(),
                    compact_request_started_at,
                    compact_request_timeout,
                )
                .await?;
                return Err(http_failure(status.as_u16(), retry_after, body.as_str()));
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            if !content_type
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
            {
                return Err(compact_protocol_failure(format!(
                    "Provider compaction response from `{url}` must use application/json, got `{content_type}`."
                )));
            }
            let body = read_responses_compact_body_with_timeout(
                response,
                transport.max_frame_bytes,
                url.as_str(),
                status.as_u16(),
                None,
                compact_request_started_at,
                compact_request_timeout,
            )
            .await?;
            let value: Value = serde_json::from_str(body.as_str()).map_err(|error| {
                compact_protocol_failure(format!(
                    "Provider compaction response from `{url}` is invalid JSON: {error}"
                ))
            })?;
            emit_responses_compact_response_shape_trace(
                trace_id.as_str(),
                &value,
                &trace_started_at,
            );
            decode_responses_compact_result(&value, url.as_str())
        }
        .await;
        match &result {
            Ok(_) => emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "execution_finished",
                &trace_started_at,
            ),
            Err(failure) => emit_provider_transport_failure_trace(
                trace_id.as_str(),
                "responses_compact",
                failure,
                &trace_started_at,
            ),
        }
        result
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
                http_send_transport_failure(
                    &error,
                    format!("Provider CountTokens request to `{url}` failed: {error}"),
                )
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
        self.execute_with_websocket_retry_budget(route, request, secret, execution_id, 0)
    }

    /// Executes one route while deferring safe WebSocket fallback through a bounded host retry budget.
    /// 执行一条路由，并通过有界宿主重试预算延后安全的 WebSocket 回退。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `request`, `secret`, and `execution_id` define the exact physical request;
    /// `websocket_retry_budget` is the number of host-controlled retries allowed before HTTP fallback.
    /// `route`、`request`、`secret` 与 `execution_id` 定义精确物理请求；
    /// `websocket_retry_budget` 是 HTTP 回退前允许的宿主控制重试次数。
    ///
    /// # Returns
    /// 返回值
    /// A normalized stream that either exposes the safe failure to the host retry loop or falls back
    /// to HTTP only after the exact session budget is exhausted.
    /// 返回规范化事件流：在预算未耗尽时向宿主重试循环暴露安全失败，仅在精确会话预算耗尽后回退 HTTP。
    pub fn execute_with_websocket_retry_budget(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
        websocket_retry_budget: u32,
    ) -> ProviderExecutionStream {
        let websocket_configured = route_prefers_codex_responses_websocket(&route);
        let websocket_disabled_for_session =
            websocket_configured && self.responses_websocket_http_fallback_active(&route, &request);
        let websocket_enabled = websocket_configured && !websocket_disabled_for_session;
        if !websocket_enabled {
            if websocket_disabled_for_session {
                let skipped_at = Instant::now();
                emit_provider_transport_trace(
                    execution_id.as_str(),
                    "responses_websocket",
                    "skipped_after_session_http_fallback",
                    &skipped_at,
                );
            }
            return self.execute_http(route, request, secret, execution_id);
        }
        let client = self.clone();
        Box::pin(try_stream! {
            let fallback_started_at = Instant::now();
            let mut websocket = client.execute_responses_websocket(
                route.clone(),
                request.clone(),
                secret.clone(),
                execution_id.clone(),
            );
            // Deferred lifecycle events prevent a failed pre-semantic WebSocket attempt from
            // publishing stale response ids before the same logical request falls back to HTTP.
            // 延迟生命周期事件可避免在同一逻辑请求回退 HTTP 前发布失败 WS 尝试产生的过期响应标识。
            let mut deferred_websocket_events = Vec::new();
            let mut websocket_semantic_output_observed = false;
            let mut websocket_failure_observed = false;
            while let Some(result) = websocket.next().await {
                match result {
                    Ok(event) => {
                        if stream_event_has_semantic_output(&event)
                            && !websocket_semantic_output_observed
                        {
                            websocket_semantic_output_observed = true;
                            client.clear_responses_websocket_safe_failures(&route, &request);
                            for deferred_event in deferred_websocket_events.drain(..) {
                                yield deferred_event;
                            }
                        }
                        if websocket_semantic_output_observed {
                            yield event;
                        } else {
                            deferred_websocket_events.push(event);
                        }
                    }
                    Err(failure) => {
                        websocket_failure_observed = true;
                        let safe_http_fallback =
                            websocket_failure_allows_http_fallback(&failure);
                        // Unsupported handshake statuses are a route capability fact, not a transient outage.
                        // 不支持的握手状态表示路由能力事实，而不是瞬时故障。
                        let immediate_http_fallback = safe_http_fallback
                            && websocket_failure_requires_immediate_http_fallback(&failure);
                        let retry_budget_exhausted = immediate_http_fallback
                            || safe_http_fallback
                            && client.record_responses_websocket_safe_failure(
                                &route,
                                &request,
                                websocket_retry_budget,
                            );
                        if safe_http_fallback && retry_budget_exhausted {
                            emit_provider_transport_trace(
                                execution_id.as_str(),
                                "responses_websocket",
                                "fallback_to_http",
                                &fallback_started_at,
                            );
                            client.remember_responses_websocket_http_fallback(&route, &request);
                            deferred_websocket_events.clear();
                            let mut http = client.execute_http(
                                route.clone(),
                                request.clone(),
                                secret.clone(),
                                execution_id.clone(),
                            );
                            while let Some(result) = http.next().await {
                                yield result?;
                            }
                        } else if safe_http_fallback {
                            emit_provider_transport_trace(
                                execution_id.as_str(),
                                "responses_websocket",
                                "retry_deferred_to_host",
                                &fallback_started_at,
                            );
                            deferred_websocket_events.clear();
                            Err(failure)?;
                        } else {
                            emit_provider_transport_trace(
                                execution_id.as_str(),
                                "responses_websocket",
                                "fallback_rejected_after_semantic_output",
                                &fallback_started_at,
                            );
                            for deferred_event in deferred_websocket_events.drain(..) {
                                yield deferred_event;
                            }
                            Err(failure)?;
                        }
                        break;
                    }
                }
            }
            if !websocket_failure_observed {
                client.clear_responses_websocket_safe_failures(&route, &request);
            }
            for deferred_event in deferred_websocket_events {
                yield deferred_event;
            }
        })
    }

    /// Starts one immutable HTTP SSE execution without transport fallback.
    /// 启动一次不可变 HTTP SSE 执行，不进行传输回退。
    fn execute_http(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
    ) -> ProviderExecutionStream {
        let client = self.clone();
        Box::pin(try_stream! {
            let transport_started_at = Instant::now();
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "execution_started",
                &transport_started_at,
            );
            let request = prepare_http_execution_request(&route, request);
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
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "request_dispatched",
                &transport_started_at,
            );
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
            .map_err(|error| {
                http_send_transport_failure(
                    &error,
                    format!("Provider request to `{url}` failed: {error}"),
                )
            })?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "response_headers_received",
                &transport_started_at,
            );
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
            let mut decoder = profile.create_stream_decoder(execution_id.clone(), &request);
            let mut parser = IncrementalFrameParser::new(transport);
            let mut bytes = response.bytes_stream();
            let mut semantic_output_observed = false;
            let mut first_chunk_observed = false;
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
                if !first_chunk_observed {
                    first_chunk_observed = true;
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "http_sse",
                        "first_chunk_received",
                        &transport_started_at,
                    );
                }
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
                        let was_semantic_output_observed = semantic_output_observed;
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut event,
                        );
                        if semantic_output_observed && !was_semantic_output_observed {
                            emit_provider_transport_trace(
                                execution_id.as_str(),
                                "http_sse",
                                "first_semantic_event",
                                &transport_started_at,
                            );
                        }
                        if matches!(
                            event,
                            AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                        ) {
                            emit_provider_transport_trace(
                                execution_id.as_str(),
                                "http_sse",
                                "terminal_event",
                                &transport_started_at,
                            );
                        }
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
                .map_err(|error| stream_completion_failure(error, semantic_output_observed))?
            {
                let was_semantic_output_observed = semantic_output_observed;
                update_semantic_output_observation(
                    &mut semantic_output_observed,
                    &mut event,
                );
                if semantic_output_observed && !was_semantic_output_observed {
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "http_sse",
                        "first_semantic_event",
                        &transport_started_at,
                    );
                }
                if matches!(
                    event,
                    AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                ) {
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "http_sse",
                        "terminal_event",
                        &transport_started_at,
                    );
                }
                validate_stream_event(&mut stream_state, &event, semantic_output_observed)?;
                yield event;
            }
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "execution_finished",
                &transport_started_at,
            );
        })
    }

    /// Executes one Codex Responses request over a JSON WebSocket stream.
    /// 通过 JSON WebSocket 流执行一次 Codex Responses 请求。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `request`, `secret`, and `execution_id` are the same immutable execution inputs
    /// used by the HTTP path.
    /// `route`、`request`、`secret` 与 `execution_id` 与 HTTP 路径使用相同的不可变执行输入。
    ///
    /// # Returns
    /// 返回值
    /// A normalized stream. A failure before semantic output is marked retryable so the caller can
    /// safely fall back to HTTP SSE; failures after semantic output remain terminal.
    /// 返回归一化流。语义输出前的失败会标记为可重试，以便调用方安全回退 HTTP SSE；语义输出后的失败保持终态。
    fn execute_responses_websocket(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
    ) -> ProviderExecutionStream {
        let client = self.clone();
        Box::pin(try_stream! {
            let transport_started_at = Instant::now();
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "execution_started",
                &transport_started_at,
            );
            let encoded = client.encode_request_with_credential(&route, &request, &secret)?;
            let profile = client
                .protocols
                .require(route.protocol_route.protocol_profile_id.as_str())
                .map_err(protocol_failure)?;
            if route.protocol_route.protocol_profile_id != OPENAI_CODEX_RESPONSES_PROFILE_ID {
                Err(websocket_protocol_failure(
                    "Responses WebSocket transport is registered only for the Codex Responses profile.",
                ))?;
            }
            let transport = profile.transport_profile();
            let http_url = route.request_url(encoded.path.as_str());
            let websocket_url = websocket_url_from_http(http_url.as_str())
                .map_err(|error| websocket_protocol_failure(error.message))?;
            let headers = execution_headers(&route, &encoded.headers, &secret)?;
            let mut websocket_request = websocket_url
                .as_str()
                .into_client_request()
                .map_err(|error| websocket_handshake_failure(format!(
                    "Provider WebSocket URL `{websocket_url}` is invalid: {error}"
                )))?;
            for (name, value) in headers.iter() {
                let websocket_name = name
                    .as_str()
                    .parse::<WebSocketHeaderName>()
                    .map_err(|error| {
                    websocket_handshake_failure(format!(
                        "Provider WebSocket header name `{name}` is invalid: {error}"
                    ))
                })?;
                let websocket_value = value.to_str().map_err(|error| {
                    websocket_handshake_failure(format!(
                        "Provider WebSocket header `{name}` is not valid UTF-8: {error}"
                    ))
                })?.parse::<WebSocketHeaderValue>().map_err(|error| {
                    websocket_handshake_failure(format!(
                        "Provider WebSocket header `{name}` is invalid: {error}"
                    ))
                })?;
                websocket_request
                    .headers_mut()
                    .insert(websocket_name, websocket_value);
            }
            insert_responses_websocket_v2_beta_header(websocket_request.headers_mut());
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "handshake_started",
                &transport_started_at,
            );
            let (mut websocket, _) = tokio::time::timeout(
                Duration::from_millis(transport.response_headers_timeout_ms),
                connect_async(websocket_request),
            )
            .await
            .map_err(|_| {
                websocket_handshake_failure(format!(
                    "Provider WebSocket handshake to `{websocket_url}` exceeded the registered {} ms response-header timeout.",
                    transport.response_headers_timeout_ms
                ))
            })?
            .map_err(|error| {
                websocket_handshake_failure_from_error(
                    error,
                    format!("Provider WebSocket handshake to `{websocket_url}` failed"),
                )
            })?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "handshake_completed",
                &transport_started_at,
            );
            let websocket_body = project_codex_responses_websocket_body(&request, encoded.body.clone());
            let request_json = serde_json::to_string(&websocket_body).map_err(|error| {
                websocket_protocol_failure(format!(
                    "Codex WebSocket response.create request could not be serialized: {error}"
                ))
            })?;
            websocket
                .send(Message::text(request_json))
                .await
                .map_err(|error| {
                    stream_transport_failure(
                        format!("Provider WebSocket request to `{websocket_url}` failed: {error}"),
                        false,
                    )
                })?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "request_sent",
                &transport_started_at,
            );
            let mut decoder = profile.create_stream_decoder(execution_id.clone(), &request);
            let mut semantic_output_observed = false;
            let mut stream_state = AiStreamStateMachine::default();
            let mut terminal_observed = false;
            let mut first_frame_observed = false;
            let mut close_detail = None;
            loop {
                let next_frame = tokio::time::timeout(
                    Duration::from_millis(transport.stream_idle_timeout_ms),
                    websocket.next(),
                )
                .await
                .map_err(|_| {
                    stream_transport_failure(
                        format!(
                            "Provider WebSocket stream from `{websocket_url}` exceeded the registered {} ms idle timeout.",
                            transport.stream_idle_timeout_ms
                        ),
                        semantic_output_observed,
                    )
                })?;
                let Some(frame) = next_frame else {
                    break;
                };
                let frame = frame.map_err(|error| {
                    stream_transport_failure(
                        format!("Provider WebSocket stream from `{websocket_url}` failed: {error}"),
                        semantic_output_observed,
                    )
                })?;
                if !first_frame_observed {
                    first_frame_observed = true;
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "responses_websocket",
                        "first_frame_received",
                        &transport_started_at,
                    );
                }
                let value = match frame {
                    Message::Text(text) => serde_json::from_str::<Value>(text.as_ref()).map_err(|error| {
                        stream_protocol_failure(ProtocolConversionError::new(format!(
                            "Provider WebSocket text event from `{websocket_url}` is invalid JSON: {error}"
                        )), semantic_output_observed)
                    })?,
                    Message::Binary(bytes) => serde_json::from_slice::<Value>(bytes.as_ref()).map_err(|error| {
                        stream_protocol_failure(ProtocolConversionError::new(format!(
                            "Provider WebSocket binary event from `{websocket_url}` is invalid JSON: {error}"
                        )), semantic_output_observed)
                    })?,
                    Message::Ping(payload) => {
                        websocket.send(Message::Pong(payload)).await.map_err(|error| {
                            stream_transport_failure(
                                format!("Provider WebSocket pong to `{websocket_url}` failed: {error}"),
                                semantic_output_observed,
                            )
                        })?;
                        continue;
                    }
                    Message::Pong(_) => continue,
                    Message::Close(close_frame) => {
                        if let Some(close_frame) = close_frame.as_ref() {
                            let close_code = format!("{:?}", close_frame.code);
                            let close_reason = close_frame.reason.as_ref();
                            emit_provider_transport_close_trace(
                                execution_id.as_str(),
                                close_code.as_str(),
                                close_reason,
                                &transport_started_at,
                            );
                            close_detail = Some(format!(
                                "close code {close_code} with reason `{close_reason}`"
                            ));
                        } else {
                            close_detail = Some(String::from("a close frame without code or reason"));
                        }
                        emit_provider_transport_trace(
                            execution_id.as_str(),
                            "responses_websocket",
                            "close_frame_received",
                            &transport_started_at,
                        );
                        break;
                    }
                    Message::Frame(_) => continue,
                };
                if let Some(failure) = websocket_wrapped_error_failure(&value) {
                    Err(failure)?;
                }
                for mut event in decoder
                    .push_json(&value)
                    .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
                {
                    let was_semantic_output_observed = semantic_output_observed;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut event,
                    );
                    if semantic_output_observed && !was_semantic_output_observed {
                        emit_provider_transport_trace(
                            execution_id.as_str(),
                            "responses_websocket",
                            "first_semantic_event",
                            &transport_started_at,
                        );
                    }
                    terminal_observed = terminal_observed
                        || matches!(event, AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. });
                    if terminal_observed {
                        emit_provider_transport_trace(
                            execution_id.as_str(),
                            "responses_websocket",
                            "terminal_event",
                            &transport_started_at,
                        );
                    }
                    validate_stream_event(&mut stream_state, &event, semantic_output_observed)?;
                    yield event;
                }
                if terminal_observed {
                    break;
                }
            }
            if !terminal_observed {
                let close_detail = close_detail
                    .as_deref()
                    .unwrap_or("the transport ended without a close frame");
                let mut failure = stream_transport_failure(
                    format!("Provider WebSocket stream from `{websocket_url}` closed before a terminal Responses event: {close_detail}."),
                    semantic_output_observed,
                );
                // The upstream account-pool signal is safe to replay over HTTPS and should not consume five host retries.
                // 上游账号池信号可以安全通过 HTTPS 重放，不应消耗五次宿主重试。
                if close_detail
                    .to_ascii_lowercase()
                    .contains("no available account")
                {
                    failure.error.subcategory = Some(String::from(
                        "websocket_no_available_account",
                    ));
                }
                Err(failure)?;
            }
            for mut event in decoder
                .finish()
                .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
            {
                let was_semantic_output_observed = semantic_output_observed;
                update_semantic_output_observation(
                    &mut semantic_output_observed,
                    &mut event,
                );
                if semantic_output_observed && !was_semantic_output_observed {
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "responses_websocket",
                        "first_semantic_event",
                        &transport_started_at,
                    );
                }
                terminal_observed = terminal_observed
                    || matches!(event, AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. });
                if terminal_observed {
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "responses_websocket",
                        "terminal_event",
                        &transport_started_at,
                    );
                }
                validate_stream_event(&mut stream_state, &event, semantic_output_observed)?;
                yield event;
            }
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "execution_finished",
                &transport_started_at,
            );
        })
    }
}

/// Returns whether one exact route is configured to prefer Codex Responses WebSocket v2.
/// 返回一个精确路由是否配置为优先使用 Codex Responses WebSocket v2。
fn route_prefers_codex_responses_websocket(route: &ProviderExecutionRoute) -> bool {
    route
        .protocol_route
        .responses_features
        .as_ref()
        .is_some_and(|features| {
            features.websocket == crate::ResponsesWebSocketPolicy::PreferredWithHttpFallback
        })
        && route.protocol_route.protocol_profile_id == OPENAI_CODEX_RESPONSES_PROFILE_ID
}

/// Removes WebSocket-only continuation state before encoding one HTTP Responses request.
/// 在编码 HTTP Responses 请求前移除仅属于 WebSocket 的延续状态。
///
/// # Parameters
/// 参数
/// `route` identifies the exact active protocol and `request` is the owned immutable request snapshot.
/// `route` 标识精确的当前协议，`request` 是拥有所有权的不可变请求快照。
///
/// # Returns
/// 返回值
/// A request that retains full canonical history for HTTP while leaving every other protocol unchanged.
/// 返回为 HTTP 保留完整规范历史、且不改变其他协议的请求。
fn prepare_http_execution_request(
    route: &ProviderExecutionRoute,
    mut request: AiExecutionRequest,
) -> AiExecutionRequest {
    if route.protocol_route.protocol_profile_id == OPENAI_CODEX_RESPONSES_PROFILE_ID {
        request.continuation = None;
    }
    request
}

/// Projects the required official Codex Responses WebSocket v2 envelope fields.
/// 投影官方 Codex Responses WebSocket v2 信封要求的字段。
///
/// # Parameters
/// 参数
/// `request` is the immutable public request and `body` is its already validated Codex Responses
/// HTTP-shaped body before the WebSocket-only envelope projection.
/// `request` 是不可变公共请求；`body` 是已经校验、尚未投影 WebSocket 专属信封的 Codex Responses
/// HTTP 形请求正文。
///
/// # Returns
/// 返回值
/// The exact response.create body with official defaults filled only when the public request did not
/// select an explicit value.
/// 返回精确的 response.create 正文，仅在公共请求未显式选择值时补齐官方默认字段。
fn project_codex_responses_websocket_body(request: &AiExecutionRequest, mut body: Value) -> Value {
    body["type"] = Value::String(String::from("response.create"));
    if body.get("tool_choice").is_none() {
        body["tool_choice"] = Value::String(String::from("auto"));
    }
    if body.get("parallel_tool_calls").is_none() {
        body["parallel_tool_calls"] = Value::Bool(!matches!(
            request.parallel_tool_calls,
            ParallelToolCallPolicy::Disabled
        ));
    }
    if body.get("include").is_none() {
        body["include"] = json!(["reasoning.encrypted_content"]);
    }
    if let Some(cache_scope_id) = request.cache_scope_id.as_deref() {
        if body.get("client_metadata").is_none() {
            body["client_metadata"] = Value::Object(serde_json::Map::new());
        }
        if let Some(client_metadata) = body["client_metadata"].as_object_mut() {
            client_metadata
                .entry(String::from("session_id"))
                .or_insert_with(|| Value::String(cache_scope_id.to_string()));
            client_metadata
                .entry(String::from("thread_id"))
                .or_insert_with(|| Value::String(cache_scope_id.to_string()));
        }
    }
    body
}

/// Installs the exact Responses WebSocket v2 capability header on one handshake request.
/// 在一次握手请求上安装精确的 Responses WebSocket v2 能力请求头。
///
/// # Parameters
/// 参数
/// `headers` is the final WebSocket handshake header map after provider and credential projection.
/// `headers` 是完成供应商与凭据投影后的最终 WebSocket 握手请求头集合。
fn insert_responses_websocket_v2_beta_header(
    headers: &mut tokio_tungstenite::tungstenite::http::HeaderMap,
) {
    headers.insert(
        WebSocketHeaderName::from_static(OPENAI_RESPONSES_WEBSOCKET_BETA_HEADER_NAME),
        WebSocketHeaderValue::from_static(OPENAI_RESPONSES_WEBSOCKET_BETA_HEADER_VALUE),
    );
}

/// Returns whether one failed WebSocket attempt can safely restart over HTTP.
/// 返回一次失败的 WebSocket 尝试是否可以安全地通过 HTTP 重新开始。
///
/// # Parameters
/// 参数
/// `failure` is the exact typed failure produced by the WebSocket transport.
/// `failure` 是 WebSocket 传输产生的精确强类型失败。
///
/// # Returns
/// 返回值
/// `true` only before semantic output or side effects, including a dispatched request whose stream
/// ended before any replay-unsafe event became visible.
/// 仅在语义输出或副作用产生前返回 `true`，包括已经分派但在任何不可安全重放事件可见前中断的请求。
fn websocket_failure_allows_http_fallback(failure: &ProviderExecutionFailure) -> bool {
    failure.error.retryable
        && !failure.error.semantic_output_observed
        && !failure.error.side_effect_observed
}

/// Returns whether a WebSocket failure proves the exact route does not support the transport.
/// 返回一次 WebSocket 失败是否已经证明精确路由不支持该传输。
///
/// # Parameters
/// 参数
/// `failure` is the typed handshake or stream failure produced by the Responses WebSocket path.
/// `failure` 是 Responses WebSocket 路径产生的类型化握手或流失败。
///
/// # Returns
/// 返回值
/// `true` only for the registered unsupported-handshake classification, which must bypass
/// transient retry delays and switch to HTTPS immediately.
/// 仅当失败属于已注册的不支持握手分类时返回 `true`，此时必须跳过瞬时重试延迟并立即切换 HTTPS。
fn websocket_failure_requires_immediate_http_fallback(failure: &ProviderExecutionFailure) -> bool {
    matches!(
        failure.error.subcategory.as_deref(),
        Some("websocket_unsupported" | "websocket_no_available_account")
    )
}

/// Converts official wrapped Responses WebSocket errors before the generic stream decoder runs.
/// 在通用流解码器运行前转换官方包装式 Responses WebSocket 错误。
///
/// # Parameters
/// 参数
/// `value` is one decoded JSON WebSocket frame.
/// `value` 是一帧已解码的 JSON WebSocket 消息。
///
/// # Returns
/// 返回值
/// A typed transport failure for official retry signals or non-success wrapped HTTP status, and
/// `None` for ordinary Responses events handled by the shared decoder.
/// 对官方重试信号或包装的非成功 HTTP 状态返回强类型传输失败；普通 Responses 事件返回 `None`
/// 并交给共享解码器处理。
fn websocket_wrapped_error_failure(value: &Value) -> Option<ProviderExecutionFailure> {
    if value.get("type").and_then(Value::as_str) != Some("error") {
        return None;
    }
    // errorPayload supports the official nested wrapper while retaining flat proxy compatibility.
    // errorPayload 支持官方嵌套包装，同时保留扁平代理兼容性。
    let error_payload = value.get("error").unwrap_or(value);
    // errorCode selects only the two retry signals explicitly registered by the official Codex client.
    // errorCode 仅选择官方 Codex 客户端显式注册的两个重试信号。
    let error_code = error_payload.get("code").and_then(Value::as_str);
    if matches!(
        error_code,
        Some(
            OPENAI_RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE
                | OPENAI_RESPONSES_PREVIOUS_RESPONSE_NOT_FOUND_CODE
        )
    ) {
        // message preserves the provider diagnostic and supplies the official recovery meaning when absent.
        // message 保留供应商诊断，并在缺失时补充官方恢复语义。
        let message = error_payload
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| match error_code {
                Some(OPENAI_RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE) => String::from(
                    "Responses WebSocket connection limit reached; retry with a new transport.",
                ),
                _ => String::from(
                    "Previous response was not found; retry the complete request without its cursor.",
                ),
            });
        // failure is replay-safe because the wrapper arrived before any semantic event escaped.
        // failure 可安全重放，因为该包装错误出现在任何语义事件逸出之前。
        let mut failure = stream_transport_failure(message, false);
        failure.error.subcategory = error_code.map(str::to_string);
        return Some(failure);
    }
    // status mirrors the official `status` plus legacy `status_code` alias.
    // status 镜像官方 `status` 字段及旧版 `status_code` 别名。
    let status = value
        .get("status")
        .or_else(|| value.get("status_code"))
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())?;
    if (200..300).contains(&status) {
        return None;
    }
    Some(http_failure(status, None, value.to_string().as_str()))
}

/// Emits one opt-in, secret-safe timing point for a physical provider transport attempt.
/// 为一次物理供应商传输尝试输出可选且不含密钥的计时点。
///
/// # Parameters
/// 参数
/// `execution_id` is the opaque local execution correlation id; `transport` and `stage` are
/// fixed internal labels; `started_at` is the monotonic start time for this physical attempt.
/// `execution_id` 是本地不透明执行关联标识；`transport` 与 `stage` 是固定内部标签；
/// `started_at` 是本次物理尝试的单调起始时间。
fn emit_provider_transport_trace(
    execution_id: &str,
    transport: &'static str,
    stage: &'static str,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport={transport} stage={stage} elapsed_ms={}",
        started_at.elapsed().as_millis()
    );
}

/// Emits one opt-in HTTP status without response bodies, headers, or credentials.
/// 输出一项可选 HTTP 状态，且不包含响应正文、请求头或凭据。
///
/// # Parameters
/// 参数
/// `execution_id` is the non-secret local correlation id, `transport` is a fixed internal label,
/// `status` is the numeric HTTP response code, and `started_at` is the monotonic attempt start.
/// `execution_id` 是不含密钥的本地关联标识，`transport` 是固定内部标签，`status` 是数字 HTTP
/// 响应码，`started_at` 是尝试开始时的单调时钟。
fn emit_provider_transport_http_status_trace(
    execution_id: &str,
    transport: &'static str,
    status: u16,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport={transport} stage=response_http_status elapsed_ms={} status={status}",
        started_at.elapsed().as_millis()
    );
}

/// Emits one opt-in normalized failure classification without the possibly sensitive error message.
/// 输出一项可选标准化失败分类，且不包含可能敏感的错误消息。
///
/// # Parameters
/// 参数
/// `execution_id` and `transport` identify the physical attempt, `failure` supplies only normalized
/// classification fields, and `started_at` is the monotonic attempt start.
/// `execution_id` 与 `transport` 标识真实尝试，`failure` 仅提供标准化分类字段，`started_at`
/// 是尝试开始时的单调时钟。
fn emit_provider_transport_failure_trace(
    execution_id: &str,
    transport: &'static str,
    failure: &ProviderExecutionFailure,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport={transport} stage=execution_failed elapsed_ms={} category={:?} subcategory={:?} error_stage={:?} http_status={:?} dispatch_outcome={:?} retryable={}",
        started_at.elapsed().as_millis(),
        failure.error.category,
        failure.error.subcategory,
        failure.error.stage,
        failure.error.http_status,
        failure.error.dispatch_outcome,
        failure.error.retryable
    );
}

/// Emits one opt-in Responses compact JSON shape without response values or user content.
/// 输出一项可选 Responses 压缩 JSON 形状，且不包含响应值或用户内容。
///
/// # Parameters
/// 参数
/// `execution_id` is the non-secret local correlation id, `value` is the decoded JSON response,
/// and `started_at` is the monotonic attempt start.
/// `execution_id` 是不含密钥的本地关联标识，`value` 是已解码 JSON 响应，`started_at` 是尝试开始时的单调时钟。
fn emit_responses_compact_response_shape_trace(
    execution_id: &str,
    value: &Value,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    // topLevelKeys lists bounded structural names only and never serializes their values.
    // topLevelKeys 仅列出有界结构名称，绝不序列化对应值。
    let top_level_keys = value
        .as_object()
        .map(|object| {
            object
                .keys()
                .take(32)
                .map(|key| bounded_trace_label(key))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // outputKind distinguishes absent, array, object, and scalar shapes without exposing content.
    // outputKind 在不暴露内容的情况下区分缺失、数组、对象与标量形状。
    let output_kind = match value.get("output") {
        None => "absent",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
        Some(_) => "scalar",
    };
    // outputLen reports the exact array length while item details remain bounded below.
    // outputLen 报告精确数组长度，而下方 item 详情仍保持有界。
    let output_len = value
        .get("output")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    // outputItems captures only bounded item type labels and object key names.
    // outputItems 仅捕获有界 item 类型标签与对象键名。
    let output_items = value
        .get("output")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(16)
                .map(|item| {
                    // itemType is a bounded structural discriminator rather than response content.
                    // itemType 是有界结构判别符，而不是响应内容。
                    let item_type = item
                        .get("type")
                        .and_then(Value::as_str)
                        .map(bounded_trace_label)
                        .unwrap_or_else(|| String::from("<missing>"));
                    // itemKeys lists object member names only.
                    // itemKeys 仅列出对象成员名称。
                    let item_keys = item
                        .as_object()
                        .map(|object| {
                            object
                                .keys()
                                .take(32)
                                .map(|key| bounded_trace_label(key))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    format!("{item_type}:{item_keys:?}")
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport=responses_compact stage=response_json_shape elapsed_ms={} top_level_keys={top_level_keys:?} output_kind={output_kind} output_len={} output_items={output_items:?}",
        started_at.elapsed().as_millis(),
        output_len,
    );
}

/// Normalizes one provider-controlled structural label into bounded ASCII trace text.
/// 把单个供应商控制的结构标签规范化为有界 ASCII 追踪文本。
///
/// # Parameters
/// 参数
/// `value` is a JSON object key or item type discriminator.
/// `value` 是 JSON 对象键或 item 类型判别符。
///
/// # Returns
/// 返回值
/// At most 64 safe ASCII characters with unsupported characters replaced by `_`.
/// 返回最多 64 个安全 ASCII 字符，不支持的字符替换为 `_`。
fn bounded_trace_label(value: &str) -> String {
    value
        .chars()
        .take(64)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Returns whether secret-safe provider transport tracing is explicitly enabled.
/// 返回是否已经显式启用不含密钥的供应商传输追踪。
fn provider_transport_trace_enabled() -> bool {
    std::env::var(PROVIDER_TRANSPORT_TRACE_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Emits one bounded WebSocket close code and provider reason for opt-in diagnostics.
/// 为可选诊断输出有界的 WebSocket 关闭码与供应商原因。
///
/// # Parameters
/// 参数
/// `execution_id` is the local correlation id, `code` is the protocol close code, `reason` is the
/// provider close reason, and `started_at` is the physical attempt start time.
/// `execution_id` 是本地关联标识，`code` 是协议关闭码，`reason` 是供应商关闭原因，
/// `started_at` 是物理尝试起始时间。
fn emit_provider_transport_close_trace(
    execution_id: &str,
    code: &str,
    reason: &str,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    let bounded_reason = reason
        .chars()
        .take(256)
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport=responses_websocket stage=close_frame_detail elapsed_ms={} code={code} reason={bounded_reason:?}",
        started_at.elapsed().as_millis()
    );
}

/// Returns whether one normalized event makes transparent replay unsafe.
/// 返回一条归一化事件是否会使透明重放变得不安全。
///
/// # Parameters
/// 参数
/// `event` is the exact normalized event emitted by one physical provider attempt.
/// `event` 是一次真实供应商尝试发出的精确归一化事件。
///
/// # Returns
/// 返回值
/// `true` for user-visible, tool-visible, or resource-side-effect semantic output.
/// 对用户可见、工具可见或资源副作用语义输出返回 `true`。
fn stream_event_has_semantic_output(event: &AiStreamEvent) -> bool {
    matches!(
        event,
        AiStreamEvent::ResponseTextDelta { .. }
            | AiStreamEvent::ReasoningDelta { .. }
            | AiStreamEvent::ToolCallStarted { .. }
            | AiStreamEvent::ResourceProgress { .. }
    )
}

/// Tracks semantic output and annotates terminal public failures with the observed state.
/// 跟踪语义输出并用已观测状态标注公共终态失败。
fn update_semantic_output_observation(observed: &mut bool, event: &mut AiStreamEvent) {
    if stream_event_has_semantic_output(event) {
        *observed = true;
    }
    if let AiStreamEvent::ResponseFailed { error } = event {
        error.semantic_output_observed = *observed;
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

/// Converts an HTTP(S) provider URL into its exact WebSocket scheme counterpart.
/// 将 HTTP(S) 供应商 URL 转换为精确对应的 WebSocket Scheme。
///
/// # Parameters
/// 参数
/// `url` is the already resolved immutable HTTP request URL.
/// `url` 是已经解析完成的不可变 HTTP 请求 URL。
///
/// # Returns
/// 返回值
/// A `ws://` or `wss://` URL, or a deterministic protocol conversion error for unsupported schemes.
/// 返回 `ws://` 或 `wss://` URL；遇到不支持的 Scheme 时返回确定性的协议转换错误。
fn websocket_url_from_http(url: &str) -> Result<String, ProtocolConversionError> {
    if let Some(rest) = url.strip_prefix("https://") {
        return Ok(format!("wss://{rest}"));
    }
    if let Some(rest) = url.strip_prefix("http://") {
        return Ok(format!("ws://{rest}"));
    }
    Err(ProtocolConversionError::new(format!(
        "Provider WebSocket transport requires an http:// or https:// route URL, got `{url}`."
    )))
}

/// Creates a pre-dispatch WebSocket failure that permits the HTTP fallback path.
/// 创建允许回退到 HTTP 路径的分派前 WebSocket 失败。
///
/// # Parameters
/// 参数
/// `message` is a controlled handshake or setup diagnostic.
/// `message` 是受控的握手或准备阶段诊断。
///
/// # Returns
/// 返回值
/// A retryable pre-dispatch failure with no observed semantic output or side effect.
/// 返回可重试且尚未分派推理请求、未观察到语义输出或副作用的失败。
fn websocket_handshake_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("websocket")),
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
            // The WebSocket handshake precedes `response.create`; no inference request was dispatched.
            // WebSocket 握手发生在 `response.create` 之前；此时尚未分派推理请求。
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: true,
            retry_after: None,
        }),
    }
}

/// Classifies a WebSocket handshake error before deciding whether HTTP fallback is safe.
/// 在决定是否安全回退 HTTP 前，对 WebSocket 握手错误进行分类。
///
/// # Parameters
/// 参数
/// `error` is the exact tungstenite handshake error, and `context` identifies the route operation.
/// `error` 是 tungstenite 的精确握手错误，`context` 标识路由操作。
///
/// # Returns
/// 返回值
/// An authentication/permission failure that must remain visible, or a retryable unsupported/
/// unavailable failure that can be handled by the existing pre-semantic HTTP fallback.
/// 返回必须显式暴露的鉴权/权限失败，或可由既有语义输出前 HTTP 回退处理的可重试不支持/不可用失败。
fn websocket_handshake_failure_from_error(
    error: tokio_tungstenite::tungstenite::Error,
    context: String,
) -> ProviderExecutionFailure {
    let message = format!("{context}: {error}");
    match &error {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            websocket_handshake_http_failure(response.status().as_u16(), message)
        }
        _ => websocket_handshake_failure(message),
    }
}

/// Builds a typed failure for an HTTP response returned during the WebSocket handshake.
/// 为 WebSocket 握手阶段返回的 HTTP 响应构造类型化失败。
///
/// # Parameters
/// 参数
/// `status` is the handshake HTTP status, and `message` is the bounded diagnostic.
/// `status` 是握手 HTTP 状态码，`message` 是受控诊断信息。
///
/// # Returns
/// 返回值
/// Authentication and permission failures are terminal and never silently fall back; unsupported
/// WebSocket statuses remain retryable so the caller can use HTTP before semantic output exists.
/// 鉴权与权限失败为终止错误且绝不静默回退；不支持 WebSocket 的状态仍可重试，以便在产生语义输出前使用 HTTP。
fn websocket_handshake_http_failure(
    status: u16,
    message: impl Into<String>,
) -> ProviderExecutionFailure {
    let (category, subcategory, retryable) = match status {
        401 => (
            AiErrorCategory::Authentication,
            "websocket_authentication",
            false,
        ),
        403 => (
            AiErrorCategory::PermissionDenied,
            "websocket_permission",
            false,
        ),
        404 | 405 | 426 => (
            AiErrorCategory::ProviderUnavailable,
            "websocket_unsupported",
            true,
        ),
        _ => (AiErrorCategory::ProviderUnavailable, "websocket", true),
    };
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category,
            subcategory: Some(String::from(subcategory)),
            stage: String::from("response_stream"),
            http_status: Some(status),
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            // An HTTP handshake response still precedes the model-facing `response.create` frame.
            // 即使收到了 HTTP 握手响应，面向模型的 `response.create` 帧仍未发送。
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable,
            retry_after: None,
        }),
    }
}

/// Creates a pre-dispatch protocol failure for an invalid WebSocket request shape.
/// 为无效 WebSocket 请求形态创建分派前协议失败。
///
/// # Parameters
/// 参数
/// `message` is the controlled protocol diagnostic.
/// `message` 是受控协议诊断。
///
/// # Returns
/// 返回值
/// A non-retryable protocol conversion failure that prevents an unsafe transport fallback.
/// 返回阻止不安全传输回退的不可重试协议转换失败。
fn websocket_protocol_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    protocol_failure(ProtocolConversionError::new(message))
}

/// Restricts one encoded Codex compaction body to the official `CompactionInput` fields.
/// 将编码后的 Codex 压缩请求体限制为官方 `CompactionInput` 字段集合。
///
/// # Parameters
/// 参数
/// `body` is the protocol-encoded Responses request body before the compact endpoint rewrite.
/// `body` 是改写到 compact 端点前的协议编码 Responses 请求体。
///
/// # Returns
/// 返回值
/// Success after removing non-compact fields while preserving the encoded parallel-call policy, or
/// a pre-dispatch failure when the encoder did not produce an object body.
/// 成功时移除非压缩字段并保留已编码的并行调用策略；编码器未产生对象体时返回分派前失败。
fn prepare_responses_compact_body(body: &mut Value) -> Result<(), ProviderExecutionFailure> {
    let object = body.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure("Codex Responses compaction request body must be a JSON object.")
    })?;
    object.retain(|key, _| {
        matches!(
            key.as_str(),
            "model"
                | "input"
                | "instructions"
                | "tools"
                | "parallel_tool_calls"
                | "reasoning"
                | "service_tier"
                | "prompt_cache_key"
                | "text"
        )
    });
    if object
        .get("instructions")
        .and_then(Value::as_str)
        .is_some_and(str::is_empty)
    {
        object.remove("instructions");
    }
    Ok(())
}

/// Decodes and validates one standalone Responses compact response.
/// 解码并校验一份独立 Responses compact 响应。
///
/// # Parameters
/// 参数
/// `value` is the bounded JSON response body and `url` identifies the exact endpoint in errors.
/// `value` 是已受大小限制的 JSON 响应体，`url` 用于错误中标识精确端点。
///
/// # Returns
/// 返回值
/// A result containing the ordered non-empty `compaction` output items.
/// 返回按原顺序保留的非空 `compaction` 输出项。
fn decode_responses_compact_result(
    value: &Value,
    url: &str,
) -> Result<ResponsesCompactResult, ProviderExecutionFailure> {
    let mut output_items = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            compact_protocol_failure(format!(
                "Provider compaction response from `{url}` does not contain an output array."
            ))
        })?
        .clone();
    canonicalize_codex_responses_replay_items(&mut output_items);
    let contains_compaction = output_items
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction"));
    if output_items.is_empty()
        || !output_items.iter().all(is_replayable_codex_responses_item)
        || !contains_compaction
    {
        return Err(compact_protocol_failure(format!(
            "Provider compaction response from `{url}` must contain ordered replayable output items and a compaction item."
        )));
    }
    Ok(ResponsesCompactResult {
        output_items,
        usage: value.get("usage").cloned(),
        response_id: value.get("id").and_then(Value::as_str).map(str::to_string),
    })
}

/// Builds one dispatched protocol failure from the standalone Responses compact response.
/// 根据独立 Responses 压缩响应构建一项已分派协议失败。
fn compact_protocol_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("response_compact"),
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

/// Builds one dispatched transport failure from the standalone Responses compact response.
/// 根据独立 Responses 压缩响应构建一项已分派传输失败。
///
/// # Parameters
/// 参数
/// `message` is the controlled transport diagnostic produced after response dispatch.
/// `message` 是响应分派后产生的受控传输诊断。
///
/// # Returns
/// 返回值
/// A retryable failure that allows the compaction coordinator to use its local safe fallback.
/// 返回允许压缩协调器使用本地安全回退的可重试失败。
fn compact_transport_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport")),
            stage: String::from("response_compact"),
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
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: true,
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

/// Converts end-of-stream validation into a retry-safe transport failure or a protocol failure.
/// 将流结束校验转换为可安全重试的传输失败或协议失败。
///
/// # Parameters
/// 参数
/// `error` is the typed decoder completion failure and `semantic_output_observed` records whether
/// replay would duplicate user-visible or tool-visible semantics.
/// `error` 是强类型解码器结束失败；`semantic_output_observed` 记录重放是否会重复用户或工具可见语义。
///
/// # Returns
/// 返回值
/// A retryable transport failure only for a pre-semantic truncated stream; malformed protocol
/// payloads and post-semantic truncation remain terminal.
/// 仅语义输出前的截断流返回可重试传输失败；畸形协议载荷与语义输出后的截断仍保持终态。
fn stream_completion_failure(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    if error.is_truncated_stream() {
        return stream_transport_failure(error.message, semantic_output_observed);
    }
    stream_protocol_failure(error, semantic_output_observed)
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

/// Classifies one HTTP request-send failure without guessing whether an established request arrived upstream.
/// 对一次 HTTP 请求发送失败进行分类，且不猜测已建立连接的请求是否到达上游。
///
/// # Parameters
/// 参数
/// `error` is the exact reqwest send error and `message` is the controlled diagnostic.
/// `error` 是精确的 reqwest 发送错误，`message` 是受控诊断信息。
///
/// # Returns
/// 返回值
/// A retryable transport failure marked not dispatched only when reqwest proves connection
/// establishment failed; every other send failure keeps the conservative unknown outcome.
/// 仅当 reqwest 证明连接建立失败时返回标记为未分派的可重试传输失败；其他发送失败均保守保持未知结果。
pub(crate) fn http_send_transport_failure(
    error: &reqwest::Error,
    message: impl Into<String>,
) -> ProviderExecutionFailure {
    // failure starts with the conservative unknown dispatch outcome used for ambiguous send errors.
    // failure 初始采用用于模糊发送错误的保守未知分派结果。
    let mut failure = transport_failure(message);
    if error.is_connect() {
        // A DNS, TCP, or TLS connection-establishment failure occurs before an HTTP request can be dispatched.
        // DNS、TCP 或 TLS 连接建立失败发生在 HTTP 请求可被分派之前。
        failure.error.dispatch_outcome = DispatchOutcome::NotDispatched;
    }
    failure
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

/// Reads one standalone Responses compact body within the official complete-request deadline.
/// 在官方完整请求截止时间内读取一份独立 Responses 压缩正文。
///
/// # Parameters
/// 参数
/// `response`, `max_bytes`, `url`, `status`, and `retry_after` retain the normal bounded body
/// contract; `request_started_at` and `request_timeout` cover the entire dispatched unary request.
/// `response`、`max_bytes`、`url`、`status` 与 `retry_after` 保留常规有界正文契约；
/// `request_started_at` 与 `request_timeout` 覆盖完整的已分派一元请求。
///
/// # Returns
/// 返回值
/// The complete bounded body, or a structured transport timeout/failure.
/// 返回完整的有界正文，或结构化传输超时/失败。
async fn read_responses_compact_body_with_timeout(
    response: reqwest::Response,
    max_bytes: usize,
    url: &str,
    status: u16,
    retry_after: Option<RetryAfterHint>,
    request_started_at: Instant,
    request_timeout: Duration,
) -> Result<String, ProviderExecutionFailure> {
    // remaining prevents a successful header response from resetting the official unary deadline.
    // remaining 避免成功收到响应头后重新计算官方一元请求截止时间。
    let remaining = request_timeout.saturating_sub(request_started_at.elapsed());
    // result separates a complete body, a bounded body-read failure, and the complete deadline.
    // result 区分完整正文、有界正文读取失败与完整请求截止超时。
    let result = tokio::time::timeout(
        remaining,
        read_bounded_response_text(response, max_bytes, url, status, retry_after),
    )
    .await;
    match result {
        Ok(Ok(body)) => Ok(body),
        Ok(Err(mut failure)) => {
            if failure.error.category == AiErrorCategory::ProviderUnavailable {
                failure.error.stage = String::from("response_compact");
                failure.error.dispatch_outcome = DispatchOutcome::Dispatched;
            }
            Err(failure)
        }
        Err(_) => Err(compact_transport_failure(format!(
            "Provider compaction request to `{url}` exceeded the registered {} ms complete-response timeout.",
            request_timeout.as_millis()
        ))),
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

/// Recognizes the official Codex overload message in either JSON or plain-text responses.
/// 识别 JSON 或纯文本响应中的官方 Codex 过载消息。
fn is_known_overload_message(message: &str) -> bool {
    message
        .to_ascii_lowercase()
        .contains("our servers are currently overloaded. please try again later")
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
    // upstream_message keeps plain-text provider bodies available to the shared retry classifier.
    // upstream_message 保留纯文本供应商响应，供共享重试分类器继续识别。
    let upstream_message = error
        .and_then(|value| value.get("message").and_then(Value::as_str))
        .map(str::to_string)
        .or_else(|| (!body.trim().is_empty()).then(|| body.trim().to_string()));
    // Official Codex overload codes remain authoritative when a proxy reports a generic error type.
    // 当代理返回通用错误类型时，官方 Codex 过载代码仍作为权威分类依据。
    let known_overload = matches!(
        upstream_code.as_deref(),
        Some("server_is_overloaded" | "slow_down")
    ) || upstream_message
        .as_deref()
        .is_some_and(is_known_overload_message);
    let category = if known_overload {
        AiErrorCategory::ProviderOverloaded
    } else {
        match error_type {
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
        }
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
            upstream_message: upstream_message.clone(),
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
            message: upstream_message.unwrap_or_else(|| {
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
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::Duration;
    use vulcan_ai_contract::{
        AiContinuation, AiMessage, AiOperation, ContentItem, MessageRole, StructuredOutputPolicy,
    };
    use vulcan_ai_protocols::{
        ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
    };

    /// Verifies only a proven connection-establishment failure is safe to replay.
    /// 验证只有已证明的连接建立失败才可安全重放。
    #[tokio::test]
    async fn http_send_failure_uses_reqwest_connection_evidence() {
        // listener reserves one loopback port so the subsequent closed-port connection fails deterministically.
        // listener 预留一个回环端口，使随后对已关闭端口的连接确定性失败。
        let listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener should bind");
        // port is the exact operating-system-assigned loopback port.
        // port 是操作系统精确分配的回环端口。
        let port = listener
            .local_addr()
            .expect("loopback address should resolve")
            .port();
        drop(listener);

        // connect_error is produced before an HTTP connection can be established.
        // connect_error 产生于 HTTP 连接建立之前。
        let connect_error = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/responses"))
            .send()
            .await
            .expect_err("closed loopback port should reject the connection");
        assert!(connect_error.is_connect());
        // connect_failure is therefore explicitly safe for structured retry.
        // connect_failure 因而可明确安全地进入结构化重试。
        let connect_failure = http_send_transport_failure(&connect_error, "connect failed");
        assert_eq!(
            connect_failure.error.dispatch_outcome,
            DispatchOutcome::NotDispatched
        );
        assert!(connect_failure.error.retryable);

        // builder_error proves that unrelated request-send failures retain the conservative outcome.
        // builder_error 证明无关的请求发送失败会保留保守结果。
        let builder_error = reqwest::Client::new()
            .post("not a valid provider URL")
            .send()
            .await
            .expect_err("invalid URL should fail before transport");
        assert!(!builder_error.is_connect());
        // builder_failure must not be promoted to replay-safe without connection evidence.
        // builder_failure 在缺少连接证据时不得被提升为可安全重放。
        let builder_failure = http_send_transport_failure(&builder_error, "builder failed");
        assert_eq!(
            builder_failure.error.dispatch_outcome,
            DispatchOutcome::Unknown
        );
    }

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

    /// Verifies the official Codex overload code overrides a generic HTTP proxy error type.
    /// 验证官方 Codex 过载代码会覆盖 HTTP 代理返回的通用错误类型。
    #[test]
    fn http_server_overloaded_code_is_retryable() {
        // failure models the structured 503 body used by official Codex and compatible proxies.
        // failure 模拟官方 Codex 与兼容代理使用的结构化 503 正文。
        let failure = http_failure(
            503,
            None,
            r#"{"error":{"type":"server","code":"server_is_overloaded","message":"Our servers are currently overloaded. Please try again later."}}"#,
        );

        assert_eq!(failure.error.category, AiErrorCategory::ProviderOverloaded);
        assert!(failure.error.retryable);
        assert_eq!(
            failure.error.upstream_code.as_deref(),
            Some("server_is_overloaded")
        );
    }

    /// Verifies the official overload message remains retryable when a proxy returns plain text.
    /// 验证代理返回纯文本时，官方过载消息仍会被归入可重试类别。
    #[test]
    fn http_server_overloaded_plain_text_is_retryable() {
        let failure = http_failure(
            400,
            None,
            "Our servers are currently overloaded. Please try again later.",
        );

        assert_eq!(failure.error.category, AiErrorCategory::ProviderOverloaded);
        assert!(failure.error.retryable);
        assert_eq!(
            failure.error.upstream_message.as_deref(),
            Some("Our servers are currently overloaded. Please try again later.")
        );
    }

    /// Verifies provider-controlled shape labels are bounded and cannot inject trace syntax.
    /// 验证供应商控制的形状标签有界且无法注入追踪语法。
    #[test]
    fn compact_shape_trace_labels_are_bounded_and_sanitized() {
        // normalized removes whitespace and punctuation while preserving safe structural characters.
        // normalized 移除空白与标点，同时保留安全结构字符。
        let normalized = bounded_trace_label("compaction\r\nsecret:value/field");
        assert_eq!(normalized, "compaction__secret_value_field");

        // bounded caps provider-controlled labels at the documented maximum length.
        // bounded 把供应商控制的标签限制在文档规定的最大长度内。
        let bounded = bounded_trace_label("a".repeat(80).as_str());
        assert_eq!(bounded.len(), 64);
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
                provider_context_items: Vec::new(),
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

    /// Verifies the compact request body follows the official Codex `CompactionInput` shape.
    /// 验证 compact 请求体遵循官方 Codex `CompactionInput` 结构。
    #[test]
    fn codex_compact_body_removes_stream_only_and_unknown_fields() {
        let mut body = json!({
            "model": "gpt-codex",
            "input": [],
            "instructions": "",
            "stream": true,
            "store": false,
            "include": ["reasoning.encrypted_content"],
            "previous_response_id": "resp-old",
            "unsupported_vendor_field": true,
            "parallel_tool_calls": true,
            "tools": [{
                "type": "function",
                "name": "inspect_workspace",
                "parameters": {"type": "object", "properties": {}}
            }],
        });

        prepare_responses_compact_body(&mut body).expect("object compact body should normalize");

        assert_eq!(body["model"], "gpt-codex");
        assert!(body["input"].is_array());
        assert!(body.get("instructions").is_none());
        assert_eq!(body["parallel_tool_calls"], true);
        assert_eq!(body["tools"][0]["name"], "inspect_workspace");
        assert!(body.get("stream").is_none());
        assert!(body.get("store").is_none());
        assert!(body.get("include").is_none());
        assert!(body.get("previous_response_id").is_none());
        assert!(body.get("unsupported_vendor_field").is_none());
    }

    /// Verifies compact requests retain non-empty ordinary-turn base instructions.
    /// 验证压缩请求会保留非空的普通回合基础指令。
    #[test]
    fn codex_compact_body_preserves_nonempty_base_instructions() {
        let mut body = json!({
            "model": "gpt-codex",
            "input": [],
            "instructions": "ordinary Codex base instructions",
            "stream": true,
        });

        prepare_responses_compact_body(&mut body).expect("object compact body should normalize");

        assert_eq!(
            body["instructions"],
            Value::String(String::from("ordinary Codex base instructions"))
        );
        assert!(body.get("stream").is_none());
    }

    /// Verifies compact responses preserve metadata and reject output without an opaque checkpoint.
    /// 验证 compact 响应保留元数据，并拒绝不含不透明检查点的输出。
    #[test]
    fn codex_compact_response_requires_a_compaction_item() {
        let valid = decode_responses_compact_result(
            &json!({
                "id": "resp-compact",
                "usage": {"input_tokens": 4},
                "output": [{"type": "compaction_summary", "encrypted_content": "opaque"}]
            }),
            "https://example.test/responses/compact",
        )
        .expect("compaction output should decode");
        assert_eq!(valid.response_id.as_deref(), Some("resp-compact"));
        assert_eq!(valid.usage, Some(json!({"input_tokens": 4})));
        assert_eq!(valid.output_items[0]["type"], "compaction");

        let failure = decode_responses_compact_result(
            &json!({"output": [{"type": "message", "content": []}]}),
            "https://example.test/responses/compact",
        )
        .expect_err("message-only output must not become a checkpoint");
        assert!(failure.error.message.contains("replayable output items"));
        assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);

        // empty_checkpoint proves a typed compaction marker still needs a usable opaque payload.
        // empty_checkpoint 证明带类型的压缩标记仍然必须拥有可用的不透明载荷。
        let empty_checkpoint = decode_responses_compact_result(
            &json!({"output": [{"type": "compaction", "encrypted_content": ""}]}),
            "https://example.test/responses/compact",
        )
        .expect_err("empty opaque checkpoints must be rejected before persistence");
        assert!(
            empty_checkpoint
                .error
                .message
                .contains("replayable output items")
        );

        let mixed = decode_responses_compact_result(
            &json!({
                "output": [
                    {"type": "compaction", "encrypted_content": "opaque"},
                    {"type": "message", "content": []}
                ]
            }),
            "https://example.test/responses/compact",
        )
        .expect("ordered replayable output items should be preserved");
        assert_eq!(mixed.output_items.len(), 2);
        let unknown_failure = decode_responses_compact_result(
            &json!({
                "output": [
                    {"type": "compaction", "encrypted_content": "opaque"},
                    {"type": "provider_private_item"}
                ]
            }),
            "https://example.test/responses/compact",
        )
        .expect_err("unknown output items must not become a checkpoint");
        assert!(
            unknown_failure
                .error
                .message
                .contains("replayable output items")
        );
    }

    /// Verifies HTTP(S) routes map to the exact WebSocket scheme and reject unsupported schemes.
    /// 验证 HTTP(S) 路由映射到精确 WebSocket Scheme，并拒绝不支持的 Scheme。
    #[test]
    fn codex_websocket_url_conversion_is_strict() {
        assert_eq!(
            websocket_url_from_http("https://example.test/v1/responses")
                .expect("HTTPS route should map to WSS"),
            "wss://example.test/v1/responses"
        );
        assert_eq!(
            websocket_url_from_http("http://localhost:8317/v1/responses")
                .expect("HTTP route should map to WS"),
            "ws://localhost:8317/v1/responses"
        );
        assert!(websocket_url_from_http("ftp://example.test/responses").is_err());
    }

    /// Verifies handshake authentication failures cannot be hidden by HTTP fallback.
    /// 验证握手鉴权失败不能被 HTTP 回退静默隐藏。
    #[test]
    fn codex_websocket_handshake_auth_failure_is_terminal() {
        let failure = websocket_handshake_http_failure(401, "unauthorized");
        assert_eq!(failure.error.category, AiErrorCategory::Authentication);
        assert_eq!(failure.error.http_status, Some(401));
        assert!(!failure.error.retryable);
        assert_eq!(
            failure.error.dispatch_outcome,
            DispatchOutcome::NotDispatched
        );
    }

    /// Verifies unsupported handshake statuses remain eligible for pre-semantic HTTP fallback.
    /// 验证不支持的握手状态在产生语义输出前仍可安全回退 HTTP。
    #[test]
    fn codex_websocket_handshake_unsupported_status_allows_fallback() {
        let failure = websocket_handshake_http_failure(426, "upgrade required");
        assert_eq!(failure.error.category, AiErrorCategory::ProviderUnavailable);
        assert_eq!(
            failure.error.subcategory.as_deref(),
            Some("websocket_unsupported")
        );
        assert_eq!(failure.error.http_status, Some(426));
        assert!(failure.error.retryable);
        assert_eq!(
            failure.error.dispatch_outcome,
            DispatchOutcome::NotDispatched
        );
    }

    /// Verifies HTTP Codex Responses never sends the WebSocket-only continuation cursor.
    /// 验证 HTTP Codex Responses 绝不会发送仅属于 WebSocket 的延续游标。
    #[test]
    fn codex_http_request_uses_full_history_without_previous_response_id() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.selected_protocol = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.verified_protocols =
            BTreeSet::from([String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID)]);
        let mut request = minimal_request();
        request.continuation = Some(AiContinuation {
            provider_id: route.provider.id.clone(),
            protocol_profile_id: String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID),
            model_id: request.model_id.clone(),
            provider_message_id: String::from("resp-previous"),
            covered_local_message_id: String::from("local-boundary"),
            covered_message_count: 0,
        });

        let websocket_encoded = client
            .encode_request(&route, &request)
            .expect("WebSocket request should retain the proven cursor");
        let http_request = prepare_http_execution_request(&route, request);
        let http_encoded = client
            .encode_request(&route, &http_request)
            .expect("HTTP request should encode with full history");

        assert_eq!(
            websocket_encoded.body["previous_response_id"],
            "resp-previous"
        );
        assert!(http_encoded.body.get("previous_response_id").is_none());
        assert_eq!(
            http_encoded.body["input"]
                .as_array()
                .expect("HTTP input should remain an array")
                .len(),
            1
        );
    }

    /// Verifies the WebSocket body preserves official HTTP controls and adds its response.create envelope.
    /// 验证 WebSocket 正文保留官方 HTTP 控制字段并增加 response.create 信封。
    #[test]
    fn codex_websocket_body_preserves_official_controls_and_projects_v2_envelope() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.selected_protocol = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.verified_protocols =
            BTreeSet::from([String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID)]);
        route.protocol_route.responses_features = Some(crate::ResponsesRouteFeatures {
            websocket: crate::ResponsesWebSocketPolicy::PreferredWithHttpFallback,
            remote_compaction: crate::ResponsesRemoteCompactionPolicy::Disabled,
        });
        let mut request = minimal_request();
        request.cache_scope_id = Some(String::from("session-stable"));
        let http_shaped = client
            .encode_request(&route, &request)
            .expect("Codex Responses body should encode");
        let websocket_body = client
            .encode_request_for_selected_transport(&route, &request)
            .expect("selected WebSocket transport should encode")
            .body;

        assert_eq!(websocket_body["type"], "response.create");
        assert_eq!(websocket_body["tool_choice"], "auto");
        assert_eq!(websocket_body["parallel_tool_calls"], true);
        assert_eq!(
            websocket_body["include"],
            json!(["reasoning.encrypted_content"])
        );
        assert_eq!(
            websocket_body["client_metadata"]["session_id"],
            "session-stable"
        );
        assert_eq!(
            websocket_body["client_metadata"]["thread_id"],
            "session-stable"
        );
        assert!(http_shaped.body.get("type").is_none());
        assert_eq!(http_shaped.body["tool_choice"], "auto");
        assert_eq!(http_shaped.body["parallel_tool_calls"], true);
        assert_eq!(
            http_shaped.body["include"],
            json!(["reasoning.encrypted_content"])
        );
    }

    /// Verifies a compact body transport failure preserves its proven dispatched boundary.
    /// 验证压缩正文传输失败会保留已经证明的已分派边界。
    #[test]
    fn responses_compact_transport_failure_is_dispatched_and_retryable() {
        let failure = compact_transport_failure("compact body timed out");

        assert_eq!(failure.error.stage, "response_compact");
        assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);
        assert!(failure.error.retryable);
        assert!(!failure.error.semantic_output_observed);
        assert!(!failure.error.side_effect_observed);
    }

    /// Verifies a safe WS fallback is remembered only for the exact provider route, connection, and session.
    /// 验证安全 WS 回退只会被精确供应商路由、连接与会话记住。
    #[test]
    fn codex_websocket_http_fallback_is_session_scoped() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.selected_protocol = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.verified_protocols =
            BTreeSet::from([String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID)]);
        route.protocol_route.responses_features = Some(crate::ResponsesRouteFeatures {
            websocket: crate::ResponsesWebSocketPolicy::PreferredWithHttpFallback,
            remote_compaction: crate::ResponsesRemoteCompactionPolicy::Disabled,
        });
        let mut first_session = minimal_request();
        first_session.cache_scope_id = Some(String::from("session-a"));
        let mut second_session = first_session.clone();
        second_session.cache_scope_id = Some(String::from("session-b"));

        assert!(!client.responses_websocket_http_fallback_active(&route, &first_session));
        assert_eq!(
            client
                .encode_request_for_selected_transport(&route, &first_session)
                .expect("first request should use the selected WebSocket shape")
                .body["type"],
            "response.create"
        );
        client.remember_responses_websocket_http_fallback(&route, &first_session);

        assert!(client.responses_websocket_http_fallback_active(&route, &first_session));
        assert!(
            client
                .encode_request_for_selected_transport(&route, &first_session)
                .expect("remembered fallback should use the HTTP shape")
                .body
                .get("type")
                .is_none()
        );
        assert!(!client.responses_websocket_http_fallback_active(&route, &second_session));
        // changedRoute simulates an edited custom-provider endpoint under the same stable identities.
        // changedRoute 模拟稳定身份不变但自定义供应商端点已被编辑的情况。
        let mut changed_route = route.clone();
        changed_route.entry.base_url = String::from("https://changed.example/v1");
        assert!(!client.responses_websocket_http_fallback_active(&changed_route, &first_session));
    }

    /// Verifies safe WebSocket failures exhaust the exact configured retry budget and reset after health.
    /// 验证安全 WebSocket 失败会耗尽精确配置的重试预算，并在恢复健康后重置。
    #[test]
    fn codex_websocket_safe_failures_respect_and_reset_retry_budget() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.selected_protocol = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.verified_protocols =
            BTreeSet::from([String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID)]);
        let mut request = minimal_request();
        request.cache_scope_id = Some(String::from("session-retry-budget"));

        assert!(!client.record_responses_websocket_safe_failure(&route, &request, 2));
        assert!(!client.record_responses_websocket_safe_failure(&route, &request, 2));
        assert!(client.record_responses_websocket_safe_failure(&route, &request, 2));

        client.clear_responses_websocket_safe_failures(&route, &request);
        assert!(!client.record_responses_websocket_safe_failure(&route, &request, 2));

        client.remember_responses_websocket_http_fallback(&route, &request);
        assert!(client.responses_websocket_http_fallback_active(&route, &request));
        assert!(
            client
                .responses_websocket_safe_failure_scopes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
    }

    /// Verifies an unsupported WebSocket handshake switches to HTTP without consuming transient retries.
    /// 验证不支持的 WebSocket 握手会直接切换 HTTP，而不会消耗瞬时重试次数。
    #[test]
    fn codex_websocket_unsupported_handshake_falls_back_immediately() {
        let unsupported = websocket_handshake_http_failure(
            426,
            "provider does not support the Responses WebSocket upgrade",
        );
        assert_eq!(
            unsupported.error.subcategory.as_deref(),
            Some("websocket_unsupported")
        );
        assert!(websocket_failure_allows_http_fallback(&unsupported));
        assert!(websocket_failure_requires_immediate_http_fallback(
            &unsupported
        ));

        let account_pool = stream_transport_failure(
            "Provider WebSocket stream closed with reason `no available account`",
            false,
        );
        let mut account_pool = account_pool;
        account_pool.error.subcategory = Some(String::from("websocket_no_available_account"));
        assert!(websocket_failure_requires_immediate_http_fallback(
            &account_pool
        ));

        let transient = websocket_handshake_failure("temporary handshake failure");
        assert!(websocket_failure_allows_http_fallback(&transient));
        assert!(!websocket_failure_requires_immediate_http_fallback(
            &transient
        ));
    }

    /// Verifies every Responses WebSocket v2 handshake carries the official beta capability value.
    /// 验证每次 Responses WebSocket v2 握手都携带官方 Beta 能力值。
    #[test]
    fn codex_websocket_handshake_installs_v2_beta_header() {
        let mut headers = tokio_tungstenite::tungstenite::http::HeaderMap::new();
        headers.insert(
            WebSocketHeaderName::from_static(OPENAI_RESPONSES_WEBSOCKET_BETA_HEADER_NAME),
            WebSocketHeaderValue::from_static("stale-value"),
        );

        insert_responses_websocket_v2_beta_header(&mut headers);

        assert_eq!(
            headers
                .get(OPENAI_RESPONSES_WEBSOCKET_BETA_HEADER_NAME)
                .and_then(|value| value.to_str().ok()),
            Some(OPENAI_RESPONSES_WEBSOCKET_BETA_HEADER_VALUE)
        );
    }

    /// Verifies one complete local WebSocket transport round trip preserves the official envelope and semantic lifecycle.
    /// 验证一次完整的本地 WebSocket 传输往返会保留官方信封与语义生命周期。
    #[tokio::test]
    #[allow(clippy::result_large_err)]
    async fn codex_websocket_transport_completes_semantic_round_trip() {
        // listener owns one exact loopback WebSocket endpoint for the transport under test.
        // listener 为被测传输持有一个精确的回环 WebSocket 端点。
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("loopback WebSocket listener should bind");
        // address is the operating-system-assigned endpoint projected into the provider route.
        // address 是投影到供应商路由中的操作系统分配端点。
        let address = listener
            .local_addr()
            .expect("loopback WebSocket address should resolve");
        // server validates the handshake and emits one official semantic Responses lifecycle.
        // server 校验握手并发送一组官方语义 Responses 生命周期事件。
        let server = tokio::spawn(async move {
            // transport is the accepted TCP connection that must be upgraded exactly once.
            // transport 是必须精确升级一次的已接受 TCP 连接。
            let (transport, _) = listener
                .accept()
                .await
                .expect("WebSocket client should connect");
            // websocket validates the official beta header during the upgrade callback.
            // websocket 在升级回调期间校验官方 Beta 请求头。
            let mut websocket = tokio_tungstenite::accept_hdr_async(
                transport,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                 response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                    assert_eq!(
                        request
                            .headers()
                            .get(OPENAI_RESPONSES_WEBSOCKET_BETA_HEADER_NAME)
                            .and_then(|value| value.to_str().ok()),
                        Some(OPENAI_RESPONSES_WEBSOCKET_BETA_HEADER_VALUE)
                    );
                    Ok(response)
                },
            )
            .await
            .expect("WebSocket handshake should complete");
            // request_message is the single response.create command sent by the client.
            // request_message 是客户端发送的唯一 response.create 命令。
            let request_message = websocket
                .next()
                .await
                .expect("WebSocket request should arrive")
                .expect("WebSocket request should be readable");
            // request_body is the exact JSON envelope observed on the wire.
            // request_body 是在线路上观察到的精确 JSON 信封。
            let request_body: Value = serde_json::from_str(
                request_message
                    .to_text()
                    .expect("WebSocket request should be text"),
            )
            .expect("WebSocket request should be valid JSON");
            assert_eq!(request_body["type"], "response.create");
            assert_eq!(request_body["model"], "openai/gpt-4.1");
            assert_eq!(
                request_body["client_metadata"]["session_id"],
                "ws-round-trip"
            );
            assert_eq!(
                request_body["client_metadata"]["thread_id"],
                "ws-round-trip"
            );

            // response_events are official JSON events consumed by the same decoder as HTTP SSE.
            // response_events 是由 HTTP SSE 共用解码器消费的官方 JSON 事件。
            let response_events = [
                json!({
                    "type": "response.created",
                    "response": {"id": "resp-ws-round-trip"}
                }),
                json!({
                    "type": "response.output_text.delta",
                    "output_index": 0,
                    "item_id": "msg-ws-round-trip",
                    "delta": "WS_SEMANTIC_OK"
                }),
                json!({
                    "type": "response.output_item.done",
                    "output_index": 0,
                    "item": {
                        "id": "msg-ws-round-trip",
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": "WS_SEMANTIC_OK"}]
                    }
                }),
                json!({
                    "type": "response.completed",
                    "response": {
                        "id": "resp-ws-round-trip",
                        "usage": {"input_tokens": 4, "output_tokens": 2, "total_tokens": 6}
                    }
                }),
            ];
            for response_event in response_events {
                websocket
                    .send(Message::text(response_event.to_string()))
                    .await
                    .expect("WebSocket response event should send");
            }
        });

        // client owns the production protocol registry and transport implementation.
        // client 持有生产协议注册表与传输实现。
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        // route is one explicit Codex Responses route pointed at the loopback server.
        // route 是一条指向回环服务器的显式 Codex Responses 路由。
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.entry.base_url = format!("http://{address}");
        route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.selected_protocol = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.verified_protocols =
            BTreeSet::from([String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID)]);
        route.protocol_route.responses_features = Some(crate::ResponsesRouteFeatures {
            websocket: crate::ResponsesWebSocketPolicy::PreferredWithHttpFallback,
            remote_compaction: crate::ResponsesRemoteCompactionPolicy::Disabled,
        });
        // request supplies the stable task identity required by the official WebSocket metadata.
        // request 提供官方 WebSocket 元数据要求的稳定任务身份。
        let mut request = minimal_request();
        request.cache_scope_id = Some(String::from("ws-round-trip"));
        // secret projects the same bearer credential shape used by production custom providers.
        // secret 投影生产自定义供应商使用的同一 Bearer 凭据形状。
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("test-secret"))
            .expect("credential envelope should validate")
            .execution_secret()
            .expect("execution secret should resolve");
        // stream is the production WebSocket-selected normalized execution stream.
        // stream 是生产环境选择 WebSocket 后的规范化执行流。
        let mut stream = client.execute(
            route,
            request,
            secret,
            String::from("ws-round-trip-execution"),
        );
        // events retain the complete normalized semantic lifecycle for assertions.
        // events 保留完整规范化语义生命周期以供断言。
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.expect("WebSocket semantic stream should succeed"));
        }
        server.await.expect("WebSocket server task should finish");

        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseMetadataUpdated { continuation }
                if continuation.provider_message_id == "resp-ws-round-trip"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            AiStreamEvent::ResponseTextDelta { delta, .. }
                if delta == "WS_SEMANTIC_OK"
        )));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AiStreamEvent::ResponseCompleted))
        );
    }

    /// Verifies official wrapped WebSocket retry signals fall back while authentication remains terminal.
    /// 验证官方包装式 WebSocket 重试信号会触发回退，而鉴权错误仍保持终止。
    #[test]
    fn codex_websocket_wrapped_errors_preserve_fallback_safety() {
        // missing_response is the official stale-cursor signal used to request a complete replay.
        // missing_response 是用于请求完整重放的官方过期游标信号。
        let missing_response = websocket_wrapped_error_failure(&json!({
            "type": "error",
            "error": {
                "code": "previous_response_not_found",
                "message": "stale cursor"
            }
        }))
        .expect("official stale-cursor wrapper should become a transport failure");
        assert_eq!(
            missing_response.error.subcategory.as_deref(),
            Some(OPENAI_RESPONSES_PREVIOUS_RESPONSE_NOT_FOUND_CODE)
        );
        assert!(websocket_failure_allows_http_fallback(&missing_response));

        // authentication is an already-dispatched credential failure and must never be hidden by HTTP fallback.
        // authentication 是已经分派的凭据失败，绝不能被 HTTP 回退隐藏。
        let authentication = websocket_wrapped_error_failure(&json!({
            "type": "error",
            "status": 401,
            "error": {
                "type": "authentication_error",
                "message": "invalid token"
            }
        }))
        .expect("wrapped authentication status should become a typed failure");
        assert_eq!(
            authentication.error.category,
            AiErrorCategory::Authentication
        );
        assert!(!websocket_failure_allows_http_fallback(&authentication));

        // generic_error remains a public terminal Responses event for the shared decoder.
        // generic_error 仍是交给共享解码器处理的公共 Responses 终态事件。
        let generic_error = websocket_wrapped_error_failure(&json!({
            "type": "error",
            "error": {"code": "invalid_request_error", "message": "bad input"}
        }));
        assert!(generic_error.is_none());
    }

    /// Verifies a dispatched terminal gap can fall back only before semantic output exists.
    /// 验证已分派请求的终态缺口仅能在尚无语义输出时回退。
    #[test]
    fn codex_websocket_terminal_gap_fallback_respects_semantic_boundary() {
        let safe = stream_transport_failure("stream ended before terminal", false);
        assert_eq!(safe.error.dispatch_outcome, DispatchOutcome::Dispatched);
        assert!(websocket_failure_allows_http_fallback(&safe));

        let unsafe_failure = stream_transport_failure("stream ended after output", true);
        assert!(!websocket_failure_allows_http_fallback(&unsafe_failure));
    }

    /// Verifies HTTP decoder completion distinguishes transient truncation from malformed payloads.
    /// 验证 HTTP 解码器结束阶段会区分瞬态截断与畸形载荷。
    #[test]
    fn responses_http_completion_failure_preserves_retry_safety() {
        let truncated = stream_completion_failure(
            ProtocolConversionError::truncated_stream("stream ended before terminal"),
            false,
        );
        assert_eq!(
            truncated.error.category,
            AiErrorCategory::ProviderUnavailable
        );
        assert_eq!(
            truncated.error.dispatch_outcome,
            DispatchOutcome::Dispatched
        );
        assert!(truncated.error.retryable);

        let post_semantic_truncation = stream_completion_failure(
            ProtocolConversionError::truncated_stream("stream ended after output"),
            true,
        );
        assert!(!post_semantic_truncation.error.retryable);
        assert!(post_semantic_truncation.error.semantic_output_observed);

        let malformed = stream_completion_failure(
            ProtocolConversionError::new("invalid Responses payload"),
            false,
        );
        assert_eq!(malformed.error.category, AiErrorCategory::InvalidRequest);
        assert_eq!(
            malformed.error.subcategory.as_deref(),
            Some("protocol_conversion")
        );
        assert!(!malformed.error.retryable);
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

    /// Verifies a configured reasoning projection reaches the final encoded request body.
    /// 验证已配置的思考投影会抵达最终编码请求体，而不是只停留在编辑器状态。
    #[test]
    fn encode_request_applies_custom_reasoning_projection_to_final_wire_body() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.custom_model_projection = Some(CustomModelRequestProjection {
            reasoning: BTreeMap::from([(
                ReasoningEffort::High,
                crate::CustomRequestMutation {
                    set: BTreeMap::from([(
                        String::from("thinking.type"),
                        Value::String(String::from("enabled")),
                    )]),
                    delete: vec![String::from("reasoning_effort")],
                },
            )]),
            ..Default::default()
        });
        let mut request = minimal_request();
        request.reasoning_effort = Some(ReasoningEffort::High);

        let encoded = client
            .encode_request(&route, &request)
            .expect("custom reasoning projection should encode");

        assert_eq!(encoded.body["thinking"]["type"], "enabled");
        assert!(encoded.body.get("reasoning_effort").is_none());
    }
}
