//! Explicit physical replacement jobs admitted independently of a compatible query runtime.
//! 独立于兼容查询运行时准入的明确物理替换作业。

use super::*;
use vulcan_context_engine::VceIndexPort;
use vulcan_workspace_store::vce_data_plane::{VceJobReplacement, VceRebuildCandidateBase};

/// Current host facts required to build a new index; these never authorize reading the predecessor.
/// 构建新索引需要的当前宿主事实；这些事实绝不授权读取前驱向量。
#[derive(Clone)]
pub(crate) struct VceRebuildProjectInput {
    /// Stable project owner from the workbench.
    /// 来自工作台的稳定项目所有者。
    pub(crate) project_id: String,
    /// Authoritative source root, canonicalized again before admission.
    /// 权威源码根目录，在准入之前再次规范化。
    pub(crate) project_root: std::path::PathBuf,
    /// Complete saved project settings, including explicit enablement.
    /// 完整保存的项目设置，包含明确启用状态。
    pub(crate) settings: ProjectVceSettings,
    /// Explicit current dimension validated by the shared readiness resolver.
    /// 由共享就绪解析器校验的明确当前维度。
    pub(crate) dimension: u32,
    /// Current physical producer, distinct from the user's preview fingerprint.
    /// 当前物理生产者，与用户预览指纹分别保留。
    pub(crate) fingerprint: String,
}

impl VceRuntimeManager {
    /// Report whether the exact project owner is executing a durable physical replacement.
    /// 报告精确项目所有者是否正在执行持久物理替换。
    /// `project_id` selects the metadata slot; returns false without allocating a runtime or slot.
    /// `project_id` 选择元数据槽位；无运行时或槽位分配地返回判断。
    pub(in crate::vce::manager) async fn project_has_replacement_job(
        &self,
        project_id: &str,
    ) -> bool {
        // state/job clones release outer locks before reading the acknowledged receipt.
        // state/job 克隆在读取已确认回执之前释放外层锁。
        let state = self
            .project_job_states
            .lock()
            .await
            .get(project_id)
            .cloned();
        let Some(state) = state else { return false };
        let job = state.active.lock().await.clone();
        let Some(job) = job else { return false };
        let receipt = job.receipt.lock().await;
        let uninitialized_rebuild = receipt.kind == VceJobKind::Rebuild
            && receipt.source_checkpoint_version.is_none()
            && receipt.replacement.is_none();
        uninitialized_rebuild || receipt.replacement.is_some()
    }

    /// Preserve new saved policy while a replacement owns a project without a query runtime.
    /// 替换持有尚无查询运行时的项目时，保留新保存的策略。
    /// `project_id` and `project_root` identify ownership; `settings` and `fingerprint` are current saved policy.
    /// `project_id` 与 `project_root` 标识归属；`settings` 与 `fingerprint` 为当前已保存策略。
    /// Returns none without a replacement, otherwise policy compatibility or a stale/invalid fact error.
    /// 无替换时返回空值，否则返回策略兼容性或过期／无效事实错误。
    /// The caller holds the project lifecycle gate; the job gate serializes policy with terminal handoff.
    /// 调用方持有项目生命周期闸门；作业闸门串行化策略与终态交接。
    pub(in crate::vce::manager) async fn reconcile_replacement_policy(
        &self,
        project_id: &str,
        project_root: &Path,
        settings: &ProjectVceSettings,
        fingerprint: &str,
    ) -> Result<Option<bool>, String> {
        // state lookup must not allocate slots or storage for cold historical projects.
        // state 查询不得为冷态历史项目分配槽位或存储。
        let state = self
            .project_job_states
            .lock()
            .await
            .get(project_id)
            .cloned();
        let Some(state) = state else { return Ok(None) };
        // admission serializes the exact job reference and policy with replacement installation.
        // admission 将精确作业引用及策略与替换安装串行化。
        let _admission = state.admission.lock().await;
        // job is retained independently of query runtime registration.
        // job 独立于查询运行时注册保留。
        let job = state.active.lock().await.clone();
        let Some(job) = job else { return Ok(None) };
        // receipt supplies the immutable physical target, not the mutable policy revision.
        // receipt 提供不可变物理目标，而不是可变策略修订。
        let receipt = job.receipt.lock().await.clone();
        let uninitialized_rebuild = receipt.kind == VceJobKind::Rebuild
            && receipt.source_checkpoint_version.is_none()
            && receipt.replacement.is_none();
        if receipt.replacement.is_none() && !uninitialized_rebuild {
            return Ok(None);
        }
        // policy exists only for the explicit replacement variant proved by the receipt above.
        // policy 仅存在于上方回执已证明的明确替换分支。
        let policy = job
            .replacement_settings
            .as_ref()
            .ok_or_else(|| String::from("VCE replacement has no owned handoff settings"))?;
        // latest retains monotonic accepted settings without rewriting the stored job contract.
        // latest 保留单调接纳的设置，不重写存储作业契约。
        let mut latest = policy.lock().await;
        if settings.revision < latest.revision
            || (settings.revision == latest.revision && settings != &*latest)
        {
            return Err(String::from(
                "VCE replacement policy reconciliation snapshot is stale or conflicting",
            ));
        }
        // root is resolved from actual host state, never from a guessed source or predecessor field.
        // root 从实际宿主状态解析，绝不取自猜测的源码或前驱字段。
        let root = match std::fs::canonicalize(project_root) {
            Ok(root) => root,
            Err(error) => {
                job.cancel_for_shutdown();
                return Err(format!("Resolve VCE replacement policy root: {error}"));
            }
        };
        let target_matches = match receipt.replacement.as_ref() {
            Some(replacement) => {
                replacement.target.physical_fingerprint == fingerprint
                    && replacement.target.root_path
                        == crate::normalized_workspace_path_text_from_path(&root)
            }
            // A null-checkpoint rebuild has no initialized predecessor target; validate the fresh
            // active binding itself so a live provider or root change revokes this attempt.
            // 空检查点重建没有已初始化前驱目标；直接校验全新的活动绑定，确保实时供应商或根目录变化会撤销本次尝试。
            None => {
                let current = self
                    .data_plane
                    .load_active_project_config(project_id)
                    .await?;
                let Some(current) = current else {
                    job.cancel_for_shutdown();
                    return Err(String::from(
                        "VCE null-checkpoint rebuild active binding disappeared during policy reconciliation",
                    ));
                };
                current.root_path == crate::normalized_workspace_path_text_from_path(&root)
                    && current
                        .binding
                        .embedding_space
                        .physical_fingerprint
                        .as_deref()
                        == Some(fingerprint)
            }
        };
        if !replacement_policy_matches(&latest, settings) || !target_matches {
            // Index-affecting changes revoke this exact attempt, even when no query runtime exists to disable.
            // 影响索引的变化撤销此精确尝试，即使没有可禁用的查询运行时。
            job.cancel_for_shutdown();
            return Ok(Some(false));
        }
        *latest = settings.clone();
        Ok(Some(true))
    }

    /// Admit confirmed rebuilding using ordinary runtime work or an independently owned replacement.
    /// 使用普通运行时工作或独立所有者替换准入已确认重建。
    /// `input` freezes host facts, `command` freezes the preview, and `repair`/`action_ids` freeze intent.
    /// `input` 冻结宿主事实，`command` 冻结预览，`repair`／`action_ids` 冻结意图。
    /// `executor` retains work beyond the request; returns a durable receipt before scanning.
    /// `executor` 在请求结束后保留工作；扫描之前返回持久回执。
    pub(crate) async fn submit_project_rebuild(
        &self,
        input: VceRebuildProjectInput,
        command: VceJobAdmission,
        repair: bool,
        action_ids: BTreeSet<String>,
        executor: &tokio::runtime::Runtime,
    ) -> Result<VceStoredJob, String> {
        input.settings.validate()?;
        if !input.settings.enabled {
            return Err(String::from("VCE rebuilding requires an enabled project"));
        }
        // root is a host fact, not the possibly older runtime's source path.
        // root 为宿主事实，而不是可能较旧的运行时源码路径。
        let root = std::fs::canonicalize(&input.project_root)
            .map_err(|error| format!("Resolve VCE rebuild project root: {error}"))?;
        // A compatible registered engine preserves its concurrent readers through the existing path.
        // 兼容的已注册引擎通过既有路径保留并发读取方。
        // A null checkpoint is the explicit first-build recovery branch and never enters the initialized candidate path.
        // 空检查点明确表示首次构建恢复分支，绝不进入已初始化候选路径。
        if command.expected_checkpoint_version.is_none() {
            return self
                .submit_uninitialized_rebuild_job(input, command, repair, action_ids, executor)
                .await;
        }
        let runtime = self.projects.read().await.get(&input.project_id).cloned();
        if let Some(runtime) = runtime
            && runtime.ai.embedding_space().dimension == input.dimension as usize
            && runtime.source.project_root() == root
            && runtime.ai.embedding_space().physical_fingerprint.as_deref()
                == Some(input.fingerprint.as_str())
            && runtime.host_status.read().await.lifecycle != VceProjectLifecycle::RebuildRequired
        {
            return self
                .submit_confirmed_rebuild(&input.project_id, command, repair, action_ids, executor)
                .await;
        }
        self.submit_replacement_job(input, command, repair, action_ids, executor)
            .await
    }

    /// Admit a null-checkpoint rebuild against an active or explicitly orphaned uninitialized binding.
    /// 为活动或明确孤立的未初始化绑定准入空检查点重建。
    ///
    /// `input` carries current host facts, `command` carries the null-checkpoint preview,
    /// `repair` and `action_ids` preserve the exact maintenance intent, and `executor` owns
    /// execution after the durable receipt is acknowledged. Returns the queued receipt.
    /// `input` 携带当前宿主事实，`command` 携带空检查点预览，`repair` 与 `action_ids` 保留精确维护意图，
    /// `executor` 在持久回执确认后负责执行。返回排队回执。
    ///
    /// The storage factory returns the fresh index and the same writer lease together, so no
    /// competing host can enter between retirement, fresh binding creation and receipt admission.
    /// 存储工厂同时返回新索引与同一写入租约，避免退役、新绑定创建和回执准入之间出现竞争宿主。
    async fn submit_uninitialized_rebuild_job(
        &self,
        input: VceRebuildProjectInput,
        command: VceJobAdmission,
        repair: bool,
        action_ids: BTreeSet<String>,
        executor: &tokio::runtime::Runtime,
    ) -> Result<VceStoredJob, String> {
        input.settings.validate()?;
        validate_uninitialized_rebuild_admission(
            &command,
            input.settings.revision,
            &input.fingerprint,
        )?;
        if repair {
            return Err(String::from(
                "VCE repair requires a published checkpoint and cannot use null recovery",
            ));
        }
        if !input.settings.enabled {
            return Err(String::from("VCE rebuilding requires an enabled project"));
        }
        if command.idempotency_key.trim().is_empty() || command.idempotency_key.len() > 512 {
            return Err(String::from(
                "VCE job idempotency key must contain 1..512 bytes",
            ));
        }
        let work = VceJobWork::Rebuild { repair, action_ids };
        let job_id = Uuid::new_v4().to_string();
        let prepared = runtime_configuration::prepare_runtime_configuration(
            &input.project_id,
            &input.project_root,
            &input.settings,
            input.dimension,
            &input.fingerprint,
            Some(&job_id),
        )?;
        let request_fingerprint = job_request_fingerprint(
            &input.project_id,
            prepared.source.project_root(),
            &command,
            &work,
        )?;
        let _lifecycle = self.lifecycle_gate.read().await;
        let project_gate = self.project_lifecycle_gate(&input.project_id).await;
        let _project_gate = project_gate.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(String::from("VCE job manager is shutting down"));
        }
        let state = self.project_job_state(&input.project_id).await;
        let admission = state.admission.lock().await;
        // An active job store is authoritative for an in-flight replay and must be checked before active-job rejection.
        // 活动作业存储是进行中重放的权威来源，必须先于活动作业拒绝进行查询。
        if let Some(active_job) = state.active.lock().await.clone() {
            if let Some(receipt) = active_job
                .store
                .find_submission(&command.idempotency_key, &request_fingerprint)
                .await?
            {
                return Ok(receipt);
            }
            return Err(String::from(
                "VCE project already has an admitted or stopping index job",
            ));
        }
        let (previous, predecessor, predecessor_is_active) = if let Some(config) = self
            .data_plane
            .load_active_project_config(&input.project_id)
            .await?
        {
            let index = self
                .data_plane
                .open_existing_project_index(&config)
                .await?
                .ok_or_else(|| String::from("VCE uninitialized predecessor disappeared"))?;
            (config, index, true)
        } else {
            let config = self
                .data_plane
                .load_uninitialized_recovery_config(&input.project_id)
                .await?
                .ok_or_else(|| {
                    String::from("VCE uninitialized rebuild requires an active or orphaned binding")
                })?;
            let index = self
                .data_plane
                .open_uninitialized_recovery_project_index(&config)
                .await?
                .ok_or_else(|| String::from("VCE uninitialized recovery index disappeared"))?;
            (config, index, false)
        };
        let store = predecessor.job_store();
        // The project job store is checked before predecessor checkpoint and plan guards, so completed null recovery can replay after namespace replacement.
        // 在检查前驱检查点与计划保护前先查询项目作业存储，使命名空间替换后的已完成空值恢复仍可重放。
        if let Some(receipt) = store
            .find_submission(&command.idempotency_key, &request_fingerprint)
            .await?
        {
            return Ok(receipt);
        }
        if state.stop_requested.load(Ordering::Acquire) {
            return Err(String::from(
                "VCE project is stopping and requires explicit recovery",
            ));
        }
        predecessor.require_write_access()?;
        let provenance = if predecessor_is_active {
            predecessor.embedding_provenance().await?
        } else {
            predecessor.uninitialized_recovery_provenance().await?
        };
        if provenance.checkpoint_version.is_some() {
            return Err(String::from(
                "VCE null-checkpoint rebuild requires an uninitialized predecessor",
            ));
        }
        validate_stored_rebuild_confirmation(&predecessor, &previous, &work).await?;
        let preview_contract = uninitialized_publication_contract(&input, &prepared, None);
        {
            let _configuration = crate::begin_public_ai_configuration_transaction().await;
            (self.publication_authority)(&preview_contract)?;
        }
        // Runtime teardown must happen before storage fencing, while the project gate still excludes successors.
        // 存储封锁前必须先拆卸运行时，项目闸门仍会排斥后继准入。
        drop(admission);
        let previous_runtime = self.projects.write().await.remove(&input.project_id);
        if let Some(previous_runtime) = previous_runtime {
            self.stop_runtime(&previous_runtime).await?;
        }
        let _admission = state.admission.lock().await;
        if state.stop_requested.load(Ordering::Acquire)
            || self.shutting_down.load(Ordering::Acquire)
            || state.active.lock().await.is_some()
        {
            return Err(String::from(
                "VCE project stopped before uninitialized rebuild admission",
            ));
        }
        let (index, writer) = self
            .data_plane
            .reinitialize_uninitialized_project_index(&previous, prepared.index.clone())
            .await?;
        index.require_write_access()?;
        let project_writer = Arc::new(writer);
        let store = index.job_store();
        if let Some(previous) = store.unfinished().await? {
            // The new project writer is exclusive after the in-memory owner slot was checked empty,
            // so an unfinished receipt from an ended host run is safe to reconcile here.
            // 内存所有者槽确认为空后已取得新的项目写入租约，因此可在此安全收敛已结束宿主留下的未完成回执。
            let reason = if previous.owner_run_id == self.job_owner_run_id {
                "VCE previous replacement job ended before its terminal receipt was acknowledged"
            } else {
                "VCE previous host replacement run ended before its terminal receipt was acknowledged"
            };
            store.interrupt_unfinished(reason).await?;
        }
        let previous_time = store
            .latest()
            .await?
            .map(|job| job.created_at_unix_ms)
            .unwrap_or(0);
        let timestamp = job_time()?.max(
            previous_time
                .checked_add(1)
                .ok_or_else(|| String::from("VCE job admission timestamp overflow"))?,
        );
        let queued = VceStoredJob {
            schema_version: vulcan_workspace_store::vce_data_plane::VCE_JOB_RECEIPT_SCHEMA_VERSION,
            project_id: input.project_id.clone(),
            job_id: job_id.clone(),
            owner_run_id: self.job_owner_run_id.clone(),
            idempotency_key: command.idempotency_key,
            retry_of_job_id: None,
            request_fingerprint,
            contract_fingerprint: job_index_contract_fingerprint(
                &prepared.engine,
                &input.settings,
            )?,
            kind: VceJobKind::Rebuild,
            state: VceJobState::Queued,
            sequence: 1,
            created_at_unix_ms: timestamp,
            updated_at_unix_ms: timestamp,
            admitted_settings_revision: input.settings.revision,
            source_checkpoint_version: None,
            replacement: None,
            compaction: None,
            result_checkpoint_version: None,
            progress: None,
            followup_requested: false,
            completed_update: None,
            error: None,
        };
        let receipt = store.register(&queued).await?;
        let execution = Arc::clone(&state.execution).read_owned().await;
        let job = Arc::new(VceManagedJob {
            job_id: receipt.job_id.clone(),
            replacement_settings: Some(Mutex::new(input.settings.clone())),
            project_writer,
            store: store.clone(),
            event_emitter: self.event_emitter.clone(),
            receipt: Mutex::new(receipt.clone()),
            persistence_gate: Mutex::new(()),
            control: VceRefreshControl::default(),
            cancel_wake: Notify::new(),
            persistence_error: Mutex::new(None),
            pending_terminal_receipt: Mutex::new(None),
            #[cfg(test)]
            compaction_observation_persistence_failures: std::sync::atomic::AtomicUsize::new(0),
        });
        *state.active.lock().await = Some(Arc::clone(&job));
        self.event_emitter
            .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                super::super::super::services::map_vce_job(receipt.clone()),
            )));
        let manager = self.clone();
        let worker_state = Arc::clone(&state);
        let completion_state = Arc::clone(&state);
        let worker = executor.spawn(async move {
            let _execution = execution;
            manager
                .run_uninitialized_rebuild_job(input, prepared, index, worker_state, job, work)
                .await;
            // Release the execution lease before removing the worker handle used by shutdown.
            // 在移除关闭流程使用的工作句柄之前，先释放执行租约。
            drop(_execution);
            // The consumed job and index are released by the callee before this worker becomes clearable.
            // 被消耗的作业与索引由被调用函数在此工作线程可清除前释放。
            completion_state.clear_worker_handle().await;
        });
        state.install_worker_handle(worker).await;
        Ok(receipt)
    }

    /// Persist a confirmed new-space target before any model registration, source scan or table allocation.
    /// 在任何模型注册、源码扫描或表分配之前持久化已确认新空间目标。
    /// Inputs carry host facts, preview guards and exact actions; returns the acknowledged queued receipt.
    /// 输入携带宿主事实、预览保护及精确动作；返回已确认排队回执。
    async fn submit_replacement_job(
        &self,
        input: VceRebuildProjectInput,
        command: VceJobAdmission,
        repair: bool,
        action_ids: BTreeSet<String>,
        executor: &tokio::runtime::Runtime,
    ) -> Result<VceStoredJob, String> {
        input.settings.validate()?;
        if !input.settings.enabled
            || command.kind != VceJobKind::Rebuild
            || command.retry_of_job_id.is_some()
        {
            return Err(String::from(
                "VCE replacement requires an enabled project and explicit rebuild confirmation",
            ));
        }
        if command.idempotency_key.trim().is_empty() || command.idempotency_key.len() > 512 {
            return Err(String::from(
                "VCE job idempotency key must contain 1..512 bytes",
            ));
        }
        // lifecycle gates exclude new admissions, removal and shutdown until the worker owns execution.
        // 生命周期闸门排斥新准入、移除及退出，直到工作任务持有执行权。
        let _lifecycle = self.lifecycle_gate.read().await;
        let project_gate = self.project_lifecycle_gate(&input.project_id).await;
        let _project_gate = project_gate.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(String::from("VCE job manager is shutting down"));
        }
        // prepared contains metadata only, with an exact fresh target that cannot alter the active binding.
        // prepared 仅包含元数据，具有无法改变活动绑定的精确新目标。
        let job_id = Uuid::new_v4().to_string();
        let prepared = runtime_configuration::prepare_runtime_configuration(
            &input.project_id,
            &input.project_root,
            &input.settings,
            input.dimension,
            &input.fingerprint,
            Some(&job_id),
        )?;
        // original/index preserve the old physical dimension without activating its model route.
        // original/index 保留旧物理维度，不激活其模型路由。
        let original = self
            .data_plane
            .load_active_project_config(&input.project_id)
            .await?
            .ok_or_else(|| {
                String::from("VCE replacement requires an existing initialized index")
            })?;
        let index = self
            .data_plane
            .open_existing_project_index(&original)
            .await?
            .ok_or_else(|| String::from("VCE replacement predecessor disappeared"))?;
        let store = index.job_store();
        let work = VceJobWork::Rebuild { repair, action_ids };
        let request_fingerprint = job_request_fingerprint(
            &input.project_id,
            prepared.source.project_root(),
            &command,
            &work,
        )?;
        // Replays must precede current checkpoint, route and plan checks because successful replacement changes them.
        // 重放必须先于当前检查点、路由及计划校验，因为成功替换会改变这些事实。
        if let Some(receipt) = store
            .find_submission(&command.idempotency_key, &request_fingerprint)
            .await?
        {
            return Ok(receipt);
        }
        let state = self.project_job_state(&input.project_id).await;
        let admission = state.admission.lock().await;
        if state.stop_requested.load(Ordering::Acquire) || state.active.lock().await.is_some() {
            return Err(String::from(
                "VCE project already has an admitted or stopping index job",
            ));
        }
        index.require_write_access()?;
        // Own the project across predecessor teardown, queueing, allocation and receipt finalization.
        // 跨前驱拆卸、排队、分配及回执终态处理持有项目归属。
        let project_writer = Arc::new(
            self.data_plane
                .try_acquire_project_writer(&input.project_id)?
                .ok_or_else(|| String::from("VCE project already has an active host writer"))?,
        );
        if let Some(previous) = store.unfinished().await? {
            // The project writer proves that no live host owns the old replacement receipt.
            // 项目写入租约证明旧替换回执不再由活动宿主持有。
            let reason = if previous.owner_run_id == self.job_owner_run_id {
                "VCE previous replacement job ended before its terminal receipt was acknowledged"
            } else {
                "VCE previous host replacement run ended before its terminal receipt was acknowledged"
            };
            store.interrupt_unfinished(reason).await?;
        }
        let provenance = index.embedding_provenance().await?;
        validate_job_admission(
            &command,
            input.settings.revision,
            provenance.checkpoint_version,
            Some(&input.fingerprint),
        )?;
        validate_stored_rebuild_confirmation(&index, &original, &work).await?;
        let checkpoint = provenance
            .checkpoint_version
            .ok_or_else(|| String::from("VCE replacement predecessor is not initialized"))?;
        let previous_time = store
            .latest()
            .await?
            .map(|job| job.created_at_unix_ms)
            .unwrap_or(0);
        let timestamp = job_time()?.max(
            previous_time
                .checked_add(1)
                .ok_or_else(|| String::from("VCE job admission timestamp overflow"))?,
        );
        let queued = VceStoredJob {
            schema_version: vulcan_workspace_store::vce_data_plane::VCE_JOB_RECEIPT_SCHEMA_VERSION,
            project_id: input.project_id.clone(),
            job_id: job_id.clone(),
            owner_run_id: self.job_owner_run_id.clone(),
            idempotency_key: command.idempotency_key,
            retry_of_job_id: None,
            request_fingerprint,
            contract_fingerprint: job_index_contract_fingerprint(
                &prepared.engine,
                &input.settings,
            )?,
            kind: VceJobKind::Rebuild,
            state: VceJobState::Queued,
            sequence: 1,
            created_at_unix_ms: timestamp,
            updated_at_unix_ms: timestamp,
            admitted_settings_revision: input.settings.revision,
            source_checkpoint_version: Some(checkpoint),
            replacement: Some(Box::new(VceJobReplacement {
                predecessor: VceRebuildCandidateBase {
                    namespace_id: original.binding.namespace_id.clone(),
                    checkpoint_version: checkpoint,
                    job_id,
                },
                target: prepared.index.clone(),
            })),
            compaction: None,
            result_checkpoint_version: None,
            progress: None,
            followup_requested: false,
            completed_update: None,
            error: None,
        };
        let contract = replacement_publication_contract(&input, &prepared, provenance, &queued);
        {
            let _configuration = crate::begin_public_ai_configuration_transaction().await;
            (self.publication_authority)(&contract)?;
        }
        // stop_runtime takes admission itself; no worker exists and the project lifecycle gate excludes new ones.
        // stop_runtime 自行获取准入锁；此时没有工作任务，项目生命周期闸门排斥新任务。
        drop(admission);
        let previous = self.projects.write().await.remove(&input.project_id);
        if let Some(previous) = previous {
            self.stop_runtime(&previous).await?;
        }
        let _admission = state.admission.lock().await;
        if state.stop_requested.load(Ordering::Acquire)
            || self.shutting_down.load(Ordering::Acquire)
        {
            return Err(String::from(
                "VCE project stopped before replacement admission",
            ));
        }
        // execution covers budget wait, allocation, publication, model handoff and terminal persistence.
        // execution 覆盖预算等待、分配、发布、模型交接及终态持久化。
        let execution = Arc::clone(&state.execution).read_owned().await;
        let receipt = store.register(&queued).await?;
        let job = Arc::new(VceManagedJob {
            job_id: receipt.job_id.clone(),
            replacement_settings: Some(Mutex::new(input.settings.clone())),
            project_writer,
            store: store.clone(),
            event_emitter: self.event_emitter.clone(),
            receipt: Mutex::new(receipt.clone()),
            persistence_gate: Mutex::new(()),
            control: VceRefreshControl::default(),
            cancel_wake: Notify::new(),
            persistence_error: Mutex::new(None),
            pending_terminal_receipt: Mutex::new(None),
            #[cfg(test)]
            compaction_observation_persistence_failures: std::sync::atomic::AtomicUsize::new(0),
        });
        *state.active.lock().await = Some(Arc::clone(&job));
        self.event_emitter
            .emit(crate::ServiceEvent::VceJobChanged(Box::new(
                super::super::super::services::map_vce_job(receipt.clone()),
            )));
        let manager = self.clone();
        let worker_state = Arc::clone(&state);
        let completion_state = Arc::clone(&state);
        let worker = executor.spawn(async move {
            let _execution = execution;
            manager
                .run_replacement_job(input, prepared, index, worker_state, job, work)
                .await;
            // Release the execution lease before removing the worker handle used by shutdown.
            // 在移除关闭流程使用的工作句柄之前，先释放执行租约。
            drop(_execution);
            // The consumed job and index are released by the callee before this worker becomes clearable.
            // 被消耗的作业与索引由被调用函数在此工作线程可清除前释放。
            completion_state.clear_worker_handle().await;
        });
        state.install_worker_handle(worker).await;
        Ok(receipt)
    }

    /// Run a fresh initial binding and restore its published target without using the project lifecycle gate.
    /// 执行全新的初始绑定，并在不使用项目生命周期闸门的情况下恢复已发布目标。
    ///
    /// `input`, `prepared` and `index` are the exact admitted target; `state` and `job` retain
    /// cancellation, receipt and writer ownership. Returns no value because terminal state is
    /// persisted through the shared managed-job boundary.
    /// `input`、`prepared` 与 `index` 是精确准入目标；`state` 与 `job` 保留取消、回执及写入归属。
    /// 无返回值，因为终态通过共享托管作业边界持久化。
    async fn run_uninitialized_rebuild_job(
        &self,
        input: VceRebuildProjectInput,
        prepared: runtime_configuration::VcePreparedRuntimeConfiguration,
        index: VceProjectIndex,
        state: Arc<VceProjectJobState>,
        job: Arc<VceManagedJob>,
        work: VceJobWork,
    ) {
        let mut runtime: Option<Arc<VceProjectRuntime>> = None;
        let attempted = AssertUnwindSafe(async {
            let notified = job.cancel_wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            job.control.check().map_err(|error| error.to_string())?;
            let budget = tokio::select! {
                permit = Arc::clone(&self.job_writer_budget).acquire_owned() => permit.map_err(|_| String::from("VCE writer budget was closed"))?,
                _ = notified => return Err(String::from(VCE_REFRESH_CANCELLED_ERROR)),
            };
            job.control.check().map_err(|error| error.to_string())?;
            let running = job
                .persist(&job.store, |receipt| {
                    if receipt.state == VceJobState::Queued {
                        receipt.state = VceJobState::Running;
                    }
                })
                .await?;
            let provenance = index.embedding_provenance().await?;
            if provenance.checkpoint_version.is_some() {
                return Err(String::from(
                    "VCE null-checkpoint rebuild target was initialized before execution",
                ));
            }
            let contract = uninitialized_publication_contract(
                &input,
                &prepared,
                Some(provenance),
            );
            {
                let _configuration = crate::begin_public_ai_configuration_transaction().await;
                (self.publication_authority)(&contract)?;
            }
            if matches!(&work, VceJobWork::Rebuild { repair: true, .. }) {
                index.retry_pending_cleanup().await?;
                if index.has_unresolved_staging().await? {
                    return Err(String::from(
                        "VCE repair requires confirmed ownership of unresolved staging",
                    ));
                }
            }
            let active = self
                .activate_uninitialized_writer(&input, prepared, index)
                .await?;
            runtime = Some(Arc::clone(&active));
            let _writer = active.refresh_gate.lock().await;
            self.run_managed_job(
                &active,
                &job,
                &job.store,
                VceJobWork::Manual,
                running.job_id,
                Some(budget),
            )
            .await
        })
        .catch_unwind()
        .await;
        let mut outcome = match attempted {
            Ok(result) => result,
            Err(_) => Err(String::from(
                "VCE initial rebuild worker panicked; explicit recovery may be required",
            )),
        };
        let published_checkpoint = outcome.as_ref().ok().copied();
        if let Some(active) = &runtime {
            // A failed initial job leaves the active fresh namespace uninitialized so the next
            // explicit rebuild can fence it with another fresh namespace.
            // 初始作业失败时保留全新的活动命名空间为未初始化状态，下一次显式重建可再次以新命名空间封锁它。
            if let Err(execution_error) = &outcome
                && let Ok(provenance) = active.index.embedding_provenance().await
                && provenance.checkpoint_version.is_some()
            {
                outcome = Err(format!(
                    "{execution_error}; initial rebuild unexpectedly published a checkpoint"
                ));
            }
            if let Err(error) = active.ai.deactivate() {
                outcome = Err(format!(
                    "VCE initial rebuild model registration cleanup failed: {error}; publication result: {outcome:?}"
                ));
            }
        }
        // Completion accounting must not hold admission while shutdown waits for the worker handle.
        // 完成计账不能持有准入锁，否则退出会在等待工作句柄前卡住。
        if let Some(checkpoint) = published_checkpoint {
            let accounting = async {
                let now = job_time()?;
                job.persist(&job.store, |receipt| {
                    receipt.completed_update = Some(VceCompletedJobUpdate {
                        checkpoint_version: checkpoint,
                        at_unix_ms: now.max(receipt.updated_at_unix_ms),
                    });
                })
                .await?;
                Ok::<(), String>(())
            }
            .await;
            if let Err(error) = accounting {
                outcome = Err(format!(
                    "VCE initial rebuild committed but completion accounting failed: {error}"
                ));
            }
        }
        let mut restored = None;
        // Reacquire admission only after the receipt write so shutdown can fence and abort a locked receipt.
        // 仅在回执写入后重新获取准入锁，使退出可以封锁并中止锁住的回执写入。
        let _admission = state.admission.lock().await;
        if outcome.is_ok()
            && !state.stop_requested.load(Ordering::Acquire)
            && !self.shutting_down.load(Ordering::Acquire)
        {
            match self.restore_replacement_runtime(&input, &job).await {
                Ok(restored_runtime) => {
                    self.projects
                        .write()
                        .await
                        .insert(input.project_id.clone(), Arc::clone(&restored_runtime));
                    let checkpoint = restored_runtime
                        .committed_projection
                        .read()
                        .await
                        .engine
                        .checkpoint_version;
                    match self
                        .publish_query_admission(&restored_runtime, checkpoint)
                        .await
                    {
                        Ok(()) => {
                            self.activation_failures
                                .write()
                                .await
                                .remove(&input.project_id);
                            restored = Some(restored_runtime);
                        }
                        Err(error) => {
                            self.projects.write().await.remove(&input.project_id);
                            let cleanup = restored_runtime.ai.deactivate();
                            outcome = Err(format!(
                                "VCE initial rebuild published but query admission failed: {error}; registration cleanup: {cleanup:?}"
                            ));
                        }
                    }
                }
                Err(error) => {
                    outcome = Err(format!(
                        "VCE initial rebuild published but runtime restoration failed: {error}"
                    ));
                }
            }
        }
        if let Err(error) = &outcome {
            // Keep failed or cancelled fresh bindings visibly recoverable after the worker releases its active slot.
            // 工作线程释放活动槽位后，仍将失败或取消的全新绑定明确保留为可恢复状态。
            self.activation_failures.write().await.insert(
                input.project_id.clone(),
                VceActivationFailure {
                    config_revision: input.settings.revision,
                    lifecycle: VceProjectLifecycle::RebuildRequired,
                    error: bounded_error(error.clone()),
                },
            );
        }
        // Terminal receipt persistence must not hold admission while shutdown waits for the worker handle.
        // 终态回执持久化不能继续持有准入锁，否则退出无法通过工作句柄排空锁住的回执写入。
        drop(_admission);
        self.finish_managed_job(None, &job, &job.store, outcome)
            .await;
        if job.persistence_error.lock().await.is_none() {
            *state.active.lock().await = None;
        }
        if let Some(restored) = restored {
            self.reconcile_runtime_watcher(&restored).await;
        }
    }

    /// Run the owned replacement and restore its published target without using the project lifecycle gate.
    /// 执行有归属替换并恢复其已发布目标，不使用项目生命周期闸门。
    /// Inputs retain exact target, predecessor and receipt ownership; completion always settles the durable job.
    /// 输入保留精确目标、前驱及回执归属；完成始终确定持久作业结果。
    async fn run_replacement_job(
        &self,
        input: VceRebuildProjectInput,
        prepared: runtime_configuration::VcePreparedRuntimeConfiguration,
        predecessor: VceProjectIndex,
        state: Arc<VceProjectJobState>,
        job: Arc<VceManagedJob>,
        work: VceJobWork,
    ) {
        // candidate stays owned outside the unwind boundary so a failed task can revoke its precise unpublished target.
        // candidate 保留在展开边界之外，使失败任务能够撤销精确未发布目标。
        let mut candidate: Option<Arc<VceProjectRuntime>> = None;
        let attempted = AssertUnwindSafe(async {
            let notified = job.cancel_wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            job.control.check().map_err(|error| error.to_string())?;
            let budget = tokio::select! {
                permit = Arc::clone(&self.job_writer_budget).acquire_owned() => permit.map_err(|_| String::from("VCE writer budget was closed"))?,
                _ = notified => return Err(String::from(VCE_REFRESH_CANCELLED_ERROR)),
            };
            job.control.check().map_err(|error| error.to_string())?;
            let running = job.persist(&job.store, |receipt| {
                if receipt.state == VceJobState::Queued { receipt.state = VceJobState::Running; }
            }).await?;
            let provenance = predecessor.embedding_provenance().await?;
            let contract = replacement_publication_contract(&input, &prepared, provenance, &running);
            {
                let _configuration = crate::begin_public_ai_configuration_transaction().await;
                (self.publication_authority)(&contract)?;
            }
            job.control.check().map_err(|error| error.to_string())?;
            if matches!(&work, VceJobWork::Rebuild { repair: true, .. }) {
                predecessor.retry_pending_cleanup().await?;
                if predecessor.has_unresolved_staging().await? {
                    return Err(String::from("VCE repair requires confirmed ownership of unresolved staging"));
                }
            }
            // runtime is private and unregistered in the query map until the target publishes successfully.
            // runtime 在目标成功发布之前保持私有，不注册到查询映射。
            let runtime = self.activate_replacement_writer(&input, prepared, &running).await?;
            candidate = Some(Arc::clone(&runtime));
            let _writer = runtime.refresh_gate.lock().await;
            self.run_managed_job(&runtime, &job, &job.store, VceJobWork::Manual, running.job_id, Some(budget)).await
        }).catch_unwind().await;
        let mut outcome = match attempted {
            Ok(result) => result,
            Err(_) => Err(String::from(
                "VCE replacement worker panicked; explicit target recovery may be required",
            )),
        };
        // A known successful publication remains an update even if later model cleanup or handoff fails.
        // 即使后续模型清理或交接失败，已知成功发布仍然属于一次更新。
        let published_checkpoint = outcome.as_ref().ok().copied();
        if let Some(runtime) = &candidate {
            if let Err(execution_error) = &outcome {
                // Acknowledged publication cannot be rolled back; only an uninitialized target may be cancelled.
                // 已确认发布不能回滚；仅未初始化目标可以取消。
                let cleanup = async {
                    let provenance = runtime.index.embedding_provenance().await?;
                    if provenance.checkpoint_version.is_none() {
                        let receipt = job.receipt.lock().await.clone();
                        let replacement = receipt
                            .replacement
                            .as_ref()
                            .ok_or_else(|| String::from("VCE replacement lost its target"))?;
                        runtime
                            .index
                            .cancel_rebuild_candidate(&replacement.predecessor)
                            .await?;
                        runtime.index.abort_refresh_staging(&receipt.job_id).await?;
                        runtime
                            .index
                            .cleanup_cancelled_rebuild_candidate(&replacement.predecessor)
                            .await?;
                    }
                    Ok::<(), String>(())
                }
                .await;
                if let Err(error) = cleanup {
                    outcome = Err(format!("{execution_error}; cleanup replacement: {error}"));
                }
            }
            // The private writer has no query callers; release only its own model registration, not project state.
            // 私有写入器没有查询调用方；仅释放自身模型注册，不释放项目状态。
            if let Err(error) = runtime.ai.deactivate() {
                outcome = Err(format!(
                    "VCE replacement model registration cleanup failed: {error}; publication result: {outcome:?}"
                ));
            }
        }
        // Completion accounting must not hold admission while shutdown waits for the worker handle.
        // 完成计账不能持有准入锁，否则退出会在等待工作句柄前卡住。
        if let Some(checkpoint) = published_checkpoint {
            let accounting = async {
                let now = job_time()?;
                job.persist(&job.store, |receipt| {
                    receipt.completed_update = Some(VceCompletedJobUpdate {
                        checkpoint_version: checkpoint,
                        at_unix_ms: now.max(receipt.updated_at_unix_ms),
                    })
                })
                .await?;
                Ok::<(), String>(())
            }
            .await;
            if let Err(error) = accounting {
                outcome = Err(format!(
                    "VCE replacement committed but completion accounting failed: {error}"
                ));
            }
        }
        // restored is retained for watcher reconciliation only after the terminal owner is settled.
        // restored 仅保留用于终态所有者确定之后的监听协调。
        // Reacquire admission only after the receipt write so shutdown can fence and abort a locked receipt.
        // 仅在回执写入后重新获取准入锁，使退出可以封锁并中止锁住的回执写入。
        let _admission = state.admission.lock().await;
        let mut restored = None;
        if outcome.is_ok()
            && !state.stop_requested.load(Ordering::Acquire)
            && !self.shutting_down.load(Ordering::Acquire)
        {
            match self.restore_replacement_runtime(&input, &job).await {
                Ok(runtime) => {
                    self.projects
                        .write()
                        .await
                        .insert(input.project_id.clone(), Arc::clone(&runtime));
                    let checkpoint = runtime
                        .committed_projection
                        .read()
                        .await
                        .engine
                        .checkpoint_version;
                    match self.publish_query_admission(&runtime, checkpoint).await {
                        Ok(()) => {
                            self.activation_failures
                                .write()
                                .await
                                .remove(&input.project_id);
                            restored = Some(runtime);
                        }
                        Err(error) => {
                            self.projects.write().await.remove(&input.project_id);
                            let cleanup = runtime.ai.deactivate();
                            outcome = Err(format!(
                                "VCE replacement published but query admission failed: {error}; registration cleanup: {cleanup:?}"
                            ));
                        }
                    }
                }
                Err(error) => {
                    outcome = Err(format!(
                        "VCE replacement published but runtime restoration failed: {error}"
                    ))
                }
            }
        }
        // Terminal receipt persistence must not hold admission while shutdown waits for the worker handle.
        // 终态回执持久化不能继续持有准入锁，否则退出无法通过工作句柄排空锁住的回执写入。
        drop(_admission);
        self.finish_managed_job(None, &job, &job.store, outcome)
            .await;
        if job.persistence_error.lock().await.is_none() {
            *state.active.lock().await = None;
        }
        if let Some(runtime) = restored {
            self.reconcile_runtime_watcher(&runtime).await;
        }
    }

    /// Restore the published target using the latest accepted policy, not the queued request snapshot.
    /// 使用最新已接纳策略恢复已发布目标，不使用排队请求快照。
    /// `input` retains physical identity and `job` owns policy; returns the restored runtime or an explicit error.
    /// `input` 保留物理身份，`job` 持有策略；返回恢复的运行时或明确错误。
    /// Caller retains job admission throughout restoration so a later setting update observes the installed runtime.
    /// 调用方在恢复期间持续持有作业准入锁，使后续设置更新观察到已安装运行时。
    async fn restore_replacement_runtime(
        &self,
        input: &VceRebuildProjectInput,
        job: &VceManagedJob,
    ) -> Result<Arc<VceProjectRuntime>, String> {
        // settings is the single reconciled snapshot for this handoff, with no persistence or model fallback.
        // settings 为此次交接唯一已协调快照，不执行持久化或模型兜底。
        let settings = job
            .replacement_settings
            .as_ref()
            .ok_or_else(|| String::from("VCE replacement has no owned handoff settings"))?
            .lock()
            .await
            .clone();
        self.activate_project(
            &input.project_id,
            &input.project_root,
            settings,
            input.dimension,
            &input.fingerprint,
            false,
        )
        .await
        .map_err(|error| error.error)
    }

    /// Register current models and activate the fresh namespace already fenced by the durable job.
    /// 注册当前模型，并激活持久作业已经封锁的全新命名空间。
    ///
    /// `input` and `prepared` carry the admitted execution contract and `index` is the exact
    /// active storage handle returned together with the writer lease. Returns a private runtime.
    /// `input` 与 `prepared` 携带已准入执行契约，`index` 是与写入租约一同返回的精确活动存储句柄。
    /// 返回私有运行时。
    async fn activate_uninitialized_writer(
        &self,
        input: &VceRebuildProjectInput,
        prepared: runtime_configuration::VcePreparedRuntimeConfiguration,
        index: VceProjectIndex,
    ) -> Result<Arc<VceProjectRuntime>, String> {
        if index.binding() != &prepared.index.binding {
            return Err(String::from(
                "VCE initial rebuild runtime differs from its durable target",
            ));
        }
        let engine = VceEngine::new(prepared.engine.clone()).map_err(|error| error.to_string())?;
        let ai = Arc::new(VceHostAiAdapter::activate(
            Arc::clone(&self.ai_port),
            VceHostAiRegistrationConfig {
                runtime_instance_id: Uuid::new_v4(),
                project_id: input.project_id.clone(),
                consumer_profile_id: String::from(VCE_CONSUMER_PROFILE_ID),
                vector_space_id: prepared.index.vector_space_id.clone(),
                dimension: Some(input.dimension),
                preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
                chunker_version: String::from(VCE_CHUNKER_VERSION),
            },
        )?);
        let activation = async {
            if ai.embedding_space() != &prepared.engine.embedding_space {
                return Err(String::from(
                    "VCE initial rebuild model registration changed its admitted physical space",
                ));
            }
            let projection = VceCommittedProjection {
                engine: engine.status(),
                workspace: engine.workspace_statistics(),
            };
            Ok(Arc::new(VceProjectRuntime {
                project_id: input.project_id.clone(),
                config: prepared.engine,
                config_revision: AtomicU64::new(input.settings.revision),
                settings: input.settings.clone(),
                latest_settings: std::sync::RwLock::new(input.settings.clone()),
                ai: Arc::clone(&ai),
                index: Arc::new(index),
                source: prepared.source,
                current_engine: RwLock::new(Arc::new(VcePublishedEngine {
                    engine,
                    read_index: None,
                })),
                committed_projection: RwLock::new(projection),
                refresh_gate: Arc::new(Mutex::new(())),
                job_state: self.project_job_state(&input.project_id).await,
                job_schedule: std::sync::Mutex::new(VceJobSchedule::new(&input.settings)),
                host_status: RwLock::new(VceHostStatus {
                    lifecycle: VceProjectLifecycle::RebuildRequired,
                    last_error: None,
                }),
                active_refresh: Mutex::new(None),
                refresh_observation_revision: AtomicU64::new(0),
                operation_admission: VceOperationAdmission::accepting(),
            }))
        }
        .await;
        match activation {
            Ok(runtime) => Ok(runtime),
            Err(error) => match ai.deactivate() {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(format!(
                    "{error}; cleanup failed VCE HostAI activation: {cleanup_error}"
                )),
            },
        }
    }

    /// Register current models and allocate only the target already named in the durable running receipt.
    /// 注册当前模型并仅分配持久运行回执已经指定的目标。
    /// `input`, `prepared` and `receipt` are frozen admission facts; returns a private, uninitialized writer.
    /// `input`、`prepared` 与 `receipt` 为冻结准入事实；返回私有未初始化写入器。
    async fn activate_replacement_writer(
        &self,
        input: &VceRebuildProjectInput,
        prepared: runtime_configuration::VcePreparedRuntimeConfiguration,
        receipt: &VceStoredJob,
    ) -> Result<Arc<VceProjectRuntime>, String> {
        let replacement = receipt
            .replacement
            .as_ref()
            .ok_or_else(|| String::from("VCE replacement receipt omitted its target"))?;
        if replacement.target != prepared.index {
            return Err(String::from(
                "VCE replacement runtime differs from its durable target",
            ));
        }
        let engine = VceEngine::new(prepared.engine.clone()).map_err(|error| error.to_string())?;
        let ai = Arc::new(VceHostAiAdapter::activate(
            Arc::clone(&self.ai_port),
            VceHostAiRegistrationConfig {
                runtime_instance_id: Uuid::new_v4(),
                project_id: input.project_id.clone(),
                consumer_profile_id: String::from(VCE_CONSUMER_PROFILE_ID),
                vector_space_id: prepared.index.vector_space_id.clone(),
                dimension: Some(input.dimension),
                preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
                chunker_version: String::from(VCE_CHUNKER_VERSION),
            },
        )?);
        let activation = async {
            if ai.embedding_space() != &prepared.engine.embedding_space {
                return Err(String::from(
                    "VCE replacement model registration changed its admitted physical space",
                ));
            }
            let index = Arc::new(
                self.data_plane
                    .prepare_rebuild_candidate(prepared.index, replacement.predecessor.clone())
                    .await?
                    .admit_rebuild_candidate_writer(replacement.predecessor.clone())
                    .await?,
            );
            let projection = VceCommittedProjection {
                engine: engine.status(),
                workspace: engine.workspace_statistics(),
            };
            Ok(Arc::new(VceProjectRuntime {
                project_id: input.project_id.clone(),
                config: prepared.engine,
                config_revision: AtomicU64::new(input.settings.revision),
                settings: input.settings.clone(),
                latest_settings: std::sync::RwLock::new(input.settings.clone()),
                ai: Arc::clone(&ai),
                index,
                source: prepared.source,
                current_engine: RwLock::new(Arc::new(VcePublishedEngine {
                    engine,
                    read_index: None,
                })),
                committed_projection: RwLock::new(projection),
                refresh_gate: Arc::new(Mutex::new(())),
                job_state: self.project_job_state(&input.project_id).await,
                job_schedule: std::sync::Mutex::new(VceJobSchedule::new(&input.settings)),
                host_status: RwLock::new(VceHostStatus {
                    lifecycle: VceProjectLifecycle::Uninitialized,
                    last_error: None,
                }),
                active_refresh: Mutex::new(None),
                refresh_observation_revision: std::sync::atomic::AtomicU64::new(0),
                operation_admission: VceOperationAdmission::accepting(),
            }))
        }
        .await;
        match activation {
            Ok(runtime) => Ok(runtime),
            Err(error) => {
                // Allocation may reserve ownership before returning an error; query only this receipt's exact target.
                // 分配可能在返回错误前预留归属；仅查询此回执的精确目标。
                let cleanup = async {
                    if let Some(allocated) = self
                        .data_plane
                        .find_rebuild_candidate(&replacement.target, &replacement.predecessor)
                        .await?
                    {
                        allocated
                            .cancel_rebuild_candidate(&replacement.predecessor)
                            .await?;
                        allocated.abort_refresh_staging(&receipt.job_id).await?;
                        allocated
                            .cleanup_cancelled_rebuild_candidate(&replacement.predecessor)
                            .await?;
                    }
                    Ok::<(), String>(())
                }
                .await;
                // Always release the exact AI registration, including when candidate cleanup itself failed.
                // 即使候选清理自身失败，也始终释放精确模型注册。
                let deactivate = ai.deactivate();
                match (cleanup, deactivate) {
                    (Ok(()), Ok(())) => Err(error),
                    (cleanup, deactivate) => Err(format!(
                        "{error}; cleanup candidate allocation: {cleanup:?}; cleanup model registration: {deactivate:?}"
                    )),
                }
            }
        }
    }
}

/// Validate the null-checkpoint admission boundary without weakening initialized-job guards.
/// 校验空检查点准入边界，同时不削弱已初始化作业的保护。
///
/// `command` is the user-confirmed admission, `settings_revision` is the current saved revision,
/// and `fingerprint` is the current physical producer. Returns an explicit stale-contract error.
/// `command` 是用户确认的准入，`settings_revision` 是当前保存修订，`fingerprint` 是当前物理生产者。
/// 返回明确的过期契约错误。
fn validate_uninitialized_rebuild_admission(
    command: &VceJobAdmission,
    settings_revision: u64,
    fingerprint: &str,
) -> Result<(), String> {
    if command.kind != VceJobKind::Rebuild
        || command.retry_of_job_id.is_some()
        || command.expected_checkpoint_version.is_some()
    {
        return Err(String::from(
            "VCE uninitialized rebuild requires a Rebuild command with a null checkpoint",
        ));
    }
    if command.expected_settings_revision != settings_revision {
        return Err(String::from(
            "VCE uninitialized rebuild settings revision changed after preview",
        ));
    }
    if command.contract_fingerprint != fingerprint {
        return Err(String::from(
            "VCE uninitialized rebuild embedding contract changed after preview",
        ));
    }
    Ok(())
}

/// Build the publication contract for a fresh initial namespace.
/// 为全新初始命名空间构建发布契约。
///
/// `input` and `prepared` provide current host facts; `provenance` is absent before storage
/// fencing and present after opening the fresh binding. Returns a contract with no predecessor.
/// `input` 与 `prepared` 提供当前宿主事实；封锁存储前 `provenance` 为空，打开新绑定后提供它。
/// 返回不含前驱的契约。
fn uninitialized_publication_contract(
    input: &VceRebuildProjectInput,
    prepared: &runtime_configuration::VcePreparedRuntimeConfiguration,
    provenance: Option<vulcan_workspace_store::vce_data_plane::VceEmbeddingProvenance>,
) -> VcePublicationContract {
    VcePublicationContract {
        project_id: input.project_id.clone(),
        canonical_root: prepared.index.root_path.clone(),
        embedding_fingerprint: input.fingerprint.clone(),
        dimension: input.dimension,
        max_file_bytes: input.settings.max_file_bytes,
        extra_ignore_patterns: input.settings.extra_ignore_patterns.clone(),
        provenance,
        replacement_admission: None,
    }
}

/// Freeze explicit target authority while retaining the exact initialized predecessor identity.
/// 冻结明确目标权威，同时保留精确已初始化前驱身份。
/// Inputs are admitted host facts and current provenance; returns metadata only, without granting model compatibility.
/// 输入为已准入宿主事实及当前来源；仅返回元数据，不授予模型兼容授权。
fn replacement_publication_contract(
    input: &VceRebuildProjectInput,
    prepared: &runtime_configuration::VcePreparedRuntimeConfiguration,
    provenance: vulcan_workspace_store::vce_data_plane::VceEmbeddingProvenance,
    receipt: &VceStoredJob,
) -> VcePublicationContract {
    VcePublicationContract {
        project_id: input.project_id.clone(),
        canonical_root: prepared.index.root_path.clone(),
        embedding_fingerprint: input.fingerprint.clone(),
        dimension: input.dimension,
        max_file_bytes: input.settings.max_file_bytes,
        extra_ignore_patterns: input.settings.extra_ignore_patterns.clone(),
        provenance: Some(provenance),
        replacement_admission: Some(Box::new(receipt.clone())),
    }
}

/// Compare immutable index intent while allowing saved query, scheduling and future execution budgets.
/// 比较不可变索引意图，同时允许保存查询、调度及后续执行预算。
/// `admitted` is this attempt's policy and `current` is saved policy; returns true only for non-index changes.
/// `admitted` 为本次尝试的策略，`current` 为已保存策略；仅非索引变化返回真。
fn replacement_policy_matches(admitted: &ProjectVceSettings, current: &ProjectVceSettings) -> bool {
    // normalized ignores only explicitly classified fields; a future field is conservatively compared.
    // normalized 仅忽略已明确分类字段；未来字段仍保守参与比较。
    let mut normalized = current.clone();
    normalized.revision = admitted.revision;
    normalized.update_mode = admitted.update_mode;
    normalized.auto_update_min_interval_seconds = admitted.auto_update_min_interval_seconds;
    normalized.allow_query_rewrite = admitted.allow_query_rewrite;
    normalized.allow_rerank = admitted.allow_rerank;
    normalized.embedding_max_concurrency = admitted.embedding_max_concurrency;
    normalized.query_max_concurrency = admitted.query_max_concurrency;
    normalized.max_results = admitted.max_results;
    normalized.max_context_chars = admitted.max_context_chars;
    &normalized == admitted
}

/// Validate confirmed actions directly against the stored predecessor without restoring a query engine.
/// 直接对存储前驱验证确认动作，不恢复查询引擎。
/// `index` and `original` identify storage; `work` contains the exact human action set. Returns explicit drift errors.
/// `index` 与 `original` 标识存储；`work` 包含精确人为动作集合。返回明确漂移错误。
async fn validate_stored_rebuild_confirmation(
    index: &VceProjectIndex,
    original: &VceProjectIndexConfig,
    work: &VceJobWork,
) -> Result<(), String> {
    let VceJobWork::Rebuild { repair, action_ids } = work else {
        return Err(String::from(
            "VCE physical replacement requires confirmed rebuild work",
        ));
    };
    let mut config = vulcan_context_engine::VceConfig::new(
        original.binding.checkpoint_chain_id.clone(),
        original.binding.embedding_space.clone(),
    )
    .map_err(|error| error.to_string())?;
    config.index_namespace_id = original.binding.namespace_id.clone();
    config.index_schema_version = original.binding.schema_version;
    let engine = VceEngine::new(config).map_err(|error| error.to_string())?;
    let response = engine
        .management_query(
            VceManagementQueryRequest {
                operation_id: format!(
                    "vce:{}:replacement-confirm:{}",
                    original.project_id,
                    Uuid::new_v4()
                ),
                query: vulcan_context_engine::VceManagementQuery::MaintenancePlan {
                    mode: if *repair {
                        vulcan_context_engine::VceMaintenanceMode::Repair
                    } else {
                        vulcan_context_engine::VceMaintenanceMode::Rebuild
                    },
                },
            },
            index,
        )
        .await
        .map_err(|error| error.to_string())?;
    let vulcan_context_engine::VceManagementQueryResult::MaintenancePlan(plan) = response.result
    else {
        return Err(String::from(
            "VCE maintenance confirmation returned an unexpected result",
        ));
    };
    let planned = plan
        .actions
        .into_iter()
        .map(|action| action.action_id)
        .collect::<BTreeSet<_>>();
    if planned.is_empty() || &planned != action_ids {
        return Err(String::from(
            "VCE maintenance actions changed; regenerate and confirm the current plan",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify only non-index policy can be carried into a replacement's final query runtime.
    /// 验证仅非索引策略能够携带进入替换的最终查询运行时。
    /// No parameters or return value; assertions require no storage, providers or project scans.
    /// 无参数及返回值；断言不需要存储、供应商或项目扫描。
    #[test]
    fn replacement_policy_preserves_source_intent_but_accepts_query_and_schedule_changes() {
        // admitted freezes source selection; current exercises every explicitly mutable policy field.
        // admitted 冻结源码选择；current 覆盖每个明确可变的策略字段。
        let admitted = ProjectVceSettings {
            enabled: true,
            ..ProjectVceSettings::default()
        };
        let mut current = admitted.clone();
        current.revision += 1;
        current.update_mode = vulcan_workbench_model::ProjectVceUpdateMode::Automatic;
        current.auto_update_min_interval_seconds = 600;
        current.allow_query_rewrite = !admitted.allow_query_rewrite;
        current.allow_rerank = !admitted.allow_rerank;
        current.embedding_max_concurrency = 2;
        current.query_max_concurrency = 1;
        current.max_results = 7;
        current.max_context_chars = 8_000;
        current.validate().expect("valid changed policy");
        assert!(replacement_policy_matches(&admitted, &current));
        current.max_file_bytes += 1;
        assert!(!replacement_policy_matches(&admitted, &current));
        current.max_file_bytes = admitted.max_file_bytes;
        current
            .extra_ignore_patterns
            .push(String::from("generated/**"));
        assert!(!replacement_policy_matches(&admitted, &current));
        current.extra_ignore_patterns.clear();
        current.enabled = false;
        assert!(!replacement_policy_matches(&admitted, &current));
    }
}
