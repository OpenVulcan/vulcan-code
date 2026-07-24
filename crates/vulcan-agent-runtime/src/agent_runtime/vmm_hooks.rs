//! Built-in VulcanMemoryMesh hook registration for conversation memory integration.
//! 面向会话记忆集成的内置 VulcanMemoryMesh hook 注册逻辑。

use crate::agent_runtime::conversation_hooks::{
    HOOK_CONVERSATION_COMPACTED, HOOK_CONVERSATION_PROFILE_PREPARE,
    HOOK_CONVERSATION_TURN_COMPLETED, HOOK_CONVERSATION_TURN_PRECHECK,
    HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT, HOOK_METADATA_VMM_RECALL_BATCH_EFFECT,
    HOOK_METADATA_VMM_WRITEBACK_EFFECTS, VmmProfileSyncHookEffect, VmmRecallBatchHookEffect,
    VmmWritebackHookEffect,
};
use crate::model::{SessionId, VmmProfileSyncState, VmmWritebackOutboxEntry};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use vulcan_context_injection::{
    InjectionCompactionPolicy, InjectionInsert, InjectionInsertItem, InjectionKind,
};
use vulcan_hooks::{
    FnHookHandler, HookCapability, HookDispatcher, HookError, HookFailurePolicy, HookInvocation,
    HookMode, HookRegistration, HookResult, HookScope, SystemPromptHookBlock,
    SystemPromptHookPhase,
};
use vulcan_vmm::{
    GetProfileNodesRequest, ProfileNodeEntry, ProfileTarget, TonicVmmClient, VmmClient,
    VmmConfigDiagnostic, VmmConfigLayer, VmmEffectiveConfig, VmmRecallBatch, VmmTransportConfig,
    chat_compact_request_from_payload, post_action_request_from_completed_payload,
    precheck_request_from_payload,
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
/// Host-side ceiling for built-in VMM hook calls; request timeout remains user-configurable.
/// 内置 VMM hook 调用的宿主侧兜底上限；请求超时仍由用户配置控制。
const VMM_HOOK_TIMEOUT_CEILING_MS: u64 = 120_000;
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
}

impl VmmHookRuntimeHandle {
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
        }
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

    /// Replaces the configuration snapshot while preserving runtime-owned profile state.
    /// 替换配置快照，同时保留运行时持有的画像状态。
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
        snapshot.profile_sync = std::mem::take(&mut state.profile_sync);
        *state = snapshot;
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
    ) -> Result<(), String> {
        // state owns the exact session entry and prevents a second cache from diverging.
        // state 持有精确会话项，避免第二份缓存发生分叉。
        let mut state = self.write_state("store session profile synchronization state")?;
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
        self.store_profile_sync_for_session(&session_id.to_string(), profile_sync)
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
        Ok(VmmEffectiveConfig::resolve(
            &snapshot.global,
            project,
            session,
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
    registration.timeout_ms = Some(VMM_HOOK_TIMEOUT_CEILING_MS);
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
        registration.progress_label = Some(String::from("正在进行记忆召回"));
    }
    registration
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
    let config = runtime
        .resolve_effective_config(&invocation)
        .map_err(HookError::handler)?;
    if !should_run_vmm_profile_prepare(&config) {
        return Ok(HookResult::default());
    }
    ensure_vmm_ready(&config)?;
    let client = connect_vmm_client(&config).await?;
    let Some(profile_sync) =
        maybe_sync_vmm_profile(&client, &runtime, &invocation, &config).await?
    else {
        return Ok(HookResult::default());
    };
    if let Some(session_id) = invocation_session_id(&invocation) {
        runtime
            .store_profile_sync_for_session(&session_id, profile_sync.state.clone())
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
    let config = runtime
        .resolve_effective_config(&invocation)
        .map_err(HookError::handler)?;
    if !should_run_vmm_precheck(&config) {
        return Ok(HookResult::default());
    }
    ensure_vmm_ready(&config)?;
    let client = connect_vmm_client(&config).await?;
    let request = precheck_request_from_payload(&invocation.payload.value, &config)
        .map_err(|error| HookError::handler(error.to_string()))?;
    let response = client
        .pre_check(request)
        .await
        .map_err(|error| HookError::handler(error.to_string()))?;
    let mut metadata = BTreeMap::new();
    if let Some(batch) = VmmRecallBatch::from_precheck_response(&response, &config) {
        metadata.insert(
            HOOK_METADATA_VMM_RECALL_BATCH_EFFECT.to_string(),
            json!(VmmRecallBatchHookEffect { batch }),
        );
    }
    Ok(HookResult {
        metadata,
        ..HookResult::default()
    })
}

/// Runs the VMM post-action writeback for a completed logical turn.
/// 为已完成逻辑回合执行 VMM 回写。
async fn handle_vmm_post_action(
    runtime: VmmHookRuntimeHandle,
    invocation: HookInvocation,
) -> Result<HookResult, HookError> {
    if !vmm_post_action_payload_is_final(&invocation.payload.value) {
        return Ok(HookResult::default());
    }
    let config = runtime
        .resolve_effective_config(&invocation)
        .map_err(HookError::handler)?;
    if config.diagnose_writeback_flow(true) != VmmConfigDiagnostic::Ready {
        return Ok(HookResult::default());
    }
    let request = post_action_request_from_completed_payload(&invocation.payload.value, &config)
        .map_err(|error| HookError::handler(error.to_string()))?;
    let client = match connect_vmm_client(&config).await {
        Ok(client) => client,
        Err(error) => {
            return Ok(vmm_writeback_effect_result(vec![
                VmmWritebackHookEffect::Enqueue {
                    request,
                    error: error.to_string(),
                },
            ]));
        }
    };
    // outbox is loaded from the same authoritative snapshot used for configuration resolution.
    // outbox 从与配置解析相同的权威快照读取。
    let outbox = runtime
        .outbox_for_invocation(&invocation)
        .map_err(HookError::handler)?;
    let mut effects = flush_vmm_writeback_outbox(&client, outbox).await;
    if effects
        .iter()
        .any(|effect| matches!(effect, VmmWritebackHookEffect::Failed { .. }))
    {
        effects.push(VmmWritebackHookEffect::Enqueue {
            request,
            error: String::from(
                "Previous VMM writeback outbox entry is still pending; queued behind it.",
            ),
        });
    } else if let Err(error) = accepted_post_action(&client, request.clone()).await {
        effects.push(VmmWritebackHookEffect::Enqueue {
            request,
            error: error.to_string(),
        });
    }
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
    let config = runtime
        .resolve_effective_config(&invocation)
        .map_err(HookError::handler)?;
    ensure_vmm_ready(&config)?;
    let request = chat_compact_request_from_payload(&invocation.payload.value, &config)
        .map_err(|error| HookError::handler(error.to_string()))?;
    let client = connect_vmm_client(&config).await?;
    client
        .chat_compact(request)
        .await
        .map_err(|error| HookError::handler(error.to_string()))?;
    Ok(HookResult::default())
}

/// Connects a one-shot VMM tonic client for one hook invocation.
/// 为一次 hook 调用连接一个一次性 VMM tonic 客户端。
async fn connect_vmm_client(config: &VmmEffectiveConfig) -> Result<TonicVmmClient, HookError> {
    let transport_config = VmmTransportConfig::from_effective(config);
    TonicVmmClient::connect(transport_config)
        .await
        .map_err(|error| HookError::handler(error.to_string()))
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
    client: &TonicVmmClient,
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
    let sync_phase = vmm_profile_sync_phase(&state, latest_compaction_id.as_deref());

    match sync_phase {
        SystemPromptHookPhase::FirstSession | SystemPromptHookPhase::AfterContextCompaction => {
            let nodes_by_id = fetch_all_profile_nodes(client, config).await?;
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
            let latest_nodes_by_id = fetch_all_profile_nodes(client, config).await?;
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

/// Fetches active profile nodes for every supported VMM target.
/// 为所有受支持的 VMM 目标拉取 active 画像节点。
async fn fetch_all_profile_nodes(
    client: &TonicVmmClient,
    config: &VmmEffectiveConfig,
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
                target,
                user_id: config.user_id.unwrap_or_default(),
                project_id: config.project_id.unwrap_or_default(),
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
    client: &TonicVmmClient,
    outbox: Vec<VmmWritebackOutboxEntry>,
) -> Vec<VmmWritebackHookEffect> {
    let mut flushed = Vec::new();
    let mut effects = Vec::new();
    for entry in outbox {
        match accepted_post_action(client, entry.request.clone()).await {
            Ok(()) => flushed.push(entry.id),
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

/// Calls VMM PostAction and requires an accepted response.
/// 调用 VMM PostAction 并要求返回 accepted。
async fn accepted_post_action(
    client: &TonicVmmClient,
    request: vulcan_vmm::PostActionRequest,
) -> Result<(), HookError> {
    let response = client
        .post_action(request)
        .await
        .map_err(|error| HookError::handler(error.to_string()))?;
    if response.accepted {
        Ok(())
    } else {
        Err(HookError::handler(format!(
            "VMM PostAction returned accepted=false trace_id={}",
            response.trace_id
        )))
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

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_hooks::{HookContext, HookPayload};

    /// Verifies snapshot replacement preserves runtime profile state inside one authoritative lock.
    /// 验证快照替换会在单一权威锁内保留运行时画像状态。
    #[test]
    fn vmm_snapshot_replacement_preserves_runtime_profile_state_atomically() {
        // runtime owns the only VMM configuration and profile snapshot used by the test.
        // runtime 持有测试使用的唯一 VMM 配置与画像快照。
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot::default());
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
