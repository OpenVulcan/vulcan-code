//! Provider runtime composition and shared process-local publication state.
//! 供应商运行时组装与共享进程内发布状态。

use crate::provider_task::ProviderTaskRecoveryStore;
use crate::{
    CredentialConnection, HttpProviderMetadataClient, OAuthAuthorizationRuntime,
    PersistedAiConfiguration, ProviderExecutionClient, ProviderRegistry, RuntimeConnectionCatalogs,
};
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex as StdMutex, RwLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;
use vulcan_ai_contract::AccountUsageSnapshot;
use vulcan_llm_core::LlmProtocolRuntime;

mod activation;
mod async_tasks;
mod catalog;
mod catalog_policy;
mod catalog_sources;
mod catalog_views;
mod conversation_media;
mod credential_snapshot;
mod credentials;
mod media;
mod model_execution;
mod model_overrides;
mod secondary_routes;
mod selection;
mod session_leases;
mod tool_diagnostics;
mod tool_execution;
mod tool_targets;
mod usage;

use secondary_routes::SecondaryRouteCacheEntry;
pub use session_leases::{
    SessionSelectionLeaseAbsenceGuard, SessionSelectionLeaseReconciliationGuard,
};

pub use catalog::{
    CatalogDiscoveryFailureKind, CatalogDiscoveryState, CatalogDiscoveryStatus,
    IsolatedConnectionCatalogObservation, StagedConnectionCatalogObservation,
};

pub use credential_snapshot::CredentialSecretSnapshot;

pub use activation::{
    PreparedSystemManifestReplacement, ProviderCatalogActivationTransaction,
    ProviderStagedManifestCatalogMutation,
};
pub use catalog_views::{
    DeclaredProviderToolTemplateModel, ProviderManifestRuntimeSnapshot,
    PublicAiRuntimeSettingsCatalogSnapshot,
};

pub use catalog_policy::{
    apply_host_default_reasoning_policy, model_capability_limit_issue, model_supports_reasoning,
    model_supports_text_conversation,
};

pub use credentials::{
    CredentialSecretResolver, CredentialSecretStore, ProviderExecutionCredentialLease,
    UnavailableCredentialSecretResolver,
};
pub use media::{
    PrepareLocalMediaRequest, PreparedLocalMediaReference, ProviderToolMediaInputProfile,
    ProviderToolMediaInputRouteConstraint,
};
pub use model_execution::ResolvedPublicAiExecution;
pub use selection::ResolvedPublicAiSelection;
pub use tool_targets::AvailableProviderToolTarget;

#[cfg(test)]
mod test_support;

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
    /// Mutable activation mirror of effective catalogs; request selection reads the matching
    /// immutable catalogs inside `resolved_provider_generation` instead of this staging mirror.
    /// 有效目录的可变激活镜像；请求选择读取 `resolved_provider_generation` 中配对的
    /// 不可变目录，而不是此暂存镜像。
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
    refresh_locks: CredentialRefreshLocks,
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
    /// Content revisions of `activated_effective_catalogs`, computed at most once per connection.
    /// `activated_effective_catalogs` 的内容修订号，每个连接最多计算一次。
    activated_catalog_revisions: CatalogRevisionMemo,
}

/// Lazily filled connection-to-revision memo owned by exactly one generation value.
/// 由单个代际值独占、按需填充的连接到修订号缓存。
///
/// A clone starts empty, so a copied generation whose catalogs are then replaced can never
/// reuse a revision computed for different content.
/// 克隆从空缓存开始，因此复制后又替换目录的代际绝不会复用为不同内容计算的修订号。
#[derive(Default)]
struct CatalogRevisionMemo(StdMutex<BTreeMap<String, String>>);

impl Clone for CatalogRevisionMemo {
    fn clone(&self) -> Self {
        Self::default()
    }
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
    /// A runtime initialized from the current compiled bundled provider catalog.
    /// 返回从当前已编译内置供应商目录初始化的运行时。
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
        // Runtime staging and the metadata client share one catalog. Request selection separately
        // retains the published immutable generation; the bundled catalog is loaded only once here.
        // 运行时暂存与元数据客户端共享一份目录。请求选择另行持有所发布的不可变代际；
        // 此处只加载一次内置目录。
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
                activated_catalog_revisions: CatalogRevisionMemo::default(),
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
            refresh_locks: shared_credential_refresh_locks(),
            secrets,
            provider_tasks: ProviderTaskRecoveryStore::default(),
            bundled_manifests,
            resolved_provider_generation,
            session_selection_leases: Arc::new(RwLock::new(BTreeMap::new())),
            runtime_media_upload_progress_observer: Arc::new(RwLock::new(None)),
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

    /// Returns the strict provider registry from the current immutable manifest generation.
    /// 从当前不可变 Manifest 代际返回严格供应商注册表。
    ///
    /// # Returns
    /// 返回值
    /// An owned coherent registry snapshot derived from the exact published manifest catalog.
    /// 返回从精确已发布清单目录派生的自有一致注册表快照。
    pub fn providers(&self) -> Result<ProviderRegistry, String> {
        let generation = self
            .resolved_provider_generation
            .read()
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))?;
        ProviderRegistry::from_catalog(generation.manifests.as_ref())
    }

    /// Builds the effective provider registry for one immutable published generation.
    /// 为一个不可变已发布代际构建有效供应商注册表。
    ///
    /// System providers derive from the captured published generation; staged replacements remain
    /// invisible. Custom compatibility entries come from the caller-owned configuration.
    /// 系统供应商从已捕获发布代际派生，暂存替换仍不可见；自定义兼容项来自调用方配置。
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

    /// Returns the core-owned protocol runtime.
    /// 返回由核心拥有的协议运行时。
    ///
    /// # Returns
    /// 返回值
    /// The same shared protocol runtime used by metadata and execution.
    /// 返回元数据与执行使用的同一共享协议运行时。
    pub fn protocols(&self) -> &LlmProtocolRuntime {
        self.protocols.as_ref()
    }

    /// Returns the shared metadata client.
    /// 返回共享元数据客户端。
    ///
    /// # Returns
    /// 返回值
    /// A borrowed client that preserves this runtime's catalog and HTTP dependencies.
    /// 返回保留此运行时目录与 HTTP 依赖的客户端借用。
    pub fn metadata_client(&self) -> &HttpProviderMetadataClient {
        &self.metadata_client
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

/// Credential refresh locks keyed by durable connection ID.
/// 按持久连接 ID 索引的凭据刷新锁。
type CredentialRefreshLocks = Arc<StdMutex<BTreeMap<String, Arc<AsyncMutex<()>>>>>;

/// Returns process-owned locks so runtime replacement cannot consume a single-use rotation twice.
/// 返回进程持有的锁，避免运行时替换导致单次轮换被消费两次。
fn shared_credential_refresh_locks() -> CredentialRefreshLocks {
    // locks outlives every runtime generation and remains scoped by random durable connection IDs.
    // locks 比每代运行时存活更久，并按随机持久连接 ID 隔离。
    static LOCKS: std::sync::OnceLock<CredentialRefreshLocks> = std::sync::OnceLock::new();
    Arc::clone(LOCKS.get_or_init(|| Arc::new(StdMutex::new(BTreeMap::new()))))
}

#[cfg(test)]
mod tests {
    //! Runtime construction and shared operation-budget regressions.
    //! 运行时构造与共用操作预算回归测试。
    use super::*;

    /// Verifies runtime construction installs a process-level Rustls provider.
    /// 验证运行时构造会安装进程级 Rustls 加密提供程序。
    #[test]
    fn first_batch_installs_process_rustls_provider() {
        let _runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
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

    /// Verifies a copied generation never inherits catalog revisions computed for its source.
    /// 验证复制的代际绝不继承为来源计算的目录修订号。
    #[test]
    fn catalog_revision_memo_clone_starts_empty() {
        let memo = CatalogRevisionMemo::default();
        memo.0
            .lock()
            .expect("memo lock")
            .insert(String::from("connection"), String::from("sha256:source"));

        let copied = memo.clone();

        assert!(copied.0.lock().expect("memo lock").is_empty());
        assert_eq!(memo.0.lock().expect("memo lock").len(), 1);
    }
}
