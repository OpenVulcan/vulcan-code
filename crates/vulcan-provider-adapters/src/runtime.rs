//! Process-local provider catalogs, clients, and credential access.
//! 进程内供应商目录、客户端与凭据访问。

use crate::provider_task::{
    ProviderAsyncTaskPhase, ProviderAsyncTaskPoll, ProviderAsyncTaskRecord,
    ProviderAsyncTaskRecoveryContext, ProviderAsyncTaskRoute, ProviderTaskRecoveryStore,
};
use crate::{
    ActiveAiModelSelection, ConnectionModel, ConnectionModelCatalog, CredentialConnection,
    CredentialExecutionSecret, CredentialSecretEnvelope, HttpProviderMetadataClient,
    InferenceRouteBinding, InferenceRouteConfiguration, ModelDiscoveryFailureKind,
    OAuthAuthorizationRuntime, PersistedAiConfiguration, ProviderExecutionClient,
    ProviderExecutionFailure, ProviderExecutionRoute, ProviderExecutionStream,
    ProviderMetadataClient, ProviderRegistry, ProviderToolCapabilityKind,
    ProviderToolExecutionResult, ProviderToolTarget, ProviderToolTargetSelection,
    RuntimeConnectionCatalogs, SecondaryRouteOption, SecondaryRouteSelection,
    built_in_provider_tool_targets, native_inference::validate_native_action_binding,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;
use vulcan_ai_contract::{
    AccountUsageSnapshot, AiExecutionRequest, AiOperation, EmbeddingRequest, EmbeddingResponse,
    InferenceUsageScope, ModelCapabilities, PromptCachePolicy, RerankRequest, RerankResponse,
};
use vulcan_ai_protocols::ProtocolRegistry;

/// Refreshes renewable tokens this long before their declared expiration.
/// 在可续期 Token 声明过期前这么长时间执行刷新。
const OAUTH_REFRESH_SKEW_MS: u64 = 5 * 60 * 1_000;

/// Keeps provider endpoint discovery results fresh for six hours by default.
/// 默认将供应商端点发现结果保鲜六小时。
const SECONDARY_ROUTE_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Maximum retained provider-native asynchronous task duration.
/// 供应商原生异步任务的最长保留执行时长。
const PROVIDER_ASYNC_TASK_TIMEOUT_MS: u64 = 30 * 60 * 1_000;

/// Poll cadence for provider-native asynchronous task workers.
/// 供应商原生异步任务工作器的轮询节奏。
const PROVIDER_ASYNC_TASK_POLL_INTERVAL: Duration = Duration::from_secs(15);

/// Maximum wall time for one provider task poll or cancellation observation.
/// 单次供应商任务轮询或取消观测的最大墙钟时间。
const PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(60);

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

/// Stores one process-local secondary-route discovery result.
/// 保存一份进程内二级路由发现结果。
#[derive(Debug, Clone)]
struct SecondaryRouteCacheEntry {
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
    /// Strict first-batch provider registry.
    /// 严格的首批供应商注册表。
    providers: ProviderRegistry,
    /// Provider-neutral wire protocol registry.
    /// 供应商无关的 Wire 协议注册表。
    protocols: Arc<ProtocolRegistry>,
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
    /// Serializes source/effective catalog replacement so overlay rebuilds cannot overwrite a newer refresh.
    /// 串行化源目录与有效目录替换，避免覆盖重建写回并覆盖更新的刷新结果。
    catalog_mutation_lock: Arc<StdMutex<()>>,
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
    /// Authoritative standalone public AI configuration snapshot.
    /// 权威独立公共 AI 配置快照。
    configuration: Arc<RwLock<PersistedAiConfiguration>>,
    /// Durable non-secret asynchronous provider-task recovery registry.
    /// 可持久化非秘密异步供应商任务恢复注册表。
    provider_tasks: ProviderTaskRecoveryStore,
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
        let protocols = ProtocolRegistry::first_batch().map_err(|error| error.to_string())?;
        let protocols = Arc::new(protocols);
        let http = build_public_ai_http_client()?;
        Ok(Self {
            providers: ProviderRegistry::first_batch()?,
            execution_client: ProviderExecutionClient::new(http.clone(), Arc::clone(&protocols)),
            metadata_client: HttpProviderMetadataClient::new(http.clone()),
            oauth: Arc::new(OAuthAuthorizationRuntime::new(http)),
            protocols,
            catalogs: Arc::new(RwLock::new(RuntimeConnectionCatalogs::default())),
            source_catalogs: Arc::new(RwLock::new(RuntimeConnectionCatalogs::default())),
            catalog_mutation_lock: Arc::new(StdMutex::new(())),
            catalog_errors: Arc::new(RwLock::new(BTreeMap::new())),
            catalog_statuses: Arc::new(RwLock::new(BTreeMap::new())),
            usage_cache: Arc::new(RwLock::new(BTreeMap::new())),
            secondary_route_cache: Arc::new(RwLock::new(BTreeMap::new())),
            refresh_locks: Arc::new(StdMutex::new(BTreeMap::new())),
            secrets,
            configuration: Arc::new(RwLock::new(PersistedAiConfiguration::default())),
            provider_tasks: ProviderTaskRecoveryStore::default(),
        })
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

    /// Returns the strict provider registry.
    /// 返回严格供应商注册表。
    pub fn providers(&self) -> &ProviderRegistry {
        &self.providers
    }

    /// Builds the effective provider registry for one immutable configuration snapshot.
    /// 为一个不可变配置快照构建有效供应商注册表。
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
        ProviderRegistry::effective(&configuration.custom_providers)
    }

    /// Returns the authoritative standalone configuration snapshot.
    /// 返回权威独立配置快照。
    ///
    /// # Returns
    /// 返回值
    /// A clone safe to retain for one execution or settings transaction.
    /// 返回可供单次执行或设置事务保留的安全克隆。
    pub fn configuration_snapshot(&self) -> Result<PersistedAiConfiguration, String> {
        Ok(self
            .configuration
            .read()
            .map_err(|_| String::from("The public AI configuration lock is poisoned."))?
            .clone())
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
        *self
            .configuration
            .write()
            .map_err(|_| String::from("The public AI configuration lock is poisoned."))? =
            configuration;
        Ok(())
    }

    /// Returns the protocol registry.
    /// 返回协议注册表。
    pub fn protocols(&self) -> &ProtocolRegistry {
        self.protocols.as_ref()
    }

    /// Returns the shared metadata client.
    /// 返回共享元数据客户端。
    pub fn metadata_client(&self) -> &HttpProviderMetadataClient {
        &self.metadata_client
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
        self.catalogs
            .write()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .replace(catalog);
        Ok(())
    }

    /// Returns all runtime-only model catalogs without persisting them.
    /// 返回全部仅驻留运行时的模型目录，但不持久化它们。
    ///
    /// # Returns
    /// 返回值
    /// A cloned connection-to-catalog map.
    /// 返回克隆后的连接到目录映射。
    pub fn catalog_snapshot(&self) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        Ok(self
            .catalogs
            .read()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .snapshot())
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
        if !configuration.model_is_enabled(provider_id, model_id) {
            return Ok(false);
        }
        let catalogs = self
            .catalogs
            .read()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?;
        Ok(configuration
            .connection_graph
            .connections
            .values()
            .filter(|connection| connection.provider_id == provider_id)
            .filter(|connection| {
                !configuration.connection_is_pending_validation(connection.id.as_str())
            })
            .any(|connection| {
                catalogs
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
        let catalogs = self
            .catalogs
            .read()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?;
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
                        configuration
                            .model_is_enabled(connection.provider_id.as_str(), model.id.as_str())
                            && model_supports_text_conversation(&model.capabilities)
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
        Ok(self
            .catalog_errors
            .read()
            .map_err(|_| String::from("The AI catalog error lock is poisoned."))?
            .clone())
    }

    /// Returns immutable provider-discovery catalogs before user overlays.
    /// 返回应用用户覆盖前的不可变供应商发现目录。
    ///
    /// # Returns
    /// 返回值
    /// A cloned connection-to-source-catalog map.
    /// 返回克隆后的连接到源目录映射。
    pub fn source_catalog_snapshot(
        &self,
    ) -> Result<BTreeMap<String, ConnectionModelCatalog>, String> {
        Ok(self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .snapshot())
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
        self.refresh_locks
            .lock()
            .map_err(|_| String::from("The AI credential refresh lock is poisoned."))?
            .remove(connection_id);
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
        let observed_at_ms = current_unix_millis()?;
        let (provider, entry, connection) =
            self.resolve_connection_metadata(configuration, connection_id)?;
        let secret = self
            .resolve_fresh_execution_secret(
                connection.id.as_str(),
                connection.secret_reference.as_str(),
            )
            .await?;
        let generation = self
            .catalogs
            .read()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .get(connection_id)
            .map_or(1, |catalog| catalog.generation.saturating_add(1));
        let discovery = self
            .metadata_client
            .discover_models(&provider, &entry, &connection, &secret, generation)
            .await;
        let source_catalog = match discovery {
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
                self.install_manual_custom_catalog_without_discovery(
                    configuration,
                    &connection,
                    generation,
                )?;
                return Err(error_message);
            }
        };
        let source_model_count = source_catalog.models.len();
        let mut catalog = source_catalog.clone();
        let kimi_coding_plan = provider.id == "kimi" && entry.id == "coding-plan";
        if kimi_coding_plan {
            let usage = self
                .metadata_client
                .query_account_usage(&entry, &connection, &secret)
                .await;
            self.cache_usage_snapshot(connection_id, usage, true)?;
        }
        let account_plan_name = self.cached_account_plan_name(connection_id)?;
        if let Err(error) = apply_model_catalog_overlays_with_account_plan(
            configuration,
            provider.id.as_str(),
            entry.id.as_str(),
            account_plan_name.as_deref(),
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
        self.replace_catalog(catalog.clone())?;
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
        let account_plan_name = self.cached_account_plan_name(connection_id)?;
        apply_model_catalog_overlays_with_account_plan(
            configuration,
            provider.id.as_str(),
            entry.id.as_str(),
            account_plan_name.as_deref(),
            &mut catalog,
        )?;
        validate_resolved_catalog_limits(&catalog)?;
        self.replace_catalog(catalog)
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
        let _mutation_guard = self
            .catalog_mutation_lock
            .lock()
            .map_err(|_| String::from("The AI catalog mutation lock is poisoned."))?;
        let effective = self.resolve_model_catalog_overlays(configuration)?;
        *self
            .catalogs
            .write()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))? = effective;
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
        let source_catalogs = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .snapshot();
        let usage_cache = self.usage_cache_snapshot()?;
        let mut effective = RuntimeConnectionCatalogs::default();
        for connection in configuration.connection_graph.connections.values() {
            let Some(mut catalog) = source_catalogs
                .get(connection.id.as_str())
                .cloned()
                .or_else(|| manual_custom_catalog(configuration, connection, 1))
            else {
                continue;
            };
            let account_plan_name = usage_cache
                .get(connection.id.as_str())
                .and_then(|usage| usage.plan_name.as_deref());
            apply_model_catalog_overlays_with_account_plan(
                configuration,
                connection.provider_id.as_str(),
                connection.service_entry_id.as_str(),
                account_plan_name,
                &mut catalog,
            )?;
            validate_resolved_catalog_limits(&catalog)?;
            effective.replace(catalog);
        }
        Ok(effective)
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
        // source_catalog keeps provider evidence immutable while the candidate is evaluated.
        // source_catalog 在评估候选配置时保持供应商证据不可变。
        let source_catalog = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .get(connection_id)
            .cloned();
        let Some(mut catalog) =
            source_catalog.or_else(|| manual_custom_catalog(configuration, connection, 1))
        else {
            return Ok(false);
        };
        let account_plan_name = self.cached_account_plan_name(connection.id.as_str())?;
        apply_model_catalog_overlays_with_account_plan(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            account_plan_name.as_deref(),
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
        let mut candidate_configuration = configuration.clone();
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

        let source_catalogs = self
            .source_catalogs
            .read()
            .map_err(|_| String::from("The source AI model catalog lock is poisoned."))?
            .snapshot();
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
            let account_plan_name = self.cached_account_plan_name(connection.id.as_str())?;
            apply_model_catalog_overlays_with_account_plan(
                &candidate_configuration,
                provider_id,
                connection.service_entry_id.as_str(),
                account_plan_name.as_deref(),
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
        }
        if !matched_connection {
            return Err(format!(
                "Provider `{provider_id}` has no immutable or manual catalog source for model `{model_id}`. Synchronize the model catalog before saving capability overrides."
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
        let account_plan_name = self.cached_account_plan_name(connection.id.as_str())?;
        apply_model_catalog_overlays_with_account_plan(
            configuration,
            connection.provider_id.as_str(),
            connection.service_entry_id.as_str(),
            account_plan_name.as_deref(),
            &mut catalog,
        )?;
        self.replace_catalog(catalog)
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
        let (provider, entry, connection) =
            self.resolve_connection_metadata(configuration, connection_id)?;
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                )
            })?;
        let secret_reference = match credential_profile.account_usage_query {
            crate::AccountUsageQueryKind::OpenRouterCredits => connection
                .management_secret_reference
                .as_deref()
                .ok_or_else(|| {
                    String::from(
                        "Add an optional OpenRouter management key to query account credits.",
                    )
                })?,
            crate::AccountUsageQueryKind::Unsupported
            | crate::AccountUsageQueryKind::MiniMaxTokenPlan
            | crate::AccountUsageQueryKind::TavilyCredits
            | crate::AccountUsageQueryKind::DeepSeekBalance
            | crate::AccountUsageQueryKind::KimiCodingPlan => connection.secret_reference.as_str(),
        };
        let secret = self
            .resolve_fresh_execution_secret(connection.id.as_str(), secret_reference)
            .await?;
        let snapshot = self
            .metadata_client
            .query_account_usage(&entry, &connection, &secret)
            .await;
        let kimi_coding_plan = provider.id == "kimi" && entry.id == "coding-plan";
        let snapshot = self.cache_usage_snapshot(connection_id, snapshot, kimi_coding_plan)?;
        if kimi_coding_plan {
            self.rebuild_catalog_after_usage(configuration, connection_id)?;
        }
        Ok(snapshot)
    }

    /// Caches one usage observation while retaining a confirmed Kimi plan through transient refresh failures.
    /// 缓存一次用量观测，并在暂时刷新失败时保留已确认的 Kimi 套餐。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the exact credential, `snapshot` is the provider observation,
    /// and `preserve_plan_name` enables the Kimi-specific stale-plan rule.
    /// `connection_id` 标识精确凭据，`snapshot` 是供应商观测，`preserve_plan_name` 启用 Kimi 专用旧套餐规则。
    ///
    /// # Returns
    /// 返回值
    /// The snapshot stored in the process-local cache.
    /// 已写入进程内缓存的观测。
    fn cache_usage_snapshot(
        &self,
        connection_id: &str,
        mut snapshot: AccountUsageSnapshot,
        preserve_plan_name: bool,
    ) -> Result<AccountUsageSnapshot, String> {
        let mut cache = self
            .usage_cache
            .write()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?;
        if preserve_plan_name && snapshot.plan_name.is_none() {
            snapshot.plan_name = cache
                .get(connection_id)
                .and_then(|previous| previous.plan_name.clone());
        }
        cache.insert(connection_id.to_string(), snapshot.clone());
        Ok(snapshot)
    }

    /// Reads the normalized plan name associated with one exact credential.
    /// 读取一个精确凭据关联的规范化套餐名称。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` identifies the process-local usage observation.
    /// `connection_id` 标识进程内用量观测。
    ///
    /// # Returns
    /// 返回值
    /// The confirmed plan name when the usage cache contains one.
    /// 当用量缓存包含已确认套餐时返回其名称。
    fn cached_account_plan_name(&self, connection_id: &str) -> Result<Option<String>, String> {
        self.usage_cache
            .read()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))
            .map(|cache| {
                cache
                    .get(connection_id)
                    .and_then(|usage| usage.plan_name.clone())
            })
    }

    /// Returns the complete process-local account-usage cache.
    /// 返回完整的进程内账号用量缓存。
    pub fn usage_cache_snapshot(&self) -> Result<BTreeMap<String, AccountUsageSnapshot>, String> {
        self.usage_cache
            .read()
            .map(|cache| cache.clone())
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))
    }

    /// Removes one stale process-local account-usage observation.
    /// 移除一个过期的进程内账号用量观测。
    pub fn remove_connection_usage(&self, connection_id: &str) -> Result<(), String> {
        self.usage_cache
            .write()
            .map_err(|_| String::from("The AI account-usage cache lock is poisoned."))?
            .remove(connection_id);
        Ok(())
    }

    /// Refreshes every configured connection that declares a supported usage query.
    /// 刷新每个声明支持用量查询的已配置连接。
    pub async fn refresh_supported_connection_usage(
        &self,
        configuration: &PersistedAiConfiguration,
    ) -> Result<(), String> {
        for connection in configuration.connection_graph.connections.values() {
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
        let (provider, entry, connection) =
            self.resolve_connection_metadata(configuration, connection_id)?;
        let catalog = self
            .catalogs
            .read()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?
            .get(connection_id)
            .cloned()
            .ok_or_else(|| {
                format!("Connection `{connection_id}` has no synchronized in-memory model catalog.")
            })?;
        if !catalog.models.iter().any(|model| model.id == model_id) {
            return Err(format!(
                "Model `{model_id}` is not present in connection `{connection_id}`."
            ));
        }
        // cache_key preserves exact credential isolation because endpoint availability may be account-scoped.
        // cache_key 保留精确凭据隔离，因为端点可用性可能受账号作用域影响。
        let cache_key = PersistedAiConfiguration::connection_model_key(connection_id, model_id);
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
        self.secondary_route_cache
            .write()
            .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?
            .insert(
                cache_key,
                SecondaryRouteCacheEntry {
                    routes: routes.clone(),
                    fetched_at: Instant::now(),
                },
            );
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
    /// Cached route options regardless of TTL, or an explicit cache-miss error.
    /// 返回不受 TTL 影响的缓存路由选项，或显式的缓存未命中错误。
    pub fn cached_secondary_routes(
        &self,
        connection_id: &str,
        model_id: &str,
    ) -> Result<Vec<SecondaryRouteOption>, String> {
        // cache_key exactly matches the discovery partition used by the selected credential and model.
        // cache_key 与所选凭据及模型使用的发现缓存分区完全一致。
        let cache_key = PersistedAiConfiguration::connection_model_key(connection_id, model_id);
        self.secondary_route_cache
            .read()
            .map_err(|_| String::from("The secondary-route cache lock is poisoned."))?
            .get(cache_key.as_str())
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
        let registry = self.effective_providers(configuration)?;
        let mut available = Vec::new();
        for connection in configuration.connection_graph.connections.values() {
            if configuration.connection_is_pending_validation(connection.id.as_str()) {
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
            for target in built_in_provider_tool_targets(provider.id.as_str()) {
                if target.contract.capability.is_ai_visible()
                    && target.service_entry_id == entry.id
                    && target
                        .credential_profile_ids
                        .contains(connection.credential_profile_id.as_str())
                {
                    available.push(AvailableProviderToolTarget {
                        connection_id: connection.id.clone(),
                        connection_display_name: connection.display_name.clone(),
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
        let selection = configuration
            .provider_tool_selections
            .get(&capability)
            .cloned()
            .ok_or_else(|| {
                format!("No provider tool target is selected for capability `{capability:?}`.")
            })?;
        self.execute_provider_tool_target_for_owner(
            configuration,
            capability,
            &selection,
            arguments,
            owner_session_id,
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
        if !capability.is_ai_visible() {
            return Err(format!(
                "Capability `{capability:?}` is internal inference and cannot execute as an AI-visible provider tool."
            ));
        }
        let resolved = self.resolve_provider_tool_target(configuration, capability, selection)?;
        let secret = self
            .resolve_fresh_execution_secret(
                resolved.connection.id.as_str(),
                resolved.connection.secret_reference.as_str(),
            )
            .await?;
        if crate::provider_tool_execution::is_provider_async_task_adapter(
            resolved.target.execution.adapter,
        ) {
            let execution_id = self
                .start_provider_async_task(
                    resolved,
                    selection.clone(),
                    arguments,
                    secret,
                    owner_session_id,
                )
                .await?;
            return self
                .await_provider_async_task(configuration, execution_id.as_str())
                .await;
        }
        self.execution_client
            .execute_provider_tool(&resolved.entry, &resolved.target, arguments, &secret)
            .await
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
        for record in records {
            // A completed task without a process-local result came from durable recovery and
            // must be observed once more to mint a fresh provider result without replaying work.
            // 缺少进程内结果的完成任务来自持久恢复，必须再次观测以获取新的供应商结果且不重放任务。
            let needs_completed_result_recovery =
                record.phase == ProviderAsyncTaskPhase::Completed && record.result.is_none();
            if record.phase.is_terminal() && !needs_completed_result_recovery {
                continue;
            }
            if record.phase == ProviderAsyncTaskPhase::CancellationRequested {
                if let Err(error) = self
                    .cancel_provider_task_internal(configuration, record.execution_id.as_str())
                    .await
                {
                    self.record_provider_task_observation_error(
                        record.execution_id.as_str(),
                        error,
                    )?;
                }
                continue;
            }
            if let Err(error) = self
                .poll_provider_async_task_once(configuration, record.execution_id.as_str())
                .await
            {
                self.record_provider_task_observation_error(record.execution_id.as_str(), error)?;
            }
        }
        self.provider_tasks.snapshot()
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
            } else if record.cancellation_supported {
                record.phase = ProviderAsyncTaskPhase::CancellationRequested;
                record.error = None;
            } else {
                // Non-cancellable provider work remains observable in the background until terminal.
                // 不支持取消的供应商任务继续由后台观测直至终态。
                record.error = Some(String::from(
                    "The owning conversation stopped, but this provider task has no cancellation endpoint; background observation continues.",
                ));
            }
            record.updated_at_unix_ms = current_unix_millis()?;
            self.provider_tasks.replace(record)?;
            changed = changed.saturating_add(1);
        }
        Ok(changed)
    }

    /// Starts one durable task after the route and credential are already frozen.
    /// 在路由与凭据已冻结后启动一个可持久化任务。
    async fn start_provider_async_task(
        &self,
        resolved: ResolvedProviderToolTarget,
        selection: ProviderToolTargetSelection,
        arguments: serde_json::Value,
        secret: CredentialExecutionSecret,
        owner_session_id: Option<String>,
    ) -> Result<String, String> {
        let now = current_unix_millis()?;
        let execution_id = Uuid::new_v4().to_string();
        let mut record = ProviderAsyncTaskRecord {
            execution_id: execution_id.clone(),
            owner_session_id,
            route: ProviderAsyncTaskRoute {
                provider_id: resolved.connection.provider_id.clone(),
                service_entry_id: resolved.entry.id.clone(),
                connection_id: resolved.connection.id.clone(),
                selection,
                adapter: resolved.target.execution.adapter,
            },
            provider_task_id: None,
            recovery: ProviderAsyncTaskRecoveryContext::None,
            phase: ProviderAsyncTaskPhase::Dispatching,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
            deadline_unix_ms: now.saturating_add(PROVIDER_ASYNC_TASK_TIMEOUT_MS),
            cancellation_supported: matches!(
                resolved.target.execution.adapter,
                crate::ProviderToolAdapterKind::AlibabaHappyHorseVideoGenerate
                    | crate::ProviderToolAdapterKind::AlibabaHappyHorseVideoEdit
                    | crate::ProviderToolAdapterKind::AlibabaWanVideoGenerate
            ),
            error: None,
            result: None,
        };
        self.provider_tasks.replace(record.clone())?;
        let started = match await_bounded_provider_task_operation(
            Duration::from_millis(PROVIDER_ASYNC_TASK_TIMEOUT_MS),
            "Provider asynchronous task start",
            crate::provider_tool_execution::start_provider_async_task(
                &self.execution_client.http,
                &resolved.entry,
                &resolved.target,
                arguments,
                &secret,
            ),
        )
        .await
        {
            Ok(started) => started,
            Err(error) => {
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
        record.cancellation_supported = started.cancellation_supported;
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
            if current_unix_millis()? >= record.deadline_unix_ms {
                let mut timed_out = record;
                timed_out.phase = ProviderAsyncTaskPhase::Failed;
                timed_out.updated_at_unix_ms = current_unix_millis()?;
                timed_out.error = Some(String::from(
                    "Provider asynchronous task exceeded its 30 minute deadline.",
                ));
                self.provider_tasks.replace(timed_out.clone())?;
                return Err(timed_out.error.expect("timeout diagnostic is present"));
            }
            tokio::time::sleep(PROVIDER_ASYNC_TASK_POLL_INTERVAL).await;
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
        let provider_task_id = record.provider_task_id.clone().ok_or_else(|| {
            format!(
                "Provider task execution `{execution_id}` has no confirmed provider task identifier."
            )
        })?;
        let resolved = self.resolve_recovered_provider_task(configuration, &record)?;
        let secret = self
            .resolve_fresh_execution_secret(
                resolved.connection.id.as_str(),
                resolved.connection.secret_reference.as_str(),
            )
            .await?;
        match await_bounded_provider_task_operation(
            PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT,
            "Provider asynchronous task observation",
            crate::provider_tool_execution::poll_provider_async_task(
                &self.execution_client.http,
                &resolved.entry,
                &resolved.target,
                provider_task_id.as_str(),
                &record.recovery,
                &secret,
            ),
        )
        .await?
        {
            ProviderAsyncTaskPoll::Pending(phase) => {
                record.phase = phase;
                record.error = None;
            }
            ProviderAsyncTaskPoll::Completed(result) => {
                record.phase = ProviderAsyncTaskPhase::Completed;
                record.result = Some(result);
                record.error = None;
            }
            ProviderAsyncTaskPoll::Failed(error) => {
                record.phase = ProviderAsyncTaskPhase::Failed;
                record.error = Some(error);
            }
            ProviderAsyncTaskPoll::Cancelled => {
                record.phase = ProviderAsyncTaskPhase::Cancelled;
                record.error = Some(String::from("Provider task was cancelled."));
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
        let secret = self
            .resolve_fresh_execution_secret(
                resolved.connection.id.as_str(),
                resolved.connection.secret_reference.as_str(),
            )
            .await?;
        await_bounded_provider_task_operation(
            PROVIDER_ASYNC_TASK_OBSERVATION_TIMEOUT,
            "Provider asynchronous task cancellation",
            crate::provider_tool_execution::cancel_provider_async_task(
                &self.execution_client.http,
                &resolved.entry,
                &resolved.target,
                provider_task_id.as_str(),
                &secret,
            ),
        )
        .await?;
        record.phase = ProviderAsyncTaskPhase::Cancelled;
        record.updated_at_unix_ms = current_unix_millis()?;
        record.error = Some(String::from("Provider cancellation was confirmed."));
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
        let registry = self.effective_providers(configuration)?;
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
        let target = built_in_provider_tool_targets(provider.id.as_str())
            .into_iter()
            .find(|target| target.target_id == record.route.selection.target_id)
            .ok_or_else(|| {
                format!(
                    "Provider task target `{}` is not registered.",
                    record.route.selection.target_id
                )
            })?;
        let target =
            crate::connection_endpoint::apply_connection_provider_tool_origin(&connection, target);
        if target.service_entry_id != entry.id
            || target.execution.adapter != record.route.adapter
            || !target
                .credential_profile_ids
                .contains(connection.credential_profile_id.as_str())
        {
            return Err(String::from(
                "Provider task target no longer matches its immutable route affinity.",
            ));
        }
        Ok(ResolvedProviderToolTarget {
            connection,
            entry,
            target,
        })
    }

    /// Retains one transient poll or cancellation diagnostic without making it terminal.
    /// 保留一个瞬时轮询或取消诊断，但不将其设为终态。
    fn record_provider_task_observation_error(
        &self,
        execution_id: &str,
        error: String,
    ) -> Result<(), String> {
        let mut record = self.provider_tasks.require(execution_id)?;
        if !record.phase.is_terminal() {
            record.updated_at_unix_ms = current_unix_millis()?;
            record.error = Some(error);
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
        configuration: &PersistedAiConfiguration,
        capability: ProviderToolCapabilityKind,
        selection: &ProviderToolTargetSelection,
    ) -> Result<ResolvedProviderToolTarget, String> {
        if !capability.is_ai_visible() {
            return Err(format!(
                "Capability `{capability:?}` is internal inference and cannot resolve as an AI-visible provider tool."
            ));
        }
        let registry = self.effective_providers(configuration)?;
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
        let target = built_in_provider_tool_targets(provider.id.as_str())
            .into_iter()
            .find(|target| target.target_id == selection.target_id)
            .ok_or_else(|| {
                format!(
                    "Provider tool target `{}` is not registered.",
                    selection.target_id
                )
            })?;
        let target =
            crate::connection_endpoint::apply_connection_provider_tool_origin(&connection, target);
        if target.contract.capability != capability
            || target.service_entry_id != entry.id
            || !target
                .credential_profile_ids
                .contains(connection.credential_profile_id.as_str())
        {
            return Err(format!(
                "Provider tool target `{}` is not eligible for connection `{}`.",
                target.target_id, connection.id
            ));
        }
        Ok(ResolvedProviderToolTarget {
            connection,
            entry,
            target,
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
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .cloned()
            .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
        let providers = self.effective_providers(configuration)?;
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
        let secret = self
            .resolve_fresh_execution_secret(
                selection.route.connection.id.as_str(),
                selection.route.connection.secret_reference.as_str(),
            )
            .await?;
        Ok(ResolvedPublicAiExecution {
            route: selection.route,
            model: selection.model,
            secret,
            overrides: selection.overrides,
            client: self.execution_client.clone(),
        })
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
        let current = self.secrets.read_secret(secret_reference).await?;
        if !current.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS) {
            return current.execution_secret();
        }
        let refresh_lock = {
            let mut locks = self
                .refresh_locks
                .lock()
                .map_err(|_| String::from("The AI credential refresh lock is poisoned."))?;
            Arc::clone(
                locks
                    .entry(connection_id.to_string())
                    .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
            )
        };
        let _guard = refresh_lock.lock().await;
        let current = self.secrets.read_secret(secret_reference).await?;
        if !current.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS) {
            return current.execution_secret();
        }
        let material = self.oauth.refresh(&current.material).await?;
        let refreshed = current.advance_with_material(material)?;
        if let Err(replace_error) = self
            .secrets
            .replace_secret(secret_reference, current.revision, &refreshed)
            .await
        {
            let concurrent = self.secrets.read_secret(secret_reference).await?;
            if concurrent.revision > current.revision
                && !concurrent.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS)
            {
                return concurrent.execution_secret();
            }
            return Err(replace_error);
        }
        refreshed.execution_secret()
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
        configuration: &PersistedAiConfiguration,
    ) -> Result<ResolvedPublicAiSelection, String> {
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
        configuration: &PersistedAiConfiguration,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiSelection, String> {
        let catalogs = self
            .catalogs
            .read()
            .map_err(|_| String::from("The AI model catalog lock is poisoned."))?;
        let mut eligible_catalogs = catalogs.snapshot();
        eligible_catalogs.retain(|connection_id, _| {
            !configuration.connection_is_pending_validation(connection_id.as_str())
        });
        let eligible_catalogs = RuntimeConnectionCatalogs::from_snapshot(eligible_catalogs);
        let connection = configuration
            .connection_graph
            .resolve_connection(provider_id, model_id, &eligible_catalogs)?
            .clone();
        drop(catalogs);
        self.resolve_selection_for_connection(
            configuration,
            connection,
            provider_id,
            model_id,
            None,
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
    /// A validated immutable selection, or an error that allows the caller to apply degradation.
    /// 返回已校验的不可变选择；错误允许调用方执行降级。
    pub fn resolve_connection_model_selection(
        &self,
        configuration: &PersistedAiConfiguration,
        connection_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedPublicAiSelection, String> {
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
        )?;
        let usage_scopes = selection
            .route
            .entry
            .effective_usage_scopes(&selection.route.connection)?;
        if !usage_scopes.contains(&inference_route.usage_scope) {
            return Err(format!(
                "Credential connection `{}` does not authorize usage scope `{:?}`.",
                inference_route.connection_id, inference_route.usage_scope
            ));
        }
        match (&inference_route.binding, inference_route.operation) {
            (
                InferenceRouteBinding::Protocol {
                    protocol_profile_id,
                },
                AiOperation::ConversationRespond,
            ) if protocol_profile_id == &selection.route.connection.selected_protocol => {}
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
                selection.catalog_revision.as_str(),
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
                selection.catalog_revision.as_str(),
                &secret,
            )
            .await
    }

    /// Executes one resolved restricted internal LLM request with fresh credential material.
    /// 使用新鲜凭据材料执行一项已解析的受限内部 LLM 请求。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the exact pre-authorized route, while `request` contains no tools,
    /// resources, continuation, or account-routing instructions.
    /// `selection` 是精确预授权路由，`request` 不包含工具、资源、延续信息或账号路由指令。
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
        if request.operation != AiOperation::ConversationRespond
            || !request.tools.is_empty()
            || !request.native_tools.is_empty()
            || !request.resources.is_empty()
            || request.continuation.is_some()
        {
            return Err(crate::execution::pre_dispatch_failure(
                "Restricted internal LLM execution rejects tools, resources, continuation, and non-conversation operations.",
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
        Ok(self
            .execution_client
            .execute(selection.route.clone(), request, secret, execution_id))
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
    ) -> Result<ResolvedPublicAiSelection, String> {
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            return Err(format!(
                "Connection `{}` is pending explicit model validation and cannot execute normal requests.",
                connection.id
            ));
        }
        if let Some(operation) = required_internal_operation {
            if !configuration.internal_model_is_enabled(provider_id, model_id, operation) {
                return Err(format!(
                    "Model `{model_id}` is disabled for internal operation `{}` on provider `{provider_id}`.",
                    operation.as_str()
                ));
            }
        } else if !configuration.model_is_enabled(provider_id, model_id) {
            return Err(format!(
                "Model `{model_id}` is disabled for provider `{provider_id}`."
            ));
        }
        let (model, catalog_revision) = {
            let catalogs = self
                .catalogs
                .read()
                .map_err(|_| String::from("The AI model catalog lock is poisoned."))?;
            let catalog = catalogs.get(connection.id.as_str()).ok_or_else(|| {
                format!(
                    "No in-memory catalog exists for connection `{}`.",
                    connection.id
                )
            })?;
            let model = catalog
                .models
                .iter()
                .find(|model| model.id == model_id)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "Model `{}` is not present in the in-memory catalog for connection `{}`.",
                        model_id, connection.id
                    )
                })?;
            (
                model,
                format!("{}:{}", catalog.connection_id, catalog.generation),
            )
        };
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
        let provider = self
            .effective_providers(configuration)?
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
        let override_key = PersistedAiConfiguration::provider_model_protocol_key(
            connection.provider_id.as_str(),
            model.id.as_str(),
            connection.selected_protocol.as_str(),
        );
        let mut overrides = configuration
            .model_overrides
            .get(override_key.as_str())
            .cloned()
            .unwrap_or_default();
        if overrides.prompt_cache.is_none() {
            overrides.prompt_cache = connection
                .prompt_cache_by_protocol
                .get(connection.selected_protocol.as_str())
                .cloned();
        }
        apply_model_parameter_defaults(
            &model,
            connection.selected_protocol.as_str(),
            &mut overrides,
        )?;
        validate_model_overrides(&model, connection.selected_protocol.as_str(), &overrides)?;
        let secondary_route_key = PersistedAiConfiguration::connection_model_key(
            connection.id.as_str(),
            model.id.as_str(),
        );
        let secondary_route = configuration
            .secondary_routes
            .get(secondary_route_key.as_str())
            .cloned()
            .unwrap_or(SecondaryRouteSelection::Automatic);
        let mut route =
            ProviderExecutionRoute::resolve(provider, entry, connection.clone(), secondary_route)
                .map_err(|failure| failure.error.message.clone())?;
        route.custom_model_projection = configuration
            .custom_model(provider_id, model_id)
            .map(|definition| definition.request_projection.clone());
        Ok(ResolvedPublicAiSelection {
            route,
            model,
            overrides,
            catalog_revision,
        })
    }
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
/// An empty credential-scoped baseline when manual definitions exist; otherwise `None`.
/// 存在手工定义时返回空的凭据作用域基线，否则返回 `None`。
fn manual_custom_catalog(
    configuration: &PersistedAiConfiguration,
    connection: &CredentialConnection,
    generation: u64,
) -> Option<ConnectionModelCatalog> {
    configuration
        .custom_models
        .get(connection.provider_id.as_str())
        .filter(|definitions| !definitions.is_empty())
        .map(|_| ConnectionModelCatalog {
            connection_id: connection.id.clone(),
            models: Vec::new(),
            generation,
            page_count: 0,
            discovery_schema_revision: String::from("manual.models.v1"),
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
    apply_model_catalog_overlays_with_account_plan(configuration, provider_id, "", None, catalog)
}

/// Applies catalog overlays together with the exact account-level Kimi entitlement observation.
/// 应用目录覆盖，并叠加精确账号级 Kimi 权益观测。
fn apply_model_catalog_overlays_with_account_plan(
    configuration: &PersistedAiConfiguration,
    provider_id: &str,
    service_entry_id: &str,
    account_plan_name: Option<&str>,
    catalog: &mut ConnectionModelCatalog,
) -> Result<(), String> {
    let custom_provider = provider_id.starts_with("custom:");
    let custom_defaults =
        custom_provider.then(|| configuration.custom_provider_model_defaults(provider_id));
    if let Some(defaults) = custom_defaults.as_ref() {
        defaults.validate()?;
    }
    let source_model_ids = catalog
        .models
        .iter()
        .map(|model| model.id.clone())
        .collect::<BTreeSet<_>>();
    // An observed empty provider page is authoritative; only the explicit manual baseline lacks discovery evidence.
    // 供应商明确返回的空分页仍具权威性；只有显式手工基线不包含发现证据。
    let discovery_is_authoritative =
        custom_provider && catalog.discovery_schema_revision != "manual.models.v1";
    let mut models = catalog
        .models
        .drain(..)
        .map(|mut model| {
            if custom_provider {
                apply_custom_model_defaults(
                    configuration,
                    provider_id,
                    model.id.as_str(),
                    custom_defaults
                        .as_ref()
                        .expect("custom defaults must exist"),
                    &model.capability_evidence,
                    &mut model.capabilities,
                );
            } else {
                crate::apply_common_model_context_default(
                    model.id.as_str(),
                    &mut model.capabilities,
                );
            }
            configuration.apply_model_capability_overrides(
                provider_id,
                model.id.as_str(),
                &mut model.capabilities,
            );
            (model.id.clone(), model)
        })
        .collect::<BTreeMap<_, _>>();
    if let Some(custom_models) = configuration.custom_models.get(provider_id) {
        for definition in custom_models.values() {
            definition.validate()?;
            let availability = configuration.custom_model_connection_enabled_override(
                catalog.connection_id.as_str(),
                definition.id.as_str(),
            );
            if availability == Some(false)
                || (discovery_is_authoritative
                    && !source_model_ids.contains(definition.id.as_str())
                    && availability != Some(true))
            {
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
                continue;
            }
            let mut capabilities = definition.capabilities.clone();
            let capability_evidence = manual_model_capability_evidence(&capabilities);
            if custom_provider {
                apply_custom_model_defaults(
                    configuration,
                    provider_id,
                    definition.id.as_str(),
                    custom_defaults
                        .as_ref()
                        .expect("custom defaults must exist"),
                    &capability_evidence,
                    &mut capabilities,
                );
            } else {
                crate::apply_common_model_context_default(
                    definition.id.as_str(),
                    &mut capabilities,
                );
            }
            configuration.apply_model_capability_overrides(
                provider_id,
                definition.id.as_str(),
                &mut capabilities,
            );
            models.insert(
                definition.id.clone(),
                ConnectionModel {
                    id: definition.id.clone(),
                    display_name: definition.display_name.clone(),
                    capabilities,
                    capability_evidence,
                },
            );
        }
    }
    // Account access is evaluated after support membership and capability resolution so it can
    // hide every fixed, discovered, or manually admitted model without mutating source evidence.
    // 账号访问策略在支持成员关系与能力解析后求值，因此可以隐藏任意固定、发现或手工准入模型，
    // 同时不会修改原始证据。
    models.retain(|model_id, _| {
        configuration.connection_model_is_enabled(catalog.connection_id.as_str(), model_id)
    });
    if provider_id == "kimi" && service_entry_id == "coding-plan" {
        // Kimi Coding Plan access is applied only after a confirmed usage observation so a transient lookup failure does not invent a tier.
        // Kimi Coding Plan 仅在获得已确认用量观测后应用访问矩阵，避免暂时查询失败臆造套餐等级。
        models.retain(|model_id, _| {
            crate::kimi_coding_plan_model_is_available(account_plan_name, model_id)
        });
    }
    catalog.models = models.into_values().collect();
    Ok(())
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
    if declared.tool_calling != vulcan_ai_contract::CapabilitySupport::Unknown {
        resolved.tool_calling = declared.tool_calling;
    }
    if declared.parallel_tool_calling != vulcan_ai_contract::CapabilitySupport::Unknown {
        resolved.parallel_tool_calling = declared.parallel_tool_calling;
    }
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
    if declared.tool_calling != vulcan_ai_contract::CapabilitySupport::Unknown
        || declared.parallel_tool_calling != vulcan_ai_contract::CapabilitySupport::Unknown
        || declared.reasoning != vulcan_ai_contract::CapabilitySupport::Unknown
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

/// Resolves custom-provider template and provider defaults without overriding credential evidence.
/// 在不覆盖凭据证据的前提下解析自定义供应商模板与供应商默认值。
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
    set_conversation_operation(capabilities, supports_text_input && supports_text_output);
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
        supports_text_input: Some(supports_text),
        supports_text_output: Some(supports_text),
    }
}

/// Adds or removes conversation capability according to the resolved text contract.
/// 根据解析后的文本契约添加或移除会话能力。
///
/// # Parameters
/// 参数
/// `capabilities` is mutated in place and `supported` is the final bidirectional-text result.
/// `capabilities` 会被原地修改，`supported` 是最终双向文本支持结果。
fn set_conversation_operation(capabilities: &mut ModelCapabilities, supported: bool) {
    capabilities
        .operations
        .retain(|operation| *operation != AiOperation::ConversationRespond);
    if supported {
        capabilities
            .operations
            .push(AiOperation::ConversationRespond);
        capabilities.operations.sort();
        capabilities.operations.dedup();
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
            vulcan_ai_contract::ModelParameterDefault::RequiredValue => {
                return Err(format!(
                    "Model parameter `{}` requires an explicit value for model `{}`.",
                    definition.id, model.id
                ));
            }
            vulcan_ai_contract::ModelParameterDefault::Omit
            | vulcan_ai_contract::ModelParameterDefault::ProviderDefault => {}
        }
    }
    Ok(())
}

/// Carries immutable public selection metadata without credential material.
/// 承载不含凭据材料的不可变公共选择元数据。
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
}

/// Carries all immutable public execution inputs resolved before controller dispatch.
/// 承载控制器分派前解析完成的全部不可变公共执行输入。
pub struct ResolvedPublicAiExecution {
    /// Exact provider execution route.
    /// 精确供应商执行路由。
    pub route: ProviderExecutionRoute,
    /// Exact credential-scoped model snapshot.
    /// 精确凭据作用域模型快照。
    pub model: crate::ConnectionModel,
    /// Exact credential secret and frozen secure-store revision.
    /// 精确凭据秘密及冻结的安全存储修订号。
    pub secret: CredentialExecutionSecret,
    /// Local model execution overrides.
    /// 本地模型执行覆盖。
    pub overrides: crate::ModelExecutionOverrides,
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
    /// Exact model- or service-specific target contract.
    /// 精确的模型或服务专属目标契约。
    pub target: ProviderToolTarget,
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
        parse_minimax_openai_model_catalog, parse_openrouter_model_catalog,
    };
    use std::collections::BTreeSet;
    use vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID;

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
            verified_protocols: BTreeSet::from([OPENAI_CHAT_PROFILE_ID.to_string()]),
            selected_protocol: OPENAI_CHAT_PROFILE_ID.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        }
    }

    /// Builds one deterministic credential-scoped catalog.
    /// 构建一个确定性的凭据作用域目录。
    fn catalog(connection_id: &str) -> ConnectionModelCatalog {
        parse_openrouter_model_catalog(
            connection_id,
            1,
            r#"{"data":[{"id":"vendor/model","name":"Model","context_length":8192,"supported_parameters":["tools","temperature"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
        )
        .expect("documented model fixture should parse")
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
            .add_connection(runtime.providers(), connection("one"))
            .expect("first connection should register");
        configuration
            .connection_graph
            .add_connection(runtime.providers(), connection("two"))
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
        configuration.set_model_enabled("openrouter", "vendor/model", true);
        (runtime, configuration)
    }

    /// Verifies rebuilding user overlays never mutates the immutable synchronized catalog snapshot.
    /// 验证重建用户覆盖绝不会修改不可变的已同步目录快照。
    #[test]
    fn model_overlay_rebuild_preserves_source_catalog() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        configuration
            .connection_graph
            .add_connection(runtime.providers(), connection("one"))
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
            PersistedAiConfiguration::provider_model_key("openrouter", "vendor/model"),
            crate::ModelCapabilityOverrides {
                context_limit: Some(4_096),
                output_limit: None,
                input_resources: Some(vec![vulcan_ai_contract::ResourceKind::Image]),
                output_resources: None,
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
        configuration.set_connection_model_enabled("one", "vendor/model", false);

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
            vec!["vendor/model"]
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
            vec!["vendor/model"]
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
            .add_connection(runtime.providers(), connection("one"))
            .expect("connection should register");
        install_source_catalog(&runtime, catalog("one"));

        for (overrides, expected_error) in [
            (
                crate::ModelCapabilityOverrides {
                    output_limit: Some(0),
                    ..crate::ModelCapabilityOverrides::default()
                },
                "effective output limit of zero",
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
                    "vendor/model",
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
                "vendor/model",
                Some(&crate::ModelCapabilityOverrides {
                    context_limit: Some(8_192),
                    output_limit: Some(4_096),
                    ..crate::ModelCapabilityOverrides::default()
                }),
            )
            .expect("valid final limits should be accepted");
    }

    /// Verifies a provider-wide difference must remain valid for every credential source.
    /// 验证供应商级差异必须对每个凭据源都保持有效。
    #[test]
    fn capability_override_candidate_validates_every_account() {
        let (runtime, configuration) = two_account_runtime();
        install_source_catalog(&runtime, catalog("one"));
        install_source_catalog(
            &runtime,
            parse_openrouter_model_catalog(
                "two",
                1,
                r#"{"data":[{"id":"vendor/model","name":"Model","context_length":2048,"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":1024}}]}"#,
            )
            .expect("second account source should parse"),
        );

        let error = runtime
            .validate_model_capability_override_candidate(
                &configuration,
                "openrouter",
                "vendor/model",
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
            .add_connection(runtime.providers(), connection("one"))
            .expect("connection should register");
        install_source_catalog(&runtime, catalog("one"));
        configuration.model_capability_overrides.insert(
            PersistedAiConfiguration::provider_model_key("openrouter", "vendor/model"),
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
        configuration
            .connection_graph
            .add_connection(runtime.providers(), connection("one"))
            .expect("connection should register");
        configuration
            .custom_models
            .entry(String::from("openrouter"))
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

    /// Verifies removing a manual-only definition removes the model from its exact connection.
    /// 验证移除仅手工定义后模型会从其精确连接消失。
    #[test]
    fn connection_catalog_membership_tracks_manual_definition_removal() {
        let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        let mut configuration = PersistedAiConfiguration::default();
        let mut custom_connection = connection("custom-account");
        custom_connection.provider_id = String::from("custom:test");
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
        let mut custom_connection = connection("custom-account");
        custom_connection.provider_id = String::from("custom:test");
        configuration
            .connection_graph
            .connections
            .insert(custom_connection.id.clone(), custom_connection);
        let source = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"vendor/model"}]}"#,
        )
        .expect("compatible source catalog should parse");
        install_source_catalog(&runtime, source);
        configuration
            .custom_models
            .entry(String::from("custom:test"))
            .or_default()
            .insert(
                String::from("vendor/model"),
                crate::CustomModelDefinition {
                    id: String::from("vendor/model"),
                    display_name: String::from("Reviewed Model"),
                    enabled: true,
                    capabilities: ModelCapabilities::default(),
                    request_projection: crate::CustomModelRequestProjection::default(),
                },
            );

        configuration.custom_models.remove("custom:test");
        assert!(
            runtime
                .connection_catalog_contains_model(
                    &configuration,
                    "custom-account",
                    "vendor/model",
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
        assert!(
            model
                .capabilities
                .operations
                .contains(&AiOperation::ConversationRespond)
        );
    }

    /// Verifies explicit credential fields outrank templates and provider defaults field by field.
    /// 验证凭据明确字段会逐字段优先于模板与供应商默认值。
    #[test]
    fn explicit_custom_catalog_evidence_outranks_template_and_defaults() {
        let mut configuration = PersistedAiConfiguration::default();
        configuration.custom_provider_model_defaults.insert(
            String::from("custom:test"),
            crate::CustomProviderModelDefaults {
                context_limit: 200_000,
                output_limit: 32_768,
                input_resources: vec![vulcan_ai_contract::ResourceKind::Image],
                output_resources: vec![vulcan_ai_contract::ResourceKind::Image],
            },
        );
        configuration.custom_model_template_decisions.insert(
            PersistedAiConfiguration::provider_model_key("custom:test", "private/embedding"),
            crate::CustomModelTemplateDecision::Applied {
                selection: crate::CustomModelTemplateSelection {
                    template_key: String::from("openai/text-embedding-3-small"),
                    template_revision: String::from(crate::CUSTOM_MODEL_TEMPLATE_REVISION),
                    context_limit: 100_000,
                    supports_text_input: true,
                    supports_text_output: false,
                    input_resources: vec![vulcan_ai_contract::ResourceKind::File],
                    output_resources: Vec::new(),
                },
            },
        );
        let mut catalog = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"private/embedding","context_window":64000,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["embedding"]}]}"#,
        )
        .expect("explicit compatible catalog should parse");

        apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
            .expect("custom evidence should resolve");

        let model = catalog.models.first().expect("model should remain visible");
        assert_eq!(model.capabilities.context_limit, Some(64_000));
        assert_eq!(model.capabilities.output_limit, Some(4_096));
        assert!(model.capabilities.input_resources.is_empty());
        assert!(model.capabilities.output_resources.is_empty());
        assert!(
            !model
                .capabilities
                .operations
                .contains(&AiOperation::ConversationRespond)
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
                output_resources: Some(vec![vulcan_ai_contract::ResourceKind::Image]),
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
        assert_eq!(
            model.capabilities.output_resources,
            vec![vulcan_ai_contract::ResourceKind::Image]
        );
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
                        output_resources: vec![vulcan_ai_contract::ResourceKind::Audio],
                        tool_calling: vulcan_ai_contract::CapabilitySupport::Native,
                        reasoning: vulcan_ai_contract::CapabilitySupport::Native,
                        reasoning_control:
                            vulcan_ai_contract::ReasoningControlMode::EffortSelection,
                        reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::High],
                        model_parameters: vec![vulcan_ai_contract::ModelParameterDefinition {
                            id: String::from("vendor.flag"),
                            display_name: String::from("Vendor flag"),
                            description: String::from("Controls one verified vendor flag."),
                            schema: vulcan_ai_contract::ModelParameterSchema::Boolean,
                            default: vulcan_ai_contract::ModelParameterDefault::ProviderDefault,
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
            model.capabilities.tool_calling,
            vulcan_ai_contract::CapabilitySupport::Native
        );
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

    /// Verifies an authoritative non-empty account catalog hides absent manual models until forced.
    /// 验证权威非空账号目录会隐藏缺失的手工模型，直至用户显式强制启用。
    #[test]
    fn authoritative_custom_catalog_requires_force_enable_for_absent_manual_model() {
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
                },
            );
        let source = crate::parse_openai_model_catalog(
            "custom-account",
            1,
            r#"{"data":[{"id":"discovered/chat"}]}"#,
        )
        .expect("authoritative catalog should parse");
        let mut hidden = source.clone();
        apply_model_catalog_overlays(&configuration, "custom:test", &mut hidden)
            .expect("catalog should resolve");
        assert!(hidden.models.iter().all(|model| model.id != "manual/chat"));

        configuration
            .custom_model_connection_enabled_overrides
            .insert(
                PersistedAiConfiguration::connection_model_key("custom-account", "manual/chat"),
                true,
            );
        let mut forced = source;
        apply_model_catalog_overlays(&configuration, "custom:test", &mut forced)
            .expect("forced catalog should resolve");
        assert!(forced.models.iter().any(|model| model.id == "manual/chat"));
    }

    /// Verifies an authoritative empty provider catalog does not degrade into manual-only mode.
    /// 验证权威空供应商目录不会降级为仅手工模式。
    #[test]
    fn authoritative_empty_custom_catalog_hides_manual_models_until_forced() {
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
                },
            );
        let source = crate::parse_openai_model_catalog("custom-account", 1, r#"{"data":[]}"#)
            .expect("authoritative empty catalog should parse");
        let mut hidden = source.clone();
        apply_model_catalog_overlays(&configuration, "custom:test", &mut hidden)
            .expect("empty catalog should remain authoritative");
        assert!(hidden.models.is_empty());

        configuration
            .custom_model_connection_enabled_overrides
            .insert(
                PersistedAiConfiguration::connection_model_key("custom-account", "manual/chat"),
                true,
            );
        let mut forced = source;
        apply_model_catalog_overlays(&configuration, "custom:test", &mut forced)
            .expect("forced empty catalog should resolve");
        assert!(forced.models.iter().any(|model| model.id == "manual/chat"));
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

    /// Verifies provider default selection is used until a model-specific account is remembered.
    /// 验证在记住模型专属账号前使用供应商默认账号。
    #[test]
    fn model_selection_prefers_remembered_account_over_provider_default() {
        let (runtime, mut configuration) = two_account_runtime();
        let initial = runtime
            .resolve_model_selection(&configuration, "openrouter", "vendor/model")
            .expect("provider default should resolve");
        assert_eq!(initial.route.connection.id, "one");

        configuration
            .connection_graph
            .remember_model_connection("openrouter", "vendor/model", "two")
            .expect("model account should be remembered");
        let remembered = runtime
            .resolve_model_selection(&configuration, "openrouter", "vendor/model")
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
        let selection = runtime
            .resolve_model_selection(&configuration, "openrouter", "vendor/model")
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
            .remember_model_connection("openrouter", "vendor/model", "one")
            .expect("model preference should be recorded");
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("vendor/model"),
            connection_id: String::from("one"),
        });

        configuration
            .set_connection_pending_validation("one", true)
            .expect("configured connection should enter pending state");

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
            .resolve_model_selection(&configuration, "openrouter", "vendor/model")
            .expect("eligible account should replace the pending preference");
        assert_eq!(fallback.route.connection.id, "two");
        let Err(exact_error) = runtime.resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "vendor/model",
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
            .add_connection(runtime.providers(), connection("one"))
            .expect("connection should register");
        configuration.set_model_enabled("openrouter", "vendor/chat", true);
        configuration.set_model_enabled("openrouter", "vendor/embedding", true);
        runtime
            .replace_catalog(ConnectionModelCatalog {
                connection_id: String::from("one"),
                models: vec![
                    ConnectionModel {
                        id: String::from("vendor/chat"),
                        display_name: String::from("Chat"),
                        capabilities: ModelCapabilities {
                            operations: vec![AiOperation::ConversationRespond],
                            ..ModelCapabilities::default()
                        },
                        capability_evidence: crate::ModelCapabilityEvidence::default(),
                    },
                    ConnectionModel {
                        id: String::from("vendor/embedding"),
                        display_name: String::from("Embedding"),
                        capabilities: ModelCapabilities {
                            operations: vec![AiOperation::EmbeddingCreate],
                            ..ModelCapabilities::default()
                        },
                        capability_evidence: crate::ModelCapabilityEvidence::default(),
                    },
                ],
                generation: 1,
                page_count: 1,
                discovery_schema_revision: String::from("test.mixed.models.v1"),
            })
            .expect("mixed catalog should install");

        assert_eq!(
            runtime
                .available_model_selectors(&configuration)
                .expect("selectors should resolve"),
            BTreeSet::from([String::from("openrouter/vendor/chat")])
        );
        let Err(error) = runtime.resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "vendor/embedding",
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
            .remember_model_connection("openrouter", "vendor/model", "two")
            .expect("model account should be remembered");
        configuration.connection_graph.connections.remove("two");

        let selection = runtime
            .resolve_model_selection(&configuration, "openrouter", "vendor/model")
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
            .remember_model_connection("openrouter", "vendor/model", "one")
            .expect("model account should be remembered");

        let selection = runtime
            .resolve_connection_model_selection(&configuration, "two", "openrouter", "vendor/model")
            .expect("exact session account should resolve");
        assert_eq!(selection.route.connection.id, "two");
    }

    /// Verifies cached secondary routes are account-scoped, expire after six hours, and may be bypassed explicitly.
    /// 验证二级路由缓存按账号隔离、六小时后过期，并可被显式绕过。
    #[tokio::test]
    async fn secondary_route_cache_honors_scope_ttl_and_forced_refresh() {
        let (runtime, configuration) = two_account_runtime();
        // cache_key targets only the second credential and its exact model.
        // cache_key 仅指向第二个凭据及其精确模型。
        let cache_key = PersistedAiConfiguration::connection_model_key("two", "vendor/model");
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
                    routes: vec![cached_route.clone()],
                    fetched_at: Instant::now(),
                },
            );

        let cached = runtime
            .discover_secondary_routes(&configuration, "two", "vendor/model", false)
            .await
            .expect("fresh cache should avoid credential access");
        assert_eq!(cached, vec![cached_route.clone()]);
        assert_eq!(
            runtime
                .cached_secondary_routes("two", "vendor/model")
                .expect("selection validation should use the populated cache"),
            vec![cached_route.clone()]
        );
        assert!(
            runtime
                .cached_secondary_routes("one", "vendor/model")
                .is_err(),
            "selection validation must not reuse another credential's cache partition"
        );
        assert!(
            runtime
                .discover_secondary_routes(&configuration, "two", "vendor/model", true)
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
                .discover_secondary_routes(&configuration, "two", "vendor/model", false)
                .await
                .is_err(),
            "expired cache should reach the unavailable secret boundary"
        );
        assert_eq!(
            runtime
                .cached_secondary_routes("two", "vendor/model")
                .expect(
                    "selection validation should not issue an endpoint request for expired cache"
                ),
            vec![cached_route]
        );
    }

    /// Verifies one active selection remains bound to its exact account snapshot.
    /// 验证活动选择始终绑定其精确账号快照。
    #[test]
    fn active_selection_uses_exact_configured_connection() {
        let (runtime, mut configuration) = two_account_runtime();
        configuration.active = Some(ActiveAiModelSelection {
            provider_id: String::from("openrouter"),
            model_id: String::from("vendor/model"),
            connection_id: String::from("two"),
        });
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
            "vendor/model",
            OPENAI_CHAT_PROFILE_ID,
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
                model_id: String::from("vendor/model"),
                connection_id: connection_id.to_string(),
            });
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
        let model = parse_minimax_openai_model_catalog(
            "connection",
            1,
            r#"{"object":"list","data":[{"id":"MiniMax-M2.7","object":"model","created":1,"owned_by":"minimax"}]}"#,
        )
        .expect("MiniMax model fixture should parse")
        .models
        .remove(0);
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
}
