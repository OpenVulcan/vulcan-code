//! Process-local provider catalogs, clients, and credential access.
//! 进程内供应商目录、客户端与凭据访问。

use crate::provider_task::{
    ProviderAsyncTaskPhase, ProviderAsyncTaskPoll, ProviderAsyncTaskRecord,
    ProviderAsyncTaskRecoveryContext, ProviderAsyncTaskRoute, ProviderTaskRecoveryStore,
    bound_provider_task_diagnostic, provider_task_observation_delay_ms,
};
use crate::{
    ActiveAiModelSelection, ConnectionModel, ConnectionModelCatalog, CredentialConnection,
    CredentialExecutionSecret, CredentialSecretEnvelope, HttpProviderMetadataClient,
    InferenceRouteBinding, InferenceRouteConfiguration, ModelCapabilityEvidence,
    ModelDiscoveryFailureKind, ModelDiscoveryKind, ModelSourcePolicy, OAuthAuthorizationRuntime,
    PersistedAiConfiguration, ProtocolModelVariant, ProviderExecutionClient,
    ProviderExecutionFailure, ProviderExecutionRoute, ProviderExecutionStream, ProviderKind,
    ProviderMetadataClient, ProviderRegistry, ProviderToolCapabilityKind,
    ProviderToolExecutionResult, ProviderToolTarget, ProviderToolTargetSelection,
    ProviderWireRequestCapture, ResolvedProviderManifest, RuntimeConnectionCatalogs,
    SecondaryRouteOption, SecondaryRouteSelection,
    native_inference::validate_native_action_binding,
};
use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::Path;
use std::sync::{
    Arc, Mutex as StdMutex, RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;
use vulcan_ai_contract::{
    AccountUsageSnapshot, AiExecutionRequest, AiMessage, AiOperation, CapabilitySupport,
    ContentItem, EmbeddingRequest, EmbeddingResponse, InferenceUsageScope, MessageRole,
    ModelCapabilities, PromptCachePolicy, ReasoningControlMode, ReasoningEffort, RerankRequest,
    RerankResponse, ResourceKind,
};
use vulcan_llm_core::LlmProtocolRuntime;

/// Refreshes renewable tokens this long before their declared expiration.
/// 在可续期 Token 声明过期前这么长时间执行刷新。
const OAUTH_REFRESH_SKEW_MS: u64 = 5 * 60 * 1_000;

/// Reports whether one media-understanding source must use an available provider upload channel.
/// 判断一个媒体理解源是否必须使用可用的 Provider 上传通道。
///
/// # Parameters
/// 参数
/// `kind` is the exact media modality, `upload_available` reflects the frozen route contract, and
/// `source_byte_len` is the immutable source length observed before preparation.
/// `kind` 是精确媒体模态，`upload_available` 反映冻结路由契约，`source_byte_len` 是准备前观察到的不可变源长度。
///
/// # Returns
/// 返回值
/// `true` for every upload-capable video and for upload-capable image/audio sources above the shared inline ceiling.
/// 对全部具备上传能力的视频，以及超过共享内联上限且具备上传能力的图片/音频返回 `true`。
fn media_understanding_uses_provider_upload(
    kind: ResourceKind,
    upload_available: bool,
    source_byte_len: u64,
) -> bool {
    upload_available
        && (kind == ResourceKind::Video
            || source_byte_len > crate::PREFERRED_INLINE_MEDIA_MAX_BYTES)
}

/// One immutable media reference prepared for a model-replacement understanding request.
/// 为模型代替理解请求准备的一条不可变媒体引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedLocalMediaReference {
    /// Exact request reference returned by inline preparation or the Provider upload channel.
    /// 由内联准备或供应商上传通道返回的精确请求引用。
    pub reference: String,
    /// Provider-declared encoding for uploaded references; `None` identifies an inline Data URL.
    /// 已上传引用使用供应商声明的编码；`None` 标识内联 Data URL。
    pub reference_encoding: Option<crate::UploadReferenceEncoding>,
}

/// Carries the complete immutable admission for one local-media preparation.
/// 携带一次本地媒体准备所需的完整不可变准入信息。
pub struct PrepareLocalMediaRequest<'a> {
    /// Exact resolved Provider selection used for preparation and caching.
    /// 用于准备与缓存的精确已解析供应商选择。
    pub selection: &'a ResolvedPublicAiSelection,
    /// Optional owning session required by session-scoped upload channels.
    /// 会话作用域上传通道要求的可选所有会话。
    pub owner_session_id: Option<&'a str>,
    /// Stable host operation identity used for progress projection.
    /// 用于进度投影的稳定宿主操作身份。
    pub operation_id: &'a str,
    /// Exact local source path read during preparation.
    /// 准备期间读取的精确本地源路径。
    pub local_path: &'a Path,
    /// Closed source resource kind.
    /// 封闭源资源类型。
    pub kind: ResourceKind,
    /// Canonical source MIME type.
    /// 规范源 MIME 类型。
    pub mime_type: &'a str,
    /// Optional transfer-progress callback.
    /// 可选传输进度回调。
    pub progress: Option<crate::MediaUploadProgressCallback>,
    /// Cooperative cancellation owner for the complete preparation.
    /// 完整准备过程使用的协作取消所有者。
    pub cancellation: &'a crate::MediaUploadCancellation,
}

/// Immutable emitter context for one media upload performed after request dispatch.
/// 请求分派后执行单次媒体上传时使用的不可变发射上下文。
#[derive(Clone)]
struct RuntimeMediaUploadProgressContext {
    /// Host callback installed by the service boundary.
    /// 服务边界安装的宿主回调。
    observer: crate::RuntimeMediaUploadProgressObserver,
    /// Stable non-secret event fields reused by every lifecycle phase.
    /// 每个生命周期阶段复用的稳定非秘密事件字段。
    base: crate::RuntimeMediaUploadProgressEvent,
    /// Latest transferred byte count retained for cancellation and failure terminal events.
    /// 为取消与失败终态事件保留的最近已传输字节数。
    last_bytes_sent: Arc<AtomicU64>,
}

impl RuntimeMediaUploadProgressContext {
    /// Emits one phase replacement for this exact transfer.
    /// 为当前精确传输发出一次阶段替换。
    ///
    /// # Parameters
    /// 参数
    /// `phase`, `bytes_sent`, `total_bytes`, and `error` are the current transfer facts.
    /// `phase`、`bytes_sent`、`total_bytes` 与 `error` 是当前传输事实。
    fn emit(
        &self,
        phase: crate::RuntimeMediaUploadPhase,
        bytes_sent: u64,
        total_bytes: Option<u64>,
        error: Option<String>,
    ) {
        self.last_bytes_sent.store(bytes_sent, Ordering::Release);
        // event preserves stable route identity while replacing only lifecycle facts.
        // event 保留稳定路由身份，仅替换生命周期事实。
        let mut event = self.base.clone();
        event.phase = phase;
        event.bytes_sent = bytes_sent;
        event.total_bytes = total_bytes;
        event.error = error;
        (self.observer)(event);
    }

    /// Builds the byte callback consumed by the generic upload engine.
    /// 构建通用上传引擎消费的字节回调。
    ///
    /// # Returns
    /// 返回值
    /// A cloneable callback that emits uploading replacements for this transfer.
    /// 返回一个可克隆回调，为当前传输发出上传中替换事件。
    fn byte_callback(&self) -> crate::MediaUploadProgressCallback {
        // context remains immutable and can safely cross the asynchronous upload boundary.
        // context 保持不可变，可安全跨越异步上传边界。
        let context = self.clone();
        Arc::new(move |bytes_sent, total_bytes| {
            context.emit(
                crate::RuntimeMediaUploadPhase::Uploading,
                bytes_sent,
                total_bytes,
                None,
            );
        })
    }
}

/// Emits a cancelled terminal event when an in-flight media upload future is dropped.
/// 当进行中的媒体上传 Future 被丢弃时发出取消终态事件。
struct RuntimeMediaUploadTerminalGuard {
    /// Exact progress context retained until a normal terminal phase is emitted.
    /// 保留到正常终态发出为止的精确进度上下文。
    context: Option<RuntimeMediaUploadProgressContext>,
}

impl RuntimeMediaUploadTerminalGuard {
    /// Creates one armed terminal guard from an optional user-visible progress context.
    /// 从可选的用户可见进度上下文创建一个已启用终态守卫。
    ///
    /// # Parameters
    /// 参数
    /// `context` is the exact transfer whose future lifetime is guarded.
    /// `context` 是其 Future 生命周期被守护的精确传输。
    ///
    /// # Returns
    /// 返回值
    /// An armed guard when progress is visible, or an inert guard otherwise.
    /// 进度可见时返回已启用守卫，否则返回惰性守卫。
    fn new(context: Option<&RuntimeMediaUploadProgressContext>) -> Self {
        Self {
            context: context.cloned(),
        }
    }

    /// Disarms the drop fallback after a normal terminal event has been emitted.
    /// 在正常终态事件发出后解除丢弃回退。
    fn disarm(&mut self) {
        self.context = None;
    }
}

impl Drop for RuntimeMediaUploadTerminalGuard {
    /// Emits cancellation with the latest observed byte count when still armed.
    /// 仍启用时使用最近观测字节数发出取消事件。
    fn drop(&mut self) {
        let Some(context) = self.context.take() else {
            return;
        };
        context.emit(
            crate::RuntimeMediaUploadPhase::Cancelled,
            context.last_bytes_sent.load(Ordering::Acquire),
            context.base.total_bytes,
            None,
        );
    }
}

/// Keeps provider endpoint discovery results fresh for six hours by default.
/// 默认将供应商端点发现结果保鲜六小时。
const SECONDARY_ROUTE_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Maximum process-local secondary-route cache partitions retained across all accounts.
/// 全部账号合计保留的进程内二级路由缓存分区上限。
const SECONDARY_ROUTE_CACHE_MAX_ENTRIES: usize = 512;
/// Maximum immutable session route leases retained by one runtime process.
/// 单个运行时进程允许保留的不可变会话路由租约最大数量。
const SESSION_SELECTION_LEASE_MAX_ENTRIES: usize = 4_096;
/// Maximum UTF-8 bytes accepted for one session route lease key.
/// 单个会话路由租约键允许接受的 UTF-8 字节最大数量。
const SESSION_SELECTION_LEASE_ID_MAX_BYTES: usize = 1_024;

/// Maximum wall time for one provider task poll or cancellation observation.
/// 单次供应商任务轮询或取消观测的最大墙钟时间。
const PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum independent provider tasks observed concurrently by one recovery pass.
/// 单次恢复允许并发观测的独立供应商任务上限。
const PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY: usize = 8;
/// Host-owned deadline for legacy code targets that have no Manifest operation declaration.
/// 没有 Manifest operation 声明的旧代码目标所使用的宿主持有截止时间。
const LEGACY_PROVIDER_TOOL_OPERATION_TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum characters retained from one untrusted provider-tool failure body.
/// 单个不可信供应商工具失败正文允许保留的最大字符数。
const PROVIDER_TOOL_ERROR_DIAGNOSTIC_MAX_CHARACTERS: usize = 6_000;
/// Maximum characters retained from one immutable provider-tool route context.
/// 单个不可变供应商工具路由上下文允许保留的最大字符数。
const PROVIDER_TOOL_ROUTE_DIAGNOSTIC_MAX_CHARACTERS: usize = 2_000;

/// Resolves the latest secret revision for one frozen account without changing its route.
/// 在不改变冻结账号路由的前提下，为该账号解析最新秘密修订版。
#[derive(Clone)]
pub struct ProviderExecutionCredentialLease {
    /// Exact connection whose refresh operations share one single-flight lock.
    /// 刷新操作共享同一个单飞锁的精确连接。
    connection_id: String,
    /// Exact secure-store reference frozen with the selected account.
    /// 随所选账号冻结的精确安全仓库引用。
    secret_reference: String,
    /// Shared provider OAuth runtime used only for renewable credentials.
    /// 仅为可续期凭据使用的共享供应商 OAuth 运行时。
    oauth: Arc<OAuthAuthorizationRuntime>,
    /// Connection-scoped locks shared with every runtime credential operation.
    /// 与全部运行时凭据操作共享的连接作用域锁。
    refresh_locks: Arc<StdMutex<BTreeMap<String, Arc<AsyncMutex<()>>>>>,
    /// Host-owned secure secret store.
    /// 宿主持有的安全秘密仓库。
    secrets: Arc<dyn CredentialSecretStore>,
    /// One-shot 401 recovery guard reset only after a physical Provider request succeeds.
    /// 仅在一次真实 Provider 请求成功后重置的单次 401 恢复护栏。
    unauthorized_refresh_used: Arc<AtomicBool>,
}

impl ProviderExecutionCredentialLease {
    /// Creates one credential lease for an already selected account.
    /// 为已经选定的账号创建一份凭据租约。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `connection_id` and `secret_reference` freeze account identity; the remaining values
    /// share the host-owned refresh and storage boundaries.
    /// `connection_id` 与 `secret_reference` 冻结账号身份；其余值共享宿主持有的刷新与存储边界。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A reusable lease that re-reads only the exact frozen secret reference.
    /// 返回仅重读精确冻结秘密引用的可复用租约。
    fn new(
        connection_id: String,
        secret_reference: String,
        oauth: Arc<OAuthAuthorizationRuntime>,
        refresh_locks: Arc<StdMutex<BTreeMap<String, Arc<AsyncMutex<()>>>>>,
        secrets: Arc<dyn CredentialSecretStore>,
    ) -> Self {
        Self {
            connection_id,
            secret_reference,
            oauth,
            refresh_locks,
            secrets,
            unauthorized_refresh_used: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Resolves the latest valid secret immediately before one physical Provider request.
    /// 在一次真实 Provider 请求之前解析最新有效秘密。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The newest stored revision, refreshing an expiring OAuth credential under one lock.
    /// 返回最新存储修订版，并在单飞锁内刷新即将过期的 OAuth 凭据。
    pub async fn resolve_for_request(&self) -> Result<CredentialExecutionSecret, String> {
        // current is re-read for every physical request so concurrent account refreshes become visible immediately.
        // current 会在每次真实请求前重新读取，使并发账号刷新立即可见。
        let current = self
            .secrets
            .read_secret(self.secret_reference.as_str())
            .await?;
        if !current.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS) {
            return current.execution_secret();
        }
        self.refresh_expiring_secret().await
    }

    /// Forces one renewable credential recovery after an exact upstream 401 response.
    /// 在收到精确上游 401 响应后强制恢复一次可续期凭据。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `rejected_revision` is the exact secret-store revision rejected by the upstream request.
    /// `rejected_revision` 是上游请求拒绝的精确秘密仓库修订号。
    ///
    /// # Returns
    /// 返回值
    ///
    /// `true` after a newer revision is observed or one forced OAuth refresh succeeds; static
    /// credentials and repeated recovery within the same physical request return `false`.
    /// 观察到更新修订版或一次强制 OAuth 刷新成功后返回 `true`；静态凭据以及同一真实请求内
    /// 的重复恢复返回 `false`。
    pub async fn refresh_after_unauthorized(&self, rejected_revision: u64) -> Result<bool, String> {
        // current determines whether the exact frozen credential is renewable before claiming recovery.
        // current 在占用恢复机会前判断精确冻结凭据是否可续期。
        let current = self
            .secrets
            .read_secret(self.secret_reference.as_str())
            .await?;
        if !current.is_renewable_oauth() {
            return Ok(false);
        }
        if self.unauthorized_refresh_used.swap(true, Ordering::AcqRel) {
            return Ok(false);
        }
        // refreshLock serializes forced 401 recovery with expiry-driven refreshes for this account.
        // refreshLock 将强制 401 恢复与该账号的到期刷新串行化。
        let refresh_lock = self.refresh_lock()?;
        let _guard = refresh_lock.lock().await;
        // current is re-read after the lock because another request may already have replaced the rejected revision.
        // current 在获得锁后重新读取，因为另一个请求可能已经替换了被拒绝的修订版。
        let current = self
            .secrets
            .read_secret(self.secret_reference.as_str())
            .await?;
        if current.revision > rejected_revision
            && !current.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS)
        {
            current.execution_secret()?;
            return Ok(true);
        }
        if !current.is_renewable_oauth() {
            return Ok(false);
        }
        self.refresh_locked_secret(&current).await?;
        Ok(true)
    }

    /// Resets the one-shot unauthorized recovery guard after one physical request succeeds.
    /// 在一次真实请求成功后重置单次未授权恢复护栏。
    pub fn mark_request_succeeded(&self) {
        self.unauthorized_refresh_used
            .store(false, Ordering::Release);
    }

    /// Refreshes one expiring credential after re-checking it under the account lock.
    /// 在账号锁内重新检查并刷新一份即将过期的凭据。
    async fn refresh_expiring_secret(&self) -> Result<CredentialExecutionSecret, String> {
        // refreshLock shares expiry refresh ordering with forced 401 recovery.
        // refreshLock 让到期刷新与强制 401 恢复共享顺序。
        let refresh_lock = self.refresh_lock()?;
        let _guard = refresh_lock.lock().await;
        // current is authoritative only after the single-flight lock is held.
        // current 仅在持有单飞锁后才是权威值。
        let current = self
            .secrets
            .read_secret(self.secret_reference.as_str())
            .await?;
        if !current.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS) {
            return current.execution_secret();
        }
        self.refresh_locked_secret(&current).await
    }

    /// Returns the connection-scoped single-flight lock for this frozen account.
    /// 返回当前冻结账号的连接作用域单飞锁。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The shared asynchronous lock, or a controlled poisoned-lock error.
    /// 返回共享异步锁，或受控的锁中毒错误。
    fn refresh_lock(&self) -> Result<Arc<AsyncMutex<()>>, String> {
        // locks is the process-local map that owns every connection refresh lock.
        // locks 是持有每个连接刷新锁的进程本地映射。
        let mut locks = self
            .refresh_locks
            .lock()
            .map_err(|_| String::from("The AI credential refresh lock is poisoned."))?;
        Ok(Arc::clone(
            locks
                .entry(self.connection_id.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        ))
    }

    /// Refreshes and compare-and-replaces one credential while its connection lock is held.
    /// 在持有连接锁时刷新并比较替换一份凭据。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `current` is the exact authoritative revision read under the refresh lock.
    /// `current` 是在刷新锁内读取的精确权威修订版。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The refreshed execution secret or a newer valid concurrent winner.
    /// 返回刷新后的执行秘密，或更新且有效的并发获胜修订版。
    async fn refresh_locked_secret(
        &self,
        current: &CredentialSecretEnvelope,
    ) -> Result<CredentialExecutionSecret, String> {
        // material is the provider-specific refreshed credential returned by the OAuth runtime.
        // material 是 OAuth 运行时返回的供应商专属刷新凭据。
        let material = self.oauth.refresh(&current.material).await?;
        // refreshed advances the secure-store revision exactly once for this material.
        // refreshed 为该凭据材料精确推进一次安全仓库修订号。
        let refreshed = current.advance_with_material(material)?;
        if let Err(replace_error) = self
            .secrets
            .replace_secret(self.secret_reference.as_str(), current.revision, &refreshed)
            .await
        {
            // concurrent is accepted only when another writer published a newer non-expiring revision.
            // concurrent 仅在另一个写入者发布更新且未临近过期的修订版时被接受。
            let concurrent = self
                .secrets
                .read_secret(self.secret_reference.as_str())
                .await?;
            if concurrent.revision > current.revision
                && !concurrent.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS)
            {
                return concurrent.execution_secret();
            }
            return Err(replace_error);
        }
        refreshed.execution_secret()
    }
}

/// Observes one durable task batch with a fixed cross-provider concurrency ceiling.
/// 以固定的跨供应商并发上限观测一批持久任务。
///
/// # Parameters
/// 参数
/// `records` is one immutable recovery-pass snapshot and `observe` performs exactly one task's
/// cancellation or poll action.
/// `records` 是一次不可变恢复轮次快照，`observe` 执行精确单个任务的取消或轮询动作。
///
/// # Returns
/// 返回值
/// Every task identity paired with its independent observation result in completion order.
/// 返回完成顺序下每个任务身份及其独立观测结果。
async fn observe_provider_task_records_bounded<Observe, Observation>(
    records: Vec<ProviderAsyncTaskRecord>,
    observe: Observe,
) -> Vec<(String, Result<(), String>)>
where
    Observe: Fn(ProviderAsyncTaskRecord) -> Observation,
    Observation: Future<Output = Result<(), String>>,
{
    stream::iter(records)
        .map(|record| {
            // execution_id survives moving the complete record into its independent future.
            // execution_id 在完整记录移入独立 Future 后仍保留任务身份。
            let execution_id = record.execution_id.clone();
            // observation starts only when buffer_unordered admits this task below the ceiling.
            // observation 仅在 buffer_unordered 于上限内接纳当前任务时启动。
            let observation = observe(record);
            async move { (execution_id, observation.await) }
        })
        .buffer_unordered(PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
}

/// Returns the remaining monotonic budget for one already-started provider operation.
/// 返回一项已经开始的供应商操作所剩余的单调时钟预算。
///
/// # Parameters
/// 参数
/// `deadline` is the immutable monotonic deadline and `operation` scopes the controlled error.
/// `deadline` 是不可变单调时钟截止点，`operation` 限定受控错误范围。
///
/// # Returns
/// 返回值
/// A positive duration before the deadline, or an explicit operation-timeout failure.
/// 返回截止前的正时长，或明确的操作超时失败。
fn remaining_provider_operation_timeout(
    deadline: Instant,
    operation: &str,
) -> Result<Duration, String> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| format!("{operation} exceeded its frozen total execution timeout."))
}

/// Identifies the host-owned default reasoning policy applied to unknown text models.
/// 标识应用于未知文本模型的宿主默认思考策略。
const HOST_DEFAULT_REASONING_EVIDENCE_REVISION: &str = "host-default-reasoning.v1";

/// Installs the host-selected process-wide Rustls provider before any TLS client is constructed.
/// 在构造任何 TLS 客户端之前安装宿主选择的进程级 Rustls 加密提供程序。
///
/// # Returns
/// 返回值
/// Success when Ring is installed or another caller already completed the same process-wide installation.
/// 当 Ring 安装成功，或其他调用方已经完成同一进程级安装时返回成功。
pub fn install_process_rustls_crypto_provider() -> Result<(), String> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }
    match rustls::crypto::ring::default_provider().install_default() {
        Ok(()) => Ok(()),
        Err(_) if rustls::crypto::CryptoProvider::get_default().is_some() => Ok(()),
        Err(_) => Err(String::from(
            "Failed to install the process-wide Ring Rustls CryptoProvider.",
        )),
    }
}

/// Identifies the closed outcome of one credential-scoped model discovery attempt.
/// 标识一次凭据作用域模型发现尝试的封闭结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogDiscoveryState {
    /// No discovery has run since process start.
    /// 进程启动后尚未执行发现。
    NotChecked,
    /// A non-empty authoritative catalog was installed.
    /// 已安装非空权威目录。
    Ready,
    /// Discovery succeeded but returned no models.
    /// 发现成功但没有返回模型。
    Empty,
    /// The endpoint explicitly does not support model discovery.
    /// 端点明确不支持模型发现。
    Unsupported,
    /// The supplied credential cannot access model discovery.
    /// 所提供凭据无法访问模型发现。
    AuthenticationFailed,
    /// The provider rate-limited model discovery.
    /// 供应商对模型发现进行了限流。
    RateLimited,
    /// A network or upstream availability failure may recover later.
    /// 网络或上游可用性故障可能稍后恢复。
    TemporarilyUnavailable,
    /// The provider returned a response that violates the selected protocol.
    /// 供应商返回了违反所选协议的响应。
    InvalidResponse,
    /// A previous successful catalog remains active after the latest failure.
    /// 最近一次失败后仍继续使用先前成功目录。
    Stale,
    /// A controlled failure did not match another closed category.
    /// 受控失败未匹配其他封闭类别。
    Failed,
}

/// Identifies the failure behind a stale catalog without changing its usability.
/// 标识过期目录背后的失败，同时不改变其可用性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogDiscoveryFailureKind {
    /// The endpoint does not implement model discovery.
    /// 端点未实现模型发现。
    Unsupported,
    /// Authentication or authorization failed.
    /// 身份认证或授权失败。
    AuthenticationFailed,
    /// The provider rejected the request due to rate limiting.
    /// 供应商因限流拒绝请求。
    RateLimited,
    /// A transient network or upstream failure occurred.
    /// 发生暂时性网络或上游故障。
    TemporarilyUnavailable,
    /// The response violated the selected discovery protocol.
    /// 响应违反所选发现协议。
    InvalidResponse,
    /// The error did not match another closed category.
    /// 错误未匹配其他封闭类别。
    Failed,
}

/// Stores one process-local model-discovery observation without credential material.
/// 保存一项不含凭据材料的进程内模型发现观测。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogDiscoveryStatus {
    /// Current user-facing state.
    /// 当前面向用户的状态。
    pub state: CatalogDiscoveryState,
    /// Latest failure kind when the current state is stale.
    /// 当前状态为过期时的最近失败类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<CatalogDiscoveryFailureKind>,
    /// Controlled diagnostic message.
    /// 受控诊断消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Number of models retained for this connection.
    /// 此连接保留的模型数量。
    pub model_count: usize,
    /// Process-local catalog generation.
    /// 进程内目录代次。
    pub generation: u64,
    /// Wall-clock observation time in Unix milliseconds.
    /// Unix 毫秒墙钟观测时间。
    pub observed_at_ms: u64,
}

/// Carries one catalog observation prepared without publishing runtime or durable state.
/// 承载一项在不发布运行时或持久状态的情况下准备的目录观测。
#[derive(Debug, Clone, PartialEq)]
pub struct IsolatedConnectionCatalogObservation {
    /// Typed discovery state produced inside the isolated runtime.
    /// 在隔离运行时内产生的类型化发现状态。
    pub status: CatalogDiscoveryStatus,
    /// Raw source catalog observed before user overlays.
    /// 应用用户覆盖前观测到的原始来源目录。
    pub catalog: Option<ConnectionModelCatalog>,
    /// Controlled discovery failure retained for explicit save-policy decisions.
    /// 为显式保存策略决策保留的受控发现失败。
    pub refresh_error: Option<String>,
}

/// Identifies one validated catalog observation awaiting atomic runtime staging.
/// 标识一项等待原子运行时暂存的已验证目录观测。
#[derive(Debug, Clone, PartialEq)]
pub struct StagedConnectionCatalogObservation {
    /// Exact durable credential connection identity.
    /// 精确的持久凭据连接身份。
    pub connection_id: String,
    /// Optional raw source catalog owned by the connection.
    /// 连接持有的可选原始来源目录。
    pub catalog: Option<ConnectionModelCatalog>,
    /// Exact typed discovery state.
    /// 精确的类型化发现状态。
    pub status: CatalogDiscoveryStatus,
    /// Controlled refresh diagnostic retained for settings.
    /// 为设置保留的受控刷新诊断。
    pub refresh_error: Option<String>,
}

/// Reports whether one model contract supports bidirectional text conversation.
/// 报告一个模型契约是否支持双向文本会话。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact provider-neutral model contract.
/// `capabilities` 是精确的供应商无关模型契约。
///
/// # Returns
/// 返回值
/// `true` only when the model explicitly exposes the conversation operation; text is the base
/// payload of that operation while resource vectors intentionally contain only non-text media.
/// 仅当模型显式公开会话操作时返回 `true`；文本是该操作的基础载荷，资源向量按契约仅保存非文本媒体。
pub fn model_supports_text_conversation(capabilities: &ModelCapabilities) -> bool {
    capabilities
        .operations
        .contains(&AiOperation::ConversationRespond)
}

/// Reports whether one model contract exposes an enabled reasoning capability to the host.
/// 报告一个模型契约是否向宿主公开已启用的思考能力。
///
/// # Parameters
/// 参数
/// `capabilities` is the provider-neutral capability contract after discovery and user overlays.
/// `capabilities` 是应用发现结果与用户覆盖后的供应商无关能力契约。
///
/// # Returns
/// 返回值
/// `true` when the model is a text conversation model whose reasoning contract is supported and
/// is not explicitly fixed off. Unknown text models receive the host's enabled default policy.
/// 当模型是文本会话模型、其思考契约受支持且没有明确固定关闭时返回 `true`；未知文本模型使用宿主的默认开启策略。
pub fn model_supports_reasoning(capabilities: &ModelCapabilities) -> bool {
    let mut effective = capabilities.clone();
    apply_host_default_reasoning_policy(&mut effective);
    effective
        .operations
        .contains(&AiOperation::ConversationRespond)
        && effective.reasoning.is_supported()
        && effective.reasoning_control != ReasoningControlMode::FixedOff
}

/// Awaits one provider-task network operation without allowing it to block the recovery scheduler forever.
/// 等待一次供应商任务网络操作，同时避免其永久阻塞恢复调度器。
///
/// # Parameters
/// 参数
/// `timeout` is the exact host limit, `operation` is a controlled diagnostic label, and
/// `future` is one non-replayed provider operation.
/// `timeout` 是精确宿主限制，`operation` 是受控诊断标签，`future` 是一次不重放的供应商操作。
///
/// # Returns
/// 返回值
/// The exact operation result, or a controlled timeout diagnostic after dropping the in-flight future.
/// 返回精确操作结果，或在丢弃进行中 Future 后返回受控超时诊断。
async fn await_bounded_provider_task_operation<T, F>(
    timeout: Duration,
    operation: &str,
    future: F,
) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    tokio::time::timeout(timeout, future).await.map_err(|_| {
        format!(
            "{operation} exceeded its {} millisecond host timeout.",
            timeout.as_millis()
        )
    })?
}

/// Builds one deterministic asynchronous-task timeout diagnostic after its exact deadline.
/// 在精确截止时间后构建一条确定性的异步任务超时诊断。
///
/// # Parameters
/// 参数
/// `created_at_unix_ms`, `deadline_unix_ms`, and `now_unix_ms` are one monotonic wall-clock domain.
/// `created_at_unix_ms`、`deadline_unix_ms` 与 `now_unix_ms` 属于同一单调墙钟域。
///
/// # Returns
/// 返回值
/// A controlled diagnostic at or after the deadline, otherwise `None`.
/// 在截止时间或之后返回受控诊断，否则返回 `None`。
fn provider_task_timeout_diagnostic(
    created_at_unix_ms: u64,
    deadline_unix_ms: u64,
    now_unix_ms: u64,
) -> Option<String> {
    (now_unix_ms >= deadline_unix_ms).then(|| {
        format!(
            "Provider asynchronous task exceeded its {} millisecond deadline.",
            deadline_unix_ms.saturating_sub(created_at_unix_ms)
        )
    })
}

/// Returns the next observation sleep without crossing the frozen task deadline.
/// 返回不会越过冻结任务截止时间的下一次观测等待时长。
///
/// # Parameters
/// 参数
/// `requested_delay_ms` is the frozen backoff result; the deadline and current time share one
/// wall-clock domain.
/// `requested_delay_ms` 是冻结退避结果；截止时间与当前时间使用同一墙钟域。
///
/// # Returns
/// 返回值
/// The smaller of the backoff delay and remaining lifecycle budget, in milliseconds.
/// 返回退避时长与剩余生命周期预算中的较小值，单位为毫秒。
fn clamp_provider_task_observation_delay_ms(
    requested_delay_ms: u64,
    deadline_unix_ms: u64,
    now_unix_ms: u64,
) -> u64 {
    requested_delay_ms.min(deadline_unix_ms.saturating_sub(now_unix_ms))
}

/// Stores one process-local secondary-route discovery result.
/// 保存一份进程内二级路由发现结果。
#[derive(Debug, Clone)]
struct SecondaryRouteCacheEntry {
    /// Exact credential connection owning this cache partition.
    /// 此缓存分区所属的精确凭据连接。
    connection_id: String,
    /// Exact provider-reported route options.
    /// 供应商精确报告的路由选项。
    routes: Vec<SecondaryRouteOption>,
    /// Monotonic instant when the successful response was received.
    /// 成功收到响应时的单调时钟时间点。
    fetched_at: Instant,
}

/// Reads secret material through a host-owned secure storage boundary.
/// 通过宿主所有的安全存储边界读取秘密材料。
#[async_trait]
pub trait CredentialSecretResolver: Send + Sync {
    /// Reads one secret when present without converting absence into a storage failure.
    /// 在秘密存在时读取它，且不把缺失转换为存储失败。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` is the opaque reference saved with the credential connection.
    /// `secret_reference` 是随凭据连接保存的不透明引用。
    ///
    /// # Returns
    /// 返回值
    /// The validated secret when present, `None` when absent, or a controlled storage error.
    /// 存在时返回已验证秘密，缺失时返回 `None`，否则返回受控存储错误。
    async fn read_optional_secret(
        &self,
        secret_reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String>;

    /// Reads the strict versioned secret identified by one exact persisted reference.
    /// 读取由一个精确持久化引用标识的严格版本化秘密。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` is the opaque reference saved with the credential connection.
    /// `secret_reference` 是随凭据连接保存的不透明引用。
    ///
    /// # Returns
    /// 返回值
    /// The validated secret envelope or a controlled storage error that never includes the secret.
    /// 返回已验证秘密封装，或不包含秘密内容的受控存储错误。
    async fn read_secret(
        &self,
        secret_reference: &str,
    ) -> Result<CredentialSecretEnvelope, String> {
        self.read_optional_secret(secret_reference)
            .await?
            .ok_or_else(|| format!("AI credential `{secret_reference}` is not configured."))
    }
}

/// Mutates versioned secrets through a host-owned compare-and-replace boundary.
/// 通过宿主所有的比较替换边界修改版本化秘密。
#[async_trait]
pub trait CredentialSecretStore: CredentialSecretResolver {
    /// Creates one secret only when the opaque reference is absent.
    /// 仅在不透明引用不存在时创建一个秘密。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` is exact and `secret` must contain revision one.
    /// `secret_reference` 为精确引用，`secret` 必须包含第一修订号。
    ///
    /// # Returns
    /// 返回值
    /// Success after an exclusive secure-store create.
    /// 独占安全存储创建完成后返回成功。
    async fn create_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String>;

    /// Replaces one secret only when the stored revision matches.
    /// 仅在已存修订号匹配时替换一个秘密。
    ///
    /// # Parameters
    /// 参数
    /// `expected_revision` protects against stale refresh writes and `secret` is the next value.
    /// `expected_revision` 防止过期刷新写入，`secret` 是下一值。
    ///
    /// # Returns
    /// 返回值
    /// Success after a compare-and-replace mutation.
    /// 比较替换变更完成后返回成功。
    async fn replace_secret(
        &self,
        secret_reference: &str,
        expected_revision: u64,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String>;

    /// Deletes one exact secret.
    /// 删除一个精确秘密。
    ///
    /// # Returns
    /// 返回值
    /// Success when the secret is absent or deleted.
    /// 秘密不存在或已删除时返回成功。
    async fn delete_secret(&self, secret_reference: &str) -> Result<(), String>;

    /// Restores one exact deleted secret for transaction rollback.
    /// 为事务回滚恢复一个精确的已删除秘密。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` must be absent and `secret` is the exact prior revision.
    /// `secret_reference` 必须缺失，`secret` 是精确的先前修订版。
    ///
    /// # Returns
    /// 返回值
    /// Success only after restoring the complete prior secret.
    /// 仅在恢复完整先前秘密后返回成功。
    async fn restore_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String>;
}

/// Overlays one candidate credential in memory while delegating every unrelated reference.
/// 在内存中覆盖一份候选凭据，同时把全部无关引用委托给宿主仓库。
struct IsolatedCredentialSecretStore {
    /// Host store used only for references outside the isolated candidate.
    /// 仅用于隔离候选之外引用的宿主仓库。
    delegate: Arc<dyn CredentialSecretStore>,
    /// Exact opaque reference owned by the isolated candidate.
    /// 隔离候选持有的精确不透明引用。
    secret_reference: String,
    /// Mutable in-memory envelope supporting OAuth compare-and-replace without durable writes.
    /// 支持 OAuth 比较替换且不产生持久写入的可变内存封装。
    secret: StdMutex<Option<CredentialSecretEnvelope>>,
}

impl IsolatedCredentialSecretStore {
    /// Creates one exact in-memory credential overlay.
    /// 创建一个精确的内存凭据覆盖层。
    ///
    /// # Parameters
    /// 参数
    /// `delegate` serves unrelated references, while `secret_reference` and `secret` form the
    /// isolated candidate identity.
    /// `delegate` 服务无关引用，`secret_reference` 与 `secret` 共同构成隔离候选身份。
    ///
    /// # Returns
    /// 返回值
    /// A store whose candidate mutations cannot reach the host credential database.
    /// 返回一个候选变更无法触达宿主凭据数据库的仓库。
    fn new(
        delegate: Arc<dyn CredentialSecretStore>,
        secret_reference: String,
        secret: CredentialSecretEnvelope,
    ) -> Self {
        Self {
            delegate,
            secret_reference,
            secret: StdMutex::new(Some(secret)),
        }
    }

    /// Reports whether one reference belongs to the isolated candidate.
    /// 报告一个引用是否属于隔离候选。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` is the exact opaque lookup key.
    /// `secret_reference` 是精确的不透明查找键。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the candidate-owned reference.
    /// 仅对候选持有的引用返回 `true`。
    fn owns(&self, secret_reference: &str) -> bool {
        self.secret_reference == secret_reference
    }
}

#[async_trait]
impl CredentialSecretResolver for IsolatedCredentialSecretStore {
    /// Reads the candidate from memory or delegates an unrelated reference.
    /// 从内存读取候选，或委托一个无关引用。
    async fn read_optional_secret(
        &self,
        secret_reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        if !self.owns(secret_reference) {
            return self.delegate.read_optional_secret(secret_reference).await;
        }
        self.secret
            .lock()
            .map(|secret| secret.clone())
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))
    }
}

#[async_trait]
impl CredentialSecretStore for IsolatedCredentialSecretStore {
    /// Creates the isolated candidate only when its in-memory slot is absent.
    /// 仅在内存槽位缺失时创建隔离候选。
    async fn create_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        if !self.owns(secret_reference) {
            return self.delegate.create_secret(secret_reference, secret).await;
        }
        secret.validate()?;
        if secret.revision != 1 {
            return Err(String::from(
                "An isolated AI credential must begin at revision one.",
            ));
        }
        let mut current = self
            .secret
            .lock()
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))?;
        if current.is_some() {
            return Err(format!(
                "Isolated AI credential `{secret_reference}` is already configured."
            ));
        }
        *current = Some(secret.clone());
        Ok(())
    }

    /// Replaces the isolated candidate through exact in-memory revision comparison.
    /// 通过精确的内存修订号比较替换隔离候选。
    async fn replace_secret(
        &self,
        secret_reference: &str,
        expected_revision: u64,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        if !self.owns(secret_reference) {
            return self
                .delegate
                .replace_secret(secret_reference, expected_revision, secret)
                .await;
        }
        secret.validate()?;
        let expected_next = expected_revision
            .checked_add(1)
            .ok_or_else(|| String::from("The isolated AI credential revision is exhausted."))?;
        if secret.revision != expected_next {
            return Err(String::from(
                "Replacing an isolated AI credential must advance exactly one revision.",
            ));
        }
        let mut current = self
            .secret
            .lock()
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))?;
        let current_revision = current
            .as_ref()
            .map(|current| current.revision)
            .ok_or_else(|| {
                format!("Isolated AI credential `{secret_reference}` is not configured.")
            })?;
        if current_revision != expected_revision {
            return Err(format!(
                "Isolated AI credential `{secret_reference}` changed concurrently."
            ));
        }
        *current = Some(secret.clone());
        Ok(())
    }

    /// Deletes only the in-memory candidate while delegating unrelated references.
    /// 仅删除内存候选，同时委托无关引用。
    async fn delete_secret(&self, secret_reference: &str) -> Result<(), String> {
        if !self.owns(secret_reference) {
            return self.delegate.delete_secret(secret_reference).await;
        }
        self.secret
            .lock()
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))?
            .take();
        Ok(())
    }

    /// Restores the exact in-memory candidate only when its slot is absent.
    /// 仅在槽位缺失时恢复精确的内存候选。
    async fn restore_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        if !self.owns(secret_reference) {
            return self.delegate.restore_secret(secret_reference, secret).await;
        }
        secret.validate()?;
        let mut current = self
            .secret
            .lock()
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))?;
        if current.is_some() {
            return Err(format!(
                "Isolated AI credential `{secret_reference}` is already configured."
            ));
        }
        *current = Some(secret.clone());
        Ok(())
    }
}

/// Rejects credential reads when the host has not installed a secure resolver.
/// 在宿主尚未安装安全解析器时拒绝凭据读取。
#[derive(Debug, Default)]
pub struct UnavailableCredentialSecretResolver;

#[async_trait]
impl CredentialSecretResolver for UnavailableCredentialSecretResolver {
    /// Rejects the optional read because no secure host boundary exists.
    /// 因不存在安全宿主边界而拒绝可选读取。
    async fn read_optional_secret(
        &self,
        _secret_reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        Err(String::from(
            "The host has not installed an AI credential secret resolver.",
        ))
    }
}

#[async_trait]
impl CredentialSecretStore for UnavailableCredentialSecretResolver {
    /// Rejects secret creation because no host store exists.
    /// 因不存在宿主存储而拒绝秘密创建。
    async fn create_secret(
        &self,
        _secret_reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from(
            "The host has not installed an AI credential secret store.",
        ))
    }

    /// Rejects secret replacement because no host store exists.
    /// 因不存在宿主存储而拒绝秘密替换。
    async fn replace_secret(
        &self,
        _secret_reference: &str,
        _expected_revision: u64,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from(
            "The host has not installed an AI credential secret store.",
        ))
    }

    /// Rejects secret deletion because no host store exists.
    /// 因不存在宿主存储而拒绝秘密删除。
    async fn delete_secret(&self, _secret_reference: &str) -> Result<(), String> {
        Err(String::from(
            "The host has not installed an AI credential secret store.",
        ))
    }

    /// Rejects rollback restoration because no host store exists.
    /// 因不存在宿主存储而拒绝回滚恢复。
    async fn restore_secret(
        &self,
        _secret_reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from(
            "The host has not installed an AI credential secret store.",
        ))
    }
}

/// Owns all process-local AI state that must never be persisted with provider configuration.
/// 拥有绝不能随供应商配置持久化的全部进程内 AI 状态。
#[derive(Clone)]
pub struct PublicAiRuntime {
    /// Core-owned provider-neutral wire protocol runtime.
    /// 由核心拥有的供应商无关 Wire 协议运行时。
    protocols: Arc<LlmProtocolRuntime>,
    /// Shared execution client.
    /// 共享执行客户端。
    execution_client: ProviderExecutionClient,
    /// Shared metadata client.
    /// 共享元数据客户端。
    metadata_client: HttpProviderMetadataClient,
    /// Process-local provider OAuth transaction runtime.
    /// 进程内供应商 OAuth 事务运行时。
    oauth: Arc<OAuthAuthorizationRuntime>,
    /// Runtime-only credential-scoped model catalogs.
    /// 仅运行时存在的凭据作用域模型目录。
    catalogs: Arc<RwLock<RuntimeConnectionCatalogs>>,
    /// Immutable provider-discovery catalogs before user overlays are applied.
    /// 应用用户覆盖前的不可变供应商发现目录。
    source_catalogs: Arc<RwLock<RuntimeConnectionCatalogs>>,
    /// Effective catalogs frozen at the last explicit activation boundary
    /// (`reapply_model_catalog_overlays`); selections read these instead of rebuilding
    /// overlays at the request boundary (plan §R4.7/R4.8).
    /// 在最近一次显式激活边界（`reapply_model_catalog_overlays`）冻结的有效目录；
    /// 选择读取这些目录，而不是在请求边界重建覆盖（方案 §R4.7/R4.8）。
    activated_effective_catalogs: Arc<RwLock<RuntimeConnectionCatalogs>>,
    /// Serializes source/effective catalog replacement so overlay rebuilds cannot overwrite a newer refresh.
    /// 串行化源目录与有效目录替换，避免覆盖重建写回并覆盖更新的刷新结果。
    catalog_mutation_lock: Arc<StdMutex<()>>,
    /// Monotonic revision of every mutable catalog, status, and usage-cache publication.
    /// 每次可变目录、状态与用量缓存发布共用的单调修订号。
    catalog_state_revision: Arc<AtomicU64>,
    /// Serializes asynchronous discovery with staged manifest activation as one catalog batch.
    /// 将异步发现与暂存清单激活串行化为一个目录批次。
    catalog_refresh_flight: Arc<AsyncMutex<()>>,
    /// Latest controlled synchronization error per connection.
    /// 每个连接最近一次受控同步错误。
    catalog_errors: Arc<RwLock<BTreeMap<String, String>>>,
    /// Closed process-local discovery state per credential connection.
    /// 每个凭据连接的封闭进程内发现状态。
    catalog_statuses: Arc<RwLock<BTreeMap<String, CatalogDiscoveryStatus>>>,
    /// Process-local account-usage observations keyed by connection.
    /// 按连接索引的进程内账号用量观测。
    usage_cache: Arc<RwLock<BTreeMap<String, AccountUsageSnapshot>>>,
    /// Process-local secondary-route catalogs keyed by exact connection and model.
    /// 按精确连接与模型索引的进程内二级路由目录。
    secondary_route_cache: Arc<RwLock<BTreeMap<String, SecondaryRouteCacheEntry>>>,
    /// Connection-scoped single-flight locks for renewable credential refresh.
    /// 用于可续期凭据刷新的连接作用域单飞锁。
    refresh_locks: Arc<StdMutex<BTreeMap<String, Arc<AsyncMutex<()>>>>>,
    /// Host-owned secure secret resolver.
    /// 宿主所有的安全秘密解析器。
    secrets: Arc<dyn CredentialSecretStore>,
    /// Durable non-secret asynchronous provider-task recovery registry.
    /// 可持久化非秘密异步供应商任务恢复注册表。
    provider_tasks: ProviderTaskRecoveryStore,
    /// Staged resolved-provider catalog used only while an activation transaction is built.
    /// 仅在构建激活事务时使用的暂存已解析供应商目录。
    ///
    /// External readers must use `resolved_provider_generation`; otherwise a manifest that has
    /// not crossed the projection/activation boundary can leak into request or UI behavior.
    /// 外部读取方必须使用 `resolved_provider_generation`；否则尚未越过投影/激活边界的
    /// Manifest 可能泄漏到请求或 UI 行为中。
    bundled_manifests: std::sync::Arc<
        std::sync::RwLock<std::sync::Arc<crate::provider_manifest::ResolvedProviderCatalog>>,
    >,
    /// Atomically published provider generation used by every request-boundary selection.
    /// 每个请求边界选择共同使用的原子发布供应商代际。
    resolved_provider_generation: Arc<RwLock<Arc<ResolvedProviderGeneration>>>,
    /// Complete non-secret provider selections leased by active sessions in this process.
    /// 当前进程内活动会话租用的完整非秘密供应商选择。
    session_selection_leases: Arc<RwLock<BTreeMap<String, ResolvedPublicAiSelection>>>,
    /// Optional host observer for media prepared by understanding tools after request dispatch.
    /// 请求分派后由理解工具准备媒体时使用的可选宿主观察器。
    runtime_media_upload_progress_observer:
        Arc<RwLock<Option<crate::RuntimeMediaUploadProgressObserver>>>,
}

/// One complete immutable provider generation published at an explicit activation boundary.
/// 在显式激活边界发布的一份完整不可变供应商代际。
///
/// The generation owns every input that can influence account choice, model membership,
/// protocol routing, profiles, operations, uploads, and provider-tool resolution. Staging
/// manifest or discovery writes cannot become visible to a request until this complete value
/// is swapped as one `Arc`.
/// 该代际持有会影响账号选择、模型成员关系、协议路由、Profile、操作、上传与供应商工具
/// 解析的全部输入。暂存中的 Manifest 或发现写入只有在该完整值作为一个 `Arc` 整体交换后
/// 才能对请求可见。
#[derive(Clone)]
struct ResolvedProviderGeneration {
    /// Authoritative persisted configuration frozen with every executable provider input.
    /// 与每项可执行供应商输入共同冻结的权威持久化配置。
    configuration: Arc<PersistedAiConfiguration>,
    /// Exact immutable manifest generation used by every selection decision.
    /// 每个选择决策共同使用的精确不可变清单代际。
    manifests: Arc<crate::provider_manifest::ResolvedProviderCatalog>,
    /// Effective model catalogs frozen with the manifest generation under one mutation lock.
    /// 在同一变更锁下与清单代际共同冻结的有效模型目录。
    activated_effective_catalogs: RuntimeConnectionCatalogs,
    /// Immutable discovery catalogs paired with this activation for source-presence decisions.
    /// 与本次激活配对、用于判断来源是否存在的不可变发现目录。
    source_catalogs: RuntimeConnectionCatalogs,
    /// Account usage facts that influenced plan-scoped model membership at this activation.
    /// 在本次激活中影响套餐作用域模型成员关系的账号用量事实。
    usage_cache: BTreeMap<String, AccountUsageSnapshot>,
}

/// One fully built system-manifest catalog candidate awaiting a compare-and-swap publication.
/// 一份已完整构建、等待比较交换发布的系统 Manifest 目录候选。
#[derive(Clone)]
pub struct PreparedSystemManifestReplacement {
    /// Exact provider whose manifest was replaced in the candidate.
    /// 候选中被替换 Manifest 所属的精确供应商。
    provider_id: String,
    /// Catalog generation observed while the candidate was built.
    /// 构建候选时观察到的目录代数。
    base_generation: u64,
    /// Complete immutable candidate catalog.
    /// 完整不可变候选目录。
    catalog: Arc<crate::provider_manifest::ResolvedProviderCatalog>,
}

/// Exact before/after image of one staged system-manifest catalog replacement.
/// 一次暂存系统 Manifest 目录替换的精确前后镜像。
///
/// The catalog values stay opaque outside this runtime; compensation can restore them only
/// through the compare-and-swap method that owns the same staging lock.
/// 目录值在本运行时之外保持不透明；补偿只能通过持有同一暂存锁的比较交换方法恢复它们。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderStagedManifestCatalogMutation {
    /// Complete immutable staging catalog before replacement.
    /// 替换前的完整不可变暂存目录。
    before: Arc<crate::provider_manifest::ResolvedProviderCatalog>,
    /// Complete immutable staging catalog immediately after replacement.
    /// 替换完成后的完整不可变暂存目录。
    after: Arc<crate::provider_manifest::ResolvedProviderCatalog>,
}

/// Opaque exact before-image for one runtime Manifest/catalog activation transaction.
/// 单次运行时 Manifest/目录激活事务的不透明精确前镜像。
#[derive(Clone)]
pub struct ProviderRuntimeActivationSnapshot {
    /// Monotonic runtime state revision captured with every field below.
    /// 与以下全部字段共同捕获的单调运行时状态修订号。
    state_revision: u64,
    /// Runtime-only connection catalogs before activation.
    /// 激活前仅运行时存在的连接目录。
    catalogs: RuntimeConnectionCatalogs,
    /// Immutable discovery source catalogs before activation.
    /// 激活前的不可变发现来源目录。
    source_catalogs: RuntimeConnectionCatalogs,
    /// Effective catalogs published at the previous activation boundary.
    /// 在上一个激活边界发布的生效目录。
    activated_effective_catalogs: RuntimeConnectionCatalogs,
    /// Controlled connection errors before activation.
    /// 激活前的受控连接错误。
    catalog_errors: BTreeMap<String, String>,
    /// Closed connection discovery states before activation.
    /// 激活前的封闭连接发现状态。
    catalog_statuses: BTreeMap<String, CatalogDiscoveryStatus>,
    /// Process-local account-usage observations before activation.
    /// 激活前的进程内账号用量观测。
    usage_cache: BTreeMap<String, AccountUsageSnapshot>,
    /// Complete immutable provider generation before activation.
    /// 激活前的完整不可变供应商代际。
    resolved_provider_generation: Arc<ResolvedProviderGeneration>,
}

/// Opaque ownership proof that serializes one complete provider-catalog activation transaction.
/// 串行化一次完整供应商目录激活事务的不透明所有权证明。
pub struct ProviderCatalogActivationTransaction {
    /// Exact runtime lock identity used to reject a guard from another runtime.
    /// 用于拒绝其他运行时 Guard 的精确运行时锁身份。
    owner: Arc<AsyncMutex<()>>,
    /// Owned guard retained through runtime, Workbench, durable-state, and compensation publication.
    /// 在运行时、Workbench、持久状态与补偿发布全程持有的所有权 Guard。
    _guard: tokio::sync::OwnedMutexGuard<()>,
}

/// Opaque proof that a session set owns no process-local provider route lease.
/// 证明一组会话不持有任何进程内供应商路由租约的不透明凭据。
///
/// The write guard remains held until this value is dropped, preventing a concurrent turn from
/// installing a lease between a caller's reconciliation check and its matching state publication.
/// 写锁会一直持有到该值被释放，防止并发回合在调用方协调检查与配套状态发布之间安装租约。
pub struct SessionSelectionLeaseAbsenceGuard<'a> {
    /// Opaque exclusive lease-map guard retained across the caller's state publication.
    /// 在调用方状态发布期间保留的不透明独占租约映射 Guard。
    _guard: std::sync::RwLockWriteGuard<'a, BTreeMap<String, ResolvedPublicAiSelection>>,
}

/// Reversible exclusive transaction for explicit session route-lease revocation.
/// 用于显式撤销会话路由租约的可逆独占事务。
///
/// The write guard excludes every concurrent lease writer until the caller either commits the
/// matching Service-state publication or drops this value. An uncommitted drop restores the exact
/// removed selections before releasing the lock.
/// 写锁会排除全部并发租约写入方，直到调用方提交配套 Service 状态发布或释放本值；未提交时
/// 释放会在解锁前精确恢复所有已移除选择。
pub struct SessionSelectionLeaseReconciliationGuard<'a> {
    /// Exclusive lease map retained across the caller-owned publication boundary.
    /// 在调用方持有的发布边界期间保留的独占租约映射。
    guard: std::sync::RwLockWriteGuard<'a, BTreeMap<String, ResolvedPublicAiSelection>>,
    /// Exact selections removed at acquisition and restored on rollback.
    /// 获取 Guard 时移除并在回滚时恢复的精确选择。
    removed: BTreeMap<String, ResolvedPublicAiSelection>,
    /// Whether the paired Service-state publication completed successfully.
    /// 配套 Service 状态发布是否已成功完成。
    committed: bool,
}

impl SessionSelectionLeaseReconciliationGuard<'_> {
    /// Commits the exact lease removals and releases writer exclusion.
    /// 提交精确租约删除并释放写入排他锁。
    ///
    /// # Returns
    /// 返回值
    /// Nothing; consuming this guard makes its removed selections permanently absent.
    /// 无返回值；消费本 Guard 后，其已移除选择将永久保持缺失。
    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for SessionSelectionLeaseReconciliationGuard<'_> {
    /// Restores every removed lease when the caller abandons the paired state publication.
    /// 当调用方放弃配套状态发布时恢复全部已移除租约。
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for (session_id, selection) in std::mem::take(&mut self.removed) {
            self.guard.insert(session_id, selection);
        }
    }
}

/// One browser-settings catalog projection derived from a single immutable provider generation.
/// 从单一不可变供应商代际派生的一份浏览器设置目录投影。
pub struct PublicAiRuntimeSettingsCatalogSnapshot {
    /// Runtime-only configuration projected against the same captured manifest generation.
    /// 针对同一已捕获 Manifest 代际投影的仅运行时配置。
    pub runtime_configuration: PersistedAiConfiguration,
    /// Effective providers resolved from the captured manifest catalog.
    /// 从已捕获 Manifest 目录解析的有效供应商。
    pub providers: ProviderRegistry,
    /// Effective per-connection conversation catalogs under current access policy.
    /// 当前访问策略下的逐连接有效对话目录。
    pub catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Access-independent per-connection conversation support catalogs.
    /// 独立于访问开关的逐连接对话支持目录。
    pub model_support_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Discovery source catalogs frozen before user overlays.
    /// 应用用户覆盖前冻结的发现来源目录。
    pub source_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Exact manifest revision per provider in the captured generation.
    /// 已捕获代际中每个供应商的精确 Manifest 修订号。
    pub manifest_revisions: BTreeMap<String, u64>,
    /// Enablement-independent provider-tool template model management inventory.
    /// 独立于启用状态的供应商工具模板模型管理目录。
    pub provider_tool_template_models: Vec<DeclaredProviderToolTemplateModel>,
    /// Inherited model access defaults keyed by the canonical provider/model identity.
    /// 按规范供应商/模型身份索引的继承模型访问默认值。
    pub model_default_enabled_by_key: BTreeMap<String, bool>,
    /// Provider-tool targets currently eligible under credential and model access policy.
    /// 当前在凭据与模型访问策略下合格的供应商工具目标。
    pub available_provider_tool_targets: Vec<AvailableProviderToolTarget>,
}

/// One coherent manifest plus model-catalog snapshot for provider diagnostics and preview.
/// 用于供应商诊断与预览的一份一致 Manifest 与模型目录快照。
#[derive(Clone)]
pub struct ProviderManifestRuntimeSnapshot {
    /// Exact compiled system manifest from the captured published generation.
    /// 来自已捕获发布代际的精确已编译系统 Manifest。
    pub manifest: Arc<ResolvedProviderManifest>,
    /// Effective connection catalogs used by request selection in the same generation.
    /// 同一代际中请求选择使用的有效连接目录。
    pub effective_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Access-independent model-support catalogs reconstructed from the same generation.
    /// 从同一代际重建且独立于账号访问开关的模型支持目录。
    pub model_support_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Discovery source catalogs paired with the same generation before user overlays.
    /// 与同一代际配对且位于用户覆盖之前的发现来源目录。
    pub source_catalogs: BTreeMap<String, ConnectionModelCatalog>,
    /// Exact manifest catalog generation captured with every field above.
    /// 与以上全部字段共同捕获的精确 Manifest 目录代际。
    pub manifest_generation: u64,
    /// Content-addressed digest of the complete captured manifest catalog.
    /// 完整已捕获 Manifest 目录的内容寻址摘要。
    pub catalog_revision: String,
}

/// Carries one successful remote Responses compaction result with its owning route identity.
/// 携带一份成功的远端 Responses 压缩结果及其归属路由身份。
#[derive(Debug, Clone)]
pub struct ResponsesCompactExecution {
    /// Ordered opaque compaction output returned by the provider.
    /// 供应商返回的有序不透明压缩输出。
    pub result: crate::ResponsesCompactResult,
    /// Exact provider identifier.
    /// 精确供应商标识。
    pub provider_id: String,
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    pub connection_id: String,
    /// Non-secret resolved Responses route fingerprint.
    /// 不含秘密的已解析 Responses 路由指纹。
    pub route_fingerprint: String,
    /// Exact protocol profile identifier.
    /// 精确协议配置标识。
    pub protocol_profile_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    pub model_id: String,
}

/// Validates one immutable execution route for the standalone Codex compaction endpoint.
/// 校验一条用于独立 Codex 压缩端点的不可变执行路由。
///
/// # Parameters
/// 参数
/// `execution` is the exact credential-bearing route prepared for one compact request.
/// `execution` 是为单次压缩请求准备的精确携带凭据路由。
///
/// # Returns
/// 返回值
/// Success only for an explicitly enabled Codex Responses route, otherwise a pre-dispatch failure.
/// 仅对显式启用的 Codex Responses 路由返回成功，否则返回分派前失败。
fn validate_responses_compact_execution_route(
    execution: &ResolvedPublicAiExecution,
) -> Result<(), ProviderExecutionFailure> {
    if execution.route.protocol_route.protocol_profile_id
        != vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID
    {
        return Err(crate::execution::pre_dispatch_failure(
            "Remote Responses compaction requires the Codex Responses protocol.",
        ));
    }
    // enabled requires an explicit per-route opt-in because compatibility providers vary.
    // enabled 要求逐路由显式开启，因为兼容供应商的能力并不一致。
    let enabled = execution.route.protocol_route.remote_compaction_enabled();
    if !enabled {
        return Err(crate::execution::pre_dispatch_failure(
            "Remote Responses compaction is disabled for the selected provider route.",
        ));
    }
    Ok(())
}

/// Resolves one model's inherited default from an exact manifest snapshot and manual declaration.
/// 根据一份精确清单快照与手工声明解析一个模型的继承默认值。
///
/// # Parameters
/// 参数
/// `configuration` owns manual models; provider, service, credential, and model identities select
/// the exact scope; `manifest` is captured from the same immutable selection generation.
/// `configuration` 持有手工模型；供应商、服务、凭据与模型身份选择精确作用域；`manifest`
/// 从同一不可变选择代际捕获。
///
/// # Returns
/// 返回值
/// A manual model's declared default, then the selected manifest default. Undeclared models fail
/// closed when no manifest exists.
/// 依次返回手工模型声明默认值、所选 Manifest 默认值；无 Manifest 时未声明模型失败关闭。
fn runtime_model_default_enabled_from_manifest(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    manifest: Option<&ResolvedProviderManifest>,
) -> bool {
    configuration
        .custom_model(provider_id, model_id)
        .map_or_else(
            || {
                manifest.is_some_and(|manifest| {
                    crate::provider_manifest::manifest_default_enabled(
                        manifest,
                        service_entry_id,
                        credential_profile_id,
                        model_id,
                    )
                })
            },
            |model| model.enabled,
        )
}

/// Resolves model enablement from one exact manifest snapshot and persisted user ownership.
/// 根据一份精确清单快照与持久化用户所有权解析模型启用状态。
///
/// # Parameters
/// 参数
/// `configuration` owns user overrides/manual models; provider, service, credential, and model
/// identities select the exact scope; `manifest` is captured from the same immutable generation.
/// `configuration` 持有用户覆盖/手动模型；供应商、服务、凭据与模型身份选择精确作用域；
/// `manifest` 从同一不可变代际捕获。
///
/// # Returns
/// 返回值
/// Explicit user/manual-model ownership first, then the selected manifest default. Undeclared
/// models fail closed when no manifest exists.
/// 依次返回显式用户/手工模型所有权与所选清单默认值；无 Manifest 时未声明模型失败关闭。
fn runtime_model_is_enabled_from_manifest(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    manifest: Option<&ResolvedProviderManifest>,
) -> bool {
    // inherited_default is derived from the same frozen manifest used for membership and routing.
    // inherited_default 从与成员关系和路由相同的冻结 Manifest 派生。
    let inherited_default = runtime_model_default_enabled_from_manifest(
        configuration,
        provider_id,
        service_entry_id,
        credential_profile_id,
        model_id,
        manifest,
    );
    configuration.model_is_enabled_with_default(provider_id, model_id, inherited_default)
}

/// Builds the complete manifest catalog published at one explicit configuration activation boundary.
/// 构建在一次显式配置激活边界整体发布的完整 Manifest 目录。
///
/// # Parameters
/// 参数
/// `configuration` owns validated custom manifests, `system_manifests` is the staged immutable
/// system catalog, and `current_manifests` is the previously published complete catalog.
/// `configuration` 持有已校验的自定义 Manifest，`system_manifests` 是已暂存的不可变系统目录，
/// `current_manifests` 是此前发布的完整目录。
///
/// # Returns
/// 返回值
/// A collision-free catalog containing the exact system and custom manifests for the next generation.
/// 返回一份无标识冲突、包含下一代精确系统与自定义 Manifest 的目录。
fn build_activated_provider_manifest_catalog(
    configuration: &PersistedAiConfiguration,
    system_manifests: &crate::provider_manifest::ResolvedProviderCatalog,
    current_manifests: &crate::provider_manifest::ResolvedProviderCatalog,
) -> Result<Arc<crate::provider_manifest::ResolvedProviderCatalog>, String> {
    let mut manifests = system_manifests.manifests.clone();
    // systemProviderIds includes healthy and provider-locally failed identities pinned by the
    // trusted bundled index. An isolated system baseline must not open its namespace to a custom
    // manifest during recovery.
    // systemProviderIds 包含可信内置 Index 固定的健康身份与逐供应商失败身份。系统基线被
    // 隔离时不得在恢复期间向自定义 Manifest 开放其命名空间。
    let system_provider_ids = system_manifests.bundled_provider_ids();
    for (provider_id, manifest) in &configuration.custom_provider_manifests {
        let descriptor = configuration.custom_providers.get(provider_id).ok_or_else(|| {
            format!(
                "Custom provider manifest `{provider_id}` has no matching custom provider descriptor."
            )
        })?;
        let compiled = crate::provider_manifest::compile_manifest(
            manifest,
            crate::provider_manifest::ManifestOwnership::Custom,
        )
        .map_err(|error| format!("Custom provider manifest `{provider_id}` is invalid: {error}"))?;
        if !crate::provider_manifest::custom_manifest_matches_activated_descriptor(
            compiled.provider(),
            descriptor,
        ) {
            return Err(format!(
                "Custom provider manifest `{provider_id}` does not match its activated provider descriptor."
            ));
        }
        if system_provider_ids.contains(provider_id) {
            return Err(format!(
                "Custom provider manifest `{provider_id}` collides with a trusted system provider identifier."
            ));
        }
        manifests.insert(provider_id.clone(), Arc::new(compiled));
    }

    let next_generation = current_manifests
        .generation
        .checked_add(1)
        .ok_or_else(|| {
            String::from("The AI manifest catalog generation exhausted the supported u64 range.")
        })?
        .max(system_manifests.generation);
    let candidate = crate::provider_manifest::ResolvedProviderCatalog::build_with_bundled_failures(
        manifests,
        system_manifests.bundled_failures.clone(),
        next_generation,
    )?;
    if candidate.content_sha256 == current_manifests.content_sha256 {
        return Ok(Arc::new(current_manifests.clone()));
    }
    Ok(Arc::new(candidate))
}

/// Builds the runtime-only configuration projection for one exact manifest catalog.
/// 为一份精确 Manifest 目录构建仅运行时配置投影。
///
/// Invalid standalone custom manifests and system providers with no executable bundled/Active/
/// Previous layer remain persisted for repair but cannot participate in catalog publication.
/// 无效独立自定义 Manifest，以及没有可执行 bundled/Active/Previous 层的系统供应商，继续
/// 持久化以供修复，但不能参与目录发布。
///
/// # Parameters
/// 参数
/// `configuration` is the authoritative persisted image and `manifests` is the immutable target
/// generation whose executable authority determines isolation.
/// `configuration` 是权威持久化镜像，`manifests` 是由可执行权威决定隔离状态的不可变目标代际。
///
/// # Returns
/// 返回值
/// A detached coherent configuration containing only providers executable in this generation.
/// 返回一份独立一致配置，仅包含当前代际可执行的供应商。
fn runtime_configuration_for_manifest_catalog(
    configuration: &PersistedAiConfiguration,
    manifests: &crate::provider_manifest::ResolvedProviderCatalog,
) -> PersistedAiConfiguration {
    // isolatedSystemProviderIds includes only providers still lacking a recovered executable layer.
    // isolatedSystemProviderIds 仅包含仍缺少恢复后可执行层的供应商。
    let isolated_system_provider_ids = manifests
        .bundled_failures
        .keys()
        .filter(|provider_id| manifests.manifest(provider_id.as_str()).is_none())
        .cloned()
        .collect::<BTreeSet<_>>();
    // customProjection first removes standalone custom failures using their persisted authority.
    // customProjection 首先依据持久化权威移除独立自定义失败。
    let custom_projection = configuration.without_isolated_custom_provider_runtime_state();
    custom_projection.without_provider_runtime_state(&isolated_system_provider_ids)
}

/// Builds the exact effective-catalog view owned by one immutable published generation.
/// 构建一份不可变已发布代际持有的精确有效目录视图。
///
/// # Parameters
/// 参数
/// `generation` owns the exact activated catalogs captured at the same activation boundary.
/// `generation` 持有在同一激活边界捕获的精确已激活目录。
///
/// # Returns
/// 返回值
/// The exact activated connection catalog map.
/// 返回精确的已激活连接目录映射。
fn effective_catalog_snapshot_from_generation(
    generation: &ResolvedProviderGeneration,
) -> BTreeMap<String, ConnectionModelCatalog> {
    generation.activated_effective_catalogs.snapshot()
}

/// Stable schema marker for a source catalog that contains only user-managed manual identities.
/// 仅包含用户管理手工身份的源目录稳定 Schema 标记。
const MANUAL_MODEL_CATALOG_SCHEMA_REVISION: &str = "manual.models.v1";

/// Resolves the exact model-source policy for one active provider service entry.
/// 解析一个生效供应商服务入口的精确模型来源策略。
///
/// # Parameters
/// 参数
/// `configuration` owns custom descriptors, `provider_id` and `service_entry_id` select the
/// exact entry, and `manifest` is the optional compiled authority for that provider.
/// `configuration` 持有自定义描述；`provider_id` 与 `service_entry_id` 选择精确入口；
/// `manifest` 是该供应商可选的编译权威。
///
/// # Returns
/// 返回值
/// Returns the explicit Manifest policy or the closed custom-provider policy derived from its
/// declared discovery mode. A missing non-custom authority fails closed.
/// 返回显式 Manifest 策略，或从自定义供应商已声明发现模式派生的封闭策略；
/// 缺失的非自定义权威一律失败关闭。
fn resolved_model_source_policy(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    manifest: Option<&ResolvedProviderManifest>,
) -> Result<ModelSourcePolicy, String> {
    if let Some(manifest) = manifest {
        return manifest.model_source_policy(service_entry_id);
    }
    let entry = resolve_custom_model_service_entry(configuration, provider_id, service_entry_id)?;
    // Custom providers may declare a distinct discovery contract per protocol route. The source
    // policy therefore aggregates every verified route instead of inheriting the first route's
    // compatibility projection; any remote route keeps hot model discovery enabled.
    // 自定义供应商可以为每条协议路由声明不同的发现契约。因此来源策略聚合全部已验证路由，
    // 而不是继承首条路由的兼容投影；任一远端路由都会保持模型热发现能力。
    // hasManual and hasRemote retain the complete verified route-set classification.
    // hasManual 与 hasRemote 保留完整已验证路由集合的分类结果。
    let mut has_manual = false;
    let mut has_remote = false;
    for route in entry.protocol_routes.iter().filter(|route| route.verified) {
        match route.resolved_model_discovery(entry.model_discovery) {
            ModelDiscoveryKind::Manual => has_manual = true,
            ModelDiscoveryKind::ProviderToolsOnly => {
                return Err(format!(
                    "Provider-tools service entry `{service_entry_id}` does not own a conversation model source policy."
                ));
            }
            ModelDiscoveryKind::ManifestStatic => {
                return Err(format!(
                    "Custom provider `{provider_id}` cannot use Manifest-static model identity without an active Manifest."
                ));
            }
            ModelDiscoveryKind::OpenRouter
            | ModelDiscoveryKind::OpenAi
            | ModelDiscoveryKind::OpenAiCodex
            | ModelDiscoveryKind::Anthropic
            | ModelDiscoveryKind::AnthropicCompatible
            | ModelDiscoveryKind::Gemini
            | ModelDiscoveryKind::GoogleCodeAssist => has_remote = true,
        }
    }
    if has_remote {
        Ok(ModelSourcePolicy::DiscoveryAndManual)
    } else if has_manual {
        Ok(ModelSourcePolicy::ManualOnly)
    } else {
        Err(format!(
            "Custom provider `{provider_id}` service entry `{service_entry_id}` has no verified model discovery route."
        ))
    }
}

/// Resolves the exact custom-provider conversation entry used by model catalog projection.
/// 解析模型目录投影使用的精确自定义供应商会话入口。
///
/// # Parameters
/// 参数
/// `configuration` owns custom descriptors, `provider_id` selects one provider, and
/// `service_entry_id` is exact for credential-scoped calls or empty only for provider-level
/// validation without a credential.
/// `configuration` 持有自定义描述；`provider_id` 选择供应商；`service_entry_id` 在凭据作用域
/// 调用中必须精确，仅无凭据的供应商级校验可传空值。
///
/// # Returns
/// 返回值
/// The exact entry, or an explicit ambiguity/missing-authority error.
/// 返回精确入口，或明确的歧义/缺少权威错误。
fn resolve_custom_model_service_entry<'a>(
    configuration: &'a PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
) -> Result<&'a crate::ServiceEntry, String> {
    let provider = configuration.custom_providers.get(provider_id).ok_or_else(|| {
        format!(
            "Provider `{provider_id}` has no active Manifest or custom descriptor for model-source resolution."
        )
    })?;
    if !service_entry_id.is_empty() {
        return provider
            .service_entries
            .iter()
            .find(|entry| entry.id == service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Custom provider `{provider_id}` has no service entry `{service_entry_id}` for model-source resolution."
                )
            });
    }
    // Provider-level validation is credential-free only when exactly one conversation entry owns
    // the model namespace; selecting an arbitrary entry would make capability validation diverge
    // from request execution.
    // 仅当恰好一个会话入口拥有模型命名空间时，供应商级校验才可不依赖凭据；任意选择入口会使
    // 能力校验与请求执行发生偏离。
    let mut conversation_entries = provider
        .service_entries
        .iter()
        .filter(|entry| entry.kind == crate::ServiceEntryKind::Conversation);
    let entry = conversation_entries.next().ok_or_else(|| {
        format!(
            "Custom provider `{provider_id}` has no conversation service entry for provider-level model-source validation."
        )
    })?;
    if conversation_entries.next().is_some() {
        return Err(format!(
            "Custom provider `{provider_id}` has multiple conversation service entries; provider-level model-source validation requires an exact credential scope."
        ));
    }
    Ok(entry)
}

/// Reports whether one discovery implementation performs remote provider I/O.
/// 报告一种发现实现是否执行远端供应商 I/O。
///
/// # Parameters
/// 参数
/// `kind` is the exact compiled discovery implementation.
/// `kind` 是精确编译后的发现实现。
///
/// # Returns
/// 返回值
/// `true` only for network-backed discovery kinds.
/// 仅对网络驱动的发现类别返回 `true`。
const fn model_discovery_is_remote(kind: ModelDiscoveryKind) -> bool {
    matches!(
        kind,
        ModelDiscoveryKind::OpenRouter
            | ModelDiscoveryKind::OpenAi
            | ModelDiscoveryKind::OpenAiCodex
            | ModelDiscoveryKind::Anthropic
            | ModelDiscoveryKind::AnthropicCompatible
            | ModelDiscoveryKind::Gemini
            | ModelDiscoveryKind::GoogleCodeAssist
    )
}

/// Builds an empty credential-scoped source catalog with one exact source marker.
/// 使用一个精确来源标记构建空的凭据作用域源目录。
///
/// # Parameters
/// 参数
/// `connection_id`, `generation`, and `schema_revision` are the exact runtime identities.
/// `connection_id`、`generation` 与 `schema_revision` 是精确运行时身份。
///
/// # Returns
/// 返回值
/// A bounded empty catalog ready for deterministic overlay reconstruction.
/// 返回可用于确定性覆盖重建的有界空目录。
fn empty_model_source_catalog(
    connection_id: &str,
    generation: u64,
    schema_revision: &str,
) -> ConnectionModelCatalog {
    ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models: Vec::new(),
        generation,
        page_count: 0,
        discovery_schema_revision: schema_revision.to_string(),
    }
}

/// Projects signed Manifest protocol variants onto one exact model identity and credential scope.
/// 把签名 Manifest 协议变体投影到一个精确模型身份与凭据作用域。
///
/// # Parameters
/// 参数
/// `manifest` owns routing/security facts; `connection` selects the exact entry/profile;
/// `generation` freezes catalog provenance; `model` supplies discovery or manual evidence;
/// `preserve_display_name` retains a user-owned manual display label after signed capability projection.
/// `manifest` 持有路由/安全事实；`connection` 选择精确入口/配置档；`generation` 冻结目录来源；
/// `model` 提供发现或手工证据；`preserve_display_name` 在签名能力投影后保留用户持有的手工显示名。
///
/// # Returns
/// 返回值
/// The number of verified supported variants projected; zero means the identity has no executable
/// signed binding under `declared_rule_required`.
/// 返回已投影的已验证受支持变体数量；零表示该身份在 `declared_rule_required` 下没有可执行
/// 签名绑定。
fn project_manifest_protocol_variants_for_connection(
    manifest: &ResolvedProviderManifest,
    connection: &CredentialConnection,
    generation: u64,
    model: &mut ConnectionModel,
    preserve_display_name: bool,
) -> Result<usize, String> {
    let entry = manifest
        .descriptor
        .service_entries
        .iter()
        .find(|entry| entry.id == connection.service_entry_id)
        .ok_or_else(|| {
            format!(
                "Provider `{}` has no service entry `{}` for Manifest protocol projection.",
                manifest.descriptor.id, connection.service_entry_id
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
    // entryDefaults includes the exact service-level protocol override compiled with this manifest.
    // entryDefaults 包含与该 Manifest 共同编译的精确服务入口级协议覆盖。
    let entry_defaults = manifest.model_defaults_for_entry(entry.id.as_str())?;
    let rule = manifest.rule_for_model(
        entry.id.as_str(),
        Some(connection.credential_profile_id.as_str()),
        model.id.as_str(),
    );
    if rule.is_none()
        && manifest.unknown_model_policy
            == crate::provider_manifest::UnknownModelPolicy::DeclaredRuleRequired
    {
        model.protocol_variants.clear();
        return Ok(0);
    }
    let supported_protocols = rule
        .map(|rule| rule.supported_protocols.as_slice())
        .unwrap_or(entry_defaults.supported_protocols.as_slice());
    let original_display_name = model.display_name.clone();
    let mut variants = BTreeMap::new();
    let mut compatibility_projection = None;
    for route in entry.protocol_routes.iter().filter(|route| route.verified) {
        if !supported_protocols
            .iter()
            .any(|protocol| protocol == &route.protocol_profile_id)
        {
            continue;
        }
        // protocolModel is isolated so protocol-scoped capability fields cannot cross routes.
        // protocolModel 相互隔离，使协议作用域能力字段不能跨路由泄漏。
        let mut protocol_model = model.clone();
        protocol_model.protocol_variants.clear();
        if rule.is_some() {
            if !crate::discovery::apply_manifest_model_rule(
                manifest,
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                route.protocol_profile_id.as_str(),
                &mut protocol_model,
            ) {
                return Err(format!(
                    "Manifest model `{}` lost its compiled rule in service entry `{}`.",
                    model.id, entry.id
                ));
            }
        } else if let Some(default_capabilities) = &entry_defaults.capabilities {
            // Unknown-model default binding is an explicit signed authority; its complete compiled
            // capability snapshot wins over untrusted manual evidence.
            // 未知模型默认绑定是显式签名权威；其完整编译能力快照高于不可信手工证据。
            protocol_model.capabilities = default_capabilities.clone();
        }
        if preserve_display_name {
            protocol_model.display_name = original_display_name.clone();
        }
        let authentication =
            route.resolved_authentication(entry.authentication, credential_profile.authentication);
        let discovery_url = route.model_discovery_url(entry, "");
        let discovery_signature = crate::discovery_signature::discovery_signature(
            ModelDiscoveryKind::ManifestStatic,
            discovery_url.as_str(),
            connection.credential_profile_id.as_str(),
            &authentication,
            &route.request_headers,
        );
        variants.insert(
            route.protocol_profile_id.clone(),
            ProtocolModelVariant {
                capabilities: protocol_model.capabilities.clone(),
                capability_evidence: protocol_model.capability_evidence.clone(),
                discovery_signature,
                catalog_generation: generation,
            },
        );
        compatibility_projection.get_or_insert(protocol_model);
    }
    if let Some(projected) = compatibility_projection {
        model.display_name = projected.display_name;
        model.capabilities = projected.capabilities;
        model.capability_evidence = projected.capability_evidence;
    }
    model.protocol_variants = variants;
    Ok(model.protocol_variants.len())
}

/// Builds the complete Manifest-owned identity catalog for one exact credential connection.
/// 为一个精确凭据连接构建完整的 Manifest 持有身份目录。
///
/// # Parameters
/// 参数
/// `manifest` is the immutable identity and capability authority; `connection` supplies the exact
/// entry/profile scope; `generation` is the process-local catalog generation.
/// `manifest` 是不可变身份与能力权威；`connection` 提供精确入口/配置档作用域；
/// `generation` 是进程内目录代次。
///
/// # Returns
/// 返回值
/// A static catalog with one independent protocol variant per verified supported route.
/// 返回静态目录，其中每条已验证且受支持的路由都有独立协议变体。
fn build_manifest_identity_catalog(
    manifest: &ResolvedProviderManifest,
    connection: &CredentialConnection,
    generation: u64,
) -> Result<ConnectionModelCatalog, String> {
    let entry = manifest
        .descriptor
        .service_entries
        .iter()
        .find(|entry| entry.id == connection.service_entry_id)
        .ok_or_else(|| {
            format!(
                "Provider `{}` has no service entry `{}` for Manifest identity projection.",
                manifest.descriptor.id, connection.service_entry_id
            )
        })?;
    // declaredModels is an explicit empty inventory when the policy includes no static identities.
    // declaredModels 在策略不含任何静态身份时是显式空清单。
    let declared_models = manifest
        .catalog_models
        .get(entry.id.as_str())
        .map(Vec::as_slice)
        .unwrap_or_default();
    let entry_defaults = manifest.model_defaults_for_entry(entry.id.as_str())?;
    // models is keyed by exact upstream identity to guarantee deterministic merging.
    // models 按精确上游身份索引，以保证确定性合并。
    let mut models = BTreeMap::<String, ConnectionModel>::new();
    for declared in declared_models {
        // model starts from the compiled default and each route receives its own cloned projection.
        // model 从编译默认值开始，每条路由接收自己的克隆投影。
        let mut model = ConnectionModel {
            id: declared.model_id.clone(),
            display_name: declared.display_name.clone(),
            capabilities: entry_defaults.capabilities.clone().unwrap_or_default(),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: None,
        };
        if project_manifest_protocol_variants_for_connection(
            manifest, connection, generation, &mut model, false,
        )? == 0
        {
            return Err(format!(
                "Manifest model `{}` has no verified supported protocol variant in service entry `{}`.",
                declared.model_id, entry.id
            ));
        }
        models.insert(model.id.clone(), model);
    }
    Ok(ConnectionModelCatalog {
        connection_id: connection.id.clone(),
        models: models.into_values().collect(),
        generation,
        page_count: u32::from(!declared_models.is_empty()),
        discovery_schema_revision: String::from(
            crate::client::MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION,
        ),
    })
}

/// Merges Manifest-owned identities into a source-derived catalog without replacing discovery evidence.
/// 将 Manifest 持有身份合并进来源目录，且不替换发现证据。
///
/// # Parameters
/// 参数
/// `catalog` owns discovery/manual evidence and `manifest_catalog` owns signed identity variants.
/// `catalog` 持有发现/手工证据；`manifest_catalog` 持有签名身份变体。
fn merge_manifest_identity_catalog(
    catalog: &mut ConnectionModelCatalog,
    manifest_catalog: ConnectionModelCatalog,
) {
    // models preserves source-derived top-level evidence and fills only absent signed variants.
    // models 保留来源派生的顶层证据，仅补入缺失的签名变体。
    let mut models = catalog
        .models
        .drain(..)
        .map(|model| (model.id.clone(), model))
        .collect::<BTreeMap<_, _>>();
    for manifest_model in manifest_catalog.models {
        match models.get_mut(manifest_model.id.as_str()) {
            Some(source_model) => {
                for (protocol_id, variant) in manifest_model.protocol_variants {
                    source_model
                        .protocol_variants
                        .entry(protocol_id)
                        .or_insert(variant);
                }
            }
            None => {
                models.insert(manifest_model.id.clone(), manifest_model);
            }
        }
    }
    catalog.models = models.into_values().collect();
}

/// Applies one catalog overlay pass from one exact immutable provider generation.
/// 从一个精确不可变供应商代际应用一次目录覆盖。
///
/// # Parameters
/// 参数
/// `configuration` owns user policy; provider and service identifiers select the exact rule
/// scope; `account_usage` is frozen account evidence; `generation` owns the matching Manifest;
/// and `catalog` is rebuilt in place.
/// `configuration` 持有用户策略；供应商与服务标识选择精确规则作用域；`account_usage`
/// 是冻结账号证据；`generation` 持有匹配 Manifest；`catalog` 会被原位重建。
///
/// # Returns
/// 返回值
/// Success after source policy and every overlay resolve from the same generation.
/// 来源策略与全部覆盖均从同一代际解析完成后返回成功。
fn apply_model_catalog_overlays_from_generation(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    account_usage: Option<&AccountUsageSnapshot>,
    generation: &ResolvedProviderGeneration,
    catalog: &mut ConnectionModelCatalog,
) -> Result<(), String> {
    let manifest = generation.manifests.manifest(provider_id);
    let source_policy =
        resolved_model_source_policy(configuration, provider_id, service_entry_id, manifest)?;
    apply_model_catalog_overlays_with_account_access(
        configuration,
        provider_id,
        service_entry_id,
        account_usage,
        source_policy,
        manifest,
        catalog,
    )
}

/// Resolves candidate model overlays from one explicit immutable input set.
/// 从一组显式不可变输入解析候选模型覆盖。
///
/// # Parameters
/// 参数
/// `configuration` supplies candidate user/account policy; `manifests`, `source_catalogs`, and
/// `usage_cache` must all come from the same published generation or the same locked activation.
/// `configuration` 提供候选用户/账号策略；`manifests`、`source_catalogs` 与 `usage_cache` 必须
/// 全部来自同一已发布代际或同一次加锁激活。
///
/// # Returns
/// 返回值
/// A complete effective catalog set after every model rule, access layer, capability layer, and
/// strict size limit is applied.
/// 返回应用全部模型规则、访问层、能力层与严格大小限制后的完整有效目录集合。
fn resolve_model_catalog_overlays_from_inputs(
    configuration: &PersistedAiConfiguration,
    manifests: &crate::provider_manifest::ResolvedProviderCatalog,
    source_catalogs: &RuntimeConnectionCatalogs,
    usage_cache: &BTreeMap<String, AccountUsageSnapshot>,
) -> Result<RuntimeConnectionCatalogs, String> {
    // effective accumulates only catalogs reconstructable from the explicit input set.
    // effective 仅累积可从显式输入集合重建的目录。
    let mut effective = RuntimeConnectionCatalogs::default();
    for connection in configuration.connection_graph.connections.values() {
        // An invalid standalone custom Manifest isolates only its provider. The persisted
        // connection remains available for repair, but it must never enter an executable catalog.
        // 无效的独立自定义 Manifest 只隔离其所属供应商。持久化连接仍保留供修复，但绝不能
        // 进入可执行目录。
        if configuration.custom_provider_manifest_is_isolated(connection.provider_id.as_str()) {
            continue;
        }
        // manifest and sourcePolicy come from the same immutable catalog supplied to this pass.
        // manifest 与 sourcePolicy 来自本次解析传入的同一不可变目录。
        let manifest = manifests.manifest(connection.provider_id.as_str());
        if manifest.is_some_and(|manifest| {
            manifest
                .descriptor
                .service_entries
                .iter()
                .find(|entry| entry.id == connection.service_entry_id)
                .is_some_and(|entry| entry.kind == crate::ServiceEntryKind::ProviderTools)
        }) {
            continue;
        }
        let source_policy = resolved_model_source_policy(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            manifest,
        )?;
        // catalog starts from frozen source evidence. Manifest/manual policies may independently
        // reconstruct a baseline when no discovery source has ever been published.
        // catalog 从冻结来源证据开始；从未发布发现来源时，Manifest/手工策略可独立重建基线。
        let mut catalog = match source_catalogs.get(connection.id.as_str()).cloned() {
            Some(catalog) => catalog,
            None if source_policy.includes_manifest() => empty_model_source_catalog(
                connection.id.as_str(),
                1,
                crate::client::MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION,
            ),
            None if source_policy.includes_manual() => {
                let Some(catalog) = manual_custom_catalog(configuration, connection, 1) else {
                    continue;
                };
                catalog
            }
            None => continue,
        };
        // account_usage is frozen with the same source generation as every catalog source.
        // account_usage 与所有目录来源冻结在同一个来源代际中。
        let account_usage = usage_cache.get(connection.id.as_str());
        apply_model_catalog_overlays_with_account_access(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            account_usage,
            source_policy,
            manifest,
            &mut catalog,
        )?;
        validate_resolved_catalog_limits(&catalog)?;
        effective.replace(catalog);
    }
    Ok(effective)
}

/// Reports conversation-model support against one exact immutable provider generation.
/// 针对一个精确不可变供应商代际报告对话模型支持状态。
///
/// # Parameters
/// 参数
/// `configuration` owns manual declarations and connection membership; `provider_id` and
/// `model_id` select the model; `generation` freezes discovery, plans, and manifest rules.
/// `configuration` 持有手工声明与连接成员关系；`provider_id` 和 `model_id` 选择模型；
/// `generation` 冻结发现、套餐与 Manifest 规则。
///
/// # Returns
/// 返回值
/// `true` when a manual declaration or any access-independent account catalog contains the model.
/// 当手工声明或任一独立于访问开关的账号目录包含该模型时返回 `true`。
fn provider_conversation_model_is_supported_in_generation(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
    generation: &ResolvedProviderGeneration,
) -> Result<bool, String> {
    let runtime_configuration =
        runtime_configuration_for_manifest_catalog(configuration, generation.manifests.as_ref());
    let configuration = &runtime_configuration;
    if configuration.custom_model(provider_id, model_id).is_some() {
        return Ok(true);
    }
    // support_configuration removes only account access policy; provider membership remains authoritative.
    // support_configuration 只移除账号访问策略；供应商成员关系仍保持权威。
    let mut support_configuration = configuration.clone();
    support_configuration
        .connection_model_access_policies
        .clear();
    // catalogs reconstructs support from the same frozen source, plan, and manifest generation.
    // catalogs 从同一冻结来源、套餐与 Manifest 代际重建支持目录。
    let catalogs = resolve_model_catalog_overlays_from_inputs(
        &support_configuration,
        generation.manifests.as_ref(),
        &generation.source_catalogs,
        &generation.usage_cache,
    )?;
    Ok(support_configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| connection.provider_id == provider_id)
        .any(|connection| {
            catalogs
                .get(connection.id.as_str())
                .is_some_and(|catalog| catalog.models.iter().any(|model| model.id == model_id))
        }))
}

/// Lists every credential-eligible model declared by reviewed provider-tool templates in one generation.
/// 列出一个代际中由已审核供应商工具模板声明且凭据合格的全部模型。
///
/// # Parameters
/// 参数
/// `configuration` supplies configured credentials and pending-validation state; `generation`
/// freezes the manifests, template provenance, and concrete target contracts for the whole scan.
/// `configuration` 提供已配置凭据与待验证状态；`generation` 为整次扫描冻结 Manifest、模板来源
/// 与具体目标契约。
///
/// # Returns
/// 返回值
/// A deterministic connection/model list independent from provider-level model enablement, or an
/// explicit invariant failure when compiled provenance cannot resolve its target.
/// 返回与供应商级模型启用状态无关的确定性连接/模型列表；若编译来源无法解析其目标则返回显式
/// 不变量错误。
fn provider_tool_template_models_in_generation(
    configuration: &PersistedAiConfiguration,
    generation: &ResolvedProviderGeneration,
) -> Result<Vec<DeclaredProviderToolTemplateModel>, String> {
    // declarations deduplicates models that instantiate several reviewed targets for one connection.
    // declarations 对同一连接实例化多个已审核目标的模型进行去重。
    let mut declarations = BTreeMap::new();
    for connection in configuration.connection_graph.connections.values() {
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            continue;
        }
        // manifest is the exact provider document frozen in the selected generation.
        // manifest 是在所选代际中冻结的精确供应商文档。
        let Some(manifest) = generation
            .manifests
            .manifest(connection.provider_id.as_str())
        else {
            continue;
        };
        if !manifest
            .descriptor
            .service_entries
            .iter()
            .any(|entry| entry.id == connection.service_entry_id)
        {
            return Err(format!(
                "Service entry `{}` is not registered for provider `{}`.",
                connection.service_entry_id, connection.provider_id
            ));
        }
        for instance in manifest.tool_target_template_instances.values() {
            if instance.service_entry_id != connection.service_entry_id {
                continue;
            }
            // target resolves the exact credential scope and AI-visibility contract of this instance.
            // target 解析该实例的精确凭据作用域与 AI 可见性契约。
            let target = manifest
                .tool_targets
                .iter()
                .find(|target| target.target_id == instance.target_id)
                .ok_or_else(|| {
                    format!(
                        "Provider `{}` template instance `{}` has no compiled target.",
                        connection.provider_id, instance.target_id
                    )
                })?;
            if !target.contract.capability.is_ai_visible()
                || target.service_entry_id != connection.service_entry_id
                || !provider_tool_target_allows_credential(
                    target,
                    connection.credential_profile_id.as_str(),
                )
            {
                continue;
            }
            // declaration_key preserves account identity while collapsing multiple operations for one model.
            // declaration_key 保留账号身份，同时折叠同一模型的多个操作。
            let declaration_key = (connection.id.clone(), instance.model_id.clone());
            declarations.entry(declaration_key).or_insert_with(|| {
                DeclaredProviderToolTemplateModel {
                    provider_id: connection.provider_id.clone(),
                    connection_id: connection.id.clone(),
                    service_entry_id: connection.service_entry_id.clone(),
                    model_id: instance.model_id.clone(),
                    display_name: instance.model_display_name.clone(),
                }
            });
        }
    }
    Ok(declarations.into_values().collect())
}

/// Reports whether one provider-tool target authorizes an exact credential profile.
/// 报告一个供应商工具目标是否授权精确凭据配置档。
///
/// # Parameters
/// 参数
/// `target` owns the manifest-declared scope and `credential_profile_id` is the connection's
/// exact profile. The schema defines an empty target scope as every profile.
/// `target` 持有 Manifest 声明的作用域，`credential_profile_id` 是连接的精确配置档；
/// Schema 把空目标作用域定义为全部配置档。
///
/// # Returns
/// 返回值
/// `true` when the scope is global or explicitly contains the profile.
/// 当作用域为全局或显式包含该配置档时返回 `true`。
fn provider_tool_target_allows_credential(
    target: &ProviderToolTarget,
    credential_profile_id: &str,
) -> bool {
    target.credential_profile_ids.is_empty()
        || target
            .credential_profile_ids
            .contains(credential_profile_id)
}

/// Validates one process-local immutable session-route lease admission.
/// 校验一次进程内不可变会话路由租约准入。
///
/// # Parameters
/// 参数
/// `session_id` is the exact key, `lease_count` is the current retained population, and
/// `already_leased` preserves idempotent reads even when the capacity is full.
/// `session_id` 是精确键，`lease_count` 是当前保留数量，`already_leased` 在容量已满时仍
/// 保留幂等读取。
///
/// # Returns
/// 返回值
/// Success for one bounded existing or admissible new key, otherwise a controlled capacity error.
/// 对一个有界的既有键或可准入新键返回成功，否则返回受控容量错误。
fn validate_session_selection_lease_admission(
    session_id: &str,
    lease_count: usize,
    already_leased: bool,
) -> Result<(), String> {
    if session_id.is_empty() || session_id.len() > SESSION_SELECTION_LEASE_ID_MAX_BYTES {
        return Err(format!(
            "Public AI session route lease identifiers must contain 1..={SESSION_SELECTION_LEASE_ID_MAX_BYTES} UTF-8 bytes."
        ));
    }
    if !already_leased && lease_count >= SESSION_SELECTION_LEASE_MAX_ENTRIES {
        return Err(format!(
            "Public AI session route lease capacity of {SESSION_SELECTION_LEASE_MAX_ENTRIES} entries is exhausted."
        ));
    }
    Ok(())
}

impl PublicAiRuntime {
    /// Builds one first-batch runtime with the exact host credential boundary.
    /// 使用精确的宿主凭据边界构建首批运行时。
    ///
    /// # Parameters
    /// 参数
    /// `secrets` resolves opaque credential references without exposing storage details.
    /// `secrets` 解析不透明凭据引用且不暴露存储细节。
    ///
    /// # Returns
    /// 返回值
    /// A runtime containing MiniMax and OpenRouter registrations.
    /// 返回包含 MiniMax 与 OpenRouter 注册项的运行时。
    pub fn first_batch(secrets: Arc<dyn CredentialSecretStore>) -> Result<Self, String> {
        install_process_rustls_crypto_provider()?;
        let protocols = Arc::new(LlmProtocolRuntime::first_batch().map_err(|error| error.message)?);
        let http = build_public_ai_http_client()?;
        Self::first_batch_from_components(secrets, protocols, http)
    }

    /// Builds one first-batch runtime with an explicitly supplied HTTP client for cross-crate E2E tests.
    /// 使用显式提供的 HTTP 客户端为跨 crate E2E 测试构建首批运行时。
    ///
    /// # Parameters
    /// 参数
    /// `secrets` resolves opaque credentials and `http` is a test-owned client whose trust roots
    /// and DNS mapping are bounded by the fixture.
    /// `secrets` 解析不透明凭据；`http` 是由夹具约束信任根与 DNS 映射的测试专用客户端。
    ///
    /// # Returns
    /// 返回值
    /// A complete runtime using the injected transport, or an exact protocol bootstrap failure.
    /// 返回使用注入传输的完整运行时，或精确的协议启动失败。
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn first_batch_with_http_client_for_testing(
        secrets: Arc<dyn CredentialSecretStore>,
        http: reqwest::Client,
    ) -> Result<Self, String> {
        install_process_rustls_crypto_provider()?;
        let protocols = Arc::new(LlmProtocolRuntime::first_batch().map_err(|error| error.message)?);
        Self::first_batch_from_components(secrets, protocols, http)
    }

    /// Builds one runtime from already validated protocol and HTTP components.
    /// 从已经校验的协议与 HTTP 组件构建一个运行时。
    ///
    /// # Parameters
    /// 参数
    /// `secrets`, `protocols`, and `http` are the exact process-local dependencies shared by
    /// discovery, execution, OAuth, and immutable generation publication.
    /// `secrets`、`protocols` 与 `http` 是发现、执行、OAuth 与不可变代际发布共享的精确
    /// 进程内依赖。
    ///
    /// # Returns
    /// 返回值
    /// A fully initialized runtime or an exact bundled-catalog construction failure.
    /// 返回完整初始化的运行时，或精确的内置目录构建失败。
    fn first_batch_from_components(
        secrets: Arc<dyn CredentialSecretStore>,
        protocols: Arc<LlmProtocolRuntime>,
        http: reqwest::Client,
    ) -> Result<Self, String> {
        // One shared catalog instance backs the runtime and the discovery client, so a hot
        // replacement is visible to both without a second load or clone (plan §17.6). The
        // registry derives from the exact same compiled catalog; the bundled resources are
        // loaded exactly once (plan §R3.1/R3.2).
        // 运行时与发现客户端共享同一目录实例，热替换对两者立即可见，无需二次加载或克隆
        // （方案 §17.6）。注册表从同一份已编译目录派生；内置资源只加载一次（方案 §R3.1/R3.2）。
        let bundled_catalog = crate::provider_manifest::try_load_bundled_resolved_catalog()?;
        let bundled_manifests =
            std::sync::Arc::new(std::sync::RwLock::new(std::sync::Arc::new(bundled_catalog)));
        {
            let catalog = bundled_manifests
                .read()
                .map_err(|_| String::from("The AI manifest lock is poisoned."))?;
            ProviderRegistry::from_catalog(&catalog)?;
        }
        let initial_manifest_generation = bundled_manifests
            .read()
            .map_err(|_| String::from("The AI manifest lock is poisoned."))?
            .clone();
        let resolved_provider_generation =
            Arc::new(RwLock::new(Arc::new(ResolvedProviderGeneration {
                configuration: Arc::new(PersistedAiConfiguration::default()),
                manifests: initial_manifest_generation,
                activated_effective_catalogs: RuntimeConnectionCatalogs::default(),
                source_catalogs: RuntimeConnectionCatalogs::default(),
                usage_cache: BTreeMap::new(),
            })));
        Ok(Self {
            execution_client: ProviderExecutionClient::new(http.clone(), Arc::clone(&protocols)),
            metadata_client: HttpProviderMetadataClient::with_bundled_manifests(
                http.clone(),
                Arc::clone(&bundled_manifests),
            ),
            oauth: Arc::new(OAuthAuthorizationRuntime::new(http)),
            protocols,
            catalogs: Arc::new(RwLock::new(RuntimeConnectionCatalogs::default())),
            source_catalogs: Arc::new(RwLock::new(RuntimeConnectionCatalogs::default())),
            activated_effective_catalogs: Arc::new(RwLock::new(
                RuntimeConnectionCatalogs::default(),
            )),
            catalog_mutation_lock: Arc::new(StdMutex::new(())),
            catalog_state_revision: Arc::new(AtomicU64::new(0)),
            catalog_refresh_flight: Arc::new(AsyncMutex::new(())),
            catalog_errors: Arc::new(RwLock::new(BTreeMap::new())),
            catalog_statuses: Arc::new(RwLock::new(BTreeMap::new())),
            usage_cache: Arc::new(RwLock::new(BTreeMap::new())),
            secondary_route_cache: Arc::new(RwLock::new(BTreeMap::new())),
            refresh_locks: Arc::new(StdMutex::new(BTreeMap::new())),
            secrets,
            provider_tasks: ProviderTaskRecoveryStore::default(),
            bundled_manifests,
            resolved_provider_generation,
            session_selection_leases: Arc::new(RwLock::new(BTreeMap::new())),
            runtime_media_upload_progress_observer: Arc::new(RwLock::new(None)),
        })
    }

    /// Installs or clears the host observer for post-dispatch media upload progress.
    /// 安装或清除分派后媒体上传进度的宿主观察器。
    ///
    /// # Parameters
    /// 参数
    /// `observer` is the process-local callback owned by the embedding service.
    /// `observer` 是嵌入服务持有的进程内回调。
    ///
    /// # Returns
    /// 返回值
    /// Success after atomically replacing the observer, or a controlled poisoned-lock error.
    /// 原子替换观察器后返回成功，或返回受控的锁中毒错误。
    pub fn set_runtime_media_upload_progress_observer(
        &self,
        observer: Option<crate::RuntimeMediaUploadProgressObserver>,
    ) -> Result<(), String> {
        let mut current = self
            .runtime_media_upload_progress_observer
            .write()
            .map_err(|_| String::from("The runtime media upload observer lock is poisoned."))?;
        *current = observer;
        Ok(())
    }

    /// Builds an unavailable runtime suitable for tests and non-service callers.
    /// 构建适用于测试与非服务调用方的不可用运行时。
    ///
    /// # Returns
    /// 返回值
    /// A valid registry whose credential resolver always rejects reads.
    /// 返回注册表有效但凭据解析器始终拒绝读取的运行时。
    pub fn unavailable() -> Result<Self, String> {
        Self::first_batch(Arc::new(UnavailableCredentialSecretResolver))
    }

    /// Returns the strict provider registry from the current immutable manifest generation.
    /// 从当前不可变 Manifest 代际返回严格供应商注册表。
    ///
    /// # Returns
    /// 返回值
    /// An owned coherent registry snapshot, including a provider recovered from persisted Active
    /// or Previous when its bundled baseline was isolated.
    /// 返回一份自有一致注册表快照；当内置基线被隔离时，包含由持久化 Active 或 Previous
    /// 恢复的供应商。
    pub fn providers(&self) -> Result<ProviderRegistry, String> {
        let generation = self
            .resolved_provider_generation
            .read()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?;
        ProviderRegistry::from_catalog(generation.manifests.as_ref())
    }

    /// Builds the effective provider registry for one immutable published generation.
    /// 为一个不可变配置快照构建有效供应商注册表。
    ///
    /// The system half derives from the single authoritative catalog so hot-replaced
    /// manifests are reflected immediately; custom providers come from the configuration.
    /// 系统半部分从唯一权威目录派生，热替换清单立即可见；自定义供应商来自配置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns the user-defined compatibility providers for this operation.
    /// `configuration` 持有本次操作使用的用户自定义兼容供应商。
    ///
    /// # Returns
    /// 返回值
    /// A validated registry containing system and custom providers.
    /// 返回包含系统供应商与自定义供应商的已校验注册表。
    pub fn effective_providers(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<ProviderRegistry, String> {
        let generation = self
            .resolved_provider_generation
            .read()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?;
        let system = ProviderRegistry::from_catalog(generation.manifests.as_ref())?;
        ProviderRegistry::effective_from_system(system, &configuration.custom_providers)
    }

    /// Returns the authoritative standalone configuration snapshot.
    /// 返回权威独立配置快照。
    ///
    /// # Returns
    /// 返回值
    /// A clone safe to retain for one execution or settings transaction.
    /// 返回可供单次执行或设置事务保留的安全克隆。
    pub fn configuration_snapshot(&self) -> Result<PersistedAiConfiguration, String> {
        let generation = self.selection_catalog_snapshot()?;
        Ok(generation.configuration.as_ref().clone())
    }

    /// Builds a detached executable configuration against the current manifest generation.
    /// 针对当前 Manifest 代际构建一份独立可执行配置。
    ///
    /// The authoritative persisted image remains unchanged so isolated providers retain their
    /// repair state, while callers that enumerate runtime connections receive only executable
    /// provider identities.
    /// 权威持久化镜像保持不变，使已隔离供应商保留修复状态；枚举运行时连接的调用方只会收到
    /// 具有可执行权威的供应商身份。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the persisted image to project.
    /// `configuration` 是待投影的持久化镜像。
    ///
    /// # Returns
    /// 返回值
    /// A coherent runtime-only clone derived from one immutable manifest generation.
    /// 返回从单一不可变 Manifest 代际派生的一致仅运行时克隆。
    pub fn runtime_configuration_snapshot(
        &self,
        _configuration: &PersistedAiConfiguration,
    ) -> Result<PersistedAiConfiguration, String> {
        // generation freezes configuration and Manifest authority for this complete projection.
        // generation 为本次完整投影冻结配置与 Manifest 权威。
        let generation = self.selection_catalog_snapshot()?;
        // configuration is the exact candidate that crossed the same activation boundary.
        // configuration 是越过同一激活边界的精确候选。
        let configuration = generation.configuration.as_ref();
        Ok(runtime_configuration_for_manifest_catalog(
            configuration,
            generation.manifests.as_ref(),
        ))
    }

    /// Atomically replaces the authoritative standalone configuration.
    /// 原子替换权威独立配置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is a successfully persisted version-one snapshot.
    /// `configuration` 是已成功持久化的第一版快照。
    pub fn replace_configuration(
        &self,
        configuration: PersistedAiConfiguration,
    ) -> Result<(), String> {
        self.reapply_model_catalog_overlays(&configuration)
    }

    /// Returns the core-owned protocol runtime.
    /// 返回由核心拥有的协议运行时。
    pub fn protocols(&self) -> &LlmProtocolRuntime {
        self.protocols.as_ref()
    }

    /// Returns the shared metadata client.
    /// 返回共享元数据客户端。
    pub fn metadata_client(&self) -> &HttpProviderMetadataClient {
        &self.metadata_client
    }

    /// Returns the current published catalog generation.
    /// 返回当前已发布目录代数。
    ///
    /// # Returns
    /// 返回值
    /// The generation visible to requests and projections under a short read lock.
    /// 在短读锁下返回请求与投影可见的目录代数。
    pub fn catalog_generation(&self) -> u64 {
        self.resolved_provider_generation
            .read()
            .map(|generation| generation.manifests.generation)
            .unwrap_or(0)
    }

    /// Returns the current shared catalog content digest.
    /// 返回当前共享目录内容摘要。
    ///
    /// # Returns
    /// 返回值
    /// The content-addressed digest that explains the current generation (plan §R3.8).
    /// 返回解释当前代数的内容寻址摘要（方案 §R3.8）。
    pub fn catalog_content_hash(&self) -> String {
        self.resolved_provider_generation
            .read()
            .map(|generation| generation.manifests.content_sha256.clone())
            .unwrap_or_default()
    }

    /// Advances the mutable catalog-state revision while the caller holds the mutation lock.
    /// 在调用方持有变更锁时推进可变目录状态修订号。
    ///
    /// # Returns
    /// 返回值
    /// The newly published monotonic revision, or an error when the supported range is exhausted.
    /// 返回新发布的单调修订号；若耗尽支持范围则返回错误。
    fn advance_catalog_state_revision_locked(&self) -> Result<u64, String> {
        let current_revision = self.catalog_state_revision.load(Ordering::Acquire);
        let next_revision = current_revision.checked_add(1).ok_or_else(|| {
            String::from("The AI catalog state revision exhausted the supported u64 range.")
        })?;
        self.catalog_state_revision
            .store(next_revision, Ordering::Release);
        Ok(next_revision)
    }

    /// Captures every process-local catalog field mutated by Manifest activation.
    /// 捕获 Manifest 激活会修改的每个进程内目录字段。
    ///
    /// # Returns
    /// 返回值
    /// An opaque exact before-image suitable for deterministic compensation.
    /// 返回适用于确定性补偿的不透明精确前镜像。
    pub fn capture_activation_snapshot(&self) -> Result<ProviderRuntimeActivationSnapshot, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        Ok(ProviderRuntimeActivationSnapshot {
            state_revision: self.catalog_state_revision.load(Ordering::Acquire),
            catalogs: self
                .catalogs
                .read()
                .map_err(|_| String::from("The AI catalog lock is poisoned."))?
                .clone(),
            source_catalogs: self
                .source_catalogs
                .read()
                .map_err(|_| String::from("The AI source catalog lock is poisoned."))?
                .clone(),
            activated_effective_catalogs: self
                .activated_effective_catalogs
                .read()
                .map_err(|_| String::from("The activated AI catalog lock is poisoned."))?
                .clone(),
            catalog_errors: self
                .catalog_errors
                .read()
                .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
                .clone(),
            catalog_statuses: self
                .catalog_statuses
                .read()
                .map_err(|_| String::from("The AI catalog status lock is poisoned."))?
                .clone(),
            usage_cache: self
                .usage_cache
                .read()
                .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
                .clone(),
            resolved_provider_generation: self.selection_catalog_snapshot()?,
        })
    }

    /// Restores every process-local catalog field when the expected activated image is still current.
    /// 仅当预期激活后镜像仍为当前状态时，恢复每个进程内目录字段。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the exact before-image and `expected_current` is the image captured immediately
    /// after this transaction published. A later unrelated mutation makes compensation fail closed.
    /// `snapshot` 是精确前镜像，`expected_current` 是本事务发布后立即捕获的镜像；若此后已有
    /// 无关变更，补偿将以失败关闭方式拒绝覆盖。
    ///
    /// # Returns
    /// 返回值
    /// Success only after compare-and-swap validation, all write guards, and one complete restore.
    /// 仅在比较交换校验、取得全部写锁并完成一次完整恢复后返回成功。
    pub fn restore_activation_snapshot(
        &self,
        snapshot: &ProviderRuntimeActivationSnapshot,
        expected_current: &ProviderRuntimeActivationSnapshot,
    ) -> Result<(), String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let current_revision = self.catalog_state_revision.load(Ordering::Acquire);
        if current_revision != expected_current.state_revision {
            return Err(format!(
                "The AI runtime changed after provider activation (expected state revision {}, observed {}); compensation refused to overwrite newer state.",
                expected_current.state_revision, current_revision
            ));
        }
        let restored_revision = current_revision.checked_add(1).ok_or_else(|| {
            String::from("The AI catalog state revision exhausted the supported u64 range.")
        })?;
        let mut catalogs = self
            .catalogs
            .write()
            .map_err(|_| String::from("The AI catalog lock is poisoned."))?;
        let mut source_catalogs = self
            .source_catalogs
            .write()
            .map_err(|_| String::from("The AI source catalog lock is poisoned."))?;
        let mut activated_effective_catalogs = self
            .activated_effective_catalogs
            .write()
            .map_err(|_| String::from("The activated AI catalog lock is poisoned."))?;
        let mut catalog_errors = self
            .catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?;
        let mut catalog_statuses = self
            .catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?;
        let mut usage_cache = self
            .usage_cache
            .write()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?;
        let mut resolved_provider_generation = self
            .resolved_provider_generation
            .write()
            .map_err(|_| String::from("The AI manifest generation lock is poisoned."))?;
        *catalogs = snapshot.catalogs.clone();
        *source_catalogs = snapshot.source_catalogs.clone();
        *activated_effective_catalogs = snapshot.activated_effective_catalogs.clone();
        *catalog_errors = snapshot.catalog_errors.clone();
        *catalog_statuses = snapshot.catalog_statuses.clone();
        *usage_cache = snapshot.usage_cache.clone();
        *resolved_provider_generation = Arc::clone(&snapshot.resolved_provider_generation);
        self.catalog_state_revision
            .store(restored_revision, Ordering::Release);
        Ok(())
    }

    /// Builds a catalog-isolated runtime used to prepare one activation without touching readers.
    /// 构建目录隔离运行时，用于在不接触读取方的情况下准备一次激活。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` supplies the exact mutable before-image and `staged_system_manifests` supplies
    /// the complete staged system authority used by discovery and final overlay resolution.
    /// `snapshot` 提供精确可变前镜像，`staged_system_manifests` 提供发现与最终覆盖解析使用的
    /// 完整暂存系统权威。
    ///
    /// # Returns
    /// 返回值
    /// A runtime whose catalog, status, usage, generation, and activation locks are fully isolated.
    /// 返回目录、状态、用量、代际与激活锁均完全隔离的运行时。
    fn isolated_catalog_activation_runtime(
        &self,
        snapshot: &ProviderRuntimeActivationSnapshot,
        staged_system_manifests: Arc<crate::provider_manifest::ResolvedProviderCatalog>,
    ) -> Self {
        // isolatedRuntime reuses immutable clients and the host secret authority, while every
        // process-local projection that can influence a reader receives a private allocation.
        // isolatedRuntime 复用不可变客户端与宿主秘密权威，同时为每项可影响读取方的进程内投影
        // 分配私有存储。
        let mut isolated_runtime = self.clone();
        isolated_runtime.catalogs = Arc::new(RwLock::new(snapshot.catalogs.clone()));
        isolated_runtime.source_catalogs = Arc::new(RwLock::new(snapshot.source_catalogs.clone()));
        isolated_runtime.activated_effective_catalogs =
            Arc::new(RwLock::new(snapshot.activated_effective_catalogs.clone()));
        isolated_runtime.catalog_mutation_lock = Arc::new(StdMutex::new(()));
        isolated_runtime.catalog_state_revision = Arc::new(AtomicU64::new(snapshot.state_revision));
        isolated_runtime.catalog_refresh_flight = Arc::new(AsyncMutex::new(()));
        isolated_runtime.catalog_errors = Arc::new(RwLock::new(snapshot.catalog_errors.clone()));
        isolated_runtime.catalog_statuses =
            Arc::new(RwLock::new(snapshot.catalog_statuses.clone()));
        isolated_runtime.usage_cache = Arc::new(RwLock::new(snapshot.usage_cache.clone()));
        isolated_runtime.bundled_manifests =
            Arc::new(std::sync::RwLock::new(staged_system_manifests));
        isolated_runtime.resolved_provider_generation = Arc::new(RwLock::new(Arc::clone(
            &snapshot.resolved_provider_generation,
        )));
        isolated_runtime.secondary_route_cache = Arc::new(RwLock::new(BTreeMap::new()));
        isolated_runtime.session_selection_leases = Arc::new(RwLock::new(BTreeMap::new()));
        isolated_runtime
    }

    /// Publishes one fully prepared isolated catalog image if no main-runtime input changed.
    /// 当主运行时输入未发生变化时，发布一份已完整准备的隔离目录镜像。
    ///
    /// # Parameters
    /// 参数
    /// `before_image` is the compare side, `candidate` is the complete replacement, and
    /// `expected_staged_system_manifests` optionally freezes the staged Manifest Arc as a second
    /// compare-and-swap input for signed hot activation.
    /// `before_image` 是比较端，`candidate` 是完整替换值；`expected_staged_system_manifests`
    /// 可选冻结暂存 Manifest Arc，作为签名热激活的第二项比较交换输入。
    ///
    /// # Returns
    /// 返回值
    /// The exact published image after one complete replacement, or a fail-closed conflict without
    /// any write. The returned revision is assigned inside the publication critical section.
    /// 返回一次完整替换后的精确已发布镜像；若发生冲突，则不写入任何字段并以失败关闭方式
    /// 返回错误。返回修订号在发布临界区内赋值。
    fn publish_isolated_catalog_candidate(
        &self,
        before_image: &ProviderRuntimeActivationSnapshot,
        candidate: &ProviderRuntimeActivationSnapshot,
        expected_staged_system_manifests: Option<
            &Arc<crate::provider_manifest::ResolvedProviderCatalog>,
        >,
    ) -> Result<ProviderRuntimeActivationSnapshot, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let current_revision = self.catalog_state_revision.load(Ordering::Acquire);
        if current_revision != before_image.state_revision {
            return Err(format!(
                "The AI runtime changed while a provider catalog candidate was prepared (expected state revision {}, observed {}); publication refused.",
                before_image.state_revision, current_revision
            ));
        }
        if let Some(expected) = expected_staged_system_manifests {
            let current = self
                .bundled_manifests
                .read()
                .map_err(|_| String::from("The AI manifest lock is poisoned."))?;
            if !Arc::ptr_eq(&current, expected) {
                return Err(String::from(
                    "The staged provider Manifest catalog changed while connection projections were prepared; publication refused.",
                ));
            }
        }
        let published_revision = current_revision.checked_add(1).ok_or_else(|| {
            String::from("The AI catalog state revision exhausted the supported u64 range.")
        })?;
        // All guards are acquired before the first assignment so lock poisoning cannot leave a
        // partially replaced image. The immutable generation is assigned last for lock-free readers.
        // 在首次赋值前取得全部写锁，因此锁中毒不会留下部分替换镜像；最后赋值不可变代际，
        // 供无变更锁读取方观察。
        let mut catalogs = self
            .catalogs
            .write()
            .map_err(|_| String::from("The AI catalog lock is poisoned."))?;
        let mut source_catalogs = self
            .source_catalogs
            .write()
            .map_err(|_| String::from("The AI source catalog lock is poisoned."))?;
        let mut activated_effective_catalogs = self
            .activated_effective_catalogs
            .write()
            .map_err(|_| String::from("The activated AI catalog lock is poisoned."))?;
        let mut catalog_errors = self
            .catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?;
        let mut catalog_statuses = self
            .catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?;
        let mut usage_cache = self
            .usage_cache
            .write()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?;
        let mut resolved_provider_generation = self
            .resolved_provider_generation
            .write()
            .map_err(|_| String::from("The AI manifest generation lock is poisoned."))?;
        *catalogs = candidate.catalogs.clone();
        *source_catalogs = candidate.source_catalogs.clone();
        *activated_effective_catalogs = candidate.activated_effective_catalogs.clone();
        *catalog_errors = candidate.catalog_errors.clone();
        *catalog_statuses = candidate.catalog_statuses.clone();
        *usage_cache = candidate.usage_cache.clone();
        *resolved_provider_generation = Arc::clone(&candidate.resolved_provider_generation);
        self.catalog_state_revision
            .store(published_revision, Ordering::Release);
        let mut published_image = candidate.clone();
        published_image.state_revision = published_revision;
        Ok(published_image)
    }

    /// Returns the complete non-secret provider selection leased by one active session.
    /// 返回单个活动会话租用的完整非秘密供应商选择。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the stable session identifier used only as a process-local map key.
    /// `session_id` 是仅用作进程内映射键的稳定会话标识。
    ///
    /// # Returns
    /// 返回值
    /// The frozen selection when the session already owns a lease, or `None` otherwise.
    /// 会话已有租约时返回冻结选择，否则返回 `None`。
    pub fn session_selection_lease(
        &self,
        session_id: &str,
    ) -> Result<Option<ResolvedPublicAiSelection>, String> {
        self.session_selection_leases
            .read()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))
            .map(|leases| leases.get(session_id).cloned())
    }

    /// Atomically leases one complete non-secret provider selection to a session if absent.
    /// 当会话尚无租约时原子租用一份完整非秘密供应商选择。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the stable session identifier and `selection` is the exact immutable
    /// route snapshot captured at the session activation boundary.
    /// `session_id` 是稳定会话标识，`selection` 是在会话激活边界捕获的精确不可变路由快照。
    ///
    /// # Returns
    /// 返回值
    /// The already leased selection or the newly installed candidate. Concurrent callers can
    /// therefore never replace an established session route.
    /// 返回已有租约选择或新安装候选，因此并发调用方绝不会替换已建立的会话路由。
    pub fn lease_session_selection(
        &self,
        session_id: String,
        selection: ResolvedPublicAiSelection,
    ) -> Result<ResolvedPublicAiSelection, String> {
        let mut leases = self
            .session_selection_leases
            .write()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))?;
        let already_leased = leases.contains_key(session_id.as_str());
        validate_session_selection_lease_admission(
            session_id.as_str(),
            leases.len(),
            already_leased,
        )?;
        if let Some(existing) = leases.get(session_id.as_str()) {
            return Ok(existing.clone());
        }
        leases.insert(session_id, selection.clone());
        Ok(selection)
    }

    /// Clears the process-local provider selection leased by one session.
    /// 清除单个会话租用的进程内供应商选择。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the exact lease invalidated by explicit model reselection or
    /// session lifecycle removal.
    /// `session_id` 标识因显式重新选择模型或会话生命周期移除而失效的精确租约。
    ///
    /// # Returns
    /// 返回值
    /// The removed selection when present, or `None` when no lease existed.
    /// 存在租约时返回被移除选择，否则返回 `None`。
    pub fn clear_session_selection_lease(
        &self,
        lease_scope_id: &str,
    ) -> Result<Option<ResolvedPublicAiSelection>, String> {
        self.session_selection_leases
            .write()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))
            .map(|mut leases| leases.remove(lease_scope_id))
    }

    /// Clears the process-local provider-route lease owned by one session.
    /// 清除单个会话拥有的进程内供应商路由租约。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the exact owner removed or explicitly reselecting its model.
    /// `session_id` 是被移除或正在显式重新选择模型的精确所有者。
    ///
    /// # Returns
    /// 返回值
    /// One when the session owned a lease, or zero otherwise.
    /// 会话拥有租约时返回一，否则返回零。
    pub fn clear_session_selection_leases(&self, session_id: &str) -> Result<usize, String> {
        self.clear_session_selection_lease(session_id)
            .map(|selection| usize::from(selection.is_some()))
    }

    /// Proves a complete session set owns no route lease and excludes concurrent installation.
    /// 证明一组完整会话不持有路由租约，并排除并发安装。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` contains the exact stable session identities whose unleased runtime controls
    /// will be replaced by one atomic caller-owned projection.
    /// `session_ids` 包含将由调用方原子投影替换未租用运行态控制的精确稳定会话身份。
    ///
    /// # Returns
    /// 返回值
    /// An opaque guard retained until the caller publishes its matching state, or a conflict/error
    /// without mutation.
    /// 返回保留到调用方发布配套状态的不透明 Guard，或无变更的冲突/错误。
    pub fn acquire_session_selection_lease_absence_guard<'a>(
        &'a self,
        session_ids: &[String],
    ) -> Result<SessionSelectionLeaseAbsenceGuard<'a>, String> {
        // leases excludes writers before the first absence check and remains owned by the result.
        // leases 在首次缺失检查前排除写入方，并由返回值持续持有。
        let leases = self
            .session_selection_leases
            .write()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))?;
        if let Some(session_id) = session_ids
            .iter()
            .find(|session_id| leases.contains_key(session_id.as_str()))
        {
            return Err(format!(
                "Session `{session_id}` acquired a public AI route lease while model reconciliation was being prepared."
            ));
        }
        Ok(SessionSelectionLeaseAbsenceGuard { _guard: leases })
    }

    /// Revokes one exact session set while proving another set remains unleased.
    /// 撤销一组精确会话租约，同时证明另一组会话仍未持有租约。
    ///
    /// # Parameters
    /// 参数
    /// `revoked_session_ids` identifies explicit access revocations whose current leases must be
    /// removed; `required_absent_session_ids` identifies catalog reconciliations that may proceed
    /// only when no route lease exists.
    /// `revoked_session_ids` 标识必须移除当前租约的显式访问撤销；
    /// `required_absent_session_ids` 标识仅在路由租约不存在时才能继续的目录协调。
    ///
    /// # Returns
    /// 返回值
    /// A reversible write guard that keeps every lease writer excluded until commit, or an error
    /// without mutation when one required-absent session already owns a lease.
    /// 返回一个在提交前排除全部租约写入方的可逆写 Guard；若要求缺失的会话已持有租约，
    /// 则无修改返回错误。
    pub fn acquire_session_selection_lease_reconciliation_guard<'a>(
        &'a self,
        revoked_session_ids: &[String],
        required_absent_session_ids: &[String],
    ) -> Result<SessionSelectionLeaseReconciliationGuard<'a>, String> {
        // leases excludes writers before validation so the absence proof and removals are atomic.
        // leases 在校验前排除写入方，使缺失证明与移除操作保持原子性。
        let mut leases = self
            .session_selection_leases
            .write()
            .map_err(|_| String::from("The public AI session route lease lock is poisoned."))?;
        if let Some(session_id) = required_absent_session_ids
            .iter()
            .find(|session_id| leases.contains_key(session_id.as_str()))
        {
            return Err(format!(
                "Session `{session_id}` acquired a public AI route lease while model reconciliation was being prepared."
            ));
        }
        // removed owns the exact before-image while duplicate identifiers remain idempotent.
        // removed 持有精确前镜像，同时让重复标识保持幂等。
        let mut removed = BTreeMap::new();
        for session_id in revoked_session_ids {
            if removed.contains_key(session_id.as_str()) {
                continue;
            }
            if let Some(selection) = leases.remove(session_id.as_str()) {
                removed.insert(session_id.clone(), selection);
            }
        }
        Ok(SessionSelectionLeaseReconciliationGuard {
            guard: leases,
            removed,
            committed: false,
        })
    }

    /// Returns the process-local provider OAuth transaction runtime.
    /// 返回进程内供应商 OAuth 事务运行时。
    ///
    /// # Returns
    /// 返回值
    /// A shared runtime used by settings services without persisting transactions.
    /// 返回设置服务使用且不持久化事务的共享运行时。
    pub fn oauth(&self) -> &Arc<OAuthAuthorizationRuntime> {
        &self.oauth
    }

    /// Returns the shared host-owned credential store used by this runtime.
    /// 返回该运行时使用的共享宿主持有凭据仓库。
    ///
    /// # Returns
    /// 返回值
    /// One cloned trait object that preserves the runtime storage identity.
    /// 返回一个保持运行时存储身份的克隆 Trait Object。
    pub fn credential_secret_store(&self) -> Arc<dyn CredentialSecretStore> {
        Arc::clone(&self.secrets)
    }

    /// Replaces one credential-scoped catalog atomically.
    /// 原子替换一个凭据作用域目录。
    ///
    /// # Parameters
    /// 参数
    /// `catalog` is the complete latest remote snapshot.
    /// `catalog` 是完整的最新远端快照。
    pub fn replace_catalog(&self, catalog: crate::ConnectionModelCatalog) -> Result<(), String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry the catalog replacement after activation completes.",
            )
        })?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.replace_catalog_locked(catalog)?;
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Replaces one mutable connection catalog while the caller holds the catalog mutation lock.
    /// 在调用方持有目录变更锁时替换一个可变连接目录。
    ///
    /// # Parameters
    /// 参数
    /// `catalog` is the complete replacement snapshot and does not advance the state revision alone.
    /// `catalog` 是完整替换快照；本方法本身不会推进状态修订号。
    ///
    /// # Returns
    /// 返回值
    /// Success after the in-memory map owns the replacement.
    /// 内存映射持有替换值后返回成功。
    fn replace_catalog_locked(&self, catalog: crate::ConnectionModelCatalog) -> Result<(), String> {
        self.catalogs
            .write()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .replace(catalog);
        Ok(())
    }

    /// Stages one immutable discovery source catalog for cross-crate activation tests.
    /// 为跨 crate 激活测试暂存一份不可变发现源目录。
    ///
    /// This seam deliberately does not publish an effective catalog. Tests must invoke the same
    /// explicit overlay activation used by production after staging the source evidence.
    /// 该接缝刻意不发布有效目录；测试在暂存来源证据后必须调用与生产相同的显式覆盖激活。
    ///
    /// # Parameters
    /// 参数
    /// `catalog` is the exact pre-overlay discovery snapshot owned by one connection.
    /// `catalog` 是一个连接持有的精确覆盖前发现快照。
    ///
    /// # Returns
    /// 返回值
    /// Success after the staged source map owns the snapshot without changing the published generation.
    /// 暂存来源表持有该快照且不改变已发布代际后返回成功。
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn stage_source_catalog_for_testing(
        &self,
        catalog: crate::ConnectionModelCatalog,
    ) -> Result<(), String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry source staging after activation completes.",
            )
        })?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.source_catalogs
            .write()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .replace(catalog);
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Returns all model catalogs from one immutable published generation.
    /// 返回同一不可变已发布代际中的全部模型目录。
    ///
    /// # Returns
    /// 返回值
    /// A coherent cloned connection-to-catalog map that cannot expose staged discovery writes.
    /// 返回一份不会暴露暂存发现写入的一致连接到目录克隆映射。
    pub fn catalog_snapshot(&self) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        let generation = self.selection_catalog_snapshot()?;
        Ok(effective_catalog_snapshot_from_generation(&generation))
    }

    /// Builds every browser-settings catalog view from one immutable provider generation.
    /// 从一个不可变供应商代际构建全部浏览器设置目录视图。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the exact persisted state whose access and custom-provider projections
    /// are evaluated without reading any staged manifest or discovery input.
    /// `configuration` 是待求值访问与自定义供应商投影的精确持久化状态，且不会读取任何暂存
    /// Manifest 或发现输入。
    ///
    /// # Returns
    /// 返回值
    /// One coherent settings projection containing providers, conversation catalogs, template
    /// models, tool targets, and manifest revisions from the same publication boundary.
    /// 返回一份一致设置投影，其中供应商、对话目录、模板模型、工具目标与 Manifest 修订号均
    /// 来自同一发布边界。
    pub fn settings_catalog_snapshot(
        &self,
        _configuration: &PersistedAiConfiguration,
    ) -> Result<PublicAiRuntimeSettingsCatalogSnapshot, String> {
        // generation is retained for the complete projection so concurrent activation cannot mix views.
        // generation 在完整投影期间持续持有，避免并发激活混合视图。
        let generation = self.selection_catalog_snapshot()?;
        let configuration = generation.configuration.as_ref();
        // runtimeConfiguration removes provider identities without executable authority in this
        // exact captured generation while preserving the caller's persisted repair image.
        // runtimeConfiguration 移除当前精确捕获代际中没有可执行权威的供应商身份，同时保留
        // 调用方的持久化修复镜像。
        let runtime_configuration = runtime_configuration_for_manifest_catalog(
            configuration,
            generation.manifests.as_ref(),
        );
        let configuration = &runtime_configuration;
        // system is the registry compiled from the captured manifest catalog.
        // system 是从已捕获 Manifest 目录编译的注册表。
        let system = ProviderRegistry::from_catalog(generation.manifests.as_ref())?;
        // providers applies the explicit custom-provider compatibility layer to that registry.
        // providers 把显式自定义供应商兼容层应用到该注册表。
        let providers =
            ProviderRegistry::effective_from_system(system, &configuration.custom_providers)?;
        // catalogs applies current access and capability layers to frozen source inputs.
        // catalogs 把当前访问与能力层应用到冻结来源输入。
        let catalogs = resolve_model_catalog_overlays_from_inputs(
            configuration,
            generation.manifests.as_ref(),
            &generation.source_catalogs,
            &generation.usage_cache,
        )?
        .snapshot();
        // support_configuration removes only account access switches for the management inventory.
        // support_configuration 仅为管理目录移除账号访问开关。
        let mut support_configuration = configuration.clone();
        support_configuration
            .connection_model_access_policies
            .clear();
        // model_support_catalogs retains disabled account membership from the same generation.
        // model_support_catalogs 从同一代际保留已关闭账号成员关系。
        let model_support_catalogs = resolve_model_catalog_overlays_from_inputs(
            &support_configuration,
            generation.manifests.as_ref(),
            &generation.source_catalogs,
            &generation.usage_cache,
        )?
        .snapshot();
        // source_catalogs is the pre-overlay discovery inventory captured with this generation.
        // source_catalogs 是与本代际共同捕获的覆盖前发现目录。
        let source_catalogs = generation
            .source_catalogs
            .snapshot()
            .into_iter()
            .filter(|(connection_id, _)| {
                configuration
                    .connection_graph
                    .connections
                    .contains_key(connection_id.as_str())
            })
            .collect();
        // manifest_revisions exposes the exact revision identity of the retained generation.
        // manifest_revisions 暴露所持有代际的精确修订身份。
        let manifest_revisions = generation
            .manifests
            .manifests
            .iter()
            .map(|(provider_id, manifest)| (provider_id.clone(), manifest.revision))
            .collect();
        // provider_tool_template_models remains visible independently from enabled state.
        // provider_tool_template_models 独立于启用状态保持可见。
        let provider_tool_template_models =
            provider_tool_template_models_in_generation(configuration, generation.as_ref())?;
        // model_default_enabled_by_key freezes browser access defaults with the same manifest generation.
        // model_default_enabled_by_key 把浏览器访问默认值与同一 Manifest 代际共同冻结。
        let mut model_default_enabled_by_key = BTreeMap::new();
        for (connection_id, catalog) in &model_support_catalogs {
            // connection resolves the owning provider for this exact account catalog.
            // connection 解析该精确账号目录所属的供应商。
            let Some(connection) = configuration
                .connection_graph
                .connections
                .get(connection_id.as_str())
            else {
                continue;
            };
            for model in &catalog.models {
                // scoped_default preserves the exact entry and credential rule scope.
                // scoped_default 保留精确入口与凭据规则作用域。
                let scoped_default = runtime_model_default_enabled_from_manifest(
                    configuration,
                    connection.provider_id.as_str(),
                    connection.service_entry_id.as_str(),
                    Some(connection.credential_profile_id.as_str()),
                    model.id.as_str(),
                    generation
                        .manifests
                        .manifest(connection.provider_id.as_str()),
                );
                // The provider-level settings row is enabled by default when at least one exact
                // configured route enables the model; OR aggregation is stable and order-free.
                // 当至少一条精确已配置路由默认启用模型时，供应商级设置行默认启用；OR 聚合稳定且与顺序无关。
                model_default_enabled_by_key
                    .entry(PersistedAiConfiguration::provider_model_key(
                        connection.provider_id.as_str(),
                        model.id.as_str(),
                    ))
                    .and_modify(|default_enabled| *default_enabled |= scoped_default)
                    .or_insert(scoped_default);
            }
        }
        for model in &provider_tool_template_models {
            // connection restores the credential scope intentionally omitted from the public inventory row.
            // connection 恢复公共管理目录行刻意省略的凭据作用域。
            let connection = configuration
                .connection_graph
                .connections
                .get(model.connection_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Provider-tool template model `{}/{}` references missing connection `{}`.",
                        model.provider_id, model.model_id, model.connection_id
                    )
                })?;
            let scoped_default = runtime_model_default_enabled_from_manifest(
                configuration,
                model.provider_id.as_str(),
                model.service_entry_id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model.model_id.as_str(),
                generation.manifests.manifest(model.provider_id.as_str()),
            );
            model_default_enabled_by_key
                .entry(PersistedAiConfiguration::provider_model_key(
                    model.provider_id.as_str(),
                    model.model_id.as_str(),
                ))
                .and_modify(|default_enabled| *default_enabled |= scoped_default)
                .or_insert(scoped_default);
        }
        // available_provider_tool_targets applies current access against the same template authority.
        // available_provider_tool_targets 针对同一模板权限应用当前访问状态。
        let available_provider_tool_targets = Self::available_provider_tool_targets_in_generation(
            configuration,
            generation.as_ref(),
            &providers,
        )?;
        Ok(PublicAiRuntimeSettingsCatalogSnapshot {
            runtime_configuration,
            providers,
            catalogs,
            model_support_catalogs,
            source_catalogs,
            manifest_revisions,
            provider_tool_template_models,
            model_default_enabled_by_key,
            available_provider_tool_targets,
        })
    }

    /// Resolves the inherited default of one model from the published manifest generation.
    /// 从已发布清单代际解析一个模型的继承默认值。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns manual model declarations and configured route scopes; `provider_id`
    /// and `model_id` identify the provider-level management row being projected.
    /// `configuration` 持有手工模型声明与已配置路由作用域；`provider_id` 与 `model_id` 标识待投影的
    /// 供应商级管理行。
    ///
    /// # Returns
    /// 返回值
    /// The stable OR of all exact configured route defaults. Without a configured route, the
    /// manual/provider-level manifest default is returned.
    /// 返回全部精确已配置路由默认值的稳定 OR；没有已配置路由时返回手工/供应商级 Manifest 默认值。
    pub fn model_default_enabled(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> bool {
        // manifest retains one published default snapshot for this complete lookup.
        // manifest 为本次完整查询保留一份已发布默认值快照。
        let manifest = self
            .resolved_provider_generation
            .read()
            .ok()
            .and_then(|generation| generation.manifests.manifest_arc(provider_id));
        // scoped_default aggregates all concrete route scopes without relying on map order.
        // scoped_default 不依赖 Map 顺序地聚合全部具体路由作用域。
        let scoped_default = configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .map(|connection| {
                runtime_model_default_enabled_from_manifest(
                    configuration,
                    provider_id,
                    connection.service_entry_id.as_str(),
                    Some(connection.credential_profile_id.as_str()),
                    model_id,
                    manifest.as_deref(),
                )
            })
            .reduce(|left, right| left || right);
        scoped_default.unwrap_or_else(|| {
            let Some(manifest) = manifest.as_deref() else {
                return runtime_model_default_enabled_from_manifest(
                    configuration,
                    provider_id,
                    "",
                    None,
                    model_id,
                    None,
                );
            };
            // declared_scope_default projects a provider-level management row across every
            // manifest-declared service and credential when no account is configured yet.
            // declared_scope_default 在尚未配置账号时跨全部 Manifest 声明的服务与凭据投影
            // 供应商级管理行。
            manifest.descriptor.service_entries.iter().any(|entry| {
                if entry.credential_profiles.is_empty() {
                    return runtime_model_default_enabled_from_manifest(
                        configuration,
                        provider_id,
                        entry.id.as_str(),
                        None,
                        model_id,
                        Some(manifest),
                    );
                }
                entry.credential_profiles.iter().any(|profile| {
                    runtime_model_default_enabled_from_manifest(
                        configuration,
                        provider_id,
                        entry.id.as_str(),
                        Some(profile.id.as_str()),
                        model_id,
                        Some(manifest),
                    )
                })
            })
        })
    }

    /// Reports one model's manifest-first enabled state for an exact configured credential route.
    /// 报告一个模型在精确已配置凭据路由上的 Manifest-first 启用状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns user access layers; `connection_id` selects the exact service and
    /// credential scope; `model_id` identifies the model inside that route.
    /// `configuration` 持有用户访问层；`connection_id` 选择精确服务与凭据作用域；`model_id`
    /// 标识该路由内的模型。
    ///
    /// # Returns
    /// 返回值
    /// The exact scoped manifest/manual default after the provider-level user override, or an
    /// explicit error when the connection or published generation cannot be read.
    /// 返回应用供应商级用户覆盖后的精确作用域 Manifest/手工默认值；连接或已发布代际不可读时显式报错。
    pub fn model_is_enabled_for_connection(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        // connection is the unique persisted owner of service-entry and credential scope.
        // connection 是服务入口与凭据作用域的唯一持久化所有者。
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
        let generation = self.selection_catalog_snapshot()?;
        let manifest = generation
            .manifests
            .manifest(connection.provider_id.as_str());
        Ok(runtime_model_is_enabled_from_manifest(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            Some(connection.credential_profile_id.as_str()),
            model_id,
            manifest,
        ))
    }

    /// Reports the manifest-first enabled state of one model under the published manifests.
    /// 报告一个模型在已发布清单下的 manifest-first 启用状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the exact persisted connection graph; `provider_id` and `model_id`
    /// identify the model whose default-enabled state is queried.
    /// `configuration` 是精确持久化连接图；`provider_id` 与 `model_id` 标识被查询默认启用状态的模型。
    ///
    /// # Returns
    /// 返回值
    /// Explicit user/manual-model state wins, then the currently published manifest default;
    /// undeclared models without a manifest fail closed.
    /// 显式用户/手工模型状态优先；其次采用当前已发布清单默认值；无 Manifest 的未声明模型失败关闭。
    pub fn model_is_enabled(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> bool {
        configuration.model_is_enabled_with_default(
            provider_id,
            model_id,
            self.model_default_enabled(configuration, provider_id, model_id),
        )
    }

    /// Captures one coherent immutable manifest/model-catalog selection snapshot.
    /// 捕获一份一致的不可变清单/模型目录选择快照。
    ///
    /// # Returns
    /// 返回值
    /// One Arc that owns the exact manifest generation and activated model catalogs observed
    /// under the shared mutation lock. Callers keep this Arc for the complete selection.
    /// 返回一个 Arc，持有在共享变更锁下观察到的精确清单代际和已激活模型目录；调用方在
    /// 完整选择过程中持续持有此 Arc。
    fn selection_catalog_snapshot(&self) -> Result<Arc<ResolvedProviderGeneration>, String> {
        self.resolved_provider_generation
            .read()
            .map(|generation| Arc::clone(&generation))
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))
    }

    /// Atomically replaces one system manifest by swapping the whole catalog generation.
    /// 通过整体交换目录代数原子替换一个系统清单。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the provider and `manifest` is the fully compiled and validated
    /// replacement; the caller must complete network, disk, and validation work outside the
    /// lock. The candidate catalog is built completely under a short read lock and swapped
    /// under a short write lock, so readers always observe one full old or one full new
    /// generation (plan §R3.5).
    /// `provider_id` 选择供应商，`manifest` 是完全编译且校验的替换件；调用方必须在锁外
    /// 完成网络、磁盘与校验工作。候选目录在短读锁下完整构建、短写锁下整体交换，
    /// 因此读取方永远观察到完整旧代或完整新代（方案 §R3.5）。
    pub fn replace_system_manifest(
        &self,
        provider_id: &str,
        manifest: Arc<ResolvedProviderManifest>,
    ) -> Result<(), String> {
        self.replace_system_manifest_transactional(provider_id, manifest)
            .map(|_mutation| ())
    }

    /// Atomically replaces one staged system manifest and returns its exact CAS before-image.
    /// 原子替换一份暂存系统 Manifest，并返回其精确 CAS 前镜像。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact provider and `manifest` is the fully compiled candidate.
    /// `provider_id` 选择精确供应商，`manifest` 是已完整编译的候选清单。
    ///
    /// # Returns
    /// 返回值
    /// The immutable before/after staging catalogs after publication, or a bounded CAS failure.
    /// 返回发布后的不可变前后暂存目录，或有界 CAS 失败。
    pub fn replace_system_manifest_transactional(
        &self,
        provider_id: &str,
        manifest: Arc<ResolvedProviderManifest>,
    ) -> Result<ProviderStagedManifestCatalogMutation, String> {
        for _attempt in 0..64 {
            let prepared =
                self.prepare_system_manifest_replacement(provider_id, Arc::clone(&manifest))?;
            if let Some(mutation) = self.publish_prepared_system_manifest_transactional(prepared)? {
                return Ok(mutation);
            }
        }
        Err(format!(
            "Provider `{provider_id}` manifest replacement exceeded the bounded catalog CAS retry limit."
        ))
    }

    /// Builds one complete replacement catalog without publishing any partial state.
    /// 构建一份完整替换目录且不发布任何部分状态。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact entry and `manifest` is fully compiled and validated.
    /// `provider_id` 选择精确条目，`manifest` 为已完整编译校验的清单。
    ///
    /// # Returns
    /// 返回值
    /// A complete candidate tied to its observed base generation, or a build error.
    /// 返回绑定其观察基代数的完整候选，或构建错误。
    pub fn prepare_system_manifest_replacement(
        &self,
        provider_id: &str,
        manifest: Arc<ResolvedProviderManifest>,
    ) -> Result<PreparedSystemManifestReplacement, String> {
        let current = self
            .bundled_manifests
            .read()
            .map_err(|_| String::from("The AI manifest lock is poisoned."))?
            .clone();
        let next_generation = current.generation.checked_add(1).ok_or_else(|| {
            String::from("The AI manifest catalog generation exhausted the supported u64 range.")
        })?;
        let mut manifests = current.manifests.clone();
        manifests.insert(provider_id.to_string(), manifest);
        let catalog =
            crate::provider_manifest::ResolvedProviderCatalog::build_with_bundled_failures(
                manifests,
                current.bundled_failures.clone(),
                next_generation,
            )?;
        Ok(PreparedSystemManifestReplacement {
            provider_id: provider_id.to_string(),
            base_generation: current.generation,
            catalog: Arc::new(catalog),
        })
    }

    /// Publishes one prepared replacement only when its base generation is still current.
    /// 仅在候选基代数仍为当前值时发布已准备替换。
    ///
    /// # Parameters
    /// 参数
    /// `prepared` owns the complete candidate and its compare-and-swap identity.
    /// `prepared` 持有完整候选及其比较交换身份。
    ///
    /// # Returns
    /// 返回值
    /// `true` after publication or an identical-content no-op, `false` on a benign generation
    /// conflict, or a lock failure.
    /// 发布完成或内容相同而无需发布时返回 `true`，良性代际冲突返回 `false`，锁失败则返回错误。
    pub fn publish_prepared_system_manifest(
        &self,
        prepared: PreparedSystemManifestReplacement,
    ) -> Result<bool, String> {
        self.publish_prepared_system_manifest_transactional(prepared)
            .map(|mutation| mutation.is_some())
    }

    /// Publishes one prepared staging replacement and captures both sides under the same lock.
    /// 在同一把锁内发布一份已准备的暂存替换并捕获前后两侧。
    ///
    /// # Parameters
    /// 参数
    /// `prepared` owns the complete candidate and the generation it observed.
    /// `prepared` 持有完整候选及其观察到的代次。
    ///
    /// # Returns
    /// 返回值
    /// `Some` with an exact mutation after publication/no-op, or `None` on a benign CAS conflict.
    /// 发布或 no-op 后返回带精确变更的 `Some`，良性 CAS 冲突返回 `None`。
    fn publish_prepared_system_manifest_transactional(
        &self,
        prepared: PreparedSystemManifestReplacement,
    ) -> Result<Option<ProviderStagedManifestCatalogMutation>, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let mut slot = self
            .bundled_manifests
            .write()
            .map_err(|_| String::from("The AI manifest lock is poisoned."))?;
        if slot.generation != prepared.base_generation {
            return Ok(None);
        }
        // before is cloned while the staging slot is exclusively locked, so no mutation can
        // escape between the compare step and snapshot capture.
        // before 在暂存槽独占锁内克隆，因此比较步骤与快照捕获之间不会逸出任何变更。
        let before = Arc::clone(&slot);
        // Identical content is already active. Treat it as a successful no-op so callers neither
        // create an observation-only generation nor spin through the bounded CAS retry loop.
        // 相同内容已经生效。将其视为成功 no-op，避免调用方创建仅含观测差异的新代次，
        // 也避免在有界 CAS 重试循环中空转。
        if slot.content_sha256 == prepared.catalog.content_sha256 {
            return Ok(Some(ProviderStagedManifestCatalogMutation {
                before: Arc::clone(&before),
                after: before,
            }));
        }
        if prepared
            .catalog
            .manifest(prepared.provider_id.as_str())
            .is_none()
        {
            return Err(format!(
                "Prepared provider manifest catalog is missing `{}`.",
                prepared.provider_id
            ));
        }
        *slot = prepared.catalog;
        Ok(Some(ProviderStagedManifestCatalogMutation {
            before,
            after: Arc::clone(&slot),
        }))
    }

    /// Restores one exact staged Manifest catalog only while its recorded after-image is current.
    /// 仅当记录的后镜像仍为当前值时恢复一份精确暂存 Manifest 目录。
    ///
    /// # Parameters
    /// 参数
    /// `mutation` is the opaque before/after image returned by the matching replacement.
    /// `mutation` 是对应替换返回的不透明前后镜像。
    ///
    /// # Returns
    /// 返回值
    /// Success after restoring the original Arc, including its exact generation and content hash;
    /// a later staging mutation causes a controlled CAS rejection.
    /// 恢复原始 Arc（包括精确代次与内容哈希）后返回成功；后续暂存变更会触发受控 CAS 拒绝。
    pub fn restore_staged_system_manifest_catalog(
        &self,
        mutation: &ProviderStagedManifestCatalogMutation,
    ) -> Result<(), String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let mut slot = self
            .bundled_manifests
            .write()
            .map_err(|_| String::from("The AI manifest lock is poisoned."))?;
        if !Arc::ptr_eq(&slot, &mutation.after) {
            return Err(format!(
                "The staged provider Manifest catalog changed after generation {}; compensation refused to overwrite newer state.",
                mutation.after.generation
            ));
        }
        *slot = Arc::clone(&mutation.before);
        Ok(())
    }

    /// Returns published system manifest revisions for UI revision markers.
    /// 返回供 UI 修订标记使用的已发布系统清单修订号。
    ///
    /// # Returns
    /// 返回值
    /// Provider id to manifest revision pairs under a short read lock.
    /// 在短读锁下返回供应商 ID 到清单修订号的映射。
    pub fn system_manifest_revisions(&self) -> Result<Vec<(String, u64)>, String> {
        let generation = self
            .resolved_provider_generation
            .read()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?;
        Ok(generation
            .manifests
            .manifests
            .iter()
            .map(|(provider_id, manifest)| (provider_id.clone(), manifest.revision))
            .collect())
    }

    /// Returns every system provider identity pinned by the trusted bundled index.
    /// 返回可信内置 Index 固定的全部系统供应商身份。
    ///
    /// # Returns
    /// 返回值
    /// Deterministically ordered executable and provider-locally isolated identities.
    /// 按确定性顺序排列的可执行身份与逐供应商隔离身份。
    pub fn system_manifest_provider_ids(&self) -> Result<Vec<String>, String> {
        let generation = self
            .resolved_provider_generation
            .read()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?;
        Ok(generation
            .manifests
            .bundled_provider_ids()
            .into_iter()
            .collect())
    }

    /// Returns one provider-local bundled baseline failure from the published generation.
    /// 返回已发布代际中的一条逐供应商内置基线失败。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact identity pinned by the bundled integrity index.
    /// `provider_id` 是内置完整性 Index 固定的精确身份。
    ///
    /// # Returns
    /// 返回值
    /// The bounded failure when the shipped fallback is invalid, otherwise `None`.
    /// 随包回退无效时返回有界失败，否则返回 `None`。
    pub fn bundled_manifest_failure(
        &self,
        provider_id: &str,
    ) -> Result<Option<crate::provider_manifest::BundledProviderManifestFailure>, String> {
        let generation = self
            .resolved_provider_generation
            .read()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?;
        Ok(generation
            .manifests
            .bundled_failures
            .get(provider_id)
            .cloned())
    }

    /// Clones one published compiled system manifest handle under a short read lock.
    /// 在短读锁下克隆一份已发布的已编译系统清单句柄。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact system provider identifier.
    /// `provider_id` 是精确系统供应商标识。
    ///
    /// # Returns
    /// 返回值
    /// The compiled manifest when the provider is registered, otherwise `None`.
    /// 供应商已注册时返回编译清单，否则返回 `None`。
    pub fn system_manifest(&self, provider_id: &str) -> Option<Arc<ResolvedProviderManifest>> {
        self.resolved_provider_generation
            .read()
            .ok()
            .and_then(|generation| generation.manifests.manifest_arc(provider_id))
    }

    /// Captures one system manifest and all paired model-catalog facts from one generation.
    /// 从一个代际捕获一份系统 Manifest 及全部配对模型目录事实。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact system provider whose diagnostics are requested;
    /// `configuration` supplies the matching account and access inventory.
    /// `provider_id` 是请求诊断的精确系统供应商；`configuration` 提供配对账号与访问目录。
    ///
    /// # Returns
    /// 返回值
    /// A coherent snapshot when registered, `None` when absent, or a lock failure.
    /// 已注册时返回一致快照，不存在时返回 `None`，否则返回锁错误。
    pub fn provider_manifest_runtime_snapshot(
        &self,
        provider_id: &str,
        configuration: &PersistedAiConfiguration,
    ) -> Result<Option<ProviderManifestRuntimeSnapshot>, String> {
        // generationGuard pins the single published Arc while every returned field is cloned.
        // generationGuard 在克隆每个返回字段期间固定唯一已发布 Arc。
        let generation_guard = self
            .resolved_provider_generation
            .read()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?;
        // manifest is cloned from the exact catalog paired with both model-catalog maps.
        // manifest 从与两份模型目录映射精确配对的目录克隆。
        let Some(manifest) = generation_guard.manifests.manifest_arc(provider_id) else {
            return Ok(None);
        };
        // runtimeConfiguration prevents an unrelated isolated provider connection from failing
        // this healthy provider's complete diagnostic projection.
        // runtimeConfiguration 防止无关的已隔离供应商连接导致当前健康供应商的完整诊断投影失败。
        let runtime_configuration = runtime_configuration_for_manifest_catalog(
            configuration,
            generation_guard.manifests.as_ref(),
        );
        let configuration = &runtime_configuration;
        // supportConfiguration removes only user account switches; manifest and account-plan
        // membership remain authoritative for the diagnostic inventory.
        // supportConfiguration 仅移除用户账号开关；Manifest 与账号套餐成员关系仍是诊断目录权威。
        let mut support_configuration = configuration.clone();
        support_configuration
            .connection_model_access_policies
            .clear();
        // modelSupportCatalogs is rebuilt from the exact frozen source, plan, and manifest facts.
        // modelSupportCatalogs 从精确冻结的来源、套餐与 Manifest 事实重建。
        let model_support_catalogs = resolve_model_catalog_overlays_from_inputs(
            &support_configuration,
            generation_guard.manifests.as_ref(),
            &generation_guard.source_catalogs,
            &generation_guard.usage_cache,
        )?
        .snapshot();
        Ok(Some(ProviderManifestRuntimeSnapshot {
            manifest,
            effective_catalogs: effective_catalog_snapshot_from_generation(&generation_guard),
            model_support_catalogs,
            source_catalogs: generation_guard.source_catalogs.snapshot(),
            manifest_generation: generation_guard.manifests.generation,
            catalog_revision: generation_guard.manifests.content_sha256.clone(),
        }))
    }

    /// Clones one staged system manifest for activation-transaction internals.
    /// 为激活事务内部克隆一份暂存系统清单。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact system provider identifier.
    /// `provider_id` 是精确系统供应商标识。
    ///
    /// # Returns
    /// 返回值
    /// The staged manifest, which may differ from the published generation until activation.
    /// 返回暂存 Manifest；在激活前它可能与已发布代际不同。
    #[cfg(test)]
    pub(crate) fn staged_system_manifest(
        &self,
        provider_id: &str,
    ) -> Option<Arc<ResolvedProviderManifest>> {
        self.bundled_manifests
            .read()
            .ok()
            .and_then(|catalog| catalog.manifest_arc(provider_id))
    }

    /// Returns whether any configured connection for a provider currently exposes a model.
    /// 返回某个供应商是否存在任一已配置连接在当前进程内目录中暴露指定模型。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `configuration` is the exact persisted connection graph; `provider_id` and `model_id`
    /// identify the model whose current runtime visibility is queried.
    /// `configuration` 是精确的持久化连接图；`provider_id` 与 `model_id` 标识待查询当前运行时可见性的模型。
    ///
    /// # Returns
    /// 返回值
    ///
    /// `true` only when a matching configured connection has a current in-memory catalog entry.
    /// 仅当匹配的已配置连接具有当前内存目录条目时返回 `true`。
    pub fn model_is_available(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        // generation freezes enablement defaults and catalog membership for this whole decision.
        // generation 为本次完整判断冻结启用默认值与目录成员关系。
        let generation = self.selection_catalog_snapshot()?;
        let manifest = generation.manifests.manifest(provider_id);
        // catalogs applies the same source-aware fallback rule as execution selection.
        // catalogs 应用与执行选择相同的来源感知回退规则。
        let catalogs = effective_catalog_snapshot_from_generation(&generation);
        Ok(configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .filter(|connection| {
                !configuration.connection_is_pending_validation(connection.id.as_str())
            })
            .any(|connection| {
                runtime_model_is_enabled_from_manifest(
                    configuration,
                    provider_id,
                    connection.service_entry_id.as_str(),
                    Some(connection.credential_profile_id.as_str()),
                    model_id,
                    manifest,
                ) && catalogs
                    .get(connection.id.as_str())
                    .is_some_and(|catalog| catalog.models.iter().any(|model| model.id == model_id))
            }))
    }

    /// Returns every provider/model selector currently exposed by configured in-memory catalogs.
    /// 返回当前已配置内存目录暴露的全部供应商/模型选择器。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `configuration` is the exact persisted connection graph used to filter orphan catalogs.
    /// `configuration` 是用于过滤孤立目录的精确持久化连接图。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A stable deduplicated `provider/model` selector set.
    /// 返回稳定去重后的 `provider/model` 选择器集合。
    pub fn available_model_selectors(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<std::collections::BTreeSet<String>, String> {
        // generation and catalogs remain paired throughout selector construction.
        // generation 与 catalogs 在整个选择器构建过程中保持配对。
        let generation = self.selection_catalog_snapshot()?;
        let catalogs = effective_catalog_snapshot_from_generation(&generation);
        Ok(configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| {
                !configuration.connection_is_pending_validation(connection.id.as_str())
            })
            .filter_map(|connection| {
                catalogs
                    .get(connection.id.as_str())
                    .map(|catalog| (connection, catalog))
            })
            .flat_map(|(connection, catalog)| {
                catalog
                    .models
                    .iter()
                    .filter(|model| {
                        runtime_model_is_enabled_from_manifest(
                            configuration,
                            connection.provider_id.as_str(),
                            connection.service_entry_id.as_str(),
                            Some(connection.credential_profile_id.as_str()),
                            model.id.as_str(),
                            generation
                                .manifests
                                .manifest(connection.provider_id.as_str()),
                        ) && model_supports_text_conversation(&model.capabilities)
                    })
                    .map(move |model| format!("{}/{}", connection.provider_id, model.id))
            })
            .collect())
    }

    /// Returns the latest process-local synchronization errors.
    /// 返回最近的进程内同步错误。
    ///
    /// # Returns
    /// 返回值
    /// A cloned connection-to-error map.
    /// 返回克隆后的连接到错误映射。
    pub fn catalog_error_snapshot(&self) -> Result<BTreeMap<String, String>, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        Ok(self
            .catalog_errors
            .read()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
            .clone())
    }

    /// Returns provider-discovery catalogs frozen in one published generation before overlays.
    /// 返回在一个已发布代际中、应用用户覆盖前冻结的供应商发现目录。
    ///
    /// # Returns
    /// 返回值
    /// A coherent cloned connection-to-source-catalog map that excludes unactivated candidates.
    /// 返回排除未激活候选的一致连接到源目录克隆映射。
    pub fn source_catalog_snapshot(
        &self,
    ) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        Ok(self
            .selection_catalog_snapshot()?
            .source_catalogs
            .snapshot())
    }

    /// Returns one staged source catalog solely for an isolated credential-validation transaction.
    /// 仅为隔离凭据验证事务返回一份暂存源目录。
    ///
    /// This method never changes publication state. Ordinary request, UI, and task readers must
    /// use `source_catalog_snapshot`, which is bound to the immutable published generation.
    /// 此方法绝不改变发布状态。普通请求、UI 与任务读取方必须使用绑定不可变已发布代际的
    /// `source_catalog_snapshot`。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the exact temporary or durable connection observed by discovery.
    /// `connection_id` 标识发现过程观察的精确临时或持久连接。
    ///
    /// # Returns
    /// 返回值
    /// The current staged source catalog when present, without exposing any other candidate.
    /// 当前暂存源目录存在时返回它，且不暴露任何其他候选。
    pub fn staged_source_catalog(
        &self,
        connection_id: &str,
    ) -> Result<Option<ConnectionModelCatalog>, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        Ok(self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .get(connection_id)
            .cloned())
    }

    /// Stages one already validated connection observation without publishing it to requests.
    /// 暂存一项已验证连接观测，且不向请求发布。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` is the durable credential identity, `catalog` is its optional raw source,
    /// `status` is the exact typed discovery result, and `refresh_error` is the controlled
    /// diagnostic retained only for provider settings.
    /// `connection_id` 是持久凭据身份，`catalog` 是其可选原始来源，`status` 是精确类型化
    /// 发现结果，`refresh_error` 是仅供供应商设置保留的受控诊断。
    ///
    /// # Returns
    /// 返回值
    /// Success after the mutable staging maps own the observation. The immutable provider
    /// generation remains unchanged until an explicit configuration activation.
    /// 可变暂存映射持有该观测后返回成功；不可变供应商代际在显式配置激活前保持不变。
    pub fn stage_connection_catalog_observation(
        &self,
        connection_id: &str,
        catalog: Option<ConnectionModelCatalog>,
        status: CatalogDiscoveryStatus,
        refresh_error: Option<String>,
    ) -> Result<(), String> {
        // beforeImage makes the single-observation compatibility seam use the same CAS boundary
        // as production batches.
        // beforeImage 让单观测兼容接缝使用与生产批次相同的 CAS 边界。
        let before_image = self.capture_activation_snapshot()?;
        self.stage_connection_catalog_observations_if_current(
            &before_image,
            &[StagedConnectionCatalogObservation {
                connection_id: connection_id.to_string(),
                catalog,
                status,
                refresh_error,
            }],
        )
        .map(|_| ())
    }

    /// Atomically stages a complete observation batch when its exact before-image is still current.
    /// 仅在精确前镜像仍为当前状态时，原子暂存一批完整观测。
    ///
    /// # Parameters
    /// 参数
    /// `expected_before` is the compare side and `observations` contains distinct durable
    /// connection identities with their complete discovery facts.
    /// `expected_before` 是比较端，`observations` 包含互不重复的持久连接身份及其完整发现事实。
    ///
    /// # Returns
    /// 返回值
    /// The exact post-staging image suitable for deterministic compensation, or no write on any
    /// validation, lock, revision, or compare failure.
    /// 返回适用于确定性补偿的精确暂存后镜像；若发生任何校验、锁、修订号或比较失败则不写入。
    pub fn stage_connection_catalog_observations_if_current(
        &self,
        expected_before: &ProviderRuntimeActivationSnapshot,
        observations: &[StagedConnectionCatalogObservation],
    ) -> Result<ProviderRuntimeActivationSnapshot, String> {
        if observations.is_empty() {
            return Err(String::from(
                "A staged connection catalog observation batch cannot be empty.",
            ));
        }
        // seenConnectionIds rejects ambiguous last-write-wins behavior before any lock or write.
        // seenConnectionIds 在任何加锁或写入前拒绝含糊的后写覆盖行为。
        let mut seen_connection_ids = BTreeSet::new();
        for observation in observations {
            if observation.connection_id.trim().is_empty() {
                return Err(String::from(
                    "A staged connection catalog observation requires a non-empty connection id.",
                ));
            }
            if !seen_connection_ids.insert(observation.connection_id.as_str()) {
                return Err(format!(
                    "Staged catalog observations contain duplicate connection `{}`.",
                    observation.connection_id
                ));
            }
            if let Some(catalog) = observation.catalog.as_ref() {
                if catalog.connection_id != observation.connection_id {
                    return Err(format!(
                        "Staged catalog connection `{}` does not match observation connection `{}`.",
                        catalog.connection_id, observation.connection_id
                    ));
                }
                validate_resolved_catalog_limits(catalog)?;
            }
        }
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry staging the connection observations after activation completes.",
            )
        })?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        // currentRevision is the single compare-and-swap input protecting unrelated mutations.
        // currentRevision 是保护无关变更的唯一比较交换输入。
        let current_revision = self.catalog_state_revision.load(Ordering::Acquire);
        if current_revision != expected_before.state_revision {
            return Err(format!(
                "The AI runtime changed while connection observations were prepared (expected state revision {}, observed {}); staging refused.",
                expected_before.state_revision, current_revision
            ));
        }
        // publishedRevision is proven before any candidate map is assigned.
        // publishedRevision 在分配任何候选映射前完成证明。
        let published_revision = current_revision.checked_add(1).ok_or_else(|| {
            String::from("The AI catalog state revision exhausted the supported u64 range.")
        })?;
        // Every guard is acquired before candidate construction and the first assignment.
        // 在候选构建与首次赋值前取得全部守卫。
        let mut catalogs = self
            .catalogs
            .write()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?;
        let mut source_catalogs = self
            .source_catalogs
            .write()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?;
        let activated_effective_catalogs = self
            .activated_effective_catalogs
            .read()
            .map_err(|_| String::from("The activated AI catalog lock is poisoned."))?
            .clone();
        let mut catalog_errors = self
            .catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?;
        let mut catalog_statuses = self
            .catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?;
        let usage_cache = self
            .usage_cache
            .read()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
            .clone();
        let resolved_provider_generation = self.selection_catalog_snapshot()?;
        // Candidate maps are complete before the first shared assignment.
        // 候选映射在首次共享赋值前已经完整。
        let mut next_source_catalogs = source_catalogs.clone();
        let mut next_catalogs = catalogs.clone();
        let mut next_catalog_errors = catalog_errors.clone();
        let mut next_catalog_statuses = catalog_statuses.clone();
        for observation in observations {
            if let Some(catalog) = observation.catalog.as_ref() {
                next_source_catalogs.replace(catalog.clone());
            } else {
                next_source_catalogs.remove(observation.connection_id.as_str());
            }
            // An old effective fallback must never cross a changed provider contract.
            // 旧有效回退绝不能越过已改变的供应商契约。
            next_catalogs.remove(observation.connection_id.as_str());
            if let Some(error) = observation.refresh_error.as_ref() {
                next_catalog_errors.insert(observation.connection_id.clone(), error.clone());
            } else {
                next_catalog_errors.remove(observation.connection_id.as_str());
            }
            next_catalog_statuses.insert(
                observation.connection_id.clone(),
                observation.status.clone(),
            );
        }
        *source_catalogs = next_source_catalogs.clone();
        *catalogs = next_catalogs.clone();
        *catalog_errors = next_catalog_errors.clone();
        *catalog_statuses = next_catalog_statuses.clone();
        self.catalog_state_revision
            .store(published_revision, Ordering::Release);
        Ok(ProviderRuntimeActivationSnapshot {
            state_revision: published_revision,
            catalogs: next_catalogs,
            source_catalogs: next_source_catalogs,
            activated_effective_catalogs,
            catalog_errors: next_catalog_errors,
            catalog_statuses: next_catalog_statuses,
            usage_cache,
            resolved_provider_generation,
        })
    }

    /// Returns the latest closed process-local discovery states.
    /// 返回最近的封闭进程内发现状态。
    ///
    /// # Returns
    /// 返回值
    /// A cloned connection-to-status map.
    /// 返回克隆后的连接到状态映射。
    pub fn catalog_status_snapshot(
        &self,
    ) -> Result<BTreeMap<String, CatalogDiscoveryStatus>, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        Ok(self
            .catalog_statuses
            .read()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?
            .clone())
    }

    /// Removes runtime-only metadata for one deleted connection.
    /// 移除一个已删除连接的仅运行时元数据。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the exact deleted connection.
    /// `connection_id` 标识被删除的精确连接。
    pub fn remove_connection_catalog(&self, connection_id: &str) -> Result<(), String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry connection removal after activation completes.",
            )
        })?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.source_catalogs
            .write()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .remove(connection_id);
        self.catalogs
            .write()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .remove(connection_id);
        self.catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
            .remove(connection_id);
        self.catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?
            .remove(connection_id);
        self.secondary_route_cache
            .write()
            .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?
            .retain(|_, cached| cached.connection_id != connection_id);
        self.refresh_locks
            .lock()
            .map_err(|_| String::from("The AI credential refresh lock is poisoned."))?
            .remove(connection_id);
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Records one catalog refresh failure while preserving the last immutable source snapshot.
    /// 记录一次目录刷新失败，同时保留最近的不可变源快照。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` scopes the observation, `generation` and `observed_at_ms` identify the
    /// attempt, `failure_kind` is closed, and `message` is already controlled for display.
    /// `connection_id` 限定观测作用域，`generation` 与 `observed_at_ms` 标识本次尝试，
    /// `failure_kind` 为封闭分类，`message` 已经完成可展示控制。
    ///
    /// # Returns
    /// 返回值
    /// Success after both error and typed status maps are updated.
    /// 在错误与强类型状态映射均更新后返回成功。
    fn record_catalog_refresh_failure(
        &self,
        connection_id: &str,
        generation: u64,
        observed_at_ms: u64,
        failure_kind: CatalogDiscoveryFailureKind,
        message: &str,
    ) -> Result<(), String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let stale_catalog = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .get(connection_id)
            .cloned();
        self.catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
            .insert(connection_id.to_string(), message.to_string());
        self.catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?
            .insert(
                connection_id.to_string(),
                CatalogDiscoveryStatus {
                    state: if stale_catalog.is_some() {
                        CatalogDiscoveryState::Stale
                    } else {
                        discovery_state_for_failure(failure_kind)
                    },
                    failure_kind: Some(failure_kind),
                    message: Some(message.to_string()),
                    model_count: stale_catalog
                        .as_ref()
                        .map_or(0, |catalog| catalog.models.len()),
                    generation: stale_catalog
                        .as_ref()
                        .map_or(generation, |catalog| catalog.generation),
                    observed_at_ms,
                },
            );
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Refreshes one credential-scoped model catalog from its provider.
    /// 从对应供应商刷新一个凭据作用域的模型目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the persisted public AI configuration and `connection_id` is exact.
    /// `configuration` 是持久化公共 AI 配置，`connection_id` 为精确连接标识。
    ///
    /// # Returns
    /// 返回值
    /// The newly installed process-local catalog.
    /// 返回新安装到进程内的目录。
    pub async fn refresh_connection_catalog(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
    ) -> Result<ConnectionModelCatalog, String> {
        let _refresh_guard = self.catalog_refresh_flight.lock().await;
        let before_image = self.capture_activation_snapshot()?;
        let provider_generation = Arc::clone(&before_image.resolved_provider_generation);
        let isolated_runtime = self.isolated_catalog_activation_runtime(
            &before_image,
            Arc::clone(&provider_generation.manifests),
        );
        let refresh = isolated_runtime
            .refresh_connection_catalog_from_generation(
                configuration,
                connection_id,
                provider_generation,
                true,
            )
            .await;
        let candidate = isolated_runtime.capture_activation_snapshot()?;
        if let Err(publication) =
            self.publish_isolated_catalog_candidate(&before_image, &candidate, None)
        {
            return Err(match refresh {
                Ok(_) => publication,
                Err(primary) => format!(
                    "Provider catalog refresh failed: {primary}; isolated failure-state publication also failed: {publication}"
                ),
            });
        }
        refresh
    }

    /// Observes one candidate catalog in a fully isolated runtime without publishing any state.
    /// 在完全隔离的运行时中观测一个候选目录，且不发布任何状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` and `connection_id` identify the candidate route; `temporary_secret`
    /// optionally overlays a not-yet-durable credential entirely in memory.
    /// `configuration` 与 `connection_id` 标识候选路由；`temporary_secret` 可选地在内存中
    /// 覆盖一份尚未持久化的凭据。
    ///
    /// # Returns
    /// 返回值
    /// The isolated typed status, raw source catalog, and controlled refresh diagnostic. Dropping
    /// or cancelling the future cannot leave a catalog or candidate secret in the main runtime.
    /// 返回隔离的类型化状态、原始来源目录与受控刷新诊断；丢弃或取消 Future 不会在主运行时
    /// 留下目录或候选秘密。
    pub async fn observe_connection_catalog_candidate(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        temporary_secret: Option<&CredentialSecretEnvelope>,
    ) -> Result<IsolatedConnectionCatalogObservation, String> {
        // beforeImage freezes every main-runtime input needed to construct an isolated reader.
        // beforeImage 冻结构建隔离读取方所需的每项主运行时输入。
        let before_image = self.capture_activation_snapshot()?;
        // providerGeneration remains retained for the entire remote observation.
        // providerGeneration 在整个远端观测期间保持持有。
        let provider_generation = Arc::clone(&before_image.resolved_provider_generation);
        // isolatedRuntime owns every mutable catalog projection and therefore needs no cleanup.
        // isolatedRuntime 持有每项可变目录投影，因此不需要清理。
        let mut isolated_runtime = self.isolated_catalog_activation_runtime(
            &before_image,
            Arc::clone(&provider_generation.manifests),
        );
        if let Some(secret) = temporary_secret {
            secret.validate()?;
            // connection supplies the single exact reference shadowed by the in-memory overlay.
            // connection 提供由内存覆盖层遮蔽的唯一精确引用。
            let connection = configuration
                .connection_graph
                .connections
                .get(connection_id)
                .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
            isolated_runtime.secrets = Arc::new(IsolatedCredentialSecretStore::new(
                Arc::clone(&self.secrets),
                connection.secret_reference.clone(),
                secret.clone(),
            ));
        }
        // refreshError remains data when discovery reaches a typed closed failure state.
        // refreshError 在发现到达类型化封闭失败状态时保留为数据。
        let refresh_error = isolated_runtime
            .refresh_connection_catalog_from_generation(
                configuration,
                connection_id,
                provider_generation,
                true,
            )
            .await
            .err();
        // status must be emitted by every accepted discovery outcome, including closed failures.
        // status 必须由每个可接受的发现结果产生，包括封闭失败。
        let status = isolated_runtime
            .catalog_status_snapshot()?
            .remove(connection_id)
            .ok_or_else(|| {
                refresh_error.clone().unwrap_or_else(|| {
                    String::from("Custom provider discovery did not produce a typed status.")
                })
            })?;
        // catalog is the exact raw source retained only by this isolated runtime.
        // catalog 是仅由该隔离运行时保留的精确原始来源。
        let catalog = isolated_runtime.staged_source_catalog(connection_id)?;
        Ok(IsolatedConnectionCatalogObservation {
            status,
            catalog,
            refresh_error,
        })
    }

    /// Begins one runtime catalog activation transaction that may span service projections.
    /// 开始一个可跨越服务投影的运行时目录激活事务。
    ///
    /// # Returns
    /// 返回值
    /// An opaque guard that blocks refresh, usage, removal, and competing activation writers until
    /// the service commits or compensates every visible projection.
    /// 返回一个不透明 Guard；在服务提交或补偿全部可见投影前，它会阻塞刷新、用量、删除和
    /// 竞争激活写入方。
    pub async fn begin_catalog_activation_transaction(
        &self,
    ) -> ProviderCatalogActivationTransaction {
        let owner = Arc::clone(&self.catalog_refresh_flight);
        let guard = Arc::clone(&owner).lock_owned().await;
        ProviderCatalogActivationTransaction {
            owner,
            _guard: guard,
        }
    }

    /// Refreshes affected source catalogs from one staged manifest generation and publishes once.
    /// 从同一暂存清单代际刷新受影响的源目录，并只发布一次。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the complete persisted configuration and `connection_ids` contains the
    /// exact credential connections affected by the staged manifest transaction.
    /// `configuration` 是完整持久化配置，`connection_ids` 包含受暂存清单事务影响的精确凭据连接。
    ///
    /// # Returns
    /// 返回值
    /// The exact immutable runtime image captured after every candidate catalog and staged
    /// manifest crosses the single atomic publication boundary.
    /// 在全部候选目录与暂存清单越过唯一原子发布边界后，返回精确不可变运行时镜像。
    pub async fn activate_staged_manifest_catalogs(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_ids: &[String],
    ) -> Result<ProviderRuntimeActivationSnapshot, String> {
        let transaction = self.begin_catalog_activation_transaction().await;
        self.activate_staged_manifest_catalogs_in_transaction(
            configuration,
            connection_ids,
            &transaction,
        )
        .await
    }

    /// Activates staged manifests while a service-owned catalog transaction remains held.
    /// 在服务持有目录事务期间激活暂存 Manifest。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` and `connection_ids` define the exact projection set; `transaction` must
    /// originate from this runtime and remains owned by the caller through downstream publication.
    /// `configuration` 与 `connection_ids` 定义精确投影集合；`transaction` 必须来自当前
    /// 运行时，并由调用方持有直至下游发布结束。
    ///
    /// # Returns
    /// 返回值
    /// The exact post-publication image produced inside the single publication critical section.
    /// 返回在唯一发布临界区内生成的精确发布后镜像。
    pub async fn activate_staged_manifest_catalogs_in_transaction(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_ids: &[String],
        transaction: &ProviderCatalogActivationTransaction,
    ) -> Result<ProviderRuntimeActivationSnapshot, String> {
        if !Arc::ptr_eq(&self.catalog_refresh_flight, &transaction.owner) {
            return Err(String::from(
                "The provider catalog activation transaction belongs to another runtime.",
            ));
        }
        // beforeImage captures every mutable runtime catalog before discovery starts.
        // beforeImage 在发现开始前捕获每个可变运行时目录。
        let before_image = self.capture_activation_snapshot()?;
        let staged_system_manifests = self
            .bundled_manifests
            .read()
            .map_err(|_| String::from("The AI manifest lock is poisoned."))?
            .clone();
        let mut staged_generation = (*before_image.resolved_provider_generation).clone();
        staged_generation.manifests = Arc::clone(&staged_system_manifests);
        let staged_generation = Arc::new(staged_generation);
        // runtimeConfiguration removes only provider identities without executable authority in
        // this exact staged generation; the persisted repair image remains untouched.
        // runtimeConfiguration 只移除当前精确暂存代际中没有可执行权威的供应商身份；
        // 持久化修复镜像保持不变。
        let runtime_configuration = runtime_configuration_for_manifest_catalog(
            configuration,
            staged_system_manifests.as_ref(),
        );
        let isolated_runtime = self.isolated_catalog_activation_runtime(
            &before_image,
            Arc::clone(&staged_system_manifests),
        );
        // failures retains every exact connection error so one bad account cannot be hidden.
        // failures 保留每个精确连接错误，因此单个坏账号不会被隐藏。
        let mut failures = BTreeMap::new();
        for connection_id in connection_ids {
            // Isolated provider connections remain persisted for repair but are intentionally
            // absent from this activation candidate.
            // 已隔离供应商连接保留在持久化中供修复，但有意不进入本次激活候选。
            if !runtime_configuration
                .connection_graph
                .connections
                .contains_key(connection_id.as_str())
            {
                continue;
            }
            // Provider-tool entries own credentials and tools but intentionally have no
            // conversation model catalog; generation activation must not project one for them.
            // 供应商工具入口只持有凭据与工具，并且有意不存在对话模型目录；代际激活不得为其
            // 投影模型目录。
            let entry_kind = match isolated_runtime.resolve_connection_metadata_from_generation(
                &runtime_configuration,
                connection_id,
                &staged_generation,
            ) {
                Ok((_, entry, _)) => entry.kind,
                Err(error) => {
                    failures.insert(connection_id.clone(), error);
                    continue;
                }
            };
            if entry_kind == crate::ServiceEntryKind::ProviderTools {
                continue;
            }
            if let Err(error) = isolated_runtime
                .refresh_connection_catalog_from_generation(
                    &runtime_configuration,
                    connection_id,
                    Arc::clone(&staged_generation),
                    false,
                )
                .await
            {
                failures.insert(connection_id.clone(), error);
            }
        }
        if !failures.is_empty() {
            // failureSummary is deterministic because BTreeMap preserves stable connection order.
            // failureSummary 具有确定性，因为 BTreeMap 保持稳定连接顺序。
            let failure_summary = failures
                .into_iter()
                .map(|(connection_id, error)| format!("{connection_id}: {error}"))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!(
                "Provider manifest connection projection failed before publication: {failure_summary}"
            ));
        }
        isolated_runtime.reapply_model_catalog_overlays_with_manifests(
            configuration,
            &runtime_configuration,
            Arc::clone(&staged_generation.manifests),
        )?;
        let candidate = isolated_runtime.capture_activation_snapshot()?;
        self.publish_isolated_catalog_candidate(
            &before_image,
            &candidate,
            Some(&staged_system_manifests),
        )
    }

    /// Refreshes one connection against one exact immutable provider generation.
    /// 针对一个精确不可变供应商代际刷新一个连接。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` and `connection_id` identify the account, `provider_generation` freezes
    /// all provider facts, and `publish_effective_catalog` controls whether this individual
    /// refresh may update the externally readable mutable catalog before an activation batch ends.
    /// `configuration` 与 `connection_id` 标识账号，`provider_generation` 冻结全部供应商事实，
    /// `publish_effective_catalog` 控制本次独立刷新能否在激活批次结束前更新外部可读的可变目录。
    ///
    /// # Returns
    /// 返回值
    /// The prepared effective catalog, whether published immediately or held for batch activation.
    /// 返回已准备的有效目录，无论其被立即发布还是等待批量激活。
    async fn refresh_connection_catalog_from_generation(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        provider_generation: Arc<ResolvedProviderGeneration>,
        publish_effective_catalog: bool,
    ) -> Result<ConnectionModelCatalog, String> {
        let observed_at_ms = current_unix_millis()?;
        let (provider, entry, connection) = self.resolve_connection_metadata_from_generation(
            configuration,
            connection_id,
            &provider_generation,
        )?;
        let manifest = provider_generation
            .manifests
            .manifest_arc(provider.id.as_str());
        let source_policy = resolved_model_source_policy(
            configuration,
            provider.id.as_str(),
            entry.id.as_str(),
            manifest.as_deref(),
        )?;
        let generation = self
            .catalogs
            .read()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .get(connection_id)
            .map_or(1, |catalog| catalog.generation.saturating_add(1));
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                )
            })?;
        let account_model_access_enabled = credential_profile.account_model_access.is_some();
        // remoteDiscoveryEnabled requires a remote implementation authorized by the source policy
        // and the active Manifest route set. Credential records never own protocol authority.
        // remoteDiscoveryEnabled 要求远端实现同时获得来源策略与活动 Manifest 路由集合授权；
        // 凭据记录永远不持有协议权威。
        let remote_discovery_enabled = source_policy.includes_discovery()
            && entry
                .protocol_routes
                .iter()
                .filter(|route| crate::client::route_is_available_for_discovery(route))
                .any(|route| {
                    model_discovery_is_remote(route.resolved_model_discovery(entry.model_discovery))
                });
        if source_policy.includes_discovery()
            && !remote_discovery_enabled
            && !source_policy.includes_manual()
        {
            return Err(format!(
                "Provider `{}` service entry `{}` declares discovery identity without a remote discovery implementation.",
                provider.id, entry.id
            ));
        }
        // discoverySecret is loaded only for remote model discovery. Account usage may declare a
        // different credential role and therefore resolves independently below.
        // discoverySecret 仅为远端模型发现加载。账号用量可能声明不同的凭据角色，因此在下方独立解析。
        let discovery_secret = if remote_discovery_enabled {
            Some(
                self.resolve_fresh_execution_secret(
                    connection.id.as_str(),
                    connection.secret_reference.as_str(),
                )
                .await?,
            )
        } else {
            None
        };
        // sourceCatalog contains exactly the non-overlay source layers authorized by the policy.
        // sourceCatalog 仅包含策略授权的非覆盖来源层。
        let source_catalog = if remote_discovery_enabled {
            let discovery = self
                .metadata_client
                .discover_models(
                    &provider,
                    &entry,
                    &connection,
                    discovery_secret
                        .as_ref()
                        .expect("remote discovery must load an execution secret"),
                    manifest.clone(),
                    generation,
                )
                .await;
            match discovery {
                Ok(catalog) => catalog,
                Err(error) => {
                    let failure_kind = catalog_discovery_failure_kind(error.kind);
                    let error_message = error.message;
                    self.record_catalog_refresh_failure(
                        connection_id,
                        generation,
                        observed_at_ms,
                        failure_kind,
                        error_message.as_str(),
                    )?;
                    if publish_effective_catalog && source_policy.includes_manual() {
                        self.install_manual_custom_catalog_without_discovery(
                            configuration,
                            &connection,
                            generation,
                        )?;
                    }
                    return Err(error_message);
                }
            }
        } else if source_policy.includes_manifest() {
            build_manifest_identity_catalog(
                manifest.as_deref().ok_or_else(|| {
                    format!(
                        "Provider `{}` requires Manifest identity without an active Manifest.",
                        provider.id
                    )
                })?,
                &connection,
                generation,
            )?
        } else if source_policy.includes_manual() {
            manual_custom_catalog(configuration, &connection, generation).unwrap_or_else(|| {
                empty_model_source_catalog(
                    connection.id.as_str(),
                    generation,
                    MANUAL_MODEL_CATALOG_SCHEMA_REVISION,
                )
            })
        } else {
            return Err(format!(
                "Provider `{}` service entry `{}` has no authorized model identity source.",
                provider.id, entry.id
            ));
        };
        let source_model_count = source_catalog.models.len();
        let mut catalog = source_catalog.clone();
        if account_model_access_enabled {
            let usage_secret_reference =
                account_usage_secret_reference(&connection, credential_profile)?;
            let usage_secret = self
                .resolve_fresh_execution_secret(connection.id.as_str(), usage_secret_reference)
                .await?;
            let usage = self
                .metadata_client
                .query_account_usage(&entry, &connection, &usage_secret)
                .await;
            self.cache_usage_snapshot(connection_id, usage, true)?;
        }
        let account_usage = self.cached_account_usage(connection_id)?;
        if let Err(error) = apply_model_catalog_overlays_with_account_access(
            configuration,
            provider.id.as_str(),
            entry.id.as_str(),
            account_usage.as_ref(),
            source_policy,
            manifest.as_deref(),
            &mut catalog,
        )
        .and_then(|_| validate_resolved_catalog_limits(&catalog))
        {
            self.record_catalog_refresh_failure(
                connection_id,
                generation,
                observed_at_ms,
                CatalogDiscoveryFailureKind::Failed,
                error.as_str(),
            )?;
            return Err(error);
        }
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.source_catalogs
            .write()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .replace(source_catalog);
        if publish_effective_catalog {
            self.replace_catalog_locked(catalog.clone())?;
        }
        self.catalog_errors
            .write()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
            .remove(connection_id);
        self.catalog_statuses
            .write()
            .map_err(|_| String::from("The AI catalog status lock is poisoned."))?
            .insert(
                connection_id.to_string(),
                CatalogDiscoveryStatus {
                    state: if source_model_count == 0 {
                        CatalogDiscoveryState::Empty
                    } else {
                        CatalogDiscoveryState::Ready
                    },
                    failure_kind: None,
                    message: None,
                    model_count: source_model_count,
                    generation: catalog.generation,
                    observed_at_ms,
                },
            );
        self.advance_catalog_state_revision_locked()?;
        Ok(catalog)
    }

    /// Rebuilds one effective catalog after an account-usage observation changes model access.
    /// 在账号用量观测改变模型访问权后重建一个有效目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies the authoritative overlays and `connection_id` identifies the
    /// exact credential whose source catalog must be reprojected.
    /// `configuration` 提供权威覆盖，`connection_id` 标识需要重新投影源目录的精确凭据。
    ///
    /// # Returns
    /// 返回值
    /// Success after the current source catalog has been atomically reprojected, or success with
    /// no mutation when discovery has not produced a source catalog yet.
    /// 当前源目录完成原子重新投影后返回成功；若发现尚未产生源目录，则成功但不修改状态。
    fn rebuild_catalog_after_usage(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
    ) -> Result<(), String> {
        let (provider, entry, _) =
            self.resolve_connection_metadata(configuration, connection_id)?;
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let source_catalog = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .get(connection_id)
            .cloned();
        let Some(mut catalog) = source_catalog else {
            return Ok(());
        };
        let account_usage = self.cached_account_usage(connection_id)?;
        self.apply_model_catalog_overlays_with_account_access(
            configuration,
            provider.id.as_str(),
            entry.id.as_str(),
            account_usage.as_ref(),
            &mut catalog,
        )?;
        validate_resolved_catalog_limits(&catalog)?;
        self.replace_catalog_locked(catalog)?;
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Applies user overlays and manifest-owned account model access to one connection catalog.
    /// 对一个连接目录应用用户覆盖与 Manifest 持有的账号模型访问规则。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns user layers; `provider_id` and `service_entry_id` identify the
    /// exact manifest scope; `account_usage` is confirmed provider evidence; `catalog`
    /// is rebuilt in place.
    /// `configuration` 持有用户层；`provider_id` 与 `service_entry_id` 标识精确 Manifest 作用域；
    /// `account_usage` 是已确认供应商证据；`catalog` 会被原位重建。
    ///
    /// # Returns
    /// 返回值
    /// Success after the staged manifest and user layers produce one valid effective catalog.
    /// 暂存 Manifest 与用户层生成一份有效生效目录后返回成功。
    fn apply_model_catalog_overlays_with_account_access(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        service_entry_id: &str,
        account_usage: Option<&AccountUsageSnapshot>,
        catalog: &mut ConnectionModelCatalog,
    ) -> Result<(), String> {
        let generation = self.selection_catalog_snapshot()?;
        apply_model_catalog_overlays_from_generation(
            configuration,
            provider_id,
            service_entry_id,
            account_usage,
            generation.as_ref(),
            catalog,
        )
    }

    /// Rebuilds every effective in-memory catalog from immutable discovery snapshots.
    /// 从不可变发现快照重建每个有效内存目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies the latest custom models and user capability differences.
    /// `configuration` 提供最新自定义模型与用户能力差异。
    pub fn reapply_model_catalog_overlays(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<(), String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry the catalog update after activation completes.",
            )
        })?;
        // published_manifests is the only system authority ordinary configuration activation may
        // observe; bundled_manifests can already contain a signed candidate staged for a later batch.
        // published_manifests 是普通配置激活唯一可观察的系统权威；bundled_manifests 可能已包含
        // 等待后续批次激活的签名候选。
        let published_manifests = self
            .resolved_provider_generation
            .read()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?
            .manifests
            .clone();
        // system_manifests removes previously activated custom documents before the candidate
        // configuration contributes its exact custom set.
        // system_manifests 在候选配置贡献其精确自定义集合前移除此前激活的自定义文档。
        let system_manifests = published_manifests
            .manifests
            .iter()
            .filter(|(_, manifest)| manifest.descriptor.kind == ProviderKind::System)
            .map(|(provider_id, manifest)| (provider_id.clone(), Arc::clone(manifest)))
            .collect::<BTreeMap<_, _>>();
        // system_catalog preserves the published generation identity and cannot expose staged bytes.
        // system_catalog 保留已发布代际身份，且不能暴露暂存字节。
        let system_catalog = Arc::new(
            crate::provider_manifest::ResolvedProviderCatalog::build_with_bundled_failures(
                system_manifests,
                published_manifests.bundled_failures.clone(),
                published_manifests.generation,
            )?,
        );
        let runtime_configuration =
            runtime_configuration_for_manifest_catalog(configuration, system_catalog.as_ref());
        self.reapply_model_catalog_overlays_with_manifests(
            configuration,
            &runtime_configuration,
            system_catalog,
        )
    }

    /// Rebuilds and publishes catalogs against one exact manifest catalog.
    /// 针对一个精确清单目录重建并发布模型目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies account and user overlays; `manifests` is the immutable
    /// published or explicitly staged manifest catalog that must share the activation boundary.
    /// `configuration` 提供账号与用户覆盖；`manifests` 是必须共享激活边界的不可变已发布
    /// 或显式暂存清单目录。
    ///
    /// # Returns
    /// 返回值
    /// Success after catalogs and the exact supplied manifests replace the former generation.
    /// 在目录与精确传入清单共同替换旧代际后返回成功。
    fn reapply_model_catalog_overlays_with_manifests(
        &self,
        published_configuration: &PersistedAiConfiguration,
        runtime_configuration: &PersistedAiConfiguration,
        system_manifests: Arc<crate::provider_manifest::ResolvedProviderCatalog>,
    ) -> Result<(), String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let current_manifests = self
            .resolved_provider_generation
            .read()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?
            .manifests
            .clone();
        let manifests = build_activated_provider_manifest_catalog(
            runtime_configuration,
            system_manifests.as_ref(),
            current_manifests.as_ref(),
        )?;
        let effective = self.resolve_model_catalog_overlays_with_manifests(
            runtime_configuration,
            manifests.as_ref(),
        )?;
        let source_catalogs = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .clone();
        // publishedRevision is proven before any runtime field changes so exhaustion cannot
        // leave a partially installed configuration/catalog image.
        // publishedRevision 在任何运行时字段改变前完成证明，避免耗尽错误留下部分安装的
        // 配置/目录镜像。
        let current_revision = self.catalog_state_revision.load(Ordering::Acquire);
        let published_revision = current_revision.checked_add(1).ok_or_else(|| {
            String::from("The AI catalog state revision exhausted the supported u64 range.")
        })?;
        // usageCache freezes the exact plan facts used while the candidate was resolved.
        // usageCache 冻结解析候选时使用的精确套餐事实。
        let usage_cache = self.usage_cache_snapshot_locked()?;
        let candidate = Arc::new(ResolvedProviderGeneration {
            configuration: Arc::new(published_configuration.clone()),
            manifests,
            activated_effective_catalogs: effective.clone(),
            source_catalogs,
            usage_cache,
        });
        // Every fallible write guard is acquired before the first assignment. The immutable
        // generation is assigned last, making configuration, manifests, and effective catalogs
        // visible through one Arc publication boundary.
        // 在首次赋值前取得全部可能失败的写守卫。不可变代际最后赋值，使配置、Manifest 与
        // 有效目录通过同一个 Arc 发布边界对外可见。
        let mut catalogs = self
            .catalogs
            .write()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?;
        let mut activated_effective_catalogs = self
            .activated_effective_catalogs
            .write()
            .map_err(|_| String::from("The activated AI model catalog lock is poisoned."))?;
        let mut resolved_provider_generation = self
            .resolved_provider_generation
            .write()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?;
        *catalogs = effective.clone();
        // The activation boundary freezes the next effective catalog; request-boundary
        // selections read this snapshot so unactivated configuration mutations stay invisible
        // until the next explicit activation (plan §R4.8).
        // 激活边界冻结下一代有效目录；请求边界选择读取该快照，使未激活的配置变更在
        // 下一次显式激活前不可见（方案 §R4.8）。
        *activated_effective_catalogs = effective;
        *resolved_provider_generation = candidate;
        self.catalog_state_revision
            .store(published_revision, Ordering::Release);
        Ok(())
    }

    /// Validates every currently reconstructable account catalog for one candidate configuration.
    /// 校验一个候选配置下当前所有可重建的账号目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` contains the complete proposed provider defaults, template decisions,
    /// manual declarations, availability choices, and user capability differences.
    /// `configuration` 包含完整候选供应商默认值、模板决策、手工声明、可用性选择与用户能力差异。
    ///
    /// # Returns
    /// 返回值
    /// Success when every source-backed or manual account projection has valid final limits.
    /// 当每个源目录或手工账号投影都具有有效最终限制时返回成功。
    pub fn validate_model_catalog_overlays(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<(), String> {
        self.resolve_model_catalog_overlays(configuration)
            .map(|_| ())
    }

    /// Resolves every currently reconstructable account catalog without mutating runtime state.
    /// 在不修改运行时状态的前提下解析当前全部可重建账号目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the complete candidate configuration whose overlays are evaluated.
    /// `configuration` 是待求值覆盖的完整候选配置。
    ///
    /// # Returns
    /// 返回值
    /// A stable connection-to-effective-catalog snapshot suitable for validation and fallback decisions.
    /// 返回稳定的连接到有效目录快照，可用于校验与降级决策。
    pub fn resolved_model_catalog_snapshot(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        Ok(self
            .resolve_model_catalog_overlays(configuration)?
            .snapshot())
    }

    /// Previews one configuration against the exact mutable sources awaiting activation.
    /// 针对等待激活的精确可变来源预览一份配置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the complete candidate whose active selection and model membership
    /// must be reconciled before durable persistence.
    /// `configuration` 是必须在持久化前协调活动选择与模型成员关系的完整候选。
    ///
    /// # Returns
    /// 返回值
    /// A detached effective-catalog snapshot built in an isolated runtime. No request-visible
    /// generation or mutable main-runtime catalog is changed.
    /// 返回在隔离运行时中构建的独立有效目录快照；不会改变任何请求可见代际或主运行时可变目录。
    pub fn preview_staged_model_catalog_overlays(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry the staged catalog preview after activation completes.",
            )
        })?;
        // beforeImage freezes every staged source and the published Manifest authority used by
        // the detached preview.
        // beforeImage 冻结独立预览使用的全部暂存来源与已发布 Manifest 权威。
        let before_image = self.capture_activation_snapshot()?;
        // isolatedRuntime ensures candidate configuration compilation cannot publish any field.
        // isolatedRuntime 确保候选配置编译不会发布任何字段。
        let isolated_runtime = self.isolated_catalog_activation_runtime(
            &before_image,
            Arc::clone(&before_image.resolved_provider_generation.manifests),
        );
        isolated_runtime.reapply_model_catalog_overlays(configuration)?;
        isolated_runtime.catalog_snapshot()
    }

    /// Reports conversation-model support independently from account access switches.
    /// 报告独立于账号访问开关的对话模型支持状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns provider membership, while `provider_id` and `model_id` select one model.
    /// `configuration` 持有供应商成员关系，`provider_id` 与 `model_id` 选择一个模型。
    ///
    /// # Returns
    /// 返回值
    /// `true` when a manual declaration or any account conversation support catalog contains the model.
    /// 当手工声明或任一账号对话支持目录包含该模型时返回 `true`。
    pub fn provider_conversation_model_is_supported(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        // generation binds support reconstruction to one published source/rule/plan snapshot.
        // generation 把支持目录重建绑定到一份已发布来源/规则/套餐快照。
        let generation = self.selection_catalog_snapshot()?;
        provider_conversation_model_is_supported_in_generation(
            configuration,
            provider_id,
            model_id,
            generation.as_ref(),
        )
    }

    /// Reports all manageable provider model identities independently from account access switches.
    /// 报告独立于账号访问开关的全部可管理供应商模型身份。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns provider membership, while `provider_id` and `model_id` select one
    /// conversation, manual, or signed provider-tool template model.
    /// `configuration` 持有供应商成员关系，`provider_id` 与 `model_id` 选择一个对话、手工或
    /// 签名供应商工具模板模型。
    ///
    /// # Returns
    /// 返回值
    /// `true` when either the conversation inventory or the signed provider-tool template inventory
    /// declares the exact model.
    /// 当对话目录或签名供应商工具模板目录声明该精确模型时返回 `true`。
    pub fn provider_model_is_supported(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        // generation keeps conversation and provider-tool inventories on one publication boundary.
        // generation 让对话目录与供应商工具目录处于同一发布边界。
        let generation = self.selection_catalog_snapshot()?;
        if provider_conversation_model_is_supported_in_generation(
            configuration,
            provider_id,
            model_id,
            generation.as_ref(),
        )? {
            return Ok(true);
        }
        Ok(
            provider_tool_template_models_in_generation(configuration, generation.as_ref())?
                .iter()
                .any(|model| model.provider_id == provider_id && model.model_id == model_id),
        )
    }

    /// Returns provider-tool template models independently from their provider-level enabled state.
    /// 返回独立于供应商级启用状态的供应商工具模板模型。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies the exact configured connections and credential profiles used to
    /// decide which signed template bindings are manageable on this installation.
    /// `configuration` 提供精确已配置连接与凭据配置档，用于判断本机可管理哪些签名模板绑定。
    ///
    /// # Returns
    /// 返回值
    /// One deterministic connection/model declaration list from a single immutable generation.
    /// 返回来自单一不可变代际的确定性连接/模型声明列表。
    pub fn provider_tool_template_models(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<Vec<DeclaredProviderToolTemplateModel>, String> {
        // generation keeps template provenance and concrete target contracts coherent for this call.
        // generation 让本次调用中的模板来源与具体目标契约保持一致。
        let generation = self.selection_catalog_snapshot()?;
        provider_tool_template_models_in_generation(configuration, generation.as_ref())
    }

    /// Applies one candidate configuration to immutable source catalogs in memory.
    /// 在内存中把一份候选配置应用到不可变源目录。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies account access, manual membership, and capability differences.
    /// `configuration` 提供账号访问、手工成员关系与能力差异。
    ///
    /// # Returns
    /// 返回值
    /// Complete effective catalogs after every limit has passed strict validation.
    /// 返回全部限制通过严格校验后的完整有效目录。
    fn resolve_model_catalog_overlays(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<RuntimeConnectionCatalogs, String> {
        // generation supplies every published input as one coherent immutable value.
        // generation 以一个一致不可变值提供全部已发布输入。
        let generation = self.selection_catalog_snapshot()?;
        let runtime_configuration = runtime_configuration_for_manifest_catalog(
            configuration,
            generation.manifests.as_ref(),
        );
        resolve_model_catalog_overlays_from_inputs(
            &runtime_configuration,
            generation.manifests.as_ref(),
            &generation.source_catalogs,
            &generation.usage_cache,
        )
    }

    /// Resolves effective catalogs against one exact manifest catalog without publishing it.
    /// 针对一个精确清单目录解析有效模型目录，但不发布它。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies account/user overlays and `manifests` freezes all system rules
    /// used by this complete resolution pass.
    /// `configuration` 提供账号/用户覆盖，`manifests` 冻结本次完整解析使用的全部系统规则。
    ///
    /// # Returns
    /// 返回值
    /// One complete effective catalog set derived from immutable sources and the supplied rules.
    /// 返回从不可变来源与传入规则派生的一份完整有效目录集合。
    fn resolve_model_catalog_overlays_with_manifests(
        &self,
        configuration: &PersistedAiConfiguration,
        manifests: &crate::provider_manifest::ResolvedProviderCatalog,
    ) -> Result<RuntimeConnectionCatalogs, String> {
        // source_catalogs captures the staged discovery inputs while the activation lock is held.
        // source_catalogs 在持有激活锁期间捕获暂存发现输入。
        let source_catalogs = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .clone();
        // usage_cache captures plan observations serialized by the same catalog mutation lock.
        // usage_cache 捕获由同一目录变更锁串行化的套餐观测。
        let usage_cache = self.usage_cache_snapshot_locked()?;
        resolve_model_catalog_overlays_from_inputs(
            configuration,
            manifests,
            &source_catalogs,
            &usage_cache,
        )
    }

    /// Determines whether one exact connection still exposes a model after applying candidate overlays.
    /// 判断一个精确连接在应用候选覆盖后是否仍暴露指定模型。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the complete candidate configuration, `connection_id` selects the exact
    /// credential catalog, and `model_id` is the upstream model identifier.
    /// `configuration` 是完整候选配置，`connection_id` 选择精确凭据目录，`model_id`
    /// 是上游模型标识。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the immutable discovery snapshot or manual fallback still contains the model
    /// after all defaults and user differences resolve; otherwise `false`.
    /// 当不可变发现快照或手工回退目录在解析全部默认值与用户差异后仍包含该模型时返回
    /// `true`，否则返回 `false`。
    pub fn connection_catalog_contains_model(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
    ) -> Result<bool, String> {
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .ok_or_else(|| format!("Credential connection `{connection_id}` is not configured."))?;
        // Candidate validation deliberately sees staged discovery evidence, but captures source,
        // plan, and published Manifest rules under one mutation lock so activation cannot expose
        // an internally mixed image.
        // 候选校验刻意读取暂存发现证据，但会在同一变更锁下捕获来源、套餐与已发布 Manifest
        // 规则，因此激活不会暴露内部混合镜像。
        let (source_catalog, account_usage, generation) = {
            let _mutation_guard = self
                .catalog_mutation_lock
                .lock()
                .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
            let source_catalog = self
                .source_catalogs
                .read()
                .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
                .get(connection_id)
                .cloned();
            let account_usage = self
                .usage_cache
                .read()
                .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
                .get(connection.id.as_str())
                .cloned();
            (
                source_catalog,
                account_usage,
                self.selection_catalog_snapshot()?,
            )
        };
        let Some(mut catalog) =
            source_catalog.or_else(|| manual_custom_catalog(configuration, connection, 1))
        else {
            return Ok(false);
        };
        apply_model_catalog_overlays_from_generation(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            account_usage.as_ref(),
            generation.as_ref(),
            &mut catalog,
        )?;
        validate_resolved_catalog_limits(&catalog)?;
        Ok(catalog.models.iter().any(|model| model.id == model_id))
    }

    /// Validates one proposed provider-model capability difference against every visible account.
    /// 针对每个可见账号校验一项候选供应商模型能力差异。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the persisted baseline, `provider_id` and `model_id` identify the exact
    /// model, and `overrides` is the replacement difference or `None` when clearing it.
    /// `configuration` 是持久化基线，`provider_id` 与 `model_id` 标识精确模型，`overrides`
    /// 是替换差异，清除差异时为 `None`。
    ///
    /// # Returns
    /// 返回值
    /// Success only when the fully resolved limits remain valid for every account that exposes
    /// the model from an immutable discovery snapshot or an explicit manual declaration.
    /// 仅当不可变发现快照或显式手工声明中暴露该模型的每个账号，其最终解析限制均有效时返回成功。
    pub fn validate_model_capability_override_candidate(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
        overrides: Option<&crate::ModelCapabilityOverrides>,
    ) -> Result<(), String> {
        if let Some(overrides) = overrides {
            overrides.validate()?;
        }
        let mut candidate_configuration = configuration.clone();
        // Provider-level capability settings must remain editable when every account hides the model.
        // 即使全部账号都隐藏该模型，供应商级能力设置仍必须可编辑。
        candidate_configuration
            .connection_model_access_policies
            .clear();
        let provider_model_key =
            PersistedAiConfiguration::provider_model_key(provider_id, model_id);
        if let Some(overrides) = overrides {
            candidate_configuration
                .model_capability_overrides
                .insert(provider_model_key, overrides.clone());
        } else {
            candidate_configuration
                .model_capability_overrides
                .remove(provider_model_key.as_str());
        }
        candidate_configuration.validate_context_window_policy_layers()?;

        // Candidate validation consumes staged discovery and usage as one immutable local image.
        // 候选校验把暂存发现与用量作为一份不可变本地镜像使用。
        let (source_catalogs, usage_cache, generation) = {
            let _mutation_guard = self
                .catalog_mutation_lock
                .lock()
                .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
            let source_catalogs = self
                .source_catalogs
                .read()
                .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
                .snapshot();
            let usage_cache = self
                .usage_cache
                .read()
                .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
                .clone();
            (
                source_catalogs,
                usage_cache,
                self.selection_catalog_snapshot()?,
            )
        };
        let mut matched_connection = false;
        for connection in candidate_configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
        {
            let Some(mut catalog) = source_catalogs
                .get(connection.id.as_str())
                .cloned()
                .or_else(|| manual_custom_catalog(&candidate_configuration, connection, 1))
            else {
                continue;
            };
            let account_usage = usage_cache.get(connection.id.as_str());
            apply_model_catalog_overlays_from_generation(
                &candidate_configuration,
                provider_id,
                connection.service_entry_id.as_str(),
                account_usage,
                generation.as_ref(),
                &mut catalog,
            )?;
            let Some(model) = catalog.models.iter().find(|model| model.id == model_id) else {
                continue;
            };
            matched_connection = true;
            validate_resolved_model_capability_limits(
                connection.id.as_str(),
                model.id.as_str(),
                &model.capabilities,
            )?;
            if let Some(overrides) = overrides
                && let Some(projection) = overrides.request_projection.as_ref()
            {
                projection.validate_for_capabilities(&model.capabilities)?;
            }
        }
        if !matched_connection
            && let Some(mut catalog) = manual_custom_catalog_for_provider(
                &candidate_configuration,
                provider_id,
                "provider-settings-validation",
                1,
            )
        {
            apply_model_catalog_overlays_from_generation(
                &candidate_configuration,
                provider_id,
                "",
                None,
                generation.as_ref(),
                &mut catalog,
            )?;
            if let Some(model) = catalog.models.iter().find(|model| model.id == model_id) {
                matched_connection = true;
                validate_resolved_model_capability_limits(
                    "provider-settings-validation",
                    model.id.as_str(),
                    &model.capabilities,
                )?;
                if let Some(overrides) = overrides
                    && let Some(projection) = overrides.request_projection.as_ref()
                {
                    projection.validate_for_capabilities(&model.capabilities)?;
                }
            }
        }
        if !matched_connection {
            return Err(format!(
                "Provider `{provider_id}` has no immutable or manual catalog source for model `{model_id}`. Synchronize the model catalog before saving capability overrides."
            ));
        }
        Ok(())
    }

    /// Validates one provider-level capability layer against every synchronized provider model.
    /// 针对供应商全部已同步模型校验一层供应商级能力配置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the current persisted state, `provider_id` identifies the provider,
    /// and `overrides` is the candidate sparse layer or `None` for restoration.
    /// `configuration` 是当前持久化状态，`provider_id` 标识供应商，`overrides` 是候选稀疏层，
    /// 为 `None` 时表示恢复继承。
    ///
    /// # Returns
    /// 返回值
    /// Success only when every provider model resolves valid limits and request projections.
    /// 仅当供应商每个模型的限制与请求投影均解析有效时返回成功。
    pub fn validate_provider_capability_override_candidate(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        overrides: Option<&crate::ModelCapabilityOverrides>,
    ) -> Result<(), String> {
        if let Some(overrides) = overrides {
            overrides.validate()?;
        }
        let mut candidate_configuration = configuration.clone();
        candidate_configuration
            .connection_model_access_policies
            .clear();
        if let Some(overrides) = overrides {
            candidate_configuration
                .provider_capability_overrides
                .insert(provider_id.to_string(), overrides.clone());
        } else {
            candidate_configuration
                .provider_capability_overrides
                .remove(provider_id);
        }
        candidate_configuration.validate_context_window_policy_layers()?;
        let catalogs = self.resolved_model_catalog_snapshot(&candidate_configuration)?;
        let matched = candidate_configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .any(|connection| {
                catalogs
                    .get(connection.id.as_str())
                    .is_some_and(|catalog| !catalog.models.is_empty())
            });
        if !matched {
            return Err(format!(
                "Provider `{provider_id}` has no synchronized model catalog for provider-level capability settings."
            ));
        }
        Ok(())
    }

    /// Validates one protocol-specific model setting against every account support route.
    /// 针对每条账号支持路由校验一项协议专属模型设置。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns immutable membership; provider, model, and protocol identify the
    /// shared route; `overrides` contains the proposed exact adapter controls.
    /// `configuration` 持有不可变成员关系；供应商、模型与协议标识共享路由；`overrides`
    /// 包含候选精确适配器控制项。
    ///
    /// # Returns
    /// 返回值
    /// Success only when at least one support route exists and every matching model accepts the settings.
    /// 仅当至少存在一条支持路由且每个匹配模型都接受设置时返回成功。
    pub fn validate_model_execution_override_candidate(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
        protocol_id: &str,
        overrides: &crate::ModelExecutionOverrides,
    ) -> Result<(), String> {
        // support_configuration removes account visibility without weakening provider membership.
        // support_configuration 移除账号可见性，但不会削弱供应商成员关系。
        let mut support_configuration = configuration.clone();
        support_configuration
            .connection_model_access_policies
            .clear();
        let catalogs = self.resolved_model_catalog_snapshot(&support_configuration)?;
        let mut matched_connection = false;
        for connection in support_configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
        {
            let Some(model) = catalogs
                .get(connection.id.as_str())
                .and_then(|catalog| catalog.models.iter().find(|model| model.id == model_id))
            else {
                continue;
            };
            // effectiveProtocolId mirrors request selection and accepts only frozen model bindings.
            // effectiveProtocolId 镜像请求选择，并且只接受冻结的模型绑定。
            let effective_protocol_id = model
                .resolved_protocol_binding
                .as_ref()
                .map(|binding| binding.protocol_profile_id.as_str());
            if effective_protocol_id != Some(protocol_id) {
                continue;
            }
            matched_connection = true;
            validate_model_overrides(model, protocol_id, overrides)?;
        }
        if !matched_connection {
            return Err(format!(
                "Provider `{provider_id}` model `{model_id}` has no synchronized account with effective protocol `{protocol_id}`."
            ));
        }
        Ok(())
    }

    /// Installs validated manual models when provider discovery has no usable source catalog.
    /// 当供应商发现没有可用源目录时安装已校验的手工模型。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns custom definitions, `connection` selects one credential, and
    /// `generation` preserves the current refresh generation.
    /// `configuration` 持有自定义定义，`connection` 选择一个凭据，`generation` 保留当前刷新代次。
    ///
    /// # Returns
    /// 返回值
    /// Success when no fallback is needed or the manual-only effective catalog is installed.
    /// 无需回退或仅含手工模型的有效目录安装完成时返回成功。
    fn install_manual_custom_catalog_without_discovery(
        &self,
        configuration: &PersistedAiConfiguration,
        connection: &CredentialConnection,
        generation: u64,
    ) -> Result<(), String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let has_source_catalog = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .get(connection.id.as_str())
            .is_some();
        if has_source_catalog {
            return Ok(());
        }
        let Some(mut catalog) = manual_custom_catalog(configuration, connection, generation) else {
            return Ok(());
        };
        let account_usage = self.cached_account_usage(connection.id.as_str())?;
        self.apply_model_catalog_overlays_with_account_access(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            account_usage.as_ref(),
            &mut catalog,
        )?;
        self.replace_catalog_locked(catalog)?;
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Queries normalized account usage for one exact credential connection.
    /// 查询一个精确凭据连接的归一化账号用量。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the persisted public AI configuration and `connection_id` is exact.
    /// `configuration` 是持久化公共 AI 配置，`connection_id` 为精确连接标识。
    ///
    /// # Returns
    /// 返回值
    /// A provider-normalized usage snapshot.
    /// 返回供应商归一化后的用量快照。
    pub async fn query_connection_usage(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
    ) -> Result<AccountUsageSnapshot, String> {
        let _refresh_guard = self.catalog_refresh_flight.lock().await;
        let (_provider, entry, connection) =
            self.resolve_connection_metadata(configuration, connection_id)?;
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                )
            })?;
        let secret_reference = account_usage_secret_reference(&connection, credential_profile)?;
        let secret = self
            .resolve_fresh_execution_secret(connection.id.as_str(), secret_reference)
            .await?;
        let snapshot = self
            .metadata_client
            .query_account_usage(&entry, &connection, &secret)
            .await;
        let account_model_access_enabled = credential_profile.account_model_access.is_some();
        let snapshot =
            self.cache_usage_snapshot(connection_id, snapshot, account_model_access_enabled)?;
        if account_model_access_enabled {
            self.rebuild_catalog_after_usage(configuration, connection_id)?;
        }
        Ok(snapshot)
    }

    /// Caches one usage observation while retaining confirmed access evidence through transient failures.
    /// 缓存一次用量观测，并在暂时失败时保留已确认的访问证据。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the exact credential, `snapshot` is the provider observation,
    /// and `preserve_access_evidence` protects the last confirmed plan and quota evidence.
    /// `connection_id` 标识精确凭据，`snapshot` 是供应商观测，`preserve_access_evidence`
    /// 保护最后一次确认的套餐与额度证据。
    ///
    /// # Returns
    /// 返回值
    /// The snapshot stored in the process-local cache.
    /// 已写入进程内缓存的观测。
    fn cache_usage_snapshot(
        &self,
        connection_id: &str,
        mut snapshot: AccountUsageSnapshot,
        preserve_access_evidence: bool,
    ) -> Result<AccountUsageSnapshot, String> {
        // mutation_guard serializes plan facts with every immutable catalog generation publication.
        // mutation_guard 把套餐事实与每次不可变目录代际发布串行化。
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let mut cache = self
            .usage_cache
            .write()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?;
        if preserve_access_evidence
            && !matches!(
                snapshot.availability,
                vulcan_ai_contract::AccountUsageAvailability::Supported
            )
            && let Some(previous) = cache.get(connection_id)
        {
            if snapshot.plan_name.is_none() {
                snapshot.plan_name = previous.plan_name.clone();
            }
            if snapshot.plan_id.is_none() {
                snapshot.plan_id = previous.plan_id.clone();
            }
            if snapshot.quota_windows.is_empty() {
                snapshot.quota_windows = previous.quota_windows.clone();
            }
        }
        cache.insert(connection_id.to_string(), snapshot.clone());
        drop(cache);
        self.advance_catalog_state_revision_locked()?;
        Ok(snapshot)
    }

    /// Reads the normalized account-usage evidence associated with one exact credential.
    /// 读取一个精确凭据关联的归一化账号用量证据。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the process-local usage observation.
    /// `connection_id` 标识进程内用量观测。
    ///
    /// # Returns
    /// 返回值
    /// The frozen usage snapshot when the cache contains one.
    /// 当缓存包含观测时返回冻结用量快照。
    fn cached_account_usage(
        &self,
        connection_id: &str,
    ) -> Result<Option<AccountUsageSnapshot>, String> {
        self.usage_cache
            .read()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))
            .map(|cache| cache.get(connection_id).cloned())
    }

    /// Returns the complete process-local account-usage cache.
    /// 返回完整的进程内账号用量缓存。
    pub fn usage_cache_snapshot(&self) -> Result<BTreeMap<String, AccountUsageSnapshot>, String> {
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.usage_cache_snapshot_locked()
    }

    /// Clones the account-usage cache while the caller holds the catalog mutation lock.
    /// 在调用方持有目录变更锁时克隆账号用量缓存。
    ///
    /// # Returns
    /// 返回值
    /// The exact process-local usage snapshot paired with the caller's other catalog reads.
    /// 返回与调用方其他目录读取配对的精确进程内用量快照。
    fn usage_cache_snapshot_locked(
        &self,
    ) -> Result<BTreeMap<String, AccountUsageSnapshot>, String> {
        self.usage_cache
            .read()
            .map(|cache| cache.clone())
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))
    }

    /// Removes one stale process-local account-usage observation.
    /// 移除一个过期的进程内账号用量观测。
    pub fn remove_connection_usage(&self, connection_id: &str) -> Result<(), String> {
        let _refresh_guard = self.catalog_refresh_flight.try_lock().map_err(|_| {
            String::from(
                "Provider manifest catalog activation is in progress; retry usage removal after activation completes.",
            )
        })?;
        // mutation_guard prevents usage removal from racing a catalog-generation snapshot.
        // mutation_guard 防止用量移除与目录代际快照竞争。
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        self.usage_cache
            .write()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
            .remove(connection_id);
        self.advance_catalog_state_revision_locked()?;
        Ok(())
    }

    /// Refreshes every configured connection that declares a supported usage query.
    /// 刷新每个声明支持用量查询的已配置连接。
    pub async fn refresh_supported_connection_usage(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<(), String> {
        for connection in configuration.connection_graph.connections.values() {
            if configuration.custom_provider_manifest_is_isolated(connection.provider_id.as_str()) {
                continue;
            }
            let (_, entry, _) =
                self.resolve_connection_metadata(configuration, connection.id.as_str())?;
            let profile = entry
                .credential_profile(connection.credential_profile_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Credential profile `{}` is not registered for service entry `{}`.",
                        connection.credential_profile_id, entry.id
                    )
                })?;
            let should_refresh = match profile.account_usage_query {
                crate::AccountUsageQueryKind::Unsupported => false,
                crate::AccountUsageQueryKind::MiniMaxTokenPlan => true,
                crate::AccountUsageQueryKind::TavilyCredits => true,
                crate::AccountUsageQueryKind::DeepSeekBalance => true,
                crate::AccountUsageQueryKind::KimiCodingPlan => true,
                crate::AccountUsageQueryKind::CommandCodeUsage => true,
                crate::AccountUsageQueryKind::OpenRouterCredits => {
                    connection.management_secret_reference.is_some()
                }
            };
            if should_refresh {
                let _ = self
                    .query_connection_usage(configuration, connection.id.as_str())
                    .await;
            }
        }
        Ok(())
    }

    /// Builds the content-addressed secondary-route cache key for one published route identity.
    /// 为一个已发布路由身份构建内容寻址的二级路由缓存键。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `connection_id`, and `model_id` select the exact account/model; every
    /// non-secret provider, service, connection, and Manifest input is frozen into the digest.
    /// `configuration`、`connection_id` 与 `model_id` 选择精确账号/模型；每项非秘密供应商、
    /// 服务、连接与 Manifest 输入都会被冻结进摘要。
    ///
    /// # Returns
    /// 返回值
    /// A stable SHA-256 cache key, resolved metadata, and the immutable generation used for lookup.
    /// 返回稳定 SHA-256 缓存键、已解析元数据以及查找使用的不可变代际。
    fn secondary_route_cache_identity(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
    ) -> Result<
        (
            String,
            crate::ProviderDescriptor,
            crate::ServiceEntry,
            CredentialConnection,
            Arc<ResolvedProviderGeneration>,
        ),
        String,
    > {
        let generation = self.selection_catalog_snapshot()?;
        let (provider, entry, connection) = self.resolve_connection_metadata_from_generation(
            configuration,
            connection_id,
            &generation,
        )?;
        // identityBytes contain no credential value; secret_reference is an opaque host-owned key.
        // identityBytes 不包含凭据值；secret_reference 是宿主持有的不透明键。
        let identity_bytes = serde_json::to_vec(&(
            generation.manifests.content_sha256.as_str(),
            &provider,
            &entry,
            &connection,
            model_id,
        ))
        .map_err(|error| format!("Failed to serialize secondary-route cache identity: {error}"))?;
        let cache_key = hex::encode(Sha256::digest(identity_bytes));
        Ok((cache_key, provider, entry, connection, generation))
    }

    /// Inserts one secondary-route observation with global expiry cleanup and bounded eviction.
    /// 通过全局过期清理与有界淘汰插入一项二级路由观测。
    ///
    /// # Parameters
    /// 参数
    /// `cache_key` is the content-addressed route identity; `connection_id` owns the partition;
    /// `routes` is the exact provider response; and `fetched_at` is its monotonic observation time.
    /// `cache_key` 是内容寻址路由身份；`connection_id` 持有该分区；`routes` 是精确供应商响应；
    /// `fetched_at` 是其单调观测时间。
    ///
    /// # Returns
    /// 返回值
    /// Success after all expired entries are removed and the cache remains within its hard limit.
    /// 移除全部过期项且缓存保持在硬上限内后返回成功。
    fn insert_secondary_route_cache_entry(
        &self,
        cache_key: String,
        connection_id: &str,
        routes: Vec<SecondaryRouteOption>,
        fetched_at: Instant,
    ) -> Result<(), String> {
        let mut cache = self
            .secondary_route_cache
            .write()
            .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?;
        // Expired entries are removed globally on every successful insertion. If the remaining
        // cache is full, stable fetched-at/key ordering evicts exactly one oldest partition.
        // 每次成功插入都会全局移除过期项；若剩余缓存已满，则按 fetched-at/key 稳定顺序
        // 精确淘汰一个最旧分区。
        cache.retain(|_, cached| cached.fetched_at.elapsed() < SECONDARY_ROUTE_CACHE_TTL);
        if !cache.contains_key(cache_key.as_str())
            && cache.len() >= SECONDARY_ROUTE_CACHE_MAX_ENTRIES
            && let Some(oldest_key) = cache
                .iter()
                .min_by(|(left_key, left), (right_key, right)| {
                    left.fetched_at
                        .cmp(&right.fetched_at)
                        .then_with(|| left_key.cmp(right_key))
                })
                .map(|(key, _)| key.clone())
        {
            cache.remove(oldest_key.as_str());
        }
        cache.insert(
            cache_key,
            SecondaryRouteCacheEntry {
                connection_id: connection_id.to_string(),
                routes,
                fetched_at,
            },
        );
        Ok(())
    }

    /// Discovers secondary routes for one exact connection and model.
    /// 发现一个精确连接与模型可用的二级路由。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the authoritative connection graph, `connection_id` selects one
    /// credential, `model_id` identifies the exact provider model, and `force_refresh` bypasses
    /// the six-hour process cache.
    /// `configuration` 是权威连接图，`connection_id` 选择一个凭据，
    /// `model_id` 标识精确供应商模型，`force_refresh` 绕过六小时进程缓存。
    ///
    /// # Returns
    /// 返回值
    /// Provider-reported route options without persisting metadata locally.
    /// 返回供应商报告的路由选项，不在本地持久化元数据。
    pub async fn discover_secondary_routes(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
        force_refresh: bool,
    ) -> Result<Vec<crate::SecondaryRouteOption>, String> {
        let (cache_key, provider, entry, connection, generation) =
            self.secondary_route_cache_identity(configuration, connection_id, model_id)?;
        let catalog = effective_catalog_snapshot_from_generation(&generation)
            .get(connection_id)
            .cloned()
            .ok_or_else(|| {
                format!("Connection `{connection_id}` has no published model catalog.")
            })?;
        if !catalog.models.iter().any(|model| model.id == model_id) {
            return Err(format!(
                "Model `{model_id}` is not present in connection `{connection_id}`."
            ));
        }
        if !force_refresh {
            let cached_routes = self
                .secondary_route_cache
                .read()
                .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?
                .get(cache_key.as_str())
                .filter(|cached| cached.fetched_at.elapsed() < SECONDARY_ROUTE_CACHE_TTL)
                .map(|cached| cached.routes.clone());
            if let Some(cached_routes) = cached_routes {
                return Ok(cached_routes);
            }
        }
        let secret = self
            .resolve_fresh_execution_secret(
                connection.id.as_str(),
                connection.secret_reference.as_str(),
            )
            .await?;
        let routes = self
            .metadata_client
            .discover_secondary_routes(&provider, &entry, &connection, model_id, &secret)
            .await?;
        self.insert_secondary_route_cache_entry(
            cache_key,
            connection_id,
            routes.clone(),
            Instant::now(),
        )?;
        Ok(routes)
    }

    /// Returns the last successful secondary-route discovery without any provider request.
    /// 在不发起任何供应商请求的情况下返回最近一次成功的二级路由发现结果。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` and `model_id` identify the exact cache partition.
    /// `connection_id` 与 `model_id` 标识精确缓存分区。
    ///
    /// # Returns
    /// 返回值
    /// Fresh route options for the exact current route identity, or an explicit cache-miss error.
    /// 返回精确当前路由身份的新鲜路由选项，或显式的缓存未命中错误。
    pub fn cached_secondary_routes(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        model_id: &str,
    ) -> Result<Vec<SecondaryRouteOption>, String> {
        // cache_key includes the exact current provider generation and non-secret route inputs.
        // cache_key 包含精确当前供应商代际与非秘密路由输入。
        let (cache_key, _, _, _, _) =
            self.secondary_route_cache_identity(configuration, connection_id, model_id)?;
        self.secondary_route_cache
            .read()
            .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?
            .get(cache_key.as_str())
            .filter(|cached| cached.fetched_at.elapsed() < SECONDARY_ROUTE_CACHE_TTL)
            .map(|cached| cached.routes.clone())
            .ok_or_else(|| {
                format!(
                    "No cached secondary routes exist for connection `{connection_id}` and model `{model_id}`. Open or refresh the secondary provider list first."
                )
            })
    }

    /// Returns every provider tool target usable by currently configured credentials.
    /// 返回当前已配置凭据能够使用的全部供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the authoritative connection graph.
    /// `configuration` 是权威连接图。
    ///
    /// # Returns
    /// 返回值
    /// Exact targets paired with eligible credential connection identifiers.
    /// 返回与合格凭据连接标识配对的精确目标。
    pub fn available_provider_tool_targets(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<Vec<AvailableProviderToolTarget>, String> {
        // generation freezes manifest targets and template-model access for the complete listing.
        // generation 为完整列表冻结 Manifest 目标与模板模型访问状态。
        let generation = self.selection_catalog_snapshot()?;
        let runtime_configuration = runtime_configuration_for_manifest_catalog(
            configuration,
            generation.manifests.as_ref(),
        );
        let configuration = &runtime_configuration;
        // system projects the exact provider registry owned by the same generation.
        // system 投影由同一代际持有的精确供应商注册表。
        let system = ProviderRegistry::from_catalog(generation.manifests.as_ref())?;
        // registry adds only the caller's explicit custom-provider compatibility projection.
        // registry 仅添加调用方的显式自定义供应商兼容投影。
        let registry =
            ProviderRegistry::effective_from_system(system, &configuration.custom_providers)?;
        Self::available_provider_tool_targets_in_generation(
            configuration,
            generation.as_ref(),
            &registry,
        )
    }

    /// Resolves the exact non-secret media input profile for one selected provider tool.
    /// 解析一个已选供应商工具的精确非秘密媒体输入 Profile。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, and `selection` identify the same fixed route used by tool
    /// execution.
    /// `configuration`、`capability` 与 `selection` 标识工具执行使用的同一条固定路线。
    ///
    /// # Returns
    /// 返回值
    /// `None` for non-understanding capabilities, or the operation binding and channel
    /// intersections that the AI-facing tool description may safely disclose.
    /// 非理解能力返回 `None`；否则返回 AI 可见工具说明可安全披露的操作绑定与通道交集。
    pub fn provider_tool_media_input_profile(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        selection: &ProviderToolTargetSelection,
    ) -> Result<Option<ProviderToolMediaInputProfile>, String> {
        let Some((resource_kind, _media_label)) = provider_tool_media_resource_kind(capability)
        else {
            return Ok(None);
        };
        let resolved = self.resolve_provider_tool_target(configuration, capability, selection)?;
        let operation = resolved.operation.as_ref().ok_or_else(|| {
            format!(
                "Media-understanding target `{}` has no owning operation.",
                resolved.target.target_id
            )
        })?;
        let projection = resolved
            .target
            .contract
            .media_input_projection
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "Media-understanding target `{}` has no input projection.",
                    resolved.target.target_id
                )
            })?;
        let source_pointer = format!("/{}", projection.path_field);
        let mut matching_bindings = operation
            .upload_bindings
            .iter()
            .filter(|binding| binding.source_path == source_pointer);
        let binding = matching_bindings.next().ok_or_else(|| {
            format!(
                "Media-understanding operation `{}` has no upload binding for `{source_pointer}`.",
                operation.operation_id
            )
        })?;
        if matching_bindings.next().is_some() {
            return Err(format!(
                "Media-understanding operation `{}` has ambiguous upload bindings for `{source_pointer}`.",
                operation.operation_id
            ));
        }
        // bindingMimeTypes is the exact closed file-extension projection declared by the operation.
        // bindingMimeTypes 是操作声明的精确封闭文件扩展名投影。
        let mut binding_mime_types = BTreeSet::new();
        if let Some(mime_type) = binding.mime_type.as_ref() {
            binding_mime_types.insert(mime_type.clone());
        }
        binding_mime_types.extend(binding.mime_types_by_extension.values().cloned());
        let mut routes = Vec::with_capacity(binding.upload_channel_refs.len());
        for channel_id in &binding.upload_channel_refs {
            let channel = resolved
                .upload_channels
                .iter()
                .find(|channel| channel.upload_channel_id == *channel_id)
                .ok_or_else(|| {
                    format!(
                        "Media-understanding operation `{}` references missing upload channel `{channel_id}`.",
                        operation.operation_id
                    )
                })?;
            let channel_mime_types = channel
                .accepted_mime_types
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            // acceptedMimeTypes retains empty-as-unrestricted semantics without inventing a format.
            // acceptedMimeTypes 保留空集合表示未进一步收窄的语义，且不虚构格式。
            let accepted_mime_types = if binding_mime_types.is_empty() {
                channel_mime_types
            } else if channel_mime_types.is_empty() {
                binding_mime_types.clone()
            } else {
                binding_mime_types
                    .intersection(&channel_mime_types)
                    .cloned()
                    .collect()
            };
            routes.push(ProviderToolMediaInputRouteConstraint {
                upload_channel_id: channel.upload_channel_id.clone(),
                accepted_mime_types: accepted_mime_types.into_iter().collect(),
                max_bytes: channel.max_bytes,
                max_count: channel.max_count,
            });
        }
        if routes.is_empty() {
            return Err(format!(
                "Media-understanding operation `{}` has no concrete upload route.",
                operation.operation_id
            ));
        }
        Ok(Some(ProviderToolMediaInputProfile {
            resource_kind,
            routes,
        }))
    }

    /// Lists eligible provider-tool targets from one already captured provider generation.
    /// 从一份已捕获供应商代际列出合格供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies credentials and model access; `generation` freezes tool/template
    /// authority; `registry` is the provider projection built from that same generation.
    /// `configuration` 提供凭据与模型访问；`generation` 冻结工具/模板权限；`registry` 是从
    /// 同一代际构建的供应商投影。
    ///
    /// # Returns
    /// 返回值
    /// Exact AI-visible targets paired with eligible credential connections in deterministic order.
    /// 返回按确定性顺序与合格凭据连接配对的精确 AI 可见目标。
    fn available_provider_tool_targets_in_generation(
        configuration: &PersistedAiConfiguration,
        generation: &ResolvedProviderGeneration,
        registry: &ProviderRegistry,
    ) -> Result<Vec<AvailableProviderToolTarget>, String> {
        // available accumulates only targets satisfying service, credential, and model access.
        // available 仅累积满足服务、凭据与模型访问条件的目标。
        let mut available = Vec::new();
        for connection in configuration.connection_graph.connections.values() {
            if configuration.connection_is_pending_validation(connection.id.as_str()) {
                continue;
            }
            if configuration.custom_provider_manifest_is_isolated(connection.provider_id.as_str()) {
                continue;
            }
            let provider = registry
                .provider(connection.provider_id.as_str())
                .ok_or_else(|| {
                    format!("Provider `{}` is not registered.", connection.provider_id)
                })?;
            let entry = provider
                .service_entries
                .iter()
                .find(|entry| entry.id == connection.service_entry_id)
                .ok_or_else(|| {
                    format!(
                        "Service entry `{}` is not registered for provider `{}`.",
                        connection.service_entry_id, provider.id
                    )
                })?;
            let targets = Self::resolved_provider_tool_targets_in_generation(
                generation,
                provider.id.as_str(),
            );
            if targets.is_empty() {
                continue;
            }
            // providerManifest supplies immutable provenance for every projected target.
            // providerManifest 为每个投影目标提供不可变溯源信息。
            let provider_manifest = generation
                .manifests
                .manifest(provider.id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Provider tool manifest for `{}` disappeared from the frozen generation.",
                        provider.id
                    )
                })?;
            for target in targets {
                if target.contract.capability.is_ai_visible()
                    && target.service_entry_id == entry.id
                    && provider_tool_target_allows_credential(
                        &target,
                        connection.credential_profile_id.as_str(),
                    )
                    // Provider-tool availability follows the credential's account-filtered catalog,
                    // while extension target selection independently controls whether AI may call
                    // the model. Conversation publication must not hide media targets.
                    // Provider Tool 可用性遵循凭据经账号权限过滤后的目录，扩展目标选择则独立控制 AI
                    // 是否允许调用该模型；会话发布开关不得隐藏媒体目标。
                    && provider_tool_target_availability_catalog_matches(
                        &target,
                        generation
                            .activated_effective_catalogs
                            .get(connection.id.as_str()),
                    )
                {
                    available.push(AvailableProviderToolTarget {
                        connection_id: connection.id.clone(),
                        connection_display_name: connection.display_name.clone(),
                        manifest_source_layer: provider_manifest.snapshot_provenance.source_layer,
                        target,
                    });
                }
            }
        }
        available.sort_by(|left, right| {
            left.target
                .contract
                .capability
                .cmp(&right.target.contract.capability)
                .then_with(|| left.target.display_name.cmp(&right.target.display_name))
                .then_with(|| left.connection_id.cmp(&right.connection_id))
        });
        Ok(available)
    }

    /// Resolves every provider tool target for one provider from the manifest-owned catalog.
    /// 从清单持有的目录解析一个供应商的全部工具目标。
    ///
    /// Manifest-owned targets are the single authority since R11 migrated every system
    /// provider target (tavily 2 + alibaba 51 + minimax 34) into the manifests. Every target
    /// is already provider-tagged by the compiler, so the frozen manifest target list is used
    /// verbatim instead of re-selecting through another catalog.
    /// 自 R11 把全部系统供应商目标（tavily 2 + alibaba 51 + minimax 34）迁入清单后，清单持有
    /// 目标就是唯一权威。编译后每个目标都已带上供应商标识，因此直接使用冻结的清单目标列表，
    /// 不再通过另一份目录重新选择。
    fn resolved_provider_tool_targets(&self, provider_id: &str) -> Vec<ProviderToolTarget> {
        self.selection_catalog_snapshot()
            .ok()
            .and_then(|generation| generation.manifests.manifest_arc(provider_id))
            .map(|manifest| manifest.tool_targets.clone())
            .unwrap_or_default()
    }

    /// Resolves provider-tool targets from one already frozen provider generation.
    /// 从一份已冻结供应商代际解析供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `generation` is the request-owned immutable snapshot and `provider_id` is exact.
    /// `generation` 是请求持有的不可变快照，`provider_id` 为精确标识。
    ///
    /// # Returns
    /// 返回值
    /// The manifest-owned targets in their declaration order, or an empty list when absent.
    /// 返回按声明顺序排列的 Manifest 目标；不存在时返回空列表。
    fn resolved_provider_tool_targets_in_generation(
        generation: &ResolvedProviderGeneration,
        provider_id: &str,
    ) -> Vec<ProviderToolTarget> {
        generation
            .manifests
            .manifest_arc(provider_id)
            .map(|manifest| manifest.tool_targets.clone())
            .unwrap_or_default()
    }

    /// Executes the one exact target selected for a semantic capability.
    /// 执行某项语义能力唯一选中的精确目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, and `arguments` are frozen invocation inputs.
    /// `configuration`、`capability` 与 `arguments` 是冻结的调用输入。
    ///
    /// # Returns
    /// 返回值
    /// A unified typed provider tool result.
    /// 返回统一强类型供应商工具结果。
    pub async fn execute_selected_provider_tool(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        arguments: serde_json::Value,
    ) -> Result<ProviderToolExecutionResult, String> {
        self.execute_selected_provider_tool_for_owner(configuration, capability, arguments, None)
            .await
    }

    /// Executes one selected provider tool while retaining its optional session ownership.
    /// 执行一个已选供应商工具，同时保留其可选会话所有权。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, and `arguments` are frozen invocation inputs;
    /// `owner_session_id` is retained only in non-secret asynchronous task recovery metadata.
    /// `configuration`、`capability` 与 `arguments` 是冻结调用输入；
    /// `owner_session_id` 仅保留在非秘密异步任务恢复元数据中。
    ///
    /// # Returns
    /// 返回值
    /// One normalized terminal result, including recovered asynchronous completion.
    /// 返回一个归一化终态结果，包括恢复后的异步完成结果。
    pub async fn execute_selected_provider_tool_for_owner(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        arguments: serde_json::Value,
        owner_session_id: Option<String>,
    ) -> Result<ProviderToolExecutionResult, String> {
        self.execute_selected_provider_tool_for_owner_in_workspace(
            configuration,
            capability,
            arguments,
            owner_session_id,
            None,
        )
        .await
    }

    /// Executes one selected provider tool with an explicit host workspace boundary.
    /// 使用显式宿主工作区边界执行一个已选供应商工具。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, and `arguments` freeze the target invocation;
    /// `owner_session_id` owns recoverable work and `workspace_root` bounds local resources.
    /// `configuration`、`capability` 与 `arguments` 冻结目标调用；`owner_session_id` 持有
    /// 可恢复任务，`workspace_root` 约束本地资源。
    ///
    /// # Returns
    /// 返回值
    /// One normalized terminal result after workspace-bounded upload materialization.
    /// 在工作区有界上传物化后返回一个归一化终态结果。
    pub async fn execute_selected_provider_tool_for_owner_in_workspace(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        arguments: serde_json::Value,
        owner_session_id: Option<String>,
        workspace_root: Option<&Path>,
    ) -> Result<ProviderToolExecutionResult, String> {
        let selection = configuration
            .provider_tool_selections
            .get(&capability)
            .cloned()
            .ok_or_else(|| {
                format!("No provider tool target is selected for capability `{capability:?}`.")
            })?;
        self.execute_provider_tool_target_for_owner_in_workspace(
            configuration,
            capability,
            &selection,
            arguments,
            owner_session_id,
            workspace_root,
        )
        .await
    }

    /// Executes one exact provider-tool target without changing the persisted default selection.
    /// 在不修改持久化默认选择的情况下执行一个精确供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns the current connection graph, `capability` and `selection` freeze the
    /// exact target, `arguments` match that target's schema, and `owner_session_id` optionally binds
    /// a recoverable asynchronous task to one session.
    /// `configuration` 持有当前连接图，`capability` 与 `selection` 固定精确目标，
    /// `arguments` 匹配该目标的 Schema，`owner_session_id` 可选地把可恢复异步任务绑定到会话。
    ///
    /// # Returns
    /// 返回值
    /// The normalized provider-tool result or an explicit resolution, credential, or execution error.
    /// 返回规范化供应商工具结果，或明确的解析、凭据与执行错误。
    pub async fn execute_provider_tool_target_for_owner(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        selection: &ProviderToolTargetSelection,
        arguments: serde_json::Value,
        owner_session_id: Option<String>,
    ) -> Result<ProviderToolExecutionResult, String> {
        self.execute_provider_tool_target_for_owner_in_workspace(
            configuration,
            capability,
            selection,
            arguments,
            owner_session_id,
            None,
        )
        .await
    }

    /// Executes one exact provider-tool target inside an explicit local-resource workspace.
    /// 在显式本地资源工作区内执行一个精确供应商工具目标。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, `selection`, and `arguments` freeze the invocation;
    /// `owner_session_id` owns recoverable tasks and `workspace_root` bounds every local path.
    /// `configuration`、`capability`、`selection` 与 `arguments` 冻结调用；`owner_session_id`
    /// 持有可恢复任务，`workspace_root` 约束每条本地路径。
    ///
    /// # Returns
    /// 返回值
    /// The normalized result or a controlled resolution, upload, credential, or execution error.
    /// 返回归一化结果，或受控的解析、上传、凭据与执行错误。
    pub async fn execute_provider_tool_target_for_owner_in_workspace(
        &self,
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        selection: &ProviderToolTargetSelection,
        arguments: serde_json::Value,
        owner_session_id: Option<String>,
        workspace_root: Option<&Path>,
    ) -> Result<ProviderToolExecutionResult, String> {
        if !capability.is_ai_visible() {
            return Err(format!(
                "Capability `{capability:?}` is internal inference and cannot execute as an AI-visible provider tool."
            ));
        }
        let resolved = self.resolve_provider_tool_target(configuration, capability, selection)?;
        let diagnostic_context = resolved.diagnostic_context();
        // operationTimeout is Manifest-owned for declarative operations; legacy code targets use
        // one explicit host ceiling instead of inheriting the streaming client's unlimited body.
        // operationTimeout 对声明式操作由 Manifest 持有；旧代码目标使用显式宿主上限，
        // 而不是继承流式客户端的无限正文时长。
        let operation_timeout = match resolved.operation.as_ref() {
            Some(operation) => Duration::from_millis(operation.timeout_ms.ok_or_else(|| {
                diagnose_provider_tool_failure(
                    format!(
                        "Provider operation `{}` has no required timeout_ms.",
                        operation.operation_id
                    ),
                    diagnostic_context.as_str(),
                )
            })?),
            None => LEGACY_PROVIDER_TOOL_OPERATION_TIMEOUT,
        };
        let operation_deadline =
            Instant::now()
                .checked_add(operation_timeout)
                .ok_or_else(|| {
                    diagnose_provider_tool_failure(
                        "Provider operation monotonic deadline overflowed.".to_owned(),
                        diagnostic_context.as_str(),
                    )
                })?;
        let operation_deadline_unix_ms = current_unix_millis()?
            .checked_add(operation_timeout.as_millis() as u64)
            .ok_or_else(|| {
                diagnose_provider_tool_failure(
                    "Provider operation persisted deadline overflowed.".to_owned(),
                    diagnostic_context.as_str(),
                )
            })?;
        let arguments = crate::provider_tool_execution::canonicalize_provider_tool_arguments(
            &resolved.target,
            arguments,
        )
        .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
        let secret = tokio::time::timeout(
            remaining_provider_operation_timeout(
                operation_deadline,
                "Provider tool credential resolution",
            )?,
            self.resolve_fresh_execution_secret(
                resolved.connection.id.as_str(),
                resolved.connection.secret_reference.as_str(),
            ),
        )
        .await
        .map_err(|_| {
            diagnose_provider_tool_failure(
                "Provider tool credential resolution exceeded the frozen operation timeout."
                    .to_owned(),
                diagnostic_context.as_str(),
            )
        })?
        .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
        // Model-extension understanding tools have no conversation ledger; their upload manager
        // reuses a provider object only for the same modality, source bytes, and upload contract.
        // 模型扩展理解工具没有会话账本；上传管理器只按相同模态、源字节与上传契约复用供应商对象。
        let mut arguments = arguments;
        let mut operation_for_upload = resolved.operation.clone();
        let mut media_upload_preflight = None;
        if provider_tool_media_resource_kind(capability).is_some()
            && let Some(operation) = operation_for_upload.as_ref()
            && !operation.upload_bindings.is_empty()
        {
            let credential_profile = resolved
                .entry
                .credential_profiles
                .iter()
                .find(|profile| profile.id == resolved.connection.credential_profile_id)
                .ok_or_else(|| {
                    diagnose_provider_tool_failure(
                        format!(
                            "Provider tool credential profile `{}` is absent from service entry `{}`.",
                            resolved.connection.credential_profile_id, resolved.entry.id
                        ),
                        diagnostic_context.as_str(),
                    )
                })?;
            let preflight = tokio::time::timeout(
                remaining_provider_operation_timeout(
                    operation_deadline,
                    "Provider tool upload cache preflight",
                )?,
                prepare_provider_tool_media_upload(
                    &self.execution_client,
                    &resolved,
                    operation,
                    &arguments,
                    credential_profile,
                    workspace_root,
                    capability,
                ),
            )
            .await
            .map_err(|_| {
                diagnose_provider_tool_failure(
                    "Provider tool upload cache preflight exceeded the frozen operation timeout."
                        .to_owned(),
                    diagnostic_context.as_str(),
                )
            })?
            .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
            if let Some((preflight, cached_reference)) = preflight {
                if let Some(cached_reference) = cached_reference {
                    arguments = crate::upload_engine::materialize_cached_scalar_operation_upload(
                        &preflight.binding,
                        &preflight.inspection.resource,
                        arguments,
                        cached_reference.provider_reference,
                        cached_reference.reference_encoding,
                    )
                    .map_err(|error| {
                        diagnose_provider_tool_failure(error, diagnostic_context.as_str())
                    })?;
                    operation_for_upload
                        .as_mut()
                        .ok_or_else(|| {
                            diagnose_provider_tool_failure(
                                "Media upload cache hit lost its owning operation.".to_owned(),
                                diagnostic_context.as_str(),
                            )
                        })?
                        .upload_bindings
                        .clear();
                } else {
                    media_upload_preflight = Some(preflight);
                }
            }
        }
        let arguments = match operation_for_upload.as_ref() {
            Some(operation) if !operation.upload_bindings.is_empty() => {
                // credentialProfile is shared by upload authorization and exact progress routing.
                // credentialProfile 由上传鉴权与精确进度路由共同使用。
                let credential_profile = resolved
                    .entry
                    .credential_profiles
                    .iter()
                    .find(|profile| profile.id == resolved.connection.credential_profile_id)
                    .ok_or_else(|| {
                        diagnose_provider_tool_failure(
                            format!(
                                "Provider tool credential profile `{}` is absent from service entry `{}`.",
                                resolved.connection.credential_profile_id, resolved.entry.id
                            ),
                            diagnostic_context.as_str(),
                        )
                    })?;
                // observer is cloned before upload so callbacks never retain the runtime registry lock.
                // observer 在上传前克隆，确保回调不会占用运行时注册表锁。
                let observer = self
                    .runtime_media_upload_progress_observer
                    .read()
                    .map_err(|_| {
                        diagnose_provider_tool_failure(
                            "The runtime media upload observer lock is poisoned.".to_owned(),
                            diagnostic_context.as_str(),
                        )
                    })?
                    .clone();
                // progressContext is present only for one owned AI-visible media understanding upload.
                // progressContext 仅在存在所有者的 AI 可见媒体理解上传中存在。
                let progress_context = provider_tool_media_upload_progress_context(
                    observer,
                    owner_session_id.as_deref(),
                    capability,
                    &resolved,
                    operation,
                    &arguments,
                    credential_profile,
                    workspace_root,
                )
                .await
                .map_err(|error| {
                    diagnose_provider_tool_failure(error, diagnostic_context.as_str())
                })?;
                if let Some(context) = progress_context.as_ref() {
                    context.emit(
                        crate::RuntimeMediaUploadPhase::Preparing,
                        0,
                        context.base.total_bytes,
                        None,
                    );
                }
                // progressTerminalGuard emits cancellation when the owning tool future is dropped.
                // progressTerminalGuard 在所属工具 Future 被丢弃时发出取消事件。
                let mut progress_terminal_guard =
                    RuntimeMediaUploadTerminalGuard::new(progress_context.as_ref());
                // progress forwards byte facts from the exact generic upload engine invocation.
                // progress 从精确的通用上传引擎调用转发字节事实。
                let progress = progress_context
                    .as_ref()
                    .map(RuntimeMediaUploadProgressContext::byte_callback);
                // uploadCancellation owns this single synchronous provider-tool upload attempt.
                // uploadCancellation 持有本次同步供应商工具上传尝试。
                let upload_cancellation = crate::MediaUploadCancellation::new();
                // uploadResult retains terminal failure so the UI receives a failed replacement.
                // uploadResult 保留终态失败，使界面收到失败替换事件。
                let upload_result = tokio::time::timeout(
                    remaining_provider_operation_timeout(
                        operation_deadline,
                        "Provider tool upload materialization",
                    )?,
                    crate::upload_engine::materialize_operation_uploads(
                        crate::upload_engine::OperationUploadExecutionContext {
                            http: &self.execution_client.http,
                            entry: &resolved.entry,
                            secret: &secret,
                            credential_profile,
                            channels: &resolved.upload_channels,
                            progress,
                            cancellation: &upload_cancellation,
                            model_id: None,
                            workspace_root,
                            fixture_loopback_origin: None,
                        },
                        operation,
                        arguments,
                    ),
                )
                .await;
                let upload_result = match upload_result {
                    Ok(Ok(arguments)) => Ok(arguments),
                    Ok(Err(error)) => Err(diagnose_provider_tool_failure_with_secret(
                        error,
                        diagnostic_context.as_str(),
                        &secret,
                    )),
                    Err(_) => Err(diagnose_provider_tool_failure(
                        "Provider tool upload materialization exceeded the frozen operation timeout."
                            .to_owned(),
                        diagnostic_context.as_str(),
                    )),
                };
                match upload_result {
                    Ok(arguments) => {
                        if let Some(preflight) = media_upload_preflight.as_ref() {
                            let local_path = preflight
                                .inspection
                                .resource
                                .local_path
                                .as_ref()
                                .ok_or_else(|| {
                                    diagnose_provider_tool_failure(
                                        "Media upload cache preflight lost its local source path."
                                            .to_owned(),
                                        diagnostic_context.as_str(),
                                    )
                                })?;
                            let provider_reference = provider_tool_materialized_reference(
                                &arguments,
                                &preflight.binding,
                            )
                            .map_err(|error| {
                                diagnose_provider_tool_failure(error, diagnostic_context.as_str())
                            })?;
                            self.execution_client
                                .remember_provider_tool_media_reference(
                                    resolved.connection.provider_id.as_str(),
                                    resolved.entry.id.as_str(),
                                    resolved.connection.id.as_str(),
                                    &preflight.inspection.channel,
                                    local_path,
                                    preflight.resource_kind,
                                    preflight.inspection.resource.mime_type.as_str(),
                                    &preflight.source_identity,
                                    provider_reference,
                                    preflight.first_used_at_unix_millis,
                                )
                                .await
                                .map_err(|error| {
                                    diagnose_provider_tool_failure_with_secret(
                                        error,
                                        diagnostic_context.as_str(),
                                        &secret,
                                    )
                                })?;
                        }
                        if let Some(context) = progress_context.as_ref() {
                            context.emit(
                                crate::RuntimeMediaUploadPhase::Completed,
                                context.base.total_bytes.unwrap_or_default(),
                                context.base.total_bytes,
                                None,
                            );
                        }
                        progress_terminal_guard.disarm();
                        arguments
                    }
                    Err(error) => {
                        if let Some(context) = progress_context.as_ref() {
                            context.emit(
                                crate::RuntimeMediaUploadPhase::Failed,
                                context.last_bytes_sent.load(Ordering::Acquire),
                                context.base.total_bytes,
                                Some(error.clone()),
                            );
                        }
                        progress_terminal_guard.disarm();
                        return Err(error);
                    }
                }
            }
            _ => arguments,
        };
        if resolved.target.execution.execution_kind
            == crate::ProviderToolExecutionKind::HttpJsonAsyncPoll
        {
            let execution_id = tokio::time::timeout(
                remaining_provider_operation_timeout(
                    operation_deadline,
                    "Provider asynchronous task start",
                )?,
                self.start_provider_async_task(
                    resolved,
                    selection.clone(),
                    arguments,
                    secret,
                    owner_session_id,
                    operation_deadline_unix_ms,
                ),
            )
            .await
            .map_err(|_| {
                diagnose_provider_tool_failure(
                    "Provider asynchronous task start exceeded the frozen operation timeout."
                        .to_owned(),
                    diagnostic_context.as_str(),
                )
            })??;
            return self
                .await_provider_async_task(configuration, execution_id.as_str())
                .await;
        }
        tokio::time::timeout(
            remaining_provider_operation_timeout(operation_deadline, "Provider tool execution")?,
            self.execution_client.execute_provider_tool_with_profiles(
                &resolved.entry,
                &resolved.target,
                &resolved.request_profiles,
                &resolved.response_profiles,
                arguments,
                &secret,
            ),
        )
        .await
        .map_err(|_| {
            diagnose_provider_tool_failure(
                "Provider tool execution exceeded the frozen operation timeout.".to_owned(),
                diagnostic_context.as_str(),
            )
        })?
        .map_err(|error| {
            diagnose_provider_tool_failure_with_secret(error, diagnostic_context.as_str(), &secret)
        })
    }

    /// Configures durable provider-task recovery and seals interrupted dispatch intents.
    /// 配置持久化供应商任务恢复并封闭中断的分派意图。
    ///
    /// # Parameters
    /// 参数
    /// `directory` stores only versioned non-secret provider-task snapshots.
    /// `directory` 仅存储带版本的非秘密供应商任务快照。
    ///
    /// # Returns
    /// 返回值
    /// The loaded records after every interrupted dispatch is marked outcome-unknown.
    /// 返回加载后的记录，其中每个中断分派都已标记为结果不确定。
    pub fn configure_provider_task_recovery(
        &self,
        directory: &Path,
    ) -> Result<Vec<ProviderAsyncTaskRecord>, String> {
        let records = self.provider_tasks.configure(directory)?;
        for mut record in records {
            if record.phase == ProviderAsyncTaskPhase::Dispatching {
                record.phase = ProviderAsyncTaskPhase::DispatchOutcomeUnknown;
                record.updated_at_unix_ms = current_unix_millis()?;
                record.error = Some(String::from(
                    "Provider dispatch was interrupted before a task identifier was confirmed; automatic replay is forbidden.",
                ));
                record.consecutive_observation_failures = 0;
                self.provider_tasks.replace(record)?;
            }
        }
        self.provider_tasks.snapshot()
    }

    /// Returns one stable provider-task status snapshot.
    /// 返回一份稳定供应商任务状态快照。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the host-owned stable task identifier.
    /// `execution_id` 是宿主持有的稳定任务标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact retained non-secret record.
    /// 返回精确保留的非秘密记录。
    pub fn provider_task_status(
        &self,
        execution_id: &str,
    ) -> Result<ProviderAsyncTaskRecord, String> {
        self.provider_tasks.require(execution_id)
    }

    /// Performs one recovery observation for every active retained provider task.
    /// 对每个活动保留供应商任务执行一次恢复观测。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies current credential material for exact retained connection IDs.
    /// `configuration` 为精确保留连接标识提供当前凭据材料。
    ///
    /// # Returns
    /// 返回值
    /// Updated records; transient observation failures remain non-terminal diagnostics.
    /// 返回更新记录；瞬时观测失败保留为非终态诊断。
    pub async fn recover_provider_tasks(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<Vec<ProviderAsyncTaskRecord>, String> {
        let records = self.provider_tasks.snapshot()?;
        // observations isolate slow providers behind a fixed concurrency ceiling; one exhausted
        // 60-second request budget cannot serially delay every unrelated task.
        // observations 以固定并发上限隔离慢供应商；单次耗尽 60 秒请求预算不能串行拖延全部
        // 无关任务。
        let mut observations = observe_provider_task_records_bounded(records, |record| {
            self.recover_provider_task_record(configuration, record)
        })
        .await;
        // failures are sorted back to stable task identity so concurrent completion order never
        // changes the surfaced store failure.
        // failures 按稳定任务身份重新排序，因此并发完成顺序不会改变对外呈现的存储失败。
        observations.sort_by(|left, right| left.0.cmp(&right.0));
        if let Some((execution_id, error)) = observations
            .into_iter()
            .find_map(|(execution_id, result)| result.err().map(|error| (execution_id, error)))
        {
            return Err(format!(
                "Provider task recovery failed for execution `{execution_id}`: {error}"
            ));
        }
        self.provider_tasks.snapshot()
    }

    /// Recovers one retained provider task without allowing it to block unrelated tasks.
    /// 恢复一条保留供应商任务，且不允许其阻塞无关任务。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` supplies current credential material and `record` is one immutable
    /// recovery-pass input selected from the durable snapshot.
    /// `configuration` 提供当前凭据材料，`record` 是从持久快照选出的一条不可变恢复轮次输入。
    ///
    /// # Returns
    /// 返回值
    /// Success after a no-op, cancellation observation, poll observation, or durable transient
    /// diagnostic; only state-store failures escape to the aggregate pass.
    /// 在无操作、取消观测、轮询观测或持久化瞬时诊断后返回成功；只有状态存储失败会返回给
    /// 聚合恢复轮次。
    async fn recover_provider_task_record(
        &self,
        configuration: &PersistedAiConfiguration,
        record: ProviderAsyncTaskRecord,
    ) -> Result<(), String> {
        // needs_completed_result_recovery identifies durable completion records whose transient
        // result bytes were intentionally omitted.
        // needs_completed_result_recovery 标识有意省略瞬时结果字节的持久完成记录。
        let needs_completed_result_recovery =
            record.phase == ProviderAsyncTaskPhase::Completed && record.result.is_none();
        if record.phase.is_terminal() && !needs_completed_result_recovery {
            return Ok(());
        }
        if record.phase == ProviderAsyncTaskPhase::CancellationRequested {
            if let Err(error) = self
                .cancel_provider_task_internal(configuration, record.execution_id.as_str())
                .await
            {
                self.record_provider_task_observation_error(record.execution_id.as_str(), error)?;
            }
            return Ok(());
        }
        if let Err(error) = self
            .poll_provider_async_task_once(configuration, record.execution_id.as_str())
            .await
        {
            self.record_provider_task_observation_error(record.execution_id.as_str(), error)?;
        }
        Ok(())
    }

    /// Requests provider cancellation while durably recording intent first.
    /// 请求供应商取消，并首先持久化记录取消意图。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `execution_id`, and optional `owner_session_id` identify one exact task.
    /// `configuration`、`execution_id` 与可选 `owner_session_id` 标识一个精确任务。
    ///
    /// # Returns
    /// 返回值
    /// A provider-confirmed cancelled record; uncertain cancellation remains recoverable.
    /// 返回供应商确认取消的记录；不确定取消保持可恢复。
    pub async fn cancel_provider_task(
        &self,
        configuration: &PersistedAiConfiguration,
        execution_id: &str,
        owner_session_id: Option<&str>,
    ) -> Result<ProviderAsyncTaskRecord, String> {
        let mut record = self.provider_tasks.require(execution_id)?;
        if let Some(expected_owner) = record.owner_session_id.as_deref()
            && Some(expected_owner) != owner_session_id
        {
            return Err(format!(
                "Provider task execution `{execution_id}` is owned by another session."
            ));
        }
        if record.phase.is_terminal() {
            return Err(format!(
                "Provider task execution `{execution_id}` is already terminal."
            ));
        }
        if !record.cancellation_supported {
            return Err(format!(
                "Provider task execution `{execution_id}` does not support provider cancellation."
            ));
        }
        record.phase = ProviderAsyncTaskPhase::CancellationRequested;
        record.updated_at_unix_ms = current_unix_millis()?;
        record.error = None;
        record.consecutive_observation_failures = 0;
        self.provider_tasks.replace(record)?;
        self.cancel_provider_task_internal(configuration, execution_id)
            .await
    }

    /// Durably records cancellation intent for every active provider task owned by one session.
    /// 为某个会话拥有的全部活动供应商任务持久化记录取消意图。
    ///
    /// # Parameters
    /// 参数
    /// `owner_session_id` is the exact conversation identity supplied when the tool started.
    /// `owner_session_id` 是工具启动时提供的精确会话身份。
    ///
    /// # Returns
    /// 返回值
    /// The number of records changed before the caller's asynchronous future is dropped.
    /// 返回调用方异步 Future 被丢弃前已变更的记录数量。
    pub fn request_provider_task_cancellation_for_owner(
        &self,
        owner_session_id: &str,
    ) -> Result<usize, String> {
        let mut changed = 0usize;
        for mut record in self.provider_tasks.snapshot()? {
            if record.owner_session_id.as_deref() != Some(owner_session_id)
                || record.phase.is_terminal()
            {
                continue;
            }
            if record.provider_task_id.is_none() {
                // A dropped dispatch without a confirmed task identity must never be replayed.
                // 未确认任务身份的已丢弃分派绝不能被重放。
                record.phase = ProviderAsyncTaskPhase::DispatchOutcomeUnknown;
                record.error = Some(String::from(
                    "The owning conversation stopped before provider dispatch confirmed a task identifier; automatic replay is forbidden.",
                ));
                record.consecutive_observation_failures = 0;
            } else if record.cancellation_supported {
                record.phase = ProviderAsyncTaskPhase::CancellationRequested;
                record.error = None;
                record.consecutive_observation_failures = 0;
            } else {
                // Non-cancellable provider work remains observable in the background until terminal.
                // 不支持取消的供应商任务继续由后台观测直至终态。
                record.error = Some(String::from(
                    "The owning conversation stopped, but this provider task has no cancellation endpoint; background observation continues.",
                ));
                record.consecutive_observation_failures = 0;
            }
            record.updated_at_unix_ms = current_unix_millis()?;
            self.provider_tasks.replace(record)?;
            changed = changed.saturating_add(1);
        }
        Ok(changed)
    }

    /// Starts one durable task after the route and credential are already frozen.
    /// 在路由与凭据已冻结后启动一个可持久化任务。
    ///
    /// # Parameters
    /// 参数
    /// `resolved`, `selection`, `arguments`, and `secret` freeze the exact dispatch;
    /// `owner_session_id` owns recovery, and `operation_deadline_unix_ms` preserves the total
    /// operation budget already consumed by credential and upload preparation.
    /// `resolved`、`selection`、`arguments` 与 `secret` 冻结精确分派；`owner_session_id`
    /// 持有恢复归属，`operation_deadline_unix_ms` 保留凭据与上传准备已经消耗的总操作预算。
    ///
    /// # Returns
    /// 返回值
    /// The durable execution id after confirmed task creation, or a controlled pre-dispatch,
    /// timeout, transport, or persistence failure.
    /// 确认任务创建后返回持久执行标识，或返回受控的分派前、超时、传输与持久化失败。
    async fn start_provider_async_task(
        &self,
        resolved: ResolvedProviderToolTarget,
        selection: ProviderToolTargetSelection,
        arguments: serde_json::Value,
        secret: CredentialExecutionSecret,
        owner_session_id: Option<String>,
        operation_deadline_unix_ms: u64,
    ) -> Result<String, String> {
        let diagnostic_context = resolved.diagnostic_context();
        let now = current_unix_millis()?;
        let execution_id = Uuid::new_v4().to_string();
        let operation = resolved.operation.as_ref().ok_or_else(|| {
            format!(
                "Provider asynchronous target `{}` is not owned by one operation declaration.",
                resolved.target.target_id
            )
        })?;
        let async_execution = operation.async_execution.as_ref().ok_or_else(|| {
            format!(
                "Provider asynchronous operation `{}` has no async execution declaration.",
                operation.operation_id
            )
        })?;
        let operation_id = operation.operation_id.clone();
        let poll_interval_ms = async_execution.poll_interval_ms;
        let task_timeout_ms = async_execution.timeout_ms;
        let manifest_allows_cancellation = async_execution.cancellation_supported;
        // Freeze the source manifest revision and the complete non-secret task route at dispatch;
        // every later recovery or poll resolves the service entry and target from this snapshot
        // and never reads a newer manifest (plan §R9.6).
        // 在分派时冻结源清单修订号与完整非秘密任务路由；后续每次恢复或轮询都从该快照
        // 解析服务入口与目标，绝不读取更新的 Manifest（方案 §R9.6）。
        let manifest_revision = resolved.manifest_revision;
        let (frozen_target, frozen_service_entry) = if manifest_revision > 0 {
            (Some(resolved.target.clone()), Some(resolved.entry.clone()))
        } else {
            // Code-owned targets are static host code with no manifest provenance; the route is
            // reconstructed from the immutable catalog at recovery.
            // 代码持有目标是静态宿主代码且无清单溯源；恢复时从不可变目录重建路由。
            (None, None)
        };
        let task_deadline_unix_ms = now
            .checked_add(task_timeout_ms)
            .ok_or_else(|| String::from("Provider asynchronous task deadline overflowed."))?
            .min(operation_deadline_unix_ms);
        if task_deadline_unix_ms <= now {
            return Err(String::from(
                "Provider asynchronous task exhausted its frozen operation timeout before dispatch.",
            ));
        }
        let mut record = ProviderAsyncTaskRecord {
            execution_id: execution_id.clone(),
            owner_session_id,
            route: ProviderAsyncTaskRoute {
                provider_id: resolved.connection.provider_id.clone(),
                service_entry_id: resolved.entry.id.clone(),
                connection_id: resolved.connection.id.clone(),
                selection,
                adapter: resolved.target.execution.adapter,
                manifest_revision,
                frozen_target,
                frozen_service_entry,
                operation_id: Some(operation_id),
                frozen_operation: Some(operation.clone()),
                frozen_request_profiles: resolved.request_profiles.clone(),
                frozen_response_profiles: resolved.response_profiles.clone(),
            },
            provider_task_id: None,
            recovery: ProviderAsyncTaskRecoveryContext::None,
            phase: ProviderAsyncTaskPhase::Dispatching,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
            deadline_unix_ms: task_deadline_unix_ms,
            poll_interval_ms,
            consecutive_observation_failures: 0,
            cancellation_supported: manifest_allows_cancellation,
            error: None,
            result: None,
        };
        self.provider_tasks.replace(record.clone())?;
        let started = match await_bounded_provider_task_operation(
            PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT.min(Duration::from_millis(
                task_deadline_unix_ms.saturating_sub(now),
            )),
            "Provider asynchronous task start",
            crate::provider_tool_execution::start_provider_async_task(
                crate::provider_tool_execution::ProviderAsyncExecutionContext {
                    http: &self.execution_client.http,
                    entry: &resolved.entry,
                    target: &resolved.target,
                    request_profiles: &resolved.request_profiles,
                    response_profiles: &resolved.response_profiles,
                    secret: &secret,
                },
                arguments,
                Some(async_execution),
            ),
        )
        .await
        {
            Ok(started) => started,
            Err(error) => {
                let error = diagnose_provider_tool_failure_with_secret(
                    error,
                    diagnostic_context.as_str(),
                    &secret,
                );
                record.phase = ProviderAsyncTaskPhase::DispatchOutcomeUnknown;
                record.updated_at_unix_ms = current_unix_millis()?;
                record.error = Some(format!(
                    "Provider task start did not confirm a task identifier; automatic replay is forbidden: {error}"
                ));
                self.provider_tasks.replace(record)?;
                return Err(error);
            }
        };
        record.provider_task_id = Some(started.provider_task_id);
        record.recovery = started.recovery;
        record.phase = ProviderAsyncTaskPhase::Queued;
        record.cancellation_supported =
            manifest_allows_cancellation && started.cancellation_supported;
        record.updated_at_unix_ms = current_unix_millis()?;
        self.provider_tasks.replace(record)?;
        Ok(execution_id)
    }

    /// Waits for one durable task while polling only the same provider task identifier.
    /// 等待一个可持久化任务，同时仅轮询同一个供应商任务标识。
    async fn await_provider_async_task(
        &self,
        configuration: &PersistedAiConfiguration,
        execution_id: &str,
    ) -> Result<ProviderToolExecutionResult, String> {
        loop {
            let record = self.provider_tasks.require(execution_id)?;
            match record.phase {
                ProviderAsyncTaskPhase::Completed => {
                    if let Some(result) = record.result {
                        return Ok(result);
                    }
                    // Durable snapshots intentionally omit generated bytes and temporary URLs.
                    // 持久快照有意省略生成字节与临时 URL。
                    self.poll_provider_async_task_once(configuration, execution_id)
                        .await?;
                    continue;
                }
                ProviderAsyncTaskPhase::Failed
                | ProviderAsyncTaskPhase::Cancelled
                | ProviderAsyncTaskPhase::DispatchOutcomeUnknown => {
                    return Err(record.error.unwrap_or_else(|| {
                        format!(
                            "Provider task execution `{execution_id}` ended without a diagnostic."
                        )
                    }));
                }
                ProviderAsyncTaskPhase::Dispatching
                | ProviderAsyncTaskPhase::Queued
                | ProviderAsyncTaskPhase::Running
                | ProviderAsyncTaskPhase::CancellationRequested => {}
            }
            if let Some(timeout_error) = provider_task_timeout_diagnostic(
                record.created_at_unix_ms,
                record.deadline_unix_ms,
                current_unix_millis()?,
            ) {
                let mut timed_out = record;
                timed_out.phase = ProviderAsyncTaskPhase::Failed;
                timed_out.updated_at_unix_ms = current_unix_millis()?;
                timed_out.error = Some(timeout_error);
                timed_out.consecutive_observation_failures = 0;
                self.provider_tasks.replace(timed_out.clone())?;
                return Err(timed_out.error.expect("timeout diagnostic is present"));
            }
            // nextDelay cannot cross the frozen lifecycle deadline even after failure backoff.
            // nextDelay 即使处于失败退避也不能越过冻结生命周期截止时间。
            let next_delay_ms = clamp_provider_task_observation_delay_ms(
                provider_task_observation_delay_ms(&record),
                record.deadline_unix_ms,
                current_unix_millis()?,
            );
            tokio::time::sleep(Duration::from_millis(next_delay_ms)).await;
            if let Err(error) = self
                .poll_provider_async_task_once(configuration, execution_id)
                .await
            {
                self.record_provider_task_observation_error(execution_id, error)?;
            }
        }
    }

    /// Observes one retained provider task once and persists every confirmed transition.
    /// 单次观测一个保留供应商任务并持久化每个已确认转换。
    async fn poll_provider_async_task_once(
        &self,
        configuration: &PersistedAiConfiguration,
        execution_id: &str,
    ) -> Result<ProviderAsyncTaskRecord, String> {
        let mut record = self.provider_tasks.require(execution_id)?;
        let needs_completed_result_recovery =
            record.phase == ProviderAsyncTaskPhase::Completed && record.result.is_none();
        if record.phase.is_terminal() && !needs_completed_result_recovery {
            return Ok(record);
        }
        if record.phase == ProviderAsyncTaskPhase::CancellationRequested {
            return self
                .cancel_provider_task_internal(configuration, execution_id)
                .await;
        }
        // A provider-confirmed completed task is no longer executing. Its original work deadline
        // must not block the read-only re-observation needed to reconstruct omitted result bytes.
        // 供应商已确认完成的任务不再执行；原始工作截止时间不得阻止为重建已省略结果字节而进行的
        // 只读重新观测。
        if !needs_completed_result_recovery
            && let Some(timeout_error) = provider_task_timeout_diagnostic(
                record.created_at_unix_ms,
                record.deadline_unix_ms,
                current_unix_millis()?,
            )
        {
            record.phase = ProviderAsyncTaskPhase::Failed;
            record.updated_at_unix_ms = current_unix_millis()?;
            record.error = Some(timeout_error);
            record.consecutive_observation_failures = 0;
            self.provider_tasks.replace(record.clone())?;
            return Ok(record);
        }
        let provider_task_id = record.provider_task_id.clone().ok_or_else(|| {
            format!(
                "Provider task execution `{execution_id}` has no confirmed provider task identifier."
            )
        })?;
        let resolved = self.resolve_recovered_provider_task(configuration, &record)?;
        let diagnostic_context = resolved.diagnostic_context();
        // observationDeadline covers credential resolution and transport together. Otherwise a
        // blocked refresh lock could bypass both the task lifecycle deadline and the host's
        // nominal 60-second observation ceiling.
        // observationDeadline 同时覆盖凭据解析与传输。否则阻塞的刷新锁可能绕过任务生命周期
        // 截止时间以及宿主名义上的 60 秒观测上限。
        let observation_deadline = Instant::now()
            .checked_add(PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT)
            .ok_or_else(|| String::from("Provider task observation deadline overflowed."))?;
        let credential_timeout = if needs_completed_result_recovery {
            remaining_provider_operation_timeout(
                observation_deadline,
                "Provider asynchronous task credential resolution",
            )?
        } else {
            remaining_provider_operation_timeout(
                observation_deadline,
                "Provider asynchronous task credential resolution",
            )?
            .min(Duration::from_millis(
                record
                    .deadline_unix_ms
                    .saturating_sub(current_unix_millis()?),
            ))
        };
        let secret = await_bounded_provider_task_operation(
            credential_timeout,
            "Provider asynchronous task credential resolution",
            self.resolve_fresh_execution_secret(
                resolved.connection.id.as_str(),
                resolved.connection.secret_reference.as_str(),
            ),
        )
        .await
        .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
        // observationTimeout is recomputed after credential resolution so secret refresh time also
        // consumes the frozen lifecycle budget. Completed-result recovery is read-only and retains
        // the independent bounded observation timeout.
        // observationTimeout 在凭据解析后重新计算，使 Secret 刷新耗时同样消耗冻结生命周期
        // 预算；已完成结果恢复属于只读操作，继续使用独立的有界观测超时。
        let observation_timeout = if needs_completed_result_recovery {
            remaining_provider_operation_timeout(
                observation_deadline,
                "Provider asynchronous task observation",
            )?
        } else {
            let now_unix_ms = current_unix_millis()?;
            if let Some(timeout_error) = provider_task_timeout_diagnostic(
                record.created_at_unix_ms,
                record.deadline_unix_ms,
                now_unix_ms,
            ) {
                record.phase = ProviderAsyncTaskPhase::Failed;
                record.updated_at_unix_ms = now_unix_ms;
                record.error = Some(timeout_error);
                record.consecutive_observation_failures = 0;
                self.provider_tasks.replace(record.clone())?;
                return Ok(record);
            }
            remaining_provider_operation_timeout(
                observation_deadline,
                "Provider asynchronous task observation",
            )?
            .min(Duration::from_millis(
                record.deadline_unix_ms.saturating_sub(now_unix_ms),
            ))
        };
        match await_bounded_provider_task_operation(
            observation_timeout,
            "Provider asynchronous task observation",
            crate::provider_tool_execution::poll_provider_async_task(
                crate::provider_tool_execution::ProviderAsyncExecutionContext {
                    http: &self.execution_client.http,
                    entry: &resolved.entry,
                    target: &resolved.target,
                    request_profiles: &resolved.request_profiles,
                    response_profiles: &resolved.response_profiles,
                    secret: &secret,
                },
                provider_task_id.as_str(),
                &record.recovery,
                resolved
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.async_execution.as_ref()),
            ),
        )
        .await
        .map_err(|error| {
            diagnose_provider_tool_failure_with_secret(error, diagnostic_context.as_str(), &secret)
        })? {
            ProviderAsyncTaskPoll::Pending(phase) => {
                record.phase = phase;
                record.error = None;
                record.consecutive_observation_failures = 0;
            }
            ProviderAsyncTaskPoll::Completed(result) => {
                record.phase = ProviderAsyncTaskPhase::Completed;
                record.result = Some(result);
                record.error = None;
                record.consecutive_observation_failures = 0;
            }
            ProviderAsyncTaskPoll::Failed(error) => {
                record.phase = ProviderAsyncTaskPhase::Failed;
                record.error = Some(redact_provider_tool_secret(error.as_str(), &secret));
                record.consecutive_observation_failures = 0;
            }
            ProviderAsyncTaskPoll::Cancelled => {
                record.phase = ProviderAsyncTaskPhase::Cancelled;
                record.error = Some(String::from("Provider task was cancelled."));
                record.consecutive_observation_failures = 0;
            }
        }
        record.updated_at_unix_ms = current_unix_millis()?;
        self.provider_tasks.replace(record.clone())?;
        Ok(record)
    }

    /// Sends one cancellation request for a previously persisted intent.
    /// 为先前已持久化的意图发送一次取消请求。
    async fn cancel_provider_task_internal(
        &self,
        configuration: &PersistedAiConfiguration,
        execution_id: &str,
    ) -> Result<ProviderAsyncTaskRecord, String> {
        let mut record = self.provider_tasks.require(execution_id)?;
        if record.phase != ProviderAsyncTaskPhase::CancellationRequested {
            return Err(format!(
                "Provider task execution `{execution_id}` has no durable cancellation intent."
            ));
        }
        let provider_task_id = record.provider_task_id.clone().ok_or_else(|| {
            format!(
                "Provider task execution `{execution_id}` has no confirmed provider task identifier."
            )
        })?;
        let resolved = self.resolve_recovered_provider_task(configuration, &record)?;
        let diagnostic_context = resolved.diagnostic_context();
        // cancellationDeadline is one total host budget for both credential refresh and the
        // provider cancellation request.
        // cancellationDeadline 是凭据刷新与供应商取消请求共同使用的一份宿主总预算。
        let cancellation_deadline = Instant::now()
            .checked_add(PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT)
            .ok_or_else(|| String::from("Provider task cancellation deadline overflowed."))?;
        let secret = await_bounded_provider_task_operation(
            remaining_provider_operation_timeout(
                cancellation_deadline,
                "Provider asynchronous task credential resolution",
            )?,
            "Provider asynchronous task credential resolution",
            self.resolve_fresh_execution_secret(
                resolved.connection.id.as_str(),
                resolved.connection.secret_reference.as_str(),
            ),
        )
        .await
        .map_err(|error| diagnose_provider_tool_failure(error, diagnostic_context.as_str()))?;
        await_bounded_provider_task_operation(
            remaining_provider_operation_timeout(
                cancellation_deadline,
                "Provider asynchronous task cancellation",
            )?,
            "Provider asynchronous task cancellation",
            crate::provider_tool_execution::cancel_provider_async_task(
                &self.execution_client.http,
                &resolved.entry,
                &resolved.target,
                provider_task_id.as_str(),
                &secret,
                resolved
                    .operation
                    .as_ref()
                    .and_then(|operation| operation.async_execution.as_ref()),
            ),
        )
        .await
        .map_err(|error| {
            diagnose_provider_tool_failure_with_secret(error, diagnostic_context.as_str(), &secret)
        })?;
        record.phase = ProviderAsyncTaskPhase::Cancelled;
        record.updated_at_unix_ms = current_unix_millis()?;
        record.error = Some(String::from("Provider cancellation was confirmed."));
        record.consecutive_observation_failures = 0;
        self.provider_tasks.replace(record.clone())?;
        Ok(record)
    }

    /// Resolves the exact retained route without consulting current tool selections.
    /// 解析精确保留路由且不查询当前工具选择。
    fn resolve_recovered_provider_task(
        &self,
        configuration: &PersistedAiConfiguration,
        record: &ProviderAsyncTaskRecord,
    ) -> Result<ResolvedProviderToolTarget, String> {
        let connection = configuration
            .connection_graph
            .connections
            .get(record.route.connection_id.as_str())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Provider task connection `{}` no longer exists.",
                    record.route.connection_id
                )
            })?;
        if connection.provider_id != record.route.provider_id
            || connection.service_entry_id != record.route.service_entry_id
            || record.route.selection.connection_id != connection.id
        {
            return Err(String::from(
                "Provider task route affinity no longer matches its retained connection.",
            ));
        }
        // Modern manifest tasks use both frozen halves of the non-secret route. The current
        // registry is deliberately not consulted, so replacing or retiring the provider entry
        // cannot redirect or reinterpret an already-created task (plan §R9.6).
        // 现代 Manifest 任务使用非秘密路由的两份冻结组成。这里刻意不查询当前注册表，
        // 因此替换或退役供应商入口不会重定向或重新解释已创建任务（方案 §R9.6）。
        let (entry, target) = if record.route.manifest_revision > 0 {
            let entry = record
                .route
                .frozen_service_entry
                .clone()
                .ok_or_else(|| {
                    format!(
                        "Provider task execution `{}` at manifest revision {} is missing its dispatch-frozen service-entry snapshot.",
                        record.execution_id, record.route.manifest_revision
                    )
                })?;
            let target = record.route.frozen_target.clone().ok_or_else(|| {
                format!(
                    "Provider task execution `{}` at manifest revision {} is missing its dispatch-frozen target snapshot.",
                    record.execution_id, record.route.manifest_revision
                )
            })?;
            (entry, target)
        } else {
            // Historical revision-zero records have no frozen endpoint evidence. Their exact
            // retained target id is reconstructed through the compatibility path and only then
            // receives the current connection origin.
            // 历史零修订记录没有冻结端点证据。兼容路径按其精确保留目标标识重建，
            // 并仅在此后应用当前连接源地址。
            let registry = self.effective_providers(configuration)?;
            let provider = registry
                .provider(record.route.provider_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Provider task provider `{}` is not registered.",
                        record.route.provider_id
                    )
                })?;
            let entry = provider
                .service_entries
                .iter()
                .find(|entry| entry.id == record.route.service_entry_id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Provider task service entry `{}` is not registered.",
                        record.route.service_entry_id
                    )
                })?;
            let legacy_target = self
                .resolved_provider_tool_targets(provider.id.as_str())
                .into_iter()
                .find(|target| target.target_id == record.route.selection.target_id)
                .ok_or_else(|| {
                    format!(
                        "Provider task target `{}` is not registered.",
                        record.route.selection.target_id
                    )
                })?;
            let target = crate::connection_endpoint::apply_connection_provider_tool_origin(
                &connection,
                &entry,
                legacy_target,
            );
            (entry, target)
        };
        let credential_profile_matches = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .is_some_and(|profile| profile.credential_mode == connection.credential_mode);
        if entry.id != record.route.service_entry_id
            || target.provider_id != record.route.provider_id
            || target.service_entry_id != entry.id
            || target.execution.adapter != record.route.adapter
            || !credential_profile_matches
            || !provider_tool_target_allows_credential(
                &target,
                connection.credential_profile_id.as_str(),
            )
        {
            return Err(String::from(
                "Provider task target no longer matches its immutable route affinity.",
            ));
        }
        Ok(ResolvedProviderToolTarget {
            connection,
            entry,
            target,
            operation: record.route.frozen_operation.clone(),
            request_profiles: record.route.frozen_request_profiles.clone(),
            response_profiles: record.route.frozen_response_profiles.clone(),
            upload_channels: Vec::new(),
            manifest_revision: record.route.manifest_revision,
            manifest_generation: 0,
            route_revision: format!(
                "provider-tool:{}:{}:{}",
                record.route.provider_id,
                record.route.connection_id,
                record.route.selection.target_id
            ),
        })
    }

    /// Retains one transient observation diagnostic without changing lifecycle phase.
    /// 保留一个瞬时观测诊断且不改变生命周期阶段。
    fn record_provider_task_observation_error(
        &self,
        execution_id: &str,
        error: String,
    ) -> Result<(), String> {
        let mut record = self.provider_tasks.require(execution_id)?;
        // Durable completed records intentionally lack process-local result bytes, so a failed
        // re-observation still needs a visible diagnostic even though the provider phase is terminal.
        // 持久完成记录刻意不含进程内结果字节，因此重新观测失败时即使供应商阶段已终结，
        // 仍需保留可见诊断。
        let needs_completed_result_recovery =
            record.phase == ProviderAsyncTaskPhase::Completed && record.result.is_none();
        if !record.phase.is_terminal() || needs_completed_result_recovery {
            let now_unix_ms = current_unix_millis()?;
            record.updated_at_unix_ms = now_unix_ms;
            if !needs_completed_result_recovery
                && let Some(timeout_error) = provider_task_timeout_diagnostic(
                    record.created_at_unix_ms,
                    record.deadline_unix_ms,
                    now_unix_ms,
                )
            {
                // A failed observation that consumed the frozen deadline is terminal now; it
                // must not wait for another recovery pass to reveal the timeout.
                // 已消耗冻结截止时间的失败观测立即终态化；不得等待下一轮恢复才暴露超时。
                record.phase = ProviderAsyncTaskPhase::Failed;
                record.error = Some(timeout_error);
                record.consecutive_observation_failures = 0;
            } else {
                record.error = Some(bound_provider_task_diagnostic(error.as_str()));
                record.consecutive_observation_failures = record
                    .consecutive_observation_failures
                    .saturating_add(1)
                    .min(crate::provider_task::PROVIDER_TASK_MAXIMUM_OBSERVATION_FAILURES);
            }
            self.provider_tasks.replace(record)?;
        }
        Ok(())
    }

    /// Resolves one persisted single-target selection without fallback.
    /// 解析一份持久化单目标选择且不执行回退。
    ///
    /// # Parameters
    /// 参数
    /// `configuration`, `capability`, and `selection` identify the exact target.
    /// `configuration`、`capability` 与 `selection` 标识精确目标。
    ///
    /// # Returns
    /// 返回值
    /// Frozen connection, entry, and target snapshots.
    /// 返回冻结的连接、入口与目标快照。
    fn resolve_provider_tool_target(
        &self,
        _configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        selection: &ProviderToolTargetSelection,
    ) -> Result<ResolvedProviderToolTarget, String> {
        if !capability.is_ai_visible() {
            return Err(format!(
                "Capability `{capability:?}` is internal inference and cannot resolve as an AI-visible provider tool."
            ));
        }
        // generation freezes every provider-tool input for this resolution.
        // generation 为本次解析冻结每项供应商工具输入。
        let generation = self.selection_catalog_snapshot()?;
        // configuration belongs to the retained immutable generation.
        // configuration 属于保留的不变代际。
        let configuration = generation.configuration.as_ref();
        let system = ProviderRegistry::from_catalog(generation.manifests.as_ref())?;
        let registry =
            ProviderRegistry::effective_from_system(system, &configuration.custom_providers)?;
        let connection = configuration
            .connection_graph
            .connections
            .get(selection.connection_id.as_str())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Provider tool connection `{}` no longer exists.",
                    selection.connection_id
                )
            })?;
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            return Err(format!(
                "Provider tool connection `{}` is pending explicit model validation.",
                connection.id
            ));
        }
        let provider = registry
            .provider(connection.provider_id.as_str())
            .cloned()
            .ok_or_else(|| format!("Provider `{}` is not registered.", connection.provider_id))?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Provider tool service entry `{}` no longer exists.",
                    connection.service_entry_id
                )
            })?;
        let manifest = generation
            .manifests
            .manifest_arc(provider.id.as_str())
            .ok_or_else(|| {
                format!(
                    "Provider tool manifest for `{}` is not registered.",
                    provider.id
                )
            })?;
        let target = manifest
            .tool_targets
            .iter()
            .cloned()
            .into_iter()
            .find(|target| target.target_id == selection.target_id)
            .ok_or_else(|| {
                format!(
                    "Provider tool target `{}` is not registered.",
                    selection.target_id
                )
            })?;
        // Provider-tool execution revalidates the same account-filtered credential membership used
        // by settings projection. Conversation publication is a separate policy boundary.
        // Provider Tool 执行重新校验设置投影使用的同一账号权限过滤后凭据成员关系；会话发布属于
        // 独立策略边界。
        if !provider_tool_target_availability_catalog_matches(
            &target,
            generation
                .activated_effective_catalogs
                .get(connection.id.as_str()),
        ) {
            return Err(format!(
                "Provider tool target `{}` is unavailable for the current credential model catalog.",
                target.target_id
            ));
        }
        let target = crate::connection_endpoint::apply_connection_provider_tool_origin(
            &connection,
            &entry,
            target,
        );
        if target.contract.capability != capability
            || target.service_entry_id != entry.id
            || !provider_tool_target_allows_credential(
                &target,
                connection.credential_profile_id.as_str(),
            )
        {
            return Err(format!(
                "Provider tool target `{}` is not eligible for connection `{}`.",
                target.target_id, connection.id
            ));
        }
        let mut matching_operations = manifest.operations.iter().filter(|operation| {
            operation
                .tool_target_refs
                .iter()
                .any(|reference| reference == &target.target_id)
        });
        let operation = matching_operations.next().cloned();
        if let Some(duplicate) = matching_operations.next() {
            return Err(format!(
                "Provider tool target `{}` is referenced by ambiguous operations including `{}`.",
                target.target_id, duplicate.operation_id
            ));
        }
        let request_profiles = manifest.request_profiles_for_operation(operation.as_ref())?;
        let response_profiles = manifest.response_profiles_for_operation(operation.as_ref())?;
        // manifestGeneration is frozen from the same atomic catalog generation as the target.
        // manifestGeneration 从与目标相同的原子目录代际中冻结。
        let manifest_generation = generation.manifests.generation;
        // routeRevision binds the catalog content, connection, and target without credential data.
        // routeRevision 在不包含凭据数据的前提下绑定目录内容、连接与目标。
        let route_revision = format!(
            "{}:{}:{}",
            generation.manifests.content_sha256, connection.id, target.target_id
        );
        Ok(ResolvedProviderToolTarget {
            connection,
            entry,
            target,
            operation,
            request_profiles,
            response_profiles,
            upload_channels: manifest.upload_channels.clone(),
            manifest_revision: manifest.revision,
            manifest_generation,
            route_revision,
        })
    }

    /// Resolves immutable provider metadata for one configured connection.
    /// 为一个已配置连接解析不可变供应商元数据。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns the connection graph and `connection_id` is exact.
    /// `configuration` 持有连接图，`connection_id` 为精确标识。
    ///
    /// # Returns
    /// 返回值
    /// The provider, service entry, and connection snapshots.
    /// 返回供应商、服务入口和连接快照。
    fn resolve_connection_metadata(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
    ) -> Result<
        (
            crate::ProviderDescriptor,
            crate::ServiceEntry,
            CredentialConnection,
        ),
        String,
    > {
        let generation = self.selection_catalog_snapshot()?;
        self.resolve_connection_metadata_from_generation(configuration, connection_id, &generation)
    }

    /// Resolves connection metadata from one already captured provider generation.
    /// 从一个已捕获的供应商代际解析连接元数据。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns the connection graph; `connection_id` identifies the connection;
    /// `generation` is the immutable published manifest snapshot used for every lookup.
    /// `configuration` 持有连接图；`connection_id` 标识连接；`generation` 是所有查找共同使用的
    /// 不可变已发布 Manifest 快照。
    ///
    /// # Returns
    /// 返回值
    /// The provider, entry, and connection resolved from one generation, or a precise error.
    /// 返回从同一代际解析的供应商、入口与连接，或精确错误。
    fn resolve_connection_metadata_from_generation(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        generation: &Arc<ResolvedProviderGeneration>,
    ) -> Result<
        (
            crate::ProviderDescriptor,
            crate::ServiceEntry,
            CredentialConnection,
        ),
        String,
    > {
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .cloned()
            .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
        let system = ProviderRegistry::from_catalog(generation.manifests.as_ref())?;
        let providers =
            ProviderRegistry::effective_from_system(system, &configuration.custom_providers)?;
        let provider = providers
            .provider(connection.provider_id.as_str())
            .cloned()
            .ok_or_else(|| format!("Provider `{}` is not registered.", connection.provider_id))?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` is not registered for provider `{}`.",
                    connection.service_entry_id, provider.id
                )
            })?;
        Ok((provider, entry, connection))
    }

    /// Resolves one immutable execution route, model, secret, and local overrides.
    /// 解析一条不可变执行路由、模型、秘密与本地覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the authoritative persisted AI configuration.
    /// `configuration` 是权威持久化 AI 配置。
    ///
    /// # Returns
    /// 返回值
    /// The exact execution inputs or a pre-dispatch error.
    /// 返回精确执行输入或分派前错误。
    pub async fn resolve_active_execution(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<ResolvedPublicAiExecution, String> {
        let selection = self.resolve_active_selection(configuration)?;
        self.resolve_execution_from_selection(selection).await
    }

    /// Resolves immutable execution inputs for one requested provider and model.
    /// 为一个请求指定的供应商与模型解析不可变执行输入。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `configuration` is the authoritative public AI configuration; `provider_id` and
    /// `model_id` identify the request-scoped model selection.
    /// `configuration` 是权威公共 AI 配置；`provider_id` 与 `model_id` 标识请求级模型选择。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The exact execution route, credential, model, and local overrides.
    /// 返回精确执行路由、凭据、模型与本地覆盖。
    pub async fn resolve_model_execution(
        &self,
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiExecution, String> {
        let selection = self.resolve_model_selection(configuration, provider_id, model_id)?;
        self.resolve_execution_from_selection(selection).await
    }

    /// Resolves complete execution inputs for one exact persisted connection and model.
    /// 为一个精确持久化连接与模型解析完整执行输入。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns the authoritative connection graph; the remaining identifiers are
    /// the exact selection frozen by the submitting session.
    /// `configuration` 持有权威连接图；其余标识是提交会话冻结的精确选择。
    ///
    /// # Returns
    /// 返回值
    /// The route, model, secret, and local overrides for that exact account.
    /// 该精确账号的路由、模型、秘密与本地覆盖。
    pub async fn resolve_connection_model_execution(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiExecution, String> {
        let selection = self.resolve_connection_model_selection(
            configuration,
            connection_id,
            provider_id,
            model_id,
        )?;
        self.resolve_execution_from_selection(selection).await
    }

    /// Attaches credential material to one already validated public selection.
    /// 为一个已经校验的公共选择附加凭据材料。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `selection` is the immutable route and model selection resolved from current state.
    /// `selection` 是从当前状态解析得到的不可变路由与模型选择。
    ///
    /// # Returns
    /// 返回值
    ///
    /// Complete execution inputs, or a controlled credential-storage error.
    /// 返回完整执行输入，或受控的凭据存储错误。
    async fn resolve_execution_from_selection(
        &self,
        selection: ResolvedPublicAiSelection,
    ) -> Result<ResolvedPublicAiExecution, String> {
        // credential freezes account identity while keeping the access token live per physical request.
        // credential 冻结账号身份，同时让 Access Token 在每次真实请求前保持实时。
        let credential = self.execution_credential_lease(
            selection.route.connection.id.as_str(),
            selection.route.connection.secret_reference.as_str(),
        );
        // The initial resolution preserves the pre-dispatch credential failure boundary without freezing this value for execution.
        // 初始解析保留分派前凭据失败边界，但不会把该值冻结给后续执行。
        credential.resolve_for_request().await?;
        Ok(ResolvedPublicAiExecution {
            route: selection.route,
            model: selection.model,
            credential,
            overrides: selection.overrides,
            protocol_binding: selection.protocol_binding,
            client: self.execution_client.clone(),
        })
    }

    /// Builds one execution from a fully frozen selection by re-reading only the secret.
    /// 通过仅重读秘密，从一个完全冻结的选择构建执行对象。
    ///
    /// This is the controlled execution entry for prepared turns: it never re-resolves the
    /// active catalog, never re-selects a protocol, and never rebuilds overlays. The frozen
    /// route, model, overrides, and binding are used verbatim; only the current secret for
    /// the frozen `secret_reference` is read (plan §6.3/§R4.3).
    /// 这是已准备回合的受控执行入口：绝不重新解析活动目录、绝不重新选择协议、绝不重建
    /// 覆盖。冻结的路由、模型、覆盖与绑定原样使用；只为冻结的 `secret_reference` 读取
    /// 当前秘密（方案 §6.3/§R4.3）。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the non-secret frozen selection captured at the activation boundary.
    /// `selection` 是在激活边界捕获的非秘密冻结选择。
    ///
    /// # Returns
    /// 返回值
    /// Complete execution inputs, or a controlled credential-storage error that never
    /// re-selects another account or protocol.
    /// 返回完整执行输入，或绝不改选其他账号或协议的受控凭据存储错误。
    pub async fn resolve_execution_from_frozen_selection(
        &self,
        selection: ResolvedPublicAiSelection,
    ) -> Result<ResolvedPublicAiExecution, String> {
        self.resolve_execution_from_selection(selection).await
    }

    /// Resolves one access secret after coordinating any required OAuth refresh.
    /// 在协调完成任何所需 OAuth 刷新后解析一个访问秘密。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` scopes the process-local single-flight lock and `secret_reference`
    /// identifies the exact operating-system credential entry.
    /// `connection_id` 确定进程内单飞锁作用域，`secret_reference`
    /// 标识精确的操作系统凭据条目。
    ///
    /// # Returns
    /// 返回值
    /// An immutable execution secret from the current or atomically refreshed revision.
    /// 返回来自当前修订版或原子刷新修订版的不可变执行秘密。
    async fn resolve_fresh_execution_secret(
        &self,
        connection_id: &str,
        secret_reference: &str,
    ) -> Result<CredentialExecutionSecret, String> {
        self.execution_credential_lease(connection_id, secret_reference)
            .resolve_for_request()
            .await
    }

    /// Creates one live credential lease for an exact frozen connection and secret reference.
    /// 为一个精确冻结连接与秘密引用创建实时凭据租约。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `connection_id` scopes refresh serialization and `secret_reference` fixes account identity.
    /// `connection_id` 限定刷新串行化范围，`secret_reference` 固定账号身份。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A lease that can observe later revisions without changing account or route.
    /// 返回能够观察后续修订版且不会改变账号或路由的租约。
    fn execution_credential_lease(
        &self,
        connection_id: &str,
        secret_reference: &str,
    ) -> ProviderExecutionCredentialLease {
        ProviderExecutionCredentialLease::new(
            connection_id.to_string(),
            secret_reference.to_string(),
            Arc::clone(&self.oauth),
            Arc::clone(&self.refresh_locks),
            Arc::clone(&self.secrets),
        )
    }

    /// Resolves public model and route metadata without reading secret material.
    /// 在不读取秘密材料的情况下解析公共模型与路由元数据。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` is the authoritative persisted AI configuration.
    /// `configuration` 是权威持久化 AI 配置。
    ///
    /// # Returns
    /// 返回值
    /// The exact route, model, and local overrides needed during preparation.
    /// 返回准备阶段所需的精确路由、模型与本地覆盖。
    pub fn resolve_active_selection(
        &self,
        _configuration: &PersistedAiConfiguration,
    ) -> Result<ResolvedPublicAiSelection, String> {
        // generation is the sole request-boundary authority for active selection.
        // generation 是活动选择在请求边界的唯一权威。
        let generation = self.selection_catalog_snapshot()?;
        // configuration was atomically published with this generation's catalogs and routes.
        // configuration 与本代际目录和路由共同原子发布。
        let configuration = generation.configuration.as_ref();
        let active = configuration
            .active
            .as_ref()
            .ok_or_else(|| String::from("No active public AI model is configured."))?;
        let connection = exact_active_connection(configuration, active)?;
        self.resolve_selection_for_connection(
            configuration,
            connection,
            active.provider_id.as_str(),
            active.model_id.as_str(),
            None,
            Arc::clone(&generation),
        )
    }

    /// Resolves one request-scoped model through deterministic account preferences.
    /// 通过确定性的账号偏好解析一个请求级模型。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `configuration` owns connections and preferences; `provider_id` and `model_id`
    /// identify the requested public model.
    /// `configuration` 持有连接与偏好；`provider_id` 与 `model_id` 标识请求的公共模型。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A validated immutable selection without secret material.
    /// 返回不含凭据材料且已经校验的不可变选择。
    pub fn resolve_model_selection(
        &self,
        _configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiSelection, String> {
        // One published generation supplies both account eligibility and downstream route
        // resolution. No mutable runtime catalog is re-read after this point.
        // 一个已发布代际同时提供账号资格与下游路由解析；此后不再重新读取可变运行时目录。
        let generation = self.selection_catalog_snapshot()?;
        let configuration = generation.configuration.as_ref();
        let mut eligible_catalogs = generation.activated_effective_catalogs.snapshot();
        eligible_catalogs.retain(|connection_id, _| {
            !configuration.connection_is_pending_validation(connection_id.as_str())
        });
        let eligible_catalogs = RuntimeConnectionCatalogs::from_snapshot(eligible_catalogs);
        let connection = configuration
            .connection_graph
            .resolve_connection(provider_id, model_id, &eligible_catalogs)?
            .clone();
        self.resolve_selection_for_connection(
            configuration,
            connection,
            provider_id,
            model_id,
            None,
            Arc::clone(&generation),
        )
    }

    /// Resolves one exact credential connection and model selected by a persisted session.
    /// 解析持久化会话选择的精确凭据连接与模型。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns connection metadata; `connection_id`, `provider_id`, and `model_id`
    /// identify the exact session selection.
    /// `configuration` 持有连接元数据；`connection_id`、`provider_id` 与 `model_id`
    /// 标识精确的会话选择。
    ///
    /// # Returns
    /// 返回值
    /// A validated immutable selection, or an error that the caller must surface without rerouting.
    /// 返回已校验的不可变选择；错误必须由调用方直接呈现且不得改道。
    pub fn resolve_connection_model_selection(
        &self,
        _configuration: &PersistedAiConfiguration,
        connection_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiSelection, String> {
        // generation freezes the exact configuration and catalog accepted by this session lookup.
        // generation 冻结本次会话查找接受的精确配置与目录。
        let generation = self.selection_catalog_snapshot()?;
        // configuration cannot drift independently from the retained generation.
        // configuration 无法独立于所保留代际发生漂移。
        let configuration = generation.configuration.as_ref();
        // connection is the exact configured credential requested by the session.
        // connection 是会话请求的精确已配置凭据。
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .cloned()
            .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
        self.resolve_selection_for_connection(
            configuration,
            connection,
            provider_id,
            model_id,
            None,
            Arc::clone(&generation),
        )
    }

    /// Resolves one exact configured internal inference route without applying workbench access.
    /// 解析一条精确已配置内部推理路由且不应用工作台访问状态。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` owns the internal enable switch and `inference_route` freezes provider,
    /// product, credential, model, action, and usage scope.
    /// `configuration` 拥有内部启用开关，`inference_route` 冻结供应商、产品、凭据、
    /// 模型、动作与用途范围。
    ///
    /// # Returns
    /// 返回值
    /// An immutable selection whose operation and account usage scope are explicitly authorized.
    /// 返回操作与账号用途范围均已显式授权的不可变选择。
    pub fn resolve_internal_inference_selection(
        &self,
        configuration: &PersistedAiConfiguration,
        inference_route: &InferenceRouteConfiguration,
    ) -> Result<ResolvedPublicAiSelection, String> {
        // generation freezes every internal-inference authorization input.
        // generation 冻结每项内部推理授权输入。
        let generation = self.selection_catalog_snapshot()?;
        // The caller-owned configuration permits validation of one exact pending settings
        // transaction, while model membership and protocol routes remain frozen by generation.
        // 调用方持有的配置允许校验一个精确的待提交设置事务；模型成员关系与协议路由仍由代际冻结。
        let connection = configuration
            .connection_graph
            .connections
            .get(inference_route.connection_id.as_str())
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Internal inference connection `{}` is not configured.",
                    inference_route.connection_id
                )
            })?;
        if connection.provider_id != inference_route.provider_id
            || connection.service_entry_id != inference_route.service_entry_id
        {
            return Err(format!(
                "Internal inference route `{}/{}` does not own connection `{}`.",
                inference_route.provider_id,
                inference_route.service_entry_id,
                inference_route.connection_id
            ));
        }
        let selection = self.resolve_selection_for_connection(
            configuration,
            connection,
            inference_route.provider_id.as_str(),
            inference_route.model_id.as_str(),
            Some(inference_route.operation),
            Arc::clone(&generation),
        )?;
        if inference_route.usage_scope == InferenceUsageScope::InteractiveAgent {
            return Err(format!(
                "Internal inference connection `{}` cannot use the interactive-agent scope.",
                inference_route.connection_id
            ));
        }
        match (&inference_route.binding, inference_route.operation) {
            (
                InferenceRouteBinding::Protocol {
                    protocol_profile_id,
                },
                AiOperation::ConversationRespond,
            ) if protocol_profile_id == &selection.route.protocol_route.protocol_profile_id => {}
            (
                InferenceRouteBinding::Action { action_binding_id },
                AiOperation::EmbeddingCreate | AiOperation::RerankDocuments,
            ) => validate_native_action_binding(
                &selection.route,
                inference_route.operation,
                action_binding_id.as_str(),
            )
            .map_err(|failure| failure.error.message.clone())?,
            _ => {
                return Err(format!(
                    "Internal inference binding does not match operation `{}`.",
                    inference_route.operation.as_str()
                ));
            }
        }
        Ok(selection)
    }

    /// Validates one selected model and protocol with an executable disabled-reasoning probe.
    /// 使用可执行的关闭思考探针校验一个已选模型与协议。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the immutable route, model capability, and protocol snapshot resolved for
    /// one configured inference purpose.
    /// `selection` 是为一个已配置推理用途解析出的不可变路由、模型能力与协议快照。
    ///
    /// # Returns
    /// 返回值
    /// Success only when exact request encoding preserves the required no-reasoning contract.
    /// 仅当精确请求编码保留必需的无思考契约时返回成功。
    pub fn validate_disabled_reasoning_route_projection(
        &self,
        selection: &ResolvedPublicAiSelection,
    ) -> Result<(), String> {
        // modelParameters mirrors the VMM host policy by removing only conflicts declared by the
        // frozen manifest before the disabled effort is encoded.
        // modelParameters 通过仅移除冻结 Manifest 声明的冲突项来复现 VMM 宿主策略，然后再编码关闭档位。
        let mut model_parameters = selection.overrides.model_parameters.clone();
        for parameter_id in selection
            .route
            .model_parameters_conflicting_with_request_field("reasoning_effort")
        {
            model_parameters.remove(parameter_id.as_str());
        }
        // probe uses one bounded user message so every registered conversation protocol executes
        // its ordinary production encoder rather than a special configuration-only shortcut.
        // probe 使用一条有界用户消息，使每个已注册对话协议执行普通生产编码器，而不是配置专用捷径。
        let probe = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: selection.model.id.clone(),
            cache_scope_id: None,
            continuation: None,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("VMM disabled-reasoning route validation probe."),
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
            reasoning_effort: Some(ReasoningEffort::None),
            reasoning_output: Default::default(),
            model_parameters,
            resources: Vec::new(),
            output_token_limit: selection
                .model
                .capabilities
                .output_limit
                .map(|limit| limit.min(128))
                .or(Some(128)),
        };
        self.validate_disabled_reasoning_request_projection(selection, &probe)
    }

    /// Validates the final encoded body for one exact required no-reasoning request.
    /// 校验一项精确必需无思考请求的最终编码请求体。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the frozen model route and `request` is the exact provider-neutral request
    /// that will be executed without protocol reselection.
    /// `selection` 是冻结模型路由，`request` 是将不经协议重选直接执行的精确供应商无关请求。
    ///
    /// # Returns
    /// 返回值
    /// Success only when the final post-projection body cannot enable reasoning and switchable
    /// models retain an explicit disable field.
    /// 仅当最终投影请求体不会开启思考，且可切换模型保留显式关闭字段时返回成功。
    pub fn validate_disabled_reasoning_request_projection(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &AiExecutionRequest,
    ) -> Result<(), String> {
        self.execution_client
            .encode_required_disabled_reasoning_request(
                &selection.route,
                request,
                selection.model.capabilities.reasoning_control == ReasoningControlMode::FixedOff,
            )
            .map(|_| ())
            .map_err(|failure| failure.error.message)
    }

    /// Executes one resolved internal embedding selection with fresh credential material.
    /// 使用新鲜凭据材料执行一项已解析内部向量选择。
    ///
    /// # Parameters
    /// 参数
    /// `selection`, `request`, and `usage_scope` are already authorized immutable facts.
    /// `selection`、`request` 与 `usage_scope` 是已授权不可变事实。
    ///
    /// # Returns
    /// 返回值
    /// A normalized complete embedding batch or a structured execution failure.
    /// 返回归一化完整向量批次或结构化执行失败。
    pub async fn execute_internal_embedding(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &EmbeddingRequest,
        usage_scope: InferenceUsageScope,
    ) -> Result<EmbeddingResponse, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .execute_embedding(
                &selection.route,
                &selection.model,
                request,
                usage_scope,
                crate::native_inference::ExecutionRevisionRefs {
                    catalog: selection.catalog_revision.as_str(),
                    route: selection.route_revision.as_str(),
                },
                &secret,
            )
            .await
    }

    /// Executes one resolved internal rerank selection with fresh credential material.
    /// 使用新鲜凭据材料执行一项已解析内部重排选择。
    ///
    /// # Parameters
    /// 参数
    /// `selection`, `request`, and `usage_scope` are already authorized immutable facts.
    /// `selection`、`request` 与 `usage_scope` 是已授权不可变事实。
    ///
    /// # Returns
    /// 返回值
    /// A normalized ranked result or a structured execution failure.
    /// 返回归一化排序结果或结构化执行失败。
    pub async fn execute_internal_rerank(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: &RerankRequest,
        usage_scope: InferenceUsageScope,
    ) -> Result<RerankResponse, ProviderExecutionFailure> {
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        self.execution_client
            .execute_rerank(
                &selection.route,
                &selection.model,
                request,
                usage_scope,
                crate::native_inference::ExecutionRevisionRefs {
                    catalog: selection.catalog_revision.as_str(),
                    route: selection.route_revision.as_str(),
                },
                &secret,
            )
            .await
    }

    /// Prepares and caches one local media input for an exact resolved conversation route.
    /// 为一条精确解析后的会话路由准备并缓存一个本地媒体输入。
    ///
    /// # Parameters
    /// 参数
    /// `selection` freezes the route; `owner_session_id` and `operation_id` route host progress;
    /// `local_path`, `kind`, and `mime_type` identify the source; `progress` and `cancellation`
    /// control the observable transfer lifecycle.
    /// `selection` 冻结路由；`owner_session_id` 与 `operation_id` 路由宿主进度；
    /// `local_path`、`kind` 与 `mime_type` 标识来源；`progress` 与 `cancellation` 控制可观测传输生命周期。
    ///
    /// # Returns
    /// 返回值
    /// The immutable reference plus its exact Provider encoding, or an inline Data URL marker.
    /// 返回不可变引用及其精确供应商编码，或内联 Data URL 标记。
    pub async fn prepare_and_remember_local_media(
        &self,
        request: PrepareLocalMediaRequest<'_>,
    ) -> Result<PreparedLocalMediaReference, String> {
        let PrepareLocalMediaRequest {
            selection,
            owner_session_id,
            operation_id,
            local_path,
            kind,
            mime_type,
            progress,
            cancellation,
        } = request;
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        // uploadContract is resolved for every media kind so future image and audio upload channels
        // use the same cache, progress, cancellation, and route isolation as video.
        // uploadContract 会为每种媒体解析，使未来图片与音频上传通道复用视频相同的缓存、进度、取消和路由隔离。
        let upload_contract = self.execution_client.conversation_media_upload_contract(
            &selection.route,
            kind,
            mime_type,
        )?;
        if kind == ResourceKind::Video && upload_contract.is_none() {
            return Err(format!(
                "Video resource `{}` has no declared provider upload channel; refusing Base64 fallback.",
                local_path.display()
            ));
        }
        // sourceByteLen freezes the policy decision before cache lookup and physical preparation.
        // sourceByteLen 在缓存查询与真实准备前冻结策略判定。
        let source_byte_len = tokio::fs::metadata(local_path)
            .await
            .map_err(|error| {
                format!(
                    "Media-understanding source `{}` could not be inspected: {error}",
                    local_path.display()
                )
            })?
            .len();
        // useProviderUpload keeps small image/audio inputs inline while requiring video uploads and
        // selecting any declared media channel above the shared inline-preference threshold.
        // useProviderUpload 让小型图片/音频保持内联，同时强制视频上传，并在超过共享内联阈值时选择任意已声明媒体通道。
        let use_provider_upload = media_understanding_uses_provider_upload(
            kind,
            upload_contract.is_some(),
            source_byte_len,
        );
        if use_provider_upload {
            let contract = upload_contract.as_ref().ok_or_else(|| {
                String::from("Media-understanding upload policy selected no provider channel.")
            })?;
            if contract.reference_scope == crate::UploadReferenceScope::Session
                && owner_session_id.is_none()
            {
                return Err(format!(
                    "Upload channel `{}` requires a session identity before media preparation.",
                    contract.upload_channel_id
                ));
            }
        }
        // cachedReference proves the exact source bytes already have a live provider reference;
        // cachedReference 证明精确源字节已经拥有仍有效的供应商引用；
        // a cache hit must not create a synthetic upload-progress row.
        // 命中缓存时不得创建伪造的上传进度行。
        if use_provider_upload
            && let Some(cached_reference) = self
                .execution_client
                .prepared_media_reference(
                    &selection.route,
                    local_path,
                    kind,
                    mime_type,
                    owner_session_id,
                    Some(cancellation),
                )
                .await
        {
            return Ok(PreparedLocalMediaReference {
                reference: cached_reference.provider_reference,
                reference_encoding: Some(cached_reference.reference_encoding),
            });
        }
        // observer is cloned before any callback invocation to avoid retaining the registry lock.
        // observer 在任何回调调用前完成克隆，避免占用注册表锁。
        let observer = self
            .runtime_media_upload_progress_observer
            .read()
            .map_err(|_| String::from("The runtime media upload observer lock is poisoned."))?
            .clone();
        // progressContext exists only for a real provider upload; Base64 image and audio
        // materialization must not look like a network transfer in the UI.
        // progressContext 仅在真实供应商上传时存在；图片与音频 Base64 物化不得在界面上伪装成网络传输。
        let progress_context = if use_provider_upload {
            if let (Some(observer), Some(owner_session_id)) = (observer, owner_session_id) {
                // mediaKind is the stable UI-facing category for this provider-neutral resource.
                // mediaKind 是当前供应商中立资源面向界面的稳定类别。
                let media_kind = match kind {
                    ResourceKind::Image => "image",
                    ResourceKind::Video => "video",
                    ResourceKind::Audio => "audio",
                    ResourceKind::File => {
                        return Err(String::from(
                            "Model-replacement media progress does not accept generic files.",
                        ));
                    }
                };
                // uploadChannelId is resolved by the same route function used by media preparation.
                // uploadChannelId 由媒体准备使用的同一路由函数解析。
                let upload_channel_id = upload_contract
                    .as_ref()
                    .map(|contract| contract.upload_channel_id.clone())
                    .ok_or_else(|| {
                        String::from(
                            "Media-understanding upload policy selected no provider channel.",
                        )
                    })?;
                // totalBytes reuses the policy snapshot and is rechecked by physical preparation.
                // totalBytes 复用策略快照，并由真实准备再次校验。
                let total_bytes = source_byte_len;
                // fileName is derived only from the canonical workspace path.
                // fileName 仅从规范工作区路径派生。
                let file_name = local_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        String::from("Media-understanding source has no valid file name.")
                    })?;
                Some(RuntimeMediaUploadProgressContext {
                    observer,
                    base: crate::RuntimeMediaUploadProgressEvent {
                        transfer_id: format!("media-tool:{}", Uuid::new_v4()),
                        owner_session_id: owner_session_id.to_string(),
                        provider_id: selection.route.provider.id.clone(),
                        service_entry_id: selection.route.entry.id.clone(),
                        connection_id: selection.route.connection.id.clone(),
                        model_id: selection.model.id.clone(),
                        protocol_profile_id: selection
                            .route
                            .protocol_route
                            .protocol_profile_id
                            .clone(),
                        manifest_revision: selection.route.manifest_revision,
                        manifest_generation: selection.manifest_generation,
                        route_revision: selection.route_revision.clone(),
                        operation_id: operation_id.to_string(),
                        upload_channel_id,
                        file_name: file_name.to_string(),
                        media_kind: media_kind.to_string(),
                        phase: crate::RuntimeMediaUploadPhase::Preparing,
                        bytes_sent: 0,
                        total_bytes: Some(total_bytes),
                        error: None,
                    },
                    last_bytes_sent: Arc::new(AtomicU64::new(0)),
                })
            } else {
                None
            }
        } else {
            None
        };
        if let Some(context) = progress_context.as_ref() {
            context.emit(
                crate::RuntimeMediaUploadPhase::Preparing,
                0,
                context.base.total_bytes,
                None,
            );
        }
        // progressTerminalGuard closes the UI row if this preparation future is cancelled.
        // progressTerminalGuard 在当前准备 Future 被取消时关闭界面进度行。
        let mut progress_terminal_guard =
            RuntimeMediaUploadTerminalGuard::new(progress_context.as_ref());
        // observedProgress composes a caller callback with the service-owned progress observer.
        // observedProgress 组合调用方回调与服务持有的进度观察器。
        let observed_progress = if use_provider_upload {
            match (progress, progress_context.as_ref()) {
                (Some(progress), Some(context)) => {
                    let observed = context.byte_callback();
                    Some(Arc::new(move |bytes_sent, total_bytes| {
                        progress(bytes_sent, total_bytes);
                        observed(bytes_sent, total_bytes);
                    }) as crate::MediaUploadProgressCallback)
                }
                (Some(progress), None) => Some(progress),
                (None, Some(context)) => Some(context.byte_callback()),
                (None, None) => None,
            }
        } else {
            None
        };
        // preparationResult keeps all terminal errors available for a failed progress event.
        // preparationResult 保留全部终态错误，以便发出失败进度事件。
        let preparation_result = async {
            // secret is the latest credential revision for the already frozen account route.
            // secret 是已冻结账号路由的最新凭据修订版。
            let secret = self
                .resolve_fresh_execution_secret(
                    selection.route.connection.id.as_str(),
                    selection.route.connection.secret_reference.as_str(),
                )
                .await?;
            // prepared binds any native provider reference to the verified local source fingerprint.
            // prepared 将任意供应商原生引用绑定到已验证本地来源指纹。
            self.execution_client
                .prepare_local_media(
                    &selection.route,
                    selection.model.id.as_str(),
                    &secret,
                    local_path,
                    kind,
                    mime_type,
                    true,
                    use_provider_upload,
                    observed_progress,
                    cancellation,
                )
                .await
        }
        .await;
        let prepared = match preparation_result {
            Ok(prepared) => prepared,
            Err(error) => {
                if let Some(context) = progress_context.as_ref() {
                    // terminalPhase distinguishes cooperative cancellation from provider failure.
                    // terminalPhase 区分协作式取消与供应商失败。
                    let terminal_phase = if cancellation.is_cancelled() {
                        crate::RuntimeMediaUploadPhase::Cancelled
                    } else {
                        crate::RuntimeMediaUploadPhase::Failed
                    };
                    context.emit(
                        terminal_phase,
                        context.last_bytes_sent.load(Ordering::Acquire),
                        context.base.total_bytes,
                        (terminal_phase == crate::RuntimeMediaUploadPhase::Failed)
                            .then(|| error.clone()),
                    );
                }
                progress_terminal_guard.disarm();
                return Err(error);
            }
        };
        // preparedReference binds this physical invocation directly to the verified bytes.
        // preparedReference 将本次物理调用直接绑定到已验证字节。
        let prepared_reference = prepared
            .prepared_reference()
            .ok_or_else(|| {
                String::from("Media-understanding preparation did not produce a request reference.")
            })?
            .to_string();
        // referenceEncoding preserves the exact wire channel for uploaded media and distinguishes
        // host-inline Data URLs without guessing from the reference string.
        // referenceEncoding 为已上传媒体保留精确 Wire 通道，并在不猜测引用字符串的情况下区分宿主内联 Data URL。
        let reference_encoding = if use_provider_upload {
            Some(
                upload_contract
                    .as_ref()
                    .ok_or_else(|| {
                        String::from(
                            "Media-understanding upload completed without its frozen contract.",
                        )
                    })?
                    .reference_encoding,
            )
        } else {
            None
        };
        if use_provider_upload {
            self.execution_client.remember_prepared_media(
                &selection.route,
                local_path,
                kind,
                mime_type,
                prepared,
                owner_session_id,
            );
        }
        if let Some(context) = progress_context.as_ref() {
            context.emit(
                crate::RuntimeMediaUploadPhase::Completed,
                context.base.total_bytes.unwrap_or_default(),
                context.base.total_bytes,
                None,
            );
        }
        progress_terminal_guard.disarm();
        Ok(PreparedLocalMediaReference {
            reference: prepared_reference,
            reference_encoding,
        })
    }

    /// Executes one resolved Responses request with fresh credential material.
    /// 使用新鲜凭据材料执行一项已解析的 Responses 请求。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the exact pre-authorized route, while `request` contains the complete
    /// provider-neutral tool and resource semantics for this execution.
    /// `selection` 是精确预授权路由，`request` 包含本次执行完整的供应商无关工具与资源语义。
    ///
    /// # Returns
    /// 返回值
    /// A normalized provider event stream or a structured pre-dispatch failure.
    /// 返回归一化供应商事件流或结构化分派前失败。
    pub async fn execute_internal_llm(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: AiExecutionRequest,
        execution_id: String,
    ) -> Result<ProviderExecutionStream, ProviderExecutionFailure> {
        self.execute_internal_llm_with_wire_request_capture(selection, request, execution_id, None)
            .await
    }

    /// Executes one resolved internal LLM request while optionally capturing the exact secret-safe transport object.
    /// 执行一项已解析内部 LLM 请求，并可选捕获精确且不含秘密的传输对象。
    ///
    /// # Parameters
    /// 参数
    /// `selection`, `request`, and `execution_id` freeze the authorized physical execution;
    /// `wire_request_capture` receives the one object handed to the selected HTTP or WebSocket transport.
    /// `selection`、`request` 与 `execution_id` 冻结已授权的真实执行；
    /// `wire_request_capture` 接收交给所选 HTTP 或 WebSocket 传输的唯一对象。
    ///
    /// # Returns
    /// 返回值
    /// A normalized event stream whose first poll performs the exact dispatch, or a structured pre-dispatch failure.
    /// 返回在首次轮询时执行精确分派的规范化事件流，或结构化分派前失败。
    pub async fn execute_internal_llm_with_wire_request_capture(
        &self,
        selection: &ResolvedPublicAiSelection,
        request: AiExecutionRequest,
        execution_id: String,
        wire_request_capture: Option<tokio::sync::oneshot::Sender<ProviderWireRequestCapture>>,
    ) -> Result<ProviderExecutionStream, ProviderExecutionFailure> {
        if request.operation != AiOperation::ConversationRespond || request.continuation.is_some() {
            return Err(crate::execution::pre_dispatch_failure(
                "Responses execution requires a conversation operation and does not accept host-private continuation metadata.",
            ));
        }
        if request.model_id != selection.model.id {
            return Err(crate::execution::pre_dispatch_failure(format!(
                "Restricted internal LLM request model `{}` does not match resolved model `{}`.",
                request.model_id, selection.model.id
            )));
        }
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        Ok(if let Some(capture) = wire_request_capture {
            self.execution_client.execute_with_wire_request_capture(
                selection.route.clone(),
                request,
                secret,
                execution_id,
                capture,
            )
        } else {
            self.execution_client
                .execute(selection.route.clone(), request, secret, execution_id)
        })
    }

    /// Executes remote Responses compaction only when the exact session route explicitly enables it.
    /// 仅当精确会话路由显式开启远端压缩时执行 Responses 远端压缩。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` and `connection_id` identify the exact session-scoped route; `request` is the
    /// provider-neutral compaction input assembled from the canonical state.
    /// `provider_id` 与 `connection_id` 标识精确会话级路由；`request` 是从规范状态组装出的供应商无关压缩输入。
    ///
    /// # Returns
    /// 返回值
    /// A route-bound opaque compaction result, or a controlled failure that the caller may safely
    /// downgrade to local compaction only before dispatch.
    /// 返回绑定路由的不透明压缩结果；或调用方可仅在分派前安全降级本地压缩的受控失败。
    pub async fn execute_responses_compact(
        &self,
        provider_id: &str,
        connection_id: &str,
        request: AiExecutionRequest,
    ) -> Result<ResponsesCompactExecution, ProviderExecutionFailure> {
        // modelId is retained before the request moves into the prepared dispatch.
        // modelId 在请求移入已准备分派前完成保留。
        let model_id = request.model_id.clone();
        let execution = self
            .prepare_responses_compact_execution(provider_id, connection_id, model_id.as_str())
            .await?;
        self.execute_prepared_responses_compact(execution, request)
            .await
    }

    /// Resolves and validates the exact immutable execution used to project a compact request.
    /// 解析并校验用于投影压缩请求的精确不可变执行对象。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id`, `connection_id`, and `model_id` are the route frozen by the owning session.
    /// `provider_id`、`connection_id` 与 `model_id` 是所属会话冻结的路由。
    ///
    /// # Returns
    /// 返回值
    /// A credential-bearing immutable execution safe to use for both projection and immediate dispatch.
    /// 返回可同时用于投影与立即分派、且携带凭据的不可变执行对象。
    pub async fn prepare_responses_compact_execution(
        &self,
        provider_id: &str,
        connection_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiExecution, ProviderExecutionFailure> {
        // configuration freezes the authoritative graph for the complete prepared execution.
        // configuration 为完整的已准备执行冻结权威连接图。
        let configuration = self
            .configuration_snapshot()
            .map_err(crate::execution::pre_dispatch_failure)?;
        // execution resolves the exact credential once so request projection and dispatch cannot diverge.
        // execution 只解析一次精确凭据，避免请求投影与分派发生分歧。
        let execution = self
            .resolve_connection_model_execution(
                &configuration,
                connection_id,
                provider_id,
                model_id,
            )
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        validate_responses_compact_execution_route(&execution)?;
        Ok(execution)
    }

    /// Dispatches one compact request through the same immutable execution used for projection.
    /// 通过用于投影的同一不可变执行对象分派一次压缩请求。
    ///
    /// # Parameters
    /// 参数
    /// `execution` is prepared by `prepare_responses_compact_execution`; `request` is projected
    /// against that exact route and may retain the current model-visible tool catalog.
    /// `execution` 由 `prepare_responses_compact_execution` 准备；`request` 基于该精确路由投影，
    /// 并可保留当前模型可见工具目录。
    ///
    /// # Returns
    /// 返回值
    /// A route-bound opaque result, or a controlled failure before or after dispatch.
    /// 返回绑定路由的不透明结果，或分派前后产生的受控失败。
    pub async fn execute_prepared_responses_compact(
        &self,
        execution: ResolvedPublicAiExecution,
        request: AiExecutionRequest,
    ) -> Result<ResponsesCompactExecution, ProviderExecutionFailure> {
        if request.operation != AiOperation::ConversationRespond
            || !request.resources.is_empty()
            || request.continuation.is_some()
        {
            return Err(crate::execution::pre_dispatch_failure(
                "Remote Responses compaction rejects resources, continuation, and non-conversation operations.",
            ));
        }
        validate_responses_compact_execution_route(&execution)?;
        if request.model_id != execution.model.id {
            return Err(crate::execution::pre_dispatch_failure(format!(
                "Remote Responses compaction request model `{}` does not match resolved model `{}`.",
                request.model_id, execution.model.id
            )));
        }
        // routeFingerprint is captured before credential consumption for the returned checkpoint identity.
        // routeFingerprint 在消耗凭据前捕获，用于返回的检查点身份。
        let route_fingerprint = execution.route.request_url("/responses");
        // providerId and connectionId retain the exact prepared route after dispatch consumes the secret.
        // providerId 与 connectionId 在分派消耗凭据后仍保留精确准备路由。
        let provider_id = execution.route.provider.id.clone();
        let connection_id = execution.route.connection.id.clone();
        let protocol_profile_id = execution.route.protocol_route.protocol_profile_id.clone();
        let model_id = execution.model.id.clone();
        // secret is resolved immediately before the first physical compaction attempt.
        // secret 在第一次真实压缩尝试之前立即解析。
        let mut secret = execution
            .credential
            .resolve_for_request()
            .await
            .map_err(crate::execution::pre_dispatch_failure)?;
        // oauthReplayUsed limits this unary operation to one safe same-account 401 replay.
        // oauthReplayUsed 将该一元操作限制为一次安全的同账号 401 重放。
        let mut oauth_replay_used = false;
        // result retries only after the exact renewable credential has been refreshed successfully.
        // result 仅在精确可续期凭据成功刷新后才进行重试。
        let result = loop {
            // rejectedRevision binds recovery to the credential sent by this exact physical attempt.
            // rejectedRevision 将恢复绑定到本次真实尝试发送的凭据。
            let rejected_revision = secret.revision();
            match execution
                .client
                .execute_responses_compact(execution.route.clone(), request.clone(), secret.clone())
                .await
            {
                Ok(result) => {
                    execution.credential.mark_request_succeeded();
                    break result;
                }
                Err(mut failure)
                    if !oauth_replay_used
                        && failure.error.http_status == Some(401)
                        && !failure.error.semantic_output_observed
                        && !failure.error.side_effect_observed =>
                {
                    match execution
                        .credential
                        .refresh_after_unauthorized(rejected_revision)
                        .await
                    {
                        Ok(true) => {
                            oauth_replay_used = true;
                            secret = execution
                                .credential
                                .resolve_for_request()
                                .await
                                .map_err(crate::execution::pre_dispatch_failure)?;
                        }
                        Ok(false) => return Err(failure),
                        Err(refresh_error) => {
                            failure.error.message = format!(
                                "{} Host OAuth credential refresh failed after the upstream 401 response: {refresh_error}",
                                failure.error.message
                            );
                            return Err(failure);
                        }
                    }
                }
                Err(failure) => return Err(failure),
            }
        };
        Ok(ResponsesCompactExecution {
            result,
            provider_id,
            connection_id,
            route_fingerprint,
            protocol_profile_id,
            model_id,
        })
    }

    /// Resolves one exact connection and model into an immutable public selection.
    /// 将一个精确连接与模型解析为不可变公共选择。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `configuration` supplies local settings, `connection` is exact, and the provider/model
    /// pair is the validated request identity.
    /// `configuration` 提供本地设置，`connection` 为精确连接，供应商/模型组合为已校验请求身份。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A route, model snapshot, and validated local overrides.
    /// 返回路由、模型快照与已校验本地覆盖。
    fn resolve_selection_for_connection(
        &self,
        configuration: &PersistedAiConfiguration,
        connection: CredentialConnection,
        provider_id: &str,
        model_id: &str,
        required_internal_operation: Option<AiOperation>,
        catalog_snapshot: Arc<ResolvedProviderGeneration>,
    ) -> Result<ResolvedPublicAiSelection, String> {
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            return Err(format!(
                "Connection `{}` is pending explicit model validation and cannot execute normal requests.",
                connection.id
            ));
        }
        // The caller captured this immutable Arc before account selection. Hot updates may
        // publish a newer generation concurrently, but no decision below re-reads mutable
        // manifest, source, or model catalogs (plan F3).
        // 调用方在账号选择前已捕获该不可变 Arc。热更新可并发发布新代际，但下方任何决策
        // 都不会重新读取可变 Manifest、来源目录或模型目录（方案 F3）。
        if required_internal_operation.is_none()
            && !runtime_model_is_enabled_from_manifest(
                configuration,
                provider_id,
                connection.service_entry_id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model_id,
                catalog_snapshot.manifests.manifest(provider_id),
            )
        {
            return Err(format!(
                "Model `{model_id}` is disabled for provider `{provider_id}`."
            ));
        }
        let (model, catalog_revision) = {
            // Selections read only the effective catalog frozen at the last explicit activation
            // boundary; unactivated configuration mutations and mutable catalogs stay invisible.
            // 选择只读取最近一次显式激活边界冻结的有效目录；未激活配置变更与可变目录保持不可见。
            let activated = catalog_snapshot
                .activated_effective_catalogs
                .get(connection.id.as_str())
                .cloned();
            let source_has_connection = catalog_snapshot
                .source_catalogs
                .get(connection.id.as_str())
                .is_some();
            let catalog = match activated {
                Some(catalog) => catalog,
                None if source_has_connection => {
                    return Err(format!(
                        "Connection `{}` has no effective catalog after applying persisted model access and capability overlays.",
                        connection.id
                    ));
                }
                None => {
                    return Err(format!(
                        "No activated catalog exists for connection `{}`.",
                        connection.id
                    ));
                }
            };
            let model = catalog
                .models
                .iter()
                .find(|model| model.id == model_id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Model `{}` is not present in the effective catalog for connection `{}`.",
                        model_id, connection.id
                    )
                })?;
            let catalog_revision = effective_model_catalog_content_revision(&catalog)?;
            (model, catalog_revision)
        };
        let provider = ProviderRegistry::effective_from_system(
            ProviderRegistry::from_catalog(catalog_snapshot.manifests.as_ref())?,
            &configuration.custom_providers,
        )?
        .provider(provider_id)
        .cloned()
        .ok_or_else(|| format!("Provider `{provider_id}` is not registered."))?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` is not registered for provider `{}`.",
                    connection.service_entry_id, provider.id
                )
            })?;
        if required_internal_operation.is_some() {
            let default_internal_enabled = provider.default_internal_use_for_connections(
                configuration
                    .connection_graph
                    .connections
                    .values()
                    .filter(|candidate| {
                        candidate.provider_id == provider_id
                            && !configuration
                                .connection_is_pending_validation(candidate.id.as_str())
                    }),
            )?;
            if !configuration.internal_model_is_enabled(
                provider_id,
                model_id,
                default_internal_enabled,
            ) {
                return Err(format!(
                    "Model `{model_id}` is disabled for non-conversation internal use on provider `{provider_id}`."
                ));
            }
        }
        // Resolve the model-level protocol binding first; the same effective protocol then
        // drives capability projection, overrides, prompt cache, parameters, the execution
        // route, internal-inference checks, and route revision (plan §13.4).
        // 先解析模型级协议绑定；同一生效协议随后驱动能力投影、覆盖、提示词缓存、参数、
        // 执行路由、内部推理校验与路由修订（方案 §13.4）。
        // activeManifest is the only system routing authority for this frozen generation.
        // activeManifest 是当前冻结代际的唯一系统路由权威。
        let active_manifest = catalog_snapshot.manifests.manifest_arc(provider_id);
        let protocol_binding = match provider.kind {
            crate::ProviderKind::System => match &active_manifest {
                Some(manifest) => {
                    // The activation transaction freezes the system binding together with its
                    // model catalog. Request execution reads that immutable fact and verifies its
                    // manifest generation instead of traversing rules again.
                    // 激活事务把系统绑定与模型目录一同冻结。请求执行读取该不可变事实并校验其
                    // Manifest 代际，而不是再次遍历规则。
                    let binding = model.resolved_protocol_binding.clone().ok_or_else(|| {
                        format!(
                            "Model `{model_id}` is not executable: no system protocol binding was frozen at the active catalog boundary."
                        )
                    })?;
                    if binding.source
                        != crate::provider_manifest::ModelProtocolBindingSource::SystemManifest
                        || binding.manifest_revision != manifest.revision
                    {
                        return Err(format!(
                            "Model `{model_id}` frozen binding does not belong to active manifest revision {}.",
                            manifest.revision
                        ));
                    }
                    binding
                }
                None => {
                    return Err(format!(
                        "Provider `{provider_id}` has no active system manifest; model `{model_id}` execution is blocked."
                    ));
                }
            },
            crate::ProviderKind::Custom => {
                // Custom bindings freeze at the catalog activation boundary; execution reads
                // the frozen binding only. A missing binding is a blocked catalog fact and must
                // never trigger request-time Auto recomputation (plan §13.3/§13.6).
                // 自定义绑定在目录激活边界冻结；执行只读取冻结绑定。缺失绑定属于被阻止的目录事实，
                // 绝不能触发请求时 Auto 重算（方案 §13.3/§13.6）。
                let binding = model.resolved_protocol_binding.clone().ok_or_else(|| {
                    format!(
                        "Custom model `{model_id}` has no protocol binding frozen at the active catalog boundary; save a valid model protocol preference and reactivate the provider catalog."
                    )
                })?;
                // A standalone custom Manifest owns the binding revision exactly.
                // 独立自定义 Manifest 精确持有绑定修订。
                // sourceIsCustom rejects system authority from the custom execution branch.
                // sourceIsCustom 拒绝系统权威进入自定义执行分支。
                let source_is_custom = matches!(
                    binding.source,
                    crate::provider_manifest::ModelProtocolBindingSource::CustomUser
                        | crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
                );
                // revisionIsCurrent proves the binding belongs to the same activated contract.
                // revisionIsCurrent 证明绑定属于同一份已激活契约。
                let revision_is_current = active_manifest
                    .as_ref()
                    .is_some_and(|manifest| binding.manifest_revision == manifest.revision);
                if !source_is_custom || !revision_is_current {
                    return Err(format!(
                        "Custom model `{model_id}` frozen binding does not belong to the active custom manifest revision."
                    ));
                }
                binding
            }
        };
        // Project the effective binding's per-protocol variant onto the effective capability
        // fields; the top-level projection is never an authority for guessing the protocol.
        // 将生效绑定的按协议变体投影到有效能力字段；顶层投影绝不用于猜测协议。
        let mut model = model;
        if let Some(variant) = model
            .protocol_variants
            .get(&protocol_binding.protocol_profile_id)
        {
            model.capabilities = variant.capabilities.clone();
            model.capability_evidence = variant.capability_evidence.clone();
        } else {
            return Err(format!(
                "Model `{model_id}` has no capability variant for its bound protocol `{}`; execution is blocked.",
                protocol_binding.protocol_profile_id
            ));
        }
        // The protocol variant is the per-route capability authority, but persisted user
        // capability overrides are the highest layer and must win over the projection
        // (plan §R5.3); the overlay rebuild folds them into the catalog, so re-applying
        // them here preserves the exact persisted overrides after the variant swap.
        // 协议变体是按路由能力权威，但持久化用户能力覆盖是最高层，必须胜过投影
        // （方案 §R5.3）；覆盖重建已把覆盖并入目录，因此在此重新应用以在变体交换后
        // 保留精确持久化覆盖。
        if provider.kind == crate::ProviderKind::System {
            configuration.apply_model_capability_overrides(
                provider_id,
                model.id.as_str(),
                &mut model.capabilities,
            );
        }
        // Capability admission must consume the effective binding projection. Checking the raw
        // catalog compatibility fields before binding resolution can authorize or reject the
        // model with evidence from a different protocol (plan §13.1/§13.4).
        // 能力准入必须消费生效绑定投影。在绑定解析前检查原始目录兼容字段，可能会使用另一协议的
        // 证据错误放行或拒绝模型（方案 §13.1/§13.4）。
        if let Some(operation) = required_internal_operation
            && !model.capabilities.operations.contains(&operation)
        {
            return Err(format!(
                "Model `{model_id}` does not expose internal operation `{}`.",
                operation.as_str()
            ));
        }
        if required_internal_operation.is_none()
            && !model_supports_text_conversation(&model.capabilities)
        {
            return Err(format!(
                "Model `{model_id}` does not expose bidirectional text conversation."
            ));
        }
        // Every executable model owns one active model-level protocol binding.
        // 每个可执行模型都持有一项活动的模型级协议绑定。
        let effective_protocol_id = protocol_binding.protocol_profile_id.as_str();
        let override_key = PersistedAiConfiguration::provider_model_protocol_key(
            connection.provider_id.as_str(),
            model.id.as_str(),
            effective_protocol_id,
        );
        let mut overrides = configuration
            .model_overrides
            .get(override_key.as_str())
            .cloned()
            .unwrap_or_default();
        if overrides.prompt_cache.is_none() {
            overrides.prompt_cache = connection
                .prompt_cache_by_protocol
                .get(effective_protocol_id)
                .cloned();
        }
        apply_model_parameter_defaults(&model, effective_protocol_id, &mut overrides)?;
        validate_model_overrides(&model, effective_protocol_id, &overrides)?;
        let secondary_route_key = PersistedAiConfiguration::connection_model_key(
            connection.id.as_str(),
            model.id.as_str(),
        );
        let secondary_route = configuration
            .secondary_routes
            .get(secondary_route_key.as_str())
            .cloned()
            .unwrap_or(SecondaryRouteSelection::Automatic);
        let mut route = ProviderExecutionRoute::resolve(
            provider,
            entry.clone(),
            connection.clone(),
            secondary_route,
            effective_protocol_id,
        )
        .map_err(|failure| failure.error.message.clone())?;
        // Custom and native models share one merged projection chain instead of replacing an inherited layer.
        // 自定义模型与系统模型共用一条合并投影链，而不是替换掉已继承的上级层。
        route.custom_model_projection =
            resolve_request_projection(configuration, provider_id, model_id)?;
        // toolSchemaProjection is resolved only from the active system Manifest or the exact
        // custom-model advanced setting. Protocol and model-name heuristics are intentionally absent.
        // toolSchemaProjection 只从活动系统 Manifest 或精确自定义模型高级设置解析。
        // 此处刻意不存在协议或模型名猜测。
        route.tool_schema_projection = match route.provider.kind {
            crate::ProviderKind::System => {
                let manifest = active_manifest.as_ref().ok_or_else(|| {
                    format!(
                        "Provider `{provider_id}` has no active system manifest for tool-schema projection."
                    )
                })?;
                manifest
                    .rule_for_model(
                        entry.id.as_str(),
                        Some(connection.credential_profile_id.as_str()),
                        model_id,
                    )
                    .map(|rule| rule.tool_schema_projection)
                    .unwrap_or(
                        manifest
                            .model_defaults_for_entry(entry.id.as_str())?
                            .tool_schema_projection,
                    )
            }
            crate::ProviderKind::Custom => configuration
                .custom_model(provider_id, model_id)
                .map(|definition| definition.tool_schema_projection)
                .unwrap_or_default(),
        };
        // A matching model rule's inherited profile refs replace the route default chain for
        // this exact model; every other model keeps the route default (plan §R8.4/§R8.5).
        // 命中模型规则的继承 Profile 引用替换该精确模型的路由默认链；其他模型保持
        // 路由默认（方案 §R8.4/§R8.5）。
        if let Some(manifest) = active_manifest.as_ref() {
            route.upload_channels = manifest.upload_channels.clone();
            route.protocol_route.input_resource_constraints = resolve_input_resource_constraints(
                manifest.as_ref(),
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model_id,
                route.protocol_route.input_resource_constraints.clone(),
            );
            // The exact selected profile is required when an explicit empty model override
            // restores the standard protocol usage baseline.
            // 当显式空模型覆盖恢复标准协议 usage 基线时，需要精确的已选 Profile。
            let usage_protocol_profile_id = route.protocol_route.protocol_profile_id.clone();
            route.protocol_route.usage_extraction_profiles = resolve_usage_extraction_chain(
                manifest.as_ref(),
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model_id,
                usage_protocol_profile_id.as_str(),
                route.protocol_route.usage_extraction_profiles.clone(),
            )?;
            route.request_projection_profiles = resolve_request_projection_chain(
                manifest.as_ref(),
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model_id,
                route.protocol_route.request_projection_profiles.clone(),
            )?;
            route.response_projection_profiles = resolve_response_projection_chain(
                manifest.as_ref(),
                entry.id.as_str(),
                Some(connection.credential_profile_id.as_str()),
                model_id,
                route.protocol_route.response_projection_profiles.clone(),
            )?;
        }
        let route_revision = provider_execution_route_revision(&route, &model, &overrides)?;
        // The execution route carries the same immutable identity used by media-cache isolation.
        // 执行路由携带与媒体缓存隔离相同的不可变身份。
        route.manifest_revision = protocol_binding.manifest_revision;
        route.manifest_generation = catalog_snapshot.manifests.generation;
        route.route_revision = route_revision.clone();
        Ok(ResolvedPublicAiSelection {
            route,
            model,
            overrides,
            catalog_revision,
            manifest_generation: catalog_snapshot.manifests.generation,
            route_revision,
            protocol_binding,
        })
    }
}

/// Reports whether one target's optional model-catalog gate is satisfied by the same credential.
/// 判断一个目标的可选模型目录门控是否由同一凭据满足。
fn provider_tool_target_availability_catalog_matches(
    target: &ProviderToolTarget,
    catalog: Option<&ConnectionModelCatalog>,
) -> bool {
    if target.availability_model_ids_any.is_empty() {
        return true;
    }
    let Some(catalog) = catalog else {
        return false;
    };
    catalog.models.iter().any(|model| {
        target
            .availability_model_ids_any
            .contains(model.id.as_str())
    })
}

/// Borrowed profile references effective for one exact model binding scope.
/// 一个精确模型绑定作用域内生效的 Profile 引用借用视图。
struct EffectiveModelProfileRefs<'a> {
    /// Ordered usage extraction profile identifiers.
    /// 有序用量提取 Profile 标识。
    usage: &'a [String],
    /// Whether the model/default layer replaces the selected protocol route usage chain.
    /// 模型/默认层是否替换所选协议路由的 usage 链。
    usage_overrides_route_default: bool,
    /// Ordered request projection profile identifiers.
    /// 有序请求投影 Profile 标识。
    request: &'a [String],
    /// Whether the model/default layer replaces the selected protocol route request chain.
    /// 模型/默认层是否替换所选协议路由的请求链。
    request_overrides_route_default: bool,
    /// Ordered response and error projection profile identifiers.
    /// 有序响应与错误投影 Profile 标识。
    response: &'a [String],
    /// Whether the model/default layer replaces the selected protocol route response chain.
    /// 模型/默认层是否替换所选协议路由的响应链。
    response_overrides_route_default: bool,
}

/// Reports whether one resolved model rule explicitly replaces a route-level profile chain.
/// 报告一条已解析模型规则是否显式替换路由级 Profile 链。
///
/// # Parameters
/// 参数
/// `instance`, `field`, and `references` identify the resolved field and its frozen provenance.
/// `instance`、`field` 与 `references` 标识已解析字段及其冻结来源。
///
/// # Returns
/// 返回值
/// `true` for a non-empty inherited model default or any exact/prefix rule declaration,
/// including an explicit empty list.
/// 对非空继承模型默认或任意 Exact/Prefix 规则声明（包括显式空列表）返回 `true`。
fn model_rule_replaces_route_profile_chain(
    instance: &crate::provider_manifest::ResolvedRuleInstance,
    field: &str,
    references: &[String],
) -> bool {
    !references.is_empty()
        || instance.field_provenance.get(field).is_some_and(|source| {
            matches!(
                source.source,
                crate::provider_manifest::FieldSourceKind::ModelRuleExact
                    | crate::provider_manifest::FieldSourceKind::ModelRulePrefix
            )
        })
}

/// Resolves model-rule or entry-default profile references from one immutable manifest snapshot.
/// 从一份不可变 Manifest 快照解析模型规则或入口默认 Profile 引用。
///
/// # Parameters
/// 参数
/// `manifest`, `service_entry_id`, `credential_profile_id`, and `model_id` identify the exact
/// signed binding scope.
/// `manifest`、`service_entry_id`、`credential_profile_id` 与 `model_id` 标识精确签名绑定作用域。
///
/// # Returns
/// 返回值
/// The winning rule's inherited references, the explicit unknown-model defaults, or empty
/// references when an unmatched model has no default-binding authority.
/// 返回胜出规则继承的引用、显式未知模型默认引用，或在未命中模型无默认绑定权限时返回空引用。
fn effective_model_profile_refs<'a>(
    manifest: &'a crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
) -> Result<EffectiveModelProfileRefs<'a>, String> {
    if let Some(instance) =
        manifest.rule_for_model(service_entry_id, credential_profile_id, model_id)
    {
        return Ok(EffectiveModelProfileRefs {
            usage: instance.usage_profile_refs.as_slice(),
            usage_overrides_route_default: model_rule_replaces_route_profile_chain(
                instance,
                "usage_profile_refs",
                instance.usage_profile_refs.as_slice(),
            ),
            request: instance.request_profile_refs.as_slice(),
            request_overrides_route_default: model_rule_replaces_route_profile_chain(
                instance,
                "request_profile_refs",
                instance.request_profile_refs.as_slice(),
            ),
            response: instance.response_profile_refs.as_slice(),
            response_overrides_route_default: model_rule_replaces_route_profile_chain(
                instance,
                "response_profile_refs",
                instance.response_profile_refs.as_slice(),
            ),
        });
    }
    if manifest.unknown_model_policy
        != crate::provider_manifest::UnknownModelPolicy::DefaultBindingAllowed
    {
        return Ok(EffectiveModelProfileRefs {
            usage: &[],
            usage_overrides_route_default: false,
            request: &[],
            request_overrides_route_default: false,
            response: &[],
            response_overrides_route_default: false,
        });
    }
    let defaults = manifest.model_defaults_for_entry(service_entry_id)?;
    Ok(EffectiveModelProfileRefs {
        usage: defaults.usage_profile_refs.as_slice(),
        usage_overrides_route_default: !defaults.usage_profile_refs.is_empty(),
        request: defaults.request_profile_refs.as_slice(),
        request_overrides_route_default: !defaults.request_profile_refs.is_empty(),
        response: defaults.response_profile_refs.as_slice(),
        response_overrides_route_default: !defaults.response_profile_refs.is_empty(),
    })
}

/// Resolves provider-wide route constraints with model-specific narrowing overrides.
/// 使用模型专属收紧覆盖解析供应商通用路由约束。
///
/// # Parameters
/// 参数
/// `manifest`, entry, credential, and model identity select the exact rule; `route_default`
/// is the selected protocol route's immutable provider-wide baseline.
/// `manifest`、入口、凭据与模型身份选择精确规则；`route_default` 是所选协议路由不可变的
/// 供应商通用基线。
///
/// # Returns
/// 返回值
/// A deterministic per-kind merge frozen into the execution route.
/// 返回冻结到执行路由的按资源类别确定性合并结果。
fn resolve_input_resource_constraints(
    manifest: &crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    mut route_default: Vec<crate::InputResourceConstraint>,
) -> Vec<crate::InputResourceConstraint> {
    let Some(instance) = manifest.rule_for_model(service_entry_id, credential_profile_id, model_id)
    else {
        return route_default;
    };
    for constraint in &instance.input_resource_constraints {
        if let Some(existing) = route_default
            .iter_mut()
            .find(|existing| existing.resource_kind == constraint.resource_kind)
        {
            let mut resolved = constraint.clone();
            if resolved.upload_channel_refs.is_empty() {
                resolved.upload_channel_refs = existing.upload_channel_refs.clone();
            }
            *existing = resolved;
        } else {
            route_default.push(constraint.clone());
        }
    }
    route_default
}

/// Resolves the effective usage-extraction profile chain for one exact model.
/// 解析一个精确模型的生效用量提取 Profile 链。
///
/// # Parameters
/// 参数
/// `manifest`, entry, credential, and model identity select the exact signed references;
/// `protocol_profile_id` identifies the selected standard baseline and `route_default` is the
/// selected protocol route's immutable provider-specific baseline.
/// `manifest`、入口、凭据与模型身份选择精确签名引用；`route_default` 是所选协议路由的
/// 不可变供应商专属基线，`protocol_profile_id` 标识所选标准协议基线。
///
/// # Returns
/// 返回值
/// The route baseline when no model-level references exist, or the fully resolved replacement
/// chain. Any missing compiled payload fails closed.
/// 无模型级引用时返回路由基线，否则返回完整解析后的替换链；任何缺失的编译载荷均失败关闭。
fn resolve_usage_extraction_chain(
    manifest: &crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    protocol_profile_id: &str,
    route_default: Vec<vulcan_llm_core::UsageExtractionProfile>,
) -> Result<Vec<vulcan_llm_core::UsageExtractionProfile>, String> {
    let refs =
        effective_model_profile_refs(manifest, service_entry_id, credential_profile_id, model_id)?;
    if !refs.usage_overrides_route_default {
        return Ok(route_default);
    }
    if refs.usage.is_empty() {
        let protocol = crate::execution::upstream_protocol_for_profile_id(protocol_profile_id)
            .ok_or_else(|| {
                format!(
                    "Model `{model_id}` clears route usage profiles but protocol profile `{protocol_profile_id}` has no registered standard usage baseline."
                )
            })?;
        return Ok(vulcan_llm_core::standard_usage_profiles(protocol));
    }
    refs.usage
        .iter()
        .map(|profile_id| {
            manifest
                .usage_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.clone())
                .ok_or_else(|| {
                    format!(
                        "Model `{model_id}` references missing compiled usage profile `{profile_id}`."
                    )
                })
        })
        .collect()
}

/// Resolves the effective request-projection profile chain for one exact model.
/// 解析一个精确模型的生效请求投影 Profile 链。
///
/// The route default chain is the base; a matching model rule whose inherited
/// `request_profile_refs` are non-empty replaces it with the resolved payloads, so a
/// model-level profile overrides the provider default without affecting other models
/// (plan §R8.4/§R8.5).
/// 路由默认链为基座；命中且继承 `request_profile_refs` 非空的模型规则以解析后的载荷
/// 替换基座，使模型级 Profile 覆盖供应商默认而不影响其他模型（方案 §R8.4/§R8.5）。
fn resolve_request_projection_chain(
    manifest: &crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    route_default: Vec<crate::provider_manifest::RequestProjectionProfile>,
) -> Result<Vec<crate::provider_manifest::RequestProjectionProfile>, String> {
    let refs =
        effective_model_profile_refs(manifest, service_entry_id, credential_profile_id, model_id)?;
    if !refs.request_overrides_route_default {
        return Ok(route_default);
    }
    refs.request
        .iter()
        .map(|profile_id| {
            manifest
                .request_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.clone())
                .ok_or_else(|| {
                    format!(
                        "Model `{model_id}` references missing compiled request profile `{profile_id}`."
                    )
                })
        })
        .collect()
}

/// Resolves the effective response-projection profile chain for one exact model.
/// 解析一个精确模型的生效响应投影 Profile 链。
///
/// Semantics mirror the request chain (plan §R8.4/§R8.5).
/// 语义与请求链一致（方案 §R8.4/§R8.5）。
fn resolve_response_projection_chain(
    manifest: &crate::provider_manifest::ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
    route_default: Vec<crate::provider_manifest::ResponseProjectionProfile>,
) -> Result<Vec<crate::provider_manifest::ResponseProjectionProfile>, String> {
    let refs =
        effective_model_profile_refs(manifest, service_entry_id, credential_profile_id, model_id)?;
    if !refs.response_overrides_route_default {
        return Ok(route_default);
    }
    refs.response
        .iter()
        .map(|profile_id| {
            manifest
                .response_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.clone())
                .ok_or_else(|| {
                    format!(
                        "Model `{model_id}` references missing compiled response profile `{profile_id}`."
                    )
                })
        })
        .collect()
}

/// Resolves provider and model request projections from low to high precedence.
/// 按从低到高的优先级解析供应商与模型请求投影。
///
/// # Parameters
/// 参数
/// `configuration` owns sparse layers, while `provider_id` and `model_id` identify the route.
/// `configuration` 持有稀疏配置层，`provider_id` 与 `model_id` 标识目标路由。
///
/// # Returns
/// 返回值
/// A merged projection, or `None` when no layer declares one.
/// 返回合并后的投影；没有任何层声明投影时返回 `None`。
fn resolve_request_projection(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
) -> Result<Option<crate::CustomModelRequestProjection>, String> {
    let mut merged = crate::CustomModelRequestProjection::default();
    let mut found = false;

    // Custom-provider defaults and generic provider overrides are alternative provider layers.
    // 自定义供应商默认与通用供应商覆盖是两种互斥的供应商层来源。
    if provider_id.starts_with("custom:") {
        if let Some(projection) = configuration
            .custom_provider_model_defaults
            .get(provider_id)
            .and_then(|defaults| defaults.request_projection.as_ref())
        {
            merged.merge_overlay(projection)?;
            found = true;
        }
    } else if let Some(projection) = configuration
        .provider_capability_overrides(provider_id)
        .and_then(|overrides| overrides.request_projection.as_ref())
    {
        merged.merge_overlay(projection)?;
        found = true;
    }
    if let Some(projection) = configuration
        .model_capability_overrides(provider_id, model_id)
        .and_then(|overrides| overrides.request_projection.as_ref())
    {
        merged.merge_overlay(projection)?;
        found = true;
    }
    if let Some(definition) = configuration.custom_model(provider_id, model_id) {
        merged.merge_overlay(&definition.request_projection)?;
        found = true;
    }
    Ok(found.then_some(merged))
}

/// Builds an empty discovery baseline only when one provider owns manual model definitions.
/// 仅在某个供应商拥有手工模型定义时构建空的发现基线。
///
/// # Parameters
/// 参数
/// `configuration` owns custom definitions, `connection` identifies the target credential, and
/// `generation` is the process-local catalog generation.
/// `configuration` 持有自定义定义，`connection` 标识目标凭据，`generation` 为进程内目录代次。
///
/// # Returns
/// 返回值
/// A credential-scoped baseline when manual definitions or confirmed template snapshots exist;
/// otherwise `None`.
/// 存在手工定义或已确认模板快照时返回凭据作用域基线，否则返回 `None`。
fn manual_custom_catalog(
    configuration: &PersistedAiConfiguration,
    connection: &CredentialConnection,
    generation: u64,
) -> Option<ConnectionModelCatalog> {
    manual_custom_catalog_for_provider(
        configuration,
        connection.provider_id.as_str(),
        connection.id.as_str(),
        generation,
    )
}

/// Builds a persisted-evidence manual baseline for an exact provider and catalog scope.
/// 为精确供应商与目录作用域构建基于持久化证据的手工基线。
///
/// # Parameters
/// 参数
/// `configuration` owns custom definitions, `provider_id` selects them, `connection_id` labels
/// the isolated catalog, and `generation` is process-local.
/// `configuration` 持有自定义定义，`provider_id` 选择这些定义，`connection_id` 标记隔离目录，
/// `generation` 仅存在于进程内。
///
/// # Returns
/// 返回值
/// A baseline containing confirmed custom-provider template identities when available, an empty
/// baseline for explicit manual definitions, or `None` without either source of persisted evidence.
/// 存在已确认自定义供应商模板身份时返回包含这些身份的基线；仅有显式手工定义时返回空基线；
/// 两类持久化证据均不存在时返回 `None`。
fn manual_custom_catalog_for_provider(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    connection_id: &str,
    generation: u64,
) -> Option<ConnectionModelCatalog> {
    // ownsManualDefinitions records explicit user-authored model declarations that overlays add below.
    // ownsManualDefinitions 记录后续由覆盖层加入的显式用户模型声明。
    let owns_manual_definitions = configuration
        .custom_models
        .get(provider_id)
        .is_some_and(|definitions| !definitions.is_empty());
    // templateModels retains only user-confirmed immutable template snapshots for this exact custom provider.
    // templateModels 仅保留属于该精确自定义供应商、且由用户确认的不可变模板快照。
    let template_models = if provider_id.starts_with("custom:") {
        configuration
            .custom_model_template_decisions
            .iter()
            .filter_map(|(key, decision)| {
                let (decision_provider_id, model_id) =
                    PersistedAiConfiguration::provider_model_key_parts(key.as_str())?;
                if decision_provider_id != provider_id
                    || !matches!(decision, crate::CustomModelTemplateDecision::Applied { .. })
                {
                    return None;
                }
                Some(ConnectionModel {
                    id: model_id.to_string(),
                    display_name: model_id.to_string(),
                    capabilities: ModelCapabilities::default(),
                    capability_evidence: crate::ModelCapabilityEvidence::default(),
                    protocol_variants: BTreeMap::new(),
                    resolved_protocol_binding: None,
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if !owns_manual_definitions && template_models.is_empty() {
        return None;
    }
    Some(ConnectionModelCatalog {
        connection_id: connection_id.to_string(),
        models: template_models,
        generation,
        page_count: 0,
        discovery_schema_revision: String::from(MANUAL_MODEL_CATALOG_SCHEMA_REVISION),
    })
}

/// Applies immutable common defaults, custom definitions, and user differences to one catalog.
/// 将不可变公共默认、自定义定义与用户差异应用到一个目录。
///
/// # Parameters
/// 参数
/// `configuration` owns user data, `provider_id` is exact, and `catalog` is rebuilt in place.
/// `configuration` 持有用户数据，`provider_id` 为精确标识，`catalog` 会被原地重建。
#[cfg(test)]
fn apply_model_catalog_overlays(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    catalog: &mut ConnectionModelCatalog,
) -> Result<(), String> {
    // sourcePolicy is explicit test input derived from the fixture's authoritative source marker.
    // sourcePolicy 是从夹具权威来源标记派生的显式测试输入。
    let source_policy = if catalog.discovery_schema_revision == MANUAL_MODEL_CATALOG_SCHEMA_REVISION
    {
        ModelSourcePolicy::ManualOnly
    } else if provider_id.starts_with("custom:") {
        ModelSourcePolicy::DiscoveryAndManual
    } else {
        ModelSourcePolicy::DiscoveryOnly
    };
    // fixtureConfiguration supplies the explicit descriptor that production callers always own;
    // unit-level overlay tests predating custom descriptors use one reviewed Chat fixture instead
    // of exercising an impossible descriptor-free runtime state.
    // fixtureConfiguration 提供生产调用方始终持有的显式描述；早于自定义描述器的覆盖单元测试
    // 使用一份已审核 Chat 夹具，而不是构造运行时不可能存在的无描述状态。
    let mut fixture_configuration = configuration.clone();
    if provider_id.starts_with("custom:")
        && !fixture_configuration
            .custom_providers
            .contains_key(provider_id)
    {
        let discovery_mode = if source_policy == ModelSourcePolicy::ManualOnly {
            crate::CustomProviderModelDiscoveryMode::Manual
        } else {
            crate::CustomProviderModelDiscoveryMode::Standard
        };
        let descriptor = crate::CustomProviderDefinition {
            id: provider_id.to_string(),
            display_name: String::from("Catalog Overlay Fixture"),
            base_url: String::from("https://fixture.example.test/v1"),
            protocols: vec![crate::CustomProviderProtocol::OpenAiChat],
            authentication: crate::CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: discovery_mode,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()?;
        fixture_configuration
            .custom_providers
            .insert(provider_id.to_string(), descriptor);
    }
    let service_entry_id = if provider_id.starts_with("custom:") {
        resolve_custom_model_service_entry(&fixture_configuration, provider_id, "")?
            .id
            .clone()
    } else {
        String::new()
    };
    apply_model_catalog_overlays_with_account_access(
        &fixture_configuration,
        provider_id,
        service_entry_id.as_str(),
        None,
        source_policy,
        None,
        catalog,
    )
}

/// Applies the host default reasoning contract only to unknown text-conversation models.
/// 仅为能力未知的文本会话模型应用宿主默认思考契约。
///
/// # Parameters
/// 参数
///
/// `capabilities` is the fully merged model capability snapshot before model-level user overrides.
/// `capabilities` 是应用模型级用户覆盖前的完整合并模型能力快照。
///
/// # Returns
/// 返回值
///
/// `true` when the host policy filled an otherwise unknown reasoning contract.
/// 当宿主策略填充了原本未知的思考契约时返回 `true`。
pub fn apply_host_default_reasoning_policy(capabilities: &mut ModelCapabilities) -> bool {
    let is_text_conversation = capabilities
        .operations
        .contains(&AiOperation::ConversationRespond);
    // A text model remains unresolved when it has no selectable levels and no explicit control
    // mode. Unknown or legacy supported markers can use the host default; Unsupported/FixedOff
    // is an authoritative provider or user declaration and must never be silently re-enabled.
    // 文本模型没有可选档位且没有明确控制模式时仍未解析。未知或旧版支持标记可以使用宿主默认；
    // Unsupported/FixedOff 是供应商或用户的权威声明，绝不能被宿主默认静默重新开启。
    let is_unresolved_reasoning = capabilities.reasoning_efforts.is_empty()
        && capabilities.reasoning_control == ReasoningControlMode::Unknown
        && matches!(
            capabilities.reasoning,
            CapabilitySupport::Unknown
                | CapabilitySupport::Native
                | CapabilitySupport::Equivalent
                | CapabilitySupport::Lossy
        );
    if !is_text_conversation || !is_unresolved_reasoning {
        return false;
    }

    // Keep this standard set shared with custom compatible providers so the editor,
    // workbench selector, and execution resolver expose one closed vocabulary.
    // 与自定义兼容供应商共享这组标准档位，确保编辑器、工作台选择器和执行解析器使用同一封闭词表。
    capabilities.reasoning = CapabilitySupport::Native;
    capabilities.reasoning_control = ReasoningControlMode::EffortSelection;
    capabilities.reasoning_efforts = crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS.to_vec();
    capabilities.evidence_revision = format!(
        "{}+{}",
        capabilities.evidence_revision, HOST_DEFAULT_REASONING_EVIDENCE_REVISION
    );
    true
}

/// Resolves the model-level protocol binding for one custom-provider model.
/// 为一个自定义供应商模型解析模型级协议绑定。
///
/// # Parameters
/// 参数
/// `configuration` owns the persisted per-model preferences; `provider` and `entry` supply the
/// enabled protocol set; `model_available_protocol_profile_ids` supplies this model's exact
/// observed or manually declared variants; `model_id` identifies the exact model; and
/// `manifest_revision` identifies the activated custom-provider contract.
/// `configuration` 持有持久化逐模型偏好；`provider` 与 `entry` 提供启用协议集合；
/// `model_available_protocol_profile_ids` 提供当前模型已观测或手工声明的精确变体；
/// `model_id` 标识精确模型；`manifest_revision` 标识已激活自定义供应商契约。
///
/// # Returns
/// 返回值
/// The binding for custom providers, or `None` for providers outside the custom contract.
/// 自定义供应商返回绑定；自定义契约之外的供应商返回 `None`。
fn resolve_custom_provider_binding(
    configuration: &PersistedAiConfiguration,
    provider: &crate::ProviderDescriptor,
    entry: &crate::ServiceEntry,
    model_available_protocol_profile_ids: &[&str],
    model_id: &str,
    manifest_revision: u64,
) -> Result<Option<crate::provider_manifest::ModelProtocolBinding>, String> {
    if provider.kind != crate::ProviderKind::Custom {
        return Ok(None);
    }
    let enabled_protocols = entry
        .protocol_routes
        .iter()
        .map(|route| route.protocol_profile_id.as_str())
        .collect::<Vec<_>>();
    let preference_key =
        PersistedAiConfiguration::provider_model_key(provider.id.as_str(), model_id);
    let explicit_protocol = configuration
        .custom_model_protocol_preferences
        .get(preference_key.as_str())
        .and_then(|preference| preference.explicit_protocol())
        .map(|protocol| protocol.profile_id());
    crate::provider_manifest::resolve_custom_model_protocol_binding(
        enabled_protocols.as_slice(),
        model_available_protocol_profile_ids,
        model_id,
        manifest_revision,
        explicit_protocol,
    )
    .map(Some)
    .map_err(|error| format!("Model `{model_id}` is not executable: {error}"))
}

/// Builds the protocol-aware in-memory capability baseline for one exact protocol variant.
/// 为一个精确协议变体构建协议感知的内存能力基线。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` is the variant protocol; `None` retains the conservative compatible
/// default used before an exact connection is available.
/// `protocol_profile_id` 是变体协议；`None` 保留精确连接可用前使用的保守兼容默认值。
///
/// # Returns
/// 返回值
/// A baseline that preserves Gemini reasoning evidence and otherwise uses the compatible default.
/// 返回一份为 Gemini 保留思考证据、其他协议使用兼容默认值的基线。
fn custom_provider_system_baseline_for_protocol(
    protocol_profile_id: Option<&str>,
) -> crate::CustomProviderModelDefaults {
    let mut baseline = crate::CustomProviderModelDefaults::default();
    if protocol_profile_id == Some(vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID) {
        baseline.reasoning = CapabilitySupport::Unknown;
        baseline.reasoning_control = ReasoningControlMode::Unknown;
        baseline.reasoning_efforts.clear();
    }
    baseline
}

/// Selects the credential reference declared by one account-usage query kind.
/// 选择一个账号用量查询类型声明的凭据引用。
///
/// # Parameters
/// 参数
/// `connection` owns the available credential roles and `credential_profile` declares which role
/// the normalized usage implementation requires.
/// `connection` 持有可用凭据角色，`credential_profile` 声明归一化用量实现需要的角色。
///
/// # Returns
/// 返回值
/// The exact secret reference, or an actionable error when the declared role is absent.
/// 返回精确密钥引用；声明的角色缺失时返回可操作错误。
fn account_usage_secret_reference<'a>(
    connection: &'a crate::CredentialConnection,
    credential_profile: &crate::CredentialProfile,
) -> Result<&'a str, String> {
    match credential_profile.account_usage_query {
        crate::AccountUsageQueryKind::OpenRouterCredits => connection
            .management_secret_reference
            .as_deref()
            .ok_or_else(|| {
                String::from("Add an optional OpenRouter management key to query account credits.")
            }),
        crate::AccountUsageQueryKind::Unsupported
        | crate::AccountUsageQueryKind::MiniMaxTokenPlan
        | crate::AccountUsageQueryKind::TavilyCredits
        | crate::AccountUsageQueryKind::DeepSeekBalance
        | crate::AccountUsageQueryKind::KimiCodingPlan
        | crate::AccountUsageQueryKind::CommandCodeUsage => {
            Ok(connection.secret_reference.as_str())
        }
    }
}

/// Reports whether one normalized non-negative decimal string is greater than zero.
/// 判断一个已归一化非负十进制字符串是否大于零。
///
/// # Parameters
/// 参数
/// `value` is provider-normalized account-usage evidence.
/// `value` 是供应商归一化后的账号用量证据。
///
/// # Returns
/// 返回值
/// `true` when the decimal mantissa contains a non-zero digit and is not negative.
/// 当十进制尾数包含非零数字且不是负数时返回 `true`。
fn normalized_usage_decimal_is_positive(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') {
        return false;
    }
    let (mantissa, exponent) = value
        .split_once(['e', 'E'])
        .map_or((value, None), |(mantissa, exponent)| {
            (mantissa, Some(exponent))
        });
    if exponent.is_some_and(|exponent| {
        let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return false;
    }
    let mantissa = mantissa.strip_prefix('+').unwrap_or(mantissa);
    let mut decimal_points = 0_u8;
    let mut digit_count = 0_usize;
    let mut positive = false;
    for byte in mantissa.bytes() {
        match byte {
            b'.' => decimal_points = decimal_points.saturating_add(1),
            b'0' => digit_count = digit_count.saturating_add(1),
            b'1'..=b'9' => {
                digit_count = digit_count.saturating_add(1);
                positive = true;
            }
            _ => return false,
        }
    }
    decimal_points <= 1 && digit_count > 0 && positive
}

/// Reports whether configured positive quota evidence unlocks the complete declared catalog.
/// 判断已配置的正数额度证据是否解锁完整已声明目录。
///
/// # Parameters
/// 参数
/// `policy` declares quota-window identities and `usage` is one credential-scoped observation.
/// `policy` 声明额度窗口身份，`usage` 是一次凭据作用域观测。
///
/// # Returns
/// 返回值
/// `true` when at least one configured, available window has positive remaining quota.
/// 当至少一个已配置且可用的窗口具有正数剩余额度时返回 `true`。
fn account_usage_unlocks_catalog(
    policy: &crate::AccountModelAccessPolicy,
    usage: &AccountUsageSnapshot,
) -> bool {
    policy
        .unrestricted_when_quota_remaining
        .iter()
        .any(|window_id| {
            usage.quota_windows.iter().any(|window| {
                window.available
                    && window.id == *window_id
                    && window
                        .remaining
                        .as_deref()
                        .is_some_and(normalized_usage_decimal_is_positive)
            })
        })
}

/// Resolves the exact configured plan identity from one account-usage observation.
/// 从一次账号用量观测中解析精确配置的套餐身份。
///
/// # Parameters
/// 参数
/// `policy` selects the identity field and `usage` owns provider evidence.
/// `policy` 选择身份字段，`usage` 持有供应商证据。
///
/// # Returns
/// 返回值
/// The selected non-empty plan identity, or `None` when the provider supplied no evidence.
/// 返回选中的非空套餐身份；供应商未提供证据时返回 `None`。
fn account_plan_identity<'a>(
    policy: &crate::AccountModelAccessPolicy,
    usage: &'a AccountUsageSnapshot,
) -> Option<&'a str> {
    let identity = match policy.plan_identity_field {
        crate::AccountPlanIdentityField::PlanName => usage.plan_name.as_deref(),
        crate::AccountPlanIdentityField::PlanId => usage.plan_id.as_deref(),
    };
    identity.map(str::trim).filter(|value| !value.is_empty())
}

/// Applies one Manifest rule's account-plan allowlist and account-access groups.
/// 应用一条 Manifest 规则的账号套餐白名单与账号访问分组。
///
/// # Parameters
/// 参数
/// `rule` owns model access metadata, `policy` owns credential plan rules, `usage` supplies
/// account evidence, and `model_id` is the exact effective-catalog identity.
/// `rule` 持有模型访问元数据，`policy` 持有凭据套餐规则，`usage` 提供账号证据，`model_id`
/// 是精确生效目录身份。
///
/// # Returns
/// 返回值
/// `true` when every configured access dimension admits the model.
/// 当所有已配置访问维度均允许该模型时返回 `true`。
fn model_rule_allows_account_access(
    rule: &crate::provider_manifest::ResolvedRuleInstance,
    policy: &crate::AccountModelAccessPolicy,
    usage: &AccountUsageSnapshot,
    model_id: &str,
) -> bool {
    let plan_name_allowed = usage.plan_name.as_deref().is_none_or(|plan_name| {
        rule.allowed_account_plan_names.is_empty()
            || rule
                .allowed_account_plan_names
                .iter()
                .any(|allowed| allowed == plan_name)
    });
    if !plan_name_allowed {
        return false;
    }
    let Some(plan_identity) = account_plan_identity(policy, usage) else {
        return true;
    };
    let Some(plan_rule) = policy.plan_rules.get(plan_identity) else {
        return true;
    };
    if plan_rule.blocked_model_ids.contains(model_id) {
        return false;
    }
    rule.account_access_groups.is_empty()
        || rule
            .account_access_groups
            .iter()
            .any(|group| plan_rule.allowed_model_groups.contains(group))
}

/// Resolves one source catalog through membership, capabilities, user state, and account access.
/// 通过成员关系、能力、用户状态与账号访问规则解析一个来源目录。
///
/// # Parameters
/// 参数
/// `configuration` owns user state; provider and service identifiers select the exact Manifest
/// scope; `account_usage` supplies credential evidence; `source_policy` and `manifest` freeze
/// system authority; `catalog` is replaced in place.
/// `configuration` 持有用户状态；供应商与服务标识选择精确 Manifest 作用域；`account_usage`
/// 提供凭据证据；`source_policy` 与 `manifest` 冻结系统权威；`catalog` 会被原位替换。
///
/// # Returns
/// 返回值
/// Success after the complete effective catalog is projected, or an exact validation failure.
/// 完整生效目录投影完成后返回成功，否则返回精确校验错误。
fn apply_model_catalog_overlays_with_account_access(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    account_usage: Option<&AccountUsageSnapshot>,
    source_policy: ModelSourcePolicy,
    manifest: Option<&ResolvedProviderManifest>,
    catalog: &mut ConnectionModelCatalog,
) -> Result<(), String> {
    let custom_provider = provider_id.starts_with("custom:");
    // serviceEntryId resolves the credential-free provider-settings path once, then every source,
    // variant, binding, and projection layer shares the same exact entry.
    // serviceEntryId 仅解析一次无凭据供应商设置路径，随后来源、变体、绑定与投影层共享同一精确入口。
    let resolved_service_entry_id = if custom_provider {
        resolve_custom_model_service_entry(configuration, provider_id, service_entry_id)?
            .id
            .clone()
    } else {
        service_entry_id.to_string()
    };
    let service_entry_id = resolved_service_entry_id.as_str();
    let source_is_manifest = catalog.discovery_schema_revision
        == crate::client::MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION;
    let source_is_manual =
        catalog.discovery_schema_revision == MANUAL_MODEL_CATALOG_SCHEMA_REVISION;
    let source_is_discovery = !source_is_manifest && !source_is_manual;
    // sourceAllowed prevents a previously published catalog from leaking identities after a hot
    // Manifest changes this entry's source authority.
    // sourceAllowed 防止热更新 Manifest 改变入口来源权威后，先前发布的目录继续泄漏身份。
    let source_allowed = if source_is_manifest {
        source_policy.includes_manifest()
    } else if source_is_manual {
        source_policy.includes_manual()
    } else {
        source_policy.includes_discovery()
    };
    if !source_allowed {
        catalog.models.clear();
        catalog.page_count = 0;
    }
    if let Some(manifest) = manifest {
        // Membership authority is independent from source availability. `declared_only` retains
        // remote evidence only for identities covered by an exact catalog record or scoped rule;
        // manual user identities remain governed solely by the explicit source policy.
        // 成员权限独立于来源可用性。`declared_only` 仅保留被精确目录记录或作用域规则覆盖的
        // 远端证据；用户手工身份仍只由显式来源策略管理。
        if source_is_discovery
            && !manifest
                .catalog_membership_policy
                .admits_unmatched_discovery()
        {
            let connection = configuration
                .connection_graph
                .connections
                .get(catalog.connection_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Credential connection `{}` is not configured for catalog membership projection.",
                        catalog.connection_id
                    )
                })?;
            catalog.models.retain(|model| {
                manifest.declares_model_identity(
                    service_entry_id,
                    Some(connection.credential_profile_id.as_str()),
                    model.id.as_str(),
                )
            });
        } else if source_is_manifest
            && !manifest
                .catalog_membership_policy
                .includes_declared_identities()
        {
            catalog.models.clear();
            catalog.page_count = 0;
        }
    }
    if source_policy.includes_manifest() {
        let manifest = manifest.ok_or_else(|| {
            format!(
                "Provider `{provider_id}` source policy includes Manifest identity without an active Manifest."
            )
        })?;
        if manifest
            .catalog_membership_policy
            .includes_declared_identities()
        {
            let connection = configuration
                .connection_graph
                .connections
                .get(catalog.connection_id.as_str())
                .ok_or_else(|| {
                    format!(
                        "Credential connection `{}` is not configured for Manifest identity projection.",
                        catalog.connection_id
                    )
                })?;
            let manifest_catalog =
                build_manifest_identity_catalog(manifest, connection, catalog.generation)?;
            merge_manifest_identity_catalog(catalog, manifest_catalog);
        }
    }
    if custom_provider
        && let Some(provider) = configuration.custom_providers.get(provider_id)
        && let Some(entry) = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == service_entry_id)
    {
        // Only provider-neutral manual evidence, or a service entry with one uniquely provable
        // route, may synthesize missing variants. A multi-protocol remote catalog without its
        // route evidence must remain unbound rather than pretending every enabled route observed
        // the model.
        // 只有供应商无关的手工证据，或仅有一条可唯一证明路由的服务入口，才可合成缺失变体。
        // 缺少路由证据的多协议远端目录必须保持未绑定，不能伪装成每条启用路由都观测到该模型。
        let seed_routes = if source_is_manual || entry.protocol_routes.len() == 1 {
            entry.protocol_routes.as_slice()
        } else {
            &[]
        };
        for model in &mut catalog.models {
            if !model.protocol_variants.is_empty() {
                continue;
            }
            for route in seed_routes {
                let material = format!(
                    "custom-manual\0{provider_id}\0{service_entry_id}\0{}\0{}",
                    model.id, route.protocol_profile_id
                );
                model.protocol_variants.insert(
                    route.protocol_profile_id.clone(),
                    ProtocolModelVariant {
                        capabilities: model.capabilities.clone(),
                        capability_evidence: model.capability_evidence.clone(),
                        discovery_signature: format!(
                            "sha256:{}",
                            hex::encode(Sha256::digest(material.as_bytes()))
                        ),
                        catalog_generation: catalog.generation,
                    },
                );
            }
        }
    }
    // custom_baseline is deliberately protocol-neutral. Exact protocol behavior is applied only
    // to per-protocol variants below, then the frozen model binding projects one variant into the
    // effective model view. Connection credentials never carry protocol authority.
    // custom_baseline 有意保持协议中立。下方只对逐协议变体应用精确协议行为，再由冻结模型绑定
    // 把唯一变体投影到有效模型视图；连接凭据绝不携带协议权威。
    let custom_baseline =
        custom_provider.then(|| custom_provider_system_baseline_for_protocol(None));
    // custom_provider_settings is the optional user-owned provider layer above catalog declarations.
    // custom_provider_settings 是位于目录声明之上的可选用户供应商层。
    let custom_provider_settings = custom_provider
        .then(|| {
            configuration
                .custom_provider_model_defaults
                .get(provider_id)
        })
        .flatten();
    if let Some(settings) = custom_provider_settings {
        settings.validate()?;
    }
    let mut models = catalog
        .models
        .drain(..)
        .map(|mut model| {
            let model_id = model.id.clone();
            if custom_provider {
                configuration.apply_provider_capability_defaults(
                    provider_id,
                    &model.capability_evidence,
                    &mut model.capabilities,
                );
                apply_custom_model_defaults(
                    configuration,
                    provider_id,
                    model.id.as_str(),
                    custom_baseline
                        .as_ref()
                        .expect("custom baseline must exist"),
                    custom_provider_settings,
                    &model.capability_evidence,
                    &mut model.capabilities,
                );
                // Every discovered protocol variant resolves the same ordered policy layers
                // against its own evidence. Applying them only to the top-level compatibility
                // view would discard provider/model settings when execution selects a variant.
                // 每个已发现协议变体都依据自身证据解析同一有序策略层。若只应用到顶层兼容视图，
                // 执行选择变体时会丢失供应商/模型设置。
                for (protocol_profile_id, variant) in &mut model.protocol_variants {
                    let baseline = custom_provider_system_baseline_for_protocol(Some(
                        protocol_profile_id.as_str(),
                    ));
                    configuration.apply_provider_capability_defaults(
                        provider_id,
                        &variant.capability_evidence,
                        &mut variant.capabilities,
                    );
                    apply_custom_model_defaults(
                        configuration,
                        provider_id,
                        model_id.as_str(),
                        &baseline,
                        custom_provider_settings,
                        &variant.capability_evidence,
                        &mut variant.capabilities,
                    );
                }
            } else {
                configuration.apply_provider_capability_defaults(
                    provider_id,
                    &model.capability_evidence,
                    &mut model.capabilities,
                );
                crate::apply_common_model_context_default(
                    model.id.as_str(),
                    &mut model.capabilities,
                );
                apply_host_default_reasoning_policy(&mut model.capabilities);
                configuration.apply_model_capability_overrides(
                    provider_id,
                    model.id.as_str(),
                    &mut model.capabilities,
                );
                for variant in model.protocol_variants.values_mut() {
                    configuration.apply_provider_capability_defaults(
                        provider_id,
                        &variant.capability_evidence,
                        &mut variant.capabilities,
                    );
                    crate::apply_common_model_context_default(
                        model_id.as_str(),
                        &mut variant.capabilities,
                    );
                    apply_host_default_reasoning_policy(&mut variant.capabilities);
                    configuration.apply_model_capability_overrides(
                        provider_id,
                        model_id.as_str(),
                        &mut variant.capabilities,
                    );
                }
            }
            (model.id.clone(), model)
        })
        .collect::<BTreeMap<_, _>>();
    if source_policy.includes_manual()
        && let Some(custom_models) = configuration.custom_models.get(provider_id)
    {
        for definition in custom_models.values() {
            definition.validate()?;
            let availability = configuration.custom_model_connection_enabled_override(
                catalog.connection_id.as_str(),
                definition.id.as_str(),
            );
            // An explicit connection-level denial is the only rule that may remove a manual
            // identity. Discovery owns observed fields for collisions, but its result set never
            // erases the independently authorized manual source in a union policy (plan §14.1).
            // 只有显式的连接级禁用规则可移除手工模型身份。发现在同 ID 冲突时拥有
            // 观测字段，但在并集策略下绝不得删除独立授权的手工来源（方案 §14.1）。
            if availability == Some(false) {
                continue;
            }
            // A successful provider catalog remains authoritative for identity, limits, modalities,
            // and operations. The manual declaration may still add explicit advanced behavior that
            // `/models` cannot describe, such as reasoning projections and model parameters.
            // 成功的供应商目录仍是身份、限制、多模态和操作的权威来源。手工声明仍可补充
            // `/models` 无法表达的显式高级行为，例如思考投影和模型参数。
            if let Some(discovered_model) = models.get_mut(definition.id.as_str()) {
                discovered_model.display_name = definition.display_name.clone();
                apply_custom_model_advanced_declaration(
                    &definition.capabilities,
                    &mut discovered_model.capabilities,
                );
                // Manual advanced fields are an allowed post-Manifest user layer and therefore
                // must reach every executable protocol variant, not only the effective view.
                // 手工高级字段是允许的 Manifest 后用户层，因此必须进入每个可执行协议变体，
                // 而不是只修改有效视图。
                for variant in discovered_model.protocol_variants.values_mut() {
                    apply_custom_model_advanced_declaration(
                        &definition.capabilities,
                        &mut variant.capabilities,
                    );
                }
                continue;
            }
            let mut capabilities = definition.capabilities.clone();
            let capability_evidence = manual_model_capability_evidence(&capabilities);
            if custom_provider {
                configuration.apply_provider_capability_defaults(
                    provider_id,
                    &capability_evidence,
                    &mut capabilities,
                );
                apply_custom_model_defaults(
                    configuration,
                    provider_id,
                    definition.id.as_str(),
                    custom_baseline
                        .as_ref()
                        .expect("custom baseline must exist"),
                    custom_provider_settings,
                    &capability_evidence,
                    &mut capabilities,
                );
                // The manual advanced declaration is a model-owned layer above the provider policy,
                // including for models admitted without a matching discovery record.
                // 手工高级声明是位于供应商策略之上的模型自有层，也适用于无对应发现记录的准入模型。
                apply_custom_model_advanced_declaration(
                    &definition.capabilities,
                    &mut capabilities,
                );
            } else {
                configuration.apply_provider_capability_defaults(
                    provider_id,
                    &capability_evidence,
                    &mut capabilities,
                );
                crate::apply_common_model_context_default(
                    definition.id.as_str(),
                    &mut capabilities,
                );
                apply_host_default_reasoning_policy(&mut capabilities);
            }
            if !custom_provider {
                configuration.apply_model_capability_overrides(
                    provider_id,
                    definition.id.as_str(),
                    &mut capabilities,
                );
            }
            // model receives signed protocol variants for system policies before the allowed
            // manual advanced layer is projected onto each variant.
            // model 在允许的手工高级层投影到每个变体前，先接收系统策略的签名协议变体。
            let mut model = ConnectionModel {
                id: definition.id.clone(),
                display_name: definition.display_name.clone(),
                capabilities,
                capability_evidence: capability_evidence.clone(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            };
            if !custom_provider {
                let manifest = manifest.ok_or_else(|| {
                    format!(
                        "System provider `{provider_id}` admitted a manual model without an active Manifest."
                    )
                })?;
                let connection = configuration
                    .connection_graph
                    .connections
                    .get(catalog.connection_id.as_str())
                    .ok_or_else(|| {
                        format!(
                            "Credential connection `{}` is not configured for manual model projection.",
                            catalog.connection_id
                        )
                    })?;
                project_manifest_protocol_variants_for_connection(
                    manifest,
                    connection,
                    catalog.generation,
                    &mut model,
                    true,
                )?;
                apply_custom_model_advanced_declaration(
                    &definition.capabilities,
                    &mut model.capabilities,
                );
                for variant in model.protocol_variants.values_mut() {
                    configuration.apply_provider_capability_defaults(
                        provider_id,
                        &variant.capability_evidence,
                        &mut variant.capabilities,
                    );
                    crate::apply_common_model_context_default(
                        definition.id.as_str(),
                        &mut variant.capabilities,
                    );
                    apply_host_default_reasoning_policy(&mut variant.capabilities);
                    apply_custom_model_advanced_declaration(
                        &definition.capabilities,
                        &mut variant.capabilities,
                    );
                    configuration.apply_model_capability_overrides(
                        provider_id,
                        definition.id.as_str(),
                        &mut variant.capabilities,
                    );
                }
            } else {
                let provider = configuration
                    .custom_providers
                    .get(provider_id)
                    .ok_or_else(|| format!("Custom provider `{provider_id}` is not configured."))?;
                let entry = provider
                    .service_entries
                    .iter()
                    .find(|entry| entry.id == service_entry_id)
                    .ok_or_else(|| {
                        format!(
                            "Custom provider `{provider_id}` has no service entry `{service_entry_id}`."
                        )
                    })?;
                // A manual identity has provider-neutral evidence. Materialize one independent
                // variant per enabled protocol before binding freeze so protocol-specific defaults
                // and later user overrides cannot leak between variants.
                // 手工身份具有供应商无关证据。在绑定冻结前为每个启用协议物化独立变体，使协议专属
                // 默认值与后续用户覆盖不会在变体间泄漏。
                for route in &entry.protocol_routes {
                    let mut variant_capabilities = definition.capabilities.clone();
                    configuration.apply_provider_capability_defaults(
                        provider_id,
                        &capability_evidence,
                        &mut variant_capabilities,
                    );
                    let baseline = custom_provider_system_baseline_for_protocol(Some(
                        route.protocol_profile_id.as_str(),
                    ));
                    apply_custom_model_defaults(
                        configuration,
                        provider_id,
                        definition.id.as_str(),
                        &baseline,
                        custom_provider_settings,
                        &capability_evidence,
                        &mut variant_capabilities,
                    );
                    apply_custom_model_advanced_declaration(
                        &definition.capabilities,
                        &mut variant_capabilities,
                    );
                    let material = format!(
                        "custom-manual\0{provider_id}\0{service_entry_id}\0{}\0{}",
                        definition.id, route.protocol_profile_id
                    );
                    model.protocol_variants.insert(
                        route.protocol_profile_id.clone(),
                        ProtocolModelVariant {
                            capabilities: variant_capabilities,
                            capability_evidence: capability_evidence.clone(),
                            discovery_signature: format!(
                                "sha256:{}",
                                hex::encode(Sha256::digest(material.as_bytes()))
                            ),
                            catalog_generation: catalog.generation,
                        },
                    );
                }
            }
            models.insert(definition.id.clone(), model);
        }
    }
    if custom_provider {
        // Direct model settings are the strongest user layer and must follow any advanced manual declaration.
        // 直接模型设置是最高用户层，必须在任何高级手工声明之后应用。
        for model in models.values_mut() {
            configuration.apply_model_capability_overrides(
                provider_id,
                model.id.as_str(),
                &mut model.capabilities,
            );
            for variant in model.protocol_variants.values_mut() {
                configuration.apply_model_capability_overrides(
                    provider_id,
                    model.id.as_str(),
                    &mut variant.capabilities,
                );
            }
        }
    }
    // Account access is evaluated after support membership and capability resolution so it can
    // hide every fixed, discovered, or manually admitted model without mutating source evidence.
    // 账号访问策略在支持成员关系与能力解析后求值，因此可以隐藏任意固定、发现或手工准入模型，
    // 同时不会修改原始证据。
    models.retain(|model_id, _| {
        configuration.connection_model_is_enabled(catalog.connection_id.as_str(), model_id)
    });
    if let (Some(account_usage), Some(manifest), Some(connection)) = (
        account_usage,
        manifest,
        configuration
            .connection_graph
            .connections
            .get(catalog.connection_id.as_str()),
    ) {
        let account_model_access = manifest
            .descriptor
            .service_entries
            .iter()
            .find(|entry| entry.id == service_entry_id)
            .and_then(|entry| entry.credential_profile(connection.credential_profile_id.as_str()))
            .and_then(|profile| profile.account_model_access.as_ref());
        if let Some(policy) = account_model_access
            && !account_usage_unlocks_catalog(policy, account_usage)
        {
            // Missing or unknown account evidence cannot invent a tier. Exact declared rules are
            // filtered only by configured dimensions; unmatched discovered models remain visible.
            // 缺失或未知账号证据不能臆造套餐。精确声明规则只按已配置维度过滤；未匹配发现模型保持可见。
            models.retain(|model_id, _| {
                manifest
                    .rule_for_model(
                        service_entry_id,
                        Some(connection.credential_profile_id.as_str()),
                        model_id,
                    )
                    .is_none_or(|rule| {
                        model_rule_allows_account_access(rule, policy, account_usage, model_id)
                    })
            });
        }
    }
    catalog.models = models.into_values().collect();
    // Custom-provider protocol bindings freeze at this catalog activation boundary so
    // execution never re-resolves Auto on every request; enabled-protocol changes and
    // preference saves rebuild the catalog, which re-freezes every binding (plan §13.6).
    // 自定义供应商协议绑定在该目录激活边界冻结，执行绝不每次请求重新解析 Auto；
    // 启用协议变更与偏好保存会重建目录，从而重新冻结全部绑定（方案 §13.6）。
    if custom_provider {
        freeze_custom_provider_protocol_bindings(
            configuration,
            provider_id,
            service_entry_id,
            manifest.map_or(0, |manifest| manifest.revision),
            catalog.generation,
            &mut catalog.models,
        )?;
    } else if let Some(manifest) = manifest {
        let connection = configuration
            .connection_graph
            .connections
            .get(catalog.connection_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential connection `{}` is not configured for system binding projection.",
                    catalog.connection_id
                )
            })?;
        freeze_system_provider_protocol_bindings(manifest, connection, &mut catalog.models)?;
    }
    if custom_provider {
        // Custom providers are conversation-only adapters; the effective bound variant decides
        // support, never an arbitrary discovery route's compatibility fields. An unbound model
        // or a binding whose exact variant was not discovered remains visible with its empty
        // projection so the UI can explain the blocked state.
        // 自定义供应商是纯会话适配器；支持状态由生效绑定变体决定，绝不读取任意发现路由的兼容
        // 字段。无绑定模型或未发现精确变体的绑定以空投影保持可见，使 UI 能解释阻止状态。
        catalog.models.retain(|model| {
            model.resolved_protocol_binding.is_none()
                || frozen_binding_variant_is_missing(model)
                || model
                    .capabilities
                    .operations
                    .contains(&AiOperation::ConversationRespond)
        });
    }
    // Validate request projection against the effective bound-model view after binding
    // freeze, so a projection cannot be accepted using another protocol's capabilities.
    // 在绑定冻结后依据有效绑定模型视图校验请求投影，避免使用另一协议能力错误放行投影。
    for model in &catalog.models {
        let manifest_managed_unbound =
            (custom_provider || manifest.is_some()) && model.resolved_protocol_binding.is_none();
        if manifest_managed_unbound || frozen_binding_variant_is_missing(model) {
            continue;
        }
        if let Some(projection) =
            resolve_request_projection(configuration, provider_id, model.id.as_str())?
        {
            projection.validate_for_capabilities(&model.capabilities)?;
        }
    }
    Ok(())
}

/// Freezes one custom provider's model-level protocol bindings into its catalog snapshot.
/// 把一个自定义供应商的模型级协议绑定冻结进其目录快照。
///
/// A binding that still matches the current preference and enabled protocol set is retained
/// without re-resolving Auto; only drifted bindings (preference change, explicit removal, or
/// enabled-protocol change) re-freeze at the activation boundary (plan §13.6).
/// 与当前偏好及启用协议集合仍一致的绑定被保留且不重新解析 Auto；只有漂移绑定
/// （偏好变更、显式偏好移除或启用协议变更）才在激活边界重新冻结（方案 §13.6）。
///
/// # Parameters
/// 参数
/// `configuration` owns the per-model preferences and the provider descriptor; `provider_id`
/// and `service_entry_id` select the exact entry; `manifest_revision` identifies the activated
/// custom contract; `catalog_generation` identifies the immutable catalog snapshot; `models`
/// receives the frozen bindings and compatibility projection.
/// `configuration` 持有逐模型偏好与供应商描述；`provider_id` 与 `service_entry_id` 选择
/// 精确入口；`manifest_revision` 标识已激活自定义契约；`catalog_generation` 标识不可变目录
/// 快照；`models` 接收冻结绑定与兼容投影。
///
/// # Returns
/// 返回值
/// Success after every drifted model carries its re-frozen binding; a missing provider or service
/// entry is an activation invariant violation, while models with invalid preferences keep `None`
/// so the UI and execution path report their controlled binding error.
/// 每个漂移模型携带其重新冻结绑定后返回成功；缺失供应商或服务入口属于激活不变量违规，
/// 偏好无效的模型保持 `None`，由 UI 与执行路径报告受控绑定错误。
fn freeze_custom_provider_protocol_bindings(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    manifest_revision: u64,
    catalog_generation: u64,
    models: &mut [ConnectionModel],
) -> Result<(), String> {
    let provider = configuration
        .custom_providers
        .get(provider_id)
        .ok_or_else(|| {
            format!(
                "Custom provider `{provider_id}` disappeared before protocol bindings were frozen."
            )
        })?;
    let entry = provider
        .service_entries
        .iter()
        .find(|entry| entry.id == service_entry_id)
        .ok_or_else(|| {
            format!(
                "Custom provider `{provider_id}` service entry `{service_entry_id}` disappeared before protocol bindings were frozen."
            )
        })?;
    let enabled_protocols = entry
        .protocol_routes
        .iter()
        .map(|route| route.protocol_profile_id.as_str())
        .collect::<Vec<_>>();
    for model in models.iter_mut() {
        // availableProtocols is the exact model-level intersection authority. Auto must never
        // select a provider-enabled route that did not discover or explicitly declare this model.
        // availableProtocols 是精确的模型级交集权威。Auto 绝不能选择未发现或未显式声明当前
        // 模型的供应商已启用路由。
        let available_protocols = model.protocol_variants.keys().cloned().collect::<Vec<_>>();
        let available_protocol_refs = available_protocols
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let preference = configuration.custom_model_protocol_preferences.get(
            PersistedAiConfiguration::provider_model_key(provider_id, model.id.as_str()).as_str(),
        );
        let frozen_is_current = model
            .resolved_protocol_binding
            .as_ref()
            .map(|binding| {
                binding.manifest_revision == manifest_revision
                    && frozen_binding_is_current(
                        binding,
                        preference,
                        enabled_protocols.as_slice(),
                        available_protocol_refs.as_slice(),
                    )
            })
            .unwrap_or(false);
        if frozen_is_current {
            ensure_manual_custom_bound_variant(
                provider_id,
                service_entry_id,
                catalog_generation,
                model,
            );
            project_frozen_model_binding_compatibility(model)?;
            continue;
        }
        model.resolved_protocol_binding = resolve_custom_provider_binding(
            configuration,
            provider,
            entry,
            available_protocol_refs.as_slice(),
            model.id.as_str(),
            manifest_revision,
        )
        .unwrap_or_default();
        ensure_manual_custom_bound_variant(
            provider_id,
            service_entry_id,
            catalog_generation,
            model,
        );
        project_frozen_model_binding_compatibility(model)?;
    }
    Ok(())
}

/// Freezes provider-neutral manual evidence into one custom model's exact bound protocol.
/// 把供应商无关的手工证据冻结到一个自定义模型的精确绑定协议。
///
/// # Parameters
/// 参数
/// Provider, entry, and generation identify the immutable catalog snapshot; `model` receives a
/// synthetic manual-source variant only when it has a binding and no discovered variants.
/// 供应商、入口与代次标识不可变目录快照；仅当 `model` 有绑定且没有发现变体时，才接收一个
/// 手工来源合成变体。
fn ensure_manual_custom_bound_variant(
    provider_id: &str,
    service_entry_id: &str,
    catalog_generation: u64,
    model: &mut ConnectionModel,
) {
    let Some(binding) = model.resolved_protocol_binding.as_ref() else {
        return;
    };
    if !model.protocol_variants.is_empty() {
        return;
    }
    // Manual-only custom models have provider-neutral user evidence rather than a discovery
    // route. The stable digest identifies this explicit local source without pretending it was
    // observed from a remote endpoint.
    // 纯手工自定义模型只有供应商无关的用户证据而没有发现路由。稳定摘要标识该显式本地来源，
    // 不会伪装成来自远端端点的观测。
    let material = format!(
        "custom-manual\0{provider_id}\0{service_entry_id}\0{}\0{}",
        model.id, binding.protocol_profile_id
    );
    model.protocol_variants.insert(
        binding.protocol_profile_id.clone(),
        ProtocolModelVariant {
            capabilities: model.capabilities.clone(),
            capability_evidence: model.capability_evidence.clone(),
            discovery_signature: format!(
                "sha256:{}",
                hex::encode(Sha256::digest(material.as_bytes()))
            ),
            catalog_generation,
        },
    );
}

/// Freezes system-manifest bindings and their exact compatibility projections for one connection.
/// 为一个连接冻结系统 Manifest 绑定及其精确兼容投影。
///
/// # Parameters
/// 参数
/// `manifest` is the active compiled authority, `connection` supplies the exact entry/profile
/// scope, and `models` receives bindings or an explicit empty non-executable projection.
/// `manifest` 是当前编译权威，`connection` 提供精确入口/配置档作用域，`models` 接收绑定或
/// 显式空的不可执行投影。
///
/// # Returns
/// 返回值
/// Success after every model is either exactly bound or explicitly non-executable.
/// 每个模型都被精确绑定或被明确标记为不可执行后返回成功。
fn freeze_system_provider_protocol_bindings(
    manifest: &ResolvedProviderManifest,
    connection: &CredentialConnection,
    models: &mut [ConnectionModel],
) -> Result<(), String> {
    for model in models {
        match crate::provider_manifest::resolve_system_model_protocol_binding(
            manifest,
            connection.service_entry_id.as_str(),
            Some(connection.credential_profile_id.as_str()),
            model.id.as_str(),
        ) {
            Ok(binding) => model.resolved_protocol_binding = Some(binding),
            Err(crate::provider_manifest::ModelProtocolBindingError::Missing { .. }) => {
                model.resolved_protocol_binding = None;
            }
            Err(error) => {
                return Err(format!(
                    "System model `{}` binding projection failed: {error}",
                    model.id
                ));
            }
        }
        project_frozen_model_binding_compatibility(model)?;
    }
    Ok(())
}

/// Projects one frozen model binding into the read-only compatibility capability fields.
/// 把一个冻结模型绑定投影到只读兼容能力字段。
///
/// # Parameters
/// 参数
/// `model` owns the frozen binding, protocol variants, and compatibility fields.
/// `model` 持有冻结绑定、协议变体与兼容字段。
///
/// # Returns
/// 返回值
/// Success after projecting an exact variant or an empty non-executable view. A bound model
/// without its exact variant remains visible for status diagnostics and fails at execution.
/// 投影精确变体或空的不可执行视图后返回成功。缺少精确变体的已绑定模型保留用于状态诊断，
/// 并在执行时失败。
fn project_frozen_model_binding_compatibility(model: &mut ConnectionModel) -> Result<(), String> {
    let Some(binding) = model.resolved_protocol_binding.as_ref() else {
        model.capabilities = ModelCapabilities::default();
        model.capability_evidence = ModelCapabilityEvidence::default();
        return Ok(());
    };
    let Some(variant) = model
        .protocol_variants
        .get(binding.protocol_profile_id.as_str())
    else {
        // A discovery mismatch blocks only this model. Clearing the effective view prevents
        // workbench admission while preserving the frozen binding and observed variants for the
        // model-level status explanation.
        // 发现不一致只阻止当前模型。清空有效视图可阻止工作台准入，同时保留冻结绑定与已观测
        // 变体，用于模型级状态解释。
        model.capabilities = ModelCapabilities::default();
        model.capability_evidence = ModelCapabilityEvidence::default();
        return Ok(());
    };
    model.capabilities = variant.capabilities.clone();
    model.capability_evidence = variant.capability_evidence.clone();
    Ok(())
}

/// Reports whether one frozen model binding lacks its exact observed protocol variant.
/// 报告一个冻结模型绑定是否缺少其精确已观测协议变体。
///
/// # Parameters
/// 参数
/// `model` owns the frozen binding and immutable protocol-variant map.
/// `model` 持有冻结绑定与不可变协议变体映射。
///
/// # Returns
/// 返回值
/// `true` only for a bound, model-level blocked catalog row.
/// 仅对已绑定但在模型级被阻止的目录行返回 `true`。
fn frozen_binding_variant_is_missing(model: &ConnectionModel) -> bool {
    model
        .resolved_protocol_binding
        .as_ref()
        .is_some_and(|binding| {
            !model
                .protocol_variants
                .contains_key(binding.protocol_profile_id.as_str())
        })
}

/// Reports whether one frozen binding still matches the current preference and model availability.
/// 报告一个冻结绑定是否仍匹配当前偏好与模型可用协议集合。
fn frozen_binding_is_current(
    binding: &crate::provider_manifest::ModelProtocolBinding,
    preference: Option<&crate::CustomModelProtocolPreference>,
    enabled_protocols: &[&str],
    model_available_protocols: &[&str],
) -> bool {
    // expectedBinding reuses the single authoritative resolver. A provider-wide route remaining
    // enabled is insufficient: this exact model must still expose the variant, and a newly
    // observed higher-priority variant must re-freeze Auto at the activation boundary.
    // expectedBinding 复用唯一权威解析器。供应商级路由仍启用并不足够：当前模型必须仍公开该
    // 变体；新观测到的更高优先级变体也必须在激活边界重新冻结 Auto。
    let explicit_protocol = preference
        .and_then(|preference| (*preference).explicit_protocol())
        .map(|protocol| protocol.profile_id());
    crate::provider_manifest::resolve_custom_model_protocol_binding(
        enabled_protocols,
        model_available_protocols,
        "frozen-model",
        binding.manifest_revision,
        explicit_protocol,
    )
    .ok()
    .is_some_and(|expected| {
        binding.source == expected.source
            && binding.protocol_profile_id == expected.protocol_profile_id
    })
}

/// Validates the final context and output limits resolved for one account model.
/// 校验一个账号模型最终解析得到的上下文与输出限制。
///
/// # Parameters
/// 参数
/// `connection_id` and `model_id` identify the exact projection, while `capabilities` contains
/// the fully resolved provider evidence, template/default values, and user differences.
/// `connection_id` 与 `model_id` 标识精确投影，`capabilities` 包含完整解析后的供应商证据、
/// 模板或默认值以及用户差异。
///
/// # Returns
/// 返回值
/// Success when every present limit is positive and output does not exceed context.
/// 当所有已存在限制均为正数且输出不超过上下文时返回成功。
fn validate_resolved_model_capability_limits(
    connection_id: &str,
    model_id: &str,
    capabilities: &ModelCapabilities,
) -> Result<(), String> {
    if capabilities.context_limit == Some(0) {
        return Err(format!(
            "Model `{model_id}` on connection `{connection_id}` has an effective context limit of zero."
        ));
    }
    if capabilities.output_limit == Some(0) {
        return Err(format!(
            "Model `{model_id}` on connection `{connection_id}` has an effective output limit of zero."
        ));
    }
    if let (Some(context_limit), Some(output_limit)) =
        (capabilities.context_limit, capabilities.output_limit)
        && output_limit > context_limit
    {
        return Err(format!(
            "Model `{model_id}` on connection `{connection_id}` has an effective output limit that exceeds its context limit."
        ));
    }
    Ok(())
}

/// Validates every final model limit in one credential-scoped catalog.
/// 校验一个凭据作用域目录中的每个最终模型限制。
///
/// # Parameters
/// 参数
/// `catalog` is a fully overlaid account projection.
/// `catalog` 是已经完整应用覆盖的账号投影。
///
/// # Returns
/// 返回值
/// Success only when every model has a coherent positive token budget.
/// 仅当每个模型都具有一致且为正数的 Token 预算时返回成功。
fn validate_resolved_catalog_limits(catalog: &ConnectionModelCatalog) -> Result<(), String> {
    if catalog.models.len() > crate::PROVIDER_MODEL_CATALOG_MAX_MODELS {
        return Err(format!(
            "Connection `{}` exceeds the {}-model runtime catalog boundary.",
            catalog.connection_id,
            crate::PROVIDER_MODEL_CATALOG_MAX_MODELS
        ));
    }
    for model in &catalog.models {
        validate_resolved_model_capability_limits(
            catalog.connection_id.as_str(),
            model.id.as_str(),
            &model.capabilities,
        )?;
    }
    Ok(())
}

/// Applies only explicit advanced fields from a manual declaration to a discovered model.
/// 仅将手工声明中的显式高级字段应用到已发现模型。
///
/// # Parameters
/// 参数
/// `declared` is the user-owned advanced declaration and `resolved` retains provider-owned basics.
/// `declared` 是用户持有的高级声明，`resolved` 保留供应商持有的基础事实。
fn apply_custom_model_advanced_declaration(
    declared: &ModelCapabilities,
    resolved: &mut ModelCapabilities,
) {
    if declared.reasoning != vulcan_ai_contract::CapabilitySupport::Unknown {
        resolved.reasoning = declared.reasoning;
    }
    if declared.reasoning_control != vulcan_ai_contract::ReasoningControlMode::Unknown {
        resolved.reasoning_control = declared.reasoning_control;
        resolved.reasoning_efforts = declared.reasoning_efforts.clone();
    } else if !declared.reasoning_efforts.is_empty() {
        resolved.reasoning_efforts = declared.reasoning_efforts.clone();
    }
    if declared.prompt_cache != vulcan_ai_contract::PromptCacheCapability::default() {
        resolved.prompt_cache = declared.prompt_cache.clone();
    }
    for parameter in &declared.model_parameters {
        if let Some(existing) = resolved
            .model_parameters
            .iter_mut()
            .find(|existing| existing.id == parameter.id)
        {
            *existing = parameter.clone();
        } else {
            resolved.model_parameters.push(parameter.clone());
        }
    }
    if declared.reasoning != vulcan_ai_contract::CapabilitySupport::Unknown
        || declared.reasoning_control != vulcan_ai_contract::ReasoningControlMode::Unknown
        || !declared.reasoning_efforts.is_empty()
        || declared.prompt_cache != vulcan_ai_contract::PromptCacheCapability::default()
        || !declared.model_parameters.is_empty()
    {
        resolved.evidence_revision = format!(
            "{}+{}",
            resolved.evidence_revision, declared.evidence_revision
        );
    }
}

/// Resolves custom-provider source fallbacks, system policy, and explicit provider settings.
/// 解析自定义供应商的来源回退、系统策略与显式供应商设置。
///
/// # Parameters
/// 参数
/// `configuration`, `provider_id`, and `model_id` identify persisted choices; `defaults` and
/// `capabilities` are the fallback contract and credential-scoped model snapshot.
/// `configuration`、`provider_id` 与 `model_id` 标识持久化选择；`defaults` 与
/// `capabilities` 分别是回退契约和凭据作用域模型快照。
fn apply_custom_model_defaults(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    model_id: &str,
    defaults: &crate::CustomProviderModelDefaults,
    provider_settings: Option<&crate::CustomProviderModelDefaults>,
    evidence: &crate::ModelCapabilityEvidence,
    capabilities: &mut ModelCapabilities,
) {
    let selection = if let Some(crate::CustomModelTemplateDecision::Applied { selection }) =
        configuration.custom_model_template_decision(provider_id, model_id)
    {
        selection.apply_missing(capabilities, evidence);
        Some(selection)
    } else {
        None
    };
    defaults.apply_missing(capabilities, evidence);
    // The system policy fills only fields the catalog did not declare.
    // 系统策略只填充目录未声明的字段。
    defaults.apply_system_capability_policy(capabilities);
    if let Some(settings) = provider_settings {
        settings.apply_provider_settings(capabilities, evidence);
    }

    let existing_conversation = capabilities
        .operations
        .contains(&AiOperation::ConversationRespond);
    let has_explicit_non_conversation_operation = capabilities
        .operations
        .iter()
        .any(|operation| *operation != AiOperation::ConversationRespond);
    let implicit_text_default = existing_conversation || !has_explicit_non_conversation_operation;
    let supports_text_input = evidence
        .supports_text_input
        .or_else(|| selection.map(|value| value.supports_text_input))
        .unwrap_or(implicit_text_default);
    let supports_text_output = evidence
        .supports_text_output
        .or_else(|| selection.map(|value| value.supports_text_output))
        .unwrap_or(implicit_text_default);
    set_custom_conversation_contract(capabilities, supports_text_input && supports_text_output);
}

/// Captures the explicit basic-capability facts carried by one manual model definition.
/// 捕获一个手工模型定义携带的显式基础能力事实。
///
/// # Parameters
/// 参数
/// `capabilities` is the user-owned provider-neutral declaration.
/// `capabilities` 是用户持有的供应商无关能力声明。
///
/// # Returns
/// 返回值
/// Evidence that prevents provider defaults from replacing intentional manual values.
/// 返回用于阻止供应商默认值替换手工明确值的证据。
fn manual_model_capability_evidence(
    capabilities: &ModelCapabilities,
) -> crate::ModelCapabilityEvidence {
    let supports_text = capabilities
        .operations
        .contains(&AiOperation::ConversationRespond);
    crate::ModelCapabilityEvidence {
        context_limit_declared: capabilities.context_limit.is_some(),
        output_limit_declared: capabilities.output_limit.is_some(),
        input_modalities_declared: true,
        output_modalities_declared: true,
        structured_output_declared: true,
        supports_text_input: Some(supports_text),
        supports_text_output: Some(supports_text),
    }
}

/// Closes one custom-provider model to the generic text-conversation contract.
/// 将一个自定义供应商模型收敛到通用文本会话契约。
///
/// # Parameters
/// 参数
/// `capabilities` is mutated in place and `supported` is the final bidirectional-text result.
/// `capabilities` 会被原地修改，`supported` 是最终双向文本支持结果。
fn set_custom_conversation_contract(capabilities: &mut ModelCapabilities, supported: bool) {
    capabilities.output_resources.clear();
    capabilities.operations.clear();
    if supported {
        capabilities
            .operations
            .push(AiOperation::ConversationRespond);
    }
}

/// Applies closed adapter-declared defaults before freezing an execution selection.
/// 在冻结执行选择前应用适配器声明的封闭默认值。
///
/// # Parameters
/// 参数
/// `model` is the exact discovered model, `protocol_id` is the selected route,
/// and `overrides` receives only applicable explicit defaults.
/// `model` 是精确发现的模型，`protocol_id` 是所选路由，
/// `overrides` 仅接收适用的显式默认值。
///
/// # Returns
/// 返回值
/// Success when every applicable required parameter is present.
/// 当每个适用的必填参数都存在时返回成功。
fn apply_model_parameter_defaults(
    model: &crate::ConnectionModel,
    protocol_id: &str,
    overrides: &mut crate::ModelExecutionOverrides,
) -> Result<(), String> {
    for definition in &model.capabilities.model_parameters {
        let applies_to_protocol = definition.protocol_ids.is_empty()
            || definition.protocol_ids.iter().any(|id| id == protocol_id);
        let applies_to_model = definition.model_ids.is_empty()
            || definition.model_ids.iter().any(|id| id == &model.id);
        if !applies_to_protocol
            || !applies_to_model
            || overrides.model_parameters.contains_key(&definition.id)
        {
            continue;
        }
        match &definition.default {
            vulcan_ai_contract::ModelParameterDefault::ExplicitValue { value } => {
                overrides
                    .model_parameters
                    .insert(definition.id.clone(), value.clone());
            }
            vulcan_ai_contract::ModelParameterDefault::RequiredValue {} => {
                return Err(format!(
                    "Model parameter `{}` requires an explicit value for model `{}`.",
                    definition.id, model.id
                ));
            }
            vulcan_ai_contract::ModelParameterDefault::Omit {}
            | vulcan_ai_contract::ModelParameterDefault::ProviderDefault {} => {}
        }
    }
    Ok(())
}

/// Builds a stable content revision for one effective connection model catalog.
/// 为一个生效连接模型目录构建稳定内容修订号。
///
/// # Parameters
/// 参数
/// `catalog` is the complete catalog frozen at one activation boundary.
/// `catalog` 是在一个激活边界冻结的完整目录。
///
/// # Returns
/// 返回值
/// A SHA-256 revision over semantic catalog content, excluding process-local observation counters.
/// 返回覆盖目录语义内容、排除进程内观测计数器的 SHA-256 修订号。
fn effective_model_catalog_content_revision(
    catalog: &ConnectionModelCatalog,
) -> Result<String, String> {
    // canonicalCatalog preserves every execution-relevant fact while removing refresh counters
    // and response ordering that cannot identify content across a process restart.
    // canonicalCatalog 保留每项与执行相关的事实，同时移除无法跨进程重启标识内容的刷新计数器
    // 与响应顺序。
    let mut canonical_catalog = catalog.clone();
    canonical_catalog.generation = 0;
    canonical_catalog
        .models
        .sort_by(|left, right| left.id.cmp(&right.id));
    for model in &mut canonical_catalog.models {
        for variant in model.protocol_variants.values_mut() {
            variant.catalog_generation = 0;
        }
    }
    // material is a closed serialization of the canonical catalog shape.
    // material 是规范目录形态的封闭序列化结果。
    let material = serde_json::to_vec(&canonical_catalog).map_err(|error| {
        format!("failed to serialize effective model catalog revision: {error}")
    })?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(material))))
}

/// Builds the immutable revision for one exact LLM execution route.
/// 为一条精确 LLM 执行路由构建不可变修订号。
///
/// # Parameters
/// 参数
/// `route`, `model`, and `overrides` are the validated execution snapshots frozen at selection
/// time. Catalog, manifest, discovery, and presentation revisions are deliberately excluded
/// because they are audit facts rather than wire semantics (plan §17.7).
/// `route`、`model` 与 `overrides` 是选择时冻结的已校验执行快照。目录、Manifest、发现与
/// 展示修订被刻意排除，因为它们是审计事实而非 Wire 语义（方案 §17.7）。
///
/// # Returns
/// 返回值
/// Returns a SHA-256 content revision or a serialization error.
/// 返回 SHA-256 内容修订号或序列化错误。
fn provider_execution_route_revision(
    route: &ProviderExecutionRoute,
    model: &ConnectionModel,
    overrides: &crate::ModelExecutionOverrides,
) -> Result<String, String> {
    // selectedCredentialProfile is the only profile whose authentication and upload contract can
    // affect this route; other profiles may be added without changing an active session.
    // selectedCredentialProfile 是唯一会影响此路由鉴权与上传契约的配置档；新增其他配置档
    // 不得改变活动会话。
    let selected_credential_profile = route
        .entry
        .credential_profile(route.connection.credential_profile_id.as_str())
        .ok_or_else(|| {
            format!(
                "credential profile `{}` is absent from execution route `{}`",
                route.connection.credential_profile_id, route.entry.id
            )
        })?;
    // canonicalCredentialProfile removes its user-facing label while preserving the complete
    // selected credential contract.
    // canonicalCredentialProfile 移除面向用户的标签，同时保留完整的已选凭据契约。
    let mut canonical_credential_profile = selected_credential_profile.clone();
    canonical_credential_profile.display_name.clear();
    // canonicalOriginPolicy excludes the non-secret UI example but retains every enforced host
    // and path restriction.
    // canonicalOriginPolicy 排除非敏感 UI 示例，同时保留全部强制主机与路径限制。
    let mut canonical_origin_policy = route.entry.service_origin_override_policy.clone();
    if let Some(policy) = canonical_origin_policy.as_mut() {
        policy.example_origin.clear();
    }
    // canonicalProtocolRoute removes model-discovery controls because discovery has completed
    // before this immutable execution route is built.
    // canonicalProtocolRoute 移除模型发现控制，因为不可变执行路由构建前发现已经完成。
    let mut canonical_protocol_route = route.protocol_route.clone();
    canonical_protocol_route.model_discovery = None;
    canonical_protocol_route.model_discovery_path = None;
    canonical_protocol_route.model_discovery_base_url_override = None;
    // entryExecutionIdentity contains exactly the selected service entry facts consumed after
    // selection, excluding its label, discovery policy, unrelated credential profiles, and
    // unrelated protocol routes.
    // entryExecutionIdentity 仅包含选择后消费的服务入口事实，排除标签、发现策略、无关凭据档
    // 与无关协议路由。
    let entry_execution_identity = (
        &route.entry.id,
        route.entry.kind,
        &route.entry.base_url,
        &canonical_credential_profile,
        route.entry.authentication,
        &route.entry.request_headers,
        &canonical_origin_policy,
        &route.entry.native_inference_bindings,
        route.entry.request_projector,
        &route.entry.default_usage_scopes,
    );
    // connectionExecutionIdentity retains credential, origin, and authorization ownership while
    // excluding the account label, management-only secret, and policies already resolved into
    // `overrides`.
    // connectionExecutionIdentity 保留凭据、源地址与授权归属，同时排除账号标签、仅管理秘密，
    // 以及已解析进 `overrides` 的策略。
    let connection_execution_identity = (
        &route.connection.id,
        &route.connection.provider_id,
        &route.connection.service_entry_id,
        &route.connection.credential_profile_id,
        route.connection.credential_mode,
        &route.connection.credential_id,
        &route.connection.secret_reference,
        &route.connection.service_origin_override,
    );
    // modelExecutionIdentity contains the only model facts read during request construction and
    // response normalization. Display labels, discovery evidence, other protocol variants, and
    // manifest provenance remain available on the frozen selection for diagnostics.
    // modelExecutionIdentity 包含请求构造与响应归一化期间读取的唯一模型事实。展示标签、发现
    // 证据、其他协议变体与 Manifest 来源仍保留在冻结选择中供诊断。
    let model_execution_identity = (&model.id, &model.capabilities);
    let material = serde_json::to_vec(&(
        (&route.provider.id, route.provider.kind),
        entry_execution_identity,
        connection_execution_identity,
        &canonical_protocol_route,
        &route.secondary_route,
        &route.custom_model_projection,
        route.tool_schema_projection,
        &route.request_projection_profiles,
        &route.response_projection_profiles,
        &route.upload_channels,
        model_execution_identity,
        overrides,
    ))
    .map_err(|error| format!("failed to serialize provider route revision: {error}"))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(material))))
}

/// Carries immutable public selection metadata without credential material.
/// 承载不含凭据材料的不可变公共选择元数据。
///
/// The selection is Clone and non-secret so a prepared turn can freeze the complete route,
/// model, overrides, and binding at the activation boundary (plan §6.3/§R4).
/// 选择可 Clone 且不含秘密，使已准备回合能在激活边界冻结完整路由、模型、覆盖与绑定
/// （方案 §6.3/§R4）。
#[derive(Debug, Clone)]
pub struct ResolvedPublicAiSelection {
    /// Exact provider execution route.
    /// 精确供应商执行路由。
    pub route: ProviderExecutionRoute,
    /// Exact credential-scoped model snapshot.
    /// 精确凭据作用域模型快照。
    pub model: crate::ConnectionModel,
    /// Local model execution overrides.
    /// 本地模型执行覆盖。
    pub overrides: crate::ModelExecutionOverrides,
    /// Runtime catalog revision frozen with this selection.
    /// 与此选择一同冻结的运行时目录修订。
    pub catalog_revision: String,
    /// Provider-manifest catalog generation frozen with this selection.
    /// 与此选择一同冻结的供应商清单目录代数。
    pub manifest_generation: u64,
    /// Model-level protocol binding that produced this selection.
    /// 产生此选择的模型级协议绑定。
    pub protocol_binding: crate::provider_manifest::ModelProtocolBinding,
    /// Content-addressed revision of the exact LLM route and request projection snapshot.
    /// 精确 LLM 路由与请求投影快照的内容寻址修订号。
    pub route_revision: String,
}

/// Carries frozen route inputs and one live credential lease resolved before controller dispatch.
/// 承载控制器分派前解析完成的冻结路由输入与一份实时凭据租约。
pub struct ResolvedPublicAiExecution {
    /// Exact provider execution route.
    /// 精确供应商执行路由。
    pub route: ProviderExecutionRoute,
    /// Exact credential-scoped model snapshot.
    /// 精确凭据作用域模型快照。
    pub model: crate::ConnectionModel,
    /// Exact account lease that resolves the latest secret revision for every physical request.
    /// 为每次真实请求解析最新秘密修订版的精确账号租约。
    pub credential: ProviderExecutionCredentialLease,
    /// Local model execution overrides.
    /// 本地模型执行覆盖。
    pub overrides: crate::ModelExecutionOverrides,
    /// Model-level protocol binding that produced this execution.
    /// 产生此执行的模型级协议绑定。
    pub protocol_binding: crate::provider_manifest::ModelProtocolBinding,
    /// Shared public execution client.
    /// 共享公共执行客户端。
    pub client: ProviderExecutionClient,
}

/// One provider tool target paired with an eligible configured credential.
/// 与一个合格已配置凭据配对的供应商工具目标。
#[derive(Debug, Clone, PartialEq)]
pub struct AvailableProviderToolTarget {
    /// Exact credential connection identifier.
    /// 精确凭据连接标识。
    pub connection_id: String,
    /// User-visible credential name.
    /// 用户可见凭据名称。
    pub connection_display_name: String,
    /// Frozen source layer that owns the selected target contract.
    /// 持有所选目标契约的冻结来源层。
    pub manifest_source_layer: crate::provider_manifest::ManifestSourceLayer,
    /// Exact model- or service-specific target contract.
    /// 精确的模型或服务专属目标契约。
    pub target: ProviderToolTarget,
}

/// Describes one exact provider-tool media input route without exposing credentials or references.
/// 描述一条精确的供应商工具媒体输入路线，且不暴露凭据或引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolMediaInputRouteConstraint {
    /// Stable upload channel identifier owning this route.
    /// 持有当前路线的稳定上传通道标识。
    pub upload_channel_id: String,
    /// Exact MIME types accepted by both the operation binding and upload channel.
    /// 操作绑定与上传通道共同接受的精确 MIME 类型。
    pub accepted_mime_types: Vec<String>,
    /// Optional maximum source bytes accepted by this route.
    /// 当前路线可接受的可选最大源字节数。
    pub max_bytes: Option<u64>,
    /// Optional maximum source count accepted by this route.
    /// 当前路线可接受的可选最大源数量。
    pub max_count: Option<u64>,
}

/// Carries reviewed media input constraints for one fixed provider-understanding facade.
/// 携带一个固定供应商理解外壳的已审核媒体输入约束。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolMediaInputProfile {
    /// Exact resource kind consumed by this fixed facade.
    /// 当前固定外壳消费的精确资源类型。
    pub resource_kind: ResourceKind,
    /// Ordered concrete upload routes declared for the projected source field.
    /// 为投影来源字段声明的有序具体上传路线。
    pub routes: Vec<ProviderToolMediaInputRouteConstraint>,
}

/// One provider-tool template model declared for an eligible configured credential.
/// 为一个合格已配置凭据声明的供应商工具模板模型。
///
/// This management inventory intentionally ignores the model's enabled state so a newly synced
/// `default_enabled=false` model remains visible and can be enabled without a software update.
/// 此管理目录刻意忽略模型启用状态，使新同步且 `default_enabled=false` 的模型仍可见，并可在
/// 无需更新软件的情况下启用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredProviderToolTemplateModel {
    /// Exact owning provider identifier.
    /// 精确所属供应商标识。
    pub provider_id: String,
    /// Exact configured credential connection identifier.
    /// 精确已配置凭据连接标识。
    pub connection_id: String,
    /// Exact regional or product service entry identifier.
    /// 精确区域或产品服务入口标识。
    pub service_entry_id: String,
    /// Exact upstream media or provider-tool model identifier.
    /// 精确上游媒体或供应商工具模型标识。
    pub model_id: String,
    /// User-visible model name supplied by the signed binding catalog.
    /// 签名绑定目录提供的用户可见模型名称。
    pub display_name: String,
}

/// Maps one fixed understanding capability to its exact provider-neutral media modality.
/// 将一个固定理解能力映射为精确的供应商无关媒体模态。
///
/// # Parameters
/// 参数
/// `capability` is the immutable provider-tool capability selected for this invocation.
/// `capability` 是本次调用选定的不可变供应商工具能力。
///
/// # Returns
/// 返回值
/// The resource kind and stable UI media label for image, video, or audio understanding; non-media
/// capabilities return `None` and cannot enter upload preflight.
/// 图片、视频或音频理解返回资源类型与稳定界面媒体标签；非媒体能力返回 `None`，不能进入上传预检。
fn provider_tool_media_resource_kind(
    capability: ProviderToolCapabilityKind,
) -> Option<(ResourceKind, &'static str)> {
    match capability {
        ProviderToolCapabilityKind::ImageUnderstand => Some((ResourceKind::Image, "image")),
        ProviderToolCapabilityKind::VideoUnderstand => Some((ResourceKind::Video, "video")),
        ProviderToolCapabilityKind::AudioUnderstand => Some((ResourceKind::Audio, "audio")),
        _ => None,
    }
}

/// Builds an exact progress emitter for one provider-tool media upload.
/// 为一次供应商工具媒体上传构建精确进度发射器。
///
/// # Parameters
/// 参数
/// The observer and owner opt into reporting; capability, route, operation, arguments, credential
/// profile, and workspace are the same frozen inputs consumed by upload materialization.
/// 观察器与所有者用于启用报告；能力、路由、操作、参数、凭据配置档与工作区均与上传物化消费的冻结输入一致。
///
/// # Returns
/// 返回值
/// `None` when no host observer or owner exists; otherwise a fully preflighted emitter context.
/// 未安装宿主观察器或不存在所有者时返回 `None`；否则返回完成全部预检的发射上下文。
/// Carries one local understanding-media upload preflight for the process-local cache.
/// 携带一次模型扩展进程级理解媒体缓存所需的本地上传预检结果。
#[derive(Debug, Clone)]
struct ProviderToolMediaUploadPreflight {
    /// Exact scalar binding that maps the fixed media facade into the provider operation.
    /// 将固定媒体外壳映射到供应商操作的精确标量绑定。
    binding: crate::OperationUploadBinding,
    /// Validated resource and upload channel selected from the frozen manifest.
    /// 从冻结 Manifest 选出的已校验资源与上传通道。
    inspection: crate::upload_engine::OperationUploadRouteInspection,
    /// Source bytes identity captured before the network upload.
    /// 网络上传前捕获的源字节身份。
    source_identity: crate::PreparedMediaSourceIdentity,
    /// Exact media modality used by the provider-reference cache key.
    /// 供应商引用缓存键使用的精确媒体模态。
    resource_kind: ResourceKind,
    /// Wall-clock timestamp that anchors the one-day reuse window.
    /// 锚定一天复用窗口的墙钟首次使用时间。
    first_used_at_unix_millis: u64,
}

/// Reports whether one media upload binding can participate in the scalar process-local cache.
/// 报告一条媒体上传绑定是否可以参与进程级标量缓存。
///
/// # Parameters
/// 参数
/// `binding` is the exact frozen operation upload contract.
/// `binding` 是精确冻结的操作上传契约。
///
/// # Returns
/// 返回值
/// `true` only for one scalar resource binding; collection bindings remain on the generic uploader.
/// 仅对单资源标量绑定返回 `true`；集合绑定继续交由通用上传器处理。
fn provider_tool_media_upload_binding_is_cacheable(
    binding: &crate::OperationUploadBinding,
) -> bool {
    binding.item_source_path.is_none() && binding.item_target_path.is_none()
}

/// Preflights one fixed media-understanding call for bounded process-local reference reuse.
/// 为一次固定媒体理解调用预检有界的进程级引用复用。
///
/// The model-extension tool has no conversation media ledger. This cache is deliberately scoped
/// only by the provider upload contract and source bytes, so a later independent tool call can
/// reuse a still-valid provider object without pretending that a session context exists.
/// 模型扩展工具没有会话媒体账本。本缓存只按供应商上传契约与源字节作用域，后续独立工具调用
/// 可以复用仍有效的供应商对象，但不会伪造会话上下文。
async fn prepare_provider_tool_media_upload(
    execution_client: &ProviderExecutionClient,
    resolved: &ResolvedProviderToolTarget,
    operation: &crate::OperationDeclaration,
    arguments: &serde_json::Value,
    credential_profile: &crate::CredentialProfile,
    workspace_root: Option<&Path>,
    capability: ProviderToolCapabilityKind,
) -> Result<
    Option<(
        ProviderToolMediaUploadPreflight,
        Option<crate::PreparedMediaReferenceSnapshot>,
    )>,
    String,
> {
    let (resource_kind, _media_label) = provider_tool_media_resource_kind(capability)
        .ok_or_else(|| format!("Capability `{capability:?}` is not media understanding."))?;
    let first_used_at_unix_millis = current_unix_millis()?;
    let projection = resolved
        .target
        .contract
        .media_input_projection
        .as_ref()
        .ok_or_else(|| {
            format!(
                "Media-understanding target `{}` has no input projection.",
                resolved.target.target_id
            )
        })?;
    let source_pointer = format!("/{}", projection.path_field);
    let mut matching_bindings = operation
        .upload_bindings
        .iter()
        .filter(|binding| binding.source_path == source_pointer);
    let binding = matching_bindings.next().ok_or_else(|| {
        format!(
            "Media-understanding operation `{}` has no upload binding for `{source_pointer}`.",
            operation.operation_id
        )
    })?;
    if matching_bindings.next().is_some() {
        return Err(format!(
            "Media-understanding operation `{}` has ambiguous upload bindings for `{source_pointer}`.",
            operation.operation_id
        ));
    }
    // Collection bindings remain owned by the generic upload materializer. The process-local
    // understanding cache has a one-resource scalar key and must not narrow valid array contracts.
    // 集合绑定继续由通用上传物化器持有。进程级理解缓存使用单资源标量键，不得收窄有效数组契约。
    if !provider_tool_media_upload_binding_is_cacheable(binding) {
        return Ok(None);
    }
    let source_reference = arguments
        .pointer(source_pointer.as_str())
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "Media-understanding upload source `{source_pointer}` must be one non-empty local path."
            )
        })?;
    let inspection = crate::upload_engine::inspect_operation_upload_route(
        binding,
        source_reference,
        1,
        workspace_root,
        &resolved.upload_channels,
        credential_profile.credential_role.as_deref(),
    )
    .await?;
    if matches!(
        inspection.channel.primitive,
        Some(crate::UploadPrimitiveKind::InlineDataUrl | crate::UploadPrimitiveKind::InlineBase64)
    ) {
        if resource_kind == ResourceKind::Video {
            return Err(String::from(
                "Video-understanding tool uploads must use a provider resource channel; Base64 is forbidden.",
            ));
        }
        return Ok(None);
    }
    let Some(local_path) = inspection.resource.local_path.as_ref() else {
        return Ok(None);
    };
    let source_identity = execution_client
        .read_media_source_identity(local_path, None)
        .await?;
    let cached_reference = execution_client
        .prepared_provider_tool_media_reference(
            resolved.connection.provider_id.as_str(),
            resolved.entry.id.as_str(),
            resolved.connection.id.as_str(),
            &inspection.channel,
            local_path,
            resource_kind,
            inspection.resource.mime_type.as_str(),
            None,
        )
        .await?;
    Ok(Some((
        ProviderToolMediaUploadPreflight {
            binding: binding.clone(),
            inspection,
            source_identity,
            resource_kind,
            first_used_at_unix_millis,
        },
        cached_reference,
    )))
}

/// Extracts one scalar provider reference written by a fixed operation upload binding.
/// 提取固定操作上传绑定写入的一个标量供应商引用。
fn provider_tool_materialized_reference(
    arguments: &serde_json::Value,
    binding: &crate::OperationUploadBinding,
) -> Result<String, String> {
    if binding.item_source_path.is_some() || binding.item_target_path.is_some() {
        return Err(String::from(
            "Media-understanding upload cache requires a scalar upload binding.",
        ));
    }
    arguments
        .pointer(&binding.target_path)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            format!(
                "Media-understanding upload target `{}` did not contain one scalar provider reference.",
                binding.target_path
            )
        })
}

/// Builds one progress context only for an actual provider-backed understanding-media upload.
/// 仅为实际使用供应商上传通道的理解媒体上传构建进度上下文。
#[allow(clippy::too_many_arguments)]
async fn provider_tool_media_upload_progress_context(
    observer: Option<crate::RuntimeMediaUploadProgressObserver>,
    owner_session_id: Option<&str>,
    capability: ProviderToolCapabilityKind,
    resolved: &ResolvedProviderToolTarget,
    operation: &crate::OperationDeclaration,
    arguments: &serde_json::Value,
    credential_profile: &crate::CredentialProfile,
    workspace_root: Option<&Path>,
) -> Result<Option<RuntimeMediaUploadProgressContext>, String> {
    // observer and owner are both required to route a user-visible progress replacement.
    // observer 与 owner 必须同时存在，才能路由面向用户的进度替换事件。
    let (Some(observer), Some(owner_session_id)) = (observer, owner_session_id) else {
        return Ok(None);
    };
    let Some((_resource_kind, media_kind)) = provider_tool_media_resource_kind(capability) else {
        return Ok(None);
    };
    // projection identifies the exact provider-native source field generated from the facade.
    // projection 标识由固定外壳生成的精确供应商原生来源字段。
    let projection = resolved
        .target
        .contract
        .media_input_projection
        .as_ref()
        .ok_or_else(|| {
            format!(
                "Media-understanding target `{}` has no input projection for upload progress.",
                resolved.target.target_id
            )
        })?;
    // sourcePointer is the authoritative operation binding path for the projected media field.
    // sourcePointer 是投影媒体字段的权威操作绑定路径。
    let source_pointer = format!("/{}", projection.path_field);
    // binding must be unique because one fixed media invocation owns exactly one input file.
    // binding 必须唯一，因为一次固定媒体调用只持有一个输入文件。
    let mut matching_bindings = operation
        .upload_bindings
        .iter()
        .filter(|binding| binding.source_path == source_pointer);
    let binding = matching_bindings.next().ok_or_else(|| {
        format!(
            "Media-understanding operation `{}` has no upload binding for `{source_pointer}`.",
            operation.operation_id
        )
    })?;
    if matching_bindings.next().is_some() {
        return Err(format!(
            "Media-understanding operation `{}` has ambiguous upload bindings for `{source_pointer}`.",
            operation.operation_id
        ));
    }
    // sourceValue is the exact native argument preserved until upload materialization.
    // sourceValue 是保留到上传物化阶段的精确原生参数。
    let source_value = arguments.pointer(source_pointer.as_str()).ok_or_else(|| {
        format!(
            "Media-understanding upload source `{source_pointer}` is missing after argument projection."
        )
    })?;
    // sourceReference and itemCount retain the scalar versus explicit one-item-array contract.
    // sourceReference 与 itemCount 保留标量与显式单项数组契约。
    let (source_reference, item_count) = match source_value {
        serde_json::Value::String(reference) if !reference.trim().is_empty() => {
            (reference.as_str(), 1_usize)
        }
        serde_json::Value::Array(values) if values.len() == 1 => {
            let reference = values[0]
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    String::from(
                        "Media-understanding upload array must contain one non-empty local path.",
                    )
                })?;
            (reference, values.len())
        }
        _ => {
            return Err(String::from(
                "Media-understanding upload source must be one non-empty local path.",
            ));
        }
    };
    // uploadRoute repeats the generic engine preflight and suppresses progress for non-reporting
    // channels such as inline data conversion.
    // uploadRoute 重复通用引擎预检，并对不报告进度的通道（例如内联数据转换）抑制进度。
    let inspection = crate::upload_engine::inspect_operation_upload_route(
        binding,
        source_reference,
        item_count,
        workspace_root,
        &resolved.upload_channels,
        credential_profile.credential_role.as_deref(),
    )
    .await?;
    if !inspection.channel.progress_supported {
        return Ok(None);
    }
    let upload_channel_id = inspection.channel.upload_channel_id;
    let total_bytes = inspection.resource.byte_len;
    // fileName is derived only from the already validated local path argument.
    // fileName 仅从已校验的本地路径参数派生。
    let file_name = Path::new(source_reference)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("Media-understanding upload path has no valid file name."))?;
    Ok(Some(RuntimeMediaUploadProgressContext {
        observer,
        base: crate::RuntimeMediaUploadProgressEvent {
            transfer_id: format!("media-tool:{}", Uuid::new_v4()),
            owner_session_id: owner_session_id.to_string(),
            provider_id: resolved.connection.provider_id.clone(),
            service_entry_id: resolved.entry.id.clone(),
            connection_id: resolved.connection.id.clone(),
            model_id: resolved.target.upstream_target_id.clone(),
            protocol_profile_id: String::from("provider-tool"),
            manifest_revision: resolved.manifest_revision,
            manifest_generation: resolved.manifest_generation,
            route_revision: resolved.route_revision.clone(),
            operation_id: operation.operation_id.clone(),
            upload_channel_id,
            file_name: file_name.to_string(),
            media_kind: media_kind.to_string(),
            phase: crate::RuntimeMediaUploadPhase::Preparing,
            bytes_sent: 0,
            total_bytes: Some(total_bytes),
            error: None,
        },
        last_bytes_sent: Arc::new(AtomicU64::new(0)),
    }))
}

/// Frozen internal provider tool route resolved before secret loading.
/// 秘密加载前解析的冻结内部供应商工具路由。
struct ResolvedProviderToolTarget {
    /// Exact configured credential.
    /// 精确已配置凭据。
    connection: CredentialConnection,
    /// Exact regional or product entry.
    /// 精确区域或产品入口。
    entry: crate::ServiceEntry,
    /// Exact model-native tool target.
    /// 精确模型原生工具目标。
    target: ProviderToolTarget,
    /// Unique manifest operation owning pre-dispatch upload bindings, when declared.
    /// 拥有分派前上传绑定的唯一 Manifest 操作（若有声明）。
    operation: Option<crate::OperationDeclaration>,
    /// Frozen request profiles referenced by the owning operation.
    /// 拥有当前目标的操作所引用的冻结请求 Profile。
    request_profiles: Vec<crate::provider_manifest::RequestProjectionProfile>,
    /// Frozen response profiles referenced by the owning operation.
    /// 拥有当前目标的操作所引用的冻结响应 Profile。
    response_profiles: Vec<crate::provider_manifest::ResponseProjectionProfile>,
    /// Immutable upload channels from the same manifest generation as the target.
    /// 与目标同一 Manifest 代际的不可变上传通道。
    upload_channels: Vec<crate::UploadChannelDeclaration>,
    /// Exact manifest revision captured with the target and operation.
    /// 与目标和操作共同捕获的精确 Manifest 修订号。
    manifest_revision: u64,
    /// Exact immutable manifest generation captured with the target.
    /// 与目标共同捕获的精确不可变 Manifest 代际。
    manifest_generation: u64,
    /// Content- and target-addressed non-secret route identity.
    /// 内容与目标寻址的非秘密路由身份。
    route_revision: String,
}

impl ResolvedProviderToolTarget {
    /// Formats the immutable, secret-free operation route used by provider-tool diagnostics.
    /// 格式化供应商工具诊断使用的不可变且不含秘密的操作路由。
    fn diagnostic_context(&self) -> String {
        let operation_id = self
            .operation
            .as_ref()
            .map(|operation| operation.operation_id.as_str())
            .unwrap_or("none");
        let request_profiles = self
            .request_profiles
            .iter()
            .map(|profile| format!("{}@{}", profile.profile_id, profile.revision))
            .collect::<Vec<_>>()
            .join(",");
        let response_profiles = self
            .response_profiles
            .iter()
            .map(|profile| format!("{}@{}", profile.profile_id, profile.revision))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "provider={}, target={}, upstream_target={}, operation={}, manifest_revision={}, request_profiles=[{}], response_profiles=[{}]",
            self.connection.provider_id,
            self.target.target_id,
            self.target.upstream_target_id,
            operation_id,
            self.manifest_revision,
            request_profiles,
            response_profiles
        )
    }
}

/// Appends one immutable provider-tool route context without exposing credential material.
/// 为错误追加一份不可变供应商工具路由上下文且不暴露凭据材料。
fn diagnose_provider_tool_failure(error: String, context: &str) -> String {
    let error = truncate_provider_tool_diagnostic_component(
        error.as_str(),
        PROVIDER_TOOL_ERROR_DIAGNOSTIC_MAX_CHARACTERS,
    );
    let context = truncate_provider_tool_diagnostic_component(
        context,
        PROVIDER_TOOL_ROUTE_DIAGNOSTIC_MAX_CHARACTERS,
    );
    bound_provider_task_diagnostic(format!("{error} [provider_tool_route: {context}]").as_str())
}

/// Truncates one provider-tool diagnostic component on a Unicode scalar boundary.
/// 在 Unicode 标量边界截断一个供应商工具诊断组成部分。
///
/// # Parameters
/// 参数
/// `value` is the exact component and `maximum_characters` is its closed public bound.
/// `value` 是精确组成部分，`maximum_characters` 是其封闭公开边界。
///
/// # Returns
/// 返回值
/// The original value within bounds, otherwise a suffix-marked truncated copy.
/// 边界内返回原值，否则返回带截断后缀的副本。
fn truncate_provider_tool_diagnostic_component(value: &str, maximum_characters: usize) -> String {
    if value.chars().count() <= maximum_characters {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(maximum_characters.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

/// Redacts the exact execution credential before appending bounded provider-tool route context.
/// 在追加有界供应商工具路由上下文前脱敏精确执行凭据。
///
/// # Parameters
/// 参数
/// `error` is the transport or provider diagnostic, `context` is non-secret route identity, and
/// `secret` is the exact in-memory credential used by the failed physical request.
/// `error` 是传输或供应商诊断，`context` 是非秘密路由身份，`secret` 是失败物理请求使用的
/// 精确内存凭据。
///
/// # Returns
/// 返回值
/// A bounded public diagnostic containing no exact credential value.
/// 返回不含精确凭据值的有界公开诊断。
fn diagnose_provider_tool_failure_with_secret(
    error: String,
    context: &str,
    secret: &CredentialExecutionSecret,
) -> String {
    diagnose_provider_tool_failure(redact_provider_tool_secret(error.as_str(), secret), context)
}

/// Removes the exact execution credential and applies the durable diagnostic boundary.
/// 移除精确执行凭据并应用持久诊断边界。
///
/// # Parameters
/// 参数
/// `value` is an untrusted diagnostic and `secret` owns the credential that must not escape.
/// `value` 是不可信诊断，`secret` 持有绝不能外泄的凭据。
///
/// # Returns
/// 返回值
/// A bounded diagnostic with every exact credential occurrence replaced.
/// 返回每个精确凭据出现位置均已替换的有界诊断。
fn redact_provider_tool_secret(value: &str, secret: &CredentialExecutionSecret) -> String {
    let sensitive_value = secret.expose_value();
    let redacted = if sensitive_value.is_empty() {
        value.to_string()
    } else {
        value.replace(sensitive_value, "<redacted>")
    };
    bound_provider_task_diagnostic(redacted.as_str())
}

/// Returns the current Unix wall-clock time in milliseconds for token-expiry decisions.
/// 返回用于 Token 过期判断的当前 Unix 墙钟毫秒时间。
///
/// # Returns
/// 返回值
/// A platform-independent timestamp or a controlled clock-range error.
/// 返回跨平台时间戳或受控的时钟范围错误。
fn current_unix_millis() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| String::from("System clock is earlier than the Unix epoch."))?
        .as_millis();
    u64::try_from(millis)
        .map_err(|_| String::from("System time exceeds the supported OAuth range."))
}

/// Maps the metadata client's typed failure into the public runtime status contract.
/// 将元数据客户端的强类型失败映射为公共运行时状态契约。
///
/// # Parameters
/// 参数
/// `failure` is produced at the exact HTTP or parser boundary.
/// `failure` 由精确 HTTP 或解析器边界产出。
///
/// # Returns
/// 返回值
/// The equivalent serializable catalog failure category.
/// 返回等价的可序列化目录失败分类。
fn catalog_discovery_failure_kind(
    failure: ModelDiscoveryFailureKind,
) -> CatalogDiscoveryFailureKind {
    match failure {
        ModelDiscoveryFailureKind::Unsupported => CatalogDiscoveryFailureKind::Unsupported,
        ModelDiscoveryFailureKind::AuthenticationFailed => {
            CatalogDiscoveryFailureKind::AuthenticationFailed
        }
        ModelDiscoveryFailureKind::RateLimited => CatalogDiscoveryFailureKind::RateLimited,
        ModelDiscoveryFailureKind::TemporarilyUnavailable => {
            CatalogDiscoveryFailureKind::TemporarilyUnavailable
        }
        ModelDiscoveryFailureKind::InvalidResponse => CatalogDiscoveryFailureKind::InvalidResponse,
        ModelDiscoveryFailureKind::Failed => CatalogDiscoveryFailureKind::Failed,
    }
}

/// Maps one failure kind to its non-stale user-facing state.
/// 将一种失败类型映射为非过期的面向用户状态。
fn discovery_state_for_failure(failure: CatalogDiscoveryFailureKind) -> CatalogDiscoveryState {
    match failure {
        CatalogDiscoveryFailureKind::Unsupported => CatalogDiscoveryState::Unsupported,
        CatalogDiscoveryFailureKind::AuthenticationFailed => {
            CatalogDiscoveryState::AuthenticationFailed
        }
        CatalogDiscoveryFailureKind::RateLimited => CatalogDiscoveryState::RateLimited,
        CatalogDiscoveryFailureKind::TemporarilyUnavailable => {
            CatalogDiscoveryState::TemporarilyUnavailable
        }
        CatalogDiscoveryFailureKind::InvalidResponse => CatalogDiscoveryState::InvalidResponse,
        CatalogDiscoveryFailureKind::Failed => CatalogDiscoveryState::Failed,
    }
}

/// Returns the exact active connection and rejects cross-provider selections.
/// 返回精确当前连接并拒绝跨供应商选择。
fn exact_active_connection(
    configuration: &PersistedAiConfiguration,
    active: &ActiveAiModelSelection,
) -> Result<CredentialConnection, String> {
    let connection = configuration
        .connection_graph
        .connections
        .get(active.connection_id.as_str())
        .cloned()
        .ok_or_else(|| {
            format!(
                "Active credential connection `{}` is not configured.",
                active.connection_id
            )
        })?;
    if connection.provider_id != active.provider_id {
        return Err(format!(
            "Active connection `{}` does not belong to provider `{}`.",
            connection.id, active.provider_id
        ));
    }
    Ok(connection)
}

/// Validates local limit overrides, cache policy, and closed model parameters.
/// 校验本地限制覆盖、缓存策略与封闭模型参数。
fn validate_model_overrides(
    model: &crate::ConnectionModel,
    protocol_id: &str,
    overrides: &crate::ModelExecutionOverrides,
) -> Result<(), String> {
    if let Some(context_limit) = overrides.context_limit {
        if context_limit == 0 {
            return Err(format!(
                "Model `{}` context override must be greater than zero.",
                model.id
            ));
        }
        if model
            .capabilities
            .context_limit
            .is_some_and(|advertised| context_limit > advertised)
        {
            return Err(format!(
                "Model `{}` context override exceeds its discovered limit.",
                model.id
            ));
        }
    }
    if let Some(output_limit) = overrides.output_limit {
        if output_limit == 0 {
            return Err(format!(
                "Model `{}` output override must be greater than zero.",
                model.id
            ));
        }
        if model
            .capabilities
            .output_limit
            .is_some_and(|advertised| output_limit > advertised)
        {
            return Err(format!(
                "Model `{}` output override exceeds its discovered limit.",
                model.id
            ));
        }
        let effective_context_limit = overrides.context_limit.or(model.capabilities.context_limit);
        if effective_context_limit.is_some_and(|context_limit| output_limit > context_limit) {
            return Err(format!(
                "Model `{}` output override exceeds its effective context limit.",
                model.id
            ));
        }
    }
    let cache_policy = overrides
        .prompt_cache
        .as_ref()
        .unwrap_or(&PromptCachePolicy::ProviderDefault);
    model
        .capabilities
        .prompt_cache
        .validate_policy(cache_policy)?;
    for (parameter_id, value) in &overrides.model_parameters {
        let definition = model
            .capabilities
            .model_parameters
            .iter()
            .find(|definition| definition.id == *parameter_id)
            .ok_or_else(|| {
                format!(
                    "Model parameter `{parameter_id}` is not registered for model `{}`.",
                    model.id
                )
            })?;
        if !definition.protocol_ids.is_empty()
            && !definition.protocol_ids.iter().any(|id| id == protocol_id)
        {
            return Err(format!(
                "Model parameter `{parameter_id}` is not registered for protocol `{protocol_id}`."
            ));
        }
        if !definition.model_ids.is_empty()
            && !definition.model_ids.iter().any(|id| id == &model.id)
        {
            return Err(format!(
                "Model parameter `{parameter_id}` is not registered for model `{}`.",
                model.id
            ));
        }
        definition.validate_value(value)?;
        for dependency in &definition.dependent_required {
            if !overrides.model_parameters.contains_key(dependency) {
                return Err(format!(
                    "Model parameter `{parameter_id}` requires `{dependency}`."
                ));
            }
        }
        for conflict in &definition.conflicts {
            if overrides.model_parameters.contains_key(conflict) {
                return Err(format!(
                    "Model parameter `{parameter_id}` conflicts with `{conflict}`."
                ));
            }
        }
    }
    Ok(())
}

/// Builds the shared HTTP client with a closed redirect policy and bounded connection setup.
/// 构建采用封闭重定向策略与有界连接建立时间的共享 HTTP 客户端。
///
/// # Returns
/// 返回值
/// A hardened client, or the exact client-construction failure.
/// 返回加固后的客户端，或精确的客户端构建失败。
fn build_public_ai_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|error| format!("Failed to build the public AI HTTP client: {error}"))
}

#[cfg(test)]
mod tests {
    //! Focused tests for request-scoped public model and account resolution.
    //! 请求级公共模型与账号解析定向测试。
    use super::*;
    use crate::{
        CredentialMode, MINIMAX_REASONING_SPLIT_PARAMETER_ID, ModelExecutionOverrides,
        parse_minimax_openai_model_catalog, parse_openai_model_catalog,
        parse_openrouter_model_catalog,
    };
    use std::collections::BTreeSet;
    use std::sync::atomic::AtomicUsize;

    use vulcan_ai_protocols::{
        ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        GEMINI_GENERATE_CONTENT_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
    };

    /// Verifies the shared inline threshold applies to every upload-capable media modality without inventing channels.
    /// 验证共享内联阈值适用于每种具备上传能力的媒体模态，且不会虚构上传通道。
    #[test]
    fn media_understanding_upload_policy_is_modality_and_threshold_exact() {
        assert!(media_understanding_uses_provider_upload(
            ResourceKind::Video,
            true,
            1
        ));
        assert!(!media_understanding_uses_provider_upload(
            ResourceKind::Video,
            false,
            u64::MAX
        ));
        assert!(!media_understanding_uses_provider_upload(
            ResourceKind::Image,
            true,
            crate::PREFERRED_INLINE_MEDIA_MAX_BYTES
        ));
        assert!(media_understanding_uses_provider_upload(
            ResourceKind::Image,
            true,
            crate::PREFERRED_INLINE_MEDIA_MAX_BYTES + 1
        ));
        assert!(media_understanding_uses_provider_upload(
            ResourceKind::Audio,
            true,
            crate::PREFERRED_INLINE_MEDIA_MAX_BYTES + 1
        ));
    }

    /// Verifies collection media bindings bypass only the scalar cache and remain valid for generic upload materialization.
    /// 验证集合媒体绑定仅绕过标量缓存，且仍可由通用上传物化器正常处理。
    #[test]
    fn media_understanding_collection_upload_binding_bypasses_scalar_cache() {
        let scalar = crate::OperationUploadBinding {
            source_path: String::from("/video_path"),
            target_path: String::from("/video"),
            item_source_path: None,
            item_target_path: None,
            item_match_path: None,
            item_match_value: None,
            upload_channel_refs: vec![String::from("provider-files")],
            mime_type: Some(String::from("video/mp4")),
            mime_types_by_extension: Default::default(),
            required: true,
            remove_source: true,
            target_encoding: Default::default(),
        };
        let collection = crate::OperationUploadBinding {
            source_path: String::from("/references"),
            target_path: String::from("/references"),
            item_source_path: Some(String::from("/path")),
            item_target_path: Some(String::from("/resource")),
            ..scalar.clone()
        };

        assert!(provider_tool_media_upload_binding_is_cacheable(&scalar));
        assert!(!provider_tool_media_upload_binding_is_cacheable(
            &collection
        ));
    }

    /// Verifies dropping an active upload emits cancellation with the latest byte count.
    /// 验证丢弃进行中的上传会使用最近字节数发出取消终态。
    #[test]
    fn media_upload_terminal_guard_emits_cancelled_on_drop() {
        // observed owns the complete ordered event sequence emitted by the test context.
        // observed 持有测试上下文发出的完整有序事件序列。
        let observed = Arc::new(StdMutex::new(Vec::new()));
        let observed_for_callback = Arc::clone(&observed);
        let context = RuntimeMediaUploadProgressContext {
            observer: Arc::new(move |event| {
                observed_for_callback
                    .lock()
                    .expect("progress event lock")
                    .push(event);
            }),
            base: crate::RuntimeMediaUploadProgressEvent {
                transfer_id: String::from("transfer-one"),
                owner_session_id: String::from("session-one"),
                provider_id: String::from("provider-one"),
                service_entry_id: String::from("entry-one"),
                connection_id: String::from("connection-one"),
                model_id: String::from("model-one"),
                protocol_profile_id: String::from("provider-tool"),
                manifest_revision: 1,
                manifest_generation: 2,
                route_revision: String::from("route-one"),
                operation_id: String::from("video-understanding"),
                upload_channel_id: String::from("video-file"),
                file_name: String::from("clip.mp4"),
                media_kind: String::from("video"),
                phase: crate::RuntimeMediaUploadPhase::Preparing,
                bytes_sent: 0,
                total_bytes: Some(10),
                error: None,
            },
            last_bytes_sent: Arc::new(AtomicU64::new(0)),
        };
        {
            // guard remains armed while one uploading event advances the retained byte counter.
            // guard 在一条上传事件推进保留字节计数时保持启用。
            let _guard = RuntimeMediaUploadTerminalGuard::new(Some(&context));
            context.emit(crate::RuntimeMediaUploadPhase::Uploading, 7, Some(10), None);
        }

        let events = observed.lock().expect("progress event lock");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].phase, crate::RuntimeMediaUploadPhase::Uploading);
        assert_eq!(events[1].phase, crate::RuntimeMediaUploadPhase::Cancelled);
        assert_eq!(events[1].bytes_sent, 7);
    }

    /// Verifies provider diagnostics redact credentials while preserving bounded route identity.
    /// 验证供应商诊断会脱敏凭据并保留有界路由身份。
    #[test]
    fn provider_tool_diagnostics_redact_secret_and_preserve_route_context() {
        let secret = CredentialSecretEnvelope::new_api_key(String::from("secret-provider-key"))
            .expect("test credential should validate")
            .execution_secret()
            .expect("test execution secret should resolve");
        let diagnostic = diagnose_provider_tool_failure_with_secret(
            format!(
                "secret-provider-key {} secret-provider-key",
                "provider diagnostic ".repeat(1_000)
            ),
            "provider=test, target=exact-target",
            &secret,
        );

        assert!(!diagnostic.contains("secret-provider-key"));
        assert!(diagnostic.contains("<redacted>"));
        assert!(diagnostic.contains("provider_tool_route"));
        assert!(diagnostic.contains("target=exact-target"));
        assert!(
            diagnostic.chars().count()
                <= crate::provider_task::PROVIDER_TASK_DIAGNOSTIC_MAX_CHARACTERS
        );
    }

    /// Verifies session-route lease admission is bounded without evicting an established route.
    /// 验证会话路由租约准入有界，且不会淘汰已建立路由。
    #[test]
    fn session_selection_lease_admission_is_bounded_and_idempotent() {
        assert!(
            validate_session_selection_lease_admission(
                "session-existing",
                SESSION_SELECTION_LEASE_MAX_ENTRIES,
                true,
            )
            .is_ok(),
            "an established immutable lease must remain readable at capacity"
        );
        let capacity_error = validate_session_selection_lease_admission(
            "session-new",
            SESSION_SELECTION_LEASE_MAX_ENTRIES,
            false,
        )
        .expect_err("a new lease above capacity must fail closed");
        assert!(capacity_error.contains("4096 entries"), "{capacity_error}");
        let identity_error = validate_session_selection_lease_admission("", 0, false)
            .expect_err("an empty lease identity must fail closed");
        assert!(identity_error.contains("1..=1024"), "{identity_error}");
    }

    /// Verifies the absence guard excludes lease writers until its caller-owned publication ends.
    /// 验证缺失 Guard 会排除租约写入方，直到调用方持有的发布结束。
    #[test]
    fn session_selection_lease_absence_guard_excludes_writers() {
        // runtime owns the exact lease map whose writer admission is under test.
        // runtime 持有本测试所验证写入准入的精确租约映射。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // sessionIds identify the complete reconciliation scope protected by the guard.
        // sessionIds 标识由 Guard 保护的完整协调作用域。
        let session_ids = vec![String::from("session-under-reconciliation")];
        // absenceGuard retains the write lock after proving the scoped session has no lease.
        // absenceGuard 在证明作用域会话没有租约后继续持有写锁。
        let absence_guard = runtime
            .acquire_session_selection_lease_absence_guard(&session_ids)
            .expect("an unleased session should admit reconciliation");
        // blockedWriter proves no concurrent lease installation can cross the publication seam.
        // blockedWriter 证明并发租约安装无法穿越发布接缝。
        let blocked_writer = runtime.session_selection_leases.try_write();
        assert!(
            blocked_writer.is_err(),
            "the absence guard must exclude every lease writer"
        );
        drop(blocked_writer);
        drop(absence_guard);
        // admittedWriter proves writer admission resumes immediately after publication releases the guard.
        // admittedWriter 证明发布释放 Guard 后写入准入会立即恢复。
        let admitted_writer = runtime.session_selection_leases.try_write();
        assert!(
            admitted_writer.is_ok(),
            "lease writers must resume after the absence guard is released"
        );
    }

    /// Verifies an abandoned explicit revocation restores the exact leased selection.
    /// 验证被放弃的显式撤销会恢复精确的已租用选择。
    #[test]
    fn session_selection_lease_reconciliation_guard_restores_on_drop() {
        // runtime and configuration provide one fully resolved immutable test route.
        // runtime 与 configuration 提供一条完整解析的不可变测试路由。
        let (runtime, configuration) = two_account_runtime();
        // selection is the exact before-image installed for the revoked session.
        // selection 是为被撤销会话安装的精确前镜像。
        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "openrouter",
                "openai/gpt-5.6-sol",
            )
            .expect("test selection should resolve");
        runtime
            .lease_session_selection(String::from("session-revoked"), selection)
            .expect("test session should lease its route");
        {
            // reconciliationGuard removes the lease while excluding every concurrent writer.
            // reconciliationGuard 在排除全部并发写入方期间移除租约。
            let reconciliation_guard = runtime
                .acquire_session_selection_lease_reconciliation_guard(
                    &[String::from("session-revoked")],
                    &[],
                )
                .expect("explicit revocation should acquire its guard");
            assert!(
                reconciliation_guard.guard.get("session-revoked").is_none(),
                "the candidate lease map must remove the revoked owner"
            );
        }
        assert!(
            runtime
                .session_selection_lease("session-revoked")
                .expect("restored lease should remain readable")
                .is_some(),
            "dropping an uncommitted guard must restore the lease"
        );
    }

    /// Verifies a committed explicit revocation remains absent after writer exclusion ends.
    /// 验证已提交的显式撤销在写入排他结束后继续保持缺失。
    #[test]
    fn session_selection_lease_reconciliation_guard_commits_removal() {
        // runtime and configuration provide one fully resolved immutable test route.
        // runtime 与 configuration 提供一条完整解析的不可变测试路由。
        let (runtime, configuration) = two_account_runtime();
        // selection is the exact route whose ownership is explicitly revoked.
        // selection 是所有权被显式撤销的精确路由。
        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "openrouter",
                "openai/gpt-5.6-sol",
            )
            .expect("test selection should resolve");
        runtime
            .lease_session_selection(String::from("session-committed"), selection)
            .expect("test session should lease its route");
        // reconciliationGuard owns the removal until the paired publication commits.
        // reconciliationGuard 持有删除操作，直到配套发布提交。
        let reconciliation_guard = runtime
            .acquire_session_selection_lease_reconciliation_guard(
                &[String::from("session-committed")],
                &[],
            )
            .expect("explicit revocation should acquire its guard");
        reconciliation_guard.commit();
        assert!(
            runtime
                .session_selection_lease("session-committed")
                .expect("lease map should remain readable")
                .is_none(),
            "committing the guard must keep the revoked route absent"
        );
    }

    /// Verifies absence conflicts reject the whole reconciliation before any revocation occurs.
    /// 验证缺失冲突会在任何撤销发生前拒绝整个协调事务。
    #[test]
    fn session_selection_lease_reconciliation_guard_is_atomic_on_conflict() {
        // runtime and configuration provide one fully resolved immutable test route.
        // runtime 与 configuration 提供一条完整解析的不可变测试路由。
        let (runtime, configuration) = two_account_runtime();
        // selection is cloned because both independent owners lease the same frozen route.
        // selection 会被克隆，因为两个独立所有者租用同一冻结路由。
        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "openrouter",
                "openai/gpt-5.6-sol",
            )
            .expect("test selection should resolve");
        runtime
            .lease_session_selection(String::from("session-to-revoke"), selection.clone())
            .expect("revocation owner should lease its route");
        runtime
            .lease_session_selection(String::from("session-must-be-absent"), selection)
            .expect("absence-conflict owner should lease its route");
        let error = runtime
            .acquire_session_selection_lease_reconciliation_guard(
                &[String::from("session-to-revoke")],
                &[String::from("session-must-be-absent")],
            )
            .err()
            .expect("an occupied absence scope must reject reconciliation");
        assert!(error.contains("session-must-be-absent"), "{error}");
        assert!(
            runtime
                .session_selection_lease("session-to-revoke")
                .expect("revocation owner should remain readable")
                .is_some(),
            "absence validation must occur before any removal"
        );
    }

    /// Builds one independent legacy task record for bounded-recovery scheduling tests.
    /// 为有界恢复调度测试构建一条独立旧版任务记录。
    ///
    /// # Parameters
    /// 参数
    /// `index` provides the stable task identity suffix.
    /// `index` 提供稳定任务身份后缀。
    ///
    /// # Returns
    /// 返回值
    /// A non-terminal record whose transport details are intentionally irrelevant to the
    /// scheduler-only test.
    /// 返回一条非终态记录；其传输细节对于仅测试调度器的场景刻意无关。
    fn recovery_scheduler_record(index: usize) -> ProviderAsyncTaskRecord {
        ProviderAsyncTaskRecord {
            execution_id: format!("execution-recovery-{index:02}"),
            owner_session_id: Some(String::from("session-recovery")),
            route: ProviderAsyncTaskRoute {
                provider_id: String::from("test-provider"),
                service_entry_id: String::from("test-entry"),
                connection_id: String::from("test-connection"),
                selection: ProviderToolTargetSelection {
                    connection_id: String::from("test-connection"),
                    target_id: String::from("test-target"),
                },
                adapter: crate::ProviderToolAdapterKind::GenericJson,
                manifest_revision: 0,
                frozen_target: None,
                frozen_service_entry: None,
                operation_id: None,
                frozen_operation: None,
                frozen_request_profiles: Vec::new(),
                frozen_response_profiles: Vec::new(),
            },
            provider_task_id: Some(format!("provider-task-{index:02}")),
            recovery: ProviderAsyncTaskRecoveryContext::None,
            phase: ProviderAsyncTaskPhase::Running,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            deadline_unix_ms: 60_001,
            poll_interval_ms: 15_000,
            consecutive_observation_failures: 0,
            cancellation_supported: false,
            error: None,
            result: None,
        }
    }

    /// Verifies one recovery pass overlaps independent tasks without crossing its fixed ceiling.
    /// 验证一次恢复轮次会重叠执行独立任务，同时不会越过固定并发上限。
    #[tokio::test]
    async fn provider_task_recovery_scheduler_is_concurrent_and_bounded() {
        // counters observe live scheduling independently from task transport behavior.
        // counters 独立于任务传输行为观测实时调度。
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let records = (0..PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY * 2)
            .map(recovery_scheduler_record)
            .collect::<Vec<_>>();

        let observations = observe_provider_task_records_bounded(records, |record| {
            // each admitted future owns counter handles and the record identity for its full life.
            // 每个获准 Future 在完整生命周期内持有计数器句柄与记录身份。
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst).saturating_add(1);
                maximum.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                if record.execution_id.is_empty() {
                    return Err(String::from("scheduler lost the task identity"));
                }
                Ok(())
            }
        })
        .await;

        assert_eq!(
            observations.len(),
            PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY * 2
        );
        assert!(observations.iter().all(|(_, result)| result.is_ok()));
        let observed_maximum = maximum.load(Ordering::SeqCst);
        assert!(
            observed_maximum > 1,
            "independent recovery observations must overlap"
        );
        assert!(
            observed_maximum <= PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY,
            "observed {observed_maximum} concurrent tasks above the configured ceiling"
        );
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    /// Verifies failed post-restart result recovery remains visible on a completed task.
    /// 验证重启后结果恢复失败仍会在已完成任务上保持可见。
    #[test]
    fn completed_task_result_recovery_retains_transient_diagnostic() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let record = ProviderAsyncTaskRecord {
            execution_id: String::from("execution-completed-recovery"),
            owner_session_id: Some(String::from("session-one")),
            route: ProviderAsyncTaskRoute {
                provider_id: String::from("test-provider"),
                service_entry_id: String::from("test-entry"),
                connection_id: String::from("test-connection"),
                selection: ProviderToolTargetSelection {
                    connection_id: String::from("test-connection"),
                    target_id: String::from("test-target"),
                },
                adapter: crate::ProviderToolAdapterKind::GenericJson,
                manifest_revision: 0,
                frozen_target: None,
                frozen_service_entry: None,
                operation_id: None,
                frozen_operation: None,
                frozen_request_profiles: Vec::new(),
                frozen_response_profiles: Vec::new(),
            },
            provider_task_id: Some(String::from("provider-task-one")),
            recovery: ProviderAsyncTaskRecoveryContext::None,
            phase: ProviderAsyncTaskPhase::Completed,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            deadline_unix_ms: 60_001,
            poll_interval_ms: 15_000,
            consecutive_observation_failures: 0,
            cancellation_supported: false,
            error: None,
            result: None,
        };
        runtime
            .provider_tasks
            .replace(record)
            .expect("completed recovery record should insert");

        runtime
            .record_provider_task_observation_error(
                "execution-completed-recovery",
                String::from("temporary recovery failure"),
            )
            .expect("transient recovery diagnostic should persist");

        let retained = runtime
            .provider_tasks
            .require("execution-completed-recovery")
            .expect("completed recovery record should remain");
        assert_eq!(retained.phase, ProviderAsyncTaskPhase::Completed);
        assert_eq!(retained.consecutive_observation_failures, 1);
        assert_eq!(
            retained.error.as_deref(),
            Some("temporary recovery failure")
        );
    }

    /// Verifies an observation failure at the frozen deadline becomes terminal immediately.
    /// 验证冻结截止时间到达时的观测失败会立即成为终态。
    #[test]
    fn expired_task_observation_error_persists_timeout_terminal_state() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let now_unix_ms = current_unix_millis().expect("fixture clock should resolve");
        let created_at_unix_ms = now_unix_ms.saturating_sub(60_000);
        let deadline_unix_ms = now_unix_ms.saturating_sub(1);
        let record = ProviderAsyncTaskRecord {
            execution_id: String::from("execution-expired-observation"),
            owner_session_id: Some(String::from("session-one")),
            route: ProviderAsyncTaskRoute {
                provider_id: String::from("test-provider"),
                service_entry_id: String::from("test-entry"),
                connection_id: String::from("test-connection"),
                selection: ProviderToolTargetSelection {
                    connection_id: String::from("test-connection"),
                    target_id: String::from("test-target"),
                },
                adapter: crate::ProviderToolAdapterKind::GenericJson,
                manifest_revision: 0,
                frozen_target: None,
                frozen_service_entry: None,
                operation_id: None,
                frozen_operation: None,
                frozen_request_profiles: Vec::new(),
                frozen_response_profiles: Vec::new(),
            },
            provider_task_id: Some(String::from("provider-task-one")),
            recovery: ProviderAsyncTaskRecoveryContext::None,
            phase: ProviderAsyncTaskPhase::Running,
            created_at_unix_ms,
            updated_at_unix_ms: created_at_unix_ms,
            deadline_unix_ms,
            poll_interval_ms: 15_000,
            consecutive_observation_failures: 0,
            cancellation_supported: false,
            error: None,
            result: None,
        };
        runtime
            .provider_tasks
            .replace(record)
            .expect("expired record should insert");

        runtime
            .record_provider_task_observation_error(
                "execution-expired-observation",
                String::from("credential refresh stalled"),
            )
            .expect("deadline transition should persist");

        let retained = runtime
            .provider_tasks
            .require("execution-expired-observation")
            .expect("expired record should remain");
        assert_eq!(retained.phase, ProviderAsyncTaskPhase::Failed);
        assert_eq!(retained.consecutive_observation_failures, 0);
        assert!(
            retained
                .error
                .as_deref()
                .is_some_and(|error| error.contains("exceeded its"))
        );
    }

    /// Builds one valid Kimi OAuth envelope for live credential-lease tests.
    /// 为实时凭据租约测试构建一份有效 Kimi OAuth 封装。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `revision` is the secure-store revision and `access_token` is the observable test value.
    /// `revision` 是安全仓库修订号，`access_token` 是可观测测试值。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A non-expiring renewable credential whose refresh endpoint is never contacted by these tests.
    /// 返回一份不会过期的可续期凭据；这些测试不会访问其刷新端点。
    fn test_kimi_oauth_secret(revision: u64, access_token: &str) -> CredentialSecretEnvelope {
        // now anchors a valid issue and expiration interval for the credential invariant.
        // now 为凭据不变量提供有效的签发与过期时间区间。
        let now = current_unix_millis().expect("test clock should be available");
        CredentialSecretEnvelope {
            version: 1,
            revision,
            material: crate::CredentialSecretMaterial::KimiCodingPlanOAuth {
                tokens: crate::OAuthTokenSet {
                    access_token: access_token.to_string(),
                    refresh_token: String::from("refresh-token"),
                    issued_at_unix_ms: now,
                    expires_at_unix_ms: now.saturating_add(60 * 60 * 1_000),
                    scopes: BTreeSet::new(),
                    account_subject: Some(String::from("account-subject")),
                    email: Some(String::from("account@example.com")),
                },
                device_id: String::from("00000000-0000-4000-8000-000000000001"),
            },
        }
    }

    /// Verifies the next physical request observes a concurrently refreshed OAuth revision.
    /// 验证下一次真实请求会观察到并发刷新的 OAuth 修订版。
    #[tokio::test]
    async fn execution_credential_lease_observes_latest_revision_after_refresh() {
        // initial is the exact token revision used by the rejected physical request.
        // initial 是被拒绝真实请求使用的精确 Token 修订版。
        let initial = test_kimi_oauth_secret(1, "token-a");
        // store provides a mutable isolated secure-store boundary without durable writes.
        // store 提供不产生持久写入的可变隔离安全仓库边界。
        let store = Arc::new(IsolatedCredentialSecretStore {
            delegate: Arc::new(UnavailableCredentialSecretResolver),
            secret_reference: String::from("oauth-secret"),
            secret: StdMutex::new(Some(initial.clone())),
        });
        // secrets erases the concrete fixture behind the production credential-store contract.
        // secrets 通过生产凭据仓库契约抹除具体夹具类型。
        let secrets: Arc<dyn CredentialSecretStore> = store.clone();
        // lease freezes the account reference while resolving its token revision per request.
        // lease 冻结账号引用，同时按请求解析其 Token 修订版。
        let lease = ProviderExecutionCredentialLease::new(
            String::from("oauth-connection"),
            String::from("oauth-secret"),
            Arc::new(OAuthAuthorizationRuntime::new(reqwest::Client::new())),
            Arc::new(StdMutex::new(BTreeMap::new())),
            secrets,
        );
        // rejected is the first physical request's immutable credential snapshot.
        // rejected 是第一次真实请求的不可变凭据快照。
        let rejected = lease
            .resolve_for_request()
            .await
            .expect("initial credential should resolve");
        assert_eq!(rejected.revision(), 1);
        assert_eq!(rejected.expose_value(), "token-a");

        // refreshed simulates another request completing OAuth refresh while this execution remains active.
        // refreshed 模拟当前执行仍活动时另一个请求完成 OAuth 刷新。
        let refreshed = test_kimi_oauth_secret(2, "token-b");
        store
            .replace_secret("oauth-secret", 1, &refreshed)
            .await
            .expect("concurrent refresh should replace revision one");
        assert!(
            lease
                .refresh_after_unauthorized(rejected.revision())
                .await
                .expect("newer revision should satisfy unauthorized recovery")
        );
        // current is the credential that the immediately following physical request will send.
        // current 是紧接着的真实请求将要发送的凭据。
        let current = lease
            .resolve_for_request()
            .await
            .expect("latest credential should resolve");
        assert_eq!(current.revision(), 2);
        assert_eq!(current.expose_value(), "token-b");
        assert!(
            !lease
                .refresh_after_unauthorized(rejected.revision())
                .await
                .expect("same physical request must not refresh twice")
        );
        lease.mark_request_succeeded();
        // later simulates a subsequent physical request observing another concurrent OAuth refresh.
        // later 模拟后续真实请求观察到另一次并发 OAuth 刷新。
        let later = test_kimi_oauth_secret(3, "token-c");
        store
            .replace_secret("oauth-secret", current.revision(), &later)
            .await
            .expect("later refresh should replace revision two");
        assert!(
            lease
                .refresh_after_unauthorized(current.revision())
                .await
                .expect("successful request should restore one recovery opportunity")
        );
        // latest is the token resolved for the physical request after the second recovery.
        // latest 是第二次恢复后真实请求解析到的 Token。
        let latest = lease
            .resolve_for_request()
            .await
            .expect("latest credential should resolve after guard reset");
        assert_eq!(latest.revision(), 3);
        assert_eq!(latest.expose_value(), "token-c");
    }

    /// Supplies one immutable API key to catalog-refresh tests.
    /// 为目录刷新测试提供一个不可变 API Key。
    #[derive(Clone)]
    struct TestStaticSecretStore {
        /// Exact versioned test secret returned for every reference.
        /// 为每个引用返回的精确版本化测试秘密。
        secret: CredentialSecretEnvelope,
    }

    impl TestStaticSecretStore {
        /// Builds one deterministic in-memory test secret.
        /// 构建一个确定性的内存测试秘密。
        ///
        /// # Returns
        /// 返回值
        /// A store containing one revision-one API key envelope.
        /// 返回包含第一修订版 API Key 封装的存储。
        fn api_key() -> Self {
            Self {
                secret: CredentialSecretEnvelope {
                    version: 1,
                    revision: 1,
                    material: crate::CredentialSecretMaterial::ApiKey {
                        value: String::from("sk-sp-manifest-activation-test"),
                    },
                },
            }
        }
    }

    #[async_trait]
    impl CredentialSecretResolver for TestStaticSecretStore {
        /// Returns the immutable test secret for any exact reference.
        /// 为任意精确引用返回不可变测试秘密。
        async fn read_optional_secret(
            &self,
            _secret_reference: &str,
        ) -> Result<Option<CredentialSecretEnvelope>, String> {
            Ok(Some(self.secret.clone()))
        }
    }

    #[async_trait]
    impl CredentialSecretStore for TestStaticSecretStore {
        /// Rejects mutation because this fixture is intentionally immutable.
        /// 拒绝变更，因为该夹具被刻意设计为不可变。
        async fn create_secret(
            &self,
            _secret_reference: &str,
            _secret: &CredentialSecretEnvelope,
        ) -> Result<(), String> {
            Err(String::from("The test secret store is immutable."))
        }

        /// Rejects replacement because this fixture is intentionally immutable.
        /// 拒绝替换，因为该夹具被刻意设计为不可变。
        async fn replace_secret(
            &self,
            _secret_reference: &str,
            _expected_revision: u64,
            _secret: &CredentialSecretEnvelope,
        ) -> Result<(), String> {
            Err(String::from("The test secret store is immutable."))
        }

        /// Rejects deletion because this fixture is intentionally immutable.
        /// 拒绝删除，因为该夹具被刻意设计为不可变。
        async fn delete_secret(&self, _secret_reference: &str) -> Result<(), String> {
            Err(String::from("The test secret store is immutable."))
        }

        /// Rejects restoration because this fixture is intentionally immutable.
        /// 拒绝恢复，因为该夹具被刻意设计为不可变。
        async fn restore_secret(
            &self,
            _secret_reference: &str,
            _secret: &CredentialSecretEnvelope,
        ) -> Result<(), String> {
            Err(String::from("The test secret store is immutable."))
        }
    }

    /// Verifies static API keys remain terminal when an upstream request returns 401.
    /// 验证上游请求返回 401 时静态 API Key 仍保持终态。
    #[tokio::test]
    async fn execution_credential_lease_does_not_refresh_static_api_key() {
        // store owns one immutable user-managed API key.
        // store 持有一份不可变的用户管理 API Key。
        let store: Arc<dyn CredentialSecretStore> = Arc::new(TestStaticSecretStore::api_key());
        // lease uses the same production boundary as renewable credentials.
        // lease 使用与可续期凭据相同的生产边界。
        let lease = ProviderExecutionCredentialLease::new(
            String::from("api-key-connection"),
            String::from("api-key-secret"),
            Arc::new(OAuthAuthorizationRuntime::new(reqwest::Client::new())),
            Arc::new(StdMutex::new(BTreeMap::new())),
            store,
        );
        // secret proves the static key remains readable for ordinary dispatch.
        // secret 证明静态 Key 仍可供普通分派读取。
        let secret = lease
            .resolve_for_request()
            .await
            .expect("static API key should resolve");
        assert!(
            !lease
                .refresh_after_unauthorized(secret.revision())
                .await
                .expect("static API key recovery should be a controlled no-op")
        );
    }

    /// Counts every call that reaches the durable-store side of an isolated credential overlay.
    /// 统计每次触达隔离凭据覆盖层持久仓库一侧的调用。
    #[derive(Default)]
    struct TestCountingSecretStore {
        /// Number of delegated reads.
        /// 被委托读取次数。
        reads: AtomicUsize,
        /// Number of delegated creates.
        /// 被委托创建次数。
        creates: AtomicUsize,
        /// Number of delegated replacements.
        /// 被委托替换次数。
        replaces: AtomicUsize,
        /// Number of delegated deletes.
        /// 被委托删除次数。
        deletes: AtomicUsize,
        /// Number of delegated restorations.
        /// 被委托恢复次数。
        restores: AtomicUsize,
    }

    #[async_trait]
    impl CredentialSecretResolver for TestCountingSecretStore {
        /// Counts and rejects every durable read.
        /// 统计并拒绝每次持久读取。
        async fn read_optional_secret(
            &self,
            _secret_reference: &str,
        ) -> Result<Option<CredentialSecretEnvelope>, String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Err(String::from("The counting store rejects durable reads."))
        }
    }

    #[async_trait]
    impl CredentialSecretStore for TestCountingSecretStore {
        /// Counts and rejects every durable create.
        /// 统计并拒绝每次持久创建。
        async fn create_secret(
            &self,
            _secret_reference: &str,
            _secret: &CredentialSecretEnvelope,
        ) -> Result<(), String> {
            self.creates.fetch_add(1, Ordering::SeqCst);
            Err(String::from("The counting store rejects durable creates."))
        }

        /// Counts and rejects every durable replacement.
        /// 统计并拒绝每次持久替换。
        async fn replace_secret(
            &self,
            _secret_reference: &str,
            _expected_revision: u64,
            _secret: &CredentialSecretEnvelope,
        ) -> Result<(), String> {
            self.replaces.fetch_add(1, Ordering::SeqCst);
            Err(String::from(
                "The counting store rejects durable replacements.",
            ))
        }

        /// Counts and rejects every durable deletion.
        /// 统计并拒绝每次持久删除。
        async fn delete_secret(&self, _secret_reference: &str) -> Result<(), String> {
            self.deletes.fetch_add(1, Ordering::SeqCst);
            Err(String::from("The counting store rejects durable deletes."))
        }

        /// Counts and rejects every durable restoration.
        /// 统计并拒绝每次持久恢复。
        async fn restore_secret(
            &self,
            _secret_reference: &str,
            _secret: &CredentialSecretEnvelope,
        ) -> Result<(), String> {
            self.restores.fetch_add(1, Ordering::SeqCst);
            Err(String::from(
                "The counting store rejects durable restorations.",
            ))
        }
    }

    /// Verifies an isolated candidate can read and mutate its secret without durable-store calls.
    /// 验证隔离候选可以读取和修改自身秘密且不会调用持久仓库。
    #[tokio::test]
    async fn isolated_candidate_secret_never_reaches_durable_store() {
        // durableStore records any accidental boundary crossing.
        // durableStore 记录任何意外的边界穿透。
        let durable_store = Arc::new(TestCountingSecretStore::default());
        // initialSecret is the exact revision-one candidate supplied by a create request.
        // initialSecret 是创建请求提供的精确第一修订版候选。
        let initial_secret = CredentialSecretEnvelope::new_api_key(String::from("candidate-key"))
            .expect("candidate secret should validate");
        // isolatedStore shadows only the candidate reference.
        // isolatedStore 仅遮蔽候选引用。
        let isolated_store = IsolatedCredentialSecretStore::new(
            durable_store.clone(),
            String::from("candidate-secret"),
            initial_secret.clone(),
        );

        // observedSecret proves the overlay returns the exact in-memory envelope.
        // observedSecret 证明覆盖层返回精确的内存封装。
        let observed_secret = isolated_store
            .read_secret("candidate-secret")
            .await
            .expect("isolated read should succeed");
        assert!(observed_secret == initial_secret);
        // replacement advances only the in-memory revision.
        // replacement 仅推进内存修订版。
        let replacement = initial_secret
            .replace_api_key(String::from("replacement-key"))
            .expect("replacement secret should validate");
        isolated_store
            .replace_secret("candidate-secret", initial_secret.revision, &replacement)
            .await
            .expect("isolated replacement should succeed");
        isolated_store
            .delete_secret("candidate-secret")
            .await
            .expect("isolated deletion should succeed");
        isolated_store
            .restore_secret("candidate-secret", &replacement)
            .await
            .expect("isolated restoration should succeed");

        assert_eq!(durable_store.reads.load(Ordering::SeqCst), 0);
        assert_eq!(durable_store.creates.load(Ordering::SeqCst), 0);
        assert_eq!(durable_store.replaces.load(Ordering::SeqCst), 0);
        assert_eq!(durable_store.deletes.load(Ordering::SeqCst), 0);
        assert_eq!(durable_store.restores.load(Ordering::SeqCst), 0);
        assert!(
            isolated_store
                .read_optional_secret("unrelated-secret")
                .await
                .is_err(),
            "an unrelated reference must still delegate"
        );
        assert_eq!(durable_store.reads.load(Ordering::SeqCst), 1);
    }

    /// Verifies multi-connection staging publishes once and rejects a stale before-image.
    /// 验证多连接暂存只发布一次，并拒绝过期前镜像。
    #[test]
    fn staged_connection_observation_batch_is_atomic_and_cas_guarded() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // beforeImage is the only state accepted by the batch CAS.
        // beforeImage 是批次 CAS 接受的唯一状态。
        let before_image = runtime
            .capture_activation_snapshot()
            .expect("before image should capture");
        // firstCatalog and secondCatalog represent one complete validated credential batch.
        // firstCatalog 与 secondCatalog 表示一批完整的已验证凭据。
        let first_catalog =
            parse_openai_model_catalog("first-account", 1, r#"{"data":[{"id":"first-model"}]}"#)
                .expect("first catalog should parse");
        let second_catalog =
            parse_openai_model_catalog("second-account", 1, r#"{"data":[{"id":"second-model"}]}"#)
                .expect("second catalog should parse");
        // readyStatus freezes one deterministic closed discovery result.
        // readyStatus 冻结一个确定性的封闭发现结果。
        let ready_status = CatalogDiscoveryStatus {
            state: CatalogDiscoveryState::Ready,
            failure_kind: None,
            message: None,
            model_count: 1,
            generation: 1,
            observed_at_ms: 1,
        };
        let after_image = runtime
            .stage_connection_catalog_observations_if_current(
                &before_image,
                &[
                    StagedConnectionCatalogObservation {
                        connection_id: String::from("first-account"),
                        catalog: Some(first_catalog),
                        status: ready_status.clone(),
                        refresh_error: None,
                    },
                    StagedConnectionCatalogObservation {
                        connection_id: String::from("second-account"),
                        catalog: Some(second_catalog),
                        status: ready_status.clone(),
                        refresh_error: None,
                    },
                ],
            )
            .expect("atomic observation batch should stage");
        assert_eq!(
            after_image.state_revision,
            before_image.state_revision + 1,
            "the whole batch must publish exactly once"
        );
        assert!(after_image.source_catalogs.get("first-account").is_some());
        assert!(after_image.source_catalogs.get("second-account").is_some());

        let stale_error = runtime
            .stage_connection_catalog_observations_if_current(
                &before_image,
                &[StagedConnectionCatalogObservation {
                    connection_id: String::from("third-account"),
                    catalog: None,
                    status: ready_status,
                    refresh_error: None,
                }],
            )
            .err()
            .expect("a stale before image must fail closed");
        assert!(stale_error.contains("staging refused"), "{stale_error}");
        let retained_image = runtime
            .capture_activation_snapshot()
            .expect("retained image should capture");
        assert_eq!(retained_image.state_revision, after_image.state_revision);
        assert!(
            retained_image
                .source_catalogs
                .get("third-account")
                .is_none()
        );
    }

    /// Verifies runtime construction installs a process-level Rustls provider.
    /// 验证运行时构造会安装进程级 Rustls 加密提供程序。
    #[test]
    fn first_batch_installs_process_rustls_provider() {
        let _runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    /// Verifies an accepted custom-model template remains executable when discovery is unavailable after restart.
    /// 验证重启后模型发现不可用时，已接受的自定义模型模板仍可执行。
    #[test]
    fn confirmed_template_builds_manual_catalog_fallback_without_discovery() {
        let provider_id = "custom:test";
        let model_id = "gpt-5.6-luna";
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_model_template_decisions.insert(
            PersistedAiConfiguration::provider_model_key(provider_id, model_id),
            crate::CustomModelTemplateDecision::Applied {
                selection: crate::CustomModelTemplateSelection {
                    template_key: String::from("openai/gpt-5.6-luna"),
                    template_revision: String::from("common-model-capabilities.v1"),
                    context_limit: 200_000,
                    supports_text_input: true,
                    supports_text_output: true,
                    input_resources: vec![vulcan_ai_contract::ResourceKind::Image],
                    output_resources: Vec::new(),
                },
            },
        );

        let mut catalog =
            manual_custom_catalog_for_provider(&configuration, provider_id, "connection-test", 7)
                .expect("confirmed template should provide a restart-safe manual baseline");
        apply_model_catalog_overlays(&configuration, provider_id, &mut catalog)
            .expect("confirmed template baseline should resolve through normal overlays");

        assert_eq!(catalog.discovery_schema_revision, "manual.models.v1");
        assert_eq!(catalog.generation, 7);
        assert_eq!(catalog.models.len(), 1);
        let model = &catalog.models[0];
        assert_eq!(model.id, model_id);
        assert_eq!(model.display_name, model_id);
        assert_eq!(model.capabilities.context_limit, Some(200_000));
        assert!(
            model
                .capabilities
                .input_resources
                .contains(&vulcan_ai_contract::ResourceKind::Image)
        );
        assert!(
            model
                .capabilities
                .operations
                .contains(&AiOperation::ConversationRespond)
        );
    }

    /// Verifies rejected templates do not invent a fallback model identity.
    /// 验证被拒绝的模板不会虚构回退模型身份。
    #[test]
    fn rejected_template_does_not_build_manual_catalog_fallback() {
        let provider_id = "custom:test";
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_model_template_decisions.insert(
            PersistedAiConfiguration::provider_model_key(provider_id, "model-rejected"),
            crate::CustomModelTemplateDecision::Rejected {
                template_key: String::from("openai/model-rejected"),
                template_revision: String::from("common-model-capabilities.v1"),
            },
        );

        assert!(
            manual_custom_catalog_for_provider(&configuration, provider_id, "connection-test", 1,)
                .is_none()
        );
    }

    /// Builds one verified OpenRouter token connection.
    /// 构建一个已验证的 OpenRouter Token 连接。
    fn connection(id: &str) -> CredentialConnection {
        CredentialConnection {
            id: id.to_string(),
            provider_id: String::from("openrouter"),
            service_entry_id: String::from("public-api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: format!("credential-{id}"),
            display_name: id.to_string(),
            secret_reference: format!("secret-{id}"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        }
    }

    /// Builds one deterministic credential-scoped account-usage observation.
    /// 构建一份确定性的凭据作用域账号用量观测。
    ///
    /// # Parameters
    /// 参数
    /// `credential_id` identifies the account, `plan_name` and `plan_id` provide the two explicit
    /// identity channels, and `quota_windows` carries exact access-relevant balances.
    /// `credential_id` 标识账号，`plan_name` 与 `plan_id` 提供两条显式身份通道，
    /// `quota_windows` 承载精确的访问相关余额。
    ///
    /// # Returns
    /// 返回值
    /// A supported normalized usage snapshot.
    /// 返回一份受支持的归一化用量快照。
    fn account_usage(
        credential_id: &str,
        plan_name: Option<&str>,
        plan_id: Option<&str>,
        quota_windows: Vec<vulcan_ai_contract::AccountUsageWindow>,
    ) -> AccountUsageSnapshot {
        AccountUsageSnapshot {
            credential_id: credential_id.to_string(),
            availability: vulcan_ai_contract::AccountUsageAvailability::Supported,
            plan_name: plan_name.map(str::to_string),
            plan_id: plan_id.map(str::to_string),
            used_amount: None,
            remaining_amount: None,
            currency: None,
            balance_amount: None,
            quota_windows,
            observed_at: None,
            refresh_message: None,
        }
    }

    /// Builds one deterministic credential-scoped catalog.
    /// 构建一个确定性的凭据作用域目录。
    /// Adds one protocol variant per protocol id to every model of one catalog.
    /// 给一个目录的每个模型按协议 ID 各添加一个协议变体。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_profile_ids` are the exact protocols the models bind to; every model
    /// receives a variant for each id so system-manifest bindings can prove their exact
    /// variant before execution (plan §R5.1).
    /// `protocol_profile_ids` 是模型绑定的精确协议；每个模型为每个 ID 获得一个变体，
    /// 使系统清单绑定能够在执行前证明其精确变体（方案 §R5.1）。
    fn with_protocol_variants(
        mut catalog: ConnectionModelCatalog,
        protocol_profile_ids: &[&str],
    ) -> ConnectionModelCatalog {
        for model in &mut catalog.models {
            for protocol_profile_id in protocol_profile_ids {
                model.protocol_variants.insert(
                    (*protocol_profile_id).to_string(),
                    crate::ProtocolModelVariant {
                        capabilities: model.capabilities.clone(),
                        capability_evidence: Default::default(),
                        discovery_signature: String::from("test-signature"),
                        catalog_generation: catalog.generation,
                    },
                );
            }
        }
        catalog
    }

    fn catalog(connection_id: &str) -> ConnectionModelCatalog {
        with_protocol_variants(
            parse_openrouter_model_catalog(
                connection_id,
                1,
                r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"Model","context_length":8192,"supported_parameters":["tools","temperature"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
            )
            .expect("documented model fixture should parse"),
            &[
                OPENAI_CHAT_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
                ANTHROPIC_MESSAGES_PROFILE_ID,
            ],
        )
    }

    /// Verifies effective catalog revisions ignore refresh counters but retain semantic changes.
    /// 验证生效目录修订忽略刷新计数器，但保留语义变化。
    #[test]
    fn effective_catalog_revision_is_content_addressed() {
        // firstCatalog is the semantic baseline observed in generation one.
        // firstCatalog 是在第一代观测到的语义基线。
        let first_catalog = catalog("revision-connection");
        // refreshedCatalog reproduces the same catalog after another process-local refresh.
        // refreshedCatalog 表示另一次进程内刷新后得到的相同目录。
        let mut refreshed_catalog = first_catalog.clone();
        refreshed_catalog.generation = 41;
        for model in &mut refreshed_catalog.models {
            for variant in model.protocol_variants.values_mut() {
                variant.catalog_generation = 41;
            }
        }
        assert_eq!(
            effective_model_catalog_content_revision(&first_catalog)
                .expect("baseline catalog revision should serialize"),
            effective_model_catalog_content_revision(&refreshed_catalog)
                .expect("refreshed catalog revision should serialize"),
            "process-local generations must not change content identity"
        );

        // changedCatalog proves one execution-relevant capability still changes the digest.
        // changedCatalog 证明任一与执行相关的能力变化仍会改变摘要。
        let mut changed_catalog = refreshed_catalog;
        changed_catalog.models[0].capabilities.context_limit = Some(16_384);
        assert_ne!(
            effective_model_catalog_content_revision(&first_catalog)
                .expect("baseline catalog revision should serialize"),
            effective_model_catalog_content_revision(&changed_catalog)
                .expect("changed catalog revision should serialize"),
            "semantic catalog drift must change content identity"
        );
    }

    /// Verifies synthetic manual discovery signatures remain stable across catalog generations.
    /// 验证合成手工发现签名跨目录代次保持稳定。
    #[test]
    fn manual_model_discovery_signature_excludes_observation_generation() {
        // baselineModel owns explicit local evidence and one exact custom binding.
        // baselineModel 持有显式本地证据与一个精确自定义绑定。
        let baseline_model = ConnectionModel {
            id: String::from("manual-model"),
            display_name: String::from("Manual Model"),
            capabilities: ModelCapabilities::default(),
            capability_evidence: ModelCapabilityEvidence::default(),
            protocol_variants: BTreeMap::new(),
            resolved_protocol_binding: Some(crate::provider_manifest::ModelProtocolBinding {
                protocol_profile_id: OPENAI_RESPONSES_PROFILE_ID.to_string(),
                source: crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault,
                manifest_revision: 0,
                rule_id: String::new(),
            }),
        };
        // firstModel and refreshedModel differ only in their observation generation.
        // firstModel 与 refreshedModel 仅观测代次不同。
        let mut first_model = baseline_model.clone();
        let mut refreshed_model = baseline_model;
        ensure_manual_custom_bound_variant(
            "custom:manual-revision",
            "default",
            1,
            &mut first_model,
        );
        ensure_manual_custom_bound_variant(
            "custom:manual-revision",
            "default",
            99,
            &mut refreshed_model,
        );
        let first_variant = first_model
            .protocol_variants
            .get(OPENAI_RESPONSES_PROFILE_ID)
            .expect("first manual variant should exist");
        let refreshed_variant = refreshed_model
            .protocol_variants
            .get(OPENAI_RESPONSES_PROFILE_ID)
            .expect("refreshed manual variant should exist");
        assert_eq!(
            first_variant.discovery_signature, refreshed_variant.discovery_signature,
            "manual source identity must be content-addressed"
        );
        assert_ne!(
            first_variant.catalog_generation, refreshed_variant.catalog_generation,
            "observation metadata must still retain the actual refresh generation"
        );
    }

    /// Installs one immutable source catalog for candidate-overlay validation tests.
    /// 为候选覆盖校验测试安装一个不可变源目录。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the process-local catalog maps and `catalog` is the exact provider snapshot.
    /// `runtime` 持有进程内目录映射，`catalog` 是精确供应商快照。
    fn install_source_catalog(runtime: &PublicAiRuntime, catalog: ConnectionModelCatalog) {
        runtime
            .source_catalogs
            .write()
            .expect("source catalog lock should remain healthy")
            .replace(catalog);
    }

    /// Verifies isolated preparation remains invisible until one compare-and-swap publication.
    /// 验证隔离准备在一次比较交换发布前始终不可见。
    #[test]
    fn isolated_catalog_candidate_publishes_as_one_runtime_image() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let before_image = runtime
            .capture_activation_snapshot()
            .expect("before-image should capture");
        let isolated_runtime = runtime.isolated_catalog_activation_runtime(
            &before_image,
            Arc::clone(&before_image.resolved_provider_generation.manifests),
        );
        let catalog = parse_openai_model_catalog(
            "isolated-account",
            1,
            r#"{"data":[{"id":"isolated-model"}]}"#,
        )
        .expect("isolated catalog should parse");
        install_source_catalog(&isolated_runtime, catalog.clone());
        isolated_runtime
            .replace_catalog(catalog)
            .expect("isolated effective catalog should install");
        let candidate = isolated_runtime
            .capture_activation_snapshot()
            .expect("candidate should capture");

        assert!(
            runtime
                .staged_source_catalog("isolated-account")
                .expect("main staged source should read")
                .is_none(),
            "isolated preparation must not mutate the main runtime"
        );
        runtime
            .publish_isolated_catalog_candidate(&before_image, &candidate, None)
            .expect("isolated candidate should publish");
        assert!(
            runtime
                .staged_source_catalog("isolated-account")
                .expect("published staged source should read")
                .is_some(),
            "the complete candidate must become visible after the single publication"
        );
        assert_eq!(
            runtime.catalog_state_revision.load(Ordering::Acquire),
            before_image.state_revision + 1,
            "one complete publication must advance the main revision exactly once"
        );
    }

    /// Verifies compensation restores usage and refuses to overwrite a later runtime mutation.
    /// 验证补偿会恢复用量，并拒绝覆盖更晚发生的运行时变更。
    #[test]
    fn activation_compensation_is_complete_and_compare_and_swap_guarded() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // usage builds one exact provider-reported account snapshot for compensation assertions.
        // usage 构造一份精确的供应商账户快照，用于补偿断言。
        let usage = |plan_name: &str| AccountUsageSnapshot {
            credential_id: String::from("credential-compensation"),
            availability: vulcan_ai_contract::AccountUsageAvailability::Supported,
            plan_name: Some(plan_name.to_string()),
            plan_id: None,
            used_amount: None,
            remaining_amount: None,
            currency: None,
            balance_amount: None,
            quota_windows: Vec::new(),
            observed_at: None,
            refresh_message: None,
        };
        runtime
            .cache_usage_snapshot("compensation-account", usage("before"), false)
            .expect("before usage should cache");
        let before_image = runtime
            .capture_activation_snapshot()
            .expect("before-image should capture");
        runtime
            .cache_usage_snapshot("compensation-account", usage("activated"), false)
            .expect("activated usage should cache");
        let activated_image = runtime
            .capture_activation_snapshot()
            .expect("activated image should capture");
        runtime
            .restore_activation_snapshot(&before_image, &activated_image)
            .expect("matching compensation should restore");
        assert_eq!(
            runtime.usage_cache_snapshot().expect("usage snapshot")["compensation-account"]
                .plan_name
                .as_deref(),
            Some("before"),
            "runtime compensation must include plan-scoped usage evidence"
        );

        runtime
            .cache_usage_snapshot("compensation-account", usage("newer"), false)
            .expect("newer usage should cache");
        assert!(
            runtime
                .restore_activation_snapshot(&before_image, &activated_image)
                .expect_err("stale compensation must fail closed")
                .contains("compensation refused"),
            "a stale before-image must never overwrite newer runtime state"
        );
    }

    /// Verifies transient usage failures retain the last confirmed plan and quota access evidence.
    /// 验证暂时用量失败会保留最后一次确认的套餐与额度访问证据。
    #[test]
    fn transient_usage_failure_preserves_account_access_evidence() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let quota = vulcan_ai_contract::AccountUsageWindow {
            id: String::from("purchased-credits"),
            display_name: String::from("Purchased credits"),
            bucket_name: None,
            used: Some(String::from("0")),
            remaining: Some(String::from("1.5")),
            total: Some(String::from("1.5")),
            remaining_percent: None,
            unit: String::from("USD"),
            starts_at: None,
            resets_at: None,
            unlimited: false,
            available: true,
        };
        let confirmed = account_usage(
            "credential-access-account",
            Some("Go"),
            Some("individual-go"),
            vec![quota.clone()],
        );
        runtime
            .cache_usage_snapshot("access-account", confirmed, true)
            .expect("confirmed usage should cache");

        let mut transient = account_usage("credential-access-account", None, None, Vec::new());
        transient.availability =
            vulcan_ai_contract::AccountUsageAvailability::TemporarilyUnavailable;
        let retained = runtime
            .cache_usage_snapshot("access-account", transient, true)
            .expect("transient usage should preserve access evidence");

        assert_eq!(retained.plan_name.as_deref(), Some("Go"));
        assert_eq!(retained.plan_id.as_deref(), Some("individual-go"));
        assert_eq!(retained.quota_windows, vec![quota]);
        assert!(matches!(
            retained.availability,
            vulcan_ai_contract::AccountUsageAvailability::TemporarilyUnavailable
        ));
    }

    /// Verifies every client-side failure category maps without message parsing.
    /// 验证每一种客户端失败分类都无需解析消息即可完成映射。
    #[test]
    fn catalog_failure_mapping_preserves_every_typed_category() {
        for (source, expected) in [
            (
                ModelDiscoveryFailureKind::Unsupported,
                CatalogDiscoveryFailureKind::Unsupported,
            ),
            (
                ModelDiscoveryFailureKind::AuthenticationFailed,
                CatalogDiscoveryFailureKind::AuthenticationFailed,
            ),
            (
                ModelDiscoveryFailureKind::RateLimited,
                CatalogDiscoveryFailureKind::RateLimited,
            ),
            (
                ModelDiscoveryFailureKind::TemporarilyUnavailable,
                CatalogDiscoveryFailureKind::TemporarilyUnavailable,
            ),
            (
                ModelDiscoveryFailureKind::InvalidResponse,
                CatalogDiscoveryFailureKind::InvalidResponse,
            ),
            (
                ModelDiscoveryFailureKind::Failed,
                CatalogDiscoveryFailureKind::Failed,
            ),
        ] {
            assert_eq!(catalog_discovery_failure_kind(source), expected);
        }
    }

    /// Builds a runtime and two configured accounts that expose the same model.
    /// 构建一个运行时以及两个暴露相同模型的已配置账号。
    fn two_account_runtime() -> (PublicAiRuntime, PersistedAiConfiguration) {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection("one"),
            )
            .expect("first connection should register");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection("two"),
            )
            .expect("second connection should register");
        let first_catalog = catalog("one");
        install_source_catalog(&runtime, first_catalog.clone());
        runtime
            .replace_catalog(first_catalog)
            .expect("first catalog should install");
        let second_catalog = catalog("two");
        install_source_catalog(&runtime, second_catalog.clone());
        runtime
            .replace_catalog(second_catalog)
            .expect("second catalog should install");
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("two-account generation should publish");
        (runtime, configuration)
    }

    /// Verifies unknown text models use the host's enabled reasoning default in public capability views.
    /// 验证未知文本模型在公共能力视图中使用宿主默认的开启思考策略。
    #[test]
    fn unknown_text_model_reasoning_is_enabled_by_default() {
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            reasoning: CapabilitySupport::Unknown,
            reasoning_control: ReasoningControlMode::Unknown,
            reasoning_efforts: Vec::new(),
            ..ModelCapabilities::default()
        };

        assert!(model_supports_reasoning(&capabilities));

        // The discovery fixture intentionally omits reasoning metadata, matching the real
        // `/models` payload that previously surfaced as Unsupported/FixedOff.
        // 发现夹具故意省略思考元数据，模拟此前会被显示为 Unsupported/FixedOff 的真实 `/models` 响应。
        let discovered = catalog("one")
            .models
            .into_iter()
            .next()
            .expect("model fixture should exist");
        assert!(model_supports_reasoning(&discovered.capabilities));
    }

    /// Verifies an explicit fixed-off contract remains authoritative over the host default.
    /// 验证明确固定关闭契约仍然高于宿主默认策略。
    #[test]
    fn explicit_fixed_off_reasoning_remains_disabled() {
        let mut capabilities = catalog("one")
            .models
            .into_iter()
            .next()
            .expect("model fixture should exist")
            .capabilities;
        capabilities.reasoning = CapabilitySupport::Unsupported;
        capabilities.reasoning_control = ReasoningControlMode::FixedOff;
        capabilities.reasoning_efforts = vec![vulcan_ai_contract::ReasoningEffort::None];

        assert!(!model_supports_reasoning(&capabilities));
    }

    /// Verifies a fixed-off declaration without stale effort entries is still authoritative.
    /// 验证即使没有残留档位，固定关闭声明仍然具有权威性。
    #[test]
    fn explicit_fixed_off_without_efforts_remains_disabled() {
        let capabilities = ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            reasoning: CapabilitySupport::Unsupported,
            reasoning_control: ReasoningControlMode::FixedOff,
            ..ModelCapabilities::default()
        };

        assert!(!model_supports_reasoning(&capabilities));
    }

    /// Verifies execution resolves persisted reasoning overrides instead of a stale mutable catalog copy.
    /// 验证执行解析使用持久化思考覆盖，而不是旧的可变目录副本。
    #[test]
    fn selection_uses_persisted_reasoning_override_when_runtime_catalog_is_stale() {
        let (runtime, mut configuration) = two_account_runtime();
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("openrouter", "openai/gpt-5.6-sol"),
            crate::ModelCapabilityOverrides {
                reasoning: Some(CapabilitySupport::Native),
                reasoning_control: Some(ReasoningControlMode::EffortSelection),
                reasoning_efforts: Some(vec![
                    vulcan_ai_contract::ReasoningEffort::None,
                    vulcan_ai_contract::ReasoningEffort::Medium,
                ]),
                request_projection: Some(crate::CustomModelRequestProjection {
                    reasoning: BTreeMap::from([(
                        vulcan_ai_contract::ReasoningEffort::Medium,
                        crate::CustomRequestMutation {
                            set: BTreeMap::from([(
                                String::from("thinking.type"),
                                serde_json::Value::String(String::from("enabled")),
                            )]),
                            delete: vec![String::from("reasoning_effort")],
                        },
                    )]),
                    ..crate::CustomModelRequestProjection::default()
                }),
                ..crate::ModelCapabilityOverrides::default()
            },
        );

        let mut stale_catalog = catalog("one");
        let stale_model = stale_catalog
            .models
            .first_mut()
            .expect("model fixture should exist");
        stale_model.capabilities.reasoning = CapabilitySupport::Unsupported;
        stale_model.capabilities.reasoning_control = ReasoningControlMode::FixedOff;
        stale_model.capabilities.reasoning_efforts =
            vec![vulcan_ai_contract::ReasoningEffort::None];
        runtime
            .replace_catalog(stale_catalog)
            .expect("stale catalog should install");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("reasoning override generation should publish");

        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "openrouter",
                "openai/gpt-5.6-sol",
            )
            .expect("selection should use the persisted capability override");
        assert_eq!(
            selection.model.capabilities.reasoning,
            CapabilitySupport::Native
        );
        assert_eq!(
            selection.model.capabilities.reasoning_control,
            ReasoningControlMode::EffortSelection
        );
        assert_eq!(
            selection.model.capabilities.reasoning_efforts,
            vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Medium,
            ]
        );
        let projection = selection
            .route
            .custom_model_projection
            .as_ref()
            .expect("persisted request projection should reach the execution route");
        let medium = projection
            .reasoning
            .get(&vulcan_ai_contract::ReasoningEffort::Medium)
            .expect("medium reasoning projection should remain available");
        assert_eq!(
            medium.set.get("thinking.type"),
            Some(&serde_json::Value::String(String::from("enabled")))
        );
        assert_eq!(medium.delete, vec![String::from("reasoning_effort")]);
    }

    /// Verifies rebuilding user overlays never mutates the immutable synchronized catalog snapshot.
    /// 验证重建用户覆盖绝不会修改不可变的已同步目录快照。
    #[test]
    fn model_overlay_rebuild_preserves_source_catalog() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection("one"),
            )
            .expect("connection should register");
        let source = catalog("one");
        runtime
            .source_catalogs
            .write()
            .expect("source catalog lock should remain healthy")
            .replace(source.clone());
        runtime
            .replace_catalog(source)
            .expect("effective source catalog should install");
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("openrouter", "openai/gpt-5.6-sol"),
            crate::ModelCapabilityOverrides {
                context_limit: Some(4_096),
                output_limit: None,
                input_resources: Some(vec![vulcan_ai_contract::ResourceKind::Image]),
                output_resources: None,
                ..crate::ModelCapabilityOverrides::default()
            },
        );

        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("overlay rebuild should succeed");

        let immutable_context = runtime
            .source_catalogs
            .read()
            .expect("source catalog lock should remain healthy")
            .get("one")
            .and_then(|catalog| catalog.models.first())
            .and_then(|model| model.capabilities.context_limit);
        let effective = runtime
            .catalog_snapshot()
            .expect("effective catalog should remain readable");
        let effective_model = effective
            .get("one")
            .and_then(|catalog| catalog.models.first())
            .expect("effective model should exist");
        assert_eq!(immutable_context, Some(8_192));
        assert_eq!(effective_model.capabilities.context_limit, Some(4_096));
        assert_eq!(
            effective_model.capabilities.input_resources,
            vec![vulcan_ai_contract::ResourceKind::Image]
        );
    }

    /// Verifies disabling one account-model pair leaves a sibling account and source evidence unchanged.
    /// 验证禁用一个账号模型组合不会影响同供应商其他账号或原始证据。
    #[test]
    fn connection_model_access_isolated_between_accounts() {
        let (runtime, mut configuration) = two_account_runtime();
        configuration.set_connection_model_enabled("one", "openai/gpt-5.6-sol", false);

        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("account access overlay should resolve");

        let effective = runtime
            .catalog_snapshot()
            .expect("effective catalogs should remain readable");
        assert!(
            effective
                .get("one")
                .expect("first account catalog should exist")
                .models
                .is_empty()
        );
        assert_eq!(
            effective
                .get("two")
                .expect("second account catalog should exist")
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["openai/gpt-5.6-sol"]
        );

        let source = runtime
            .source_catalog_snapshot()
            .expect("source catalogs should remain readable");
        assert_eq!(
            source
                .get("one")
                .expect("first source catalog should exist")
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["openai/gpt-5.6-sol"]
        );
    }

    /// Verifies a disabled account default applies to a model discovered after the decision.
    /// 验证禁用账号默认值会应用到决定之后才发现的模型。
    #[test]
    fn disabled_connection_default_hides_future_discovery() {
        let (runtime, mut configuration) = two_account_runtime();
        configuration.set_connection_models_enabled("one", false);
        let future_catalog = parse_openrouter_model_catalog(
            "one",
            2,
            r#"{"data":[{"id":"vendor/future-model","name":"Future Model","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
        )
        .expect("future model fixture should parse");
        install_source_catalog(&runtime, future_catalog.clone());
        runtime
            .replace_catalog(future_catalog)
            .expect("future source catalog should install");

        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("future model overlay should resolve");
        assert!(
            runtime
                .catalog_snapshot()
                .expect("effective catalogs should remain readable")
                .get("one")
                .expect("first account catalog should exist")
                .models
                .is_empty()
        );

        configuration.set_connection_model_enabled("one", "vendor/future-model", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("explicit future model enable should resolve");
        assert_eq!(
            runtime
                .catalog_snapshot()
                .expect("effective catalogs should remain readable")
                .get("one")
                .expect("first account catalog should exist")
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["vendor/future-model"]
        );
    }

    /// Verifies candidate capability differences are checked after inherited limits resolve.
    /// 验证候选能力差异会在继承限制解析完成后接受校验。
    #[test]
    fn capability_override_candidate_validates_final_resolved_limits() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection("one"),
            )
            .expect("connection should register");
        install_source_catalog(&runtime, catalog("one"));

        for (overrides, expected_error) in [
            (
                crate::ModelCapabilityOverrides {
                    output_limit: Some(0),
                    ..crate::ModelCapabilityOverrides::default()
                },
                "output limit must be greater than zero",
            ),
            (
                crate::ModelCapabilityOverrides {
                    context_limit: Some(2_048),
                    ..crate::ModelCapabilityOverrides::default()
                },
                "effective output limit that exceeds its context limit",
            ),
            (
                crate::ModelCapabilityOverrides {
                    output_limit: Some(9_000),
                    ..crate::ModelCapabilityOverrides::default()
                },
                "effective output limit that exceeds its context limit",
            ),
        ] {
            let error = runtime
                .validate_model_capability_override_candidate(
                    &configuration,
                    "openrouter",
                    "openai/gpt-5.6-sol",
                    Some(&overrides),
                )
                .expect_err("invalid final limits must be rejected");
            assert!(
                error.contains(expected_error),
                "unexpected validation error: {error}"
            );
        }

        runtime
            .validate_model_capability_override_candidate(
                &configuration,
                "openrouter",
                "openai/gpt-5.6-sol",
                Some(&crate::ModelCapabilityOverrides {
                    context_limit: Some(8_192),
                    output_limit: Some(4_096),
                    ..crate::ModelCapabilityOverrides::default()
                }),
            )
            .expect("valid final limits should be accepted");
    }

    /// Verifies provider-level model management ignores account visibility while preserving route validation.
    /// 验证供应商级模型管理忽略账号可见性，同时保留路由校验。
    #[test]
    fn provider_model_management_uses_support_catalog_when_account_is_disabled() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection("one"),
            )
            .expect("connection should register");
        install_source_catalog(&runtime, catalog("one"));
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("support catalog should cross the explicit activation boundary");
        configuration.set_connection_models_enabled("one", false);

        assert!(
            runtime
                .provider_model_is_supported(&configuration, "openrouter", "openai/gpt-5.6-sol")
                .expect("provider support should resolve")
        );
        runtime
            .validate_model_capability_override_candidate(
                &configuration,
                "openrouter",
                "openai/gpt-5.6-sol",
                Some(&crate::ModelCapabilityOverrides {
                    context_limit: Some(8_192),
                    output_limit: Some(4_096),
                    ..crate::ModelCapabilityOverrides::default()
                }),
            )
            .expect("account visibility must not block provider capability settings");
        runtime
            .validate_model_execution_override_candidate(
                &configuration,
                "openrouter",
                "openai/gpt-5.6-sol",
                OPENAI_RESPONSES_PROFILE_ID,
                &crate::ModelExecutionOverrides {
                    prompt_cache: Some(vulcan_ai_contract::PromptCachePolicy::ProviderDefault),
                    ..crate::ModelExecutionOverrides::default()
                },
            )
            .expect("account visibility must not block the frozen model protocol settings");
    }

    /// Verifies a credential-free custom provider can retain direct settings for its manual model.
    /// 验证无凭据自定义供应商可以保留其手工模型的直接设置。
    #[test]
    fn credential_free_manual_model_accepts_direct_capability_settings() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = custom_definition_with_discovery_mode(
            "custom:test",
            &[crate::CustomProviderProtocol::OpenAiChat],
            crate::CustomProviderModelDiscoveryMode::Manual,
        );
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider);
        configuration.custom_models.insert(
            String::from("custom:test"),
            BTreeMap::from([(
                String::from("private/chat"),
                crate::CustomModelDefinition {
                    id: String::from("private/chat"),
                    display_name: String::from("Private Chat"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        ..ModelCapabilities::default()
                    },
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            )]),
        );

        runtime
            .validate_model_capability_override_candidate(
                &configuration,
                "custom:test",
                "private/chat",
                Some(&crate::ModelCapabilityOverrides {
                    context_limit: Some(8_192),
                    output_limit: Some(1_024),
                    ..crate::ModelCapabilityOverrides::default()
                }),
            )
            .expect("manual provider-only validation should not require a credential");
    }

    /// Verifies a provider-wide difference must remain valid for every credential source.
    /// 验证供应商级差异必须对每个凭据源都保持有效。
    #[test]
    fn capability_override_candidate_validates_every_account() {
        let (runtime, configuration) = two_account_runtime();
        install_source_catalog(&runtime, catalog("one"));
        install_source_catalog(
            &runtime,
            with_protocol_variants(
                parse_openrouter_model_catalog(
                    "two",
                    1,
                    r#"{"data":[{"id":"openai/gpt-5.6-sol","name":"Model","context_length":2048,"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":1024}}]}"#,
                )
                .expect("second account source should parse"),
                &[
                    OPENAI_CHAT_PROFILE_ID,
                    OPENAI_RESPONSES_PROFILE_ID,
                    ANTHROPIC_MESSAGES_PROFILE_ID,
                ],
            ),
        );

        let error = runtime
            .validate_model_capability_override_candidate(
                &configuration,
                "openrouter",
                "openai/gpt-5.6-sol",
                Some(&crate::ModelCapabilityOverrides {
                    output_limit: Some(3_000),
                    ..crate::ModelCapabilityOverrides::default()
                }),
            )
            .expect_err("an override invalid for the second account must be rejected");
        assert!(
            error.contains("connection `two`"),
            "unexpected error: {error}"
        );
    }

    /// Verifies ordinary catalog rebuilds also refuse an already persisted invalid difference.
    /// 验证常规目录重建也会拒绝已经持久化的无效差异。
    #[test]
    fn model_overlay_rebuild_rejects_invalid_final_limits() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection("one"),
            )
            .expect("connection should register");
        install_source_catalog(&runtime, catalog("one"));
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("openrouter", "openai/gpt-5.6-sol"),
            crate::ModelCapabilityOverrides {
                output_limit: Some(9_000),
                ..crate::ModelCapabilityOverrides::default()
            },
        );

        let error = runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect_err("invalid final limits must stop the rebuild");

        assert!(error.contains("exceeds its context limit"));
    }

    /// Verifies manual models remain usable when a provider has no discovered source catalog.
    /// 验证供应商没有发现源目录时手工模型仍然可用。
    #[test]
    fn manual_custom_model_builds_catalog_without_provider_discovery() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = custom_definition_with_discovery_mode(
            "custom:manual-catalog",
            &[crate::CustomProviderProtocol::OpenAiChat],
            crate::CustomProviderModelDiscoveryMode::Manual,
        );
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider);
        configuration.connection_graph.connections.insert(
            String::from("one"),
            custom_connection("one", "custom:manual-catalog"),
        );
        configuration
            .custom_models
            .entry(String::from("custom:manual-catalog"))
            .or_default()
            .insert(
                String::from("manual/chat"),
                crate::CustomModelDefinition {
                    id: String::from("manual/chat"),
                    display_name: String::from("Manual Chat"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        context_limit: Some(200_000),
                        ..ModelCapabilities::default()
                    },
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            );

        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("manual-only catalog should rebuild");

        assert!(
            runtime
                .source_catalogs
                .read()
                .expect("source catalog lock should remain healthy")
                .get("one")
                .is_none(),
            "manual fallback must not masquerade as provider discovery"
        );
        let catalogs = runtime
            .catalog_snapshot()
            .expect("effective catalog should remain readable");
        let model = catalogs
            .get("one")
            .and_then(|catalog| catalog.models.first())
            .expect("manual model should be installed");
        assert_eq!(model.id, "manual/chat");
        assert_eq!(model.capabilities.context_limit, Some(200_000));
    }

    /// Verifies an activated custom catalog preserves discovery evidence and adds manual identities.
    /// 验证已激活的自定义目录保留发现证据并添加手工模型身份。
    #[test]
    fn activated_custom_catalog_publishes_discovery_and_manual_union() {
        // runtime owns separate immutable source and activated effective generations.
        // runtime 分别持有不可变来源与已激活有效代际。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // configuration declares remote discovery and one independent manual identity.
        // configuration 声明远程发现与一个独立手工模型身份。
        let mut configuration = PersistedAiConfiguration::default();
        // provider uses the standard remote discovery contract, which resolves to the explicit
        // discovery-and-manual source policy.
        // provider 使用标准远程发现契约，其会解析为显式的发现与手工并集策略。
        let provider = custom_definition_with_discovery_mode(
            "custom:merged-catalog",
            &[crate::CustomProviderProtocol::OpenAiChat],
            crate::CustomProviderModelDiscoveryMode::Standard,
        );
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider);
        configuration.connection_graph.connections.insert(
            String::from("merged-account"),
            custom_connection("merged-account", "custom:merged-catalog"),
        );
        configuration
            .custom_models
            .entry(String::from("custom:merged-catalog"))
            .or_default()
            .insert(
                String::from("manual/chat"),
                crate::CustomModelDefinition {
                    id: String::from("manual/chat"),
                    display_name: String::from("Manual Chat"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        ..ModelCapabilities::default()
                    },
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            );
        // sourceCatalog contains only provider-observed identity evidence.
        // sourceCatalog 仅包含供应商观测到的身份证据。
        let source_catalog = crate::parse_openai_model_catalog(
            "merged-account",
            7,
            r#"{"data":[{"id":"discovered/chat"}]}"#,
        )
        .expect("discovery catalog should parse");
        install_source_catalog(&runtime, source_catalog);

        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("merged catalog should activate");

        // activatedCatalog is the immutable reader-visible union after the explicit boundary.
        // activatedCatalog 是跨过显式边界后读取方可见的不可变并集。
        let activated_catalog = runtime
            .catalog_snapshot()
            .expect("activated catalog should remain readable")
            .remove("merged-account")
            .expect("merged account catalog should exist");
        assert!(
            activated_catalog
                .models
                .iter()
                .any(|model| model.id == "discovered/chat")
        );
        assert!(
            activated_catalog
                .models
                .iter()
                .any(|model| model.id == "manual/chat")
        );
        // retainedSource proves the manual identity did not masquerade as discovery evidence.
        // retainedSource 证明手工身份没有伪装成发现证据。
        let retained_source = runtime
            .source_catalogs
            .read()
            .expect("source catalog lock should remain healthy")
            .get("merged-account")
            .cloned()
            .expect("discovery source should remain available");
        assert_eq!(retained_source.models.len(), 1);
        assert_eq!(retained_source.models[0].id, "discovered/chat");
    }

    /// Verifies removing a manual-only definition removes the model from its exact connection.
    /// 验证移除仅手工定义后模型会从其精确连接消失。
    #[test]
    fn connection_catalog_membership_tracks_manual_definition_removal() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = custom_definition_with_discovery_mode(
            "custom:test",
            &[crate::CustomProviderProtocol::OpenAiChat],
            crate::CustomProviderModelDiscoveryMode::Manual,
        );
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider);
        let custom_connection = custom_connection("custom-account", "custom:test");
        configuration
            .connection_graph
            .connections
            .insert(custom_connection.id.clone(), custom_connection);
        configuration
            .custom_models
            .entry(String::from("custom:test"))
            .or_default()
            .insert(
                String::from("manual/chat"),
                crate::CustomModelDefinition {
                    id: String::from("manual/chat"),
                    display_name: String::from("Manual Chat"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        context_limit: Some(200_000),
                        ..ModelCapabilities::default()
                    },
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            );

        assert!(
            runtime
                .connection_catalog_contains_model(&configuration, "custom-account", "manual/chat",)
                .expect("manual catalog membership should resolve")
        );
        configuration.custom_models.remove("custom:test");
        assert!(
            !runtime
                .connection_catalog_contains_model(&configuration, "custom-account", "manual/chat",)
                .expect("removed manual model should resolve as absent")
        );
    }

    /// Verifies removing a user overlay does not hide a model retained by immutable discovery.
    /// 验证移除用户覆盖不会隐藏不可变发现目录仍保留的模型。
    #[test]
    fn connection_catalog_membership_retains_discovered_model_after_overlay_removal() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = multi_protocol_custom_definition(
            "custom:test",
            &[crate::CustomProviderProtocol::OpenAiChat],
        );
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider);
        let custom_connection = custom_connection("custom-account", "custom:test");
        configuration
            .connection_graph
            .connections
            .insert(custom_connection.id.clone(), custom_connection);
        let source = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"openai/gpt-5.6-sol"}]}"#,
        )
        .expect("compatible source catalog should parse");
        install_source_catalog(&runtime, source);
        configuration
            .custom_models
            .entry(String::from("custom:test"))
            .or_default()
            .insert(
                String::from("openai/gpt-5.6-sol"),
                crate::CustomModelDefinition {
                    id: String::from("openai/gpt-5.6-sol"),
                    display_name: String::from("Reviewed Model"),
                    enabled: true,
                    capabilities: ModelCapabilities::default(),
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            );

        configuration.custom_models.remove("custom:test");
        assert!(
            runtime
                .connection_catalog_contains_model(
                    &configuration,
                    "custom-account",
                    "openai/gpt-5.6-sol",
                )
                .expect("source-backed membership should resolve")
        );
    }

    /// Verifies an ID-only custom catalog receives the documented provider defaults.
    /// 验证仅含 ID 的自定义目录会获得文档规定的供应商默认值。
    #[test]
    fn custom_catalog_uses_two_hundred_thousand_and_thirty_two_thousand_defaults() {
        let configuration = PersistedAiConfiguration::default();
        let mut catalog = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"private/chat"}]}"#,
        )
        .expect("ID-only compatible catalog should parse");

        apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
            .expect("custom defaults should resolve");

        let model = catalog.models.first().expect("model should remain visible");
        assert_eq!(model.capabilities.context_limit, Some(200_000));
        assert_eq!(model.capabilities.output_limit, Some(32_768));
        assert_eq!(
            model.capabilities.reasoning,
            vulcan_ai_contract::CapabilitySupport::Native
        );
        assert_eq!(
            model.capabilities.reasoning_control,
            vulcan_ai_contract::ReasoningControlMode::EffortSelection
        );
        assert_eq!(
            model.capabilities.reasoning_efforts,
            vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Minimal,
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::XHigh,
                vulcan_ai_contract::ReasoningEffort::Max,
            ]
        );
        assert!(
            model
                .capabilities
                .operations
                .contains(&AiOperation::ConversationRespond)
        );
    }

    /// Verifies unknown compatible text models receive the host default reasoning contract.
    /// 验证未知兼容文本模型会获得宿主默认思考契约。
    #[test]
    fn unknown_compatible_conversation_defaults_to_enabled_reasoning() {
        let configuration = PersistedAiConfiguration::default();
        let mut catalog = crate::parse_openai_model_catalog(
            "apikeyfun-account",
            1,
            r#"{"data":[{"id":"gpt-5.6-luna"}]}"#,
        )
        .expect("compatible source catalog should parse");

        apply_model_catalog_overlays(&configuration, "apikeyfun", &mut catalog)
            .expect("host reasoning defaults should resolve");

        let model = catalog.models.first().expect("model should remain visible");
        assert_eq!(model.capabilities.reasoning, CapabilitySupport::Native);
        assert_eq!(
            model.capabilities.reasoning_control,
            ReasoningControlMode::EffortSelection
        );
        assert_eq!(
            model.capabilities.reasoning_efforts,
            crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS
        );
        assert!(
            model
                .capabilities
                .evidence_revision
                .contains(HOST_DEFAULT_REASONING_EVIDENCE_REVISION)
        );
    }

    /// Verifies a legacy Native-only declaration receives the same host default levels.
    /// 验证只有 Native 支持标记的旧声明也会获得相同的宿主默认档位。
    #[test]
    fn native_only_conversation_declaration_defaults_to_enabled_reasoning() {
        let configuration = PersistedAiConfiguration::default();
        let mut catalog = crate::parse_openai_model_catalog(
            "apikeyfun-account",
            1,
            r#"{"data":[{"id":"gpt-5.6-luna"}]}"#,
        )
        .expect("compatible source catalog should parse");
        let model = catalog
            .models
            .first_mut()
            .expect("compatible model should exist");
        model.capabilities.reasoning = CapabilitySupport::Native;

        apply_model_catalog_overlays(&configuration, "apikeyfun", &mut catalog)
            .expect("partial reasoning declaration should resolve");

        let model = catalog.models.first().expect("model should remain visible");
        assert_eq!(model.capabilities.reasoning, CapabilitySupport::Native);
        assert_eq!(
            model.capabilities.reasoning_control,
            ReasoningControlMode::EffortSelection
        );
        assert_eq!(
            model.capabilities.reasoning_efforts,
            crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS
        );
    }

    /// Verifies an explicit model-level disablement outranks the host default reasoning policy.
    /// 验证显式模型级关闭配置优先于宿主默认思考策略。
    #[test]
    fn explicit_model_reasoning_disablement_outranks_host_default() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("apikeyfun", "gpt-5.6-luna"),
            crate::ModelCapabilityOverrides {
                reasoning: Some(CapabilitySupport::Unsupported),
                reasoning_control: Some(ReasoningControlMode::FixedOff),
                reasoning_efforts: Some(vec![vulcan_ai_contract::ReasoningEffort::None]),
                ..crate::ModelCapabilityOverrides::default()
            },
        );
        let mut catalog = crate::parse_openai_model_catalog(
            "apikeyfun-account",
            1,
            r#"{"data":[{"id":"gpt-5.6-luna"}]}"#,
        )
        .expect("compatible source catalog should parse");

        apply_model_catalog_overlays(&configuration, "apikeyfun", &mut catalog)
            .expect("explicit disablement should resolve");

        let model = catalog.models.first().expect("model should remain visible");
        assert_eq!(model.capabilities.reasoning, CapabilitySupport::Unsupported);
        assert_eq!(
            model.capabilities.reasoning_control,
            ReasoningControlMode::FixedOff
        );
        assert_eq!(
            model.capabilities.reasoning_efforts,
            vec![vulcan_ai_contract::ReasoningEffort::None]
        );
    }

    /// Verifies an explicit model-level enablement reaches the effective catalog even when discovery says off.
    /// 验证显式模型级开启配置会抵达有效目录，即使发现结果声明模型关闭思考。
    #[test]
    fn explicit_model_reasoning_enablement_outranks_disabled_discovery() {
        let model_id = "gpt-5.6-luna";
        let mut configuration = PersistedAiConfiguration::default();
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("apikeyfun", model_id),
            crate::ModelCapabilityOverrides {
                reasoning: Some(CapabilitySupport::Native),
                reasoning_control: Some(ReasoningControlMode::EffortSelection),
                reasoning_efforts: Some(crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS.to_vec()),
                ..crate::ModelCapabilityOverrides::default()
            },
        );
        let mut catalog = crate::parse_openai_model_catalog(
            "apikeyfun-account",
            1,
            r#"{"data":[{"id":"gpt-5.6-luna"}]}"#,
        )
        .expect("compatible source catalog should parse");
        let source_capabilities = &mut catalog
            .models
            .first_mut()
            .expect("source model should exist")
            .capabilities;
        source_capabilities.reasoning = CapabilitySupport::Unsupported;
        source_capabilities.reasoning_control = ReasoningControlMode::FixedOff;
        source_capabilities.reasoning_efforts = vec![vulcan_ai_contract::ReasoningEffort::None];

        apply_model_catalog_overlays(&configuration, "apikeyfun", &mut catalog)
            .expect("explicit enablement should resolve");

        let model = catalog.models.first().expect("model should remain visible");
        assert_eq!(model.capabilities.reasoning, CapabilitySupport::Native);
        assert_eq!(
            model.capabilities.reasoning_control,
            ReasoningControlMode::EffortSelection
        );
        assert_eq!(
            model.capabilities.reasoning_efforts,
            crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS
        );
    }

    /// Verifies direct provider settings outrank catalog values inside the text-conversation contract.
    /// 验证直接供应商设置会在文本会话契约内覆盖目录值。
    #[test]
    fn provider_settings_outrank_catalog_values_within_text_contract() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_provider_model_defaults.insert(
            String::from("custom:test"),
            crate::CustomProviderModelDefaults {
                context_limit: 200_000,
                output_limit: 32_768,
                input_resources: vec![vulcan_ai_contract::ResourceKind::Image],
                ..crate::CustomProviderModelDefaults::default()
            },
        );
        configuration.custom_model_template_decisions.insert(
            PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
            crate::CustomModelTemplateDecision::Applied {
                selection: crate::CustomModelTemplateSelection {
                    template_key: String::from("openai/gpt-5.2"),
                    template_revision: String::from(crate::CUSTOM_MODEL_TEMPLATE_REVISION),
                    context_limit: 100_000,
                    supports_text_input: true,
                    supports_text_output: true,
                    input_resources: vec![vulcan_ai_contract::ResourceKind::File],
                    output_resources: Vec::new(),
                },
            },
        );
        let mut catalog = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"private/chat","context_window":64000,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("explicit compatible catalog should parse");

        apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
            .expect("custom evidence should resolve");

        let model = catalog.models.first().expect("model should remain visible");
        assert_eq!(model.capabilities.context_limit, Some(200_000));
        assert_eq!(model.capabilities.output_limit, Some(32_768));
        assert_eq!(
            model.capabilities.input_resources,
            vec![vulcan_ai_contract::ResourceKind::Image]
        );
        assert!(model.capabilities.output_resources.is_empty());
        assert!(
            model
                .capabilities
                .operations
                .contains(&AiOperation::ConversationRespond)
        );
    }

    /// Verifies custom providers do not publish resource-producing models as generic conversation choices.
    /// 验证自定义供应商不会把资源生成模型发布为通用会话选项。
    #[test]
    fn custom_provider_filters_explicit_non_text_output_models() {
        let configuration = PersistedAiConfiguration::default();
        let mut catalog = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"private/image","input_modalities":["text"],"output_modalities":["image"]},{"id":"private/mixed","input_modalities":["text"],"output_modalities":["text","image"]}]}"#,
        )
        .expect("explicit non-text model should parse");

        apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
            .expect("custom conversation contract should resolve");

        assert_eq!(catalog.models.len(), 1);
        let model = &catalog.models[0];
        assert_eq!(model.id, "private/mixed");
        assert_eq!(
            model.capabilities.operations,
            vec![AiOperation::ConversationRespond]
        );
        assert!(model.capabilities.output_resources.is_empty());
    }

    /// Verifies system reasoning defaults outrank conservative source flags.
    /// 验证系统思考默认值会覆盖保守来源标记。
    #[test]
    fn custom_provider_system_baseline_enables_reasoning() {
        let configuration = PersistedAiConfiguration::default();
        let mut catalog = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"private/chat"}]}"#,
        )
        .expect("compatible catalog should parse");
        let source_capabilities = &mut catalog
            .models
            .first_mut()
            .expect("source model should exist")
            .capabilities;
        source_capabilities.reasoning = vulcan_ai_contract::CapabilitySupport::Unsupported;
        source_capabilities.reasoning_control = vulcan_ai_contract::ReasoningControlMode::FixedOff;
        source_capabilities.reasoning_efforts = vec![vulcan_ai_contract::ReasoningEffort::None];
        apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
            .expect("system baseline should resolve");

        let capabilities = &catalog
            .models
            .first()
            .expect("model should remain visible")
            .capabilities;
        assert_eq!(
            capabilities.reasoning,
            vulcan_ai_contract::CapabilitySupport::Native
        );
        assert_eq!(
            capabilities.reasoning_control,
            vulcan_ai_contract::ReasoningControlMode::EffortSelection
        );
        assert_eq!(
            capabilities.reasoning_efforts,
            vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Minimal,
                vulcan_ai_contract::ReasoningEffort::Low,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::High,
                vulcan_ai_contract::ReasoningEffort::XHigh,
                vulcan_ai_contract::ReasoningEffort::Max,
            ]
        );
    }

    /// Verifies a custom Gemini variant does not inherit OpenAI-only reasoning levels.
    /// 验证自定义 Gemini 变体不会继承 OpenAI 专属思考档位。
    #[test]
    fn custom_gemini_system_baseline_preserves_protocol_reasoning_evidence() {
        // baseline consumes the exact variant protocol and no connection-level compatibility field.
        // baseline 消费精确变体协议，不读取任何连接级兼容字段。
        let baseline = custom_provider_system_baseline_for_protocol(Some(
            vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID,
        ));

        assert_eq!(baseline.reasoning, CapabilitySupport::Unknown);
        assert_eq!(baseline.reasoning_control, ReasoningControlMode::Unknown);
        assert!(baseline.reasoning_efforts.is_empty());
    }

    /// Verifies an explicit provider setting can disable the system reasoning defaults.
    /// 验证显式供应商设置可以关闭系统默认的思考能力。
    #[test]
    fn explicit_provider_settings_can_disable_system_reasoning() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_provider_model_defaults.insert(
            String::from("custom:test"),
            crate::CustomProviderModelDefaults {
                reasoning: vulcan_ai_contract::CapabilitySupport::Unsupported,
                reasoning_control: vulcan_ai_contract::ReasoningControlMode::FixedOff,
                reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::None],
                ..crate::CustomProviderModelDefaults::default()
            },
        );
        let mut catalog = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"private/chat"}]}"#,
        )
        .expect("compatible catalog should parse");

        apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
            .expect("explicit provider settings should resolve");

        let capabilities = &catalog
            .models
            .first()
            .expect("model should remain visible")
            .capabilities;
        assert_eq!(
            capabilities.reasoning,
            vulcan_ai_contract::CapabilitySupport::Unsupported
        );
        assert_eq!(
            capabilities.reasoning_control,
            vulcan_ai_contract::ReasoningControlMode::FixedOff
        );
        assert_eq!(
            capabilities.reasoning_efforts,
            vec![vulcan_ai_contract::ReasoningEffort::None]
        );
    }

    /// Verifies one direct model layer outranks disabled provider reasoning settings.
    /// 验证一层直接模型设置会覆盖供应商层已关闭的思考设置。
    #[test]
    fn direct_model_settings_outrank_provider_reasoning_settings() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_provider_model_defaults.insert(
            String::from("custom:test"),
            crate::CustomProviderModelDefaults {
                reasoning: vulcan_ai_contract::CapabilitySupport::Unsupported,
                reasoning_control: vulcan_ai_contract::ReasoningControlMode::FixedOff,
                reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::None],
                ..crate::CustomProviderModelDefaults::default()
            },
        );
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
            crate::ModelCapabilityOverrides {
                reasoning: Some(vulcan_ai_contract::CapabilitySupport::Native),
                reasoning_control: Some(vulcan_ai_contract::ReasoningControlMode::EffortSelection),
                reasoning_efforts: Some(vec![
                    vulcan_ai_contract::ReasoningEffort::None,
                    vulcan_ai_contract::ReasoningEffort::Medium,
                    vulcan_ai_contract::ReasoningEffort::High,
                ]),
                ..crate::ModelCapabilityOverrides::default()
            },
        );
        configuration.custom_models.insert(
            String::from("custom:test"),
            BTreeMap::from([(
                String::from("private/chat"),
                crate::CustomModelDefinition {
                    id: String::from("private/chat"),
                    display_name: String::from("Advanced Private Chat"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        reasoning: vulcan_ai_contract::CapabilitySupport::Unsupported,
                        reasoning_control: vulcan_ai_contract::ReasoningControlMode::FixedOff,
                        reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::None],
                        evidence_revision: String::from("manual-advanced.v1"),
                        ..ModelCapabilities::default()
                    },
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            )]),
        );
        let mut catalog = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"private/chat"}]}"#,
        )
        .expect("compatible catalog should parse");

        apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
            .expect("model settings should resolve");

        let capabilities = &catalog
            .models
            .first()
            .expect("model should remain visible")
            .capabilities;
        assert_eq!(
            capabilities.reasoning,
            vulcan_ai_contract::CapabilitySupport::Native
        );
        assert_eq!(
            capabilities.reasoning_control,
            vulcan_ai_contract::ReasoningControlMode::EffortSelection
        );
        assert_eq!(
            capabilities.reasoning_efforts,
            vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::High,
            ]
        );
    }

    /// Verifies an explicit user difference outranks credential evidence.
    /// 验证显式用户差异会优先于凭据证据。
    #[test]
    fn custom_model_user_override_outranks_credential_evidence() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
            crate::ModelCapabilityOverrides {
                context_limit: Some(32_000),
                output_limit: Some(2_000),
                input_resources: Some(vec![vulcan_ai_contract::ResourceKind::File]),
                output_resources: None,
                ..crate::ModelCapabilityOverrides::default()
            },
        );
        let mut catalog = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"private/chat","context_window":64000,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("explicit compatible catalog should parse");

        apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
            .expect("custom override should resolve");

        let model = catalog.models.first().expect("model should remain visible");
        assert_eq!(model.capabilities.context_limit, Some(32_000));
        assert_eq!(model.capabilities.output_limit, Some(2_000));
        assert_eq!(
            model.capabilities.input_resources,
            vec![vulcan_ai_contract::ResourceKind::File]
        );
        assert!(model.capabilities.output_resources.is_empty());
    }

    /// Verifies discovered basics survive while explicit manual advanced declarations become effective.
    /// 验证已发现基础事实会保留，同时显式手工高级声明会真正生效。
    #[test]
    fn discovered_custom_model_applies_only_manual_advanced_declarations() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_models
            .entry(String::from("custom:test"))
            .or_default()
            .insert(
                String::from("private/chat"),
                crate::CustomModelDefinition {
                    id: String::from("private/chat"),
                    display_name: String::from("Private Chat Renamed"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        context_limit: Some(1_024),
                        output_limit: Some(512),
                        input_resources: vec![vulcan_ai_contract::ResourceKind::File],
                        reasoning: vulcan_ai_contract::CapabilitySupport::Native,
                        reasoning_control:
                            vulcan_ai_contract::ReasoningControlMode::EffortSelection,
                        reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::High],
                        model_parameters: vec![vulcan_ai_contract::ModelParameterDefinition {
                            id: String::from("vendor.flag"),
                            display_name: String::from("Vendor flag"),
                            description: String::from("Controls one verified vendor flag."),
                            schema: vulcan_ai_contract::ModelParameterSchema::Boolean {},
                            default: vulcan_ai_contract::ModelParameterDefault::ProviderDefault {},
                            protocol_ids: Vec::new(),
                            model_ids: Vec::new(),
                            dependent_required: Vec::new(),
                            conflicts: Vec::new(),
                            evidence_revision: String::from("user.custom-model.v1"),
                        }],
                        evidence_revision: String::from("user.custom-model.v2"),
                        ..ModelCapabilities::default()
                    },
                    request_projection: crate::CustomModelRequestProjection {
                        model_parameter_paths: BTreeMap::from([(
                            String::from("vendor.flag"),
                            String::from("vendor_flag"),
                        )]),
                        ..crate::CustomModelRequestProjection::default()
                    },
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            );
        let mut catalog = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"private/chat","name":"Provider Chat","context_window":64000,"max_output_tokens":4096,"input_modalities":["text","image"],"output_modalities":["text"]}]}"#,
        )
        .expect("explicit compatible catalog should parse");

        apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
            .expect("advanced declaration should merge");

        let model = catalog.models.first().expect("model should remain visible");
        assert_eq!(model.display_name, "Private Chat Renamed");
        assert_eq!(model.capabilities.context_limit, Some(64_000));
        assert_eq!(model.capabilities.output_limit, Some(4_096));
        assert_eq!(
            model.capabilities.input_resources,
            vec![vulcan_ai_contract::ResourceKind::Image]
        );
        assert!(model.capabilities.output_resources.is_empty());
        assert_eq!(
            model.capabilities.reasoning_control,
            vulcan_ai_contract::ReasoningControlMode::EffortSelection
        );
        assert_eq!(
            model.capabilities.reasoning_efforts,
            vec![vulcan_ai_contract::ReasoningEffort::High]
        );
        assert_eq!(model.capabilities.model_parameters.len(), 1);
        assert!(
            model
                .capabilities
                .evidence_revision
                .contains("user.custom-model.v2")
        );
    }

    /// Verifies provider capability defaults are inherited before a stronger model override.
    /// 验证供应商能力默认值先被继承，再由更高优先级的模型覆盖取代。
    #[test]
    fn provider_capability_defaults_are_overridden_by_model_capabilities() {
        let provider_id = "system-provider";
        let model_id = "chat-model";
        let mut configuration = PersistedAiConfiguration::default();
        configuration.provider_capability_overrides.insert(
            String::from(provider_id),
            crate::ModelCapabilityOverrides {
                context_limit: Some(32_000),
                output_limit: Some(2_000),
                reasoning: Some(vulcan_ai_contract::CapabilitySupport::Native),
                reasoning_control: Some(vulcan_ai_contract::ReasoningControlMode::EffortSelection),
                reasoning_efforts: Some(vec![vulcan_ai_contract::ReasoningEffort::Low]),
                ..crate::ModelCapabilityOverrides::default()
            },
        );
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key(provider_id, model_id),
            crate::ModelCapabilityOverrides {
                context_limit: Some(16_000),
                ..crate::ModelCapabilityOverrides::default()
            },
        );
        let mut catalog = crate::parse_openai_model_catalog(
            "system-account",
            1,
            r#"{"data":[{"id":"chat-model"},{"id":"source-model","context_window":64000,"max_output_tokens":4096}]}"#,
        )
        .expect("system catalog should parse");

        apply_model_catalog_overlays(&configuration, provider_id, &mut catalog)
            .expect("provider and model layers should resolve");

        let model = catalog
            .models
            .iter()
            .find(|model| model.id == model_id)
            .expect("model should remain visible");
        assert_eq!(model.capabilities.context_limit, Some(16_000));
        assert_eq!(model.capabilities.output_limit, Some(2_000));
        assert_eq!(
            model.capabilities.reasoning_efforts,
            vec![vulcan_ai_contract::ReasoningEffort::Low]
        );
        let source_model = catalog
            .models
            .iter()
            .find(|model| model.id == "source-model")
            .expect("source model should remain visible");
        assert_eq!(source_model.capabilities.context_limit, Some(64_000));
        assert_eq!(source_model.capabilities.output_limit, Some(4_096));
    }

    /// Verifies a non-empty discovery result is unioned with independent manual identities.
    /// 验证非空发现结果与独立手工模型身份取并集。
    #[test]
    fn custom_discovery_catalog_unions_absent_manual_model_unless_explicitly_disabled() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_models
            .entry(String::from("custom:test"))
            .or_default()
            .insert(
                String::from("manual/chat"),
                crate::CustomModelDefinition {
                    id: String::from("manual/chat"),
                    display_name: String::from("Manual Chat"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        ..ModelCapabilities::default()
                    },
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            );
        let source = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"discovered/chat"}]}"#,
        )
        .expect("authoritative catalog should parse");
        let mut merged = source.clone();
        apply_model_catalog_overlays(&configuration, "custom:test", &mut merged)
            .expect("catalog should resolve");
        assert!(merged.models.iter().any(|model| model.id == "manual/chat"));
        assert!(
            merged
                .models
                .iter()
                .any(|model| model.id == "discovered/chat")
        );

        configuration
            .custom_model_connection_enabled_overrides
            .insert(
                PersistedAiConfiguration::connection_model_key("custom-account", "manual/chat"),
                false,
            );
        let mut explicitly_disabled = source;
        apply_model_catalog_overlays(&configuration, "custom:test", &mut explicitly_disabled)
            .expect("explicitly disabled catalog should resolve");
        assert!(
            explicitly_disabled
                .models
                .iter()
                .all(|model| model.id != "manual/chat")
        );
    }

    /// Verifies an empty discovery result retains the independently authorized manual source.
    /// 验证空发现结果仍保留独立授权的手工来源。
    #[test]
    fn empty_custom_discovery_catalog_retains_manual_model_unless_explicitly_disabled() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_models
            .entry(String::from("custom:test"))
            .or_default()
            .insert(
                String::from("manual/chat"),
                crate::CustomModelDefinition {
                    id: String::from("manual/chat"),
                    display_name: String::from("Manual Chat"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        ..ModelCapabilities::default()
                    },
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            );
        let source = crate::parse_openai_model_catalog("custom-account", 1, r#"{"data":[]}"#)
            .expect("authoritative empty catalog should parse");
        let mut merged = source.clone();
        apply_model_catalog_overlays(&configuration, "custom:test", &mut merged)
            .expect("empty discovery catalog should merge with manual identities");
        assert!(merged.models.iter().any(|model| model.id == "manual/chat"));

        configuration
            .custom_model_connection_enabled_overrides
            .insert(
                PersistedAiConfiguration::connection_model_key("custom-account", "manual/chat"),
                false,
            );
        let mut explicitly_disabled = source;
        apply_model_catalog_overlays(&configuration, "custom:test", &mut explicitly_disabled)
            .expect("explicitly disabled empty catalog should resolve");
        assert!(explicitly_disabled.models.is_empty());
    }

    /// Verifies a provider-task network future cannot block its caller beyond the exact host limit.
    /// 验证供应商任务网络 Future 无法阻塞调用方超过精确宿主限制。
    #[tokio::test]
    async fn provider_task_network_operation_has_a_controlled_timeout() {
        let error = await_bounded_provider_task_operation(
            Duration::from_millis(1),
            "Test provider task observation",
            std::future::pending::<Result<(), String>>(),
        )
        .await
        .expect_err("pending provider operation should time out");

        assert!(error.contains("Test provider task observation"));
        assert!(error.contains("1 millisecond"));
    }

    /// Verifies every provider-operation phase observes one immutable monotonic deadline.
    /// 验证供应商操作的每个阶段都遵守同一个不可变单调时钟截止点。
    #[test]
    fn provider_operation_remaining_budget_fails_after_deadline() {
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .expect("test deadline should be representable");

        let error = remaining_provider_operation_timeout(expired, "Provider fixture")
            .expect_err("expired provider operation must fail");

        assert!(error.contains("Provider fixture"));
        assert!(error.contains("frozen total execution timeout"));
    }

    /// Verifies provider default selection is used until a model-specific account is remembered.
    /// 验证在记住模型专属账号前使用供应商默认账号。
    #[test]
    fn model_selection_prefers_remembered_account_over_provider_default() {
        let (runtime, mut configuration) = two_account_runtime();
        let initial = runtime
            .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
            .expect("provider default should resolve");
        assert_eq!(initial.route.connection.id, "one");

        configuration
            .connection_graph
            .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "two")
            .expect("model account should be remembered");
        runtime
            .replace_configuration(configuration.clone())
            .expect("remembered account should cross the activation boundary");
        let remembered = runtime
            .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
            .expect("remembered account should resolve");
        assert_eq!(remembered.route.connection.id, "two");
    }

    /// Verifies missing preferences fall back to the first stable visible account.
    /// 验证偏好缺失时回退到首个稳定可见账号。
    #[test]
    fn model_selection_uses_first_stable_account_without_preference() {
        let (runtime, mut configuration) = two_account_runtime();
        configuration
            .connection_graph
            .account_preferences
            .provider_defaults
            .clear();
        runtime
            .replace_configuration(configuration.clone())
            .expect("cleared preference should cross the activation boundary");
        let selection = runtime
            .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
            .expect("stable first account should resolve");
        assert_eq!(selection.route.connection.id, "one");
    }

    /// Verifies pending credentials are removed from preferences and all normal resolution paths.
    /// 验证待验证凭据会从偏好中移除，并被全部常规解析路径隔离。
    #[test]
    fn pending_validation_connection_is_isolated_from_normal_routing() {
        let (runtime, mut configuration) = two_account_runtime();
        configuration
            .connection_graph
            .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "one")
            .expect("model preference should be recorded");
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("one"),
        });

        configuration
            .set_connection_pending_validation("one", true)
            .expect("configured connection should enter pending state");
        runtime
            .replace_configuration(configuration.clone())
            .expect("pending validation should cross the activation boundary");

        assert!(configuration.active.is_none());
        assert!(
            !configuration
                .connection_graph
                .account_preferences
                .provider_defaults
                .contains_key("openrouter")
        );
        assert!(
            configuration
                .connection_graph
                .account_preferences
                .model_last_connections
                .is_empty()
        );
        let fallback = runtime
            .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
            .expect("eligible account should replace the pending preference");
        assert_eq!(fallback.route.connection.id, "two");
        let Err(exact_error) = runtime.resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        ) else {
            panic!("pending exact account must not execute");
        };
        assert!(exact_error.contains("pending explicit model validation"));
    }

    /// Verifies workbench selectors and resolution reject operation-only internal models.
    /// 验证工作台选择器与解析会拒绝仅供内部操作使用的模型。
    #[test]
    fn workbench_models_require_bidirectional_text_conversation() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection("one"),
            )
            .expect("connection should register");
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-terra", true);
        install_source_catalog(
            &runtime,
            with_protocol_variants(
                ConnectionModelCatalog {
                    connection_id: String::from("one"),
                    models: vec![
                        ConnectionModel {
                            id: String::from("openai/gpt-5.6-sol"),
                            display_name: String::from("Chat"),
                            capabilities: ModelCapabilities {
                                operations: vec![AiOperation::ConversationRespond],
                                ..ModelCapabilities::default()
                            },
                            capability_evidence: crate::ModelCapabilityEvidence::default(),
                            protocol_variants: BTreeMap::new(),
                            resolved_protocol_binding: None,
                        },
                        ConnectionModel {
                            id: String::from("openai/gpt-5.6-terra"),
                            display_name: String::from("Embedding"),
                            capabilities: ModelCapabilities {
                                operations: vec![AiOperation::EmbeddingCreate],
                                ..ModelCapabilities::default()
                            },
                            capability_evidence: crate::ModelCapabilityEvidence::default(),
                            protocol_variants: BTreeMap::new(),
                            resolved_protocol_binding: None,
                        },
                    ],
                    generation: 1,
                    page_count: 1,
                    discovery_schema_revision: String::from("test.mixed.models.v1"),
                },
                &[
                    OPENAI_CHAT_PROFILE_ID,
                    OPENAI_RESPONSES_PROFILE_ID,
                    ANTHROPIC_MESSAGES_PROFILE_ID,
                ],
            ),
        );
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("mixed-model generation should publish");

        assert_eq!(
            runtime
                .available_model_selectors(&configuration)
                .expect("selectors should resolve"),
            BTreeSet::from([String::from("openrouter/openai/gpt-5.6-sol")])
        );
        let Err(error) = runtime.resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-terra",
        ) else {
            panic!("embedding-only model must not resolve as a workbench model");
        };
        assert!(error.contains("does not expose bidirectional text conversation"));
    }

    /// Verifies a removed remembered account degrades to another account exposing the same model.
    /// 验证已记住账号被移除后会降级到另一个暴露相同模型的账号。
    #[test]
    fn model_selection_degrades_after_remembered_account_is_removed() {
        let (runtime, mut configuration) = two_account_runtime();
        configuration
            .connection_graph
            .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "two")
            .expect("model account should be remembered");
        configuration.connection_graph.connections.remove("two");
        runtime
            .replace_configuration(configuration.clone())
            .expect("connection removal should cross the activation boundary");

        let selection = runtime
            .resolve_model_selection(&configuration, "openrouter", "openai/gpt-5.6-sol")
            .expect("remaining account should resolve");
        assert_eq!(selection.route.connection.id, "one");
    }

    /// Verifies a session-selected connection overrides the model-wide remembered account.
    /// 验证会话选中的连接会覆盖模型级记忆账号。
    #[test]
    fn connection_model_selection_uses_exact_session_account() {
        let (runtime, mut configuration) = two_account_runtime();
        configuration
            .connection_graph
            .remember_model_connection("openrouter", "openai/gpt-5.6-sol", "one")
            .expect("model account should be remembered");

        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "two",
                "openrouter",
                "openai/gpt-5.6-sol",
            )
            .expect("exact session account should resolve");
        assert_eq!(selection.route.connection.id, "two");
        assert!(selection.route_revision.starts_with("sha256:"));

        let other_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "openrouter",
                "openai/gpt-5.6-sol",
            )
            .expect("other exact account should resolve");
        assert_ne!(selection.route_revision, other_selection.route_revision);
    }

    /// Verifies route identity ignores presentation and unrelated catalog facts but detects wire drift.
    /// 验证路由身份忽略展示与无关目录事实，同时检测 Wire 漂移。
    #[test]
    fn route_revision_tracks_only_exact_execution_semantics() {
        // baselineSelection provides one fully resolved route with real bundled contracts.
        // baselineSelection 提供一条带真实内置契约的完整已解析路由。
        let (runtime, configuration) = two_account_runtime();
        let baseline_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "openrouter",
                "openai/gpt-5.6-sol",
            )
            .expect("baseline route should resolve");
        let baseline_revision = provider_execution_route_revision(
            &baseline_selection.route,
            &baseline_selection.model,
            &baseline_selection.overrides,
        )
        .expect("baseline revision should serialize");

        // presentationRoute changes only UI labels, audit counters, and an unrelated provider entry.
        // presentationRoute 仅修改 UI 标签、审计计数器与无关供应商入口。
        let mut presentation_route = baseline_selection.route.clone();
        presentation_route.provider.display_name = String::from("Renamed Provider");
        presentation_route.entry.display_name = String::from("Renamed Entry");
        presentation_route.connection.display_name = String::from("Renamed Account");
        presentation_route.manifest_revision =
            presentation_route.manifest_revision.saturating_add(9);
        presentation_route.manifest_generation =
            presentation_route.manifest_generation.saturating_add(11);
        if let Some(profile) = presentation_route.entry.credential_profiles.first_mut() {
            profile.display_name = String::from("Renamed Credential Profile");
        }
        let mut unrelated_entry = presentation_route.entry.clone();
        unrelated_entry.id = String::from("new-unrelated-entry");
        unrelated_entry.display_name = String::from("New Unrelated Entry");
        presentation_route
            .provider
            .service_entries
            .push(unrelated_entry);
        // presentationModel changes only the user-facing model label.
        // presentationModel 仅修改面向用户的模型标签。
        let mut presentation_model = baseline_selection.model.clone();
        presentation_model.display_name = String::from("Renamed Model");
        let presentation_revision = provider_execution_route_revision(
            &presentation_route,
            &presentation_model,
            &baseline_selection.overrides,
        )
        .expect("presentation-only revision should serialize");
        assert_eq!(
            presentation_revision, baseline_revision,
            "presentation and unrelated catalog changes must preserve route identity"
        );

        // changedRoute changes the exact selected endpoint and must therefore produce a new identity.
        // changedRoute 修改精确已选端点，因此必须产生新身份。
        let mut changed_route = baseline_selection.route.clone();
        changed_route.protocol_route.endpoint_path = Some(String::from("/different-wire-endpoint"));
        let changed_revision = provider_execution_route_revision(
            &changed_route,
            &baseline_selection.model,
            &baseline_selection.overrides,
        )
        .expect("changed route revision should serialize");
        assert_ne!(
            changed_revision, baseline_revision,
            "wire-semantic changes must invalidate route identity"
        );

        // schemaRoute changes the provider-visible function contract and must invalidate the route.
        // schemaRoute 修改 Provider 可见函数契约，因此必须使路由失效。
        let mut schema_route = baseline_selection.route.clone();
        schema_route.tool_schema_projection = crate::ToolSchemaProjectionMode::Passthrough;
        let schema_revision = provider_execution_route_revision(
            &schema_route,
            &baseline_selection.model,
            &baseline_selection.overrides,
        )
        .expect("tool-schema route revision should serialize");
        assert_ne!(
            schema_revision, baseline_revision,
            "tool-schema projection changes must invalidate route identity"
        );
    }

    /// Verifies cached secondary routes are account-scoped, expire after six hours, and may be bypassed explicitly.
    /// 验证二级路由缓存按账号隔离、六小时后过期，并可被显式绕过。
    #[tokio::test]
    async fn secondary_route_cache_honors_scope_ttl_and_forced_refresh() {
        let (runtime, configuration) = two_account_runtime();
        // cache_key targets only the second credential and its exact model.
        // cache_key 仅指向第二个凭据及其精确模型。
        let (cache_key, _, _, _, _) = runtime
            .secondary_route_cache_identity(&configuration, "two", "openai/gpt-5.6-sol")
            .expect("cache identity should resolve");
        // cached_route is deterministic and requires no network access.
        // cached_route 是确定性的，不需要网络访问。
        let cached_route = SecondaryRouteOption {
            route_id: String::from("cached"),
            provider_display_name: String::from("Cached Provider"),
            endpoint_display_name: String::from("Cached Endpoint"),
            prompt_price_per_token: None,
            completion_price_per_token: None,
            cache_read_price_per_token: None,
            latency_milliseconds: None,
            uptime_percent: None,
            throughput_tokens_per_second: None,
        };
        runtime
            .secondary_route_cache
            .write()
            .expect("cache should be writable")
            .insert(
                cache_key.clone(),
                SecondaryRouteCacheEntry {
                    connection_id: String::from("two"),
                    routes: vec![cached_route.clone()],
                    fetched_at: Instant::now(),
                },
            );

        let cached = runtime
            .discover_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol", false)
            .await
            .expect("fresh cache should avoid credential access");
        assert_eq!(cached, vec![cached_route.clone()]);
        assert_eq!(
            runtime
                .cached_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol")
                .expect("selection validation should use the populated cache"),
            vec![cached_route.clone()]
        );
        assert!(
            runtime
                .cached_secondary_routes(&configuration, "one", "openai/gpt-5.6-sol")
                .is_err(),
            "selection validation must not reuse another credential's cache partition"
        );
        assert!(
            runtime
                .discover_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol", true)
                .await
                .is_err(),
            "forced refresh should bypass the cache and reach the unavailable secret boundary"
        );

        runtime
            .secondary_route_cache
            .write()
            .expect("cache should be writable")
            .get_mut(cache_key.as_str())
            .expect("cached route should remain present")
            .fetched_at = Instant::now()
            .checked_sub(SECONDARY_ROUTE_CACHE_TTL)
            .expect("six hours should fit in the monotonic clock");
        assert!(
            runtime
                .discover_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol", false)
                .await
                .is_err(),
            "expired cache should reach the unavailable secret boundary"
        );
        assert!(
            runtime
                .cached_secondary_routes(&configuration, "two", "openai/gpt-5.6-sol")
                .is_err(),
            "selection validation must reject an expired route identity without issuing a request"
        );
    }

    /// Verifies secondary-route discovery cannot grow its process cache without bound.
    /// 验证二级路由发现无法让进程缓存无界增长。
    #[test]
    fn secondary_route_cache_evicts_the_oldest_partition_at_capacity() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // baseInstant keeps every inserted entry fresh while giving the first key a stable oldest time.
        // baseInstant 让全部插入项保持新鲜，同时为首个键提供稳定的最旧时间。
        let base_instant = Instant::now();
        // index identifies one deterministic cache partition across the capacity boundary.
        // index 标识一个跨越容量边界的确定性缓存分区。
        for index in 0..=SECONDARY_ROUTE_CACHE_MAX_ENTRIES {
            // ageSeconds gives lower indexes an older but still-live observation time.
            // ageSeconds 为较小索引提供更旧但仍有效的观测时间。
            let age_seconds = (SECONDARY_ROUTE_CACHE_MAX_ENTRIES
                - index.min(SECONDARY_ROUTE_CACHE_MAX_ENTRIES))
                as u64;
            // fetchedAt is the exact monotonic timestamp stored for deterministic eviction.
            // fetchedAt 是用于确定性淘汰的精确单调时钟时间戳。
            let fetched_at = base_instant
                .checked_sub(Duration::from_secs(age_seconds))
                .expect("bounded cache age should fit the monotonic clock");
            runtime
                .insert_secondary_route_cache_entry(
                    format!("cache-key-{index:04}"),
                    format!("connection-{index:04}").as_str(),
                    Vec::new(),
                    fetched_at,
                )
                .expect("bounded cache insertion should succeed");
        }
        let cache = runtime
            .secondary_route_cache
            .read()
            .expect("secondary-route cache should remain readable");
        assert_eq!(cache.len(), SECONDARY_ROUTE_CACHE_MAX_ENTRIES);
        assert!(
            !cache.contains_key("cache-key-0000"),
            "the deterministically oldest partition must be evicted"
        );
        assert!(cache.contains_key("cache-key-0512"));
    }

    /// Verifies one active selection remains bound to its exact account snapshot.
    /// 验证活动选择始终绑定其精确账号快照。
    #[test]
    fn active_selection_uses_exact_configured_connection() {
        let (runtime, mut configuration) = two_account_runtime();
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("two"),
        });
        runtime
            .replace_configuration(configuration.clone())
            .expect("active selection should cross the activation boundary");
        let selection = runtime
            .resolve_active_selection(&configuration)
            .expect("active account should resolve");
        assert_eq!(selection.route.connection.id, "two");
    }

    /// Verifies accounts sharing a provider, model, and protocol receive the same override.
    /// 验证共享供应商、模型与协议的账号取得同一份覆盖。
    #[test]
    fn model_overrides_are_shared_across_matching_accounts() {
        let (runtime, mut configuration) = two_account_runtime();
        let key = PersistedAiConfiguration::provider_model_protocol_key(
            "openrouter",
            "openai/gpt-5.6-sol",
            OPENAI_RESPONSES_PROFILE_ID,
        );
        configuration.model_overrides.insert(
            key,
            ModelExecutionOverrides {
                context_limit: Some(4096),
                ..ModelExecutionOverrides::default()
            },
        );

        for connection_id in ["one", "two"] {
            configuration.active = Some(ActiveAiModelSelection {
                provider_id: String::from("openrouter"),
                model_id: String::from("openai/gpt-5.6-sol"),
                connection_id: connection_id.to_string(),
            });
            runtime
                .replace_configuration(configuration.clone())
                .expect("active override selection should cross the activation boundary");
            let selection = runtime
                .resolve_active_selection(&configuration)
                .expect("shared model override should resolve");
            assert_eq!(selection.overrides.context_limit, Some(4096));
        }
    }

    /// Verifies an undeclared provider limit may be supplied explicitly by the user.
    /// 验证供应商未声明上限时允许用户显式提供覆盖值。
    #[test]
    fn undeclared_model_limits_accept_explicit_overrides() {
        let mut model = catalog("connection")
            .models
            .into_iter()
            .next()
            .expect("catalog fixture should expose one model");
        model.capabilities.context_limit = None;
        model.capabilities.output_limit = None;
        let overrides = ModelExecutionOverrides {
            context_limit: Some(16_384),
            output_limit: Some(4_096),
            ..ModelExecutionOverrides::default()
        };

        validate_model_overrides(&model, OPENAI_CHAT_PROFILE_ID, &overrides)
            .expect("explicit limits should be accepted when the provider does not declare caps");
    }

    /// Verifies declared provider limits remain hard upper bounds for local overrides.
    /// 验证供应商已声明的上限仍是本地覆盖不可突破的硬上限。
    #[test]
    fn declared_model_limits_reject_expansive_overrides() {
        let model = catalog("connection")
            .models
            .into_iter()
            .next()
            .expect("catalog fixture should expose one model");
        let overrides = ModelExecutionOverrides {
            context_limit: Some(16_384),
            ..ModelExecutionOverrides::default()
        };

        let error = validate_model_overrides(&model, OPENAI_CHAT_PROFILE_ID, &overrides)
            .expect_err("declared context cap must reject an expansive override");
        assert!(error.contains("context override exceeds its discovered limit"));
    }

    /// Verifies adapter-declared explicit defaults become immutable execution values.
    /// 验证适配器声明的显式默认值会成为不可变执行值。
    #[test]
    fn explicit_model_parameter_defaults_are_applied_before_validation() {
        let mut catalog = parse_minimax_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"MiniMax-M2.7","object":"model","created":1,"owned_by":"minimax"}]}"#,
        )
        .expect("MiniMax model fixture should parse");
        let manifests = crate::provider_manifest::load_bundled_resolved_catalog();
        let manifest = manifests
            .manifest_arc("minimax")
            .expect("MiniMax manifest must exist");
        crate::enrich_compatible_model_catalog_with_manifest(
            "minimax",
            "global",
            Some("payg-api-key"),
            OPENAI_CHAT_PROFILE_ID,
            &mut catalog,
            &manifest,
        );
        let model = catalog.models.remove(0);
        let mut overrides = ModelExecutionOverrides::default();
        apply_model_parameter_defaults(&model, OPENAI_CHAT_PROFILE_ID, &mut overrides)
            .expect("registered defaults should apply");
        assert_eq!(
            overrides
                .model_parameters
                .get(MINIMAX_REASONING_SPLIT_PARAMETER_ID),
            Some(&vulcan_ai_contract::ModelParameterValue::Boolean(true))
        );
    }

    /// Verifies the manifest-first enabled resolution order: user override, manual model, then
    /// bundled manifest default; undeclared providers fail closed.
    /// 验证 manifest-first 启用解析顺序：用户覆盖、手动模型、内置清单默认；未声明供应商失败关闭。
    #[test]
    fn runtime_model_is_enabled_follows_manifest_first_precedence() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();

        // Bundled openrouter manifest: curated featured models enabled, everything else disabled.
        // 内置 openrouter 清单：精选模型启用，其余禁用。
        assert!(runtime.model_is_enabled(&configuration, "openrouter", "openai/gpt-5.6-sol"));
        assert!(!runtime.model_is_enabled(&configuration, "openrouter", "openai/gpt-4o"));

        // An explicit user override wins over the manifest default in both directions.
        // 显式用户覆盖在两个方向都优先于清单默认。
        configuration.set_model_enabled("openrouter", "openai/gpt-4o", true);
        assert!(runtime.model_is_enabled(&configuration, "openrouter", "openai/gpt-4o"));
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", false);
        assert!(!runtime.model_is_enabled(&configuration, "openrouter", "openai/gpt-5.6-sol"));

        // Manual models keep their declared enabled state without manifest defaults.
        // 手动模型保留声明的启用状态，不参与清单默认。
        let mut manual = PersistedAiConfiguration::default();
        manual.custom_models.insert(
            String::from("openrouter"),
            BTreeMap::from([(
                String::from("user/private-model"),
                crate::CustomModelDefinition {
                    id: String::from("user/private-model"),
                    display_name: String::from("Private Model"),
                    enabled: true,
                    capabilities: ModelCapabilities::default(),
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            )]),
        );
        assert!(runtime.model_is_enabled(&manual, "openrouter", "user/private-model"));

        // Providers without a bundled manifest or manual declaration fail closed.
        // 无内置 Manifest 且无手工声明的供应商失败关闭。
        assert!(!runtime.model_is_enabled(&configuration, "unknown-provider", "any-model"));
    }

    /// Verifies model-level protocol routing: two models on one credential bind to their own
    /// manifest-declared protocols.
    /// 验证模型级协议路由：同一凭据上的两个模型绑定各自 Manifest 声明协议。
    #[test]
    fn model_level_protocol_routing_uses_declared_bindings_per_model() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection("one"),
            )
            .expect("connection should register");
        let catalog = with_protocol_variants(
            parse_openrouter_model_catalog(
                "one",
                1,
                r#"{"data":[
                    {"id":"openai/gpt-5.6-sol","name":"OpenAI: GPT-5.6 Sol","context_length":1050000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":128000}},
                    {"id":"anthropic/claude-sonnet-5","name":"Anthropic: Claude Sonnet 5","context_length":1000000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":64000}}
                ]}"#,
            )
            .expect("openrouter catalog should parse"),
            &[
                OPENAI_RESPONSES_PROFILE_ID,
                ANTHROPIC_MESSAGES_PROFILE_ID,
            ],
        );
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("catalog should install");
        configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
        configuration.set_model_enabled("openrouter", "anthropic/claude-sonnet-5", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("protocol-routing generation should publish");

        runtime
            .validate_model_execution_override_candidate(
                &configuration,
                "openrouter",
                "openai/gpt-5.6-sol",
                OPENAI_RESPONSES_PROFILE_ID,
                &crate::ModelExecutionOverrides::default(),
            )
            .expect("model settings must validate against the frozen Responses binding");
        let wrong_protocol_error = runtime
            .validate_model_execution_override_candidate(
                &configuration,
                "openrouter",
                "openai/gpt-5.6-sol",
                OPENAI_CHAT_PROFILE_ID,
                &crate::ModelExecutionOverrides::default(),
            )
            .expect_err("another protocol must not authorize model settings");
        assert!(
            wrong_protocol_error.contains("effective protocol"),
            "unexpected wrong-protocol error: {wrong_protocol_error}"
        );

        let published_catalogs = runtime
            .catalog_snapshot()
            .expect("published catalogs should remain readable");
        let published_models = &published_catalogs
            .get("one")
            .expect("connection catalog must exist")
            .models;
        for published_model in published_models {
            let published_binding = published_model
                .resolved_protocol_binding
                .as_ref()
                .expect("reviewed system models must freeze a catalog binding");
            let published_variant = published_model
                .protocol_variants
                .get(published_binding.protocol_profile_id.as_str())
                .expect("the frozen system binding must own an exact variant");
            assert_eq!(published_model.capabilities, published_variant.capabilities);
            assert_eq!(
                published_model.capability_evidence,
                published_variant.capability_evidence
            );
        }

        let sol = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "openrouter",
                "openai/gpt-5.6-sol",
            )
            .expect("GPT-5.6 Sol must resolve");
        assert_eq!(
            sol.route.protocol_route.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
            "GPT-5.6 Sol binds to its declared Responses protocol"
        );
        let binding = &sol.protocol_binding;
        assert_eq!(binding.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID);
        assert!(binding.manifest_revision >= 1);

        let claude = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "openrouter",
                "anthropic/claude-sonnet-5",
            )
            .expect("Claude Sonnet 5 must resolve");
        assert_eq!(
            claude.route.protocol_route.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID,
            "Claude Sonnet 5 binds to its declared Anthropic protocol"
        );
        assert_ne!(
            sol.route_revision, claude.route_revision,
            "distinct protocol bindings must produce distinct route revisions"
        );
    }

    /// Verifies APIKEY.FUN routes a credential-only connection through its signed model binding.
    /// 验证 APIKEY.FUN 会通过已签名模型绑定路由仅含凭据身份的连接。
    #[test]
    fn apikeyfun_manifest_uses_model_protocol_binding() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        // apikeyfunConnection stores only credential and service identity.
        // apikeyfunConnection 仅保存凭据与服务身份，协议由当前 Manifest 模型绑定提供。
        let mut apikeyfun_connection = connection("apikeyfun-account");
        apikeyfun_connection.provider_id = String::from("apikeyfun");
        apikeyfun_connection.service_entry_id = String::from("public-api");
        configuration
            .connection_graph
            .connections
            .insert(apikeyfun_connection.id.clone(), apikeyfun_connection);

        let model_id = "deepseek-v4-flash";
        // sourceCatalog represents one `/models` identity projected through the signed Responses rule.
        // sourceCatalog 表示一个通过已签名 Responses 规则投影的 `/models` 身份。
        let source_catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "apikeyfun-account",
                1,
                format!(r#"{{"data":[{{"id":"{model_id}"}}]}}"#).as_str(),
            )
            .expect("compatible catalog should parse"),
            &[OPENAI_RESPONSES_PROFILE_ID],
        );
        install_source_catalog(&runtime, source_catalog.clone());
        runtime
            .replace_catalog(source_catalog)
            .expect("catalog should install");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("APIKEY.FUN Manifest catalog should publish");

        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "apikeyfun-account",
                "apikeyfun",
                model_id,
            )
            .expect("declared APIKEY.FUN model should resolve");
        assert_eq!(
            selection.route.protocol_route.protocol_profile_id,
            OPENAI_RESPONSES_PROFILE_ID
        );
        assert_eq!(
            selection.protocol_binding.source,
            crate::provider_manifest::ModelProtocolBindingSource::SystemManifest
        );
    }

    /// Verifies APIKEY.FUN authorizes Manifest discovery before reading the credential.
    /// 验证 APIKEY.FUN 会先授权 Manifest 发现路由，再读取凭据。
    #[tokio::test]
    async fn apikeyfun_manifest_authorizes_discovery_before_credential_access() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        // apikeyfunConnection stores only credential and service identity.
        // apikeyfunConnection 仅保存凭据与服务身份，不保存任何协议历史。
        let mut apikeyfun_connection = connection("apikeyfun-account");
        apikeyfun_connection.provider_id = String::from("apikeyfun");
        apikeyfun_connection.service_entry_id = String::from("public-api");
        configuration.connection_graph.connections.insert(
            apikeyfun_connection.id.clone(),
            apikeyfun_connection.clone(),
        );

        // The unavailable store is the first expected boundary after discovery authorization. A
        // discovery-identity error here would prove the Manifest route was not used.
        // 不可用存储是发现授权后的首个预期边界；若此处出现发现身份错误，则证明未使用 Manifest 路由。
        let error = runtime
            .refresh_connection_catalog(&configuration, apikeyfun_connection.id.as_str())
            .await
            .expect_err("the unavailable test store must reject the credential read");
        assert!(
            error.contains("credential secret resolver"),
            "signed discovery should reach the credential boundary, got: {error}"
        );
        assert!(
            !error.contains("discovery identity without a remote discovery implementation"),
            "signed discovery must not be rejected by stale protocol history: {error}"
        );
    }

    /// Verifies APIKEY.FUN rejects a discovered model outside the reviewed declaration whitelist.
    /// 验证 APIKEY.FUN 会拒绝已发现但不在已审核声明白名单中的模型。
    #[test]
    fn apikeyfun_rejects_discovered_model_outside_declared_whitelist() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let mut apikeyfun = connection("apikeyfun-account");
        apikeyfun.provider_id = String::from("apikeyfun");
        apikeyfun.service_entry_id = String::from("public-api");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                apikeyfun,
            )
            .expect("APIKEY.FUN connection should register");

        let model_id = "future-provider-model";
        let catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "apikeyfun-account",
                1,
                format!(r#"{{"data":[{{"id":"{model_id}"}}]}}"#).as_str(),
            )
            .expect("compatible catalog should parse"),
            &[OPENAI_CHAT_PROFILE_ID],
        );
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("catalog should install");
        configuration.set_model_enabled("apikeyfun", model_id, true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("reviewed provider catalog should publish");

        let error = runtime
            .resolve_connection_model_selection(
                &configuration,
                "apikeyfun-account",
                "apikeyfun",
                model_id,
            )
            .expect_err("undeclared APIKEY.FUN model must not resolve");
        assert!(
            error.contains("not present in the effective catalog"),
            "rejection must identify the exact effective-catalog boundary: {error}"
        );
    }

    /// Verifies an undeclared model on a `declared_rule_required` provider is not executable.
    /// 验证 `declared_rule_required` 供应商上未声明模型不可执行。
    #[test]
    fn undeclared_model_on_declared_rule_required_provider_is_not_executable() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                connection("one"),
            )
            .expect("connection should register");
        let catalog = parse_openrouter_model_catalog(
            "one",
            1,
            r#"{"data":[{"id":"openai/gpt-4o","name":"OpenAI: GPT-4o","context_length":128000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":16384}}]}"#,
        )
        .expect("openrouter catalog should parse");
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("catalog should install");
        configuration.set_model_enabled("openrouter", "openai/gpt-4o", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("undeclared-model generation should publish");

        let published_catalogs = runtime
            .catalog_snapshot()
            .expect("published catalogs should remain readable");
        let published_model = published_catalogs
            .get("one")
            .and_then(|catalog| catalog.models.first())
            .expect("the unmatched identity remains visible for diagnostics");
        assert!(published_model.resolved_protocol_binding.is_none());
        assert_eq!(published_model.capabilities, ModelCapabilities::default());
        assert_eq!(
            published_model.capability_evidence,
            ModelCapabilityEvidence::default()
        );

        let error = match runtime.resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-4o",
        ) {
            Ok(_) => panic!("undeclared model must not execute"),
            Err(error) => error,
        };
        assert!(
            error.contains("not executable"),
            "expected a binding-missing error, got: {error}"
        );
    }

    /// Builds one custom-provider connection for multi-instance isolation tests.
    /// 为多实例隔离测试构建一个自定义供应商连接。
    fn custom_connection(id: &str, provider_id: &str) -> CredentialConnection {
        CredentialConnection {
            id: id.to_string(),
            provider_id: provider_id.to_string(),
            service_entry_id: String::from("default"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: format!("credential-{id}"),
            display_name: id.to_string(),
            secret_reference: format!("secret-{id}"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        }
    }

    /// Builds one multi-protocol custom provider definition.
    /// 构建一个多协议自定义供应商定义。
    fn multi_protocol_custom_definition(
        provider_id: &str,
        protocols: &[crate::CustomProviderProtocol],
    ) -> crate::ProviderDescriptor {
        custom_definition_with_discovery_mode(
            provider_id,
            protocols,
            crate::CustomProviderModelDiscoveryMode::Standard,
        )
    }

    /// Installs one current custom descriptor together with its same-generation Manifest.
    /// 同时安装一个当前自定义描述符及其同代际 Manifest。
    ///
    /// # Parameters
    /// 参数
    /// `configuration` receives both authorities and `provider` is the exact current descriptor.
    /// `configuration` 接收两项权威，`provider` 是精确的当前描述符。
    fn install_current_custom_provider(
        configuration: &mut PersistedAiConfiguration,
        provider: crate::ProviderDescriptor,
    ) {
        // manifest is projected before the descriptor moves into persisted configuration.
        // manifest 在描述符移入持久化配置前完成投影。
        let manifest = crate::provider_manifest::project_custom_descriptor_manifest(&provider)
            .expect("current custom descriptor must project to a Manifest");
        configuration
            .custom_provider_manifests
            .insert(provider.id.clone(), manifest);
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider);
    }

    /// Builds one custom provider definition with an exact model-source mode.
    /// 使用精确模型来源模式构建一个自定义供应商定义。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the stable custom identity, `protocols` are its declared execution
    /// protocols, and `model_discovery_mode` is the authoritative identity-source mode.
    /// `provider_id` 是稳定自定义身份，`protocols` 是其声明执行协议，
    /// `model_discovery_mode` 是权威身份来源模式。
    ///
    /// # Returns
    /// 返回值
    /// A validated runtime descriptor for test configuration and connection construction.
    /// 返回用于测试配置与连接构造的已校验运行时描述。
    fn custom_definition_with_discovery_mode(
        provider_id: &str,
        protocols: &[crate::CustomProviderProtocol],
        model_discovery_mode: crate::CustomProviderModelDiscoveryMode,
    ) -> crate::ProviderDescriptor {
        crate::CustomProviderDefinition {
            id: provider_id.to_string(),
            display_name: String::from("Multi Protocol Fixture"),
            base_url: String::from("https://fixture.example.test/v1"),
            protocols: protocols.to_vec(),
            authentication: crate::CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("multi-protocol fixture should project")
    }

    /// Verifies a later remote route keeps discovery enabled when the first route is manual.
    /// 验证首条路由为手工发现时，后续远端路由仍会保持发现能力。
    #[test]
    fn mixed_custom_protocol_discovery_aggregates_every_verified_route() {
        // provider declares Responses as manual first and Chat as a route-specific remote source.
        // provider 首先声明手工 Responses，并把 Chat 声明为路由专属远端来源。
        let provider = crate::CustomProviderDefinition {
            id: String::from("custom:mixed-discovery"),
            display_name: String::from("Mixed Discovery"),
            base_url: String::from("https://mixed-discovery.example.test/v1"),
            protocols: vec![
                crate::CustomProviderProtocol::OpenAiResponses,
                crate::CustomProviderProtocol::OpenAiChat,
            ],
            authentication: crate::CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: crate::CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocol_routes: vec![crate::CustomProtocolRoute {
                protocol: crate::CustomProviderProtocol::OpenAiChat,
                base_url: None,
                endpoint_path: None,
                authentication: None,
                model_discovery_mode: Some(crate::CustomProviderModelDiscoveryMode::Standard),
                model_discovery_path: None,
                request_headers: BTreeMap::new(),
            }],
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("mixed custom provider should project");
        // entry retains Manual only as a compatibility projection of the first route.
        // entry 仅把 Manual 保留为首条路由的兼容投影。
        let entry = provider
            .service_entries
            .first()
            .expect("custom provider should expose one entry");
        assert_eq!(entry.model_discovery, ModelDiscoveryKind::Manual);
        // configuration supplies the exact descriptor to the production source-policy resolver.
        // configuration 向生产来源策略解析器提供精确描述符。
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider.clone());

        assert_eq!(
            resolved_model_source_policy(
                &configuration,
                provider.id.as_str(),
                entry.id.as_str(),
                None,
            )
            .expect("mixed route source policy should resolve"),
            ModelSourcePolicy::DiscoveryAndManual,
        );
    }

    /// Verifies three custom providers stay fully isolated across models and credentials.
    /// 验证三个自定义供应商在模型与凭据上完全隔离。
    #[test]
    fn three_custom_providers_stay_isolated_from_each_other() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        // provider-a owns only the chat protocol; provider-b only anthropic; provider-c only gemini.
        // provider-a 仅拥有聊天协议；provider-b 仅 anthropic；provider-c 仅 gemini。
        let provider_a = multi_protocol_custom_definition(
            "custom:alpha",
            &[crate::CustomProviderProtocol::OpenAiChat],
        );
        let provider_b = multi_protocol_custom_definition(
            "custom:beta",
            &[crate::CustomProviderProtocol::AnthropicMessages],
        );
        let provider_c = multi_protocol_custom_definition(
            "custom:gamma",
            &[crate::CustomProviderProtocol::GeminiGenerateContent],
        );
        install_current_custom_provider(&mut configuration, provider_a);
        install_current_custom_provider(&mut configuration, provider_b);
        install_current_custom_provider(&mut configuration, provider_c);
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("alpha-account", "custom:alpha"),
            )
            .expect("alpha connection should register");
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("beta-account", "custom:beta"),
            )
            .expect("beta connection should register");
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("gamma-account", "custom:gamma"),
            )
            .expect("gamma connection should register");
        // Every provider owns the same model identifier; the identities must never leak.
        // 每个供应商拥有相同模型标识；身份绝不能相互泄漏。
        let catalog = parse_openai_model_catalog(
            "alpha-account",
            1,
            r#"{"data":[{"id":"shared/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("alpha catalog should parse");
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("alpha catalog should install");
        configuration.set_model_enabled("custom:alpha", "shared/model", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("custom-provider isolation generation should publish");

        let alpha = runtime
            .resolve_connection_model_selection(
                &configuration,
                "alpha-account",
                "custom:alpha",
                "shared/model",
            )
            .expect("alpha model must resolve on its own provider");
        assert_eq!(
            alpha.route.protocol_route.protocol_profile_id,
            OPENAI_CHAT_PROFILE_ID
        );
        let binding = &alpha.protocol_binding;
        assert_eq!(binding.protocol_profile_id, OPENAI_CHAT_PROFILE_ID);

        // beta and gamma are absent from the alpha catalog and must never resolve there.
        // beta 与 gamma 不在 alpha 目录中，绝不能在此解析。
        let beta_error = runtime
            .resolve_connection_model_selection(
                &configuration,
                "alpha-account",
                "custom:beta",
                "shared/model",
            )
            .map(|_| ())
            .expect_err("beta provider must not resolve on the alpha account");
        assert!(
            beta_error.contains("does not belong to provider")
                || beta_error.contains("not configured")
                || beta_error.contains("not registered"),
            "unexpected isolation error: {beta_error}"
        );
    }

    /// Verifies one invalid standalone custom provider cannot block a healthy sibling activation.
    /// 验证一个无效独立自定义供应商不会阻断健康同级供应商激活。
    #[tokio::test]
    async fn isolated_custom_provider_connection_is_excluded_from_atomic_activation() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let healthy_provider_id = "custom:healthy-activation";
        let isolated_provider_id = "custom:isolated-activation";
        let healthy_connection_id = "healthy-activation-account";
        let isolated_connection_id = "isolated-activation-account";
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_provider_file_refs.extend([
            healthy_provider_id.to_string(),
            isolated_provider_id.to_string(),
        ]);
        let healthy_provider = custom_definition_with_discovery_mode(
            healthy_provider_id,
            &[crate::CustomProviderProtocol::OpenAiChat],
            crate::CustomProviderModelDiscoveryMode::Manual,
        );
        install_current_custom_provider(&mut configuration, healthy_provider);
        configuration.connection_graph.connections.extend([
            (
                healthy_connection_id.to_string(),
                custom_connection(healthy_connection_id, healthy_provider_id),
            ),
            (
                isolated_connection_id.to_string(),
                custom_connection(isolated_connection_id, isolated_provider_id),
            ),
        ]);
        configuration.custom_models.insert(
            healthy_provider_id.to_string(),
            BTreeMap::from([(
                String::from("healthy-model"),
                crate::CustomModelDefinition {
                    id: String::from("healthy-model"),
                    display_name: String::from("Healthy Model"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        ..ModelCapabilities::default()
                    },
                    request_projection: crate::CustomModelRequestProjection::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            )]),
        );
        let connection_ids = vec![
            healthy_connection_id.to_string(),
            isolated_connection_id.to_string(),
        ];

        runtime
            .activate_staged_manifest_catalogs(&configuration, &connection_ids)
            .await
            .expect("healthy provider activation should ignore the isolated sibling");

        let catalogs = runtime
            .catalog_snapshot()
            .expect("catalog snapshot should read");
        assert!(catalogs.contains_key(healthy_connection_id));
        assert!(!catalogs.contains_key(isolated_connection_id));
        assert!(
            runtime
                .resolve_connection_model_selection(
                    &configuration,
                    healthy_connection_id,
                    healthy_provider_id,
                    "healthy-model",
                )
                .is_ok()
        );
        let isolated_error = runtime
            .resolve_connection_model_selection(
                &configuration,
                isolated_connection_id,
                isolated_provider_id,
                "isolated-model",
            )
            .map(|_| ())
            .expect_err("isolated provider execution must fail closed");
        assert!(
            isolated_error.contains("disabled")
                || isolated_error.contains("no active system manifest")
                || isolated_error.contains("No activated catalog exists"),
            "{isolated_error}"
        );
    }

    /// Verifies custom descriptor matching excludes only standalone-Manifest Profile payloads.
    /// 验证自定义描述符匹配仅排除独立 Manifest 的 Profile 载荷。
    #[test]
    fn custom_manifest_descriptor_match_excludes_only_manifest_owned_profiles() {
        let provider = multi_protocol_custom_definition(
            "custom:manifest-descriptor-match",
            &[crate::CustomProviderProtocol::OpenAiChat],
        );
        let manifest = crate::provider_manifest::project_custom_descriptor_manifest(&provider)
            .expect("custom descriptor should project to one manifest");
        let compiled = crate::provider_manifest::compile_manifest(
            &manifest,
            crate::provider_manifest::ManifestOwnership::Custom,
        )
        .expect("projected custom manifest should compile");

        assert!(
            crate::provider_manifest::custom_manifest_matches_activated_descriptor(
                compiled.provider(),
                &provider,
            ),
            "manifest-owned standard Usage payloads must not create a false descriptor mismatch"
        );

        let mut conflicting_provider = provider.clone();
        conflicting_provider.service_entries[0].base_url =
            String::from("https://conflicting.example.invalid/v1");
        assert!(
            !crate::provider_manifest::custom_manifest_matches_activated_descriptor(
                compiled.provider(),
                &conflicting_provider,
            ),
            "descriptor-owned service facts must remain fail-closed"
        );
    }

    /// Verifies one full custom manifest publishes tool targets only at the explicit activation boundary.
    /// 验证一份完整自定义 Manifest 仅在显式激活边界发布工具目标。
    #[test]
    fn custom_manifest_tool_targets_publish_as_one_frozen_generation() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = multi_protocol_custom_definition(
            "custom:manifest-tools",
            &[crate::CustomProviderProtocol::OpenAiChat],
        );
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider.clone());
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("manifest-tools-account", provider.id.as_str()),
            )
            .expect("custom connection should register");
        // sourceCatalog supplies one exact executable model whose binding must carry the custom
        // manifest revision across activation boundaries.
        // sourceCatalog 提供一个精确可执行模型，其绑定必须跨激活边界携带自定义 Manifest 修订。
        let source_catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "manifest-tools-account",
                1,
                r#"{"data":[{"id":"manifest/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
            )
            .expect("custom manifest model catalog should parse"),
            &[OPENAI_CHAT_PROFILE_ID],
        );
        install_source_catalog(&runtime, source_catalog);
        configuration.set_model_enabled(provider.id.as_str(), "manifest/model", true);

        let mut document = serde_json::to_value(
            crate::provider_manifest::project_custom_descriptor_manifest(&provider)
                .expect("custom descriptor should project to a manifest"),
        )
        .expect("custom manifest should serialize");
        document["engine_contract"]["required_execution_primitives"] =
            serde_json::json!(["http_json_sync"]);
        document["tool_targets"] = serde_json::json!([{
            "target_id": "default.search",
            "description": "Generic custom-provider search fixture.",
            "service_entry_id": "default",
            "credential_profile_ids": [],
            "upstream_target_id": "search-v1",
            "display_name": "Manifest Search V1",
            "tool_id": "vulcan_web_search",
            "capability": "web_search",
            "input_schema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {"query": {"type": "string", "minLength": 1}},
                "required": ["query"]
            },
            "output": {
                "kind": "search",
                "source_path": "/results",
                "normalized_result_path": "search",
                "resource_kind": null,
                "mime_types": []
            },
            "execution": {
                "endpoint_path": "/search",
                "authentication": "bearer_authorization",
                "adapter": "generic_json",
                "execution_kind": "http_json_sync"
            }
        }]);
        document["operations"] = serde_json::json!([{
            "operation_id": "manifest-tools.search",
            "operation": "web_search",
            "carrier": "provider_tool",
            "input_resources": [],
            "output_resources": [],
            "tool_target_refs": ["default.search"],
            "request_profile_refs": [],
            "response_profile_refs": [],
            "upload_channel_refs": [],
            "upload_bindings": [],
            "timeout_ms": 60000,
            "idempotent": false
        }]);
        let manifest: crate::provider_manifest::ProviderManifestV1 =
            serde_json::from_value(document.clone()).expect("custom manifest should deserialize");
        configuration
            .custom_provider_manifests
            .insert(provider.id.clone(), manifest);

        assert!(
            runtime
                .available_provider_tool_targets(&configuration)
                .expect("pre-activation targets should resolve")
                .is_empty(),
            "an unactivated custom manifest must remain invisible"
        );
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("custom manifest generation should publish");
        let activated = runtime
            .available_provider_tool_targets(&configuration)
            .expect("activated custom targets should resolve");
        assert_eq!(activated.len(), 1);
        assert!(
            activated[0].target.credential_profile_ids.is_empty(),
            "an empty target credential scope must authorize every profile"
        );
        assert_eq!(activated[0].target.display_name, "Manifest Search V1");
        // firstBinding proves custom model provenance retains the activated manifest revision.
        // firstBinding 证明自定义模型来源保留已激活 Manifest 修订。
        let first_binding = runtime
            .resolve_connection_model_selection(
                &configuration,
                "manifest-tools-account",
                provider.id.as_str(),
                "manifest/model",
            )
            .expect("first custom manifest model must resolve")
            .protocol_binding;
        assert_eq!(first_binding.manifest_revision, 1);

        document["revision"] = serde_json::json!(2);
        document["tool_targets"][0]["display_name"] = serde_json::json!("Manifest Search V2");
        configuration.custom_provider_manifests.insert(
            provider.id.clone(),
            serde_json::from_value(document).expect("replacement manifest should deserialize"),
        );
        let still_frozen = runtime
            .available_provider_tool_targets(&configuration)
            .expect("frozen custom targets should resolve");
        assert_eq!(still_frozen[0].target.display_name, "Manifest Search V1");
        // stillFrozenBinding proves an unactivated file edit cannot alter model provenance.
        // stillFrozenBinding 证明未激活的文件编辑不能改变模型来源。
        let still_frozen_binding = runtime
            .resolve_connection_model_selection(
                &configuration,
                "manifest-tools-account",
                provider.id.as_str(),
                "manifest/model",
            )
            .expect("pre-activation custom model must remain selectable")
            .protocol_binding;
        assert_eq!(still_frozen_binding.manifest_revision, 1);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("replacement custom generation should publish");
        let replaced = runtime
            .available_provider_tool_targets(&configuration)
            .expect("replacement custom targets should resolve");
        assert_eq!(replaced[0].target.display_name, "Manifest Search V2");
        // replacedBinding proves the next activation atomically advances binding provenance.
        // replacedBinding 证明下一次激活会原子推进绑定来源修订。
        let replaced_binding = runtime
            .resolve_connection_model_selection(
                &configuration,
                "manifest-tools-account",
                provider.id.as_str(),
                "manifest/model",
            )
            .expect("replacement custom manifest model must resolve")
            .protocol_binding;
        assert_eq!(replaced_binding.manifest_revision, 2);
    }

    /// Verifies multi-protocol Auto resolves responses first, then anthropic, then chat.
    /// 验证多协议 Auto 依次解析 responses、anthropic、chat。
    #[test]
    fn custom_multi_protocol_auto_resolves_by_fixed_priority() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = multi_protocol_custom_definition(
            "custom:multi",
            &[
                crate::CustomProviderProtocol::OpenAiChat,
                crate::CustomProviderProtocol::AnthropicMessages,
                crate::CustomProviderProtocol::OpenAiResponses,
            ],
        );
        install_current_custom_provider(&mut configuration, provider);
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("multi-account", "custom:multi"),
            )
            .expect("multi connection should register");
        let catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "multi-account",
                1,
                r#"{"data":[{"id":"auto/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
            )
            .expect("auto catalog should parse"),
            &[
                OPENAI_CHAT_PROFILE_ID,
                ANTHROPIC_MESSAGES_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
            ],
        );
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("auto catalog should install");
        configuration.set_model_enabled("custom:multi", "auto/model", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("custom auto generation should publish");

        let published_catalogs = runtime
            .catalog_snapshot()
            .expect("published custom catalogs should remain readable");
        let published_model = published_catalogs
            .get("multi-account")
            .and_then(|catalog| catalog.models.first())
            .expect("the custom model must remain published");
        let published_binding = published_model
            .resolved_protocol_binding
            .as_ref()
            .expect("the custom catalog must freeze its auto binding");
        assert_eq!(
            published_binding.protocol_profile_id,
            OPENAI_RESPONSES_PROFILE_ID
        );
        assert_eq!(published_model.protocol_variants.len(), 3);
        let published_variant = published_model
            .protocol_variants
            .get(OPENAI_RESPONSES_PROFILE_ID)
            .expect("the auto-bound Responses variant must exist");
        assert_eq!(published_model.capabilities, published_variant.capabilities);
        assert_eq!(
            published_model.capability_evidence,
            published_variant.capability_evidence
        );

        let auto = runtime
            .resolve_connection_model_selection(
                &configuration,
                "multi-account",
                "custom:multi",
                "auto/model",
            )
            .expect("auto model must resolve");
        let binding = &auto.protocol_binding;
        assert_eq!(
            binding.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
            "auto must pick the highest fixed-priority protocol even when declared last"
        );
        assert_eq!(
            binding.source,
            crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
        );
        assert_eq!(
            auto.route.protocol_route.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
            "the effective execution protocol must follow the auto binding"
        );
    }

    /// Verifies Auto intersects provider protocols with this exact model's discovered variants.
    /// 验证 Auto 会把供应商协议与当前模型实际发现的协议变体取交集。
    #[test]
    fn custom_multi_protocol_auto_uses_exact_model_available_protocols() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = multi_protocol_custom_definition(
            "custom:model-intersection",
            &[
                crate::CustomProviderProtocol::OpenAiResponses,
                crate::CustomProviderProtocol::AnthropicMessages,
            ],
        );
        install_current_custom_provider(&mut configuration, provider);
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("model-intersection-account", "custom:model-intersection"),
            )
            .expect("multi-protocol connection should register");
        let catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "model-intersection-account",
                1,
                r#"{"data":[{"id":"anthropic-only/model","input_modalities":["text"],"output_modalities":["text"]}]}"#,
            )
            .expect("model-intersection catalog should parse"),
            &[ANTHROPIC_MESSAGES_PROFILE_ID],
        );
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("model-intersection catalog should install");
        configuration.set_model_enabled("custom:model-intersection", "anthropic-only/model", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("model-intersection catalog should publish");

        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "model-intersection-account",
                "custom:model-intersection",
                "anthropic-only/model",
            )
            .expect("the model's discovered Anthropic route must remain executable");
        let binding = selection.protocol_binding;
        assert_eq!(
            binding.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID,
            "provider-wide Responses support must not fabricate Responses support for this model"
        );
        assert_eq!(
            binding.source,
            crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
        );
        let published_catalogs = runtime.catalog_snapshot().expect("catalog snapshot");
        let published_model = published_catalogs["model-intersection-account"]
            .models
            .first()
            .expect("published model");
        assert_eq!(published_model.protocol_variants.len(), 1);
        assert!(
            published_model
                .protocol_variants
                .contains_key(ANTHROPIC_MESSAGES_PROFILE_ID)
        );
    }

    /// Verifies a higher-priority protocol addition invalidates Auto but preserves explicit binding.
    /// 验证新增更高优先级协议会使 Auto 绑定失效，但保留显式绑定。
    #[test]
    fn custom_auto_binding_rebinds_when_protocol_priority_changes() {
        // autoBinding represents the catalog frozen while only Anthropic was enabled.
        // autoBinding 表示仅启用 Anthropic 时由目录冻结的绑定。
        let auto_binding = crate::provider_manifest::ModelProtocolBinding {
            protocol_profile_id: ANTHROPIC_MESSAGES_PROFILE_ID.to_string(),
            source: crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault,
            manifest_revision: 1,
            rule_id: String::new(),
        };
        assert!(frozen_binding_is_current(
            &auto_binding,
            None,
            &[ANTHROPIC_MESSAGES_PROFILE_ID],
            &[ANTHROPIC_MESSAGES_PROFILE_ID],
        ));
        assert!(
            !frozen_binding_is_current(
                &auto_binding,
                None,
                &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,],
                &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,],
            ),
            "adding Responses must invalidate an older lower-priority Auto binding"
        );

        // explicitBinding remains authoritative while its exact protocol stays enabled.
        // explicitBinding 在其精确协议仍启用时保持权威。
        let explicit_binding = crate::provider_manifest::ModelProtocolBinding {
            protocol_profile_id: ANTHROPIC_MESSAGES_PROFILE_ID.to_string(),
            source: crate::provider_manifest::ModelProtocolBindingSource::CustomUser,
            manifest_revision: 1,
            rule_id: String::new(),
        };
        let explicit_preference = crate::CustomModelProtocolPreference::Explicit {
            protocol: crate::CustomProviderProtocol::AnthropicMessages,
        };
        assert!(frozen_binding_is_current(
            &explicit_binding,
            Some(&explicit_preference),
            &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,],
            &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,],
        ));
    }

    /// Verifies a custom Gemini binding projects Gemini evidence instead of the connection default.
    /// 验证自定义 Gemini 绑定投影 Gemini 证据，而不是连接默认协议证据。
    #[test]
    fn custom_protocol_variant_policy_follows_effective_binding() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = multi_protocol_custom_definition(
            "custom:variant-policy",
            &[
                crate::CustomProviderProtocol::OpenAiResponses,
                crate::CustomProviderProtocol::GeminiGenerateContent,
            ],
        );
        install_current_custom_provider(&mut configuration, provider);
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("variant-policy-account", "custom:variant-policy"),
            )
            .expect("custom connection should register");
        configuration.custom_model_protocol_preferences.insert(
            PersistedAiConfiguration::provider_model_key("custom:variant-policy", "variant/model"),
            crate::CustomModelProtocolPreference::Explicit {
                protocol: crate::CustomProviderProtocol::GeminiGenerateContent,
            },
        );
        let catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "variant-policy-account",
                1,
                r#"{"data":[{"id":"variant/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
            )
            .expect("custom catalog should parse"),
            &[
                OPENAI_RESPONSES_PROFILE_ID,
                GEMINI_GENERATE_CONTENT_PROFILE_ID,
            ],
        );
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("custom catalog should install");
        configuration.set_model_enabled("custom:variant-policy", "variant/model", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("protocol-aware variants should publish");

        let published_catalogs = runtime
            .catalog_snapshot()
            .expect("published catalogs should remain readable");
        let model = published_catalogs
            .get("variant-policy-account")
            .and_then(|catalog| catalog.models.first())
            .expect("the custom model must remain published");
        let binding = model
            .resolved_protocol_binding
            .as_ref()
            .expect("the explicit Gemini binding must freeze");
        assert_eq!(
            binding.protocol_profile_id,
            GEMINI_GENERATE_CONTENT_PROFILE_ID
        );
        let responses = model
            .protocol_variants
            .get(OPENAI_RESPONSES_PROFILE_ID)
            .expect("the Responses variant must exist");
        let gemini = model
            .protocol_variants
            .get(GEMINI_GENERATE_CONTENT_PROFILE_ID)
            .expect("the Gemini variant must exist");
        assert_eq!(responses.capabilities.reasoning, CapabilitySupport::Native);
        assert_ne!(gemini.capabilities.reasoning, CapabilitySupport::Native);
        assert_eq!(model.capabilities, gemini.capabilities);
        assert_eq!(model.capability_evidence, gemini.capability_evidence);
    }

    /// Verifies custom bindings freeze at the catalog activation boundary: a preference
    /// change without a catalog rebuild never re-binds an in-flight session, and the next
    /// activation-boundary rebuild applies it (plan §13.6).
    /// 验证自定义绑定在目录激活边界冻结：未重建目录的偏好变更绝不会重新绑定进行中的
    /// 会话，下一次激活边界重建才应用它（方案 §13.6）。
    #[test]
    fn frozen_custom_binding_only_rebinds_at_activation_boundary() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = multi_protocol_custom_definition(
            "custom:frozen",
            &[
                crate::CustomProviderProtocol::OpenAiChat,
                crate::CustomProviderProtocol::AnthropicMessages,
                crate::CustomProviderProtocol::OpenAiResponses,
            ],
        );
        install_current_custom_provider(&mut configuration, provider);
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("frozen-account", "custom:frozen"),
            )
            .expect("frozen connection should register");
        let catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "frozen-account",
                1,
                r#"{"data":[{"id":"frozen/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
            )
            .expect("frozen catalog should parse"),
            &[
                OPENAI_CHAT_PROFILE_ID,
                ANTHROPIC_MESSAGES_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
            ],
        );
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("frozen catalog should install");
        configuration.set_model_enabled("custom:frozen", "frozen/model", true);
        assert!(
            runtime
                .catalog_snapshot()
                .expect("published catalogs should remain readable")
                .is_empty(),
            "a staged effective catalog must not leak through the published snapshot"
        );
        assert!(
            runtime
                .source_catalog_snapshot()
                .expect("published source catalogs should remain readable")
                .is_empty(),
            "a staged source catalog must not leak through the published snapshot"
        );
        assert!(
            runtime
                .staged_source_catalog("frozen-account")
                .expect("staged validation catalog should remain readable")
                .is_some(),
            "the validation-only staged accessor must retain the exact candidate"
        );
        assert!(
            !runtime
                .model_is_available(&configuration, "custom:frozen", "frozen/model")
                .expect("published availability should resolve"),
            "model availability must not combine staged catalogs with published manifests"
        );
        assert!(
            runtime
                .available_model_selectors(&configuration)
                .expect("published selectors should resolve")
                .is_empty(),
            "selector enumeration must not expose an unactivated catalog"
        );
        assert!(
            runtime
                .resolved_model_catalog_snapshot(&configuration)
                .expect("published overlay reconstruction should remain readable")
                .is_empty(),
            "support-catalog reconstruction must not read a staged discovery source"
        );
        assert!(
            !runtime
                .provider_conversation_model_is_supported(
                    &configuration,
                    "custom:frozen",
                    "frozen/model",
                )
                .expect("published provider support should resolve"),
            "provider support must not combine staged sources with a published manifest generation"
        );
        // The activation boundary freezes the auto binding (highest fixed priority).
        // 激活边界冻结 Auto 绑定（最高固定优先级）。
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("overlays should reapply");
        let binding = runtime
            .resolve_connection_model_selection(
                &configuration,
                "frozen-account",
                "custom:frozen",
                "frozen/model",
            )
            .expect("frozen model must resolve")
            .protocol_binding;
        assert_eq!(binding.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID);

        // A preference change is a configuration activation boundary: the next resolution
        // detects the drift and re-freezes the binding without any catalog rebuild, while
        // unchanged models keep their frozen binding untouched.
        // 偏好变更是配置激活边界：下一次解析检测漂移并重新冻结绑定，无需任何目录重建，
        // 未变化模型保留其冻结绑定不动。
        configuration.custom_model_protocol_preferences.insert(
            PersistedAiConfiguration::provider_model_key("custom:frozen", "frozen/model"),
            crate::CustomModelProtocolPreference::Explicit {
                protocol: crate::CustomProviderProtocol::AnthropicMessages,
            },
        );
        // The preference only takes effect after the next explicit activation boundary.
        // 偏好只在下一次显式激活边界后生效。
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("overlays should reapply");
        let rebound = runtime
            .resolve_connection_model_selection(
                &configuration,
                "frozen-account",
                "custom:frozen",
                "frozen/model",
            )
            .expect("frozen model must rebind")
            .protocol_binding;
        assert_eq!(
            rebound.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID,
            "a preference drift must re-freeze the binding at the activation boundary"
        );
        assert_eq!(
            rebound.source,
            crate::provider_manifest::ModelProtocolBindingSource::CustomUser
        );

        // Removing the explicit preference re-freezes Auto; the explicit binding never
        // survives a configuration activation that no longer declares it.
        // 移除显式偏好会重新冻结 Auto；显式绑定绝不存活于不再声明它的配置激活。
        configuration.custom_model_protocol_preferences.remove(
            PersistedAiConfiguration::provider_model_key("custom:frozen", "frozen/model").as_str(),
        );
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("overlays should reapply");
        let auto_again = runtime
            .resolve_connection_model_selection(
                &configuration,
                "frozen-account",
                "custom:frozen",
                "frozen/model",
            )
            .expect("frozen model must resolve again")
            .protocol_binding;
        assert_eq!(
            auto_again.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
            "removing the explicit preference must re-freeze the fixed-priority auto binding"
        );
        assert_eq!(
            auto_again.source,
            crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
        );

        // A frozen binding whose protocol leaves the enabled set is never re-selected.
        // 协议离开启用集合的冻结绑定绝不被重新选择。
        let mut provider = configuration
            .custom_providers
            .remove("custom:frozen")
            .expect("frozen provider must exist");
        provider.service_entries[0]
            .protocol_routes
            .retain(|route| route.protocol_profile_id != OPENAI_RESPONSES_PROFILE_ID);
        install_current_custom_provider(&mut configuration, provider);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("overlays should reapply");
        let narrowed = runtime
            .resolve_connection_model_selection(
                &configuration,
                "frozen-account",
                "custom:frozen",
                "frozen/model",
            )
            .expect("frozen model must resolve under the narrowed set")
            .protocol_binding;
        assert_ne!(
            narrowed.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
            "a frozen binding outside the enabled set must be re-frozen away"
        );

        // Reintroducing the higher-priority protocol is another activation-boundary change:
        // Auto must return to Responses even though the lower-priority binding remains enabled.
        // 重新引入更高优先级协议是另一次激活边界变更：即使较低优先级绑定仍然启用，
        // Auto 也必须返回 Responses。
        let widened = multi_protocol_custom_definition(
            "custom:frozen",
            &[
                crate::CustomProviderProtocol::OpenAiChat,
                crate::CustomProviderProtocol::AnthropicMessages,
                crate::CustomProviderProtocol::OpenAiResponses,
            ],
        );
        install_current_custom_provider(&mut configuration, widened);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("overlays should reapply after widening the protocol set");
        let widened_auto = runtime
            .resolve_connection_model_selection(
                &configuration,
                "frozen-account",
                "custom:frozen",
                "frozen/model",
            )
            .expect("frozen model must resolve under the widened set")
            .protocol_binding;
        assert_eq!(
            widened_auto.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
            "Auto must re-freeze to the newly enabled highest-priority protocol"
        );
        assert_eq!(
            widened_auto.source,
            crate::provider_manifest::ModelProtocolBindingSource::CustomAutoDefault
        );
    }

    /// Verifies an explicit preference is honored, never refreshed away, and requires re-selection
    /// when the referenced protocol leaves the enabled set (plan §13.3.4/13.3.6).
    /// 验证显式偏好被遵守、不会被刷新覆盖，且引用协议离开启用集合时必须重新选择（方案 §13.3.4/13.3.6）。
    #[test]
    fn custom_explicit_preference_survives_and_requires_reselection_after_protocol_removal() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = multi_protocol_custom_definition(
            "custom:explicit",
            &[
                crate::CustomProviderProtocol::OpenAiChat,
                crate::CustomProviderProtocol::AnthropicMessages,
                crate::CustomProviderProtocol::OpenAiResponses,
            ],
        );
        install_current_custom_provider(&mut configuration, provider);
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("explicit-account", "custom:explicit"),
            )
            .expect("explicit connection should register");
        let catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "explicit-account",
                1,
                r#"{"data":[{"id":"explicit/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
            )
            .expect("explicit catalog should parse"),
            &[
                OPENAI_CHAT_PROFILE_ID,
                ANTHROPIC_MESSAGES_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
            ],
        );
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("explicit catalog should install");
        configuration.set_model_enabled("custom:explicit", "explicit/model", true);
        // The user explicitly pins anthropic despite the auto default being responses.
        // 尽管 Auto 默认是 responses，用户显式固定 anthropic。
        configuration.custom_model_protocol_preferences.insert(
            PersistedAiConfiguration::provider_model_key("custom:explicit", "explicit/model"),
            crate::CustomModelProtocolPreference::Explicit {
                protocol: crate::CustomProviderProtocol::AnthropicMessages,
            },
        );
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("explicit custom generation should publish");

        let selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "explicit-account",
                "custom:explicit",
                "explicit/model",
            )
            .expect("explicit model must resolve");
        let binding = &selection.protocol_binding;
        assert_eq!(
            binding.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID,
            "the explicit user choice must never be refreshed away"
        );
        assert_eq!(
            binding.source,
            crate::provider_manifest::ModelProtocolBindingSource::CustomUser
        );
        assert_eq!(
            selection.route.protocol_route.protocol_profile_id,
            ANTHROPIC_MESSAGES_PROFILE_ID
        );

        // Removing the anthropic protocol makes the explicit model require re-selection; the
        // resolver must never silently move it to the next protocol.
        // 移除 anthropic 协议后显式模型必须重新选择；解析器绝不能静默移到下一协议。
        let narrowed = multi_protocol_custom_definition(
            "custom:explicit",
            &[crate::CustomProviderProtocol::OpenAiChat],
        );
        install_current_custom_provider(&mut configuration, narrowed);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("the invalid explicit preference must publish a blocked catalog state");
        let blocked_catalogs = runtime
            .catalog_snapshot()
            .expect("blocked custom catalogs should remain readable");
        let blocked_model = blocked_catalogs
            .get("explicit-account")
            .and_then(|catalog| catalog.models.first())
            .expect("the blocked model must remain visible for re-selection");
        assert!(blocked_model.resolved_protocol_binding.is_none());
        assert_eq!(blocked_model.capabilities, ModelCapabilities::default());
        assert_eq!(
            blocked_model.capability_evidence,
            ModelCapabilityEvidence::default()
        );
        let error = runtime
            .resolve_connection_model_selection(
                &configuration,
                "explicit-account",
                "custom:explicit",
                "explicit/model",
            )
            .map(|_| ())
            .expect_err("explicit model referencing a removed protocol must fail");
        assert!(
            error.contains("re-selection")
                || error.contains("not executable")
                || error.contains("no protocol binding")
                || error.contains("not registered"),
            "unexpected re-selection error: {error}"
        );

        // After the user moves the preference to Auto, the same narrowed provider binds chat.
        // 用户将偏好改为 Auto 后，同一收窄供应商绑定 chat。
        configuration.custom_model_protocol_preferences.insert(
            PersistedAiConfiguration::provider_model_key("custom:explicit", "explicit/model"),
            crate::CustomModelProtocolPreference::Auto,
        );
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("auto custom generation should publish");
        let auto = runtime
            .resolve_connection_model_selection(
                &configuration,
                "explicit-account",
                "custom:explicit",
                "explicit/model",
            )
            .expect("auto preference must rebind on the narrowed provider");
        assert_eq!(
            auto.route.protocol_route.protocol_profile_id,
            OPENAI_CHAT_PROFILE_ID
        );
    }

    /// Verifies the DeepSeek manifest supplies exact protocol variants for its current identities.
    /// 验证 DeepSeek Manifest 为当前模型身份提供精确协议变体。
    #[test]
    fn deepseek_manifest_identity_supplies_current_protocol_variants() {
        // runtime exposes the same compiled bundled manifest used by production startup.
        // runtime 暴露生产启动使用的同一份已编译内置 Manifest。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // configuration owns the persisted connection snapshot activated by the runtime.
        // configuration 持有由运行时激活的持久化连接快照。
        let mut configuration = PersistedAiConfiguration::default();
        // deepseekConnection reproduces an existing credential created before Responses was added.
        // deepseekConnection 复现一个在加入 Responses 前创建的既有凭据。
        let mut deepseek_connection = connection("deepseek-existing-account");
        deepseek_connection.provider_id = String::from("deepseek");
        deepseek_connection.service_entry_id = String::from("official-api");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                deepseek_connection,
            )
            .expect("existing DeepSeek connection should register");
        configuration.set_model_enabled("deepseek", "deepseek-v4-flash", true);
        configuration.set_model_enabled("deepseek", "deepseek-v4-flash-vision-exp", true);
        configuration.set_model_enabled("deepseek", "deepseek-v4-pro", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("DeepSeek fixed manifest catalog should publish");
        // catalogs is the production projection consumed by workbench selection and execution.
        // catalogs 是工作台选择与执行消费的生产投影。
        let catalogs = runtime
            .catalog_snapshot()
            .expect("published DeepSeek catalog should remain readable");
        // catalog is the exact existing-account projection built without model discovery.
        // catalog 是无需模型发现构建的精确既有账号投影。
        let catalog = catalogs
            .get("deepseek-existing-account")
            .expect("existing DeepSeek account catalog should exist");
        assert_eq!(catalog.models.len(), 3);
        // flash must bind Responses and retain all three protocol variants.
        // flash 必须绑定 Responses 并保留全部三个协议变体。
        let flash = catalog
            .models
            .iter()
            .find(|model| model.id == "deepseek-v4-flash")
            .expect("DeepSeek V4 Flash must exist");
        assert_eq!(
            flash
                .resolved_protocol_binding
                .as_ref()
                .map(|binding| binding.protocol_profile_id.as_str()),
            Some(OPENAI_RESPONSES_PROFILE_ID)
        );
        assert_eq!(
            flash
                .protocol_variants
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                ANTHROPIC_MESSAGES_PROFILE_ID,
                OPENAI_CHAT_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
            ])
        );
        // vision must retain the Flash protocol contract while adding exact image input support.
        // vision 必须保留 Flash 协议契约，同时增加精确的图片输入支持。
        let vision = catalog
            .models
            .iter()
            .find(|model| model.id == "deepseek-v4-flash-vision-exp")
            .expect("DeepSeek V4 Flash Vision Exp must exist");
        assert_eq!(
            vision
                .resolved_protocol_binding
                .as_ref()
                .map(|binding| binding.protocol_profile_id.as_str()),
            Some(OPENAI_RESPONSES_PROFILE_ID)
        );
        assert_eq!(
            vision
                .protocol_variants
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                ANTHROPIC_MESSAGES_PROFILE_ID,
                OPENAI_CHAT_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
            ])
        );
        assert_eq!(
            vision.capabilities.input_resources,
            vec![vulcan_ai_contract::ResourceKind::Image]
        );
        // pro must bind Responses and retain the same three protocol variants as flash.
        // pro 必须绑定 Responses，并保留与 flash 相同的三个协议变体。
        let pro = catalog
            .models
            .iter()
            .find(|model| model.id == "deepseek-v4-pro")
            .expect("DeepSeek V4 Pro must exist");
        assert_eq!(
            pro.resolved_protocol_binding
                .as_ref()
                .map(|binding| binding.protocol_profile_id.as_str()),
            Some(OPENAI_RESPONSES_PROFILE_ID)
        );
        assert_eq!(
            pro.protocol_variants
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                ANTHROPIC_MESSAGES_PROFILE_ID,
                OPENAI_CHAT_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
            ])
        );
        // flashSelection proves the stale persisted protocol set cannot block a newer signed route.
        // flashSelection 证明陈旧的持久化协议集合无法阻止较新的签名路由。
        let flash_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "deepseek-existing-account",
                "deepseek",
                "deepseek-v4-flash",
            )
            .expect("existing DeepSeek account should execute Flash through Responses");
        assert_eq!(
            flash_selection.route.protocol_route.protocol_profile_id,
            OPENAI_RESPONSES_PROFILE_ID
        );
        assert_eq!(
            flash_selection.model.capabilities.provider_native_tools,
            vec![vulcan_ai_contract::NativeToolKind::WebSearch],
            "the executable Flash Responses route must carry native web search"
        );
        // visionSelection proves the executable route carries both image constraints and web search.
        // visionSelection 证明可执行路由同时携带图片约束与联网搜索能力。
        let vision_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "deepseek-existing-account",
                "deepseek",
                "deepseek-v4-flash-vision-exp",
            )
            .expect("existing DeepSeek account should execute Vision through Responses");
        assert_eq!(
            vision_selection.route.protocol_route.protocol_profile_id,
            OPENAI_RESPONSES_PROFILE_ID
        );
        assert_eq!(
            vision_selection.model.capabilities.input_resources,
            vec![vulcan_ai_contract::ResourceKind::Image]
        );
        assert_eq!(
            vision_selection.model.capabilities.provider_native_tools,
            vec![vulcan_ai_contract::NativeToolKind::WebSearch],
            "the executable Vision Responses route must carry native web search"
        );
        // imageConstraint is the route-level attachment contract consumed by request preparation.
        // imageConstraint 是请求准备阶段消费的路由级附件契约。
        let image_constraint = vision_selection
            .route
            .protocol_route
            .input_resource_constraints
            .first()
            .expect("Vision route must carry its image input constraint");
        assert_eq!(
            image_constraint.resource_kind,
            vulcan_ai_contract::ResourceKind::Image
        );
        assert_eq!(image_constraint.max_bytes, Some(32 * 1024 * 1024));
        assert_eq!(image_constraint.max_count, Some(600));
        assert_eq!(
            image_constraint.reference_policy,
            crate::ResourceReferencePolicy::LocalOnly
        );
        assert!(image_constraint.upload_channel_refs.is_empty());
        // proSelection proves V4 Pro now follows the exact same Responses route contract.
        // proSelection 证明 V4 Pro 现在遵循完全相同的 Responses 路由契约。
        let pro_selection = runtime
            .resolve_connection_model_selection(
                &configuration,
                "deepseek-existing-account",
                "deepseek",
                "deepseek-v4-pro",
            )
            .expect("existing DeepSeek account should execute Pro through Responses");
        assert_eq!(
            pro_selection.route.protocol_route.protocol_profile_id,
            OPENAI_RESPONSES_PROFILE_ID
        );
        assert_eq!(
            pro_selection.model.capabilities.provider_native_tools,
            vec![vulcan_ai_contract::NativeToolKind::WebSearch],
            "the executable Pro Responses route must carry native web search"
        );
    }

    /// Verifies a hot-replaced system manifest drives new bindings on the next resolution.
    /// 验证热替换的系统清单驱动下一次解析的新绑定。
    #[tokio::test]
    async fn hot_replaced_manifest_drives_new_bindings() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let mut deepseek_connection = connection("one");
        deepseek_connection.provider_id = String::from("deepseek");
        deepseek_connection.service_entry_id = String::from("official-api");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                deepseek_connection,
            )
            .expect("connection should register");
        let catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "one",
                1,
                r#"{"data":[{"id":"deepseek-v4-pro","context_window":131072,"max_output_tokens":8192,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
            )
            .expect("openai catalog should parse"),
            &[
                ANTHROPIC_MESSAGES_PROFILE_ID,
                OPENAI_CHAT_PROFILE_ID,
            ],
        );
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("catalog should install");
        configuration.set_model_enabled("deepseek", "deepseek-v4-pro", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("initial deepseek generation should publish");

        // The bundled manifest binds v4-pro to Responses by rule.
        // 内置清单按规则把 v4-pro 绑定到 Responses。
        let before = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "deepseek",
                "deepseek-v4-pro",
            )
            .expect("bundled binding must resolve");
        assert_eq!(
            before.route.protocol_route.protocol_profile_id,
            OPENAI_RESPONSES_PROFILE_ID
        );

        // A fully compiled replacement re-binds the same rule to chat; the write lock is held
        // only for the in-memory insertion.
        // 完全编译的替换件把同一规则重新绑定到 chat；写锁仅为内存插入而持有。
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled deepseek manifest must exist");
        let mut manifest: crate::provider_manifest::ProviderManifestV1 =
            serde_json::from_str(&text).expect("manifest should parse");
        for rule in &mut manifest.model_rules {
            let matches_pro = rule.matcher.value == "deepseek-v4-pro";
            if matches_pro {
                rule.selected_protocol = Some(String::from("openai-chat-completions.v1"));
            }
        }
        let replacement_text =
            serde_json::to_string_pretty(&manifest).expect("manifest should serialize");
        let replaced = crate::provider_manifest::compile_manifest_text(
            &replacement_text,
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("replacement manifest must compile");
        runtime
            .replace_system_manifest("deepseek", Arc::new(replaced))
            .expect("hot replace should succeed");
        let staged = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "deepseek",
                "deepseek-v4-pro",
            )
            .expect("selection must retain the published generation before activation");
        assert_eq!(
            staged.route.protocol_route.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
            "a staged manifest must remain invisible before the explicit activation boundary"
        );
        runtime
            .activate_staged_manifest_catalogs(&configuration, &[])
            .await
            .expect("replacement deepseek generation should publish");

        let after = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "deepseek",
                "deepseek-v4-pro",
            )
            .expect("replaced binding must resolve");
        assert_eq!(
            after.route.protocol_route.protocol_profile_id, OPENAI_CHAT_PROFILE_ID,
            "the hot-replaced manifest must drive the next resolution"
        );
    }

    /// Verifies atomic generation activation excludes provider-tool connections from model projection.
    /// 验证原子代际激活会从模型投影中排除供应商工具连接。
    #[tokio::test]
    async fn atomic_manifest_activation_skips_provider_tool_connections() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let connection_id = String::from("tavily-provider-tool-account");
        let mut tavily_connection = connection(connection_id.as_str());
        tavily_connection.provider_id = String::from("tavily");
        tavily_connection.service_entry_id = String::from("public-api");
        tavily_connection.credential_profile_id = String::from("api-key");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                tavily_connection,
            )
            .expect("Tavily provider-tool connection should register");

        runtime
            .activate_staged_manifest_catalogs(&configuration, std::slice::from_ref(&connection_id))
            .await
            .expect("provider-tool connection must not block generation activation");

        assert!(
            !runtime
                .catalog_snapshot()
                .expect("catalog snapshot should resolve")
                .contains_key(connection_id.as_str()),
            "provider-tool connection must not acquire a conversation model catalog"
        );
    }

    /// Verifies ordinary refreshes cannot mix a published provider with a staged manifest.
    /// 验证普通刷新不能把已发布供应商与暂存清单混合使用。
    #[tokio::test]
    async fn staged_manifest_catalogs_publish_only_through_activation_batch() {
        let runtime = PublicAiRuntime::first_batch(Arc::new(TestStaticSecretStore::api_key()))
            .expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let connection_id = String::from("alibaba-activation-account");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                CredentialConnection {
                    id: connection_id.clone(),
                    provider_id: String::from("alibaba"),
                    service_entry_id: String::from("coding-plan-cn"),
                    credential_profile_id: String::from("coding-plan-api-key"),
                    credential_mode: CredentialMode::Token,
                    credential_id: String::from("credential-alibaba-activation"),
                    display_name: String::from("Alibaba Activation Account"),
                    secret_reference: String::from("secret-alibaba-activation"),
                    management_secret_reference: None,
                    service_origin_override: None,
                    prompt_cache_by_protocol: BTreeMap::new(),
                },
            )
            .expect("Alibaba connection should register");
        let initial = runtime
            .refresh_connection_catalog(&configuration, &connection_id)
            .await
            .expect("bundled static catalog should refresh");
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("initial catalog generation should publish");

        let future_model_id = "qwen-manifest-activation-fixture";
        assert!(
            initial
                .models
                .iter()
                .all(|model| model.id != future_model_id),
            "the bundled catalog must not already contain the future fixture"
        );
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("alibaba.json"))
            .expect("bundled Alibaba manifest must exist");
        let mut document = serde_json::from_str::<serde_json::Value>(&text)
            .expect("Alibaba manifest should parse");
        let former_revision = document["revision"]
            .as_u64()
            .expect("Alibaba revision must exist");
        document["revision"] = serde_json::Value::from(former_revision + 1);
        let services = document["services"]
            .as_array_mut()
            .expect("Alibaba services must be an array");
        let coding_plan = services
            .iter_mut()
            .find(|service| service["service_entry_id"] == "coding-plan-cn")
            .expect("Alibaba coding plan entry must exist");
        coding_plan["model_catalog"]["models"]
            .as_array_mut()
            .expect("Alibaba static models must be an array")
            .push(serde_json::json!({
                "model_id": future_model_id,
                "display_name": "Qwen Manifest Activation Fixture"
            }));
        // base_rule is the reviewed rule for this exact service, not another product sharing the model id.
        // base_rule 是此精确服务的已审核规则，而不是共享模型 ID 的其他产品规则。
        let base_rule = document["model_rules"]
            .as_array()
            .expect("Alibaba model rules must be an array")
            .iter()
            .find(|rule| {
                rule["matcher"]["value"] == "qwen3.7-plus"
                    && rule["scope"]["service_entry_ids"]
                        .as_array()
                        .is_some_and(|services| {
                            services.iter().any(|service| service == "coding-plan-cn")
                        })
            })
            .cloned()
            .expect("Alibaba Coding Plan Qwen rule must exist");
        // future_rule changes only bounded identity facts and retains the pre-authorized contract.
        // future_rule 仅修改有界身份事实并保留预授权契约。
        let mut future_rule = base_rule;
        future_rule["rule_id"] =
            serde_json::Value::String(String::from("alibaba-manifest-activation-fixture"));
        future_rule["matcher"]["value"] = serde_json::Value::String(future_model_id.to_string());
        document["model_rules"]
            .as_array_mut()
            .expect("Alibaba model rules must be mutable")
            .push(future_rule);
        let replacement = crate::provider_manifest::compile_manifest_text(
            &serde_json::to_string_pretty(&document).expect("manifest should serialize"),
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("replacement manifest must compile");
        runtime
            .replace_system_manifest("alibaba", Arc::new(replacement))
            .expect("replacement manifest should stage");

        let ordinary = runtime
            .refresh_connection_catalog(&configuration, &connection_id)
            .await
            .expect("ordinary refresh should still use the published manifest");
        assert!(
            ordinary
                .models
                .iter()
                .all(|model| model.id != future_model_id),
            "ordinary refresh must not observe an unpublished manifest"
        );
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("ordinary reapply should retain the published manifest generation");
        assert_eq!(
            runtime
                .system_manifest("alibaba")
                .expect("published Alibaba manifest should exist")
                .revision,
            former_revision,
            "staging must not alter the published generation"
        );

        runtime
            .activate_staged_manifest_catalogs(&configuration, std::slice::from_ref(&connection_id))
            .await
            .expect("staged manifest activation should succeed");
        assert_eq!(
            runtime
                .system_manifest("alibaba")
                .expect("activated Alibaba manifest should exist")
                .revision,
            former_revision + 1
        );
        let activated = runtime.catalog_snapshot().expect("catalog snapshot");
        assert!(
            activated[&connection_id]
                .models
                .iter()
                .any(|model| model.id == future_model_id),
            "the newly declared model must appear in the same published activation"
        );
    }

    /// Verifies account-plan model access is read from manifest rules instead of provider code.
    /// 验证账号套餐模型访问从 Manifest 规则读取而不是来自供应商代码。
    #[test]
    fn manifest_account_plan_allowlist_filters_catalog_models() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let manifest = runtime
            .system_manifest("kimi")
            .expect("Kimi manifest must exist");
        let mut configuration = PersistedAiConfiguration::default();
        let mut plan_connection = connection("kimi-plan-account");
        plan_connection.provider_id = String::from("kimi");
        plan_connection.service_entry_id = String::from("coding-plan");
        plan_connection.credential_profile_id = String::from("coding-api-key");
        configuration
            .connection_graph
            .connections
            .insert(plan_connection.id.clone(), plan_connection);
        let source = with_protocol_variants(
            parse_openai_model_catalog(
                "kimi-plan-account",
                1,
                r#"{"data":[{"id":"kimi-for-coding"},{"id":"kimi-for-coding-highspeed"},{"id":"k3-256k"},{"id":"k3"},{"id":"future-plan-model"}]}"#,
            )
            .expect("fixture catalog should parse"),
            &[
                OPENAI_CHAT_PROFILE_ID,
                ANTHROPIC_MESSAGES_PROFILE_ID,
                ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            ],
        );
        // moderato_usage is the exact provider evidence consumed by Kimi's configured plan-name policy.
        // moderato_usage 是 Kimi 已配置套餐名称策略消费的精确供应商证据。
        let moderato_usage = AccountUsageSnapshot {
            credential_id: String::from("kimi-plan-account"),
            availability: vulcan_ai_contract::AccountUsageAvailability::Supported,
            plan_name: Some(String::from("Moderato")),
            plan_id: None,
            used_amount: None,
            remaining_amount: None,
            currency: None,
            balance_amount: None,
            quota_windows: Vec::new(),
            observed_at: None,
            refresh_message: None,
        };
        let mut moderato = source.clone();
        apply_model_catalog_overlays_with_account_access(
            &configuration,
            "kimi",
            "coding-plan",
            Some(&moderato_usage),
            manifest
                .model_source_policy("coding-plan")
                .expect("Kimi coding-plan source policy must compile"),
            Some(manifest.as_ref()),
            &mut moderato,
        )
        .expect("manifest plan filter should apply");
        let visible = moderato
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(visible.contains("kimi-for-coding"));
        assert!(visible.contains("k3-256k"));
        assert!(visible.contains("future-plan-model"));
        assert!(!visible.contains("kimi-for-coding-highspeed"));
        assert!(!visible.contains("k3"));

        let mut unknown_plan = source;
        apply_model_catalog_overlays_with_account_access(
            &configuration,
            "kimi",
            "coding-plan",
            None,
            manifest
                .model_source_policy("coding-plan")
                .expect("Kimi coding-plan source policy must compile"),
            Some(manifest.as_ref()),
            &mut unknown_plan,
        )
        .expect("missing plan evidence must not hide models");
        assert_eq!(unknown_plan.models.len(), 5);
    }

    /// Verifies CommandCode plan groups, exact exclusions, and quota unlocks are Manifest-driven.
    /// 验证 CommandCode 套餐分组、精确排除项与额度解锁均由 Manifest 驱动。
    #[test]
    fn manifest_account_model_access_projects_commandcode_catalog() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let manifest = runtime
            .system_manifest("commandcode")
            .expect("CommandCode manifest must exist");
        let mut configuration = PersistedAiConfiguration::default();
        let mut plan_connection = connection("commandcode-plan-account");
        plan_connection.provider_id = String::from("commandcode");
        plan_connection.service_entry_id = String::from("provider-api");
        plan_connection.credential_profile_id = String::from("user-api-key");
        configuration
            .connection_graph
            .connections
            .insert(plan_connection.id.clone(), plan_connection);
        let source = with_protocol_variants(
            parse_openai_model_catalog(
                "commandcode-plan-account",
                1,
                r#"{"data":[{"id":"gpt-5.5"},{"id":"deepseek/deepseek-v4-flash"},{"id":"meta/muse-spark-1.2"},{"id":"google/gemini-3.6-flash"},{"id":"claude-fable-5"}]}"#,
            )
            .expect("fixture catalog should parse"),
            &[OPENAI_CHAT_PROFILE_ID, ANTHROPIC_MESSAGES_PROFILE_ID],
        );
        let source_policy = manifest
            .model_source_policy("provider-api")
            .expect("CommandCode source policy must compile");

        let go_usage = account_usage(
            "credential-commandcode-plan-account",
            Some("Go"),
            Some("individual-go"),
            Vec::new(),
        );
        let mut go_catalog = source.clone();
        apply_model_catalog_overlays_with_account_access(
            &configuration,
            "commandcode",
            "provider-api",
            Some(&go_usage),
            source_policy,
            Some(manifest.as_ref()),
            &mut go_catalog,
        )
        .expect("Go plan access should apply");
        let go_models = go_catalog
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(go_models.contains("deepseek/deepseek-v4-flash"));
        assert!(go_models.contains("google/gemini-3.6-flash"));
        assert!(!go_models.contains("gpt-5.5"));
        assert!(!go_models.contains("meta/muse-spark-1.2"));
        assert!(!go_models.contains("claude-fable-5"));

        let pro_usage = account_usage(
            "credential-commandcode-plan-account",
            Some("Pro"),
            Some("individual-pro"),
            Vec::new(),
        );
        let mut pro_catalog = source.clone();
        apply_model_catalog_overlays_with_account_access(
            &configuration,
            "commandcode",
            "provider-api",
            Some(&pro_usage),
            source_policy,
            Some(manifest.as_ref()),
            &mut pro_catalog,
        )
        .expect("Pro plan access should apply");
        let pro_models = pro_catalog
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(pro_models.contains("gpt-5.5"));
        assert!(pro_models.contains("deepseek/deepseek-v4-flash"));
        assert!(!pro_models.contains("claude-fable-5"));

        let unknown_usage = account_usage(
            "credential-commandcode-plan-account",
            Some("Future"),
            Some("future-plan"),
            Vec::new(),
        );
        let mut unknown_catalog = source.clone();
        apply_model_catalog_overlays_with_account_access(
            &configuration,
            "commandcode",
            "provider-api",
            Some(&unknown_usage),
            source_policy,
            Some(manifest.as_ref()),
            &mut unknown_catalog,
        )
        .expect("an unknown plan must retain CommandCode's upstream allow behavior");
        assert_eq!(unknown_catalog.models.len(), 5);

        let purchased_credit = vulcan_ai_contract::AccountUsageWindow {
            id: String::from("purchased-credits"),
            display_name: String::from("Purchased credits"),
            bucket_name: None,
            used: Some(String::from("0")),
            remaining: Some(String::from("0.25")),
            total: Some(String::from("0.25")),
            remaining_percent: None,
            unit: String::from("USD"),
            starts_at: None,
            resets_at: None,
            unlimited: false,
            available: true,
        };
        let mut zero_credit = purchased_credit.clone();
        zero_credit.remaining = Some(String::from("0"));
        let zero_credit_usage = account_usage(
            "credential-commandcode-plan-account",
            Some("Go"),
            Some("individual-go"),
            vec![zero_credit],
        );
        let mut zero_credit_catalog = source.clone();
        apply_model_catalog_overlays_with_account_access(
            &configuration,
            "commandcode",
            "provider-api",
            Some(&zero_credit_usage),
            source_policy,
            Some(manifest.as_ref()),
            &mut zero_credit_catalog,
        )
        .expect("zero purchased credits must retain plan filtering");
        let zero_credit_models = zero_credit_catalog
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(zero_credit_models, go_models);

        let unlocked_usage = account_usage(
            "credential-commandcode-plan-account",
            Some("Go"),
            Some("individual-go"),
            vec![purchased_credit],
        );
        let mut unlocked_catalog = source;
        apply_model_catalog_overlays_with_account_access(
            &configuration,
            "commandcode",
            "provider-api",
            Some(&unlocked_usage),
            source_policy,
            Some(manifest.as_ref()),
            &mut unlocked_catalog,
        )
        .expect("positive purchased credits should unlock the declared catalog");
        assert_eq!(unlocked_catalog.models.len(), 5);
    }

    /// Verifies quota unlock comparisons accept exact decimal forms and reject malformed evidence.
    /// 验证额度解锁比较接受精确十进制形式并拒绝格式错误的证据。
    #[test]
    fn normalized_usage_decimal_positive_check_is_strict() {
        for value in ["1", "+0.25", ".5", "1e-3"] {
            assert!(normalized_usage_decimal_is_positive(value), "{value}");
        }
        for value in ["0", "0.0", "0e10", "-1", "1e", "abc1", "1.2.3"] {
            assert!(!normalized_usage_decimal_is_positive(value), "{value}");
        }
    }

    /// Verifies providers without an account-model-access declaration keep their own catalog logic.
    /// 验证未声明账号模型访问投影的供应商保持其自身目录逻辑。
    #[test]
    fn apikeyfun_catalog_does_not_inherit_account_plan_filtering() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let manifest = runtime
            .system_manifest("apikeyfun")
            .expect("APIKEY.FUN manifest must exist");
        let mut configuration = PersistedAiConfiguration::default();
        let mut apikeyfun_connection = connection("apikeyfun-group-account");
        apikeyfun_connection.provider_id = String::from("apikeyfun");
        apikeyfun_connection.service_entry_id = String::from("public-api");
        apikeyfun_connection.credential_profile_id = String::from("api-key");
        configuration
            .connection_graph
            .connections
            .insert(apikeyfun_connection.id.clone(), apikeyfun_connection);
        let mut catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "apikeyfun-group-account",
                1,
                r#"{"data":[{"id":"deepseek-v4-flash"}]}"#,
            )
            .expect("fixture catalog should parse"),
            &[OPENAI_RESPONSES_PROFILE_ID],
        );
        let unrelated_usage = account_usage(
            "credential-apikeyfun-group-account",
            Some("Go"),
            Some("individual-go"),
            Vec::new(),
        );
        apply_model_catalog_overlays_with_account_access(
            &configuration,
            "apikeyfun",
            "public-api",
            Some(&unrelated_usage),
            manifest
                .model_source_policy("public-api")
                .expect("APIKEY.FUN source policy must compile"),
            Some(manifest.as_ref()),
            &mut catalog,
        )
        .expect("APIKEY.FUN catalog projection should remain independent");
        assert_eq!(
            catalog
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["deepseek-v4-flash"]
        );
    }

    /// Verifies template-only media models ignore conversation publication and remain extension targets.
    /// 验证仅模板媒体模型忽略会话发布状态并保持为扩展目标。
    #[test]
    fn provider_tool_template_models_are_independent_from_conversation_access() {
        // runtime owns the bundled provider registry and one immutable publication generation.
        // runtime 持有内置供应商注册表与一个不可变发布代际。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // configuration owns one eligible Alibaba Model Studio credential.
        // configuration 持有一个合格的阿里云百炼凭据。
        let mut configuration = PersistedAiConfiguration::default();
        // alibaba_connection reuses the complete test credential shape with exact Alibaba identities.
        // alibaba_connection 复用完整测试凭据结构并改为精确阿里云身份。
        let mut alibaba_connection = connection("alibaba-media-account");
        alibaba_connection.provider_id = String::from("alibaba");
        alibaba_connection.service_entry_id = String::from("model-studio-cn");
        alibaba_connection.credential_profile_id = String::from("api-key");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                alibaba_connection,
            )
            .expect("Alibaba media connection should register");

        // directory resolves the authoritative bundled manifest fixture.
        // directory 解析权威内置 Manifest 测试夹具。
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        // text contains the reviewed Alibaba manifest bytes.
        // text 保存已审核的阿里云 Manifest 原文。
        let text = std::fs::read_to_string(directory.join("alibaba.json"))
            .expect("bundled Alibaba manifest must exist");
        // document disables inherited models while preserving every reviewed target and binding.
        // document 在保留全部已审核目标与绑定的同时关闭继承模型。
        let mut document: serde_json::Value =
            serde_json::from_str(&text).expect("Alibaba manifest should parse");
        document["model_defaults"]["default_enabled"] = serde_json::json!(false);
        // compiled freezes the disabled-by-default signed media catalog.
        // compiled 冻结默认关闭的签名媒体目录。
        let compiled = crate::provider_manifest::compile_manifest_text(
            &serde_json::to_string(&document).expect("Alibaba manifest should serialize"),
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("disabled-by-default Alibaba manifest should compile");
        // wan_target_id selects the exact generated video target used for direct-resolution rejection.
        // wan_target_id 选择用于直接解析拒绝验证的精确生成视频目标。
        let wan_target_id = compiled
            .tool_target_template_instances
            .values()
            .find(|instance| instance.model_id == "wan2.7-t2v")
            .map(|instance| instance.target_id.clone())
            .expect("Wan template target should compile");
        runtime
            .replace_system_manifest("alibaba", Arc::new(compiled))
            .expect("Alibaba replacement should stage");
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("test async runtime should initialize")
            .block_on(runtime.activate_staged_manifest_catalogs(&configuration, &[]))
            .expect("Alibaba template generation should publish");

        // declared proves disabled template identities remain present in the management inventory.
        // declared 证明关闭的模板身份仍存在于管理目录。
        let declared = runtime
            .provider_tool_template_models(&configuration)
            .expect("template model inventory should resolve");
        assert!(declared.iter().any(|model| {
            model.provider_id == "alibaba" && model.model_id == "qwen-image-2.0-pro"
        }));
        assert!(
            declared
                .iter()
                .any(|model| model.provider_id == "alibaba" && model.model_id == "wan2.7-t2v")
        );
        assert!(
            runtime
                .provider_model_is_supported(&configuration, "alibaba", "wan2.7-t2v")
                .expect("template model support should resolve")
        );
        assert!(
            !runtime
                .provider_conversation_model_is_supported(&configuration, "alibaba", "wan2.7-t2v",)
                .expect("conversation support should resolve")
        );

        // extension_targets proves the extension inventory remains selectable even though the
        // same provider/model conversation publication switches are disabled.
        // extension_targets 证明即使同一供应商/模型的会话发布开关已关闭，扩展清单仍可选择。
        let extension_targets = runtime
            .available_provider_tool_targets(&configuration)
            .expect("extension target inventory should resolve");
        assert!(
            extension_targets
                .iter()
                .any(|available| available.target.upstream_target_id == "qwen-image-2.0-pro")
        );
        assert!(
            extension_targets
                .iter()
                .any(|available| available.target.upstream_target_id == "wan2.7-t2v")
        );
        // extension_selection resolves the exact target that a validated extension target set owns.
        // extension_selection 解析已验证扩展目标集合持有的精确目标。
        let extension_selection = ProviderToolTargetSelection {
            connection_id: String::from("alibaba-media-account"),
            target_id: wan_target_id,
        };
        let resolved = runtime
            .resolve_provider_tool_target(
                &configuration,
                ProviderToolCapabilityKind::VideoGenerate,
                &extension_selection,
            )
            .expect("extension target should resolve independently from conversation access");
        assert_eq!(resolved.target.upstream_target_id, "wan2.7-t2v");
    }

    /// Verifies account model access remains authoritative for extension target availability.
    /// 验证账号模型访问仍对扩展目标可用性保持权威。
    #[test]
    fn provider_tool_targets_obey_account_model_access() {
        // runtime owns the bundled APIKEY.FUN manifest and its reviewed fixed targets.
        // runtime 持有内置 APIKEY.FUN Manifest 及其已审核固定目标。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        // configuration owns one exact APIKEY.FUN credential account.
        // configuration 持有一个精确的 APIKEY.FUN 凭据账号。
        let mut configuration = PersistedAiConfiguration::default();
        // apikeyfunConnection binds the exact public service and API-key credential profile.
        // apikeyfunConnection 绑定精确公共服务与 API Key 凭据配置档。
        let mut apikeyfun_connection = connection("apikeyfun-media-account");
        apikeyfun_connection.provider_id = String::from("apikeyfun");
        apikeyfun_connection.service_entry_id = String::from("public-api");
        apikeyfun_connection.credential_profile_id = String::from("api-key");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                apikeyfun_connection,
            )
            .expect("APIKEY.FUN media connection should register");
        // sourceCatalog reproduces the exact per-key `/models` evidence required by APIKEY.FUN.
        // sourceCatalog 复现 APIKEY.FUN 所需的精确逐密钥 `/models` 证据。
        let source_catalog = parse_openai_model_catalog(
            "apikeyfun-media-account",
            1,
            r#"{"data":[{"id":"grok-imagine-video"}]}"#,
        )
        .expect("APIKEY.FUN discovery fixture should parse");
        install_source_catalog(&runtime, source_catalog);
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("test async runtime should initialize")
            .block_on(runtime.activate_staged_manifest_catalogs(&configuration, &[]))
            .expect("APIKEY.FUN generation should publish");
        // initialTarget proves the account catalog initially exposes the fixed video model.
        // initialTarget 证明账号目录最初公开固定视频模型。
        let initial_target = runtime
            .available_provider_tool_targets(&configuration)
            .expect("initial extension target inventory should resolve")
            .into_iter()
            .find(|available| available.target.upstream_target_id == "grok-imagine-video")
            .expect("Grok Imagine video target should initially be available");
        // selection freezes the exact credential and extension target used for execution revalidation.
        // selection 冻结用于执行重新校验的精确凭据与扩展目标。
        let selection = ProviderToolTargetSelection {
            connection_id: String::from("apikeyfun-media-account"),
            target_id: initial_target.target.target_id,
        };
        // Account access remains authoritative even though conversation publication is independent.
        // 即使会话发布保持独立，账号访问仍保持权威。
        configuration.set_connection_model_enabled(
            "apikeyfun-media-account",
            "grok-imagine-video",
            false,
        );
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("account access update should rebuild effective catalogs");
        // filteredTargets proves the disabled account model disappears from extension inventory.
        // filteredTargets 证明被账号禁用的模型会从扩展清单消失。
        let filtered_targets = runtime
            .available_provider_tool_targets(&configuration)
            .expect("account-filtered target inventory should resolve");
        assert!(
            filtered_targets
                .iter()
                .all(|available| available.target.upstream_target_id != "grok-imagine-video")
        );
        // error proves direct execution cannot bypass the account-filtered extension inventory.
        // error 证明直接执行无法绕过账号权限过滤后的扩展清单。
        let error = runtime
            .resolve_provider_tool_target(
                &configuration,
                ProviderToolCapabilityKind::VideoGenerate,
                &selection,
            )
            .map(|_| ())
            .expect_err("account-disabled extension target must reject direct resolution");
        assert!(
            error.contains("unavailable for the current credential model catalog"),
            "{error}"
        );
    }

    /// Verifies manifest-owned tool targets are the sole authority for availability and hot replacement.
    /// 验证 Manifest 持有的工具目标是可用性与热替换的唯一权威。
    #[test]
    fn manifest_owned_tool_targets_drive_availability_and_resolution() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let mut tavily_connection = connection("tavily-account");
        tavily_connection.provider_id = String::from("tavily");
        tavily_connection.service_entry_id = String::from("public-api");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                tavily_connection,
            )
            .expect("connection should register");

        // The initial target inventory comes directly from the bundled Tavily manifest.
        // 初始目标清单直接来自内置 Tavily Manifest。
        let initial_manifest = runtime
            .available_provider_tool_targets(&configuration)
            .expect("bundled manifest targets must resolve");
        assert_eq!(
            initial_manifest.len(),
            2,
            "the bundled tavily manifest exposes two targets"
        );

        // A newly compiled manifest atomically replaces the same authoritative target set.
        // 新编译的 Manifest 原子替换同一份权威目标集合。
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("tavily.json"))
            .expect("bundled tavily manifest must exist");
        let mut document: serde_json::Value =
            serde_json::from_str(&text).expect("manifest should parse");
        let search_schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "query": {"type": "string", "minLength": 1},
                "max_results": {"type": "integer", "minimum": 1, "maximum": 20, "default": 5}
            },
            "required": ["query"]
        });
        document["tool_targets"] = serde_json::json!([
            {
                "target_id": "public-api.search",
                "description": "Tavily structured web search.",
                "service_entry_id": "public-api",
                "credential_profile_ids": ["api-key"],
                "upstream_target_id": "search",
                "display_name": "Tavily Search",
                "tool_id": "vulcan_web_search",
                "capability": "web_search",
                "input_schema": search_schema,
                "output": {
                    "kind": "search",
                    "source_path": "/results",
                    "normalized_result_path": "search",
                    "resource_kind": null,
                    "mime_types": []
                },
                "execution": {
                    "endpoint_path": "/search",
                    "authentication": "bearer_authorization",
                    "adapter": "generic_json",
                    "execution_kind": "http_json_sync"
                }
            },
            {
                "target_id": "public-api.extract",
                "description": "Tavily structured web extraction.",
                "service_entry_id": "public-api",
                "credential_profile_ids": ["api-key"],
                "upstream_target_id": "extract",
                "display_name": "Tavily Extract",
                "tool_id": "vulcan_web_extract",
                "capability": "web_extract",
                "input_schema": serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "urls": {"type": "array", "minItems": 1, "maxItems": 20, "uniqueItems": true, "items": {"type": "string"}}
                    },
                    "required": ["urls"]
                }),
                "output": {
                    "kind": "extract",
                    "source_path": "/results",
                    "normalized_result_path": "extract",
                    "resource_kind": null,
                    "mime_types": []
                },
                "execution": {
                    "endpoint_path": "/extract",
                    "authentication": "bearer_authorization",
                    "adapter": "generic_json",
                    "execution_kind": "http_json_sync"
                }
            }
        ]);
        let compiled = crate::provider_manifest::compile_manifest_text(
            &serde_json::to_string_pretty(&document).expect("manifest should serialize"),
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("manifest with tool targets must compile");
        runtime
            .replace_system_manifest("tavily", Arc::new(compiled))
            .expect("hot replace should succeed");
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("test async runtime should initialize")
            .block_on(runtime.activate_staged_manifest_catalogs(&configuration, &[]))
            .expect("tavily target generation should publish");

        let manifest_driven = runtime
            .available_provider_tool_targets(&configuration)
            .expect("manifest targets must resolve");
        assert_eq!(
            manifest_driven.len(),
            2,
            "the replacement manifest must publish both tavily targets"
        );
        for available in &manifest_driven {
            assert_eq!(
                available.target.provider_id, "tavily",
                "manifest targets keep the compiled provider id"
            );
            assert!(
                matches!(
                    available.target.contract.tool_id.as_str(),
                    "vulcan_web_search" | "vulcan_web_extract"
                ),
                "manifest targets keep the declared tool id"
            );
        }
    }

    /// Verifies recovered async tasks resolve the complete route from the dispatch-frozen snapshot
    /// and never read a newer manifest (plan §R9.6).
    /// 验证恢复的异步任务从分派时冻结的快照解析完整路由，绝不读取更新的 Manifest
    /// （方案 §R9.6）。
    #[tokio::test]
    async fn recovered_async_task_uses_frozen_target_snapshot() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let mut tavily_connection = connection("tavily-account");
        tavily_connection.provider_id = String::from("tavily");
        tavily_connection.service_entry_id = String::from("public-api");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                tavily_connection,
            )
            .expect("connection should register");

        // Dispatch freezes a manifest-owned target snapshot with revision one.
        // 分派时冻结一个 Manifest 持有的目标快照，修订号为一。
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("tavily.json"))
            .expect("bundled tavily manifest must exist");
        let mut document: serde_json::Value =
            serde_json::from_str(&text).expect("manifest should parse");
        document["tool_targets"] = serde_json::json!([
            {
                "target_id": "public-api.search",
                "description": "Tavily structured web search.",
                "service_entry_id": "public-api",
                "credential_profile_ids": ["api-key"],
                "upstream_target_id": "search",
                "display_name": "Tavily Search",
                "tool_id": "vulcan_web_search",
                "capability": "web_search",
                "input_schema": serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "query": {"type": "string", "minLength": 1}
                    },
                    "required": ["query"]
                }),
                "output": {
                    "kind": "search",
                    "source_path": "/results",
                    "normalized_result_path": "search",
                    "resource_kind": null,
                    "mime_types": []
                },
                "execution": {
                    "endpoint_path": "/search",
                    "authentication": "bearer_authorization",
                    "adapter": "generic_json",
                    "execution_kind": "http_json_sync"
                }
            }
        ]);
        document["operations"]
            .as_array_mut()
            .expect("tavily operations should be an array")
            .retain(|operation| {
                operation["tool_target_refs"]
                    .as_array()
                    .is_some_and(|references| {
                        references
                            .iter()
                            .any(|reference| reference == "public-api.search")
                    })
            });
        let compiled = crate::provider_manifest::compile_manifest_text(
            &serde_json::to_string_pretty(&document).expect("manifest should serialize"),
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("manifest with tool targets must compile");
        runtime
            .replace_system_manifest("tavily", Arc::new(compiled))
            .expect("hot replace should succeed");
        runtime
            .activate_staged_manifest_catalogs(&configuration, &[])
            .await
            .expect("first frozen-target generation should publish");

        let frozen = runtime
            .resolved_provider_tool_targets("tavily")
            .into_iter()
            .find(|target| target.target_id == "public-api.search")
            .expect("frozen target must resolve at dispatch");
        let mut frozen_service_entry = runtime
            .providers()
            .expect("dispatch registry should resolve")
            .provider("tavily")
            .expect("dispatch provider should exist")
            .service_entries
            .iter()
            .find(|entry| entry.id == "public-api")
            .cloned()
            .expect("dispatch service entry should exist");
        // The frozen service entry records the fallback origin that was effective at dispatch.
        // 冻结服务入口记录分派时生效的回退源地址。
        frozen_service_entry.base_url = String::from("https://dispatch-frozen.example");
        let frozen_display_name = frozen.display_name.clone();

        // A newer manifest changes the same target's display name after dispatch.
        // 分派后更新的 Manifest 修改了同一目标的显示名称。
        document["revision"] = serde_json::json!(2);
        document["tool_targets"][0]["display_name"] = serde_json::json!("Tavily Search Renamed");
        let replaced = crate::provider_manifest::compile_manifest_text(
            &serde_json::to_string_pretty(&document).expect("manifest should serialize"),
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("replacement manifest must compile");
        runtime
            .replace_system_manifest("tavily", Arc::new(replaced))
            .expect("hot replace should succeed");
        runtime
            .activate_staged_manifest_catalogs(&configuration, &[])
            .await
            .expect("replacement frozen-target generation should publish");

        // Recovery resolves the exact frozen snapshot captured at dispatch instead of the
        // newer manifest's renamed target.
        // 恢复时解析分派时捕获的精确冻结快照，而不是新清单中已重命名的目标。
        let mut record = ProviderAsyncTaskRecord {
            execution_id: String::from("execution-frozen"),
            owner_session_id: Some(String::from("session-one")),
            route: ProviderAsyncTaskRoute {
                provider_id: String::from("tavily"),
                service_entry_id: String::from("public-api"),
                connection_id: String::from("tavily-account"),
                selection: ProviderToolTargetSelection {
                    connection_id: String::from("tavily-account"),
                    target_id: String::from("public-api.search"),
                },
                adapter: frozen.execution.adapter,
                manifest_revision: 1,
                frozen_target: Some(frozen.clone()),
                frozen_service_entry: Some(frozen_service_entry),
                operation_id: Some(String::from("tavily.search")),
                frozen_operation: None,
                frozen_request_profiles: Vec::new(),
                frozen_response_profiles: Vec::new(),
            },
            provider_task_id: Some(String::from("task-one")),
            recovery: crate::provider_task::ProviderAsyncTaskRecoveryContext::None,
            phase: crate::provider_task::ProviderAsyncTaskPhase::Queued,
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
            deadline_unix_ms: u64::MAX,
            poll_interval_ms: 15_000,
            consecutive_observation_failures: 0,
            cancellation_supported: false,
            error: None,
            result: None,
        };
        // The same connection changes origin after dispatch; recovery must not redirect the frozen task.
        // 同一连接在分派后变更源地址；恢复不得重定向已冻结任务。
        configuration
            .connection_graph
            .connections
            .get_mut("tavily-account")
            .expect("retained connection must exist")
            .service_origin_override = Some(String::from("https://current-origin.example"));
        let resolved = runtime
            .resolve_recovered_provider_task(&configuration, &record)
            .expect("frozen recovery must resolve");
        assert_eq!(
            resolved.target.display_name, frozen_display_name,
            "recovery must use the dispatch-frozen target snapshot"
        );
        assert_eq!(
            resolved.target.target_id, "public-api.search",
            "recovery must keep the frozen target identity"
        );
        assert_eq!(
            resolved.entry.base_url, "https://dispatch-frozen.example",
            "recovery must keep the dispatch-frozen service-entry origin"
        );

        // A nonzero revision without its frozen target must not read a hot-replaced target.
        // 缺少冻结目标的非零修订不得读取已热替换目标。
        record.route.frozen_target = None;
        let missing_snapshot_error =
            match runtime.resolve_recovered_provider_task(&configuration, &record) {
                Ok(_) => panic!("manifest-backed recovery without frozen target must fail closed"),
                Err(error) => error,
            };
        assert!(
            missing_snapshot_error.contains("dispatch-frozen target"),
            "error: {missing_snapshot_error}"
        );

        // A persisted revision-zero record predating frozen manifest snapshots resolves through
        // the current authoritative manifest target for backward compatibility.
        // 早于冻结 Manifest 快照的持久化零修订记录会通过当前权威 Manifest 目标解析，以保持向后兼容。
        record.route.manifest_revision = 0;
        record.route.frozen_service_entry = None;
        record.execution_id = String::from("execution-legacy");
        let resolved = runtime
            .resolve_recovered_provider_task(&configuration, &record)
            .expect("legacy recovery must resolve");
        assert_eq!(
            resolved.target.display_name, "Tavily Search Renamed",
            "legacy recovery reads the current catalog target"
        );
        assert_eq!(
            resolved.target.execution.endpoint_base_url.as_deref(),
            Some("https://current-origin.example"),
            "legacy recovery without endpoint evidence uses the current connection origin"
        );
    }

    /// R0/P1-03: an unactivated configuration mutation is invisible to selection.
    /// R0/P1-03：未激活的配置内存变更对选择不可见。
    ///
    /// Business invariant (plan §6.2/§6.3): the effective catalog is generated at an explicit
    /// activation boundary (save/manifest activation/manual refresh). Mutating the in-memory
    /// configuration object after activation must not change existing or new selections until
    /// the next activation; selection must read the activated effective catalog instead of
    /// rebuilding overlays at the request boundary.
    /// 业务不变量（方案 §6.2/§6.3）：有效目录在显式激活边界（保存/清单激活/手动刷新）生成。
    /// 激活后修改内存配置对象不得改变既有或新的选择，直到下一次激活；选择必须读取
    /// 已激活有效目录，而不是在请求边界重建覆盖。
    #[test]
    fn unactivated_configuration_mutation_is_invisible_to_selection() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = multi_protocol_custom_definition(
            "custom:frozen",
            &[
                crate::CustomProviderProtocol::OpenAiChat,
                crate::CustomProviderProtocol::AnthropicMessages,
                crate::CustomProviderProtocol::OpenAiResponses,
            ],
        );
        install_current_custom_provider(&mut configuration, provider);
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("frozen-account", "custom:frozen"),
            )
            .expect("frozen connection should register");
        let catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "frozen-account",
                1,
                r#"{"data":[{"id":"frozen/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
            )
            .expect("frozen catalog should parse"),
            &[
                OPENAI_CHAT_PROFILE_ID,
                ANTHROPIC_MESSAGES_PROFILE_ID,
                OPENAI_RESPONSES_PROFILE_ID,
            ],
        );
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("frozen catalog should install");
        configuration.set_model_enabled("custom:frozen", "frozen/model", true);
        // The activation boundary freezes the auto binding (highest fixed priority).
        // 激活边界冻结 Auto 绑定（最高固定优先级）。
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("overlays should reapply");
        let binding = runtime
            .resolve_connection_model_selection(
                &configuration,
                "frozen-account",
                "custom:frozen",
                "frozen/model",
            )
            .expect("frozen model must resolve")
            .protocol_binding;
        assert_eq!(binding.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID);

        // Mutating the in-memory configuration without any activation must be invisible to
        // the next selection; only an explicit activation may rebind.
        // 未经过任何激活的内存配置变更对下一次选择必须不可见；只有显式激活才可重新绑定。
        configuration.custom_model_protocol_preferences.insert(
            PersistedAiConfiguration::provider_model_key("custom:frozen", "frozen/model"),
            crate::CustomModelProtocolPreference::Explicit {
                protocol: crate::CustomProviderProtocol::AnthropicMessages,
            },
        );
        let still_frozen = runtime
            .resolve_connection_model_selection(
                &configuration,
                "frozen-account",
                "custom:frozen",
                "frozen/model",
            )
            .expect("frozen model must resolve")
            .protocol_binding;
        assert_eq!(
            still_frozen.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
            "an unactivated preference mutation must not rebind the model"
        );
    }

    /// R5/§13.3: request execution never resolves a missing custom binding from live settings.
    /// R5/§13.3：请求执行绝不从实时设置解析缺失的自定义绑定。
    ///
    /// A malformed activated catalog intentionally lacks the activation-time binding. Even though
    /// the current provider configuration can produce an Auto choice, selection must fail closed.
    /// 一份异常已激活目录刻意缺少激活时绑定。即使当前供应商配置可产生 Auto 选择，模型选择也
    /// 必须失败关闭。
    #[test]
    fn missing_custom_catalog_binding_never_resolves_live_at_request_time() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let provider = multi_protocol_custom_definition(
            "custom:missing-binding",
            &[
                crate::CustomProviderProtocol::OpenAiChat,
                crate::CustomProviderProtocol::OpenAiResponses,
            ],
        );
        install_current_custom_provider(&mut configuration, provider);
        let effective_providers = runtime
            .effective_providers(&configuration)
            .expect("effective providers should resolve");
        configuration
            .connection_graph
            .add_connection(
                &effective_providers,
                custom_connection("missing-binding-account", "custom:missing-binding"),
            )
            .expect("custom connection should register");
        let catalog = parse_openai_model_catalog(
            "missing-binding-account",
            1,
            r#"{"data":[{"id":"missing/binding","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("raw catalog should parse");
        configuration.set_model_enabled("custom:missing-binding", "missing/binding", true);
        // candidateGeneration injects one malformed activated catalog while preserving the
        // immutable generation boundary used by request selection.
        // candidateGeneration 注入一份异常已激活目录，同时保留请求选择使用的不可变代际边界。
        let mut candidate_generation = runtime
            .selection_catalog_snapshot()
            .expect("published generation should load")
            .as_ref()
            .clone();
        candidate_generation.configuration = Arc::new(configuration.clone());
        candidate_generation
            .activated_effective_catalogs
            .replace(catalog);
        *runtime
            .resolved_provider_generation
            .write()
            .expect("published generation lock should remain healthy") =
            Arc::new(candidate_generation);
        let error = runtime
            .resolve_connection_model_selection(
                &configuration,
                "missing-binding-account",
                "custom:missing-binding",
                "missing/binding",
            )
            .map(|_| ())
            .expect_err("request-time live binding resolution must remain forbidden");
        assert!(
            error.contains("no protocol binding frozen"),
            "unexpected missing-binding error: {error}"
        );
    }

    /// R0/P1-01: a system binding without its variant stays visible but cannot execute.
    /// R0/P1-01：缺少协议变体的系统绑定保持可见但不能执行。
    ///
    /// Business invariant (plan §6.5, R5): any system execution must prove that its bound
    /// protocol, route, variant, and profile agree; an empty `protocol_variants` map is never
    /// a compatibility bypass for a system-manifest binding.
    /// 业务不变量（方案 §6.5、阶段 R5）：任何系统执行都必须证明其绑定协议、路由、
    /// 变体与 Profile 一致；空 `protocol_variants` 映射绝不是系统清单绑定的兼容绕过。
    #[test]
    fn system_manifest_binding_without_variant_is_model_level_blocked() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let mut deepseek_connection = connection("one");
        deepseek_connection.provider_id = String::from("deepseek");
        deepseek_connection.service_entry_id = String::from("official-api");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                deepseek_connection,
            )
            .expect("connection should register");
        let catalog = parse_openai_model_catalog(
            "one",
            1,
            r#"{"data":[{"id":"deepseek-v4-pro","context_window":131072,"max_output_tokens":8192,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("openai catalog should parse");
        install_source_catalog(&runtime, catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("catalog should install");
        configuration.set_model_enabled("deepseek", "deepseek-v4-pro", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("one incompatible discovered model must not block the provider generation");
        // brokenCatalog starts from a valid published DeepSeek catalog before removing the exact
        // bound variant to exercise the request-boundary fail-closed guard directly.
        // brokenCatalog 从有效的已发布 DeepSeek 目录开始，再移除精确绑定变体，以直接验证
        // 请求边界的失败关闭守卫。
        let published = runtime
            .catalog_snapshot()
            .expect("catalog should remain readable");
        let mut broken_catalog = published["one"].clone();
        // model is the exact system-bound identity whose selected variant is corrupted by the fixture.
        // model 是夹具刻意破坏所选变体的精确系统绑定身份。
        let model = broken_catalog
            .models
            .iter_mut()
            .find(|model| model.id == "deepseek-v4-pro")
            .expect("DeepSeek V4 Pro should be published");
        // boundProtocol identifies the only variant whose absence must block execution.
        // boundProtocol 标识缺失后必须阻止执行的唯一绑定变体。
        let bound_protocol = model
            .resolved_protocol_binding
            .as_ref()
            .expect("system model must have a frozen binding")
            .protocol_profile_id
            .clone();
        assert!(model.protocol_variants.remove(&bound_protocol).is_some());
        model.capabilities = ModelCapabilities::default();
        // candidateGeneration publishes the intentionally broken catalog atomically for selection.
        // candidateGeneration 为请求选择原子发布刻意破坏的目录。
        let mut candidate_generation = runtime
            .selection_catalog_snapshot()
            .expect("published generation should load")
            .as_ref()
            .clone();
        candidate_generation.configuration = Arc::new(configuration.clone());
        candidate_generation
            .activated_effective_catalogs
            .replace(broken_catalog);
        *runtime
            .resolved_provider_generation
            .write()
            .expect("published generation lock should remain healthy") =
            Arc::new(candidate_generation);

        let error = runtime
            .resolve_connection_model_selection(
                &configuration,
                "one",
                "deepseek",
                "deepseek-v4-pro",
            )
            .map(|_| ())
            .expect_err("a bound system model without its variant must fail execution");
        assert!(
            error.contains("variant") || error.contains("capability"),
            "unexpected fail-closed error: {error}"
        );
    }

    /// R3/§R3.8: every replacement swaps the whole catalog generation with a new content hash.
    /// R3/§R3.8：每次替换整体交换目录代数并产生新内容哈希。
    ///
    /// Business invariant (plan §6.2/§R3.5): the catalog is published whole; the generation
    /// explains the content because the digest covers every manifest identity, revision, and
    /// source digest. A generation bump without a content change is impossible.
    /// 业务不变量（方案 §6.2/§R3.5）：目录整体发布；摘要覆盖每份清单身份、修订号与来源
    /// 摘要，使代数能解释内容。不存在只递增代数却无内容变化的情况。
    #[tokio::test]
    async fn hot_replaced_catalog_swaps_whole_generation_with_content_hash() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let generation_before = runtime.catalog_generation();
        let hash_before = runtime.catalog_content_hash();
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled deepseek manifest must exist");
        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("manifest should parse");
        value["revision"] = serde_json::json!(value["revision"].as_u64().unwrap_or(1) + 1);
        // replacementDisplayName proves registry reads follow the published generation.
        // replacementDisplayName 证明注册表读取跟随已发布代际。
        let replacement_display_name = "DeepSeek Hot-Replaced";
        value["display"]["name"] = serde_json::json!(replacement_display_name);
        let replacement_text = serde_json::to_string(&value).expect("manifest should serialize");
        let replaced = crate::provider_manifest::compile_manifest_text(
            &replacement_text,
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("replacement manifest must compile");
        runtime
            .replace_system_manifest("deepseek", Arc::new(replaced))
            .expect("hot replace should succeed");
        assert_eq!(
            runtime.catalog_generation(),
            generation_before,
            "a staged catalog must not change the externally visible generation"
        );
        assert_eq!(
            runtime.catalog_content_hash(),
            hash_before,
            "a staged catalog must not change the externally visible digest"
        );
        runtime
            .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
            .await
            .expect("the complete generation should publish");
        assert_eq!(
            runtime.catalog_generation(),
            generation_before + 1,
            "the explicit activation boundary publishes the next whole generation"
        );
        let hash_after = runtime.catalog_content_hash();
        assert_ne!(
            hash_after, hash_before,
            "a content change must produce a new content digest"
        );
        assert_eq!(
            runtime
                .providers()
                .expect("published provider registry should resolve")
                .provider("deepseek")
                .expect("DeepSeek must remain registered")
                .display_name
                .as_str(),
            replacement_display_name,
            "registry snapshots must never retain the initial bundled descriptor"
        );
        // After activation the discovery client and published runtime converge on one generation.
        // 激活后发现客户端与已发布运行时收敛到同一代数。
        assert_eq!(
            runtime.metadata_client().catalog_generation(),
            runtime.catalog_generation(),
            "runtime and discovery client must share one catalog generation"
        );
    }

    /// Verifies one missing bundled system provider is excluded without suppressing healthy peers.
    /// 验证一个缺失的内置系统供应商会被排除，且不会压制健康同级供应商。
    #[test]
    fn runtime_settings_projection_isolates_one_failed_system_provider() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let current = runtime
            .selection_catalog_snapshot()
            .expect("published generation should resolve");
        let mut manifests = current.manifests.manifests.clone();
        manifests.remove("deepseek");
        let mut bundled_failures = current.manifests.bundled_failures.clone();
        bundled_failures.insert(
            String::from("deepseek"),
            crate::provider_manifest::BundledProviderManifestFailure {
                failure_kind: String::from("bundled_baseline_invalid"),
                message: String::from("isolated test baseline"),
            },
        );
        let isolated_catalog = Arc::new(
            crate::provider_manifest::ResolvedProviderCatalog::build_with_bundled_failures(
                manifests,
                bundled_failures,
                current.manifests.generation + 1,
            )
            .expect("provider-local failed catalog should build"),
        );
        let mut isolated_generation = current.as_ref().clone();
        isolated_generation.manifests = isolated_catalog;
        *runtime
            .resolved_provider_generation
            .write()
            .expect("published generation lock should remain healthy") =
            Arc::new(isolated_generation);

        let mut configuration = PersistedAiConfiguration::default();
        let healthy_connection = connection("openrouter-account");
        let mut isolated_connection = connection("deepseek-account");
        isolated_connection.provider_id = String::from("deepseek");
        isolated_connection.service_entry_id = String::from("official-api");
        configuration.connection_graph.connections.extend([
            (healthy_connection.id.clone(), healthy_connection),
            (isolated_connection.id.clone(), isolated_connection),
        ]);
        // configuredGeneration installs the intentional isolated-provider fixture as one image.
        // configuredGeneration 把刻意构造的隔离供应商夹具作为一份完整镜像安装。
        let mut configured_generation = runtime
            .selection_catalog_snapshot()
            .expect("isolated generation should remain published")
            .as_ref()
            .clone();
        configured_generation.configuration = Arc::new(configuration.clone());
        *runtime
            .resolved_provider_generation
            .write()
            .expect("published generation lock should remain healthy") =
            Arc::new(configured_generation);

        let snapshot = runtime
            .settings_catalog_snapshot(&configuration)
            .expect("healthy provider settings must remain available");

        assert!(
            snapshot
                .runtime_configuration
                .connection_graph
                .connections
                .contains_key("openrouter-account")
        );
        assert!(
            !snapshot
                .runtime_configuration
                .connection_graph
                .connections
                .contains_key("deepseek-account")
        );
        assert!(snapshot.providers.provider("openrouter").is_some());
        assert!(snapshot.providers.provider("deepseek").is_none());
        assert!(
            runtime
                .provider_manifest_runtime_snapshot("openrouter", &configuration)
                .expect("healthy provider diagnostics must ignore the isolated peer")
                .is_some()
        );
    }

    /// Verifies APIKEY.FUN fails closed when its authoritative bundled Manifest is unavailable.
    /// 验证 APIKEY.FUN 的权威内置 Manifest 不可用时会失败封闭。
    #[test]
    fn runtime_settings_projection_isolates_invalid_apikeyfun_manifest() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let current = runtime
            .selection_catalog_snapshot()
            .expect("published generation should resolve");
        let mut manifests = current.manifests.manifests.clone();
        manifests.remove("apikeyfun");
        let mut bundled_failures = current.manifests.bundled_failures.clone();
        bundled_failures.insert(
            String::from("apikeyfun"),
            crate::provider_manifest::BundledProviderManifestFailure {
                failure_kind: String::from("bundled_baseline_invalid"),
                message: String::from("isolated authoritative baseline"),
            },
        );
        let isolated_catalog = Arc::new(
            crate::provider_manifest::ResolvedProviderCatalog::build_with_bundled_failures(
                manifests,
                bundled_failures,
                current.manifests.generation + 1,
            )
            .expect("provider-local failed catalog should build"),
        );
        let mut isolated_generation = current.as_ref().clone();
        isolated_generation.manifests = isolated_catalog;
        *runtime
            .resolved_provider_generation
            .write()
            .expect("published generation lock should remain healthy") =
            Arc::new(isolated_generation);

        let mut configuration = PersistedAiConfiguration::default();
        // apikeyfunConnection represents persisted account state that must be removed from the
        // executable projection when its signed provider authority is unavailable.
        // apikeyfunConnection 表示持久化账号状态；其已签名供应商权威不可用时，必须从可执行投影移除。
        let mut apikeyfun_connection = connection("apikeyfun-account");
        apikeyfun_connection.provider_id = String::from("apikeyfun");
        apikeyfun_connection.service_entry_id = String::from("public-api");
        configuration
            .connection_graph
            .connections
            .insert(apikeyfun_connection.id.clone(), apikeyfun_connection);
        // configuredGeneration installs the intentional isolated-provider fixture as one image.
        // configuredGeneration 把刻意构造的隔离供应商夹具作为一份完整镜像安装。
        let mut configured_generation = runtime
            .selection_catalog_snapshot()
            .expect("isolated generation should remain published")
            .as_ref()
            .clone();
        configured_generation.configuration = Arc::new(configuration.clone());
        *runtime
            .resolved_provider_generation
            .write()
            .expect("published generation lock should remain healthy") =
            Arc::new(configured_generation);

        let snapshot = runtime
            .settings_catalog_snapshot(&configuration)
            .expect("healthy provider settings should remain available");

        assert!(
            !snapshot
                .runtime_configuration
                .connection_graph
                .connections
                .contains_key("apikeyfun-account")
        );
        assert!(snapshot.providers.provider("apikeyfun").is_none());
    }

    /// R3/§R3.2: identical content rebuilds produce the identical content digest.
    /// R3/§R3.2：相同内容的重新构建产生相同内容摘要。
    #[test]
    fn catalog_build_is_deterministic_for_identical_content() {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled deepseek manifest must exist");
        let resolved = crate::provider_manifest::compile_manifest_text(
            &text,
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("deepseek manifest must compile");
        let mut manifests = BTreeMap::new();
        manifests.insert(String::from("deepseek"), Arc::new(resolved));
        let first = crate::provider_manifest::ResolvedProviderCatalog::build(manifests.clone(), 1)
            .expect("catalog must build");
        let second = crate::provider_manifest::ResolvedProviderCatalog::build(manifests, 2)
            .expect("catalog must build");
        assert_eq!(
            first.content_sha256, second.content_sha256,
            "identical content must share one digest across generations"
        );
        assert_ne!(first.generation, second.generation);
    }

    /// Verifies installing identical Manifest content succeeds without creating a new generation.
    /// 验证安装相同 Manifest 内容时成功返回且不创建新代次。
    #[test]
    fn identical_system_manifest_replacement_is_a_generation_noop() {
        // runtime owns the current bundled Manifest catalog and its process-local generation.
        // runtime 持有当前内置 Manifest 目录及其进程内代次。
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let before = runtime
            .bundled_manifests
            .read()
            .expect("manifest catalog lock should remain healthy")
            .clone();
        let deepseek = before
            .manifest_arc("deepseek")
            .expect("bundled DeepSeek manifest should exist");

        runtime
            .replace_system_manifest("deepseek", deepseek)
            .expect("identical replacement should be a successful no-op");

        let after = runtime
            .bundled_manifests
            .read()
            .expect("manifest catalog lock should remain healthy")
            .clone();
        assert_eq!(after.generation, before.generation);
        assert_eq!(after.content_sha256, before.content_sha256);
    }

    /// Verifies configuration identities and effective catalogs always share one Arc publication.
    /// 验证配置身份与有效目录始终共享同一个 Arc 发布边界。
    #[test]
    fn concurrent_configuration_activation_never_mixes_catalog_generations() {
        // runtime begins with two fully activated credential catalogs.
        // runtime 初始持有两个已完整激活的凭据目录。
        let (runtime, mut old_configuration) = two_account_runtime();
        // oldActive identifies the first complete publication state.
        // oldActive 标识第一份完整发布状态。
        old_configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("one"),
        });
        runtime
            .replace_configuration(old_configuration.clone())
            .expect("old configuration generation should publish");
        // newConfiguration removes account one and selects account two in one candidate image.
        // newConfiguration 在同一候选镜像中移除账号 one 并选择账号 two。
        let mut new_configuration = old_configuration.clone();
        new_configuration.connection_graph.connections.remove("one");
        new_configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("openai/gpt-5.6-sol"),
            connection_id: String::from("two"),
        });
        // runtime is shared by concurrent readers and the alternating writer.
        // runtime 由并发读取方与交替写入方共享。
        let runtime = Arc::new(runtime);
        // readers repeatedly retain one immutable generation and validate both halves together.
        // readers 重复保留一个不可变代际，并共同校验其配置与目录两半。
        let mut readers = Vec::new();
        for _reader_index in 0..8 {
            let runtime = Arc::clone(&runtime);
            readers.push(std::thread::spawn(move || {
                for _sample_index in 0..400 {
                    // generation is the sole atomic observation made by this sample.
                    // generation 是本次采样进行的唯一原子观测。
                    let generation = runtime
                        .selection_catalog_snapshot()
                        .expect("provider generation should remain readable");
                    // connectionIds comes from the configuration frozen in the retained Arc.
                    // connectionIds 来自保留 Arc 中冻结的配置。
                    let connection_ids = generation
                        .configuration
                        .connection_graph
                        .connections
                        .keys()
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    // catalogIds comes from the effective catalogs frozen in the same Arc.
                    // catalogIds 来自同一 Arc 中冻结的有效目录。
                    let catalog_ids = generation
                        .activated_effective_catalogs
                        .snapshot()
                        .keys()
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    assert_eq!(connection_ids, catalog_ids);
                    // activeConnection must belong to the same exact connection set.
                    // activeConnection 必须属于同一精确连接集合。
                    let active_connection = generation
                        .configuration
                        .active
                        .as_ref()
                        .expect("each test generation should own an active selection")
                        .connection_id
                        .as_str();
                    assert!(connection_ids.contains(active_connection));
                }
            }));
        }
        // writer alternates both complete configurations while readers sample.
        // writer 在读取方采样期间交替发布两份完整配置。
        for activation_index in 0..200 {
            let candidate = if activation_index % 2 == 0 {
                &new_configuration
            } else {
                &old_configuration
            };
            runtime
                .replace_configuration(candidate.clone())
                .expect("complete configuration generation should publish atomically");
        }
        for reader in readers {
            reader.join().expect("configuration reader should finish");
        }
    }

    /// R3/§R3.4: concurrent selection observes one whole old or one whole new generation.
    /// R3/§R3.4：并发选择观察到完整旧代或完整新代。
    ///
    /// Business invariant (plan §R3.4): every selection snapshot pairs the catalog generation
    /// with the manifest it read; a reader must never observe the new manifest under the old
    /// generation or vice versa.
    /// 业务不变量（方案 §R3.4）：每个选择快照把目录代数与它所读清单配对；读取方绝不
    /// 会在旧代数下观察到新清单，反之亦然。
    #[test]
    fn concurrent_selection_observes_whole_generations() {
        let runtime = std::sync::Arc::new(PublicAiRuntime::unavailable().expect("runtime"));
        let mut configuration = PersistedAiConfiguration::default();
        let mut deepseek_connection = connection("generation-account");
        deepseek_connection.provider_id = String::from("deepseek");
        deepseek_connection.service_entry_id = String::from("official-api");
        configuration
            .connection_graph
            .add_connection(
                &runtime
                    .providers()
                    .expect("provider registry should resolve"),
                deepseek_connection,
            )
            .expect("deepseek connection should register");
        let catalog = with_protocol_variants(
            parse_openai_model_catalog(
                "generation-account",
                1,
                r#"{"data":[{"id":"deepseek-v4-pro","context_window":131072,"max_output_tokens":8192,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
            )
            .expect("deepseek catalog should parse"),
            &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID],
        );
        install_source_catalog(runtime.as_ref(), catalog.clone());
        runtime
            .replace_catalog(catalog)
            .expect("deepseek catalog should install");
        configuration.set_model_enabled("deepseek", "deepseek-v4-pro", true);
        runtime
            .reapply_model_catalog_overlays(&configuration)
            .expect("initial selection generation should publish");
        let old_revision = runtime
            .system_manifest("deepseek")
            .expect("DeepSeek manifest must exist")
            .revision;
        let new_revision = old_revision.saturating_add(1);
        let configuration = Arc::new(configuration);
        let mut handles = Vec::new();
        for _ in 0..8 {
            let runtime = std::sync::Arc::clone(&runtime);
            let configuration = Arc::clone(&configuration);
            handles.push(std::thread::spawn(move || {
                for _ in 0..200 {
                    let selection = runtime
                        .resolve_connection_model_selection(
                            configuration.as_ref(),
                            "generation-account",
                            "deepseek",
                            "deepseek-v4-pro",
                        )
                        .expect("real selection API must resolve");
                    let binding = selection.protocol_binding;
                    let expected_protocol = match binding.manifest_revision {
                        revision if revision == old_revision => OPENAI_RESPONSES_PROFILE_ID,
                        revision if revision == new_revision => OPENAI_CHAT_PROFILE_ID,
                        revision => panic!("unexpected published revision {revision}"),
                    };
                    assert_eq!(binding.protocol_profile_id, expected_protocol);
                    assert_eq!(
                        selection.route.protocol_route.protocol_profile_id, expected_protocol,
                        "one real selection must not mix manifest and route generations"
                    );
                }
            }));
        }
        // The writer stages a new manifest and then publishes it with the existing model
        // catalog at an explicit activation boundary while real selection calls sample.
        // 写入方暂存新 Manifest，并在显式激活边界把它与现有模型目录共同发布，同时真实选择
        // 调用持续采样。
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled deepseek manifest must exist");
        let mut value: crate::provider_manifest::ProviderManifestV1 =
            serde_json::from_str(&text).expect("manifest");
        value.revision = new_revision;
        for rule in &mut value.model_rules {
            if rule.matcher.value == "deepseek-v4-pro" {
                rule.selected_protocol = Some(String::from("openai-chat-completions.v1"));
            }
        }
        let replaced = crate::provider_manifest::compile_manifest_text(
            &serde_json::to_string(&value).expect("serialize"),
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("replacement must compile");
        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("test async runtime should initialize");
        for _ in 0..200 {
            runtime
                .replace_system_manifest("deepseek", Arc::new(replaced.clone()))
                .expect("swap must succeed");
            async_runtime
                .block_on(runtime.activate_staged_manifest_catalogs(configuration.as_ref(), &[]))
                .expect("complete selection generation must publish");
        }
        for handle in handles {
            handle.join().expect("reader thread must not panic");
        }
        let published = runtime
            .resolve_connection_model_selection(
                configuration.as_ref(),
                "generation-account",
                "deepseek",
                "deepseek-v4-pro",
            )
            .expect("the activated selection must resolve");
        assert_eq!(
            published.protocol_binding.manifest_revision, new_revision,
            "the writer must actually publish the staged generation"
        );
    }

    /// Verifies concurrent replacements of distinct providers cannot lose either update.
    /// 验证不同供应商的并发替换不会丢失任一更新。
    #[test]
    fn concurrent_distinct_provider_replacements_preserve_both_updates() {
        let runtime = std::sync::Arc::new(PublicAiRuntime::unavailable().expect("runtime"));
        let generation_before = runtime.catalog_generation();
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let compile_revision = |provider_id: &str, revision: u64| {
            let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
                .expect("bundled manifest must exist");
            let mut value: serde_json::Value =
                serde_json::from_str(&text).expect("manifest must parse");
            value["revision"] = serde_json::json!(revision);
            Arc::new(
                crate::provider_manifest::compile_manifest_text(
                    &serde_json::to_string(&value).expect("manifest must serialize"),
                    crate::provider_manifest::ManifestOwnership::System,
                )
                .expect("replacement manifest must compile"),
            )
        };
        let deepseek = compile_revision("deepseek", 101);
        let openrouter = compile_revision("openrouter", 202);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let deepseek_handle = {
            let runtime = std::sync::Arc::clone(&runtime);
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                runtime
                    .replace_system_manifest("deepseek", deepseek)
                    .expect("deepseek replacement must succeed");
            })
        };
        let openrouter_handle = {
            let runtime = std::sync::Arc::clone(&runtime);
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                runtime
                    .replace_system_manifest("openrouter", openrouter)
                    .expect("openrouter replacement must succeed");
            })
        };
        barrier.wait();
        deepseek_handle.join().expect("deepseek writer must join");
        openrouter_handle
            .join()
            .expect("openrouter writer must join");

        let catalog = runtime
            .bundled_manifests
            .read()
            .expect("catalog lock must not be poisoned");
        assert_eq!(
            catalog.manifest("deepseek").map(|value| value.revision),
            Some(101)
        );
        assert_eq!(
            catalog.manifest("openrouter").map(|value| value.revision),
            Some(202)
        );
        assert_eq!(
            catalog.generation,
            generation_before + 2,
            "each serialized whole-generation replacement must be retained"
        );
    }

    /// Verifies a malformed prepared generation fails before any staging catalog publication.
    /// 验证畸形候选代际会在任何暂存目录发布前失败。
    #[test]
    fn provider_manifest_generation_build_failure_preserves_current_catalog() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let generation_before = runtime.catalog_generation();
        let digest_before = runtime.catalog_content_hash();
        let openrouter = runtime
            .system_manifest("openrouter")
            .expect("openrouter manifest must exist");

        let result = runtime.prepare_system_manifest_replacement("deepseek", openrouter);
        let error = match result {
            Ok(_) => panic!("identity-mismatched generation preparation must fail"),
            Err(error) => error,
        };
        assert!(error.contains("declares provider `openrouter`"), "{error}");
        assert_eq!(runtime.catalog_generation(), generation_before);
        assert_eq!(runtime.catalog_content_hash(), digest_before);
    }

    /// Verifies compare-and-swap rejects a stale prepared generation without losing a newer update.
    /// 验证比较交换拒绝过期候选代际且不会丢失较新的更新。
    #[test]
    fn provider_manifest_generation_publish_conflict_preserves_newer_update() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let compile_revision = |provider_id: &str, revision: u64| {
            let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
                .expect("bundled manifest must exist");
            let mut value: serde_json::Value =
                serde_json::from_str(&text).expect("manifest must parse");
            value["revision"] = serde_json::json!(revision);
            Arc::new(
                crate::provider_manifest::compile_manifest_text(
                    &serde_json::to_string(&value).expect("manifest must serialize"),
                    crate::provider_manifest::ManifestOwnership::System,
                )
                .expect("replacement manifest must compile"),
            )
        };
        let stale = runtime
            .prepare_system_manifest_replacement("deepseek", compile_revision("deepseek", 301))
            .expect("candidate generation should prepare");
        runtime
            .replace_system_manifest("openrouter", compile_revision("openrouter", 302))
            .expect("newer provider update should publish");

        assert!(
            !runtime
                .publish_prepared_system_manifest(stale)
                .expect("stale publication should return a benign conflict"),
            "the stale base generation must be rejected"
        );
        assert_eq!(
            runtime
                .staged_system_manifest("openrouter")
                .expect("openrouter manifest must remain")
                .revision,
            302,
            "the newer update must remain authoritative"
        );
        assert_ne!(
            runtime
                .staged_system_manifest("deepseek")
                .expect("deepseek manifest must remain")
                .revision,
            301,
            "the stale candidate must not leak into the staging catalog"
        );
    }

    /// Verifies compensation restores the exact staged catalog Arc, generation, and digest.
    /// 验证补偿会恢复精确的暂存目录 Arc、代次与摘要。
    #[test]
    fn staged_manifest_compensation_restores_exact_before_image() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        // replacementText changes only the reviewed DeepSeek revision for a deterministic mutation.
        // replacementText 只修改已审核 DeepSeek 修订号，以产生确定性变更。
        let text = std::fs::read_to_string(directory.join("deepseek.json"))
            .expect("bundled manifest must exist");
        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("manifest must parse");
        value["revision"] = serde_json::json!(9_001_u64);
        let replacement = Arc::new(
            crate::provider_manifest::compile_manifest_text(
                &serde_json::to_string(&value).expect("manifest must serialize"),
                crate::provider_manifest::ManifestOwnership::System,
            )
            .expect("replacement manifest must compile"),
        );

        let mutation = runtime
            .replace_system_manifest_transactional("deepseek", replacement)
            .expect("replacement must publish");
        assert_eq!(
            mutation.after.generation,
            mutation.before.generation + 1,
            "the staged replacement must advance exactly one generation"
        );
        runtime
            .restore_staged_system_manifest_catalog(&mutation)
            .expect("exact compensation must succeed");
        let restored = runtime
            .bundled_manifests
            .read()
            .expect("staging lock must remain healthy")
            .clone();
        assert!(
            Arc::ptr_eq(&restored, &mutation.before),
            "compensation must reinstall the original immutable catalog"
        );
        assert_eq!(restored.generation, mutation.before.generation);
        assert_eq!(restored.content_sha256, mutation.before.content_sha256);
    }

    /// Verifies staged compensation refuses to overwrite a later independent replacement.
    /// 验证暂存补偿拒绝覆盖后续独立替换。
    #[test]
    fn staged_manifest_compensation_rejects_newer_mutation() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        // compileRevision builds one valid provider-specific replacement at a chosen revision.
        // compileRevision 为指定修订构建一份有效的供应商专属替换件。
        let compile_revision = |provider_id: &str, revision: u64| {
            let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
                .expect("bundled manifest must exist");
            let mut value: serde_json::Value =
                serde_json::from_str(&text).expect("manifest must parse");
            value["revision"] = serde_json::json!(revision);
            Arc::new(
                crate::provider_manifest::compile_manifest_text(
                    &serde_json::to_string(&value).expect("manifest must serialize"),
                    crate::provider_manifest::ManifestOwnership::System,
                )
                .expect("replacement manifest must compile"),
            )
        };
        let first = runtime
            .replace_system_manifest_transactional("deepseek", compile_revision("deepseek", 9_101))
            .expect("first replacement must publish");
        let second = runtime
            .replace_system_manifest_transactional(
                "openrouter",
                compile_revision("openrouter", 9_102),
            )
            .expect("second replacement must publish");

        let error = runtime
            .restore_staged_system_manifest_catalog(&first)
            .expect_err("out-of-order compensation must fail closed");
        assert!(error.contains("compensation refused"), "error: {error}");
        // Reverse-order unwind proves a multi-provider transaction can restore both exact Arcs.
        // 逆序展开证明多供应商事务能够恢复两个精确 Arc。
        runtime
            .restore_staged_system_manifest_catalog(&second)
            .expect("latest mutation must restore first");
        runtime
            .restore_staged_system_manifest_catalog(&first)
            .expect("earlier mutation must restore after its successor");
        let restored = runtime
            .bundled_manifests
            .read()
            .expect("staging lock must remain healthy")
            .clone();
        assert!(Arc::ptr_eq(&restored, &first.before));
    }

    /// Verifies timeout classification fires exactly at the frozen manifest deadline.
    /// 验证超时分类在冻结的 Manifest 截止时间精确触发。
    #[test]
    fn provider_task_timeout_uses_frozen_operation_deadline() {
        assert_eq!(provider_task_timeout_diagnostic(1_000, 3_000, 2_999), None);
        assert_eq!(
            provider_task_timeout_diagnostic(1_000, 3_000, 3_000).as_deref(),
            Some("Provider asynchronous task exceeded its 2000 millisecond deadline.")
        );
    }

    /// Verifies observation sleep is clipped to the exact remaining lifecycle budget.
    /// 验证观测等待被裁剪到精确的剩余生命周期预算。
    #[test]
    fn provider_task_observation_delay_never_crosses_deadline() {
        assert_eq!(
            clamp_provider_task_observation_delay_ms(60_000, 3_000, 2_999),
            1
        );
        assert_eq!(
            clamp_provider_task_observation_delay_ms(250, 3_000, 2_000),
            250
        );
        assert_eq!(
            clamp_provider_task_observation_delay_ms(250, 3_000, 3_000),
            0
        );
    }

    /// Verifies provider-wide and model-specific resource constraints merge into frozen routes.
    /// 验证供应商通用与模型专属资源约束会合并到冻结路由。
    #[test]
    fn bundled_resource_constraints_resolve_without_provider_hardcoding() {
        let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
        let minimax = manifests
            .get("minimax")
            .expect("MiniMax manifest must be bundled");
        let minimax_constraints = resolve_input_resource_constraints(
            minimax,
            "global",
            Some("payg-api-key"),
            "MiniMax-M3",
            Vec::new(),
        );
        assert_eq!(minimax_constraints.len(), 2);
        assert_eq!(
            minimax_constraints
                .iter()
                .find(|constraint| {
                    constraint.resource_kind == vulcan_ai_contract::ResourceKind::Video
                })
                .and_then(|constraint| constraint.max_bytes),
            Some(50_000_000)
        );

        let openrouter = manifests
            .get("openrouter")
            .expect("OpenRouter manifest must be bundled");
        let chat_route = openrouter.descriptor.service_entries[0]
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == OPENAI_CHAT_PROFILE_ID)
            .expect("OpenRouter Chat route must exist");
        assert_eq!(chat_route.input_resource_constraints.len(), 2);
        assert!(
            chat_route
                .input_resource_constraints
                .iter()
                .any(|constraint| {
                    constraint.resource_kind == vulcan_ai_contract::ResourceKind::File
                        && constraint.accepted_mime_types == [String::from("application/pdf")]
                })
        );
    }

    /// Verifies model rules and default bindings replace all three route profile chains.
    /// 验证模型规则与默认绑定会替换三类路由 Profile 链。
    #[test]
    fn model_scoped_profiles_freeze_into_the_execution_route() {
        // manifests supplies validated payloads so this test exercises only runtime precedence.
        // manifests 提供已验证载荷，使本测试只验证运行时优先级。
        let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
        let mut manifest = manifests
            .get("deepseek")
            .expect("DeepSeek manifest must be bundled")
            .as_ref()
            .clone();
        // This precedence fixture intentionally enables a generic unmatched-model default so the
        // test can compare rule-owned and default-owned profile chains independently of DeepSeek's
        // production fixed-inventory policy.
        // 本优先级夹具有意启用通用未匹配模型默认值，使测试可以脱离 DeepSeek 生产固定清单
        // 策略，独立比较规则持有与默认持有的 Profile 链。
        manifest.unknown_model_policy =
            crate::provider_manifest::UnknownModelPolicy::DefaultBindingAllowed;
        let minimax = manifests
            .get("minimax")
            .expect("MiniMax manifest must be bundled");
        let default_response = minimax
            .response_profiles
            .first()
            .expect("MiniMax must declare a response profile")
            .clone();
        let rule_response = minimax
            .response_profiles
            .get(1)
            .expect("MiniMax must declare a second response profile")
            .clone();
        manifest.response_profiles.push(default_response.clone());
        manifest.response_profiles.push(rule_response.clone());
        let defaults = manifest
            .entry_model_defaults
            .get_mut("official-api")
            .expect("DeepSeek official entry defaults must compile");
        defaults.usage_profile_refs = vec![String::from("deepseek.chat.body.v1")];
        defaults.request_profile_refs = vec![String::from("deepseek.responses.docs.v1")];
        defaults.response_profile_refs = vec![default_response.profile_id.clone()];
        let rule = manifest
            .rule_instances
            .iter_mut()
            .find(|rule| rule.matcher.matches("deepseek-v4-flash"))
            .expect("DeepSeek flash rule must compile");
        rule.usage_profile_refs = vec![String::from("deepseek.chat.sse.usage.v1")];
        rule.request_profile_refs = vec![String::from("deepseek.chat.thinking.v1")];
        rule.response_profile_refs = vec![rule_response.profile_id.clone()];

        let matched_usage = resolve_usage_extraction_chain(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-v4-flash",
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            Vec::new(),
        )
        .expect("matched usage profile must resolve");
        let default_usage = resolve_usage_extraction_chain(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-future-model",
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            Vec::new(),
        )
        .expect("default usage profile must resolve");
        assert_eq!(matched_usage[0].profile_id, "deepseek.chat.sse.usage.v1");
        assert_eq!(default_usage[0].profile_id, "deepseek.chat.body.v1");

        let matched_request = resolve_request_projection_chain(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-v4-flash",
            Vec::new(),
        )
        .expect("matched request profile must resolve");
        let default_request = resolve_request_projection_chain(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-future-model",
            Vec::new(),
        )
        .expect("default request profile must resolve");
        assert_eq!(matched_request[0].profile_id, "deepseek.chat.thinking.v1");
        assert_eq!(default_request[0].profile_id, "deepseek.responses.docs.v1");

        let matched_response = resolve_response_projection_chain(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-v4-flash",
            Vec::new(),
        )
        .expect("matched response profile must resolve");
        let default_response_chain = resolve_response_projection_chain(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-future-model",
            Vec::new(),
        )
        .expect("default response profile must resolve");
        assert_eq!(matched_response[0].profile_id, rule_response.profile_id);
        assert_eq!(
            default_response_chain[0].profile_id,
            default_response.profile_id
        );
    }

    /// Verifies an explicit-empty model profile override clears provider special cases while
    /// preserving the standard protocol usage baseline.
    /// 验证显式空模型 Profile 覆盖会清除供应商特例，同时保留标准协议 usage 基线。
    #[test]
    fn explicit_empty_model_profiles_clear_route_special_cases() {
        // manifests provides real compiled provider-specific payloads for all three chains.
        // manifests 为三类链提供真实已编译的供应商专属载荷。
        let manifests = crate::provider_manifest::load_bundled_resolved_manifests();
        let mut manifest = manifests
            .get("deepseek")
            .expect("DeepSeek manifest must be bundled")
            .as_ref()
            .clone();
        let minimax = manifests
            .get("minimax")
            .expect("MiniMax manifest must be bundled");
        // routeUsageDefault, routeRequestDefault, and routeResponseDefault emulate the selected
        // route's provider special cases before model-level replacement.
        // routeUsageDefault、routeRequestDefault 与 routeResponseDefault 模拟模型级替换前
        // 所选路由的供应商特例。
        let route_usage_default = vec![
            manifest
                .usage_profiles
                .first()
                .and_then(|declaration| declaration.payload.clone())
                .expect("DeepSeek must declare a compiled usage profile"),
        ];
        let route_request_default = vec![
            manifest
                .request_profiles
                .first()
                .and_then(|declaration| declaration.payload.clone())
                .expect("DeepSeek must declare a compiled request profile"),
        ];
        let route_response_default = vec![
            minimax
                .response_profiles
                .first()
                .and_then(|declaration| declaration.payload.clone())
                .expect("MiniMax must declare a compiled response profile"),
        ];
        // rule carries the compiler fact produced by an explicit empty list, including rule-owned
        // provenance even though the frozen references are empty.
        // rule 携带显式空列表产生的编译事实；即使冻结引用为空，来源仍归规则所有。
        let rule = manifest
            .rule_instances
            .iter_mut()
            .find(|rule| rule.matcher.matches("deepseek-v4-flash"))
            .expect("DeepSeek flash rule must compile");
        rule.usage_profile_refs.clear();
        rule.request_profile_refs.clear();
        rule.response_profile_refs.clear();
        for field in [
            "usage_profile_refs",
            "request_profile_refs",
            "response_profile_refs",
        ] {
            rule.field_provenance
                .get_mut(field)
                .expect("profile provenance must exist")
                .source = crate::provider_manifest::FieldSourceKind::ModelRulePrefix;
        }

        // clearedUsage must restore only the protocol baseline; request and response have no
        // lower protocol baseline and therefore become empty.
        // clearedUsage 必须只恢复协议基线；请求与响应不存在更低协议基线，因此变为空。
        let cleared_usage = resolve_usage_extraction_chain(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-v4-flash",
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            route_usage_default,
        )
        .expect("explicit usage clear must restore the standard baseline");
        let cleared_request = resolve_request_projection_chain(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-v4-flash",
            route_request_default,
        )
        .expect("explicit request clear must resolve");
        let cleared_response = resolve_response_projection_chain(
            &manifest,
            "official-api",
            Some("api-key"),
            "deepseek-v4-flash",
            route_response_default,
        )
        .expect("explicit response clear must resolve");
        let expected_usage = vulcan_llm_core::standard_usage_profiles(
            vulcan_llm_core::UpstreamProtocol::OpenAiChatCompletions,
        );
        assert_eq!(
            cleared_usage
                .iter()
                .map(|profile| profile.profile_id.as_str())
                .collect::<Vec<_>>(),
            expected_usage
                .iter()
                .map(|profile| profile.profile_id.as_str())
                .collect::<Vec<_>>()
        );
        assert!(cleared_request.is_empty());
        assert!(cleared_response.is_empty());
    }

    /// Verifies source and membership policies jointly expose the exact identity union.
    /// 验证来源策略与成员策略共同公开精确身份并集。
    #[test]
    fn model_source_policies_control_runtime_identity_union() {
        use crate::provider_manifest::{
            CatalogMembershipPolicy, ManifestOwnership, ModelSourcePolicy,
        };

        // sourcePolicies and membershipPolicies form the complete 6x3 closed policy matrix.
        // sourcePolicies 与 membershipPolicies 组成完整的 6x3 封闭策略矩阵。
        let source_policies = [
            ModelSourcePolicy::ManifestOnly,
            ModelSourcePolicy::DiscoveryOnly,
            ModelSourcePolicy::ManifestAndDiscovery,
            ModelSourcePolicy::ManualOnly,
            ModelSourcePolicy::DiscoveryAndManual,
            ModelSourcePolicy::ManifestDiscoveryAndManual,
        ];
        let membership_policies = [
            CatalogMembershipPolicy::DeclaredOnly,
            CatalogMembershipPolicy::DiscoveryEvidence,
            CatalogMembershipPolicy::DeclaredAndDiscovery,
        ];
        for source_policy in source_policies {
            for membership_policy in membership_policies {
                let discovery_kind = if source_policy.includes_discovery() {
                    "open_ai"
                } else if source_policy.includes_manifest() {
                    "manifest_static"
                } else {
                    "manual"
                };
                // modelCatalog is assembled from the policy truth table without optional fallback fields.
                // modelCatalog 依据策略真值表组装，不包含可选兜底字段。
                let mut model_catalog = serde_json::json!({
                    "source_policy": serde_json::to_value(source_policy).expect("policy must serialize"),
                    "discovery_kind": discovery_kind
                });
                if source_policy.includes_manifest() {
                    model_catalog["models"] = serde_json::json!([
                        {"model_id": "manifest-model", "display_name": "Manifest Model"}
                    ]);
                }
                let manifest_value = serde_json::json!({
                    "schema_version": "vulcan.provider-manifest.v2",
                    "provider_id": "source-policy-probe",
                    "revision": 1,
                    "published_at": "2026-08-09T00:00:00Z",
                    "engine_contract": {
                        "minimum_manifest_engine": 1,
                        "required_protocol_profiles": [OPENAI_CHAT_PROFILE_ID]
                    },
                    "display": {"name": "Source Policy Probe", "category": "official"},
                    "services": [{
                        "service_entry_id": "public-api",
                        "display_name": "Public API",
                        "kind": "conversation",
                        "base_url": "https://source-policy.example.test/v1",
                        "credential_profiles": [{
                            "credential_profile_id": "api-key",
                            "display_name": "API Key",
                            "credential_mode": "token"
                        }],
                        "default_authentication": "bearer_authorization",
                        "model_catalog": model_catalog,
                        "protocol_routes": [{
                            "protocol_profile_id": OPENAI_CHAT_PROFILE_ID,
                            "endpoint_path": "/chat/completions",
                            "verified": true
                        }]
                    }],
                    "model_defaults": {
                        "default_enabled": true,
                        "catalog_membership_policy": serde_json::to_value(membership_policy)
                            .expect("membership policy must serialize"),
                        "unknown_model_policy": "declared_rule_required",
                        "supported_protocols": [OPENAI_CHAT_PROFILE_ID],
                        "selected_protocol": OPENAI_CHAT_PROFILE_ID,
                        "capabilities": {"operations": ["conversation_respond"]}
                    },
                    "model_rules": [
                        {
                            "rule_id": "manifest-model",
                            "matcher": {"kind": "exact", "value": "manifest-model"},
                            "supported_protocols": [OPENAI_CHAT_PROFILE_ID],
                            "selected_protocol": OPENAI_CHAT_PROFILE_ID
                        },
                        {
                            "rule_id": "discovery-model",
                            "matcher": {"kind": "exact", "value": "discovery-model"},
                            "supported_protocols": [OPENAI_CHAT_PROFILE_ID],
                            "selected_protocol": OPENAI_CHAT_PROFILE_ID
                        },
                        {
                            "rule_id": "manual-model",
                            "matcher": {"kind": "exact", "value": "manual-model"},
                            "supported_protocols": [OPENAI_CHAT_PROFILE_ID],
                            "selected_protocol": OPENAI_CHAT_PROFILE_ID
                        }
                    ],
                    "provenance": {"status": "migrated", "sources": []}
                });
                let manifest = crate::provider_manifest::compile_manifest_text(
                    serde_json::to_string(&manifest_value)
                        .expect("source-policy manifest must serialize")
                        .as_str(),
                    ManifestOwnership::System,
                )
                .expect("source-policy manifest must compile");
                let connection = CredentialConnection {
                    id: String::from("connection-source-policy"),
                    provider_id: String::from("source-policy-probe"),
                    service_entry_id: String::from("public-api"),
                    credential_profile_id: String::from("api-key"),
                    credential_mode: CredentialMode::Token,
                    credential_id: String::from("credential-source-policy"),
                    display_name: String::from("Source Policy"),
                    secret_reference: String::from("secret-source-policy"),
                    management_secret_reference: None,
                    service_origin_override: None,
                    prompt_cache_by_protocol: BTreeMap::new(),
                };
                let mut configuration = PersistedAiConfiguration::default();
                configuration
                    .connection_graph
                    .connections
                    .insert(connection.id.clone(), connection.clone());
                configuration.custom_models.insert(
                    String::from("source-policy-probe"),
                    BTreeMap::from([(
                        String::from("manual-model"),
                        crate::CustomModelDefinition {
                            id: String::from("manual-model"),
                            display_name: String::from("Manual Model"),
                            enabled: true,
                            capabilities: ModelCapabilities {
                                operations: vec![AiOperation::ConversationRespond],
                                ..ModelCapabilities::default()
                            },
                            request_projection: Default::default(),
                            tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                        },
                    )]),
                );
                let mut source_catalog = ConnectionModelCatalog {
                    connection_id: connection.id.clone(),
                    models: ["discovery-model", "unmatched-discovery-model"]
                        .into_iter()
                        .map(|model_id| ConnectionModel {
                            id: model_id.to_string(),
                            display_name: model_id.to_string(),
                            capabilities: ModelCapabilities {
                                operations: vec![AiOperation::ConversationRespond],
                                ..ModelCapabilities::default()
                            },
                            capability_evidence: ModelCapabilityEvidence::default(),
                            protocol_variants: BTreeMap::from([(
                                OPENAI_CHAT_PROFILE_ID.to_string(),
                                ProtocolModelVariant {
                                    capabilities: ModelCapabilities {
                                        operations: vec![AiOperation::ConversationRespond],
                                        ..ModelCapabilities::default()
                                    },
                                    capability_evidence: ModelCapabilityEvidence::default(),
                                    discovery_signature: String::from("sha256:discovery"),
                                    catalog_generation: 7,
                                },
                            )]),
                            resolved_protocol_binding: None,
                        })
                        .collect(),
                    generation: 7,
                    page_count: 1,
                    discovery_schema_revision: String::from("openai-compatible.models.v1"),
                };
                apply_model_catalog_overlays_with_account_access(
                    &configuration,
                    "source-policy-probe",
                    "public-api",
                    None,
                    source_policy,
                    Some(&manifest),
                    &mut source_catalog,
                )
                .expect("source-policy overlay must resolve");
                let mut expected_model_ids = BTreeSet::new();
                if source_policy.includes_manifest()
                    && membership_policy.includes_declared_identities()
                {
                    expected_model_ids.insert(String::from("manifest-model"));
                }
                if source_policy.includes_discovery() {
                    expected_model_ids.insert(String::from("discovery-model"));
                    if membership_policy.admits_unmatched_discovery() {
                        expected_model_ids.insert(String::from("unmatched-discovery-model"));
                    }
                }
                if source_policy.includes_manual() {
                    expected_model_ids.insert(String::from("manual-model"));
                }
                let actual_model_ids = source_catalog
                    .models
                    .iter()
                    .map(|model| model.id.clone())
                    .collect::<BTreeSet<_>>();
                assert_eq!(
                    actual_model_ids, expected_model_ids,
                    "source {source_policy:?}, membership {membership_policy:?}"
                );
                if source_policy.includes_manual() {
                    let manual_model = source_catalog
                        .models
                        .iter()
                        .find(|model| model.id == "manual-model")
                        .expect("manual source policy must admit the manual identity");
                    assert!(
                        manual_model
                            .protocol_variants
                            .contains_key(OPENAI_CHAT_PROFILE_ID),
                        "signed Manifest rule must produce the manual model's executable variant"
                    );
                }
            }
        }
    }

    /// Verifies Manifest-only refresh succeeds without reading a secret or contacting metadata.
    /// 验证 Manifest-only 刷新无需读取秘密或访问元数据即可成功。
    #[tokio::test]
    async fn manifest_only_refresh_performs_no_credential_or_remote_discovery() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let generation = runtime
            .selection_catalog_snapshot()
            .expect("manifest generation must be available");
        let manifest = generation
            .manifests
            .manifest("alibaba")
            .expect("Alibaba Manifest must be bundled");
        let entry = manifest
            .descriptor
            .service_entries
            .iter()
            .find(|entry| {
                entry.kind == crate::ServiceEntryKind::Conversation
                    && manifest
                        .model_source_policy(entry.id.as_str())
                        .is_ok_and(|policy| policy == ModelSourcePolicy::ManifestOnly)
            })
            .expect("Alibaba must expose a Manifest-only conversation entry");
        let profile = entry
            .credential_profiles
            .first()
            .expect("Manifest-only entry must declare a credential profile");
        let _route = entry
            .protocol_routes
            .iter()
            .find(|route| route.verified)
            .expect("Manifest-only entry must declare a verified route");
        let connection = CredentialConnection {
            id: String::from("connection-manifest-only"),
            provider_id: String::from("alibaba"),
            service_entry_id: entry.id.clone(),
            credential_profile_id: profile.id.clone(),
            credential_mode: profile.credential_mode,
            credential_id: String::from("credential-manifest-only"),
            display_name: String::from("Manifest Only"),
            secret_reference: String::from("unavailable-secret-must-not-be-read"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .connections
            .insert(connection.id.clone(), connection.clone());

        let catalog = runtime
            .refresh_connection_catalog(&configuration, connection.id.as_str())
            .await
            .expect("Manifest-only refresh must not require the unavailable secret store");
        assert!(!catalog.models.is_empty());
        assert_eq!(
            catalog.discovery_schema_revision,
            crate::client::MANIFEST_STATIC_DISCOVERY_SCHEMA_REVISION
        );
    }

    /// Verifies custom manual-only refresh succeeds without a `/models` request or secret read.
    /// 验证自定义 manual-only 刷新无需 `/models` 请求或秘密读取即可成功。
    #[tokio::test]
    async fn custom_manual_only_refresh_performs_no_credential_or_remote_discovery() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let provider = crate::CustomProviderDefinition {
            id: String::from("custom:manual-only-probe"),
            display_name: String::from("Manual Only Probe"),
            base_url: String::from("https://manual-only.example.test/v1"),
            protocols: vec![crate::CustomProviderProtocol::OpenAiChat],
            authentication: crate::CustomProviderAuthentication::BearerAuthorization,
            model_discovery_mode: crate::CustomProviderModelDiscoveryMode::Manual,
            model_discovery_path: None,
            protocol_routes: Vec::new(),
            request_headers: BTreeMap::new(),
        }
        .into_descriptor()
        .expect("manual-only custom provider must project");
        let entry = provider
            .service_entries
            .first()
            .expect("custom provider must expose one entry");
        let profile = entry
            .credential_profiles
            .first()
            .expect("custom provider must expose one profile");
        let _route = entry
            .protocol_routes
            .first()
            .expect("custom provider must expose one route");
        let connection = CredentialConnection {
            id: String::from("connection-custom-manual-only"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id: profile.id.clone(),
            credential_mode: profile.credential_mode,
            credential_id: String::from("credential-custom-manual-only"),
            display_name: String::from("Custom Manual Only"),
            secret_reference: String::from("unavailable-secret-must-not-be-read"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .custom_providers
            .insert(provider.id.clone(), provider.clone());
        configuration
            .connection_graph
            .connections
            .insert(connection.id.clone(), connection.clone());
        configuration.custom_models.insert(
            provider.id.clone(),
            BTreeMap::from([(
                String::from("manual-model"),
                crate::CustomModelDefinition {
                    id: String::from("manual-model"),
                    display_name: String::from("Manual Model"),
                    enabled: true,
                    capabilities: ModelCapabilities {
                        operations: vec![AiOperation::ConversationRespond],
                        ..ModelCapabilities::default()
                    },
                    request_projection: Default::default(),
                    tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                },
            )]),
        );

        let catalog = runtime
            .refresh_connection_catalog(&configuration, connection.id.as_str())
            .await
            .expect("manual-only refresh must not require the unavailable secret store");
        assert_eq!(
            catalog
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["manual-model"]
        );
        assert_eq!(
            catalog.discovery_schema_revision,
            MANUAL_MODEL_CATALOG_SCHEMA_REVISION
        );
    }
}
