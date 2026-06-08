//! Built-in VulcanMemoryMesh hook registration for conversation memory integration.
//! 面向会话记忆集成的内置 VulcanMemoryMesh hook 注册逻辑。

use crate::agent_runtime::conversation_hooks::{
    HOOK_CONVERSATION_COMPACTED, HOOK_CONVERSATION_TURN_COMPLETED, HOOK_CONVERSATION_TURN_PRECHECK,
    HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT, HOOK_METADATA_VMM_WRITEBACK_EFFECTS,
    VmmProfileSyncHookEffect, VmmWritebackHookEffect,
};
use crate::model::{SessionId, VmmProfileSyncState, VmmWritebackOutboxEntry};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
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
    VmmConfigDiagnostic, VmmConfigLayer, VmmEffectiveConfig, VmmTransportConfig,
    chat_compact_request_from_payload, injection_from_precheck_response,
    post_action_request_from_completed_payload, precheck_request_from_payload,
};

/// Stable id used by the built-in VMM precheck hook.
/// 内置 VMM 预检查 hook 使用的稳定标识。
pub const VMM_PRECHECK_HOOK_ID: &str = "builtin.vmm.precheck";
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
    /// Latest configuration snapshot visible to hook handlers.
    /// hook 处理器可见的最新配置快照。
    state: Arc<RwLock<VmmHookConfigSnapshot>>,
    /// Runtime-owned profile sync cache keyed by session id string.
    /// 按会话标识索引且由运行时持有的画像同步缓存。
    profile_cache: Arc<RwLock<BTreeMap<String, VmmProfileSyncState>>>,
}

impl VmmHookRuntimeHandle {
    /// Builds a shared VMM hook runtime handle.
    /// 构建共享 VMM hook 运行时句柄。
    pub fn new(snapshot: VmmHookConfigSnapshot) -> Self {
        let profile_cache = snapshot.profile_sync.clone();
        Self {
            state: Arc::new(RwLock::new(snapshot)),
            profile_cache: Arc::new(RwLock::new(profile_cache)),
        }
    }

    /// Replaces the configuration snapshot while preserving runtime-owned profile cache.
    /// 替换配置快照，同时保留运行时持有的画像缓存。
    pub fn replace_snapshot(&self, snapshot: VmmHookConfigSnapshot) {
        let profile_sync = self
            .profile_cache
            .read()
            .map(|cache| cache.clone())
            .unwrap_or_default();
        if let Ok(mut state) = self.state.write() {
            let mut snapshot = snapshot;
            snapshot.profile_sync = profile_sync;
            *state = snapshot;
        }
    }

    /// Seeds the runtime profile cache from persisted state during startup or session load.
    /// 在启动或会话加载时用持久化状态初始化运行时画像缓存。
    pub fn seed_profile_cache(&self, profile_sync: BTreeMap<String, VmmProfileSyncState>) {
        if let Ok(mut cache) = self.profile_cache.write() {
            *cache = profile_sync.clone();
        }
        if let Ok(mut state) = self.state.write() {
            state.profile_sync = profile_sync;
        }
    }

    /// Stores a freshly produced profile sync state in the runtime cache.
    /// 将刚产生的画像同步状态写入运行时缓存。
    fn store_profile_sync_for_session(&self, session_id: &str, profile_sync: VmmProfileSyncState) {
        if let Ok(mut cache) = self.profile_cache.write() {
            if profile_sync.is_empty() {
                cache.remove(session_id);
            } else {
                cache.insert(session_id.to_string(), profile_sync.clone());
            }
        }
        if let Ok(mut state) = self.state.write() {
            if profile_sync.is_empty() {
                state.profile_sync.remove(session_id);
            } else {
                state
                    .profile_sync
                    .insert(session_id.to_string(), profile_sync);
            }
        }
    }

    /// Stores a freshly produced profile sync state in the runtime cache.
    /// 将刚产生的画像同步状态写入运行时缓存。
    pub fn store_profile_sync_state(
        &self,
        session_id: SessionId,
        profile_sync: VmmProfileSyncState,
    ) {
        self.store_profile_sync_for_session(&session_id.to_string(), profile_sync);
    }

    /// Builds read-only cached profile system blocks for a request preview.
    /// 为请求预览构建只读缓存画像系统提示词块。
    pub fn cached_profile_system_prompt_blocks_for_session(
        &self,
        session_id: SessionId,
        config: &VmmEffectiveConfig,
        phase: SystemPromptHookPhase,
    ) -> Vec<SystemPromptHookBlock> {
        if config.diagnose_memory_flow(true) != VmmConfigDiagnostic::Ready {
            return Vec::new();
        }
        self.profile_cache
            .read()
            .ok()
            .and_then(|cache| cache.get(&session_id.to_string()).cloned())
            .and_then(|state| base_profile_block(&state, phase, "cached-preview"))
            .into_iter()
            .collect()
    }

    /// Resolves the effective VMM configuration for one hook invocation payload.
    /// 为一次 hook 调用载荷解析最终 VMM 配置。
    fn resolve_effective_config(&self, invocation: &HookInvocation) -> VmmEffectiveConfig {
        let snapshot = self
            .state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default();
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
        VmmEffectiveConfig::resolve(&snapshot.global, project, session)
    }

    /// Loads the pending writeback outbox for one hook invocation.
    /// 为一次 hook 调用读取待处理写回 outbox。
    fn outbox_for_invocation(&self, invocation: &HookInvocation) -> Vec<VmmWritebackOutboxEntry> {
        let snapshot = self
            .state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default();
        let session_id = invocation
            .context
            .session_id
            .clone()
            .or_else(|| payload_string(&invocation.payload.value, "session_id"));
        session_id
            .as_deref()
            .and_then(|session_id| snapshot.outboxes.get(session_id))
            .cloned()
            .unwrap_or_default()
    }

    /// Loads the profile sync state for one hook invocation.
    /// 为一次 hook 调用读取画像同步状态。
    fn profile_sync_for_invocation(&self, invocation: &HookInvocation) -> VmmProfileSyncState {
        self.profile_cache
            .read()
            .ok()
            .and_then(|cache| {
                invocation_session_id(invocation)
                    .as_deref()
                    .and_then(|session_id| cache.get(session_id))
                    .cloned()
            })
            .unwrap_or_default()
    }

    /// Loads the latest compaction id for one hook invocation.
    /// 为一次 hook 调用读取最近压缩标识。
    fn last_compaction_id_for_invocation(&self, invocation: &HookInvocation) -> Option<String> {
        let snapshot = self
            .state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default();
        invocation_session_id(invocation)
            .as_deref()
            .and_then(|session_id| snapshot.last_compaction_ids.get(session_id))
            .cloned()
            .flatten()
    }
}

/// Registers built-in VMM hooks on the provided conversation hook dispatcher.
/// 在给定会话 hook 调度器上注册内置 VMM hook。
pub fn register_builtin_vmm_hooks(
    dispatcher: &mut HookDispatcher,
    runtime: VmmHookRuntimeHandle,
) -> Result<(), HookError> {
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
    config.precheck_enabled
}

/// Returns the current Unix timestamp in milliseconds as stable text.
/// 返回当前 Unix 毫秒时间戳的稳定文本形式。
fn current_unix_millis_text() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| String::from("0"))
}

/// Runs the VMM precheck request and returns context injections.
/// 执行 VMM 预检查请求并返回上下文注入。
async fn handle_vmm_precheck(
    runtime: VmmHookRuntimeHandle,
    invocation: HookInvocation,
) -> Result<HookResult, HookError> {
    let config = runtime.resolve_effective_config(&invocation);
    if !config.enabled {
        return Ok(HookResult::default());
    }
    ensure_vmm_ready(&config)?;
    let client = connect_vmm_client(&config).await?;
    let profile_sync = maybe_sync_vmm_profile(&client, &runtime, &invocation, &config).await?;
    let injection = if should_run_vmm_precheck(&config) {
        let request = precheck_request_from_payload(&invocation.payload.value, &config)
            .map_err(|error| HookError::handler(error.to_string()))?;
        let response = client
            .pre_check(request)
            .await
            .map_err(|error| HookError::handler(error.to_string()))?;
        injection_from_precheck_response(&response, &config)
    } else {
        None
    };
    let mut metadata = BTreeMap::new();
    let mut system_prompt_blocks = Vec::new();
    let mut injections = injection.into_iter().collect::<Vec<_>>();
    if let Some(profile_sync) = profile_sync {
        if let Some(session_id) = invocation_session_id(&invocation) {
            runtime.store_profile_sync_for_session(&session_id, profile_sync.state.clone());
        }
        metadata.insert(
            HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT.to_string(),
            json!(VmmProfileSyncHookEffect {
                state: profile_sync.state
            }),
        );
        if let Some(block) = profile_sync.block {
            system_prompt_blocks.push(block);
        }
        injections.extend(profile_sync.injections);
    }
    Ok(HookResult {
        injections,
        system_prompt_blocks,
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
    let config = runtime.resolve_effective_config(&invocation);
    ensure_vmm_ready(&config)?;
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
    let mut effects =
        flush_vmm_writeback_outbox(&client, runtime.outbox_for_invocation(&invocation)).await;
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
    let config = runtime.resolve_effective_config(&invocation);
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

/// VMM profile synchronization output for one precheck dispatch.
/// 单次 precheck 调度产生的 VMM 画像同步输出。
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
    let mut state = runtime.profile_sync_for_invocation(invocation);
    let latest_compaction_id = runtime.last_compaction_id_for_invocation(invocation);
    let sync_phase = vmm_profile_sync_phase(&state, latest_compaction_id.as_deref());
    let placement_phase = invocation_system_prompt_hook_phase(invocation).unwrap_or(sync_phase);

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
            let block = base_profile_block(&state, placement_phase, "full");
            Ok(Some(VmmProfileSyncOutput {
                state,
                block,
                injections: Vec::new(),
            }))
        }
        SystemPromptHookPhase::SubsequentSession => {
            let block = base_profile_block(&state, placement_phase, "baseline");
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

/// Reads the host-calculated system prompt hook phase from one invocation payload.
/// 从一次调用载荷中读取宿主计算的系统提示词 hook 阶段。
fn invocation_system_prompt_hook_phase(
    invocation: &HookInvocation,
) -> Option<SystemPromptHookPhase> {
    match payload_string(&invocation.payload.value, "system_prompt_hook_phase")?.as_str() {
        "first_session" => Some(SystemPromptHookPhase::FirstSession),
        "after_context_compaction" => Some(SystemPromptHookPhase::AfterContextCompaction),
        "subsequent_session" => Some(SystemPromptHookPhase::SubsequentSession),
        _ => None,
    }
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

/// Builds the persistent system-prompt block for the cached profile baseline.
/// 为缓存画像基点构建持续系统提示词块。
fn base_profile_block(
    state: &VmmProfileSyncState,
    phase: SystemPromptHookPhase,
    kind: &str,
) -> Option<SystemPromptHookBlock> {
    state
        .base_profile_content
        .as_deref()
        .map(|content| vmm_profile_hook_block(content, phase, kind))
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
