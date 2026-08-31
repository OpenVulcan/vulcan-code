//! Verified media preparation and shared provider-reference cache lifecycles.
//! 经验证的媒体准备与共享供应商引用缓存生命周期。

use super::{
    PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS, ProviderExecutionClient, ProviderExecutionRoute,
};
use crate::CredentialExecutionSecret;
use crate::media::{MediaUploadCancellation, MediaUploadProgressCallback};
use base64::Engine as _;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};
use vulcan_ai_contract::ResourceKind;

/// Host-wide preferred inline ceiling before an available provider upload channel is selected.
/// 存在 Provider 上传通道时，宿主优先内联所采用的统一字节上限。
pub const PREFERRED_INLINE_MEDIA_MAX_BYTES: u64 = 512 * 1024;

/// Maximum process-local prepared media references retained across all providers and accounts.
/// 全部供应商与账号合计允许保留的最大进程内媒体准备引用数。
const PREPARED_MEDIA_CACHE_MAX_ENTRIES: usize = 256;

/// Stable provider-neutral operation identity for pre-dispatch conversation media materialization.
/// 分派前对话媒体物化使用的稳定供应商无关 operation 身份。
pub const CONVERSATION_MEDIA_INPUT_OPERATION_ID: &str = "conversation.media_input";

/// Stores one temporary provider media reference until the matching request is dispatched.
/// 在匹配请求分派前保存一个临时供应商媒体引用。
#[derive(Clone)]
pub(super) struct PreparedMediaCacheEntry {
    /// Provider-native reference used by the exact request projector.
    /// 精确请求投影器使用的供应商原生引用。
    provider_reference: String,
    /// Closed encoding that determines whether the reference is a file id or URI-shaped value.
    /// 决定该引用是文件标识还是 URI 形值的封闭编码。
    reference_encoding: crate::UploadReferenceEncoding,
    /// Optional monotonic expiry; durable conversation references intentionally have no process expiry.
    /// 可选的单调时钟过期点；持久会话引用有意不设置进程过期时间。
    expires_at: Option<Instant>,
    /// Creation time provides deterministic oldest-first capacity eviction.
    /// 创建时间用于确定性的最旧优先容量淘汰。
    created_at: Instant,
    /// Wall-clock creation time is persisted when a cache hit becomes a durable ledger entry.
    /// 当缓存命中转为持久账本条目时，使用墙钟创建时间保持真实供应商 TTL。
    created_at_unix_millis: u64,
    /// Wall-clock expiry time prevents a replay from extending the provider-issued lifetime.
    /// 墙钟过期时间防止重放延长供应商签发引用的有效期。
    expires_at_unix_millis: u64,
    /// Source metadata prevents a rewritten file from reusing an older provider reference.
    /// 源文件元数据防止被改写的文件复用旧供应商引用。
    source_fingerprint: PreparedMediaSourceFingerprint,
}

/// Carries one prepared media outcome together with the exact verified source revision.
/// 携带一次媒体准备结果及其经过精确验证的源文件修订。
#[derive(Debug)]
pub struct PreparedConversationMedia {
    /// Immutable provider-native or caller-requested host-inline reference created from verified bytes.
    /// 从已验证字节创建的不可变供应商原生或调用方请求的宿主内联引用。
    prepared_reference: Option<String>,
    /// Content-addressed source revision verified before and after materialization.
    /// 在物化前后验证过的内容寻址源文件修订。
    source_fingerprint: PreparedMediaSourceFingerprint,
}

impl PreparedConversationMedia {
    /// Returns the immutable reference created from the verified source bytes.
    /// 返回从已验证源字节创建的不可变引用。
    ///
    /// # Returns
    /// 返回值
    /// A provider-native URI for upload routes, a requested Base64 Data URL for host-inline
    /// routes, or `None` when the caller requested validation-only inline preparation.
    /// 上传路由返回供应商原生 URI；请求内联保留时返回 Base64 Data URL；调用方仅请求内联校验时返回 `None`。
    pub fn prepared_reference(&self) -> Option<&str> {
        self.prepared_reference.as_deref()
    }

    /// Returns the public content identity of the verified source bytes.
    /// 返回已验证源字节的公开内容身份。
    pub fn source_identity(&self) -> PreparedMediaSourceIdentity {
        PreparedMediaSourceIdentity {
            byte_len: self.source_fingerprint.byte_len,
            content_sha256: hex::encode(self.source_fingerprint.content_sha256),
        }
    }
}

/// Carries the provider-neutral source identity needed by the durable media ledger.
/// 携带可持久化媒体账本所需的供应商无关源身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedMediaSourceIdentity {
    /// Exact source byte length covered by the digest.
    /// 摘要覆盖的精确源字节长度。
    pub byte_len: u64,
    /// Lowercase hexadecimal SHA-256 digest of the exact source bytes.
    /// 精确源字节的小写十六进制 SHA-256 摘要。
    pub content_sha256: String,
}

/// Describes one cached provider reference together with its original wall-clock lifetime.
/// 描述一条缓存供应商引用及其原始墙钟生命周期。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedMediaReferenceSnapshot {
    /// Provider-native URI or URL used by the request projector.
    /// 请求投影器使用的供应商原生 URI 或 URL。
    pub provider_reference: String,
    /// Closed Manifest encoding required by the protocol resource projector.
    /// 协议资源投影器所需的封闭 Manifest 编码。
    pub reference_encoding: crate::UploadReferenceEncoding,
    /// Wall-clock time when the provider reference was cached after upload.
    /// 供应商引用上传后进入缓存的墙钟时间。
    pub created_at_unix_millis: u64,
    /// Wall-clock time after which the provider reference must not be reused.
    /// 供应商引用不得继续复用的墙钟过期时间。
    pub expires_at_unix_millis: u64,
}

/// Binds one conversation media reference to its session scope and provider-issued lifetime.
/// 把一条会话媒体引用绑定到会话作用域与供应商签发生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversationPreparedMediaRetention<'a> {
    /// Required owner identity for session-scoped upload channels.
    /// 会话作用域上传通道要求的所有者身份。
    pub session_scope_id: Option<&'a str>,
    /// Original provider wall-clock creation and expiry timestamps.
    /// 原始供应商墙钟创建与过期时间戳。
    pub wall_clock_lifetime: Option<(u64, u64)>,
}

/// Carries the immutable upload-contract identity required for reference reuse.
/// 携带引用复用所需的不可变上传契约身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationMediaUploadContract {
    /// Stable Manifest upload-channel identifier.
    /// 稳定的 Manifest 上传通道标识。
    pub upload_channel_id: String,
    /// Content fingerprint of the request-relevant upload semantics.
    /// 与请求相关的上传语义内容指纹。
    pub contract_fingerprint: String,
    /// Explicit reference reuse boundary declared by the channel.
    /// 通道声明的显式引用复用边界。
    pub reference_scope: crate::UploadReferenceScope,
    /// Provider-declared lifetime for the returned reference.
    /// 供应商声明的返回引用生命周期。
    pub cache_duration_ms: Option<u64>,
    /// Exact wire encoding used by references returned from this channel.
    /// 当前通道返回引用使用的精确 Wire 编码。
    pub reference_encoding: crate::UploadReferenceEncoding,
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
    /// SHA-256 digest of the exact source bytes.
    /// 精确源字节的 SHA-256 摘要。
    content_sha256: [u8; 32],
}

/// Borrows the exact route, upload contract, and source identity used to build one cache key.
/// 借用构建一条缓存键所需的精确路由、上传契约与源身份。
struct PreparedMediaCacheKeyInput<'a> {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    provider_id: &'a str,
    /// Stable service entry identifier.
    /// 稳定服务入口标识。
    service_entry_id: &'a str,
    /// Stable credential connection identifier.
    /// 稳定凭据连接标识。
    connection_id: &'a str,
    /// Exact upload-channel declaration.
    /// 精确上传通道声明。
    channel: &'a crate::UploadChannelDeclaration,
    /// Content-addressed source revision.
    /// 内容寻址源版本。
    source_fingerprint: &'a PreparedMediaSourceFingerprint,
    /// Closed resource kind.
    /// 封闭资源类型。
    kind: ResourceKind,
    /// Canonical source MIME type.
    /// 规范源 MIME 类型。
    mime_type: &'a str,
    /// Required owner identity for session-scoped references.
    /// 会话作用域引用要求的所有者身份。
    session_scope_id: Option<&'a str>,
}

/// Owns one exact cache insertion after source and lifetime validation.
/// 在源与生命周期校验后持有一次精确缓存写入。
struct PreparedMediaCacheWrite {
    /// Content-addressed cache key.
    /// 内容寻址缓存键。
    key: String,
    /// Provider-native prepared reference.
    /// 供应商原生已准备引用。
    provider_reference: String,
    /// Exact wire encoding for the reference.
    /// 引用使用的精确 Wire 编码。
    reference_encoding: crate::UploadReferenceEncoding,
    /// Verified source revision.
    /// 已验证源版本。
    source_fingerprint: PreparedMediaSourceFingerprint,
    /// Declared monotonic cache lifetime.
    /// 声明的单调缓存生命周期。
    cache_duration_ms: u64,
    /// Optional provider-issued wall-clock lifetime.
    /// 可选供应商签发墙钟生命周期。
    wall_clock_lifetime: Option<(u64, u64)>,
    /// Whether the entry remains reusable until process exit.
    /// 是否让条目保持可复用直到进程退出。
    keep_until_process_exit: bool,
}

/// Carries one stable source read and the optional retained bytes used for inline projection.
/// 携带一次稳定源读取及用于内联投影的可选保留字节。
struct PreparedMediaSourceRead {
    /// Content-addressed identity of the exact bytes read.
    /// 已读取精确字节的内容寻址身份。
    fingerprint: PreparedMediaSourceFingerprint,
    /// Exact source bytes retained only when the caller must build a Data URL.
    /// 仅在调用方必须构建 Data URL 时保留的精确源字节。
    retained_bytes: Option<Vec<u8>>,
}

/// Streams and hashes the exact stable source revision used by one media reference.
/// 流式读取并哈希一个媒体引用所使用的精确稳定源文件修订。
///
/// # Parameters
/// 参数
/// `local_path` is the exact local file, `progress` observes monotonic bytes for the optional
/// pre-dispatch read, and `cancellation` cooperatively stops long media hashing.
/// `local_path` 是精确本地文件，`progress` 观察可选分派前读取的单调字节进度，
/// `cancellation` 协作终止长媒体哈希。
///
/// # Returns
/// 返回值
/// A content-addressed fingerprint only when metadata and byte length remain stable for the full
/// read, otherwise a controlled disappearance, mutation, I/O, or cancellation failure.
/// 仅当完整读取期间元数据与字节长度保持稳定时返回内容寻址指纹；否则返回受控的消失、
/// 变更、I/O 或取消失败。
async fn prepared_media_source_fingerprint(
    local_path: &Path,
    progress: Option<MediaUploadProgressCallback>,
    cancellation: Option<&MediaUploadCancellation>,
) -> Result<PreparedMediaSourceFingerprint, String> {
    read_prepared_media_source(local_path, progress, cancellation, false)
        .await
        .map(|source| source.fingerprint)
}

/// Streams one stable source revision while optionally retaining the exact bytes for inline use.
/// 流式读取一个稳定源修订，并可选保留用于内联处理的精确字节。
///
/// # Parameters
/// 参数
/// `local_path`, `progress`, and `cancellation` define the same bounded read contract as
/// `prepared_media_source_fingerprint`; `retain_bytes` enables request-owned byte retention.
/// `local_path`、`progress` 与 `cancellation` 定义和 `prepared_media_source_fingerprint`
/// 相同的有界读取契约；`retain_bytes` 启用请求自有字节保留。
///
/// # Returns
/// 返回值
/// A verified fingerprint and, only when requested, the exact bytes covered by that fingerprint.
/// 返回已验证指纹，并且仅在请求时返回该指纹覆盖的精确字节。
async fn read_prepared_media_source(
    local_path: &Path,
    progress: Option<MediaUploadProgressCallback>,
    cancellation: Option<&MediaUploadCancellation>,
    retain_bytes: bool,
) -> Result<PreparedMediaSourceRead, String> {
    let metadata = tokio::fs::metadata(local_path)
        .await
        .map_err(|error| format!("Media source metadata could not be read: {error}"))?;
    if !metadata.is_file() {
        return Err(String::from("Media source must be a file."));
    }
    let expected_byte_len = metadata.len();
    if expected_byte_len == 0 {
        return Err(String::from("Media source must be a non-empty file."));
    }
    let modified_at = metadata.modified().ok();
    let file = tokio::fs::File::open(local_path)
        .await
        .map_err(|error| format!("Media source could not be opened: {error}"))?;
    let mut source = tokio_util::io::ReaderStream::new(file);
    let mut content_hasher = Sha256::new();
    let mut processed = 0_u64;
    // retainedBytes owns exactly the chunks included in contentHasher when inline projection is required.
    // retainedBytes 在需要内联投影时精确持有纳入 contentHasher 的全部分块。
    let mut retained_bytes = retain_bytes.then(Vec::new);
    while let Some(chunk) = source.next().await {
        if cancellation.is_some_and(MediaUploadCancellation::is_cancelled) {
            return Err(String::from("media upload cancelled"));
        }
        let chunk = chunk.map_err(|error| format!("Media source could not be read: {error}"))?;
        processed = processed
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| String::from("Media source byte count overflowed."))?;
        if processed > expected_byte_len {
            return Err(String::from(
                "Media source changed while its content fingerprint was being read.",
            ));
        }
        content_hasher.update(chunk.as_ref());
        if let Some(retained_bytes) = retained_bytes.as_mut() {
            retained_bytes.extend_from_slice(chunk.as_ref());
        }
        if let Some(callback) = progress.as_ref() {
            callback(processed, Some(expected_byte_len));
        }
    }
    if processed != expected_byte_len {
        return Err(String::from(
            "Media source changed while its content fingerprint was being read.",
        ));
    }
    let current_metadata = tokio::fs::metadata(local_path)
        .await
        .map_err(|_| String::from("Media source disappeared during preparation."))?;
    if !current_metadata.is_file()
        || current_metadata.len() != expected_byte_len
        || current_metadata.modified().ok() != modified_at
    {
        return Err(String::from(
            "Media source changed during preparation; please retry the submission.",
        ));
    }
    Ok(PreparedMediaSourceRead {
        fingerprint: PreparedMediaSourceFingerprint {
            byte_len: expected_byte_len,
            modified_at,
            content_sha256: content_hasher.finalize().into(),
        },
        retained_bytes,
    })
}

/// Resolves the declaration that materializes one pre-dispatch conversation resource.
/// 解析物化一个分派前对话资源的声明。
///
/// # Parameters
/// 参数
/// `route` is the frozen route; `kind` and `mime_type` are the exact normalized resource facts.
/// `route` 是冻结路由；`kind` 与 `mime_type` 是精确归一化资源事实。
///
/// # Returns
/// 返回值
/// The selected channel, `None` for inline host materialization, or a closed reference failure.
/// 返回选中通道；宿主内联物化返回 `None`；引用错误返回封闭失败。
fn resolve_conversation_media_upload_channel<'a>(
    route: &'a ProviderExecutionRoute,
    kind: ResourceKind,
    mime_type: &str,
) -> Result<Option<&'a crate::UploadChannelDeclaration>, String> {
    let credential_profile = route
        .entry
        .credential_profiles
        .iter()
        .find(|profile| profile.id == route.connection.credential_profile_id)
        .ok_or_else(|| {
            format!(
                "Provider route credential profile `{}` is absent from service entry `{}`.",
                route.connection.credential_profile_id, route.entry.id
            )
        })?;
    let Some(constraint) =
        route
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| {
                constraint.resource_kind == kind && !constraint.upload_channel_refs.is_empty()
            })
    else {
        return Ok(None);
    };
    // Channel selection depends only on the frozen references, resource kind, and MIME allowlist;
    // byte and path checks remain inside the materializer before network dispatch.
    // 通道选择只依赖冻结引用、资源类别与 MIME 允许列表；字节与路径检查仍由物化器在网络
    // 分派前执行。
    let resource = crate::upload_engine::UploadResource {
        local_path: None,
        inline_bytes: None,
        mime_type: mime_type.to_string(),
        byte_len: 1,
        remote_url: None,
        item_count: 1,
    };
    crate::upload_engine::resolve_upload_channel(
        route.upload_channels.as_slice(),
        constraint.upload_channel_refs.as_slice(),
        &resource,
        credential_profile.credential_role.as_deref(),
    )
    .map(Some)
}

/// Builds the content fingerprint of the request-relevant upload semantics.
/// 构造与请求相关的上传语义内容指纹。
fn upload_channel_contract_fingerprint(
    channel: &crate::UploadChannelDeclaration,
) -> Option<String> {
    // contractMaterial excludes descriptive text, progress affordances, and cache lifetime.
    // contractMaterial 排除描述文本、进度能力与缓存生命周期。
    let contract_material = (
        channel.primitive,
        channel.signing_primitive,
        channel.endpoint_host.as_deref(),
        channel.endpoint_path.as_deref(),
        channel.response_host_suffixes.as_slice(),
        channel.remote_url_host_suffixes.as_slice(),
        channel.method.as_deref(),
        channel.credential_role.as_deref(),
        channel.file_field.as_deref(),
        channel.accepted_mime_types.as_slice(),
        channel.response_id_pointer.as_deref(),
        channel.response_url_pointer.as_deref(),
        channel.reference_template.as_deref(),
        &channel.fixed_fields,
        channel.reference_encoding,
        channel.reference_scope,
    );
    let bytes = serde_json::to_vec(&contract_material).ok()?;
    Some(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

/// Builds the process-local cache key for one exact provider media projection.
/// 构建一个精确供应商媒体投影的进程内缓存键。
fn prepared_media_key(
    route: &ProviderExecutionRoute,
    source_fingerprint: &PreparedMediaSourceFingerprint,
    kind: ResourceKind,
    mime_type: &str,
    session_scope_id: Option<&str>,
) -> Option<String> {
    let credential_profile = route
        .entry
        .credential_profiles
        .iter()
        .find(|profile| profile.id == route.connection.credential_profile_id)?;
    let source = crate::upload_engine::UploadResource {
        local_path: None,
        inline_bytes: None,
        mime_type: mime_type.to_string(),
        byte_len: source_fingerprint.byte_len,
        remote_url: None,
        item_count: 1,
    };
    let constraint = route
        .protocol_route
        .input_resource_constraints
        .iter()
        .find(|constraint| {
            constraint.resource_kind == kind && !constraint.upload_channel_refs.is_empty()
        })?;
    let channel = crate::upload_engine::resolve_upload_channel(
        route.upload_channels.as_slice(),
        constraint.upload_channel_refs.as_slice(),
        &source,
        credential_profile.credential_role.as_deref(),
    )
    .ok()?;
    prepared_media_cache_key(PreparedMediaCacheKeyInput {
        provider_id: route.provider.id.as_str(),
        service_entry_id: route.entry.id.as_str(),
        connection_id: route.connection.id.as_str(),
        channel,
        source_fingerprint,
        kind,
        mime_type,
        session_scope_id,
    })
}

/// Builds the shared cache key used by native conversation and provider-tool video uploads.
/// 构建原生会话与供应商工具视频上传共同使用的缓存键。
///
/// The key deliberately omits model, chat protocol, operation, manifest generation, and concrete
/// account unless the upload channel explicitly narrows its reuse scope. This is the user-defined
/// boundary: provider plus service/API entry plus upload-contract semantics.
/// 该键按设计省略模型、聊天协议、操作、Manifest 代际及具体账号，除非上传通道显式收窄复用
/// 作用域。这正是用户定义的边界：供应商、服务/API 入口与上传契约语义。
fn prepared_media_cache_key(input: PreparedMediaCacheKeyInput<'_>) -> Option<String> {
    let PreparedMediaCacheKeyInput {
        provider_id,
        service_entry_id,
        connection_id,
        channel,
        source_fingerprint,
        kind,
        mime_type,
        session_scope_id,
    } = input;
    let contract_fingerprint = upload_channel_contract_fingerprint(channel)?;
    let scope_identity = match channel.reference_scope {
        crate::UploadReferenceScope::ProviderProtocol => None,
        crate::UploadReferenceScope::Credential => Some(connection_id),
        crate::UploadReferenceScope::Session => {
            session_scope_id?;
            session_scope_id
        }
    };
    let material = serde_json::to_vec(&(
        provider_id,
        service_entry_id,
        channel.upload_channel_id.as_str(),
        contract_fingerprint,
        channel.reference_scope,
        scope_identity,
        kind,
        mime_type,
        source_fingerprint.byte_len,
        source_fingerprint.content_sha256,
    ))
    .ok()?;
    Some(format!("sha256:{}", hex::encode(Sha256::digest(material))))
}

/// Removes expired prepared media references and enforces the global process-local capacity.
/// 移除已过期媒体准备引用并强制执行进程内全局容量上限。
fn prune_prepared_media_cache(cache: &mut BTreeMap<String, PreparedMediaCacheEntry>, now: Instant) {
    cache.retain(|_, entry| entry.expires_at.is_none_or(|expires_at| expires_at > now));
    while cache.len() > PREPARED_MEDIA_CACHE_MAX_ENTRIES {
        let Some(oldest_key) = cache
            .iter()
            .min_by(|(left_key, left), (right_key, right)| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left_key.cmp(right_key))
            })
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        cache.remove(&oldest_key);
    }
}

/// Returns the current Unix time used to preserve provider reference lifetimes.
/// 返回用于保持供应商引用生命周期的当前 Unix 时间。
fn current_prepared_media_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

/// Checks whether a provider-tool cache entry is still inside both lifetime boundaries.
/// 检查供应商工具缓存条目是否同时处于两个生命周期边界内。
fn provider_tool_reference_is_reusable(
    reference: &PreparedMediaReferenceSnapshot,
    now_unix_millis: u64,
    max_age_ms: u64,
) -> bool {
    reference.created_at_unix_millis <= now_unix_millis
        && reference.expires_at_unix_millis > now_unix_millis
        && now_unix_millis.saturating_sub(reference.created_at_unix_millis) < max_age_ms
}

impl ProviderExecutionClient {
    /// Uploads or pre-reads one local media resource before the conversation is materialized.
    /// 在会话物化前上传或预读取一个本地媒体资源。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `model_id`, and `secret` freeze the exact provider account and model contract.
    /// `route`、`model_id` 与 `secret` 固定精确的供应商账号及模型契约。
    /// `local_path` and `mime_type` identify the validated host attachment.
    /// `local_path` 与 `mime_type` 标识已校验的宿主附件。
    /// `retain_inline_reference` requests an owned Data URL for request-scoped media;
    /// `progress` receives video upload bytes and `cancellation` owns the submit cancel path.
    /// `retain_inline_reference` 为请求作用域媒体请求自有 Data URL；`progress` 接收视频上传字节，
    /// `cancellation` 持有提交取消路径。
    /// `allow_provider_upload` is true only for media kinds whose protocol contract owns a
    /// reusable provider reference, currently video; images and audio remain request-scoped.
    /// `allow_provider_upload` 仅对协议契约要求可复用供应商引用的媒体类型开启，目前为视频；
    /// 图片与音频始终保持请求作用域。
    ///
    /// # Returns
    /// 返回值
    /// A prepared outcome containing an immutable provider-native or host-inline reference and
    /// the exact source fingerprint verified before and after materialization.
    /// 返回包含不可变供应商原生或宿主内联引用，以及在物化前后验证过的精确源文件指纹的准备结果。
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
        retain_inline_reference: bool,
        allow_provider_upload: bool,
        progress: Option<MediaUploadProgressCallback>,
        cancellation: &MediaUploadCancellation,
    ) -> Result<PreparedConversationMedia, String> {
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        // The request must never bind a provider reference to a source that changed during preparation.
        // 请求绝不能把供应商引用绑定到准备期间已经发生变化的源文件。
        let source_fingerprint =
            prepared_media_source_fingerprint(local_path, None, Some(cancellation)).await?;
        // maxBytes is the route-wide ceiling enforced before either upload or inline ownership.
        // maxBytes 是在上传或内联持有之前统一执行的路由级上限。
        if let Some(max_bytes) = route
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| constraint.resource_kind == kind)
            .and_then(|constraint| constraint.max_bytes)
            && source_fingerprint.byte_len > max_bytes
        {
            return Err(format!(
                "Media source is {} bytes and exceeds the frozen route limit of {max_bytes} bytes.",
                source_fingerprint.byte_len
            ));
        }
        let upload_channel = if allow_provider_upload {
            resolve_conversation_media_upload_channel(route, kind, mime_type)?
        } else {
            None
        };
        if kind == ResourceKind::Video && upload_channel.is_none() {
            return Err(format!(
                "Video resource `{}` has no declared provider upload channel; refusing Base64 fallback.",
                local_path.display()
            ));
        }
        let (prepared_reference, current_source_fingerprint) = if let Some(channel) = upload_channel
        {
            // The frozen route selects a bounded upload channel by resource kind and ordered
            // channel references; provider identity never participates in dispatch.
            // 冻结路由按资源类型与有序通道引用选择有界上传通道；供应商身份不参与分派。
            let resource = crate::upload_engine::UploadResource {
                local_path: Some(local_path.to_path_buf()),
                inline_bytes: None,
                mime_type: mime_type.to_string(),
                byte_len: source_fingerprint.byte_len,
                remote_url: None,
                item_count: 1,
            };
            let credential_profile = route
                .entry
                .credential_profiles
                .iter()
                .find(|profile| profile.id == route.connection.credential_profile_id)
                .ok_or_else(|| {
                    format!(
                        "Provider route credential profile `{}` is absent from service entry `{}`.",
                        route.connection.credential_profile_id, route.entry.id
                    )
                })?;
            // nativeReference is immutable provider-owned content produced from the uploaded bytes.
            // nativeReference 是由已上传字节生成的不可变供应商托管内容。
            let native_reference = crate::upload_engine::materialize_upload(
                crate::upload_engine::UploadExecutionContext {
                    http: &self.http,
                    entry: &route.entry,
                    secret,
                    credential_profile,
                    progress: progress.clone(),
                    cancellation,
                    model_id: Some(model_id),
                    fixture_loopback_origin: None,
                },
                channel,
                &resource,
            )
            .await?
            .reference;
            // currentSourceFingerprint proves the source stayed unchanged across upload materialization.
            // currentSourceFingerprint 证明源文件在上传物化期间保持不变。
            let current_source_fingerprint =
                prepared_media_source_fingerprint(local_path, None, Some(cancellation)).await?;
            (Some(native_reference), current_source_fingerprint)
        } else {
            // inlineSource retains bytes only for callers that bind the result directly to a request.
            // inlineSource 仅为把结果直接绑定到请求的调用方保留字节。
            let inline_source = read_prepared_media_source(
                local_path,
                progress,
                Some(cancellation),
                retain_inline_reference,
            )
            .await?;
            // inlineReference owns exact verified bytes when requested and otherwise avoids Base64 allocation.
            // inlineReference 在被请求时持有精确已验证字节，否则避免分配 Base64。
            let inline_reference = inline_source.retained_bytes.map(|inline_bytes| {
                format!(
                    "data:{mime_type};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(inline_bytes)
                )
            });
            (inline_reference, inline_source.fingerprint)
        };
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        if current_source_fingerprint != source_fingerprint {
            return Err(String::from(
                "Media source changed during preparation; please retry the submission.",
            ));
        }
        Ok(PreparedConversationMedia {
            prepared_reference,
            source_fingerprint: current_source_fingerprint,
        })
    }

    /// Resolves the exact non-secret upload-channel identity used for one conversation resource.
    /// 解析一个对话资源使用的精确非秘密上传通道身份。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the immutable execution route; `kind` and `mime_type` identify the exact resource.
    /// `route` 是不可变执行路由；`kind` 与 `mime_type` 标识精确资源。
    ///
    /// # Returns
    /// 返回值
    /// The manifest channel id, `None` for host inline materialization, or a closed route error.
    /// 返回 Manifest 通道标识；宿主内联物化返回 `None`；路由错误返回封闭错误。
    pub fn conversation_media_upload_channel_id(
        &self,
        route: &ProviderExecutionRoute,
        kind: ResourceKind,
        mime_type: &str,
    ) -> Result<Option<String>, String> {
        resolve_conversation_media_upload_channel(route, kind, mime_type)
            .map(|channel| channel.map(|channel| channel.upload_channel_id.clone()))
    }

    /// Resolves the immutable contract identity used to cache one provider media reference.
    /// 解析缓存供应商媒体引用时使用的不可变契约身份。
    pub fn conversation_media_upload_contract(
        &self,
        route: &ProviderExecutionRoute,
        kind: ResourceKind,
        mime_type: &str,
    ) -> Result<Option<ConversationMediaUploadContract>, String> {
        let Some(channel) = resolve_conversation_media_upload_channel(route, kind, mime_type)?
        else {
            return Ok(None);
        };
        Ok(Some(ConversationMediaUploadContract {
            upload_channel_id: channel.upload_channel_id.clone(),
            contract_fingerprint: upload_channel_contract_fingerprint(channel)
                .ok_or_else(|| String::from("Media upload contract could not be fingerprinted."))?,
            reference_scope: channel.reference_scope,
            cache_duration_ms: channel.cache_duration_ms,
            reference_encoding: channel
                .reference_encoding
                .unwrap_or(crate::UploadReferenceEncoding::ProviderFileId),
        }))
    }

    /// Reads one exact source identity without retaining the source bytes in memory.
    /// 读取一个精确源身份，但不在内存中保留源字节。
    pub async fn read_media_source_identity(
        &self,
        local_path: &Path,
        cancellation: Option<&MediaUploadCancellation>,
    ) -> Result<PreparedMediaSourceIdentity, String> {
        prepared_media_source_fingerprint(local_path, None, cancellation)
            .await
            .map(|fingerprint| PreparedMediaSourceIdentity {
                byte_len: fingerprint.byte_len,
                content_sha256: hex::encode(fingerprint.content_sha256),
            })
    }

    /// Looks up a reusable provider-tool media reference under the shared upload-contract boundary.
    /// 按共享上传契约边界查找一条可复用的供应商工具媒体引用。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `service_entry_id`, `connection_id`, and `channel` identify the exact
    /// provider upload contract; `local_path`, `kind`, and `mime_type` identify the source;
    /// session-scoped channels are intentionally non-reusable because this API has no session ledger.
    /// `provider_id`、`service_entry_id`、`connection_id` 与 `channel` 标识精确供应商上传契约；
    /// `local_path`、`kind` 与 `mime_type` 标识源文件；由于该 API 没有会话账本，显式会话作用域通道
    /// 有意不参与复用。
    ///
    /// # Returns
    /// 返回值
    /// A still-valid reference snapshot, `None` when the channel is intentionally non-cacheable or
    /// no matching source reference exists, or an explicit scope/source validation error.
    /// 仍然有效的引用快照；通道明确不可缓存或没有匹配源引用时返回 `None`；作用域/源校验
    /// 失败返回明确错误。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn prepared_provider_tool_media_reference(
        &self,
        provider_id: &str,
        service_entry_id: &str,
        connection_id: &str,
        channel: &crate::UploadChannelDeclaration,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        cancellation: Option<&MediaUploadCancellation>,
    ) -> Result<Option<PreparedMediaReferenceSnapshot>, String> {
        let Some(cache_duration_ms) = channel
            .cache_duration_ms
            .filter(|duration| *duration > 0)
            .map(|duration| duration.min(PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS))
        else {
            return Ok(None);
        };
        if channel.reference_scope == crate::UploadReferenceScope::Session {
            return Ok(None);
        }
        let source_fingerprint =
            prepared_media_source_fingerprint(local_path, None, cancellation).await?;
        let key = prepared_media_cache_key(PreparedMediaCacheKeyInput {
            provider_id,
            service_entry_id,
            connection_id,
            channel,
            source_fingerprint: &source_fingerprint,
            kind,
            mime_type,
            session_scope_id: None,
        })
        .ok_or_else(|| String::from("Provider-tool media cache key could not be constructed."))?;
        let Some(reference) =
            self.cached_prepared_media_reference(key.as_str(), &source_fingerprint)
        else {
            return Ok(None);
        };
        if !provider_tool_reference_is_reusable(
            &reference,
            current_prepared_media_unix_millis(),
            cache_duration_ms,
        ) {
            return Ok(None);
        }
        Ok(Some(reference))
    }

    /// Validates and remembers one provider-tool media reference with its original lifetime.
    /// 校验并按原始生命周期记住一条供应商工具媒体引用。
    ///
    /// # Parameters
    /// 参数
    /// The route identity and channel define the reusable contract; `expected_source_identity`
    /// must be the bytes fingerprint captured before the upload; `provider_reference` is the
    /// reference emitted by the upload engine after the network transfer.
    /// 路由身份与通道定义可复用契约；`expected_source_identity` 必须是上传前捕获的字节指纹；
    /// `provider_reference` 是网络传输完成后上传引擎生成的引用。
    ///
    /// # Returns
    /// 返回值
    /// Success after the post-upload source recheck and cache insertion, or a controlled mismatch,
    /// scope, reference, or lifetime failure.
    /// 上传后源文件复检与缓存插入完成后返回成功；否则返回受控的源不一致、作用域、引用或
    /// 生命周期错误。
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn remember_provider_tool_media_reference(
        &self,
        provider_id: &str,
        service_entry_id: &str,
        connection_id: &str,
        channel: &crate::UploadChannelDeclaration,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        expected_source_identity: &PreparedMediaSourceIdentity,
        provider_reference: String,
        first_used_at_unix_millis: u64,
    ) -> Result<(), String> {
        let Some(cache_duration_ms) = channel
            .cache_duration_ms
            .filter(|duration| *duration > 0)
            .map(|duration| duration.min(PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS))
        else {
            return Ok(());
        };
        if channel.reference_scope == crate::UploadReferenceScope::Session {
            return Ok(());
        }
        crate::upload_engine::validate_prepared_reference(channel, provider_reference.as_str())?;
        let source_fingerprint = prepared_media_source_fingerprint(local_path, None, None).await?;
        let actual_source_identity = PreparedMediaSourceIdentity {
            byte_len: source_fingerprint.byte_len,
            content_sha256: hex::encode(source_fingerprint.content_sha256),
        };
        if &actual_source_identity != expected_source_identity {
            return Err(String::from(
                "Provider-tool media source changed during upload; refusing to cache the provider reference.",
            ));
        }
        let key = prepared_media_cache_key(PreparedMediaCacheKeyInput {
            provider_id,
            service_entry_id,
            connection_id,
            channel,
            source_fingerprint: &source_fingerprint,
            kind,
            mime_type,
            session_scope_id: None,
        })
        .ok_or_else(|| String::from("Provider-tool media cache key could not be constructed."))?;
        let now_unix_millis = current_prepared_media_unix_millis();
        if first_used_at_unix_millis > now_unix_millis {
            return Err(String::from(
                "Provider-tool media cache first-use time is in the future; refusing to cache the reference.",
            ));
        }
        let expires_at_unix_millis = first_used_at_unix_millis.saturating_add(cache_duration_ms);
        self.remember_prepared_media_cache_entry(PreparedMediaCacheWrite {
            key,
            provider_reference,
            reference_encoding: channel
                .reference_encoding
                .unwrap_or(crate::UploadReferenceEncoding::ProviderFileId),
            source_fingerprint,
            cache_duration_ms,
            wall_clock_lifetime: Some((first_used_at_unix_millis, expires_at_unix_millis)),
            keep_until_process_exit: false,
        });
        Ok(())
    }

    /// Returns one cache entry after pruning and source-fingerprint validation.
    /// 裁剪缓存并校验源文件指纹后返回一条缓存项。
    fn cached_prepared_media_reference(
        &self,
        key: &str,
        source_fingerprint: &PreparedMediaSourceFingerprint,
    ) -> Option<PreparedMediaReferenceSnapshot> {
        let mut cache = self.prepared_media.lock().ok()?;
        prune_prepared_media_cache(&mut cache, Instant::now());
        let entry = cache.get(key)?.clone();
        if entry.source_fingerprint != *source_fingerprint {
            cache.remove(key);
            return None;
        }
        Some(PreparedMediaReferenceSnapshot {
            provider_reference: entry.provider_reference,
            reference_encoding: entry.reference_encoding,
            created_at_unix_millis: entry.created_at_unix_millis,
            expires_at_unix_millis: entry.expires_at_unix_millis,
        })
    }

    /// Seeds the process cache from a persisted reference after revalidating the local source.
    /// 在重新验证本地源后，用持久化引用预热进程缓存。
    #[allow(clippy::too_many_arguments)]
    pub async fn remember_persisted_media_reference(
        &self,
        route: &ProviderExecutionRoute,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        provider_reference: String,
        source_sha256: &str,
        source_byte_len: u64,
        session_scope_id: Option<&str>,
        created_at_unix_millis: u64,
        expires_at_unix_millis: u64,
    ) -> Result<(), String> {
        let channel = resolve_conversation_media_upload_channel(route, kind, mime_type)?
            .ok_or_else(|| String::from("Persisted media reference requires an upload channel."))?;
        if channel.reference_scope == crate::UploadReferenceScope::Session
            && session_scope_id.is_none()
        {
            return Err(format!(
                "Upload channel `{}` requires a session identity before a persisted media reference can be reused.",
                channel.upload_channel_id
            ));
        }
        crate::upload_engine::validate_prepared_reference(channel, provider_reference.as_str())?;
        let source_fingerprint = prepared_media_source_fingerprint(local_path, None, None).await?;
        let actual_source_sha256 = hex::encode(source_fingerprint.content_sha256);
        if actual_source_sha256 != source_sha256 || source_fingerprint.byte_len != source_byte_len {
            return Err(String::from(
                "Persisted media reference does not match the current source bytes.",
            ));
        }
        self.remember_prepared_media_with_wall_clock_and_retention(
            route,
            local_path,
            kind,
            mime_type,
            PreparedConversationMedia {
                prepared_reference: Some(provider_reference),
                source_fingerprint,
            },
            session_scope_id,
            Some((created_at_unix_millis, expires_at_unix_millis)),
            true,
        );
        Ok(())
    }

    /// Remembers one provider-native reference for the exact route and local source.
    /// 为精确路由与本地源记住一个供应商原生引用。
    pub fn remember_prepared_media(
        &self,
        route: &ProviderExecutionRoute,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        prepared: PreparedConversationMedia,
        session_scope_id: Option<&str>,
    ) {
        self.remember_prepared_media_with_wall_clock(
            route,
            local_path,
            kind,
            mime_type,
            prepared,
            session_scope_id,
            None,
        );
    }

    /// Remembers one reference while preserving an existing provider wall-clock lifetime when supplied.
    /// 在提供既有供应商墙钟生命周期时，按原有效期记住一条引用。
    #[allow(clippy::too_many_arguments)]
    pub fn remember_prepared_media_with_wall_clock(
        &self,
        route: &ProviderExecutionRoute,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        prepared: PreparedConversationMedia,
        session_scope_id: Option<&str>,
        wall_clock_lifetime: Option<(u64, u64)>,
    ) {
        self.remember_prepared_media_with_wall_clock_and_retention(
            route,
            local_path,
            kind,
            mime_type,
            prepared,
            session_scope_id,
            wall_clock_lifetime,
            false,
        );
    }

    /// Remembers a newly uploaded conversation reference until the process exits.
    /// 将新上传的会话引用保留到进程退出，以便普通会话永不因宿主 TTL 重新上传。
    pub fn remember_conversation_prepared_media(
        &self,
        route: &ProviderExecutionRoute,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        prepared: PreparedConversationMedia,
        retention: ConversationPreparedMediaRetention<'_>,
    ) {
        self.remember_prepared_media_with_wall_clock_and_retention(
            route,
            local_path,
            kind,
            mime_type,
            prepared,
            retention.session_scope_id,
            retention.wall_clock_lifetime,
            true,
        );
    }

    /// Remembers one reference with an explicit process-retention policy.
    /// 按显式进程保留策略记住一条引用。
    #[allow(clippy::too_many_arguments)]
    fn remember_prepared_media_with_wall_clock_and_retention(
        &self,
        route: &ProviderExecutionRoute,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        prepared: PreparedConversationMedia,
        session_scope_id: Option<&str>,
        wall_clock_lifetime: Option<(u64, u64)>,
        keep_until_process_exit: bool,
    ) {
        let PreparedConversationMedia {
            prepared_reference: Some(url),
            source_fingerprint,
        } = prepared
        else {
            return;
        };
        let resource = crate::upload_engine::UploadResource {
            local_path: Some(local_path.to_path_buf()),
            inline_bytes: None,
            mime_type: mime_type.to_string(),
            byte_len: source_fingerprint.byte_len,
            remote_url: None,
            item_count: 1,
        };
        let Some(credential_profile) = route
            .entry
            .credential_profiles
            .iter()
            .find(|profile| profile.id == route.connection.credential_profile_id)
        else {
            return;
        };
        let credential_role = credential_profile.credential_role.as_deref();
        let channel = route
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| {
                constraint.resource_kind == kind && !constraint.upload_channel_refs.is_empty()
            })
            .and_then(|constraint| {
                crate::upload_engine::resolve_upload_channel(
                    route.upload_channels.as_slice(),
                    constraint.upload_channel_refs.as_slice(),
                    &resource,
                    credential_role,
                )
                .ok()
            });
        let cache_duration_ms = channel.and_then(|channel| channel.cache_duration_ms);
        // Timed process caches require a declared provider lifetime; durable conversation entries may retain a reference without one.
        // 有时效的进程缓存必须声明供应商生命周期；持久会话条目可以在未声明生命周期时继续保留引用。
        let cache_duration_ms = cache_duration_ms.unwrap_or_default();
        if !keep_until_process_exit && cache_duration_ms == 0 {
            return;
        }
        let Some(key) = prepared_media_key(
            route,
            &source_fingerprint,
            kind,
            mime_type,
            session_scope_id,
        ) else {
            return;
        };
        self.remember_prepared_media_cache_entry(PreparedMediaCacheWrite {
            key,
            provider_reference: url,
            reference_encoding: channel
                .and_then(|channel| channel.reference_encoding)
                .unwrap_or(crate::UploadReferenceEncoding::ProviderFileId),
            source_fingerprint,
            cache_duration_ms,
            wall_clock_lifetime,
            keep_until_process_exit,
        });
    }

    /// Inserts one provider reference while preserving the provider-issued wall-clock lifetime.
    /// 保留供应商签发的墙钟生命周期并插入一条供应商引用。
    ///
    /// # Parameters
    /// 参数
    /// `key`, `provider_reference`, `reference_encoding`, and `source_fingerprint` identify the exact cached source; `cache_duration_ms`
    /// supplies the declared lifetime for a fresh upload; `wall_clock_lifetime` preserves an
    /// existing persisted or provider-tool lifetime when available; `keep_until_process_exit`
    /// disables monotonic expiry for a durable conversation reference.
    /// `key`、`provider_reference`、`reference_encoding` 与 `source_fingerprint` 标识精确缓存源；`cache_duration_ms` 为新上传提供
    /// 声明生命周期；`wall_clock_lifetime` 在可用时保留已有持久或供应商工具生命周期；
    /// `keep_until_process_exit` 为持久会话引用关闭单调时钟过期。
    fn remember_prepared_media_cache_entry(&self, input: PreparedMediaCacheWrite) {
        let PreparedMediaCacheWrite {
            key,
            provider_reference,
            reference_encoding,
            source_fingerprint,
            cache_duration_ms,
            wall_clock_lifetime,
            keep_until_process_exit,
        } = input;
        let now = Instant::now();
        let now_unix_millis = current_prepared_media_unix_millis();
        let (created_at_unix_millis, expires_at_unix_millis, expires_at) =
            if let Some((created_at_unix_millis, expires_at_unix_millis)) = wall_clock_lifetime {
                if created_at_unix_millis > expires_at_unix_millis {
                    return;
                }
                if keep_until_process_exit {
                    (created_at_unix_millis, expires_at_unix_millis, None)
                } else {
                    if expires_at_unix_millis <= now_unix_millis {
                        return;
                    }
                    let remaining_ms = expires_at_unix_millis.saturating_sub(now_unix_millis);
                    let Some(expires_at) = now.checked_add(Duration::from_millis(remaining_ms))
                    else {
                        return;
                    };
                    (
                        created_at_unix_millis,
                        expires_at_unix_millis,
                        Some(expires_at),
                    )
                }
            } else {
                let expires_at_unix_millis = if cache_duration_ms == 0 {
                    u64::MAX
                } else {
                    now_unix_millis.saturating_add(cache_duration_ms)
                };
                if keep_until_process_exit {
                    (now_unix_millis, expires_at_unix_millis, None)
                } else {
                    let Some(expires_at) =
                        now.checked_add(Duration::from_millis(cache_duration_ms))
                    else {
                        return;
                    };
                    (now_unix_millis, expires_at_unix_millis, Some(expires_at))
                }
            };
        if let Ok(mut cache) = self.prepared_media.lock() {
            cache.insert(
                key,
                PreparedMediaCacheEntry {
                    provider_reference,
                    reference_encoding,
                    expires_at,
                    created_at: now,
                    created_at_unix_millis,
                    expires_at_unix_millis,
                    source_fingerprint,
                },
            );
            prune_prepared_media_cache(&mut cache, now);
        }
    }

    /// Returns a still-valid provider reference and its original wall-clock lifetime.
    /// 返回仍然有效的供应商引用及其原始墙钟生命周期。
    pub async fn prepared_media_reference(
        &self,
        route: &ProviderExecutionRoute,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        session_scope_id: Option<&str>,
        cancellation: Option<&MediaUploadCancellation>,
    ) -> Option<PreparedMediaReferenceSnapshot> {
        let source_fingerprint = prepared_media_source_fingerprint(local_path, None, cancellation)
            .await
            .ok()?;
        let key = prepared_media_key(
            route,
            &source_fingerprint,
            kind,
            mime_type,
            session_scope_id,
        )?;
        self.cached_prepared_media_reference(key.as_str(), &source_fingerprint)
    }

    /// Returns a still-valid provider-native reference for the exact local source.
    /// 返回精确本地源仍然有效的供应商原生引用。
    pub async fn prepared_media_url(
        &self,
        route: &ProviderExecutionRoute,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        session_scope_id: Option<&str>,
        cancellation: Option<&MediaUploadCancellation>,
    ) -> Option<String> {
        self.prepared_media_reference(
            route,
            local_path,
            kind,
            mime_type,
            session_scope_id,
            cancellation,
        )
        .await
        .map(|snapshot| snapshot.provider_reference)
    }
}

#[cfg(test)]
mod tests;
