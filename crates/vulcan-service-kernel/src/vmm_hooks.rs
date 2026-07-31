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
    agent_runtime::{conversation_hooks::VmmWritebackHookEffect, vmm_hooks::VmmHookConfigSnapshot},
    provider_config::ProviderConfigurationFile,
};
use vulcan_workbench_model::SessionRuntimeStateEntry;
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use crate::{
    ServiceBridgeState, SharedServiceBridgeState, current_unix_millis_value,
    load_project_configuration, webui_trace, workspace_task_projects_including_archived,
};

/// Process-local project VMM layer cache keyed by normalized workspace path.
/// 按规范化工作区路径索引的进程内项目 VMM 配置层缓存。
static PROJECT_VMM_LAYER_CACHE: OnceLock<ProjectVmmLayerCache> = OnceLock::new();

/// Process-wide ephemeral transport injected only while the host-owned VMM child is ready.
/// 仅在宿主持有的 VMM 子进程就绪期间注入的进程级临时传输。
static MANAGED_VMM_TRANSPORT_OVERLAY: OnceLock<RwLock<Option<ManagedVmmTransportOverlay>>> =
    OnceLock::new();

/// Keeps the actual managed gRPC target and secret authorization outside persisted configuration.
/// 将实际托管 gRPC 目标与秘密鉴权信息保留在持久化配置之外。
#[derive(Clone)]
struct ManagedVmmTransportOverlay {
    /// Actual ready gRPC target.
    /// 实际就绪 gRPC 目标。
    grpc_target: String,
    /// Full authorization metadata value.
    /// 完整鉴权元数据值。
    authorization: String,
    /// Whether the managed generation can execute PreCheck.
    /// 当前托管代次是否能够执行 PreCheck。
    precheck_ready: bool,
    /// Whether the managed generation can execute PostAction.
    /// 当前托管代次是否能够执行 PostAction。
    postaction_ready: bool,
    /// Whether the exact configured numeric user/project scope resolved.
    /// 准确配置的数字用户/项目作用域是否解析成功。
    scope_ready: bool,
}

/// Installs one process-local managed transport without mutating persisted provider settings.
/// 安装一份进程内托管传输，且不修改持久化 Provider 设置。
pub(crate) fn install_managed_vmm_transport_overlay(
    grpc_target: String,
    authorization: String,
    precheck_ready: bool,
    postaction_ready: bool,
    scope_ready: bool,
) -> Result<(), String> {
    let overlay = MANAGED_VMM_TRANSPORT_OVERLAY.get_or_init(|| RwLock::new(None));
    let mut guard = overlay
        .write()
        .map_err(|_| String::from("managed VMM transport overlay is locked"))?;
    *guard = Some(ManagedVmmTransportOverlay {
        grpc_target,
        authorization,
        precheck_ready,
        postaction_ready,
        scope_ready,
    });
    Ok(())
}

/// Clears the process-local managed transport after the owned child stops.
/// 在持有的子进程停止后清除进程内托管传输。
pub(crate) fn clear_managed_vmm_transport_overlay() {
    let Some(overlay) = MANAGED_VMM_TRANSPORT_OVERLAY.get() else {
        return;
    };
    if let Ok(mut guard) = overlay.write() {
        *guard = None;
    }
}

/// Returns the current non-secret managed gRPC target for settings projection.
/// 返回用于设置投影的当前不含秘密托管 gRPC 目标。
pub(crate) fn managed_vmm_public_target() -> Option<String> {
    MANAGED_VMM_TRANSPORT_OVERLAY
        .get()
        .and_then(|overlay| overlay.read().ok())
        .and_then(|guard| guard.as_ref().map(|overlay| overlay.grpc_target.clone()))
}

/// Applies the current managed transport to every configuration layer so lower scopes cannot
/// replace an owned child with stale external connection data.
/// 将当前托管传输应用到每一层配置，使较低作用域无法用陈旧外部连接数据替换宿主持有的子进程。
fn apply_managed_vmm_transport_overlay(snapshot: &mut VmmHookConfigSnapshot) {
    let Some(overlay) = MANAGED_VMM_TRANSPORT_OVERLAY
        .get()
        .and_then(|overlay| overlay.read().ok())
        .and_then(|guard| guard.clone())
    else {
        return;
    };
    let apply = |layer: &mut vulcan_vmm::VmmConfigLayer| {
        layer.enabled = Some(true);
        layer.grpc_target = Some(overlay.grpc_target.clone());
        layer.grpc_api_key = Some(overlay.authorization.clone());
        if !overlay.scope_ready {
            layer.enabled = Some(false);
        }
        if !overlay.precheck_ready {
            layer.precheck_enabled = Some(false);
        }
        if !overlay.postaction_ready {
            layer.memory_writeback_enabled = Some(false);
        }
    };
    apply(&mut snapshot.global);
    for layer in snapshot.projects_by_workspace.values_mut() {
        apply(layer);
    }
    for layer in snapshot.sessions.values_mut() {
        apply(layer);
    }
}

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

/// Refreshes the retained VMM hook snapshot from shared service state.
/// 从共享服务状态刷新保留的 VMM hook 快照。
///
/// # Parameters
/// 参数
/// `state` is the shared service-kernel state holding runtime and workspace snapshots.
/// `state` 是持有运行时与工作区快照的共享 service-kernel 状态。
///
/// # Returns
/// 返回值
/// `Ok(())` after the hook snapshot has been refreshed or skipped when runtime state is absent.
/// 刷新 hook 快照完成后返回 `Ok(())`；运行态缺失时跳过并返回成功。
pub(super) fn refresh_service_vmm_hook_snapshot(
    state: &SharedServiceBridgeState,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone());
    if let Some(configuration) = configuration {
        refresh_guard_vmm_hook_snapshot(&mut guard, &configuration)?;
    }
    Ok(())
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
    if let (Some(agent_runtime), Some(workspace_snapshot)) = (
        guard.agent_runtime.as_ref(),
        guard.workspace_snapshot.as_ref(),
    ) {
        agent_runtime
            .vmm_hook_runtime
            .replace_snapshot(build_vmm_hook_config_snapshot(
                provider_configuration,
                workspace_snapshot,
            ))?;
    }
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
    let mut snapshot = VmmHookConfigSnapshot {
        global: provider_configuration.vmm.clone(),
        projects_by_workspace,
        sessions,
        outboxes,
        profile_sync,
        last_compaction_ids,
    };
    apply_managed_vmm_transport_overlay(&mut snapshot);
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
            VmmWritebackHookEffect::Flushed { entry_ids } => {
                let ids = entry_ids.into_iter().collect::<BTreeSet<_>>();
                runtime_state
                    .vmm_writeback_outbox
                    .retain(|entry| !ids.contains(&entry.id));
            }
            VmmWritebackHookEffect::Enqueue { request, error } => {
                let id = format!(
                    "vmm-outbox-{}-{}",
                    request.session_id,
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
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
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
            session_id: String::from("session-a"),
            user_id: 7,
            project_id: 11,
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
}
