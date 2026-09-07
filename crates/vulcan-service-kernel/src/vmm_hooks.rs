//! VMM hook snapshot and writeback integration for service-kernel runtime paths.
//! service-kernel 运行时路径使用的 VMM hook 快照与写回集成逻辑。

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard,
        atomic::{AtomicU64, Ordering},
    },
};

use vulcan_agent_runtime::{
    agent_runtime::{
        conversation_hooks::VmmWritebackHookEffect,
        vmm_hooks::{VmmHookConfigSnapshot, VmmHookRuntimeHandle},
    },
    provider_config::ProviderConfigurationFile,
};
use vulcan_workbench_model::{SessionId, SessionRuntimeStateEntry};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use crate::{
    ServiceBridgeState, SharedServiceBridgeState, current_unix_millis_value,
    load_project_configuration, webui_trace, workspace_task_projects_including_archived,
};

#[path = "vmm_global_mode_projection.rs"]
mod global_mode_projection;

/// Process-local project VMM layer cache keyed by normalized workspace path.
/// 按规范化工作区路径索引的进程内项目 VMM 配置层缓存。
static PROJECT_VMM_LAYER_CACHE: OnceLock<ProjectVmmLayerCache> = OnceLock::new();

/// Cached project VMM layer with monotonic metadata for debugging and tests.
/// 带单调元数据的项目 VMM 配置层缓存项，用于调试与测试。
#[derive(Clone)]
struct CachedProjectVmmLayer {
    /// Cached project VMM configuration layer.
    /// 已缓存的项目 VMM 配置层。
    layer: vulcan_vmm::VmmConfigLayer,
    /// Monotonic cache generation assigned when this entry is written.
    /// 写入该缓存项时分配的单调缓存代次。
    generation: u64,
    /// Millisecond timestamp recorded when this entry is written.
    /// 写入该缓存项时记录的毫秒时间戳。
    loaded_at_ms: i128,
}

/// Process-local manager for project VMM layer caching and write-through updates.
/// 进程内项目 VMM 配置层缓存与写穿更新管理器。
struct ProjectVmmLayerCache {
    /// Cached entries keyed by normalized workspace path.
    /// 以规范化工作区路径为键的缓存项集合。
    entries: RwLock<BTreeMap<String, CachedProjectVmmLayer>>,
    /// Global monotonic generation for cache writes.
    /// 缓存写入使用的全局单调代次。
    generation: AtomicU64,
}

impl ProjectVmmLayerCache {
    /// Creates an empty project VMM layer cache manager.
    /// 创建空的项目 VMM 配置层缓存管理器。
    fn new() -> Self {
        Self {
            entries: RwLock::new(BTreeMap::new()),
            generation: AtomicU64::new(0),
        }
    }

    /// Acquires the project VMM cache read guard and recovers poisoned locks.
    /// 获取项目 VMM 缓存读锁，并恢复已中毒的锁。
    ///
    /// # Returns
    /// 返回值
    /// The read guard for cached project VMM entries.
    /// 返回项目 VMM 缓存项的读锁守卫。
    fn read_entries(&self) -> RwLockReadGuard<'_, BTreeMap<String, CachedProjectVmmLayer>> {
        self.entries
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Acquires the project VMM cache write guard and recovers poisoned locks.
    /// 获取项目 VMM 缓存写锁，并恢复已中毒的锁。
    ///
    /// # Returns
    /// 返回值
    /// The write guard for cached project VMM entries.
    /// 返回项目 VMM 缓存项的写锁守卫。
    fn write_entries(&self) -> RwLockWriteGuard<'_, BTreeMap<String, CachedProjectVmmLayer>> {
        self.entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Returns a cached layer or loads it once without overwriting concurrent writes.
    /// 返回缓存配置层，未命中时加载一次且不会覆盖并发写入的新值。
    ///
    /// # Parameters
    /// 参数
    /// `workspace_path` is the workspace path used to identify the project VMM layer.
    /// `workspace_path` 是用于标识项目 VMM 配置层的工作区路径。
    /// `loader` reads the authoritative project VMM layer on a cache miss.
    /// `loader` 在缓存未命中时读取权威项目 VMM 配置层。
    ///
    /// # Returns
    /// 返回值
    /// The cached or freshly loaded project VMM configuration layer.
    /// 返回缓存中或刚加载的项目 VMM 配置层。
    fn get_or_load(
        &self,
        workspace_path: &str,
        loader: impl FnOnce(&str) -> vulcan_vmm::VmmConfigLayer,
    ) -> vulcan_vmm::VmmConfigLayer {
        let key = project_vmm_cache_key(workspace_path);
        if let Some(entry) = self.get_entry(&key) {
            return Self::layer_from_entry(&entry);
        }

        let loaded_layer = loader(workspace_path);
        let mut entries = self.write_entries();
        if let Some(existing) = entries.get(&key) {
            return Self::layer_from_entry(existing);
        }
        entries.insert(key, self.entry_for_layer(loaded_layer.clone()));
        loaded_layer
    }

    /// Writes one authoritative project VMM layer into the cache.
    /// 将单个权威项目 VMM 配置层写入缓存。
    ///
    /// # Parameters
    /// 参数
    /// `workspace_path` identifies the project whose cached VMM layer is replaced.
    /// `workspace_path` 标识被替换缓存 VMM 配置层的项目。
    /// `layer` is the authoritative project VMM layer that has already been saved.
    /// `layer` 是已经完成保存的权威项目 VMM 配置层。
    ///
    /// # Returns
    /// 返回值
    /// This function updates the cache in place and returns no value.
    /// 该函数原地更新缓存且不返回值。
    fn update(&self, workspace_path: &str, layer: vulcan_vmm::VmmConfigLayer) {
        let key = project_vmm_cache_key(workspace_path);
        let entry = self.entry_for_layer(layer);
        let mut entries = self.write_entries();
        entries.insert(key, entry);
    }

    /// Reads one cached entry by an already normalized key.
    /// 通过已经规范化的键读取单个缓存项。
    ///
    /// # Parameters
    /// 参数
    /// `key` is the normalized project VMM cache key.
    /// `key` 是规范化后的项目 VMM 缓存键。
    ///
    /// # Returns
    /// 返回值
    /// The cached entry when present.
    /// 存在时返回对应缓存项。
    fn get_entry(&self, key: &str) -> Option<CachedProjectVmmLayer> {
        let entries = self.read_entries();
        entries.get(key).cloned()
    }

    /// Creates a cache entry for one project VMM layer.
    /// 为单个项目 VMM 配置层创建缓存项。
    ///
    /// # Parameters
    /// 参数
    /// `layer` is the VMM layer stored in the cache entry.
    /// `layer` 是写入缓存项的 VMM 配置层。
    ///
    /// # Returns
    /// 返回值
    /// A cache entry with fresh generation and timestamp metadata.
    /// 返回带有新代次与时间戳元数据的缓存项。
    fn entry_for_layer(&self, layer: vulcan_vmm::VmmConfigLayer) -> CachedProjectVmmLayer {
        CachedProjectVmmLayer {
            layer,
            generation: self.generation.fetch_add(1, Ordering::Relaxed) + 1,
            loaded_at_ms: current_unix_millis_value(),
        }
    }

    /// Clones the cached layer while intentionally touching metadata.
    /// 克隆缓存配置层，同时显式读取缓存元数据。
    ///
    /// # Parameters
    /// 参数
    /// `entry` is the cached project VMM layer entry.
    /// `entry` 是项目 VMM 配置层缓存项。
    ///
    /// # Returns
    /// 返回值
    /// The cloned project VMM configuration layer.
    /// 返回克隆后的项目 VMM 配置层。
    fn layer_from_entry(entry: &CachedProjectVmmLayer) -> vulcan_vmm::VmmConfigLayer {
        let _ = (entry.generation, entry.loaded_at_ms);
        entry.layer.clone()
    }
}

/// Returns the shared project VMM layer cache manager.
/// 返回共享项目 VMM 配置层缓存管理器。
fn project_vmm_layer_cache() -> &'static ProjectVmmLayerCache {
    PROJECT_VMM_LAYER_CACHE.get_or_init(ProjectVmmLayerCache::new)
}

/// Builds the canonical cache key for one workspace path.
/// 为单个工作区路径构建规范缓存键。
///
/// # Parameters
/// 参数
/// `workspace_path` is the raw workspace path persisted in the project snapshot.
/// `workspace_path` 是项目快照中持久化的原始工作区路径。
///
/// # Returns
/// 返回值
/// The normalized workspace path used as the project VMM cache key.
/// 返回作为项目 VMM 缓存键使用的规范化工作区路径。
fn project_vmm_cache_key(workspace_path: &str) -> String {
    let mut key = workspace_path.trim().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    if key.len() >= 2 && key.as_bytes()[1] == b':' && key.as_bytes()[0].is_ascii_alphabetic() {
        let drive = key[..1].to_ascii_lowercase();
        key.replace_range(0..1, &drive);
    }
    key
}

/// Reads one project VMM layer from memory, loading it once on a cache miss.
/// 从内存读取单个项目 VMM 配置层；缓存未命中时只加载一次。
pub(super) fn cached_project_vmm_layer(workspace_path: &str) -> vulcan_vmm::VmmConfigLayer {
    project_vmm_layer_cache()
        .get_or_load(workspace_path, |path| load_project_configuration(path).vmm)
}

/// Updates one cached project VMM layer after an authoritative save.
/// 在权威保存后更新单个项目 VMM 配置层缓存。
pub(super) fn update_cached_project_vmm_layer(
    workspace_path: &str,
    layer: vulcan_vmm::VmmConfigLayer,
) {
    project_vmm_layer_cache().update(workspace_path, layer);
}

/// Refreshes the VMM hook snapshot for one locked service state.
/// 为单个已加锁服务状态刷新 VMM hook 快照。
///
/// # Parameters
/// 参数
/// `guard` is the locked service state containing the runtime hook handle and workspace snapshot.
/// `guard` 是包含运行时 hook 句柄与工作区快照的已加锁服务状态。
/// `provider_configuration` is the provider configuration used as the global VMM layer.
/// `provider_configuration` 是作为全局 VMM 配置层使用的 Provider 配置。
///
/// # Returns
/// 返回值
/// Returns success after updating or legitimately skipping the runtime, or a VMM state-lock error.
/// 更新或合法跳过运行时后返回成功，VMM 状态锁失败时返回错误。
pub(super) fn refresh_guard_vmm_hook_snapshot(
    guard: &mut ServiceBridgeState,
    provider_configuration: &ProviderConfigurationFile,
) -> Result<(), String> {
    // Global activation cannot be overridden by a project; observe it without fabricated session IDs.
    // 项目不能覆盖全局启用状态；不虚构会话 ID 来观察它。
    let next = vulcan_vmm::VmmEffectiveConfig::resolve(&provider_configuration.vmm, None, None);
    let next_mode = next.enabled.then_some(next.backend_mode);
    let mut reset_sources = next_mode.is_none();
    if let Some(agent_runtime) = guard.agent_runtime.as_ref() {
        reset_sources |= agent_runtime.vmm_hook_runtime.global_memory_mode()? != next_mode;
        agent_runtime
            .vmm_hook_runtime
            .replace_global_projection(&provider_configuration.vmm)?;
    }
    if reset_sources {
        global_mode_projection::clear_global_mode_caches(guard)?;
    }
    reconcile_vmm_writeback_mirrors(guard)?;
    Ok(())
}

/// Removes revoked pending work from active and archived process-local UI mirrors using the hook admission authority.
/// 使用 Hook 准入权威，从活动及归档的进程内界面镜像移除已撤销待处理工作。
/// Takes the already-locked service state; does not delete accepted memory or acquire the service lock recursively.
/// 接收已加锁服务状态；不删除已接受记忆，也不递归取得服务锁。
pub(crate) fn reconcile_vmm_writeback_mirrors(
    guard: &mut ServiceBridgeState,
) -> Result<(), String> {
    let Some(agent_runtime) = guard.agent_runtime.as_ref() else {
        return Ok(());
    };
    let Some(workspace) = guard.workspace_snapshot.as_mut() else {
        return Ok(());
    };
    for (session_id, runtime_state) in workspace
        .session_runtime_state
        .iter_mut()
        .chain(workspace.archived_session_runtime_state.iter_mut())
    {
        runtime_state.vmm_writeback_outbox = agent_runtime
            .vmm_hook_runtime
            .current_writeback_outbox(*session_id, &runtime_state.vmm_writeback_outbox)?;
    }
    Ok(())
}

/// Refreshes one exact project VMM layer after its authoritative configuration save.
/// 在单个精确项目的权威配置保存后刷新其 VMM 配置层。
///
/// # Parameters
/// 参数
/// `state` holds the optional runtime hook handle.
/// `state` 持有可选的运行时 hook 句柄。
/// `workspace_path` is the exact normalized project key.
/// `workspace_path` 是精确的规范化项目键。
/// `project` is the complete saved project layer.
/// `project` 是已保存的完整项目配置层。
///
/// # Returns
/// 返回值
/// Returns success after one exact map update or a lock diagnostic.
/// 单次精确映射更新完成后返回成功，失败时返回锁诊断。
pub(super) fn refresh_exact_vmm_hook_project_projection(
    state: &SharedServiceBridgeState,
    workspace_path: &str,
    project: &vulcan_vmm::VmmConfigLayer,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(agent_runtime) = guard.agent_runtime.as_ref() {
        agent_runtime
            .vmm_hook_runtime
            .replace_project_projection(workspace_path, project)?;
    }
    reconcile_vmm_writeback_mirrors(&mut guard)?;
    Ok(())
}

/// Builds the shared VMM hook snapshot from the current Tauri workspace state.
/// 根据当前 Tauri 工作区状态构建共享 VMM hook 快照。
///
/// # Parameters
/// 参数
/// `provider_configuration` provides the global VMM configuration layer.
/// `provider_configuration` 提供全局 VMM 配置层。
/// `workspace_snapshot` provides project, session, outbox, profile-sync and compaction state.
/// `workspace_snapshot` 提供项目、会话、outbox、画像同步与压缩状态。
///
/// # Returns
/// 返回值
/// The complete VMM hook configuration snapshot consumed by agent-runtime hooks.
/// 返回 agent-runtime hook 使用的完整 VMM hook 配置快照。
pub(super) fn build_vmm_hook_config_snapshot(
    provider_configuration: &ProviderConfigurationFile,
    workspace_snapshot: &WorkspaceSnapshot,
) -> VmmHookConfigSnapshot {
    let snapshot_started_at = current_unix_millis_value();
    let mut projects_by_workspace = std::collections::BTreeMap::new();
    let mut project_count = 0usize;
    for project in workspace_task_projects_including_archived(workspace_snapshot) {
        project_count += 1;
        let project_layer = cached_project_vmm_layer(&project.path);
        if !project_layer.is_empty() {
            projects_by_workspace.insert(project.path.clone(), project_layer);
        }
    }

    // sessions stores non-empty per-session VMM layers keyed by session id.
    // sessions 按会话标识保存非空的会话级 VMM 配置层。
    let mut sessions = std::collections::BTreeMap::new();
    // outboxes stores pending VMM writeback work keyed by session id.
    // outboxes 按会话标识保存待处理的 VMM 写回任务。
    let mut outboxes = std::collections::BTreeMap::new();
    // profile_sync stores non-empty VMM profile synchronization state keyed by session id.
    // profile_sync 按会话标识保存非空的 VMM 画像同步状态。
    let mut profile_sync = std::collections::BTreeMap::new();
    // last_compaction_ids stores the latest known compaction id for every runtime-state row.
    // last_compaction_ids 保存每条运行态记录最近一次已知的压缩标识。
    let mut last_compaction_ids = std::collections::BTreeMap::new();
    for (session_id, runtime_state) in workspace_snapshot
        .session_runtime_state
        .iter()
        .chain(workspace_snapshot.archived_session_runtime_state.iter())
    {
        // session_key is shared by all VMM hook snapshot maps for this runtime-state row.
        // session_key 是该运行态记录在所有 VMM hook 快照映射中的共享键。
        let session_key = session_id.to_string();
        if !runtime_state.vmm.is_empty() {
            sessions.insert(session_key.clone(), runtime_state.vmm.clone());
        }
        if !runtime_state.vmm_writeback_outbox.is_empty() {
            outboxes.insert(
                session_key.clone(),
                runtime_state.vmm_writeback_outbox.clone(),
            );
        }
        if !runtime_state.vmm_profile_sync.is_empty() {
            profile_sync.insert(session_key.clone(), runtime_state.vmm_profile_sync.clone());
        }
        last_compaction_ids.insert(session_key, runtime_state.last_compaction_id.clone());
    }
    let snapshot = VmmHookConfigSnapshot {
        writeback_admissions: Default::default(),
        global: provider_configuration.vmm.clone(),
        projects_by_workspace,
        sessions,
        outboxes,
        profile_sync,
        last_compaction_ids,
    };
    webui_trace(
        "vmm_hook_snapshot_built",
        format!(
            "projects_scanned={} project_vmm={} session_vmm={} outboxes={} profile_sync={} last_compaction_ids={} elapsed_ms={}",
            project_count,
            snapshot.projects_by_workspace.len(),
            snapshot.sessions.len(),
            snapshot.outboxes.len(),
            snapshot.profile_sync.len(),
            snapshot.last_compaction_ids.len(),
            current_unix_millis_value().saturating_sub(snapshot_started_at)
        ),
    );
    snapshot
}

/// Refreshes only the VMM hook projection owned by one exact retained session.
/// 仅刷新单个精确保留会话拥有的 VMM hook 投影。
///
/// # Parameters
/// 参数
/// `runtime` is the authoritative hook runtime receiving the targeted update.
/// `runtime` 是接收定向更新的权威 hook 运行时。
/// `workspace_snapshot` provides direct active and archived runtime-state indexes.
/// `workspace_snapshot` 提供活动与归档运行态的直接索引。
/// `session_id` identifies the only projection allowed to change.
/// `session_id` 标识唯一允许发生变化的投影。
///
/// # Returns
/// 返回值
/// Returns success after one exact map lookup and update, or a runtime lock diagnostic.
/// 单次精确映射查询与更新完成后返回成功，失败时返回运行时锁诊断。
pub(super) fn refresh_exact_vmm_hook_session_projection(
    runtime: &VmmHookRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    // runtimeState resolves from the two schema-declared ownership maps without enumerating either map.
    // runtimeState 从 Schema 声明的两个归属映射中解析，且不会枚举任一映射。
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .or_else(|| {
            workspace_snapshot
                .archived_session_runtime_state
                .get(&session_id)
        });
    let Some(runtime_state) = runtime_state else {
        return runtime.remove_session_projection(session_id);
    };
    runtime.replace_session_projection(
        session_id,
        &runtime_state.vmm,
        &runtime_state.vmm_writeback_outbox,
        runtime_state.last_compaction_id.as_deref(),
    )
}

/// Applies VMM writeback hook effects to one session runtime state.
/// 将 VMM 写回 hook 效果应用到单个会话运行态。
///
/// # Parameters
/// 参数
/// `runtime_state` is the mutable session runtime state receiving outbox changes.
/// `runtime_state` 是接收 outbox 变更的可变会话运行态。
/// `effects` are the hook effects returned by the VMM completion hook.
/// `effects` 是 VMM completion hook 返回的 hook 效果集合。
/// `now` is the timestamp text stored on created or updated outbox entries.
/// `now` 是写入新建或更新 outbox 条目的时间戳文本。
///
/// # Returns
/// 返回值
/// This function mutates the runtime state in place and returns no value.
/// 该函数会原地修改运行态且不返回值。
pub(super) fn apply_vmm_writeback_effects_to_runtime_state(
    runtime_state: &mut SessionRuntimeStateEntry,
    effects: Vec<VmmWritebackHookEffect>,
    now: &str,
) {
    for effect in effects {
        match effect {
            // SessionStatus is committed by the Core VMM controller before ordinary runtime effects are applied.
            // SessionStatus 会在应用普通运行时效果前由 Core VMM 控制器提交。
            VmmWritebackHookEffect::SessionStatus { .. } => {}
            VmmWritebackHookEffect::Flushed { entry_ids }
            | VmmWritebackHookEffect::Cancelled { entry_ids, .. } => {
                let ids = entry_ids.into_iter().collect::<BTreeSet<_>>();
                runtime_state
                    .vmm_writeback_outbox
                    .retain(|entry| !ids.contains(&entry.id));
            }
            VmmWritebackHookEffect::Enqueue {
                permit_id,
                request,
                error,
            } => {
                let id = format!(
                    "vmm-outbox-{}-{}-{}",
                    request.session_id,
                    permit_id,
                    stable_vmm_outbox_request_hash(&request)
                );
                if let Some(existing) = runtime_state
                    .vmm_writeback_outbox
                    .iter_mut()
                    .find(|entry| entry.id == id)
                {
                    existing.request = request;
                    existing.attempt_count = existing.attempt_count.saturating_add(1);
                    existing.last_error = Some(error);
                    existing.updated_at = now.to_string();
                } else {
                    runtime_state.vmm_writeback_outbox.push(
                        vulcan_workbench_model::VmmWritebackOutboxEntry {
                            permit_id,
                            id,
                            request,
                            attempt_count: 1,
                            last_error: Some(error),
                            created_at: now.to_string(),
                            updated_at: now.to_string(),
                        },
                    );
                }
            }
            VmmWritebackHookEffect::Failed { entry_id, error } => {
                if let Some(existing) = runtime_state
                    .vmm_writeback_outbox
                    .iter_mut()
                    .find(|entry| entry.id == entry_id)
                {
                    existing.attempt_count = existing.attempt_count.saturating_add(1);
                    existing.last_error = Some(error);
                    existing.updated_at = now.to_string();
                }
            }
        }
    }
}

/// Builds one stable hash for a VMM writeback request.
/// 为一条 VMM 写回请求构建稳定哈希。
///
/// # Parameters
/// 参数
/// `request` is the VMM PostAction request that needs a deterministic local outbox key.
/// `request` 是需要确定性本地 outbox 键的 VMM PostAction 请求。
///
/// # Returns
/// 返回值
/// A deterministic non-cryptographic hash used only for persisted outbox identity.
/// 返回仅用于持久化 outbox 身份的确定性非加密哈希。
fn stable_vmm_outbox_request_hash(request: &vulcan_vmm::PostActionRequest) -> u64 {
    let mut hasher = StableVmmOutboxHasher::default();
    // Scope queue identity to the logical host turn and its exact destination, not just repeated prose.
    // 队列身份属于逻辑宿主回合及准确目的地，不仅仅取决于重复正文。
    hasher.write_str(&request.operation_id);
    hasher.write_str(&request.host_project_id);
    match &request.expected_source {
        Some(vulcan_vmm::VmmMemorySource::Native { project_id }) => {
            hasher.write_str("native");
            hasher.write_u64(*project_id);
        }
        Some(vulcan_vmm::VmmMemorySource::ExternalGrpc {
            route_hash,
            user_id,
            project_id,
        }) => {
            hasher.write_str("external_grpc");
            hasher.write_str(route_hash);
            hasher.write_u64(*user_id);
            hasher.write_u64(*project_id);
        }
        None => hasher.write_str("unresolved"),
    }
    hasher.write_str(&request.session_id);
    hasher.write_u64(request.user_id);
    hasher.write_u64(request.project_id);
    hasher.write_str(&request.user_content);
    hasher.write_str(&request.assistant_content);
    for item in &request.timeline {
        hasher.write_str(item.kind.as_wire_type());
        hasher.write_str(&item.content);
    }
    hasher.finish()
}

/// Tiny deterministic FNV-1a helper used for persisted VMM outbox ids.
/// 用于持久化 VMM outbox 标识的小型确定性 FNV-1a 辅助器。
struct StableVmmOutboxHasher {
    /// Current FNV-1a accumulator.
    /// 当前 FNV-1a 累加值。
    value: u64,
}

impl Default for StableVmmOutboxHasher {
    /// Creates one FNV-1a hasher with the canonical 64-bit offset basis.
    /// 使用标准 64 位 offset basis 创建一个 FNV-1a 哈希器。
    fn default() -> Self {
        Self {
            value: 0xcbf29ce484222325,
        }
    }
}

impl StableVmmOutboxHasher {
    /// Writes a length-prefixed string into the hash stream.
    /// 将带长度前缀的字符串写入哈希流。
    fn write_str(&mut self, value: &str) {
        self.write_u64(value.len() as u64);
        self.write_bytes(value.as_bytes());
    }

    /// Writes one integer into the hash stream in little-endian form.
    /// 以小端形式将一个整数写入哈希流。
    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Writes raw bytes into the FNV-1a accumulator.
    /// 将原始字节写入 FNV-1a 累加器。
    fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.value ^= u64::from(*byte);
            self.value = self.value.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Returns the final deterministic hash value.
    /// 返回最终的确定性哈希值。
    fn finish(self) -> u64 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use uuid::Uuid;
    use vulcan_workbench_model::VmmWritebackOutboxEntry;
    use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

    /// Builds a project VMM layer with a deterministic enabled flag.
    /// 构建带有确定 enabled 标记的项目 VMM 配置层。
    fn test_vmm_layer(enabled: bool) -> vulcan_vmm::VmmConfigLayer {
        vulcan_vmm::VmmConfigLayer {
            enabled: Some(enabled),
            ..Default::default()
        }
    }

    /// Builds a minimal workspace snapshot for VMM hook integration tests.
    /// 为 VMM hook 集成测试构建最小工作区快照。
    fn test_workspace_snapshot() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session: Uuid::nil(),
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies Windows-style workspace paths share one project VMM cache key.
    /// 验证 Windows 风格工作区路径会共享同一个项目 VMM 缓存键。
    #[test]
    fn project_vmm_cache_key_normalizes_windows_style_paths() {
        assert_eq!(
            project_vmm_cache_key(r"D:\OpenVulcan\vulcan-code\"),
            "d:/OpenVulcan/vulcan-code"
        );
        assert_eq!(
            project_vmm_cache_key("D:/OpenVulcan/vulcan-code/"),
            "d:/OpenVulcan/vulcan-code"
        );
    }

    /// Verifies a cache miss loads once and equivalent paths reuse the cached entry.
    /// 验证缓存未命中只加载一次，且等价路径会复用同一个缓存项。
    #[test]
    fn project_vmm_layer_cache_loads_and_reuses_entry() {
        let cache = ProjectVmmLayerCache::new();
        let calls = AtomicU64::new(0);

        let first = cache.get_or_load("D:/repo", |_| {
            calls.fetch_add(1, Ordering::Relaxed);
            test_vmm_layer(true)
        });
        let second = cache.get_or_load(r"D:\repo\", |_| {
            calls.fetch_add(1, Ordering::Relaxed);
            test_vmm_layer(false)
        });

        let entry = cache
            .get_entry(&project_vmm_cache_key("D:/repo"))
            .expect("cache entry should exist after load");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(first.enabled, Some(true));
        assert_eq!(second.enabled, Some(true));
        assert_eq!(entry.generation, 1);
        assert!(entry.loaded_at_ms > 0);
    }

    /// Verifies saved project VMM layers take precedence over later cache-miss loaders.
    /// 验证已保存的项目 VMM 配置层优先于后续缓存未命中加载器。
    #[test]
    fn project_vmm_layer_cache_update_writes_authoritative_value() {
        let cache = ProjectVmmLayerCache::new();

        cache.update("D:/repo", test_vmm_layer(false));
        let layer = cache.get_or_load("D:/repo", |_| test_vmm_layer(true));

        assert_eq!(layer.enabled, Some(false));
    }

    /// Verifies a stale miss load cannot overwrite a concurrent authoritative update.
    /// 验证过期的未命中加载结果不能覆盖并发权威更新。
    #[test]
    fn project_vmm_layer_cache_miss_does_not_overwrite_concurrent_update() {
        let cache = ProjectVmmLayerCache::new();

        let layer = cache.get_or_load("D:/repo", |_| {
            cache.update("D:/repo", test_vmm_layer(false));
            test_vmm_layer(true)
        });

        let entry = cache
            .get_entry(&project_vmm_cache_key("D:/repo"))
            .expect("cache entry should keep the authoritative update");
        assert_eq!(layer.enabled, Some(false));
        assert_eq!(entry.layer.enabled, Some(false));
    }

    /// Verifies the cache keeps working after a panic poisons its lock.
    /// 验证缓存锁被 panic 标记为中毒后仍能继续工作。
    #[test]
    fn project_vmm_layer_cache_recovers_poisoned_lock() {
        let cache = ProjectVmmLayerCache::new();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _entries = cache
                .entries
                .write()
                .expect("test lock should start healthy");
            panic!("poison project VMM cache lock for recovery coverage");
        }));

        cache.update("D:/repo", test_vmm_layer(true));
        let layer = cache.get_or_load("D:/repo", |_| test_vmm_layer(false));

        assert_eq!(layer.enabled, Some(true));
    }

    /// Builds a stable PostAction request for writeback tests.
    /// 为写回测试构建稳定的 PostAction 请求。
    fn test_post_action_request() -> vulcan_vmm::PostActionRequest {
        vulcan_vmm::PostActionRequest {
            expected_source: Some(vulcan_vmm::VmmMemorySource::native(11).expect("test source")),
            operation_id: String::from("turn-a"),
            session_id: String::from("session-a"),
            user_id: 7,
            project_id: 11,
            host_project_id: String::from("host-project-a"),
            user_content: String::from("hello"),
            assistant_content: String::from("world"),
            timeline: vec![vulcan_vmm::PostActionTimelineItem {
                kind: vulcan_vmm::PostActionTimelineType::Assistant,
                content: String::from("middle"),
            }],
        }
    }

    /// Verifies hook snapshots collect session VMM, outbox, profile and compaction state.
    /// 验证 hook 快照会收集会话 VMM、outbox、画像和压缩状态。
    #[test]
    fn build_vmm_hook_config_snapshot_collects_session_runtime_state() {
        let session_id = Uuid::from_u128(42);
        let archived_session_id = Uuid::from_u128(43);
        let mut runtime_state = SessionRuntimeStateEntry::default();
        runtime_state.vmm.enabled = Some(true);
        runtime_state
            .vmm_writeback_outbox
            .push(VmmWritebackOutboxEntry {
                permit_id: 1,
                id: String::from("outbox-a"),
                request: test_post_action_request(),
                attempt_count: 1,
                last_error: Some(String::from("offline")),
                created_at: String::from("100"),
                updated_at: String::from("100"),
            });
        runtime_state.vmm_profile_sync.updated_at = Some(String::from("profile-updated"));
        runtime_state.last_compaction_id = Some(String::from("compact-a"));

        let mut workspace = test_workspace_snapshot();
        workspace
            .session_runtime_state
            .insert(session_id, runtime_state);
        let mut archived_runtime_state = SessionRuntimeStateEntry::default();
        archived_runtime_state.vmm.ai_tools_enabled = Some(false);
        archived_runtime_state.last_compaction_id = Some(String::from("archived-compact"));
        workspace
            .archived_session_runtime_state
            .insert(archived_session_id, archived_runtime_state);
        let mut provider_configuration = ProviderConfigurationFile::default();
        provider_configuration.vmm.enabled = Some(true);

        let snapshot = build_vmm_hook_config_snapshot(&provider_configuration, &workspace);

        assert_eq!(snapshot.global.enabled, Some(true));
        assert!(snapshot.sessions.contains_key(&session_id.to_string()));
        assert!(
            snapshot
                .sessions
                .contains_key(&archived_session_id.to_string())
        );
        assert_eq!(
            snapshot.outboxes.get(&session_id.to_string()).map(Vec::len),
            Some(1)
        );
        assert!(snapshot.profile_sync.contains_key(&session_id.to_string()));
        assert_eq!(
            snapshot
                .last_compaction_ids
                .get(&session_id.to_string())
                .and_then(Clone::clone)
                .as_deref(),
            Some("compact-a")
        );
        assert_eq!(
            snapshot
                .last_compaction_ids
                .get(&archived_session_id.to_string())
                .and_then(Clone::clone)
                .as_deref(),
            Some("archived-compact")
        );
    }

    /// Verifies writeback effects enqueue, update, mark failure and flush one outbox entry.
    /// 验证写回效果会入队、更新、标记失败并冲刷单个 outbox 条目。
    #[test]
    fn apply_vmm_writeback_effects_updates_outbox_lifecycle() {
        let request = test_post_action_request();
        let mut runtime_state = SessionRuntimeStateEntry::default();

        apply_vmm_writeback_effects_to_runtime_state(
            &mut runtime_state,
            vec![VmmWritebackHookEffect::Enqueue {
                permit_id: 1,
                request: request.clone(),
                error: String::from("offline"),
            }],
            "100",
        );

        assert_eq!(runtime_state.vmm_writeback_outbox.len(), 1);
        let entry_id = runtime_state.vmm_writeback_outbox[0].id.clone();
        assert_eq!(runtime_state.vmm_writeback_outbox[0].attempt_count, 1);
        assert_eq!(
            runtime_state.vmm_writeback_outbox[0].last_error.as_deref(),
            Some("offline")
        );

        apply_vmm_writeback_effects_to_runtime_state(
            &mut runtime_state,
            vec![VmmWritebackHookEffect::Enqueue {
                permit_id: 1,
                request,
                error: String::from("still offline"),
            }],
            "200",
        );

        assert_eq!(runtime_state.vmm_writeback_outbox.len(), 1);
        assert_eq!(runtime_state.vmm_writeback_outbox[0].attempt_count, 2);
        assert_eq!(runtime_state.vmm_writeback_outbox[0].updated_at, "200");

        apply_vmm_writeback_effects_to_runtime_state(
            &mut runtime_state,
            vec![VmmWritebackHookEffect::Failed {
                entry_id: entry_id.clone(),
                error: String::from("replay failed"),
            }],
            "300",
        );

        assert_eq!(runtime_state.vmm_writeback_outbox[0].attempt_count, 3);
        assert_eq!(
            runtime_state.vmm_writeback_outbox[0].last_error.as_deref(),
            Some("replay failed")
        );

        apply_vmm_writeback_effects_to_runtime_state(
            &mut runtime_state,
            vec![VmmWritebackHookEffect::Flushed {
                entry_ids: vec![entry_id],
            }],
            "400",
        );

        assert!(runtime_state.vmm_writeback_outbox.is_empty());
    }

    /// Keeps repeated prompts in different turns and destinations distinct, then cancels only the selected queue ID.
    /// 区分不同回合及目的地的重复提示词，然后仅取消选中的队列 ID。
    #[test]
    fn writeback_identity_and_cancellation_preserve_other_turns_and_sources() {
        // Identical content cannot merge distinct turn identities or actual native bindings.
        // 相同正文不能合并不同回合身份或实际原生绑定。
        let first = test_post_action_request();
        let mut second = first.clone();
        second.operation_id = "turn-b".to_owned();
        let mut rebound = first.clone();
        rebound.expected_source = Some(vulcan_vmm::VmmMemorySource::native(12).expect("rebound"));
        assert_ne!(
            stable_vmm_outbox_request_hash(&first),
            stable_vmm_outbox_request_hash(&second)
        );
        assert_ne!(
            stable_vmm_outbox_request_hash(&first),
            stable_vmm_outbox_request_hash(&rebound)
        );
        // The actual effect consumer must remove only cancelled pending entries and ignore their late Failed effect.
        // 实际效果消费方必须仅移除已取消待处理项，并忽略其迟到的 Failed 效果。
        let mut state = SessionRuntimeStateEntry::default();
        apply_vmm_writeback_effects_to_runtime_state(
            &mut state,
            vec![
                VmmWritebackHookEffect::Enqueue {
                    permit_id: 1,
                    request: first,
                    error: "offline".to_owned(),
                },
                VmmWritebackHookEffect::Enqueue {
                    permit_id: 1,
                    request: second,
                    error: "offline".to_owned(),
                },
                VmmWritebackHookEffect::Enqueue {
                    permit_id: 1,
                    request: rebound,
                    error: "offline".to_owned(),
                },
            ],
            "1",
        );
        assert_eq!(state.vmm_writeback_outbox.len(), 3);
        let cancelled_id = state.vmm_writeback_outbox[0].id.clone();
        apply_vmm_writeback_effects_to_runtime_state(
            &mut state,
            vec![
                VmmWritebackHookEffect::Cancelled {
                    entry_ids: vec![cancelled_id.clone()],
                    reason: "source changed".to_owned(),
                },
                VmmWritebackHookEffect::Failed {
                    entry_id: cancelled_id.clone(),
                    error: "late".to_owned(),
                },
            ],
            "2",
        );
        assert_eq!(state.vmm_writeback_outbox.len(), 2);
        assert!(
            state
                .vmm_writeback_outbox
                .iter()
                .all(|entry| entry.id != cancelled_id && entry.attempt_count == 1)
        );
    }
}
