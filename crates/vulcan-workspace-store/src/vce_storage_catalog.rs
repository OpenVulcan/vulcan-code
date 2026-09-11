//! Lightweight host-owned directory for project-isolated VCE storage.
//! 项目隔离 VCE 存储使用的轻量宿主自有目录。

use crate::data_plane::HostRuntimeHandle;
use crate::local_database_runtime::SqliteFileBinding;
use crate::local_sql;
use crate::vce_data_plane::{
    VceDataPlaneHandle, VceEmbeddingProvenance, VceJobKind, VceProjectIndex, VceProjectIndexConfig,
    VceProjectJobStore, VceRebuildCandidateBase, VceStoredJob, prepare_vce_storage_binding,
};
use crate::vce_storage_paths::{VceProjectStoragePaths, VceProjectWriterLease, VceStoragePaths};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use vulcan_sqlite_engine::runtime::{SqliteOpenOptions, SqlitePragmaOptions};
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// Current catalog layout format; independent from engine checkpoints and vector schema versions.
/// 当前目录布局格式；独立于引擎检查点与向量结构版本。
const VCE_CATALOG_SCHEMA_VERSION: u32 = 1;

/// Maximum resident physical storage bindings; active leases are never evicted.
/// 驻留物理存储绑定上限；绝不淘汰活动租约。
const MAX_CACHED_STORES: usize = 8;

/// Host-native lazy factory for the sole project-isolated storage layout.
/// 唯一项目隔离存储布局的宿主原生惰性工厂。
#[derive(Clone)]
pub struct VceDataPlaneFactory {
    /// Metadata-only location ownership, independent from content checkpoints.
    /// 仅元数据的位置归属，独立于内容检查点。
    catalog: VceStorageCatalog,
    /// Shared bounded binding cache retained by all manager clones.
    /// 全部管理器克隆共享的有界绑定缓存。
    stores: Arc<Mutex<BTreeMap<String, CachedVceStore>>>,
}

/// One cached binding, including incomplete closure that must finish before it can be reused.
/// 一个缓存绑定，包含必须先完成才能复用的不完整关闭状态。
struct CachedVceStore {
    /// Exact data-plane handles with externally counted query/write leases.
    /// 具有外部查询与写入租约计数的精确数据面句柄。
    handle: VceDataPlaneHandle,
    /// Whether partial teardown prohibits handing out this handle again.
    /// 部分拆卸是否禁止再次发放此句柄。
    closing: bool,
}

impl VceDataPlaneFactory {
    /// Reconcile a stopped project job while retaining exclusive cross-process writer ownership.
    /// 保留跨进程独占写入归属期间，收敛已经停止的项目作业。
    /// `project_id` selects existing storage and `job_id` selects the observed receipt.
    /// `project_id` 选择已有存储，`job_id` 选择已观察回执。
    /// Returns no change if a cooperating writer is alive or the selected job has ended.
    /// 协作写入者仍活动或选定作业已结束时不返回变更。
    pub async fn reconcile_ended_project_job(
        &self,
        project_id: &str,
        job_id: &str,
    ) -> Result<Option<VceStoredJob>, String> {
        let Some(store) = self.existing_store(project_id).await? else {
            return Ok(None);
        };
        // The OS lease, not elapsed time or a different host ID, proves the previous writer has ended.
        // 操作系统租约而非经过的时间或不同宿主编号，证明先前写入者已经结束。
        let Some(_writer) = self.try_acquire_project_writer(project_id)? else {
            return Ok(None);
        };
        VceProjectJobStore::new(store, project_id.to_owned())
            .reconcile_ended_job(job_id)
            .await
    }

    /// Acquire an ended host's writer lease and reopen one unfinished physical compaction job.
    /// 获取已结束宿主的写入租约，并重新打开一项未完成的物理整理作业。
    ///
    /// `project_id` and `job_id` identify the exact durable owner. Returns the active index,
    /// job store, receipt, and retained writer lease without submitting or cancelling any work.
    /// `project_id` 与 `job_id` 标识精确持久所有者。返回活动索引、作业存储、回执及保留的
    /// 写入租约，不提交也不取消任何操作。
    pub async fn prepare_cold_compaction_recovery(
        &self,
        project_id: &str,
        job_id: &str,
    ) -> Result<
        Option<(
            VceProjectWriterLease,
            VceProjectIndex,
            VceProjectJobStore,
            VceStoredJob,
        )>,
        String,
    > {
        let Some(writer) = self.try_acquire_project_writer(project_id)? else {
            return Ok(None);
        };
        let Some(store) = self.existing_store(project_id).await? else {
            return Ok(None);
        };
        let Some(config) = store.load_active_project_config(project_id).await? else {
            return Ok(None);
        };
        let index = store
            .open_existing_project_index(&config)
            .await?
            .ok_or_else(|| String::from("VCE compaction recovery index disappeared"))?;
        let job_store = index.job_store();
        let Some(receipt) = job_store.get(job_id).await? else {
            return Ok(None);
        };
        if receipt.project_id != project_id
            || receipt.job_id != job_id
            || receipt.kind != VceJobKind::Compact
        {
            return Err(String::from(
                "VCE compaction recovery receipt identity does not match its project",
            ));
        }
        if receipt.state.is_terminal() {
            return Ok(Some((writer, index, job_store, receipt)));
        }
        let unfinished = job_store
            .unfinished()
            .await?
            .ok_or_else(|| String::from("VCE compaction recovery has no unfinished owner"))?;
        if unfinished != receipt {
            return Err(String::from(
                "VCE compaction recovery receipt is not the unfinished project owner",
            ));
        }
        let provenance = index.embedding_provenance().await?;
        // Policy revision is mutable admission metadata and is not part of the persisted physical binding.
        // 策略修订是可变准入元数据，不属于持久化物理绑定。
        if provenance.checkpoint_version != receipt.source_checkpoint_version
            || config.physical_fingerprint != receipt.contract_fingerprint
        {
            return Err(String::from(
                "VCE compaction recovery target changed after durable admission",
            ));
        }
        Ok(Some((writer, index, job_store, receipt)))
    }

    /// Read one project's original and retained vector origins without creating project storage.
    /// 读取一个项目的原始及保留向量来源，不创建项目存储。
    ///
    /// `project_id` is exact host ownership. Returns validated metadata or known absent storage.
    /// `project_id` 是精确宿主归属。返回已验证元数据或已知存储缺失。
    pub async fn embedding_provenance(
        &self,
        project_id: &str,
    ) -> Result<Option<VceEmbeddingProvenance>, String> {
        match self.existing_store(project_id).await? {
            Some(store) => store.embedding_provenance(project_id).await,
            None => Ok(None),
        }
    }

    /// Bootstraps only the lightweight catalog through the existing host runtime.
    /// 通过现有宿主运行时仅引导轻量目录。
    ///
    /// `host_runtime` owns the resolved root and registration. Returns an empty lazy binding cache.
    /// `host_runtime` 持有已解析根与注册。返回空的惰性绑定缓存。
    pub async fn bootstrap(host_runtime: &HostRuntimeHandle) -> Result<Self, String> {
        Ok(Self {
            catalog: VceStorageCatalog::bootstrap(host_runtime).await?,
            stores: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    /// Load original project provenance from its published location without creating storage.
    /// 从已发布位置加载项目原始来源，不创建存储。
    ///
    /// `project_id` is the stable host identity. Returns original metadata or confirmed absence.
    /// `project_id` 是稳定宿主身份。返回原始元数据或已确认的缺失。
    pub async fn load_active_project_config(
        &self,
        project_id: &str,
    ) -> Result<Option<VceProjectIndexConfig>, String> {
        match self.existing_store(project_id).await? {
            Some(store) => store.load_active_project_config(project_id).await,
            None => Ok(None),
        }
    }

    /// Read the sole explicit orphaned uninitialized binding without creating storage.
    /// 读取唯一明确孤立的未初始化绑定，不创建存储。
    pub async fn load_uninitialized_recovery_config(
        &self,
        project_id: &str,
    ) -> Result<Option<VceProjectIndexConfig>, String> {
        match self.existing_store(project_id).await? {
            Some(store) => store.load_uninitialized_recovery_config(project_id).await,
            None => Ok(None),
        }
    }

    /// Restore a compatible existing handle at the exact published location without mutation.
    /// 在精确已发布位置恢复兼容现有句柄，不执行修改。
    ///
    /// `config` is the current host contract. Returns the existing index or confirmed absence.
    /// `config` 是当前宿主契约。返回现有索引或已确认的缺失。
    pub async fn open_existing_project_index(
        &self,
        config: &VceProjectIndexConfig,
    ) -> Result<Option<VceProjectIndex>, String> {
        match self.existing_store(&config.project_id).await? {
            Some(store) => store.open_existing_project_index(config).await,
            None => Ok(None),
        }
    }

    /// Open one explicit orphaned uninitialized binding without activating it.
    /// 打开一个明确孤立的未初始化绑定，但不激活它。
    pub async fn open_uninitialized_recovery_project_index(
        &self,
        config: &VceProjectIndexConfig,
    ) -> Result<Option<VceProjectIndex>, String> {
        match self.existing_store(&config.project_id).await? {
            Some(store) => {
                store
                    .open_uninitialized_recovery_project_index(config)
                    .await
            }
            None => Ok(None),
        }
    }

    /// Acquire the project writer, fence an uninitialized binding, and return its exact job store.
    /// 获取项目写入租约、封锁未初始化绑定，并返回其精确作业存储。
    ///
    /// `project_id` and `job_id` identify one durable owner; the returned writer must remain held
    /// until the caller persists the terminal cancellation transition.
    /// `project_id` 与 `job_id` 标识一个持久所有者；调用方必须一直持有返回的写入租约，直到
    /// 持久化取消终态。
    ///
    /// Returns no value when storage or the receipt is absent; published attempts are reconciled
    /// without retiring their binding. Competing writers and identity mismatches remain errors.
    /// 存储或回执缺失时返回空值；已发布尝试只收敛回执、不退役绑定。
    /// 竞争写入者与身份不匹配仍返回错误。
    pub async fn prepare_cold_job_cancellation(
        &self,
        project_id: &str,
        job_id: &str,
    ) -> Result<Option<(VceProjectWriterLease, VceProjectJobStore, VceStoredJob)>, String> {
        let writer = self
            .try_acquire_project_writer(project_id)?
            .ok_or_else(|| String::from("VCE project already has an active host writer"))?;
        let Some(store) = self.existing_store(project_id).await? else {
            return Ok(None);
        };
        let (index, active_config) = if let Some(config) =
            store.load_active_project_config(project_id).await?
        {
            let index = store
                .open_existing_project_index(&config)
                .await?
                .ok_or_else(|| {
                    String::from("VCE cancellation storage disappeared during recovery")
                })?;
            (index, Some(config))
        } else {
            let Some(config) = store.load_uninitialized_recovery_config(project_id).await? else {
                return Ok(None);
            };
            let index = store
                .open_uninitialized_recovery_project_index(&config)
                .await?
                .ok_or_else(|| {
                    String::from("VCE orphaned cancellation storage disappeared during recovery")
                })?;
            (index, None)
        };
        let job_store = index.job_store();
        let Some(receipt) = job_store.get(job_id).await? else {
            return Ok(None);
        };
        if receipt.project_id != project_id || receipt.job_id != job_id {
            return Err(String::from(
                "VCE cold cancellation receipt identity does not match its project",
            ));
        }
        if receipt.state.is_terminal() {
            return Ok(Some((writer, job_store, receipt)));
        }
        let Some(unfinished) = job_store.unfinished().await? else {
            return Err(String::from(
                "VCE cold cancellation found no unfinished owner for its receipt",
            ));
        };
        if unfinished != receipt || unfinished.job_id != job_id {
            return Err(String::from(
                "VCE cold cancellation receipt is not the unfinished owner of this binding",
            ));
        }
        let provenance = if active_config.is_some() {
            index.embedding_provenance().await?
        } else {
            index.uninitialized_recovery_provenance().await?
        };
        if provenance.checkpoint_version.is_some()
            || !matches!(receipt.kind, VceJobKind::Initialize | VceJobKind::Rebuild)
            || receipt.source_checkpoint_version.is_some()
            || receipt.replacement.is_some()
        {
            // Published or replacement attempts are settled without retiring any binding.
            // 已发布或替换尝试只收敛回执，不退役任何绑定。
            let settled = job_store
                .reconcile_ended_job(job_id)
                .await?
                .ok_or_else(|| {
                    String::from("VCE cold cancellation lost its exact unfinished receipt")
                })?;
            return Ok(Some((writer, job_store, settled)));
        }
        if let Some(config) = active_config {
            store.retire_uninitialized_active_binding(&config).await?;
        }
        Ok(Some((writer, job_store, receipt)))
    }

    /// Replace one uninitialized binding with a fresh current-contract namespace under one writer lease.
    /// 在一个写者租约下，用新的当前契约命名空间替换一个未初始化绑定。
    ///
    /// `previous` is the exact active or explicitly orphaned identity; `replacement` must use a
    /// distinct namespace so stale runtime requests cannot write the new binding.
    /// 必须使用不同的命名空间，避免旧运行时请求写入新绑定。
    /// The returned lease remains held so the caller can start the replacement job without a race.
    /// 返回的租约会继续持有，使调用方可以无竞态地启动替换作业。
    pub async fn reinitialize_uninitialized_project_index(
        &self,
        previous: &VceProjectIndexConfig,
        replacement: VceProjectIndexConfig,
    ) -> Result<(VceProjectIndex, VceProjectWriterLease), String> {
        previous.validate()?;
        replacement.validate()?;
        if previous.project_id != replacement.project_id {
            return Err(String::from(
                "VCE uninitialized replacement changed its project identity",
            ));
        }
        if previous.binding.namespace_id == replacement.binding.namespace_id {
            return Err(String::from(
                "VCE uninitialized replacement requires a distinct namespace",
            ));
        }
        // A replacement must also receive a distinct physical vector-table identity.
        // 替换还必须获得不同的物理向量表身份。
        if previous.vector_space_id == replacement.vector_space_id
            && previous.physical_fingerprint == replacement.physical_fingerprint
            && previous.preprocessing_version == replacement.preprocessing_version
            && previous.chunker_version == replacement.chunker_version
            && previous.dimension == replacement.dimension
        {
            return Err(String::from(
                "VCE uninitialized replacement requires distinct physical vector tables",
            ));
        }
        let writer = self
            .catalog
            .paths
            .try_acquire_writer(&previous.project_id, false)?
            .ok_or_else(|| String::from("VCE project already has an active host writer"))?;
        let store = self
            .existing_store(&previous.project_id)
            .await?
            .ok_or_else(|| {
                String::from("VCE uninitialized replacement requires published project storage")
            })?;
        let active = store
            .load_active_project_config(&previous.project_id)
            .await?;
        match active {
            Some(current) if current == *previous => {
                let index = store
                    .open_existing_project_index(&current)
                    .await?
                    .ok_or_else(|| String::from("VCE uninitialized predecessor disappeared"))?;
                if index
                    .embedding_provenance()
                    .await?
                    .checkpoint_version
                    .is_some()
                {
                    return Err(String::from(
                        "VCE uninitialized replacement cannot retire a published checkpoint",
                    ));
                }
                store.retire_uninitialized_active_binding(previous).await?;
            }
            Some(_) => {
                return Err(String::from(
                    "VCE uninitialized replacement predecessor changed before fencing",
                ));
            }
            None => {
                let recovery = store
                    .load_uninitialized_recovery_config(&previous.project_id)
                    .await?
                    .ok_or_else(|| {
                        String::from("VCE uninitialized replacement predecessor is missing")
                    })?;
                if recovery != *previous {
                    return Err(String::from(
                        "VCE uninitialized replacement recovery identity changed",
                    ));
                }
            }
        }
        // open_project_index allocates a fresh initial binding while the same writer lease remains held.
        // open_project_index 在同一写者租约保持时分配新的初始化绑定。
        let index = store.open_project_index(replacement).await?;
        Ok((index, writer))
    }

    /// Try to exclude competing host mutations in one existing project without opening its databases.
    /// 尝试排斥单个现有项目的竞争宿主修改，不打开其数据库。
    /// `project_id` selects stable ownership. Returns an owned lease, busy, or an explicit error.
    /// `project_id` 选择稳定归属。返回自有租约、忙状态或明确错误。
    /// Keep the lease until all admitted work and receipt finalization settle; queries do not need it.
    /// 保留租约直到全部准入工作与回执终态处理确定；查询不需要此租约。
    pub fn try_acquire_project_writer(
        &self,
        project_id: &str,
    ) -> Result<Option<VceProjectWriterLease>, String> {
        self.catalog.paths.try_acquire_writer(project_id, false)
    }

    /// Create only a missing project's isolated store under explicit initialization authority.
    /// 仅在明确初始化授权下创建缺失项目的隔离存储。
    ///
    /// `config` is the admitted creation contract. Existing original state requires a separate
    /// explicit rebuild workflow and is never overwritten by this entry.
    /// `config` 是已准入创建契约。现有原始状态要求单独明确重建流程，
    /// 本入口绝不覆盖它。
    pub async fn create_project_index(
        &self,
        config: VceProjectIndexConfig,
    ) -> Result<VceProjectIndex, String> {
        config.validate()?;
        // Creation owns the same lock as later jobs before any catalog or physical table allocation.
        // 创建在任何目录记录或物理表分配之前持有与后续作业相同的锁。
        let _writer = self
            .catalog
            .paths
            .try_acquire_writer(&config.project_id, true)?
            .ok_or_else(|| String::from("VCE project already has an active host writer"))?;
        if self
            .load_active_project_config(&config.project_id)
            .await?
            .is_some()
        {
            return Err(String::from(
                "VCE project already has an index; use explicit rebuilding",
            ));
        }
        // paths allocates only the authorized project, never all known workspace projects.
        // paths 仅分配获授权项目，绝不分配全部已知工作区项目。
        let paths = self.catalog.allocate_project(&config.project_id).await?;
        // store binds this project's physical SQLite and LanceDB directories through the host.
        // store 通过宿主绑定此项目的物理 SQLite 与 LanceDB 目录。
        let store = self
            .open_cached(
                &format!("project:{}", paths.project_key),
                &paths.root,
                config.project_id.clone(),
                true,
            )
            .await?;
        // index preserves any successfully created origin if publication was previously interrupted.
        // 若此前位置发布被中断，index 保留已经成功创建的来源。
        let index = match store.open_existing_project_index(&config).await? {
            Some(index) => index,
            None => store.open_project_index(config.clone()).await?,
        };
        self.catalog.publish_allocation(&config.project_id).await?;
        Ok(index)
    }

    /// Prepare a distinct inactive candidate at an already published project location.
    /// 在已发布项目位置准备独立非活动候选。
    /// `config` names new physical space; `base` identifies the confirmed predecessor and owning job.
    /// `config` 指定新物理空间；`base` 标识已确认前驱及所属作业。
    /// Returns an inactive handle without allocating another location or replacing the active index.
    /// 返回非活动句柄，不分配另一位置，不替换活动索引。
    pub async fn prepare_rebuild_candidate(
        &self,
        config: VceProjectIndexConfig,
        base: VceRebuildCandidateBase,
    ) -> Result<VceProjectIndex, String> {
        config.validate()?;
        // store must already exist because rebuilding cannot substitute for explicit initialization.
        // store 必须已存在，因为重建不能代替明确初始化。
        let store = self
            .existing_store(&config.project_id)
            .await?
            .ok_or_else(|| {
                String::from("VCE rebuild requires an existing published project location")
            })?;
        store.prepare_rebuild_candidate(config, base).await
    }

    /// Reopen an allocated unpublished candidate for explicit owning-job recovery without creating storage.
    /// 为所属作业的显式恢复重新打开已分配未发布候选，不创建存储。
    /// `config` and `base` must match allocation. Returns an unadmitted handle, including a cancelled candidate.
    /// `config` 与 `base` 必须匹配分配。返回未准入句柄，也允许读取已取消候选。
    pub async fn reopen_rebuild_candidate(
        &self,
        config: &VceProjectIndexConfig,
        base: &VceRebuildCandidateBase,
    ) -> Result<VceProjectIndex, String> {
        config.validate()?;
        // store is the existing published location; recovery must not allocate a replacement directory.
        // store 是既有已发布位置；恢复不得分配替代目录。
        let store = self
            .existing_store(&config.project_id)
            .await?
            .ok_or_else(|| {
                String::from("VCE candidate recovery requires its existing project location")
            })?;
        store.reopen_rebuild_candidate(config, base).await
    }

    /// Find an exact candidate after failed allocation without creating storage or assuming allocation succeeded.
    /// 分配失败后查找精确候选，不创建存储，也不假定分配已经成功。
    /// `config` and `base` are the durable job target; returns its optional unadmitted handle.
    /// `config` 与 `base` 为持久作业目标；返回其可选未准入句柄。
    pub async fn find_rebuild_candidate(
        &self,
        config: &VceProjectIndexConfig,
        base: &VceRebuildCandidateBase,
    ) -> Result<Option<VceProjectIndex>, String> {
        config.validate()?;
        let store = self
            .existing_store(&config.project_id)
            .await?
            .ok_or_else(|| {
                String::from("VCE candidate recovery requires its existing project location")
            })?;
        store.find_rebuild_candidate(config, base).await
    }

    /// Shut down exact cached bindings only after every project lease has drained.
    /// 仅在全部项目租约排空后关闭精确缓存绑定。
    ///
    /// Takes no extra parameters. Returns explicit live-lease or cleanup errors and never deletes data.
    /// 无额外参数。返回明确活动租约或清理错误，绝不删除数据。
    pub async fn shutdown(&self) -> Result<(), String> {
        self.close_project_bindings().await?;
        self.catalog.close().await
    }

    /// Close project bindings while retaining catalog availability for host shutdown recovery.
    /// 关闭项目绑定，同时保留目录可用性以供宿主关闭失败恢复。
    ///
    /// Takes no parameters. Returns an error without invalidating a binding with live leases.
    /// 无参数。存在活动租约时返回错误，不使其绑定失效。
    pub async fn close_project_bindings(&self) -> Result<(), String> {
        // stores is held only for binding lifecycle, not by provider or vector query execution.
        // stores 仅用于绑定生命周期，不被供应商调用或向量查询执行持有。
        let mut stores = self.stores.lock().await;
        // Preflight every lease before changing any binding. A rejected shutdown must leave the
        // still-running application with the exact cache state it had before the attempt.
        // 在修改任何绑定前预检全部租约。关闭被拒绝时，仍在运行的应用必须保留尝试前的精确缓存状态。
        if stores
            .values()
            .any(|store| Arc::strong_count(&store.handle.lease) != 1)
        {
            return Err(String::from("VCE storage still has live project leases"));
        }
        for store in stores.values_mut() {
            store.closing = true;
            store.handle.close_bindings().await?;
        }
        stores.clear();
        Ok(())
    }

    /// Lists cached owners and closure state for lifecycle regression tests.
    /// 为生命周期回归测试列出缓存所有者及关闭状态。
    ///
    /// Takes no parameters and returns entries in the cache's deterministic key order.
    /// 无参数，按缓存的确定键顺序返回条目。
    #[cfg(test)]
    pub(crate) async fn cached_store_states(&self) -> Vec<(String, bool)> {
        self.stores
            .lock()
            .await
            .values()
            .map(|store| {
                (
                    store.handle.project_owner_for_test().to_owned(),
                    store.closing,
                )
            })
            .collect()
    }

    /// Select only the catalog-published project store; no old-layout probing or adaptation.
    /// 仅选择目录发布的项目存储；不探测或适配旧布局。
    ///
    /// `project_id` is the exact host identity. Missing allocation cannot create a database.
    /// Returns an optional leased handle; broken published locations are explicit errors.
    /// `project_id` 是精确宿主身份。缺失分配不能创建数据库。
    /// 返回可选租约句柄；已发布位置损坏时返回明确错误。
    async fn existing_store(&self, project_id: &str) -> Result<Option<VceDataPlaneHandle>, String> {
        if let Some(location) = self.catalog.project_location(project_id).await? {
            if location.layout_state == VceStorageLocationState::Allocated {
                return Ok(None);
            }
            // paths is independently recomputed rather than trusting a persisted relative path.
            // paths 独立重新计算，不信任持久化相对路径。
            let paths = self.catalog.paths.project(project_id)?;
            if !paths.sqlite_path.is_file() {
                return Err(String::from("Published VCE project database is missing"));
            }
            return self
                .open_cached(
                    &format!("project:{}", paths.project_key),
                    &paths.root,
                    project_id.to_owned(),
                    false,
                )
                .await
                .map(Some);
        }
        Ok(None)
    }

    /// Reuse or open one checked binding while retaining a strict resident-resource bound.
    /// 复用或打开一个已检查绑定，同时保持严格驻留资源上限。
    ///
    /// `key`, `root` and `owner` are factory-derived identities; `create` comes from an admitted job.
    /// Returns a counted external lease for exactly one project.
    /// `key`、`root` 与 `owner` 是工厂派生身份；`create` 来自已准入作业。
    /// 返回精确单项目的带计数外部租约。
    async fn open_cached(
        &self,
        key: &str,
        root: &Path,
        owner: String,
        create: bool,
    ) -> Result<VceDataPlaneHandle, String> {
        // stores serializes only lazy binding setup/closure; existing leased queries remain independent.
        // stores 仅串行化惰性绑定建立与关闭；已有租约查询保持独立。
        let mut stores = self.stores.lock().await;
        if let Some(existing) = stores.get(key) {
            if !existing.closing {
                return Ok(existing.handle.clone());
            }
            existing.handle.close_bindings().await?;
            stores.remove(key);
        }
        if stores.len() >= MAX_CACHED_STORES {
            // idle_key can be removed only when the cache owns the final remaining lease.
            // idle_key 仅在缓存拥有最后一个剩余租约时可以移除。
            let idle_key = stores
                .iter()
                .find(|(_, store)| Arc::strong_count(&store.handle.lease) == 1)
                .map(|(key, _)| key.clone())
                .ok_or_else(|| {
                    String::from("VCE storage binding budget is occupied by active project leases")
                })?;
            // idle remains retained on cleanup failure so partial closure is retried, not reused.
            // idle 在清理失败时继续保留，使部分关闭被重试而不是被复用。
            let idle = stores
                .get_mut(&idle_key)
                .ok_or_else(|| String::from("VCE idle storage binding disappeared"))?;
            idle.closing = true;
            idle.handle.close_bindings().await?;
            stores.remove(&idle_key);
        }
        // handle is opened only after location checks and cache admission succeeded.
        // handle 仅在位置检查与缓存准入成功后打开。
        let handle = prepare_vce_storage_binding(
            &self.catalog.host_runtime,
            root.to_path_buf(),
            owner,
            create,
        )?;
        stores.insert(
            key.to_owned(),
            CachedVceStore {
                handle: handle.clone(),
                closing: true,
            },
        );
        if let Err(error) = handle.activate_bindings(create).await {
            return match handle.close_bindings().await {
                Ok(()) => {
                    stores.remove(key);
                    Err(error)
                }
                Err(cleanup) => Err(format!("{error}; close partial VCE activation: {cleanup}")),
            };
        }
        stores
            .get_mut(key)
            .ok_or_else(|| String::from("VCE opening binding disappeared"))?
            .closing = false;
        Ok(handle)
    }
}

/// Host catalog containing locations only, never an assertion that a project has a usable index.
/// 仅包含位置的宿主目录，绝不据此断言项目拥有可用索引。
#[derive(Clone)]
pub(crate) struct VceStorageCatalog {
    /// Host runtime that owns the in-process engine serving this catalog.
    /// 拥有服务本目录的进程内引擎的宿主运行时。
    host_runtime: HostRuntimeHandle,
    /// Checked host data paths; no environment fallback is allowed here.
    /// 已检查的宿主数据路径；此处不允许环境变量兜底。
    paths: VceStoragePaths,
    /// Exact metadata-only SQLite binding, independent from every project binding.
    /// 精确的仅元数据 SQLite 绑定，独立于全部项目绑定。
    binding: SqliteFileBinding,
}

/// Persisted ownership of one project directory, not code-index initialization state.
/// 一个项目目录的持久化归属，不是代码索引初始化状态。
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct VceProjectStorageLocation {
    /// Exact host project identity, retained to verify its hashed key.
    /// 精确宿主项目身份，用于验证其哈希键。
    pub(crate) project_id: String,
    /// Full safe directory key derived from the project identity.
    /// 从项目身份派生的完整安全目录键。
    pub(crate) project_key: String,
    /// Host-defined relative project directory, never a freely supplied path.
    /// 宿主定义的项目相对目录，绝不是自由提供的路径。
    pub(crate) relative_root: String,
    /// Physical directory layout version.
    /// 物理目录布局版本。
    pub(crate) layout_version: u32,
    /// Location publication revision, independent from a content checkpoint revision.
    /// 位置发布修订，独立于内容检查点修订。
    pub(crate) location_version: u64,
    /// Whether storage creation has been published; this does not assert a usable checkpoint.
    /// 存储创建是否已发布；此字段不代表存在可用检查点。
    pub(crate) layout_state: VceStorageLocationState,
}

/// Location allocation lifecycle, independent from content-index initialization.
/// 位置分配生命周期，独立于内容索引初始化。
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VceStorageLocationState {
    /// Explicitly allocated storage whose publication may still be incomplete.
    /// 已明确分配但发布可能尚未完成的存储。
    Allocated,
    /// A validated physical store has been published at this exact location.
    /// 已校验物理存储在此精确位置发布。
    Published,
}

/// Exact version row required before opening an existing catalog.
/// 打开现有目录前要求的精确版本行。
#[derive(Deserialize)]
struct CatalogVersionRow {
    /// Supported catalog schema number.
    /// 支持的目录结构编号。
    schema_version: u32,
}

/// Builds the catalog's exact in-process SQLite binding.
/// 构建目录精确的进程内 SQLite 绑定。
///
/// The catalog declares only options enforced by the in-process engine. Host-side runtime
/// serialization provides the concurrency boundary instead of exposing a fictitious pool size.
/// 目录只声明进程内引擎实际执行的选项。并发边界由宿主运行时串行化提供，不再暴露虚假的连接池大小。
fn catalog_sqlite_binding(path: PathBuf) -> SqliteFileBinding {
    SqliteFileBinding {
        path,
        options: SqliteOpenOptions {
            pragmas: SqlitePragmaOptions {
                cache_size_kib: 4_096,
                mmap_size_bytes: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    }
}
impl VceStorageCatalog {
    /// Bootstraps only the catalog using the already registered host runtime.
    /// 使用已注册宿主运行时仅引导轻量目录。
    ///
    /// `host_runtime` owns the resolved application root and local database runtime. Returns a metadata-only
    /// catalog handle; no historical project is opened and no vector database is created.
    /// `host_runtime` 持有已解析应用根与本地数据库运行时。返回仅元数据目录句柄；
    /// 不打开任何历史项目，也不创建任何向量数据库。
    pub(crate) async fn bootstrap(host_runtime: &HostRuntimeHandle) -> Result<Self, String> {
        // paths is derived exclusively from the verified host-owned root.
        // paths 仅从已验证的宿主自有根派生。
        let paths = VceStoragePaths::resolve(&host_runtime.data_root)?;
        // catalog_path is checked before engine activation may open the file.
        // catalog_path 在引擎激活可能打开文件前完成检查。
        let catalog_path = paths.prepare_catalog()?;
        // existed distinguishes a new catalog from an existing catalog requiring version validation.
        // existed 区分新目录与需要版本校验的现有目录。
        let existed = catalog_path.is_file();
        // The catalog database is served only by this host runtime, so no external process can open it.
        // That keeps ownership local and makes its handle ours to close.
        // 目录数据库只由本宿主运行时服务，因此外部进程无法打开它。这使所有权保持在本地，句柄也由我们关闭。
        let catalog = Self {
            host_runtime: host_runtime.clone(),
            paths,
            binding: catalog_sqlite_binding(catalog_path),
        };
        if let Err(error) = catalog.initialize(existed).await {
            return match catalog.close().await {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; close failed catalog: {cleanup}")),
            };
        }
        Ok(catalog)
    }

    /// Read a location by exact project identity without allocating or opening its database.
    /// 按精确项目身份读取位置，不分配或打开项目数据库。
    ///
    /// `project_id` is the host identity. Returns a verified existing location, absence, or
    /// an explicit ownership/schema error; absence is not proof about any legacy shared index.
    /// `project_id` 是宿主身份。返回已验证的现有位置、空值或明确归属／结构错误；
    /// 空值不能证明旧共享索引不存在。
    pub(crate) async fn project_location(
        &self,
        project_id: &str,
    ) -> Result<Option<VceProjectStorageLocation>, String> {
        // expected validates the caller identity and every existing physical path component.
        // expected 校验调用方身份与全部现有物理路径组件。
        let expected = self.paths.project(project_id)?;
        // rows is restricted to one exact primary key; no global project scan is needed.
        // rows 仅限一个精确主键，无需全局项目扫描。
        let rows: Vec<VceProjectStorageLocation> = self.query(
            "SELECT project_id, project_key, relative_root, layout_version, location_version, layout_state FROM vce_storage_projects WHERE project_id = ?;",
            vec![SqliteValue::Text(project_id.to_owned())],
        ).await?;
        if rows.len() > 1 {
            return Err(String::from(
                "VCE catalog contains duplicate project identities",
            ));
        }
        if let Some(location) = rows.first() {
            location.validate(&expected)?;
        }
        Ok(rows.into_iter().next())
    }

    /// Allocate a project directory after explicit creation admission, without creating an index.
    /// 在明确创建准入后分配项目目录，不创建索引。
    ///
    /// `project_id` identifies the authorized project. Returns its exact stable physical paths;
    /// repeat allocation preserves the existing location revision and never rewrites provenance.
    /// `project_id` 标识获授权项目。返回精确稳定物理路径；
    /// 重复分配保留现有位置修订，绝不改写来源。
    pub(crate) async fn allocate_project(
        &self,
        project_id: &str,
    ) -> Result<VceProjectStoragePaths, String> {
        // Existing records must pass validation before any directory is created.
        // 任何目录创建前，现有记录必须通过校验。
        self.project_location(project_id).await?;
        // project contains only paths created under this explicit host admission.
        // project 仅包含本次明确宿主准入下创建的路径。
        let project = self.paths.prepare_project(project_id)?;
        self.execute(
            "INSERT INTO vce_storage_projects(project_id, project_key, relative_root, layout_version, location_version, layout_state) VALUES (?, ?, ?, 1, 1, 'allocated') ON CONFLICT(project_id) DO NOTHING;",
            vec![SqliteValue::Text(project_id.to_owned()), SqliteValue::Text(project.project_key.clone()), SqliteValue::Text(format!("projects/{}", project.project_key))],
        ).await?;
        self.project_location(project_id)
            .await?
            .ok_or_else(|| String::from("VCE project allocation did not persist its location"))?;
        Ok(project)
    }

    /// Publish a successfully created physical binding without claiming content initialization.
    /// 发布成功创建的物理绑定，不宣称内容初始化已经完成。
    ///
    /// `project_id` identifies an allocated location. Returns an error if ownership disappeared.
    /// `project_id` 标识已分配位置。归属消失时返回错误。
    async fn publish_allocation(&self, project_id: &str) -> Result<(), String> {
        self.project_location(project_id)
            .await?
            .ok_or_else(|| String::from("VCE allocation is missing"))?;
        self.execute(
            "UPDATE vce_storage_projects SET layout_state = 'published' WHERE project_id = ? AND layout_state = 'allocated';",
            vec![SqliteValue::Text(project_id.to_owned())],
        ).await?;
        // published verifies the durable location receipt; no checkpoint facts are duplicated here.
        // published 验证持久化位置回执；此处不重复保存检查点事实。
        let published = self
            .project_location(project_id)
            .await?
            .ok_or_else(|| String::from("VCE published allocation is missing"))?;
        if published.layout_state != VceStorageLocationState::Published {
            return Err(String::from("VCE location publication did not complete"));
        }
        Ok(())
    }

    /// Return the metadata root for physical-layout diagnostics without listing project contents.
    /// 返回用于物理布局诊断的元数据根，不列举项目内容。
    ///
    /// Takes no extra parameters and returns a borrowed host-owned path.
    /// 无额外参数，返回借用的宿主自有路径。
    #[cfg(test)]
    pub(crate) fn root(&self) -> &Path {
        self.paths.root()
    }

    /// Close only this metadata binding; never detach the shared VCE space or delete files.
    /// 仅关闭本元数据绑定；绝不分离共享 VCE 空间，也不删除文件。
    ///
    /// Takes no extra parameters and returns success when already absent or successfully closed.
    /// 无额外参数，已不存在或成功关闭时返回成功。
    pub(crate) async fn close(&self) -> Result<(), String> {
        // No external service owns this database, so there is no remote binding to disable. The in-process
        // handle must be closed instead: the engine keeps every opened database alive until
        // told otherwise, and on Windows an open file makes its containing directory undeletable, which
        // is exactly what a VCE layout change needs to do.
        // 没有外部服务持有该数据库，因此没有远程绑定需要停用。关闭进程内句柄：引擎会保持每个已打开的
        // 数据库存活直到被告知关闭，而在 Windows 上打开的文件会使其所在目录无法删除——而 VCE 的布局变更正是
        // 要删除它。
        self.host_runtime.local.close_sqlite_file(&self.binding);
        Ok(())
    }

    /// Initialize a new catalog or validate an existing version before any schema mutation.
    /// 初始化新目录，或在任何结构修改前校验现有版本。
    ///
    /// `existed` records whether the database existed before opening. Returns an explicit
    /// error for an unsupported or malformed existing catalog rather than overwriting it.
    /// `existed` 记录打开前数据库是否存在。现有目录不受支持或格式错误时返回明确错误，
    /// 而不是覆盖它。
    async fn initialize(&self, existed: bool) -> Result<(), String> {
        if !existed {
            self.execute("BEGIN IMMEDIATE; CREATE TABLE vce_storage_catalog_metadata(id INTEGER PRIMARY KEY CHECK(id = 1), schema_version INTEGER NOT NULL CHECK(schema_version > 0)); INSERT INTO vce_storage_catalog_metadata VALUES (1, 1); CREATE TABLE vce_storage_projects(project_id TEXT PRIMARY KEY, project_key TEXT NOT NULL UNIQUE, relative_root TEXT NOT NULL UNIQUE, layout_version INTEGER NOT NULL CHECK(layout_version > 0), location_version INTEGER NOT NULL CHECK(location_version > 0), layout_state TEXT NOT NULL CHECK(layout_state IN ('allocated', 'published'))); COMMIT;", Vec::new()).await?;
        }
        // versions contains the one required catalog metadata row, not a content checkpoint.
        // versions 包含唯一必需的目录元数据行，不是内容检查点。
        let versions: Vec<CatalogVersionRow> = self
            .query(
                "SELECT schema_version FROM vce_storage_catalog_metadata WHERE id = 1;",
                Vec::new(),
            )
            .await?;
        if versions.len() != 1 || versions[0].schema_version != VCE_CATALOG_SCHEMA_VERSION {
            return Err(String::from(
                "VCE storage catalog schema is missing or unsupported",
            ));
        }
        Ok(())
    }

    /// Executes a bounded metadata mutation through the host runtime and validates its result.
    /// 通过宿主运行时执行有界元数据修改并校验结果。
    ///
    /// `sql` and `parameters` are exact catalog-owned statements and bindings. Returns explicit errors.
    /// `sql` 与 `parameters` 是目录自有的精确语句和绑定值。失败时返回明确错误。
    async fn execute(&self, sql: &str, parameters: Vec<SqliteValue>) -> Result<(), String> {
        local_sql::execute_script_async(&self.host_runtime.local, &self.binding, sql, parameters)
            .await
            .map_err(|error| format!("Write VCE storage catalog: {error}"))?;
        Ok(())
    }

    /// Decodes an exact metadata query through the existing host row-array contract.
    /// 通过现有宿主行数组契约解码精确元数据查询。
    ///
    /// `sql` and `parameters` select catalog-owned metadata; returns typed rows or an explicit error.
    /// `sql` 与 `parameters` 选择目录自有元数据；返回类型化行或明确错误。
    async fn query<T: for<'de> Deserialize<'de>>(
        &self,
        sql: &str,
        parameters: Vec<SqliteValue>,
    ) -> Result<Vec<T>, String> {
        let result =
            local_sql::query_json_async(&self.host_runtime.local, &self.binding, sql, parameters)
                .await
                .map_err(|error| format!("Read VCE storage catalog: {error}"))?;
        serde_json::from_str(&result.json_data)
            .map_err(|error| format!("Decode VCE storage catalog: {error}"))
    }
}

impl VceProjectStorageLocation {
    /// Validate location ownership against paths derived independently from the stable project ID.
    /// 针对由稳定项目 ID 独立派生的路径校验位置归属。
    ///
    /// `expected` is produced by the host path resolver. Returns success only for exact supported facts.
    /// `expected` 由宿主路径解析器产生。仅精确匹配且受支持的事实返回成功。
    fn validate(&self, expected: &VceProjectStoragePaths) -> Result<(), String> {
        if self.project_id != expected.project_id
            || self.project_key != expected.project_key
            || self.relative_root != format!("projects/{}", expected.project_key)
            || self.layout_version != 1
            || self.location_version == 0
        {
            return Err(String::from(
                "VCE catalog project identity, path, or layout is invalid",
            ));
        }
        Ok(())
    }
}
