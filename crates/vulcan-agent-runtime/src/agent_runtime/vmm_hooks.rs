//! Built-in VulcanMemoryMesh hook registration for conversation memory integration.
//! 面向会话记忆集成的内置 VulcanMemoryMesh hook 注册逻辑。

#[path = "vmm_writeback_admission.rs"]
mod writeback_admission;
pub use writeback_admission::VmmWritebackAdmissions;
use writeback_admission::reconcile_writeback_admissions;

use crate::agent_runtime::conversation_hooks::{
    HOOK_CONVERSATION_COMPACTED, HOOK_CONVERSATION_PROFILE_PREPARE,
    HOOK_CONVERSATION_TURN_COMPLETED, HOOK_CONVERSATION_TURN_PRECHECK,
    HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT, HOOK_METADATA_VMM_RECALL_BATCH_EFFECT,
    HOOK_METADATA_VMM_RECALL_NOTICE_EFFECT, HOOK_METADATA_VMM_SESSION_STATUS_EFFECT,
    HOOK_METADATA_VMM_WRITEBACK_EFFECTS, VmmProfileSyncHookEffect, VmmRecallBatchHookEffect,
    VmmSessionStatusHookEffect, VmmWritebackHookEffect,
};
use crate::model::{SessionId, VmmProfileSyncState, VmmWritebackOutboxEntry};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use vulcan_context_injection::{
    InjectionCompactionPolicy, InjectionInsert, InjectionInsertItem, InjectionKind,
};
use vulcan_hooks::{
    FnHookHandler, HookCapability, HookDiagnostic, HookDiagnosticSeverity, HookDispatcher,
    HookError, HookFailurePolicy, HookInvocation, HookMode, HookRegistration, HookResult,
    HookScope, SystemPromptHookBaselineKind, SystemPromptHookBaselineProvenance,
    SystemPromptHookBlock, SystemPromptHookPhase,
};
use vulcan_vmm::{
    GetProfileNodesRequest, ProfileNodeEntry, ProfileTarget, TonicVmmClient, VmmBackendMode,
    VmmClient, VmmConfigDiagnostic, VmmConfigLayer, VmmEffectiveConfig, VmmRecallBatch,
    VmmRecallNotice, VmmTransportConfig, chat_compact_request_from_payload,
    post_action_request_from_completed_payload, precheck_request_from_payload,
};

/// Stable id used by the built-in VMM precheck hook.
/// 内置 VMM 预检查 hook 使用的稳定标识。
pub const VMM_PRECHECK_HOOK_ID: &str = "builtin.vmm.precheck";
/// Stable id used by the independent built-in VMM profile preparation hook.
/// 独立内置 VMM 画像准备 Hook 使用的稳定标识。
const VMM_PROFILE_PREPARE_HOOK_ID: &str = "builtin.vmm.profile_prepare";
/// Stable id used by the built-in VMM post-action hook.
/// 内置 VMM 回合完成回写 hook 使用的稳定标识。
const VMM_POST_ACTION_HOOK_ID: &str = "builtin.vmm.post_action";
/// Stable id used by the built-in VMM chat-compaction hook.
/// 内置 VMM 会话压缩 hook 使用的稳定标识。
const VMM_CHAT_COMPACT_HOOK_ID: &str = "builtin.vmm.chat_compact";
/// Host-side ceiling for interactive VMM precheck calls.
/// 交互式 VMM Precheck 调用的宿主侧兜底上限。
const VMM_PRECHECK_HOOK_TIMEOUT_MS: u64 = 30_000;
/// Host-side ceiling for VMM effects and background preparation calls.
/// VMM 副作用与后台准备调用的宿主侧兜底上限。
const VMM_BACKGROUND_HOOK_TIMEOUT_MS: u64 = 120_000;
/// Stable system-prompt tag used for VMM profile injection.
/// VMM 画像注入使用的稳定系统提示词标签。
const VMM_PROFILE_SYSTEM_PROMPT_TAG: &str = "vmm_profile";
/// Maximum active profile nodes pulled for local diff snapshots per target.
/// 每个目标为本地差量快照拉取的最大 active 画像节点数。
const VMM_PROFILE_SYNC_NODE_LIMIT: u32 = 256;
/// Stable hidden-user injection tag used for VMM profile delta updates.
/// VMM 画像增量更新使用的稳定隐藏用户注入标签。
const VMM_PROFILE_DELTA_INJECTION_TAG: &str = "vmm_profile_delta";

/// Shared VMM configuration snapshot captured by hook closures.
/// 被 hook 闭包捕获的共享 VMM 配置快照。
#[derive(Debug, Clone, Default)]
pub struct VmmHookConfigSnapshot {
    /// Live non-serializable delivery authority, preserved independently of incoming configuration mirrors.
    /// 活动不可序列化投递权限，独立于传入配置镜像保留。
    pub writeback_admissions: VmmWritebackAdmissions,
    /// Global VMM configuration layer.
    /// 全局 VMM 配置层。
    pub global: VmmConfigLayer,
    /// Project-level VMM configuration layers keyed by workspace path.
    /// 按工作区路径索引的项目级 VMM 配置层。
    pub projects_by_workspace: BTreeMap<String, VmmConfigLayer>,
    /// Session-level VMM configuration layers keyed by session id string.
    /// 按会话标识字符串索引的会话级 VMM 配置层。
    pub sessions: BTreeMap<String, VmmConfigLayer>,
    /// Pending writeback outboxes keyed by session id string.
    /// 按会话标识字符串索引的待处理写回 outbox。
    pub outboxes: BTreeMap<String, Vec<VmmWritebackOutboxEntry>>,
    /// Profile synchronization states keyed by session id string.
    /// 按会话标识字符串索引的画像同步状态。
    pub profile_sync: BTreeMap<String, VmmProfileSyncState>,
    /// Latest compaction identifiers keyed by session id string.
    /// 按会话标识字符串索引的最近压缩标识。
    pub last_compaction_ids: BTreeMap<String, Option<String>>,
}

/// Shared runtime handle used by built-in VMM hook handlers.
/// 内置 VMM hook 处理器使用的共享运行时句柄。
#[derive(Clone)]
pub struct VmmHookRuntimeHandle {
    /// Authoritative configuration and runtime profile snapshot visible to hook handlers.
    /// hook 处理器可见的权威配置与运行时画像快照。
    state: Arc<RwLock<VmmHookConfigSnapshot>>,
    /// Process-local native VMM client installed by the VulcanCode service host.
    /// 由 VulcanCode 服务宿主安装的进程内原生 VMM 客户端。
    native_client: Arc<RwLock<Option<Arc<dyn VmmClient>>>>,
    /// Process-local admission epoch advanced when global activation or backend selection changes.
    /// 全局启停或后端选择变化时推进的进程内准入代次。
    profile_admission_epoch: Arc<AtomicU64>,
}

impl VmmHookRuntimeHandle {
    /// Validates VMM read publication while preventing configuration changes during a synchronous cache or profile commit.
    /// 在同步缓存或画像提交期间阻止配置变化，并校验记忆读取发布。
    /// Takes exact session/workspace/source and a commit callback; native callers retain their binding barrier.
    /// 接收准确会话/工作区/来源及提交回调；原生调用方须保持其绑定屏障。
    /// Returns the callback result only for a current enabled source; no AI or remote scope lookup is performed.
    /// 仅对当前启用的来源返回回调结果；不执行 AI 或远端范围查询。
    pub fn with_current_profile_publication<R>(
        &self,
        session_id: SessionId,
        workspace_path: &str,
        expected: &vulcan_vmm::VmmMemoryReadSource,
        publish: impl FnOnce() -> R,
    ) -> Result<R, String> {
        expected.validate().map_err(|error| error.to_string())?;
        // Any service-state acquisition must precede this guard; callbacks neither acquire it nor refresh hooks.
        // 任何服务状态锁的获取必须先于本守卫；回调既不获取该锁，也不刷新 Hook。
        let snapshot = self.read_state("validate final VMM profile publication")?;
        // A project override cannot keep publishing after the application-wide service is disabled.
        // 应用级服务关闭后，项目覆盖不能继续发布结果。
        let service_enabled = VmmEffectiveConfig::resolve(&snapshot.global, None, None).enabled;
        let config = VmmEffectiveConfig::resolve(
            &snapshot.global,
            snapshot.projects_by_workspace.get(workspace_path),
            snapshot.sessions.get(&session_id.to_string()),
        );
        if !service_enabled
            || !config.enabled
            || config.backend_mode
                != match &expected.owner {
                    vulcan_vmm::VmmMemorySource::Native { .. } => VmmBackendMode::Native,
                    vulcan_vmm::VmmMemorySource::ExternalGrpc { .. } => {
                        VmmBackendMode::ExternalGrpc
                    }
                }
        {
            return Err(
                "VMM profile source changed before the prepared request could be committed."
                    .to_owned(),
            );
        }
        if config.backend_mode == VmmBackendMode::ExternalGrpc {
            // External visibility is opaque; only declared transport and subject/project identity are checked.
            // 外置可见性不透明；只核对声明的传输及主体/项目身份。
            let actual = vulcan_vmm::VmmMemorySource::external(
                &VmmTransportConfig::from_effective(&config),
                config.user_id.ok_or_else(|| {
                    "VMM profile publication requires a current user ID.".to_owned()
                })?,
                config.project_id.ok_or_else(|| {
                    "VMM profile publication requires a current project ID.".to_owned()
                })?,
            )
            .map_err(|error| error.to_string())?;
            if actual != expected.owner {
                return Err(
                    "VMM profile source changed before the prepared request could be committed."
                        .to_owned(),
                );
            }
        }
        Ok(publish())
    }
    /// Resolves current publication policy from the same atomic projection used by live hooks.
    /// 从活动 Hook 使用的同一原子投影解析当前发布策略。
    /// Takes exact session and workspace identities; returns the declared configuration precedence.
    /// 接收准确会话及工作区身份；返回声明的配置优先级解析结果。
    pub fn resolve_publication_config(
        &self,
        session_id: SessionId,
        workspace_path: &str,
    ) -> Result<VmmEffectiveConfig, String> {
        // Configuration refresh owns this snapshot, not the detached agent bridge.
        // 配置刷新拥有本快照，而不是脱离状态的 Agent 桥接实例。
        let snapshot = self.read_state("resolve VMM publication configuration")?;
        Ok(VmmEffectiveConfig::resolve(
            &snapshot.global,
            snapshot.projects_by_workspace.get(workspace_path),
            snapshot.sessions.get(&session_id.to_string()),
        ))
    }

    /// Reads the active profile for a memory panel without publishing a prompt baseline.
    /// 为记忆侧栏读取当前有效画像，但不发布提示词基线。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` and `workspace_path` select the exact effective VMM policy; `host_project_id`
    /// identifies the current host project for visibility binding.
    /// `session_id` 与 `workspace_path` 选择精确的 VMM 生效策略；`host_project_id` 标识当前宿主项目以约束可见范围。
    ///
    /// # Returns
    /// 返回值
    /// The current active nodes, `None` when VMM is disabled, or an explicit read error.
    /// 返回当前有效节点；VMM 关闭时返回 `None`，读取失败时返回明确错误。
    pub async fn read_static_profile_for_panel(
        &self,
        session_id: SessionId,
        workspace_path: &str,
        host_project_id: &str,
    ) -> Result<Option<BTreeMap<u64, ProfileNodeEntry>>, String> {
        let config = self.resolve_publication_config(session_id, workspace_path)?;
        if !config.enabled {
            return Ok(None);
        }
        ensure_vmm_ready(&config).map_err(|error| error.to_string())?;
        let client = resolve_vmm_client(self, &config)
            .await
            .map_err(|error| error.to_string())?;
        let source = client
            .memory_read_source(
                host_project_id,
                config.user_id.unwrap_or_default(),
                config.project_id.unwrap_or_default(),
            )
            .await
            .map_err(|error| error.to_string())?;
        let nodes = fetch_all_profile_nodes(client.as_ref(), &config, host_project_id, &source)
            .await
            .map_err(|error| error.to_string())?;
        self.with_current_profile_publication(session_id, workspace_path, &source, || ())?;
        Ok(Some(nodes))
    }
    /// Builds a shared VMM hook runtime handle.
    /// 构建共享 VMM hook 运行时句柄。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the initial authoritative configuration and profile state.
    /// `snapshot` 是初始权威配置与画像状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns a cloneable handle backed by one atomic runtime snapshot.
    /// 返回由单一原子运行时快照支撑的可克隆句柄。
    pub fn new(snapshot: VmmHookConfigSnapshot) -> Self {
        Self {
            state: Arc::new(RwLock::new(snapshot)),
            native_client: Arc::new(RwLock::new(None)),
            profile_admission_epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Installs the process-local native VMM client used by native backend invocations.
    /// 安装由原生后端调用使用的进程内 VMM 客户端。
    ///
    /// # Parameters
    /// 参数
    /// `client` is the service-hosted native implementation of the shared VMM client contract.
    /// `client` 是由服务宿主管理并实现共享 VMM 客户端契约的原生实现。
    ///
    /// # Returns
    /// 返回值
    /// Returns an explicit error when the native-client registry lock is poisoned.
    /// 当原生客户端注册锁中毒时返回显式错误。
    pub fn install_native_client(&self, client: Arc<dyn VmmClient>) -> Result<(), String> {
        let mut slot = self.native_client.write().map_err(|_| {
            String::from("The native VMM client registry is locked and cannot update.")
        })?;
        *slot = Some(client);
        Ok(())
    }

    /// Reads the currently installed process-local native VMM client.
    /// 读取当前已安装的进程内原生 VMM 客户端。
    ///
    /// # Returns
    /// 返回值
    /// Returns the installed client or an explicit readiness diagnostic.
    /// 返回已安装客户端或显式就绪诊断。
    fn native_client(&self) -> Result<Arc<dyn VmmClient>, HookError> {
        self.native_client
            .read()
            .map_err(|_| {
                HookError::handler("The native VMM client registry is locked and cannot be read.")
            })?
            .clone()
            .ok_or_else(|| {
                HookError::handler(
                    "The native VMM backend is enabled but its service client is not installed.",
                )
            })
    }

    /// Acquires the authoritative VMM runtime state for a read operation.
    /// 为读取操作获取权威 VMM 运行时状态。
    ///
    /// # Parameters
    /// 参数
    /// `operation` describes the rejected operation when the lock is poisoned.
    /// `operation` 描述锁中毒时被拒绝的操作。
    ///
    /// # Returns
    /// 返回值
    /// Returns the read guard or an explicit state-lock diagnostic.
    /// 返回读守卫或明确的状态锁诊断。
    fn read_state(
        &self,
        operation: &str,
    ) -> Result<RwLockReadGuard<'_, VmmHookConfigSnapshot>, String> {
        self.state
            .read()
            .map_err(|_| format!("The VMM hook runtime state is locked and cannot {operation}."))
    }

    /// Acquires the authoritative VMM runtime state for a write operation.
    /// 为写入操作获取权威 VMM 运行时状态。
    ///
    /// # Parameters
    /// 参数
    /// `operation` describes the rejected operation when the lock is poisoned.
    /// `operation` 描述锁中毒时被拒绝的操作。
    ///
    /// # Returns
    /// 返回值
    /// Returns the write guard or an explicit state-lock diagnostic.
    /// 返回写守卫或明确的状态锁诊断。
    fn write_state(
        &self,
        operation: &str,
    ) -> Result<RwLockWriteGuard<'_, VmmHookConfigSnapshot>, String> {
        self.state
            .write()
            .map_err(|_| format!("The VMM hook runtime state is locked and cannot {operation}."))
    }

    /// Replaces configuration while preserving runtime profiles only across an unchanged global activation mode.
    /// 替换配置，仅在全局启用模式不变时保留运行时画像。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` contains the latest configuration, outbox and compaction projection.
    /// `snapshot` 包含最新配置、outbox 与压缩投影。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after one atomic replacement, or a state-lock diagnostic.
    /// 单次原子替换完成后返回成功，失败时返回状态锁诊断。
    pub fn replace_snapshot(&self, mut snapshot: VmmHookConfigSnapshot) -> Result<(), String> {
        // state is the only authority for both configuration and runtime-produced profile state.
        // state 是配置与运行时生成画像状态的唯一权威来源。
        let mut state = self.write_state("replace the configuration snapshot")?;
        // Mode transitions cancel admitted profile preparation, including a later return to its old source.
        // 模式转换取消已准入画像准备，包括之后切回其旧来源的情况。
        if configured_memory_mode(&state.global) != configured_memory_mode(&snapshot.global) {
            self.profile_admission_epoch.fetch_add(1, Ordering::AcqRel);
            state.profile_sync.clear();
        }
        snapshot.profile_sync = std::mem::take(&mut state.profile_sync);
        snapshot.writeback_admissions = std::mem::take(&mut state.writeback_admissions);
        reconcile_writeback_admissions(&mut snapshot);
        *state = snapshot;
        Ok(())
    }

    /// Replaces the global layer and invalidates profile caches on mode changes, preserving project/session configuration maps.
    /// 替换全局层，模式变化时使画像缓存失效，保留项目及会话配置映射。
    ///
    /// # Parameters
    /// 参数
    /// `global` is the complete authoritative global VMM layer.
    /// `global` 是完整的权威全局 VMM 配置层。
    /// # Returns
    /// 返回值
    /// Returns success after one atomic configuration/cache transition, or a state-lock diagnostic.
    /// 单次原子配置及缓存转换完成后返回成功，失败时返回状态锁诊断。
    pub fn replace_global_projection(&self, global: &VmmConfigLayer) -> Result<(), String> {
        let mut state = self.write_state("replace the global VMM projection")?;
        // Ordinary tuning on the same active backend retains its valid baseline and admission epoch.
        // 同一活动后端的普通调参保留有效基线及准入代次。
        let next_mode = configured_memory_mode(global);
        if configured_memory_mode(&state.global) != next_mode {
            self.profile_admission_epoch.fetch_add(1, Ordering::AcqRel);
            state.profile_sync.clear();
        } else if next_mode.is_none() {
            state.profile_sync.clear();
        }
        state.global = global.clone();
        reconcile_writeback_admissions(&mut state);
        Ok(())
    }

    /// Reads global memory activation without fabricating a session or workspace identity.
    /// 读取全局记忆启用状态，不虚构会话或工作区身份。
    /// Returns the selected backend when enabled, otherwise none; lock failure remains explicit.
    /// 启用时返回所选后端，否则返回空；锁失败保持显式。
    pub fn global_memory_mode(&self) -> Result<Option<VmmBackendMode>, String> {
        let state = self.read_state("inspect global memory activation")?;
        Ok(configured_memory_mode(&state.global))
    }

    /// Replaces one exact project configuration layer without rebuilding other projects.
    /// 替换单个精确项目配置层，且不重建其他项目。
    ///
    /// # Parameters
    /// 参数
    /// `workspace_path` is the exact normalized project key used by hook payloads.
    /// `workspace_path` 是 hook 载荷使用的精确规范化项目键。
    /// `project` is the complete project layer; an empty layer removes the entry.
    /// `project` 是完整项目配置层；空配置层会移除对应条目。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after one exact map update, or a state-lock diagnostic.
    /// 单次精确映射更新完成后返回成功，失败时返回状态锁诊断。
    pub fn replace_project_projection(
        &self,
        workspace_path: &str,
        project: &VmmConfigLayer,
    ) -> Result<(), String> {
        let mut state = self.write_state("replace one project VMM projection")?;
        if project.is_empty() {
            state.projects_by_workspace.remove(workspace_path);
        } else {
            state
                .projects_by_workspace
                .insert(workspace_path.to_string(), project.clone());
        }
        reconcile_writeback_admissions(&mut state);
        Ok(())
    }

    /// Replaces the configuration, outbox, and compaction projection for one exact session.
    /// 替换单个精确会话的配置、outbox 与压缩投影。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the only session entry allowed to change.
    /// `session_id` 标识唯一允许发生变化的会话条目。
    /// `vmm` is the complete session configuration layer; an empty layer removes the entry.
    /// `vmm` 是完整会话配置层；空配置层会移除对应条目。
    /// `outbox` is the complete pending writeback queue; an empty queue removes the entry.
    /// `outbox` 是完整待处理写回队列；空队列会移除对应条目。
    /// `last_compaction_id` is the latest exact compaction boundary for this session.
    /// `last_compaction_id` 是该会话最近一次精确压缩边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after one constant-scope update, or a state-lock diagnostic.
    /// 单次常量范围更新完成后返回成功，失败时返回状态锁诊断。
    pub fn replace_session_projection(
        &self,
        session_id: SessionId,
        vmm: &VmmConfigLayer,
        outbox: &[VmmWritebackOutboxEntry],
        last_compaction_id: Option<&str>,
    ) -> Result<(), String> {
        // sessionKey is shared by the three independently sparse session projections.
        // sessionKey 由三个相互独立的稀疏会话投影共同使用。
        let session_key = session_id.to_string();
        let mut state = self.write_state("replace one session VMM projection")?;
        if vmm.is_empty() {
            state.sessions.remove(&session_key);
        } else {
            state.sessions.insert(session_key.clone(), vmm.clone());
        }
        if outbox.is_empty() {
            state.outboxes.remove(&session_key);
        } else {
            state.outboxes.insert(session_key.clone(), outbox.to_vec());
        }
        if let Some(last_compaction_id) = last_compaction_id {
            state
                .last_compaction_ids
                .insert(session_key, Some(last_compaction_id.to_string()));
        } else {
            state.last_compaction_ids.remove(&session_key);
        }
        reconcile_writeback_admissions(&mut state);
        Ok(())
    }

    /// Removes all non-profile VMM projections owned by one exact session.
    /// 移除单个精确会话拥有的全部非画像 VMM 投影。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the exact missing or deleted session.
    /// `session_id` 标识精确缺失或已删除的会话。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after removing the exact keys, or a state-lock diagnostic.
    /// 移除精确键后返回成功，失败时返回状态锁诊断。
    pub fn remove_session_projection(&self, session_id: SessionId) -> Result<(), String> {
        let session_key = session_id.to_string();
        let mut state = self.write_state("remove one session VMM projection")?;
        state.writeback_admissions.remove_session(&session_key);
        state.sessions.remove(&session_key);
        state.outboxes.remove(&session_key);
        state.last_compaction_ids.remove(&session_key);
        Ok(())
    }

    /// Seeds runtime profile synchronization state from persisted workspace data.
    /// 使用持久化工作区数据播种运行时画像同步状态。
    ///
    /// # Parameters
    /// 参数
    /// `profile_sync` is the complete persisted session-to-profile-state mapping.
    /// `profile_sync` 是完整的持久化会话到画像状态映射。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the authoritative snapshot is updated, or a lock diagnostic.
    /// 权威快照更新后返回成功，失败时返回锁诊断。
    pub fn seed_profile_sync(
        &self,
        profile_sync: BTreeMap<String, VmmProfileSyncState>,
    ) -> Result<(), String> {
        self.write_state("seed persisted profile synchronization state")?
            .profile_sync = profile_sync;
        Ok(())
    }

    /// Stores a freshly produced profile sync state in the authoritative runtime snapshot.
    /// 将刚产生的画像同步状态写入权威运行时快照。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the exact session profile entry to replace or remove.
    /// `session_id` 标识需要替换或移除的精确会话画像项。
    /// `profile_sync` is the new profile state; an empty state removes the entry.
    /// `profile_sync` 是新的画像状态；空状态会移除该项。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after one authoritative write, or a state-lock diagnostic.
    /// 单次权威写入完成后返回成功，失败时返回状态锁诊断。
    fn store_profile_sync_for_session(
        &self,
        session_id: &str,
        profile_sync: VmmProfileSyncState,
        admitted_epoch: Option<u64>,
    ) -> Result<(), String> {
        // state owns the exact session entry and prevents a second cache from diverging.
        // state 持有精确会话项，避免第二份缓存发生分叉。
        let mut state = self.write_state("store session profile synchronization state")?;
        // Compare under the global-replacement lock so late results cannot resurrect cleared caches.
        // 在全局替换锁内比较，防止迟到结果复活已清除缓存。
        if admitted_epoch
            .is_some_and(|epoch| epoch != self.profile_admission_epoch.load(Ordering::Acquire))
        {
            return Err(
                "VMM profile preparation was cancelled by a global memory mode change.".to_owned(),
            );
        }
        if profile_sync.is_empty() {
            state.profile_sync.remove(session_id);
        } else {
            state
                .profile_sync
                .insert(session_id.to_string(), profile_sync);
        }
        Ok(())
    }

    /// Stores a freshly produced profile sync state in the authoritative runtime snapshot.
    /// 将刚产生的画像同步状态写入权威运行时快照。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the session whose profile state changed.
    /// `session_id` 标识画像状态发生变化的会话。
    /// `profile_sync` is the complete replacement profile state for that session.
    /// `profile_sync` 是该会话完整的替代画像状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the state is stored, or a state-lock diagnostic.
    /// 状态存储后返回成功，失败时返回状态锁诊断。
    pub fn store_profile_sync_state(
        &self,
        session_id: SessionId,
        profile_sync: VmmProfileSyncState,
    ) -> Result<(), String> {
        self.store_profile_sync_for_session(&session_id.to_string(), profile_sync, None)
    }

    /// Resolves the effective VMM configuration for one hook invocation payload.
    /// 为一次 hook 调用载荷解析最终 VMM 配置。
    ///
    /// # Parameters
    /// 参数
    /// `invocation` provides workspace and session identity for layered resolution.
    /// `invocation` 提供分层解析所需的工作区与会话身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the resolved configuration, or a state-lock diagnostic.
    /// 返回解析后的配置，失败时返回状态锁诊断。
    fn resolve_effective_config(
        &self,
        invocation: &HookInvocation,
    ) -> Result<VmmEffectiveConfig, String> {
        self.resolve_profile_admission(invocation)
            .map(|(config, _)| config)
    }

    /// Resolves configuration and the profile admission epoch from one atomic read snapshot.
    /// 从同一个原子读取快照解析配置及画像准入代次。
    /// Takes the actual Hook invocation; returns the resolved config and process-local epoch.
    /// 接收实际 Hook 调用；返回解析配置及进程内代次。
    fn resolve_profile_admission(
        &self,
        invocation: &HookInvocation,
    ) -> Result<(VmmEffectiveConfig, u64), String> {
        // snapshot keeps all layer references valid during one coherent resolution.
        // snapshot 在一次一致解析期间保持所有配置层引用有效。
        let snapshot = self.read_state("resolve effective configuration")?;
        let workspace_path = payload_string(&invocation.payload.value, "workspace_path");
        let project = workspace_path
            .as_deref()
            .and_then(|path| snapshot.projects_by_workspace.get(path));
        let session_id = invocation
            .context
            .session_id
            .clone()
            .or_else(|| payload_string(&invocation.payload.value, "session_id"));
        let session = session_id
            .as_deref()
            .and_then(|session_id| snapshot.sessions.get(session_id));
        Ok((
            VmmEffectiveConfig::resolve(&snapshot.global, project, session),
            self.profile_admission_epoch.load(Ordering::Acquire),
        ))
    }

    /// Loads the pending writeback outbox for one hook invocation.
    /// 为一次 hook 调用读取待处理写回 outbox。
    ///
    /// # Parameters
    /// 参数
    /// `invocation` provides the session identity owning the outbox.
    /// `invocation` 提供拥有 outbox 的会话身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact session outbox, an empty outbox when genuinely absent, or a lock error.
    /// 返回精确会话 outbox，确实不存在时返回空 outbox，锁失败时返回错误。
    fn outbox_for_invocation(
        &self,
        invocation: &HookInvocation,
    ) -> Result<Vec<VmmWritebackOutboxEntry>, String> {
        let snapshot = self.read_state("load the session writeback outbox")?;
        let session_id = invocation
            .context
            .session_id
            .clone()
            .or_else(|| payload_string(&invocation.payload.value, "session_id"));
        Ok(session_id
            .as_deref()
            .and_then(|session_id| snapshot.outboxes.get(session_id))
            .cloned()
            .unwrap_or_default())
    }

    /// Loads the profile sync state for one hook invocation.
    /// 为一次 hook 调用读取画像同步状态。
    ///
    /// # Parameters
    /// 参数
    /// `invocation` provides the session identity owning the profile state.
    /// `invocation` 提供拥有画像状态的会话身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact profile state, an empty state when genuinely absent, or a lock error.
    /// 返回精确画像状态，确实不存在时返回空状态，锁失败时返回错误。
    fn profile_sync_for_invocation(
        &self,
        invocation: &HookInvocation,
    ) -> Result<VmmProfileSyncState, String> {
        let snapshot = self.read_state("load the session profile synchronization state")?;
        Ok(invocation_session_id(invocation)
            .as_deref()
            .and_then(|session_id| snapshot.profile_sync.get(session_id))
            .cloned()
            .unwrap_or_default())
    }

    /// Loads the latest compaction id for one hook invocation.
    /// 为一次 hook 调用读取最近压缩标识。
    ///
    /// # Parameters
    /// 参数
    /// `invocation` provides the session identity owning the compaction marker.
    /// `invocation` 提供拥有压缩标识的会话身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact optional compaction id, or a state-lock diagnostic.
    /// 返回精确的可选压缩标识，失败时返回状态锁诊断。
    fn last_compaction_id_for_invocation(
        &self,
        invocation: &HookInvocation,
    ) -> Result<Option<String>, String> {
        let snapshot = self.read_state("load the latest session compaction identifier")?;
        Ok(invocation_session_id(invocation)
            .as_deref()
            .and_then(|session_id| snapshot.last_compaction_ids.get(session_id))
            .cloned()
            .flatten())
    }
}

/// Registers built-in VMM hooks on the provided conversation hook dispatcher.
/// 在给定会话 hook 调度器上注册内置 VMM hook。
pub fn register_builtin_vmm_hooks(
    dispatcher: &mut HookDispatcher,
    runtime: VmmHookRuntimeHandle,
) -> Result<(), HookError> {
    let profile_runtime = runtime.clone();
    dispatcher.register(
        vmm_hook_registration(
            VMM_PROFILE_PREPARE_HOOK_ID,
            HOOK_CONVERSATION_PROFILE_PREPARE,
            HookMode::Transform,
        ),
        Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
            let runtime = profile_runtime.clone();
            async move { handle_vmm_profile_prepare(runtime, invocation).await }
        })),
    )?;

    let precheck_runtime = runtime.clone();
    dispatcher.register(
        vmm_hook_registration(
            VMM_PRECHECK_HOOK_ID,
            HOOK_CONVERSATION_TURN_PRECHECK,
            HookMode::Transform,
        ),
        Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
            let runtime = precheck_runtime.clone();
            async move { handle_vmm_precheck(runtime, invocation).await }
        })),
    )?;

    let post_action_runtime = runtime.clone();
    dispatcher.register(
        vmm_hook_registration(
            VMM_POST_ACTION_HOOK_ID,
            HOOK_CONVERSATION_TURN_COMPLETED,
            HookMode::Effect,
        ),
        Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
            let runtime = post_action_runtime.clone();
            async move { handle_vmm_post_action(runtime, invocation).await }
        })),
    )?;

    dispatcher.register(
        vmm_hook_registration(
            VMM_CHAT_COMPACT_HOOK_ID,
            HOOK_CONVERSATION_COMPACTED,
            HookMode::Effect,
        ),
        Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
            let runtime = runtime.clone();
            async move { handle_vmm_chat_compact(runtime, invocation).await }
        })),
    )?;
    Ok(())
}

/// Builds one built-in VMM hook registration with non-blocking failure behavior.
/// 构建一条具备非阻塞失败策略的内置 VMM hook 注册项。
fn vmm_hook_registration(id: &str, point: &str, mode: HookMode) -> HookRegistration {
    let mut registration = HookRegistration::new(id, point, mode);
    registration.scope = HookScope::Session;
    registration.priority = -100;
    registration.timeout_ms = Some(vmm_hook_timeout_ms(id));
    registration.failure_policy = HookFailurePolicy::Continue;
    registration
        .requested_capabilities
        .insert(HookCapability::ConfigRead);
    registration
        .requested_capabilities
        .insert(HookCapability::Network);
    if mode == HookMode::Transform {
        registration
            .requested_capabilities
            .insert(HookCapability::InjectionWrite);
    }
    registration.display_name = Some(String::from("VulcanMemoryMesh"));
    if id == VMM_PRECHECK_HOOK_ID {
        registration.progress_label = Some(String::from("Recalling memories"));
    }
    registration
}

/// Selects the host-side timeout ceiling for one built-in VMM hook.
/// 为一个内置 VMM Hook 选择宿主侧超时上限。
///
/// # Parameters
/// 参数
/// `id` is the immutable built-in hook identifier registered by the host.
/// `id` 是宿主注册的不可变内置 Hook 标识。
///
/// # Returns
/// 返回值
/// The stage-specific host timeout ceiling in milliseconds.
/// 返回以毫秒表示的阶段专属宿主超时上限。
fn vmm_hook_timeout_ms(id: &str) -> u64 {
    if id == VMM_PRECHECK_HOOK_ID {
        VMM_PRECHECK_HOOK_TIMEOUT_MS
    } else {
        VMM_BACKGROUND_HOOK_TIMEOUT_MS
    }
}

/// Resolves only the global master switch and backend identity, never project/session overrides.
/// 仅解析全局总开关及后端身份，不使用项目或会话覆盖。
/// Takes the saved global layer; returns the enabled mode or none.
/// 接收保存的全局层；返回已启用模式或空。
fn configured_memory_mode(global: &VmmConfigLayer) -> Option<VmmBackendMode> {
    let config = VmmEffectiveConfig::resolve(global, None, None);
    config.enabled.then_some(config.backend_mode)
}

/// Returns whether one effective VMM configuration should run automatic precheck now.
/// 返回一个最终 VMM 配置当前是否应运行自动 precheck。
fn should_run_vmm_precheck(config: &VmmEffectiveConfig) -> bool {
    config.diagnose_precheck_flow(true) == VmmConfigDiagnostic::Ready
}

/// Returns whether independent VMM profile preparation may run now.
/// 返回独立 VMM 画像准备当前是否可以运行。
fn should_run_vmm_profile_prepare(config: &VmmEffectiveConfig) -> bool {
    config.diagnose_memory_flow(true) == VmmConfigDiagnostic::Ready
}

/// Returns the current Unix timestamp in milliseconds as stable text.
/// 返回当前 Unix 毫秒时间戳的稳定文本形式。
fn current_unix_millis_text() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| String::from("0"))
}

/// Prepares the persistent VMM profile baseline and append-only profile deltas.
/// 准备持久化 VMM 画像基线与仅追加的画像增量。
///
/// # Parameters
/// 参数
/// `runtime` owns the authoritative profile state and effective configuration snapshot.
/// `runtime` 持有权威画像状态与有效配置快照。
/// `invocation` carries the exact session and prompt-turn boundary.
/// `invocation` 携带精确会话及提示词回合边界。
///
/// # Returns
/// 返回值
/// A profile-only Hook result that never contains precheck recall state.
/// 返回绝不包含 precheck 召回状态的纯画像 Hook 结果。
async fn handle_vmm_profile_prepare(
    runtime: VmmHookRuntimeHandle,
    invocation: HookInvocation,
) -> Result<HookResult, HookError> {
    if !invocation_vmm_runtime_ready(&invocation) {
        return Ok(HookResult::default());
    }
    // Admit the configuration and epoch together before asynchronous profile reads.
    // 在异步画像读取前，一起准入配置及代次。
    let (config, admitted_epoch) = runtime
        .resolve_profile_admission(&invocation)
        .map_err(HookError::handler)?;
    if !should_run_vmm_profile_prepare(&config) {
        return Ok(HookResult::default());
    }
    ensure_vmm_ready(&config)?;
    let client = resolve_vmm_client(&runtime, &config).await?;
    let Some(profile_sync) =
        maybe_sync_vmm_profile(client.as_ref(), &runtime, &invocation, &config).await?
    else {
        return Ok(HookResult::default());
    };
    if let Some(session_id) = invocation_session_id(&invocation) {
        runtime
            .store_profile_sync_for_session(
                &session_id,
                profile_sync.state.clone(),
                Some(admitted_epoch),
            )
            .map_err(HookError::handler)?;
    }
    // metadata transports the updated profile state on the profile-only line.
    // metadata 仅在画像线路上传输更新后的画像状态。
    let mut metadata = BTreeMap::new();
    metadata.insert(
        HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT.to_string(),
        json!(VmmProfileSyncHookEffect {
            state: profile_sync.state
        }),
    );
    // system_prompt_blocks contains only the stable baseline owned by profile preparation.
    // system_prompt_blocks 仅包含画像准备持有的稳定基线。
    let system_prompt_blocks = profile_sync.block.into_iter().collect();
    Ok(HookResult {
        injections: profile_sync.injections,
        system_prompt_blocks,
        metadata,
        ..HookResult::default()
    })
}

/// Runs the VMM precheck request and returns only a typed recall effect.
/// 执行 VMM precheck 请求并且只返回类型化召回效果。
async fn handle_vmm_precheck(
    runtime: VmmHookRuntimeHandle,
    invocation: HookInvocation,
) -> Result<HookResult, HookError> {
    let runtime_ready = invocation_vmm_runtime_ready(&invocation);
    eprintln!(
        "[webui-service][vmm-precheck] stage=hook_entry session={:?} runtime_ready={}",
        invocation_session_id(&invocation),
        runtime_ready,
    );
    if !runtime_ready {
        return Ok(HookResult::default());
    }
    let config = runtime
        .resolve_effective_config(&invocation)
        .map_err(HookError::handler)?;
    eprintln!(
        "[webui-service][vmm-precheck] stage=hook_config session={:?} backend={:?} diagnostic={:?}",
        invocation_session_id(&invocation),
        config.backend_mode,
        config.diagnose_precheck_flow(true),
    );
    if !should_run_vmm_precheck(&config) {
        return Ok(HookResult::default());
    }
    let response = match request_vmm_precheck(&runtime, &invocation, &config).await {
        Ok(response) => response,
        Err(error) => return vmm_precheck_failure_result(&config, error),
    };
    eprintln!(
        "[webui-service][vmm-precheck] stage=response session={:?} result=ok context_items={} should_inject={} degraded={}",
        invocation_session_id(&invocation),
        response.context_items.len(),
        response.should_inject,
        response.degraded,
    );
    let mut metadata = BTreeMap::new();
    metadata.insert(
        HOOK_METADATA_VMM_SESSION_STATUS_EFFECT.to_string(),
        json!(VmmSessionStatusHookEffect {
            read_source: response.read_source.clone(),
            status: response.session_memory_status,
        }),
    );
    if let Some(batch) = VmmRecallBatch::from_precheck_response(&response, &config) {
        metadata.insert(
            HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_string(),
            json!(VmmRecallBatchHookEffect {
                read_source: response.read_source.clone(),
                batch,
            }),
        );
    }
    if let Some(notice) = VmmRecallNotice::from_precheck_response(&response, &config) {
        metadata.insert(
            HOOK_METADATA_VMM_RECALL_NOTICE_EFFECT.to_string(),
            json!(notice),
        );
    }
    Ok(HookResult {
        metadata,
        ..HookResult::default()
    })
}

/// Connects to the resolved VMM backend and runs one PreCheck request for the hook payload.
/// 连接已解析的 VMM 后端，并为 hook 载荷执行一次 PreCheck 请求。
async fn request_vmm_precheck(
    runtime: &VmmHookRuntimeHandle,
    invocation: &HookInvocation,
    config: &VmmEffectiveConfig,
) -> Result<vulcan_vmm::PreCheckResponse, HookError> {
    ensure_vmm_ready(config)?;
    let client = resolve_vmm_client(runtime, config).await?;
    let request = precheck_request_from_payload(&invocation.payload.value, config)
        .map_err(|error| HookError::handler(error.to_string()))?;
    eprintln!(
        "[webui-service][vmm-precheck] stage=request session={} project={} operation={}",
        request.session_id, request.host_project_id, request.operation_id,
    );
    client.pre_check(request).await.map_err(|error| {
        eprintln!(
            "[webui-service][vmm-precheck] stage=response session={:?} result=error kind={}",
            invocation_session_id(invocation),
            error.kind,
        );
        HookError::handler(error.to_string())
    })
}

/// Turns a failed PreCheck into a disclosed failure, so the model never reads it as absent memory.
/// 将失败的 PreCheck 转为已披露的失败，使模型绝不把它读作没有记忆。
/// The error stays an error diagnostic; without implicit injection the hook fails as before.
/// 错误仍作为错误诊断保留；未启用隐式注入时 hook 照旧失败。
fn vmm_precheck_failure_result(
    config: &VmmEffectiveConfig,
    error: HookError,
) -> Result<HookResult, HookError> {
    let Some(notice) = VmmRecallNotice::request_failed(config) else {
        return Err(error);
    };
    Ok(HookResult {
        metadata: BTreeMap::from([(
            HOOK_METADATA_VMM_RECALL_NOTICE_EFFECT.to_string(),
            json!(notice),
        )]),
        diagnostics: vec![HookDiagnostic {
            severity: HookDiagnosticSeverity::Error,
            message: error.to_string(),
            hook_id: None,
        }],
        ..HookResult::default()
    })
}

/// Runs the VMM post-action writeback for a completed logical turn.
/// 为已完成逻辑回合执行 VMM 回写。
async fn handle_vmm_post_action(
    runtime: VmmHookRuntimeHandle,
    invocation: HookInvocation,
) -> Result<HookResult, HookError> {
    // These two authoritative gates decide whether a completed hook may write memory.
    // 这两个权威闸门决定完成 Hook 是否允许写入记忆。
    let runtime_ready = invocation_vmm_runtime_ready(&invocation);
    let final_turn = vmm_post_action_payload_is_final(&invocation.payload.value);
    eprintln!(
        "[webui-service][vmm-writeback] stage=postaction_entry session={:?} runtime_ready={} final_turn={}",
        invocation_session_id(&invocation),
        runtime_ready,
        final_turn,
    );
    if !runtime_ready || !final_turn {
        return Ok(HookResult::default());
    }
    let config = runtime
        .resolve_effective_config(&invocation)
        .map_err(HookError::handler)?;
    // The effective configuration may disable writeback even when the service is ready.
    // 即使服务就绪，有效配置仍可能禁止回写。
    let writeback_diagnostic = config.diagnose_writeback_flow(true);
    eprintln!(
        "[webui-service][vmm-writeback] stage=postaction_config session={:?} backend={:?} diagnostic={:?}",
        invocation_session_id(&invocation),
        config.backend_mode,
        writeback_diagnostic,
    );
    if writeback_diagnostic != VmmConfigDiagnostic::Ready {
        return Ok(HookResult::default());
    }
    let mut request =
        post_action_request_from_completed_payload(&invocation.payload.value, &config)
            .map_err(|error| HookError::handler(error.to_string()))?;
    // Resolve exact ownership before connecting; an unavailable remote still has a known configured source.
    // 连接前解析准确归属；远端不可用时依然有确定的已配置来源。
    let source = match config.backend_mode {
        VmmBackendMode::Native => {
            runtime
                .native_client()?
                .memory_source(
                    &request.host_project_id,
                    request.user_id,
                    request.project_id,
                )
                .await
        }
        VmmBackendMode::ExternalGrpc => vulcan_vmm::VmmMemorySource::external(
            &VmmTransportConfig::from_effective(&config),
            request.user_id,
            request.project_id,
        ),
    }
    .map_err(|error| HookError::handler(error.to_string()))?;
    request.expected_source = Some(source.clone());
    // Confirm projection and source resolution without printing memory text.
    // 确认载荷投影与来源解析成功，但不输出记忆正文。
    eprintln!(
        "[webui-service][vmm-writeback] stage=postaction_request session={:?} project={} operation={}",
        invocation_session_id(&invocation),
        request.host_project_id,
        request.operation_id,
    );
    // The generation is admitted atomically with current configuration; later rebinding can revoke every clone.
    // 代次与当前配置原子准入；后续重绑可以撤销全部克隆。
    let permit = runtime
        .admit_writeback(&invocation, &source)
        .map_err(HookError::handler)?;
    // Remove every foreign entry before retrying the FIFO, even if its first valid entry later fails.
    // 重试 FIFO 前移除所有异源条目，即使后续首个有效条目失败也不保留旧来源。
    let (outbox, mut effects) = cancel_foreign_vmm_writebacks(
        runtime
            .outbox_for_invocation(&invocation)
            .map_err(HookError::handler)?,
        &source,
        permit.id(),
    );
    let client = match resolve_vmm_client(&runtime, &config).await {
        Ok(client) => client,
        Err(error) => {
            eprintln!(
                "[webui-service][vmm-writeback] stage=client_resolution session={:?} result=error",
                invocation_session_id(&invocation),
            );
            effects.push(VmmWritebackHookEffect::Enqueue {
                permit_id: permit.id(),
                request,
                error: error.to_string(),
            });
            return Ok(vmm_writeback_effect_result(effects));
        }
    };
    effects.extend(flush_vmm_writeback_outbox(client.as_ref(), outbox, &permit).await);
    if effects
        .iter()
        .any(|effect| matches!(effect, VmmWritebackHookEffect::Failed { .. }))
    {
        effects.push(VmmWritebackHookEffect::Enqueue {
            permit_id: permit.id(),
            request,
            error: String::from(
                "Previous VMM writeback outbox entry is still pending; queued behind it.",
            ),
        });
    } else {
        let delivery = accepted_post_action(client.as_ref(), request.clone(), &permit).await;
        match &delivery {
            Ok(_) => eprintln!(
                "[webui-service][vmm-writeback] stage=postaction_delivery session={:?} result=accepted",
                invocation_session_id(&invocation),
            ),
            Err(error) => eprintln!(
                "[webui-service][vmm-writeback] stage=postaction_delivery session={:?} result=error kind={}",
                invocation_session_id(&invocation),
                error.kind,
            ),
        }
        match delivery {
            Ok(status) => effects.push(VmmWritebackHookEffect::SessionStatus {
                status,
                permit_id: permit.id(),
            }),
            Err(error)
                if matches!(
                    error.kind.as_str(),
                    "memory_source_changed" | "writeback_cancelled"
                ) =>
            {
                effects.push(VmmWritebackHookEffect::Cancelled {
                    entry_ids: Vec::new(),
                    reason: error.to_string(),
                })
            }
            Err(error) => effects.push(VmmWritebackHookEffect::Enqueue {
                permit_id: permit.id(),
                request,
                error: error.to_string(),
            }),
        }
    }
    eprintln!(
        "[webui-service][vmm-writeback] stage=postaction_effects session={:?} count={}",
        invocation_session_id(&invocation),
        effects.len(),
    );
    Ok(vmm_writeback_effect_result(effects))
}

/// Returns whether one completed-turn payload represents the final logical user turn.
/// 返回一条完成回合载荷是否代表最终逻辑用户回合。
fn vmm_post_action_payload_is_final(payload: &Value) -> bool {
    let turn_complete = payload
        .get("turn_complete")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let status_is_final = payload
        .get("completion_status")
        .and_then(Value::as_str)
        .map(|status| status == "completed")
        .unwrap_or(true);
    turn_complete && status_is_final
}

/// Runs the VMM chat-compaction notification after a session summary is accepted.
/// 在会话摘要被接受后执行 VMM 会话压缩通知。
async fn handle_vmm_chat_compact(
    runtime: VmmHookRuntimeHandle,
    invocation: HookInvocation,
) -> Result<HookResult, HookError> {
    if !invocation_vmm_runtime_ready(&invocation) {
        return Ok(HookResult::default());
    }
    let config = runtime
        .resolve_effective_config(&invocation)
        .map_err(HookError::handler)?;
    ensure_vmm_ready(&config)?;
    let request = chat_compact_request_from_payload(&invocation.payload.value, &config)
        .map_err(|error| HookError::handler(error.to_string()))?;
    let client = resolve_vmm_client(&runtime, &config).await?;
    client
        .chat_compact(request)
        .await
        .map_err(|error| HookError::handler(error.to_string()))?;
    Ok(HookResult::default())
}

/// Resolves the backend-specific VMM client for one hook invocation.
/// 为一次 hook 调用解析后端专属的 VMM 客户端。
///
/// # Parameters
/// 参数
/// `runtime` owns the process-local native client registry.
/// `runtime` 持有进程内原生客户端注册表。
/// `config` selects either the native client or one external gRPC connection.
/// `config` 选择原生客户端或一次外部 gRPC 连接。
///
/// # Returns
/// 返回值
/// Returns one shared client implementing the transport-neutral VMM contract.
/// 返回一个实现传输无关 VMM 契约的共享客户端。
async fn resolve_vmm_client(
    runtime: &VmmHookRuntimeHandle,
    config: &VmmEffectiveConfig,
) -> Result<Arc<dyn VmmClient>, HookError> {
    match config.backend_mode {
        VmmBackendMode::Native => runtime.native_client(),
        VmmBackendMode::ExternalGrpc => {
            let transport_config = VmmTransportConfig::from_effective(config);
            TonicVmmClient::connect(transport_config)
                .await
                .map(|client| Arc::new(client) as Arc<dyn VmmClient>)
                .map_err(|error| HookError::handler(error.to_string()))
        }
    }
}

/// Ensures the effective configuration is ready for outbound VMM calls.
/// 确保最终配置已准备好执行 VMM 出站调用。
fn ensure_vmm_ready(config: &VmmEffectiveConfig) -> Result<(), HookError> {
    match config.diagnose_memory_flow(true) {
        VmmConfigDiagnostic::Ready => Ok(()),
        diagnostic => Err(HookError::handler(format!(
            "VMM memory flow is not ready: {diagnostic:?}"
        ))),
    }
}

/// VMM profile synchronization output for one independent profile dispatch.
/// 单次独立画像派发产生的 VMM 画像同步输出。
struct VmmProfileSyncOutput {
    /// Updated session-scoped profile sync state.
    /// 更新后的会话级画像同步状态。
    state: VmmProfileSyncState,
    /// Optional system prompt block for the current request.
    /// 当前请求可选的系统提示词块。
    block: Option<SystemPromptHookBlock>,
    /// Optional hidden-user injection for this request.
    /// 当前请求可选的隐藏用户注入。
    injections: Vec<InjectionInsert>,
}

/// Runs the VMM profile synchronization policy before provider preparation.
/// 在 Provider 准备前运行 VMM 画像同步策略。
async fn maybe_sync_vmm_profile(
    client: &dyn VmmClient,
    runtime: &VmmHookRuntimeHandle,
    invocation: &HookInvocation,
    config: &VmmEffectiveConfig,
) -> Result<Option<VmmProfileSyncOutput>, HookError> {
    let mut state = runtime
        .profile_sync_for_invocation(invocation)
        .map_err(HookError::handler)?;
    let latest_compaction_id = runtime
        .last_compaction_id_for_invocation(invocation)
        .map_err(HookError::handler)?;
    let host_project_id =
        payload_string(&invocation.payload.value, "project_id").unwrap_or_default();
    // Freeze complete read ownership before consulting cached nodes or refresh thresholds.
    // 在查阅缓存节点或刷新阈值之前冻结完整读取归属。
    let read_source = client
        .memory_read_source(
            &host_project_id,
            config.user_id.unwrap_or_default(),
            config.project_id.unwrap_or_default(),
        )
        .await
        .map_err(|error| HookError::handler(error.to_string()))?;
    state
        .select_read_source(&read_source)
        .map_err(|error| HookError::handler(error.to_string()))?;
    let sync_phase = vmm_profile_sync_phase(&state, latest_compaction_id.as_deref());

    match sync_phase {
        SystemPromptHookPhase::FirstSession | SystemPromptHookPhase::AfterContextCompaction => {
            let nodes_by_id =
                fetch_all_profile_nodes(client, config, host_project_id.as_str(), &read_source)
                    .await?;
            let content = render_profile_system_prompt_from_nodes(&nodes_by_id);
            apply_vmm_profile_base_refresh(
                &mut state,
                &content,
                nodes_by_id,
                latest_compaction_id,
                current_unix_millis_text(),
            );
            let block = base_profile_block(&state);
            Ok(Some(VmmProfileSyncOutput {
                state,
                block,
                injections: Vec::new(),
            }))
        }
        SystemPromptHookPhase::SubsequentSession => {
            let block = base_profile_block(&state);
            let mut injections = Vec::new();
            // Initial base refreshes do not count as turn one; this check refreshes only
            // after the configured number of completed turns since that base.
            // 初始化基点刷新不计为第一轮；此处仅在基点之后已完成指定轮数时刷新。
            if !vmm_profile_refresh_threshold_reached(&state, config.profile_refresh_turns) {
                return Ok(Some(VmmProfileSyncOutput {
                    state,
                    block,
                    injections,
                }));
            }

            let previous_nodes_by_id = state.nodes_by_id.clone();
            let latest_nodes_by_id =
                fetch_all_profile_nodes(client, config, host_project_id.as_str(), &read_source)
                    .await?;
            let delta = render_profile_node_diff(&previous_nodes_by_id, &latest_nodes_by_id);
            state.completed_turns_since_sync = 0;
            state.delta_profile_content = normalized_optional_text(&delta);
            state.nodes_by_id = latest_nodes_by_id;
            state.updated_at = Some(current_unix_millis_text());
            if !delta.trim().is_empty() {
                injections.push(vmm_profile_delta_injection(
                    &delta,
                    "refresh",
                    state.updated_at.as_deref().unwrap_or("unknown"),
                ));
            }
            Ok(Some(VmmProfileSyncOutput {
                state,
                block,
                injections,
            }))
        }
    }
}

/// Resolves where a full profile refresh should be placed for the current request.
/// 解析当前请求中的全量画像刷新应放置的位置。
pub fn vmm_profile_placement_phase(
    conversation_message_count: usize,
    latest_compaction_id: Option<&str>,
    covered_compaction_id: Option<&str>,
) -> SystemPromptHookPhase {
    if conversation_message_count <= 1 {
        return SystemPromptHookPhase::FirstSession;
    }
    if latest_compaction_id.is_some() && latest_compaction_id != covered_compaction_id {
        return SystemPromptHookPhase::AfterContextCompaction;
    }
    SystemPromptHookPhase::SubsequentSession
}

/// Resolves the profile sync phase from current session state and compaction boundary.
/// 根据当前会话状态和压缩边界解析画像同步阶段。
pub fn vmm_profile_sync_phase(
    state: &VmmProfileSyncState,
    latest_compaction_id: Option<&str>,
) -> SystemPromptHookPhase {
    if state.base_profile_content.is_none() && state.nodes_by_id.is_empty() {
        return SystemPromptHookPhase::FirstSession;
    }
    if latest_compaction_id != state.covered_compaction_id.as_deref() {
        return SystemPromptHookPhase::AfterContextCompaction;
    }
    SystemPromptHookPhase::SubsequentSession
}

/// Validates recall and lifecycle effects before either can alter the current memory state.
/// 在召回或生命周期效果改变当前记忆状态前校验二者。
/// Takes a correlated outcome and the host's current optional source; returns whether stale VMM effects were removed.
/// 接收关联结果与宿主当前可选来源；返回是否移除了旧 VMM 效果。
pub(crate) fn reconcile_vmm_precheck_publication(
    outcome: &mut crate::agent_runtime::conversation_hooks::ConversationPrecheckOutcome,
    expected: Option<&vulcan_vmm::VmmMemoryReadSource>,
) -> Result<bool, String> {
    if let Some(expected) = expected {
        expected.validate().map_err(|error| error.to_string())?;
    }
    let Ok(report) = outcome.report.as_mut() else {
        return Ok(false);
    };
    // Decode both effects before mutation: malformed metadata is an explicit contract error, not absence.
    // 在变更前解码两个效果：畸形元数据属于明确契约错误，而非缺失。
    let recall =
        crate::agent_runtime::conversation_hooks::vmm_recall_batch_effect_from_report(report)?;
    // Empty PreCheck responses still carry lifecycle authority and must be source-fenced.
    // 空 PreCheck 响应仍带有生命周期权威，必须校验来源。
    let status =
        crate::agent_runtime::conversation_hooks::vmm_session_status_effect_from_report(report)?;
    for source in recall
        .iter()
        .map(|effect| &effect.read_source)
        .chain(status.iter().map(|effect| &effect.read_source))
    {
        source.validate().map_err(|error| error.to_string())?;
    }
    // Treat the two effects as one response; never combine status from one source with another source's content.
    // 把两个效果视为同一响应；绝不组合一个来源的状态与另一个来源的内容。
    let rejected = recall
        .as_ref()
        .is_some_and(|effect| Some(&effect.read_source) != expected)
        || status
            .as_ref()
            .is_some_and(|effect| Some(&effect.read_source) != expected);
    if rejected {
        report
            .metadata
            .remove(HOOK_METADATA_VMM_RECALL_BATCH_EFFECT);
        report
            .metadata
            .remove(HOOK_METADATA_VMM_RECALL_NOTICE_EFFECT);
        report
            .metadata
            .remove(HOOK_METADATA_VMM_SESSION_STATUS_EFFECT);
    }
    Ok(rejected)
}

/// Revalidates resident and incoming profile content against the current publication source.
/// 根据当前发布来源重新校验驻留和传入画像内容。
/// Takes current cache, injections, outcome and optional active source; returns whether a stale result was dropped.
/// 接收当前缓存、注入、结果及可选活动来源；返回是否丢弃了旧结果。
pub(crate) fn reconcile_vmm_profile_publication(
    current: &mut VmmProfileSyncState,
    injections: &mut vulcan_context_injection::ContextInjectionState,
    outcome: &mut crate::agent_runtime::conversation_hooks::ConversationProfileOutcome,
    expected: Option<&vulcan_vmm::VmmMemoryReadSource>,
) -> Result<bool, String> {
    if let Some(expected) = expected {
        expected.validate().map_err(|error| error.to_string())?;
    }
    if expected.is_none() || current.read_source.as_ref() != expected {
        // Unknown provenance cannot be relabeled as the new source even when numeric IDs happen to match.
        // 即使数字 ID 恰好相同，也不能把未知来源重新标记为新来源。
        *current = VmmProfileSyncState {
            read_source: expected.cloned(),
            ..Default::default()
        };
        injections.remove_group(
            Some(InjectionKind::Persistent),
            VMM_PROFILE_DELTA_INJECTION_TAG,
        );
    }
    let Ok(report) = outcome.report.as_mut() else {
        return Ok(false);
    };
    // A malformed or duplicated effect is not equivalent to a skipped hook.
    // 畸形或重复效果不等同于跳过 Hook。
    let effect =
        crate::agent_runtime::conversation_hooks::vmm_profile_sync_effect_from_report(report)?;
    let accepted = expected.is_some()
        && effect
            .as_ref()
            .is_some_and(|effect| effect.state.read_source.as_ref() == expected);
    if accepted {
        return Ok(false);
    }
    let rejected = effect.is_some();
    clear_vmm_profile_report(report);
    Ok(rejected)
}

/// Removes only built-in VMM profile outputs from an existing dispatch report.
/// 从既有派发报告中仅移除内置 VMM 画像输出。
/// Takes a mutable report and preserves all other hooks' metadata, injections and system blocks.
/// 接收可变报告，保留全部其他 Hook 的元数据、注入及系统块。
pub fn clear_vmm_profile_report(report: &mut vulcan_hooks::HookDispatchReport) {
    report
        .metadata
        .remove(HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT);
    report
        .injections
        .retain(|insert| insert.tag != VMM_PROFILE_DELTA_INJECTION_TAG);
    report
        .system_prompt_blocks
        .retain(|block| block.tag != VMM_PROFILE_SYSTEM_PROMPT_TAG);
}

/// Publishes one profile baseline and removes only old-source profile deltas before installing it.
/// 发布一个画像基线，并在安装前仅移除旧来源画像增量。
/// Takes the current baseline, shared injections and incoming state; preserves unrelated injection groups.
/// 接收当前基线、共享注入及传入状态；保留无关的注入分组。
pub(crate) fn apply_vmm_profile_sync_state(
    current: &mut VmmProfileSyncState,
    injections: &mut vulcan_context_injection::ContextInjectionState,
    incoming: VmmProfileSyncState,
) {
    if current.read_source != incoming.read_source {
        // Deltas describe the previous source's baseline and must not be applied on a new owner's baseline.
        // 增量描述此前来源的基线，不得叠加在新归属的基线上。
        injections.remove_group(
            Some(InjectionKind::Persistent),
            VMM_PROFILE_DELTA_INJECTION_TAG,
        );
    }
    *current = incoming;
}

/// Fetches active profile nodes for every supported VMM target.
/// 为所有受支持的 VMM 目标拉取 active 画像节点。
async fn fetch_all_profile_nodes(
    client: &dyn VmmClient,
    config: &VmmEffectiveConfig,
    host_project_id: &str,
    read_source: &vulcan_vmm::VmmMemoryReadSource,
) -> Result<BTreeMap<u64, ProfileNodeEntry>, HookError> {
    let mut nodes = BTreeMap::new();
    for target in [
        ProfileTarget::ProfileTargetTeam,
        ProfileTarget::ProfileTargetSpace,
        ProfileTarget::ProfileTargetProject,
        ProfileTarget::ProfileTargetUser,
    ] {
        let response = client
            .get_profile_nodes(GetProfileNodesRequest {
                expected_read_source: Some(read_source.clone()),
                target,
                user_id: config.user_id.unwrap_or_default(),
                project_id: config.project_id.unwrap_or_default(),
                host_project_id: host_project_id.to_string(),
                limit: effective_profile_node_limit(config),
            })
            .await
            .map_err(|error| HookError::handler(error.to_string()))?;
        for node in response.nodes {
            nodes.insert(node.profile_node_id, node);
        }
    }
    Ok(nodes)
}

/// Renders local profile node changes into a compact diff prompt.
/// 将本地画像节点变化渲染为紧凑差量提示词。
fn render_profile_node_diff(
    previous: &BTreeMap<u64, ProfileNodeEntry>,
    next: &BTreeMap<u64, ProfileNodeEntry>,
) -> String {
    let mut sections = Vec::new();
    let added = next
        .iter()
        .filter(|(id, _)| !previous.contains_key(id))
        .map(|(_, node)| render_profile_node_line("ADDED", node))
        .collect::<Vec<_>>();
    if !added.is_empty() {
        sections.push(format!("# Added Profile Nodes\n{}", added.join("\n")));
    }
    let removed = previous
        .iter()
        .filter(|(id, _)| !next.contains_key(id))
        .map(|(_, node)| render_profile_node_line("REMOVED", node))
        .collect::<Vec<_>>();
    if !removed.is_empty() {
        sections.push(format!("# Removed Profile Nodes\n{}", removed.join("\n")));
    }
    let changed = next
        .iter()
        .filter_map(|(id, node)| {
            previous
                .get(id)
                .filter(|previous_node| *previous_node != node)
                .map(|_| render_profile_node_line("UPDATED", node))
        })
        .collect::<Vec<_>>();
    if !changed.is_empty() {
        sections.push(format!("# Updated Profile Nodes\n{}", changed.join("\n")));
    }
    sections.join("\n\n")
}

/// Renders active profile nodes into the provider-visible system prompt.
/// 将 active 画像节点渲染为 Provider 可见系统提示词。
fn render_profile_system_prompt_from_nodes(nodes: &BTreeMap<u64, ProfileNodeEntry>) -> String {
    let mut sections = Vec::new();
    for target in ordered_profile_targets() {
        if let Some(section) = render_profile_target_section(nodes, target) {
            sections.push(section);
        }
    }
    if sections.is_empty() {
        return String::new();
    }
    let header = [
        "# Profile Bundle",
        "This durable profile bundle summarizes active TEAM, SPACE, PROJECT, and USER constraints or preferences.",
        "Use TEAM, SPACE, and PROJECT entries as environment constraints; use USER entries as target-user preferences when they do not conflict with environment constraints.",
        "Environment constraint priority is Project > Space > Team, even though sections are displayed as TEAM, SPACE, PROJECT, USER for stable reading.",
        "P = Priority: P0 hard constraint, P1 important preference or working agreement, P2 ordinary reference.",
        "L = Lifetime Level: L0 short-lived context, L1 phase-specific context, L2 stable habit, L3 persistent rule or identity feature.",
        "W = Refresh Weight: a higher W means the profile was reconfirmed or renewed more often.",
    ];
    format!("{}\n\n{}", header.join("\n"), sections.join("\n\n"))
}

/// Renders one target section grouped by profile date.
/// 按画像日期分组渲染一个目标段落。
fn render_profile_target_section(
    nodes: &BTreeMap<u64, ProfileNodeEntry>,
    target: ProfileTarget,
) -> Option<String> {
    let mut grouped: BTreeMap<String, Vec<&ProfileNodeEntry>> = BTreeMap::new();
    for node in nodes.values().filter(|node| node.target == target) {
        if node.content.trim().is_empty() {
            continue;
        }
        grouped
            .entry(normalized_profile_date(node))
            .or_default()
            .push(node);
    }
    if grouped.is_empty() {
        return None;
    }
    let mut lines = vec![format!("[{}]", profile_target_label(target))];
    for (date, mut date_nodes) in grouped {
        date_nodes.sort_by_key(|node| node.profile_node_id);
        lines.push(format!("{date}:"));
        for node in date_nodes {
            lines.push(render_profile_node_prompt_line(node));
        }
        lines.push(String::new());
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    Some(lines.join("\n"))
}

/// Converts non-empty text into a persisted optional string.
/// 将非空文本转换为持久化可选字符串。
fn normalized_optional_text(content: &str) -> Option<String> {
    let content = content.trim();
    (!content.is_empty()).then(|| content.to_string())
}

/// Applies a full profile refresh as the new system baseline.
/// 将一次全量画像刷新应用为新的系统基点。
fn apply_vmm_profile_base_refresh(
    state: &mut VmmProfileSyncState,
    content: &str,
    nodes_by_id: BTreeMap<u64, ProfileNodeEntry>,
    covered_compaction_id: Option<String>,
    updated_at: String,
) {
    state.covered_compaction_id = covered_compaction_id;
    state.completed_turns_since_sync = 0;
    state.base_profile_content = normalized_optional_text(content);
    state.delta_profile_content = None;
    state.nodes_by_id = nodes_by_id;
    state.updated_at = Some(updated_at);
}

/// Returns whether the incremental profile refresh threshold has been reached.
/// 返回增量画像刷新阈值是否已经达到。
fn vmm_profile_refresh_threshold_reached(state: &VmmProfileSyncState, threshold: u32) -> bool {
    threshold > 0 && state.completed_turns_since_sync >= threshold
}

/// Returns the effective node fetch limit for profile synchronization.
/// 返回画像同步实际使用的节点拉取上限。
fn effective_profile_node_limit(config: &VmmEffectiveConfig) -> u32 {
    if config.profile_node_limit == 0 {
        VMM_PROFILE_SYNC_NODE_LIMIT
    } else {
        config
            .profile_node_limit
            .min(vulcan_vmm::MAX_PROFILE_NODE_LIMIT)
    }
}

/// Renders one profile node as a stable compact line.
/// 将单个画像节点渲染为稳定紧凑行。
fn render_profile_node_line(prefix: &str, node: &ProfileNodeEntry) -> String {
    format!(
        "- {prefix} VMM_PROFILE_NODE_ID:{} target={} date={} [{}][{}][{}] {}",
        node.profile_node_id,
        profile_target_label(node.target),
        normalized_profile_date(node),
        normalized_profile_marker("P", &node.priority),
        normalized_profile_marker("L", &node.level),
        normalized_refresh_weight(node),
        node.content.trim()
    )
}

/// Renders one active profile node line for the full system prompt.
/// 为完整系统提示词渲染一条 active 画像节点行。
fn render_profile_node_prompt_line(node: &ProfileNodeEntry) -> String {
    format!(
        "[{}][{}][{}] {}",
        normalized_profile_marker("P", &node.priority),
        normalized_profile_marker("L", &node.level),
        normalized_refresh_weight(node),
        node.content.trim()
    )
}

/// Returns VMM profile targets in stable prompt order.
/// 按稳定提示词顺序返回 VMM 画像目标。
fn ordered_profile_targets() -> [ProfileTarget; 4] {
    [
        ProfileTarget::ProfileTargetTeam,
        ProfileTarget::ProfileTargetSpace,
        ProfileTarget::ProfileTargetProject,
        ProfileTarget::ProfileTargetUser,
    ]
}

/// Converts a profile target into its prompt label.
/// 将画像目标转换为提示词标签。
fn profile_target_label(target: ProfileTarget) -> &'static str {
    match target {
        ProfileTarget::ProfileTargetTeam => "TEAM",
        ProfileTarget::ProfileTargetSpace => "SPACE",
        ProfileTarget::ProfileTargetProject => "PROJECT",
        ProfileTarget::ProfileTargetUser => "USER",
        ProfileTarget::ProfileTargetAll => "ALL",
        ProfileTarget::ProfileTargetUnspecified => "UNSPECIFIED",
    }
}

/// Normalizes one profile priority or lifetime marker.
/// 规范化一个画像优先级或生命周期标记。
fn normalized_profile_marker(prefix: &str, value: &str) -> String {
    let value = value.trim().to_ascii_uppercase();
    if value.is_empty() {
        return format!("{prefix}?");
    }
    if value.starts_with(prefix) {
        value
    } else {
        format!("{prefix}{value}")
    }
}

/// Normalizes one profile refresh weight marker.
/// 规范化一个画像刷新权重标记。
fn normalized_refresh_weight(node: &ProfileNodeEntry) -> String {
    format!("W{}", node.refresh_weight)
}

/// Normalizes the profile date shown in prompt sections.
/// 规范化提示词段落中展示的画像日期。
fn normalized_profile_date(node: &ProfileNodeEntry) -> String {
    let date = node.profile_date.trim();
    if date.is_empty() {
        "unknown-date".to_string()
    } else {
        date.to_string()
    }
}

/// Builds one VMM profile system prompt hook block.
/// 构建一个 VMM 画像系统提示词 hook 块。
fn vmm_profile_hook_block(
    content: &str,
    phase: SystemPromptHookPhase,
    kind: &str,
) -> SystemPromptHookBlock {
    SystemPromptHookBlock {
        tag: VMM_PROFILE_SYSTEM_PROMPT_TAG.to_string(),
        description: String::from(
            "VMM profile context. This block carries the current active profile snapshot; when a refresh detects changes, a compact change summary is appended. Treat removed profile nodes as no longer active.",
        ),
        content: content.trim().to_string(),
        source: Some(format!("VMM profile {kind}")),
        phase,
        baseline_provenance: Some(SystemPromptHookBaselineProvenance::new(
            SystemPromptHookBaselineKind::MemoryProfile,
            "hook.vmm-memory-profile",
            VMM_PROFILE_PREPARE_HOOK_ID,
        )),
    }
}

/// Builds the byte-stable system-prompt block for the persisted profile baseline.
/// 为持久化画像基线构建字节稳定的系统提示词块。
///
/// # Parameters
/// 参数
/// `state` owns the exact baseline content and the compaction boundary that created it.
/// `state` 持有精确的基线正文及创建该基线的压缩边界。
///
/// # Returns
/// 返回值
/// The stable baseline block, or `None` before the first successful profile synchronization.
/// 返回稳定的基线块；首次画像同步成功前返回 `None`。
fn base_profile_block(state: &VmmProfileSyncState) -> Option<SystemPromptHookBlock> {
    // baseline_phase changes only when context compaction creates a new full baseline.
    // baseline_phase 仅在上下文压缩创建新全量基线时发生变化。
    let baseline_phase = if state.covered_compaction_id.is_some() {
        SystemPromptHookPhase::AfterContextCompaction
    } else {
        SystemPromptHookPhase::FirstSession
    };
    state
        .base_profile_content
        .as_deref()
        .map(|content| vmm_profile_hook_block(content, baseline_phase, "full"))
}

/// Removes only VMM-owned profile layers from a request-local publication projection.
/// 仅从请求级发布投影移除记忆模块拥有的画像层。
/// `injections` contains generic layers and `blocks` contains system Hook contributions; unrelated owners remain unchanged.
/// `injections` 包含通用注入层，`blocks` 包含系统 Hook 贡献；其他所有者保持不变。
/// Returns no value and does not change profile storage, canonical messages or memory extraction jobs.
/// 无返回值，不修改画像存储、规范消息或记忆提炼作业。
pub fn clear_vmm_profile_injections(
    injections: &mut vulcan_context_injection::ContextInjectionState,
    blocks: &mut Vec<SystemPromptHookBlock>,
) {
    injections.remove_group(
        Some(InjectionKind::Persistent),
        VMM_PROFILE_DELTA_INJECTION_TAG,
    );
    blocks.retain(|block| block.tag != VMM_PROFILE_SYSTEM_PROMPT_TAG);
}

/// Restores the persisted VMM profile baseline into every provider request without duplicating it.
/// 将持久化的 VMM 画像基线恢复到每次 Provider 请求中，同时避免重复。
///
/// # Parameters
/// 参数
/// `blocks` contains all request-scoped system Hook blocks from independent preparation lines.
/// `blocks` 包含来自各独立准备线路的全部请求级系统 Hook 块。
/// `state` owns the authoritative profile baseline reused across every Provider request.
/// `state` 持有在每次 Provider 请求间复用的权威画像基线。
pub(crate) fn stabilize_vmm_profile_system_prompt_blocks(
    blocks: &mut Vec<SystemPromptHookBlock>,
    state: &VmmProfileSyncState,
) {
    // insertion_index preserves the profile line's relative position while removing duplicate or foreign copies.
    // insertion_index 在移除重复或外来副本时保留画像线路的相对位置。
    let insertion_index = blocks
        .iter()
        .position(|block| block.tag == VMM_PROFILE_SYSTEM_PROMPT_TAG);
    blocks.retain(|block| block.tag != VMM_PROFILE_SYSTEM_PROMPT_TAG);
    let Some(stable_block) = base_profile_block(state) else {
        return;
    };
    // VMM is the earliest built-in profile handler, so a missing request copy is restored at the front.
    // VMM 是最早的内置画像处理器，因此请求中缺失副本时会在最前方恢复。
    let insertion_index = insertion_index.unwrap_or(0).min(blocks.len());
    blocks.insert(insertion_index, stable_block);
}

/// Builds one hidden-user delta profile injection for refresh changes.
/// 为刷新变化构建一条隐藏用户画像增量注入。
fn vmm_profile_delta_injection(content: &str, kind: &str, sync_marker: &str) -> InjectionInsert {
    vmm_profile_hidden_user_injection(
        content,
        kind,
        VMM_PROFILE_DELTA_INJECTION_TAG,
        &format!(
            "vmm_profile:delta:{sync_marker}:{}",
            stable_profile_delta_key(content)
        ),
        "VMM profile changes since the previous profile sync. Apply these changes on top of the baseline profile and treat removed facts as no longer active.",
        -49,
    )
}

/// Builds a compact stable key suffix for one profile delta content block.
/// 为一段画像增量内容构建紧凑稳定键后缀。
fn stable_profile_delta_key(content: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in content.trim().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Builds one persistent hidden-user profile injection with stable replacement semantics.
/// 使用稳定替换语义构建一条持久隐藏用户画像注入。
fn vmm_profile_hidden_user_injection(
    content: &str,
    kind: &str,
    tag: &str,
    dedupe_key: &str,
    description: &str,
    priority: i32,
) -> InjectionInsert {
    let mut item = InjectionInsertItem::new(content.trim());
    item.dedupe_key = Some(dedupe_key.to_string());
    item.compaction_policy = Some(InjectionCompactionPolicy::Clear);
    let mut insert = InjectionInsert::single(
        InjectionKind::Persistent,
        tag,
        description,
        String::new(),
        None,
    );
    insert.items = vec![item];
    insert.compaction_policy = InjectionCompactionPolicy::Clear;
    insert.source_display = Some(format!("VMM profile {kind}"));
    insert.priority = priority;
    insert
}

/// Flushes queued VMM writebacks in FIFO order until the first failure.
/// 按 FIFO 顺序冲刷排队的 VMM 写回，直到遇到第一个失败。
async fn flush_vmm_writeback_outbox(
    client: &dyn VmmClient,
    outbox: Vec<VmmWritebackOutboxEntry>,
    permit: &vulcan_vmm::VmmWritebackPermit,
) -> Vec<VmmWritebackHookEffect> {
    let mut flushed = Vec::new();
    let mut effects = Vec::new();
    for entry in outbox {
        match accepted_post_action(client, entry.request.clone(), permit).await {
            Ok(status) => {
                flushed.push(entry.id);
                effects.push(VmmWritebackHookEffect::SessionStatus {
                    status,
                    permit_id: permit.id(),
                });
            }
            Err(error)
                if matches!(
                    error.kind.as_str(),
                    "memory_source_changed" | "writeback_cancelled"
                ) =>
            {
                effects.push(VmmWritebackHookEffect::Cancelled {
                    entry_ids: vec![entry.id],
                    reason: error.to_string(),
                });
            }
            Err(error) => {
                if !flushed.is_empty() {
                    effects.push(VmmWritebackHookEffect::Flushed {
                        entry_ids: std::mem::take(&mut flushed),
                    });
                }
                effects.push(VmmWritebackHookEffect::Failed {
                    entry_id: entry.id,
                    error: error.to_string(),
                });
                return effects;
            }
        }
    }
    if !flushed.is_empty() {
        effects.push(VmmWritebackHookEffect::Flushed { entry_ids: flushed });
    }
    effects
}

/// Partitions pending deliveries by exact ownership without invoking any backend.
/// 按准确归属划分待投递项，不调用任何后端。
/// Takes the current source and queued entries; returns its FIFO and explicit cancellations for all others.
/// 接收当前来源及队列，返回同源 FIFO 及其他条目的明确取消效果。
fn cancel_foreign_vmm_writebacks(
    outbox: Vec<VmmWritebackOutboxEntry>,
    source: &vulcan_vmm::VmmMemorySource,
    permit_id: u64,
) -> (Vec<VmmWritebackOutboxEntry>, Vec<VmmWritebackHookEffect>) {
    // Missing provenance cannot establish permission to replay; never guess from the request envelope.
    // 缺失来源不能建立重放权限；绝不根据请求封套猜测。
    let (retained, cancelled): (Vec<_>, Vec<_>) = outbox.into_iter().partition(|entry| {
        entry.request.expected_source.as_ref() == Some(source) && entry.permit_id == permit_id
    });
    // Cancellation is not a successful delivery and must remain distinguishable in host effects.
    // 取消不是成功投递，必须在宿主效果中保持可区分。
    let effects = if cancelled.is_empty() {
        Vec::new()
    } else {
        vec![VmmWritebackHookEffect::Cancelled {
            entry_ids: cancelled.into_iter().map(|entry| entry.id).collect(),
            reason: "Memory source changed or could not be established; pending writeback cancelled without replay.".to_owned(),
        }]
    };
    (retained, effects)
}

/// Calls VMM PostAction and requires an accepted response.
/// 调用 VMM PostAction 并要求返回 accepted。
async fn accepted_post_action(
    client: &dyn VmmClient,
    request: vulcan_vmm::PostActionRequest,
    permit: &vulcan_vmm::VmmWritebackPermit,
) -> Result<vulcan_vmm::SessionMemoryStatus, vulcan_vmm::VmmClientError> {
    // Retain the typed failure kind so terminal source cancellation cannot become a retryable outbox entry.
    // 保留类型化失败类别，防止终态来源取消被转换成可重试队列项。
    let response = client.post_action_with_permit(request, permit).await?;
    if response.accepted {
        Ok(response.session_memory_status)
    } else {
        Err(vulcan_vmm::VmmClientError::new(
            "postaction_not_accepted",
            format!(
                "VMM PostAction returned accepted=false trace_id={}",
                response.trace_id
            ),
        ))
    }
}

/// Builds one hook result carrying VMM writeback side effects.
/// 构建一条携带 VMM 写回副作用的 hook 结果。
fn vmm_writeback_effect_result(effects: Vec<VmmWritebackHookEffect>) -> HookResult {
    if effects.is_empty() {
        return HookResult::default();
    }
    let mut metadata = BTreeMap::new();
    metadata.insert(
        HOOK_METADATA_VMM_WRITEBACK_EFFECTS.to_string(),
        json!(effects),
    );
    HookResult {
        metadata,
        ..HookResult::default()
    }
}

/// Reads one non-empty string field from a JSON payload.
/// 从 JSON 载荷读取一个非空字符串字段。
fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Reads the session id associated with one hook invocation.
/// 读取一次 hook 调用关联的会话标识。
fn invocation_session_id(invocation: &HookInvocation) -> Option<String> {
    invocation
        .context
        .session_id
        .clone()
        .or_else(|| payload_string(&invocation.payload.value, "session_id"))
}

/// Returns whether the host proved the VMM service was configured and live for this invocation.
/// 返回宿主是否已证明本次调用对应的 VMM 服务完成配置且处于运行就绪状态。
///
/// Missing or non-boolean payload data fails closed because configuration alone cannot prove that
/// the managed service process exists.
/// 载荷缺失或类型错误时按未启动处理，因为仅凭配置不能证明托管服务进程存在。
fn invocation_vmm_runtime_ready(invocation: &HookInvocation) -> bool {
    invocation
        .payload
        .value
        .get("vmm_runtime_ready")
        .and_then(Value::as_bool)
        == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a failed PreCheck is disclosed while injection is on and stays a hook failure otherwise.
    /// 验证失败的 PreCheck 在启用注入时被披露，否则仍为 hook 失败。
    #[test]
    fn failed_precheck_is_disclosed_only_while_injection_is_on() {
        let mut config = VmmEffectiveConfig {
            implicit_memory_turns: 3,
            injection_tag: String::from("VMM_MEMORY"),
            ..Default::default()
        };
        let result = vmm_precheck_failure_result(&config, HookError::handler("unreachable"))
            .expect("failure is disclosed");
        let notice: VmmRecallNotice =
            serde_json::from_value(result.metadata[HOOK_METADATA_VMM_RECALL_NOTICE_EFFECT].clone())
                .expect("typed notice");
        assert_eq!(notice.outcome, vulcan_vmm::VmmRecallNoticeOutcome::Failed);
        assert_eq!(
            result.diagnostics[0].severity,
            HookDiagnosticSeverity::Error
        );
        config.implicit_memory_turns = 0;
        assert!(vmm_precheck_failure_result(&config, HookError::handler("unreachable")).is_err());
    }

    /// Verifies a disabled VMM panel read does not require a client or seed prompt state.
    /// 验证 VMM 关闭时侧栏读取无需客户端，也不会预置提示词状态。
    #[tokio::test]
    async fn disabled_static_profile_panel_read_returns_none() {
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot::default());
        assert!(
            runtime
                .read_static_profile_for_panel(SessionId::nil(), "", "")
                .await
                .expect("disabled read should succeed")
                .is_none()
        );
    }

    /// Verifies request-local suppression removes every VMM profile block and only its persistent deltas.
    /// 验证请求级屏蔽移除全部记忆画像块及其持久增量，不影响其他来源。
    /// Takes no arguments; returns after exact preservation and repeated-call no-op assertions.
    /// 无参数；准确保留及重复调用无变化断言完成后返回。
    #[test]
    fn profile_suppression_preserves_other_injection_owners() {
        // The same profile may be present more than once after independent preparation phases.
        // 独立准备阶段结束后，同一画像可能出现多次。
        let mut blocks = vec![
            vmm_profile_hook_block("old base", SystemPromptHookPhase::FirstSession, "full"),
            SystemPromptHookBlock::new("other_hook", "keep system"),
            vmm_profile_hook_block(
                "old refresh",
                SystemPromptHookPhase::AfterContextCompaction,
                "full",
            ),
        ];
        // These are production-shaped persistent profile deltas alongside another feature's data.
        // 这里包含生产形态的持久画像增量及另一功能的数据。
        let mut injections = vulcan_context_injection::ContextInjectionState::default();
        injections.apply_insert(
            vmm_profile_delta_injection("old delta", "refresh", "1"),
            "1",
        );
        injections.apply_insert(
            vmm_profile_hidden_user_injection(
                "keep generic",
                "test",
                "other_feature",
                "other",
                "other",
                0,
            ),
            "1",
        );
        clear_vmm_profile_injections(&mut injections, &mut blocks);
        assert_eq!(
            blocks,
            vec![SystemPromptHookBlock::new("other_hook", "keep system")]
        );
        assert!(!injections.remove_group(
            Some(InjectionKind::Persistent),
            VMM_PROFILE_DELTA_INJECTION_TAG
        ));
        // A second suppression cannot disturb the remaining feature or its ordering.
        // 第二次屏蔽不能扰动保留功能或其顺序。
        let after = injections.clone();
        clear_vmm_profile_injections(&mut injections, &mut blocks);
        assert_eq!(injections, after);
        assert!(injections.remove_group(Some(InjectionKind::Persistent), "other_feature"));
        assert!(injections.is_empty());
    }

    /// Final publication retains the configuration read lock and never runs the commit after a route change.
    /// 最终发布保留配置读锁，并且路由变化后绝不执行提交。
    #[test]
    fn final_profile_publication_locks_current_source_through_commit() {
        let session = SessionId::from_u128(617);
        let mut layer = VmmConfigLayer {
            enabled: Some(true),
            backend_mode: Some(VmmBackendMode::ExternalGrpc),
            grpc_target: Some("127.0.0.1:50051".to_owned()),
            user_id: Some(1),
            project_id: Some(7),
            ..Default::default()
        };
        let expected = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::external(
                &VmmTransportConfig::from_effective(&VmmEffectiveConfig::resolve(
                    &layer, None, None,
                )),
                1,
                7,
            )
            .expect("source"),
            Vec::new(),
        )
        .expect("read source");
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
            global: layer.clone(),
            ..Default::default()
        });
        assert_eq!(
            runtime
                .with_current_profile_publication(session, "D:/project", &expected, || {
                    assert!(
                        runtime.state.try_write().is_err(),
                        "configuration must remain frozen while committing"
                    );
                    42
                })
                .expect("same source commit"),
            42
        );
        layer.project_id = Some(8);
        runtime
            .replace_global_projection(&layer)
            .expect("change source");
        let mut committed = false;
        assert!(
            runtime
                .with_current_profile_publication(session, "D:/project", &expected, || committed =
                    true)
                .is_err()
        );
        assert!(!committed);
        assert!(
            runtime.state.try_write().is_ok(),
            "guard must release after rejection"
        );
        // Project enablement can override the behavior layer, but cannot reactivate the global service.
        // 项目启用可以覆盖行为层，但不能重新激活全局服务。
        runtime
            .replace_project_projection(
                "D:/project",
                &VmmConfigLayer {
                    enabled: Some(true),
                    ..Default::default()
                },
            )
            .expect("project behavior enabled");
        layer.project_id = Some(7);
        layer.enabled = Some(false);
        runtime
            .replace_global_projection(&layer)
            .expect("stop service");
        assert!(
            runtime
                .resolve_publication_config(session, "D:/project")
                .expect("effective project behavior")
                .enabled
        );
        assert!(
            runtime
                .with_current_profile_publication(session, "D:/project", &expected, || committed =
                    true,)
                .is_err()
        );
        assert!(
            !committed,
            "global stop must reject even a matching project override"
        );
    }

    /// Publication reads current hook configuration after a global change instead of an earlier bridge clone.
    /// 全局变化后发布读取当前 Hook 配置，而非此前的桥接克隆。
    #[test]
    fn publication_configuration_tracks_live_global_and_session_policy() {
        let session = SessionId::from_u128(616);
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
            global: VmmConfigLayer {
                enabled: Some(true),
                backend_mode: Some(VmmBackendMode::Native),
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(
            runtime
                .resolve_publication_config(session, "D:/project")
                .expect("native")
                .backend_mode,
            VmmBackendMode::Native
        );
        runtime
            .replace_global_projection(&VmmConfigLayer {
                enabled: Some(true),
                backend_mode: Some(VmmBackendMode::ExternalGrpc),
                grpc_target: Some("127.0.0.1:50051".to_owned()),
                user_id: Some(1),
                project_id: Some(7),
                ..Default::default()
            })
            .expect("changed global");
        let current = runtime
            .resolve_publication_config(session, "D:/project")
            .expect("current");
        assert_eq!(current.backend_mode, VmmBackendMode::ExternalGrpc);
        assert_eq!(current.project_id, Some(7));
        runtime
            .replace_session_projection(
                session,
                &VmmConfigLayer {
                    enabled: Some(false),
                    ..Default::default()
                },
                &[],
                None,
            )
            .expect("disabled session");
        assert!(
            !runtime
                .resolve_publication_config(session, "D:/project")
                .expect("session override")
                .enabled
        );
    }

    /// Replacing a source clears its old profile deltas without removing other feature injections.
    /// 替换来源会清除其旧画像增量，但不移除其他功能的注入。
    #[test]
    fn profile_source_publication_removes_only_previous_profile_deltas() {
        // Start with a valid same-source baseline and actual production-shaped delta injection.
        // 从有效同来源基线及实际生产形态的增量注入开始。
        let source = vulcan_vmm::VmmMemoryReadSource::new(
            vulcan_vmm::VmmMemorySource::native(7).expect("owner"),
            vec![7],
        )
        .expect("source");
        let mut current = VmmProfileSyncState {
            read_source: Some(source),
            base_profile_content: Some("original".to_owned()),
            ..Default::default()
        };
        let mut injections = vulcan_context_injection::ContextInjectionState::default();
        injections.apply_insert(
            vmm_profile_delta_injection("old delta", "refresh", "1"),
            "1",
        );
        injections.apply_insert(
            vmm_profile_hidden_user_injection(
                "unrelated",
                "test",
                "another_feature",
                "unrelated",
                "unrelated",
                0,
            ),
            "1",
        );
        let original = injections.clone();
        let unchanged = current.clone();
        apply_vmm_profile_sync_state(&mut current, &mut injections, unchanged);
        assert_eq!(injections, original);
        let changed = VmmProfileSyncState {
            read_source: Some(
                vulcan_vmm::VmmMemoryReadSource::new(
                    vulcan_vmm::VmmMemorySource::native(7).expect("same owner"),
                    vec![7, 8],
                )
                .expect("changed read scope"),
            ),
            base_profile_content: Some("new".to_owned()),
            ..Default::default()
        };
        apply_vmm_profile_sync_state(&mut current, &mut injections, changed.clone());
        assert_eq!(current, changed);
        assert!(!injections.remove_group(
            Some(InjectionKind::Persistent),
            VMM_PROFILE_DELTA_INJECTION_TAG
        ));
        assert!(injections.remove_group(Some(InjectionKind::Persistent), "another_feature"));
        assert!(injections.is_empty());
    }

    /// Cancels all foreign or unowned requests while retaining the exact source FIFO without transport calls.
    /// 无传输调用地取消所有异源或无来源请求，并保留准确同源 FIFO。
    #[test]
    fn writeback_source_change_cancels_foreign_entries_including_later_fifo_items() {
        // Equal numeric IDs deliberately cannot make a remote source equivalent to a native one.
        // 特意使用相同数值 ID，远端来源仍不能等同原生来源。
        let source = vulcan_vmm::VmmMemorySource::native(7).expect("native");
        let remote = vulcan_vmm::VmmMemorySource::external(
            &VmmTransportConfig::from_effective(&VmmEffectiveConfig {
                grpc_target: "http://127.0.0.1:50051".to_owned(),
                ..Default::default()
            }),
            1,
            7,
        )
        .expect("remote");
        // Each fixture names its exact provenance, including the explicitly unresolvable entry.
        // 每个夹具明确命名自身来源，包括明确无法解析来源的条目。
        let entries = [
            ("first", Some(source.clone())),
            ("remote", Some(remote)),
            ("unowned", None),
            (
                "other-project",
                Some(vulcan_vmm::VmmMemorySource::native(8).expect("other")),
            ),
            ("last", Some(source.clone())),
        ]
        .into_iter()
        .map(|(id, expected_source)| VmmWritebackOutboxEntry {
            permit_id: 1,
            id: id.to_owned(),
            request: vulcan_vmm::PostActionRequest {
                expected_source,
                ..Default::default()
            },
            attempt_count: 2,
            last_error: Some("offline".to_owned()),
            created_at: "1".to_owned(),
            updated_at: "2".to_owned(),
        })
        .collect();
        // A later foreign item is cancelled even when an earlier retained retry might fail.
        // 即使更早保留项的重试可能失败，位于其后的异源条目仍被取消。
        let (retained, effects) = cancel_foreign_vmm_writebacks(entries, &source, 1);
        assert_eq!(
            retained
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["first", "last"]
        );
        assert!(retained.iter().all(|entry| entry.attempt_count == 2));
        assert_eq!(effects, vec![VmmWritebackHookEffect::Cancelled {
            entry_ids: vec!["remote".to_owned(), "unowned".to_owned(), "other-project".to_owned()],
            reason: "Memory source changed or could not be established; pending writeback cancelled without replay.".to_owned(),
        }]);
        assert!(
            cancel_foreign_vmm_writebacks(retained, &source, 1)
                .1
                .is_empty()
        );
    }
    use vulcan_hooks::{HookContext, HookPayload};

    /// Verifies precheck keeps its short UI protection while effect and preparation hooks receive the long host budget.
    /// 验证 Precheck 保持短前端保护，而副作用与准备 Hook 使用较长宿主预算。
    #[test]
    fn built_in_vmm_hooks_use_stage_specific_host_timeouts() {
        assert_eq!(
            vmm_hook_timeout_ms(VMM_PRECHECK_HOOK_ID),
            VMM_PRECHECK_HOOK_TIMEOUT_MS
        );
        for hook_id in [
            VMM_PROFILE_PREPARE_HOOK_ID,
            VMM_POST_ACTION_HOOK_ID,
            VMM_CHAT_COMPACT_HOOK_ID,
        ] {
            assert_eq!(vmm_hook_timeout_ms(hook_id), VMM_BACKGROUND_HOOK_TIMEOUT_MS);
        }
    }

    /// Verifies missing, false, and malformed readiness facts all keep VMM stopped.
    /// 验证缺失、false 与畸形就绪事实都会让 VMM 保持未启动状态。
    #[test]
    fn vmm_hook_runtime_readiness_fails_closed() {
        let invocation = HookInvocation {
            registration: vmm_hook_registration(
                VMM_PRECHECK_HOOK_ID,
                HOOK_CONVERSATION_TURN_PRECHECK,
                HookMode::Transform,
            ),
            payload: HookPayload::empty_object(),
            context: HookContext::new("readiness-test", HOOK_CONVERSATION_TURN_PRECHECK),
        };
        assert!(!invocation_vmm_runtime_ready(&invocation));

        let mut false_invocation = invocation.clone();
        false_invocation.payload = HookPayload {
            value: json!({ "vmm_runtime_ready": false }),
        };
        assert!(!invocation_vmm_runtime_ready(&false_invocation));

        let mut malformed_invocation = invocation.clone();
        malformed_invocation.payload = HookPayload {
            value: json!({ "vmm_runtime_ready": "yes" }),
        };
        assert!(!invocation_vmm_runtime_ready(&malformed_invocation));

        let mut ready_invocation = invocation;
        ready_invocation.payload = HookPayload {
            value: json!({ "vmm_runtime_ready": true }),
        };
        assert!(invocation_vmm_runtime_ready(&ready_invocation));
    }

    /// Verifies every built-in VMM handler exits before configuration or transport work when stopped.
    /// 验证 VMM 停止时所有内置处理器都会在配置解析或传输工作前退出。
    #[tokio::test]
    async fn stopped_vmm_runtime_skips_all_builtin_handlers() {
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot::default());
        let invocation = HookInvocation {
            registration: vmm_hook_registration(
                VMM_PRECHECK_HOOK_ID,
                HOOK_CONVERSATION_TURN_PRECHECK,
                HookMode::Transform,
            ),
            payload: HookPayload {
                value: json!({
                    "vmm_runtime_ready": false,
                    "turn_complete": true,
                    "completion_status": "completed"
                }),
            },
            context: HookContext::new("stopped-runtime", HOOK_CONVERSATION_TURN_PRECHECK),
        };

        assert_eq!(
            handle_vmm_profile_prepare(runtime.clone(), invocation.clone())
                .await
                .expect("stopped profile handler should skip"),
            HookResult::default()
        );
        assert_eq!(
            handle_vmm_precheck(runtime.clone(), invocation.clone())
                .await
                .expect("stopped precheck handler should skip"),
            HookResult::default()
        );
        assert_eq!(
            handle_vmm_post_action(runtime.clone(), invocation.clone())
                .await
                .expect("stopped post-action handler should skip"),
            HookResult::default()
        );
        assert_eq!(
            handle_vmm_chat_compact(runtime, invocation)
                .await
                .expect("stopped compaction handler should skip"),
            HookResult::default()
        );
    }

    /// Verifies one session projection update preserves every unrelated VMM entry.
    /// 验证单会话投影更新会保留全部无关 VMM 条目。
    #[test]
    fn vmm_session_projection_update_is_exact_and_sparse() {
        // targetSession identifies the only three sparse maps allowed to change.
        // targetSession 标识唯一允许发生变化的三个稀疏映射条目。
        let target_session = SessionId::from_u128(41);
        // otherSession proves the update does not rebuild or discard unrelated state.
        // otherSession 证明本次更新不会重建或丢弃无关状态。
        let other_session = SessionId::from_u128(42);
        let mut initial = VmmHookConfigSnapshot::default();
        initial.global.backend_mode = Some(VmmBackendMode::Native);
        initial.global.enabled = Some(true);
        initial.sessions.insert(
            other_session.to_string(),
            VmmConfigLayer {
                enabled: Some(true),
                ..VmmConfigLayer::default()
            },
        );
        initial.profile_sync.insert(
            target_session.to_string(),
            VmmProfileSyncState {
                updated_at: Some(String::from("profile")),
                ..VmmProfileSyncState::default()
            },
        );
        let runtime = VmmHookRuntimeHandle::new(initial);
        let target_layer = VmmConfigLayer {
            ai_tools_enabled: Some(false),
            ..VmmConfigLayer::default()
        };
        // A mirrored pending entry must belong to a real live admission rather than inventing a permit ID.
        // 镜像待处理项必须属于真实活动准入，不能编造许可 ID。
        let source = vulcan_vmm::VmmMemorySource::native(7).expect("test source");
        let permit = runtime.admit_writeback(&HookInvocation {
            registration: vmm_hook_registration(VMM_POST_ACTION_HOOK_ID, HOOK_CONVERSATION_TURN_COMPLETED, HookMode::Effect),
            context: HookContext::new("projection", HOOK_CONVERSATION_TURN_COMPLETED),
            payload: HookPayload { value: json!({ "session_id": target_session.to_string(), "project_id": "host-project" }) },
        }, &source).expect("admit test delivery");
        let target_outbox = vec![VmmWritebackOutboxEntry {
            permit_id: permit.id(),
            id: String::from("outbox-target"),
            request: vulcan_vmm::PostActionRequest {
                expected_source: Some(source),
                ..Default::default()
            },
            attempt_count: 1,
            last_error: None,
            created_at: String::from("100"),
            updated_at: String::from("100"),
        }];

        runtime
            .replace_session_projection(
                target_session,
                &target_layer,
                &target_outbox,
                Some("compact-target"),
            )
            .expect("exact session projection should update");

        let state = runtime
            .read_state("verify exact session projection")
            .expect("VMM runtime should remain readable");
        assert_eq!(
            state.sessions.get(&target_session.to_string()),
            Some(&target_layer)
        );
        assert!(state.sessions.contains_key(&other_session.to_string()));
        let retained_outbox = state
            .outboxes
            .get(&target_session.to_string())
            .expect("target outbox should be retained");
        assert_eq!(retained_outbox.len(), 1);
        assert_eq!(retained_outbox[0].id, target_outbox[0].id);
        assert_eq!(
            state
                .last_compaction_ids
                .get(&target_session.to_string())
                .and_then(Clone::clone)
                .as_deref(),
            Some("compact-target")
        );
        assert!(state.profile_sync.contains_key(&target_session.to_string()));
    }

    /// Verifies snapshot replacement preserves runtime profile state inside one authoritative lock.
    /// 验证快照替换会在单一权威锁内保留运行时画像状态。
    #[test]
    fn vmm_snapshot_replacement_preserves_runtime_profile_state_atomically() {
        // runtime owns the only VMM configuration and profile snapshot used by the test.
        // runtime 持有测试使用的唯一 VMM 配置与画像快照。
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
            global: VmmConfigLayer {
                enabled: Some(true),
                ..Default::default()
            },
            ..Default::default()
        });
        // sessionId identifies the runtime-produced profile entry that must survive refresh.
        // sessionId 标识必须在刷新后保留的运行时画像项。
        let session_id = SessionId::new_v4();
        // runtimeProfile is the authoritative state produced after the persisted snapshot was built.
        // runtimeProfile 是持久化快照构建后生成的权威运行时状态。
        let runtime_profile = VmmProfileSyncState {
            base_profile_content: Some(String::from("runtime profile")),
            ..VmmProfileSyncState::default()
        };
        runtime
            .store_profile_sync_state(session_id, runtime_profile.clone())
            .expect("runtime profile state should be stored");
        // staleProfile simulates an older workspace projection that must not replace runtime state.
        // staleProfile 模拟不得覆盖运行时状态的旧工作区投影。
        let stale_profile = VmmProfileSyncState {
            base_profile_content: Some(String::from("stale persisted profile")),
            ..VmmProfileSyncState::default()
        };
        // replacement supplies the newest configuration while carrying stale profile projection.
        // replacement 提供最新配置，同时携带旧画像投影。
        let mut replacement = VmmHookConfigSnapshot::default();
        replacement.global.enabled = Some(true);
        replacement
            .profile_sync
            .insert(session_id.to_string(), stale_profile);

        runtime
            .replace_snapshot(replacement)
            .expect("configuration snapshot should replace atomically");

        // state proves configuration replacement and profile preservation committed together.
        // state 证明配置替换与画像保留已共同提交。
        let state = runtime
            .state
            .read()
            .expect("VMM runtime state should remain readable");
        assert_eq!(state.global.enabled, Some(true));
        assert_eq!(
            state.profile_sync.get(&session_id.to_string()),
            Some(&runtime_profile)
        );
    }

    /// Verifies master-mode changes clear orphaned profiles and reject late preparation after switching back.
    /// 验证总模式变化清除孤立画像，并拒绝切回后的迟到准备结果。
    /// Uses the actual admission/store helpers; ordinary tuning must retain its baseline and epoch.
    /// 使用实际准入及存储辅助方法；普通调参必须保留基线和代次。
    #[test]
    fn vmm_global_mode_profile_epoch_rejects_late_results_without_losing_same_mode_cache() {
        // Native activation is explicit so the fixture never depends on transport inference.
        // 显式启用原生模式，夹具不依赖传输推断。
        let mut global = VmmConfigLayer {
            enabled: Some(true),
            backend_mode: Some(VmmBackendMode::Native),
            ..Default::default()
        };
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
            global: global.clone(),
            ..Default::default()
        });
        let session_id = SessionId::new_v4().to_string();
        let invocation = HookInvocation {
            registration: vmm_hook_registration(
                VMM_PROFILE_PREPARE_HOOK_ID,
                HOOK_CONVERSATION_PROFILE_PREPARE,
                HookMode::Transform,
            ),
            payload: HookPayload {
                value: json!({"session_id":session_id, "vmm_runtime_ready":true}),
            },
            context: HookContext::new("profile-mode-test", HOOK_CONVERSATION_PROFILE_PREPARE),
        };
        let (_, admitted_epoch) = runtime
            .resolve_profile_admission(&invocation)
            .expect("admit profile");
        let profile = VmmProfileSyncState {
            base_profile_content: Some("accepted cached profile".to_owned()),
            ..Default::default()
        };
        runtime
            .store_profile_sync_for_session(&session_id, profile.clone(), Some(admitted_epoch))
            .expect("initial cache");
        global.implicit_memory_turns = Some(7);
        runtime
            .replace_global_projection(&global)
            .expect("same-mode tuning");
        assert_eq!(
            runtime
                .resolve_profile_admission(&invocation)
                .expect("same epoch")
                .1,
            admitted_epoch
        );
        assert_eq!(
            runtime
                .read_state("check cache")
                .expect("state")
                .profile_sync
                .get(&session_id),
            Some(&profile)
        );
        global.enabled = Some(false);
        runtime.replace_global_projection(&global).expect("disable");
        assert_eq!(runtime.global_memory_mode().expect("mode"), None);
        assert!(
            runtime
                .read_state("disabled cache")
                .expect("state")
                .profile_sync
                .is_empty()
        );
        global.enabled = Some(true);
        runtime
            .replace_global_projection(&global)
            .expect("enable again");
        assert!(
            runtime
                .store_profile_sync_for_session(&session_id, profile.clone(), Some(admitted_epoch))
                .is_err()
        );
        assert!(
            runtime
                .read_state("late cache rejected")
                .expect("state")
                .profile_sync
                .is_empty()
        );
        let (_, current_epoch) = runtime
            .resolve_profile_admission(&invocation)
            .expect("new admission");
        runtime
            .store_profile_sync_for_session(&session_id, profile.clone(), Some(current_epoch))
            .expect("current cache");
        global.backend_mode = Some(VmmBackendMode::ExternalGrpc);
        runtime
            .replace_snapshot(VmmHookConfigSnapshot {
                global,
                ..Default::default()
            })
            .expect("replace backend");
        assert_eq!(
            runtime.global_memory_mode().expect("external mode"),
            Some(VmmBackendMode::ExternalGrpc)
        );
        assert!(
            runtime
                .store_profile_sync_for_session(&session_id, profile, Some(current_epoch))
                .is_err()
        );
        assert!(
            runtime
                .read_state("backend cache")
                .expect("state")
                .profile_sync
                .is_empty()
        );
    }

    /// Verifies one persisted profile baseline remains byte-stable until a new compaction baseline is created.
    /// 验证同一持久化画像基线在创建新压缩基线前始终保持字节稳定。
    #[test]
    fn persisted_profile_baseline_block_remains_stable_until_compaction() {
        // state owns the first-session baseline reused by ordinary turns and incremental refreshes.
        // state 持有由普通回合及增量刷新复用的首次会话基线。
        let mut state = VmmProfileSyncState {
            base_profile_content: Some(String::from("[USER]\nstable profile")),
            ..VmmProfileSyncState::default()
        };
        // first_block is the exact system contribution emitted when the baseline is created.
        // first_block 是创建基线时发出的精确系统贡献块。
        let first_block = base_profile_block(&state).expect("baseline block should exist");
        state.completed_turns_since_sync = 7;
        state.delta_profile_content = Some(String::from("incremental change"));
        state.updated_at = Some(String::from("later"));
        // subsequent_block proves ordinary turn counters and deltas cannot rewrite the baseline wrapper.
        // subsequent_block 证明普通回合计数及增量不得改写基线包装。
        let subsequent_block =
            base_profile_block(&state).expect("subsequent baseline block should exist");

        assert_eq!(subsequent_block, first_block);
        assert_eq!(first_block.phase, SystemPromptHookPhase::FirstSession);
        assert_eq!(first_block.source.as_deref(), Some("VMM profile full"));

        state.covered_compaction_id = Some(String::from("compaction-1"));
        // compacted_block is the only permitted baseline wrapper transition for this state.
        // compacted_block 是该状态唯一允许发生的基线包装转换。
        let compacted_block =
            base_profile_block(&state).expect("compacted baseline block should exist");
        // repeated_compacted_block proves later requests after the same compaction remain identical.
        // repeated_compacted_block 证明同一压缩后的后续请求仍保持一致。
        let repeated_compacted_block =
            base_profile_block(&state).expect("repeated compacted baseline block should exist");

        assert_ne!(compacted_block, first_block);
        assert_eq!(
            compacted_block.phase,
            SystemPromptHookPhase::AfterContextCompaction
        );
        assert_eq!(compacted_block.source.as_deref(), Some("VMM profile full"));
        assert_eq!(repeated_compacted_block, compacted_block);
    }

    /// Verifies disabling precheck recall never disables the independent profile preparation line.
    /// 验证禁用 precheck 召回绝不会禁用独立画像准备线路。
    #[test]
    fn profile_preparation_does_not_depend_on_precheck_enablement() {
        // config is a memory-ready VMM configuration with recall explicitly disabled.
        // config 是已满足普通记忆链路条件但显式禁用召回的 VMM 配置。
        let mut config = VmmEffectiveConfig {
            enabled: true,
            grpc_target: String::from("http://127.0.0.1:50051"),
            user_id: Some(1),
            project_id: Some(2),
            precheck_enabled: false,
            ..VmmEffectiveConfig::default()
        };

        assert!(should_run_vmm_profile_prepare(&config));
        assert!(!should_run_vmm_precheck(&config));

        config.enabled = false;
        assert!(!should_run_vmm_profile_prepare(&config));
        assert!(!should_run_vmm_precheck(&config));
    }

    /// Verifies poisoned VMM runtime state rejects every authoritative read and write operation.
    /// 验证 VMM 运行时状态锁中毒时会拒绝所有权威读写操作。
    #[test]
    fn vmm_runtime_state_lock_failure_is_explicit() {
        // runtime owns the single state lock deliberately poisoned below.
        // runtime 持有下方刻意中毒的单一状态锁。
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot::default());
        // poisonResult proves the sole authoritative VMM snapshot is unavailable.
        // poisonResult 证明唯一权威 VMM 快照已经不可用。
        let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // guard keeps the state write lock held until the deliberate panic poisons it.
            // guard 将状态写锁保持到刻意 panic，使其进入中毒状态。
            let _guard = runtime
                .state
                .write()
                .expect("VMM runtime state should lock before poisoning");
            panic!("poison VMM runtime state for explicit failure test");
        }));
        assert!(poison_result.is_err());
        // sessionId identifies the rejected profile-state write.
        // sessionId 标识被拒绝的画像状态写入。
        let session_id = SessionId::new_v4();
        // invocation supplies a valid identity so only lock failure can reject runtime reads.
        // invocation 提供合法身份，确保运行时读取只会因锁失败而被拒绝。
        let invocation = HookInvocation {
            registration: vmm_hook_registration(
                "test.vmm.lock_failure",
                HOOK_CONVERSATION_TURN_PRECHECK,
                HookMode::Transform,
            ),
            context: HookContext::new("test-run", HOOK_CONVERSATION_TURN_PRECHECK),
            payload: HookPayload { value: json!({}) },
        };

        assert!(
            runtime
                .replace_snapshot(VmmHookConfigSnapshot::default())
                .expect_err("poisoned state should reject snapshot replacement")
                .contains("VMM hook runtime state is locked")
        );
        assert!(
            runtime
                .seed_profile_sync(BTreeMap::new())
                .expect_err("poisoned state should reject profile seeding")
                .contains("VMM hook runtime state is locked")
        );
        assert!(
            runtime
                .store_profile_sync_state(session_id, VmmProfileSyncState::default())
                .expect_err("poisoned state should reject profile storage")
                .contains("VMM hook runtime state is locked")
        );
        assert!(
            runtime
                .resolve_effective_config(&invocation)
                .expect_err("poisoned state should reject configuration resolution")
                .contains("VMM hook runtime state is locked")
        );
        assert!(
            runtime
                .outbox_for_invocation(&invocation)
                .expect_err("poisoned state should reject outbox lookup")
                .contains("VMM hook runtime state is locked")
        );
        assert!(
            runtime
                .profile_sync_for_invocation(&invocation)
                .expect_err("poisoned state should reject profile lookup")
                .contains("VMM hook runtime state is locked")
        );
        assert!(
            runtime
                .last_compaction_id_for_invocation(&invocation)
                .expect_err("poisoned state should reject compaction lookup")
                .contains("VMM hook runtime state is locked")
        );
    }

    /// Verifies VMM post-action skips unfinished delegated subagent turns.
    /// 验证 VMM post-action 会跳过尚未完成的委派子代理回合。
    #[test]
    fn vmm_post_action_payload_requires_final_turn() {
        assert!(vmm_post_action_payload_is_final(&json!({
            "session_id": "s",
            "user_content": "u",
            "assistant_content": "a"
        })));
        assert!(!vmm_post_action_payload_is_final(&json!({
            "completion_status": "awaiting_subagents",
            "turn_complete": false
        })));
        assert!(!vmm_post_action_payload_is_final(&json!({
            "completion_status": "completed",
            "turn_complete": false
        })));
    }
}
