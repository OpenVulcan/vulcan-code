//! Staged Manifest replacement, coherent configuration activation, and guarded compensation.
//! 暂存 Manifest 替换、一致配置激活与受保护的补偿。

use super::catalog_views::{
    CatalogProjectionUse, effective_catalog_snapshot_from_generation,
    resolve_model_catalog_overlays_from_inputs,
};
use super::{
    ProviderManifestRuntimeSnapshot, ProviderRuntimeActivationSnapshot, PublicAiRuntime,
    ResolvedProviderGeneration,
};
use crate::{PersistedAiConfiguration, ProviderKind, ResolvedProviderManifest};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc, Mutex as StdMutex, RwLock,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::Mutex as AsyncMutex;

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

impl ProviderStagedManifestCatalogMutation {
    /// Returns one exact signed candidate from the immutable staged after-image.
    /// 从不可变暂存后镜像返回一份精确签名候选。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the provider staged by the caller's activation report.
    /// `provider_id` 选择调用方激活报告暂存的供应商。
    ///
    /// # Returns
    /// 返回值
    /// The staged manifest when present, without exposing mutable catalog state.
    /// 存在时返回暂存清单，不暴露可变目录状态。
    pub fn staged_manifest(
        &self,
        provider_id: &str,
    ) -> Option<Arc<crate::provider_manifest::ResolvedProviderManifest>> {
        self.after.manifest_arc(provider_id)
    }
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
pub(super) fn runtime_configuration_for_manifest_catalog(
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

impl PublicAiRuntime {
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
    /// `_configuration` preserves the caller signature; the published generation owns the authoritative image.
    /// `_configuration` 保留调用签名；权威配置镜像由已发布代际持有。
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

    /// Captures one coherent immutable manifest/model-catalog selection snapshot.
    /// 捕获一份一致的不可变清单/模型目录选择快照。
    ///
    /// # Returns
    /// 返回值
    /// One Arc that owns the exact manifest generation and activated model catalogs observed
    /// under its read lock. Callers keep this Arc for the complete selection.
    /// 返回一个 Arc，持有在代际读锁下观察到的精确清单代际和已激活模型目录；调用方在
    /// 完整选择过程中持续持有此 Arc。
    pub(super) fn selection_catalog_snapshot(
        &self,
    ) -> Result<Arc<ResolvedProviderGeneration>, String> {
        self.resolved_provider_generation
            .read()
            .map(|generation| Arc::clone(&generation))
            .map_err(|_| String::from("The resolved provider generation lock is poisoned."))
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
    pub(super) fn isolated_catalog_activation_runtime(
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
    pub(super) fn publish_isolated_catalog_candidate(
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

    /// Stages one complete system Manifest replacement without publishing a request generation.
    /// 暂存一份完整系统 Manifest 替换件，但不发布请求代际。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the exact provider and `manifest` is its compiled, validated candidate.
    /// `provider_id` 选择精确供应商，`manifest` 是其已编译校验的候选。
    ///
    /// # Returns
    /// 返回值
    /// Success after the staged catalog CAS; requests observe the replacement only after explicit activation.
    /// 暂存目录比较交换成功后返回；请求仅在显式激活后才能观察到替换内容。
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
        // Identical staged content is a successful no-op; request visibility still requires activation.
        // 相同暂存内容视为成功的无变更操作；请求可见性仍须通过激活建立。
        // Do not create an observation-only generation or repeat the bounded CAS loop.
        // 不创建仅含观测差异的新代次，也不重复有界比较交换循环。
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
            CatalogProjectionUse::Management,
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
    pub fn staged_system_manifest(
        &self,
        provider_id: &str,
    ) -> Option<Arc<ResolvedProviderManifest>> {
        self.bundled_manifests
            .read()
            .ok()
            .and_then(|catalog| catalog.manifest_arc(provider_id))
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
            // Disabled credentials retain their prior source evidence but never perform activation-time I/O.
            // 已停用凭据保留既有来源证据，但绝不执行激活阶段 I/O。
            if !runtime_configuration.connection_is_enabled(connection_id.as_str()) {
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
    /// `published_configuration` preserves the authoritative image; `runtime_configuration` excludes isolated providers.
    /// `published_configuration` 保留权威镜像；`runtime_configuration` 排除已隔离供应商。
    /// `system_manifests` is the exact published or explicitly staged system authority for this activation.
    /// `system_manifests` 是本次激活使用的精确已发布或显式暂存系统权威。
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
        // Reject invalid media settings before acquiring mutation ownership or publishing catalogs.
        // 在获取修改所有权或发布目录之前拒绝无效媒体设置。
        published_configuration
            .effective_media_delivery_policy()
            .validate()?;
        runtime_configuration
            .effective_media_delivery_policy()
            .validate()?;
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
}

#[cfg(test)]
mod tests;
